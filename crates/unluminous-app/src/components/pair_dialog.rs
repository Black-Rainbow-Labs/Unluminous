//! The Pair a Phone modal (`task-30`).
//!
//! `Unluminous -> Pair a Phone...` and `modal open pair-phone` open it, and so does `Pair a Phone...` on the
//! Phones page. It is drawn the way the Report an Issue modal is, with `components::realm::dialog`'s surface,
//! heading and footer and `components::settings_kit`'s cards, in the three cards of
//! `tasks/task-29-pairing-tdd.md` section 7.2:
//!
//! - **Connection**: the addresses a phone can be given with a tick box each and the result of `Test` beside
//!   each one, the switch `Reachable on this network`, and the Tailscale part of section 8.
//! - **Scan**: the QR code, `Copy Link`, the typed code and the time left.
//! - **Confirm**: shown once a phone has claimed the offer. The phone's name and model, the six digits, and
//!   `Confirm` and `Reject`. After `Confirm` it shows the phone's own test results as they arrive, and `Done`.
//!
//! With no board found the dialog is one sentence and the command that starts the board.
//!
//! Like every component it decides nothing. It draws what [`PairingState`] holds and answers what was
//! pressed in an [`Outcome`]; `app::pairing` does the rest.
//!
//! **The QR code is dark squares on a white plate in every theme**, with a quiet zone of four modules,
//! because a scanner needs dark on light and a dark theme's ground would make the code unreadable. The
//! squares are put on whole pixels so no two of them are separated by a seam.

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use rux::{Rux, Style};

use crate::components::modal;
use crate::components::realm::dialog;
use crate::components::settings_kit::{self as kit, Column};
use crate::services::pairing::{
    self as pairing, Address, Device, Face, Listen, PairingState, Presence, Probe, Tailscale,
    TailscaleState,
};
use crate::services::qr::{Matrix, QUIET_ZONE};

/// The modal's id, which is what `modal move`, `modal size` and `modal zoom` name it by.
pub const ID: &str = "unluminous-pair";
const WIDTH: f32 = 760.0;
const HEIGHT: f32 = 780.0;

/// How wide the plate holding the QR code is.
const PLATE: f32 = 264.0;

/// The command Tailscale is asked to run when the board's report names none.
pub const DEFAULT_SERVE_COMMAND: &str = "tailscale serve --bg --tcp=8196 tcp://127.0.0.1:8196";

/// The three steps on the phone, shown under every Tailscale state.
pub const PHONE_STEPS: [&str; 3] = [
    "Install Tailscale from the App Store.",
    "Sign in with the same account as this computer.",
    "Turn the VPN switch on in the Tailscale app.",
];

/// The sentence for an address Tailscale makes unreachable from this computer.
pub const CANNOT_CHECK_HERE: &str =
    "This computer cannot connect to its own Tailscale address. The phone's test will check it.";

/// The line under the list of addresses.
pub const FROM_THIS_COMPUTER: &str =
    "These results are from this computer. The phone tests each address itself after pairing.";

/// What the dialog shows besides the state.
pub struct Showing<'a> {
    pub state: &'a PairingState,
    /// The QR code of the offer's link, when there is an offer.
    pub matrix: Option<&'a Matrix>,
}

/// What a person pressed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// `Done`, `Cancel` or the cross: close, and delete the offer.
    pub close: bool,
    /// An address's tick box, by `host:port`.
    pub toggle_address: Option<String>,
    /// The switch `Reachable on this network`.
    pub set_listen: Option<Listen>,
    pub test: bool,
    pub copy_link: bool,
    /// `Copy` on the Tailscale command.
    pub copy_serve: bool,
    pub run_serve: bool,
    pub check_again: bool,
    /// The link to the Tailscale download page.
    pub open_download: bool,
    pub confirm: bool,
    pub reject: bool,
}

/// The words the Tailscale part of the Connection card shows for one report from the board.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TailscaleText {
    /// The sentence at the top.
    pub lead: String,
    /// The steps still to do, each one a sentence. Empty when there are none.
    pub steps: Vec<String>,
    /// The command to run, when one is needed: shown with `Copy` and `Run It`.
    pub command: Option<String>,
    /// Whether `Check Again` is offered.
    pub check_again: bool,
    /// Whether the link to the download page is offered.
    pub download: bool,
    /// A closing sentence about Tailscale being optional.
    pub footnote: Option<String>,
}

/// What the Tailscale card says for `report`, one case for each row of the table in section 8.
/// @param report - the `tailscale` part of `GET /pair/network`
pub fn tailscale_text(report: &Tailscale) -> TailscaleText {
    match report.state {
        TailscaleState::NotInstalled => TailscaleText {
            lead: "Tailscale lets your phone reach this computer from anywhere.".to_owned(),
            steps: vec![
                "Install Tailscale on this computer from https://tailscale.com/download."
                    .to_owned(),
                "Sign in to Tailscale.".to_owned(),
            ],
            check_again: true,
            download: true,
            footnote: Some(
                "Tailscale is optional. Without it the phone will work at home only.".to_owned(),
            ),
            ..TailscaleText::default()
        },
        TailscaleState::NeedsLogin | TailscaleState::Stopped => TailscaleText {
            lead: "Tailscale is installed but not connected.".to_owned(),
            steps: vec!["Open Tailscale and sign in, or run `tailscale up`.".to_owned()],
            check_again: true,
            ..TailscaleText::default()
        },
        TailscaleState::Error => TailscaleText {
            lead: match &report.problem {
                Some(problem) => format!("Tailscale did not answer: {problem}"),
                None => "Tailscale did not answer.".to_owned(),
            },
            check_again: true,
            ..TailscaleText::default()
        },
        TailscaleState::Running if report.serve.needed && !report.serve.configured => {
            TailscaleText {
                lead: "One more step: let Tailscale pass the board's port through.".to_owned(),
                command: Some(match report.serve.command.is_empty() {
                    true => DEFAULT_SERVE_COMMAND.to_owned(),
                    false => report.serve.command.clone(),
                }),
                ..TailscaleText::default()
            }
        }
        TailscaleState::Running => {
            TailscaleText { lead: connected_sentence(report), ..TailscaleText::default() }
        }
    }
}

/// `Tailscale is connected as macbook.tail1234.ts.net (100.75.142.76).`
fn connected_sentence(report: &Tailscale) -> String {
    let ip = report.ips.iter().find(|ip| !ip.contains(':')).or(report.ips.first());
    match (&report.dns_name, ip) {
        (Some(name), Some(ip)) => format!("Tailscale is connected as {name} ({ip})."),
        (Some(name), None) => format!("Tailscale is connected as {name}."),
        (None, Some(ip)) => format!("Tailscale is connected as {ip}."),
        (None, None) => "Tailscale is connected.".to_owned(),
    }
}

/// The lines about the phones the tailnet already has: `iPhone: connected`, or the sentence for none.
/// @param report - the `tailscale` part of `GET /pair/network`
pub fn phone_lines(report: &Tailscale) -> Vec<String> {
    if report.phones.is_empty() {
        return vec!["No phone is signed in to this tailnet yet.".to_owned()];
    }
    report
        .phones
        .iter()
        .map(|phone| match phone.online {
            true => format!("{}: connected", phone.name),
            false => format!("{}: not connected", phone.name),
        })
        .collect()
}

/// The sentence a probe shows. A probe that cannot be checked from here says why when the board sent none.
/// @param probe - the board's result for one address
pub fn probe_sentence(probe: &Probe) -> String {
    match (probe.reason.as_str(), probe.message.is_empty()) {
        ("cannot-check-here", true) => CANNOT_CHECK_HERE.to_owned(),
        _ => probe.sentence(),
    }
}

/// Whether the Windows Firewall line is called for: this computer's own test reached a home network
/// address and a phone's test could not (`no-route` or `timeout`). Section 9.3 names it on Windows only.
/// @param tests - this computer's results
/// @param devices - the paired devices, with the results each phone posted
pub fn firewall_may_be_blocking(tests: &[Probe], devices: &[Device]) -> bool {
    let passed_here = tests.iter().any(|probe| probe.kind == "lan" && probe.ok);
    let blocked_for_a_phone = devices.iter().any(|device| {
        device.last_test.as_ref().is_some_and(|test| {
            test.results.iter().any(|probe| {
                probe.kind == "lan" && matches!(probe.reason.as_str(), "no-route" | "timeout")
            })
        })
    });
    passed_here && blocked_for_a_phone
}

/// Draw the modal for what `showing` holds. Answers what was pressed.
/// @param ctx - the egui context
/// @param showing - the pairing state and the QR matrix
pub fn show(ctx: &egui::Context, showing: &Showing<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let (_, escaped) = modal::show_unframed(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        dialog::surface(ui, "pair", area);
        let inner = area.shrink2(Vec2::new(dialog::PAD_X, dialog::PAD_Y));
        let closed = kit::layer(ui, "pair-heading", inner, |rux| {
            dialog::heading(rux, inner, "Pair a Phone")
        });
        outcome.close |= closed;
        // The heading is named as well as drawn, so a test can say which dialog is showing.
        let named = ui.interact(
            Rect::from_min_size(inner.min, Vec2::new(220.0, dialog::CLOSE)),
            ui.id().with("pair-heading-name"),
            Sense::hover(),
        );
        named.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "Pair a Phone")
        });
        let below = inner.top() + dialog::CLOSE + 8.0;
        match &showing.state.presence {
            Presence::Missing(_) => no_board(ui, inner, below, &mut outcome),
            _ => body(ui, inner, below, showing, &mut outcome),
        }
    });
    outcome.close |= escaped;
    outcome
}

/// The whole dialog when no board was found: one sentence, the command, and `Check Again`.
fn no_board(ui: &mut egui::Ui, inner: Rect, top: f32, outcome: &mut Outcome) {
    let said = format!(
        "The Tasks board service is not running on this computer. Start it with `{}` in agent-tasks-service on a Mac.",
        pairing::START_COMMAND
    );
    let after = kit::layer(ui, "pair-no-board", inner, |rux| {
        let end =
            text_block(rux, Pos2::new(inner.left(), top + 8.0), inner.width(), &said, Ink::Body);
        kit::code_block(
            rux,
            Pos2::new(inner.left(), end + 12.0),
            inner.width(),
            "Start the board",
            pairing::START_COMMAND,
        )
        .bottom()
    });
    let _ = after;
    let foot = inner.bottom() - dialog::BUTTON;
    let footer = Rect::from_min_max(Pos2::new(inner.left(), foot), inner.max);
    let pressed = kit::layer(ui, "pair-no-board-footer", footer, |rux| {
        dialog::footer(rux, inner, &[("Check Again", true)], ("Done", true))
    });
    match pressed {
        Some(0) => outcome.check_again = true,
        Some(_) => outcome.close = true,
        None => {}
    }
}

/// The lead sentence, the cards that scroll, and the footer.
fn body(ui: &mut egui::Ui, inner: Rect, top: f32, showing: &Showing<'_>, outcome: &mut Outcome) {
    let foot = inner.bottom() - dialog::BUTTON - dialog::GAP;
    let lead = match showing.state.face() {
        Face::Scanning => {
            "Open Unluminous on your phone and scan the code, or type the code it asks for."
        }
        Face::Confirming => {
            "A phone is asking to pair. Nothing is shared with it until you confirm."
        }
        Face::Confirmed => "The phone is paired.",
    };
    let below_the_lead = kit::layer(ui, "pair-lead", inner, |rux| {
        text_block(rux, Pos2::new(inner.left(), top), inner.width(), lead, Ink::Quiet)
    }) + 6.0;
    let region = Rect::from_min_max(
        Pos2::new(inner.left() - dialog::PAD_X + 4.0, below_the_lead),
        Pos2::new(inner.right() + dialog::PAD_X - 4.0, foot - 6.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(region));
    child.set_clip_rect(region.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("pair-cards")
        .max_height(region.height())
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(&mut child, |ui| {
            let room = ui.available_rect_before_wrap();
            let mut column = Column::new(inner.left(), inner.right(), room.top() + 10.0);
            cards(ui, &mut column, showing, outcome);
            let drew = (column.pen - kit::CARD_GAP + 14.0 - room.top()).max(0.0);
            ui.allocate_space(Vec2::new(room.width(), drew));
        });
    footer(ui, inner, foot, showing.state.face(), outcome);
}

/// The footer: `Cancel` while a phone is asking, `Done` otherwise.
fn footer(ui: &mut egui::Ui, inner: Rect, foot: f32, face: Face, outcome: &mut Outcome) {
    let label = match face {
        Face::Confirming => "Cancel",
        _ => "Done",
    };
    let area = Rect::from_min_max(Pos2::new(inner.left(), foot), inner.max);
    let pressed =
        kit::layer(ui, "pair-footer", area, |rux| dialog::footer(rux, inner, &[], (label, true)));
    if pressed.is_some() {
        outcome.close = true;
    }
}

/// The cards for the face the dialog is on.
fn cards(ui: &mut egui::Ui, column: &mut Column, showing: &Showing<'_>, outcome: &mut Outcome) {
    match showing.state.face() {
        Face::Scanning => {
            // The code first: it is what a person came to do. The connections are under it.
            scan_card(ui, column, showing, outcome);
            connection_card(ui, column, showing, outcome);
        }
        Face::Confirming | Face::Confirmed => {
            confirm_card(ui, column, showing, outcome);
            connection_card(ui, column, showing, outcome);
        }
    }
    if let Some(problem) = showing.state.problem.as_deref() {
        kit::card(ui, column, "", None, |card| card.help(problem));
    }
}

// ------------------------------------------------------------------------------------ Connection

/// The Connection card: the switch, the addresses with their results, and Tailscale.
fn connection_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    showing: &Showing<'_>,
    outcome: &mut Outcome,
) {
    let state = showing.state;
    kit::card(
        ui,
        column,
        "Connection",
        Some(
            "The addresses the phone is given. Take the tick off one to leave it out of the code.",
        ),
        |card| {
            let mut all = state.status.as_ref().is_some_and(|status| status.listen == Listen::All);
            if card.switch(
                "Reachable on this network",
                Some("Off lets only programs on this computer reach the board, so no phone can."),
                &mut all,
            ) {
                outcome.set_listen = Some(if all { Listen::All } else { Listen::Loopback });
            }
            addresses(card, state, outcome);
            tailscale_section(card, state, outcome);
        },
    );
}

/// The list of addresses, `Test`, and the lines under them.
fn addresses(card: &mut kit::Card<'_, '_>, state: &PairingState, outcome: &mut Outcome) {
    let list: Vec<Address> =
        state.network.as_ref().map(|n| n.addresses.clone()).unwrap_or_default();
    if list.is_empty() {
        card.help("No address was found for this computer. Turn on Reachable on this network.");
    }
    for address in &list {
        let probe = state.tests.iter().find(|probe| probe.host_port() == address.host_port());
        card.custom(46.0, |rux, rect| {
            if address_row(rux, rect, address, state.is_ticked(&address.host_port()), probe) {
                outcome.toggle_address = Some(address.host_port());
            }
        });
    }
    card.custom(kit::BUTTON, |rux, rect| {
        let label = if state.testing { "Testing..." } else { "Test" };
        let button = Rect::from_min_size(
            rect.min,
            Vec2::new(kit::button_width(rux, label, false), kit::BUTTON),
        );
        if kit::button(rux, button, label, kit::Kind::Chip) && !state.testing {
            outcome.test = true;
        }
    });
    card.help(FROM_THIS_COMPUTER);
    if cfg!(windows) && firewall_may_be_blocking(&state.tests, &state.devices) {
        card.help("Windows Firewall may be blocking port 8196 for `tasks-api`.");
    }
}

/// One address: its tick box, its kind and `host:port`, and what `Test` found. Answers whether the tick box
/// was pressed.
fn address_row(
    rux: &mut Rux<'_>,
    rect: Rect,
    address: &Address,
    ticked: bool,
    probe: Option<&Probe>,
) -> bool {
    let theme = rux.theme();
    let name = format!("Include {}", address.host_port());
    let response = rux.ui.interact(rect, rux.ui.id().with(("pair-address", &name)), Sense::click());
    let tick =
        Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 9.0), Vec2::splat(18.0));
    modal::tick_box(rux, tick, ticked);
    let style = Style::sans(12.5).medium();
    let galley = rux.text(style, address.kind_label(), theme.ink.i900);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(tick.right() + 12.0, rect.top() + 13.0),
        galley,
        style,
        theme.ink.i900,
    );
    let mono = Style::mono(11.5);
    let galley = rux.text(mono, &address.host_port(), theme.ink.i500);
    rux.painter().galley(Pos2::new(tick.right() + 12.0, rect.top() + 24.0), galley, theme.ink.i500);
    if let Some(probe) = probe {
        let ink = match probe.ok {
            true => theme.ink.i700,
            false => theme.chat.slower,
        };
        let room = rect.width() * 0.5;
        let galley = rux.elided(Style::sans(12.0), &probe_sentence(probe), ink, room);
        let at = Pos2::new(rect.right() - galley.size().x, rect.center().y - galley.size().y / 2.0);
        rux.painter().galley(at, galley, ink);
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, ticked, name.clone())
    });
    response.clicked()
}

/// The Tailscale part: its words for the state, the command when one is needed, and the steps on the phone.
fn tailscale_section(card: &mut kit::Card<'_, '_>, state: &PairingState, outcome: &mut Outcome) {
    let report = state.network.as_ref().map(|n| n.tailscale.clone()).unwrap_or_default();
    let words = tailscale_text(&report);
    let output = state.serve_output.clone();
    card.grow(|rux, column| {
        let mut pen = column.pen + 6.0;
        pen = heading_line(rux, column.left, pen, "Tailscale");
        pen = text_block(
            rux,
            Pos2::new(column.left, pen + 4.0),
            column.width(),
            &words.lead,
            Ink::Body,
        );
        for (number, step) in words.steps.iter().enumerate() {
            let said = format!("{}. {step}", number + 1);
            pen = text_block(
                rux,
                Pos2::new(column.left + 8.0, pen + 4.0),
                column.width() - 8.0,
                &said,
                Ink::Body,
            );
        }
        if let Some(command) = &words.command {
            pen = serve_command(rux, &column, pen + 8.0, command, output.as_deref(), outcome);
        }
        pen = tailscale_buttons(rux, &column, pen + 8.0, &words, outcome);
        if let Some(note) = &words.footnote {
            pen = text_block(
                rux,
                Pos2::new(column.left, pen + 6.0),
                column.width(),
                note,
                Ink::Quiet,
            );
        }
        pen = phone_steps(rux, &column, pen + 12.0, &report);
        ((), pen)
    });
}

/// The command in a carved block, with `Copy` and `Run It` under it, and what running it printed.
fn serve_command(
    rux: &mut Rux<'_>,
    column: &Column,
    top: f32,
    command: &str,
    output: Option<&str>,
    outcome: &mut Outcome,
) -> f32 {
    let block = kit::code_block(
        rux,
        Pos2::new(column.left, top),
        column.width(),
        "Tailscale serve command",
        command,
    );
    let mut x = column.left;
    let y = block.bottom() + 8.0;
    for (label, which) in [("Copy", 0), ("Run It", 1)] {
        let width = kit::button_width(rux, label, false);
        let button = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, kit::BUTTON));
        if kit::button(rux, button, label, kit::Kind::Chip) {
            match which {
                0 => outcome.copy_serve = true,
                _ => outcome.run_serve = true,
            }
        }
        x = button.right() + 10.0;
    }
    let mut bottom = y + kit::BUTTON;
    if let Some(output) = output {
        bottom = text_block(
            rux,
            Pos2::new(column.left, bottom + 6.0),
            column.width(),
            output,
            Ink::Quiet,
        );
    }
    bottom
}

/// `Check Again` and the link to the download page, for the states that offer them.
fn tailscale_buttons(
    rux: &mut Rux<'_>,
    column: &Column,
    top: f32,
    words: &TailscaleText,
    outcome: &mut Outcome,
) -> f32 {
    if !words.check_again && !words.download {
        return top - 8.0;
    }
    let mut x = column.left;
    let mut shown: Vec<(&str, bool)> = Vec::new();
    if words.download {
        shown.push(("Open Download Page", true));
    }
    if words.check_again {
        shown.push(("Check Again", false));
    }
    for (label, is_download) in shown {
        let width = kit::button_width(rux, label, false);
        let button = Rect::from_min_size(Pos2::new(x, top), Vec2::new(width, kit::BUTTON));
        if kit::button(rux, button, label, kit::Kind::Chip) {
            match is_download {
                true => outcome.open_download = true,
                false => outcome.check_again = true,
            }
        }
        x = button.right() + 10.0;
    }
    top + kit::BUTTON
}

/// The three steps on the phone, then the phones the tailnet already has.
fn phone_steps(rux: &mut Rux<'_>, column: &Column, top: f32, report: &Tailscale) -> f32 {
    let mut pen = heading_line(rux, column.left, top, "On the phone");
    for (number, step) in PHONE_STEPS.iter().enumerate() {
        let said = format!("{}. {step}", number + 1);
        pen = text_block(
            rux,
            Pos2::new(column.left + 8.0, pen + 4.0),
            column.width() - 8.0,
            &said,
            Ink::Body,
        );
    }
    for line in phone_lines(report) {
        pen = text_block(rux, Pos2::new(column.left, pen + 6.0), column.width(), &line, Ink::Quiet);
    }
    pen
}

// ------------------------------------------------------------------------------------ Scan

/// The Scan card: the QR code, and beside it `Copy Link`, the typed code and the time left.
fn scan_card(ui: &mut egui::Ui, column: &mut Column, showing: &Showing<'_>, outcome: &mut Outcome) {
    let state = showing.state;
    kit::card(ui, column, "Scan", None, |card| {
        card.custom(PLATE, |rux, rect| {
            let plate = Rect::from_min_size(rect.min, Vec2::splat(PLATE));
            match showing.matrix {
                Some(matrix) => paint_qr(rux, plate, matrix),
                None => waiting_plate(rux, plate),
            }
            let beside = Rect::from_min_max(Pos2::new(plate.right() + 24.0, rect.top()), rect.max);
            scan_details(rux, beside, state, outcome);
        });
    });
}

/// What stands in for the code while there is no offer yet.
fn waiting_plate(rux: &mut Rux<'_>, plate: Rect) {
    let theme = rux.theme();
    kit::list_well(rux, plate, 16.0);
    let galley = rux.text(Style::sans(12.5), "Making a code...", theme.ink.i500);
    rux::text::draw_centred(rux.painter(), plate, galley, theme.ink.i500);
}

/// Beside the code: the buttons, the typed code and how long the code lasts.
fn scan_details(rux: &mut Rux<'_>, area: Rect, state: &PairingState, outcome: &mut Outcome) {
    let theme = rux.theme();
    let Some(offer) = &state.offer else { return };
    let button = Rect::from_min_size(
        area.min,
        Vec2::new(kit::button_width(rux, "Copy Link", false), kit::BUTTON),
    );
    if kit::button(rux, button, "Copy Link", kit::Kind::Chip) {
        outcome.copy_link = true;
    }
    let mut pen = heading_line(rux, area.left(), button.bottom() + 18.0, "Or type this code");
    let style = Style::mono(22.0).semibold();
    let galley = rux.text(style, &offer.shown_code(), theme.ink.i900);
    let height = galley.size().y;
    rux.painter().galley(Pos2::new(area.left(), pen + 6.0), galley, theme.ink.i900);
    pen += 6.0 + height;
    let left = pairing::time_left(&offer.expires_at, state.now_ms());
    let said = format!("Valid for {left}. A new code is made before this one ends.");
    pen = text_block(rux, Pos2::new(area.left(), pen + 10.0), area.width(), &said, Ink::Quiet);
    let help = "The phone asks for this computer's address and this code. Any address under Connection will do.";
    text_block(rux, Pos2::new(area.left(), pen + 8.0), area.width(), help, Ink::Quiet);
}

/// Paint the code as dark squares on a white plate with a quiet zone of four modules, each square on whole
/// pixels. The two colors are the same on any ground, which is what a scanner needs.
/// @param rux - the layer to paint into
/// @param plate - the square the code fills
/// @param matrix - the modules
pub fn paint_qr(rux: &mut Rux<'_>, plate: Rect, matrix: &Matrix) {
    let layout = qr_layout(plate, matrix.size, rux.ui.ctx().pixels_per_point());
    let painter = rux.painter().clone();
    let paper = Color32::WHITE; // any ground: a scanner needs dark squares on a light plate in every theme
    let ink = Color32::BLACK; // any ground: the same plate, the same squares
    painter.rect_filled(layout.plate, 10.0, paper);
    for y in 0..matrix.size {
        let mut x = 0;
        while x < matrix.size {
            if !matrix.is_dark(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < matrix.size && matrix.is_dark(x, y) {
                x += 1;
            }
            painter.rect_filled(layout.run(start, x, y), 0.0, ink);
        }
    }
}

/// Where the code's squares go: whole pixels, centered in the plate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QrLayout {
    /// The white plate, quiet zone included.
    pub plate: Rect,
    /// Where the first module's top left is.
    pub origin: Pos2,
    /// The side of one module in points, a whole number of pixels.
    pub module: f32,
}

impl QrLayout {
    /// The rectangle of the modules `from..to` in row `y`.
    /// @param from - the first column
    /// @param to - one past the last column
    /// @param y - the row
    pub fn run(&self, from: usize, to: usize, y: usize) -> Rect {
        let top = Pos2::new(
            self.origin.x + from as f32 * self.module,
            self.origin.y + y as f32 * self.module,
        );
        Rect::from_min_size(top, Vec2::new((to - from) as f32 * self.module, self.module))
    }
}

/// Lay a code of `size` modules out in `area` at `pixels_per_point`, so every module is a whole number of
/// pixels and the code is as large as it can be.
/// @param area - the room for the plate
/// @param size - how many modules there are along one side
/// @param pixels_per_point - how many pixels one point is on this screen
pub fn qr_layout(area: Rect, size: usize, pixels_per_point: f32) -> QrLayout {
    let total = (size + 2 * QUIET_ZONE) as f32;
    let pixels = (area.width().min(area.height()) * pixels_per_point / total).floor().max(1.0);
    let module = pixels / pixels_per_point;
    let side = module * total;
    let left = ((area.center().x - side / 2.0) * pixels_per_point).round() / pixels_per_point;
    let top = ((area.center().y - side / 2.0) * pixels_per_point).round() / pixels_per_point;
    let plate = Rect::from_min_size(Pos2::new(left, top), Vec2::splat(side));
    let quiet = QUIET_ZONE as f32 * module;
    QrLayout { plate, origin: plate.min + Vec2::splat(quiet), module }
}

// ------------------------------------------------------------------------------------ Confirm

/// The Confirm card: who is asking and the six digits, then `Confirm` and `Reject`. After `Confirm`: the
/// phone's own test results as they arrive.
fn confirm_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    showing: &Showing<'_>,
    outcome: &mut Outcome,
) {
    let state = showing.state;
    kit::card(ui, column, "Confirm", None, |card| match state.face() {
        Face::Confirmed => confirmed_rows(card, state),
        _ => claim_rows(card, state, outcome),
    });
}

/// The phone that is asking, the digits, and the two buttons.
fn claim_rows(card: &mut kit::Card<'_, '_>, state: &PairingState, outcome: &mut Outcome) {
    let Some(claim) = state.claim() else { return };
    let who = match claim.model.is_empty() {
        true => format!("{} wants to pair with this computer.", claim.device_name),
        false => {
            format!("{} ({}) wants to pair with this computer.", claim.device_name, claim.model)
        }
    };
    card.grow(|rux, column| {
        let mut pen = text_block(rux, Pos2::new(column.left, column.pen), column.width(), &who, Ink::Body);
        let theme = rux.theme();
        let style = Style::mono(40.0).semibold();
        let galley = rux.text(style, &claim.shown_code(), theme.ink.i900);
        let height = galley.size().y;
        rux.painter().galley(Pos2::new(column.left, pen + 12.0), galley, theme.ink.i900);
        pen += 12.0 + height;
        let said = "Check that the phone shows the same number, then press Confirm. If you do not know this phone, press Reject.";
        pen = text_block(rux, Pos2::new(column.left, pen + 10.0), column.width(), said, Ink::Quiet);
        buttons(rux, column.left, pen + 14.0, outcome);
        ((), pen + 14.0 + kit::BUTTON)
    });
}

/// `Confirm` on the action's gradient and `Reject` beside it.
fn buttons(rux: &mut Rux<'_>, left: f32, top: f32, outcome: &mut Outcome) {
    let confirm = Rect::from_min_size(Pos2::new(left, top), Vec2::new(110.0, kit::BUTTON));
    if kit::primary(rux, confirm, "Confirm", "Confirm") {
        outcome.confirm = true;
    }
    let width = kit::button_width(rux, "Reject", false);
    let reject =
        Rect::from_min_size(Pos2::new(confirm.right() + 10.0, top), Vec2::new(width, kit::BUTTON));
    if kit::button(rux, reject, "Reject", kit::Kind::Chip) {
        outcome.reject = true;
    }
}

/// After `Confirm`: the phone's name, and what it found when it tested each address.
fn confirmed_rows(card: &mut kit::Card<'_, '_>, state: &PairingState) {
    let device = state.paired_device().cloned().unwrap_or_default();
    let now = state.now_ms();
    card.grow(|rux, column| {
        let said = format!("Paired with {}.", device.name);
        let mut pen =
            text_block(rux, Pos2::new(column.left, column.pen), column.width(), &said, Ink::Body);
        match &device.last_test {
            Some(test) => {
                let from = format!("From {}, {}", device.name, pairing::ago(&test.at, now));
                pen = heading_line(rux, column.left, pen + 12.0, &from);
                for probe in &test.results {
                    let line = format!("{}  {}", probe.host_port(), probe.sentence());
                    pen = text_block(
                        rux,
                        Pos2::new(column.left + 8.0, pen + 4.0),
                        column.width() - 8.0,
                        &line,
                        Ink::Body,
                    );
                }
            }
            None => {
                let waiting = "Waiting for the phone to test each address...";
                pen = text_block(
                    rux,
                    Pos2::new(column.left, pen + 10.0),
                    column.width(),
                    waiting,
                    Ink::Quiet,
                );
            }
        }
        ((), pen)
    });
}

// ------------------------------------------------------------------------------------ text

/// Which ink a block of words is set in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    Body,
    Quiet,
}

/// Lay a paragraph out at `at`, wrapped to `width`. Answers where it ends.
fn text_block(rux: &mut Rux<'_>, at: Pos2, width: f32, said: &str, ink: Ink) -> f32 {
    let theme = rux.theme();
    let tint = match ink {
        Ink::Body => theme.ink.i700,
        Ink::Quiet => theme.ink.i500,
    };
    let style = Style::sans(12.5).leading(1.4);
    let galley = rux::text::wrapped(rux.painter(), style, said, tint, width.max(40.0));
    let height = galley.size().y;
    rux.painter().galley(at, galley, tint);
    at.y + height
}

/// A small heading at `(left, top)`. Answers where it ends.
fn heading_line(rux: &mut Rux<'_>, left: f32, top: f32, said: &str) -> f32 {
    let theme = rux.theme();
    let style = Style::sans(12.5).semibold();
    let galley = rux.text(style, said, theme.ink.i900);
    let height = galley.size().y;
    rux.painter().galley(Pos2::new(left, top), galley, theme.ink.i900);
    top + height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::pairing::{LastTest, Phone, Serve};

    fn report(state: TailscaleState) -> Tailscale {
        Tailscale {
            installed: state != TailscaleState::NotInstalled,
            state,
            ..Tailscale::default()
        }
    }

    #[test]
    fn tailscale_not_installed_says_to_install_it_and_sign_in() {
        let words = tailscale_text(&report(TailscaleState::NotInstalled));
        assert_eq!(words.lead, "Tailscale lets your phone reach this computer from anywhere.");
        assert_eq!(words.steps.len(), 2);
        assert!(words.steps[0].contains("https://tailscale.com/download"));
        assert!(words.steps[1].contains("Sign in"));
        assert!(words.check_again && words.download);
        assert!(words.footnote.as_deref().is_some_and(|note| note.contains("home only")));
    }

    #[test]
    fn tailscale_that_needs_login_or_is_stopped_says_it_is_not_connected() {
        for state in [TailscaleState::NeedsLogin, TailscaleState::Stopped] {
            let words = tailscale_text(&report(state));
            assert_eq!(words.lead, "Tailscale is installed but not connected.");
            assert_eq!(words.steps, ["Open Tailscale and sign in, or run `tailscale up`."]);
            assert!(words.check_again && !words.download);
        }
    }

    #[test]
    fn tailscale_running_with_serve_needed_shows_the_command_to_copy_and_run() {
        let mut running = report(TailscaleState::Running);
        running.serve = Serve { needed: true, configured: false, command: String::new() };
        let words = tailscale_text(&running);
        assert_eq!(words.lead, "One more step: let Tailscale pass the board's port through.");
        assert_eq!(words.command.as_deref(), Some(DEFAULT_SERVE_COMMAND));
        running.serve.command = "ts serve --bg --tcp=8196 tcp://127.0.0.1:8196".to_owned();
        assert_eq!(
            tailscale_text(&running).command.as_deref(),
            Some("ts serve --bg --tcp=8196 tcp://127.0.0.1:8196")
        );
    }

    #[test]
    fn tailscale_running_and_ready_names_its_name_and_address() {
        let mut running = report(TailscaleState::Running);
        running.dns_name = Some("macbook-pro-board.tail1234.ts.net".to_owned());
        running.ips = vec!["fd7a::1".to_owned(), "100.75.142.76".to_owned()];
        running.serve = Serve { needed: true, configured: true, command: String::new() };
        let words = tailscale_text(&running);
        assert_eq!(
            words.lead,
            "Tailscale is connected as macbook-pro-board.tail1234.ts.net (100.75.142.76)."
        );
        assert!(words.steps.is_empty() && words.command.is_none() && !words.check_again);
    }

    #[test]
    fn the_phones_the_tailnet_has_are_listed_or_the_lack_is_said() {
        let mut running = report(TailscaleState::Running);
        assert_eq!(phone_lines(&running), ["No phone is signed in to this tailnet yet."]);
        running.phones = vec![
            Phone { name: "iPhone".into(), online: true },
            Phone { name: "iPad".into(), online: false },
        ];
        assert_eq!(phone_lines(&running), ["iPhone: connected", "iPad: not connected"]);
    }

    #[test]
    fn a_probe_that_cannot_be_checked_here_says_why_even_when_the_board_said_nothing() {
        let probe = Probe { reason: "cannot-check-here".into(), ..Probe::default() };
        assert_eq!(probe_sentence(&probe), CANNOT_CHECK_HERE);
        let probe = Probe { ok: true, ms: Some(12), reason: "ok".into(), ..Probe::default() };
        assert_eq!(probe_sentence(&probe), "Reachable, 12 ms");
    }

    #[test]
    fn the_firewall_line_needs_a_pass_here_and_a_failure_on_the_phone() {
        let here = vec![Probe { kind: "lan".into(), ok: true, ..Probe::default() }];
        let blocked = Device {
            last_test: Some(LastTest {
                at: String::new(),
                results: vec![Probe {
                    kind: "lan".into(),
                    reason: "timeout".into(),
                    ..Probe::default()
                }],
            }),
            ..Device::default()
        };
        assert!(firewall_may_be_blocking(&here, std::slice::from_ref(&blocked)));
        assert!(!firewall_may_be_blocking(&[], &[blocked]));
        assert!(!firewall_may_be_blocking(&here, &[Device::default()]));
    }

    #[test]
    fn the_code_is_laid_out_on_whole_pixels_with_a_quiet_zone_of_four_modules() {
        for scale in [1.0_f32, 1.5, 2.0] {
            let area = Rect::from_min_size(Pos2::new(10.3, 20.7), Vec2::splat(PLATE));
            let layout = qr_layout(area, 61, scale);
            assert!((layout.module * scale).fract() == 0.0, "a module is whole pixels at {scale}");
            assert!(
                (layout.plate.min.x * scale).fract().abs() < 1e-3,
                "the plate starts on a pixel"
            );
            let quiet = layout.origin.x - layout.plate.min.x;
            assert!((quiet - 4.0 * layout.module).abs() < 1e-4);
            let side = layout.plate.width();
            assert!((side - (61.0 + 8.0) * layout.module).abs() < 1e-3);
            assert!(side <= PLATE + 1e-3, "{side} fits in {PLATE}");
        }
    }
}
