//! The draggable divider between two panes.
//!
//! Every pane in Unluminous is resized by dragging its edge, and every one of them uses this. A later pane
//! must use it too rather than growing its own: the grab width, the highlight, the pointer shape and the
//! double click that puts the pane back to its usual size are decided here once.
//!
//! The divider is drawn as a one pixel line, which is what the design shows, but it is grabbed over
//! [`GRAB`] pixels centred on that line, because a one pixel target cannot be hit with a mouse.

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme::color;

/// How wide the invisible grab area is, centred on the line.
pub const GRAB: f32 = 8.0;

/// Which way the divider runs, and so which way dragging it moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// An upright line between two panes side by side. Dragging it changes a width.
    Upright,
    /// A flat line between two panes above and below. Dragging it changes a height.
    Flat,
}

/// What a divider was asked to do this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Drag {
    /// How far the pointer moved along the axis since the last frame, in points.
    pub delta: f32,
    /// The divider was double clicked, which means put the pane back to its usual size.
    pub reset: bool,
}

/// Every divider drawn this frame, by the rectangle it can be grabbed over.
///
/// **What the window's own resize grips are cut against.** The grips are added last and take the
/// outermost few points of the window, so a divider that reaches an edge was underneath one — and both
/// set the same double headed cursor, so a person aiming at the divider got the window's dead edge
/// instead. `task-2062`, and `components::resize_edges::without` is the other half.
///
/// **Recorded by [`show`] itself rather than worked out again where the grips are added.** That is
/// `follow_the_open_file`'s rule: a list of the places that have to say "I drew a divider here" is a
/// list whose next entry is the one that forgets, and there are eight callers of `show` today. It rides
/// egui's own per-frame data rather than a field on the window, so a divider drawn by a component that
/// has never heard of `UnluminousApp` — the ones inside the references modal and Find in Files — is in
/// the list for nothing.
#[derive(Debug, Default, Clone)]
struct Grabbed(Vec<Rect>);

/// Where the list lives for the length of one frame.
fn grabbed_id() -> egui::Id {
    egui::Id::new("splitter-grabbed")
}

/// Forget the dividers of the frame before, which the window does at the top of each frame.
pub fn forget_last_frames_dividers(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<Grabbed>(grabbed_id()));
}

/// Where every divider drawn so far this frame can be grabbed.
///
/// Read after every panel and pane has been drawn, which is where the resize grips are added, so by
/// then this is all of them. It is the same ordering rule the panel drag and the tab drag are settled
/// under: the earliest moment anything knows where all of them are.
pub fn dividers_drawn_this_frame(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data(|data| data.get_temp::<Grabbed>(grabbed_id()).map(|held| held.0).unwrap_or_default())
}

/// Draw a divider and report the drag.
///
/// `line` is the one pixel line to draw: for an upright divider a rectangle one point wide running the
/// height of the panes, and for a flat one a rectangle one point tall running their width. The grab area
/// is worked out from it.
pub fn show(ui: &mut egui::Ui, line: Rect, id: &str, axis: Axis) -> Drag {
    let hit = match axis {
        Axis::Upright => Rect::from_min_max(
            Pos2::new(line.center().x - GRAB / 2.0, line.top()),
            Pos2::new(line.center().x + GRAB / 2.0, line.bottom()),
        ),
        Axis::Flat => Rect::from_min_max(
            Pos2::new(line.left(), line.center().y - GRAB / 2.0),
            Pos2::new(line.right(), line.center().y + GRAB / 2.0),
        ),
    };
    // Recorded before anything else, so that a divider is in the list whatever this function goes on to
    // do with it. The window's own resize grips are cut against these — see [`Grabbed`].
    ui.ctx().data_mut(|data| data.get_temp_mut_or_default::<Grabbed>(grabbed_id()).0.push(hit));
    let response = ui.interact(hit, ui.id().with(("splitter", id)), Sense::click_and_drag());
    let active = response.hovered() || response.dragged();
    if active {
        ui.ctx().set_cursor_icon(match axis {
            Axis::Upright => egui::CursorIcon::ResizeHorizontal,
            Axis::Flat => egui::CursorIcon::ResizeVertical,
        });
    }

    // The line itself, brighter while it is being pointed at so it is clear it can be moved.
    let colour = if active { color::accent() } else { color::divider() };
    let drawn = if active {
        match axis {
            Axis::Upright => Rect::from_center_size(line.center(), Vec2::new(2.0, line.height())),
            Axis::Flat => Rect::from_center_size(line.center(), Vec2::new(line.width(), 2.0)),
        }
    } else {
        line
    };
    ui.painter().rect_filled(drawn, CornerRadius::ZERO, colour);

    let delta = if response.dragged() {
        match axis {
            Axis::Upright => response.drag_delta().x,
            Axis::Flat => response.drag_delta().y,
        }
    } else {
        0.0
    };
    // A divider is a control, so it is named for the tests and for assistive technology.
    let name = format!("Resize {id}");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), name.clone())
    });
    Drag { delta, reset: response.double_clicked() }
}

/// Draw a plain divider with nothing draggable about it.
pub fn line(painter: &egui::Painter, from: Pos2, to: Pos2) {
    painter.line_segment([from, to], Stroke::new(1.0, color::divider()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_divider_reports_no_movement_when_nothing_happens() {
        assert_eq!(Drag::default(), Drag { delta: 0.0, reset: false });
    }
}
