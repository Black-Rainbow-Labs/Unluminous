//! Text `egui` lays out, rasterised at the size it is **seen** at rather than the size it is laid out at.
//!
//! `services::text_renderer::Crispness` is this same idea for the glyphs Unluminous rasterises itself,
//! which is the editor and the terminal. Everything else in the window draws its words through `egui`'s
//! own font atlas — a node's title, a browser node's toolbar, a folder node's rows, the chat pane and the
//! whole Agent Tasks board — and until `task-1945` none of it was covered.
//!
//! ## Why any of this is needed
//!
//! A canvas node is drawn into an `egui` layer carrying the camera as a `TSTransform`, and `epaint`
//! applies that transform to the **finished shape**: `TextShape::transform` multiplies the glyph quads
//! and leaves the texture coordinates alone. So a glyph rasterised for a 12.5 point layout and composited
//! at 1.75 is a bitmap magnified by 1.75, and at 0.61 it is the same bitmap resampled down. `task-1945`
//! was reported from a camera at 0.61 and the board's cards there were a grey mush.
//!
//! ## What this does instead
//!
//! Lay the text out at `size × scale`, then scale the finished shape by `1 / scale` about the position it
//! was asked to be drawn at. The mesh ends up exactly where a galley laid out at the caller's own size
//! would have put it, and its glyphs come out of the atlas entry for `size × scale`. The layer's own
//! transform then multiplies by the camera, and the glyph is composited at the number of pixels it was
//! rasterised with.
//!
//! **Nothing here ever moves anything.** A caller measures with [`Text::size`], which divides back, and
//! draws with [`galley`], which divides back. The only thing a scale other than one changes is which entry
//! of the font atlas the glyphs are read from — so a scale left on by mistake makes something sharper than
//! it needs to be and cannot make it the wrong size or the wrong shape.
//!
//! ## The scale is ambient, the way the theme is
//!
//! It is set once around a node and read by every one of the hundred-odd places that draw words, which is
//! the same bargain `TextRenderer::composite_at` makes and for the same reason: the alternative is a
//! parameter on every function in `components/` that a node can reach, including the ones a modal and a
//! settings page share with it. Held per thread, like the active theme — see this module's parent — so a
//! test that sets one cannot reach another test's frame.

use std::cell::Cell;
use std::sync::Arc;

use egui::epaint::text::LayoutJob;
use egui::epaint::{Galley, TextShape};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

use crate::services::text_renderer::Crispness;

thread_local! {
    /// The quantised scale text is being composited at on this thread. One everywhere but inside a node.
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// The scale text is being composited at right now.
pub fn current() -> f32 {
    SCALE.with(Cell::get)
}

/// Whether text is being composited at its own size, which is everything outside a canvas node.
pub fn is_exact() -> bool {
    current() == 1.0
}

/// Rasterise at the size a layer transform of `scale` will composite at, until [`restore`] puts it back.
///
/// Answers what it was, which is what the caller hands back. **Set and put back around a node's drawing**
/// rather than held by a guard, so this reads the same way as the pair beside it in `app::space` that does
/// the same job for `services::text_renderer`.
///
/// The number is quantised by [`Crispness`] rather than here, so the two text engines put the same ladder
/// of sizes into their atlases and a pinch settles onto one of them in both.
pub fn composite_at(scale: f32) -> f32 {
    let was = current();
    SCALE.with(|cell| cell.set(Crispness::at(scale).scale()));
    was
}

/// Put the scale back to what [`composite_at`] answered with.
pub fn restore(was: f32) {
    SCALE.with(|cell| cell.set(was));
}

/// Some laid-out text that **measures in the caller's own points** whatever it was rasterised at.
///
/// Everything a caller does with a galley in this codebase is ask its size and then draw it, so those are
/// the two things this offers. The galley inside is laid out at `size × scale` and nobody outside this
/// module should see that.
#[derive(Clone)]
pub struct Text {
    galley: Arc<Galley>,
    scale: f32,
}

impl Text {
    /// How much room the words take, in the caller's own points.
    pub fn size(&self) -> Vec2 {
        self.galley.size() / self.scale
    }

    /// The rectangle they take from `at`, in the caller's own points.
    pub fn rect(&self, at: Pos2) -> Rect {
        Rect::from_min_size(at, self.size())
    }

    /// Whether there is anything to draw.
    pub fn is_empty(&self) -> bool {
        self.galley.is_empty()
    }
}

/// `Painter::layout_no_wrap`, rasterised at the size it is seen at.
pub fn layout_no_wrap(painter: &Painter, text: String, font: FontId, colour: Color32) -> Text {
    layout(painter, text, font, colour, f32::INFINITY)
}

/// `Painter::layout`, rasterised at the size it is seen at.
///
/// **The wrap width is scaled with the font**, which is what keeps the line breaks exactly where they
/// were: a wider box and a proportionally larger font break in the same places.
pub fn layout(
    painter: &Painter,
    text: String,
    font: FontId,
    colour: Color32,
    wrap_width: f32,
) -> Text {
    let scale = current();
    if scale == 1.0 {
        return Text { galley: painter.layout(text, font, colour, wrap_width), scale };
    }
    let font = FontId { size: font.size * scale, family: font.family };
    let wrap_width = if wrap_width.is_finite() { wrap_width * scale } else { wrap_width };
    Text { galley: painter.layout(text, font, colour, wrap_width), scale }
}

/// `Painter::layout_job`, rasterised at the size it is seen at.
///
/// **Every size in the job is scaled, not only the font**, because a `LayoutJob` carries the wrap width,
/// the row height and the letter spacing that decide where the lines break — and a job whose font grew
/// while its box did not would wrap in different places, which is the one thing this must never do.
pub fn layout_job(painter: &Painter, job: LayoutJob) -> Text {
    let scale = current();
    if scale == 1.0 {
        return Text { galley: painter.layout_job(job), scale };
    }
    let mut job = job;
    for section in &mut job.sections {
        section.format.font_id.size *= scale;
        section.format.extra_letter_spacing *= scale;
        if let Some(height) = section.format.line_height.as_mut() {
            *height *= scale;
        }
    }
    if job.wrap.max_width.is_finite() {
        job.wrap.max_width *= scale;
    }
    job.first_row_min_height *= scale;
    Text { galley: painter.layout_job(job), scale }
}

/// `Painter::galley`, put back into the rectangle the caller's own points gave it.
pub fn galley(painter: &Painter, at: Pos2, text: Text, colour: Color32) {
    if text.is_empty() {
        return;
    }
    if text.scale == 1.0 {
        painter.galley(at, text.galley, colour);
        return;
    }
    // Built at the origin and then transformed, because `TSTransform` scales about the origin: starting
    // there makes the translation the position outright, so the words land exactly where a galley laid out
    // at the caller's own size would have landed.
    let mut shape = TextShape::new(Pos2::ZERO, text.galley, colour);
    shape.transform(egui::emath::TSTransform {
        scaling: 1.0 / text.scale,
        translation: at.to_vec2(),
    });
    painter.add(shape);
}

/// `Painter::text`, rasterised at the size it is seen at. Answers the rectangle the words took.
pub fn text(
    painter: &Painter,
    at: Pos2,
    anchor: Align2,
    text: impl ToString,
    font: FontId,
    colour: Color32,
) -> Rect {
    let laid = layout_no_wrap(painter, text.to_string(), font, colour);
    let rect = anchor.anchor_size(at, laid.size());
    galley(painter, rect.min, laid, colour);
    rect
}

/// The four `Painter` methods that draw words, rasterised at the size they are seen at.
///
/// **A trait rather than four free functions**, so a call site keeps the shape it had — `painter.text(…)`
/// becomes `painter.crisp_text(…)` and nothing else about the line moves. There are about a hundred of
/// them across the components a canvas node can reach, and a rewrite that also moved the receiver would be
/// a hundred chances to change what is drawn rather than only how sharply.
pub trait CrispPainter {
    /// [`text`], as a method.
    fn crisp_text(
        &self,
        at: Pos2,
        anchor: Align2,
        text: impl ToString,
        font: FontId,
        colour: Color32,
    ) -> Rect;
    /// [`layout_no_wrap`], as a method.
    fn crisp_layout_no_wrap(&self, text: String, font: FontId, colour: Color32) -> Text;
    /// [`layout`], as a method.
    fn crisp_layout(&self, text: String, font: FontId, colour: Color32, wrap_width: f32) -> Text;
    /// [`layout_job`], as a method.
    fn crisp_layout_job(&self, job: LayoutJob) -> Text;
    /// [`galley`], as a method.
    fn crisp_galley(&self, at: Pos2, text: Text, colour: Color32);
}

impl CrispPainter for Painter {
    fn crisp_text(
        &self,
        at: Pos2,
        anchor: Align2,
        words: impl ToString,
        font: FontId,
        colour: Color32,
    ) -> Rect {
        text(self, at, anchor, words, font, colour)
    }

    fn crisp_layout_no_wrap(&self, words: String, font: FontId, colour: Color32) -> Text {
        layout_no_wrap(self, words, font, colour)
    }

    fn crisp_layout(&self, words: String, font: FontId, colour: Color32, wrap_width: f32) -> Text {
        layout(self, words, font, colour, wrap_width)
    }

    fn crisp_layout_job(&self, job: LayoutJob) -> Text {
        layout_job(self, job)
    }

    fn crisp_galley(&self, at: Pos2, words: Text, colour: Color32) {
        galley(self, at, words, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scale of one is the window as it was: the same galley, the same size, and no transform.
    ///
    /// This is what makes the change safe to make everywhere at once. Every pane, every modal and every
    /// one of the accepted screenshots draws at a scale of one, so nothing outside a zoomed canvas moves.
    #[test]
    fn text_composited_at_its_own_size_is_left_exactly_alone() {
        assert!(is_exact(), "a thread that has drawn no node is at one");
        let was = composite_at(1.0);
        assert!(is_exact(), "and a camera at one is the same thing");
        restore(was);
    }

    /// The scale is the ladder `Crispness` decides, so the two atlases are asked for the same sizes.
    #[test]
    fn the_scale_is_the_same_ladder_the_other_atlas_uses() {
        for zoom in [0.25_f32, 0.61, 1.0, 1.1, 1.75, 2.5] {
            let was = composite_at(zoom);
            assert_eq!(
                current(),
                Crispness::at(zoom).scale(),
                "at a camera of {zoom} the two text engines disagreed about the raster size"
            );
            assert!(
                current() >= zoom - 0.001,
                "and it is never rasterised smaller than it is seen"
            );
            restore(was);
        }
        assert!(is_exact(), "and it is put back");
    }

    /// Measuring divides back, so a caller laying words out gets the room they take at its own size.
    #[test]
    fn measuring_answers_in_the_callers_own_points() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let font = FontId::proportional(12.0);
            let plain = layout_no_wrap(
                &painter,
                "Base of Infinite Space".to_owned(),
                font.clone(),
                Color32::WHITE,
            );
            let at_one = plain.size();

            let was = composite_at(2.0);
            let zoomed =
                layout_no_wrap(&painter, "Base of Infinite Space".to_owned(), font, Color32::WHITE);
            let at_two = zoomed.size();
            restore(was);

            assert!(
                (at_one.x - at_two.x).abs() < at_one.x * 0.02,
                "the words take the same room at either raster size: {at_one:?} against {at_two:?}"
            );
            assert!((at_one.y - at_two.y).abs() < at_one.y * 0.02);
        });
        // egui insists a pass's texture changes are taken or cleared before the output is dropped.
        output.textures_delta.clear();
    }
}
