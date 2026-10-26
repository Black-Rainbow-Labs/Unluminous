//! Find and Replace with Word's options (task-2359, TDD 5.4 C43, M19): wildcards and special characters
//! through [`crate::find`], formatting to find and formatting to replace with, every story or the one
//! the caret is in, and Go To.
//!
//! The text model has one paragraph's text at a time, so a match never crosses a paragraph mark, which
//! is also what Word does outside wildcard searches for `^p`.

use std::sync::Arc;

use crate::edit;
use crate::find::{self, Query};
use crate::model::*;
use crate::position::{self, Pos, Selection, StoryId};
use crate::props::{self, Align, Rgb, Underline};
use crate::session::{Scope, Session};
use crate::xml::El;

/// The formatting a match must have, each part only when it is set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Format {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub colour: Option<Rgb>,
    pub highlight: Option<bool>,
    /// A paragraph style by id or name.
    pub style: Option<String>,
    pub align: Option<Align>,
}

impl Format {
    pub fn is_empty(&self) -> bool {
        *self == Format::default()
    }

    /// The run properties that give text this formatting, for Replace With formatting.
    pub fn run_props(&self) -> El {
        let mut rpr = El::new("w:rPr");
        if let Some(b) = self.bold {
            props::set_on_off(&mut rpr, "w:b", Some(b));
        }
        if let Some(i) = self.italic {
            props::set_on_off(&mut rpr, "w:i", Some(i));
        }
        if let Some(u) = self.underline {
            props::set_val(&mut rpr, "w:u", Some(if u { "single" } else { "none" }));
        }
        if let Some(c) = self.colour {
            rpr.put(El::new("w:color").with("w:val", c.hex()));
        }
        if let Some(h) = self.highlight {
            props::set_val(&mut rpr, "w:highlight", Some(if h { "yellow" } else { "none" }));
        }
        crate::order::sort(&mut rpr);
        rpr
    }
}

/// One search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Search {
    pub query: Query,
    pub format: Format,
    /// Search the headers, footers, notes, comments and text boxes too, not only the caret's story.
    pub every_story: bool,
}

impl Session {
    /// Every match of a search, story by story in document order (the body first), each a selection.
    pub fn search(&self, search: &Search) -> Result<Vec<Selection>, String> {
        if search.query.text.is_empty() && search.format.is_empty() {
            return Ok(Vec::new());
        }
        let stories = if search.every_story {
            every_story(&self.document)
        } else {
            vec![self.selection.head.story.clone()]
        };
        let resolver = crate::styles::Resolver::new(&self.document);
        let mut out = Vec::new();
        for story in stories {
            let Some(blocks) = position::story(&self.document, &story) else { continue };
            for (path, paragraph) in position::paragraphs(blocks) {
                if !paragraph_matches(&search.format, paragraph, &resolver) {
                    continue;
                }
                let (text, map) = searchable(paragraph);
                let ranges: Vec<(usize, usize)> = if search.query.text.is_empty() {
                    // Formatting alone: every run of the paragraph that has it.
                    runs_with(&search.format, paragraph, &resolver)
                } else {
                    find::find_all(&search.query, &text)?
                        .into_iter()
                        .map(|f| {
                            (map[f.start], if f.end == 0 { map[0] } else { map[f.end - 1] + 1 })
                        })
                        .map(|(a, b)| (a, widen_to_char(paragraph, b)))
                        .filter(|&(a, b)| {
                            run_format_matches(&search.format, paragraph, &resolver, a, b)
                        })
                        .collect()
                };
                for (start, end) in ranges {
                    out.push(Selection {
                        anchor: Pos { story: story.clone(), path: path.clone(), offset: start },
                        head: Pos { story: story.clone(), path: path.clone(), offset: end },
                    });
                }
            }
        }
        Ok(out)
    }

    /// Replace every match with `with` (its special characters and, with wildcards, its `\1` groups
    /// filled in), giving the new text `format` when it is set, as one undo step. Answers how many.
    pub fn replace_matches(
        &mut self,
        search: &Search,
        with: &str,
        format: &Format,
    ) -> Result<usize, String> {
        let matches = self.search(search)?;
        if matches.is_empty() {
            return Ok(0);
        }
        // Each replacement worked out from the text as it is now, before anything changes.
        let mut plan = Vec::with_capacity(matches.len());
        for found in &matches {
            let paragraph =
                position::paragraph(&self.document, &found.anchor.story, &found.anchor.path);
            let (text, map) = paragraph.map(searchable).unwrap_or_default();
            let flat = paragraph.map(Paragraph::flat_text).unwrap_or_default();
            // The match in the searchable text, found again for its groups and its own text.
            let hit = find::find_all(&search.query, &text)
                .unwrap_or_default()
                .into_iter()
                .find(|f| map.get(f.start) == Some(&found.anchor.offset))
                .unwrap_or(find::Found { start: 0, end: 0, groups: Vec::new() });
            let new = if search.query.text.is_empty() {
                flat.get(found.anchor.offset..found.head.offset).unwrap_or("").to_owned()
            } else {
                find::replacement(&search.query, with, &text, &hit)
            };
            plan.push((found.clone(), new));
        }
        let count = plan.len();
        let extra = (!format.is_empty()).then(|| format.run_props());
        self.change(Scope::Whole, move |document, selection, tracking| {
            for (found, new) in plan.iter().rev() {
                let base = position::paragraph(document, &found.anchor.story, &found.anchor.path)
                    .map(|p| edit::props_at(p, found.anchor.offset + 1).0);
                let props = match (&extra, base) {
                    (Some(extra), base) => {
                        let mut merged = base.map_or_else(|| El::new("w:rPr"), |b| (*b).clone());
                        for child in extra.elements() {
                            merged.put(child.clone());
                        }
                        crate::order::sort(&mut merged);
                        Some(Arc::new(merged))
                    }
                    (None, base) => base,
                };
                let at = edit::delete_range(document, &found.anchor, &found.head, tracking);
                // A paragraph mark in the replacement splits the paragraph.
                let mut pos = at;
                for (k, piece) in new.split('\n').enumerate() {
                    if k > 0 {
                        pos = edit::split_paragraph(document, &pos, tracking);
                    }
                    if !piece.is_empty() {
                        pos = edit::insert_text(document, &pos, piece, props.clone(), tracking);
                    }
                }
            }
            let length = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(length)))
        });
        Ok(count)
    }

    /// Go To, as Word's `Ctrl+G` offers it: the `n`th (from 1) page, section, line, bookmark (by number
    /// or name), footnote, endnote, comment, heading, table or picture. Answers where the caret went.
    pub fn go_to(
        &mut self,
        what: &str,
        which: &str,
        pages: &crate::layout::Pages,
    ) -> Result<Pos, String> {
        let n: usize = which.parse().unwrap_or(0);
        let nth = |list: Vec<Pos>, name: &str| -> Result<Pos, String> {
            let count = list.len();
            list.into_iter()
                .nth(n.saturating_sub(1))
                .ok_or_else(|| format!("there is no {name} {which}: the document has {count}"))
        };
        let body = position::paragraphs(&self.document.body);
        let at = match what {
            "page" => {
                let page = pages.pages.get(n.saturating_sub(1)).ok_or_else(|| {
                    format!("there is no page {which}: the document has {}", pages.pages.len())
                })?;
                page.first.clone().ok_or("that page has no text")?
            }
            "line" => {
                let lines: Vec<Pos> = pages
                    .pages
                    .iter()
                    .flat_map(|p| p.carets.iter())
                    .filter(|l| l.story == StoryId::Body)
                    .map(|l| Pos { story: StoryId::Body, path: l.path.clone(), offset: l.start })
                    .collect();
                nth(lines, "line")?
            }
            "section" => {
                let mut starts = vec![body.first().map_or(Pos::new(vec![0], 0), |(p, _)| Pos::new(p.clone(), 0))];
                for (i, (_, p)) in body.iter().enumerate() {
                    if p.props.child("w:sectPr").is_some() {
                        if let Some((next, _)) = body.get(i + 1) {
                            starts.push(Pos::new(next.clone(), 0));
                        }
                    }
                }
                nth(starts, "section")?
            }
            "bookmark" => {
                let marks = self.bookmarks();
                match marks.iter().find(|(name, _)| name == which) {
                    Some((_, pos)) => pos.clone(),
                    None => nth(marks.into_iter().map(|(_, p)| p).collect(), "bookmark")?,
                }
            }
            "footnote" | "endnote" | "comment" => {
                let name = match what {
                    "footnote" => "w:footnoteReference",
                    "endnote" => "w:endnoteReference",
                    _ => "w:commentReference",
                };
                let mut found = Vec::new();
                for (path, p) in &body {
                    let mut offset = 0;
                    for run in &p.runs {
                        if matches!(&run.content, Content::Object(el) if el.name == name) {
                            found.push(Pos::new(path.clone(), offset));
                        }
                        offset += run.len();
                    }
                }
                nth(found, what)?
            }
            "heading" => {
                let resolver = crate::styles::Resolver::new(&self.document);
                let headings: Vec<Pos> = body
                    .iter()
                    .filter(|(_, p)| resolver.paragraph(&p.props, None).outline_level.is_some())
                    .map(|(path, _)| Pos::new(path.clone(), 0))
                    .collect();
                nth(headings, "heading")?
            }
            "table" => {
                let tables: Vec<Pos> = self
                    .document
                    .body
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| matches!(b, Block::Table(_)))
                    .map(|(i, _)| Pos::new(vec![i as u32, 0, 0, 0], 0))
                    .collect();
                nth(tables, "table")?
            }
            "picture" | "graphic" => {
                let mut found = Vec::new();
                for (path, p) in &body {
                    let mut offset = 0;
                    for run in &p.runs {
                        if run.content.picture().is_some() {
                            found.push(Pos::new(path.clone(), offset));
                        }
                        offset += run.len();
                    }
                }
                nth(found, "picture")?
            }
            other => {
                return Err(format!(
                    "go to takes page, section, line, bookmark, footnote, endnote, comment, heading, table or picture, not {other}"
                ))
            }
        };
        self.select(at.clone(), at.clone());
        Ok(at)
    }
}

/// Every story in the order a search walks them: the body, the headers and footers, the footnotes,
/// the endnotes, the comments and the text boxes.
pub fn every_story(document: &Document) -> Vec<StoryId> {
    let mut out = vec![StoryId::Body];
    out.extend(document.headers.iter().map(|h| StoryId::Header(h.id.clone())));
    let notes = |part: &Option<NotesPart>, make: fn(i64) -> StoryId| -> Vec<StoryId> {
        part.as_ref()
            .map(|p| p.notes.iter().filter_map(Story::id).filter(|id| *id > 0).map(make).collect())
            .unwrap_or_default()
    };
    out.extend(notes(&document.footnotes, StoryId::Footnote));
    out.extend(notes(&document.endnotes, StoryId::Endnote));
    if let Some(part) = &document.comments {
        out.extend(
            part.notes.iter().filter_map(Story::id).filter(|id| *id >= 0).map(StoryId::Comment),
        );
    }
    out.extend(document.textboxes.iter().map(|t| StoryId::TextBox(t.id)));
    out
}

/// A paragraph's text as Find reads it, and for each of its bytes the byte of the flat text it came
/// from: comment references and marks, field codes and field characters are left out, so a comment
/// placed inside a word does not stop the word being found, as in Word.
pub fn searchable(paragraph: &Paragraph) -> (String, Vec<usize>) {
    let mut text = String::new();
    let mut map = Vec::new();
    let mut at = 0;
    let mut in_code = Vec::new();
    for run in &paragraph.runs {
        let length = run.len();
        match &run.content {
            Content::FieldChar(el) => match el.attr("w:fldCharType") {
                Some("begin") => in_code.push(true),
                Some("separate") => {
                    if let Some(last) = in_code.last_mut() {
                        *last = false;
                    }
                }
                _ => {
                    in_code.pop();
                }
            },
            Content::Instr(_) => {}
            Content::Object(el)
                if matches!(el.name.as_str(), "w:commentReference" | "w:annotationRef") => {}
            _ if in_code.iter().any(|c| *c) => {}
            _ => {
                let flat: String = match &run.content {
                    Content::Text(t) => t.clone(),
                    other => other.character().map(String::from).unwrap_or_default(),
                };
                for (i, _) in flat.char_indices() {
                    let width = flat[i..].chars().next().map_or(1, char::len_utf8);
                    for k in 0..width {
                        map.push(at + i + k);
                    }
                }
                text.push_str(&flat);
            }
        }
        at += length;
    }
    map.push(at);
    (text, map)
}

/// The end of a match moved past the rest of a character it ends inside.
fn widen_to_char(paragraph: &Paragraph, end: usize) -> usize {
    let flat = paragraph.flat_text();
    let mut end = end.min(flat.len());
    while !flat.is_char_boundary(end) {
        end += 1;
    }
    end
}

fn paragraph_matches(
    format: &Format,
    paragraph: &Paragraph,
    resolver: &crate::styles::Resolver,
) -> bool {
    if format.style.is_none() && format.align.is_none() {
        return true;
    }
    let look = resolver.paragraph(&paragraph.props, None);
    let style_ok = format.style.as_ref().is_none_or(|want| {
        let id = resolver.paragraph_style_id(&paragraph.props).unwrap_or("Normal");
        id.eq_ignore_ascii_case(want) || resolver.name_of(id).eq_ignore_ascii_case(want)
    });
    let align_ok = format.align.is_none_or(|a| a == look.align);
    style_ok && align_ok
}

/// The look of every run, with the byte range of the paragraph's flat text each covers.
fn looks(
    paragraph: &Paragraph,
    resolver: &crate::styles::Resolver,
) -> Vec<(usize, usize, props::RunLook)> {
    let mut at = 0;
    let mut out = Vec::new();
    for run in &paragraph.runs {
        let length = run.len();
        if length > 0 && matches!(run.content, Content::Text(_)) {
            out.push((at, at + length, resolver.run(&paragraph.props, &run.props, None)));
        }
        at += length;
    }
    out
}

fn look_matches(format: &Format, look: &props::RunLook) -> bool {
    format.bold.is_none_or(|b| b == look.bold)
        && format.italic.is_none_or(|i| i == look.italic)
        && format.underline.is_none_or(|u| u == (look.underline != Underline::None))
        && format.colour.is_none_or(|c| Some(c) == look.color)
        && format.highlight.is_none_or(|h| h == look.highlight.is_some())
}

fn run_format_matches(
    format: &Format,
    paragraph: &Paragraph,
    resolver: &crate::styles::Resolver,
    from: usize,
    to: usize,
) -> bool {
    let character = Format { style: None, align: None, ..format.clone() };
    if character.is_empty() {
        return true;
    }
    looks(paragraph, resolver)
        .iter()
        .filter(|(a, b, _)| *a < to.max(from + 1) && *b > from)
        .all(|(_, _, look)| look_matches(&character, look))
}

/// For a search for formatting alone: the runs of a paragraph that have it, joined where they touch.
fn runs_with(
    format: &Format,
    paragraph: &Paragraph,
    resolver: &crate::styles::Resolver,
) -> Vec<(usize, usize)> {
    let character = Format { style: None, align: None, ..format.clone() };
    if character.is_empty() {
        let length = paragraph.len();
        return if length > 0 { vec![(0, length)] } else { Vec::new() };
    }
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (a, b, look) in looks(paragraph, resolver) {
        if look_matches(&character, &look) {
            match out.last_mut() {
                Some(last) if last.1 == a => last.1 = b,
                _ => out.push((a, b)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(text: &str) -> Session {
        let mut session = Session::new(crate::new::blank());
        session.type_text(text);
        session
    }

    fn text(session: &Session) -> String {
        position::paragraph(&session.document, &StoryId::Body, &[0]).unwrap().flat_text()
    }

    #[test]
    fn a_wildcard_replacement_swaps_the_two_words() {
        let mut s = session("Smith John and Doe Jane");
        let search = Search {
            query: Query {
                text: "(<[A-Z][a-z]@>) (<[A-Z][a-z]@>)".into(),
                wildcards: true,
                ..Query::default()
            },
            ..Search::default()
        };
        assert_eq!(s.search(&search).unwrap().len(), 2);
        let count = s.replace_matches(&search, "\\2 \\1", &Format::default()).unwrap();
        assert_eq!(count, 2);
        assert_eq!(text(&s), "John Smith and Jane Doe");
        assert!(s.undo());
        assert_eq!(text(&s), "Smith John and Doe Jane");
    }

    #[test]
    fn formatting_is_found_and_given() {
        let mut s = session("plain ");
        s.toggle("w:b");
        s.type_text("bold");
        s.toggle("w:b");
        s.type_text(" plain bold");
        let bold = Search {
            format: Format { bold: Some(true), ..Format::default() },
            ..Search::default()
        };
        let found = s.search(&bold).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].anchor.offset, found[0].head.offset), (6, 10));
        let words = Search {
            query: Query { text: "bold".into(), ..Query::default() },
            format: Format { bold: Some(false), ..Format::default() },
            ..Search::default()
        };
        assert_eq!(s.search(&words).unwrap().len(), 1, "only the plain one");
        let italic = Format { italic: Some(true), ..Format::default() };
        let plain_bold = Search {
            query: Query { text: "plain".into(), ..Query::default() },
            ..Search::default()
        };
        assert_eq!(s.replace_matches(&plain_bold, "^&", &italic).unwrap(), 2);
        let p = position::paragraph(&s.document, &StoryId::Body, &[0]).unwrap();
        assert!(p.runs.iter().any(|r| r.props.child("w:i").is_some()));
    }

    #[test]
    fn a_paragraph_mark_in_the_replacement_splits_the_paragraph() {
        let mut s = session("one;two");
        let search =
            Search { query: Query { text: ";".into(), ..Query::default() }, ..Search::default() };
        s.replace_matches(&search, "^p", &Format::default()).unwrap();
        assert_eq!(position::paragraphs(&s.document.body).len(), 2);
    }
}
