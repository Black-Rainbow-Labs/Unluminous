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
    /// The cells' area, under the column header and right of the row header.
    pub cells: Rect,
    pub zoom: f32,
    pub frozen: (u32, u32),
    pub scroll: Vec2,
}

impl Frame {
    /// The left edge of a column on the screen.
    pub fn x(&self, book: &Book, col: u32) -> f32 {
        let axes = book.view.axes.as_ref().expect("axes are built before drawing");
        let start = axes.columns.start(col);
        let shift = if col >= self.frozen.1 { self.scroll.x } else { 0.0 };
        self.cells.left() + (start - shift) * self.zoom
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
        let local = (p - self.cells.min) / self.zoom;
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
    let sheet_index = book.view.sheet;
    let frozen = book.session.book.sheets[sheet_index].frozen.unwrap_or((0, 0));
    let cells = Rect::from_min_max(
        Pos2::new(area.left() + ROW_HEADER * zoom, area.top() + COLUMN_HEADER * zoom),
        area.max,
    );
    let frame = Frame { cells, zoom, frozen, scroll: book.view.scroll };
    book.view.drawn = Some(cells);
    out.frame = Some(frame);
    let painter = ui.painter_at(area);

    // The pointer first, so a press this frame is drawn this frame.
    let response =
        ui.interact(area, ui.id().with(("spreadsheet-grid", &book.key)), Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Spreadsheet grid")
    });
    read_the_wheel(ui, &response, book, frame, zoom);
    let frame = Frame { scroll: book.view.scroll, ..frame };
    read_the_pointer(ui, &response, book, frame, area, pointing, out);
    let frame = Frame { scroll: book.view.scroll, ..frame };
    out.frame = Some(frame);

    painter.rect_filled(cells, 0, ink.paper);
    let (first_row, last_row, first_col, last_col) = frame.visible(book);
    // The four parts of a frozen sheet: the corner that never moves, the rows along the top, the columns
    // down the left, and the scrolled part, each clipped to itself.
    let frozen_w = book.view.axes.as_ref().map_or(0.0, |a| a.columns.start(frozen.1)) * zoom;
    let frozen_h = book.view.axes.as_ref().map_or(0.0, |a| a.rows.start(frozen.0)) * zoom;
    let parts = [
        (0..frozen.0, 0..frozen.1, Rect::from_min_size(cells.min, Vec2::new(frozen_w, frozen_h))),
        (
            0..frozen.0,
            first_col..last_col + 1,
            Rect::from_min_max(
                Pos2::new(cells.left() + frozen_w, cells.top()),
                Pos2::new(cells.right(), cells.top() + frozen_h),
            ),
        ),
        (
            first_row..last_row + 1,
            0..frozen.1,
            Rect::from_min_max(
                Pos2::new(cells.left(), cells.top() + frozen_h),
                Pos2::new(cells.left() + frozen_w, cells.bottom()),
            ),
        ),
        (
            first_row..last_row + 1,
            first_col..last_col + 1,
            Rect::from_min_max(
                Pos2::new(cells.left() + frozen_w, cells.top() + frozen_h),
                cells.max,
            ),
        ),
    ];
    let mut formats: HashMap<u32, Format> = HashMap::new();
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
            &ink,
            rows.start,
            rows.end - 1,
            cols.start,
            cols.end - 1,
            clip,
            &mut formats,
        );
    }
    // The line under the frozen rows and right of the frozen columns.
    if frozen.0 > 0 {
        painter.line_segment(
            [
                Pos2::new(cells.left(), cells.top() + frozen_h),
                Pos2::new(cells.right(), cells.top() + frozen_h),
            ],
            Stroke::new(1.5, look.palette.text_faint),
        );
    }
    if frozen.1 > 0 {
        painter.line_segment(
            [
                Pos2::new(cells.left() + frozen_w, cells.top()),
                Pos2::new(cells.left() + frozen_w, cells.bottom()),
            ],
            Stroke::new(1.5, look.palette.text_faint),
        );
    }
    draw_selection(ui, book, frame, &ink, look);
    draw_headers(ui, area, book, frame, &ink, look, first_row, last_row, first_col, last_col);
}

/// The wheel scrolls; with Shift it scrolls across; with Ctrl it zooms.
fn read_the_wheel(
    ui: &egui::Ui,
    response: &egui::Response,
    book: &mut Book,
    frame: Frame,
    zoom: f32,
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
    book.view.scroll.x = (book.view.scroll.x - delta.x / zoom).clamp(0.0, max_x);
    book.view.scroll.y = (book.view.scroll.y - delta.y / zoom).clamp(0.0, max_y);
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
    let in_columns = p.y < frame.cells.top() && p.x >= frame.cells.left();
    let in_rows = p.x < frame.cells.left() && p.y >= frame.cells.top();
    let at = frame.at(book, p);
    let axes = book.view.axes.clone().expect("built");
    // The edge of a header under the pointer, for the resize cursor and the drag.
    let column_edge = in_columns
        .then(|| {
            let col = at.col;
            let right = frame.x(book, col) + axes.columns.size(col) * frame.zoom;
            let left = frame.x(book, col);
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
    if response.hovered() {
        if column_edge.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        } else if row_edge.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
        } else if on_handle {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if frame.cells.contains(p) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Cell);
        }
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
            book.view.drag = Some(Drag::Fill);
        } else if p.x < frame.cells.left() && p.y < frame.cells.top() {
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
            Some(Drag::Fill) => scroll_by(book, push / frame.zoom),
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
            Some(Drag::Fill) => {
                let target = frame.at(book, p.max(frame.cells.min));
                let r = book.view.range();
                // The fill goes the way the pointer went furthest: down or across.
                let down = target.row as i64 - r.last.row as i64;
                let up = r.first.row as i64 - target.row as i64;
                let right = target.col as i64 - r.last.col as i64;
                let left = r.first.col as i64 - target.col as i64;
                let most = down.max(up).max(right).max(left);
                if most > 0 {
                    let to = if most == down {
                        Range::new(r.first, CellRef::new(target.row, r.last.col))
                    } else if most == right {
                        Range::new(r.first, CellRef::new(r.last.row, target.col))
                    } else if most == up {
                        Range::new(CellRef::new(target.row, r.first.col), r.last)
                    } else {
                        Range::new(CellRef::new(r.first.row, target.col), r.last)
                    };
                    out.acts.push(Act::FillTo(to));
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
            Some(Drag::ColumnEdge(..)) | Some(Drag::RowEdge(..)) | Some(Drag::Fill)
        )
    {
        book.view.drag = None;
    }
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
        if let Some((fraction, color)) = effect.bar {
            let bar = Rect::from_min_size(
                rect.min + Vec2::new(1.0, 2.0),
                Vec2::new((rect.width() - 2.0) * fraction, rect.height() - 4.0),
            );
            bars.push((bar, rgb(color)));
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
        for (i, w) in widths.iter().enumerate() {
            let x = match shown.across {
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

/// The column letters along the top and the row numbers down the left, the chosen ones lit.
#[allow(clippy::too_many_arguments)]
fn draw_headers(
    ui: &egui::Ui,
    area: Rect,
    book: &Book,
    frame: Frame,
    ink: &Ink,
    look: &Look<'_>,
    first_row: u32,
    last_row: u32,
    first_col: u32,
    last_col: u32,
) {
    let painter = ui.painter_at(area);
    let zoom = frame.zoom;
    let top_strip = Rect::from_min_max(
        Pos2::new(area.left(), area.top()),
        Pos2::new(area.right(), frame.cells.top()),
    );
    let left_strip = Rect::from_min_max(
        Pos2::new(area.left(), frame.cells.top()),
        Pos2::new(frame.cells.left(), area.bottom()),
    );
    painter.rect_filled(top_strip, 0, ink.header);
    painter.rect_filled(left_strip, 0, ink.header);
    let axes = book.view.axes.as_ref().expect("built");
    let chosen_cols: Vec<(u32, u32)> =
        book.view.ranges().iter().map(|r| (r.first.col, r.last.col)).collect();
    let chosen_rows: Vec<(u32, u32)> =
        book.view.ranges().iter().map(|r| (r.first.row, r.last.row)).collect();
    let font = egui::FontId::proportional((11.0 * zoom).clamp(7.0, 20.0));
    let divider = Stroke::new(1.0, look.palette.divider);
    let columns = (0..frame.frozen.1).chain(first_col..=last_col);
    let col_painter = ui.painter_at(Rect::from_min_max(
        Pos2::new(frame.cells.left(), area.top()),
        Pos2::new(area.right(), frame.cells.top()),
    ));
    for col in columns {
        let size = axes.columns.size(col) * zoom;
        if size <= 0.0 {
            continue;
        }
        let x = frame.x(book, col);
        let rect =
            Rect::from_min_size(Pos2::new(x, top_strip.top()), Vec2::new(size, top_strip.height()));
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
        col_painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            column_name(col),
            font.clone(),
            if lit { look.palette.text_strong } else { ink.header_text },
        );
    }
    let rows = (0..frame.frozen.0).chain(first_row..=last_row);
    let row_painter = ui.painter_at(Rect::from_min_max(
        Pos2::new(area.left(), frame.cells.top()),
        Pos2::new(frame.cells.left(), area.bottom()),
    ));
    for row in rows {
        let size = axes.rows.size(row) * zoom;
        if size <= 0.0 {
            continue;
        }
        let y = frame.y(book, row);
        let rect = Rect::from_min_size(
            Pos2::new(left_strip.left(), y),
            Vec2::new(left_strip.width(), size),
        );
        let lit = chosen_rows.iter().any(|(a, b)| *a <= row && row <= *b);
        if lit {
            row_painter.rect_filled(rect, 0, ink.header_chosen);
            row_painter.line_segment(
                [rect.right_top() - Vec2::new(1.0, 0.0), rect.right_bottom() - Vec2::new(1.0, 0.0)],
                Stroke::new(2.0, ink.accent),
            );
        }
        row_painter.line_segment([rect.left_bottom(), rect.right_bottom()], divider);
        row_painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            (row + 1).to_string(),
            font.clone(),
            if lit { look.palette.text_strong } else { ink.header_text },
        );
    }
    // The corner, which chooses everything.
    let corner = Rect::from_min_max(area.min, frame.cells.min);
    painter.rect_filled(corner, 0, ink.header);
    let c = corner.right_bottom() - Vec2::splat(4.0);
    let s = (corner.height() * 0.45).min(10.0);
    painter.add(Shape::convex_polygon(
        vec![c, c - Vec2::new(s, 0.0), c - Vec2::new(0.0, s)],
        look.palette.text_faint,
        Stroke::NONE,
    ));
    painter.line_segment(
        [Pos2::new(area.left(), frame.cells.top()), Pos2::new(area.right(), frame.cells.top())],
        divider,
    );
    painter.line_segment(
        [Pos2::new(frame.cells.left(), area.top()), Pos2::new(frame.cells.left(), area.bottom())],
        divider,
    );
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
