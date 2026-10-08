//! Text `egui` lays out, rasterised at the size it is **seen** at rather than the size it is laid out at.
//!
//! `services::text_renderer::Crispness` is this same idea for the glyphs Unluminous rasterises itself,
//! which is the editor and the terminal. Everything else in the window draws its words through `egui`'s
//! own font atlas — a node's title, a browser node's toolbar, a folder node's rows, the chat pane, the whole
//! Agent Tasks board, `rux`'s labels and every `egui::TextEdit`.
//!
//! ## Why any of this is needed
//!
//! A canvas node and a zoomed modal are drawn into an `egui` layer carrying a `TSTransform`, and `epaint`
//! applies that transform to the **finished shape**: `TextShape::transform` multiplies the glyph quads and
//! leaves the texture coordinates alone. So a glyph rasterised for a 12.5 point layout and composited at
//! 1.75 is a bitmap magnified by 1.75, and at 0.61 it is the same bitmap resampled down. `task-1945` was
//! reported from a camera at 0.61 and the board's cards there were a grey mush.
//!
//! ## What this does instead
//!
//! Once a transformed layer has been drawn, [`sharpen_the_text_in`] swaps every `TextShape` in it for the
//! same job laid out at `size × scale` and scaled back down by `1 / scale` about its own position. The layer's
//! transform then multiplies by the zoom, and when `scale` is the zoom itself the glyph is composited one
//! texel to one pixel, with every row of the galley on a whole pixel.
//!
//! **Nothing moves.** The widgets were laid out, and their carets and selections measured, from the original
//! galleys before the pass runs. What is swapped is how a shape is drawn, not where.
//!
//! ## Why a pass over the finished layer rather than a scale at each caller (`task-2216`)
//!
//! `task-1945` did this the other way: an ambient scale set around each node, read by about a hundred and
//! sixty call sites that drew through the helpers below. That covered the words Unluminous lays out itself
//! and nothing else, so a `TextEdit` inside a node, a `rux` label and anything drawn with `Painter::galley`
//! were still magnified bitmaps. And the scale was the quarter step above the zoom, so a camera at 1.1 drew
//! every word at 0.88 of the size it was rasterised at, which is soft. The pass `task-2198` wrote for zoomed
//! modals reaches every piece of text in a layer, so the canvas uses it too, with the zoom exactly once the
//! camera has stopped (see `Crispness`). The helpers below are kept, because a hundred and sixty call sites
//! use their shape, and they are now plain layout.

use std::sync::Arc;

use egui::epaint::text::LayoutJob;
use egui::epaint::{Galley, TextShape};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

/// Some laid-out text, measured in the caller's own points.
///
/// Everything a caller does with a galley in this codebase is ask its size and then draw it, so those are
/// the two things this offers.
#[derive(Clone)]
pub struct Text {
    galley: Arc<Galley>,
}

impl Text {
    /// How much room the words take, in the caller's own points.
    pub fn size(&self) -> Vec2 {
        self.galley.size()
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

/// `Painter::layout_no_wrap`.
pub fn layout_no_wrap(painter: &Painter, text: String, font: FontId, colour: Color32) -> Text {
    layout(painter, text, font, colour, f32::INFINITY)
}

/// `Painter::layout`.
pub fn layout(
    painter: &Painter,
    text: String,
    font: FontId,
    colour: Color32,
    wrap_width: f32,
) -> Text {
    Text { galley: painter.layout(text, font, colour, wrap_width) }
}

/// `Painter::layout_job`.
pub fn layout_job(painter: &Painter, job: LayoutJob) -> Text {
    Text { galley: painter.layout_job(job) }
}

/// Where to put the top of one line of `font` so that its capitals are centred on `centre_y`.
///
/// `task-2198`: *"Icons aren't vertically aligned with text."* A row of the explorer centred its name's
/// galley on the row, and a galley is as tall as the font's whole line: the room above for accents, the
/// room below for descenders, and the line gap. The letters a name is read by, the capitals and the tall
/// strokes of `l`, `d` and `t`, run from the top of a capital down to the baseline, and that band sits
/// above the middle of the galley. Measured with the face this window ships, a 12.5 point name's capitals
/// were centred about two and a half points above the icon beside it, which was centred on the row.
///
/// So a name that sits beside an icon is placed by its capitals instead. The band is measured off the font
/// itself, from an `H`, so every row of a list is placed the same whatever letters are in it: centring by
/// each name's own ink would lift `mcp.json` and drop `README.md`.
pub fn top_centring_capitals(painter: &Painter, font: &FontId, centre_y: f32) -> f32 {
    let probe = painter.layout_no_wrap("H".to_owned(), font.clone(), Color32::WHITE);
    match capital_band(&probe) {
        Some((cap, baseline)) => centre_y - (cap + baseline) / 2.0,
        None => centre_y - probe.size().y / 2.0,
    }
}

/// From the top of a galley, where its first glyph's ink starts and where its baseline is.
///
/// `Glyph::pos` is the baseline relative to its row, and `uv_rect.offset` is where the ink starts relative
/// to that, which is the sum `epaint`'s own tessellator makes when it places the glyph.
fn capital_band(galley: &Galley) -> Option<(f32, f32)> {
    let row = galley.rows.first()?;
    let glyph = row.row.glyphs.first()?;
    let baseline = row.pos.y + glyph.pos.y;
    let cap = baseline + glyph.uv_rect.offset.y;
    (baseline > cap).then_some((cap, baseline))
}

/// Lay every piece of text in `layers` out again `scale` times larger, and draw it `scale` times smaller.
///
/// `task-2198` wrote this for a zoomed modal and `task-2216` gave it the canvas's nodes as well. A layer
/// drawn through a transform has its finished shapes multiplied by that transform, so a word laid out at 14
/// points in a modal at 1.6 was a 14 point bitmap magnified by 1.6. So once the layers have been drawn, each
/// `TextShape` in them is swapped for the same job laid out at the size it is seen at and scaled back down
/// about its own position. Called with the transform's own zoom as `scale`, the glyphs are composited one
/// texel to one pixel. Any visible layer drawn through the same transform as one of `layers`, such as a `rux`
/// dropdown opened from inside a node or a modal, is done too.
///
/// **Each layer is done once.** The canvas's nodes all share one transform, so asking about each node's
/// layer in turn would find the others through that transform and lay them out larger twice.
///
/// Nothing moves. What is swapped is how a shape is drawn, not where: the widgets were laid out and their
/// carets and selections measured from the original galleys before this runs.
pub fn sharpen_the_text_in(ctx: &egui::Context, layers: &[egui::LayerId], scale: f32) {
    if (scale - 1.0).abs() < 0.001 || layers.is_empty() {
        return;
    }
    let transforms: Vec<egui::emath::TSTransform> =
        layers.iter().filter_map(|layer| ctx.layer_transform_to_global(*layer)).collect();
    let mut every: Vec<egui::LayerId> = layers.to_vec();
    let visible = ctx.memory(|memory| memory.areas().visible_layer_ids());
    for other in visible {
        let shares = ctx
            .layer_transform_to_global(other)
            .is_some_and(|transform| transforms.contains(&transform));
        if shares && !every.contains(&other) {
            every.push(other);
        }
    }
    let mut done: Vec<egui::LayerId> = Vec::with_capacity(every.len());
    for one in every {
        if done.contains(&one) {
            continue;
        }
        done.push(one);
        sharpen_one_layer(ctx, one, scale);
    }
}

/// [`sharpen_the_text_in`] for one layer.
fn sharpen_one_layer(ctx: &egui::Context, layer: egui::LayerId, scale: f32) {
    // Read out, laid out and put back in three steps, because the fonts and the shape lists are both
    // behind the context's one lock and laying out while holding the list would wait on itself.
    let mut found: Vec<(usize, egui::Shape)> = Vec::new();
    ctx.graphics_mut(|graphics| {
        if let Some(list) = graphics.get_mut(layer) {
            for (index, clipped) in list.all_entries().enumerate() {
                if holds_text(&clipped.shape) {
                    found.push((index, clipped.shape.clone()));
                }
            }
        }
    });
    if found.is_empty() {
        return;
    }
    for (_, shape) in &mut found {
        sharpen(ctx, shape, scale);
    }
    ctx.graphics_mut(|graphics| {
        if let Some(list) = graphics.get_mut(layer) {
            for (index, shape) in found {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| clipped.shape = shape);
            }
        }
    });
}

/// Whether a shape is text, or a group with text somewhere in it.
fn holds_text(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Text(_) => true,
        egui::Shape::Vec(shapes) => shapes.iter().any(holds_text),
        _ => false,
    }
}

/// Swap every `TextShape` in `shape` for its job laid out `scale` times larger and drawn `scale` times
/// smaller about its own position.
fn sharpen(ctx: &egui::Context, shape: &mut egui::Shape, scale: f32) {
    match shape {
        egui::Shape::Text(text) => {
            let mut job = (*text.galley.job).clone();
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
            let larger = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            let mut sharp = TextShape {
                galley: larger,
                underline: egui::Stroke::new(text.underline.width * scale, text.underline.color),
                ..text.clone()
            };
            let at = sharp.pos.to_vec2();
            sharp.transform(egui::emath::TSTransform {
                scaling: 1.0 / scale,
                translation: at * (1.0 - 1.0 / scale),
            });
            *text = sharp;
        }
        egui::Shape::Vec(shapes) => shapes.iter_mut().for_each(|one| sharpen(ctx, one, scale)),
        _ => {}
    }
}

/// `Painter::galley`.
pub fn galley(painter: &Painter, at: Pos2, text: Text, colour: Color32) {
    if text.is_empty() {
        return;
    }
    painter.galley(at, text.galley, colour);
}

/// `Painter::text`, answering the rectangle the words took.
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

/// The `Painter` methods that draw words, as `task-1945` named them.
///
/// **A trait rather than free functions**, so a call site keeps the shape it had — `painter.text(…)` became
/// `painter.crisp_text(…)` and nothing else about the line moved. Since `task-2216` they lay text out
/// exactly as the `Painter` methods do, and the sharpening is [`sharpen_the_text_in`]'s.
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

    /// The capitals of a name land on the middle of the row, at every size. `task-2198`.
    ///
    /// How far that is from the galley's own middle depends on the face: the one the report came from put
    /// its capitals two and a half points above it, and egui's built-in face, which this test has, is within
    /// a point either way. So what is checked is where the capitals end up, not which way they moved.
    #[test]
    fn a_names_capitals_are_centred_on_the_row_it_is_drawn_in() {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let painter = Painter::new(context.clone(), egui::LayerId::background(), Rect::EVERYTHING);
        for size in [11.0_f32, 12.5, 16.0, 24.0] {
            let font = FontId::proportional(size);
            let probe = painter.layout_no_wrap("H".to_owned(), font.clone(), Color32::WHITE);
            let (cap, baseline) = capital_band(&probe).expect("an H has ink");
            let row_middle = 100.0;
            let top = top_centring_capitals(&painter, &font, row_middle);
            let capitals_middle = top + (cap + baseline) / 2.0;
            assert!((capitals_middle - row_middle).abs() < 0.01, "at {size}: {capitals_middle}");
            assert!(baseline - cap > size * 0.5, "an H is most of a line tall at {size}");
        }
    }

    /// Draw `words` at 12 points into one layer per transform, sharpen them at `scale`, and answer every
    /// `TextShape` as it leaves the frame, with the layer transform already applied.
    fn drawn_through(transforms: &[egui::emath::TSTransform], scale: f32) -> Vec<TextShape> {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            let layers: Vec<egui::LayerId> = (0..transforms.len())
                .map(|index| {
                    egui::LayerId::new(egui::Order::Background, egui::Id::new(("probe", index)))
                })
                .collect();
            for (layer, transform) in layers.iter().zip(transforms) {
                ctx.set_transform_layer(*layer, *transform);
                let painter = Painter::new(ctx.clone(), *layer, Rect::EVERYTHING);
                painter.text(
                    Pos2::new(20.3, 40.7),
                    Align2::LEFT_TOP,
                    "Agent Tasks · Sprint 12",
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
            }
            sharpen_the_text_in(&ctx, &layers, scale);
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|clipped| match clipped.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    /// How many window pixels each texel of every glyph covers, across and down.
    ///
    /// A row's mesh holds the glyph quads in the galley's own points and their corners in the font atlas in
    /// texels, which is what `epaint` writes before it normalises them. One means the glyph is drawn exactly
    /// as it was rasterised.
    fn pixels_per_texel(text: &TextShape) -> Vec<(f32, f32)> {
        let mut ratios = Vec::new();
        for row in &text.galley.rows {
            for quad in row.visuals.mesh.vertices.chunks(4) {
                let (Some(first), Some(last)) = (quad.first(), quad.get(3)) else { continue };
                let drawn = last.pos - first.pos;
                let texels = last.uv - first.uv;
                if texels.x.abs() > 0.5 && texels.y.abs() > 0.5 {
                    ratios.push((drawn.x / texels.x, drawn.y / texels.y));
                }
            }
        }
        ratios
    }

    /// A layer at a zoom the quarter steps miss is drawn one texel to one pixel, on whole pixels. `task-2216`.
    ///
    /// *"Our panels like agent tasks, agent chat, realm, etc show pixelated font at certain zoom levels."* A
    /// camera at 1.1 used to rasterise at the quarter step above it, 1.25, and draw every glyph at 0.88 of
    /// its bitmap. Sharpened with the zoom itself, every glyph covers exactly as many pixels as it has texels,
    /// and every row of it starts on a whole pixel whatever fraction of a pixel the camera is panned to.
    #[test]
    fn a_layer_at_any_zoom_is_drawn_one_texel_to_one_pixel() {
        for zoom in [0.61_f32, 0.9, 1.1, 1.33, 1.77, 2.3] {
            let camera = egui::emath::TSTransform::new(Vec2::new(13.37, 7.21), zoom);
            let shapes = drawn_through(&[camera], zoom);
            assert_eq!(shapes.len(), 1);
            let ratios = pixels_per_texel(&shapes[0]);
            assert!(!ratios.is_empty(), "the words have glyphs at {zoom}");
            for (across, down) in ratios {
                assert!(
                    (across - 1.0).abs() < 0.001 && (down - 1.0).abs() < 0.001,
                    "at a camera of {zoom} a texel covered {across} by {down} pixels"
                );
            }
            for row in &shapes[0].galley.rows {
                let top = row.pos.y;
                assert!((top - top.round()).abs() < 0.01, "a row at {zoom} starts {top} down");
            }
        }
    }

    /// The quarter step above the zoom is what made the letters soft, and this test can tell.
    ///
    /// Asserted so the test above cannot pass by measuring nothing: drawn with the ladder a glide uses, a
    /// camera at 1.1 puts 0.88 of a pixel under each texel.
    #[test]
    fn the_quarter_step_above_the_zoom_resamples_every_glyph() {
        let camera = egui::emath::TSTransform::new(Vec2::ZERO, 1.1);
        let shapes =
            drawn_through(&[camera], crate::services::text_renderer::Crispness::ladder(1.1, false));
        let (across, _) = pixels_per_texel(&shapes[0])[0];
        assert!((across - 0.88).abs() < 0.01, "{across}");
    }

    /// Two layers drawn through one transform are each laid out larger once, not once for each other.
    ///
    /// The canvas's nodes all share the camera, so asking about each node's layer found the others through
    /// it. Done twice, a word at 12 points would have been laid out at 14.52 rather than 13.2.
    #[test]
    fn layers_that_share_a_transform_are_each_sharpened_once() {
        let camera = egui::emath::TSTransform::new(Vec2::new(4.5, 2.25), 1.1);
        let shapes = drawn_through(&[camera, camera], 1.1);
        assert_eq!(shapes.len(), 2);
        for shape in &shapes {
            let size = shape.galley.job.sections[0].format.font_id.size;
            assert!((size - 13.2).abs() < 0.001, "laid out at {size}");
        }
    }

    /// A scale of one changes nothing, which is every pane and every modal nobody has zoomed.
    #[test]
    fn text_composited_at_its_own_size_is_left_exactly_alone() {
        let shapes = drawn_through(&[egui::emath::TSTransform::IDENTITY], 1.0);
        let size = shapes[0].galley.job.sections[0].format.font_id.size;
        assert_eq!(size, 12.0);
    }
}
