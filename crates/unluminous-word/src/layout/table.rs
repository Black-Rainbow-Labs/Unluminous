//! A table into rows, each one piece that is placed whole.
//!
//! Column widths come from the table's grid. Each cell is laid out as a small story at its width less its
//! margins, a row is as tall as its tallest cell (or its `w:trHeight`), and cells merged across columns
//! (`w:gridSpan`) or down rows (`w:vMerge`) are drawn as one. Borders are the cell's own, else the
//! table's, else the table style's, with `insideH` and `insideV` for the lines between cells.

use super::{lay_out_blocks, Chunk, Ctx, Item, Rect, RuleKind};
use crate::model::{Row, Table};
use crate::numbering::Counters;
use crate::position::StoryId;
use crate::props::{self, number, twips, Border, Rgb};
use crate::xml::El;

/// A row laid out, and whether it repeats at the top of each page as a header.
#[derive(Debug, Clone)]
pub struct LaidRow {
    pub chunk: Chunk,
    pub header: bool,
}

/// The table's settings: its own `w:tblPr` over its style's.
struct TableLook<'a> {
    own: &'a El,
    style: Option<&'a El>,
}

impl<'a> TableLook<'a> {
    /// A child of `w:tblPr`, the table's own first.
    fn get(&self, name: &str) -> Option<&'a El> {
        self.own.child(name).or_else(|| self.style.and_then(|style| style.child(name)))
    }

    fn border(&self, side: &str) -> Option<Border> {
        let own = self.own.child("w:tblBorders").and_then(|b| b.child(side));
        let style = self.style.and_then(|s| s.child("w:tblBorders")).and_then(|b| b.child(side));
        match own {
            Some(el) => props::border(Some(el)),
            None => props::border(style),
        }
    }

    fn margin(&self, side: &str, fallback: f32) -> f32 {
        let own = self.own.child("w:tblCellMar").and_then(|m| m.child(side));
        let style = self.style.and_then(|s| s.child("w:tblCellMar")).and_then(|m| m.child(side));
        own.or(style).and_then(|el| el.attr("w:w")).and_then(number).map_or(fallback, twips)
    }
}

/// Lay a table out at `width`.
pub fn lay_out(ctx: &Ctx, counters: &mut Counters, table: &Table, story: &StoryId, path: &[u32], width: f32) -> Vec<LaidRow> {
    let style_id = ctx.resolver.table_style_id(&table.props);
    let style_el = style_id.and_then(|id| ctx.resolver.style(id));
    let look = TableLook { own: &table.props, style: style_el.and_then(|s| s.child("w:tblPr")) };
    let columns = column_widths(table, &look, width);
    let total: f32 = columns.iter().sum();
    let indent = look.get("w:tblInd").and_then(|el| el.attr("w:w")).and_then(number).map_or(0.0, twips);
    let left = match look.get("w:jc").and_then(|el| el.attr("w:val")) {
        Some("center") => (width - total) / 2.0,
        Some("right" | "end") => width - total,
        _ => indent,
    };
    let margins = (look.margin("w:left", 5.4).max(look.margin("w:start", 0.0)), look.margin("w:right", 5.4).max(look.margin("w:end", 0.0)), look.margin("w:top", 0.0), look.margin("w:bottom", 0.0));
    let tbl_look = table.props.child("w:tblLook");
    let first_row_style = tbl_look.is_none_or(|l| l.attr("w:firstRow") != Some("0") && l.attr("w:val").is_none_or(|v| u32::from_str_radix(v, 16).map_or(true, |bits| bits & 0x20 != 0)));
    let banded = tbl_look.is_none_or(|l| l.attr("w:noHBand") != Some("1"));
    let mut out: Vec<LaidRow> = Vec::with_capacity(table.rows.len());
    // Heights first, so a cell merged down several rows can make the last of them taller.
    let mut laid_cells: Vec<Vec<Option<(Chunk, f32, f32, usize)>>> = Vec::new();
    let mut heights: Vec<f32> = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::new();
        let mut column = grid_before(row);
        let mut tallest: f32 = 0.0;
        for (c, cell) in row.cells.iter().enumerate() {
            let span = cell.props.val_of("w:gridSpan").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).max(1);
            let x: f32 = columns.iter().take(column).sum();
            let w: f32 = columns.iter().skip(column).take(span).sum();
            let continues = cell.props.child("w:vMerge").is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"));
            let (left_margin, right_margin, top_margin, bottom_margin) = cell_margins(&cell.props, margins);
            let inner = (w - left_margin - right_margin).max(6.0);
            let mut prefix = path.to_vec();
            prefix.extend([r as u32, c as u32]);
            let mut chunks = Vec::new();
            let mut pending = 0.0;
            lay_out_blocks(ctx, counters, &cell.blocks, story, &mut prefix, inner, style_id, &mut chunks, &mut pending);
            let content = super::stack(chunks);
            let height = content.height + pending + top_margin + bottom_margin;
            if !continues {
                tallest = tallest.max(height);
            }
            cells.push(Some((content, x, w, span)));
            column += span;
        }
        let rule = row.props.child("w:trHeight");
        let set = rule.and_then(|h| h.attr("w:val")).and_then(number).map_or(0.0, twips);
        let height = match rule.and_then(|h| h.attr("w:hRule")) {
            Some("exact") => set,
            _ => tallest.max(set),
        };
        heights.push(height.max(1.0));
        laid_cells.push(cells);
    }
    // A merged cell taller than the rows it spans makes the last one taller.
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.props.child("w:vMerge").and_then(|m| m.attr("w:val")) != Some("restart") {
                continue;
            }
            let span = merged_rows(table, r, column_of(row, c));
            let Some(Some((content, ..))) = laid_cells[r].get(c) else { continue };
            let (_, _, top, bottom) = cell_margins(&cell.props, margins);
            let need = content.height + top + bottom;
            let have: f32 = heights[r..r + span].iter().sum();
            if need > have {
                heights[r + span - 1] += need - have;
            }
        }
    }
    for (r, row) in table.rows.iter().enumerate() {
        let height = heights[r];
        let mut chunk = Chunk { height, ..Chunk::default() };
        let mut fills = Vec::new();
        let mut rules = Vec::new();
        for (c, cell) in row.cells.iter().enumerate() {
            let Some(Some((content, x, w, _))) = laid_cells[r].get(c).cloned() else { continue };
            let continues = cell.props.child("w:vMerge").is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"));
            let restart = cell.props.child("w:vMerge").and_then(|m| m.attr("w:val")) == Some("restart");
            let rows_spanned = if restart { merged_rows(table, r, column_of(row, c)) } else { 1 };
            let cell_height: f32 = heights[r..r + rows_spanned].iter().sum();
            let x = left + x;
            let shading = cell
                .props
                .child("w:shd")
                .and_then(props::shading)
                .or_else(|| conditional_shading(style_el, r, table.rows.len(), first_row_style, banded));
            if let Some(color) = shading {
                if !continues {
                    fills.push(Item::Fill { rect: Rect::new(x, 0.0, w, cell_height), color });
                }
            }
            if !continues {
                let (left_margin, _, top_margin, bottom_margin) = cell_margins(&cell.props, margins);
                let free = cell_height - content.height - top_margin - bottom_margin;
                let shift = match cell.props.val_of("w:vAlign") {
                    Some("center") => free.max(0.0) / 2.0,
                    Some("bottom") => free.max(0.0),
                    _ => 0.0,
                };
                chunk.absorb(content, x + left_margin, top_margin + shift);
            }
            // Borders: the cell's own, else the table's outer or inside line.
            let last_row = r + rows_spanned >= table.rows.len();
            let first_column = c == 0;
            let last_column = c + 1 == row.cells.len();
            let tc = cell.props.child("w:tcBorders");
            let side = |name: &str, alt: &str, outer: bool, inside: &str| -> Option<Border> {
                match tc.and_then(|b| b.child(name).or_else(|| b.child(alt))) {
                    Some(el) => props::border(Some(el)),
                    None if outer => look.border(name).or_else(|| look.border(alt)),
                    None => look.border(inside),
                }
            };
            let line = |x0: f32, y0: f32, x1: f32, y1: f32, b: Border| Item::Rule {
                x0,
                y0,
                x1,
                y1,
                width: b.width,
                color: b.color.unwrap_or(Rgb::BLACK),
                kind: if b.double { RuleKind::Double } else { RuleKind::Solid },
            };
            if !continues {
                if let Some(b) = side("w:top", "w:top", r == 0, "w:insideH") {
                    rules.push(line(x, 0.0, x + w, 0.0, b));
                }
            }
            if let Some(b) = side("w:left", "w:start", first_column, "w:insideV") {
                rules.push(line(x, 0.0, x, height, b));
            }
            if let Some(b) = side("w:right", "w:end", last_column, "w:insideV") {
                rules.push(line(x + w, 0.0, x + w, height, b));
            }
            let merged_below = table.rows.get(r + 1).is_some_and(|next| {
                cell_at_column(next, column_of(row, c)).is_some_and(|below| {
                    below.props.child("w:vMerge").is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"))
                })
            });
            if !merged_below {
                if let Some(b) = side("w:bottom", "w:bottom", last_row || r + 1 == table.rows.len(), "w:insideH") {
                    rules.push(line(x, height, x + w, height, b));
                }
            }
        }
        let mut items = fills;
        items.append(&mut chunk.items);
        items.extend(rules);
        chunk.items = items;
        let header = crate::props::on_off(&row.props, "w:tblHeader") == Some(true);
        out.push(LaidRow { chunk, header });
    }
    out
}

/// Grid columns in points, scaled to the table's width when it states one as a percentage.
fn column_widths(table: &Table, look: &TableLook, width: f32) -> Vec<f32> {
    let mut columns: Vec<f32> = table.columns().into_iter().map(|w| twips(w as f64)).collect();
    if columns.is_empty() || columns.iter().all(|w| *w <= 0.0) {
        let count = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
        columns = vec![width / count as f32; count];
    }
    let stated = look.get("w:tblW");
    let total: f32 = columns.iter().sum();
    let want = match stated.and_then(|w| w.attr("w:type")) {
        Some("pct") => stated
            .and_then(|w| w.attr("w:w"))
            .map(|w| match w.strip_suffix('%') {
                Some(percent) => percent.parse::<f32>().unwrap_or(100.0) / 100.0,
                None => w.parse::<f32>().unwrap_or(5000.0) / 5000.0,
            })
            .map(|fraction| width * fraction),
        _ => None,
    };
    if let Some(want) = want {
        if total > 0.0 {
            columns.iter_mut().for_each(|w| *w *= want / total);
        }
    }
    columns
}

fn grid_before(row: &Row) -> usize {
    row.props.val_of("w:gridBefore").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// The grid column a cell starts at.
fn column_of(row: &Row, cell: usize) -> usize {
    grid_before(row)
        + row.cells.iter().take(cell).map(|c| c.props.val_of("w:gridSpan").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1)).sum::<usize>()
}

fn cell_at_column(row: &Row, column: usize) -> Option<&crate::model::Cell> {
    (0..row.cells.len()).find(|&c| column_of(row, c) == column).map(|c| &row.cells[c])
}

/// How many rows a merge that restarts at `(row, column)` covers.
fn merged_rows(table: &Table, row: usize, column: usize) -> usize {
    let mut count = 1;
    while let Some(next) = table.rows.get(row + count) {
        let continues = cell_at_column(next, column)
            .is_some_and(|cell| cell.props.child("w:vMerge").is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue")));
        if !continues {
            break;
        }
        count += 1;
    }
    count
}

/// A cell's margins: its own `w:tcMar`, else the table's.
fn cell_margins(tcpr: &El, table: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let Some(mar) = tcpr.child("w:tcMar") else { return table };
    let get = |names: &[&str], fallback: f32| {
        names.iter().find_map(|n| mar.child(n)).and_then(|el| el.attr("w:w")).and_then(number).map_or(fallback, twips)
    };
    (get(&["w:left", "w:start"], table.0), get(&["w:right", "w:end"], table.1), get(&["w:top"], table.2), get(&["w:bottom"], table.3))
}

/// The table style's conditional formatting that applies to a row: first row, last row, banded rows.
fn conditional_style<'a>(style: Option<&'a El>, row: usize, rows: usize, first_row: bool, banded: bool) -> Vec<&'a El> {
    let Some(style) = style else { return Vec::new() };
    let mut kinds = Vec::new();
    if first_row && row == 0 {
        kinds.push("firstRow");
    } else if row + 1 == rows && rows > 1 {
        kinds.push("lastRow");
    }
    if banded {
        let data_row = if first_row { row.saturating_sub(1) } else { row };
        kinds.push(if data_row % 2 == 0 { "band1Horz" } else { "band2Horz" });
    }
    style
        .children_named("w:tblStylePr")
        .filter(|el| el.attr("w:type").is_some_and(|t| kinds.contains(&t)))
        .collect()
}

fn conditional_shading(style: Option<&El>, row: usize, rows: usize, first_row: bool, banded: bool) -> Option<Rgb> {
    conditional_style(style, row, rows, first_row, banded)
        .iter()
        .find_map(|el| el.child("w:tcPr").and_then(|tc| tc.child("w:shd")).and_then(props::shading))
}
