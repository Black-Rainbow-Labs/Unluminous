//! Where a note's box sits: the anchor Excel positions it by, and the same place in points.
//!
//! A note's VML shape is positioned by `x:Anchor`, which names the cell the box starts in, how far into it
//! in pixels, and the same for the cell it ends in. The shape's style says the same thing in points. Going
//! from one to the other needs the width of every column and the height of every row on the way, which is
//! what [`Geometry`] reads from the sheet. A column's width in pixels depends on the widest digit of the
//! workbook's default font, which Excel takes from the screen it runs on, so the digit width is a field.

use crate::coord::CellRef;
use crate::model::{NoteBox, Sheet};

/// How wide a note's box is when Excel makes it, in points, and how tall.
const NEW_WIDTH: f64 = 108.0;
const NEW_HEIGHT: f64 = 59.25;

/// The pixel size of the columns and rows of one sheet.
#[derive(Debug, Clone, Copy)]
pub struct Geometry<'a> {
    pub sheet: &'a Sheet,
    /// The width in pixels of the widest digit of the default font: 7 for Calibri 11 at 100%.
    pub digit_width: f64,
}

impl<'a> Geometry<'a> {
    /// The geometry Excel has at 100% zoom with its default font.
    pub fn standard(sheet: &'a Sheet) -> Self {
        Self { sheet, digit_width: 7.0 }
    }

    /// A column's width in pixels: a hidden column has none, and one with no width of its own has the
    /// default, which Excel rounds up to a multiple of eight pixels.
    pub fn column_px(&self, col: u32) -> i32 {
        let digit = self.digit_width;
        if let Some(info) = self.sheet.column(col) {
            if info.hidden {
                return 0;
            }
            if let Some(w) = info.width {
                return (((256.0 * w + (128.0 / digit).trunc()) / 256.0) * digit).trunc() as i32;
            }
        }
        match self.sheet.default_col_width {
            Some(w) => (((256.0 * w + (128.0 / digit).trunc()) / 256.0) * digit).trunc() as i32,
            None => ((8.0 * digit + 5.0) / 8.0).ceil() as i32 * 8,
        }
    }

    /// A row's height in pixels; a hidden row has none.
    pub fn row_px(&self, row: u32) -> i32 {
        match self.sheet.rows.get(&row) {
            Some(info) if info.hidden => 0,
            Some(info) if info.height.is_some() => points_to_px(info.height.unwrap_or(0.0)),
            _ => points_to_px(self.sheet.default_row_height),
        }
    }

    /// Where a column starts, in pixels from the left of the sheet.
    pub fn column_start(&self, col: u32) -> i32 {
        (0..col).map(|c| self.column_px(c)).sum()
    }

    /// Where a row starts, in pixels from the top of the sheet.
    pub fn row_start(&self, row: u32) -> i32 {
        (0..row).map(|r| self.row_px(r)).sum()
    }

    /// The column a distance from the left of the sheet falls in, and how far into it.
    fn column_at(&self, px: i32) -> (i32, i32) {
        let (mut col, mut start) = (0u32, 0);
        loop {
            let width = self.column_px(col);
            if start + width > px || col >= crate::coord::MAX_COLS - 1 {
                return (col as i32, px - start);
            }
            start += width;
            col += 1;
        }
    }

    /// The row a distance from the top of the sheet falls in, and how far into it.
    fn row_at(&self, px: i32) -> (i32, i32) {
        let (mut row, mut start) = (0u32, 0);
        loop {
            let height = self.row_px(row);
            if start + height > px || row >= crate::coord::MAX_ROWS - 1 {
                return (row as i32, px - start);
            }
            start += height;
            row += 1;
        }
    }

    /// A box from its place and size in points: the anchor is worked out from the sheet's geometry.
    pub fn box_from_points(&self, left: f64, top: f64, width: f64, height: f64) -> NoteBox {
        let (x0, y0) = (points_to_px(left), points_to_px(top));
        let (x1, y1) = (x0 + points_to_px(width), y0 + points_to_px(height));
        let ((lc, lo), (tr, to)) = (self.column_at(x0), self.row_at(y0));
        let ((rc, ro), (br, bo)) = (self.column_at(x1), self.row_at(y1));
        NoteBox {
            anchor: [lc, lo, tr, to, rc, ro, br, bo],
            left: px_to_points(x0),
            top: px_to_points(y0),
            width: px_to_points(x1 - x0),
            height: px_to_points(y1 - y0),
        }
    }

    /// The place and size in points an anchor names.
    pub fn points_of(&self, anchor: [i32; 8]) -> (f64, f64, f64, f64) {
        let x0 = self.column_start(anchor[0].max(0) as u32) + anchor[1];
        let y0 = self.row_start(anchor[2].max(0) as u32) + anchor[3];
        let x1 = self.column_start(anchor[4].max(0) as u32) + anchor[5];
        let y1 = self.row_start(anchor[6].max(0) as u32) + anchor[7];
        (px_to_points(x0), px_to_points(y0), px_to_points(x1 - x0), px_to_points(y1 - y0))
    }

    /// The box Excel gives a new note on a cell: 108 by 59.25 points, starting in the column to the right
    /// and a little above the cell's row.
    pub fn new_box(&self, at: CellRef) -> NoteBox {
        let x = self.column_start(at.col + 1) + 15;
        let y = match at.row {
            0 => 2,
            row => self.row_start(row - 1) + 9,
        };
        self.box_from_points(px_to_points(x), px_to_points(y), NEW_WIDTH, NEW_HEIGHT)
    }
}

/// The box Excel gives a new note on a cell of a sheet, at the standard geometry.
pub fn default_box(sheet: &Sheet, at: CellRef) -> NoteBox {
    Geometry::standard(sheet).new_box(at)
}

/// Points to whole pixels, at 96 pixels to the inch.
fn points_to_px(points: f64) -> i32 {
    (points * 4.0 / 3.0).round() as i32
}

/// Whole pixels to points.
fn px_to_points(px: i32) -> f64 {
    f64::from(px) * 0.75
}
