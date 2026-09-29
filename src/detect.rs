use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;

const SEVEN_Z: &[u8] = b"7z\xBC\xAF\x27\x1C";
const RAR4: &[u8] = b"Rar!\x1A\x07\x00";
const RAR5: &[u8] = b"Rar!\x1A\x07\x01\x00";

#[derive(Clone, Copy, Debug)]
pub(crate) enum Format {
    Zip,
    SevenZ(u64),
    Rar,
}

struct ScanContext<'a> {
    path: &'a Path,
    password: Option<&'a str>,
    end: u64,
}

pub(crate) fn detect_format(path: &Path, password: Option<&str>) -> Result<Option<Format>> {
    detect_reader(File::open(path)?, path, password, || valid_zip(path))
}

pub(crate) fn detect_format_bytes(
    bytes: &[u8],
    path: &Path,
    password: Option<&str>,
) -> Result<Option<Format>> {
    detect_reader(Cursor::new(bytes), path, password, || {
        Ok(zip::ZipArchive::new(Cursor::new(bytes))
            .ok()
            .map(|archive| archive.offset()))
    })
}

fn detect_reader<R, F>(
    mut file: R,
    path: &Path,
    password: Option<&str>,
    mut zip_offset: F,
) -> Result<Option<Format>>
where
    R: Read + Seek,
    F: FnMut() -> Result<Option<u64>>,
{
    let mut first = [0u8; 8];
    let n = file.read(&mut first)?;
    if first[..n].starts_with(SEVEN_Z) && valid_7z_header(&mut file, 0)? {
        return Ok(Some(Format::SevenZ(0)));
    }
    if (first[..n].starts_with(RAR4) || first[..n].starts_with(RAR5))
        && valid_rar_header(&mut file, path, 0, password)?
    {
        return Ok(Some(Format::Rar));
    }
    if (first[..n].starts_with(b"PK\x03\x04") || first[..n].starts_with(b"PK\x05\x06"))
        && zip_offset()? == Some(0)
    {
        return Ok(Some(Format::Zip));
    }
    let size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(0))?;
    let prechecked_zip = if size > 1024 * 1024 {
        Some(zip_offset()?)
    } else {
        None
    };
    scan_signatures(&mut file, path, password, &mut zip_offset, prechecked_zip)
}

fn valid_zip(path: &Path) -> Result<Option<u64>> {
    Ok(zip::ZipArchive::new(File::open(path)?)
        .ok()
        .map(|archive| archive.offset()))
}

fn scan_signatures<R, F>(
    file: &mut R,
    path: &Path,
    password: Option<&str>,
    valid_zip: &mut F,
    prechecked_zip: Option<Option<u64>>,
) -> Result<Option<Format>>
where
    R: Read + Seek,
    F: FnMut() -> Result<Option<u64>>,
{
    let mut zip_offset = prechecked_zip;
    let mut buffer = [0u8; 64 * 1024 + 8];
    let (mut carry, mut base) = (0usize, 0u64);
    loop {
        let n = file.read(&mut buffer[carry..64 * 1024 + carry])?;
        if n == 0 {
            break;
        }
        let len = carry + n;
        let end = base + len as u64;
        let context = ScanContext {
            path,
            password,
            end,
        };
        if prechecked_zip.is_some() {
            for i in memchr::memchr2_iter(SEVEN_Z[0], RAR4[0], &buffer[..len]) {
                if let Some(format) = check_candidate(
                    file,
                    &context,
                    valid_zip,
                    &mut zip_offset,
                    &buffer[i..len],
                    base + i as u64,
                )? {
                    return Ok(Some(format));
                }
            }
        } else {
            for i in memchr::memchr3_iter(SEVEN_Z[0], RAR4[0], b'P', &buffer[..len]) {
                if let Some(format) = check_candidate(
                    file,
                    &context,
                    valid_zip,
                    &mut zip_offset,
                    &buffer[i..len],
                    base + i as u64,
                )? {
                    return Ok(Some(format));
                }
            }
        }
        carry = len.min(7);
        buffer.copy_within(len - carry..len, 0);
        base += (len - carry) as u64;
    }
    Ok(zip_offset.flatten().map(|_| Format::Zip))
}

fn check_candidate<R, F>(
    file: &mut R,
    context: &ScanContext<'_>,
    valid_zip: &mut F,
    zip_offset: &mut Option<Option<u64>>,
    tail: &[u8],
    offset: u64,
) -> Result<Option<Format>>
where
    R: Read + Seek,
    F: FnMut() -> Result<Option<u64>>,
{
    if zip_offset.is_none() && is_zip_signature(tail) {
        *zip_offset = Some(valid_zip()?);
    }
    if zip_offset.is_some_and(|candidate| candidate.is_some_and(|zip| zip <= offset)) {
        return Ok(Some(Format::Zip));
    }
    let format = if tail.starts_with(SEVEN_Z) {
        Some(Format::SevenZ(offset))
    } else if tail.starts_with(RAR4) || tail.starts_with(RAR5) {
        Some(Format::Rar)
    } else {
        None
    };
    let valid = match format {
        Some(Format::SevenZ(_)) => valid_7z_header(file, offset)?,
        Some(Format::Rar) => valid_rar_header(file, context.path, offset, context.password)?,
        _ => false,
    };
    if format.is_some() {
        file.seek(SeekFrom::Start(context.end))?;
    }
    Ok(valid.then_some(format).flatten())
}

fn is_zip_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x06\x06")
        || bytes.starts_with(b"PK\x06\x07")
}

fn valid_7z_header<R: Read + Seek>(file: &mut R, offset: u64) -> Result<bool> {
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0u8; 32];
    if file.read_exact(&mut header).is_err() || !header.starts_with(SEVEN_Z) {
        return Ok(false);
    }
    let expected = u32::from_le_bytes(header[8..12].try_into()?);
    Ok(crc32fast::hash(&header[12..32]) == expected)
}

fn valid_rar_header<R: Read + Seek>(
    file: &mut R,
    path: &Path,
    offset: u64,
    password: Option<&str>,
) -> Result<bool> {
    file.seek(SeekFrom::Start(offset))?;
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() {
        return Ok(false);
    }
    if signature.starts_with(RAR5) {
        return valid_rar5_block(file);
    }
    if !signature.starts_with(RAR4) {
        return Ok(false);
    }
    let archive = if let Some(value) = password {
        unrar_ng::Archive::with_password(path, value)
    } else {
        unrar_ng::Archive::new(path)
    };
    Ok(archive.open_for_processing().is_ok())
}

fn valid_rar5_block<R: Read>(file: &mut R) -> Result<bool> {
    let mut checksum = [0u8; 4];
    if file.read_exact(&mut checksum).is_err() {
        return Ok(false);
    }
    let mut encoded_size = Vec::with_capacity(3);
    let mut size = 0usize;
    for shift in [0, 7, 14] {
        let mut byte = [0u8; 1];
        if file.read_exact(&mut byte).is_err() {
            return Ok(false);
        }
        encoded_size.push(byte[0]);
        size |= usize::from(byte[0] & 0x7f) << shift;
        if byte[0] & 0x80 == 0 {
            break;
        }
    }
    if encoded_size.last().is_some_and(|byte| byte & 0x80 != 0) || !(2..=2_097_152).contains(&size)
    {
        return Ok(false);
    }
    let mut data = vec![0u8; encoded_size.len() + size];
    data[..encoded_size.len()].copy_from_slice(&encoded_size);
    if file.read_exact(&mut data[encoded_size.len()..]).is_err() {
        return Ok(false);
    }
    if !matches!(data[encoded_size.len()], 1 | 4) {
        return Ok(false);
    }
    Ok(crc32fast::hash(&data) == u32::from_le_bytes(checksum))
}
