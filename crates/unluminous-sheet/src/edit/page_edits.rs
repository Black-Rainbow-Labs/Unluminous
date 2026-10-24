//! Changes to how a sheet prints: orientation, paper, margins, scale, print area and titles, headers and
//! footers, and manual page breaks. Each one is a single undo step.
//!
//! The window's Page Setup dialog, its Page Layout tab and the `page-setup` verbs of `plugins run
//! spreadsheet` all call these functions, so a change made with the pointer and the same change asked for
//! by an agent are one change.

use super::Session;
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::Axis;
use crate::pages::{
    self, HeaderFooterPart, Margins, Orientation, PageBreak, PageOrder, PageSetup, PrintComments,
    PrintErrors, PrintTitles,
};

/// Excel's range for the scale of a printout, in percent.
const SCALE_RANGE: std::ops::RangeInclusive<u32> = 10..=400;
/// The most pages wide or tall a fit can ask for.
const MOST_FIT_PAGES: u32 = 32_767;
/// The range Excel allows for the number of the first page.
const FIRST_PAGE_RANGE: std::ops::RangeInclusive<i32> = -32_767..=32_767;
/// The largest margin Excel accepts, in inches.
const MOST_MARGIN: f64 = 49.0;

impl Session {
    /// Make one change to a sheet's page setup as a single undo step.
    fn change_page(
        &mut self,
        sheet: usize,
        label: &str,
        change: impl FnOnce(&mut PageSetup),
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(label);
        change(&mut self.book.sheets[sheet].page);
        Ok(())
    }

    /// Replace a sheet's whole page setup, as the Page Setup dialog does when it is accepted.
    pub fn set_page_setup(&mut self, sheet: usize, setup: PageSetup) -> Result<(), String> {
        validate_margins(&setup.margins)?;
        self.change_page(sheet, "Page setup", |page| *page = setup)
    }

    /// Portrait or landscape.
    pub fn set_orientation(
        &mut self,
        sheet: usize,
        orientation: Orientation,
    ) -> Result<(), String> {
        self.change_page(sheet, "Page orientation", |page| page.orientation = Some(orientation))
    }

    /// The paper, by the number Excel gives it: 1 Letter, 9 A4 and so on.
    pub fn set_paper_size(&mut self, sheet: usize, code: u32) -> Result<(), String> {
        if pages::paper::paper(code).is_none() {
            return Err(format!("there is no paper size {code}"));
        }
        self.change_page(sheet, "Paper size", |page| {
            page.paper_size = code;
            page.paper_width = None;
            page.paper_height = None;
        })
    }

    /// The six margins, in inches.
    pub fn set_margins(&mut self, sheet: usize, margins: Margins) -> Result<(), String> {
        validate_margins(&margins)?;
        self.change_page(sheet, "Margins", |page| page.margins = margins)
    }

    /// Print at a percentage of full size, which turns fitting to pages off.
    pub fn set_scale(&mut self, sheet: usize, percent: u32) -> Result<(), String> {
        if !SCALE_RANGE.contains(&percent) {
            return Err(format!(
                "a scale is from {} to {} percent",
                SCALE_RANGE.start(),
                SCALE_RANGE.end()
            ));
        }
        self.change_page(sheet, "Scale", |page| {
            page.scale = percent;
            page.fit_to_page = false;
        })
    }

    /// Fit the printout to a number of pages wide and tall; zero means as many as it takes.
    pub fn set_fit_to_pages(&mut self, sheet: usize, wide: u32, tall: u32) -> Result<(), String> {
        if wide > MOST_FIT_PAGES || tall > MOST_FIT_PAGES {
            return Err(format!("a fit is at most {MOST_FIT_PAGES} pages wide or tall"));
        }
        self.change_page(sheet, "Fit to pages", |page| {
            page.fit_to_width = wide;
            page.fit_to_height = tall;
            page.fit_to_page = true;
        })
    }

    /// Print only these ranges, each on pages of its own. An empty list prints the whole sheet.
    pub fn set_print_area(&mut self, sheet: usize, areas: &[Range]) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if areas.is_empty() { "Clear print area" } else { "Set print area" });
        pages::set_print_area(&mut self.book, sheet, areas);
        Ok(())
    }

    /// Print the whole sheet again.
    pub fn clear_print_area(&mut self, sheet: usize) -> Result<(), String> {
        self.set_print_area(sheet, &[])
    }

    /// Add a range to the print area; it prints on pages of its own.
    pub fn add_to_print_area(&mut self, sheet: usize, area: Range) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let mut areas = pages::print_area(&self.book, sheet);
        areas.push(area);
        self.remember_whole("Add to print area");
        pages::set_print_area(&mut self.book, sheet, &areas);
        Ok(())
    }

    /// Repeat rows at the top and columns at the left of every page; `None` for both clears them.
    pub fn set_print_titles(
        &mut self,
        sheet: usize,
        rows: Option<(u32, u32)>,
        cols: Option<(u32, u32)>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let bad_rows = rows.is_some_and(|(a, b)| a > b || b >= MAX_ROWS);
        let bad_cols = cols.is_some_and(|(a, b)| a > b || b >= MAX_COLS);
        if bad_rows || bad_cols {
            return Err(
                "print titles need a first row or column that comes before the last".to_owned()
            );
        }
        self.remember_whole("Print titles");
        pages::set_print_titles(&mut self.book, sheet, PrintTitles { rows, cols });
        Ok(())
    }

    /// Print the gridlines.
    pub fn set_print_gridlines(&mut self, sheet: usize, on: bool) -> Result<(), String> {
        self.change_page(sheet, "Print gridlines", |page| page.options.grid_lines = on)
    }

    /// Print the row and column headings.
    pub fn set_print_headings(&mut self, sheet: usize, on: bool) -> Result<(), String> {
        self.change_page(sheet, "Print headings", |page| page.options.headings = on)
    }

    /// Centre the printout between the margins, across and down.
    pub fn set_centered(
        &mut self,
        sheet: usize,
        horizontal: bool,
        vertical: bool,
    ) -> Result<(), String> {
        self.change_page(sheet, "Center on page", |page| {
            page.options.horizontal_centered = horizontal;
            page.options.vertical_centered = vertical;
        })
    }

    /// The order pages are numbered and printed in.
    pub fn set_page_order(&mut self, sheet: usize, order: PageOrder) -> Result<(), String> {
        self.change_page(sheet, "Page order", |page| page.page_order = order)
    }

    /// The number the first page carries, or `None` to number from 1 automatically.
    pub fn set_first_page_number(
        &mut self,
        sheet: usize,
        number: Option<i32>,
    ) -> Result<(), String> {
        if number.is_some_and(|n| !FIRST_PAGE_RANGE.contains(&n)) {
            return Err("the first page number is from -32767 to 32767".to_owned());
        }
        self.change_page(sheet, "First page number", |page| match number {
            Some(n) => {
                page.first_page_number = n;
                page.use_first_page_number = true;
            }
            None => {
                page.first_page_number = 1;
                page.use_first_page_number = false;
            }
        })
    }

    /// How notes are printed.
    pub fn set_print_comments(&mut self, sheet: usize, how: PrintComments) -> Result<(), String> {
        self.change_page(sheet, "Print comments", |page| page.comments = how)
    }

    /// How cells showing an error are printed.
    pub fn set_print_errors(&mut self, sheet: usize, how: PrintErrors) -> Result<(), String> {
        self.change_page(sheet, "Print errors", |page| page.errors = how)
    }

    /// Print in black and white, and print in draft quality.
    pub fn set_print_quality(
        &mut self,
        sheet: usize,
        black_and_white: bool,
        draft: bool,
    ) -> Result<(), String> {
        self.change_page(sheet, "Print quality", |page| {
            page.black_and_white = black_and_white;
            page.draft = draft;
        })
    }

    /// The text of one header or footer, codes and all.
    pub fn set_header_footer(
        &mut self,
        sheet: usize,
        part: HeaderFooterPart,
        text: &str,
    ) -> Result<(), String> {
        if text.chars().count() > 255 {
            return Err("a header or footer holds at most 255 characters".to_owned());
        }
        self.change_page(sheet, "Header and footer", |page| page.header_footer.set_text(part, text))
    }

    /// The four switches of the header and footer: a different first page, different odd and even pages,
    /// scaling with the document, and aligning with the margins.
    pub fn set_header_footer_options(
        &mut self,
        sheet: usize,
        different_first: bool,
        different_odd_even: bool,
        scale_with_doc: bool,
        align_with_margins: bool,
    ) -> Result<(), String> {
        self.change_page(sheet, "Header and footer options", |page| {
            let h = &mut page.header_footer;
            h.different_first = different_first;
            h.different_odd_even = different_odd_even;
            h.scale_with_doc = scale_with_doc;
            h.align_with_margins = align_with_margins;
        })
    }

    /// Break the page above `row` (zero based), as Insert Page Break does with a cell in column A.
    pub fn insert_row_break(&mut self, sheet: usize, row: u32) -> Result<(), String> {
        if row == 0 || row >= MAX_ROWS {
            return Err("a page break goes between two rows".to_owned());
        }
        self.change_page(sheet, "Insert page break", |page| {
            add_break(&mut page.row_breaks, row, MAX_COLS - 1)
        })
    }

    /// Break the page to the left of `col` (zero based).
    pub fn insert_col_break(&mut self, sheet: usize, col: u32) -> Result<(), String> {
        if col == 0 || col >= MAX_COLS {
            return Err("a page break goes between two columns".to_owned());
        }
        self.change_page(sheet, "Insert page break", |page| {
            add_break(&mut page.col_breaks, col, MAX_ROWS - 1)
        })
    }

    /// Insert Page Break with a cell chosen: a break above the cell's row and one left of its column. In row
    /// 1 or column A there is nothing to break, so only the other one is made.
    pub fn insert_page_break(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        if at.row == 0 && at.col == 0 {
            return Err("a page break goes between two rows or two columns".to_owned());
        }
        self.change_page(sheet, "Insert page break", |page| {
            if at.row > 0 {
                add_break(&mut page.row_breaks, at.row, MAX_COLS - 1);
            }
            if at.col > 0 {
                add_break(&mut page.col_breaks, at.col, MAX_ROWS - 1);
            }
        })
    }

    /// Remove the manual breaks above a cell's row and left of its column.
    pub fn remove_page_break(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let page = &self.book.sheets[sheet].page;
        let row_break = page.row_breaks.iter().any(|b| b.manual && b.id == at.row);
        let col_break = page.col_breaks.iter().any(|b| b.manual && b.id == at.col);
        if !row_break && !col_break {
            return Err(format!("there is no manual page break at {}", at.a1()));
        }
        self.change_page(sheet, "Remove page break", |page| {
            page.row_breaks.retain(|b| !(b.manual && b.id == at.row));
            page.col_breaks.retain(|b| !(b.manual && b.id == at.col));
        })
    }

    /// Take away every manual page break on the sheet.
    pub fn reset_page_breaks(&mut self, sheet: usize) -> Result<(), String> {
        self.change_page(sheet, "Reset all page breaks", |page| {
            page.row_breaks.retain(|b| !b.manual);
            page.col_breaks.retain(|b| !b.manual);
        })
    }
}

/// Move the page breaks the way the rows or columns around them moved: a break goes with the row (or column)
/// that starts its page, so inserting before that row pushes the break down with it, and deleting that row
/// takes the break away.
pub(super) fn shift_page_breaks(page: &mut PageSetup, axis: Axis, at: u32, count: i64) {
    let breaks = match axis {
        Axis::Rows => &mut page.row_breaks,
        Axis::Columns => &mut page.col_breaks,
    };
    let removed_to = (at as i64).saturating_sub(count);
    breaks.retain_mut(|b| {
        let id = b.id as i64;
        if count < 0 && id >= at as i64 && id < removed_to {
            return false;
        }
        if id >= at as i64 {
            b.id = (id + count).max(0) as u32;
        }
        true
    });
}

/// Put a manual break in its place in the list, unless one is already there.
fn add_break(breaks: &mut Vec<PageBreak>, id: u32, max: u32) {
    match breaks.iter_mut().find(|b| b.id == id) {
        Some(existing) => existing.manual = true,
        None => {
            breaks.push(PageBreak { id, min: 0, max, manual: true, pt: false });
            breaks.sort_by_key(|b| b.id);
        }
    }
}

/// Margins are numbers of inches from zero to Excel's largest.
fn validate_margins(m: &Margins) -> Result<(), String> {
    for (name, value) in [
        ("left", m.left),
        ("right", m.right),
        ("top", m.top),
        ("bottom", m.bottom),
        ("header", m.header),
        ("footer", m.footer),
    ] {
        if !value.is_finite() || !(0.0..=MOST_MARGIN).contains(&value) {
            return Err(format!("the {name} margin is from 0 to {MOST_MARGIN} inches"));
        }
    }
    Ok(())
}
