//! The Jupyter kernel a notebook cell runs in, started through the machine's own Python.
//!
//! **Why a Python script and not ZeroMQ.** A Jupyter kernel speaks ZeroMQ. Linking a ZeroMQ library
//! would add a C dependency to every build, and the macOS cross build from Windows is the place that
//! costs most. Every machine that can run a kernel already has Python and `jupyter_client` (`pip
//! install ipykernel` brings both), and `jupyter_client` is the reference implementation of the
//! protocol. So the editor embeds `kernel/bridge.py` in its binary and runs it with that Python. The
//! script starts the kernel and relays between it and Unluminous: commands arrive on its standard
//! input as JSON, one object per line, and events leave on its standard output the same way.
//!
//! **How it is arranged.** The same as `unluminous_dap::Client`. A child process, one reader thread
//! that parses each line into an [`Event`] and pushes it on a channel, and a [`Waker`] the window
//! passes in so that it repaints when something arrives. The window drains the channel with
//! [`Kernel::events`] once per frame. There is no async runtime, and the window never waits for the
//! kernel: starting returns at once, and the kernel's readiness arrives as an event, because a kernel
//! takes a second or more to start and a window that waited would stop drawing.
//!
//! Writing to the bridge happens on the window's thread. Each command is a few hundred bytes to a
//! pipe, which does not block in practice.
//!
//! **Nothing is left running.** When the editor ends, the bridge sees its standard input close and
//! shuts the kernel down. Dropping a [`Kernel`] asks for the same, waits a few seconds, and then
//! stops the bridge and the kernel by process id.

mod python;

pub use python::{find_pythons, install_command, list_kernelspecs, probe, Python};

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The bridge script, embedded so the editor needs no files beside its own program.
const BRIDGE: &str = include_str!("kernel/bridge.py");

/// How many lines of the bridge's standard error are kept.
const STDERR_LINES: usize = 50;

/// How long dropping a [`Kernel`] waits for the bridge to shut the kernel down before it stops both.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(4);

/// A function the reader thread calls to have the window drawn again.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What the kernel is doing, as the window shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum KernelState {
    /// The bridge is running and the kernel has not yet said it is ready.
    Starting,
    /// The kernel is ready for the next cell.
    Idle,
    /// The kernel is running code.
    Busy,
    /// A restart was asked for and the new kernel has not yet said it is ready.
    Restarting,
    /// The kernel was shut down on request.
    Stopped,
    /// The kernel or the bridge ended without being asked to. The text says why.
    Dead(String),
}

/// What a kernel says about the language it runs, from its `kernel_info_reply`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LanguageInfo {
    /// The language name, such as `python`.
    pub name: String,
    /// The language version, such as `3.13.11`.
    pub version: String,
    /// The file extension for the language, including the dot, such as `.py`.
    pub file_extension: String,
    /// The MIME type of the language's source.
    pub mimetype: String,
    /// The Pygments lexer name, when the kernel gives one.
    pub pygments_lexer: Option<String>,
}

/// One variable in the kernel's user namespace.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    /// The variable's name.
    pub name: String,
    /// The name of the variable's type, such as `list` or `DataFrame`.
    pub type_name: String,
    /// The start of the variable's `repr`, at most 200 characters.
    pub value: String,
    /// The shape of an array or table, such as `(3, 2)`, when the value has one.
    pub shape: Option<String>,
    /// The length of a list, string, table or similar value, when it has one.
    pub size: Option<u64>,
}

/// A kernel the machine has installed, from its kernelspec.
#[derive(Debug, Clone, PartialEq)]
pub struct KernelSpec {
    /// The kernelspec name, such as `python3`.
    pub name: String,
    /// The name to show a person, such as `Python 3 (ipykernel)`.
    pub display_name: String,
    /// The language the kernel runs.
    pub language: String,
}

/// Something the kernel or the bridge sent. `request` is the id returned by the method that asked,
/// or `None` for output the kernel produced on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The kernel is running and ready. `pid` is the kernel's process id when it is known.
    Started { info: Value, pid: Option<u32> },
    /// The kernel's execution state changed. `state` is `busy`, `idle` or `starting`.
    Status { state: String },
    /// The kernel began running a cell and numbered it.
    ExecuteInput { request: Option<String>, execution_count: Option<u64> },
    /// Text written to standard output or standard error. `name` is `stdout` or `stderr`.
    Stream { request: Option<String>, name: String, text: String },
    /// Rich output such as an image or a plot.
    DisplayData {
        request: Option<String>,
        data: Value,
        metadata: Value,
        display_id: Option<String>,
    },
    /// New content for an earlier display that has the same `display_id`.
    UpdateDisplayData { request: Option<String>, data: Value, metadata: Value, display_id: String },
    /// The value of the last expression in a cell.
    ExecuteResult {
        request: Option<String>,
        execution_count: Option<u64>,
        data: Value,
        metadata: Value,
    },
    /// An exception. `traceback` has ANSI colour codes in it.
    Error { request: Option<String>, ename: String, evalue: String, traceback: Vec<String> },
    /// The kernel asks for the cell's earlier output to be cleared. `wait` defers it to the next output.
    ClearOutput { request: Option<String>, wait: bool },
    /// The kernel finished a cell. `status` is `ok`, `error` or `aborted`.
    ExecuteReply { request: Option<String>, status: String, execution_count: Option<u64> },
    /// The running cell called `input()`.
    InputRequest { request: Option<String>, prompt: String, password: bool },
    /// Completions for the code and cursor position sent to [`Kernel::complete`].
    CompleteReply {
        request: Option<String>,
        matches: Vec<String>,
        cursor_start: usize,
        cursor_end: usize,
    },
    /// Documentation for the name under the cursor sent to [`Kernel::inspect`].
    InspectReply { request: Option<String>, found: bool, data: Value },
    /// Whether the code is a complete statement. `status` is `complete`, `incomplete`, `invalid` or `unknown`.
    IsCompleteReply { request: Option<String>, status: String, indent: String },
    /// The variables in the user namespace.
    Variables { request: Option<String>, rows: Vec<Variable> },
    /// The kernels installed on the machine.
    KernelSpecs { request: Option<String>, specs: Vec<KernelSpec> },
    /// The kernel was restarted and is ready again. `pid` is the new kernel's process id.
    Restarted { info: Value, pid: Option<u32> },
    /// The kernel was shut down because it was asked to.
    Stopped,
    /// The kernel or the bridge ended without being asked to.
    Died { reason: String },
    /// The kernel could not be started. `missing` is `jupyter_client` or `ipykernel` when that package is the cause.
    Failed { message: String, missing: Option<String> },
}

/// A running kernel and the bridge process that holds it.
pub struct Kernel {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<Event>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    state: KernelState,
    language: Option<LanguageInfo>,
    info: Option<Value>,
    kernel_pid: Option<u32>,
    next_id: u64,
}

impl Kernel {
    /// Start the bridge and ask it to start a kernel. Returns at once: [`Event::Started`] or
    /// [`Event::Failed`] arrives later through [`Kernel::events`].
    ///
    /// `python` is the interpreter that has `jupyter_client`. `kernel_name` is a kernelspec name, or
    /// `None` for the default. `cwd` is the notebook's folder, where the kernel runs.
    pub fn start(
        python: &Path,
        kernel_name: Option<&str>,
        cwd: &Path,
        wake: Waker,
    ) -> std::io::Result<Kernel> {
        let mut command = python::command(python);
        command.args(["-u", "-X", "utf8", "-c", BRIDGE]).env("PYTHONIOENCODING", "utf-8");
        command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = spawn_one_at_a_time(&mut command)?;
        let missing = || std::io::Error::other("the kernel bridge was given no pipe");
        let stdin = child.stdin.take().ok_or_else(missing)?;
        let stdout = child.stdout.take().ok_or_else(missing)?;
        let stderr = child.stderr.take().ok_or_else(missing)?;
        let (sender, events) = std::sync::mpsc::channel();
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        spawn_event_reader(stdout, sender, wake);
        spawn_stderr_reader(stderr, tail.clone());
        let mut kernel = Kernel {
            child,
            stdin: Some(stdin),
            events,
            stderr: tail,
            state: KernelState::Starting,
            language: None,
            info: None,
            kernel_pid: None,
            next_id: 1,
        };
        let cwd_text = cwd.to_string_lossy().into_owned();
        kernel.send(json!({"cmd": "start", "kernel": kernel_name, "cwd": cwd_text}));
        Ok(kernel)
    }

    /// Run code as a cell: it is numbered and recorded in the history, and `input()` may ask a
    /// question. Returns the request id that the resulting events carry.
    pub fn execute(&mut self, code: &str) -> String {
        let fields =
            json!({"code": code, "silent": false, "store_history": true, "allow_stdin": true});
        self.request("execute", fields)
    }

    /// Run code without a number, history entry or `input()`. Returns the request id.
    pub fn execute_silently(&mut self, code: &str) -> String {
        let fields =
            json!({"code": code, "silent": true, "store_history": false, "allow_stdin": false});
        self.request("execute", fields)
    }

    /// Run code without a number, history entry or `input()`, whose printed output is still sent.
    ///
    /// What asking the kernel a question needs: Jupyter sends no output at all for a silent request,
    /// so an answer printed by one never arrives. `task-2220`.
    pub fn execute_quietly(&mut self, code: &str) -> String {
        let fields =
            json!({"code": code, "silent": false, "store_history": false, "allow_stdin": false});
        self.request("execute", fields)
    }

    /// Ask for completions at `cursor`, counted in Unicode code points as Jupyter counts. Returns the request id.
    pub fn complete(&mut self, code: &str, cursor: usize) -> String {
        self.request("complete", json!({"code": code, "cursor": cursor}))
    }

    /// Ask for documentation of the name at `cursor`. `detail` is 0 for a short answer and 1 for more.
    /// Returns the request id.
    pub fn inspect(&mut self, code: &str, cursor: usize, detail: u8) -> String {
        self.request("inspect", json!({"code": code, "cursor": cursor, "detail": detail}))
    }

    /// Ask whether `code` is a complete statement. Returns the request id.
    pub fn is_complete(&mut self, code: &str) -> String {
        self.request("is_complete", json!({"code": code}))
    }

    /// Ask for the variables in the kernel's namespace. Only Python kernels answer; others send an
    /// [`Event::Error`]. Returns the request id.
    pub fn variables(&mut self) -> String {
        self.request("variables", json!({}))
    }

    /// Ask for the kernelspecs installed for this Python. Returns the request id.
    pub fn kernelspecs(&mut self) -> String {
        self.request("kernelspecs", json!({}))
    }

    /// Answer an [`Event::InputRequest`] with the text the person typed.
    pub fn input_reply(&mut self, value: &str) {
        self.send(json!({"cmd": "input_reply", "value": value}));
    }

    /// Interrupt the running cell. The cell ends with a `KeyboardInterrupt` error.
    pub fn interrupt(&mut self) {
        self.send(json!({"cmd": "interrupt"}));
    }

    /// Restart the kernel, which forgets every variable. [`Event::Restarted`] arrives when it is ready.
    pub fn restart(&mut self) {
        self.state = KernelState::Restarting;
        self.send(json!({"cmd": "restart"}));
    }

    /// Ask the kernel to shut down. [`Event::Stopped`] arrives when it has.
    pub fn shutdown(&mut self) {
        self.send(json!({"cmd": "shutdown"}));
    }

    /// Everything that has arrived since the last call. The state, language and info the other
    /// methods report are updated from the events handed back.
    pub fn events(&mut self) -> Vec<Event> {
        let mut arrived = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            self.apply(&event);
            arrived.push(event);
        }
        arrived
    }

    /// What the kernel is doing.
    pub fn state(&self) -> &KernelState {
        &self.state
    }

    /// The language the kernel runs, once it has started.
    pub fn language(&self) -> Option<&LanguageInfo> {
        self.language.as_ref()
    }

    /// The kernel's whole `kernel_info_reply`, once it has started.
    pub fn info(&self) -> Option<&Value> {
        self.info.as_ref()
    }

    /// The last lines, at most fifty, that the bridge wrote to standard error.
    pub fn stderr_tail(&self) -> String {
        let lines = self
            .stderr
            .lock()
            .map(|held| held.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        lines.join("\n")
    }

    /// The operating system process id of the kernel, once it has started and when it is known.
    pub fn kernel_pid(&self) -> Option<u32> {
        self.kernel_pid
    }

    /// The operating system process id of the bridge.
    pub fn bridge_pid(&self) -> u32 {
        self.child.id()
    }

    /// Send a command that carries a request id, and return that id.
    fn request(&mut self, command: &str, fields: Value) -> String {
        let id = format!("k{}", self.next_id);
        self.next_id += 1;
        let mut message = fields;
        message["cmd"] = json!(command);
        message["id"] = json!(id);
        self.send(message);
        id
    }

    /// Write one command line to the bridge. A broken pipe means the bridge has gone, which the
    /// reader thread reports as [`Event::Died`], so the failure is not reported twice.
    fn send(&mut self, message: Value) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        let line = format!("{message}\n");
        if stdin.write_all(line.as_bytes()).and_then(|()| stdin.flush()).is_err() {
            self.stdin = None;
        }
    }

    /// Update the state, language and info from an event.
    fn apply(&mut self, event: &Event) {
        match event {
            Event::Started { info, pid } | Event::Restarted { info, pid } => {
                self.language = Some(language_of(info));
                self.info = Some(info.clone());
                self.kernel_pid = *pid;
                self.state = KernelState::Idle;
            }
            Event::Status { state } => self.apply_status(state),
            Event::Stopped => self.state = KernelState::Stopped,
            Event::Died { reason } if self.state != KernelState::Stopped => {
                self.state = KernelState::Dead(reason.clone());
            }
            Event::Failed { message, .. } => self.state = KernelState::Dead(message.clone()),
            _ => {}
        }
    }

    /// Follow the kernel's `busy` and `idle` reports, except while it is starting or restarting,
    /// when only the start or restart event says it is ready.
    fn apply_status(&mut self, state: &str) {
        if matches!(
            self.state,
            KernelState::Starting
                | KernelState::Restarting
                | KernelState::Stopped
                | KernelState::Dead(_)
        ) {
            return;
        }
        match state {
            "busy" => self.state = KernelState::Busy,
            "idle" => self.state = KernelState::Idle,
            _ => {}
        }
    }
}

impl Drop for Kernel {
    /// Ask the bridge to shut the kernel down, wait up to four seconds, then stop whatever is left.
    fn drop(&mut self) {
        self.send(json!({"cmd": "shutdown"}));
        self.stdin = None;
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(pid) = self.kernel_pid {
            python::kill_process(pid);
        }
    }
}

/// The language information in a `kernel_info_reply`.
fn language_of(info: &Value) -> LanguageInfo {
    let language = &info["language_info"];
    let text = |key: &str| language[key].as_str().unwrap_or_default().to_owned();
    LanguageInfo {
        name: text("name"),
        version: text("version"),
        file_extension: text("file_extension"),
        mimetype: text("mimetype"),
        pygments_lexer: language["pygments_lexer"].as_str().map(str::to_owned),
    }
}

/// Read the bridge's standard output, parse each line, and push the events on the channel.
///
/// When the output ends without a `stopped` or `failed` event the bridge has gone without being
/// asked to, and that is reported as [`Event::Died`].
fn spawn_event_reader(stdout: std::process::ChildStdout, sender: Sender<Event>, wake: Waker) {
    std::thread::Builder::new()
        .name("unluminous-jupyter".to_owned())
        .spawn(move || {
            let mut ended_cleanly = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let Some(event) =
                    serde_json::from_str::<Value>(&line).ok().and_then(|value| parse_event(&value))
                else {
                    continue;
                };
                ended_cleanly |= matches!(event, Event::Stopped | Event::Failed { .. });
                if sender.send(event).is_err() {
                    return;
                }
                wake();
            }
            if !ended_cleanly {
                let _ = sender.send(Event::Died { reason: "the kernel bridge ended".to_owned() });
            }
            wake();
        })
        .ok();
}

/// Keep the last [`STDERR_LINES`] lines of the bridge's standard error.
fn spawn_stderr_reader(stderr: std::process::ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    std::thread::Builder::new()
        .name("unluminous-jupyter-errors".to_owned())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let Ok(mut held) = tail.lock() else { return };
                held.push_back(line);
                while held.len() > STDERR_LINES {
                    held.pop_front();
                }
            }
        })
        .ok();
}

/// Turn one line from the bridge into an [`Event`]. `None` for an event this editor does not know.
fn parse_event(value: &Value) -> Option<Event> {
    let request = value["request"].as_str().map(str::to_owned);
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
    let count = |key: &str| value[key].as_u64();
    let pid = || value["pid"].as_u64().map(|number| number as u32);
    let display_id = || value["display_id"].as_str().map(str::to_owned);
    let event = match value["event"].as_str()? {
        "started" => Event::Started { info: value["info"].clone(), pid: pid() },
        "restarted" => Event::Restarted { info: value["info"].clone(), pid: pid() },
        "status" => Event::Status { state: text("state") },
        "execute_input" => {
            Event::ExecuteInput { request, execution_count: count("execution_count") }
        }
        "stream" => Event::Stream { request, name: text("name"), text: text("text") },
        "display_data" => Event::DisplayData {
            request,
            data: value["data"].clone(),
            metadata: value["metadata"].clone(),
            display_id: display_id(),
        },
        "update_display_data" => Event::UpdateDisplayData {
            request,
            data: value["data"].clone(),
            metadata: value["metadata"].clone(),
            display_id: display_id()?,
        },
        "execute_result" => Event::ExecuteResult {
            request,
            execution_count: count("execution_count"),
            data: value["data"].clone(),
            metadata: value["metadata"].clone(),
        },
        "error" => Event::Error {
            request,
            ename: text("ename"),
            evalue: text("evalue"),
            traceback: strings(&value["traceback"]),
        },
        "clear_output" => {
            Event::ClearOutput { request, wait: value["wait"].as_bool().unwrap_or(false) }
        }
        "execute_reply" => Event::ExecuteReply {
            request,
            status: text("status"),
            execution_count: count("execution_count"),
        },
        "input_request" => Event::InputRequest {
            request,
            prompt: text("prompt"),
            password: value["password"].as_bool().unwrap_or(false),
        },
        _ => return parse_reply_event(value, request),
    };
    Some(event)
}

/// The events that answer a question rather than describe output.
fn parse_reply_event(value: &Value, request: Option<String>) -> Option<Event> {
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
    let event = match value["event"].as_str()? {
        "complete_reply" => Event::CompleteReply {
            request,
            matches: strings(&value["matches"]),
            cursor_start: value["cursor_start"].as_u64().unwrap_or(0) as usize,
            cursor_end: value["cursor_end"].as_u64().unwrap_or(0) as usize,
        },
        "inspect_reply" => Event::InspectReply {
            request,
            found: value["found"].as_bool().unwrap_or(false),
            data: value["data"].clone(),
        },
        "is_complete_reply" => {
            Event::IsCompleteReply { request, status: text("status"), indent: text("indent") }
        }
        "variables" => Event::Variables {
            request,
            rows: value["rows"].as_array()?.iter().map(variable_of).collect(),
        },
        "kernelspecs" => Event::KernelSpecs { request, specs: python::specs_of(&value["specs"]) },
        "stopped" => Event::Stopped,
        "died" => Event::Died { reason: text("reason") },
        "failed" => Event::Failed {
            message: text("message"),
            missing: value["missing"].as_str().map(str::to_owned),
        },
        _ => return None,
    };
    Some(event)
}

/// A JSON array of strings, ignoring anything in it that is not a string.
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// One row of the bridge's `variables` answer.
fn variable_of(row: &Value) -> Variable {
    let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
    Variable {
        name: text("name"),
        type_name: text("type"),
        value: text("value"),
        shape: row["shape"].as_str().map(str::to_owned),
        size: row["size"].as_u64(),
    }
}

/// Start the bridge while no other thread in this program is starting a child process.
///
/// On Windows a child process inherits every handle that is inheritable at the moment it is created,
/// and the pipes of a child that another thread is creating are inheritable for that moment. Two
/// notebooks opening together then hold each other's pipes, and a bridge never sees its standard input
/// close. One lock around the start removes the overlap between notebooks.
fn spawn_one_at_a_time(command: &mut std::process::Command) -> std::io::Result<Child> {
    static SPAWNING: Mutex<()> = Mutex::new(());
    let _held = SPAWNING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    command.spawn()
}
