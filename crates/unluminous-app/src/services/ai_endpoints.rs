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
use std::time::{Duration, Instant};

use unluminous_chat::provider::{Provider, Wire, WIRES};

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
        Self { providers, problems, ..Self::default() }
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
         # `Settings -> AI Configuration` writes this file and reads it back. `wire` is `claude-cli`, `codex-cli`\n\
         # or `opencode-cli` for a row that runs the agent installed on this machine, which needs no key, or\n\
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
    }))
}

/// The endpoints that run an agent on this machine, which are the ones a ticket can be given to.
pub fn agents(providers: &[Provider]) -> impl Iterator<Item = &Provider> {
    providers.iter().filter(|provider| provider.is_a_program())
}

/// What one key of an endpoint is called in `settings list`, `get` and `set`: `ai.provider.<n>.<field>`.
///
/// **The agent's half of the AI Configuration page** (`task-2247`). The same names the file uses, under
/// `ai.`, the way a plugin's own file is reached as `plugins.<id>.<key>`, so nothing about the page needs a
/// command of its own: `settings set ai.provider.3.model llama-local/local` is the field on the page.
const FIELDS: [&str; 7] = ["name", "wire", "command", "url", "model", "key-env", "max-tokens"];

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(names, ["claude", "gateway", "codex", "local", "opencode"]);
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
        assert_eq!(names, ["claude", "codex", "local", "opencode", "anthropic"]);
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
        assert!(listed.contains(&("ai.providers".to_owned(), "4".to_owned())), "{listed:?}");
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

    #[test]
    fn only_a_program_can_be_given_a_ticket() {
        let names: Vec<String> =
            agents(&Provider::defaults()).map(|provider| provider.name.clone()).collect();
        assert_eq!(names, ["claude", "codex", "opencode"]);
    }
}
