//! The formula language: its tree, parsing, printing, and moving references when cells move.

pub mod ast;
pub mod parser;
pub mod print;

pub use ast::{BinOp, Expr, Formula, Point, Reference, Shape, TableRef};

use std::sync::Arc;

use crate::coord::{MAX_COLS, MAX_ROWS};

/// A formula copied by a number of rows and columns: relative references move, absolute ones stay, and a
/// reference moved off the sheet becomes `#REF!`.
pub fn shifted(expr: &Expr, rows: i64, cols: i64) -> Expr {
    expr.map_refs(&mut |r| {
        let mut r = r.clone();
        let move_point = |p: &mut Point, whole_rows: bool, whole_cols: bool| -> bool {
            if !p.row_abs && !whole_cols {
                let row = p.row as i64 + rows;
                if row < 0 || row >= MAX_ROWS as i64 {
                    return false;
                }
                p.row = row as u32;
            }
            if !p.col_abs && !whole_rows {
                let col = p.col as i64 + cols;
                if col < 0 || col >= MAX_COLS as i64 {
                    return false;
                }
                p.col = col as u32;
            }
            true
        };
        let whole_cols = r.shape == Shape::Columns;
        let whole_rows = r.shape == Shape::Rows;
        if !move_point(&mut r.first, whole_rows, whole_cols)
            || !move_point(&mut r.last, whole_rows, whole_cols)
        {
            return None;
        }
        Some(r)
    })
}

/// Which way rows or columns were inserted or deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Rows,
    Columns,
}

/// A formula after rows or columns were inserted (`count` positive) or deleted (`count` negative) at
/// `at` on the sheet called `sheet`. `own_sheet` is the name of the sheet the formula is on, which is
/// what a reference with no sheet means.
pub fn adjusted(
    expr: &Expr,
    sheet: &str,
    own_sheet: &str,
    axis: Axis,
    at: u32,
    count: i64,
) -> Expr {
    expr.map_refs(&mut |r| {
        let on = match &r.sheet {
            Some(name) => name.eq_ignore_ascii_case(sheet) && r.sheet_last.is_none(),
            None => own_sheet.eq_ignore_ascii_case(sheet),
        };
        if !on {
            return Some(r.clone());
        }
        adjust_reference(r, axis, at, count)
    })
}

/// One reference after an insert or a delete, or `None` when every cell it named was deleted.
pub fn adjust_reference(r: &Reference, axis: Axis, at: u32, count: i64) -> Option<Reference> {
    let mut r = r.clone();
    let whole = match axis {
        Axis::Rows => r.shape == Shape::Columns,
        Axis::Columns => r.shape == Shape::Rows,
    };
    if whole {
        return Some(r);
    }
    let limit = match axis {
        Axis::Rows => MAX_ROWS,
        Axis::Columns => MAX_COLS,
    } as i64;
    let get = |p: &Point| match axis {
        Axis::Rows => p.row as i64,
        Axis::Columns => p.col as i64,
    };
    let set = |p: &mut Point, v: i64| match axis {
        Axis::Rows => p.row = v as u32,
        Axis::Columns => p.col = v as u32,
    };
    let (mut first, mut last) = (get(&r.first), get(&r.last));
    let at = at as i64;
    if count > 0 {
        if first >= at {
            first += count;
        }
        if last >= at {
            last += count;
        }
        if first >= limit {
            return None;
        }
        last = last.min(limit - 1);
    } else {
        let gone = -count;
        let end = at + gone; // first row after the deleted ones
        let map = |v: i64| if v >= end { v - gone } else { v };
        let first_deleted = first >= at && first < end;
        let last_deleted = last >= at && last < end;
        if first_deleted && last_deleted {
            return None;
        }
        first = if first_deleted { at } else { map(first) };
        last = if last_deleted { at - 1 } else { map(last) };
        if last < first {
            return None;
        }
    }
    set(&mut r.first, first);
    set(&mut r.last, last);
    if r.shape == Shape::Cell && (r.first.row != r.last.row || r.first.col != r.last.col) {
        r.shape = Shape::Range;
    }
    Some(r)
}

/// A formula after the cells `from` were moved to start at `to` on the same sheet, which is what cut and
/// paste does to every reference that named them.
pub fn moved(
    expr: &Expr,
    sheet: &str,
    own_sheet: &str,
    from: crate::coord::Range,
    rows: i64,
    cols: i64,
) -> Expr {
    expr.map_refs(&mut |r| {
        let on = match &r.sheet {
            Some(name) => name.eq_ignore_ascii_case(sheet) && r.sheet_last.is_none(),
            None => own_sheet.eq_ignore_ascii_case(sheet),
        };
        if !on || !(from.contains(r.first.at()) && from.contains(r.last.at())) {
            return Some(r.clone());
        }
        let mut r = r.clone();
        for p in [&mut r.first, &mut r.last] {
            let row = p.row as i64 + rows;
            let col = p.col as i64 + cols;
            if row < 0 || col < 0 || row >= MAX_ROWS as i64 || col >= MAX_COLS as i64 {
                return None;
            }
            p.row = row as u32;
            p.col = col as u32;
        }
        Some(r)
    })
}

/// Every sheet name a formula mentions.
pub fn sheets_named(expr: &Expr) -> Vec<Arc<str>> {
    let mut names = Vec::new();
    expr.walk(&mut |e| {
        if let Expr::Ref(r) = e {
            if let Some(s) = &r.sheet {
                names.push(s.clone());
            }
        }
    });
    names
}

/// The four forms F4 cycles a reference through: `$A$1`, `A$1`, `$A1`, `A1`.
pub fn cycle_absolute(p: &mut Point) {
    let next = match (p.col_abs, p.row_abs) {
        (false, false) => (true, true),
        (true, true) => (false, true),
        (false, true) => (true, false),
        (true, false) => (false, false),
    };
    p.col_abs = next.0;
    p.row_abs = next.1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Expr {
        parser::parse(text).unwrap()
    }

    fn s(expr: &Expr) -> String {
        print::print(expr, false)
    }

    #[test]
    fn copying_moves_relative_references_and_keeps_absolute_ones() {
        assert_eq!(s(&shifted(&p("A1+$B$1+C$1+$D1"), 2, 1)), "B3+$B$1+D$1+$D3");
        assert_eq!(s(&shifted(&p("SUM(A:A)"), 5, 1)), "SUM(B:B)");
        assert_eq!(s(&shifted(&p("A1"), -1, 0)), "#REF!");
    }

    #[test]
    fn inserting_and_deleting_rows_moves_and_resizes_references() {
        let insert = |t: &str| s(&adjusted(&p(t), "Sheet1", "Sheet1", Axis::Rows, 2, 3));
        assert_eq!(insert("A1+A3+SUM(A2:A4)+$A$5"), "A1+A6+SUM(A2:A7)+$A$8");
        assert_eq!(insert("Sheet2!A3"), "Sheet2!A3");
        let delete = |t: &str| s(&adjusted(&p(t), "Sheet1", "Sheet1", Axis::Rows, 2, -2));
        assert_eq!(delete("A1+A3+A5+SUM(A2:A6)+SUM(A3:A4)"), "A1+#REF!+A3+SUM(A2:A4)+SUM(#REF!)");
        let columns = |t: &str| s(&adjusted(&p(t), "Sheet1", "Sheet1", Axis::Columns, 1, 1));
        assert_eq!(columns("A1+B1+SUM(A1:C1)+SUM(3:3)"), "A1+C1+SUM(A1:D1)+SUM(3:3)");
    }

    #[test]
    fn f4_cycles_through_the_four_forms() {
        let mut point = Point { row: 0, col: 0, row_abs: false, col_abs: false };
        let mut seen = Vec::new();
        for _ in 0..4 {
            cycle_absolute(&mut point);
            seen.push((point.col_abs, point.row_abs));
        }
        assert_eq!(seen, vec![(true, true), (false, true), (true, false), (false, false)]);
    }
}
