//! Spell check for the Word tab: the dictionary, the words a person added, and what each paragraph has
//! underlined.
//!
//! **Nothing is read until a document is first checked.** The dictionary is about 50,000 stems and a few
//! megabytes once read, so a window with the Word plugin switched off, or with no Word document open,
//! never reads it. It is kept for the rest of the session once read, because the next document will
//! want it too.
//!
//! The words a person adds go in `words.dic` in the plugin's folder, one a line. Office's own custom
//! dictionary, `%APPDATA%\Microsoft\UProof\CUSTOM.DIC`, is read as well, so a word already added in Word
//! is not underlined here. Unluminous never writes to Office's file.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use unluminous_word::model::{Document, Paragraph};
use unluminous_word::spell::{self, Kind, Misspelling, Speller};

/// What one paragraph has underlined: byte ranges of its flat text, and why.
pub type Underlined = Rc<Vec<(Range<usize>, Kind)>>;

/// The speller and what it has already found, by the text that was checked.
struct Spelling {
    speller: Speller,
    found: HashMap<u64, Underlined>,
}

/// How many paragraphs' results are kept before the cache starts again.
const KEPT: usize = 20_000;

thread_local! {
    static FOLDER: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static SPELLING: RefCell<Option<Spelling>> = const { RefCell::new(None) };
    /// The dictionary could not be read, so spell check stays off rather than trying on every frame.
    static FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Say which folder the person's own words are kept in. Called when the plugin opens.
pub fn use_folder(folder: Option<&Path>) {
    FOLDER.with(|f| *f.borrow_mut() = folder.map(Path::to_path_buf));
}

/// The file the person's added words are in.
fn words_file() -> Option<PathBuf> {
    FOLDER.with(|f| f.borrow().as_ref().map(|folder| folder.join("words.dic")))
}

/// Run `work` with the speller, reading the dictionary and the added words the first time.
fn with<R>(work: impl FnOnce(&mut Spelling) -> R) -> Option<R> {
    SPELLING.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            if FAILED.get() {
                return None;
            }
            let mut speller = match Speller::english() {
                Ok(speller) => speller,
                Err(error) => {
                    eprintln!("Unluminous could not read the Word spell check dictionary: {error}");
                    FAILED.set(true);
                    return None;
                }
            };
            for word in own_words().into_iter().chain(office_words()) {
                speller.add_word(&word);
            }
            *slot = Some(Spelling { speller, found: HashMap::new() });
        }
        slot.as_mut().map(work)
    })
}

/// The words in `words.dic`.
fn own_words() -> Vec<String> {
    let Some(file) = words_file() else { return Vec::new() };
    std::fs::read_to_string(file)
        .map(|text| {
            text.lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_owned).collect()
        })
        .unwrap_or_default()
}

/// The words in Office's custom dictionary, which Word writes as UTF-16 with a byte order mark.
fn office_words() -> Vec<String> {
    let Some(appdata) = std::env::var_os("APPDATA") else { return Vec::new() };
    let Ok(bytes) = std::fs::read(Path::new(&appdata).join("Microsoft/UProof/CUSTOM.DIC")) else {
        return Vec::new();
    };
    let text = match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => String::from_utf16_lossy(
            &rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<u16>>(),
        ),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        other => String::from_utf8_lossy(other).into_owned(),
    };
    text.lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// What a paragraph has underlined. A paragraph whose checked text is unchanged is not checked again.
pub fn underlined(paragraph: &Paragraph) -> Underlined {
    let text = spell::checked_text(paragraph);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let key = hasher.finish();
    with(|spelling| {
        if let Some(found) = spelling.found.get(&key) {
            return Rc::clone(found);
        }
        if spelling.found.len() >= KEPT {
            spelling.found.clear();
        }
        let found: Underlined = Rc::new(spell::check_text(&text, &spelling.speller));
        spelling.found.insert(key, Rc::clone(&found));
        found
    })
    .unwrap_or_default()
}

/// Every underlined word in a document, in reading order. Each paragraph goes through the cache, so the
/// status line can count them on every frame.
pub fn misspellings(document: &Document) -> Vec<Misspelling> {
    let mut out = Vec::new();
    for story in spell::stories(document) {
        let Some(blocks) = unluminous_word::position::story(document, &story) else { continue };
        for (path, paragraph) in unluminous_word::position::paragraphs(blocks) {
            let found = underlined(paragraph);
            if found.is_empty() {
                continue;
            }
            let text = paragraph.flat_text();
            for (range, kind) in found.iter() {
                let word = text.get(range.clone()).unwrap_or_default().to_owned();
                out.push(Misspelling {
                    story: story.clone(),
                    path: path.clone(),
                    range: range.clone(),
                    word,
                    kind: *kind,
                });
            }
        }
    }
    out
}

/// Up to `most` spellings the dictionary suggests for a word.
pub fn suggestions(word: &str, most: usize) -> Vec<String> {
    with(|spelling| spelling.speller.suggestions(word, most)).unwrap_or_default()
}

/// Add a word to the dictionary and to `words.dic`, so it stays added.
pub fn add_word(word: &str) -> Result<(), String> {
    with(|spelling| {
        spelling.speller.add_word(word);
        spelling.found.clear();
    });
    let Some(file) = words_file() else { return Ok(()) };
    let mut words = own_words();
    if !words.iter().any(|w| w == word) {
        words.push(word.to_owned());
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|e| format!("{} could not be made: {e}", folder.display()))?;
    }
    let text = words.join("\n") + "\n";
    crate::services::store::write_atomically(&file, text.as_bytes())
        .map_err(|e| format!("{} could not be written: {e}", file.display()))
}

/// Stop underlining a word for the rest of the session, which is Word's Ignore All.
pub fn ignore(word: &str) {
    with(|spelling| {
        spelling.speller.ignore(word);
        spelling.found.clear();
    });
}

/// The underlined word a position is in, if it is in one.
pub fn underlined_at(doc: &super::Doc, at: &unluminous_word::position::Pos) -> Option<Misspelling> {
    let paragraph =
        unluminous_word::position::paragraph(&doc.session.document, &at.story, &at.path)?;
    let found = underlined(paragraph);
    let (range, kind) =
        found.iter().find(|(range, _)| range.start <= at.offset && at.offset <= range.end)?;
    let word = paragraph.flat_text().get(range.clone())?.to_owned();
    Some(Misspelling {
        story: at.story.clone(),
        path: at.path.clone(),
        range: range.clone(),
        word,
        kind: *kind,
    })
}

/// Select an underlined word and open its card with what the dictionary suggests.
pub fn open_menu(doc: &mut super::Doc, word: Misspelling) {
    let start = unluminous_word::position::Pos {
        story: word.story.clone(),
        path: word.path.clone(),
        offset: word.range.start,
    };
    doc.session.select(start.clone(), start.at(word.range.end));
    let suggestions = match word.kind {
        Kind::Spelling => suggestions(&word.word, 6),
        Kind::Repeated => Vec::new(),
    };
    doc.view.spell_menu = Some(super::SpellMenu { word, suggestions });
    doc.view.reveal = true;
}

/// F7: the next underlined word after the caret, from the start again after the last, with its card open.
pub fn next(doc: &mut super::Doc) -> Result<String, String> {
    let found = misspellings(&doc.session.document);
    let caret = doc.session.selection.ordered().1.clone();
    let after = |m: &&Misspelling| {
        m.story == caret.story
            && (m.path.as_slice(), m.range.start) > (caret.path.as_slice(), caret.offset)
    };
    // In another story, the body's words come first in reading order.
    let Some(word) = found.iter().find(after).or_else(|| found.first()).cloned() else {
        doc.view.spell_menu = None;
        return Ok("No spelling errors".to_owned());
    };
    open_menu(doc, word);
    Ok(String::new())
}

/// Whether the word a card was opened on is still there, unchanged; when it is not, the card closes.
pub fn menu_is_current(doc: &super::Doc) -> bool {
    let Some(menu) = &doc.view.spell_menu else { return false };
    let word = &menu.word;
    unluminous_word::position::paragraph(&doc.session.document, &word.story, &word.path)
        .is_some_and(|p| p.flat_text().get(word.range.clone()) == Some(word.word.as_str()))
}

/// Replace the card's word with a suggestion, keeping the word's formatting, and close the card.
pub fn replace(doc: &mut super::Doc, with: &str) {
    let Some(menu) = doc.view.spell_menu.take() else { return };
    let word = menu.word;
    let start = unluminous_word::position::Pos {
        story: word.story,
        path: word.path,
        offset: word.range.start,
    };
    doc.session.select(start.clone(), start.at(word.range.end));
    doc.session.type_text(with);
}

/// Delete a repeated word and the space before it, and close the card.
pub fn delete_repeated(doc: &mut super::Doc) {
    let Some(menu) = doc.view.spell_menu.take() else { return };
    let word = menu.word;
    let Some(paragraph) =
        unluminous_word::position::paragraph(&doc.session.document, &word.story, &word.path)
    else {
        return;
    };
    let text = paragraph.flat_text();
    let from = text[..word.range.start].trim_end_matches(' ').len();
    let start = unluminous_word::position::Pos { story: word.story, path: word.path, offset: from };
    doc.session.select(start.clone(), start.at(word.range.end));
    doc.session.backspace(unluminous_word::edit::Unit::Character);
}
