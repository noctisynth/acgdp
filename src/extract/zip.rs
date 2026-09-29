use std::fs::{self, File};
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::{Budget, FileCallback, Progress, copy_limited, safe_path};

pub(super) fn extract(
    source: &Path,
    dest: &Path,
    password: Option<&str>,
    budget: &Budget,
    progress: &dyn Progress,
    on_file: &mut FileCallback<'_>,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(source)?)?;
    if progress.is_visible() {
        let mut total = Some(0u64);
        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i)?;
            if !entry.is_dir() {
                total = total.and_then(|bytes| bytes.checked_add(entry.size()));
            }
        }
        if let Some(bytes) = total {
            progress.set_total(bytes);
        }
    }
    for i in 0..archive.len() {
        let raw = archive.by_index_raw(i)?;
        let path = safe_path(Path::new(raw.name()))?;
        let (is_dir, encrypted, mode) = (raw.is_dir(), raw.encrypted(), raw.unix_mode());
        drop(raw);
        if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
            bail!("ZIP 含符号链接：{}", path.display());
        }
        let target = dest.join(path);
        if is_dir {
            fs::create_dir_all(target)?;
            continue;
        }
        let mut entry = if encrypted {
            let pw = password.context("ZIP 需要密码；请使用 -p 或 --ask-password")?;
            archive.by_index_decrypt(i, pw.as_bytes())?
        } else {
            archive.by_index(i)?
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if target.exists() {
            bail!("ZIP 含重复或冲突路径：{}", target.display());
        }
        copy_limited(&mut entry, &mut File::create(&target)?, budget, progress)?;
        on_file(&target, None)?;
    }
    Ok(())
}
