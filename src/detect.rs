use std::fs::File;
use std::io::Read;
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

pub(crate) fn detect_format(path: &Path) -> Result<Option<Format>> {
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
