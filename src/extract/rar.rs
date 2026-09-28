use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{Budget, safe_path};

/*
UnRAR source code may be used in any software to handle
RAR archives without limitations free of charge, but cannot be
used to develop RAR (WinRAR) compatible archiver and to
re-create RAR compression algorithm, which is proprietary.
Distribution of modified UnRAR source code in separate form
or as a part of other software is permitted, provided that
full text of this paragraph, starting from "UnRAR source code"
words, is included in license, or in documentation if license
is not available, and in source code comments of resulting package.
*/
pub(super) fn extract(
    source: &Path,
    dest: &Path,
    password: Option<&str>,
    budget: &mut Budget,
) -> Result<()> {
    let archive = if let Some(pw) = password {
        unrar_ng::Archive::with_password(source, pw)
    } else {
        unrar_ng::Archive::new(source)
    };
    let mut archive = archive.open_for_processing().context("无法打开 RAR")?;
    while let Some(entry) = archive.read_header()? {
        let header = entry.entry();
        let target = dest.join(safe_path(&header.filename)?);
        if header.file_attr & 0o170000 == 0o120000 {
            bail!("RAR 含符号链接：{}", header.filename.display());
        }
        if header.is_directory() {
            fs::create_dir_all(target)?;
            archive = entry.skip()?;
        } else {
            if header.unpacked_size > budget.remaining {
                bail!(
                    "RAR 条目超过累计解压大小限制：{}",
                    header.filename.display()
                );
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            archive = entry
                .extract_to(&target)
                .with_context(|| format!("RAR 条目解压失败：{}", target.display()))?;
            let size = fs::metadata(&target)
                .with_context(|| format!("RAR 报告成功但未找到输出文件：{}", target.display()))?
                .len();
            budget.charge(size)?;
        }
    }
    Ok(())
}
