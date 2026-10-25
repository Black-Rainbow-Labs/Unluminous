//! Records: the unit a BIFF8 stream is made of, and a reader for the fields inside one.
//!
//! A record is a 16 bit type, a 16 bit length and that many bytes. A record longer than 8224 bytes is
//! continued in `CONTINUE` records straight after it; they are joined here, and the offsets where each
//! continuation began are kept because a string cut by a boundary starts its second half with a flag byte.

/// The record type of a continuation.
pub const CONTINUE: u16 = 0x003C;

/// One record with its continuations joined.
#[derive(Debug, Clone)]
pub struct Record {
    pub id: u16,
    pub data: Vec<u8>,
    /// Where in `data` each continuation began.
    pub starts: Vec<usize>,
    /// Where the record began in the stream, which a sheet's `BOUNDSHEET` names.
    pub offset: usize,
}

/// Cut a stream into records, joining each `CONTINUE` onto the record before it.
pub fn split(stream: &[u8]) -> Vec<Record> {
    let mut out: Vec<Record> = Vec::new();
    let mut at = 0usize;
    while at + 4 <= stream.len() {
        let id = u16::from_le_bytes([stream[at], stream[at + 1]]);
        let len = u16::from_le_bytes([stream[at + 2], stream[at + 3]]) as usize;
        let body_end = (at + 4 + len).min(stream.len());
        let body = &stream[at + 4..body_end];
        match out.last_mut() {
            Some(last) if id == CONTINUE => {
                last.starts.push(last.data.len());
                last.data.extend_from_slice(body);
            }
            _ => out.push(Record { id, data: body.to_vec(), starts: Vec::new(), offset: at }),
        }
        at = body_end;
    }
    out
}

/// A reader over the bytes of a record. Every read answers `None` at the end of the data, so a damaged
/// record ends the work on it instead of stopping the program.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, at: 0 }
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.data.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(slice)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    pub fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }

    pub fn f64(&mut self) -> Option<f64> {
        self.take(8).map(|b| {
            let mut a = [0u8; 8];
            a.copy_from_slice(b);
            f64::from_le_bytes(a)
        })
    }

    pub fn rest(&self) -> &'a [u8] {
        self.data.get(self.at..).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.at >= self.data.len()
    }

    /// `cch` characters, each one byte (the low byte of the character) or two.
    pub fn chars(&mut self, cch: usize, wide: bool) -> Option<String> {
        if wide {
            let bytes = self.take(cch.checked_mul(2)?)?;
            let units: Vec<u16> =
                bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Some(String::from_utf16_lossy(&units))
        } else {
            Some(self.take(cch)?.iter().map(|&b| b as char).collect())
        }
    }

    /// A string with a one byte length and a flag byte: `ShortXLUnicodeString`.
    pub fn short_string(&mut self) -> Option<String> {
        let cch = self.u8()? as usize;
        let flags = self.u8()?;
        self.chars(cch, flags & 1 != 0)
    }

    /// A string with a two byte length and a flag byte: `XLUnicodeString`.
    pub fn string(&mut self) -> Option<String> {
        let cch = self.u16()? as usize;
        let flags = self.u8()?;
        self.chars(cch, flags & 1 != 0)
    }

    /// A string whose length is known elsewhere: a flag byte and the characters.
    pub fn string_of(&mut self, cch: usize) -> Option<String> {
        let flags = self.u8()?;
        self.chars(cch, flags & 1 != 0)
    }
}

/// Read the shared string table. A string can be cut by the end of a record, and then its second half
/// starts with a new flag byte that says whether the characters are still one byte or two.
pub fn shared_strings(record: &Record) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 8usize;
    let data = &record.data;
    let count = data.get(4..8).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) as usize;
    while at < data.len() && out.len() < count.max(1 << 20) {
        let Some((text, next)) = shared_string(record, at) else { break };
        out.push(text);
        at = next;
    }
    out
}

/// One `XLUnicodeRichExtendedString` at an offset, and the offset after it.
fn shared_string(record: &Record, mut at: usize) -> Option<(String, usize)> {
    let data = &record.data;
    let cch = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as usize;
    let flags = *data.get(at + 2)?;
    at += 3;
    let mut wide = flags & 1 != 0;
    let runs = if flags & 8 != 0 {
        let v = u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]) as usize;
        at += 2;
        v
    } else {
        0
    };
    let extra = if flags & 4 != 0 {
        let v = u32::from_le_bytes([
            *data.get(at)?,
            *data.get(at + 1)?,
            *data.get(at + 2)?,
            *data.get(at + 3)?,
        ]) as usize;
        at += 4;
        v
    } else {
        0
    };
    let mut text = String::new();
    let mut remaining = cch;
    while remaining > 0 {
        if record.starts.contains(&at) {
            // The second half of a cut string begins with its own flag byte.
            wide = *data.get(at)? & 1 != 0;
            at += 1;
        }
        let boundary = record.starts.iter().copied().find(|&s| s > at).unwrap_or(data.len());
        let unit = if wide { 2 } else { 1 };
        let take = remaining.min((boundary - at) / unit);
        if take == 0 {
            return None;
        }
        let slice = data.get(at..at + take * unit)?;
        if wide {
            let units: Vec<u16> =
                slice.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            text.push_str(&String::from_utf16_lossy(&units));
        } else {
            text.extend(slice.iter().map(|&b| b as char));
        }
        at += take * unit;
        remaining -= take;
    }
    at += runs * 4 + extra;
    Some((text, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u16, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_le_bytes().to_vec();
        out.extend((body.len() as u16).to_le_bytes());
        out.extend(body);
        out
    }

    #[test]
    fn continue_records_are_joined_and_their_starts_kept() {
        let mut stream = record(0x00FC, &[1, 2, 3]);
        stream.extend(record(CONTINUE, &[4, 5]));
        stream.extend(record(0x000A, &[]));
        let records = split(&stream);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].data, vec![1, 2, 3, 4, 5]);
        assert_eq!(records[0].starts, vec![3]);
        assert_eq!(records[1].offset, 4 + 3 + 4 + 2);
    }

    #[test]
    fn a_string_cut_between_records_continues_with_a_new_flag_byte() {
        // Two strings: "abcdef" (cut after "abc", the rest as UTF-16) and "z".
        let mut first = vec![2, 0, 0, 0, 2, 0, 0, 0];
        first.extend([6, 0, 0]);
        first.extend(b"abc");
        let mut second = vec![1];
        for c in "def".encode_utf16() {
            second.extend(c.to_le_bytes());
        }
        second.extend([1, 0, 0, b'z']);
        let mut stream = record(0x00FC, &first);
        stream.extend(record(CONTINUE, &second));
        let strings = shared_strings(&split(&stream)[0]);
        assert_eq!(strings, vec!["abcdef".to_owned(), "z".to_owned()]);
    }
}
