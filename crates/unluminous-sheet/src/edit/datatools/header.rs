//! Whether Excel takes the first row of a range as a header when it is not told.
//!
//! The rule was measured against Excel 16 on 330 small tables (`tests/fixtures/datatools/header-*.json`)
//! and it holds for all of them. The first row is a header when it differs from the second row in some
//! column, and the differences that count are a different kind of value (text, number, logical or error)
//! and a different format (font, fill, border, alignment or number format, which includes a date format on
//! one and not the other). A column where either cell is empty says nothing. A first row with an empty cell
//! after its first filled one is never a header, and neither is one that starts with more empty cells than
//! it has filled ones.

use crate::coord::{CellRef, Range};
use crate::model::{Cell, Workbook};
use crate::value::Value;

/// What kind of value a cell holds, for the comparison of two rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Blank,
    Number,
    Text,
    Logical,
    Error,
}

fn kind_of(cell: &Cell) -> Kind {
    match cell.value {
        Value::Empty => Kind::Blank,
        Value::Number(_) => Kind::Number,
        Value::Text(_) => Kind::Text,
        Value::Bool(_) => Kind::Logical,
        Value::Error(_) => Kind::Error,
    }
}

/// Whether two cells are formatted alike.
fn same_format(book: &Workbook, a: &Cell, b: &Cell) -> bool {
    let (xa, xb) = (book.styles.xf(a.style), book.styles.xf(b.style));
    book.styles.font_of(a.style) == book.styles.font_of(b.style)
        && book.styles.fill_of(a.style) == book.styles.fill_of(b.style)
        && book.styles.border_of(a.style) == book.styles.border_of(b.style)
        && xa.align == xb.align
        && book.styles.format_code(a.style) == book.styles.format_code(b.style)
}

/// Excel's guess whether the first row (the first column when sorting left to right) is a header.
pub(crate) fn guess(book: &Workbook, sheet: usize, range: Range, left_to_right: bool) -> bool {
    let s = &book.sheets[sheet];
    let lines = if left_to_right { range.cols() } else { range.rows() };
    let across = if left_to_right { range.rows() } else { range.cols() };
    if lines < 2 {
        return false;
    }
    let at = |line: u32, i: u32| {
        let (r, c) = if left_to_right {
            (range.first.row + i, range.first.col + line)
        } else {
            (range.first.row + line, range.first.col + i)
        };
        s.cell(CellRef::new(r, c)).cloned().unwrap_or_default()
    };
    let first: Vec<Cell> = (0..across).map(|i| at(0, i)).collect();
    let second: Vec<Cell> = (0..across).map(|i| at(1, i)).collect();
    let blank: Vec<bool> = first.iter().map(|c| kind_of(c) == Kind::Blank).collect();
    let Some(leading) = blank.iter().position(|b| !b) else { return false };
    let filled = across as usize - leading;
    if blank[leading..].iter().any(|b| *b) || leading >= filled {
        return false;
    }
    first.iter().zip(&second).any(|(a, b)| {
        let (ka, kb) = (kind_of(a), kind_of(b));
        ka != Kind::Blank && kb != Kind::Blank && (ka != kb || !same_format(book, a, b))
    })
}
