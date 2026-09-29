use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

use anyhow::{Context, Result, bail};
use clap::ColorChoice;
use inquire::{Password, ui::RenderConfig};
use walkdir::WalkDir;

use crate::cli::{self, Cli};
use crate::detect::detect_format;
use crate::extract::{
    Budget, GrowingFile, extract_growing_rar, extract_one, extract_one_with_callback,
    extract_sevenz_with_progress,
};

pub(crate) fn run(color: ColorChoice) -> Result<()> {
    let Some(cli) = cli::parse_or_help(color)? else {
        return Ok(());
    };
    if cli.max_depth == 0 {
        bail!("--max-depth 必须大于 0");
    }
    let input = cli.input.canonicalize().context("无法打开输入文件")?;
    if !input.is_file() {
        bail!("输入必须是文件：{}", input.display());
    }
    let password = if cli.ask_password {
        let render = if cli::terminal_color(cli.color, std::io::stdout().is_terminal()) {
            RenderConfig::default_colored()
        } else {
            RenderConfig::empty()
        };
        Some(
            Password::new("压缩包密码")
                .without_confirmation()
                .with_render_config(render)
                .prompt()
                .context("无法从终端读取密码")?,
        )
    } else {
        cli.password.clone()
    };
    let output = if let Some(path) = &cli.output {
        path.clone()
    } else {
        let name = input.file_name().context("无法确定输入文件名")?;
        input.with_file_name(format!("{}.extracted", name.to_string_lossy()))
    };
    if output.exists() {
        bail!("输出位置已存在，拒绝覆盖：{}", output.display());
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".acgdp-")
        .tempdir_in(parent)?;
    let budget = Budget::new(
        cli.max_gib
            .checked_mul(1024 * 1024 * 1024)
            .context("--max-gib 数值过大")?,
    );
    let format =
        detect_format(&input, password.as_deref())?.context("输入文件未识别为 ZIP、7z 或 RAR")?;
    cli::extraction_started(cli.color, &input, format);
    if cli.jobs == 1 {
        extract_one(&input, staging.path(), format, password.as_deref(), &budget)?;
        expand_nested(staging.path(), 1, &cli, password.as_deref(), &budget)?;
    } else {
        pipeline_layer(
            LayerSource {
                input: &input,
                format,
                progress: None,
            },
            staging.path(),
            1,
            &cli,
            password.as_deref(),
            &budget,
        )?;
    }
    fs::rename(staging.path(), &output)
        .with_context(|| format!("无法保存到 {}", output.display()))?;
    cli::extraction_finished(cli.color, &output, budget.written());
    Ok(())
}

struct CompletedLayer {
    source: PathBuf,
    output: tempfile::TempDir,
}

struct LayerSource<'a> {
    input: &'a Path,
    format: crate::detect::Format,
    progress: Option<&'a GrowingFile>,
}

enum WorkItem {
    Completed(PathBuf),
    GrowingRar(PathBuf, Arc<GrowingFile>),
}

#[derive(Default)]
struct RarProbe {
    carry: Vec<u8>,
    candidates: VecDeque<u64>,
}

impl RarProbe {
    fn observe(&mut self, chunk: &[u8], written: u64) -> bool {
        let start = written - chunk.len() as u64;
        let mut boundary = self.carry.clone();
        boundary.extend_from_slice(&chunk[..chunk.len().min(7)]);
        let boundary_start = start.saturating_sub(self.carry.len() as u64);
        for index in memchr::memchr_iter(b'R', &boundary[..self.carry.len()]) {
            if is_rar_signature(&boundary[index..]) {
                self.add_candidate(boundary_start + index as u64);
            }
        }
        for index in memchr::memchr_iter(b'R', chunk) {
            if is_rar_signature(&chunk[index..]) {
                self.add_candidate(start + index as u64);
            }
        }
        if chunk.len() >= 7 {
            self.carry.clear();
            self.carry.extend_from_slice(&chunk[chunk.len() - 7..]);
        } else {
            self.carry.extend_from_slice(chunk);
            let excess = self.carry.len().saturating_sub(7);
            self.carry.drain(..excess);
        }
        self.candidates
            .front()
            .is_some_and(|offset| offset.saturating_add(2 * 1024 * 1024) <= written)
    }

    fn add_candidate(&mut self, offset: u64) {
        if self.candidates.len() < 128 && self.candidates.back().is_none_or(|last| *last != offset)
        {
            self.candidates.push_back(offset);
        }
    }

    fn discard_checked(&mut self, written: u64) {
        while self
            .candidates
            .front()
            .is_some_and(|offset| offset.saturating_add(2 * 1024 * 1024) <= written)
        {
            self.candidates.pop_front();
        }
    }
}

fn is_rar_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"Rar!\x1A\x07\x00") || bytes.starts_with(b"Rar!\x1A\x07\x01\x00")
}

fn pipeline_layer(
    source: LayerSource<'_>,
    staging: &Path,
    depth: usize,
    cli: &Cli,
    password: Option<&str>,
    budget: &Budget,
) -> Result<()> {
    let LayerSource {
        input,
        format,
        progress: source_progress,
    } = source;
    let workspace = tempfile::Builder::new()
        .prefix(".acgdp-work-")
        .tempdir_in(staging.parent().context("无法确定临时目录父路径")?)?;
    let completed = thread::scope(|scope| -> Result<Vec<CompletedLayer>> {
        let (sender, receiver) = sync_channel(1);
        let worker = scope.spawn(|| {
            process_completed_files(receiver, workspace.path(), depth, cli, password, budget)
        });
        let growing = RefCell::new(HashMap::<PathBuf, Arc<GrowingFile>>::new());
        let probes = RefCell::new(HashMap::<PathBuf, RarProbe>::new());
        let mut on_file = |file: &Path| {
            probes.borrow_mut().remove(file);
            if let Some(state) = growing.borrow_mut().remove(file) {
                state.finish(false)
            } else {
                sender
                    .send(WorkItem::Completed(file.to_path_buf()))
                    .context("内层解压工作线程已停止")
            }
        };
        let extraction = if let Some(progress) = source_progress {
            extract_growing_rar(input, staging, password, budget, progress, &mut on_file)
        } else if let crate::detect::Format::SevenZ(offset) = format {
            extract_sevenz_with_progress(
                input,
                staging,
                offset,
                password,
                budget,
                &mut on_file,
                &mut |file, chunk, written| {
                    if let Some(state) = growing.borrow().get(file).cloned() {
                        return state.update(written);
                    }
                    let ready = probes
                        .borrow_mut()
                        .entry(file.to_path_buf())
                        .or_default()
                        .observe(chunk, written);
                    if !ready {
                        return Ok(());
                    }
                    if matches!(
                        detect_format(file, password)?,
                        Some(crate::detect::Format::Rar)
                    ) {
                        let state = Arc::new(GrowingFile::new());
                        state.update(written)?;
                        sender
                            .send(WorkItem::GrowingRar(file.to_path_buf(), Arc::clone(&state)))
                            .context("内层解压工作线程已停止")?;
                        growing.borrow_mut().insert(file.to_path_buf(), state);
                    } else {
                        if let Some(probe) = probes.borrow_mut().get_mut(file) {
                            probe.discard_checked(written);
                        }
                    }
                    Ok(())
                },
            )
        } else {
            extract_one_with_callback(input, staging, format, password, budget, &mut on_file)
        };
        for state in growing.borrow().values() {
            state.finish(extraction.is_err())?;
        }
        drop(sender);
        let processed = worker
            .join()
            .map_err(|_| anyhow::anyhow!("内层解压工作线程异常退出"))?;
        let completed = processed?;
        extraction?;
        Ok(completed)
    })?;
    for layer in completed {
        let parent = layer.source.parent().context("无法确定内层压缩包的位置")?;
        merge_contents(layer.output.path(), parent)?;
        if !cli.keep_intermediates {
            fs::remove_file(&layer.source)?;
        }
    }
    Ok(())
}

fn process_completed_files(
    receiver: Receiver<WorkItem>,
    workspace: &Path,
    depth: usize,
    cli: &Cli,
    password: Option<&str>,
    budget: &Budget,
) -> Result<Vec<CompletedLayer>> {
    let mut completed = Vec::new();
    for item in receiver {
        let (file, format, progress) = match item {
            WorkItem::Completed(file) => {
                let Some(format) = detect_format(&file, password)? else {
                    continue;
                };
                (file, format, None)
            }
            WorkItem::GrowingRar(file, progress) => {
                (file, crate::detect::Format::Rar, Some(progress))
            }
        };
        if depth >= cli.max_depth {
            bail!(
                "达到 --max-depth={}，但仍有嵌套压缩包：{}",
                cli.max_depth,
                file.display()
            );
        }
        let target = tempfile::Builder::new()
            .prefix(".acgdp-layer-")
            .tempdir_in(workspace)?;
        cli::layer_started(cli.color, depth + 1, &file, format);
        pipeline_layer(
            LayerSource {
                input: &file,
                format,
                progress: progress.as_deref(),
            },
            target.path(),
            depth + 1,
            cli,
            password,
            budget,
        )
        .with_context(|| format!("流水线解压失败：{}", file.display()))?;
        completed.push(CompletedLayer {
            source: file,
            output: target,
        });
    }
    Ok(completed)
}

fn expand_nested(
    root: &Path,
    depth: usize,
    cli: &Cli,
    password: Option<&str>,
    budget: &Budget,
) -> Result<()> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            bail!("压缩包包含符号链接：{}", entry.path().display());
        }
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    for file in files {
        let Some(format) = detect_format(&file, password)? else {
            continue;
        };
        if depth >= cli.max_depth {
            bail!(
                "达到 --max-depth={}，但仍有嵌套压缩包：{}",
                cli.max_depth,
                file.display()
            );
        }
        let parent = file.parent().context("无法确定内层压缩包的位置")?;
        let target = tempfile::Builder::new()
            .prefix(".acgdp-layer-")
            .tempdir_in(parent)?;
        cli::layer_started(cli.color, depth + 1, &file, format);
        extract_one(&file, target.path(), format, password, budget)
            .with_context(|| format!("解压失败：{}", file.display()))?;
        expand_nested(target.path(), depth + 1, cli, password, budget)?;
        merge_contents(target.path(), parent)?;
        if !cli.keep_intermediates {
            fs::remove_file(&file)?;
        }
    }
    Ok(())
}

fn merge_contents(from: &Path, into: &Path) -> Result<()> {
    for item in fs::read_dir(from)? {
        let item = item?;
        let source = item.path();
        let target = into.join(item.file_name());
        if target.exists() {
            if source.is_dir() && target.is_dir() {
                merge_contents(&source, &target)?;
            } else {
                bail!("内层文件与现有文件重名，拒绝覆盖：{}", target.display());
            }
        } else {
            fs::rename(&source, &target)?;
        }
    }
    Ok(())
}
