//! Drawing the Designer with `rux`, in the language the Settings window, the Agent-Tasks board, the
//! Agent-Chat pane and the Database pane already use.
//!
//! `task-2254`. A group of things is a raised card on the ground, a value that is typed or read is a carved
//! well, a button is a chip raised out of whatever it sits on, and the button that does the thing the surface
//! is for is a pill on the action's gradient. The values are `rux::theme::Chat`'s, worked out from the active
//! theme, so the Designer follows Unluminous Dark, Unluminous Light and every plugin theme, and nothing here
//! names a colour.
//!
//! ## One state per window, kept per thread
//!
//! The pane zooms by a multiplier (`Look::scale`), which `rux` is told through `RuxState::set_zoom`, so the
//! plugin keeps a state of its own rather than sharing the furniture's: the title bar must not grow when the
//! Designer is zoomed. It is held per thread and per egui context for the reason `components::chrome_kit`
//! gives: the screenshot tests run many windows on many threads. **The window calls [`end_frame`] once at the
//! end of a frame**, because the pane and the tab are drawn in the same frame and each one giving back "what
//! was not drawn" would give back the other's.
//!
//! ## A surface is a layer, and a layer is painted where it is opened
//!
//! A `rux` layer reserves its slot in the painter's list when it is opened, so a layer opened after another is
//! painted over it, and everything `egui` paints after a layer is opened is painted over that layer. So
//! [`card`] and [`well`] each open a small layer of their own, and the words drawn after both are on top.

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

use crate::components::controls::TextBoxes;
use egui::{Pos2, Rect, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Fill, Rux, Style};

thread_local! {
    static STATE: RefCell<Option<(u64, Rc<rux::RuxState>)>> = const { RefCell::new(None) };
}

/// How far a card's shadow reaches past it, which a layer has to be made large enough to hold.
pub const REACH: f32 = 30.0;
/// How round a card is.
pub const CARD_RADIUS: f32 = 22.0;
/// How round a well is, unless it is a pill.
pub const WELL_RADIUS: f32 = 14.0;

/// The Designer's `rux` state for the context drawing now, at the pane's zoom.
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

/// Give back the canvases of the pieces not drawn this frame, once everything has been drawn.
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
    let id = ui.id().with(("designer-kit", salt));
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

/// A raised card in a layer of its own, for a part that draws its insides with `egui`.
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
        .mark(16.0, Some(1.6))
        .show(rux, rect)
        .clicked()
}

/// A pill switch over `rect`: a carved track holding one pill a choice. Answers the choice pressed.
///
/// @param label - what the switch is called, which names each pill as `<label>: <choice>`
pub fn pill_switch(
    rux: &mut Rux<'_>,
    rect: Rect,
    choices: &[String],
    chosen: Option<usize>,
    label: &'static str,
    small: bool,
) -> Option<usize> {
    let switch = rux::components::PillSwitch::new(choices, chosen, label);
    match small {
        true => switch.small().show(rux, rect),
        false => switch.show(rux, rect),
    }
}

/// The words of a heading, left aligned and centred on `at.y`. Answers how wide they came out.
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

/// Words in the code face, left aligned and centred on `at.y`, in the quieter ink.
pub fn mono(rux: &mut Rux<'_>, at: Pos2, words: &str, size: f32, room: f32) -> f32 {
    let ink = rux.theme().ink.i400;
    let style = rux.zs(Style::mono(size));
    let galley = rux.elided(style, words, ink, room.max(0.0));
    let width = galley.size().x;
    rux::text::draw_left_capitals(rux.painter(), at, galley, style, ink);
    width
}

/// Words wrapped to `width` at `at`, in the stronger ink. Answers where they end.
pub fn paragraph(rux: &mut Rux<'_>, at: Pos2, width: f32, words: &str, size: f32) -> f32 {
    let ink = rux.theme().ink.i700;
    let style = rux.zs(Style::sans(size).leading(1.45));
    let galley = rux::text::wrapped(rux.painter(), style, words, ink, width.max(40.0));
    let bottom = at.y + galley.size().y;
    rux.painter().galley(at, galley, ink);
    bottom
}

/// How tall words come out when wrapped to `width`, which is what a card is measured by before it is drawn.
pub fn paragraph_height(rux: &Rux<'_>, width: f32, words: &str, size: f32) -> f32 {
    let ink = rux.theme().ink.i700;
    let style = rux.zs(Style::sans(size).leading(1.45));
    rux::text::wrapped(rux.painter(), style, words, ink, width.max(40.0)).size().y
}

/// A small mark in a carved disc, which is what a section wears beside its heading.
pub fn badge(rux: &mut Rux<'_>, centre: Pos2, icon: Icon) {
    let side = rux.z(30.0);
    let disc = Rect::from_center_size(centre, Vec2::splat(side));
    rux::components::chip_disc(rux, disc);
    let ink = rux.theme().chat.sky;
    rux.mark(Mark::new(icon, rux.z(13.0)), centre, ink);
}

/// A carved field with a text box in it, named. Answers whether the words changed.
///
/// The surface is `rux`'s own carved well and the box is `egui`'s, set in the design's type and given the
/// well's whole height so a press anywhere in it is a press in the box. `radius` is the design's: 30 for the
/// prompt box, 18 for a note's text area, 16 for a reply.
///
/// @param rect - the well
/// @param radius - how round it is
/// @param text - the words
/// @param hint - what the empty box says
/// @param name - what the box is called, for a person and for a test
/// @param salt - what keeps this box's id apart from every other
/// @param multiline - whether Enter makes a new line
/// @param size - the type size, in the design's points
#[allow(clippy::too_many_arguments)]
pub fn carved_field(
    rux: &mut Rux<'_>,
    rect: Rect,
    radius: f32,
    text: &mut String,
    hint: &str,
    name: &str,
    salt: impl Hash + std::fmt::Debug,
    multiline: bool,
    size: f32,
) -> bool {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, chat.well, chat.carved());
    let across = rux.z(radius.min(22.0));
    let body = Rect::from_min_max(
        Pos2::new(rect.left() + across, rect.top()),
        Pos2::new(rect.right() - across, rect.bottom()),
    );
    let field = FieldText { well: rect, body, size, multiline };
    field_text(rux, field, text, hint, name, ("designer-field", salt))
}

/// A search well: a carved pill with the magnifier at its left and the words after it, every size the design's
/// times the pane's zoom. Answers whether the words changed.
///
/// `rux::components::TextInput::search` set its words, its placeholder, its padding and its magnifier at the
/// design's size whatever the zoom, and its placeholder sat at the top of the well; this is the Skills and
/// Designer panes' search instead (`task-2405`).
/// @param rect - the well
/// @param text - the words
/// @param hint - what the empty well says, which is also its name
/// @param salt - what keeps this box's id apart from every other
pub fn search_field(
    rux: &mut Rux<'_>,
    rect: Rect,
    text: &mut String,
    hint: &str,
    salt: impl Hash + std::fmt::Debug,
) -> bool {
    let theme = rux.theme();
    rux.chrome.surface(rect, rect.height() / 2.0, theme.chat.well, theme.chat.carved_sm());
    let mark = rux.z(13.0);
    let centre = Pos2::new(rect.left() + rux.z(18.0) + mark / 2.0, rect.center().y);
    rux.mark(Mark::new(Icon::Search, mark), centre, theme.ink.i500);
    let body = Rect::from_min_max(
        Pos2::new(centre.x + mark / 2.0 + rux.z(10.0), rect.top()),
        Pos2::new(rect.right() - rux.z(18.0), rect.bottom()),
    );
    let field = FieldText { well: rect, body, size: 13.0, multiline: false };
    field_text(rux, field, text, hint, hint, ("search-field", salt))
}

/// Where a field's words go: the well that takes a press, the strip across it the words are laid out in, their
/// size in the design's points, and whether Enter makes a new line.
#[derive(Clone, Copy)]
struct FieldText {
    well: Rect,
    body: Rect,
    size: f32,
    multiline: bool,
}

/// The text box inside a well, its one line centred on the well's middle and set at the pane's zoom, and its
/// placeholder in the same font. Answers whether the words changed.
fn field_text(
    rux: &mut Rux<'_>,
    field: FieldText,
    text: &mut String,
    hint: &str,
    name: &str,
    salt: impl Hash + std::fmt::Debug,
) -> bool {
    let theme = rux.theme();
    let FieldText { well: rect, size, multiline, .. } = field;
    let style = rux.zs(Style::sans(size));
    let down = match multiline {
        true => rux.z(12.0),
        false => ((rect.height() - rux.measure(style, "Ag").y) / 2.0).max(0.0),
    };
    let body = Rect::from_min_max(
        Pos2::new(field.body.left(), rect.top() + down),
        Pos2::new(field.body.right(), rect.bottom() - down),
    );
    let id = rux.ui.id().with(salt);
    // The whole well takes a press, a double click in its empty part included (`task-2392`).
    crate::components::controls::claim_the_field(rux.ui, rect, id, &format!("{name} field"));
    let edit = match multiline {
        true => egui::TextEdit::multiline(text).desired_rows(1),
        false => egui::TextEdit::singleline(text),
    };
    let response = rux.ui.text_box_at(
        body,
        edit.id(id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .desired_width(body.width())
            .font(style.font())
            .text_color(theme.ink.i900)
            .hint_text(crate::components::controls::placeholder(
                hint,
                &style.font(),
                theme.ink.i400,
            )),
    );
    let shown = text.clone();
    let named = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::text_edit(true, "", shown.clone(), named.clone()));
    response.changed()
}

/// How wide a button is for its words and a mark before them.
pub fn button_width(rux: &Rux<'_>, label: &str, mark: bool) -> f32 {
    let words = rux.measure(rux.zs(Style::sans(12.0).medium()), label).x;
    words + rux.z(32.0) + if mark { rux.z(20.0) } else { 0.0 }
}

/// How a button is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A chip raised out of the surface.
    Chip,
    /// No surface until it is pressed.
    Ghost,
    /// The action's gradient, for the one thing a surface is for.
    Primary,
}

/// A button with words, and a mark before them when it has one. Answers whether it was pressed.
///
/// The words are the control's name, so a test finds it by them.
pub fn button(rux: &mut Rux<'_>, rect: Rect, label: &str, mark: Option<Icon>, look: Look) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let id =
        rux.ui.id().with(("designer-button", label, rect.min.x.to_bits(), rect.min.y.to_bits()));
    let response = rux.ui.interact(rect, id, egui::Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let held = response.is_pointer_button_down_on();
    let radius = rect.height() / 2.0;
    let ink = match (look, response.hovered()) {
        (Look::Primary, _) => crate::theme::color::on_accent(),
        (_, true) => theme.ink.i900,
        (Look::Chip, false) => theme.ink.i700,
        (Look::Ghost, false) => theme.ink.i500,
    };
    match (look, held) {
        (Look::Primary, true) => rux.chrome.surface(
            rect,
            radius,
            Fill::gradient(chat.action, rect),
            chat.action_pressed(),
        ),
        (Look::Primary, false) => rux.chrome.surface(
            rect,
            radius,
            Fill::gradient(chat.action, rect),
            chat.action_raised(),
        ),
        (Look::Chip, true) | (Look::Ghost, true) => {
            rux.chrome.surface(rect, radius, chat.well, chat.carved_sm())
        }
        (Look::Chip, false) => {
            rux.chrome.surface(rect, radius, Fill::gradient(chat.chip, rect), chat.raised_chip())
        }
        (Look::Ghost, false) => {}
    }
    let style = rux.zs(Style::sans(12.0).medium());
    let mark_room = if mark.is_some() { rux.z(20.0) } else { 0.0 };
    // Words that would reach the button's edge end with `…` a little inside it, rather than running
    // out of both sides (`task-2282`). Several callers hand a button less than `button_width`'s full
    // padding and their words fit, so only the edge itself is the limit.
    let room = (rect.width() - rux.z(16.0) - mark_room).max(0.0);
    let mut galley = rux.text(style, label, ink);
    if galley.size().x > room + 1.0 {
        galley = rux.elided(style, label, ink, room);
    }
    let width = galley.size().x + mark_room;
    let left = rect.center().x - width / 2.0;
    if let Some(icon) = mark {
        rux.mark(Mark::new(icon, rux.z(13.0)), Pos2::new(left + rux.z(6.5), rect.center().y), ink);
    }
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left + mark_room, rect.center().y),
        galley,
        style,
        ink,
    );
    let named = label.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    response.clicked()
}

/// A small carved chip with words in the code face, such as an element's tag. Answers how wide it came out.
pub fn tag_chip(rux: &mut Rux<'_>, left_centre: Pos2, words: &str, room: f32) -> f32 {
    let theme = rux.theme();
    let style = rux.zs(Style::mono(11.0));
    let galley = rux.elided(style, words, theme.ink.i500, (room - rux.z(20.0)).max(0.0));
    let width = galley.size().x + rux.z(20.0);
    let chip = Rect::from_min_size(
        Pos2::new(left_centre.x, left_centre.y - rux.z(11.0)),
        Vec2::new(width, rux.z(22.0)),
    );
    well_surface(rux, chip, chip.height() / 2.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(chip.left() + rux.z(10.0), chip.center().y),
        galley,
        style,
        theme.ink.i500,
    );
    width
}

/// A round number on the action's gradient, which is what a note's pin is. `dashed` draws it as a note not yet
/// made: the number it will have, with a ring round it and no fill.
pub fn number_disc(rux: &mut Rux<'_>, centre: Pos2, diameter: f32, number: u32, dashed: bool) {
    let theme = rux.theme();
    let chat = theme.chat;
    let disc = Rect::from_center_size(centre, Vec2::splat(rux.z(diameter)));
    let on = crate::theme::color::on_accent();
    let style = rux.zs(Style::sans(diameter * 0.46).semibold());
    match dashed {
        true => {
            rux.painter().circle_stroke(
                disc.center(),
                disc.width() / 2.0 - 1.0,
                egui::Stroke::new(1.5, chat.sky),
            );
            let galley = rux.text(style, &number.to_string(), chat.sky);
            rux::text::draw_centred(rux.painter(), disc, galley, chat.sky);
        }
        false => {
            rux.chrome.surface(
                disc,
                disc.height() / 2.0,
                Fill::gradient(chat.action, disc),
                chat.action_raised(),
            );
            let galley = rux.text(style, &number.to_string(), on);
            rux::text::draw_centred(rux.painter(), disc, galley, on);
        }
    }
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
        assert_eq!(again.zoom(), 1.5, "the zoom is what rux measures at");
        let other = egui::Context::default();
        let elsewhere = state(&other, 1.0);
        assert!(!Rc::ptr_eq(&first, &elsewhere), "a second window gets a state of its own");
    }
}
