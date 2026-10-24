//! Making a clip: the cells copied and everything that travels with them, and the cell formats they use.

use std::collections::BTreeMap;

use super::{ClipExtras, ClipRules, ClipStyle};
use crate::coord::{CellRef, Range};
use crate::edit::{Clip, Session};
use crate::model::{Cell, Sheet, Validation};
use crate::styles::{Color, Styles};

impl Session {
    /// Copy a range: its cells, notes, data validation, conditional formats, merged cells, links, column
    /// widths and the cell formats they use. Rows a filter hides are left out, as Excel leaves them out.
    pub(in crate::edit) fn build_clip(&self, sheet: usize, range: Range, cut: bool) -> Clip {
        let s = &self.book.sheets[sheet];
        let rows = if cut { Vec::new() } else { filtered_rows(s, range) };
        let last_row = range.last.row.min(last_used_row(s).max(range.first.row));
        let last_col = range.last.col.min(last_used_col(s).max(range.first.col));
        let mut cells = Vec::new();
        for r in range.first.row..=last_row {
            let Some(shown) = shown_row(&rows, range.first.row, r) else { continue };
            for c in range.first.col..=last_col {
                if let Some(cell) = cell_or_spill(s, r, c) {
                    cells.push((shown, c - range.first.col, cell));
                }
            }
        }
        let mut extras = block_extras(s, range);
        extras.rows = rows;
        extras.date1904 = self.book.date1904;
        extras.theme = self.book.styles.theme.clone();
        remember_formats(&self.book.styles, &cells, &mut extras);
        Clip { sheet, range, cells, cut, extras }
    }
}

/// The offsets from the first row of the rows a copy keeps, or nothing when it keeps every row.
fn filtered_rows(s: &Sheet, range: Range) -> Vec<u32> {
    let Some(filter) = &s.filter else { return Vec::new() };
    if filter.columns.is_empty() {
        return Vec::new();
    }
    let hidden_by_filter = |row: u32| {
        row > filter.range.first.row && row <= filter.range.last.row && s.is_row_hidden(row)
    };
    // Only rows that hold something matter, so a copy of whole columns is cut to the rows in use.
    let limit = range.last.row.min(last_used_row(s).max(filter.range.last.row)).max(range.first.row);
    let kept: Vec<u32> = (range.first.row..=limit)
        .filter(|r| !hidden_by_filter(*r))
        .map(|r| r - range.first.row)
        .collect();
    if kept.len() as u32 == limit - range.first.row + 1 {
        return Vec::new();
    }
    kept
}

/// The row a source row is counted as in the block, or `None` for a row left out.
fn shown_row(kept: &[u32], first: u32, row: u32) -> Option<u32> {
    if kept.is_empty() {
        return Some(row - first);
    }
    kept.iter().position(|offset| *offset == row - first).map(|i| i as u32)
}

fn last_used_row(s: &Sheet) -> u32 {
    s.used_range().map_or(0, |r| r.last.row)
}

fn last_used_col(s: &Sheet) -> u32 {
    s.used_range().map_or(0, |r| r.last.col)
}

/// The cell at a place, or a cell holding the value a spill put there.
fn cell_or_spill(s: &Sheet, row: u32, col: u32) -> Option<Cell> {
    match s.cells.get(&(row, col)) {
        Some(cell) => Some(cell.clone()),
        None => s.spills.values.get(&(row, col)).map(|v| Cell::value(v.clone())),
    }
}

/// The notes, validation, conditional formats, merges, links, widths and heights of a block.
fn block_extras(s: &Sheet, range: Range) -> ClipExtras {
    let at = |r: Range| {
        Range::new(
            CellRef::new(r.first.row - range.first.row, r.first.col - range.first.col),
            CellRef::new(r.last.row - range.first.row, r.last.col - range.first.col),
        )
    };
    let mut extras = ClipExtras {
        whole_columns: range.is_whole_columns(),
        whole_rows: range.is_whole_rows(),
        ..Default::default()
    };
    for ((r, c), note) in &s.notes {
        if range.contains(CellRef::new(*r, *c)) {
            extras.notes.push((r - range.first.row, c - range.first.col, note.clone()));
        }
    }
    for v in &s.validations {
        for part in v.ranges.iter().filter_map(|r| r.intersection(&range)) {
            let single = Validation { ranges: Vec::new(), ..v.clone() };
            extras.validations.push((at(part), single));
        }
    }
    for cf in &s.conditional {
        let ranges: Vec<Range> =
            cf.ranges.iter().filter_map(|r| r.intersection(&range)).map(at).collect();
        if !ranges.is_empty() {
            extras.conditional.push(ClipRules { ranges, rules: cf.rules.clone() });
        }
    }
    extras.merges = s
        .merges
        .iter()
        .filter(|m| range.contains(m.first) && range.contains(m.last))
        .map(|m| at(*m))
        .collect();
    for link in s.links.iter().filter(|l| range.contains(l.range.first) && range.contains(l.range.last)) {
        extras.links.push(crate::model::Hyperlink { range: at(link.range), ..link.clone() });
    }
    widths_and_heights(s, range, &mut extras);
    hidden_lines(s, range, &mut extras);
    extras
}

/// The rows and columns of the block that are hidden.
fn hidden_lines(s: &Sheet, range: Range, extras: &mut ClipExtras) {
    let last_row = range.last.row.min(last_used_row(s).max(range.first.row));
    extras.hidden_rows = (range.first.row..=last_row)
        .filter(|r| s.is_row_hidden(*r))
        .map(|r| r - range.first.row)
        .collect();
    extras.hidden_columns = (range.first.col..=range.last.col.min(range.first.col + 16_383))
        .filter(|c| s.is_column_hidden(*c))
        .map(|c| c - range.first.col)
        .collect();
}

/// The width of every copied column and the height of every copied row that has one.
fn widths_and_heights(s: &Sheet, range: Range, extras: &mut ClipExtras) {
    for c in range.first.col..=range.last.col.min(range.first.col + 16_383) {
        if let Some(info) = s.column(c) {
            extras.widths.push((c - range.first.col, info.clone()));
        }
    }
    if range.is_whole_rows() || range.rows() < 10_000 {
        for r in range.first.row..=range.last.row {
            if let Some(height) = s.rows.get(&r).and_then(|i| i.height) {
                extras.heights.push((r - range.first.row, height));
            }
        }
    }
}

/// Keep the cell formats the copied cells use and the differential formats the conditional formats name.
fn remember_formats(styles: &Styles, cells: &[(u32, u32, Cell)], extras: &mut ClipExtras) {
    for (_, _, cell) in cells {
        extras.styles.entry(cell.style).or_insert_with(|| snapshot_style(styles, cell.style));
    }
    for rules in &extras.conditional {
        for dxf in rules.rules.iter().filter_map(|r| r.dxf) {
            if let Some(d) = styles.dxfs.get(dxf as usize) {
                extras.dxfs.insert(dxf, d.clone());
            }
        }
    }
}

/// The cell format at an index with the font, fill, border and number format it names written out.
pub(super) fn snapshot_style(styles: &Styles, index: u32) -> ClipStyle {
    let xf = styles.xf(index);
    ClipStyle {
        font: styles.font_of(index),
        fill: styles.fill_of(index),
        border: styles.border_of(index),
        format_code: styles.format_code(index),
        alignment: xf.align.clone(),
        xf,
    }
}

/// A theme colour written as the colour it looks like, which is what pasting with the source theme keeps.
pub(super) fn through_theme(color: Color, theme: &[u32]) -> Color {
    match color {
        Color::Theme(..) => match color.resolve(theme) {
            Some((r, g, b)) => Color::rgb(r, g, b),
            None => color,
        },
        other => other,
    }
}

/// Make the format a clip held, in this workbook, finding an equal one when there is one.
pub(super) fn import_style(
    styles: &mut Styles,
    clip: &ClipExtras,
    index: u32,
    source_theme: bool,
) -> u32 {
    let Some(cs) = clip.styles.get(&index) else { return 0 };
    let mut font = cs.font.clone();
    let mut fill = cs.fill.clone();
    let mut border = cs.border.clone();
    if source_theme {
        font.color = through_theme(font.color, &clip.theme);
        fill.fg = through_theme(fill.fg, &clip.theme);
        fill.bg = through_theme(fill.bg, &clip.theme);
        for edge in [
            &mut border.left,
            &mut border.right,
            &mut border.top,
            &mut border.bottom,
            &mut border.diagonal,
        ] {
            edge.color = through_theme(edge.color, &clip.theme);
        }
    }
    let mut xf = cs.xf.clone();
    xf.font = intern(&mut styles.fonts, font);
    xf.fill = intern(&mut styles.fills, fill);
    xf.border = intern(&mut styles.borders, border);
    xf.num_fmt = styles.format_id(&cs.format_code);
    xf.applies.clear();
    intern_xf(styles, xf)
}

fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    if let Some(i) = list.iter().position(|existing| *existing == item) {
        return i as u32;
    }
    list.push(item);
    (list.len() - 1) as u32
}

/// The index of a cell format equal to this one, adding it when there is none.
pub(super) fn intern_xf(styles: &mut Styles, mut xf: crate::styles::Xf) -> u32 {
    xf.applies.clear();
    if let Some(i) = styles.xfs.iter().position(|existing| {
        let mut e = existing.clone();
        e.applies.clear();
        e == xf
    }) {
        return i as u32;
    }
    styles.xfs.push(xf);
    (styles.xfs.len() - 1) as u32
}

/// The destination index of every differential format a clip names, adding the ones this workbook lacks.
pub(super) fn import_dxfs(styles: &mut Styles, clip: &ClipExtras) -> BTreeMap<u32, u32> {
    let mut map = BTreeMap::new();
    for (index, dxf) in &clip.dxfs {
        map.insert(*index, intern(&mut styles.dxfs, dxf.clone()));
    }
    map
}
