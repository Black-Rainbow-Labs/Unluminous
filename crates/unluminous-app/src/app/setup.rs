//! The Set Up Unluminous dialog: what is left of setting Unluminous up on this machine, and the button that
//! does it (`task-2301`).
//!
//! `unluminous-cli setup` (`task-2278`) takes the steps: the command line tool on the path, each coding agent
//! connected to the MCP server, the server asked whether it answers. `unluminous_voice` downloads the speech
//! model and asks for the microphone (`task-2290`). What was missing was something that notices setup is not
//! finished, shows what is left, and runs it where the result can be seen: an install on a Mac from before
//! `task-2278` never ran it, an update on a Mac never runs it, and a Windows upgrade keeps the installer tasks
//! the first install chose, so a task added since then is never run. `tasks/task-2301-setup-modal-tdd.md` is
//! the design.
//!
//! **The steps are taken on a thread, one at a time**, and each one's state comes back down a channel, so the
//! dialog draws each row as it starts and finishes and the speech model's row as its bytes arrive.
//!
//! **The microphone is asked for here, in the window's own process.** macOS records a microphone request
//! against the process responsible for it, and for a program started from a terminal that is the terminal, so
//! `unluminous-cli setup` run in Terminal asked on Terminal's behalf. The window is Unluminous itself.
//!
//! **Only the released binary looks at this machine.** `main` calls `allow_the_setup`, which hands the window
//! the real planner and the real steps. A window a test builds has neither unless the test hands it its own,
//! so a test never reads or changes the person's agents, path or microphone.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use serde_json::{json, Value};
use unluminous_cli::protocol::{code, Request};
use unluminous_cli::setup::{self, record, State, Step};

use crate::app::cli::{no, ok, Outcome};
use crate::app::UnluminousApp;
use crate::components::setup_dialog;
use crate::components::toast::Act;

/// The steps the dialog can take, in the order it takes them. The server is last because asking it is a
/// check of what the steps before it changed.
pub const STEPS: [&str; 7] =
    ["path", "claude", "codex", "opencode", "speech-model", "microphone", "server"];

/// One row of the dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The step's name: one of [`STEPS`].
    pub name: String,
    pub title: String,
    pub state: State,
    /// One sentence about the step as it stands.
    pub message: String,
    /// Whether `Run` takes it.
    pub ticked: bool,
    /// Taking it now.
    pub running: bool,
    /// Waiting its turn in a run.
    pub queued: bool,
    /// How far through it is, while it is running and can say.
    pub fraction: Option<f32>,
}

impl Row {
    pub fn new(name: &str, state: State, message: impl Into<String>) -> Row {
        Row {
            name: name.to_owned(),
            title: title(name),
            state,
            message: message.into(),
            ticked: needs_doing(state),
            running: false,
            queued: false,
            fraction: None,
        }
    }

    /// Whether there is anything for `Run` to do.
    pub fn needs_doing(&self) -> bool {
        needs_doing(self.state)
    }

    /// The word the row's state is drawn and reported with.
    pub fn word(&self) -> &'static str {
        if self.running {
            return "running";
        }
        if self.queued {
            return "waiting";
        }
        match self.state {
            // The server is asked at the end of every run rather than ticked.
            State::Would if self.name == "server" => "on run",
            State::Would => "to do",
            State::Done => "done",
            State::Already => "already",
            State::Skipped => "skipped",
            State::Left => "left",
            State::Declined => "declined",
            State::Noted => "noted",
            State::Failed => "failed",
        }
    }

    /// Whether the row has a tick box: the server is checked whenever a run ends, so it has none.
    pub fn can_be_ticked(&self) -> bool {
        self.name != "server"
    }
}

/// Whether a step in `state` still has something to do.
fn needs_doing(state: State) -> bool {
    matches!(state, State::Would | State::Failed)
}

/// What a step is called in the dialog.
pub fn title(name: &str) -> String {
    match name {
        "path" => "Command line tool".to_owned(),
        "speech-model" => "Speech model (66 MB)".to_owned(),
        "microphone" => "Microphone".to_owned(),
        "server" => "MCP server".to_owned(),
        other => unluminous_cli::mcp::install::Client::parse(other)
            .map(|client| client.title().to_owned())
            .unwrap_or_else(|| other.to_owned()),
    }
}

/// What a plan found: one row a step, and the two lines under the rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub rows: Vec<Row>,
    /// The agents that are not on this machine, by title.
    pub not_found: Vec<String>,
    /// Which API keys are set, as a sentence.
    pub keys: String,
}

/// What a step says while it is being taken.
pub enum Event {
    Started(String),
    /// A fraction from 0 to 1 and what is happening.
    Progress(String, f32, String),
    Finished(String, State, String),
    /// Every step asked for has been taken, or the run was stopped.
    Ended,
}

/// Works out the plan. The released binary's looks at this machine; a test hands in its own.
pub type Planner = Arc<dyn Fn() -> Plan + Send + Sync>;

/// Takes one step by name, saying how far through it is. The released binary's changes this machine.
pub type Taker = Arc<dyn Fn(&str, &dyn Fn(f32, &str)) -> (State, String) + Send + Sync>;

/// The dialog's state, whether or not it is showing.
#[derive(Default)]
pub struct SetupState {
    /// Whether the dialog is on the screen.
    pub showing: bool,
    pub plan: Option<Plan>,
    /// A plan being worked out.
    planning: Option<Receiver<Plan>>,
    /// A run going on.
    running: Option<Receiver<Event>>,
    /// Set by `Stop`: the step being taken finishes and no other is started.
    stop: Arc<AtomicBool>,
    /// The `Don't show this again` tick box.
    pub dont_show: bool,
    /// Something to say at the foot, such as a record that could not be written.
    pub problem: Option<String>,
    pub planner: Option<Planner>,
    pub taker: Option<Taker>,
    /// Whether this window may open the dialog by itself when it starts, which only the released binary may.
    may_prompt: bool,
    /// The first plan has been asked for, so the window decides once whether to open the dialog.
    deciding: bool,
    /// Decided to open, waiting for any other modal to close first.
    open_when_free: bool,
    /// How many runs have ended, which a test waits on.
    pub runs: usize,
}

impl SetupState {
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn is_planning(&self) -> bool {
        self.planning.is_some()
    }
}

impl UnluminousApp {
    /// The released binary's planner and steps, and leave to open the dialog by itself. Called from `main`
    /// and by nothing else, so a test's window never reads or changes this machine's setup.
    pub fn allow_the_setup(&mut self) {
        self.setup.planner = Some(Arc::new(plan_this_machine));
        self.setup.taker = Some(Arc::new(take_on_this_machine));
        self.setup.may_prompt = true;
    }

    /// A test's planner and steps, in place of this machine's.
    pub fn use_a_setup(&mut self, planner: Planner, taker: Taker) {
        self.setup.planner = Some(planner);
        self.setup.taker = Some(taker);
    }

    /// The settings folder the record of what the person chose is kept in.
    fn setup_folder(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|store| store.folder().to_path_buf())
    }

    /// What the person chose, from the store's folder. The default when the window has no store.
    pub(crate) fn setup_record(&self) -> record::Record {
        self.setup_folder().map(|folder| record::read(&folder)).unwrap_or_default()
    }

    fn write_the_setup_record(&mut self, record: &record::Record) {
        let Some(folder) = self.setup_folder() else { return };
        if let Err(problem) = record::write(&folder, record) {
            self.setup.problem = Some(problem);
        }
    }

    /// Open the dialog and work the plan out again, because a step may have changed since it was last
    /// looked at. Every other modal is closed first, which is what opening any of them does.
    pub(crate) fn open_the_setup(&mut self) {
        self.close_every_modal();
        self.setup.showing = true;
        self.setup.dont_show = !self.setup_record().prompt;
        if !self.setup.is_running() {
            self.plan_the_setup();
        }
    }

    /// Close the dialog. A run carries on, and its rows are there when the dialog is opened again.
    pub(crate) fn close_the_setup(&mut self) {
        self.setup.showing = false;
        let mut record = self.setup_record();
        if record.prompt == self.setup.dont_show {
            record.prompt = !self.setup.dont_show;
            self.write_the_setup_record(&record);
        }
    }

    /// Start working the plan out on a thread.
    fn plan_the_setup(&mut self) {
        let Some(planner) = self.setup.planner.clone() else {
            return;
        };
        let (send, receive) = channel();
        self.setup.planning = Some(receive);
        let waker = self.thread_waker();
        std::thread::spawn(move || {
            let _ = send.send(planner());
            waker();
        });
    }

    /// Once a frame, as the window starts: work the plan out, and when it arrives open the dialog if setup is
    /// not finished. Only in the released binary, and only in the one window a session that holds the claim.
    pub(crate) fn decide_whether_to_open_the_setup(&mut self) {
        if !self.setup.may_prompt || self.setup.deciding {
            return;
        }
        self.setup.deciding = true;
        let record = self.setup_record();
        if !record.prompt {
            return;
        }
        self.plan_the_setup();
    }

    /// Take in a plan and the run's events. Called once a frame; true when something arrived and the window
    /// has to draw.
    pub(crate) fn take_the_setup_news(&mut self) -> bool {
        let mut changed = false;
        if let Some(planning) = self.setup.planning.as_ref() {
            if let Ok(plan) = planning.try_recv() {
                self.setup.planning = None;
                self.take_a_plan(plan);
                changed = true;
            }
        }
        changed |= self.take_the_run_events();
        if self.setup.open_when_free && self.open_modal().is_none() {
            self.setup.open_when_free = false;
            self.close_every_modal();
            self.setup.showing = true;
            changed = true;
        }
        changed
    }

    /// A plan arrived: keep it, and when this window is the one deciding at its start, open the dialog or
    /// offer the new gap in a notice.
    fn take_a_plan(&mut self, plan: Plan) {
        let record = self.setup_record();
        let finished =
            record::finished(plan.rows.iter().map(|row| (row.name.as_str(), row.state)), &record);
        let deciding = self.setup.deciding && self.setup.may_prompt && !self.setup.showing;
        let left: Vec<String> = plan
            .rows
            .iter()
            .filter(|row| {
                row.can_be_ticked() && row.needs_doing() && !record.has_declined(&row.name)
            })
            .map(|row| row.title.clone())
            .collect();
        self.setup.plan = Some(plan);
        if !deciding || finished || !record.prompt {
            self.setup.may_prompt = false;
            return;
        }
        self.setup.may_prompt = false;
        if !self.claim_the_setup_prompt() {
            return;
        }
        match record.finished.is_some() {
            // Setup was finished once, so what is missing now is something new, such as an agent installed
            // since. A notice says so rather than a modal in the person's way.
            true => {
                let text = format!(
                    "{} {} not set up for Unluminous.",
                    left.join(", "),
                    match left.len() {
                        1 => "is",
                        _ => "are",
                    }
                );
                self.toasts.offer(
                    text,
                    vec![
                        ("Not Now".to_owned(), Act::DeclineSetup),
                        ("Set Up".to_owned(), Act::RunSetup),
                    ],
                );
            }
            false => self.setup.open_when_free = true,
        }
    }

    /// Whether this window is the one that offers setup when several start together. The first to write its
    /// process id into `setup-prompt.pid` holds it while it runs, which is `voice/listening.pid`'s shape.
    fn claim_the_setup_prompt(&self) -> bool {
        let Some(folder) = self.setup_folder() else { return true };
        let file = folder.join("setup-prompt.pid");
        let mine = std::process::id();
        let holder =
            std::fs::read_to_string(&file).ok().and_then(|text| text.trim().parse::<u32>().ok());
        if let Some(holder) = holder {
            if holder != mine && unluminous_cli::instances::is_running(holder) {
                return false;
            }
        }
        crate::services::store::write_atomically(&file, mine.to_string().as_bytes()).is_ok()
    }

    /// `Not Now` and `Set Up` on the notice.
    pub(crate) fn act_on_the_setup_notice(&mut self, run: bool) {
        self.toasts.withdraw_the_offers();
        match run {
            true => {
                self.open_the_setup();
                self.setup.open_when_free = false;
                let wanted = self.setup_left();
                if let Err(problem) = self.run_the_setup(&wanted) {
                    self.setup.problem = Some(problem);
                }
            }
            false => {
                let mut record = self.setup_record();
                for name in self.setup_left() {
                    record.decline(&name);
                }
                self.write_the_setup_record(&record);
            }
        }
    }

    /// The steps that still have something to do and were not declined.
    fn setup_left(&self) -> Vec<String> {
        let record = self.setup_record();
        self.setup
            .plan
            .iter()
            .flat_map(|plan| plan.rows.iter())
            .filter(|row| {
                row.can_be_ticked() && row.needs_doing() && !record.has_declined(&row.name)
            })
            .map(|row| row.name.clone())
            .collect()
    }

    /// Take `wanted`, in the order of [`STEPS`], on a thread, and the server after them. A step not in
    /// `wanted` that still had something to do is declined, which is what leaving its box unticked means.
    pub(crate) fn run_the_setup(&mut self, wanted: &[String]) -> Result<(), String> {
        if self.setup.is_running() {
            return Err("Setup is already running.".to_owned());
        }
        let Some(taker) = self.setup.taker.clone() else {
            return Err("This window was not started by the installed Unluminous, so it does not change this machine's setup. Run `unluminous-cli setup` in a terminal instead.".to_owned());
        };
        let Some(plan) = self.setup.plan.as_mut() else {
            return Err(
                "Setup has not been looked at yet. `setup status` says when it has.".to_owned()
            );
        };
        let mut record = match self.store.as_ref() {
            Some(store) => record::read(store.folder()),
            None => record::Record::default(),
        };
        let mut order = Vec::new();
        for row in plan.rows.iter_mut() {
            let asked = wanted.iter().any(|name| *name == row.name);
            if row.name == "server" {
                continue;
            }
            if asked {
                record.accept(&row.name);
                row.queued = true;
                row.ticked = true;
                order.push(row.name.clone());
            } else if row.needs_doing() {
                record.decline(&row.name);
                row.state = State::Declined;
                row.ticked = false;
                row.message = "Declined. Tick it and press Run Setup to set it up.".to_owned();
            }
        }
        if let Some(server) = plan.rows.iter_mut().find(|row| row.name == "server") {
            server.queued = true;
            order.push("server".to_owned());
        }
        self.write_the_setup_record(&record);
        self.setup.stop.store(false, Ordering::SeqCst);
        let stop = self.setup.stop.clone();
        let (send, receive) = channel();
        self.setup.running = Some(receive);
        let waker = self.thread_waker();
        std::thread::spawn(move || {
            for name in order {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let _ = send.send(Event::Started(name.clone()));
                waker();
                let progress = |fraction: f32, said: &str| {
                    let _ = send.send(Event::Progress(name.clone(), fraction, said.to_owned()));
                    waker();
                };
                let (state, message) = taker(&name, &progress);
                let _ = send.send(Event::Finished(name.clone(), state, message));
                waker();
            }
            let _ = send.send(Event::Ended);
            waker();
        });
        Ok(())
    }

    /// `Stop`: the step being taken finishes and no other is started.
    pub(crate) fn stop_the_setup(&mut self) {
        self.setup.stop.store(true, Ordering::SeqCst);
    }

    /// The run's events since the last frame. True when one arrived.
    fn take_the_run_events(&mut self) -> bool {
        let Some(running) = self.setup.running.as_ref() else { return false };
        let events: Vec<Event> = running.try_iter().collect();
        if events.is_empty() {
            return false;
        }
        let mut ended = false;
        for event in events {
            let Some(plan) = self.setup.plan.as_mut() else { break };
            match event {
                Event::Started(name) => {
                    if let Some(row) = plan.rows.iter_mut().find(|row| row.name == name) {
                        row.queued = false;
                        row.running = true;
                        row.fraction = None;
                    }
                }
                Event::Progress(name, fraction, said) => {
                    if let Some(row) = plan.rows.iter_mut().find(|row| row.name == name) {
                        row.fraction = Some(fraction.clamp(0.0, 1.0));
                        row.message = said;
                    }
                }
                Event::Finished(name, state, message) => {
                    if let Some(row) = plan.rows.iter_mut().find(|row| row.name == name) {
                        row.running = false;
                        row.fraction = None;
                        row.state = state;
                        row.message = message;
                        row.ticked = needs_doing(state);
                    }
                }
                Event::Ended => ended = true,
            }
        }
        if ended {
            self.end_the_run();
        }
        true
    }

    /// A run ended: what was stopped before it started goes back to waiting for a press, and when nothing is
    /// left to do the record says setup is finished.
    fn end_the_run(&mut self) {
        self.setup.running = None;
        self.setup.runs += 1;
        let mut record = self.setup_record();
        let Some(plan) = self.setup.plan.as_mut() else { return };
        for row in plan.rows.iter_mut().filter(|row| row.queued) {
            row.queued = false;
        }
        let finished =
            record::finished(plan.rows.iter().map(|row| (row.name.as_str(), row.state)), &record);
        if finished {
            record.finished = Some(format!(
                "{} {}",
                crate::build_info::VERSION,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or_default()
            ));
            self.write_the_setup_record(&record);
        }
    }

    /// Draw the dialog, and carry out what was pressed.
    pub(crate) fn show_the_setup(&mut self, ui: &mut egui::Ui) {
        if !self.setup.showing {
            return;
        }
        let ctx = ui.ctx().clone();
        let showing = setup_dialog::Showing {
            running: self.setup.is_running(),
            looking: self.setup.plan.is_none(),
            problem: self.setup.problem.clone(),
            can_run: self.setup.taker.is_some(),
        };
        let had_a_plan = self.setup.plan.is_some();
        let mut plan = self.setup.plan.take().unwrap_or_default();
        let outcome = setup_dialog::show(&ctx, &mut plan, &showing, &mut self.setup.dont_show);
        if had_a_plan {
            self.setup.plan = Some(plan);
        }
        self.act_on_the_setup(outcome);
    }

    fn act_on_the_setup(&mut self, outcome: setup_dialog::Outcome) {
        if let Some(name) = outcome.fix {
            self.fix_a_setup_step(&name);
        }
        if outcome.stop {
            self.stop_the_setup();
        }
        if outcome.run {
            let wanted: Vec<String> = self
                .setup
                .plan
                .iter()
                .flat_map(|plan| plan.rows.iter())
                .filter(|row| row.ticked && row.can_be_ticked() && row.needs_doing())
                .map(|row| row.name.clone())
                .collect();
            if let Err(problem) = self.run_the_setup(&wanted) {
                self.setup.problem = Some(problem);
            }
        }
        if outcome.close {
            self.close_the_setup();
        }
    }

    /// The button on a failed row that fixes what failed.
    fn fix_a_setup_step(&mut self, name: &str) {
        match name {
            "microphone" if self.setup.taker.is_some() => {
                if let Err(problem) = unluminous_voice::access::open_settings() {
                    self.setup.problem = Some(problem);
                }
            }
            "path" if self.setup.taker.is_some() => {
                if let Ok(program) = std::env::current_exe() {
                    crate::services::launcher::reveal(&program);
                }
            }
            other => {
                let wanted = vec![other.to_owned()];
                if let Err(problem) = self.run_the_setup(&wanted) {
                    self.setup.problem = Some(problem);
                }
            }
        }
    }

    /// The dialog as data, for `setup status` and the other `setup` commands.
    pub(crate) fn setup_status(&self) -> Value {
        let record = self.setup_record();
        let plan = self.setup.plan.as_ref();
        let rows: Vec<Value> = plan
            .map(|plan| {
                plan.rows
                    .iter()
                    .map(|row| {
                        json!({
                            "step": row.name,
                            "title": row.title,
                            "state": row.word(),
                            "message": row.message,
                            "ticked": row.ticked,
                            "fraction": row.fraction,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let finished = plan.map(|plan| {
            record::finished(plan.rows.iter().map(|row| (row.name.as_str(), row.state)), &record)
        });
        json!({
            "showing": self.setup.showing,
            "looking": self.setup.is_planning(),
            "running": self.setup.is_running(),
            "finished": finished,
            "steps": rows,
            "notFound": plan.map(|plan| plan.not_found.clone()).unwrap_or_default(),
            "keys": plan.map(|plan| plan.keys.clone()).unwrap_or_default(),
            "declined": record.declined,
            "prompt": record.prompt,
            "runs": self.setup.runs,
            "problem": self.setup.problem,
            "looksAtThisMachine": self.setup.planner.is_some(),
        })
    }

    /// The `setup` commands.
    pub(crate) fn cli_setup(&mut self, request: &Request, verb: &str) -> Outcome {
        match verb {
            "status" => {
                if self.setup.plan.is_none() && !self.setup.is_planning() {
                    self.plan_the_setup();
                }
                let status = self.setup_status();
                let said = match (&self.setup.plan, self.setup.is_running()) {
                    (_, true) => "Setup is running.".to_owned(),
                    (None, false) if self.setup.planner.is_none() => "This window does not look at this machine's setup, because it was not started by the installed Unluminous.".to_owned(),
                    (None, false) => "Setup is being looked at. Ask again in a moment.".to_owned(),
                    (Some(_), false) => match status["finished"].as_bool() {
                        Some(true) => "Setup is finished.".to_owned(),
                        _ => "Setup is not finished.".to_owned(),
                    },
                };
                ok(request, said, status)
            }
            "open" => {
                self.open_the_setup();
                ok(request, "Set Up Unluminous is open.", self.setup_status())
            }
            "run" => {
                // A run asked for before the dialog has looked works the plan out first, here, because the
                // steps are named against it. It reads a few files, the same as `setup --check`.
                if self.setup.plan.is_none() && !self.setup.is_running() {
                    if let Some(planner) = self.setup.planner.clone() {
                        self.setup.planning = None;
                        self.setup.plan = Some(planner());
                    }
                }
                let wanted = match request.text("steps") {
                    Some(named) => match steps_named(&named) {
                        Ok(steps) => steps,
                        Err(problem) => return no(request, code::USAGE, problem),
                    },
                    None => {
                        let record = self.setup_record();
                        self.setup
                            .plan
                            .iter()
                            .flat_map(|plan| plan.rows.iter())
                            .filter(|row| {
                                row.can_be_ticked()
                                    && row.needs_doing()
                                    && !record.has_declined(&row.name)
                            })
                            .map(|row| row.name.clone())
                            .collect()
                    }
                };
                if !self.setup.showing {
                    self.close_every_modal();
                    self.setup.showing = true;
                }
                match self.run_the_setup(&wanted) {
                    Ok(()) => ok(
                        request,
                        format!("Setup is running: {}.", wanted.join(", ")),
                        self.setup_status(),
                    ),
                    Err(problem) => no(request, code::NOT_APPLICABLE, problem),
                }
            }
            "decline" => {
                let named = request.text("steps").unwrap_or_default();
                let steps = match steps_named(&named) {
                    Ok(steps) if !steps.is_empty() => steps,
                    Ok(_) => return no(request, code::USAGE, "setup decline needs a step name."),
                    Err(problem) => return no(request, code::USAGE, problem),
                };
                let mut record = self.setup_record();
                for name in &steps {
                    record.decline(name);
                }
                self.write_the_setup_record(&record);
                if let Some(plan) = self.setup.plan.as_mut() {
                    for row in plan.rows.iter_mut().filter(|row| steps.contains(&row.name)) {
                        if row.needs_doing() {
                            row.state = State::Declined;
                            row.ticked = false;
                        }
                    }
                }
                ok(request, format!("Declined {}.", steps.join(", ")), self.setup_status())
            }
            "close" => {
                if request.switch("dont-show-again") {
                    self.setup.dont_show = true;
                }
                self.setup.showing = true;
                self.close_the_setup();
                ok(request, "Set Up Unluminous is closed.", self.setup_status())
            }
            other => no(request, code::USAGE, format!("setup has no command called {other}.")),
        }
    }
}

/// Step names from a comma separated list, refused when one is not a step.
fn steps_named(named: &str) -> Result<Vec<String>, String> {
    let mut steps = Vec::new();
    for name in named.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        if !STEPS.contains(&name) || name == "server" {
            return Err(format!(
                "`{name}` is not a setup step. The steps are path, claude, codex, opencode, speech-model and microphone."
            ));
        }
        if !steps.iter().any(|step| step == name) {
            steps.push(name.to_owned());
        }
    }
    Ok(steps)
}

/// The machine `setup` looks at: on macOS and Linux with the login shell's environment, because an
/// application started from the Dock is given a `PATH` that finds none of the agents.
fn this_machine() -> setup::Machine {
    if cfg!(windows) {
        setup::Machine::this_one()
    } else {
        setup::Machine::with_variables(crate::services::login_shell::for_a_child())
    }
}

/// The options every step is taken with: the declined steps from the record and from the installer.
fn options_for_this_machine() -> setup::Options {
    let mut declined = record::read(&unluminous_cli::instances::settings_folder()).declined;
    if let Some((selected, offered)) = record::the_installers_tasks() {
        for name in record::declined_by_the_installer(&selected, &offered) {
            if !declined.contains(&name) {
                declined.push(name);
            }
        }
    }
    setup::Options { voice: false, ask_the_server: false, declined, ..setup::Options::default() }
}

/// The plan for this machine: the path and each agent as `setup --check` sees them, the speech model, the
/// microphone and the server.
fn plan_this_machine() -> Plan {
    let machine = this_machine();
    let options = options_for_this_machine();
    let steps = setup::plan(&options, &machine);
    let keys = setup::keys(&machine).message;
    let voice = Voice {
        model: unluminous_voice::install::status().is_complete(),
        microphone: unluminous_voice::access::status(),
    };
    plan_from(&steps, &voice, &options.declined, keys)
}

/// What the voice half of a plan needs to know.
pub struct Voice {
    /// Whether the speech model and ONNX Runtime are both on disk.
    pub model: bool,
    pub microphone: unluminous_voice::access::Access,
}

/// The rows for `steps` and `voice`. A pure function of what was found, so a test can ask it.
pub fn plan_from(steps: &[Step], voice: &Voice, declined: &[String], keys: String) -> Plan {
    let mut plan = Plan { keys, ..Plan::default() };
    for step in steps {
        let is_an_agent = unluminous_cli::mcp::install::Client::parse(&step.step).is_some();
        if is_an_agent && step.state == State::Skipped {
            plan.not_found.push(title(&step.step));
            continue;
        }
        let message = match step.state {
            State::Would if is_an_agent => {
                format!("Gives {} Unluminous's tools, through its MCP server.", title(&step.step))
            }
            State::Would => {
                "Puts `unluminous-cli` on the path, for terminals and agents.".to_owned()
            }
            _ => step.message.clone(),
        };
        plan.rows.push(Row::new(&step.step, step.state, message));
    }
    let is_declined = |name: &str| declined.iter().any(|declined| declined == name);
    let model = match (voice.model, is_declined("speech-model")) {
        (true, _) => Row::new("speech-model", State::Already, "The speech model is downloaded."),
        (false, true) => Row::new(
            "speech-model",
            State::Declined,
            "Declined. Tick it and press Run Setup to download it.",
        ),
        (false, false) => Row::new(
            "speech-model",
            State::Would,
            "Downloads the model that turns speech into text in the Agent-Chat prompt.",
        ),
    };
    plan.rows.push(model);
    use unluminous_voice::access::Access;
    let microphone = match (&voice.microphone, is_declined("microphone")) {
        (Access::Allowed, _) => {
            Some(Row::new("microphone", State::Already, "Unluminous may use the microphone."))
        }
        (Access::Unknown, _) => None,
        (_, true) => Some(Row::new(
            "microphone",
            State::Declined,
            "Declined. Tick it and press Run Setup to be asked.",
        )),
        (Access::NotAsked, false) => Some(Row::new(
            "microphone",
            State::Would,
            "Asks for the microphone, for dictation and for listening for \"Sable\".",
        )),
        (Access::Refused(why), false) => Some(Row::new("microphone", State::Failed, why.clone())),
    };
    plan.rows.extend(microphone);
    plan.rows.push(Row {
        ticked: false,
        ..Row::new(
            "server",
            State::Would,
            "Asked whether it answers once the steps above have run.",
        )
    });
    plan
}

/// Take one step on this machine.
fn take_on_this_machine(name: &str, progress: &dyn Fn(f32, &str)) -> (State, String) {
    match name {
        "speech-model" => take_the_speech_model(progress),
        "microphone" => take_the_microphone(),
        _ => {
            let machine = this_machine();
            let mut options = options_for_this_machine();
            // Run on purpose, so it is taken whatever was declined.
            options.declined.retain(|declined| declined != name);
            match setup::take(name, &options, &machine) {
                Some(step) => (step.state, step.message),
                None => (State::Failed, format!("`{name}` is not a setup step.")),
            }
        }
    }
}

/// Download the speech model and ONNX Runtime, saying how far through it is.
fn take_the_speech_model(progress: &dyn Fn(f32, &str)) -> (State, String) {
    if unluminous_voice::install::status().is_complete() {
        return (State::Already, "The speech model is downloaded.".to_owned());
    }
    match unluminous_voice::install::install(&|step, fraction| {
        progress(fraction, &format!("{step} {:.0}%", fraction * 100.0))
    }) {
        Ok(_) => (State::Done, "Downloaded the speech model.".to_owned()),
        Err(problem) => (State::Failed, format!("The speech model was not downloaded: {problem}")),
    }
}

/// How long the system's prompt is given to be answered.
const MICROPHONE_PATIENCE: std::time::Duration = std::time::Duration::from_secs(180);

/// Ask for the microphone in this process, and wait for the answer, because on macOS it comes back on
/// another thread after the person presses a button in the system's prompt.
fn take_the_microphone() -> (State, String) {
    use unluminous_voice::access::{self, Access};
    if access::status() == Access::NotAsked {
        access::request();
        let started = std::time::Instant::now();
        while access::status() == Access::NotAsked && started.elapsed() < MICROPHONE_PATIENCE {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    match access::status() {
        Access::Allowed => (State::Done, "Unluminous may use the microphone.".to_owned()),
        Access::NotAsked => (
            State::Failed,
            "The system's question about the microphone was not answered. Press Try Again."
                .to_owned(),
        ),
        Access::Refused(why) => (State::Failed, why),
        Access::Unknown => {
            (State::Skipped, "This system has nothing to ask about the microphone.".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_voice::access::Access;

    fn step(name: &str, state: State) -> Step {
        Step { step: name.to_owned(), state, message: format!("{name} said"), detail: json!({}) }
    }

    #[test]
    fn an_agent_that_is_not_installed_is_named_under_the_rows_rather_than_given_one() {
        let steps = [
            step("path", State::Already),
            step("claude", State::Would),
            step("codex", State::Skipped),
            step("opencode", State::Done),
        ];
        let voice = Voice { model: false, microphone: Access::NotAsked };
        let plan = plan_from(&steps, &voice, &[], "No API key is set.".to_owned());
        let names: Vec<&str> = plan.rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["path", "claude", "opencode", "speech-model", "microphone", "server"]);
        assert_eq!(plan.not_found, ["Codex"]);
        let claude = &plan.rows[1];
        assert!(claude.ticked && claude.word() == "to do", "{claude:?}");
        assert!(!plan.rows[0].ticked, "a step already done has nothing to tick");
        assert!(!plan.rows.last().unwrap().can_be_ticked(), "the server has no tick box");
    }

    #[test]
    fn a_platform_with_no_microphone_question_has_no_microphone_row() {
        let voice = Voice { model: true, microphone: Access::Unknown };
        let plan = plan_from(&[], &voice, &[], String::new());
        assert!(plan.rows.iter().all(|row| row.name != "microphone"));
        assert_eq!(plan.rows[0].state, State::Already);
    }

    #[test]
    fn a_declined_voice_step_is_drawn_declined_and_unticked() {
        let voice = Voice { model: false, microphone: Access::NotAsked };
        let declined = vec!["speech-model".to_owned(), "microphone".to_owned()];
        let plan = plan_from(&[], &voice, &declined, String::new());
        for name in ["speech-model", "microphone"] {
            let row = plan.rows.iter().find(|row| row.name == name).expect("a row");
            assert_eq!(row.state, State::Declined);
            assert!(!row.ticked);
        }
    }

    #[test]
    fn a_refused_microphone_is_a_failure_with_the_reason() {
        let voice =
            Voice { model: true, microphone: Access::Refused("The switch is off.".to_owned()) };
        let plan = plan_from(&[], &voice, &[], String::new());
        let row = plan.rows.iter().find(|row| row.name == "microphone").expect("a row");
        assert_eq!((row.state, row.message.as_str()), (State::Failed, "The switch is off."));
    }

    #[test]
    fn step_names_are_checked() {
        assert_eq!(steps_named("claude, codex,claude").unwrap(), ["claude", "codex"]);
        assert!(steps_named("server").is_err());
        assert!(steps_named("gemini").is_err());
    }
}
