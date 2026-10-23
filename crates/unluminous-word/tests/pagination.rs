//! Where Unluminous breaks pages, against where Word broke them.
//!
//! Word's report gives the page each paragraph starts on. Unluminous lays the same document out with the
//! same fonts and the page each paragraph starts on is compared. Fonts differ by fractions of a point in
//! how two programs measure them, so a paragraph near the bottom of a page can fall either way; the
//! test asks for the same page count and for nearly every paragraph to start on Word's page.

mod common;

use std::path::Path;

use serde_json::Value;
use unluminous_word::layout::{lay_out, Options};

fn report(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.word.json"));
    serde_json::from_str(std::fs::read_to_string(path).unwrap().trim_start_matches('\u{feff}')).unwrap()
}

/// The page each body paragraph starts on, in Unluminous's layout, in document order.
fn our_pages(name: &str, fonts: &common::RealFonts) -> (usize, Vec<(Vec<u32>, usize)>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(path).unwrap()).unwrap();
    let pages = lay_out(&document, fonts, Options::default());
    let mut starts: Vec<(Vec<u32>, usize)> = Vec::new();
    for (index, page) in pages.pages.iter().enumerate() {
        for line in &page.carets {
            if line.story == unluminous_word::position::StoryId::Body && !starts.iter().any(|(p, _)| *p == line.path) {
                starts.push((line.path.clone(), index + 1));
            }
        }
    }
    (pages.pages.len(), starts)
}

fn compare(name: &str) -> (usize, usize, usize, usize, Vec<String>) {
    let fonts = common::RealFonts::new();
    let report = report(name);
    let (count, starts) = our_pages(name, &fonts);
    let document_paths: Vec<Vec<u32>> = {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.docx"));
        let document = unluminous_word::read::open(&std::fs::read(path).unwrap()).unwrap();
        unluminous_word::position::paragraphs(&document.body).into_iter().map(|(p, _)| p).collect()
    };
    let words: Vec<&Value> = report["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| !p["inTable"].as_bool().unwrap_or(false))
        .collect();
    let ours: Vec<&(Vec<u32>, usize)> = starts.iter().filter(|(p, _)| p.len() == 1).collect();
    let mut same = 0;
    let mut misses = Vec::new();
    let top_level: Vec<&Vec<u32>> = document_paths.iter().filter(|p| p.len() == 1).collect();
    for (path, word) in top_level.iter().zip(&words) {
        let word_page = word["page"].as_u64().unwrap_or(0) as usize;
        let our_page = ours.iter().find(|(p, _)| p == *path).map_or(0, |(_, page)| *page);
        if word_page == our_page {
            same += 1;
        } else {
            misses.push(format!("{:?} {:?}: ours {our_page}, Word {word_page}", path, word["text"].as_str().unwrap_or("").chars().take(40).collect::<String>()));
        }
    }
    (count, report["pages"].as_u64().unwrap_or(0) as usize, same, top_level.len().min(words.len()), misses)
}

#[test]
fn page_counts_and_breaks_are_close_to_words() {
    if !common::RealFonts::new().has("Aptos") {
        return;
    }
    let mut lines = Vec::new();
    let mut failed = false;
    for name in ["long", "headers", "kitchen-sink", "paragraphs", "styles", "lists", "notes", "sections", "tables", "formatting", "fields"] {
        let (ours, words, same, total, misses) = compare(name);
        lines.push(format!("{name}: {ours} pages against Word's {words}; {same} of {total} paragraphs start on Word's page"));
        for miss in misses.iter().take(4) {
            lines.push(format!("    {miss}"));
        }
        if ours != words || (total > 0 && (same as f32) < total as f32 * 0.8) {
            failed = true;
        }
    }
    println!("{}", lines.join("\n"));
    assert!(!failed, "\n{}", lines.join("\n"));
}

/// One paragraph set in Calibri, Times New Roman, Arial and Aptos, and the lines Word broke each into,
/// which `_agent_output`'s `fonts-doc.ps1` read off Word with `Information(wdFirstCharacterLineNumber)`.
///
/// **Where a face is installed as a font file, the lines are Word's to the word.** Aptos is the exception
/// and is left out: Word lays it out about three percent narrower than the `Aptos` Office downloads into
/// its font cache measures, so Word fits a word or two more on some of its lines. Every other number this
/// file compares for Aptos documents (where each page starts) is still checked above.
#[test]
fn lines_break_where_word_breaks_them_in_installed_fonts() {
    let fonts = common::RealFonts::new();
    if !fonts.has("Calibri") || !fonts.has("Times New Roman") || !fonts.has("Arial") {
        return;
    }
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let document = unluminous_word::read::open(&std::fs::read(folder.join("line-breaks.docx")).unwrap()).unwrap();
    let expected = std::fs::read_to_string(folder.join("line-breaks.word-lines.txt")).unwrap();
    let mut words: Vec<(String, Vec<String>)> = Vec::new();
    for line in expected.lines() {
        if let Some(font) = line.strip_prefix("== ") {
            words.push((font.trim().to_owned(), Vec::new()));
        } else if let Some((_, lines)) = words.last_mut() {
            lines.push(line.trim().to_owned());
        }
    }
    let pages = lay_out(&document, &fonts, Options::default());
    for ((path, paragraph), (font, word_lines)) in unluminous_word::position::paragraphs(&document.body).into_iter().zip(&words) {
        if font == "Aptos" {
            continue;
        }
        let text = paragraph.flat_text();
        let ours: Vec<String> = pages.pages.iter().flat_map(|p| p.carets.iter()).filter(|l| l.path == path).map(|l| text[l.start..l.end.min(text.len())].trim().to_owned()).collect();
        assert_eq!(&ours, word_lines, "{font}");
    }
}
