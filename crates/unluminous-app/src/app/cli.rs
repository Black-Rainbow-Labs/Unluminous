//! What a command line command means.
//!
//! `UnluminousApp::run_action` is the one place a menu entry turns into a change, and this is the same
//! rule for the command line: [`UnluminousApp::run_cli`] is the one place a request turns into a change.
//! A command reaches it from `services::control`, which read it off a socket, and never from
//! anywhere else.
//!
//! ## Where the work is, and where it is not
//!
//! Nothing here decides what a command is called or what it takes — `unluminous_cli::catalogue` does,
//! and the client parses against the same list, so a command that the CLI will accept is a command
//! the window knows. Nothing here draws, either. Every arm either reads the window's state into a
//! reply or asks the window to change in exactly the way a menu entry or a click would, so a thing
//! done from the command line and the same thing done by hand are the same thing.
//!
//! Wherever there is already a way in, it is used: `run_action` for anything on a menu,
//! `open_path`, `save`, `set_settings`, `set_font_size`, `FileTree::expand`, `Document::apply`. The
//! commands that have no menu entry behind them — reading the text, moving the caret, typing into a
//! modal — are the ones that do anything of their own here.
//!
//! ## Answers that cannot be given at once
//!
//! Five commands are asked on one frame and answered on a later one: a screenshot, because the
//! picture of a frame arrives after that frame has been painted; `terminal read --wait-for`,
//! because it is waiting for a shell; `modal results --wait`, because `Find in Files` reads the
//! project on a thread; `git status` after repository identity changes; and `git action --wait`,
//! because git runs on a thread too. Each one keeps
//! its request in [`Waiting`] and answers it when it is ready or when its time runs out. That is
//! also why every one of them takes a timeout: a command that could wait for ever is a script that
//! hangs.
//!
//! ## Paths
//!
//! One rule, everywhere: **a relative path is relative to the project folder**. Not to wherever the
//! client happened to be run from — that would make the same command mean different things in two
//! terminals, and an agent's working directory is rarely the thing it is editing. A path that must
//! be somewhere else is given in full. Every reply says which absolute path it used, so there is
//! never any doubt about where a file went.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Rect, ViewportCommand};
use serde_json::{json, Map, Value};

use unluminous_cli::protocol::{code, Reply, Request};

use crate::app::actions::{Action, DebugAction, GitAction, HighlightColor, RunAction};
use crate::app::debug::DebugState;
use crate::app::dock;
use crate::app::{UnluminousApp, ViewMode};
use crate::services::debuggers;
use unluminous_core::symbols::Role;

use crate::components::find_in_files::FindInFiles;
use crate::components::go_to_file::GoToFile;
use crate::components::modal;
use crate::components::prompt_dialog::{Prompt, Purpose};
use crate::components::references::{self, References};
use crate::components::status_bar;
use crate::services::browser::BrowserCommand;
use crate::services::control::Pending;
use crate::services::file_kind;
use crate::services::run_configurations::{self, Configuration, Origin};
use crate::settings;
use crate::theme::size;

// The fourteen files this module is split into, one per catalogue area or small group of related
// areas -- `unluminous-cli/src/catalogue.rs` is the list, and `run_cli` below is the whole of the
// dispatch. Declared as submodules of `cli` itself, in a `cli/` folder beside this file, rather than
// as siblings of `cli` in `app/mod.rs`: a private item here (`ok`, `no`, `Outcome`, `Waiting`,
// `DEFAULT_WAIT`, `cli_path`, `cli_offset`, `view_mode_name`...) is visible to a descendant module
// with no change at all, which is what lets every one of them keep `use super::*;` as its only import
// from this file. `space.rs` is the one area that already lived in a file of its own before this
// split, as a sibling of `cli` rather than a child of it, which is why its own dispatch entry,
// `cli_space`, was already `pub(crate)` -- the same reason every dispatch entry below needs it.
mod cli_action;
mod cli_debug;
mod cli_editor;
mod cli_explorer;
mod cli_git;
mod cli_highlight;
mod cli_modal;
mod cli_panel;
mod cli_plugins;
mod cli_run;
mod cli_settings;
mod cli_space;
mod cli_tab;
mod cli_terminal;
mod cli_window;

/// How long a command waits when it was not told.
const DEFAULT_WAIT: Duration = Duration::from_millis(10_000);
/// What running a command produced.
pub enum Outcome {
    /// Answer it now.
    Reply(Reply),
    /// Keep it, and answer it when [`Waiting`] says it is ready.
    Hold(Waiting),
}

/// Which reply a held git refresh owes its caller once the worker has answered.
pub enum GitAnswer {
    GitStatus,
    WindowStatus,
}

/// A request that has been accepted and is waiting for something.
pub enum Waiting {
    /// A picture of the window, once it has stopped moving and one has been painted.
    ///
    /// `settle` counts the frames still to be drawn before the picture is asked for. It exists
    /// because a screenshot taken on the frame a command lands catches the window **mid animation**:
    /// egui fades a modal and its backdrop in over about a twelfth of a second, and the first
    /// picture of a newly opened `Settings` showed the editor's text through it, half faded. That is
    /// what the window really looked like at that instant, and it is not what anybody wanted to be
    /// shown. Settling first costs a quarter of a second and makes the picture the answer to "what
    /// does it look like now" rather than "what did it look like on the way there".
    /// A picture of the window, or of one rectangle of it.
    ///
    /// `crop` is `task-1904`'s: *"see screenshots"* of a browser node means a picture of that node
    /// rather than of the whole window, and cutting the window's own picture is the honest way to
    /// take one - a native child view is composited by the operating system, so nothing inside
    /// Unluminous can render it on its own.
    Screenshot { path: PathBuf, until: Instant, settled: Instant, asked: bool, crop: Option<Rect> },
    /// Input that has been queued and not yet reached a frame.
    ///
    /// **A count rather than a flag**, which is the shape `DebugState::reads` already has: two `input`
    /// commands in flight would each see the other's empty queue and answer for a gesture that was not
    /// theirs. `settle` is the frames to draw *after* the last step, so a screenshot taken straight after
    /// an `input click` is a picture of the window after the click rather than during it - which is the
    /// same reason `Waiting::Screenshot` settles.
    Input { target: u64, settle: u8, until: Instant },
    /// Some text on a terminal tab's screen.
    ///
    /// `tab` is the tab the wait was asked about, resolved to a number when the request arrived,
    /// and it is what the wait keeps looking at: a hold that started on one tab is answered by that
    /// tab and by nothing else, because a promise kept by looking at a different screen is not a
    /// promise. Following the tab that is showing while the wait runs is the race `task-1705` is
    /// about, so the number is fixed rather than re-asked on every frame.
    TerminalText { tab: usize, needle: String, lines: Option<usize>, until: Instant },
    /// Some text a run has written, which is how an agent waits for a dev server to say it is
    /// listening before it uses it.
    RunOutput { name: Option<String>, needle: String, tail: Option<usize>, until: Instant },
    /// A search that is still running.
    ModalResults { limit: usize, until: Instant },
    /// The references search, which runs on the same kind of thread.
    ///
    /// It carries what is to be done with the answer, because `editor references` and
    /// `editor rename` are the same search asked for two reasons and a second waiting variant
    /// would be a second place to get the cancellation right.
    References { until: Instant, code_only: bool, rename: Option<CliRename> },
    /// Git, which is on a thread of its own.
    Git { until: Instant, answer: GitAnswer },
    /// A debug session stopping somewhere, which is what makes `debug start --wait-for-pause` and
    /// the four stepping verbs answerable from a script.
    ///
    /// **This is the sequence the whole feature is an acceptance test of**: set a breakpoint, start,
    /// wait for the stop and read a variable — four commands, and an agent driving Unluminous can then
    /// observe a program's actual state instead of reasoning about it.
    DebugPause { command: String, until: Instant },
    /// An expression the debugger is still evaluating.
    ///
    /// `evaluate` is a request like any other, so its answer arrives on a later frame — and a
    /// command that reported the question rather than the answer would be useless to the one caller
    /// it exists for. The id is what the session labelled the question with, so an answer that lands
    /// after another was asked cannot be mistaken for this one.
    DebugEvaluate { expression: String, id: u64, until: Instant },
    /// `debug hover` waiting for the tree the value tooltip shows. `task-1696`.
    ///
    /// Its own kind rather than [`Waiting::DebugEvaluate`], because what it waits for is not the
    /// same thing: an `evaluate` answer is one round trip and a *tree* is that plus the `variables`
    /// its children come from, which is what `DebugState::hover_is_ready` counts.
    DebugHover { id: u64, expand: Option<String>, until: Instant },
    /// `update check` waiting for the releases endpoint, which is on a thread. `task-1984` L1.
    UpdateCheck { until: Instant },
}

/// Who asked for a tool call to be run, which is who its answer goes back to.
pub(crate) enum ToolCaller {
    /// A plugin's pane or tab, by the plugin's id.
    Plugin(String),
    /// A chat node on the canvas, which holds a chat of its own.
    Node(crate::services::space::NodeId),
}

/// A tool call whose command answers on a later frame. `task-2096`.
///
/// The tool call's own form of a `(Pending, Waiting)` in `cli_waiting`: the same [`Waiting`] and the
/// same readiness check, with the caller and the call's `id` in place of a socket to answer down.
pub(crate) struct HeldToolCall {
    caller: ToolCaller,
    id: String,
    request: Request,
    waiting: Waiting,
}

/// A rename a command asked for, waiting for the search that will find what it changes.
pub struct CliRename {
    to: String,
    /// `Some(true)` for this file, `Some(false)` for the project, `None` for whatever the name
    /// resolves to — which is the modal's own default.
    scope: Option<bool>,
    /// `comments`, `strings`, or neither, which is the default: they are textual matches.
    include: Vec<String>,
    /// False when the change set is only to be printed.
    apply: bool,
}

impl Waiting {
    fn until(&self) -> Instant {
        match self {
            Waiting::Screenshot { until, .. }
            | Waiting::Input { until, .. }
            | Waiting::TerminalText { until, .. }
            | Waiting::RunOutput { until, .. }
            | Waiting::ModalResults { until, .. }
            | Waiting::References { until, .. }
            | Waiting::Git { until, .. }
            | Waiting::UpdateCheck { until }
            | Waiting::DebugPause { until, .. }
            | Waiting::DebugEvaluate { until, .. }
            | Waiting::DebugHover { until, .. } => *until,
        }
    }
}

/// Answer it now, with a sentence and some data.
pub(crate) fn ok(request: &Request, message: impl Into<String>, result: Value) -> Outcome {
    Outcome::Reply(Reply::done(&request.command, message, result))
}

/// Refuse it, with a code a caller can match on and a sentence a person can read.
pub(crate) fn no(request: &Request, code: &str, message: impl Into<String>) -> Outcome {
    Outcome::Reply(Reply::failed(&request.command, code, message))
}

/// A value the command has no name for is refused, rather than being dropped and the command run as
/// though it had not been sent.
///
/// A caller that misspells a name gets a reply saying so instead of a success that did something
/// other than what was asked. `run output --tail 40` written with the dashes intact used to return
/// the whole screen and say it had worked, and `tab open --permanent` used to leave the tab
/// transient — both of them silently, which is the fault this closes. The dashes themselves are not
/// what makes a name unknown: they are taken off when the request is read.
///
/// A command this version does not have is left alone, because the answer to that is the sentence
/// naming the command, which the area dispatch below already writes.
fn unknown_argument_refusal(request: &Request) -> Option<Outcome> {
    let command = unluminous_cli::catalogue::find(&request.command)?;
    let unknown = unluminous_cli::catalogue::unknown_arguments(command, &request.arguments);
    if unknown.is_empty() {
        return None;
    }
    let takes = unluminous_cli::catalogue::value_names(command);
    let says = match takes.is_empty() {
        true => format!("{} takes no values.", command.typed()),
        false => format!("{} takes {}.", command.typed(), takes.join(", ")),
    };
    let hints = unknown
        .iter()
        .filter_map(|name| unluminous_cli::catalogue::argument_hint(command, name))
        .map(|hint| format!("Try {hint} for {}.", either(&unknown)))
        .collect::<Vec<_>>();
    let hint = if hints.is_empty() { String::new() } else { format!(" {}", hints.join(" ")) };
    Some(no(
        request,
        code::USAGE,
        format!("{} has no {}.{hint} {says}", command.typed(), either(&unknown)),
    ))
}

/// A value of a kind the command cannot use is refused, rather than being dropped and the command
/// run as though it had not been sent.
///
/// The other half of [`unknown_argument_refusal`], and the same fault seen from the other side: that
/// one is about a name the command does not have, and this is about a name it has whose **value** it
/// cannot read. `Request::number` answers `None` for an absent key and for an unusable one alike, so
/// `window size --width nonsense` set no width and reported success, and so did `editor caret --line
/// nonsense`, `explorer width nonsense` and `explorer tree --limit nonsense`. `task-1922` B13
/// measured six of them.
///
/// Which names hold a number is [`unluminous_cli::catalogue::Kind`], written down beside each
/// argument and flag, so this refuses every one of them without knowing what any command does — and
/// a name marked as a number later is covered the day it is marked.
fn wrong_number_refusal(request: &Request) -> Option<Outcome> {
    let command = unluminous_cli::catalogue::find(&request.command)?;
    let wrong = unluminous_cli::catalogue::wrong_numbers(command, &request.arguments);
    if wrong.is_empty() {
        return None;
    }
    let said: Vec<String> = wrong
        .iter()
        .map(|(name, given)| {
            let kind = unluminous_cli::catalogue::kind_of(command, name)
                .map(|kind| kind.named())
                .unwrap_or("a number");
            format!("{} takes {kind} for {name}, and it was given `{given}`.", command.typed())
        })
        .collect();
    Some(no(request, code::USAGE, said.join(" ")))
}

/// `a`, `a or b`, `a, b or c` — one name read as a name and three read as a list.
fn either(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} or {second}"),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

/// Nothing but a sentence, which is what most commands that change something answer with.
pub(crate) fn done(request: &Request, message: impl Into<String>) -> Outcome {
    ok(request, message, Value::Null)
}

/// A reply whose data is a list of lines the client prints as they are.
pub(crate) fn lines(
    request: &Request,
    message: impl Into<String>,
    lines: Vec<String>,
    extra: Value,
) -> Outcome {
    let mut result = match extra {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    result.insert("lines".to_owned(), json!(lines));
    ok(request, message, Value::Object(result))
}

impl UnluminousApp {
    /// Take everything the command channel has, run it, and answer whatever is now ready.
    ///
    /// Called once at the top of a frame, before anything is drawn, so that a command's effect is in
    /// the frame that is about to be painted and therefore in the next screenshot.
    pub fn pump_control(&mut self, ctx: &egui::Context) {
        self.finish_waiting(ctx);
        let arrived = match &self.control {
            Some(server) => server.take(),
            None => Vec::new(),
        };
        if !arrived.is_empty() {
            // A window whose project has terminals restores them at the end of its first frame, so
            // that it appears before it starts a shell. A command that arrives before that has
            // happened has to see the finished window rather than a half-restored one — `terminal
            // list` would otherwise answer that there were none. Taking the list is idempotent, so
            // this costs one comparison on every later frame.
            self.start_the_restored_terminals();
        }
        for pending in arrived {
            // A request whose caller gave up before the window ever picked it up is thrown away
            // rather than run. It has already been told the command did not happen — `task-1691`
            // measured three that reported a timeout and had been applied — and an agent that
            // retries after a timeout would otherwise apply the command twice.
            if pending.was_abandoned() {
                continue;
            }
            let outcome = self.run_cli(&pending.request, ctx);
            self.settle(pending, outcome);
        }
        if !self.cli_waiting.is_empty() || !self.tool_waiting.is_empty() {
            // Something is being waited for, so keep drawing: a window that has gone to sleep is a
            // request that is never answered.
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }

    /// Answer a request now, or keep it.
    fn settle(&mut self, pending: Pending, outcome: Outcome) {
        match outcome {
            Outcome::Reply(reply) => pending.answer(reply),
            Outcome::Hold(waiting) => self.cli_waiting.push((pending, waiting)),
        }
    }

    /// Look at everything that is waiting, and answer what is ready or out of time.
    fn finish_waiting(&mut self, ctx: &egui::Context) {
        if self.cli_waiting.is_empty() && self.tool_waiting.is_empty() {
            return;
        }
        let picture = screenshot_from(ctx);
        let mut still_waiting = Vec::new();
        for (pending, mut waiting) in std::mem::take(&mut self.cli_waiting) {
            match self.ready(&pending.request, &mut waiting, ctx, picture.as_ref()) {
                Some(reply) => pending.answer(reply),
                None if Instant::now() >= waiting.until() => {
                    pending.answer(self.timed_out(&waiting))
                }
                None => still_waiting.push((pending, waiting)),
            }
        }
        self.cli_waiting = still_waiting;
        self.finish_the_waiting_tool_calls(ctx, picture.as_ref());
    }

    /// The same for the tool calls a chat is waiting on, answered back to the chat that asked.
    ///
    /// One picture a frame serves both lists: egui hands a screenshot to the frame after it was
    /// asked for, and a command line request and a tool call waiting on the same frame are both
    /// answered by it.
    fn finish_the_waiting_tool_calls(
        &mut self,
        ctx: &egui::Context,
        picture: Option<&egui::ColorImage>,
    ) {
        let mut still_waiting = Vec::new();
        for mut held in std::mem::take(&mut self.tool_waiting) {
            let reply = match self.ready(&held.request, &mut held.waiting, ctx, picture) {
                Some(reply) => reply,
                None if Instant::now() >= held.waiting.until() => self.timed_out(&held.waiting),
                None => {
                    still_waiting.push(held);
                    continue;
                }
            };
            self.answer_a_tool_call(&held.caller, &held.id, reply);
        }
        // Anything held while those answers were given is kept too, rather than overwritten.
        still_waiting.append(&mut self.tool_waiting);
        self.tool_waiting = still_waiting;
    }

    /// The reply for something that was waiting, if it is ready.
    fn ready(
        &mut self,
        request: &Request,
        waiting: &mut Waiting,
        ctx: &egui::Context,
        picture: Option<&egui::ColorImage>,
    ) -> Option<Reply> {
        match waiting {
            Waiting::Screenshot { path, settled, asked, crop, .. } => {
                if !*asked {
                    if Instant::now() < *settled {
                        ctx.request_repaint();
                        return None;
                    }
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                    ctx.request_repaint();
                    *asked = true;
                    return None;
                }
                let picture = picture?;
                let cut;
                let picture = match crop {
                    Some(area) => {
                        cut = cut_out(picture, *area, ctx.pixels_per_point());
                        &cut
                    }
                    None => picture,
                };
                Some(match write_png(picture, path) {
                    Ok(()) => Reply::done(
                        &request.command,
                        format!("Wrote {}", path.display()),
                        json!({
                            "path": path.to_string_lossy(),
                            "width": picture.size[0],
                            "height": picture.size[1],
                        }),
                    ),
                    Err(problem) => Reply::failed(
                        &request.command,
                        code::FAILED,
                        format!("Could not write {}: {problem}", path.display()),
                    ),
                })
            }
            Waiting::Input { target, settle, .. } => {
                if self.input.fed() < *target {
                    ctx.request_repaint();
                    return None;
                }
                if *settle > 0 {
                    *settle -= 1;
                    ctx.request_repaint();
                    return None;
                }
                Some(Reply::done(&request.command, "Done.", json!({ "frames": *target })))
            }
            Waiting::TerminalText { tab, needle, lines, .. } => {
                let screen = self.terminal_text(*tab, *lines)?;
                screen.contains(needle.as_str()).then(|| {
                    Reply::done(
                        &request.command,
                        format!("Found {needle} on the terminal"),
                        json!({ "text": screen, "waitedFor": needle, "found": true }),
                    )
                })
            }
            Waiting::RunOutput { name, needle, tail, .. } => {
                // Pumped first, so a read straight after a start is not looking at a screen the
                // program has already written past.
                self.run.settle();
                let output = self.run_output(name.as_deref(), *tail)?;
                output.contains(needle.as_str()).then(|| {
                    Reply::done(
                        &request.command,
                        format!("Found {needle} in the run's output"),
                        json!({ "text": output, "waitedFor": needle, "found": true }),
                    )
                })
            }
            Waiting::ModalResults { limit, .. } => {
                let find = self.find_in_files.as_ref()?;
                (!find.is_searching()).then(|| self.modal_results_reply(request, *limit))
            }
            Waiting::UpdateCheck { .. } => {
                let check = self.update.as_ref()?;
                // `take_the_update_answer` is what reads the channel, once a frame, beside the git
                // replies -- so by the time this is asked the answer is on the window.
                (!check.is_asking()).then(|| self.update_check_reply(request))
            }
            Waiting::References { .. } => {
                let modal = self.references.as_ref()?;
                (!modal.is_searching()).then(|| self.references_reply(request, waiting))
            }
            Waiting::Git { answer, .. } => {
                let git = self.git.as_ref()?;
                (!git.is_busy()).then(|| match answer {
                    GitAnswer::GitStatus => self.git_status_reply(request),
                    GitAnswer::WindowStatus => self.window_status_reply(request, ctx),
                })
            }
            Waiting::DebugEvaluate { expression, id, .. } => {
                let debug = self.debug.as_ref()?;
                let (asked, _, answer) = debug.evaluated.as_ref()?;
                if asked != id {
                    return None;
                }
                // A session that ended is an answer too: nothing is going to evaluate anything now.
                let answered = answer.clone().or_else(|| {
                    (!debug.is_alive())
                        .then(|| Err("The session ended before it answered.".to_owned()))
                })?;
                Some(match answered {
                    Ok(value) => Reply::done(
                        &request.command,
                        format!("{expression} = {}", value.value),
                        json!({
                            "expression": expression,
                            "value": value.value,
                            "type": value.kind,
                            "expandable": value.reference != 0,
                        }),
                    ),
                    // The debugger's own refusal, shown as it was written: it explains a bad
                    // expression far better than Unluminous could.
                    Err(problem) => Reply::failed(&request.command, code::NOT_APPLICABLE, problem),
                })
            }
            Waiting::DebugHover { id, expand, .. } => {
                let id = *id;
                {
                    let debug = self.debug.as_ref()?;
                    let hover = debug.hover.as_ref()?;
                    if hover.id != id {
                        return None;
                    }
                    if !debug.hover_is_ready() {
                        // A session that ended is an answer too: nothing is going to evaluate
                        // anything now, and saying so at once is what stops a script waiting out
                        // the timeout.
                        if debug.is_alive() {
                            return None;
                        }
                        return Some(Reply::failed(
                            &request.command,
                            code::NOT_APPLICABLE,
                            "The session ended before it answered.",
                        ));
                    }
                }
                // **A row cannot be opened before there is a tree to open it in.** `--expand` on a
                // question that had to be asked is applied here, once the answer is in, and then the
                // wait goes round again for the children — which is what makes one command enough
                // where two round trips are needed. A row the expression does not have is a toggle
                // that does nothing, and the next pass answers.
                if let Some(key) = expand.take() {
                    if let Some(debug) = self.debug.as_mut() {
                        debug.toggle_hover_row(&key);
                    }
                    return None;
                }
                Some(self.hover_reply(request))
            }
            Waiting::DebugPause { command, .. } => {
                // A build that has to finish first is not an answer yet; a build that failed is, and
                // saying so at once is what stops a caller waiting out a timeout for a session that
                // is never going to exist.
                if self.debug.is_none() {
                    if self.debug_build.is_some() {
                        return None;
                    }
                    let said =
                        self.message.clone().unwrap_or_else(|| "Nothing was started.".to_owned());
                    return Some(Reply::failed(command, code::NOT_APPLICABLE, said));
                }
                let debug = self.debug.as_ref()?;
                // An ended session is an answer too, and a better one than waiting out the timeout:
                // a program that ran to completion is never going to stop, and saying so at once is
                // what stops a script sitting there for thirty seconds.
                (debug.is_ready() || !debug.is_alive()).then(|| self.debug_status_reply(request))
            }
        }
    }

    /// What to say when the time ran out.
    fn timed_out(&mut self, waiting: &Waiting) -> Reply {
        let (command, message) = match waiting {
            Waiting::Screenshot { path, .. } => (
                "window.screenshot",
                format!(
                    "The window did not paint a frame to capture, so nothing was written to {}.",
                    path.display()
                ),
            ),
            Waiting::Input { .. } => (
                "input",
                "The window did not draw the frames the input needed, so it may not all have arrived."
                    .to_owned(),
            ),
            Waiting::TerminalText { tab, needle, lines, .. } => {
                let screen = self.terminal_text(*tab, *lines).unwrap_or_default();
                return Reply {
                    ok: false,
                    command: "terminal.read".to_owned(),
                    message: format!("{needle} did not appear on the terminal in time."),
                    result: json!({ "text": screen, "waitedFor": needle, "found": false }),
                    error: Some(unluminous_cli::protocol::Failure {
                        code: code::TIMED_OUT.to_owned(),
                        message: format!("{needle} did not appear on the terminal in time."),
                    }),
                };
            }
            Waiting::RunOutput { name, needle, tail, .. } => {
                let output = self.run_output(name.as_deref(), *tail).unwrap_or_default();
                return Reply {
                    ok: false,
                    command: "run.output".to_owned(),
                    message: format!("{needle} was not written in time."),
                    result: json!({ "text": output, "waitedFor": needle, "found": false }),
                    error: Some(unluminous_cli::protocol::Failure {
                        code: code::TIMED_OUT.to_owned(),
                        message: format!("{needle} was not written in time."),
                    }),
                };
            }
            Waiting::ModalResults { .. } => {
                ("modal.results", "The search was still running when the time ran out.".to_owned())
            }
            Waiting::UpdateCheck { .. } => (
                "update.check",
                "The releases page had not answered when the time ran out.".to_owned(),
            ),
            Waiting::References { rename, .. } => (
                if rename.is_some() { "editor.rename" } else { "editor.references" },
                "The search was still running when the time ran out, so nothing was changed."
                    .to_owned(),
            ),
            Waiting::Git { answer, .. } => {
                let command = match answer {
                    GitAnswer::GitStatus => "git.status",
                    GitAnswer::WindowStatus => "status",
                };
                (command, "Git was still running when the time ran out.".to_owned())
            }
            Waiting::DebugEvaluate { expression, .. } => {
                let expression = expression.clone();
                return Reply::failed(
                    "debug.evaluate",
                    code::TIMED_OUT,
                    format!("The debugger did not answer {expression} in time."),
                );
            }
            Waiting::DebugHover { .. } => {
                let said = self
                    .debug
                    .as_ref()
                    .and_then(|debug| debug.hover.as_ref())
                    .map(|hover| format!("The debugger did not answer {} in time.", hover.expression))
                    .unwrap_or_else(|| "The debugger did not answer in time.".to_owned());
                return Reply::failed("debug.hover", code::TIMED_OUT, said);
            }
            Waiting::DebugPause { command, .. } => {
                let command = command.clone();
                return Reply {
                    ok: false,
                    command: command.clone(),
                    message: "The program did not stop in time.".to_owned(),
                    result: self.debug_state_value(),
                    error: Some(unluminous_cli::protocol::Failure {
                        code: code::TIMED_OUT.to_owned(),
                        message: "The program did not stop in time.".to_owned(),
                    }),
                };
            }
        };
        Reply::failed(command, code::TIMED_OUT, message)
    }

    /// Run a whole command line, the way `unluminous-cli` would, and take the answer.
    ///
    /// `line` is what somebody types after the word `unluminous-cli`. It is parsed against the same
    /// catalogue the client parses against and run through the same [`Self::run_cli`], so a test
    /// driving Unluminous this way goes down the whole command line path apart from the socket — the
    /// parser, the argument names, the dispatch and the reply are all the real ones.
    ///
    /// `None` for a command that is answered on a later frame: a screenshot, or one of the three
    /// waits. Those need the frame loop, so they belong to the running window rather than to a test
    /// that calls this and looks at the answer.
    pub fn run_command_line(&mut self, line: &str, ctx: &egui::Context) -> Option<Reply> {
        let words = split_line(line);
        let typed = match unluminous_cli::parse::parse(&words) {
            Ok(typed) => typed,
            Err(problem) => return Some(Reply::failed("", code::USAGE, problem.message)),
        };
        let Some(command) = typed.command else {
            return Some(Reply::failed("", code::USAGE, "no command"));
        };
        let request = Request::new("", &command.wire(), typed.arguments);
        match self.run_cli(&request, ctx) {
            Outcome::Reply(reply) => Some(reply),
            Outcome::Hold(_) => None,
        }
    }

    /// Run a request that has already been built, and take the answer.
    ///
    /// The same as [`Self::run_command_line`] for a caller that has a [`Request`] rather than a line
    /// of text — which is a test walking the whole catalogue and asking the window whether it knows
    /// each command. `None` when the answer belongs to a later frame.
    pub fn run_cli_for_test(&mut self, request: &Request, ctx: &egui::Context) -> Option<Reply> {
        match self.run_cli(request, ctx) {
            Outcome::Reply(reply) => Some(reply),
            Outcome::Hold(_) => None,
        }
    }

    /// Run one command on behalf of a plugin's tool call, and hand the answer back to whoever asked.
    ///
    /// **The same `run_cli` an `unluminous-cli` request goes down**, so a model calling a tool and a person
    /// pressing the same menu entry are the same thing rather than two paths that agree today. That is
    /// the whole reason `plugin_ui::Request::RunCommand` names a catalogue command rather than
    /// carrying a closure.
    ///
    /// A command that answers on a later frame — `window screenshot` waits for a frame to be painted —
    /// is kept in `tool_waiting` and answered by [`Self::finish_waiting`] exactly as a held command
    /// line request is. It used to be refused, which is `task-2096`'s *"waits for something to
    /// happen"*: a chat could not take a picture of the window it was in. Every held command has a
    /// deadline, so the call is answered either way, and the provider already matches an answer to
    /// its call by `id` because answers do not arrive in order. A call that *asks* to wait with a
    /// flag is still refused by `agent_chat::tools::resolve` before it reaches here.
    pub(crate) fn run_cli_for_a_plugin(
        &mut self,
        caller: ToolCaller,
        id: String,
        request: Request,
        ctx: &egui::Context,
    ) {
        match self.run_cli(&request, ctx) {
            Outcome::Reply(reply) => self.answer_a_tool_call(&caller, &id, reply),
            Outcome::Hold(waiting) => {
                ctx.request_repaint();
                self.tool_waiting.push(HeldToolCall { caller, id, request, waiting });
            }
        }
    }

    /// Give a tool call's answer to the plugin or the chat node that asked for it.
    fn answer_a_tool_call(&mut self, caller: &ToolCaller, id: &str, reply: Reply) {
        let answer = match reply.ok {
            true => Ok(match reply.result.is_null() {
                // A command that changed something and returned no data still said a sentence, and the
                // sentence is what a model needs to read.
                true => serde_json::Value::String(reply.message),
                false => reply.result,
            }),
            false => Err(reply.message),
        };
        match caller {
            ToolCaller::Plugin(plugin) => {
                if let Some(provider) = self.plugin_ui.provider(plugin) {
                    provider.answered(id, answer);
                }
            }
            ToolCaller::Node(node) => {
                if let Some(chat) = self.space.live.chat_mut(*node) {
                    crate::services::plugin_ui::UiProvider::answered(chat, id, answer);
                }
            }
        }
    }

    /// Run one command. The single place a command line request turns into a change.
    fn run_cli(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        if let Some(refusal) = unknown_argument_refusal(request) {
            return refusal;
        }
        if let Some(refusal) = wrong_number_refusal(request) {
            return refusal;
        }
        let (area, verb) = match request.command.split_once('.') {
            Some((area, verb)) => (area, verb),
            None => ("", request.command.as_str()),
        };
        match area {
            "" => self.cli_top(request, verb, ctx),
            "window" => self.cli_window(request, verb, ctx),
            "input" => self.cli_input(request, verb),
            "browser" => self.cli_browser(request, verb),
            "tab" => self.cli_tab(request, verb),
            "pane" => self.cli_pane(request, verb),
            "editor" => self.cli_editor(request, verb, ctx),
            "highlight" => self.cli_highlight(request, verb),
            "fold" => self.cli_fold(request, verb),
            "panel" => self.cli_panel(request, verb),
            "space" => self.cli_space(request, verb, ctx),
            "terminal" => self.cli_terminal(request, verb),
            "run" => self.cli_run(request, verb),
            "debug" => self.cli_debug(request, verb),
            "explorer" => self.cli_explorer(request, verb),
            "modal" => self.cli_modal(request, verb, ctx),
            "settings" => self.cli_settings(request, verb),
            "theme" => self.cli_theme(request, verb),
            "background" => self.cli_background(request, verb),
            "plugins" => self.cli_plugins(request, verb),
            "git" => self.cli_git(request, verb),
            "action" => self.cli_action(request, verb, ctx),
            "project" => self.cli_project(request, verb),
            "mcp" => self.cli_mcp(request, verb),
            "update" => self.cli_update(request, verb),
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no command called {}.", request.command),
            ),
        }
    }

    /// A relative path is relative to the project. See the note at the top of this file.
    fn cli_path(&self, text: &str) -> PathBuf {
        let path = PathBuf::from(text);
        // `project_state::absolute`'s reason, at the other door a relative path comes in by: a
        // caller writes `src/report.rs`, and on Windows joining that gives a path with both
        // separators in it that some other program will later be handed. Normalised here so that a
        // path from the command line and a path from the explorer are the same path.
        unluminous_terminal::paths::native(&if path.is_absolute() {
            path
        } else {
            self.tree.root().join(path)
        })
    }

    /// The path argument called `name`, resolved.
    pub(crate) fn cli_path_argument(&self, request: &Request, name: &str) -> Option<PathBuf> {
        request.text(name).map(|text| self.cli_path(&text))
    }
}

/// Split a command line the way a shell would, honouring quotation marks.
///
/// A shell has already done this by the time `unluminous-cli` sees its arguments. Anything driving
/// [`UnluminousApp::run_command_line`] has not had a shell, so it is done here — and it is the same two
/// rules a shell keeps: whitespace separates, and a quoted run is one word however much whitespace
/// is in it.
fn split_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    // **An explicitly quoted empty string is a word.** Without this, `settings set terminal.shell ""`
    // arrived as two words and was refused with "Say a setting and a value" -- so a setting whose
    // documented meaning of *empty* is "whatever this machine's own default is" could be set from a
    // Settings page and never cleared from the command line. `terminal.shell`, `appearance.theme`,
    // `appearance.accent`, `appearance.icons`, `editor.exclude` and `mcp.areas` all say that about
    // themselves. `task-1804`.
    let mut quoted = false;
    for character in line.chars() {
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), c) => word.push(c),
            (None, c @ ('"' | '\'')) => {
                quote = Some(c);
                quoted = true;
            }
            (None, c) if c.is_whitespace() => {
                if !word.is_empty() || quoted {
                    out.push(std::mem::take(&mut word));
                    quoted = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if !word.is_empty() || quoted {
        out.push(word);
    }
    out
}

/// The screenshot in this frame's input, if one arrived.
fn screenshot_from(ctx: &egui::Context) -> Option<egui::ColorImage> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Screenshot { image, .. } => Some((**image).clone()),
            _ => None,
        })
    })
}

/// Write a captured frame to a PNG.
/// The part of a picture inside `area`, which is in **points** while a picture is in pixels.
///
/// One function rather than the arithmetic at the call site, because the two units are exactly the
/// thing that is easy to get wrong: a display at two pixels a point makes a 400 point node an 800
/// pixel picture, and a crop that forgot would cut out its top left quarter.
pub(crate) fn cut_out(
    image: &egui::ColorImage,
    area: Rect,
    pixels_per_point: f32,
) -> egui::ColorImage {
    let scale = pixels_per_point.max(0.01);
    let left = (area.left() * scale).round().max(0.0) as usize;
    let top = (area.top() * scale).round().max(0.0) as usize;
    let right = ((area.right() * scale).round().max(0.0) as usize).min(image.size[0]);
    let bottom = ((area.bottom() * scale).round().max(0.0) as usize).min(image.size[1]);
    if right <= left || bottom <= top {
        return image.clone();
    }
    let width = right - left;
    let height = bottom - top;
    let mut pixels = Vec::with_capacity(width * height);
    for row in top..bottom {
        let from = row * image.size[0] + left;
        pixels.extend_from_slice(&image.pixels[from..from + width]);
    }
    egui::ColorImage {
        size: [width, height],
        pixels,
        source_size: egui::Vec2::new(width as f32, height as f32),
    }
}

fn write_png(image: &egui::ColorImage, path: &Path) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        if !folder.as_os_str().is_empty() {
            std::fs::create_dir_all(folder)?;
        }
    }
    let width = image.size[0] as u32;
    let height = image.size[1] as u32;
    // egui holds colour premultiplied by alpha; a PNG holds it straight. Writing the premultiplied
    // bytes as though they were straight ones darkens everything the window lets the desktop through,
    // which is most of it.
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for pixel in &image.pixels {
        bytes.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let buffer = image::RgbaImage::from_raw(width, height, bytes)
        .ok_or_else(|| std::io::Error::other("the captured frame was the wrong size"))?;
    buffer.save(path).map_err(|problem| std::io::Error::other(format!("{problem}")))
}

/// How long a command was told to wait, or the default.
fn waits_for(request: &Request, name: &str, fallback: Duration) -> Instant {
    let milliseconds = request.number(name).filter(|value| *value >= 0.0).map(|value| value as u64);
    Instant::now() + milliseconds.map(Duration::from_millis).unwrap_or(fallback)
}

impl UnluminousApp {
    // ------------------------------------------------------------------ the CLI and a whole Unluminous

    fn cli_top(&mut self, request: &Request, verb: &str, ctx: &egui::Context) -> Outcome {
        match verb {
            "status" => {
                // A section that is not a section is a question that was not asked, so it is
                // refused before anything is waited for.
                if let Err(message) = status_sections(request) {
                    return no(request, code::USAGE, message);
                }
                let changed = self.refresh_repository();
                if changed && self.git.as_ref().is_some_and(|git| git.is_busy()) {
                    Outcome::Hold(Waiting::Git {
                        until: waits_for(request, "wait", DEFAULT_WAIT),
                        answer: GitAnswer::WindowStatus,
                    })
                } else {
                    Outcome::Reply(self.window_status_reply(request, ctx))
                }
            }
            "quit" => {
                self.run_action(Action::Quit, ctx);
                done(request, "Unluminous is closing.")
            }
            _ => no(
                request,
                code::UNKNOWN_COMMAND,
                format!("There is no command called {}.", request.command),
            ),
        }
    }

    /// The one line `status` shows when nobody asked for JSON.
    fn status_sentence(&self) -> String {
        format!(
            "{} \u{00B7} {} tab{} \u{00B7} {} \u{00B7} explorer {} \u{00B7} terminal {}",
            self.tree.root().display(),
            self.files.len(),
            if self.files.len() == 1 { "" } else { "s" },
            self.files.active().name(),
            if self.explorer_visible { "shown" } else { "hidden" },
            if self.terminal.visible { "shown" } else { "hidden" },
        )
    }

    /// The complete top-level status reply, also used after repository discovery settles.
    ///
    /// The sentence is the whole window in one line and is the same whatever was asked for; the
    /// value is narrowed to the sections the caller named, so "how many panes are open" does not
    /// carry every setting and its help text with it. `task-1704` measured 4,900 bytes for the
    /// question that wants one of them.
    fn window_status_reply(&self, request: &Request, ctx: &egui::Context) -> Reply {
        Reply::done(&request.command, self.status_sentence(), self.status_value_for(request, ctx))
    }

    /// The status value, narrowed to the sections the caller asked for when it asked for any.
    fn status_value_for(&self, request: &Request, ctx: &egui::Context) -> Value {
        let full = self.status_value(ctx);
        let Ok(sections) = status_sections(request) else {
            return full;
        };
        let Some(keys) = status_keys_for(&sections) else {
            return full;
        };
        let Value::Object(map) = full else {
            return full;
        };
        let mut kept = Map::new();
        for (key, value) in map {
            if keys.contains(&key.as_str()) {
                kept.insert(key, value);
            }
        }
        Value::Object(kept)
    }

    /// Everything about the window, in one value.
    ///
    /// One command rather than eight, because the first thing anything driving Unluminous wants is to
    /// know where it is, and eight round trips to find that out is eight chances to read a window
    /// that changed underneath you.
    fn status_value(&self, ctx: &egui::Context) -> Value {
        let screen = ctx.content_rect();
        json!({
            "version": crate::build_info::VERSION,
            "buildDate": crate::build_info::BUILD_DATE,
            "pid": std::process::id(),
            "port": self.control.as_ref().map(|server| server.port()),
            "project": self.tree.root().to_string_lossy(),
            // **`focused` is the operating system's answer, not Unluminous's own.**
            // `status --section keyboard` says which surface *inside* the window Unluminous gave the
            // keys to; this says whether the window is the one the platform is sending keys to at all.
            // The two are different questions and `task-1945` was the second one: a browser node's page
            // is a native child window, and while it holds the focus every key press goes to the page,
            // `egui-winit` drops `StartDrag` so the title bar cannot move the window, and
            // `BeginResize` is a request the window manager throws away. Until this field there was no
            // way to ask that from outside the window — it had to be found by trying to drag it.
            "focused": ctx.input(|input| input.viewport().focused),
            // **And what the operating system itself says**, which is a different question and was the
            // one nothing could ask. `focused` above is `winit`'s cache of two window messages; these
            // two are `GetForegroundWindow` and `GetFocus` asked afresh. `task-2009` is a window that
            // was the foreground window with the keyboard while `winit` said it had no focus, and the
            // only symptom was that the title bar would not move it. `null` off Windows, where there
            // is no such cache. See `services::windows_focus`.
            "osForeground": self.os_focus.map(|os| os.foreground),
            "osKeyboard": self.os_focus.map(|os| os.keyboard),
            "maximised": ctx.input(|input| input.viewport().maximized),
            // **Whether a page has taken the operating system's keyboard**, which is the cause `focused`
            // above is only the symptom of — and the one thing that stops the window asking for a resize.
            // `task-2004` reported the window as unresizable while the canvas was open, intermittently;
            // this is the field that says which of the two it is, since a window that is merely in the
            // background also answers `focused: false`.
            "pageHasTheKeyboard": self.browser.page_holds_the_keyboard(),
            // **Whether Windows is resizing the window from its edges itself**, by the window answering
            // the hit test for them, rather than egui's grips asking `winit`. `task-2063`; see
            // `services::windows_resize`.
            "nativeResize": self.native_resize,
            // The last resize this window asked the window manager for, which is the only thing inside
            // the process that can be read back about one: `BeginResize` hands the drag to the window
            // manager and nothing here sees what it did with it. See `UnluminousApp::last_resize_asked`.
            "lastResizeAsked": self.last_resize_asked.map(|direction| match direction {
                egui::viewport::ResizeDirection::North => "north",
                egui::viewport::ResizeDirection::South => "south",
                egui::viewport::ResizeDirection::West => "west",
                egui::viewport::ResizeDirection::East => "east",
                egui::viewport::ResizeDirection::NorthEast => "north east",
                egui::viewport::ResizeDirection::NorthWest => "north west",
                egui::viewport::ResizeDirection::SouthEast => "south east",
                egui::viewport::ResizeDirection::SouthWest => "south west",
            }),
            "window": { "width": screen.width(), "height": screen.height() },
            "tabs": self.tabs_value(),
            "activeTab": self.files.active_index(),
            "panes": self.panes_value(),
            // **Whether the editing area is showing**, which is not in `panels` below and cannot be:
            // `dock::Panel::ALL` is the four panels that dock to an edge, and the editing area is
            // what the rest of the window is left over from. `task-1922` found that `toggle-editor`
            // therefore changed something no command could read back -- the one thing an agent could
            // do to this window and then not see.
            "editorShowing": self.editor_visible,
            // Which edge each panel is docked to, because since `task-1697` the terminal is not
            // necessarily along the bottom and nothing driving the window can assume it is.
            "panels": dock::Panel::ALL
                .into_iter()
                .map(|panel| json!({
                    "panel": panel.name(),
                    "side": self.panes.dock.side_of(panel).name(),
                    "position": self.panes.dock.order_of(panel),
                    "showing": self.panel_is_showing(panel),
                }))
                .collect::<Vec<_>>(),
            "editor": self.editor_value(),
            "explorer": {
                "visible": self.explorer_visible,
                "width": self.panes.explorer_width,
                "filter": self.filter,
                "rows": self.tree.rows().len(),
                "files": self.tree.all_files().len(),
            },
            "terminal": self.terminal_value(),
            // **Who holds the keyboard**, which is the one thing about this window nothing could be
            // asked. `task-1914` reported four separate "I cannot type in X" faults and every one of
            // them was a question about this, answerable only by trying it — so a person, and an agent,
            // and a test all had to guess. `Focus` is Unluminous's own answer and `text_edit_focused` is
            // egui's, and the two together are the whole of it: a text box that holds egui's focus makes
            // **every** surface stand aside, which is why one stuck field reads as "the terminal is
            // broken".
            "keyboard": self.keyboard_value(ctx),
            "modal": self.modal_value(ctx),
            "settings": self.settings_value(),
            "git": self.git_value(),
            "message": self.message,
        })
    }
}

impl crate::app::UnluminousApp {
    /// Who holds the keyboard, as data.
    ///
    /// Three answers rather than one, because they are three different questions and a fault in this
    /// area is always a disagreement between them:
    ///
    /// * `holder` is [`crate::app::Focus`], which is Unluminous's own answer: the editing area, the
    ///   explorer, a terminal tile, the canvas or a plugin.
    /// * `textBox` is `egui`'s: whether some `TextEdit` anywhere has the focus. While it is true the
    ///   editing area, every terminal grid and every provider **stands aside**, which is right when
    ///   somebody is typing in a field and is a window nothing can be typed into when the field that
    ///   holds it is not on the screen any more.
    /// * `node` is which canvas node the keyboard is in, when it is in one.
    fn keyboard_value(&self, ctx: &egui::Context) -> Value {
        json!({
            "holder": match self.focus {
                crate::app::Focus::Editor => "editor",
                crate::app::Focus::Explorer => "explorer",
                crate::app::Focus::Terminal => "terminal",
                crate::app::Focus::Space => "space",
                crate::app::Focus::Plugin => "plugin",
            },
            "textBox": crate::app::text_box_has_the_keyboard(ctx),
            "modal": crate::app::a_modal_has_the_keyboard(ctx),
            "node": self.space.chosen(),
            "pane": self.files.focus().pane(),
        })
    }
}

/// The sections `status --section` knows about, and the top-level keys each one carries.
///
/// A section is a filter over the keys the value already has, so there is no second answer to keep
/// in step with the first: the whole value is built and the sections are the names of its parts.
const STATUS_SECTIONS: &[(&str, &[&str])] = &[
    ("editor", &["editor"]),
    ("tabs", &["tabs", "activeTab"]),
    ("panes", &["panes"]),
    ("panels", &["panels"]),
    ("explorer", &["explorer"]),
    ("terminal", &["terminal"]),
    ("keyboard", &["keyboard"]),
    ("modal", &["modal"]),
    ("settings", &["settings"]),
    ("git", &["git"]),
    (
        "window",
        &[
            "window",
            "version",
            "buildDate",
            "pid",
            "port",
            "focused",
            "maximised",
            "pageHasTheKeyboard",
            "nativeResize",
            // What Windows itself says, beside `winit`'s cache of it. `task-2009`.
            "osForeground",
            "osKeyboard",
            "lastResizeAsked",
        ],
    ),
    ("project", &["project"]),
    ("message", &["message"]),
];

/// Read `--section` and check it against the sections there are.
///
/// Comma-separated, the way `editor references --include` takes its list, and case-insensitive,
/// because an agent writes `view` where the menu is `View`. Nothing given is the whole window,
/// which is what `status` has always been.
fn status_sections(request: &Request) -> Result<Vec<String>, String> {
    let Some(text) = request.text("section") else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = Vec::new();
    for part in text.split(',') {
        let name = part.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if !STATUS_SECTIONS.iter().any(|(known, _)| *known == name) {
            let all =
                STATUS_SECTIONS.iter().map(|(known, _)| *known).collect::<Vec<_>>().join(", ");
            return Err(format!("`{name}` is not a section of `status`. It is one of: {all}."));
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// The top-level keys the sections carry, in the order the value has them, or nothing when the
/// caller asked for the whole window.
fn status_keys_for(sections: &[String]) -> Option<Vec<&'static str>> {
    if sections.is_empty() {
        return None;
    }
    let mut keys: Vec<&'static str> = Vec::new();
    for section in sections {
        for (name, section_keys) in STATUS_SECTIONS {
            if *name == *section {
                for key in *section_keys {
                    if !keys.contains(key) {
                        keys.push(key);
                    }
                }
            }
        }
    }
    Some(keys)
}

pub(crate) fn unknown(request: &Request) -> Outcome {
    no(
        request,
        code::UNKNOWN_COMMAND,
        format!(
            "There is no command called {}. `unluminous-cli commands` lists them.",
            request.command
        ),
    )
}

fn view_mode_name(mode: ViewMode) -> &'static str {
    match mode {
        ViewMode::Raw => "raw",
        ViewMode::SideBySide => "side",
        ViewMode::Preview => "preview",
    }
}

/// Undo the escapes a shell will not: `\n` and `\t` typed as two characters.
///
/// A command line cannot carry a real new line through every shell there is, and typing two lines
/// into a document is an ordinary thing to want, so the two escapes every language spells the same
/// way are understood. `\\` is a backslash, so a literal `\n` is still reachable.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Where in the document a line and a column are, both counting from one.
///
/// Past the end of a line lands at the end of that line, and past the end of the document lands at
/// the end of the document, so a caller that guessed too far still lands somewhere sensible rather
/// than being refused.
pub fn offset_at(text: &unluminous_core::Rope, line: usize, column: usize) -> usize {
    let line = line.saturating_sub(1).min(text.len_lines().saturating_sub(1));
    let range = text.line_range(line);
    let body = text.byte_slice(range.clone());
    let body = body.trim_end_matches('\n').trim_end_matches('\r');
    let wanted = column.saturating_sub(1);
    let mut at = range.start;
    for (count, (offset, character)) in body.char_indices().enumerate() {
        if count == wanted {
            return range.start + offset;
        }
        at = range.start + offset + character.len_utf8();
    }
    at.max(range.start)
}

impl UnluminousApp {
    // `cli_offset` lives here rather than in `cli_editor.rs`, its heaviest user, because
    // `cli_debug.rs`'s `cli_debug_hover` asks it the same question -- where a symbol command is
    // talking about, as a caret, a byte offset, or a line and column -- and a helper two areas
    // both need is cheaper left where every area can already reach it than exported from
    // whichever one happened to hold it first.

    /// The position a symbol command is about: the caret, an offset, or a line and column.
    ///
    /// A line and a column go through the same `offset_at` `editor caret` and `editor select`
    /// already use, so all three mean the same thing by line 42 column 9 — including in a file whose
    /// letters are wider than one byte.
    fn cli_offset(&mut self, request: &Request) -> Result<usize, String> {
        if let Some(offset) = request.whole("offset") {
            let length = self.document().text().len_bytes();
            if offset > length {
                return Err(format!(
                    "This file is {length} bytes long, so there is no byte {offset}."
                ));
            }
            return Ok(offset);
        }
        let Some(line) = request.whole("line") else {
            return Ok(self.caret_offset());
        };
        let text = self.document().text();
        if line == 0 || line > text.len_lines() {
            return Err(format!(
                "This file has {} lines, so there is no line {line}.",
                text.len_lines()
            ));
        }
        Ok(offset_at(text, line, request.whole("column").unwrap_or(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_line_is_split_the_way_a_shell_splits_one() {
        assert_eq!(split_line("tab open README.md"), vec!["tab", "open", "README.md"]);
        // An explicitly quoted empty string is a word, so a setting can be cleared from here.
        assert_eq!(
            split_line(r#"settings set terminal.shell """#),
            vec!["settings", "set", "terminal.shell", ""]
        );
        assert_eq!(
            split_line(r#"settings set editor.exclude "" --json"#),
            vec!["settings", "set", "editor.exclude", "", "--json"]
        );
        // And plain whitespace still makes no word at all.
        assert_eq!(split_line("   tab   list   "), vec!["tab", "list"]);
        assert_eq!(
            split_line("settings set appearance.font.family \"Courier New\""),
            vec!["settings", "set", "appearance.font.family", "Courier New"]
        );
        assert_eq!(split_line("  spaced   out  "), vec!["spaced", "out"]);
        assert!(split_line("").is_empty());
    }

    #[test]
    fn the_two_escapes_a_shell_will_not_carry_are_understood() {
        assert_eq!(unescape("one\\ntwo"), "one\ntwo");
        assert_eq!(unescape("a\\tb"), "a\tb");
        assert_eq!(unescape("back\\\\slash"), "back\\slash");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("\\q"), "\\q", "an escape that means nothing is left alone");
    }

    #[test]
    fn a_line_and_a_column_find_the_place_in_the_text() {
        let text = unluminous_core::Rope::from_str("one\ntwo\nthree\n");
        assert_eq!(offset_at(&text, 1, 1), 0);
        assert_eq!(offset_at(&text, 2, 1), 4);
        assert_eq!(offset_at(&text, 2, 3), 6);
        assert_eq!(offset_at(&text, 3, 1), 8);
    }

    #[test]
    fn a_column_past_the_end_of_the_line_lands_at_the_end_of_it() {
        let text = unluminous_core::Rope::from_str("one\ntwo\n");
        assert_eq!(offset_at(&text, 1, 99), 3, "the end of `one`, not the next line");
        assert_eq!(offset_at(&text, 99, 1), 8, "past the last line is the end of the text");
    }

    #[test]
    fn a_line_and_a_column_find_the_place_in_text_that_is_not_ascii() {
        let text = unluminous_core::Rope::from_str("héllo\nwörld\n");
        // The second character is one byte in and two bytes wide, so the third is at three.
        assert_eq!(offset_at(&text, 1, 3), 3);
        assert_eq!(offset_at(&text, 2, 1), 7);
    }
}
