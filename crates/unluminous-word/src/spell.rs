//! Spell check, as Word does it by default: a word not in the dictionary is underlined, and so is a word
//! typed twice in a row. Words in capitals, words with digits in them, and web, email and file addresses
//! are left alone, as are runs marked `w:noProof` and runs in a language other than English, because the
//! only dictionary is English.
//!
//! The dictionary is SCOWL's en_US Hunspell dictionary (`dictionaries/`, licence beside it), read by
//! `spellbook`. It is about 50,000 stems and costs a few megabytes once read, so it is read the first
//! time something asks to check a document, never when the plugin is only installed.

use std::collections::HashSet;
use std::io::Read;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::model::{Content, Document, Paragraph, Run};
use crate::position::{self, Path, StoryId};

const AFFIXES: &str = include_str!("../dictionaries/en_US.aff");
const WORDS_DEFLATED: &[u8] = include_bytes!("../dictionaries/en_US.dic.deflate");

/// Why a word is underlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Not in the dictionary.
    Spelling,
    /// The same word as the one before it.
    Repeated,
}

/// One underlined word: where it is, what it says, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Misspelling {
    pub story: StoryId,
    pub path: Path,
    /// Byte offsets in the paragraph's flat text.
    pub range: Range<usize>,
    pub word: String,
    pub kind: Kind,
}

/// The dictionary, with the words the person added and the words they chose to ignore.
pub struct Speller {
    dictionary: spellbook::Dictionary,
    ignored: HashSet<String>,
}

impl Speller {
    /// The English dictionary that ships with the crate.
    pub fn english() -> Result<Speller, String> {
        let mut words = String::new();
        flate2::read::DeflateDecoder::new(WORDS_DEFLATED)
            .read_to_string(&mut words)
            .map_err(|e| format!("the dictionary: {e}"))?;
        let dictionary = spellbook::Dictionary::new(AFFIXES, &words)
            .map_err(|e| format!("the dictionary: {e}"))?;
        Ok(Speller { dictionary, ignored: HashSet::new() })
    }

    /// Add a word to the dictionary for as long as this speller lives. Saving it is the caller's job.
    pub fn add_word(&mut self, word: &str) {
        let word = word.trim();
        if !word.is_empty() && !word.contains(char::is_whitespace) {
            let _ = self.dictionary.add(word);
        }
    }

    /// Stop underlining a word anywhere, which is Word's Ignore All.
    pub fn ignore(&mut self, word: &str) {
        self.ignored.insert(word.to_owned());
    }

    /// Whether a word is spelled as the dictionary has it, or was added or ignored.
    pub fn is_correct(&self, word: &str) -> bool {
        self.ignored.contains(word) || self.dictionary.check(word)
    }

    /// Up to `most` spellings the dictionary suggests for a word, best first.
    pub fn suggestions(&self, word: &str, most: usize) -> Vec<String> {
        let mut out = Vec::new();
        self.dictionary.suggest(word, &mut out);
        out.truncate(most);
        out
    }
}

/// The paragraph's flat text with everything that is not checked turned into spaces of the same byte
/// length, so offsets in it are offsets in the paragraph: field codes, deleted text, pictures and other
/// objects, and runs marked `w:noProof` or in a language other than English.
pub fn checked_text(paragraph: &Paragraph) -> String {
    let mut out = String::with_capacity(paragraph.len());
    let mut in_code: Vec<bool> = Vec::new();
    for run in &paragraph.runs {
        if let Content::FieldChar(el) = &run.content {
            match el.attr("w:fldCharType") {
                Some("begin") => in_code.push(true),
                Some("separate") => {
                    if let Some(last) = in_code.last_mut() {
                        *last = false;
                    }
                }
                _ => {
                    in_code.pop();
                }
            }
        }
        let checked = in_code.iter().all(|code| !*code) && !run.is_deleted() && is_proofed(run);
        match &run.content {
            Content::Text(text) if checked => out.push_str(text),
            content => out.extend(std::iter::repeat_n(' ', content.len())),
        }
    }
    out
}

/// Whether a run's own properties let it be checked: not `w:noProof`, and English or no language.
fn is_proofed(run: &Run) -> bool {
    if crate::props::on_off(&run.props, "w:noProof") == Some(true) {
        return false;
    }
    run.props
        .child("w:lang")
        .and_then(|lang| lang.attr("w:val"))
        .is_none_or(|lang| lang.to_ascii_lowercase().starts_with("en"))
}

/// The underlined words in a paragraph, as byte ranges of its flat text.
pub fn check_paragraph(paragraph: &Paragraph, speller: &Speller) -> Vec<(Range<usize>, Kind)> {
    check_text(&checked_text(paragraph), speller)
}

/// The underlined words in text that has already had what is not checked blanked out.
pub fn check_text(text: &str, speller: &Speller) -> Vec<(Range<usize>, Kind)> {
    let mut out = Vec::new();
    let mut previous: Option<(Range<usize>, String)> = None;
    for (start, token) in text.split_word_bound_indices() {
        if !token.chars().any(char::is_alphabetic) {
            // Only spaces between two words keep them a repeated pair; a comma or a full stop does not.
            if !token.chars().all(|c| c == ' ') {
                previous = None;
            }
            continue;
        }
        let word = token.trim_matches(|c| c == '\'' || c == '\u{2019}');
        let start = start + token.find(word).unwrap_or(0);
        let range = start..start + word.len();
        if is_left_alone(text, &range, word) {
            previous = None;
            continue;
        }
        let lower = word.to_lowercase();
        if previous.as_ref().is_some_and(|(_, before)| *before == lower) {
            out.push((range.clone(), Kind::Repeated));
        } else if !speller.is_correct(word) {
            out.push((range.clone(), Kind::Spelling));
        }
        previous = Some((range, lower));
    }
    out
}

/// Word's default exceptions: capitals, digits, and anything that is part of an address.
fn is_left_alone(text: &str, range: &Range<usize>, word: &str) -> bool {
    if word.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return true;
    }
    if word.contains('.') {
        return true;
    }
    // The run of text between spaces the word is in, which is what an address is.
    let before = text[..range.start].rfind(char::is_whitespace).map_or(0, |at| at + 1);
    let after = text[range.end..].find(char::is_whitespace).map_or(text.len(), |at| range.end + at);
    let chunk = &text[before..after];
    chunk.contains("://")
        || chunk.contains('@')
        || chunk.contains('\\')
        || chunk.starts_with("www.")
        || chunk.contains('/') && chunk.contains('.')
}

/// Every underlined word in the body, the headers and footers, the notes and the comments, in reading order.
pub fn check_document(document: &Document, speller: &Speller) -> Vec<Misspelling> {
    let mut out = Vec::new();
    for story in stories(document) {
        let Some(blocks) = position::story(document, &story) else { continue };
        for (path, paragraph) in position::paragraphs(blocks) {
            let text = paragraph.flat_text();
            for (range, kind) in check_paragraph(paragraph, speller) {
                out.push(Misspelling {
                    story: story.clone(),
                    path: path.clone(),
                    word: text[range.clone()].to_owned(),
                    range,
                    kind,
                });
            }
        }
    }
    out
}

/// The stories of a document, the body first.
pub fn stories(document: &Document) -> Vec<StoryId> {
    let mut out = vec![StoryId::Body];
    out.extend(document.headers.iter().map(|h| StoryId::Header(h.id.clone())));
    // Notes numbered 0 and below are the separator lines Word keeps in the notes part.
    let ids = |part: &Option<crate::model::NotesPart>, lowest: i64| -> Vec<i64> {
        part.as_ref()
            .map(|p| p.notes.iter().filter_map(|n| n.id()).filter(|id| *id >= lowest).collect())
            .unwrap_or_default()
    };
    out.extend(ids(&document.footnotes, 1).into_iter().map(StoryId::Footnote));
    out.extend(ids(&document.endnotes, 1).into_iter().map(StoryId::Endnote));
    out.extend(ids(&document.comments, 0).into_iter().map(StoryId::Comment));
    out
}
