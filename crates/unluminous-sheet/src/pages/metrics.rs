//! What the printer makes of a sheet's rows and columns.
//!
//! Excel does not print a column of 64 screen pixels as 64 pixels' worth of paper. It lays the page out in the
//! printer's pixels (600 to the inch for Microsoft Print to PDF) and sizes each column and row there from the
//! workbook's default font. These were measured against Excel 16 with `tools/excel-oracle/page-breaks.ps1`,
//! whose answers are in `tests/fixtures/pages`:
//!
//! - A column is `round(screen pixels * digit ratio)` printer pixels, the digit ratio being how wide the widest
//!   digit of the default font is at 600 dpi against its width on screen at 96 dpi (47 against 7 for Calibri 11,
//!   which makes a 64 pixel column 430 printer pixels, where plain arithmetic would give 400).
//! - A row whose height was set is eight printer pixels for each point of its height,, rounded. A row nobody sized is
//!   the printer's line height for the default font, 121 for Calibri 11, one more than six times its 20 pixels.

use crate::model::{Sheet, Workbook};

/// Printer pixels a point of a row's set height takes, to the nearest: a row of 9.95 points, as Excel
/// saves one set to 10, is 80 pixels.
const PIXELS_PER_POINT_OF_HEIGHT: f64 = 8.0;

/// The measurements a page layout needs for the workbook's default font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Pixels the widest digit takes on screen at 96 dpi.
    pub screen_digit: f64,
    /// Printer pixels the widest digit takes at 600 dpi.
    pub printer_digit: f64,
    /// Printer pixels a row takes when its height was not set.
    pub default_row: f64,
}

impl Metrics {
    /// Printer pixels a screen pixel of a column's width takes.
    pub fn column_ratio(&self) -> f64 {
        self.printer_digit / self.screen_digit
    }

    /// The measurements for a font: from the table of fonts Excel was measured with, or an estimate from the
    /// font's size for one that is not in it.
    pub fn for_font(name: &str, size: f64) -> Metrics {
        let key = name.trim().to_ascii_lowercase();
        match FONT_METRICS.iter().find(|m| m.0 == key && (m.1 - size).abs() < 0.01) {
            Some(&(_, _, screen_digit, printer_digit, default_row)) => {
                Metrics { screen_digit, printer_digit, default_row }
            }
            None => Metrics::estimate(size),
        }
    }

    /// The measurements for the workbook's default font, which is the first font in its styles.
    pub fn for_workbook(book: &Workbook) -> Metrics {
        match book.styles.fonts.first() {
            Some(font) => Metrics::for_font(&font.name, font.size),
            None => Metrics::for_font("Calibri", 11.0),
        }
    }

    /// A font that is not in the table: digits half an em wide and a line a third taller than the em, which is
    /// close to the common faces.
    fn estimate(size: f64) -> Metrics {
        let screen_digit = (size * 96.0 / 72.0 * 0.5).round().max(4.0);
        let printer_digit = (size * 600.0 / 72.0 * 0.5).round().max(4.0);
        let default_row = (size * 600.0 / 72.0 * 1.32).round();
        Metrics { screen_digit, printer_digit, default_row }
    }
}

/// `(lowercase name, size, screen digit, printer digit, default row)`, measured with Excel 16 by the `fonts` part of
/// `tools/excel-oracle/page-breaks.ps1`: the screen digit from the width of a column of ten characters, the
/// printer digit from how many columns of five widths fit on a Letter page, and the default row from how many
/// small rows fit after forty default ones.
const FONT_METRICS: &[(&str, f64, f64, f64, f64)] = &[
    ("calibri", 9.0, 6.0, 38.0, 99.0),
    ("calibri", 10.0, 7.0, 42.0, 109.0),
    ("calibri", 11.0, 7.0, 47.0, 121.0),
    ("calibri", 12.0, 8.0, 51.0, 130.0),
    ("calibri", 14.0, 10.0, 59.0, 153.0),
    ("arial", 9.0, 7.0, 42.0, 83.0),
    ("arial", 10.0, 7.0, 46.0, 103.0),
    ("arial", 11.0, 8.0, 51.0, 114.0),
    ("arial", 12.0, 9.0, 56.0, 123.0),
    ("arial", 14.0, 11.0, 65.0, 145.0),
    ("times new roman", 9.0, 6.0, 38.0, 96.0),
    ("times new roman", 10.0, 6.0, 42.0, 106.0),
    ("times new roman", 11.0, 7.0, 46.0, 111.0),
    ("times new roman", 12.0, 8.0, 50.0, 126.0),
    ("times new roman", 14.0, 9.0, 59.0, 145.0),
    ("aptos narrow", 9.0, 7.0, 42.0, 83.0),
    ("aptos narrow", 10.0, 7.0, 46.0, 103.0),
    ("aptos narrow", 11.0, 8.0, 51.0, 114.0),
    ("aptos narrow", 12.0, 9.0, 56.0, 123.0),
    ("aptos narrow", 14.0, 11.0, 65.0, 145.0),
    ("aptos", 9.0, 6.0, 40.0, 106.0),
    ("aptos", 10.0, 7.0, 44.0, 116.0),
    ("aptos", 11.0, 8.0, 49.0, 127.0),
    ("aptos", 12.0, 9.0, 53.0, 137.0),
    ("aptos", 14.0, 10.0, 62.0, 162.0),
    ("segoe ui", 9.0, 6.0, 40.0, 118.0),
    ("segoe ui", 10.0, 7.0, 45.0, 124.0),
    ("segoe ui", 11.0, 8.0, 50.0, 138.0),
    ("segoe ui", 12.0, 9.0, 54.0, 149.0),
    ("segoe ui", 14.0, 10.0, 63.0, 172.0),
    ("verdana", 9.0, 8.0, 48.0, 100.0),
    ("verdana", 10.0, 8.0, 53.0, 108.0),
    ("verdana", 11.0, 10.0, 58.0, 122.0),
    ("verdana", 12.0, 10.0, 64.0, 129.0),
    ("verdana", 14.0, 12.0, 74.0, 153.0),
    ("tahoma", 9.0, 7.0, 41.0, 98.0),
    ("tahoma", 10.0, 7.0, 45.0, 108.0),
    ("tahoma", 11.0, 8.0, 50.0, 119.0),
    ("tahoma", 12.0, 9.0, 55.0, 129.0),
    ("tahoma", 14.0, 10.0, 64.0, 149.0),
    ("consolas", 9.0, 7.0, 41.0, 96.0),
    ("consolas", 10.0, 7.0, 46.0, 105.0),
    ("consolas", 11.0, 8.0, 51.0, 111.0),
    ("consolas", 12.0, 9.0, 55.0, 125.0),
    ("consolas", 14.0, 10.0, 64.0, 145.0),
    ("courier new", 9.0, 7.0, 45.0, 77.0),
    ("courier new", 10.0, 8.0, 50.0, 87.0),
    ("courier new", 11.0, 9.0, 55.0, 100.0),
    ("courier new", 12.0, 10.0, 60.0, 110.0),
    ("courier new", 14.0, 11.0, 71.0, 131.0),
    ("cambria", 9.0, 7.0, 42.0, 96.0),
    ("cambria", 10.0, 7.0, 46.0, 105.0),
    ("cambria", 11.0, 8.0, 51.0, 115.0),
    ("cambria", 12.0, 9.0, 55.0, 125.0),
    ("cambria", 14.0, 11.0, 65.0, 145.0),
    ("georgia", 9.0, 8.0, 46.0, 94.0),
    ("georgia", 10.0, 9.0, 51.0, 104.0),
    ("georgia", 11.0, 9.0, 56.0, 113.0),
    ("georgia", 12.0, 10.0, 62.0, 122.0),
    ("georgia", 14.0, 12.0, 72.0, 141.0),
    ("trebuchet ms", 9.0, 6.0, 39.0, 95.0),
    ("trebuchet ms", 10.0, 7.0, 44.0, 107.0),
    ("trebuchet ms", 11.0, 8.0, 48.0, 120.0),
    ("trebuchet ms", 12.0, 8.0, 52.0, 131.0),
    ("trebuchet ms", 14.0, 10.0, 62.0, 158.0),
    ("comic sans ms", 9.0, 7.0, 46.0, 121.0),
    ("comic sans ms", 10.0, 8.0, 51.0, 133.0),
    ("comic sans ms", 11.0, 9.0, 56.0, 149.0),
    ("comic sans ms", 12.0, 10.0, 62.0, 162.0),
    ("comic sans ms", 14.0, 12.0, 72.0, 183.0),
];

/// A column's width in printer pixels at full scale; zero when it is hidden.
pub fn column_pixels(sheet: &Sheet, col: u32, metrics: &Metrics) -> f64 {
    if sheet.is_column_hidden(col) {
        return 0.0;
    }
    (screen_column_pixels(sheet, col, metrics) * metrics.column_ratio()).round()
}

/// A column's width on screen in pixels: the width the file gives it, or the sheet's default.
pub fn screen_column_pixels(sheet: &Sheet, col: u32, metrics: &Metrics) -> f64 {
    match sheet.column(col).and_then(|c| c.width) {
        Some(w) => (w * metrics.screen_digit).round(),
        None => default_column_pixels(sheet, metrics),
    }
}

/// The width of a column nothing sets: a stated default width, or eight characters and the padding rounded up
/// to a multiple of eight pixels, which is how Excel makes it (64 pixels in Calibri 11).
fn default_column_pixels(sheet: &Sheet, metrics: &Metrics) -> f64 {
    if let Some(w) = sheet.default_col_width {
        return (w * metrics.screen_digit).round();
    }
    let padding = 2.0 * (metrics.screen_digit / 4.0).ceil() + 1.0;
    ((8.0 * metrics.screen_digit + padding) / 8.0).ceil() * 8.0
}

/// A row's height in printer pixels at full scale; zero when it is hidden.
pub fn row_pixels(sheet: &Sheet, row: u32, metrics: &Metrics) -> f64 {
    let info = sheet.rows.get(&row);
    if info.is_some_and(|i| i.hidden) {
        return 0.0;
    }
    match info.and_then(|i| i.height) {
        Some(h) if (h - sheet.default_row_height).abs() > 1e-6 => {
            (h * PIXELS_PER_POINT_OF_HEIGHT).round()
        }
        _ => metrics.default_row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ColumnInfo, RowInfo};

    fn calibri() -> Metrics {
        Metrics::for_font("Calibri", 11.0)
    }

    #[test]
    fn a_default_column_is_64_screen_pixels_and_430_printer_pixels() {
        let sheet = Sheet::new("S");
        assert_eq!(screen_column_pixels(&sheet, 0, &calibri()), 64.0);
        assert_eq!(column_pixels(&sheet, 0, &calibri()), 430.0);
    }

    #[test]
    fn a_stated_default_width_wins() {
        let mut sheet = Sheet::new("S");
        sheet.default_col_width = Some(10.0);
        assert_eq!(screen_column_pixels(&sheet, 3, &calibri()), 70.0);
    }

    #[test]
    fn a_column_width_in_characters_is_pixels_of_the_digit() {
        let mut sheet = Sheet::new("S");
        sheet.columns.push(ColumnInfo {
            first: 1,
            last: 1,
            width: Some(10.7109375),
            hidden: false,
            style: 0,
            custom_width: true,
            outline: 0,
            collapsed: false,
            best_fit: false,
        });
        assert_eq!(screen_column_pixels(&sheet, 1, &calibri()), 75.0);
    }

    #[test]
    fn a_hidden_row_or_column_is_nothing() {
        let mut sheet = Sheet::new("S");
        sheet.rows.insert(2, RowInfo { hidden: true, ..RowInfo::default() });
        assert_eq!(row_pixels(&sheet, 2, &calibri()), 0.0);
    }

    #[test]
    fn a_row_nobody_sized_is_the_printers_line_and_a_set_row_is_six_pixels_a_screen_pixel() {
        let mut sheet = Sheet::new("S");
        sheet.default_row_height = 15.0;
        sheet.rows.insert(1, RowInfo { height: Some(30.0), ..RowInfo::default() });
        assert_eq!(row_pixels(&sheet, 0, &calibri()), 121.0);
        assert_eq!(row_pixels(&sheet, 1, &calibri()), 240.0);
    }

    #[test]
    fn a_font_not_in_the_table_is_estimated_from_its_size() {
        let m = Metrics::for_font("Nowhere Sans", 11.0);
        assert!(
            m.screen_digit >= 6.0 && m.printer_digit >= 40.0 && m.default_row >= 100.0,
            "{m:?}"
        );
    }
}
