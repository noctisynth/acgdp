use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail};

pub(super) enum VolumeReader {
    Single(File),
    Split(SplitReader),
}

impl VolumeReader {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let extension = path.extension().and_then(|value| value.to_str());
        if extension != Some("001") {
            return Ok(Self::Single(File::open(path)?));
        }

        let mut files = Vec::new();
        let mut ends = Vec::new();
        let mut total = 0u64;
        for index in 1..=999 {
            let part = path.with_extension(format!("{index:03}"));
            if index > 1 && !part.is_file() {
                break;
            }
            let file = File::open(&part)
                .with_context(|| format!("无法打开第 {index} 个分卷：{}", part.display()))?;
            let length = file.metadata()?.len();
            if length == 0 {
                bail!("分卷为空：{}", part.display());
            }
            total = total
                .checked_add(length)
                .context("分卷总大小超出支持范围")?;
            files.push(file);
            ends.push(total);
        }
        Ok(Self::Split(SplitReader {
            files,
            ends,
            position: 0,
        }))
    }
}

impl Read for VolumeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Single(file) => file.read(buf),
            Self::Split(reader) => reader.read(buf),
        }
    }
}

impl Seek for VolumeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Single(file) => file.seek(pos),
            Self::Split(reader) => reader.seek(pos),
        }
    }
}

pub(super) struct SplitReader {
    files: Vec<File>,
    ends: Vec<u64>,
    position: u64,
}

impl Read for SplitReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let index = self.ends.partition_point(|end| *end <= self.position);
        let Some(&end) = self.ends.get(index) else {
            return Ok(0);
        };
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        self.files[index].seek(SeekFrom::Start(self.position - start))?;
        let count = (end - self.position).min(buf.len() as u64) as usize;
        let read = self.files[index].read(&mut buf[..count])?;
        if read == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "分卷提前结束"));
        }
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for SplitReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let end = self.ends.last().copied().unwrap_or(0);
        let next = match pos {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(end) + i128::from(value),
        };
        if !(0..=i128::from(u64::MAX)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "无效的分卷偏移量",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}
