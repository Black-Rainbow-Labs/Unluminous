//! Printing a sheet (`task-2360`, the `wiringb` lane): the pages the paginator lays out, drawn as items a PDF
//! writer writes. The print preview, Export to PDF and Print all take their pages from [`layout_of`], so the
//! preview shows the pages the PDF has, and the PDF has exactly the paginator's pages.
//!
//! The items are the Word plugin's (`unluminous_word::layout::Item`), so the one PDF writer in the program
//! writes both. Positions are in points from the top left corner of the paper, as that writer wants them.

use std::collections::HashMap;

use ab_glyph::{Font, FontRef};
use unluminous_core::{CharStyle, Color};
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::pages::headerfooter::{Run, Sections};
use unluminous_sheet::pages::layout::{NoteLine, Options, Page, PageLayout};
use unluminous_sheet::{Sheet, Workbook};
use unluminous_word::layout::{Item, Page as PdfPage, Pages, Rect, RuleKind, TextItem};
use unluminous_word::props::Rgb;

use super::display::{self, Across, Down, Shown};
use super::Book;

/// The pages of the sheet showing, as Excel would print it.
pub fn layout_of(book: &Book) -> PageLayout {
    let when = crate::services::clock::date_time();
    let options = Options {
        file: book.name(),
        path: book.path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        date: format!("{}/{}/{}", when.month, when.day, when.year),
        time: clock_text(when.hour, when.minute),
        metrics: None,
    };
    unluminous_sheet::pages::paginate::paginate(&book.session.book, book.view.sheet, &options)
}

/// The desk between two sheets of paper in the page layout view, in screen points at a zoom of one.
pub const PAPER_GAP: f32 = 18.0;

/// A length on paper in points, as screen points at a zoom of one (a screen point is three quarters of a
/// paper point, as the row heights are).
pub fn paper_to_screen(points: f64) -> f32 {
    (points * 4.0 / 3.0) as f32
}

/// Make the sheet showing the page layout view's: leave the paper's margins and the desk between pages in
/// the axes, before the first row and column of each page, so every cell keeps its place on its own page
/// and selecting, typing and dragging work as they do in the normal view. Done once for each build of the
/// axes, and undone (the axes built again) when the view goes back to normal.
pub fn gap_the_axes(book: &mut Book) {
    use unluminous_sheet::model::ViewKind;
    let want = book.session.book.sheets[book.view.sheet].view.kind == ViewKind::PageLayout;
    let have = book.view.axes.as_ref().is_some_and(|a| a.paged);
    if want == have {
        return;
    }
    if !want {
        book.view.axes = None;
        let revision = book.revision;
        let sheet = &book.session.book.sheets[book.view.sheet];
        book.view.axes(sheet, revision);
        return;
    }
    let layout = layout_of(book);
    let m = layout.margins;
    let (left, right) = (paper_to_screen(m.left), paper_to_screen(m.right));
    let (top, bottom) = (paper_to_screen(m.top), paper_to_screen(m.bottom));
    let mut rows = std::collections::BTreeMap::new();
    let mut cols = std::collections::BTreeMap::new();
    for area in &layout.areas {
        rows.insert(area.range.first.row, PAPER_GAP + top);
        cols.insert(area.range.first.col, PAPER_GAP + left);
        for b in area.row_breaks.iter().filter(|b| b.at > area.range.first.row) {
            rows.insert(b.at, bottom + PAPER_GAP + top);
        }
        for b in area.col_breaks.iter().filter(|b| b.at > area.range.first.col) {
            cols.insert(b.at, right + PAPER_GAP + left);
        }
    }
    if let Some(axes) = book.view.axes.as_mut() {
        axes.rows = axes.rows.clone().with_leads(rows.into_iter().collect());
        axes.columns = axes.columns.clone().with_leads(cols.into_iter().collect());
        axes.paged = true;
    }
}

/// A time of day as Excel prints it: `3:07 PM`.
fn clock_text(hour: u8, minute: u8) -> String {
    let (h, suffix) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    format!("{h}:{minute:02} {suffix}")
}

/// Finds the font file a style prints with and measures text in it, remembering what it found.
pub struct Measurer {
    faces: HashMap<(String, bool, bool), Option<unluminous_word::pdf::FaceBytes>>,
}

impl Measurer {
    pub fn new() -> Self {
        Self { faces: HashMap::new() }
    }

    /// The left edge of each character of `text` set in `style`, starting at `x`.
    pub fn positions(&mut self, style: &CharStyle, text: &str, x: f32) -> Vec<f32> {
        let key = (style.family.to_string(), style.bold, style.italic);
        let face = self
            .faces
            .entry(key)
            .or_insert_with(|| crate::services::word::commands::pdf_face(style))
            .clone();
        let parsed = face
            .as_ref()
            .and_then(|(bytes, index)| FontRef::try_from_slice_and_index(bytes, *index).ok());
        let mut at = x;
        let mut out = Vec::new();
        for c in text.chars() {
            out.push(at);
            at += match &parsed {
                Some(font) => {
                    let units = font.units_per_em().unwrap_or(1000.0);
                    font.h_advance_unscaled(font.glyph_id(c)) / units * style.size
                }
                None => style.size * 0.5,
            };
        }
        out.push(at);
        out
    }

    /// The width of `text` set in `style`.
    pub fn width(&mut self, style: &CharStyle, text: &str) -> f32 {
        self.positions(style, text, 0.0).last().copied().unwrap_or(0.0)
    }
}

impl Default for Measurer {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything one page of a printout is drawn from.
struct Job<'a> {
    book: &'a Workbook,
    sheet: &'a Sheet,
    layout: &'a PageLayout,
    scale: f32,
}

/// The paginator's pages as the PDF writer's pages.
pub fn pdf_pages(book: &Book, layout: &PageLayout, measurer: &mut Measurer) -> Pages {
    let workbook = &book.session.book;
    let job = Job {
        book: workbook,
        sheet: &workbook.sheets[layout.sheet],
        layout,
        scale: layout.scale as f32 / 100.0,
    };
    let mut pages: Vec<PdfPage> = layout.pages.iter().map(|p| job.page(p, measurer)).collect();
    for (i, notes) in layout.note_pages.iter().enumerate() {
        pages.push(job.note_page(notes, layout.pages.len() + i, measurer));
    }
    Pages { pages }
}

/// A page of the PDF with nothing on it yet.
fn blank(layout: &PageLayout, number: u32, items: Vec<Item>) -> PdfPage {
    let (w, h) = (layout.paper.width as f32, layout.paper.height as f32);
    PdfPage {
        width: w,
        height: h,
        number,
        section: 0,
        body: Rect::new(
            layout.margins.left as f32,
            layout.margins.top as f32,
            w - (layout.margins.left + layout.margins.right) as f32,
            h - (layout.margins.top + layout.margins.bottom) as f32,
        ),
        items,
        carets: Vec::new(),
        first: None,
        background: None,
    }
}

fn style_of(shown: &Shown, scale: f32, black_and_white: bool) -> CharStyle {
    let color = match (black_and_white, shown.color) {
        (false, Some((r, g, b))) => Color::rgb(r, g, b),
        _ => Color::rgb(0, 0, 0),
    };
    CharStyle {
        family: shown.family.clone(),
        size: shown.size * scale,
        bold: shown.bold,
        italic: shown.italic,
        underline: shown.underline,
        strikethrough: shown.strike,
        color,
    }
}

impl Job<'_> {
    /// Draw one page: the cells, their fills and borders, the headings, the header and the footer.
    fn page(&self, page: &Page, measurer: &mut Measurer) -> PdfPage {
        let rows = lines(page.title_rows, page.body.first.row, page.body.last.row);
        let cols = lines(page.title_cols, page.body.first.col, page.body.last.col);
        let (x0, y0) = (page.origin.0 as f32, page.origin.1 as f32);
        let widths: Vec<f32> = cols.iter().map(|c| self.column(*c)).collect();
        let heights: Vec<f32> = rows.iter().map(|r| self.row(*r)).collect();
        let xs = running(x0, &widths);
        let ys = running(y0, &heights);
        let mut items = Vec::new();
        for (ri, row) in rows.iter().enumerate() {
            for (ci, col) in cols.iter().enumerate() {
                let rect = Rect::new(xs[ci], ys[ri], widths[ci], heights[ri]);
                if rect.w <= 0.0 || rect.h <= 0.0 {
                    continue;
                }
                let at = CellRef::new(*row, *col);
                self.cell(at, rect, &xs[ci..], &widths[ci..], measurer, &mut items);
            }
        }
        if self.layout.gridlines {
            grid_lines(&xs, &ys, &widths, &heights, &mut items);
        }
        if self.layout.headings {
            self.headings(&rows, &cols, &xs, &ys, &widths, &heights, measurer, &mut items);
        }
        self.header_and_footer(page, measurer, &mut items);
        let mut out = blank(self.layout, page.number.max(0) as u32, items);
        out.number = page.number.max(0) as u32;
        out
    }

    fn column(&self, col: u32) -> f32 {
        self.layout.column_points.get(col as usize).copied().unwrap_or(0.0) as f32
    }

    fn row(&self, row: u32) -> f32 {
        self.layout.row_points.get(row as usize).copied().unwrap_or(0.0) as f32
    }

    /// One cell: its fill, its text and its borders.
    fn cell(
        &self,
        at: CellRef,
        rect: Rect,
        xs: &[f32],
        widths: &[f32],
        measurer: &mut Measurer,
        items: &mut Vec<Item>,
    ) {
        let chars = display::chars_in(rect.w / self.scale);
        let mut shown = display::shown(self.book, self.sheet, at, chars);
        if shown.error {
            shown.text =
                unluminous_sheet::pages::layout::printed_error(self.layout.errors, &shown.text);
        }
        if let (Some((r, g, b)), false) = (shown.fill, self.layout.black_and_white) {
            items.push(Item::Fill { rect, color: Rgb(r, g, b) });
        }
        if !shown.text.is_empty() {
            self.text(&shown, rect, xs, widths, at, measurer, items);
        }
        borders(&shown, rect, self.scale, items);
    }

    /// The text of a cell, set in lines that fit its width when it wraps.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        shown: &Shown,
        rect: Rect,
        xs: &[f32],
        widths: &[f32],
        at: CellRef,
        measurer: &mut Measurer,
        items: &mut Vec<Item>,
    ) {
        let style = style_of(shown, self.scale, self.layout.black_and_white);
        let room = self.room(shown, at, rect, widths);
        let lines = if shown.wrap {
            wrap_text(&shown.text, room - 3.0, &style, measurer)
        } else {
            vec![shown.text.clone()]
        };
        let line_height = style.size * 1.2;
        let block = line_height * lines.len() as f32;
        let top = match shown.down {
            Down::Top => rect.y + 1.0,
            Down::Center => rect.y + (rect.h - block) / 2.0,
            Down::Bottom => rect.y + rect.h - block - 1.0,
        };
        let _ = xs;
        for (i, line) in lines.iter().enumerate() {
            let width = measurer.width(&style, line);
            let left = match shown.across {
                Across::Right => rect.x + rect.w - width - 2.0,
                Across::Center | Across::CenterAcross => rect.x + (rect.w - width) / 2.0,
                _ if shown.numeric && shown.across != Across::Left => rect.x + rect.w - width - 2.0,
                _ => rect.x + 2.0 + shown.indent as f32 * 3.0 * self.scale,
            };
            let cut = fit(line, width, room - 2.0, &style, measurer);
            items.push(Item::Text(TextItem {
                baseline: top + line_height * i as f32 + style.size,
                xs: measurer.positions(&style, &cut, left)[..cut.chars().count()].to_vec(),
                text: cut,
                style: style.clone(),
            }));
        }
    }

    /// The width text may take: the cell's own, and the empty cells after it for text that runs on.
    fn room(&self, shown: &Shown, at: CellRef, rect: Rect, widths: &[f32]) -> f32 {
        let runs_on = !shown.wrap && !shown.numeric && shown.across == Across::Left;
        if !runs_on {
            return rect.w;
        }
        let mut room = rect.w;
        for (i, w) in widths.iter().enumerate().skip(1) {
            let next = CellRef::new(at.row, at.col + i as u32);
            if self.sheet.cell(next).is_some_and(|c| !c.value.is_empty()) {
                break;
            }
            room += w;
        }
        room
    }

    /// Row numbers down the left of the cells and column letters across the top.
    #[allow(clippy::too_many_arguments)]
    fn headings(
        &self,
        rows: &[u32],
        cols: &[u32],
        xs: &[f32],
        ys: &[f32],
        widths: &[f32],
        heights: &[f32],
        measurer: &mut Measurer,
        items: &mut Vec<Item>,
    ) {
        let style = heading_style(self.scale);
        let (Some(&left), Some(&top)) = (xs.first(), ys.first()) else { return };
        for (i, row) in rows.iter().enumerate() {
            let text = (row + 1).to_string();
            let x = left - 3.0 - measurer.width(&style, &text);
            items.push(text_item(&style, text, x.max(0.0), ys[i] + heights[i] * 0.75, measurer));
        }
        for (i, col) in cols.iter().enumerate() {
            let text = unluminous_sheet::coord::column_name(*col);
            let x = xs[i] + (widths[i] - measurer.width(&style, &text)) / 2.0;
            items.push(text_item(&style, text, x, (top - 3.0).max(style.size), measurer));
        }
    }

    /// The header above the cells and the footer below them, in their three sections.
    fn header_and_footer(&self, page: &Page, measurer: &mut Measurer, items: &mut Vec<Item>) {
        let m = &self.layout.margins;
        let width = self.layout.paper.width as f32;
        let left = m.left as f32;
        let right = width - m.right as f32;
        let header_y = m.header as f32;
        let footer_y = self.layout.paper.height as f32 - m.footer as f32;
        sections(&page.header, left, right, header_y, true, measurer, items);
        sections(&page.footer, left, right, footer_y, false, measurer, items);
    }

    /// A page of the notes that print after the sheet.
    fn note_page(&self, notes: &[NoteLine], index: usize, measurer: &mut Measurer) -> PdfPage {
        let style = heading_style(1.0);
        let m = &self.layout.margins;
        let mut items = Vec::new();
        for (i, note) in notes.iter().enumerate() {
            let text = format!("{}: {}: {}", note.cell.a1(), note.author, note.text);
            let y = m.top as f32 + style.size * 1.4 * (i + 1) as f32;
            items.push(text_item(&style, text, m.left as f32, y, measurer));
        }
        blank(self.layout, index as u32 + 1, items)
    }
}

/// The rows or columns of a page: the repeated titles, then the body.
fn lines(titles: Option<(u32, u32)>, first: u32, last: u32) -> Vec<u32> {
    let mut all: Vec<u32> = titles.map(|(a, b)| (a..=b).collect()).unwrap_or_default();
    all.extend(first..=last);
    all
}

/// Where each element starts, given where the first does and how big each is.
fn running(start: f32, sizes: &[f32]) -> Vec<f32> {
    let mut at = start;
    sizes
        .iter()
        .map(|s| {
            let here = at;
            at += s;
            here
        })
        .collect()
}

fn heading_style(scale: f32) -> CharStyle {
    CharStyle {
        family: "Calibri".into(),
        size: 9.0 * scale,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        color: Color::rgb(90, 90, 90),
    }
}

fn text_item(style: &CharStyle, text: String, x: f32, baseline: f32, m: &mut Measurer) -> Item {
    let xs = m.positions(style, &text, x)[..text.chars().count()].to_vec();
    Item::Text(TextItem { baseline, style: style.clone(), text, xs })
}

/// Light grey rules between the cells.
fn grid_lines(xs: &[f32], ys: &[f32], widths: &[f32], heights: &[f32], items: &mut Vec<Item>) {
    let (Some(&x0), Some(&y0)) = (xs.first(), ys.first()) else { return };
    let x1 = xs.last().copied().unwrap_or(x0) + widths.last().copied().unwrap_or(0.0);
    let y1 = ys.last().copied().unwrap_or(y0) + heights.last().copied().unwrap_or(0.0);
    let grey = Rgb(191, 191, 191);
    let rule = |x0, y0, x1, y1| Item::Rule {
        x0,
        y0,
        x1,
        y1,
        width: 0.5,
        color: grey,
        kind: RuleKind::Solid,
    };
    for (i, x) in xs.iter().enumerate() {
        items.push(rule(*x, y0, *x, y1));
        if i + 1 == xs.len() {
            items.push(rule(x + widths[i], y0, x + widths[i], y1));
        }
    }
    for (i, y) in ys.iter().enumerate() {
        items.push(rule(x0, *y, x1, *y));
        if i + 1 == ys.len() {
            items.push(rule(x0, y + heights[i], x1, y + heights[i]));
        }
    }
}

/// The borders a cell has, as rules along its edges.
fn borders(shown: &Shown, rect: Rect, scale: f32, items: &mut Vec<Item>) {
    let edges = [
        (&shown.top, rect.x, rect.y, rect.x + rect.w, rect.y),
        (&shown.bottom, rect.x, rect.y + rect.h, rect.x + rect.w, rect.y + rect.h),
        (&shown.left, rect.x, rect.y, rect.x, rect.y + rect.h),
        (&shown.right, rect.x + rect.w, rect.y, rect.x + rect.w, rect.y + rect.h),
    ];
    for (line, x0, y0, x1, y1) in edges {
        let Some(line) = line else { continue };
        let (r, g, b) = line.color.unwrap_or((0, 0, 0));
        items.push(Item::Rule {
            x0,
            y0,
            x1,
            y1,
            width: (line.width * scale).max(0.25),
            color: Rgb(r, g, b),
            kind: match (line.double, line.dashed) {
                (true, _) => RuleKind::Double,
                (_, true) => RuleKind::Dashed,
                _ => RuleKind::Solid,
            },
        });
    }
}

/// Cut text so it is no wider than `room`.
fn fit(text: &str, width: f32, room: f32, style: &CharStyle, m: &mut Measurer) -> String {
    if width <= room {
        return text.to_owned();
    }
    let positions = m.positions(style, text, 0.0);
    let keep = positions.iter().take_while(|p| **p <= room).count().saturating_sub(1);
    text.chars().take(keep).collect()
}

/// Break text into lines no wider than `room`, at spaces.
fn wrap_text(text: &str, room: f32, style: &CharStyle, m: &mut Measurer) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let trial = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && m.width(style, &trial) > room {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            } else {
                line = trial;
            }
        }
        lines.push(line);
    }
    lines
}

/// The three sections of a header or footer on one line: left, centre and right.
fn sections(
    s: &Sections,
    left: f32,
    right: f32,
    y: f32,
    header: bool,
    m: &mut Measurer,
    items: &mut Vec<Item>,
) {
    for (runs, side) in [(&s.left, 0), (&s.center, 1), (&s.right, 2)] {
        let Some(first) = runs.first() else { continue };
        let style = run_style(first);
        let text = Sections::plain(runs);
        if text.is_empty() {
            continue;
        }
        let width = m.width(&style, &text);
        let x = match side {
            0 => left,
            1 => (left + right - width) / 2.0,
            _ => right - width,
        };
        let baseline = if header { y + style.size } else { y };
        items.push(text_item(&style, text, x, baseline, m));
    }
}

fn run_style(run: &Run) -> CharStyle {
    let color = run
        .style
        .color
        .as_deref()
        .and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
        .map_or(Color::rgb(0, 0, 0), |v| Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
    CharStyle {
        family: run.style.font.clone().unwrap_or_else(|| "Calibri".to_owned()).into(),
        size: run.style.size.unwrap_or(11.0) as f32,
        bold: run.style.bold,
        italic: run.style.italic,
        underline: run.style.underline > 0,
        strikethrough: run.style.strike,
        color,
    }
}

/// The sheet as a PDF with exactly the paginator's pages.
pub fn pdf_bytes(book: &Book) -> (Vec<u8>, PageLayout) {
    let layout = layout_of(book);
    let mut measurer = Measurer::new();
    let pages = pdf_pages(book, &layout, &mut measurer);
    let title = book.name();
    let bytes = unluminous_word::pdf::write(
        &pages,
        &title,
        &|style| crate::services::word::commands::pdf_face(style),
        &|_| None,
    );
    (bytes, layout)
}

/// The text of each page of a printout, as the PDF has it, for a test and for `print-preview` read back.
pub fn page_texts(pages: &Pages) -> Vec<String> {
    pages
        .pages
        .iter()
        .map(|p| {
            p.items
                .iter()
                .filter_map(|i| match i {
                    Item::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Every range of the print area, or the used range when there is none, for a read back.
pub fn area_text(book: &Book) -> Vec<String> {
    let areas = unluminous_sheet::pages::print_area(&book.session.book, book.view.sheet);
    areas.iter().map(|r: &Range| r.a1()).collect()
}
