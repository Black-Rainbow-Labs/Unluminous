//! The Agent-Chat plugin: a pane you talk to a model in.
//!
//! `tasks/task-1767-agent-chat-tdd.md` is the design. `unluminous-chat` is the half that can be tested
//! with no window — the endpoints, the two wire shapes, the framing, the conversation and the thread
//! — and this is the half that has a window behind it: what is being typed, what is attached, which
//! conversation is open, what the tools are doing, and the one place a command becomes a change.
//!
//! ## The provider decides nothing about the window
//!
//! It draws and it returns [`Request`]s, which is the rule `components::activity_bar` set and every
//! contributed surface since has kept. Running a tool is the same shape in both directions: the
//! provider asks for a command by its catalogue name, the window runs it through
//! `UnluminousApp::run_cli` — **the one place a command turns into a change** — and hands the answer back
//! on the next frame. So a tool call and a person pressing the same menu entry are the same thing
//! rather than two paths that agree today.
//!
//! ## Nothing is fetched that was not asked for
//!
//! One request is made, when somebody presses send. There is no discovery, no model list, no
//! telemetry and nothing at startup — which is the rule the Markdown preview, the Mermaid reader and
//! the plugin loader all keep, and the reason it is worth stating is that this is the first thing in
//! Unluminous with a socket in it.

pub mod store;
pub mod tools;

use std::path::{Path, PathBuf};

use unluminous_chat::model::{Message, Part, Role};
use unluminous_chat::provider::{Provider, Wire, WIRES};
use unluminous_chat::{Client, Conversation, Session, State};

use crate::services::plugin_ui::{self, Answer, Context, Look, Request, UiProvider};
use crate::services::store::Values;

use store::{Store, Summary};

/// How many rounds of tools one turn may take before the pane stops asking.
///
/// Thirty since `task-2096`, which asks for it. It was eight, and eight rounds ends a turn that reads a
/// few files, searches and opens a tab before the work it was asked for is done.
pub const DEFAULT_TOOL_LIMIT: u32 = 30;

/// The limit every settings file written before `task-2096` holds, because the code wrote it there.
const OLD_DEFAULT_TOOL_LIMIT: u32 = 8;

/// How many conversations are kept.
pub const DEFAULT_HISTORY: usize = 20;

/// What a request for the clipboard's picture is answered under.
///
/// A name rather than a number, so it cannot be mistaken for one of the tool positions
/// `ask_for_the_tools` sends — those parse as a `usize` and this does not.
pub const CLIPBOARD: &str = "clipboard";

/// How long an endpoint's readiness is believed for.
///
/// `services::debuggers::ADAPTER_SEARCH_TTL`'s number and its reason: the answer comes from reading
/// directories, and the commonest thing to happen next is an install finishing.
pub const READINESS: std::time::Duration = std::time::Duration::from_secs(5);

/// The plugin's own settings, in `plugins/agent-chat/settings.conf` beside its manifest.
///
/// Read by the same `store::Values` the window's own settings are read by, in the same
/// `name = value` form, so a person can open it in Unluminous and change it. **No key is in it** — a
/// provider names the environment variable its key comes from and nothing else, which is
/// `services::agent_tasks::keychain`'s rule: what is written down is the name of the place the
/// secret is.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    pub providers: Vec<Provider>,
    /// Which one is used, by name. Empty means the first.
    pub chosen: String,
    pub stream: bool,
    /// Whether Unluminous's own commands are offered to the model. On unless somebody turns it off.
    pub tools: bool,
    /// Whether the commands that run a program of the model's choosing are among them.
    ///
    /// A second switch rather than a wider first one, because `terminal send` is a different kind of
    /// thing from `editor text`: it hands a server on the far end of a socket this machine's shell,
    /// with this machine's environment in it. See `tools::RUNS_A_PROGRAM`.
    pub shell: bool,
    pub tool_limit: u32,
    /// How much a command-line agent may do to this machine without being asked.
    ///
    /// **Only a command-line agent has one**, and it is the setting that matters most for those:
    /// `claude` and `codex` run with `--print`, so they cannot stop and ask a question, and what they
    /// may do has to be decided before they start. `full` is the default since `task-2003` — see
    /// [`unluminous_chat::Permission::Full`]. Unluminous's own `tools` and `shell` switches are the equivalent for an HTTP endpoint,
    /// where the model asks and Unluminous runs it — the two are never both in play, because the
    /// transport decides which of them applies.
    pub permission: unluminous_chat::Permission,
    /// The person's own system prompt, added after Unluminous's own line.
    pub system: String,
    pub history: usize,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            providers: Provider::defaults(),
            chosen: String::new(),
            stream: true,
            // **On**, and the reasoning that made it off is narrowed rather than abandoned.
            //
            // `task-1767` turned it off because the other end of a URL is a server, and Unluminous's
            // catalogue includes commands that run a program. What ships as the chosen provider is
            // `claude` or `codex`, which run a program the person already trusts with this machine, hold
            // their own credentials, and are offered **no** Unluminous tools at all — so the switch was
            // defending against the case that is not the common one, and a chat pane in an editor that
            // cannot read the project it is beside is a pane that answers about nothing.
            //
            // `task-1848` asks for this outright. What keeps it safe is the second switch below, not this
            // one: reading the project, opening a tab and running a query are things this pane should do,
            // and running an arbitrary command line is not.
            tools: true,
            // **Still off**, and this is the half that must not move. `tools::RUNS_A_PROGRAM` is
            // `terminal send`, `run add`, `run start`, `debug install` and `launch`, which between them
            // will run any command line at all on this machine.
            shell: false,
            tool_limit: DEFAULT_TOOL_LIMIT,
            // **`full`**, which `task-2003` asks for: *"Agent chat should have full by default."* See
            // `unluminous_chat::Permission::Full` for why the safest value was the wrong one to start
            // from here. It governs a command-line agent only; `tools` and `shell` above are the
            // equivalent for an endpoint, and `shell` is still off.
            permission: unluminous_chat::Permission::default(),
            system: String::new(),
            history: DEFAULT_HISTORY,
        }
    }
}

impl Configuration {
    const FILE: &'static str = "settings.conf";
    /// What wrote this file, so a setting whose default changed can be moved once and only once.
    ///
    /// **1 is the first**, written from `task-2003` onwards; a file with no `version` at all was
    /// written before it. See the migration in [`Configuration::of`], and the reason there is one: a
    /// file that is rewritten on every change carries the old default as though it were a choice.
    ///
    /// **2** from `task-2096`, when the tool limit's default moved from eight to thirty.
    const VERSION: f64 = 2.0;

    /// Read the configuration out of the plugin's folder, or the defaults when there is no file.
    ///
    /// Answers what it could not read as well; the pane draws it where a refusal goes.
    pub fn read(folder: &Path) -> (Self, Vec<String>) {
        let Ok(text) = std::fs::read_to_string(folder.join(Self::FILE)) else {
            return (Self::default(), Vec::new());
        };
        Self::of(&Values::parse(&text))
    }

    /// The same, from values already read, so a test needs no file.
    ///
    /// Answers the rows it could not read as well, because a row silently dropped is an endpoint that
    /// is simply not there and nothing says why — the fault every other registry in Unluminous is refused
    /// with a list to avoid.
    pub fn of(values: &Values) -> (Self, Vec<String>) {
        let mut configuration = Self { providers: Vec::new(), ..Self::default() };
        let (providers, mut refused) =
            plugin_ui::read_numbered_rows(values, "provider", 32, provider_at);
        configuration.providers = providers;
        // A file that names no provider at all — or whose every row was refused — gets the three
        // that ship, because a pane with no endpoint is a pane that cannot do anything and a person
        // who wanted none would have switched the plugin off.
        if configuration.providers.is_empty() {
            configuration.providers = Provider::defaults();
        }
        if let Some(chosen) = values.text("chosen") {
            configuration.chosen = chosen.trim().to_owned();
        }
        if let Some(stream) = values.flag("stream") {
            configuration.stream = stream;
        }
        if let Some(tools) = values.flag("tools") {
            configuration.tools = tools;
        }
        if let Some(shell) = values.flag("shell") {
            configuration.shell = shell;
        }
        if let Some(limit) = values.number("tool-limit") {
            configuration.tool_limit = limit.clamp(1.0, 32.0) as u32;
        }
        // The same reasoning as the permission below: a file written before version 2 holds eight
        // because the code wrote eight there, so eight in such a file is the old default and not a
        // choice. Any other number in it was chosen and is kept.
        let written_before_version_2 = values.number("version").is_none_or(|version| version < 2.0);
        if written_before_version_2 && configuration.tool_limit == OLD_DEFAULT_TOOL_LIMIT {
            configuration.tool_limit = DEFAULT_TOOL_LIMIT;
        }
        // **A file written before `task-2003` does not get to keep the old default.** `full` is what
        // the ticket asks for — *"Agent chat should have full by default"* — and a default alone would
        // never have reached anybody: this file is rewritten whenever anything on the page changes, so
        // every existing installation has `permission = read` in it because the **code** wrote it and
        // not because a person chose it. Without this the change would be true of a fresh install and
        // of nobody else, which is not what was asked for.
        //
        // It happens once. `version` is written from here on, so a file that has been read by this
        // version or a later one is left exactly as it is, including a `read` somebody really did
        // choose. See [`Configuration::VERSION`].
        let written_by_an_older_version = values.number("version").is_none();
        if let Some(named) = values.text("permission").filter(|_| !written_by_an_older_version) {
            match unluminous_chat::Permission::from_name(named) {
                Some(permission) => configuration.permission = permission,
                // Refused with the list rather than falling back quietly, because falling back to
                // `read` would look like an agent that will not do as it is told, and falling back
                // to anything else would be Unluminous widening a permission nobody granted.
                None => refused.push(format!(
                    "`{}` is not something an agent may be allowed to do, and this version of Unluminous has {}.",
                    named.trim(),
                    unluminous_chat::PERMISSIONS.join(", ")
                )),
            }
        }
        if let Some(system) = values.text("system") {
            configuration.system = system.to_owned();
        }
        if let Some(history) = values.number("history") {
            configuration.history = history.clamp(1.0, 500.0) as usize;
        }
        (configuration, refused)
    }

    /// Write it back into the plugin's folder.
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        let mut values = Values::new();
        values.set("providers", self.providers.len().to_string());
        for (index, provider) in self.providers.iter().enumerate() {
            values.set(&format!("provider.{index}.name"), provider.name.clone());
            values.set(&format!("provider.{index}.wire"), provider.wire.name());
            values.set(&format!("provider.{index}.command"), provider.command.clone());
            values.set(&format!("provider.{index}.url"), provider.url.clone());
            values.set(&format!("provider.{index}.model"), provider.model.clone());
            values.set(&format!("provider.{index}.key-env"), provider.key_env.clone());
            values.set(&format!("provider.{index}.key-entry"), provider.key_entry.clone());
            values.set(&format!("provider.{index}.max-tokens"), provider.max_tokens.to_string());
        }
        values.set("chosen", self.chosen.clone());
        values.set("stream", self.stream.to_string());
        values.set("tools", self.tools.to_string());
        values.set("shell", self.shell.to_string());
        values.set("tool-limit", self.tool_limit.to_string());
        values.set("version", Self::VERSION.to_string());
        values.set("permission", self.permission.name());
        values.set("system", self.system.replace('\n', " "));
        values.set("history", self.history.to_string());
        plugin_ui::write_values(
            folder,
            Self::FILE,
            &values,
            "# The Agent-Chat plugin's settings. `Settings -> Agent-Chat` writes this file and reads it back.\n\
             # `wire` is `claude-cli` or `codex-cli` for a row that runs the agent installed on this machine —\n\
             # those need no key at all — or `openai`, `anthropic` or `responses` for one that sends to a URL.\n\
             # A row that sends names the environment variable its key comes from; the key itself is never\n\
             # written here or anywhere else by Unluminous.",
        )
    }

    /// Drop the rows Unluminous ships for a command-line agent that is not on this machine.
    ///
    /// `task-2003`: *"why do we show codex option if its not on the machine?"* Three rows ship and two
    /// of them run an agent, so a machine with only one of the two agents installed opened the Settings
    /// page on a card that said in red that it could never work — and the endpoint list in the pane's
    /// own header offered it as something to talk to. A row that cannot answer, that nobody asked for
    /// and that names a program which is not here is not a choice; it is a thing to explain.
    ///
    /// Three conditions, and each one is there to make this smaller than it sounds:
    ///
    /// - **It runs a program, and the program is not installed.** A URL that is unreachable is not the
    ///   same thing: nothing here can tell a server that is down from one that is asleep, and the row
    ///   is still where the address a person typed lives.
    /// - **It is one of the rows Unluminous ships, unchanged** — [`Provider::is_one_unluminous_ships`].
    ///   A row somebody wrote stays, whatever it names, because it is a thing they meant.
    /// - **Nobody chose it by name.** Choosing it is asking for it, and a chosen row that will not run
    ///   has to say so rather than disappear and leave the pane answering from a different endpoint.
    ///   By **name**, and not [`Self::provider`]: with nothing chosen that function answers with the
    ///   first row there is, so on a machine with `codex` and no `claude` the broken row would have
    ///   been kept and gone on being the default, which is the case this exists for.
    ///
    /// **Nothing is written.** The file keeps the row, so installing the agent brings it back at the
    /// next start with no settings to repair. It is a filter on what is offered rather than a deletion,
    /// which is also why it is safe to run on every open. The `local` row sends to an address, so there
    /// is always at least one row left however few agents are installed.
    pub fn forget_the_agents_that_are_not_installed(
        &mut self,
        environment: &unluminous_chat::Environment,
    ) {
        let chosen = self.chosen.trim().to_owned();
        self.providers.retain(|one| {
            one.name == chosen
                || !one.is_a_program()
                || !one.is_one_unluminous_ships()
                || one.program_path(environment).is_some()
        });
    }

    /// The endpoint that is used, which is the chosen one or the first there is.
    pub fn provider(&self) -> Option<&Provider> {
        self.providers.iter().find(|one| one.name == self.chosen).or_else(|| self.providers.first())
    }

    /// Choose one by name, or say it is not there.
    pub fn choose(&mut self, name: &str) -> Result<(), String> {
        match self.providers.iter().any(|one| one.name == name) {
            true => {
                self.chosen = name.to_owned();
                Ok(())
            }
            false => Err(format!(
                "there is no endpoint called `{name}`. There is {}.",
                self.providers
                    .iter()
                    .map(|one| one.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ")
            )),
        }
    }
}

/// One provider's row, nothing when the row is empty, or the sentence it is refused with.
///
/// **Refused with the list rather than half-loaded**, which is the rule `plugin.kind`,
/// `language.renders`, `run.project`, `debug.adapter` and `ui.chrome` all keep. A row whose wire is
/// unknown would be an endpoint every request to which fails obscurely — and a row silently dropped
/// is worse still, because the Settings page then shows a list somebody's file does not match.
fn provider_at(values: &Values, index: usize) -> Result<Option<Provider>, String> {
    let Some(name) = values.text(&format!("provider.{index}.name")) else {
        return Ok(None);
    };
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Ok(None);
    }
    let named = values.text(&format!("provider.{index}.wire")).unwrap_or("openai").trim();
    let Some(wire) = Wire::from_name(named) else {
        return Err(format!(
            "`{name}` speaks `{named}`, and this version of Unluminous speaks {}.",
            WIRES.join(", ")
        ));
    };
    Ok(Some(Provider {
        name,
        wire,
        command: values
            .text(&format!("provider.{index}.command"))
            .unwrap_or_default()
            .trim()
            .to_owned(),
        url: values.text(&format!("provider.{index}.url")).unwrap_or_default().trim().to_owned(),
        model: values
            .text(&format!("provider.{index}.model"))
            .unwrap_or_default()
            .trim()
            .to_owned(),
        key_env: values
            .text(&format!("provider.{index}.key-env"))
            .unwrap_or_default()
            .trim()
            .to_owned(),
        key_entry: values
            .text(&format!("provider.{index}.key-entry"))
            .unwrap_or_default()
            .trim()
            .to_owned(),
        max_tokens: values
            .number(&format!("provider.{index}.max-tokens"))
            .unwrap_or(unluminous_chat::provider::DEFAULT_MAX_TOKENS as f32)
            .clamp(64.0, 200_000.0) as u32,
    }))
}

/// A picture waiting to go up with the next message.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// Its own number, so the drawing can key a texture on it without keying on the bytes.
    pub id: u64,
    pub name: String,
    pub media: String,
    pub bytes: Vec<u8>,
}

/// A tool call handed to the window and not yet answered.
#[derive(Debug, Clone, PartialEq)]
struct Outstanding {
    /// **Where the call sits in the message that asked for it**, not the id the server gave.
    ///
    /// A server can send two calls with one id — it should not, and one does — and filing the work
    /// under the id meant skipping the second as already outstanding and then answering only the
    /// first, which left the turn stopped with no way out. `Session::tools_to_run` gives the
    /// position and nothing else can collide with it.
    at: usize,
    /// When it went out, so the block can say how long it took.
    began: std::time::Instant,
}

/// What the drawing keeps between frames.
///
/// On the provider rather than in the component, because a component in Unluminous takes a rectangle,
/// draws and returns what happened — it holds nothing. `components::agent_chat` is written to that
/// rule and this is where the little it has to remember lives.
#[derive(Default)]
pub struct PaneState {
    /// Put the conversation at the bottom on the next frame, and then stop.
    ///
    /// **The scrolling itself is `egui`'s.** `ScrollArea::stick_to_bottom` follows an answer while the
    /// view is already at the bottom and stops the moment somebody scrolls up, which is
    /// `ChatPage.tsx`'s own `shouldAutoScroll` rule and is better than reimplementing it. What egui
    /// will not do is go *back* to the bottom once somebody has scrolled away, so sending, opening a
    /// conversation and starting a new one each ask for it once — the one-shot shape
    /// `UnluminousApp::follow_the_open_file` already uses.
    pub jump_to_bottom: bool,
    /// Whether the history list is open over the conversation.
    pub history_open: bool,
    /// The model selector in the header, made the first time it is drawn. `task-2096`.
    pub model_select: Option<ModelSelect>,
    /// The tool blocks somebody has opened by hand, by their call id.
    pub opened_tools: Vec<String>,
    /// Which message's thinking has been opened.
    pub opened_thinking: Vec<u64>,
    /// The markdown each message came to, kept between frames and keyed on the message.
    ///
    /// Rendering and laying out is the expensive half and the source of a finished message never
    /// changes, so a conversation of forty messages costs nothing while the forty-first is arriving.
    /// `components::markdown_text::Cache` re-renders only when the source or the width has moved,
    /// which bounds a streaming answer at one render a frame — `task-1666`'s rule applied to the one
    /// thing here that changes sixty times a second.
    pub rendered: crate::components::markdown_text::Cache,
    /// The pictures already uploaded to the graphics card, by message and part.
    ///
    /// Keyed on where the picture is rather than on its bytes, so a conversation with twenty pictures
    /// in it does not decode twenty pictures a frame.
    pub pictures: std::collections::HashMap<String, egui::TextureHandle>,
    /// Whether the composer's own prompt field held the keyboard on the frame just drawn.
    ///
    /// **Not "some text box in the window has it".** `task-1771`'s paste is read off the key going back up,
    /// and the question of whose that key is has to be answered by *this* field rather than by any field:
    /// with the pane showing and the explorer's filter box focused, `Ctrl`+`V` was attaching the clipboard's
    /// picture to a conversation nobody was typing into. Found by the ticket's own review.
    pub prompt_focused: bool,
    /// How far down the list that is showing the view is, and where to put it on the next frame.
    ///
    /// `task-1771`: the pane is zoomable, and a zoom that does not keep what the pointer was over still is
    /// a zoom you have to scroll back from. The scrolling is `egui`'s, so the offset is read back off the
    /// `ScrollArea` each frame and handed to it once when it has to move - the one-shot shape
    /// `jump_to_bottom` beside it already uses. Worked out in `AgentChat::zoomed`, which is what the
    /// window calls when the pane's zoom changes.
    ///
    /// **Whichever list is showing**, which is the conversation, the history or the endpoints: the three
    /// are drawn into the same rectangle and one of them at a time, so one set of numbers describes what
    /// a person is looking at. Since `task-2003` the wheel over a canvas node is applied through the same
    /// three, so the history and the endpoints scroll there exactly as the conversation does.
    pub scrolled: f32,
    /// How far the list that is showing *could* be scrolled: its content height less the room it is drawn in.
    ///
    /// Kept beside `scrolled` so that "is it at the bottom" is answerable as data — `scrolled` alone says
    /// nothing without the maximum to compare it against. `task-1848`.
    pub scrollable: f32,
    pub scroll_to: Option<f32>,
    /// Where that list was drawn, in the points the pane was handed.
    ///
    /// Written by the drawing each frame so the window can ask whether a wheel it read belongs to the
    /// list rather than to the header or the composer. `task-2003`: a canvas node's layer registers no
    /// `AreaState`, so nothing inside one can take the wheel itself and the window has to hand it over —
    /// see [`AgentChat::scroll_at`].
    pub list_rect: Option<egui::Rect>,
    /// A wheel the window read over a canvas node, for the list that is showing to take.
    ///
    /// Taken once and cleared, like `jump_to_bottom` beside it. It is handed to `egui` as a **delta**
    /// through `Ui::scroll_with_delta` rather than as an offset, and that is not a detail: an offset
    /// written straight into `ScrollArea::vertical_scroll_offset` is overwritten again before the frame
    /// ends whenever `stick_to_bottom` is on and the view was already at the bottom — which is exactly
    /// where a conversation sits, so scrolling up did nothing at all. A delta sets egui's own
    /// `had_explicit_scroll_adjustment`, which is what unsticks it. `task-2003`.
    pub wheel: Option<f32>,
    /// How big each of those is, read out of the picture's own header.
    ///
    /// Kept apart from the textures because the measuring pass needs it and has no `egui::Ui` to
    /// upload one with. See `services::picture::dimensions_of`.
    pub picture_sizes: std::collections::HashMap<String, (f32, f32)>,
    /// What one message has selected, and which message that is.
    ///
    /// **One message at a time**, which is what a selection in a column of bubbles can honestly be:
    /// each is laid out into its own rope, so a range that crossed two of them would be two ranges
    /// with nothing to say how the gap between them reads. Dragging inside another message replaces
    /// it, which is what every list of separately selectable blocks does. `task-2060`.
    pub selection: Option<Selected>,
    /// Which message's copy button is up, and when the pointer was last on it or on its message.
    ///
    /// `task-2060`: *"I can't actually select copy of a message because it goes away as soon as i
    /// hover off the message."* The button is drawn beside the bubble rather than inside it, so the
    /// pointer has to cross a gap to reach it and the hover that put it there is over before it
    /// arrives. The time is `InputState::time`, which is seconds since the window started.
    pub copy_shown: Option<(u64, f64)>,
    /// The right click menu over a message, while it is open.
    pub menu: Option<MessageMenu>,
}

/// The model selector: `rux`'s `Select` and what it keeps between frames.
///
/// `rux` keeps its own decoration canvases and icon marks in a `RuxState`, and the select's open flag in
/// a `SelectState`; both belong to the caller, so each chat, a pane or a canvas node, has its own.
pub struct ModelSelect {
    pub rux: rux::RuxState,
    pub menu: rux::components::SelectState,
}

impl ModelSelect {
    /// Drawn in `rux`'s dark theme, which is the ai-service dark neumorphism this pane is modelled on.
    ///
    /// **Deterministic**, meaning its canvases rasterise at the SIMD level every machine of this target
    /// has, for the reason `UnluminousApp::draw_deterministically` gives: a screenshot of this pane must
    /// be the same picture on every machine. The trigger is a few hundred points of decoration, so the
    /// slower level costs nothing anybody can measure.
    pub fn new() -> Self {
        let theme = rux::Theme::named("dark-neumorphic").unwrap_or_else(rux::theme::dark);
        Self { rux: rux::RuxState::deterministic(theme), menu: Default::default() }
    }
}

impl Default for ModelSelect {
    fn default() -> Self {
        Self::new()
    }
}

/// A selection inside one message's rendered words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selected {
    /// The message it is in, which is what keys the rendered markdown it is measured against.
    pub message: u64,
    pub range: unluminous_core::Selection,
}

impl Selected {
    /// The key the rendered markdown for this message is cached under.
    pub fn key(&self) -> String {
        format!("message-{}", self.message)
    }
}

/// The right click menu over a message: where it was opened and which message it is about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageMenu {
    pub at: egui::Pos2,
    pub message: u64,
}

/// The pieces of the pane the drawing is handed.
///
/// A component in Unluminous takes a rectangle, draws and reports what happened — it changes nothing. So
/// the drawing is given borrows of what it reads and one mutable borrow of what it must write into
/// (the draft, and the little the drawing has to remember between frames), and everything else it
/// wants done comes back as a `components::agent_chat::Act`.
pub struct Parts<'a> {
    pub session: &'a Session,
    pub configuration: &'a Configuration,
    pub state: &'a mut PaneState,
    pub draft: &'a mut String,
    pub attachments: &'a [Attachment],
    /// What has been typed while an answer was arriving, waiting its turn. See [`AgentChat::send`].
    pub queued: &'a [Message],
    pub history: &'a [Summary],
    pub problem: Option<&'a str>,
    /// Why each endpoint cannot answer, or `None` where it can, in the order the providers are in.
    ///
    /// **Handed over rather than asked for**, which is [`AgentChat::readiness`]'s own reason: for a row
    /// that runs a program the question is a walk of `PATH` and a directory listing per folder, so the
    /// endpoint list asking each row once a frame is a frame broken by the file system. It is also the
    /// only way the component can know: since `task-1905` the answer depends on the shell profile, which
    /// is `unluminous-app`'s to read and not a component's.
    pub readiness: &'a [Option<String>],
}

impl PaneState {
    /// Forget what was being looked at, which is what leaving a conversation means.
    ///
    /// A selection, a lingering copy button, an open right click menu and an opened tool block all
    /// name something by an id, and ids are **per conversation** — so kept across a switch they name
    /// whatever happens to carry that number in the conversation just opened. The rendered markdown
    /// is keyed the same way; it re-renders itself when the source under a key changes, so this is
    /// about the room it holds rather than about correctness.
    pub fn forget_what_was_showing(&mut self) {
        self.selection = None;
        self.copy_shown = None;
        self.menu = None;
        self.opened_tools.clear();
        self.opened_thinking.clear();
        self.rendered.forget();
    }
}

impl std::fmt::Debug for PaneState {
    /// Written by hand because a `TextureHandle` has no `Debug` and a rendered markdown cache is a
    /// screenful of glyph positions. What is printed is what a failing assertion wants to see.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PaneState")
            .field("jump_to_bottom", &self.jump_to_bottom)
            .field("history_open", &self.history_open)
            .field("model_select_open", &self.model_select.as_ref().is_some_and(|one| one.menu.open))
            .field("opened_tools", &self.opened_tools)
            .field("pictures", &self.pictures.len())
            .finish()
    }
}

/// The plugin.
pub struct AgentChat {
    open: bool,
    folder: Option<PathBuf>,
    project: Option<PathBuf>,
    /// The file showing in the window, which the window tells the provider — see
    /// [`UiProvider::showing`]. A chat in an editor that does not know what you are looking at is a
    /// browser tab.
    showing: Option<PathBuf>,
    configuration: Configuration,
    store: Store,
    session: Session,
    client: Client,
    /// What is being typed, and what is attached to it.
    pub draft: String,
    pub attachments: Vec<Attachment>,
    /// What was sent while an answer was still arriving, in the order it was sent.
    ///
    /// **Held here rather than pushed into the conversation**, and that is the whole of the design.
    /// An answer is written into a message that `Session` makes lazily on the first word that
    /// arrives, so a question pushed into the transcript while one was streaming could land *before*
    /// the answer it is a reply to — and the transcript is what goes back up the wire. So a queued
    /// question is drawn as a row of its own, quietly, and is pushed into the conversation at the
    /// moment its turn starts. `task-2060`: *"I should be able to send new messages that get added to
    /// the queue when the agent is working. I should see my message immediately posted after I send
    /// it."*
    queued: Vec<Message>,
    next_attachment: u64,
    outstanding: Vec<Outstanding>,
    /// How many tool calls this turn has made, so the calls are bounded as well as the rounds.
    calls: u32,
    /// Requests made outside a draw — running a tool — drained by the window once a frame.
    asking: Vec<Request>,
    history: Vec<Summary>,
    /// Something that went wrong before a request could go out, drawn at the top of the composer.
    pub problem: Option<String>,
    pub ui: PaneState,
    /// Whether anything changed since the conversation was last written down.
    dirty: bool,
    /// Why each endpoint cannot answer, and when that was last worked out. See [`AgentChat::readiness`].
    readiness: Vec<Option<String>>,
    readiness_taken: Option<std::time::Instant>,
    /// Which wire shapes a row could be switched to here. See [`AgentChat::shapes_available`].
    shapes: Vec<&'static str>,
    shapes_taken: Option<std::time::Instant>,
}

impl std::fmt::Debug for AgentChat {
    /// Written by hand because a `Client` holds a channel and a `Session` holds a whole transcript,
    /// and what a failing assertion wants to see is neither.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("AgentChat")
            .field("open", &self.open)
            .field("provider", &self.configuration.provider().map(|one| one.name.clone()))
            .field("state", &self.session.state().name())
            .field("messages", &self.session.chat.messages.len())
            .finish()
    }
}

impl Default for AgentChat {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentChat {
    pub fn new() -> Self {
        Self {
            open: false,
            folder: None,
            project: None,
            showing: None,
            configuration: Configuration::default(),
            store: Store::at(None),
            session: Session::new(Conversation::new("", "")),
            client: Client::new(),
            draft: String::new(),
            attachments: Vec::new(),
            queued: Vec::new(),
            next_attachment: 1,
            outstanding: Vec::new(),
            calls: 0,
            asking: Vec::new(),
            history: Vec::new(),
            problem: None,
            ui: PaneState { jump_to_bottom: true, ..PaneState::default() },
            dirty: false,
            readiness: Vec::new(),
            readiness_taken: None,
            shapes: Vec::new(),
            shapes_taken: None,
        }
    }

    /// The pane's own pieces, split so that the drawing can read the conversation while it writes
    /// into the draft.
    pub fn parts(&mut self) -> Parts<'_> {
        // Before the borrows below, because it needs `&mut self` to refresh its own cache.
        let _ = self.readiness();
        Parts {
            session: &self.session,
            configuration: &self.configuration,
            state: &mut self.ui,
            draft: &mut self.draft,
            attachments: &self.attachments,
            queued: &self.queued,
            history: &self.history,
            problem: self.problem.as_deref(),
            readiness: &self.readiness,
        }
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub fn configuration_mut(&mut self) -> &mut Configuration {
        &mut self.configuration
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The session, to drive by hand.
    ///
    /// The two callers `UiProvider::as_any_mut` exists for: a test building a conversation the way the
    /// wire would have built it, and the drawing. Nothing outside the plugin reaches it.
    pub fn session_mut(&mut self) -> &mut Session {
        self.dirty = true;
        &mut self.session
    }

    pub fn chat(&self) -> &Conversation {
        &self.session.chat
    }

    pub fn history(&self) -> &[Summary] {
        &self.history
    }

    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// The endpoint in use.
    pub fn provider(&self) -> Option<&Provider> {
        self.configuration.provider()
    }

    /// Write the configuration back, and say so if it could not be written.
    pub fn save_the_configuration(&mut self) -> Result<(), String> {
        let Some(folder) = &self.folder else {
            return Ok(());
        };
        self.configuration.write(folder)
    }

    /// The environment the pane answers out of: the person's own shell profile.
    ///
    /// **The one place `unluminous-chat` and `services::login_shell` are joined**, so a second reader
    /// cannot come to a different answer about the same key. An Unluminous started from the Dock has
    /// launchd's dozen variables and `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, so a key exported from
    /// `~/.zshrc` is not in this process at all and neither is `~/.local/bin` — which is what
    /// `task-1905` reports and what `login_shell`'s own module comment records from measuring it twice.
    ///
    /// `for_a_child()` is the same value the Agent-Tasks board's agents, the run tile's programs and the
    /// debug adapters are all started with, so there is one reading of the profile behind four consumers.
    /// It is an `OnceLock` read after the first call, so this costs a vector rather than a shell.
    pub fn the_environment() -> unluminous_chat::Environment {
        let mut variables = crate::services::login_shell::for_a_child();
        // **And `unluminous-cli` on the agent's `PATH`**, which is `task-2004`'s report said about a chat
        // node rather than a terminal node: an agent this pane starts is the same agent, on the same
        // project, and it reached the window the same way — which is to say not at all, because the name
        // it guesses is on nobody's `PATH`. See `agent_tasks::how_to_reach_this_window`, which is the one
        // answer both routes use.
        variables.extend(crate::services::agent_tasks::how_to_reach_this_window(&variables));
        unluminous_chat::Environment::from(variables)
    }

    /// Why each endpoint cannot answer, or `None` where it can — worked out at most every
    /// [`READINESS`] rather than every frame.
    ///
    /// **Because for a program the question is a walk of `PATH`.** `Provider::why_not` reads
    /// `PATHEXT`, builds a candidate name for each extension and asks the file system about each one
    /// in each folder; the settings page asked it once a row once a frame, which is `task-1666`'s
    /// rule about a frame costing what is on the screen broken by a directory listing.
    pub fn readiness(&mut self) -> Vec<Option<String>> {
        let stale = self.readiness_taken.is_none_or(|at| at.elapsed() >= READINESS);
        if stale || self.readiness.len() != self.configuration.providers.len() {
            // Built once for the whole list rather than once a row: it allocates the profile.
            let environment = Self::the_environment();
            self.readiness = self
                .configuration
                .providers
                .iter()
                .map(|provider| provider.why_not(&environment))
                .collect();
            self.readiness_taken = Some(std::time::Instant::now());
        }
        self.readiness.clone()
    }

    /// Which wire shapes a row could really be switched to on this machine.
    ///
    /// Every address shape, always — nothing about a URL is a fact about this machine. A **program**
    /// shape only when the program is installed, which is `task-2003`: *"why do we show codex option if
    /// its not on the machine?"* A button that turns a working endpoint into a broken one is not a
    /// choice, and Unluminous's rule is that a control which cannot apply is absent rather than dimmed.
    ///
    /// The Settings page still draws the shape a row **already** uses whether or not it is in here, so a
    /// row is never left naming something that is not on its own row of buttons.
    ///
    /// Cached on the same clock as [`Self::readiness`] and for the same reason: the answer is a walk of
    /// `PATH` per shape, and a page asking once a shape once a row once a frame is a frame broken by the
    /// file system.
    pub fn shapes_available(&mut self) -> Vec<&'static str> {
        let stale = self.shapes_taken.is_none_or(|at| at.elapsed() >= READINESS);
        if stale {
            let environment = Self::the_environment();
            self.shapes = unluminous_chat::provider::WIRES
                .iter()
                .copied()
                .filter(|named| {
                    unluminous_chat::provider::Wire::from_name(named)
                        .is_some_and(|wire| wire.is_available(&environment))
                })
                .collect();
            self.shapes_taken = Some(std::time::Instant::now());
        }
        self.shapes.clone()
    }

    /// Ask again on the next frame, because the answer may have changed.
    ///
    /// Called where a row is edited: a program name being typed would otherwise go on saying `Ready`
    /// about the program it used to name for as long as five seconds.
    pub fn readiness_may_have_changed(&mut self) {
        self.readiness_taken = None;
    }

    /// Scroll whatever list this pane is showing, because the window read a wheel over it.
    ///
    /// **A canvas node's layer registers no `AreaState`**, so `Context::rect_contains_pointer` is false
    /// everywhere inside one and `egui::ScrollArea` never takes the wheel itself. That is `task-1905`'s
    /// *"I can't scroll the node"* about a folder node and `task-2003`'s *"I cant scroll agent chat in
    /// base of infinite space"* about this one — the same fault, and the same answer: the window reads
    /// the wheel and says how far to move, and `egui` is handed an offset the way [`Self::zoomed`]
    /// already hands it one.
    ///
    /// `at` is where the pointer is in the points the pane was drawn in, so a wheel over the header or
    /// the composer is not the list's. Answers whether it took it, which is what tells the caller to
    /// take the wheel out of the frame so the canvas does not zoom at the same time.
    ///
    /// The three lists — the conversation, the history and the endpoints — share one set of numbers
    /// because one of them is drawn at a time, in the same rectangle. See [`PaneState::scrolled`].
    pub fn scroll_at(&mut self, at: egui::Pos2, wheel: f32) -> bool {
        if wheel.abs() < 0.5 || !self.ui.list_rect.is_some_and(|rect| rect.contains(at)) {
            return false;
        }
        // Nothing to scroll is not something to take the wheel for: the canvas's own zoom should still
        // get it, which is what a wheel over a pane with a two line conversation means.
        if self.ui.scrollable <= 0.5 {
            return false;
        }
        self.ui.wheel = Some(wheel);
        true
    }

    /// Start a new conversation, keeping the one that was open.
    /// Which conversation this chat is on, as [`Store`] names one.
    ///
    /// Read back by a chat node so the canvas can write it down: a node that reopened the newest
    /// conversation would be a second view of whatever the pane last looked at, and a canvas of agents
    /// would come back as several views of one. See `services::space::node::Chat::conversation`.
    pub fn conversation_id(&self) -> &str {
        &self.session.chat.id
    }

    /// What the header calls this conversation, which is the first thing said in it until it is named.
    pub fn display_name(&self) -> String {
        self.session.chat.display_name().to_owned()
    }

    pub fn new_conversation(&mut self) {
        self.stop_before_switching();
        self.write_the_conversation();
        let provider = self.provider().map(|one| one.name.clone()).unwrap_or_default();
        let id = self.store.new_id();
        self.session = Session::new(Conversation::new(id, provider));
        self.attachments.clear();
        self.problem = None;
        self.ui.jump_to_bottom = true;
        self.refresh_the_history();
    }

    /// Open one out of the history.
    pub fn open_conversation(&mut self, id: &str) -> Result<(), String> {
        let Some(chat) = self.store.read(id) else {
            return Err(format!("there is no conversation called `{id}`."));
        };
        self.stop_before_switching();
        self.write_the_conversation();
        if !chat.provider.is_empty() {
            let _ = self.configuration.choose(&chat.provider);
        }
        self.session = Session::new(chat);
        self.problem = None;
        self.ui.jump_to_bottom = true;
        Ok(())
    }

    /// Throw one away.
    pub fn remove_conversation(&mut self, id: &str) -> Result<(), String> {
        self.store.remove(id)?;
        if self.session.chat.id == id {
            self.new_conversation();
        } else {
            self.refresh_the_history();
        }
        Ok(())
    }

    /// Attach a picture from a file.
    ///
    /// Read into bytes here rather than remembered as a path, which is `model::Part::Picture`'s own
    /// reason: a conversation reopened after the file has moved still shows what was sent.
    pub fn attach(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path)
            .map_err(|problem| format!("{} could not be read: {problem}", path.display()))?;
        let media = media_type_of(path, &bytes)
            .ok_or_else(|| format!("{} is not a picture Unluminous can send.", path.display()))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "picture".to_owned());
        self.attach_bytes(name, media, bytes);
        Ok(())
    }

    /// Attach the picture the window read off the clipboard.
    fn attach_from(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let media = value["media"].as_str().unwrap_or("image/png").to_owned();
        let name = value["name"].as_str().unwrap_or("pasted.png").to_owned();
        let bytes = unluminous_chat::base64::decode(value["data"].as_str().unwrap_or_default())
            .ok_or_else(|| "the picture on the clipboard could not be read.".to_owned())?;
        self.attach_bytes(name, media, bytes);
        Ok(())
    }

    /// Attach a picture already in memory, which is what a paste from the clipboard is.
    pub fn attach_bytes(&mut self, name: String, media: String, bytes: Vec<u8>) {
        let id = self.next_attachment;
        self.next_attachment += 1;
        self.attachments.push(Attachment { id, name, media, bytes });
    }

    /// The attachments as the picture parts of a message, taken off the draft.
    ///
    /// Taken rather than copied, because a picture that stayed on the draft after being sent would go
    /// up again with the next question.
    pub fn take_the_attachments(&mut self) -> Vec<Part> {
        std::mem::take(&mut self.attachments)
            .into_iter()
            .map(|attachment| Part::Picture {
                media: attachment.media,
                bytes: attachment.bytes,
                name: attachment.name,
            })
            .collect()
    }

    pub fn remove_attachment(&mut self, id: u64) {
        self.attachments.retain(|one| one.id != id);
    }

    /// Send what is being typed, or queue it when an answer is still arriving.
    ///
    /// Answers the message's id, or a sentence saying why nothing went — which is where a URL with a
    /// typo in it and a missing key are reported, before a request rather than thirty seconds after
    /// one.
    ///
    /// **A question asked while an answer is arriving waits its turn rather than being refused.**
    /// `task-2060` asks for exactly that, and for the question to be on the screen the moment it is
    /// sent: the id comes back at once, the words are drawn as a queued row straight away, and
    /// [`Self::take_the_replies`] starts the turn as soon as the one in front of it ends. What is
    /// **not** done is pushing it into the conversation now — see [`AgentChat::queued`].
    pub fn send(&mut self) -> Result<u64, String> {
        if self.draft.trim().is_empty() && self.attachments.is_empty() {
            return Err("there is nothing to send.".to_owned());
        }
        let Some(provider) = self.provider().cloned() else {
            return Err(
                "no endpoint is configured. Settings -> Agent-Chat is where they go.".to_owned()
            );
        };
        if let Some(why) = provider.why_not(&Self::the_environment()) {
            self.problem = Some(why.clone());
            return Err(why);
        }
        let id = self.session.chat.next_id();
        let mut message = Message::new(id, Role::User);
        let said = std::mem::take(&mut self.draft);
        if !said.trim().is_empty() {
            message.parts.push(Part::Text(said.trim_end().to_owned()));
        }
        message.parts.extend(self.take_the_attachments());
        self.session.chat.provider = provider.name.clone();
        self.ui.jump_to_bottom = true;
        self.problem = None;
        if self.session.is_busy() {
            self.queued.push(message);
            return Ok(id);
        }
        self.begin_a_turn(message, &provider);
        Ok(id)
    }

    /// Put one question into the conversation and start the turn that answers it.
    ///
    /// The one place a turn begins, so the bounds a turn carries — the rounds, the calls outstanding
    /// — start again wherever the question came from: the composer, the command line, or the queue.
    fn begin_a_turn(&mut self, message: Message, provider: &Provider) {
        self.session.ask_keeping_the_id(message);
        // A new turn, so the bounds start again.
        self.calls = 0;
        self.outstanding.clear();
        self.problem = None;
        self.dirty = true;
        self.dispatch(provider);
    }

    /// What is waiting its turn, in the order it was sent.
    pub fn queued(&self) -> &[Message] {
        &self.queued
    }

    /// What the pane has selected inside one message, as words, or [`None`] when nothing is.
    ///
    /// **The rendered words rather than the source**, which is what somebody dragged across: a
    /// heading comes back without its hashes and a list item without its dash. Copying the source is
    /// a separate row on the menu and is what the button beside the bubble has always done.
    pub fn selected_text(&self) -> Option<String> {
        let selected = self.ui.selection.filter(|one| !one.range.is_empty())?;
        self.ui.rendered.slice(&selected.key(), selected.range.range())
    }

    /// Select the whole of one message's words, which is what the menu's `Select All` means.
    ///
    /// Nothing happens for a message that has not been drawn yet: what is being selected is the
    /// rendered markdown, and there is none until the row has been on the screen once.
    pub fn select_the_whole_message(&mut self, id: u64) {
        let key = format!("message-{id}");
        let Some(length) = self.ui.rendered.length(&key) else {
            return;
        };
        self.ui.selection =
            Some(Selected { message: id, range: unluminous_core::Selection::new(0, length) });
    }

    /// Start the turn for the question at the head of the queue.
    ///
    /// Called the moment the turn in front of it ends. A turn that **failed** does not pull the next
    /// one through — see [`Self::give_the_queue_back`] — because three queued questions against an
    /// endpoint that is not answering would be three failures nobody could stop.
    fn send_the_next_queued(&mut self) {
        let Some(provider) = self.provider().cloned() else {
            return;
        };
        if self.queued.is_empty() {
            return;
        }
        let message = self.queued.remove(0);
        self.ui.jump_to_bottom = true;
        self.begin_a_turn(message, &provider);
    }

    /// Put everything still queued back in the composer, and say so.
    ///
    /// **Nothing somebody typed is thrown away.** Stopping an answer, switching conversation and a
    /// turn that failed all end the queue, and in each of those the words waiting in it are words a
    /// person wrote a moment ago — so they go back where they were typed, above whatever is being
    /// typed now, with their pictures attached again. Answers how many came back, so the caller can
    /// say so where somebody is looking.
    fn give_the_queue_back(&mut self) -> usize {
        if self.queued.is_empty() {
            return 0;
        }
        let queued = std::mem::take(&mut self.queued);
        let count = queued.len();
        let mut said: Vec<String> = Vec::new();
        for message in queued {
            for part in message.parts {
                match part {
                    Part::Text(text) if !text.trim().is_empty() => said.push(text),
                    Part::Text(_) => {}
                    Part::Picture { media, bytes, name } => self.attach_bytes(name, media, bytes),
                }
            }
        }
        if !self.draft.trim().is_empty() {
            said.push(std::mem::take(&mut self.draft));
        }
        self.draft = said.join(
            "

",
        );
        count
    }

    /// Put the request on the wire. The session has already been told a turn is starting.
    ///
    /// **Checked again here rather than only at `send`**, because a round after a tool is a request
    /// nobody pressed a button for: a key cleared out of the environment while a turn was running
    /// would otherwise be a request that fails at the far end rather than a sentence in the pane.
    fn dispatch(&mut self, provider: &Provider) {
        if let Some(why) = provider.why_not(&Self::the_environment()) {
            self.problem = Some(why.clone());
            self.session.reply(unluminous_chat::Reply::Failed(why));
            return;
        }
        if provider.is_a_program() {
            let ask = self.what_to_ask_the_agent();
            self.client.ask(provider, ask);
            return;
        }
        let tools = match self.configuration.tools {
            true => tools::offered(),
            false => Vec::new(),
        };
        let body = unluminous_chat::wire::request(
            provider,
            &self.session.chat,
            &self.system_prompt(),
            &tools,
            self.configuration.stream,
        );
        self.client.send(
            provider,
            body.to_string(),
            self.configuration.stream,
            &Self::the_environment(),
        );
    }

    /// What one turn asks a command-line agent.
    ///
    /// **One turn's words, not the transcript.** The agent keeps its own session, so a second
    /// question is `--resume` and the context it has built is the context it keeps; sending the whole
    /// conversation again would be paying twice for something the agent already has, and would
    /// confuse an agent whose own record of the turn includes tools Unluminous never saw.
    ///
    /// Unluminous's own line about where it is goes in front of the **first** question only, for the same
    /// reason: after that the agent knows, and repeating it every turn would be a paragraph of
    /// preamble on every message.
    fn what_to_ask_the_agent(&mut self) -> unluminous_chat::Ask {
        let newest =
            self.session.chat.messages.iter().rev().find(|message| message.role == Role::User);
        let said = newest.map(Message::text).unwrap_or_default();
        let session = self.session.chat.session.clone();
        let prompt = match session.is_empty() {
            true => format!("{}\n\n{said}", self.what_the_agent_should_know()),
            false => said,
        };
        // **Written to files, because both agents take a picture by path.** The pane holds one as
        // bytes so that a conversation reopened after the file moved still shows what was sent; an
        // agent needs a file, so one is written into the temporary folder for the length of the turn.
        let mut pictures = Vec::new();
        if let Some(message) = newest {
            let folder = std::env::temp_dir();
            for (name, _, bytes) in message.pictures() {
                if let Some(path) = unluminous_chat::agent::write_a_picture(&folder, name, bytes) {
                    pictures.push(path);
                }
            }
        }
        unluminous_chat::Ask {
            prompt,
            // **The project the window has open**, which is what makes the agent's answer about the
            // code in front of you: `claude` finds that project's `CLAUDE.md` there and `codex` its
            // `AGENTS.md`, and every file tool either of them has is rooted in it.
            folder: self.project.clone(),
            session,
            pictures,
            permission: self.configuration.permission,
            // **The person's own shell profile**, so the agent is found where they installed it and
            // starts logged in — the two faults `login_shell` records from measuring them, both of
            // which `task-1905` would have hit next.
            environment: Self::the_environment(),
        }
    }

    /// The line Unluminous puts in front of an agent's first question.
    ///
    /// Shorter than [`system_prompt`](Self::system_prompt), and deliberately: an agent already knows
    /// what it is and has its own instructions from the project. What it cannot know is that it is
    /// answering in a pane rather than in a terminal, and which file the person is looking at.
    fn what_the_agent_should_know(&self) -> String {
        let mut lines = vec![
            "You are answering in a chat pane inside Unluminous, a code editor, beside the person's work. Be brief and concrete; they can see their own screen, and your answer is read as markdown rather than in a terminal.".to_owned(),
        ];
        if let Some(showing) = &self.showing {
            lines.push(format!("The file they are looking at is {}.", showing.display()));
        }
        if !self.configuration.system.trim().is_empty() {
            lines.push(self.configuration.system.trim().to_owned());
        }
        lines.join(" ")
    }

    /// What Unluminous tells the model about where it is.
    ///
    /// Unluminous's own line first, then the person's. **Which project is open and which file is showing,
    /// and not the file's text**: a pane that quietly uploaded whatever was on the screen is a pane
    /// nobody could use on anything confidential. With the tools on, the model can read the file by
    /// asking, which is the right shape for an editor whose every command is already a tool.
    pub fn system_prompt(&self) -> String {
        let mut lines = vec![
            "You are answering inside Unluminous, a code editor, in a pane beside the person's work. Be brief and concrete; they can see their own screen.".to_owned(),
        ];
        if let Some(project) = &self.project {
            lines.push(format!("The project open is {}.", project.display()));
        }
        if let Some(showing) = &self.showing {
            lines.push(format!("The file showing is {}.", showing.display()));
        }
        if self.configuration.tools {
            lines.push(
                "You can drive this window with the tools you have been given. They are Unluminous's own commands, so anything you do this way is exactly what the person would get from the menu, and it is one undo step. Read a file before changing it."
                    .to_owned(),
            );
        }
        if !self.configuration.system.trim().is_empty() {
            lines.push(self.configuration.system.trim().to_owned());
        }
        lines.join("\n")
    }

    /// End the turn in flight before the conversation under it is replaced.
    ///
    /// **Without this every later word went into the wrong conversation.** The client's generation
    /// was still current, so the text, the tool results, the usage, the failure and the session id of
    /// the turn that was running were all applied to whichever conversation had just been opened —
    /// and then written to disk over it. It is [`stop`](Self::stop) rather than a flag because the
    /// answer belongs to the conversation being left: it is finished there, marked `stopped`, and
    /// written down with what had arrived.
    fn stop_before_switching(&mut self) {
        if self.session.is_busy() {
            self.stop();
        }
        self.outstanding.clear();
        self.calls = 0;
        // Whatever is still queued belongs to the conversation being left — its ids are that
        // conversation's — so it goes back to the composer rather than being asked of the next one.
        self.give_the_queue_back();
        self.ui.forget_what_was_showing();
    }

    /// Stop whatever is arriving, keeping it.
    ///
    /// **Stop means stop, including what is queued.** A question waiting its turn would otherwise be
    /// sent the instant the answer was stopped, which is the opposite of what the button says — so
    /// the queue goes back into the composer, where nothing typed is lost and sending it again is one
    /// key press.
    pub fn stop(&mut self) {
        self.client.stop();
        self.outstanding.clear();
        if self.session.is_busy() {
            self.session.stop();
            self.dirty = true;
        }
        if self.give_the_queue_back() > 0 {
            self.asking
                .push(Request::Message("what was queued is back in the composer.".to_owned()));
        }
    }

    /// Read the replies that have arrived and act on what they mean.
    ///
    /// The whole of what happens between frames: replies in, tool calls out, the next round sent
    /// when the tools have all answered. Answers whether anything is still happening.
    fn take_the_replies(&mut self) -> bool {
        let replies = self.client.take();
        let anything = !replies.is_empty();
        for reply in replies {
            // **A turn that failed raises a notice**, so the reason is on the screen rather than only in
            // the message's own `failure`. `task-1848` reported the server's own `HTTP 500: the current
            // context does not logits computation` as something that had to be found by reading the
            // conversation back as data — the pane showed an answer with no words in it and said nothing
            // about why. The server's words are carried verbatim, which is `unluminous_git`'s rule about
            // never inventing an error message.
            if let unluminous_chat::Reply::Failed(why) = &reply {
                self.asking.push(Request::Notice {
                    text: why.clone(),
                    kind: crate::components::toast::Kind::Problem,
                });
                self.problem = Some(why.clone());
            }
            self.session.reply(reply);
            self.dirty = true;
        }
        if anything {
            self.session.chat.changed = store::seconds_now();
        }
        // **A command-line agent runs its own tools, so Unluminous runs none of them.** Measured against
        // a real `claude`: it asked for `Grep` and `Read`, which are its own tools and are not in
        // Unluminous's catalogue, and this window answered both with "there is no tool called `Grep`" —
        // put that refusal in the transcript, made a `tool` message nobody asked for, and sent
        // another round. The agent recovered, but it had been argued with by its own client.
        // `WaitingForTools` still means what it says there; what it means is *the agent* is running
        // one, which is exactly what the block in the pane is drawing.
        let mine_to_run = !self.configuration.provider().is_some_and(|one| one.is_a_program());
        if mine_to_run && matches!(self.session.state(), State::WaitingForTools) {
            self.ask_for_the_tools();
        }
        if anything && !self.session.is_busy() {
            self.write_the_conversation();
        }
        // **The next queued question goes the moment the turn in front of it ends**, which is what
        // makes the queue a queue rather than a list.
        match what_the_queue_does_next(self.queued.len(), self.session.state()) {
            Queue::Waits => {}
            Queue::Goes => self.send_the_next_queued(),
            Queue::GoesBack => {
                let count = self.give_the_queue_back();
                self.asking.push(Request::Notice {
                    text: format!(
                        "{count} queued {} back in the composer, because the answer before {} failed.",
                        match count == 1 {
                            true => "message is",
                            false => "messages are",
                        },
                        match count == 1 {
                            true => "it",
                            false => "them",
                        },
                    ),
                    kind: crate::components::toast::Kind::Problem,
                });
            }
        }
        self.session.is_busy()
    }

    /// Hand every outstanding tool call to the window, once each.
    ///
    /// Three things are refused here rather than run, and each goes back up as the call's answer so
    /// the model reads it and picks something else — a turn hanging on a call nobody will run is the
    /// one outcome with nothing on the screen to explain it.
    fn ask_for_the_tools(&mut self) {
        for (at, call) in self.session.tools_to_run() {
            if self.outstanding.iter().any(|one| one.at == at) {
                continue;
            }
            self.outstanding.push(Outstanding { at, began: std::time::Instant::now() });
            // **The calls of a turn are bounded as well as its rounds.** `tool_limit` bounds how many
            // times the model may be asked again; nothing bounded how many calls one answer could
            // ask for, and one answer can hold any number.
            self.calls += 1;
            if self.calls > self.call_limit() {
                self.answered_at(
                    at,
                    Err(format!(
                        "this turn has already asked for {} tools, which is the limit \
                         (Tool rounds in Settings -> Agent-Chat, four calls a round).",
                        self.call_limit()
                    )),
                );
                continue;
            }
            // Arguments that are not JSON are a refusal rather than an empty object, because reading
            // them as `{}` runs the command with its defaults — see `ToolCall::parsed_arguments`.
            let arguments = match call.parsed_arguments() {
                Ok(arguments) => arguments,
                Err(problem) => {
                    self.answered_at(at, Err(problem));
                    continue;
                }
            };
            match tools::resolve(&call.name, &arguments, self.configuration.shell) {
                Ok(resolved) => self.asking.push(Request::RunCommand {
                    id: format!("{at}"),
                    command: resolved.command.wire(),
                    arguments: resolved.arguments,
                }),
                Err(problem) => self.answered_at(at, Err(problem)),
            }
        }
    }

    /// The most tool calls one turn may make.
    ///
    /// Four a round, which is the shape of the thing being bounded: a round is one answer from the
    /// model, and an answer that asks for more than a handful of tools at once is an answer that has
    /// misunderstood the question. One number in the settings rather than two, because two numbers
    /// nobody can tell apart is worse than a ratio written down here.
    fn call_limit(&self) -> u32 {
        self.configuration.tool_limit.saturating_mul(4)
    }

    /// What the window answered: a tool call by its position, or the clipboard's picture.
    fn tool_answered(&mut self, id: &str, answer: Result<serde_json::Value, String>) {
        if id == CLIPBOARD {
            // **Nothing on the clipboard is not a fault.** The window answers `null` when there is no
            // picture there, because the paste chord is now seen on the key going back up and that happens
            // after an ordinary text paste as well — see `components::agent_chat::pasting`. Saying "there
            // is no picture on the clipboard" under the composer every time somebody pasted a sentence
            // into it would be a message about nothing. A picture that is there and will not decode is a
            // real failure and still says so.
            if matches!(answer, Ok(serde_json::Value::Null)) {
                return;
            }
            match answer.and_then(|value| self.attach_from(&value)) {
                Ok(()) => self.problem = None,
                Err(problem) => self.problem = Some(problem),
            }
            return;
        }
        let Ok(at) = id.parse::<usize>() else {
            return;
        };
        self.answered_at(at, answer.map(|value| shorten_for_a_model(&value)));
    }

    /// The same, for a call this side refused before it ever reached the window.
    fn answered_at(&mut self, at: usize, answer: Result<String, String>) {
        let took = self
            .outstanding
            .iter()
            .find(|one| one.at == at)
            .map(|one| one.began.elapsed().as_millis().min(u64::MAX as u128) as u64)
            .unwrap_or(0);
        self.outstanding.retain(|one| one.at != at);
        self.dirty = true;
        if !self.session.tool_answered(at, answer, took) {
            return;
        }
        // Every tool has answered, so the model is asked again — unless it has been asked enough
        // times already, which is what stops a loop nobody is watching from being funded.
        if self.session.round() >= self.configuration.tool_limit {
            self.session.reply(unluminous_chat::Reply::Failed(format!(
                "the model asked for tools {} times in one turn, which is the limit in Settings -> Agent-Chat.",
                self.session.round()
            )));
            self.write_the_conversation();
            return;
        }
        let Some(provider) = self.provider().cloned() else {
            return;
        };
        self.session.begin();
        self.dispatch(&provider);
    }

    /// Write the conversation down if anything changed.
    fn write_the_conversation(&mut self) {
        if !self.dirty || self.session.chat.messages.is_empty() {
            return;
        }
        self.dirty = false;
        self.session.chat.changed = store::seconds_now();
        if let Err(problem) = self.store.write(&self.session.chat) {
            self.problem = Some(problem);
        }
        self.store.tidy(self.configuration.history);
        self.refresh_the_history();
    }

    fn refresh_the_history(&mut self) {
        self.history = self.store.list(self.configuration.history);
    }

    /// What the pane holds, as data — which is what `unluminous-cli plugins view agent-chat` prints.
    fn view_value(&self) -> serde_json::Value {
        serde_json::json!({
            "provider": self.provider().map(|one| serde_json::json!({
                "name": one.name,
                "wire": one.wire.name(),
                "url": one.url,
                "model": one.model,
                "key": one.has_a_key(&Self::the_environment()),
            })),
            "state": self.session.state().name(),
            "model": self.session.model,
            "round": self.session.round(),
            // Where the conversation is scrolled to, and how far it *can* be scrolled, so that "is it at
            // the bottom" is a question something other than a screenshot can answer. `task-1848` reported
            // pressing Enter scrolling to the top, and on a machine where `window screenshot` does not
            // work there was no way to check it from outside the window at all.
            "scrolled": self.ui.scrolled,
            "scrollable": self.ui.scrollable,
            "tools": self.configuration.tools,
            "stream": self.configuration.stream,
            "streaming": self.session.is_busy(),
            "draft": self.draft,
            // What was sent while an answer was arriving and has not gone yet, so an agent driving
            // the pane can tell "it is waiting its turn" from "it was never sent". `task-2060`.
            "queued": self.queued.iter().map(|one| serde_json::json!({
                "id": one.id, "text": one.text(),
            })).collect::<Vec<serde_json::Value>>(),
            "attachments": self.attachments.iter().map(|one| serde_json::json!({
                "name": one.name, "media": one.media, "bytes": one.bytes.len(),
            })).collect::<Vec<serde_json::Value>>(),
            "problem": self.problem,
            // **Which file, never the file's text.** A pane that quietly uploaded whatever was on
            // the screen is a pane nobody could use on anything confidential; with the tools on the
            // model can read it by asking, which is the right shape for an editor whose every command
            // is already a tool.
            "showing": self.showing.as_ref().map(|path| path.display().to_string()),
            "project": self.project.as_ref().map(|path| path.display().to_string()),
            // Deliberately **not** `total`, which is the key the pane's own header draws as a count
            // beside its name. A board's count is how many tickets there are and is worth reading at a
            // glance; a chat's is how many messages have been said, which is a number nobody wants in
            // a header and which reads as `Agent-Chat 0` on an empty pane.
            "messages": self.session.chat.messages.len(),
            "conversation": self.session.chat.to_json(),
        })
    }
}

/// What becomes of the queue now that a reply has been read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Queue {
    /// Nothing is waiting, or the turn in front of it has not finished.
    Waits,
    /// The turn ended, so the question at the head of the queue is asked.
    Goes,
    /// The turn **failed**, so what is waiting goes back to the composer.
    GoesBack,
}

/// Whether the queue moves, and which way. `task-2060`.
///
/// A decision rather than the act, so it can be asserted with no transport behind it — which is the
/// shape `services::wake`'s escalation already has, and for the same reason: what is worth pinning
/// here is *when* a queued question goes, and nothing about that needs a socket.
///
/// **A turn that failed ends the queue rather than pulling the next one through.** Three questions
/// asked of an endpoint that is not answering would be three failures with nothing on the screen to
/// stop them, and the words in them are words somebody wrote a moment ago — so they go back where
/// they were typed. `Finished` covers a turn somebody stopped as well, but `stop` has already
/// emptied the queue by the time this is asked, so there is nothing left for it to send.
fn what_the_queue_does_next(waiting: usize, state: &State) -> Queue {
    if waiting == 0 || state.is_busy() {
        return Queue::Waits;
    }
    match state {
        State::Failed(_) => Queue::GoesBack,
        _ => Queue::Goes,
    }
}

/// A tool's answer, cut to something worth sending back up.
///
/// A command like `explorer tree` can answer with a megabyte, and sending a megabyte back into the
/// conversation costs the person money and fills the model's context with one answer. `task-1695`
/// measured the other half of this: an agent handed 3,000 tokens to learn one number stops asking.
/// So it is cut, and the cut says so, which is the honest form.
fn shorten_for_a_model(value: &serde_json::Value) -> String {
    const LIMIT: usize = 8000;
    let text = match value {
        serde_json::Value::String(said) => said.clone(),
        serde_json::Value::Null => "ok".to_owned(),
        other => other.to_string(),
    };
    match text.len() > LIMIT {
        true => format!(
            "{}\n… cut short at {LIMIT} characters.",
            &text[..floor_char_boundary(&text, LIMIT)]
        ),
        false => text,
    }
}

/// The largest index at or below `at` that is a character boundary.
///
/// Written out because `str::floor_char_boundary` is not stable, and slicing a `String` of somebody
/// else's JSON in the middle of a character panics.
fn floor_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// What kind of picture this is, from its own first bytes and then from its name.
///
/// The bytes first, because a screenshot saved as `.png` that is really a JPEG is a thing that
/// happens and an API told the wrong media type refuses the whole request.
fn media_type_of(path: &Path, bytes: &[u8]) -> Option<String> {
    let sniffed = match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    };
    if let Some(media) = sniffed {
        return Some(media.to_owned());
    }
    match path.extension().and_then(|kind| kind.to_str()).map(str::to_lowercase).as_deref() {
        Some("png") => Some("image/png".to_owned()),
        Some("jpg") | Some("jpeg") => Some("image/jpeg".to_owned()),
        Some("gif") => Some("image/gif".to_owned()),
        Some("webp") => Some("image/webp".to_owned()),
        _ => None,
    }
}

impl UiProvider for AgentChat {
    fn id(&self) -> &'static str {
        "agent-chat"
    }

    fn open(&mut self, context: &Context) -> Result<(), String> {
        self.folder = context.folder.clone();
        self.project = context.project.clone();
        // **Told at open, not only when it next changes.** The window compares before it tells, so a
        // provider opened after the comparison had already settled was never told at all and its
        // system prompt said nothing about the file in front of the person until they switched tabs.
        self.showing = context.showing.clone();
        self.store = Store::at(self.folder.clone());
        if let Some(folder) = &self.folder {
            let (mut configuration, refused) = Configuration::read(folder);
            // Before anything is drawn or offered, so the endpoint list, the header's chip and the
            // Settings page all see the same list. See [`Configuration::forget_the_agents_that_are_not_installed`].
            configuration.forget_the_agents_that_are_not_installed(&Self::the_environment());
            self.configuration = configuration;
            if !refused.is_empty() {
                self.problem = Some(refused.join(" "));
            }
        }
        self.client.set_waker(context.wake.clone());
        self.refresh_the_history();
        // The newest conversation is reopened, because a pane that came back empty every time would
        // be a pane you cannot leave a question in — which is what `task-1693` asks a project to
        // remember about everything else in the window.
        let newest = self.history.first().map(|one| one.id.clone());
        match newest.and_then(|id| self.store.read(&id)) {
            Some(chat) => {
                if !chat.provider.is_empty() {
                    let _ = self.configuration.choose(&chat.provider);
                }
                self.session = Session::new(chat);
                self.ui.jump_to_bottom = true;
            }
            None => {
                let id = self.store.new_id();
                let provider = self.provider().map(|one| one.name.clone()).unwrap_or_default();
                self.session = Session::new(Conversation::new(id, provider));
            }
        }
        self.open = true;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.open
    }

    /// The decoration `egui` cannot draw: the raised bubbles, the pressed wells, the gradient send
    /// button and its glow. See `services::vello_canvas`.
    fn draws_chrome(&self) -> bool {
        true
    }

    /// Zooming a file does not resize the chat. `task-2096`: *"Zoom on an open file must not change
    /// the agent chat zoom."* The chat's size is its own pane zoom, `panel zoom agent-chat/chat`.
    fn follows_the_editor_font(&self) -> bool {
        false
    }

    fn pane(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::agent_chat::pane(self, ui, look)
    }

    fn settings(&mut self, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
        crate::components::agent_chat::settings_page::show(self, ui, look)
    }

    fn command(&mut self, command: &str, arguments: &[String]) -> Result<Answer, String> {
        let rest = plugin_ui::rest(arguments, 0);
        match command {
            // The window's own name for "put this pane on the screen", which `run_plugin_command`
            // acts on: the menu entry, the rail button and the command line are one path.
            "open-pane" => Ok(Answer::said("the chat")),
            "new" => {
                self.new_conversation();
                Ok(
                    Answer::said("a new conversation")
                        .with(serde_json::json!({ "id": self.session.chat.id })),
                )
            }
            "send" => {
                if !rest.trim().is_empty() {
                    self.draft = rest;
                }
                let queued_before = self.queued.len();
                let id = self.send()?;
                // **A question asked while an answer is arriving is queued rather than refused**, and
                // the answer says which of the two happened — otherwise an agent that sent twice has
                // no way to tell a question that went from one that is waiting. `task-2060`.
                let waiting = self.queued.len() > queued_before;
                // **It does not wait.** `command` is called inside a frame, and a command that
                // blocked would stop the window drawing for the length of a model's answer — which
                // is the sentence `unluminous_git::Worker` exists for. `state` says when it has finished.
                Ok(Answer::said(match waiting {
                    true => "queued",
                    false => "sent",
                })
                .with(serde_json::json!({
                    "id": id,
                    "queued": waiting,
                    "waiting": self.queued.len(),
                    "state": self.session.state().name(),
                })))
            }
            "stop" => {
                self.stop();
                Ok(Answer::said("stopped"))
            }
            "state" => Ok(Answer::said(self.session.state().name()).with(serde_json::json!({
                "state": self.session.state().name(),
                "busy": self.session.is_busy(),
                "round": self.session.round(),
                "characters": self.session.chat.last().map(|last| last.text().len()).unwrap_or(0),
                // How many questions are waiting their turn behind this answer. `task-2060`.
                "queued": self.queued.len(),
                "problem": self.problem,
            }))),
            "messages" => Ok(
                Answer::said(format!("{} messages", self.session.chat.messages.len()))
                    .with(self.session.chat.to_json()),
            ),
            "last" => {
                let last = self
                    .session
                    .chat
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == Role::Assistant);
                Ok(
                    Answer::said(last.map(unluminous_chat::Message::text).unwrap_or_default()).with(
                        serde_json::json!({
                            "text": last.map(unluminous_chat::Message::text),
                            "failure": last.and_then(|message| message.failure.clone()),
                            "finish": last.and_then(|message| message.finish.clone()),
                        }),
                    ),
                )
            }
            "attach" => {
                let path = PathBuf::from(rest.trim());
                if path.as_os_str().is_empty() {
                    return Err("attach takes the path of a picture.".to_owned());
                }
                self.attach(&path)?;
                Ok(Answer::said(format!(
                    "attached {}",
                    crate::services::paths::the_useful_end_of(&path.display().to_string())
                ))
                    .with(serde_json::json!({ "attachments": self.attachments.len() })))
            }
            "providers" => Ok(Answer::said(
                self.configuration
                    .providers
                    .iter()
                    .map(|one| one.name.as_str())
                    .collect::<Vec<&str>>()
                    .join(", "),
            )
            .with(serde_json::json!({
                "chosen": self.provider().map(|one| one.name.clone()),
                "permission": self.configuration.permission.name(),
                "providers": self.configuration.providers.iter().map(|one| serde_json::json!({
                    "name": one.name,
                    "wire": one.wire.name(),
                    // Whether it runs a program or sends to an address, and — for a program — where
                    // that program really is, because an agent that is not installed is the
                    // commonest reason a row cannot answer and a path is what says so.
                    "runs_a_program": one.is_a_program(),
                    "command": one.command,
                    "program": one.program_path(&Self::the_environment()).map(|path| path.display().to_string()),
                    "url": one.url,
                    "model": one.model,
                    "key_env": one.key_env,
                    "key": one.has_a_key(&Self::the_environment()),
                    "why_not": one.why_not(&Self::the_environment()),
                })).collect::<Vec<serde_json::Value>>(),
            }))),
            "use" => {
                self.configuration.choose(rest.trim())?;
                self.save_the_configuration()?;
                Ok(Answer::said(format!("talking to {}", rest.trim())))
            }
            "history" => Ok(
                Answer::said(format!("{} conversations", self.history.len())).with(serde_json::json!(self
                    .history
                    .iter()
                    .map(|one| serde_json::json!({
                        "id": one.id,
                        "name": one.name,
                        "provider": one.provider,
                        "messages": one.messages,
                        "changed": one.changed,
                    }))
                    .collect::<Vec<serde_json::Value>>())),
            ),
            "open" => {
                self.open_conversation(rest.trim())?;
                Ok(Answer::said(format!(
                    "opened {}",
                    self.session.chat.display_name()
                )))
            }
            "remove" => {
                self.remove_conversation(rest.trim())?;
                Ok(Answer::said("removed"))
            }
            "tools" => {
                match rest.trim() {
                    "on" => self.configuration.tools = true,
                    "off" => self.configuration.tools = false,
                    // With nothing said it toggles, because the menu entry that runs it is a
                    // `Toggle` and a menu row cannot carry an argument.
                    "" => self.configuration.tools = !self.configuration.tools,
                    other => return Err(format!("tools takes `on` or `off`, not `{other}`.")),
                }
                self.save_the_configuration()?;
                Ok(Answer::said(match self.configuration.tools {
                    true => "Unluminous's own commands are offered to the model",
                    false => "no tools are offered",
                })
                .with(serde_json::json!({ "tools": self.configuration.tools })))
            }
            "view" => Ok(Answer::said("the pane").with(self.view_value())),
            other => Err(self.refuse(other)),
        }
    }

    fn commands(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("open-pane", "Show the chat pane."),
            ("new", "Start a new conversation."),
            (
                "send",
                "Add a message and start the answer. While one is arriving it is queued instead and goes when that turn ends. Does not wait; `state` says when it has finished and how many are queued.",
            ),
            ("stop", "Stop the answer, keeping what has arrived."),
            (
                "state",
                "Idle, sending, streaming, waiting-for-tools, finished or failed.",
            ),
            ("messages", "The whole conversation as data."),
            ("last", "Just the last answer."),
            ("attach", "Attach a picture to the message being composed."),
            (
                "providers",
                "The endpoints, their URLs and models, and whether each has a key.",
            ),
            ("use", "Talk to one of the endpoints by name."),
            ("history", "The conversations kept, newest first."),
            ("open", "Open one of them by its id."),
            ("remove", "Throw one away."),
            (
                "tools",
                "`on` or `off`: whether Unluminous's own commands are offered to the model.",
            ),
            ("view", "Everything the pane is showing, as data."),
        ]
    }

    fn view(&self) -> serde_json::Value {
        self.view_value()
    }

    fn catch_up(&mut self) -> bool {
        self.take_the_replies()
    }

    fn asking(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.asking)
    }

    fn answered(&mut self, id: &str, answer: Result<serde_json::Value, String>) {
        self.tool_answered(id, answer);
    }

    /// Keep the point the pointer was over still through a zoom of the pane. `task-1771`.
    ///
    /// The conversation is an `egui::ScrollArea` and its offset belongs to `egui`, so what is done here is
    /// to work out where it has to go and ask for it once on the next frame - the shape `jump_to_bottom`
    /// already has. Every bubble is laid out at `Look::scale`, so the whole column's height is proportional
    /// to the zoom and the point at `offset + above` lands at `(offset + above) * ratio`.
    fn zoomed(&mut self, ratio: f32, above: f32) {
        if !ratio.is_finite() || ratio <= 0.0 {
            return;
        }
        let put = (self.ui.scrolled + above) * ratio - above;
        self.ui.scroll_to = Some(put.max(0.0));
    }

    fn showing(&mut self, project: Option<&Path>, file: Option<&Path>) {
        self.project = project.map(Path::to_path_buf);
        self.showing = file.map(Path::to_path_buf);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn close(&mut self) {
        self.stop();
        self.write_the_conversation();
        self.open = false;
    }
}

#[cfg(test)]
mod tests_task_2003 {
    use super::*;

    /// `full` is what a command-line agent may do, and a file written before that reaches it once.
    ///
    /// `task-2003` asks for it outright: *"Agent chat should have full by default."* A default on its
    /// own would have reached a fresh install and nobody else, because this file is rewritten whenever
    /// anything on the page changes — so every existing installation carries `permission = read`
    /// because the code wrote it, not because a person chose it. `version` is what tells the two apart
    /// from here on.
    #[test]
    fn full_is_the_default_and_a_file_written_before_it_takes_it_once() {
        assert_eq!(Configuration::default().permission, unluminous_chat::Permission::Full);

        let folder =
            std::env::temp_dir().join(format!("unluminous-chat-permission-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");

        // A file from before the default moved. It says `read` because that is what was written out.
        std::fs::write(
            folder.join("settings.conf"),
            "permission = read
",
        )
        .expect("a file");
        let (read_back, refused) = Configuration::read(&folder);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(
            read_back.permission,
            unluminous_chat::Permission::Full,
            "a file with no version was written before the default moved, so it takes the new one"
        );

        // Written back, and read again: the version is there now, so what it says is kept — including
        // a `read` somebody really did choose.
        let mut chosen = read_back;
        chosen.permission = unluminous_chat::Permission::Read;
        chosen.write(&folder).expect("it writes");
        assert_eq!(
            Configuration::read(&folder).0.permission,
            unluminous_chat::Permission::Read,
            "a choice made since is a choice, and is not moved again"
        );

        let written = std::fs::read_to_string(folder.join("settings.conf")).expect("it is there");
        assert!(written.contains("version"), "and the version is written down: {written}");
    }

    /// Thirty rounds a turn is the default, and a file holding the old default of eight takes it once.
    ///
    /// `task-2096`. A file written by version 1 says `tool-limit = 8` because the code wrote eight;
    /// any other number in it was chosen and is kept, and once version 2 has written the file an
    /// eight in it is a choice too.
    #[test]
    fn thirty_rounds_is_the_default_and_a_file_holding_the_old_eight_takes_it_once() {
        assert_eq!(Configuration::default().tool_limit, 30);

        let old = |text: &str| Configuration::of(&Values::parse(text)).0.tool_limit;
        assert_eq!(old("tool-limit = 8\n"), 30, "no version: eight is the old default");
        assert_eq!(old("tool-limit = 8\nversion = 1\n"), 30, "version 1: eight is the old default");
        assert_eq!(old("tool-limit = 12\nversion = 1\n"), 12, "a number somebody chose is kept");
        assert_eq!(old("tool-limit = 8\nversion = 2\n"), 8, "written by version 2, eight was chosen");
    }

    /// A row Unluminous ships for an agent that is not here is not offered, and nothing is written.
    ///
    /// `task-2003`: *"why do we show codex option if its not on the machine?"* Three conditions, and
    /// each one is asserted, because each is what keeps this smaller than it sounds.
    #[test]
    fn a_shipped_row_for_an_agent_that_is_not_installed_is_not_offered() {
        use unluminous_chat::provider::Provider;
        // **A `PATH` with one empty folder on it**, rather than no `PATH` at all: `environment::path_of`
        // falls back to this process's own, so an empty environment finds whatever is really installed
        // and the test would answer differently on two machines.
        let nowhere =
            std::env::temp_dir().join(format!("unluminous-nowhere-{}", std::process::id()));
        std::fs::create_dir_all(&nowhere).expect("an empty folder");
        let bare = unluminous_chat::Environment::from(vec![(
            "PATH".to_owned(),
            nowhere.display().to_string(),
        )]);

        let mut shipped =
            Configuration { providers: Provider::defaults(), ..Configuration::default() };
        shipped.forget_the_agents_that_are_not_installed(&bare);
        let left: Vec<&str> = shipped.providers.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(
            left,
            ["local"],
            "with nothing installed, only the row that sends to an address"
        );

        // The chosen one stays, however little it can do, because choosing it is asking for it.
        let mut chosen =
            Configuration { providers: Provider::defaults(), ..Configuration::default() };
        chosen.chosen = "codex".to_owned();
        chosen.forget_the_agents_that_are_not_installed(&bare);
        let left: Vec<&str> = chosen.providers.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(
            left,
            ["codex", "local"],
            "the chosen row says why it cannot run rather than going"
        );

        // And a row somebody wrote stays whatever it names, because it is a thing they meant.
        let mut theirs =
            Configuration { providers: Provider::defaults(), ..Configuration::default() };
        let mut mine = Provider::defaults()[1].clone();
        mine.name = "mine".to_owned();
        theirs.providers.push(mine);
        theirs.forget_the_agents_that_are_not_installed(&bare);
        let left: Vec<&str> = theirs.providers.iter().map(|one| one.name.as_str()).collect();
        assert_eq!(
            left,
            ["local", "mine"],
            "a row nobody shipped is not one of the rows that ship"
        );
    }
}

#[cfg(test)]
mod tests_task_1848 {
    use super::*;

    /// Unluminous's own commands are offered unless somebody turned them off.
    ///
    /// `task-1848` asks for this outright. A configuration written before the default moved has no `tools`
    /// line at all, so it takes the new default — which is the intent: absent means "never chosen", and
    /// that is what a default is for.
    #[test]
    fn tools_are_offered_unless_somebody_turned_them_off() {
        assert!(Configuration::default().tools, "on by default");

        let folder =
            std::env::temp_dir().join(format!("unluminous-chat-tools-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");

        // A file from before the default moved: it says nothing about tools.
        std::fs::write(folder.join("settings.conf"), "stream = true\n").expect("a file");
        assert!(
            Configuration::read(&folder).0.tools,
            "a file that never chose takes the new default"
        );

        // And a file that did choose is obeyed, both ways.
        std::fs::write(folder.join("settings.conf"), "tools = false\n").expect("a file");
        assert!(!Configuration::read(&folder).0.tools, "somebody turned it off, so it is off");
        std::fs::write(folder.join("settings.conf"), "tools = true\n").expect("a file");
        assert!(Configuration::read(&folder).0.tools);

        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The second switch is the one that must not move.
    ///
    /// `tools::RUNS_A_PROGRAM` is `terminal send`, `run add`, `run start`, `debug install` and `launch`,
    /// which between them will run any command line at all on this machine. That is what `chat.shell`
    /// gates, and turning the first switch on is not a reason to turn this one on.
    #[test]
    fn the_shell_switch_is_still_off_by_default() {
        assert!(!Configuration::default().shell);

        let folder =
            std::env::temp_dir().join(format!("unluminous-chat-shell-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(folder.join("settings.conf"), "tools = true\n").expect("a file");
        assert!(
            !Configuration::read(&folder).0.shell,
            "a file that turned the tools on has said nothing about the shell"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-chat-plugin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        folder
    }

    fn opened(name: &str) -> AgentChat {
        let mut chat = AgentChat::new();
        chat.open(&Context { folder: Some(a_folder(name)), ..Context::default() }).expect("opened");
        chat
    }

    #[test]
    fn the_configuration_round_trips_through_the_plugins_own_folder() {
        let folder = a_folder("configuration");
        let mut configuration = Configuration {
            chosen: "codex".to_owned(),
            tools: true,
            tool_limit: 3,
            system: "Be terse.".to_owned(),
            permission: unluminous_chat::Permission::Edit,
            ..Configuration::default()
        };
        configuration.providers[2].url = "http://127.0.0.1:9999/v1/chat/completions".to_owned();
        configuration.providers[2].key_env = "OPENAI_API_KEY".to_owned();
        configuration.write(&folder).expect("written");
        let (read, refused) = Configuration::read(&folder);
        assert_eq!(read, configuration);
        assert!(refused.is_empty(), "{refused:?}");
        let text = std::fs::read_to_string(folder.join(Configuration::FILE)).expect("the file");
        // The two rows that ship run a program, so what is written down for them is the program.
        assert!(text.contains("provider.0.command = claude"), "{text}");
        assert!(text.contains("provider.1.command = codex"), "{text}");
        // And a row that does send names the variable its key comes from — never the key.
        assert!(
            text.contains("OPENAI_API_KEY"),
            "the name of the variable is written down: {text}"
        );
        assert!(!text.to_lowercase().contains("secret"));
    }

    #[test]
    fn a_row_naming_a_wire_this_version_has_not_got_is_refused_rather_than_half_loaded() {
        // The rule `plugin.kind`, `language.renders`, `run.project`, `debug.adapter` and `ui.chrome`
        // all keep: an endpoint every request to which fails obscurely is worse than a refusal.
        let values = Values::parse(
            "providers = 2\n\
             provider.0.name = gemini\n\
             provider.0.wire = gemini\n\
             provider.0.url = https://example.com\n\
             provider.1.name = mine\n\
             provider.1.wire = openai\n\
             provider.1.url = http://127.0.0.1:8080/v1/chat/completions\n\
             provider.1.model = m\n",
        );
        let (configuration, refused) = Configuration::of(&values);
        assert_eq!(configuration.providers.len(), 1);
        assert_eq!(configuration.providers[0].name, "mine");
        assert!(unluminous_chat::provider::WIRES.contains(&configuration.providers[0].wire.name()));
        // **And it says so**, which is the other half of the rule: a row silently dropped is an
        // endpoint that is not there with nothing at all to explain it.
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(refused[0].contains("gemini"), "{refused:?}");
        assert!(refused[0].contains("anthropic"), "the refusal lists what there is: {refused:?}");
    }

    #[test]
    fn a_file_that_names_no_provider_gets_the_three_that_ship() {
        // A pane with no endpoint cannot do anything, and somebody who wanted none would have
        // switched the plugin off.
        let (configuration, refused) = Configuration::of(&Values::parse("stream = false\n"));
        assert_eq!(configuration.providers.len(), 3);
        assert!(!configuration.stream);
        assert!(refused.is_empty());
    }

    #[test]
    fn choosing_an_endpoint_that_is_not_there_names_the_ones_that_are() {
        let mut configuration = Configuration::default();
        let problem = configuration.choose("gemini").expect_err("a refusal");
        assert!(problem.contains("claude"), "{problem}");
        assert!(problem.contains("codex"), "{problem}");
        configuration.choose("codex").expect("chosen");
        assert_eq!(configuration.provider().expect("one").name, "codex");
    }

    #[test]
    fn sending_with_nothing_typed_and_with_a_broken_endpoint_both_refuse_before_a_request_goes_out()
    {
        let mut chat = opened("refusals");
        assert!(chat.send().expect_err("nothing to send").contains("nothing to send"));
        chat.draft = "hello".to_owned();
        // **A program that is not installed is refused before it is spawned**, which is
        // `task-1692`'s rule for a missing debug adapter kept for a missing agent — and it is the
        // refusal that replaced the missing-key one, because a command-line row has no key.
        chat.configuration.choose("claude").expect("chosen");
        chat.configuration.providers[0].command = "unluminous-no-such-agent-anywhere".to_owned();
        let problem = chat.send().expect_err("not installed");
        assert!(problem.contains("unluminous-no-such-agent-anywhere"), "{problem}");

        // And the missing-key refusal for a row that really does send to an address.
        chat.configuration.providers[0].wire = unluminous_chat::Wire::Anthropic;
        chat.configuration.providers[0].url = "https://api.anthropic.com/v1/messages".to_owned();
        chat.configuration.providers[0].model = "claude-opus-5".to_owned();
        chat.configuration.providers[0].key_env = "UNLUMINOUS_A_VARIABLE_NOTHING_SETS".to_owned();
        let problem = chat.send().expect_err("no key");
        assert!(problem.contains("UNLUMINOUS_A_VARIABLE_NOTHING_SETS"), "{problem}");
        // And the draft is still there, because a refusal must not eat what somebody typed.
        assert_eq!(chat.draft, "hello");
        assert_eq!(chat.session.chat.messages.len(), 0);
    }

    #[test]
    fn the_system_prompt_says_where_it_is_and_never_sends_the_file() {
        let mut chat = opened("system");
        chat.showing(Some(Path::new("/p")), Some(Path::new("/p/src/main.rs")));
        chat.configuration.system = "Be terse.".to_owned();
        let prompt = chat.system_prompt();
        assert!(prompt.contains("Unluminous"), "{prompt}");
        assert!(prompt.contains("/p/src/main.rs"), "{prompt}");
        assert!(prompt.contains("Be terse."), "{prompt}");
        // **Tools are on by default now** — `task-1848` — so the prompt mentions them, and it is turning
        // them *off* that has to take the sentence away.
        assert!(
            prompt.contains("tools you have been given"),
            "tools are offered by default: {prompt}"
        );
        chat.configuration.tools = false;
        assert!(
            !chat.system_prompt().contains("tools you have been given"),
            "and a person who turned them off is not told about them"
        );
    }

    #[test]
    fn a_tool_call_becomes_a_request_the_window_runs_and_its_answer_goes_back_up() {
        let mut chat = opened("tools");
        chat.configuration.tools = true;
        chat.session.ask(Message::said(0, Role::User, "what does git say?"));
        chat.session.reply(unluminous_chat::Reply::ToolCall {
            id: "t1".to_owned(),
            name: "unluminous_git".to_owned(),
            arguments: "{\"command\":\"status\"}".to_owned(),
        });
        chat.session.reply(unluminous_chat::Reply::Finished { reason: "tool_use".to_owned() });
        chat.ask_for_the_tools();
        let asked = chat.asking();
        assert_eq!(asked.len(), 1);
        let Request::RunCommand { id, command, .. } = &asked[0] else {
            panic!("{asked:?}");
        };
        // **The position rather than the server's id**, because two calls can carry one id and
        // answering by id then left the second running for ever. See `Outstanding`.
        assert_eq!(id, "0");
        assert_eq!(command, "git.status");

        // The answer goes back into the conversation as a tool result. Found by its role rather than
        // as the last message, because answering the last outstanding tool is what starts the next
        // round — and here that round is refused for want of a key, which is a message of its own.
        chat.answered("0", Ok(serde_json::json!({ "branch": "main" })));
        let results = chat
            .session
            .chat
            .messages
            .iter()
            .find(|message| message.role == Role::Tool)
            .expect("a result message");
        assert!(results.tools[0].answer.as_deref().expect("an answer").contains("main"));
    }

    #[test]
    fn a_tool_that_cannot_be_resolved_answers_at_once_rather_than_hanging_the_turn() {
        let mut chat = opened("bad-tool");
        chat.configuration.tools = true;
        chat.session.ask(Message::said(0, Role::User, "do something odd"));
        chat.session.reply(unluminous_chat::Reply::ToolCall {
            id: "t1".to_owned(),
            name: "unluminous_levitate".to_owned(),
            arguments: "{}".to_owned(),
        });
        chat.session.reply(unluminous_chat::Reply::Finished { reason: "tool_use".to_owned() });
        chat.ask_for_the_tools();
        assert!(chat.asking().is_empty(), "nothing is asked of the window");
        let results = chat
            .session
            .chat
            .messages
            .iter()
            .find(|message| message.role == Role::Tool)
            .expect("a result message");
        assert!(results.tools[0].failed);
        assert!(results.tools[0]
            .answer
            .as_deref()
            .expect("a reason")
            .contains("unluminous_levitate"));
    }

    #[test]
    fn a_tool_answer_is_cut_short_rather_than_billed_whole() {
        let long = serde_json::Value::String("x".repeat(20_000));
        let cut = shorten_for_a_model(&long);
        assert!(cut.len() < 9_000, "{}", cut.len());
        assert!(cut.ends_with("characters."), "the cut says so rather than being silent");
        // A short one is untouched, and null is `ok` rather than the word `null`.
        assert_eq!(shorten_for_a_model(&serde_json::json!("fine")), "fine");
        assert_eq!(shorten_for_a_model(&serde_json::Value::Null), "ok");
        // And a cut never lands in the middle of a character.
        let wide = serde_json::Value::String("é".repeat(20_000));
        assert!(!shorten_for_a_model(&wide).is_empty());
    }

    #[test]
    fn a_conversation_is_written_when_the_turn_ends_and_read_back_when_the_pane_is_opened_again() {
        let folder = a_folder("persist");
        let mut chat = AgentChat::new();
        chat.open(&Context { folder: Some(folder.clone()), ..Context::default() }).expect("opened");
        chat.session.ask(Message::said(0, Role::User, "Remember me"));
        chat.session.reply(unluminous_chat::Reply::Text("I will.".to_owned()));
        chat.session.reply(unluminous_chat::Reply::Finished { reason: "stop".to_owned() });
        chat.dirty = true;
        chat.write_the_conversation();

        let mut again = AgentChat::new();
        again.open(&Context { folder: Some(folder), ..Context::default() }).expect("opened");
        assert_eq!(again.session.chat.messages.len(), 2);
        assert_eq!(again.session.chat.messages[0].text(), "Remember me");
        assert_eq!(again.history().len(), 1);
    }

    /// An endpoint that sends to an address and needs no key, so `send` gets past its own checks.
    ///
    /// Pointed at a port nothing listens on, which nothing in these tests ever reaches: every one of
    /// them stops before a turn is dispatched.
    fn a_local_endpoint(chat: &mut AgentChat) {
        chat.configuration.choose("local").expect("the local row ships");
        let local = chat.configuration.providers.len() - 1;
        let chosen = chat.configuration.chosen.clone();
        let row = chat
            .configuration
            .providers
            .iter_mut()
            .find(|one| one.name == chosen)
            .expect("the chosen row");
        let _ = local;
        row.url = "http://127.0.0.1:1/v1/chat/completions".to_owned();
        row.model = "a-model".to_owned();
        row.key_env = String::new();
    }

    /// `task-2060`: *"I should be able to send new messages that get added to the queue when the
    /// agent is working. I should see my message immediately posted after I send it."*
    #[test]
    fn a_question_asked_while_an_answer_is_arriving_waits_its_turn_and_is_visible_at_once() {
        let mut chat = opened("queue");
        a_local_endpoint(&mut chat);
        // A turn in flight, driven through the session so no request leaves this process.
        chat.session.ask(Message::said(0, Role::User, "First"));
        assert!(chat.session.is_busy());

        chat.draft = "Second".to_owned();
        let id = chat.send().expect("queued rather than refused");
        assert_eq!(chat.queued().len(), 1, "it is waiting its turn");
        assert_eq!(chat.queued()[0].id, id, "and the id it was given is the id it keeps");
        assert_eq!(chat.queued()[0].text(), "Second");
        assert!(chat.draft.is_empty(), "the composer is empty, as it is for a message that went");
        assert_eq!(chat.session.chat.messages.len(), 1, "and it is not in the conversation yet");
        assert!(chat.ui.jump_to_bottom, "the conversation goes to the bottom to show it");

        // `state` says so, which is how an agent driving the pane tells waiting from never-sent.
        let answered = chat.command("state", &[]).expect("state answers");
        assert_eq!(answered.value["queued"], 1);

        // The turn ends, and the queued question is asked with the id it has been drawn under.
        chat.session.reply(unluminous_chat::Reply::Text("An answer.".to_owned()));
        chat.session.reply(unluminous_chat::Reply::Finished { reason: "stop".to_owned() });
        assert_eq!(what_the_queue_does_next(chat.queued.len(), chat.session.state()), Queue::Goes);
        chat.send_the_next_queued();
        assert!(chat.queued().is_empty());
        let last = chat.session.chat.last().expect("the question went");
        assert_eq!(last.text(), "Second");
        assert_eq!(last.id, id, "and it kept the id the pane drew it under");
        assert!(chat.session.is_busy(), "which is a turn of its own");
    }

    /// Stopping means stopping, and nothing somebody typed is thrown away. `task-2060`.
    #[test]
    fn stopping_puts_what_was_queued_back_in_the_composer_with_its_pictures() {
        let mut chat = opened("queue-stop");
        a_local_endpoint(&mut chat);
        chat.session.ask(Message::said(0, Role::User, "First"));
        chat.draft = "Second".to_owned();
        chat.attach_bytes("a.png".to_owned(), "image/png".to_owned(), vec![1, 2, 3]);
        chat.send().expect("queued");
        chat.draft = "Third".to_owned();
        chat.send().expect("queued");
        assert_eq!(chat.queued().len(), 2);
        assert!(
            chat.attachments.is_empty(),
            "the picture went with the message it was attached to"
        );

        chat.stop();
        assert!(chat.queued().is_empty(), "stop means stop, including what is waiting");
        assert_eq!(chat.draft, "Second\n\nThird", "and the words are back where they were typed");
        assert_eq!(chat.attachments.len(), 1, "and so is the picture");
        assert_eq!(chat.attachments[0].name, "a.png");
    }

    /// A queue belongs to the conversation it was typed into, and does not follow one being left.
    #[test]
    fn starting_a_new_conversation_empties_the_queue_rather_than_asking_it_of_the_new_one() {
        let mut chat = opened("queue-switch");
        a_local_endpoint(&mut chat);
        chat.session.ask(Message::said(0, Role::User, "First"));
        chat.draft = "Second".to_owned();
        chat.send().expect("queued");
        chat.new_conversation();
        assert!(chat.queued().is_empty());
        assert_eq!(chat.draft, "Second", "back in the composer rather than thrown away");
        assert!(chat.session.chat.messages.is_empty(), "and the new conversation is empty");
    }

    /// A turn that failed ends the queue rather than pulling the next question through.
    #[test]
    fn the_queue_goes_when_a_turn_ends_and_goes_back_when_one_fails() {
        use unluminous_chat::State;
        assert_eq!(what_the_queue_does_next(0, &State::Idle), Queue::Waits, "nothing waiting");
        assert_eq!(
            what_the_queue_does_next(2, &State::Streaming),
            Queue::Waits,
            "the turn in front of it has not finished"
        );
        assert_eq!(
            what_the_queue_does_next(2, &State::WaitingForTools),
            Queue::Waits,
            "a tool is still running, which is still this turn"
        );
        assert_eq!(
            what_the_queue_does_next(2, &State::Finished { reason: "stop".to_owned() }),
            Queue::Goes
        );
        assert_eq!(
            what_the_queue_does_next(2, &State::Failed("HTTP 500".to_owned())),
            Queue::GoesBack,
            "three questions asked of an endpoint that is not answering would be three failures"
        );
    }

    #[test]
    fn a_picture_is_sniffed_from_its_bytes_rather_than_trusted_from_its_name() {
        // A screenshot saved as `.png` that is really a JPEG is a thing that happens, and an API
        // told the wrong media type refuses the whole request.
        assert_eq!(
            media_type_of(Path::new("shot.png"), &[0xFF, 0xD8, 0xFF, 0xE0]).as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(media_type_of(Path::new("shot.png"), &[]).as_deref(), Some("image/png"));
        assert_eq!(media_type_of(Path::new("notes.txt"), &[1, 2, 3]), None);
    }

    #[test]
    fn attaching_and_taking_off_a_picture_leaves_the_draft_alone() {
        let mut chat = opened("attach");
        chat.draft = "look".to_owned();
        chat.attach_bytes("a.png".to_owned(), "image/png".to_owned(), vec![1, 2, 3]);
        chat.attach_bytes("b.png".to_owned(), "image/png".to_owned(), vec![4]);
        assert_eq!(chat.attachments.len(), 2);
        let first = chat.attachments[0].id;
        chat.remove_attachment(first);
        assert_eq!(chat.attachments.len(), 1);
        assert_eq!(chat.attachments[0].name, "b.png");
        assert_eq!(chat.draft, "look");
    }

    #[test]
    fn a_picture_off_the_clipboard_is_attached_and_an_empty_clipboard_says_so() {
        // The window owns the clipboard, so the pane **asks** for the picture and the answer comes
        // back through `UiProvider::answered` under the `clipboard` id — the same channel a tool
        // call's answer comes back on, which is why the id is a name rather than a number.
        let mut chat = opened("paste");
        let bytes = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3];
        let answer = serde_json::json!({
            "media": "image/png",
            "name": "pasted-8x8.png",
            "data": unluminous_chat::base64::encode(&bytes),
        });
        chat.answered(CLIPBOARD, Ok(answer));
        assert_eq!(chat.attachments.len(), 1);
        assert_eq!(chat.attachments[0].name, "pasted-8x8.png");
        assert_eq!(chat.attachments[0].bytes, bytes, "the bytes survived the base64");
        assert!(chat.problem.is_none());

        // And a paste with nothing on the clipboard is an ordinary thing to do by accident, so it
        // says so where every other refusal goes rather than doing nothing at all.
        chat.answered(CLIPBOARD, Err("there is no picture on the clipboard.".to_owned()));
        assert_eq!(chat.attachments.len(), 1, "the one already attached is left alone");
        assert_eq!(chat.problem.as_deref(), Some("there is no picture on the clipboard."));
    }

    #[test]
    fn the_calls_a_turn_may_make_are_bounded_as_well_as_the_rounds() {
        // Two bounds, because one does not imply the other: eight rounds of one call each and one
        // round of eight hundred calls are both a turn nobody is watching. The call bound is derived
        // from the round bound rather than being a second number in the settings, because two numbers
        // nobody can tell apart is worse than a ratio written down beside the code.
        let mut chat = opened("bound");
        assert_eq!(chat.call_limit(), chat.configuration.tool_limit * 4);
        chat.configuration_mut().tool_limit = 2;
        assert_eq!(chat.call_limit(), 8);
    }

    #[test]
    fn what_is_written_down_is_read_back_and_a_key_is_never_among_it() {
        // The Settings page writes through `Configuration::write`, so what it can set is what comes
        // back. **The key is the thing that must not be there**: an endpoint names the environment
        // variable it is in, and the value is read at the moment a request is sent and never held.
        let folder = a_folder("settings-round-trip");
        let mut configuration = Configuration {
            chosen: "codex".to_owned(),
            stream: false,
            tools: true,
            shell: true,
            tool_limit: 3,
            permission: unluminous_chat::Permission::Full,
            ..Configuration::default()
        };
        configuration.providers[0].wire = unluminous_chat::Wire::Anthropic;
        configuration.providers[0].url = "https://example.test/v1/messages".to_owned();
        configuration.providers[0].key_env = "ANTHROPIC_API_KEY".to_owned();
        configuration.providers[0].max_tokens = 2048;
        configuration.write(&folder).expect("the settings are written");

        let (read, refused) = Configuration::read(&folder);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(read.chosen, "codex");
        assert!(!read.stream);
        assert!(read.tools);
        assert!(read.shell, "the second switch is remembered separately from the first");
        assert_eq!(read.tool_limit, 3);
        assert_eq!(read.providers[0].url, "https://example.test/v1/messages");
        assert_eq!(read.providers[0].max_tokens, 2048);
        assert_eq!(read.providers[1].wire, unluminous_chat::Wire::CodexCli);
        assert_eq!(read.permission, unluminous_chat::Permission::Full);

        let written = std::fs::read_to_string(folder.join(Configuration::FILE)).expect("the file");
        assert!(!written.contains("sk-"), "a key reached the disk: {written}");
        assert!(
            written.contains("ANTHROPIC_API_KEY"),
            "what is written down is the name of the place the key is: {written}"
        );
    }

    #[test]
    fn every_command_it_lists_is_a_command_it_answers() {
        // The rule `every_registered_provider_can_be_built` keeps for the registry, kept for the
        // commands: one listed with no arm would be a command `plugins show` offers and `plugins run`
        // refuses.
        let mut chat = opened("commands");
        for (name, help) in chat.commands() {
            assert!(!help.is_empty(), "{name} says nothing");
            let answered = chat.command(name, &[]);
            // Some of them need an argument, and refusing for want of one is still answering.
            if let Err(problem) = answered {
                assert!(
                    !problem.contains("there is no `"),
                    "{name} is listed and not answered: {problem}"
                );
            }
        }
        assert!(chat.command("levitate", &[]).is_err());
    }

    #[test]
    fn the_view_answers_what_the_pane_is_showing_rather_than_a_screenshot() {
        let mut chat = opened("view");
        chat.session.ask(Message::said(0, Role::User, "hello"));
        let view = chat.view();
        assert_eq!(view["state"], "sending");
        assert_eq!(view["messages"], 1);
        assert!(view["total"].is_null(), "a chat's count is not drawn beside its pane's name");
        assert_eq!(view["conversation"]["messages"][0]["text"], "hello");
        // On by default since `task-1848`, and what the view reports is the setting rather than a constant.
        assert_eq!(view["tools"], true);
        assert!(view["provider"]["name"].is_string());
    }
}
