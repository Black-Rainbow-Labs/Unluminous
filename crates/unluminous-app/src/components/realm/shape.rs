//! Drawing a shape node. `task-2284`.
//!
//! `services::realm::shape` is the arithmetic: the outline, where the words go and where a wire joins. This is
//! the picture and the pointer: a shape is drawn into the node's own layer, in world points, the way the
//! design draws it. The theme's card gradient or the shape's own colour, a lit shadow up and to the left and a
//! dark one down and to the right, an accent outline while it is chosen, its words, its eight grips while it
//! is chosen, and a port at each point a wire can join it.
//!
//! **A shape is raised exactly as every other node is**: the same three shadows, the same lit edge inside its
//! top and left, and on a light ground the same hairline, all read from `vello_canvas::elevation` with
//! `Lift::Medium`, which is what `components::realm::frame` raises a node's card with. The numbers are in
//! screen points there, so they are divided by the zoom here and a shape's shadow is the same width on the
//! screen as a node's at any zoom.
//!
//! **What differs is how they are painted**, because the pane's decoration canvas draws blurred rounded
//! rectangles and a diamond's shadow is a diamond. Each shadow is the outline moved by its offset and blurred
//! by its standard deviation: a band of rings either side of the outline whose opacity follows the normal
//! distribution, which is what a Gaussian blur of an edge is. They are painted under the fill in the shape's
//! own layer.
//!
//! **Every colour comes from the theme as it is now**: `chrome_kit::chat()` is `rux::theme::Chat::lit_from` of
//! the theme's ground, so a shape follows a theme change on the next frame, Unluminous Light included.

use egui::epaint::{Mesh, PathShape, Vertex, WHITE_UV};
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_core::Rgba;

use super::{
    look_accent, paint_a_port, port_fade, scale_of, show_the_grips, Framing, Look, NodeOutcome,
    PORT, PORT_REVEAL, PORT_SMALLEST_HIT,
};
use crate::components::controls::TextBoxes;
use crate::services::realm::shape::{self, Shape, ShapeKind};
use crate::services::realm::{Node, NodeId};
use crate::theme::color;

/// Where the rings of a blurred edge are, in standard deviations from the edge, inside negative. The blur is
/// all but complete two deviations in and two and a half out.
const RINGS: [f32; 9] = [-2.0, -1.5, -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.5];
/// How big a grip square is, in world points.
const GRIP: f32 = 9.0;

/// What the frame needs to know about one shape beyond what every node's frame does.
#[derive(Debug, Clone, Copy)]
pub struct ShapeFraming<'a> {
    pub node: Framing<'a>,
    /// Which of this shape's points a wire already joins, which draws them lit.
    pub joined: &'a [usize],
    /// The point a wire in the air would land on, when it would land on this shape.
    pub landing_point: Option<usize>,
    /// The colour to draw it in: its own, or the colour picker's while the picker is open on it.
    pub fill: Option<Rgba>,
}

/// Draw one shape node and report what was done to it.
///
/// `editing` is the shape's words while they are being typed into, which puts a text box where the words go;
/// the words the box ends with are in it afterwards. What a press, a drag and a right click did is in the
/// outcome, exactly as a node's own frame reports it, plus `edit` for a double click and `finished` for the
/// end of typing.
pub fn frame(
    ui: &mut egui::Ui,
    node: &Node,
    held: &Shape,
    framing: ShapeFraming<'_>,
    look: Look<'_>,
    editing: Option<&mut String>,
) -> NodeOutcome {
    let mut outcome = NodeOutcome::default();
    let rect = node.rect();
    let scale = scale_of(rect, framing.node.on_screen).max(0.01);
    paint(
        ui.painter(),
        held.shape,
        rect,
        framing.fill,
        framing.node.chosen,
        framing.node.keyboard,
        scale,
    );

    // The shape itself takes a press, a drag, a double click and a right click. Added first, so the words,
    // the grips and the ports added after it take the points they cover.
    let response =
        ui.interact(rect, egui::Id::new(("realm-shape", node.id)), Sense::click_and_drag());
    let name = name_of(held);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Node: {name}"))
    });
    if response.dragged() && editing.is_none() {
        outcome.moved = Some(response.drag_delta());
    }
    if response.clicked() || response.drag_started() {
        outcome.chose = true;
    }
    if response.double_clicked() {
        outcome.edit = true;
    }
    if response.secondary_clicked() {
        outcome.menu = response.interact_pointer_pos().or_else(|| response.hover_pos()).map(|at| {
            match ui.ctx().layer_transform_to_global(ui.layer_id()) {
                Some(out) => out * at,
                None => at,
            }
        });
    }
    if response.hovered() && editing.is_none() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }

    let room = shape::text_rect(held.shape, rect);
    let ink = ink_for(framing.fill);
    match editing {
        Some(text) => {
            let (finished, focused) = type_into(ui, node.id, held, room, ink, text);
            outcome.finished = finished;
            outcome.typing = focused;
        }
        None => paint_the_words(ui.painter(), held, rect, room, ink),
    }

    if framing.node.chosen && !outcome.typing {
        paint_the_grips(ui.painter(), rect, scale);
    }
    show_the_grips(ui, node, framing.node, &mut outcome);
    if outcome.resized.is_some() {
        outcome.square = ui.input(|input| input.modifiers.shift);
    }
    show_the_points(ui, node, held, framing, look, scale, &mut outcome);
    outcome
}

/// What a shape is called on the screen and in the accessibility tree: the first line of its words, or the
/// name of the shape when it has none.
pub fn name_of(held: &Shape) -> String {
    let first = held.text.lines().map(str::trim).find(|line| !line.is_empty());
    match first {
        Some(line) => line.chars().take(40).collect(),
        None => held.shape.label().to_owned(),
    }
}

/// The text box the words are typed into, while they are. Answers whether typing finished this frame, and
/// whether the box holds the keyboard.
///
/// **Escape, Ctrl+Enter or a click outside the shape finishes**, which is the ticket's item 6. Escape and a
/// click outside both take the keyboard from the box, which is egui's own rule for a text box; Ctrl+Enter is
/// taken out of the frame before the box sees it, so it does not also put a line break in the words.
fn type_into(
    ui: &mut egui::Ui,
    node: NodeId,
    held: &Shape,
    room: Rect,
    ink: Color32,
    text: &mut String,
) -> (bool, bool) {
    let id = text_box_id(node);
    let had_it = ui.memory(|memory| memory.has_focus(id));
    let done = had_it
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
    let font = FontId::proportional(held.size);
    let response = ui.text_box_at(
        room,
        egui::TextEdit::multiline(text)
            .id(id)
            .font(font)
            .text_color(ink)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .horizontal_align(held.align.egui())
            .vertical_align(held.valign.egui())
            .desired_width(room.width())
            .min_size(room.size()),
    );
    let named = format!("Text in {}", held.shape.label());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, named.clone()));
    if done {
        ui.memory_mut(|memory| memory.surrender_focus(id));
    }
    (done || response.lost_focus(), response.has_focus() && !done)
}

/// The id of the text box a shape's words are typed into, which the window hands the keyboard to.
pub fn text_box_id(node: NodeId) -> egui::Id {
    egui::Id::new(("realm-shape-text", node))
}

/// The colour a shape's words are set in: the theme's own, unless the shape's colour is light enough on a
/// dark theme, or dark enough on a light one, that the theme's words would not read on it.
///
/// **Then the words take the colour of the editor's own ground**, which is the colour that reads on the
/// opposite of the ground: dark on a light fill under a dark theme, light on a dark fill under a light one.
/// It is a role of the theme rather than a colour chosen here, so a theme says what it is.
pub fn ink_for(fill: Option<Rgba>) -> Color32 {
    let Some(fill) = fill else { return color::text() };
    let card = super::card_colour();
    let alpha = f32::from(fill.a) / 255.0;
    let channel = |own: u8, under: u8| f32::from(own) * alpha + f32::from(under) * (1.0 - alpha);
    let light = (0.299 * channel(fill.r, card.r())
        + 0.587 * channel(fill.g, card.g())
        + 0.114 * channel(fill.b, card.b()))
        / 255.0;
    match crate::theme::is_dark() {
        true if light > 0.62 => color::editor(),
        false if light < 0.42 => color::editor(),
        _ => color::text(),
    }
}

/// The shape's words, laid out in the part of it that holds them, or its name in the quiet colour when it has
/// none, which is the design's hint.
fn paint_the_words(painter: &egui::Painter, held: &Shape, rect: Rect, room: Rect, ink: Color32) {
    let (words, colour) = match held.text.trim().is_empty() {
        true => (held.shape.label().to_owned(), color::text_faint()),
        false => (held.text.clone(), ink),
    };
    let mut job = egui::text::LayoutJob::single_section(
        words,
        egui::TextFormat {
            font_id: FontId::proportional(held.size),
            color: colour,
            ..Default::default()
        },
    );
    job.wrap.max_width = room.width();
    job.halign = held.align.egui();
    let galley = painter.layout_job(job);
    let height = galley.size().y;
    let x = match held.align {
        shape::Align::Left => room.left(),
        shape::Align::Centre => room.center().x,
        shape::Align::Right => room.right(),
    };
    let y = match held.valign {
        shape::VAlign::Top => room.top(),
        shape::VAlign::Middle => room.center().y - height / 2.0,
        shape::VAlign::Bottom => room.bottom() - height,
    };
    let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    clipped.galley(Pos2::new(x, y), galley, colour);
}

/// The shape: its two shadows, its fill, a sticky note's folded corner, and the accent outline while it is
/// chosen.
pub fn paint(
    painter: &egui::Painter,
    kind: ShapeKind,
    rect: Rect,
    fill: Option<Rgba>,
    chosen: bool,
    keyboard: bool,
    scale: f32,
) {
    let chat = crate::components::chrome_kit::chat();
    let outline = shape::outline(kind, rect);
    let kernel = shape::kernel(kind, rect);
    // A shape of a translucent colour casts a fainter shadow, which is what a CSS drop shadow does.
    let solid = fill.map(|fill| f32::from(fill.a) / 255.0).unwrap_or(1.0);
    let surface = super::card_colour();
    let raised = crate::services::vello_canvas::elevation(
        surface,
        crate::services::vello_canvas::Lift::Medium,
    );
    for layer in raised.shadows {
        let by = layer.offset / scale;
        let moved: Vec<Pos2> = outline.iter().map(|point| *point + by).collect();
        let colour = layer.colour.gamma_multiply(solid);
        painter.add(blurred(&moved, kernel + by, layer.blur / scale, colour));
    }
    if let Some(hairline) = raised.hairline {
        painter.add(PathShape::closed_line(outline.clone(), Stroke::new(1.5 / scale, hairline)));
    }
    // About one pixel of feathering on the fill's edge, which is what smooths it at any zoom.
    let edge = 1.0 / scale;
    // **Under glass a shape is glass**, as a node's card is (`components::realm::cover`): the picture behind
    // the window bent through it, and the card's translucent tint over that. With no picture to bend, an
    // opaque ground under the tint does the hiding instead.
    // A rectangle and a rounded rectangle are what `rux` builds glass for already, so they take a node's own
    // glass; every other outline is bent by `glass_in`.
    let bent = crate::theme::is_glass()
        && match shape::corner_radius(kind, rect) {
            Some(radius) => crate::theme::glass::paint_glass(painter, rect, radius),
            None => glass_in(painter, &outline, kernel, rect),
        };
    if crate::theme::is_glass() && !bent {
        painter.add(feathered(&outline, kernel, edge, |_| color::editor()));
    }
    match fill {
        None => {
            let (from, to) = chat.card.axis(rect);
            painter.add(feathered(&outline, kernel, edge, |at| {
                along_gradient(chat.card.from, chat.card.to, from, to, at)
            }));
        }
        Some(own) => {
            let base = Color32::from_rgba_unmultiplied(own.r, own.g, own.b, own.a);
            let (from, to) = chat.card.axis(rect);
            painter
                .add(feathered(&outline, kernel, edge, |at| sheen(base, chat.dark, from, to, at)));
        }
    }
    lit_edge(painter, &outline, kernel, raised.edge, scale);
    if kind == ShapeKind::Sticky {
        let fold = shape::fold_of(rect);
        let corner = [
            Pos2::new(rect.right() - fold, rect.bottom()),
            Pos2::new(rect.right() - fold, rect.bottom() - fold),
            Pos2::new(rect.right(), rect.bottom() - fold),
        ];
        let shade = chat.dark.gamma_multiply(0.35 * solid);
        painter.add(PathShape::convex_polygon(corner.to_vec(), shade, Stroke::NONE));
    }
    if chosen {
        let tint = if keyboard { color::accent() } else { color::accent().gamma_multiply(0.45) };
        painter.add(PathShape::closed_line(outline, Stroke::new(1.5, tint)));
    }
}

/// The colour a gradient has at `at`, where it runs from `from` at the point `start` to `to` at `end`.
fn along_gradient(from: Color32, to: Color32, start: Pos2, end: Pos2, at: Pos2) -> Color32 {
    let along = end - start;
    let t = ((at - start).dot(along) / along.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
    lerp(from, to, t)
}

/// A shape's own colour with the design's sheen over it: a little lighter at the top left and a little
/// darker at the bottom right, which is what keeps a coloured shape looking lit like the rest of the canvas.
fn sheen(base: Color32, dark: Color32, start: Pos2, end: Pos2, at: Pos2) -> Color32 {
    let along = end - start;
    let t = ((at - start).dot(along) / along.length_sq().max(f32::EPSILON)).clamp(0.0, 1.0);
    let [r, g, b, a] = base.to_srgba_unmultiplied();
    let opaque = Color32::from_rgb(r, g, b);
    let tinted = match t < 0.5 {
        // any ground: the design's sheen lightens a shape's own colour at its lit corner whatever is under it
        true => lerp(opaque, Color32::WHITE, 0.08 * (1.0 - 2.0 * t)),
        false => {
            lerp(opaque, Color32::from_rgb(dark.r(), dark.g(), dark.b()), 0.22 * (2.0 * t - 1.0))
        }
    };
    Color32::from_rgba_unmultiplied(tinted.r(), tinted.g(), tinted.b(), a)
}

/// The colour a fraction of the way from one opaque colour to another.
fn lerp(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
}

/// A polygon filled as a fan of triangles from `kernel`, with an edge that fades to nothing over `feather`.
///
/// **A fan, because every outline here is star shaped about its kernel**, the concave star included, which
/// `PathShape`'s convex fill is not. The fading ring outside the edge is what smooths the edge when
/// `feather` is a pixel, and what makes a soft shadow when it is several points.
pub fn feathered(
    points: &[Pos2],
    kernel: Pos2,
    feather: f32,
    colour: impl Fn(Pos2) -> Color32,
) -> Mesh {
    let mut mesh = Mesh::default();
    let count = points.len();
    if count < 3 {
        return mesh;
    }
    let normals = outward_normals(points, kernel);
    mesh.vertices.push(Vertex { pos: kernel, uv: WHITE_UV, color: colour(kernel) });
    for (point, normal) in points.iter().zip(&normals) {
        mesh.vertices.push(Vertex { pos: *point, uv: WHITE_UV, color: colour(*point) });
        mesh.vertices.push(Vertex {
            pos: *point + *normal * feather,
            uv: WHITE_UV,
            color: Color32::TRANSPARENT,
        });
    }
    for index in 0..count {
        let next = (index + 1) % count;
        let (inner, outer) = (1 + index as u32 * 2, 2 + index as u32 * 2);
        let (next_inner, next_outer) = (1 + next as u32 * 2, 2 + next as u32 * 2);
        mesh.indices.extend_from_slice(&[0, inner, next_inner]);
        mesh.indices.extend_from_slice(&[inner, outer, next_inner]);
        mesh.indices.extend_from_slice(&[next_inner, outer, next_outer]);
    }
    mesh
}

/// A polygon's edge blurred by a Gaussian of standard deviation `sigma`: full inside it, fading through half
/// at the outline to nothing outside it, the way `fill_blurred_rounded_rect` fades a rectangle's.
///
/// Rings either side of the outline, moved along each corner's outward direction, at [`RINGS`] deviations,
/// each at the opacity the normal distribution gives it. A ring moved inward is stopped short of the kernel,
/// so the inner corners of a small star do not cross over.
pub fn blurred(points: &[Pos2], kernel: Pos2, sigma: f32, colour: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    let count = points.len();
    if count < 3 {
        return mesh;
    }
    let sigma = sigma.max(0.05);
    let normals = outward_normals(points, kernel);
    mesh.vertices.push(Vertex {
        pos: kernel,
        uv: WHITE_UV,
        color: colour.gamma_multiply(outside_share(RINGS[0])),
    });
    for ring in RINGS {
        let share = outside_share(ring);
        for (point, normal) in points.iter().zip(&normals) {
            let reach = ring * sigma;
            let inward_room = (*point - kernel).length() * 0.9 / normal.length().max(1.0);
            let reach = reach.max(-inward_room);
            mesh.vertices.push(Vertex {
                pos: *point + *normal * reach,
                uv: WHITE_UV,
                color: colour.gamma_multiply(share),
            });
        }
    }
    let count = count as u32;
    for index in 0..count {
        let next = (index + 1) % count;
        mesh.indices.extend_from_slice(&[0, 1 + index, 1 + next]);
        for ring in 0..RINGS.len() as u32 - 1 {
            let (here, out) = (1 + ring * count, 1 + (ring + 1) * count);
            mesh.indices.extend_from_slice(&[here + index, out + index, here + next]);
            mesh.indices.extend_from_slice(&[here + next, out + index, out + next]);
        }
    }
    mesh
}

/// How much of a Gaussian blurred edge is left `deviations` standard deviations outside it: one minus the
/// normal distribution's cumulative share.
fn outside_share(deviations: f32) -> f32 {
    0.5 * (1.0 - error_function(deviations / std::f32::consts::SQRT_2))
}

/// The error function, by Abramowitz and Stegun's formula 7.1.26, which is good to about one part in a
/// hundred thousand and far finer than a shadow's opacity is drawn in.
fn error_function(x: f32) -> f32 {
    let sign = x.signum();
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_6
            + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152 + t * 1.061_405_4))));
    sign * (1.0 - poly * (-x * x).exp())
}

/// Glass cut to a shape's outline: the picture behind the window, bent and split into its three colours the
/// way `rux` bends it under a node's rounded card, over the part of the picture the shape covers. Answers
/// whether there was a picture to bend.
///
/// **`rux` builds glass for a rounded rectangle only**, so this builds the same rings from the outline:
/// a faded ring on the edge, six rings across the bevel moved inward along each corner's direction, and the
/// kernel at the middle, each bent by `rux::render::glass::offset_at`, the same function `rux` bends a card
/// with. The cover and the three passes and the cut at the picture's strips are `rux::render::glass::shapes`'s
/// own, written out for a list of points.
fn glass_in(painter: &egui::Painter, outline: &[Pos2], kernel: Pos2, rect: Rect) -> bool {
    use rux::render::glass::{cut, offset_at, Pane, Sample};
    let ctx = painter.ctx();
    let Some(backdrop) = rux::render::backdrop_for(ctx, painter.layer_id()) else { return false };
    let Some(material) = crate::theme::rux_theme().glass else { return false };
    if backdrop.bands.is_empty() || outline.len() < 3 {
        return false;
    }
    let to_global = ctx.layer_transform_to_global(painter.layer_id()).unwrap_or_default();
    let feather = 1.0 / (ctx.pixels_per_point() * to_global.scaling).max(0.1);
    let pane = Pane {
        rect,
        corners: rux::Corners::all(0.0),
        bevel: material.bevel_for(rect.size()),
        strength: material.strength_for(rect.size()),
    };
    let normals = outward_normals(outline, kernel);
    let ring = |inset: f32, edge: bool| -> Vec<Sample> {
        outline
            .iter()
            .zip(&normals)
            .map(|(point, normal)| {
                let room = (*point - kernel).length() * 0.9 / normal.length().max(1.0);
                let inset = inset.min(room);
                let at = *point - *normal * inset;
                let offset = offset_at(&material, &pane, at, normal.normalized(), -inset);
                Sample { at, offset, feather: edge }
            })
            .collect()
    };
    let mut samples = ring(0.0, true);
    let mut starts = vec![0_usize];
    const STEPS: usize = 6;
    for step in 0..=STEPS {
        starts.push(samples.len());
        let inset = (pane.bevel * step as f32 / STEPS as f32).max(feather);
        samples.extend(ring(inset, false));
    }
    // **Rings between the bevel and the middle as well**, each the last ring drawn in towards the kernel. The
    // lens bends the picture by how far a point is from the middle, and one fan of long triangles from the
    // bevel to the kernel interpolated that across corners and showed as seams along the diagonals.
    let inner: Vec<Pos2> =
        samples[*starts.last().unwrap_or(&0)..].iter().map(|sample| sample.at).collect();
    for share in [0.8_f32, 0.6, 0.4, 0.2] {
        starts.push(samples.len());
        for point in &inner {
            let at = kernel + (*point - kernel) * share;
            let offset = offset_at(&material, &pane, at, Vec2::ZERO, -pane.bevel - 1.0);
            samples.push(Sample { at, offset, feather: false });
        }
    }
    let around = outline.len() as u32;
    let centre = samples.len() as u32;
    samples.push(Sample { at: kernel, offset: Vec2::ZERO, feather: false });
    let mut indices = Vec::new();
    for pair in starts.windows(2) {
        let (outer, inner) = (pair[0] as u32, pair[1] as u32);
        for i in 0..around {
            let j = (i + 1) % around;
            indices.extend([outer + i, outer + j, inner + j, outer + i, inner + j, inner + i]);
        }
    }
    let last = *starts.last().unwrap_or(&0) as u32;
    for i in 0..around {
        indices.extend([last + i, last + (i + 1) % around, centre]);
    }

    let opacity = (backdrop.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    // any ground: what the three passes add their colours onto is darkened by the picture's own opacity
    let dark = Color32::from_black_alpha(opacity);
    let cover = samples
        .iter()
        .map(|sample| Vertex {
            pos: sample.at,
            uv: WHITE_UV,
            color: if sample.feather { Color32::TRANSPARENT } else { dark },
        })
        .collect();
    painter.add(egui::Shape::mesh(Mesh {
        indices: indices.clone(),
        vertices: cover,
        ..Mesh::default()
    }));
    let mut whole = Mesh::default();
    for (channel, share) in material.chroma.iter().enumerate() {
        let first = whole.vertices.len() as u32;
        for sample in &samples {
            let uv = backdrop.picture_at(to_global * (sample.at + sample.offset * *share));
            let mut rgba = [0_u8; 4];
            rgba[channel] = opacity;
            let color = match sample.feather {
                true => Color32::TRANSPARENT,
                false => Color32::from_rgba_premultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
            };
            whole.vertices.push(Vertex { pos: sample.at, uv, color });
        }
        whole.indices.extend(indices.iter().map(|index| index + first));
    }
    for band in &backdrop.bands {
        let mut piece = cut(&whole, band.drawn.0, band.drawn.1);
        if piece.indices.is_empty() {
            continue;
        }
        let span = (band.held.1 - band.held.0).max(f32::EPSILON);
        for vertex in &mut piece.vertices {
            vertex.uv.y = (vertex.uv.y - band.held.0) / span;
        }
        piece.texture_id = band.texture;
        painter.add(egui::Shape::mesh(piece));
    }
    true
}

/// The lit edge inside a raised shape's top and left: a line of light one point in from each side that faces
/// up or left, brightest where it faces the top left corner, which is `Chrome::raised`'s inset lit edge drawn
/// along an outline that is not a rectangle.
fn lit_edge(painter: &egui::Painter, outline: &[Pos2], kernel: Pos2, edge: Color32, scale: f32) {
    let inset = 0.5 / scale;
    for index in 0..outline.len() {
        let (from, to) = (outline[index], outline[(index + 1) % outline.len()]);
        let along = to - from;
        let mut normal = Vec2::new(along.y, -along.x).normalized();
        if normal.dot(from + along / 2.0 - kernel) < 0.0 {
            normal = -normal;
        }
        let facing = (-(normal.x + normal.y) / std::f32::consts::SQRT_2).max(0.0);
        if facing < 0.05 {
            continue;
        }
        painter.line_segment(
            [from - normal * inset, to - normal * inset],
            Stroke::new(1.0 / scale, edge.gamma_multiply(facing)),
        );
    }
}

/// The direction out of a polygon at each of its corners: the two sides' outward normals averaged, and
/// lengthened at a sharp corner so the fading ring keeps its width there, up to a limit.
fn outward_normals(points: &[Pos2], kernel: Pos2) -> Vec<Vec2> {
    let count = points.len();
    let side = |from: Pos2, to: Pos2| {
        let along = to - from;
        let normal = Vec2::new(along.y, -along.x).normalized();
        let middle = from + along / 2.0;
        match normal.dot(middle - kernel) >= 0.0 {
            true => normal,
            false => -normal,
        }
    };
    (0..count)
        .map(|index| {
            let previous = points[(index + count - 1) % count];
            let here = points[index];
            let next = points[(index + 1) % count];
            let (before, after) = (side(previous, here), side(here, next));
            let average = (before + after).normalized();
            if !average.x.is_finite() {
                return before;
            }
            average / average.dot(before).max(0.4)
        })
        .collect()
}

/// The eight grip squares round a chosen shape, which say it can be resized from any of them.
fn paint_the_grips(painter: &egui::Painter, rect: Rect, scale: f32) {
    let size = GRIP / scale.clamp(0.5, 1.5);
    for at in [
        rect.left_top(),
        rect.center_top(),
        rect.right_top(),
        rect.right_center(),
        rect.right_bottom(),
        rect.center_bottom(),
        rect.left_bottom(),
        rect.left_center(),
    ] {
        let square = Rect::from_center_size(at, Vec2::splat(size));
        painter.rect(
            square,
            egui::CornerRadius::same(3),
            color::text(),
            Stroke::new(1.5, color::accent()),
            egui::StrokeKind::Outside,
        );
    }
}

/// A port at each point a wire may join the shape, shown while the pointer is near it, while the shape is
/// chosen, while a wire is in the air, and always where a wire already joins it.
fn show_the_points(
    ui: &mut egui::Ui,
    node: &Node,
    held: &Shape,
    framing: ShapeFraming<'_>,
    look: Look<'_>,
    scale: f32,
    outcome: &mut NodeOutcome,
) {
    let on_screen = |world: Pos2| framing.node.on_screen.min + (world - node.rect().min) * scale;
    let pointer = ui.ctx().pointer_hover_pos();
    let reach = (PORT * 3.0).max(PORT_SMALLEST_HIT / scale.max(0.01));
    for (index, anchor) in shape::anchors(held.shape, node.rect()).iter().enumerate() {
        let hit = Rect::from_center_size(anchor.at, Vec2::splat(reach));
        let response = ui.interact(
            hit,
            egui::Id::new(("realm-shape-point", node.id, index)),
            Sense::click_and_drag(),
        );
        let point = shape::point_name(held.shape, index);
        let name = name_of(held);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Connect from the {point} of {name}"),
            )
        });
        if response.dragged() {
            outcome.wiring = ui.ctx().pointer_latest_pos().and_then(|at| {
                ui.ctx().layer_transform_from_global(ui.layer_id()).map(|back| back * at)
            });
            outcome.point = Some(index);
        }
        if response.drag_started() {
            outcome.chose = true;
        }
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        let joined = framing.joined.contains(&index);
        let target = framing.landing_point == Some(index);
        // **Shown only while the pointer is near it**, as every other node's ports are (`task-2291`). A
        // point with a wire on it used to stay lit for good, so a diagram of joined shapes was covered in
        // dots. A point still shows while a wire is in the air, so there is somewhere to let it go.
        let near = pointer.is_some_and(|at| at.distance(on_screen(anchor.at)) <= PORT_REVEAL);
        let shown = port_fade(
            ui,
            ("realm-shape-point-shown", node.id, index),
            near || framing.node.wire_is_looking || response.hovered() || response.dragged(),
        );
        let fill = match joined || target || response.hovered() || response.dragged() {
            true => look_accent(),
            false => color::text_faint(),
        };
        paint_a_port(ui, look, on_screen(anchor.at), anchor.at, scale, fill, shown);
        if target {
            look.chrome.glow(
                Rect::from_center_size(on_screen(anchor.at), Vec2::splat(PORT * 2.0 * scale)),
                PORT * scale,
                look_accent(),
                6.0 * scale,
            );
        }
    }
}

/// The small drawing of a shape's outline the Add a node dialog shows beside its name, in `colour`.
///
/// The two rectangles are drawn with their own corners here, because at eighteen points across their real
/// outlines looked the same: the rectangle nearly square and the rounded one clearly round.
pub fn mark(painter: &egui::Painter, kind: ShapeKind, area: Rect, colour: Color32) {
    let stroke = Stroke::new(1.6, colour);
    match kind {
        ShapeKind::Rectangle | ShapeKind::Rounded => {
            let corner = if kind == ShapeKind::Rectangle { 1 } else { 5 };
            painter.rect_stroke(
                area,
                egui::CornerRadius::same(corner),
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        _ => {
            painter.add(PathShape::closed_line(shape::outline(kind, area), stroke));
        }
    }
}

/// The Shape row's own mark in the Add a node dialog: a square, a circle and a triangle, which is the
/// design's mark for "a shape" and a picture none of the other kinds use.
pub fn kind_mark(painter: &egui::Painter, centre: Pos2, colour: Color32) {
    let stroke = Stroke::new(1.4, colour);
    let square = Rect::from_min_size(centre + Vec2::new(-7.5, -0.5), Vec2::splat(7.0));
    painter.rect_stroke(square, egui::CornerRadius::same(1), stroke, egui::StrokeKind::Middle);
    painter.circle_stroke(centre + Vec2::new(3.5, -3.5), 3.0, stroke);
    painter.add(PathShape::closed_line(
        vec![
            centre + Vec2::new(3.5, 1.5),
            centre + Vec2::new(6.8, 7.0),
            centre + Vec2::new(0.2, 7.0),
        ],
        stroke,
    ));
}
