//! The menu bar drawn inside the window, which is what Windows uses.
//!
//! On macOS the menus belong in the bar along the top of the screen instead, so this is not drawn there;
//! `services::native_menu` builds that one. Both are built from the same [`crate::app::actions::menus`],
//! so `File` holds the same entries with the same shortcuts either way. The application's own menu is the
//! exception: macOS draws its pill in the window as well, after the traffic lights, through
//! [`application_menu`], so the mark opens the same list on both platforms (`task-2282`).
//!
//! `Unluminous` is first and sits at the very left of the bar, so it reads `Unluminous  File  Edit  View`, which is
//! what `tasks/improvements.md` asks for. When the menus are in the window the three window buttons move
//! to the right hand end of the bar, which is where Windows puts them, because the left is taken.
//!
//! **Each menu is a pill** (`task-2241`, the Editing code design): its word in the quieter ink with no surface,
//! and while its list is open the pill is carved into the bar with the word in the strong ink.
//!
//! **The application's own menu is the mark rather than its name** (`task-2258`): a round pill holding the
//! Unluminous mark, drawn by `components::logo` in the version for the theme the window is in. It is still the
//! `Unluminous` menu, and it is still named `Unluminous`, so a test, a screen reader and `action list` find it
//! as they always did.

use egui::{Pos2, Rect, Vec2};

use crate::app::actions::{Action, Entry, Menu};
use crate::components::controls;
use crate::theme::crisp::CrispPainter;

/// Between the bar's left edge and the first word, and between two pills.
const LEFT: f32 = 8.0;
const GAP: f32 = 2.0;
/// `height: 30px; padding: 0 11px; font-size: 12.5px`.
const PILL: f32 = 30.0;
const PAD: f32 = 11.0;
/// How tall the mark is inside the application's round pill.
pub const MARK: f32 = 20.0;

/// Draw the bar into the left of `area`, and answer what was chosen and where the last pill ended.
///
pub fn show(ui: &mut egui::Ui, area: Rect, menus: &[Menu]) -> (Option<Action>, f32) {
    let mut chosen = None;
    let mut pen = area.left() + LEFT;
    let font = egui::FontId::proportional(12.5);
    let ink = crate::theme::rux_theme().ink;
    // Laid out first, so the decoration of the open pill can be recorded in one layer for the whole bar.
    let mut pills = Vec::with_capacity(menus.len());
    for menu in menus {
        let app = menu.name == "Unluminous";
        let width = match app {
            // Round, holding the mark.
            true => PILL,
            false => {
                ui.painter().layout_no_wrap(menu.name.clone(), font.clone(), ink.i900).size().x
                    + PAD * 2.0
            }
        };
        let rect = Rect::from_min_size(
            Pos2::new(pen, area.center().y - PILL / 2.0),
            Vec2::new(width, PILL),
        );
        pills.push((menu, rect, app));
        pen = rect.right() + GAP;
    }
    let reach =
        Rect::from_min_max(Pos2::new(area.left(), area.top()), Pos2::new(pen, area.bottom()));
    crate::components::chrome_kit::layer(ui, "menu-bar", reach, 4.0, |rux| {
        for (menu, rect, app) in &pills {
            if let Some(action) = pill(rux, menu, *rect, *app) {
                chosen = Some(action);
            }
        }
    });
    (chosen, pen)
}

/// The application's own menu on its own: the round pill holding the mark, which drops the `Unluminous`
/// list down when pressed (`task-2282`).
///
/// macOS draws this in the window's bar after the traffic lights, beside the same menu in the bar along
/// the top of the screen, so the mark does there what it does on Windows. It is the pill `show` draws
/// first, so the two platforms cannot drift apart.
///
/// @param ui - the title bar
/// @param rect - where the pill goes
/// @param menu - the `Unluminous` menu
pub fn application_menu(ui: &mut egui::Ui, rect: Rect, menu: &Menu) -> Option<Action> {
    crate::components::chrome_kit::layer(ui, "application-menu", rect, 4.0, |rux| {
        pill(rux, menu, rect, true)
    })
}

/// One pill of the bar: the menu's word, or the mark for the application's own menu, carved into the bar
/// while its list is open, and the list itself. Answers what was chosen from the list.
///
/// @param rux - the bar's layer
/// @param menu - the menu the pill opens
/// @param rect - where the pill goes
/// @param app - whether this is the application's own menu, which is the mark rather than a word
fn pill(rux: &mut rux::Rux<'_>, menu: &Menu, rect: Rect, app: bool) -> Option<Action> {
    let font = egui::FontId::proportional(12.5);
    let ink = crate::theme::rux_theme().ink;
    let chat = crate::components::chrome_kit::chat();
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("bar-button", &menu.name)), egui::Sense::click());
    let open = egui::Popup::is_id_open(rux.ui.ctx(), egui::Popup::default_response_id(&response));
    if open {
        rux.chrome.surface(rect, PILL / 2.0, chat.well, chat.carved_sm());
    } else if response.hovered() {
        rux.wash(rect, PILL / 2.0, rux.theme().interaction.hover);
    }
    if app {
        crate::components::logo::paint(rux.ui, rect, MARK);
    } else {
        let tint = match (open, response.hovered()) {
            (true, _) => ink.i900,
            (false, true) => chat.sky,
            (false, false) => ink.i500,
        };
        let painter = rux.ui.painter();
        let label = painter.crisp_layout_no_wrap(menu.name.clone(), font.clone(), tint);
        let top = crate::theme::crisp::top_centring_capitals(painter, &font, rect.center().y);
        painter.crisp_galley(Pos2::new(rect.center().x - label.size().x / 2.0, top), label, tint);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, menu.name.clone())
    });
    popup(rux.ui, &response, &menu.entries)
}

/// The list that drops down from one of the words in the bar.
fn popup(ui: &mut egui::Ui, response: &egui::Response, entries: &[Entry]) -> Option<Action> {
    egui::Popup::from_toggle_button_response(response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .frame(controls::menu_frame(ui))
        // Wide enough that the longest entry and its shortcut do not meet in the middle. `Open Folder in New
        // Window` at `Cmd+Option+O` is the longest pair, and at 260 points they overlapped, which a
        // screenshot showed.
        .width(340.0)
        .show(|ui| controls::menu_rows(ui, entries, 0.0))
        .and_then(|inner| inner.inner)
}
