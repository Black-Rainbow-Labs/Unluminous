//! Where the pages of a sheet break, the way Excel breaks them.
//!
//! [`paginate`] takes a sheet with its [`PageSetup`], print area and print titles and returns a [`PageLayout`]:
//! the cell ranges on each page in the chosen order, the scale used, the breaks, the header and footer of every
//! page with their codes expanded, and the paper and margins in points. The window paints a page from it and a
//! PDF writer writes the same page.
//!
//! **Excel lays a printout out in printer pixels.** It was measured against Excel 16 printing to Microsoft
//! Print to PDF at 600 dpi (`tools/excel-oracle/page-breaks.ps1`, answers in `tests/fixtures/pages`), and
//! [`super::metrics`] says how a row and a column are sized there. The rules for a page are:
//!
//! - A page holds what fits in the paper less the margins, less six pixels: an element fits when it ends before
//!   that edge. Each element counts at the scale in use rounded to a whole pixel.
//! - Rows and columns are filled in order; a manual break, or an element that does not fit, starts the next page.
//! - Title rows and columns are printed again at the start of every page after the one they first appear on, and
//!   take room from it.
//! - Excel behaves as if the sheet had one more row and column after the last, so a page that ends exactly full
//!   reports a break after the data. That page is not printed.
//! - A page with nothing on it is not printed.
//! - Fitting to pages takes the smaller of the largest scale from 100 down at which the sheet needs no more
//!   pages than asked, and an estimate (see `estimate`); manual breaks are ignored while it is on, and Excel lists
//!   only the manual breaks of an axis that has any. For Calibri 11 this gives Excel's scale in every combination
//!   measured. In the other default fonts five fits differ by a percent or two (`tests/pages.rs`).

use std::collections::BTreeSet;

use super::headerfooter::{self, Context, Sections};
use super::metrics::{column_pixels, row_pixels};
use super::paper;
use super::{
    print_area, print_titles, HeaderFooterPart, Orientation, PageOrder, PageSetup, PrintComments,
    PrintTitles,
};
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::model::{Sheet, Workbook};

pub use super::layout::{
    printed_error, AreaLayout, Break, MarginPoints, NoteLine, Options, Page, PageLayout, Size,
};
pub use super::metrics::Metrics;

/// The resolution Excel lays a page out at for a PDF printer.
pub const PRINTER_DPI: f64 = 600.0;
/// Printer pixels in a point.
const PIXELS_PER_POINT: f64 = PRINTER_DPI / 72.0;
/// What the estimate of a fitted scale takes off a page, in printer pixels.
const FIT_ALLOWANCE: f64 = 12.0;
/// How much of an average element each page after the first costs the estimate of a fitted scale.
const FIT_BOUNDARY: f64 = 0.875;
/// What an element must leave free at the far edge of a page: one screen pixel, which is six printer pixels.
const EDGE: f64 = 6.0;

// ----------------------------------------------------------------------------------------------------
// One axis of an area, cut into pages.

/// A row or column axis of one print area, in printer pixels at full scale, with the one element past the end
/// that Excel lays out as well.
#[derive(Debug, Clone)]
struct Axis {
    first: u32,
    sizes: Vec<f64>,
}

impl Axis {
    /// The index just past the last element, which is the extra one.
    fn end(&self) -> u32 {
        self.first + self.sizes.len() as u32
    }

    /// The size at an absolute index.
    fn size(&self, index: u32) -> f64 {
        self.sizes.get((index - self.first) as usize).copied().unwrap_or(0.0)
    }
}

/// What repeats at the start of the pages after the one it first appears on.
#[derive(Debug, Clone, Copy)]
struct Repeat {
    first: u32,
    last: u32,
    /// Printer pixels at full scale.
    size: f64,
}

/// An element at a scale, in whole pixels.
fn scaled(size: f64, scale: u32) -> f64 {
    (size * f64::from(scale) / 100.0).round()
}

/// Cut an axis where its pages fill and where a manual break says. Each cut is the first element of a page, and
/// the first is always the axis' first element. The last cut may be the extra element past the end.
fn cut_axis(
    axis: &Axis,
    room: f64,
    scale: u32,
    manual: &BTreeSet<u32>,
    repeat: Option<Repeat>,
) -> Vec<Break> {
    let limit = room - EDGE;
    let mut cuts = vec![Break { at: axis.first, manual: false }];
    let mut start = axis.first;
    loop {
        let mut used = repeat.filter(|r| start > r.last).map_or(0.0, |r| scaled(r.size, scale));
        let mut index = start;
        let mut forced = false;
        while index < axis.end() {
            if index > start && manual.contains(&index) {
                forced = true;
                break;
            }
            let size = scaled(axis.size(index), scale);
            if index > start && used + size >= limit {
                break;
            }
            used += size;
            index += 1;
        }
        if index >= axis.end() {
            return cuts;
        }
        cuts.push(Break { at: index, manual: forced });
        start = index;
    }
}

/// What Excel's estimate of the scale for `pages` pages along an axis comes to. It is
/// `round(100 * pages * room / (length + 0.875 * (pages - 1) * average))`: the length of the axis at full scale,
/// and for every page after the first most of one average element, against the room in `pages` pages less
/// twelve pixels. Measured over a hundred and fifty combinations of columns and pages, it and the largest scale
/// that fits agree with Excel's in all of them when the smaller of the two is taken. `None` when the axis is not
/// fitted or has nothing on it.
fn estimate(axis: &Axis, room: f64, pages: usize) -> Option<u32> {
    let real = &axis.sizes[..axis.sizes.len() - 1];
    let total: f64 = real.iter().sum();
    let shown = real.iter().filter(|s| **s > 0.0).count();
    if pages == 0 || total <= 0.0 || shown == 0 {
        return None;
    }
    let average = total / shown as f64;
    let needed = total + FIT_BOUNDARY * (pages as f64 - 1.0) * average;
    Some((100.0 * pages as f64 * (room - FIT_ALLOWANCE) / needed).round().clamp(10.0, 100.0) as u32)
}

/// How many pages hold something of the area: the cuts that start before the first element past the end.
fn pages_needed(cuts: &[Break], real_end: u32) -> usize {
    cuts.iter().filter(|c| c.at < real_end).count()
}

// ----------------------------------------------------------------------------------------------------
// Paper.

/// The paper in points, and the room inside its margins in printer pixels.
#[derive(Debug, Clone, Copy)]
struct PaperRoom {
    points: Size,
    width: f64,
    height: f64,
}

fn paper_points(setup: &PageSetup) -> Size {
    let (w, h) = custom_size(setup).unwrap_or_else(|| paper::size_of(setup.paper_size));
    match setup.orientation_or_default() {
        Orientation::Portrait => Size { width: w, height: h },
        Orientation::Landscape => Size { width: h, height: w },
    }
}

/// A paper size the file states in `paperWidth` and `paperHeight`, such as `210mm`.
fn custom_size(setup: &PageSetup) -> Option<(f64, f64)> {
    let width = length_points(setup.paper_width.as_deref()?)?;
    let height = length_points(setup.paper_height.as_deref()?)?;
    Some((width, height))
}

fn length_points(text: &str) -> Option<f64> {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let value: f64 = text[..split].parse().ok()?;
    match &text[split..] {
        "mm" => Some(value * 72.0 / 25.4),
        "cm" => Some(value * 72.0 / 2.54),
        "in" => Some(value * 72.0),
        "pt" => Some(value),
        _ => None,
    }
}

fn paper_room(setup: &PageSetup) -> PaperRoom {
    let points = paper_points(setup);
    let m = &setup.margins;
    let pixels = |inches: f64| (inches * PRINTER_DPI).round();
    PaperRoom {
        points,
        width: (points.width * PIXELS_PER_POINT).round() - pixels(m.left) - pixels(m.right),
        height: (points.height * PIXELS_PER_POINT).round() - pixels(m.top) - pixels(m.bottom),
    }
}

// ----------------------------------------------------------------------------------------------------
// What prints.

/// The last row and column that print when no print area says otherwise: the last one holding a value or a
/// format, a merge, a picture or a chart. Counted from zero; `None` for a sheet with nothing on it.
fn printed_extent(sheet: &Sheet) -> Option<(u32, u32)> {
    let mut last: Option<(u32, u32)> = None;
    let mut take = |row: u32, col: u32| {
        last = Some(last.map_or((row, col), |(r, c)| (r.max(row), c.max(col))));
    };
    for (&(row, col), cell) in &sheet.cells {
        if !cell.value.is_empty() || cell.formula.is_some() || cell.style != 0 {
            take(row, col);
        }
    }
    for merge in &sheet.merges {
        take(merge.last.row, merge.last.col);
    }
    for chart in &sheet.charts {
        take(chart.to.0.row, chart.to.0.col);
    }
    for picture in &sheet.pictures() {
        take(picture.to.0.row, picture.to.0.col);
    }
    last
}

/// The ranges that print: the print area, clipped to the sheet's extent where it names whole rows or columns, or
/// the extent from A1.
fn areas_to_print(book: &Workbook, index: usize, sheet: &Sheet) -> Vec<Range> {
    let extent = printed_extent(sheet);
    let areas = print_area(book, index);
    if areas.is_empty() {
        return match extent {
            Some((row, col)) => vec![Range::new(CellRef::new(0, 0), CellRef::new(row, col))],
            None => Vec::new(),
        };
    }
    let (last_row, last_col) = extent.unwrap_or((0, 0));
    areas
        .into_iter()
        .map(|a| {
            let row = if a.last.row == MAX_ROWS - 1 {
                a.last.row.min(last_row.max(a.first.row))
            } else {
                a.last.row
            };
            let col = if a.last.col == MAX_COLS - 1 {
                a.last.col.min(last_col.max(a.first.col))
            } else {
                a.last.col
            };
            Range::new(a.first, CellRef::new(row, col))
        })
        .collect()
}

/// Whether anything is printed in a range: Excel does not print a page without it.
fn page_has_content(sheet: &Sheet, body: &Range) -> bool {
    let in_range = |col: u32| col >= body.first.col && col <= body.last.col;
    let has_cell = sheet.cells.range((body.first.row, 0)..=(body.last.row, u32::MAX)).any(
        |(&(_, col), cell)| {
            in_range(col) && (!cell.value.is_empty() || cell.formula.is_some() || cell.style != 0)
        },
    );
    has_cell
        || sheet.merges.iter().any(|m| m.intersects(body))
        || sheet.charts.iter().any(|c| Range::new(c.from.0, c.to.0).intersects(body))
        || sheet.pictures().iter().any(|p| Range::new(p.from.0, p.to.0).intersects(body))
}

// ----------------------------------------------------------------------------------------------------
// The job: everything the steps of laying a sheet out share.

struct Job<'a> {
    index: usize,
    sheet: &'a Sheet,
    setup: &'a PageSetup,
    options: &'a Options,
    metrics: Metrics,
    paper: PaperRoom,
    /// Room for cells on a page in printer pixels: the paper inside the margins, less the headings.
    room: (f64, f64),
    areas: Vec<Range>,
    axes: Vec<(Axis, Axis)>,
    repeat: (Option<Repeat>, Option<Repeat>),
}

impl<'a> Job<'a> {
    fn new(book: &'a Workbook, index: usize, options: &'a Options) -> Job<'a> {
        let sheet = &book.sheets[index];
        let setup = &sheet.page;
        let metrics = options.metrics.unwrap_or_else(|| Metrics::for_workbook(book));
        let paper = paper_room(setup);
        let areas = areas_to_print(book, index, sheet);
        let last_row = areas.iter().map(|a| a.last.row).max().unwrap_or(0);
        let (heading_width, heading_height) = headings_space(setup, &metrics, last_row);
        let axes = areas.iter().map(|a| build_axes(sheet, *a, &metrics)).collect();
        let titles = print_titles(book, index);
        Job {
            index,
            sheet,
            setup,
            options,
            metrics,
            paper,
            room: (paper.width - heading_width, paper.height - heading_height),
            areas,
            axes,
            repeat: repeats(titles, sheet, &metrics),
        }
    }

    /// The scale to print at: the percentage in the setup, or the one fitting to pages gives.
    fn scale(&self) -> u32 {
        if self.setup.fit_to_page {
            self.fit_scale()
        } else {
            self.setup.scale.clamp(10, 400)
        }
    }

    /// The scale that fitting to pages gives, which never enlarges. Excel takes the smaller of two scales: the
    /// largest from 100 down at which the pages of the sheet can be laid out in the pages asked for, and an
    /// estimate that sets the sheet's length against that many pages, each but the first costing most of a
    /// column or row (see [`estimate`]).
    fn fit_scale(&self) -> u32 {
        let (wide, tall) = (self.setup.fit_to_width as usize, self.setup.fit_to_height as usize);
        let fits = |scale: u32| {
            self.areas.iter().zip(&self.axes).all(|(area, (rows, cols))| {
                let row_cuts = cut_axis(rows, self.room.1, scale, &BTreeSet::new(), self.repeat.0);
                let col_cuts = cut_axis(cols, self.room.0, scale, &BTreeSet::new(), self.repeat.1);
                (wide == 0 || pages_needed(&col_cuts, area.last.col + 1) <= wide)
                    && (tall == 0 || pages_needed(&row_cuts, area.last.row + 1) <= tall)
            })
        };
        let largest = (10..=100).rev().find(|&scale| fits(scale)).unwrap_or(10);
        let estimates = self.axes.iter().flat_map(|(rows, cols)| {
            [estimate(cols, self.room.0, wide), estimate(rows, self.room.1, tall)]
        });
        estimates.flatten().fold(largest, u32::min).max(10)
    }

    /// The cuts of one area on both axes. Manual breaks count unless the sheet is fitted to pages.
    fn cuts(&self, area: usize, scale: u32) -> (Vec<Break>, Vec<Break>) {
        let (rows, cols) = &self.axes[area];
        let (row_manual, col_manual) = if self.setup.fit_to_page {
            (BTreeSet::new(), BTreeSet::new())
        } else {
            (
                manual_cuts(&self.setup.row_break_rows(), rows),
                manual_cuts(&self.setup.col_break_cols(), cols),
            )
        };
        (
            cut_axis(rows, self.room.1, scale, &row_manual, self.repeat.0),
            cut_axis(cols, self.room.0, scale, &col_manual, self.repeat.1),
        )
    }

    /// The breaks of an axis as Excel lists them. A sheet fitted to pages is laid out without its manual breaks, but
    /// Excel lists only the manual breaks of an axis that has any, and none of the automatic ones.
    fn listed(&self, cuts: Vec<Break>, manual: &[u32], axis: &Axis) -> Vec<Break> {
        let kept = manual_cuts(manual, axis);
        if self.setup.fit_to_page && !kept.is_empty() {
            return kept.into_iter().map(|at| Break { at, manual: true }).collect();
        }
        cuts.into_iter().skip(1).collect()
    }

    /// Lay the sheet out.
    fn lay_out(&self) -> PageLayout {
        let scale = self.scale();
        let mut layout = self.blank(scale);
        let mut spans: Vec<(usize, Range)> = Vec::new();
        for (i, area) in self.areas.iter().enumerate() {
            let (row_cuts, col_cuts) = self.cuts(i, scale);
            spans.extend(page_ranges(i, *area, &row_cuts, &col_cuts, self.setup.page_order));
            let (rows, cols) = &self.axes[i];
            layout.areas.push(AreaLayout {
                range: *area,
                row_breaks: self.listed(row_cuts, &self.setup.row_break_rows(), rows),
                col_breaks: self.listed(col_cuts, &self.setup.col_break_cols(), cols),
            });
        }
        spans.retain(|(_, body)| page_has_content(self.sheet, body));
        let total = spans.len() as u32;
        layout.pages = spans
            .into_iter()
            .enumerate()
            .map(|(index, (area, body))| self.page(&layout, index, area, body, total))
            .collect();
        self.measure(&mut layout);
        self.add_notes(&mut layout);
        layout
    }

    fn blank(&self, scale: u32) -> PageLayout {
        let m = &self.setup.margins;
        let o = &self.setup.options;
        PageLayout {
            sheet: self.index,
            paper: self.paper.points,
            margins: MarginPoints {
                left: m.left * 72.0,
                right: m.right * 72.0,
                top: m.top * 72.0,
                bottom: m.bottom * 72.0,
                header: m.header * 72.0,
                footer: m.footer * 72.0,
            },
            scale,
            areas: Vec::new(),
            pages: Vec::new(),
            gridlines: o.grid_lines,
            headings: o.headings,
            black_and_white: self.setup.black_and_white,
            draft: self.setup.draft,
            errors: self.setup.errors,
            comments: self.setup.comments,
            note_pages: Vec::new(),
            column_points: Vec::new(),
            row_points: Vec::new(),
        }
    }

    /// One page: its cells, the titles it repeats, and its header and footer.
    fn page(
        &self,
        layout: &PageLayout,
        index: usize,
        area: usize,
        body: Range,
        total: u32,
    ) -> Page {
        let title_rows =
            self.repeat.0.filter(|r| body.first.row > r.last).map(|r| (r.first, r.last));
        let title_cols =
            self.repeat.1.filter(|r| body.first.col > r.last).map(|r| (r.first, r.last));
        let number = first_number(self.setup) + index as i32;
        let (header, footer) = self.header_and_footer(index, number, total);
        Page {
            index,
            number,
            area,
            body,
            title_rows,
            title_cols,
            content: Size { width: 0.0, height: 0.0 },
            origin: (layout.margins.left, layout.margins.top),
            header,
            footer,
        }
    }

    /// The header and footer of a page, with the codes expanded: the first page's when the setup asks for one,
    /// the even pages' when it asks for those, and the odd pages' otherwise.
    fn header_and_footer(&self, index: usize, number: i32, total: u32) -> (Sections, Sections) {
        let hf = &self.setup.header_footer;
        let (header, footer) = if hf.different_first && index == 0 {
            (HeaderFooterPart::FirstHeader, HeaderFooterPart::FirstFooter)
        } else if hf.different_odd_even && number % 2 == 0 {
            (HeaderFooterPart::EvenHeader, HeaderFooterPart::EvenFooter)
        } else {
            (HeaderFooterPart::OddHeader, HeaderFooterPart::OddFooter)
        };
        let context = Context {
            page: number,
            pages: total,
            date: self.options.date.clone(),
            time: self.options.time.clone(),
            file: self.options.file.clone(),
            path: self.options.path.clone(),
            sheet: self.sheet.name.clone(),
        };
        (
            headerfooter::expand(hf.text(header), &context),
            headerfooter::expand(hf.text(footer), &context),
        )
    }

    /// The printed size of every row and column, and of every page's content, in points at the scale in use.
    fn measure(&self, layout: &mut PageLayout) {
        let last_row = self.areas.iter().map(|a| a.last.row).max().unwrap_or(0);
        let last_col = self.areas.iter().map(|a| a.last.col).max().unwrap_or(0);
        let points = |pixels: f64| scaled(pixels, layout.scale) / PIXELS_PER_POINT;
        layout.column_points =
            (0..=last_col).map(|c| points(column_pixels(self.sheet, c, &self.metrics))).collect();
        layout.row_points =
            (0..=last_row).map(|r| points(row_pixels(self.sheet, r, &self.metrics))).collect();
        let room = Size {
            width: layout.paper.width - layout.margins.left - layout.margins.right,
            height: layout.paper.height - layout.margins.top - layout.margins.bottom,
        };
        let centre = (self.setup.options.horizontal_centered, self.setup.options.vertical_centered);
        for page in &mut layout.pages {
            let width = extent(
                &layout.column_points,
                page.body.first.col,
                page.body.last.col,
                page.title_cols,
            );
            let height = extent(
                &layout.row_points,
                page.body.first.row,
                page.body.last.row,
                page.title_rows,
            );
            page.content = Size { width, height };
            let x = if centre.0 { ((room.width - width) / 2.0).max(0.0) } else { 0.0 };
            let y = if centre.1 { ((room.height - height) / 2.0).max(0.0) } else { 0.0 };
            page.origin = (layout.margins.left + x, layout.margins.top + y);
        }
    }

    /// Notes printed at the end, a page's worth of lines at a time.
    fn add_notes(&self, layout: &mut PageLayout) {
        if self.setup.comments != PrintComments::AtEnd || self.sheet.notes.is_empty() {
            return;
        }
        let lines: Vec<NoteLine> = self
            .sheet
            .notes
            .iter()
            .map(|(&(row, col), note)| NoteLine {
                cell: CellRef::new(row, col),
                author: note.author.clone(),
                text: note.text.clone(),
            })
            .collect();
        let per_page = ((self.room.1 / self.metrics.default_row).floor() as usize).max(1);
        layout.note_pages = lines.chunks(per_page).map(<[NoteLine]>::to_vec).collect();
    }
}

/// Space the row and column headings take on every page, in printer pixels.
fn headings_space(setup: &PageSetup, metrics: &Metrics, last_row: u32) -> (f64, f64) {
    if !setup.options.headings {
        return (0.0, 0.0);
    }
    let digits = f64::from(last_row + 1).log10().floor().max(1.0) + 1.0;
    let width = ((digits * metrics.screen_digit + 9.0) * metrics.column_ratio()).round();
    (width, metrics.default_row)
}

fn build_axes(sheet: &Sheet, area: Range, metrics: &Metrics) -> (Axis, Axis) {
    let rows =
        (area.first.row..=area.last.row + 1).map(|r| row_pixels(sheet, r, metrics)).collect();
    let cols =
        (area.first.col..=area.last.col + 1).map(|c| column_pixels(sheet, c, metrics)).collect();
    (Axis { first: area.first.row, sizes: rows }, Axis { first: area.first.col, sizes: cols })
}

/// The repeated rows and columns as the cuts need them.
fn repeats(
    titles: PrintTitles,
    sheet: &Sheet,
    metrics: &Metrics,
) -> (Option<Repeat>, Option<Repeat>) {
    let rows = titles.rows.map(|(first, last)| Repeat {
        first,
        last,
        size: (first..=last).map(|r| row_pixels(sheet, r, metrics)).sum(),
    });
    let cols = titles.cols.map(|(first, last)| Repeat {
        first,
        last,
        size: (first..=last).map(|c| column_pixels(sheet, c, metrics)).sum(),
    });
    (rows, cols)
}

/// The manual breaks inside an area, as the first element of the page they start.
fn manual_cuts(breaks: &[u32], axis: &Axis) -> BTreeSet<u32> {
    breaks.iter().copied().filter(|&b| b > axis.first && b < axis.end()).collect()
}

fn first_number(setup: &PageSetup) -> i32 {
    if setup.use_first_page_number {
        setup.first_page_number
    } else {
        1
    }
}

/// The ranges of an area's pages, in the order they print. Pages past the last element are left out.
fn page_ranges(
    area: usize,
    range: Range,
    rows: &[Break],
    cols: &[Break],
    order: PageOrder,
) -> Vec<(usize, Range)> {
    let row_spans = spans_of(rows, range.last.row);
    let col_spans = spans_of(cols, range.last.col);
    let make = |r: (u32, u32), c: (u32, u32)| {
        (area, Range::new(CellRef::new(r.0, c.0), CellRef::new(r.1, c.1)))
    };
    match order {
        PageOrder::DownThenOver => {
            col_spans.iter().flat_map(|&c| row_spans.iter().map(move |&r| make(r, c))).collect()
        }
        PageOrder::OverThenDown => {
            row_spans.iter().flat_map(|&r| col_spans.iter().map(move |&c| make(r, c))).collect()
        }
    }
}

/// The first and last element of each page of an axis that holds any of the real extent.
fn spans_of(cuts: &[Break], last: u32) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    for (i, cut) in cuts.iter().enumerate() {
        if cut.at > last {
            break;
        }
        let end = cuts.get(i + 1).map_or(last, |next| (next.at - 1).min(last));
        spans.push((cut.at, end));
    }
    spans
}

/// The length of a run of elements and of the titles repeated before it.
fn extent(sizes: &[f64], first: u32, last: u32, titles: Option<(u32, u32)>) -> f64 {
    let sum = |a: u32, b: u32| {
        (a..=b).map(|i| sizes.get(i as usize).copied().unwrap_or(0.0)).sum::<f64>()
    };
    sum(first, last) + titles.map_or(0.0, |(a, b)| sum(a, b))
}

/// Lay a sheet out in pages.
pub fn paginate(book: &Workbook, sheet: usize, options: &Options) -> PageLayout {
    Job::new(book, sheet, options).lay_out()
}
