//! Where Unluminous breaks pages, against where Word broke them.
//!
//! Word's report gives the page each paragraph starts on. Unluminous lays the same document out with the
//! same fonts and the page each paragraph starts on is compared, for every paragraph of every fixture,
//! table cells included, with no allowance: a paragraph on another page than Word's is a failure
//! (task-2359). The page count has to be equal too.
//!
//! The fonts are the machine's own. A machine without Aptos, the font of every new document, cannot run
//! this test; it says so, and in the release gate (`UNLUMINOUS_REQUIRE_WORD=1`) it fails instead.

mod common;

use std::path::{Path, PathBuf};

use serde_json::Value;
use unluminous_word::layout::{lay_out, Options};
use unluminous_word::model::{Block, Paragraph};

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn report(name: &str) -> Value {
    let path = folder().join(format!("{name}.word.json"));
    serde_json::from_str(std::fs::read_to_string(path).unwrap().trim_start_matches('\u{feff}'))
        .unwrap()
}

/// Every fixture with a Word report, by name.
fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder())
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx" || e == "dotx" || e == "docm"))
        .filter(|path| path.with_extension("word.json").exists())
        .map(|path| path.file_stem().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// The paragraphs in the order Word's `Document.Paragraphs` lists them, with a `None` for the end mark
/// of each table row, which Word lists as a paragraph of its own.
fn word_order(blocks: &[Block]) -> Vec<(Vec<u32>, Option<&Paragraph>)> {
    fn walk<'a>(
        blocks: &'a [Block],
        prefix: &mut Vec<u32>,
        out: &mut Vec<(Vec<u32>, Option<&'a Paragraph>)>,
    ) {
        for (index, block) in blocks.iter().enumerate() {
            prefix.push(index as u32);
            match block {
                Block::Paragraph(_) => out.push((prefix.clone(), block.as_paragraph())),
                Block::Table(table) => {
                    for (r, row) in table.rows.iter().enumerate() {
                        for (c, cell) in row.cells.iter().enumerate() {
                            prefix.extend([r as u32, c as u32]);
                            walk(&cell.blocks, prefix, out);
                            prefix.truncate(prefix.len() - 2);
                        }
                        out.push((prefix.clone(), None));
                    }
                }
                Block::Sdt(sdt) => walk(&sdt.blocks, prefix, out),
                Block::Kept(_) => {}
            }
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(blocks, &mut Vec::new(), &mut out);
    out
}

/// Our page count, and every paragraph Word lists with the page it starts on in our layout and in Word's,
/// as lines saying where they differ.
fn compare(name: &str, fonts: &common::RealFonts) -> (usize, usize, usize, Vec<String>) {
    let report = report(name);
    let path = ["docx", "dotx", "docm"]
        .iter()
        .map(|e| folder().join(format!("{name}.{e}")))
        .find(|p| p.is_file())
        .unwrap();
    let document = unluminous_word::read::open(&std::fs::read(path).unwrap()).unwrap();
    let pages = lay_out(&document, fonts, Options::default());
    let mut starts: Vec<(&Vec<u32>, usize)> = Vec::new();
    let mut tops: Vec<f32> = Vec::new();
    for (index, page) in pages.pages.iter().enumerate() {
        for line in &page.carets {
            if line.story == unluminous_word::position::StoryId::Body
                && !starts.iter().any(|(p, _)| **p == line.path)
            {
                starts.push((&line.path, index + 1));
                tops.push(line.top);
            }
        }
    }
    // `PAGINATION_DUMP=<fixture>` prints where each paragraph starts in both layouts.
    if std::env::var("PAGINATION_DUMP").is_ok_and(|d| d == name) {
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        for ((path, _), word) in word_order(&document.body).iter().zip(&words) {
            let at = starts.iter().position(|(p, _)| *p == path);
            let (page, top) = at.map_or((0, 0.0), |i| (starts[i].1, tops[i]));
            println!(
                "{path:?} ours {page} {top:.2} Word {} {} {:?}",
                word["page"],
                word["top"],
                word["text"].as_str().unwrap_or("").chars().take(30).collect::<String>()
            );
        }
    }
    let order = word_order(&document.body);
    let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
    let mut misses = Vec::new();
    if order.len() != words.len() {
        misses.push(format!("{} paragraphs against Word's {}", order.len(), words.len()));
    }
    let mut compared = 0;
    for ((path, paragraph), word) in order.iter().zip(&words) {
        if paragraph.is_none() {
            continue;
        }
        compared += 1;
        let word_page = word["page"].as_u64().unwrap_or(0) as usize;
        let our_page = starts.iter().find(|(p, _)| *p == path).map_or(0, |(_, page)| *page);
        if word_page != our_page {
            misses.push(format!(
                "{path:?} {:?}: ours {our_page}, Word {word_page}",
                word["text"].as_str().unwrap_or("").chars().take(40).collect::<String>()
            ));
        }
    }
    (pages.pages.len(), report["pages"].as_u64().unwrap_or(0) as usize, compared, misses)
}

/// Whether the machine has the fonts the fixtures are set in, failing in the release gate when not.
fn fonts_are_installed(fonts: &common::RealFonts) -> bool {
    let missing: Vec<&str> = ["Aptos", "Calibri", "Times New Roman", "Arial", "Cambria Math"]
        .into_iter()
        .filter(|family| !fonts.has(family))
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var("UNLUMINOUS_REQUIRE_WORD").is_err(),
        "the release gate needs {missing:?} installed to compare pages with Word's"
    );
    false
}

#[test]
fn every_paragraph_of_every_fixture_starts_on_words_page() {
    let fonts = common::RealFonts::new();
    if !fonts_are_installed(&fonts) {
        return;
    }
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    for name in fixture_names() {
        let (ours, words, compared, misses) = compare(&name, &fonts);
        lines.push(format!(
            "{name}: {ours} pages against Word's {words}; {} of {compared} paragraphs start on Word's page",
            compared.saturating_sub(misses.len())
        ));
        let shown = if std::env::var("PAGINATION_ALL").is_ok() { usize::MAX } else { 6 };
        for miss in misses.iter().take(shown) {
            lines.push(format!("    {miss}"));
        }
        if ours != words || !misses.is_empty() {
            failed.push(name);
        }
    }
    println!("{}", lines.join("\n"));
    assert!(failed.is_empty(), "differ from Word: {failed:?}\n{}", lines.join("\n"));
}

/// One paragraph set in Calibri, Times New Roman, Arial and Aptos, and the lines Word broke each into,
/// which `fonts-doc.ps1` read off Word with `Information(wdFirstCharacterLineNumber)`. Every face's lines
/// are Word's to the word.
#[test]
fn lines_break_where_word_breaks_them_in_installed_fonts() {
    let fonts = common::RealFonts::new();
    if !fonts_are_installed(&fonts) {
        return;
    }
    let document =
        unluminous_word::read::open(&std::fs::read(folder().join("line-breaks.docx")).unwrap())
            .unwrap();
    let expected = std::fs::read_to_string(folder().join("line-breaks.word-lines.txt")).unwrap();
    let mut words: Vec<(String, Vec<String>)> = Vec::new();
    for line in expected.lines() {
        if let Some(font) = line.strip_prefix("== ") {
            words.push((font.trim().to_owned(), Vec::new()));
        } else if let Some((_, lines)) = words.last_mut() {
            lines.push(line.trim().to_owned());
        }
    }
    let pages = lay_out(&document, &fonts, Options::default());
    let mut failures = Vec::new();
    for ((path, paragraph), (font, word_lines)) in
        unluminous_word::position::paragraphs(&document.body).into_iter().zip(&words)
    {
        let text = paragraph.flat_text();
        let ours: Vec<String> = pages
            .pages
            .iter()
            .flat_map(|p| p.carets.iter())
            .filter(|l| l.path == path)
            .map(|l| text[l.start..l.end.min(text.len())].trim().to_owned())
            .collect();
        if &ours != word_lines {
            failures.push(format!("{font}:\n  ours {ours:#?}\n  Word {word_lines:#?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
