//! GA4 Measurement Protocol bodies, built from what a window counted. Pure, so every limit is a test with
//! no network behind it.
//!
//! The limits are GA4's own, from the Measurement Protocol reference: 25 events a request, an event name
//! of 40 characters, 25 parameters an event, a parameter name of 40 characters and a value of 100. A body
//! that breaks one is accepted with a 2xx and silently dropped, so the only place to find out is here.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::{Category, Opening};

/// The most events one request may carry.
pub const MOST_EVENTS: usize = 25;
/// The most requests one send may become. What does not fit waits for the next one.
pub const MOST_REQUESTS_AT_ONCE: usize = 4;

/// The ids and the facts about this machine that every request carries.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub client_id: String,
    /// The window's session, which is the Unix time in seconds it started at, as GA4's own tag makes it.
    pub session_id: u64,
    pub version: String,
    pub debug: bool,
}

impl Context {
    /// The context for this window.
    ///
    /// @param client_id - this installation's id
    /// @param debug - whether to ask GA4 to show the events in DebugView
    pub fn for_this_window(client_id: String, debug: bool) -> Context {
        let session_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        Context { client_id, session_id, version: crate::build_info::VERSION.to_owned(), debug }
    }
}

/// The time now, in microseconds since the Unix epoch, which is what `timestamp_micros` is.
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_micros() as u64)
        .unwrap_or_default()
}

/// The operating system's name as GA4's device reports spell it.
pub fn operating_system() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "Macintosh",
        "linux" => "Linux",
        _ => "Other",
    }
}

/// One event, its name cut to 40 characters and every string value to 100.
///
/// @param name - the event's name
/// @param params - its parameters, a JSON object
pub fn event(name: &str, params: Value) -> Value {
    let mut cut = Map::new();
    if let Value::Object(params) = params {
        for (key, value) in params.into_iter().take(MOST_EVENTS - 2) {
            let key: String = key.chars().take(40).collect();
            let value = match value {
                Value::String(text) => Value::String(text.chars().take(100).collect()),
                other => other,
            };
            cut.insert(key, value);
        }
    }
    json!({ "name": name.chars().take(40).collect::<String>(), "params": Value::Object(cut) })
}

/// The `app_open` event.
///
/// @param opening - what the window says about itself
pub fn app_open(opening: &Opening) -> Value {
    event(
        "app_open",
        json!({
            "launch": opening.launch,
            "theme": opening.theme,
            "first_run": u8::from(opening.first_run),
        }),
    )
}

/// The `app_heartbeat` event, which carries the active time since the last request.
///
/// @param engaged - active milliseconds since the last request
pub fn heartbeat(engaged: u64) -> Value {
    let mut beat = event("app_heartbeat", json!({}));
    beat["params"]["engagement_time_msec"] = json!(engaged.max(1));
    beat
}

/// One `feature` event a counted name, most used first, and what did not fit.
///
/// @param tally - every name counted, by category
/// @param room - how many events the requests have room for
pub fn feature_rows(
    tally: BTreeMap<(Category, String), u32>,
    room: usize,
) -> (Vec<Value>, BTreeMap<(Category, String), u32>) {
    let mut rows: Vec<((Category, String), u32)> = tally.into_iter().collect();
    // Most used first, so a send that runs out of room keeps what matters and the rest waits.
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let room = room.min(MOST_EVENTS * MOST_REQUESTS_AT_ONCE);
    let left: BTreeMap<(Category, String), u32> =
        rows.split_off(room.min(rows.len())).into_iter().collect();
    let events = rows
        .into_iter()
        .map(|((category, name), count)| {
            event("feature", json!({ "category": category.name(), "feature": name, "uses": count }))
        })
        .collect();
    (events, left)
}

/// The request bodies for a list of events: 25 a body, at most [`MOST_REQUESTS_AT_ONCE`] bodies, each
/// event given the session and an engagement time, and each body the client id, the user properties and
/// the device.
///
/// @param context - the ids and the facts about this machine
/// @param events - the events, in order
/// @param timestamp - when they happened, in microseconds since the epoch
pub fn bodies(context: &Context, events: Vec<Value>, timestamp: u64) -> Vec<String> {
    let events: Vec<Value> = events
        .into_iter()
        .map(|mut one| {
            let params = &mut one["params"];
            params["session_id"] = json!(context.session_id.to_string());
            if params.get("engagement_time_msec").is_none() {
                params["engagement_time_msec"] = json!(1);
            }
            if context.debug {
                params["debug_mode"] = json!(1);
            }
            one
        })
        .collect();
    events
        .chunks(MOST_EVENTS)
        .take(MOST_REQUESTS_AT_ONCE)
        .map(|chunk| {
            json!({
                "client_id": context.client_id,
                "timestamp_micros": timestamp,
                "user_properties": {
                    "app_version": { "value": context.version },
                    "os": { "value": std::env::consts::OS },
                    "arch": { "value": std::env::consts::ARCH },
                },
                "device": {
                    "category": "desktop",
                    "operating_system": operating_system(),
                },
                "events": chunk,
            })
            .to_string()
        })
        .collect()
}

/// The `timestamp_micros` a spooled body was made at, which decides whether GA4 will still take it.
///
/// @param body - a request body
pub fn timestamp_of(body: &str) -> Option<u64> {
    serde_json::from_str::<Value>(body).ok()?.get("timestamp_micros")?.as_u64()
}
