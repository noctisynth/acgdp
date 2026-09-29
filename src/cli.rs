use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;
use clap::builder::styling::{AnsiColor, Style, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

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

pub(crate) struct Reporter {
    choice: ColorChoice,
    spinner: Option<ProgressBar>,
    started: Instant,
    deepest: AtomicUsize,
}

impl Reporter {
    pub(crate) fn new(choice: ColorChoice, input: &Path, format: Format) -> Result<Self> {
        let spinner = if io::stdout().is_terminal() {
            let bar = ProgressBar::new_spinner();
            bar.set_draw_target(ProgressDrawTarget::stdout());
            let template = if terminal_color(choice, true) {
                "{spinner:.cyan}  正在解压 · {elapsed_precise}"
            } else {
                "{spinner}  正在解压 · {elapsed_precise}"
            };
            bar.set_style(ProgressStyle::with_template(template)?);
            Some(bar)
        } else {
            None
        };
        let reporter = Self {
            choice,
            spinner,
            started: Instant::now(),
            deepest: AtomicUsize::new(1),
        };
        let name = input
            .file_name()
            .unwrap_or(input.as_os_str())
            .to_string_lossy();
        reporter.line(format!("acgdp  {name}"));
        reporter.layer_started(1, input, format);
        if let Some(bar) = &reporter.spinner {
            bar.enable_steady_tick(std::time::Duration::from_millis(100));
        }
        Ok(reporter)
    }

    pub(crate) fn layer_started(&self, depth: usize, path: &Path, format: Format) {
        self.deepest.fetch_max(depth, Ordering::Relaxed);
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let label = match format {
            Format::Zip => "ZIP",
            Format::SevenZ(_) => "7z",
            Format::Rar => "RAR",
        };
        let number = output_style(self.choice, AnsiColor::Cyan);
        self.line(format!(
            "  {number}{depth}{}  {label:<3}  {name}",
            number.render_reset()
        ));
    }

    pub(crate) fn finished(&self, path: &Path, written: u64) {
        if let Some(bar) = &self.spinner {
            bar.finish_and_clear();
        }
        let style = output_style(self.choice, AnsiColor::Green);
        println!(
            "{style}✓ 完成{} · {} 层 · 累计写出 {} · {:.1} 秒",
            style.render_reset(),
            self.deepest.load(Ordering::Relaxed),
            human_bytes(written),
            self.started.elapsed().as_secs_f64()
        );
        println!("  输出 {}", display_path(path));
    }

    fn line(&self, message: String) {
        if let Some(bar) = &self.spinner {
            bar.suspend(|| println!("{message}"));
        } else {
            println!("{message}");
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        if let Some(bar) = &self.spinner {
            bar.finish_and_clear();
        }
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn display_path(path: &Path) -> String {
    let shown = path.to_string_lossy();
    #[cfg(windows)]
    {
        if let Some(rest) = shown.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = shown.strip_prefix(r"\\?\") {
            return rest.to_owned();
        }
    }
    shown.into_owned()
}

pub(crate) fn print_error(choice: ColorChoice, error: &anyhow::Error) {
    let style = if terminal_color(choice, io::stderr().is_terminal()) {
        AnsiColor::Red.on_default().bold()
    } else {
        Style::new()
    };
    eprintln!("{style}错误：{}{error:#}", style.render_reset());
}
