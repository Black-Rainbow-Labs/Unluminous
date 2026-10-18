//! The realm bar's folder dropdown: every realm file in the project, newest first.
//!
//! `task-2293`: *"On the left there should be a folder icon that when clicked has a dropdown of the realm files
//! in the project, sorted by most recent first. The list should be scrollable. When I click one, it should open
//! in a new tab."* The bar's pills are the realms open as tabs; this is every realm file, so a tab that was closed
//! is one press away from coming back.
//!
//! It hangs from the folder chip as a raised card on a foreground layer of its own, so it is drawn over the
//! canvas's nodes, and the one native web view steps aside for it as it does for every popup. A search well at
//! the top takes the keyboard as it opens, the list scrolls, and the arrow keys and Enter walk it the way the realm
//! manager's are walked. A row says which realm it is, where its file is, what is on it, how long ago it was
//! written, and whether it is already a tab. The foot makes a new realm or opens the manager.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use egui::{Pos2, Rect, Vec2};
use rux::icon::{Icon, Mark};
use rux::Style;

use crate::components::realm::dialog;
use crate::components::{modal, settings_kit as kit};

/// How wide the card is.
pub const WIDTH: f32 = 420.0;
/// The most the list grows before it scrolls.
const LIST_MOST: f32 = 384.0;
/// One row, and the card's own padding.
const ROW: f32 = 56.0;
const PAD: f32 = 16.0;
/// The heading's row, the search well and the buttons at the foot.
const HEADING: f32 = 24.0;
const SEARCH: f32 = 38.0;
const FOOT: f32 = 34.0;
const GAP: f32 = 12.0;
/// The card's corner.
const RADIUS: f32 = 24.0;

/// One realm file, as the dropdown draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: PathBuf,
    pub name: String,
    /// The folder the file is in, relative to the project.
    pub folder: String,
    /// When the file was last written.
    pub written: Option<SystemTime>,
    pub nodes: usize,
    /// Whether it is open as a tab on the bar.
    pub tab: bool,
    /// Whether it is the realm on the canvas.
    pub showing: bool,
}

/// What the dropdown holds while it is open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub filter: String,
    /// Which row the arrow keys are on, counted within what the filter left.
    pub highlighted: usize,
    /// Whether the search well has been given the keyboard, which is done once, as it opens.
    pub focused: bool,
}

/// What the dropdown reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    /// Open this realm, in a tab of its own when it is not one.
    pub open: Option<PathBuf>,
    /// `New Realm` was pressed.
    pub new_realm: bool,
    /// `All Realms…` was pressed, which opens the realm manager.
    pub manage: bool,
    /// Escape, a press outside it, or a choice.
    pub closed: bool,
}

/// Which rows the filter leaves, in the order they are drawn: the name or the folder holds what was typed.
pub fn matching(rows: &[Row], filter: &str) -> Vec<Row> {
    let wanted = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            wanted.is_empty()
                || row.name.to_lowercase().contains(&wanted)
                || row.folder.to_lowercase().contains(&wanted)
        })
        .cloned()
        .collect()
}

/// How long ago something was written, the way a person says it: `just now`, `4 min ago`, `3 h ago`,
/// `yesterday`, `5 days ago`, `3 weeks ago`, `8 months ago`.
pub fn ago(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    match () {
        _ if minutes < 1 => "just now".to_owned(),
        _ if hours < 1 => format!("{minutes} min ago"),
        _ if days < 1 => format!("{hours} h ago"),
        _ if days < 2 => "yesterday".to_owned(),
        _ if days < 14 => format!("{days} days ago"),
        _ if days < 60 => format!("{} weeks ago", days / 7),
        _ => format!("{} months ago", days / 30),
    }
}

/// How tall the card is for this many rows.
fn card_height(rows: usize) -> f32 {
    let list = (rows.max(1) as f32 * ROW + 12.0).min(LIST_MOST);
    PAD + HEADING + GAP + SEARCH + GAP + list + GAP + FOOT + PAD
}

/// Draw the dropdown under `anchor`, the folder chip, and report what was chosen.
///
/// @param ctx - the window's context
/// @param anchor - the chip it hangs from, in the window's points
/// @param state - what it holds while it is open
/// @param rows - every realm file, newest first
/// @param now - the time `ago` is measured from, so a test can fix it
pub fn show(
    ctx: &egui::Context,
    anchor: Rect,
    state: &mut State,
    rows: &[Row],
    now: SystemTime,
) -> Outcome {
    let mut outcome = Outcome::default();
    let found = matching(rows, &state.filter);
    let (down, up, enter, escape) = ctx.input_mut(|input| {
        (
            input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
            input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
        )
    });
    if down {
        state.highlighted = (state.highlighted + 1).min(found.len().saturating_sub(1));
    }
    if up {
        state.highlighted = state.highlighted.saturating_sub(1);
    }
    state.highlighted = state.highlighted.min(found.len().saturating_sub(1));

    let screen = ctx.content_rect();
    let height = card_height(found.len()).min(screen.height() - 24.0).max(200.0);
    let left = anchor.left().min(screen.right() - WIDTH - 12.0).max(screen.left() + 12.0);
    let card =
        Rect::from_min_size(Pos2::new(left, anchor.bottom() + 8.0), Vec2::new(WIDTH, height));
    egui::Area::new(egui::Id::new("unluminous-realm-recent"))
        .order(egui::Order::Foreground)
        .fixed_pos(card.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_min_size(card.size());
            kit::layer_reaching(ui, "realm-recent-surface", card, modal::SHADOW_REACH, |rux| {
                let chat = rux.theme().chat;
                rux.chrome.surface(
                    card,
                    RADIUS,
                    rux::Fill::gradient(chat.card, card),
                    modal::shadow(),
                );
            });
            let inner = card.shrink(PAD);
            let list = kit::layer(ui, "realm-recent", inner, |rux| {
                let theme = rux.theme();
                // The heading and how many there are.
                let style = Style::sans(14.0).semibold().tracking(-0.01);
                let heading_middle = inner.top() + HEADING / 2.0;
                let galley = rux.text(style, "Realm files", theme.ink.i900);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.left() + 2.0, heading_middle),
                    galley,
                    style,
                    theme.ink.i900,
                );
                let count = match rows.len() {
                    1 => "1 realm".to_owned(),
                    many => format!("{many} realms"),
                };
                let galley = rux.text(Style::sans(11.5), &count, theme.ink.i400);
                let width = galley.size().x;
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.right() - 2.0 - width, heading_middle),
                    galley,
                    Style::sans(11.5),
                    theme.ink.i400,
                );
                // The search well, which has the keyboard from the moment the dropdown opens.
                let well = Rect::from_min_size(
                    Pos2::new(inner.left(), inner.top() + HEADING + GAP),
                    Vec2::new(inner.width(), SEARCH),
                );
                let entry = dialog::search(
                    rux,
                    well,
                    "Find a realm file",
                    "Search realms",
                    &mut state.filter,
                );
                if !state.focused {
                    entry.request_focus();
                    state.focused = true;
                }
                // The two buttons at the foot: the quieter one first, so the bar's own plus and this read alike.
                let top = inner.bottom() - FOOT;
                let new_width = kit::button_width(rux, "New Realm", false);
                let new_realm =
                    Rect::from_min_size(Pos2::new(inner.left(), top), Vec2::new(new_width, FOOT));
                if kit::button(rux, new_realm, "New Realm", kit::Kind::Chip) {
                    outcome.new_realm = true;
                }
                let all_width = kit::button_width(rux, "All Realms\u{2026}", false);
                let all = Rect::from_min_size(
                    Pos2::new(inner.right() - all_width, top),
                    Vec2::new(all_width, FOOT),
                );
                if kit::button(rux, all, "All Realms\u{2026}", kit::Kind::Ghost) {
                    outcome.manage = true;
                }
                let list = Rect::from_min_max(
                    Pos2::new(inner.left(), well.bottom() + GAP),
                    Pos2::new(inner.right(), top - GAP),
                );
                kit::list_well(rux, list, 18.0);
                list
            });
            show_the_rows(ui, list.shrink(6.0), &found, state, now, &mut outcome);
        });
    if enter {
        if let Some(row) = found.get(state.highlighted) {
            outcome.open = Some(row.id.clone());
        }
    }
    // A press anywhere that is neither the card nor the chip it hangs from puts it away, which is what every
    // dropdown does. The chip's own press is the bar's, which toggles it.
    let pressed_outside = ctx.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|at| !card.contains(at) && !anchor.contains(at))
    });
    if escape || pressed_outside || outcome.open.is_some() || outcome.new_realm || outcome.manage {
        outcome.closed = true;
    }
    outcome
}

/// The rows, in a scrolling list: a mark, the name, where it is and what is on it, and on the right how long ago
/// it was written and whether it is a tab.
fn show_the_rows(
    ui: &mut egui::Ui,
    area: Rect,
    found: &[Row],
    state: &mut State,
    now: SystemTime,
    outcome: &mut Outcome,
) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.set_clip_rect(ui.clip_rect().intersect(area));
    egui::ScrollArea::vertical().id_salt("realm-recent-rows").auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            if found.is_empty() {
                let rect =
                    Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), ROW));
                kit::layer(ui, "realm-recent-none", rect, |rux| {
                    let theme = rux.theme();
                    let galley =
                        rux.text(Style::sans(12.0), "No realm file is called that", theme.ink.i400);
                    rux::text::draw_centred(rux.painter(), rect, galley, theme.ink.i400);
                });
                ui.allocate_space(rect.size());
                return;
            }
            let width = ui.available_width();
            let top = ui.cursor().top();
            let total = Rect::from_min_size(
                Pos2::new(ui.cursor().left(), top),
                Vec2::new(width, ROW * found.len() as f32),
            );
            kit::layer(ui, "realm-recent-rows", total, |rux| {
                for (index, row) in found.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        Pos2::new(total.left(), top + index as f32 * ROW),
                        Vec2::new(width, ROW - 4.0),
                    );
                    draw_a_row(rux, rect, row, index == state.highlighted, now, outcome);
                }
            });
            ui.allocate_space(total.size());
        },
    );
}

/// One row. Answers nothing: a press puts the realm in `outcome.open`.
fn draw_a_row(
    rux: &mut rux::Rux<'_>,
    rect: Rect,
    row: &Row,
    highlighted: bool,
    now: SystemTime,
    outcome: &mut Outcome,
) {
    let response = dialog::row(rux, rect, &format!("recent {}", row.id.display()), highlighted);
    let theme = rux.theme();
    let chat = theme.chat;
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // The realm's mark in a small carved disc, lit while it is the one on the canvas.
    let disc =
        Rect::from_center_size(Pos2::new(rect.left() + 26.0, rect.center().y), Vec2::splat(32.0));
    rux.chrome.surface(disc, 16.0, chat.well, chat.carved_sm());
    let mark_tint = match row.showing {
        true => chat.sky,
        false => theme.ink.i500,
    };
    rux.mark(Mark::new(Icon::Realm, 15.0), disc.center(), mark_tint);

    // On the right: how long ago, and whether it is a tab.
    let when = row
        .written
        .map(|written| ago(now.duration_since(written).unwrap_or_default()))
        .unwrap_or_default();
    let mut right = rect.right() - 14.0;
    let when_galley = rux.text(Style::sans(11.0), &when, theme.ink.i400);
    let when_width = when_galley.size().x;
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(right - when_width, rect.center().y - 8.0),
        when_galley,
        Style::sans(11.0),
        theme.ink.i400,
    );
    let badge = match (row.showing, row.tab) {
        (true, _) => Some(("Showing", chat.sky)),
        (false, true) => Some(("Open", theme.ink.i500)),
        (false, false) => None,
    };
    let mut badge_width: f32 = 0.0;
    if let Some((word, tint)) = badge {
        let style = Style::sans(10.5).semibold();
        let galley = rux.text(style, word, tint);
        let pill = Rect::from_min_size(
            Pos2::new(right - galley.size().x - 16.0, rect.center().y + 1.0),
            Vec2::new(galley.size().x + 16.0, 18.0),
        );
        rux.chrome.surface(pill, 9.0, chat.well, chat.carved_sm());
        rux::text::draw_centred(rux.painter(), pill, galley, tint);
        badge_width = pill.width();
    }
    right -= when_width.max(badge_width) + 14.0;

    // The name, and under it where the file is and what is on it.
    let text_left = disc.right() + 12.0;
    let room = (right - text_left).max(40.0);
    let name_style = Style::sans(13.0).medium();
    let tint = match row.showing {
        true => theme.ink.i900,
        false => theme.ink.i700,
    };
    let galley = rux.elided(name_style, &row.name, tint, room);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(text_left, rect.center().y - 8.0),
        galley,
        name_style,
        tint,
    );
    let detail = format!(
        "{} \u{00B7} {} node{}",
        row.folder,
        row.nodes,
        if row.nodes == 1 { "" } else { "s" }
    );
    let galley = rux.elided(Style::sans(11.0), &detail, theme.ink.i400, room);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(text_left, rect.center().y + 9.0),
        galley,
        Style::sans(11.0),
        theme.ink.i400,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            true,
            row.showing,
            format!("Realm file: {}", row.name),
        )
    });
    if response.clicked() {
        outcome.open = Some(row.id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, folder: &str) -> Row {
        Row {
            id: PathBuf::from(format!("{folder}/{name}.realm")),
            name: name.to_owned(),
            folder: folder.to_owned(),
            written: None,
            nodes: 0,
            tab: false,
            showing: false,
        }
    }

    #[test]
    fn the_filter_matches_a_name_or_a_folder() {
        let rows = vec![row("Plan", ".realm-files"), row("Sketch", "design/boards")];
        assert_eq!(matching(&rows, "").len(), 2);
        assert_eq!(matching(&rows, "PLAN").len(), 1, "case does not matter");
        assert_eq!(matching(&rows, "boards").len(), 1, "the folder counts");
        assert!(matching(&rows, "nothing like it").is_empty());
    }

    #[test]
    fn how_long_ago_reads_the_way_a_person_says_it() {
        assert_eq!(ago(Duration::from_secs(20)), "just now");
        assert_eq!(ago(Duration::from_secs(4 * 60)), "4 min ago");
        assert_eq!(ago(Duration::from_secs(3 * 3600)), "3 h ago");
        assert_eq!(ago(Duration::from_secs(30 * 3600)), "yesterday");
        assert_eq!(ago(Duration::from_secs(5 * 86_400)), "5 days ago");
        assert_eq!(ago(Duration::from_secs(21 * 86_400)), "3 weeks ago");
        assert_eq!(ago(Duration::from_secs(240 * 86_400)), "8 months ago");
    }

    #[test]
    fn the_card_grows_with_its_rows_and_then_the_list_scrolls() {
        assert!(card_height(2) < card_height(4));
        assert_eq!(card_height(40), card_height(400), "past the most, the list scrolls instead");
    }
}
