//! Pairing a phone from the desktop (`task-30`): the Pair a Phone dialog, the Phones page and the `pair`
//! commands, against a scripted board.
//!
//! The board is a `TcpListener` in the test that replays the answers it is given and writes down every request
//! (`common::board`). A conf file in the real board's form names it. `UNLUMINOUS_BOARD_CONF` is how the released
//! binary is told which file to use; every test in a binary shares one environment, so a test names its file
//! through `PairingState::use_source` instead, which is the same file read the same way.
//!
//! The window's clock for countdowns is held, so a picture and a renewal do not depend on how long a test took.

mod common;

use common::board::*;
use common::*;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use serde_json::{json, Value};
use unluminous_app::services::pairing::{Face, Presence, Source};
use unluminous_app::UnluminousApp;

/// A window pointed at `board`, with its clock held at [`NOW_MS`].
fn pairing_harness(board: &ScriptedBoard) -> Harness<'static, UnluminousApp> {
    let mut harness = harness("");
    let pairing = &mut harness.state_mut().pairing;
    pairing.use_source(Source { folder: None, file: Some(board.conf.clone()) });
    pairing.hold_the_clock_at(Some(NOW_MS));
    harness
}

/// Move the held clock forward.
fn advance(harness: &mut Harness<'static, UnluminousApp>, seconds: i64) {
    let now = harness.state().pairing.now_ms();
    harness.state_mut().pairing.hold_the_clock_at(Some(now + seconds * 1000));
}

/// Scroll the dialog's cards down by `points`, with the pointer over them.
fn scroll_the_cards(harness: &mut Harness<'static, UnluminousApp>, points: f32) {
    harness.event(egui::Event::PointerMoved(egui::pos2(590.0, 400.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -points),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    steady(harness);
    steady(harness);
}

/// Open the dialog and wait until the board has answered what it needs to draw.
fn open_the_dialog(harness: &mut Harness<'static, UnluminousApp>) {
    did(harness, "modal open pair-phone");
    wait_until(harness, "the offer and the network", |app| {
        app.pairing.offer.is_some() && app.pairing.network.is_some()
    });
}

/// Wait for `count` requests of `method` to `path`.
fn wait_for_requests(
    harness: &mut Harness<'static, UnluminousApp>,
    board: &ScriptedBoard,
    method: &str,
    path: &str,
    count: usize,
) {
    let what = format!("{count} {method} {path}");
    wait_until(harness, &what, |_| board.requests(method, path).len() >= count);
}

#[test]
fn with_no_board_the_dialog_says_the_service_is_not_running_and_how_to_start_it() {
    let nowhere =
        std::env::temp_dir().join(format!("unluminous-no-board-{}.conf", std::process::id()));
    let mut harness = harness("");
    harness.state_mut().pairing.use_source(Source { folder: None, file: Some(nowhere) });
    did(&mut harness, "modal open pair-phone");
    wait_until(&mut harness, "the missing board", |app| {
        matches!(app.pairing.presence, Presence::Missing(_))
    });
    harness.get_by_label("Pair a Phone");
    harness.get_by_label("Check Again");
    harness.get_by_label("Done");
    assert!(harness.query_by_label("Test").is_none(), "none of the cards is drawn");
    let state = did(&mut harness, "modal state");
    assert_eq!(state["open"], "pair-phone");
}

#[test]
fn the_dialog_lists_the_addresses_with_a_tick_box_each_and_makes_an_offer() {
    let board = ScriptedBoard::start("addresses");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    for address in [
        "192.168.0.48:8196",
        "Scripted-Mac.local:8196",
        "scripted-mac.tail1234.ts.net:8196",
        "100.75.142.76:8196",
    ] {
        harness.get_by_label(&format!("Include {address}"));
    }
    harness.get_by_label("Reachable on this network");
    harness.get_by_label("Copy Link");
    harness.get_by_label("Test");
    let offers = board.requests("POST", "/pair/offers");
    assert_eq!(offers.len(), 1, "one offer is made when the dialog opens");
    assert_eq!(
        offers[0].header("x-skip-token"),
        Some(TOKEN),
        "every request carries the board's token"
    );
    assert!(!offers[0].body.contains("addresses"), "with every address ticked none is named");
}

/// What each Tailscale state draws, by the buttons it offers.
fn tailscale_draws(state: &str, present: &[&str], absent: &[&str]) {
    let board = ScriptedBoard::start(state);
    standard_answers(&board, state);
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    for label in present {
        harness.get_by_label(label);
    }
    for label in absent {
        assert!(harness.query_by_label(label).is_none(), "{state} should not offer {label}");
    }
}

#[test]
fn tailscale_not_installed_offers_the_download_page_and_check_again() {
    tailscale_draws("not-installed", &["Open Download Page", "Check Again"], &["Run It", "Copy"]);
}

#[test]
fn tailscale_that_needs_login_offers_check_again() {
    tailscale_draws("needs-login", &["Check Again"], &["Open Download Page", "Run It"]);
}

#[test]
fn tailscale_that_is_stopped_offers_check_again() {
    tailscale_draws("stopped", &["Check Again"], &["Open Download Page", "Run It"]);
}

#[test]
fn tailscale_running_with_serve_needed_offers_copy_and_run_it() {
    tailscale_draws("running", &["Run It", "Copy"], &["Open Download Page", "Check Again"]);
}

#[test]
fn run_it_asks_the_board_to_run_the_serve_command_and_copy_copies_it() {
    let board = ScriptedBoard::start("serve");
    standard_answers(&board, "running");
    board.answer(
        "POST",
        "/pair/tailscale/serve",
        200,
        json!({ "ok": true, "command": "tailscale serve --bg --tcp=8196 tcp://127.0.0.1:8196", "output": "Available within your tailnet" }),
    );
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    scroll_the_cards(&mut harness, 900.0);
    harness.get_by_label("Copy").click();
    steady(&mut harness);
    assert_eq!(
        harness.state().pair_dialog.copied.as_deref(),
        Some("tailscale serve --bg --tcp=8196 tcp://127.0.0.1:8196")
    );
    harness.get_by_label("Run It").click();
    wait_for_requests(&mut harness, &board, "POST", "/pair/tailscale/serve", 1);
    wait_until(&mut harness, "its output", |app| app.pairing.serve_output.is_some());
}

#[test]
fn copy_link_copies_the_unluminous_link() {
    let board = ScriptedBoard::start("copy-link");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    harness.get_by_label("Copy Link").click();
    steady(&mut harness);
    assert_eq!(harness.state().pair_dialog.copied.as_deref(), Some(LINK));
}

#[test]
fn the_switch_sets_the_listen_setting_on_the_board() {
    let board = ScriptedBoard::start("listen");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    harness.get_by_label("Reachable on this network").click();
    wait_for_requests(&mut harness, &board, "PUT", "/pair/settings", 1);
    let body: Value = serde_json::from_str(&board.requests("PUT", "/pair/settings")[0].body)
        .expect("a JSON body");
    assert_eq!(body, json!({ "listen": "loopback" }));
}

#[test]
fn unticking_an_address_asks_for_a_new_offer_without_it() {
    let board = ScriptedBoard::start("untick");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    harness.get_by_label("Include 192.168.0.48:8196").click();
    wait_for_requests(&mut harness, &board, "POST", "/pair/offers", 2);
    let body: Value =
        serde_json::from_str(&board.requests("POST", "/pair/offers")[1].body).expect("JSON");
    assert_eq!(
        body["addresses"],
        json!([
            "Scripted-Mac.local:8196",
            "scripted-mac.tail1234.ts.net:8196",
            "100.75.142.76:8196"
        ])
    );
}

#[test]
fn test_shows_each_address_result_and_says_where_it_ran() {
    let board = ScriptedBoard::start("test");
    standard_answers(&board, "running");
    board.answer(
        "POST",
        "/pair/test",
        200,
        json!({ "results": [
            { "kind": "lan", "host": "192.168.0.48", "port": 8196, "ok": true, "ms": 12, "reason": "ok", "message": "" },
            { "kind": "tailscale-ip", "host": "100.75.142.76", "port": 8196, "ok": false, "ms": null,
              "reason": "cannot-check-here", "message": "" }
        ] }),
    );
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    scroll_the_cards(&mut harness, 900.0);
    harness.get_by_label("Test").click();
    wait_until(&mut harness, "the results", |app| app.pairing.tests.len() == 2);
    steady(&mut harness);
    assert_eq!(harness.state().pairing.tests[0].sentence(), "Reachable, 12 ms");
}

#[test]
fn a_claim_shows_the_confirm_card_and_confirm_sends_the_confirm_request() {
    let board = ScriptedBoard::start("confirm");
    standard_answers(&board, "running");
    board.answer(
        "POST",
        "/pair/offers/offer-1/confirm",
        200,
        json!({ "device": a_device(3, "Jason's iPhone") }),
    );
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "claimed", "expiresAt": iso_after(300), "claim": a_claim() }),
    );
    advance(&mut harness, 2);
    wait_until(&mut harness, "the claim", |app| app.pairing.face() == Face::Confirming);
    steady(&mut harness);
    let claim = harness.state().pairing.claim().expect("a claim").clone();
    assert_eq!(claim.shown_code(), "482 913");
    assert_eq!(claim.device_name, "Jason's iPhone");
    harness.get_by_label("Reject");
    harness.get_by_label("Confirm").click();
    wait_for_requests(&mut harness, &board, "POST", "/pair/offers/offer-1/confirm", 1);
    let confirm = &board.requests("POST", "/pair/offers/offer-1/confirm")[0];
    assert_eq!(confirm.header("x-skip-token"), Some(TOKEN));
    wait_until(&mut harness, "the confirmed face", |app| app.pairing.face() == Face::Confirmed);
    // The phone posts its own results, and the device list shows them as they arrive.
    let mut device = a_device(3, "Jason's iPhone");
    device["lastTest"] = json!({ "at": iso_after(1), "results": [
        { "kind": "lan", "host": "192.168.0.48", "port": 8196, "ok": true, "ms": 9, "reason": "ok", "message": "" }
    ] });
    board.answer("GET", "/pair/devices", 200, json!([device]));
    advance(&mut harness, 3);
    wait_until(&mut harness, "the phone's results", |app| {
        app.pairing.paired_device().is_some_and(|device| device.last_test.is_some())
    });
    steady(&mut harness);
    harness.get_by_label("Done");
}

#[test]
fn reject_sends_the_reject_request_and_a_new_code_is_made() {
    let board = ScriptedBoard::start("reject");
    standard_answers(&board, "running");
    board.answer("POST", "/pair/offers/offer-1/reject", 204, Value::Null);
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "claimed", "expiresAt": iso_after(300), "claim": a_claim() }),
    );
    advance(&mut harness, 2);
    wait_until(&mut harness, "the claim", |app| app.pairing.face() == Face::Confirming);
    steady(&mut harness);
    board.answer("POST", "/pair/offers", 200, an_offer("offer-2", 300));
    harness.get_by_label("Reject").click();
    wait_for_requests(&mut harness, &board, "POST", "/pair/offers/offer-1/reject", 1);
    wait_for_requests(&mut harness, &board, "POST", "/pair/offers", 2);
    wait_until(&mut harness, "the new offer", |app| app.pairing.offer_id() == Some("offer-2"));
    assert_eq!(harness.state().pairing.face(), Face::Scanning);
}

#[test]
fn an_offer_is_renewed_before_it_expires_and_closing_deletes_the_offer() {
    let board = ScriptedBoard::start("renew");
    standard_answers(&board, "running");
    board.answer("POST", "/pair/offers", 200, an_offer("offer-1", 30));
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "waiting", "expiresAt": iso_after(30) }),
    );
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    assert_eq!(board.requests("POST", "/pair/offers").len(), 1);
    board.answer("POST", "/pair/offers", 200, an_offer("offer-2", 300));
    board.answer("DELETE", "/pair/offers/offer-2", 204, Value::Null);
    board.answer(
        "GET",
        "/pair/offers/offer-2",
        200,
        json!({ "state": "waiting", "expiresAt": iso_after(300) }),
    );
    // Fifteen seconds on, the offer has fewer than twenty left.
    advance(&mut harness, 15);
    wait_until(&mut harness, "the renewed offer", |app| app.pairing.offer_id() == Some("offer-2"));
    assert_eq!(board.requests("POST", "/pair/offers").len(), 2);
    wait_for_requests(&mut harness, &board, "DELETE", "/pair/offers/offer-1", 1);
    harness.get_by_label("Done").click();
    wait_for_requests(&mut harness, &board, "DELETE", "/pair/offers/offer-2", 1);
    assert!(harness.state().pairing.offer.is_none());
    assert!(!harness.state().pair_dialog.showing);
}

#[test]
fn the_phones_page_lists_the_devices_and_remove_deletes_one() {
    let board = ScriptedBoard::start("phones");
    standard_answers(&board, "running");
    board.answer(
        "GET",
        "/pair/devices",
        200,
        json!([a_device(3, "Jason's iPhone"), a_device(4, "Jason's iPad")]),
    );
    board.answer("DELETE", "/pair/devices/3", 204, Value::Null);
    board.answer("PATCH", "/pair/devices/4", 200, a_device(4, "Office iPad"));
    let mut harness = pairing_harness(&board);
    harness.set_size(egui::vec2(1180.0, 1300.0));
    did(&mut harness, "modal open settings --page phones");
    wait_until(&mut harness, "the devices", |app| app.pairing.devices.len() == 2);
    steady(&mut harness);
    harness.get_by_label("Pair a Phone...");
    harness.get_by_label("Rename Jason's iPhone");
    harness.get_by_label("Remove Jason's iPad");
    harness.get_by_label("Remove Jason's iPhone").click();
    wait_for_requests(&mut harness, &board, "DELETE", "/pair/devices/3", 1);
    assert_eq!(board.requests("DELETE", "/pair/devices/3")[0].header("x-skip-token"), Some(TOKEN));
    // Rename turns the name into a text box with Save beside it.
    harness.get_by_label("Rename Jason's iPad").click();
    steady(&mut harness);
    harness.get_by_label("Cancel rename");
    harness.state_mut().settings_window.phones.renaming = Some((4, "Office iPad".to_owned()));
    steady(&mut harness);
    harness.get_by_label("Save name").click();
    wait_for_requests(&mut harness, &board, "PATCH", "/pair/devices/4", 1);
    let body: Value =
        serde_json::from_str(&board.requests("PATCH", "/pair/devices/4")[0].body).expect("JSON");
    assert_eq!(body, json!({ "name": "Office iPad" }));
}

#[test]
fn pair_a_phone_on_the_page_opens_the_dialog() {
    let board = ScriptedBoard::start("page-opens");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    did(&mut harness, "modal open settings --page phones");
    steady(&mut harness);
    harness.get_by_label("Pair a Phone...").click();
    wait_until(&mut harness, "the dialog", |app| app.pair_dialog.showing);
    assert_eq!(did(&mut harness, "modal state")["open"], "pair-phone");
}

#[test]
fn the_menu_entry_opens_the_dialog() {
    let board = ScriptedBoard::start("menu");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    did(&mut harness, "action run pair-phone");
    assert!(harness.state().pair_dialog.showing);
}

// ------------------------------------------------------------------------------------ the commands

#[test]
fn every_pair_command_talks_to_the_board() {
    let board = ScriptedBoard::start("commands");
    standard_answers(&board, "running");
    board.answer("POST", "/pair/test", 200, json!({ "results": [
        { "kind": "lan", "host": "192.168.0.48", "port": 8196, "ok": true, "ms": 4, "reason": "ok", "message": "" }] }));
    board.answer(
        "POST",
        "/pair/tailscale/serve",
        200,
        json!({ "ok": true, "command": "tailscale serve", "output": "" }),
    );
    board.answer("GET", "/pair/devices", 200, json!([a_device(3, "Jason's iPhone")]));
    board.answer("PATCH", "/pair/devices/3", 200, a_device(3, "Desk"));
    board.answer("DELETE", "/pair/devices/3", 204, Value::Null);
    board.answer(
        "POST",
        "/pair/offers/offer-1/confirm",
        200,
        json!({ "device": a_device(3, "Jason's iPhone") }),
    );
    let mut harness = pairing_harness(&board);

    assert_eq!(did(&mut harness, "pair status")["name"], "Scripted Mac");
    assert_eq!(did(&mut harness, "pair network")["tailscale"]["state"], "running");
    assert_eq!(did(&mut harness, "pair test")["results"][0]["ok"], true);
    assert_eq!(did(&mut harness, "pair tailscale-serve")["ok"], true);
    assert_eq!(did(&mut harness, "pair devices")["devices"][0]["id"], 3);

    let offer = did(&mut harness, "pair offer --user jason --ttl 600");
    assert_eq!(offer["link"], LINK);
    assert_eq!(offer["code"], "K7QM4D2X9TRB");
    assert!(offer["webLink"]
        .as_str()
        .is_some_and(|link| link.starts_with("https://unluminous.com/pair")));
    let sent: Value =
        serde_json::from_str(&board.requests("POST", "/pair/offers")[0].body).expect("JSON");
    assert_eq!(sent, json!({ "username": "jason", "ttlSeconds": 600 }));

    // The long poll is shared with the dialog: the claim arrives and `pair wait` answers with the digits.
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "claimed", "expiresAt": iso_after(300), "claim": a_claim() }),
    );
    let claimed = did(&mut harness, "pair wait --timeout 20000");
    assert_eq!(claimed["digits"], "482 913");
    assert_eq!(claimed["deviceName"], "Jason's iPhone");

    assert_eq!(did(&mut harness, "pair confirm")["device"]["name"], "Jason's iPhone");
    assert_eq!(did(&mut harness, "pair rename 3 Desk")["name"], "Desk");
    assert_eq!(did(&mut harness, "pair rename \"jason's iphone\" Desk")["name"], "Desk");
    assert_eq!(did(&mut harness, "pair revoke 3")["removed"], true);
    assert_eq!(board.requests("DELETE", "/pair/devices/3").len(), 1);
    assert_eq!(refused(&mut harness, "pair revoke 99"), "not-found");
}

#[test]
fn pair_reject_and_cancel_end_the_offer() {
    let board = ScriptedBoard::start("end-offer");
    standard_answers(&board, "running");
    board.answer("POST", "/pair/offers/offer-1/reject", 204, Value::Null);
    let mut harness = pairing_harness(&board);
    did(&mut harness, "pair offer");
    did(&mut harness, "pair reject");
    assert_eq!(board.requests("POST", "/pair/offers/offer-1/reject").len(), 1);
    did(&mut harness, "pair offer");
    did(&mut harness, "pair cancel");
    assert_eq!(board.requests("DELETE", "/pair/offers/offer-1").len(), 1);
}

#[test]
fn pair_commands_are_refused_with_no_board_or_no_offer() {
    let nowhere =
        std::env::temp_dir().join(format!("unluminous-no-board-cmd-{}.conf", std::process::id()));
    let mut harness = harness("");
    harness.state_mut().pairing.use_source(Source { folder: None, file: Some(nowhere) });
    for line in [
        "pair status",
        "pair network",
        "pair test",
        "pair tailscale-serve",
        "pair devices",
        "pair offer",
    ] {
        assert_eq!(refused(&mut harness, line), "not-found", "{line}");
    }
    for line in ["pair wait", "pair confirm", "pair reject", "pair cancel"] {
        assert_eq!(refused(&mut harness, line), "not-found", "{line} has no offer");
    }
    assert_eq!(refused(&mut harness, "pair rename 3 Desk"), "not-found");
    assert_eq!(refused(&mut harness, "pair revoke 3"), "not-found");
}

#[test]
fn a_board_that_refuses_is_reported_with_its_own_sentence() {
    let board = ScriptedBoard::start("refuses");
    standard_answers(&board, "running");
    board.answer(
        "POST",
        "/pair/offers/offer-1/confirm",
        409,
        json!({ "error": "already_claimed", "message": "Another phone claimed this offer." }),
    );
    let mut harness = pairing_harness(&board);
    did(&mut harness, "pair offer");
    let reply = run(&mut harness, "pair confirm");
    assert!(!reply.ok);
    assert_eq!(reply.error.expect("an error").code, "refused");
    assert_eq!(reply.message, "Another phone claimed this offer.");
}

#[test]
fn pair_wait_times_out_when_no_phone_claims() {
    let board = ScriptedBoard::start("wait-times-out");
    standard_answers(&board, "running");
    let mut harness = pairing_harness(&board);
    did(&mut harness, "pair offer");
    assert_eq!(refused(&mut harness, "pair wait --timeout 300"), "timed-out");
}

// ------------------------------------------------------------------------------------ pictures

/// The same board for the pictures: Tailscale running and ready.
fn a_board_for_a_picture(name: &str, tailscale_state: &str) -> ScriptedBoard {
    let board = ScriptedBoard::start(name);
    standard_answers(&board, tailscale_state);
    board.answer(
        "POST",
        "/pair/offers/offer-1/confirm",
        200,
        json!({ "device": a_device(3, "Jason's iPhone") }),
    );
    board
}

#[test]
fn the_scan_picture_holds_the_link_as_a_qr_code_a_scanner_can_read() {
    let board = a_board_for_a_picture("scan-picture", "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    harness.snapshot(shot("pair_dialog_scan"));
    let decoded = decode_the_code(&mut harness);
    assert_eq!(decoded, LINK, "the picture of the dialog holds the offer's link");
}

#[test]
fn the_scan_picture_in_the_light_theme_is_the_same_dark_on_white_code() {
    let board = a_board_for_a_picture("scan-light", "running");
    let mut harness = pairing_harness(&board);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_command_line("theme set \"Unluminous Light\"", &ctx);
    steady(&mut harness);
    open_the_dialog(&mut harness);
    steady(&mut harness);
    harness.snapshot(shot("pair_dialog_scan_light"));
    assert_eq!(decode_the_code(&mut harness), LINK);
}

/// Render the window and read the QR code out of the picture.
fn decode_the_code(harness: &mut Harness<'static, UnluminousApp>) -> String {
    let picture = harness.render().expect("render the window");
    let gray = image::DynamicImage::ImageRgba8(picture).to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1, "exactly one QR code is in the picture");
    let (_, content) = grids[0].decode().expect("the code decodes");
    content
}

#[test]
fn the_confirm_picture() {
    let board = a_board_for_a_picture("confirm-picture", "running");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    board.answer(
        "GET",
        "/pair/offers/offer-1",
        200,
        json!({ "state": "claimed", "expiresAt": iso_after(300), "claim": a_claim() }),
    );
    advance(&mut harness, 2);
    wait_until(&mut harness, "the claim", |app| app.pairing.face() == Face::Confirming);
    steady(&mut harness);
    harness.snapshot(shot("pair_dialog_confirm"));
}

#[test]
fn the_tailscale_missing_picture() {
    let board = a_board_for_a_picture("tailscale-missing", "not-installed");
    let mut harness = pairing_harness(&board);
    open_the_dialog(&mut harness);
    // The Connection card is under the Scan card, so the picture is taken with the cards scrolled to it.
    steady(&mut harness);
    scroll_the_cards(&mut harness, 700.0);
    harness.snapshot(shot("pair_dialog_tailscale_missing"));
}

#[test]
fn the_phones_page_picture() {
    let board = a_board_for_a_picture("phones-picture", "running");
    let mut tested = a_device(4, "Jason's iPad");
    tested["lastTest"] = json!({ "at": iso_after(-60), "results": [] });
    board.answer("GET", "/pair/devices", 200, json!([a_device(3, "Jason's iPhone"), tested]));
    let mut harness = pairing_harness(&board);
    harness.set_size(egui::vec2(1180.0, 1300.0));
    did(&mut harness, "modal open settings --page phones");
    wait_until(&mut harness, "the devices", |app| app.pairing.devices.len() == 2);
    steady(&mut harness);
    harness.snapshot(shot("phones_page"));
}
