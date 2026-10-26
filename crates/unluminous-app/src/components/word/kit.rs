//! The parts the Word tab is drawn with: the round chip, the select chip, the pill button, the section
//! heading, the colour swatches, the switch, and the egui style its deep panel levels are set in
//! (`task-2366`).
//!
//! They are the window's own parts (`text_tools`' round chip, `settings_kit`'s select, pill and switch,
//! `controls::menu_frame`'s card) at the sizes the Claude Design canvas gives them, so the Word tab reads as
//! the rest of the window does. They are gathered here so the bar, the panel, the outline toolbar and the
//! popups over the pages cannot disagree about what a button is.
//!
//! ## Sizes
//!
//! Every size is the design's points times the tab's own zoom, which [`set_zoom`] records once a frame and
//! [`zoom`] reads. None follows the editor's font: a machine whose interface font was 24 points set the
//! whole Word chrome at twice the size of the window's menus, which is one of the faults the ticket reports.
//!
//! ## One `rux` state per window
//!
//! The tab zooms by a multiplier, so it keeps a `rux` state of its own rather than the title bar's, for the
//! reason `components::database::kit` gives. It is held per thread and per egui context, and the window gives
//! back the canvases a frame did not draw through [`end_frame`].

use std::cell::RefCell;
use std::hash::Hash;
use std::rc::Rc;

use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Vec2};
use rux::icon::{Icon, Mark as RuxMark};
use rux::{Fill, Rux};

use crate::components::controls::{self, WithHint};
use crate::theme::crisp::CrispPainter;
use crate::theme::svg_icon::{self, SvgMark};

thread_local! {
    static STATE: RefCell<Option<(u64, Rc<rux::RuxState>)>> = const { RefCell::new(None) };
}

/// How far a chip's shadow reaches past it, which a layer is made large enough to hold.
const REACH: f32 = 12.0;
/// How tall a pill button is.
pub const PILL: f32 = 28.0;
/// How tall a select chip is in the bar.
pub const CHIP: f32 = 30.0;
/// How large a round chip is: the title bar's 32 points.
pub const ROUND: f32 = 32.0;
/// How large a colour swatch is.
const SWATCH: f32 = 18.0;

/// A mark: one of the design's own, or one kept as SVG path data in `theme::svg_icon`.
#[derive(Clone, Copy)]
pub enum Mark {
    Rux(Icon),
    Svg(&'static SvgMark),
}

/// The tab's `rux` state for the context drawing now.
fn state(ctx: &egui::Context) -> Rc<rux::RuxState> {
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
    state
}

/// Give back the canvases of the pieces not drawn this frame, once the frame has been drawn.
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

/// Record the tab's zoom for this frame, which every size below is multiplied by.
pub fn set_zoom(ctx: &egui::Context, zoom: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("word-kit-zoom"), zoom.max(0.25)));
}

/// The tab's zoom, 1.0 until [`set_zoom`] says otherwise.
pub fn zoom(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp(egui::Id::new("word-kit-zoom"))).unwrap_or(1.0)
}

/// Draw with `rux` into `rect`, in a layer of its own that reaches past it for the shadows.
pub fn layer<R>(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    rect: Rect,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let state = state(ui.ctx());
    let id = ui.id().with(("word-kit", salt));
    rux::layer(ui, &state, id, rect.expand(REACH * zoom(ui.ctx())), add)
}

/// The design's values for the pane the tab is drawn on.
pub fn chat() -> rux::theme::Chat {
    crate::theme::rux_theme().chat
}

/// The design's ink ladder.
pub fn ink() -> rux::theme::Ink {
    crate::theme::rux_theme().ink
}

/// Paint `mark` centred on `centre`, `size` points across.
pub fn paint_mark(rux: &mut Rux<'_>, mark: Mark, centre: Pos2, size: f32, tint: Color32) {
    match mark {
        Mark::Rux(icon) => rux.mark(RuxMark::new(icon, size), centre, tint),
        Mark::Svg(svg) => svg_icon::paint(rux.painter(), centre, size, svg, tint),
    }
}

/// A chip surface raised out of whatever it sits on, or carved into it while `carved`.
pub fn chip_surface(rux: &mut Rux<'_>, rect: Rect, radius: f32, carved: bool) {
    let chat = rux.theme().chat;
    match carved {
        true => rux.chrome.surface(rect, radius, chat.well, chat.carved_sm()),
        false => {
            rux.chrome.surface(rect, radius, Fill::gradient(chat.chip, rect), chat.raised_chip())
        }
    }
}

/// A well carved into whatever it sits on.
pub fn well_surface(rux: &mut Rux<'_>, rect: Rect, radius: f32) {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, chat.well, chat.carved_sm());
}

/// A raised card: the chat card's gradient with the design's small raised shadow.
pub fn card_surface(rux: &mut Rux<'_>, rect: Rect, radius: f32) {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, Fill::gradient(chat.card, rect), chat.raised_sm());
}

/// The round chip of the title bar: a disc raised out of the bar with a line mark, carved in while what it
/// opens is open (`on`). `name` is what a test asks for and `hint` what the pointer is told.
pub fn round_chip(
    ui: &mut egui::Ui,
    rect: Rect,
    name: &str,
    hint: &str,
    mark: Mark,
    on: bool,
) -> Response {
    let z = zoom(ui.ctx());
    let response =
        ui.interact(rect, ui.id().with(("word-round", name)), Sense::click()).with_hint(hint);
    let hovered = response.hovered();
    let held = response.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    layer(ui, ("round", name), rect, |rux| {
        let ink = rux.theme().ink;
        let chat = rux.theme().chat;
        match on || held {
            true => well_surface(rux, rect, rect.width() / 2.0),
            false => rux::components::chip_disc(rux, rect),
        }
        let tint = match (on, hovered) {
            (true, _) => ink.i900,
            (false, true) => chat.sky,
            (false, false) => ink.i500,
        };
        paint_mark(rux, mark, rect.center(), 16.0 * z, tint);
    });
    let named = name.to_owned();
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, &named));
    response
}

/// Words in the proportional face at `size` points, laid out on one line and cut with an ellipsis to
/// `width`.
pub fn words(
    ui: &egui::Ui,
    text: &str,
    size: f32,
    colour: Color32,
    width: f32,
) -> crate::theme::crisp::Text {
    let font = egui::FontId::proportional(size);
    let (shown, _) = controls::elide(text, width.max(0.0), |t| {
        ui.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font.clone(), colour).size().x)
    });
    ui.painter().crisp_layout_no_wrap(shown, font, colour)
}

/// Draw `galley` with its capitals centred on `centre_y`, its left edge at `left`.
pub fn draw_by_capitals(
    painter: &egui::Painter,
    left: f32,
    centre_y: f32,
    galley: crate::theme::crisp::Text,
    size: f32,
    colour: Color32,
) {
    let font = egui::FontId::proportional(size);
    let top = crate::theme::crisp::top_centring_capitals(painter, &font, centre_y);
    painter.crisp_galley(Pos2::new(left, top), galley, colour);
}

/// A raised pill with the value and a chevron, opening a menu card under it whose rows `rows` draws. Answers
/// what `rows` answered on the frame a row was chosen, and closes the card then.
pub fn select_chip<T>(
    ui: &mut egui::Ui,
    rect: Rect,
    name: &str,
    value: &str,
    list_width: f32,
    rows: impl FnOnce(&mut egui::Ui) -> Option<T>,
) -> Option<T> {
    let z = zoom(ui.ctx());
    let response = ui.interact(rect, ui.id().with(("word-select", name)), Sense::click());
    let popup_id = egui::Popup::default_response_id(&response);
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id) != response.clicked();
    let hovered = response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = ink();
    let shows_words = rect.width() > 44.0 * z;
    layer(ui, ("select", name), rect, |rux| {
        chip_surface(rux, rect, rect.height() / 2.0, open);
        let chevron = match shows_words {
            true => Pos2::new(rect.right() - 15.0 * z, rect.center().y),
            false => rect.center(),
        };
        let tint = if hovered { rux.theme().chat.sky } else { ink.i400 };
        rux.mark(RuxMark::new(Icon::ChevDown, 10.0 * z).stroke(1.6 * 2.4), chevron, tint);
    });
    if shows_words {
        let size = 12.5 * z;
        let left = rect.left() + 14.0 * z;
        let galley = words(ui, value, size, ink.i900, rect.right() - 28.0 * z - left);
        draw_by_capitals(ui.painter(), left, rect.center().y, galley, size, ink.i900);
    }
    let response = response.with_hint(format!("{name}: {value}"));
    let (named, said) = (name.to_owned(), value.to_owned());
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &named);
        info.current_text_value = Some(said.clone());
        info
    });
    let chosen = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(controls::menu_frame(ui))
        .width(list_width)
        .show(|ui| {
            controls::set_in_the_list_size(ui);
            rows(ui)
        })
        .and_then(|inner| inner.inner);
    if chosen.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    chosen
}

/// A row of a menu card whose name is set in its own size and weight: the style list shows each paragraph
/// style looking a little like itself. Otherwise `controls::menu_row`.
pub fn styled_row(ui: &mut egui::Ui, name: &str, size: f32, strong: bool, checked: bool) -> bool {
    let height = (size * 1.9).max(controls::MENU_ROW);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let chat = chat();
    let ink = ink();
    if response.hovered() {
        let painter = ui.painter();
        painter.rect_filled(rect, egui::CornerRadius::same(10), chat.well);
        painter.rect_stroke(
            rect.shrink(0.5),
            egui::CornerRadius::same(10),
            Stroke::new(1.0, chat.dark.gamma_multiply(0.55)),
            egui::StrokeKind::Inside,
        );
    }
    if checked {
        crate::theme::icon::tick(
            ui.painter(),
            Pos2::new(rect.left() + 19.0, rect.center().y),
            chat.faster,
        );
    }
    let left = rect.left() + 36.0;
    let mut job = egui::text::LayoutJob::default();
    let (shown, cut) = controls::elide(name, rect.right() - 12.0 - left, |t| {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(t.to_owned(), egui::FontId::proportional(size), ink.i900).size().x
        })
    });
    job.append(
        &shown,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: ink.i900,
            ..Default::default()
        },
    );
    let galley = ui.painter().crisp_layout_job(job);
    let at = Pos2::new(left, rect.center().y - galley.size().y / 2.0);
    // The interface face may have no bold, so a strong name is drawn twice, half a point apart.
    if strong {
        ui.painter().crisp_galley(at + Vec2::new(0.5, 0.0), galley.clone(), ink.i900);
    }
    ui.painter().crisp_galley(at, galley, ink.i900);
    let response = if cut { response.with_hint(name) } else { response };
    let named = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, checked, &named)
    });
    response.clicked()
}

/// How wide a pill button is for `label`.
pub fn pill_width(ui: &egui::Ui, label: &str) -> f32 {
    let z = zoom(ui.ctx());
    let font = egui::FontId::proportional(12.0 * z);
    let words = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font, Color32::WHITE).size().x);
    words + 26.0 * z
}

/// A pill button at `rect`: a chip with a word on it, carved in while `on`, quiet and taking no press while
/// not `enabled`. `name` is its accessible name. Answers its response.
pub fn pill_at(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    name: &str,
    on: bool,
    enabled: bool,
) -> Response {
    let z = zoom(ui.ctx());
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let response = ui.interact(rect, ui.auto_id_with(("word-pill", name)), sense);
    let hovered = enabled && response.hovered();
    let held = enabled && response.is_pointer_button_down_on();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = ink();
    // Named by the response, which is the same from frame to frame while the pill scrolls, so its canvas is
    // kept rather than made again on every frame of a scroll.
    layer(ui, ("pill", response.id), rect, |rux| {
        chip_surface(rux, rect, rect.height() / 2.0, on || held);
        let tint = match (enabled, on, hovered) {
            (false, _, _) => ink.i300,
            (true, true, _) => ink.i900,
            (true, false, true) => rux.theme().chat.sky,
            (true, false, false) => ink.i700,
        };
        let size = 12.0 * z;
        let font = egui::FontId::proportional(size);
        let (shown, _) = controls::elide(label, rect.width() - 14.0 * z, |t| {
            rux.ui.fonts_mut(|f| f.layout_no_wrap(t.to_owned(), font.clone(), tint).size().x)
        });
        let galley = rux.painter().crisp_layout_no_wrap(shown, font.clone(), tint);
        let left = rect.center().x - galley.size().x / 2.0;
        draw_by_capitals(rux.painter(), left, rect.center().y, galley, size, tint);
    });
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, &named));
    response
}

/// A pill button laid out in `ui`, as wide as its word.
pub fn pill(ui: &mut egui::Ui, label: &str, on: bool, enabled: bool) -> Response {
    let z = zoom(ui.ctx());
    let size = Vec2::new(pill_width(ui, label), PILL * z);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    pill_at(ui, rect, label, label, on, enabled)
}

/// A pill button laid out in `ui` whose accessible name is not its word.
pub fn named_pill(ui: &mut egui::Ui, label: &str, name: &str) -> Response {
    let z = zoom(ui.ctx());
    let size = Vec2::new(pill_width(ui, label), PILL * z);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    pill_at(ui, rect, label, name, false, true)
}

/// A pill button in `ui` in place of egui's `ui.button`.
pub fn button(ui: &mut egui::Ui, label: impl Into<String>) -> Response {
    let label = label.into();
    pill(ui, &label, false, true)
}

/// A pill that stays carved in while `on`, in place of egui's `ui.selectable_label`.
pub fn toggle(ui: &mut egui::Ui, on: bool, label: impl Into<String>) -> Response {
    let label = label.into();
    pill(ui, &label, on, true)
}

/// A switch with its label at the left, in place of egui's `ui.checkbox`. Answers a response whose
/// `changed()` says it was flipped.
pub fn check(ui: &mut egui::Ui, value: &mut bool, label: impl Into<String>) -> Response {
    let label = label.into();
    let z = zoom(ui.ctx());
    let width = ui.available_width().max(160.0 * z);
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, 28.0 * z), Sense::hover());
    let track = Rect::from_min_size(
        Pos2::new(row.right() - 40.0 * z, row.center().y - 11.0 * z),
        Vec2::new(40.0 * z, 22.0 * z),
    );
    let label_rect = Rect::from_min_max(row.min, Pos2::new(track.left() - 8.0 * z, row.max.y));
    let mut changed = false;
    let salt = (label.clone(), row.min.y.to_bits());
    layer(ui, ("check", salt), track, |rux| {
        changed = crate::components::settings_kit::switch(rux, track, &label, value, label_rect);
    });
    let size = 12.5 * z;
    let ink = ink();
    let galley = words(ui, &label, size, ink.i700, label_rect.width());
    draw_by_capitals(ui.painter(), label_rect.left(), row.center().y, galley, size, ink.i700);
    let mut response = ui.interact(row, ui.auto_id_with(("word-check", &label)), Sense::hover());
    if changed {
        response.mark_changed();
    }
    response
}

/// A section heading inside a panel level: small spaced capitals in the quiet ink, with room above it.
pub fn section(ui: &mut egui::Ui, title: &str) {
    let z = zoom(ui.ctx());
    ui.add_space(6.0 * z);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0 * z), Sense::hover());
    let ink = ink();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &title.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(10.5 * z),
            color: ink.i400,
            extra_letter_spacing: 0.9 * z,
            ..Default::default()
        },
    );
    let galley = ui.painter().crisp_layout_job(job);
    ui.painter().crisp_galley(
        Pos2::new(rect.left() + 2.0 * z, rect.center().y - galley.size().y / 2.0),
        galley,
        ink.i400,
    );
    let named = title.to_owned();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &named));
}

/// Colour swatches: circles in rows that wrap, a ring round the `chosen` one. Answers the one pressed.
pub fn swatches(
    ui: &mut egui::Ui,
    colours: &[(String, Color32)],
    chosen: Option<usize>,
) -> Option<usize> {
    let z = zoom(ui.ctx());
    let mut pressed = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(7.0 * z);
        for (index, (name, colour)) in colours.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(SWATCH * z), Sense::click());
            let response = response.with_hint(name);
            let painter = ui.painter();
            let centre = rect.center();
            let radius = SWATCH * z / 2.0;
            painter.circle_filled(centre, radius - 1.0, *colour);
            // A hairline in the dark half of the design's shadow pair, so white and the pane's own colour
            // still read as circles.
            painter.circle_stroke(
                centre,
                radius - 1.0,
                Stroke::new(1.0, chat().dark.gamma_multiply(0.8)),
            );
            if chosen == Some(index) || response.hovered() {
                let ring = if chosen == Some(index) { chat().sky } else { ink().i500 };
                painter.circle_stroke(centre, radius + 1.5 * z, Stroke::new(1.5 * z, ring));
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let named = name.clone();
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    chosen == Some(index),
                    &named,
                )
            });
            if response.clicked() {
                pressed = Some(index);
            }
        }
    });
    pressed
}

/// A label for the control after it in a row, at the panel's size in the middle ink.
pub fn label(ui: &mut egui::Ui, text: &str) -> Response {
    let z = zoom(ui.ctx());
    ui.label(egui::RichText::new(text).size(12.5 * z).color(ink().i700))
}

/// Set the egui style of a part of the tab that still draws egui's own widgets (a number to drag, a list
/// to choose from, a field to type in) to the design's: the fields are carved wells with round ends, the
/// lists open in the menu card, and the words are the design's sizes at the tab's zoom.
pub fn style_the_panel(ui: &mut egui::Ui) {
    let z = zoom(ui.ctx());
    let chat = chat();
    let ink = ink();
    let style = ui.style_mut();
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Small, FontId::new(11.0 * z, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(12.5 * z, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(12.0 * z, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(15.0 * z, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0 * z, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(6.0 * z, 8.0 * z);
    style.spacing.button_padding = Vec2::new(10.0 * z, 5.0 * z);
    style.spacing.interact_size = Vec2::new(40.0 * z, 26.0 * z);
    style.spacing.combo_width = 160.0 * z;
    let corner = egui::CornerRadius::same((13.0 * z).round().clamp(0.0, 255.0) as u8);
    let visuals = &mut style.visuals;
    let widget = |fill: Color32, stroke: Stroke, ink: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: stroke,
        corner_radius: corner,
        fg_stroke: Stroke::new(1.2, ink),
        expansion: 0.0,
    };
    visuals.widgets.inactive =
        widget(chat.well, Stroke::new(1.0, chat.dark.gamma_multiply(0.6)), ink.i700);
    visuals.widgets.hovered =
        widget(chat.well, Stroke::new(1.0, chat.sky.gamma_multiply(0.6)), ink.i900);
    visuals.widgets.active = widget(chat.well, Stroke::new(1.0, chat.sky), ink.i900);
    visuals.widgets.open =
        widget(chat.well, Stroke::new(1.0, chat.sky.gamma_multiply(0.6)), ink.i900);
    visuals.widgets.noninteractive =
        widget(Color32::TRANSPARENT, Stroke::new(1.0, chat.dark.gamma_multiply(0.7)), ink.i700);
    visuals.extreme_bg_color = chat.well;
    visuals.text_edit_bg_color = Some(chat.well);
    visuals.selection.bg_fill = chat.sky.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, chat.sky);
    visuals.window_fill = chat.card.from;
    visuals.panel_fill = chat.card.from;
    visuals.window_stroke = Stroke::NONE;
    visuals.window_corner_radius = egui::CornerRadius::same(18);
    visuals.menu_corner_radius = egui::CornerRadius::same(18);
    visuals.popup_shadow =
        egui::Shadow { offset: [0, 16], blur: 22, spread: 0, color: chat.dark.gamma_multiply(0.9) };
    visuals.override_text_color = Some(ink.i700);
}
