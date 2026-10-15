//! Telemetry with no window: the names it will send, the clocks, the limits, the spool, and one real
//! `POST` to a scripted server on loopback.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde_json::Value;

use super::payload::{self, Context};
use super::sender::{self, Sender, Target};
use super::*;

/// The tally is one per process, so the tests that switch it on take turns.
static TURN: Mutex<()> = Mutex::new(());

fn turn() -> MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn scratch(name: &str) -> std::path::PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-telemetry-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A server on loopback that answers 204 to every request and hands each body back through a channel.
fn scripted_server() -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/mp/collect", listener.local_addr().unwrap());
    let (bodies, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            let _ = bodies.send(String::from_utf8_lossy(&body).into_owned());
            let mut stream = stream;
            let _ = stream.write_all(
                b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            );
        }
    });
    (address, received)
}

fn context() -> Context {
    Context {
        client_id: "abc123".into(),
        session_id: 1_700_000_000,
        version: "9.9.9".into(),
        debug: false,
    }
}

#[test]
fn a_plain_name_is_sent_as_it_is_and_anything_else_is_dropped() {
    assert_eq!(clean_name("toggle-terminal"), "toggle-terminal");
    assert_eq!(clean_name("tab.open"), "tab.open");
    assert_eq!(clean_name("Plain text"), "Plain text");
    assert_eq!(clean_name("C:\\Users\\jason\\secret.txt"), "");
    assert_eq!(clean_name("what \"they\" typed"), "");
    assert_eq!(clean_name(&"a".repeat(300)).len(), 100);
}

#[test]
fn a_third_party_plugin_is_never_named() {
    assert_eq!(clean_name("plugin-run:agent-chat:send"), "plugin-run:agent-chat:send");
    assert_eq!(clean_name("plugin-run:my-private-plugin:do"), "plugin-run:other");
    assert_eq!(clean_name("plugin-pane:acme/thing"), "plugin-pane:other");
    assert_eq!(clean_name("plugin:database"), "plugin:database");
}

#[test]
fn the_environment_wins_and_a_background_window_sends_nothing_unless_asked() {
    assert_eq!(Permission::for_the_binary(false, None), Permission::Allowed);
    assert_eq!(Permission::for_the_binary(true, None), Permission::Background);
    assert_eq!(Permission::for_the_binary(true, Some("on")), Permission::Allowed);
    assert_eq!(Permission::for_the_binary(true, Some("debug")), Permission::Allowed);
    assert_eq!(Permission::for_the_binary(false, Some("off")), Permission::Environment);
}

#[test]
fn nothing_is_counted_while_telemetry_is_off() {
    let _turn = turn();
    set_on(false);
    note(Category::Action, "save");
    assert!(take_tally().is_empty());
    set_on(true);
    note(Category::Action, "save");
    note(Category::Action, "save");
    note(Category::Command, "tab.open");
    let tally = take_tally();
    assert_eq!(tally.get(&(Category::Action, "save".into())), Some(&2));
    assert_eq!(tally.get(&(Category::Command, "tab.open".into())), Some(&1));
    note(Category::Action, "save");
    set_on(false);
    assert!(take_tally().is_empty(), "turning it off throws away what was not sent");
}

fn quiet_sender(folder: &std::path::Path) -> Sender {
    // Nothing listens on port 9 on loopback, so every post fails at once and goes to the spool.
    Sender::start(Target {
        url: "http://127.0.0.1:9/mp/collect".into(),
        folder: folder.into(),
        keep_a_copy: false,
    })
}

#[test]
fn open_focused_and_active_are_three_clocks() {
    let folder = scratch("clocks");
    let mut telemetry = Telemetry::new(quiet_sender(&folder), context(), Opening::default(), EVERY);
    telemetry.tick(0.0, true, true);
    // Thirty seconds of a focused window with input at the start: all active.
    for second in 1..=30 {
        telemetry.tick(second as f64, true, false);
    }
    // Ninety more focused seconds with no input: active until a minute has passed since the input.
    for second in 31..=120 {
        telemetry.tick(second as f64, true, false);
    }
    // A minute in the background.
    for second in 121..=180 {
        telemetry.tick(second as f64, false, false);
    }
    assert_eq!(telemetry.focused_seconds.round(), 120.0);
    assert_eq!(telemetry.active_seconds.round(), 59.0);
}

#[test]
fn a_machine_that_slept_does_not_count_the_night() {
    let folder = scratch("sleep");
    let mut telemetry = Telemetry::new(quiet_sender(&folder), context(), Opening::default(), EVERY);
    telemetry.tick(0.0, true, true);
    telemetry.tick(8.0 * 3600.0, true, true);
    assert!(telemetry.focused_seconds <= 5.0);
}

#[test]
fn panels_are_counted_only_while_somebody_is_using_the_window() {
    let folder = scratch("panels");
    let mut telemetry = Telemetry::new(quiet_sender(&folder), context(), Opening::default(), EVERY);
    telemetry.tick(0.0, true, true);
    let panels = vec!["editor".to_owned(), "terminal".to_owned()];
    for second in 1..=10 {
        telemetry.tick(second as f64, true, false);
        telemetry.sample(second as f64, true, &panels, Some("Rust"));
    }
    for second in 11..=20 {
        telemetry.tick(second as f64, false, false);
        telemetry.sample(second as f64, false, &panels, Some("Rust"));
    }
    assert_eq!(telemetry.panel_seconds.get("terminal").copied().map(f64::round), Some(10.0));
    assert_eq!(telemetry.tab_seconds.get("Rust").copied().map(f64::round), Some(10.0));
}

#[test]
fn the_first_request_is_ten_seconds_in_and_an_idle_window_sends_nothing_after_it() {
    let _turn = turn();
    set_on(false);
    let folder = scratch("due");
    let mut telemetry = Telemetry::new(quiet_sender(&folder), context(), Opening::default(), 60.0);
    telemetry.tick(0.0, false, false);
    assert!(!telemetry.due(5.0));
    assert!(telemetry.due(10.0));
    telemetry.send(10.0);
    assert!(!telemetry.due(500.0), "nothing was counted and nobody was active");
    telemetry.tick(501.0, true, true);
    telemetry.tick(503.0, true, false);
    assert!(telemetry.due(503.0));
}

#[test]
fn a_request_never_holds_more_than_twenty_five_events_and_what_does_not_fit_waits() {
    let mut tally = BTreeMap::new();
    for index in 0..150 {
        tally.insert((Category::Action, format!("action-{index:03}")), 1 + index as u32);
    }
    let (rows, left) = payload::feature_rows(tally, 200);
    assert_eq!(rows.len(), payload::MOST_EVENTS * payload::MOST_REQUESTS_AT_ONCE);
    assert_eq!(left.len(), 50);
    // The most used are sent first.
    assert_eq!(rows[0]["params"]["feature"], "action-149");
    let bodies = payload::bodies(&context(), rows, 1);
    assert_eq!(bodies.len(), 4);
    for body in &bodies {
        let value: Value = serde_json::from_str(body).unwrap();
        assert!(value["events"].as_array().unwrap().len() <= 25);
    }
}

#[test]
fn every_event_carries_the_session_and_an_engagement_time_and_no_name_breaks_a_limit() {
    let events = vec![
        payload::event(&"x".repeat(60), serde_json::json!({ "long": "y".repeat(300) })),
        payload::heartbeat(1234),
    ];
    let bodies = payload::bodies(&context(), events, 42);
    let body: Value = serde_json::from_str(&bodies[0]).unwrap();
    assert_eq!(body["client_id"], "abc123");
    assert_eq!(body["timestamp_micros"], 42);
    assert_eq!(body["user_properties"]["app_version"]["value"], "9.9.9");
    let first = &body["events"][0];
    assert_eq!(first["name"].as_str().unwrap().len(), 40);
    assert_eq!(first["params"]["long"].as_str().unwrap().len(), 100);
    assert_eq!(first["params"]["session_id"], "1700000000");
    assert_eq!(first["params"]["engagement_time_msec"], 1);
    assert_eq!(body["events"][1]["params"]["engagement_time_msec"], 1234);
    assert!(first["params"].get("debug_mode").is_none());
}

#[test]
fn a_spooled_body_older_than_seventy_two_hours_is_dropped() {
    let now = 100 * 3600 * 1_000_000u64;
    let young = format!("{{\"timestamp_micros\":{}}}", now - 3600 * 1_000_000);
    let old = format!("{{\"timestamp_micros\":{}}}", now - 80 * 3600 * 1_000_000);
    assert_eq!(sender::still_wanted(vec![old, young.clone(), "not json".into()], now), vec![young]);
}

#[test]
fn a_spool_is_claimed_once_and_never_while_its_window_runs() {
    let folder = scratch("claim");
    // A process id nothing has: well past what any operating system hands out.
    std::fs::write(folder.join("spool-4000000001.jsonl"), "x\n").unwrap();
    std::fs::write(folder.join(format!("spool-{}.jsonl", std::process::id())), "own\n").unwrap();
    let first = sender::claim_the_leftovers(&folder, 77);
    assert_eq!(first.len(), 1);
    assert!(first[0].file_name().unwrap().to_string_lossy().starts_with("sending-77-"));
    // A second window asking at once finds nothing left to claim: the window 77 is not running, but this
    // test's own process is, and its own spool stays its own.
    let second = sender::claim_the_leftovers(&folder, std::process::id());
    assert!(second
        .iter()
        .all(|path| !path.to_string_lossy().contains(&format!("spool-{}", std::process::id()))));
}

#[test]
fn the_client_id_is_made_once_and_kept() {
    let folder = scratch("id");
    let (id, made) = client_id(&folder);
    assert!(made);
    assert_eq!(id.len(), 32);
    let (again, made_again) = client_id(&folder);
    assert_eq!(again, id);
    assert!(!made_again);
}

#[test]
fn a_crash_log_that_grew_is_a_crash() {
    let settings = scratch("crash");
    let folder = folder_in(&settings);
    assert!(
        !crashed_since_last_start(&settings, &folder),
        "the first start has nothing to compare"
    );
    assert!(!crashed_since_last_start(&settings, &folder));
    std::fs::write(settings.join(crate::services::crash_log::FILE), "panicked at …").unwrap();
    assert!(crashed_since_last_start(&settings, &folder));
    assert!(!crashed_since_last_start(&settings, &folder));
}

#[test]
fn a_window_posts_its_opening_its_features_and_its_close_to_the_server() {
    let _turn = turn();
    let (address, received) = scripted_server();
    let folder = scratch("post");
    let sender = Sender::start(Target { url: address, folder: folder.clone(), keep_a_copy: true });
    let opening = Opening {
        launch: "desktop",
        theme: "unluminous/dark".into(),
        first_run: true,
        crashed: false,
    };
    let mut telemetry = Telemetry::new(sender, context(), opening, EVERY);
    set_on(true);
    telemetry.tick(0.0, true, true);
    note(Category::Action, "toggle-terminal");
    note(Category::Command, "tab.open");
    telemetry.tick(11.0, true, false);
    assert!(telemetry.due(11.0));
    telemetry.send(11.0);
    let first: Value =
        serde_json::from_str(&received.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
    let names: Vec<&str> =
        first["events"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names[0], "app_open");
    assert!(names.contains(&"app_heartbeat"));
    let features: Vec<&str> = first["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["name"] == "feature")
        .map(|e| e["params"]["feature"].as_str().unwrap())
        .collect();
    assert!(features.contains(&"toggle-terminal") && features.contains(&"tab.open"));
    telemetry.close(20.0, Duration::from_secs(10));
    set_on(false);
    let last: Value =
        serde_json::from_str(&received.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
    assert!(last["events"].as_array().unwrap().iter().any(|e| e["name"] == "app_close"));
    assert_eq!(telemetry.status()["posted"], 2);
    assert!(
        !folder.join(format!("spool-{}.jsonl", std::process::id())).exists(),
        "the close was posted, so nothing is spooled"
    );
    assert_eq!(std::fs::read_to_string(folder.join("sent.jsonl")).unwrap().lines().count(), 2);
}

#[test]
fn a_close_that_cannot_be_posted_is_left_in_the_spool_for_the_next_window() {
    let _turn = turn();
    let folder = scratch("spool");
    let mut telemetry = Telemetry::new(quiet_sender(&folder), context(), Opening::default(), EVERY);
    telemetry.tick(0.0, true, true);
    telemetry.close(5.0, Duration::from_secs(10));
    let spool = std::fs::read_to_string(folder.join(format!("spool-{}.jsonl", std::process::id())))
        .unwrap();
    assert!(spool.contains("app_close"));
}
