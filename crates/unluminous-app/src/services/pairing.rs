//! Pairing a phone with the Tasks board, seen from the desktop (`task-30`).
//!
//! `tasks/task-29-pairing-tdd.md` section 3 says which program the phone pairs with: the board. Desktop
//! Unluminous only starts a pairing, shows the code, confirms who claimed it and lists the devices. It is a
//! client of machine only routes on the board, on the same computer, and it never talks to the phone. The
//! desktop stores nothing: devices, offers and claims live in the board.
//!
//! ## Finding the board
//!
//! The board writes `<Unluminous settings folder>/boards/<port>.conf` when it starts, in the same
//! `name = value` form as `instances/<pid>.conf`, with the keys `pid`, `port`, `token` and `name`. [`find_board`]
//! reads the folder, keeps the entries whose process is running and uses the first whose `GET /health`
//! answers 200. A [`Source`] with a file names one file to use instead, which is what `UNLUMINOUS_BOARD_CONF`
//! sets and what the tests use. Every request goes to `http://127.0.0.1:<port>` with the header
//! `x-skip-token: <token>`, in plain HTTP.
//!
//! ## One thread a request
//!
//! A request over the network on the drawing thread would stop the window drawing for as long as it took, so
//! each request runs on a thread of its own and sends its answer down a channel, then wakes the window. That
//! is the shape of `services::update::Check`. A thread for each request rather than one thread for all of
//! them keeps `GET /pair/offers/<id>?wait=20`, which the board holds for up to 25 seconds, from making a
//! `Confirm` wait behind it.
//!
//! [`PairingState`] is the one value the dialog and the `pair` commands share. [`PairingState::pump`] takes
//! the answers that have arrived and applies each to the state, by the route it answered.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

/// Whether `Copy Link` copies the `https://unluminous.com/pair#...` form, which the board answers as
/// `webLink`. The page does not exist yet (`tasks/task-29-pairing-tdd.md` section 7.6), so this is off and
/// `Copy Link` copies the `unluminous://` link, which the phone's `Paste Link` accepts.
pub const WEB_LINKS: bool = false;

/// How long the board is asked to hold `GET /pair/offers/<id>` for, in seconds. The board allows 25.
pub const WAIT_SECONDS: u64 = 20;

/// How many seconds before an offer expires the dialog asks for a new one.
pub const RENEW_BEFORE_SECONDS: i64 = 20;

/// The command that starts the board on a Mac, shown when no board is found.
pub const START_COMMAND: &str = "scripts/mac-board.sh start";

/// How long an ordinary request is given before it fails.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the answer to a request that holds is waited for: the board's own wait and some room.
const WAIT_TIMEOUT: Duration = Duration::from_secs(WAIT_SECONDS + 10);

/// How long the board's own connection test and the Tailscale command are given.
const SLOW_TIMEOUT: Duration = Duration::from_secs(40);

/// How long a `GET /health` is given when choosing a board. It was 1.5 seconds, and on Windows a board's health
/// check took 2.05 seconds while it asked a stopped terminal daemon, so Unluminous on the PC said no board was running
/// (task-2879). A board that is not running is refused at once, so the longer wait only costs time on a slow board.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

/// The gap between two long polls of an offer, so a board that answers at once is not asked in a loop.
const POLL_GAP_MS: i64 = 400;

/// How long to wait before asking for a new offer again after a try, so a board that refuses is not asked in a loop.
const RENEW_RETRY_MS: i64 = 3000;

/// The gap between two reads of the device list while a phone's own test results are awaited.
const DEVICES_GAP_MS: i64 = 2000;

// ------------------------------------------------------------------------------------ the board

/// A board that was found: where it is and the secret that every request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub name: String,
}

/// Read one `boards/<port>.conf`. `None` when it has no port or no token.
/// @param text - the file's text, `name = value` a line and `#` for a comment
pub fn read_conf(text: &str) -> Option<Board> {
    let (mut pid, mut port, mut token, mut name) = (0, None, None, String::new());
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "pid" => pid = value.parse().unwrap_or(0),
            "port" => port = value.parse().ok(),
            "token" => token = Some(value.to_owned()),
            "name" => name = value.to_owned(),
            _ => {}
        }
    }
    Some(Board { pid, port: port?, token: token.filter(|token| !token.is_empty())?, name })
}

/// Where a board is looked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    /// The `boards` folder inside the Unluminous settings folder. `None` in a window that has no settings
    /// folder, which is every window a test builds, so such a window finds a board only through `file`.
    pub folder: Option<PathBuf>,
    /// One conf file to use instead of the folder: `UNLUMINOUS_BOARD_CONF`, or what a test names.
    pub file: Option<PathBuf>,
}

impl Source {
    /// The source for a window whose settings folder is `settings`, with `UNLUMINOUS_BOARD_CONF` read from
    /// the environment.
    /// @param settings - the Unluminous settings folder, or `None` when the window has none
    pub fn for_the_settings_folder(settings: Option<&std::path::Path>) -> Self {
        Source {
            folder: settings.map(|folder| folder.join("boards")),
            file: std::env::var_os("UNLUMINOUS_BOARD_CONF")
                .filter(|named| !named.is_empty())
                .map(PathBuf::from),
        }
    }
}

/// Find a board: the named file when there is one, otherwise the first running board in the folder whose
/// `/health` answers 200. The reason is a sentence when none is found.
/// @param source - where to look
pub fn find_board(source: &Source) -> Result<Board, String> {
    if let Some(file) = &source.file {
        return named_board(file);
    }
    let Some(folder) = &source.folder else {
        return Err("No Tasks board was found: this window has no Unluminous settings folder to look for one in.".to_owned());
    };
    let mut found: Vec<Board> = std::fs::read_dir(folder)
        .map(|listed| listed.flatten().filter_map(|entry| read_board_file(&entry.path())).collect())
        .unwrap_or_default();
    found.retain(|board| board.pid == 0 || unluminous_cli::instances::is_running(board.pid));
    found.sort_by_key(|board| board.port);
    found.into_iter().find(is_healthy).ok_or_else(|| {
        format!("No Tasks board is running on this computer ({}).", folder.display())
    })
}

/// The board in the one file a source names, which must answer its `/health`.
fn named_board(file: &std::path::Path) -> Result<Board, String> {
    let board = read_board_file(file).ok_or_else(|| {
        format!("No Tasks board was found: {} is not a board's conf file.", file.display())
    })?;
    match is_healthy(&board) {
        true => Ok(board),
        false => Err(format!(
            "No Tasks board is answering: the one named by {} is not answering on port {}.",
            file.display(),
            board.port
        )),
    }
}

/// The board a conf file describes, when it is a `.conf` file that can be read.
fn read_board_file(path: &std::path::Path) -> Option<Board> {
    let is_conf = path.extension().is_some_and(|end| end == "conf");
    let text = std::fs::read_to_string(path).ok()?;
    read_conf(&text).filter(|_| is_conf || path.is_file())
}

/// Whether `GET /health` on the board answers 200.
fn is_healthy(board: &Board) -> bool {
    let agent = agent(HEALTH_TIMEOUT);
    agent
        .get(&format!("http://127.0.0.1:{}/health", board.port))
        .call()
        .map(|reply| reply.status().as_u16() == 200)
        .unwrap_or(false)
}

/// An HTTP agent that answers an error status as an answer, which is what every route here wants.
fn agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .build();
    ureq::Agent::new_with_config(config)
}

// ------------------------------------------------------------------------------------ requests

/// The HTTP method of a [`Call`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
    Post,
    Patch,
    Delete,
}

/// One request to the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub method: Method,
    /// The path with its query, such as `/pair/offers/abc?wait=20`.
    pub path: String,
    /// The JSON body, for the methods that send one.
    pub body: Option<Value>,
}

impl Call {
    /// A call with no body.
    /// @param method - the HTTP method
    /// @param path - the path with its query
    pub fn of(method: Method, path: impl Into<String>) -> Self {
        Call { method, path: path.into(), body: None }
    }

    /// The same call carrying a JSON body.
    /// @param body - what to send
    pub fn with(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// How long this call is given.
    fn timeout(&self) -> Duration {
        match self.path.as_str() {
            path if path.contains("wait=") => WAIT_TIMEOUT,
            "/pair/test" | "/pair/tailscale/serve" | "/pair/network" => SLOW_TIMEOUT,
            _ => REQUEST_TIMEOUT,
        }
    }
}

/// What the board answered: any status, with its JSON body (`Null` when there was none).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub status: u16,
    pub body: Value,
}

impl Answer {
    /// Whether the status is a success.
    pub fn is_ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The board's own sentence for a refusal (`message`, then `error`), or the status.
    pub fn problem(&self) -> String {
        let said = text(&self.body, "message");
        let said = if said.is_empty() { text(&self.body, "error") } else { said };
        match said.is_empty() {
            true => format!("The board answered {}.", self.status),
            false => said,
        }
    }
}

/// One answer on its way back to the window.
struct Done {
    tag: u64,
    result: Result<Answer, String>,
}

/// The board found by an earlier request, kept so the next one does not look again.
type Cache = Arc<Mutex<Option<Board>>>;

/// Send `call` to the board, finding it first when none is cached. A failure to connect clears the cache and
/// finds the board again once, because the board may have restarted on another port.
fn send_call(source: &Source, cache: &Cache, call: &Call) -> Result<Answer, String> {
    for attempt in 0..2 {
        let cached = cache.lock().ok().and_then(|held| held.clone());
        let board = match cached {
            Some(board) => board,
            None => {
                let board = find_board(source)?;
                if let Ok(mut held) = cache.lock() {
                    *held = Some(board.clone());
                }
                board
            }
        };
        match request(&board, call) {
            Ok(answer) => return Ok(answer),
            Err(problem) if attempt == 1 => return Err(problem),
            Err(_) => {
                if let Ok(mut held) = cache.lock() {
                    *held = None;
                }
            }
        }
    }
    Err("The board could not be reached.".to_owned())
}

/// One HTTP request to a found board.
fn request(board: &Board, call: &Call) -> Result<Answer, String> {
    let agent = agent(call.timeout());
    let url = format!("http://127.0.0.1:{}{}", board.port, call.path);
    let sent = match (call.method, &call.body) {
        (Method::Get, _) => agent.get(&url).header("x-skip-token", &board.token).call(),
        (Method::Delete, _) => agent.delete(&url).header("x-skip-token", &board.token).call(),
        (method, body) => {
            let text = body.as_ref().map(Value::to_string).unwrap_or_default();
            let builder = match method {
                Method::Put => agent.put(&url),
                Method::Patch => agent.patch(&url),
                _ => agent.post(&url),
            };
            builder
                .header("x-skip-token", &board.token)
                .header("content-type", "application/json")
                .send(text.as_str())
        }
    };
    let mut reply = sent.map_err(|problem| problem.to_string())?;
    let status = reply.status().as_u16();
    let body = reply.body_mut().read_to_string().unwrap_or_default();
    Ok(Answer { status, body: serde_json::from_str(&body).unwrap_or(Value::Null) })
}

// ------------------------------------------------------------------------------------ typed values

/// What `PUT /pair/settings` `listen` and `GET /pair/status` `listen` say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Listen {
    #[default]
    Off,
    Loopback,
    All,
}

impl Listen {
    /// The word the board uses.
    pub fn word(self) -> &'static str {
        match self {
            Listen::Off => "off",
            Listen::Loopback => "loopback",
            Listen::All => "all",
        }
    }

    /// The listen setting a word names.
    /// @param word - `off`, `loopback` or `all`
    pub fn from_word(word: &str) -> Option<Self> {
        [Listen::Off, Listen::Loopback, Listen::All]
            .into_iter()
            .find(|listen| listen.word() == word)
    }
}

/// `GET /pair/status`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub listen: Listen,
    pub port: u16,
    pub board_id: String,
    pub name: String,
    pub fingerprint: String,
    pub device_count: u64,
    pub live_offers: u64,
}

impl Status {
    /// Read the board's answer.
    /// @param value - the JSON body
    pub fn from_json(value: &Value) -> Option<Self> {
        value.is_object().then(|| Status {
            listen: Listen::from_word(&text(value, "listen")).unwrap_or_default(),
            port: number(value, "port") as u16,
            board_id: text(value, "boardId"),
            name: text(value, "name"),
            fingerprint: text(value, "fingerprint"),
            device_count: number(value, "deviceCount"),
            live_offers: number(value, "liveOffers"),
        })
    }
}

/// An address the phone may be given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Address {
    /// `lan`, `mdns`, `tailscale-name` or `tailscale-ip`.
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub interface: String,
    pub note: String,
}

impl Address {
    /// Read one address.
    /// @param value - one entry of an `addresses` list
    pub fn from_json(value: &Value) -> Self {
        Address {
            kind: text(value, "kind"),
            host: text(value, "host"),
            port: number(value, "port") as u16,
            interface: text(value, "interface"),
            note: text(value, "note"),
        }
    }

    /// `host:port`, with an IPv6 host in brackets. What a tick box is keyed by and what `POST /pair/offers`
    /// is given.
    pub fn host_port(&self) -> String {
        match self.host.contains(':') && !self.host.starts_with('[') {
            true => format!("[{}]:{}", self.host, self.port),
            false => format!("{}:{}", self.host, self.port),
        }
    }

    /// The words that say what kind of address it is.
    pub fn kind_label(&self) -> &'static str {
        match self.kind.as_str() {
            "lan" => "Home network",
            "mdns" => "Local name",
            "tailscale-name" => "Tailscale name",
            "tailscale-ip" => "Tailscale address",
            _ => "Address",
        }
    }
}

/// What Tailscale is doing on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TailscaleState {
    Running,
    NeedsLogin,
    Stopped,
    #[default]
    NotInstalled,
    Error,
}

impl TailscaleState {
    /// Read the board's word for it.
    /// @param word - `running`, `needs-login`, `stopped`, `not-installed` or `error`
    pub fn from_word(word: &str) -> Self {
        match word {
            "running" => TailscaleState::Running,
            "needs-login" => TailscaleState::NeedsLogin,
            "stopped" => TailscaleState::Stopped,
            "error" => TailscaleState::Error,
            _ => TailscaleState::NotInstalled,
        }
    }

    /// The board's word for it.
    pub fn word(self) -> &'static str {
        match self {
            TailscaleState::Running => "running",
            TailscaleState::NeedsLogin => "needs-login",
            TailscaleState::Stopped => "stopped",
            TailscaleState::NotInstalled => "not-installed",
            TailscaleState::Error => "error",
        }
    }
}

/// A phone the tailnet already has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Phone {
    pub name: String,
    pub online: bool,
}

/// The `serve` part of the Tailscale report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Serve {
    pub needed: bool,
    pub configured: bool,
    pub command: String,
}

/// The Tailscale part of `GET /pair/network`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tailscale {
    pub installed: bool,
    pub state: TailscaleState,
    pub problem: Option<String>,
    pub version: Option<String>,
    pub dns_name: Option<String>,
    pub ips: Vec<String>,
    pub magic_dns: bool,
    pub userspace: bool,
    pub serve: Serve,
    pub phones: Vec<Phone>,
}

impl Tailscale {
    /// Read the board's report.
    /// @param value - the `tailscale` object
    pub fn from_json(value: &Value) -> Self {
        let serve = value.get("serve").cloned().unwrap_or(Value::Null);
        Tailscale {
            installed: flag(value, "installed"),
            state: TailscaleState::from_word(&text(value, "state")),
            problem: optional_text(value, "problem"),
            version: optional_text(value, "version"),
            dns_name: optional_text(value, "dnsName"),
            ips: strings(value, "ips"),
            magic_dns: flag(value, "magicDns"),
            userspace: flag(value, "userspace"),
            serve: Serve {
                needed: flag(&serve, "needed"),
                configured: flag(&serve, "configured"),
                command: text(&serve, "command"),
            },
            phones: list(value, "phones")
                .iter()
                .map(|phone| Phone { name: text(phone, "name"), online: flag(phone, "online") })
                .collect(),
        }
    }
}

/// `GET /pair/network`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Network {
    pub addresses: Vec<Address>,
    pub tailscale: Tailscale,
}

impl Network {
    /// Read the board's answer.
    /// @param value - the JSON body
    pub fn from_json(value: &Value) -> Self {
        Network {
            addresses: list(value, "addresses").iter().map(Address::from_json).collect(),
            tailscale: Tailscale::from_json(value.get("tailscale").unwrap_or(&Value::Null)),
        }
    }
}

/// The result of the board's own test of one address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probe {
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub ok: bool,
    pub ms: Option<u64>,
    /// `ok`, `name-not-found`, `no-route`, `refused`, `timeout`, `tls`, `wrong-board`, `wrong-token`, `http`
    /// or `cannot-check-here`.
    pub reason: String,
    pub message: String,
}

impl Probe {
    /// Read one probe.
    /// @param value - one entry of a `results` list
    pub fn from_json(value: &Value) -> Self {
        Probe {
            kind: text(value, "kind"),
            host: text(value, "host"),
            port: number(value, "port") as u16,
            ok: flag(value, "ok"),
            ms: value.get("ms").and_then(Value::as_u64),
            reason: text(value, "reason"),
            message: text(value, "message"),
        }
    }

    /// The `host:port` this probe is about, which is what ties it to a row in the address list.
    pub fn host_port(&self) -> String {
        Address { host: self.host.clone(), port: self.port, ..Address::default() }.host_port()
    }

    /// The sentence a row shows: `Reachable, 12 ms`, or the board's own message.
    pub fn sentence(&self) -> String {
        match (self.ok, self.ms) {
            (true, Some(ms)) => format!("Reachable, {ms} ms"),
            (true, None) => "Reachable".to_owned(),
            (false, _) if !self.message.is_empty() => self.message.clone(),
            (false, _) => self.reason.clone(),
        }
    }
}

/// `POST /pair/offers`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Offer {
    pub offer_id: String,
    pub link: String,
    pub web_link: String,
    /// Twelve characters, shown as `XXXX-XXXX-XXXX`.
    pub code: String,
    pub expires_at: String,
    pub addresses: Vec<Address>,
    pub fingerprint: String,
}

impl Offer {
    /// Read the board's answer. `None` without an offer id.
    /// @param value - the JSON body
    pub fn from_json(value: &Value) -> Option<Self> {
        let offer_id = text(value, "offerId");
        (!offer_id.is_empty()).then(|| Offer {
            offer_id,
            link: text(value, "link"),
            web_link: text(value, "webLink"),
            code: text(value, "code"),
            expires_at: text(value, "expiresAt"),
            addresses: list(value, "addresses").iter().map(Address::from_json).collect(),
            fingerprint: text(value, "fingerprint"),
        })
    }

    /// The link `Copy Link` copies: `link`, or `web_link` when [`WEB_LINKS`] is on.
    pub fn link_to_copy(&self) -> &str {
        match WEB_LINKS && !self.web_link.is_empty() {
            true => &self.web_link,
            false => &self.link,
        }
    }

    /// The typed code as `XXXX-XXXX-XXXX`, with dashes put in when the board sent none.
    pub fn shown_code(&self) -> String {
        let letters: String = self.code.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        letters
            .as_bytes()
            .chunks(4)
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// Where an offer is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfferState {
    #[default]
    Waiting,
    Claimed,
    Confirmed,
    Rejected,
    Expired,
    Cancelled,
}

impl OfferState {
    /// Read the board's word for it.
    /// @param word - `waiting`, `claimed`, `confirmed`, `rejected`, `expired` or `cancelled`
    pub fn from_word(word: &str) -> Self {
        match word {
            "claimed" => OfferState::Claimed,
            "confirmed" => OfferState::Confirmed,
            "rejected" => OfferState::Rejected,
            "expired" => OfferState::Expired,
            "cancelled" => OfferState::Cancelled,
            _ => OfferState::Waiting,
        }
    }

    /// The board's word for it.
    pub fn word(self) -> &'static str {
        match self {
            OfferState::Waiting => "waiting",
            OfferState::Claimed => "claimed",
            OfferState::Confirmed => "confirmed",
            OfferState::Rejected => "rejected",
            OfferState::Expired => "expired",
            OfferState::Cancelled => "cancelled",
        }
    }

    /// True while the offer can still change: nothing has ended it.
    pub fn is_live(self) -> bool {
        matches!(self, OfferState::Waiting | OfferState::Claimed)
    }
}

/// The phone that claimed an offer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claim {
    pub device_name: String,
    pub platform: String,
    pub model: String,
    pub app_version: String,
    /// Six digits.
    pub confirm_code: String,
    pub via: String,
    pub claimed_at: String,
}

impl Claim {
    /// Read the board's `claim` object.
    /// @param value - the object
    pub fn from_json(value: &Value) -> Self {
        Claim {
            device_name: text(value, "deviceName"),
            platform: text(value, "platform"),
            model: text(value, "model"),
            app_version: text(value, "appVersion"),
            confirm_code: text(value, "confirmCode"),
            via: text(value, "via"),
            claimed_at: text(value, "claimedAt"),
        }
    }

    /// The six digits as `482 913`.
    pub fn shown_code(&self) -> String {
        let digits: String = self.confirm_code.chars().filter(char::is_ascii_digit).collect();
        match digits.len() == 6 {
            true => format!("{} {}", &digits[..3], &digits[3..]),
            false => digits,
        }
    }
}

/// `GET /pair/offers/<id>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OfferStatus {
    pub state: OfferState,
    pub expires_at: String,
    pub claim: Option<Claim>,
    pub device: Option<Device>,
}

impl OfferStatus {
    /// Read the board's answer.
    /// @param value - the JSON body
    pub fn from_json(value: &Value) -> Self {
        OfferStatus {
            state: OfferState::from_word(&text(value, "state")),
            expires_at: text(value, "expiresAt"),
            claim: value.get("claim").filter(|claim| claim.is_object()).map(Claim::from_json),
            device: value.get("device").filter(|device| device.is_object()).map(Device::from_json),
        }
    }
}

/// The phone's own test of its addresses, as it posted them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LastTest {
    pub at: String,
    pub results: Vec<Probe>,
}

/// A paired phone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Device {
    pub id: u64,
    pub name: String,
    pub platform: String,
    pub model: String,
    pub app_version: String,
    pub username: String,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    pub last_seen_via: Option<String>,
    pub last_test: Option<LastTest>,
}

impl Device {
    /// Read one device.
    /// @param value - one entry of the device list
    pub fn from_json(value: &Value) -> Self {
        Device {
            id: number(value, "id"),
            name: text(value, "name"),
            platform: text(value, "platform"),
            model: text(value, "model"),
            app_version: text(value, "appVersion"),
            username: text(value, "username"),
            paired_at: text(value, "pairedAt"),
            last_seen_at: optional_text(value, "lastSeenAt"),
            last_seen_via: optional_text(value, "lastSeenVia"),
            last_test: value.get("lastTest").filter(|test| test.is_object()).map(|test| LastTest {
                at: text(test, "at"),
                results: list(test, "results").iter().map(Probe::from_json).collect(),
            }),
        }
    }
}

/// Read the device list the board answers with. An answer that is not a list is an empty list.
/// @param value - the JSON body
pub fn devices_from_json(value: &Value) -> Vec<Device> {
    value.as_array().map(|all| all.iter().map(Device::from_json).collect()).unwrap_or_default()
}

/// The device a command names, by its id or by its name (ignoring case).
/// @param devices - the paired devices
/// @param named - an id such as `3`, or a name such as `Jason's iPhone`
pub fn find_device<'a>(devices: &'a [Device], named: &str) -> Option<&'a Device> {
    let named = named.trim();
    named
        .parse::<u64>()
        .ok()
        .and_then(|id| devices.iter().find(|device| device.id == id))
        .or_else(|| devices.iter().find(|device| device.name.eq_ignore_ascii_case(named)))
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).filter(|said| !said.is_empty()).map(str::to_owned)
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or_default()
}

fn list(value: &Value, key: &str) -> Vec<Value> {
    value.get(key).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    list(value, key).iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()
}

// ------------------------------------------------------------------------------------ time

/// Milliseconds since 1970 for an ISO time such as `2026-11-23T17:15:07.397Z`. `None` when it is not one.
/// @param iso - the board's timestamp
pub fn parse_time(iso: &str) -> Option<i64> {
    let (date, time) = iso.trim().split_once('T')?;
    let mut parts = date.split('-');
    let (year, month, day): (i64, i64, i64) =
        (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let time = time.trim_end_matches('Z');
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock = clock.split(':');
    let (hours, minutes, seconds): (i64, i64, i64) =
        (clock.next()?.parse().ok()?, clock.next()?.parse().ok()?, clock.next()?.parse().ok()?);
    let millis: i64 = format!("{fraction:0<3}")[..3].parse().ok()?;
    let days = days_from_civil(year, month, day);
    Some(((days * 24 + hours) * 60 + minutes) * 60_000 + seconds * 1000 + millis)
}

/// Days from 1970-01-01 to a calendar date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_from_march = (month + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The milliseconds since 1970 on this computer's clock.
pub fn real_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}

/// How long ago `iso` was, in words: `just now`, `3 minutes ago`, `2 hours ago`, `5 days ago`.
/// @param iso - the board's timestamp
/// @param now_ms - the time to measure from
pub fn ago(iso: &str, now_ms: i64) -> String {
    let Some(then) = parse_time(iso) else { return String::new() };
    let seconds = ((now_ms - then) / 1000).max(0);
    let (count, unit) = match seconds {
        0..=59 => return "just now".to_owned(),
        60..=3599 => (seconds / 60, "minute"),
        3600..=86_399 => (seconds / 3600, "hour"),
        _ => (seconds / 86_400, "day"),
    };
    match count {
        1 => format!("1 {unit} ago"),
        count => format!("{count} {unit}s ago"),
    }
}

/// The time left until `iso` as `4:32`, or `0:00` once it has passed.
/// @param iso - the board's timestamp
/// @param now_ms - the time to measure from
pub fn time_left(iso: &str, now_ms: i64) -> String {
    let seconds = parse_time(iso).map(|end| ((end - now_ms) / 1000).max(0)).unwrap_or(0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

// ------------------------------------------------------------------------------------ the state

/// Whether a board has been found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Presence {
    /// Nothing has been asked yet.
    #[default]
    Unknown,
    /// No board was found, with the sentence that says why.
    Missing(String),
    /// A request has reached a board.
    Found,
}

/// What the dialog is doing, which decides which cards it draws and what it keeps asking the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Face {
    /// The Connection and Scan cards, waiting for a phone.
    #[default]
    Scanning,
    /// A phone claimed the offer and the person is deciding.
    Confirming,
    /// `Confirm` was pressed: the phone's own results are awaited, then `Done`.
    Confirmed,
}

/// Everything about a pairing the desktop knows, shared by the dialog and the `pair` commands.
pub struct PairingState {
    source: Source,
    cache: Cache,
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: Sender<Done>,
    receiver: Receiver<Done>,
    next_tag: u64,
    /// The calls in flight, by tag, so an answer is applied by the route it answers.
    pending: HashMap<u64, Call>,
    /// Tags a command wants the raw answer of, and the answers that have arrived for them.
    wanted: HashSet<u64>,
    answers: HashMap<u64, Result<Answer, String>>,
    /// A time to use instead of the clock, which a test holds so a picture and a countdown do not drift.
    held_clock: Option<i64>,
    pub presence: Presence,
    pub status: Option<Status>,
    pub network: Option<Network>,
    /// The results of the last `Test`, and whether one is running.
    pub tests: Vec<Probe>,
    pub testing: bool,
    /// The offer being shown, and what the board last said about it.
    pub offer: Option<Offer>,
    pub offer_status: Option<OfferStatus>,
    /// The addresses taken out of the offer, as `host:port`.
    pub unticked: BTreeSet<String>,
    pub devices: Vec<Device>,
    /// The device the last `Confirm` made.
    pub confirmed: Option<Device>,
    /// What went wrong last, as one sentence.
    pub problem: Option<String>,
    /// What `Run It` printed.
    pub serve_output: Option<String>,
    /// The username and lifetime of the last offer, so a renewal asks the same thing.
    last_offer_request: Value,
    making_offer: bool,
    poll: Option<u64>,
    next_poll_at: i64,
    next_offer_at: i64,
    devices_asked_at: i64,
}

impl PairingState {
    /// A state that finds a board through `source` and calls `wake` when an answer lands.
    /// @param source - where a board is looked for
    /// @param wake - asks the window to draw again, from any thread
    pub fn new(source: Source, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, receiver) = channel();
        PairingState {
            source,
            cache: Arc::new(Mutex::new(None)),
            wake,
            sender,
            receiver,
            next_tag: 1,
            pending: HashMap::new(),
            wanted: HashSet::new(),
            answers: HashMap::new(),
            held_clock: None,
            presence: Presence::Unknown,
            status: None,
            network: None,
            tests: Vec::new(),
            testing: false,
            offer: None,
            offer_status: None,
            unticked: BTreeSet::new(),
            devices: Vec::new(),
            confirmed: None,
            problem: None,
            serve_output: None,
            last_offer_request: Value::Null,
            making_offer: false,
            poll: None,
            next_poll_at: 0,
            next_offer_at: 0,
            devices_asked_at: 0,
        }
    }

    /// Look for boards in `folder` from now on. A conf file the source names is left as it is.
    /// @param folder - the `boards` folder inside the settings folder, or `None` when the window has none
    pub fn set_boards_folder(&mut self, folder: Option<PathBuf>) {
        self.source.folder = folder;
    }

    /// Use `source` from now on, forgetting the board found so far. A test names its scripted board this way,
    /// because the environment is shared by every test in a process.
    /// @param source - where a board is looked for
    pub fn use_source(&mut self, source: Source) {
        self.source = source;
        if let Ok(mut held) = self.cache.lock() {
            *held = None;
        }
        self.presence = Presence::Unknown;
    }

    /// Where a board is looked for.
    pub fn source(&self) -> &Source {
        &self.source
    }

    /// Replace the waker, which the window sets once it has a context to wake.
    /// @param wake - asks the window to draw again, from any thread
    pub fn use_waker(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.wake = wake;
    }

    /// Hold the clock at `ms`, or let it run again with `None`.
    /// @param ms - milliseconds since 1970
    pub fn hold_the_clock_at(&mut self, ms: Option<i64>) {
        self.held_clock = ms;
    }

    /// The time every countdown and every `ago` is measured from.
    pub fn now_ms(&self) -> i64 {
        self.held_clock.unwrap_or_else(real_now_ms)
    }

    /// Send `call` on a thread of its own. Answers its tag, which [`PairingState::take_answer`] collects the
    /// answer under when `wanted` is true.
    /// @param call - the request
    /// @param wanted - whether a command is waiting for the raw answer
    pub fn send(&mut self, call: Call, wanted: bool) -> u64 {
        let tag = self.next_tag;
        self.next_tag += 1;
        if wanted {
            self.wanted.insert(tag);
        }
        self.pending.insert(tag, call.clone());
        let (source, cache) = (self.source.clone(), self.cache.clone());
        let (sender, wake) = (self.sender.clone(), self.wake.clone());
        std::thread::Builder::new()
            .name("unluminous-pairing".to_owned())
            .spawn(move || {
                let result = send_call(&source, &cache, &call);
                // The window may have gone; a send to a closed channel ends this thread.
                let _ = sender.send(Done { tag, result });
                wake();
            })
            .ok();
        tag
    }

    /// The answer to a request a command asked for, once, or `None` while it is still on its way.
    /// @param tag - what [`PairingState::send`] answered
    pub fn take_answer(&mut self, tag: u64) -> Option<Result<Answer, String>> {
        self.answers.remove(&tag)
    }

    /// Whether any request is still on its way.
    pub fn is_busy(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take every answer that has arrived and apply each to the state. Answers whether anything changed.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Ok(done) = self.receiver.try_recv() {
            changed = true;
            let Some(call) = self.pending.remove(&done.tag) else { continue };
            self.apply(done.tag, &call, &done.result);
            if self.wanted.remove(&done.tag) {
                self.answers.insert(done.tag, done.result);
            }
        }
        changed
    }

    /// Whether an offer exists that can still change, so the window keeps asking about it.
    pub fn watching(&self) -> bool {
        self.offer.is_some()
            && self.offer_status.as_ref().map(|s| s.state.is_live()).unwrap_or(true)
    }

    // -------------------------------------------------------------------- applying answers

    /// Apply one answer to the state, by the route it answers.
    fn apply(&mut self, tag: u64, call: &Call, result: &Result<Answer, String>) {
        let answer = match result {
            Ok(answer) => answer,
            Err(problem) => return self.apply_failure(tag, call, problem),
        };
        self.presence = Presence::Found;
        let path = call.path.split('?').next().unwrap_or_default();
        if !answer.is_ok() {
            return self.apply_refusal(tag, path, call, answer);
        }
        self.problem = None;
        match (call.method, path) {
            (Method::Get, "/pair/status") | (Method::Put, "/pair/settings") => {
                self.status = Status::from_json(&answer.body)
            }
            (Method::Get, "/pair/network") => self.network = Some(Network::from_json(&answer.body)),
            (Method::Post, "/pair/test") => self.apply_test(&answer.body),
            (Method::Post, "/pair/tailscale/serve") => self.apply_serve(&answer.body),
            (Method::Post, "/pair/offers") => self.apply_offer(&answer.body),
            (Method::Get, "/pair/devices") => self.devices = devices_from_json(&answer.body),
            (Method::Get, offer) if offer.starts_with("/pair/offers/") => {
                self.apply_offer_status(tag, offer, &answer.body)
            }
            (Method::Post, confirm) if confirm.ends_with("/confirm") => {
                self.apply_confirmed(&answer.body)
            }
            (Method::Post, reject) if reject.ends_with("/reject") => {
                self.end_the_offer(OfferState::Rejected)
            }
            (Method::Delete, offer) if offer.starts_with("/pair/offers/") => {
                self.end_the_offer(OfferState::Cancelled)
            }
            _ => {}
        }
    }

    /// A request that did not reach a board.
    fn apply_failure(&mut self, tag: u64, call: &Call, problem: &str) {
        if self.poll == Some(tag) {
            self.poll = None;
        }
        if call.path == "/pair/offers" {
            self.making_offer = false;
        }
        if call.path == "/pair/test" {
            self.testing = false;
        }
        self.presence = Presence::Missing(problem.to_owned());
    }

    /// A request the board answered with an error status.
    fn apply_refusal(&mut self, tag: u64, path: &str, call: &Call, answer: &Answer) {
        if self.poll == Some(tag) {
            self.poll = None;
        }
        match path {
            "/pair/offers" => self.making_offer = false,
            "/pair/test" => self.testing = false,
            _ => {}
        }
        if call.method == Method::Get && path.starts_with("/pair/offers/") && answer.status == 404 {
            return self.end_the_offer(OfferState::Expired);
        }
        self.problem = Some(answer.problem());
    }

    fn apply_test(&mut self, body: &Value) {
        self.testing = false;
        self.tests = list(body, "results").iter().map(Probe::from_json).collect();
    }

    fn apply_serve(&mut self, body: &Value) {
        let output = text(body, "output");
        let command = text(body, "command");
        self.serve_output = Some(match output.is_empty() {
            true => command,
            false => output,
        });
    }

    /// A new offer: it replaces the one shown, and the one it replaces is cancelled.
    fn apply_offer(&mut self, body: &Value) {
        self.making_offer = false;
        let Some(offer) = Offer::from_json(body) else { return };
        if let Some(old) = self.offer.take() {
            if old.offer_id != offer.offer_id {
                self.send(
                    Call::of(Method::Delete, format!("/pair/offers/{}", old.offer_id)),
                    false,
                );
            }
        }
        self.offer_status = Some(OfferStatus {
            state: OfferState::Waiting,
            expires_at: offer.expires_at.clone(),
            ..OfferStatus::default()
        });
        self.offer = Some(offer);
        self.confirmed = None;
        self.next_poll_at = 0;
    }

    /// What the board said about an offer. Ignored when it is not the offer being shown.
    fn apply_offer_status(&mut self, tag: u64, path: &str, body: &Value) {
        if self.poll == Some(tag) {
            self.poll = None;
            self.next_poll_at = real_now_ms() + POLL_GAP_MS;
        }
        let shown = self.offer.as_ref().map(|offer| format!("/pair/offers/{}", offer.offer_id));
        if shown.as_deref() != Some(path) {
            return;
        }
        let status = OfferStatus::from_json(body);
        if let Some(device) = status.device.clone() {
            self.confirmed = Some(device);
        }
        self.offer_status = Some(status);
    }

    fn apply_confirmed(&mut self, body: &Value) {
        let device = body.get("device").map(Device::from_json);
        self.confirmed = device.clone();
        if let Some(status) = self.offer_status.as_mut() {
            status.state = OfferState::Confirmed;
            status.device = device;
        }
        self.devices_asked_at = 0;
    }

    /// The offer is over, for the reason in `state`.
    fn end_the_offer(&mut self, state: OfferState) {
        match self.offer_status.as_mut() {
            Some(status) => status.state = state,
            None => self.offer_status = Some(OfferStatus { state, ..OfferStatus::default() }),
        }
    }

    // -------------------------------------------------------------------- what the window asks for

    /// Ask for the status, the network and the device list: what the dialog needs when it opens.
    pub fn refresh(&mut self) {
        self.send(Call::of(Method::Get, "/pair/status"), false);
        self.send(Call::of(Method::Get, "/pair/network"), false);
        self.send(Call::of(Method::Get, "/pair/devices"), false);
    }

    /// Ask for a new offer. The addresses are the ticked ones, or left out when every address is ticked.
    /// @param username - the account the phone signs in as, or `None` for the board's owner
    /// @param ttl_seconds - how long the offer lives, or `None` for the board's default
    /// @param wanted - whether a command waits for the raw answer
    pub fn make_offer(
        &mut self,
        username: Option<&str>,
        ttl_seconds: Option<u64>,
        wanted: bool,
    ) -> u64 {
        let mut body = json!({});
        if let Some(username) = username {
            body["username"] = json!(username);
        }
        if let Some(ttl) = ttl_seconds {
            body["ttlSeconds"] = json!(ttl);
        }
        self.last_offer_request = body.clone();
        self.send_offer(body, wanted)
    }

    /// Ask for another offer with what the last one asked for, which is what a renewal and a changed tick do.
    pub fn make_the_same_offer_again(&mut self) {
        let body = match self.last_offer_request.is_object() {
            true => self.last_offer_request.clone(),
            false => json!({}),
        };
        self.send_offer(body, false);
    }

    fn send_offer(&mut self, mut body: Value, wanted: bool) -> u64 {
        if let Some(chosen) = self.chosen_addresses() {
            body["addresses"] = json!(chosen);
        }
        self.making_offer = true;
        self.send(Call::of(Method::Post, "/pair/offers").with(body), wanted)
    }

    /// The `host:port` of every address that is ticked, or `None` when all of them are.
    pub fn chosen_addresses(&self) -> Option<Vec<String>> {
        if self.unticked.is_empty() {
            return None;
        }
        let network = self.network.as_ref()?;
        Some(
            network
                .addresses
                .iter()
                .map(Address::host_port)
                .filter(|address| !self.unticked.contains(address))
                .collect(),
        )
    }

    /// Whether the address `host_port` is ticked.
    /// @param host_port - an address as `host:port`
    pub fn is_ticked(&self, host_port: &str) -> bool {
        !self.unticked.contains(host_port)
    }

    /// Tick or untick an address, and ask for a new offer that holds the ticked ones.
    /// @param host_port - an address as `host:port`
    pub fn toggle_address(&mut self, host_port: &str) {
        if !self.unticked.remove(host_port) {
            self.unticked.insert(host_port.to_owned());
        }
        self.make_the_same_offer_again();
    }

    /// `PUT /pair/settings` with the listen setting, then ask for the network again.
    /// @param listen - `all` makes the board answer on the home network, `loopback` only on this computer
    pub fn set_listen(&mut self, listen: Listen) {
        let call = Call::of(Method::Put, "/pair/settings").with(json!({ "listen": listen.word() }));
        self.send(call, false);
        self.send(Call::of(Method::Get, "/pair/network"), false);
    }

    /// `POST /pair/test`.
    pub fn run_the_test(&mut self) {
        self.testing = true;
        self.send(Call::of(Method::Post, "/pair/test"), false);
    }

    /// `POST /pair/tailscale/serve`, then ask for the network again.
    pub fn run_the_serve_command(&mut self) {
        self.send(Call::of(Method::Post, "/pair/tailscale/serve"), false);
        self.send(Call::of(Method::Get, "/pair/network"), false);
    }

    /// The id of the offer being shown.
    pub fn offer_id(&self) -> Option<&str> {
        self.offer.as_ref().map(|offer| offer.offer_id.as_str())
    }

    /// `POST /pair/offers/<id>/confirm`. Answers its tag, or `None` when there is no offer.
    /// @param wanted - whether a command waits for the raw answer
    pub fn confirm(&mut self, wanted: bool) -> Option<u64> {
        let id = self.offer_id()?.to_owned();
        Some(self.send(Call::of(Method::Post, format!("/pair/offers/{id}/confirm")), wanted))
    }

    /// `POST /pair/offers/<id>/reject`. Answers its tag, or `None` when there is no offer.
    /// @param wanted - whether a command waits for the raw answer
    pub fn reject(&mut self, wanted: bool) -> Option<u64> {
        let id = self.offer_id()?.to_owned();
        Some(self.send(Call::of(Method::Post, format!("/pair/offers/{id}/reject")), wanted))
    }

    /// `DELETE /pair/offers/<id>`. Answers its tag, or `None` when there is no offer.
    /// @param wanted - whether a command waits for the raw answer
    pub fn cancel(&mut self, wanted: bool) -> Option<u64> {
        let id = self.offer_id()?.to_owned();
        Some(self.send(Call::of(Method::Delete, format!("/pair/offers/{id}")), wanted))
    }

    /// Put the offer away when the dialog closes: delete it on the board and forget it here.
    ///
    /// Every offer but a confirmed one is deleted. The state the window last heard can belong to the offer a
    /// renewal just replaced, so deciding by it sometimes left the new offer alive on the board after the
    /// dialog closed; deleting an offer that has already ended does nothing on the board.
    pub fn close_the_offer(&mut self) {
        let confirmed =
            self.offer_status.as_ref().is_some_and(|status| status.state == OfferState::Confirmed);
        if self.offer.is_some() && !confirmed {
            self.cancel(false);
        }
        self.offer = None;
        self.offer_status = None;
        self.confirmed = None;
        self.tests.clear();
        self.serve_output = None;
    }

    /// Which face the dialog shows for the state of the offer.
    pub fn face(&self) -> Face {
        match self.offer_status.as_ref().map(|status| status.state) {
            Some(OfferState::Claimed) => Face::Confirming,
            Some(OfferState::Confirmed) => Face::Confirmed,
            _ => Face::Scanning,
        }
    }

    /// The phone that claimed the offer, while the person decides.
    pub fn claim(&self) -> Option<&Claim> {
        self.offer_status.as_ref().and_then(|status| status.claim.as_ref())
    }

    /// The device that was just paired, as the board's device list has it now (so its test results show).
    pub fn paired_device(&self) -> Option<&Device> {
        let confirmed = self.confirmed.as_ref()?;
        self.devices.iter().find(|device| device.id == confirmed.id).or(Some(confirmed))
    }

    // -------------------------------------------------------------------- what keeps happening

    /// Do what is due on this frame: keep one long poll of the offer in flight, and read the device list while
    /// the phone's own results are awaited. When `renew` is true the offer is replaced shortly before it ends.
    /// Answers how long until something is next due, so the window can ask to be drawn then.
    /// @param renew - true while the dialog is open and showing the offer
    pub fn tick(&mut self, renew: bool) -> Option<Duration> {
        let now = self.now_ms();
        let real = real_now_ms();
        if self.offer.is_some() {
            self.expire_locally(now);
            if self.watching() && self.poll.is_none() && real >= self.next_poll_at {
                self.poll_the_offer();
            }
            if renew && self.renewal_is_due(now, real) {
                self.next_offer_at = real + RENEW_RETRY_MS;
                self.make_the_same_offer_again();
            }
            self.read_the_devices_while_awaiting_the_phone(real);
        }
        self.next_due(real)
    }

    /// An offer whose time has passed is over, whatever the board has not yet said.
    fn expire_locally(&mut self, now: i64) {
        let Some(status) = self.offer_status.as_mut() else { return };
        let ended = parse_time(&status.expires_at).is_some_and(|end| now >= end);
        if ended && status.state == OfferState::Waiting {
            status.state = OfferState::Expired;
        }
    }

    fn poll_the_offer(&mut self) {
        let Some(id) = self.offer_id().map(str::to_owned) else { return };
        let path = format!("/pair/offers/{id}?wait={WAIT_SECONDS}");
        self.poll = Some(self.send(Call::of(Method::Get, path), false));
    }

    /// Whether a new offer is wanted: the one shown is a waiting one that ends in under
    /// [`RENEW_BEFORE_SECONDS`], or it has ended without being paired (rejected, expired or cancelled). No
    /// request for one may be in flight, and a failed try is not repeated for [`RENEW_RETRY_MS`].
    fn renewal_is_due(&self, now: i64, real: i64) -> bool {
        let state = self.offer_status.as_ref().map(|status| status.state);
        let ends = self.offer.as_ref().and_then(|offer| parse_time(&offer.expires_at));
        let soon = state == Some(OfferState::Waiting)
            && ends.is_some_and(|end| end - now < RENEW_BEFORE_SECONDS * 1000);
        let ended = matches!(
            state,
            Some(OfferState::Rejected | OfferState::Expired | OfferState::Cancelled)
        );
        (soon || ended) && !self.making_offer && real >= self.next_offer_at
    }

    /// While `Confirm` has been pressed and the phone has not posted its results, read the device list.
    fn read_the_devices_while_awaiting_the_phone(&mut self, now: i64) {
        let awaiting = self.face() == Face::Confirmed
            && self.paired_device().is_some_and(|device| device.last_test.is_none());
        if awaiting && now - self.devices_asked_at >= DEVICES_GAP_MS {
            self.devices_asked_at = now;
            self.send(Call::of(Method::Get, "/pair/devices"), false);
        }
    }

    /// How long until the next thing that is due: a poll, a renewal or the countdown's next second.
    fn next_due(&self, now: i64) -> Option<Duration> {
        self.offer.as_ref()?;
        let mut due = 1000;
        if self.watching() && self.poll.is_none() {
            due = due.min((self.next_poll_at - now).max(50));
        }
        if self.face() == Face::Confirmed {
            due = due.min(DEVICES_GAP_MS.max(500));
        }
        Some(Duration::from_millis(due as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conf_file_gives_the_pid_port_token_and_name() {
        let text = "# The Tasks board\npid = 4242\nport = 8096\ntoken = abc123\nname = Jasons-MacBook-Pro\n";
        let board = read_conf(text).expect("a board");
        assert_eq!(
            board,
            Board {
                pid: 4242,
                port: 8096,
                token: "abc123".into(),
                name: "Jasons-MacBook-Pro".into()
            }
        );
    }

    #[test]
    fn a_conf_file_without_a_port_or_a_token_is_not_a_board() {
        assert!(read_conf("pid = 1\ntoken = a\n").is_none());
        assert!(read_conf("pid = 1\nport = 8096\n").is_none());
        assert!(read_conf("pid = 1\nport = 8096\ntoken =\n").is_none());
        assert!(read_conf("not a conf file").is_none());
    }

    #[test]
    fn a_conf_file_keeps_a_value_that_holds_an_equals_sign() {
        let board = read_conf("port = 1\ntoken = a=b==\nname = x\n").expect("a board");
        assert_eq!(board.token, "a=b==");
    }

    #[test]
    fn an_iso_time_is_read_to_the_millisecond() {
        assert_eq!(parse_time("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_time("1970-01-02T00:00:01.5Z"), Some(86_400_000 + 1500));
        assert_eq!(parse_time("2026-11-23T17:15:07.397Z"), Some(1_795_454_107_397));
        assert_eq!(parse_time("yesterday"), None);
    }

    #[test]
    fn a_time_ago_is_said_in_the_largest_unit() {
        let now = parse_time("2026-11-23T17:15:07.397Z").expect("a time");
        assert_eq!(ago("2026-11-23T17:15:00.000Z", now), "just now");
        assert_eq!(ago("2026-11-23T17:14:00.000Z", now), "1 minute ago");
        assert_eq!(ago("2026-11-23T17:13:00.000Z", now), "2 minutes ago");
        assert_eq!(ago("2026-11-23T14:00:00.000Z", now), "3 hours ago");
        assert_eq!(ago("2026-11-20T14:00:00.000Z", now), "3 days ago");
    }

    #[test]
    fn the_time_left_counts_down_and_stops_at_nothing() {
        let end = parse_time("2026-11-23T17:20:00.000Z").expect("a time");
        assert_eq!(time_left("2026-11-23T17:20:00.000Z", end - 272_000), "4:32");
        assert_eq!(time_left("2026-11-23T17:20:00.000Z", end + 5000), "0:00");
    }

    #[test]
    fn the_typed_code_and_the_six_digits_are_shown_the_way_a_person_reads_them() {
        let offer = Offer { code: "ABCD1234EFGH".into(), ..Offer::default() };
        assert_eq!(offer.shown_code(), "ABCD-1234-EFGH");
        let offer = Offer { code: "ABCD-1234-EFGH".into(), ..Offer::default() };
        assert_eq!(offer.shown_code(), "ABCD-1234-EFGH");
        let claim = Claim { confirm_code: "482913".into(), ..Claim::default() };
        assert_eq!(claim.shown_code(), "482 913");
    }

    #[test]
    fn copy_link_copies_the_unluminous_link_while_web_links_are_off() {
        let offer = Offer {
            link: "unluminous://pair?v=1".into(),
            web_link: "https://unluminous.com/pair#v=1".into(),
            ..Offer::default()
        };
        const { assert!(!WEB_LINKS) };
        assert_eq!(offer.link_to_copy(), "unluminous://pair?v=1");
    }

    #[test]
    fn an_ipv6_host_is_written_in_brackets() {
        let address = Address { host: "fd7a::1".into(), port: 8196, ..Address::default() };
        assert_eq!(address.host_port(), "[fd7a::1]:8196");
        let address = Address { host: "192.168.0.48".into(), port: 8196, ..Address::default() };
        assert_eq!(address.host_port(), "192.168.0.48:8196");
    }

    #[test]
    fn a_device_is_found_by_its_id_or_by_its_name() {
        let devices = vec![
            Device { id: 3, name: "Jason's iPhone".into(), ..Device::default() },
            Device { id: 4, name: "iPad".into(), ..Device::default() },
        ];
        assert_eq!(find_device(&devices, "4").map(|d| d.name.as_str()), Some("iPad"));
        assert_eq!(find_device(&devices, "jason's iphone").map(|d| d.id), Some(3));
        assert!(find_device(&devices, "7").is_none());
    }

    #[test]
    fn a_source_with_no_folder_and_no_file_finds_no_board() {
        let problem = find_board(&Source::default()).expect_err("no board");
        assert!(problem.contains("no Unluminous settings folder"), "{problem}");
    }

    #[test]
    fn a_folder_with_no_running_board_finds_none() {
        let folder = std::env::temp_dir().join(format!("unluminous-boards-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        // A pid that cannot be a running process.
        std::fs::write(folder.join("1.conf"), "pid = 4294967\nport = 1\ntoken = t\nname = n\n")
            .expect("write");
        let found = find_board(&Source { folder: Some(folder.clone()), file: None });
        assert!(found.is_err(), "{found:?}");
        std::fs::remove_file(folder.join("1.conf")).ok();
        std::fs::remove_dir(&folder).ok();
    }
}
