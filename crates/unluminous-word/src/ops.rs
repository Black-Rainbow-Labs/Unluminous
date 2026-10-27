//! The commands that build structure: lists, tables, sections, headers and footers, notes, comments,
//! links, bookmarks and the table of contents.
//!
//! Each is a method of [`Session`], so it is one undo step and the format bar, the panel and the agent's
//! `plugins run word` reach it the same way.

use std::sync::Arc;

use crate::builtin::{self, ListKind};
use crate::edit;
use crate::model::*;
use crate::package::{kind, Package};
use crate::position::{self, Pos, Selection, StoryId};
use crate::props;
use crate::session::{now_iso, Scope, Session};
use crate::xml::{El, Node};

/// Page sizes Word lists, in points, portrait.
pub const PAGE_SIZES: &[(&str, f32, f32)] = &[
    ("letter", 612.0, 792.0),
    ("legal", 612.0, 1008.0),
    ("a4", 595.3, 841.9),
    ("a5", 419.6, 595.3),
    ("a3", 841.9, 1190.6),
    ("b5", 498.9, 708.7),
    ("executive", 522.0, 756.0),
    ("tabloid", 792.0, 1224.0),
];

/// Word's margin presets, in points: top, bottom, left, right.
pub const MARGINS: &[(&str, [f32; 4])] = &[
    ("normal", [72.0, 72.0, 72.0, 72.0]),
    ("narrow", [36.0, 36.0, 36.0, 36.0]),
    ("moderate", [72.0, 72.0, 54.0, 54.0]),
    ("wide", [72.0, 72.0, 144.0, 144.0]),
];

fn twips(points: f32) -> String {
    ((points * 20.0).round() as i64).to_string()
}

impl Session {
    // ---- Lists ----

    /// Turn the selected paragraphs into a list of `kind`, or take them out of it when they already are
    /// one. A list directly above, of the same kind, is continued, which is what Word does.
    pub fn toggle_list(&mut self, kind: ListKind) {
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        let paths: Vec<Vec<u32>> =
            edit::covered(&self.document, &from, &to).into_iter().map(|(p, _, _)| p).collect();
        let resolver = crate::styles::Resolver::new(&self.document);
        let all_in_kind = paths.iter().all(|path| {
            position::paragraph(&self.document, &from.story, path)
                .and_then(|p| resolver.numbering_of(&p.props))
                .is_some_and(|(num, _)| builtin::kind_of(&self.document, num) == Some(kind))
        });
        // The list just above the first paragraph, when it is of this kind.
        let above = position::story(&self.document, &from.story)
            .and_then(|blocks| position::previous_paragraph(blocks, &from.path))
            .and_then(|path| position::paragraph(&self.document, &from.story, &path))
            .and_then(|p| resolver.numbering_of(&p.props))
            .filter(|(num, _)| builtin::kind_of(&self.document, *num) == Some(kind))
            .map(|(num, _)| num);
        drop(resolver);
        self.change(Scope::Whole, move |document, selection, _| {
            if all_in_kind {
                for path in &paths {
                    if let Some(p) = position::paragraph_mut(document, &from.story, path) {
                        p.props.remove_children("w:numPr");
                        if p.props.val_of("w:pStyle") == Some("ListParagraph") {
                            p.props.remove_children("w:pStyle");
                        }
                    }
                    // A paragraph whose style puts it in a list is taken out with numId 0.
                    let by_style =
                        position::paragraph(document, &from.story, path).is_some_and(|p| {
                            crate::styles::Resolver::new(document).numbering_of(&p.props).is_some()
                        });
                    if by_style {
                        if let Some(p) = position::paragraph_mut(document, &from.story, path) {
                            p.props.put(
                                El::new("w:numPr")
                                    .with_child(El::new("w:numId").with("w:val", "0")),
                            );
                        }
                    }
                }
                return selection.clone();
            }
            let num = above.unwrap_or_else(|| builtin::new_list(document, kind));
            let _ = builtin::ensure_style(document, "ListParagraph");
            for path in &paths {
                if let Some(p) = position::paragraph_mut(document, &from.story, path) {
                    let level = p
                        .props
                        .child("w:numPr")
                        .and_then(|n| n.val_of("w:ilvl"))
                        .unwrap_or("0")
                        .to_owned();
                    p.props.put(
                        El::new("w:numPr")
                            .with_child(El::new("w:ilvl").with("w:val", level))
                            .with_child(El::new("w:numId").with("w:val", num.to_string())),
                    );
                    // Word's list buttons give a Normal paragraph the List Paragraph style, which is
                    // what draws a list without the space Normal puts after each paragraph.
                    if matches!(p.props.val_of("w:pStyle"), None | Some("Normal")) {
                        p.props.put(El::new("w:pStyle").with("w:val", "ListParagraph"));
                    }
                }
            }
            selection.clone()
        });
    }

    /// Move the selected list paragraphs one level deeper or shallower, as Tab and Shift+Tab do in a list.
    /// Answers false when no selected paragraph is in a list, so Tab can type a tab instead.
    pub fn change_list_level(&mut self, deeper: bool) -> bool {
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        let resolver = crate::styles::Resolver::new(&self.document);
        let lists: Vec<(Vec<u32>, i64, u8)> = edit::covered(&self.document, &from, &to)
            .into_iter()
            .filter_map(|(path, _, _)| {
                let p = position::paragraph(&self.document, &from.story, &path)?;
                let (num, level) = resolver.numbering_of(&p.props)?;
                Some((path, num, level))
            })
            .collect();
        drop(resolver);
        if lists.is_empty() {
            return false;
        }
        self.change(Scope::Story, move |document, selection, _| {
            for (path, num, level) in &lists {
                let level = if deeper { (level + 1).min(8) } else { level.saturating_sub(1) };
                if let Some(p) = position::paragraph_mut(document, &from.story, path) {
                    p.props.put(
                        El::new("w:numPr")
                            .with_child(El::new("w:ilvl").with("w:val", level.to_string()))
                            .with_child(El::new("w:numId").with("w:val", num.to_string())),
                    );
                }
            }
            selection.clone()
        });
        true
    }

    /// Start the numbering of the list the caret is in again at `start` from this paragraph on.
    pub fn restart_list(&mut self, start: i64) {
        let head = self.selection.head.clone();
        let found = position::paragraph(&self.document, &head.story, &head.path)
            .and_then(|p| crate::styles::Resolver::new(&self.document).numbering_of(&p.props));
        let Some((num, level)) = found else { return };
        self.change(Scope::Whole, move |document, selection, _| {
            let Some(abstract_id) = document.numbering.as_ref().and_then(|part| {
                let numbering = crate::numbering::Numbering::new(Some(&part.root));
                numbering.abstract_of(num).map(|(id, _)| id)
            }) else {
                return selection.clone();
            };
            let part = document.numbering.as_mut().expect("there is a list");
            let new_id = part
                .root
                .children_named("w:num")
                .filter_map(|el| el.attr("w:numId")?.parse::<i64>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            let num_el = El::new("w:num")
                .with("w:numId", new_id.to_string())
                .with_child(El::new("w:abstractNumId").with("w:val", abstract_id.to_string()))
                .with_child(
                    El::new("w:lvlOverride")
                        .with("w:ilvl", level.to_string())
                        .with_child(El::new("w:startOverride").with("w:val", start.to_string())),
                );
            let last = part
                .root
                .children
                .iter()
                .rposition(|n| matches!(n, Node::El(el) if el.name == "w:num"));
            match last {
                Some(at) => part.root.children.insert(at + 1, Node::El(num_el)),
                None => part.root.children.push(Node::El(num_el)),
            }
            part.dirty = true;
            // This paragraph and the rest of the list after it move to the new instance.
            let story = selection.head.story.clone();
            let paths: Vec<Vec<u32>> = position::story(document, &story)
                .map(|blocks| position::paragraphs(blocks).into_iter().map(|(p, _)| p).collect())
                .unwrap_or_default();
            let mut started = false;
            for path in paths {
                if path == selection.head.path {
                    started = true;
                }
                if !started {
                    continue;
                }
                let in_list = position::paragraph(document, &story, &path)
                    .and_then(|p| p.props.child("w:numPr"))
                    .and_then(|n| n.val_of("w:numId"))
                    .and_then(|v| v.parse::<i64>().ok())
                    == Some(num);
                if !in_list {
                    continue;
                }
                if let Some(p) = position::paragraph_mut(document, &story, &path) {
                    if let Some(n) = p.props.child_mut("w:numPr") {
                        n.put(El::new("w:numId").with("w:val", new_id.to_string()));
                    }
                }
            }
            selection.clone()
        });
    }

    // ---- Tables ----

    /// Insert a table of `rows` by `columns` at the caret, as wide as the text, with the caret in its
    /// first cell.
    pub fn insert_table(&mut self, rows: usize, columns: usize) {
        let width = self
            .section_at_caret()
            .map_or(468.0, |s| crate::layout::section::Geometry::of(&s).text_width());
        let has_grid = builtin::ensure_style(&mut self.document, "TableGrid").is_ok();
        let table = edit::new_table(rows, columns, width, has_grid.then_some("TableGrid"));
        self.change(Scope::Whole, move |document, selection, _| {
            let at = selection.head.clone();
            match edit::insert_block(document, &at, Block::Table(table)) {
                Some(mut path) => {
                    path.extend([0, 0, 0]);
                    Selection::caret(Pos { story: at.story, path, offset: 0 })
                }
                None => selection.clone(),
            }
        });
    }

    /// The table, row and cell the caret is in.
    pub fn cell_at_caret(&self) -> Option<(Vec<u32>, u32, u32)> {
        let head = &self.selection.head;
        position::cell_of(position::story(&self.document, &head.story)?, &head.path)
    }

    /// One table operation at the caret: `row-above`, `row-below`, `column-left`, `column-right`,
    /// `delete-row`, `delete-column`, `delete-table`, `merge-right`, `merge-down`, `split`, `header-row`.
    pub fn table_op(&mut self, op: &str) -> Result<(), String> {
        let (table_path, row, cell) = self.cell_at_caret().ok_or("the caret is not in a table")?;
        let op = op.to_owned();
        let mut failure = None;
        self.change(Scope::Story, |document, selection, _| {
            let story = selection.head.story.clone();
            let Some(Block::Table(table)) = position::story_mut(document, &story)
                .and_then(|b| position::block_mut(b, &table_path))
            else {
                failure = Some("the table has gone".to_owned());
                return selection.clone();
            };
            let (r, c) = (row as usize, cell as usize);
            let caret_in = |r: usize, c: usize| {
                let mut path = table_path.clone();
                path.extend([r as u32, c as u32, 0]);
                Selection::caret(Pos { story: story.clone(), path, offset: 0 })
            };
            let empty_cell = |template: &Cell| Cell {
                attrs: Vec::new(),
                props: {
                    let mut p = template.props.clone();
                    p.remove_children("w:vMerge");
                    p.remove_children("w:gridSpan");
                    p
                },
                blocks: vec![Block::Paragraph(Paragraph {
                    props: first_paragraph_props(template),
                    ..Paragraph::new()
                })],
            };
            match op.as_str() {
                "row-above" | "row-below" => {
                    let template = table.rows[r].clone();
                    let new = Row {
                        cells: template.cells.iter().map(empty_cell).collect(),
                        attrs: Vec::new(),
                        ..template
                    };
                    let at = if op == "row-above" { r } else { r + 1 };
                    table.rows.insert(at, new);
                    return caret_in(at, c.min(table.rows[at].cells.len() - 1));
                }
                "column-left" | "column-right" => {
                    let at = if op == "column-left" { c } else { c + 1 };
                    let columns = table.columns();
                    let width = columns.get(c).copied().unwrap_or(1440);
                    let mut grid = Vec::new();
                    for (index, w) in columns.iter().enumerate() {
                        if index == at {
                            grid.push(width);
                        }
                        grid.push(*w);
                    }
                    if at >= columns.len() {
                        grid.push(width);
                    }
                    table.grid.children = grid
                        .iter()
                        .map(|w| Node::El(El::new("w:gridCol").with("w:w", w.to_string())))
                        .collect();
                    for row in &mut table.rows {
                        let template = row
                            .cells
                            .get(c.min(row.cells.len().saturating_sub(1)))
                            .cloned()
                            .unwrap_or_default();
                        let new = empty_cell(&template);
                        let at = at.min(row.cells.len());
                        row.cells.insert(at, new);
                    }
                    return caret_in(r, at);
                }
                "delete-row" => {
                    if table.rows.len() == 1 {
                        return remove_block(document, &story, &table_path, selection);
                    }
                    table.rows.remove(r);
                    let r = r.min(table.rows.len() - 1);
                    return caret_in(r, c.min(table.rows[r].cells.len() - 1));
                }
                "delete-column" => {
                    if table.rows.iter().all(|row| row.cells.len() <= 1) {
                        return remove_block(document, &story, &table_path, selection);
                    }
                    let mut columns = table.columns();
                    if c < columns.len() {
                        columns.remove(c);
                    }
                    table.grid.children = columns
                        .iter()
                        .map(|w| Node::El(El::new("w:gridCol").with("w:w", w.to_string())))
                        .collect();
                    for row in &mut table.rows {
                        if c < row.cells.len() && row.cells.len() > 1 {
                            row.cells.remove(c);
                        }
                    }
                    return caret_in(r, c.saturating_sub(1).min(table.rows[r].cells.len() - 1));
                }
                "delete-table" => return remove_block(document, &story, &table_path, selection),
                "merge-right" => {
                    if c + 1 >= table.rows[r].cells.len() {
                        failure = Some("there is no cell to the right to merge with".into());
                        return selection.clone();
                    }
                    let next = table.rows[r].cells.remove(c + 1);
                    let this = &mut table.rows[r].cells[c];
                    let span = span_of(this) + span_of(&next);
                    this.props.put(El::new("w:gridSpan").with("w:val", span.to_string()));
                    // The text of the cell merged in follows, as Word keeps it.
                    let has_text =
                        next.blocks.iter().any(|b| b.as_paragraph().is_none_or(|p| !p.is_empty()));
                    if has_text {
                        this.blocks.extend(next.blocks);
                    }
                    return caret_in(r, c);
                }
                "merge-down" => {
                    if r + 1 >= table.rows.len() {
                        failure = Some("there is no row below to merge with".into());
                        return selection.clone();
                    }
                    if table.rows[r].cells[c].props.child("w:vMerge").is_none() {
                        table.rows[r].cells[c]
                            .props
                            .put(El::new("w:vMerge").with("w:val", "restart"));
                    }
                    if let Some(below) = table.rows[r + 1].cells.get_mut(c) {
                        below.props.put(El::new("w:vMerge"));
                        below.blocks = vec![Block::Paragraph(Paragraph::new())];
                    }
                    return caret_in(r, c);
                }
                "split" => {
                    let this = &mut table.rows[r].cells[c];
                    let span = span_of(this);
                    if span > 1 {
                        this.props.remove_children("w:gridSpan");
                        let template = this.clone();
                        for k in 1..span {
                            table.rows[r].cells.insert(c + k, empty_cell(&template));
                        }
                    }
                    return caret_in(r, c);
                }
                "header-row" => {
                    let row = &mut table.rows[r];
                    let on = props::on_off(&row.props, "w:tblHeader") != Some(true);
                    props::set_on_off(&mut row.props, "w:tblHeader", on.then_some(true));
                    return selection.clone();
                }
                other => failure = Some(format!("there is no table operation `{other}`")),
            }
            selection.clone()
        });
        match failure {
            Some(problem) => {
                self.undo();
                Err(problem)
            }
            None => Ok(()),
        }
    }

    /// Set the width of the caret's column, in points.
    pub fn set_column_width(&mut self, points: f32) -> Result<(), String> {
        let (table_path, _, cell) = self.cell_at_caret().ok_or("the caret is not in a table")?;
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            if let Some(Block::Table(table)) = position::story_mut(document, &story)
                .and_then(|b| position::block_mut(b, &table_path))
            {
                let mut columns = table.columns();
                if let Some(slot) = columns.get_mut(cell as usize) {
                    *slot = (points * 20.0).round() as i64;
                }
                table.grid.children = columns
                    .iter()
                    .map(|w| Node::El(El::new("w:gridCol").with("w:w", w.to_string())))
                    .collect();
                for row in &mut table.rows {
                    if let Some(c) = row.cells.get_mut(cell as usize) {
                        c.props
                            .put(El::new("w:tcW").with("w:w", twips(points)).with("w:type", "dxa"));
                    }
                }
            }
            selection.clone()
        });
        Ok(())
    }

    /// Shade the caret's cell, or clear it with `None`.
    pub fn shade_cell(&mut self, color: Option<props::Rgb>) -> Result<(), String> {
        let (table_path, row, cell) = self.cell_at_caret().ok_or("the caret is not in a table")?;
        self.change(Scope::Story, move |document, selection, _| {
            let story = selection.head.story.clone();
            if let Some(Block::Table(table)) = position::story_mut(document, &story)
                .and_then(|b| position::block_mut(b, &table_path))
            {
                if let Some(c) =
                    table.rows.get_mut(row as usize).and_then(|r| r.cells.get_mut(cell as usize))
                {
                    match color {
                        Some(rgb) => c.props.put(
                            El::new("w:shd")
                                .with("w:val", "clear")
                                .with("w:color", "auto")
                                .with("w:fill", rgb.hex()),
                        ),
                        None => c.props.remove_children("w:shd"),
                    }
                }
            }
            selection.clone()
        });
        Ok(())
    }

    // ---- Sections ----

    /// The `w:sectPr` of the section the caret is in.
    pub fn section_at_caret(&self) -> Option<El> {
        Some(self.section_path().map_or_else(
            || self.document.section.clone(),
            |path| {
                position::paragraph(&self.document, &StoryId::Body, &path)
                    .and_then(|p| p.props.child("w:sectPr").cloned())
                    .unwrap_or_else(|| self.document.section.clone())
            },
        ))
    }

    /// The body paragraph whose `w:sectPr` ends the caret's section, or `None` for the last section.
    fn section_path(&self) -> Option<Vec<u32>> {
        let head = &self.selection.head;
        let top = if head.story == StoryId::Body { *head.path.first()? } else { 0 };
        self.document.body.iter().enumerate().skip(top as usize).find_map(|(index, block)| {
            block.as_paragraph().and_then(|p| p.props.child("w:sectPr")).map(|_| vec![index as u32])
        })
    }

    /// Change the caret's section's `w:sectPr`.
    pub fn change_section(&mut self, change: impl FnOnce(&mut El)) {
        let path = self.section_path();
        self.change(Scope::Whole, move |document, selection, _| {
            let sect = match &path {
                Some(path) => position::paragraph_mut(document, &StoryId::Body, path)
                    .and_then(|p| p.props.child_mut("w:sectPr")),
                None => Some(&mut document.section),
            };
            if let Some(sect) = sect {
                change(sect);
            }
            selection.clone()
        });
    }

    /// Page size by name (`letter`, `a4`...) or as `<width>x<height>` points, keeping the orientation.
    pub fn set_page_size(&mut self, size: &str) -> Result<(), String> {
        let (w, h) = match PAGE_SIZES.iter().find(|(name, _, _)| name.eq_ignore_ascii_case(size)) {
            Some((_, w, h)) => (*w, *h),
            None => {
                let (w, h) = size.split_once(['x', 'X']).ok_or_else(|| {
                    format!(
                        "{size} is not a page size: use one of {} or WIDTHxHEIGHT in points",
                        PAGE_SIZES.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")
                    )
                })?;
                (
                    w.trim().parse::<f32>().map_err(|e| e.to_string())?,
                    h.trim().parse::<f32>().map_err(|e| e.to_string())?,
                )
            }
        };
        let landscape = self
            .section_at_caret()
            .map(|s| crate::layout::section::Geometry::of(&s).landscape())
            .unwrap_or(false);
        let (w, h) = if landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
        self.change_section(move |sect| {
            let pg = sect.child_or_insert("w:pgSz");
            pg.set_attr("w:w", twips(w));
            pg.set_attr("w:h", twips(h));
            if landscape {
                pg.set_attr("w:orient", "landscape");
            } else {
                pg.remove_attr("w:orient");
            }
        });
        Ok(())
    }

    /// Portrait or landscape, swapping the page's width and height and its margins round.
    pub fn set_orientation(&mut self, landscape: bool) {
        let geometry =
            crate::layout::section::Geometry::of(&self.section_at_caret().unwrap_or_default());
        if geometry.landscape() == landscape {
            return;
        }
        self.change_section(move |sect| {
            let pg = sect.child_or_insert("w:pgSz");
            pg.set_attr("w:w", twips(geometry.height));
            pg.set_attr("w:h", twips(geometry.width));
            if landscape {
                pg.set_attr("w:orient", "landscape");
            } else {
                pg.remove_attr("w:orient");
            }
            let mar = sect.child_or_insert("w:pgMar");
            // Turning the page turns its margins with it, as Word does.
            let (top, bottom, left, right) =
                (geometry.top, geometry.bottom, geometry.left, geometry.right);
            let (t, b, l, r) =
                if landscape { (left, right, bottom, top) } else { (right, left, top, bottom) };
            mar.set_attr("w:top", twips(t));
            mar.set_attr("w:bottom", twips(b));
            mar.set_attr("w:left", twips(l));
            mar.set_attr("w:right", twips(r));
        });
    }

    /// Margins by preset name, or `[top, bottom, left, right]` in points.
    pub fn set_margins(&mut self, margins: [f32; 4]) {
        self.change_section(move |sect| {
            let mar = sect.child_or_insert("w:pgMar");
            for (name, value) in ["w:top", "w:bottom", "w:left", "w:right"].iter().zip(margins) {
                mar.set_attr(name, twips(value));
            }
            for (name, fallback) in [("w:header", 720), ("w:footer", 720), ("w:gutter", 0)] {
                if mar.attr(name).is_none() {
                    mar.set_attr(name, fallback.to_string());
                }
            }
        });
    }

    /// How many equal columns the section has.
    pub fn set_columns(&mut self, count: u32) {
        self.change_section(move |sect| {
            let cols = sect.child_or_insert("w:cols");
            cols.children.clear();
            cols.set_attr("w:space", "720");
            if count <= 1 {
                cols.remove_attr("w:num");
            } else {
                cols.set_attr("w:num", count.to_string());
            }
        });
    }

    /// Insert a section break at the caret: `nextPage`, `continuous`, `evenPage` or `oddPage`.
    pub fn insert_section_break(&mut self, kind: &str) {
        let current = self.section_at_caret().unwrap_or_else(|| El::new("w:sectPr"));
        let kind = kind.to_owned();
        let later = self.section_path();
        self.change(Scope::Whole, move |document, selection, _| {
            if selection.head.story != StoryId::Body {
                return selection.clone();
            }
            let next = edit::split_paragraph(document, &selection.head, None);
            let mut before = next.path.clone();
            *before.last_mut().expect("a path") -= 1;
            // The paragraph before the break ends a section with the settings the caret's section had; the
            // section after it is the one that starts with `kind`.
            let mut ended = current.clone();
            ended.remove_children("w:type");
            if let Some(p) = position::paragraph_mut(document, &StoryId::Body, &before) {
                p.props.put(ended);
            }
            let following = later.map(|mut path| {
                path[0] += 1;
                path
            });
            let sect = match &following {
                Some(path) => position::paragraph_mut(document, &StoryId::Body, path)
                    .and_then(|p| p.props.child_mut("w:sectPr")),
                None => Some(&mut document.section),
            };
            if let Some(sect) = sect {
                sect.put(El::new("w:type").with("w:val", kind.clone()));
            }
            Selection::caret(next)
        });
    }

    // ---- Headers and footers ----

    /// Set the text of the caret's section's header or footer (`default`, `first` or `even`), making the
    /// part when the section has none. A `{PAGE}` and `{NUMPAGES}` in the text become those fields.
    pub fn set_header_text(&mut self, footer: bool, which: &str, text: &str) {
        let which = which.to_owned();
        let text = text.to_owned();
        let section_path = self.section_path();
        self.change(Scope::Whole, move |document, selection, _| {
            let reference = if footer { "w:footerReference" } else { "w:headerReference" };
            let sect = match &section_path {
                Some(path) => position::paragraph(document, &StoryId::Body, path)
                    .and_then(|p| p.props.child("w:sectPr"))
                    .cloned(),
                None => Some(document.section.clone()),
            }
            .unwrap_or_else(|| El::new("w:sectPr"));
            let existing = sect
                .children_named(reference)
                .find(|r| r.attr("w:type").unwrap_or("default") == which)
                .and_then(|r| r.attr("r:id").map(str::to_owned));
            let style = if footer { "Footer" } else { "Header" };
            let _ = builtin::ensure_style(document, style);
            let paragraph = header_paragraph(style, &text);
            let id = match existing {
                Some(id) => id,
                None => {
                    let id = new_header_part(document, footer);
                    let sect = match &section_path {
                        Some(path) => position::paragraph_mut(document, &StoryId::Body, path)
                            .and_then(|p| p.props.child_mut("w:sectPr")),
                        None => Some(&mut document.section),
                    };
                    if let Some(sect) = sect {
                        sect.children.insert(
                            0,
                            Node::El(
                                El::new(reference)
                                    .with("w:type", which.clone())
                                    .with("r:id", id.clone()),
                            ),
                        );
                    }
                    if which == "first" {
                        if let Some(sect) = section_sect_mut(document, &section_path) {
                            props::set_on_off(sect, "w:titlePg", Some(true));
                        }
                    }
                    if which == "even" {
                        if let Some(settings) = document.settings.as_mut() {
                            props::set_on_off(
                                &mut settings.root,
                                "w:evenAndOddHeaders",
                                Some(true),
                            );
                            settings.dirty = true;
                        }
                    }
                    id
                }
            };
            if let Some(header) = document.headers.iter_mut().find(|h| h.id == id) {
                replace_header_text(&mut header.story.blocks, paragraph);
                header.dirty = true;
            }
            selection.clone()
        });
    }

    // ---- Notes ----

    /// Insert a footnote (or an endnote) at the caret holding `text`, numbered automatically.
    pub fn insert_note(&mut self, endnote: bool, text: &str) {
        let text = text.to_owned();
        for style in ["FootnoteText", "FootnoteReference", "EndnoteText", "EndnoteReference"] {
            let _ = builtin::ensure_style(&mut self.document, style);
        }
        self.change(Scope::Whole, move |document, selection, tracking| {
            let id = ensure_notes_part(document, endnote);
            let (text_style, reference_style, reference, own) = if endnote {
                ("EndnoteText", "EndnoteReference", "w:endnoteReference", "w:endnoteRef")
            } else {
                ("FootnoteText", "FootnoteReference", "w:footnoteReference", "w:footnoteRef")
            };
            let reference_props = Arc::new(
                El::new("w:rPr").with_child(El::new("w:rStyle").with("w:val", reference_style)),
            );
            let mut paragraph = Paragraph::new();
            paragraph.props.put(El::new("w:pStyle").with("w:val", text_style));
            paragraph.runs.push(Run {
                content: Content::Object(El::new(own)),
                ..Run::text("", Arc::clone(&reference_props))
            });
            paragraph.runs.push(Run::text(format!(" {text}"), no_props()));
            let note = Story {
                el: El::new(if endnote { "w:endnote" } else { "w:footnote" })
                    .with("w:id", id.to_string()),
                blocks: vec![Block::Paragraph(paragraph)],
            };
            let part =
                if endnote { document.endnotes.as_mut() } else { document.footnotes.as_mut() };
            if let Some(part) = part {
                part.notes.push(note);
                part.dirty = true;
            }
            let at = selection.ordered().1.clone();
            let Some(p) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let index = p.split_at(at.offset);
            let mut run = Run {
                content: Content::Object(El::new(reference).with("w:id", id.to_string())),
                ..Run::text("", reference_props)
            };
            if let Some(t) = tracking {
                let ins = document.next_id();
                run.wraps.push(Wrap::new(
                    El::new("w:ins")
                        .with("w:id", ins.to_string())
                        .with("w:author", t.author.clone())
                        .with("w:date", t.date.clone()),
                ));
            }
            let len = run.len();
            let Some(p) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            p.runs.insert(index, run);
            for mark in &mut p.marks {
                if mark.offset > at.offset {
                    mark.offset += len;
                }
            }
            // The caret goes into the new note, after its text, as Word's Insert Footnote leaves it.
            let story = if endnote { StoryId::Endnote(id) } else { StoryId::Footnote(id) };
            let end = position::paragraph(document, &story, &[0]).map_or(0, Paragraph::len);
            Selection::caret(Pos { story, path: vec![0], offset: end })
        });
    }

    /// Turn the footnote whose reference is at the caret into an endnote, or an endnote into a
    /// footnote: the note's text moves to the other part under a new number, its styles and its own
    /// reference mark change kind, and the reference in the text names it.
    pub fn convert_note(&mut self) -> Result<(), String> {
        let at = self.selection.ordered().0.clone();
        let paragraph = position::paragraph(&self.document, &at.story, &at.path)
            .ok_or("the caret is not in text")?;
        let mut offset = 0;
        let mut found = None;
        for run in &paragraph.runs {
            if (offset == at.offset || offset + run.len() == at.offset) && found.is_none() {
                if let Content::Object(el) = &run.content {
                    if matches!(el.name.as_str(), "w:footnoteReference" | "w:endnoteReference") {
                        found = Some((
                            el.name == "w:endnoteReference",
                            el.attr("w:id").and_then(|i| i.parse::<i64>().ok()),
                        ));
                    }
                }
            }
            offset += run.len();
        }
        let (from_endnote, Some(old_id)) =
            found.ok_or("the caret is not at a footnote or endnote reference")?
        else {
            return Err("the note has no number".into());
        };
        for style in ["FootnoteText", "FootnoteReference", "EndnoteText", "EndnoteReference"] {
            let _ = builtin::ensure_style(&mut self.document, style);
        }
        self.change(Scope::Whole, move |document, selection, _| {
            let to_endnote = !from_endnote;
            let source =
                if from_endnote { document.endnotes.as_mut() } else { document.footnotes.as_mut() };
            let Some(source) = source else { return selection.clone() };
            let Some(index) = source.notes.iter().position(|n| n.id() == Some(old_id)) else {
                return selection.clone();
            };
            let mut note = source.notes.remove(index);
            source.dirty = true;
            let new_id = ensure_notes_part(document, to_endnote);
            let (kind, text_style, reference_style, own) = if to_endnote {
                ("w:endnote", "EndnoteText", "EndnoteReference", "w:endnoteRef")
            } else {
                ("w:footnote", "FootnoteText", "FootnoteReference", "w:footnoteRef")
            };
            note.el.name = kind.into();
            note.el.set_attr("w:id", new_id.to_string());
            let (old_text, old_reference) = if from_endnote {
                ("EndnoteText", "EndnoteReference")
            } else {
                ("FootnoteText", "FootnoteReference")
            };
            for block in &mut note.blocks {
                let Block::Paragraph(p) = block else { continue };
                if p.props.val_of("w:pStyle") == Some(old_text) {
                    p.props.put(El::new("w:pStyle").with("w:val", text_style));
                }
                for run in &mut p.runs {
                    if let Content::Object(el) = &mut run.content {
                        if matches!(el.name.as_str(), "w:footnoteRef" | "w:endnoteRef") {
                            el.name = own.into();
                        }
                    }
                    if run.props.val_of("w:rStyle") == Some(old_reference) {
                        let mut props = (*run.props).clone();
                        props.put(El::new("w:rStyle").with("w:val", reference_style));
                        run.props = Arc::new(props);
                    }
                }
            }
            let target =
                if to_endnote { document.endnotes.as_mut() } else { document.footnotes.as_mut() };
            if let Some(target) = target {
                target.notes.push(note);
                target.dirty = true;
            }
            if let Some(p) = position::paragraph_mut(document, &at.story, &at.path) {
                for run in &mut p.runs {
                    if let Content::Object(el) = &mut run.content {
                        let name =
                            if from_endnote { "w:endnoteReference" } else { "w:footnoteReference" };
                        if el.name == name && el.attr("w:id") == Some(old_id.to_string().as_str()) {
                            el.name = if to_endnote {
                                "w:endnoteReference"
                            } else {
                                "w:footnoteReference"
                            }
                            .into();
                            el.set_attr("w:id", new_id.to_string());
                            if run.props.val_of("w:rStyle") == Some(old_reference) {
                                let mut props = (*run.props).clone();
                                props.put(El::new("w:rStyle").with("w:val", reference_style));
                                run.props = Arc::new(props);
                            }
                        }
                    }
                }
            }
            selection.clone()
        });
        Ok(())
    }

    /// The note whose reference is at or just before `at`: its story.
    pub fn note_at(&self, at: &Pos) -> Option<StoryId> {
        let paragraph = position::paragraph(&self.document, &at.story, &at.path)?;
        let mut offset = 0;
        for run in &paragraph.runs {
            if offset == at.offset || offset + run.len() == at.offset {
                if let Content::Object(el) = &run.content {
                    let id = el.attr("w:id").and_then(|i| i.parse::<i64>().ok());
                    match (el.name.as_str(), id) {
                        ("w:footnoteReference", Some(id)) => return Some(StoryId::Footnote(id)),
                        ("w:endnoteReference", Some(id)) => return Some(StoryId::Endnote(id)),
                        _ => {}
                    }
                }
            }
            offset += run.len();
        }
        None
    }

    /// Empty the header or footer the caret is in, keeping its paragraph's formatting, as Word's Remove
    /// Header does.
    pub fn remove_header_at_caret(&mut self) -> Result<(), String> {
        let StoryId::Header(rid) = self.selection.head.story.clone() else {
            return Err("the caret is not in a header or footer".into());
        };
        self.change(Scope::Story, move |document, selection, _| {
            if let Some(blocks) = position::story_mut(document, &StoryId::Header(rid.clone())) {
                let props = blocks
                    .iter()
                    .find_map(|b| b.as_paragraph())
                    .map(|p| p.props.clone())
                    .unwrap_or_default();
                let mut paragraph = Paragraph::new();
                paragraph.props = props;
                *blocks = vec![Block::Paragraph(paragraph)];
            }
            Selection::caret(Pos { story: selection.head.story.clone(), path: vec![0], offset: 0 })
        });
        Ok(())
    }

    // ---- Comments ----

    /// Add a comment on the selection (or the word at the caret) by the session's author. Answers its id.
    pub fn add_comment(&mut self, text: &str) -> i64 {
        // Restrict Editing to comments lets a comment through.
        let permitted = self.protection.as_deref() == Some("comments") && !self.read_only;
        self.permit = permitted;
        let id = self.add_comment_inner(text);
        self.permit = false;
        id
    }

    fn add_comment_inner(&mut self, text: &str) -> i64 {
        let text = text.to_owned();
        let author =
            if self.author.is_empty() { "Unluminous".to_owned() } else { self.author.clone() };
        for style in ["CommentText", "CommentReference"] {
            let _ = builtin::ensure_style(&mut self.document, style);
        }
        let mut made = 0;
        self.change(Scope::Whole, |document, selection, _| {
            let id = ensure_comments_part(document);
            made = id;
            let initials: String =
                author.split_whitespace().filter_map(|w| w.chars().next()).collect();
            let mut paragraph = Paragraph::new();
            paragraph.props.put(El::new("w:pStyle").with("w:val", "CommentText"));
            let para_id = random_para_id(id);
            paragraph.attrs.push(("w14:paraId".into(), para_id.clone()));
            add_comment_to_side_parts(document, &para_id, &author);
            let reference_props = Arc::new(
                El::new("w:rPr").with_child(El::new("w:rStyle").with("w:val", "CommentReference")),
            );
            paragraph.runs.push(Run {
                content: Content::Object(El::new("w:annotationRef")),
                ..Run::text("", Arc::clone(&reference_props))
            });
            paragraph.runs.push(Run::text(text.clone(), no_props()));
            let comment = Story {
                el: El::new("w:comment")
                    .with("w:id", id.to_string())
                    .with("w:author", author.clone())
                    .with("w:date", now_iso())
                    .with("w:initials", initials),
                blocks: vec![Block::Paragraph(paragraph)],
            };
            if let Some(part) = document.comments.as_mut() {
                part.notes.push(comment);
                part.dirty = true;
            }
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            // A comment with nothing selected covers the word at the caret.
            let (from, to) = if from == to {
                let text = position::paragraph(document, &from.story, &from.path)
                    .map(Paragraph::flat_text)
                    .unwrap_or_default();
                let start = edit::previous_word(&text, from.offset.min(text.len()));
                let end = text[from.offset.min(text.len())..]
                    .find(|c: char| !c.is_alphanumeric())
                    .map_or(text.len(), |n| from.offset + n);
                (from.at(start), from.at(end.max(from.offset)))
            } else {
                (from, to)
            };
            let mark = |name: &str| El::new(name).with("w:id", id.to_string());
            if let Some(p) = position::paragraph_mut(document, &from.story, &from.path) {
                p.marks.push(Mark {
                    offset: from.offset,
                    el: mark("w:commentRangeStart"),
                    wraps: Vec::new(),
                });
                p.marks.sort_by_key(|m| m.offset);
            }
            if let Some(p) = position::paragraph_mut(document, &to.story, &to.path) {
                p.marks.push(Mark {
                    offset: to.offset,
                    el: mark("w:commentRangeEnd"),
                    wraps: Vec::new(),
                });
                p.marks.sort_by_key(|m| m.offset);
                let index = p.split_at(to.offset);
                let run = Run {
                    content: Content::Object(mark("w:commentReference")),
                    ..Run::text("", reference_props)
                };
                p.runs.insert(index, run);
                // Marks after the reference move past it; the range end stays before it.
                for m in &mut p.marks {
                    if m.offset > to.offset {
                        m.offset += crate::model::OBJECT.len_utf8();
                    }
                }
            }
            selection.clone()
        });
        made
    }

    /// Every comment: its id, author, date, text, whether it is resolved, and the text it is on.
    pub fn comments(&self) -> Vec<serde_json::Value> {
        let Some(part) = &self.document.comments else { return Vec::new() };
        let done = resolved_comments(&self.document);
        part.notes
            .iter()
            .filter_map(|note| {
                let id = note.id()?;
                let text: Vec<String> = position::paragraphs(&note.blocks)
                    .iter()
                    .map(|(_, p)| crate::session::clean_text(&p.visible_text()))
                    .collect();
                let para_id = last_para_id(note);
                Some(serde_json::json!({
                    "id": id,
                    "author": note.el.attr("w:author").unwrap_or_default(),
                    "date": note.el.attr("w:date").unwrap_or_default(),
                    "text": text.join("\n").trim(),
                    "resolved": para_id.is_some_and(|p| done.contains(&p)),
                    "on": self.commented_text(id),
                }))
            })
            .collect()
    }

    /// The text a comment's range covers in the body.
    fn commented_text(&self, id: i64) -> String {
        let id = id.to_string();
        let mut out = String::new();
        let mut inside = false;
        for (_, p) in position::paragraphs(&self.document.body) {
            let text = p.flat_text();
            let start = p
                .marks
                .iter()
                .find(|m| m.el.name == "w:commentRangeStart" && m.el.attr("w:id") == Some(&id))
                .map(|m| m.offset);
            let end = p
                .marks
                .iter()
                .find(|m| m.el.name == "w:commentRangeEnd" && m.el.attr("w:id") == Some(&id))
                .map(|m| m.offset);
            match (start, end, inside) {
                (Some(s), Some(e), _) => {
                    return crate::session::clean_text(&text[s.min(text.len())..e.min(text.len())])
                }
                (Some(s), None, _) => {
                    out.push_str(&text[s.min(text.len())..]);
                    inside = true;
                }
                (None, Some(e), true) => {
                    out.push('\n');
                    out.push_str(&text[..e.min(text.len())]);
                    return crate::session::clean_text(&out);
                }
                (None, None, true) => {
                    out.push('\n');
                    out.push_str(&text);
                }
                _ => {}
            }
        }
        crate::session::clean_text(&out)
    }

    /// Reply to a comment, which Word keeps as another comment whose parent is the first.
    pub fn reply_to_comment(&mut self, parent: i64, text: &str) -> Result<i64, String> {
        let part = self.document.comments.as_ref().ok_or("the document has no comments")?;
        let note = part
            .notes
            .iter()
            .find(|n| n.id() == Some(parent))
            .ok_or_else(|| format!("there is no comment {parent}"))?;
        let parent_para = last_para_id(note).unwrap_or_else(|| random_para_id(parent));
        // A reply sits on the same range as its parent.
        let anchor = position::paragraphs(&self.document.body).into_iter().find_map(|(path, p)| {
            p.marks
                .iter()
                .find(|m| {
                    m.el.name == "w:commentRangeStart"
                        && m.el.attr("w:id") == Some(&parent.to_string())
                })
                .map(|m| Pos::new(path.clone(), m.offset))
        });
        if let Some(at) = anchor {
            self.selection = Selection::caret(at);
        }
        let id = self.add_comment(text);
        // Word needs the parent's paragraph to carry the id the reply names.
        ensure_para_id(&mut self.document, parent, &parent_para);
        set_comment_extended(&mut self.document, id, Some(&parent_para), false);
        Ok(id)
    }

    /// Change a comment's text. Its first paragraph keeps the annotation mark and its id, so a reply
    /// and the resolved state still find it; any other paragraphs it had go.
    pub fn edit_comment(&mut self, id: i64, text: &str) -> Result<(), String> {
        let exists = self
            .document
            .comments
            .as_ref()
            .is_some_and(|p| p.notes.iter().any(|n| n.id() == Some(id)));
        if !exists {
            return Err(format!("there is no comment {id}"));
        }
        let text = text.to_owned();
        self.change(Scope::Whole, move |document, selection, _| {
            let Some(part) = document.comments.as_mut() else { return selection.clone() };
            let Some(note) = part.notes.iter_mut().find(|n| n.id() == Some(id)) else {
                return selection.clone();
            };
            let first = note.blocks.iter().position(|b| matches!(b, Block::Paragraph(_)));
            if let Some(first) = first {
                note.blocks.truncate(first + 1);
                if let Block::Paragraph(p) = &mut note.blocks[first] {
                    let props = p
                        .runs
                        .iter()
                        .find(|r| matches!(r.content, Content::Text(_)))
                        .map_or_else(no_props, |r| Arc::clone(&r.props));
                    p.runs.retain(|r| matches!(&r.content, Content::Object(el) if el.name == "w:annotationRef"));
                    p.runs.push(Run::text(text.clone(), props));
                    p.marks.clear();
                }
            }
            part.dirty = true;
            selection.clone()
        });
        Ok(())
    }

    /// Delete every comment in the document. Answers how many.
    pub fn delete_all_comments(&mut self) -> usize {
        let ids: Vec<i64> = self
            .document
            .comments
            .as_ref()
            .map(|p| p.notes.iter().filter_map(Story::id).filter(|id| *id >= 0).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            return 0;
        }
        self.change(Scope::Whole, |document, selection, _| {
            let paras: Vec<String> = document
                .comments
                .as_ref()
                .map(|p| p.notes.iter().filter_map(last_para_id).collect())
                .unwrap_or_default();
            for para in paras {
                remove_comment_from_side_parts(document, &para);
            }
            for id in &ids {
                remove_comment_pieces(&mut document.body, &id.to_string());
            }
            if let Some(part) = document.comments.as_mut() {
                part.notes.retain(|n| n.id().is_none_or(|id| id < 0));
                part.dirty = true;
            }
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        ids.len()
    }

    /// Select the next (or previous) comment's range after the caret, wrapping round. Answers its id.
    pub fn go_to_comment(&mut self, forward: bool) -> Option<i64> {
        let mut starts: Vec<(Pos, Pos, i64)> = Vec::new();
        for (path, p) in position::paragraphs(&self.document.body) {
            for mark in p.marks.iter().filter(|m| m.el.name == "w:commentRangeStart") {
                let Some(id) = mark.el.attr("w:id").and_then(|i| i.parse().ok()) else { continue };
                let end = position::paragraphs(&self.document.body).into_iter().find_map(
                    |(end_path, q)| {
                        q.marks
                            .iter()
                            .find(|m| {
                                m.el.name == "w:commentRangeEnd"
                                    && m.el.attr("w:id") == mark.el.attr("w:id")
                            })
                            .map(|m| Pos::new(end_path.clone(), m.offset))
                    },
                );
                let start = Pos::new(path.clone(), mark.offset);
                starts.push((start.clone(), end.unwrap_or(start), id));
            }
        }
        if starts.is_empty() {
            return None;
        }
        let caret = self.selection.ordered().0.clone();
        let chosen = if forward {
            starts.iter().find(|(s, _, _)| *s > caret).or(starts.first())
        } else {
            starts.iter().rev().find(|(s, _, _)| *s < caret).or(starts.last())
        }?
        .clone();
        self.select(chosen.0, chosen.1);
        Some(chosen.2)
    }

    /// Mark a comment resolved, or open again.
    pub fn resolve_comment(&mut self, id: i64, done: bool) -> Result<(), String> {
        let part = self.document.comments.as_ref().ok_or("the document has no comments")?;
        let note = part
            .notes
            .iter()
            .find(|n| n.id() == Some(id))
            .ok_or_else(|| format!("there is no comment {id}"))?;
        let para = last_para_id(note).unwrap_or_else(|| random_para_id(id));
        self.change(Scope::Whole, |document, selection, _| {
            ensure_para_id(document, id, &para);
            set_comment_extended(document, id, None, done);
            selection.clone()
        });
        Ok(())
    }

    /// Delete a comment, its range and its reference.
    pub fn delete_comment(&mut self, id: i64) -> Result<(), String> {
        let exists = self
            .document
            .comments
            .as_ref()
            .is_some_and(|p| p.notes.iter().any(|n| n.id() == Some(id)));
        if !exists {
            return Err(format!("there is no comment {id}"));
        }
        self.change(Scope::Whole, |document, selection, _| {
            let key = id.to_string();
            let para = document
                .comments
                .as_ref()
                .and_then(|p| p.notes.iter().find(|n| n.id() == Some(id)))
                .and_then(last_para_id);
            if let Some(para) = para {
                remove_comment_from_side_parts(document, &para);
            }
            if let Some(part) = document.comments.as_mut() {
                part.notes.retain(|n| n.id() != Some(id));
                part.dirty = true;
            }
            remove_comment_pieces(&mut document.body, &key);
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        Ok(())
    }

    // ---- Links and bookmarks ----

    /// Make the selection a link to `target`, a web address or `#bookmark`. With nothing selected the
    /// address itself is typed and linked.
    pub fn insert_hyperlink(&mut self, target: &str, text: Option<&str>) {
        let _ = builtin::ensure_style(&mut self.document, "Hyperlink");
        if self.selection.is_empty() {
            let shown = text.unwrap_or(target).to_owned();
            let start = self.selection.head.clone();
            self.type_text(&shown);
            self.selection = Selection { anchor: start, head: self.selection.head.clone() };
        }
        let target = target.to_owned();
        self.change(Scope::Whole, move |document, selection, _| {
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            let mut link = El::new("w:hyperlink").with("w:history", "1");
            match target.strip_prefix('#') {
                Some(anchor) => link.set_attr("w:anchor", anchor),
                None => {
                    let main = document.main.clone();
                    let id = document.package.rels_mut(&main).add(kind::HYPERLINK, &target, true);
                    link.set_attr("r:id", id);
                }
            }
            let wrap = Wrap::new(link);
            for (path, len, _) in edit::covered(document, &from, &to) {
                let lo = if path == from.path { from.offset } else { 0 };
                let hi = if path == to.path { to.offset } else { len };
                let Some(p) = position::paragraph_mut(document, &from.story, &path) else {
                    continue;
                };
                let first = p.split_at(lo);
                let last = p.split_at(hi);
                for run in &mut p.runs[first..last] {
                    run.wraps.retain(|w| w.el.name != "w:hyperlink");
                    run.wraps.insert(0, Arc::clone(&wrap));
                    let mut props = run.props.as_ref().clone();
                    props::set_val(&mut props, "w:rStyle", Some("Hyperlink"));
                    run.props = Arc::new(props);
                }
                p.merge_runs();
            }
            Selection::caret(to)
        });
    }

    /// Put a bookmark called `name` round the selection.
    pub fn insert_bookmark(&mut self, name: &str) {
        let name = name.to_owned();
        self.change(Scope::Story, move |document, selection, _| {
            let id = next_mark_id(document, "w:bookmarkStart");
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            if let Some(p) = position::paragraph_mut(document, &from.story, &from.path) {
                p.marks.push(Mark {
                    offset: from.offset,
                    el: El::new("w:bookmarkStart")
                        .with("w:id", id.to_string())
                        .with("w:name", name.clone()),
                    wraps: Vec::new(),
                });
                p.marks.sort_by_key(|m| m.offset);
            }
            if let Some(p) = position::paragraph_mut(document, &to.story, &to.path) {
                p.marks.push(Mark {
                    offset: to.offset,
                    el: El::new("w:bookmarkEnd").with("w:id", id.to_string()),
                    wraps: Vec::new(),
                });
                p.marks.sort_by_key(|m| m.offset);
            }
            selection.clone()
        });
    }

    /// Every bookmark's name, with the position it starts at.
    pub fn bookmarks(&self) -> Vec<(String, Pos)> {
        position::paragraphs(&self.document.body)
            .into_iter()
            .flat_map(|(path, p)| {
                p.marks
                    .iter()
                    .filter(|m| m.el.name == "w:bookmarkStart")
                    .filter_map(|m| {
                        Some((m.el.attr("w:name")?.to_owned(), Pos::new(path.clone(), m.offset)))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|(name, _)| name != "_GoBack")
            .collect()
    }

    // ---- Fields and the table of contents ----

    /// Insert a field with `code` (`PAGE`, `DATE \@ "d MMMM yyyy"`, `NUMPAGES`...) showing `result`.
    pub fn insert_field(&mut self, code: &str, result: &str) {
        let code = format!(" {} ", code.trim());
        let result = result.to_owned();
        self.change(Scope::Story, move |document, selection, _| {
            let at = selection.head.clone();
            let (props, wraps) = position::paragraph(document, &at.story, &at.path)
                .map(|p| edit::props_at(p, at.offset))
                .unwrap_or_else(|| (no_props(), Vec::new()));
            let Some(p) = position::paragraph_mut(document, &at.story, &at.path) else {
                return selection.clone();
            };
            let index = p.split_at(at.offset);
            let run = |content: Content| Run {
                props: Arc::clone(&props),
                content,
                wraps: wraps.clone(),
                attrs: Arc::default(),
            };
            let runs = vec![
                run(Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "begin"))),
                run(Content::Instr(code.clone())),
                run(Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "separate"))),
                run(Content::Text(result.clone())),
                run(Content::FieldChar(El::new("w:fldChar").with("w:fldCharType", "end"))),
            ];
            let len: usize = runs.iter().map(Run::len).sum();
            p.runs.splice(index..index, runs);
            for mark in &mut p.marks {
                if mark.offset > at.offset {
                    mark.offset += len;
                }
            }
            Selection::caret(at.at(at.offset + len))
        });
    }

    // ---- Text ----

    /// Change the case of the selection: `upper`, `lower`, `sentence`, `title` or `toggle`.
    pub fn change_case(&mut self, how: &str) {
        let how = how.to_owned();
        self.change(Scope::Story, move |document, selection, _| {
            let (from, to) = (selection.ordered().0.clone(), selection.ordered().1.clone());
            for (path, len, _) in edit::covered(document, &from, &to) {
                let lo = if path == from.path { from.offset } else { 0 };
                let hi = if path == to.path { to.offset } else { len };
                let Some(p) = position::paragraph_mut(document, &from.story, &path) else {
                    continue;
                };
                let first = p.split_at(lo);
                let last = p.split_at(hi);
                let mut start_of_sentence = lo == 0;
                for run in &mut p.runs[first..last] {
                    if let Content::Text(text) = &mut run.content {
                        *text = recase(text, &how, &mut start_of_sentence);
                    }
                }
                p.merge_runs();
            }
            selection.clone()
        });
    }

    /// Clear all formatting of the selection: direct run formatting goes, and the paragraphs go back to
    /// `Normal`, which is Word's Clear All Formatting.
    pub fn clear_formatting(&mut self) {
        self.change(Scope::Story, |document, selection, _| {
            edit::format_range(document, &selection.anchor, &selection.head, &|rpr| {
                rpr.children.clear()
            });
            edit::format_paragraphs(document, &selection.anchor, &selection.head, &|ppr| {
                ppr.children.retain(
                    |n| matches!(n, Node::El(el) if el.name == "w:sectPr" || el.name == "w:rPr"),
                );
            });
            selection.clone()
        });
    }
}

fn first_paragraph_props(cell: &Cell) -> El {
    cell.blocks
        .iter()
        .find_map(Block::as_paragraph)
        .map(|p| p.props.clone())
        .unwrap_or_else(|| El::new("w:pPr"))
}

fn span_of(cell: &Cell) -> usize {
    cell.props.val_of("w:gridSpan").and_then(|v| v.parse().ok()).unwrap_or(1)
}

/// Take a block out and put the caret where it was.
fn remove_block(
    document: &mut Document,
    story: &StoryId,
    path: &[u32],
    selection: &Selection,
) -> Selection {
    let Some(blocks) = position::story_mut(document, story) else { return selection.clone() };
    let Some((container, index)) = position::container_mut(blocks, path) else {
        return selection.clone();
    };
    container.remove(index);
    if container.is_empty()
        || !matches!(
            container.get(index.min(container.len().saturating_sub(1))),
            Some(Block::Paragraph(_))
        )
    {
        container.insert(index.min(container.len()), Block::Paragraph(Paragraph::new()));
    }
    let mut at = path.to_vec();
    *at.last_mut().expect("a path") = index.min(container.len() - 1) as u32;
    Selection::caret(Pos { story: story.clone(), path: at, offset: 0 })
}

fn section_sect_mut<'a>(document: &'a mut Document, path: &Option<Vec<u32>>) -> Option<&'a mut El> {
    match path {
        Some(path) => position::paragraph_mut(document, &StoryId::Body, path)
            .and_then(|p| p.props.child_mut("w:sectPr")),
        None => Some(&mut document.section),
    }
}

/// Put `paragraph`'s runs in place of the text of the first paragraph of a header story that holds
/// text, keeping that paragraph's properties and everything else in the story: a logo, a table, the
/// other paragraphs (task-2359, D2). A story with no paragraph of text gets `paragraph` added at its end.
fn replace_header_text(blocks: &mut Vec<Block>, paragraph: Paragraph) {
    let target = blocks.iter_mut().find_map(|block| match block {
        Block::Paragraph(p) if !p.visible_text().trim().is_empty() => Some(p),
        _ => None,
    });
    match target {
        Some(existing) => {
            // A picture or a shape in the paragraph stays where it is; the text after it is replaced.
            existing.runs.retain(|run| {
                run.content.picture().is_some()
                    || matches!(run.content, Content::ParagraphObject(_))
            });
            existing.runs.extend(paragraph.runs);
            let length = existing.len();
            for mark in &mut existing.marks {
                mark.offset = mark.offset.min(length);
            }
        }
        // An empty header Word made is one empty paragraph, which the text goes into.
        None => match blocks.iter_mut().find_map(Block::as_paragraph_mut) {
            Some(existing) if existing.runs.is_empty() => existing.runs = paragraph.runs,
            _ => blocks.push(Block::Paragraph(paragraph)),
        },
    }
}

/// A header or footer paragraph, with `{PAGE}` and `{NUMPAGES}` as fields.
fn header_paragraph(style: &str, text: &str) -> Paragraph {
    let mut paragraph = Paragraph::new();
    paragraph.props.put(El::new("w:pStyle").with("w:val", style));
    let mut rest = text;
    while !rest.is_empty() {
        let next = ["{PAGE}", "{NUMPAGES}"]
            .iter()
            .filter_map(|token| rest.find(token).map(|at| (at, *token)))
            .min();
        match next {
            Some((at, token)) => {
                if at > 0 {
                    paragraph.runs.push(Run::text(&rest[..at], no_props()));
                }
                let code = token.trim_matches(['{', '}']);
                let field = |content| Run { content, ..Run::text("", no_props()) };
                paragraph.runs.push(field(Content::FieldChar(
                    El::new("w:fldChar").with("w:fldCharType", "begin"),
                )));
                paragraph.runs.push(field(Content::Instr(format!(" {code} "))));
                paragraph.runs.push(field(Content::FieldChar(
                    El::new("w:fldChar").with("w:fldCharType", "separate"),
                )));
                paragraph.runs.push(Run::text("1", no_props()));
                paragraph.runs.push(field(Content::FieldChar(
                    El::new("w:fldChar").with("w:fldCharType", "end"),
                )));
                rest = &rest[at + token.len()..];
            }
            None => {
                paragraph.runs.push(Run::text(rest, no_props()));
                rest = "";
            }
        }
    }
    paragraph
}

/// A new, empty header or footer part, registered with the package. Answers its relationship id.
/// A new, empty header or footer part with its relationship, for another module. Answers its id.
pub(crate) fn new_header_part_for(document: &mut Document, footer: bool) -> String {
    new_header_part(document, footer)
}

fn new_header_part(document: &mut Document, footer: bool) -> String {
    let stem = if footer { "word/footer" } else { "word/header" };
    let name = document.package.unused_name(stem, "xml");
    let main = document.main.clone();
    let target = Package::relative(&main, &name);
    let id = document.package.rels_mut(&main).add(
        if footer { kind::FOOTER } else { kind::HEADER },
        &target,
        false,
    );
    let content_type = if footer {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
    } else {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
    };
    document.package.set_override(&name, content_type);
    document.package.set_part(&name, Vec::new());
    let mut root = El::new(if footer { "w:ftr" } else { "w:hdr" });
    root.attrs = document.root.attrs.clone();
    root.declares = document.root.declares.clone();
    document.headers.push(HeaderPart {
        id: id.clone(),
        name,
        footer,
        story: Story { el: root, blocks: vec![Block::Paragraph(Paragraph::new())] },
        dirty: true,
    });
    id
}

/// The footnotes or endnotes part, made with Word's two separator notes when missing. Answers the next id.
fn ensure_notes_part(document: &mut Document, endnote: bool) -> i64 {
    let slot_missing =
        if endnote { document.endnotes.is_none() } else { document.footnotes.is_none() };
    if slot_missing {
        let (stem, rel, content_type, root_name, note_name) = if endnote {
            (
                "word/endnotes.xml",
                kind::ENDNOTES,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
                "w:endnotes",
                "w:endnote",
            )
        } else {
            (
                "word/footnotes.xml",
                kind::FOOTNOTES,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
                "w:footnotes",
                "w:footnote",
            )
        };
        let main = document.main.clone();
        let target = Package::relative(&main, stem);
        document.package.rels_mut(&main).add(rel, &target, false);
        document.package.set_override(stem, content_type);
        document.package.set_part(stem, Vec::new());
        let separator = |id: &str, kind: &str, el: &str| {
            let mut p = Paragraph::new();
            p.props.put(
                El::new("w:spacing")
                    .with("w:after", "0")
                    .with("w:line", "240")
                    .with("w:lineRule", "auto"),
            );
            p.runs.push(Run { content: Content::Object(El::new(el)), ..Run::text("", no_props()) });
            Story {
                el: El::new(note_name).with("w:type", kind).with("w:id", id),
                blocks: vec![Block::Paragraph(p)],
            }
        };
        let mut root = El::new(root_name);
        root.attrs = document.root.attrs.clone();
        root.declares = document.root.declares.clone();
        let part = NotesPart {
            name: stem.to_owned(),
            root,
            notes: vec![
                separator("-1", "separator", "w:separator"),
                separator("0", "continuationSeparator", "w:continuationSeparator"),
            ],
            dirty: true,
        };
        // The settings name the two separators; Word repairs a file whose settings do not.
        if let Some(settings) = document.settings.as_mut() {
            let (pr, child) =
                if endnote { ("w:endnotePr", "w:endnote") } else { ("w:footnotePr", "w:footnote") };
            if settings.root.child(pr).is_none() {
                let el = El::new(pr)
                    .with_child(El::new(child).with("w:id", "-1"))
                    .with_child(El::new(child).with("w:id", "0"));
                // Word writes these after `w:characterSpacingControl`; the settings part has no order table,
                // so it goes after that element when there is one.
                let at = settings
                    .root
                    .children
                    .iter()
                    .position(|n| matches!(n, Node::El(e) if e.name == "w:characterSpacingControl"))
                    .map_or(settings.root.children.len(), |i| i + 1);
                settings.root.children.insert(at, Node::El(el));
                settings.dirty = true;
            }
        }
        if endnote {
            document.endnotes = Some(part);
        } else {
            document.footnotes = Some(part);
        }
    }
    let part = if endnote { document.endnotes.as_ref() } else { document.footnotes.as_ref() };
    part.map_or(1, |p| p.notes.iter().filter_map(Story::id).max().unwrap_or(0) + 1).max(1)
}

/// The comments part, made when missing. Answers the next comment id.
fn ensure_comments_part(document: &mut Document) -> i64 {
    if document.comments.is_none() {
        let name = "word/comments.xml";
        let main = document.main.clone();
        let target = Package::relative(&main, name);
        document.package.rels_mut(&main).add(kind::COMMENTS, &target, false);
        document.package.set_override(
            name,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        );
        document.package.set_part(name, Vec::new());
        let mut root = El::new("w:comments");
        root.attrs = document.root.attrs.clone();
        root.declares = document.root.declares.clone();
        document.comments =
            Some(NotesPart { name: name.to_owned(), root, notes: Vec::new(), dirty: true });
    }
    document
        .comments
        .as_ref()
        .map_or(0, |p| p.notes.iter().filter_map(Story::id).max().map_or(0, |n| n + 1))
}

const COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

/// The `w14:paraId` of a comment's last paragraph, which `commentsExtended` refers to it by.
fn last_para_id(note: &Story) -> Option<String> {
    position::paragraphs(&note.blocks)
        .last()
        .and_then(|(_, p)| p.attrs.iter().find(|(k, _)| k == "w14:paraId").map(|(_, v)| v.clone()))
}

/// A paragraph id for a comment, eight hexadecimal digits below 0x80000000 as the schema requires.
fn random_para_id(seed: i64) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos()) as u64;
    format!(
        "{:08X}",
        (nanos.wrapping_mul(2_654_435_761).wrapping_add(seed as u64 * 40_503)) & 0x7FFF_FFFF
    )
}

fn ensure_para_id(document: &mut Document, comment: i64, para: &str) {
    let Some(part) = document.comments.as_mut() else { return };
    let Some(note) = part.notes.iter_mut().find(|n| n.id() == Some(comment)) else { return };
    if let Some(Block::Paragraph(p)) =
        note.blocks.iter_mut().rev().find(|b| matches!(b, Block::Paragraph(_)))
    {
        if !p.attrs.iter().any(|(k, _)| k == "w14:paraId") {
            p.attrs.push(("w14:paraId".into(), para.to_owned()));
            part.dirty = true;
        }
    }
}

/// The paragraph ids of the comments `commentsExtended` marks done.
fn resolved_comments(document: &Document) -> Vec<String> {
    let Some(root) =
        comments_extended(document).and_then(|(_, bytes)| crate::xml::parse_bytes(&bytes).ok())
    else {
        return Vec::new();
    };
    root.elements()
        .filter(|el| el.name == "w15:commentEx" && el.attr("w15:done") == Some("1"))
        .filter_map(|el| el.attr("w15:paraId").map(str::to_owned))
        .collect()
}

fn comments_extended(document: &Document) -> Option<(String, Vec<u8>)> {
    let rel = document
        .package
        .rels
        .get(&document.main)?
        .rels
        .iter()
        .find(|r| r.kind == COMMENTS_EXTENDED)?;
    let name = Package::resolve(&document.main, &rel.target);
    Some((name.clone(), document.package.part(&name)?.to_vec()))
}

/// Set a comment's entry in `commentsExtended`, making the part when there is none.
fn set_comment_extended(document: &mut Document, comment: i64, parent: Option<&str>, done: bool) {
    let para = document
        .comments
        .as_ref()
        .and_then(|p| p.notes.iter().find(|n| n.id() == Some(comment)))
        .and_then(last_para_id);
    let Some(para) = para else { return };
    let (name, mut root) = match comments_extended(document) {
        Some((name, bytes)) => {
            (name, crate::xml::parse_bytes(&bytes).unwrap_or_else(|_| El::new("w15:commentsEx")))
        }
        None => {
            let name = "word/commentsExtended.xml".to_owned();
            let main = document.main.clone();
            let target = Package::relative(&main, &name);
            let rels = document.package.rels_mut(&main);
            let id = format!("rIdU{}", rels.rels.len() + 100);
            rels.rels.push(crate::package::Rel {
                id,
                kind: COMMENTS_EXTENDED.into(),
                target,
                external: false,
            });
            rels.dirty = true;
            document.package.set_override(&name, "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml");
            (name, El::new("w15:commentsEx").with("mc:Ignorable", "w14 w15"))
        }
    };
    root.children
        .retain(|n| !matches!(n, Node::El(el) if el.attr("w15:paraId") == Some(para.as_str())));
    let mut entry = El::new("w15:commentEx").with("w15:paraId", para.clone());
    if let Some(parent) = parent {
        entry.set_attr("w15:paraIdParent", parent);
    }
    entry.set_attr("w15:done", if done { "1" } else { "0" });
    root.children.push(Node::El(entry));
    let _ = W15;
    let text = crate::write::whole_part(&root);
    document.package.set_part(&name, text.into_bytes());
}

const COMMENTS_IDS: &str = "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds";
const COMMENTS_EXTENSIBLE: &str =
    "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible";
const PEOPLE: &str = "http://schemas.microsoft.com/office/2011/relationships/people";

/// Change a part the main part points at with a relationship of type `kind`, when there is one, and
/// write it back. Answers whether there was such a part.
fn edit_related_part(document: &mut Document, kind: &str, edit: impl FnOnce(&mut El)) -> bool {
    let Some(rel) = document
        .package
        .rels
        .get(&document.main)
        .and_then(|rels| rels.rels.iter().find(|r| r.kind == kind).cloned())
    else {
        return false;
    };
    let name = Package::resolve(&document.main, &rel.target);
    let Some(mut root) = document.package.part(&name).and_then(|b| crate::xml::parse_bytes(b).ok())
    else {
        return false;
    };
    edit(&mut root);
    document.package.set_part(&name, crate::write::whole_part(&root).into_bytes());
    true
}

/// Record a new comment in the parts Word keeps beside the comments, those the file already has:
/// `commentsIds` (a durable id for the comment's paragraph), `commentsExtensible` (that id and the date)
/// and `people` (the author) (task-2359, D8).
fn add_comment_to_side_parts(document: &mut Document, para: &str, author: &str) {
    let durable = random_para_id(i64::from(para.len() as u32) + 7);
    edit_related_part(document, COMMENTS_IDS, |root| {
        root.children.push(Node::El(
            El::new("w16cid:commentId")
                .with("w16cid:paraId", para.to_owned())
                .with("w16cid:durableId", durable.clone()),
        ));
    });
    // Word lists every comment in commentsExtended, open (`w15:done="0"`) unless it is resolved; a reply
    // has its parent set there later by `set_comment_extended`, which replaces this entry.
    edit_related_part(document, COMMENTS_EXTENDED, |root| {
        root.children.push(Node::El(
            El::new("w15:commentEx").with("w15:paraId", para.to_owned()).with("w15:done", "0"),
        ));
    });
    let date = now_iso();
    edit_related_part(document, COMMENTS_EXTENSIBLE, |root| {
        root.children.push(Node::El(
            El::new("w16cex:commentExtensible")
                .with("w16cex:durableId", durable.clone())
                .with("w16cex:dateUtc", date.clone()),
        ));
    });
    edit_related_part(document, PEOPLE, |root| {
        let known = root.elements().any(|person| person.attr("w15:author") == Some(author));
        if !known {
            root.children.push(Node::El(
                El::new("w15:person").with("w15:author", author.to_owned()).with_child(
                    El::new("w15:presenceInfo")
                        .with("w15:providerId", "None")
                        .with("w15:userId", author.to_owned()),
                ),
            ));
        }
    });
}

/// Take a deleted comment out of `commentsIds`, `commentsExtensible` and `commentsExtended`.
fn remove_comment_from_side_parts(document: &mut Document, para: &str) {
    let mut durable: Option<String> = None;
    edit_related_part(document, COMMENTS_IDS, |root| {
        root.children.retain(|node| match node {
            Node::El(el) if el.attr("w16cid:paraId") == Some(para) => {
                durable = el.attr("w16cid:durableId").map(str::to_owned);
                false
            }
            _ => true,
        });
    });
    if let Some(durable) = durable {
        edit_related_part(document, COMMENTS_EXTENSIBLE, |root| {
            root.children.retain(|node| {
                !matches!(node, Node::El(el) if el.attr("w16cex:durableId") == Some(durable.as_str()))
            });
        });
    }
    edit_related_part(document, COMMENTS_EXTENDED, |root| {
        root.children
            .retain(|node| !matches!(node, Node::El(el) if el.attr("w15:paraId") == Some(para)));
    });
}

fn remove_comment_pieces(blocks: &mut [Block], id: &str) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                p.marks.retain(|m| {
                    !(matches!(m.el.name.as_str(), "w:commentRangeStart" | "w:commentRangeEnd")
                        && m.el.attr("w:id") == Some(id))
                });
                let before = p.runs.len();
                let mut removed = Vec::new();
                let mut at = 0;
                p.runs.retain(|run| {
                    let len = run.len();
                    let reference = matches!(&run.content, Content::Object(el) if el.name == "w:commentReference" && el.attr("w:id") == Some(id));
                    if reference {
                        removed.push((at, len));
                    }
                    at += len;
                    !reference
                });
                if p.runs.len() != before {
                    for (offset, len) in removed.into_iter().rev() {
                        for m in &mut p.marks {
                            if m.offset > offset {
                                m.offset -= len;
                            }
                        }
                    }
                    p.merge_runs();
                }
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        remove_comment_pieces(&mut cell.blocks, id);
                    }
                }
            }
            Block::Sdt(s) => remove_comment_pieces(&mut s.blocks, id),
            Block::Kept(_) => {}
        }
    }
}

fn next_mark_id(document: &Document, name: &str) -> i64 {
    position::paragraphs(&document.body)
        .iter()
        .flat_map(|(_, p)| p.marks.iter())
        .filter(|m| m.el.name == name)
        .filter_map(|m| m.el.attr("w:id")?.parse::<i64>().ok())
        .max()
        .unwrap_or(-1)
        + 1
}

fn recase(text: &str, how: &str, start_of_sentence: &mut bool) -> String {
    match how {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "toggle" => text
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<String>()
                } else {
                    c.to_uppercase().collect()
                }
            })
            .collect(),
        "title" => {
            let mut out = String::new();
            let mut start = true;
            for c in text.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    start = false;
                } else {
                    out.extend(c.to_lowercase());
                }
                if c.is_whitespace() {
                    start = true;
                }
            }
            out
        }
        _ => {
            let mut out = String::new();
            for c in text.chars() {
                if *start_of_sentence && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    *start_of_sentence = false;
                } else {
                    out.extend(c.to_lowercase());
                }
                if matches!(c, '.' | '!' | '?') {
                    *start_of_sentence = true;
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    fn session(text: &str) -> Session {
        let mut s = Session::new(crate::new::blank());
        s.type_text(text);
        s
    }

    fn reopened(s: &Session) -> Document {
        crate::read::open(&crate::write::save(&s.document).unwrap()).unwrap()
    }

    #[test]
    fn a_bullet_list_toggles_on_and_off_and_survives_a_save() {
        let mut s = session("one\ntwo\nthree");
        s.select_all();
        s.toggle_list(ListKind::Bullet);
        let again = reopened(&s);
        let resolver = crate::styles::Resolver::new(&again);
        let listed = position::paragraphs(&again.body)
            .iter()
            .filter(|(_, p)| resolver.numbering_of(&p.props).is_some())
            .count();
        assert_eq!(listed, 3);
        s.toggle_list(ListKind::Bullet);
        let resolver = crate::styles::Resolver::new(&s.document);
        assert!(position::paragraphs(&s.document.body)
            .iter()
            .all(|(_, p)| resolver.numbering_of(&p.props).is_none()));
    }

    #[test]
    fn tab_in_a_list_makes_the_item_deeper() {
        let mut s = session("item");
        s.toggle_list(ListKind::Number);
        assert!(s.change_list_level(true));
        let resolver = crate::styles::Resolver::new(&s.document);
        let (_, p) = &position::paragraphs(&s.document.body)[0];
        assert_eq!(resolver.numbering_of(&p.props).map(|(_, l)| l), Some(1));
    }

    #[test]
    fn a_table_is_inserted_and_rows_and_columns_are_added_and_merged() {
        let mut s = session("before");
        s.insert_table(2, 2);
        assert!(s.cell_at_caret().is_some());
        s.table_op("row-below").unwrap();
        s.table_op("column-right").unwrap();
        let Block::Table(t) = &s.document.body[1] else { panic!("a table") };
        assert_eq!((t.rows.len(), t.rows[0].cells.len(), t.columns().len()), (3, 3, 3));
        s.table_op("merge-right").unwrap();
        let Block::Table(t) = &s.document.body[1] else { panic!() };
        assert!(t.rows.iter().any(|r| r.cells.len() == 2));
        let again = reopened(&s);
        assert!(matches!(again.body[1], Block::Table(_)));
    }

    #[test]
    fn a_footnote_is_numbered_and_its_part_is_made() {
        let mut s = session("Claim");
        s.insert_note(false, "The source.");
        let again = reopened(&s);
        let notes = again.footnotes.as_ref().unwrap();
        assert_eq!(notes.notes.len(), 3, "two separators and the note");
        let pages = crate::layout::lay_out(
            &again,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        assert_eq!(pages.pages.len(), 1);
        let has_note_text =
            pages.pages[0].carets.iter().any(|l| matches!(l.story, StoryId::Footnote(_)));
        assert!(has_note_text, "the note is laid out at the foot of the page");
    }

    #[test]
    fn a_comment_is_added_replied_to_resolved_and_deleted() {
        let mut s = session("Review this sentence");
        s.select(Pos::new(vec![0], 7), Pos::new(vec![0], 11));
        let id = s.add_comment("Is this right?");
        let comments = s.comments();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0]["on"], "this");
        let reply = s.reply_to_comment(id, "Yes").unwrap();
        s.resolve_comment(id, true).unwrap();
        let again = Session::new(reopened(&s));
        let listed = again.comments();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed.iter().find(|c| c["id"] == id).unwrap()["resolved"], true);
        s.delete_comment(reply).unwrap();
        s.delete_comment(id).unwrap();
        assert!(s.comments().is_empty());
        assert_eq!(s.selected_text(), "");
    }

    #[test]
    fn a_hyperlink_wraps_the_selection_and_keeps_its_address() {
        let mut s = session("visit the site");
        s.select(Pos::new(vec![0], 10), Pos::new(vec![0], 14));
        s.insert_hyperlink("https://example.com", None);
        let again = reopened(&s);
        let (_, p) = &position::paragraphs(&again.body)[0];
        let link = p.runs.iter().find_map(|r| r.link()).expect("a link");
        let id = link.attr("r:id").unwrap();
        assert_eq!(
            again.package.rels[&again.main].by_id(id).unwrap().target,
            "https://example.com"
        );
    }

    #[test]
    fn page_settings_change_the_section_and_round_trip() {
        let mut s = session("text");
        s.set_page_size("a4").unwrap();
        s.set_orientation(true);
        s.set_margins([36.0, 36.0, 36.0, 36.0]);
        s.set_columns(2);
        let geometry = crate::layout::section::Geometry::of(&reopened(&s).section);
        assert!(geometry.landscape());
        assert!((geometry.width - 841.9).abs() < 0.1);
        assert_eq!(geometry.columns.len(), 2);
    }

    #[test]
    fn a_footer_with_page_numbers_is_made_and_numbers_each_page() {
        let mut s = session("first");
        s.set_header_text(true, "default", "Page {PAGE} of {NUMPAGES}");
        s.type_text("\u{c}second");
        let again = reopened(&s);
        let pages = crate::layout::lay_out(
            &again,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        assert_eq!(pages.pages.len(), 2);
        let footer_text = |page: &crate::layout::Page| -> String {
            page.items
                .iter()
                .filter_map(|i| match i {
                    crate::layout::Item::Text(t) if t.baseline > 700.0 => Some(t.text.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(footer_text(&pages.pages[1]).replace(' ', ""), "Page2of2");
    }

    #[test]
    fn a_section_break_starts_a_new_page() {
        let mut s = session("one two");
        s.select(Pos::new(vec![0], 3), Pos::new(vec![0], 3));
        s.insert_section_break("nextPage");
        let pages = crate::layout::lay_out(
            &s.document,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        assert_eq!(pages.pages.len(), 2);
    }

    #[test]
    fn a_table_of_contents_lists_the_headings_with_their_pages() {
        let mut s = session("Intro\nBody\nMore");
        s.select(Pos::new(vec![0], 0), Pos::new(vec![0], 0));
        s.apply_style("Heading1").unwrap();
        s.select(Pos::new(vec![2], 0), Pos::new(vec![2], 0));
        s.apply_style("Heading2").unwrap();
        s.select(Pos::new(vec![0], 0), Pos::new(vec![0], 0));
        let pages = crate::layout::lay_out(
            &s.document,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        s.insert_toc(3, &pages);
        let texts: Vec<String> =
            position::paragraphs(&s.document.body).iter().map(|(_, p)| p.visible_text()).collect();
        assert_eq!(texts[0], "Contents");
        assert_eq!(texts[1], "Intro\t1");
        assert_eq!(texts[2], "More\t1");
        let pages = crate::layout::lay_out(
            &s.document,
            &unluminous_core::FixedMetrics::default(),
            Default::default(),
        );
        assert_eq!(s.update_fields(&pages), 1);
        let count = position::paragraphs(&s.document.body)
            .iter()
            .filter(|(_, p)| p.visible_text() == "Contents")
            .count();
        assert_eq!(count, 1, "updating replaces the table rather than adding a second");
    }

    #[test]
    fn case_changes_follow_words_rules() {
        let mut flag = true;
        assert_eq!(recase("hello world. again", "sentence", &mut flag), "Hello world. Again");
        assert_eq!(recase("hello world", "title", &mut true), "Hello World");
    }
}
