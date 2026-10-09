//! Running a notebook's cells, and what the kernel says back.
//!
//! **One cell is sent at a time.** Jupyter kernels queue requests themselves, but a queue held here
//! is what makes "queued" true on the screen and what lets a failure stop the rest: after an error the
//! remaining cells are marked skipped and never sent, which is what Jupyter's own Run All does
//! (`tasks/task-2220-jupyter-notebooks-tdd.md` §4.2).
//!
//! Every frame, [`UnluminousApp::hear_from_the_kernels`] drains each notebook tab's kernel, whether or
//! not the tab is showing, and sends the next queued cell when the kernel is free. The kernel's thread
//! wakes the window when it has something, so an idle notebook costs nothing.
//!
//! Which Python a kernel is started with is looked for once, on a thread, the first time a notebook
//! needs one: probing a Python takes a short run of it, and a window must not wait on that.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Instant, SystemTime};

use serde_json::Value;
use unluminous_jupyter::kernel::{self, Event, Kernel, KernelState, Python};
use unluminous_jupyter::nbformat::{CellKind, Output};
use unluminous_jupyter::text;

use crate::app::notebook::{KernelSlot, NotebookTab, Run, Running, Waiting, LONG_RUN};
use crate::app::UnluminousApp;

/// The Pythons on this machine, once somebody has asked.
#[derive(Default)]
pub enum Pythons {
    /// Nobody has needed one yet.
    #[default]
    NotLooked,
    /// Being looked for on a thread.
    Looking(Receiver<Vec<Python>>),
    /// What was found, best first.
    Found(Vec<Python>),
}

impl Pythons {
    /// What has been found so far, which is nothing while the search is running.
    pub fn found(&self) -> &[Python] {
        match self {
            Pythons::Found(found) => found,
            _ => &[],
        }
    }
}

/// What the events of one frame added up to for one notebook.
struct Gathered {
    /// A cell's outputs or execution count changed, so the file is now unsaved.
    changed: bool,
    /// The last thing to say in the status bar.
    said: Option<String>,
    /// The kernel answered a completion.
    completions: bool,
    /// A cell finished running.
    finished: bool,
    /// The debugger steps the events completed, in order.
    steps: Vec<crate::app::notebook_debug::DebugStep>,
}

/// Take each of `events` into `tab`, and add up what they changed.
fn take_the_events(tab: &mut NotebookTab, events: Vec<Event>) -> Gathered {
    let mut gathered = Gathered {
        changed: false,
        said: None,
        completions: false,
        finished: false,
        steps: Vec::new(),
    };
    for event in events {
        if let Some(step) = crate::app::notebook_debug::take_a_debug_event(tab, &event) {
            gathered.steps.push(step);
        }
        let heard = take_event(tab, event);
        gathered.changed |= heard.changed;
        gathered.completions |= heard.completions;
        gathered.finished |= heard.finished;
        gathered.said = heard.message.or(gathered.said.take());
    }
    gathered
}

/// What taking one kernel event changed, which the window acts on once the tab is no longer borrowed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Heard {
    /// A cell's outputs or execution count changed, so the file is now unsaved.
    pub changed: bool,
    /// Something to say in the status bar.
    pub message: Option<String>,
    /// The kernel answered a completion, so the popup is worked out again.
    pub completions: bool,
    /// A cell finished running, so a Variables panel that is showing asks for the variables again.
    pub finished: bool,
}

impl UnluminousApp {
    /// Queue the code cells in `cells` of the tab at `index` to run, starting a kernel if there is
    /// none. Markdown cells among them are rendered, which is what running one means in Jupyter.
    pub(crate) fn run_notebook_cells(&mut self, index: usize, cells: std::ops::Range<usize>) {
        // Running a cell is not glancing at a file, so a transient tab stops being one and a single
        // click elsewhere cannot take the kernel away with it.
        self.files.make_permanent(index);
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        queue_cells(tab, cells);
        if !tab.queue.is_empty() {
            self.start_a_kernel_if_needed(index);
        }
        self.wake_the_window();
    }

    /// Run every code cell in the notebook at `index`, from the top.
    pub(crate) fn run_every_notebook_cell(&mut self, index: usize) {
        let count = self.files.at(index).notebook.as_ref().map(|tab| tab.len()).unwrap_or(0);
        self.run_notebook_cells(index, 0..count);
    }

    /// Start a kernel for the tab at `index` when it has none, or has one that has died.
    pub(crate) fn start_a_kernel_if_needed(&mut self, index: usize) {
        let needs = match &self.files.at(index).notebook.as_ref().map(|tab| &tab.kernel) {
            Some(KernelSlot::NotStarted) | Some(KernelSlot::Failed { .. }) => true,
            Some(KernelSlot::Live(kernel)) => {
                matches!(kernel.state(), KernelState::Dead(_) | KernelState::Stopped)
            }
            None => false,
        };
        if !needs {
            return;
        }
        let Some(python) = self.python_for_the_notebook(index) else {
            // Still looking: the next frame asks again, and the cells stay queued.
            return;
        };
        self.start_the_kernel(index, &python);
    }

    /// The Python chosen for the notebook at `index`, if one has been: by the person, by the
    /// notebook's last run, or by [`UnluminousApp::python_for_the_notebook`] choosing one. This is the
    /// one place the window reads it, whether for the kernel picker, the command line or a run.
    pub(crate) fn notebook_python(&self, index: usize) -> Option<PathBuf> {
        self.files.at(index).notebook.as_ref().and_then(|tab| tab.python.clone())
    }

    /// The Python a notebook's kernel is started with: the one chosen for it, or the best one found
    /// that has `ipykernel`. `None` while the machine is still being looked over.
    fn python_for_the_notebook(&mut self, index: usize) -> Option<PathBuf> {
        if let Some(chosen) = self.notebook_python(index) {
            return Some(chosen);
        }
        self.look_for_pythons();
        let found = self.pythons.found();
        if found.is_empty() {
            if matches!(self.pythons, Pythons::Found(_)) {
                self.no_python_was_found(index);
            }
            return None;
        }
        let path = best_python(found)?.path.clone();
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            tab.python = Some(path.clone());
        }
        Some(path)
    }

    /// Start looking for the Pythons on this machine, once.
    pub(crate) fn look_for_pythons(&mut self) {
        match &self.pythons {
            Pythons::NotLooked => {
                let (send, receive) = mpsc::channel();
                let project = self.tree.root().to_path_buf();
                let context = self.context.clone();
                std::thread::spawn(move || {
                    let _ = send.send(kernel::find_pythons(Some(&project)));
                    if let Some(context) = context {
                        context.request_repaint();
                    }
                });
                self.pythons = Pythons::Looking(receive);
            }
            Pythons::Looking(receive) => {
                if let Ok(found) = receive.try_recv() {
                    self.pythons = Pythons::Found(found);
                }
            }
            Pythons::Found(_) => {}
        }
    }

    /// Stop every notebook's kernel and wait until each one has gone, for a window that is closing.
    /// A kernel dropped earlier, by a closed tab or a restart, is waited for too.
    pub(crate) fn stop_every_kernel(&mut self) {
        for index in 0..self.files.len() {
            if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
                tab.kernel = KernelSlot::NotStarted;
            }
        }
        kernel::wait_for_kernels_to_stop();
    }

    /// Say that a notebook cannot run because there is no Python, and stop waiting for one.
    fn no_python_was_found(&mut self, index: usize) {
        if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
            tab.kernel = KernelSlot::Failed {
                reason: "No Python was found on this machine. Install Python 3 and then ipykernel."
                    .to_owned(),
                missing: Some("python".to_owned()),
            };
            skip_what_is_queued(tab);
        }
    }

    /// Start the kernel for the tab at `index` with `python`, replacing any it had.
    pub(crate) fn start_the_kernel(&mut self, index: usize, python: &Path) {
        let folder = self.files.at(index).path().and_then(Path::parent).map(Path::to_path_buf);
        let folder = folder.unwrap_or_else(|| self.tree.root().to_path_buf());
        let wake = self.kernel_waker();
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let name = tab.kernel_name.clone().or_else(|| kernel_name_in(&tab.model.metadata));
        tab.kernel = match Kernel::start(python, name.as_deref(), &folder, wake) {
            Ok(kernel) => KernelSlot::Live(Box::new(kernel)),
            Err(problem) => KernelSlot::Failed {
                reason: format!("{} could not be started: {problem}", python.display()),
                missing: None,
            },
        };
        tab.python = Some(python.to_path_buf());
        tab.running = None;
    }

    /// What a kernel's thread calls when it has something: a repaint of this window, by the route
    /// that reaches a window whose run loop is asleep. See `thread_waker`.
    fn kernel_waker(&self) -> kernel::Waker {
        self.thread_waker()
    }

    /// Ask for another frame, from a change made outside one.
    fn wake_the_window(&self) {
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }

    /// Drain every notebook's kernel, and send each the next queued cell when it is free.
    pub(crate) fn hear_from_the_kernels(&mut self) {
        if matches!(self.pythons, Pythons::Looking(_)) {
            self.look_for_pythons();
        }
        for index in 0..self.files.len() {
            if self.files.at(index).notebook.is_none() {
                continue;
            }
            self.hear_from_one_kernel(index);
            self.run_a_cell_the_debugger_is_ready_for(index);
            self.send_the_next_cell(index);
        }
    }

    /// Take everything one tab's kernel has said since the last frame.
    fn hear_from_one_kernel(&mut self, index: usize) {
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return };
        let Some(events) = tab.kernel().map(|kernel| kernel.events()) else { return };
        let spoken = crate::app::notebook_frame::notebook_extension(&tab.model.metadata);
        let mut gathered = take_the_events(tab, events);
        // A kernel that started in another language than the notebook said, such as the Rust kernel
        // chosen for a notebook made as Python, has just written its language into the metadata. The
        // cells are coloured and read in that language from now on. `task-2229`.
        if crate::app::notebook_frame::notebook_extension(&tab.model.metadata) != spoken {
            file.coloured_revision = None;
            file.cached.symbols = None;
        }
        if let Some(note) = notice_a_long_run(tab) {
            gathered.said = Some(note);
        }
        if gathered.changed {
            file.document.note_a_change_outside_the_text();
        }
        self.act_on_what_the_kernel_said(index, gathered);
    }

    /// Do what the events of one frame call for, once the tab is no longer borrowed: say what the
    /// kernel said, work out completions and variables again, and take the debugger's steps.
    fn act_on_what_the_kernel_said(&mut self, index: usize, gathered: Gathered) {
        let Gathered { said, completions, finished, steps, .. } = gathered;
        if let Some(message) = said {
            self.say_what_a_kernel_said(index, message);
        }
        if completions && index == self.files.active_index() {
            self.kernel_completions_arrived();
        }
        let showing =
            self.files.at(index).notebook.as_deref().is_some_and(|tab| tab.variables_showing);
        if finished && showing {
            self.ask_for_the_variables(index);
        }
        for step in steps {
            self.take_a_debug_step(index, step);
        }
        // A cell waiting to be debugged carries on whenever the kernel has answered everything it
        // was asked: once it has started, and once each step's answer is in.
        let waiting = self
            .files
            .at(index)
            .notebook
            .as_deref()
            .and_then(|tab| tab.debugging.as_ref())
            .is_some_and(|debugging| debugging.waiting.is_some());
        if waiting {
            self.carry_on_debugging(index);
        }
    }

    /// Show `message` in the status bar. A notebook that is not showing names itself, or its kernel's
    /// news reads as being about the tab that is.
    fn say_what_a_kernel_said(&mut self, index: usize, message: String) {
        let file = self.files.at(index);
        let name =
            file.path().and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
        self.message = Some(match (index == self.files.active_index(), name) {
            (false, Some(name)) => format!("{name}: {message}"),
            _ => message,
        });
    }

    /// Send the tab's next queued cell, when the kernel is ready and nothing of ours is running.
    fn send_the_next_cell(&mut self, index: usize) {
        let needs_a_kernel = {
            let Some(tab) = self.files.at(index).notebook.as_deref() else { return };
            !tab.queue.is_empty() && !matches!(tab.kernel, KernelSlot::Live(_))
        };
        if needs_a_kernel {
            self.start_a_kernel_if_needed(index);
            return;
        }
        let ready = self.files.at(index).notebook.as_deref().is_some_and(|tab| {
            tab.running.is_none() && !tab.queue.is_empty() && kernel_is_free(tab)
        });
        if !ready {
            return;
        }
        // Copied only once there is a cell to send, since this is asked on every frame.
        let text = self.files.at(index).document.text().to_string();
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(id) = tab.queue.pop_front() else { return };
        let Some(cell) = tab.index_of(&id) else { return };
        let source = text::cell_source(&text, &tab.spans[cell]).into_owned();
        let Some(kernel) = tab.kernel() else { return };
        let request = kernel.execute(&source);
        begin_a_run(tab, &id, &request);
    }

    /// Interrupt what the kernel of the tab at `index` is running, and drop what was queued.
    pub(crate) fn interrupt_the_kernel(&mut self, index: usize) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        skip_what_is_queued(tab);
        if let Some(kernel) = tab.kernel() {
            kernel.interrupt();
        }
    }

    /// Restart the kernel of the tab at `index`, forgetting every variable. With `run_all`, every
    /// code cell is queued once the new kernel is ready. Queued now, the cells would be skipped by the
    /// `Restarted` event that clears the old kernel's run.
    pub(crate) fn restart_the_kernel(&mut self, index: usize, run_all: bool) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        skip_what_is_queued(tab);
        tab.running = None;
        tab.waiting = None;
        tab.variables.clear();
        match tab.kernel() {
            Some(kernel) => {
                kernel.restart();
                tab.run_all_after_restart = run_all;
            }
            None if run_all => self.run_every_notebook_cell(index),
            None => self.start_a_kernel_if_needed(index),
        }
    }

    /// Shut the kernel of the tab at `index` down. The next run starts a new one.
    pub(crate) fn shut_down_the_kernel(&mut self, index: usize) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        skip_what_is_queued(tab);
        if let Some(kernel) = tab.kernel() {
            kernel.shutdown();
        }
        tab.kernel = KernelSlot::NotStarted;
        tab.running = None;
        tab.waiting = None;
    }

    /// Answer the `input()` the kernel of the tab at `index` is waiting on.
    pub(crate) fn answer_the_kernel(&mut self, index: usize, value: &str) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(waiting) = tab.waiting.take() else { return };
        // The kernel does not print what was asked or what was answered; a Jupyter front end does,
        // so the cell's output reads as the conversation it was. A password is not repeated.
        let shown = if waiting.password {
            "\u{2022}".repeat(value.chars().count())
        } else {
            value.to_owned()
        };
        let echo = format!("{}{shown}\n", waiting.prompt);
        add_stream(tab, Some(waiting_request(tab, &waiting.cell).as_str()), "stdout", &echo);
        if let Some(kernel) = tab.kernel() {
            kernel.input_reply(value);
        }
    }

    /// Ask the kernel of the tab at `index` what variables it holds. The answer arrives as an event.
    pub(crate) fn ask_for_the_variables(&mut self, index: usize) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let request = tab.kernel().map(|kernel| kernel.variables());
        tab.variables_request = request;
    }
}

/// The Python to start a kernel with when none was chosen: the first that has `ipykernel`, or else
/// the first found. `find_pythons` lists the best first.
fn best_python(found: &[Python]) -> Option<&Python> {
    found.iter().find(|python| python.has_ipykernel).or_else(|| found.first())
}

/// The request a cell is running as, which is what its output is filed under.
fn waiting_request(tab: &NotebookTab, cell: &str) -> String {
    tab.requests
        .iter()
        .find(|(_, id)| id.as_str() == cell)
        .map(|(request, _)| request.clone())
        .unwrap_or_default()
}

/// True when the kernel has started and is not busy restarting or dead.
fn kernel_is_free(tab: &NotebookTab) -> bool {
    match &tab.kernel {
        KernelSlot::Live(kernel) => matches!(kernel.state(), KernelState::Idle | KernelState::Busy),
        _ => false,
    }
}

/// Write the language a kernel says it runs into a notebook's metadata, as Jupyter does when it saves:
/// `language_info` from the kernel's `kernel_info_reply`, and `kernelspec.language` beside it.
/// Answers whether anything changed, which leaves the notebook unsaved. `task-2229`.
///
/// This is what makes choosing the Rust kernel for a notebook that was Python colour and complete its
/// cells as Rust: the metadata is the one place every reader of a notebook's language looks.
pub fn adopt_the_kernels_language(metadata: &mut Value, info: &Value) -> bool {
    let Some(language) = info.get("language_info").filter(|value| {
        value.as_object().is_some_and(|fields| fields.get("name").is_some_and(Value::is_string))
    }) else {
        return false;
    };
    let Some(fields) = metadata.as_object_mut() else { return false };
    let mut changed = false;
    if fields.get("language_info") != Some(language) {
        fields.insert("language_info".to_owned(), language.clone());
        changed = true;
    }
    let name = language["name"].as_str().unwrap_or_default().to_lowercase();
    let spec = fields.entry("kernelspec").or_insert_with(|| serde_json::json!({}));
    if let Some(spec) = spec.as_object_mut() {
        if spec.get("language").and_then(Value::as_str) != Some(name.as_str()) {
            spec.insert("language".to_owned(), Value::String(name));
            changed = true;
        }
    }
    changed
}

/// The kernelspec a notebook's own metadata names, which is the kernel it was last run with.
fn kernel_name_in(metadata: &Value) -> Option<String> {
    metadata.get("kernelspec")?.get("name")?.as_str().map(str::to_owned)
}

/// Mark a cell as running under `request`, clearing what it showed from its last run.
fn begin_a_run(tab: &mut NotebookTab, id: &str, request: &str) {
    tab.running = Some(Running { cell: id.to_owned(), request: request.to_owned() });
    tab.requests.insert(request.to_owned(), id.to_owned());
    tab.runs.insert(id.to_owned(), Run::Running { since: Instant::now() });
    tab.long_run_noticed = false;
    if let Some(cell) = tab.index_of(id).and_then(|at| tab.model.cells.get_mut(at)) {
        cell.outputs.clear();
        cell.execution_count = None;
    }
    tab.clear_before_next.remove(id);
    tab.tracebacks_open.remove(id);
    tab.output_scroll.remove(id);
    tab.outputs_changed(id);
}

/// Mark every queued cell as skipped and empty the queue, which is what a failure or an interrupt
/// does to a run.
pub(crate) fn skip_what_is_queued(tab: &mut NotebookTab) {
    for id in tab.queue.drain(..) {
        tab.runs.insert(id, Run::Skipped);
    }
    tab.bands_revision += 1;
}

/// The notice the reference editor gives when a cell has run longer than a minute, once per run.
fn notice_a_long_run(tab: &mut NotebookTab) -> Option<String> {
    let running = tab.running.as_ref()?;
    let Some(Run::Running { since }) = tab.runs.get(&running.cell) else { return None };
    if tab.long_run_noticed || since.elapsed() < LONG_RUN {
        return None;
    }
    tab.long_run_noticed = true;
    let number = tab.index_of(&running.cell).map(|at| at + 1).unwrap_or(0);
    Some(format!("Cell {number} has been running for more than a minute."))
}

/// Take one event from a notebook's kernel. Pure apart from the clock, so a test can feed events in.
pub fn take_event(tab: &mut NotebookTab, event: Event) -> Heard {
    match event {
        Event::ExecuteInput { request, execution_count } => {
            let Some(id) = cell_for(tab, request.as_deref()) else { return Heard::default() };
            set_count(tab, &id, execution_count);
            Heard { changed: true, message: None, completions: false, finished: false }
        }
        event @ (Event::Stream { .. }
        | Event::DisplayData { .. }
        | Event::UpdateDisplayData { .. }
        | Event::ExecuteResult { .. }
        | Event::Error { .. }
        | Event::ClearOutput { .. }
        | Event::ExecuteReply { .. }) => take_an_output_event(tab, event),
        event @ (Event::InputRequest { .. }
        | Event::CompleteReply { .. }
        | Event::Variables { .. }) => take_an_answer_event(tab, event),
        other => take_a_lifecycle_event(tab, other),
    }
}

/// The events that add to a cell's outputs, change them, clear them, or end the cell's run.
fn take_an_output_event(tab: &mut NotebookTab, event: Event) -> Heard {
    match event {
        Event::Stream { request, name, text } => add_stream(tab, request.as_deref(), &name, &text),
        Event::DisplayData { request, data, metadata, display_id } => {
            let mut output = Output::display_data(data, metadata);
            if let Some(display) = display_id {
                output.set_display_id(&display);
            }
            add_output(tab, request.as_deref(), output)
        }
        Event::UpdateDisplayData { data, metadata, display_id, .. } => {
            update_display(tab, &display_id, data, metadata)
        }
        Event::ExecuteResult { request, execution_count, data, metadata } => add_output(
            tab,
            request.as_deref(),
            Output::execute_result(execution_count, data, metadata),
        ),
        Event::Error { request, ename, evalue, traceback } => {
            add_output(tab, request.as_deref(), Output::error(&ename, &evalue, traceback))
        }
        Event::ClearOutput { request, wait } => clear_output(tab, request.as_deref(), wait),
        Event::ExecuteReply { request, status, execution_count } => {
            finish_a_run(tab, request.as_deref(), &status, execution_count)
        }
        _ => Heard::default(),
    }
}

/// The events that answer something asked of the kernel: a prompt for `input()`, a completion
/// request, or the list of variables.
fn take_an_answer_event(tab: &mut NotebookTab, event: Event) -> Heard {
    match event {
        Event::InputRequest { request, prompt, password } => {
            let Some(cell) = cell_for(tab, request.as_deref()) else { return Heard::default() };
            tab.waiting = Some(Waiting { cell, prompt, password, typed: String::new() });
            tab.bands_revision += 1;
            Heard::default()
        }
        Event::CompleteReply { request, matches, cursor_start, metadata, .. } => {
            let Some(asked) = tab
                .asked
                .as_mut()
                .filter(|asked| Some(asked.request.as_str()) == request.as_deref())
            else {
                return Heard::default();
            };
            asked.matches = unluminous_jupyter::completion::matches_of(&matches, &metadata);
            asked.kernel_start = asked.body_start
                + unluminous_jupyter::completion::byte_of(&asked.source, cursor_start);
            asked.answered = true;
            Heard { completions: true, ..Heard::default() }
        }
        Event::Variables { request, rows } => {
            if request.is_some() && request == tab.variables_request {
                tab.variables = rows;
                tab.variables_request = None;
            }
            Heard::default()
        }
        _ => Heard::default(),
    }
}

/// The events about the kernel itself rather than about a cell.
fn take_a_lifecycle_event(tab: &mut NotebookTab, event: Event) -> Heard {
    match event {
        Event::Started { info, .. } => Heard {
            changed: adopt_the_kernels_language(&mut tab.model.metadata, &info),
            ..Heard::default()
        },
        Event::Restarted { info, .. } => {
            adopt_the_kernels_language(&mut tab.model.metadata, &info);
            stop_the_run(tab);
            let run_all = std::mem::take(&mut tab.run_all_after_restart);
            if run_all {
                queue_cells(tab, 0..tab.len());
            }
            Heard {
                changed: false,
                message: Some(match run_all {
                    true => "The kernel restarted. Running every cell.".to_owned(),
                    false => "The kernel restarted.".to_owned(),
                }),
                completions: false,
                finished: false,
            }
        }
        Event::Died { reason } => {
            let changed = say_the_kernel_died_in_the_running_cell(tab, &reason);
            stop_the_run(tab);
            Heard {
                changed,
                message: Some(format!("The kernel died: {reason}")),
                completions: false,
                finished: false,
            }
        }
        Event::Failed { message, missing } => {
            stop_the_run(tab);
            tab.kernel = KernelSlot::Failed { reason: message.clone(), missing };
            Heard { changed: false, message: Some(message), completions: false, finished: false }
        }
        Event::Stopped => {
            stop_the_run(tab);
            Heard::default()
        }
        _ => Heard::default(),
    }
}

/// Queue the code cells in `cells` to run, skipping any already queued or running, and show the
/// Markdown cells among them rendered.
fn queue_cells(tab: &mut NotebookTab, cells: std::ops::Range<usize>) {
    for cell in cells {
        let Some(found) = tab.model.cells.get(cell) else { continue };
        let id = found.id.clone();
        match found.kind {
            CellKind::Code => {
                if !tab.queue.contains(&id) && tab.running.as_ref().is_none_or(|run| run.cell != id)
                {
                    tab.queue.push_back(id.clone());
                    tab.runs.insert(id, Run::Queued);
                }
            }
            CellKind::Markdown => {
                tab.editing.remove(&id);
                tab.bands_revision += 1;
            }
            CellKind::Raw => {}
        }
    }
}

/// Mark the cell that was running when the kernel died as failed, with an output saying so, which
/// is what the reference editor shows. Answers whether there was such a cell.
fn say_the_kernel_died_in_the_running_cell(tab: &mut NotebookTab, reason: &str) -> bool {
    let Some(running) = tab.running.take() else { return false };
    let took = match tab.runs.get(&running.cell) {
        Some(Run::Running { since }) => since.elapsed(),
        _ => std::time::Duration::ZERO,
    };
    tab.runs.insert(
        running.cell.clone(),
        Run::Done {
            ok: false,
            took,
            at: SystemTime::now(),
            clock: crate::services::clock::time_of_day(),
        },
    );
    if let Some(cell) = tab.index_of(&running.cell).and_then(|at| tab.model.cells.get_mut(at)) {
        let text = format!("The kernel died while this cell was running: {reason}\n");
        cell.outputs.push(unluminous_jupyter::nbformat::Output::stream("stderr", &text));
    }
    tab.outputs_changed(&running.cell);
    true
}

/// Whatever was running did not finish and whatever was queued will not run.
fn stop_the_run(tab: &mut NotebookTab) {
    if let Some(running) = tab.running.take() {
        tab.runs.insert(running.cell, Run::Skipped);
    }
    tab.waiting = None;
    skip_what_is_queued(tab);
}

/// The cell a kernel request belongs to.
fn cell_for(tab: &NotebookTab, request: Option<&str>) -> Option<String> {
    let request = request?;
    tab.requests.get(request).cloned().filter(|id| tab.index_of(id).is_some())
}

/// Set a cell's execution count.
fn set_count(tab: &mut NotebookTab, id: &str, count: Option<u64>) {
    if let Some(cell) = tab.index_of(id).and_then(|at| tab.model.cells.get_mut(at)) {
        cell.execution_count = count;
    }
    tab.outputs_changed(id);
}

/// Clear a cell's outputs now, when `clear_output(wait=True)` asked for that to happen before the
/// next one.
fn clear_if_asked(tab: &mut NotebookTab, id: &str) {
    if tab.clear_before_next.remove(id) {
        if let Some(cell) = tab.index_of(id).and_then(|at| tab.model.cells.get_mut(at)) {
            cell.outputs.clear();
        }
    }
}

/// Add text a cell printed. Consecutive text on one stream goes into one output, as Jupyter keeps it.
fn add_stream(tab: &mut NotebookTab, request: Option<&str>, name: &str, text: &str) -> Heard {
    let Some(id) = cell_for(tab, request) else { return Heard::default() };
    clear_if_asked(tab, &id);
    let Some(cell) = tab.index_of(&id).and_then(|at| tab.model.cells.get_mut(at)) else {
        return Heard::default();
    };
    match cell.outputs.last_mut() {
        Some(last) if last.stream_name() == Some(name) => last.append_stream_text(text),
        _ => cell.outputs.push(Output::stream(name, text)),
    }
    tab.outputs_changed(&id);
    Heard { changed: true, message: None, completions: false, finished: false }
}

/// Add one output to the cell a request belongs to.
fn add_output(tab: &mut NotebookTab, request: Option<&str>, output: Output) -> Heard {
    let Some(id) = cell_for(tab, request) else { return Heard::default() };
    clear_if_asked(tab, &id);
    if let Some(count) = output.to_value().get("execution_count").and_then(Value::as_u64) {
        set_count(tab, &id, Some(count));
    }
    let Some(cell) = tab.index_of(&id).and_then(|at| tab.model.cells.get_mut(at)) else {
        return Heard::default();
    };
    cell.outputs.push(output);
    tab.outputs_changed(&id);
    Heard { changed: true, message: None, completions: false, finished: false }
}

/// Replace every output shown under `display_id`, in whichever cell it is.
fn update_display(tab: &mut NotebookTab, display_id: &str, data: Value, metadata: Value) -> Heard {
    let mut touched = Vec::new();
    for cell in &mut tab.model.cells {
        for output in &mut cell.outputs {
            if output.display_id() == Some(display_id) {
                output.set_data(data.clone(), metadata.clone());
                touched.push(cell.id.clone());
            }
        }
    }
    let changed = !touched.is_empty();
    for id in touched {
        tab.outputs_changed(&id);
    }
    Heard { changed, message: None, completions: false, finished: false }
}

/// `clear_output`: now, or when the next output arrives.
fn clear_output(tab: &mut NotebookTab, request: Option<&str>, wait: bool) -> Heard {
    let Some(id) = cell_for(tab, request) else { return Heard::default() };
    if wait {
        tab.clear_before_next.insert(id);
        return Heard::default();
    }
    if let Some(cell) = tab.index_of(&id).and_then(|at| tab.model.cells.get_mut(at)) {
        cell.outputs.clear();
    }
    tab.outputs_changed(&id);
    Heard { changed: true, message: None, completions: false, finished: false }
}

/// The kernel has finished a cell: record how it went, and stop the run if it failed.
fn finish_a_run(
    tab: &mut NotebookTab,
    request: Option<&str>,
    status: &str,
    count: Option<u64>,
) -> Heard {
    let finished =
        tab.running.as_ref().is_some_and(|running| Some(running.request.as_str()) == request);
    if !finished {
        return Heard::default();
    }
    let running = tab.running.take().expect("checked above");
    let took = match tab.runs.get(&running.cell) {
        Some(Run::Running { since }) => since.elapsed(),
        _ => std::time::Duration::ZERO,
    };
    let ok = status == "ok";
    tab.runs.insert(
        running.cell.clone(),
        Run::Done { ok, took, at: SystemTime::now(), clock: crate::services::clock::time_of_day() },
    );
    if count.is_some() {
        set_count(tab, &running.cell, count);
    }
    tab.waiting = None;
    if !ok {
        skip_what_is_queued(tab);
    }
    tab.outputs_changed(&running.cell);
    Heard { changed: true, message: None, completions: false, finished: true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_jupyter::nbformat;

    /// A tab holding two code cells, `a` and `b`, with `a` running as request `r1`.
    fn running_tab() -> NotebookTab {
        let mut model = nbformat::empty();
        model.cells = vec![
            nbformat::Cell::new(CellKind::Code, "a", "print(1)"),
            nbformat::Cell::new(CellKind::Code, "b", "1/0"),
        ];
        let (mut tab, text) = NotebookTab::new(model);
        tab.spans = text::spans(&text);
        tab.queue.push_back("b".to_owned());
        begin_a_run(&mut tab, "a", "r1");
        tab
    }

    fn outputs(tab: &NotebookTab, id: &str) -> Vec<String> {
        let cell = &tab.model.cells[tab.index_of(id).unwrap()];
        cell.outputs.iter().map(|output| output.output_type().to_owned()).collect()
    }

    #[test]
    fn consecutive_text_on_one_stream_is_one_output_and_a_new_stream_is_another() {
        let mut tab = running_tab();
        let stream = |name: &str, text: &str| Event::Stream {
            request: Some("r1".into()),
            name: name.into(),
            text: text.into(),
        };
        take_event(&mut tab, stream("stdout", "one\n"));
        take_event(&mut tab, stream("stdout", "two\n"));
        take_event(&mut tab, stream("stderr", "bad\n"));
        let cell = &tab.model.cells[0];
        assert_eq!(cell.outputs.len(), 2);
        assert_eq!(cell.outputs[0].text().as_deref(), Some("one\ntwo\n"));
    }

    #[test]
    fn a_failed_cell_skips_the_rest_of_the_run() {
        let mut tab = running_tab();
        let heard = take_event(
            &mut tab,
            Event::ExecuteReply {
                request: Some("r1".into()),
                status: "error".into(),
                execution_count: Some(3),
            },
        );
        assert!(heard.changed);
        assert!(tab.running.is_none());
        assert!(tab.queue.is_empty());
        assert_eq!(tab.runs.get("b"), Some(&Run::Skipped));
        assert!(matches!(tab.runs.get("a"), Some(Run::Done { ok: false, .. })));
        assert_eq!(tab.model.cells[0].execution_count, Some(3));
    }

    #[test]
    fn clear_output_with_wait_clears_only_when_the_next_output_arrives() {
        let mut tab = running_tab();
        let stream = |text: &str| Event::Stream {
            request: Some("r1".into()),
            name: "stdout".into(),
            text: text.into(),
        };
        take_event(&mut tab, stream("10%"));
        take_event(&mut tab, Event::ClearOutput { request: Some("r1".into()), wait: true });
        assert_eq!(outputs(&tab, "a"), vec!["stream"], "nothing is cleared yet");
        take_event(&mut tab, stream("20%"));
        assert_eq!(tab.model.cells[0].outputs[0].text().as_deref(), Some("20%"));
    }

    #[test]
    fn an_updated_display_replaces_the_output_it_names_wherever_it_is() {
        let mut tab = running_tab();
        take_event(
            &mut tab,
            Event::DisplayData {
                request: Some("r1".into()),
                data: serde_json::json!({"text/plain": "old"}),
                metadata: serde_json::json!({}),
                display_id: Some("d".into()),
            },
        );
        take_event(
            &mut tab,
            Event::UpdateDisplayData {
                request: None,
                data: serde_json::json!({"text/plain": "new"}),
                metadata: serde_json::json!({}),
                display_id: "d".into(),
            },
        );
        assert_eq!(tab.model.cells[0].outputs[0].mime_text("text/plain").as_deref(), Some("new"));
    }

    #[test]
    fn output_for_a_request_nobody_sent_is_ignored() {
        let mut tab = running_tab();
        let heard = take_event(
            &mut tab,
            Event::Stream {
                request: Some("elsewhere".into()),
                name: "stdout".into(),
                text: "x".into(),
            },
        );
        assert!(!heard.changed);
        assert!(tab.model.cells.iter().all(|cell| cell.outputs.is_empty()));
    }

    #[test]
    fn a_restart_stops_the_run_and_skips_what_was_queued() {
        let mut tab = running_tab();
        take_event(&mut tab, Event::Restarted { info: serde_json::json!({}), pid: None });
        assert!(tab.running.is_none());
        assert_eq!(tab.runs.get("a"), Some(&Run::Skipped));
        assert_eq!(tab.runs.get("b"), Some(&Run::Skipped));
    }

    #[test]
    fn restart_and_run_all_queues_every_code_cell_once_the_new_kernel_is_up() {
        let mut tab = running_tab();
        tab.run_all_after_restart = true;
        let heard =
            take_event(&mut tab, Event::Restarted { info: serde_json::json!({}), pid: None });
        let code: Vec<String> = tab
            .model
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
        assert_eq!(tab.queue.iter().cloned().collect::<Vec<_>>(), code);
        assert!(code.iter().all(|id| tab.runs.get(id) == Some(&Run::Queued)));
        assert!(!tab.run_all_after_restart, "the flag is used once");
        assert_eq!(heard.message.as_deref(), Some("The kernel restarted. Running every cell."));
    }

    #[test]
    fn a_kernel_that_dies_mid_cell_fails_that_cell_and_says_why_in_its_output() {
        let mut tab = running_tab();
        take_event(&mut tab, Event::Died { reason: "the kernel process ended".into() });
        assert!(matches!(tab.runs.get("a"), Some(Run::Done { ok: false, .. })));
        assert_eq!(tab.runs.get("b"), Some(&Run::Skipped));
        let outputs = &tab.model.cells[tab.index_of("a").unwrap()].outputs;
        let said = outputs.last().unwrap().text().unwrap_or_default();
        assert!(said.contains("The kernel died while this cell was running"), "{said}");
    }
}
