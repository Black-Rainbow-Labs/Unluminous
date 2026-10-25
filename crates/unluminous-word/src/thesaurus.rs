//! Synonyms for a word, answered offline from WordNet.
//!
//! The data is `data/thesaurus.txt.gz`, made from Princeton's WordNet 3.1 by a script described in
//! `data/README.md`. It is compiled into the crate, decompressed once on first use and kept as one string
//! with a sorted list of line positions, so a lookup is a binary search and the memory held is the text
//! plus sixteen bytes a line.
//!
//! Each line of the text is the word, the part of speech, then one tab separated field for each meaning,
//! with `|` between its words. A section after `#exceptions` maps an inflected form such as `ran` or
//! `geese` to its base word.

use flate2::read::GzDecoder;
use std::io::Read;
use std::sync::OnceLock;

static COMPRESSED: &[u8] = include_bytes!("../data/thesaurus.txt.gz");

/// One meaning of a word: its part of speech (`noun`, `verb`, `adjective`, `adverb`) and the words that mean the same.
#[derive(Debug, Clone, PartialEq)]
pub struct Meaning {
    pub part_of_speech: &'static str,
    pub synonyms: Vec<String>,
}

/// The decompressed text, the byte range of each word line in sorted order, and the exception lines.
struct Thesaurus {
    text: String,
    lines: Vec<(usize, usize)>,
    exceptions: Vec<(usize, usize)>,
}

static THESAURUS: OnceLock<Thesaurus> = OnceLock::new();

/// Decompresses the shipped data and records where each line starts and ends.
fn load() -> Thesaurus {
    let mut text = String::new();
    let _ = GzDecoder::new(COMPRESSED).read_to_string(&mut text);
    let mut lines = Vec::new();
    let mut exceptions = Vec::new();
    let mut in_exceptions = false;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let end = start + line.trim_end_matches('\n').len();
        if line.starts_with("#exceptions") {
            in_exceptions = true;
        } else if in_exceptions {
            exceptions.push((start, end));
        } else {
            lines.push((start, end));
        }
        start += line.len();
    }
    Thesaurus { text, lines, exceptions }
}

/// The first field of a line, which is the word it is about.
fn key(text: &str, range: (usize, usize)) -> &str {
    text[range.0..range.1].split('\t').next().unwrap_or("")
}

impl Thesaurus {
    /// The index of the first line in `ranges` whose word is not before `word`.
    fn first_at_or_after(&self, ranges: &[(usize, usize)], word: &str) -> usize {
        ranges.partition_point(|r| key(&self.text, *r) < word)
    }

    /// The meanings listed for exactly `word`, in the order the data has them.
    fn meanings(&self, word: &str) -> Vec<Meaning> {
        let mut found = Vec::new();
        let mut at = self.first_at_or_after(&self.lines, word);
        while let Some(range) = self.lines.get(at) {
            let mut fields = self.text[range.0..range.1].split('\t');
            if fields.next() != Some(word) {
                break;
            }
            let part_of_speech = match fields.next() {
                Some("n") => "noun",
                Some("v") => "verb",
                Some("a") => "adjective",
                _ => "adverb",
            };
            for field in fields {
                let synonyms: Vec<String> = field.split('|').map(str::to_string).collect();
                found.push(Meaning { part_of_speech, synonyms });
            }
            at += 1;
        }
        found
    }

    /// The base word an irregular inflected form is listed under.
    fn base_of(&self, word: &str) -> Option<&str> {
        let at = self.first_at_or_after(&self.exceptions, word);
        let range = self.exceptions.get(at)?;
        let mut fields = self.text[range.0..range.1].split('\t');
        if fields.next() != Some(word) {
            return None;
        }
        fields.next()
    }
}

/// Candidate base words for a regular inflection, such as `runs`, `jumped`, `taller` or `cities`.
fn stripped(word: &str) -> Vec<String> {
    let rules = [
        ("ies", "y"),
        ("es", ""),
        ("s", ""),
        ("ing", ""),
        ("ing", "e"),
        ("ed", ""),
        ("ed", "e"),
        ("er", ""),
        ("est", ""),
    ];
    let mut out = Vec::new();
    for (suffix, replacement) in rules {
        let Some(stem) = word.strip_suffix(suffix) else { continue };
        if stem.len() < 2 {
            continue;
        }
        out.push(format!("{stem}{replacement}"));
        // A doubled final consonant, as in running or stopped.
        let bytes = stem.as_bytes();
        if bytes.len() >= 3 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
            out.push(stem[..stem.len() - 1].to_string());
        }
    }
    out
}

/// The meanings of `word`, most common first, empty when the thesaurus does not have it. Case insensitive;
/// an inflected form ("ran", "geese") is looked up as its base word.
pub fn synonyms(word: &str) -> Vec<Meaning> {
    let lower = word.trim().to_lowercase();
    if lower.is_empty() {
        return Vec::new();
    }
    let thesaurus = THESAURUS.get_or_init(load);
    let mut found = thesaurus.meanings(&lower);
    if let Some(base) = thesaurus.base_of(&lower) {
        found.extend(thesaurus.meanings(base));
    }
    if found.is_empty() {
        for candidate in stripped(&lower) {
            found = thesaurus.meanings(&candidate);
            if !found.is_empty() {
                break;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn has(meanings: &[Meaning], part_of_speech: &str, word: &str) -> bool {
        meanings
            .iter()
            .any(|m| m.part_of_speech == part_of_speech && m.synonyms.iter().any(|s| s == word))
    }

    #[test]
    fn happy_has_glad_as_an_adjective() {
        assert!(has(&synonyms("happy"), "adjective", "glad"));
    }

    #[test]
    fn car_has_automobile_as_a_noun() {
        assert!(has(&synonyms("car"), "noun", "automobile"));
    }

    #[test]
    fn an_irregular_form_is_looked_up_as_its_base() {
        assert!(synonyms("ran").iter().any(|m| m.part_of_speech == "verb"));
        assert_eq!(synonyms("geese"), synonyms("goose"));
    }

    #[test]
    fn a_regular_inflection_is_looked_up_as_its_stem() {
        assert!(has(&synonyms("cars"), "noun", "automobile"));
    }

    #[test]
    fn an_unknown_word_has_no_meanings() {
        assert!(synonyms("zzqx").is_empty());
        assert!(synonyms("").is_empty());
    }

    #[test]
    fn a_capital_letter_makes_no_difference() {
        assert_eq!(synonyms("Happy"), synonyms("happy"));
    }

    #[test]
    fn lookups_are_quick_after_the_first() {
        let started = Instant::now();
        let _ = synonyms("happy");
        let first = started.elapsed();
        let started = Instant::now();
        let _ = synonyms("quick");
        let second = started.elapsed();
        eprintln!("first lookup {first:?}, second {second:?}");
        assert!(first.as_secs_f64() < 2.0, "first lookup took {first:?}");
        assert!(second.as_millis() < 10, "second lookup took {second:?}");
    }
}
