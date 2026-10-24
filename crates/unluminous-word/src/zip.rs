//! The zip container a `.docx` file is.
//!
//! Only what an Office file uses: entries stored or deflated, no encryption, no zip64. The central
//! directory is the authority on what is in the file, which is what every zip reader does, so a file
//! with bytes before its first entry still reads. The interface is the one `unluminous-sheet` has
//! (`task-2338`), so one copy can later serve both.

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
/// The largest part this reader inflates, so a damaged size field cannot ask for the whole memory.
const LARGEST_PART: usize = 1 << 31;

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
        return Err("the file is too short to be a zip, which a .docx file is".to_owned());
    }
    // The end record is in the last 64 KB plus its own size, after an optional comment.
    let from = bytes.len().saturating_sub(22 + 65_535);
    let end = (from..=bytes.len() - 22)
        .rev()
        .find(|&at| u32_at(bytes, at) == Ok(END))
        .ok_or_else(|| not_a_zip(bytes))?;
    let count = u16_at(bytes, end + 10)? as usize;
    let directory_size = u32_at(bytes, end + 12)? as usize;
    let directory_at = u32_at(bytes, end + 16)? as usize;
    if count == 0xFFFF || directory_at == 0xFFFF_FFFF {
        return Err("the file is a zip64 archive, which this version does not read".to_owned());
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
        let name = bytes.get(at + 46..at + 46 + name_len).ok_or_else(truncated)?;
        let name = String::from_utf8_lossy(name).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if flags & 1 != 0 {
            return Err(format!("{name} is encrypted inside the zip"));
        }
        let data = entry_data(bytes, local, method, compressed, size, &name)?;
        entries.push(Entry { name, data });
    }
    Ok(entries)
}

/// The sentence for a file that has no zip directory at all, which names the two things it usually is.
fn not_a_zip(bytes: &[u8]) -> String {
    // An OLE compound file: a Word 97-2003 `.doc`, or a `.docx` protected with a password.
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return "this is a Word 97-2003 document or a password protected one, which is not a zip; \
                open it in Word and save it as .docx"
            .to_owned();
    }
    "this is not a zip file, which a .docx file is".to_owned()
}

/// One entry's bytes, read through its local header.
fn entry_data(
    bytes: &[u8],
    local: usize,
    method: u16,
    compressed: usize,
    size: usize,
    name: &str,
) -> Result<Vec<u8>, String> {
    if u32_at(bytes, local)? != LOCAL {
        return Err(format!("{name}: its local header is damaged"));
    }
    let name_len = u16_at(bytes, local + 26)? as usize;
    let extra_len = u16_at(bytes, local + 28)? as usize;
    let start = local + 30 + name_len + extra_len;
    let raw = bytes.get(start..start + compressed).ok_or_else(truncated)?;
    match method {
        0 => Ok(raw.to_vec()),
        8 => {
            let mut out = Vec::with_capacity(size.min(LARGEST_PART));
            flate2::read::DeflateDecoder::new(raw)
                .take(LARGEST_PART as u64)
                .read_to_end(&mut out)
                .map_err(|problem| format!("{name} could not be inflated: {problem}"))?;
            Ok(out)
        }
        other => Err(format!("{name} uses zip method {other}, which an Office file never does")),
    }
}

/// Write the entries as a zip, deflated, in the order given.
pub fn write(entries: &[Entry]) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for entry in entries {
        if entry.data.len() >= u32::MAX as usize {
            return Err(format!("{} is larger than 4 GB", entry.name));
        }
        let mut deflated =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
        deflated.write_all(&entry.data).map_err(|problem| problem.to_string())?;
        let deflated = deflated.finish().map_err(|problem| problem.to_string())?;
        let crc = crc32fast::hash(&entry.data);
        let offset = out.len() as u32;
        let name = entry.name.as_bytes();
        // The fixed date Office itself writes, 1980-01-01, so the same document saves to the same bytes.
        let (time, date) = (0u16, 0x0021u16);
        out.extend_from_slice(&LOCAL.to_le_bytes());
        for value in [20u16, 0, 8, time, date] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, deflated.len() as u32, entry.data.len() as u32] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&deflated);

        directory.extend_from_slice(&CENTRAL.to_le_bytes());
        for value in [20u16, 20, 0, 8, time, date] {
            directory.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, deflated.len() as u32, entry.data.len() as u32] {
            directory.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name.len() as u16, 0, 0, 0, 0] {
            directory.extend_from_slice(&value.to_le_bytes());
        }
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name);
    }
    if entries.len() >= 0xFFFF {
        return Err("a document with 65,535 parts or more needs zip64".to_owned());
    }
    let directory_at = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(&END.to_le_bytes());
    for value in [0u16, 0, entries.len() as u16, entries.len() as u16] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&directory_at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back_in_the_same_order() {
        let entries = vec![
            Entry { name: "[Content_Types].xml".into(), data: b"<Types/>".to_vec() },
            Entry { name: "word/document.xml".into(), data: "é".repeat(5000).into_bytes() },
            Entry { name: "empty".into(), data: Vec::new() },
        ];
        let bytes = write(&entries).unwrap();
        assert_eq!(read(&bytes).unwrap(), entries);
    }

    #[test]
    fn a_stub_in_front_of_the_archive_still_reads() {
        let entries = vec![Entry { name: "a".into(), data: b"hello".to_vec() }];
        let mut bytes = b"STUB-STUB".to_vec();
        bytes.extend(write(&entries).unwrap());
        assert_eq!(read(&bytes).unwrap(), entries);
    }

    #[test]
    fn an_old_word_file_is_named_rather_than_called_damaged() {
        let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        bytes.resize(600, 0);
        assert!(read(&bytes).unwrap_err().contains("Word 97-2003"));
    }

    #[test]
    fn a_cut_off_file_is_refused_rather_than_panicking() {
        let bytes = write(&[Entry { name: "a".into(), data: vec![7; 100] }]).unwrap();
        for cut in [10, 40, bytes.len() - 30] {
            let mut short = bytes[..cut].to_vec();
            short.extend_from_slice(&bytes[bytes.len() - 22..]);
            assert!(read(&short).is_err());
        }
    }
}
