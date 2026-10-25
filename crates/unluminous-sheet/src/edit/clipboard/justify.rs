//! Justify: pour the text of a column of cells into lines that fit the width of the selected columns.

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::Cell;
use crate::value::Value;

/// Excel's refusal when a cell in the range is not text.
const NOT_TEXT: &str = "Cannot justify cells containing numbers or formulas.";

/// How wide each printable ASCII character is in the default font (Aptos Narrow 11), in pixels, from the
/// space to the tilde. Measured by fitting a column to a run of each character in Excel.
const PIXELS: [u32; 95] = [
    4, 5, 5, 8, 8, 13, 10, 3, 5, 5, 6, 9, 4, 5, 4, 4, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 4, 4, 9, 9, 9,
    8, 15, 9, 10, 11, 11, 10, 9, 11, 10, 3, 7, 10, 8, 11, 10, 12, 10, 12, 11, 10, 9, 10, 9, 15, 9,
    9, 8, 4, 4, 4, 5, 8, 5, 8, 8, 8, 8, 8, 4, 8, 8, 3, 3, 7, 3, 13, 8, 8, 8, 8, 5, 8, 4, 8, 7, 11,
    7, 7, 8, 5, 3, 5, 9,
];

/// The pixels in one unit of a column width as a file stores it: the width of a digit of the default font.
const PIXELS_PER_UNIT: f64 = 8.0;

/// What the margin of a cell takes from the room for text, in pixels. Measured with columns of 7 to 64 units.
const MARGIN: f64 = 3.0;

/// The width of a text in pixels, for a font of this size (the default is 11 points).
fn text_pixels(text: &str, size: f64) -> f64 {
    let wide: u32 = text
        .chars()
        .map(|c| match c as usize {
            32..=126 => PIXELS[c as usize - 32],
            _ => 8,
        })
        .sum();
    f64::from(wide) * size / 11.0
}

/// Pour words into lines of at most `room` pixels. A word wider than a line gets a line of its own.
fn flow(text: &str, room: f64, size: f64) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_pixels(&format!("{line} {word}"), size) <= room => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

impl Session {
    /// Justify the text in the first column of a range: the text of cells next to each other is poured
    /// into lines as wide as the columns of the range, a blank cell ends a paragraph, and the lines go
    /// into the cells down from the top of the range, as far down as they need, one blank cell between
    /// paragraphs. Returns the cells written.
    pub fn justify(&mut self, sheet: usize, range: Range) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        self.refuse_array_member(sheet, range)?;
        self.refuse_locked(sheet, range)?;
        let (not_text, size, style) = {
            let s = &self.book.sheets[sheet];
            let bad = range.cells().any(|at| {
                s.cell(at).is_some_and(|c| {
                    c.formula.is_some()
                        || matches!(c.value, Value::Number(_) | Value::Bool(_) | Value::Error(_))
                })
            });
            let first = s.cell(range.first);
            let size = first.map_or(11.0, |c| self.book.styles.font_of(c.style).size);
            (bad, size, first.map_or(0, |c| c.style))
        };
        if not_text {
            return Err(NOT_TEXT.to_owned());
        }
        let paragraphs = self.paragraphs_of(sheet, range);
        if paragraphs.is_empty() {
            return Ok(Range::cell(range.first));
        }
        let lines = lay_out(&paragraphs, self.room_in_pixels(sheet, range), size);
        let col = range.first.col;
        let last = CellRef::new(range.last.row.max(range.first.row + lines.len() as u32 - 1), col);
        self.remember_cells("Justify", sheet, &[Range::new(range.first, last)], true);
        self.write_lines(sheet, range, &lines, style);
        self.recalculate();
        Ok(Range::new(range.first, CellRef::new(range.first.row + lines.len() as u32 - 1, col)))
    }

    /// The paragraphs in the first column of the range: the text of each run of cells that hold text.
    fn paragraphs_of(&self, sheet: usize, range: Range) -> Vec<String> {
        let s = &self.book.sheets[sheet];
        let mut out: Vec<String> = Vec::new();
        let mut open = false;
        for r in range.first.row..=range.last.row {
            let text = s
                .cell(CellRef::new(r, range.first.col))
                .map(|c| c.value.to_text())
                .unwrap_or_default();
            if text.trim().is_empty() {
                open = false;
            } else if open {
                let last = out.last_mut().unwrap();
                last.push(' ');
                last.push_str(text.trim());
            } else {
                out.push(text.trim().to_owned());
                open = true;
            }
        }
        out
    }

    /// How wide the lines may be: the width of the columns of the range.
    fn room_in_pixels(&self, sheet: usize, range: Range) -> f64 {
        let s = &self.book.sheets[sheet];
        let columns: f64 = (range.first.col..=range.last.col)
            .map(|c| s.column(c).and_then(|i| i.width).or(s.default_col_width).unwrap_or(9.0))
            .sum();
        columns * PIXELS_PER_UNIT - MARGIN
    }

    /// Put the lines in the first column from the top of the range, and empty the cells of the range the
    /// lines do not reach.
    fn write_lines(&mut self, sheet: usize, range: Range, lines: &[Option<String>], style: u32) {
        let col = range.first.col;
        let last = range.last.row.max(range.first.row + lines.len() as u32 - 1);
        for (i, r) in (range.first.row..=last).enumerate() {
            let at = CellRef::new(r, col);
            let old = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
            let text = lines.get(i).cloned().flatten();
            let value = text.map_or(Value::Empty, Value::text);
            let cell = if i < lines.len() {
                Cell { value, formula: None, style, rich: None, extra: old.extra }
            } else if r <= range.last.row {
                Cell { style: old.style, ..Default::default() }
            } else {
                old
            };
            self.book.sheets[sheet].set(at, cell);
            self.engine.changed(&self.book, sheet, at);
        }
    }
}

/// The lines of every paragraph, with a blank line between paragraphs.
fn lay_out(paragraphs: &[String], room: f64, size: f64) -> Vec<Option<String>> {
    let mut out = Vec::new();
    for (i, p) in paragraphs.iter().enumerate() {
        if i > 0 {
            out.push(None);
        }
        out.extend(flow(p, room, size).into_iter().map(Some));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_fill_a_line_and_a_long_word_has_one_to_itself() {
        let lines = flow("one two three four five six seven eight", 93.0, 11.0);
        assert_eq!(lines, ["one two three", "four five six", "seven eight"]);
        let lines = flow("abcdefghijklmnopqrstuvwxyz short", 77.0, 11.0);
        assert_eq!(lines, ["abcdefghijklmnopqrstuvwxyz", "short"]);
    }
}
