//! `Settings -> Keymap`: which chord each menu entry is on. `task-2286`, and
//! `tasks/task-2286-keymap-tdd.md` §7 is the design.
//!
//! Two cards. The first chooses the preset, `Unluminous`, `VS Code` or `IntelliJ`, and says how many of the
//! person's own changes are kept on top of it. The second is a search well and every bindable action, each a
//! row with its menu path, its chord as a chip, where the chord came from, and `Reset` when the row holds the
//! person's own binding.
//!
//! **Pressing a chip records the next key press.** The row says `Press a shortcut` in the accent and the
//! page takes every key press out of the frame until one is recorded, so the dialog's own `Enter` does not
//! close it and a typed letter does not reach the search well. The menu watcher stands aside while a row is
//! recording, which `app::frame::read_the_menu_shortcuts` asks of [`KeymapState::recording`]; otherwise
//! pressing `Ctrl+S` to bind it would also save.
//!
//! Nothing about which chord wins is decided here. `services::keymap` holds all of it, and the page only reads
//! a table and asks `keymap::bind` to change the person's bindings.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::{Fill, Rux, Style};

use crate::app::actions::Shortcut;
use crate::components::settings_dialog::{drawn, Drawn};
use crate::components::settings_kit::{self as kit, Column};
use crate::services::keymap::{self, Bindable, Binding, Preset, Source, Table};
use crate::settings::Settings;

/// How tall one row of the list is, and the gap between rows.
const ROW: f32 = 40.0;
/// How wide the chord chip is, and the `Reset` chip beside it.
const CHIP: f32 = 170.0;
const RESET: f32 = 64.0;
/// The room between the chip and what is to the left of it.
const GAP: f32 = 10.0;
/// The words a chip says while it is recording.
pub const RECORDING: &str = "Press a shortcut";

/// What the page is showing. Lives in the Settings window's state, so it is still there when the page is
/// opened again.
#[derive(Debug, Clone, Default)]
pub struct KeymapState {
    /// What has been typed in the search well.
    pub search: String,
    /// The action whose chip is recording, by name.
    pub recording: Option<String>,
    /// The line under the search well: what the last binding did, or why it was refused.
    pub notice: Option<String>,
}

/// What a key press while recording asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Recorded {
    /// `Escape`: stop and change nothing.
    Cancel,
    /// `Backspace` or `Delete` with no modifier: take the chord away.
    Clear,
    /// One of the clipboard chords, which egui hands over as its own events.
    Clipboard,
    Chord(Shortcut),
}

/// Draw the page down `area`, and answer whether a setting changed and how tall the page came out.
pub(crate) fn show(ui: &mut egui::Ui, area: Rect, state: &mut KeymapState, settings: &mut Settings) -> Drawn {
    let mut changed = false;
    if state.recording.is_some() {
        changed |= follow_the_recording(ui, state, settings);
    }
    let mut column = Column::new(area.left(), area.right(), area.top());
    changed |= preset_card(ui, &mut column, state, settings);
    let table = keymap::table(settings.keymap_preset, &settings.key_bindings, keymap::bindables());
    changed |= shortcuts_card(ui, &mut column, state, settings, &table);
    drawn(area.top(), &column, changed)
}

/// The preset, and how many of the person's own changes are on top of it.
fn preset_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut KeymapState,
    settings: &mut Settings,
) -> bool {
    let mut changed = false;
    let help = "The set of shortcuts you start from. Your own changes stay on top of whichever you choose. \
                VS Code and IntelliJ give the actions Unluminous has the chords those editors use, and an \
                action they say nothing about keeps its Unluminous chord unless they gave it to another.";
    kit::card(ui, column, "Preset", Some(help), |card| {
        let labels: Vec<&str> = Preset::ALL.iter().map(|preset| preset.label()).collect();
        let chosen = Preset::ALL.iter().position(|preset| *preset == settings.keymap_preset);
        let own = settings.key_bindings.len();
        let said = match own {
            0 => "None of your own changes.".to_owned(),
            1 => "1 of your own changes is kept on top.".to_owned(),
            n => format!("{n} of your own changes are kept on top."),
        };
        card.row("Keymap", Some(&said), |row| {
            if let Some(index) = row.segmented("Keymap preset", &labels, chosen, 96.0, false) {
                if Preset::ALL[index] != settings.keymap_preset {
                    settings.keymap_preset = Preset::ALL[index];
                    state.notice = Some(format!("{} chosen.", Preset::ALL[index].label()));
                    changed = true;
                }
            }
            if own > 0 && row.ghost("Reset all") {
                settings.key_bindings.clear();
                state.notice = Some("Your own changes were taken away.".to_owned());
                changed = true;
            }
        });
    });
    changed
}

/// The search well, the line about the last change, and every row that matches the search.
fn shortcuts_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut KeymapState,
    settings: &mut Settings,
    table: &Table,
) -> bool {
    let mut changed = false;
    let help = "Press a shortcut's chip, then press the keys. Escape stops, Backspace takes the shortcut \
                away. A shortcut another action has is taken from it.";
    let rows = matching_rows(keymap::bindables(), table, &state.search);
    kit::card(ui, column, "Shortcuts", Some(help), |card| {
        card.row("Search", None, |row| {
            let width = row.right - row.x;
            row.text("Search shortcuts", &mut state.search, "An action, a menu or keys", width);
        });
        if let Some(notice) = state.notice.clone() {
            card.help(&notice);
        }
        let height = (rows.len().max(1) as f32) * ROW;
        changed = card.custom(height, |rux, rect| list(rux, rect, &rows, table, state, settings));
    });
    changed
}

/// The bindables the search matches, the ones that can be changed first.
fn matching_rows<'a>(bindables: &'a [Bindable], table: &Table, search: &str) -> Vec<&'a Bindable> {
    let mut rows: Vec<&Bindable> = bindables
        .iter()
        .filter(|bindable| matches_search(bindable, table.rows.get(&bindable.name), search))
        .collect();
    rows.sort_by_key(|bindable| bindable.fixed.is_some());
    rows
}

/// True when a row is worth showing for what was typed: its name, its menu path or its chord, ignoring case,
/// spaces and whether `+` or `-` joins a chord's parts.
pub fn matches_search(bindable: &Bindable, binding: Option<&Binding>, search: &str) -> bool {
    let squash = |text: &str| -> String {
        text.to_lowercase().chars().filter(|c| !c.is_whitespace()).map(|c| if c == '-' { '+' } else { c }).collect()
    };
    let needle = squash(search);
    if needle.is_empty() {
        return true;
    }
    let chord = binding.and_then(|binding| binding.chord).or(bindable.default);
    let chord = chord.map(|chord| keymap::text(&chord)).unwrap_or_default();
    [bindable.path(), bindable.name.clone(), chord].iter().any(|text| squash(text).contains(&needle))
}

/// Draw the rows that are inside the part of the page that can be seen. Answers whether a binding changed.
fn list(
    rux: &mut Rux<'_>,
    rect: Rect,
    rows: &[&Bindable],
    table: &Table,
    state: &mut KeymapState,
    settings: &mut Settings,
) -> bool {
    let theme = rux.theme();
    if rows.is_empty() {
        let galley = rux.text(kit::label_style(), "No action matches", theme.ink.i400);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(rect.left(), rect.top() + ROW / 2.0),
            galley,
            kit::label_style(),
            theme.ink.i400,
        );
        return false;
    }
    let seen = rux.ui.clip_rect();
    let mut changed = false;
    for (index, bindable) in rows.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + index as f32 * ROW),
            Vec2::new(rect.width(), ROW),
        );
        if !row.intersects(seen) {
            continue;
        }
        changed |= one_row(rux, row, bindable, table.rows.get(&bindable.name), state, settings);
    }
    changed
}

/// One action: its menu path and name, where its chord came from, its chip, and `Reset` when it is the
/// person's own.
fn one_row(
    rux: &mut Rux<'_>,
    row: Rect,
    bindable: &Bindable,
    binding: Option<&Binding>,
    state: &mut KeymapState,
    settings: &mut Settings,
) -> bool {
    let theme = rux.theme();
    let chip = Rect::from_min_size(
        Pos2::new(row.right() - RESET - GAP - CHIP, row.center().y - 15.0),
        Vec2::new(CHIP, 30.0),
    );
    let words_right = chip.left() - GAP - source_width(rux, bindable, binding, settings.keymap_preset);
    words(rux, row, bindable, words_right);
    source_tag(rux, chip, bindable, binding, settings.keymap_preset);
    let recording = state.recording.as_deref() == Some(bindable.name.as_str());
    let chord = binding.map(|binding| binding.chord).unwrap_or(bindable.default);
    let said = match (recording, chord) {
        (true, _) => RECORDING.to_owned(),
        (false, Some(chord)) => keymap::text(&chord),
        (false, None) => "None".to_owned(),
    };
    let ink = match (recording, bindable.fixed.is_some(), chord.is_some()) {
        (true, _, _) => theme.accent.blue,
        (false, true, _) | (false, false, false) => theme.ink.i400,
        (false, false, true) => theme.ink.i900,
    };
    let name = format!("Shortcut: {}", bindable.path());
    let pressed = chord_chip(rux, chip, &said, &name, ink, bindable.fixed.is_none(), recording);
    if pressed {
        state.recording = Some(bindable.name.clone());
        state.notice = None;
        rux.ui.ctx().memory_mut(|memory| memory.stop_text_input());
    }
    let yours = binding.is_some_and(|binding| binding.source == Source::Yours);
    if !yours {
        return false;
    }
    let reset = Rect::from_min_size(Pos2::new(chip.right() + GAP, chip.top()), Vec2::new(RESET, 30.0));
    if chord_chip(rux, reset, "Reset", &format!("Reset {}", bindable.path()), theme.ink.i500, true, false) {
        keymap::reset(&mut settings.key_bindings, &bindable.name);
        state.notice = Some(format!("{} is back on the preset's shortcut.", bindable.path()));
        return true;
    }
    false
}

/// The menu path in the quiet ink and the action's name after it, cut short before `right`.
fn words(rux: &mut Rux<'_>, row: Rect, bindable: &Bindable, right: f32) {
    let theme = rux.theme();
    let room = (right - row.left()).max(20.0);
    let menu = format!("{} \u{203A} ", bindable.menu);
    let menu_galley = rux.elided(Style::sans(12.0), &menu, theme.ink.i400, room * 0.55);
    let menu_width = menu_galley.size().x;
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(row.left(), row.center().y),
        menu_galley,
        Style::sans(12.0),
        theme.ink.i400,
    );
    let label = rux.elided(kit::label_style(), &bindable.label, theme.ink.i900, room - menu_width);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(row.left() + menu_width, row.center().y),
        label,
        kit::label_style(),
        theme.ink.i900,
    );
}

/// The small word before a chip that says where its chord came from, or why it is fixed.
fn source_word(bindable: &Bindable, binding: Option<&Binding>, preset: Preset) -> Option<String> {
    if bindable.fixed.is_some() {
        return Some("Fixed".to_owned());
    }
    let binding = binding?;
    if let Some(winner) = &binding.lost_to {
        let path = keymap::bindables().iter().find(|b| &b.name == winner).map(|b| b.label.clone());
        return Some(format!("Taken by {}", path.unwrap_or_else(|| winner.clone())));
    }
    match binding.source {
        Source::Unluminous => None,
        Source::Preset => Some(preset.label().to_owned()),
        Source::Yours => Some("Yours".to_owned()),
    }
}

fn source_width(rux: &Rux<'_>, bindable: &Bindable, binding: Option<&Binding>, preset: Preset) -> f32 {
    match source_word(bindable, binding, preset) {
        Some(word) => rux.measure(Style::sans(11.0), &word).x + GAP,
        None => 0.0,
    }
}

fn source_tag(rux: &mut Rux<'_>, chip: Rect, bindable: &Bindable, binding: Option<&Binding>, preset: Preset) {
    let Some(word) = source_word(bindable, binding, preset) else {
        return;
    };
    let theme = rux.theme();
    let galley = rux.text(Style::sans(11.0), &word, theme.ink.i400);
    let at = Pos2::new(chip.left() - GAP - galley.size().x, chip.center().y);
    rux::text::draw_left_capitals(rux.painter(), at, galley, Style::sans(11.0), theme.ink.i400);
}

/// A chip with words on it and a name of its own, raised, or carved while it is recording. Answers whether it
/// was pressed. A chip that cannot be pressed is drawn flat.
fn chord_chip(
    rux: &mut Rux<'_>,
    rect: Rect,
    said: &str,
    name: &str,
    ink: egui::Color32,
    live: bool,
    recording: bool,
) -> bool {
    let chat = rux.theme().chat;
    let sense = if live { Sense::click() } else { Sense::hover() };
    let response = rux.ui.interact(rect, rux.ui.id().with(("keymap-chip", name)), sense);
    if live && response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let radius = rect.height() / 2.0;
    match (recording, live) {
        (true, _) => rux.chrome.surface(rect, radius, chat.well, chat.carved_sm()),
        (false, true) => rux.chrome.surface(rect, radius, Fill::gradient(chat.chip, rect), chat.raised_chip()),
        (false, false) => rux.chrome.surface(rect, radius, chat.well, chat.carved_sm()),
    }
    let style = Style::mono(11.5);
    let galley = rux.elided(style, said, ink, rect.width() - 16.0);
    rux::text::draw_centred(rux.painter(), rect, galley, ink);
    let named = format!("{name}: {said}");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, named.clone()));
    live && response.clicked()
}

/// Read what the recording row was given this frame, and act on it. Answers whether a binding changed.
fn follow_the_recording(ui: &mut egui::Ui, state: &mut KeymapState, settings: &mut Settings) -> bool {
    let Some(action) = state.recording.clone() else {
        return false;
    };
    // A press anywhere stops it; a press on another chip starts that one when it is released.
    if ui.input(|input| input.pointer.any_pressed()) {
        state.recording = None;
        return false;
    }
    let Some(recorded) = take_the_key_press(ui.ctx(), keymap::is_mac()) else {
        return false;
    };
    state.recording = None;
    let label = keymap::bindables().iter().find(|b| b.name == action).map(Bindable::path);
    let label = label.unwrap_or_else(|| action.clone());
    match recorded {
        Recorded::Cancel => {
            state.notice = None;
            false
        }
        Recorded::Clipboard => {
            state.notice = Some("Ctrl+C, Ctrl+X and Ctrl+V belong to the clipboard.".to_owned());
            false
        }
        Recorded::Clear => bind_and_say(state, settings, &action, &label, None),
        Recorded::Chord(chord) => bind_and_say(state, settings, &action, &label, Some(chord)),
    }
}

/// Bind a chord through `keymap::bind` and write what it did, or why not, under the search well.
fn bind_and_say(
    state: &mut KeymapState,
    settings: &mut Settings,
    action: &str,
    label: &str,
    chord: Option<Shortcut>,
) -> bool {
    let preset = settings.keymap_preset;
    match keymap::bind(&mut settings.key_bindings, preset, action, chord) {
        Err(why) => {
            state.notice = Some(why);
            false
        }
        Ok(bound) => {
            state.notice = Some(what_binding_did(label, chord, &bound));
            true
        }
    }
}

/// The sentence a binding leaves under the search well.
pub fn what_binding_did(label: &str, chord: Option<Shortcut>, bound: &keymap::Bound) -> String {
    let Some(chord) = chord else {
        return format!("{label} has no shortcut now.");
    };
    let text = keymap::text(&chord);
    let mut said = format!("{label} is on {text}.");
    for taken in &bound.taken_from {
        let other = keymap::bindables().iter().find(|b| &b.name == taken).map(Bindable::path);
        said.push_str(&format!(" It was taken from {}.", other.unwrap_or_else(|| taken.clone())));
    }
    if let Some(what) = keymap::formatting_chord(&chord) {
        said.push_str(&format!(" In a Markdown file {text} no longer does {what}."));
    }
    said
}

/// Take this frame's key presses out of the frame, and answer the first one as a chord.
///
/// Every key press and every typed letter goes, recorded or not, so `Enter` does not close the dialog and a
/// letter does not reach a text box.
pub fn take_the_key_press(ctx: &egui::Context, mac: bool) -> Option<Recorded> {
    ctx.input_mut(|input| {
        let mut found = None;
        input.events.retain(|event| match event {
            egui::Event::Key { key, pressed, modifiers, .. } => {
                if *pressed && found.is_none() {
                    found = Some(recorded(*key, modifiers, mac));
                }
                false
            }
            egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => {
                found.get_or_insert(Recorded::Clipboard);
                false
            }
            egui::Event::Text(_) => false,
            _ => true,
        });
        found
    })
}

/// What one key press with its modifiers means to a recording row.
pub fn recorded(key: egui::Key, modifiers: &egui::Modifiers, mac: bool) -> Recorded {
    let bare = !modifiers.any();
    match key {
        egui::Key::Escape if bare => return Recorded::Cancel,
        egui::Key::Backspace | egui::Key::Delete if bare => return Recorded::Clear,
        _ => {}
    }
    let chord = Shortcut {
        key,
        command: if mac { modifiers.mac_cmd || modifiers.command } else { modifiers.ctrl || modifiers.command },
        ctrl: mac && modifiers.ctrl,
        alt: modifiers.alt,
        shift: modifiers.shift,
    };
    Recorded::Chord(chord.normalised_for(mac))
}
