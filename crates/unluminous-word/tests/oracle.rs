//! What Unluminous reads out of a document Word wrote, against what Word itself reports about it.
//!
//! Each fixture in `tests/fixtures` was made by Word through `tools/word-oracle/make-fixtures.ps1`, and the
//! `.word.json` beside it is Word's own report (`inspect.ps1`). These tests need no Word: the reports are
//! committed.

use std::path::{Path, PathBuf};

use serde_json::Value;

mod common;
use unluminous_word::styles::Resolver;

fn fixtures() -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx"))
        .collect();
    files.sort();
    files
}

fn report(docx: &Path) -> Value {
    let text = std::fs::read_to_string(docx.with_extension("word.json")).unwrap();
    serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
}

/// Word's text for a paragraph: its `Range.Text` with the paragraph mark taken off, and Word's own
/// characters for a line break (11) and a page break (12) as Unluminous's flat text has them.
/// The paragraphs in the order Word's `Document.Paragraphs` lists them, which has one more after the last
/// cell of every table row: the row's end mark, given here as `None`.
fn word_order(
    blocks: &[unluminous_word::model::Block],
) -> Vec<(Vec<u32>, Option<&unluminous_word::model::Paragraph>)> {
    use unluminous_word::model::Block;
    let mut out = Vec::new();
    fn walk<'a>(
        blocks: &'a [Block],
        prefix: &mut Vec<u32>,
        out: &mut Vec<(Vec<u32>, Option<&'a unluminous_word::model::Paragraph>)>,
    ) {
        for (index, block) in blocks.iter().enumerate() {
            prefix.push(index as u32);
            match block {
                Block::Paragraph(p) => out.push((prefix.clone(), Some(p))),
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
    walk(blocks, &mut Vec::new(), &mut out);
    out
}

fn words_text(paragraph: &Value) -> String {
    paragraph["text"]
        .as_str()
        .unwrap_or_default()
        .replace(['\u{7}'], "")
        .trim_end_matches('\r')
        .to_owned()
}

/// A paragraph's text the way Word's `Range.Text` gives it: field codes left out, deleted text kept, hidden
/// text left out, a note reference as character 2, a picture as `/`, and a section break at the end of the
/// paragraph that ends a section.
fn ours(paragraph: &unluminous_word::model::Paragraph, resolver: &Resolver) -> String {
    ours_on_page(paragraph, resolver, None)
}

/// `ours`, with `PAGE` and `NUMPAGES` worked out for page `page` of `pages` the way the layout works
/// them out for a header, rather than the result the file stored.
fn ours_on_page(
    paragraph: &unluminous_word::model::Paragraph,
    resolver: &Resolver,
    page: Option<(u32, u32)>,
) -> String {
    use unluminous_word::layout::fields::page_result;
    use unluminous_word::model::Content;
    let mut out = String::new();
    // Each open field: whether its code is still being read, its code, and whether a worked out result
    // replaces the stored one.
    let mut in_code: Vec<(bool, String, bool)> = Vec::new();
    for run in &paragraph.runs {
        if let Content::FieldChar(el) = &run.content {
            match el.attr("w:fldCharType") {
                Some("begin") => in_code.push((true, String::new(), false)),
                Some("separate") => {
                    if let Some(last) = in_code.last_mut() {
                        last.0 = false;
                        if let Some(value) = page_result(&last.1, page) {
                            out.push_str(&value);
                            last.2 = true;
                        }
                    }
                }
                _ => {
                    in_code.pop();
                }
            }
            continue;
        }
        if let Content::Instr(code) = &run.content {
            if let Some(last) = in_code.last_mut() {
                last.1.push_str(code);
            }
            continue;
        }
        if in_code.iter().any(|c| c.0 || c.2) {
            continue;
        }
        if let Some(simple) = run.wraps.iter().find(|w| w.el.name == "w:fldSimple") {
            if let Some(value) = simple.el.attr("w:instr").and_then(|c| page_result(c, page)) {
                if !out.ends_with(&value) {
                    out.push_str(&value);
                }
                continue;
            }
        }
        if resolver.run(&paragraph.props, &run.props, None).hidden {
            continue;
        }
        match &run.content {
            Content::Text(text) => out.push_str(text),
            Content::Object(el)
                if el.name == "w:footnoteReference" || el.name == "w:endnoteReference" =>
            {
                out.push('\u{2}')
            }
            Content::Object(_)
                if run.content.picture().is_some_and(|p| {
                    p.elements().next().is_some_and(|f| f.name != "wp:anchor")
                }) =>
            {
                out.push('/')
            }
            Content::Object(el)
                if matches!(el.name.as_str(), "w:commentReference" | "w:annotationRef") => {}
            Content::Object(_) if run.content.picture().is_some() => {}
            other => out.extend(other.character().filter(|c| *c != '\u{FFFC}')),
        }
    }
    if paragraph.props.child("w:sectPr").is_some() {
        out.push('\u{c}');
    }
    out
}

#[test]
fn every_paragraphs_text_and_style_are_what_word_reports() {
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let ours_list = word_order(&document.body);
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        if ours_list.len() != words.len() {
            failures.push(format!(
                "{}: {} paragraphs against Word's {}",
                file.display(),
                ours_list.len(),
                words.len()
            ));
            continue;
        }
        for ((_, paragraph), word) in ours_list.iter().zip(&words) {
            let index = word["index"].as_i64().unwrap_or(0);
            let Some(paragraph) = paragraph else {
                if !words_text(word).is_empty() {
                    failures.push(format!(
                        "{} paragraph {index}: a row end against Word's {:?}",
                        file.display(),
                        words_text(word)
                    ));
                }
                continue;
            };
            let text = ours(paragraph, &resolver);
            // Word reports text in all capitals as it is drawn.
            if text.to_lowercase() != words_text(word).to_lowercase() {
                failures.push(format!(
                    "{} paragraph {index}: {text:?} against Word's {:?}",
                    file.display(),
                    words_text(word)
                ));
            }
            let style_id = resolver.paragraph_style_id(&paragraph.props).unwrap_or("Normal");
            let style = resolver.name_of(style_id);
            if Some(style.as_str()) != word["style"].as_str() {
                failures.push(format!(
                    "{} paragraph {index}: style {style:?} against Word's {:?}",
                    file.display(),
                    word["style"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_paragraphs_indents_spacing_and_alignment_are_what_word_reports() {
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let ours_list = word_order(&document.body);
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        if ours_list.len() != words.len() {
            failures.push(format!(
                "{}: {} paragraphs against Word's {}",
                file.display(),
                ours_list.len(),
                words.len()
            ));
            continue;
        }
        for ((path, paragraph), word) in ours_list.iter().zip(&words) {
            let index = word["index"].as_i64().unwrap_or(0);
            let Some(paragraph) = paragraph else { continue };
            let table_style = unluminous_word::position::cell_of(&document.body, path).and_then(
                |(table, _, _)| match unluminous_word::position::block(&document.body, &table) {
                    Some(unluminous_word::model::Block::Table(t)) => {
                        resolver.table_style_id(&t.props)
                    }
                    _ => None,
                },
            );
            let look = resolver.paragraph(&paragraph.props, table_style);
            let alignment = match look.align {
                unluminous_word::props::Align::Left => 0,
                unluminous_word::props::Align::Center => 1,
                unluminous_word::props::Align::Right => 2,
                unluminous_word::props::Align::Justify => 3,
            };
            let close =
                |a: f32, b: &Value| (a - b.as_f64().unwrap_or(f64::NAN) as f32).abs() < 0.11;
            let checks = [
                ("alignment", word["alignment"].as_i64() == Some(alignment)),
                ("leftIndent", close(look.indent_left, &word["leftIndent"])),
                ("rightIndent", close(look.indent_right, &word["rightIndent"])),
                ("firstLineIndent", close(look.indent_first, &word["firstLineIndent"])),
                ("spaceBefore", look.before_auto || close(look.space_before, &word["spaceBefore"])),
                ("spaceAfter", look.after_auto || close(look.space_after, &word["spaceAfter"])),
                ("keepWithNext", word["keepWithNext"].as_bool() == Some(look.keep_next)),
                (
                    "pageBreakBefore",
                    word["pageBreakBefore"].as_bool() == Some(look.page_break_before),
                ),
            ];
            for (name, ok) in checks {
                if !ok {
                    failures.push(format!(
                        "{} paragraph {index} {:?}: {name} ours {look:?} Word {}",
                        file.display(),
                        words_text(word),
                        word[name]
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

// ---- Everything else the reports hold (task-2359) ----

/// Every character of a paragraph the way `ours` gives its text, each with the formatting it is drawn in.
fn our_characters(
    paragraph: &unluminous_word::model::Paragraph,
    resolver: &Resolver,
    table_style: Option<&str>,
) -> Vec<(char, unluminous_word::props::RunLook)> {
    use unluminous_word::model::Content;
    let mut out = Vec::new();
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
            continue;
        }
        if in_code.iter().any(|c| *c) || matches!(run.content, Content::Instr(_)) {
            continue;
        }
        let look = resolver.run(&paragraph.props, &run.props, table_style);
        if look.hidden {
            continue;
        }
        let mut one = |c: char| out.push((c, look.clone()));
        match &run.content {
            Content::Text(text) => text.chars().for_each(&mut one),
            Content::Object(el)
                if el.name == "w:footnoteReference" || el.name == "w:endnoteReference" =>
            {
                one('\u{2}')
            }
            Content::Object(_)
                if run.content.picture().is_some_and(|p| {
                    p.elements().next().is_some_and(|f| f.name != "wp:anchor")
                }) =>
            {
                one('/')
            }
            Content::Object(el)
                if matches!(el.name.as_str(), "w:commentReference" | "w:annotationRef") => {}
            Content::Object(_) if run.content.picture().is_some() => {}
            other => other.character().filter(|c| *c != '\u{FFFC}').into_iter().for_each(one),
        }
    }
    out
}

/// Whether two colours are the same. `inspect.ps1` works a theme colour out again from Word's
/// `TintAndShade`, which Word gives to two decimals, so its answer can be one unit a channel from the
/// colour Word itself wrote beside the theme name (`w:val`); one unit is allowed for that.
fn same_colour(
    word: Option<unluminous_word::props::Rgb>,
    ours: Option<unluminous_word::props::Rgb>,
) -> bool {
    match (word, ours) {
        (Some(a), Some(b)) => {
            let near = |x: u8, y: u8| x.abs_diff(y) <= 1;
            near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2)
        }
        (a, b) => a == b,
    }
}

/// Word's `WdUnderline` as the kind Unluminous draws.
fn underline_kind(word: i64) -> unluminous_word::props::Underline {
    use unluminous_word::props::Underline;
    match word {
        0 => Underline::None,
        2 => Underline::Words,
        3 | 43 => Underline::Double,
        6 | 20 | 23 | 25 | 26 | 27 | 55 => Underline::Thick,
        4 => Underline::Dotted,
        7 | 39 => Underline::Dash,
        9 | 10 => Underline::DotDash,
        11 => Underline::Wave,
        _ => Underline::Single,
    }
}

/// Word's `WdColorIndex` for a highlight, as the name `w:highlight` writes.
fn highlight_name(index: i64) -> Option<&'static str> {
    Some(match index {
        1 => "black",
        2 => "blue",
        3 => "cyan",
        4 => "green",
        5 => "magenta",
        6 => "red",
        7 => "yellow",
        8 => "white",
        9 => "darkBlue",
        10 => "darkCyan",
        11 => "darkGreen",
        12 => "darkMagenta",
        13 => "darkRed",
        14 => "darkYellow",
        15 => "darkGray",
        16 => "lightGray",
        _ => return None,
    })
}

/// The table style a paragraph at `path` is inside, which its runs inherit from.
fn table_style_at<'a>(
    document: &'a unluminous_word::model::Document,
    resolver: &Resolver<'a>,
    path: &[u32],
) -> Option<&'a str> {
    unluminous_word::position::cell_of(&document.body, path).and_then(|(table, _, _)| {
        match unluminous_word::position::block(&document.body, &table) {
            Some(unluminous_word::model::Block::Table(t)) => resolver.table_style_id(&t.props),
            _ => None,
        }
    })
}

#[test]
fn every_runs_formatting_is_what_word_reports() {
    use unluminous_word::props::{Rgb, VertAlign};
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let ours_list = word_order(&document.body);
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        if ours_list.len() != words.len() {
            continue;
        }
        for ((path, paragraph), word) in ours_list.iter().zip(&words) {
            let (Some(paragraph), Some(runs)) = (paragraph, word["runs"].as_array()) else {
                continue;
            };
            let index = word["index"].as_i64().unwrap_or(0);
            let style = table_style_at(&document, &resolver, path);
            let characters = our_characters(paragraph, &resolver, style);
            let mut theirs: Vec<(char, &Value)> = Vec::new();
            for run in runs {
                for c in run["text"].as_str().unwrap_or("").chars() {
                    theirs.push((c, run));
                }
            }
            theirs.retain(|(c, _)| *c != '\u{7}' && *c != '\r');
            // A paragraph whose text differs is the text test's business.
            if theirs.len() != characters.len() {
                continue;
            }
            let mut seen: Vec<&str> = Vec::new();
            for ((c, ours), (_, word_run)) in characters.iter().zip(&theirs) {
                let colour = match word_run["color"].as_str() {
                    Some("auto") | None => None,
                    Some(hex) => Rgb::parse(hex),
                };
                let highlight = word_run["highlight"]
                    .as_i64()
                    .and_then(highlight_name)
                    .and_then(unluminous_word::props::highlight_colour);
                let size = word_run["size"].as_f64().unwrap_or(0.0) as f32;
                let underline = underline_kind(word_run["underline"].as_i64().unwrap_or(0));
                let checks = [
                    ("font", word_run["font"].as_str() == Some(&*ours.font)),
                    ("size", (size - ours.size).abs() < 0.01),
                    ("bold", word_run["bold"].as_bool() == Some(ours.bold)),
                    ("italic", word_run["italic"].as_bool() == Some(ours.italic)),
                    ("underline", underline == ours.underline),
                    ("color", same_colour(colour, ours.color)),
                    ("highlight", highlight == ours.highlight),
                    ("strike", word_run["strike"].as_bool() == Some(ours.strike)),
                    ("doubleStrike", word_run["doubleStrike"].as_bool() == Some(ours.double_strike)),
                    (
                        "superscript",
                        word_run["superscript"].as_bool()
                            == Some(ours.vert == VertAlign::Superscript),
                    ),
                    (
                        "subscript",
                        word_run["subscript"].as_bool() == Some(ours.vert == VertAlign::Subscript),
                    ),
                    ("smallCaps", word_run["smallCaps"].as_bool() == Some(ours.small_caps)),
                    ("allCaps", word_run["allCaps"].as_bool() == Some(ours.caps)),
                ];
                for (key, ok) in checks {
                    if !ok && !seen.contains(&key) {
                        seen.push(key);
                        let ours_value = match key {
                            "font" => ours.font.to_string(),
                            "size" => ours.size.to_string(),
                            "color" => format!("{:?}", ours.color.map(Rgb::hex)),
                            "highlight" => format!("{:?}", ours.highlight.map(Rgb::hex)),
                            "underline" => ours.underline.name().to_owned(),
                            _ => "the other".to_owned(),
                        };
                        failures.push(format!(
                            "{name} paragraph {index} at {c:?}: {key} ours {ours_value} Word {}",
                            word_run[key]
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_list_label_is_what_word_reports() {
    use unluminous_word::numbering::{symbol_to_unicode, Counters};
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let ours_list = word_order(&document.body);
        let words = report["paragraphs"].as_array().cloned().unwrap_or_default();
        if ours_list.len() != words.len() {
            continue;
        }
        let mut counters = Counters::default();
        for ((path, paragraph), word) in ours_list.iter().zip(&words) {
            let Some(paragraph) = paragraph else { continue };
            let index = word["index"].as_i64().unwrap_or(0);
            let style = table_style_at(&document, &resolver, path);
            let look = resolver.paragraph(&paragraph.props, style);
            let (label, level) = match look.numbering {
                Some((num, level)) => match counters.next(&resolver.numbering, num, level) {
                    Some(label) => (label.text, i64::from(level) + 1),
                    None => (String::new(), 0),
                },
                None => (String::new(), 0),
            };
            let theirs: String =
                word["listString"].as_str().unwrap_or("").chars().map(symbol_to_unicode).collect();
            let their_level = word["listLevel"].as_i64().unwrap_or(0);
            if label != theirs || (!theirs.is_empty() && level != their_level) {
                failures.push(format!(
                    "{name} paragraph {index} {:?}: label {label:?} level {level} against Word's {theirs:?} level {their_level}",
                    words_text(word).chars().take(30).collect::<String>()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

/// The text Word gives for a run of blocks, paragraphs joined by a paragraph mark, normalised so two
/// readings compare.
fn story_text(blocks: &[unluminous_word::model::Block], resolver: &Resolver) -> String {
    story_text_on_page(blocks, resolver, None)
}

fn story_text_on_page(
    blocks: &[unluminous_word::model::Block],
    resolver: &Resolver,
    page: Option<(u32, u32)>,
) -> String {
    let mut out = Vec::new();
    for (_, paragraph) in word_order(blocks) {
        if let Some(paragraph) = paragraph {
            out.push(ours_on_page(paragraph, resolver, page));
        }
    }
    normalised(&out.join("\r"))
}

/// Text with the cell end marks and section breaks taken off, each line trimmed, empty lines left out
/// and lower case, which is how two readings of one story compare.
fn normalised(text: &str) -> String {
    text.replace(['\u{7}', '\u{c}'], "")
        .split('\r')
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\r")
        .to_lowercase()
}

#[test]
fn every_table_cell_is_what_word_reports() {
    use unluminous_word::model::{Block, Table};
    fn collect<'a>(blocks: &'a [Block], out: &mut Vec<&'a Table>) {
        for block in blocks {
            match block {
                Block::Table(table) => {
                    out.push(table);
                    for row in &table.rows {
                        for cell in &row.cells {
                            collect(&cell.blocks, out);
                        }
                    }
                }
                Block::Sdt(sdt) => collect(&sdt.blocks, out),
                _ => {}
            }
        }
    }
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let mut tables = Vec::new();
        collect(&document.body, &mut tables);
        let words = report["tables"].as_array().cloned().unwrap_or_default();
        if tables.len() != words.len() {
            failures.push(format!("{name}: {} tables against Word's {}", tables.len(), words.len()));
            continue;
        }
        for (t, (table, word)) in tables.iter().zip(&words).enumerate() {
            if word["rows"].as_u64() != Some(table.rows.len() as u64) {
                failures.push(format!(
                    "{name} table {t}: {} rows against Word's {}",
                    table.rows.len(),
                    word["rows"]
                ));
            }
            for cell in word["cells"].as_array().into_iter().flatten() {
                let r = cell["row"].as_u64().unwrap_or(0) as usize;
                let c = cell["column"].as_u64().unwrap_or(0) as usize;
                let found = table
                    .rows
                    .get(r.wrapping_sub(1))
                    .and_then(|row| row.cells.get(c.wrapping_sub(1)));
                let Some(ours) = found else {
                    failures.push(format!("{name} table {t}: no cell at row {r} column {c}"));
                    continue;
                };
                let text = story_text(&ours.blocks, &resolver);
                let theirs = normalised(cell["text"].as_str().unwrap_or(""));
                if text != theirs {
                    failures.push(format!(
                        "{name} table {t} cell {r},{c}: {text:?} against Word's {theirs:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_header_and_footer_is_what_word_reports() {
    use std::collections::HashMap;
    use unluminous_word::model::Block;
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        // Word's header text has `PAGE` as 1 and `NUMPAGES` as the page count, so ours is worked out
        // with the page count this layout gives.
        let pages = unluminous_word::layout::lay_out(
            &document,
            &common::RealFonts::new(),
            unluminous_word::layout::Options::default(),
        )
        .pages
        .len() as u32;
        let mut sects: Vec<&unluminous_word::xml::El> = document
            .body
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => p.props.child("w:sectPr"),
                _ => None,
            })
            .collect();
        sects.push(&document.section);
        let words = report["sections"].as_array().cloned().unwrap_or_default();
        if words.len() != sects.len() {
            failures.push(format!(
                "{name}: {} sections against Word's {}",
                sects.len(),
                words.len()
            ));
            continue;
        }
        let even_odd = document
            .settings
            .as_ref()
            .and_then(|s| unluminous_word::props::on_off(&s.root, "w:evenAndOddHeaders"))
            .unwrap_or(false);
        // A section that names no header of a kind has the previous section's.
        let mut inherited: [HashMap<String, String>; 2] = Default::default();
        for (s, (sect, word)) in sects.iter().zip(&words).enumerate() {
            let title = unluminous_word::props::on_off(sect, "w:titlePg") == Some(true);
            let parts = [("w:headerReference", "headers"), ("w:footerReference", "footers")];
            for (f, (reference, key)) in parts.iter().enumerate() {
                for r in sect.children_named(reference) {
                    if let Some(id) = r.attr("r:id") {
                        let kind = r.attr("w:type").unwrap_or("default").to_owned();
                        inherited[f].insert(kind, id.to_owned());
                    }
                }
                let kinds =
                    [("default", "primary", true), ("first", "first", title), ("even", "even", even_odd)];
                for (kind, word_kind, shown) in kinds {
                    if !shown {
                        continue;
                    }
                    let text = inherited[f]
                        .get(kind)
                        .and_then(|id| document.headers.iter().find(|h| &h.id == id))
                        .map(|h| story_text_on_page(&h.story.blocks, &resolver, Some((1, pages))))
                        .unwrap_or_default();
                    let theirs = normalised(word[key][word_kind].as_str().unwrap_or(""));
                    if text != theirs {
                        failures.push(format!(
                            "{name} section {s} {key} {word_kind}: {text:?} against Word's {theirs:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_note_and_comment_is_what_word_reports() {
    use unluminous_word::model::Content;
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let resolver = Resolver::new(&document);
        let kinds = [
            (&document.footnotes, "w:footnoteReference", "footnotes"),
            (&document.endnotes, "w:endnoteReference", "endnotes"),
        ];
        for (part, reference, key) in kinds {
            let mut ids = Vec::new();
            for (_, paragraph) in unluminous_word::position::paragraphs(&document.body) {
                for run in &paragraph.runs {
                    if let Content::Object(el) = &run.content {
                        if el.name == reference {
                            if let Some(id) = el.attr("w:id").and_then(|i| i.parse::<i64>().ok()) {
                                ids.push(id);
                            }
                        }
                    }
                }
            }
            let ours: Vec<String> = ids
                .iter()
                .filter_map(|id| part.as_ref()?.notes.iter().find(|n| n.id() == Some(*id)))
                .map(|note| {
                    story_text(&note.blocks, &resolver).replace('\u{2}', "").trim().to_owned()
                })
                .collect();
            let theirs: Vec<String> = report[key]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| normalised(t.as_str().unwrap_or("")).trim().to_owned())
                .collect();
            if ours != theirs {
                failures.push(format!("{name} {key}: {ours:?} against Word's {theirs:?}"));
            }
        }
        let session = unluminous_word::session::Session::new(document.clone());
        let ours: Vec<(String, String, bool)> = session
            .comments()
            .iter()
            .map(|c| {
                (
                    c["author"].as_str().unwrap_or("").to_owned(),
                    normalised(c["text"].as_str().unwrap_or("")),
                    c["resolved"].as_bool().unwrap_or(false),
                )
            })
            .collect();
        let theirs: Vec<(String, String, bool)> = report["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                (
                    c["author"].as_str().unwrap_or("").to_owned(),
                    normalised(c["text"].as_str().unwrap_or("")),
                    c["done"].as_bool().unwrap_or(false),
                )
            })
            .collect();
        if ours != theirs {
            failures.push(format!("{name} comments: {ours:?} against Word's {theirs:?}"));
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn every_revision_is_what_word_reports() {
    use unluminous_word::model::Content;
    let mut failures = Vec::new();
    for file in fixtures() {
        let report = report(&file);
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        // Word's `WdRevisionType`: 1 insert, 2 delete, 3 property (formatting).
        let mut ours: Vec<(i64, String, String)> = Vec::new();
        for (_, paragraph) in unluminous_word::position::paragraphs(&document.body) {
            let mut last: Option<(i64, u64)> = None;
            for run in &paragraph.runs {
                let wrap =
                    run.wraps.iter().find(|w| matches!(w.el.name.as_str(), "w:ins" | "w:del"));
                let (kind, id, author) = match wrap {
                    Some(w) => (
                        if w.el.name == "w:ins" { 1 } else { 2 },
                        w.id,
                        w.el.attr("w:author").unwrap_or("").to_owned(),
                    ),
                    None => match run.props.child("w:rPrChange") {
                        Some(change) => (
                            3,
                            std::sync::Arc::as_ptr(&run.props) as u64,
                            change.attr("w:author").unwrap_or("").to_owned(),
                        ),
                        None => {
                            last = None;
                            continue;
                        }
                    },
                };
                let text = match &run.content {
                    Content::Text(t) => t.clone(),
                    other => other.character().map(String::from).unwrap_or_default(),
                };
                match ours.last_mut() {
                    Some(entry) if last == Some((kind, id)) => entry.2.push_str(&text),
                    _ => ours.push((kind, author, text)),
                }
                last = Some((kind, id));
            }
        }
        let theirs: Vec<(i64, String, String)> = report["revisions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                (
                    r["type"].as_i64().unwrap_or(0),
                    r["author"].as_str().unwrap_or("").to_owned(),
                    r["text"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect();
        if ours != theirs {
            failures.push(format!("{name} revisions: {ours:?} against Word's {theirs:?}"));
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}
