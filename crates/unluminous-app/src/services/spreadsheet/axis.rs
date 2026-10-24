//! Where each column and row of a sheet is, in points at a zoom of one.
//!
//! A sheet has 16,384 columns and 1,048,576 rows and nearly all of them are the default size, so an axis
//! is the default and a sorted list of the few that are not. Where an index starts is the default times
//! the index plus what the exceptions before it add, found by a binary search; which index a position
//! falls in is the same search the other way. Built again when the workbook's revision moves.
//!
//! **Excel's units.** A column width in the file is in characters of the default font's widest digit,
//! which is seven pixels for Calibri and Aptos at 11 points, so a column is `round(width * 7)` pixels and
//! the default of 8.43 characters (9.140625 in the file) is 64. A row height is in points, and a pixel is
//! three quarters of a point. Unluminous's points are a screen's pixels at a zoom of one, so a column of 64
//! pixels is 64 points here.

use unluminous_sheet::coord::{MAX_COLS, MAX_ROWS};
use unluminous_sheet::Sheet;

/// One axis of a sheet: columns or rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    /// How many there are.
    pub count: u32,
    pub default: f32,
    /// Indexes whose size is not the default, in order, with their size. A hidden one has size 0.
    exceptions: Vec<(u32, f32)>,
    /// For each exception, how much the ones before it and itself add over the default.
    added: Vec<f32>,
}

/// The pixels a column width in the file's characters is.
pub fn width_to_points(width: f64) -> f32 {
    (width * 7.0).round() as f32
}

/// The file's characters for a column of so many points: what a drag to resize a column writes. Excel's
/// own arithmetic: the characters the column shows to two places, then the file's width in 256ths.
pub fn points_to_width(points: f32) -> f64 {
    let pixels = points.max(0.0).round() as f64;
    let characters = (((pixels - 5.0).max(0.0) / 7.0) * 100.0 + 0.5).trunc() / 100.0;
    ((characters * 7.0 + 5.0) / 7.0 * 256.0).trunc() / 256.0
}

/// The width a column has when nothing sets one, in the file's characters.
pub const DEFAULT_WIDTH: f64 = 9.140625;

/// The points a row height in points is drawn at.
pub fn height_to_points(height: f64) -> f32 {
    (height * 4.0 / 3.0).round() as f32
}

/// A row height in the file's points for a row of so many screen points.
pub fn points_to_height(points: f32) -> f64 {
    ((points.max(0.0) as f64 * 0.75) * 4.0).round() / 4.0
}

impl Axis {
    fn build(count: u32, default: f32, mut exceptions: Vec<(u32, f32)>) -> Axis {
        exceptions.sort_by_key(|(index, _)| *index);
        exceptions.dedup_by_key(|(index, _)| *index);
        let mut added = Vec::with_capacity(exceptions.len());
        let mut sum = 0.0;
        for (_, size) in &exceptions {
            sum += size - default;
            added.push(sum);
        }
        Axis { count, default, exceptions, added }
    }

    /// The columns of a sheet.
    pub fn columns(sheet: &Sheet) -> Axis {
        let default = width_to_points(sheet.default_col_width.unwrap_or(DEFAULT_WIDTH));
        let mut exceptions = Vec::new();
        for info in &sheet.columns {
            let size = if info.hidden { 0.0 } else { info.width.map_or(default, width_to_points) };
            if size != default {
                // A range of columns all sized alike is common (`A:XFD` styled). Listed one a column up to
                // the end of a sensible sheet, beyond which the rest keep the default.
                for col in info.first..=info.last.min(info.first + 4096) {
                    exceptions.push((col, size));
                }
            }
        }
        Axis::build(MAX_COLS, default, exceptions)
    }

    /// The rows of a sheet.
    pub fn rows(sheet: &Sheet) -> Axis {
        let default = height_to_points(sheet.default_row_height);
        let mut exceptions = Vec::new();
        for (row, info) in &sheet.rows {
            let size =
                if info.hidden { 0.0 } else { info.height.map_or(default, height_to_points) };
            if size != default {
                exceptions.push((*row, size));
            }
        }
        Axis::build(MAX_ROWS, default, exceptions)
    }

    /// How many exceptions lie strictly before `index`.
    fn before(&self, index: u32) -> usize {
        self.exceptions.partition_point(|(at, _)| *at < index)
    }

    /// Where `index` starts.
    pub fn start(&self, index: u32) -> f32 {
        let n = self.before(index);
        index as f32 * self.default + if n == 0 { 0.0 } else { self.added[n - 1] }
    }

    /// How large `index` is.
    pub fn size(&self, index: u32) -> f32 {
        let n = self.before(index);
        match self.exceptions.get(n) {
            Some((at, size)) if *at == index => *size,
            _ => self.default,
        }
    }

    /// Where `index` ends.
    pub fn end(&self, index: u32) -> f32 {
        self.start(index) + self.size(index)
    }

    /// The whole length.
    pub fn total(&self) -> f32 {
        self.start(self.count)
    }

    /// The index at a position, clamped to the axis. A position on a hidden one gives the next shown one.
    pub fn index_at(&self, position: f32) -> u32 {
        if position <= 0.0 {
            return 0;
        }
        let (mut low, mut high) = (0u32, self.count - 1);
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if self.start(middle) <= position {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        // Past hidden ones of size zero, which share their start with the next.
        let mut index = low;
        while index + 1 < self.count && self.size(index) == 0.0 && self.start(index + 1) <= position
        {
            index += 1;
        }
        index
    }

    /// Whether `index` is hidden.
    pub fn is_hidden(&self, index: u32) -> bool {
        self.size(index) == 0.0
    }

    /// The next shown index after `index` going by `step`, or `index` itself at the edge.
    pub fn next_shown(&self, index: u32, step: i64) -> u32 {
        let mut at = index as i64;
        loop {
            let next = at + step.signum();
            if next < 0 || next >= self.count as i64 {
                return index;
            }
            at = next;
            if !self.is_hidden(at as u32) {
                return at as u32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_add_the_exceptions_before_them() {
        let axis = Axis::build(100, 10.0, vec![(3, 30.0), (5, 0.0)]);
        assert_eq!(axis.start(3), 30.0);
        assert_eq!(axis.start(4), 60.0);
        assert_eq!(axis.start(6), 70.0);
        assert_eq!(axis.size(5), 0.0);
        assert_eq!(axis.index_at(65.0), 4);
        assert_eq!(axis.index_at(70.0), 6);
        assert_eq!(axis.index_at(35.0), 3);
        assert_eq!(axis.next_shown(4, 1), 6);
        assert_eq!(axis.next_shown(6, -1), 4);
    }

    #[test]
    fn excel_units_come_out_as_excel_draws_them() {
        assert_eq!(width_to_points(DEFAULT_WIDTH), 64.0);
        assert_eq!(height_to_points(15.0), 20.0);
        assert_eq!(points_to_width(64.0), 9.140625);
        assert_eq!(points_to_height(20.0), 15.0);
    }
}
