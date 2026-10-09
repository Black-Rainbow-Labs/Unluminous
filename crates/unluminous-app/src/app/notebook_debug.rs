//! Debugging a notebook cell, with the debugger tile Unluminous already has.
//!
//! **The kernel is the program, so the debugger attaches to it.** `ipykernel` ships with `debugpy`,
//! which is a Debug Adapter Protocol server, so `Debug Cell` asks the kernel to start listening for one
//! (`debugpy.listen`) and then opens an ordinary session on that port with an `attach` request — the
//! same `DebugState`, the same tile, the same stepping keys and variables tree as debugging a script.
//! `tasks/task-2220-jupyter-notebooks-tdd.md` §4.7.
//!
//! **A cell runs under a file name of its own.** IPython compiles each cell as a file named for a hash
//! of its source, in a folder of the kernel's (`ipykernel.compiler.get_file_name`), and that is the
//! path the debugger reports a stop in and the path a breakpoint has to name. So before a cell is
//! debugged the kernel is asked what that name is, the cell's breakpoints are sent against it with
//! line numbers counted within the cell, and a stop in that file is drawn in the notebook at the
//! cell's own line. A cell with no breakpoint gets one on its first line for the run, which is
//! The reference editor's "add a breakpoint if none are set".
//!
//! Three steps, each waiting on the kernel, so this is a small state machine on the tab
//! ([`CellDebug`]) driven by [`UnluminousApp::hear_from_one_kernel`]'s events. Each step is taken as
//! soon as none of the previous step's requests is outstanding. The kernel does not have to be idle,
//! because it queues our requests behind whatever it is running.
//!
//! **Ending the session leaves the kernel running.** The session attached, so stopping it only
//! disconnects. debugpy keeps listening on the same port, and `listen` cannot be called twice in one
//! process, so the next Debug Cell attaches to that port again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use unluminous_jupyter::kernel::{Event, KernelState};
use unluminous_jupyter::nbformat::CellKind;

use crate::app::debug::DebugState;
use crate::app::notebook::{KernelSlot, NotebookTab};
use crate::app::UnluminousApp;

/// One notebook's debugging, from asking the kernel to listen to the cells whose files are known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CellDebug {
    /// The port debugpy was asked to listen on. Nought until it has been asked.
    pub port: u16,
    /// The request the listen was sent as, until the kernel answers it.
    pub listening: Option<String>,
    /// True once the kernel has said debugpy is listening on `port`.
    pub listens: bool,
    /// The file each cell runs under, by cell id, as the kernel last said.
    pub files: HashMap<String, PathBuf>,
    /// The questions asked about a cell's file name, by request.
    pub asking: HashMap<String, String>,
    /// The cell to debug once everything above is ready.
    pub waiting: Option<String>,
    /// A cell whose breakpoints have been sent: it runs once the debugger has answered this many of
    /// them, or at the deadline, whichever comes first.
    pub run_when: Option<(String, u64, std::time::Instant)>,
}

impl CellDebug {
    /// True while a request of ours to the kernel has not been answered.
    fn is_waiting_on_the_kernel(&self) -> bool {
        self.listening.is_some() || !self.asking.is_empty()
    }
}

/// What a kernel event meant for debugging, acted on by the window once the tab is not borrowed.
#[derive(Debug, Clone, PartialEq)]
pub enum DebugStep {
    /// debugpy is listening: open a session on its port.
    Listening,
    /// The kernel said which file this cell runs under.
    FileKnown(String),
    /// Something went wrong, in these words.
    Failed(String),
}

/// Whether a notebook's cells can be debugged: only a Python notebook's, because debugging a cell is
/// debugpy inside ipykernel. A Rust notebook's Debug button is not drawn. `task-2229`.
pub fn can_be_debugged(metadata: &serde_json::Value) -> bool {
    crate::app::notebook_frame::notebook_extension(metadata) == ".py"
}

/// The Python that asks debugpy to listen on `port`, leaving nothing behind in the person's namespace.
fn listen_code(port: u16) -> String {
    format!("import debugpy as _unluminous_debugpy\n_unluminous_debugpy.listen((\"127.0.0.1\", {port}))\ndel _unluminous_debugpy")
}

/// The Python that prints the file IPython runs `source` under. The source goes over as a JSON string,
/// which is a Python string literal too.
fn file_name_code(source: &str) -> String {
    let literal = serde_json::to_string(source).unwrap_or_else(|_| "\"\"".to_owned());
    format!("import ipykernel.compiler as _unluminous_compiler\nprint(_unluminous_compiler.get_file_name({literal}), end=\"\")\ndel _unluminous_compiler")
}

/// How long a cell about to be debugged waits for the debugger to answer its breakpoints.
const BREAKPOINT_ANSWER_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);

/// A port on the loopback interface that nothing is using this instant.
fn a_free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).ok()?;
    listener.local_addr().ok().map(|address| address.port())
}

/// Look at one kernel event for what it means to debugging. Answers the step it completes, if any.
pub fn take_a_debug_event(tab: &mut NotebookTab, event: &Event) -> Option<DebugStep> {
    let debugging = tab.debugging.as_mut()?;
    match event {
        Event::ExecuteReply { request, status, .. }
            if request.is_some() && *request == debugging.listening =>
        {
            debugging.listening = None;
            match status.as_str() {
                "ok" => {
                    debugging.listens = true;
                    Some(DebugStep::Listening)
                }
                _ => Some(DebugStep::Failed("debugpy would not listen in the kernel. Is it installed? It comes with ipykernel.".to_owned())),
            }
        }
        Event::Stream { request: Some(request), text, .. } => {
            let cell = debugging.asking.remove(request)?;
            debugging.files.insert(cell.clone(), PathBuf::from(text.trim()));
            Some(DebugStep::FileKnown(cell))
        }
        Event::Error { request: Some(request), ename, evalue, .. }
            if debugging.asking.contains_key(request)
                || debugging.listening.as_deref() == Some(request) =>
        {
            debugging.asking.remove(request);
            Some(DebugStep::Failed(format!(
                "The kernel could not be made ready to debug: {ename}: {evalue}"
            )))
        }
        Event::Restarted { .. } | Event::Died { .. } | Event::Stopped => {
            // A new kernel process has no debugpy listening and its cells will run under new names.
            tab.debugging = None;
            None
        }
        _ => None,
    }
}

impl UnluminousApp {
    /// Debug the chosen cell of the notebook at `index`: start the kernel if it is not running, have
    /// it listen for the debugger, attach, learn the cell's file, send its breakpoints, and run it.
    pub(crate) fn debug_the_chosen_cell(&mut self, index: usize) -> Result<String, String> {
        let chosen = self.chosen_cells(index);
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else {
            return Err("not a notebook".to_owned());
        };
        if !can_be_debugged(&tab.model.metadata) {
            return Err(
                "Debugging a cell needs a Python kernel, because it uses debugpy inside ipykernel. This notebook runs another language."
                    .to_owned(),
            );
        }
        let Some(cell) = tab.model.cells.get(chosen.start) else {
            return Err("No cell is chosen.".to_owned());
        };
        if cell.kind != CellKind::Code {
            return Err("Only a code cell can be debugged.".to_owned());
        }
        let id = cell.id.clone();
        tab.debugging.get_or_insert_with(CellDebug::default).waiting = Some(id);
        self.files.make_permanent(index);
        self.carry_on_debugging(index);
        Ok(format!("Debugging cell {}.", chosen.start + 1))
    }

    /// Take the next step towards debugging the waiting cell: start the kernel, have debugpy listen,
    /// attach a session, then ask for the cells' file names. Does nothing while a request of ours is
    /// still outstanding, so it is safe to call again whenever the kernel says anything.
    pub(crate) fn carry_on_debugging(&mut self, index: usize) {
        let started =
            self.files.at(index).notebook.as_deref().is_some_and(|tab| match &tab.kernel {
                KernelSlot::Live(kernel) => {
                    matches!(kernel.state(), KernelState::Idle | KernelState::Busy)
                }
                _ => false,
            });
        if !started {
            self.start_a_kernel_if_needed(index);
            return;
        }
        let attached = self.debug.as_ref().is_some_and(DebugState::is_alive);
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(debugging) = tab.debugging.as_ref() else { return };
        if debugging.is_waiting_on_the_kernel() {
            return;
        }
        let waiting = debugging.waiting.clone();
        if waiting.as_deref().and_then(|cell| tab.index_of(cell)).is_none() {
            // Nothing to debug, or the cell was deleted while the kernel was getting ready.
            if let Some(debugging) = tab.debugging.as_mut() {
                debugging.waiting = None;
            }
            return;
        }
        let Some(debugging) = tab.debugging.as_ref() else { return };
        if !debugging.listens {
            self.ask_debugpy_to_listen(index);
            return;
        }
        if !attached {
            let port = debugging.port;
            if !self.attach_to_the_kernel(port) {
                if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
                    tab.debugging = None;
                }
                return;
            }
        }
        self.ask_for_the_cells_files(index);
    }

    /// Ask the kernel to start debugpy listening on a free port.
    fn ask_debugpy_to_listen(&mut self, index: usize) {
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(port) = a_free_port() else {
            tab.debugging = None;
            self.message = Some("No free port could be found for the debugger.".to_owned());
            return;
        };
        let listening = tab.kernel().map(|kernel| kernel.execute_silently(&listen_code(port)));
        if let Some(debugging) = tab.debugging.as_mut() {
            debugging.port = port;
            debugging.listening = listening;
        }
    }

    /// Ask the kernel which file each cell to be debugged runs under: the waiting cell, and every code
    /// cell holding a breakpoint, so a function another cell defined stops where its breakpoint is
    /// when the debugged cell calls it.
    fn ask_for_the_cells_files(&mut self, index: usize) {
        let text = self.files.at(index).document.text().to_string();
        let marks: Vec<usize> =
            self.files.at(index).document.breakpoints().iter().map(|mark| mark.offset).collect();
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(waiting) = tab.debugging.as_ref().and_then(|debugging| debugging.waiting.clone())
        else {
            return;
        };
        let questions: Vec<(String, String)> = (0..tab.len())
            .filter(|cell| tab.spans[*cell].kind == CellKind::Code)
            .filter(|cell| {
                let body = &tab.spans[*cell].body_bytes;
                tab.model.cells[*cell].id == waiting
                    || marks.iter().any(|mark| *mark >= body.start && *mark <= body.end)
            })
            .map(|cell| {
                let source = unluminous_jupyter::text::cell_source(&text, &tab.spans[cell]);
                (tab.model.cells[cell].id.clone(), file_name_code(&source))
            })
            .collect();
        let asked: Vec<(String, String)> = questions
            .into_iter()
            .filter_map(|(id, question)| {
                tab.kernel().map(|kernel| (kernel.execute_quietly(&question), id))
            })
            .collect();
        if let Some(debugging) = tab.debugging.as_mut() {
            debugging.asking.extend(asked);
        }
    }

    /// Act on a step the kernel's events completed.
    pub(crate) fn take_a_debug_step(&mut self, index: usize, step: DebugStep) {
        match step {
            DebugStep::Listening => self.carry_on_debugging(index),
            DebugStep::FileKnown(cell) => {
                self.write_a_cells_file(index, &cell);
                let (all_known, waiting) = match self
                    .files
                    .at(index)
                    .notebook
                    .as_deref()
                    .and_then(|tab| tab.debugging.as_ref())
                {
                    Some(debugging) => (debugging.asking.is_empty(), debugging.waiting.clone()),
                    None => (false, None),
                };
                if let (true, Some(waiting)) = (all_known, waiting) {
                    self.run_the_cell_under_the_debugger(index, &waiting);
                }
            }
            DebugStep::Failed(problem) => {
                if let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() {
                    tab.debugging = None;
                }
                self.message = Some(problem);
            }
        }
    }

    /// Open a debug session on debugpy listening in the kernel, with an `attach` rather than a launch.
    /// Answers whether the session opened.
    fn attach_to_the_kernel(&mut self, port: u16) -> bool {
        let command = unluminous_dap::AdapterCommand {
            program: None,
            args: Vec::new(),
            working_directory: None,
            env: Vec::new(),
            transport: unluminous_dap::Transport::Port(port),
        };
        // `justMyCode` off, so stepping into a function another cell defined stops there, which is the
        // one place the reference editor's own notebook debugger is weakest.
        let body = serde_json::json!({ "request": "attach", "justMyCode": false, "redirectOutput": false });
        let configuration =
            crate::services::run_configurations::Configuration::new("Notebook cell", "");
        let waker = self.waker();
        match DebugState::start("python", &command, body, "", configuration, waker) {
            Ok(state) => {
                self.debug = Some(state);
                self.show_the_debug_tile(true);
                true
            }
            Err(problem) => {
                self.message =
                    Some(format!("The debugger could not attach to the kernel: {problem}"));
                false
            }
        }
    }

    /// The cell's file is known: send its breakpoints, adding one on its first line when it has none,
    /// and run it.
    fn run_the_cell_under_the_debugger(&mut self, index: usize, cell: &str) {
        let before = self.debug.as_ref().map(DebugState::breakpoint_answers).unwrap_or(0);
        let files = self.send_a_notebooks_breakpoints(index, Some(cell)) as u64;
        // The cell waits for the answers: sent straight away, it could start before debugpy has
        // bound them, and run through a breakpoint it was given. A session that has only just
        // attached answers once its handshake is done, which took seconds on a busy machine, so
        // the limit is long. It only exists so a debugger that never answers does not hold the cell
        // for ever, and running at it says so.
        let deadline = std::time::Instant::now() + BREAKPOINT_ANSWER_LIMIT;
        if let Some(debugging) =
            self.files.at_mut(index).notebook.as_deref_mut().and_then(|tab| tab.debugging.as_mut())
        {
            debugging.waiting = None;
            debugging.run_when = Some((cell.to_owned(), before + files, deadline));
        }
    }

    /// Run the cell waiting on its breakpoint answers, once they are in or the deadline has passed.
    pub(crate) fn run_a_cell_the_debugger_is_ready_for(&mut self, index: usize) {
        let answers = self.debug.as_ref().map(DebugState::breakpoint_answers).unwrap_or(0);
        let Some(tab) = self.files.at_mut(index).notebook.as_deref_mut() else { return };
        let Some(debugging) = tab.debugging.as_mut() else { return };
        let Some((cell, wanted, deadline)) = debugging.run_when.clone() else { return };
        if answers < wanted && std::time::Instant::now() < deadline {
            if let Some(context) = &self.context {
                context.request_repaint_after(std::time::Duration::from_millis(50));
            }
            return;
        }
        debugging.run_when = None;
        let Some(at) = tab.index_of(&cell) else { return };
        self.run_notebook_cells(index, at..at + 1);
        self.message = Some(match answers < wanted {
            true => format!(
                "Debugging cell {}. The debugger did not confirm its breakpoints, so it may run past them.",
                at + 1
            ),
            false => format!("Debugging cell {}.", at + 1),
        });
    }

    /// Write a cell's source to the file it runs under, which is Jupyter's own `dumpCell`: debugpy reads
    /// a breakpoint's file to place it, and a stop has source to show only when the file is there.
    fn write_a_cells_file(&mut self, index: usize, cell: &str) {
        let Some(at) = self.files.at(index).notebook.as_deref().and_then(|tab| tab.index_of(cell))
        else {
            return;
        };
        let file = self.files.at(index);
        if let Some(tab) = file.notebook.as_deref() {
            let path =
                tab.debugging.as_ref().and_then(|debugging| debugging.files.get(cell)).cloned();
            let text = file.document.text().to_string();
            let source = unluminous_jupyter::text::cell_source(&text, &tab.spans[at]).into_owned();
            if let Some(path) = path {
                if let Some(folder) = path.parent() {
                    let _ = std::fs::create_dir_all(folder);
                }
                if let Err(problem) =
                    crate::services::store::write_atomically(&path, source.as_bytes())
                {
                    self.message =
                        Some(format!("The cell could not be written for the debugger: {problem}"));
                }
            }
        }
    }

    /// Tell the debugger every cell's breakpoints, each against the file the cell runs under and
    /// counted within the cell. `first_line_for` is a cell about to be debugged, which gets a
    /// breakpoint on its first line when it has none of its own.
    pub(crate) fn send_a_notebooks_breakpoints(
        &mut self,
        index: usize,
        first_line_for: Option<&str>,
    ) -> usize {
        let file = self.files.at(index);
        let Some(tab) = file.notebook.as_deref() else { return 0 };
        let Some(debugging) = tab.debugging.as_ref() else { return 0 };
        let mut sends: Vec<(PathBuf, Vec<(usize, unluminous_dap::SourceBreakpoint)>)> = Vec::new();
        for (id, path) in &debugging.files {
            let Some(at) = tab.index_of(id) else { continue };
            let add_first = first_line_for == Some(id.as_str());
            let lines = breakpoints_of_a_cell(&file.document, &tab.spans[at], add_first);
            sends.push((path.clone(), lines));
        }
        let count = sends.len();
        match self.debug.as_mut() {
            Some(debug) => {
                for (path, lines) in sends {
                    debug.set_breakpoints(&path, lines);
                }
                count
            }
            None => 0,
        }
    }

    /// The notebook tab and the paragraph of it a stop in `path` at the one-based `line` is, when the
    /// path is a file a notebook cell runs under.
    pub(crate) fn notebook_stop(&self, path: &Path, line: usize) -> Option<(usize, usize)> {
        let plain =
            |path: &Path| unluminous_terminal::paths::plain(path).to_string_lossy().to_lowercase();
        let wanted = plain(path);
        for index in 0..self.files.len() {
            let Some(tab) = self.files.at(index).notebook.as_deref() else { continue };
            let Some(debugging) = tab.debugging.as_ref() else { continue };
            for (id, file) in &debugging.files {
                if plain(file) != wanted {
                    continue;
                }
                let at = tab.index_of(id)?;
                let span = &tab.spans[at];
                let paragraph =
                    (span.body.start + line.saturating_sub(1)).min(span.body.end.saturating_sub(1));
                return Some((index, paragraph));
            }
        }
        None
    }
}

/// The enabled breakpoints inside one cell, each with its offset in the text and its line counted
/// from one within the cell. A cell with none gets one on its first line when `add_first` is set.
fn breakpoints_of_a_cell(
    document: &unluminous_core::Document,
    span: &unluminous_jupyter::text::CellSpan,
    add_first: bool,
) -> Vec<(usize, unluminous_dap::SourceBreakpoint)> {
    let mut lines: Vec<(usize, unluminous_dap::SourceBreakpoint)> = document
        .breakpoints()
        .iter()
        .filter(|breakpoint| {
            breakpoint.enabled
                && breakpoint.offset >= span.body_bytes.start
                && breakpoint.offset <= span.body_bytes.end
        })
        .map(|breakpoint| {
            let line = document.text().byte_to_line(breakpoint.offset) - span.body.start + 1;
            let wanted = unluminous_dap::SourceBreakpoint {
                line,
                condition: breakpoint.condition.clone(),
                log_message: breakpoint.log_message.clone(),
            };
            (breakpoint.offset, wanted)
        })
        .collect();
    if lines.is_empty() && add_first {
        let first =
            unluminous_dap::SourceBreakpoint { line: 1, condition: None, log_message: None };
        lines.push((span.body_bytes.start, first));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listen_and_file_name_questions_are_python_that_leaves_nothing_behind() {
        let listen = listen_code(5678);
        assert!(listen.contains("listen((\"127.0.0.1\", 5678))"));
        assert!(listen.ends_with("del _unluminous_debugpy"));
        let asked = file_name_code("print(\"a\")\nx = 'b'\n");
        assert!(asked.contains("get_file_name(\"print(\\\"a\\\")\\nx = 'b'\\n\")"), "{asked}");
        assert!(asked.ends_with("del _unluminous_compiler"));
    }

    #[test]
    fn the_kernels_answers_complete_the_steps_in_order() {
        let (mut tab, _) = NotebookTab::new(unluminous_jupyter::nbformat::empty());
        tab.debugging =
            Some(CellDebug { port: 9, listening: Some("r1".into()), ..CellDebug::default() });
        assert!(tab.debugging.as_ref().unwrap().is_waiting_on_the_kernel());
        let attach = take_a_debug_event(
            &mut tab,
            &Event::ExecuteReply {
                request: Some("r1".into()),
                status: "ok".into(),
                execution_count: None,
            },
        );
        assert_eq!(attach, Some(DebugStep::Listening));
        let debugging = tab.debugging.as_ref().unwrap();
        assert!(debugging.listens && !debugging.is_waiting_on_the_kernel());
        tab.debugging.as_mut().unwrap().asking.insert("r2".into(), "cell".into());
        let known = take_a_debug_event(
            &mut tab,
            &Event::Stream {
                request: Some("r2".into()),
                name: "stdout".into(),
                text: "C:\\tmp\\ipykernel_1\\42.py".into(),
            },
        );
        assert_eq!(known, Some(DebugStep::FileKnown("cell".into())));
        assert_eq!(
            tab.debugging.as_ref().unwrap().files["cell"],
            PathBuf::from("C:\\tmp\\ipykernel_1\\42.py")
        );
        take_a_debug_event(&mut tab, &Event::Stopped);
        assert!(tab.debugging.is_none(), "a kernel that stopped takes its debugger with it");
    }
}
