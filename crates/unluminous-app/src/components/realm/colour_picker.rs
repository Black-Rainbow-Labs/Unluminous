//! The colour picker a shape's `Colour...` row opens. `task-2284`.
//!
//! The design's RGBA picker: a saturation and value square, a hue strip, an alpha strip over a checkerboard,
//! fields for the colour as `#RRGGBBAA` and as its four numbers, a row of swatches, the colour before and the
//! colour now side by side, and Cancel and Apply.
//!
//! **It floats beside the menu rather than being a modal**, because the point of it is to watch the shape
//! change while the colour is dragged, and a modal's backdrop would darken the shape being recoloured. It can
//! be moved by its heading. The colour being chosen is held in [`State`] and the window draws the shape in it,
//! so nothing is written to the realm file until Apply.
//!
//! **There is no "use the theme's colour" button.** The ticket asks for none: a shape that has never been
//! given a colour follows the theme, and the picker opens on the theme's card colour for such a shape.

use egui::epaint::Mesh;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rux::components::ChipButton;
use rux::icon::Icon;
use rux::{Rux, Style};
use unluminous_core::Rgba;

use crate::components::color_wheel::{hsv_to_rgb, rgb_to_hsv};
use crate::components::controls::TextBoxes;
use crate::components::{modal, settings_kit as kit};
use crate::services::realm::NodeId;
use crate::theme::color;

/// How wide the picker is, and the margin inside it.
const WIDTH: f32 = 300.0;
const PAD: f32 = 16.0;
/// The heading's row, which is also the close chip.
const HEADING: f32 = 26.0;
/// The saturation and value square.
const SQUARE: f32 = 150.0;
/// A strip, and the well it is carved into.
const STRIP: f32 = 14.0;
const WELL_PAD: f32 = 3.0;
/// The small capitals over a strip and under a field.
const LABEL: f32 = 12.0;
/// The fields' row.
const FIELD: f32 = 28.0;
/// One swatch, and the gap between two.
const SWATCH: f32 = 24.0;
const SWATCH_GAP: f32 = 7.0;
/// The before and after bar, and the buttons.
const COMPARE: f32 = 28.0;
const BUTTON: f32 = 32.0;
/// One square of the checkerboard behind a translucent colour.
const CHECK: f32 = 5.0;
/// How many pieces a strip's gradient is drawn in.
const STEPS: usize = 36;
/// How finely the square's gradient is drawn.
const GRID: usize = 12;

/// How tall the whole picker is, worked out from its rows so a row added later moves the bottom with it.
pub const HEIGHT: f32 = PAD
    + HEADING
    + 12.0
    + SQUARE
    + 8.0
    + 12.0
    + LABEL
    + 4.0
    + STRIP
    + WELL_PAD * 2.0
    + 12.0
    + LABEL
    + 4.0
    + STRIP
    + WELL_PAD * 2.0
    + 12.0
    + FIELD
    + 4.0
    + LABEL
    + 14.0
    + SWATCH * 2.0
    + SWATCH_GAP
    + 14.0
    + COMPARE
    + 16.0
    + BUTTON
    + PAD;

/// The picker while it is open: which shape, the colour it had, and the colour being chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub node: NodeId,
    /// The shape's colour when the picker opened, which Cancel leaves it with. `None` is the theme's card.
    pub before: Option<Rgba>,
    /// The colour being chosen, as hue, saturation and value from 0 to 1, and an alpha.
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub alpha: u8,
    /// What is in the five fields. A field holding the keyboard keeps what is typed into it; the others
    /// follow the colour.
    pub hex: String,
    pub red: String,
    pub green: String,
    pub blue: String,
    pub percent: String,
    /// Where it opens, in screen points.
    pub at: Pos2,
}

impl State {
    /// A picker for `node`, opening on its colour, or on the theme's card colour for a shape that follows the
    /// theme.
    pub fn new(node: NodeId, before: Option<Rgba>, at: Pos2) -> State {
        let mut state = State {
            node,
            before,
            hue: 0.0,
            saturation: 0.0,
            value: 0.0,
            alpha: 255,
            hex: String::new(),
            red: String::new(),
            green: String::new(),
            blue: String::new(),
            percent: String::new(),
            at,
        };
        let card = super::card_colour();
        state.set(before.unwrap_or(Rgba::new(card.r(), card.g(), card.b(), 255)));
        state
    }

    /// The colour being chosen.
    pub fn colour(&self) -> Rgba {
        hsv_to_rgb(self.hue, self.saturation, self.value, self.alpha)
    }

    /// Put the picker on `colour`. A grey keeps the hue it had, so dragging through grey does not throw the
    /// hue strip back to red.
    pub fn set(&mut self, colour: Rgba) {
        let (hue, saturation, value) = rgb_to_hsv(colour);
        if saturation > 0.0 && value > 0.0 {
            self.hue = hue;
        }
        self.saturation = saturation;
        self.value = value;
        self.alpha = colour.a;
        self.fill_the_fields(None);
    }

    /// Write the colour into every field but `typing`, which is the one somebody is typing into.
    fn fill_the_fields(&mut self, typing: Option<Field>) {
        let colour = self.colour();
        if typing != Some(Field::Hex) {
            self.hex = colour.to_hex();
        }
        if typing != Some(Field::Red) {
            self.red = colour.r.to_string();
        }
        if typing != Some(Field::Green) {
            self.green = colour.g.to_string();
        }
        if typing != Some(Field::Blue) {
            self.blue = colour.b.to_string();
        }
        if typing != Some(Field::Alpha) {
            self.percent = format!("{:.0}", f32::from(colour.a) / 255.0 * 100.0);
        }
    }

    /// Read what was typed into `field`, and move the colour to it when it reads as one.
    fn read_a_field(&mut self, field: Field) {
        let channel =
            |text: &str, most: f32| text.trim().parse::<f32>().ok().map(|n| n.clamp(0.0, most));
        let now = self.colour();
        let colour = match field {
            Field::Hex => Rgba::parse(&self.hex),
            Field::Red => channel(&self.red, 255.0).map(|r| Rgba { r: r as u8, ..now }),
            Field::Green => channel(&self.green, 255.0).map(|g| Rgba { g: g as u8, ..now }),
            Field::Blue => channel(&self.blue, 255.0).map(|b| Rgba { b: b as u8, ..now }),
            Field::Alpha => channel(&self.percent, 100.0)
                .map(|percent| Rgba { a: (percent / 100.0 * 255.0).round() as u8, ..now }),
        };
        if let Some(colour) = colour {
            let (hue, saturation, value) = rgb_to_hsv(colour);
            if saturation > 0.0 && value > 0.0 {
                self.hue = hue;
            }
            self.saturation = saturation;
            self.value = value;
            self.alpha = colour.a;
            self.fill_the_fields(Some(field));
        }
    }
}

/// One of the five fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Hex,
    Red,
    Green,
    Blue,
    Alpha,
}

/// What the picker reported.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// The colour moved this frame.
    pub changed: bool,
    /// Apply was pressed: keep the colour.
    pub apply: bool,
    /// Cancel, the close chip or Escape: put the colour back.
    pub cancel: bool,
}

/// The colours the swatches offer: the theme's accent and action blues, its sky, its greens, its amber and
/// coral, a violet, the four colours a passage of text can be highlighted in, its text and its quiet grey.
fn swatches() -> Vec<Rgba> {
    let theme = crate::theme::rux_theme();
    let chat = theme.chat;
    let solid = |colour: Color32| {
        let [r, g, b, a] = colour.to_srgba_unmultiplied();
        Rgba::new(r, g, b, a)
    };
    vec![
        solid(chat.action.from),
        solid(color::accent()),
        solid(chat.sky),
        solid(chat.faster),
        solid(chat.done.from),
        solid(chat.waiting),
        solid(chat.slower),
        solid(theme.accent.violet),
        color::HIGHLIGHT_YELLOW,
        color::HIGHLIGHT_GREEN,
        color::HIGHLIGHT_BLUE,
        color::HIGHLIGHT_PINK,
        solid(color::text()),
        solid(chat.quiet),
    ]
}

/// Draw the picker and report what was done in it.
pub fn show(ctx: &egui::Context, state: &mut State) -> Outcome {
    let mut outcome = Outcome::default();
    let before = state.colour();
    egui::Area::new(egui::Id::new("realm-colour-picker"))
        .order(egui::Order::Foreground)
        .default_pos(state.at)
        .movable(true)
        .constrain(true)
        .show(ctx, |ui| {
            let (area, _) = ui.allocate_exact_size(Vec2::new(WIDTH, HEIGHT), Sense::hover());
            kit::layer_reaching(ui, "realm-colour-picker", area, modal::SHADOW_REACH, |rux| {
                let chat = rux.theme().chat;
                rux.chrome.surface(area, 22.0, chat.ground, modal::shadow());
                draw_the_rows(rux, area, state, &mut outcome);
            });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        outcome.cancel = true;
    }
    outcome.changed = state.colour() != before;
    outcome
}

/// Every row of the picker, top to bottom.
fn draw_the_rows(rux: &mut Rux<'_>, area: Rect, state: &mut State, outcome: &mut Outcome) {
    let inner = area.shrink(PAD);
    let mut pen = inner.top();
    if heading(
        rux,
        Rect::from_min_size(Pos2::new(inner.left(), pen), Vec2::new(inner.width(), HEADING)),
    ) {
        outcome.cancel = true;
    }
    pen += HEADING + 12.0;
    let square =
        Rect::from_min_size(Pos2::new(inner.left(), pen), Vec2::new(inner.width(), SQUARE + 8.0));
    shade_square(rux, square, state);
    pen = square.bottom() + 12.0;
    small_capitals(rux, Pos2::new(inner.left(), pen), "HUE", None);
    pen += LABEL + 4.0;
    let hue = Rect::from_min_size(
        Pos2::new(inner.left(), pen),
        Vec2::new(inner.width(), STRIP + WELL_PAD * 2.0),
    );
    hue_strip(rux, hue, state);
    pen = hue.bottom() + 12.0;
    let percent = format!("{:.0}%", f32::from(state.alpha) / 255.0 * 100.0);
    small_capitals(
        rux,
        Pos2::new(inner.left(), pen),
        "ALPHA",
        Some((inner.right(), percent.as_str())),
    );
    pen += LABEL + 4.0;
    let alpha = Rect::from_min_size(
        Pos2::new(inner.left(), pen),
        Vec2::new(inner.width(), STRIP + WELL_PAD * 2.0),
    );
    alpha_strip(rux, alpha, state);
    pen = alpha.bottom() + 12.0;
    fields(
        rux,
        Rect::from_min_size(
            Pos2::new(inner.left(), pen),
            Vec2::new(inner.width(), FIELD + 4.0 + LABEL),
        ),
        state,
    );
    pen += FIELD + 4.0 + LABEL + 14.0;
    let row = Rect::from_min_size(
        Pos2::new(inner.left(), pen),
        Vec2::new(inner.width(), SWATCH * 2.0 + SWATCH_GAP),
    );
    swatch_rows(rux, row, state);
    pen = row.bottom() + 14.0;
    compare(
        rux,
        Rect::from_min_size(Pos2::new(inner.left(), pen), Vec2::new(inner.width(), COMPARE)),
        state,
    );
    pen += COMPARE + 16.0;
    let foot = Rect::from_min_size(Pos2::new(inner.left(), pen), Vec2::new(inner.width(), BUTTON));
    let apply = rux::components::PillButton::new("Apply").primary();
    let width = apply.measure(rux).x.max(84.0);
    let apply_rect =
        Rect::from_min_size(Pos2::new(foot.right() - width, foot.top()), Vec2::new(width, BUTTON));
    if apply.show(rux, apply_rect).clicked() {
        outcome.apply = true;
    }
    let cancel_width = kit::button_width(rux, "Cancel", false);
    let cancel = Rect::from_min_size(
        Pos2::new(apply_rect.left() - 10.0 - cancel_width, foot.top() + 1.0),
        Vec2::new(cancel_width, BUTTON - 2.0),
    );
    if kit::button(rux, cancel, "Cancel", kit::Kind::Chip) {
        outcome.cancel = true;
    }
}

/// The heading and its round close chip. Answers whether the chip was pressed.
fn heading(rux: &mut Rux<'_>, row: Rect) -> bool {
    let theme = rux.theme();
    let close = Rect::from_center_size(
        Pos2::new(row.right() - HEADING / 2.0, row.center().y),
        Vec2::splat(HEADING),
    );
    let pressed = ChipButton::new(Icon::X, HEADING, "Close colour picker")
        .mark(10.0, Some(1.6))
        .show(rux, close)
        .clicked();
    let style = Style::sans(13.0).semibold();
    let galley = rux.text(style, "Colour", theme.ink.i900);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(row.left(), row.center().y),
        galley,
        style,
        theme.ink.i900,
    );
    pressed
}

/// A label in the design's small spaced capitals, with a reading at the right end when one is given.
fn small_capitals(rux: &mut Rux<'_>, at: Pos2, words: &str, reading: Option<(f32, &str)>) {
    let ink = rux.theme().ink.i400;
    let style = Style::sans(10.5).tracking(0.1);
    let galley = rux.text(style, words, ink);
    rux.painter().galley(at, galley, ink);
    if let Some((right, said)) = reading {
        let galley = rux.text(style, said, ink);
        let width = galley.size().x;
        rux.painter().galley(Pos2::new(right - width, at.y), galley, ink);
    }
}

/// Where a press or a drag in a control is, as fractions of the control across and down.
fn dragged_to(rux: &mut Rux<'_>, rect: Rect, id: &str, name: &str) -> Option<Vec2> {
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("realm-colour", id)), Sense::click_and_drag());
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, named.clone()));
    if response.hovered() || response.dragged() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if !(response.dragged() || response.clicked() || response.is_pointer_button_down_on()) {
        return None;
    }
    let at = response.interact_pointer_pos()?;
    Some(Vec2::new(
        ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((at.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    ))
}

/// The saturation and value square in its well, with the knob where the colour is.
fn shade_square(rux: &mut Rux<'_>, well: Rect, state: &mut State) {
    let chat = rux.theme().chat;
    rux.chrome.surface(well, 12.0, chat.well, chat.carved_sm());
    let square = well.shrink(4.0);
    if let Some(at) = dragged_to(rux, square, "square", "Saturation and brightness") {
        state.saturation = at.x;
        state.value = 1.0 - at.y;
        state.fill_the_fields(None);
    }
    let mut mesh = Mesh::default();
    for row in 0..=GRID {
        for column in 0..=GRID {
            let (s, v) = (column as f32 / GRID as f32, 1.0 - row as f32 / GRID as f32);
            let at = Pos2::new(
                square.left() + square.width() * s,
                square.top() + square.height() * (1.0 - v),
            );
            mesh.colored_vertex(at, crate::theme::color32(hsv_to_rgb(state.hue, s, v, 255)));
            if row > 0 && column > 0 {
                let here = (row * (GRID + 1) + column) as u32;
                let (left, up, corner) =
                    (here - 1, here - (GRID as u32 + 1), here - (GRID as u32 + 2));
                mesh.add_triangle(corner, up, here);
                mesh.add_triangle(corner, left, here);
            }
        }
    }
    rux.painter().add(Shape::mesh(mesh));
    let knob = Pos2::new(
        square.left() + square.width() * state.saturation,
        square.top() + square.height() * (1.0 - state.value),
    );
    let solid = state.colour();
    paint_a_knob(rux.painter(), knob, crate::theme::color32(Rgba { a: 255, ..solid }));
}

/// The hue strip in its well.
fn hue_strip(rux: &mut Rux<'_>, well: Rect, state: &mut State) {
    let chat = rux.theme().chat;
    rux.chrome.surface(well, well.height() / 2.0, chat.well, chat.carved_sm());
    let strip = well.shrink(WELL_PAD);
    if let Some(at) = dragged_to(rux, strip, "hue", "Hue") {
        state.hue = at.x.min(0.9999);
        state.fill_the_fields(None);
    }
    strip_mesh(rux.painter(), strip, |t| crate::theme::color32(hsv_to_rgb(t, 1.0, 1.0, 255)));
    let knob = Pos2::new(strip.left() + strip.width() * state.hue, strip.center().y);
    paint_a_knob(rux.painter(), knob, Color32::TRANSPARENT);
}

/// The alpha strip in its well, over a checkerboard so a translucent end reads as translucent.
fn alpha_strip(rux: &mut Rux<'_>, well: Rect, state: &mut State) {
    let chat = rux.theme().chat;
    rux.chrome.surface(well, well.height() / 2.0, chat.well, chat.carved_sm());
    let strip = well.shrink(WELL_PAD);
    if let Some(at) = dragged_to(rux, strip, "alpha", "Opacity") {
        state.alpha = (at.x * 255.0).round() as u8;
        state.fill_the_fields(None);
    }
    checkerboard(rux.painter(), strip);
    let solid = state.colour();
    strip_mesh(rux.painter(), strip, |t| {
        crate::theme::color32(Rgba { a: (t * 255.0).round() as u8, ..solid })
    });
    let knob =
        Pos2::new(strip.left() + strip.width() * f32::from(state.alpha) / 255.0, strip.center().y);
    paint_a_knob(rux.painter(), knob, Color32::TRANSPARENT);
}

/// A strip filled with `colour(t)` from `t = 0` at its left to `t = 1` at its right.
fn strip_mesh(painter: &egui::Painter, strip: Rect, colour: impl Fn(f32) -> Color32) {
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let x = strip.left() + strip.width() * t;
        mesh.colored_vertex(Pos2::new(x, strip.top()), colour(t));
        mesh.colored_vertex(Pos2::new(x, strip.bottom()), colour(t));
        if step > 0 {
            let base = (step as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 2, base + 3);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Squares of the theme's field and control colours, which is what a translucent colour is shown over.
fn checkerboard(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, CornerRadius::ZERO, color::field());
    let mut column = 0;
    let mut x = rect.left();
    while x < rect.right() {
        let mut row = 0;
        let mut y = rect.top();
        while y < rect.bottom() {
            if (row + column) % 2 == 0 {
                let square = Rect::from_min_max(
                    Pos2::new(x, y),
                    Pos2::new((x + CHECK).min(rect.right()), (y + CHECK).min(rect.bottom())),
                );
                painter.rect_filled(square, CornerRadius::ZERO, color::control());
            }
            row += 1;
            y += CHECK;
        }
        column += 1;
        x += CHECK;
    }
}

/// The knob on the square and the strips: a ring with a dark edge round it, filled when `fill` is not clear.
fn paint_a_knob(painter: &egui::Painter, at: Pos2, fill: Color32) {
    // any ground: the knob sits on a saturated colour, never on the theme's ground, so it is white edged in black
    let (ring, edge) = (Color32::WHITE, Color32::from_black_alpha(130));
    painter.circle(at, 7.0, fill, Stroke::new(2.0, ring));
    painter.circle_stroke(at, 8.5, Stroke::new(1.0, edge));
}

/// The five fields: the colour as `#RRGGBBAA`, and its red, green, blue and alpha as numbers.
fn fields(rux: &mut Rux<'_>, row: Rect, state: &mut State) {
    let gap = 6.0;
    let shares = [1.8_f32, 1.0, 1.0, 1.0, 1.0];
    let unit = (row.width() - gap * 4.0) / shares.iter().sum::<f32>();
    let mut left = row.left();
    let mut typed: Option<Field> = None;
    for (index, (share, field)) in shares
        .iter()
        .zip([Field::Hex, Field::Red, Field::Green, Field::Blue, Field::Alpha])
        .enumerate()
    {
        let width = share * unit;
        let well = Rect::from_min_size(Pos2::new(left, row.top()), Vec2::new(width, FIELD));
        left += width + gap;
        let (label, name) = match field {
            Field::Hex => ("HEX RGBA", "Colour as hex"),
            Field::Red => ("R", "Red"),
            Field::Green => ("G", "Green"),
            Field::Blue => ("B", "Blue"),
            Field::Alpha => ("A %", "Opacity percent"),
        };
        let chat = rux.theme().chat;
        rux.chrome.surface(well, 9.0, chat.well, chat.carved_sm());
        let text = match field {
            Field::Hex => &mut state.hex,
            Field::Red => &mut state.red,
            Field::Green => &mut state.green,
            Field::Blue => &mut state.blue,
            Field::Alpha => &mut state.percent,
        };
        let ink = rux.theme().ink;
        let font = FontId::monospace(11.5);
        let id = egui::Id::new(("realm-colour-field", index));
        let response = rux.ui.text_box_at(
            well.shrink2(Vec2::new(4.0, 0.0)),
            egui::TextEdit::singleline(text)
                .id(id)
                .font(font)
                .frame(egui::Frame::NONE)
                .horizontal_align(egui::Align::Center)
                .text_color(ink.i900)
                .desired_width(well.width() - 8.0),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, name));
        if response.changed() {
            typed = Some(field);
        }
        let style = Style::sans(9.5).tracking(0.08);
        let galley = rux.text(style, label, ink.i400);
        let width = galley.size().x;
        rux.painter().galley(
            Pos2::new(well.center().x - width / 2.0, well.bottom() + 4.0),
            galley,
            ink.i400,
        );
    }
    if let Some(field) = typed {
        state.read_a_field(field);
    }
}

/// The swatches, two rows of them. A press puts the picker on that colour.
fn swatch_rows(rux: &mut Rux<'_>, area: Rect, state: &mut State) {
    let per_row = ((area.width() + SWATCH_GAP) / (SWATCH + SWATCH_GAP)).floor().max(1.0) as usize;
    for (index, colour) in swatches().into_iter().enumerate() {
        let (row, column) = (index / per_row, index % per_row);
        let rect = Rect::from_min_size(
            Pos2::new(
                area.left() + column as f32 * (SWATCH + SWATCH_GAP),
                area.top() + row as f32 * (SWATCH + SWATCH_GAP),
            ),
            Vec2::splat(SWATCH),
        );
        let chat = rux.theme().chat;
        rux.chrome.surface(rect, 8.0, rux::Fill::gradient(chat.chip, rect), chat.raised_chip());
        if colour.a < 255 {
            checkerboard(rux.painter(), rect.shrink(2.0));
        }
        rux.painter().rect_filled(
            rect.shrink(2.0),
            CornerRadius::same(6),
            crate::theme::color32(colour),
        );
        let response =
            rux.ui.interact(rect, rux.ui.id().with(("realm-colour-swatch", index)), Sense::click());
        let named = format!("Swatch {}", colour.to_hex());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone())
        });
        if response.hovered() {
            rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            state.set(colour);
        }
    }
}

/// The colour before and the colour now, side by side over a checkerboard.
fn compare(rux: &mut Rux<'_>, bar: Rect, state: &State) {
    let chat = rux.theme().chat;
    rux.chrome.surface(bar, 9.0, chat.well, chat.carved_sm());
    let inside = bar.shrink(2.0);
    checkerboard(rux.painter(), inside);
    let (left, right) = inside.split_left_right_at_fraction(0.5);
    let card = super::card_colour();
    let was = state.before.map(crate::theme::color32).unwrap_or(card);
    rux.painter().rect_filled(left, CornerRadius::ZERO, was);
    rux.painter().rect_filled(right, CornerRadius::ZERO, crate::theme::color32(state.colour()));
    for (rect, words, fill) in
        [(left, "Before", state.before), (right, "After", Some(state.colour()))]
    {
        let ink = super::shape::ink_for(fill);
        let galley = rux.text(Style::sans(10.5), words, ink);
        rux::text::draw_centred(rux.painter(), rect, galley, ink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_reads_back_the_colour_it_was_put_on() {
        let mut state = State::new(1, Some(Rgba::new(255, 192, 98, 210)), Pos2::ZERO);
        assert_eq!(state.colour(), Rgba::new(255, 192, 98, 210));
        assert_eq!(state.hex, "#FFC062D2");
        assert_eq!(state.percent, "82");
        state.set(Rgba::new(61, 120, 255, 80));
        assert_eq!(state.colour(), Rgba::new(61, 120, 255, 80));
    }

    #[test]
    fn a_field_that_reads_as_a_colour_moves_the_picker_and_one_that_does_not_leaves_it() {
        let mut state = State::new(1, Some(Rgba::new(10, 20, 30, 255)), Pos2::ZERO);
        state.hex = "#3ad6ad80".to_owned();
        state.read_a_field(Field::Hex);
        assert_eq!(state.colour(), Rgba::new(0x3a, 0xd6, 0xad, 0x80));
        assert_eq!(state.hex, "#3ad6ad80", "the field being typed in keeps what was typed");
        assert_eq!(state.red, "58", "and the others follow");
        state.red = "not a number".to_owned();
        state.read_a_field(Field::Red);
        assert_eq!(state.colour(), Rgba::new(0x3a, 0xd6, 0xad, 0x80));
        state.percent = "100".to_owned();
        state.read_a_field(Field::Alpha);
        assert_eq!(state.colour().a, 255);
    }

    #[test]
    fn a_shape_that_follows_the_theme_opens_the_picker_on_the_card() {
        let state = State::new(1, None, Pos2::ZERO);
        let card = super::super::card_colour();
        let colour = state.colour();
        assert!((i16::from(colour.r) - i16::from(card.r())).abs() <= 1);
        assert_eq!(colour.a, 255);
        assert_eq!(state.before, None, "Cancel puts it back to following the theme");
    }

    #[test]
    fn there_are_two_rows_of_swatches_and_no_theme_button() {
        let count = swatches().len();
        assert_eq!(count, 14);
        let per_row = ((WIDTH - PAD * 2.0 + SWATCH_GAP) / (SWATCH + SWATCH_GAP)).floor() as usize;
        assert!(count.div_ceil(per_row) <= 2, "{per_row} a row");
    }
}
