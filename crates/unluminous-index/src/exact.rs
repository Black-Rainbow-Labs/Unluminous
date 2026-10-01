//! The exact engine: literal and regex search with the same `(path, line)` results as ripgrep.
//!
//! Every file's content is held compressed in memory, beside its trigrams' posting lists. A search plans
//! a trigram query from the pattern (`plan`), intersects posting lists to get candidate files, and
//! verifies each candidate with `grep-searcher` and `grep-regex`, the crates ripgrep is built from, set
//! up as ripgrep sets them up for a directory search: line numbers on, binary detection that stops at
//! the first NUL, byte order mark sniffing, `\n` as the line terminator. A file named explicitly as the
//! search path is searched the way ripgrep searches a file named on its command line, with binary
//! detection that converts rather than stops.
//!
//! Nothing is walked and no process is started, so a search costs the candidates it verifies.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Bytes;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use rayon::prelude::*;

use crate::files::{self, Found, Scope};
use crate::plan::{plan, Query};
use crate::trigram::{trigrams_of, Postings};

/// One file the index holds.
#[derive(Clone, Debug)]
pub struct FileRecord {
    /// Its path relative to the root, with forward slashes.
    pub rel: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its modification time in nanoseconds since the epoch.
    pub mtime_ns: i64,
    /// The blake3 hash of its bytes.
    pub hash: [u8; 32],
    /// Its bytes, lz4 compressed with the length in front.
    pub packed: Vec<u8>,
    /// Whether it holds a NUL byte, which is how ripgrep decides a file is binary.
    pub binary: bool,
    /// How many lines it has.
    pub lines: u32,
}

impl FileRecord {
    /// Reads, hashes and packs a file.
    ///
    /// @param root - the root
    /// @param found - the file as the walk found it
    pub fn read(root: &Path, found: &Found) -> Option<(FileRecord, Vec<u32>)> {
        let bytes = std::fs::read(root.join(&found.rel)).ok()?;
        Some(FileRecord::from_bytes(found.rel.clone(), found.mtime_ns, &bytes))
    }

    /// A record and its trigrams from a file's bytes.
    ///
    /// @param rel - the path relative to the root
    /// @param mtime_ns - the modification time
    /// @param bytes - the file's bytes
    pub fn from_bytes(rel: String, mtime_ns: i64, bytes: &[u8]) -> (FileRecord, Vec<u32>) {
        let searched = searched_text(bytes);
        let record = FileRecord {
            rel,
            size: bytes.len() as u64,
            mtime_ns,
            hash: *blake3::hash(bytes).as_bytes(),
            packed: lz4_flex::block::compress_prepend_size(bytes),
            binary: memchr::memchr(0, &searched).is_some(),
            lines: memchr::memchr_iter(b'\n', &searched).count() as u32 + u32::from(!searched.ends_with(b"\n") && !searched.is_empty()),
        };
        (record, trigrams_of(&searched))
    }

    /// The file's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        lz4_flex::block::decompress_size_prepended(&self.packed).unwrap_or_default()
    }
}

/// The text ripgrep actually searches in a file: a file starting with a UTF-16 byte order mark is
/// decoded to UTF-8 first, as ripgrep's default `--encoding auto` does; anything else is searched as it
/// is.
///
/// @param bytes - the file's bytes
pub fn searched_text(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let encoding = match bytes {
        [0xFF, 0xFE, ..] => encoding_rs::UTF_16LE,
        [0xFE, 0xFF, ..] => encoding_rs::UTF_16BE,
        _ => return std::borrow::Cow::Borrowed(bytes),
    };
    let (text, _, _) = encoding.decode(bytes);
    std::borrow::Cow::Owned(text.into_owned().into_bytes())
}

/// One matching line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The file, relative to the root.
    pub path: String,
    /// The line number, from one.
    pub line: u64,
    /// The line's text, lossily decoded, without its line ending.
    pub text: String,
}

/// What one exact search did and found.
#[derive(Debug, Default)]
pub struct ExactAnswer {
    /// The matching lines, sorted by path then line.
    pub hits: Vec<Hit>,
    /// Files in scope.
    pub in_scope: usize,
    /// Files the trigram query let through.
    pub candidates: usize,
    /// Files the regex was run over.
    pub verified: usize,
    /// Whether the pattern had no usable trigrams, so every file in scope was verified.
    pub unbounded: bool,
}

/// A search request, as the Grep tool's arguments put it.
pub struct ExactRequest<'a> {
    /// The pattern, a regex.
    pub pattern: &'a str,
    /// `-i`.
    pub case_insensitive: bool,
    /// The path, globs and types narrowing the file set.
    pub scope: &'a Scope,
}

/// The in-memory exact index of one root.
#[derive(Default)]
pub struct Exact {
    /// Files by id; a tombstoned id holds None.
    pub files: Vec<Option<FileRecord>>,
    /// Ids by path.
    pub by_path: HashMap<String, u32>,
    /// The posting lists.
    pub postings: Postings,
    tombstones: usize,
}

impl Exact {
    /// Builds the index of a root from scratch: walks it, reads every file in parallel, and adds them
    /// in path order.
    ///
    /// @param root - the root
    pub fn build(root: &Path) -> Exact {
        let found = files::walk(root);
        let read: Vec<Option<(FileRecord, Vec<u32>)>> = found.par_iter().map(|f| FileRecord::read(root, f)).collect();
        let mut exact = Exact::default();
        for (record, trigrams) in read.into_iter().flatten() {
            exact.insert(record, &trigrams);
        }
        exact
    }

    /// An index put back together from what the store held.
    ///
    /// @param files - records by id, None for a tombstone
    /// @param postings - the posting lists, which already cover every id
    pub fn restore(files: Vec<Option<FileRecord>>, postings: Postings) -> Exact {
        let by_path = files.iter().enumerate().filter_map(|(id, f)| f.as_ref().map(|f| (f.rel.clone(), id as u32))).collect();
        let tombstones = files.iter().filter(|f| f.is_none()).count();
        Exact { files, by_path, postings, tombstones }
    }

    /// Adds a file under the next id, tombstoning any older copy of the same path.
    ///
    /// @param record - the file
    /// @param trigrams - its trigrams
    pub fn insert(&mut self, record: FileRecord, trigrams: &[u32]) {
        self.remove(&record.rel);
        let id = self.files.len() as u32;
        self.postings.add(id, trigrams);
        self.by_path.insert(record.rel.clone(), id);
        self.files.push(Some(record));
    }

    /// Tombstones a path.
    ///
    /// @param rel - the path relative to the root
    pub fn remove(&mut self, rel: &str) {
        if let Some(id) = self.by_path.remove(rel) {
            self.files[id as usize] = None;
            self.tombstones += 1;
        }
    }

    /// How many ids are tombstoned, which is when the lists are worth rebuilding.
    pub fn tombstones(&self) -> usize {
        self.tombstones
    }

    /// How many live files the index holds.
    pub fn live(&self) -> usize {
        self.by_path.len()
    }

    /// The bytes the index holds in memory.
    pub fn heap_bytes(&self) -> usize {
        self.files.iter().flatten().map(|f| f.packed.capacity() + f.rel.capacity() + 96).sum::<usize>() + self.postings.heap_bytes()
    }

    /// Runs one exact search.
    ///
    /// @param root - the root, for an explicit file the index does not hold
    /// @param request - the pattern, case and scope
    pub fn search(&self, root: &Path, request: &ExactRequest) -> Result<ExactAnswer, String> {
        let matcher = matcher(request.pattern, request.case_insensitive)?;
        if let Some(path) = request.scope.explicit_path(root) {
            return Ok(self.search_explicit(&matcher, &request.scope.path, &path));
        }
        let query = plan(request.pattern, request.case_insensitive)?;
        let in_scope: Vec<u32> = self
            .by_path
            .iter()
            .filter(|(rel, _)| request.scope.contains(rel))
            .map(|(_, &id)| id)
            .collect();
        let unbounded = query == Query::All;
        let candidates: Vec<u32> = if unbounded {
            in_scope.clone()
        } else {
            let allowed = self.evaluate(&query);
            let mut ids: Vec<u32> = in_scope.iter().copied().filter(|id| allowed.binary_search(id).is_ok()).collect();
            ids.sort_unstable();
            ids
        };
        let mut hits: Vec<Hit> = candidates
            .par_iter()
            .filter_map(|&id| self.files[id as usize].as_ref())
            .flat_map_iter(|record| verify(&matcher, &record.rel, &record.bytes(), BinaryDetection::quit(b'\x00')))
            .collect();
        hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        Ok(ExactAnswer { hits, in_scope: in_scope.len(), candidates: candidates.len(), verified: candidates.len(), unbounded })
    }

    /// Searches one file named as the search path, the way ripgrep searches a file named on its
    /// command line: whether or not an ignore rule covers it, and converting binary data rather than
    /// stopping at it.
    ///
    /// @param matcher - the regex
    /// @param rel - the path relative to the root
    /// @param path - the absolute path
    fn search_explicit(&self, matcher: &RegexMatcher, rel: &str, path: &PathBuf) -> ExactAnswer {
        let bytes = match self.by_path.get(rel).and_then(|&id| self.files[id as usize].as_ref()) {
            Some(record) => record.bytes(),
            None => std::fs::read(path).unwrap_or_default(),
        };
        let hits = verify(matcher, rel, &bytes, BinaryDetection::convert(b'\x00'));
        ExactAnswer { hits, in_scope: 1, candidates: 1, verified: 1, unbounded: false }
    }

    /// The sorted ids of the files a trigram query lets through.
    ///
    /// @param query - the query, never `All` at the top
    fn evaluate(&self, query: &Query) -> Vec<u32> {
        match query {
            Query::All => {
                let mut ids: Vec<u32> = self.by_path.values().copied().collect();
                ids.sort_unstable();
                ids
            }
            Query::Tri(t) => self.postings.files_with(*t),
            Query::And(parts) => {
                let mut parts: Vec<&Query> = parts.iter().collect();
                parts.sort_by_key(|q| match q {
                    Query::Tri(t) => self.postings.count(*t),
                    _ => usize::MAX,
                });
                let mut acc = self.evaluate(parts[0]);
                for p in &parts[1..] {
                    if acc.is_empty() {
                        break;
                    }
                    acc = intersect(&acc, &self.evaluate(p));
                }
                acc
            }
            Query::Or(parts) => {
                let mut acc: Vec<u32> = parts.iter().flat_map(|p| self.evaluate(p)).collect();
                acc.sort_unstable();
                acc.dedup();
                acc
            }
        }
    }
}

/// The intersection of two sorted id lists.
///
/// @param a - a sorted list
/// @param b - a sorted list
fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// The regex matcher ripgrep builds for a pattern with the Grep tool's arguments.
///
/// @param pattern - the pattern
/// @param case_insensitive - `-i`
pub fn matcher(pattern: &str, case_insensitive: bool) -> Result<RegexMatcher, String> {
    RegexMatcherBuilder::new()
        .case_insensitive(case_insensitive)
        .case_smart(false)
        .multi_line(false)
        .unicode(true)
        .octal(false)
        .line_terminator(Some(b'\n'))
        .build(pattern)
        .map_err(|e| e.to_string())
}

/// Runs the regex over one file's bytes as ripgrep's searcher would, and returns the matching lines.
///
/// @param matcher - the regex
/// @param rel - the file's path, for the hits
/// @param bytes - the file's bytes
/// @param binary - how binary data is treated
pub fn verify(matcher: &RegexMatcher, rel: &str, bytes: &[u8], binary: BinaryDetection) -> Vec<Hit> {
    if binary.convert_byte().is_some() && memchr::memchr(0, bytes).is_some() {
        return verify_converted(matcher, rel, bytes, binary);
    }
    let mut searcher: Searcher = SearcherBuilder::new().line_number(true).binary_detection(binary).bom_sniffing(true).build();
    let mut hits = Vec::new();
    let _ = searcher.search_reader(
        matcher,
        Cursor::new(bytes),
        Bytes(|line, text| {
            let text = String::from_utf8_lossy(text);
            hits.push(Hit { path: rel.to_owned(), line, text: text.trim_end_matches(['\n', '\r']).to_owned() });
            Ok(true)
        }),
    );
    hits
}

/// Searches a binary file named as the search path. The searcher turns each NUL into a line break to
/// keep going, so the line numbers it counts include those breaks; ripgrep reports the file's real line,
/// so each match's byte offset is mapped back to the line it is on, and a real line is reported once.
///
/// @param matcher - the regex
/// @param rel - the file's path
/// @param bytes - the file's bytes
/// @param binary - convert mode
fn verify_converted(matcher: &RegexMatcher, rel: &str, bytes: &[u8], binary: BinaryDetection) -> Vec<Hit> {
    struct Offsets(Vec<u64>);
    impl grep_searcher::Sink for Offsets {
        type Error = std::io::Error;
        fn matched(&mut self, _: &Searcher, m: &grep_searcher::SinkMatch<'_>) -> Result<bool, std::io::Error> {
            self.0.push(m.absolute_byte_offset());
            Ok(true)
        }
    }
    let mut searcher: Searcher = SearcherBuilder::new().line_number(false).binary_detection(binary).bom_sniffing(true).build();
    let mut offsets = Offsets(Vec::new());
    let _ = searcher.search_reader(matcher, Cursor::new(bytes), &mut offsets);
    let mut hits: Vec<Hit> = Vec::new();
    for offset in offsets.0 {
        let at = (offset as usize).min(bytes.len());
        let line = memchr::memchr_iter(b'\n', &bytes[..at]).count() as u64 + 1;
        if hits.last().is_some_and(|h| h.line == line) {
            continue;
        }
        let start = memchr::memrchr(b'\n', &bytes[..at]).map_or(0, |i| i + 1);
        let end = memchr::memchr(b'\n', &bytes[at..]).map_or(bytes.len(), |i| at + i);
        let text = String::from_utf8_lossy(&bytes[start..end]).replace('\0', " ");
        hits.push(Hit { path: rel.to_owned(), line, text: text.trim_end_matches('\r').to_owned() });
    }
    hits
}
