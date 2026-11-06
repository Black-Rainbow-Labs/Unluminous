//! Color emoji for the terminal, in a full color atlas of their own (`task-2514`).
//!
//! The glyph atlas in [`crate::services::text_renderer`] holds coverage only: white pixels whose alpha is how much
//! of the pixel a glyph covers, tinted by the text color when drawn. An emoji is a picture, and a picture has no
//! single color to tint it with, so a terminal that drew emoji through that atlas drew empty boxes or flat
//! silhouettes. Termoil's own window drew them in color, which is what the ticket compared against.
//!
//! An emoji is drawn here by `resvg`, as one line of SVG text in the system's emoji family. `resvg` already reads
//! every kind of color font there is: COLR layers and gradients (Segoe UI Emoji on Windows), the PNG pictures inside
//! `sbix` and `CBDT` (Apple Color Emoji, Noto Color Emoji), and it shapes the text first, so a family, a flag or a
//! skin tone joined with `U+200D` comes out as the one picture the sequence names rather than as its parts.
//!
//! The atlas is allocated the first time an emoji is drawn, so a window that never shows one pays nothing for it.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, ColorImage, TextureHandle};

/// The color atlas is one texture this many pixels on each side: one megabyte, which stays under the size of
/// upload the graphics driver keeps for the life of the process (`task-2279`).
const SIDE: usize = 512;

/// The emoji families tried, in order. The first one the system has is used, and `resvg` falls back through the
/// system's fonts for a character none of them has.
const EMOJI_FAMILIES: &str =
    "'Apple Color Emoji', 'Segoe UI Emoji', 'Noto Color Emoji', 'Twemoji Mozilla'";

/// How large the emoji is set, as a share of the box it is drawn in. A little under the whole box, because an
/// emoji font's pictures reach the edges of the em square and a picture touching the next cell reads as crowded.
const FILL: f32 = 0.86;

/// Emoji drawn so far, and where each one is in the atlas.
pub struct ColorGlyphs {
    /// Premultiplied pixels, row after row. Empty until the first emoji is drawn.
    pixels: Vec<Color32>,
    texture: Option<TextureHandle>,
    /// The text of an emoji and the pixel size of its box, and where it is in the atlas: `None` when the text
    /// drew nothing, so the ordinary glyph path is used for it.
    entries: HashMap<(String, u32, u32), Option<egui::Rect>>,
    pen_x: usize,
    pen_y: usize,
    row_height: usize,
    /// The pixels changed since the texture was last uploaded.
    changed: bool,
    /// Bumped whenever the atlas is cleared, so a painter can tell its positions went stale.
    generation: u64,
}

impl Default for ColorGlyphs {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorGlyphs {
    pub fn new() -> Self {
        Self {
            pixels: Vec::new(),
            texture: None,
            entries: HashMap::new(),
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            changed: false,
            generation: 0,
        }
    }

    /// Where the emoji `text` drawn into a box of `width` by `height` pixels is in the atlas, as a fraction of the
    /// atlas, drawing it the first time it is asked for. `None` when no font on the system draws it in color.
    ///
    /// @param text - the emoji: one character, with any variation selector, joiner or modifier after it
    /// @param width - the width of the box it is drawn into, in pixels
    /// @param height - the height of the box, in pixels
    pub fn glyph(&mut self, text: &str, width: u32, height: u32) -> Option<egui::Rect> {
        let (width, height) = (width.clamp(1, SIDE as u32 / 2), height.clamp(1, SIDE as u32 / 2));
        let key = (text.to_owned(), width, height);
        if let Some(found) = self.entries.get(&key) {
            return *found;
        }
        let drawn = draw_emoji(text, width, height);
        let placed = drawn.and_then(|pixels| {
            self.place(&pixels, width as usize, height as usize).or_else(|| {
                // The atlas is full. Start it again rather than stop drawing.
                self.clear();
                self.place(&pixels, width as usize, height as usize)
            })
        });
        self.entries.insert(key, placed);
        placed
    }

    /// Copy one emoji's pixels into the atlas and answer where they went, or `None` when there is no room.
    fn place(&mut self, pixels: &[Color32], width: usize, height: usize) -> Option<egui::Rect> {
        if self.pixels.is_empty() {
            self.pixels = vec![Color32::TRANSPARENT; SIDE * SIDE];
        }
        if self.pen_x + width > SIDE {
            self.pen_x = 0;
            self.pen_y += self.row_height + 1;
            self.row_height = 0;
        }
        if self.pen_y + height > SIDE {
            return None;
        }
        let (x, y) = (self.pen_x, self.pen_y);
        for row in 0..height {
            let start = (y + row) * SIDE + x;
            self.pixels[start..start + width]
                .copy_from_slice(&pixels[row * width..(row + 1) * width]);
        }
        self.pen_x += width + 1;
        self.row_height = self.row_height.max(height);
        self.changed = true;
        let side = SIDE as f32;
        Some(egui::Rect::from_min_max(
            egui::pos2(x as f32 / side, y as f32 / side),
            egui::pos2((x + width) as f32 / side, (y + height) as f32 / side),
        ))
    }

    /// Forget every emoji, which a full atlas does.
    fn clear(&mut self) {
        self.pixels.iter_mut().for_each(|pixel| *pixel = Color32::TRANSPARENT);
        self.entries.clear();
        self.pen_x = 0;
        self.pen_y = 0;
        self.row_height = 0;
        self.changed = true;
        self.generation += 1;
    }

    /// Bumped whenever the atlas is cleared.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The atlas texture, uploaded again when an emoji has been added since the last upload.
    pub fn texture(&mut self, ctx: &egui::Context) -> egui::TextureId {
        if self.pixels.is_empty() {
            self.pixels = vec![Color32::TRANSPARENT; SIDE * SIDE];
            self.changed = true;
        }
        if self.changed || self.texture.is_none() {
            let image = ColorImage::new([SIDE, SIDE], self.pixels.clone());
            match &mut self.texture {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    self.texture = Some(ctx.load_texture(
                        "unluminous-color-glyphs",
                        image,
                        egui::TextureOptions::LINEAR,
                    ))
                }
            }
            self.changed = false;
        }
        self.texture.as_ref().map(TextureHandle::id).unwrap_or_default()
    }
}

/// Draw `text` in the system's emoji family into a box of `width` by `height` pixels, as premultiplied pixels.
/// `None` when nothing was drawn, or when what was drawn has no color in it, which is a monochrome symbol font
/// standing in for an emoji font the system does not have.
fn draw_emoji(text: &str, width: u32, height: u32) -> Option<Vec<Color32>> {
    let size = (height as f32).min(width as f32) * FILL;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\">\
         <text x=\"{x}\" y=\"{y}\" font-family=\"{EMOJI_FAMILIES}\" font-size=\"{size}\" \
         text-anchor=\"middle\" dominant-baseline=\"central\" fill=\"#000\">{text}</text></svg>",
        x = width as f32 / 2.0,
        y = height as f32 / 2.0,
        text = escaped(text),
    );
    let options = resvg::usvg::Options { fontdb: emoji_fonts(), ..Default::default() };
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &options).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let pixels: Vec<Color32> = pixmap
        .data()
        .chunks_exact(4)
        .map(|p| Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    has_color(&pixels).then_some(pixels)
}

/// Whether any visible pixel is something other than the black the text was filled with, which is what says a
/// color font drew it.
fn has_color(pixels: &[Color32]) -> bool {
    pixels.iter().any(|pixel| pixel.a() > 16 && (pixel.r() > 8 || pixel.g() > 8 || pixel.b() > 8))
}

/// `text` with the five characters XML gives a meaning to written as references.
fn escaped(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// The machine's fonts, read once. Shared with the vector pictures, which read the same list.
fn emoji_fonts() -> Arc<resvg::usvg::fontdb::Database> {
    crate::services::picture::system_fonts()
}

/// Whether a cell holding `character` followed by `marks` is an emoji drawn in color.
///
/// A character whose default presentation is an emoji, or any character followed by `U+FE0F`, the selector that
/// asks for the emoji presentation, or a keycap. A character followed by `U+FE0E`, which asks for the text
/// presentation, is not.
pub fn is_emoji(character: char, marks: &[char]) -> bool {
    if marks.contains(&'\u{FE0E}') {
        return false;
    }
    if marks.contains(&'\u{FE0F}') || marks.contains(&'\u{20E3}') {
        return !character.is_ascii_alphabetic();
    }
    has_emoji_presentation(character)
}

/// The characters Unicode draws as emoji with nothing after them: `Emoji_Presentation=Yes` in `emoji-data.txt`.
fn has_emoji_presentation(character: char) -> bool {
    matches!(character as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD..=0x25FE | 0x2614..=0x2615
        | 0x2648..=0x2653 | 0x267F | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3 | 0x26F5 | 0x26FA | 0x26FD
        | 0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757
        | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
        | 0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A | 0x1F1E6..=0x1F1FF | 0x1F201 | 0x1F21A
        | 0x1F22F | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A | 0x1F250..=0x1F251 | 0x1F300..=0x1F320
        | 0x1F32D..=0x1F335 | 0x1F337..=0x1F37C | 0x1F37E..=0x1F393 | 0x1F3A0..=0x1F3CA
        | 0x1F3CF..=0x1F3D3 | 0x1F3E0..=0x1F3F0 | 0x1F3F4 | 0x1F3F8..=0x1F43E | 0x1F440
        | 0x1F442..=0x1F4FC | 0x1F4FF..=0x1F53D | 0x1F54B..=0x1F54E | 0x1F550..=0x1F567 | 0x1F57A
        | 0x1F595..=0x1F596 | 0x1F5A4 | 0x1F5FB..=0x1F64F | 0x1F680..=0x1F6C5 | 0x1F6CC
        | 0x1F6D0..=0x1F6D2 | 0x1F6D5..=0x1F6D7 | 0x1F6DC..=0x1F6DF | 0x1F6EB..=0x1F6EC
        | 0x1F6F4..=0x1F6FC | 0x1F7E0..=0x1F7EB | 0x1F7F0 | 0x1F90C..=0x1F93A | 0x1F93C..=0x1F945
        | 0x1F947..=0x1F9FF | 0x1FA70..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emoji_is_drawn_in_color_and_a_letter_is_not_an_emoji() {
        assert!(is_emoji('\u{1F600}', &[]), "a face is an emoji with nothing after it");
        assert!(is_emoji('\u{2705}', &[]), "so is the check mark claude prints");
        assert!(!is_emoji('\u{2764}', &[]), "a heart with nothing after it is text");
        assert!(is_emoji('\u{2764}', &['\u{FE0F}']), "and an emoji with the selector after it");
        assert!(!is_emoji('\u{1F600}', &['\u{FE0E}']), "the text selector asks for text");
        assert!(!is_emoji('a', &[]));
        assert!(is_emoji('1', &['\u{FE0F}', '\u{20E3}']), "a keycap");
    }

    #[test]
    fn markup_in_the_text_is_written_as_references() {
        assert_eq!(escaped("<&>"), "&lt;&amp;&gt;");
    }

    /// Only meaningful on a machine with a color emoji font, which every desktop this is released for has.
    #[test]
    fn an_emoji_drawn_through_resvg_has_color_in_it() {
        let has_a_color_font = emoji_fonts().faces().any(|face| {
            face.families.iter().any(|(name, _)| {
                matches!(name.as_str(), "Apple Color Emoji" | "Segoe UI Emoji" | "Noto Color Emoji")
            })
        });
        if !has_a_color_font {
            return;
        }
        let mut glyphs = ColorGlyphs::new();
        assert!(glyphs.glyph("\u{1F600}", 32, 32).is_some(), "a grinning face is drawn in color");
        assert!(glyphs.glyph("a", 32, 32).is_none(), "a letter is not");
    }
}
