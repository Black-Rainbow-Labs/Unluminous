//! Tables of contents and of figures, Update Fields, captions, cross references, hyperlinks and the field
//! commands (unlink, lock), as [`Session`] methods so each is one undo step (task-2359, TDD 5.4).
//!
//! **A table of contents is written the way Word writes one**: a block content control whose
//! `w:docPartGallery` is `Table of Contents`, holding a `TOC Heading` paragraph and then the `TOC`
//! field, each entry a hyperlink to a `_Toc` bookmark on its heading with a `PAGEREF` field for the page,
//! and the field's end in a paragraph of its own. Update finds a table of contents by its gallery or its
//! `TOC` field anywhere in the body and builds its entries again inside it, keeping its field code, so
//! a table Word made and one made here update alike.

use std::collections::HashMap;
use std::sync::Arc;

use crate::fields::{self, Env};
use crate::layout::Pages;
use crate::model::*;
use crate::position::{self, Path, Pos, Selection, StoryId};
use crate::session::{Scope, Session};
use crate::xml::{El, Node};

/// What a table of contents (or of figures) lists and how, which is its field code.
#[derive(Debug, Clone, PartialEq)]
pub struct TocOptions {
    /// The heading levels listed, 1 to this.
    pub levels: u8,
    pub page_numbers: bool,
    pub right_align: bool,
    /// The tab leader before the page number: `dot`, `hyphen`, `underscore` or `none`.
    pub leader: String,
    pub hyperlinks: bool,
    /// For a table of figures: the caption label its entries are (`Figure`, `Table`).
    pub caption: Option<String>,
}

impl Default for TocOptions {
    fn default() -> Self {
        TocOptions {
            levels: 3,
            page_numbers: true,
            right_align: true,
            leader: "dot".into(),
            hyperlinks: true,
            caption: None,
        }
    }
}

impl TocOptions {
    /// The field code these options are.
    pub fn code(&self) -> String {
        let mut code = match &self.caption {
            Some(label) => format!(" TOC \\h \\z \\c \"{label}\""),
            None => format!(" TOC \\o \"1-{}\" \\h \\z \\u", self.levels),
        };
        if !self.hyperlinks {
            code = code.replace(" \\h", "");
        }
        if !self.page_numbers {
            code.push_str(" \\n");
        }
        code.push(' ');
        code
    }

    /// The options a `TOC` field code says.
    pub fn of(code: &str) -> TocOptions {
        let parsed = fields::parse(code);
        let levels = parsed
            .switch("\\o")
            .and_then(|range| range.split('-').nth(1))
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(if parsed.has("\\c") { 0 } else { 3 });
        TocOptions {
            levels,
            page_numbers: !parsed.has("\\n"),
            right_align: true,
            leader: "dot".into(),
            hyperlinks: parsed.has("\\h"),
            caption: parsed.switch("\\c").filter(|c| !c.is_empty()).map(str::to_owned),
        }
    }
}

/// Where a table of contents is: the path of the block holding it (a content control, or `None` for one
/// whose field sits directly in the body) and the range of paragraphs it covers in that container.
#[derive(Debug, Clone, PartialEq)]
pub struct TocPlace {
    /// The content control holding it, by its path in the body.
    pub sdt: Option<Path>,
    /// In the container: the first and last block index it covers.
    pub first: Path,
    pub last: Path,
    pub code: String,
}

/// Every table of contents and of figures in the body.
pub fn find_tocs(document: &Document) -> Vec<TocPlace> {
    let mut out = Vec::new();
    find_in(&document.body, &mut Vec::new(), None, &mut out);
    out
}

fn find_in(blocks: &[Block], prefix: &mut Path, sdt: Option<&Path>, out: &mut Vec<TocPlace>) {
    let mut open: Option<(Path, String, usize)> = None;
    for (index, block) in blocks.iter().enumerate() {
        prefix.push(index as u32);
        match block {
            Block::Paragraph(paragraph) => {
                for run in &paragraph.runs {
                    match &run.content {
                        Content::FieldChar(el) if open.is_some() => {
                            let (_, _, depth) = open.as_mut().expect("open");
                            match el.attr("w:fldCharType") {
                                Some("begin") => *depth += 1,
                                Some("end") => {
                                    if *depth == 0 {
                                        let (first, code, _) = open.take().expect("open");
                                        out.push(TocPlace {
                                            sdt: sdt.cloned(),
                                            first,
                                            last: prefix.clone(),
                                            code,
                                        });
                                    } else {
                                        *depth -= 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        Content::Instr(code)
                            if open.is_none() && fields::parse(code).kind == "TOC" =>
                        {
                            open = Some((prefix.clone(), code.clone(), 0));
                        }
                        _ => {}
                    }
                }
            }
            Block::Sdt(inner) => {
                let gallery = gallery_of(&inner.el);
                let path = prefix.clone();
                if gallery.as_deref() == Some("Table of Contents") {
                    let mut found = Vec::new();
                    find_in(&inner.blocks, &mut Vec::new(), Some(&path), &mut found);
                    match found.into_iter().next() {
                        Some(place) => out.push(place),
                        None => out.push(TocPlace {
                            sdt: Some(path),
                            first: vec![0],
                            last: vec![inner.blocks.len().saturating_sub(1) as u32],
                            code: TocOptions::default().code(),
                        }),
                    }
                } else {
                    find_in(&inner.blocks, prefix, sdt, out);
                }
            }
            Block::Table(_) | Block::Kept(_) => {}
        }
        prefix.pop();
    }
}

/// A content control's `w:docPartGallery`.
fn gallery_of(el: &El) -> Option<String> {
    el.child("w:sdtPr")?
        .child("w:docPartObj")?
        .child("w:docPartGallery")?
        .attr("w:val")
        .map(str::to_owned)
}

/// The paragraph style ids that are entries of a table of contents (`TOC1`...), its heading, and caption
/// tables, which a table of contents leaves out.
fn is_toc_style(style: &str) -> bool {
    style.starts_with("TOC")
        || style == "TableofFigures"
        || style.eq_ignore_ascii_case("TOCHeading")
}

impl Session {
    /// The headings a table of contents lists: level, text, page and where each is. Headings inside a
    /// table of contents and its own entries are left out.
    pub fn toc_entries(&self, levels: u8, pages: &Pages) -> Vec<(u8, String, u32, Vec<u32>)> {
        let resolver = crate::styles::Resolver::new(&self.document);
        let inside: Vec<Path> =
            find_tocs(&self.document).into_iter().filter_map(|t| t.sdt).collect();
        let mut out = Vec::new();
        for (path, p) in position::paragraphs(&self.document.body) {
            if inside.iter().any(|sdt| path.starts_with(sdt))
                || position::cell_of(&self.document.body, &path).is_some()
            {
                continue;
            }
            let look = resolver.paragraph(&p.props, None);
            let Some(level) = look.outline_level else { continue };
            if level >= levels || is_toc_style(&look.style) {
                continue;
            }
            let text = crate::session::clean_text(&p.visible_text()).trim().to_owned();
            if text.is_empty() {
                continue;
            }
            let page = page_of(pages, &path);
            out.push((level + 1, text, page, path));
        }
        out
    }

    /// The captions a table of figures lists for a label: the caption's text, its page, where it is.
    pub fn caption_entries(&self, label: &str, pages: &Pages) -> Vec<(u8, String, u32, Vec<u32>)> {
        let mut out = Vec::new();
        for (path, p) in position::paragraphs(&self.document.body) {
            let has_seq = p.runs.iter().any(|r| {
                matches!(&r.content, Content::Instr(code) if {
                    let code = fields::parse(code);
                    code.kind == "SEQ" && code.arguments.first().is_some_and(|a| a.eq_ignore_ascii_case(label))
                })
            });
            if !has_seq {
                continue;
            }
            let text = crate::session::clean_text(&p.visible_text()).trim().to_owned();
            out.push((1, text, page_of(pages, &path), path));
        }
        out
    }

    /// Insert a table of contents at the caret, as Word's Automatic Table does.
    pub fn insert_toc(&mut self, levels: u8, pages: &Pages) {
        self.insert_toc_with(&TocOptions { levels, ..TocOptions::default() }, pages);
    }

    /// Insert a table of contents or of figures with these options at the caret.
    pub fn insert_toc_with(&mut self, options: &TocOptions, pages: &Pages) {
        let entries = match &options.caption {
            Some(label) => self.caption_entries(label, pages),
            None => self.toc_entries(options.levels, pages),
        };
        self.ensure_toc_styles(options);
        let width = self
            .section_at_caret()
            .map_or(468.0, |s| crate::layout::section::Geometry::of(&s).text_width());
        let options = options.clone();
        self.change(Scope::Whole, move |document, selection, _| {
            let blocks = toc_blocks(document, &options, &options.code(), &entries, width, true);
            let control = Block::Sdt(Sdt { el: toc_control(), blocks });
            let at = selection.head.clone();
            let Some(story) = position::story_mut(document, &at.story) else {
                return selection.clone();
            };
            let Some((container, index)) = position::container_mut(story, &at.path) else {
                return selection.clone();
            };
            container.insert(index, control);
            let mut path = at.path.clone();
            *path.last_mut().expect("a path") += 1;
            Selection::caret(Pos { story: at.story, path, offset: 0 })
        });
    }

    fn ensure_toc_styles(&mut self, options: &TocOptions) {
        for level in 1..=options.levels.max(1) {
            let _ = crate::builtin::ensure_style(&mut self.document, &format!("TOC{level}"));
        }
        let _ = crate::builtin::ensure_style(&mut self.document, "TOCHeading");
        if options.caption.is_some() {
            let _ = crate::builtin::ensure_style(&mut self.document, "TableofFigures");
        }
    }

    /// Build every table of contents and of figures again where it stands, keeping its field code.
    /// Answers how many.
    pub fn update_tocs(&mut self, pages: &Pages) -> usize {
        let places = find_tocs(&self.document);
        let width = self
            .section_at_caret()
            .map_or(468.0, |s| crate::layout::section::Geometry::of(&s).text_width());
        let mut count = 0;
        // From the last, so the paths of the earlier ones stay right.
        for place in places.into_iter().rev() {
            let options = TocOptions::of(&place.code);
            let entries = match &options.caption {
                Some(label) => self.caption_entries(label, pages),
                None => self.toc_entries(options.levels, pages),
            };
            self.ensure_toc_styles(&options);
            self.change(Scope::Whole, |document, selection, _| {
                replace_toc(document, &place, &options, &entries, width);
                selection.clone()
            });
            count += 1;
        }
        count
    }

    /// Take out the table of contents the caret is in, or the first one.
    pub fn remove_toc(&mut self) -> bool {
        let places = find_tocs(&self.document);
        let caret = self.selection.head.path.clone();
        let Some(place) = places
            .iter()
            .find(|p| p.sdt.as_ref().is_some_and(|s| caret.starts_with(s)))
            .or(places.first())
            .cloned()
        else {
            return false;
        };
        self.change(Scope::Whole, |document, _, _| {
            match &place.sdt {
                Some(sdt) => {
                    if let Some((container, index)) =
                        position::container_mut(&mut document.body, sdt)
                    {
                        container.remove(index);
                    }
                }
                None => {
                    let (from, to) =
                        (*place.first.last().unwrap_or(&0), *place.last.last().unwrap_or(&0));
                    if let Some((container, _)) =
                        position::container_mut(&mut document.body, &place.first)
                    {
                        container.drain(from as usize..=(to as usize).min(container.len() - 1));
                    }
                }
            }
            if document.body.is_empty() {
                document.body.push(Block::Paragraph(Paragraph::new()));
            }
            let first =
                position::paragraphs(&document.body).first().map_or(vec![0], |(p, _)| p.clone());
            Selection::caret(Pos::new(first, 0))
        });
        true
    }

    /// Update every field that can be: tables of contents and of figures, then every other field in
    /// the body, the headers and footers and the notes, in document order so `SEQ` counts. A locked field
    /// keeps its result. Answers how many fields changed.
    pub fn update_fields(&mut self, pages: &Pages) -> usize {
        self.update_fields_where(pages, None)
    }

    /// Update the fields in the paragraphs the selection covers, as F9 and the right click menu's Update
    /// Field do; every other field keeps its result. `SEQ` still counts from the top of the document.
    pub fn update_selected_fields(&mut self, pages: &Pages) -> usize {
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        let paths: Vec<Path> =
            crate::edit::covered(&self.document, &from, &to).into_iter().map(|(path, _, _)| path).collect();
        self.update_fields_where(pages, Some((from.story, paths)))
    }

    fn update_fields_where(&mut self, pages: &Pages, only: Option<(StoryId, Vec<Path>)>) -> usize {
        let mut count = if only.is_none() { self.update_tocs(pages) } else { 0 };
        let page_map = body_pages(pages);
        let toc_paths: Vec<Path> =
            find_tocs(&self.document).into_iter().filter_map(|t| t.sdt).collect();
        // First every result, with the document as it is; then each written in.
        let mut results: Vec<(StoryId, Path, usize, String)> = Vec::new();
        {
            let mut env = self.field_env(pages);
            for story in story_order(&self.document) {
                let Some(blocks) = position::story(&self.document, &story) else { continue };
                for (path, paragraph) in position::paragraphs(blocks) {
                    if story == StoryId::Body && toc_paths.iter().any(|t| path.starts_with(t)) {
                        continue;
                    }
                    env.at = path.clone();
                    env.page = page_map.get(&path).copied().unwrap_or(1);
                    env.cells = table_cells(blocks, &path);
                    for (index, field) in field_spans(paragraph).into_iter().enumerate() {
                        if field.locked {
                            continue;
                        }
                        let wanted = only.as_ref().is_none_or(|(s, paths)| *s == story && paths.contains(&path));
                        if let Some(result) = fields::evaluate(&field.code, &mut env) {
                            if result != field.result && wanted {
                                results.push((story.clone(), path.clone(), index, result));
                            }
                        }
                    }
                }
            }
        }
        if results.is_empty() {
            return count;
        }
        count += results.len();
        self.change(Scope::Whole, move |document, selection, _| {
            for (story, path, index, result) in &results {
                if let Some(p) = position::paragraph_mut(document, story, path) {
                    set_field_result(p, *index, result);
                }
            }
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        count
    }

    /// Replace every field in the selection with its result, as Word's Unlink Field does.
    pub fn unlink_fields(&mut self) -> usize {
        let mut count = 0;
        self.change(Scope::Story, |document, selection, _| {
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            for (path, _, _) in crate::edit::covered(document, &from, &to) {
                if let Some(p) = position::paragraph_mut(document, &from.story, &path) {
                    count += unlink_in(p);
                }
            }
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        count
    }

    /// Lock or unlock the fields in the selection (`w:fldLock`), so Update Fields leaves them alone.
    pub fn lock_fields(&mut self, lock: bool) -> usize {
        let mut count = 0;
        self.change(Scope::Story, |document, selection, _| {
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            for (path, _, _) in crate::edit::covered(document, &from, &to) {
                let Some(p) = position::paragraph_mut(document, &from.story, &path) else {
                    continue;
                };
                for run in &mut p.runs {
                    if let Content::FieldChar(el) = &mut run.content {
                        if el.attr("w:fldCharType") == Some("begin") {
                            if lock {
                                el.set_attr("w:fldLock", "1");
                            } else {
                                el.remove_attr("w:fldLock");
                            }
                            count += 1;
                        }
                    }
                }
            }
            selection.clone()
        });
        count
    }

    /// Insert a caption at the caret's paragraph: a `Caption` paragraph above or below it reading
    /// `<label> <SEQ number>` and the text, numbered among the captions of that label.
    pub fn insert_caption(&mut self, label: &str, text: &str, below: bool, pages: &Pages) {
        let _ = crate::builtin::ensure_style(&mut self.document, "Caption");
        let label = label.to_owned();
        let text = text.to_owned();
        self.change(Scope::Whole, move |document, selection, _| {
            let mut paragraph = Paragraph::new();
            paragraph.props.put(El::new("w:pStyle").with("w:val", "Caption"));
            paragraph.runs.push(Run::text(format!("{label} "), no_props()));
            push_field(&mut paragraph, &format!(" SEQ {label} \\* ARABIC "), "1");
            if !text.is_empty() {
                paragraph.runs.push(Run::text(format!(" {text}"), no_props()));
            }
            let at = selection.head.clone();
            let Some(story) = position::story_mut(document, &at.story) else {
                return selection.clone();
            };
            let Some((container, index)) = position::container_mut(story, &at.path) else {
                return selection.clone();
            };
            let place = if below { index + 1 } else { index };
            container.insert(place, Block::Paragraph(paragraph));
            let mut path = at.path.clone();
            *path.last_mut().expect("a path") = place as u32;
            let length = position::paragraph(document, &at.story, &path).map_or(0, Paragraph::len);
            Selection::caret(Pos { story: at.story, path, offset: length })
        });
        // The new caption's number, and every caption after it, are worked out again.
        self.update_fields(pages);
    }

    /// Insert a cross reference at the caret to a heading, a bookmark, a note or a caption, as Word
    /// writes one: a hidden `_Ref` bookmark on the target and a `REF`, `PAGEREF` or `NOTEREF` field. `what`
    /// is `text`, `page`, `number` or `above-below`.
    pub fn insert_cross_reference(
        &mut self,
        target: &Path,
        what: &str,
        pages: &Pages,
    ) -> Result<(), String> {
        let paragraph = position::paragraph(&self.document, &StoryId::Body, target)
            .ok_or("there is no paragraph there to refer to")?;
        let existing = paragraph.marks.iter().find_map(|m| {
            (m.el.name == "w:bookmarkStart")
                .then(|| m.el.attr("w:name"))
                .flatten()
                .filter(|n| n.starts_with("_Ref"))
                .map(str::to_owned)
        });
        let note = paragraph.runs.iter().any(|r| {
            matches!(&r.content, Content::Object(el) if el.name == "w:footnoteReference" || el.name == "w:endnoteReference")
        });
        let name =
            existing.unwrap_or_else(|| format!("_Ref{}", 100_000_000 + self.document.next_id()));
        let target = target.clone();
        let what = what.to_owned();
        let code = match what.as_str() {
            "page" => format!(" PAGEREF {name} \\h "),
            "number" if note => format!(" NOTEREF {name} \\h "),
            "number" => format!(" REF {name} \\r \\h "),
            "above-below" => format!(" REF {name} \\p \\h "),
            _ => format!(" REF {name} \\h "),
        };
        self.change(Scope::Whole, move |document, selection, _| {
            let id = document.next_id();
            if let Some(p) = position::paragraph_mut(document, &StoryId::Body, &target) {
                if !p.marks.iter().any(|m| m.el.attr("w:name") == Some(name.as_str())) {
                    let length = p.len();
                    p.marks.push(Mark {
                        offset: 0,
                        el: El::new("w:bookmarkStart")
                            .with("w:id", id.to_string())
                            .with("w:name", name.clone()),
                        wraps: Vec::new(),
                    });
                    p.marks.push(Mark {
                        offset: length,
                        el: El::new("w:bookmarkEnd").with("w:id", id.to_string()),
                        wraps: Vec::new(),
                    });
                    p.marks.sort_by_key(|m| m.offset);
                }
            }
            let at = selection.head.clone();
            let Some(p) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let index = p.split_at(at.offset);
            let mut field = Paragraph::new();
            push_field(&mut field, &code, "?");
            let length = field.len();
            for (k, run) in field.runs.into_iter().enumerate() {
                p.runs.insert(index + k, run);
            }
            for mark in &mut p.marks {
                if mark.offset > at.offset {
                    mark.offset += length;
                }
            }
            Selection::caret(at.at(at.offset + length))
        });
        self.update_fields(pages);
        Ok(())
    }

    /// Change the hyperlink at the caret: its address (or `#bookmark`), its tooltip and, when given,
    /// the text it shows.
    pub fn edit_hyperlink(
        &mut self,
        address: &str,
        text: Option<&str>,
        tooltip: Option<&str>,
    ) -> Result<(), String> {
        let at = self.selection.head.clone();
        let paragraph =
            position::paragraph(&self.document, &at.story, &at.path).ok_or("no paragraph")?;
        let (index, _) = paragraph.run_at(at.offset.max(1)).ok_or("the caret is not in a link")?;
        let wrap = paragraph
            .runs
            .get(index)
            .and_then(|r| r.wraps.iter().find(|w| w.el.name == "w:hyperlink").cloned())
            .ok_or("the caret is not in a link")?;
        let address = address.to_owned();
        let text = text.map(str::to_owned);
        let tooltip = tooltip.map(str::to_owned);
        self.change(Scope::Whole, move |document, selection, _| {
            let main = document.main.clone();
            let mut el = wrap.el.clone();
            el.remove_attr("r:id");
            el.remove_attr("w:anchor");
            match address.strip_prefix('#') {
                Some(anchor) => el.set_attr("w:anchor", anchor),
                None => {
                    let id = document.package.rels_mut(&main).add(
                        crate::package::kind::HYPERLINK,
                        &address,
                        true,
                    );
                    el.set_attr("r:id", id);
                }
            }
            match &tooltip {
                Some(tip) if !tip.is_empty() => el.set_attr("w:tooltip", tip.clone()),
                _ => el.remove_attr("w:tooltip"),
            }
            let replacement = Wrap::new(el);
            let Some(p) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let mut first: Option<usize> = None;
            for (i, run) in p.runs.iter_mut().enumerate() {
                if let Some(slot) = run.wraps.iter_mut().find(|w| w.id == wrap.id) {
                    *slot = Arc::clone(&replacement);
                    first.get_or_insert(i);
                }
            }
            if let (Some(text), Some(first)) = (&text, first) {
                let props = Arc::clone(&p.runs[first].props);
                let wraps = p.runs[first].wraps.clone();
                p.runs.retain(|run| !run.wraps.iter().any(|w| w.id == replacement.id));
                p.runs.insert(
                    first,
                    Run {
                        props,
                        content: Content::Text(text.clone()),
                        wraps,
                        attrs: Arc::default(),
                    },
                );
            }
            selection.clone()
        });
        Ok(())
    }

    /// Take the hyperlink off the text at the caret, leaving the text.
    pub fn remove_hyperlink(&mut self) -> Result<(), String> {
        let at = self.selection.head.clone();
        let paragraph =
            position::paragraph(&self.document, &at.story, &at.path).ok_or("no paragraph")?;
        let (index, _) = paragraph.run_at(at.offset.max(1)).ok_or("the caret is not in a link")?;
        let id = paragraph
            .runs
            .get(index)
            .and_then(|r| r.wraps.iter().find(|w| w.el.name == "w:hyperlink").map(|w| w.id))
            .ok_or("the caret is not in a link")?;
        self.change(Scope::Story, move |document, selection, _| {
            if let Some(p) = position::paragraph_mut(document, &at.story, &at.path) {
                for run in &mut p.runs {
                    run.wraps.retain(|w| w.id != id);
                    // The Hyperlink style goes with the link.
                    if run.props.val_of("w:rStyle") == Some("Hyperlink") {
                        let mut props = (*run.props).clone();
                        props.remove_children("w:rStyle");
                        run.props = Arc::new(props);
                    }
                }
                p.merge_runs();
            }
            selection.clone()
        });
        Ok(())
    }

    /// The address of the hyperlink at a position: the web address, or `#bookmark`.
    pub fn hyperlink_at(&self, at: &Pos) -> Option<(String, Option<String>)> {
        let paragraph = position::paragraph(&self.document, &at.story, &at.path)?;
        let (index, _) = paragraph.run_at(at.offset.max(1))?;
        let el = paragraph.runs.get(index)?.link()?;
        let tooltip = el.attr("w:tooltip").map(str::to_owned);
        if let Some(anchor) = el.attr("w:anchor") {
            return Some((format!("#{anchor}"), tooltip));
        }
        let part = match &at.story {
            StoryId::Header(id) => self.document.headers.iter().find(|h| &h.id == id)?.name.clone(),
            _ => self.document.main.clone(),
        };
        let rel = self.document.package.rels.get(&part)?.by_id(el.attr("r:id")?)?;
        Some((rel.target.clone(), tooltip))
    }
}

/// Every field in the body: the paragraph it is in (numbered from 1, as `read` numbers them), its code
/// and its result.
pub fn field_list(document: &Document) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (n, (_, paragraph)) in position::paragraphs(&document.body).into_iter().enumerate() {
        for field in field_spans(paragraph) {
            out.push((n + 1, field.code.trim().to_owned(), field.result));
        }
    }
    out
}

/// The page each body paragraph starts on, by its path.
fn body_pages(pages: &Pages) -> HashMap<Path, u32> {
    let mut page_map: HashMap<Path, u32> = HashMap::new();
    for page in &pages.pages {
        for line in page.carets.iter().filter(|l| l.story == StoryId::Body) {
            page_map.entry(line.path.clone()).or_insert(page.number);
        }
    }
    page_map
}

impl Session {
    /// What a field update works from: the file, the pages, the counts and the time now.
    fn field_env(&self, pages: &Pages) -> Env<'_> {
        let stats = self.stats(Some(pages));
        let file = self.file.as_ref().map(|path| {
            let name =
                path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            (name, path.display().to_string())
        });
        let file_size = self.file.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
        Env {
            document: Some(&self.document),
            now: crate::session::now_iso(),
            file,
            file_size,
            pages: body_pages(pages),
            page_count: pages.pages.len() as u32,
            words: stats.words,
            characters: stats.characters,
            ..Env::default()
        }
    }

    /// Every field of the body in order, its code and the result the evaluator works out for it, `None`
    /// for a field it keeps the stored result of. A test compares these with Word's results.
    pub fn evaluated_fields(&self, pages: &Pages) -> Vec<(String, Option<String>)> {
        let mut env = self.field_env(pages);
        let mut out = Vec::new();
        for (path, paragraph) in position::paragraphs(&self.document.body) {
            env.at = path.clone();
            env.page = env.pages.get(&path).copied().unwrap_or(1);
            env.cells = table_cells(&self.document.body, &path);
            for field in field_spans(paragraph) {
                let result = fields::evaluate(&field.code, &mut env);
                out.push((field.code.trim().to_owned(), result));
            }
        }
        out
    }
}

/// The page a body paragraph starts on.
fn page_of(pages: &Pages, path: &[u32]) -> u32 {
    pages.find(&Pos::new(path.to_vec(), 0)).map_or(1, |(index, _)| pages.pages[index].number)
}

/// The stories in the order Update Fields walks them: the body, the headers and footers, the
/// footnotes, the endnotes, the text boxes.
fn story_order(document: &Document) -> Vec<StoryId> {
    let mut out = vec![StoryId::Body];
    out.extend(document.headers.iter().map(|h| StoryId::Header(h.id.clone())));
    for (part, make) in [
        (&document.footnotes, StoryId::Footnote as fn(i64) -> StoryId),
        (&document.endnotes, StoryId::Endnote),
    ] {
        if let Some(part) = part {
            out.extend(part.notes.iter().filter_map(|n| n.id()).filter(|id| *id > 0).map(make));
        }
    }
    out.extend(document.textboxes.iter().map(|t| StoryId::TextBox(t.id)));
    out
}

/// One field in a paragraph: its code, its stored result and whether it is locked.
#[derive(Debug, Clone)]
struct FieldSpan {
    code: String,
    result: String,
    locked: bool,
}

/// The fields of a paragraph in order: complex fields (begin, code, separate, result, end), outermost
/// only, and simple fields (`w:fldSimple`).
fn field_spans(paragraph: &Paragraph) -> Vec<FieldSpan> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut current: Option<(String, String, bool, bool)> = None;
    let mut simple_seen: Vec<u64> = Vec::new();
    for run in &paragraph.runs {
        if let Some(simple) = run.wraps.iter().find(|w| w.el.name == "w:fldSimple") {
            if depth == 0 && !simple_seen.contains(&simple.id) {
                simple_seen.push(simple.id);
                let result: String = paragraph
                    .runs
                    .iter()
                    .filter(|r| r.wraps.iter().any(|w| w.id == simple.id))
                    .filter_map(|r| match &r.content {
                        Content::Text(t) => Some(t.as_str()),
                        _ => None,
                    })
                    .collect();
                out.push(FieldSpan {
                    code: simple.el.attr("w:instr").unwrap_or("").to_owned(),
                    result,
                    locked: simple.el.attr("w:fldLock").is_some_and(|v| v == "1" || v == "true"),
                });
            }
            continue;
        }
        match &run.content {
            Content::FieldChar(el) => match el.attr("w:fldCharType") {
                Some("begin") => {
                    if depth == 0 {
                        let locked = el.attr("w:fldLock").is_some_and(|v| v == "1" || v == "true");
                        current = Some((String::new(), String::new(), false, locked));
                    }
                    depth += 1;
                }
                Some("separate") if depth == 1 => {
                    if let Some(c) = current.as_mut() {
                        c.2 = true;
                    }
                }
                // An end with no begin in this paragraph closes a field that started in an earlier one.
                Some("end") if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some((code, result, _, locked)) = current.take() {
                            out.push(FieldSpan { code, result, locked });
                        }
                    }
                }
                _ => {}
            },
            Content::Instr(text) if depth == 1 => {
                if let Some(c) = current.as_mut() {
                    if !c.2 {
                        c.0.push_str(text);
                    }
                }
            }
            Content::Text(text) if depth == 1 => {
                if let Some(c) = current.as_mut() {
                    if c.2 {
                        c.1.push_str(text);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Write a new result into the `index`th field of a paragraph (in `field_spans`' order): the first result
/// run takes the text and keeps its formatting, the rest go.
fn set_field_result(paragraph: &mut Paragraph, index: usize, result: &str) {
    let mut seen = 0;
    let mut depth = 0;
    let mut in_result = false;
    let mut simple_seen: Vec<u64> = Vec::new();
    let mut target_simple: Option<u64> = None;
    let mut begin_props: Option<Arc<El>> = None;
    let mut first_result: Option<usize> = None;
    let mut separate_at: Option<usize> = None;
    let mut remove: Vec<usize> = Vec::new();
    let mut done = false;
    for (i, run) in paragraph.runs.iter().enumerate() {
        if done {
            break;
        }
        if let Some(simple) = run.wraps.iter().find(|w| w.el.name == "w:fldSimple") {
            if depth == 0 {
                if !simple_seen.contains(&simple.id) {
                    simple_seen.push(simple.id);
                    if seen == index {
                        target_simple = Some(simple.id);
                    }
                    seen += 1;
                }
                if target_simple == Some(simple.id) {
                    if first_result.is_none() {
                        first_result = Some(i);
                    } else {
                        remove.push(i);
                    }
                }
            }
            continue;
        }
        if target_simple.is_some() {
            done = true;
            continue;
        }
        match &run.content {
            Content::FieldChar(el) => match el.attr("w:fldCharType") {
                Some("begin") => {
                    if depth == 0 && seen == index {
                        begin_props = Some(Arc::clone(&run.props));
                    }
                    depth += 1;
                }
                Some("separate") if depth == 1 && begin_props.is_some() => {
                    in_result = true;
                    separate_at = Some(i);
                }
                Some("end") if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        if begin_props.is_some() {
                            done = true;
                        }
                        seen += 1;
                        in_result = false;
                    }
                }
                _ => {}
            },
            _ if in_result && depth == 1 => {
                if matches!(run.content, Content::Text(_)) && first_result.is_none() {
                    first_result = Some(i);
                } else {
                    remove.push(i);
                }
            }
            _ => {}
        }
    }
    let shrink: Vec<usize> = remove.clone();
    match first_result {
        Some(i) => paragraph.runs[i].content = Content::Text(result.to_owned()),
        None => {
            if let Some(at) = separate_at {
                let props = begin_props.unwrap_or_else(no_props);
                let wraps = paragraph.runs[at].wraps.clone();
                paragraph.runs.insert(
                    at + 1,
                    Run {
                        props,
                        content: Content::Text(result.to_owned()),
                        wraps,
                        attrs: Arc::default(),
                    },
                );
                for mark in &mut paragraph.marks {
                    let _ = mark;
                }
                return;
            }
        }
    }
    for i in shrink.into_iter().rev() {
        let i = if first_result.is_none() { i + 1 } else { i };
        if i < paragraph.runs.len() {
            paragraph.runs.remove(i);
        }
    }
}

/// Replace each field in a paragraph with its result. Answers how many. A mark after a removed code keeps
/// its place in the text that is left.
fn unlink_in(paragraph: &mut Paragraph) -> usize {
    let mut count = 0;
    let mut depth = 0;
    let mut in_code = false;
    let mut offset = 0;
    // Where each removed run started, and how long it was.
    let mut removed: Vec<(usize, usize)> = Vec::new();
    let mut kept = Vec::with_capacity(paragraph.runs.len());
    for run in std::mem::take(&mut paragraph.runs) {
        let length = run.len();
        let start = offset;
        offset += length;
        let drop = match &run.content {
            Content::FieldChar(el) => {
                match el.attr("w:fldCharType") {
                    Some("begin") => {
                        depth += 1;
                        in_code = true;
                    }
                    Some("separate") => in_code = false,
                    _ if depth > 0 => {
                        depth -= 1;
                        if depth == 0 {
                            count += 1;
                        }
                    }
                    _ => {}
                }
                true
            }
            Content::Instr(_) => true,
            _ => depth > 0 && in_code,
        };
        if drop {
            removed.push((start, length));
            continue;
        }
        let mut run = run;
        let before = run.wraps.len();
        run.wraps.retain(|w| w.el.name != "w:fldSimple");
        if run.wraps.len() != before {
            count += 1;
        }
        kept.push(run);
    }
    paragraph.runs = kept;
    for mark in &mut paragraph.marks {
        let shift: usize = removed
            .iter()
            .map(|&(start, length)| {
                if start + length <= mark.offset {
                    length
                } else {
                    mark.offset.saturating_sub(start).min(length)
                }
            })
            .sum();
        mark.offset -= shift;
    }
    paragraph.merge_runs();
    count
}

/// The texts of the cells of the table a paragraph is in, and its row and column, for `=` fields.
fn table_cells(blocks: &[Block], path: &[u32]) -> Option<(Vec<Vec<String>>, usize, usize)> {
    let (table_path, row, column) = position::cell_of(blocks, path)?;
    let Some(Block::Table(table)) = position::block(blocks, &table_path) else { return None };
    let cells = table
        .rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| {
                    position::paragraphs(&c.blocks)
                        .iter()
                        .map(|(_, p)| crate::session::clean_text(&p.visible_text()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect()
        })
        .collect();
    Some((cells, row as usize, column as usize))
}

/// Add a complex field to the end of a paragraph: begin, code, separate, result, end.
fn push_field(paragraph: &mut Paragraph, code: &str, result: &str) {
    let char_run = |kind: &str| Run {
        props: no_props(),
        content: Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", kind)),
        wraps: Vec::new(),
        attrs: Arc::default(),
    };
    paragraph.runs.push(char_run("begin"));
    paragraph
        .runs
        .push(Run { content: Content::Instr(code.to_owned()), ..Run::text("", no_props()) });
    paragraph.runs.push(char_run("separate"));
    paragraph.runs.push(Run::text(result, no_props()));
    paragraph.runs.push(char_run("end"));
}

/// The content control Word puts a table of contents in.
fn toc_control() -> El {
    let mut el = El::new("w:sdt");
    let mut properties = El::new("w:sdtPr");
    properties.children.push(Node::El(El::new("w:id").with("w:val", "1953260000")));
    properties.children.push(Node::El(
        El::new("w:docPartObj")
            .with_child(El::new("w:docPartGallery").with("w:val", "Table of Contents"))
            .with_child(El::new("w:docPartUnique")),
    ));
    el.children.push(Node::El(properties));
    el.children.push(Node::El(El::new("w:sdtEndPr")));
    el
}

/// The blocks of a table of contents: its heading (when `heading`), an entry paragraph each (the first
/// opening the `TOC` field), and a paragraph closing the field. Each entry links to a `_Toc` bookmark the
/// heading is given here when it has none.
fn toc_blocks(
    document: &mut Document,
    options: &TocOptions,
    code: &str,
    entries: &[(u8, String, u32, Vec<u32>)],
    width: f32,
    heading: bool,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    if heading && options.caption.is_none() {
        let mut title = Paragraph::new();
        title.props.put(El::new("w:pStyle").with("w:val", "TOCHeading"));
        title.runs.push(Run::text("Contents", no_props()));
        blocks.push(Block::Paragraph(title));
    }
    let char_run = |kind: &str| Run {
        props: no_props(),
        content: Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", kind)),
        wraps: Vec::new(),
        attrs: Arc::default(),
    };
    let leader = (options.leader != "none").then(|| options.leader.clone());
    for (index, (level, text, page, path)) in entries.iter().enumerate() {
        let bookmark = toc_bookmark(document, path);
        let mut entry = Paragraph::new();
        let style = if options.caption.is_some() {
            "TableofFigures".to_owned()
        } else {
            format!("TOC{level}")
        };
        entry.props.put(El::new("w:pStyle").with("w:val", style));
        if options.page_numbers && options.right_align {
            let mut tab = El::new("w:tab")
                .with("w:val", "right")
                .with("w:pos", ((width - 0.5) * 20.0).round().to_string());
            if let Some(leader) = &leader {
                tab.set_attr("w:leader", leader.clone());
            }
            entry.props.put(El::new("w:tabs").with_child(tab));
        }
        if index == 0 {
            entry.runs.push(char_run("begin"));
            entry.runs.push(Run {
                content: Content::Instr(code.to_owned()),
                ..Run::text("", no_props())
            });
            entry.runs.push(char_run("separate"));
        }
        let link = options.hyperlinks.then(|| {
            Wrap::new(
                El::new("w:hyperlink").with("w:anchor", bookmark.clone()).with("w:history", "1"),
            )
        });
        let mut pieces: Vec<Run> = vec![Run::text(text.clone(), no_props())];
        if options.page_numbers {
            pieces.push(Run { content: Content::Tab, ..Run::text("", no_props()) });
            let mut number = Paragraph::new();
            push_field(&mut number, &format!(" PAGEREF {bookmark} \\h "), &page.to_string());
            pieces.extend(number.runs);
        }
        for mut run in pieces {
            if let Some(link) = &link {
                run.wraps.push(Arc::clone(link));
            }
            entry.runs.push(run);
        }
        blocks.push(Block::Paragraph(entry));
    }
    if entries.is_empty() {
        let mut none = Paragraph::new();
        none.runs.push(char_run("begin"));
        none.runs
            .push(Run { content: Content::Instr(code.to_owned()), ..Run::text("", no_props()) });
        none.runs.push(char_run("separate"));
        none.runs.push(Run::text("No table of contents entries found.", no_props()));
        blocks.push(Block::Paragraph(none));
    }
    let mut end = Paragraph::new();
    end.runs.push(char_run("end"));
    blocks.push(Block::Paragraph(end));
    blocks
}

/// The `_Toc` bookmark on a heading, added when it has none.
fn toc_bookmark(document: &mut Document, path: &[u32]) -> String {
    let existing = position::paragraph(document, &StoryId::Body, path).and_then(|p| {
        p.marks.iter().find_map(|m| {
            (m.el.name == "w:bookmarkStart")
                .then(|| m.el.attr("w:name"))
                .flatten()
                .filter(|n| n.starts_with("_Toc"))
                .map(str::to_owned)
        })
    });
    if let Some(name) = existing {
        return name;
    }
    let id = document.next_id();
    let name = format!("_Toc{}", 190_000_000 + id);
    if let Some(p) = position::paragraph_mut(document, &StoryId::Body, path) {
        let length = p.len();
        p.marks.push(Mark {
            offset: 0,
            el: El::new("w:bookmarkStart")
                .with("w:id", id.to_string())
                .with("w:name", name.clone()),
            wraps: Vec::new(),
        });
        p.marks.push(Mark {
            offset: length,
            el: El::new("w:bookmarkEnd").with("w:id", id.to_string()),
            wraps: Vec::new(),
        });
        p.marks.sort_by_key(|m| m.offset);
    }
    name
}

/// Put a table of contents' entries back where it stands, keeping its heading and its field code.
fn replace_toc(
    document: &mut Document,
    place: &TocPlace,
    options: &TocOptions,
    entries: &[(u8, String, u32, Vec<u32>)],
    width: f32,
) {
    match &place.sdt {
        Some(sdt) => {
            let has_heading = match position::block(&document.body, sdt) {
                Some(Block::Sdt(s)) => s.blocks.iter().any(|b| {
                    matches!(b, Block::Paragraph(p) if p.props.val_of("w:pStyle").is_some_and(|s| s.eq_ignore_ascii_case("TOCHeading")))
                }),
                _ => false,
            };
            let mut blocks = toc_blocks(document, options, &place.code, entries, width, false);
            if let Some(Block::Sdt(s)) = position::block_mut(&mut document.body, sdt) {
                if has_heading {
                    let heading = s.blocks.iter().find(|b| {
                        matches!(b, Block::Paragraph(p) if p.props.val_of("w:pStyle").is_some_and(|s| s.eq_ignore_ascii_case("TOCHeading")))
                    }).cloned();
                    if let Some(heading) = heading {
                        blocks.insert(0, heading);
                    }
                }
                s.blocks = blocks;
            }
        }
        None => {
            let blocks = toc_blocks(document, options, &place.code, entries, width, false);
            let (from, to) = (
                *place.first.last().unwrap_or(&0) as usize,
                *place.last.last().unwrap_or(&0) as usize,
            );
            if let Some((container, _)) = position::container_mut(&mut document.body, &place.first)
            {
                let to = to.min(container.len().saturating_sub(1));
                container.splice(from..=to, blocks);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toc_options_go_to_a_code_and_back() {
        let options = TocOptions { levels: 2, page_numbers: false, ..TocOptions::default() };
        let back = TocOptions::of(&options.code());
        assert_eq!((back.levels, back.page_numbers, back.hyperlinks), (2, false, true));
        let figures = TocOptions { caption: Some("Figure".into()), ..TocOptions::default() };
        assert_eq!(TocOptions::of(&figures.code()).caption.as_deref(), Some("Figure"));
    }

    #[test]
    fn a_fields_result_is_replaced_keeping_its_formatting() {
        let mut p = Paragraph::new();
        push_field(&mut p, " DATE ", "1/1/2000");
        set_field_result(&mut p, 0, "10/24/2026");
        assert_eq!(p.visible_text(), "10/24/2026");
        assert_eq!(field_spans(&p)[0].code, " DATE ");
    }

    #[test]
    fn unlinking_leaves_the_result_as_text() {
        let mut p = Paragraph::new();
        p.runs.push(Run::text("Page ", no_props()));
        push_field(&mut p, " PAGE ", "4");
        assert_eq!(unlink_in(&mut p), 1);
        assert_eq!(p.flat_text(), "Page 4");
        assert!(field_spans(&p).is_empty());
    }
}
