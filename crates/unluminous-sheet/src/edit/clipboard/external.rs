//! Paste from other programs: tab separated text, and HTML tables.
//!
//! Text from outside is typed the way a person would type it, with one difference: a cell that begins with
//! `=` is text, not a formula, unless the person chose to paste formulas. That keeps a spreadsheet or a web
//! page from running a formula in a workbook that opened it.

use super::html::{self, HtmlCell, HtmlGrid};
use super::snapshot::intern_xf;
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::edit::Session;
use crate::formula::Formula;
use crate::model::{Cell, Hyperlink, LinkTarget};
use crate::styles::{AlignEdit, Color, FontEdit, StyleChange, Underline};
use crate::value::Value;

/// The choices of a paste from another program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExternalPaste {
    /// Cells that begin with `=` are formulas. Off unless the person asks.
    pub formulas: bool,
}

/// Tab separated text as rows of cells: a cell in quotes can hold tabs and line breaks, a doubled quote in
/// it is one quote, and the line break at the very end of the text ends the last row rather than starting
/// another.
pub fn parse_text_grid(text: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut i = 0;
    let mut pending = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' && cell.is_empty() && !pending_quote_text(&chars, i) {
            let (quoted, next) = read_quoted(&chars, i);
            cell = quoted;
            i = next;
            pending = true;
            continue;
        }
        match c {
            '\t' => {
                row.push(std::mem::take(&mut cell));
                pending = true;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.get(i + 1) == Some(&'\n') {
                    i += 1;
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                pending = false;
            }
            _ => {
                cell.push(c);
                pending = true;
            }
        }
        i += 1;
    }
    if pending || !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// Whether a quote starts text that is not quoted: one with no closing quote before the cell ends.
fn pending_quote_text(chars: &[char], at: usize) -> bool {
    let mut i = at + 1;
    while i < chars.len() {
        match chars[i] {
            '"' if chars.get(i + 1) == Some(&'"') => i += 2,
            '"' => return false,
            _ => i += 1,
        }
    }
    true
}

/// A cell in quotes: the text in it and where the text after it begins.
fn read_quoted(chars: &[char], at: usize) -> (String, usize) {
    let mut out = String::new();
    let mut i = at + 1;
    while i < chars.len() {
        match chars[i] {
            '"' if chars.get(i + 1) == Some(&'"') => {
                out.push('"');
                i += 2;
            }
            '"' => return (out, i + 1),
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i)
}

impl Session {
    /// Paste text from another program: tab separated cells, a row to a line, entered the way they would be
    /// typed. Returns the cells written.
    pub fn paste_text(
        &mut self,
        sheet: usize,
        at: CellRef,
        text: &str,
        options: ExternalPaste,
    ) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        let grid = parse_text_grid(text);
        let (rows, cols) = (grid.len() as u32, grid.iter().map(Vec::len).max().unwrap_or(0) as u32);
        if rows == 0 || cols == 0 {
            return Err("There is nothing to paste.".to_owned());
        }
        let target = clipped_target(at, rows, cols);
        self.refuse_array_member(sheet, target)?;
        self.refuse_locked(sheet, target)?;
        self.remember_cells("Paste", sheet, &[target], true);
        for (r, row) in grid.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                if let Some(dest) = at.offset(r as i64, c as i64) {
                    self.enter_text(sheet, dest, cell, options.formulas);
                }
            }
        }
        self.recalculate();
        Ok(target)
    }

    /// Put text into a cell as if it were typed. A text that would be a formula is text with the quote mark
    /// a typed apostrophe makes, unless `formulas` is set.
    pub(super) fn enter_text(&mut self, sheet: usize, at: CellRef, text: &str, formulas: bool) {
        let old = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let mut style = old.style;
        let mut formula = None;
        let mut quote = false;
        let value = if text.is_empty() {
            Value::Empty
        } else if text.starts_with('=') && text.len() > 1 && formulas {
            match Formula::parse(text) {
                Ok(f) => {
                    formula = Some(f);
                    Value::Empty
                }
                Err(_) => Value::text(text),
            }
        } else {
            let typed = crate::input::read(text, self.book.date1904, self.this_year);
            if let Some(code) = typed.format {
                if self.book.styles.format_code(style) == "General" {
                    style =
                        self.book.styles.derive(style, &StyleChange::NumberFormat(code.to_owned()));
                }
            }
            quote = matches!(typed.value, Value::Text(_)) && text.starts_with(['=', '@', '+', '-']);
            typed.value
        };
        if quote {
            style = self.quoted_style(style);
        }
        self.book.sheets[sheet]
            .set(at, Cell { value, formula, style, rich: None, extra: old.extra });
        self.engine.changed(&self.book, sheet, at);
    }

    /// Put text in a cell as text, whatever it looks like, keeping the cell's format.
    pub(super) fn enter_plain_text(&mut self, sheet: usize, at: CellRef, text: &str) {
        let old = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let value = if text.is_empty() { Value::Empty } else { Value::text(text) };
        self.book.sheets[sheet]
            .set(at, Cell { value, formula: None, style: old.style, rich: None, extra: old.extra });
        self.engine.changed(&self.book, sheet, at);
    }

    /// The cell format that is this one with the quote prefix, which keeps a text that looks like a formula
    /// from being read as one when it is edited.
    fn quoted_style(&mut self, style: u32) -> u32 {
        let mut xf = self.book.styles.xf(style);
        xf.quote_prefix = true;
        intern_xf(&mut self.book.styles, xf)
    }

    /// Paste HTML from another program: the tables in it become cells, with merged cells where a cell spans
    /// rows or columns, and bold, italic, colours, alignment and number formats where the page asks.
    pub fn paste_html(
        &mut self,
        sheet: usize,
        at: CellRef,
        page: &str,
        options: ExternalPaste,
    ) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        let grid = html::parse(page);
        let layout = layout(&grid);
        if layout.pieces.is_empty() {
            return Err("There is nothing to paste.".to_owned());
        }
        let target = clipped_target(at, layout.rows, layout.cols);
        self.refuse_array_member(sheet, target)?;
        self.refuse_locked(sheet, target)?;
        self.remember_whole("Paste");
        self.clear_for_html(sheet, target);
        for piece in &layout.pieces {
            self.place_piece(sheet, at, piece, options);
        }
        self.recalculate_all();
        Ok(target)
    }

    fn clear_for_html(&mut self, sheet: usize, target: Range) {
        let s = &mut self.book.sheets[sheet];
        s.cells.retain(|(r, c), _| !target.contains(CellRef::new(*r, *c)));
        s.merges.retain(|m| !m.intersects(&target));
    }

    /// Write one cell of the page, and merge it when it spans.
    fn place_piece(
        &mut self,
        sheet: usize,
        origin: CellRef,
        piece: &Piece,
        options: ExternalPaste,
    ) {
        let Some(first) = origin.offset(piece.row as i64, piece.col as i64) else { return };
        let span = Range::new(
            first,
            CellRef::new(
                (first.row + piece.rows - 1).min(MAX_ROWS - 1),
                (first.col + piece.cols - 1).min(MAX_COLS - 1),
            ),
        );
        let style = self.html_style(&piece.cell);
        for at in span.cells() {
            let mut cell = Cell { style, ..Default::default() };
            self.book.sheets[sheet].set(at, std::mem::take(&mut cell));
        }
        self.enter_html_text(sheet, first, piece, options);
        let s = &mut self.book.sheets[sheet];
        if span.len() > 1 {
            s.merges.push(span);
        }
        if let Some(href) = &piece.cell.link {
            s.links.push(Hyperlink {
                range: Range::cell(first),
                target: LinkTarget::External(href.clone()),
                display: None,
                tooltip: None,
            });
        }
        self.engine.changed(&self.book, sheet, first);
    }

    fn enter_html_text(
        &mut self,
        sheet: usize,
        at: CellRef,
        piece: &Piece,
        options: ExternalPaste,
    ) {
        let text = piece.text.as_str();
        match piece.cell.number {
            Some(n) if !piece.cell.as_text => {
                let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
                self.book.sheets[sheet]
                    .set(at, Cell { value: Value::Number(n), style, ..Default::default() });
            }
            _ if piece.cell.as_text => {
                let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
                let style =
                    self.book.styles.derive(style, &StyleChange::NumberFormat("@".to_owned()));
                let value = if text.is_empty() { Value::Empty } else { Value::text(text) };
                self.book.sheets[sheet].set(at, Cell { value, style, ..Default::default() });
            }
            _ => self.enter_text(sheet, at, text, options.formulas),
        }
    }

    /// The cell format a page's cell asks for.
    fn html_style(&mut self, cell: &HtmlCell) -> u32 {
        let mut style = 0;
        let mut apply = |this: &mut Session, change: StyleChange| {
            style = this.book.styles.derive(style, &change);
        };
        let link_colour = Color::Theme(10, 0.0);
        let colour = cell.color.or(cell.link.is_some().then_some(link_colour));
        if cell.bold
            || cell.italic
            || cell.underline
            || cell.strike
            || colour.is_some()
            || cell.link.is_some()
        {
            let edit = FontEdit {
                bold: cell.bold.then_some(true),
                italic: cell.italic.then_some(true),
                underline: (cell.underline || cell.link.is_some()).then_some(Underline::Single),
                strike: cell.strike.then_some(true),
                color: colour,
                ..Default::default()
            };
            apply(self, StyleChange::Font(edit));
        }
        if let Some(fill) = cell.fill {
            apply(self, StyleChange::Fill(Some(fill)));
        }
        if let Some(a) = cell.align {
            let edit = AlignEdit { horizontal: Some(Some(a.to_owned())), ..Default::default() };
            apply(self, StyleChange::Align(edit));
        }
        if let Some(code) = &cell.format {
            apply(self, StyleChange::NumberFormat(code.clone()));
        }
        style
    }
}

/// The cells of a range of this size from a place, cut off at the edge of the sheet.
fn clipped_target(at: CellRef, rows: u32, cols: u32) -> Range {
    Range::new(
        at,
        CellRef::new((at.row + rows - 1).min(MAX_ROWS - 1), (at.col + cols - 1).min(MAX_COLS - 1)),
    )
}

/// One cell of the page as it lands on the sheet: where, how many cells it covers, and what it says.
struct Piece {
    row: u32,
    col: u32,
    rows: u32,
    cols: u32,
    text: String,
    cell: HtmlCell,
}

struct Layout {
    pieces: Vec<Piece>,
    rows: u32,
    cols: u32,
}

/// Lay the page's cells out on the sheet. A cell with line breaks in it takes a row for each line and the
/// cells beside it are merged down to cover them, which is how Excel pastes a cell of several lines.
fn layout(grid: &HtmlGrid) -> Layout {
    let heights: Vec<u32> = grid
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .filter(|c| !c.same_cell)
                .map(|c| c.lines.len() as u32)
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();
    let tops: Vec<u32> = heights
        .iter()
        .scan(0, |at, h| {
            let top = *at;
            *at += h;
            Some(top)
        })
        .collect();
    let mut taken: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut pieces = Vec::new();
    let (mut rows_used, mut cols_used) = (0, 0);
    for (r, row) in grid.rows.iter().enumerate() {
        let mut x = 0;
        for cell in row {
            while taken.contains(&(tops[r], x)) {
                x += 1;
            }
            let covered = (r..(r + cell.row_span as usize).min(grid.rows.len()))
                .map(|i| heights[i])
                .sum::<u32>();
            for piece in pieces_of(cell, tops[r], x, covered) {
                for dy in 0..piece.rows {
                    for dx in 0..piece.cols {
                        taken.insert((piece.row + dy, piece.col + dx));
                    }
                }
                rows_used = rows_used.max(piece.row + piece.rows);
                cols_used = cols_used.max(piece.col + piece.cols);
                pieces.push(piece);
            }
            x += cell.col_span;
        }
    }
    Layout { pieces, rows: rows_used, cols: cols_used }
}

/// The cells one cell of the page makes: one, or one for each line.
fn pieces_of(cell: &HtmlCell, top: u32, col: u32, covered: u32) -> Vec<Piece> {
    let lines: Vec<String> =
        if cell.same_cell { vec![cell.lines.join("\n")] } else { cell.lines.clone() };
    let last = lines.len() as u32 - 1;
    lines
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let rows = if i as u32 == last { covered.saturating_sub(last).max(1) } else { 1 };
            Piece { row: top + i as u32, col, rows, cols: cell.col_span, text, cell: cell.clone() }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_split_at_tabs_and_line_breaks() {
        assert_eq!(parse_text_grid("a\tb\r\nc\td\r\n"), [["a", "b"], ["c", "d"]]);
        assert_eq!(parse_text_grid("x\ty"), [["x", "y"]]);
        assert_eq!(
            parse_text_grid("a\t\tc\r\n\r\nx\ty\r\n"),
            [vec!["a", "", "c"], vec![""], vec!["x", "y"]]
        );
    }

    #[test]
    fn a_cell_in_quotes_holds_tabs_line_breaks_and_quotes() {
        let grid = parse_text_grid("\"a\tb\"\t\"line1\nline2\"\t\"say \"\"hi\"\"\"\t\"12\"\r\n");
        assert_eq!(grid, [vec!["a\tb", "line1\nline2", "say \"hi\"", "12"]]);
        assert_eq!(parse_text_grid("say \"hi\"\tb"), [vec!["say \"hi\"", "b"]]);
    }
}
