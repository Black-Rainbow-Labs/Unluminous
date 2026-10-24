//! A Word document's tab: the format bar, the pages, the drill down panel, the navigation pane, and the
//! menus that open over the pages (`/`, `Ctrl+/`, the selection bubble, find).
//!
//! `task-2341`. There is no ribbon. The bar holds what most edits need and two buttons, `Aa` and `+`, that
//! open the panel; everything else is a level of the panel, a name in the `Ctrl+/` search, or a `/` at the
//! start of an empty paragraph.

pub mod format_bar;
pub mod keys;
pub mod navigation;
pub mod overlays;
pub mod page_view;
pub mod panel;
pub mod settings_page;
pub mod spelling;

use egui::{Rect, Vec2};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{Config, Doc};

/// How tall the format bar is, before the tab's zoom.
pub const BAR: f32 = 40.0;
/// How wide the panel is.
pub const PANEL: f32 = 320.0;
/// How wide the navigation pane is.
pub const NAVIGATION: f32 = 230.0;

/// Draw a document's tab and take its input.
pub fn show(doc: &mut Doc, config: &Config, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let _ = config;
    doc.drawn += 1;
    let mut requests = Vec::new();
    let area = ui.max_rect();
    let scale = look.scale();
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), BAR * scale));
    let panel_width =
        if doc.view.panel.is_empty() { 0.0 } else { (PANEL * scale).min(area.width() * 0.5) };
    let navigation_width =
        if doc.view.navigation { (NAVIGATION * scale).min(area.width() * 0.3) } else { 0.0 };
    let body = Rect::from_min_max(egui::pos2(area.left(), bar.bottom()), area.max);
    let pages = Rect::from_min_max(
        egui::pos2(body.left() + navigation_width, body.top()),
        egui::pos2(body.right() - panel_width, body.bottom()),
    );
    // Keys first, against the layout the person is looking at, then lay out what they changed.
    // A field of the panel or the find bar takes typing for itself; the window keeps egui's own focus on a
    // widget of its own at other times, so the question is whether a text box has it.
    let typing_into_a_field = crate::app::text_box_has_the_keyboard(ui.ctx());
    doc.keyboard = (look.has_the_keyboard, typing_into_a_field);
    if look.has_the_keyboard && !typing_into_a_field {
        keys::handle(ui, doc, &mut requests);
    }
    doc.lay_out(look.renderer);
    page_view::show(ui, look, doc, pages, &mut requests);
    if navigation_width > 0.0 {
        let rect = Rect::from_min_max(body.min, egui::pos2(pages.left(), body.bottom()));
        navigation::show(ui, look, doc, rect, &mut requests);
    }
    if panel_width > 0.0 {
        let rect = Rect::from_min_max(egui::pos2(pages.right(), body.top()), body.max);
        panel::show(ui, look, doc, rect, &mut requests);
    }
    overlays::show(ui, look, doc, pages, &mut requests);
    format_bar::show(ui, look, doc, bar, &mut requests);
    requests
}

/// Run an action from the shared list by its id and say what it said in the status bar.
pub fn run_action(doc: &mut Doc, id: &str, requests: &mut Vec<Request>) {
    let Some(action) = crate::services::word::actions::find(id) else { return };
    match (action.run)(doc) {
        Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
        Ok(_) => {}
        Err(problem) => requests
            .push(Request::Notice { text: problem, kind: crate::components::toast::Kind::Problem }),
    }
    doc.view.reveal = true;
}
