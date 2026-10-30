//! The project tabs in the title bar, and the dropdown the plus opens (`task-2322`).
//!
//! *"We want the ability to open N projects in a window ... switch via tabs at the top."* A window holds one
//! `UnluminousApp` a project tab (`app::project_tabs`). This is what the title bar draws of them: a carved track
//! holding a pill a project, the one showing raised out of it, and after the last a round `+` chip. They are the
//! Realm bar's pills and chip, at the Realm bar's sizes, which is what the ticket asks for.
//!
//! The plus opens [`show_the_dropdown`], which is the Realm bar's folder dropdown made for projects: the recent
//! projects newest first, a search well, and a folder chip beside the well that opens the platform's folder
//! picker. A press on a project opens it in a new tab.
//!
//! Nothing here opens, shows or closes a tab. Each function reports what was asked for and the window does it.

use std::path::PathBuf;

use egui::{Pos2, Rect, Sense, Vec2};
use rux::icon::{Icon, Mark};
use rux::Style;

use crate::components::realm::dialog;
use crate::components::{modal, settings_kit as kit};

/// The track the tabs sit in, and a tab's pill inside it. The Realm bar's numbers.
const TRACK: f32 = 36.0;
const PILL: f32 = 28.0;
const TRACK_PAD: f32 = 4.0;
const PILL_GAP: f32 = 3.0;
/// The plus chip, and the gap in front of it.
const PLUS: f32 = 30.0;
const PLUS_GAP: f32 = 8.0;
/// How much room the row that counts the tabs left out takes.
const MORE: f32 = 62.0;
/// The widest a pill grows, so one long folder name cannot take the whole bar.
const PILL_MOST: f32 = 220.0;
/// The narrowest a pill shrinks to when the bar is crowded: the mark and a few letters of the name.
const PILL_LEAST: f32 = 66.0;
/// Below this width a pill draws its name with no folder mark in front of it.
const MARKED_LEAST: f32 = 96.0;
/// The folder mark at the start of every pill, and the room it takes.
const MARK: f32 = 13.0;

/// What the tab strip reported.
#[derive(Debug, Clone, PartialEq)]
pub struct StripOutcome {
    /// A tab was pressed: show it.
    pub show: Option<usize>,
    /// A tab was right clicked: where the pointer was, and which tab.
    pub menu: Option<(Pos2, usize)>,
    /// The plus, or the row counting the tabs left out, was pressed.
    pub plus: bool,
    /// Where the plus was drawn, which the dropdown hangs from.
    pub plus_rect: Rect,
    /// Where the strip ended, which is where the title bar's drag area starts.
    pub right: f32,
}

impl Default for StripOutcome {
    /// Nothing pressed and nothing drawn.
    fn default() -> Self {
        Self { show: None, menu: None, plus: false, plus_rect: Rect::NOTHING, right: 0.0 }
    }
}

/// How wide a tab's pill is for a name this wide: the mark, the words and the padding, floored so a short name is
/// still a target and capped so a long one leaves room for the others.
pub fn pill_width_for(word: f32) -> f32 {
    (12.0 + MARK + 7.0 + word + 14.0).clamp(64.0, PILL_MOST)
}

/// Where each tab goes between `from` and `to`, given each name's natural width: which tabs are drawn, in order,
/// and how wide each one's pill is.
///
/// Every tab at its natural width while they all fit. When they do not, the pills **shrink to share the room**,
/// as a browser's tabs do, each down to [`PILL_LEAST`]; a pill narrower than its share keeps its own width and
/// the others share what it leaves. Only when even that does not fit are tabs left out, the first ones kept and
/// counted by the `+N more` row, and the tab that is showing is always one of those drawn.
pub fn lay_the_tabs_out(widths: &[f32], showing: usize, from: f32, to: f32) -> Vec<(usize, f32)> {
    let room = to - from - TRACK_PAD * 2.0 - PLUS - PLUS_GAP;
    let gaps = |count: usize| PILL_GAP * count.saturating_sub(1) as f32;
    let natural: f32 = widths.iter().sum::<f32>() + gaps(widths.len());
    if natural <= room {
        return widths.iter().copied().enumerate().collect();
    }
    let at_least =
        widths.iter().map(|width| width.min(PILL_LEAST)).sum::<f32>() + gaps(widths.len());
    if at_least <= room {
        let share = share_for(widths, room - gaps(widths.len()));
        return widths.iter().map(|width| width.min(share)).enumerate().collect();
    }
    // Some are left out: as many at their least as fit beside the `+N more` row.
    let room = room - MORE - PILL_GAP;
    let fit = (((room + PILL_GAP) / (PILL_LEAST + PILL_GAP)).floor() as usize).max(1);
    let mut drawn: Vec<usize> = (0..widths.len().min(fit)).collect();
    if !drawn.contains(&showing) && showing < widths.len() {
        drawn.pop();
        drawn.push(showing);
    }
    drawn.into_iter().map(|index| (index, widths[index].min(PILL_LEAST))).collect()
}

/// The widest each pill may be so the pills fill `room` and no more: a pill narrower than that keeps its own
/// width. Found by halving, which a handful of tabs settles in a few dozen steps.
fn share_for(widths: &[f32], room: f32) -> f32 {
    let (mut low, mut high) = (PILL_LEAST, PILL_MOST);
    for _ in 0..30 {
        let middle = (low + high) / 2.0;
        let used: f32 = widths.iter().map(|width| width.min(middle)).sum();
        match used <= room {
            true => low = middle,
            false => high = middle,
        }
    }
    low
}

/// Draw the tabs and the plus into the title bar, starting at `from` and stopping before `to`.
///
/// `names` is one name a tab, in order, and `showing` is the one the window is drawing. A window with one
/// project draws one pill and the plus, which is the project chip the bar always had with a plus after it.
///
/// @param ui - the title bar
/// @param area - the title bar's rectangle
/// @param names - each tab's name
/// @param showing - the tab that is showing
/// @param from - where the strip starts
/// @param to - where it must stop
pub fn show_the_strip(
    ui: &mut egui::Ui,
    area: Rect,
    names: &[String],
    showing: usize,
    from: f32,
    to: f32,
) -> StripOutcome {
    let mut outcome = StripOutcome { right: from, plus_rect: Rect::NOTHING, ..Default::default() };
    let middle = area.center().y;
    // **A bar with no room for a pill still has a way to the tabs.** A narrow window at a large interface size
    // fills the bar with the menus and the tools, and the tabs vanishing there would leave a window whose other
    // projects nobody can reach. So the strip becomes one round chip that opens the dropdown, which lists every
    // project open here and shows one when it is pressed.
    if to - from < TRACK_PAD * 2.0 + PILL_LEAST + PLUS + PLUS_GAP {
        if to - from < PLUS {
            return outcome;
        }
        let chip = Rect::from_min_size(Pos2::new(from, middle - PLUS / 2.0), Vec2::splat(PLUS));
        let name = format!("Project tabs, {} open", names.len());
        crate::components::chrome_kit::layer(ui, "project-tabs-compact", chip, 16.0, |rux| {
            if rux::components::ChipButton::new(Icon::Project, PLUS, &name)
                .mark(13.0, Some(1.7))
                .show(rux, chip)
                .clicked()
            {
                outcome.plus = true;
            }
        });
        outcome.plus_rect = chip;
        outcome.right = chip.right();
        return outcome;
    }
    let style = Style::sans(12.0).medium();
    let strip = Rect::from_min_max(Pos2::new(from, area.top()), Pos2::new(to, area.bottom()));
    crate::components::chrome_kit::layer(ui, "project-tabs", strip, 16.0, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        let widths: Vec<f32> =
            names.iter().map(|name| pill_width_for(rux.measure(style, name).x)).collect();
        let drawn = lay_the_tabs_out(&widths, showing, from, to);
        let left_out = names.len() - drawn.len();
        let pills: f32 = drawn.iter().map(|(_, width)| width).sum::<f32>()
            + PILL_GAP * drawn.len().saturating_sub(1) as f32;
        let more = match left_out {
            0 => 0.0,
            _ => MORE + PILL_GAP,
        };
        let track = Rect::from_min_max(
            Pos2::new(from, middle - TRACK / 2.0),
            Pos2::new(from + TRACK_PAD * 2.0 + pills + more, middle + TRACK / 2.0),
        );
        rux.chrome.surface(track, TRACK / 2.0, chat.well, chat.carved_sm());
        let mut pen = track.left() + TRACK_PAD;
        for (index, width) in drawn {
            let name = &names[index];
            let chip =
                Rect::from_min_size(Pos2::new(pen, middle - PILL / 2.0), Vec2::new(width, PILL));
            pen = chip.right() + PILL_GAP;
            let on = index == showing;
            let response =
                rux.ui.interact(chip, rux.ui.id().with(("project-tab", index)), Sense::click());
            let hovered = response.hovered();
            if on {
                rux.chrome.surface(
                    chip,
                    PILL / 2.0,
                    rux::Fill::gradient(chat.chip, chip),
                    chat.raised_chip(),
                );
            } else if hovered {
                rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let ink = match (on, hovered) {
                (true, _) => theme.ink.i900,
                (false, true) => chat.sky,
                (false, false) => theme.ink.i500,
            };
            let mark_ink = match on {
                true => chat.sky,
                false => ink,
            };
            // A pill shrunk below its own width and below [`MARKED_LEAST`] gives the mark's room to the
            // name, which is what tells two narrow pills apart.
            let marked = width >= MARKED_LEAST || width >= widths[index] - 0.5;
            if marked {
                rux.mark(
                    Mark::new(Icon::Project, MARK).stroke(1.7),
                    Pos2::new(chip.left() + 12.0 + MARK / 2.0, middle),
                    mark_ink,
                );
            }
            let words_from = match marked {
                true => chip.left() + 12.0 + MARK + 7.0,
                false => chip.left() + 12.0,
            };
            let words = Rect::from_min_max(
                Pos2::new(words_from, chip.top()),
                Pos2::new(chip.right() - 12.0, chip.bottom()),
            );
            let galley = rux.elided(style, name, ink, words.width());
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(words.left(), middle),
                galley,
                style,
                ink,
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    on,
                    format!("Project tab: {name}"),
                )
            });
            if response.clicked() && !on {
                outcome.show = Some(index);
            }
            if response.secondary_clicked() {
                if let Some(at) = response.interact_pointer_pos().or_else(|| response.hover_pos()) {
                    outcome.menu = Some((at, index));
                }
            }
        }
        if left_out > 0 {
            let row =
                Rect::from_min_size(Pos2::new(pen, middle - PILL / 2.0), Vec2::new(MORE, PILL));
            let response =
                rux.ui.interact(row, rux.ui.id().with("project-tabs-more"), Sense::click());
            let ink = match response.hovered() {
                true => chat.sky,
                false => theme.ink.i500,
            };
            if response.hovered() {
                rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let said = format!("+{left_out} more");
            let galley = rux.text(Style::sans(11.5), &said, ink);
            rux::text::draw_centred(rux.painter(), row, galley, ink);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Project tabs, {said}"),
                )
            });
            if response.clicked() {
                outcome.plus = true;
            }
        }
        // After the last tab, which is where a browser puts it and where the Realm bar puts its own.
        let plus = Rect::from_min_size(
            Pos2::new(track.right() + PLUS_GAP, middle - PLUS / 2.0),
            Vec2::splat(PLUS),
        );
        outcome.plus_rect = plus;
        if rux::components::ChipButton::new(Icon::Plus, PLUS, "New Project Tab")
            .mark(12.0, Some(1.8))
            .show(rux, plus)
            .clicked()
        {
            outcome.plus = true;
        }
        outcome.right = plus.right();
    });
    outcome
}

/// One project the dropdown offers.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub folder: PathBuf,
    pub name: String,
    /// Which tab of this window it is open in, when it is.
    pub tab: Option<usize>,
    /// Whether that tab is the one showing.
    pub showing: bool,
}

/// What the dropdown holds while it is open.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DropdownState {
    pub filter: String,
    /// Which row the arrow keys are on, counted within what the filter left.
    pub highlighted: usize,
    /// Whether the search well has been given the keyboard, which is done once, as it opens.
    pub focused: bool,
}

/// What the dropdown reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DropdownOutcome {
    /// Open this project in a new tab, a second one when it is already open.
    pub open: Option<PathBuf>,
    /// The folder chip was pressed: ask the platform's folder picker for a folder.
    pub choose_a_folder: bool,
    /// Escape, a press outside it, or a choice.
    pub closed: bool,
}

/// How wide the card is.
pub const WIDTH: f32 = 440.0;
const LIST_MOST: f32 = 384.0;
const ROW: f32 = 52.0;
const PAD: f32 = 16.0;
const HEADING: f32 = 24.0;
const SEARCH: f32 = 38.0;
const GAP: f32 = 12.0;
const RADIUS: f32 = 24.0;

/// Which rows the filter leaves: the name or the folder holds what was typed.
pub fn matching(rows: &[Row], filter: &str) -> Vec<Row> {
    let wanted = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            wanted.is_empty()
                || row.name.to_lowercase().contains(&wanted)
                || row.folder.to_string_lossy().to_lowercase().contains(&wanted)
        })
        .cloned()
        .collect()
}

/// How tall the card is for this many rows.
fn card_height(rows: usize) -> f32 {
    let list = (rows.max(1) as f32 * ROW + 12.0).min(LIST_MOST);
    PAD + HEADING + GAP + SEARCH + GAP + list + PAD
}

/// Draw the dropdown under `anchor`, the plus, and report what was chosen.
///
/// The Realm bar's folder dropdown (`components::realm::recent`), made for projects. The folder chip at the
/// right of the search well is the way to a folder that is not in the list: it opens the platform's own folder
/// picker.
///
/// @param ctx - the window's context
/// @param anchor - the plus it hangs from, in the window's points
/// @param state - what it holds while it is open
/// @param rows - the recent projects, newest first
pub fn show_the_dropdown(
    ctx: &egui::Context,
    anchor: Rect,
    state: &mut DropdownState,
    rows: &[Row],
) -> DropdownOutcome {
    let mut outcome = DropdownOutcome::default();
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
    let height = card_height(found.len()).min(screen.height() - 24.0).max(180.0);
    let left = anchor.left().min(screen.right() - WIDTH - 12.0).max(screen.left() + 12.0);
    let card =
        Rect::from_min_size(Pos2::new(left, anchor.bottom() + 8.0), Vec2::new(WIDTH, height));
    egui::Area::new(egui::Id::new("unluminous-project-tabs-dropdown"))
        .order(egui::Order::Foreground)
        .fixed_pos(card.min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_min_size(card.size());
            kit::layer_reaching(ui, "project-dropdown-surface", card, modal::SHADOW_REACH, |rux| {
                let chat = rux.theme().chat;
                rux.chrome.surface(
                    card,
                    RADIUS,
                    rux::Fill::gradient(chat.card, card),
                    modal::shadow(),
                );
            });
            let inner = card.shrink(PAD);
            let list = kit::layer(ui, "project-dropdown", inner, |rux| {
                let theme = rux.theme();
                let style = Style::sans(14.0).semibold().tracking(-0.01);
                let heading_middle = inner.top() + HEADING / 2.0;
                let galley = rux.text(style, "Open a project in a new tab", theme.ink.i900);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.left() + 2.0, heading_middle),
                    galley,
                    style,
                    theme.ink.i900,
                );
                let count = match rows.len() {
                    1 => "1 recent".to_owned(),
                    many => format!("{many} recent"),
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
                // The search well, and the folder chip beside it that opens the platform's folder picker.
                let top = inner.top() + HEADING + GAP;
                let chip = Rect::from_min_size(
                    Pos2::new(inner.right() - SEARCH, top),
                    Vec2::splat(SEARCH),
                );
                let well = Rect::from_min_max(
                    Pos2::new(inner.left(), top),
                    Pos2::new(chip.left() - 8.0, top + SEARCH),
                );
                let entry = dialog::search(
                    rux,
                    well,
                    "Find a project",
                    "Search recent projects",
                    &mut state.filter,
                );
                if !state.focused {
                    entry.request_focus();
                    state.focused = true;
                }
                if rux::components::ChipButton::new(Icon::Folder, SEARCH, "Choose a folder")
                    .mark(15.0, Some(1.7))
                    .show(rux, chip)
                    .clicked()
                {
                    outcome.choose_a_folder = true;
                }
                let list = Rect::from_min_max(
                    Pos2::new(inner.left(), well.bottom() + GAP),
                    Pos2::new(inner.right(), inner.bottom()),
                );
                kit::list_well(rux, list, 18.0);
                list
            });
            show_the_rows(ui, list.shrink(6.0), &found, state, &mut outcome);
        });
    if enter {
        if let Some(row) = found.get(state.highlighted) {
            outcome.open = Some(row.folder.clone());
        }
    }
    let pressed_outside = ctx.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .interact_pos()
                .is_some_and(|at| !card.contains(at) && !anchor.contains(at))
    });
    if escape || pressed_outside || outcome.open.is_some() || outcome.choose_a_folder {
        outcome.closed = true;
    }
    outcome
}

/// The rows, in a scrolling list: a folder mark, the name and where it is, and whether it is open here already.
fn show_the_rows(
    ui: &mut egui::Ui,
    area: Rect,
    found: &[Row],
    state: &mut DropdownState,
    outcome: &mut DropdownOutcome,
) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.set_clip_rect(ui.clip_rect().intersect(area));
    egui::ScrollArea::vertical().id_salt("project-dropdown-rows").auto_shrink([false, false]).show(
        &mut child,
        |ui| {
            if found.is_empty() {
                let rect =
                    Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), ROW));
                kit::layer(ui, "project-dropdown-none", rect, |rux| {
                    let theme = rux.theme();
                    let said =
                        "No recent project is called that. The folder button chooses any folder.";
                    let galley = rux.text(Style::sans(12.0), said, theme.ink.i400);
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
            kit::layer(ui, "project-dropdown-rows", total, |rux| {
                for (index, row) in found.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        Pos2::new(total.left(), top + index as f32 * ROW),
                        Vec2::new(width, ROW - 4.0),
                    );
                    draw_a_row(rux, rect, row, index == state.highlighted, outcome);
                }
            });
            ui.allocate_space(total.size());
        },
    );
}

/// One row. A press puts its folder in `outcome.open`.
fn draw_a_row(
    rux: &mut rux::Rux<'_>,
    rect: Rect,
    row: &Row,
    highlighted: bool,
    outcome: &mut DropdownOutcome,
) {
    let response =
        dialog::row(rux, rect, &format!("project {}", row.folder.display()), highlighted);
    let theme = rux.theme();
    let chat = theme.chat;
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let disc =
        Rect::from_center_size(Pos2::new(rect.left() + 24.0, rect.center().y), Vec2::splat(30.0));
    rux.chrome.surface(disc, 15.0, chat.well, chat.carved_sm());
    let mark_tint = match row.showing {
        true => chat.sky,
        false => theme.ink.i500,
    };
    rux.mark(Mark::new(Icon::Project, 14.0).stroke(1.7), disc.center(), mark_tint);

    let mut right = rect.right() - 14.0;
    let badge = match (row.showing, row.tab.is_some()) {
        (true, _) => Some(("Showing", chat.sky)),
        (false, true) => Some(("Open", theme.ink.i500)),
        (false, false) => None,
    };
    if let Some((word, tint)) = badge {
        let style = Style::sans(10.5).semibold();
        let galley = rux.text(style, word, tint);
        let pill = Rect::from_min_size(
            Pos2::new(right - galley.size().x - 16.0, rect.center().y - 9.0),
            Vec2::new(galley.size().x + 16.0, 18.0),
        );
        rux.chrome.surface(pill, 9.0, chat.well, chat.carved_sm());
        rux::text::draw_centred(rux.painter(), pill, galley, tint);
        right = pill.left() - 10.0;
    }
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
    let place = row.folder.display().to_string();
    let galley = rux.elided(Style::sans(11.0), &place, theme.ink.i400, room);
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
            format!("Recent project: {}", row.name),
        )
    });
    if response.clicked() {
        outcome.open = Some(row.folder.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tab at its own width while they all fit.
    #[test]
    fn every_tab_is_drawn_at_its_own_width_while_they_all_fit() {
        let laid = lay_the_tabs_out(&[100.0, 120.0, 100.0], 2, 0.0, 1000.0);
        assert_eq!(laid, vec![(0, 100.0), (1, 120.0), (2, 100.0)]);
    }

    /// When they do not, the pills shrink to share the room before any tab is left out, and a short pill keeps
    /// its own width.
    #[test]
    fn crowded_pills_shrink_to_share_the_room_before_any_is_left_out() {
        let widths = [220.0, 220.0, 100.0];
        let room = 340.0 + TRACK_PAD * 2.0 + PLUS + PLUS_GAP;
        let laid = lay_the_tabs_out(&widths, 0, 0.0, room);
        assert_eq!(laid.len(), 3, "all three are drawn: {laid:?}");
        assert_eq!(laid[2].1, 100.0, "the short one keeps its width");
        let used: f32 = laid.iter().map(|(_, width)| width).sum::<f32>() + PILL_GAP * 2.0;
        assert!(used <= 340.5, "and together they fit: {used}");
        assert!(laid[0].1 >= PILL_LEAST && laid[0].1 < 220.0);
    }

    /// When even the least does not fit, the first are drawn, and the showing tab is always one of them.
    #[test]
    fn the_showing_tab_is_drawn_however_many_are_left_out() {
        let widths = [150.0; 12];
        let laid = lay_the_tabs_out(&widths, 11, 0.0, 600.0);
        let drawn: Vec<usize> = laid.iter().map(|(index, _)| *index).collect();
        assert!(drawn.len() < widths.len(), "not all twelve fit in 600 points: {drawn:?}");
        assert!(drawn.contains(&11), "the tab that is showing is drawn: {drawn:?}");
        assert!(drawn.contains(&0), "and the first are still there: {drawn:?}");
    }

    /// A short name is still a target and a long one cannot take the bar.
    #[test]
    fn a_pill_is_never_too_small_to_press_or_wide_enough_to_take_the_bar() {
        assert_eq!(pill_width_for(1.0), 64.0);
        assert_eq!(pill_width_for(900.0), PILL_MOST);
    }

    #[test]
    fn the_filter_matches_a_name_or_a_folder() {
        let rows = vec![
            Row {
                folder: PathBuf::from("/code/alpha"),
                name: "alpha".to_owned(),
                tab: None,
                showing: false,
            },
            Row {
                folder: PathBuf::from("/work/beta"),
                name: "beta".to_owned(),
                tab: Some(0),
                showing: true,
            },
        ];
        assert_eq!(matching(&rows, "").len(), 2);
        assert_eq!(matching(&rows, "ALPHA").len(), 1, "case does not matter");
        assert_eq!(matching(&rows, "work").len(), 1, "the folder counts");
    }
}
