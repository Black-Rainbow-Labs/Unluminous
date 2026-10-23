//! What Unluminous reads out of a document Word wrote, against what Word itself reports about it.
//!
//! Each fixture in `tests/fixtures` was made by Word through `tools/word-oracle/make-fixtures.ps1`, and the
//! `.word.json` beside it is Word's own report (`inspect.ps1`). These tests need no Word: the reports are
//! committed.

use std::path::{Path, PathBuf};

use serde_json::Value;
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
fn word_order(blocks: &[unluminous_word::model::Block]) -> Vec<(Vec<u32>, Option<&unluminous_word::model::Paragraph>)> {
    use unluminous_word::model::Block;
    let mut out = Vec::new();
    fn walk<'a>(blocks: &'a [Block], prefix: &mut Vec<u32>, out: &mut Vec<(Vec<u32>, Option<&'a unluminous_word::model::Paragraph>)>) {
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
    paragraph["text"].as_str().unwrap_or_default().replace(['\u{7}'], "").trim_end_matches('\r').to_owned()
}

/// A paragraph's text the way Word's `Range.Text` gives it: field codes left out, deleted text kept, hidden
/// text left out, a note reference as character 2, a picture as `/`, and a section break at the end of the
/// paragraph that ends a section.
fn ours(paragraph: &unluminous_word::model::Paragraph, resolver: &Resolver) -> String {
    use unluminous_word::model::Content;
    let mut out = String::new();
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
        if resolver.run(&paragraph.props, &run.props, None).hidden {
            continue;
        }
        match &run.content {
            Content::Text(text) => out.push_str(text),
            Content::Object(el) if el.name == "w:footnoteReference" || el.name == "w:endnoteReference" => out.push('\u{2}'),
            Content::Object(_) if run.content.picture().is_some_and(|p| p.elements().next().is_some_and(|f| f.name != "wp:anchor")) => out.push('/'),
            Content::Object(el) if matches!(el.name.as_str(), "w:commentReference" | "w:annotationRef") => {}
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
            failures.push(format!("{}: {} paragraphs against Word's {}", file.display(), ours_list.len(), words.len()));
            continue;
        }
        for ((_, paragraph), word) in ours_list.iter().zip(&words) {
            let index = word["index"].as_i64().unwrap_or(0);
            let Some(paragraph) = paragraph else {
                if !words_text(word).is_empty() {
                    failures.push(format!("{} paragraph {index}: a row end against Word's {:?}", file.display(), words_text(word)));
                }
                continue;
            };
            let text = ours(paragraph, &resolver);
            // Word reports text in all capitals as it is drawn.
            if text.to_lowercase() != words_text(word).to_lowercase() {
                failures.push(format!("{} paragraph {index}: {text:?} against Word's {:?}", file.display(), words_text(word)));
            }
            let style_id = resolver.paragraph_style_id(&paragraph.props).unwrap_or("Normal");
            let style = resolver.name_of(style_id);
            if Some(style.as_str()) != word["style"].as_str() {
                failures.push(format!("{} paragraph {index}: style {style:?} against Word's {:?}", file.display(), word["style"]));
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
            failures.push(format!("{}: {} paragraphs against Word's {}", file.display(), ours_list.len(), words.len()));
            continue;
        }
        for ((path, paragraph), word) in ours_list.iter().zip(&words) {
            let index = word["index"].as_i64().unwrap_or(0);
            let Some(paragraph) = paragraph else { continue };
            let table_style = unluminous_word::position::cell_of(&document.body, path).and_then(|(table, _, _)| {
                match unluminous_word::position::block(&document.body, &table) {
                    Some(unluminous_word::model::Block::Table(t)) => resolver.table_style_id(&t.props),
                    _ => None,
                }
            });
            let look = resolver.paragraph(&paragraph.props, table_style);
            let alignment = match look.align {
                unluminous_word::props::Align::Left => 0,
                unluminous_word::props::Align::Center => 1,
                unluminous_word::props::Align::Right => 2,
                unluminous_word::props::Align::Justify => 3,
            };
            let close = |a: f32, b: &Value| (a - b.as_f64().unwrap_or(f64::NAN) as f32).abs() < 0.11;
            let checks = [
                ("alignment", word["alignment"].as_i64() == Some(alignment)),
                ("leftIndent", close(look.indent_left, &word["leftIndent"])),
                ("rightIndent", close(look.indent_right, &word["rightIndent"])),
                ("firstLineIndent", close(look.indent_first, &word["firstLineIndent"])),
                ("spaceBefore", look.before_auto || close(look.space_before, &word["spaceBefore"])),
                ("spaceAfter", look.after_auto || close(look.space_after, &word["spaceAfter"])),
                ("keepWithNext", word["keepWithNext"].as_bool() == Some(look.keep_next)),
                ("pageBreakBefore", word["pageBreakBefore"].as_bool() == Some(look.page_break_before)),
            ];
            for (name, ok) in checks {
                if !ok {
                    failures.push(format!("{} paragraph {index} {:?}: {name} ours {look:?} Word {}", file.display(), words_text(word), word[name]));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} differences:\n{}", failures.len(), failures.join("\n"));
}
