//! The worker thread: it owns the child process, keeps its own copy of every open document, and speaks
//! whichever protocol the adapter names through the [`Protocol`] trait.
//!
//! Three threads exist for each running server. The worker writes to the child's standard input and
//! reads one channel, which carries the caller's commands and what the reader thread decoded. The reader
//! thread decodes the child's standard output. A third reads its standard error into a small buffer so a
//! failure can say why. Nothing here is async and nothing blocks the caller: every call on [`Handle`]
//! puts a command on the channel and returns.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::convert::single_change;
use crate::framing::Decoder;
use crate::{
    Adapter, Item, Reply, ServerSpec, ServerState, Trigger, Waker, INITIALIZE_TIMEOUT,
    RESTARTS_AN_HOUR,
};

/// How often the worker wakes with nothing to do, to check the initialize deadline.
const TICK: Duration = Duration::from_millis(25);
/// The span in which [`RESTARTS_AN_HOUR`] restarts are counted.
const RESTART_WINDOW: Duration = Duration::from_secs(3600);
/// How long `stop` waits for the child to leave on its own before killing it.
const GRACE: Duration = Duration::from_secs(2);
/// The most standard error text kept for a failure's reason.
const STDERR_KEPT: usize = 2000;

/// Knobs a test changes. The product uses the defaults.
#[derive(Clone, Copy)]
pub(crate) struct Settings {
    pub initialize_timeout: Duration,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { initialize_timeout: INITIALIZE_TIMEOUT }
    }
}

/// The three kinds of question, which a newer ticket of the same kind for the same file supersedes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReqKind {
    Complete,
    Resolve,
    Signature,
}

/// What a request asks, as the protocols need it.
pub(crate) enum What {
    Complete { offset: usize, trigger: Trigger },
    Resolve { item: Box<Item>, offset: usize },
    Signature { offset: usize },
}

/// A question for the server, with the text its positions are in.
pub(crate) struct Request {
    pub ticket: u64,
    pub path: PathBuf,
    pub revision: u64,
    pub text: Arc<str>,
    pub what: What,
}

/// What one message from the server comes to: messages to send back, replies, a new state.
#[derive(Default)]
pub(crate) struct Incoming {
    pub send: Vec<Value>,
    pub replies: Vec<Reply>,
    pub state: Option<ServerState>,
    /// True once the server has answered `initialize` (LSP) or `configure` (tsserver).
    pub initialized: bool,
}

/// One protocol's messages. It owns request ids and which requests are in flight; the worker owns the
/// process, the documents and the tickets.
pub(crate) trait Protocol: Send {
    /// A message as bytes for the server's standard input.
    fn encode(&self, message: &Value) -> Vec<u8>;
    /// The messages that start a session in a project.
    fn initialize(&mut self, root: &Path) -> Vec<Value>;
    /// A document was opened, at its whole text.
    fn open(&mut self, path: &Path, language: &str, revision: u64, text: &str) -> Vec<Value>;
    /// One change to an open document: the bytes of `old` that go and the text that replaces them.
    fn change(
        &mut self,
        path: &Path,
        revision: u64,
        old: &str,
        range: &Range<usize>,
        replacement: &str,
    ) -> Vec<Value>;
    /// A document was closed.
    fn close(&mut self, path: &Path) -> Vec<Value>;
    /// A question.
    fn request(&mut self, request: Request) -> Vec<Value>;
    /// Withdraws a request the caller no longer wants, where the protocol can.
    fn cancel(&mut self, ticket: u64) -> Vec<Value>;
    /// What one decoded message means.
    fn incoming(&mut self, message: Value) -> Incoming;
    /// The messages that ask the server to leave.
    fn shutdown(&mut self) -> Vec<Value>;
}

/// A fresh protocol for an adapter.
fn make_protocol(adapter: Adapter) -> Box<dyn Protocol> {
    match adapter {
        Adapter::Lsp => Box::new(crate::lsp::Lsp::new()),
        Adapter::TsServer => Box::new(crate::tsserver::TsServer::new()),
    }
}

/// What the caller asks the worker for.
enum Cmd {
    Sync { path: PathBuf, language: String, revision: u64, text: Arc<str> },
    Close(PathBuf),
    Ask { ticket: u64, path: PathBuf, revision: u64, what: Ask },
    Stop,
}

enum Ask {
    Complete { offset: usize, trigger: Trigger },
    Resolve(Box<Item>),
    Signature { offset: usize },
}

/// Everything the worker's channel carries.
enum Input {
    Cmd(Cmd),
    Message(u64, Value),
    Eof(u64),
}

/// What the handle and the worker both see.
struct Shared {
    state: Mutex<ServerState>,
    replies: Mutex<VecDeque<Reply>>,
    latest: Mutex<HashMap<(ReqKind, PathBuf), u64>>,
    wake: Waker,
}

impl Shared {
    /// Changes the state, and tells the window when it changed.
    fn set_state(&self, state: ServerState) {
        {
            let mut current = self.state.lock().unwrap();
            if *current == state {
                return;
            }
            *current = state.clone();
        }
        self.replies.lock().unwrap().push_back(Reply::State(state));
        (self.wake)();
    }

    /// True when a newer question of the same kind has been asked for the same file.
    fn is_stale(&self, kind: ReqKind, path: &Path, ticket: u64) -> bool {
        self.latest
            .lock()
            .unwrap()
            .get(&(kind, path.to_path_buf()))
            .is_some_and(|newest| *newest > ticket)
    }
}

/// The caller's side of a server.
pub(crate) struct Handle {
    label: String,
    tx: Option<Sender<Input>>,
    shared: Arc<Shared>,
    ticket: u64,
    join: Option<JoinHandle<()>>,
}

impl Handle {
    /// Starts the worker with the default settings.
    pub(crate) fn start(spec: ServerSpec, wake: Waker) -> Handle {
        Handle::start_with(spec, wake, Settings::default())
    }

    /// Starts the worker with settings a test chose.
    pub(crate) fn start_with(spec: ServerSpec, wake: Waker, settings: Settings) -> Handle {
        let shared = shared_with(ServerState::Starting, wake);
        let (tx, rx) = channel();
        let label = spec.label.clone();
        let worker = Worker::new(spec, settings, shared.clone(), tx.clone(), rx);
        let join = std::thread::Builder::new()
            .name(format!("lsp-{label}"))
            .spawn(move || worker.run())
            .ok();
        Handle { label, tx: Some(tx), shared, ticket: 0, join }
    }

    /// A handle with no worker, whose state is `Absent`.
    pub(crate) fn absent(label: &str, why: &str) -> Handle {
        let shared = shared_with(ServerState::Absent(why.to_owned()), Arc::new(|| {}));
        Handle { label: label.to_owned(), tx: None, shared, ticket: 0, join: None }
    }

    fn send(&self, command: Cmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Input::Cmd(command));
        }
    }

    /// A new ticket, which makes every older one of the same kind for the file stale.
    fn next_ticket(&mut self, kind: ReqKind, path: &Path) -> u64 {
        self.ticket += 1;
        self.shared.latest.lock().unwrap().insert((kind, path.to_path_buf()), self.ticket);
        self.ticket
    }

    pub(crate) fn sync(&mut self, path: &Path, language: &str, revision: u64, text: Arc<str>) {
        self.send(Cmd::Sync {
            path: path.to_path_buf(),
            language: language.to_owned(),
            revision,
            text,
        });
    }

    pub(crate) fn close(&mut self, path: &Path) {
        self.send(Cmd::Close(path.to_path_buf()));
    }

    pub(crate) fn complete(
        &mut self,
        path: &Path,
        revision: u64,
        offset: usize,
        trigger: Trigger,
    ) -> u64 {
        let ticket = self.next_ticket(ReqKind::Complete, path);
        self.send(Cmd::Ask {
            ticket,
            path: path.to_path_buf(),
            revision,
            what: Ask::Complete { offset, trigger },
        });
        ticket
    }

    pub(crate) fn resolve(&mut self, path: &Path, item: &Item) -> u64 {
        let ticket = self.next_ticket(ReqKind::Resolve, path);
        self.send(Cmd::Ask {
            ticket,
            path: path.to_path_buf(),
            revision: 0,
            what: Ask::Resolve(Box::new(item.clone())),
        });
        ticket
    }

    pub(crate) fn signature(&mut self, path: &Path, revision: u64, offset: usize) -> u64 {
        let ticket = self.next_ticket(ReqKind::Signature, path);
        self.send(Cmd::Ask {
            ticket,
            path: path.to_path_buf(),
            revision,
            what: Ask::Signature { offset },
        });
        ticket
    }

    pub(crate) fn state(&self) -> ServerState {
        self.shared.state.lock().unwrap().clone()
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// Every reply waiting, oldest first, without the ones a newer question has superseded.
    pub(crate) fn poll(&mut self) -> Vec<Reply> {
        let waiting: Vec<Reply> = self.shared.replies.lock().unwrap().drain(..).collect();
        waiting.into_iter().filter(|reply| !self.superseded(reply)).collect()
    }

    fn superseded(&self, reply: &Reply) -> bool {
        match reply {
            Reply::Completions { ticket, path, .. } => {
                self.shared.is_stale(ReqKind::Complete, path, *ticket)
            }
            Reply::Signature { ticket, path, .. } => {
                self.shared.is_stale(ReqKind::Signature, path, *ticket)
            }
            _ => false,
        }
    }

    /// Asks the worker to shut the server down and waits for the worker to finish.
    pub(crate) fn stop(mut self) {
        self.send(Cmd::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Handle {
    /// A handle dropped without `stop` still asks its worker to leave, so no server outlives the window.
    fn drop(&mut self) {
        if self.join.is_some() {
            self.send(Cmd::Stop);
        }
    }
}

fn shared_with(state: ServerState, wake: Waker) -> Arc<Shared> {
    Arc::new(Shared {
        state: Mutex::new(state),
        replies: Mutex::new(VecDeque::new()),
        latest: Mutex::new(HashMap::new()),
        wake,
    })
}

/// The worker's copy of one open document.
struct Doc {
    language: String,
    revision: u64,
    text: Arc<str>,
}

/// A thread reading the child, and whether it has finished.
struct Reading {
    join: JoinHandle<()>,
    done: Arc<AtomicBool>,
}

struct Worker {
    spec: ServerSpec,
    settings: Settings,
    shared: Arc<Shared>,
    tx: Sender<Input>,
    rx: Receiver<Input>,
    proto: Box<dyn Protocol>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr: Arc<Mutex<String>>,
    readings: Vec<Reading>,
    generation: u64,
    initialized: bool,
    dead: bool,
    deadline: Option<Instant>,
    restarts: Vec<Instant>,
    docs: HashMap<PathBuf, Doc>,
    queued: Vec<Request>,
    issued: HashMap<u64, (ReqKind, PathBuf)>,
    completing: HashMap<PathBuf, u64>,
    last_complete: HashMap<PathBuf, (usize, Arc<str>)>,
}

impl Worker {
    fn new(
        spec: ServerSpec,
        settings: Settings,
        shared: Arc<Shared>,
        tx: Sender<Input>,
        rx: Receiver<Input>,
    ) -> Worker {
        let proto = make_protocol(spec.adapter);
        Worker {
            spec,
            settings,
            shared,
            tx,
            rx,
            proto,
            child: None,
            stdin: None,
            stderr: Arc::new(Mutex::new(String::new())),
            readings: Vec::new(),
            generation: 0,
            initialized: false,
            dead: false,
            deadline: None,
            restarts: Vec::new(),
            docs: HashMap::new(),
            queued: Vec::new(),
            issued: HashMap::new(),
            completing: HashMap::new(),
            last_complete: HashMap::new(),
        }
    }

    /// Starts the program, then serves commands until told to stop.
    fn run(mut self) {
        if let Err(why) = self.start_child() {
            self.fail(why);
        }
        loop {
            match self.rx.recv_timeout(TICK) {
                Ok(Input::Cmd(Cmd::Stop)) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(input) => self.on_input(input),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.check_deadline();
        }
        self.close_down();
    }

    /// Spawns the program with its pipes and threads, and sends the messages that start a session.
    fn start_child(&mut self) -> Result<(), String> {
        self.generation += 1;
        self.proto = make_protocol(self.spec.adapter);
        self.stderr.lock().unwrap().clear();
        let mut command = Command::new(&self.spec.program);
        command
            .args(&self.spec.args)
            .current_dir(&self.spec.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", self.spec.program.display()))?;
        end_with_this_process(&child);
        self.stdin = child.stdin.take();
        if let Some(out) = child.stdout.take() {
            self.readings.push(read_messages(out, self.generation, self.tx.clone()));
        }
        if let Some(err) = child.stderr.take() {
            self.readings.push(keep_stderr(err, self.stderr.clone()));
        }
        self.child = Some(child);
        self.initialized = false;
        self.deadline = Some(Instant::now() + self.settings.initialize_timeout);
        self.shared.set_state(ServerState::Starting);
        let messages = self.proto.initialize(&self.spec.root.clone());
        self.write(messages);
        Ok(())
    }

    /// Writes messages to the child. A write that fails is left for the end of the child's output to explain.
    fn write(&mut self, messages: Vec<Value>) {
        let Some(stdin) = self.stdin.as_mut() else { return };
        for message in messages {
            let bytes = self.proto.encode(&message);
            if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                self.stdin = None;
                return;
            }
        }
    }

    fn on_input(&mut self, input: Input) {
        match input {
            Input::Message(generation, message) if generation == self.generation && !self.dead => {
                let incoming = self.proto.incoming(message);
                self.apply(incoming);
            }
            Input::Eof(generation) if generation == self.generation && !self.dead => self.on_exit(),
            Input::Cmd(command) if !self.dead => self.on_command(command),
            _ => {}
        }
    }

    /// Carries out what a message from the server came to.
    fn apply(&mut self, incoming: Incoming) {
        self.write(incoming.send);
        if incoming.initialized && !self.initialized {
            self.initialized = true;
            self.deadline = None;
            self.after_initialized();
        }
        if let Some(state) = incoming.state {
            self.shared.set_state(state);
        }
        for reply in incoming.replies {
            self.publish(reply);
        }
    }

    /// Opens every document the caller has synced so far, and sends the questions that waited.
    fn after_initialized(&mut self) {
        let mut messages = Vec::new();
        for (path, doc) in &self.docs {
            messages.extend(self.proto.open(path, &doc.language, doc.revision, &doc.text));
        }
        self.write(messages);
        for request in std::mem::take(&mut self.queued) {
            let messages = self.proto.request(request);
            self.write(messages);
        }
    }

    fn on_command(&mut self, command: Cmd) {
        match command {
            Cmd::Sync { path, language, revision, text } => {
                self.on_sync(path, language, revision, text)
            }
            Cmd::Close(path) => {
                self.docs.remove(&path);
                if self.initialized {
                    let messages = self.proto.close(&path);
                    self.write(messages);
                }
            }
            Cmd::Ask { ticket, path, revision, what } => self.on_ask(ticket, path, revision, what),
            Cmd::Stop => {}
        }
    }

    /// Opens a document the first time, sends one incremental change after, and nothing for a repeat.
    fn on_sync(&mut self, path: PathBuf, language: String, revision: u64, text: Arc<str>) {
        let messages = match self.docs.get_mut(&path) {
            None => {
                let open =
                    self.initialized.then(|| self.proto.open(&path, &language, revision, &text));
                self.docs.insert(path, Doc { language, revision, text });
                open.unwrap_or_default()
            }
            Some(doc) if doc.revision == revision => Vec::new(),
            Some(doc) => {
                let (range, replacement) = single_change(&doc.text, &text);
                let sends = self.initialized && !(range.is_empty() && replacement.is_empty());
                let change = sends
                    .then(|| self.proto.change(&path, revision, &doc.text, &range, replacement));
                doc.revision = revision;
                doc.text = text;
                change.unwrap_or_default()
            }
        };
        self.write(messages);
    }

    /// Turns a question into a request for the protocol, cancelling the completion it supersedes.
    fn on_ask(&mut self, ticket: u64, path: PathBuf, revision: u64, what: Ask) {
        let Some(doc) = self.docs.get(&path) else { return };
        let (kind, text, what) = match what {
            Ask::Complete { offset, trigger } => {
                self.last_complete.insert(path.clone(), (offset, doc.text.clone()));
                (ReqKind::Complete, doc.text.clone(), What::Complete { offset, trigger })
            }
            Ask::Signature { offset } => {
                (ReqKind::Signature, doc.text.clone(), What::Signature { offset })
            }
            Ask::Resolve(item) => {
                let (offset, text) =
                    self.last_complete.get(&path).cloned().unwrap_or((0, doc.text.clone()));
                (ReqKind::Resolve, text, What::Resolve { item, offset })
            }
        };
        self.issued.insert(ticket, (kind, path.clone()));
        if kind == ReqKind::Complete {
            self.cancel_superseded_completion(&path, ticket);
        }
        let request = Request { ticket, path, revision, text, what };
        if self.initialized {
            let messages = self.proto.request(request);
            self.write(messages);
        } else {
            self.queued.push(request);
        }
    }

    /// Remembers the newest completion for a file and withdraws the one it replaces.
    fn cancel_superseded_completion(&mut self, path: &Path, ticket: u64) {
        if let Some(older) = self.completing.insert(path.to_path_buf(), ticket) {
            if self.initialized {
                let messages = self.proto.cancel(older);
                self.write(messages);
            }
        }
    }

    /// Hands a reply to the caller unless a newer question of its kind has been asked for its file.
    fn publish(&mut self, reply: Reply) {
        let ticket = match &reply {
            Reply::Completions { ticket, .. }
            | Reply::Resolved { ticket, .. }
            | Reply::Signature { ticket, .. } => *ticket,
            Reply::State(_) => return self.shared.set_state_from_reply(reply),
        };
        let Some((kind, path)) = self.issued.remove(&ticket) else { return };
        if kind == ReqKind::Complete && self.completing.get(&path) == Some(&ticket) {
            self.completing.remove(&path);
        }
        if self.shared.is_stale(kind, &path, ticket) {
            return;
        }
        self.shared.replies.lock().unwrap().push_back(reply);
        (self.shared.wake)();
    }

    /// The child's output ended. Starts it again unless it has crashed too often, and says why if not.
    fn on_exit(&mut self) {
        let status = self.child.take().and_then(|mut child| child.wait().ok());
        self.stdin = None;
        self.deadline = None;
        self.initialized = false;
        self.issued.clear();
        self.completing.clear();
        self.queued.clear();
        let why = self.exit_reason(status);
        let now = Instant::now();
        self.restarts.retain(|at| now.duration_since(*at) < RESTART_WINDOW);
        if self.restarts.len() >= RESTARTS_AN_HOUR {
            return self.fail(why);
        }
        self.restarts.push(now);
        if let Err(why) = self.start_child() {
            self.fail(why);
        }
    }

    /// What to say when the child has gone: its exit status and the end of what it wrote to standard error.
    fn exit_reason(&self, status: Option<std::process::ExitStatus>) -> String {
        let status = status.map_or_else(|| "unknown status".to_owned(), |s| s.to_string());
        let tail = self.stderr.lock().unwrap().trim().to_owned();
        if tail.is_empty() {
            format!("{} exited ({status})", self.spec.label)
        } else {
            format!("{} exited ({status}): {tail}", self.spec.label)
        }
    }

    /// Gives up on the server: the child is killed and the state says why.
    fn fail(&mut self, reason: String) {
        self.kill_child();
        self.dead = true;
        self.deadline = None;
        self.shared.set_state(ServerState::Failed(reason));
    }

    fn check_deadline(&mut self) {
        if self.deadline.is_some_and(|at| Instant::now() >= at) {
            let seconds = self.settings.initialize_timeout.as_secs_f32();
            self.fail(format!(
                "{} did not answer initialize in {seconds:.0} seconds",
                self.spec.label
            ));
        }
    }

    fn kill_child(&mut self) {
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Asks the server to leave, waits up to [`GRACE`], kills it, and joins the threads that read it.
    fn close_down(&mut self) {
        if self.initialized && !self.dead {
            let messages = self.proto.shutdown();
            self.write(messages);
        }
        self.stdin = None;
        let started = Instant::now();
        while started.elapsed() < GRACE
            && self.child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.kill_child();
        self.join_readers();
    }

    /// Joins the reading threads that have finished and lets go of any that a grandchild is holding open.
    fn join_readers(&mut self) {
        let started = Instant::now();
        for reading in self.readings.drain(..) {
            while !reading.done.load(Ordering::SeqCst)
                && started.elapsed() < Duration::from_millis(500)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            if reading.done.load(Ordering::SeqCst) {
                let _ = reading.join.join();
            }
        }
    }
}

impl Shared {
    /// A state that a protocol put in its replies is set like any other.
    fn set_state_from_reply(&self, reply: Reply) {
        if let Reply::State(state) = reply {
            self.set_state(state);
        }
    }
}

/// Decodes the child's standard output and sends each message to the worker, then says it ended.
fn read_messages(
    mut out: impl Read + Send + 'static,
    generation: u64,
    tx: Sender<Input>,
) -> Reading {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    let join = std::thread::spawn(move || {
        let mut decoder = Decoder::default();
        let mut buffer = [0u8; 16 * 1024];
        while let Ok(n) = out.read(&mut buffer) {
            if n == 0 {
                break;
            }
            for message in decoder.push(&buffer[..n]).into_iter().flatten() {
                if tx.send(Input::Message(generation, message)).is_err() {
                    break;
                }
            }
        }
        let _ = tx.send(Input::Eof(generation));
        flag.store(true, Ordering::SeqCst);
    });
    Reading { join, done }
}

/// Keeps the end of the child's standard error, for the reason a failure gives.
fn keep_stderr(mut err: impl Read + Send + 'static, kept: Arc<Mutex<String>>) -> Reading {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    let join = std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = err.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let mut text = kept.lock().unwrap();
            text.push_str(&String::from_utf8_lossy(&buffer[..n]));
            if text.len() > STDERR_KEPT {
                let mut cut = text.len() - STDERR_KEPT;
                while !text.is_char_boundary(cut) {
                    cut += 1;
                }
                text.drain(..cut);
            }
        }
        flag.store(true, Ordering::SeqCst);
    });
    Reading { join, done }
}

/// Puts a server in a job object that ends it when this process ends.
///
/// `close_down` asks a server to leave and then kills it, but none of that runs when the window is
/// killed or crashes, and Windows does not end a process's children with it: a rust-analyzer started by
/// a window that was killed went on running with no parent. One job is made for the process, set to
/// kill everything in it when its last handle closes, and the handle is never closed, so the operating
/// system closes it as the process ends. A job that cannot be made or joined leaves the server as it
/// was, which is the state before this existed.
///
/// @param child - the server just started
fn end_with_this_process(child: &std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::sync::OnceLock;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        /// The job's handle as a number, so it can live in a static. Zero when no job could be made.
        static JOB: OnceLock<usize> = OnceLock::new();
        let job = *JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let set = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if set == 0 {
                return 0;
            }
            job as usize
        });
        if job != 0 {
            unsafe {
                AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = child;
}

/// Stops a console window appearing for the server on Windows.
fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// When this test binary is started as a server that never answers, this test is that server.
    #[test]
    fn hang_entry() {
        if std::env::args().any(|a| a == "script=hang") {
            let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
            std::process::exit(0);
        }
    }

    fn hanging_spec() -> ServerSpec {
        ServerSpec {
            adapter: Adapter::Lsp,
            root: std::env::temp_dir(),
            program: std::env::current_exe().unwrap(),
            args: ["--exact", "worker::tests::hang_entry", "--nocapture", "script=hang"]
                .map(String::from)
                .to_vec(),
            label: "hanging".to_owned(),
        }
    }

    #[test]
    fn a_server_that_never_answers_initialize_is_failed() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let settings = Settings { initialize_timeout: Duration::from_millis(300) };
        let mut handle = Handle::start_with(
            hanging_spec(),
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
            settings,
        );
        assert_eq!(handle.state(), ServerState::Starting);
        let started = Instant::now();
        while !matches!(handle.state(), ServerState::Failed(_))
            && started.elapsed() < Duration::from_secs(10)
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ServerState::Failed(why) = handle.state() else {
            panic!("not failed: {:?}", handle.state())
        };
        assert!(why.contains("did not answer initialize"), "{why}");
        assert!(wakes.load(Ordering::SeqCst) >= 1);
        assert!(handle.poll().iter().any(|r| matches!(r, Reply::State(ServerState::Failed(_)))));
        handle.stop();
    }

    #[test]
    fn an_absent_handle_says_why_and_answers_nothing() {
        let mut handle = Handle::absent("rust-analyzer", "rustup component add rust-analyzer");
        assert_eq!(
            handle.state(),
            ServerState::Absent("rustup component add rust-analyzer".to_owned())
        );
        let path = Path::new("a.rs");
        handle.sync(path, "rust", 1, Arc::from("fn main() {}"));
        let ticket = handle.complete(path, 1, 0, Trigger::Invoked);
        assert_eq!(ticket, 1);
        assert!(handle.poll().is_empty());
        handle.stop();
    }

    #[test]
    fn a_program_that_cannot_start_is_failed_with_the_reason() {
        let spec =
            ServerSpec { program: PathBuf::from("definitely-not-a-program-xyz"), ..hanging_spec() };
        let handle = Handle::start(spec, Arc::new(|| {}));
        let started = Instant::now();
        while !matches!(handle.state(), ServerState::Failed(_))
            && started.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let ServerState::Failed(why) = handle.state() else { panic!("not failed") };
        assert!(why.contains("could not start"), "{why}");
        handle.stop();
    }
}
