//! The AI endpoints: one list of what can answer, kept in `ai-endpoints.conf` in the settings folder
//! (`task-2247`).
//!
//! *"The goal is to have the configurations for keys, urls, cli, etc in one place, and then for Agent Chat,
//! Agent Tasks, etc to have dropdowns to pick from the list of configured endpoints."* An endpoint is a
//! [`Provider`] from `unluminous-chat`: a name, the shape it speaks, and either a program on this machine or
//! an address with a model and the environment variable its key is in. `Settings -> AI Configuration` edits
//! the list, and the window hands it to every plugin pane through `UiProvider::endpoints`, so a pane picks an
//! endpoint by name and never holds a second copy of what it means.
//!
//! **No key is written here**, which is the rule the Agent-Chat settings already kept: a row that sends to an
//! address names the environment variable its key is read from at the moment a request is sent.
//!
//! **The first time, the list is the one Agent-Chat had.** Before this file existed the endpoints were rows in
//! `plugins/agent-chat/settings.conf`, so a machine that had already configured some keeps them. Any endpoint
//! Unluminous ships whose name is not among them is added after them, which is how OpenCode reaches a list
//! written before it existed.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use unluminous_chat::acp_registry;

use unluminous_chat::provider::{
    join_arguments, join_variables, split_arguments, split_variables, Provider, Wire, WIRES,
};

use crate::services::plugin_ui;
use crate::services::store::Values;

/// The file, in the settings folder.
pub const FILE: &str = "ai-endpoints.conf";

/// The most endpoints a file is read for.
const MOST: usize = 32;

/// How long an answer about whether an endpoint can be reached is kept before it is asked again.
const READINESS: Duration = Duration::from_secs(5);

/// The window's endpoints: the list, what was wrong with the file it came from, and whether each one can
/// answer.
#[derive(Debug)]
pub struct Endpoints {
    pub providers: Vec<Provider>,
    /// A sentence for each row of the file that could not be read.
    pub problems: Vec<String>,
    pub readiness: Readiness,
    /// Moved every time the list changes, so a pane can be told once rather than every frame.
    pub revision: u64,
    /// The ACP registry, once somebody has asked for it (`task-2308`).
    pub acp: AcpRegistry,
}

impl Default for Endpoints {
    /// What a window with no settings folder has, which is a test's window: the endpoints Unluminous
    /// ships, and nothing read from or written to anybody's disk.
    fn default() -> Self {
        Self {
            providers: Provider::defaults(),
            problems: Vec::new(),
            readiness: Readiness::default(),
            revision: 1,
            acp: AcpRegistry::default(),
        }
    }
}

impl Endpoints {
    /// The endpoints in the settings folder `folder`. See [`load`].
    pub fn in_folder(folder: &Path) -> Self {
        Self::in_folder_with(folder, &|_| false)
    }

    /// The same, with `has_key` saying which API keys are set, for a list made for the first time. See
    /// [`load_with`].
    pub fn in_folder_with(folder: &Path, has_key: &dyn Fn(&str) -> bool) -> Self {
        let (providers, problems) = load_with(folder, has_key);
        Self { providers, problems, acp: AcpRegistry::in_folder(folder), ..Self::default() }
    }

    /// Say the list changed, so every pane is told.
    pub fn changed(&mut self) {
        self.revision += 1;
        self.readiness.forget();
    }
}

/// The endpoints in `folder`, and a sentence for each row that could not be read. See [`load_with`].
pub fn load(folder: &Path) -> (Vec<Provider>, Vec<String>) {
    load_with(folder, &|_| false)
}

/// The endpoints in `folder`, and a sentence for each row that could not be read.
///
/// `folder` is the settings folder. With no file there, the list is what Agent-Chat's own settings named,
/// plus an API endpoint for each key `has_key` says is set ([`api_endpoints`]), and it is written out so the
/// next start reads it from here. `has_key` is asked only then, so a window whose list already exists never
/// waits for the login shell to answer it.
pub fn load_with(folder: &Path, has_key: &dyn Fn(&str) -> bool) -> (Vec<Provider>, Vec<String>) {
    if let Ok(text) = std::fs::read_to_string(folder.join(FILE)) {
        let (rows, refused) = read_rows(&Values::parse(&text));
        return (rows_or_defaults(rows), refused);
    }
    let chat = folder.join("plugins").join("agent-chat").join("settings.conf");
    let (mut rows, refused) = match std::fs::read_to_string(chat) {
        Ok(text) => read_rows(&Values::parse(&text)),
        Err(_) => (Vec::new(), Vec::new()),
    };
    for shipped in Provider::defaults() {
        if !rows.iter().any(|row| row.name == shipped.name) {
            rows.push(shipped);
        }
    }
    for api in api_endpoints() {
        if has_key(&api.key_env) && !rows.iter().any(|row| row.name == api.name) {
            rows.push(api);
        }
    }
    let _ = write(folder, &rows);
    (rows, refused)
}

/// The two hosted APIs a fresh list offers when their key is set (`task-2278`): Anthropic's Messages API
/// and OpenAI's Responses API. Each names the variable its key is read from and never the key, which is the
/// rule this file keeps. Added once, when the list is first made: a list that already exists is somebody's,
/// and a row they took out stays out. `Add endpoint` makes either one later.
pub fn api_endpoints() -> Vec<Provider> {
    let api = |name: &str, wire: Wire, url: &str, model: &str, key_env: &str| Provider {
        name: name.to_owned(),
        wire,
        command: String::new(),
        url: url.to_owned(),
        model: model.to_owned(),
        key_env: key_env.to_owned(),
        key_entry: String::new(),
        max_tokens: unluminous_chat::provider::DEFAULT_MAX_TOKENS,
        args: Vec::new(),
        env: Vec::new(),
    };
    vec![
        api(
            "anthropic",
            Wire::Anthropic,
            "https://api.anthropic.com/v1/messages",
            "claude-sonnet-5-5",
            "ANTHROPIC_API_KEY",
        ),
        api(
            "openai",
            Wire::Responses,
            "https://api.openai.com/v1/responses",
            "gpt-5",
            "OPENAI_API_KEY",
        ),
    ]
}

/// Every row a file holds, or the endpoints Unluminous ships when it holds none.
fn rows_or_defaults(rows: Vec<Provider>) -> Vec<Provider> {
    match rows.is_empty() {
        true => Provider::defaults(),
        false => rows,
    }
}

/// Write the endpoints to `folder`.
pub fn write(folder: &Path, providers: &[Provider]) -> Result<(), String> {
    let mut values = Values::new();
    write_rows(&mut values, providers);
    plugin_ui::write_values(
        folder,
        FILE,
        &values,
        "# What can answer, for Agent Chat, Agent Tasks and anything else in Unluminous that talks to a model.\n\
         # `Settings -> AI Configuration` writes this file and reads it back. `wire` is `claude-cli`, `codex-cli`,\n\
         # `opencode-cli` or `pi-cli` for a row that runs the agent installed on this machine, which needs no key,\n\
         # `acp` for an agent spoken to through the Agent Client Protocol, started as `command` with `args`, or\n\
         # `openai`, `anthropic` or `responses` for one that sends to a URL. A row that sends names the\n\
         # environment variable its key comes from; the key itself is never written here.",
    )
}

/// The `provider.<n>.*` rows in `values`, and a sentence for each one that names a shape this version
/// does not speak.
pub fn read_rows(values: &Values) -> (Vec<Provider>, Vec<String>) {
    plugin_ui::read_numbered_rows(values, "provider", MOST, provider_at)
}

/// Write `providers` as `provider.<n>.*` rows.
pub fn write_rows(values: &mut Values, providers: &[Provider]) {
    values.set("providers", providers.len().to_string());
    for (index, provider) in providers.iter().enumerate() {
        values.set(&format!("provider.{index}.name"), provider.name.clone());
        values.set(&format!("provider.{index}.wire"), provider.wire.name());
        values.set(&format!("provider.{index}.command"), provider.command.clone());
        values.set(&format!("provider.{index}.url"), provider.url.clone());
        values.set(&format!("provider.{index}.model"), provider.model.clone());
        values.set(&format!("provider.{index}.key-env"), provider.key_env.clone());
        values.set(&format!("provider.{index}.key-entry"), provider.key_entry.clone());
        values.set(&format!("provider.{index}.max-tokens"), provider.max_tokens.to_string());
        // Only for a row that has them, so a file of the shapes that never do reads as it always did.
        if !provider.args.is_empty() {
            values.set(&format!("provider.{index}.args"), join_arguments(&provider.args));
        }
        if !provider.env.is_empty() {
            values.set(&format!("provider.{index}.env"), join_variables(&provider.env));
        }
    }
}

/// One row, or `None` for a row with no name.
fn provider_at(values: &Values, index: usize) -> Result<Option<Provider>, String> {
    let text = |field: &str| {
        values.text(&format!("provider.{index}.{field}")).unwrap_or_default().trim().to_owned()
    };
    let name = text("name");
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
        command: text("command"),
        url: text("url"),
        model: text("model"),
        key_env: text("key-env"),
        key_entry: text("key-entry"),
        max_tokens: values
            .number(&format!("provider.{index}.max-tokens"))
            .unwrap_or(unluminous_chat::provider::DEFAULT_MAX_TOKENS as f32)
            .clamp(64.0, 200_000.0) as u32,
        args: split_arguments(&text("args")),
        env: split_variables(&text("env")),
    }))
}

/// The endpoints that run an agent on this machine, which are the ones a ticket can be given to.
///
/// **Not an `acp` row** (`task-2308`): a ticket's agent runs in a terminal with its own interface, and an ACP
/// agent has none.
pub fn agents(providers: &[Provider]) -> impl Iterator<Item = &Provider> {
    providers.iter().filter(|provider| provider.is_a_program() && provider.wire != Wire::Acp)
}

/// What one key of an endpoint is called in `settings list`, `get` and `set`: `ai.provider.<n>.<field>`.
///
/// **The agent's half of the AI Configuration page** (`task-2247`). The same names the file uses, under
/// `ai.`, the way a plugin's own file is reached as `plugins.<id>.<key>`, so nothing about the page needs a
/// command of its own: `settings set ai.provider.3.model llama-local/local` is the field on the page.
const FIELDS: [&str; 9] =
    ["name", "wire", "command", "args", "env", "url", "model", "key-env", "max-tokens"];

/// Every endpoint key and its value, for `settings list` and `settings get`.
pub fn settings(providers: &[Provider]) -> Vec<(String, String)> {
    let mut values = Values::new();
    write_rows(&mut values, providers);
    let mut out = vec![("ai.providers".to_owned(), providers.len().to_string())];
    for index in 0..providers.len() {
        for field in FIELDS {
            let key = format!("provider.{index}.{field}");
            out.push((format!("ai.{key}"), values.text(&key).unwrap_or_default().to_owned()));
        }
    }
    out
}

/// Change one endpoint key, or `None` when `name` is not one. `ai.providers` grows the list with copies of
/// the address row Unluminous ships, each with a name no other has, or shrinks it, never below one.
pub fn set(providers: &mut Vec<Provider>, name: &str, value: &str) -> Option<Result<(), String>> {
    let rest = name.strip_prefix("ai.")?;
    let value = value.trim();
    if rest == "providers" {
        return Some(match value.parse::<usize>() {
            Ok(count) if (1..=MOST).contains(&count) => {
                providers.truncate(count);
                while providers.len() < count {
                    let mut new = Provider::defaults()[2].clone();
                    new.name = unused_name(providers);
                    providers.push(new);
                }
                Ok(())
            }
            _ => Err(format!("ai.providers is how many endpoints there are, from 1 to {MOST}")),
        });
    }
    let (index, field) = rest.strip_prefix("provider.")?.split_once('.')?;
    let index: usize = index.parse().ok()?;
    if !FIELDS.contains(&field) {
        return None;
    }
    let Some(provider) = providers.get_mut(index) else {
        return Some(Err(format!(
            "there is no endpoint {index}: ai.providers says how many there are"
        )));
    };
    if field == "name" && value.is_empty() {
        return Some(Err("an endpoint's name cannot be empty".to_owned()));
    }
    match field {
        "name" => provider.name = value.to_owned(),
        "wire" => match Wire::from_name(value) {
            Some(wire) => provider.wire = wire,
            None => {
                return Some(Err(format!(
                    "`{value}` is not a shape this version speaks: {}",
                    WIRES.join(", ")
                )))
            }
        },
        "command" => provider.command = value.to_owned(),
        "args" => provider.args = split_arguments(value),
        "env" => provider.env = split_variables(value),
        "url" => provider.url = value.to_owned(),
        "model" => provider.model = value.to_owned(),
        "key-env" => provider.key_env = value.to_owned(),
        _ => match value.parse::<u32>() {
            Ok(count) => provider.max_tokens = count.clamp(64, 200_000),
            Err(_) => return Some(Err("max-tokens is a number of tokens".to_owned())),
        },
    }
    Some(Ok(()))
}

/// A name no endpoint has yet, so `Add endpoint` twice makes two rows rather than two of one name.
pub fn unused_name(providers: &[Provider]) -> String {
    let mut count = providers.len() + 1;
    loop {
        let name = format!("endpoint-{count}");
        if !providers.iter().any(|one| one.name == name) {
            return name;
        }
        count += 1;
    }
}

/// Whether each endpoint can be reached, and which shapes this machine can run, asked at most every
/// [`READINESS`].
///
/// For a program row the answer is a walk of `PATH`, so asking once a row once a frame was a directory
/// listing inside the draw. The window keeps one of these for the AI Configuration page.
#[derive(Debug, Default)]
pub struct Readiness {
    rows: Vec<Option<String>>,
    rows_taken: Option<Instant>,
    shapes: Vec<&'static str>,
    shapes_taken: Option<Instant>,
}

impl Readiness {
    /// Why each of `providers` cannot answer, or `None` for one that can.
    pub fn rows(&mut self, providers: &[Provider]) -> Vec<Option<String>> {
        let stale = self.rows_taken.is_none_or(|at| at.elapsed() >= READINESS);
        if stale || self.rows.len() != providers.len() {
            let environment = crate::services::agent_chat::AgentChat::the_environment();
            self.rows = providers.iter().map(|provider| provider.why_not(&environment)).collect();
            self.rows_taken = Some(Instant::now());
        }
        self.rows.clone()
    }

    /// Which wire shapes a row could be switched to on this machine: every address shape, and a program
    /// shape only when the program is installed.
    pub fn shapes(&mut self) -> Vec<&'static str> {
        let stale = self.shapes_taken.is_none_or(|at| at.elapsed() >= READINESS);
        if stale {
            let environment = crate::services::agent_chat::AgentChat::the_environment();
            self.shapes = WIRES
                .iter()
                .copied()
                .filter(|named| {
                    Wire::from_name(named).is_some_and(|wire| wire.is_available(&environment))
                })
                .collect();
            self.shapes_taken = Some(Instant::now());
        }
        self.shapes.clone()
    }

    /// Ask again on the next frame, because a row was just edited.
    pub fn forget(&mut self) {
        self.rows_taken = None;
    }
}

/// The ACP registry as the window holds it: the agents once they have been read, and an agent being installed
/// (`task-2308`).
///
/// **Nothing is fetched until somebody asks**, by opening `Settings -> AI Configuration` or running
/// `settings acp-registry`.
/// The fetch and a binary agent's download each run on a thread of their own, and the window asks [`Self::poll`]
/// once a frame, which costs two channel reads while nothing is happening.
/// How long [`AcpRegistry::runners`] keeps its answer. An install in a terminal shows on the page this soon after.
const RUNNERS_FRESH: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct AcpRegistry {
    /// The agents, once read.
    pub agents: Option<Vec<acp_registry::Agent>>,
    /// Why the last fetch or the last install did not work.
    pub problem: Option<String>,
    /// What the last install or add did, for the page and for `settings acp-add`.
    pub said: Option<String>,
    /// What the page's `Find` field holds.
    pub filter: String,
    /// The settings folder a binary agent is installed under. `None` in a test's window.
    folder: Option<std::path::PathBuf>,
    loading: Option<Receiver<Result<Vec<acp_registry::Agent>, String>>>,
    /// The registry id being installed, and where its endpoint will arrive.
    installing: Option<(String, Receiver<Result<Installed, String>>)>,
    /// The npm package each registry pin is run as on this machine, when npm here cannot fetch the registry's
    /// own (`task-2313`). Kept in `acp-agents/npm-pins.conf` in the settings folder. See [`NPM_PINS`].
    npm_pins: Vec<(String, String)>,
    /// Whether adding an npx agent asks npm which version it can fetch. Only the released binary does, through
    /// [`Self::read_when_shown`], so a test never runs npm.
    asks_npm: bool,
    /// Endpoints that are ready to be added to the list, taken by [`Self::take_ready`].
    ready: Vec<Provider>,
    /// Whether showing the page reads the registry. See [`Self::read_when_shown`].
    read_when_shown: bool,
    /// Which package runners are installed, and when that was asked. See [`Self::runners`].
    runners: Option<(std::time::Instant, Runners)>,
}

/// An agent a thread has finished installing: its endpoint, and the npm pin it was resolved to when that is not
/// the registry's own (`registry pin`, `pin used`).
#[derive(Debug)]
struct Installed {
    provider: Provider,
    npm_pin: Option<(String, String)>,
}

/// The file in the settings folder that says which npm package each registry pin is run as here.
const NPM_PINS: &str = "acp-agents/npm-pins.conf";

/// Which of the package runners an ACP agent can be started through are on this machine.
#[derive(Debug, Clone, Copy, Default)]
pub struct Runners {
    pub npx: bool,
    pub uvx: bool,
}

impl Runners {
    /// Whether `program` is one of them and is installed. Anything else is answered yes, which is what
    /// [`acp_registry::launch_for`] needs: it asks only about `npx` and `uvx`.
    pub fn has(&self, program: &str) -> bool {
        match program {
            "npx" => self.npx,
            "uvx" => self.uvx,
            _ => true,
        }
    }
}

impl AcpRegistry {
    /// The registry for a window whose settings are in `folder`.
    pub fn in_folder(folder: &Path) -> Self {
        let npm_pins = read_npm_pins(&folder.join(NPM_PINS));
        // A turn for an endpoint still pinned to a version npm here lacks runs the package unpinned straight
        // away, rather than failing once first.
        for (pinned, _) in &npm_pins {
            unluminous_chat::acp::remember_npm_lacks(pinned);
        }
        Self { folder: Some(folder.to_path_buf()), npm_pins, ..Self::default() }
    }

    /// Record that the registry's `pinned` package is run here as `used`, and write the list down.
    fn note_npm_pin(&mut self, pinned: &str, used: &str) {
        match self.npm_pins.iter_mut().find(|(known, _)| known == pinned) {
            Some(entry) if entry.1 == used => return,
            Some(entry) => entry.1 = used.to_owned(),
            None => self.npm_pins.push((pinned.to_owned(), used.to_owned())),
        }
        unluminous_chat::acp::remember_npm_lacks(pinned);
        if let Some(folder) = &self.folder {
            let path = folder.join(NPM_PINS);
            let mut text = String::from(
                "# The npm package each ACP registry pin is run as, because npm on this machine cannot fetch it.\n",
            );
            for (pinned, used) in &self.npm_pins {
                text.push_str(&format!("{pinned} = {used}\n"));
            }
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, text);
        }
    }

    /// The npm package the registry's `package` is run as here.
    fn npm_pin_for(&self, package: &str) -> String {
        self.npm_pins
            .iter()
            .find(|(pinned, _)| pinned == package)
            .map(|(_, used)| used.clone())
            .unwrap_or_else(|| package.to_owned())
    }

    /// `distribution` with its npx package replaced by the one it is run as here.
    fn as_run_here(&self, distribution: &acp_registry::Distribution) -> acp_registry::Distribution {
        let mut distribution = distribution.clone();
        if let acp_registry::Distribution::Npx { package, .. } = &mut distribution {
            *package = self.npm_pin_for(package);
        }
        distribution
    }

    /// Let the AI Configuration page read the registry when it is shown (`task-2308`). Only the released binary
    /// calls this, so a test's window fetches nothing unless `UNLUMINOUS_ACP_REGISTRY` names a file to read.
    pub fn read_when_shown(&mut self) {
        self.read_when_shown = true;
        self.asks_npm = true;
    }

    /// Which package runners are installed, asked again at most every [`RUNNERS_FRESH`] (`task-2308`). Each
    /// card on the page asks, and looking a program up walks `PATH`: asked afresh, forty agents did that eighty
    /// times a frame.
    pub fn runners(&mut self) -> Runners {
        if let Some((asked, runners)) = self.runners {
            if asked.elapsed() < RUNNERS_FRESH {
                return runners;
            }
        }
        let environment = crate::services::agent_chat::AgentChat::the_environment();
        let has =
            |program: &str| unluminous_chat::provider::program(program, &environment).is_some();
        let runners = Runners { npx: has("npx"), uvx: has("uvx") };
        self.runners = Some((std::time::Instant::now(), runners));
        runners
    }

    /// Read the registry because the page that lists it is showing, when this window may.
    pub fn shown(&mut self) {
        if self.read_when_shown || std::env::var_os("UNLUMINOUS_ACP_REGISTRY").is_some() {
            self.load(false);
        }
    }

    /// Read the registry on a thread, unless it is already being read. `again` reads it even when it has been.
    pub fn load(&mut self, again: bool) {
        if self.loading.is_some() || (self.agents.is_some() && !again) {
            return;
        }
        let (send, receive) = channel();
        let started = std::thread::Builder::new().name("unluminous-acp-registry".to_owned()).spawn(
            move || {
                let _ =
                    send.send(acp_registry::fetch().and_then(|text| acp_registry::parse(&text)));
            },
        );
        match started {
            Ok(_) => {
                self.loading = Some(receive);
                self.problem = None;
            }
            Err(problem) => {
                self.problem = Some(format!("The registry could not be read: {problem}"))
            }
        }
    }

    /// The agents, as a test or a window with no network would be handed them.
    pub fn use_agents(&mut self, agents: Vec<acp_registry::Agent>) {
        self.agents = Some(agents);
        self.loading = None;
    }

    /// Whether the registry is being read or an agent installed, so the window keeps drawing until it is done.
    pub fn is_busy(&self) -> bool {
        self.loading.is_some() || self.installing.is_some()
    }

    /// Whether the registry is being read.
    pub fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    /// The id of the agent being installed, if one is.
    pub fn installing(&self) -> Option<&str> {
        self.installing.as_ref().map(|(id, _)| id.as_str())
    }

    /// Take what the threads have finished. Answers whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        // A turn that found npm lacked a pinned version, for an endpoint added before installs asked npm. One
        // atomic load while nothing has been found. The package is written down as run unpinned, so the next
        // start knows it too and the agent's card offers `Update`, which pins the version npm here has.
        if let Some(lacks) = unluminous_chat::acp::npm_lacks_if_changed() {
            for pinned in lacks {
                if !self.npm_pins.iter().any(|(known, _)| *known == pinned) {
                    let unpinned = pinned[..pinned.rfind('@').unwrap_or(pinned.len())].to_owned();
                    self.note_npm_pin(&pinned, &unpinned);
                    changed = true;
                }
            }
        }
        if let Some(loading) = &self.loading {
            let arrived = match loading.try_recv() {
                Ok(arrived) => Some(arrived),
                Err(TryRecvError::Disconnected) => {
                    Some(Err("The registry stopped being read.".to_owned()))
                }
                Err(TryRecvError::Empty) => None,
            };
            if let Some(arrived) = arrived {
                match arrived {
                    Ok(agents) => self.agents = Some(agents),
                    Err(problem) => self.problem = Some(problem),
                }
                self.loading = None;
                changed = true;
            }
        }
        if let Some((id, installing)) = &self.installing {
            let id = id.clone();
            let arrived = match installing.try_recv() {
                Ok(arrived) => Some(arrived),
                Err(TryRecvError::Disconnected) => {
                    Some(Err(format!("Installing {id} stopped before it finished.")))
                }
                Err(TryRecvError::Empty) => None,
            };
            if let Some(arrived) = arrived {
                match arrived {
                    Ok(Installed { provider, npm_pin }) => {
                        if let Some((pinned, used)) = npm_pin {
                            self.note_npm_pin(&pinned, &used);
                        }
                        self.said =
                            Some(format!("Installed {id} and added it as `{}`.", provider.name));
                        self.ready.push(provider);
                    }
                    Err(problem) => self.problem = Some(problem),
                }
                self.installing = None;
                changed = true;
            }
        }
        changed
    }

    /// The endpoints that are ready to be added.
    pub fn take_ready(&mut self) -> Vec<Provider> {
        std::mem::take(&mut self.ready)
    }

    /// Add the agent `id` as an endpoint named `name`, or by its id. `has` says which programs are installed.
    ///
    /// An agent a package runner starts is ready at once and is answered. A binary agent is downloaded on a
    /// thread, and its endpoint arrives through [`Self::poll`] and [`Self::take_ready`].
    pub fn add(
        &mut self,
        id: &str,
        name: Option<&str>,
        providers: &[Provider],
        has: &dyn Fn(&str) -> bool,
    ) -> Result<Option<Provider>, String> {
        let Some(agents) = &self.agents else {
            return Err("The ACP registry has not been read yet.".to_owned());
        };
        let Some(agent) = agents.iter().find(|agent| agent.id == id.trim()) else {
            return Err(format!("The ACP registry has no agent `{id}`."));
        };
        let name = name
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(agent.id.as_str())
            .to_owned();
        if providers.iter().any(|provider| provider.name == name) {
            return Err(format!("There is already an endpoint called `{name}`."));
        }
        let platform = acp_registry::platform();
        let Some(distribution) = acp_registry::launch_for(agent, &platform, has) else {
            return Err(format!("{} has no way to run on {platform}.", agent.name));
        };
        let asks_npm =
            self.asks_npm && matches!(distribution, acp_registry::Distribution::Npx { .. });
        if !asks_npm && !matches!(distribution, acp_registry::Distribution::Binary { .. }) {
            self.said = Some(format!("Added {} as `{name}`.", agent.id));
            self.problem = None;
            let distribution = self.as_run_here(distribution);
            return Ok(Some(acp_registry::provider_for(&name, &distribution, None)));
        }
        let (agent, distribution) = (agent.clone(), distribution.clone());
        self.install_on_a_thread(&agent, &distribution, name)?;
        Ok(None)
    }

    /// Download and unpack a binary agent on a thread; its endpoint, called `name`, arrives through
    /// [`Self::poll`] and [`Self::take_ready`], which replaces an endpoint of that name or adds one.
    fn install_on_a_thread(
        &mut self,
        agent: &acp_registry::Agent,
        distribution: &acp_registry::Distribution,
        name: String,
    ) -> Result<(), String> {
        if self.installing.is_some() {
            return Err(
                "Another agent is being installed. Try this one when it has finished.".to_owned()
            );
        }
        let Some(settings) = &self.folder else {
            return Err("This window has no settings folder to install an agent into.".to_owned());
        };
        let into = acp_registry::install_folder(settings, &agent.id, &agent.version);
        let distribution = distribution.clone();
        let known = self.npm_pins.clone();
        let (send, receive) = channel();
        std::thread::Builder::new()
            .name("unluminous-acp-install".to_owned())
            .spawn(move || {
                let installed = match &distribution {
                    // **An npx agent is pinned to a version npm here can fetch** (`task-2313`), asked once
                    // here rather than discovered by every turn.
                    acp_registry::Distribution::Npx { package, .. } => {
                        Ok(npx_installed(&name, &distribution, package, &known))
                    }
                    _ => acp_registry::install(&distribution, &into).map(|program| Installed {
                        provider: acp_registry::provider_for(&name, &distribution, Some(&program)),
                        npm_pin: None,
                    }),
                };
                let _ = send.send(installed);
            })
            .map_err(|problem| format!("The agent could not be installed: {problem}"))?;
        self.installing = Some((agent.id.clone(), receive));
        self.problem = None;
        self.said = Some(format!("Installing {}.", agent.id));
        Ok(())
    }

    /// The registry's agent `id`, once the registry has been read.
    fn agent(&self, id: &str) -> Option<&acp_registry::Agent> {
        self.agents.as_ref().and_then(|agents| agents.iter().find(|one| one.id == id))
    }

    /// The endpoint the registry's current version of `agent` would make, called `name`, for comparing with
    /// the one that is there. `None` when it has no way to run here.
    fn current_endpoint(
        &self,
        agent: &acp_registry::Agent,
        name: &str,
        has: &dyn Fn(&str) -> bool,
    ) -> Option<Provider> {
        let platform = acp_registry::platform();
        let distribution = acp_registry::launch_for(agent, &platform, has)?;
        let program = match (distribution, &self.folder) {
            (acp_registry::Distribution::Binary { cmd, .. }, Some(settings)) => {
                Some(acp_registry::program_in(
                    &acp_registry::install_folder(settings, &agent.id, &agent.version),
                    cmd,
                ))
            }
            _ => None,
        };
        let distribution = self.as_run_here(distribution);
        Some(acp_registry::provider_for(name, &distribution, program.as_deref()))
    }

    /// Whether the endpoint `id` added runs another version than the registry has now (`task-2308`). Compared by
    /// what it runs, so a binary in an older version's folder and an npx package pinned to an older version are
    /// both found, and an agent that was never added is not out of date.
    pub fn can_update(&self, id: &str, providers: &[Provider], has: &dyn Fn(&str) -> bool) -> bool {
        let Some(agent) = self.agent(id) else {
            return false;
        };
        let Some(there) = providers.iter().find(|one| one.wire == Wire::Acp && one.name == id)
        else {
            return false;
        };
        match self.current_endpoint(agent, id, has) {
            Some(current) => current.command != there.command || current.args != there.args,
            None => false,
        }
    }

    /// Bring the endpoint `id` added up to the registry's version. A package runner's endpoint is changed at once
    /// and answers true; a binary is downloaded first, answers false, and arrives through [`Self::take_ready`].
    /// The endpoint's model is kept.
    pub fn update(
        &mut self,
        id: &str,
        providers: &mut [Provider],
        has: &dyn Fn(&str) -> bool,
    ) -> Result<bool, String> {
        let Some(agent) = self.agent(id).cloned() else {
            return Err(format!("The ACP registry has no agent `{id}`."));
        };
        let Some(index) = providers.iter().position(|one| one.wire == Wire::Acp && one.name == id)
        else {
            return Err(format!(
                "There is no endpoint `{id}` to update. Add it with `settings acp-add {id}`."
            ));
        };
        let platform = acp_registry::platform();
        let Some(distribution) = acp_registry::launch_for(&agent, &platform, has).cloned() else {
            return Err(format!("{} has no way to run on {platform}.", agent.name));
        };
        let asks_npm =
            self.asks_npm && matches!(distribution, acp_registry::Distribution::Npx { .. });
        if asks_npm || matches!(distribution, acp_registry::Distribution::Binary { .. }) {
            self.install_on_a_thread(&agent, &distribution, id.to_owned())?;
            return Ok(false);
        }
        let model = providers[index].model.clone();
        let distribution = self.as_run_here(&distribution);
        providers[index] = acp_registry::provider_for(id, &distribution, None);
        providers[index].model = model;
        self.said = Some(format!("Updated {id} to {}.", agent.version));
        self.problem = None;
        Ok(true)
    }

    /// Take away the endpoint `id` added, and the binary agent downloaded for it (`task-2308`). The last endpoint
    /// is never taken away, because a list with no row has nowhere to type a new one.
    pub fn remove(&mut self, id: &str, providers: &mut Vec<Provider>) -> Result<(), String> {
        let gone = providers.iter().filter(|one| one.wire == Wire::Acp && one.name == id).count();
        if gone == 0 {
            return Err(format!("There is no endpoint `{id}` to remove."));
        }
        if gone == providers.len() {
            return Err("The last endpoint cannot be removed. Add another first.".to_owned());
        }
        providers.retain(|one| !(one.wire == Wire::Acp && one.name == id));
        if let Some(settings) = &self.folder {
            if let Some(folder) = acp_registry::install_folder(settings, id, "version").parent() {
                if folder.is_dir() {
                    std::fs::remove_dir_all(folder).map_err(|problem| {
                        format!(
                            "Removed the endpoint `{id}`, but not its download at {}: {problem}",
                            folder.display()
                        )
                    })?;
                }
            }
        }
        self.said = Some(format!("Removed {id}."));
        self.problem = None;
        Ok(())
    }
}

/// The endpoint for an npx agent called `name`, pinned to the version of `package` npm on this machine can
/// fetch. `known` is what an earlier install found, used when npm cannot be asked now.
fn npx_installed(
    name: &str,
    distribution: &acp_registry::Distribution,
    package: &str,
    known: &[(String, String)],
) -> Installed {
    let environment = crate::services::agent_chat::AgentChat::the_environment();
    let used = acp_registry::npm_version_to_pin(package, &environment)
        .or_else(|| {
            known.iter().find(|(pinned, _)| pinned == package).map(|(_, used)| used.clone())
        })
        .unwrap_or_else(|| package.to_owned());
    let npm_pin = (used != package).then(|| (package.to_owned(), used.clone()));
    let mut run_here = distribution.clone();
    if let acp_registry::Distribution::Npx { package, .. } = &mut run_here {
        *package = used;
    }
    Installed { provider: acp_registry::provider_for(name, &run_here, None), npm_pin }
}

/// The registry pins and the packages they are run as, from `acp-agents/npm-pins.conf`. A file that is not there
/// is an empty list.
fn read_npm_pins(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once(" = "))
        .map(|(pinned, used)| (pinned.trim().to_owned(), used.trim().to_owned()))
        .filter(|(pinned, used)| !pinned.is_empty() && !used.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A registry pin npm here cannot fetch is run as the package written in `acp-agents/npm-pins.conf`, by an
    /// agent added from the registry and by `Update`, and the agent is not offered `Update` for it for ever
    /// (`task-2313`). A turn for an endpoint pinned to it is told to run the package unpinned at once.
    #[test]
    fn a_pin_npm_lacks_is_run_as_the_version_written_down_for_it() {
        let folder = a_folder("npm-pins");
        let pinned = "@agentclientprotocol/claude-agent-acp@0.94.0";
        let used = "@agentclientprotocol/claude-agent-acp@0.91.2";
        std::fs::create_dir_all(folder.join("acp-agents")).expect("made");
        std::fs::write(
            folder.join(NPM_PINS),
            format!(
                "# written by Unluminous
{pinned} = {used}
"
            ),
        )
        .expect("written");
        let mut acp = AcpRegistry::in_folder(&folder);
        assert!(unluminous_chat::acp::npm_lacks(pinned), "a turn runs it unpinned straight away");
        let text = include_str!("../../../unluminous-chat/tests/streams/acp-registry.json");
        acp.use_agents(unluminous_chat::acp_registry::parse(text).expect("the registry reads"));
        let has = |_: &str| true;
        let added =
            acp.add("claude-acp", None, &[], &has).expect("added").expect("at once in a test");
        assert_eq!(added.args, vec!["-y".to_owned(), used.to_owned()]);
        let providers = vec![added];
        assert!(
            !acp.can_update("claude-acp", &providers, &has),
            "the version npm has is current here"
        );

        // And a pin a turn found missing is written down, so the next start knows it too.
        unluminous_chat::acp::remember_npm_lacks("some-agent@1.2.3");
        acp.poll();
        let written = std::fs::read_to_string(folder.join(NPM_PINS)).expect("written");
        assert!(written.contains("some-agent@1.2.3 = some-agent"), "{written}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    fn a_folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-ai-endpoints-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("made");
        folder
    }

    #[test]
    fn the_first_list_is_the_one_agent_chat_had_with_what_ships_added() {
        let folder = a_folder("migrate");
        let chat = folder.join("plugins").join("agent-chat");
        std::fs::create_dir_all(&chat).expect("made");
        let mut values = Values::new();
        let mut gateway = Provider::defaults()[2].clone();
        gateway.name = "gateway".to_owned();
        gateway.url = "https://gateway.example/v1/chat/completions".to_owned();
        write_rows(&mut values, &[Provider::defaults()[0].clone(), gateway.clone()]);
        std::fs::write(chat.join("settings.conf"), values.to_text_headed("")).expect("written");

        let (rows, refused) = load(&folder);
        assert!(refused.is_empty(), "{refused:?}");
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["claude", "gateway", "codex", "local", "opencode", "pi"]);
        assert_eq!(rows[1], gateway, "a row somebody wrote keeps every field");
        assert!(folder.join(FILE).is_file(), "written out so the next start reads it from here");

        // Read from the file the second time, and the same list comes back.
        let (again, _) = load(&folder);
        assert_eq!(again, rows);
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// `task-2278`. A key that is set adds its API to a list made for the first time, the row names the
    /// variable and never a key, and a list that already exists is not changed by a key set later.
    #[test]
    fn a_key_that_is_set_adds_its_api_to_a_new_list_and_never_to_an_existing_one() {
        let folder = a_folder("api-keys");
        let (rows, _) = load_with(&folder, &|name| name == "ANTHROPIC_API_KEY");
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["claude", "codex", "local", "opencode", "pi", "anthropic"]);
        let anthropic = rows.last().expect("a row");
        assert_eq!(anthropic.wire, Wire::Anthropic);
        assert_eq!(anthropic.key_env, "ANTHROPIC_API_KEY");
        assert!(anthropic.key_entry.is_empty());
        let written = std::fs::read_to_string(folder.join(FILE)).expect("written");
        assert!(written.contains("key-env = ANTHROPIC_API_KEY"), "{written}");

        let (again, _) = load_with(&folder, &|_| true);
        assert_eq!(again, rows, "a list that exists is somebody's and is read as it is");
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_machine_with_nothing_configured_gets_what_ships() {
        let folder = a_folder("fresh");
        let (rows, _) = load(&folder);
        assert_eq!(rows, Provider::defaults());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_row_in_a_shape_this_version_does_not_speak_is_refused_by_name() {
        let values =
            Values::parse("providers = 1\nprovider.0.name = odd\nprovider.0.wire = smoke\n");
        let (rows, refused) = read_rows(&values);
        assert!(rows.is_empty());
        assert!(refused[0].contains("`odd` speaks `smoke`"), "{refused:?}");
        assert!(refused[0].contains("opencode-cli"), "{refused:?}");
    }

    #[test]
    fn every_field_on_the_page_is_a_setting_an_agent_can_change() {
        let mut providers = Provider::defaults();
        let listed = settings(&providers);
        assert!(listed.contains(&("ai.providers".to_owned(), "5".to_owned())), "{listed:?}");
        assert!(listed.contains(&("ai.provider.3.wire".to_owned(), "opencode-cli".to_owned())));
        assert_eq!(set(&mut providers, "ai.provider.3.model", "llama-local/local"), Some(Ok(())));
        assert_eq!(providers[3].model, "llama-local/local");
        assert!(matches!(set(&mut providers, "ai.provider.3.wire", "smoke"), Some(Err(_))));
        assert_eq!(set(&mut providers, "ai.providers", "6"), Some(Ok(())));
        assert_eq!(providers.len(), 6);
        assert_ne!(providers[4].name, providers[5].name, "each added row has a name of its own");
        assert!(matches!(set(&mut providers, "ai.providers", "0"), Some(Err(_))));
        assert_eq!(set(&mut providers, "editor.indent", "tabs"), None, "not one of these");
    }

    /// An `acp` row keeps its program's arguments and its variables through the file and through `settings set`
    /// (`task-2308`), and a row of any other shape writes neither, so a file written before stays as it was.
    #[test]
    fn an_acp_rows_arguments_and_variables_survive_the_file_and_the_settings() {
        let folder = a_folder("acp");
        let mut row = Provider::defaults()[0].clone();
        row.name = "gemini".to_owned();
        row.wire = Wire::Acp;
        row.command = "npx".to_owned();
        row.args = vec![
            "-y".to_owned(),
            "@google/gemini-cli@0.64.0".to_owned(),
            "--acp".to_owned(),
            "a b".to_owned(),
        ];
        row.env = vec![("GEMINI_NO_UPDATE".to_owned(), "1".to_owned())];
        write(&folder, &[Provider::defaults()[0].clone(), row.clone()]).expect("written");
        let written = std::fs::read_to_string(folder.join(FILE)).expect("there");
        assert!(
            written.contains(r#"provider.1.args = -y @google/gemini-cli@0.64.0 --acp "a b""#),
            "{written}"
        );
        assert!(!written.contains("provider.0.args"), "a row with no arguments writes none");
        let (rows, refused) = load(&folder);
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(rows[1], row);

        let mut providers = rows;
        assert_eq!(set(&mut providers, "ai.provider.1.args", "-y other@1"), Some(Ok(())));
        assert_eq!(providers[1].args, ["-y", "other@1"]);
        assert_eq!(set(&mut providers, "ai.provider.1.env", "A=1; B = two"), Some(Ok(())));
        assert_eq!(
            providers[1].env,
            [("A".to_owned(), "1".to_owned()), ("B".to_owned(), "two".to_owned())]
        );
        let listed = settings(&providers);
        assert!(
            listed.contains(&("ai.provider.1.wire".to_owned(), "acp".to_owned())),
            "{listed:?}"
        );
        assert!(
            listed.contains(&("ai.provider.1.args".to_owned(), "-y other@1".to_owned())),
            "{listed:?}"
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn only_a_program_can_be_given_a_ticket() {
        let mut providers = Provider::defaults();
        let mut acp = providers[0].clone();
        acp.name = "claude-acp".to_owned();
        acp.wire = Wire::Acp;
        providers.push(acp);
        let names: Vec<String> = agents(&providers).map(|provider| provider.name.clone()).collect();
        assert_eq!(names, ["claude", "codex", "opencode", "pi"]);
    }
}
