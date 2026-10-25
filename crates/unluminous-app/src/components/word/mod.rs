//! A Word document's tab: the format bar, the pages, the drill down panel, the navigation pane, and the
//! menus that open over the pages (`/`, `Ctrl+/`, the selection bubble, find).
//!
//! `task-2341`. There is no ribbon. The bar holds what most edits need and two buttons, `Aa` and `+`, that
//! open the panel; everything else is a level of the panel, a name in the `Ctrl+/` search, or a `/` at the
//! start of an empty paragraph.

pub mod banner;
pub mod context_menu;
pub mod format_bar;
pub mod keys;
pub mod navigation;
pub mod objects;
pub mod overlays;
pub mod page_view;
pub mod panel;
pub mod panel_setup;
pub mod outline;
pub mod settings_page;
pub mod spelling;
pub mod views;

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
    doc.view.dark = config.dark_paper;
    doc.drawn += 1;
    let mut requests = Vec::new();
    let area = ui.max_rect();
    let scale = look.scale();
    let bar = Rect::from_min_size(area.min, Vec2::new(area.width(), BAR * scale));
    let panel_width =
        if doc.view.panel.is_empty() { 0.0 } else { (PANEL * scale).min(area.width() * 0.5) };
    let navigation_width =
        if doc.view.navigation { (NAVIGATION * scale).min(area.width() * 0.3) } else { 0.0 };
    let status_height = views::STATUS * scale;
    let status = Rect::from_min_max(egui::pos2(area.left(), area.bottom() - status_height), area.max);
    // A document opened read only, or with a recovered copy to offer, has a strip under the bar.
    let banner_height = if banner::wanted(doc) { banner::HEIGHT * scale } else { 0.0 };
    let banner = Rect::from_min_size(egui::pos2(area.left(), bar.bottom()), Vec2::new(area.width(), banner_height));
    let body = Rect::from_min_max(egui::pos2(area.left(), banner.bottom()), egui::pos2(area.right(), status.top()));
    let ruler_height =
        if doc.view.ruler && doc.view.mode == crate::services::word::Mode::Print { views::RULER * scale } else { 0.0 };
    let ruler = Rect::from_min_max(
        egui::pos2(body.left() + navigation_width, body.top()),
        egui::pos2(body.right() - panel_width, body.top() + ruler_height),
    );
    let pages = Rect::from_min_max(
        egui::pos2(body.left() + navigation_width, ruler.bottom()),
        egui::pos2(body.right() - panel_width, body.bottom()),
    );
    // Read Mode lays the text out on screens the size of the pane.
    if doc.view.mode == crate::services::word::Mode::Read {
        let zoom = doc.view.zoom.max(0.1);
        doc.view.read_size = Some(((pages.width() - 48.0) / zoom, (pages.height() - 24.0) / zoom));
    }
    if std::mem::take(&mut doc.view.new_window) {
        let n = doc.windows.len() + 2;
        requests.push(Request::OpenKeyedTab { key: format!("{}#{n}", doc.key), label: format!("{}:{n}", doc.label()) });
    }
    // Keys first, against the layout the person is looking at, then lay out what they changed.
    // A field of the panel or the find bar takes typing for itself; the window keeps egui's own focus on a
    // widget of its own at other times, so the question is whether a text box has it.
    let typing_into_a_field = crate::app::text_box_has_the_keyboard(ui.ctx());
    doc.keyboard = (look.has_the_keyboard, typing_into_a_field);
    if look.has_the_keyboard && !typing_into_a_field {
        match doc.view.mode {
            // Read Mode takes no typing: the arrows and the page keys turn the screens, Escape leaves.
            crate::services::word::Mode::Read => read_keys(ui, doc),
            _ => keys::handle(ui, doc, &mut requests),
        }
    }
    // An edit the document's protection refused says why.
    if let Some(refused) = doc.session.refused.take() {
        requests.push(Request::Notice { text: refused, kind: crate::components::toast::Kind::Problem });
    }
    doc.lay_out(look.renderer);
    match doc.view.mode {
        crate::services::word::Mode::Outline => outline::show(ui, look, doc, pages, &mut requests),
        _ => page_view::show(ui, look, doc, pages, &mut requests),
    }
    if ruler_height > 0.0 {
        views::ruler(ui, look, doc, ruler, pages);
    }
    views::status(ui, look, doc, status, &mut requests);
    if navigation_width > 0.0 {
        let rect = Rect::from_min_max(body.min, egui::pos2(pages.left(), body.bottom()));
        navigation::show(ui, look, doc, rect, &mut requests);
    }
    if panel_width > 0.0 {
        let rect = Rect::from_min_max(egui::pos2(pages.right(), body.top()), body.max);
        panel::show(ui, look, doc, rect, &mut requests);
    }
    if std::mem::take(&mut doc.view.menu_at_caret) {
        context_menu::open_at_caret(doc, pages);
    }
    if doc.view.context_menu.is_some() {
        context_menu::show(ui, look, doc, pages, &mut requests);
    } else {
        overlays::show(ui, look, doc, pages, &mut requests);
    }
    format_bar::show(ui, look, doc, bar, &mut requests);
    if banner_height > 0.0 {
        banner::show(ui, look, doc, banner, &mut requests);
    }
    requests
}

/// The keys Read Mode answers: a screen down or up, and Escape back to Print Layout.
fn read_keys(ui: &egui::Ui, doc: &mut Doc) {
    let step = doc.view.viewport_height * 0.9;
    for event in ui.input(|i| i.events.clone()) {
        if let egui::Event::Key { key, pressed: true, .. } = event {
            match key {
                egui::Key::PageDown | egui::Key::ArrowDown | egui::Key::ArrowRight | egui::Key::Space => doc.view.scroll += step,
                egui::Key::PageUp | egui::Key::ArrowUp | egui::Key::ArrowLeft => doc.view.scroll = (doc.view.scroll - step).max(0.0),
                egui::Key::Escape => {
                    doc.view.mode = crate::services::word::Mode::Print;
                    doc.laid = u64::MAX;
                    doc.view.reveal = true;
                }
                _ => {}
            }
        }
    }
}

/// Run an action from the shared list by its id and say what it said in the status bar.
pub fn run_action(doc: &mut Doc, id: &str, requests: &mut Vec<Request>) {
    let Some(action) = crate::services::word::actions::find(id) else { return };
    // F4 repeats the last command that formats or inserts, as Word's Repeat does.
    if crate::services::word::actions::repeatable(action) {
        doc.view.last_action = Some(action.id);
    }
    match (action.run)(doc) {
        Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
        Ok(_) => {}
        Err(problem) => requests
            .push(Request::Notice { text: problem, kind: crate::components::toast::Kind::Problem }),
    }
    doc.view.reveal = true;
}
