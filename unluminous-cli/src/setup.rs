//! `unluminous-cli setup`: finish setting Unluminous up on this machine (`task-2278`).
//!
//! An install used to stop at copying two programs. Connecting an agent was a second command the
//! person had to know about, the agent instructions on unluminous.com had to name every agent, and on
//! macOS the command line tool was put on the path by an `ln -sf` into a folder that a new Mac does not
//! have. So the setting up is one command, and everything that installs Unluminous runs it: the Windows
//! installer, the window's first start on macOS, an agent following the instructions, a person.
//! `tasks/task-2278-installers-tdd.md` is the design.
//!
//! It does five things and reports each as a [`Step`]:
//!
//! 1. **The command line tool is on the path.** On Windows that is the user's `PATH` in the registry;
//!    elsewhere it is a symbolic link in a folder on the path, and a marked line in the login shell's
//!    profile when the only folder that can be written is not on it.
//! 2. **Each coding agent on the machine** (Claude Code, Codex, OpenCode) is found.
//! 3. **Each agent found is connected** to Unluminous's MCP server, through `mcp::install`. An entry
//!    that already points at another program that exists is somebody's choice and is left alone.
//! 4. **The API keys** the window's AI endpoints read are named, set or not. Never their values.
//! 5. **The server answers**: this program is started as `mcp serve` and asked `initialize` and
//!    `tools/list`, exactly as an agent would ask it.
//!
//! Everything it reads about the machine comes in through [`Machine`], so a test hands it a home
//! folder and a `PATH` of its own, and the window hands it the login shell's environment on macOS.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::mcp::install::{self, Client, Pointing, Wanted};

/// What was asked for.
#[derive(Debug, Clone)]
pub struct Options {
    /// Say what would be done and change nothing.
    pub check: bool,
    /// Take out what `setup` put in.
    pub remove: bool,
    /// Replace an entry or a link that points at another program.
    pub force: bool,
    /// Which agents. All of them by default.
    pub agents: Vec<Client>,
    /// Whether to put the command line tool on the path.
    pub path: PathChoice,
    /// Whether to start the server and ask it for its tools. Off only in a test with no program to
    /// start.
    pub ask_the_server: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            check: false,
            remove: false,
            force: false,
            agents: Client::ALL.to_vec(),
            path: PathChoice::Auto,
            ask_the_server: true,
        }
    }
}

/// Whether the command line tool is put on the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathChoice {
    /// When this program is an installed copy, which on Windows is a folder the installer wrote an
    /// uninstaller into, and anywhere else is a folder that is not a cargo `target` folder. A build
    /// in a checkout is not put on anybody's path unless they ask.
    Auto,
    /// Always, which is `--path`.
    Yes,
    /// Never, which is `--no-path`. The Windows installer passes it, because its own `addtopath`
    /// task is what decides that there.
    No,
}

/// Everything `setup` reads about the machine it is on.
#[derive(Debug, Clone)]
pub struct Machine {
    /// This `unluminous-cli`, which is what every agent is told to start.
    pub program: PathBuf,
    /// The `PATH` agents are looked for on and the path step checks.
    pub search_path: OsString,
    /// The person's home folder.
    pub home: PathBuf,
    /// The environment variables keys and folders are read from.
    pub variables: Vec<(String, String)>,
}

impl Machine {
    /// This process's own environment. On Windows the user's and the machine's `PATH` from the
    /// registry are searched after this process's own, because a process an installer starts can have
    /// a `PATH` older than the one a terminal opened now would get: measured on the Windows installer,
    /// whose `setup` found `claude.exe` and not `codex.cmd` beside it in `C:\nvm4w\nodejs`.
    pub fn this_one() -> Machine {
        #[cfg_attr(not(windows), allow(unused_mut))]
        let mut machine = Machine::with_variables(std::env::vars().collect());
        #[cfg(windows)]
        {
            let mut folders: Vec<PathBuf> = std::env::split_paths(&machine.search_path).collect();
            for list in path::registry_paths() {
                for entry in list.split(';').filter(|entry| !entry.trim().is_empty()) {
                    let folder = PathBuf::from(path::expand(entry.trim()));
                    if !folders.contains(&folder) {
                        folders.push(folder);
                    }
                }
            }
            if let Ok(joined) = std::env::join_paths(folders) {
                machine.search_path = joined;
            }
        }
        machine
    }

    /// The machine as `variables` describes it, which is what the window passes on macOS: the login
    /// shell's environment, because launchd gives an application `PATH=/usr/bin:/bin:/usr/sbin:/sbin`.
    pub fn with_variables(variables: Vec<(String, String)>) -> Machine {
        let search_path = variables
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| OsString::from(value))
            .unwrap_or_default();
        Machine {
            program: install::unluminous_cli_program(),
            search_path,
            home: install::home(),
            variables,
        }
    }

    fn variable(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(named, _)| named == name)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.trim().is_empty())
    }
}

/// How a step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Something was changed, and it worked.
    Done,
    /// It was already as it should be.
    Already,
    /// `--check`: this is what would be changed.
    Would,
    /// Nothing to do: an agent that is not installed, a path that was not asked for.
    Skipped,
    /// Something somebody else chose is in the way, and it was left as it was.
    Left,
    /// Information only, such as which keys are set.
    Noted,
    /// It did not work.
    Failed,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Done => "done",
            State::Already => "already",
            State::Would => "would",
            State::Skipped => "skipped",
            State::Left => "left",
            State::Noted => "noted",
            State::Failed => "failed",
        }
    }
}

/// One step and what came of it.
#[derive(Debug, Clone)]
pub struct Step {
    /// `path`, `claude`, `codex`, `opencode`, `keys` or `server`.
    pub step: String,
    pub state: State,
    /// One sentence for a person.
    pub message: String,
    /// The facts behind the sentence, for `--json`.
    pub detail: Value,
}

impl Step {
    fn new(step: &str, state: State, message: impl Into<String>, detail: Value) -> Step {
        Step { step: step.to_owned(), state, message: message.into(), detail }
    }
}

/// Everything `setup` did.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub steps: Vec<Step>,
}

impl Report {
    /// Whether every step that was attempted worked. An agent that is not installed is not a failure.
    pub fn worked(&self) -> bool {
        self.steps.iter().all(|step| step.state != State::Failed)
    }

    /// The step called `name`.
    pub fn step(&self, name: &str) -> Option<&Step> {
        self.steps.iter().find(|step| step.step == name)
    }

    /// The agents that are connected now, by name.
    pub fn connected(&self) -> Vec<&str> {
        Client::ALL
            .iter()
            .map(|client| client.name())
            .filter(|name| {
                self.step(name)
                    .is_some_and(|step| matches!(step.state, State::Done | State::Already))
            })
            .collect()
    }

    pub fn to_json(&self, machine: &Machine) -> Value {
        json!({
            "program": machine.program.to_string_lossy(),
            "version": crate::VERSION,
            "searched": machine.search_path.to_string_lossy(),
            "pathext": std::env::var("PATHEXT").ok(),
            "worked": self.worked(),
            "connected": self.connected(),
            "steps": self.steps.iter().map(|step| json!({
                "step": step.step,
                "state": step.state.name(),
                "message": step.message,
                "detail": step.detail,
            })).collect::<Vec<Value>>(),
        })
    }

    /// One line a step, for a person.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            out.push_str(&format!("{:<9} {:<8} {}\n", step.step, step.state.name(), step.message));
        }
        out
    }
}

/// Do the setting up, or with `options.check` say what it would do.
pub fn run(options: &Options, machine: &Machine) -> Report {
    let mut report = Report::default();
    // macOS runs an application opened straight from a download out of a temporary, read only copy
    // (App Translocation), whose path changes every time. A link or an agent entry pointing there would
    // stop working the next time the application starts, so nothing is written until it has been moved.
    if machine.program.to_string_lossy().contains("/AppTranslocation/") {
        report.steps.push(Step::new(
            "path",
            State::Failed,
            "Unluminous is running from a temporary copy macOS made of a download. Move Unluminous.app to \
             /Applications, open it from there, and run `unluminous-cli setup` again.",
            json!({ "program": machine.program.to_string_lossy() }),
        ));
        return report;
    }
    report.steps.push(path::step(options, machine));
    for client in Client::ALL {
        if options.agents.contains(&client) {
            report.steps.push(agent_step(client, options, machine));
        }
    }
    if !options.remove {
        report.steps.push(keys_step(machine));
        if options.ask_the_server {
            report.steps.push(server_step(&machine.program));
        }
    }
    report
}

// ---------------------------------------------------------------------------------------- agents

/// Where an agent was found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// Its program, when one was found.
    pub program: Option<PathBuf>,
    /// Its settings folder, when that exists.
    pub folder: Option<PathBuf>,
}

impl Found {
    pub fn is_there(&self) -> bool {
        self.program.is_some() || self.folder.is_some()
    }
}

/// Look for an agent: its program on the `PATH`, then in the folders its own installers use, and its
/// settings folder. Either is enough, because an agent whose program is somewhere this search does not
/// know still reads the file in its folder.
pub fn find_agent(client: Client, machine: &Machine) -> Found {
    let program = look_up(client.name(), &machine.search_path)
        .or_else(|| look_up_in(client.name(), &usual_folders(client, machine)));
    let folder = match client {
        Client::Claude => Some(install::claude_folder()),
        Client::Codex => Some(install::codex_folder()),
        Client::OpenCode => Some(install::opencode_folder()),
    }
    .filter(|folder| folder.is_dir());
    Found { program, folder }
}

/// The folders an agent's own installer puts it in, which may not be on the `PATH` this process was
/// given: Claude Code's native installer uses `~/.local/bin`, npm uses `%APPDATA%\npm` on Windows, and
/// OpenCode's script uses `~/.opencode/bin`.
fn usual_folders(client: Client, machine: &Machine) -> Vec<PathBuf> {
    let home = &machine.home;
    let mut folders = vec![home.join(".local").join("bin")];
    if cfg!(windows) {
        if let Some(data) = machine.variable("APPDATA") {
            folders.push(PathBuf::from(data).join("npm"));
        }
    } else {
        folders.push(PathBuf::from("/opt/homebrew/bin"));
        folders.push(PathBuf::from("/usr/local/bin"));
        folders.push(home.join(".npm-global").join("bin"));
    }
    folders.push(home.join(".bun").join("bin"));
    if client == Client::OpenCode {
        folders.push(home.join(".opencode").join("bin"));
    }
    folders
}

/// Where `name` is on `path`. On Windows only the spellings `PATHEXT` completes it to: npm puts an
/// extensionless shell script beside `claude.cmd` for Git Bash, and Windows cannot start it.
pub fn look_up(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    let folders: Vec<PathBuf> =
        std::env::split_paths(path).filter(|folder| !folder.as_os_str().is_empty()).collect();
    look_up_in(name, &folders)
}

fn look_up_in(name: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    let folders: Vec<PathBuf> = folders.iter().map(|folder| traversable(folder)).collect();
    folders.iter().find_map(|folder| {
        spellings(name)
            .into_iter()
            .map(|spelling| folder.join(spelling))
            .find(|file| file.is_file())
    })
}

/// `folder`, or the folder its links lead to when this process is not allowed through them.
///
/// Inno Setup turns on Windows's RedirectionGuard, and the program it starts after the install is under
/// it too: a junction an ordinary user made cannot be traversed, and every file under one answers
/// `ERROR_UNTRUSTED_MOUNT_POINT` (448). nvm for Windows puts node, npm's `codex.cmd` and `opencode.cmd`
/// behind exactly such a junction, `C:\nvm4w\nodejs`, so `setup` run by the installer found Claude Code
/// and neither of the others (`task-2278`). Reading where a link points is not traversing it, so each
/// part of the path that is a link is replaced by its target, and the folder is searched there.
fn traversable(folder: &Path) -> PathBuf {
    if std::fs::metadata(folder).is_ok() {
        return folder.to_path_buf();
    }
    let mut resolved = PathBuf::new();
    for part in folder.components() {
        resolved.push(part);
        if let Ok(target) = std::fs::read_link(&resolved) {
            resolved = match target.is_absolute() {
                true => target,
                false => resolved.parent().map(|parent| parent.join(&target)).unwrap_or(target),
            };
        }
    }
    resolved
}

/// The file names `name` can have. On Windows the four Windows always runs come first, then anything
/// else `PATHEXT` adds: a program an installer starts may be given no `PATHEXT` at all.
fn spellings(name: &str) -> Vec<String> {
    if !cfg!(windows) {
        return vec![name.to_owned()];
    }
    let mut extensions: Vec<String> =
        [".exe", ".cmd", ".bat", ".com"].iter().map(|e| e.to_string()).collect();
    let listed = std::env::var("PATHEXT").unwrap_or_default();
    for extension in listed.split(';').map(str::trim).filter(|extension| !extension.is_empty()) {
        let extension = extension.to_lowercase();
        if !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }
    extensions.into_iter().map(|extension| format!("{name}{extension}")).collect()
}

/// Connect one agent, or take it out again, or say what would be done.
fn agent_step(client: Client, options: &Options, machine: &Machine) -> Step {
    let found = find_agent(client, machine);
    let wanted = Wanted {
        program: machine.program.clone(),
        folder: machine.home.clone(),
        agent: match &found.program {
            Some(program) => install::Agent::At(program.clone()),
            None => install::Agent::Nothing,
        },
        ..Wanted::default()
    };
    let file = client.file(&wanted);
    let detail = json!({
        "title": client.title(),
        "agent": found.program.as_ref().map(|program| program.to_string_lossy()),
        "folder": found.folder.as_ref().map(|folder| folder.to_string_lossy()),
        "file": file.to_string_lossy(),
    });
    let current = install::points_at(client, &wanted);
    if options.remove {
        return removing(client, options, &wanted, current, detail);
    }
    if !found.is_there() {
        let state = match &current {
            Some(Pointing::Program(program)) if same_program(program, &machine.program) => {
                State::Already
            }
            _ => State::Skipped,
        };
        let message = match state {
            State::Already => format!(
                "{} is not found, and its settings already name Unluminous.",
                client.title()
            ),
            _ => format!("{} is not installed.", client.title()),
        };
        return Step::new(client.name(), state, message, detail);
    }
    let reason = match &current {
        None => None,
        Some(Pointing::Program(program)) if same_program(program, &machine.program) => {
            return Step::new(
                client.name(),
                State::Already,
                format!("{} is already connected, in {}.", client.title(), file.display()),
                detail,
            );
        }
        Some(Pointing::Program(program)) if !program.exists() => Some(format!(
            "its `unluminous` entry named {}, which is not there any more",
            program.display()
        )),
        Some(other) if !options.force => {
            let target = match other {
                Pointing::Program(program) => program.display().to_string(),
                Pointing::Address(url) => url.clone(),
                Pointing::Unknown => "something this program cannot read".to_owned(),
            };
            return Step::new(
                client.name(),
                State::Left,
                format!(
                    "{}'s `unluminous` entry points at {target}, so it was left as it is. `--force` replaces it.",
                    client.title()
                ),
                detail,
            );
        }
        Some(_) => Some("`--force` replaces its `unluminous` entry".to_owned()),
    };
    if options.check {
        let why = reason.map(|why| format!(" ({why})")).unwrap_or_default();
        return Step::new(
            client.name(),
            State::Would,
            format!("{} would be connected{why}.", client.title()),
            detail,
        );
    }
    match install::install(client, &wanted) {
        Ok(done) => Step::new(client.name(), State::Done, done.message, detail),
        Err(problem) => Step::new(client.name(), State::Failed, problem, detail),
    }
}

/// `--remove` for one agent: only an entry that starts this program is taken out.
fn removing(
    client: Client,
    options: &Options,
    wanted: &Wanted,
    current: Option<Pointing>,
    detail: Value,
) -> Step {
    match current {
        Some(Pointing::Program(program)) if same_program(&program, &wanted.program) => {
            if options.check {
                return Step::new(
                    client.name(),
                    State::Would,
                    format!("{}'s `unluminous` entry would be taken out.", client.title()),
                    detail,
                );
            }
            match install::remove(client, wanted) {
                Ok(done) => Step::new(client.name(), State::Done, done.message, detail),
                Err(problem) => Step::new(client.name(), State::Failed, problem, detail),
            }
        }
        Some(_) => Step::new(
            client.name(),
            State::Left,
            format!(
                "{}'s `unluminous` entry starts another program, so it was left as it is.",
                client.title()
            ),
            detail,
        ),
        None => Step::new(
            client.name(),
            State::Skipped,
            format!("{} has no `unluminous` entry.", client.title()),
            detail,
        ),
    }
}

/// Whether two paths name the same program: compared after resolving links, and without regard to
/// case on Windows, where `C:\Users` and `c:\users` are one folder.
pub fn same_program(one: &Path, other: &Path) -> bool {
    let resolve = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (one, other) = (resolve(one), resolve(other));
    if cfg!(windows) {
        one.to_string_lossy().to_lowercase() == other.to_string_lossy().to_lowercase()
    } else {
        one == other
    }
}

// ------------------------------------------------------------------------------------------ keys

/// The keys the window's AI endpoints read, by name. A value is never read out, only whether it is
/// there, which is the rule `Settings -> AI Configuration` keeps.
pub const KEYS: [(&str, &str); 2] =
    [("ANTHROPIC_API_KEY", "the Anthropic API"), ("OPENAI_API_KEY", "the OpenAI API")];

fn keys_step(machine: &Machine) -> Step {
    let set: Vec<&str> = KEYS
        .iter()
        .filter(|(name, _)| machine.variable(name).is_some())
        .map(|(name, _)| *name)
        .collect();
    let message = match set.is_empty() {
        true => "No API key is set. Agents that are installed need none; ANTHROPIC_API_KEY or OPENAI_API_KEY adds an endpoint for that API.".to_owned(),
        false => {
            let named: Vec<String> = KEYS
                .iter()
                .filter(|(name, _)| set.contains(name))
                .map(|(name, what)| format!("{name} is set, so Unluminous offers {what}"))
                .collect();
            format!("{}.", named.join("; "))
        }
    };
    Step::new(
        "keys",
        State::Noted,
        message,
        json!(KEYS
            .iter()
            .map(|(name, _)| (name.to_string(), json!(set.contains(name))))
            .collect::<serde_json::Map<String, Value>>()),
    )
}

// ---------------------------------------------------------------------------------------- server

/// How long the server is given to answer.
const SERVER_PATIENCE: Duration = Duration::from_secs(30);

/// Start `program` as `mcp serve` and ask it `initialize` and `tools/list`, as an agent would.
fn server_step(program: &Path) -> Step {
    match ask_the_server(program, SERVER_PATIENCE) {
        Ok((tools, server)) => Step::new(
            "server",
            State::Already,
            format!("The MCP server answered with {tools} tools."),
            json!({ "tools": tools, "server": server }),
        ),
        Err(problem) => Step::new(
            "server",
            State::Failed,
            format!("The MCP server did not answer: {problem}"),
            json!({ "program": program.to_string_lossy() }),
        ),
    }
}

/// The number of tools the server offered and the name it gave, or why it could not be asked.
pub fn ask_the_server(program: &Path, patience: Duration) -> Result<(usize, String), String> {
    let mut child = std::process::Command::new(program)
        .args(["mcp", "serve"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|problem| format!("{} could not be started: {problem}", program.display()))?;
    let questions = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "unluminous-cli setup", "version": crate::VERSION },
        }}),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    ];
    // Kept open until the answers are read, because a server that sees its input end may stop
    // before it has answered.
    let mut input = child.stdin.take();
    if let Some(input) = input.as_mut() {
        for question in &questions {
            let _ = writeln!(input, "{question}");
        }
        let _ = input.flush();
    }
    let output = child.stdout.take().ok_or("its output could not be read")?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut server = String::new();
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            let Ok(answer) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if answer["id"] == json!(1) {
                server =
                    answer["result"]["serverInfo"]["name"].as_str().unwrap_or_default().to_owned();
            }
            if answer["id"] == json!(2) {
                let tools = answer["result"]["tools"].as_array().map(Vec::len);
                let _ =
                    send.send(tools.map(|count| (count, server.clone())).ok_or_else(|| {
                        format!("it answered `tools/list` without a list: {}", answer)
                    }));
                return;
            }
        }
        let _ = send.send(Err("it stopped before it answered".to_owned()));
    });
    let answer = receive.recv_timeout(patience).unwrap_or_else(|_| {
        Err(format!("it did not answer within {} seconds", patience.as_secs()))
    });
    drop(input);
    let _ = child.kill();
    let _ = child.wait();
    answer
}

// ------------------------------------------------------------------------------------------ path

/// Putting the command line tool on the path.
pub mod path {
    use super::*;

    /// Whether `program` is a copy an installer put there rather than a build in a checkout.
    pub fn is_an_installed_copy(program: &Path) -> bool {
        let folder = program.parent().unwrap_or(Path::new(""));
        if cfg!(windows) {
            return folder.join("unins000.exe").is_file();
        }
        !program.components().any(|part| part.as_os_str() == "target")
    }

    pub fn step(options: &Options, machine: &Machine) -> Step {
        let folder = machine.program.parent().map(Path::to_path_buf).unwrap_or_default();
        let detail = json!({ "folder": folder.to_string_lossy() });
        let wanted = match options.path {
            PathChoice::No => false,
            PathChoice::Yes => true,
            PathChoice::Auto => is_an_installed_copy(&machine.program),
        };
        if options.remove {
            if options.path == PathChoice::No {
                return Step::new(
                    "path",
                    State::Skipped,
                    "`--no-path` was given, so the path was left.",
                    detail,
                );
            }
            return platform::remove(options, machine, detail);
        }
        if platform::is_on_the_path(machine, &folder) {
            return Step::new(
                "path",
                State::Already,
                format!("`unluminous-cli` is on the path, in {}.", folder.display()),
                detail,
            );
        }
        if !wanted {
            let why = match options.path {
                PathChoice::No => "`--no-path` was given",
                _ => "this is a build rather than an installed copy, so `--path` is needed",
            };
            return Step::new(
                "path",
                State::Skipped,
                format!("`unluminous-cli` is not on the path and was not put there: {why}."),
                detail,
            );
        }
        platform::put_on_the_path(options, machine, &folder, detail)
    }

    #[cfg(windows)]
    mod platform {
        use super::*;

        /// Whether `folder` is in this process's `PATH`, or in the user's or the machine's `PATH` in
        /// the registry, which is where an installer that has just run wrote it and where a terminal
        /// opened from now on reads it.
        pub fn is_on_the_path(machine: &Machine, folder: &Path) -> bool {
            let mut lists = vec![machine.search_path.to_string_lossy().into_owned()];
            lists.extend(registry::user_path());
            lists.extend(registry::machine_path());
            lists.iter().any(|list| list_has(list, folder))
        }

        /// Whether a `;` separated list names `folder`, after expanding `%VARIABLES%` the way
        /// Windows does when it builds a process's `PATH`.
        pub fn list_has(list: &str, folder: &Path) -> bool {
            let wanted = folder.to_string_lossy().trim_end_matches('\\').to_lowercase();
            list.split(';')
                .any(|entry| expand(entry).trim().trim_end_matches('\\').to_lowercase() == wanted)
        }

        pub fn expand(entry: &str) -> String {
            let mut out = String::new();
            let mut rest = entry;
            while let Some(start) = rest.find('%') {
                out.push_str(&rest[..start]);
                let after = &rest[start + 1..];
                match after.find('%') {
                    Some(end) => {
                        let name = &after[..end];
                        match std::env::var(name) {
                            Ok(value) => out.push_str(&value),
                            Err(_) => out.push_str(&format!("%{name}%")),
                        }
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push_str(&rest[start..]);
                        rest = "";
                    }
                }
            }
            out.push_str(rest);
            out
        }

        /// The user's and the machine's `PATH` as the registry holds them.
        pub fn registry_paths() -> Vec<String> {
            registry::user_path().into_iter().chain(registry::machine_path()).collect()
        }

        pub fn put_on_the_path(
            options: &Options,
            _machine: &Machine,
            folder: &Path,
            detail: Value,
        ) -> Step {
            if options.check {
                return Step::new(
                    "path",
                    State::Would,
                    format!("{} would be added to your PATH.", folder.display()),
                    detail,
                );
            }
            let existing = registry::user_path().unwrap_or_default();
            let joined = match existing.is_empty() || existing.ends_with(';') {
                true => format!("{existing}{}", folder.display()),
                false => format!("{existing};{}", folder.display()),
            };
            match registry::set_user_path(&joined) {
                Ok(()) => Step::new(
                    "path",
                    State::Done,
                    format!(
                        "Added {} to your PATH. A terminal opened from now on finds `unluminous-cli`.",
                        folder.display()
                    ),
                    detail,
                ),
                Err(problem) => Step::new("path", State::Failed, problem, detail),
            }
        }

        /// Take the folder back out of the user's `PATH`, leaving every other entry as it was.
        pub fn remove(options: &Options, machine: &Machine, detail: Value) -> Step {
            let folder = machine.program.parent().map(Path::to_path_buf).unwrap_or_default();
            let Some(existing) = registry::user_path() else {
                return Step::new("path", State::Skipped, "Your PATH is not set.", detail);
            };
            if !list_has(&existing, &folder) {
                return Step::new(
                    "path",
                    State::Skipped,
                    format!("{} is not in your PATH.", folder.display()),
                    detail,
                );
            }
            if options.check {
                return Step::new(
                    "path",
                    State::Would,
                    format!("{} would be taken out of your PATH.", folder.display()),
                    detail,
                );
            }
            let kept: Vec<&str> =
                existing.split(';').filter(|entry| !list_has(entry, &folder)).collect();
            match registry::set_user_path(&kept.join(";")) {
                Ok(()) => Step::new(
                    "path",
                    State::Done,
                    format!("Took {} out of your PATH.", folder.display()),
                    detail,
                ),
                Err(problem) => Step::new("path", State::Failed, problem, detail),
            }
        }

        /// The user's and the machine's `PATH`, read without expanding the variables in it, because a
        /// value written back expanded would freeze today's `%USERPROFILE%` into it for good.
        mod registry {
            use windows_sys::Win32::System::Registry::{
                RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
                REG_EXPAND_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
            };

            fn wide(text: &str) -> Vec<u16> {
                text.encode_utf16().chain(std::iter::once(0)).collect()
            }

            fn read(root: HKEY, key: &str) -> Option<String> {
                let key = wide(key);
                let name = wide("Path");
                let mut size = 0u32;
                let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
                // Safe: asking for the size only, with no buffer.
                let status = unsafe {
                    RegGetValueW(
                        root,
                        key.as_ptr(),
                        name.as_ptr(),
                        flags,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut size,
                    )
                };
                if status != 0 {
                    return None;
                }
                let mut buffer = vec![0u16; size as usize / 2 + 1];
                let mut size = (buffer.len() * 2) as u32;
                // Safe: `buffer` outlives the call and `size` says how large it is.
                let status = unsafe {
                    RegGetValueW(
                        root,
                        key.as_ptr(),
                        name.as_ptr(),
                        flags,
                        std::ptr::null_mut(),
                        buffer.as_mut_ptr().cast(),
                        &mut size,
                    )
                };
                if status != 0 {
                    return None;
                }
                let length = (size as usize / 2).saturating_sub(1).min(buffer.len());
                Some(String::from_utf16_lossy(&buffer[..length]).trim_end_matches('\0').to_owned())
            }

            pub fn user_path() -> Option<String> {
                read(HKEY_CURRENT_USER, "Environment")
            }

            pub fn machine_path() -> Option<String> {
                read(
                    HKEY_LOCAL_MACHINE,
                    "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
                )
            }

            /// Write the user's `PATH` as `REG_EXPAND_SZ`, then tell every window the environment
            /// changed, which is what makes Explorer hand the new value to the next terminal.
            pub fn set_user_path(value: &str) -> Result<(), String> {
                let key = wide("Environment");
                let name = wide("Path");
                let data = wide(value);
                // Safe: every pointer is to a buffer that outlives the call, and the size is the
                // buffer's size in bytes including its terminating zero.
                let status = unsafe {
                    RegSetKeyValueW(
                        HKEY_CURRENT_USER,
                        key.as_ptr(),
                        name.as_ptr(),
                        REG_EXPAND_SZ,
                        data.as_ptr().cast(),
                        (data.len() * 2) as u32,
                    )
                };
                if status != 0 {
                    return Err(format!("Your PATH could not be written: Windows error {status}."));
                }
                let topic = wide("Environment");
                let mut answer = 0usize;
                // Safe: `topic` outlives the call, and a window that does not answer is given up on
                // after five seconds rather than waited for.
                unsafe {
                    SendMessageTimeoutW(
                        HWND_BROADCAST,
                        WM_SETTINGCHANGE,
                        0,
                        topic.as_ptr() as isize,
                        SMTO_ABORTIFHUNG,
                        5000,
                        &mut answer,
                    );
                }
                Ok(())
            }
        }
    }

    #[cfg(not(windows))]
    mod platform {
        use super::*;

        /// The line `setup` adds to a login profile, marked so it is never added twice and can be
        /// found again by `--remove`.
        pub const MARK: &str = "# added by unluminous-cli setup";

        pub fn is_on_the_path(machine: &Machine, folder: &Path) -> bool {
            let program = &machine.program;
            if std::env::split_paths(&machine.search_path).any(|entry| same_program(&entry, folder))
            {
                return true;
            }
            look_up("unluminous-cli", &machine.search_path)
                .is_some_and(|found| same_program(&found, program))
        }

        /// The folder the link goes in: the first that exists and can be written of
        /// `/usr/local/bin` and `/opt/homebrew/bin`, and otherwise `~/.local/bin`.
        pub fn link_folder(machine: &Machine, candidates: &[PathBuf]) -> PathBuf {
            candidates
                .iter()
                .find(|folder| folder.is_dir() && writable(folder))
                .cloned()
                .unwrap_or_else(|| machine.home.join(".local").join("bin"))
        }

        fn writable(folder: &Path) -> bool {
            use std::os::unix::ffi::OsStrExt;
            let mut bytes = folder.as_os_str().as_bytes().to_vec();
            bytes.push(0);
            // Safe: `bytes` is a terminated path that outlives the call.
            unsafe { libc::access(bytes.as_ptr().cast(), libc::W_OK) == 0 }
        }

        /// The login profile a line goes in, for the login shell `SHELL` names.
        pub fn profile(machine: &Machine) -> PathBuf {
            let shell = machine.variable("SHELL").unwrap_or("/bin/zsh");
            let name = match Path::new(shell).file_name().and_then(|name| name.to_str()) {
                Some("bash") => ".bash_profile",
                Some("zsh") | None => ".zprofile",
                Some(_) => ".profile",
            };
            machine.home.join(name)
        }

        /// The line that puts `folder` on the path, written with `$HOME` where it can be.
        pub fn profile_line(machine: &Machine, folder: &Path) -> String {
            let shown = match folder.strip_prefix(&machine.home) {
                Ok(rest) => format!("$HOME/{}", rest.display()),
                Err(_) => folder.display().to_string(),
            };
            format!("export PATH=\"{shown}:$PATH\" {MARK}")
        }

        pub fn put_on_the_path(
            options: &Options,
            machine: &Machine,
            _folder: &Path,
            detail: Value,
        ) -> Step {
            let candidates = [PathBuf::from("/usr/local/bin"), PathBuf::from("/opt/homebrew/bin")];
            let folder = link_folder(machine, &candidates);
            let link = folder.join("unluminous-cli");
            let mut detail = detail;
            detail["link"] = json!(link.to_string_lossy());
            match std::fs::read_link(&link) {
                Ok(target) if same_program(&target, &machine.program) => {}
                Ok(target) if target.exists() && !options.force => {
                    return Step::new(
                        "path",
                        State::Left,
                        format!(
                            "{} already points at {}, so it was left. `--force` replaces it.",
                            link.display(),
                            target.display()
                        ),
                        detail,
                    );
                }
                Err(_) if link.exists() && !options.force => {
                    return Step::new(
                        "path",
                        State::Left,
                        format!("{} is a file of somebody else's, so it was left.", link.display()),
                        detail,
                    );
                }
                _ => {
                    if options.check {
                        return Step::new(
                            "path",
                            State::Would,
                            format!(
                                "{} would link to {}.",
                                link.display(),
                                machine.program.display()
                            ),
                            detail,
                        );
                    }
                    let made = std::fs::create_dir_all(&folder)
                        .and_then(|()| match std::fs::symlink_metadata(&link) {
                            Ok(_) => std::fs::remove_file(&link),
                            Err(_) => Ok(()),
                        })
                        .and_then(|()| std::os::unix::fs::symlink(&machine.program, &link));
                    if let Err(problem) = made {
                        return Step::new(
                            "path",
                            State::Failed,
                            format!("{} could not be made: {problem}", link.display()),
                            detail,
                        );
                    }
                }
            }
            let on_the_path = std::env::split_paths(&machine.search_path)
                .any(|entry| same_program(&entry, &folder));
            if on_the_path {
                return Step::new(
                    "path",
                    State::Done,
                    format!("Linked {} to {}.", link.display(), machine.program.display()),
                    detail,
                );
            }
            let profile = profile(machine);
            let existing = std::fs::read_to_string(&profile).unwrap_or_default();
            if !existing.contains(MARK) {
                let line = profile_line(machine, &folder);
                let mut text = existing.clone();
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&line);
                text.push('\n');
                if let Err(problem) = std::fs::write(&profile, text) {
                    return Step::new(
                        "path",
                        State::Failed,
                        format!("{} could not be written: {problem}", profile.display()),
                        detail,
                    );
                }
            }
            detail["profile"] = json!(profile.to_string_lossy());
            Step::new(
                "path",
                State::Done,
                format!(
                    "Linked {} to {}, and added {} to the PATH in {}. A terminal opened from now on finds `unluminous-cli`.",
                    link.display(),
                    machine.program.display(),
                    folder.display(),
                    profile.display()
                ),
                detail,
            )
        }

        /// Take out a link that points at this program, and the marked line.
        pub fn remove(options: &Options, machine: &Machine, detail: Value) -> Step {
            let candidates = [
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                machine.home.join(".local").join("bin"),
            ];
            let links: Vec<PathBuf> = candidates
                .iter()
                .map(|folder| folder.join("unluminous-cli"))
                .filter(|link| {
                    std::fs::read_link(link)
                        .is_ok_and(|target| same_program(&target, &machine.program))
                })
                .collect();
            let profile = profile(machine);
            let text = std::fs::read_to_string(&profile).unwrap_or_default();
            let marked = text.contains(MARK);
            if links.is_empty() && !marked {
                return Step::new(
                    "path",
                    State::Skipped,
                    "There is no link or profile line to take out.",
                    detail,
                );
            }
            if options.check {
                return Step::new(
                    "path",
                    State::Would,
                    "The link and the profile line would be taken out.",
                    detail,
                );
            }
            for link in &links {
                let _ = std::fs::remove_file(link);
            }
            if marked {
                let kept: Vec<&str> = text.lines().filter(|line| !line.contains(MARK)).collect();
                let mut kept = kept.join("\n");
                kept.push('\n');
                let _ = std::fs::write(&profile, kept);
            }
            Step::new("path", State::Done, "Took the link and the profile line out.", detail)
        }
    }

    #[cfg(windows)]
    pub use platform::{expand, list_has, registry_paths};
    #[cfg(not(windows))]
    pub use platform::{link_folder, profile, profile_line, MARK};
}

// ------------------------------------------------------------------------------------------- log

/// Write the report to `setup.log` in the settings folder, which is where the Windows installer's
/// hidden run of `setup` says what it did, since nothing reads its output.
pub fn write_the_log(report: &Report, machine: &Machine) -> Option<PathBuf> {
    let folder = crate::instances::settings_folder();
    std::fs::create_dir_all(&folder).ok()?;
    let file = folder.join("setup.log");
    let text = format!(
        "unluminous-cli {} setup, {}\n{}\n{}\n",
        crate::VERSION,
        crate::instances::now(),
        report.to_text(),
        serde_json::to_string_pretty(&report.to_json(machine)).unwrap_or_default()
    );
    std::fs::write(&file, text).ok()?;
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test at a time, in a home of its own, because where the agents' files are is decided by
    /// `UNLUMINOUS_HOME`, which belongs to the whole process. The same lock `mcp::install`'s tests
    /// take, since they set the same variable.
    use crate::mcp::install::tests::ONE_AT_A_TIME;

    struct Home {
        folder: PathBuf,
        _turn: std::sync::MutexGuard<'static, ()>,
    }

    impl Drop for Home {
        fn drop(&mut self) {
            std::env::remove_var("UNLUMINOUS_HOME");
            std::fs::remove_dir_all(&self.folder).ok();
        }
    }

    fn a_home(name: &str) -> Home {
        let turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let folder =
            std::env::temp_dir().join(format!("unluminous-setup-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(folder.join("bin")).expect("make the folder");
        std::env::set_var("UNLUMINOUS_HOME", &folder);
        Home { folder, _turn: turn }
    }

    /// A machine whose `PATH` is the home's own `bin` folder, holding the agents named, and whose
    /// `unluminous-cli` is a file in the home.
    fn machine(home: &Home, agents: &[&str]) -> Machine {
        let bin = home.folder.join("bin");
        for agent in agents {
            // A `.cmd` on Windows that fails at once, so an agent's own command is tried, refuses,
            // and the file is what gets written. That is the half a test can check.
            let (file, text) = match cfg!(windows) {
                true => (bin.join(format!("{agent}.cmd")), "@exit /b 1\r\n".to_owned()),
                false => (bin.join(agent), "#!/bin/sh\nexit 1\n".to_owned()),
            };
            std::fs::write(&file, text).expect("write an agent");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
                    .expect("make it runnable");
            }
        }
        let program =
            home.folder.join(if cfg!(windows) { "unluminous-cli.exe" } else { "unluminous-cli" });
        std::fs::write(&program, "").expect("a program");
        Machine {
            program,
            search_path: bin.into_os_string(),
            home: home.folder.clone(),
            variables: vec![("ANTHROPIC_API_KEY".to_owned(), "a secret".to_owned())],
        }
    }

    fn options() -> Options {
        Options { path: PathChoice::No, ask_the_server: false, ..Options::default() }
    }

    #[test]
    fn an_agent_on_the_path_is_connected_and_one_that_is_not_is_skipped() {
        let home = a_home("connects");
        let machine = machine(&home, &["claude", "opencode"]);
        let report = run(&options(), &machine);
        assert!(report.worked(), "{}", report.to_text());
        assert_eq!(
            report.step("claude").expect("a step").state,
            State::Done,
            "{}",
            report.to_text()
        );
        assert_eq!(report.step("opencode").expect("a step").state, State::Done);
        assert_eq!(report.step("codex").expect("a step").state, State::Skipped);
        assert_eq!(report.connected(), vec!["claude", "opencode"]);

        let claude: Value = serde_json::from_str(
            &std::fs::read_to_string(home.folder.join(".claude.json")).expect("written"),
        )
        .expect("json");
        assert_eq!(
            claude["mcpServers"]["unluminous"]["command"],
            json!(machine.program.to_string_lossy())
        );
        let opencode: Value = serde_json::from_str(
            &std::fs::read_to_string(home.folder.join(".config/opencode/opencode.json"))
                .expect("written"),
        )
        .expect("json");
        assert_eq!(
            opencode["mcp"]["unluminous"]["command"][0],
            json!(machine.program.to_string_lossy())
        );
        assert_eq!(opencode["mcp"]["unluminous"]["type"], json!("local"));
    }

    #[test]
    fn running_it_twice_changes_nothing_the_second_time() {
        let home = a_home("twice");
        let machine = machine(&home, &["claude", "codex", "opencode"]);
        run(&options(), &machine);
        let files = [".claude.json", ".codex/config.toml", ".config/opencode/opencode.json"];
        let before: Vec<String> = files
            .iter()
            .map(|file| std::fs::read_to_string(home.folder.join(file)).expect(file))
            .collect();
        let second = run(&options(), &machine);
        for client in ["claude", "codex", "opencode"] {
            assert_eq!(
                second.step(client).expect("a step").state,
                State::Already,
                "{}",
                second.to_text()
            );
        }
        let after: Vec<String> = files
            .iter()
            .map(|file| std::fs::read_to_string(home.folder.join(file)).expect(file))
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn an_entry_somebody_pointed_at_another_program_is_left_unless_forced() {
        let home = a_home("left");
        let machine = machine(&home, &["claude"]);
        let theirs = home.folder.join("a-build-of-their-own");
        std::fs::write(&theirs, "").expect("their program");
        let entry = json!({ "mcpServers": { "unluminous": { "type": "stdio", "command": theirs.to_string_lossy(), "args": ["mcp", "serve"] } } });
        std::fs::write(home.folder.join(".claude.json"), entry.to_string()).expect("write");
        let only_claude = Options { agents: vec![Client::Claude], ..options() };
        let report = run(&only_claude, &machine);
        assert_eq!(
            report.step("claude").expect("a step").state,
            State::Left,
            "{}",
            report.to_text()
        );
        let forced = run(&Options { force: true, ..only_claude }, &machine);
        assert_eq!(
            forced.step("claude").expect("a step").state,
            State::Done,
            "{}",
            forced.to_text()
        );
    }

    #[test]
    fn an_entry_whose_program_is_gone_is_replaced() {
        let home = a_home("gone");
        let machine = machine(&home, &["opencode"]);
        let folder = home.folder.join(".config/opencode");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(
            folder.join("opencode.json"),
            r#"{"$schema":"https://opencode.ai/config.json","mcp":{"unluminous":{"type":"local","command":["C:/nowhere/unluminous-cli.exe","mcp","serve"]}}}"#,
        )
        .expect("write");
        let report = run(&Options { agents: vec![Client::OpenCode], ..options() }, &machine);
        assert_eq!(
            report.step("opencode").expect("a step").state,
            State::Done,
            "{}",
            report.to_text()
        );
        let text = std::fs::read_to_string(folder.join("opencode.json")).expect("read");
        assert!(
            text.find("$schema") < text.find("\"mcp\""),
            "the order of the file is kept: {text}"
        );
    }

    #[test]
    fn check_says_what_it_would_do_and_writes_nothing() {
        let home = a_home("check");
        let machine = machine(&home, &["claude", "codex", "opencode"]);
        let report = run(&Options { check: true, ..options() }, &machine);
        for client in ["claude", "codex", "opencode"] {
            assert_eq!(report.step(client).expect("a step").state, State::Would);
        }
        assert!(!home.folder.join(".claude.json").exists());
        assert!(!home.folder.join(".codex").exists());
        assert!(!home.folder.join(".config").exists());
    }

    #[test]
    fn remove_takes_out_only_entries_that_start_this_program() {
        let home = a_home("remove");
        let machine = machine(&home, &["claude", "opencode"]);
        run(&options(), &machine);
        let folder = home.folder.join(".config/opencode");
        let theirs = home.folder.join("theirs");
        std::fs::write(&theirs, "").expect("theirs");
        let mut opencode: Value = serde_json::from_str(
            &std::fs::read_to_string(folder.join("opencode.json")).expect("read"),
        )
        .expect("json");
        opencode["mcp"]["unluminous"]["command"][0] = json!(theirs.to_string_lossy());
        std::fs::write(folder.join("opencode.json"), opencode.to_string()).expect("write");

        let report = run(&Options { remove: true, ..options() }, &machine);
        assert_eq!(
            report.step("claude").expect("a step").state,
            State::Done,
            "{}",
            report.to_text()
        );
        assert_eq!(report.step("opencode").expect("a step").state, State::Left);
        assert_eq!(report.step("codex").expect("a step").state, State::Skipped);
        let claude: Value = serde_json::from_str(
            &std::fs::read_to_string(home.folder.join(".claude.json")).expect("read"),
        )
        .expect("json");
        assert!(claude["mcpServers"].get("unluminous").is_none());
    }

    #[test]
    fn a_settings_folder_is_enough_to_find_an_agent() {
        let home = a_home("folder");
        let machine = machine(&home, &[]);
        std::fs::create_dir_all(home.folder.join(".codex")).expect("folder");
        let found = find_agent(Client::Codex, &machine);
        assert!(found.is_there() && found.program.is_none(), "{found:?}");
        let report = run(&Options { agents: vec![Client::Codex], ..options() }, &machine);
        assert_eq!(
            report.step("codex").expect("a step").state,
            State::Done,
            "{}",
            report.to_text()
        );
        assert!(std::fs::read_to_string(home.folder.join(".codex/config.toml"))
            .expect("written")
            .contains("[mcp_servers.unluminous]"));
    }

    #[test]
    fn the_keys_are_named_and_their_values_never_are() {
        let home = a_home("keys");
        let machine = machine(&home, &[]);
        let report = run(&options(), &machine);
        let keys = report.step("keys").expect("a step");
        assert_eq!(keys.detail["ANTHROPIC_API_KEY"], json!(true));
        assert_eq!(keys.detail["OPENAI_API_KEY"], json!(false));
        let everything = report.to_json(&machine).to_string() + &report.to_text();
        assert!(!everything.contains("a secret"), "{everything}");
    }

    #[test]
    fn a_build_in_a_checkout_is_not_an_installed_copy() {
        let build = PathBuf::from("C:/dev/unluminous/target/release/unluminous-cli");
        assert!(!path::is_an_installed_copy(&build) || cfg!(windows));
        if cfg!(windows) {
            // On Windows an installed copy is one with the installer's uninstaller beside it.
            let home = a_home("installed");
            let program = home.folder.join("unluminous-cli.exe");
            assert!(!path::is_an_installed_copy(&program));
            std::fs::write(home.folder.join("unins000.exe"), "").expect("write");
            assert!(path::is_an_installed_copy(&program));
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_path_entry_is_matched_without_case_a_trailing_slash_or_its_variables() {
        let folder = PathBuf::from(std::env::var("USERPROFILE").expect("a profile"))
            .join("Programs")
            .join("Unluminous");
        assert!(path::list_has("C:\\Windows;%USERPROFILE%\\Programs\\Unluminous\\;D:\\x", &folder));
        assert!(path::list_has(&folder.to_string_lossy().to_uppercase(), &folder));
        assert!(!path::list_has("C:\\Windows;C:\\Programs\\Unluminous Extras", &folder));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_link_goes_in_the_first_folder_that_can_be_written_and_the_profile_line_uses_home() {
        let home = a_home("link");
        let machine = machine(&home, &[]);
        let missing = home.folder.join("no-such-folder");
        let writable = home.folder.join("writable");
        std::fs::create_dir_all(&writable).expect("folder");
        assert_eq!(path::link_folder(&machine, &[missing.clone(), writable.clone()]), writable);
        assert_eq!(path::link_folder(&machine, &[missing]), home.folder.join(".local/bin"));
        let line = path::profile_line(&machine, &home.folder.join(".local/bin"));
        assert_eq!(line, format!("export PATH=\"$HOME/.local/bin:$PATH\" {}", path::MARK));
        let zsh = Machine {
            variables: vec![("SHELL".to_owned(), "/bin/zsh".to_owned())],
            ..machine.clone()
        };
        assert_eq!(path::profile(&zsh), home.folder.join(".zprofile"));
        let bash =
            Machine { variables: vec![("SHELL".to_owned(), "/bin/bash".to_owned())], ..machine };
        assert_eq!(path::profile(&bash), home.folder.join(".bash_profile"));
    }

    #[test]
    fn on_windows_an_extensionless_script_is_not_an_agent() {
        let home = a_home("spelling");
        let bin = home.folder.join("bin");
        std::fs::write(bin.join("codex"), "#!/bin/sh\n").expect("write");
        let found = look_up("codex", bin.as_os_str());
        assert_eq!(found.is_some(), !cfg!(windows), "{found:?}");
    }
}
