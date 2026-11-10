//! The design tab: a toolbar, the well the page sits in, the notes at the right, and the prompt box under the
//! page.
//!
//! **The page is not drawn here.** It is the window's one native web view, which paints above everything
//! egui draws, so this draws the carved well it sits in and says where the page goes ([`page_placement`]);
//! `app::designer` puts the view there. A native child is in no screenshot Unluminous takes, so a picture of
//! this tab shows an empty well.
//!
//! Every function takes a rectangle and reports an [`Act`](super::Act); [`show`] applies them at the end.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Vec2};
use rux::icon::Icon;
use rux::Rux;

use super::{kit, notes, Act};
use crate::services::designer::page::Mode;
use crate::services::designer::{Designer, Device};
use crate::services::plugin_ui::{Look, Request};

/// How tall the toolbar is.
pub const TOOLBAR: f32 = 50.0;
/// How wide the notes are until the divider beside them is dragged.
pub const SIDEBAR: f32 = 320.0;
/// How tall the one bar over a maximized design is (`task-2601`).
pub const MAXIMISED_BAR: f32 = 40.0;
/// The width of a tab below which the notes start hidden.
pub const NARROW: f32 = 760.0;
/// How tall the prompt box is.
pub const PROMPT: f32 = 92.0;
/// The room round everything, and between the parts.
const PAD: f32 = 14.0;
const GAP: f32 = 12.0;
/// How round the well the page sits in is.
const WELL_RADIUS: f32 = 24.0;
/// The room round a page that fills its well, and the room round one drawn at a device's width.
const FIT_INSET: f32 = 12.0;
const DEVICE_INSET: f32 = 24.0;
/// The smallest a page is shrunk to when it does not fit.
const LEAST_ZOOM: f64 = 0.25;
/// A chip button in the toolbar.
const CHIP: f32 = 34.0;

/// Where each part of the tab goes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arranged {
    pub toolbar: Rect,
    /// The carved surface the page sits in.
    pub well: Rect,
    pub sidebar: Option<Rect>,
    pub prompt: Option<Rect>,
}

/// Where the toolbar, the well, the notes and the prompt box go in a tab of this size.
///
/// @param area - the tab
/// @param scale - the surface's own zoom
/// @param sidebar - whether the notes are showing
/// @param notes_width - how wide the notes are, in points at no zoom, which the divider beside them sets
pub fn arrange(area: Rect, scale: f32, sidebar: bool, notes_width: f32) -> Arranged {
    let pad = PAD * scale;
    let toolbar = Rect::from_min_size(
        Pos2::new(area.left() + pad, area.top() + 8.0 * scale),
        Vec2::new((area.width() - 2.0 * pad).max(0.0), TOOLBAR * scale),
    );
    let body = Rect::from_min_max(
        Pos2::new(area.left() + pad, toolbar.bottom() + 8.0 * scale),
        Pos2::new(area.right() - pad, (area.bottom() - pad).max(toolbar.bottom() + 8.0 * scale)),
    );
    let width = (notes_width * scale).min(body.width() * 0.7);
    let (main, side) = match sidebar && body.width() > 200.0 * scale {
        true => (
            Rect::from_min_max(
                body.min,
                Pos2::new(body.right() - width - GAP * scale, body.bottom()),
            ),
            Some(Rect::from_min_max(Pos2::new(body.right() - width, body.top()), body.max)),
        ),
        false => (body, None),
    };
    let room_for_a_prompt = main.height() > (PROMPT + 120.0) * scale;
    match room_for_a_prompt {
        true => {
            let prompt = Rect::from_min_max(
                Pos2::new(main.left(), main.bottom() - PROMPT * scale),
                main.max,
            );
            let well =
                Rect::from_min_max(main.min, Pos2::new(main.right(), prompt.top() - GAP * scale));
            Arranged { toolbar, well, sidebar: side, prompt: Some(prompt) }
        }
        false => Arranged { toolbar, well: main, sidebar: side, prompt: None },
    }
}

/// Where the page is drawn inside its well, and the zoom that makes it the width of a device.
///
/// **A page wider than the room it has is zoomed down to fit**, so the whole width is always visible. The page
/// still lays itself out at the device's width, because the engine's layout viewport is the bounds divided by
/// the zoom, which is the arithmetic a browser node on the canvas already relies on. `Fit` is the well less
/// a small inset, at no zoom.
///
/// @param well - the carved surface the page sits in
/// @param device - the width the page is laid out at
pub fn page_placement(well: Rect, device: Device) -> (Rect, f64) {
    let Some(width) = device.width() else { return (well.shrink(FIT_INSET), 1.0) };
    let room = well.shrink(DEVICE_INSET);
    let available = room.width().max(1.0);
    let (drawn, zoom) = match width <= available {
        true => (width, 1.0),
        false => {
            let fitted = (f64::from(available / width) * 100.0).floor() / 100.0;
            let zoom = fitted.max(LEAST_ZOOM);
            ((width * zoom as f32).min(available), zoom)
        }
    };
    let page = Rect::from_min_size(
        Pos2::new(room.center().x - drawn / 2.0, room.top()),
        Vec2::new(drawn, room.height().max(1.0)),
    );
    (page, zoom)
}

/// What drawing the tab came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Where the page goes, in the window's points.
    pub page: Rect,
    /// The zoom the page is drawn at, which is `1.0` unless the device is wider than the room.
    pub zoom: f64,
    pub requests: Vec<Request>,
}

/// What the toolbar needs to know, copied out of the view so the drawing holds no borrow of the provider.
struct Facts {
    name: String,
    mode: Mode,
    device: Device,
    status: String,
    notes_shown: Option<bool>,
    maximised: bool,
}

/// What was pressed in the toolbar that the tab answers itself.
#[derive(Default)]
struct Chips {
    notes: bool,
    maximise: bool,
}

/// Draw the tab for a design and apply what was pressed.
///
/// @param designer - the provider
/// @param ui - where to draw
/// @param area - the tab
/// @param look - the window's look, at the surface's own size
/// @param design - the design, relative to the project
pub fn show(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    design: &Path,
) -> Outcome {
    let scale = look.scale();
    let Some(facts) = facts_of(designer, design) else {
        return Outcome { page: area, zoom: 1.0, requests: Vec::new() };
    };
    if facts.maximised {
        return show_maximised(designer, ui, area, look, design, &facts);
    }
    let showing = facts.notes_shown.unwrap_or(area.width() >= NARROW * scale);
    let width = designer.notes_width.unwrap_or(SIDEBAR);
    let arranged = arrange(area, scale, showing, width);
    if let Some(view) = designer.design_view_mut(design) {
        view.notes_drawn = arranged.sidebar.is_some();
    }
    let (page, zoom) = page_placement(arranged.well, facts.device);
    kit::well(ui, scale, arranged.well, WELL_RADIUS * scale);
    let mut acts = Vec::new();
    let showing = arranged.sidebar.is_some();
    let pressed = toolbar(ui, look, arranged.toolbar, design, &facts, zoom, showing, &mut acts);
    if pressed.notes {
        acts.push(Act::ShowNotes(design.to_path_buf(), !showing));
    }
    if pressed.maximise {
        acts.push(Act::Maximise(design.to_path_buf(), true));
    }
    device_label(ui, scale, arranged.well, page, facts.device);
    if let Some(side) = arranged.sidebar {
        acts.extend(the_divider(
            ui,
            scale,
            arranged.well,
            side,
            width,
            designer.notes_width_unsaved,
        ));
        acts.extend(notes::show(designer, ui, look, side, design));
    }
    if let Some(prompt) = arranged.prompt {
        acts.extend(prompt_box(designer, ui, look, prompt, design));
    }
    Outcome { page, zoom, requests: super::apply(designer, acts) }
}

/// Copy what the toolbar needs out of the view, so the drawing holds no borrow of the provider.
fn facts_of(designer: &Designer, design: &Path) -> Option<Facts> {
    let view = designer.design_view(design)?;
    Some(Facts {
        name: designer.name_of(design),
        mode: view.mode,
        device: view.device,
        status: view.status.clone(),
        notes_shown: view.notes_shown,
        maximised: view.maximised,
    })
}

/// A maximized design (`task-2601`): one bar along the top with the design's name and a button that puts it back
/// in its tab, and the page in the rest of the pane, with no toolbar, notes or prompt box.
fn show_maximised(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    design: &Path,
    facts: &Facts,
) -> Outcome {
    let scale = look.scale();
    let inset = 6.0 * scale;
    let bar = Rect::from_min_size(
        Pos2::new(area.left() + PAD * scale, area.top() + inset),
        Vec2::new((area.width() - 2.0 * PAD * scale).max(0.0), MAXIMISED_BAR * scale),
    );
    let well = Rect::from_min_max(
        Pos2::new(area.left() + inset, bar.bottom() + inset),
        Pos2::new(area.right() - inset, (area.bottom() - inset).max(bar.bottom() + inset)),
    );
    let (page, zoom) = page_placement(well, facts.device);
    kit::well(ui, scale, well, WELL_RADIUS * scale / 2.0);
    let mut acts = Vec::new();
    kit::layer(ui, scale, "designer-maximised-bar", bar, |rux| {
        let middle = bar.center().y;
        let close = Rect::from_center_size(
            Pos2::new(bar.right() - CHIP * scale / 2.0, middle),
            Vec2::splat(CHIP * scale),
        );
        let room = (close.left() - bar.left() - 12.0 * scale).max(0.0);
        kit::heading(rux, Pos2::new(bar.left(), middle), &facts.name, 15.0, room);
        if kit::chip_button(rux, close, Icon::X, CHIP, "Normal size") {
            acts.push(Act::Maximise(design.to_path_buf(), false));
        }
    });
    device_label(ui, scale, well, page, facts.device);
    Outcome { page, zoom, requests: super::apply(designer, acts) }
}

/// The divider between the page and the notes, which sets how wide the notes are (`task-2601`). A double click
/// puts them back to their usual width. The width is written down when the drag stops.
///
/// @param well - the page's well, at the divider's left
/// @param side - the notes, at its right
/// @param width - how wide the notes are now, in points at no zoom
/// @param unsaved - a drag has moved the width and not yet written it down
fn the_divider(
    ui: &mut egui::Ui,
    scale: f32,
    well: Rect,
    side: Rect,
    width: f32,
    unsaved: bool,
) -> Vec<Act> {
    use crate::components::splitter;
    let middle = (well.right() + side.left()) / 2.0;
    let line = Rect::from_min_max(
        Pos2::new(middle - 0.5, side.top() + 8.0 * scale),
        Pos2::new(middle + 0.5, side.bottom() - 8.0 * scale),
    );
    let drag = splitter::show(ui, line, "notes", splitter::Axis::Upright);
    let mut acts = Vec::new();
    if drag.reset {
        acts.push(Act::NotesWidth(SIDEBAR, true));
    } else if drag.delta != 0.0 {
        acts.push(Act::NotesWidth(width - drag.delta / scale, false));
    } else if unsaved && !ui.input(|input| input.pointer.primary_down()) {
        acts.push(Act::NotesWidth(width, true));
    }
    acts
}

/// `Phone · 390` over the page, so a person can see which device the page is laid out for.
fn device_label(ui: &mut egui::Ui, scale: f32, well: Rect, page: Rect, device: Device) {
    let Some(width) = device.width() else { return };
    let band = Rect::from_min_size(
        Pos2::new(page.left(), well.top() + 2.0 * scale),
        Vec2::new(page.width(), (DEVICE_INSET - 4.0) * scale),
    );
    let words = format!("{} \u{00B7} {}", device.title(), width);
    kit::layer(ui, scale, "designer-device-label", band, |rux| {
        let ink = rux.theme().ink.i400;
        let galley = rux.text(rux::Style::sans(11.5).medium(), &words, ink);
        rux::text::draw_centred(rux.painter(), band, galley, ink);
        let frame = page.expand(3.0 * scale);
        let stroke = egui::Stroke::new(1.5, rux.theme().ink.i500.gamma_multiply(0.5));
        rux.painter().rect_stroke(frame, 6.0 * scale, stroke, egui::StrokeKind::Outside);
    });
}

/// What is left of the toolbar's width, as it is spent from the left.
struct Pen {
    x: f32,
    right: f32,
}

impl Pen {
    /// Take `width` and the gap after it, when there is room for both. Answers where it is.
    fn take(&mut self, width: f32, gap: f32) -> Option<f32> {
        (self.x + width <= self.right).then(|| {
            let at = self.x;
            self.x += width + gap;
            at
        })
    }
}

/// How the device is chosen in a toolbar of a given width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceForm {
    /// Five pills in a carved track.
    Switch,
    /// One button that names the device and moves to the next one when it is pressed.
    Cycle,
}

/// What the toolbar spends its width on, in points at the surface's own size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub name: f32,
    pub mode: f32,
    pub device: DeviceForm,
    /// The width the device control takes.
    pub device_width: f32,
}

/// What fits in `room`, which is the toolbar less its four chips.
///
/// **What gives way when it is narrow** is spent in the order of what matters least: the name's width, then the
/// device switch's width, then the name itself (the tab above already says it), then the device switch, which
/// becomes one button that cycles through the devices, then the mode switch's width. The mode words are never
/// cut while the name is still drawn. The mode and the device are never both absent, so
/// there is always a way to change either.
///
/// @param room - the toolbar's width less the chips, at the surface's own size
pub fn plan(room: f32) -> Plan {
    const MODE_FULL: f32 = 3.0 * 86.0 + 18.0;
    const MODE_TIGHT: f32 = 3.0 * 80.0 + 18.0;
    const SWITCH_FULL: f32 = 5.0 * 66.0 + 22.0;
    const SWITCH_TIGHT: f32 = 5.0 * 61.0 + 22.0;
    const CYCLE: f32 = 88.0;
    let gaps = 12.0;
    let tries = [
        (170.0, MODE_FULL, DeviceForm::Switch, SWITCH_FULL),
        (110.0, MODE_FULL, DeviceForm::Switch, SWITCH_TIGHT),
        (0.0, MODE_FULL, DeviceForm::Switch, SWITCH_TIGHT),
        (110.0, MODE_FULL, DeviceForm::Cycle, CYCLE),
        (70.0, MODE_TIGHT, DeviceForm::Cycle, CYCLE),
        (0.0, MODE_TIGHT, DeviceForm::Cycle, CYCLE),
    ];
    let wanted = |name: f32, mode: f32, device: f32| name + mode + device + gaps * 3.0;
    for (name, mode, form, device) in tries {
        if wanted(name, mode, device) <= room {
            return Plan { name, mode, device: form, device_width: device };
        }
    }
    Plan { name: 0.0, mode: MODE_TIGHT, device: DeviceForm::Cycle, device_width: CYCLE }
}

/// The toolbar: the design's name, the mode and the device, the zoom, the status, and four chips.
///
/// Answers whether the notes chip or the maximize chip was pressed, which the tab answers itself.
#[allow(clippy::too_many_arguments)]
fn toolbar(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    design: &Path,
    facts: &Facts,
    zoom: f64,
    notes_showing: bool,
    acts: &mut Vec<Act>,
) -> Chips {
    let scale = look.scale();
    let mut pressed = Chips::default();
    kit::layer(ui, scale, "designer-toolbar", rect, |rux| {
        let middle = rect.center().y;
        let chips_width = 4.0 * (CHIP + 8.0) * scale;
        let chosen = plan((rect.width() - chips_width) / scale);
        let mut pen = Pen { x: rect.left(), right: rect.right() - chips_width };
        if chosen.name > 0.0 {
            let width = chosen.name * scale;
            kit::heading(rux, Pos2::new(rect.left(), middle), &facts.name, 15.0, width);
            pen.take(width, 12.0 * scale);
        }
        let modes: Vec<String> = Mode::ALL.iter().map(|mode| mode.title().to_owned()).collect();
        if let Some(x) = pen.take(chosen.mode * scale, 12.0 * scale) {
            let rect = switch_rect(x, chosen.mode * scale, middle, scale);
            let at = Mode::ALL.iter().position(|mode| *mode == facts.mode);
            if let Some(picked) = kit::pill_switch(rux, rect, &modes, at, "Mode", true) {
                acts.push(Act::SetMode(design.to_path_buf(), Mode::ALL[picked]));
            }
        }
        if let Some(x) = pen.take(chosen.device_width * scale, 12.0 * scale) {
            let rect = switch_rect(x, chosen.device_width * scale, middle, scale);
            if let Some(next) = the_device_control(rux, rect, chosen.device, facts.device) {
                acts.push(Act::SetDevice(design.to_path_buf(), next));
            }
        }
        if zoom < 1.0 {
            show_the_zoom(rux, &mut pen, middle, scale, zoom);
        }
        let room = (pen.right - pen.x).max(0.0);
        if !facts.status.is_empty() && room > 60.0 * scale {
            kit::quiet(rux, Pos2::new(pen.x, middle), &facts.status, 12.0, room);
        }
        pressed = the_four_chips(rux, rect, design, scale, notes_showing, acts);
    });
    pressed
}

/// The device control in the form the toolbar had room for. Answers the device chosen this frame, if any.
fn the_device_control(
    rux: &mut Rux<'_>,
    rect: Rect,
    form: DeviceForm,
    now: Device,
) -> Option<Device> {
    match form {
        DeviceForm::Switch => {
            let titles: Vec<String> =
                Device::ALL.iter().map(|device| device.title().to_owned()).collect();
            let at = Device::ALL.iter().position(|device| *device == now);
            kit::pill_switch(rux, rect, &titles, at, "Device", true)
                .map(|picked| Device::ALL[picked])
        }
        DeviceForm::Cycle => {
            let button =
                Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 30.0 * rux.zoom()));
            let pressed = crate::components::settings_kit::button(
                rux,
                button,
                now.title(),
                crate::components::settings_kit::Kind::Chip,
            );
            let at = Device::ALL.iter().position(|device| *device == now).unwrap_or(0);
            pressed.then(|| Device::ALL[(at + 1) % Device::ALL.len()])
        }
    }
}

/// A switch's rectangle: `width` wide, 38 points tall, centred on the toolbar's middle.
fn switch_rect(x: f32, width: f32, middle: f32, scale: f32) -> Rect {
    Rect::from_min_size(Pos2::new(x, middle - 19.0 * scale), Vec2::new(width, 38.0 * scale))
}

/// The zoom as a number in a carved chip, so a person knows the page is drawn smaller than it is.
fn show_the_zoom(rux: &mut Rux<'_>, pen: &mut Pen, middle: f32, scale: f32, zoom: f64) {
    let words = format!("{}%", (zoom * 100.0).round() as i64);
    let width = 56.0 * scale;
    let Some(x) = pen.take(width, 10.0 * scale) else { return };
    let chip =
        Rect::from_min_size(Pos2::new(x, middle - 14.0 * scale), Vec2::new(width, 28.0 * scale));
    kit::well_surface(rux, chip, chip.height() / 2.0);
    let ink = rux.theme().ink.i500;
    let galley = rux.text(rux::Style::mono(11.5).medium(), &words, ink);
    rux::text::draw_centred(rux.painter(), chip, galley, ink);
}

/// The notes toggle, Source, History and Maximize, from the right. Answers which of the two the tab answers itself
/// was pressed.
///
/// **The notes toggle is where Reload was** (`task-2601`): *"Replace the undo button with the button for toggle
/// notes."* Reload's arrow read as undo. The page reloads by itself when its file changes, and Reload is still
/// `Designer -> Reload Design` and `plugins run designer reload`. The toggle sits in a carved well while the notes
/// are showing, so it reads as on or off.
fn the_four_chips(
    rux: &mut Rux<'_>,
    rect: Rect,
    design: &Path,
    scale: f32,
    notes_showing: bool,
    acts: &mut Vec<Act>,
) -> Chips {
    let middle = rect.center().y;
    let mut x = rect.right() - CHIP * scale / 2.0;
    let mut place = || {
        let centre = Pos2::new(x, middle);
        x -= (CHIP + 8.0) * scale;
        Rect::from_center_size(centre, Vec2::splat(CHIP * scale))
    };
    let mut pressed = Chips::default();
    let notes = place();
    if notes_showing {
        kit::well_surface(rux, notes.expand(2.0 * scale), notes.height() / 2.0 + 2.0 * scale);
    }
    let name = if notes_showing { "Hide notes" } else { "Show notes" };
    pressed.notes = rux::components::ChipButton::new(Icon::Bubble, CHIP, name)
        .mark(16.0, Some(1.6))
        .folds(notes_showing)
        .show(rux, notes)
        .clicked();
    if kit::chip_button(rux, place(), Icon::Document, CHIP, "Source") {
        acts.push(Act::OpenSource(design.to_path_buf()));
    }
    if kit::chip_button(rux, place(), Icon::Clock, CHIP, "History") {
        acts.push(Act::ShowHistory(design.to_path_buf()));
    }
    pressed.maximise = kit::chip_button(rux, place(), Icon::Expand, CHIP, "Maximize design");
    pressed
}

/// The prompt box under the page: a carved box with a hint naming what it is about, `Send open notes` beside
/// the send button when there are open notes.
fn prompt_box(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    design: &Path,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let Some(view) = designer.design_view_mut(design) else { return acts };
    let hint = match &view.selection {
        Some(element) => format!("Ask about {}...", element.tag),
        None => "Ask about this design...".to_owned(),
    };
    let open = view.notes.counts().0;
    let has_words = !view.prompt.trim().is_empty();
    let prompt = &mut view.prompt;
    let owner: PathBuf = design.to_path_buf();
    let target = view.selection.as_ref().map(|element| element.tag.clone());
    kit::layer(ui, scale, "designer-prompt", rect, |rux| {
        let theme = rux.theme();
        let chips = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 28.0 * scale));
        if let Some(tag) = &target {
            let words = format!("Asking about {tag}");
            let width = kit::quiet(
                rux,
                Pos2::new(chips.left() + 12.0 * scale, chips.center().y),
                &words,
                11.5,
                chips.width() * 0.6,
            );
            let chip =
                Rect::from_min_size(chips.min, Vec2::new(width + 24.0 * scale, chips.height()));
            kit::well_surface(rux, chip, chip.height() / 2.0);
            kit::quiet(
                rux,
                Pos2::new(chip.left() + 12.0 * scale, chip.center().y),
                &words,
                11.5,
                width + 4.0,
            );
        }
        if open > 0 {
            let label = format!("Send open notes ({open})");
            let width = kit::button_width(rux, &label, true) + 14.0 * scale;
            let pill = Rect::from_min_size(
                Pos2::new(chips.right() - width, chips.top()),
                Vec2::new(width, chips.height()),
            );
            if kit::button(rux, pill, &label, Some(Icon::Spark), kit::Look::Chip) {
                acts.push(Act::AskAll(owner.clone()));
            }
        }
        let row =
            Rect::from_min_max(Pos2::new(rect.left(), chips.bottom() + 6.0 * scale), rect.max);
        let send = Rect::from_center_size(
            Pos2::new(row.right() - 26.0 * scale, row.center().y),
            Vec2::splat(44.0 * scale),
        );
        let field = Rect::from_min_max(row.min, Pos2::new(row.right(), row.bottom()));
        kit::carved_field(
            rux,
            field,
            28.0 * scale,
            prompt,
            &hint,
            "Ask about this design",
            "designer-prompt-field",
            false,
            13.0,
        );
        let _ = theme;
        let sent = rux::components::RoundButton::send("Send to agent")
            .enabled(has_words)
            .show(rux, send)
            .clicked();
        if sent && has_words {
            acts.push(Act::Prompt(owner.clone()));
        }
    });
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn well() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 80.0), Vec2::new(900.0, 600.0))
    }

    #[test]
    fn fit_is_the_well_less_a_small_inset_at_no_zoom() {
        let (page, zoom) = page_placement(well(), Device::Fit);
        assert_eq!(zoom, 1.0);
        assert_eq!(page, well().shrink(12.0));
    }

    #[test]
    fn a_device_that_fits_is_its_own_width_and_centred() {
        let (page, zoom) = page_placement(well(), Device::Phone);
        assert_eq!(zoom, 1.0);
        assert_eq!(page.width(), 390.0);
        assert_eq!(page.center().x, well().center().x);
        assert_eq!(page.top(), well().top() + 24.0);
        assert_eq!(page.height(), well().height() - 48.0);
    }

    #[test]
    fn a_device_wider_than_the_room_is_zoomed_down_to_a_hundredth_and_still_laid_out_at_its_width()
    {
        // 852 points of room for a page 1440 wide: 0.5916... rounded down to 0.59.
        let (page, zoom) = page_placement(well(), Device::Desktop);
        assert_eq!(zoom, 0.59);
        assert!((page.width() - 1440.0 * 0.59).abs() < 0.01, "{}", page.width());
        // The viewport the engine lays the page out in is the bounds divided by the zoom: 1440 wide.
        assert!((page.width() / zoom as f32 - 1440.0).abs() < 0.5);
        assert!(page.width() <= 852.0);
    }

    #[test]
    fn the_zoom_never_goes_below_a_quarter() {
        let narrow = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 400.0));
        let (page, zoom) = page_placement(narrow, Device::Desktop);
        assert_eq!(zoom, 0.25);
        assert!(page.width() <= narrow.shrink(24.0).width() + f32::EPSILON);
    }

    #[test]
    fn the_toolbar_gives_way_in_the_order_that_matters_least_and_keeps_a_way_to_change_both() {
        let wide = plan(900.0);
        assert!(matches!(wide.device, DeviceForm::Switch) && wide.name == 170.0);
        let middling = plan(720.0);
        assert!(matches!(middling.device, DeviceForm::Switch), "{middling:?}");
        assert!(middling.name < wide.name && middling.device_width < wide.device_width);
        let narrow = plan(420.0);
        assert_eq!(narrow.device, DeviceForm::Cycle, "the five pills become one button");
        let narrowest = plan(100.0);
        assert!(
            narrowest.mode > 0.0 && narrowest.device_width > 0.0,
            "a mode and a device can always be chosen"
        );
        assert_eq!(narrowest.name, 0.0, "the name is what goes");
    }

    #[test]
    fn the_notes_are_as_wide_as_the_divider_says_and_never_take_most_of_the_tab() {
        let tall = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
        let wide = arrange(tall, 1.0, true, 500.0).sidebar.expect("showing");
        assert_eq!(wide.width(), 500.0);
        let at_two = arrange(tall, 2.0, true, 200.0).sidebar.expect("showing");
        assert_eq!(at_two.width(), 400.0, "the width is at no zoom, so it grows with the surface");
        let too_wide = arrange(tall, 1.0, true, 5000.0).sidebar.expect("showing");
        assert!(too_wide.width() <= (1200.0 - 28.0) * 0.7 + 0.01, "{}", too_wide.width());
    }

    #[test]
    fn the_notes_start_hidden_below_a_narrow_tab_and_the_prompt_needs_some_height() {
        let tall = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
        let with = arrange(tall, 1.0, true, SIDEBAR);
        assert!(with.sidebar.is_some() && with.prompt.is_some());
        assert!(
            with.well.right() < with.sidebar.unwrap().left(),
            "the well and the notes do not overlap"
        );
        let without = arrange(tall, 1.0, false, SIDEBAR);
        assert!(without.sidebar.is_none());
        assert!(without.well.width() > with.well.width());
        let short = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 200.0));
        assert!(
            arrange(short, 1.0, true, SIDEBAR).prompt.is_none(),
            "no room for a prompt, so none is drawn"
        );
    }
}
