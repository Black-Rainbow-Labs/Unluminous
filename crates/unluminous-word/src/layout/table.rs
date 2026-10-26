//! A table into rows, each of which can be cut between the lines of its cells.
//!
//! Column widths come from the table's grid, which is what Word laid the table out with when it saved it
//! (measured on the six width cases of `table-widths`: fixed, autofit to contents, autofit to window,
//! percentage, points and cells of different widths). A table with no usable grid, which other programs
//! write, gets its widths from its cells' `w:tcW`, then from its content (task-2359, C6). Each cell is laid
//! out as a small story at its width less its margins, a row is as tall as its tallest cell (or its
//! `w:trHeight`), and cells merged across columns (`w:gridSpan`) or down rows (`w:vMerge`) are drawn as
//! one. Borders are the cell's own, else the table's, else the table style's, with `insideH` and
//! `insideV` for the lines between cells.
//!
//! A row is kept as its cells' lines rather than as one picture, so the page flow can split it where it
//! runs off the page, as Word does unless the row says `w:cantSplit` (task-2359, C1).

use super::{lay_out_blocks, Chunk, Ctx, Item, Rect, RuleKind};
use crate::model::{Block, Row, Table};
use crate::numbering::Counters;
use crate::position::StoryId;
use crate::props::{self, number, twips, Border, Rgb};
use crate::xml::El;

/// One cell of a laid out row: where it is, its lines, and how it is drawn.
#[derive(Debug, Clone)]
pub struct CellPiece {
    /// The left edge and width of its column span, from the table's left.
    pub x: f32,
    pub w: f32,
    /// Its content, one piece a line (a nested table's rows are pieces too), top to bottom.
    pub lines: Vec<Chunk>,
    /// Space after the last paragraph, which the cell's height includes.
    pub after: f32,
    /// Left, right, top and bottom margins.
    pub margins: (f32, f32, f32, f32),
    pub shading: Option<Rgb>,
    /// Top, left, right and bottom borders.
    pub borders: [Option<Border>; 4],
    /// `w:vAlign`: top, center or bottom.
    pub valign: String,
    /// A cell merged into the one above: drawn by that one.
    pub continues: bool,
    /// How many rows a merge that starts here covers.
    pub spans: usize,
    /// Whether the cell below continues this one, so no bottom border is drawn between them.
    pub merged_below: bool,
    /// The height it is drawn at when that is not its row's: the rows a merge covers. 0 for the row's.
    pub drawn_height: f32,
}

impl CellPiece {
    fn content_height(&self) -> f32 {
        self.lines.iter().map(|l| l.height).sum::<f32>() + self.after
    }
}

/// A row laid out: its pieces, how tall it is, and the rules for placing it.
#[derive(Debug, Clone)]
pub struct LaidRow {
    /// The whole row drawn, for placing it in one piece.
    pub chunk: Chunk,
    pub header: bool,
    /// `w:cantSplit`: the row moves to the next page whole rather than being cut.
    pub cant_split: bool,
    pub cells: Vec<CellPiece>,
    /// `w:trHeight` and its rule (`atLeast`, `exact`), which a split row's parts keep.
    pub set_height: (f32, Option<String>),
    /// The table's left edge in the column.
    pub left: f32,
}

impl LaidRow {
    /// Cut the row so its first part is at most `available` tall: each cell keeps the lines that fit and
    /// the rest go to the second part, which is drawn with the cells' top borders again. `None` when not
    /// even one line of any cell fits, or when nothing would be left for the second part.
    pub fn split(&self, available: f32) -> Option<(Chunk, LaidRow)> {
        if self.set_height.1.as_deref() == Some("exact") {
            return None;
        }
        let mut first: Vec<CellPiece> = Vec::with_capacity(self.cells.len());
        let mut rest: Vec<CellPiece> = Vec::with_capacity(self.cells.len());
        let mut moved = false;
        let mut kept = false;
        let top_border =
            self.cells.iter().filter_map(|c| c.borders[0]).map(|b| b.width).fold(0.0, f32::max);
        for cell in &self.cells {
            let room = available - cell.margins.2 - cell.margins.3 - top_border;
            let mut used = 0.0;
            let mut take = 0;
            for line in &cell.lines {
                if used + line.height - line.slack > room + 0.01 {
                    break;
                }
                used += line.height;
                take += 1;
            }
            let take = keep_two_lines(&cell.lines, take);
            let (head, tail) = cell.lines.split_at(take);
            // A cell that would start on the next page takes the whole row there, as Word does.
            if head.is_empty() && !tail.is_empty() && !cell.continues {
                return None;
            }
            kept |= !head.is_empty();
            moved |= !tail.is_empty();
            first.push(CellPiece {
                lines: head.to_vec(),
                after: if tail.is_empty() { cell.after } else { 0.0 },
                merged_below: false,
                drawn_height: 0.0,
                ..cell.clone()
            });
            rest.push(CellPiece { lines: tail.to_vec(), drawn_height: 0.0, ..cell.clone() });
        }
        if !kept || !moved {
            return None;
        }
        let chunk = draw_row(&first, available);
        let rest_height = rest
            .iter()
            .filter(|c| !c.continues)
            .map(|c| c.content_height() + c.margins.2 + c.margins.3)
            .fold(0.0, f32::max)
            .max(1.0);
        let rest_chunk = draw_row(&rest, rest_height);
        Some((
            chunk,
            LaidRow {
                chunk: rest_chunk,
                header: false,
                cant_split: false,
                cells: rest,
                set_height: (0.0, None),
                left: self.left,
            },
        ))
    }
}

/// Where a cell's lines may be cut, given that the first `take` fit: a paragraph cut there keeps two of
/// its lines on each side, as widow control keeps them in the body. Measured on long-table.docx, Word
/// moves a row whole to the next page when only one line of its four line cell would fit.
fn keep_two_lines(lines: &[Chunk], take: usize) -> usize {
    let path = |line: &Chunk| line.carets.first().map(|c| c.path.clone());
    if take == 0 || take >= lines.len() || path(&lines[take - 1]) != path(&lines[take]) {
        return take;
    }
    let paragraph = path(&lines[take]);
    let start = lines[..take].iter().rposition(|l| path(l) != paragraph).map_or(0, |i| i + 1);
    let end =
        lines[take..].iter().position(|l| path(l) != paragraph).map_or(lines.len(), |i| i + take);
    let (before, after) = (take - start, end - take);
    if before >= 2 && after >= 2 {
        take
    } else if before >= 2 && before + after >= 4 {
        // Two lines go over with the last one.
        end - 2
    } else {
        // The paragraph goes over whole.
        start
    }
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
pub fn lay_out(
    ctx: &Ctx,
    counters: &mut Counters,
    table: &Table,
    story: &StoryId,
    path: &[u32],
    width: f32,
) -> Vec<LaidRow> {
    let style_id = ctx.resolver.table_style_id(&table.props);
    let style_el = style_id.and_then(|id| ctx.resolver.style(id));
    let look = TableLook { own: &table.props, style: style_el.and_then(|s| s.child("w:tblPr")) };
    let margins = (
        look.margin("w:left", 5.4).max(look.margin("w:start", 0.0)),
        look.margin("w:right", 5.4).max(look.margin("w:end", 0.0)),
        look.margin("w:top", 0.0),
        look.margin("w:bottom", 0.0),
    );
    let columns = column_widths(ctx, table, &look, width, margins);
    let total: f32 = columns.iter().sum();
    let indent =
        look.get("w:tblInd").and_then(|el| el.attr("w:w")).and_then(number).map_or(0.0, twips);
    let left = match look.get("w:jc").and_then(|el| el.attr("w:val")) {
        Some("center") => (width - total) / 2.0,
        Some("right" | "end") => width - total,
        _ => indent,
    };
    // Every cell laid out first, so a cell merged down several rows can make the last of them taller.
    let mut rows: Vec<Vec<CellPiece>> = Vec::with_capacity(table.rows.len());
    let mut heights: Vec<f32> = Vec::with_capacity(table.rows.len());
    // Each row's cell edges, for the pointer: the right edge of each cell and the grid column it ends at.
    let mut edges: Vec<Vec<(f32, usize)>> = Vec::with_capacity(table.rows.len());
    for (r, row) in table.rows.iter().enumerate() {
        let mut cells = Vec::with_capacity(row.cells.len());
        let mut column = grid_before(row);
        let mut tallest: f32 = 0.0;
        let mut row_edges: Vec<(f32, usize)> = Vec::new();
        for (c, cell) in row.cells.iter().enumerate() {
            let span = span_of(cell);
            let x: f32 = left + columns.iter().take(column).sum::<f32>();
            let w: f32 = columns.iter().skip(column).take(span).sum();
            let merge = cell.props.child("w:vMerge");
            let continues = merge.is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"));
            let restart = merge.and_then(|m| m.attr("w:val")) == Some("restart");
            let cell_margins = cell_margins(&cell.props, margins);
            let inner = (w - cell_margins.0 - cell_margins.1).max(6.0);
            let mut prefix = path.to_vec();
            prefix.extend([r as u32, c as u32]);
            let mut lines = Vec::new();
            let mut after = 0.0;
            let kinds = cell_kinds(table, style_el, r, c);
            let cell_style = style_id.map(|id| crate::styles::cell_style(id, &kinds));
            lay_out_blocks(
                ctx,
                counters,
                &cell.blocks,
                story,
                &mut prefix,
                inner,
                cell_style.as_deref(),
                &mut lines,
                &mut after,
            );
            let shading = cell
                .props
                .child("w:shd")
                .and_then(props::shading)
                .or_else(|| conditional_shading(style_el, &kinds));
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
            let spans = if restart { merged_rows(table, r, column) } else { 1 };
            let last_row = r + spans >= table.rows.len();
            let merged_below = table.rows.get(r + 1).is_some_and(|next| {
                cell_at_column(next, column).is_some_and(|below| {
                    below
                        .props
                        .child("w:vMerge")
                        .is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"))
                })
            });
            let piece = CellPiece {
                x,
                w,
                lines,
                after,
                margins: cell_margins,
                shading,
                borders: [
                    side("w:top", "w:top", r == 0, "w:insideH"),
                    side("w:left", "w:start", first_column, "w:insideV"),
                    side("w:right", "w:end", last_column, "w:insideV"),
                    side(
                        "w:bottom",
                        "w:bottom",
                        last_row || r + 1 == table.rows.len(),
                        "w:insideH",
                    ),
                ],
                valign: cell.props.val_of("w:vAlign").unwrap_or("top").to_owned(),
                continues,
                spans,
                merged_below,
                drawn_height: 0.0,
            };
            // A cell merged down several rows is as tall as all of them together, so it does not make its
            // first row taller; the loop after this gives the last row what the rows together lack. Word
            // draws tables.docx's "Merged down two rows" across its two rows, each the height of its own text.
            if !continues && spans <= 1 {
                tallest = tallest.max(piece.content_height() + cell_margins.2 + cell_margins.3);
            }
            row_edges.push((x + w, column + span));
            cells.push(piece);
            column += span;
        }
        edges.push(std::mem::take(&mut row_edges));
        let (set, rule) = set_height(row);
        let height = match rule.as_deref() {
            Some("exact") => set,
            _ => tallest.max(set),
        };
        heights.push(height.max(1.0));
        rows.push(cells);
    }
    // A row is as tall as its text and the width of the border along its top, and the last row the
    // border along its bottom too: measured on table-widths.docx, each one line row with 0.5 point borders
    // is 25.5 points in Word against 25.0 for its text and spacing.
    for (r, cells) in rows.iter().enumerate() {
        let top = cells.iter().filter_map(|c| c.borders[0]).map(|b| b.width).fold(0.0, f32::max);
        let bottom = if r + 1 == rows.len() {
            cells.iter().filter_map(|c| c.borders[3]).map(|b| b.width).fold(0.0, f32::max)
        } else {
            0.0
        };
        let (_, rule) = set_height(&table.rows[r]);
        if rule.as_deref() != Some("exact") {
            heights[r] += top + bottom;
        }
    }
    // A merged cell taller than the rows it spans makes the last one taller.
    for r in 0..rows.len() {
        for cell in &rows[r] {
            if cell.spans <= 1 {
                continue;
            }
            let need = cell.content_height() + cell.margins.2 + cell.margins.3;
            let end = (r + cell.spans).min(heights.len());
            let have: f32 = heights[r..end].iter().sum();
            if need > have {
                heights[end - 1] += need - have;
            }
        }
    }
    let mut out: Vec<LaidRow> = Vec::with_capacity(rows.len());
    for (r, cells) in rows.into_iter().enumerate() {
        let row = &table.rows[r];
        let height = heights[r];
        // A merge's first cell is drawn the height of every row it covers.
        let mut cells = cells;
        for cell in &mut cells {
            if cell.spans > 1 {
                let end = (r + cell.spans).min(heights.len());
                cell.drawn_height = heights[r..end].iter().sum();
            }
        }
        let mut chunk = draw_row(&cells, height);
        for (x, column) in &edges[r] {
            chunk.items.push(super::Item::TableEdge {
                x: *x,
                top: 0.0,
                bottom: height,
                story: story.clone(),
                table: path.to_vec(),
                column: *column,
            });
        }
        let header = crate::props::on_off(&row.props, "w:tblHeader") == Some(true);
        let cant_split = crate::props::on_off(&row.props, "w:cantSplit") == Some(true)
            || cells.iter().any(|c| c.spans > 1);
        out.push(LaidRow { chunk, header, cant_split, cells, set_height: set_height(row), left });
    }
    out
}

/// `w:trHeight`: the height in points and its rule.
fn set_height(row: &Row) -> (f32, Option<String>) {
    let rule = row.props.child("w:trHeight");
    let set = rule.and_then(|h| h.attr("w:val")).and_then(number).map_or(0.0, twips);
    (set, rule.and_then(|h| h.attr("w:hRule")).map(str::to_owned))
}

/// Draw the cells of a row, or a part of one, `height` tall. The top and bottom borders are drawn where
/// the part starts and ends, which for a split row is the cut, as Word draws it.
fn draw_row(cells: &[CellPiece], height: f32) -> Chunk {
    let mut chunk = Chunk { height, ..Chunk::default() };
    // The text starts under the border along the row's top, which the row's height includes.
    let top_border = cells.iter().filter_map(|c| c.borders[0]).map(|b| b.width).fold(0.0, f32::max);
    let mut fills = Vec::new();
    let mut rules = Vec::new();
    let line = |x0: f32, y0: f32, x1: f32, y1: f32, b: Border| Item::Rule {
        x0,
        y0,
        x1,
        y1,
        width: b.width,
        color: b.color.unwrap_or(Rgb::BLACK),
        kind: if b.double { RuleKind::Double } else { RuleKind::Solid },
    };
    for cell in cells {
        let cell_height = if cell.drawn_height > 0.0 { cell.drawn_height } else { height };
        if !cell.continues {
            if let Some(color) = cell.shading {
                fills.push(Item::Fill { rect: Rect::new(cell.x, 0.0, cell.w, cell_height), color });
            }
            let content: f32 = cell.lines.iter().map(|l| l.height).sum();
            let free = cell_height - content - cell.margins.2 - cell.margins.3;
            let shift = match cell.valign.as_str() {
                "center" => free.max(0.0) / 2.0,
                "bottom" => free.max(0.0),
                _ => 0.0,
            };
            let mut y = cell.margins.2 + shift + top_border;
            for piece in &cell.lines {
                let h = piece.height;
                chunk.absorb(piece.clone(), cell.x + cell.margins.0, y);
                y += h;
            }
            if let Some(b) = cell.borders[0] {
                rules.push(line(cell.x, 0.0, cell.x + cell.w, 0.0, b));
            }
        }
        if let Some(b) = cell.borders[1] {
            rules.push(line(cell.x, 0.0, cell.x, height, b));
        }
        if let Some(b) = cell.borders[2] {
            rules.push(line(cell.x + cell.w, 0.0, cell.x + cell.w, height, b));
        }
        if !cell.merged_below {
            if let Some(b) = cell.borders[3] {
                rules.push(line(cell.x, height, cell.x + cell.w, height, b));
            }
        }
    }
    let mut items = fills;
    items.append(&mut chunk.items);
    items.extend(rules);
    chunk.items = items;
    chunk
}

/// How many grid columns a cell spans.
fn span_of(cell: &crate::model::Cell) -> usize {
    cell.props.val_of("w:gridSpan").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).max(1)
}

/// A width element (`w:tblW`, `w:tcW`) in points for a container `of` points wide: `dxa` is twips, `pct`
/// is fiftieths of a percent (or a number with `%`), and `auto` or `nil` is no width.
fn preferred(el: Option<&El>, of: f32) -> Option<f32> {
    let el = el?;
    let value = el.attr("w:w")?;
    match el.attr("w:type").unwrap_or("dxa") {
        "pct" => {
            let fraction = match value.strip_suffix('%') {
                Some(percent) => percent.parse::<f32>().ok()? / 100.0,
                None => value.parse::<f32>().ok()? / 5000.0,
            };
            Some(of * fraction)
        }
        "dxa" => number(value).map(twips).filter(|w| *w > 0.0),
        _ => None,
    }
}

/// Grid columns in points. A grid Word wrote is the layout it drew; a table whose stated width is a
/// percentage is scaled to it. With no grid, the widths come from the cells (task-2359, C6).
fn column_widths(
    ctx: &Ctx,
    table: &Table,
    look: &TableLook,
    width: f32,
    margins: (f32, f32, f32, f32),
) -> Vec<f32> {
    let grid: Vec<f32> = table.columns().into_iter().map(|w| twips(w as f64)).collect();
    let stated = look.get("w:tblW");
    let stated_width = preferred(stated, width);
    let mut columns = if grid.is_empty() || grid.iter().all(|w| *w <= 0.0) {
        widths_from_cells(ctx, table, width, stated_width, margins)
    } else {
        grid
    };
    let pct = stated.and_then(|w| w.attr("w:type")) == Some("pct");
    if let (true, Some(want)) = (pct, stated_width) {
        let total: f32 = columns.iter().sum();
        if total > 0.0 {
            columns.iter_mut().for_each(|w| *w *= want / total);
        }
    }
    columns
}

/// Column widths for a table with no grid: each column's `w:tcW` where a cell gives one, the rest
/// shared so the table fills its stated width or, with none, fits its content within `width`.
fn widths_from_cells(
    ctx: &Ctx,
    table: &Table,
    width: f32,
    stated: Option<f32>,
    margins: (f32, f32, f32, f32),
) -> Vec<f32> {
    let count = table
        .rows
        .iter()
        .map(|row| grid_before(row) + row.cells.iter().map(span_of).sum::<usize>())
        .max()
        .unwrap_or(1)
        .max(1);
    let available = stated.unwrap_or(width);
    let mut fixed: Vec<Option<f32>> = vec![None; count];
    let mut content: Vec<f32> = vec![0.0; count];
    for row in &table.rows {
        let mut column = grid_before(row);
        for cell in &row.cells {
            let span = span_of(cell);
            if span == 1 && column < count {
                if let Some(w) = preferred(cell.props.child("w:tcW"), available) {
                    fixed[column] = Some(fixed[column].map_or(w, |f: f32| f.max(w)));
                }
                content[column] =
                    content[column].max(natural_width(ctx, cell) + margins.0 + margins.1);
            }
            column += span;
        }
    }
    let known: f32 = fixed.iter().flatten().sum();
    let open: Vec<usize> = (0..count).filter(|c| fixed[*c].is_none()).collect();
    let mut columns: Vec<f32> = fixed.iter().map(|f| f.unwrap_or(0.0)).collect();
    if open.is_empty() {
        return columns;
    }
    let left = (available - known).max(open.len() as f32 * 12.0);
    let wanted: f32 = open.iter().map(|c| content[*c].max(12.0)).sum();
    for c in &open {
        let want = content[*c].max(12.0);
        columns[*c] = match stated {
            // A table with a width of its own fills it, sharing in proportion to the content.
            Some(_) => left * want / wanted,
            // Autofit to contents: the content's width when it all fits, otherwise in proportion.
            None if wanted <= left => want,
            None => left * want / wanted,
        };
    }
    columns
}

/// The width a cell's content takes on one line: its widest paragraph laid out with no wrapping.
fn natural_width(ctx: &Ctx, cell: &crate::model::Cell) -> f32 {
    let mut widest: f32 = 0.0;
    for block in &cell.blocks {
        if let Block::Paragraph(paragraph) = block {
            let mut counters = Counters::default();
            let laid = super::paragraph::lay_out(
                ctx,
                &mut counters,
                paragraph,
                &StoryId::Body,
                &[],
                100_000.0,
                None,
            );
            for line in &laid.lines {
                for caret in &line.carets {
                    if let Some((_, x)) = caret.stops.last() {
                        widest = widest.max(*x);
                    }
                }
            }
        }
    }
    widest
}

fn grid_before(row: &Row) -> usize {
    row.props.val_of("w:gridBefore").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// The grid column a cell starts at.
fn column_of(row: &Row, cell: usize) -> usize {
    grid_before(row) + row.cells.iter().take(cell).map(span_of).sum::<usize>()
}

fn cell_at_column(row: &Row, column: usize) -> Option<&crate::model::Cell> {
    (0..row.cells.len()).find(|&c| column_of(row, c) == column).map(|c| &row.cells[c])
}

/// How many rows a merge that restarts at `(row, column)` covers.
fn merged_rows(table: &Table, row: usize, column: usize) -> usize {
    let mut count = 1;
    while let Some(next) = table.rows.get(row + count) {
        let continues = cell_at_column(next, column).is_some_and(|cell| {
            cell.props
                .child("w:vMerge")
                .is_some_and(|m| m.attr("w:val").is_none_or(|v| v == "continue"))
        });
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
        names
            .iter()
            .find_map(|n| mar.child(n))
            .and_then(|el| el.attr("w:w"))
            .and_then(number)
            .map_or(fallback, twips)
    };
    (
        get(&["w:left", "w:start"], table.0),
        get(&["w:right", "w:end"], table.1),
        get(&["w:top"], table.2),
        get(&["w:bottom"], table.3),
    )
}

/// The table style's conditional formatting that applies to a row: first row, last row, banded rows.
/// The conditional formats of a table style that cell `cell` of row `row` takes, as `w:tblStylePr`
/// types, from the table's `w:tblLook` (its flags, or the bits of its older `w:val`) and the style's
/// band sizes. The bands start after a header row and a first column the look turns on.
pub fn cell_kinds(table: &Table, style: Option<&El>, row: usize, cell: usize) -> Vec<&'static str> {
    let look = table.props.child("w:tblLook");
    let bits = look.and_then(|l| l.attr("w:val")).and_then(|v| u32::from_str_radix(v, 16).ok());
    let flag = |name: &str, bit: u32, default: bool| -> bool {
        match look.and_then(|l| l.attr(name)) {
            Some(v) => v == "1" || v == "true" || v == "on",
            None => bits.map_or(default, |b| b & bit != 0),
        }
    };
    let first_row = flag("w:firstRow", 0x20, true);
    let last_row = flag("w:lastRow", 0x40, false);
    let first_col = flag("w:firstColumn", 0x80, true);
    let last_col = flag("w:lastColumn", 0x100, false);
    let no_h_band = flag("w:noHBand", 0x200, false);
    let no_v_band = flag("w:noVBand", 0x400, true);
    let rows = table.rows.len();
    let cells = table.rows.get(row).map_or(0, |r| r.cells.len());
    let band = |name: &str| {
        style
            .and_then(|s| s.child("w:tblPr"))
            .and_then(|p| p.val_of(name))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1)
    };
    let mut kinds = vec!["wholeTable"];
    let is_first_row = first_row && row == 0;
    let is_last_row = last_row && row + 1 == rows && rows > 1;
    let is_first_col = first_col && cell == 0;
    let is_last_col = last_col && cell + 1 == cells && cells > 1;
    if !no_v_band && !is_first_col && !is_last_col {
        let data = if first_col { cell.saturating_sub(1) } else { cell };
        kinds.push(if (data / band("w:tblStyleColBandSize")) % 2 == 0 {
            "band1Vert"
        } else {
            "band2Vert"
        });
    }
    if !no_h_band && !is_first_row && !is_last_row {
        let data = if first_row { row.saturating_sub(1) } else { row };
        kinds.push(if (data / band("w:tblStyleRowBandSize")) % 2 == 0 {
            "band1Horz"
        } else {
            "band2Horz"
        });
    }
    if is_first_col {
        kinds.push("firstCol");
    }
    if is_last_col {
        kinds.push("lastCol");
    }
    if is_first_row {
        kinds.push("firstRow");
    }
    if is_last_row {
        kinds.push("lastRow");
    }
    match (is_first_row, is_last_row, is_first_col, is_last_col) {
        (true, _, true, _) => kinds.push("nwCell"),
        (true, _, _, true) => kinds.push("neCell"),
        (_, true, true, _) => kinds.push("swCell"),
        (_, true, _, true) => kinds.push("seCell"),
        _ => {}
    }
    kinds
}

/// The table style a paragraph in cell `cell` of row `row` is resolved against, with that cell's
/// conditional formats ([`crate::styles::cell_style`]).
pub fn cell_style_of(
    resolver: &crate::styles::Resolver,
    table: &Table,
    row: usize,
    cell: usize,
) -> Option<String> {
    let id = resolver.table_style_id(&table.props)?;
    let kinds = cell_kinds(table, resolver.style(id), row, cell);
    Some(crate::styles::cell_style(id, &kinds))
}

/// The cell shading a table style's conditional formats give, the last that names one winning.
fn conditional_shading(style: Option<&El>, kinds: &[&str]) -> Option<Rgb> {
    let style = style?;
    let mut found = style.child("w:tcPr").and_then(|tc| tc.child("w:shd")).and_then(props::shading);
    for kind in crate::styles::CONDITION_ORDER {
        if !kinds.contains(&kind) {
            continue;
        }
        for el in style.children_named("w:tblStylePr").filter(|el| el.attr("w:type") == Some(kind))
        {
            if let Some(shading) =
                el.child("w:tcPr").and_then(|tc| tc.child("w:shd")).and_then(props::shading)
            {
                found = Some(shading);
            }
        }
    }
    found
}
