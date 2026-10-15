//! Usage statistics, sent to the `Unluminous App` property in Google Analytics. `task-2271`.
//!
//! `tasks/task-2271-telemetry-tdd.md` is the design. What a person needs to know is in three sentences:
//! it is on unless `telemetry.enabled` is turned off, what it sends is a closed list of names and a few
//! durations, and it never sends a path, a file name, a project name or anything typed.
//!
//! ## This reverses a standing rule, on instruction
//!
//! `services::update` records `task-1692`'s line, *"no telemetry and nothing at startup"*. `task-2063`
//! reversed the update check's half of it and this ticket reverses the telemetry half: *"add telemetry to
//! Unluminous. add a setting to turn it off in Settings."* The switch is on `Settings -> Editor`, and off
//! means nothing is counted and nothing is sent from that frame on, the closing event included.
//!
//! ## Three pieces
//!
//! - [`note`] counts one use of something, from anywhere on the window's thread. It is a function over a
//!   process wide tally rather than a field threaded through every caller, because the places worth
//!   counting are the funnels every interaction passes through (`run_action`, `run_cli`) and two places
//!   inside plugins that cannot reach the window. Off, it is one relaxed atomic load.
//! - [`Telemetry`] is the window's own clock: open, focused and active time, which panels and which kind
//!   of tab were showing while somebody was active, and when the next request is due.
//! - [`sender`] is the thread that posts, and the spool that keeps what could not be posted.
//!
//! [`payload`] turns all of it into GA4 Measurement Protocol bodies, and is where the limits live.

pub mod payload;
pub mod sender;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde_json::{json, Value};

/// The `Unluminous App` web stream's measurement id.
pub const MEASUREMENT_ID: &str = "G-WHZDTZ5JK1";
/// The stream's Measurement Protocol API secret, nicknamed `Unluminous desktop app` in GA4's admin page.
///
/// It is compiled in because a desktop program has nowhere else to keep it. A Measurement Protocol
/// secret lets its holder send events to this one stream and read nothing, so it is the same kind of
/// value as a public web page's measurement id. If it is ever abused it is rotated in the admin page and
/// the next release carries the new one.
pub const API_SECRET: &str = "S8zZ-zcKT8uqRl2My1Jwzg";
/// Where a request goes. `UNLUMINOUS_TELEMETRY_URL` replaces it for a test's scripted server.
pub const ENDPOINT: &str = "https://www.google-analytics.com/mp/collect";

/// How long after the first frame the first request goes, so a start is not competing with it.
pub const FIRST_AFTER: f64 = 10.0;
/// How often a window that is being used sends what it counted. Under GA4's 30 minute session timeout,
/// so a session that is being used stays one session.
pub const EVERY: f64 = 20.0 * 60.0;
/// How long after the last input a focused window still counts as being used. GA4's own idea of
/// engagement is the same minute.
pub const ACTIVE_FOR: f64 = 60.0;
/// The most requests one window sends in its life, so a fault cannot become a flood.
pub const MOST_REQUESTS: u32 = 200;
/// How often the panels and the tab are looked at.
pub const SAMPLE_EVERY: f64 = 1.0;
/// The most time one frame or one sample may add, so a machine that slept does not count the night.
const LONGEST_STEP: f64 = 5.0;

/// What kind of thing was used. Sent as the `category` parameter of a `feature` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// A menu entry, a key chord, the command palette or `action run`: `UnluminousApp::run_action`.
    Action,
    /// A command from `unluminous-cli` or an outside agent's MCP tool call.
    Command,
    /// A command a model inside Unluminous called as a tool: the Agent-Chat pane, a chat node, a plugin.
    AgentTool,
    /// A question sent to an agent from the Agent-Chat pane or a chat node, by its wire shape.
    AgentChat,
    /// A ticket's agent started on the Agent-Tasks board, by its kind.
    AgentTask,
    /// A file opened in a tab, by its kind.
    File,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Action => "action",
            Category::Command => "command",
            Category::AgentTool => "agent_tool",
            Category::AgentChat => "agent_chat",
            Category::AgentTask => "agent_task",
            Category::File => "file",
        }
    }
}

/// Whether anything is being counted, in this process.
static ON: AtomicBool = AtomicBool::new(false);
/// Every use counted since the last request, by category and name.
static TALLY: Mutex<BTreeMap<(Category, String), u32>> = Mutex::new(BTreeMap::new());

/// Whether telemetry is counting in this process.
pub fn is_on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Start or stop counting. Stopping throws away what was counted and not yet sent, because a person who
/// turned the setting off did not agree to that being sent either.
pub(crate) fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        if let Ok(mut tally) = TALLY.lock() {
            tally.clear();
        }
    }
}

/// Count one use of `name`. Nothing at all while telemetry is off.
///
/// @param category - what kind of thing it is
/// @param name - its name from one of the closed lists; [`clean_name`] is applied, so a name that is not
///   a plain identifier is never sent as it stands
pub fn note(category: Category, name: &str) {
    if !is_on() {
        return;
    }
    let name = clean_name(name);
    if name.is_empty() {
        return;
    }
    if let Ok(mut tally) = TALLY.lock() {
        *tally.entry((category, name)).or_insert(0) += 1;
    }
}

/// Count an action by the name the command line calls it, built only when telemetry is on.
///
/// @param name - builds the name; not called while telemetry is off, so the `String` it makes costs
///   nothing then
pub fn note_with(category: Category, name: impl FnOnce() -> String) {
    if is_on() {
        note(category, &name());
    }
}

/// Count a command by its wire name, only when the catalogue has it: a name somebody sent that is not a
/// command is refused by `run_cli` and is not a name Unluminous chose.
///
/// @param category - `Command` or `AgentTool`
/// @param wire - the request's command, such as `tab.open`
pub fn note_command(category: Category, wire: &str) {
    if is_on() && is_a_command(wire) {
        note(category, wire);
    }
}

/// Whether a wire name is a command in the catalogue.
///
/// @param wire - a request's command
pub fn is_a_command(wire: &str) -> bool {
    let (area, verb) = wire.split_once('.').unwrap_or(("", wire));
    unluminous_cli::catalogue::COMMANDS
        .iter()
        .any(|command| command.area == area && command.verb == verb)
}

/// Take everything counted since the last request.
pub(crate) fn take_tally() -> BTreeMap<(Category, String), u32> {
    TALLY.lock().map(|mut tally| std::mem::take(&mut *tally)).unwrap_or_default()
}

/// Put back what a request had no room for, so the next one carries it.
///
/// @param rows - the rows to add back
pub(crate) fn put_back(rows: BTreeMap<(Category, String), u32>) {
    if let Ok(mut tally) = TALLY.lock() {
        for (key, count) in rows {
            *tally.entry(key).or_insert(0) += count;
        }
    }
}

/// A name safe to send: what it is when it is a plain identifier from one of the closed lists, cut to
/// GA4's 100 characters, and with a third party plugin's own names replaced by `other`.
///
/// Every name counted comes from a list compiled into Unluminous (an `Action` name, a catalogue wire
/// name, `file_kind::kind_name`, a wire shape), so this is the second line, not the first: a character
/// outside the identifier set means something unexpected reached it, and it is dropped rather than sent.
///
/// @param name - the name as counted
pub fn clean_name(name: &str) -> String {
    let name = without_third_party_plugins(name);
    let plain = name.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.' | '/' | ' ' | '+' | '#')
    });
    match plain {
        true => name.chars().take(100).collect(),
        false => String::new(),
    }
}

/// A plugin's action names its plugin, and a plugin that did not ship with Unluminous is a folder name
/// somebody chose, so its names become `other`.
fn without_third_party_plugins(name: &str) -> String {
    for prefix in ["plugin-pane:", "plugin-tab:", "plugin-run:", "plugin:"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let plugin = rest.split(['/', ':']).next().unwrap_or_default();
            return match is_bundled(plugin) {
                true => name.to_owned(),
                false => format!("{prefix}other"),
            };
        }
    }
    name.to_owned()
}

/// Whether a plugin id is one Unluminous ships, or one of its own core providers.
///
/// @param id - the plugin's id
pub fn is_bundled(id: &str) -> bool {
    id == "unluminous"
        || crate::services::plugins::bundled::ALL.iter().any(|(bundled, _, _)| *bundled == id)
        || crate::services::plugins::UI_PROVIDERS.contains(&id)
}

/// Why telemetry is not running, or that it may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// It may run while the setting is on.
    Allowed,
    /// `UNLUMINOUS_TELEMETRY=off`.
    Environment,
    /// A window started with `--background`, which something else is driving.
    Background,
    /// A window nobody started telemetry in, which is every window a test builds.
    Test,
}

impl Permission {
    /// The released binary's answer: a `--background` window sends nothing unless the environment says
    /// `on` or `debug`, and `off` in the environment wins over everything.
    ///
    /// @param background - whether the window was started with `--background`
    /// @param environment - `UNLUMINOUS_TELEMETRY`, when it is set
    pub fn for_the_binary(background: bool, environment: Option<&str>) -> Permission {
        match environment.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("off" | "0" | "false" | "no") => Permission::Environment,
            Some("on" | "1" | "true" | "yes" | "debug") => Permission::Allowed,
            _ if background => Permission::Background,
            _ => Permission::Allowed,
        }
    }

    /// What `settings telemetry` calls it.
    pub fn name(self) -> &'static str {
        match self {
            Permission::Allowed => "allowed",
            Permission::Environment => "environment",
            Permission::Background => "background window",
            Permission::Test => "test",
        }
    }
}

/// Whether `UNLUMINOUS_TELEMETRY=debug` asked for GA4's DebugView and a copy of every body on disk.
pub fn debug_from_the_environment() -> bool {
    std::env::var("UNLUMINOUS_TELEMETRY")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("debug"))
}

/// How often a used window sends, which `UNLUMINOUS_TELEMETRY_EVERY` shortens for a test.
pub fn every_from_the_environment() -> f64 {
    std::env::var("UNLUMINOUS_TELEMETRY_EVERY")
        .ok()
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .filter(|seconds| *seconds >= 1.0)
        .unwrap_or(EVERY)
}

/// The folder telemetry keeps its client id and spool in.
///
/// @param settings - the person's settings folder
pub fn folder_in(settings: &Path) -> PathBuf {
    settings.join("telemetry")
}

/// This installation's client id, made the first time and kept in `id.txt`, and whether it was just made.
///
/// 32 hexadecimal characters from the operating system's random source. Nothing about the machine or
/// the person goes into it, and deleting the file makes a new one.
///
/// @param folder - the telemetry folder
pub fn client_id(folder: &Path) -> (String, bool) {
    let file = folder.join("id.txt");
    if let Ok(text) = std::fs::read_to_string(&file) {
        let id = text.trim();
        if id.len() >= 16 && id.chars().all(|c| c.is_ascii_hexdigit()) {
            return (id.to_owned(), false);
        }
    }
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // A machine with no random source is not one to send from with a made up id that might
        // collide; the time and the process id are unique enough for the one window that asks.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        bytes[..8].copy_from_slice(&(nanos as u64).to_le_bytes());
        bytes[8..12].copy_from_slice(&std::process::id().to_le_bytes());
    }
    let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let _ = std::fs::create_dir_all(folder);
    let _ = crate::services::store::write_atomically(&file, id.as_bytes());
    (id, true)
}

/// Whether `crash.log` grew since the last start, and remember its length for the next one.
///
/// @param settings - the person's settings folder, where `crash.log` is
/// @param folder - the telemetry folder, where its length is remembered
pub fn crashed_since_last_start(settings: &Path, folder: &Path) -> bool {
    let now = std::fs::metadata(settings.join(crate::services::crash_log::FILE))
        .map(|meta| meta.len())
        .unwrap_or(0);
    let file = folder.join("crash-size.txt");
    let before =
        std::fs::read_to_string(&file).ok().and_then(|text| text.trim().parse::<u64>().ok());
    let _ = std::fs::create_dir_all(folder);
    let _ = crate::services::store::write_atomically(&file, now.to_string().as_bytes());
    // A log that rolled over to `crash.log.old` is shorter than it was, which is a crash too.
    before.is_some_and(|before| now != before && now > 0)
}

/// What a window sends about itself in `app_open`.
#[derive(Debug, Clone, Default)]
pub struct Opening {
    /// `desktop` when Unluminous started on the last session, `folder` when a folder was named.
    pub launch: &'static str,
    /// The theme's id when Unluminous ships it, and `other` otherwise.
    pub theme: String,
    pub first_run: bool,
    pub crashed: bool,
}

/// A window's telemetry: its clocks, what it sampled, and when it next sends.
pub struct Telemetry {
    pub(crate) sender: sender::Sender,
    context: payload::Context,
    opening: Option<Opening>,
    /// The window's own time at the first frame, and at the last.
    started: Option<f64>,
    last_tick: f64,
    last_sample: f64,
    last_input: f64,
    last_send: f64,
    focused_seconds: f64,
    active_seconds: f64,
    /// Active time since the last request, which is that request's `engagement_time_msec`.
    active_since_send: f64,
    panel_seconds: BTreeMap<String, f64>,
    tab_seconds: BTreeMap<String, f64>,
    requests: u32,
    every: f64,
}

impl Telemetry {
    /// A window's telemetry, sending through `sender`.
    ///
    /// @param sender - the thread that posts
    /// @param context - the ids and the facts about this machine every request carries
    /// @param opening - what `app_open` says
    /// @param every - how often a used window sends
    pub fn new(
        sender: sender::Sender,
        context: payload::Context,
        opening: Opening,
        every: f64,
    ) -> Self {
        Telemetry {
            sender,
            context,
            opening: Some(opening),
            started: None,
            last_tick: 0.0,
            last_sample: 0.0,
            last_input: f64::NEG_INFINITY,
            last_send: 0.0,
            focused_seconds: 0.0,
            active_seconds: 0.0,
            active_since_send: 0.0,
            panel_seconds: BTreeMap::new(),
            tab_seconds: BTreeMap::new(),
            requests: 0,
            every,
        }
    }

    /// Advance the clocks by one frame. Answers whether it is time to look at the panels and the tab.
    ///
    /// @param now - the window's own time, in seconds
    /// @param focused - whether the window has the operating system's focus
    /// @param input - whether this frame carried any input
    pub fn tick(&mut self, now: f64, focused: bool, input: bool) -> bool {
        let Some(_) = self.started else {
            self.started = Some(now);
            self.last_tick = now;
            self.last_sample = now;
            self.last_send = now;
            if focused && input {
                self.last_input = now;
            }
            return false;
        };
        let step = (now - self.last_tick).clamp(0.0, LONGEST_STEP);
        self.last_tick = now;
        if focused {
            self.focused_seconds += step;
            if input {
                self.last_input = now;
            }
            if self.is_active(now) {
                self.active_seconds += step;
                self.active_since_send += step;
            }
        }
        now - self.last_sample >= SAMPLE_EVERY
    }

    /// The window's own time at the last frame, which is the closing request's time: `on_exit` has no
    /// frame to ask.
    pub fn last_frame(&self) -> f64 {
        self.last_tick
    }

    /// Whether somebody is using the window: it has the focus and had input in the last minute.
    fn is_active(&self, now: f64) -> bool {
        now - self.last_input < ACTIVE_FOR
    }

    /// Add the time since the last sample to each panel showing and to the kind of tab showing, when
    /// somebody is using the window.
    ///
    /// @param now - the window's own time
    /// @param focused - whether the window has the focus
    /// @param panels - the names of the panels showing, the editing area included
    /// @param tab - the kind of tab showing, when the editing area is showing
    pub fn sample(&mut self, now: f64, focused: bool, panels: &[String], tab: Option<&str>) {
        let step = (now - self.last_sample).clamp(0.0, LONGEST_STEP);
        self.last_sample = now;
        if !focused || !self.is_active(now) {
            return;
        }
        for panel in panels {
            *self.panel_seconds.entry(clean_name(panel)).or_insert(0.0) += step;
        }
        if let Some(tab) = tab {
            *self.tab_seconds.entry(clean_name(tab)).or_insert(0.0) += step;
        }
    }

    /// Whether a request is due: the first one ten seconds in, and then one every [`EVERY`] while
    /// something was counted or somebody was active.
    ///
    /// @param now - the window's own time
    pub fn due(&self, now: f64) -> bool {
        let Some(started) = self.started else {
            return false;
        };
        if self.requests >= MOST_REQUESTS {
            return false;
        }
        if self.opening.is_some() {
            return now - started >= FIRST_AFTER;
        }
        now - self.last_send >= self.every && self.has_something()
    }

    /// Whether there is anything to say since the last request.
    fn has_something(&self) -> bool {
        self.active_since_send >= 1.0
            || !self.panel_seconds.is_empty()
            || TALLY.lock().map(|tally| !tally.is_empty()).unwrap_or(false)
    }

    /// The events since the last request, oldest kinds first: `app_open` when it has not gone, the
    /// heartbeat, then the panels, the tabs and the features.
    fn events_since_the_last(&mut self) -> Vec<Value> {
        let mut events = Vec::new();
        if let Some(opening) = self.opening.take() {
            events.push(payload::app_open(&opening));
            if opening.crashed {
                events.push(payload::event("app_crash", json!({ "uses": 1 })));
            }
        }
        let engaged = (self.active_since_send * 1000.0).round() as u64;
        self.active_since_send = 0.0;
        events.push(payload::heartbeat(engaged));
        for (panel, seconds) in std::mem::take(&mut self.panel_seconds) {
            events.push(payload::event(
                "panel_time",
                json!({ "panel": panel, "seconds": seconds.round() as u64 }),
            ));
        }
        for (kind, seconds) in std::mem::take(&mut self.tab_seconds) {
            events.push(payload::event(
                "tab_time",
                json!({ "kind": kind, "seconds": seconds.round() as u64 }),
            ));
        }
        events
    }

    /// Send what is due, through the thread. Never waits.
    ///
    /// @param now - the window's own time
    pub fn send(&mut self, now: f64) {
        self.last_send = now;
        let mut events = self.events_since_the_last();
        let tally = take_tally();
        let (rows, left) = payload::feature_rows(tally, payload::MOST_EVENTS - events.len());
        events.extend(rows);
        put_back(left);
        for body in payload::bodies(&self.context, events, payload::now_micros()) {
            if self.requests >= MOST_REQUESTS {
                break;
            }
            self.requests += 1;
            self.sender.send(body);
        }
    }

    /// The closing request, with `app_close` and everything counted since the last one, written to the
    /// spool and then given to the thread, which is waited for at most `wait`.
    ///
    /// @param now - the window's own time
    /// @param wait - how long the window may wait for it to be posted
    pub fn close(&mut self, now: f64, wait: std::time::Duration) {
        let started = self.started.unwrap_or(now);
        let mut events = self.events_since_the_last();
        events.push(payload::event(
            "app_close",
            json!({
                "open_seconds": (now - started).max(0.0).round() as u64,
                "focused_seconds": self.focused_seconds.round() as u64,
                "active_seconds": self.active_seconds.round() as u64,
            }),
        ));
        let (rows, _) = payload::feature_rows(take_tally(), payload::MOST_EVENTS * 2);
        events.extend(rows);
        let bodies = payload::bodies(&self.context, events, payload::now_micros());
        self.requests += bodies.len() as u32;
        self.sender.close(bodies, wait);
    }

    /// What `settings telemetry` reports about this window.
    pub fn status(&self) -> Value {
        let status = self.sender.status();
        json!({
            "clientId": self.context.client_id,
            "sessionId": self.context.session_id,
            "requests": self.requests,
            "posted": status.posted,
            "failed": status.failed,
            "lastProblem": status.last_problem,
            "waiting": TALLY.lock().map(|tally| tally.len()).unwrap_or(0),
            "focusedSeconds": self.focused_seconds.round() as u64,
            "activeSeconds": self.active_seconds.round() as u64,
            "debug": self.context.debug,
        })
    }
}

#[cfg(test)]
mod tests;
