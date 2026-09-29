use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::Result;
use clap::builder::styling::{AnsiColor, Style, Styles};
use clap::{ColorChoice, CommandFactory, FromArgMatches, Parser};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::detect::Format;
use crate::extract::Progress;

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
    multi: Option<Arc<MultiProgress>>,
    bar_style: ProgressStyle,
    spinner_style: ProgressStyle,
    started: Instant,
    deepest: AtomicUsize,
}

impl Reporter {
    pub(crate) fn new(choice: ColorChoice, input: &Path) -> Result<Self> {
        let colored = terminal_color(choice, io::stdout().is_terminal());
        let multi = if io::stdout().is_terminal() {
            Some(Arc::new(MultiProgress::with_draw_target(
                ProgressDrawTarget::stdout(),
            )))
        } else {
            None
        };
        let bar_template = if colored {
            "    {bar:30.cyan/blue} {bytes}/{total_bytes} {percent:>3}% · {elapsed_precise}"
        } else {
            "    {bar:30} {bytes}/{total_bytes} {percent:>3}% · {elapsed_precise}"
        };
        let spinner_template = if colored {
            "    {spinner:.cyan} 已写出 {bytes} · {elapsed_precise}"
        } else {
            "    {spinner} 已写出 {bytes} · {elapsed_precise}"
        };
        let reporter = Self {
            choice,
            multi,
            bar_style: ProgressStyle::with_template(bar_template)?,
            spinner_style: ProgressStyle::with_template(spinner_template)?,
            started: Instant::now(),
            deepest: AtomicUsize::new(1),
        };
        let name = input
            .file_name()
            .unwrap_or(input.as_os_str())
            .to_string_lossy();
        reporter.line(format!("acgdp  {name}"));
        Ok(reporter)
    }

    pub(crate) fn layer_started(&self, depth: usize, path: &Path, format: Format) -> LayerProgress {
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
        let bar = self.multi.as_ref().map(|multi| {
            let bar = multi.add(ProgressBar::new_spinner());
            bar.set_style(self.spinner_style.clone());
            bar.enable_steady_tick(std::time::Duration::from_millis(100));
            bar
        });
        LayerProgress {
            bar,
            multi: self.multi.clone(),
            bar_style: self.bar_style.clone(),
            finished: AtomicBool::new(false),
        }
    }

    pub(crate) fn finished(&self, path: &Path, written: u64) {
        if let Some(multi) = &self.multi {
            let _ = multi.clear();
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
        if let Some(multi) = &self.multi {
            multi.suspend(|| println!("{message}"));
        } else {
            println!("{message}");
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        if let Some(multi) = &self.multi {
            let _ = multi.clear();
        }
    }
}

pub(crate) struct LayerProgress {
    bar: Option<ProgressBar>,
    multi: Option<Arc<MultiProgress>>,
    bar_style: ProgressStyle,
    finished: AtomicBool,
}

impl Progress for LayerProgress {
    fn is_visible(&self) -> bool {
        self.bar.is_some()
    }

    fn set_total(&self, bytes: u64) {
        if bytes > 0
            && let Some(bar) = &self.bar
        {
            bar.set_style(self.bar_style.clone());
            bar.set_length(bytes);
        }
    }

    fn advance(&self, bytes: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(bytes);
        }
    }
}

impl LayerProgress {
    pub(crate) fn finish(&self) {
        if !self.finished.swap(true, Ordering::Relaxed)
            && let Some(bar) = &self.bar
        {
            bar.finish_and_clear();
            if let Some(multi) = &self.multi {
                multi.remove(bar);
            }
        }
    }
}

impl Drop for LayerProgress {
    fn drop(&mut self) {
        self.finish();
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
