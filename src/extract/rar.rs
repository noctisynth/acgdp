use std::fs;
use std::path::Path;
use std::sync::{Condvar, Mutex};

use anyhow::{Context, Result, bail};

use super::{Budget, Progress, safe_path};

pub(crate) struct GrowingFile {
    state: Mutex<GrowingState>,
    changed: Condvar,
}

#[derive(Default)]
struct GrowingState {
    size: u64,
    done: bool,
    failed: bool,
}

impl GrowingFile {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(GrowingState::default()),
            changed: Condvar::new(),
        }
    }

    pub(crate) fn update(&self, size: u64) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("流状态已损坏"))?;
        state.size = size;
        self.changed.notify_all();
        Ok(())
    }

    pub(crate) fn finish(&self, failed: bool) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("流状态已损坏"))?;
        state.done = true;
        state.failed = failed;
        self.changed.notify_all();
        Ok(())
    }

    fn wait_after(&self, previous: u64) -> Result<(u64, bool)> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("流状态已损坏"))?;
        while state.size <= previous && !state.done {
            state = self
                .changed
                .wait(state)
                .map_err(|_| anyhow::anyhow!("流状态已损坏"))?;
        }
        if state.failed {
            bail!("外层文件写入失败");
        }
        Ok((state.size, state.done))
    }
}

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
    budget: &Budget,
    progress: &dyn Progress,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
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
            if header.unpacked_size > budget.remaining() {
                bail!(
                    "RAR 条目超过累计解压大小限制：{}",
                    header.filename.display()
                );
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            if target.exists() {
                bail!("RAR 含重复或冲突路径：{}", target.display());
            }
            archive = entry
                .extract_to(&target)
                .with_context(|| format!("RAR 条目解压失败：{}", target.display()))?;
            let size = fs::metadata(&target)
                .with_context(|| format!("RAR 报告成功但未找到输出文件：{}", target.display()))?
                .len();
            budget.charge(size)?;
            progress.advance(size);
            on_file(&target)?;
        }
    }
    Ok(())
}

pub(super) fn extract_growing(
    source: &Path,
    dest: &Path,
    password: Option<&str>,
    budget: &Budget,
    growing: &GrowingFile,
    progress: &dyn Progress,
    on_file: &mut dyn FnMut(&Path) -> Result<()>,
) -> Result<()> {
    let mut completed = 0usize;
    let mut checked_size = 0u64;
    loop {
        let (available, done) = growing.wait_after(checked_size)?;
        checked_size = available.saturating_add(256 * 1024 * 1024 - 1);
        let archive = if let Some(pw) = password {
            unrar_ng::Archive::with_password(source, pw)
        } else {
            unrar_ng::Archive::new(source)
        };
        let mut archive = match archive.open_for_processing() {
            Ok(archive) => archive,
            Err(error) if !done => {
                let _ = error;
                continue;
            }
            Err(error) => return Err(error).context("无法打开增长中的 RAR"),
        };
        if archive.is_solid() {
            if done {
                return extract(source, dest, password, budget, progress, on_file);
            }
            continue;
        }
        let mut index = 0usize;
        let mut at_end = false;
        loop {
            let entry = match archive.read_header() {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    at_end = true;
                    break;
                }
                Err(error) if !done => {
                    let _ = error;
                    break;
                }
                Err(error) => return Err(error).context("RAR 文件头读取失败"),
            };
            if index < completed {
                archive = match entry.skip() {
                    Ok(next) => next,
                    Err(error) if !done => {
                        let _ = error;
                        break;
                    }
                    Err(error) => return Err(error).context("RAR 已处理条目跳过失败"),
                };
                index += 1;
                continue;
            }
            let header = entry.entry();
            let target = dest.join(safe_path(&header.filename)?);
            if header.file_attr & 0o170000 == 0o120000 {
                bail!("RAR 含符号链接：{}", header.filename.display());
            }
            if header.is_directory() {
                fs::create_dir_all(target)?;
                archive = match entry.skip() {
                    Ok(next) => next,
                    Err(error) if !done => {
                        let _ = error;
                        break;
                    }
                    Err(error) => return Err(error).context("RAR 目录条目跳过失败"),
                };
            } else {
                if header.unpacked_size > budget.remaining() {
                    bail!(
                        "RAR 条目超过累计解压大小限制：{}",
                        header.filename.display()
                    );
                }
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                if target.exists() {
                    bail!("RAR 含重复或冲突路径：{}", target.display());
                }
                archive = match entry.extract_to(&target) {
                    Ok(next) => next,
                    Err(error) if !done => {
                        if target.exists() {
                            fs::remove_file(&target)?;
                        }
                        let _ = error;
                        break;
                    }
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("RAR 条目解压失败：{}", target.display()));
                    }
                };
                let size = fs::metadata(&target)?.len();
                budget.charge(size)?;
                progress.advance(size);
                on_file(&target)?;
            }
            index += 1;
            completed = index;
        }
        if done {
            if at_end && index >= completed {
                return Ok(());
            }
            bail!("RAR 未能完整解压");
        }
    }
}
