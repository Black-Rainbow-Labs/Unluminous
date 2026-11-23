//! `Settings -> Phones`: the phones paired with the Tasks board on this computer (`task-30`).
//!
//! Built from `components::settings_kit` like the other Settings pages. The first card holds `Pair a Phone...`,
//! which opens the dialog. Under it is one card for each paired phone with its model, the account it signs in
//! as, when it was paired, when and through which address it was last seen, and `Rename` and `Remove`.
//!
//! The desktop stores nothing here. The devices are read from the board, and `Rename` and `Remove` are requests
//! to the board that the window sends; what the page asks for comes back as a [`PhonesAsk`].

use egui::Rect;

use crate::components::settings_dialog::{drawn, Drawn};
use crate::components::settings_kit::{self as kit, Column};
use crate::services::pairing::{self, Device};

/// What the window knows about the paired phones, for the page to show.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhonesFacts {
    /// The sentence that says why no board was found, when none was.
    pub board_missing: Option<String>,
    pub devices: Vec<Device>,
    /// The time every `ago` is measured from.
    pub now_ms: i64,
    /// What went wrong last, as a sentence.
    pub problem: Option<String>,
}

/// What the page asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhonesAsk {
    /// `Pair a Phone...`.
    Pair,
    /// `Save` after `Rename`.
    Rename { id: u64, name: String },
    /// `Remove`.
    Remove { id: u64 },
}

/// The page's own state: which phone is being renamed, and the name being typed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhonesState {
    pub renaming: Option<(u64, String)>,
}

/// What the page reports: the usual [`Drawn`], and anything it asked for.
pub(crate) struct PhonesPage {
    pub(crate) drawn: Drawn,
    pub ask: Option<PhonesAsk>,
}

/// The line that says when a phone was last seen, and through which address.
/// @param device - the phone
/// @param now_ms - the time to measure from
pub fn last_seen_line(device: &Device, now_ms: i64) -> String {
    match (&device.last_seen_at, &device.last_seen_via) {
        (Some(at), Some(via)) => format!("{} through {via}", pairing::ago(at, now_ms)),
        (Some(at), None) => pairing::ago(at, now_ms),
        _ => "Not seen yet".to_owned(),
    }
}

/// The line that says what a phone is: its model and platform.
/// @param device - the phone
pub fn model_line(device: &Device) -> String {
    match (device.model.is_empty(), device.platform.is_empty()) {
        (false, false) => format!("{} ({})", device.model, device.platform),
        (false, true) => device.model.clone(),
        (true, false) => device.platform.clone(),
        (true, true) => "Unknown".to_owned(),
    }
}

/// Draw the page.
/// @param ui - the dialog's page area
/// @param area - where the cards go
/// @param state - which phone is being renamed
/// @param facts - what the window knows
pub(crate) fn show(
    ui: &mut egui::Ui,
    area: Rect,
    state: &mut PhonesState,
    facts: &PhonesFacts,
) -> PhonesPage {
    let mut ask = None;
    let mut column = Column::new(area.left(), area.right(), area.top());
    kit::card(ui, &mut column, "Pair a Phone", Some(INTRO), |card| {
        card.row("", None, |row| {
            if row.button("Pair a Phone...") {
                ask = Some(PhonesAsk::Pair);
            }
        });
        if let Some(missing) = &facts.board_missing {
            card.help(&format!(
                "{missing} Start it with `{}` in agent-tasks-service on a Mac.",
                pairing::START_COMMAND
            ));
        }
        if let Some(problem) = &facts.problem {
            card.help(problem);
        }
    });
    if facts.devices.is_empty() && facts.board_missing.is_none() {
        kit::card(ui, &mut column, "Paired phones", None, |card| {
            card.help("No phone is paired yet.")
        });
    }
    for device in &facts.devices {
        device_card(ui, &mut column, state, facts, device, &mut ask);
    }
    PhonesPage { drawn: drawn(area.top(), &column, false), ask }
}

const INTRO: &str = "A phone pairs with the Tasks board on this computer by scanning a code. Removing a phone signs it out at once, and it can pair again.";

/// One phone: what is known about it, and `Rename` and `Remove`.
fn device_card(
    ui: &mut egui::Ui,
    column: &mut Column,
    state: &mut PhonesState,
    facts: &PhonesFacts,
    device: &Device,
    ask: &mut Option<PhonesAsk>,
) {
    let title = match device.name.is_empty() {
        true => format!("Phone {}", device.id),
        false => device.name.clone(),
    };
    kit::card(ui, column, &title, None, |card| {
        card.row("Model", None, |row| row.suffix(&model_line(device)));
        card.row("Account", None, |row| row.suffix(&device.username));
        card.row("Paired", None, |row| row.suffix(&pairing::ago(&device.paired_at, facts.now_ms)));
        card.row("Last seen", None, |row| row.suffix(&last_seen_line(device, facts.now_ms)));
        if let Some(test) = &device.last_test {
            let said = format!(
                "{}, {} addresses tested",
                pairing::ago(&test.at, facts.now_ms),
                test.results.len()
            );
            card.row("Its own test", None, |row| row.suffix(&said));
        }
        let editing = matches!(&state.renaming, Some((id, _)) if *id == device.id);
        match editing {
            true => renaming_row(card, device, state, ask),
            false => buttons_row(card, device, state, ask),
        }
    });
}

/// `Rename` and `Remove`.
fn buttons_row(
    card: &mut kit::Card<'_, '_>,
    device: &Device,
    state: &mut PhonesState,
    ask: &mut Option<PhonesAsk>,
) {
    card.row("", None, |row| {
        if row.button(&format!("Rename {}", device.name)) {
            state.renaming = Some((device.id, device.name.clone()));
        }
        if row.button(&format!("Remove {}", device.name)) {
            *ask = Some(PhonesAsk::Remove { id: device.id });
        }
    });
}

/// The name being typed, with `Save` and `Cancel`.
fn renaming_row(
    card: &mut kit::Card<'_, '_>,
    device: &Device,
    state: &mut PhonesState,
    ask: &mut Option<PhonesAsk>,
) {
    let mut typed = state.renaming.as_ref().map(|(_, typed)| typed.clone()).unwrap_or_default();
    let (mut save, mut cancel) = (false, false);
    card.row("Name", None, |row| {
        row.text("Device name", &mut typed, "A name for this phone", 260.0);
        save = row.button("Save name");
        cancel = row.ghost("Cancel rename");
    });
    if save && !typed.trim().is_empty() {
        *ask = Some(PhonesAsk::Rename { id: device.id, name: typed.trim().to_owned() });
        state.renaming = None;
    } else if cancel {
        state.renaming = None;
    } else {
        state.renaming = Some((device.id, typed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phone() -> Device {
        Device {
            id: 3,
            name: "Jason's iPhone".into(),
            platform: "iOS".into(),
            model: "iPhone16,1".into(),
            paired_at: "2026-11-23T17:00:00.000Z".into(),
            last_seen_at: Some("2026-11-23T17:10:00.000Z".into()),
            last_seen_via: Some("192.168.0.48:8196".into()),
            ..Device::default()
        }
    }

    #[test]
    fn a_phone_is_described_by_its_model_and_platform() {
        assert_eq!(model_line(&phone()), "iPhone16,1 (iOS)");
        assert_eq!(model_line(&Device::default()), "Unknown");
    }

    #[test]
    fn last_seen_says_when_and_through_which_address() {
        let now = pairing::parse_time("2026-11-23T17:15:00.000Z").expect("a time");
        assert_eq!(last_seen_line(&phone(), now), "5 minutes ago through 192.168.0.48:8196");
        assert_eq!(last_seen_line(&Device::default(), now), "Not seen yet");
    }
}
