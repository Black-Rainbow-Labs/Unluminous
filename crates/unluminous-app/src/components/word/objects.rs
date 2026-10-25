//! Pictures, shapes and text boxes on the pages as things a person selects and changes (task-2359, TDD
//! 5.3): a press on one selects it, its eight handles resize it, a drag moves a floating one and drops an
//! inline one where the pointer lets go, and the Crop action turns the handles into crop handles.
//!
//! Where each object is comes from the layout's `Item::Object`, which carries the position of the
//! object's character, so selecting one is selecting that character and every change goes through the
//! session's picture operations, one undo step each.

use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use unluminous_word::layout::Item;
use unluminous_word::position::Pos;

use super::page_view::{page_point, page_tops};
use crate::services::word::Doc;

/// Which part of a selected object the pointer took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Move,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A drag of an object or one of its handles, while it lasts.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDrag {
    pub handle: Handle,
    pub page: usize,
    /// The object's rectangle on its page, in points, when the drag began.
    pub start: unluminous_word::layout::Rect,
    /// Where the pointer was, in page points, when the drag began.
    pub from: (f32, f32),
    /// Where it is now.
    pub to: (f32, f32),
    /// Whether this drag crops rather than resizes.
    pub crop: bool,
}

/// How far from a handle's centre a press still takes it, in screen points.
const GRAB: f32 = 6.0;

/// Every object on a page: its rectangle in page points and its character's position.
pub fn on_page(doc: &Doc, page: usize) -> Vec<(unluminous_word::layout::Rect, Pos)> {
    let Some(page) = doc.pages.pages.get(page) else { return Vec::new() };
    page.items
        .iter()
        .filter_map(|item| match item {
            Item::Object { rect, story, path, offset } => {
                Some((*rect, Pos { story: story.clone(), path: path.clone(), offset: *offset }))
            }
            _ => None,
        })
        .collect()
}

/// The selected object's page and rectangle, when it is on a page.
pub fn selected(doc: &Doc) -> Option<(usize, unluminous_word::layout::Rect)> {
    let at = doc.session.selected_object()?;
    (0..doc.pages.pages.len()).find_map(|page| {
        on_page(doc, page).into_iter().find(|(_, pos)| *pos == at).map(|(rect, _)| (page, rect))
    })
}

/// The eight handles of a rectangle, with the point each is drawn at.
fn handles(rect: Rect) -> [(Handle, Pos2); 8] {
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let (cx, cy) = (rect.center().x, rect.center().y);
    [
        (Handle::TopLeft, Pos2::new(l, t)),
        (Handle::Top, Pos2::new(cx, t)),
        (Handle::TopRight, Pos2::new(r, t)),
        (Handle::Left, Pos2::new(l, cy)),
        (Handle::Right, Pos2::new(r, cy)),
        (Handle::BottomLeft, Pos2::new(l, b)),
        (Handle::Bottom, Pos2::new(cx, b)),
        (Handle::BottomRight, Pos2::new(r, b)),
    ]
}

/// A page rectangle on the screen.
fn to_screen(doc: &Doc, area: Rect, page: usize, rect: unluminous_word::layout::Rect) -> Option<Rect> {
    let (tops, _) = page_tops(doc, area.width());
    let top = tops.get(page)?;
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(top.x, top.y - doc.view.scroll);
    Some(Rect::from_min_size(
        origin + Vec2::new(rect.x * zoom, rect.y * zoom),
        Vec2::new(rect.w * zoom, rect.h * zoom),
    ))
}

/// A press at `at`: on a handle of the selected object it starts a resize or a crop; on an object it
/// selects the object and starts a move. Answers whether the press was taken. A press inside a text
/// box's text is left to the text, so the caret goes there; its edge selects the box.
pub fn press(doc: &mut Doc, area: Rect, at: Pos2) -> bool {
    if let Some((page, rect)) = selected(doc) {
        if let Some(screen) = to_screen(doc, area, page, rect) {
            let handle = handles(screen).into_iter().find(|(_, p)| p.distance(at) <= GRAB).map(|(h, _)| h);
            if let Some(handle) = handle {
                let point = page_point(doc, area, at).map_or((0.0, 0.0), |(_, x, y)| (x, y));
                doc.view.object_drag =
                    Some(ObjectDrag { handle, page, start: rect, from: point, to: point, crop: doc.view.cropping });
                return true;
            }
        }
    }
    let Some((page, x, y)) = page_point(doc, area, at) else { return false };
    let objects = on_page(doc, page);
    // The last one drawn is on top.
    let Some((rect, pos)) = objects.into_iter().rev().find(|(r, _)| {
        x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
    }) else {
        return false;
    };
    let edge = 4.0 / doc.view.zoom;
    let near_edge = x - rect.x < edge || rect.x + rect.w - x < edge || y - rect.y < edge || rect.y + rect.h - y < edge;
    if box_holds_text(doc, &pos) && !near_edge {
        return false;
    }
    if doc.session.selected_object().as_ref() != Some(&pos) {
        doc.view.cropping = false;
    }
    doc.session.select_object(pos);
    doc.view.object_drag = Some(ObjectDrag {
        handle: Handle::Move,
        page,
        start: rect,
        from: (x, y),
        to: (x, y),
        crop: false,
    });
    true
}

/// Whether the object at `pos` is a text box.
fn box_holds_text(doc: &Doc, pos: &Pos) -> bool {
    let Some(paragraph) = unluminous_word::position::paragraph(&doc.session.document, &pos.story, &pos.path) else {
        return false;
    };
    let mut at = 0;
    for run in &paragraph.runs {
        if at == pos.offset {
            if let unluminous_word::model::Content::Object(el) = &run.content {
                return unluminous_word::read::descendant(el, "w:txbxContent").is_some();
            }
            return false;
        }
        at += run.len();
    }
    false
}

/// The rectangle a drag would leave the object with, in page points. A corner keeps the ratio, as
/// Word's corner handles do, unless Shift is held.
pub fn dragged_rect(drag: &ObjectDrag, free: bool) -> unluminous_word::layout::Rect {
    let mut r = drag.start;
    let (dx, dy) = (drag.to.0 - drag.from.0, drag.to.1 - drag.from.1);
    let (mut left, mut top, mut right, mut bottom) = (r.x, r.y, r.x + r.w, r.y + r.h);
    match drag.handle {
        Handle::Move => {
            r.x += dx;
            r.y += dy;
            return r;
        }
        Handle::Top => top += dy,
        Handle::Bottom => bottom += dy,
        Handle::Left => left += dx,
        Handle::Right => right += dx,
        Handle::TopLeft => {
            left += dx;
            top += dy;
        }
        Handle::TopRight => {
            right += dx;
            top += dy;
        }
        Handle::BottomLeft => {
            left += dx;
            bottom += dy;
        }
        Handle::BottomRight => {
            right += dx;
            bottom += dy;
        }
    }
    let corner = matches!(drag.handle, Handle::TopLeft | Handle::TopRight | Handle::BottomLeft | Handle::BottomRight);
    if corner && !free && !drag.crop && r.w > 0.0 && r.h > 0.0 {
        let width = (right - left).abs().max(4.0);
        let height = width * r.h / r.w;
        match drag.handle {
            Handle::TopLeft | Handle::TopRight => top = bottom - height,
            _ => bottom = top + height,
        }
    }
    let (x0, x1) = (left.min(right), left.max(right));
    let (y0, y1) = (top.min(bottom), top.max(bottom));
    unluminous_word::layout::Rect { x: x0, y: y0, w: (x1 - x0).max(4.0), h: (y1 - y0).max(4.0) }
}

/// The pointer moved while an object is held.
pub fn drag(doc: &mut Doc, area: Rect, at: Pos2) {
    let Some((_, x, y)) = page_point(doc, area, at) else { return };
    if let Some(drag) = &mut doc.view.object_drag {
        drag.to = (x, y);
    }
}

/// The pointer let go: the change the drag describes is made, one undo step. Answers what was done, or
/// why it could not be.
pub fn release(doc: &mut Doc, area: Rect, at: Pos2, free: bool) -> Option<Result<String, String>> {
    let drag = doc.view.object_drag.take()?;
    let moved = (drag.to.0 - drag.from.0).abs() + (drag.to.1 - drag.from.1).abs() > 1.0;
    if !moved {
        return None;
    }
    let rect = dragged_rect(&drag, free);
    let info = doc.session.object_info()?;
    let result = match (drag.handle, drag.crop) {
        (Handle::Move, _) if info.wrap == "inline" => {
            let target = page_point(doc, area, at).and_then(|(page, x, y)| {
                doc.pages.hit(page, x, y, &super::page_view::clickable(doc, page, x, y, false))
            });
            match target {
                Some(target) => doc.session.move_object_to(target).map(|_| "Moved".to_owned()),
                None => Err("there is no text there".into()),
            }
        }
        (Handle::Move, _) => doc.session.move_object_on_page(rect.x, rect.y).map(|_| "Moved".to_owned()),
        (_, true) => {
            // A crop handle cuts by how far it moved in, as a share of the whole picture.
            let (l0, t0, r0, b0) = info.crop;
            let full_w = drag.start.w / (1.0 - l0 - r0).max(0.01);
            let full_h = drag.start.h / (1.0 - t0 - b0).max(0.01);
            let l = l0 + (rect.x - drag.start.x) / full_w;
            let t = t0 + (rect.y - drag.start.y) / full_h;
            let r = r0 + (drag.start.x + drag.start.w - rect.x - rect.w) / full_w;
            let b = b0 + (drag.start.y + drag.start.h - rect.y - rect.h) / full_h;
            let p = |f: f32| (f * 100.0).max(0.0);
            doc.session.crop_object(p(l), p(t), p(r), p(b)).map(|_| "Cropped".to_owned())
        }
        _ => doc.session.set_object_size(rect.w, Some(rect.h)).map(|_| "Resized".to_owned()),
    };
    Some(result)
}

/// Draw the selected object's frame and handles, and the outline of where a drag would leave it.
pub fn paint(painter: &egui::Painter, doc: &Doc, area: Rect, free: bool) {
    let Some((page, rect)) = selected(doc) else { return };
    let Some(screen) = to_screen(doc, area, page, rect) else { return };
    let accent = crate::theme::color::accent();
    painter.rect_stroke(screen, 0.0, Stroke::new(1.0, accent), egui::StrokeKind::Middle);
    for (handle, point) in handles(screen) {
        if doc.view.cropping {
            // Crop handles are black bars along the edges and corners, as Word draws them.
            let horizontal = matches!(handle, Handle::Top | Handle::Bottom);
            let size = if horizontal { Vec2::new(14.0, 4.0) } else if matches!(handle, Handle::Left | Handle::Right) { Vec2::new(4.0, 14.0) } else { Vec2::splat(8.0) };
            painter.rect_filled(Rect::from_center_size(point, size), 0.0, Color32::from_gray(20)); // any ground: Word's crop handles are black
        } else {
            painter.circle_filled(point, 4.0, Color32::WHITE); // any ground: a handle sits on the page
            painter.circle_stroke(point, 4.0, Stroke::new(1.0, accent));
        }
    }
    if let Some(drag) = &doc.view.object_drag {
        if let Some(outline) = to_screen(doc, area, drag.page, dragged_rect(drag, free)) {
            painter.rect_stroke(outline, 0.0, Stroke::new(1.0, accent.gamma_multiply(0.7)), egui::StrokeKind::Middle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_keeps_the_ratio_and_an_edge_does_not() {
        let start = unluminous_word::layout::Rect { x: 10.0, y: 10.0, w: 100.0, h: 50.0 };
        let drag = |handle, to| ObjectDrag { handle, page: 0, start, from: (110.0, 60.0), to, crop: false };
        let r = dragged_rect(&drag(Handle::BottomRight, (210.0, 60.0)), false);
        assert_eq!((r.w, r.h), (200.0, 100.0));
        let r = dragged_rect(&drag(Handle::BottomRight, (210.0, 60.0)), true);
        assert_eq!((r.w, r.h), (200.0, 50.0), "Shift frees the ratio");
        let r = dragged_rect(&drag(Handle::Right, (160.0, 90.0)), false);
        assert_eq!((r.w, r.h), (150.0, 50.0));
        let r = dragged_rect(&drag(Handle::Move, (120.0, 80.0)), false);
        assert_eq!((r.x, r.y, r.w), (20.0, 30.0, 100.0));
    }
}
