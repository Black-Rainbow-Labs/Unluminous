//! Pairing a phone, from the window: the dialog, the Phones page and the `pair` commands (`task-30`).
//!
//! `services::pairing` is the half with no window in it: it finds the board, makes the requests on threads
//! and keeps what the board answered. `components::pair_dialog` and `components::phones_page` draw. This is the
//! window's half: when the dialog opens and closes, what each button does, how the `pair` commands are held
//! until the board answers, and what they reply.
//!
//! **One [`PairingState`] on the window is shared by the dialog and the commands.** `pair offer` makes the
//! offer the dialog would show, `pair wait` waits on the same long poll the dialog keeps, and the dialog opened
//! after `pair offer` shows that offer instead of making a second one.
//!
//! **Only the released binary reaches outside the process.** `report_reaches_out` is the flag the Report an
//! Issue modal already uses for the browser, and it is used here for the Tailscale download page for the same
//! reason: a window a test builds never opens a browser.

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use unluminous_cli::protocol::{code, Reply, Request};

use crate::app::cli::{no, Outcome, Waiting};
use crate::app::UnluminousApp;
use crate::components::pair_dialog::{self, Showing};
use crate::components::phones_page::{PhonesAsk, PhonesFacts};
use crate::services::pairing::{
    self as pairing, Answer, Call, Listen, Method, OfferState, Presence,
};
use crate::services::qr::{self, Matrix};

/// The modal's name for `modal open`.
pub const MODAL: &str = "pair-phone";

/// The page `Install Tailscale` points at.
const TAILSCALE_DOWNLOAD: &str = "https://tailscale.com/download";

/// How long a `pair` command waits for the board when `--timeout` is not given.
const ANSWER_WAIT: Duration = Duration::from_secs(45);

/// How long `pair wait` waits for a phone when `--timeout` is not given. The client waits 15 seconds unless it
/// is told longer, so a longer default would be cut off by it.
pub const CLAIM_WAIT: Duration = Duration::from_secs(10);

/// How long a read of the device list stays fresh while the Phones page is showing.
const DEVICES_FRESH: Duration = Duration::from_secs(5);

/// The dialog's own state: whether it is showing, and the code made for the offer's link.
#[derive(Default)]
pub struct PairDialog {
    pub showing: bool,
    /// The QR code of `link`, kept so it is not made again every frame.
    matrix: Option<(String, Matrix)>,
    /// What `Copy Link` and `Copy` last put on the clipboard, which a test reads because a window a test
    /// builds does not touch the real clipboard.
    pub copied: Option<String>,
}

/// What a `pair` command is waiting for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairStep {
    /// The board's answer to the request with this tag.
    Answer { tag: u64 },
    /// A phone to claim the offer, which the long poll the state keeps in flight learns.
    Claim,
    /// The device list, to find which device `device` names. Then `rename` is the new name, or the device is
    /// removed when it is `None`.
    Resolve { tag: u64, device: String, rename: Option<String> },
}

impl UnluminousApp {
    // ------------------------------------------------------------------ the dialog

    /// Open the dialog: ask the board about itself and make an offer, unless one is already live.
    pub(crate) fn open_the_pair_dialog(&mut self) {
        self.close_every_modal();
        self.prepare_the_pairing();
        self.pairing.problem = None;
        self.pair_dialog.showing = true;
        self.pairing.presence = Presence::Unknown;
        self.pairing.refresh();
        if !self.pairing.watching() {
            self.pairing.make_offer(None, None, false);
        }
    }

    /// Put the dialog away: the offer it showed is deleted on the board, which is what closing does.
    pub(crate) fn close_the_pair_dialog(&mut self) {
        self.pair_dialog.showing = false;
        self.pairing.close_the_offer();
    }

    /// Point the pairing at this window's settings folder and its waker, before a request is made.
    fn prepare_the_pairing(&mut self) {
        let wake = self.thread_waker();
        self.pairing.use_waker(wake);
        let folder = self.store.as_ref().map(|store| store.folder().join("boards"));
        self.pairing.set_boards_folder(folder);
    }

    /// Take what the board has answered, and do what is due: the long poll of the offer, and the renewal of an
    /// offer the dialog is showing. Called once a frame.
    pub(crate) fn pump_the_pairing(&mut self, ctx: &egui::Context) {
        let changed = self.pairing.pump();
        let renew = self.pair_dialog.showing;
        if let Some(after) = self.pairing.tick(renew) {
            ctx.request_repaint_after(after);
        }
        if changed {
            ctx.request_repaint();
        }
    }

    /// Draw the dialog, and carry out what was pressed.
    pub(crate) fn show_the_pair_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.pair_dialog.showing {
            return;
        }
        let ctx = ui.ctx().clone();
        self.keep_the_code_for_the_link();
        let matrix = self.pair_dialog.matrix.as_ref().map(|(_, matrix)| matrix);
        let outcome = pair_dialog::show(&ctx, &Showing { state: &self.pairing, matrix });
        self.act_on_the_pair_dialog(&ctx, outcome);
    }

    /// Make the QR code again only when the offer's link is not the one the code already holds.
    fn keep_the_code_for_the_link(&mut self) {
        let link = self.pairing.offer.as_ref().map(|offer| offer.link.clone());
        let held = self.pair_dialog.matrix.as_ref().map(|(link, _)| link.clone());
        if link == held {
            return;
        }
        self.pair_dialog.matrix =
            link.and_then(|link| qr::matrix(&link).map(|matrix| (link, matrix)));
    }

    /// Carry out what was pressed in the dialog.
    fn act_on_the_pair_dialog(&mut self, ctx: &egui::Context, outcome: pair_dialog::Outcome) {
        if outcome.close {
            self.close_the_pair_dialog();
            return;
        }
        if let Some(address) = outcome.toggle_address.as_deref() {
            self.pairing.toggle_address(address);
        }
        if let Some(listen) = outcome.set_listen {
            self.pairing.set_listen(listen);
        }
        if outcome.test {
            self.pairing.run_the_test();
        }
        self.copy_what_the_dialog_asked_for(ctx, &outcome);
        if outcome.run_serve {
            self.pairing.run_the_serve_command();
        }
        if outcome.check_again {
            self.check_the_board_again();
        }
        if outcome.open_download && self.report_reaches_out {
            let _ = crate::services::links::open_in_the_default_browser(TAILSCALE_DOWNLOAD);
        }
        if outcome.confirm {
            self.pairing.confirm(false);
        }
        if outcome.reject {
            self.pairing.reject(false);
        }
    }

    /// `Copy Link` and `Copy`: the text goes to the clipboard, and is remembered for a test to read.
    fn copy_what_the_dialog_asked_for(
        &mut self,
        ctx: &egui::Context,
        outcome: &pair_dialog::Outcome,
    ) {
        let text = if outcome.copy_link {
            self.pairing.offer.as_ref().map(|offer| offer.link_to_copy().to_owned())
        } else if outcome.copy_serve {
            let report = self.pairing.network.as_ref().map(|network| network.tailscale.clone());
            report.and_then(|report| pair_dialog::tailscale_text(&report).command)
        } else {
            None
        };
        if let Some(text) = text {
            ctx.copy_text(text.clone());
            self.pair_dialog.copied = Some(text);
        }
    }

    /// `Check Again`: ask the board about itself and the network again, and make an offer when there is none.
    fn check_the_board_again(&mut self) {
        self.prepare_the_pairing();
        self.pairing.presence = Presence::Unknown;
        self.pairing.refresh();
        if !self.pairing.watching() {
            self.pairing.make_offer(None, None, false);
        }
    }

    // ------------------------------------------------------------------ the Phones page

    /// What the Phones page needs: the paired devices, read again when they are a few seconds old.
    pub(crate) fn phones_facts(&mut self) -> PhonesFacts {
        self.prepare_the_pairing();
        let stale = self.phones_read_at.map(|at| at.elapsed() > DEVICES_FRESH).unwrap_or(true);
        if stale && !self.pairing.is_busy() {
            self.phones_read_at = Some(Instant::now());
            self.pairing.send(Call::of(Method::Get, "/pair/devices"), false);
        }
        PhonesFacts {
            board_missing: match &self.pairing.presence {
                Presence::Missing(problem) => Some(problem.clone()),
                _ => None,
            },
            devices: self.pairing.devices.clone(),
            now_ms: self.pairing.now_ms(),
            problem: self.pairing.problem.clone(),
        }
    }

    /// What the Phones page asked for.
    pub(crate) fn phones_page_asked(&mut self, ask: PhonesAsk) {
        match ask {
            PhonesAsk::Pair => self.open_the_pair_dialog(),
            PhonesAsk::Rename { id, name } => {
                let call = Call::of(Method::Patch, format!("/pair/devices/{id}"))
                    .with(json!({ "name": name }));
                self.send_and_read_the_devices_again(call);
            }
            PhonesAsk::Remove { id } => {
                self.send_and_read_the_devices_again(Call::of(
                    Method::Delete,
                    format!("/pair/devices/{id}"),
                ));
            }
        }
    }

    /// Send a change to a device, then read the list so the page shows it.
    fn send_and_read_the_devices_again(&mut self, call: Call) {
        self.prepare_the_pairing();
        self.pairing.send(call, false);
        self.pairing.send(Call::of(Method::Get, "/pair/devices"), false);
        self.phones_read_at = Some(Instant::now());
    }

    // ------------------------------------------------------------------ the commands

    /// The `pair` commands.
    pub(crate) fn cli_pair(&mut self, request: &Request, verb: &str) -> Outcome {
        self.prepare_the_pairing();
        let until = Instant::now() + timeout_of(request, ANSWER_WAIT);
        match verb {
            "status" => self.pair_ask(request, until, Call::of(Method::Get, "/pair/status")),
            "network" => self.pair_ask(request, until, Call::of(Method::Get, "/pair/network")),
            "test" => self.pair_ask(request, until, Call::of(Method::Post, "/pair/test")),
            "tailscale-serve" => {
                self.pair_ask(request, until, Call::of(Method::Post, "/pair/tailscale/serve"))
            }
            "devices" => self.pair_ask(request, until, Call::of(Method::Get, "/pair/devices")),
            "offer" => self.pair_offer(request, until),
            "wait" => self.pair_wait(request),
            "confirm" | "reject" | "cancel" => self.pair_end_the_offer(request, verb, until),
            "rename" | "revoke" => self.pair_device(request, verb, until),
            _ => crate::app::cli::unknown(request),
        }
    }

    /// Send `call` for a command, and hold the command until the board answers.
    fn pair_ask(&mut self, request: &Request, until: Instant, call: Call) -> Outcome {
        let tag = self.pairing.send(call, true);
        Outcome::Hold(Waiting::Pairing {
            step: PairStep::Answer { tag },
            until,
            command: request.command.clone(),
        })
    }

    /// `pair offer`: make an offer with no dialog, and answer with what a phone needs.
    fn pair_offer(&mut self, request: &Request, until: Instant) -> Outcome {
        let user = request.text("user").filter(|user| !user.trim().is_empty());
        let ttl = request.whole("ttl").map(|seconds| seconds as u64);
        let tag = self.pairing.make_offer(user.as_deref(), ttl, true);
        Outcome::Hold(Waiting::Pairing {
            step: PairStep::Answer { tag },
            until,
            command: request.command.clone(),
        })
    }

    /// `pair wait`: hold until a phone has claimed the offer, which the state's long poll learns.
    fn pair_wait(&mut self, request: &Request) -> Outcome {
        if self.pairing.offer.is_none() {
            return no(
                request,
                code::NOT_FOUND,
                "There is no offer to wait on. `pair offer` makes one.",
            );
        }
        let until = Instant::now() + timeout_of(request, CLAIM_WAIT);
        Outcome::Hold(Waiting::Pairing {
            step: PairStep::Claim,
            until,
            command: request.command.clone(),
        })
    }

    /// `pair confirm`, `pair reject` and `pair cancel`, which each act on the offer there is.
    fn pair_end_the_offer(&mut self, request: &Request, verb: &str, until: Instant) -> Outcome {
        let tag = match verb {
            "confirm" => self.pairing.confirm(true),
            "reject" => self.pairing.reject(true),
            _ => self.pairing.cancel(true),
        };
        match tag {
            Some(tag) => Outcome::Hold(Waiting::Pairing {
                step: PairStep::Answer { tag },
                until,
                command: request.command.clone(),
            }),
            None => no(request, code::NOT_FOUND, "There is no offer. `pair offer` makes one."),
        }
    }

    /// `pair rename` and `pair revoke`: read the device list, find the device, then change it.
    fn pair_device(&mut self, request: &Request, verb: &str, until: Instant) -> Outcome {
        let Some(device) = request.text("device").filter(|device| !device.trim().is_empty()) else {
            return no(request, code::USAGE, "Name the device, by its id or its name.");
        };
        let rename = match verb {
            "rename" => match request.text("name").filter(|name| !name.trim().is_empty()) {
                Some(name) => Some(name),
                None => return no(request, code::USAGE, "Say what to rename the device to."),
            },
            _ => None,
        };
        let tag = self.pairing.send(Call::of(Method::Get, "/pair/devices"), true);
        Outcome::Hold(Waiting::Pairing {
            step: PairStep::Resolve { tag, device, rename },
            until,
            command: request.command.clone(),
        })
    }

    /// Whether a held `pair` command can be answered now, and with what.
    pub(crate) fn pair_ready(&mut self, request: &Request, step: &mut PairStep) -> Option<Reply> {
        self.pairing.pump();
        match step.clone() {
            PairStep::Answer { tag } => {
                let answer = self.pairing.take_answer(tag)?;
                Some(pair_reply(request, answer))
            }
            PairStep::Claim => self.claim_reply(request),
            PairStep::Resolve { tag, device, rename } => {
                let answer = self.pairing.take_answer(tag)?;
                match self.change_the_device(request, answer, &device, rename) {
                    Ok(next) => {
                        *step = next;
                        None
                    }
                    Err(reply) => Some(*reply),
                }
            }
        }
    }

    /// The device list has arrived: find the device and send the change, or refuse.
    fn change_the_device(
        &mut self,
        request: &Request,
        answer: Result<Answer, String>,
        device: &str,
        rename: Option<String>,
    ) -> Result<PairStep, Box<Reply>> {
        let list = match answer {
            Ok(answer) if answer.is_ok() => pairing::devices_from_json(&answer.body),
            other => return Err(Box::new(pair_reply(request, other))),
        };
        let Some(found) = pairing::find_device(&list, device) else {
            let said = format!("No paired device is called {device}. `pair devices` lists them.");
            return Err(Box::new(Reply::failed(&request.command, code::NOT_FOUND, said)));
        };
        let path = format!("/pair/devices/{}", found.id);
        let call = match rename {
            Some(name) => Call::of(Method::Patch, path).with(json!({ "name": name })),
            None => Call::of(Method::Delete, path),
        };
        Ok(PairStep::Answer { tag: self.pairing.send(call, true) })
    }

    /// The reply to `pair wait` once the offer has changed, or `None` while it is still waiting.
    fn claim_reply(&self, request: &Request) -> Option<Reply> {
        let status = self.pairing.offer_status.as_ref()?;
        match status.state {
            OfferState::Waiting => None,
            OfferState::Claimed | OfferState::Confirmed => {
                let claim = status.claim.clone().unwrap_or_default();
                let said = format!(
                    "{} ({}) claimed the offer. The six digits are {}.",
                    claim.device_name,
                    claim.model,
                    claim.shown_code()
                );
                let result = json!({
                    "state": status.state.word(),
                    "deviceName": claim.device_name,
                    "platform": claim.platform,
                    "model": claim.model,
                    "appVersion": claim.app_version,
                    "confirmCode": claim.confirm_code,
                    "digits": claim.shown_code(),
                    "via": claim.via,
                    "claimedAt": claim.claimed_at,
                });
                Some(Reply::done(&request.command, said, result))
            }
            ended => {
                let said = format!("The offer ended ({}) before a phone claimed it.", ended.word());
                Some(Reply::failed(&request.command, code::NOT_APPLICABLE, said))
            }
        }
    }

    /// What to say when the time ran out waiting on a `pair` command.
    pub(crate) fn pair_timed_out(step: &PairStep) -> String {
        match step {
            PairStep::Claim => "No phone claimed the offer before the time ran out.".to_owned(),
            _ => "The board had not answered when the time ran out.".to_owned(),
        }
    }

    /// The listen setting the board reports, for a test that reads the dialog's switch.
    pub fn pair_listen(&self) -> Option<Listen> {
        self.pairing.status.as_ref().map(|status| status.listen)
    }
}

/// How long a command is given when `--timeout` names a number of milliseconds.
fn timeout_of(request: &Request, fallback: Duration) -> Duration {
    request
        .number("timeout")
        .filter(|value| *value >= 0.0)
        .map(|value| Duration::from_millis(value as u64))
        .unwrap_or(fallback)
}

/// The reply for a command from what the board answered.
/// @param request - the command
/// @param answer - the board's answer, or why it could not be reached
fn pair_reply(request: &Request, answer: Result<Answer, String>) -> Reply {
    let answer = match answer {
        Ok(answer) => answer,
        Err(problem) => {
            let kind =
                match problem.starts_with("No Tasks board") || problem.contains("not answering") {
                    true => code::NOT_FOUND,
                    false => code::FAILED,
                };
            return Reply::failed(&request.command, kind, problem);
        }
    };
    if !answer.is_ok() {
        let kind = match answer.status {
            404 => code::NOT_FOUND,
            403 | 409 | 410 | 429 => code::REFUSED,
            _ => code::FAILED,
        };
        return Reply::failed(&request.command, kind, answer.problem());
    }
    let verb = request.command.strip_prefix("pair.").unwrap_or(&request.command);
    let (said, result) = success_of(verb, &answer.body);
    Reply::done(&request.command, said, result)
}

/// The sentence and the data for a command the board answered with a success.
fn success_of(verb: &str, body: &Value) -> (String, Value) {
    match verb {
        "status" => (status_sentence(body), body.clone()),
        "network" => (network_sentence(body), body.clone()),
        "test" => (test_sentence(body), body.clone()),
        "tailscale-serve" => (format!("Ran: {}", text_of(body, "command")), body.clone()),
        "offer" => (offer_sentence(body), body.clone()),
        "confirm" => (confirm_sentence(body), body.clone()),
        "reject" => {
            ("The offer was rejected. No phone was paired.".to_owned(), json!({ "rejected": true }))
        }
        "cancel" => ("The offer was cancelled.".to_owned(), json!({ "cancelled": true })),
        "devices" => devices_answer(body),
        "rename" => (format!("Renamed the device to {}.", text_of(body, "name")), body.clone()),
        _ => (
            "The device was removed. Its token no longer works.".to_owned(),
            json!({ "removed": true }),
        ),
    }
}

fn text_of(body: &Value, key: &str) -> String {
    body.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn status_sentence(body: &Value) -> String {
    let count = |key: &str| body.get(key).and_then(Value::as_u64).unwrap_or_default();
    format!(
        "{} listens {} on port {}, with {} paired devices and {} live offers.",
        text_of(body, "name"),
        text_of(body, "listen"),
        count("port"),
        count("deviceCount"),
        count("liveOffers")
    )
}

fn network_sentence(body: &Value) -> String {
    let network = pairing::Network::from_json(body);
    format!(
        "{} addresses. Tailscale is {}.",
        network.addresses.len(),
        network.tailscale.state.word()
    )
}

fn test_sentence(body: &Value) -> String {
    let results = body.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
    let reachable = results
        .iter()
        .filter(|probe| probe.get("ok").and_then(Value::as_bool) == Some(true))
        .count();
    format!("{reachable} of {} addresses answered from this computer.", results.len())
}

fn offer_sentence(body: &Value) -> String {
    let offer = pairing::Offer::from_json(body).unwrap_or_default();
    format!(
        "An offer is ready. The code is {} and it expires at {}. Give the phone `link`, or have it scan the dialog.",
        offer.shown_code(),
        offer.expires_at
    )
}

fn confirm_sentence(body: &Value) -> String {
    let device = pairing::Device::from_json(body.get("device").unwrap_or(&Value::Null));
    format!("Paired {}. Its token is now issued.", device.name)
}

/// `pair devices` answers a list, which a reply holds under a key.
fn devices_answer(body: &Value) -> (String, Value) {
    let count = body.as_array().map(Vec::len).unwrap_or_default();
    (format!("{count} paired devices."), json!({ "devices": body.clone() }))
}
