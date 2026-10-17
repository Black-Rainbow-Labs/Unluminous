//! The parts every page of the Settings window is built from, in the Settings design's sizes.
//!
//! `task-2246`. The Claude Design canvas "Unluminous", page Settings (`SettingsModal.dc.html`), redraws the
//! window in the language the Agent-Chat and Agent-Tasks panes already use: every group of settings is a
//! raised card on the dialog's ground, a value chosen from a list is a raised chip with a chevron, a value
//! that is typed is a carved well, an on and off setting is a switch, and a choice between a few words is a
//! carved track with the chosen word raised out of it.
//!
//! The pages are Unluminous's own six and the three a plugin contributes, so the parts are here rather than
//! in `settings_dialog`: a plugin page draws its cards with exactly the code the window's own pages use.
//!
//! ## Two layers a card, and why
//!
//! A card's decoration is a `rux` surface, and `rux` rasterises a layer's decoration into a slot reserved
//! **before** anything in that layer is drawn. The card's height is only known once its rows are drawn, so a
//! card is two layers: the outer one, whose slot is reserved first and which records the card's surface once
//! its height is known, and the inner one, which records the wells, chips and switches of the rows. The inner
//! slot is reserved after the outer one, so the rows' decoration is painted over the card, and the words are
//! painted with egui after both.
//!
//! ## One type size, whatever the editor is
//!
//! Every size here is the design's, in points, as the ticket modal is. A page that followed the interface
//! font would set `Family` at 24 points on the machine the reports come from, beside a 38 point chip.

use std::hash::Hash;

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use rux::components::{Select, SelectLead, SelectState};
use rux::icon::{Icon, Mark};
use rux::{Elevation, Fill, Rux, Shadow, Style};

use crate::components::controls;
use crate::components::controls::TextBoxes;

/// The room between two cards.
pub const CARD_GAP: f32 = 18.0;
/// How round a card is, and the room inside its edge: `padding: 18px 20px 20px; border-radius: 24px`.
pub const CARD_RADIUS: f32 = 24.0;
const CARD_TOP: f32 = 18.0;
const CARD_SIDE: f32 = 20.0;
const CARD_BOTTOM: f32 = 20.0;
/// Between a card's title and its help, and between the heading and the first row.
const TITLE_GAP: f32 = 5.0;
const HEAD_GAP: f32 = 16.0;
/// Between one row and the next.
pub const ROW_GAP: f32 = 16.0;
/// The label column and the gap after it: `grid-template-columns: 150px 1fr; gap: 14px`.
pub const LABEL_COLUMN: f32 = 150.0;
const COLUMN_GAP: f32 = 14.0;
/// How tall a chip, a well or a track is.
pub const CONTROL: f32 = 38.0;
/// Between a control and the help under it, and between the inline things in a row.
const HELP_GAP: f32 = 7.0;
const INLINE_GAP: f32 = 10.0;
/// How far a card's shadow reaches past it, so the layer is made large enough to hold it.
pub const CARD_REACH: f32 = 30.0;
/// How tall a text area starts, and how short and how tall dragging its bottom edge can make it.
const AREA_HEIGHT: f32 = 96.0;
const AREA_SHORTEST: f32 = 56.0;
const AREA_TALLEST: f32 = 480.0;

/// The design's type: a card's title, a label, help, what is in a control, and code.
pub fn title_style() -> Style {
    Style::sans(13.0).semibold()
}
pub fn label_style() -> Style {
    Style::sans(12.5)
}
pub fn help_style() -> Style {
    Style::sans(11.5).leading(1.55)
}
pub fn value_style(mono: bool) -> Style {
    match mono {
        true => Style::mono(12.0),
        false => Style::sans(12.5),
    }
}

/// The window's `rux` state, made in step with the theme.
fn state(ctx: &egui::Context) -> std::rc::Rc<rux::RuxState> {
    let state = crate::components::chrome_kit::state_for_the_window(ctx);
    crate::theme::in_step(&state);
    state.set_still(true);
    state
}

/// Draw with `rux` into `rect`, with the window's state, for a part of the Settings window that is not a card.
pub fn layer<R>(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    rect: Rect,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let state = state(ui.ctx());
    let id = ui.id().with(("settings-kit-layer", salt));
    rux::layer(ui, &state, id, rect.expand(CARD_REACH), add)
}

/// [`layer`] with a canvas that reaches `reach` past the rectangle, for a surface whose shadow is longer
/// than a card's, such as a dialog's own.
pub fn layer_reaching<R>(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    rect: Rect,
    reach: f32,
    add: impl FnOnce(&mut Rux<'_>) -> R,
) -> R {
    let state = state(ui.ctx());
    let id = ui.id().with(("settings-kit-layer", salt));
    rux::layer(ui, &state, id, rect.expand(reach), add)
}

/// A column of cards: where it is across, and how far down the next card starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub left: f32,
    pub right: f32,
    pub pen: f32,
}

impl Column {
    pub fn new(left: f32, right: f32, top: f32) -> Self {
        Self { left, right, pen: top }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }
}

/// One card: a title, the help under it when there is any, and whatever rows `add` draws. Moves the column's
/// pen past the card and the gap under it.
pub fn card<R>(
    ui: &mut egui::Ui,
    column: &mut Column,
    title: &str,
    help: Option<&str>,
    add: impl FnOnce(&mut Card<'_, '_>) -> R,
) -> R {
    let state = state(ui.ctx());
    let top = column.pen;
    let outer =
        Rect::from_min_max(Pos2::new(column.left, top), Pos2::new(column.right, top + 10_000.0));
    let outer_id = ui.id().with(("settings-card", title));
    let inner_id = ui.id().with(("settings-card-rows", title));
    let (result, bottom) =
        rux::layer(ui, &state, outer_id, outer.expand(CARD_REACH), |outer_rux| {
            let state = outer_rux.state;
            let (result, bottom) =
                rux::layer(&mut *outer_rux.ui, state, inner_id, outer.expand(CARD_REACH), |rux| {
                    let mut card = Card {
                        rux,
                        left: column.left + CARD_SIDE,
                        right: column.right - CARD_SIDE,
                        pen: top + CARD_TOP,
                        rows: 0,
                    };
                    card.heading(title, help);
                    let result = add(&mut card);
                    (result, card.pen + CARD_BOTTOM)
                });
            let chat = outer_rux.theme().chat;
            let rect =
                Rect::from_min_max(Pos2::new(column.left, top), Pos2::new(column.right, bottom));
            outer_rux.chrome.surface(
                rect,
                CARD_RADIUS,
                Fill::gradient(chat.card, rect),
                chat.raised(),
            );
            (result, bottom)
        });
    // A card is named after its title, so a test can find which card a setting is on and a person reading the
    // window out loud hears the group before its settings.
    let rect = Rect::from_min_max(Pos2::new(column.left, top), Pos2::new(column.right, bottom));
    let response = ui.interact(rect, ui.id().with(("settings-card-name", title)), Sense::hover());
    if !title.is_empty() {
        let named = format!("{title} settings");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, named.clone())
        });
    }
    column.pen = bottom + CARD_GAP;
    result
}

/// What a card draws its rows with.
pub struct Card<'r, 'u> {
    pub rux: &'r mut Rux<'u>,
    pub left: f32,
    pub right: f32,
    pub pen: f32,
    rows: usize,
}

impl<'r, 'u> Card<'r, 'u> {
    fn heading(&mut self, title: &str, help: Option<&str>) {
        // A card with no title of its own draws its own heading, so its first row starts at the top.
        if title.is_empty() {
            self.pen -= HEAD_GAP;
            return;
        }
        let theme = self.rux.theme();
        let galley = self.rux.text(title_style(), title, theme.ink.i900);
        let height = galley.size().y;
        self.rux.painter().galley(Pos2::new(self.left, self.pen), galley, theme.ink.i900);
        self.pen += height;
        if let Some(help) = help.filter(|help| !help.is_empty()) {
            self.pen += TITLE_GAP;
            self.pen = paragraph(self.rux, Pos2::new(self.left, self.pen), self.width(), help);
        }
    }

    /// How wide the inside of the card is.
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    /// Where the control column starts.
    pub fn controls_left(&self) -> f32 {
        self.left + LABEL_COLUMN + COLUMN_GAP
    }

    /// The gap before a row, which the first row has after the heading instead.
    fn open_row(&mut self) {
        self.pen += match self.rows {
            0 => HEAD_GAP,
            _ => ROW_GAP,
        };
        self.rows += 1;
    }

    /// A row with a label in the left column and controls in the right, and the help under both, from the
    /// card's margin.
    pub fn row<R>(
        &mut self,
        label: &str,
        help: Option<&str>,
        add: impl FnOnce(&mut Row<'_, 'u>) -> R,
    ) -> R {
        self.open_row();
        let theme = self.rux.theme();
        let top = self.pen;
        if !label.is_empty() {
            let galley = self.rux.text(label_style(), label, theme.ink.i500);
            rux::text::draw_left_capitals(
                self.rux.painter(),
                Pos2::new(self.left, top + CONTROL / 2.0),
                galley,
                label_style(),
                theme.ink.i500,
            );
        }
        let left = self.controls_left();
        let right = self.right;
        let mut row = Row { rux: &mut *self.rux, x: left, left, right, top, bottom: top + CONTROL };
        let result = add(&mut row);
        let mut bottom = row.bottom;
        // A row's help starts at the card's own margin, where the label and a switch start, rather than
        // under the controls (`task-2282`): every paragraph on a card lines up on one edge.
        if let Some(help) = help.filter(|help| !help.is_empty()) {
            let margin = self.left;
            bottom =
                paragraph(self.rux, Pos2::new(margin, bottom + HELP_GAP), right - margin, help);
        }
        self.pen = bottom;
        result
    }

    /// A text box several lines tall, with a label in the left column and the help under it, that is made
    /// taller or shorter by dragging its bottom edge (`task-2247`). Answers whether the words changed.
    ///
    /// How tall it was dragged to is kept in egui's memory under the box's name, which is where a modal's
    /// placement is kept: it is about how this window is being used rather than anything to write down.
    pub fn text_area(
        &mut self,
        label: &str,
        help: Option<&str>,
        value: &mut String,
        hint: &str,
    ) -> bool {
        self.open_row();
        let theme = self.rux.theme();
        let top = self.pen;
        let galley = self.rux.text(label_style(), label, theme.ink.i500);
        rux::text::draw_left_capitals(
            self.rux.painter(),
            Pos2::new(self.left, top + CONTROL / 2.0),
            galley,
            label_style(),
            theme.ink.i500,
        );
        let left = self.controls_left();
        let held = self.rux.ui.id().with(("settings-area-height", label));
        let height =
            self.rux.ui.memory(|memory| memory.data.get_temp::<f32>(held)).unwrap_or(AREA_HEIGHT);
        let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(self.right - left, height));
        let changed = text_area(self.rux, rect, label, value, hint).changed();
        if let Some(taller) = area_grip(self.rux, rect, label) {
            let height = (height + taller).clamp(AREA_SHORTEST, AREA_TALLEST);
            self.rux.ui.memory_mut(|memory| memory.data.insert_temp(held, height));
        }
        let mut bottom = rect.bottom();
        if let Some(help) = help.filter(|help| !help.is_empty()) {
            bottom =
                paragraph(self.rux, Pos2::new(self.left, bottom + HELP_GAP), self.width(), help);
        }
        self.pen = bottom;
        changed
    }

    /// A switch with its label beside it and the help under both, from the card's margin.
    pub fn switch(&mut self, label: &str, help: Option<&str>, value: &mut bool) -> bool {
        self.open_row();
        let top = self.pen;
        let theme = self.rux.theme();
        let track = Rect::from_min_size(Pos2::new(self.left, top + 1.0), SWITCH);
        let words_left = track.right() + 14.0;
        let galley = self.rux.text(label_style(), label, theme.ink.i900);
        let label_rect =
            Rect::from_min_size(Pos2::new(words_left, top), Vec2::new(galley.size().x, 26.0));
        rux::text::draw_left_capitals(
            self.rux.painter(),
            Pos2::new(words_left, top + 13.0),
            galley,
            label_style(),
            theme.ink.i900,
        );
        let changed = switch(self.rux, track, label, value, label_rect);
        let mut bottom = top + 26.0;
        if let Some(help) = help.filter(|help| !help.is_empty()) {
            bottom =
                paragraph(self.rux, Pos2::new(self.left, bottom + TITLE_GAP), self.width(), help);
        }
        self.pen = bottom;
        changed
    }

    /// A paragraph of help on its own, across the whole card.
    pub fn help(&mut self, said: &str) {
        self.open_row();
        self.pen = paragraph(self.rux, Pos2::new(self.left, self.pen), self.width(), said);
    }

    /// Something the caller draws itself, `height` tall, across the whole card.
    pub fn custom<R>(&mut self, height: f32, add: impl FnOnce(&mut Rux<'u>, Rect) -> R) -> R {
        self.open_row();
        let rect =
            Rect::from_min_size(Pos2::new(self.left, self.pen), Vec2::new(self.width(), height));
        let result = add(self.rux, rect);
        self.pen = rect.bottom();
        result
    }

    /// Something the caller draws itself across the whole card, which answers how far down it reached.
    pub fn grow<R>(&mut self, add: impl FnOnce(&mut Rux<'u>, Column) -> (R, f32)) -> R {
        self.open_row();
        let column = Column::new(self.left, self.right, self.pen);
        let (result, bottom) = add(self.rux, column);
        self.pen = bottom;
        result
    }
}

/// The right hand side of one row: controls laid out left to right, and blocks under them.
pub struct Row<'a, 'u> {
    pub rux: &'a mut Rux<'u>,
    /// Where the next inline control goes.
    pub x: f32,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    /// How far down the row reaches so far.
    pub bottom: f32,
}

impl<'a, 'u> Row<'a, 'u> {
    /// The next `width` points of the row, `height` tall and centred on the first line.
    fn take(&mut self, width: f32, height: f32) -> Rect {
        let width = width.min(self.right - self.x).max(0.0);
        let rect = Rect::from_min_size(
            Pos2::new(self.x, self.top + (CONTROL - height) / 2.0),
            Vec2::new(width, height),
        );
        self.x = rect.right() + INLINE_GAP;
        rect
    }

    /// A value chosen from a list, as the design's chip with a chevron. Answers the index chosen.
    pub fn select(
        &mut self,
        name: &str,
        options: &[String],
        selected: Option<usize>,
        width: f32,
    ) -> Option<usize> {
        let rect = self.take(width, CONTROL);
        select(self.rux, rect, name, options, selected)
    }

    /// A value that is chosen somewhere else, drawn as a select's chip: pressing it opens whatever chooses it,
    /// such as the background grid. Answers whether it was pressed.
    pub fn chooser(&mut self, name: &str, value: &str, width: f32) -> bool {
        let rect = self.take(width, CONTROL);
        chooser(self.rux, rect, name, value)
    }

    /// A value that is typed, in a carved well.
    pub fn text(&mut self, name: &str, value: &mut String, hint: &str, width: f32) -> bool {
        let rect = self.take(width, CONTROL);
        text_well(self.rux, rect, name, value, hint, false, false).changed()
    }

    /// The same, set in the code face.
    pub fn code_text(&mut self, name: &str, value: &mut String, hint: &str, width: f32) -> bool {
        let rect = self.take(width, CONTROL);
        text_well(self.rux, rect, name, value, hint, true, false).changed()
    }

    /// A secret, typed and never shown.
    pub fn password(&mut self, name: &str, value: &mut String, hint: &str, width: f32) -> bool {
        let rect = self.take(width, CONTROL);
        text_well(self.rux, rect, name, value, hint, false, true).changed()
    }

    /// A value that is read rather than typed, in the code face in a carved well.
    pub fn path(&mut self, name: &str, value: &str, width: f32) {
        let rect = self.take(width, CONTROL);
        path_well(self.rux, rect, name, value);
    }

    /// A choice between a few words, as a carved track with the chosen one raised.
    pub fn segmented(
        &mut self,
        name: &str,
        options: &[&str],
        chosen: Option<usize>,
        least: f32,
        mono: bool,
    ) -> Option<usize> {
        let width = segmented_width(self.rux, options, least, mono, SEGMENT_PAD);
        let rect = self.take(width, SEGMENT_TRACK);
        segmented(self.rux, rect, name, options, chosen, least, mono)
    }

    /// A button on the chip surface. Answers whether it was pressed.
    pub fn button(&mut self, label: &str) -> bool {
        let width = button_width(self.rux, label, false);
        let rect = self.take(width, BUTTON);
        button(self.rux, rect, label, Kind::Chip)
    }

    /// A button with no surface, for the quieter of two.
    pub fn ghost(&mut self, label: &str) -> bool {
        let width = button_width(self.rux, label, false);
        let rect = self.take(width, BUTTON);
        button(self.rux, rect, label, Kind::Ghost)
    }

    /// A fraction between `low` and `high`, as a carved track with a raised knob. Answers whether it moved.
    pub fn slider(&mut self, name: &str, value: &mut f32, low: f32, high: f32) -> bool {
        let rect = self.take(280.0, 24.0);
        slider(self.rux, rect, name, value, low, high)
    }

    /// Words after the controls, in the quieter ink.
    pub fn suffix(&mut self, said: &str) {
        let theme = self.rux.theme();
        let galley = self.rux.text(Style::sans(12.0), said, theme.ink.i500);
        let width = galley.size().x;
        let at = Pos2::new(self.x, self.top + CONTROL / 2.0);
        rux::text::draw_left_capitals(
            self.rux.painter(),
            at,
            galley,
            Style::sans(12.0),
            theme.ink.i500,
        );
        self.x += width + INLINE_GAP;
    }

    /// A state read rather than set: a carved disc with a dash, and the words beside it.
    pub fn status(&mut self, said: &str) {
        let theme = self.rux.theme();
        let chat = theme.chat;
        let disc = Rect::from_center_size(
            Pos2::new(self.x + 11.0, self.top + CONTROL / 2.0),
            Vec2::splat(22.0),
        );
        self.rux.chrome.surface(disc, 11.0, chat.well, chat.carved_sm());
        self.rux.painter().line_segment(
            [disc.center() - Vec2::new(2.5, 0.0), disc.center() + Vec2::new(2.5, 0.0)],
            egui::Stroke::new(1.7, theme.ink.i400),
        );
        let left = disc.right() + 10.0;
        let galley = self.rux.elided(Style::sans(12.0), said, theme.ink.i500, self.right - left);
        rux::text::draw_left_capitals(
            self.rux.painter(),
            Pos2::new(left, disc.center().y),
            galley,
            Style::sans(12.0),
            theme.ink.i500,
        );
        self.x = self.right;
    }

    /// A block under the controls, `height` tall, across the control column.
    pub fn below<R>(&mut self, height: f32, add: impl FnOnce(&mut Rux<'u>, Rect) -> R) -> R {
        let rect = Rect::from_min_size(
            Pos2::new(self.left, self.bottom + 12.0),
            Vec2::new(self.right - self.left, height),
        );
        self.bottom = rect.bottom();
        add(self.rux, rect)
    }
}

/// Lay a paragraph of help out at `at`, wrapped to `width`, and answer where it ends.
pub fn paragraph(rux: &mut Rux<'_>, at: Pos2, width: f32, said: &str) -> f32 {
    let theme = rux.theme();
    let galley =
        rux::text::wrapped(rux.painter(), help_style(), said, theme.ink.i400, width.max(40.0));
    let height = galley.size().y;
    rux.painter().galley(at, galley, theme.ink.i400);
    at.y + height
}

/// `width: 44px; height: 24px`.
pub const SWITCH: Vec2 = Vec2::new(44.0, 24.0);

/// A switch: a carved track with a knob at the left, or the action's gradient with the knob at the right.
///
/// Drawn here rather than with `rux::components::Switch`, whose label is a `&'static str` and whose off
/// track is the page's ground rather than the design's well. `label_rect` is the words beside it, which a
/// press on also flips it, the way a `<label>` does.
pub fn switch(
    rux: &mut Rux<'_>,
    track: Rect,
    name: &str,
    value: &mut bool,
    label_rect: Rect,
) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let id = rux.ui.id().with(("settings-switch", name));
    let response = rux.ui.interact(track.union(label_rect), id, Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut changed = false;
    if response.clicked() {
        *value = !*value;
        changed = true;
    }
    let radius = track.height() / 2.0;
    match *value {
        true => rux.chrome.surface(
            track,
            radius,
            Fill::gradient(chat.action, track),
            Elevation::of([Shadow::inset(1.0, 1.0, 3.0, chat.dark.gamma_multiply(0.6))]),
        ),
        false => rux.chrome.surface(track, radius, chat.well, chat.carved_sm()),
    }
    let knob = Rect::from_center_size(
        Pos2::new(
            match *value {
                true => track.right() - 2.0 - 10.0,
                false => track.left() + 2.0 + 10.0,
            },
            track.center().y,
        ),
        Vec2::splat(20.0),
    );
    let knob_fill = match *value {
        true => Fill::Solid(theme.ink.i900),
        false => Fill::gradient(chat.chip, knob),
    };
    rux.chrome.surface(
        knob,
        10.0,
        knob_fill,
        Elevation::of([Shadow::outer(1.0, 1.0, 3.0, chat.dark)]),
    );
    let on = *value;
    let named = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, on, named.clone())
    });
    changed
}

/// A list to choose from, as the design's chip: `rux`'s own select in its chip form, opened under it.
///
/// The open state is kept in egui's memory under the select's name, so a page needs nothing of its own to
/// hold it.
pub fn select(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    options: &[String],
    selected: Option<usize>,
) -> Option<usize> {
    let key = rux.ui.id().with(("settings-select", name));
    let mut state: SelectState = rux.ui.ctx().data(|data| data.get_temp(key)).unwrap_or_default();
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(key));
    let outcome = {
        let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
        Select::new(options, selected)
            .chip(SelectLead::None)
            .label(name)
            .show(&mut inner, rect, &mut state)
    };
    if outcome.chosen.is_some() {
        state.open = false;
    }
    rux.ui.ctx().data_mut(|data| data.insert_temp(key, state));
    outcome.chosen.filter(|chosen| Some(*chosen) != selected)
}

/// A chip that says what is chosen and opens something else to choose it: the select's look with no menu.
pub fn chooser(rux: &mut Rux<'_>, rect: Rect, name: &str, value: &str) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("settings-chooser", name)), Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let elevation = match response.is_pointer_button_down_on() {
        true => chat.carved_sm(),
        false => chat.raised_chip(),
    };
    rux.chrome.surface(rect, rect.height() / 2.0, Fill::gradient(chat.chip, rect), elevation);
    let chevron = Pos2::new(rect.right() - 14.0 - 5.0, rect.center().y);
    rux.mark(Mark::new(Icon::ChevDown, 10.0).stroke(1.6 * 2.4), chevron, theme.ink.i400);
    let galley =
        rux.elided(value_style(false), value, theme.ink.i900, rect.width() - 16.0 - 14.0 - 20.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 16.0, rect.center().y),
        galley,
        value_style(false),
        theme.ink.i900,
    );
    let named = name.to_owned();
    let said = value.to_owned();
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone());
        info.current_text_value = Some(said.clone());
        info
    });
    response.clicked()
}

/// A typed value in a carved well: `height: 38px; padding: 0 16px; border-radius: 999px`.
pub fn text_well(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    value: &mut String,
    hint: &str,
    mono: bool,
    password: bool,
) -> egui::Response {
    text_well_styled(rux, rect, name, value, hint, value_style(mono), password)
}

/// The same, with the words set in `style`: an endpoint's name is semibold.
pub fn text_well_styled(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    value: &mut String,
    hint: &str,
    style: Style,
    password: bool,
) -> egui::Response {
    let theme = rux.theme();
    let chat = theme.chat;
    rux.chrome.surface(rect, rect.height() / 2.0, chat.well, chat.carved_sm());
    let font = style.font();
    let id = rux.ui.id().with(("settings-field", name));
    let inset = (rect.height() * 0.42).min(16.0);
    let text = controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        rect,
        inset,
        id,
        &format!("{name} field"),
        &font,
    );
    let text = Rect::from_min_max(text.min, Pos2::new(rect.right() - inset, text.max.y));
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = child.text_box(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(controls::placeholder(hint, &font, theme.ink.i400))
            .font(font.clone())
            .frame(egui::Frame::NONE)
            .password(password)
            .desired_width(text.width())
            .text_color(theme.ink.i900),
    );
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, named.clone()));
    response
}

/// Several lines of words typed into a carved well. See [`Card::text_area`].
pub fn text_area(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    let theme = rux.theme();
    let chat = theme.chat;
    rux.chrome.surface(rect, 16.0, chat.well, chat.carved_sm());
    let font = value_style(false).font();
    let id = rux.ui.id().with(("settings-area", name));
    let inside = rect.shrink2(Vec2::new(14.0, 10.0));
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(inside));
    child.set_clip_rect(child.clip_rect().intersect(inside));
    let response = egui::ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .max_height(inside.height())
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            ui.text_box(
                egui::TextEdit::multiline(value)
                    .id(id)
                    .hint_text(controls::placeholder(hint, &font, theme.ink.i400))
                    .font(font.clone())
                    .frame(egui::Frame::NONE)
                    .desired_width(inside.width())
                    .desired_rows(1)
                    .text_color(theme.ink.i900),
            )
        })
        .inner;
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, named.clone()));
    response
}

/// The strip along a text area's bottom edge that is dragged to make it taller, with a small mark at its
/// right hand corner. Answers how far it was dragged this frame.
fn area_grip(rux: &mut Rux<'_>, rect: Rect, name: &str) -> Option<f32> {
    let grip = Rect::from_min_max(
        Pos2::new(rect.left() + 16.0, rect.bottom() - 5.0),
        Pos2::new(rect.right(), rect.bottom() + 5.0),
    );
    let response =
        rux.ui.interact(grip, rux.ui.id().with(("settings-area-grip", name)), Sense::drag());
    let label = format!("Resize {name}");
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, label.clone()));
    if response.hovered() || response.dragged() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let ink = rux.theme().ink.i400;
    let corner = Pos2::new(rect.right() - 9.0, rect.bottom() - 9.0);
    for step in [4.0, 8.0] {
        rux.painter().line_segment(
            [corner + Vec2::new(-step, 4.0), corner + Vec2::new(4.0, -step)],
            egui::Stroke::new(1.2, ink),
        );
    }
    response.dragged().then(|| response.drag_delta().y)
}

/// A value read rather than typed, in the code face, in a carved well.
pub fn path_well(rux: &mut Rux<'_>, rect: Rect, name: &str, value: &str) {
    let theme = rux.theme();
    let chat = theme.chat;
    rux.chrome.surface(rect, rect.height() / 2.0, chat.well, chat.carved_sm());
    let galley = rux.elided(Style::mono(12.0), value, theme.ink.i500, rect.width() - 32.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 16.0, rect.center().y),
        galley,
        Style::mono(12.0),
        theme.ink.i500,
    );
    let response = rux.ui.interact(rect, rux.ui.id().with(("settings-path", name)), Sense::hover());
    let named = name.to_owned();
    let said = value.to_owned();
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Label, true, named.clone());
        info.current_text_value = Some(said.clone());
        info
    });
}

/// How tall a segmented track is, and the room round its options: `padding: 4px; gap: 3px`, 30 point options.
pub const SEGMENT_TRACK: f32 = 38.0;
const SEGMENT_OPTION: f32 = 30.0;
const SEGMENT_PAD: f32 = 12.0;

/// How wide a segmented track is for these words.
pub fn segmented_width(rux: &Rux<'_>, options: &[&str], least: f32, mono: bool, pad: f32) -> f32 {
    let style = segment_style(mono);
    let inside: f32 =
        options.iter().map(|option| (rux.measure(style, option).x + pad * 2.0).max(least)).sum();
    inside + 3.0 * (options.len().saturating_sub(1)) as f32 + 8.0
}

fn segment_style(mono: bool) -> Style {
    match mono {
        true => Style::mono(12.0),
        false => Style::sans(12.0),
    }
}

/// A choice between a few words: a carved track, the chosen word raised out of it. Answers a new choice.
pub fn segmented(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    options: &[&str],
    chosen: Option<usize>,
    least: f32,
    mono: bool,
) -> Option<usize> {
    segmented_sized(rux, rect, name, options, chosen, least, mono, SEGMENT_OPTION, SEGMENT_PAD)
}

/// The same, with the option's height and side padding named: an endpoint's wire shapes are 26 points.
#[allow(clippy::too_many_arguments)]
pub fn segmented_sized(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    options: &[&str],
    chosen: Option<usize>,
    least: f32,
    mono: bool,
    option_height: f32,
    pad: f32,
) -> Option<usize> {
    let theme = rux.theme();
    let chat = theme.chat;
    let style = segment_style(mono).at(match option_height < SEGMENT_OPTION {
        true => 11.0,
        false => 12.0,
    });
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), option_height + 8.0));
    rux.chrome.surface(track, track.height() / 2.0, chat.well, chat.carved_sm());
    let mut x = track.left() + 4.0;
    let mut picked = None;
    for (index, option) in options.iter().enumerate() {
        let width = (rux.measure(style, option).x + pad * 2.0).max(least);
        let pill = Rect::from_min_size(
            Pos2::new(x, track.center().y - option_height / 2.0),
            Vec2::new(width, option_height),
        );
        let on = chosen == Some(index);
        let response = rux.ui.interact(
            pill,
            rux.ui.id().with(("settings-segment", name, index)),
            Sense::click(),
        );
        if on {
            rux.chrome.surface(
                pill,
                option_height / 2.0,
                Fill::gradient(chat.chip, pill),
                chat.raised_chip(),
            );
        } else if response.hovered() {
            rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let ink = match (on, mono && option_height < SEGMENT_OPTION) {
            (true, _) => theme.ink.i900,
            (false, true) => theme.ink.i400,
            (false, false) => theme.ink.i500,
        };
        let galley = rux.text(style, option, ink);
        rux::text::draw_centred(rux.painter(), pill, galley, ink);
        let said = (*option).to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, said.clone())
        });
        if response.clicked() && !on {
            picked = Some(index);
        }
        x = pill.right() + 3.0;
    }
    let response =
        rux.ui.interact(track, rux.ui.id().with(("settings-segments", name)), Sense::hover());
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, named.clone()));
    picked
}

/// How tall a button in a row is: `height: 34px`.
pub const BUTTON: f32 = 34.0;

/// How a button is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// On the chip surface, with the board's small raised shadow.
    Chip,
    /// No surface at all, in the quieter ink.
    Ghost,
}

/// How wide a button is for its word: `padding: 0 15px`, or `0 16px 0 13px` with a plus in front.
pub fn button_width(rux: &Rux<'_>, label: &str, plus: bool) -> f32 {
    let words = rux.measure(Style::sans(12.0).medium(), label).x;
    match plus {
        true => words + 16.0 + 13.0 + 12.0 + 8.0,
        false => words + 30.0,
    }
}

/// A button with a word on it. Answers whether it was pressed.
pub fn button(rux: &mut Rux<'_>, rect: Rect, label: &str, kind: Kind) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let response = rux.ui.interact(
        rect,
        rux.ui.id().with(("settings-button", label, rect.min.x.to_bits(), rect.min.y.to_bits())),
        Sense::click(),
    );
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let ink = match kind {
        Kind::Chip => {
            let elevation = match response.is_pointer_button_down_on() {
                true => chat.carved_sm(),
                false => chat.raised_chip(),
            };
            rux.chrome.surface(
                rect,
                rect.height() / 2.0,
                Fill::gradient(chat.chip, rect),
                elevation,
            );
            match response.hovered() {
                true => theme.ink.i900,
                false => theme.ink.i700,
            }
        }
        Kind::Ghost => match response.hovered() {
            true => theme.ink.i700,
            false => theme.ink.i500,
        },
    };
    // Words that would reach the button's edge end with `…` a little inside it, rather than running out
    // of both of its sides (`task-2282`). Callers often hand a button less than `button_width`'s full
    // padding and their words fit, so only the edge itself is the limit.
    let room = (rect.width() - 12.0).max(0.0);
    let mut galley = rux.text(Style::sans(12.0).medium(), label, ink);
    if galley.size().x > room + 1.0 {
        galley = rux.elided(Style::sans(12.0).medium(), label, ink, room);
    }
    rux::text::draw_centred(rux.painter(), rect, galley, ink);
    let named = label.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    response.clicked()
}

/// `Add endpoint` and `New data source`: a 36 point chip with a plus in front of the words.
pub fn add_button(rux: &mut Rux<'_>, at: Pos2, label: &str) -> (bool, Rect) {
    let theme = rux.theme();
    let chat = theme.chat;
    let words = rux.measure(Style::sans(12.5).medium(), label).x;
    let rect = Rect::from_min_size(at, Vec2::new(13.0 + 12.0 + 8.0 + words + 16.0, 36.0));
    let response = rux.ui.interact(rect, rux.ui.id().with(("settings-add", label)), Sense::click());
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let elevation = match response.is_pointer_button_down_on() {
        true => chat.carved_sm(),
        false => chat.raised_chip(),
    };
    rux.chrome.surface(rect, 18.0, Fill::gradient(chat.chip, rect), elevation);
    let ink = match response.hovered() {
        true => theme.ink.i900,
        false => theme.ink.i700,
    };
    rux.mark(
        Mark::new(Icon::Plus, 12.0).stroke(1.8 * 2.0),
        Pos2::new(rect.left() + 19.0, rect.center().y),
        ink,
    );
    let galley = rux.text(Style::sans(12.5).medium(), label, ink);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(rect.left() + 33.0, rect.center().y),
        galley,
        Style::sans(12.5).medium(),
        ink,
    );
    let named = label.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone()));
    (response.clicked(), rect)
}

/// A fraction on a carved track with the action's gradient filled to it and a raised knob.
pub fn slider(
    rux: &mut Rux<'_>,
    rect: Rect,
    name: &str,
    value: &mut f32,
    low: f32,
    high: f32,
) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 12.0));
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("settings-slider", name)), Sense::click_and_drag());
    let mut changed = false;
    if let Some(at) = response.interact_pointer_pos() {
        if response.dragged() || response.clicked() || response.drag_started() {
            let fraction = ((at.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let wanted = low + fraction * (high - low);
            if (wanted - *value).abs() > f32::EPSILON {
                *value = wanted;
                changed = true;
            }
        }
    }
    if response.hovered() || response.dragged() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let fraction = ((*value - low) / (high - low)).clamp(0.0, 1.0);
    rux.chrome.surface(track, 6.0, chat.well, chat.carved_sm());
    let fill = Rect::from_min_max(
        Pos2::new(track.left() + 2.0, track.top() + 2.0),
        Pos2::new(
            (track.left() + 2.0 + (track.width() - 4.0) * fraction).max(track.left() + 6.0),
            track.bottom() - 2.0,
        ),
    );
    rux.chrome.rect(fill, 4.0, Fill::gradient(chat.action, fill));
    let knob = Rect::from_center_size(
        Pos2::new(track.left() + track.width() * fraction, track.center().y),
        Vec2::splat(24.0),
    );
    let knob = knob.translate(Vec2::new(
        (track.left() - knob.left()).max(0.0) + (track.right() - knob.right()).min(0.0),
        0.0,
    ));
    rux.chrome.surface(knob, 12.0, Fill::gradient(chat.chip, knob), chat.raised_chip());
    let shown = *value;
    let named = name.to_owned();
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, named.clone());
        info.value = Some(f64::from(shown));
        info
    });
    changed
}

/// A run of discs to choose a colour from, in a carved track. Answers the index pressed.
///
/// `None` in `colours` is the one that follows the theme, drawn hollow because it is not a colour of its own.
pub fn swatches(
    rux: &mut Rux<'_>,
    at: Pos2,
    names: &[&str],
    colours: &[(Option<Color32>, Color32)],
    chosen: Option<usize>,
) -> (Option<usize>, Rect) {
    let theme = rux.theme();
    let chat = theme.chat;
    let count = colours.len() as f32;
    let track =
        Rect::from_min_size(at, Vec2::new(24.0 + count * 24.0 + (count - 1.0) * 12.0, CONTROL));
    rux.chrome.surface(track, CONTROL / 2.0, chat.well, chat.carved_sm());
    let mut picked = None;
    for (index, (own, shown)) in colours.iter().enumerate() {
        let centre = Pos2::new(track.left() + 12.0 + 12.0 + index as f32 * 36.0, track.center().y);
        let disc = Rect::from_center_size(centre, Vec2::splat(24.0));
        let name = names.get(index).copied().unwrap_or("Color");
        let response =
            rux.ui.interact(disc, rux.ui.id().with(("settings-swatch", name)), Sense::click());
        let response = controls::WithHint::with_hint(response, name);
        let on = chosen == Some(index);
        let painter = rux.painter().clone();
        match own {
            Some(colour) => {
                painter.circle_filled(centre, 12.0, *colour);
            }
            None => {
                painter.circle_stroke(centre, 10.0, egui::Stroke::new(2.0, *shown));
            }
        }
        if on {
            painter.circle_stroke(
                centre,
                15.0,
                egui::Stroke::new(2.0, theme.ink.i900.gamma_multiply(0.7)),
            );
        } else if response.hovered() {
            painter.circle_stroke(centre, 15.0, egui::Stroke::new(1.0, theme.ink.i400));
        }
        let named = name.to_owned();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, named.clone())
        });
        if response.clicked() {
            picked = Some(index);
        }
    }
    (picked, track)
}

/// A block of code to read and copy, in a carved well: `padding: 14px 16px; border-radius: 16px`.
pub fn code_block(rux: &mut Rux<'_>, at: Pos2, width: f32, name: &str, text: &str) -> Rect {
    let theme = rux.theme();
    let chat = theme.chat;
    let style = Style::mono(11.5).leading(1.6);
    let galley = rux::text::wrapped(rux.painter(), style, text, theme.ink.i700, width - 32.0);
    let rect = Rect::from_min_size(at, Vec2::new(width, galley.size().y + 28.0));
    rux.chrome.surface(rect, 16.0, chat.well, chat.carved_sm());
    rux.painter().galley(rect.min + Vec2::new(16.0, 14.0), galley, theme.ink.i700);
    let response = rux.ui.interact(rect, rux.ui.id().with(("settings-code", name)), Sense::hover());
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, named.clone()));
    rect
}

/// A list of rows in a carved well, one of them raised: the themes, the plugins.
pub fn list_well(rux: &mut Rux<'_>, rect: Rect, radius: f32) {
    let chat = rux.theme().chat;
    rux.chrome.surface(rect, radius, chat.well, chat.carved_sm());
}

/// One row in a [`list_well`], raised when it is the chosen one. Answers its response.
pub fn list_row(
    rux: &mut Rux<'_>,
    rect: Rect,
    radius: f32,
    name: &str,
    chosen: bool,
) -> egui::Response {
    let chat = rux.theme().chat;
    let response =
        rux.ui.interact(rect, rux.ui.id().with(("settings-list-row", name)), Sense::click());
    if chosen {
        rux.chrome.surface(rect, radius, Fill::gradient(chat.chip, rect), chat.raised_chip());
    } else if response.hovered() {
        rux.wash(rect, radius, rux.theme().ink.i900.gamma_multiply(0.04));
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let named = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, named.clone())
    });
    response
}

/// A scrolling page for a plugin's own settings: the cards down a column, inside the room the dialog gives.
///
/// The dialog hands a page the rectangle the cards may be **seen** in, which reaches past the column the cards
/// are laid out in so their shadows are not cut: `padding: 24px 30px 30px 24px; margin: -24px -30px -30px -24px`.
/// `add` draws the cards into the column it is given and the page allocates what it drew, so the scrolling area
/// knows how tall the page is.
pub fn plugin_page<R>(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    add: impl FnOnce(&mut egui::Ui, &mut Column) -> R,
) -> R {
    let height = ui.available_rect_before_wrap().height();
    egui::ScrollArea::vertical()
        .id_salt(salt)
        .max_height(height)
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let mut column = Column::new(
                area.left() + PAGE_MARGIN.left,
                area.right() - PAGE_MARGIN.right,
                area.top() + PAGE_MARGIN.top,
            );
            let result = add(ui, &mut column);
            let drew = (column.pen - CARD_GAP + PAGE_MARGIN.bottom - area.top()).max(0.0);
            ui.allocate_space(Vec2::new(area.width(), drew));
            result
        })
        .inner
}

/// How far a page's visible room reaches past its column of cards, so the cards' shadows are not cut.
pub struct Margin {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// The design's `padding: 24px 30px 30px 24px` round the scrolling area, as far as the dialog's own edge lets it
/// reach: the dialog's padding is 20 points, so the room reaches 20 points past the column on the right and at
/// the bottom, and 16 above it, which keeps the top card's lit edge without running under the breadcrumb.
pub const PAGE_MARGIN: Margin = Margin { left: 24.0, top: 16.0, right: 20.0, bottom: 20.0 };
