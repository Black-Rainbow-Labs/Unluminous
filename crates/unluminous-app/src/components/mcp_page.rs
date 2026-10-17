//! `Settings -> Tools -> MCP`: how an AI agent is given Unluminous.
//!
//! Three cards from `components::settings_kit`, which every page of the Settings window is drawn with,
//! in the Settings design's layout (`task-2246`): Install, Server and Configuration.
//!
//! ## Install comes first, because it is what somebody opened the page for
//!
//! One button a client, and the label is what it will do: `Install for Claude Code`, or
//! `Remove from Claude Code` once it is there. Whether it is there is read when the page is opened
//! and again after a click rather than every frame, because it is a file on disk and a page that
//! reads one sixty times a second is a page that hesitates.
//!
//! The buttons write the **stdio** server: the agent launches `unluminous-cli mcp serve` itself, nothing
//! listens on a port, and the process lives exactly as long as the conversation. That is why the
//! server section under it is off by default and does not need to be touched for the buttons to
//! work.
//!
//! ## And the configuration is shown whether or not there is a button for it
//!
//! The block, verbatim, with a `Copy`: the JSON for anything that reads an `mcpServers` object, or
//! the TOML Codex reads. It is the answer for a client nobody has written a button for, and it is
//! also how somebody checks what a button just did.
//!
//! **One block at a time, chosen by a carved track of two**, rather than both at once, and rather than a
//! list that has to be opened, which would hide which block is showing.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect};

use unluminous_cli::mcp::install::{self, Client, Wanted};
use unluminous_cli::mcp::Shape;

use crate::components::settings_dialog::drawn;
use crate::components::settings_kit::{self as kit, Column};
use crate::services::mcp::State;
use crate::settings::{clamp_port, Settings};

/// What the page is showing between frames.
///
/// It lives in the Settings window's state, so what a button did is still on the screen when the
/// page is left and come back to.
#[derive(Debug, Clone, Default)]
pub struct McpState {
    /// The port, as it is being typed. A field holds text rather than a number, because a person
    /// halfway through typing `7345` has typed `73`, and a number would have snapped to it.
    pub port: String,
    /// Whether each client already has Unluminous, read when the page opens and after every click.
    pub claude_installed: bool,
    pub codex_installed: bool,
    pub opencode_installed: bool,
    /// Whether it has been read at all yet.
    pub read: bool,
    /// What the last button did, for the line under it.
    pub said: Option<String>,
    /// Which client's configuration block is showing.
    pub showing: Option<Client>,
}

impl McpState {
    /// Read each client's configuration, which is what decides what the buttons say.
    pub fn refresh(&mut self, settings: &Settings, program: &Path) {
        let wanted = wanted_from(settings, program);
        self.claude_installed = install::installed(Client::Claude, &wanted).is_some();
        self.codex_installed = install::installed(Client::Codex, &wanted).is_some();
        self.opencode_installed = install::installed(Client::OpenCode, &wanted).is_some();
        self.read = true;
        if self.port.is_empty() {
            self.port = settings.mcp_port.to_string();
        }
    }
}

/// What the page asked for.
#[derive(Debug, Default, PartialEq)]
pub struct McpOutcome {
    /// A setting changed, so the window applies it and writes the settings file.
    pub changed: bool,
    /// How tall the page came out, which is what decides whether the dialog gives it a scrollbar.
    /// See `settings_dialog::Drawn`.
    pub height: f32,
}

/// Draw the page down `area`.
pub fn show(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut McpState,
    settings: &mut Settings,
    running: &State,
    program: &Path,
) -> McpOutcome {
    let mut outcome = McpOutcome::default();
    if !state.read {
        state.refresh(settings, program);
    }
    let mut column = Column::new(area.left(), area.right(), area.top());
    install_card(ui, &mut column, state, settings, program);
    server_card(ui, &mut column, state, settings, running, &mut outcome);
    configuration_card(ui, &mut column, state, settings, program);
    outcome.height = drawn(area.top(), &column, false).height;
    outcome
}

/// One button an agent, and what the last one did.
fn install_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut McpState,
    settings: &Settings,
    program: &Path,
) {
    let help = match &state.said {
        Some(said) => said.clone(),
        None => "Restart the agent afterwards. In Claude Code, /mcp then lists Unluminous's tools."
            .to_owned(),
    };
    kit::card(
        ui,
        column,
        "Install",
        Some(
            "Writes Unluminous into the agent's own configuration. The agent then launches Unluminous's server \
             itself over a pipe, so nothing listens on a port.",
        ),
        |card| {
            card.row("", Some(&help), |row| {
                for (client, installed) in [
                    (Client::Claude, state.claude_installed),
                    (Client::Codex, state.codex_installed),
                    (Client::OpenCode, state.opencode_installed),
                ] {
                    let name = match installed {
                        true => format!("Remove from {}", client.title()),
                        false => format!("Install for {}", client.title()),
                    };
                    if row.button(&name) {
                        let wanted = wanted_from(settings, program);
                        let done = match installed {
                            true => install::remove(client, &wanted),
                            false => install::install(client, &wanted),
                        };
                        state.said = Some(match done {
                            Ok(done) => done.message,
                            Err(problem) => problem,
                        });
                        state.refresh(settings, program);
                    }
                }
            });
        },
    );
}

/// The switch, the port, the tool shape, the areas, and what is actually happening right now.
fn server_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut McpState,
    settings: &mut Settings,
    running: &State,
    outcome: &mut McpOutcome,
) {
    kit::card(ui, column, "Server", None, |card| {
        let mut enabled = settings.mcp_enabled;
        if card.switch("Also serve over HTTP on this machine", None, &mut enabled) {
            settings.mcp_enabled = enabled;
            outcome.changed = true;
        }
        card.row("Port", None, |row| {
            let before = state.port.clone();
            row.text("MCP port", &mut state.port, "", 96.0);
            if state.port != before {
                // A number is taken from the field only when it is one. Half a port number is what
                // somebody typing looks like, and snapping the setting to `73` on the way to `7345` would
                // start a listener on a port nobody asked for.
                if let Ok(port) = state.port.trim().parse::<f32>() {
                    let port = clamp_port(port);
                    if port != settings.mcp_port {
                        settings.mcp_port = port;
                        outcome.changed = true;
                    }
                }
            }
        });
        let shapes = [Shape::Grouped, Shape::Every];
        let options: Vec<String> =
            shapes.iter().map(|shape| shape_name(*shape).to_owned()).collect();
        let chosen = shapes.iter().position(|shape| *shape == settings.mcp_tools);
        card.row("Tools", None, |row| {
            if let Some(index) = row.select("MCP tool shape", &options, chosen, 200.0) {
                settings.mcp_tools = shapes[index];
                outcome.changed = true;
            }
        });
        let help = format!(
            "Which parts of the catalog this offers, such as editor,git. Empty offers everything.\n{} tools. \
             One an area names every command Unluminous has for about a third of the context one a command \
             costs; one a command lets an agent permit a single tool by name.",
            unluminous_cli::mcp::tools::tools(settings.mcp_tools).len()
        );
        let before = settings.mcp_areas.clone();
        card.row("Areas", Some(&help), |row| {
            row.code_text("MCP areas", &mut settings.mcp_areas, "every area", 300.0);
        });
        if settings.mcp_areas != before {
            outcome.changed = true;
        }
        card.row("State", None, |row| {
            row.status(&running.message());
        });
    });
}

/// The block for the chosen client, and a `Copy` for it.
///
/// **One block at a time, chosen by a track of two**, because the two together ran off the bottom of the
/// page, and because a list that has to be opened would hide which one is showing.
fn configuration_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut McpState,
    settings: &Settings,
    program: &Path,
) {
    let showing = state.showing.unwrap_or(Client::Claude);
    let wanted = wanted_from(settings, program);
    let text = wanted.example(showing);
    kit::card(
        ui,
        column,
        "Configuration",
        Some("For an agent with no button above. Paste it into that client's own configuration file."),
        |card| {
            card.row("Client", None, |row| {
                let names: Vec<&str> = Client::ALL.iter().map(|client| client.title()).collect();
                let chosen = Client::ALL.iter().position(|client| *client == showing);
                if let Some(index) = row.segmented("MCP client", &names, chosen, 110.0, false) {
                    state.showing = Some(Client::ALL[index]);
                }
                if row.button("Copy") {
                    row.rux.ui.ctx().copy_text(text.clone());
                }
                let width = row.right - row.left;
                let at = Pos2::new(row.left, row.bottom + 12.0);
                // Named so a screenshot test can find it, and so somebody reading the window out loud is told
                // what the block is rather than being read the whole of it.
                let block = kit::code_block(
                    row.rux,
                    at,
                    width,
                    &format!("{} configuration", showing.title()),
                    &text,
                );
                row.bottom = block.bottom();
            });
        },
    );
}

fn shape_name(shape: Shape) -> &'static str {
    match shape {
        Shape::Grouped => "One tool an area",
        Shape::Every => "One tool a command",
    }
}

/// What the buttons and the blocks describe.
///
/// The stdio transport, always. The HTTP endpoint is a thing somebody switched on for a client that
/// wants a URL, and it is not what an install button should quietly write: a configuration naming a
/// port would stop working the moment the tick box above it was cleared.
fn wanted_from(settings: &Settings, program: &Path) -> Wanted {
    Wanted { port: settings.mcp_port, program: PathBuf::from(program), ..Wanted::default() }
}
