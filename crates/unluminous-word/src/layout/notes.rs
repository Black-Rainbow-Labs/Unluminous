//! Footnote and endnote numbering, from the document's `w:footnotePr` and `w:endnotePr` (task-2359, M11).
//!
//! Word numbers notes in order of first reference, in a format (`w:numFmt`, decimal for footnotes and
//! lower case roman for endnotes when nothing is said), from a start (`w:numStart`), and again from the
//! start on each section or each page when `w:numRestart` says so. Endnotes go at the end of the document,
//! or at the end of each section with `w:pos` `sectEnd`. A reference with `w:customMarkFollows` shows the
//! mark typed after it and is not counted.

use std::collections::HashMap;

use crate::model::{Block, Content, Document};
use crate::xml::El;

/// How a note kind restarts its numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    Continuous,
    EachSection,
    EachPage,
}

/// How one kind of note is numbered and where endnotes go.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteOptions {
    pub format: String,
    pub start: i64,
    pub restart: Restart,
    /// Endnotes at the end of each section rather than of the document.
    pub section_end: bool,
}

impl NoteOptions {
    /// The options for footnotes (`endnote` false) or endnotes, from the settings part.
    pub fn of(document: &Document, endnote: bool) -> NoteOptions {
        let name = if endnote { "w:endnotePr" } else { "w:footnotePr" };
        let settings = document.settings.as_ref().and_then(|s| s.root.child(name));
        // A section's own options win over the settings', the last section's standing for the document.
        let section = document.section.child(name);
        let get = |child: &str| -> Option<String> {
            section
                .and_then(|el| el.val_of(child))
                .or_else(|| settings.and_then(|el| el.val_of(child)))
                .map(str::to_owned)
        };
        NoteOptions {
            format: get("w:numFmt").unwrap_or_else(|| {
                if endnote { "lowerRoman".into() } else { "decimal".into() }
            }),
            start: get("w:numStart").and_then(|s| s.parse().ok()).unwrap_or(1),
            restart: match get("w:numRestart").as_deref() {
                Some("eachSect") => Restart::EachSection,
                Some("eachPage") => Restart::EachPage,
                _ => Restart::Continuous,
            },
            section_end: get("w:pos").as_deref() == Some("sectEnd"),
        }
    }
}

/// Every note reference in the body, in order: whether it is an endnote, the note's id, and the section
/// it is in. Custom marks are left out.
pub fn references(document: &Document) -> Vec<(bool, i64, usize)> {
    let mut out = Vec::new();
    let mut section = 0;
    for block in &document.body {
        for (_, paragraph) in crate::position::paragraphs(std::slice::from_ref(block)) {
            for run in &paragraph.runs {
                let Content::Object(el) = &run.content else { continue };
                let endnote = match el.name.as_str() {
                    "w:footnoteReference" => false,
                    "w:endnoteReference" => true,
                    _ => continue,
                };
                if custom(el) {
                    continue;
                }
                if let Some(id) = el.attr("w:id").and_then(|id| id.parse::<i64>().ok()) {
                    if !out.iter().any(|(e, i, _)| *e == endnote && *i == id) {
                        out.push((endnote, id, section));
                    }
                }
            }
        }
        if let Block::Paragraph(paragraph) = block {
            if paragraph.props.child("w:sectPr").is_some() {
                section += 1;
            }
        }
    }
    out
}

/// Whether a reference shows its own mark.
pub fn custom(el: &El) -> bool {
    el.attr("w:customMarkFollows").is_some_and(|v| v == "1" || v == "true")
}

/// The numbers of one kind of note: each note's order of first reference (counting from 1 over the
/// whole document), and the mark it shows. `pages` is the page each footnote landed on, for numbering
/// that restarts on each page; without it every note is taken to be on one page.
pub fn number(
    references: &[(bool, i64, usize)],
    endnote: bool,
    options: &NoteOptions,
    pages: Option<&HashMap<i64, usize>>,
) -> (HashMap<i64, u32>, HashMap<i64, String>) {
    let mut order = HashMap::new();
    let mut marks = HashMap::new();
    let mut counter = options.start - 1;
    let mut last_group: Option<usize> = None;
    for (kind, id, section) in references {
        if *kind != endnote {
            continue;
        }
        let group = match options.restart {
            Restart::Continuous => 0,
            Restart::EachSection => *section,
            Restart::EachPage => pages.and_then(|p| p.get(id)).copied().unwrap_or(0),
        };
        if last_group.is_some_and(|last| last != group) {
            counter = options.start - 1;
        }
        last_group = Some(group);
        counter += 1;
        let next = order.len() as u32 + 1;
        order.insert(*id, next);
        marks.insert(*id, crate::numbering::format_number(counter, &options.format));
    }
    (order, marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(format: &str, start: i64, restart: Restart) -> NoteOptions {
        NoteOptions { format: format.into(), start, restart, section_end: false }
    }

    #[test]
    fn notes_number_from_their_start_in_their_format() {
        let refs = vec![(false, 4, 0), (false, 5, 0), (true, 1, 0), (false, 6, 1)];
        let (order, marks) = number(&refs, false, &options("lowerRoman", 3, Restart::Continuous), None);
        assert_eq!(order[&6], 3);
        assert_eq!(marks[&4], "iii");
        assert_eq!(marks[&6], "v");
        let (_, marks) = number(&refs, false, &options("decimal", 1, Restart::EachSection), None);
        assert_eq!(marks[&6], "1", "a new section starts again");
        let (_, marks) = number(&refs, true, &options("upperLetter", 3, Restart::Continuous), None);
        assert_eq!(marks[&1], "C");
    }

    #[test]
    fn numbering_by_page_starts_again_on_each_page() {
        let refs = vec![(false, 1, 0), (false, 2, 0), (false, 3, 0)];
        let pages: HashMap<i64, usize> = [(1, 0), (2, 0), (3, 1)].into_iter().collect();
        let (_, marks) =
            number(&refs, false, &options("lowerRoman", 1, Restart::EachPage), Some(&pages));
        assert_eq!((marks[&1].as_str(), marks[&2].as_str(), marks[&3].as_str()), ("i", "ii", "i"));
    }
}
