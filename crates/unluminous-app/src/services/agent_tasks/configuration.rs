//! What the board is showing, and where its database and its agents come from.
//!
//! **Split out of `mod.rs` by `task-1984` §3.6**, which found that file at 2,967 lines holding four
//! different things: the provider, this, the pseudoterminal driver and the JSON view. It is the split
//! `task-1922` gave `plugins.rs`, and nothing about what any of it does changed in the move.
//!
//! [`Configuration`] is what `Settings -> Agent-Tasks` writes and what the board reads: the database
//! file, which endpoint a new ticket is given to and with what, and the lease a working agent renews.
//!
//! **There is no gateway and no key here any more** (`task-2247`). The board used to hand every agent the
//! Iliad gateway's URL and a key from the keychain. The agents it launches are the command-line agents
//! installed on this machine, which hold their own accounts, and which of them a ticket goes to is an
//! endpoint chosen from `Settings -> AI Configuration`.

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
    /// The kind of agent a new ticket is assigned to, which is the kind of [`Self::endpoint`] when one is
    /// named.
    pub agent: Assignee,
    /// The AI endpoint a new ticket is given to, by name, and the one a ticket assigned to a person is
    /// handed to when it is started (`task-2247`). `None` is the first endpoint of [`Self::agent`]'s kind.
    pub endpoint: Option<String>,
    /// How long a lease is before the watchdog calls it expired.
    pub lease_minutes: i64,
    /// The model a new ticket is given, when it should not be the agent's own default.
    pub model: Option<String>,
    /// The effort a new ticket is given: `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: Option<String>,
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

impl Default for Configuration {
    fn default() -> Self {
        Self {
            database: None,
            project: None,
            agent: Assignee::Claude,
            endpoint: None,
            lease_minutes: watchdog::Thresholds::default().lease_minutes,
            model: None,
            effort: None,
            claude_command: None,
            codex_command: None,
        }
    }
}

impl Configuration {
    pub(crate) const FILE: &'static str = "settings.conf";

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
            endpoint: said("endpoint"),
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
        set("endpoint", self.endpoint.clone());
        values.set("agent", self.agent.name());
        values.set("lease", self.lease_minutes.to_string());
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
        crate::services::store::write_atomically(
            &folder.join(Self::FILE),
            values
                .to_text_headed(
                "The Agent-Tasks board. No secret is in here: the agents it launches hold their own accounts. \
                 `endpoint` names the AI endpoint a new ticket is given to, from `ai-endpoints.conf` in the \
                 settings folder. `claude-command` and `codex-command` are the program and the flags in front of \
                 it a ticket's agent is launched with, and the ticket's own model, effort and session flag are \
                 added after them; empty means the command Unluminous builds itself.",
                )
                .as_bytes(),
        )
        .map_err(|problem| format!("the settings could not be written: {problem}"))
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
            Assignee::OpenCode | Assignee::Pi | Assignee::Human => None,
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
