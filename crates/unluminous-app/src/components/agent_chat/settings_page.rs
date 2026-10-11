//! The Agent-Chat page in the Settings window.
//!
//! The ticket's fourth ask: "a config in settings to allow configure url for Claude, codex etc." So
//! an endpoint is a **row in a list** rather than a constant in the binary, and the rows that ship
//! are only the rows that are there the first time this page is opened. Every field of every one is
//! editable, rows can be added and taken away, and one of them is the one that is used.
//!
//! Three cards from `components::settings_kit`, which every page of the Settings window is drawn with, in the
//! Settings design's layout (`task-2246`): Endpoints, Answering and Conversation. It scrolls inside the room
//! the Settings window gives a page.
//!
//! ## An endpoint is a card of its own
//!
//! - **The one in use is raised out of the Endpoints card and says `In use`** where the others offer `Use`, and
//!   the others are carved into it, so which endpoint answers is readable without comparing buttons.
//! - **The five wire shapes are a carved track with a row and a label of their own**, `Speaks`, because they are
//!   the whole of what a row is and a list that had to be opened would hide the answer.
//! - **A shape that cannot work here is not offered.** *"why do we show codex option if its not on the
//!   machine?"* (`task-2003`). `AgentChat::shapes_available` walks `PATH` once every few seconds, and a program
//!   that is not installed is absent rather than dimmed. The shape a row already uses is always drawn, so a row
//!   is never left naming something missing from its own track.
//!
//! ## A row is a program or an address, and it draws the fields that apply
//!
//! The rows that run the **command-line agent** installed on this machine need a program and how much
//! it may do — and a URL, a key and a token budget are three fields that mean nothing there. They are
//! **absent** rather than dimmed for such a row, which is the same rule again.
//!
//! ## No key is drawn and no key is written
//!
//! A row that sends to an address says `set` or `not set` and never the value, because a page that
//! showed it would be a page somebody screenshots. What is typed here is the **name of an
//! environment variable**; the value is read at the moment a request is sent and is never held,
//! written down or logged. That is `services::agent_tasks::keychain`'s rule, and it is what makes it
//! safe for this file to be copied between machines. A row that runs a program has no key at all —
//! the agent holds its own.

use egui::{Pos2, Rect, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Fill, Rux, Style};

use unluminous_chat::provider::{Provider, Wire, DEFAULT_MAX_TOKENS, WIRES};

use crate::components::settings_kit::{self as kit, Column, Kind as ButtonKind};
use crate::services::agent_chat::AgentChat;
use crate::services::plugin_ui::{Look, Request};

/// An endpoint card's own padding and the gap between the things in it: `padding: 14px 16px; gap: 12px`.
const ENDPOINT_PAD_Y: f32 = 14.0;
const ENDPOINT_PAD_X: f32 = 16.0;
const ENDPOINT_GAP: f32 = 12.0;
/// How round an endpoint card is.
const ENDPOINT_RADIUS: f32 = 20.0;
/// The name field, the `Use` and `Remove` buttons, and the label column: `grid-template-columns: 96px 1fr;
/// gap: 10px 14px`.
const ENDPOINT_HEAD: f32 = 34.0;
const ENDPOINT_LABEL: f32 = 96.0;
const ENDPOINT_ROW: f32 = 34.0;
const ENDPOINT_ROW_GAP: f32 = 10.0;
/// The line that says whether it can answer.
const ENDPOINT_STATUS: f32 = 18.0;

/// Draw the page inside the rectangle every page gets.
pub fn show(chat: &mut AgentChat, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "agent-chat-settings", |ui, column| cards(chat, ui, column))
}

/// The page's three cards.
fn cards(chat: &mut AgentChat, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    // **Asked once a frame at most, and cached for five seconds under that.** For a program row the answer is a
    // walk of `PATH`; asked once a row once a frame it was a directory listing inside the draw. See
    // `AgentChat::readiness`.
    let refusals = chat.readiness();
    let shapes = chat.shapes_available();
    let mut configuration = chat.configuration().clone();
    let mut changed = false;
    let chosen = configuration.provider().map(|one| one.name.clone()).unwrap_or_default();

    kit::card(
        ui,
        column,
        "Endpoints",
        Some(
            "What answers. claude-cli and codex-cli run the agent already installed on this machine: they need \
             no key, they bring their own tools, and they read the open project's own instructions. openai, \
             anthropic and responses send to a URL instead; for those a key is never stored by Unluminous, so \
             name the environment variable it is in.",
        ),
        |card| {
            card.grow(|rux, column| {
                let only_one = configuration.providers.len() == 1;
                let mut removing: Option<usize> = None;
                let mut pen = column.pen;
                for index in 0..configuration.providers.len() {
                    // **A child `Ui` an endpoint, carrying its own id salt.** Every field and button here is
                    // named by its word, so three rows each holding a `Use`, a `Remove` and an `openai` would be
                    // three widgets sharing one id, which makes the second row's buttons unclickable.
                    let mut child = rux.ui.new_child(
                        egui::UiBuilder::new().id_salt(("agent-chat-endpoint", index)),
                    );
                    let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
                    let why_not = refusals.get(index).cloned().flatten();
                    let at = EndpointRow {
                        left: column.left,
                        right: column.right,
                        top: pen,
                        index,
                        chosen: &chosen,
                        why_not,
                        shapes: &shapes,
                        only_one,
                    };
                    let (bottom, act) = endpoint(&mut inner, &mut configuration, at);
                    pen = bottom + 14.0;
                    match act {
                        Some(Act::Remove) => removing = Some(index),
                        Some(Act::Use) => {
                            let name = configuration.providers[index].name.clone();
                            configuration.chosen = name;
                            changed = true;
                        }
                        Some(Act::Changed) => changed = true,
                        None => {}
                    }
                }
                // The last one is never taken away: a pane with no endpoint cannot do anything, and there would
                // then be no row to type a new one into. The button is **absent** on a single row.
                if let Some(index) = removing {
                    if configuration.providers.len() > 1 {
                        configuration.providers.remove(index);
                        changed = true;
                    }
                }
                let (pressed, rect) = kit::add_button(rux, Pos2::new(column.left, pen), "Add endpoint");
                if pressed {
                    let mut new = Provider::defaults()[2].clone();
                    new.name = unused_name(&configuration.providers);
                    configuration.providers.push(new);
                    changed = true;
                }
                ((), rect.bottom())
            });
        },
    );

    // **Which card is drawn depends on what the chosen row is**, because the two transports ask opposite
    // questions. An agent is a whole program with its own tools and its own sandbox, so the only question is how
    // much it may do; an endpoint is a model on the end of a socket, so the question is which of Unluminous's
    // own commands it may ask for.
    let an_agent = configuration.provider().is_some_and(|one| one.is_a_program());
    match an_agent {
        true => kit::card(
            ui,
            column,
            "Answering",
            Some(
                "The chosen row is a program on this machine, so it answers with its own tools, its own sandbox \
                 and its own account. It runs in the project this window has open and reads that project's own \
                 instructions.",
            ),
            |card| {
                let said = permission_said(configuration.permission);
                let names: Vec<&str> = unluminous_chat::PERMISSIONS.to_vec();
                let chosen = names.iter().position(|named| configuration.permission.name() == *named);
                card.row("May", Some(said), |row| {
                    if let Some(index) = row.segmented("May", &names, chosen, 72.0, false) {
                        if let Some(permission) = unluminous_chat::Permission::from_name(names[index]) {
                            configuration.permission = permission;
                            changed = true;
                        }
                    }
                });
            },
        ),
        false => kit::card(ui, column, "Answering", None, |card| {
            changed |= card.switch(
                "Let the model use Unluminous's own commands",
                Some(
                    "On unless you turn it off. The model is offered Unluminous's whole command catalogue as \
                     tools, so it can open a file, read the git status or run a search, through exactly the code \
                     a menu entry runs. The commands that run a program of the model's choosing are the switch \
                     below, and that one is off. The same switch is the first button in the composer.",
                ),
                &mut configuration.tools,
            );
            changed |= card.switch(
                "\u{2026}including the ones that run a program",
                Some(
                    "A second switch, because typing into a terminal, starting a run configuration and installing \
                     a debug adapter hand this machine's shell to whatever is on the far end of that URL.",
                ),
                &mut configuration.shell,
            );
            let mut limit = configuration.tool_limit.to_string();
            card.row(
                "Tool rounds",
                Some("How many times in one turn the model may ask for tools before the pane stops asking again."),
                |row| {
                    if row.text("Tool rounds", &mut limit, "100", 96.0) {
                        if let Ok(rounds) = limit.trim().parse::<u32>() {
                            if rounds > 0 {
                                configuration.tool_limit =
                                    rounds.min(crate::services::agent_chat::LARGEST_TOOL_LIMIT);
                                changed = true;
                            }
                        }
                    }
                },
            );
        }),
    }

    kit::card(ui, column, "Conversation", None, |card| {
        let mut system = configuration.system.clone();
        card.row(
            "System",
            Some(
                "Added after Unluminous's own line, which says which file is showing. The file's text is never \
                 sent by Unluminous.",
            ),
            |row| {
                if row.text("System", &mut system, "nothing", 340.0) {
                    configuration.system = system.clone();
                    changed = true;
                }
            },
        );
        let mut history = configuration.history.to_string();
        card.row("History", Some("How many conversations are kept."), |row| {
            if row.text("History", &mut history, "20", 96.0) {
                if let Ok(count) = history.trim().parse::<usize>() {
                    if count > 0 {
                        configuration.history = count.min(500);
                        changed = true;
                    }
                }
            }
        });
    });

    if changed {
        *chat.configuration_mut() = configuration;
        // A row that was just edited may name a different program, so the next frame asks again rather than
        // repeating what it said about the one it used to name.
        chat.readiness_may_have_changed();
        if let Err(problem) = chat.save_the_configuration() {
            requests.push(Request::Message(problem));
        }
    }
    requests
}

/// What each agent's answer to a permission really is, because the two are not the same strength and one
/// label over both would promise something `claude` does not enforce: `codex` is put in an operating system
/// sandbox, and `claude` is told a policy about its own tools while its process, its hooks and its plugins are
/// not limited at all.
fn permission_said(permission: unluminous_chat::Permission) -> &'static str {
    match permission {
        unluminous_chat::Permission::Read => {
            "Look and answer, changing nothing. codex runs in a read-only sandbox; claude refuses any tool of its \
             own that would change something."
        }
        unluminous_chat::Permission::Edit => {
            "Change files in the project this window has open. codex is sandboxed to it; claude accepts its own \
             edits without asking."
        }
        unluminous_chat::Permission::Full => {
            "Anything at all: edit any file and run any command, with no sandbox and nothing to ask. This is what \
             ships, because the agent on the other end is the one already signed in on this machine and trusted \
             with this checkout."
        }
    }
}

/// What one endpoint's card reported.
enum Act {
    Changed,
    Use,
    Remove,
}

/// Where one endpoint's card draws, and which of `configuration.providers` it is.
struct EndpointRow<'a> {
    left: f32,
    right: f32,
    /// How far down the page this card starts.
    top: f32,
    /// Which provider in `configuration.providers` this card is.
    index: usize,
    /// The name of the provider that is chosen, so the card can say whether it is the one in use.
    chosen: &'a str,
    /// Why this endpoint cannot be reached right now, if it cannot.
    why_not: Option<String>,
    /// Which wire shapes could be used on this machine. See [`AgentChat::shapes_available`].
    shapes: &'a [&'static str],
    /// Whether this is the only endpoint there is, in which case there is no `Remove`.
    only_one: bool,
}

/// The fields an endpoint draws under its shapes: a program row needs a program and a model, and a row that
/// sends to an address needs the address, a model, where its key is and a token budget.
///
/// **Absent rather than dimmed** for a row they cannot apply to: a URL, a key and a token budget mean nothing
/// to an agent that holds its own key and picks its own model.
fn fields_of(provider: &Provider) -> Vec<(&'static str, f32)> {
    match provider.is_a_program() {
        true => vec![("Program", 220.0), ("Model", 220.0)],
        false => {
            vec![("URL", 360.0), ("Model", 220.0), ("Key from", 220.0), ("Most tokens", 110.0)]
        }
    }
}

/// One endpoint: its name, what it speaks, where it is, and whether it can answer. Answers where it ended.
///
/// The one in use is raised out of the card on the chip surface and says `In use` where the others offer
/// `Use`, so which endpoint answers is readable without comparing buttons; the others are carved in.
fn endpoint(
    rux: &mut Rux<'_>,
    configuration: &mut crate::services::agent_chat::Configuration,
    at: EndpointRow<'_>,
) -> (f32, Option<Act>) {
    let EndpointRow { left, right, top, index, chosen, why_not, shapes, only_one } = at;
    let theme = rux.theme();
    let chat = theme.chat;
    let mut act = None;
    let in_use = configuration.providers[index].name == chosen;
    let fields = fields_of(&configuration.providers[index]);
    let rows = 1 + fields.len();
    let height = ENDPOINT_PAD_Y * 2.0
        + ENDPOINT_HEAD
        + ENDPOINT_GAP
        + rows as f32 * ENDPOINT_ROW
        + (rows - 1) as f32 * ENDPOINT_ROW_GAP
        + ENDPOINT_GAP
        + ENDPOINT_STATUS;
    let card = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, top + height));
    match in_use {
        true => rux.chrome.surface(
            card,
            ENDPOINT_RADIUS,
            Fill::gradient(chat.chip, card),
            chat.raised_chip(),
        ),
        false => rux.chrome.surface(card, ENDPOINT_RADIUS, chat.well, chat.carved_sm()),
    }
    let inside_left = card.left() + ENDPOINT_PAD_X;
    let inside_right = card.right() - ENDPOINT_PAD_X;
    let mut pen = card.top() + ENDPOINT_PAD_Y;

    // The name, and the two things that can be done to the row, on one line.
    let mut name = configuration.providers[index].name.clone();
    let name_at = Rect::from_min_size(Pos2::new(inside_left, pen), Vec2::new(140.0, ENDPOINT_HEAD));
    if kit::text_well_styled(
        rux,
        name_at,
        "Endpoint name",
        &mut name,
        "",
        Style::sans(12.5).semibold(),
        false,
    )
    .changed()
    {
        configuration.providers[index].name = name.trim().to_owned();
        act = Some(Act::Changed);
    }
    let mut x = inside_right;
    if !only_one {
        let width = kit::button_width(rux, "Remove", false) - 6.0;
        let remove = Rect::from_min_size(Pos2::new(x - width, pen + 2.0), Vec2::new(width, 30.0));
        if kit::button(rux, remove, "Remove", ButtonKind::Ghost) {
            act = Some(Act::Remove);
        }
        x = remove.left() - 8.0;
    }
    if in_use {
        // **A statement rather than a button that does nothing.** Which endpoint answers is the one thing a
        // person comes to this page to change.
        let words = rux.measure(Style::sans(11.5), "In use").x;
        let badge = Rect::from_min_size(
            Pos2::new(x - (words + 10.0 + 11.0 + 6.0 + 12.0), pen + 4.0),
            Vec2::new(words + 10.0 + 11.0 + 6.0 + 12.0, 26.0),
        );
        rux.chrome.surface(badge, 13.0, chat.well, chat.carved_sm());
        rux.mark(
            Mark::new(Icon::Tick, 11.0),
            Pos2::new(badge.left() + 10.0 + 5.5, badge.center().y),
            chat.faster,
        );
        let galley = rux.text(Style::sans(11.5), "In use", chat.faster);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(badge.left() + 10.0 + 11.0 + 6.0, badge.center().y),
            galley,
            Style::sans(11.5),
            chat.faster,
        );
        let response =
            rux.ui.interact(badge, rux.ui.id().with("agent-chat-in-use"), egui::Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "In use"));
    } else {
        let width = kit::button_width(rux, "Use", false) - 2.0;
        let use_at = Rect::from_min_size(Pos2::new(x - width, pen + 2.0), Vec2::new(width, 30.0));
        if kit::button(rux, use_at, "Use", ButtonKind::Chip) {
            act = Some(Act::Use);
        }
    }
    pen += ENDPOINT_HEAD + ENDPOINT_GAP;

    // Which shape it speaks, as a track rather than a list: there are five and they are the whole of what this
    // row is, so a list that has to be opened would hide the answer. A shape that cannot work on this machine is
    // not offered, and the one this row already uses always is.
    let here = configuration.providers[index].wire;
    let offered: Vec<&str> = WIRES
        .iter()
        .copied()
        .filter(|named| shapes.contains(named) || here.name() == *named)
        .collect();
    label(rux, Pos2::new(inside_left, pen + ENDPOINT_ROW / 2.0), "Speaks");
    let controls_left = inside_left + ENDPOINT_LABEL + 14.0;
    let width = kit::segmented_width(rux, &offered, 0.0, true, 11.0);
    let track = Rect::from_min_size(Pos2::new(controls_left, pen), Vec2::new(width, ENDPOINT_ROW));
    let chosen_shape = offered.iter().position(|named| here.name() == *named);
    if let Some(picked) =
        kit::segmented_sized(rux, track, "Speaks", &offered, chosen_shape, 0.0, true, 26.0, 11.0)
    {
        if let Some(wire) = Wire::from_name(offered[picked]) {
            configuration.providers[index].wire = wire;
            // **Changing the shape fills in what that shape needs.** A row switched to `codex-cli` with an
            // address still in it is a row that says it runs codex and names an address.
            let row = &mut configuration.providers[index];
            if let Some(program) = wire.program_name() {
                if row.command.trim().is_empty() {
                    row.command = program.to_owned();
                }
            }
            act = Some(Act::Changed);
        }
    }
    pen += ENDPOINT_ROW + ENDPOINT_ROW_GAP;

    let a_program = configuration.providers[index].is_a_program();
    for (field, width) in fields_of(&configuration.providers[index]) {
        label(rux, Pos2::new(inside_left, pen + ENDPOINT_ROW / 2.0), field);
        let at = Rect::from_min_size(
            Pos2::new(controls_left, pen),
            Vec2::new(width.min(inside_right - controls_left), ENDPOINT_ROW),
        );
        let provider = &mut configuration.providers[index];
        let (mut value, hint) = match field {
            "Program" => (provider.command.clone(), "claude".to_owned()),
            "URL" => (provider.url.clone(), "https://\u{2026}".to_owned()),
            // Empty is the honest default for an agent: it answers with the model its own person chose.
            "Model" => (
                provider.model.clone(),
                match a_program {
                    true => "the agent's own".to_owned(),
                    false => "the model's name".to_owned(),
                },
            ),
            // **The hint names the variable that shape's own tools use**, so the `openai` row pointed at a local
            // llama.cpp does not suggest Anthropic's.
            "Key from" => (
                provider.key_env.clone(),
                match provider.wire {
                    Wire::Anthropic => "ANTHROPIC_API_KEY".to_owned(),
                    _ => "OPENAI_API_KEY".to_owned(),
                },
            ),
            _ => (provider.max_tokens.to_string(), DEFAULT_MAX_TOKENS.to_string()),
        };
        if kit::text_well(rux, at, field, &mut value, &hint, true, false).changed() {
            let provider = &mut configuration.providers[index];
            match field {
                "Program" => provider.command = value.trim().to_owned(),
                "URL" => provider.url = value.trim().to_owned(),
                "Model" => provider.model = value.trim().to_owned(),
                "Key from" => provider.key_env = value.trim().to_owned(),
                _ => {
                    if let Ok(count) = value.trim().parse::<u32>() {
                        provider.max_tokens = count.clamp(64, 200_000);
                    }
                }
            }
            act = Some(Act::Changed);
        }
        pen += ENDPOINT_ROW + ENDPOINT_ROW_GAP;
    }
    pen += ENDPOINT_GAP - ENDPOINT_ROW_GAP;

    // Whether it can be reached, and never the key itself. `Provider::why_not` is the same sentence the composer
    // shows when a send is refused, so the page and the pane cannot disagree.
    let provider = &configuration.providers[index];
    let (said, mark, tint) = match why_not {
        Some(why) => (why, Icon::Cross, chat.slower),
        None => (
            match (provider.is_a_program(), provider.wants_a_key()) {
                // **Not the path it was found at.** A person's own home folder is in that path, and this page is
                // drawn into a screenshot test that is committed.
                (true, _) => {
                    "Ready \u{00B7} found on this machine, and it holds its own account".to_owned()
                }
                (false, true) => format!("Ready \u{00B7} key set from ${}", provider.key_env),
                (false, false) => "Ready \u{00B7} no key needed".to_owned(),
            },
            Icon::Tick,
            chat.faster,
        ),
    };
    let centre = pen + ENDPOINT_STATUS / 2.0;
    rux.mark(Mark::new(mark, 14.0), Pos2::new(inside_left + 7.0, centre), tint);
    let galley =
        rux.elided(Style::sans(11.5), &said, theme.ink.i400, inside_right - inside_left - 22.0);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(inside_left + 22.0, centre),
        galley,
        Style::sans(11.5),
        theme.ink.i400,
    );
    (card.bottom(), act)
}

/// A word in an endpoint's label column, centred on its row.
fn label(rux: &mut Rux<'_>, at: Pos2, name: &str) {
    let theme = rux.theme();
    let galley = rux.text(Style::sans(12.0), name, theme.ink.i500);
    rux::text::draw_left_capitals(rux.painter(), at, galley, Style::sans(12.0), theme.ink.i500);
}

// A name no endpoint has yet, so `Add endpoint` twice makes two rows rather than two of one name.
fn unused_name(providers: &[Provider]) -> String {
    let mut count = providers.len() + 1;
    loop {
        let name = format!("endpoint-{count}");
        if !providers.iter().any(|one| one.name == name) {
            return name;
        }
        count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_an_endpoint_twice_makes_two_rows_rather_than_two_of_one_name() {
        let mut providers = Provider::defaults();
        let first = unused_name(&providers);
        let mut new = providers[2].clone();
        new.name = first.clone();
        providers.push(new);
        let second = unused_name(&providers);
        assert_ne!(first, second);
    }
}
