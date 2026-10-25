//! Pictures, shapes and the other drawing objects of a sheet, drawn over the cells, and the geometry the
//! pointer needs to select, move, resize and turn them.
//!
//! **The drawing functions decide nothing about the workbook.** [`draw_object`] takes a painter, the
//! rectangle the object's frame is on the screen, the object as the engine models it and what it needs
//! from the window (the theme colours, a way to find a picture's texture, the zoom). [`draw_sheet_objects`]
//! is the loop the grid calls once a frame, and [`hit_test`], [`handle_at`], [`resize_by_handle`] and
//! [`rotation_from_pointer`] are what the pointer handling calls. Those four are pure geometry and are
//! tested with no window.
//!
//! An object whose kind this version does not understand (a form control, SmartArt, an equation) is drawn
//! from its stored picture where it has one, and otherwise as a card naming the kind.

use egui::epaint::TextShape;
use egui::{Color32, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2};
use unluminous_sheet::model::Sheet;
use unluminous_sheet::objects::{
    Anchor, AnchorKind, Dash, DrawingObject, Fill, HAlign, LineEnd, ObjectId, ObjectKind,
    ShapeColor, ShapeData, Stroke as Outline, VAlign,
};

use super::grid::{Frame, Ink};
use crate::services::spreadsheet::Book;

/// One pixel in drawing units.
const EMU_PER_PIXEL: f32 = 9_525.0;
/// How far above a selected object's top edge the turning handle is, in points.
const ROTATE_DISTANCE: f32 = 24.0;
/// A handle's half size, in points.
const HANDLE_RADIUS: f32 = 4.5;

// ------------------------------------------------------------------------------------------------ geometry

/// A point turned clockwise about another, by an angle in degrees.
pub fn rotate_point(p: Pos2, about: Pos2, degrees: f32) -> Pos2 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let d = p - about;
    about + Vec2::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
}

/// The four corners of a frame turned about its centre, from the top left clockwise.
pub fn corners(frame: Rect, degrees: f32) -> [Pos2; 4] {
    let c = frame.center();
    [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom()]
        .map(|p| rotate_point(p, c, degrees))
}

/// Whether a point is inside a frame turned about its centre.
pub fn contains(frame: Rect, degrees: f32, p: Pos2) -> bool {
    frame.contains(rotate_point(p, frame.center(), -degrees))
}

/// The topmost object under a point. `frames` are the objects bottom first, each with its frame on the screen
/// and its rotation in degrees; the answer is the id of the last one containing the point.
pub fn hit_test(frames: &[(ObjectId, Rect, f32)], p: Pos2) -> Option<ObjectId> {
    frames
        .iter()
        .rev()
        .find(|(_, frame, degrees)| contains(*frame, *degrees, p))
        .map(|(id, _, _)| *id)
}

/// A handle on a selected object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    /// The round handle above the top edge that turns the object.
    Rotate,
}

impl Handle {
    pub const RESIZE: [Handle; 8] = [
        Handle::NorthWest,
        Handle::North,
        Handle::NorthEast,
        Handle::East,
        Handle::SouthEast,
        Handle::South,
        Handle::SouthWest,
        Handle::West,
    ];

    /// Which sides the handle moves: -1 the left or top, 1 the right or bottom, 0 neither.
    fn sides(self) -> (i32, i32) {
        match self {
            Handle::NorthWest => (-1, -1),
            Handle::North => (0, -1),
            Handle::NorthEast => (1, -1),
            Handle::East => (1, 0),
            Handle::SouthEast => (1, 1),
            Handle::South => (0, 1),
            Handle::SouthWest => (-1, 1),
            Handle::West => (-1, 0),
            Handle::Rotate => (0, 0),
        }
    }

    /// The pointer shape to show over the handle.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::North | Handle::South => egui::CursorIcon::ResizeVertical,
            Handle::East | Handle::West => egui::CursorIcon::ResizeHorizontal,
            Handle::NorthWest | Handle::SouthEast => egui::CursorIcon::ResizeNwSe,
            Handle::NorthEast | Handle::SouthWest => egui::CursorIcon::ResizeNeSw,
            Handle::Rotate => egui::CursorIcon::Grab,
        }
    }
}

/// Where the nine handles of a frame turned by `degrees` are on the screen.
pub fn handle_positions(frame: Rect, degrees: f32) -> [(Handle, Pos2); 9] {
    let c = frame.center();
    let at = |sx: i32, sy: i32| {
        let p = Pos2::new(
            c.x + sx as f32 * frame.width() / 2.0,
            c.y + sy as f32 * frame.height() / 2.0,
        );
        rotate_point(p, c, degrees)
    };
    let mut out = [(Handle::Rotate, Pos2::ZERO); 9];
    for (i, h) in Handle::RESIZE.iter().enumerate() {
        let (sx, sy) = h.sides();
        out[i] = (*h, at(sx, sy));
    }
    let above = Pos2::new(c.x, frame.top() - ROTATE_DISTANCE);
    out[8] = (Handle::Rotate, rotate_point(above, c, degrees));
    out
}

/// The handle under a point, within `reach` points of its middle. The turning handle wins a tie.
pub fn handle_at(frame: Rect, degrees: f32, p: Pos2, reach: f32) -> Option<Handle> {
    let positions = handle_positions(frame, degrees);
    positions
        .iter()
        .rev()
        .filter(|(_, at)| at.distance(p) <= reach)
        .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
        .map(|(h, _)| *h)
}

/// The frame after a handle is dragged by `delta` on the screen. The side opposite the handle stays where it
/// is, as it does in Excel, for a frame turned by `degrees` too. With `keep_aspect` a corner keeps the
/// proportions and a side handle is unchanged.
pub fn resize_by_handle(
    frame: Rect,
    degrees: f32,
    handle: Handle,
    delta: Vec2,
    keep_aspect: bool,
) -> Rect {
    let (sx, sy) = handle.sides();
    if (sx, sy) == (0, 0) {
        return frame;
    }
    let (sin, cos) = (-degrees).to_radians().sin_cos();
    let local = Vec2::new(delta.x * cos - delta.y * sin, delta.x * sin + delta.y * cos);
    let (mut w, mut h) = (frame.width(), frame.height());
    if sx != 0 {
        w = (w + sx as f32 * local.x).max(1.0);
    }
    if sy != 0 {
        h = (h + sy as f32 * local.y).max(1.0);
    }
    if keep_aspect && sx != 0 && sy != 0 && frame.width() > 0.0 && frame.height() > 0.0 {
        let scale = (w / frame.width()).max(h / frame.height());
        w = frame.width() * scale;
        h = frame.height() * scale;
    }
    // The fixed side's middle stays put: the centre moves by half of what the size changed, along the side moved.
    let shift =
        Vec2::new(sx as f32 * (w - frame.width()) / 2.0, sy as f32 * (h - frame.height()) / 2.0);
    let moved = rotate_point(frame.center() + shift, frame.center(), degrees);
    Rect::from_center_size(moved, Vec2::new(w, h))
}

/// The angle in degrees a pointer at `p` turns an object with its centre at `centre` to: 0 is the handle straight
/// above. With `snap` the angle goes to the nearest 15 degrees.
pub fn rotation_from_pointer(centre: Pos2, p: Pos2, snap: bool) -> f32 {
    let d = p - centre;
    let mut degrees = d.x.atan2(-d.y).to_degrees().rem_euclid(360.0);
    if snap {
        degrees = (degrees / 15.0).round() * 15.0 % 360.0;
    }
    degrees
}

// ------------------------------------------------------------------------------------------ anchors and screen

/// The corner of a sheet position given as a cell and offsets, on the screen.
fn corner(frame: &Frame, book: &Book, at: (unluminous_sheet::CellRef, i64, i64)) -> Pos2 {
    let px = |emu: i64| emu as f32 / EMU_PER_PIXEL * frame.zoom;
    frame.cell(book, at.0).min + Vec2::new(px(at.1), px(at.2))
}

/// An anchor's rectangle on the screen, in the columns and rows the grid draws.
pub fn screen_rect(frame: &Frame, book: &Book, anchor: &Anchor) -> Rect {
    let px = |emu: i64| emu as f32 / EMU_PER_PIXEL * frame.zoom;
    match anchor.kind {
        AnchorKind::TwoCell => {
            let from = corner(frame, book, anchor.from);
            let to = corner(frame, book, anchor.to);
            Rect::from_min_max(from, to.max(from))
        }
        AnchorKind::OneCell => {
            let from = corner(frame, book, anchor.from);
            Rect::from_min_size(from, Vec2::new(px(anchor.ext.0), px(anchor.ext.1)))
        }
        AnchorKind::Absolute => {
            let origin = frame.cell(book, unluminous_sheet::CellRef::new(0, 0)).min;
            Rect::from_min_size(
                origin + Vec2::new(px(anchor.pos.0), px(anchor.pos.1)),
                Vec2::new(px(anchor.ext.0), px(anchor.ext.1)),
            )
        }
    }
}

/// The anchor for a rectangle on the screen, keeping the kind and placement of `like`. The cells and offsets are
/// found in the columns and rows the grid draws, so a drag lands where the pointer was.
pub fn anchor_for_screen_rect(frame: &Frame, book: &Book, like: &Anchor, rect: Rect) -> Anchor {
    let emu = |px: f32| (px / frame.zoom * EMU_PER_PIXEL).round() as i64;
    let place = |p: Pos2| {
        let cell = frame.at(book, p);
        let origin = frame.cell(book, cell).min;
        (cell, emu(p.x - origin.x), emu(p.y - origin.y))
    };
    let mut anchor = like.clone();
    match like.kind {
        AnchorKind::TwoCell => {
            anchor.from = place(rect.min);
            anchor.to = place(rect.max);
        }
        AnchorKind::OneCell => {
            anchor.from = place(rect.min);
            anchor.ext = (emu(rect.width()), emu(rect.height()));
        }
        AnchorKind::Absolute => {
            let origin = frame.cell(book, unluminous_sheet::CellRef::new(0, 0)).min;
            anchor.pos = (emu(rect.min.x - origin.x), emu(rect.min.y - origin.y));
            anchor.ext = (emu(rect.width()), emu(rect.height()));
        }
    }
    anchor
}

// -------------------------------------------------------------------------------------------------- colours

fn colour(c: &ShapeColor, theme: &[u32]) -> Color32 {
    let (r, g, b, a) = c.resolve(theme);
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// What a draw needs from the window.
pub struct DrawContext<'a> {
    pub theme: &'a [u32],
    pub zoom: f32,
    /// The texture of an image part, if it is loaded.
    pub texture: &'a dyn Fn(&str) -> Option<TextureId>,
    /// The ground, text and divider colours the card for an object of an unknown kind uses.
    pub ink: &'a Ink,
}

// ------------------------------------------------------------------------------------------------- shapes

/// The outline of a preset geometry in a frame, as points, before the frame is turned. `None` for a line.
fn outline(preset: &str, adjust: Option<i64>, frame: Rect) -> Option<Vec<Pos2>> {
    let (l, t, r, b) = (frame.left(), frame.top(), frame.right(), frame.bottom());
    let c = frame.center();
    let ellipse = |steps: usize| {
        (0..steps)
            .map(|i| {
                let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                Pos2::new(c.x + a.cos() * frame.width() / 2.0, c.y + a.sin() * frame.height() / 2.0)
            })
            .collect::<Vec<_>>()
    };
    match preset {
        "line" | "straightConnector1" => None,
        "ellipse" => Some(ellipse(64)),
        "triangle" => Some(vec![Pos2::new(c.x, t), Pos2::new(r, b), Pos2::new(l, b)]),
        "rtTriangle" => Some(vec![Pos2::new(l, t), Pos2::new(r, b), Pos2::new(l, b)]),
        "diamond" => {
            Some(vec![Pos2::new(c.x, t), Pos2::new(r, c.y), Pos2::new(c.x, b), Pos2::new(l, c.y)])
        }
        "roundRect" => {
            let share = adjust.unwrap_or(16_667) as f32 / 100_000.0;
            let radius = (frame.width().min(frame.height()) * share)
                .clamp(0.0, frame.width().min(frame.height()) / 2.0);
            let mut points = Vec::new();
            for (corner, start) in [
                (Pos2::new(r - radius, t + radius), 270.0),
                (Pos2::new(r - radius, b - radius), 0.0),
                (Pos2::new(l + radius, b - radius), 90.0),
                (Pos2::new(l + radius, t + radius), 180.0),
            ] {
                for i in 0..=8 {
                    let a = (start + i as f32 * 90.0 / 8.0).to_radians();
                    points.push(corner + Vec2::new(a.cos(), a.sin()) * radius);
                }
            }
            Some(points)
        }
        _ => Some(vec![Pos2::new(l, t), Pos2::new(r, t), Pos2::new(r, b), Pos2::new(l, b)]),
    }
}

fn line_stroke(shape: &ShapeData, cx: &DrawContext) -> Option<(Stroke, Dash)> {
    let width =
        shape.line.width.map_or(1.0, |w| (w as f32 / 12_700.0 * 96.0 / 72.0).max(0.5)) * cx.zoom;
    let colour = match &shape.line.stroke {
        Outline::None => return None,
        Outline::Solid(c) => colour(c, cx.theme),
        Outline::Inherit => shape.style_line.as_ref().map(|c| colour(c, cx.theme))?,
    };
    Some((Stroke::new(width, colour), shape.line.dash))
}

fn fill_colour(shape: &ShapeData, cx: &DrawContext) -> Color32 {
    match &shape.fill {
        Fill::None => Color32::TRANSPARENT,
        Fill::Solid(c) => colour(c, cx.theme),
        Fill::Other => Color32::from_rgba_unmultiplied(128, 128, 128, 90),
        Fill::Inherit => {
            shape.style_fill.as_ref().map_or(Color32::TRANSPARENT, |c| colour(c, cx.theme))
        }
    }
}

fn paint_line(painter: &Painter, points: &[Pos2], stroke: Stroke, dash: Dash) {
    match dash {
        Dash::Solid => {
            painter.add(Shape::line(points.to_vec(), stroke));
        }
        other => {
            let (dash_len, gap) = match other {
                Dash::Dot | Dash::SquareDot => (stroke.width, stroke.width * 2.0),
                Dash::LongDash | Dash::LongDashDot => (stroke.width * 8.0, stroke.width * 3.0),
                _ => (stroke.width * 4.0, stroke.width * 3.0),
            };
            painter.extend(Shape::dashed_line(points, stroke, dash_len, gap));
        }
    }
}

/// An arrow head at the end of a line: `tip` is the end, `from` is where the line comes from.
fn paint_line_end(painter: &Painter, end: LineEnd, tip: Pos2, from: Pos2, stroke: Stroke) {
    if end == LineEnd::None {
        return;
    }
    let dir = (tip - from).normalized();
    if !dir.x.is_finite() {
        return;
    }
    let size = (stroke.width * 4.0).max(6.0);
    let back = tip - dir * size;
    let side = Vec2::new(-dir.y, dir.x) * size / 2.0;
    match end {
        LineEnd::Oval => {
            painter.circle_filled(tip - dir * size / 2.0, size / 2.0, stroke.color);
        }
        LineEnd::Diamond => {
            let mid = tip - dir * size / 2.0;
            painter.add(Shape::convex_polygon(
                vec![tip, mid + side, back, mid - side],
                stroke.color,
                Stroke::NONE,
            ));
        }
        _ => {
            painter.add(Shape::convex_polygon(
                vec![tip, back + side, back - side],
                stroke.color,
                Stroke::NONE,
            ));
        }
    }
}

fn draw_shape(
    painter: &Painter,
    frame: Rect,
    object: &DrawingObject,
    shape: &ShapeData,
    cx: &DrawContext,
) {
    let c = frame.center();
    let turn = |p: Pos2| {
        let mut q = p;
        if object.flip_h {
            q.x = 2.0 * c.x - q.x;
        }
        if object.flip_v {
            q.y = 2.0 * c.y - q.y;
        }
        rotate_point(q, c, object.rotation as f32)
    };
    let stroke = line_stroke(shape, cx);
    match outline(&shape.preset, shape.adjust, frame) {
        Some(points) => {
            let turned: Vec<Pos2> = points.into_iter().map(turn).collect();
            let fill = fill_colour(shape, cx);
            painter.add(Shape::convex_polygon(turned.clone(), fill, Stroke::NONE));
            if let Some((s, dash)) = stroke {
                let mut closed = turned.clone();
                closed.push(turned[0]);
                paint_line(painter, &closed, s, dash);
            }
        }
        None => {
            let (a, b) = (turn(frame.left_top()), turn(frame.right_bottom()));
            let (s, dash) = stroke.unwrap_or((Stroke::new(cx.zoom, cx.ink.text), Dash::Solid));
            paint_line(painter, &[a, b], s, dash);
            paint_line_end(painter, shape.line.head, a, b, s);
            paint_line_end(painter, shape.line.tail, b, a, s);
        }
    }
    draw_text(painter, frame, object, shape, cx);
}

fn draw_text(
    painter: &Painter,
    frame: Rect,
    object: &DrawingObject,
    shape: &ShapeData,
    cx: &DrawContext,
) {
    let Some(body) = &shape.text else { return };
    let inset = 7.0 * cx.zoom;
    let area = frame.shrink2(Vec2::new(inset, 3.6 * cx.zoom));
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = if body.wrap { area.width().max(1.0) } else { f32::INFINITY };
    for (i, p) in body.paragraphs.iter().enumerate() {
        if i > 0 {
            job.append("\n", 0.0, egui::TextFormat::default());
        }
        for run in &p.runs {
            let size = run.size.unwrap_or(11.0) as f32 * 96.0 / 72.0 * cx.zoom;
            let colour = run
                .color
                .as_ref()
                .or(shape.style_text.as_ref())
                .map_or(cx.ink.text, |c| colour(c, cx.theme));
            let mut format = egui::TextFormat::simple(FontId::proportional(size), colour);
            if run.italic {
                format.italics = true;
            }
            if run.underline {
                format.underline = Stroke::new(1.0, colour);
            }
            job.append(&run.text, 0.0, format);
        }
        if let Some(first) = body.paragraphs.get(i).map(|p| p.align) {
            if i == 0 {
                job.halign = match first {
                    HAlign::Left | HAlign::Justify => egui::Align::LEFT,
                    HAlign::Centre => egui::Align::Center,
                    HAlign::Right => egui::Align::RIGHT,
                };
            }
        }
    }
    let galley = painter.layout_job(job);
    let x = match body.paragraphs.first().map_or(HAlign::Left, |p| p.align) {
        HAlign::Left | HAlign::Justify => area.left(),
        HAlign::Centre => area.center().x - galley.size().x / 2.0,
        HAlign::Right => area.right() - galley.size().x,
    };
    let y = match body.valign {
        VAlign::Top => area.top(),
        VAlign::Middle => area.center().y - galley.size().y / 2.0,
        VAlign::Bottom => area.bottom() - galley.size().y,
    };
    let at = rotate_point(Pos2::new(x, y), frame.center(), object.rotation as f32);
    let ink = cx.ink.text;
    painter.add(TextShape::new(at, galley, ink).with_angle(object.rotation.to_radians() as f32));
}

// ------------------------------------------------------------------------------------------------ pictures

/// A picture: its texture cropped, turned and flipped, as one mesh.
fn draw_picture(painter: &Painter, frame: Rect, object: &DrawingObject, texture: TextureId) {
    let Some(picture) = &object.picture else { return };
    let crop = picture.crop;
    let uv = Rect::from_min_max(
        Pos2::new(crop.left as f32 / 100_000.0, crop.top as f32 / 100_000.0),
        Pos2::new(1.0 - crop.right as f32 / 100_000.0, 1.0 - crop.bottom as f32 / 100_000.0),
    );
    let c = frame.center();
    let mut mesh = Mesh::with_texture(texture);
    let quad = [frame.left_top(), frame.right_top(), frame.right_bottom(), frame.left_bottom()];
    let uvs = [uv.left_top(), uv.right_top(), uv.right_bottom(), uv.left_bottom()];
    for (p, mut t) in quad.iter().zip(uvs) {
        if object.flip_h {
            t.x = uv.left() + uv.right() - t.x;
        }
        if object.flip_v {
            t.y = uv.top() + uv.bottom() - t.y;
        }
        // any ground: the tint a picture is drawn with, which leaves its own colours as they are.
        mesh.vertices.push(egui::epaint::Vertex {
            pos: rotate_point(*p, c, object.rotation as f32),
            uv: t,
            color: Color32::WHITE,
        });
    }
    mesh.indices.extend([0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

/// A card for an object this version does not draw: its kind and name on a quiet ground.
fn draw_card(painter: &Painter, frame: Rect, object: &DrawingObject, cx: &DrawContext) {
    let c = frame.center();
    let points: Vec<Pos2> = corners(frame, object.rotation as f32).to_vec();
    let ground = cx.ink.wash;
    painter.add(Shape::convex_polygon(points.clone(), ground, Stroke::new(1.0, cx.ink.grid)));
    let label = if object.name.is_empty() {
        object.kind.label().to_owned()
    } else {
        format!("{}: {}", object.kind.label(), object.name)
    };
    let galley = painter.layout(
        label,
        FontId::proportional(12.0 * cx.zoom),
        cx.ink.text,
        (frame.width() - 8.0).max(8.0),
    );
    let at = rotate_point(c - galley.size() / 2.0, c, object.rotation as f32);
    painter.add(
        TextShape::new(at, galley, cx.ink.text).with_angle(object.rotation.to_radians() as f32),
    );
}

fn draw_group(painter: &Painter, frame: Rect, object: &DrawingObject, cx: &DrawContext) {
    let space = object.child_space;
    if space.w == 0 || space.h == 0 {
        return;
    }
    let (sx, sy) = (frame.width() / space.w as f32, frame.height() / space.h as f32);
    for child in object.children.iter().filter(|c| !c.hidden) {
        let f = child.frame;
        let min = frame.min + Vec2::new((f.x - space.x) as f32 * sx, (f.y - space.y) as f32 * sy);
        let rect = Rect::from_min_size(min, Vec2::new(f.w as f32 * sx, f.h as f32 * sy));
        draw_object(painter, rect, child, cx);
    }
}

/// Draw one object into the rectangle its frame has on the screen (the frame before the object is turned).
pub fn draw_object(painter: &Painter, frame: Rect, object: &DrawingObject, cx: &DrawContext) {
    match (&object.kind, &object.picture, &object.shape) {
        (_, Some(picture), _) => match (cx.texture)(&picture.part) {
            Some(texture) => draw_picture(painter, frame, object, texture),
            None => draw_card(painter, frame, object, cx),
        },
        (_, _, Some(shape)) => draw_shape(painter, frame, object, shape, cx),
        (ObjectKind::Group, _, _) => draw_group(painter, frame, object, cx),
        _ => match object.fallback.as_deref().and_then(|p| (cx.texture)(p)) {
            Some(texture) => draw_picture_from_part(painter, frame, object, texture),
            None => draw_card(painter, frame, object, cx),
        },
    }
}

/// An object kept as written, drawn from its stored picture.
fn draw_picture_from_part(
    painter: &Painter,
    frame: Rect,
    object: &DrawingObject,
    texture: TextureId,
) {
    let mut mesh = Mesh::with_texture(texture);
    // any ground: the tint a picture is drawn with, which leaves its own colours as they are.
    mesh.add_rect_with_uv(
        frame,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    let _ = object;
    painter.add(Shape::mesh(mesh));
}

/// The selection round an object: its outline, the eight resize handles and the turning handle on its stem.
pub fn draw_selection(
    painter: &Painter,
    frame: Rect,
    degrees: f32,
    accent: Color32,
    ground: Color32,
) {
    let outline = corners(frame, degrees);
    let mut closed = outline.to_vec();
    closed.push(outline[0]);
    painter.add(Shape::line(closed, Stroke::new(1.0, accent)));
    let positions = handle_positions(frame, degrees);
    let (rotate, top) = (positions[8].1, positions[1].1);
    painter.line_segment([top, rotate], Stroke::new(1.0, accent));
    for (handle, at) in positions {
        if handle == Handle::Rotate {
            painter.circle(at, HANDLE_RADIUS, ground, Stroke::new(1.0, accent));
        } else {
            let r = Rect::from_center_size(at, Vec2::splat(HANDLE_RADIUS * 2.0));
            painter.rect(r, 1, ground, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        }
    }
}

// ---------------------------------------------------------------------------------------------- the sheet

/// Load the textures of every picture the sheet draws that is not loaded yet.
fn load_textures(ctx: &egui::Context, book: &mut Book, parts: &[String]) {
    for part in parts {
        if book.pictures.0.contains_key(part) {
            continue;
        }
        let texture = book
            .session
            .book
            .kept
            .get(part)
            .and_then(|bytes| crate::services::picture::decode_bytes(bytes).ok())
            .map(|image| {
                crate::services::picture::upload(
                    ctx,
                    part.clone(),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
        book.pictures.0.insert(part.clone(), texture);
    }
}

fn parts_of(objects: &[DrawingObject], out: &mut Vec<String>) {
    for o in objects {
        out.extend(o.picture.as_ref().map(|p| p.part.clone()));
        out.extend(o.fallback.clone());
        parts_of(&o.children, out);
    }
}

/// The frames of a sheet's objects on the screen, bottom first: id, frame and rotation. The grid hands these
/// to [`hit_test`] and [`handle_at`].
pub fn object_frames(frame: &Frame, book: &Book) -> Vec<(ObjectId, Rect, f32)> {
    let sheet: &Sheet = &book.session.book.sheets[book.view.sheet];
    sheet
        .drawing
        .objects
        .iter()
        .filter(|o| !o.hidden)
        .map(|o| (o.id, screen_rect(frame, book, &o.anchor), o.rotation as f32))
        .collect()
}

/// Draw every object of the sheet showing, bottom first, and the selection on the chosen one.
pub fn draw_sheet_objects(
    ui: &egui::Ui,
    book: &mut Book,
    frame: Frame,
    ink: &Ink,
    chosen: Option<ObjectId>,
) {
    let mut parts = Vec::new();
    parts_of(&book.session.book.sheets[book.view.sheet].drawing.objects, &mut parts);
    load_textures(ui.ctx(), book, &parts);
    let painter = ui.painter_at(frame.cells);
    let sheet = &book.session.book.sheets[book.view.sheet];
    let theme = sheet_theme(book);
    let textures = |part: &str| book.pictures.0.get(part).and_then(Option::as_ref).map(|t| t.id());
    let cx = DrawContext { theme: &theme, zoom: frame.zoom, texture: &textures, ink };
    for object in sheet.drawing.objects.iter().filter(|o| !o.hidden) {
        let rect = screen_rect(&frame, book, &object.anchor);
        if rect.expand(ROTATE_DISTANCE).intersects(frame.cells) {
            draw_object(&painter, rect, object, &cx);
        }
    }
    if let Some(id) = chosen {
        if let Some(object) = sheet.drawing.find(id) {
            let rect = screen_rect(&frame, book, &object.anchor);
            draw_selection(&painter, rect, object.rotation as f32, ink.accent, ink.paper);
        }
    }
}

fn sheet_theme(book: &Book) -> Vec<u32> {
    book.session.book.styles.theme.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Pos2, b: Pos2) -> bool {
        a.distance(b) < 0.01
    }

    #[test]
    fn a_turned_frame_is_hit_inside_its_turned_outline_only() {
        let frame = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(100.0, 20.0));
        assert!(contains(frame, 0.0, Pos2::new(95.0, 10.0)));
        // Turned a quarter, the frame stands upright about its centre (50, 10).
        assert!(contains(frame, 90.0, Pos2::new(50.0, 55.0)));
        assert!(!contains(frame, 90.0, Pos2::new(95.0, 10.0)));
    }

    #[test]
    fn the_topmost_object_under_the_pointer_wins() {
        let low = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::splat(100.0));
        let high = Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::splat(100.0));
        let frames = [(1, low, 0.0), (2, high, 0.0)];
        assert_eq!(hit_test(&frames, Pos2::new(70.0, 70.0)), Some(2));
        assert_eq!(hit_test(&frames, Pos2::new(10.0, 10.0)), Some(1));
        assert_eq!(hit_test(&frames, Pos2::new(300.0, 10.0)), None);
    }

    #[test]
    fn the_nine_handles_are_where_the_frame_turned_puts_them() {
        let frame = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(40.0, 20.0));
        let p = handle_positions(frame, 0.0);
        assert!(near(p[0].1, Pos2::new(100.0, 100.0)) && near(p[4].1, Pos2::new(140.0, 120.0)));
        assert!(near(p[8].1, Pos2::new(120.0, 100.0 - ROTATE_DISTANCE)));
        let turned = handle_positions(frame, 90.0);
        assert!(near(turned[8].1, Pos2::new(120.0 + 10.0 + ROTATE_DISTANCE, 110.0)));
        assert_eq!(handle_at(frame, 0.0, Pos2::new(140.5, 120.5), 5.0), Some(Handle::SouthEast));
        assert_eq!(handle_at(frame, 0.0, Pos2::new(120.0, 76.0), 5.0), Some(Handle::Rotate));
        assert_eq!(handle_at(frame, 0.0, Pos2::new(120.0, 110.0), 5.0), None);
    }

    #[test]
    fn dragging_a_handle_keeps_the_opposite_side_still() {
        let frame = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(40.0, 20.0));
        let grown = resize_by_handle(frame, 0.0, Handle::SouthEast, Vec2::new(10.0, 5.0), false);
        assert_eq!((grown.min, grown.max), (Pos2::new(100.0, 100.0), Pos2::new(150.0, 125.0)));
        let left = resize_by_handle(frame, 0.0, Handle::West, Vec2::new(-10.0, 3.0), false);
        assert_eq!((left.min.x, left.max.x, left.min.y, left.max.y), (90.0, 140.0, 100.0, 120.0));
        let kept = resize_by_handle(frame, 0.0, Handle::SouthEast, Vec2::new(20.0, 0.0), true);
        assert!((kept.width() / kept.height() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_turned_frame_resizes_along_its_own_axes() {
        let frame = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(40.0, 20.0));
        // Turned 90 degrees, its east side faces down the screen: pulling it down 10 lengthens it by 10.
        let grown = resize_by_handle(frame, 90.0, Handle::East, Vec2::new(0.0, 10.0), false);
        assert!((grown.width() - 50.0).abs() < 1e-3 && (grown.height() - 20.0).abs() < 1e-3);
        let west = rotate_point(frame.center() + Vec2::new(-20.0, 0.0), frame.center(), 90.0);
        let new_west = rotate_point(grown.center() + Vec2::new(-25.0, 0.0), grown.center(), 90.0);
        assert!(near(west, new_west), "the far side stayed: {west:?} {new_west:?}");
    }

    #[test]
    fn the_turning_handle_gives_the_angle_from_straight_up() {
        let c = Pos2::new(100.0, 100.0);
        assert_eq!(rotation_from_pointer(c, Pos2::new(100.0, 50.0), false), 0.0);
        assert!((rotation_from_pointer(c, Pos2::new(150.0, 100.0), false) - 90.0).abs() < 1e-3);
        assert!((rotation_from_pointer(c, Pos2::new(100.0, 150.0), false) - 180.0).abs() < 1e-3);
        assert_eq!(rotation_from_pointer(c, Pos2::new(150.0, 55.0), true), 45.0);
    }

    #[test]
    fn a_round_cornered_outline_stays_inside_its_frame() {
        let frame = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(80.0, 40.0));
        let points = outline("roundRect", Some(30_000), frame).unwrap();
        assert!(points.iter().all(|p| frame.expand(0.01).contains(*p)));
        assert!(outline("line", None, frame).is_none());
        assert_eq!(outline("rect", None, frame).unwrap().len(), 4);
    }
}
