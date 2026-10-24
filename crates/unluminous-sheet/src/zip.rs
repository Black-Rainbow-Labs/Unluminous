//! The zip container an `.xlsx` file is.
//!
//! Only what an Office file uses: entries stored or deflated, no encryption, no zip64. The central
//! directory is the authority on what is in the file, which is what every zip reader does, so a file
//! with bytes before its first entry (a self extracting stub) still reads.

use std::io::{Read, Write};

/// One file inside the zip.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(truncated)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(truncated)
}

fn truncated() -> String {
    "the file ends in the middle of its zip structure".to_owned()
}

/// Read every entry, in the order the central directory lists them.
pub fn read(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    if bytes.len() < 22 {
        return Err("the file is too short to be a zip".to_owned());
    }
    // The end record is in the last 64 KB plus its own size, after an optional comment.
    let from = bytes.len().saturating_sub(22 + 65_535);
    let end = (from..=bytes.len() - 22)
        .rev()
        .find(|&at| u32_at(bytes, at) == Ok(END))
        .ok_or_else(|| "this is not a zip file, which an .xlsx file is".to_owned())?;
    let count = u16_at(bytes, end + 10)? as usize;
    let directory_size = u32_at(bytes, end + 12)? as usize;
    let directory_at = u32_at(bytes, end + 16)? as usize;
    if count == 0xFFFF || directory_at == 0xFFFF_FFFF {
        return Err(
            "the file is a zip64 archive, which no workbook needs and this version does not read"
                .to_owned(),
        );
    }
    // A stub before the archive moves every offset by the same amount.
    let shift = end.saturating_sub(directory_at + directory_size);
    let mut at = directory_at + shift;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL {
            return Err("the zip directory is damaged".to_owned());
        }
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let compressed = u32_at(bytes, at + 20)? as usize;
        let size = u32_at(bytes, at + 24)? as usize;
        let name_len = u16_at(bytes, at + 28)? as usize;
        let extra_len = u16_at(bytes, at + 30)? as usize;
        let comment_len = u16_at(bytes, at + 32)? as usize;
        let local = u32_at(bytes, at + 42)? as usize + shift;
        let name_bytes = bytes.get(at + 46..at + 46 + name_len).ok_or_else(truncated)?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if flags & 1 != 0 {
            return Err(format!(
                "{name} is encrypted: this workbook is protected with a password, which this version cannot open"
            ));
        }
        if u32_at(bytes, local)? != LOCAL {
            return Err(format!("the zip entry for {name} is damaged"));
        }
        let data_at =
            local + 30 + u16_at(bytes, local + 26)? as usize + u16_at(bytes, local + 28)? as usize;
        let raw = bytes.get(data_at..data_at + compressed).ok_or_else(truncated)?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut out = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(raw)
                    .read_to_end(&mut out)
                    .map_err(|problem| format!("{name} could not be decompressed: {problem}"))?;
                out
            }
            other => {
                return Err(format!(
                    "{name} is compressed with method {other}, which this version does not read"
                ))
            }
        };
        if name.ends_with('/') {
            continue;
        }
        entries.push(Entry { name, data });
    }
    Ok(entries)
}

/// Write entries as a zip, deflated, in the order given.
pub fn write(entries: &[Entry]) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    // 1 January 1980, the earliest time a zip can hold; a fixed time keeps the file the same for the same
    // contents, which is what makes a saved workbook compare equal in a test.
    let (time, date) = (0u16, (1u16 << 5) | 1);
    for entry in entries {
        let mut crc = crc32fast::Hasher::new();
        crc.update(&entry.data);
        let crc = crc.finalize();
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&entry.data).map_err(|problem| problem.to_string())?;
        let compressed = encoder.finish().map_err(|problem| problem.to_string())?;
        if compressed.len() > u32::MAX as usize || out.len() > u32::MAX as usize {
            return Err("the workbook is larger than 4 GB, which a zip without zip64 cannot hold"
                .to_owned());
        }
        let offset = out.len() as u32;
        let name = entry.name.as_bytes();
        let header = |sig: u32| {
            let mut h: Vec<u8> = Vec::new();
            h.extend(sig.to_le_bytes());
            if sig == CENTRAL {
                h.extend(20u16.to_le_bytes()); // made by
            }
            h.extend(20u16.to_le_bytes()); // needed
            h.extend(0x0800u16.to_le_bytes()); // names are UTF-8
            h.extend(8u16.to_le_bytes());
            h.extend(time.to_le_bytes());
            h.extend(date.to_le_bytes());
            h.extend(crc.to_le_bytes());
            h.extend((compressed.len() as u32).to_le_bytes());
            h.extend((entry.data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            h
        };
        out.extend(header(LOCAL));
        out.extend(name);
        out.extend(&compressed);
        directory.extend(header(CENTRAL));
        directory.extend(0u16.to_le_bytes()); // comment
        directory.extend(0u16.to_le_bytes()); // disk
        directory.extend(0u16.to_le_bytes()); // internal attributes
        directory.extend(0u32.to_le_bytes()); // external attributes
        directory.extend(offset.to_le_bytes());
        directory.extend(name);
    }
    let directory_at = out.len() as u32;
    let count = entries.len() as u16;
    out.extend(&directory);
    out.extend(END.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend((directory.len() as u32).to_le_bytes());
    out.extend(directory_at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back_the_same() {
        let entries = vec![
            Entry { name: "[Content_Types].xml".to_owned(), data: b"<Types/>".to_vec() },
            Entry {
                name: "xl/worksheets/sheet1.xml".to_owned(),
                data: "é".repeat(10_000).into_bytes(),
            },
            Entry { name: "empty".to_owned(), data: Vec::new() },
        ];
        let bytes = write(&entries).unwrap();
        assert_eq!(read(&bytes).unwrap(), entries);
        assert_eq!(write(&entries).unwrap(), bytes, "the same entries make the same bytes");
    }

    #[test]
    fn something_that_is_not_a_zip_is_refused_with_a_sentence() {
        assert!(read(b"name,value\n").unwrap_err().contains("too short"));
        assert!(read(&[0u8; 100]).unwrap_err().contains("not a zip"));
    }
}
