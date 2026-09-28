mod rar;
mod sevenz;
mod zip;

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::detect::Format;

pub(crate) struct Budget {
    pub(crate) remaining: u64,
    pub(crate) written: u64,
}

impl Budget {
    pub(crate) fn charge(&mut self, n: u64) -> Result<()> {
        if n > self.remaining {
            bail!("超过累计解压大小限制；可用 --max-gib 调整");
        }
        self.remaining -= n;
        self.written += n;
        Ok(())
    }
}

pub(crate) fn extract_one(
    source: &Path,
    dest: &Path,
    format: Format,
    password: Option<&str>,
    budget: &mut Budget,
) -> Result<()> {
    match format {
        Format::Zip => zip::extract(source, dest, password, budget),
        Format::SevenZ(offset) => sevenz::extract(source, dest, offset, password, budget),
        Format::Rar => rar::extract(source, dest, password, budget),
    }
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

fn copy_limited(reader: &mut dyn Read, output: &mut File, budget: &mut Budget) -> Result<()> {
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
