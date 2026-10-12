//! The modal a right click on the canvas opens: search at the top, a list of node kinds under it.
//!
//! `task-1904` asks for it and shows it — a search field, a scrolling list, a highlighted row. Four
//! kinds is a short list today; the field is there because the ticket says *"we'll have so many
//! similar components"*, and a list that gets long later should not need a second design then.
//!
//! It is `components::modal::show_unframed`, so it has the dragging, the resizing and the zoom every dialog
//! in Unluminous has, and since `task-2250` it is drawn with the parts of `components::realm::dialog`, which
//! are the New Data Source dialog's.

use egui::{Pos2, Rect, Vec2};
use rux::Style;

use crate::components::realm::dialog;
use crate::components::{modal, settings_kit};
use crate::services::realm::Kind;

/// The size it opens at. Narrow, because the list is names rather than sentences.
const WIDTH: f32 = 460.0;
/// How tall one row is.
///
/// A name and two lines of summary, which is what the longest of the four needs: at 46 the second
/// line of the terminal's own description ran into the row under it, which the first picture showed.
const ROW: f32 = 58.0;

/// Tall enough for every kind, **worked out rather than chosen**.
///
/// `task-1905`: at 360 the body ran from y=60 to y=300, the field left the list starting at y=98, and
/// the fourth row of 60 ended at y=334 — so [`show_the_rows`] broke out of its loop and `Kind::Editor`
/// was never drawn at all. The modal read as a list of three kinds with a large empty realm under it,
/// and the report it produced guessed the kind was missing from the modal rather than off the bottom of
/// it. A number typed here is a number the next kind added would break again, so this is the arithmetic
/// `modal::body` and `show_the_rows` really do.
const HEIGHT: f32 = dialog::PAD_Y
    + dialog::CLOSE
    + dialog::GAP
    + dialog::SEARCH
    + dialog::GAP
    + ROW * Kind::ALL.len() as f32
    + dialog::PAD_Y;

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
///
/// `task-2250` drew it in the Settings design's language: see `components::realm::dialog`. Each kind wears
/// the mark the rail and the node's own header use for it.
pub fn show(ctx: &egui::Context, state: &mut State) -> Outcome {
    let mut outcome = Outcome::default();
    let (chosen, closed) =
        modal::show_unframed(ctx, "unluminous-add-node", WIDTH, HEIGHT, |ui, area| {
            dialog::surface(ui, "add-node", area);
            let mut chosen = None;
            let inner = inner_of(area);
            let found = matching(&state.filter);
            state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
            walk_with_the_arrow_keys(ui, state, found.len());
            settings_kit::layer(ui, "add-node", inner, |rux| {
                if dialog::heading(rux, inner, "Add a node") {
                    chosen = Some(None);
                }
                let well = Rect::from_min_size(
                    Pos2::new(inner.left(), inner.top() + dialog::CLOSE + dialog::GAP),
                    Vec2::new(inner.width(), dialog::SEARCH),
                );
                let response =
                    dialog::search(rux, well, "Search nodes", "Search nodes", &mut state.filter);
                // The keyboard starts in the field, which is what a modal opened to be typed into means.
                if !response.has_focus() && rux.ui.memory(|memory| memory.focused().is_none()) {
                    response.request_focus();
                }
                let list = Rect::from_min_max(
                    Pos2::new(inner.left(), well.bottom() + dialog::GAP),
                    inner.max,
                );
                if let Some(kind) = show_the_rows(rux, list, &found, state) {
                    chosen = Some(Some(kind));
                }
            });
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

/// The room inside the dialog's padding.
fn inner_of(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + dialog::PAD_X, area.top() + dialog::PAD_Y),
        Pos2::new(area.right() - dialog::PAD_X, area.bottom() - dialog::PAD_Y),
    )
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

/// The rows themselves: the kind's mark in a chip, its name, the line under it, and the raised row on
/// whichever is highlighted.
fn show_the_rows(
    rux: &mut rux::Rux<'_>,
    area: Rect,
    found: &[Kind],
    state: &mut State,
) -> Option<Kind> {
    let mut chosen = None;
    let theme = rux.theme();
    if found.is_empty() {
        let galley = rux.text(Style::sans(12.0), "Nothing is called that.", theme.ink.i400);
        rux.painter().galley(
            Pos2::new(area.left() + 4.0, area.top() + 8.0),
            galley,
            theme.ink.i400,
        );
        return None;
    }
    for (index, kind) in found.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(area.left(), area.top() + index as f32 * ROW),
            Vec2::new(area.width(), ROW - 4.0),
        );
        if row.bottom() > area.bottom() + 1.0 {
            break;
        }
        let on = index == state.highlighted;
        let response = dialog::row(rux, row, kind.name(), on);
        if response.hovered() {
            state.highlighted = index;
        }
        let disc = Rect::from_center_size(
            Pos2::new(row.left() + 14.0 + 15.0, row.center().y),
            Vec2::splat(30.0),
        );
        rux::components::chip_disc(rux, disc);
        crate::components::realm::kind_mark_of(*kind)(rux.painter(), disc.center(), theme.chat.sky);
        let left = disc.right() + 12.0;
        let name_style = Style::sans(13.0).semibold();
        let galley = rux.text(name_style, kind.label(), theme.ink.i900);
        let name_height = galley.size().y;
        rux.painter().galley(Pos2::new(left, row.top() + 7.0), galley, theme.ink.i900);
        let galley = rux::text::wrapped(
            rux.painter(),
            Style::sans(11.0).leading(1.3),
            kind.summary(),
            theme.ink.i400,
            row.right() - 14.0 - left,
        );
        rux.painter().galley(
            Pos2::new(left, row.top() + 7.0 + name_height + 1.0),
            galley,
            theme.ink.i400,
        );
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

    /// Every kind fits inside the body, which is what makes it drawn rather than merely offered.
    ///
    /// `task-1905`: this is the arithmetic `show_the_rows` does, asserted rather than looked at. It
    /// fails at the 360 the modal opened at, where the fourth row ended six points past the body and
    /// the loop that draws the rows gave up — so `Kind::Editor` was in `matching("")`, which the test
    /// below already asserted, and on the screen it was nowhere.
    #[test]
    fn every_kind_has_room_to_be_drawn_in_the_modal() {
        let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH, HEIGHT));
        let body = inner_of(area);
        let list_top = body.top() + dialog::CLOSE + dialog::GAP + dialog::SEARCH + dialog::GAP;
        let last = list_top + (Kind::ALL.len() - 1) as f32 * ROW + ROW - 4.0;
        assert!(
            last <= body.bottom(),
            "the last of {} kinds ends at {last} and the body ends at {}",
            Kind::ALL.len(),
            body.bottom(),
        );
        // And not so much taller than it needs to be that a kind could be added without anybody
        // noticing the modal had stopped fitting: one row of slack, no more.
        assert!(body.bottom() - last < ROW, "the modal is more than a row taller than its list");
    }

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
