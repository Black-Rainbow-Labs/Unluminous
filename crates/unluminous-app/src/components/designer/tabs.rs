//! The Designer pane's tab strip: one pill a tab in a carved track, the one showing raised out of it, and a
//! round `New tab` chip after the last of them.
//!
//! `task-2260` gave the pane tabs of its own, so a design no longer opens in the editing area beside every other
//! open file. The strip is drawn the way the Database pane's page strip is (`components::database::workspace`),
//! which is the way the Agent-Tasks board draws its views: the language the plugin panes already share. A design's
//! pill carries the page mark and the file's name, and the blank tab is `New tab` with a plus.
//!
//! **What gives way when the strip is narrow**: each name is cut shorter, down to a few letters, and a tab that
//! still does not fit is left off the strip. The tab showing is always drawn, and `plugins run designer tabs`
//! lists every one.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Fill, Style};

use super::{kit, Act};
use crate::services::designer::Designer;
use crate::services::plugin_ui::{Look, Request};

/// How tall the strip is: a 40 point track holding 32 point pills.
pub const STRIP: f32 = 40.0;
const PILL: f32 = 32.0;
/// The room inside the track round its pills, and between two pills.
const TRACK_PAD: f32 = 4.0;
const PILL_GAP: f32 = 3.0;
/// The room round the strip, and the gap under it.
const MARGIN: f32 = 14.0;
const TOP: f32 = 10.0;
const STRIP_GAP: f32 = 4.0;
/// The widest a tab's name is drawn before it is cut short.
const LONGEST_NAME: f32 = 170.0;
/// The `New tab` chip.
const NEW_TAB: f32 = 32.0;

/// One tab's pill, measured before anything is drawn.
struct Pill {
    index: usize,
    title: String,
    icon: Icon,
    width: f32,
}

/// Draw the strip at the top of `body` and apply what was pressed. Answers the room under the strip, which is
/// where the showing tab is drawn, and what the presses asked the window for.
///
/// @param designer - the provider
/// @param ui - where to draw
/// @param look - the window's look, zoomed by the pane's own zoom
/// @param body - the pane under its header
pub fn show(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    body: Rect,
) -> (Rect, Vec<Request>) {
    let scale = look.scale();
    let strip = Rect::from_min_size(
        Pos2::new(body.left() + MARGIN * scale, body.top() + TOP * scale),
        Vec2::new((body.width() - 2.0 * MARGIN * scale).max(0.0), STRIP * scale),
    );
    let content = Rect::from_min_max(
        Pos2::new(body.left(), (strip.bottom() + STRIP_GAP * scale).min(body.bottom())),
        body.max,
    );
    if strip.width() < 60.0 || body.height() < STRIP * scale {
        return (content, Vec::new());
    }
    let acts = the_strip(designer, ui, scale, strip);
    (content, super::apply(designer, acts))
}

/// The track of pills and the `New tab` chip after it.
fn the_strip(designer: &Designer, ui: &mut egui::Ui, scale: f32, strip: Rect) -> Vec<Act> {
    let mut acts = Vec::new();
    let style = Style::sans(12.0).medium();
    let room = strip.width() - (NEW_TAB + 10.0) * scale - TRACK_PAD * 2.0 * scale;
    let showing = designer.active_tab();
    let tabs: Vec<(usize, String, Icon)> = designer
        .tabs()
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let icon = match tab {
                Some(_) => Icon::Document,
                None => Icon::Plus,
            };
            (index, designer.tab_title(tab), icon)
        })
        .collect();
    kit::layer(ui, scale, "designer-tabs", strip, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        // Each pill is a mark, the name and a cross: `padding: 0 8px 0 12px; gap: 7px`.
        let furniture = (12.0 + 13.0 + 7.0 + 8.0 + 16.0 + 8.0) * scale;
        let measure = |rux: &rux::Rux<'_>, longest: f32| -> Vec<Pill> {
            tabs.iter()
                .map(|(index, title, icon)| {
                    let words = rux.measure(rux.zs(style), title).x.min(longest);
                    Pill { index: *index, title: title.clone(), icon: *icon, width: furniture + words }
                })
                .collect()
        };
        let mut longest = LONGEST_NAME * scale;
        let mut pills = measure(rux, longest);
        let total = |pills: &[Pill]| -> f32 {
            pills.iter().map(|pill| pill.width).sum::<f32>()
                + PILL_GAP * scale * pills.len().saturating_sub(1) as f32
        };
        while total(&pills) > room && longest > 40.0 * scale {
            longest -= 10.0 * scale;
            pills = measure(rux, longest);
        }
        while total(&pills) > room && pills.len() > 1 {
            match pills.iter().rposition(|pill| pill.index != showing) {
                Some(at) => {
                    pills.remove(at);
                }
                None => break,
            }
        }
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
            let on = pill.index == showing;
            let response =
                rux.ui.interact(rect, rux.ui.id().with(("designer-tab", pill.index)), Sense::click());
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
            // Named `Tab: …` rather than by the design alone, because the blank tab's list names a row for every
            // design and two controls must not share a name.
            let named = format!("Tab: {}", pill.title);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, named.clone())
            });
            if response.clicked() {
                acts.push(Act::ShowTab(pill.index));
            }
            // The cross is added after the pill, so it takes the press over the pill under it.
            let cross = Rect::from_center_size(
                Pos2::new(rect.right() - (8.0 + 8.0) * scale, rect.center().y),
                Vec2::splat(18.0 * scale),
            );
            let close = rux.ui.interact(
                cross,
                rux.ui.id().with(("designer-tab-close", pill.index)),
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
            let named = format!("Close tab: {}", pill.title);
            close.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone())
            });
            if close.clicked() {
                acts.push(Act::CloseTab(pill.index));
            }
        }
        let new = Rect::from_center_size(
            Pos2::new(track.right() + 10.0 * scale + NEW_TAB * scale / 2.0, strip.center().y),
            Vec2::splat(NEW_TAB * scale),
        );
        if kit::chip_button(rux, new, Icon::Plus, NEW_TAB, "New tab") {
            acts.push(Act::NewTab);
        }
    });
    acts
}
