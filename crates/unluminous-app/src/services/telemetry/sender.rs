//! The thread that posts telemetry, and the spool that keeps what it could not post.
//!
//! **The window never waits for the network.** It hands a finished body to the `unluminous-telemetry`
//! thread through a channel and carries on. The one exception is closing, which waits a bounded time,
//! and only after the closing bodies are already on disk.
//!
//! **What could not be posted is kept, not retried in a loop.** A failed body is appended to this
//! window's spool, `spool-<pid>.jsonl`, and the next window that starts sends it. A body older than
//! 72 hours is dropped, because GA4 refuses a `timestamp_micros` older than that.
//!
//! **Two windows starting together cannot both send a leftover spool.** Each one claims a file by
//! renaming it to `sending-<its own pid>-<name>` before reading it, and a rename only succeeds once.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::payload;

/// How long one request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// GA4 refuses an event whose `timestamp_micros` is older than this.
const OLDEST: u64 = 72 * 60 * 60 * 1_000_000;
/// The most lines a spool keeps; the oldest go first.
const SPOOL_LINES: usize = 200;

/// What the window asks the thread to do.
enum Job {
    /// Post one body; spool it when that fails.
    Post(String),
    /// Post this window's own spool, and say when that is done.
    Drain(mpsc::Sender<()>),
}

/// What the thread has done, for `settings telemetry`.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub posted: u32,
    pub failed: u32,
    pub last_problem: Option<String>,
}

/// Where the thread posts and spools.
#[derive(Debug, Clone)]
pub struct Target {
    /// The full address, with the measurement id and the API secret in its query.
    pub url: String,
    /// The telemetry folder, where the spool and the debug copy go.
    pub folder: PathBuf,
    /// Whether every body is also appended to `sent.jsonl`.
    pub keep_a_copy: bool,
}

impl Target {
    /// The real endpoint, or `UNLUMINOUS_TELEMETRY_URL` when a test names one.
    ///
    /// @param folder - the telemetry folder
    /// @param keep_a_copy - whether to write every body to `sent.jsonl`
    pub fn for_the_binary(folder: PathBuf, keep_a_copy: bool) -> Target {
        let base = std::env::var("UNLUMINOUS_TELEMETRY_URL")
            .unwrap_or_else(|_| super::ENDPOINT.to_owned());
        let url = format!(
            "{base}?measurement_id={}&api_secret={}",
            super::MEASUREMENT_ID,
            super::API_SECRET
        );
        Target { url, folder, keep_a_copy }
    }

    /// This window's own spool file.
    fn own_spool(&self) -> PathBuf {
        self.folder.join(format!("spool-{}.jsonl", std::process::id()))
    }
}

/// The window's handle on the thread.
pub struct Sender {
    jobs: mpsc::Sender<Job>,
    status: Arc<Mutex<Status>>,
    target: Target,
}

impl Sender {
    /// Start the thread. It first sends whatever earlier windows left in the spool.
    ///
    /// @param target - where it posts and spools
    pub fn start(target: Target) -> Sender {
        let (jobs, inbox) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let thread_status = status.clone();
        let thread_target = target.clone();
        std::thread::Builder::new()
            .name("unluminous-telemetry".to_owned())
            .spawn(move || run(thread_target, inbox, thread_status))
            .ok();
        Sender { jobs, status, target }
    }

    /// Post one body, without waiting.
    ///
    /// @param body - a request body
    pub fn send(&self, body: String) {
        let _ = self.jobs.send(Job::Post(body));
    }

    /// The closing bodies: written to this window's spool at once, then posted by the thread, which is
    /// waited for at most `wait`. Whatever it does not post in that time is sent by the next window.
    ///
    /// @param bodies - the closing request bodies
    /// @param wait - the longest the window waits
    pub fn close(&self, bodies: Vec<String>, wait: Duration) {
        append(&self.target.own_spool(), &bodies);
        let (done, finished) = mpsc::channel();
        if self.jobs.send(Job::Drain(done)).is_ok() {
            let _ = finished.recv_timeout(wait);
        }
    }

    /// What the thread has done so far.
    pub fn status(&self) -> Status {
        self.status.lock().map(|status| status.clone()).unwrap_or_default()
    }
}

/// The thread: leftover spools first, then each job as it arrives, until the window goes.
fn run(target: Target, inbox: Receiver<Job>, status: Arc<Mutex<Status>>) {
    send_the_leftovers(&target, &status);
    while let Ok(job) = inbox.recv() {
        match job {
            Job::Post(body) => {
                if !post_and_count(&target, &body, &status) {
                    append(&target.own_spool(), &[body]);
                }
            }
            Job::Drain(done) => {
                drain(&target, &target.own_spool(), &status);
                let _ = done.send(());
            }
        }
    }
}

/// Post one body and record how it went. True when GA4 took it.
fn post_and_count(target: &Target, body: &str, status: &Mutex<Status>) -> bool {
    let answer = post(&target.url, body);
    if target.keep_a_copy {
        append(&target.folder.join("sent.jsonl"), &[body.to_owned()]);
    }
    if let Ok(mut status) = status.lock() {
        match &answer {
            Ok(()) => status.posted += 1,
            Err(problem) => {
                status.failed += 1;
                status.last_problem = Some(problem.clone());
            }
        }
    }
    answer.is_ok()
}

/// One `POST`. GA4 answers 204 to a body it took, and 2xx to one it silently dropped, so anything in
/// the 200s is a success here; the limits that make it drop one are kept in `payload`.
///
/// @param url - the endpoint with its query
/// @param body - the JSON body
pub fn post(url: &str, body: &str) -> Result<(), String> {
    let config = ureq::Agent::config_builder()
        .tls_config(unluminous_chat::client::tls_config())
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .max_redirects(0)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    match agent.post(url).header("content-type", "application/json").send(body) {
        Ok(reply) if reply.status().is_success() => Ok(()),
        Ok(reply) => Err(format!("answered {}", reply.status().as_u16())),
        Err(problem) => Err(problem.to_string()),
    }
}

/// Send every spool an earlier window left, claiming each one first.
fn send_the_leftovers(target: &Target, status: &Mutex<Status>) {
    for claimed in claim_the_leftovers(&target.folder, std::process::id()) {
        drain(target, &claimed, status);
    }
}

/// Rename every spool file that is not this window's own, and is not being sent by another window, to a
/// name this window owns. Answers the new names.
///
/// @param folder - the telemetry folder
/// @param pid - this window's process id
pub fn claim_the_leftovers(folder: &Path, pid: u32) -> Vec<PathBuf> {
    let own = format!("spool-{pid}.jsonl");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut claimed = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let leftover = name.starts_with("spool-") && name.ends_with(".jsonl") && name != own;
        // A `sending-` file whose window died part way through is picked up again; one whose window is
        // still running belongs to that window.
        let abandoned =
            name.starts_with("sending-")
                && name.split('-').nth(1).and_then(|pid| pid.parse::<u32>().ok()).is_some_and(
                    |other| other != pid && !unluminous_cli::instances::is_running(other),
                );
        let spool_of_a_running_window = name
            .strip_prefix("spool-")
            .and_then(|rest| rest.strip_suffix(".jsonl"))
            .and_then(|pid| pid.parse::<u32>().ok())
            .is_some_and(unluminous_cli::instances::is_running);
        if !(leftover && !spool_of_a_running_window) && !abandoned {
            continue;
        }
        let base = name.trim_start_matches("sending-").to_owned();
        let to = folder.join(format!("sending-{pid}-{base}"));
        if std::fs::rename(entry.path(), &to).is_ok() {
            claimed.push(to);
        }
    }
    claimed
}

/// Post every line of a spool file GA4 will still take, and keep only what failed. A file left empty is
/// removed.
fn drain(target: &Target, file: &Path, status: &Mutex<Status>) {
    let lines = read_lines(file);
    if lines.is_empty() {
        let _ = std::fs::remove_file(file);
        return;
    }
    let now = payload::now_micros();
    let mut kept = Vec::new();
    for line in still_wanted(lines, now) {
        if !post_and_count(target, &line, status) {
            kept.push(line);
        }
    }
    match kept.is_empty() {
        true => {
            let _ = std::fs::remove_file(file);
        }
        false => replace(file, &kept),
    }
}

/// The lines GA4 will still take: those younger than 72 hours, the newest [`SPOOL_LINES`] of them.
///
/// @param lines - every line of a spool
/// @param now - the time now, in microseconds
pub fn still_wanted(lines: Vec<String>, now: u64) -> Vec<String> {
    let young: Vec<String> = lines
        .into_iter()
        .filter(|line| {
            payload::timestamp_of(line).is_some_and(|at| now.saturating_sub(at) < OLDEST)
        })
        .collect();
    let skip = young.len().saturating_sub(SPOOL_LINES);
    young.into_iter().skip(skip).collect()
}

/// The non-empty lines of a file.
fn read_lines(file: &Path) -> Vec<String> {
    std::fs::read_to_string(file)
        .map(|text| {
            text.lines().filter(|line| !line.trim().is_empty()).map(str::to_owned).collect()
        })
        .unwrap_or_default()
}

/// Append lines to a file, making its folder when it is missing.
fn append(file: &Path, lines: &[String]) {
    use std::io::Write;
    if lines.is_empty() {
        return;
    }
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        for line in lines {
            let _ = writeln!(out, "{line}");
        }
    }
}

/// Write a file whole, through a temporary file renamed over it, so a window that dies part way through
/// leaves the old file rather than half of the new one.
fn replace(file: &Path, lines: &[String]) {
    let temporary = file.with_extension("tmp");
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    if std::fs::write(&temporary, text).is_ok() {
        let _ = std::fs::rename(&temporary, file);
    }
}
