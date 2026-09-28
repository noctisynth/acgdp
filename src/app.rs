use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::ColorChoice;
use walkdir::WalkDir;

use crate::cli::{self, Cli};
use crate::detect::detect_format;
use crate::extract::{Budget, extract_one};

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
    let mut budget = Budget {
        remaining: cli
            .max_gib
            .checked_mul(1024 * 1024 * 1024)
            .context("--max-gib 数值过大")?,
        written: 0,
    };
    let format = detect_format(&input)?.context("输入文件未识别为 ZIP、7z 或 RAR")?;
    cli::extraction_started(cli.color, &input, format);
    extract_one(
        &input,
        staging.path(),
        format,
        password.as_deref(),
        &mut budget,
    )?;
    expand_nested(staging.path(), 1, &cli, password.as_deref(), &mut budget)?;
    fs::rename(staging.path(), &output)
        .with_context(|| format!("无法保存到 {}", output.display()))?;
    cli::extraction_finished(cli.color, &output, budget.written);
    Ok(())
}

fn expand_nested(
    root: &Path,
    depth: usize,
    cli: &Cli,
    password: Option<&str>,
    budget: &mut Budget,
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
