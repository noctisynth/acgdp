use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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

pub(crate) fn detect_format(path: &Path, password: Option<&str>) -> Result<Option<Format>> {
    let mut first = [0u8; 8];
    let n = File::open(path)?.read(&mut first)?;
    if first[..n].starts_with(SEVEN_Z) && valid_7z_header(path, 0)? {
        return Ok(Some(Format::SevenZ(0)));
    }
    if (first[..n].starts_with(RAR4) || first[..n].starts_with(RAR5))
        && valid_rar_header(path, 0, password)?
    {
        return Ok(Some(Format::Rar));
    }
    let mut choices = Vec::new();
    if let Ok(zip) = zip::ZipArchive::new(File::open(path)?) {
        if zip.offset() == 0 {
            return Ok(Some(Format::Zip));
        }
        choices.push((zip.offset(), Format::Zip));
    }
    choices.extend(scan_signatures(path, password)?);
    choices.sort_by_key(|(offset, _)| *offset);
    Ok(choices.first().map(|(_, format)| *format))
}

fn scan_signatures(path: &Path, password: Option<&str>) -> Result<Vec<(u64, Format)>> {
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
        for i in memchr::memchr2_iter(SEVEN_Z[0], RAR4[0], &buffer[..len]) {
            let tail = &buffer[i..len];
            let format = if tail.starts_with(SEVEN_Z) {
                Some(Format::SevenZ(base + i as u64))
            } else if tail.starts_with(RAR4) || tail.starts_with(RAR5) {
                Some(Format::Rar)
            } else {
                None
            };
            if let Some(format) = format {
                let offset = base + i as u64;
                let valid = match format {
                    Format::SevenZ(_) => valid_7z_header(path, offset)?,
                    Format::Rar => valid_rar_header(path, offset, password)?,
                    Format::Zip => false,
                };
                if !valid {
                    continue;
                }
                found.push((offset, format));
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

fn valid_7z_header(path: &Path, offset: u64) -> Result<bool> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut header = [0u8; 32];
    if file.read_exact(&mut header).is_err() || !header.starts_with(SEVEN_Z) {
        return Ok(false);
    }
    let expected = u32::from_le_bytes(header[8..12].try_into()?);
    Ok(crc32fast::hash(&header[12..32]) == expected)
}

fn valid_rar_header(path: &Path, offset: u64, password: Option<&str>) -> Result<bool> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() {
        return Ok(false);
    }
    if signature.starts_with(RAR5) {
        return valid_rar5_block(&mut file);
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

fn valid_rar5_block(file: &mut File) -> Result<bool> {
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
