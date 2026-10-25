//! The cells of a sheet: headers, gridlines, fills, borders, text, merged cells, frozen panes, conditional
//! formats, the selection with its fill handle, the copy marquee, and what the pointer does on them.
//!
//! Only the rows and columns on the screen are walked. Positions come from the sheet's two axes
//! (`services::spreadsheet::axis`), multiplied by the tab's zoom. A cell's text is set with Unluminous's own
//! glyph renderer in the workbook's font, so a bold or italic cell is the real face rather than a slanted copy.

use std::collections::HashMap;

use egui::{Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_core::{CharStyle, FontMetrics};
use unluminous_sheet::coord::{column_name, CellRef, Range, MAX_COLS, MAX_ROWS};
use unluminous_sheet::numfmt::Format;
use unluminous_sheet::Value;

use crate::services::plugin_ui::{Look, Request};
use crate::services::spreadsheet::actions::Act;
use crate::services::spreadsheet::conditional::{Effect, Rules};
use crate::services::spreadsheet::config::Paper;
use crate::services::spreadsheet::display::{self, Across, Down, Rgb, Shown};
use crate::services::spreadsheet::view::Drag;
use crate::services::spreadsheet::Book;

/// The colours a sheet is drawn in, decided once a frame from the paper and the theme.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    pub paper: Color32,
    pub text: Color32,
    pub grid: Color32,
    pub header: Color32,
    pub header_text: Color32,
    pub header_chosen: Color32,
    pub accent: Color32,
    pub wash: Color32,
    pub dark: bool,
}

impl Ink {
    pub fn of(look: &Look<'_>, paper: Paper) -> Ink {
        let p = &look.palette;
        let light = paper == Paper::Light || !crate::theme::is_dark();
        let (paper, text, grid) = match paper {
            Paper::Light => {
                let (w, t, g) = (display::LIGHT_PAPER, display::LIGHT_TEXT, display::LIGHT_GRID);
                (rgb(w), rgb(t), rgb(g))
            }
            Paper::Theme => (
                look.ground(p.editor),
                p.text_strong,
                crate::components::controls::mix(p.editor, p.divider, 0.7),
            ),
        };
        Ink {
            paper,
            text,
            grid,
            header: look.ground(p.panel),
            header_text: p.text_dim,
            header_chosen: crate::components::controls::mix(p.panel, p.accent, 0.22),
            accent: p.accent,
            wash: p.accent.gamma_multiply(0.16),
            dark: !light,
        }
    }

    /// A colour from the workbook, as it can be read on this paper.
    pub fn adapt(&self, color: Rgb, over_fill: bool) -> Color32 {
        if !self.dark || over_fill {
            return rgb(color);
        }
        // On dark paper a dark colour the workbook chose for a white page is turned to its light twin, the
        // way Office's dark mode turns one: lightness mirrored, hue kept.
        let luminance = 0.2126 * color.0 as f32 + 0.7152 * color.1 as f32 + 0.0722 * color.2 as f32;
        if luminance >= 110.0 {
            return rgb(color);
        }
        let mirror = |c: u8| 255 - ((255 - c as u16) * 60 / 100) as u8;
        let lifted = (mirror(color.0), mirror(color.1), mirror(color.2));
        let (r, g, b) = (color.0 as f32, color.1 as f32, color.2 as f32);
        let max = r.max(g).max(b);
        if max < 1.0 {
            return self.text;
        }
        // Keep the hue: scale the colour up to the lifted lightness.
        let scale =
            (0.2126 * lifted.0 as f32 + 0.7152 * lifted.1 as f32 + 0.0722 * lifted.2 as f32)
                / luminance.max(1.0);
        let up = |c: f32| (c * scale).min(255.0) as u8;
        Color32::from_rgb(up(r).max(lifted.0 / 2), up(g).max(lifted.1 / 2), up(b).max(lifted.2 / 2))
    }
}

pub fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// How light a colour is, from 0 to 255.
fn lightness(c: Rgb) -> f32 {
    0.2126 * c.0 as f32 + 0.7152 * c.1 as f32 + 0.0722 * c.2 as f32
}

/// The size of the header strips, before the zoom.
pub const COLUMN_HEADER: f32 = 22.0;
pub const ROW_HEADER: f32 = 46.0;
/// How near a header's edge the pointer has to be to resize it.
const EDGE_GRAB: f32 = 4.0;

/// Where the grid is on the screen this frame, and how to turn a cell into a rectangle and back.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The cells' area, under the column header and beside the row header.
    pub cells: Rect,
    pub zoom: f32,
    pub frozen: (u32, u32),
    pub scroll: Vec2,
    /// A right to left sheet: column A is at the right and the row header down the right side.
    pub rtl: bool,
    /// The strip of column letters above these cells and the strip of row numbers beside them, which are
    /// empty when the sheet hides its headings.
    pub column_header: Rect,
    pub row_header: Rect,
}

impl Frame {
    /// The left edge of a column on the screen.
    pub fn x(&self, book: &Book, col: u32) -> f32 {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let start = axes.columns.start(col);
        let shift = if col >= self.frozen.1 { self.scroll.x } else { 0.0 };
        if self.rtl {
            self.cells.right() - (start - shift + axes.columns.size(col)) * self.zoom
        } else {
            self.cells.left() + (start - shift) * self.zoom
        }
    }

    /// Whether a point is on the strip of column letters or on the strip of row numbers.
    pub fn in_column_header(&self, p: Pos2) -> bool {
        self.column_header.contains(p)
    }

    pub fn in_row_header(&self, p: Pos2) -> bool {
        self.row_header.contains(p)
    }

    pub fn y(&self, book: &Book, row: u32) -> f32 {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let start = axes.rows.start(row);
        let shift = if row >= self.frozen.0 { self.scroll.y } else { 0.0 };
        self.cells.top() + (start - shift) * self.zoom
    }

    /// A cell's rectangle on the screen.
    pub fn cell(&self, book: &Book, at: CellRef) -> Rect {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let x = self.x(book, at.col);
        let y = self.y(book, at.row);
        Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(axes.columns.size(at.col) * self.zoom, axes.rows.size(at.row) * self.zoom),
        )
    }

    /// A range's rectangle on the screen.
    pub fn range(&self, book: &Book, range: Range) -> Rect {
        let a = self.cell(book, range.first);
        let b = self.cell(book, range.last);
        a.union(b)
    }

    /// The cell under a point, which may be outside the cells' area (for a drag past the edge).
    pub fn at(&self, book: &Book, p: Pos2) -> CellRef {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let local = if self.rtl {
            Vec2::new(self.cells.right() - p.x, p.y - self.cells.top()) / self.zoom
        } else {
            (p - self.cells.min) / self.zoom
        };
        let frozen_w = axes.columns.start(self.frozen.1);
        let frozen_h = axes.rows.start(self.frozen.0);
        let x = if local.x < frozen_w { local.x } else { local.x + self.scroll.x };
        let y = if local.y < frozen_h { local.y } else { local.y + self.scroll.y };
        CellRef::new(axes.rows.index_at(y), axes.columns.index_at(x))
    }

    /// The first and last rows and columns that show, in the scrolled part.
    pub fn visible(&self, book: &Book) -> (u32, u32, u32, u32) {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let frozen_w = axes.columns.start(self.frozen.1);
        let frozen_h = axes.rows.start(self.frozen.0);
        let first_col = axes.columns.index_at(frozen_w + self.scroll.x).max(self.frozen.1);
        let first_row = axes.rows.index_at(frozen_h + self.scroll.y).max(self.frozen.0);
        let last_col = axes.columns.index_at(
            frozen_w + self.scroll.x + (self.cells.width() / self.zoom - frozen_w).max(0.0),
        );
        let last_row = axes.rows.index_at(
            frozen_h + self.scroll.y + (self.cells.height() / self.zoom - frozen_h).max(0.0),
        );
        (first_row, last_row.min(MAX_ROWS - 1), first_col, last_col.min(MAX_COLS - 1))
    }
}

/// What the grid did this frame.
#[derive(Debug, Default)]
pub struct GridOut {
    pub acts: Vec<Act>,
    pub requests: Vec<Request>,
    /// The pointer pressed on the grid, so the tab takes the keyboard.
    pub pressed: bool,
    /// A cell was double clicked: start typing into it.
    pub edit: Option<CellRef>,
    /// A cell was clicked while a formula was being typed, to be put into it.
    pub point: Option<Range>,
    /// The right click menu asked for, where.
    pub menu: Option<Pos2>,
    /// A filter button was pressed on this column.
    pub filter_menu: Option<(u32, Pos2)>,
    pub frame: Option<Frame>,
    /// Commands the pointer asked for, run after the frame as a menu entry's are.
    pub words: Vec<Vec<String>>,
    /// A menu of commands to open where the pointer let go, as a right drag does.
    pub drop_menu: Option<(Pos2, Vec<crate::services::spreadsheet::catalogue::Item>)>,
}

/// One cell's text, laid out, to draw after the fills and the borders.
struct TextItem {
    lines: Vec<String>,
    style: CharStyle,
    color: Color32,
    clip: Rect,
    /// Where each line starts, left edge and baseline.
    at: Vec<Pos2>,
    underline: bool,
    strike: bool,
    widths: Vec<f32>,
}

/// The width of a string in a style.
fn width_of(look: &Look<'_>, text: &str, style: &CharStyle) -> f32 {
    let mut buffer = [0u8; 4];
    text.chars().map(|c| look.renderer.advance(c.encode_utf8(&mut buffer), style)).sum()
}

/// Words broken into lines no wider than `width`.
fn wrap(look: &Look<'_>, text: &str, style: &CharStyle, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate =
                if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if width_of(look, &candidate, style) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            }
        }
        lines.push(line);
    }
    lines
}

/// Draw the grid into `area` and read the pointer over it.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    book: &mut Book,
    rules: &Rules,
    look: &Look<'_>,
    pointing: bool,
    out: &mut GridOut,
) {
    let ink = Ink::of(look, book.view.paper);
    let zoom = book.view.zoom * look.scale();
    let revision = book.revision;
    {
        let sheet = &book.session.book.sheets[book.view.sheet];
        book.view.axes(sheet, revision);
    }
    let layout = Layout::of(area, book, zoom);
    let mut panes = layout.panes(book);
    book.view.drawn = Some(layout.cells);
    // The pointer first, so a press this frame is drawn this frame. It is read by the pane under it.
    let response =
        ui.interact(area, ui.id().with(("spreadsheet-grid", &book.key)), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Spreadsheet grid")
    });
    let under = pointer_pane(&response, &panes);
    read_the_wheel(ui, &response, book, panes[under].frame, zoom, panes[under].scrolls);
    panes = layout.panes(book);
    let split = read_the_split_bars(ui, &response, book, &layout);
    let on_outline = super::overlay_a::over_outline(ui, book, &layout, panes[under].frame);
    // A press on a picture, shape or text box belongs to it, and so does a drag it has begun.
    let on_object = !split
        && !on_outline
        && !pointing
        && super::objects_c::pointer(ui, &response, book, &panes, out);
    if !split && !on_outline && !on_object {
        read_the_pointer(ui, &response, book, panes[under].frame, area, pointing, out);
    }
    panes = layout.panes(book);
    let main = panes.last().expect("at least one pane").frame;
    out.frame = Some(main);
    ui.painter_at(layout.cells).rect_filled(layout.cells, 0, ink.paper);
    let mut formats: HashMap<u32, Format> = HashMap::new();
    for (index, pane) in panes.iter().enumerate() {
        draw_pane(ui, look, book, rules, pane.frame, &ink, &mut formats);
        draw_selection(ui, book, pane.frame, &ink, look);
        super::objects_c::draw(ui, book, pane.frame, &ink, index);
    }
    draw_dividers(ui, &layout, &panes, look);
    for pane in &panes {
        draw_headers(ui, book, pane, &ink, look);
    }
    draw_corner(ui, &layout, &ink, look);
    super::overlay_a::draw(ui, look, book, &layout, &panes, &ink, out);
}

/// Where the parts of the grid go this frame: the header strips, and the cells beside them.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub area: Rect,
    pub cells: Rect,
    pub zoom: f32,
    pub rtl: bool,
    pub headings: bool,
    /// The split position in points from the cells' top left, when the window is split rather than
    /// frozen.
    pub split: Option<Vec2>,
}

/// One pane of a split window: its frame, and which of the view's two scroll positions it reads across
/// and down (`false` is the main scroll, `true` the split's own).
#[derive(Debug, Clone, Copy)]
pub struct Pane {
    pub frame: Frame,
    pub scrolls: (bool, bool),
}

impl Layout {
    pub fn of(area: Rect, book: &Book, zoom: f32) -> Layout {
        let sheet = &book.session.book.sheets[book.view.sheet];
        let headings = sheet.view.show_headings;
        let rtl = sheet.view.right_to_left;
        let (rw, ch) =
            if headings { (ROW_HEADER * zoom, COLUMN_HEADER * zoom) } else { (0.0, 0.0) };
        let cells = if rtl {
            Rect::from_min_max(
                Pos2::new(area.left(), area.top() + ch),
                Pos2::new(area.right() - rw, area.bottom()),
            )
        } else {
            Rect::from_min_max(Pos2::new(area.left() + rw, area.top() + ch), area.max)
        };
        let split = sheet.frozen.is_none().then(|| sheet.view.split.as_ref()).flatten().map(|sp| {
            // A split is saved in twentieths of a point from the window's top left, headers included.
            let x = if sp.x > 0.0 { (sp.x as f32 / 20.0 - rw / zoom).max(8.0) * zoom } else { 0.0 };
            let y = if sp.y > 0.0 { (sp.y as f32 / 20.0 - ch / zoom).max(8.0) * zoom } else { 0.0 };
            Vec2::new(x.min(cells.width() - 8.0), y.min(cells.height() - 8.0))
        });
        Layout { area, cells, zoom, rtl, headings, split }
    }

    /// The panes: one, or two or four for a split window, the main pane (bottom right) last.
    pub fn panes(&self, book: &Book) -> Vec<Pane> {
        let sheet = &book.session.book.sheets[book.view.sheet];
        let frozen = sheet.frozen.unwrap_or((0, 0));
        let make = |cells: Rect, scrolls: (bool, bool)| {
            let scroll = Vec2::new(
                if scrolls.0 { book.view.split_scroll.x } else { book.view.scroll.x },
                if scrolls.1 { book.view.split_scroll.y } else { book.view.scroll.y },
            );
            let column_header = Rect::from_min_max(
                Pos2::new(cells.left(), self.area.top()),
                Pos2::new(cells.right(), self.cells.top()),
            );
            let row_header = if self.rtl {
                Rect::from_min_max(
                    Pos2::new(self.cells.right(), cells.top()),
                    Pos2::new(self.area.right(), cells.bottom()),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(self.area.left(), cells.top()),
                    Pos2::new(self.cells.left(), cells.bottom()),
                )
            };
            let frozen = if self.split.is_some() { (0, 0) } else { frozen };
            let frame = Frame {
                cells,
                zoom: self.zoom,
                frozen,
                scroll,
                rtl: self.rtl,
                column_header,
                row_header,
            };
            Pane { frame, scrolls }
        };
        let Some(split) = self.split else { return vec![make(self.cells, (false, false))] };
        let c = self.cells;
        let (sx, sy) = (split.x, split.y);
        // In a right to left sheet the first pane across is the one at the right.
        let (first_x, second_x) = if self.rtl {
            ((c.right() - sx, c.right()), (c.left(), c.right() - sx))
        } else {
            ((c.left(), c.left() + sx), (c.left() + sx, c.right()))
        };
        let cols: Vec<((f32, f32), bool)> = if sx > 0.0 {
            vec![(first_x, true), (second_x, false)]
        } else {
            vec![((c.left(), c.right()), false)]
        };
        let rows: Vec<((f32, f32), bool)> = if sy > 0.0 {
            vec![((c.top(), c.top() + sy), true), ((c.top() + sy, c.bottom()), false)]
        } else {
            vec![((c.top(), c.bottom()), false)]
        };
        let mut panes = Vec::new();
        for ((top, bottom), own_y) in &rows {
            for ((left, right), own_x) in &cols {
                let rect = Rect::from_min_max(Pos2::new(*left, *top), Pos2::new(*right, *bottom));
                panes.push(make(rect, (*own_x, *own_y)));
            }
        }
        panes
    }
}

/// The pane the pointer is over, or the main pane.
fn pointer_pane(response: &egui::Response, panes: &[Pane]) -> usize {
    let main = panes.len() - 1;
    let Some(p) = response.hover_pos().or_else(|| response.interact_pointer_pos()) else {
        return main;
    };
    panes
        .iter()
        .position(|pane| {
            let f = pane.frame;
            f.cells.contains(p)
                || (f.column_header.contains(p))
                || (f.row_header.contains(p) && p.y >= f.cells.top() && p.y < f.cells.bottom())
        })
        .unwrap_or(main)
}

/// Dragging a split bar moves it; dragged back to the edge it removes the split. Answers whether the
/// pointer is on a split bar, so the cells do not read the same drag.
fn read_the_split_bars(
    ui: &egui::Ui,
    response: &egui::Response,
    book: &mut Book,
    layout: &Layout,
) -> bool {
    let Some(split) = layout.split else { return false };
    let Some(p) = response.hover_pos().or_else(|| response.interact_pointer_pos()) else {
        return false;
    };
    let c = layout.cells;
    let bar_x = if layout.rtl { c.right() - split.x } else { c.left() + split.x };
    let bar_y = c.top() + split.y;
    let id = egui::Id::new(("split-bar", &book.key));
    let held: Option<(bool, bool)> = ui.ctx().data(|d| d.get_temp(id));
    let near_x = split.x > 0.0 && (p.x - bar_x).abs() <= 4.0;
    let near_y = split.y > 0.0 && (p.y - bar_y).abs() <= 4.0;
    let grip = held.unwrap_or((near_x, near_y));
    if !(grip.0 || grip.1) {
        return false;
    }
    ui.ctx().set_cursor_icon(match grip {
        (true, true) => egui::CursorIcon::Move,
        (true, false) => egui::CursorIcon::ResizeColumn,
        _ => egui::CursorIcon::ResizeRow,
    });
    if response.drag_started() {
        ui.ctx().data_mut(|d| d.insert_temp(id, grip));
    }
    if response.dragged() || response.drag_stopped() {
        let header =
            if layout.headings { Vec2::new(ROW_HEADER, COLUMN_HEADER) } else { Vec2::ZERO };
        let along = if layout.rtl { c.right() - p.x } else { p.x - c.left() };
        if let Some(sp) = book.session.book.sheets[book.view.sheet].view.split.clone() {
            let mut next = sp;
            if grip.0 {
                next.x = if along < 8.0 {
                    0.0
                } else {
                    ((along / layout.zoom + header.x) * 20.0) as f64
                };
            }
            if grip.1 {
                let down = p.y - c.top();
                next.y =
                    if down < 8.0 { 0.0 } else { ((down / layout.zoom + header.y) * 20.0) as f64 };
            }
            if response.drag_stopped() {
                ui.ctx().data_mut(|d| d.remove::<(bool, bool)>(id));
                let gone = next.x <= 0.0 && next.y <= 0.0;
                let _ = book.session.split(book.view.sheet, (!gone).then_some(next));
                book.changed();
            } else {
                book.session.book.sheets[book.view.sheet].view.split = Some(next);
                book.changed();
            }
        }
    }
    true
}

/// The cells of one pane: a frozen sheet's four parts, or the one part a pane of a split window has.
fn draw_pane(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    rules: &Rules,
    frame: Frame,
    ink: &Ink,
    formats: &mut HashMap<u32, Format>,
) {
    let cells = frame.cells;
    let frozen = frame.frozen;
    let zoom = frame.zoom;
    let (first_row, last_row, first_col, last_col) = frame.visible(book);
    // The four parts of a frozen sheet: the corner that never moves, the rows along the top, the columns
    // down the left, and the scrolled part, each clipped to itself.
    let frozen_w = book.view.axes.as_ref().map_or(0.0, |a| a.columns.start(frozen.1)) * zoom;
    let frozen_h = book.view.axes.as_ref().map_or(0.0, |a| a.rows.start(frozen.0)) * zoom;
    let across = |from: f32, to: f32| {
        if frame.rtl {
            (cells.right() - to, cells.right() - from)
        } else {
            (cells.left() + from, cells.left() + to)
        }
    };
    let rect = |x: (f32, f32), top: f32, bottom: f32| {
        Rect::from_min_max(Pos2::new(x.0, top), Pos2::new(x.1, bottom))
    };
    let fixed = across(0.0, frozen_w);
    let moving = across(frozen_w, cells.width());
    let parts = [
        (0..frozen.0, 0..frozen.1, rect(fixed, cells.top(), cells.top() + frozen_h)),
        (0..frozen.0, first_col..last_col + 1, rect(moving, cells.top(), cells.top() + frozen_h)),
        (first_row..last_row + 1, 0..frozen.1, rect(fixed, cells.top() + frozen_h, cells.bottom())),
        (
            first_row..last_row + 1,
            first_col..last_col + 1,
            rect(moving, cells.top() + frozen_h, cells.bottom()),
        ),
    ];
    for (rows, cols, clip) in parts.iter().cloned() {
        if rows.is_empty() || cols.is_empty() || clip.width() <= 0.0 || clip.height() <= 0.0 {
            continue;
        }
        let clip = clip.intersect(cells);
        draw_cells(
            ui,
            look,
            book,
            rules,
            frame,
            ink,
            rows.start,
            rows.end - 1,
            cols.start,
            cols.end - 1,
            clip,
            formats,
        );
    }
    let painter = ui.painter_at(cells);
    // The line under the frozen rows and beside the frozen columns.
    if frozen.0 > 0 {
        let y = cells.top() + frozen_h;
        painter.line_segment(
            [Pos2::new(cells.left(), y), Pos2::new(cells.right(), y)],
            Stroke::new(1.5, look.palette.text_faint),
        );
    }
    if frozen.1 > 0 {
        let x = fixed.1.max(fixed.0) - if frame.rtl { frozen_w } else { 0.0 };
        painter.line_segment(
            [Pos2::new(x, cells.top()), Pos2::new(x, cells.bottom())],
            Stroke::new(1.5, look.palette.text_faint),
        );
    }
}

/// The split bars between the panes of a split window.
fn draw_dividers(ui: &egui::Ui, layout: &Layout, panes: &[Pane], look: &Look<'_>) {
    let Some(split) = layout.split else { return };
    let painter = ui.painter_at(layout.area);
    let c = layout.cells;
    let stroke = Stroke::new(4.0, look.palette.divider);
    if split.x > 0.0 {
        let x = if layout.rtl { c.right() - split.x } else { c.left() + split.x };
        painter.line_segment([Pos2::new(x, layout.area.top()), Pos2::new(x, c.bottom())], stroke);
    }
    if split.y > 0.0 {
        let y = c.top() + split.y;
        let (left, right) = if layout.rtl {
            (c.left(), layout.area.right())
        } else {
            (layout.area.left(), c.right())
        };
        painter.line_segment([Pos2::new(left, y), Pos2::new(right, y)], stroke);
    }
    let _ = panes;
}

/// The wheel scrolls; with Shift it scrolls across; with Ctrl it zooms.
fn read_the_wheel(
    ui: &egui::Ui,
    response: &egui::Response,
    book: &mut Book,
    frame: Frame,
    zoom: f32,
    scrolls: (bool, bool),
) {
    if !response.contains_pointer() {
        return;
    }
    let (delta, modifiers, zoom_delta) =
        ui.input(|i| (i.smooth_scroll_delta, i.modifiers, i.zoom_delta()));
    if modifiers.command && zoom_delta != 1.0 {
        book.view.zoom = (book.view.zoom * zoom_delta).clamp(0.25, 4.0);
        return;
    }
    if delta == Vec2::ZERO {
        return;
    }
    let delta = if modifiers.shift && delta.x == 0.0 { Vec2::new(delta.y, 0.0) } else { delta };
    let axes = book.view.axes.as_ref().expect("built");
    let sheet = &book.session.book.sheets[book.view.sheet];
    let used = sheet.used_range().map_or(CellRef::new(0, 0), |r| r.last);
    // Scrolling stops a few screens past what is in use, as Excel's bars do, so the wheel cannot lose the data.
    let max_x = (axes.columns.end(used.col.max(book.view.cursor.col))
        + frame.cells.width() / zoom * 2.0)
        .min(axes.columns.total());
    let max_y = (axes.rows.end(used.row.max(book.view.cursor.row))
        + frame.cells.height() / zoom * 2.0)
        .min(axes.rows.total());
    let x = if scrolls.0 { &mut book.view.split_scroll.x } else { &mut book.view.scroll.x };
    let dx = if frame.rtl { -delta.x } else { delta.x };
    *x = (*x - dx / zoom).clamp(0.0, max_x);
    let y = if scrolls.1 { &mut book.view.split_scroll.y } else { &mut book.view.scroll.y };
    *y = (*y - delta.y / zoom).clamp(0.0, max_y);
}

/// Presses, drags and double clicks on the cells and the headers.
fn read_the_pointer(
    ui: &egui::Ui,
    response: &egui::Response,
    book: &mut Book,
    frame: Frame,
    area: Rect,
    pointing: bool,
    out: &mut GridOut,
) {
    let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
    let Some(p) = pointer else { return };
    let modifiers = ui.input(|i| i.modifiers);
    let in_columns = frame.in_column_header(p);
    let in_rows = frame.in_row_header(p);
    let at = frame.at(book, p);
    let axes = book.view.axes.clone().expect("built");
    // The edge of a header under the pointer, for the resize cursor and the drag.
    let column_edge = in_columns
        .then(|| {
            let col = at.col;
            let (left, right) = {
                let a = frame.x(book, col);
                let b = a + axes.columns.size(col) * frame.zoom;
                if frame.rtl {
                    (b, a)
                } else {
                    (a, b)
                }
            };
            if (p.x - right).abs() <= EDGE_GRAB {
                Some(col)
            } else if (p.x - left).abs() <= EDGE_GRAB && col > 0 {
                Some(axes.columns.next_shown(col, -1))
            } else {
                None
            }
        })
        .flatten();
    let row_edge = in_rows
        .then(|| {
            let row = at.row;
            let bottom = frame.y(book, row) + axes.rows.size(row) * frame.zoom;
            let top = frame.y(book, row);
            if (p.y - bottom).abs() <= EDGE_GRAB {
                Some(row)
            } else if (p.y - top).abs() <= EDGE_GRAB && row > 0 {
                Some(axes.rows.next_shown(row, -1))
            } else {
                None
            }
        })
        .flatten();
    // The fill handle at the bottom right of the selection.
    let range = book.view.range();
    let handle = frame.range(book, range).right_bottom();
    let on_handle = !pointing && (p - handle).length() <= 5.0 && book.view.extra.is_empty();
    let border = (!pointing && !on_handle).then(|| on_the_border(book, frame, p)).flatten();
    if response.hovered() {
        if column_edge.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        } else if row_edge.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
        } else if on_handle {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if border.is_some() || matches!(book.view.drag, Some(Drag::Move { .. })) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        } else if frame.cells.contains(p) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Cell);
        }
    }
    let secondary_drag = response.drag_started_by(egui::PointerButton::Secondary);
    if secondary_drag && (border.is_some() || on_handle) {
        out.pressed = true;
        book.view.drag = Some(if on_handle {
            Drag::Fill { right: true }
        } else {
            let grab = at;
            Drag::Move {
                rows: grab.row.saturating_sub(range.first.row).min(range.rows() - 1),
                cols: grab.col.saturating_sub(range.first.col).min(range.cols() - 1),
                right: true,
            }
        });
        return;
    }
    if response.secondary_clicked() {
        out.pressed = true;
        if !frame.range(book, range).contains(p)
            && !book.view.extra.iter().any(|r| frame.range(book, *r).contains(p))
            && frame.cells.contains(p)
        {
            book.view.select_cell(at);
        }
        out.menu = Some(p);
        return;
    }
    if response.double_clicked() {
        if let Some(col) = column_edge {
            book.view.select_columns(col, col);
            out.acts.push(Act::AutoFitColumns);
            book.view.select_cell(CellRef::new(book.view.cursor.row.min(MAX_ROWS - 1), col));
            return;
        }
        if let Some(row) = row_edge {
            // A double click on a row's lower edge fits the row to what is in it, as for a column.
            book.view.select_rows(row, row);
            out.acts.push(Act::AutoFitRows);
            book.view.select_cell(CellRef::new(row, book.view.cursor.col.min(MAX_COLS - 1)));
            return;
        }
        if on_handle {
            // A double click on the fill handle fills down as far as the column beside goes.
            if let Some(to) = fill_down_extent(book) {
                out.acts.push(Act::FillTo(to));
            }
            return;
        }
        if let Some(edge) = border {
            // A double click on the active cell's border goes to the edge of the data that way.
            let (rows, cols) = edge;
            let sheet = &book.session.book.sheets[book.view.sheet];
            let to = crate::services::spreadsheet::view::edge_of_data(
                sheet,
                book.view.cursor,
                rows,
                cols,
            );
            book.view.select_cell(to);
            return;
        }
        if frame.cells.contains(p) {
            out.edit = Some(at);
            return;
        }
    }
    if response.drag_started() || (response.clicked() && book.view.drag.is_none()) {
        out.pressed = true;
        book.view.held = None;
        let filter_button = filter_button_at(book, frame, p);
        if let Some(col) = filter_button {
            out.filter_menu = Some((col, p));
            return;
        }
        if let Some(col) = column_edge {
            book.view.drag = Some(Drag::ColumnEdge(col, axes.columns.size(col)));
        } else if let Some(row) = row_edge {
            book.view.drag = Some(Drag::RowEdge(row, axes.rows.size(row)));
        } else if on_handle {
            book.view.drag = Some(Drag::Fill { right: false });
        } else if border.is_some() && response.drag_started() {
            book.view.drag = Some(Drag::Move {
                rows: at.row.saturating_sub(range.first.row).min(range.rows() - 1),
                cols: at.col.saturating_sub(range.first.col).min(range.cols() - 1),
                right: false,
            });
        } else if !frame.cells.contains(p) && !in_columns && !in_rows {
            book.view.select_range(Range::new(
                CellRef::new(0, 0),
                CellRef::new(MAX_ROWS - 1, MAX_COLS - 1),
            ));
            book.view.cursor = CellRef::new(0, 0);
        } else if in_columns {
            if modifiers.shift {
                book.view.anchor = CellRef::new(MAX_ROWS - 1, book.view.anchor.col);
                book.view.cursor = CellRef::new(0, at.col);
            } else {
                book.view.select_columns(at.col, at.col);
            }
            book.view.drag = Some(Drag::Columns(at.col));
        } else if in_rows {
            if modifiers.shift {
                book.view.anchor = CellRef::new(book.view.anchor.row, MAX_COLS - 1);
                book.view.cursor = CellRef::new(at.row, 0);
            } else {
                book.view.select_rows(at.row, at.row);
            }
            book.view.drag = Some(Drag::Rows(at.row));
        } else if frame.cells.contains(p) {
            if pointing {
                book.view.drag = Some(Drag::Point);
                book.view.extra.clear();
                out.point = Some(Range::cell(at));
                book.view.choices = false;
                return;
            }
            // Ctrl and a link opens the link.
            if modifiers.command {
                if let Some(link) = book.session.book.sheets[book.view.sheet]
                    .links
                    .iter()
                    .find(|l| l.range.contains(at))
                    .cloned()
                {
                    open_link(book, &link.target, out);
                    return;
                }
                book.view.extra.push(book.view.range());
                book.view.cursor = at;
                book.view.anchor = at;
            } else if modifiers.shift {
                // The active cell stays where it was and the range grows to the click.
                book.view.anchor = at;
            } else {
                // A click on a merged cell chooses all of it.
                let merged = book.session.book.sheets[book.view.sheet].merge_at(at);
                match merged {
                    Some(m) => {
                        book.view.select_cell(m.first);
                        book.view.anchor = m.last;
                    }
                    None => book.view.select_cell(at),
                }
            }
            book.view.drag = Some(Drag::Select);
            book.view.choices = false;
        }
    }
    if response.dragged() {
        // Past the edge of the cells, the sheet scrolls towards the pointer.
        let edge_scroll = |inside: f32, low: f32, high: f32| {
            if inside < low {
                (inside - low) / 4.0
            } else if inside > high {
                (inside - high) / 4.0
            } else {
                0.0
            }
        };
        let push = Vec2::new(
            edge_scroll(p.x, frame.cells.left(), frame.cells.right()),
            edge_scroll(p.y, frame.cells.top(), frame.cells.bottom()),
        );
        let clamped =
            Pos2::new(p.x.clamp(area.left(), area.right()), p.y.clamp(area.top(), area.bottom()));
        let target = frame.at(book, clamped.max(frame.cells.min));
        match book.view.drag {
            Some(Drag::Select) => {
                book.view.anchor = target;
                scroll_by(book, push / frame.zoom);
            }
            Some(Drag::Point) => {
                if let Some(Range { first, .. }) = out.point {
                    out.point = Some(Range::new(first, target));
                }
                scroll_by(book, push / frame.zoom);
            }
            Some(Drag::Columns(from)) => {
                book.view.cursor = CellRef::new(0, from.min(target.col));
                book.view.anchor = CellRef::new(MAX_ROWS - 1, from.max(target.col));
                scroll_by(book, Vec2::new(push.x, 0.0) / frame.zoom);
            }
            Some(Drag::Rows(from)) => {
                book.view.cursor = CellRef::new(from.min(target.row), 0);
                book.view.anchor = CellRef::new(from.max(target.row), MAX_COLS - 1);
                scroll_by(book, Vec2::new(0.0, push.y) / frame.zoom);
            }
            Some(Drag::Fill { .. }) => scroll_by(book, push / frame.zoom),
            Some(Drag::Move { rows, cols, .. }) => {
                scroll_by(book, push / frame.zoom);
                let r = book.view.range();
                let to = moved_to(target, rows, cols, r);
                let shape = frame.range(book, to);
                ui.painter_at(frame.cells).rect_stroke(
                    shape,
                    0,
                    Stroke::new(2.0, ui.visuals().selection.stroke.color),
                    egui::StrokeKind::Inside,
                );
            }
            _ => {}
        }
    }
    if response.drag_stopped() {
        match book.view.drag.take() {
            Some(Drag::ColumnEdge(col, _)) => {
                let width = ((p.x - frame.x(book, col)) / frame.zoom).max(0.0);
                let chosen = book.view.range();
                let cols = if chosen.is_whole_columns()
                    && chosen.first.col <= col
                    && col <= chosen.last.col
                {
                    chosen
                } else {
                    Range::new(CellRef::new(0, col), CellRef::new(MAX_ROWS - 1, col))
                };
                book.view.select_range(cols);
                out.acts.push(Act::Width(Some(
                    crate::services::spreadsheet::axis::points_to_width(width),
                )));
            }
            Some(Drag::RowEdge(row, _)) => {
                let height = ((p.y - frame.y(book, row)) / frame.zoom).max(0.0);
                let chosen = book.view.range();
                let rows = if chosen.is_whole_rows()
                    && chosen.first.row <= row
                    && row <= chosen.last.row
                {
                    chosen
                } else {
                    Range::new(CellRef::new(row, 0), CellRef::new(row, MAX_COLS - 1))
                };
                book.view.select_range(rows);
                out.acts.push(Act::Height(Some(
                    crate::services::spreadsheet::axis::points_to_height(height),
                )));
            }
            Some(Drag::Move { rows, cols, right }) => {
                let target = frame.at(book, p.max(frame.cells.min));
                let r = book.view.range();
                let to = moved_to(target, rows, cols, r);
                if to != r {
                    let copy = modifiers.command;
                    let insert = modifiers.shift;
                    if right {
                        out.drop_menu = Some((p, drop_items(to.first)));
                    } else {
                        let mut words = vec!["move-cells".to_owned(), to.first.a1()];
                        if copy {
                            words.push("copy".to_owned());
                        }
                        if insert {
                            words.push(
                                if to.first.row != r.first.row {
                                    "shift-down"
                                } else {
                                    "shift-right"
                                }
                                .to_owned(),
                            );
                        }
                        out.words.push(words);
                    }
                }
            }
            Some(Drag::Fill { right: true }) => {
                let target = frame.at(book, p.max(frame.cells.min));
                if let Some(to) = fill_target(book.view.range(), target) {
                    out.drop_menu = Some((p, fill_items(to)));
                }
            }
            Some(Drag::Fill { right: false }) => {
                let target = frame.at(book, p.max(frame.cells.min));
                if let Some(to) = fill_target(book.view.range(), target) {
                    if modifiers.command {
                        // Ctrl with the fill handle turns a series into a copy and a copy into a series.
                        out.words.push(vec!["fill-handle".to_owned(), to.a1(), "series".to_owned(), "control".to_owned()]);
                    } else {
                        out.acts.push(Act::FillTo(to));
                    }
                }
            }
            _ => {}
        }
    }
    // The size a header is being dragged to, drawn as the drag goes.
    if let Some(Drag::ColumnEdge(col, _)) = book.view.drag {
        let x = p.x.max(frame.x(book, col));
        ui.painter().line_segment(
            [Pos2::new(x, area.top()), Pos2::new(x, area.bottom())],
            Stroke::new(1.0, ui.visuals().selection.stroke.color),
        );
    }
    if let Some(Drag::RowEdge(row, _)) = book.view.drag {
        let y = p.y.max(frame.y(book, row));
        ui.painter().line_segment(
            [Pos2::new(area.left(), y), Pos2::new(area.right(), y)],
            Stroke::new(1.0, ui.visuals().selection.stroke.color),
        );
    }
    if response.clicked()
        && book.view.drag.is_some()
        && !matches!(
            book.view.drag,
            Some(Drag::ColumnEdge(..))
                | Some(Drag::RowEdge(..))
                | Some(Drag::Fill { .. })
                | Some(Drag::Move { .. })
        )
    {
        book.view.drag = None;
    }
}

/// Which edge of the current range the pointer is on, as the step it points along: up, down, left or
/// right. The border is where the cells can be picked up and carried.
fn on_the_border(book: &Book, frame: Frame, p: Pos2) -> Option<(i64, i64)> {
    let r = book.view.range();
    if r.is_whole_columns() || r.is_whole_rows() || !book.view.extra.is_empty() {
        return None;
    }
    let rect = frame.range(book, r);
    let near = |a: f32, b: f32| (a - b).abs() <= 3.0;
    let within_x = p.x >= rect.left() - 3.0 && p.x <= rect.right() + 3.0;
    let within_y = p.y >= rect.top() - 3.0 && p.y <= rect.bottom() + 3.0;
    if within_x && near(p.y, rect.top()) {
        Some((-1, 0))
    } else if within_x && near(p.y, rect.bottom()) {
        Some((1, 0))
    } else if within_y && near(p.x, rect.left()) {
        Some((0, if frame.rtl { 1 } else { -1 }))
    } else if within_y && near(p.x, rect.right()) {
        Some((0, if frame.rtl { -1 } else { 1 }))
    } else {
        None
    }
}

/// Where carried cells land: the range the same size as the one picked up, placed so the cell it was
/// picked up by is under the pointer.
fn moved_to(target: CellRef, rows: u32, cols: u32, from: Range) -> Range {
    let row = target.row.saturating_sub(rows).min(MAX_ROWS - from.rows());
    let col = target.col.saturating_sub(cols).min(MAX_COLS - from.cols());
    Range::new(CellRef::new(row, col), CellRef::new(row + from.rows() - 1, col + from.cols() - 1))
}

/// The range a double click on the fill handle fills: down to the last row of the block in the column
/// beside the selection, left first, then right.
fn fill_down_extent(book: &Book) -> Option<Range> {
    let r = book.view.range();
    let sheet = &book.session.book.sheets[book.view.sheet];
    let filled = |row: u32, col: u32| !sheet.value_at(CellRef::new(row, col)).is_empty();
    let beside = [r.first.col.checked_sub(1), Some(r.last.col + 1)];
    for col in beside.into_iter().flatten() {
        if !filled(r.last.row, col) && !filled(r.first.row, col) {
            continue;
        }
        let mut last = r.last.row;
        while last + 1 < MAX_ROWS && filled(last + 1, col) {
            last += 1;
        }
        if last > r.last.row {
            return Some(Range::new(r.first, CellRef::new(last, r.last.col)));
        }
    }
    None
}

/// The range the fill handle is pulled to: the way the pointer went furthest.
fn fill_target(r: Range, target: CellRef) -> Option<Range> {
    let down = target.row as i64 - r.last.row as i64;
    let up = r.first.row as i64 - target.row as i64;
    let right = target.col as i64 - r.last.col as i64;
    let left = r.first.col as i64 - target.col as i64;
    let most = down.max(up).max(right).max(left);
    if most <= 0 {
        return None;
    }
    Some(if most == down {
        Range::new(r.first, CellRef::new(target.row, r.last.col))
    } else if most == right {
        Range::new(r.first, CellRef::new(r.last.row, target.col))
    } else if most == up {
        Range::new(CellRef::new(target.row, r.first.col), r.last)
    } else {
        Range::new(CellRef::new(r.first.row, target.col), r.last)
    })
}

/// The menu a right drag of the chosen cells opens where it lets go, as Excel's does.
fn drop_items(to: CellRef) -> Vec<crate::services::spreadsheet::catalogue::Item> {
    use crate::services::spreadsheet::catalogue::Item;
    let at = to.a1();
    let entry = |label: &str, more: &str| Item::Run {
        label: label.to_owned(),
        keys: "",
        words: format!("move-cells {at} {more}").split_whitespace().map(str::to_owned).collect(),
        checked: false,
    };
    vec![
        entry("Move here", ""),
        entry("Copy here", "copy"),
        entry("Copy here as values only", "copy values"),
        entry("Copy here as formats only", "copy formats"),
        entry("Link here", "link"),
        entry("Create hyperlink here", "hyperlink"),
        Item::Line,
        entry("Shift down and copy", "copy shift-down"),
        entry("Shift right and copy", "copy shift-right"),
        entry("Shift down and move", "shift-down"),
        entry("Shift right and move", "shift-right"),
    ]
}

/// The menu a right drag of the fill handle opens, as Excel's does.
fn fill_items(to: Range) -> Vec<crate::services::spreadsheet::catalogue::Item> {
    use crate::services::spreadsheet::catalogue::Item;
    let target = to.a1();
    let entry = |label: &str, how: &str| Item::Run {
        label: label.to_owned(),
        keys: "",
        words: format!("fill-handle {target} {how}")
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        checked: false,
    };
    vec![
        entry("Copy cells", "copy"),
        entry("Fill series", "series"),
        entry("Fill formatting only", "formats"),
        entry("Fill without formatting", "values"),
        Item::Line,
        entry("Fill days", "days"),
        entry("Fill weekdays", "weekdays"),
        entry("Fill months", "months"),
        entry("Fill years", "years"),
        Item::Line,
        entry("Linear trend", "linear"),
        entry("Growth trend", "growth"),
        entry("Flash fill", "flash"),
    ]
}

/// Follow a link: a web address in the browser, a place in the workbook by choosing it.
fn open_link(book: &mut Book, target: &unluminous_sheet::model::LinkTarget, out: &mut GridOut) {
    match target {
        unluminous_sheet::model::LinkTarget::External(address) => {
            if address.starts_with("http://") || address.starts_with("https://") {
                // In a browser tab beside the workbook, through the command an agent would use.
                let mut arguments = serde_json::Map::new();
                arguments.insert("address".to_owned(), serde_json::json!(address));
                out.requests.push(Request::RunCommand {
                    id: "spreadsheet-link".to_owned(),
                    command: "browser.open".to_owned(),
                    arguments,
                });
            } else {
                out.requests.push(Request::Message(format!("This link goes to {address}")));
            }
        }
        unluminous_sheet::model::LinkTarget::Location(place) => {
            let place = place.trim_start_matches('#');
            match crate::services::spreadsheet::commands::parse_range(book, place) {
                Some((sheet, range)) => {
                    book.view.sheet = sheet;
                    book.view.select_range(range);
                }
                None => out
                    .requests
                    .push(Request::Message(format!("There is no {place} in this workbook"))),
            }
        }
    }
}

fn scroll_by(book: &mut Book, by: Vec2) {
    book.view.scroll = (book.view.scroll + by).max(Vec2::ZERO);
}

/// The filter button under a point, by its column.
fn filter_button_at(book: &Book, frame: Frame, p: Pos2) -> Option<u32> {
    let filter = book.session.book.sheets[book.view.sheet].filter.as_ref()?;
    let row = filter.range.first.row;
    (filter.range.first.col..=filter.range.last.col)
        .find(|col| filter_button(book, frame, CellRef::new(row, *col)).contains(p))
}

/// Where a filter button sits in its cell.
pub fn filter_button(book: &Book, frame: Frame, at: CellRef) -> Rect {
    let cell = frame.cell(book, at);
    let side = (16.0 * frame.zoom).min(cell.height() - 2.0).max(6.0);
    Rect::from_min_size(
        Pos2::new(cell.right() - side - 2.0, cell.bottom() - side - 2.0),
        Vec2::splat(side),
    )
}

/// The cells of one part of the sheet.
#[allow(clippy::too_many_arguments)]
fn draw_cells(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    rules: &Rules,
    frame: Frame,
    ink: &Ink,
    first_row: u32,
    last_row: u32,
    first_col: u32,
    last_col: u32,
    clip: Rect,
    formats: &mut HashMap<u32, Format>,
) {
    let painter = ui.painter_at(clip);
    let workbook = &book.session.book;
    let sheet = &workbook.sheets[book.view.sheet];
    let axes = book.view.axes.as_ref().expect("built");
    let zoom = frame.zoom;
    let now = book.session.now;
    // Gridlines, under everything.
    if sheet.show_grid {
        let stroke = Stroke::new(1.0, ink.grid);
        let top = frame.y(book, first_row).max(clip.top());
        let bottom = (frame.y(book, last_row) + axes.rows.size(last_row) * zoom).min(clip.bottom());
        let left = frame.x(book, first_col).max(clip.left());
        let right =
            (frame.x(book, last_col) + axes.columns.size(last_col) * zoom).min(clip.right());
        for col in first_col..=last_col {
            if axes.columns.is_hidden(col) {
                continue;
            }
            let x = (frame.x(book, col) + axes.columns.size(col) * zoom).round() - 0.5;
            painter.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], stroke);
        }
        for row in first_row..=last_row {
            if axes.rows.is_hidden(row) {
                continue;
            }
            let y = (frame.y(book, row) + axes.rows.size(row) * zoom).round() - 0.5;
            painter.line_segment([Pos2::new(left, y), Pos2::new(right, y)], stroke);
        }
    }
    // Merged ranges that cross this part, drawn whole.
    let view_range =
        Range::new(CellRef::new(first_row, first_col), CellRef::new(last_row, last_col));
    let merges: Vec<Range> =
        sheet.merges.iter().filter(|m| m.intersects(&view_range)).copied().collect();
    let in_merge = |at: CellRef| merges.iter().find(|m| m.contains(at)).copied();

    let mut texts: Vec<TextItem> = Vec::new();
    let mut lines: Vec<(Pos2, Pos2, Stroke)> = Vec::new();
    let mut doubles: Vec<(Pos2, Pos2, Stroke)> = Vec::new();
    let mut notes: Vec<Pos2> = Vec::new();
    let mut bars: Vec<(Rect, Color32)> = Vec::new();
    let mut bar_edges: Vec<(Rect, Color32)> = Vec::new();
    let mut icons: Vec<(Rect, unluminous_sheet::conditional::IconPick)> = Vec::new();
    let mut filter_cells: Vec<CellRef> = Vec::new();
    if let Some(filter) = &sheet.filter {
        for col in filter.range.first.col..=filter.range.last.col {
            filter_cells.push(CellRef::new(filter.range.first.row, col));
        }
    }
    // The cells to draw: every visible cell that has something, and the top left of each merge.
    let mut drawn_merges: Vec<Range> = Vec::new();
    let mut wanted: Vec<(CellRef, Rect)> = Vec::new();
    for row in first_row..=last_row {
        if axes.rows.is_hidden(row) {
            continue;
        }
        for col in first_col..=last_col {
            let at = CellRef::new(row, col);
            if let Some(m) = in_merge(at) {
                if !drawn_merges.contains(&m) {
                    drawn_merges.push(m);
                    wanted.push((m.first, frame.range(book, m)));
                }
                continue;
            }
            if axes.columns.is_hidden(col) {
                continue;
            }
            wanted.push((at, frame.cell(book, at)));
        }
    }
    let used = sheet.used_range();
    for (at, rect) in wanted {
        let style = display::style_at(sheet, at);
        let value = sheet.value_at(at);
        let effect =
            if rules.is_empty() { Effect::default() } else { rules.effect(workbook, now, at) };
        let merged = in_merge(at).is_some();
        if style == 0
            && value.is_empty()
            && effect.is_none()
            && !merged
            && !sheet.notes.contains_key(&(at.row, at.col))
        {
            continue;
        }
        let chars = display::chars_in(rect.width() / zoom);
        let code = effect.number_format.clone();
        let format = match &code {
            Some(code) => Format::parse(code),
            None => formats
                .entry(style)
                .or_insert_with(|| Format::parse(&workbook.styles.format_code(style)))
                .clone(),
        };
        let mut shown: Shown = display::shown_in(workbook, style, &value, chars, &format);
        display::adjust_for_the_view(workbook, sheet, at, &value, &mut shown);
        apply_effect(&mut shown, &effect);
        // The fill, over the gridlines. A merged cell with no fill is painted with the paper, which hides
        // the gridlines inside it.
        match shown.fill {
            Some(fill) => {
                painter.rect_filled(rect, 0, rgb(fill));
            }
            None if merged => {
                painter.rect_filled(rect.shrink2(Vec2::new(0.0, 0.0)), 0, ink.paper);
            }
            None => {}
        }
        if let Some(shape) = &effect.bar_shape {
            super::overlay_a::data_bar(rect, shape, &mut bars, &mut bar_edges);
        } else if let Some((fraction, color)) = effect.bar {
            let bar = Rect::from_min_size(
                rect.min + Vec2::new(1.0, 2.0),
                Vec2::new((rect.width() - 2.0) * fraction, rect.height() - 4.0),
            );
            bars.push((bar, rgb(color)));
        }
        if let Some(pick) = effect.icon.clone().filter(|p| !p.hidden) {
            icons.push((rect, pick));
        }
        // Borders.
        let edge = |line: Option<display::Line>,
                    a: Pos2,
                    b: Pos2,
                    lines: &mut Vec<(Pos2, Pos2, Stroke)>,
                    doubles: &mut Vec<(Pos2, Pos2, Stroke)>| {
            if let Some(line) = line {
                let color = line.color.map_or(ink.text, |c| ink.adapt(c, false));
                let stroke = Stroke::new(line.width.min(3.0), color);
                if line.double {
                    doubles.push((a, b, Stroke::new(1.0, color)));
                } else {
                    lines.push((a, b, stroke));
                }
            }
        };
        let r = rect;
        edge(shown.top, r.left_top(), r.right_top(), &mut lines, &mut doubles);
        edge(
            shown.bottom,
            r.left_bottom() - Vec2::new(0.0, 0.5),
            r.right_bottom() - Vec2::new(0.0, 0.5),
            &mut lines,
            &mut doubles,
        );
        edge(shown.left, r.left_top(), r.left_bottom(), &mut lines, &mut doubles);
        edge(
            shown.right,
            r.right_top() - Vec2::new(0.5, 0.0),
            r.right_bottom() - Vec2::new(0.5, 0.0),
            &mut lines,
            &mut doubles,
        );
        if sheet.notes.contains_key(&(at.row, at.col)) {
            notes.push(r.right_top());
        }
        if shown.text.is_empty() {
            continue;
        }
        // The text: its style, its colour, and where it may go.
        let size = (shown.size.max(1.0) * 4.0 / 3.0) * zoom;
        let style = CharStyle {
            family: shown.family.clone(),
            size,
            bold: shown.bold,
            italic: shown.italic,
            ..CharStyle::default()
        };
        let over_fill = shown.fill.is_some();
        let color = match shown.color {
            Some(c) => ink.adapt(c, over_fill),
            None => match shown.fill {
                // Automatic text on a fill is dark on a light fill and light on a dark one.
                Some(fill) if lightness(fill) > 140.0 => rgb(display::LIGHT_TEXT),
                Some(_) => rgb(display::LIGHT_PAPER),
                None => {
                    if shown.error && ink.dark {
                        look.palette.text_dim
                    } else {
                        ink.text
                    }
                }
            },
        };
        let pad = 3.0 * zoom + shown.indent as f32 * 9.0 * zoom;
        let inner = rect.shrink2(Vec2::new(pad.min(rect.width() / 3.0), 0.0));
        let filter_room = if filter_cells.contains(&at) { 18.0 * zoom } else { 0.0 };
        let available = (inner.width() - filter_room).max(0.0);
        let mut text = shown.text.clone();
        let mut width = width_of(look, &text, &style);
        if shown.numeric && width > available {
            // A number that does not fit: General shows fewer digits, every other format shows #.
            if format.is_general() {
                if let Value::Number(n) = value {
                    let mut chars = chars;
                    while chars > 1 && width > available {
                        chars -= 1;
                        text = unluminous_sheet::numfmt::general(n, chars);
                        width = width_of(look, &text, &style);
                    }
                }
            }
            if width > available {
                let hash = width_of(look, "#", &style).max(1.0);
                text = "#".repeat((available / hash).floor().max(1.0) as usize);
                width = width_of(look, &text, &style);
            }
        }
        if let Some((fill_char, at_char)) = shown.repeat {
            // `*` in a format repeats a character to fill the cell.
            let one = width_of(look, &fill_char.to_string(), &style).max(1.0);
            let count = ((available - width) / one).floor().max(0.0) as usize;
            let byte = text.char_indices().nth(at_char).map_or(text.len(), |(b, _)| b);
            text.insert_str(byte, &fill_char.to_string().repeat(count));
            width = width_of(look, &text, &style);
        }
        let metrics = look.renderer.line_metrics(&style);
        // Lines are set a font's ascent and descent apart; the gap a font asks for between paragraphs is
        // not used inside a cell, as Excel does not use it.
        let line_height = metrics.ascent + metrics.descent;
        let wrapped = if shown.wrap {
            wrap(look, &text, &style, available.max(1.0))
        } else {
            vec![text.replace('\n', " ")]
        };
        let widths: Vec<f32> = wrapped
            .iter()
            .map(|line| if wrapped.len() == 1 { width } else { width_of(look, line, &style) })
            .collect();
        // Where text that is wider than its cell may run: over empty neighbours, the way it is aligned.
        let mut clip_rect = rect;
        let longest = widths.iter().cloned().fold(0.0, f32::max);
        if !shown.wrap && !shown.numeric && longest > available && !merged {
            let free = |c: u32| {
                let n = CellRef::new(at.row, c);
                sheet.value_at(n).is_empty() && in_merge(n).is_none() && used.is_some_and(|_| true)
            };
            let need = longest - available;
            let mut extra_right = 0.0;
            let mut extra_left = 0.0;
            let (go_right, go_left) = match shown.across {
                Across::Left | Across::Fill | Across::Justify => (need, 0.0),
                Across::Right => (0.0, need),
                Across::Center | Across::CenterAcross => (need / 2.0, need / 2.0),
            };
            let mut c = at.col + 1;
            while extra_right < go_right && c < MAX_COLS && free(c) && c <= last_col + 1 {
                extra_right += axes.columns.size(c) * zoom;
                c += 1;
            }
            let mut c = at.col;
            while extra_left < go_left && c > 0 && free(c - 1) && c > first_col.saturating_sub(1) {
                extra_left += axes.columns.size(c - 1) * zoom;
                c -= 1;
            }
            if frame.rtl {
                std::mem::swap(&mut extra_left, &mut extra_right);
            }
            clip_rect = Rect::from_min_max(
                Pos2::new(rect.left() - extra_left, rect.top()),
                Pos2::new(rect.right() + extra_right, rect.bottom()),
            );
            // The gridlines the text runs over are hidden, as Excel hides them.
            if extra_right > 0.0 || extra_left > 0.0 {
                let cover = Rect::from_min_max(
                    Pos2::new(clip_rect.left() + 1.0, rect.top() + 0.5),
                    Pos2::new(clip_rect.right() - 1.0, rect.bottom() - 1.5),
                );
                let background = shown.fill.map_or(ink.paper, rgb);
                painter.rect_filled(cover, 0, background);
            }
        }
        let block = line_height * wrapped.len() as f32;
        let top = match shown.down {
            Down::Top => rect.top() + 1.0 * zoom,
            Down::Center => rect.center().y - block / 2.0,
            Down::Bottom => rect.bottom() - block - 1.5 * zoom,
        };
        let mut positions = Vec::new();
        // A right to left sheet mirrors the alignment: text that starts at the left starts at the right.
        let across = match (frame.rtl, shown.across) {
            (true, Across::Left | Across::Fill | Across::Justify) => Across::Right,
            (true, Across::Right) => Across::Left,
            (_, other) => other,
        };
        for (i, w) in widths.iter().enumerate() {
            let x = match across {
                Across::Left | Across::Fill | Across::Justify => inner.left(),
                Across::Right => inner.right() - filter_room - w,
                Across::Center | Across::CenterAcross => rect.center().x - w / 2.0,
            };
            positions.push(Pos2::new(x, top + metrics.ascent + line_height * i as f32));
        }
        texts.push(TextItem {
            lines: wrapped,
            style,
            color,
            clip: clip_rect.intersect(clip),
            at: positions,
            underline: shown.underline,
            strike: shown.strike,
            widths,
        });
    }
    for (bar, color) in bars {
        painter.rect_filled(bar, 1, color.gamma_multiply(0.75));
    }
    for (bar, color) in bar_edges {
        painter.rect_stroke(bar, 1, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    }
    for (rect, pick) in icons {
        super::overlay_a::icon(&painter, rect, &pick);
    }
    for (a, b, stroke) in lines {
        painter.line_segment([a, b], stroke);
    }
    for (a, b, stroke) in doubles {
        let along = (b - a).normalized();
        let across = Vec2::new(-along.y, along.x) * 1.0;
        painter.line_segment([a - across, b - across], stroke);
        painter.line_segment([a + across, b + across], stroke);
    }
    paint_texts(ui, look, &painter, &texts);
    for corner in notes {
        let s = 6.0 * zoom;
        painter.add(Shape::convex_polygon(
            vec![corner, corner + Vec2::new(0.0, s), corner - Vec2::new(s, 0.0)],
            look.palette.unsaved,
            Stroke::NONE,
        ));
    }
    for at in filter_cells {
        if at.row < first_row || at.row > last_row || at.col < first_col || at.col > last_col {
            continue;
        }
        let button = filter_button(book, frame, at);
        let active = sheet
            .filter
            .as_ref()
            .is_some_and(|f| f.columns.contains_key(&(at.col - f.range.first.col)));
        painter.rect(
            button,
            CornerRadius::same(3),
            look.palette.control,
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
        let c = button.center();
        let w = button.width() * 0.28;
        let tint = if active { look.palette.accent } else { look.palette.text_dim };
        painter.add(Shape::convex_polygon(
            vec![
                c + Vec2::new(-w, -w / 2.0),
                c + Vec2::new(w, -w / 2.0),
                c + Vec2::new(0.0, w / 1.5),
            ],
            tint,
            Stroke::NONE,
        ));
    }
}

/// Lay the conditional format's look over the cell's own.
fn apply_effect(shown: &mut Shown, effect: &Effect) {
    if let Some(fill) = effect.fill {
        shown.fill = Some(fill);
    }
    if let Some(color) = effect.color {
        shown.color = Some(color);
    }
    if let Some(b) = effect.bold {
        shown.bold = b;
    }
    if let Some(i) = effect.italic {
        shown.italic = i;
    }
    if let Some(s) = effect.strike {
        shown.strike = s;
    }
    if let Some(u) = effect.underline {
        shown.underline = u;
    }
}

/// Every cell's text as one mesh of glyphs from the renderer's atlas, each quad cut to its cell's clip.
fn paint_texts(ui: &egui::Ui, look: &Look<'_>, painter: &egui::Painter, texts: &[TextItem]) {
    let renderer = look.renderer;
    let mut placed: Vec<(Rect, Rect, Color32, Rect)> = Vec::new();
    let mut rules: Vec<(Rect, Color32)> = Vec::new();
    for _ in 0..3 {
        let generation = renderer.generation();
        placed.clear();
        rules.clear();
        for item in texts {
            for ((line, at), width) in item.lines.iter().zip(&item.at).zip(&item.widths) {
                let mut x = at.x;
                let mut buffer = [0u8; 4];
                for c in line.chars() {
                    let advance = renderer.advance(c.encode_utf8(&mut buffer), &item.style);
                    if let Some(glyph) = renderer.glyph(c, &item.style) {
                        let position = renderer
                            .crispness()
                            .snap(Pos2::new(x + glyph.offset.x, at.y + glyph.offset.y));
                        placed.push((
                            Rect::from_min_size(position, glyph.size),
                            glyph.uv,
                            item.color,
                            item.clip,
                        ));
                    }
                    x += advance;
                }
                let thickness = (item.style.size / 14.0).max(1.0);
                if item.underline {
                    let r = Rect::from_min_size(
                        Pos2::new(at.x, at.y + thickness * 1.5),
                        Vec2::new(*width, thickness),
                    )
                    .intersect(item.clip);
                    rules.push((r, item.color));
                }
                if item.strike {
                    let r = Rect::from_min_size(
                        Pos2::new(at.x, at.y - item.style.size * 0.3),
                        Vec2::new(*width, thickness),
                    )
                    .intersect(item.clip);
                    rules.push((r, item.color));
                }
            }
        }
        if renderer.generation() == generation {
            break;
        }
    }
    let texture = renderer.texture(ui.ctx());
    let mut mesh = Mesh::with_texture(texture);
    for (rect, uv, color, clip) in placed {
        // A glyph is cut to its cell's clip by moving the quad's edges and its texture coordinates together.
        let cut = rect.intersect(clip);
        if cut.width() <= 0.0 || cut.height() <= 0.0 {
            continue;
        }
        let fx = |x: f32| uv.left() + (x - rect.left()) / rect.width() * uv.width();
        let fy = |y: f32| uv.top() + (y - rect.top()) / rect.height() * uv.height();
        let cut_uv = Rect::from_min_max(
            Pos2::new(fx(cut.left()), fy(cut.top())),
            Pos2::new(fx(cut.right()), fy(cut.bottom())),
        );
        mesh.add_rect_with_uv(cut, cut_uv, color);
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
    for (rect, color) in rules {
        if rect.width() > 0.0 {
            painter.rect_filled(rect, 0, color);
        }
    }
}

/// The chosen cells: a wash over them, a line round the current range, the active cell left clear, the fill
/// handle, and the outline of what was copied.
fn draw_selection(ui: &egui::Ui, book: &Book, frame: Frame, ink: &Ink, look: &Look<'_>) {
    let painter = ui.painter_at(frame.cells);
    let active = frame.cell(book, book.view.cursor);
    let active = match book.session.book.sheets[book.view.sheet].merge_at(book.view.cursor) {
        Some(m) => frame.range(book, m),
        None => active,
    };
    for range in book.view.ranges() {
        let rect = frame.range(book, clip_whole(range));
        if range.len() > 1 {
            // The wash, drawn round the active cell rather than over it.
            for part in around(rect, active) {
                painter.rect_filled(part, 0, ink.wash);
            }
        }
    }
    let range = book.view.range();
    let rect = frame.range(book, clip_whole(range));
    painter.rect_stroke(
        rect,
        0,
        Stroke::new(2.0 * frame.zoom.max(0.75), ink.accent),
        egui::StrokeKind::Inside,
    );
    if book.view.extra.is_empty() {
        let handle =
            Rect::from_center_size(rect.right_bottom() - Vec2::splat(1.0), Vec2::splat(6.0));
        painter.rect_filled(handle.expand(1.0), 0, ink.paper);
        painter.rect_filled(handle, 0, ink.accent);
    }
    if let Some((sheet, copied, cut)) = book.view.marquee {
        if sheet == book.view.sheet {
            let r = frame.range(book, clip_whole(copied)).shrink(1.0);
            // Dashes that move with time, so the copied cells read as waiting for a paste.
            let phase = (ui.input(|i| i.time) * 8.0) as f32 % 8.0;
            let stroke = Stroke::new(1.5, if cut { look.palette.unsaved } else { ink.accent });
            let corners =
                [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            for pair in corners.windows(2) {
                let shapes = Shape::dashed_line_with_offset(
                    &[pair[0], pair[1]],
                    stroke,
                    &[4.0],
                    &[4.0],
                    phase,
                );
                painter.extend(shapes);
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}

/// A whole row or column chosen is drawn to the edge of the sheet; nothing else changes.
fn clip_whole(range: Range) -> Range {
    range
}

/// The parts of `rect` around `hole`.
fn around(rect: Rect, hole: Rect) -> Vec<Rect> {
    let hole = hole.intersect(rect);
    if hole.width() <= 0.0 || hole.height() <= 0.0 {
        return vec![rect];
    }
    let mut parts = Vec::new();
    if hole.top() > rect.top() {
        parts.push(Rect::from_min_max(rect.min, Pos2::new(rect.right(), hole.top())));
    }
    if hole.bottom() < rect.bottom() {
        parts.push(Rect::from_min_max(Pos2::new(rect.left(), hole.bottom()), rect.max));
    }
    if hole.left() > rect.left() {
        parts.push(Rect::from_min_max(
            Pos2::new(rect.left(), hole.top()),
            Pos2::new(hole.left(), hole.bottom()),
        ));
    }
    if hole.right() < rect.right() {
        parts.push(Rect::from_min_max(
            Pos2::new(hole.right(), hole.top()),
            Pos2::new(rect.right(), hole.bottom()),
        ));
    }
    parts
}

/// The column letters above a pane and the row numbers beside it, the chosen ones lit.
fn draw_headers(ui: &egui::Ui, book: &Book, pane: &Pane, ink: &Ink, look: &Look<'_>) {
    let frame = pane.frame;
    if frame.column_header.height() <= 0.0 {
        return;
    }
    let zoom = frame.zoom;
    let axes = book.view.axes.as_ref().expect("built");
    let (first_row, last_row, first_col, last_col) = frame.visible(book);
    let chosen_cols: Vec<(u32, u32)> =
        book.view.ranges().iter().map(|r| (r.first.col, r.last.col)).collect();
    let chosen_rows: Vec<(u32, u32)> =
        book.view.ranges().iter().map(|r| (r.first.row, r.last.row)).collect();
    let font = egui::FontId::proportional((11.0 * zoom).clamp(7.0, 20.0));
    let divider = Stroke::new(1.0, look.palette.divider);
    let col_painter = ui.painter_at(frame.column_header);
    col_painter.rect_filled(frame.column_header, 0, ink.header);
    for col in (0..frame.frozen.1).chain(first_col..=last_col) {
        let size = axes.columns.size(col) * zoom;
        if size <= 0.0 {
            continue;
        }
        let x = frame.x(book, col);
        let rect = Rect::from_min_size(
            Pos2::new(x, frame.column_header.top()),
            Vec2::new(size, frame.column_header.height()),
        );
        let lit = chosen_cols.iter().any(|(a, b)| *a <= col && col <= *b);
        if lit {
            col_painter.rect_filled(rect, 0, ink.header_chosen);
            col_painter.line_segment(
                [
                    rect.left_bottom() - Vec2::new(0.0, 1.0),
                    rect.right_bottom() - Vec2::new(0.0, 1.0),
                ],
                Stroke::new(2.0, ink.accent),
            );
        }
        col_painter.line_segment([rect.right_top(), rect.right_bottom()], divider);
        let text = if lit { look.palette.text_strong } else { ink.header_text };
        col_painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            column_name(col),
            font.clone(),
            text,
        );
    }
    let row_painter = ui.painter_at(frame.row_header);
    row_painter.rect_filled(frame.row_header, 0, ink.header);
    for row in (0..frame.frozen.0).chain(first_row..=last_row) {
        let size = axes.rows.size(row) * zoom;
        if size <= 0.0 {
            continue;
        }
        let y = frame.y(book, row);
        let rect = Rect::from_min_size(
            Pos2::new(frame.row_header.left(), y),
            Vec2::new(frame.row_header.width(), size),
        );
        let lit = chosen_rows.iter().any(|(a, b)| *a <= row && row <= *b);
        if lit {
            row_painter.rect_filled(rect, 0, ink.header_chosen);
            let edge = if frame.rtl { rect.left() + 1.0 } else { rect.right() - 1.0 };
            row_painter.line_segment(
                [Pos2::new(edge, rect.top()), Pos2::new(edge, rect.bottom())],
                Stroke::new(2.0, ink.accent),
            );
        }
        row_painter.line_segment([rect.left_bottom(), rect.right_bottom()], divider);
        let text = if lit { look.palette.text_strong } else { ink.header_text };
        row_painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            (row + 1).to_string(),
            font.clone(),
            text,
        );
    }
}

/// The corner where the two header strips meet, which chooses everything.
fn draw_corner(ui: &egui::Ui, layout: &Layout, ink: &Ink, look: &Look<'_>) {
    if !layout.headings {
        return;
    }
    let painter = ui.painter_at(layout.area);
    let corner = if layout.rtl {
        Rect::from_min_max(
            Pos2::new(layout.cells.right(), layout.area.top()),
            Pos2::new(layout.area.right(), layout.cells.top()),
        )
    } else {
        Rect::from_min_max(layout.area.min, layout.cells.min)
    };
    painter.rect_filled(corner, 0, ink.header);
    let c = if layout.rtl {
        Pos2::new(corner.left() + 4.0, corner.bottom() - 4.0)
    } else {
        corner.right_bottom() - Vec2::splat(4.0)
    };
    let s = (corner.height() * 0.45).min(10.0);
    let along = if layout.rtl { s } else { -s };
    painter.add(Shape::convex_polygon(
        vec![c, c + Vec2::new(along, 0.0), c - Vec2::new(0.0, s)],
        look.palette.text_faint,
        Stroke::NONE,
    ));
    let divider = Stroke::new(1.0, look.palette.divider);
    let a = layout.area;
    let c = layout.cells;
    painter.line_segment([Pos2::new(a.left(), c.top()), Pos2::new(a.right(), c.top())], divider);
    let x = if layout.rtl { c.right() } else { c.left() };
    painter.line_segment([Pos2::new(x, a.top()), Pos2::new(x, a.bottom())], divider);
}

/// Scroll so the active cell is in view, which a move with the keyboard asks for.
pub fn reveal(book: &mut Book, cells: Rect, zoom: f32) {
    let Some(axes) = book.view.axes.as_ref() else { return };
    let frozen = book.session.book.sheets[book.view.sheet].frozen.unwrap_or((0, 0));
    let at = book.view.cursor;
    let frozen_w = axes.columns.start(frozen.1);
    let frozen_h = axes.rows.start(frozen.0);
    let view_w = (cells.width() / zoom - frozen_w).max(10.0);
    let view_h = (cells.height() / zoom - frozen_h).max(10.0);
    if at.col >= frozen.1 {
        let left = axes.columns.start(at.col) - frozen_w;
        let right = axes.columns.end(at.col) - frozen_w;
        if left < book.view.scroll.x {
            book.view.scroll.x = left.max(0.0);
        } else if right > book.view.scroll.x + view_w {
            book.view.scroll.x = (right - view_w).max(0.0);
        }
    }
    if at.row >= frozen.0 {
        let top = axes.rows.start(at.row) - frozen_h;
        let bottom = axes.rows.end(at.row) - frozen_h;
        if top < book.view.scroll.y {
            book.view.scroll.y = top.max(0.0);
        } else if bottom > book.view.scroll.y + view_h {
            book.view.scroll.y = (bottom - view_h).max(0.0);
        }
    }
}
