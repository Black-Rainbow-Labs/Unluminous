//! The workspace: a strip of pages, a `New query` button, and whichever page is showing on a card.
//!
//! **The plugin contributes one pane**, so the pane holds a strip of its own — which is the shape the
//! Services tool window has in the reference screenshots' `db_ui_query_console_result_tab.png`. A console
//! and a row editor are the two kinds of page, and both are drawn on the same card.
//!
//! `task-2250` drew it in the language the Agent-Tasks board's header uses: the pages are pills in a carved
//! track with the one showing raised out of it, the same as the board's views, and `New query` is a round
//! chip after the last of them, which is where a browser puts its plus and where the Realm bar puts its own.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Fill, Style};

use crate::components::database::{console, grid, kit, Act};
use crate::services::database::{DatabaseExplorer, Sheet};
use crate::services::plugin_ui::Look;

/// How tall the strip of pages is: a 40 point track holding 32 point pills.
pub const STRIP: f32 = 40.0;
const PILL: f32 = 32.0;
/// The room inside the track round its pills, and between two pills.
const TRACK_PAD: f32 = 4.0;
const PILL_GAP: f32 = 3.0;
/// The gap between the strip and the card under it.
const STRIP_GAP: f32 = 14.0;
/// The room inside the card round the page.
pub const CARD_PAD: f32 = 16.0;
/// The widest a page's name is drawn before it is cut short.
const LONGEST_NAME: f32 = 150.0;
/// The `New query` chip.
const NEW_QUERY: f32 = 32.0;

/// Draw the workspace.
pub fn show(
    explorer: &mut DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let strip = Rect::from_min_size(area.min, Vec2::new(area.width(), STRIP * scale));
    acts.extend(pages(explorer, ui, look, strip));
    let body =
        Rect::from_min_max(Pos2::new(area.left(), strip.bottom() + STRIP_GAP * scale), area.max);
    if body.height() < 40.0 * scale {
        return acts;
    }
    kit::card(ui, scale, body, kit::CARD_RADIUS * scale);
    let inside = body.shrink(CARD_PAD * scale);
    let Some(page) = explorer.pages.get(explorer.current) else {
        acts.extend(nothing_open(explorer, ui, look, inside));
        return acts;
    };
    let id = page.id;
    match &page.sheet {
        Sheet::Console(_) => acts.extend(console::show(explorer, ui, look, inside, id)),
        Sheet::Grid(_) => acts.extend(grid::show(explorer, ui, look, inside, id)),
    }
    acts
}

/// One page's pill, measured before anything is drawn.
struct Pill {
    id: u64,
    title: String,
    icon: Icon,
    width: f32,
}

/// The mark a page wears: lines of SQL for a console, the grid for a table's rows.
fn mark_for(sheet: &Sheet) -> Icon {
    match sheet {
        Sheet::Console(_) => Icon::Lines,
        Sheet::Grid(_) => Icon::Board,
    }
}

/// The strip of pages and the `New query` chip after it.
///
/// **What gives way when the strip is narrow**: each name is cut shorter, down to a few letters, and a page
/// that still does not fit is left off the strip. The page showing is always drawn, because a strip that hid
/// the page under it would leave nothing saying what it is; `pages` on the command line lists every one.
fn pages(explorer: &DatabaseExplorer, ui: &mut egui::Ui, look: &Look<'_>, strip: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let style = Style::sans(12.0).medium();
    let room = strip.width() - (NEW_QUERY + 10.0) * scale - TRACK_PAD * 2.0 * scale;
    kit::layer(ui, scale, "database-pages", strip, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        // Each pill is a mark, the name and a cross: `padding: 0 8px 0 12px; gap: 7px`.
        let furniture = (12.0 + 13.0 + 7.0 + 8.0 + 16.0 + 8.0) * scale;
        let mut longest = LONGEST_NAME * scale;
        let measure = |rux: &rux::Rux<'_>, longest: f32| -> Vec<Pill> {
            explorer
                .pages
                .iter()
                .map(|page| {
                    let title = page.title();
                    let words = rux.measure(rux.zs(style), &title).x.min(longest);
                    Pill {
                        id: page.id,
                        icon: mark_for(&page.sheet),
                        width: furniture + words,
                        title,
                    }
                })
                .collect()
        };
        let mut pills = measure(rux, longest);
        let total = |pills: &[Pill]| -> f32 {
            pills.iter().map(|pill| pill.width).sum::<f32>()
                + PILL_GAP * scale * pills.len().saturating_sub(1) as f32
        };
        while total(&pills) > room && longest > 40.0 * scale {
            longest -= 10.0 * scale;
            pills = measure(rux, longest);
        }
        // Still too wide: keep the page showing and as many of the others as fit beside it.
        let showing = explorer.pages.get(explorer.current).map(|page| page.id);
        while total(&pills) > room && pills.len() > 1 {
            match pills.iter().rposition(|pill| Some(pill.id) != showing) {
                Some(at) => {
                    pills.remove(at);
                }
                None => break,
            }
        }
        let mut left = strip.left();
        if !pills.is_empty() {
            let track = Rect::from_min_size(
                Pos2::new(strip.left(), strip.center().y - STRIP * scale / 2.0),
                Vec2::new(total(&pills) + TRACK_PAD * 2.0 * scale, STRIP * scale),
            );
            rux.chrome.surface(track, track.height() / 2.0, chat.well, chat.carved_sm());
            let mut x = track.left() + TRACK_PAD * scale;
            for pill in &pills {
                let rect = Rect::from_min_size(
                    Pos2::new(x, track.center().y - PILL * scale / 2.0),
                    Vec2::new(pill.width, PILL * scale),
                );
                x = rect.right() + PILL_GAP * scale;
                let on = Some(pill.id) == showing;
                let response = rux.ui.interact(
                    rect,
                    rux.ui.id().with(("database-page", pill.id)),
                    Sense::click(),
                );
                let hovered = response.hovered();
                if hovered {
                    rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if on {
                    rux.chrome.surface(
                        rect,
                        rect.height() / 2.0,
                        Fill::gradient(chat.chip, rect),
                        chat.raised_chip(),
                    );
                }
                let ink = match (on, hovered) {
                    (true, _) => theme.ink.i900,
                    (false, true) => chat.sky,
                    (false, false) => theme.ink.i500,
                };
                rux.mark(
                    Mark::new(pill.icon, rux.z(13.0)),
                    Pos2::new(rect.left() + (12.0 + 6.5) * scale, rect.center().y),
                    ink,
                );
                let words_left = rect.left() + (12.0 + 13.0 + 7.0) * scale;
                let words_room = rect.right() - (8.0 + 16.0 + 8.0) * scale - words_left;
                let galley = rux.elided(rux.zs(style), &pill.title, ink, words_room);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(words_left, rect.center().y),
                    galley,
                    rux.zs(style),
                    ink,
                );
                let title = pill.title.clone();
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, title.clone())
                });
                if response.clicked() {
                    acts.push(Act::ShowPage(pill.id));
                }
                // The cross is added after the pill, so it takes the press over the pill under it.
                let cross = Rect::from_center_size(
                    Pos2::new(rect.right() - (8.0 + 8.0) * scale, rect.center().y),
                    Vec2::splat(18.0 * scale),
                );
                let close = rux.ui.interact(
                    cross,
                    rux.ui.id().with(("database-page-close", pill.id)),
                    Sense::click(),
                );
                if close.hovered() {
                    rux.wash(cross, cross.height() / 2.0, theme.ink.i900.gamma_multiply(0.08));
                    rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let cross_ink = match close.hovered() {
                    true => theme.ink.i900,
                    false => theme.ink.i400,
                };
                rux.mark(Mark::new(Icon::X, rux.z(9.0)).stroke(1.8), cross.center(), cross_ink);
                // Prefixed for `no_two_controls_share_a_name`'s reason -- see `agent_tasks::mod`.
                let named = format!("Close source {}", pill.title);
                close.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone())
                });
                if close.clicked() {
                    acts.push(Act::ClosePage(pill.id));
                }
            }
            left = track.right() + 10.0 * scale;
        }
        let new = Rect::from_center_size(
            Pos2::new(left + NEW_QUERY * scale / 2.0, strip.center().y),
            Vec2::splat(NEW_QUERY * scale),
        );
        if kit::chip_button(rux, new, Icon::Plus, NEW_QUERY, "New query") {
            acts.push(Act::NewQuery);
        }
    });
    acts
}

/// What the card shows before anything has been opened.
///
/// Not an empty card: the thing somebody can do from here is drawn as the action's pill, which is what makes
/// an empty state a place to start rather than a place to leave.
fn nothing_open(
    explorer: &DatabaseExplorer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let source = explorer.source_for_a_new_query();
    let (heading, said, button) = match &source {
        None => (
            "No data sources yet",
            "Add a PostgreSQL server, a SQLite file or an Inillucent database, and its tables appear on the right.".to_owned(),
            "New data source",
        ),
        Some(source) => (
            "Nothing open",
            format!("Open a query on `{source}`, or double click a table on the right to read its rows."),
            "Open a query console",
        ),
    };
    let middle = area.center();
    kit::layer(ui, scale, "database-nothing-open", area, |rux| {
        let theme = rux.theme();
        kit::badge(rux, Pos2::new(middle.x, middle.y - 58.0 * scale), Icon::Database);
        let style = rux.zs(Style::sans(15.0).semibold());
        let galley = rux.elided(style, heading, theme.ink.i900, area.width());
        let at = Rect::from_center_size(
            Pos2::new(middle.x, middle.y - 22.0 * scale),
            Vec2::new(area.width(), 20.0 * scale),
        );
        rux::text::draw_centred(rux.painter(), at, galley, theme.ink.i900);
        let width = (area.width() - 32.0 * scale).clamp(80.0, 340.0 * scale);
        let galley = rux::text::wrapped(
            rux.painter(),
            rux.zs(Style::sans(12.0).leading(1.5)),
            &said,
            theme.ink.i400,
            width,
        );
        let top = Pos2::new(middle.x - galley.size().x / 2.0, middle.y - 6.0 * scale);
        let below = top.y + galley.size().y;
        rux.painter().galley(top, galley, theme.ink.i400);
        let pill = rux::components::PillButton::new(button).primary().mark(Icon::Plus);
        let size = pill.measure(rux);
        let rect =
            Rect::from_center_size(Pos2::new(middle.x, below + 18.0 * scale + size.y / 2.0), size);
        if pill.show(rux, rect).clicked() {
            acts.push(match &source {
                None => Act::NewSource,
                Some(source) => Act::OpenConsole(source.clone()),
            });
        }
    });
    acts
}
