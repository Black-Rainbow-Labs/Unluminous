//! `Settings -> AI Configuration`: the one list of what can answer (`task-2247`).
//!
//! *"We need to move Agent-Chat's configurations to an AI Configuration setting under Tools. The goal is to
//! have the configurations for keys, urls, cli, etc in one place, and then for Agent Chat, Agent Tasks, etc
//! to have dropdowns to pick from the list of configured endpoints/config entries."* So the endpoint cards
//! that were the first card of the Agent-Chat page are this page, and what they edit is
//! `services::ai_endpoints`, which the window hands to every pane.
//!
//! ## An endpoint is a card of its own
//!
//! - **The six wire shapes are a carved track with a row and a label of their own**, `Speaks`, because they
//!   are the whole of what a row is and a list that had to be opened would hide the answer.
//! - **A shape that cannot work here is not offered.** *"why do we show codex option if its not on the
//!   machine?"* (`task-2003`). `ai_endpoints::Readiness` walks `PATH` once every few seconds, and a program
//!   that is not installed is absent rather than dimmed. The shape a row already uses is always drawn, so a row
//!   is never left naming something missing from its own track.
//! - **A row draws the fields that apply.** A row that runs the command-line agent installed on this machine
//!   needs a program and a model; a URL, a key and a token budget mean nothing there, so they are absent.
//!
//! ## No key is drawn and no key is written
//!
//! A row that sends to an address says `set` or `not set` and never the value. What is typed here is the
//! **name of an environment variable**; the value is read at the moment a request is sent and is never held,
//! written down or logged. A row that runs a program has no key at all: the agent holds its own.

use egui::{Pos2, Rect, Vec2};
use rux::icon::{Icon, Mark};
use rux::{Rux, Style};

use unluminous_chat::acp_registry::{self, Agent};
use unluminous_chat::provider::{
    join_arguments, split_arguments, Provider, Wire, DEFAULT_MAX_TOKENS, WIRES,
};

use crate::components::settings_dialog::{drawn, Drawn};
use crate::components::settings_kit::{self as kit, Column, Kind as ButtonKind};
use crate::services::ai_endpoints::{self, AcpRegistry, Readiness};

/// How wide the dropdown of shapes is when the track does not fit across the card (`task-2302`).
const SPEAKS_SELECT: f32 = 200.0;

/// An endpoint card's own padding and the gap between the things in it: `padding: 14px 16px; gap: 12px`.
const ENDPOINT_PAD_Y: f32 = 14.0;
const ENDPOINT_PAD_X: f32 = 16.0;
const ENDPOINT_GAP: f32 = 12.0;
/// How round an endpoint card is.
const ENDPOINT_RADIUS: f32 = 20.0;
/// The name field and the `Remove` button, and the label column: `grid-template-columns: 96px 1fr;
/// gap: 10px 14px`.
const ENDPOINT_HEAD: f32 = 34.0;
const ENDPOINT_LABEL: f32 = 96.0;
const ENDPOINT_ROW: f32 = 34.0;
const ENDPOINT_ROW_GAP: f32 = 10.0;
/// The line that says whether it can answer.
const ENDPOINT_STATUS: f32 = 18.0;

/// Draw the page down `area`, and answer whether an endpoint changed and how tall the page came out.
pub(crate) fn show(
    ui: &mut egui::Ui,
    area: Rect,
    providers: &mut Vec<Provider>,
    readiness: &mut Readiness,
    acp: &mut AcpRegistry,
) -> Drawn {
    let mut column = Column::new(area.left(), area.right(), area.top());
    let refusals = readiness.rows(providers);
    let shapes = readiness.shapes();
    let mut changed = false;
    kit::card(
        ui,
        &mut column,
        "Endpoints",
        Some(
            "What answers, for Agent Chat, Agent Tasks and anything else that talks to a model. claude-cli, \
             codex-cli, opencode-cli and pi-cli run the agent already installed on this machine: they need no key, they \
             bring their own tools, and they read the open project's own instructions. acp runs any agent that speaks \
             the Agent Client Protocol, started as the program and arguments you give it. openai, anthropic and \
             responses send to a URL instead; for those a key is never stored by Unluminous, so name the \
             environment variable it is in.",
        ),
        |card| {
            card.grow(|rux, column| {
                let only_one = providers.len() == 1;
                let mut removing: Option<usize> = None;
                let mut pen = column.pen;
                for index in 0..providers.len() {
                    // **A child `Ui` an endpoint, carrying its own id salt.** Every field and button here is
                    // named by its word, so three rows each holding a `Remove` and an `openai` would be three
                    // widgets sharing one id, which makes the second row's buttons unclickable.
                    let mut child =
                        rux.ui.new_child(egui::UiBuilder::new().id_salt(("ai-endpoint", index)));
                    let mut inner = Rux { ui: &mut child, state: rux.state, chrome: rux.chrome };
                    let at = EndpointRow {
                        left: column.left,
                        right: column.right,
                        top: pen,
                        index,
                        why_not: refusals.get(index).cloned().flatten(),
                        shapes: &shapes,
                        only_one,
                    };
                    let (bottom, act) = endpoint(&mut inner, providers, at);
                    pen = bottom + 14.0;
                    match act {
                        Some(Act::Remove) => removing = Some(index),
                        Some(Act::Changed) => changed = true,
                        None => {}
                    }
                }
                // The last one is never taken away: there would then be no row to type a new one into.
                if let Some(index) = removing {
                    if providers.len() > 1 {
                        providers.remove(index);
                        changed = true;
                    }
                }
                let (pressed, rect) = kit::add_button(rux, Pos2::new(column.left, pen), "Add endpoint");
                if pressed {
                    let mut new = Provider::defaults()[2].clone();
                    new.name = ai_endpoints::unused_name(providers);
                    providers.push(new);
                    changed = true;
                }
                ((), rect.bottom())
            });
        },
    );
    if let Some(new) = registry_card(ui, &mut column, providers, acp) {
        providers.push(new);
        changed = true;
    }
    if changed {
        readiness.forget();
    }
    drawn(area.top(), &column, changed)
}

/// How tall one agent of the registry is in its list.
const AGENT_ROW: f32 = 58.0;

/// The `ACP Registry` card (`task-2308`): read the published list of ACP agents when asked, and add one as an
/// `acp` endpoint. Answers an endpoint to add now; a binary agent arrives later through `AcpRegistry::poll`,
/// once it has been downloaded.
fn registry_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    providers: &[Provider],
    acp: &mut AcpRegistry,
) -> Option<Provider> {
    let mut added = None;
    kit::card(
        ui,
        column,
        "ACP Registry",
        Some(
            "Agents that speak the Agent Client Protocol, from the registry at agentclientprotocol.com. Nothing is \
             read from it until you press Load registry. Add makes an acp endpoint: an npx or uvx agent runs through \
             that package runner, and a binary agent is downloaded into the settings folder and its checksum \
             checked. Each agent signs in with its own account.",
        ),
        |card| {
            let loading = acp.is_loading();
            let status = match (&acp.problem, &acp.said, &acp.agents) {
                _ if loading => "Reading the registry".to_owned(),
                (Some(problem), _, _) => problem.clone(),
                (None, Some(said), _) => said.clone(),
                (None, None, Some(agents)) => format!("{} agents", agents.len()),
                (None, None, None) => "Not read yet".to_owned(),
            };
            card.row("Registry", None, |row| {
                let word = match acp.agents.is_some() {
                    true => "Read again",
                    false => "Load registry",
                };
                if row.button(word) && !loading {
                    acp.load(true);
                }
                row.status(&status);
            });
            let Some(agents) = acp.agents.clone() else {
                return;
            };
            card.row("Find", None, |row| {
                row.text("Find an agent", &mut acp.filter, "a name or an id", 260.0);
            });
            let wanted = acp.filter.trim().to_lowercase();
            let shown: Vec<&Agent> = agents
                .iter()
                .filter(|agent| {
                    wanted.is_empty()
                        || agent.id.to_lowercase().contains(&wanted)
                        || agent.name.to_lowercase().contains(&wanted)
                })
                .collect();
            let platform = acp_registry::platform();
            card.grow(|rux, column| {
                let mut pen = column.pen;
                for agent in shown {
                    let rect = Rect::from_min_size(
                        Pos2::new(column.left, pen),
                        Vec2::new(column.right - column.left, AGENT_ROW),
                    );
                    if let Some(pressed) = agent_row(rux, rect, agent, providers, acp, &platform) {
                        let environment = crate::services::agent_chat::AgentChat::the_environment();
                        let has = |program: &str| {
                            unluminous_chat::provider::program(program, &environment).is_some()
                        };
                        match acp.add(&pressed, None, providers, &has) {
                            Ok(Some(provider)) => added = Some(provider),
                            Ok(None) => {}
                            Err(why) => acp.problem = Some(why),
                        }
                    }
                    pen = rect.bottom() + 8.0;
                }
                ((), pen)
            });
        },
    );
    added
}

/// One agent of the registry: its name, version and how it runs, its description, and `Add`. Answers the id when
/// `Add` was pressed.
fn agent_row(
    rux: &mut Rux<'_>,
    rect: Rect,
    agent: &Agent,
    providers: &[Provider],
    acp: &AcpRegistry,
    platform: &str,
) -> Option<String> {
    let theme = rux.theme();
    let chat = theme.chat;
    rux.chrome.surface(rect, 16.0, chat.well, chat.carved_sm());
    let left = rect.left() + 14.0;
    let mut kinds: Vec<&str> = Vec::new();
    for kind in agent.distributions.iter().map(|one| one.kind()) {
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let runs_here = acp_registry::launch_for(agent, platform, &|_| true).is_some();
    let installing = acp.installing() == Some(agent.id.as_str());
    let added = providers.iter().any(|one| one.wire == Wire::Acp && one.name == agent.id);
    let (word, kind) = match (added, installing, runs_here) {
        (true, _, _) => ("Added", ButtonKind::Ghost),
        (false, true, _) => ("Installing", ButtonKind::Ghost),
        (false, false, false) => ("Not for this machine", ButtonKind::Ghost),
        (false, false, true) => ("Add", ButtonKind::Chip),
    };
    let width = kit::button_width(rux, word, false);
    let button = Rect::from_min_size(
        Pos2::new(rect.right() - 12.0 - width, rect.center().y - 15.0),
        Vec2::new(width, 30.0),
    );
    let words_right = button.left() - 12.0;
    let title = format!("{}  {}  \u{00B7}  {}", agent.name, agent.version, kinds.join(", "));
    let galley =
        rux.elided(Style::sans(12.5).semibold(), &title, theme.ink.i900, words_right - left);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left, rect.top() + 19.0),
        galley,
        Style::sans(12.5).semibold(),
        theme.ink.i900,
    );
    let galley =
        rux.elided(Style::sans(11.5), &agent.description, theme.ink.i400, words_right - left);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left, rect.top() + 40.0),
        galley,
        Style::sans(11.5),
        theme.ink.i400,
    );
    let pressed = kit::button_named(rux, button, word, &format!("{word} {}", agent.id), kind);
    (pressed && word == "Add").then(|| agent.id.clone())
}

/// What one endpoint's card reported.
enum Act {
    Changed,
    Remove,
}

/// Where one endpoint's card draws, and which of the endpoints it is.
struct EndpointRow<'a> {
    left: f32,
    right: f32,
    /// How far down the page this card starts.
    top: f32,
    /// Which of the endpoints this card is.
    index: usize,
    /// Why this endpoint cannot be reached right now, if it cannot.
    why_not: Option<String>,
    /// Which wire shapes could be used on this machine. See [`Readiness::shapes`].
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
    if provider.wire == Wire::Acp {
        return vec![("Program", 220.0), ("Arguments", 420.0), ("Model", 220.0)];
    }
    match provider.is_a_program() {
        true => vec![("Program", 220.0), ("Model", 220.0)],
        false => {
            vec![("URL", 360.0), ("Model", 220.0), ("Key from", 220.0), ("Most tokens", 110.0)]
        }
    }
}

/// One endpoint: its name, what it speaks, where it is, and whether it can answer. Answers where it ended.
///
/// Carved into the card. Which endpoint a pane uses is that pane's own choice, so this card says nothing
/// about being in use.
fn endpoint(
    rux: &mut Rux<'_>,
    providers: &mut [Provider],
    at: EndpointRow<'_>,
) -> (f32, Option<Act>) {
    let EndpointRow { left, right, top, index, why_not, shapes, only_one } = at;
    let theme = rux.theme();
    let chat = theme.chat;
    let mut act = None;
    let fields = fields_of(&providers[index]);
    let rows = 1 + fields.len();
    let height = ENDPOINT_PAD_Y * 2.0
        + ENDPOINT_HEAD
        + ENDPOINT_GAP
        + rows as f32 * ENDPOINT_ROW
        + (rows - 1) as f32 * ENDPOINT_ROW_GAP
        + ENDPOINT_GAP
        + ENDPOINT_STATUS;
    let card = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, top + height));
    rux.chrome.surface(card, ENDPOINT_RADIUS, chat.well, chat.carved_sm());
    let inside_left = card.left() + ENDPOINT_PAD_X;
    let inside_right = card.right() - ENDPOINT_PAD_X;
    let mut pen = card.top() + ENDPOINT_PAD_Y;

    // The name, and the two things that can be done to the row, on one line.
    let mut name = providers[index].name.clone();
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
        providers[index].name = name.trim().to_owned();
        act = Some(Act::Changed);
    }
    if !only_one {
        let width = kit::button_width(rux, "Remove", false) - 6.0;
        let remove =
            Rect::from_min_size(Pos2::new(inside_right - width, pen + 2.0), Vec2::new(width, 30.0));
        if kit::button(rux, remove, "Remove", ButtonKind::Ghost) {
            act = Some(Act::Remove);
        }
    }
    pen += ENDPOINT_HEAD + ENDPOINT_GAP;

    // Which shape it speaks, as a track rather than a list: there are six and they are the whole of what this
    // row is, so a list that has to be opened would hide the answer. A shape that cannot work on this machine is
    // not offered, and the one this row already uses always is.
    let here = providers[index].wire;
    let offered: Vec<&str> = WIRES
        .iter()
        .copied()
        .filter(|named| shapes.contains(named) || here.name() == *named)
        .collect();
    label(rux, Pos2::new(inside_left, pen + ENDPOINT_ROW / 2.0), "Speaks");
    let controls_left = inside_left + ENDPOINT_LABEL + 14.0;
    // Six shapes do not fit across a narrow card at eleven points, so the track takes the largest of three
    // sizes that fits rather than running past the card's edge. **With seven (`task-2302`) none of the three
    // fits on a card of the usual width**, and the last shape was drawn past the edge where it could not be
    // pressed, so a card too narrow for the track offers the shapes as a dropdown instead.
    let room = inside_right - controls_left;
    let size = [11.0, 10.0, 9.0]
        .into_iter()
        .find(|size| kit::segmented_width(rux, &offered, 0.0, true, *size) <= room);
    let chosen_shape = offered.iter().position(|named| here.name() == *named);
    let picked = match size {
        Some(size) => {
            let width = kit::segmented_width(rux, &offered, 0.0, true, size);
            let track =
                Rect::from_min_size(Pos2::new(controls_left, pen), Vec2::new(width, ENDPOINT_ROW));
            kit::segmented_sized(
                rux,
                track,
                "Speaks",
                &offered,
                chosen_shape,
                0.0,
                true,
                26.0,
                size,
            )
        }
        None => {
            let names: Vec<String> = offered.iter().map(|named| (*named).to_owned()).collect();
            let chip = Rect::from_min_size(
                Pos2::new(controls_left, pen),
                Vec2::new(SPEAKS_SELECT.min(room), ENDPOINT_ROW),
            );
            kit::select(rux, chip, &format!("Speaks {index}"), &names, chosen_shape)
        }
    };
    if let Some(picked) = picked {
        if let Some(wire) = Wire::from_name(offered[picked]) {
            providers[index].wire = wire;
            // **Changing the shape fills in what that shape needs.** A row switched to `codex-cli` with an
            // address still in it is a row that says it runs codex and names an address.
            let row = &mut providers[index];
            if let Some(program) = wire.program_name() {
                if row.command.trim().is_empty() {
                    row.command = program.to_owned();
                }
            }
            act = Some(Act::Changed);
        }
    }
    pen += ENDPOINT_ROW + ENDPOINT_ROW_GAP;

    let a_program = providers[index].is_a_program();
    for (field, width) in fields_of(&providers[index]) {
        label(rux, Pos2::new(inside_left, pen + ENDPOINT_ROW / 2.0), field);
        let at = Rect::from_min_size(
            Pos2::new(controls_left, pen),
            Vec2::new(width.min(inside_right - controls_left), ENDPOINT_ROW),
        );
        let provider = &mut providers[index];
        let (mut value, hint) = match field {
            "Program" => (
                provider.command.clone(),
                match provider.wire {
                    Wire::Acp => "npx".to_owned(),
                    _ => "claude".to_owned(),
                },
            ),
            "Arguments" => (
                join_arguments(&provider.args),
                "-y @agentclientprotocol/claude-agent-acp".to_owned(),
            ),
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
            let provider = &mut providers[index];
            match field {
                "Program" => provider.command = value.trim().to_owned(),
                "Arguments" => provider.args = split_arguments(&value),
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
    let provider = &providers[index];
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
