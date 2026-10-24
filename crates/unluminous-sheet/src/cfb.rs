//! The compound file: the container an `.xls` workbook (and an encrypted `.xlsx`) is stored in.
//!
//! A compound file is a small file system inside one file. It is cut into sectors, a table of sector
//! chains (the FAT) says which sector follows which, and a directory lists the storages and streams by
//! name. A stream shorter than the cutoff (4096 bytes) lives in the mini stream, which is itself a
//! stream made of 64 byte sectors with a FAT of its own.
//!
//! Only reading is done here, and only what a workbook needs: finding a stream by name and reading its
//! bytes. Every offset is checked, and a chain that loops or runs off the end of the file is an error, not
//! a hang.

/// The first eight bytes of every compound file.
pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// What a person is told for a workbook that is protected with a password. The `.xlsx` path says the same
/// words for an encrypted zip entry.
pub const ENCRYPTED: &str =
    "this workbook is protected with a password, which this version cannot open";

const FREE: u32 = 0xFFFF_FFFF;
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
const FAT_SECTOR: u32 = 0xFFFF_FFFD;
const DIFAT_SECTOR: u32 = 0xFFFF_FFFC;
/// The most directory entries read, a bound for a damaged file.
const MOST_ENTRIES: usize = 1 << 20;

/// What a directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Storage,
    Stream,
    Root,
}

/// One directory entry with the path of storages above it.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Names from the root down, joined with `/`: `_VBA_PROJECT_CUR/VBA/Module1`.
    pub path: String,
    pub kind: EntryKind,
    pub start: u32,
    pub size: u64,
}

/// A compound file read into memory.
#[derive(Debug)]
pub struct Compound {
    entries: Vec<Entry>,
    data: Vec<u8>,
    sector_size: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    mini_sector: usize,
    mini_cutoff: u64,
}

fn truncated() -> String {
    "the compound file ends in the middle of a sector".to_owned()
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(truncated)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(truncated)
}

/// Whether the bytes start like a compound file.
pub fn is_compound(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

impl Compound {
    /// Read the container: header, FAT, directory and mini stream.
    pub fn parse(bytes: &[u8]) -> Result<Compound, String> {
        if !is_compound(bytes) {
            return Err("this is not a compound file".to_owned());
        }
        let shift = u16_at(bytes, 0x1E)? as u32;
        if !(7..=16).contains(&shift) {
            return Err("the compound file names an impossible sector size".to_owned());
        }
        let sector_size = 1usize << shift;
        let mini_shift = u16_at(bytes, 0x20)? as u32;
        let mini_size = 1usize << mini_shift.min(16);
        let mini_cutoff = u32_at(bytes, 0x38)? as u64;
        let first_directory = u32_at(bytes, 0x30)?;
        let first_mini_fat = u32_at(bytes, 0x3C)?;
        let mut book = Compound {
            entries: Vec::new(),
            data: bytes.to_vec(),
            sector_size,
            fat: Vec::new(),
            mini_fat: Vec::new(),
            mini_stream: Vec::new(),
            mini_sector: mini_size,
            mini_cutoff,
        };
        book.fat = book.read_fat(bytes)?;
        let directory = book.chain_bytes(first_directory, None)?;
        let nodes = parse_directory(&directory)?;
        book.entries = nodes_to_entries(&nodes);
        if let Some(root) = nodes.first() {
            let mini_bytes = book.chain_bytes(root.start, Some(root.size))?;
            book.mini_stream = mini_bytes;
        }
        let mini_fat_bytes = if first_mini_fat < END_OF_CHAIN {
            book.chain_bytes(first_mini_fat, None)?
        } else {
            Vec::new()
        };
        book.mini_fat = mini_fat_bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(book)
    }

    /// The most sectors a chain can pass through: every one of the file, so a longer chain loops.
    fn most_links(&self) -> usize {
        self.data.len() / self.sector_size + 2
    }

    /// The offset of a sector in the file: sector 0 starts after the header sector.
    fn sector_at(&self, sector: u32) -> Option<&[u8]> {
        let start = (sector as usize).checked_add(1)?.checked_mul(self.sector_size)?;
        self.data.get(start..start.checked_add(self.sector_size)?).or_else(|| {
            // The last sector of a file may be written short.
            let tail = self.data.get(start..)?;
            (!tail.is_empty()).then_some(tail)
        })
    }

    /// The whole FAT, from the 109 entries in the header and the DIFAT sectors after them.
    fn read_fat(&self, header: &[u8]) -> Result<Vec<u32>, String> {
        let mut fat_sectors: Vec<u32> = Vec::new();
        for i in 0..109 {
            let sector = u32_at(header, 0x4C + i * 4)?;
            if sector < FAT_SECTOR {
                fat_sectors.push(sector);
            }
        }
        let mut difat = u32_at(header, 0x44)?;
        let mut guard = 0;
        while difat < FAT_SECTOR {
            guard += 1;
            if guard > self.most_links() {
                return Err("the compound file's sector index loops".to_owned());
            }
            let sector = self.sector_at(difat).ok_or_else(truncated)?;
            let per = self.sector_size / 4 - 1;
            for i in 0..per {
                let entry = u32_at(sector, i * 4)?;
                if entry < FAT_SECTOR {
                    fat_sectors.push(entry);
                }
            }
            difat = u32_at(sector, per * 4)?;
        }
        let mut fat = Vec::with_capacity(fat_sectors.len() * self.sector_size / 4);
        for sector in fat_sectors {
            let bytes = self.sector_at(sector).ok_or_else(truncated)?;
            fat.extend(bytes.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])));
        }
        Ok(fat)
    }

    /// The bytes of a chain of ordinary sectors, cut to `size` when it is given.
    fn chain_bytes(&self, start: u32, size: Option<u64>) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut at = start;
        let mut links = 0usize;
        while at < FAT_SECTOR {
            links += 1;
            if links > self.most_links() {
                return Err("a chain of sectors in the compound file loops".to_owned());
            }
            let sector = self.sector_at(at).ok_or_else(truncated)?;
            out.extend_from_slice(sector);
            if let Some(size) = size {
                if out.len() as u64 >= size {
                    break;
                }
            }
            at = *self.fat.get(at as usize).ok_or_else(truncated)?;
            if at == FREE || at == DIFAT_SECTOR {
                break;
            }
        }
        if let Some(size) = size {
            out.truncate(size as usize);
        }
        Ok(out)
    }

    /// The bytes of a chain in the mini stream.
    fn mini_chain_bytes(&self, start: u32, size: u64) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut at = start;
        let mut links = 0usize;
        while at < FAT_SECTOR && (out.len() as u64) < size {
            links += 1;
            if links > self.mini_stream.len() / self.mini_sector + 2 {
                return Err("a chain of mini sectors in the compound file loops".to_owned());
            }
            let from = at as usize * self.mini_sector;
            let part = self
                .mini_stream
                .get(from..(from + self.mini_sector).min(self.mini_stream.len()))
                .filter(|p| !p.is_empty())
                .ok_or_else(truncated)?;
            out.extend_from_slice(part);
            at = *self.mini_fat.get(at as usize).ok_or_else(truncated)?;
        }
        out.truncate(size as usize);
        Ok(out)
    }

    /// Every directory entry, storages and streams.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Whether a stream or storage of this name exists, compared without regard to case.
    pub fn has(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.kind != EntryKind::Root && e.path.eq_ignore_ascii_case(path))
    }

    /// The bytes of a stream, or `None` when there is no stream of that path.
    pub fn stream(&self, path: &str) -> Option<Result<Vec<u8>, String>> {
        let entry = self.find(path).filter(|e| e.kind == EntryKind::Stream)?;
        Some(self.read_entry(entry))
    }

    /// The bytes of one entry.
    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        if entry.size < self.mini_cutoff {
            self.mini_chain_bytes(entry.start, entry.size)
        } else {
            self.chain_bytes(entry.start, Some(entry.size))
        }
    }
}

/// A directory entry as stored, before its path is known.
struct Node {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

fn parse_directory(bytes: &[u8]) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    for raw in bytes.chunks_exact(128).take(MOST_ENTRIES) {
        let name_len = (u16_at(raw, 64)? as usize).min(64);
        let units: Vec<u16> = raw[..name_len.saturating_sub(2)]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        nodes.push(Node {
            name: String::from_utf16_lossy(&units),
            kind: raw[66],
            left: u32_at(raw, 68)?,
            right: u32_at(raw, 72)?,
            child: u32_at(raw, 76)?,
            start: u32_at(raw, 116)?,
            size: u32_at(raw, 120)? as u64 | (u32_at(raw, 124)? as u64) << 32,
        });
    }
    if nodes.is_empty() {
        return Err("the compound file has no directory".to_owned());
    }
    Ok(nodes)
}

/// Walk the directory's trees from the root, giving each entry its path.
fn nodes_to_entries(nodes: &[Node]) -> Vec<Entry> {
    let mut entries = Vec::new();
    let root = &nodes[0];
    entries.push(Entry {
        path: String::new(),
        kind: EntryKind::Root,
        start: root.start,
        size: root.size,
    });
    let mut seen = vec![false; nodes.len()];
    let mut stack: Vec<(u32, String)> = vec![(root.child, String::new())];
    while let Some((index, prefix)) = stack.pop() {
        let Some(node) = nodes.get(index as usize).filter(|_| !seen[index as usize]) else {
            continue;
        };
        seen[index as usize] = true;
        let path =
            if prefix.is_empty() { node.name.clone() } else { format!("{prefix}/{}", node.name) };
        stack.push((node.left, prefix.clone()));
        stack.push((node.right, prefix));
        let kind = match node.kind {
            1 => EntryKind::Storage,
            _ => EntryKind::Stream,
        };
        if node.kind == 1 {
            stack.push((node.child, path.clone()));
        }
        if node.kind == 1 || node.kind == 2 {
            entries.push(Entry { path, kind, start: node.start, size: node.size });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A compound file with one small stream and one large one, built by hand.
    fn tiny_file() -> Vec<u8> {
        let sector = 512usize;
        let mut file = vec![0u8; sector * 7];
        file[..8].copy_from_slice(&SIGNATURE);
        file[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        file[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        file[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes()); // one FAT sector
        file[0x30..0x34].copy_from_slice(&1u32.to_le_bytes()); // directory at sector 1
        file[0x38..0x3C].copy_from_slice(&512u32.to_le_bytes());
        file[0x3C..0x40].copy_from_slice(&2u32.to_le_bytes()); // mini FAT at sector 2
        file[0x44..0x48].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        file[0x4C..0x50].copy_from_slice(&0u32.to_le_bytes()); // FAT at sector 0
        for i in 1..109 {
            file[0x4C + i * 4..0x50 + i * 4].copy_from_slice(&FREE.to_le_bytes());
        }
        let set_fat = |file: &mut Vec<u8>, index: usize, value: u32| {
            let at = sector + index * 4;
            file[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        // Sector 0 is the FAT, 1 the directory, 2 the mini FAT, 3 the mini stream, 4 and 5 a large stream.
        set_fat(&mut file, 0, FAT_SECTOR);
        set_fat(&mut file, 1, END_OF_CHAIN);
        set_fat(&mut file, 2, END_OF_CHAIN);
        set_fat(&mut file, 3, END_OF_CHAIN);
        set_fat(&mut file, 4, 5);
        set_fat(&mut file, 5, END_OF_CHAIN);
        for index in 6..128 {
            set_fat(&mut file, index, FREE);
        }
        // Directory: root, "Small" (mini sector 0, 5 bytes) and "Big" (sector 4, 1024 bytes).
        let dir = sector * 2;
        let entry = |file: &mut Vec<u8>,
                     n: usize,
                     name: &str,
                     kind: u8,
                     links: (u32, u32, u32),
                     start: u32,
                     size: u32| {
            let at = dir + n * 128;
            for (i, unit) in name.encode_utf16().enumerate() {
                file[at + i * 2..at + i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
            }
            file[at + 64..at + 66].copy_from_slice(&(((name.len() + 1) * 2) as u16).to_le_bytes());
            file[at + 66] = kind;
            file[at + 68..at + 72].copy_from_slice(&links.0.to_le_bytes());
            file[at + 72..at + 76].copy_from_slice(&links.1.to_le_bytes());
            file[at + 76..at + 80].copy_from_slice(&links.2.to_le_bytes());
            file[at + 116..at + 120].copy_from_slice(&start.to_le_bytes());
            file[at + 120..at + 124].copy_from_slice(&size.to_le_bytes());
        };
        entry(&mut file, 0, "Root Entry", 5, (FREE, FREE, 1), 3, 64);
        entry(&mut file, 1, "Small", 2, (FREE, 2, FREE), 0, 5);
        entry(&mut file, 2, "Big", 2, (FREE, FREE, FREE), 4, 1024);
        // The mini FAT: mini sector 0 ends the chain.
        file[sector * 3..sector * 3 + 4].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        // The mini stream holds "hello" at its start.
        file[sector * 4..sector * 4 + 5].copy_from_slice(b"hello");
        // The large stream's bytes are a counting pattern.
        for i in 0..1024 {
            file[sector * 5 + i] = (i % 251) as u8;
        }
        file
    }

    #[test]
    fn a_small_and_a_large_stream_are_read_by_name() {
        let compound = Compound::parse(&tiny_file()).unwrap();
        assert_eq!(compound.stream("Small").unwrap().unwrap(), b"hello");
        let big = compound.stream("big").unwrap().unwrap();
        assert_eq!(big.len(), 1024);
        assert_eq!(big[250], 250);
        assert!(compound.stream("Missing").is_none());
        assert!(compound.has("Small"));
    }

    #[test]
    fn a_file_that_is_not_a_compound_file_is_refused() {
        assert!(Compound::parse(b"PK\x03\x04 not it").is_err());
    }
}
