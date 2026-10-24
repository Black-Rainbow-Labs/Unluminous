//! Page setup and printing: what a sheet's `pageSetup`, `pageMargins`, `printOptions`, `headerFooter`,
//! `rowBreaks`, `colBreaks`, print area and print titles say, and the pages they make.
//!
//! - This file is the model: [`PageSetup`] with everything the file keeps about printing a sheet, and the
//!   print area and print titles, which the file keeps as defined names.
//! - [`xml`] reads those elements and writes them back in the schema's order, keeping any attribute the
//!   model does not hold.
//! - [`headerfooter`] expands the codes in a header or footer (`&P`, `&A`, `&"Arial,Bold"` and the rest).
//! - [`paginate`] decides where the pages break the way Excel does and returns the pages as plain data for
//!   the window to paint and for a PDF writer to write.
//!
//! `tasks/task-2357-spreadsheet-round2-tdd.md` section 4.5 is the design.

pub mod headerfooter;
pub mod layout;
pub mod metrics;
pub mod paginate;
pub mod paper;
pub mod xml;

use crate::coord::{column_name, Range, MAX_COLS, MAX_ROWS};
use crate::model::{DefinedName, Workbook};

/// The defined name that holds a sheet's print area.
pub const PRINT_AREA: &str = "_xlnm.Print_Area";
/// The defined name that holds a sheet's print titles.
pub const PRINT_TITLES: &str = "_xlnm.Print_Titles";

/// Which way the paper is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// The order pages are numbered in when a sheet is wider and taller than one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrder {
    #[default]
    DownThenOver,
    OverThenDown,
}

/// How notes are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintComments {
    #[default]
    None,
    /// On pages after the sheet, with the cell each belongs to.
    AtEnd,
    /// Where they are shown on the sheet.
    AsDisplayed,
}

/// How a cell showing an error is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintErrors {
    #[default]
    Displayed,
    Blank,
    Dash,
    NotAvailable,
}

/// The four edges and the two header and footer distances, in inches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header: f64,
    pub footer: f64,
}

impl Default for Margins {
    /// Excel's normal margins.
    fn default() -> Self {
        Self { left: 0.7, right: 0.7, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 }
    }
}

impl Margins {
    /// Excel's Narrow preset.
    pub fn narrow() -> Self {
        Self { left: 0.25, right: 0.25, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 }
    }

    /// Excel's Wide preset.
    pub fn wide() -> Self {
        Self { left: 1.0, right: 1.0, top: 1.0, bottom: 1.0, header: 0.5, footer: 0.5 }
    }
}

/// The `printOptions` element.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintOptions {
    pub horizontal_centered: bool,
    pub vertical_centered: bool,
    pub headings: bool,
    pub grid_lines: bool,
    /// `gridLinesSet`, which the schema defaults to true.
    pub grid_lines_set: bool,
    /// Attributes this version does not model, kept.
    pub extra: Vec<(String, String)>,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            horizontal_centered: false,
            vertical_centered: false,
            headings: false,
            grid_lines: false,
            grid_lines_set: true,
            extra: Vec::new(),
        }
    }
}

/// The text of the six headers and footers and the four switches that say how they are used.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderFooter {
    pub odd_header: String,
    pub odd_footer: String,
    pub even_header: String,
    pub even_footer: String,
    pub first_header: String,
    pub first_footer: String,
    pub different_odd_even: bool,
    pub different_first: bool,
    /// `scaleWithDoc`, which the schema defaults to true.
    pub scale_with_doc: bool,
    /// `alignWithMargins`, which the schema defaults to true.
    pub align_with_margins: bool,
    pub extra: Vec<(String, String)>,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        Self {
            odd_header: String::new(),
            odd_footer: String::new(),
            even_header: String::new(),
            even_footer: String::new(),
            first_header: String::new(),
            first_footer: String::new(),
            different_odd_even: false,
            different_first: false,
            scale_with_doc: true,
            align_with_margins: true,
            extra: Vec::new(),
        }
    }
}

/// Which of the six texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderFooterPart {
    OddHeader,
    OddFooter,
    EvenHeader,
    EvenFooter,
    FirstHeader,
    FirstFooter,
}

impl HeaderFooter {
    /// The text of one part.
    pub fn text(&self, part: HeaderFooterPart) -> &str {
        match part {
            HeaderFooterPart::OddHeader => &self.odd_header,
            HeaderFooterPart::OddFooter => &self.odd_footer,
            HeaderFooterPart::EvenHeader => &self.even_header,
            HeaderFooterPart::EvenFooter => &self.even_footer,
            HeaderFooterPart::FirstHeader => &self.first_header,
            HeaderFooterPart::FirstFooter => &self.first_footer,
        }
    }

    /// Replace the text of one part.
    pub fn set_text(&mut self, part: HeaderFooterPart, text: &str) {
        let slot = match part {
            HeaderFooterPart::OddHeader => &mut self.odd_header,
            HeaderFooterPart::OddFooter => &mut self.odd_footer,
            HeaderFooterPart::EvenHeader => &mut self.even_header,
            HeaderFooterPart::EvenFooter => &mut self.even_footer,
            HeaderFooterPart::FirstHeader => &mut self.first_header,
            HeaderFooterPart::FirstFooter => &mut self.first_footer,
        };
        *slot = text.to_owned();
    }

    /// Whether any text or switch differs from a sheet with no header or footer.
    pub fn is_default(&self) -> bool {
        *self == HeaderFooter::default()
    }
}

/// One manual or automatic page break: the first row (or column) of the page that follows it, counted from
/// zero, which is the file's one based `id` of the last row (or column) before the break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageBreak {
    /// Zero based index of the first row or column after the break.
    pub id: u32,
    /// The first column (or row) the break spans, as the file's `min`.
    pub min: u32,
    /// The last column (or row) the break spans, as the file's `max`.
    pub max: u32,
    /// Put there by a person rather than by Excel.
    pub manual: bool,
    /// The file's `pt`, which is set on a break made by dragging in Page Break Preview.
    pub pt: bool,
}

/// Everything a sheet keeps about printing.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    /// `None` when the file does not say, which is portrait.
    pub orientation: Option<Orientation>,
    pub paper_size: u32,
    pub paper_width: Option<String>,
    pub paper_height: Option<String>,
    /// Percent, 10 to 400. Ignored while [`PageSetup::fit_to_page`] is on.
    pub scale: u32,
    pub first_page_number: i32,
    pub use_first_page_number: bool,
    /// How many pages wide; zero is as many as it takes.
    pub fit_to_width: u32,
    /// How many pages tall; zero is as many as it takes.
    pub fit_to_height: u32,
    /// `sheetPr/pageSetUpPr/@fitToPage`: scale to fit rather than by percent.
    pub fit_to_page: bool,
    /// `sheetPr/pageSetUpPr/@autoPageBreaks`, which the schema defaults to true.
    pub auto_page_breaks: bool,
    pub page_order: PageOrder,
    pub use_printer_defaults: bool,
    pub black_and_white: bool,
    pub draft: bool,
    pub comments: PrintComments,
    pub errors: PrintErrors,
    pub horizontal_dpi: u32,
    pub vertical_dpi: u32,
    pub copies: u32,
    /// The relationship id of the printer settings part, kept so the part stays attached.
    pub printer_rid: Option<String>,
    /// `pageSetup` attributes this version does not model, kept.
    pub extra: Vec<(String, String)>,
    pub margins: Margins,
    pub options: PrintOptions,
    pub header_footer: HeaderFooter,
    /// Breaks between rows.
    pub row_breaks: Vec<PageBreak>,
    /// Breaks between columns.
    pub col_breaks: Vec<PageBreak>,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            orientation: None,
            paper_size: 1,
            paper_width: None,
            paper_height: None,
            scale: 100,
            first_page_number: 1,
            use_first_page_number: false,
            fit_to_width: 1,
            fit_to_height: 1,
            fit_to_page: false,
            auto_page_breaks: true,
            page_order: PageOrder::DownThenOver,
            use_printer_defaults: true,
            black_and_white: false,
            draft: false,
            comments: PrintComments::None,
            errors: PrintErrors::Displayed,
            horizontal_dpi: 600,
            vertical_dpi: 600,
            copies: 1,
            printer_rid: None,
            extra: Vec::new(),
            margins: Margins::default(),
            options: PrintOptions::default(),
            header_footer: HeaderFooter::default(),
            row_breaks: Vec::new(),
            col_breaks: Vec::new(),
        }
    }
}

impl PageSetup {
    /// Landscape or portrait; a file that does not say is portrait.
    pub fn orientation_or_default(&self) -> Orientation {
        self.orientation.unwrap_or(Orientation::Portrait)
    }

    /// Whether the sheet has any manual break.
    pub fn has_manual_breaks(&self) -> bool {
        self.row_breaks.iter().chain(&self.col_breaks).any(|b| b.manual)
    }

    /// The row breaks as the zero based first row of each page after a break, in order.
    pub fn row_break_rows(&self) -> Vec<u32> {
        sorted_ids(&self.row_breaks)
    }

    /// The column breaks as the zero based first column of each page after a break, in order.
    pub fn col_break_cols(&self) -> Vec<u32> {
        sorted_ids(&self.col_breaks)
    }
}

fn sorted_ids(breaks: &[PageBreak]) -> Vec<u32> {
    let mut ids: Vec<u32> = breaks.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The rows and columns repeated at the top and left of every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrintTitles {
    /// First and last row, zero based, both included.
    pub rows: Option<(u32, u32)>,
    /// First and last column, zero based, both included.
    pub cols: Option<(u32, u32)>,
}

// ----------------------------------------------------------------------------------------------------
// Print area and print titles, which the file keeps as defined names scoped to the sheet.

/// A sheet's print area: one range, or several that print on pages of their own. Empty when there is none.
pub fn print_area(book: &Workbook, sheet: usize) -> Vec<Range> {
    match name_formula(book, sheet, PRINT_AREA) {
        Some(formula) => {
            split_references(formula).into_iter().filter_map(|r| parse_reference(&r)).collect()
        }
        None => Vec::new(),
    }
}

/// A sheet's print titles.
pub fn print_titles(book: &Workbook, sheet: usize) -> PrintTitles {
    let mut titles = PrintTitles::default();
    let Some(formula) = name_formula(book, sheet, PRINT_TITLES) else { return titles };
    for reference in split_references(formula) {
        let Some(range) = parse_reference(&reference) else { continue };
        if range.is_whole_rows() {
            titles.rows = Some((range.first.row, range.last.row));
        } else if range.is_whole_columns() {
            titles.cols = Some((range.first.col, range.last.col));
        }
    }
    titles
}

/// Set the print area; an empty list clears it.
pub fn set_print_area(book: &mut Workbook, sheet: usize, areas: &[Range]) {
    let Some(name) = book.sheets.get(sheet).map(|s| s.name.clone()) else { return };
    let text = areas
        .iter()
        .map(|r| format!("{}!{}", quoted_sheet(&name), absolute(*r)))
        .collect::<Vec<_>>()
        .join(",");
    set_scoped_name(book, sheet, PRINT_AREA, (!areas.is_empty()).then_some(text));
}

/// Set the print titles; passing neither clears them. Excel writes the columns first.
pub fn set_print_titles(book: &mut Workbook, sheet: usize, titles: PrintTitles) {
    let Some(name) = book.sheets.get(sheet).map(|s| s.name.clone()) else { return };
    let quoted = quoted_sheet(&name);
    let mut parts = Vec::new();
    if let Some((first, last)) = titles.cols {
        parts.push(format!("{quoted}!${}:${}", column_name(first), column_name(last)));
    }
    if let Some((first, last)) = titles.rows {
        parts.push(format!("{quoted}!${}:${}", first + 1, last + 1));
    }
    set_scoped_name(book, sheet, PRINT_TITLES, (!parts.is_empty()).then(|| parts.join(",")));
}

fn name_formula<'a>(book: &'a Workbook, sheet: usize, name: &str) -> Option<&'a str> {
    book.names
        .iter()
        .find(|n| n.scope == Some(sheet) && n.name.eq_ignore_ascii_case(name))
        .map(|n| n.formula.as_str())
}

/// Put a sheet scoped name in place, or take it away when `formula` is `None`.
fn set_scoped_name(book: &mut Workbook, sheet: usize, name: &str, formula: Option<String>) {
    book.names.retain(|n| !(n.scope == Some(sheet) && n.name.eq_ignore_ascii_case(name)));
    if let Some(formula) = formula {
        book.names.push(DefinedName {
            name: name.to_owned(),
            scope: Some(sheet),
            formula,
            hidden: false,
            attrs: Vec::new(),
        });
    }
}

/// A sheet name as a reference writes it: quoted when it holds anything but letters, digits and
/// underscores or starts with a digit.
fn quoted_sheet(name: &str) -> String {
    crate::formula::print::sheet_name(name)
}

/// `$A$1:$D$50`, or `$A$1` for one cell, `$A:$C` for whole columns and `$1:$2` for whole rows.
fn absolute(range: Range) -> String {
    if range.first.row == 0 && range.last.row == MAX_ROWS - 1 {
        return format!("${}:${}", column_name(range.first.col), column_name(range.last.col));
    }
    if range.first.col == 0 && range.last.col == MAX_COLS - 1 {
        return format!("${}:${}", range.first.row + 1, range.last.row + 1);
    }
    let corner = |row: u32, col: u32| format!("${}${}", column_name(col), row + 1);
    if range.first == range.last {
        return corner(range.first.row, range.first.col);
    }
    format!(
        "{}:{}",
        corner(range.first.row, range.first.col),
        corner(range.last.row, range.last.col)
    )
}

/// Split `Sheet1!$A$1:$B$2,'My Sheet'!$D:$D` at the commas that are not inside quotes.
fn split_references(formula: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in formula.trim().trim_matches(|c| c == '(' || c == ')').chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                current.push(c);
            }
            ',' if !quoted => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts.into_iter().map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()).collect()
}

/// The range after the sheet name in `Sheet1!$A$1:$B$2`.
fn parse_reference(reference: &str) -> Option<Range> {
    let at = reference.rfind('!')?;
    Range::parse(&reference[at + 1..])
}

#[cfg(test)]
mod edit_tests;
#[cfg(test)]
mod paginate_tests;
#[cfg(test)]
mod tests;
