//! Signature help: one line above the caret while it is between a call's brackets, holding the
//! callable's parameters with the one being typed in bold. `task-2231` §6.8.
//!
//! It is an `egui::Area` that never takes the keyboard and never takes a click, drawn after the pane
//! loop from the caret the pane recorded, which is the completion popup's arrangement made a second
//! time. Above the caret rather than under it, because the completion list hangs under the caret
//! and the two are often open together.

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::signature::Signature;
use crate::theme::{color, size};

/// The padding inside the line's frame.
const PADDING: f32 = 6.0;
/// The gap between the line and the caret's own line.
const GAP: f32 = 4.0;
/// The size of the words.
const TEXT: f32 = 12.0;
/// The widest the line may be, past which the words are cut with an ellipsis.
const WIDEST: f32 = 640.0;

/// Draws the line for a signature above a caret, inside a pane.
///
/// Named `Signature help`, so a test and an agent can find it.
///
/// @param ui - the window's `Ui`
/// @param signature - the callable and which parameter is being typed
/// @param caret - the caret's box on the screen
/// @param pane - the editing area it is in, which the line is kept inside
pub fn show(ui: &mut egui::Ui, signature: &Signature, caret: Rect, pane: Rect) {
    let job = layout_job(signature);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let width = (galley.size().x + PADDING * 2.0).min(WIDEST).min(pane.width());
    let height = galley.size().y + PADDING * 2.0;
    let area = where_it_goes(width, height, caret, pane);
    egui::Area::new(egui::Id::new("unluminous-signature-help"))
        .order(egui::Order::Foreground)
        .fixed_pos(area.min)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            let response = ui.allocate_rect(area, Sense::hover());
            ui.painter().rect(
                area,
                CornerRadius::same(size::CONTROL_CORNER),
                color::menu(),
                Stroke::new(1.0, color::control_border()),
                egui::StrokeKind::Inside,
            );
            ui.painter().with_clip_rect(area.shrink(PADDING / 2.0)).galley(
                Pos2::new(area.left() + PADDING, area.top() + PADDING),
                galley,
                color::text_control(),
            );
            let label = signature.label.clone();
            response.widget_info(move || {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Label,
                    true,
                    format!("Signature help: {label}"),
                )
            });
        });
}

/// The label as text in two weights: the parameter being typed strong, everything else quiet.
///
/// @param signature - the callable and which parameter is being typed
fn layout_job(signature: &Signature) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let quiet = egui::TextFormat::simple(egui::FontId::monospace(TEXT), color::text_control());
    let strong = egui::TextFormat::simple(egui::FontId::monospace(TEXT), color::text_strong());
    let label = signature.label.as_str();
    let active = signature.active.and_then(|a| signature.parameters.get(a)).cloned();
    match active.filter(|r| {
        r.end <= label.len() && label.is_char_boundary(r.start) && label.is_char_boundary(r.end)
    }) {
        Some(range) => {
            job.append(&label[..range.start], 0.0, quiet.clone());
            let mut bold = strong;
            bold.underline = Stroke::new(1.0, color::text_strong());
            job.append(&label[range.clone()], 0.0, bold);
            job.append(&label[range.end..], 0.0, quiet);
        }
        None => job.append(label, 0.0, quiet),
    }
    job.wrap.max_rows = 1;
    job.wrap.max_width = WIDEST - PADDING * 2.0;
    job.wrap.break_anywhere = true;
    job
}

/// Where the line goes: above the caret's line, below it when there is no room above, and kept inside
/// the pane's left and right edges.
///
/// A pure function of its four arguments, so it can be checked with no window.
///
/// @param width - the line's width
/// @param height - the line's height
/// @param caret - the caret's box
/// @param pane - the editing area
pub fn where_it_goes(width: f32, height: f32, caret: Rect, pane: Rect) -> Rect {
    let above = caret.top() - GAP - height;
    let top = if above >= pane.top() { above } else { caret.bottom() + GAP };
    let left = caret.left().min(pane.right() - width).max(pane.left());
    Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_goes_above_the_caret_and_below_it_at_the_top_of_the_pane() {
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let caret = Rect::from_min_size(Pos2::new(100.0, 300.0), Vec2::new(2.0, 18.0));
        assert!(where_it_goes(200.0, 24.0, caret, pane).bottom() <= caret.top());
        let high = Rect::from_min_size(Pos2::new(100.0, 4.0), Vec2::new(2.0, 18.0));
        assert!(where_it_goes(200.0, 24.0, high, pane).top() >= high.bottom());
    }

    #[test]
    fn the_line_is_kept_inside_the_pane_on_the_right() {
        let pane = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 600.0));
        let caret = Rect::from_min_size(Pos2::new(390.0, 300.0), Vec2::new(2.0, 18.0));
        assert!(where_it_goes(200.0, 24.0, caret, pane).right() <= pane.right());
    }
}
