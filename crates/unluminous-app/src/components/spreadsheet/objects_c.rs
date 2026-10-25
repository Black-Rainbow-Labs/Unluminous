//! The pointer and keys of the drawing objects on a sheet (`task-2360`, the `wiringc` lane): choosing a
//! picture, shape or text box by clicking it, moving, sizing and turning it with the pointer, drawing a new
//! shape by dragging out its box, and nudging or deleting what is chosen with the keys.
//!
//! **A drag writes nothing until the pointer is released.** While it goes on, the outline of where the
//! object would land is drawn. On release the drag becomes the words of the same verbs an agent uses
//! (`object frame`, `object rotate`, `shape <kind> ...`), so a pointer and an agent change the workbook the
//! same way and the whole drag is one undo step for each object it moved. Each pane of a split or frozen
//! window draws the objects in its own frame; a drag belongs to the pane it started in.

use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use unluminous_sheet::objects::{Anchor, ObjectId, EMU_PER_POINT};

use super::grid::{Frame, GridOut, Ink, Pane};
use super::objects::{
    anchor_for_screen_rect, corners, draw_selection, handle_at, hit_test, object_frames,
    resize_by_handle, rotation_from_pointer, Handle,
};
use crate::services::spreadsheet::Book;

/// How far from a handle's centre a press still takes it, in points.
const REACH: f32 = 6.0;
/// A drag shorter than this is a click.
const CLICK: f32 = 2.0;
/// The size a shape gets when it is clicked rather than dragged out, in points.
const DEFAULT_SHAPE: (f32, f32) = (120.0, 60.0);

/// What a drag is doing.
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Move,
    Resize(Handle),
    Rotate,
    /// Drawing a new shape of this kind word.
    Draw(String),
}

/// An object a drag carries: where its frame was on the screen when the drag began.
#[derive(Clone, Debug)]
struct Carried {
    id: ObjectId,
    rect: Rect,
    rotation: f32,
    lock_aspect: bool,
}

/// A drag in progress, kept between frames in the context's memory.
#[derive(Clone, Debug)]
struct Drag {
    kind: Kind,
    pane: usize,
    start: Pos2,
    now: Pos2,
    shift: bool,
    items: Vec<Carried>,
}

fn drag_id(book: &Book) -> egui::Id {
    egui::Id::new(("spreadsheet-object-drag", &book.key))
}

/// The pane whose cells hold the point.
fn pane_at(panes: &[Pane], p: Pos2) -> Option<usize> {
    panes.iter().position(|pane| pane.frame.cells.contains(p))
}

/// The objects chosen, with their frames on the screen in this pane.
fn carried(book: &Book, frame: &Frame) -> Vec<Carried> {
    let s = &book.session.book.sheets[book.view.sheet];
    object_frames(frame, book)
        .into_iter()
        .filter(|(id, _, _)| book.view.objects.has(*id))
        .map(|(id, rect, rotation)| Carried {
            id,
            rect,
            rotation,
            lock_aspect: s.drawing.find(id).is_some_and(|o| o.lock_aspect),
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ pointer

/// Read the pointer for the objects. Answers whether the pointer belongs to them this frame, so the cells do
/// not read the same press.
pub fn pointer(
    ui: &egui::Ui,
    response: &egui::Response,
    book: &mut Book,
    panes: &[Pane],
    out: &mut GridOut,
) -> bool {
    if book.view.menu.is_none() {
        book.view.objects.menu = None;
    }
    let id = drag_id(book);
    if let Some(drag) = ui.data(|d| d.get_temp::<Drag>(id)) {
        return carry_on(ui, book, panes, out, id, drag);
    }
    // The raw pointer, because the grid's own response only knows the pointer is over it a frame after it
    // arrives, and a press can arrive in the same frame as the move that brought the pointer.
    let over_us = |p: &Pos2| ui.ctx().layer_id_at(*p).is_none_or(|l| l == response.layer_id);
    let Some(p) = ui.input(|i| i.pointer.interact_pos()).filter(over_us) else {
        return false;
    };
    let Some(index) = pane_at(panes, p) else { return false };
    let frame = panes[index].frame;
    let pressed = ui.input(|i| i.pointer.primary_pressed());
    if book.view.objects.drawing.is_some() {
        if pressed {
            let kind = Kind::Draw(book.view.objects.drawing.clone().unwrap_or_default());
            begin(ui, id, kind, index, p, Vec::new());
            out.pressed = true;
            return true;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        return false;
    }
    if response.secondary_clicked() {
        return open_the_menu(book, &frame, p, out);
    }
    if !pressed {
        return hover(ui, book, &frame, p);
    }
    press(ui, book, id, index, &frame, p, out)
}

/// Show the pointer the shape of what a press here would do. Answers whether an object is under it.
fn hover(ui: &egui::Ui, book: &Book, frame: &Frame, p: Pos2) -> bool {
    let frames = object_frames(frame, book);
    if let Some(handle) = single_handle(book, &frames, p) {
        ui.ctx().set_cursor_icon(handle.cursor());
        return true;
    }
    if hit_test(&frames, p).is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        return true;
    }
    false
}

/// The handle of the one chosen object under a point.
fn single_handle(book: &Book, frames: &[(ObjectId, Rect, f32)], p: Pos2) -> Option<Handle> {
    let [only] = book.view.objects.selected.as_slice() else { return None };
    let (_, rect, rotation) = frames.iter().find(|(id, _, _)| id == only)?;
    handle_at(*rect, *rotation, p, REACH)
}

/// A primary press: take a handle, take an object, or leave the cells to it.
fn press(
    ui: &egui::Ui,
    book: &mut Book,
    id: egui::Id,
    index: usize,
    frame: &Frame,
    p: Pos2,
    out: &mut GridOut,
) -> bool {
    let frames = object_frames(frame, book);
    if let Some(handle) = single_handle(book, &frames, p) {
        let kind = if handle == Handle::Rotate { Kind::Rotate } else { Kind::Resize(handle) };
        begin(ui, id, kind, index, p, carried(book, frame));
        out.pressed = true;
        return true;
    }
    let Some(hit) = hit_test(&frames, p) else {
        book.view.objects.choose(None);
        return false;
    };
    let additive = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
    if additive {
        book.view.objects.toggle(hit);
    } else if !book.view.objects.has(hit) {
        book.view.objects.choose(Some(hit));
    }
    book.view.chart = None;
    begin(ui, id, Kind::Move, index, p, carried(book, frame));
    out.pressed = true;
    true
}

/// A right click on an object chooses it and opens its menu.
fn open_the_menu(book: &mut Book, frame: &Frame, p: Pos2, out: &mut GridOut) -> bool {
    let frames = object_frames(frame, book);
    let Some(hit) = hit_test(&frames, p) else { return false };
    if !book.view.objects.has(hit) {
        book.view.objects.choose(Some(hit));
    }
    book.view.objects.menu = Some(p);
    out.menu = Some(p);
    true
}

/// Start a drag and keep it in memory.
fn begin(ui: &egui::Ui, id: egui::Id, kind: Kind, pane: usize, start: Pos2, items: Vec<Carried>) {
    let drag = Drag { kind, pane, start, now: start, shift: false, items };
    ui.data_mut(|d| d.insert_temp(id, drag));
}

/// A drag already begun: follow the pointer, and on release turn it into the words that make it so.
fn carry_on(
    ui: &egui::Ui,
    book: &mut Book,
    panes: &[Pane],
    out: &mut GridOut,
    id: egui::Id,
    mut drag: Drag,
) -> bool {
    let (down, latest, shift) =
        ui.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos(), i.modifiers.shift));
    if let Some(p) = latest {
        drag.now = p;
    }
    drag.shift = shift;
    out.pressed = true;
    if down {
        ui.ctx().set_cursor_icon(cursor_of(&drag));
        ui.data_mut(|d| d.insert_temp(id, drag));
        ui.ctx().request_repaint();
        return true;
    }
    ui.data_mut(|d| d.remove::<Drag>(id));
    let Some(pane) = panes.get(drag.pane) else { return true };
    let words = release_words(book, &pane.frame, &drag);
    out.words.extend(words);
    if matches!(drag.kind, Kind::Draw(_)) {
        book.view.objects.drawing = None;
    }
    true
}

/// The pointer shape while a drag goes on.
fn cursor_of(drag: &Drag) -> egui::CursorIcon {
    match &drag.kind {
        Kind::Move => egui::CursorIcon::Grabbing,
        Kind::Resize(h) => h.cursor(),
        Kind::Rotate => egui::CursorIcon::Grabbing,
        Kind::Draw(_) => egui::CursorIcon::Crosshair,
    }
}

// ----------------------------------------------------------------------------------------- the words

/// Where each carried object lands on the screen if the drag ended now: id, frame, turn.
fn landing(drag: &Drag) -> Vec<(ObjectId, Rect, f32)> {
    let delta = drag.now - drag.start;
    drag.items
        .iter()
        .map(|c| match &drag.kind {
            Kind::Move => (c.id, c.rect.translate(delta), c.rotation),
            Kind::Resize(h) => {
                let keep = drag.shift || c.lock_aspect;
                (c.id, resize_by_handle(c.rect, c.rotation, *h, delta, keep), c.rotation)
            }
            Kind::Rotate => {
                let degrees = rotation_from_pointer(c.rect.center(), drag.now, drag.shift);
                (c.id, c.rect, degrees)
            }
            Kind::Draw(_) => (c.id, c.rect, c.rotation),
        })
        .collect()
}

/// A number of points as a word.
fn word(points: f64) -> String {
    format!("{:.2}", points)
}

/// A screen rectangle as the sheet's own frame in points: left, top, width, height.
pub fn sheet_frame(book: &Book, frame: &Frame, like: &Anchor, rect: Rect) -> [f64; 4] {
    let anchor = anchor_for_screen_rect(frame, book, like, rect);
    let r = anchor.rect(&book.session.book.sheets[book.view.sheet]);
    let pt = |v: i64| v as f64 / EMU_PER_POINT as f64;
    [pt(r.x), pt(r.y), pt(r.w), pt(r.h)]
}

/// The words a released drag stands for.
fn release_words(book: &Book, frame: &Frame, drag: &Drag) -> Vec<Vec<String>> {
    if let Kind::Draw(kind) = &drag.kind {
        return draw_words(book, frame, drag, kind);
    }
    if (drag.now - drag.start).length() < CLICK {
        return Vec::new();
    }
    let s = &book.session.book.sheets[book.view.sheet];
    let mut all = Vec::new();
    for (id, rect, degrees) in landing(drag) {
        let Some(object) = s.drawing.find(id) else { continue };
        let id_word = id.to_string();
        if drag.kind == Kind::Rotate {
            all.push(vec!["object".to_owned(), "rotate".to_owned(), id_word, word(degrees as f64)]);
            continue;
        }
        let f = sheet_frame(book, frame, &object.anchor, rect);
        let mut w = vec!["object".to_owned(), "frame".to_owned(), id_word];
        w.extend(f.iter().map(|v| word(*v)));
        all.push(w);
    }
    all
}

/// The words for a shape dragged out: a click gives the default size at the pointer.
fn draw_words(book: &Book, frame: &Frame, drag: &Drag, kind: &str) -> Vec<Vec<String>> {
    let mut rect = Rect::from_two_pos(drag.start, drag.now);
    if rect.width() < CLICK * 4.0 && rect.height() < CLICK * 4.0 {
        let size = Vec2::new(DEFAULT_SHAPE.0, DEFAULT_SHAPE.1) * frame.zoom;
        rect = Rect::from_min_size(drag.start, size);
    }
    let f = sheet_frame(book, frame, &Anchor::default(), rect);
    let mut w = vec!["shape".to_owned(), kind.to_owned()];
    w.extend(f.iter().map(|v| word(*v)));
    vec![w]
}

// ------------------------------------------------------------------------------------------------ keys

/// The keys for chosen objects: arrows nudge, Delete removes, Escape lets go. Answers whether a key was used,
/// so the cells do not read it as well.
pub fn keys(ctx: &egui::Context, book: &mut Book, words: &mut Vec<Vec<String>>) -> bool {
    use egui::{Key, Modifiers};
    let drawing = book.view.objects.drawing.is_some();
    if book.view.objects.selected.is_empty() && !drawing {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        book.view.objects.choose(None);
        book.view.objects.drawing = None;
        return true;
    }
    if book.view.objects.selected.is_empty() {
        return false;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete)) {
        words.push(vec!["object".to_owned(), "delete".to_owned()]);
        return true;
    }
    for (key, dx, dy) in [
        (Key::ArrowLeft, -1.0, 0.0),
        (Key::ArrowRight, 1.0, 0.0),
        (Key::ArrowUp, 0.0, -1.0),
        (Key::ArrowDown, 0.0, 1.0),
    ] {
        // Shift first: a pattern with no modifier also matches a press with Shift held.
        for (modifiers, steps) in [(Modifiers::SHIFT, 10.0), (Modifiers::NONE, 1.0)] {
            if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                // One pixel is 0.75 points.
                let step = steps * 0.75;
                words.push(vec![
                    "object".to_owned(),
                    "move".to_owned(),
                    word(dx * step),
                    word(dy * step),
                ]);
                return true;
            }
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------ drawing

/// Draw what a pane holds that sits over the cells: the sparklines, the objects, the selection and the
/// outline of a drag in progress.
pub fn draw(ui: &egui::Ui, book: &mut Book, frame: Frame, ink: &Ink, pane: usize) {
    note_where_they_are(book, &frame, pane);
    super::sparkline::draw_sparklines(ui, book, frame, ink);
    super::objects::draw_sheet_objects(ui, book, frame, ink, None);
    let painter = ui.painter_at(frame.cells);
    let drag = ui.data(|d| d.get_temp::<Drag>(drag_id(book)));
    let moving = drag.as_ref().filter(|d| d.pane == pane);
    draw_chosen(&painter, book, &frame, ink, moving);
    if let Some(d) = moving {
        draw_drag(&painter, d, ink);
    }
}

/// Remember where the objects are on the screen, for the `objects` verb. The first pane clears the list.
fn note_where_they_are(book: &mut Book, frame: &Frame, pane: usize) {
    let found = object_frames(frame, book);
    let shown = &mut book.view.objects.shown;
    if pane == 0 {
        shown.clear();
    }
    for (id, rect, _) in found {
        if rect.intersects(frame.cells) {
            shown.retain(|(s, _)| *s != id);
            shown.push((id, [rect.left(), rect.top(), rect.width(), rect.height()]));
        }
    }
}

/// The selection on every chosen object: handles on a lone one, an outline on each of several.
fn draw_chosen(
    painter: &egui::Painter,
    book: &Book,
    frame: &Frame,
    ink: &Ink,
    moving: Option<&Drag>,
) {
    if moving.is_some_and(|d| !matches!(d.kind, Kind::Draw(_))) {
        return;
    }
    let frames = object_frames(frame, book);
    let chosen: Vec<_> = frames.iter().filter(|(id, _, _)| book.view.objects.has(*id)).collect();
    for (_, rect, degrees) in &chosen {
        if chosen.len() == 1 {
            draw_selection(painter, *rect, *degrees, ink.accent, ink.paper);
        } else {
            outline(painter, *rect, *degrees, ink.accent);
        }
    }
}

/// A turned frame's outline.
fn outline(painter: &egui::Painter, rect: Rect, degrees: f32, colour: Color32) {
    let c = corners(rect, degrees);
    let mut closed = c.to_vec();
    closed.push(c[0]);
    painter.add(egui::Shape::line(closed, Stroke::new(1.5, colour)));
}

/// Where the dragged objects would land, as outlines (handles on a lone one).
fn draw_drag(painter: &egui::Painter, drag: &Drag, ink: &Ink) {
    if let Kind::Draw(_) = &drag.kind {
        let rect = Rect::from_two_pos(drag.start, drag.now);
        outline(painter, rect, 0.0, ink.accent);
        return;
    }
    let landed = landing(drag);
    for (_, rect, degrees) in &landed {
        if landed.len() == 1 {
            draw_selection(painter, *rect, *degrees, ink.accent, ink.paper);
        } else {
            outline(painter, *rect, *degrees, ink.accent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(kind: Kind, items: Vec<Carried>, to: Pos2) -> Drag {
        Drag { kind, pane: 0, start: Pos2::new(100.0, 100.0), now: to, shift: false, items }
    }

    fn one(rect: Rect, rotation: f32) -> Carried {
        Carried { id: 7, rect, rotation, lock_aspect: false }
    }

    #[test]
    fn moving_carries_the_frame_by_the_pointer() {
        let rect = Rect::from_min_size(Pos2::new(90.0, 90.0), Vec2::new(40.0, 20.0));
        let landed = landing(&drag(Kind::Move, vec![one(rect, 0.0)], Pos2::new(130.0, 110.0)));
        assert_eq!(landed[0].1.min, Pos2::new(120.0, 100.0));
        assert_eq!(landed[0].1.size(), rect.size());
    }

    #[test]
    fn a_corner_handle_sizes_and_shift_keeps_the_aspect() {
        let rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(40.0, 20.0));
        let mut d =
            drag(Kind::Resize(Handle::SouthEast), vec![one(rect, 0.0)], Pos2::new(120.0, 100.0));
        let loose = landing(&d)[0].1;
        assert_eq!((loose.width(), loose.height()), (60.0, 20.0));
        d.shift = true;
        let kept = landing(&d)[0].1;
        assert!((kept.width() / kept.height() - 2.0).abs() < 1e-3);
    }

    #[test]
    fn the_turning_handle_turns_about_the_centre_and_shift_snaps() {
        let rect = Rect::from_center_size(Pos2::new(100.0, 100.0), Vec2::splat(40.0));
        let mut d = drag(Kind::Rotate, vec![one(rect, 0.0)], Pos2::new(150.0, 98.0));
        d.start = Pos2::new(100.0, 60.0);
        let free = landing(&d)[0].2;
        assert!((free - 87.7).abs() < 0.5, "{free}");
        d.shift = true;
        assert_eq!(landing(&d)[0].2, 90.0);
    }
}
