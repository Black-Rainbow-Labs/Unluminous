//! Where a drawing object is, in the units a drawing part uses (English Metric Units), and how a position
//! in those units is a cell and an offset into it.
//!
//! A column is `round(width * 7)` pixels and a pixel is 9,525 units; a row is its height in points and a
//! point is 12,700 units. Both were measured against the anchors Excel writes: a picture at column A offset
//! 254,000 and row 2 offset 73,025 has its frame at x 254,000 and y 254,000 beside a 14.25 point first row.

use crate::coord::{CellRef, MAX_COLS, MAX_ROWS};
use crate::model::Sheet;

pub const EMU_PER_PIXEL: i64 = 9_525;
pub const EMU_PER_POINT: i64 = 12_700;

/// The width Excel gives a column nothing sets, in the file's characters.
const DEFAULT_COLUMN_CHARACTERS: f64 = 9.140625;

/// A rectangle in drawing units: its top left corner and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    pub fn new(x: i64, y: i64, w: i64, h: i64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> i64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i64 {
        self.y + self.h
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.x as f64 + self.w as f64 / 2.0, self.y as f64 + self.h as f64 / 2.0)
    }

    pub fn contains(&self, x: i64, y: i64) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(x, y, self.right().max(other.right()) - x, self.bottom().max(other.bottom()) - y)
    }

    /// The rectangle a rotation of `degrees` clockwise about this one's centre covers.
    pub fn rotated_bounds(&self, degrees: f64) -> Rect {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (w, h) = (self.w as f64, self.h as f64);
        let bw = (w * cos.abs() + h * sin.abs()).round() as i64;
        let bh = (w * sin.abs() + h * cos.abs()).round() as i64;
        let (cx, cy) = self.centre();
        Rect::new(
            (cx - bw as f64 / 2.0).round() as i64,
            (cy - bh as f64 / 2.0).round() as i64,
            bw,
            bh,
        )
    }
}

/// A column's width in pixels as Excel lays it out.
fn column_pixels(sheet: &Sheet, col: u32) -> i64 {
    let per_character = if sheet.drawing.char_px > 0.0 { sheet.drawing.char_px } else { 7.0 };
    let default = match sheet.default_col_width {
        Some(w) => w * per_character,
        // A column nothing sets is 72 pixels with Aptos Narrow and 64 with Calibri.
        None if per_character >= 8.0 => 72.0,
        None => DEFAULT_COLUMN_CHARACTERS * per_character,
    };
    match sheet.column(col) {
        Some(info) if info.hidden => 0,
        Some(info) => info.width.map_or(default, |w| w * per_character).round() as i64,
        None => default.round() as i64,
    }
}

/// Record, on every sheet, how many pixels a character of the workbook's default font is wide, which the
/// positions of objects depend on.
pub fn set_metrics(book: &mut crate::model::Workbook) {
    let per_character = crate::ods::pixels_per_character(book);
    for sheet in &mut book.sheets {
        sheet.drawing.char_px = per_character;
    }
}

/// A row's height in drawing units.
fn row_units(sheet: &Sheet, row: u32) -> i64 {
    let info = sheet.rows.get(&row);
    if info.is_some_and(|i| i.hidden) {
        return 0;
    }
    let points = info.and_then(|i| i.height).unwrap_or(sheet.default_row_height);
    (points * EMU_PER_POINT as f64).round() as i64
}

/// The width of a column in drawing units.
pub fn column_width(sheet: &Sheet, col: u32) -> i64 {
    column_pixels(sheet, col) * EMU_PER_PIXEL
}

/// The height of a row in drawing units.
pub fn row_height(sheet: &Sheet, row: u32) -> i64 {
    row_units(sheet, row)
}

/// The distance from the left edge of the sheet to the left edge of a column.
pub fn column_x(sheet: &Sheet, col: u32) -> i64 {
    (0..col.min(MAX_COLS)).map(|c| column_width(sheet, c)).sum()
}

/// The distance from the top edge of the sheet to the top edge of a row.
pub fn row_y(sheet: &Sheet, row: u32) -> i64 {
    let row = row.min(MAX_ROWS);
    let custom: Vec<(u32, i64)> =
        sheet.rows.range(..row).map(|(&r, _)| (r, row_units(sheet, r))).collect();
    let default = (sheet.default_row_height * EMU_PER_POINT as f64).round() as i64;
    let custom_total: i64 = custom.iter().map(|(_, h)| h).sum();
    default * (row as i64 - custom.len() as i64) + custom_total
}

/// The column holding a distance from the left edge, and how far into it the distance is.
pub fn column_at(sheet: &Sheet, x: i64) -> (u32, i64) {
    let mut left = x.max(0);
    for col in 0..MAX_COLS {
        let width = column_width(sheet, col);
        if left < width || col == MAX_COLS - 1 {
            return (col, left.min(width));
        }
        left -= width;
    }
    (MAX_COLS - 1, 0)
}

/// The row holding a distance from the top edge, and how far into it the distance is.
pub fn row_at(sheet: &Sheet, y: i64) -> (u32, i64) {
    let mut top = y.max(0);
    let default = (sheet.default_row_height * EMU_PER_POINT as f64).round().max(1.0) as i64;
    let mut row = 0u32;
    let customs: Vec<(u32, i64)> =
        sheet.rows.iter().map(|(&r, _)| (r, row_units(sheet, r))).collect();
    for (r, height) in customs {
        let before = (r - row) as i64 * default;
        if top < before {
            break;
        }
        top -= before;
        row = r;
        if top < height {
            return (row, top);
        }
        top -= height;
        row += 1;
    }
    let extra = top / default;
    let row = (row as i64 + extra).min(MAX_ROWS as i64 - 1) as u32;
    (row, top - extra * default)
}

/// The position of a cell and an offset into it, as a distance from the top left corner of the sheet.
pub fn point_of(sheet: &Sheet, at: (CellRef, i64, i64)) -> (i64, i64) {
    (column_x(sheet, at.0.col) + at.1, row_y(sheet, at.0.row) + at.2)
}

/// The cell and offset a distance from the top left corner of the sheet falls in.
pub fn cell_at(sheet: &Sheet, x: i64, y: i64) -> (CellRef, i64, i64) {
    let (col, dx) = column_at(sheet, x);
    let (row, dy) = row_at(sheet, y);
    (CellRef::new(row, col), dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_in_a_default_sheet_is_a_cell_and_an_offset_and_back() {
        let sheet = Sheet::new("S");
        // Column A is 64 pixels, the first row 14.25 points: the measured anchor of a picture.
        assert_eq!(point_of(&sheet, (CellRef::new(1, 0), 254_000, 73_025)), (254_000, 254_000));
        let (at, dx, dy) = cell_at(&sheet, 254_000, 254_000);
        assert_eq!((at, dx, dy), (CellRef::new(1, 0), 254_000, 73_025));
    }

    #[test]
    fn a_wide_column_and_a_tall_row_move_everything_after_them() {
        let mut sheet = Sheet::new("S");
        sheet.edit_column(0, |c| c.width = Some(20.0));
        sheet.rows.entry(0).or_default().height = Some(30.0);
        assert_eq!(column_x(&sheet, 1), 140 * EMU_PER_PIXEL);
        assert_eq!(row_y(&sheet, 1), 30 * EMU_PER_POINT);
        let (at, dx, dy) = cell_at(&sheet, 140 * EMU_PER_PIXEL + 5, 30 * EMU_PER_POINT + 7);
        assert_eq!((at, dx, dy), (CellRef::new(1, 1), 5, 7));
    }

    #[test]
    fn hidden_lines_have_no_size() {
        let mut sheet = Sheet::new("S");
        sheet.edit_column(1, |c| c.hidden = true);
        assert_eq!(column_x(&sheet, 2), column_x(&sheet, 1));
        sheet.rows.entry(2).or_default().hidden = true;
        assert_eq!(row_y(&sheet, 3), row_y(&sheet, 2));
    }

    #[test]
    fn a_rotated_rectangle_is_covered_by_its_bounds() {
        let r = Rect::new(0, 0, 200, 100).rotated_bounds(90.0);
        assert_eq!((r.w, r.h), (100, 200));
        assert_eq!(r.centre(), (100.0, 50.0));
    }
}
