//! The modal a right click on the canvas opens: search at the top, a list of node kinds under it.
//!
//! `task-1904` asks for it and shows it — a search field, a scrolling list, a highlighted row. Four
//! kinds is a short list today; the field is there because the ticket says *"we'll have so many
//! similar components"*, and a list that gets long later should not need a second design then.
//!
//! It is `components::modal`, so it has the frame, the header, the dragging, the resizing and the
//! Enter that presses the last button that every other dialog in Unluminous has. A tenth modal drawing
//! its own header would be a tenth modal that almost agrees with the other nine.

use egui::{Align2, FontId, Pos2, Rect, Sense, Vec2};

use crate::components::{controls, modal};
use crate::services::space::Kind;
use crate::theme::{color, size};

/// The size it opens at. Tall enough for every kind with room to grow, and narrow, because the list is
/// names rather than sentences.
const WIDTH: f32 = 380.0;
const HEIGHT: f32 = 360.0;
/// How tall one row is.
///
/// A name and two lines of summary, which is what the longest of the four needs: at 46 the second
/// line of the terminal's own description ran into the row under it, which the first picture showed.
const ROW: f32 = 60.0;

/// What the person has typed and which row is picked out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub filter: String,
    /// Which row the arrow keys are on, counted within what the filter left.
    pub highlighted: usize,
    /// Where on the canvas the node will go, in world points, which is where the right click was.
    pub at: egui::Pos2,
}

/// What the modal reported.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// A kind was chosen, so a node of it goes on the canvas.
    pub chose: Option<Kind>,
    pub closed: bool,
}

/// Which kinds the filter leaves, in the order they are drawn.
///
/// A plain case insensitive match on the name and on the one line under it, which is what a list of
/// four wants. `services::file_search`'s subsequence ranking is for a list of two thousand.
pub fn matching(filter: &str) -> Vec<Kind> {
    let wanted = filter.trim().to_lowercase();
    Kind::ALL
        .into_iter()
        .filter(|kind| {
            wanted.is_empty()
                || kind.label().to_lowercase().contains(&wanted)
                || kind.summary().to_lowercase().contains(&wanted)
        })
        .collect()
}

/// Draw the modal and report what was chosen.
pub fn show(ctx: &egui::Context, state: &mut State) -> Outcome {
    let mut outcome = Outcome::default();
    let (chosen, closed) = modal::show(ctx, "unluminous-add-node", WIDTH, HEIGHT, |ui, area| {
        let mut chosen = None;
        if modal::header(ui, area, "Add a node") {
            chosen = Some(None);
        }
        let body = modal::body(area);
        let field = Rect::from_min_size(body.min, Vec2::new(body.width(), 28.0));
        let response =
            controls::search_field(ui, field, "Search nodes", "Search nodes", &mut state.filter);
        // The keyboard starts in the field, which is what a modal opened to be typed into means.
        if !response.has_focus() && ui.memory(|memory| memory.focused().is_none()) {
            response.request_focus();
        }
        let found = matching(&state.filter);
        state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
        walk_with_the_arrow_keys(ui, state, found.len());
        let list = Rect::from_min_max(Pos2::new(body.left(), field.bottom() + 10.0), body.max);
        if let Some(kind) = show_the_rows(ui, list, &found, state) {
            chosen = Some(Some(kind));
        }
        if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            if let Some(kind) = found.get(state.highlighted) {
                chosen = Some(Some(*kind));
            }
        }
        chosen
    });
    match chosen {
        Some(Some(kind)) => {
            outcome.chose = Some(kind);
            outcome.closed = true;
        }
        Some(None) => outcome.closed = true,
        None => outcome.closed = closed,
    }
    outcome
}

/// Up and down walk the list, and neither wraps.
///
/// Not wrapping is the explorer's own answer: a list of four that jumped from the bottom to the top
/// would be a list whose end nobody can feel.
fn walk_with_the_arrow_keys(ui: &egui::Ui, state: &mut State, rows: usize) {
    if rows == 0 {
        state.highlighted = 0;
        return;
    }
    let (down, up) = ui.input(|input| {
        (input.key_pressed(egui::Key::ArrowDown), input.key_pressed(egui::Key::ArrowUp))
    });
    if down {
        state.highlighted = (state.highlighted + 1).min(rows - 1);
    }
    if up {
        state.highlighted = state.highlighted.saturating_sub(1);
    }
}

/// The rows themselves: a name, the line under it, and the pill on whichever is highlighted.
fn show_the_rows(ui: &mut egui::Ui, area: Rect, found: &[Kind], state: &mut State) -> Option<Kind> {
    let mut chosen = None;
    if found.is_empty() {
        ui.painter_at(area).text(
            Pos2::new(area.left() + 4.0, area.top() + 12.0),
            Align2::LEFT_TOP,
            "Nothing is called that.",
            FontId::proportional(12.0),
            color::text_faint(),
        );
        return None;
    }
    for (index, kind) in found.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(area.left(), area.top() + index as f32 * ROW),
            Vec2::new(area.width(), ROW - 4.0),
        );
        if row.bottom() > area.bottom() {
            break;
        }
        let response = ui.interact(row, ui.id().with(("add-node", kind.name())), Sense::click());
        if response.hovered() {
            state.highlighted = index;
        }
        let on = index == state.highlighted;
        let painter = ui.painter_at(area);
        if on {
            painter.rect_filled(row, size::CONTROL_CORNER, color::selected_row());
        }
        painter.text(
            Pos2::new(row.left() + 12.0, row.top() + 9.0),
            Align2::LEFT_TOP,
            kind.label(),
            FontId::proportional(13.0),
            color::text_strong(),
        );
        let galley = painter.layout(
            kind.summary().to_owned(),
            FontId::proportional(11.0),
            color::text_dim(),
            row.width() - 24.0,
        );
        painter.galley(Pos2::new(row.left() + 12.0, row.top() + 26.0), galley, color::text_dim());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, kind.label())
        });
        if response.clicked() {
            chosen = Some(*kind);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_a_name_or_the_line_under_it() {
        assert_eq!(matching(""), Kind::ALL.to_vec());
        assert_eq!(matching("brow"), vec![Kind::Browser]);
        assert_eq!(matching("BROWSER"), vec![Kind::Browser], "case does not matter");
        // The summaries are what make a search useful on a list nobody has memorised: "shell" is not
        // in the word `Terminal`.
        assert_eq!(matching("shell"), vec![Kind::Terminal]);
        assert!(matching("nothing like this").is_empty());
    }
}
