mod rar;
mod sevenz;
mod volume;
mod zip;

pub(crate) use rar::GrowingFile;

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, bail};

use crate::detect::Format;

type ProgressCallback<'a> = dyn FnMut(&Path, &[u8], u64) -> Result<()> + 'a;

pub(crate) struct Budget {
    limit: u64,
    remaining: AtomicU64,
}

impl Budget {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            limit,
            remaining: AtomicU64::new(limit),
        }
    }

    pub(crate) fn remaining(&self) -> u64 {
        self.remaining.load(Ordering::Relaxed)
    }

    pub(crate) fn written(&self) -> u64 {
        self.limit - self.remaining()
    }

    pub(crate) fn charge(&self, n: u64) -> Result<()> {
        self.remaining
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                remaining.checked_sub(n)
            })
            .map_err(|_| anyhow::anyhow!("超过累计解压大小限制；可用 --max-gib 调整"))?;
        Ok(())
    }
}

pub(crate) fn extract_one(
    source: &Path,
    dest: &Path,
    format: Format,
    password: Option<&str>,
    budget: &Budget,
) -> Result<()> {
    extract_one_with_callback(source, dest, format, password, budget, &mut |_| Ok(()))
}

pub(crate) fn extract_one_with_callback(
    source: &Path,
    dest: &Path,
    format: Format,
    password: Option<&str>,
    budget: &Budget,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    match format {
        Format::Zip => zip::extract(source, dest, password, budget, on_file),
        Format::SevenZ(offset) => sevenz::extract(source, dest, offset, password, budget, on_file),
        Format::Rar => rar::extract(source, dest, password, budget, on_file),
    }
}

pub(crate) fn extract_sevenz_with_progress(
    source: &Path,
    dest: &Path,
    offset: u64,
    password: Option<&str>,
    budget: &Budget,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
    on_progress: &mut ProgressCallback<'_>,
) -> Result<()> {
    sevenz::extract_with_progress(source, dest, offset, password, budget, on_file, on_progress)
}

pub(crate) fn extract_growing_rar(
    source: &Path,
    dest: &Path,
    password: Option<&str>,
    budget: &Budget,
    growing: &GrowingFile,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    rar::extract_growing(source, dest, password, budget, growing, on_file)
}

fn safe_path(name: &Path) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for part in name.components() {
        match part {
            Component::Normal(s) => result.push(s),
            Component::CurDir => {}
            _ => bail!("压缩包含不安全路径：{}", name.display()),
        }
    }
    if result.as_os_str().is_empty() {
        bail!("压缩包含空路径");
    }
    Ok(result)
}

fn copy_limited(reader: &mut dyn Read, output: &mut File, budget: &Budget) -> Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        budget.charge(n as u64)?;
        output.write_all(&buffer[..n])?;
    }
    Ok(())
}

fn copy_limited_with_progress(
    reader: &mut dyn Read,
    output: &mut File,
    budget: &Budget,
    target: &Path,
    on_progress: &mut ProgressCallback<'_>,
) -> Result<()> {
    let mut buffer = [0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        budget.charge(n as u64)?;
        output.write_all(&buffer[..n])?;
        written += n as u64;
        on_progress(target, &buffer[..n], written)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Budget;

    #[test]
    fn concurrent_writes_cannot_exceed_global_limit() {
        let budget = Budget::new(1_000);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..200 {
                        let _ = budget.charge(1);
                    }
                });
            }
        });
        assert_eq!(budget.written(), 1_000);
        assert_eq!(budget.remaining(), 0);
        assert!(budget.charge(1).is_err());
    }
}
