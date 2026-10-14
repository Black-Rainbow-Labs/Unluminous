//! The Unluminous mark in the title bar, which is where the word `Unluminous` used to be (`task-2258`).
//!
//! The mark is six nested outlined channels making a `U`, from the Claude Design canvas "unluminous — App
//! Icon", page "Arches U". `installer/icon` draws it from the design's own numbers and writes it twice, on
//! nothing: `icons/logo/logo-dark.png` is the dark board's light channels and `logo-light.png` the light
//! board's dark ones. The mark drawn is the one for the theme the window is in, and it never moves.
//!
//! **What it costs.** The picture is decoded once, the first time the mark is drawn, and shrunk to the size the
//! mark takes on this display, in pixels, so it is one texel to one pixel rather than a large picture filtered
//! down. After that it is one textured rectangle a frame. A new texture is made only when the display's
//! density or the darkness of the theme changes.

use std::sync::{Arc, Mutex};

use egui::{Color32, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

/// The mark for a dark theme and for a light one, each `SIDE` pixels square.
const DARK: &[u8] = include_bytes!("../../icons/logo/logo-dark.png");
const LIGHT: &[u8] = include_bytes!("../../icons/logo/logo-light.png");
const SIDE: usize = 256;

/// The mark shrunk to one display's pixels, for one darkness of theme.
struct Shrunk {
    side: usize,
    dark: bool,
    texture: TextureHandle,
}

fn kept_id() -> egui::Id {
    egui::Id::new("unluminous-logo")
}

/// Decode one of the two pictures into premultiplied colours from 0 to 1, which is what averaging has to be
/// done on so the colour of the transparent pixels round each channel does not bleed into it.
fn decode(bytes: &[u8]) -> Vec<[f32; 4]> {
    let image = image::load_from_memory(bytes).expect("the logo is a PNG").to_rgba8();
    image
        .pixels()
        .map(|pixel| {
            let a = pixel[3] as f32 / 255.0;
            [
                pixel[0] as f32 / 255.0 * a,
                pixel[1] as f32 / 255.0 * a,
                pixel[2] as f32 / 255.0 * a,
                a,
            ]
        })
        .collect()
}

/// Average the picture down to `side` pixels square.
fn shrink(picture: &[[f32; 4]], side: usize) -> Vec<Color32> {
    let ratio = SIDE as f32 / side as f32;
    let mut out = vec![Color32::TRANSPARENT; side * side];
    for y in 0..side {
        let y0 = (y as f32 * ratio).floor() as usize;
        let y1 = (((y + 1) as f32 * ratio).ceil() as usize).min(SIDE).max(y0 + 1);
        for x in 0..side {
            let x0 = (x as f32 * ratio).floor() as usize;
            let x1 = (((x + 1) as f32 * ratio).ceil() as usize).min(SIDE).max(x0 + 1);
            let mut sum = [0f32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let pixel = picture[sy * SIDE + sx];
                    for channel in 0..4 {
                        sum[channel] += pixel[channel];
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as f32;
            let byte = |v: f32| (v / count * 255.0).round().clamp(0.0, 255.0) as u8;
            out[y * side + x] = Color32::from_rgba_premultiplied(
                byte(sum[0]),
                byte(sum[1]),
                byte(sum[2]),
                byte(sum[3]),
            );
        }
    }
    out
}

/// The texture for the mark `side` pixels square, made the first time it is asked for at that size and
/// darkness.
fn texture(ctx: &egui::Context, side: usize, dark: bool) -> Arc<Shrunk> {
    let kept = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<Option<Arc<Shrunk>>>>>(kept_id()).clone()
    });
    let mut kept = kept.lock().expect("the logo's texture");
    if let Some(shrunk) = kept.as_ref().filter(|shrunk| shrunk.side == side && shrunk.dark == dark)
    {
        return shrunk.clone();
    }
    let picture = decode(if dark { DARK } else { LIGHT });
    let image = egui::ColorImage::new([side, side], shrink(&picture, side));
    let texture = ctx.load_texture("unluminous-logo", image, TextureOptions::LINEAR);
    let shrunk = Arc::new(Shrunk { side, dark, texture });
    *kept = Some(shrunk.clone());
    shrunk
}

/// Draw the mark `height` points tall, centred in `rect`, in the version for the theme the window is in.
pub fn paint(ui: &egui::Ui, rect: Rect, height: f32) {
    let ctx = ui.ctx();
    // On whole pixels, one texel to one pixel, so it is as sharp as the picture allows.
    let ppp = ctx.pixels_per_point();
    let side = ((height * ppp).round() as usize).clamp(8, SIDE);
    let shown = side as f32 / ppp;
    let centre = rect.center();
    let min = Pos2::new(
        ((centre.x - shown / 2.0) * ppp).round() / ppp,
        ((centre.y - shown / 2.0) * ppp).round() / ppp,
    );
    let shrunk = texture(ctx, side, crate::theme::is_dark());
    let rect = Rect::from_min_size(min, Vec2::splat(shown));
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    // any ground: white is the tint that leaves a picture's own colours alone.
    ui.painter().image(shrunk.texture.id(), rect, uv, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_pictures_are_the_size_the_icon_tool_writes() {
        for bytes in [DARK, LIGHT] {
            let image = image::load_from_memory(bytes).expect("a PNG").to_rgba8();
            assert_eq!((image.width() as usize, image.height() as usize), (SIDE, SIDE));
        }
    }

    #[test]
    fn the_dark_themes_mark_is_light_and_the_light_themes_is_dark() {
        // The most opaque pixel of each picture is in a channel, so its colour is the ink.
        let ink = |bytes: &[u8]| {
            let image = image::load_from_memory(bytes).expect("a PNG").to_rgba8();
            let pixel = image.pixels().max_by_key(|pixel| pixel[3]).expect("pixels");
            pixel[0]
        };
        assert!(ink(DARK) > 200, "light channels for a dark window");
        assert!(ink(LIGHT) < 60, "dark channels for a light window");
    }

    #[test]
    fn a_shrunk_mark_keeps_its_corners_clear_and_its_bowl_solid() {
        let side = 20;
        let out = shrink(&decode(DARK), side);
        let at = |x: usize, y: usize| out[y * side + x];
        assert_eq!(at(0, side - 1).a(), 0, "the bottom corners are outside the round of the bowl");
        assert_eq!(at(side - 1, side - 1).a(), 0);
        let bowl = (0..side).map(|y| at(side / 2, y).a()).max().unwrap_or(0);
        assert!(bowl > 150, "the channels under the middle of the bowl are drawn: {bowl}");
    }
}
