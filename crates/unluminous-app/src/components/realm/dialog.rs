//! The parts the Realm's two dialogs are built from, in the Settings design's look.
//!
//! `task-2250`. The New Data Source dialog set the shape (`components::database::modal`): a surface thirty
//! points round on the pane's own ground with one long soft shadow, a heading and a round close chip at the
//! top, the work in the middle, and the buttons at the foot, the one that does the thing on the action's
//! gradient. The realm manager and Add a node are both a search box over a list, so they share the search
//! well, the list well and the rows as well.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::components::ChipButton;
use rux::icon::Icon;
use rux::{Rux, Style};

use crate::components::controls::TextBoxes;
use crate::components::{controls, modal, settings_kit as kit};

/// The dialog's padding and corner.
pub const PAD_X: f32 = 24.0;
pub const PAD_Y: f32 = 22.0;
pub const RADIUS: f32 = 28.0;
/// The close chip, which is also how tall the heading's row is.
pub const CLOSE: f32 = 34.0;
/// The gap between the parts of the dialog.
pub const GAP: f32 = 16.0;
/// The search well.
pub const SEARCH: f32 = 40.0;
/// The buttons at the foot.
pub const BUTTON: f32 = 38.0;

/// The surface: the pane's ground, thirty points round, with the shadow a layer over the window casts.
pub fn surface(ui: &mut egui::Ui, salt: &str, area: Rect) {
    kit::layer_reaching(ui, (salt, "surface"), area, modal::SHADOW_REACH, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, RADIUS, chat.ground, modal::shadow());
    });
}

/// The heading and the round close chip. Answers whether the chip was pressed.
pub fn heading(rux: &mut Rux<'_>, inner: Rect, words: &str) -> bool {
    let theme = rux.theme();
    let close = Rect::from_center_size(
        Pos2::new(inner.right() - CLOSE / 2.0, inner.top() + CLOSE / 2.0),
        Vec2::splat(CLOSE),
    );
    let pressed =
        ChipButton::new(Icon::X, CLOSE, "Close").mark(12.0, Some(1.8)).show(rux, close).clicked();
    let style = Style::sans(18.0).semibold().tracking(-0.015);
    let galley = rux.text(style, words, theme.ink.i900);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(inner.left(), close.center().y),
        galley,
        style,
        theme.ink.i900,
    );
    pressed
}

/// A search well with a text box in it, named. Answers the box's response.
///
/// The whole well hands the box the keyboard, which is `controls::field_takes_the_whole_rectangle_at`'s
/// promise.
pub fn search(
    rux: &mut Rux<'_>,
    well: Rect,
    name: &str,
    hint: &str,
    value: &mut String,
) -> egui::Response {
    let inside = rux::components::search_well(rux, well);
    let font = Style::sans(13.0).font();
    let id = rux.ui.id().with(("realm-dialog-search", name));
    let text = controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        well,
        inside.left() - well.left(),
        id,
        &format!("{name} field"),
        &font,
    );
    let text = Rect::from_min_max(text.min, Pos2::new(inside.right(), text.max.y));
    let ink = rux.theme().ink;
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = child.text_box(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(controls::placeholder(hint, &font, ink.i400))
            .font(font.clone())
            .frame(egui::Frame::NONE)
            .desired_width(text.width())
            .text_color(ink.i900),
    );
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, named.clone()));
    response
}

/// The action's pill at the right of the foot, and the buttons before it. Answers which was pressed, counted
/// from the left, with the pill last: the pill is the one Enter presses, which is `modal::footer`'s rule.
pub fn footer(
    rux: &mut Rux<'_>,
    inner: Rect,
    others: &[(&str, bool)],
    primary: (&str, bool),
) -> Option<usize> {
    let top = inner.bottom() - BUTTON;
    let pill = rux::components::PillButton::new(primary.0).primary().enabled(primary.1);
    let width = pill.measure(rux).x.max(96.0);
    let rect = Rect::from_min_size(Pos2::new(inner.right() - width, top), Vec2::new(width, BUTTON));
    let mut pressed = None;
    if pill.show(rux, rect).clicked() && primary.1 {
        pressed = Some(others.len());
    }
    let mut right = rect.left() - 10.0;
    for (index, (label, chip)) in others.iter().enumerate().rev() {
        let width = kit::button_width(rux, label, false);
        let button = Rect::from_min_size(
            Pos2::new(right - width, top + 2.0),
            Vec2::new(width, BUTTON - 4.0),
        );
        let kind = match chip {
            true => kit::Kind::Chip,
            false => kit::Kind::Ghost,
        };
        if kit::button(rux, button, label, kind) {
            pressed = Some(index);
        }
        right = button.left() - 8.0;
    }
    pressed
}

/// One row of a list: raised out of the list's well when it is the one picked out, washed under the pointer.
pub fn row(rux: &mut Rux<'_>, rect: Rect, name: &str, on: bool) -> egui::Response {
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("realm-dialog-row", name)), Sense::click());
    let chat = rux.theme().chat;
    if on {
        rux.chrome.surface(rect, 14.0, rux::Fill::gradient(chat.chip, rect), chat.raised_chip());
    } else if response.hovered() {
        rux.wash(rect, 14.0, rux.theme().ink.i900.gamma_multiply(0.04));
    }
    response
}
