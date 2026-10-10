//! The dropdown of suggestions, drawn under the caret.
//!
//! It reports what was clicked and decides nothing, which is the rule every component in Unluminous
//! follows: what is offered and what accepting means are both `app::completion`'s.
//!
//! ## It is not an `egui::Popup`, and that is the whole of its shape
//!
//! egui keeps at most one popup open at a time — the rule that already turned the text options
//! panel's three line spacings into three buttons and that puts the colour wheel *inside* the text
//! menu rather than over it. This list has to coexist with nothing at all, but it also must never
//! take the keyboard: the document keeps it, typing flows into the file underneath, and the
//! dropdown is a picture of an offer rather than a control being used. So it is an
//! [`egui::Area`] on the foreground order, positioned by the window from the caret's own geometry
//! every frame, exactly as cheap to draw as the menu it resembles — and it neither opens nor closes
//! anything else.
//!
//! ## Where it goes
//!
//! Under the caret's own line, flipped **above** it when the rows would cross the bottom of the
//! pane, and clamped inside the pane horizontally. The caret's box is the same arithmetic the caret
//! itself is painted with, handed over by the pane that drew it, so the list follows the writing
//! rather than being placed at a remembered point.
//!
//! Up to eight rows, and more scroll: the list is drawn from `CompletionState::shown`, which the
//! window keeps in step with the chosen row.

use egui::{CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::completion::CompletionState;
use crate::components::controls;
use crate::theme::{color, icon, size};

/// One row. A menu row, which is what `design/style-guide.md` gives a list of things to choose
/// between — the same 24 points the menu bar, the context menus and the text menu all use.
pub const ROW: f32 = 24.0;
/// How wide the list is. Wide enough for a long identifier and its signature beside it (`task-2231`
/// §6.6 took it from 360 to 480), and narrow enough that it reads as a list hanging off a word.
const WIDTH: f32 = 480.0;
/// How wide the documentation panel to the right of the list is.
pub const PANEL: f32 = 360.0;
/// The most lines of documentation the panel shows; the rest is cut with an ellipsis.
const PANEL_LINES: usize = 12;
/// The frame's own margin, matching `components::context_menu`'s.
const PADDING: f32 = 6.0;
/// How far below the caret's line the list hangs, so it never touches the letters it is about.
const GAP: f32 = 4.0;
/// How wide the column holding the kind glyph is.
const GLYPH: f32 = 20.0;

/// What happened in the list this frame.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    /// A row was clicked, by name. The window takes it exactly as `Enter` would — the click never
    /// reaches the editing area, because the list's own `Area` is in front of it and takes the hit.
    pub accepted: Option<String>,
}

/// Draw the list, and the documentation panel beside it when `documented` says the chosen row has
/// rested long enough to be read about. `caret` is the caret's box on the screen and `pane` is the
/// editing area it is in.
pub fn show(
    ui: &mut egui::Ui,
    state: &CompletionState,
    caret: Rect,
    pane: Rect,
    documented: bool,
) -> Outcome {
    let mut outcome = Outcome::default();
    let shown = state.shown();
    if shown.is_empty() {
        return outcome;
    }
    let area = where_it_goes(shown.len(), caret, pane);
    egui::Area::new(egui::Id::new("unluminous-completion"))
        .order(egui::Order::Foreground)
        .fixed_pos(area.min)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            // Reserve the whole of it, so egui knows where the list is for the pointer's sake; the
            // drawing itself is at absolute positions, as everything in Unluminous is.
            ui.allocate_space(area.size());
            frame(ui, area);
            for (offset, index) in shown.clone().enumerate() {
                let row = Rect::from_min_size(
                    Pos2::new(area.left() + PADDING, area.top() + PADDING + offset as f32 * ROW),
                    Vec2::new(area.width() - PADDING * 2.0, ROW),
                );
                if draw_row(ui, row, &state.rows[index], index == state.chosen) {
                    outcome.accepted = Some(state.rows[index].name.clone());
                }
            }
        });
    if documented {
        if let Some(row) = state.chosen_row() {
            documentation(ui, row, area, pane);
        }
    }
    outcome
}

/// The documentation panel: the chosen row's signature and its documentation, beside the list, on the
/// side of it there is room for. Nothing is drawn for a row with nothing to say.
///
/// Named `Completion documentation`, so a test can find it.
fn documentation(
    ui: &mut egui::Ui,
    row: &unluminous_core::completion::Row,
    list: Rect,
    pane: Rect,
) {
    let signature = row.info.signature.clone().filter(|s| *s != row.name);
    let doc = row.info.doc.clone();
    if signature.is_none() && doc.is_none() {
        return;
    }
    let mut lines: Vec<String> = Vec::new();
    if let Some(signature) = &signature {
        lines.push(signature.clone());
    }
    if let Some(doc) = &doc {
        lines.extend(doc.lines().map(str::to_owned));
    }
    if lines.len() > PANEL_LINES {
        lines.truncate(PANEL_LINES);
        lines.push("\u{2026}".to_owned());
    }
    let height = lines.len() as f32 * 16.0 + PADDING * 2.0;
    let area = documentation_goes(list, pane, height);
    egui::Area::new(egui::Id::new("unluminous-completion-documentation"))
        .order(egui::Order::Foreground)
        .fixed_pos(area.min)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            let response =
                ui.allocate_rect(Rect::from_min_size(area.min, area.size()), Sense::hover());
            frame(ui, area);
            let painter = ui.painter();
            for (at, line) in lines.iter().enumerate() {
                let (font, tint) = match at == 0 && signature.is_some() {
                    true => (egui::FontId::monospace(11.5), color::text_strong()),
                    false => (egui::FontId::proportional(11.5), color::text_control()),
                };
                let galley = painter.layout(line.clone(), font, tint, PANEL - PADDING * 2.0);
                let y = area.top() + PADDING + at as f32 * 16.0;
                painter.with_clip_rect(area.shrink(PADDING / 2.0)).galley(
                    Pos2::new(area.left() + PADDING, y),
                    galley,
                    tint,
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Completion documentation")
            });
        });
}

/// Where the documentation panel goes: beside the list on the right, else on the left, and when the
/// pane has no room on either side, under the list, or above it when there is no room under it. It is
/// never drawn over the list, whose rows are what the panel is about.
///
/// A pure function of its three arguments, so it can be checked with no window.
///
/// @param list - the list's rectangle
/// @param pane - the editing area
/// @param height - the panel's height
pub fn documentation_goes(list: Rect, pane: Rect, height: f32) -> Rect {
    let size = Vec2::new(PANEL, height);
    if list.right() + GAP + PANEL <= pane.right() {
        return Rect::from_min_size(Pos2::new(list.right() + GAP, list.top()), size);
    }
    if list.left() - GAP - PANEL >= pane.left() {
        return Rect::from_min_size(Pos2::new(list.left() - GAP - PANEL, list.top()), size);
    }
    let left = list.left().min(pane.right() - PANEL).max(pane.left());
    let top = match list.bottom() + GAP + height <= pane.bottom() {
        true => list.bottom() + GAP,
        false => list.top() - GAP - height,
    };
    Rect::from_min_size(Pos2::new(left, top), size)
}

/// Where the list is drawn: under the caret, flipped above it near the bottom of the pane, and
/// clamped inside the pane's left and right edges.
///
/// A pure function of four numbers, so the flip and the clamp can be checked with no window.
pub fn where_it_goes(rows: usize, caret: Rect, pane: Rect) -> Rect {
    let height = rows as f32 * ROW + PADDING * 2.0;
    let width = WIDTH.min(pane.width().max(120.0));
    let below = caret.bottom() + GAP;
    // Above the caret's own line when the rows would cross the bottom of the pane, and only then:
    // under the word is where the eye already is.
    let top = if below + height > pane.bottom() && caret.top() - GAP - height >= pane.top() {
        caret.top() - GAP - height
    } else {
        below
    };
    let top = top.min((pane.bottom() - height).max(pane.top())).max(pane.top());
    let left = caret.left().min(pane.right() - width).max(pane.left());
    Rect::from_min_size(Pos2::new(left, top), Vec2::new(width, height))
}

/// The popup frame: the menu fill and the one point border every menu in Unluminous is drawn with.
fn frame(ui: &egui::Ui, area: Rect) {
    ui.painter().rect(
        area,
        CornerRadius::same(size::CONTROL_CORNER),
        color::menu(),
        Stroke::new(1.0, color::control_border()),
        egui::StrokeKind::Inside,
    );
}

/// One row: the kind glyph, the name with its matched letters picked out, and the quiet suffix.
///
/// Named `Completion draw_frame`, because the screenshot tests find controls by name and a control
/// with no name cannot be tested at all.
fn draw_row(
    ui: &mut egui::Ui,
    area: Rect,
    row: &unluminous_core::completion::Row,
    chosen: bool,
) -> bool {
    let name = format!("Completion {}", row.name);
    let response = ui.interact(area, ui.id().with(("completion", &row.name)), Sense::click());
    let painter = ui.painter();
    // One pill, drawn one way: the same `SELECTED_ROW` fill the explorer's open file and every menu
    // row's hover already use.
    if chosen {
        controls::pill(painter, area, 4);
    } else if response.hovered() {
        painter.rect_filled(area, CornerRadius::same(4), color::control());
    }
    if let Some(kind) = row.kind {
        icon::completion_kind(
            painter,
            Pos2::new(area.left() + GLYPH / 2.0 + 2.0, area.center().y),
            kind,
            color::text_dim(),
        );
    }
    let tint = if chosen { color::text_strong() } else { color::text_control() };
    // The matched letters in the accent colour, which is how `Find in Files` and `Go to File`
    // already answer "why is this row here".
    let galley = controls::marked_text(
        painter,
        &row.name,
        &row.matched,
        tint,
        egui::FontId::proportional(12.5),
    );
    let left = area.left() + GLYPH + 6.0;
    let name_width = galley.size().x;
    let name_top = area.center().y - galley.size().y / 2.0;
    painter.galley(Pos2::new(left, name_top), galley, tint);
    // A deprecated row is struck through, as the reference editor draws one.
    if row.info.deprecated {
        let y = area.center().y;
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(left + name_width, y)],
            Stroke::new(1.0, tint),
        );
    }
    let suffix_text = row_detail(row);
    if !suffix_text.is_empty() {
        // Cut to the column there is: from just after the name to the row's right edge.
        let room = (area.right() - 6.0 - (left + name_width + 12.0)).max(0.0);
        let suffix = painter.layout_no_wrap(
            suffix_text,
            egui::FontId::proportional(11.0),
            color::text_faint(),
        );
        let clip = Rect::from_min_max(Pos2::new(area.right() - 6.0 - room, area.top()), area.max);
        painter.with_clip_rect(clip).galley(
            Pos2::new(
                area.right() - 6.0 - suffix.size().x.min(room),
                area.center().y - suffix.size().y / 2.0,
            ),
            suffix,
            color::text_faint(),
        );
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, &name));
    response.clicked()
}

/// What a row says after its name: the import it would add, its signature cut to what follows the
/// name, or its detail.
///
/// @param row - the row
pub fn row_detail(row: &unluminous_core::completion::Row) -> String {
    if row.info.needs_import.is_some() {
        return format!("\u{00B7} {}", row.detail);
    }
    let signature = row.info.signature.as_deref().unwrap_or_default();
    let tail = signature.strip_prefix(row.name.as_str()).filter(|t| !t.is_empty());
    // A detail the signature already ends with, the `f32` after `-> f32`, is said once.
    let said =
        tail.is_some_and(|t| !row.detail.is_empty() && t.trim_end().ends_with(row.detail.trim()));
    match (tail, row.detail.is_empty() || said) {
        (Some(tail), true) => tail.to_owned(),
        (Some(tail), false) => format!("{tail}  \u{00B7} {}", row.detail),
        (None, false) => format!("\u{00B7} {}", row.detail),
        (None, true) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(800.0, 600.0))
    }

    #[test]
    fn the_documentation_panel_never_covers_the_list() {
        // Room on the right: beside it.
        let list = Rect::from_min_size(Pos2::new(120.0, 100.0), Vec2::new(300.0, 200.0));
        let beside = documentation_goes(list, pane(), 80.0);
        assert!(beside.left() >= list.right());
        // No room on either side of a wide list in a narrow pane: under it.
        let wide = Rect::from_min_size(Pos2::new(300.0, 100.0), Vec2::new(480.0, 200.0));
        let under = documentation_goes(wide, pane(), 80.0);
        assert!(!under.intersects(wide), "{under:?} covers {wide:?}");
        assert!(under.top() >= wide.bottom());
        // And above it when it hangs at the bottom of the pane.
        let low = Rect::from_min_size(Pos2::new(300.0, 500.0), Vec2::new(480.0, 140.0));
        let above = documentation_goes(low, pane(), 80.0);
        assert!(!above.intersects(low));
        assert!(above.bottom() <= low.top());
    }

    #[test]
    fn the_list_hangs_under_the_caret_and_stays_inside_the_pane() {
        let caret = Rect::from_min_size(Pos2::new(300.0, 200.0), Vec2::new(2.0, 18.0));
        let area = where_it_goes(5, caret, pane());
        assert!(area.top() > caret.bottom(), "under the caret's own line");
        assert_eq!(area.left(), caret.left(), "and lined up with it");
        assert!(pane().contains_rect(area), "{area:?} is outside {:?}", pane());
    }

    #[test]
    fn a_caret_near_the_bottom_of_the_pane_puts_the_list_above_it() {
        // Scenario 22: the rows would cross the bottom, so the list flips.
        let caret = Rect::from_min_size(Pos2::new(300.0, 620.0), Vec2::new(2.0, 18.0));
        let area = where_it_goes(8, caret, pane());
        assert!(area.bottom() < caret.top(), "above the caret: {area:?}");
        assert!(pane().contains_rect(area), "and still on the screen");
    }

    #[test]
    fn a_caret_near_the_right_hand_edge_clamps_the_list_inside_the_pane() {
        let caret = Rect::from_min_size(Pos2::new(880.0, 200.0), Vec2::new(2.0, 18.0));
        let area = where_it_goes(3, caret, pane());
        assert!(area.right() <= pane().right() + 0.01, "{area:?}");
        assert!(area.left() >= pane().left() - 0.01);
    }

    #[test]
    fn a_pane_too_short_to_hold_the_list_either_way_still_puts_it_on_the_screen() {
        // A pane dragged down to nothing is not a reason to draw a list off the window.
        let short = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(300.0, 60.0));
        let caret = Rect::from_min_size(Pos2::new(120.0, 90.0), Vec2::new(2.0, 18.0));
        let area = where_it_goes(8, caret, short);
        assert_eq!(area.top(), short.top(), "clamped to the top rather than drawn off the pane");
        assert!(area.width() <= short.width() + 0.01, "and no wider than the pane");
    }
}
