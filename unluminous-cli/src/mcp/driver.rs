//! Finding the Unluminous a tool call is for, and sending it there.
//!
//! This is the only part of the MCP server that knows an Unluminous exists. It sends exactly what
//! `unluminous-cli` sends — the same wire name, the same arguments object, the same token out of the
//! same instance file, to the same port — so `UnluminousApp::run_cli` stays the one place a command
//! turns into a change and a command run by an agent is the same command a person types.
//!
//! ## Which window
//!
//! A project is a window, so several Unluminous windows run at once and something has to choose. In order,
//! first answer wins:
//!
//! 1. What the tool call said in `instance`, or `--instance` on `mcp serve`, or `UNLUMINOUS_INSTANCE` in
//!    the environment. A process id, a port, or part of a project's path — the three things
//!    `client::choose` already accepts.
//! 2. Exactly one Unluminous running: that one.
//! 3. Exactly one running Unluminous whose project folder is this server's own **preference** — the
//!    folder it was started in, or `CLAUDE_PROJECT_DIR` which Claude Code sets in a spawned
//!    server's environment, or the window's own folder when a window is hosting the server.
//! 4. Otherwise the refusal that lists them. Guessing which window somebody meant is a command
//!    landing in the wrong project, which is exactly the mistake that is hard to notice.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

use crate::catalogue::Command;
use crate::client;
use crate::mcp::server::{Driver, Failure};
use crate::protocol::{code, Reply};

/// How long a tool call waits when the command itself did not say.
///
/// The same fifteen seconds `unluminous-cli` uses, extended by whatever the command was told to wait
/// for, for the same reason: the window's own timeout should be the one that fires, so that what
/// comes back says what it was waiting for rather than "no answer".
const DEFAULT_TIMEOUT: Duration = client::DEFAULT_TIMEOUT;

/// The driver a real server uses.
pub struct UnluminousWindows {
    /// The project this server would rather drive when several are running. See the module comment.
    preference: Option<PathBuf>,
    /// A window named on the command line or in the environment, which beats everything.
    named: Option<String>,
}

impl UnluminousWindows {
    /// A driver with the preference a spawned server has: what it was told, then what the agent
    /// told it, then wherever it was started.
    pub fn new(named: Option<String>) -> Self {
        let named = named.or_else(|| non_empty(std::env::var("UNLUMINOUS_INSTANCE").ok()));
        Self { preference: preferred_folder(), named }
    }

    /// A driver for a window hosting the server itself, which prefers its own project.
    pub fn for_window(folder: PathBuf) -> Self {
        Self { preference: Some(folder), named: None }
    }

    /// The instance a call goes to.
    fn choose(&self, asked: Option<&str>) -> Result<crate::instances::Instance, Failure> {
        let wanted = asked.or(self.named.as_deref());
        match client::choose(wanted) {
            Ok(instance) => Ok(instance),
            Err(problem) => {
                // Several are running and none was named. Before repeating the client's refusal,
                // see whether this server has a preference that picks exactly one of them out.
                if problem.code == code::SEVERAL && wanted.is_none() {
                    if let Some(instance) = self.preferred_among(&client::running()) {
                        return Ok(instance);
                    }
                }
                Err(Failure { code: problem.code, message: problem.message })
            }
        }
    }

    /// The one running Unluminous on the folder this server prefers, when there is exactly one.
    fn preferred_among(
        &self,
        running: &[crate::instances::Instance],
    ) -> Option<crate::instances::Instance> {
        let wanted = self.preference.as_ref()?;
        let mut matched = running.iter().filter(|instance| same_folder(&instance.folder, wanted));
        let first = matched.next()?.clone();
        matched.next().is_none().then_some(first)
    }
}

impl Driver for UnluminousWindows {
    fn run(
        &self,
        command: &'static Command,
        arguments: Map<String, Value>,
        instance: Option<&str>,
    ) -> Result<Reply, Failure> {
        // The commands the client answers on its own are answered here too, without an Unluminous. It is
        // what makes `commands` and `instances` usable at the start of a conversation, before the
        // agent knows whether anything is running.
        if let Some(reply) = locally(command, &arguments) {
            return Ok(reply);
        }
        // A batch is several calls, each sent down this same function, so a step is answered
        // exactly as the same command sent on its own would be. `crate::batch` is the rules.
        if command.wire() == "batch" {
            let steps = match crate::batch::steps_from(&arguments) {
                Ok(steps) => steps,
                Err(problem) => return Ok(Reply::failed("batch", code::USAGE, problem)),
            };
            let keep_going = arguments.get("keep-going").and_then(Value::as_bool).unwrap_or(false);
            return Ok(crate::batch::run(steps, keep_going, |step, given| {
                self.run(step, given, instance)
                    .map_err(|failure| (failure.code.to_owned(), failure.message))
            }));
        }
        // The code index is answered by the index host, which this server becomes when no other
        // process is: an MCP server lives as long as the conversation, so the index stays warm.
        if command.area == "search" {
            // A window named by `instance` is a question about that window's project, so its folder is
            // the root before this server's own. It searched the folder the server was started in.
            let named = instance
                .and_then(|named| self.choose(Some(named)).ok())
                .map(|window| window.folder.to_string_lossy().into_owned());
            let given = arguments
                .get("root")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or(named)
                .or_else(|| self.preference.as_ref().map(|p| p.to_string_lossy().into_owned()));
            let root = crate::search::project_root(given.as_deref());
            let timeout = timeout_for(command, &arguments);
            return Ok(crate::search::ask(
                &root,
                &command.wire(),
                &arguments,
                timeout,
                crate::search::Hosting::InProcess,
            ));
        }
        let instance = self.choose(instance)?;
        // Answered here, in the window's project folder, because the answer is git's text and the
        // window's own Git menu only opens a diff for a person to look at (`git_diff`).
        if command.wire() == "git.diff" {
            return Ok(git_diff(&instance.folder, &arguments));
        }
        let mut arguments = arguments;
        let shortened = shorten_the_wait(command, &mut arguments);
        let timeout = timeout_for(command, &arguments);
        // `timeout` on a command that has no such flag is about the call rather than about the
        // command, exactly as `instance` is, so it does not go on the wire — what is sent is
        // exactly the request `unluminous-cli` would have sent, which is the rule this module is built
        // on.
        if command.flag("timeout").is_none() {
            arguments.remove("timeout");
        }
        let mut reply = client::ask(&instance, &command.wire(), arguments, timeout)
            .map_err(|problem| Failure { code: problem.code, message: problem.message })?;
        if shortened {
            let said = format!(
                "

Waited {} seconds, the longest one call waits, so the agent's own deadline for a tool call is not reached first. Ask again to wait longer.",
                LONGEST_WAIT.as_secs()
            );
            reply.message.push_str(&said);
            if let Some(failure) = reply.error.as_mut() {
                failure.message.push_str(&said);
            }
        }
        Ok(reply)
    }

    fn read_file(&self, path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }
}

/// The longest one tool call waits for something of its own, such as `terminal read --wait-for`.
///
/// **Shorter than an agent's own deadline for a tool call** (`task-2314`). A client gives up on a call
/// it has waited a minute for: opencode answered `MCP error -32001: Request timed out` to a
/// `terminal read` asked to wait two minutes for a build, and what the terminal had printed was lost
/// with it. A wait cut to this answers with what has arrived, and says to ask again.
const LONGEST_WAIT: Duration = Duration::from_secs(50);

/// Cut a command's own wait to [`LONGEST_WAIT`]. Answers whether it did.
fn shorten_the_wait(command: &Command, arguments: &mut Map<String, Value>) -> bool {
    let mut shortened = false;
    for name in ["timeout", "wait"] {
        if command.flag(name).is_none() {
            continue;
        }
        let longest = LONGEST_WAIT.as_millis() as u64;
        if arguments.get(name).and_then(as_millis).is_some_and(|asked| asked > longest) {
            arguments.insert(name.to_owned(), Value::from(longest));
            shortened = true;
        }
    }
    shortened
}

/// The most lines of diff one answer holds, so a change to a generated file does not fill an agent's
/// context. The answer says how many more there were.
const LONGEST_DIFF: usize = 400;

/// `git diff`: the machine's own git, run in the window's project folder, its output as the answer.
///
/// **Text, because that is the question** (`task-2314`). `git action show-diff` opens a diff in the
/// window for a person to read and answers only that it asked, so an agent asked "show me what
/// changed" ran `git diff` in a shell every time: both reps of the agent study's git case did.
fn git_diff(folder: &Path, arguments: &Map<String, Value>) -> Reply {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(folder).args(["--no-pager", "diff", "--no-color"]);
    if arguments.get("staged").and_then(Value::as_bool).unwrap_or(false) {
        command.arg("--staged");
    }
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        command.arg("--").arg(path);
    }
    let output = match command.output() {
        Ok(output) => output,
        Err(problem) => {
            return Reply::failed(
                "git.diff",
                code::FAILED,
                format!("git could not be started: {problem}"),
            )
        }
    };
    if !output.status.success() {
        return Reply::failed(
            "git.diff",
            code::FAILED,
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = text.lines().collect();
    let files = lines.iter().filter(|line| line.starts_with("diff --git ")).count();
    let mut shown = lines.iter().take(LONGEST_DIFF).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > LONGEST_DIFF {
        shown.push_str(&format!(
            "\n… {} more lines; name one file to see the rest of it.",
            lines.len() - LONGEST_DIFF
        ));
    }
    let said = match files {
        0 => "No changes.".to_owned(),
        1 => format!("1 file changed.\n\n{shown}"),
        n => format!("{n} files changed.\n\n{shown}"),
    };
    Reply::done("git.diff", said, serde_json::json!({ "files": files, "lines": lines.len() }))
}

/// The commands that need no window, answered from the catalogue and the instance files.
///
/// `launch` is deliberately not here: it starts a process, and it is `client::launch`'s job rather
/// than a lookup, so it goes down the same road as everything else — which for a local command
/// means it is answered by [`launched`] below.
fn locally(command: &'static Command, arguments: &Map<String, Value>) -> Option<Reply> {
    let answer = answered(command, arguments);
    debug_assert!(
        answer.is_some() == ANSWERED_LOCALLY.contains(&command.wire().as_str()),
        "{} is in ANSWERED_LOCALLY and has no arm below, or the other way round",
        command.typed()
    );
    answer
}

/// Every command this file answers, named so a test can walk the catalogue against it.
///
/// **`task-1984` L4.** A command declared `local: true` and offered as a tool that nothing here
/// answers is a tool that resolves and is then refused by the window with `unknown-command` -- which
/// is what `mcp tools`, `mcp config` and `mcp install` were. `locally` below is the arms; this is the
/// same list in a form a test can read, and `locally`'s own `debug_assert` is what stops the two
/// coming apart.
pub const ANSWERED_LOCALLY: [&str; 6] =
    ["version", "commands", "instances", "launch", "mcp.tools", "mcp.config"];

/// The answer to one of [`ANSWERED_LOCALLY`], or `None` for a command that needs a window.
///
/// **Public because `main.rs` asks it too** (`task-1984` §3.6). The client had its own reading of the
/// same six commands and the two had already drifted; this is the one place they are answered, and
/// what `main.rs` keeps is how a person is shown the answer.
pub fn answered(command: &'static Command, arguments: &Map<String, Value>) -> Option<Reply> {
    match command.wire().as_str() {
        "version" => Some(Reply::done(
            "version",
            format!("unluminous-cli {}", crate::VERSION),
            serde_json::json!({ "version": crate::VERSION }),
        )),
        "commands" => {
            let only = arguments.get("name").and_then(Value::as_str);
            if let Some(name) = only {
                if crate::catalogue::find(name).is_none() {
                    return Some(Reply::failed(
                        "commands",
                        code::NOT_FOUND,
                        format!("There is no command called `{name}`."),
                    ));
                }
            }
            Some(Reply::done("commands", "Unluminous's commands", crate::help::as_json(only)))
        }
        "instances" => {
            let running = client::running();
            let listed: Vec<Value> = running
                .iter()
                .map(|instance| {
                    serde_json::json!({
                        "pid": instance.pid,
                        "port": instance.port,
                        "folder": instance.folder.to_string_lossy(),
                        "started": instance.started,
                        // A window holds a project a tab, and each tab is listed. `task-2322`.
                        "tab": instance.tab,
                        "showing": instance.showing,
                    })
                })
                .collect();
            Some(Reply::done(
                "instances",
                match running.len() {
                    0 => "No Unluminous is running.".to_owned(),
                    1 => "1 Unluminous is running.".to_owned(),
                    several => format!("{several} Unluminous windows are running."),
                },
                serde_json::json!({ "count": running.len(), "instances": listed }),
            ))
        }
        "launch" => Some(launched(arguments)),
        // **`task-1984` L4.** These two are `local: true` and were answered by neither half: not here,
        // and not by the window, which refused them with `unknown-command`. So they resolved as tools
        // and could never succeed. `mcp::ask` is the one reading of their arguments, shared with
        // `main.rs`, so the agent's answer and the person's cannot come apart.
        "mcp.tools" => Some(mcp_tools(arguments)),
        "mcp.config" => Some(mcp_config(arguments)),
        _ => None,
    }
}

/// `mcp tools`: what an agent would be equipped with, and what it costs.
fn mcp_tools(arguments: &Map<String, Value>) -> Reply {
    let areas = match super::ask::areas_from(arguments) {
        Ok(areas) => areas,
        Err(problem) => return Reply::failed("mcp.tools", code::USAGE, problem),
    };
    if arguments.contains_key("count") {
        let counted = super::ask::counted(&areas);
        let said = counted
            .iter()
            .map(|shape| {
                format!(
                    "{}: {} tools, roughly {} tokens",
                    shape["shape"].as_str().unwrap_or_default(),
                    shape["tools"],
                    shape["roughTokens"]
                )
            })
            .collect::<Vec<String>>()
            .join("; ");
        return Reply::done(
            "mcp.tools",
            said,
            serde_json::json!({ "shapes": counted, "areas": areas.names() }),
        );
    }
    let shape = match super::ask::shape_from(arguments) {
        Ok(shape) => shape,
        Err(problem) => return Reply::failed("mcp.tools", code::USAGE, problem),
    };
    let tools = super::tools::as_json_in(shape, &areas);
    Reply::done(
        "mcp.tools",
        format!("{} tools in the {} shape", tools.len(), shape.name()),
        serde_json::json!({
            "shape": shape.name(),
            "areas": areas.names(),
            "count": tools.len(),
            "tools": tools,
        }),
    )
}

/// `mcp config`: the block a person would paste into an agent's own configuration file.
///
/// Read only. Writing that file is `mcp install`, which is held back from the tool list -- see
/// `tools::offered`.
fn mcp_config(arguments: &Map<String, Value>) -> Reply {
    let clients = match super::ask::clients_from(arguments.get("client").and_then(Value::as_str)) {
        Ok(clients) => clients,
        Err(problem) => return Reply::failed("mcp.config", code::USAGE, problem),
    };
    let wanted = match super::ask::wanted_from(arguments) {
        Ok(wanted) => wanted,
        Err(problem) => return Reply::failed("mcp.config", code::USAGE, problem),
    };
    let described = super::ask::configurations(&clients, &wanted);
    Reply::done(
        "mcp.config",
        format!(
            "The configuration for {}",
            clients.iter().map(|client| client.title()).collect::<Vec<&str>>().join(" and ")
        ),
        serde_json::json!({
            "name": wanted.name,
            "transport": wanted.transport.name(),
            "clients": described,
        }),
    )
}

/// Start another Unluminous and wait until it answers, which is what makes the next tool call safe.
fn launched(arguments: &Map<String, Value>) -> Reply {
    let folder = arguments
        .get("folder")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let timeout = arguments
        .get("timeout")
        .and_then(as_millis)
        .map(Duration::from_millis)
        .unwrap_or(client::DEFAULT_LAUNCH_TIMEOUT);
    let wait = !arguments.contains_key("no-wait");
    match client::launch(&folder, timeout, wait) {
        Ok((Some(instance), _)) => Reply::done(
            "launch",
            format!(
                "Unluminous {} is running on port {} in {}",
                instance.pid,
                instance.port,
                instance.folder.display()
            ),
            serde_json::json!({
                "pid": instance.pid,
                "port": instance.port,
                "folder": instance.folder.to_string_lossy(),
            }),
        ),
        Ok((None, pid)) => Reply::done(
            "launch",
            format!("Unluminous was started as process {pid}"),
            serde_json::json!({ "pid": pid }),
        ),
        Err(problem) => Reply::failed("launch", problem.code, problem.message),
    }
}

/// How long to wait for this call.
///
/// Two readings of one argument, and the catalogue is what tells them apart. For the fourteen
/// commands that take a `timeout` **of their own** — `terminal read --wait-for`, `debug start
/// --wait-for-pause`, `git action --wait` — the window is going to hold the answer open for that
/// long on purpose, and a transport that gave up first would report a timeout for something about
/// to work. So those get what they asked for and [`SLACK`] on top.
///
/// For every other command `timeout` means nothing to the window at all: it is simply how long this
/// call is prepared to wait, and it is used exactly as given. `task-1691` reported that it could
/// not be: `DEFAULT_TIMEOUT.max(...)` made fifteen seconds a floor, so an agent could raise the
/// deadline and never lower it, and there was no way to fail fast.
///
/// The rule is `Command::deadline`, in the catalogue, which `unluminous-cli`'s own `client_timeout`
/// also calls. `task-1922` B11: it was written out twice, and neither copy knew what the window
/// waits when no number is given at all.
fn timeout_for(command: &Command, arguments: &Map<String, Value>) -> Duration {
    let asked = ["timeout", "wait"]
        .iter()
        .filter_map(|name| arguments.get(*name))
        .filter_map(as_millis)
        .max();
    command.deadline(asked, DEFAULT_TIMEOUT)
}

fn as_millis(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// The folder this server would rather drive.
///
/// `CLAUDE_PROJECT_DIR` first, because Claude Code sets it in a spawned server's environment and it
/// names the project the person is actually working in — which is a better answer than the working
/// directory a client happened to launch the process from.
fn preferred_folder() -> Option<PathBuf> {
    non_empty(std::env::var("CLAUDE_PROJECT_DIR").ok())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
}

/// Whether two paths name the same folder, as well as this can be known without touching the disk.
///
/// Compared case-insensitively, because Windows paths are, and with a trailing separator ignored.
/// `canonicalize` is deliberately not used: it touches the disk on a path that may be a network
/// share, and choosing which window to drive is not worth a stall.
fn same_folder(one: &Path, other: &Path) -> bool {
    let tidy = |path: &Path| {
        path.to_string_lossy().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase()
    };
    tidy(one) == tidy(other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_matches_itself_however_it_is_spelled() {
        assert!(same_folder(
            Path::new("C:\\jason\\dev\\unluminous"),
            Path::new("c:/jason/dev/unluminous/")
        ));
        assert!(!same_folder(Path::new("C:\\jason\\dev\\unluminous"), Path::new("C:\\jason\\dev")));
    }

    #[test]
    fn the_commands_that_need_no_window_are_answered_without_one() {
        let commands = crate::catalogue::find("commands").expect("commands");
        let reply = locally(commands, &Map::new()).expect("answered locally");
        assert!(reply.ok);
        assert!(reply.result["commands"].as_array().expect("commands").len() > 50);

        let version = crate::catalogue::find("version").expect("version");
        let reply = locally(version, &Map::new()).expect("answered locally");
        assert_eq!(reply.result["version"], serde_json::json!(crate::VERSION));
    }

    #[test]
    fn a_command_that_needs_a_window_is_not_answered_locally() {
        let open = crate::catalogue::find("tab.open").expect("tab open");
        assert!(locally(open, &Map::new()).is_none());
    }

    #[test]
    fn asking_about_a_command_that_does_not_exist_is_refused_rather_than_answered_emptily() {
        let commands = crate::catalogue::find("commands").expect("commands");
        let mut arguments = Map::new();
        arguments.insert("name".to_owned(), Value::String("tab.explode".to_owned()));
        let reply = locally(commands, &arguments).expect("answered locally");
        assert!(!reply.ok);
    }

    #[test]
    fn a_command_told_to_wait_outlasts_its_own_wait() {
        let read = crate::catalogue::find("terminal.read").expect("terminal read");
        assert!(read.flag("timeout").is_some(), "this is the half of the rule that waits");
        let mut arguments = Map::new();
        arguments.insert("timeout".to_owned(), Value::String("60000".to_owned()));
        assert!(timeout_for(read, &arguments) >= Duration::from_millis(65_000));
        assert_eq!(timeout_for(read, &Map::new()), DEFAULT_TIMEOUT);
    }

    #[test]
    fn a_command_that_does_not_wait_is_given_exactly_the_deadline_it_asked_for() {
        // `task-1691`: fifteen seconds was a floor, so an agent could raise the deadline and never
        // lower it. `tab list` does not wait for anything, so `timeout` is purely how long this
        // call will wait for it.
        let list = crate::catalogue::find("tab.list").expect("tab list");
        assert!(list.flag("timeout").is_none(), "this is the half of the rule that does not wait");
        let mut arguments = Map::new();
        arguments.insert("timeout".to_owned(), Value::Number(500.into()));
        assert_eq!(timeout_for(list, &arguments), Duration::from_millis(500));
        assert_eq!(timeout_for(list, &Map::new()), DEFAULT_TIMEOUT);
    }

    /// `git diff` answers with git's own text for the window's folder, one file or all of them.
    #[test]
    fn git_diff_answers_with_the_text_of_what_changed() {
        let folder =
            std::env::temp_dir().join(format!("unluminous-git-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let git = |args: &[&str]| {
            let done = std::process::Command::new("git")
                .arg("-C")
                .arg(&folder)
                .args(args)
                .output()
                .expect("git runs");
            assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
        };
        git(&["init", "-q"]);
        std::fs::write(
            folder.join("a.txt"),
            "one
",
        )
        .unwrap();
        std::fs::write(
            folder.join("b.txt"),
            "two
",
        )
        .unwrap();
        git(&["add", "-A"]);
        git(&["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-qm", "first"]);
        assert_eq!(git_diff(&folder, &Map::new()).message, "No changes.");
        std::fs::write(
            folder.join("a.txt"),
            "one
more
",
        )
        .unwrap();
        std::fs::write(
            folder.join("b.txt"),
            "changed
",
        )
        .unwrap();
        let all = git_diff(&folder, &Map::new());
        assert!(all.ok && all.message.starts_with("2 files changed."), "{}", all.message);
        assert!(all.message.contains("+more"));
        let one = git_diff(&folder, &Map::from_iter([("path".to_owned(), Value::from("a.txt"))]));
        assert!(one.message.starts_with("1 file changed."), "{}", one.message);
        assert!(!one.message.contains("b.txt"));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
