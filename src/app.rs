use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

use anyhow::{Context, Result, bail};
use clap::ColorChoice;
use walkdir::WalkDir;

use crate::cli::{self, Cli};
use crate::detect::detect_format;
use crate::extract::{Budget, extract_one, extract_one_with_callback};

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
        Some(rpassword::prompt_password("压缩包密码：")?)
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
    let format = detect_format(&input)?.context("输入文件未识别为 ZIP、7z 或 RAR")?;
    cli::extraction_started(cli.color, &input, format);
    if cli.jobs == 1 {
        extract_one(&input, staging.path(), format, password.as_deref(), &budget)?;
        expand_nested(staging.path(), 1, &cli, password.as_deref(), &budget)?;
    } else {
        pipeline_first_layer(
            &input,
            staging.path(),
            format,
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

fn pipeline_first_layer(
    input: &Path,
    staging: &Path,
    format: crate::detect::Format,
    cli: &Cli,
    password: Option<&str>,
    budget: &Budget,
) -> Result<()> {
    let workspace = tempfile::Builder::new()
        .prefix(".acgdp-work-")
        .tempdir_in(staging.parent().context("无法确定临时目录父路径")?)?;
    let completed = thread::scope(|scope| -> Result<Vec<CompletedLayer>> {
        let (sender, receiver) = sync_channel(1);
        let worker = scope
            .spawn(|| process_completed_files(receiver, workspace.path(), cli, password, budget));
        let extraction =
            extract_one_with_callback(input, staging, format, password, budget, &mut |file| {
                sender
                    .send(file.to_path_buf())
                    .context("内层解压工作线程已停止")
            });
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
    receiver: Receiver<PathBuf>,
    workspace: &Path,
    cli: &Cli,
    password: Option<&str>,
    budget: &Budget,
) -> Result<Vec<CompletedLayer>> {
    let mut completed = Vec::new();
    for file in receiver {
        let Some(format) = detect_format(&file)? else {
            continue;
        };
        if cli.max_depth <= 1 {
            bail!(
                "达到 --max-depth={}，但仍有嵌套压缩包：{}",
                cli.max_depth,
                file.display()
            );
        }
        let target = tempfile::Builder::new()
            .prefix(".acgdp-layer-")
            .tempdir_in(workspace)?;
        cli::layer_started(cli.color, 2, &file, format);
        extract_one(&file, target.path(), format, password, budget)
            .with_context(|| format!("解压失败：{}", file.display()))?;
        expand_nested(target.path(), 2, cli, password, budget)?;
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
        let Some(format) = detect_format(&file)? else {
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
