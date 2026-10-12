//! The modal that lists every realm in this project, so one can be found rather than scrolled past.
//!
//! `task-1906` asked for *"a space/view manager modal so i can open other saved spaces/tabs"*. It listed the
//! views inside `space.conf`; since `task-2202` a view is a `.realm` file, so it lists the project's realm
//! files, and its footer still opens another *project*, in a window of its own, because a project is a
//! window and a realm's nodes name that project's files.
//!
//! ## Why a modal at all, when the chips are already there
//!
//! `view_bar` breaks out of its loop when a chip would reach the zoom controls, so past about six views the
//! rest are not merely hard to reach — they are not drawn. The bar keeps its chips, because they are the
//! quick way between two or three, and gains one row at its end saying how many it is not showing.
//!
//! ## A row is one line
//!
//! `components::agent_tasks::listings` draws the distinction this follows: a card is a hundred points tall
//! and carries buttons because a lane holds a dozen of them, and a row is one line because a list holds
//! hundreds and what somebody is doing in one is **finding** something. So a row is a name, a count and
//! whether it is the one showing, and everything a person can *do* to a view is on the right click menu it
//! already has.

use egui::{Pos2, Rect, Vec2};
use rux::Style;

use crate::components::realm::dialog;
use crate::components::{modal, settings_kit};
use std::path::PathBuf;

/// A realm, named by where its file is in the project. `task-2202`: a realm file is what a view was.
type ViewId = PathBuf;
use crate::theme::color;

/// The size it opens at.
///
/// The same shape `Go to File` opens at, because it is the same kind of thing: a search box over a list you
/// are looking through. **One size for every state**, which is the Settings window's rule — a modal that grew
/// as views were added would be a modal that jumped under the pointer — so the list scrolls instead.
const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 500.0;
/// How tall one row of the list is.
const ROW: f32 = 44.0;

/// One row's worth of what the modal needs to know about a view.
///
/// A value rather than the `View` itself, because a component draws and does not reach into the window's
/// state — the rule `explorer::Decoration` already states. It is also what lets a test build the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: ViewId,
    pub name: String,
    pub nodes: usize,
    pub connections: usize,
    /// True for the one the canvas is showing.
    pub showing: bool,
}

/// What the modal is holding while it is open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub filter: String,
    /// Which row the arrow keys are on, counted within what the filter left.
    pub highlighted: usize,
}

/// What the modal reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    /// A view was chosen, so the canvas shows it.
    pub show: Option<ViewId>,
    /// A row was right clicked: where the pointer was, and which view.
    pub menu: Option<(Pos2, ViewId)>,
    /// `New Realm` was pressed.
    pub add: bool,
    /// `Open Another Project...` was pressed.
    pub open_a_project: bool,
    pub closed: bool,
}

/// Which rows the filter leaves, in the order they are drawn.
///
/// A plain case insensitive match on the name, which is what a list of this size wants — `services::file_search`'s
/// subsequence ranking is for a list of two thousand. The field is there for the reason the add modal's own
/// comment gives: a list that gets long later should not need a second design then.
pub fn matching(rows: &[Row], filter: &str) -> Vec<Row> {
    let wanted = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| wanted.is_empty() || row.name.to_lowercase().contains(&wanted))
        .cloned()
        .collect()
}

/// How many views the bar is not showing, given how many it drew.
///
/// What the bar's own overflow row says. Zero when every view has a chip, which is the ordinary case.
pub fn not_showing(total: usize, drawn: usize) -> usize {
    total.saturating_sub(drawn)
}

/// Draw the modal and report what was chosen.
///
/// `task-2250` drew it in the Settings design's language: see `components::realm::dialog`.
pub fn show(ctx: &egui::Context, state: &mut State, rows: &[Row]) -> Outcome {
    let mut outcome = Outcome::default();
    let (_, closed) = modal::show_unframed(
        ctx,
        "unluminous-realm-manager",
        WIDTH,
        HEIGHT,
        |ui, area| {
            dialog::surface(ui, "realm-manager", area);
            // The arrow keys and Enter are taken out of the frame's events before the field is drawn,
            // because egui leaves what a text box consumed in the list for everyone else to read and a list
            // moving is not the same as a caret moving. `go_to_file`'s own reason, kept.
            let (down, up, enter) = ui.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                )
            });
            let inner = Rect::from_min_max(
                Pos2::new(area.left() + dialog::PAD_X, area.top() + dialog::PAD_Y),
                Pos2::new(area.right() - dialog::PAD_X, area.bottom() - dialog::PAD_Y),
            );
            let found = matching(rows, &state.filter);
            if down {
                state.highlighted = (state.highlighted + 1).min(found.len().saturating_sub(1));
            }
            if up {
                state.highlighted = state.highlighted.saturating_sub(1);
            }
            state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
            let list = settings_kit::layer(ui, "realm-manager", inner, |rux| {
                if dialog::heading(rux, inner, "Realms") {
                    outcome.closed = true;
                }
                // **What a canvas is, said once**, because a control that silently did less than its name
                // suggests would be worse than one that says what it does — see §3.2 of the design.
                let below = settings_kit::paragraph(
                    rux,
                    Pos2::new(inner.left(), inner.top() + dialog::CLOSE + 8.0),
                    inner.width(),
                    "Every canvas in this project. A canvas belongs to the project it was made in, because \
                     its nodes name that project's files.",
                );
                let well = Rect::from_min_size(
                    Pos2::new(inner.left(), below + dialog::GAP - 4.0),
                    Vec2::new(inner.width(), dialog::SEARCH),
                );
                let entry =
                    dialog::search(rux, well, "Find a realm", "Type a name", &mut state.filter);
                // The box has the keyboard from the moment the modal opens, because a search box that has to
                // be clicked before it can be typed into is a search box that gets typed past.
                if !entry.has_focus() {
                    entry.request_focus();
                }
                let summary = match rows.len() {
                    1 => "1 realm".to_owned(),
                    many => format!("{many} realms"),
                };
                let theme = rux.theme();
                let galley = rux.text(Style::sans(11.5), &summary, theme.ink.i400);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.left() + 4.0, inner.bottom() - dialog::BUTTON / 2.0),
                    galley,
                    Style::sans(11.5),
                    theme.ink.i400,
                );
                // The last button is the one Enter presses, which is `modal::footer`'s rule — so the one that
                // opens a realm is last and the two that do something else come before it.
                match dialog::footer(
                    rux,
                    inner,
                    &[("Open Another Project…", false), ("New Realm", true)],
                    ("Open", found.get(state.highlighted).is_some()),
                ) {
                    Some(0) => outcome.open_a_project = true,
                    Some(1) => outcome.add = true,
                    Some(2) => {
                        if let Some(row) = found.get(state.highlighted) {
                            outcome.show = Some(row.id.clone());
                        }
                    }
                    _ => {}
                }
                let list = Rect::from_min_max(
                    Pos2::new(inner.left(), well.bottom() + dialog::GAP),
                    Pos2::new(inner.right(), inner.bottom() - dialog::BUTTON - dialog::GAP),
                );
                settings_kit::list_well(rux, list, 20.0);
                list
            });
            show_the_rows(ui, list.shrink(6.0), &found, state, &mut outcome);
            if enter {
                if let Some(row) = found.get(state.highlighted) {
                    outcome.show = Some(row.id.clone());
                }
            }
        },
    );
    if closed {
        outcome.closed = true;
    }
    if outcome.show.is_some() || outcome.add || outcome.open_a_project {
        outcome.closed = true;
    }
    outcome
}

/// The rows: a name, what is on it, and the raised row on whichever is highlighted.
fn show_the_rows(
    ui: &mut egui::Ui,
    area: Rect,
    found: &[Row],
    state: &mut State,
    outcome: &mut Outcome,
) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.set_clip_rect(ui.painter().clip_rect().intersect(area));
    egui::ScrollArea::vertical().id_salt("realm-manager-rows").show(&mut child, |ui| {
        if found.is_empty() {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("   Nothing is called that")
                    .size(12.0)
                    .color(color::text_faint()),
            );
            return;
        }
        let width = ui.available_width();
        let top = ui.cursor().top();
        let total = Rect::from_min_size(
            Pos2::new(ui.cursor().left(), top),
            Vec2::new(width, ROW * found.len() as f32),
        );
        settings_kit::layer(ui, "realm-manager-rows", total, |rux| {
            for (index, row) in found.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(total.left(), top + index as f32 * ROW),
                    Vec2::new(width, ROW - 4.0),
                );
                let on = index == state.highlighted;
                let response = dialog::row(rux, rect, &row.name, on);
                // **A press moves the choice; resting the pointer does not.** `go_to_file` reads its own rows
                // the same way and for the reason the Codex Sol review gave about this one: the pointer very
                // often sits still over a row while somebody walks the list with the arrow keys, so moving the
                // choice on hover put it back under the pointer on every frame and `Enter` then opened the row
                // the pointer happened to be over rather than the row the pill was on.
                if response.clicked() {
                    state.highlighted = index;
                }
                let theme = rux.theme();
                let name_style = Style::sans(13.0).medium();
                let tint = match row.showing {
                    true => theme.ink.i900,
                    false => theme.ink.i700,
                };
                let galley = rux.elided(name_style, &row.name, tint, rect.width() * 0.5);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(rect.left() + 16.0, rect.center().y),
                    galley,
                    name_style,
                    tint,
                );
                // What is on it, and whether it is the one showing — the two things that tell one canvas
                // from another when the names are all somebody's own words.
                let count = match row.connections {
                    0 => format!("{} node{}", row.nodes, if row.nodes == 1 { "" } else { "s" }),
                    wires => format!(
                        "{} node{} · {wires} connection{}",
                        row.nodes,
                        if row.nodes == 1 { "" } else { "s" },
                        if wires == 1 { "" } else { "s" },
                    ),
                };
                let mut right = rect.right() - 16.0;
                if row.showing {
                    let style = Style::sans(11.0).semibold();
                    let galley = rux.text(style, "showing", theme.chat.sky);
                    right -= galley.size().x;
                    rux::text::draw_left_capitals(
                        rux.painter(),
                        Pos2::new(right, rect.center().y),
                        galley,
                        style,
                        theme.chat.sky,
                    );
                    right -= 14.0;
                }
                let galley = rux.text(Style::sans(11.5), &count, theme.ink.i400);
                right -= galley.size().x;
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(right, rect.center().y),
                    galley,
                    Style::sans(11.5),
                    theme.ink.i400,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Button,
                        true,
                        row.showing,
                        format!("Realm: {}", row.name),
                    )
                });
                if response.double_clicked() {
                    outcome.show = Some(row.id.clone());
                }
                if response.secondary_clicked() {
                    if let Some(at) =
                        response.interact_pointer_pos().or_else(|| response.hover_pos())
                    {
                        outcome.menu = Some((at, row.id.clone()));
                    }
                }
            }
        });
        ui.allocate_space(total.size());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Row> {
        vec![
            Row {
                id: PathBuf::from("Main.realm"),
                name: "Main".to_owned(),
                nodes: 4,
                connections: 2,
                showing: true,
            },
            Row {
                id: PathBuf::from("Rendering.realm"),
                name: "Rendering".to_owned(),
                nodes: 7,
                connections: 5,
                showing: false,
            },
            Row {
                id: PathBuf::from("Notes.realm"),
                name: "Notes".to_owned(),
                nodes: 1,
                connections: 0,
                showing: false,
            },
        ]
    }

    #[test]
    fn the_filter_matches_a_name_and_nothing_else() {
        let all = rows();
        assert_eq!(matching(&all, "").len(), 3);
        assert_eq!(matching(&all, "rend").len(), 1);
        assert_eq!(matching(&all, "REND").len(), 1, "case does not matter");
        assert_eq!(matching(&all, "  notes  ").len(), 1, "and neither does room either side");
        assert!(matching(&all, "nothing like this").is_empty());
    }

    /// The highlighted row is always one that is there, however the filter narrows the list.
    ///
    /// The index counts within **what the filter left**, so it has to be clamped whenever that list gets
    /// shorter — otherwise `Enter` opens nothing on a list of three narrowed to one, or worse, the row under
    /// the index is not the row the pill was drawn on.
    #[test]
    fn the_highlighted_row_is_always_one_that_is_there() {
        let all = rows();
        // Walked to the end of the full list, then narrowed to one row.
        let mut state = State { filter: String::new(), highlighted: 2 };
        let found = matching(&all, &state.filter);
        assert_eq!(found.len(), 3);
        assert!(found.get(state.highlighted).is_some(), "the last row of three");

        state.filter = "notes".to_owned();
        let found = matching(&all, &state.filter);
        assert_eq!(found.len(), 1);
        // This is what `show` does before it reads the row, and what makes `Enter` open the one on the screen.
        state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
        assert_eq!(state.highlighted, 0);
        assert_eq!(
            found[state.highlighted].name, "Notes",
            "the row the pill is on is the row Enter opens"
        );

        // And a filter matching nothing leaves an index nothing is read at.
        state.filter = "nothing like this".to_owned();
        let found = matching(&all, &state.filter);
        state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
        assert!(found.get(state.highlighted).is_none(), "there is nothing to open");
    }

    #[test]
    fn the_bar_says_how_many_it_is_not_showing() {
        // Zero when every view has a chip, which is the ordinary case and the one where no row is drawn.
        assert_eq!(not_showing(3, 3), 0);
        assert_eq!(not_showing(9, 6), 3);
        // And never negative, which a bar that drew more chips than there are views would ask for.
        assert_eq!(not_showing(2, 5), 0);
    }
}
