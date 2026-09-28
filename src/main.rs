use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use walkdir::WalkDir;

const SEVEN_Z: &[u8] = b"7z\xBC\xAF\x27\x1C";
const RAR4: &[u8] = b"Rar!\x1A\x07\x00";
const RAR5: &[u8] = b"Rar!\x1A\x07\x01\x00";

#[derive(Parser)]
#[command(version, about = "递归解开 ZIP、7z、RAR，以及伪装成图片的压缩包")]
struct Cli {
    /// 输入文件，扩展名可以是 .jpg 或 .png
    input: PathBuf,
    /// 输出目录，默认「输入文件名.extracted」
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// 各层共用的密码
    #[arg(short, long, conflicts_with = "ask_password")]
    password: Option<String>,
    /// 从终端安全读取密码
    #[arg(long)]
    ask_password: bool,
    /// 保留解压出的中间压缩包
    #[arg(long)]
    keep_intermediates: bool,
    /// 最多解开的层数
    #[arg(long, default_value_t = 32)]
    max_depth: usize,
    /// 所有层累计最多写出的 GiB 数
    #[arg(long, default_value_t = 20)]
    max_gib: u64,
}

#[derive(Clone, Copy, Debug)]
enum Format {
    Zip,
    SevenZ(u64),
    Rar,
}

struct Budget {
    remaining: u64,
    written: u64,
}
impl Budget {
    fn charge(&mut self, n: u64) -> Result<()> {
        if n > self.remaining {
            bail!("超过累计解压大小限制；可用 --max-gib 调整");
        }
        self.remaining -= n;
        self.written += n;
        Ok(())
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
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
    let output = cli.output.clone().unwrap_or_else(|| {
        input.with_file_name(format!(
            "{}.extracted",
            input.file_name().unwrap().to_string_lossy()
        ))
    });
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
    println!("解压 {} ({format:?})", input.display());
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
    println!(
        "完成：{}（累计解压 {} 字节）",
        output.display(),
        budget.written
    );
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
        println!("第 {} 层：{} ({format:?})", depth + 1, file.display());
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

fn extract_one(
    source: &Path,
    dest: &Path,
    format: Format,
    password: Option<&str>,
    budget: &mut Budget,
) -> Result<()> {
    match format {
        Format::Zip => extract_zip(source, dest, password, budget),
        Format::SevenZ(offset) => extract_sevenz(source, dest, offset, password, budget),
        Format::Rar => extract_rar(source, dest, password, budget),
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

fn extract_zip(
    source: &Path,
    dest: &Path,
    password: Option<&str>,
    budget: &mut Budget,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(source)?)?;
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
        copy_limited(&mut entry, &mut File::create(target)?, budget)?;
    }
    Ok(())
}

fn extract_sevenz(
    source: &Path,
    dest: &Path,
    offset: u64,
    password: Option<&str>,
    budget: &mut Budget,
) -> Result<()> {
    let reader = OffsetReader::new(File::open(source)?, offset)?;
    let mut failure = None;
    let extract = |entry: &sevenz_rust2::ArchiveEntry, reader: &mut dyn Read, _: &PathBuf| {
        let result = (|| -> Result<()> {
            let target = dest.join(safe_path(Path::new(entry.name()))?);
            if entry.is_directory() {
                fs::create_dir_all(target)?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                copy_limited(reader, &mut File::create(target)?, budget)?;
            }
            Ok(())
        })();
        if let Err(err) = result {
            failure = Some(err);
            return Err(sevenz_rust2::Error::from(io::Error::other("无法解压条目")));
        }
        Ok(true)
    };
    let pw = sevenz_rust2::Password::from(password.unwrap_or_default());
    let result = sevenz_rust2::decompress_with_extract_fn_and_password(reader, dest, pw, extract);
    if let Some(err) = failure {
        return Err(err);
    }
    result.context("7z 解压失败；若已加密，请提供密码")?;
    Ok(())
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
fn extract_rar(
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

fn detect_format(path: &Path) -> Result<Option<Format>> {
    let mut choices = Vec::new();
    if let Ok(zip) = zip::ZipArchive::new(File::open(path)?) {
        choices.push((zip.offset(), Format::Zip));
    }
    choices.extend(scan_signatures(path)?);
    choices.sort_by_key(|(offset, _)| *offset);
    Ok(choices.first().map(|(_, format)| *format))
}

fn scan_signatures(path: &Path) -> Result<Vec<(u64, Format)>> {
    let mut file = File::open(path)?;
    let mut found = Vec::new();
    let mut buffer = [0u8; 64 * 1024 + 8];
    let (mut carry, mut base) = (0usize, 0u64);
    loop {
        let n = file.read(&mut buffer[carry..64 * 1024 + carry])?;
        if n == 0 {
            break;
        }
        let len = carry + n;
        for i in 0..len {
            let tail = &buffer[i..len];
            let format = if tail.starts_with(SEVEN_Z) {
                Some(Format::SevenZ(base + i as u64))
            } else if tail.starts_with(RAR4) || tail.starts_with(RAR5) {
                Some(Format::Rar)
            } else {
                None
            };
            if let Some(format) = format {
                found.push((base + i as u64, format));
                if found.len() >= 16 {
                    return Ok(found);
                }
            }
        }
        carry = len.min(7);
        buffer.copy_within(len - carry..len, 0);
        base += (len - carry) as u64;
    }
    Ok(found)
}

struct OffsetReader<R> {
    inner: R,
    start: u64,
    len: u64,
}
impl<R: Read + Seek> OffsetReader<R> {
    fn new(mut inner: R, start: u64) -> Result<Self> {
        let end = inner.seek(SeekFrom::End(0))?;
        if start >= end {
            bail!("7z 起点超出文件范围");
        }
        inner.seek(SeekFrom::Start(start))?;
        Ok(Self {
            inner,
            start,
            len: end - start,
        })
    }
}
impl<R: Read> Read for OffsetReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}
impl<R: Seek> Seek for OffsetReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let current = self.inner.stream_position()?;
        let relative = match pos {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.len) + i128::from(n),
            SeekFrom::Current(n) => i128::from(current - self.start) + i128::from(n),
        };
        if !(0..=i128::from(u64::MAX - self.start)).contains(&relative) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "无效的偏移量"));
        }
        let absolute = self.start + relative as u64;
        self.inner.seek(SeekFrom::Start(absolute))?;
        Ok(relative as u64)
    }
}
