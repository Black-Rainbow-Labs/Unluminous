//! The Agent-Chat pane: the one raised card, its header, the conversation and the composer.
//!
//! `_agent_output/task-1767-agent-chat/reference-chat.png` is the picture this is measured against
//! and `tasks/task-1767-agent-chat-tdd.md` §1 says how it was made. The structure is the ai-service
//! LLM chat page's own — `ChatPanel` wrapping `ChatHeader`, `ChatConversation` and `ChatComposer` —
//! wearing the dark neumorphic palette the Agent-Tasks board is drawn in.
//!
//! ## The drawing changes nothing, and `pane` is where the change happens
//!
//! Every function here that draws takes a rectangle, draws, and reports an [`Act`]; not one of them
//! changes the conversation. [`pane`] is the provider's own entry rather than a component — it is what
//! `UiProvider::pane` calls — and it is the one place the acts are applied, after everything has been
//! drawn. That split is what lets the whole surface be drawn from a borrow of the conversation while
//! the things that change it need a mutable one.
//!
//! ## The Agent-Chat design (`task-2235`)
//!
//! Since `task-2235` the pane is drawn to the design on the Claude Design canvas "Unluminous", page
//! Agent-Chat: no card round the whole pane, a header of two round buttons with the conversation's name
//! centred between them, the person's messages as raised bubbles and the agent's words on the pane with
//! no bubble, tool runs as cards with a status well, a 30 point gear centred over one carved prompt box,
//! and the model choice and the context used in a Chat settings dialog the gear opens. Every surface is
//! `rux`'s chat parts, through [`kit`].
//!
//! ## The ground is the window's
//!
//! `show_the_plugin_panes` fills the pane and reserves the decoration's slot before this is called. A
//! second ground painted here would go into the painter *after* that slot and wash the decoration
//! out, which is the fault `task-1765` records for the board. So the only surface painted here is the
//! panel itself, through `Chrome::raised`.

pub mod blocks;
pub mod composer;
pub mod kit;
pub mod message;
pub mod settings_page;
pub mod welcome;

use egui::{CornerRadius, Pos2, Rect, Stroke, Vec2};

use crate::components::controls;
use crate::services::agent_chat::{AgentChat, Parts};
use crate::services::plugin_ui::{Look, Request};
use crate::theme::icon;

/// The pane's side padding and the header's top padding: `padding: 18px`.
pub const SIDE: f32 = 18.0;
/// The header row, which is as tall as its 38 point round buttons.
pub const HEADER: f32 = 38.0;
/// Under the header: `padding-bottom: 14px`.
pub const UNDER_HEADER: f32 = 14.0;
/// Between two rows of the conversation: `gap: 18px`.
pub const GAP: f32 = 18.0;
/// The gear above the prompt box, and the room above and below it: `padding: 4px 18px 14px`.
pub const GEAR: f32 = 30.0;
const ABOVE_GEAR: f32 = 4.0;
const BELOW_GEAR: f32 = 14.0;

/// What the drawing reported, applied by [`pane`] once everything has been drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Send,
    Stop,
    New,
    Open(String),
    Remove(String),
    Choose(String),
    /// A picture dropped on the pane.
    Dropped(std::path::PathBuf),
    /// Ctrl/Cmd+V in the composer: ask the window for whatever picture is on the clipboard.
    Paste,
    Detach(u64),
    Copy(String),
    ShowHistory(bool),
    /// Open or close one tool block, by its call id.
    ToggleTool(String),
    /// Open or close a run of tool calls, by `message::run_key`.
    ToggleGroup(String),
    /// Open or close one message's thinking.
    ToggleThinking(u64),
    /// Select the whole of one message's words, which is what the menu's `Select All` means.
    SelectAll(u64),
    /// A component's button asked for these words to be sent as the person's next message.
    SendWords(String),
    /// A component's button asked for these words to be put in the composer.
    Fill(String),
    /// A component asked for a project file to be opened, at a line when it named one.
    OpenFile(String, Option<u32>),
    /// Open or close the Chat settings dialog.
    ShowSettings(bool),
}

/// Draw the pane, and act on what was pressed.
pub fn pane(chat: &mut AgentChat, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let mut acts = Vec::new();
    if area.width() > 40.0 && area.height() > 40.0 {
        acts = surface(chat.parts(), ui, look, area);
    }
    // The pointer's own answer to a picture dropped on the pane. `egui` collects what the window
    // manager handed over, and a path that is a picture is attached exactly as the `+` would attach
    // it — which is the third way in the ticket's own list, after the button and the clipboard.
    for path in dropped_pictures(ui, area) {
        acts.push(Act::Dropped(path));
    }
    // **A paste is claimed before the pane loop reads it**, which is the one-frame ordering the
    // Markdown preview's own copy already uses. What `egui` reports of Ctrl/Cmd+V when the clipboard
    // holds a picture rather than text is nothing at all, so the chord is read off the key going back
    // up and the window is asked for the picture - see `pasting`.
    if pasting(ui, chat.parts().state.prompt_focused) {
        acts.push(Act::Paste);
    }
    // **A selection is about the message it was made in, and a press somewhere else ends it.**
    // Without this, words selected in an answer would go on being what `Ctrl`/`Cmd`+`C` copied for
    // the rest of the session, including from the editing area beside the pane. Not while the right
    // click menu is open, because the press that chooses one of its rows is a press outside the pane
    // and would throw away the very selection the row is about.
    if chat.ui.menu.is_none() && a_press_landed_outside(ui, area) {
        chat.ui.selection = None;
    }
    if let Some(text) = copying(ui, chat) {
        acts.push(Act::Copy(text));
    }
    apply(chat, acts)
}

/// Whether the primary button went down this frame somewhere that is not this pane.
///
/// The pointer through [`controls::pointer_in`], because a chat node on the canvas is drawn into a
/// layer carrying the camera and the frame's raw position is the window's — the two agree only while
/// the camera sits at one on the origin, which is where a test leaves it and where nobody leaves a
/// canvas. `task-2003` wrote that down for the board's own wheel.
fn a_press_landed_outside(ui: &egui::Ui, area: Rect) -> bool {
    let pressed = ui.ctx().input(|input| input.pointer.primary_pressed());
    pressed && controls::pointer_in(ui).is_none_or(|at| !area.contains(at))
}

/// What `Ctrl`/`Cmd`+`C` should copy out of this pane, taking the event so nothing else copies too.
///
/// `task-2060`: *"I should be able to select & copy text in a message. e.g. select/highlight a sub
/// section, then right click and see copy option, or press Ctrl/CMD+C."* egui delivers a copy as an
/// `egui::Event::Copy` rather than as a key press — which is why `Copy` is marked in `actions::menus`
/// as not coming from the keyboard — so the event is what has to be claimed, and removing it from
/// the frame's input is what stops the editing area copying its own selection a moment later. That
/// is `UnluminousApp::route_the_preview_copy`'s rule, kept here.
///
/// **Not while a text box has the keyboard.** The composer is a text box in this same pane, and a
/// copy made while the caret is in it is that box's own.
fn copying(ui: &egui::Ui, chat: &mut AgentChat) -> Option<String> {
    if crate::app::text_box_has_the_keyboard(ui.ctx()) {
        return None;
    }
    let text = chat.selected_text()?;
    let took = ui.ctx().input_mut(|input| {
        let before = input.events.len();
        input.events.retain(|event| !matches!(event, egui::Event::Copy));
        before != input.events.len()
    });
    took.then_some(text)
}

/// Everything inside the pane, from a borrow of the conversation.
///
/// Top to bottom: the header, the conversation, the gear and the prompt box, and the Chat settings dialog
/// over all of it while it is open.
fn surface(mut parts: Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    parts.state.speaker = parts.session.chat.provider.clone();
    let side = SIDE * scale;
    if area.width() < side * 2.0 + 40.0 {
        return acts;
    }
    let header_rect = Rect::from_min_size(
        area.min + Vec2::new(side, side),
        Vec2::new(area.width() - side * 2.0, HEADER * scale),
    );
    acts.extend(header(&mut parts, ui, look, header_rect));

    let inner_width = area.width() - side * 2.0;
    let composer_height = composer::height(&parts, look, inner_width);
    let composer_rect = Rect::from_min_size(
        Pos2::new(area.left() + side, area.bottom() - side - composer_height),
        Vec2::new(inner_width, composer_height),
    );
    let gear_rect = Rect::from_center_size(
        Pos2::new(area.center().x, composer_rect.top() - (BELOW_GEAR + GEAR / 2.0) * scale),
        Vec2::splat(GEAR * scale),
    );
    // **The conversation is the pane's whole width**, and its rows are inset by [`SIDE`] inside it, so a
    // bubble's shadow at the right-hand edge is not cut by the scrolling area's clip.
    let body = Rect::from_min_max(
        Pos2::new(area.left(), header_rect.bottom() + UNDER_HEADER * scale),
        Pos2::new(area.right(), gear_rect.top() - ABOVE_GEAR * scale),
    );
    // Forgotten before it is drawn again, so a pane too short to hold a list does not go on answering a
    // wheel with where one used to be. See `PaneState::list_rect`.
    parts.state.list_rect = None;
    if body.height() > 20.0 {
        // The two lists are drawn **over** the conversation rather than in a popup, because egui keeps
        // at most one popup open at a time, and a pane that could not open its own history while a menu
        // was up would be a pane whose history is unreachable at the moment somebody wants it.
        if parts.state.history_open {
            acts.extend(history_list(&mut parts, ui, look, body.shrink2(Vec2::new(side, 0.0))));
        } else {
            acts.extend(conversation(&mut parts, ui, look, body));
        }
    }
    // Drawn after the conversation and before the composer takes `parts`. A popup is a layer of its
    // own, so where it sits in this order decides nothing about what it is drawn over — what it
    // decides is that the rows are read on the frame after the right click that opened it, which is
    // `controls::field_menu`'s own shape.
    acts.extend(message_menu(&mut parts, ui, look));
    acts.extend(gear(&mut parts, ui, look, gear_rect));
    if parts.state.settings_open {
        acts.extend(settings(&mut parts, ui, look, area));
    }
    kit::end_frame(parts.state);
    acts.extend(composer::show(parts, ui, look, composer_rect));
    acts
}

/// The gear centred over the prompt box, which opens and closes the Chat settings dialog.
fn gear(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, rect: Rect) -> Vec<Act> {
    let open = parts.state.settings_open;
    let pressed = kit::layer(parts.state, look, ui, "gear", rect, kit::SMALL_REACH, |rux| {
        rux::components::RoundButton::mark(rux::Icon::Settings, 13.0, "Chat settings")
            .size(rux::components::RoundSize::Small)
            .on(open)
            .show(rux, rect)
            .clicked()
    });
    match pressed {
        true => vec![Act::ShowSettings(!open)],
        false => Vec::new(),
    }
}

/// The Chat settings dialog: the model as a pill switch with one pill a provider, and how much of the
/// model's context the conversation fills. It closes with its round cross and nothing else, which is
/// what the design asks (`task-2235`): no Done button, and a press on the scrim does nothing.
///
/// It is an overlay of the pane's own rather than a window modal: it is about this conversation, and on a
/// canvas each chat node has its own. It is a layer of its own above the pane, so nothing under it takes
/// the pointer while it is open.
fn settings(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let names: Vec<String> =
        parts.configuration.providers.iter().map(|one| one.name.clone()).collect();
    let chosen = parts
        .configuration
        .provider()
        .and_then(|chosen| names.iter().position(|name| *name == chosen.name));
    let used = parts.session.chat.context_used;
    let window = parts.session.chat.context_window;
    let side = SIDE * scale;
    let foot = settings_foot(parts, look, area);
    // `padding: 20px 20px 22px; gap: 22px`: the title row, the model and the context well.
    let height = (20.0 + 34.0 + 22.0 + 17.0 + 10.0 + 46.0 + 22.0 + 80.0 + 22.0) * scale;
    let dialog = Rect::from_min_max(
        Pos2::new(area.left() + side, (foot - height).max(area.top() + side)),
        Pos2::new(area.right() - side, foot),
    );
    let id = ui.id().with("agent-chat-settings");
    let still = parts.state.still;
    let state = kit::state(&mut parts.state.chrome_rux, look, still);
    // `kit::state` has already put it on the active theme; said again here because this file opens a
    // `rux` layer of its own, and every file that does says so (`every_rux_drawing_follows_the_theme`).
    crate::theme::in_step(state);
    let ctx = ui.ctx().clone();
    // **Drawn through the same transform as the pane**, so on a canvas node the dialog is placed and
    // scaled with the node rather than in the window's own points.
    let layer = egui::LayerId::new(egui::Order::Middle, id);
    if let Some(to_global) = ctx.layer_transform_to_global(ui.layer_id()) {
        ctx.set_transform_layer(layer, to_global);
    }
    let clip = area.intersect(ui.clip_rect());
    egui::Area::new(id).order(egui::Order::Middle).fixed_pos(area.min).constrain(false).show(
        &ctx,
        |ui| {
            ui.set_clip_rect(clip);
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            // The scrim, which takes every press under the dialog: `rgba(8, 10, 14, 0.55)`.
            let _ = child.interact(area, id.with("scrim"), egui::Sense::click_and_drag());
            child.painter().rect_filled(area, 0.0, crate::theme::rux_theme().interaction.scrim);
            rux::layer(&mut child, state, id.with("chrome"), area, |rux| {
                let chat = rux.theme().chat;
                let theme = rux.theme();
                rux.chrome.surface(
                    dialog,
                    rux.z(28.0),
                    rux::Fill::gradient(chat.card, dialog),
                    chat.floating(),
                );
                let inner = dialog.shrink2(Vec2::new(rux.z(20.0), 0.0));
                let mut top = dialog.top() + rux.z(20.0);
                // The title and the round cross.
                let close = Rect::from_min_size(
                    Pos2::new(inner.right() - rux.z(34.0), top),
                    Vec2::splat(rux.z(34.0)),
                );
                let style = rux.zs(rux::Style::sans(15.0).semibold());
                let title = rux.text(style, "Chat settings", theme.ink.i900);
                rux::text::draw_left_capitals(
                    rux.painter(),
                    Pos2::new(inner.left(), close.center().y),
                    title,
                    style,
                    theme.ink.i900,
                );
                let cross = rux::Mark::new(rux::Icon::X, 12.0).stroke(2.2);
                if rux::components::RoundButton::new(
                    rux::components::RoundContent::Mark(cross),
                    "Close settings",
                )
                .size(rux::components::RoundSize::Close)
                .show(rux, close)
                .clicked()
                {
                    acts.push(Act::ShowSettings(false));
                }
                top = close.bottom() + rux.z(22.0);
                // The model.
                let label = rux.text(rux.zs(rux::Style::sans(12.0)), "Model", theme.ink.i500);
                let label_height = label.size().y;
                rux.painter().galley(Pos2::new(inner.left(), top), label, theme.ink.i500);
                top += label_height + rux.z(10.0);
                let switch = Rect::from_min_size(
                    Pos2::new(inner.left(), top),
                    Vec2::new(inner.width(), rux.z(46.0)),
                );
                if names.is_empty() {
                    let none = rux.text(
                        rux.zs(rux::Style::sans(12.0)),
                        "No agent is set up. Add one on Settings, Agent-Chat.",
                        theme.ink.i400,
                    );
                    let at = Pos2::new(switch.left(), switch.center().y - none.size().y / 2.0);
                    rux.painter().galley(at, none, theme.ink.i400);
                } else if let Some(index) =
                    rux::components::PillSwitch::new(&names, chosen, "Model").show(rux, switch)
                {
                    if let Some(name) = names.get(index) {
                        acts.push(Act::Choose(name.clone()));
                    }
                }
                top = switch.bottom() + rux.z(22.0);
                // The context used.
                let well = Rect::from_min_size(
                    Pos2::new(inner.left(), top),
                    Vec2::new(inner.width(), rux.z(80.0)),
                );
                context_well(rux, well, used, window);
            });
        },
    );
    acts
}

/// Where the Chat settings dialog's foot is: just above the gear, which is the design's `padding-bottom:
/// 120px` measured from the pane's own gear rather than written as a number.
fn settings_foot(parts: &Parts<'_>, look: &Look<'_>, area: Rect) -> f32 {
    let scale = look.scale();
    let composer = composer::height(parts, look, area.width() - SIDE * 2.0 * scale);
    area.bottom() - SIDE * scale - composer - (BELOW_GEAR + GEAR + 12.0) * scale
}

/// The well in the Chat settings that says how much of the model's context is used, with a ring.
fn context_well(rux: &mut rux::Rux<'_>, well: Rect, used: Option<u64>, window: Option<u64>) {
    let chat = rux.theme().chat;
    let theme = rux.theme();
    rux.chrome.surface(well, rux.z(20.0), chat.well, chat.carved_sm());
    // A 52 point ring: `r="21"`, five points wide, the track in `#2c323b` and the used share in sky.
    let centre = Pos2::new(well.left() + rux.z(16.0 + 26.0), well.center().y);
    let radius = rux.z(21.0);
    let width = rux.z(5.0);
    rux.chrome.ring(centre, radius, width, chat.track);
    let share = context_share(used, window);
    if let Some(share) = share.filter(|share| *share > 0.0) {
        // Round ends reach half the stroke past the arc, so the arc is shortened by the stroke and what
        // is seen is exactly the share.
        let seen = std::f32::consts::TAU * radius * share;
        match seen > width {
            true => rux.chrome.arc(
                centre,
                radius,
                width,
                width / 2.0 / radius,
                (seen - width) / radius,
                chat.sky,
            ),
            false => rux.chrome.arc_square(centre, radius, width, 0.0, seen / radius, chat.sky),
        }
    }
    let middle = match share {
        Some(share) => format!("{:.0}%", share * 100.0),
        None => "\u{2013}".to_owned(),
    };
    let galley = rux.text(rux.zs(rux::Style::sans(12.0).semibold()), &middle, theme.ink.i900);
    rux::text::draw_centred(
        rux.painter(),
        Rect::from_center_size(centre, Vec2::splat(radius * 2.0)),
        galley,
        theme.ink.i900,
    );
    let left = centre.x + rux.z(26.0 + 16.0);
    let detail = context_words(used, window);
    let room = (well.right() - rux.z(16.0) - left).max(0.0);
    let title = rux.elided(rux.zs(rux::Style::sans(12.5)), "Context used", theme.ink.i900, room);
    let line = rux.elided(rux.zs(rux::Style::sans(11.0)), &detail, theme.ink.i400, room);
    let gap = rux.z(2.0);
    let block = title.size().y + gap + line.size().y;
    let top = well.center().y - block / 2.0;
    let title_height = title.size().y;
    rux.painter().galley(Pos2::new(left, top), title, theme.ink.i900);
    rux.painter().galley(Pos2::new(left, top + title_height + gap), line, theme.ink.i400);
}

/// The share of the context used, when both numbers are known.
pub fn context_share(used: Option<u64>, window: Option<u64>) -> Option<f32> {
    match (used, window) {
        (Some(used), Some(window)) if window > 0 => {
            Some((used as f32 / window as f32).clamp(0.0, 1.0))
        }
        _ => None,
    }
}

/// The line under "Context used": `76k of 200k tokens`, or what is not known.
pub fn context_words(used: Option<u64>, window: Option<u64>) -> String {
    match (used, window) {
        (Some(used), Some(window)) => format!("{} of {} tokens", tokens(used), tokens(window)),
        (Some(used), None) => {
            format!("{} tokens; the agent does not say its context size", tokens(used))
        }
        (None, _) => "Not measured until the agent answers".to_owned(),
    }
}

/// A token count the way the design writes one: `76k`, `200k`, `1.2M`.
pub fn tokens(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{}k", (count as f64 / 1000.0).round() as u64),
        _ => {
            let millions = count as f64 / 1_000_000.0;
            match (millions - millions.round()).abs() < 0.05 {
                true => format!("{}M", millions.round() as u64),
                false => format!("{millions:.1}M"),
            }
        }
    }
}

/// How wide a message's right click menu is. Three short rows, so it is a field menu's width.
const MENU_WIDTH: f32 = 200.0;

/// The right click menu over a message: copy the selection, copy the message, select all of it.
///
/// `task-2060` asks for the first of those in as many words. The other two are what a menu opened on
/// a block of text has to offer beside it: `Copy` is dimmed with nothing selected, which is the style
/// guide's distinction — a control that will apply the moment something is selected is dimmed, and
/// one that can never apply is absent.
///
/// **What `Copy` copies is the words as they are drawn**, out of the rendered markdown, so a heading
/// comes back without its hashes. `Copy Message` is the source, which is what the button beside the
/// bubble has always copied and what somebody pasting an answer back into a file wants.
fn message_menu(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let Some(menu) = parts.state.menu else {
        return Vec::new();
    };
    let mut acts = Vec::new();
    let _ = look;
    let key = format!("message-{}", menu.message);
    let selected = parts
        .state
        .selection
        .filter(|one| one.message == menu.message && !one.range.is_empty())
        .and_then(|one| parts.state.rendered.slice(&key, one.range.range()));
    // The message's own source, whether it is in the conversation or still waiting in the queue.
    let source = parts
        .session
        .chat
        .message(menu.message)
        .or_else(|| parts.queued.iter().find(|one| one.id == menu.message))
        .map(unluminous_chat::Message::text);
    let mut close = false;
    let popup = egui::Popup::new(
        egui::Id::new("agent-chat-message-menu"),
        ui.ctx().clone(),
        menu.at,
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .frame(
        egui::Frame::popup(ui.style())
            .fill(crate::theme::color::menu())
            .stroke(Stroke::new(1.0, crate::theme::color::control_border()))
            .inner_margin(6),
    )
    .width(MENU_WIDTH);
    if let Some(response) = popup.show(|ui| {
        if controls::menu_row(ui, "Copy", "", selected.is_some(), false, 0.0) {
            if let Some(text) = selected.clone() {
                acts.push(Act::Copy(text));
            }
            close = true;
        }
        if controls::menu_row(ui, "Copy Message", "", source.is_some(), false, 0.0) {
            if let Some(text) = source.clone() {
                acts.push(Act::Copy(text));
            }
            close = true;
        }
        if controls::menu_row(ui, "Select All", "", true, false, 0.0) {
            acts.push(Act::SelectAll(menu.message));
            close = true;
        }
    }) {
        close |= response.response.should_close();
    }
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if close {
        parts.state.menu = None;
    }
    acts
}

/// The header: a round history button, the conversation's name centred, and a round new-conversation
/// button, which is the design's `header` (`task-2235`).
///
/// **The model selector is not here any more.** It moved into the Chat settings dialog the gear opens,
/// as a pill switch. The history button stays pressed in, with its mark in sky, while the history is
/// open. Everything is sized by the pane's zoom.
fn header(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let button = HEADER * scale;
    let history = Rect::from_min_size(area.min, Vec2::splat(button));
    let new =
        Rect::from_min_size(Pos2::new(area.right() - button, area.top()), Vec2::splat(button));
    let open = parts.state.history_open;
    let (history_pressed, new_pressed) =
        kit::layer(parts.state, look, ui, "header", area, kit::SMALL_REACH, |rux| {
            let history_pressed =
                rux::components::RoundButton::mark(rux::Icon::Clock, 20.0, "Conversations")
                    .on(open)
                    .show(rux, history)
                    .clicked();
            let new_pressed =
                rux::components::RoundButton::mark(rux::Icon::Plus, 15.0, "New Conversation")
                    .show(rux, new)
                    .clicked();
            (history_pressed, new_pressed)
        });
    if history_pressed {
        acts.push(Act::ShowHistory(!open));
    }
    if new_pressed {
        acts.push(Act::New);
    }
    // **One line, centred, cut with an ellipsis** before it reaches either button: `font-size: 14px;
    // font-weight: 600; gap: 12px`.
    let room = Rect::from_min_max(
        Pos2::new(history.right() + 12.0 * scale, area.top()),
        Pos2::new(new.left() - 12.0 * scale, area.bottom()),
    );
    if room.width() > 8.0 {
        let painter = ui.painter_at(room.intersect(ui.clip_rect()));
        let style = rux::Style::sans(14.0 * scale).semibold();
        let ink = crate::theme::rux_theme().ink.i900;
        let name = parts.session.chat.display_name();
        let galley = rux::text::elided(&painter, style, name, ink, room.width());
        let at = Pos2::new(room.center().x - galley.size().x / 2.0, room.center().y);
        rux::text::draw_left_capitals(&painter, at, galley, style, ink);
        let _ = painter;
    }
    acts
}

/// The conversation: every message, scrolled, with the empty state when there is nothing.
fn conversation(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    // Copied out of the parts so the conversation can be read while the little the drawing remembers
    // is written into. Both are borrows of different fields, which is what makes this legal at all.
    let session: &unluminous_chat::Session = parts.session;
    // Asked for once and then cleared, which is what `reveal_caret` does: a jump that ran on every
    // frame would make the conversation impossible to scroll at all.
    let jump = std::mem::take(&mut parts.state.jump_to_bottom);
    // Where the list a wheel belongs to is, for the reason `PaneState::list_rect` gives.
    parts.state.list_rect = Some(area);
    let wheel = parts.state.wheel.take();
    if session.chat.messages.is_empty() && parts.queued.is_empty() {
        parts.state.scrolled = 0.0;
        parts.state.scrollable = 0.0;
        return welcome::show(parts, ui, look, area);
    }
    let mut acts = Vec::new();
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area));
    // Intersected rather than replaced, so a chat node scrolled half off the canvas does not draw its
    // transcript over whatever is beside the pane. See `agent_tasks::lanes::show`.
    body.set_clip_rect(area.intersect(ui.clip_rect()));
    // **The decoration is cut to the conversation, and it has to be.** A `Chrome` records absolute
    // rectangles into one canvas that covers the whole pane, so a bubble scrolled half out of view
    // recorded its whole surface and the canvas painted it over the header above. `egui`'s own clip
    // rectangle cannot reach the canvas; `Decor::Clip` is the one thing that can. Measured on a real
    // window: a message scrolled off the top was drawn across the pane's own name.
    //
    // Cut above and below only: the list is the pane's whole width since `task-2235`.
    look.chrome.clip(area, 0.0);
    let mut scroller = egui::ScrollArea::vertical()
        .id_salt("agent-chat-conversation")
        // **Stuck to the bottom while an answer is arriving, and unstuck the moment somebody scrolls
        // up** — which is `ChatPage.tsx`'s own `shouldAutoScroll` rule. egui's own stickiness does
        // exactly that: it follows while the view is already at the bottom and stops when it is not.
        .stick_to_bottom(true)
        // Nothing is done about dragging the contents to scroll them, and that is deliberate:
        // `ScrollSource::drag` is `OnTouch` by default, so a drag with a pointer already selects
        // words rather than scrolling — which is what `task-2060` asks for — and forcing it off
        // would take scrolling away from a touch screen to fix something that is not broken there.
        .auto_shrink([false, false]);
    if jump {
        // What stickiness will not do is go *back* to the bottom once somebody has scrolled away, and
        // sending, opening a conversation and starting a new one all have to.
        //
        // **A finite offset, not `f32::MAX`.** `task-1848` reported pressing Enter scrolling the
        // conversation to the *top*. `vertical_scroll_offset` is written straight into `state.offset.y`
        // before the pass and is only clamped against `max_offset` at the end of it, so for the whole of
        // the pass every piece of arithmetic that reads the offset — the bar's handle, the fade areas,
        // `paint_fade_areas_impl` — is working with 3.4e38. What it is asked for now is past the bottom of
        // the conversation as it was last frame and therefore lands at the bottom once clamped, while
        // being an ordinary number all the way through.
        //
        // `scrolled` is where the conversation was left, measured at the end of the previous frame, and
        // one pane's height past it is further than any single message can have added.
        scroller = scroller.vertical_scroll_offset(parts.state.scrolled.max(0.0) + area.height());
    } else if let Some(offset) = parts.state.scroll_to.take() {
        // A zoom moved everything, so the point that was under the pointer is put back under it. The same
        // one-shot shape, worked out by `AgentChat::zoomed`. `task-1771`.
        scroller = scroller.vertical_scroll_offset(offset.max(0.0));
    }
    let scrolled = scroller.show(&mut body, |ui| {
        // The wheel the window read over a canvas node, handed to `egui` the way a wheel it read itself
        // would have been. See `PaneState::wheel` for why it is a delta and not an offset, and
        // `ScrollAnimation::none` because a wheel moves the page now rather than gliding to it.
        if let Some(wheel) = wheel {
            ui.scroll_with_delta_animation(
                Vec2::new(0.0, wheel),
                egui::style::ScrollAnimation::none(),
            );
        }
        // The rows are inset by the pane's side padding inside a list as wide as the pane, so the shadows
        // of the bubbles and cards are not cut at the list's edge (`task-2235`).
        let side = SIDE * look.scale();
        let width = (area.width() - side * 2.0).max(40.0);
        // **A conversation shorter than the pane sits at its bottom**, next to the prompt, which is the
        // design's `justify-content: flex-end`. Measured from last frame's rows, so the room put above
        // them is never part of what it is worked out from.
        let spare = (area.height() - parts.state.rows_height).max(0.0);
        ui.add_space(spare);
        let rows_top = ui.cursor().top();
        ui.add_space(12.0 * look.scale());
        // The conversation, and then whatever was sent while this answer was arriving. A queued
        // question is **not** in the conversation — see `AgentChat::queued` for why — so it is drawn
        // after it, in the order it was sent, which is where it will be asked.
        let queued = parts.queued.iter().map(|one| (one, true));
        let messages = session.chat.messages.iter().map(|one| (one, false)).chain(queued);
        for row in rows_of(messages) {
            match row {
                Row::Said(one, waiting) => {
                    // Worked out once and handed to the drawing, because the height has to be known
                    // before the rectangle can be allocated and running it twice built the message's
                    // text twice. See `message::Shape`.
                    let shape =
                        message::shape(one, parts.state, look, width, waiting, false, Some(ui));
                    if shape.height <= 0.0 {
                        continue;
                    }
                    let (slot, _) = ui.allocate_exact_size(
                        Vec2::new(area.width(), shape.height),
                        egui::Sense::hover(),
                    );
                    let rect = slot.shrink2(Vec2::new(side, 0.0));
                    // **Only what can be seen is drawn**, which is `task-1666`'s rule and, here, also
                    // what keeps the decoration's canvas the size of the pane: a bubble scrolled a
                    // thousand points away would otherwise record shadows a thousand points outside it.
                    if rect.intersects(ui.clip_rect()) {
                        acts.extend(message::show(one, shape, parts.state, ui, look, rect));
                    }
                }
                Row::Tools(tools) => {
                    let height = message::run_height(&tools, parts.state, look, width);
                    let (slot, _) = ui
                        .allocate_exact_size(Vec2::new(area.width(), height), egui::Sense::hover());
                    let rect = slot.shrink2(Vec2::new(side, 0.0));
                    if rect.intersects(ui.clip_rect()) {
                        acts.extend(message::run_show(&tools, parts.state, ui, look, rect));
                    }
                }
            }
            ui.add_space(GAP * look.scale());
        }
        ui.cursor().top() - rows_top
    });
    parts.state.rows_height = scrolled.inner;
    // The canvases of components that were not drawn this frame are given back. Once a frame, after
    // every row has had its turn, which is what `rux::RuxState::end_frame` asks for.
    if let Some(kept) = &parts.state.blocks_rux {
        kept.end_frame();
    }
    // Where the conversation was left, so a zoom can put it back where it was. See `PaneState::scrolled`.
    parts.state.scrolled = scrolled.state.offset.y;
    // And how far it could be scrolled, so `plugins view agent-chat` can answer whether it is at the
    // bottom. Never negative: a conversation shorter than the pane has nowhere to go.
    parts.state.scrollable = (scrolled.content_size.y - area.height()).max(0.0);
    look.chrome.unclip();
    acts
}

/// One row of the conversation: something said, or a run of tool calls between two things said.
#[derive(Debug)]
enum Row<'a> {
    /// A message, drawn without its tool calls, and whether it is a question waiting its turn.
    Said(&'a unluminous_chat::Message, bool),
    Tools(Vec<&'a unluminous_chat::model::ToolCall>),
}

/// The conversation as the rows it is drawn in.
///
/// `task-2193`: the tool calls an answer made are gathered into one run per gap between two things
/// said, however many rounds they took — a model that calls three tools, says nothing, calls four more
/// and then answers made seven blocks in three messages, and that is one run of seven. A message with
/// nothing in it but its calls is not a row of its own. A tool result is the copy that goes back up the
/// wire and is drawn inside the call it answers, so it is never a row.
fn rows_of<'a>(
    messages: impl Iterator<Item = (&'a unluminous_chat::Message, bool)>,
) -> Vec<Row<'a>> {
    use unluminous_chat::Role;
    let mut rows = Vec::new();
    let mut run: Vec<&unluminous_chat::model::ToolCall> = Vec::new();
    let flush = |run: &mut Vec<&'a unluminous_chat::model::ToolCall>, rows: &mut Vec<Row<'a>>| {
        if !run.is_empty() {
            rows.push(Row::Tools(std::mem::take(run)));
        }
    };
    for (one, waiting) in messages {
        if one.role == Role::Tool {
            continue;
        }
        let says_something = waiting
            || one.role != Role::Assistant
            || !one.text().trim().is_empty()
            || !one.thinking.is_empty()
            || one.failure.is_some()
            || one
                .parts
                .iter()
                .any(|part| matches!(part, unluminous_chat::model::Part::Picture { .. }));
        if says_something {
            flush(&mut run, &mut rows);
            rows.push(Row::Said(one, waiting));
        }
        run.extend(one.tools.iter());
    }
    flush(&mut run, &mut rows);
    rows
}

/// The conversations kept, drawn over the conversation area.
fn history_list(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let session: &unluminous_chat::Session = parts.session;
    let history = parts.history;
    let mut acts = Vec::new();
    let scale = look.scale();
    parts.state.list_rect = Some(area);
    let put = parts.state.scroll_to.take();
    let wheel = parts.state.wheel.take();
    let painter = ui.painter_at(area);
    if history.is_empty() {
        controls::centred_line(
            &painter,
            area,
            area.top() + 20.0 * scale,
            "No conversations yet.",
            look.font_size * 0.85,
            look.palette.text_dim,
        );
    }
    // **Two lines a row**, `task-2219`: the name, and under it the agent, how many messages and how long
    // ago, which is what tells two conversations with similar names apart. The chosen row is a card, a
    // hovered row is a wash, and the cross that removes a conversation is drawn only on the row under the
    // pointer, because a column of crosses is a column of invitations to delete something.
    let title_size = look.font_size * 0.85;
    let detail_size = look.font_size * 0.72;
    let row = (title_size * 1.3 + detail_size * 1.35 + 16.0 * scale).max(look.row_height * scale);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area));
    body.set_clip_rect(area.intersect(ui.clip_rect()));
    let mut scroller =
        egui::ScrollArea::vertical().id_salt("agent-chat-history").auto_shrink([false, false]);
    if let Some(offset) = put {
        scroller = scroller.vertical_scroll_offset(offset.max(0.0));
    }
    let scrolled = scroller.show(&mut body, |ui| {
        if let Some(wheel) = wheel {
            ui.scroll_with_delta_animation(
                Vec2::new(0.0, wheel),
                egui::style::ScrollAnimation::none(),
            );
        }
        ui.add_space(4.0 * scale);
        for one in history {
            let (slot, _) = ui.allocate_exact_size(
                Vec2::new(area.width(), row + 4.0 * scale),
                egui::Sense::hover(),
            );
            if !slot.intersects(ui.clip_rect()) {
                continue;
            }
            let rect = Rect::from_min_size(slot.min, Vec2::new(slot.width(), row))
                .shrink2(Vec2::new(4.0 * scale, 0.0));
            let chosen = one.id == session.chat.id;
            let response = ui.interact(
                rect,
                ui.id().with(("agent-chat-history", &one.id)),
                egui::Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Conversation: {}", one.name),
                )
            });
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let painter = ui.painter_at(rect.expand(1.0));
            let corner = CornerRadius::same((8.0 * scale).round().clamp(0.0, 255.0) as u8);
            if chosen {
                painter.rect(
                    rect,
                    corner,
                    look.ground(look.palette.board_card),
                    Stroke::new(1.0, look.palette.divider),
                    egui::StrokeKind::Inside,
                );
                // A short bar of the accent against the card's left edge says which one is open.
                let bar = Rect::from_center_size(
                    Pos2::new(rect.left() + 1.5 * scale, rect.center().y),
                    Vec2::new(3.0 * scale, rect.height() * 0.46),
                );
                painter.rect_filled(bar, CornerRadius::same(2), look.palette.accent);
            } else if response.hovered() {
                painter.rect_filled(
                    rect,
                    corner,
                    crate::theme::color::hover_wash().gamma_multiply(0.05),
                );
            }
            let hovered = response.hovered()
                || ui.rect_contains_pointer(rect) && ui.ctx().pointer_hover_pos().is_some();
            let cross = Rect::from_center_size(
                Pos2::new(rect.right() - 14.0 * scale, rect.center().y),
                Vec2::splat(20.0 * scale),
            );
            let text_right = match hovered {
                true => cross.left() - 4.0 * scale,
                false => rect.right() - 10.0 * scale,
            };
            let left = rect.left() + 12.0 * scale;
            let clipped = painter
                .with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(text_right, rect.max.y)));
            let block = title_size * 1.3 + detail_size * 1.35;
            let top = rect.center().y - block / 2.0;
            clipped.text(
                Pos2::new(left, top),
                egui::Align2::LEFT_TOP,
                &one.name,
                egui::FontId::proportional(title_size),
                match chosen {
                    true => look.palette.text_strong,
                    false => look.palette.text,
                },
            );
            let messages = match one.messages {
                1 => "1 message".to_owned(),
                count => format!("{count} messages"),
            };
            let detail = format!(
                "{}  \u{00B7}  {}  \u{00B7}  {}",
                one.provider,
                messages,
                welcome::ago(one.changed, now)
            );
            clipped.text(
                Pos2::new(left, top + title_size * 1.3),
                egui::Align2::LEFT_TOP,
                detail,
                egui::FontId::proportional(detail_size),
                look.palette.text_faint,
            );
            if response.clicked() {
                acts.push(Act::Open(one.id.clone()));
            }
            if hovered
                && crate::components::controls::icon_button_at(
                    ui,
                    cross,
                    &format!("Remove conversation: {}", one.name),
                    icon::cross,
                    scale,
                )
            {
                acts.push(Act::Remove(one.id.clone()));
            }
        }
    });
    // Read back where the list ended up and how far it can go, which is the pair the wheel over a canvas
    // node is measured from. See `PaneState::scrolled`.
    parts.state.scrolled = scrolled.state.offset.y;
    parts.state.scrollable = (scrolled.content_size.y - area.height()).max(0.0);
    acts
}

/// Do what the drawing reported, and answer what the window has to do.
fn apply(chat: &mut AgentChat, acts: Vec<Act>) -> Vec<Request> {
    let mut requests = Vec::new();
    for act in acts {
        match act {
            Act::Send => {
                // **A notice rather than the status bar.** `task-1848` reported this as "nothing happens,
                // no error": every reason `send` refuses — nothing typed, no endpoint configured, a
                // program that is not installed, a key that is not set — went to the status bar, which is
                // a sentence in the smallest text at the far bottom edge of the window, replaced by
                // whatever was reported next. It is the one control on this pane and the person is looking
                // straight at it.
                if let Err(problem) = chat.send() {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Stop => chat.stop(),
            Act::New => {
                chat.new_conversation();
                chat.ui.history_open = false;
            }
            Act::Dropped(path) => {
                // Same reason as `Act::Send`: a picture that could not be attached is a thing somebody
                // just did, and they are watching the pane rather than the status bar.
                if let Err(problem) = chat.attach(&path) {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Paste => requests.push(Request::ClipboardPicture {
                id: crate::services::agent_chat::CLIPBOARD.to_owned(),
            }),
            Act::Open(id) => {
                if let Err(problem) = chat.open_conversation(&id) {
                    requests.push(Request::Message(problem));
                }
                chat.ui.history_open = false;
            }
            Act::Remove(id) => {
                if let Err(problem) = chat.remove_conversation(&id) {
                    requests.push(Request::Message(problem));
                }
            }
            Act::Choose(name) => {
                if let Err(problem) = chat.configuration_mut().choose(&name) {
                    requests.push(Request::Message(problem));
                }
                if let Err(problem) = chat.save_the_configuration() {
                    requests.push(Request::Message(problem));
                }
            }
            Act::Detach(id) => chat.remove_attachment(id),
            Act::Copy(text) if !text.is_empty() => requests.push(Request::Copy(text)),
            Act::Copy(_) => {}
            Act::ShowHistory(open) => {
                chat.ui.history_open = open;
            }
            Act::ShowSettings(open) => {
                chat.ui.settings_open = open;
            }
            Act::ToggleTool(id) => match chat.ui.opened_tools.iter().position(|one| *one == id) {
                Some(at) => {
                    chat.ui.opened_tools.remove(at);
                }
                None => chat.ui.opened_tools.push(id),
            },
            Act::ToggleGroup(key) => {
                if !chat.ui.opened_groups.remove(&key) {
                    chat.ui.opened_groups.insert(key);
                }
            }
            Act::SelectAll(id) => chat.select_the_whole_message(id),
            Act::SendWords(words) => {
                // Through the same path as typing and pressing send, so it queues behind an answer
                // that is still arriving and refuses with a notice when nothing can answer it.
                if let Err(problem) = chat.send_words(&words) {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Fill(words) => {
                chat.draft = words;
                chat.ui.focus_the_prompt = true;
            }
            Act::OpenFile(path, line) => match chat.open_a_file(&path, line) {
                Ok(asked) => requests.extend(asked),
                Err(problem) => requests.push(Request::Notice {
                    text: problem,
                    kind: crate::components::toast::Kind::Problem,
                }),
            },
            Act::ToggleThinking(id) => {
                match chat.ui.opened_thinking.iter().position(|one| *one == id) {
                    Some(at) => {
                        chat.ui.opened_thinking.remove(at);
                    }
                    None => chat.ui.opened_thinking.push(id),
                }
            }
        }
    }
    requests
}

/// Whether the composer was pasted into this frame.
///
/// Only while a field in this pane has the keyboard, so a paste meant for the editing area is not
/// taken.
///
/// **It is the key going back up that says so, and it took reading `egui-winit` to find out why.**
/// This used to watch for an *empty* `egui::Event::Paste`, on the reasoning that a picture is not text
/// and would therefore arrive as a paste with nothing in it. `egui-winit` does not do that. Its
/// `on_keyboard_input` recognises the paste chord, reads the clipboard's **text**, and pushes
/// `Event::Paste` only `if !contents.is_empty()` — then returns, so the `V` key press is swallowed as
/// well. With a picture on the clipboard `get_text()` fails outright and the frame carries **no event
/// at all**. So the condition that was being watched for could never once have happened, and pasting a
/// picture into this pane has never worked. `task-1771`.
///
/// The `return` is only taken while the key is **down**. The release comes back through the ordinary
/// path as `Event::Key { key: V, pressed: false, modifiers }`, with the modifier still held, which is
/// the one report of the chord that reaches Unluminous. Asking the window for a picture on the way up is a
/// keystroke late and nobody can tell; a clipboard with no picture on it answers `None` and nothing
/// happens, which is what makes it safe to ask after an ordinary text paste as well.
fn pasting(ui: &egui::Ui, in_the_composer: bool) -> bool {
    // **This pane's own field, not any field.** `text_box_has_the_keyboard` answers whether *some* box in
    // the window has the keys, and with this pane showing beside a focused explorer filter that was enough
    // to attach the clipboard's picture to a conversation nobody was typing into. The composer says whether
    // it has them — see `agent_chat::PaneState::prompt_focused`.
    if !in_the_composer {
        return false;
    }
    ui.ctx().input(|input| is_a_paste_chord(&input.events))
}

/// Whether this frame's events carry the paste chord as Unluminous can actually see it.
///
/// Its own function so a test can hold the two event shapes side by side: the one that was being
/// watched for and never arrives, and the one that does.
fn is_a_paste_chord(events: &[egui::Event]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. }
                if modifiers.command
        )
    })
}

/// Whether a file the window manager dropped belongs to the pane at `area`.
///
/// `None` means the platform reported no pointer through the drag, which Windows does not: a file is
/// carried over a window through OLE and no cursor movement is sent at all, so `egui` can still be
/// holding a position from before the drag started. The system is asked first, and a drop that still
/// cannot be placed belongs here rather than nowhere.
fn belongs_here(pointer: Option<Pos2>, area: Rect) -> bool {
    pointer.is_none_or(|at| area.contains(at))
}

/// Every picture the window manager dropped on this pane this frame.
///
/// `egui` collects what was dropped and hands over a path; a file that is not a picture is left
/// alone rather than refused, because a drag that landed on the wrong pane should do nothing rather
/// than say something.
///
/// **`area` is the pane's own rectangle, taken before anything was drawn in it.** It used to ask the `Ui`
/// for what was left, which is a different rectangle by the time the composer has been laid out.
///
/// **A drop with no pointer is this pane's.** Windows carries a file over a window through OLE, which
/// reports no cursor movement at all, so `egui` can be holding a position from before the drag began — and
/// gating on it silently threw the picture away. The folder pane reads `dropped_files` too since
/// `task-2194`, which is why the system is asked where the pointer is before this falls back.
///
/// **`hover_pos` alone, and not `latest_pos` behind it.** The zoom asks for both because a wheel event
/// arrives with no pointer at all and the *last place it was seen* is still the honest answer. A drop is the
/// opposite case: `latest_pos` during an OLE drag is a position from before the drag began, so falling back
/// to it is falling back to a stale answer that reads as a confident one — which threw the picture away
/// exactly where this was meant to stop it. Found by the `task-1771` review.
fn dropped_pictures(ui: &egui::Ui, area: Rect) -> Vec<std::path::PathBuf> {
    // **Asked of the system first** (`task-2194`), which knows where the pointer is during a drop when
    // egui does not. Since the folder pane takes dropped files as well, a drop nobody could place would
    // otherwise be attached here and copied into a folder there at once.
    let pointer = crate::services::system_files::pointer(ui.ctx())
        .or_else(|| ui.ctx().input(|input| input.pointer.hover_pos()));
    if !belongs_here(pointer, area) {
        return Vec::new();
    }
    ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|dropped| dropped.path().to_path_buf())
            .filter(|path| {
                matches!(
                    path.extension()
                        .and_then(|kind| kind.to_str())
                        .map(str::to_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "gif" | "webp")
                )
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2193`: the calls between two things said are one run, however many rounds they took, and a
    /// message that is nothing but calls is not a row of its own.
    #[test]
    fn the_tool_calls_between_two_things_said_are_one_run() {
        use unluminous_chat::model::{Message, ToolCall};
        use unluminous_chat::Role;
        let question = Message::said(1, Role::User, "Make three tickets");
        let mut first = Message::said(2, Role::Assistant, "Reading the board first.");
        first.tools.push(ToolCall::new("a", "unluminous_plugins", "{}"));
        first.tools.push(ToolCall::new("b", "unluminous_plugins", "{}"));
        let result = Message::said(3, Role::Tool, "{}");
        let mut quiet = Message::new(4, Role::Assistant);
        quiet.tools.push(ToolCall::new("c", "unluminous_editor", "{}"));
        let answer = Message::said(5, Role::Assistant, "Done: task-1, task-2 and task-3.");
        let messages = [&question, &first, &result, &quiet, &answer];
        let rows = rows_of(messages.iter().map(|one| (*one, false)));
        let shape: Vec<String> = rows
            .iter()
            .map(|row| match row {
                Row::Said(one, _) => format!("said {}", one.id),
                Row::Tools(tools) => format!(
                    "run {}",
                    tools.iter().map(|tool| tool.id.as_str()).collect::<Vec<_>>().join("")
                ),
            })
            .collect();
        assert_eq!(shape, vec!["said 1", "said 2", "run abc", "said 5"]);
    }

    /// `task-1771`: pasting a picture into the composer had never worked, and this is why.
    #[test]
    fn a_paste_is_seen_on_the_key_going_up_and_never_as_an_empty_paste_event() {
        let chord = |pressed: bool, command: bool| egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers { command, ..Default::default() },
        };
        // What this used to watch for. `egui-winit` pushes `Event::Paste` only when the clipboard held
        // **text**, so with a picture on it the frame carries nothing at all — and the condition below is
        // one no window ever satisfies.
        assert!(!is_a_paste_chord(&[egui::Event::Paste(String::new())]));
        // The press is swallowed by the same early return that reads the clipboard.
        assert!(!is_a_paste_chord(&[chord(true, true)]));
        // The release is not, and the modifier is still down on it. This is the one report that arrives.
        assert!(is_a_paste_chord(&[chord(false, true)]));
        // A `V` with nothing held is a letter somebody typed.
        assert!(!is_a_paste_chord(&[chord(false, false)]));
    }

    /// A drop the platform cannot place is this pane's, because nothing else in Unluminous wants one.
    #[test]
    fn a_drop_with_no_pointer_still_lands_on_the_pane() {
        let area = Rect::from_min_size(Pos2::new(100.0, 40.0), Vec2::new(400.0, 600.0));
        assert!(belongs_here(Some(Pos2::new(200.0, 300.0)), area), "over the pane");
        assert!(!belongs_here(Some(Pos2::new(20.0, 300.0)), area), "over something else");
        // Windows carries a file through OLE and sends no cursor movement, so the last position `egui`
        // holds can be from before the drag began. Refusing there threw the picture away silently.
        assert!(belongs_here(None, area), "nowhere in particular");
    }
}
