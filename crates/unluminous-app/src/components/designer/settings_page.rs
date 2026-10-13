//! The Designer page in the Settings window: two cards from `components::settings_kit`.
//!
//! `Designs` holds the folder the designs are kept in, the device a design opens at and the starter page a
//! new design is made from. `Agent requests` holds whether the element's markup goes with a request and how
//! many copies of a design History keeps. Each change is written to the plugin's `settings.conf` the moment
//! it is made, which is the file `settings set plugins.designer.<key>` writes too.

use crate::components::settings_kit::{self as kit, Column};
use crate::services::designer::config::{checked_folder, slashed, MOST_HISTORY_KEPT};
use crate::services::designer::{Designer, Device, Starter};
use crate::services::plugin_ui::{Look, Request};

/// The width of the folder's text box.
const FOLDER_WIDTH: f32 = 240.0;

/// Draw the page inside the rectangle every page gets.
///
/// @param designer - the provider
/// @param ui - where to draw
pub fn show(designer: &mut Designer, ui: &mut egui::Ui, _look: &Look<'_>) -> Vec<Request> {
    kit::plugin_page(ui, "designer-settings", |ui, column| cards(designer, ui, column))
}

/// The page's two cards, then writing what changed.
fn cards(designer: &mut Designer, ui: &mut egui::Ui, column: &mut Column) -> Vec<Request> {
    let mut requests = Vec::new();
    let mut reveal = false;
    let changed = the_designs_card(designer, ui, column, &mut reveal)
        | the_requests_card(designer, ui, column);
    if reveal {
        requests.push(Request::Reveal(designer.config.folder.clone()));
    }
    if changed {
        if let Err(why) = designer.write_the_configuration() {
            requests.push(Request::Message(why));
        }
    }
    if let Some(problem) = designer.problem.clone() {
        requests.push(Request::Message(problem));
        designer.problem = None;
    }
    requests
}

/// `Designs`: the folder, the device a design opens at and the starter page. Answers whether anything changed.
fn the_designs_card(
    designer: &mut Designer,
    ui: &mut egui::Ui,
    column: &mut Column,
    reveal: &mut bool,
) -> bool {
    let mut changed = false;
    let help = Some("A design is one HTML file in this folder, with its notes beside it in a .comments.json file. The folder is relative to the project.");
    kit::card(ui, column, "Where designs live", help, |card| {
        let mut folder = slashed(&designer.config.folder);
        let folder_help = Some("Relative to the project. A dot folder is shown in the explorer.");
        card.row("Designs folder", folder_help, |row| {
            if row.text("Designs folder", &mut folder, ".designs", FOLDER_WIDTH) {
                match checked_folder(&folder) {
                    Ok(wanted) if wanted != designer.config.folder => {
                        designer.config.folder = wanted;
                        changed = true;
                    }
                    Ok(_) => {}
                    Err(why) => designer.problem = Some(why),
                }
            }
            if row.ghost("Reveal") {
                *reveal = true;
            }
        });
        let devices: Vec<&str> = Device::ALL.iter().map(|device| device.title()).collect();
        let chosen = Device::ALL.iter().position(|device| *device == designer.config.device);
        card.row("Default device", Some("The width a design opens at."), |row| {
            if let Some(picked) = row.segmented("Default device", &devices, chosen, 56.0, false) {
                designer.config.device = Device::ALL[picked];
                changed = true;
            }
        });
        let starters: Vec<&str> = Starter::ALL.iter().map(|starter| starter.title()).collect();
        let chosen = Starter::ALL.iter().position(|starter| *starter == designer.config.starter);
        card.row("Starter page", Some("What New Design writes."), |row| {
            if let Some(picked) = row.segmented("Starter page", &starters, chosen, 90.0, false) {
                designer.config.starter = Starter::ALL[picked];
                changed = true;
            }
        });
    });
    changed
}

/// `Agent requests`: whether the markup goes with a request, and how many copies History keeps. Answers whether
/// anything changed.
fn the_requests_card(designer: &mut Designer, ui: &mut egui::Ui, column: &mut Column) -> bool {
    let mut changed = false;
    let help = Some("What goes to the Agent-Chat pane when a design is handed over.");
    kit::card(ui, column, "Requests to the agent", help, |card| {
        let mut send = designer.config.send_html;
        let send_help = Some("Off sends the selector and the excerpt alone.");
        if card.switch("Send the HTML of the element", send_help, &mut send) {
            designer.config.send_html = send;
            changed = true;
        }
        let mut kept = designer.config.history_kept as f32;
        card.row("History copies kept", Some("The newest are kept."), |row| {
            if row.slider("History copies kept", &mut kept, 1.0, MOST_HISTORY_KEPT as f32) {
                designer.config.history_kept = (kept.round() as usize).clamp(1, MOST_HISTORY_KEPT);
                changed = true;
            }
            row.suffix(&format!("{} for each design", designer.config.history_kept));
        });
    });
    changed
}
