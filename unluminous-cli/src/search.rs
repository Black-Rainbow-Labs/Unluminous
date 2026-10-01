//! The `search` area: the code index host, and how a caller reaches it
//! (`tasks/task-2138-unluminous-code-index-tdd.md` §4 and §6.10).
//!
//! **One host per checkout owns the index.** Inillucent lets several processes open a file, but a reader
//! waits for a writer, and two watchers on one tree would fight; so whichever process takes the lock in
//! the index folder first is the host, and every other caller talks to it over a loopback socket with a
//! token, the way `unluminous-cli` talks to a window. The host file beside the lock says the port and the
//! token.
//!
//! **Who hosts depends on how long the caller lives.** A window and an MCP server live for as long as
//! somebody is working, so they host in their own process and answer their own calls with no socket at
//! all. A one-off `unluminous-cli search ...` would throw a host away the moment it printed, so it starts
//! a detached `unluminous-cli search serve` instead and asks it; the next call finds it running.
//!
//! **A query never fails because the index is missing.** When no host can be reached or started,
//! `search find` is answered by scanning the files directly and says `"index":"none"`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};
use unluminous_index::index::Index;
use unluminous_index::store::index_folder;
use unluminous_index::verbs;

use crate::protocol::{code, Reply, Request};

/// How long a headless host waits with no request before it stops.
pub const IDLE_EXIT: Duration = Duration::from_secs(2 * 60 * 60);
/// How long a caller waits for a host it started to come up.
const START_WAIT: Duration = Duration::from_secs(20);

/// Where a caller's index host comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hosting {
    /// Host in this process when no other host is running. For the window and `mcp serve`.
    InProcess,
    /// Start a detached `unluminous-cli search serve` when no host is running. For a one-off command.
    Spawn,
}

/// A running host: the index, the port it answers on, and the lock that makes it the only one.
pub struct Host {
    pub index: Arc<Index>,
    pub port: u16,
    pub token: String,
    last_request: Arc<Mutex<Instant>>,
    _lock: File,
}

/// What the host file in the index folder says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFile {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub root: String,
}

/// The hosts this process runs, by root.
fn hosts() -> &'static Mutex<HashMap<PathBuf, Arc<Host>>> {
    static HOSTS: OnceLock<Mutex<HashMap<PathBuf, Arc<Host>>>> = OnceLock::new();
    HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The project a search is about: the given folder, or the nearest folder at or above the working folder
/// that holds `.git`, or the working folder itself.
///
/// @param given - a `--root` argument, if one was given
pub fn project_root(given: Option<&str>) -> PathBuf {
    if let Some(given) = given.filter(|g| !g.trim().is_empty()) {
        return canonical(Path::new(given));
    }
    let here = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut at = Some(here.as_path());
    while let Some(folder) = at {
        if folder.join(".git").exists() {
            return canonical(folder);
        }
        at = folder.parent();
    }
    canonical(&here)
}

/// A folder's canonical path without Windows' `\\?\` prefix.
///
/// @param path - the folder
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).map(|p| unluminous_index::index::strip_verbatim(&p)).unwrap_or_else(|_| path.to_path_buf())
}

/// A token nobody else can guess: the hash of the time, the process and the root.
///
/// @param root - the root
fn new_token(root: &Path) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let seed = format!("{nanos}:{}:{}", std::process::id(), root.display());
    blake3::hash(seed.as_bytes()).to_hex().as_str()[..32].to_owned()
}

/// Reads the host file of a root, if there is one.
///
/// @param root - the root
pub fn read_host_file(root: &Path) -> Option<HostFile> {
    let text = std::fs::read_to_string(index_folder(root).join("host.conf")).ok()?;
    let get = |name: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{name} = ")).map(str::trim).map(str::to_owned));
    Some(HostFile { pid: get("pid")?.parse().ok()?, port: get("port")?.parse().ok()?, token: get("token")?, root: get("root").unwrap_or_default() })
}

impl Host {
    /// Becomes the host of a root: takes the lock, opens the index, listens on a loopback port and
    /// writes the host file. Fails when another process already holds the lock.
    ///
    /// @param root - the root
    pub fn start(root: &Path) -> Result<Arc<Host>, String> {
        let folder = index_folder(root);
        std::fs::create_dir_all(&folder).map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
        let lock = File::options().create(true).truncate(false).write(true).open(folder.join("host.lock")).map_err(|e| e.to_string())?;
        lock.try_lock().map_err(|_| "another process is already the index host for this folder".to_owned())?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let token = new_token(root);
        let index = Index::open(root);
        let host = Arc::new(Host { index, port, token: token.clone(), last_request: Arc::new(Mutex::new(Instant::now())), _lock: lock });
        let file = format!("pid = {}\nport = {port}\ntoken = {token}\nroot = {}\n", std::process::id(), root.display());
        std::fs::write(folder.join("host.conf"), file).map_err(|e| e.to_string())?;
        let serving = Arc::clone(&host);
        std::thread::Builder::new().name("unluminous-index-listen".into()).spawn(move || serving.listen(listener)).map_err(|e| e.to_string())?;
        Ok(host)
    }

    /// Answers connections, one thread each, one request a connection.
    ///
    /// @param listener - the bound socket
    fn listen(self: Arc<Self>, listener: TcpListener) {
        for stream in listener.incoming().flatten() {
            let host = Arc::clone(&self);
            let _ = std::thread::Builder::new().name("unluminous-index-call".into()).spawn(move || host.serve_one(stream));
        }
    }

    /// Reads one request from a connection, checks its token, answers it and writes the reply.
    ///
    /// @param stream - the connection
    fn serve_one(&self, stream: TcpStream) {
        let Ok(mut writer) = stream.try_clone() else { return };
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_err() {
            return;
        }
        let reply = match serde_json::from_str::<Value>(line.trim()).ok().as_ref().and_then(Request::from_json) {
            Some(request) if request.token == self.token => self.answer(&request.command, &request.arguments),
            Some(request) => Reply::failed(&request.command, code::REFUSED, "The token does not match this index host."),
            None => Reply::failed("", code::USAGE, "That was not a request."),
        };
        let _ = writeln!(writer, "{}", reply.to_json());
        let _ = writer.flush();
    }

    /// Answers one `search` command in this process.
    ///
    /// @param command - the wire name, such as `search.find`
    /// @param arguments - the command's arguments
    pub fn answer(&self, command: &str, arguments: &Map<String, Value>) -> Reply {
        *self.last_request.lock().expect("idle clock") = Instant::now();
        let verb = command.strip_prefix("search.").unwrap_or(command);
        match verb {
            "serve" => Reply::done(command, "", json!({ "host": self.describe(), "text": format!("the index host is already running on port {}\n", self.port) })),
            "status" => match verbs::answer(&self.index, "status", arguments) {
                Ok(mut value) => {
                    value["host"] = self.describe();
                    Reply::done(command, "", value)
                }
                Err(refusal) => Reply::failed(command, refusal.code, refusal.message),
            },
            _ => match verbs::answer(&self.index, verb, arguments) {
                Ok(value) => Reply::done(command, "", value),
                Err(refusal) => Reply::failed(command, refusal.code, refusal.message),
            },
        }
    }

    /// The host's own facts for `search status`.
    fn describe(&self) -> Value {
        json!({ "pid": std::process::id(), "port": self.port, "inProcess": true })
    }

    /// Blocks until the host has had no request for `IDLE_EXIT`, then removes its host file.
    pub fn serve_until_idle(&self) {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            if self.last_request.lock().expect("idle clock").elapsed() > IDLE_EXIT {
                let _ = std::fs::remove_file(index_folder(self.index.root()).join("host.conf"));
                return;
            }
        }
    }
}

/// The host this process runs for a root, starting one when `hosting` allows it and no other process
/// is the host.
///
/// @param root - the root
fn in_process(root: &Path) -> Option<Arc<Host>> {
    let mut hosts = hosts().lock().expect("hosts");
    if let Some(host) = hosts.get(root) {
        return Some(Arc::clone(host));
    }
    let host = Host::start(root).ok()?;
    hosts.insert(root.to_path_buf(), Arc::clone(&host));
    Some(host)
}

/// Asks another process's host, if the host file names one that answers.
///
/// @param root - the root
/// @param command - the wire name
/// @param arguments - the arguments
/// @param timeout - how long to wait
fn ask_running(root: &Path, command: &str, arguments: &Map<String, Value>, timeout: Duration) -> Option<Reply> {
    let file = read_host_file(root)?;
    if file.pid == std::process::id() {
        return None;
    }
    let request = Request::new(&file.token, command, arguments.clone());
    crate::protocol::ask(file.port, &request, timeout).ok()
}

/// Starts a detached `unluminous-cli search serve` for a root and waits for its host file.
///
/// @param root - the root
fn spawn_host(root: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let exe = if exe.file_stem().is_some_and(|s| s == "unluminous-cli") { exe } else { exe.with_file_name(if cfg!(windows) { "unluminous-cli.exe" } else { "unluminous-cli" }) };
    let mut command = std::process::Command::new(exe);
    command.args(["search", "serve", "--root"]).arg(root).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW: the host outlives this command
        // and never opens a console of its own.
        command.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
    }
    if command.spawn().is_err() {
        return false;
    }
    let deadline = Instant::now() + START_WAIT;
    while Instant::now() < deadline {
        if let Some(file) = read_host_file(root) {
            if TcpStream::connect_timeout(&std::net::SocketAddr::from(([127, 0, 0, 1], file.port)), Duration::from_millis(200)).is_ok() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// Answers a `search` command for a root: through this process's host, another process's host, a host
/// started for the purpose, or, when none of those can be had, a direct scan.
///
/// @param root - the root
/// @param command - the wire name, such as `search.find`
/// @param arguments - the arguments
/// @param timeout - how long to wait for another process
/// @param hosting - whether this process may host
pub fn ask(root: &Path, command: &str, arguments: &Map<String, Value>, timeout: Duration, hosting: Hosting) -> Reply {
    if let Some(host) = hosts().lock().expect("hosts").get(root).cloned() {
        return host.answer(command, arguments);
    }
    if let Some(reply) = ask_running(root, command, arguments, timeout) {
        return reply;
    }
    match hosting {
        Hosting::InProcess => {
            if let Some(host) = in_process(root) {
                return host.answer(command, arguments);
            }
        }
        Hosting::Spawn => {
            if spawn_host(root) {
                if let Some(reply) = ask_running(root, command, arguments, timeout) {
                    return reply;
                }
            }
        }
    }
    if let Some(reply) = ask_running(root, command, arguments, timeout) {
        return reply;
    }
    without_a_host(root, command, arguments)
}

/// The answer when no host can be had: `find` by scanning the files, anything else refused.
///
/// @param root - the root
/// @param command - the wire name
/// @param arguments - the arguments
fn without_a_host(root: &Path, command: &str, arguments: &Map<String, Value>) -> Reply {
    if command != "search.find" {
        return Reply::failed(command, code::NOT_RUNNING, "No index host could be started for this folder, and only `search find` can be answered without one.");
    }
    match verbs::scan_without_index(root, arguments) {
        Ok(value) => Reply::done(command, "", value),
        Err(refusal) => Reply::failed(command, refusal.code, refusal.message),
    }
}

/// Runs the headless host for `search serve` and does not return until it has been idle for
/// `IDLE_EXIT`. Exit code 0, or 1 when another process is already the host.
///
/// @param root - the root
pub fn serve(root: &Path) -> i32 {
    match Host::start(root) {
        Ok(host) => {
            hosts().lock().expect("hosts").insert(root.to_path_buf(), Arc::clone(&host));
            host.serve_until_idle();
            0
        }
        Err(problem) => {
            eprintln!("unluminous-cli search serve: {problem}");
            1
        }
    }
}
