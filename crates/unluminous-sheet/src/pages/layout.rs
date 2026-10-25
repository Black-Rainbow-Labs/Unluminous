//! What the paginator returns: plain data a window paints and a PDF writer writes, with no behaviour in it.

use super::headerfooter::Sections;
use super::{PrintComments, PrintErrors};
use crate::coord::{CellRef, Range};

/// What to tell the header and footer codes, and how to measure.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The file's name, for `&F`.
    pub file: String,
    /// The folder, for `&Z`.
    pub path: String,
    /// The date as printed, for `&D`.
    pub date: String,
    /// The time as printed, for `&T`.
    pub time: String,
    /// Measurements to use in place of the table's, for a font Excel was not measured with.
    pub metrics: Option<super::metrics::Metrics>,
}

/// A place where one page ends and the next begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Break {
    /// The first row (or column) of the next page, zero based.
    pub at: u32,
    /// Put there by a person rather than by the page filling.
    pub manual: bool,
}

/// A width and a height in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// The margins in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarginPoints {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    /// From the top of the paper to the header.
    pub header: f64,
    /// From the bottom of the paper to the footer.
    pub footer: f64,
}

/// One range that prints on pages of its own, with the breaks inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaLayout {
    pub range: Range,
    /// Where a page of rows ends, including the break after the data when the last page is exactly full.
    pub row_breaks: Vec<Break>,
    /// Where a page of columns ends, with the same trailing break.
    pub col_breaks: Vec<Break>,
}

/// One printed page.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// Where the page is in the printout, from zero.
    pub index: usize,
    /// The number printed on it, which starts at the sheet's first page number.
    pub number: i32,
    /// Which print area it belongs to.
    pub area: usize,
    /// The cells of the area on this page.
    pub body: Range,
    /// Rows repeated at the top of the page, when it does not begin with them.
    pub title_rows: Option<(u32, u32)>,
    /// Columns repeated at the left of the page, when it does not begin with them.
    pub title_cols: Option<(u32, u32)>,
    /// The size of what is drawn, in points at the scale in use.
    pub content: Size,
    /// Where the content starts, in points from the top left corner of the paper.
    pub origin: (f64, f64),
    pub header: Sections,
    pub footer: Sections,
}

/// A note printed on the pages after the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteLine {
    pub cell: CellRef,
    pub author: String,
    pub text: String,
}

/// The whole printout of a sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLayout {
    pub sheet: usize,
    /// The paper in points, turned to the orientation in use.
    pub paper: Size,
    pub margins: MarginPoints,
    /// Percent, from the setup or from fitting.
    pub scale: u32,
    pub areas: Vec<AreaLayout>,
    /// The pages with something on them, in the order they print.
    pub pages: Vec<Page>,
    /// Print the cell gridlines.
    pub gridlines: bool,
    /// Print the row and column headings.
    pub headings: bool,
    pub black_and_white: bool,
    pub draft: bool,
    pub errors: PrintErrors,
    pub comments: PrintComments,
    /// The notes for pages after the sheet, when they print at the end, a page's worth at a time.
    pub note_pages: Vec<Vec<NoteLine>>,
    /// Printed width in points at the scale in use, for each column from A to the last printed.
    pub column_points: Vec<f64>,
    /// Printed height in points at the scale in use, for each row from 1 to the last printed.
    pub row_points: Vec<f64>,
}

impl PageLayout {
    /// Pages as Excel counts them: those with something on them, and the pages of notes.
    pub fn page_count(&self) -> usize {
        self.pages.len() + self.note_pages.len()
    }

    /// The breaks between rows of the first print area as Excel lists them in `HPageBreaks`: the one based row
    /// each new page starts at.
    pub fn row_break_rows(&self) -> Vec<u32> {
        self.areas
            .first()
            .map_or_else(Vec::new, |a| a.row_breaks.iter().map(|b| b.at + 1).collect())
    }

    /// The breaks between columns of the first print area as Excel lists them in `VPageBreaks`.
    pub fn col_break_cols(&self) -> Vec<u32> {
        self.areas
            .first()
            .map_or_else(Vec::new, |a| a.col_breaks.iter().map(|b| b.at + 1).collect())
    }
}

/// How a cell showing an error is printed: as shown, blank, as two dashes, or as `#N/A`.
pub fn printed_error(how: PrintErrors, shown: &str) -> String {
    match how {
        PrintErrors::Displayed => shown.to_owned(),
        PrintErrors::Blank => String::new(),
        PrintErrors::Dash => "--".to_owned(),
        PrintErrors::NotAvailable => "#N/A".to_owned(),
    }
}
