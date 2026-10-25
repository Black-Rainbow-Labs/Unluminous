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
        let mut our_page = starts.iter().find(|(p, _)| *p == path).map_or(0, |(_, page)| *page);
        // A cell that continues a merge down has no text of its own on the page; Word gives it the page of
        // the cell the merge starts in, which is the same cell a row or more above.
        if our_page == 0 && path.len() >= 4 {
            let row = path.len() - 3;
            for above in (0..path[row]).rev() {
                let mut up = path.clone();
                up[row] = above;
                if let Some((_, page)) = starts.iter().find(|(p, _)| **p == up) {
                    our_page = *page;
                    break;
                }
            }
        }
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

/// The visible text of `paragraph` between two offsets of its flat text: field codes, field characters and
/// hidden text left out, a footnote reference as character 2, which is how Word's line report gives it.
fn visible_between(
    paragraph: &Paragraph,
    resolver: &unluminous_word::styles::Resolver,
    from: usize,
    to: usize,
) -> String {
    use unluminous_word::model::Content;
    let mut out = String::new();
    let mut in_code: Vec<bool> = Vec::new();
    let mut at = 0;
    for run in &paragraph.runs {
        let start = at;
        at += run.len();
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
            continue;
        }
        if start >= to || at <= from && start < from {
            continue;
        }
        if in_code.iter().any(|c| *c) || matches!(run.content, Content::Instr(_)) {
            continue;
        }
        if resolver.run(&paragraph.props, &run.props, None).hidden {
            continue;
        }
        match &run.content {
            Content::Text(text) => {
                let lo = from.saturating_sub(start).min(text.len());
                let hi = (to - start).min(text.len());
                if let Some(piece) = text.get(lo..hi) {
                    out.push_str(piece);
                }
            }
            Content::Object(el)
                if el.name == "w:footnoteReference" || el.name == "w:endnoteReference" =>
            {
                out.push('\u{2}')
            }
            // Word's `Range.Text` gives a positional tab as 0 and an inline picture as `/`.
            Content::Object(el) if el.name == "w:ptab" => out.push('0'),
            Content::Object(_)
                if run.content.picture().is_some_and(|p| {
                    p.elements().next().is_some_and(|f| f.name != "wp:anchor")
                }) =>
            {
                out.push('/')
            }
            Content::ParagraphObject(el) if el.name.starts_with("m:") => {
                out.push_str(&unluminous_word::math::plain_text(el))
            }
            other => out.extend(other.character().filter(|c| *c != '\u{FFFC}')),
        }
    }
    out
}

/// Two paragraphs' lines are the same when each line holds the same words. A word hyphenated at a
/// different point is the same: Word's hyphenation dictionary is its own and splits a few words in a
/// different place from the en-US patterns (par-agraph against para-graph), and what matters is that
/// the lines break around the same words.
fn same_lines(ours: &[String], theirs: &[String]) -> bool {
    let joined = |lines: &[String]| lines.join("").replace(' ', "");
    if joined(ours) != joined(theirs) {
        return false;
    }
    // A line that differs only in its last word (a word split elsewhere) or its first (the rest of it).
    let head = |line: &str| line.rsplit_once(' ').map(|(h, _)| h.to_owned());
    let tail = |line: &str| line.split_once(' ').map(|(_, t)| t.to_owned());
    ours.iter().zip(theirs).all(|(a, b)| {
        a == b
            || (head(a).is_some() && head(a) == head(b))
            || (tail(a).is_some() && tail(a) == tail(b))
    })
}

/// A line's text for comparing with Word's: no column break mark, an equation's letters and minus sign
/// as plain characters, in lower case because Word reports capitals as they are drawn.
fn comparable(text: &str) -> String {
    text.chars()
        .filter(|c| *c != '\u{e}' && *c != '\r')
        .map(|c| match c as u32 {
            0x2212 => '-',
            0x210E => 'h',
            0x1D400..=0x1D6A3 => {
                let i = ((c as u32 - 0x1D400) % 52) as u8;
                if i < 26 {
                    (b'A' + i) as char
                } else {
                    (b'a' + i - 26) as char
                }
            }
            0x1D6A8..=0x1D7C9 => {
                let i = (c as u32 - 0x1D6A8) % 58;
                match i {
                    26..=50 => char::from_u32(0x3B1 + i - 26).unwrap_or(c),
                    _ => c,
                }
            }
            0x1D7CE..=0x1D7FF => (b'0' + ((c as u32 - 0x1D7CE) % 10) as u8) as char,
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// Every line of every body paragraph, against the lines Word's report gives for it (`inspect.ps1` reads
/// them with the `\Line` bookmark): the same words on each line, and each line starting where Word's does,
/// which is what wrapping around a picture changes (task-2359).
#[test]
fn every_line_holds_the_words_words_line_holds() {
    let fonts = common::RealFonts::new();
    if !fonts_are_installed(&fonts) {
        return;
    }
    let mut failures = Vec::new();
    for name in fixture_names() {
        let report = report(&name);
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        if !words.iter().any(|p| p["lines"].is_array()) {
            continue;
        }
        let path = ["docx", "dotx", "docm"]
            .iter()
            .map(|e| folder().join(format!("{name}.{e}")))
            .find(|p| p.is_file())
            .unwrap();
        let document = unluminous_word::read::open(&std::fs::read(path).unwrap()).unwrap();
        let resolver = unluminous_word::styles::Resolver::new(&document);
        let pages = lay_out(&document, &fonts, Options::default());
        let mut wrong = 0;
        for ((path, paragraph), word) in word_order(&document.body).iter().zip(&words) {
            let (Some(paragraph), Some(word_lines)) = (paragraph, word["lines"].as_array()) else {
                continue;
            };
            if word["inTable"].as_bool() == Some(true) {
                continue;
            }
            // Word works a date field out again when it opens the file, so its line holds the day it was
            // inspected rather than the stored one.
            let dated = paragraph.runs.iter().any(|r| {
                matches!(&r.content, unluminous_word::model::Content::Instr(code)
                    if matches!(code.split_whitespace().next(), Some("DATE" | "TIME")))
            });
            if dated {
                continue;
            }
            // Word gives an empty paragraph no lines.
            if word_lines.is_empty() && paragraph.visible_text().trim().is_empty() {
                continue;
            }
            // Word's `left` is where the line's first character is for a left aligned paragraph with no
            // indent and no list label; for any other it reports the margin, so only those are placed.
            let placed =
                !paragraph.runs.iter().any(|r| {
                    matches!(r.content, unluminous_word::model::Content::ParagraphObject(_))
                }) && word["alignment"].as_i64() == Some(0)
                    && word["leftIndent"].as_f64() == Some(0.0)
                    && word["firstLineIndent"].as_f64() == Some(0.0)
                    && word["listString"].as_str().unwrap_or("").is_empty();
            let ours: Vec<(String, f32)> = pages
                .pages
                .iter()
                .flat_map(|p| p.carets.iter())
                .filter(|l| l.story == unluminous_word::position::StoryId::Body && l.path == *path)
                .map(|l| {
                    let text = visible_between(paragraph, &resolver, l.start, l.end);
                    let left = l.stops.first().map_or(0.0, |s| s.1);
                    (text.trim().to_owned(), left)
                })
                .collect();
            let theirs: Vec<(String, f32)> = word_lines
                .iter()
                .map(|l| {
                    let text = l["text"].as_str().unwrap_or("").replace('\u{b}', "");
                    (text.trim().to_owned(), l["left"].as_f64().unwrap_or(0.0) as f32)
                })
                .collect();
            let ours_text: Vec<String> = ours.iter().map(|(t, _)| comparable(t)).collect();
            let their_text: Vec<String> = theirs.iter().map(|(t, _)| comparable(t)).collect();
            let same_words = ours.len() == theirs.len() && same_lines(&ours_text, &their_text);
            let same_place = !placed
                || ours.len() == theirs.len()
                    && ours.iter().zip(&theirs).all(|(a, b)| (a.1 - b.1).abs() < 1.5);
            if !same_words || !same_place {
                wrong += 1;
                if wrong <= 3 {
                    failures.push(format!(
                        "{name} {path:?}:\n  ours {:?}\n  Word {:?}",
                        ours.iter().map(|(t, x)| format!("{x:.1} {t}")).collect::<Vec<_>>(),
                        theirs.iter().map(|(t, x)| format!("{x:.1} {t}")).collect::<Vec<_>>()
                    ));
                }
            }
        }
        if wrong > 3 {
            failures.push(format!("{name}: {wrong} paragraphs in all"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
