//! What the board is showing, and where its database and its agents come from.
//!
//! **Split out of `mod.rs` by `task-1984` §3.6**, which found that file at 2,967 lines holding four
//! different things: the provider, this, the pseudoterminal driver and the JSON view. It is the split
//! `task-1922` gave `plugins.rs`, and nothing about what any of it does changed in the move.
//!
//! [`Configuration`] is what `Settings -> Agent-Tasks` writes and what the board reads: the database
//! file, which agents may be started and with what, and the lease a working agent renews.

use super::*;

/// What the board is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Board,
    Backlog,
    Completed,
    Epics,
}

impl View {
    /// **Four, not five.** `task-28`: there was a `Schedule` view listing the rows in the `task_schedule`
    /// table with when each one next runs, and nothing on this board ever writes such a row — the browser
    /// board's scheduler is a server that runs while nobody is looking, which
    /// `tasks/agent-tasks-plugin-tdd.md` lists as absent. So it was a view of a table that is always empty.
    ///
    /// The table itself is still there and `store::schedules` still reads it. Dropping a table is deleting
    /// data, and this schema has never dropped anything.
    pub const ALL: [View; 4] = [View::Board, View::Backlog, View::Completed, View::Epics];

    pub fn name(self) -> &'static str {
        match self {
            View::Board => "board",
            View::Backlog => "backlog",
            View::Completed => "completed",
            View::Epics => "epics",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Board => "Board",
            View::Backlog => "Backlog",
            View::Completed => "Completed",
            View::Epics => "Epics",
        }
    }

    pub fn parse(name: &str) -> Option<View> {
        View::ALL.into_iter().find(|view| view.name() == name)
    }
}

/// The plugin's own settings, in `plugins/agent-tasks/settings.conf` beside its manifest.
///
/// Read by the same `store::Values` the window's own settings are read by, in the same `name = value`
/// format, so a person can correct one in a text editor.
///
/// **No secret is in this file.** An agent's authentication key goes to the machine's keychain, which is
/// what the board being replaced does and for the same reason: a settings file is copied between machines,
/// read by anything that can read the folder, and pasted into a bug report. `keychain.rs` is the code half,
/// and what the file holds is the *name* of the keychain entry rather than what is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// Where the board is. Empty means [`store::Store::default_path`].
    pub database: Option<PathBuf>,
    /// The project folder a ticket's agent is launched in when the ticket names none.
    pub project: Option<PathBuf>,
    /// The agent a new ticket is assigned to.
    pub agent: Assignee,
    /// How long a lease is before the watchdog calls it expired.
    pub lease_minutes: i64,
    /// The model a new ticket is given, when it should not be the agent's own default.
    pub model: Option<String>,
    /// The effort a new ticket is given: `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: Option<String>,
    /// Which gateway the agent talks to, when it is not the one the agent ships pointing at.
    ///
    /// Passed to the agent as an environment variable, which is how both of them take it. It is a URL rather
    /// than a secret, so it lives in the file: somebody reading a settings file should be able to see which
    /// gateway their agents are talking to.
    ///
    /// `task-28`: this is Iliad's URL on a configuration that has never been written, because that is the
    /// gateway this machine uses and a field somebody has to paste a URL into before anything works is a
    /// field that stops them. **Empty means the agent's own endpoint** — `api.anthropic.com` for Claude and
    /// OpenAI's for Codex — which is what leaving both environment variables unset already does, and which
    /// is the ticket's "default url for that model".
    pub base_url: Option<String>,
    /// The command a Claude ticket is launched with, when Unluminous's own is not what is wanted.
    ///
    /// The program and the flags in front of it — `claude --dangerously-skip-permissions
    /// --add-dir /tmp` — and Unluminous still puts the ticket's `--model`, `--effort` and the session flag
    /// after it, because those come from the row rather than from a setting and the board cannot
    /// resume a conversation it did not name. Empty means [`agent::launch`]'s own command line.
    ///
    /// **A whole command rather than an extra-arguments field**, so the program itself can be named:
    /// a wrapper script, a particular version under a version manager, or `claude` by full path on a
    /// machine where it is not on any `PATH`. It is split by `run_configurations::split_command`, the
    /// same splitter a run configuration uses, so **no shell runs it** — nothing expands, nothing
    /// globs, and `&&` is one program with a strange argument.
    pub claude_command: Option<String>,
    /// The same for a Codex ticket. `codex resume <id>` puts its subcommand first, which Unluminous goes on
    /// doing, so what is written here is the program and the flags that follow the subcommand.
    pub codex_command: Option<String>,
}

/// The gateway this machine's agents talk to, which a configuration that has never been written starts at.
///
/// The same URL `~/.zshrc` exports as `ANTHROPIC_BASE_URL`. It is a URL rather than a secret, so it is
/// written here in the open, which is the same reason it is written to the settings file.
pub const ILIAD_URL: &str = "https://iliad-emerging-api.abbvienet.com/api/llm";

/// What the key is called in this machine's keychain, under the service `unluminous-agent-tasks`.
///
/// **A constant rather than a setting.** `task-28`: the page used to ask for a `Key name`, a `Key variable`
/// and then the key, which is three values to describe one connection and two of them are Unluminous's own
/// plumbing described to the person using it. There is one key, it is called this, and the variables it is
/// handed to the agent in are [`KEY_VARIABLES`].
pub const KEY_NAME: &str = "iliad";

/// Which environment variables the key is handed to the agent in.
///
/// All of the names that matter, because a name nothing reads costs nothing and a name the agent needed and
/// did not get is a board that cannot talk to the gateway. `~/.zshrc` sets `ANTHROPIC_API_KEY` and points
/// `ILIAD_API_KEY` at the same value for the Codex command line, and `OPENAI_API_KEY` is what Codex reads
/// when it is talking to an OpenAI compatible endpoint.
pub const KEY_VARIABLES: &[&str] = &["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "ILIAD_API_KEY"];

impl Default for Configuration {
    fn default() -> Self {
        Self {
            database: None,
            project: None,
            agent: Assignee::Claude,
            lease_minutes: watchdog::Thresholds::default().lease_minutes,
            model: None,
            effort: None,
            base_url: Some(ILIAD_URL.to_owned()),
            claude_command: None,
            codex_command: None,
        }
    }
}

impl Configuration {
    const FILE: &'static str = "settings.conf";

    /// Read the configuration out of the plugin's folder, or the defaults when there is no file.
    ///
    /// A file that cannot be read is treated as a file that is not there, which is the rule
    /// `services::store` already keeps: a board that opened with its defaults is better than a board
    /// that refused to open because a settings line had a stray character in it.
    pub fn read(folder: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(folder.join(Self::FILE)) else {
            return Self::default();
        };
        let values = crate::services::store::Values::parse(&text);
        let path = |name: &str| {
            values.text(name).map(str::trim).filter(|value| !value.is_empty()).map(PathBuf::from)
        };
        // The same reading as `path`, as text. One closure rather than the same three lines six times: a value
        // that is present and empty is a value nobody chose, which is the rule `Settings::shell` keeps.
        let said = |name: &str| {
            values.text(name).map(str::trim).filter(|value| !value.is_empty()).map(str::to_owned)
        };
        Self {
            database: path("database"),
            project: path("project"),
            agent: values
                .text("agent")
                .and_then(Assignee::parse)
                .filter(|agent| agent.is_an_agent())
                .unwrap_or(Assignee::Claude),
            lease_minutes: values
                .number("lease")
                .map(|minutes| minutes as i64)
                .filter(|minutes| *minutes > 0)
                .unwrap_or_else(|| watchdog::Thresholds::default().lease_minutes),
            model: said("model"),
            effort: said("effort").filter(|level| EFFORTS.contains(&level.as_str())),
            // **Present and empty is not the same as absent.** A file with no `base-url` line has never been
            // written by this version, so it gets Iliad's URL; a file whose line is empty is one somebody
            // cleared on purpose, and clearing it means the agent's own endpoint. `write` always writes the
            // line, so the difference is a real one rather than an accident of which keys happen to be there.
            base_url: match values.text("base-url") {
                Some(url) => Some(url.trim().to_owned()).filter(|url| !url.is_empty()),
                None => Some(ILIAD_URL.to_owned()),
            },
            claude_command: said("claude-command"),
            codex_command: said("codex-command"),
        }
    }

    pub fn write(&self, folder: &std::path::Path) -> Result<(), String> {
        let mut values = crate::services::store::Values::new();
        // Written only once it has been chosen, which is the rule `Settings::shell` and the debug adapter
        // paths already keep: a settings file copied to another machine should name nothing it does not have
        // to. An empty line reads as a value of nothing rather than as no value.
        let mut set = |name: &str, value: Option<String>| {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                values.set(name, value);
            }
        };
        set("database", self.database.clone().map(|path| path.display().to_string()));
        set("project", self.project.clone().map(|path| path.display().to_string()));
        set("model", self.model.clone());
        set("effort", self.effort.clone());
        set("claude-command", self.claude_command.clone());
        set("codex-command", self.codex_command.clone());
        // Always written, even when it is empty, so that a person who cleared it gets the agent's own
        // endpoint rather than Iliad's URL back again. See `read`.
        values.set("base-url", self.base_url.clone().unwrap_or_default());
        values.set("agent", self.agent.name());
        values.set("lease", self.lease_minutes.to_string());
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
        crate::services::store::write_atomically(
            &folder.join(Self::FILE),
            values
                .to_text_headed(
                "The Agent-Tasks board. No secret is in here: the agent's authentication key lives in this \
                 machine's keychain under `iliad`, and `base-url` is the gateway it is used against. An empty \
                 `base-url` means whichever endpoint the agent itself is configured for. `claude-command` and \
                 `codex-command` are the program and the flags in front of it a ticket's agent is launched with, \
                 and the ticket's own model, effort and session flag are added after them; empty means the \
                 command Unluminous builds itself.",
                )
                .as_bytes(),
        )
        .map_err(|problem| format!("the settings could not be written: {problem}"))
    }

    /// The environment an agent is launched with, which is the base URL and the key.
    ///
    /// The key is read from the keychain at the moment of launch rather than held anywhere, so it is in this
    /// process for as long as it takes to hand it to a child and never written down. A key that cannot be read
    /// is left out rather than refused: both agents already know how to log in, and a board that would not
    /// start anything because a keychain entry had been renamed would be worse than one that lets the agent
    /// use its own credentials.
    ///
    /// ## Where the key ends up, said plainly
    ///
    /// In the agent process's own environment, and in the environment of everything that process starts. On
    /// macOS and Linux that is readable by any program running as the same user, which is the same reach a
    /// program would have to run `security find-generic-password` itself, so the keychain is not being
    /// undermined here — it is protecting the key from a copied settings file and from other users, and it does
    /// both. What it does not protect against is a program already running as you, and putting the key in an
    /// environment variable does not change that.
    ///
    /// Both agents read their key from the environment and neither reads it from a file, so there is no
    /// alternative to pass instead. Nothing Unluminous writes ever carries the value: the settings file holds the
    /// **name** of the keychain entry, `SessionSettings` prints its variable names and not their values, and the
    /// terminal's saved scrollback is what the program printed rather than what it was started with.
    pub fn environment(&self) -> Vec<(String, String)> {
        self.environment_given(the_key().as_deref())
    }

    /// The same, told what the key is rather than finding out.
    ///
    /// Split out so that what is built can be tested without a keychain and without touching the environment
    /// of the process running the tests: reading either would make the answer depend on the machine, and
    /// `std::env::set_var` in a test is a race against every other test in the binary. This is the shape
    /// `agent::launch` already has, which is why every command line in that module is a test with no terminal.
    pub fn environment_given(&self, key: Option<&str>) -> Vec<(String, String)> {
        let mut environment = Vec::new();
        if let Some(secret) = key {
            for variable in KEY_VARIABLES {
                environment.push(((*variable).to_owned(), secret.to_owned()));
            }
            // What the Iliad gateway itself wants, which is what `~/.zshrc` sets alongside the key. It carries
            // the key, so it is built here at the moment of launch with the rest and is never written down.
            environment
                .push(("ANTHROPIC_CUSTOM_HEADERS".to_owned(), format!("x-api-key: {secret}")));
        }
        if let Some(url) = &self.base_url {
            // Both agents read a base URL from the environment, and they read different names, so both are
            // set: a value nothing reads costs nothing and a value the agent needed and did not get is a
            // board that talks to the wrong gateway.
            environment.push(("ANTHROPIC_BASE_URL".to_owned(), url.clone()));
            environment.push(("OPENAI_BASE_URL".to_owned(), url.clone()));
        }
        environment
    }

    /// The launch command written down for one agent, if there is one.
    ///
    /// One function rather than the caller choosing between two fields, so `Start` and `Resume session`
    /// cannot disagree about which command a ticket's agent runs. A person is not launched, so they
    /// name nothing.
    pub fn command_for(&self, agent: Assignee) -> Option<String> {
        match agent {
            Assignee::Claude => self.claude_command.clone(),
            Assignee::Codex => self.codex_command.clone(),
            Assignee::Human => None,
        }
    }

    /// Where the board file is, whether or not one was configured.
    ///
    /// `folder` is the plugin's own folder under whichever settings store the window is using, and `None`
    /// is a window with no store at all — a test's window, which gets a board **in memory**. That is what
    /// makes a window pointed at a temporary store keep its board there too; see
    /// [`Store::default_path_in`] for the fault this replaced.
    pub fn database_path(&self, folder: Option<&std::path::Path>) -> Option<PathBuf> {
        match (&self.database, folder) {
            (Some(named), _) => Some(named.clone()),
            (None, Some(folder)) => Some(Store::default_path_in(folder)),
            (None, None) => None,
        }
    }

    /// The same answer as text, for the places that report it. `in memory` is what a window with no
    /// settings folder has, which is a test's window.
    pub fn database_said(&self, folder: Option<&std::path::Path>) -> String {
        match self.database_path(folder) {
            Some(path) => path.display().to_string(),
            None => "in memory".to_owned(),
        }
    }
}
