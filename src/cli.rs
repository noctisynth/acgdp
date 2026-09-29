use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::builder::styling::{AnsiColor, Style, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser};

use crate::detect::Format;

#[derive(Parser)]
#[command(version, about = "递归解开 ZIP、7z、RAR，以及伪装成图片的压缩包")]
pub(crate) struct Cli {
    /// 输入文件，扩展名可以是 .jpg 或 .png
    pub(crate) input: PathBuf,
    /// 输出目录，默认「输入文件名.extracted」
    #[arg(short, long)]
    pub(crate) output: Option<PathBuf>,
    /// 各层共用的密码
    #[arg(short, long, conflicts_with = "ask_password")]
    pub(crate) password: Option<String>,
    /// 从终端安全读取密码
    #[arg(long)]
    pub(crate) ask_password: bool,
    /// 保留解压出的中间压缩包
    #[arg(long)]
    pub(crate) keep_intermediates: bool,
    /// 最多解开的层数
    #[arg(long, default_value_t = 32)]
    pub(crate) max_depth: usize,
    /// 所有层累计最多写出的 GiB 数
    #[arg(long, default_value_t = 20)]
    pub(crate) max_gib: u64,
    /// 解压模式：1 为串行，2 为各嵌套层建立流水线阶段
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=2))]
    pub(crate) jobs: u8,
    /// 终端颜色
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub(crate) color: ColorChoice,
}

fn help_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Cyan.on_default().bold())
        .usage(AnsiColor::Cyan.on_default().bold())
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::Yellow.on_default())
        .error(AnsiColor::Red.on_default().bold())
}

pub(crate) fn requested_color() -> ColorChoice {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let Some(arg) = arg.to_str() else { continue };
        if arg == "--" {
            break;
        }
        let value = if arg == "--color" {
            args.next().and_then(|value| value.into_string().ok())
        } else {
            arg.strip_prefix("--color=").map(str::to_owned)
        };
        if let Some(value) = value {
            return value.parse().unwrap_or(ColorChoice::Auto);
        }
    }
    ColorChoice::Auto
}

pub(crate) fn parse_or_help(color: ColorChoice) -> Result<Option<Cli>> {
    let mut command = Cli::command().color(color).styles(help_styles());
    if std::env::args_os().nth(1).is_none() {
        command.print_help()?;
        println!();
        return Ok(None);
    }
    let matches = command.get_matches();
    Ok(Some(Cli::from_arg_matches(&matches)?))
}

pub(crate) fn terminal_color(choice: ColorChoice, terminal: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => terminal && std::env::var_os("NO_COLOR").is_none(),
    }
}

fn output_style(choice: ColorChoice, color: AnsiColor) -> Style {
    if terminal_color(choice, io::stdout().is_terminal()) {
        color.on_default().bold()
    } else {
        Style::new()
    }
}

pub(crate) fn extraction_started(choice: ColorChoice, path: &Path, format: Format) {
    let style = output_style(choice, AnsiColor::Cyan);
    println!(
        "{style}解压{} {} ({format:?})",
        style.render_reset(),
        path.display()
    );
}

pub(crate) fn layer_started(choice: ColorChoice, depth: usize, path: &Path, format: Format) {
    let style = output_style(choice, AnsiColor::Magenta);
    println!(
        "{style}第 {} 层：{} {} ({format:?})",
        depth,
        style.render_reset(),
        path.display()
    );
}

pub(crate) fn extraction_finished(choice: ColorChoice, path: &Path, written: u64) {
    let style = output_style(choice, AnsiColor::Green);
    println!(
        "{style}完成：{}{}（累计解压 {} 字节）",
        style.render_reset(),
        path.display(),
        written
    );
}

pub(crate) fn print_error(choice: ColorChoice, error: &anyhow::Error) {
    let style = if terminal_color(choice, io::stderr().is_terminal()) {
        AnsiColor::Red.on_default().bold()
    } else {
        Style::new()
    };
    eprintln!("{style}错误：{}{error:#}", style.render_reset());
}
