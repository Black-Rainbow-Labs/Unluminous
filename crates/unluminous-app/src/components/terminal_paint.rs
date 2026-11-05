//! The parts of drawing a terminal grid that go beyond a letter in a cell (`task-2514`).
//!
//! `task-2489` put Termoil's emulator behind every terminal and kept Unluminous's own painter, and the painter drew
//! less than the emulator knew: every underline was one plain line, an overline was nothing, a picture a program
//! sent left an empty space, an emoji was an empty box, a combining accent sat beside its letter, and a box drawn
//! with line characters had gaps between its rows. Each of those is drawn here:
//!
//! - [`paint_glyphs`] draws the letters from the glyph atlas, the box drawing and block characters as shapes that
//!   fill the cell (`unluminous_terminal::sprite`, which is Termoil's geometry), a combining mark centered over the
//!   letter it belongs to, and an emoji in color from `services::color_glyphs`.
//! - [`paint_lines`] draws the five underline styles in the underline's own color, the overline, the
//!   strikethrough, and an underline under the `OSC 8` link the pointer is over.
//! - [`paint_images`] draws the kitty, sixel and iTerm2 pictures the emulator holds, under or over the text by
//!   their `z`.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, Mesh, Pos2, Rect, Shape, Stroke, Vec2};
use unluminous_terminal::screen::{ImagePixels, Screen, ScreenCell, UnderlineStyle};

use crate::services::color_glyphs::is_emoji;
use crate::services::text_renderer::{CellMetrics, TextRenderer};

/// The tint that leaves a texture's own colors alone, for an emoji and a picture.
const UNTINTED: Color32 = Color32::WHITE; // any ground: it multiplies the texture's colors by one

/// Where the grid's first cell is and how large a cell is, which every pass measures from.
#[derive(Clone, Copy)]
pub(crate) struct Grid {
    pub origin: Pos2,
    pub cell: CellMetrics,
}

impl Grid {
    /// The top left corner of the cell at `row` and `column`.
    pub fn at(&self, row: usize, column: usize) -> Pos2 {
        Pos2::new(
            self.origin.x + column as f32 * self.cell.width,
            self.origin.y + row as f32 * self.cell.height,
        )
    }

    /// The rectangle of `span` cells starting at `row` and `column`.
    pub fn cells(&self, row: usize, column: usize, span: usize) -> Rect {
        Rect::from_min_max(
            self.at(row, column),
            Pos2::new(self.at(row, column + span).x, self.at(row, column).y + self.cell.height),
        )
    }
}

/// How many columns a cell's character covers.
fn span_of(cell: &ScreenCell) -> usize {
    if cell.wide {
        2
    } else {
        1
    }
}

/// A mark that is not drawn: the variation selectors and the joiner, which say how the characters around them are
/// drawn rather than being drawn themselves.
fn is_invisible_mark(mark: char) -> bool {
    matches!(mark as u32, 0xFE00..=0xFE0F | 0x200D | 0xE0100..=0xE01EF)
}

/// The glyph quads of a grid, collected before either atlas texture is fetched.
#[derive(Default)]
struct Quads {
    /// From the coverage atlas, tinted by the color.
    tinted: Vec<(Rect, Rect, Color32)>,
    /// From the color atlas, drawn as they are.
    colored: Vec<(Rect, Rect)>,
}

/// Draw every character on the screen.
///
/// Collected first and drawn after, and the whole collection made again if collecting filled either atlas and
/// cleared it, for the reason `editor_view::paint_text` records.
pub(crate) fn paint_glyphs(
    ui: &egui::Ui,
    renderer: &TextRenderer,
    screen: &Screen,
    grid: Grid,
    font_size: f32,
) {
    let mut quads = Quads::default();
    for _ in 0..3 {
        let generation = (renderer.generation(), renderer.color_generation());
        quads = Quads::default();
        for row in 0..screen.rows {
            for column in 0..screen.columns {
                let Some(cell) = screen.cell(row, column) else { continue };
                if cell.is_blank() {
                    continue;
                }
                let colour =
                    Color32::from_rgb(cell.foreground.r, cell.foreground.g, cell.foreground.b);
                collect_cell(renderer, cell, grid, (row, column), font_size, colour, &mut quads);
            }
        }
        if (renderer.generation(), renderer.color_generation()) == generation {
            break;
        }
    }
    draw_quads(ui, renderer, quads);
}

/// Draw one character again in `colour`, which is how the letter under a solid block cursor stays readable.
pub(crate) fn paint_one(
    ui: &egui::Ui,
    renderer: &TextRenderer,
    cell: &ScreenCell,
    grid: Grid,
    at: (usize, usize),
    font_size: f32,
    colour: Color32,
) {
    let mut quads = Quads::default();
    collect_cell(renderer, cell, grid, at, font_size, colour, &mut quads);
    draw_quads(ui, renderer, quads);
}

/// Hand the collected quads to the painter, one mesh for each atlas.
fn draw_quads(ui: &egui::Ui, renderer: &TextRenderer, quads: Quads) {
    if !quads.tinted.is_empty() {
        let mut mesh = Mesh::with_texture(renderer.texture(ui.ctx()));
        for (rect, uv, colour) in quads.tinted {
            mesh.add_rect_with_uv(rect, uv, colour);
        }
        ui.painter().add(Shape::mesh(mesh));
    }
    if !quads.colored.is_empty() {
        let mut mesh = Mesh::with_texture(renderer.color_texture(ui.ctx()));
        for (rect, uv) in quads.colored {
            mesh.add_rect_with_uv(rect, uv, UNTINTED);
        }
        ui.painter().add(Shape::mesh(mesh));
    }
}

/// The quads for one cell: an emoji, a sprite, or a letter and its marks.
fn collect_cell(
    renderer: &TextRenderer,
    cell: &ScreenCell,
    grid: Grid,
    (row, column): (usize, usize),
    font_size: f32,
    colour: Color32,
    quads: &mut Quads,
) {
    let crispness = renderer.crispness();
    let span = span_of(cell);
    // Snapped corner by corner, so two neighboring cells share an edge exactly and a box has no seam in it.
    let rect = grid.cells(row, column, span);
    let snapped = Rect::from_min_max(crispness.snap(rect.min), crispness.snap(rect.max));
    let pixels = |points: f32| crispness.raster_size(points).round().max(1.0) as u32;
    if is_emoji(cell.character, &cell.marks) {
        let text: String =
            std::iter::once(cell.character).chain(cell.marks.iter().copied()).collect();
        if let Some(uv) =
            renderer.color_glyph(&text, pixels(snapped.width()), pixels(snapped.height()))
        {
            quads.colored.push((snapped, uv));
            return;
        }
    }
    if let Some(sprite) =
        renderer.sprite_glyph(cell.character, pixels(snapped.width()), pixels(snapped.height()))
    {
        quads.tinted.push((snapped, sprite.uv, colour));
        return;
    }
    let style = renderer.terminal_style(font_size, cell.bold, cell.italic);
    let pen = grid.at(row, column);
    if let Some(glyph) = renderer.glyph(cell.character, &style) {
        // Snapped to whole pixels: a glyph drawn on a fraction of a pixel is resampled, which softens every letter
        // in the grid. Whole pixels of the window rather than whole points, because a terminal node on the canvas
        // is composited through the camera. See `Crispness::snap`.
        let position = crispness
            .snap(Pos2::new(pen.x + glyph.offset.x, pen.y + grid.cell.ascent + glyph.offset.y));
        quads.tinted.push((Rect::from_min_size(position, glyph.size), glyph.uv, colour));
    }
    // **A combining mark is centered over its letter**, which is the one placement that is right whatever font the
    // mark came from. Drawn at the cell's own pen, as it was, a mark with no advance of its own and a negative side
    // bearing landed to the left of the letter it belongs to. Its height is the font's, so an accent stays above
    // and a cedilla below.
    for mark in cell.marks.iter().copied().filter(|mark| !is_invisible_mark(*mark)) {
        let Some(glyph) = renderer.glyph(mark, &style) else { continue };
        let x = pen.x + (rect.width() - glyph.size.x) / 2.0;
        let position = crispness.snap(Pos2::new(x, pen.y + grid.cell.ascent + glyph.offset.y));
        quads.tinted.push((Rect::from_min_size(position, glyph.size), glyph.uv, colour));
    }
}

/// How thick a decoration line is in a cell this tall: one point up to 24 points, and a point more for each 24
/// after, so the lines grow with the font rather than vanishing next to large letters.
fn line_thickness(cell: CellMetrics) -> f32 {
    (cell.height / 24.0).floor().max(1.0)
}

/// Draw the underlines, overlines, strikethroughs and the hovered link's underline, from the cell's own
/// measurements rather than baked into the glyphs, which is how the editor draws them too.
pub(crate) fn paint_lines(
    painter: &egui::Painter,
    screen: &Screen,
    grid: Grid,
    hovered_link: Option<u32>,
) {
    let thickness = line_thickness(grid.cell);
    // **One mesh of plain rectangles rather than a `rect_filled` each.** egui draws a filled rectangle a pixel tall
    // and only a few pixels wide as a thin line, which smeared a dash across two rows and dropped a dot entirely.
    let mut bars = Mesh::default();
    for row in 0..screen.rows {
        for column in 0..screen.columns {
            let Some(cell) = screen.cell(row, column) else { continue };
            let hovered = hovered_link.is_some() && cell.link == hovered_link;
            if !cell.underline.is_on() && !cell.overline && !cell.strikethrough && !hovered {
                continue;
            }
            let foreground =
                Color32::from_rgb(cell.foreground.r, cell.foreground.g, cell.foreground.b);
            let pen = grid.at(row, column);
            let width = grid.cell.width;
            let underline_y = (pen.y + grid.cell.ascent + 2.0).round();
            let underline_colour = cell
                .underline_color
                .map(|color| Color32::from_rgb(color.r, color.g, color.b))
                .unwrap_or(foreground);
            let style = match (cell.underline, hovered) {
                (UnderlineStyle::None, true) => UnderlineStyle::Single,
                (other, _) => other,
            };
            let underline = Underline {
                left: pen.x,
                y: underline_y,
                width,
                thickness,
                colour: underline_colour,
            };
            paint_underline(painter, &mut bars, style, underline, grid);
            if cell.overline {
                let rect = Rect::from_min_size(
                    Pos2::new(pen.x, pen.y.round()),
                    Vec2::new(width, thickness),
                );
                bars.add_colored_rect(rect, foreground);
            }
            if cell.strikethrough {
                let y = (pen.y + grid.cell.ascent * 0.65).round();
                bars.add_colored_rect(
                    Rect::from_min_size(Pos2::new(pen.x, y), Vec2::new(width, thickness)),
                    foreground,
                );
            }
        }
    }
    if !bars.is_empty() {
        painter.add(Shape::mesh(bars));
    }
}

/// Where one cell's underline goes.
#[derive(Clone, Copy)]
struct Underline {
    left: f32,
    y: f32,
    width: f32,
    thickness: f32,
    colour: Color32,
}

/// Draw one cell's underline in `style`.
///
/// The dotted, dashed and curly lines are measured from the grid's own left edge rather than from the cell, so
/// a run of underlined cells is one unbroken pattern rather than a pattern that starts again in every cell.
fn paint_underline(
    painter: &egui::Painter,
    bars: &mut Mesh,
    style: UnderlineStyle,
    line: Underline,
    grid: Grid,
) {
    let Underline { left, y, width, thickness, colour } = line;
    let bar =
        |x: f32, w: f32, y: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, thickness));
    match style {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => {
            bars.add_colored_rect(bar(left, width, y), colour);
        }
        UnderlineStyle::Double => {
            bars.add_colored_rect(bar(left, width, y - thickness), colour);
            bars.add_colored_rect(bar(left, width, y + thickness), colour);
        }
        // **Dots and dashes start on whole pixels.** Placed at the fractions of a point a cell's edge falls on, a
        // one pixel dot was spread over two pixels at half strength and the dotted line could not be seen at all.
        UnderlineStyle::Dotted => {
            let dot = thickness;
            for (from, to) in pattern(left, width, grid.origin.x, dot, dot * 2.0, painter) {
                bars.add_colored_rect(bar(from, to - from, y), colour);
            }
        }
        UnderlineStyle::Dashed => {
            let dash = (thickness * 3.0).max(3.0);
            for (from, to) in
                pattern(left, width, grid.origin.x, dash, dash + thickness * 2.0, painter)
            {
                bars.add_colored_rect(bar(from, to - from, y), colour);
            }
        }
        UnderlineStyle::Curly => {
            // One full wave a cell, so the wave meets itself at every cell's edge.
            let amplitude = (thickness * 1.5).max(1.5);
            let steps = 12;
            let points: Vec<Pos2> = (0..=steps)
                .map(|step| {
                    let x = left + width * step as f32 / steps as f32;
                    let phase = (x - grid.origin.x) / grid.cell.width * std::f32::consts::TAU;
                    Pos2::new(x, y + thickness / 2.0 + amplitude * phase.sin())
                })
                .collect();
            painter.add(Shape::line(points, Stroke::new(thickness, colour)));
        }
    }
}

/// The runs of a repeating pattern of `mark` points every `step` points that fall inside one cell's underline,
/// counted from the grid's left edge so a run of cells is one unbroken pattern, each run starting and ending on a
/// whole pixel.
///
/// @param left - where the cell's underline starts
/// @param width - how long it is
/// @param origin - the grid's left edge, which the pattern is counted from
/// @param mark - how long each dot or dash is
/// @param step - how far apart two dots or dashes start
/// @param painter - the painter, for the size of a pixel
fn pattern(
    left: f32,
    width: f32,
    origin: f32,
    mark: f32,
    step: f32,
    painter: &egui::Painter,
) -> Vec<(f32, f32)> {
    let pixels = painter.pixels_per_point();
    let snap = |value: f32| (value * pixels).round() / pixels;
    let (right, step) = (left + width, step.max(1.0 / pixels));
    let mut runs = Vec::new();
    let mut index = ((left - origin) / step).floor() as i64;
    loop {
        let start = snap(origin + index as f32 * step);
        if start >= right {
            break;
        }
        let (from, to) =
            (start.max(snap(left)), snap(start + mark.max(1.0 / pixels)).min(snap(right)));
        if to > from {
            runs.push((from, to));
        }
        index += 1;
    }
    runs
}

/// Which pictures a pass draws, by their `z`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageLayer {
    /// Below every cell's background: kitty's `z` under `i32::MIN / 2`.
    BelowBackgrounds,
    /// Over the backgrounds and under the text: a negative `z`.
    BelowText,
    /// Over the text: a `z` of zero or more, which is where a picture goes when a program says nothing.
    OverText,
}

impl ImageLayer {
    fn holds(self, z: i32) -> bool {
        match self {
            ImageLayer::BelowBackgrounds => z < i32::MIN / 2,
            ImageLayer::BelowText => (i32::MIN / 2..0).contains(&z),
            ImageLayer::OverText => z >= 0,
        }
    }
}

/// Draw the pictures in `layer`.
///
/// A picture is measured in the pixels of the cell size the program was told, which is the cell size in whole
/// points, so it is scaled by the cell's real size here to sit on exactly the cells the emulator kept free for it.
pub(crate) fn paint_images(ui: &egui::Ui, screen: &Screen, grid: Grid, layer: ImageLayer) {
    for image in screen.images.iter().filter(|image| layer.holds(image.z)) {
        let scale = Vec2::new(
            grid.cell.width / image.cell.0.max(1) as f32,
            grid.cell.height / image.cell.1.max(1) as f32,
        );
        let corner = Pos2::new(
            grid.origin.x + image.column as f32 * grid.cell.width + image.offset.0 as f32 * scale.x,
            grid.origin.y + image.row as f32 * grid.cell.height + image.offset.1 as f32 * scale.y,
        );
        let size = Vec2::new(image.size.0 as f32 * scale.x, image.size.1 as f32 * scale.y);
        let (width, height) = (image.pixels.width.max(1) as f32, image.pixels.height.max(1) as f32);
        let (left, top, shown_width, shown_height) = image.source;
        let uv = Rect::from_min_max(
            Pos2::new(left as f32 / width, top as f32 / height),
            Pos2::new((left + shown_width) as f32 / width, (top + shown_height) as f32 / height),
        );
        let texture = image_texture(ui.ctx(), &image.pixels);
        ui.painter().image(texture, Rect::from_min_size(corner, size), uv, UNTINTED);
    }
}

/// The most pixels one picture is uploaded with: two megabytes, the largest upload the graphics driver does not
/// keep for the rest of the process (`task-2279`). A larger picture is scaled down to fit before it is uploaded.
const MOST_IMAGE_PIXELS: usize = 512 * 1024;

/// How many frames a picture's texture is kept after it was last drawn.
const IMAGE_KEPT_FRAMES: u64 = 600;

/// The textures of the pictures drawn recently, by the address of their pixels, which no two live pictures share.
#[derive(Clone, Default)]
struct ImageTextures(Arc<std::sync::Mutex<TexturesByPixels>>);

/// A picture's texture and the frame it was last drawn on, by the address and the size of its pixels.
type TexturesByPixels = HashMap<(usize, u32, u32), (egui::TextureHandle, u64)>;

/// The texture for one picture, uploaded the first time it is drawn and kept while it is still being drawn.
fn image_texture(ctx: &egui::Context, pixels: &ImagePixels) -> egui::TextureId {
    let cache = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ImageTextures>(egui::Id::new("unluminous-terminal-images"))
            .clone()
    });
    let pass = ctx.cumulative_pass_nr();
    let key = (Arc::as_ptr(&pixels.rgba) as usize, pixels.width, pixels.height);
    let mut textures = cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    textures.retain(|_, (_, last)| pass.saturating_sub(*last) <= IMAGE_KEPT_FRAMES);
    if let Some((texture, last)) = textures.get_mut(&key) {
        *last = pass;
        return texture.id();
    }
    let texture = ctx.load_texture(
        "unluminous-terminal-image",
        image_of(pixels),
        egui::TextureOptions::LINEAR,
    );
    let id = texture.id();
    textures.insert(key, (texture, pass));
    id
}

/// A picture's pixels as an egui image, scaled down first when there are more of them than one upload should hold.
fn image_of(pixels: &ImagePixels) -> egui::ColorImage {
    let (width, height) = (pixels.width.max(1), pixels.height.max(1));
    let size = [width as usize, height as usize];
    let whole = (width as usize) * (height as usize);
    if pixels.rgba.len() < whole * 4 {
        return egui::ColorImage::new(size, vec![Color32::TRANSPARENT; whole]);
    }
    if whole <= MOST_IMAGE_PIXELS {
        return egui::ColorImage::from_rgba_unmultiplied(size, &pixels.rgba[..whole * 4]);
    }
    let shrink = (MOST_IMAGE_PIXELS as f64 / whole as f64).sqrt();
    let (to_width, to_height) =
        (((width as f64 * shrink) as u32).max(1), ((height as f64 * shrink) as u32).max(1));
    let Some(source) = image::RgbaImage::from_raw(width, height, pixels.rgba[..whole * 4].to_vec())
    else {
        return egui::ColorImage::new(size, vec![Color32::TRANSPARENT; whole]);
    };
    let smaller = image::imageops::resize(
        &source,
        to_width,
        to_height,
        image::imageops::FilterType::Triangle,
    );
    egui::ColorImage::from_rgba_unmultiplied(
        [to_width as usize, to_height as usize],
        smaller.as_raw(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_with_no_z_is_drawn_over_the_text() {
        assert!(ImageLayer::OverText.holds(0));
        assert!(ImageLayer::BelowText.holds(-1));
        assert!(!ImageLayer::BelowText.holds(i32::MIN));
        assert!(ImageLayer::BelowBackgrounds.holds(i32::MIN));
    }

    #[test]
    fn a_picture_too_large_for_one_upload_is_scaled_down() {
        let (width, height) = (2048_u32, 1024_u32);
        let pixels = ImagePixels {
            id: 1,
            generation: 1,
            width,
            height,
            rgba: Arc::new(vec![255; (width * height * 4) as usize]),
        };
        let image = image_of(&pixels);
        assert!(image.size[0] * image.size[1] <= MOST_IMAGE_PIXELS);
        assert_eq!(image.size[0] / image.size[1], 2, "with its proportions kept");
    }

    #[test]
    fn the_joiner_and_the_selectors_are_not_drawn_as_marks() {
        assert!(is_invisible_mark('\u{200D}'));
        assert!(is_invisible_mark('\u{FE0F}'));
        assert!(!is_invisible_mark('\u{0301}'), "an acute accent is drawn");
    }
}
