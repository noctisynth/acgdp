use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{Budget, copy_limited, safe_path, volume::VolumeReader};

pub(super) fn extract(
    source: &Path,
    dest: &Path,
    offset: u64,
    password: Option<&str>,
    budget: &Budget,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let reader = OffsetReader::new(VolumeReader::open(source)?, offset)?;
    let mut failure = None;
    let extract = |entry: &sevenz_rust2::ArchiveEntry, reader: &mut dyn Read, _: &PathBuf| {
        let result = (|| -> Result<()> {
            let target = dest.join(safe_path(Path::new(entry.name()))?);
            if entry.is_directory() {
                fs::create_dir_all(target)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                if target.exists() {
                    bail!("7z 含重复或冲突路径：{}", target.display());
                }
                copy_limited(reader, &mut File::create(&target)?, budget)?;
                on_file(&target)?;
            }
            Ok(())
        })();
        if let Err(err) = result {
            failure = Some(err);
            return Err(sevenz_rust2::Error::from(io::Error::other("无法解压条目")));
        }
        Ok(true)
    };
    let pw = sevenz_rust2::Password::from(password.unwrap_or_default());
    let result = sevenz_rust2::decompress_with_extract_fn_and_password(reader, dest, pw, extract);
    if let Some(err) = failure {
        return Err(err);
    }
    result.context("7z 解压失败；若已加密，请提供密码")?;
    Ok(())
}

struct OffsetReader<R> {
    inner: R,
    start: u64,
    len: u64,
}

impl<R: Read + Seek> OffsetReader<R> {
    fn new(mut inner: R, start: u64) -> Result<Self> {
        let end = inner.seek(SeekFrom::End(0))?;
        if start >= end {
            bail!("7z 起点超出文件范围");
        }
        inner.seek(SeekFrom::Start(start))?;
        Ok(Self {
            inner,
            start,
            len: end - start,
        })
    }
}

impl<R: Read> Read for OffsetReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for OffsetReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let current = self.inner.stream_position()?;
        let relative = match pos {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.len) + i128::from(n),
            SeekFrom::Current(n) => i128::from(current - self.start) + i128::from(n),
        };
        if !(0..=i128::from(u64::MAX - self.start)).contains(&relative) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "无效的偏移量"));
        }
        let absolute = self.start + relative as u64;
        self.inner.seek(SeekFrom::Start(absolute))?;
        Ok(relative as u64)
    }
}
