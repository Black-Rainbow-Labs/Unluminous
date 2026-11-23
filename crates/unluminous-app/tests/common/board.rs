//! A scripted Tasks board for the pairing tests (`task-30`).
//!
//! A `TcpListener` on a free loopback port replays the answers a test gives it and writes down every request it
//! gets: the method, the path, the headers and the body. A conf file in the same form the real board writes
//! (`boards/<port>.conf`) names it, and a window is pointed at that file with
//! `PairingState::use_source`. (`UNLUMINOUS_BOARD_CONF` is the same file named through the environment, which
//! the released binary reads; a test cannot use it because every test in a binary shares one environment.)

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// The token the scripted board's conf file holds, which every request has to carry.
pub const TOKEN: &str = "scripted-board-token";

/// One request the board received.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    /// The path with its query.
    pub path: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    /// The value of a header.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    /// The path without its query.
    pub fn route(&self) -> &str {
        self.path.split('?').next().unwrap_or_default()
    }
}

struct Route {
    method: String,
    path: String,
    status: u16,
    body: Value,
}

/// A board that answers what it was told to.
pub struct ScriptedBoard {
    pub port: u16,
    /// The conf file that names it.
    pub conf: PathBuf,
    folder: PathBuf,
    seen: Arc<Mutex<Vec<Seen>>>,
    routes: Arc<Mutex<Vec<Route>>>,
    stop: Arc<AtomicBool>,
}

impl ScriptedBoard {
    /// Start a board that answers `/health` and nothing else, and write the conf file that names it.
    /// @param name - a name for this test's folder
    pub fn start(name: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        let folder = std::env::temp_dir()
            .join(format!("unluminous-board-{name}-{}-{port}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("a folder");
        let conf = folder.join(format!("{port}.conf"));
        let text = format!(
            "pid = {}\nport = {port}\ntoken = {TOKEN}\nname = Scripted Mac\n",
            std::process::id()
        );
        std::fs::write(&conf, text).expect("the conf file");
        let board = ScriptedBoard {
            port,
            conf,
            folder,
            seen: Arc::new(Mutex::new(Vec::new())),
            routes: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        board.answer("GET", "/health", 200, json!({ "ok": true }));
        board.serve(listener);
        board
    }

    fn serve(&self, listener: TcpListener) {
        let (seen, routes, stop) = (self.seen.clone(), self.routes.clone(), self.stop.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let (seen, routes) = (seen.clone(), routes.clone());
                std::thread::spawn(move || handle(stream, &seen, &routes));
            }
        });
    }

    /// Answer `method` `path` (without its query) with `status` and `body` from now on.
    /// @param method - `GET`, `POST`...
    /// @param path - the route, such as `/pair/status`
    /// @param status - the HTTP status
    /// @param body - the JSON body
    pub fn answer(&self, method: &str, path: &str, status: u16, body: Value) {
        let mut routes = self.routes.lock().expect("the routes");
        routes.retain(|route| !(route.method == method && route.path == path));
        routes.push(Route { method: method.to_owned(), path: path.to_owned(), status, body });
    }

    /// Every request received so far.
    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().expect("the log").clone()
    }

    /// The requests received for `method` and `path` (without the query).
    pub fn requests(&self, method: &str, path: &str) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|seen| seen.method == method && seen.route() == path)
            .collect()
    }

    /// Forget what was received, so a test can ask what came after a point.
    pub fn forget(&self) {
        self.seen.lock().expect("the log").clear();
    }
}

impl Drop for ScriptedBoard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // One connection, to wake the accept loop so its thread ends.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        let _ = std::fs::remove_file(&self.conf);
        let _ = std::fs::remove_dir(&self.folder);
    }
}

/// Read one request, write it down and answer it.
fn handle(stream: TcpStream, seen: &Mutex<Vec<Seen>>, routes: &Mutex<Vec<Route>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) =
        (parts.next().unwrap_or("").to_owned(), parts.next().unwrap_or("").to_owned());
    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let (name, value) = (name.trim().to_ascii_lowercase(), value.trim().to_owned());
            if name == "content-length" {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let request = Seen {
        method: method.clone(),
        path: path.clone(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    seen.lock().expect("the log").push(request.clone());
    let route = path.split('?').next().unwrap_or_default().to_owned();
    let (status, answer) = {
        let routes = routes.lock().expect("the routes");
        match routes.iter().rev().find(|r| r.method == method && r.path == route) {
            Some(found) => (found.status, found.body.clone()),
            None => {
                (404, json!({ "error": "not_found", "message": format!("nothing at {route}") }))
            }
        }
    };
    let text = if status == 204 { String::new() } else { answer.to_string() };
    let head = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        text.len()
    );
    let mut stream = stream;
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.flush();
}

/// The time the pairing tests hold the window's clock at, as an ISO string and in milliseconds.
pub const NOW_ISO: &str = "2026-11-23T17:15:07.397Z";
pub const NOW_MS: i64 = 1_795_454_107_397;

/// An ISO time `seconds` after [`NOW_MS`].
/// @param seconds - how far after
pub fn iso_after(seconds: i64) -> String {
    let ms = NOW_MS + seconds * 1000;
    let days = ms.div_euclid(86_400_000);
    let rest = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest / 60_000 % 60,
        rest / 1000 % 60,
        rest % 1000
    )
}

/// The calendar date of a count of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 { month_from_march + 3 } else { month_from_march - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

/// The link the scripted offers carry. A real one is about this long, so its code is the size a phone scans.
pub const LINK: &str = "unluminous://pair?v=1&o=AAAAAAAAAAAAAAAAAAAAAA&s=BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB&f=CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC&n=Scripted%20Mac&a=192.168.0.48:8196,Scripted-Mac.local:8196,scripted-mac.tail1234.ts.net:8196,100.75.142.76:8196";

/// The four addresses of the link, as the board lists them.
pub fn addresses() -> Value {
    json!([
        { "kind": "lan", "host": "192.168.0.48", "port": 8196, "interface": "en0", "note": "" },
        { "kind": "mdns", "host": "Scripted-Mac.local", "port": 8196, "interface": "", "note": "" },
        { "kind": "tailscale-name", "host": "scripted-mac.tail1234.ts.net", "port": 8196, "interface": "utun4", "note": "" },
        { "kind": "tailscale-ip", "host": "100.75.142.76", "port": 8196, "interface": "utun4", "note": "" }
    ])
}

/// What Tailscale is doing, in the board's own words, for each state a test draws.
/// @param state - `running`, `needs-login`, `stopped`, `not-installed` or `error`
pub fn tailscale(state: &str) -> Value {
    let running = state == "running";
    json!({
        "installed": state != "not-installed",
        "state": state,
        "problem": if state == "error" { json!("tailscale status timed out") } else { Value::Null },
        "version": if state != "not-installed" { json!("1.80.0") } else { Value::Null },
        "dnsName": if running { json!("scripted-mac.tail1234.ts.net") } else { Value::Null },
        "ips": if running { json!(["100.75.142.76"]) } else { json!([]) },
        "magicDns": running,
        "userspace": running,
        "serve": {
            "needed": running,
            "configured": false,
            "command": "tailscale serve --bg --tcp=8196 tcp://127.0.0.1:8196"
        },
        "phones": if running { json!([{ "name": "iPhone", "online": true }]) } else { json!([]) }
    })
}

/// Give a board the answers a healthy one has: a status, a network, an offer and no devices.
/// @param board - the scripted board
/// @param state - what Tailscale is doing
pub fn standard_answers(board: &ScriptedBoard, state: &str) {
    board.answer(
        "GET",
        "/pair/status",
        200,
        json!({ "listen": "all", "port": 8196, "boardId": "b1", "name": "Scripted Mac",
                "fingerprint": "CCCC", "deviceCount": 0, "liveOffers": 1 }),
    );
    board.answer(
        "PUT",
        "/pair/settings",
        200,
        json!({ "listen": "loopback", "port": 8196, "boardId": "b1",
                "name": "Scripted Mac", "fingerprint": "CCCC", "deviceCount": 0, "liveOffers": 0 }),
    );
    board.answer(
        "GET",
        "/pair/network",
        200,
        json!({ "addresses": addresses(), "tailscale": tailscale(state) }),
    );
    board.answer("GET", "/pair/devices", 200, json!([]));
    board.answer("POST", "/pair/offers", 200, an_offer("offer-1", 300));
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "waiting", "expiresAt": iso_after(300) }),
    );
    board.answer("DELETE", "/pair/offers/offer-1", 204, Value::Null);
}

/// An offer the board answers `POST /pair/offers` with.
/// @param id - the offer id
/// @param seconds - how long from [`NOW_MS`] it lasts
pub fn an_offer(id: &str, seconds: i64) -> Value {
    json!({ "offerId": id, "link": LINK, "webLink": format!("https://unluminous.com/pair#{id}"),
            "code": "K7QM4D2X9TRB", "expiresAt": iso_after(seconds), "addresses": addresses(),
            "fingerprint": "CCCC" })
}

/// A paired device as the board lists it.
/// @param id - the device id
/// @param name - its name
pub fn a_device(id: u64, name: &str) -> Value {
    json!({ "id": id, "name": name, "platform": "iOS", "model": "iPhone16,1", "appVersion": "1.4.0",
            "username": "jason", "pairedAt": iso_after(-600), "lastSeenAt": iso_after(-120),
            "lastSeenVia": "192.168.0.48:8196", "lastTest": Value::Null })
}

/// A phone's claim on an offer, as `GET /pair/offers/<id>` shows it.
pub fn a_claim() -> Value {
    json!({ "deviceName": "Jason's iPhone", "platform": "iOS", "model": "iPhone16,1", "appVersion": "1.4.0",
            "confirmCode": "482913", "via": "192.168.0.48:8196", "claimedAt": iso_after(-5) })
}
