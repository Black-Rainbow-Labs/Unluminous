//! Drawing a notebook: each code cell's outputs, the line under a cell saying how its last run went,
//! the buttons on a cell, the add buttons between cells, and the toolbar across the top.
//!
//! The cells themselves are not drawn here. A notebook is one document, and its text is drawn by the
//! editing area like any other file's (`tasks/task-2220-jupyter-notebooks-tdd.md` §1). What this file
//! draws goes in the room the layout leaves round each cell, which `app::notebook_frame` measures with
//! [`lay_out_outputs`] before the layout and paints with [`paint_outputs`] after it — so the two are
//! one function's answer and cannot disagree about how tall an output is.
//!
//! Outputs are set in egui's own monospaced font rather than through the editor's glyph atlas: they
//! are read rather than typed into, and an output can be a table or a picture, which the editor's
//! layout has no notion of.

use std::sync::Arc;

use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{Color32, CornerRadius, FontId, Galley, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_jupyter::nbformat::{Cell, CellKind};
use unluminous_jupyter::outputs::{self, Ansi, Shown, Span, Table};

use crate::components::controls::WithHint;
use crate::components::scrollbar;
use crate::theme::{color, icon};

/// The most lines an output is laid out with. A cell that prints a hundred thousand lines keeps all of
/// them in the file; laying them all out would stop the window for seconds on every width change.
pub const MOST_LINES: usize = 2000;

/// The most rows of a table drawn. The rest are counted in a line under it.
pub const MOST_ROWS: usize = 100;

/// How tall a cell's outputs may be, in lines of output text, before they scroll inside the cell.
/// The reference editor's own default is thirty percent of the screen; a count of lines does not change with the
/// window, which is what keeps a notebook's layout steady while the window is resized.
pub const TALLEST: f32 = 30.0;

/// The measurements of a notebook's furniture at one editor font size.
///
/// Everything scales with the editor's font, so zooming a notebook zooms its outputs and its gaps
/// with its code, which is `task-1771`'s rule that every pane zooms as one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub size: f32,
    /// Whether outputs taller than [`TALLEST`] lines scroll inside the cell, which is the
    /// `notebook.scroll_outputs` setting. Off, they are drawn at their full height.
    pub scroll: bool,
}

impl Metrics {
    /// How much larger or smaller than the default size the editor's font is, never below half.
    fn scale(&self) -> f32 {
        (self.size / 16.0).max(0.5)
    }
    /// Between two cells.
    pub fn gap(&self) -> f32 {
        (20.0 * self.scale()).round()
    }
    /// Inside a code cell's block, above its first line and below its last.
    pub fn pad(&self) -> f32 {
        (6.0 * self.scale()).round()
    }
    /// The line under a code cell that says how its last run went.
    pub fn status(&self) -> f32 {
        (20.0 * self.scale()).round()
    }
    /// The size outputs are set at.
    pub fn output_font(&self) -> FontId {
        FontId::monospace((self.size * 0.82).max(9.0))
    }
    /// How far outputs are set in from the cell's left edge.
    pub fn indent(&self) -> f32 {
        (10.0 * self.scale()).round()
    }
    /// The room after the last cell, which holds the buttons that add one.
    pub fn tail(&self) -> f32 {
        (56.0 * self.scale()).round()
    }
    /// How tall the toolbar across the top of a notebook is.
    pub fn toolbar(&self) -> f32 {
        34.0
    }
}

/// One cell's outputs, laid out at one width. Kept between frames by cell id.
pub struct Drawn {
    /// What the outputs were laid out from: the cell's output revision, the width, the font size,
    /// whether its traceback was open, and how its table was sorted. A change to any is a new layout.
    pub key: DrawnKey,
    pub blocks: Vec<Block>,
    /// The height of everything, which may be more than is shown.
    pub height: f32,
}

/// See [`Drawn::key`].
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnKey {
    pub revision: u64,
    pub width: f32,
    pub size: f32,
    pub traceback_open: bool,
    pub sort: Option<(usize, bool)>,
}

/// One output, laid out.
pub struct Block {
    /// From the top of the outputs, and how tall.
    pub y: f32,
    pub height: f32,
    pub body: Body,
    /// A wash behind it: stderr is red tinted, which is how the reference editor marks it.
    pub ground: Option<Color32>,
    /// HTML that can be opened in a browser tab, which draws it exactly.
    pub html: Option<String>,
    /// True for the line under an error that opens or closes its traceback.
    pub traceback_toggle: bool,
}

/// What a block draws.
pub enum Body {
    Text(Arc<Galley>),
    Picture { texture: egui::TextureHandle, size: Vec2 },
    Table(TableView),
}

/// A table laid out: its cells, how wide each column is, and how tall a row is.
pub struct TableView {
    pub header: Vec<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    pub index_columns: usize,
    pub columns: Vec<f32>,
    pub row: f32,
    /// The column it is sorted by and whether downwards, when it is.
    pub sort: Option<(usize, bool)>,
    /// The line under it: pandas' own count, and how many rows were left out here.
    pub footer: Option<String>,
}

/// What a press in a cell's outputs asked for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputOutcome {
    pub toggle_traceback: bool,
    pub open_html: Option<String>,
    pub sort_by: Option<usize>,
    pub pressed: bool,
}

/// The colour an ANSI code means, from the terminal's own palette, so a traceback is coloured exactly
/// as the same text would be in a terminal tab under the same theme.
pub fn ansi_colour(ansi: &Ansi) -> Color32 {
    let index = match ansi {
        Ansi::Black => 0,
        Ansi::Red => 1,
        Ansi::Green => 2,
        Ansi::Yellow => 3,
        Ansi::Blue => 4,
        Ansi::Magenta => 5,
        Ansi::Cyan => 6,
        Ansi::White => 7,
        Ansi::BrightBlack => 8,
        Ansi::BrightRed => 9,
        Ansi::BrightGreen => 10,
        Ansi::BrightYellow => 11,
        Ansi::BrightBlue => 12,
        Ansi::BrightMagenta => 13,
        Ansi::BrightCyan => 14,
        Ansi::BrightWhite => 15,
        Ansi::Indexed(index) => *index,
        // The kernel's own colour, written by the program rather than chosen here.
        Ansi::Rgb(r, g, b) => return Color32::from_rgb(*r, *g, *b),
    };
    let rgb = unluminous_terminal::Palette::current().indexed(index);
    Color32::from_rgb(rgb.r, rgb.g, rgb.b)
}

/// Lay a cell's outputs out at `width`.
pub fn lay_out_outputs(ctx: &egui::Context, cell: &Cell, key: DrawnKey, metrics: Metrics) -> Drawn {
    let mut blocks = Vec::new();
    let mut y = 0.0;
    let space = (4.0 * metrics.size / 16.0).round();
    for (at, output) in cell.outputs.iter().enumerate() {
        let shown = outputs::shown(output);
        let name = format!("notebook-output-{}-{at}", cell.id);
        for mut block in blocks_for(ctx, shown, &key, metrics, &name) {
            block.y = y;
            y += block.height + space;
            blocks.push(block);
        }
    }
    Drawn { key, blocks, height: (y - space).max(0.0) }
}

/// The blocks one output is drawn as: one, or two for an error with its traceback toggle.
fn blocks_for(
    ctx: &egui::Context,
    shown: Shown,
    key: &DrawnKey,
    metrics: Metrics,
    name: &str,
) -> Vec<Block> {
    let font = metrics.output_font();
    let width = key.width.max(40.0);
    match shown {
        Shown::Stream { stderr, text } => {
            let ground = stderr.then(|| color::failure().gamma_multiply(0.16));
            let tint = if stderr { color::failure() } else { color::text() };
            vec![text_block(ctx, &capped(&text), font, tint, width, ground)]
        }
        Shown::Text(text) | Shown::Json(text) | Shown::Latex(text) | Shown::Markdown(text) => {
            vec![text_block(ctx, &capped(&text), font, color::text(), width, None)]
        }
        Shown::Html(text) => {
            let mut block = text_block(ctx, &capped(&text), font, color::text(), width, None);
            block.html = Some(text);
            vec![block]
        }
        Shown::Svg(svg) => {
            let note = "An SVG picture. Open it in a browser tab to see it drawn.";
            let mut block = text_block(ctx, note, font, color::text_dim(), width, None);
            block.html = Some(svg);
            vec![block]
        }
        Shown::Png(bytes) | Shown::Jpeg(bytes) => {
            picture_block(ctx, &bytes, width, name).into_iter().collect()
        }
        Shown::Table(table) => vec![table_block(ctx, table, key.sort, font, width)],
        Shown::Error { ename, evalue, traceback } => {
            error_blocks(ctx, &ename, &evalue, &traceback, key.traceback_open, font, width)
        }
    }
}

/// Text cut to [`MOST_LINES`], saying how many lines were left out.
fn capped(text: &str) -> String {
    let lines = text.lines().count();
    if lines <= MOST_LINES {
        return text.trim_end_matches('\n').to_owned();
    }
    let kept: Vec<&str> = text.lines().take(MOST_LINES).collect();
    format!("{}\n... {} more lines, which the file keeps", kept.join("\n"), lines - MOST_LINES)
}

/// One block of wrapped text.
fn text_block(
    ctx: &egui::Context,
    text: &str,
    font: FontId,
    tint: Color32,
    width: f32,
    ground: Option<Color32>,
) -> Block {
    // Text with ANSI colour codes in it, which a page from `?name` and many programs' output have,
    // is drawn in those colours rather than showing the codes.
    let galley = match text.contains('\u{1b}') {
        true => {
            let lines: Vec<Vec<Span>> = text.lines().map(outputs::ansi_spans).collect();
            ctx.fonts_mut(|fonts| fonts.layout_job(coloured_job(&lines, font, tint, width)))
        }
        false => ctx.fonts_mut(|fonts| fonts.layout(text.to_owned(), font, tint, width)),
    };
    let height = galley.size().y;
    Block { y: 0.0, height, body: Body::Text(galley), ground, html: None, traceback_toggle: false }
}

/// A picture, at its own size in points unless that is wider than the cell.
fn picture_block(ctx: &egui::Context, bytes: &[u8], width: f32, name: &str) -> Option<Block> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let pixels = [decoded.width() as usize, decoded.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(pixels, decoded.as_raw());
    let texture =
        crate::services::picture::upload(ctx, name.to_owned(), image, egui::TextureOptions::LINEAR);
    // A matplotlib figure is written at 100 dots to the inch for a screen of 96, so its pixels are its
    // points; anything wider than the cell is shrunk to the cell, keeping its shape.
    let natural = Vec2::new(pixels[0] as f32, pixels[1] as f32);
    let fit = (width / natural.x).min(1.0);
    let size = natural * fit;
    Some(Block {
        y: 0.0,
        height: size.y,
        body: Body::Picture { texture, size },
        ground: None,
        html: None,
        traceback_toggle: false,
    })
}

/// A table: its rows, sorted when somebody asked, and each column as wide as its widest cell.
fn table_block(
    ctx: &egui::Context,
    table: Table,
    sort: Option<(usize, bool)>,
    font: FontId,
    width: f32,
) -> Block {
    let digit = ctx.fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
    let row = (font.size * 1.7).round();
    let order = match sort {
        Some((column, down)) => outputs::sort_rows(&table, column, down),
        None => (0..table.rows.len()).collect(),
    };
    let left_out = order.len().saturating_sub(MOST_ROWS);
    let rows: Vec<Vec<String>> =
        order.into_iter().take(MOST_ROWS).map(|at| table.rows[at].clone()).collect();
    let columns = column_widths(&table.header, &rows, digit, width);
    let footer = table_footer(&table, left_out);
    let header_rows = table.header.len().max(1) as f32;
    let height = row * (header_rows + rows.len() as f32) + if footer.is_some() { row } else { 0.0 };
    let view = TableView {
        header: table.header,
        rows,
        index_columns: table.index_columns,
        columns,
        row,
        sort,
        footer,
    };
    Block {
        y: 0.0,
        height,
        body: Body::Table(view),
        ground: None,
        html: None,
        traceback_toggle: false,
    }
}

/// How wide each column of a table is: its widest cell, at most forty characters, plus a margin.
fn column_widths(header: &[Vec<String>], rows: &[Vec<String>], digit: f32, width: f32) -> Vec<f32> {
    let count = header.iter().chain(rows).map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; count];
    for line in header.iter().chain(rows) {
        for (column, cell) in line.iter().enumerate() {
            widths[column] = widths[column].max(cell.chars().count().min(40));
        }
    }
    let columns: Vec<f32> =
        widths.into_iter().map(|chars| (chars.max(2) as f32 + 2.0) * digit).collect();
    // A table wider than the cell keeps its columns and is cut at the cell's edge, which is what a
    // terminal does with a wide frame; the file and the browser tab have all of it.
    let _ = width;
    columns
}

/// The line under a table: pandas' own count of rows and columns, and the rows not drawn here.
fn table_footer(table: &Table, left_out: usize) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(footer) = &table.footer {
        parts.push(footer.clone());
    }
    if left_out > 0 {
        parts.push(format!("{left_out} more rows are not drawn here"));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{00B7} "))
}

/// An error: its last line in red, and under it the traceback or a line that opens it.
fn error_blocks(
    ctx: &egui::Context,
    ename: &str,
    evalue: &str,
    traceback: &[Vec<Span>],
    open: bool,
    font: FontId,
    width: f32,
) -> Vec<Block> {
    let red = color::failure();
    let ground = Some(color::failure().gamma_multiply(0.1));
    let head = format!("{ename}: {evalue}");
    let mut blocks = vec![text_block(ctx, &head, font.clone(), red, width, ground)];
    if traceback.is_empty() {
        return blocks;
    }
    if open {
        let job = coloured_job(traceback, font.clone(), color::text(), width);
        let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        let height = galley.size().y;
        blocks.push(Block {
            y: 0.0,
            height,
            body: Body::Text(galley),
            ground,
            html: None,
            traceback_toggle: false,
        });
    }
    let label = match open {
        true => "Hide the traceback".to_owned(),
        false => format!("Show the traceback ({} lines)", traceback.len()),
    };
    let mut toggle = text_block(ctx, &label, font, color::accent(), width, None);
    toggle.traceback_toggle = true;
    blocks.push(toggle);
    blocks
}

/// Lines of ANSI coloured spans in their own colours, and `tint` where a span names none.
fn coloured_job(lines: &[Vec<Span>], font: FontId, tint: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    for (at, line) in lines.iter().enumerate() {
        if at > 0 {
            job.append("\n", 0.0, TextFormat::simple(font.clone(), tint));
        }
        for span in line {
            let tint = span.colour.as_ref().map(ansi_colour).unwrap_or(tint);
            let mut format = TextFormat::simple(font.clone(), tint);
            if let Some(background) = &span.background {
                format.background = ansi_colour(background).gamma_multiply(0.35);
            }
            job.append(&span.text, 0.0, format);
        }
    }
    job
}

/// How tall a cell's outputs are drawn: all of them, or [`TALLEST`] lines when they are taller and
/// scroll.
pub fn shown_height(drawn: &Drawn, metrics: Metrics) -> f32 {
    let line = metrics.output_font().size * 1.35;
    match metrics.scroll {
        true => drawn.height.min(line * TALLEST),
        false => drawn.height,
    }
}

/// Paint a cell's outputs with their top left at `at`, scrolled by `scroll`, and answer what was
/// pressed. `salt` keeps two cells' controls apart.
pub fn paint_outputs(
    ui: &mut egui::Ui,
    drawn: &Drawn,
    at: Pos2,
    width: f32,
    metrics: Metrics,
    scroll: &mut f32,
    salt: &str,
) -> OutputOutcome {
    let mut outcome = OutputOutcome::default();
    let shown = shown_height(drawn, metrics);
    let area = Rect::from_min_size(at, Vec2::new(width, shown));
    let clip = ui.clip_rect().intersect(area);
    let response = ui.interact(area, ui.id().with(("notebook-outputs", salt)), Sense::click());
    outcome.pressed = response.clicked();
    let overflow = (drawn.height - shown).max(0.0);
    let wheeled = scroll_with_the_wheel(ui, &response, scroll, overflow);
    let bar = scrollbar::Bar::new(area, *scroll, drawn.height, shown);
    let grabbed = bar.map(|bar| scrollbar::grab(ui, &bar, &bar_name(salt))).unwrap_or_default();
    if let Some(dragged_to) = grabbed.scroll {
        *scroll = dragged_to;
    }
    *scroll = scroll.clamp(0.0, overflow);
    let painter = ui.painter().with_clip_rect(clip);
    for (index, block) in drawn.blocks.iter().enumerate() {
        let top = at.y + block.y - *scroll;
        let rect = Rect::from_min_size(Pos2::new(at.x, top), Vec2::new(width, block.height));
        if rect.bottom() < clip.top() || rect.top() > clip.bottom() {
            continue;
        }
        paint_block(ui, &painter, block, rect, salt, index, &mut outcome);
    }
    if let Some(bar) = scrollbar::Bar::new(area, *scroll, drawn.height, shown) {
        scrollbar::paint(ui, &bar, &bar_name(salt), grabbed.active || wheeled);
    }
    outcome
}

/// The name the scrollbar of the outputs of the cell `salt` goes by, which is unique in the window.
fn bar_name(salt: &str) -> String {
    format!("notebook output {salt}")
}

/// Move `scroll` by the mouse wheel while the pointer is over a cell's outputs and they overflow, and
/// take the wheel's movement out of the frame's input so the page does not also scroll. Answers
/// whether the wheel moved the outputs.
fn scroll_with_the_wheel(
    ui: &mut egui::Ui,
    response: &egui::Response,
    scroll: &mut f32,
    overflow: f32,
) -> bool {
    if overflow <= 0.0 || !response.hovered() {
        return false;
    }
    let wheel = ui.input(|input| input.smooth_scroll_delta.y);
    if wheel == 0.0 {
        return false;
    }
    *scroll = (*scroll - wheel).clamp(0.0, overflow);
    ui.input_mut(|input| input.smooth_scroll_delta.y = 0.0);
    true
}

/// One block, and the one control it may carry.
fn paint_block(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    block: &Block,
    rect: Rect,
    salt: &str,
    index: usize,
    outcome: &mut OutputOutcome,
) {
    if let Some(ground) = block.ground {
        painter.rect_filled(rect.expand2(Vec2::new(4.0, 1.0)), CornerRadius::same(3), ground);
    }
    paint_block_body(ui, painter, &block.body, rect, salt, outcome);
    if block.traceback_toggle {
        outcome.toggle_traceback |= traceback_toggle_pressed(ui, &block.body, rect, salt, index);
    }
    if let Some(html) = &block.html {
        if open_in_browser_pressed(ui, rect) {
            outcome.open_html = Some(html.clone());
        }
    }
}

/// What a block is made of, drawn into `rect`. A table may report the column its header was pressed on.
fn paint_block_body(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    body: &Body,
    rect: Rect,
    salt: &str,
    outcome: &mut OutputOutcome,
) {
    match body {
        Body::Text(galley) => painter.galley(rect.min, galley.clone(), color::text()),
        Body::Picture { texture, size } => {
            let place = Rect::from_min_size(rect.min, *size);
            let whole = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            // any ground: a picture is drawn in its own colours, untinted.
            let untinted = Color32::WHITE;
            painter.image(texture.id(), place, whole, untinted);
        }
        Body::Table(view) => {
            if let Some(column) = paint_table(ui, painter, view, rect, salt) {
                outcome.sort_by = Some(column);
            }
        }
    }
}

/// The line that opens or closes an error's traceback, as a control over `rect`. Answers whether it
/// was clicked this frame.
fn traceback_toggle_pressed(
    ui: &mut egui::Ui,
    body: &Body,
    rect: Rect,
    salt: &str,
    index: usize,
) -> bool {
    let response = ui.interact(rect, ui.id().with(("traceback", salt, index)), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // Named by its words, so a screen reader and a test can find the control by what it says.
    let words = match body {
        Body::Text(galley) => galley.text().to_owned(),
        _ => String::new(),
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &words));
    response.clicked()
}

/// The small button at the top right of a block whose output can be opened in a browser tab. Answers
/// whether it was pressed.
fn open_in_browser_pressed(ui: &mut egui::Ui, rect: Rect) -> bool {
    let button = Rect::from_min_size(Pos2::new(rect.right() - 22.0, rect.top()), Vec2::splat(20.0));
    crate::components::controls::icon_button(
        ui,
        button,
        "Open the output in a browser tab",
        icon::whole_page,
    )
}

/// A table, with its header rows over a rule and its index columns dimmed, as pandas draws one.
fn paint_table(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    view: &TableView,
    rect: Rect,
    salt: &str,
) -> Option<usize> {
    let font = FontId::monospace(view.row / 1.7);
    let mut sort = None;
    let mut y = rect.top();
    for (line, cells) in view.header.iter().enumerate() {
        let last = line + 1 == view.header.len();
        if let Some(column) = paint_table_row(
            ui,
            painter,
            view,
            cells,
            Pos2::new(rect.left(), y),
            &font,
            Some((salt, last)),
        ) {
            sort = Some(column);
        }
        y += view.row;
    }
    let total: f32 = view.columns.iter().sum();
    painter.hline(rect.left()..=rect.left() + total, y, Stroke::new(1.0, color::divider()));
    for (at, cells) in view.rows.iter().enumerate() {
        if at % 2 == 1 {
            let band = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(total, view.row));
            painter.rect_filled(band, CornerRadius::ZERO, color::code_panel());
        }
        paint_table_row(ui, painter, view, cells, Pos2::new(rect.left(), y), &font, None);
        y += view.row;
    }
    if let Some(footer) = &view.footer {
        painter.text(
            Pos2::new(rect.left(), y + view.row / 2.0),
            egui::Align2::LEFT_CENTER,
            footer,
            font,
            color::text_dim(),
        );
    }
    sort
}

/// One row of a table. A header row's cells are buttons that sort by their column.
fn paint_table_row(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    view: &TableView,
    cells: &[String],
    at: Pos2,
    font: &FontId,
    header: Option<(&str, bool)>,
) -> Option<usize> {
    let mut x = at.x;
    let mut pressed = None;
    for (column, width) in view.columns.iter().enumerate() {
        let cell = cells.get(column).map(String::as_str).unwrap_or("");
        let shown = crate::components::controls::truncate_chars(cell, 40, 38);
        let place = Rect::from_min_size(Pos2::new(x, at.y), Vec2::new(*width, view.row));
        let numeric = cell.trim().replace(',', "").parse::<f64>().is_ok();
        let dim = column < view.index_columns || header.is_some();
        let tint = if dim { color::text_dim() } else { color::text() };
        let (anchor, point) = match numeric && header.is_none() {
            true => (egui::Align2::RIGHT_CENTER, Pos2::new(place.right() - 6.0, place.center().y)),
            false => (egui::Align2::LEFT_CENTER, Pos2::new(place.left() + 6.0, place.center().y)),
        };
        painter.text(point, anchor, &shown, font.clone(), tint);
        if let Some((salt, true)) = header {
            if column >= view.index_columns {
                let response =
                    ui.interact(place, ui.id().with(("sort", salt, column)), Sense::click());
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if response.clicked() {
                    pressed = Some(column);
                }
                if let Some((sorted, down)) = view.sort.filter(|(sorted, _)| *sorted == column) {
                    let _ = sorted;
                    let mark = Pos2::new(place.right() - 6.0, place.center().y);
                    match down {
                        true => icon::chevron_down(painter, mark, color::accent()),
                        false => icon::chevron_up(painter, mark, color::accent()),
                    }
                }
            }
        }
        x += width;
    }
    pressed
}

/// How a code cell's last run went, as the mark at the left of its status line. The application works
/// this out from the cell's run, so this file does not need to know what a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusMark {
    /// No run to show a mark for.
    Nothing,
    /// The last run finished without an error.
    Succeeded,
    /// The last run raised an error.
    Failed,
    /// Waiting behind another cell.
    Queued,
    /// Running now.
    Running,
    /// Not run, because an earlier cell failed or the run was stopped.
    Skipped,
}

/// The line under a code cell: `mark` for how its last run went, then `words`, set at the notebook's
/// font `size`.
pub fn paint_status(painter: &egui::Painter, rect: Rect, mark: StatusMark, words: &str, size: f32) {
    let centre = Pos2::new(rect.left() + 8.0, rect.center().y);
    match mark {
        StatusMark::Succeeded => icon::tick(painter, centre, color::git_added()),
        StatusMark::Failed => icon::cross_at(painter, centre, color::failure(), 0.8),
        StatusMark::Queued => icon::clock(painter, centre, color::text_dim()),
        StatusMark::Running => {
            paint_spinner(painter, centre, painter.ctx().input(|input| input.time))
        }
        StatusMark::Skipped => icon::stop(painter, centre, color::text_faint()),
        StatusMark::Nothing => {}
    }
    let font = FontId::monospace((size * 0.72).max(9.0));
    painter.text(
        Pos2::new(rect.left() + 20.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        words,
        font,
        color::text_dim(),
    );
}

/// A turning arc, for a cell that is running.
fn paint_spinner(painter: &egui::Painter, centre: Pos2, time: f64) {
    let start = (time * 4.0) as f32;
    let points: Vec<Pos2> = (0..12)
        .map(|step| {
            let angle = start + step as f32 * 0.4;
            centre + Vec2::new(angle.cos(), angle.sin()) * 5.0
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.6, color::accent())));
    painter.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

/// A drawn icon: what every button here is handed to paint its mark.
pub type Draw = fn(&egui::Painter, Pos2, Color32);

/// What a button on a cell, or between two cells, asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellButton {
    Run,
    Debug,
    MoveUp,
    MoveDown,
    Delete,
    More,
    /// Render a Markdown cell that is being edited, or open a rendered one.
    ToggleMarkdown,
    /// Add a cell of this kind, below the cell the buttons belong to.
    Add(CellKind),
}

/// The buttons over a cell's top right corner, shown on the cell under the pointer and on the chosen
/// one: the reference editor's own place for them.
pub fn cell_buttons(
    ui: &mut egui::Ui,
    top_right: Pos2,
    kind: CellKind,
    rendered: bool,
    salt: &str,
) -> Option<CellButton> {
    let size = 22.0;
    let mut buttons: Vec<(CellButton, &str, Draw)> = Vec::new();
    match kind {
        CellKind::Code => {
            buttons.push((CellButton::Run, "Run Cell", icon::run));
            buttons.push((CellButton::Debug, "Debug Cell", icon::bug));
        }
        CellKind::Markdown => match rendered {
            true => buttons.push((CellButton::ToggleMarkdown, "Edit Markdown Cell", icon::font)),
            false => buttons.push((CellButton::ToggleMarkdown, "Render Markdown Cell", icon::tick)),
        },
        CellKind::Raw => {}
    }
    buttons.push((CellButton::MoveUp, "Move Cell Up", icon::chevron_up));
    buttons.push((CellButton::MoveDown, "Move Cell Down", icon::chevron_down));
    buttons.push((CellButton::Delete, "Delete Cell", icon::bin));
    buttons.push((CellButton::More, "More Cell Actions", icon::more));
    let width = size * buttons.len() as f32 + 4.0;
    let strip = Rect::from_min_size(
        Pos2::new(top_right.x - width, top_right.y),
        Vec2::new(width, size + 4.0),
    );
    ui.painter().rect_filled(strip, CornerRadius::same(5), color::menu());
    ui.painter().rect_stroke(
        strip,
        CornerRadius::same(5),
        Stroke::new(1.0, color::control_border()),
        egui::StrokeKind::Inside,
    );
    let mut chosen = None;
    for (at, (button, name, draw)) in buttons.into_iter().enumerate() {
        let place = Rect::from_min_size(
            Pos2::new(strip.left() + 2.0 + at as f32 * size, strip.top() + 2.0),
            Vec2::splat(size),
        );
        let name = format!("{name} {salt}");
        if crate::components::controls::icon_button(ui, place, &name, draw) {
            chosen = Some(button);
        }
    }
    chosen
}

/// The `+ Code` and `+ Markdown` buttons, centred on `centre`.
pub fn add_buttons(ui: &mut egui::Ui, centre: Pos2, salt: &str) -> Option<CellButton> {
    let font = FontId::proportional(12.0);
    let labels = [("+ Code", CellKind::Code), ("+ Markdown", CellKind::Markdown)];
    let widths: Vec<f32> =
        labels.iter().map(|(label, _)| label.len() as f32 * 7.0 + 18.0).collect();
    let total: f32 = widths.iter().sum::<f32>() + 6.0;
    let mut x = centre.x - total / 2.0;
    let mut chosen = None;
    for ((label, kind), width) in labels.into_iter().zip(widths) {
        let place =
            Rect::from_center_size(Pos2::new(x + width / 2.0, centre.y), Vec2::new(width, 22.0));
        let response = ui
            .interact(place, ui.id().with(("add-cell", salt, label)), Sense::click())
            .with_hint(label);
        let ground = if response.hovered() { color::control_hover() } else { color::control() };
        ui.painter().rect_filled(place, CornerRadius::same(11), ground);
        ui.painter().text(
            place.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            color::text_control(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Add {label} cell {salt}"),
            )
        });
        if response.clicked() {
            chosen = Some(CellButton::Add(kind));
        }
        x += width + 6.0;
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_output_is_cut_and_says_how_much_was_left_out() {
        let text: String = (0..MOST_LINES + 5).map(|line| format!("{line}\n")).collect();
        let cut = capped(&text);
        assert_eq!(cut.lines().count(), MOST_LINES + 1);
        assert!(cut.ends_with("5 more lines, which the file keeps"));
    }
}
