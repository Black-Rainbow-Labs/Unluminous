//! Drawing the Database pane with `rux`, in the language the Settings window, the Agent-Tasks board and the
//! Agent-Chat pane already use.
//!
//! `task-2250`. A group of things is a raised card on the pane's ground, a value that is typed or read is a
//! carved well, a button is a chip raised out of whatever it sits on, and the button that does the thing the
//! page is for is a pill on the action's gradient. The values are `rux::theme::Chat`'s, worked out from the
//! active theme, so the pane follows Unluminous Dark, Unluminous Light and every plugin theme.
//!
//! ## One state per window, kept per thread
//!
//! The pane zooms by a multiplier (`Look::scale`), which `rux` is told through `RuxState::set_zoom`, so the
//! plugin keeps a state of its own rather than sharing the furniture's: the title bar must not grow when the
//! Database pane is zoomed. It is held per thread and per egui context for the reason
//! `components::chrome_kit` gives: the screenshot tests run many windows on many threads.
//!
//! ## A surface is a layer, and a layer is painted where it is opened
//!
//! A `rux` layer reserves its slot in the painter's list when it is opened, so a layer opened after another
//! is painted over it, and everything `egui` paints after a layer is opened is painted over that layer. So
//! [`card`] and [`well`] each open a small layer of their own: a well drawn inside a card is opened after the
//! card and is painted on it, and the words drawn after both are painted on top of both. That is what lets
//! the grid and the tree keep drawing their words and rows with `egui` exactly as they did.

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

use egui::{Pos2, Rect, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Fill, Rux, Style};

thread_local! {
    static STATE: RefCell<Option<(u64, Rc<rux::RuxState>)>> = const { RefCell::new(None) };
}

/// How far a card's shadow reaches past it, which a layer has to be made large enough to hold.
pub const REACH: f32 = 30.0;
/// How round a card is: the Settings design's `border-radius: 22px` for a card inside a pane.
pub const CARD_RADIUS: f32 = 22.0;
/// How round a well is, unless it is a pill.
pub const WELL_RADIUS: f32 = 14.0;

/// The pane's `rux` state for the context drawing now, at the pane's zoom.
pub fn state(ctx: &egui::Context, zoom: f32) -> Rc<rux::RuxState> {
    let owner = crate::components::chrome_kit::owner(ctx);
    let state = STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_ref() {
            Some((held, state)) if *held == owner => state.clone(),
            _ => {
                let state = Rc::new(crate::theme::rux_state(crate::theme::rux_theme()));
                *slot = Some((owner, state.clone()));
                state
            }
        }
    });
    crate::theme::in_step(&state);
    state.set_still(true);
    state.set_zoom(zoom);
    state
}

/// Give back the canvases of the pieces not drawn this frame, once the pane has been drawn.
pub fn end_frame(ctx: &egui::Context) {
    let owner = crate::components::chrome_kit::owner(ctx);
    STATE.with(|slot| {
        if let Some((held, state)) = slot.borrow().as_ref() {
            if *held == owner {
                state.end_frame();
            }
        }
    });
}

/// Draw with `rux` into `rect`, in a layer that reaches [`REACH`] points past it for the shadows.
///
/// `zoom` is the pane's own, which every size `rux` measures is multiplied by.
pub fn layer<R>(
    ui: &mut egui::Ui,
    zoom: f32,
    salt: impl Hash + std::fmt::Debug,
    rect: Rect,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let state = state(ui.ctx(), zoom);
    let id = ui.id().with(("database-kit", salt));
    rux::layer(ui, &state, id, rect.expand(REACH * zoom), add)
}

/// A raised card: the chat card's gradient with the design's raised shadow.
pub fn card_surface(rux: &mut Rux<'_>, rect: Rect, radius: f32) {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, Fill::gradient(chat.card, rect), chat.raised());
}

/// A well carved into whatever it sits on.
pub fn well_surface(rux: &mut Rux<'_>, rect: Rect, radius: f32) {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, chat.well, chat.carved_sm());
}

/// A raised card in a layer of its own, for a part of the pane that draws its insides with `egui`.
pub fn card(ui: &mut egui::Ui, zoom: f32, rect: Rect, radius: f32) {
    let salt = (rect.min.x.to_bits(), rect.min.y.to_bits(), "card");
    layer(ui, zoom, salt, rect, |rux| card_surface(rux, rect, radius));
}

/// A carved well in a layer of its own.
pub fn well(ui: &mut egui::Ui, zoom: f32, rect: Rect, radius: f32) {
    let salt = (rect.min.x.to_bits(), rect.min.y.to_bits(), "well");
    layer(ui, zoom, salt, rect, |rux| well_surface(rux, rect, radius));
}

/// A round chip with a mark in it, named. Answers whether it was pressed.
///
/// `diameter` is in the design's points; the layer's zoom makes it the pane's.
pub fn chip_button(rux: &mut Rux<'_>, rect: Rect, icon: Icon, diameter: f32, name: &str) -> bool {
    rux::components::ChipButton::new(icon, diameter, name)
        .mark(12.0, Some(1.8))
        .show(rux, rect)
        .clicked()
}

/// A round chip with one of Unluminous's own drawn marks in it, for a meaning `rux` has no mark for: undo,
/// the bin, a key. Answers whether it was pressed.
pub fn chip_drawn(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    drawing: fn(&egui::Painter, Pos2, egui::Color32),
) -> bool {
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    let rect = Rect::from_center_size(rect.center(), Vec2::splat(rect.width().min(rect.height())));
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("database-chip", name)), egui::Sense::click());
    let held = response.is_pointer_button_down_on();
    match held {
        true => rux.chrome.surface(rect, rect.width() / 2.0, chat.well, chat.carved_sm()),
        false => rux.chrome.surface(
            rect,
            rect.width() / 2.0,
            Fill::gradient(chat.chip, rect),
            chat.raised_chip(),
        ),
    }
    let tint = match (response.hovered(), held) {
        (_, true) => ink.i900,
        (true, false) => chat.sky,
        (false, false) => ink.i500,
    };
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    drawing(rux.painter(), rect.center(), tint);
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    response.clicked()
}

/// The words of a heading, left aligned and centred on `middle`. Answers how wide they came out.
pub fn heading(rux: &mut Rux<'_>, at: Pos2, words: &str, size: f32, room: f32) -> f32 {
    let ink = rux.theme().ink.i900;
    let style = rux.zs(Style::sans(size).semibold().tracking(-0.01));
    let galley = rux.elided(style, words, ink, room.max(0.0));
    let width = galley.size().x;
    rux::text::draw_left_capitals(rux.painter(), at, galley, style, ink);
    width
}

/// Quieter words, left aligned and centred on `at.y`. Answers how wide they came out.
pub fn quiet(rux: &mut Rux<'_>, at: Pos2, words: &str, size: f32, room: f32) -> f32 {
    let ink = rux.theme().ink.i400;
    let style = rux.zs(Style::sans(size));
    let galley = rux.elided(style, words, ink, room.max(0.0));
    let width = galley.size().x;
    rux::text::draw_left_capitals(rux.painter(), at, galley, style, ink);
    width
}

/// A small mark in a carved disc, which is what a section of the pane wears beside its heading.
pub fn badge(rux: &mut Rux<'_>, centre: Pos2, icon: Icon) {
    let side = rux.z(30.0);
    let disc = Rect::from_center_size(centre, Vec2::splat(side));
    rux::components::chip_disc(rux, disc);
    let ink = rux.theme().chat.sky;
    rux.mark(Mark::new(icon, rux.z(13.0)), centre, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_is_kept_for_one_context_and_follows_the_zoom_it_is_asked_for() {
        let ctx = egui::Context::default();
        let first = state(&ctx, 1.0);
        let again = state(&ctx, 1.5);
        assert!(Rc::ptr_eq(&first, &again), "one state per context");
        assert_eq!(again.zoom(), 1.5, "the pane's zoom is what rux measures at");
        let other = egui::Context::default();
        let elsewhere = state(&other, 1.0);
        assert!(!Rc::ptr_eq(&first, &elsewhere), "a second window gets a state of its own");
    }
}
