//! Talking to a command-line agent: `claude` and `codex`, run as a child process.
//!
//! **This is the transport the ticket asks for.** *"connection to Claude and codex etc through
//! cli"* — so the pane runs the agent that is already installed and already signed in rather than
//! sending to the API behind it. Four things follow from that, and each of them is a reason this is
//! the better half of the feature rather than a cheaper one:
//!
//! - **Unluminous holds no key.** There is nothing to put in a settings file, nothing to read out of an
//!   environment variable, nothing to redact out of an error message. The agent's own credentials
//!   are the agent's own business.
//! - **The agent brings its own tools**, its own sandbox and its own permission model. Unluminous does not
//!   hand it a tool catalogue and does not run anything on its behalf, so the whole question of what
//!   a model may do to this machine is answered by the program a person already trusts with it.
//! - **It reads the project.** Started in the folder the window has open, `claude` finds that
//!   project's `CLAUDE.md` and `codex` its `AGENTS.md`, so the answer is about the code in front of
//!   you without a word of it being uploaded by Unluminous.
//! - **The conversation is the agent's.** A second question is `--resume <session>` rather than the
//!   whole transcript sent again, so the context the agent has built is the context it keeps.
//!
//! ## Two shapes, and one of them is already understood
//!
//! `claude -p --output-format stream-json --include-partial-messages` emits one JSON object a line,
//! and the interesting ones carry `event`, which is **the Anthropic wire verbatim** — the same
//! `content_block_delta`, `input_json_delta` and `message_delta` that `/v1/messages` sends. So the
//! decoder that already reads that API reads this too, one level down, and thinking blocks, tool
//! calls and token deltas all arrive with no new code. What is added on top is the envelope:
//! `system` with the session id, `user` carrying the results of the tools the agent ran itself, and
//! `result` ending the turn with what it cost.
//!
//! `codex exec --json` is a different model and gets its own reading: a thread of **items** that are
//! started, updated and completed, where a shell command the agent ran is an item beside the words
//! it said. There are no token deltas — an item arrives whole — so an answer appears a paragraph at
//! a time rather than a word at a time, which is the agent's own shape and not something to paper
//! over.
//!
//! Both are read into the same [`crate::wire::Reply`] values, so the pane has never heard of either.
//!
//! ## The prompt goes down standard input
//!
//! Both agents read their instructions from standard input when none is given as an argument, and
//! that is how Unluminous sends one — not as the last word of the command line. **Because a batch file
//! cannot take a multi-line argument**: npm installs `codex` on Windows as `codex.cmd`, and Rust's
//! own `Command` refuses to spawn a batch file with an argument it cannot safely escape, answering
//! *"batch file arguments are invalid"*. Unluminous's first line tells the agent it is answering in a
//! pane, so every prompt has a blank line in it and every `codex` turn failed. Measured on this
//! machine, after the run before it had failed for a different reason in the same place.
//!
//! It is better on every platform besides: a command line has a length limit — 32,767 characters on
//! Windows — and a person pasting a stack trace into the composer would reach it.
//!
//! ## The child is killed, not asked to stop — and the kill comes from outside the reader
//!
//! Stopping is what the composer's stop button does, and an agent in the middle of a turn has no
//! protocol message for it. The process is killed and its pipes are dropped, which is the same thing
//! `unluminous_terminal::Session::kill` does and for the same reason: the alternative is a window that
//! goes on being billed for an answer nobody will read.
//!
//! **The kill cannot come from the thread that is reading it.** That thread is asleep inside a read
//! on the agent's standard output and will not look at a flag until the next line arrives — which,
//! for an agent thinking about a long answer, is not soon. Measured: `stop` put the pane back to
//! `finished` at once, kept what had arrived, and left the child running. So the [`Running`] handle
//! is shared with whoever started the turn, and stopping kills through it; the blocked read then
//! ends because the pipe has closed, which is what unwedges the thread as well.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::model::Conversation;
use crate::provider::{Provider, Wire};
use crate::wire::Reply;

/// The child of a turn that is running, so that stopping can reach it.
///
/// Empty before the agent has been started and after it has ended, which is what makes
/// [`Running::stop`] safe to call at any moment — including on a turn that never started, which is
/// what the stop button does when somebody presses it twice.
///
/// **The turn's generation is held beside the child, and every method names the generation it means.**
/// `task-1922` B2: without it this was one slot that any turn could take out of, and `Client::ask`
/// stops the turn in flight and then starts the new one. The old turn's thread is woken by exactly
/// that kill, walks out of its read, asks for its child back — and was handed whichever child was in
/// the slot, which by then could be the new turn's. It then killed it. So a second question asked
/// while the first was still running could silently kill the second, and what a person saw was a turn
/// that started and stopped for no reason.
///
/// Naming the generation makes each of the three operations about one turn: a thread can only stop,
/// hold or take *its own* child, and a turn that has been overtaken is told no.
#[derive(Default)]
pub struct Running(Mutex<Option<(u64, Child)>>);

impl Running {
    /// Kill the child of turn `generation`, if that is the turn whose child is held.
    ///
    /// The `Child` and its pipe handles are kept rather than dropped, because the thread that started
    /// the turn is the one that waits for it. What ends the blocked read is the **process**: a dead
    /// child closes its own end of the pipe, so the reader gets the end of its input at once — which
    /// is the half that matters, and the reason this is not simply a flag.
    pub fn stop(&self, generation: u64) {
        if let Ok(mut held) = self.0.lock() {
            if let Some((at, child)) = held.as_mut() {
                if *at == generation {
                    let _ = child.kill();
                }
            }
        }
    }

    /// Hold turn `generation`'s child, reaping whatever the slot held before it.
    ///
    /// A displaced child has already been killed by the `stop` that `Client::ask` makes before it
    /// starts a new turn, and its own thread will ask for it by generation and be told no — so this
    /// is the one place left that can wait for it. Without the wait it is a zombie on Unix. The wait
    /// is inside the lock and returns at once, because the process is already dead.
    pub(crate) fn hold(&self, generation: u64, child: Child) {
        if let Ok(mut held) = self.0.lock() {
            if let Some((_, mut displaced)) = held.replace((generation, child)) {
                let _ = displaced.kill();
                let _ = displaced.wait();
            }
        }
    }

    /// Take turn `generation`'s child, or nothing when the slot belongs to another turn.
    pub(crate) fn take(&self, generation: u64) -> Option<Child> {
        let mut held = self.0.lock().ok()?;
        match held.as_ref() {
            Some((at, _)) if *at == generation => held.take().map(|(_, child)| child),
            _ => None,
        }
    }
}

impl std::fmt::Debug for Running {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("Running")
    }
}

/// What one turn asks a command-line agent.
///
/// Built by the caller because it knows things this crate does not: which folder the window has
/// open, what the person actually typed, and what environment the agent runs in.
#[derive(Debug, Clone, Default)]
pub struct Ask {
    /// What to say. One turn's worth: the transcript is the agent's own.
    pub prompt: String,
    /// The folder the agent runs in, which is the project the window has open.
    pub folder: Option<std::path::PathBuf>,
    /// The agent's own session, to carry the conversation on. Empty for the first turn.
    pub session: String,
    /// Pictures written to files, because both agents take an attachment by path.
    pub pictures: Vec<std::path::PathBuf>,
    /// How much the agent may do without being asked.
    pub permission: Permission,
    /// The environment the program is looked for on and started with.
    ///
    /// **Given rather than read**, which is [`crate::environment`]'s whole reason. Two things went
    /// wrong without it, and `unluminous_app::services::login_shell` records both from measuring them:
    /// `claude` lives in `~/.local/bin`, which is not on the `PATH` an Unluminous started from the Dock
    /// has, so it could not be found at all; and a child started from that process gets launchd's dozen
    /// variables rather than the gateway's, so an agent that did start said it was not logged in.
    /// `task-1905`.
    pub environment: crate::environment::Environment,
    /// Where the person's answers arrive when the agent asks before doing something, or `None` when nobody is
    /// to be asked and what the mode does not allow is refused (`task-2392`). See [`crate::approval`].
    pub answers: Option<std::sync::Arc<crate::approval::Answers>>,
}

/// How much a command-line agent may do to this machine without asking.
///
/// **The one setting that matters, and it is deliberately the person's.** An agent run with
/// `--print` cannot stop and ask a question, so what it may do has to be decided before it starts.
/// Three values rather than each agent's own vocabulary, because a person choosing between "read
/// only" and "may edit" should not have to know that one of them calls it a sandbox and the other a
/// permission mode.
///
/// **The two are not the same strength, and saying so is the honest part.** Codex takes an operating
/// system sandbox: at `read` its process cannot write, whatever it decides to do. Claude Code takes
/// a *permission mode*: at `read` it refuses any tool that would change something, which is its own
/// policy rather than the machine's, and its hooks, plugins and MCP servers still start. So one is
/// enforced underneath the agent and the other by it. The values mean the same intent in both, and
/// `Settings -> Agent-Chat` says which kind of guarantee each row gets rather than implying they are
/// one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Permission {
    /// Look, and change nothing. Each agent's own safest setting.
    Read,
    /// May change files in the project it was started in.
    Edit,
    /// May do anything, including run commands with no sandbox at all.
    ///
    /// **The default since `task-2003`**, which asks for it outright: *"Agent chat should have full by
    /// default."* It was `read`, on the reasoning that an agent run with `--print` cannot stop and ask
    /// so the safest setting is the one to start from. What that reasoning left out is who is on the
    /// other end of this one: a chat pane in an editor, started by the person sitting at the machine,
    /// running the agent they already have signed in and already trust with this checkout — the same
    /// agent they would otherwise have run in a terminal, where it has no sandbox either. A pane whose
    /// answer to *"fix this"* is that it is not allowed to is a pane whose first use is a trip to
    /// Settings, and `read` was that for everybody.
    ///
    /// It is a setting rather than a prompt, and the two other values are still a press away for a
    /// checkout where they are what is wanted.
    #[default]
    Full,
}

impl Permission {
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Full => "full",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim() {
            "read" => Some(Self::Read),
            "edit" => Some(Self::Edit),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// Every value [`Permission`] has, for a settings page and for a configuration that names one.
pub const PERMISSIONS: &[&str] = &["read", "edit", "full"];

/// The command line one turn is, as the program and its arguments.
///
/// Split out from [`run`] so a test can assert on what would be run without running it — which is
/// the only way to test the flags at all, since a real agent's answer is not something a test can
/// know. The arguments are **built here rather than configured**, so a row in a settings file cannot
/// turn into a command line that means something else.
pub fn command_line(provider: &Provider, ask: &Ask) -> Vec<String> {
    let mut out = Vec::new();
    match provider.wire {
        Wire::ClaudeCli => {
            out.push("--print".to_owned());
            out.push("--output-format".to_owned());
            out.push("stream-json".to_owned());
            // Without it an answer arrives whole, once, when the turn ends — which is the one thing
            // the ticket asks for by name.
            out.push("--include-partial-messages".to_owned());
            // `stream-json` refuses to run without it.
            out.push("--verbose".to_owned());
            out.push("--permission-mode".to_owned());
            out.push(mode_of(provider, ask.permission).to_owned());
            // **Asked through standard input** (`task-2392`): whatever the mode does not allow is put to the
            // person in the pane rather than refused. The question then goes in as a `stream-json` message and
            // standard input stays open for the answers. See [`crate::approval`].
            if ask.answers.is_some() {
                out.push("--input-format".to_owned());
                out.push("stream-json".to_owned());
                out.push("--permission-prompt-tool".to_owned());
                out.push("stdio".to_owned());
            }
            if !provider.model.trim().is_empty() {
                out.push("--model".to_owned());
                out.push(provider.model.trim().to_owned());
            }
            if !ask.session.trim().is_empty() {
                out.push("--resume".to_owned());
                out.push(ask.session.trim().to_owned());
            }
        }
        Wire::CodexCli => {
            out.push("exec".to_owned());
            let resuming = !ask.session.trim().is_empty();
            if resuming {
                out.push("resume".to_owned());
                out.push(ask.session.trim().to_owned());
            }
            out.push("--json".to_owned());
            // A pane opened on a folder that is not a repository is an ordinary thing, and codex
            // refuses to start in one without this.
            out.push("--skip-git-repo-check".to_owned());
            match mode_of(provider, ask.permission) {
                // Its own name, and it is a long one on purpose. Both forms take this one.
                "danger-full-access" => {
                    out.push("--dangerously-bypass-approvals-and-sandbox".to_owned())
                }
                // **Codex reviews its own requests** (`task-2392`), which is `--approve-for-me` on `codex exec` and
                // absent from `codex exec resume`. The flag is three configuration values, read out of Codex
                // 0.153.4 itself, so both forms are given those.
                "approve-for-me" => {
                    for value in [
                        "sandbox_mode=\"workspace-write\"",
                        "approval_policy=\"on-request\"",
                        "approvals_reviewer=\"auto_review\"",
                    ] {
                        out.push("-c".to_owned());
                        out.push(value.to_owned());
                    }
                }
                // **`codex exec resume` has no `--sandbox`**, though `codex exec` does — measured,
                // after every second question failed with *"unexpected argument '--sandbox'"*. What
                // resume does take is `-c`, its own configuration override, so the sandbox is named
                // that way there. The same value either way, said in the two vocabularies one
                // program has for it.
                mode => match resuming {
                    true => {
                        out.push("-c".to_owned());
                        out.push(format!("sandbox_mode=\"{mode}\""));
                    }
                    false => {
                        out.push("--sandbox".to_owned());
                        out.push(mode.to_owned());
                    }
                },
            }
            if !provider.model.trim().is_empty() {
                out.push("--model".to_owned());
                out.push(provider.model.trim().to_owned());
            }
            for picture in &ask.pictures {
                out.push("--image".to_owned());
                out.push(picture.display().to_string());
            }
        }
        // `opencode run` takes the prompt on standard input, like the other two, which matters for the
        // same reason: on Windows npm installs it as `opencode.cmd`. Measured on OpenCode 1.18.32.
        Wire::OpenCodeCli => {
            out.push("run".to_owned());
            out.push("--format".to_owned());
            out.push("json".to_owned());
            match mode_of(provider, ask.permission) {
                // Its own read-only agent, which is refused every edit and every write.
                "plan" => {
                    out.push("--agent".to_owned());
                    out.push("plan".to_owned());
                }
                "auto" => out.push("--auto".to_owned()),
                // Its own default agent, which edits and asks before anything its configuration has not
                // already allowed. With `run` there is nobody to ask, so what it would ask about is refused.
                _ => {}
            }
            if !provider.model.trim().is_empty() {
                out.push("--model".to_owned());
                out.push(provider.model.trim().to_owned());
            }
            if !ask.session.trim().is_empty() {
                out.push("--session".to_owned());
                out.push(ask.session.trim().to_owned());
            }
            if let Some(folder) = &ask.folder {
                out.push("--dir".to_owned());
                out.push(folder.display().to_string());
            }
            for picture in &ask.pictures {
                out.push("--file".to_owned());
                out.push(picture.display().to_string());
            }
        }
        // `pi --mode json`, with the prompt on standard input because npm installs it as `pi.cmd` on Windows.
        // Measured on Pi 1.3.0 (`task-2302`).
        Wire::PiCli => {
            out.push("--mode".to_owned());
            out.push("json".to_owned());
            // **Pi has no permission prompts**, so what it may do is which tools it is given. `full` is its
            // own default set, which includes `bash`. Like Claude Code's mode, this is the agent's own policy
            // rather than the operating system's.
            match mode_of(provider, ask.permission) {
                "read" => {
                    out.push("--tools".to_owned());
                    out.push("read,grep,find,ls".to_owned());
                }
                "edit" => {
                    out.push("--tools".to_owned());
                    out.push("read,grep,find,ls,edit,write".to_owned());
                }
                _ => {}
            }
            if !provider.model.trim().is_empty() {
                out.push("--model".to_owned());
                out.push(provider.model.trim().to_owned());
            }
            // **`--session`, never `--session-id`, to carry a conversation on.** `--session-id` makes an empty
            // session for an id Pi does not have and carries on with an agent that has seen none of the
            // conversation, saying so only on standard error. `--session` refuses with `No session found
            // matching`, which `refused_to_resume` knows, so the turn is asked again as a replay.
            if !ask.session.trim().is_empty() {
                out.push("--session".to_owned());
                out.push(ask.session.trim().to_owned());
            }
            // A file is attached as `@<path>`, which is how Pi's own command line takes one.
            for picture in &ask.pictures {
                out.push(format!("@{}", picture.display()));
            }
        }
        // **An ACP agent is started with the row's own arguments** (`task-2308`), because which agent it is lives
        // in them: `npx -y @agentclientprotocol/claude-agent-acp`. What it is asked goes over the protocol.
        Wire::Acp => out.extend(provider.args.iter().cloned()),
        // Not a program. A caller that asks anyway gets an empty command line rather than a panic,
        // and `run` refuses it by name.
        Wire::OpenAi | Wire::Anthropic | Wire::Responses => {}
    }
    // **The row's own arguments, after Unluminous's** (`task-2392`), for every program but an ACP agent, whose
    // arguments are which agent it is and are already in front.
    if provider.wire.is_a_program() && provider.wire != Wire::Acp {
        out.extend(provider.args.iter().filter(|word| !word.is_empty()).cloned());
    }
    out
}

/// The mode a turn runs this agent in: the row's own when it names one the agent has, and otherwise
/// `chat.permission` said in the agent's own words (`task-2392`).
pub fn mode_of(provider: &Provider, permission: Permission) -> &str {
    let named = provider.mode.trim();
    if !named.is_empty() && provider.wire.takes_mode(named) {
        return named;
    }
    match (provider.wire, permission) {
        // Its own name for "ask me", which with nobody to ask means "do not do it".
        (Wire::ClaudeCli, Permission::Read) => "manual",
        (Wire::ClaudeCli, Permission::Edit) => "acceptEdits",
        (Wire::ClaudeCli, Permission::Full) => "bypassPermissions",
        (Wire::CodexCli, Permission::Read) => "read-only",
        (Wire::CodexCli, Permission::Edit) => "workspace-write",
        (Wire::CodexCli, Permission::Full) => "danger-full-access",
        (Wire::OpenCodeCli, Permission::Read) => "plan",
        (Wire::OpenCodeCli, Permission::Edit) => "build",
        (Wire::OpenCodeCli, Permission::Full) => "auto",
        (Wire::PiCli, permission) => permission.name(),
        // An ACP agent's mode is asked for over the protocol, and only when the row names one.
        _ => "",
    }
}

/// What is written to the agent's standard input: the words, and any picture it takes no other way.
///
/// Claude Code has no `--image`: it reads a picture when it is given the path of one, which is what
/// a person does in its own prompt box. So the paths go in the words rather than being dropped, and
/// they are named as attachments so the agent is not left guessing why a path is there. Codex has
/// `--image`, so its paths are already on the command line and are not repeated here.
pub fn prompt_for(provider: &Provider, ask: &Ask) -> String {
    if ask.pictures.is_empty()
        || matches!(provider.wire, Wire::CodexCli | Wire::OpenCodeCli | Wire::PiCli)
    {
        return ask.prompt.clone();
    }
    let named: Vec<String> = ask.pictures.iter().map(|path| path.display().to_string()).collect();
    match ask.prompt.trim().is_empty() {
        true => format!("Look at these images: {}", named.join(", ")),
        false => format!("{}\n\nAttached images: {}", ask.prompt, named.join(", ")),
    }
}

/// Run one turn against a command-line agent, handing each reply to `on_reply`.
///
/// Answers `false` from `on_reply` to stop, which is what a request the pane has moved on from does
/// — the same shape [`crate::client::run`] uses for an HTTP one, so `Client` does not care which
/// transport a provider has.
pub fn run(
    provider: &Provider,
    ask: &Ask,
    stopping: &AtomicBool,
    running: &Running,
    generation: u64,
    on_reply: &dyn Fn(Reply) -> bool,
) {
    // **An ACP agent is a conversation, not a stream to read**: it answers requests and asks some of its own,
    // so it has a transport of its own (`task-2308`).
    if provider.wire == Wire::Acp {
        crate::acp::run(provider, ask, stopping, running, generation, on_reply);
        return;
    }
    let mut child = match start(provider, ask, &command_line(provider, ask)) {
        Ok(child) => child,
        Err(why) => {
            on_reply(Reply::Failed(why));
            return;
        }
    };
    let Some(output) = child.stdout.take() else {
        let _ = child.kill();
        on_reply(Reply::Failed("the agent gave nothing to read.".to_owned()));
        return;
    };
    let input = child.stdin.take();
    // **Read on a thread of its own**, because a program that says nothing on standard error must
    // not fill a pipe nobody is emptying and stop writing to the one that is being read. What it
    // says is kept for the refusal, which is `unluminous-git`'s rule about git's own stderr.
    let said = child.stderr.take().map(|errors| {
        std::thread::spawn(move || {
            let mut all = String::new();
            for line in BufReader::new(errors).lines().map_while(Result::ok) {
                if all.len() < 8000 {
                    all.push_str(&line);
                    all.push('\n');
                }
            }
            all
        })
    });

    // **Held before anything that can block**, which is the order the writing below depends on.
    // The prompt is written to a pipe, and a child that has stopped reading — or that is filling a
    // pipe nobody is draining — makes that write block; with the child held only afterwards,
    // `Client::stop` found nothing to kill and the worker and the process both leaked. The stderr
    // reader above is started first for the other half of the same reason.
    //
    // Standard output is deliberately **not** drained during the write, which would need a third
    // thread. Both agents read their input to its end before they say anything, and a prompt is a
    // few kilobytes against a pipe buffer measured in the same units — so the write does not block
    // in practice, and if a future agent made it block, the child is held and stopping kills it.
    running.hold(generation, child);
    if stopping.load(Ordering::Relaxed) {
        running.stop(generation);
        let _ = running.take(generation).map(|mut child| child.wait());
        return;
    }
    // Written and then **closed**, which is the half that matters: both agents read until the end
    // of their input, so a pipe left open is an agent that waits for ever.
    //
    // **Except when the person can be asked** (`task-2392`): then Claude Code reads `stream-json` messages and the
    // answers to its questions arrive on the same pipe, so it stays open until the turn's `result`.
    let asking = ask.answers.is_some() && provider.wire == Wire::ClaudeCli;
    let mut kept: Option<std::process::ChildStdin> = None;
    if let Some(mut input) = input {
        use std::io::Write;
        let prompt = match asking {
            true => format!(
                "{}\n",
                serde_json::json!({
                    "type": "user",
                    "message": { "role": "user", "content": prompt_for(provider, ask) },
                })
            ),
            false => prompt_for(provider, ask),
        };
        if let Err(problem) = input.write_all(prompt.as_bytes()).and_then(|()| input.flush()) {
            let killed = stopping.load(Ordering::Relaxed);
            if let Some(mut child) = running.take(generation) {
                let _ = child.kill();
                let _ = child.wait();
            }
            // A write that failed because somebody pressed stop is not something to report.
            if !killed {
                on_reply(Reply::Failed(format!(
                    "{} would not take the question: {problem}",
                    provider.command.trim()
                )));
            }
            return;
        }
        if asking {
            kept = Some(input);
        }
    }
    let mut decoder = Decoder::new(provider.wire);
    let mut carried_on = true;
    let lines = crate::approval::lines_of(output);
    'reading: loop {
        if stopping.load(Ordering::Relaxed) {
            carried_on = false;
            break;
        }
        // The person's answers, between lines: the agent says nothing while it waits for one.
        if let Some(answers) = ask.answers.as_ref().filter(|_| asking) {
            for (question, choice) in answers.take() {
                if let Some(reply) = decoder.answer(&question, &choice) {
                    if !on_reply(reply) {
                        carried_on = false;
                        break 'reading;
                    }
                }
            }
        }
        if let Some(input) = kept.as_mut() {
            use std::io::Write;
            for message in std::mem::take(&mut decoder.outgoing) {
                let line = format!("{message}\n");
                let _ = input.write_all(line.as_bytes()).and_then(|()| input.flush());
            }
        }
        let line = match lines.recv_timeout(crate::approval::LOOK_AGAIN) {
            Ok(line) => line,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        for reply in decoder.line(line) {
            if !on_reply(reply) {
                carried_on = false;
                break;
            }
        }
        if !carried_on {
            break;
        }
        // The turn's `result` has arrived, so the agent is told there is nothing more to say, and leaves.
        if decoder.ended {
            kept = None;
        }
    }
    drop(kept);
    let mut child = match running.take(generation) {
        Some(child) => child,
        // Killed and taken by `Running::stop`, or displaced by a newer turn — which is exactly what
        // stopping and being overtaken look like from here.
        None => return,
    };
    if !carried_on {
        end(&mut child);
        return;
    }
    let status = child.wait();
    let errors = said.and_then(|thread| thread.join().ok()).unwrap_or_default();
    // **A turn that ended cleanly has already said so**, through its own last event. What is left
    // here is a program that stopped without one, and the honest thing to report is what it said on
    // standard error — which is the only place `claude` and `codex` put a start-up failure at all.
    //
    // **One failure, not two.** `Decoder::finish` says the stream ended mid-answer and the block
    // below says the program stopped without finishing; both fired, so a `codex` that exited with an
    // error reported an interrupted stream *and* an exit code, one under the other.
    let mut already_failed = false;
    for reply in decoder.finish() {
        already_failed |= matches!(reply, Reply::Failed(_));
        if !on_reply(reply) {
            return;
        }
    }
    if !decoder.ended && !already_failed {
        let ended = match status {
            Ok(status) if status.success() => {
                "the agent stopped without finishing its answer.".to_owned()
            }
            Ok(status) => format!("{} stopped with {status}.", provider.command.trim()),
            Err(problem) => {
                format!("{} could not be waited for: {problem}", provider.command.trim())
            }
        };
        let detail = errors.trim();
        on_reply(Reply::Failed(match detail.is_empty() {
            true => ended,
            false => format!("{ended}\n\n{detail}"),
        }));
    }
}

/// Start the program `provider` names with `arguments`, in the turn's folder and environment, with all three of
/// its standard streams piped. The refusal is a sentence for the pane.
///
/// Split out of [`run`] so the ACP transport starts its agent the same way (`task-2308`).
pub(crate) fn start(provider: &Provider, ask: &Ask, arguments: &[String]) -> Result<Child, String> {
    let Some(program) = provider.program_path(&ask.environment) else {
        return Err(provider
            .why_not(&ask.environment)
            .unwrap_or_else(|| format!("`{}` could not be run.", provider.command)));
    };
    let mut command = Command::new(&program);
    command.args(arguments);
    if let Some(folder) = &ask.folder {
        command.current_dir(folder);
    }
    // **The whole environment the caller handed over, and nothing this process happens to hold.** An agent
    // started from a window launchd started has none of the person's own exports otherwise, and says it is
    // not logged in — `task-1905`.
    //
    // `env_clear` before `envs`, which is the half the Codex Sol review of `task-1905` found missing:
    // `Command::envs` replaces the names it is given and **leaves the rest**, so a stale `ANTHROPIC_API_KEY`
    // this process was launched with survived a profile that had removed it, and the child went on using a
    // key its owner had revoked. What the child gets is what a command typed in the person's own shell would
    // have got, which is the whole claim `services::login_shell` makes.
    //
    // **It is cleared only for an environment that really is one**, which is the safe reading of
    // `Environment::is_whole`: a caller that handed over nothing, or a shell profile that could not be read
    // and came back holding a `PATH` and little else, means "whatever you have". Clearing for one of those
    // would start `claude` with no `HOME` and no `USER`, which is a worse failure than a stale variable.
    if ask.environment.is_whole() {
        command.env_clear();
    }
    command.envs(ask.environment.variables().iter().map(|(name, value)| (name, value)));
    // What the row itself names, such as an ACP registry entry's `AUGMENT_DISABLE_AUTO_UPDATE=1`, last so it wins.
    command.envs(provider.env.iter().map(|(name, value)| (name, value)));
    // **The prompt goes down standard input**, for the reason at the top of this file: a batch file
    // cannot take a multi-line argument, and `codex` is a batch file on Windows. Inheriting the
    // window's own would be worse than either, since there is nothing on the other end of it.
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // No console window, which is what `unluminous_git` does for the same reason: a flashing black
        // rectangle every time somebody asks a question is not a feature.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map_err(|problem| format!("{} could not be started: {problem}", program.display()))
}

/// Kill a child and reap it, so a stopped turn leaves no process behind.
fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// One agent's stream, read into [`Reply`] values.
///
/// Holds the state a line on its own does not carry: the session id the turn is to be continued
/// with, and — for Codex, whose items arrive whole rather than as deltas — how much of each item has
/// already been reported, so an item that is updated does not repeat what was already shown.
pub struct Decoder {
    wire: Wire,
    /// The Anthropic decoder, for the events `claude` nests inside its own envelope.
    inner: crate::wire::Decoder,
    /// What has already been sent on for each Codex item, by item id.
    ///
    /// The **text itself** rather than how many bytes of it there were: an item is a whole snapshot
    /// each time, and a length alone assumes every snapshot starts with the last one. A revision that
    /// rewrote its own beginning would then have its middle sent as if it were the end.
    sent: Vec<(String, String)>,
    /// Whether a `result` or a `turn.completed` has been seen, so a child that stops without one is
    /// reported rather than looking like a clean end.
    pub ended: bool,
    /// The agent's own session, which is what the next turn is resumed with.
    pub session: String,
    started: bool,
    /// How much context the last message of this turn started with, from its `message_start`.
    context: Option<u64>,
    /// The tokens read and written by the OpenCode steps of this turn so far, which it reports a step at a
    /// time rather than once for the turn.
    used: (u64, u64),
    /// Claude Code's questions still waiting for the person, by request id, with the request each came in
    /// (`task-2392`).
    questions: Vec<(crate::approval::Question, serde_json::Value)>,
    /// Lines to write to the agent's standard input, which the thread running the turn writes and empties.
    pub outgoing: Vec<serde_json::Value>,
}

impl Decoder {
    pub fn new(wire: Wire) -> Self {
        Self {
            wire,
            inner: crate::wire::Decoder::new(Wire::Anthropic),
            sent: Vec::new(),
            ended: false,
            session: String::new(),
            started: false,
            context: None,
            used: (0, 0),
            questions: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    /// The person chose `choice` for the question `question`: the answer to write, and the line the pane keeps.
    /// `None` for a question this turn did not ask, or a choice it did not offer.
    pub fn answer(&mut self, question: &str, choice: &str) -> Option<Reply> {
        let at = self.questions.iter().position(|(asked, _)| asked.id == question)?;
        let chosen = self.questions[at].0.choice(choice)?.clone();
        let (asked, request) = self.questions.remove(at);
        self.outgoing.push(crate::approval::claude_answer(&asked.id, &request, chosen.kind));
        Some(Reply::Answered {
            id: asked.id.clone(),
            said: crate::approval::decided(&asked, &chosen),
        })
    }

    /// One line of the agent's output.
    ///
    /// A line that is not JSON is **ignored rather than reported**: both agents print the occasional
    /// human sentence among their events, and a pane that showed a refusal every time one did would
    /// be a pane that cried wolf.
    pub fn line(&mut self, line: &str) -> Vec<Reply> {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return Vec::new();
        };
        match self.wire {
            Wire::ClaudeCli => self.claude(&value),
            Wire::CodexCli => self.codex(&value),
            Wire::OpenCodeCli => self.opencode(&value),
            Wire::PiCli => self.pi(&value),
            _ => Vec::new(),
        }
    }

    /// Whatever is still being built when the child has stopped.
    ///
    /// **Only Claude Code has an inner decoder to finish.** Codex's events are read here rather than
    /// nested, so its inner one has seen nothing and finishing it reported an interrupted stream on
    /// every turn that ended without a `turn.completed` — under the sentence saying the program
    /// stopped, which is the one that is actually about what happened.
    pub fn finish(&mut self) -> Vec<Reply> {
        match self.ended || self.wire != Wire::ClaudeCli {
            true => Vec::new(),
            // The Anthropic decoder's own rule: a stream that stopped mid-answer keeps what arrived
            // and says the connection ended before the answer did.
            false => self.inner.finish(),
        }
    }

    /// Claude Code's envelope, whose `event` is the Anthropic wire verbatim.
    fn claude(&mut self, value: &serde_json::Value) -> Vec<Reply> {
        let mut out = Vec::new();
        if let Some(session) = value["session_id"].as_str() {
            if !session.is_empty() && session != self.session {
                self.session = session.to_owned();
                out.push(Reply::Session(session.to_owned()));
            }
        }
        match value["type"].as_str().unwrap_or_default() {
            // **Claude Code asks before using a tool** (`task-2392`), which it does only when started with
            // `--permission-prompt-tool stdio`. Any other request this client did not offer is answered with an
            // error at once, so the agent is never left waiting.
            "control_request" => {
                let id = value["request_id"].as_str().unwrap_or_default().to_owned();
                let request = &value["request"];
                match request["subtype"].as_str() {
                    Some("can_use_tool") => {
                        let question = crate::approval::claude_question(&id, request);
                        self.questions.push((question.clone(), request.clone()));
                        out.push(Reply::Asked(question));
                    }
                    subtype => self.outgoing.push(serde_json::json!({
                        "type": "control_response",
                        "response": {
                            "subtype": "error",
                            "request_id": id,
                            "error": format!("Unluminous does not answer {}.", subtype.unwrap_or("this request")),
                        },
                    })),
                }
            }
            "system"
                // `init` names the model the session really started with, which is the honest answer
                // to what is in the header chip — a row that names no model gets the agent's own.
                if value["subtype"] == "init" => {
                    if let Some(model) = value["model"].as_str().filter(|one| !one.is_empty()) {
                        if !self.started {
                            self.started = true;
                            out.push(Reply::Started { model: model.to_owned() });
                        }
                    }
                }
            // **The context was compacted**, which Claude Code says between two turns or in the middle of
            // one. The note is drawn where it happened.
            "system" if value["subtype"] == "compact_boundary" => {
                let before = value["compact_metadata"]["pre_tokens"].as_u64().unwrap_or(0);
                out.push(Reply::Compacted { before });
            }
            "stream_event" => {
                let event = &value["event"];
                if event["type"] == "message_start" {
                    let usage = &event["message"]["usage"];
                    let used = usage["input_tokens"].as_u64().unwrap_or(0)
                        + usage["cache_read_input_tokens"].as_u64().unwrap_or(0)
                        + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                    if used > 0 {
                        self.context = Some(used);
                    }
                }
                if !self.started {
                    if let Some(model) = event["message"]["model"].as_str() {
                        self.started = true;
                        out.push(Reply::Started { model: model.to_owned() });
                    }
                }
                let inner = self.inner.event(&crate::sse::Event {
                    name: event["type"].as_str().unwrap_or_default().to_owned(),
                    data: event.to_string(),
                });
                // **The usage of a nested message is thrown away**, because the envelope's own
                // `result` reports the whole turn and one turn is what the pane shows. Kept, an
                // ordinary one-message answer was banked twice — once from the `message_delta` in
                // here and once from the `result` outside — and a tool-using turn banked every
                // assistant message and then the sum of all of them again.
                out.extend(inner.into_iter().filter(|reply| !matches!(reply, Reply::Usage { .. })));
            }
            // **The results of the tools the agent ran itself.** Unluminous did not run them and does not
            // need to: what it does with them is show them, so the block the pane already draws for
            // a tool call is filled in from here.
            "user" => {
                for block in value["message"]["content"].as_array().into_iter().flatten() {
                    if block["type"] == "tool_result" {
                        out.push(Reply::ToolAnswer {
                            id: block["tool_use_id"].as_str().unwrap_or_default().to_owned(),
                            answer: text_of(&block["content"]),
                            failed: block["is_error"].as_bool().unwrap_or(false),
                        });
                    }
                }
            }
            "result" => {
                self.ended = true;
                let usage = &value["usage"];
                let input = usage["input_tokens"].as_u64().unwrap_or(0)
                    + usage["cache_read_input_tokens"].as_u64().unwrap_or(0)
                    + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                out.push(Reply::Usage {
                    input,
                    output: usage["output_tokens"].as_u64().unwrap_or(0),
                });
                // **An error is the agent's own words**, which is `unluminous-git`'s rule: it explains
                // itself better than Unluminous could.
                //
                // Claude Code leaves `result` empty for an error it hit before the model was asked, such as
                // a session it does not have, and puts its words in `errors` instead (`task-2285`).
                let errors = value["errors"]
                    .as_array()
                    .map(|all| all.iter().filter_map(|one| one.as_str()).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default();
                match value["is_error"].as_bool().unwrap_or(false) {
                    true => out.push(Reply::Failed(
                        value["result"]
                            .as_str()
                            .filter(|said| !said.is_empty())
                            .map(str::to_owned)
                            .or_else(|| Some(errors).filter(|said| !said.is_empty()))
                            .unwrap_or_else(|| "the agent stopped with an error.".to_owned()),
                    )),
                    false => out.push(Reply::Finished {
                        reason: value["subtype"].as_str().unwrap_or("stop").to_owned(),
                    }),
                }
                // After the end, so a turn's replies keep the order they always had.
                let window = value["modelUsage"]
                    .as_object()
                    .into_iter()
                    .flat_map(|models| models.values())
                    .max_by_key(|model| {
                        model["inputTokens"].as_u64().unwrap_or(0)
                            + model["cacheReadInputTokens"].as_u64().unwrap_or(0)
                            + model["cacheCreationInputTokens"].as_u64().unwrap_or(0)
                    })
                    .and_then(|model| model["contextWindow"].as_u64());
                if self.context.is_some() || window.is_some() {
                    out.push(Reply::Context { used: self.context.take(), window });
                }
            }
            _ => {}
        }
        out
    }

    /// Codex's thread of items.
    fn codex(&mut self, value: &serde_json::Value) -> Vec<Reply> {
        let mut out = Vec::new();
        match value["type"].as_str().unwrap_or_default() {
            "thread.started" => {
                if let Some(thread) = value["thread_id"].as_str() {
                    self.session = thread.to_owned();
                    out.push(Reply::Session(thread.to_owned()));
                }
                if !self.started {
                    self.started = true;
                    out.push(Reply::Started { model: String::new() });
                }
            }
            "item.started" | "item.updated" | "item.completed" => {
                out.extend(self.item(&value["item"], value["type"] == "item.completed"));
            }
            "turn.completed" => {
                self.ended = true;
                let usage = &value["usage"];
                out.push(Reply::Usage {
                    input: usage["input_tokens"].as_u64().unwrap_or(0)
                        + usage["cached_input_tokens"].as_u64().unwrap_or(0),
                    output: usage["output_tokens"].as_u64().unwrap_or(0),
                });
                out.push(Reply::Finished { reason: "stop".to_owned() });
            }
            "turn.failed" | "error" => {
                self.ended = true;
                out.push(Reply::Failed(
                    value["error"]["message"]
                        .as_str()
                        .or_else(|| value["message"].as_str())
                        .unwrap_or("the agent stopped with an error.")
                        .to_owned(),
                ));
            }
            _ => {}
        }
        out
    }

    /// OpenCode's events, one line each: `step_start`, `text`, `reasoning`, `tool_use` and `step_finish`, each
    /// carrying the session and a `part`.
    ///
    /// **A part arrives finished.** OpenCode 1.18 writes a text part once it is complete rather than as it
    /// grows, and a tool call once it has run, with its input and its output in the one event. So a tool
    /// call and its answer are reported together, and a text part goes through [`Self::rest_of`] so a part
    /// that did arrive twice is shown once. A turn is several steps: a step that ends in `tool-calls` is
    /// followed by another, and the step that ends any other way is the end of the turn.
    fn opencode(&mut self, value: &serde_json::Value) -> Vec<Reply> {
        let mut out = Vec::new();
        if let Some(session) = value["sessionID"].as_str() {
            if !session.is_empty() && session != self.session {
                self.session = session.to_owned();
                out.push(Reply::Session(session.to_owned()));
            }
        }
        if !self.started {
            self.started = true;
            out.push(Reply::Started { model: String::new() });
        }
        let part = &value["part"];
        let id = part["id"].as_str().unwrap_or_default().to_owned();
        match value["type"].as_str().unwrap_or_default() {
            "text" => {
                let text = part["text"].as_str().unwrap_or_default();
                out.extend(self.rest_of(&id, text).map(Reply::Text));
            }
            "reasoning" => {
                let text = part["text"].as_str().unwrap_or_default();
                out.extend(self.rest_of(&id, text).map(Reply::Thinking));
            }
            "tool_use" => {
                let call = part["callID"].as_str().unwrap_or(id.as_str()).to_owned();
                let state = &part["state"];
                if self.first_time(&call) {
                    out.push(Reply::ToolCall {
                        id: call.clone(),
                        name: part["tool"].as_str().unwrap_or("tool").to_owned(),
                        arguments: state["input"].to_string(),
                    });
                }
                match state["status"].as_str().unwrap_or_default() {
                    "completed" => out.push(Reply::ToolAnswer {
                        id: call,
                        answer: text_of(&state["output"]),
                        failed: false,
                    }),
                    "error" => out.push(Reply::ToolAnswer {
                        id: call,
                        answer: text_of(&state["error"]),
                        failed: true,
                    }),
                    _ => {}
                }
            }
            "step_finish" => {
                let tokens = &part["tokens"];
                self.used.0 += tokens["input"].as_u64().unwrap_or(0)
                    + tokens["cache"]["read"].as_u64().unwrap_or(0);
                self.used.1 += tokens["output"].as_u64().unwrap_or(0);
                if part["reason"] != "tool-calls" {
                    self.ended = true;
                    out.push(Reply::Usage { input: self.used.0, output: self.used.1 });
                    out.push(Reply::Finished {
                        reason: part["reason"].as_str().unwrap_or("stop").to_owned(),
                    });
                }
            }
            "error" => {
                self.ended = true;
                let error = &value["error"];
                out.push(Reply::Failed(
                    error["data"]["message"]
                        .as_str()
                        .or_else(|| error["message"].as_str())
                        .or_else(|| error["name"].as_str())
                        .unwrap_or("the agent stopped with an error.")
                        .to_owned(),
                ));
            }
            _ => {}
        }
        out
    }

    /// Pi's events (`task-2302`): a `session` header, then the agent's own events, one a line.
    ///
    /// **Words and thinking arrive as deltas**, inside `message_update`'s `assistantMessageEvent`, so they are
    /// passed on as they come. A tool call is complete at `toolcall_end`, and what it got back is
    /// `tool_execution_end`. A model's failure is not a separate event: the assistant `message_end` says
    /// `stopReason: "error"` with the server's words in `errorMessage`, and Pi still exits 0, so that message is
    /// what fails the turn. `agent_settled` is the end of the turn, after any retry or compaction Pi does by
    /// itself. Pi does not say how large the model's context is, so the context ring is given the tokens used
    /// and no window.
    fn pi(&mut self, value: &serde_json::Value) -> Vec<Reply> {
        let mut out = Vec::new();
        match value["type"].as_str().unwrap_or_default() {
            "session" => {
                if let Some(session) = value["id"].as_str().filter(|one| !one.is_empty()) {
                    if session != self.session {
                        self.session = session.to_owned();
                        out.push(Reply::Session(session.to_owned()));
                    }
                }
            }
            "message_start" if value["message"]["role"] == "assistant" && !self.started => {
                self.started = true;
                let model = value["message"]["model"].as_str().unwrap_or_default();
                out.push(Reply::Started { model: model.to_owned() });
            }
            "message_update" => {
                let event = &value["assistantMessageEvent"];
                match event["type"].as_str().unwrap_or_default() {
                    "text_delta" => {
                        let delta = event["delta"].as_str().unwrap_or_default();
                        if !delta.is_empty() {
                            out.push(Reply::Text(delta.to_owned()));
                        }
                    }
                    "thinking_delta" => {
                        let delta = event["delta"].as_str().unwrap_or_default();
                        if !delta.is_empty() {
                            out.push(Reply::Thinking(delta.to_owned()));
                        }
                    }
                    "toolcall_end" => {
                        let call = &event["toolCall"];
                        let id = call["id"].as_str().unwrap_or_default().to_owned();
                        if self.first_time(&id) {
                            out.push(Reply::ToolCall {
                                id,
                                name: call["name"].as_str().unwrap_or("tool").to_owned(),
                                arguments: call["arguments"].to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            "tool_execution_end" => out.push(Reply::ToolAnswer {
                id: value["toolCallId"].as_str().unwrap_or_default().to_owned(),
                answer: text_of(&value["result"]["content"]),
                failed: value["isError"].as_bool().unwrap_or(false),
            }),
            "message_end" if value["message"]["role"] == "assistant" => {
                let message = &value["message"];
                let usage = &message["usage"];
                let input = usage["input"].as_u64().unwrap_or(0)
                    + usage["cacheRead"].as_u64().unwrap_or(0)
                    + usage["cacheWrite"].as_u64().unwrap_or(0);
                self.used.0 += input;
                self.used.1 += usage["output"].as_u64().unwrap_or(0);
                if input > 0 {
                    self.context = Some(input);
                }
                if matches!(message["stopReason"].as_str(), Some("error" | "aborted")) {
                    self.ended = true;
                    out.push(Reply::Failed(
                        message["errorMessage"]
                            .as_str()
                            .filter(|said| !said.trim().is_empty())
                            .unwrap_or("the agent stopped with an error.")
                            .to_owned(),
                    ));
                }
            }
            // **A retry is the turn going on**, so it is shown with the thinking rather than as a failure.
            "auto_retry_start" => {
                let said = value["errorMessage"].as_str().unwrap_or_default();
                out.push(Reply::Thinking(format!("Retrying after: {said}\n")));
            }
            "compaction_end" if value["result"].is_object() => {
                out.push(Reply::Compacted {
                    before: value["result"]["tokensBefore"].as_u64().unwrap_or(0),
                });
            }
            "agent_settled" if !self.ended => {
                self.ended = true;
                out.push(Reply::Usage { input: self.used.0, output: self.used.1 });
                out.push(Reply::Finished {
                    reason: match value["aborted"].as_bool().unwrap_or(false) {
                        true => "aborted".to_owned(),
                        false => "stop".to_owned(),
                    },
                });
                if let Some(used) = self.context.take() {
                    out.push(Reply::Context { used: Some(used), window: None });
                }
            }
            _ => {}
        }
        out
    }

    /// One Codex item, reported as only the part of it that has not been reported already.
    ///
    /// **Because an item arrives whole and then arrives again.** `item.updated` carries the item's
    /// text so far and `item.completed` carries all of it, so sending each one on would show the
    /// answer three times over. What is remembered is how many characters of each item have been
    /// passed on, and only the rest is.
    fn item(&mut self, item: &serde_json::Value, completed: bool) -> Vec<Reply> {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        match item["type"].as_str().unwrap_or_default() {
            "agent_message" => {
                let text = item["text"].as_str().unwrap_or_default();
                self.rest_of(&id, text).map(Reply::Text).into_iter().collect()
            }
            "reasoning" => {
                let text = item["text"].as_str().unwrap_or_default();
                self.rest_of(&id, text).map(Reply::Thinking).into_iter().collect()
            }
            // **A command the agent ran is a tool call**, which is what the pane already draws: the
            // command is the call and its output is the answer. Codex's own shape, reported as
            // Unluminous's, so a tool block means one thing whichever agent produced it.
            "command_execution" => {
                let mut out = Vec::new();
                if self.first_time(&id) {
                    out.push(Reply::ToolCall {
                        id: id.clone(),
                        name: "shell".to_owned(),
                        arguments: serde_json::json!({
                            "command": item["command"].as_str().unwrap_or_default(),
                        })
                        .to_string(),
                    });
                }
                if completed {
                    let code = item["exit_code"].as_i64();
                    out.push(Reply::ToolAnswer {
                        id,
                        answer: item["aggregated_output"].as_str().unwrap_or_default().to_owned(),
                        failed: code.is_some_and(|code| code != 0),
                    });
                }
                out
            }
            "file_change" | "mcp_tool_call" | "web_search" => {
                let mut out = Vec::new();
                if self.first_time(&id) {
                    out.push(Reply::ToolCall {
                        id: id.clone(),
                        name: item["type"].as_str().unwrap_or("tool").to_owned(),
                        arguments: item.to_string(),
                    });
                }
                if completed {
                    out.push(Reply::ToolAnswer {
                        id,
                        answer: item["status"].as_str().unwrap_or("done").to_owned(),
                        failed: item["status"] == "failed",
                    });
                }
                out
            }
            // **An error item is a warning, and the turn goes on.** codex-cli 0.153.4 writes one on every turn
            // on a machine with many skills ("Skill descriptions were shortened to fit the skills context
            // budget"), followed by the answer and `turn.completed`. Read as a failure it marked every Codex
            // turn failed (`task-2285`). A turn that really fails ends with `turn.failed` or a top level
            // `error`, which are failures above. So the warning is shown with the agent's thinking.
            "error" if completed => match item["message"].as_str().map(str::trim) {
                Some(said) if !said.is_empty() => vec![Reply::Thinking(format!("{said}\n"))],
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// The part of `text` that has not been reported for this item yet, and nothing when it has all
    /// been.
    ///
    /// The already-sent text is compared as a **prefix**, not as a length. When the snapshot grows on
    /// the front of what was sent — a revision, a shorter value, a reused id — the two have nothing
    /// to do with each other and the whole of the new one is sent rather than a slice out of its
    /// middle. It cannot land inside a character either, because the split is at a prefix boundary.
    /// **Nothing is copied but the part being reported** (`task-1984` P17). This used to clone the
    /// whole already-sent text to compare against, and then clone the whole new snapshot to record --
    /// and Codex sends a snapshot per update, so an answer of a hundred kilobytes arriving in a
    /// thousand updates copied about a hundred megabytes to report a hundred kilobytes. What is kept
    /// is grown in place instead, so a long answer costs one buffer that doubles rather than a
    /// thousand buffers thrown away.
    fn rest_of(&mut self, id: &str, text: &str) -> Option<String> {
        let at = self.sent.iter().position(|(one, _)| one == id);
        let carries_on = at.is_some_and(|at| text.starts_with(self.sent[at].1.as_str()));
        let from = match carries_on {
            true => self.sent[at.expect("carries_on needs one")].1.len(),
            false => 0,
        };
        let rest = text[from..].to_owned();
        match at {
            Some(at) => {
                let said = &mut self.sent[at].1;
                if !carries_on {
                    said.clear();
                }
                said.push_str(&rest);
            }
            None => self.sent.push((id.to_owned(), rest.clone())),
        }
        match rest.is_empty() {
            true => None,
            false => Some(rest),
        }
    }

    /// Whether this item has not been seen before, remembering that it has now.
    fn first_time(&mut self, id: &str) -> bool {
        if self.sent.iter().any(|(one, _)| one == id) {
            return false;
        }
        self.remember(id, String::new());
        true
    }

    fn remember(&mut self, id: &str, said: String) {
        match self.sent.iter_mut().find(|(one, _)| one == id) {
            Some(entry) => entry.1 = said,
            None => self.sent.push((id.to_owned(), said)),
        }
    }
}

/// A tool result's content, which is a string in the simple case and a list of blocks otherwise.
pub(crate) fn text_of(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(said) => said.clone(),
        serde_json::Value::Array(blocks) => {
            blocks.iter().filter_map(|block| block["text"].as_str()).collect::<Vec<&str>>().join("")
        }
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Whether an agent's failure says it does not have the session it was asked to resume.
///
/// Measured on Claude Code 2.1.302, codex-cli 0.153.4 and OpenCode 1.18.32 by asking each to resume an id
/// it had never issued. All three refuse before any model is called, so recognising the refusal and asking
/// again without the id costs nothing (`task-2285`):
///
/// - Claude Code: `No conversation found with session ID: <id>`, in its `result` line's `errors`.
/// - Codex: `thread/resume failed: no rollout found for thread id <id>`, on standard error.
/// - OpenCode: `Error: Session not found`, on standard error.
/// - Pi 1.3.0: `No session found matching '<id>'`, on standard error, with exit code 1 (`task-2302`).
pub fn refused_to_resume(words: &str) -> bool {
    let words = words.to_lowercase();
    words.contains("no conversation found with session id")
        || words.contains("no rollout found for thread id")
        || (words.contains("thread/resume") && words.contains("not found"))
        || words.contains("session not found")
        || words.contains("no session found matching")
}

/// The session an agent gave for this conversation, so the next turn carries on rather than starting
/// again.
///
/// On the conversation rather than on the provider, because it is *this* conversation the agent is
/// holding — starting a new one in the pane must start a new one in the agent too.
pub fn session_of(chat: &Conversation) -> String {
    chat.session.clone()
}

/// Where a picture is written so an agent can be given its path.
///
/// Both agents take an attachment as a file, and the pane holds one as bytes — so it is written to
/// the temporary folder under a name that says what it is. Left there once written: deleting it
/// while the agent is still reading it is a race, and the operating system clears its own temporary
/// folder.
///
/// **A fresh file every time, created rather than written over.** The name carries a counter as well
/// as the process, because two attachments called `shot.png` are an ordinary thing and the first
/// version gave them one path — so one overwrote the other, and a second turn overwrote a file the
/// first agent might still be reading. `create_new` is the other half: a predictable path in a
/// folder every account can write is a path something else can leave a symbolic link at, and
/// `File::create` would follow it. Creating fails on anything already there, so the counter moves on.
pub fn write_a_picture(folder: &Path, name: &str, bytes: &[u8]) -> Option<std::path::PathBuf> {
    use std::io::Write;
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let safe: String = name
        .chars()
        .map(|one| match one.is_ascii_alphanumeric() || matches!(one, '.' | '-' | '_') {
            true => one,
            false => '-',
        })
        .collect();
    for _ in 0..64 {
        let at = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = folder.join(format!("unluminous-chat-{}-{at}-{safe}", std::process::id()));
        let made = std::fs::OpenOptions::new().write(true).create_new(true).open(&path);
        if let Ok(mut file) = made {
            file.write_all(bytes).ok()?;
            file.flush().ok()?;
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(wire: Wire) -> Provider {
        Provider::defaults().into_iter().find(|one| one.wire == wire).expect("a row of that shape")
    }

    #[test]
    fn a_rows_own_mode_beats_chat_permission_and_its_arguments_follow_unluminouss() {
        let mut claude = provider(Wire::ClaudeCli);
        claude.mode = "auto".to_owned();
        claude.args = vec!["--add-dir".to_owned(), "../shared".to_owned()];
        let line = command_line(&claude, &Ask { permission: Permission::Read, ..Ask::default() });
        let at = line.iter().position(|word| word == "--permission-mode").expect("a mode");
        assert_eq!(line[at + 1], "auto");
        assert_eq!(&line[line.len() - 2..], ["--add-dir", "../shared"]);
        // A mode the agent does not have is not passed on: the permission decides.
        claude.mode = "approve-for-me".to_owned();
        let line = command_line(&claude, &Ask { permission: Permission::Edit, ..Ask::default() });
        let at = line.iter().position(|word| word == "--permission-mode").expect("a mode");
        assert_eq!(line[at + 1], "acceptEdits");

        let mut codex = provider(Wire::CodexCli);
        codex.mode = "approve-for-me".to_owned();
        for session in ["", "s1"] {
            let line = command_line(&codex, &Ask { session: session.to_owned(), ..Ask::default() })
                .join(" ");
            assert!(line.contains("-c approvals_reviewer=\"auto_review\""), "{line}");
            assert!(!line.contains("--dangerously"), "{line}");
        }
        let mut opencode = provider(Wire::OpenCodeCli);
        opencode.mode = "plan".to_owned();
        assert!(command_line(&opencode, &Ask::default()).join(" ").contains("--agent plan"));
    }

    #[test]
    fn claude_code_is_asked_through_standard_input_when_the_person_can_answer() {
        let claude = provider(Wire::ClaudeCli);
        let answers = std::sync::Arc::new(crate::approval::Answers::default());
        let line =
            command_line(&claude, &Ask { answers: Some(answers), ..Ask::default() }).join(" ");
        assert!(
            line.contains("--input-format stream-json --permission-prompt-tool stdio"),
            "{line}"
        );
        assert!(!command_line(&claude, &Ask::default()).join(" ").contains("--input-format"));
    }

    #[test]
    fn claude_codes_question_is_shown_and_the_answer_is_written_back() {
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let replies = decoder.line(r#"{"type":"control_request","request_id":"r7","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"},"permission_suggestions":[]}}"#);
        let Some(Reply::Asked(question)) = replies.first() else { panic!("{replies:?}") };
        assert_eq!(question.tool, "Bash");
        assert!(decoder.outgoing.is_empty());
        assert!(decoder.answer("r8", "allow").is_none());
        let said = decoder.answer("r7", "deny").expect("answered");
        assert!(matches!(said, Reply::Answered { ref id, .. } if id == "r7"));
        assert_eq!(decoder.outgoing[0]["response"]["request_id"], "r7");
        assert_eq!(decoder.outgoing[0]["response"]["response"]["behavior"], "deny");
        // A request nobody here answers is refused at once rather than left waiting.
        decoder.outgoing.clear();
        decoder.line(
            r#"{"type":"control_request","request_id":"r9","request":{"subtype":"hook_callback"}}"#,
        );
        assert_eq!(decoder.outgoing[0]["response"]["subtype"], "error");
    }

    #[test]
    fn the_two_agents_are_asked_in_their_own_words() {
        let ask = Ask {
            prompt: "What does this do?".to_owned(),
            permission: Permission::Read,
            ..Ask::default()
        };
        let claude = command_line(&provider(Wire::ClaudeCli), &ask);
        assert!(claude.contains(&"--print".to_owned()));
        assert!(claude.contains(&"stream-json".to_owned()));
        // Without it the answer arrives once, whole, at the end — and streaming is the thing the
        // ticket asks for by name.
        assert!(claude.contains(&"--include-partial-messages".to_owned()));
        assert!(claude.contains(&"--verbose".to_owned()), "stream-json refuses without it");

        let codex = command_line(&provider(Wire::CodexCli), &ask);
        assert_eq!(codex[0], "exec");
        assert!(codex.contains(&"--json".to_owned()));
        assert!(codex.contains(&"read-only".to_owned()));

        // **The question is on neither command line**, because a batch file cannot take a multi-line
        // argument and `codex` is one on Windows. It goes down standard input instead.
        for line in [&claude, &codex] {
            assert!(
                !line.iter().any(|word| word.contains("What does this do?")),
                "the question reached the command line: {line:?}"
            );
        }
        assert_eq!(prompt_for(&provider(Wire::ClaudeCli), &ask), "What does this do?");

        // And an endpoint is not a program, so it has no command line at all.
        assert!(command_line(&provider(Wire::OpenAi), &ask).is_empty());
    }

    /// `task-2247`: OpenCode is asked in its own words, and the question goes down standard input because
    /// npm installs it as `opencode.cmd` on Windows.
    #[test]
    fn opencode_is_asked_in_its_own_words() {
        let mut ask = Ask {
            prompt: "What does this do?\nAnd this?".to_owned(),
            permission: Permission::Read,
            folder: Some(std::path::PathBuf::from("project")),
            ..Ask::default()
        };
        let mut row = provider(Wire::OpenCodeCli);
        row.model = "llama-local/local".to_owned();
        let read = command_line(&row, &ask);
        assert_eq!(&read[..3], ["run", "--format", "json"]);
        assert!(read.windows(2).any(|pair| pair == ["--agent", "plan"]), "{read:?}");
        assert!(read.windows(2).any(|pair| pair == ["--model", "llama-local/local"]), "{read:?}");
        assert!(read.windows(2).any(|pair| pair == ["--dir", "project"]), "{read:?}");
        assert!(!read.iter().any(|word| word.contains("What does this do?")), "{read:?}");
        ask.permission = Permission::Full;
        ask.session = "ses_1".to_owned();
        let full = command_line(&row, &ask);
        assert!(full.contains(&"--auto".to_owned()), "{full:?}");
        assert!(!full.contains(&"plan".to_owned()), "{full:?}");
        assert!(full.windows(2).any(|pair| pair == ["--session", "ses_1"]), "{full:?}");
        assert_eq!(prompt_for(&row, &ask), "What does this do?\nAnd this?");
    }

    /// `task-2302`: Pi's permission is the tool list it is given, and a conversation is carried on with
    /// `--session`, which refuses an id it does not have, never `--session-id`, which would make an empty one.
    #[test]
    fn pi_is_asked_in_its_own_words() {
        let mut ask = Ask {
            prompt: "What does this do?\nAnd this?".to_owned(),
            permission: Permission::Read,
            pictures: vec![std::path::PathBuf::from("shot.png")],
            ..Ask::default()
        };
        let mut row = provider(Wire::PiCli);
        assert_eq!(row.command, "pi");
        row.model = "local-qwen/qwen".to_owned();
        let read = command_line(&row, &ask);
        assert_eq!(&read[..2], ["--mode", "json"]);
        assert!(read.windows(2).any(|pair| pair == ["--tools", "read,grep,find,ls"]), "{read:?}");
        assert!(read.windows(2).any(|pair| pair == ["--model", "local-qwen/qwen"]), "{read:?}");
        assert!(read.contains(&"@shot.png".to_owned()), "{read:?}");
        assert!(!read.iter().any(|word| word.contains("What does this do?")), "{read:?}");
        assert!(!read.contains(&"--session".to_owned()), "a first turn names no session");
        ask.permission = Permission::Edit;
        let edit = command_line(&row, &ask);
        assert!(edit.windows(2).any(|pair| pair == ["--tools", "read,grep,find,ls,edit,write"]));
        ask.permission = Permission::Full;
        ask.session = "01a1541d-de65-73d9-af86-1d6c68c1eabd".to_owned();
        let full = command_line(&row, &ask);
        assert!(!full.contains(&"--tools".to_owned()), "full is Pi's own set, with bash: {full:?}");
        assert!(
            full.windows(2)
                .any(|pair| pair == ["--session", "01a1541d-de65-73d9-af86-1d6c68c1eabd"]),
            "{full:?}"
        );
        assert!(!full.contains(&"--session-id".to_owned()), "{full:?}");
        // The picture is an argument, so the words are not given its path a second time.
        assert_eq!(prompt_for(&row, &ask), "What does this do?\nAnd this?");
    }

    #[test]
    fn a_second_question_carries_the_agents_own_session_rather_than_the_transcript() {
        // Which is the whole reason this transport is better than sending to the API behind it: the
        // context the agent has built is the context it keeps, and Unluminous sends one turn's words.
        // **The permission is named rather than defaulted**, because the second half of this asserts
        // how `codex exec resume` is told about the sandbox — and at `full`, which is the default since
        // `task-2003`, there is no sandbox to name. `read` is what that half is about.
        let ask = Ask {
            prompt: "And the other one?".to_owned(),
            session: "29611139-4d2a-495a-b9a9-94a6189e509c".to_owned(),
            permission: Permission::Read,
            ..Ask::default()
        };
        let claude = command_line(&provider(Wire::ClaudeCli), &ask);
        let at = claude.iter().position(|one| one == "--resume").expect("resumed");
        assert_eq!(claude[at + 1], "29611139-4d2a-495a-b9a9-94a6189e509c");

        let codex = command_line(&provider(Wire::CodexCli), &ask);
        assert_eq!(codex[0], "exec");
        assert_eq!(codex[1], "resume", "its own sub-command rather than a flag");
        assert_eq!(codex[2], "29611139-4d2a-495a-b9a9-94a6189e509c");
        // And the sandbox is named the way *resume* names it, which is not the way `exec` does.
        assert!(!codex.contains(&"--sandbox".to_owned()), "{codex:?}");
        let at = codex.iter().position(|one| one == "-c").expect("a config override");
        assert_eq!(codex[at + 1], "sandbox_mode=\"read-only\"");
    }

    #[test]
    fn what_an_agent_may_do_is_said_in_each_agents_own_vocabulary() {
        for (permission, claude_says, codex_says) in [
            (Permission::Read, "manual", "read-only"),
            (Permission::Edit, "acceptEdits", "workspace-write"),
            (Permission::Full, "bypassPermissions", "--dangerously-bypass-approvals-and-sandbox"),
        ] {
            let ask = Ask { permission, ..Ask::default() };
            assert!(
                command_line(&provider(Wire::ClaudeCli), &ask).contains(&claude_says.to_owned()),
                "{permission:?}"
            );
            assert!(
                command_line(&provider(Wire::CodexCli), &ask).contains(&codex_says.to_owned()),
                "{permission:?}"
            );
        }
        // And every value the settings page offers is one the code has.
        for name in PERMISSIONS {
            assert_eq!(Permission::from_name(name).expect("registered with no code").name(), *name);
        }
        assert!(Permission::from_name("whatever").is_none());
    }

    #[test]
    fn a_picture_reaches_each_agent_the_way_that_agent_takes_one() {
        let ask = Ask {
            prompt: "What is wrong with this?".to_owned(),
            pictures: vec![std::path::PathBuf::from("/tmp/shot.png")],
            ..Ask::default()
        };
        // Codex has a flag for it.
        let codex = command_line(&provider(Wire::CodexCli), &ask);
        let at = codex.iter().position(|one| one == "--image").expect("an image flag");
        assert!(codex[at + 1].contains("shot.png"));
        // …so its question does not name the file twice.
        assert_eq!(prompt_for(&provider(Wire::CodexCli), &ask), "What is wrong with this?");
        // Claude Code has no such flag, and reads a picture it is given the path of — so the path
        // goes in the words rather than the attachment being dropped in silence.
        let prompt = prompt_for(&provider(Wire::ClaudeCli), &ask);
        assert!(prompt.contains("What is wrong with this?"));
        assert!(prompt.contains("shot.png"), "{prompt}");
    }

    #[test]
    fn claude_code_saying_it_compacted_its_context_is_read_as_a_note() {
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto","pre_tokens":41234}}"#,
            ],
        );
        assert_eq!(replies, vec![Reply::Compacted { before: 41_234 }]);
    }

    #[test]
    fn claude_code_wraps_the_anthropic_wire_and_is_read_by_the_decoder_that_already_reads_it() {
        // Which is the measurement this whole transport rests on: `stream_event.event` is byte for
        // byte what `/v1/messages` sends, so thinking, tool calls and token deltas need no new code.
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"system","subtype":"init","session_id":"s-1","model":"claude-sonnet-5"}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},"session_id":"s-1"}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello "}},"session_id":"s-1"}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"there"}},"session_id":"s-1"}"#,
                r#"{"type":"result","subtype":"success","is_error":false,"session_id":"s-1","usage":{"input_tokens":2,"cache_read_input_tokens":90,"output_tokens":7}}"#,
            ],
        );
        assert_eq!(replies[0], Reply::Session("s-1".to_owned()));
        assert_eq!(replies[1], Reply::Started { model: "claude-sonnet-5".to_owned() });
        assert_eq!(replies[2], Reply::Text("hello ".to_owned()));
        assert_eq!(replies[3], Reply::Text("there".to_owned()));
        assert_eq!(replies[4], Reply::Usage { input: 92, output: 7 });
        assert_eq!(replies[5], Reply::Finished { reason: "success".to_owned() });
        assert_eq!(decoder.session, "s-1", "the next turn resumes this one");
        assert!(decoder.ended);
    }

    #[test]
    fn a_tool_the_agent_ran_itself_is_shown_with_the_answer_it_got() {
        // **Unluminous runs nothing here.** The agent has its own tools and its own permission model, so
        // what the pane does with a tool call is show it — the block it already draws, filled in
        // from the agent's own report of what it did.
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"Bash","input":{}}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"git status\"}"}}}"#,
                r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"main","is_error":false}]}}"#,
            ],
        );
        assert_eq!(
            replies[0],
            Reply::ToolCall {
                id: "toolu_1".to_owned(),
                name: "Bash".to_owned(),
                arguments: "{\"command\":\"git status\"}".to_owned(),
            }
        );
        assert_eq!(
            replies[1],
            Reply::ToolAnswer {
                id: "toolu_1".to_owned(),
                answer: "main".to_owned(),
                failed: false,
            }
        );
    }

    #[test]
    fn a_codex_item_that_arrives_twice_is_shown_once() {
        // Its items are not deltas: `item.updated` carries the text so far and `item.completed`
        // carries all of it, so passing each on whole showed the answer twice over.
        let mut decoder = Decoder::new(Wire::CodexCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"thread.started","thread_id":"t-1"}"#,
                r#"{"type":"turn.started"}"#,
                r#"{"type":"item.updated","item":{"id":"item_0","type":"agent_message","text":"hello"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"hello from the cli."}}"#,
                r#"{"type":"turn.completed","usage":{"input_tokens":21429,"cached_input_tokens":11008,"output_tokens":9}}"#,
            ],
        );
        assert_eq!(replies[0], Reply::Session("t-1".to_owned()));
        assert_eq!(replies[1], Reply::Started { model: String::new() });
        assert_eq!(replies[2], Reply::Text("hello".to_owned()));
        assert_eq!(
            replies[3],
            Reply::Text(" from the cli.".to_owned()),
            "only the part that had not been shown"
        );
        assert_eq!(replies[4], Reply::Usage { input: 32437, output: 9 });
        assert_eq!(replies[5], Reply::Finished { reason: "stop".to_owned() });
        assert_eq!(decoder.session, "t-1");
    }

    #[test]
    fn a_command_codex_ran_is_a_tool_block_with_its_output_in_it() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"git status","status":"in_progress"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"git status","aggregated_output":"nothing to commit","exit_code":0,"status":"completed"}}"#,
            ],
        );
        let Reply::ToolCall { id, name, arguments } = &replies[0] else {
            panic!("{replies:?}");
        };
        assert_eq!(id, "item_1");
        assert_eq!(name, "shell");
        assert!(arguments.contains("git status"));
        assert_eq!(
            replies[1],
            Reply::ToolAnswer {
                id: "item_1".to_owned(),
                answer: "nothing to commit".to_owned(),
                failed: false,
            }
        );
        assert_eq!(replies.len(), 2, "the call is announced once: {replies:?}");
    }

    #[test]
    fn a_command_that_failed_says_so_rather_than_looking_like_an_answer() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        let replies = feed(
            &mut decoder,
            &[
                r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"nope","aggregated_output":"not found","exit_code":127,"status":"failed"}}"#,
            ],
        );
        let Reply::ToolAnswer { failed, .. } = replies[1] else {
            panic!("{replies:?}");
        };
        assert!(failed);
    }

    #[test]
    fn a_line_that_is_not_json_is_ignored_and_an_error_is_the_agents_own_words() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        assert!(decoder.line("Reading additional input from stdin...").is_empty());
        assert!(decoder.line("").is_empty());
        let said =
            decoder.line(r#"{"type":"turn.failed","error":{"message":"the model is overloaded"}}"#);
        assert_eq!(said, vec![Reply::Failed("the model is overloaded".to_owned())]);
        assert!(decoder.ended);

        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let said = decoder.line(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Credit balance is too low"}"#,
        );
        assert!(said
            .iter()
            .any(|reply| matches!(reply, Reply::Failed(said) if said.contains("Credit balance"))));
    }

    #[test]
    fn stopping_kills_the_child_rather_than_waiting_for_it_to_say_something() {
        // **The measurement this exists for.** The thread reading an agent is asleep inside a read
        // with no timeout, so a flag it will not look at until the next line arrives does not stop
        // anything: `stop` put the pane back to `finished` at once, kept what had arrived, and left
        // the agent running. A long-lived child stands in for one here, because when a real agent
        // next says something is not something a test can know.
        let running = Running::default();
        // Its own name on each platform, and both are always there.
        let mut command = match cfg!(windows) {
            true => {
                let mut one = Command::new("ping");
                one.args(["-n", "60", "127.0.0.1"]);
                one
            }
            false => {
                let mut one = Command::new("sleep");
                one.arg("60");
                one
            }
        };
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("a long-lived child");
        running.hold(1, child);
        running.stop(1);
        let mut child = running.take(1).expect("the child is still held after being killed");
        let ended = child.wait().expect("it can be waited for");
        assert!(!ended.success(), "a killed program does not succeed: {ended}");

        // And stopping a turn that never started is not an error, which is what pressing the stop
        // button twice does.
        let nothing = Running::default();
        nothing.stop(1);
        assert!(nothing.take(1).is_none());
    }

    /// A long-lived child, for the tests that need a process rather than an agent.
    fn a_long_lived_child() -> Child {
        // Its own name on each platform, and both are always there.
        let mut command = match cfg!(windows) {
            true => {
                let mut one = Command::new("ping");
                one.args(["-n", "60", "127.0.0.1"]);
                one
            }
            false => {
                let mut one = Command::new("sleep");
                one.arg("60");
                one
            }
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("a long-lived child")
    }

    /// **A turn that has been overtaken cannot take the new turn's child.** `task-1922` B2.
    ///
    /// `Client::ask` stops the turn in flight and then starts the new one. The kill is what wakes the
    /// old turn's thread out of a read that has no timeout -- so the old thread runs its cleanup
    /// *after* the new turn has started, and with one untagged slot it was handed the new turn's
    /// child and killed it. A second question asked while the first was still running could silently
    /// kill the second.
    #[test]
    fn a_turn_that_has_been_overtaken_cannot_take_the_newer_turn_s_child() {
        let running = Running::default();
        let first = a_long_lived_child();
        let first_id = first.id();
        running.hold(1, first);

        // What `Client::ask` does: stop the turn in flight, then start the next one.
        running.stop(1);
        let second = a_long_lived_child();
        let second_id = second.id();
        running.hold(2, second);
        assert_ne!(first_id, second_id, "two different processes");

        // Now the old thread wakes up and asks for its child back, the way `run` does at the end of
        // its read loop. It must be told no.
        assert!(running.take(1).is_none(), "the overtaken turn is not handed the new turn's child");
        running.stop(1);

        // And the new turn's child is still there, and still alive.
        let mut child = running.take(2).expect("the newer turn still holds its own child");
        assert_eq!(child.id(), second_id);
        assert!(
            child.try_wait().expect("it can be asked").is_none(),
            "the newer turn's child is still running"
        );
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn a_program_is_found_the_way_a_shell_finds_one() {
        // A name with a separator is a path; a bare name is looked for on `PATH`, with `PATHEXT`
        // tried on Windows because `claude` is a `.cmd` there.
        let here = crate::Environment::of_this_process();
        assert!(crate::provider::program("", &here).is_none());
        assert!(crate::provider::program("unluminous-no-such-agent-anywhere", &here).is_none());
        assert!(crate::provider::program("/definitely/not/here/claude", &here).is_none());
        let folder =
            std::env::temp_dir().join(format!("unluminous-chat-program-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        let name = match cfg!(windows) {
            true => "unluminous-chat-test-agent.exe",
            false => "unluminous-chat-test-agent",
        };
        let made = folder.join(name);
        std::fs::write(&made, b"").expect("a file that stands in for a program");
        assert_eq!(
            crate::provider::program(&made.to_string_lossy(), &here),
            Some(made.clone()),
            "a path is taken as one"
        );
        let _ = std::fs::remove_file(&made);
    }

    #[test]
    fn an_agent_that_is_not_installed_says_so_before_anything_is_run() {
        let mut provider = provider(Wire::ClaudeCli);
        provider.command = "unluminous-no-such-agent-anywhere".to_owned();
        let here = crate::Environment::of_this_process();
        let why = provider.why_not(&here).expect("a refusal");
        assert!(why.contains("unluminous-no-such-agent-anywhere"), "{why}");
        assert!(why.contains("PATH"), "{why}");
        // And it never asks for a key, whatever a settings file says.
        provider.key_env = "ANTHROPIC_API_KEY".to_owned();
        assert!(!provider.wants_a_key());
        assert!(provider.headers(&here).iter().all(|(name, _)| name == "content-type"));
    }

    pub(super) fn feed(decoder: &mut Decoder, lines: &[&str]) -> Vec<Reply> {
        let mut out = Vec::new();
        for line in lines {
            out.extend(decoder.line(line));
        }
        out
    }
}

#[cfg(test)]
mod only_the_new_part {
    use super::*;

    /// Codex sends a snapshot per update, and only what is new reaches the pane.
    ///
    /// **`task-1984` P17.** `rest_of` cloned the whole already-sent text to compare against and then
    /// cloned the whole new snapshot to record -- so an answer of a hundred kilobytes arriving in a
    /// thousand updates copied about a hundred megabytes to report a hundred kilobytes. What is kept
    /// is grown in place now, and what has to stay true is the answer: each update reports the part
    /// that is new and nothing else, and an update that says the same thing twice reports nothing.
    #[test]
    fn each_update_reports_the_part_that_is_new_and_nothing_else() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        let mut so_far = String::new();
        for word in ["A model ", "writing ", "one word ", "at a time."] {
            so_far.push_str(word);
            assert_eq!(decoder.rest_of("item-1", &so_far).as_deref(), Some(word));
        }
        assert_eq!(
            decoder.rest_of("item-1", &so_far),
            None,
            "the same snapshot twice says nothing"
        );
        // A second item is its own conversation, and the first is not disturbed by it.
        assert_eq!(decoder.rest_of("item-2", "elsewhere").as_deref(), Some("elsewhere"));
        so_far.push_str(" And more.");
        assert_eq!(decoder.rest_of("item-1", &so_far).as_deref(), Some(" And more."));
    }

    /// A snapshot that is not a continuation of the last one is sent whole.
    ///
    /// `task-1984` P17 kept the rule the comment above `rest_of` already stated and that the
    /// rewrite could have lost: the already-sent text is compared as a **prefix**, not as a length.
    /// A revision, a shorter value or a reused id has nothing to do with what was sent, so the whole
    /// of the new one goes out rather than a slice out of its middle -- which could otherwise land
    /// inside a character.
    #[test]
    fn a_snapshot_that_does_not_carry_on_from_the_last_is_sent_whole() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        assert_eq!(
            decoder.rest_of("item", "the first answer").as_deref(),
            Some("the first answer")
        );
        // Shorter, and not a prefix of what was sent.
        assert_eq!(decoder.rest_of("item", "a revision").as_deref(), Some("a revision"));
        // And carrying on from the revision works from there rather than from what came before it.
        assert_eq!(decoder.rest_of("item", "a revision, longer").as_deref(), Some(", longer"));
    }

    /// Multi byte text is never split in the middle of a character.
    ///
    /// `task-1984` P17. The split is at a prefix boundary, which is a character boundary by
    /// construction — but the rewrite slices by a length it works out itself, so this is the thing
    /// that would panic if the arithmetic were wrong.
    #[test]
    fn a_snapshot_of_multi_byte_text_is_never_split_inside_a_character() {
        let mut decoder = Decoder::new(Wire::CodexCli);
        let whole = "\u{4f60}\u{597d}\u{1f600} a mix of widths";
        let mut so_far = String::new();
        for character in whole.chars() {
            so_far.push(character);
            let rest = decoder.rest_of("item", &so_far).expect("one character is new");
            assert_eq!(rest, character.to_string(), "after {so_far:?}");
        }
    }

    /// `task-2235`: the Chat settings' context ring reads what Claude Code says about the context, and
    /// the window is the one of the model that carried the conversation rather than a helper's.
    #[test]
    fn claude_code_says_how_much_context_is_used_and_how_large_it_is() {
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        decoder.line(r#"{"type":"stream_event","event":{"type":"message_start","message":{"model":"m","usage":{"input_tokens":2,"cache_read_input_tokens":70000,"cache_creation_input_tokens":6000}}}}"#);
        let replies = decoder.line(r#"{"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":2,"output_tokens":7},"modelUsage":{"small":{"inputTokens":900,"contextWindow":200000},"big":{"inputTokens":2,"cacheReadInputTokens":70000,"contextWindow":1000000}}}"#);
        assert_eq!(
            replies.last(),
            Some(&Reply::Context { used: Some(76002), window: Some(1_000_000) })
        );
    }

    #[test]
    fn a_session_claude_code_does_not_have_is_reported_in_its_own_words() {
        // The line Claude Code 2.1.302 printed for `--resume` with an id it never issued: `result` is absent
        // and the reason is in `errors` (`task-2285`).
        let mut decoder = Decoder::new(Wire::ClaudeCli);
        let replies = super::tests::feed(
            &mut decoder,
            &[
                r#"{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0,"session_id":"1111","usage":{"input_tokens":0,"output_tokens":0},"errors":["No conversation found with session ID: 1111"]}"#,
            ],
        );
        let failed = replies.iter().find_map(|reply| match reply {
            Reply::Failed(why) => Some(why.clone()),
            _ => None,
        });
        assert_eq!(failed.as_deref(), Some("No conversation found with session ID: 1111"));
        assert!(refused_to_resume(&failed.unwrap_or_default()));
    }

    #[test]
    fn each_agents_refusal_to_resume_is_recognised_and_nothing_else_is() {
        // Recorded from the three agents on this machine, asked to resume an id they never issued.
        for refusal in [
            "No conversation found with session ID: 11111111-2222-3333-4444-555555555555",
            "codex stopped with exit code: 1.\n\nError: thread/resume: thread/resume failed: no rollout found for thread id 01a00000-0000-7000-8000-000000000000 (code -32600)",
            "opencode stopped with exit code: 1.\n\n\u{1b}[91m\u{1b}[1mError: \u{1b}[0mSession not found",
            "pi stopped with exit code: 1.\n\nNo session found matching '00000000-dead-beef-0000-000000000000'",
        ] {
            assert!(refused_to_resume(refusal), "{refusal}");
        }
        for other in [
            "HTTP 500: the server is overloaded",
            "claude could not be started: program not found",
            "the agent stopped without finishing its answer.",
            "Error: file not found",
        ] {
            assert!(!refused_to_resume(other), "{other}");
        }
    }

    #[test]
    fn a_codex_error_item_is_a_warning_and_the_turn_still_finishes() {
        // The lines codex-cli 0.153.4 wrote for one ordinary turn on this machine (`task-2285`).
        let mut decoder = Decoder::new(Wire::CodexCli);
        let replies = super::tests::feed(
            &mut decoder,
            &[
                r#"{"type":"thread.started","thread_id":"01a148bb-16b6-7223-838d-4ace4749305b"}"#,
                r#"{"type":"turn.started"}"#,
                r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Skill descriptions were shortened to fit the skills context budget."}}"#,
                r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"OK"}}"#,
                r#"{"type":"turn.completed","usage":{"input_tokens":28697,"cached_input_tokens":17152,"output_tokens":5}}"#,
            ],
        );
        assert!(!replies.iter().any(|reply| matches!(reply, Reply::Failed(_))), "{replies:?}");
        assert!(replies.contains(&Reply::Text("OK".to_owned())), "{replies:?}");
        assert!(replies.iter().any(
            |reply| matches!(reply, Reply::Thinking(said) if said.contains("Skill descriptions"))
        ));
        assert!(replies.iter().any(|reply| matches!(reply, Reply::Finished { .. })));
    }
}
