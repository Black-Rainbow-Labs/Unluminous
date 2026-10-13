//! The Unluminous mark in the title bar, which is where the word `Unluminous` used to be (`task-2258`).
//!
//! The mark is the blue `U` with eight coloured pages turning round its bowl, from the Claude Design canvas
//! "unluminous — App Icon". `installer/icon` draws it from the design's own numbers and writes the turn as
//! [`FRAMES`] pictures in one strip, `icons/logo/frames.png`, which is built into the binary. Nothing here
//! knows how the mark is drawn: it shows one of those pictures.
//!
//! **What it costs.** The strip is decoded once, the first time the mark is drawn, and shrunk to the size
//! the mark takes on this display, in pixels, so the picture on the screen is one texel to one pixel rather
//! than a 96 pixel picture filtered down. All [`FRAMES`] pictures go into one texture, and a frame of the
//! animation is a different rectangle of it: no upload, no decode and no drawing beyond one textured
//! rectangle. A new texture is made only when the display's density changes.
//!
//! **What a frame of the animation costs is the window's frame.** egui has no way to draw one rectangle
//! again on its own, so a turning mark is the whole window drawn at the animation's rate.
//! `appearance.logo_motion` says when it turns, and [`Motion`] is that setting; see its comment for what
//! each answer was measured to cost.

use std::sync::Arc;

use egui::{Color32, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

/// The strip `installer/icon` writes: [`FRAMES`] pictures, [`ACROSS`] to a row, each [`SIDE`] pixels
/// square, the first at the start of a turn.
const STRIP: &[u8] = include_bytes!("../../icons/logo/frames.png");
pub const FRAMES: usize = 32;
const ACROSS: usize = 8;
const SIDE: usize = 96;
/// The design turns once in this many seconds.
pub const TURN: f64 = 0.55;
/// The picture the mark stands still on, which is half way through a turn and is the design's still version.
pub const STILL: usize = FRAMES / 2;
/// How long the mark turns for when a window opens, so the first thing a person sees it do is move.
pub const OPENING: f64 = 2.0 * TURN;

/// When the mark turns.
///
/// Measured on the release build with the `sample` folder open and nothing else happening
/// (`_agent_output/task-2258-logo/measure.ps1`): with the mark still the window used under a millisecond of
/// processor time a second, and with it turning **216 ms a second**, a fifth of a core, because each of the
/// sixty pictures a second is a whole frame of the window. So the default turns it only when somebody is
/// looking at it, and turning all the time is a choice a person makes in `Settings -> Appearance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// While the pointer is over it, and for [`OPENING`] seconds when the window opens.
    Hover,
    /// All the time.
    Always,
    /// Never: the still picture.
    Off,
}

impl Motion {
    /// The words `appearance.logo_motion` is written with.
    pub const NAMES: [&'static str; 3] = ["hover", "always", "off"];

    /// Read the setting. Empty, or a word this version does not know, is [`Motion::Hover`].
    pub fn parse(words: &str) -> Self {
        match words.trim().to_ascii_lowercase().as_str() {
            "always" => Motion::Always,
            "off" => Motion::Off,
            _ => Motion::Hover,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Motion::Hover => "hover",
            Motion::Always => "always",
            Motion::Off => "off",
        }
    }
}

/// Which picture of the turn to show, `now` seconds into the window's life, and whether the mark is turning.
///
/// `opened` is when the mark was first drawn. A pure function, so the rule is tested with no window.
pub fn frame_at(motion: Motion, now: f64, opened: f64, hovered: bool) -> (usize, bool) {
    let turning = match motion {
        Motion::Off => false,
        Motion::Always => true,
        Motion::Hover => hovered || now - opened < OPENING,
    };
    if !turning {
        return (STILL, false);
    }
    // Counted from when the mark was first drawn, so a window that opens starts at the beginning of a turn.
    let turns = ((now - opened).max(0.0) / TURN).fract();
    (((turns * FRAMES as f64) as usize).min(FRAMES - 1), true)
}

/// How long until the picture after the one showing is due.
fn until_the_next_frame(now: f64, opened: f64) -> f64 {
    let step = TURN / FRAMES as f64;
    let into = (now - opened).max(0.0) % step;
    (step - into).max(0.001)
}

/// The strip shrunk to one display's pixels, in one texture.
struct Shrunk {
    /// How many pixels square each picture is.
    side: usize,
    texture: TextureHandle,
}

/// What the mark keeps between frames: the decoded strip and its texture, and when it was first drawn.
#[derive(Default)]
struct Kept {
    strip: Option<Arc<Vec<[f32; 4]>>>,
    shrunk: Option<Arc<Shrunk>>,
    opened: Option<f64>,
}

fn kept_id() -> egui::Id {
    egui::Id::new("unluminous-logo")
}

/// Decode the strip into premultiplied colours from 0 to 1, which is what averaging has to be done on so
/// the colour of the transparent pixels round the edge of the letter does not bleed into it.
fn decode() -> Vec<[f32; 4]> {
    let image = image::load_from_memory(STRIP).expect("the logo strip is a PNG").to_rgba8();
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

/// Average picture `frame` of the strip down to `side` pixels square, written into `out` at column
/// `frame * side` of an image `FRAMES * side` wide.
fn shrink_into(strip: &[[f32; 4]], frame: usize, side: usize, out: &mut [Color32]) {
    let width = ACROSS * SIDE;
    let (left, top) = ((frame % ACROSS) * SIDE, (frame / ACROSS) * SIDE);
    let ratio = SIDE as f32 / side as f32;
    for y in 0..side {
        let y0 = (y as f32 * ratio).floor() as usize;
        let y1 = (((y + 1) as f32 * ratio).ceil() as usize).min(SIDE).max(y0 + 1);
        for x in 0..side {
            let x0 = (x as f32 * ratio).floor() as usize;
            let x1 = (((x + 1) as f32 * ratio).ceil() as usize).min(SIDE).max(x0 + 1);
            let mut sum = [0f32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let pixel = strip[(top + sy) * width + left + sx];
                    for channel in 0..4 {
                        sum[channel] += pixel[channel];
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as f32;
            let byte = |v: f32| (v / count * 255.0).round().clamp(0.0, 255.0) as u8;
            out[y * FRAMES * side + frame * side + x] = Color32::from_rgba_premultiplied(
                byte(sum[0]),
                byte(sum[1]),
                byte(sum[2]),
                byte(sum[3]),
            );
        }
    }
}

/// The texture for pictures `side` pixels square, made the first time it is asked for at that size.
fn texture(ctx: &egui::Context, side: usize) -> Arc<Shrunk> {
    let kept = ctx.data_mut(|data| {
        let kept = data.get_temp_mut_or_default::<Arc<std::sync::Mutex<Kept>>>(kept_id());
        kept.clone()
    });
    let mut kept = kept.lock().expect("the logo's state");
    if let Some(shrunk) = kept.shrunk.as_ref().filter(|shrunk| shrunk.side == side) {
        return shrunk.clone();
    }
    let strip = kept.strip.get_or_insert_with(|| Arc::new(decode())).clone();
    let mut pixels = vec![Color32::TRANSPARENT; FRAMES * side * side];
    for frame in 0..FRAMES {
        shrink_into(&strip, frame, side, &mut pixels);
    }
    let image = egui::ColorImage::new([FRAMES * side, side], pixels);
    let texture = ctx.load_texture("unluminous-logo", image, TextureOptions::LINEAR);
    let shrunk = Arc::new(Shrunk { side, texture });
    kept.shrunk = Some(shrunk.clone());
    shrunk
}

/// When the mark was first drawn in this window.
fn opened(ctx: &egui::Context, now: f64) -> f64 {
    let kept = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<std::sync::Mutex<Kept>>>(kept_id()).clone()
    });
    let mut kept = kept.lock().expect("the logo's state");
    *kept.opened.get_or_insert(now)
}

/// Draw the mark `height` points tall, centred in `rect`, and ask for the next frame when it is turning.
/// Answers which picture of the turn it drew, so a test can ask.
pub fn paint(ui: &egui::Ui, rect: Rect, height: f32, motion: Motion, hovered: bool) -> usize {
    let ctx = ui.ctx();
    let now = ui.input(|input| input.time);
    let opened = opened(ctx, now);
    let (frame, turning) = frame_at(motion, now, opened, hovered);
    if turning {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(until_the_next_frame(
            now, opened,
        )));
    }

    // The mark is drawn one texel to one pixel, on whole pixels, so it is as sharp as the strip allows.
    let ppp = ctx.pixels_per_point();
    let side = ((height * ppp).round() as usize).clamp(8, SIDE);
    let shown = side as f32 / ppp;
    let centre = rect.center();
    let min = Pos2::new(
        ((centre.x - shown / 2.0) * ppp).round() / ppp,
        ((centre.y - shown / 2.0) * ppp).round() / ppp,
    );
    let shrunk = texture(ctx, side);
    let u = frame as f32 / FRAMES as f32;
    let uv = Rect::from_min_max(Pos2::new(u, 0.0), Pos2::new(u + 1.0 / FRAMES as f32, 1.0));
    let rect = Rect::from_min_size(min, Vec2::splat(shown));
    // any ground: white is the tint that leaves a picture's own colours alone.
    ui.painter().image(shrunk.texture.id(), rect, uv, Color32::WHITE);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_is_the_size_the_icon_tool_writes() {
        let image = image::load_from_memory(STRIP).expect("a PNG").to_rgba8();
        assert_eq!(image.width() as usize, ACROSS * SIDE);
        assert_eq!(image.height() as usize, FRAMES.div_ceil(ACROSS) * SIDE);
        // Transparent in the corners, which is outside the letter, and opaque in the middle of its left arm.
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(30, 20)[3], 255);
    }

    #[test]
    fn a_mark_left_alone_is_the_still_picture_once_the_window_has_opened() {
        assert_eq!(frame_at(Motion::Hover, 10.0, 0.0, false), (STILL, false));
        assert_eq!(frame_at(Motion::Off, 0.1, 0.0, true), (STILL, false));
    }

    #[test]
    fn the_mark_turns_while_it_is_pointed_at_and_when_the_window_opens() {
        let (_, turning) = frame_at(Motion::Hover, 10.0, 0.0, true);
        assert!(turning);
        let (frame, turning) = frame_at(Motion::Hover, 0.0, 0.0, false);
        assert!(turning);
        assert_eq!(frame, 0, "a window that opens starts at the beginning of a turn");
        assert!(frame_at(Motion::Hover, OPENING - 0.01, 0.0, false).1);
        assert!(!frame_at(Motion::Hover, OPENING + 0.01, 0.0, false).1);
    }

    #[test]
    fn one_turn_shows_every_picture_once_in_order() {
        let step = TURN / FRAMES as f64;
        let seen: Vec<usize> = (0..FRAMES)
            .map(|i| frame_at(Motion::Always, 5.0 + step * (i as f64 + 0.5), 5.0, false).0)
            .collect();
        assert_eq!(seen, (0..FRAMES).collect::<Vec<_>>());
    }

    #[test]
    fn the_setting_reads_its_three_words_and_falls_back_to_hover() {
        for name in Motion::NAMES {
            assert_eq!(Motion::parse(name).name(), name);
        }
        assert_eq!(Motion::parse(""), Motion::Hover);
        assert_eq!(Motion::parse("Always"), Motion::Always);
        assert_eq!(Motion::parse("spin"), Motion::Hover);
    }

    #[test]
    fn a_shrunk_picture_keeps_the_letter_opaque_and_the_corners_clear() {
        let strip = decode();
        let side = 20;
        let mut out = vec![Color32::TRANSPARENT; FRAMES * side * side];
        shrink_into(&strip, STILL, side, &mut out);
        let at = |x: usize, y: usize| out[y * FRAMES * side + STILL * side + x];
        // The two bottom corners are outside the round of the bowl.
        assert_eq!(at(0, side - 1).a(), 0);
        assert_eq!(at(side - 1, side - 1).a(), 0);
        assert!(at(side / 2, side - 2).a() > 200, "the bottom of the bowl is solid");
    }
}
