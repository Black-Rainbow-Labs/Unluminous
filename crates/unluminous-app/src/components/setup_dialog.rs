//! The Set Up Unluminous dialog (`task-2301`).
//!
//! It opens by itself while setup is not finished, and from `Unluminous -> Set Up Unluminous...`. One row a
//! step, each with a round tick box, a well that says its state, its name and one sentence about it. `Run`
//! takes the ticked rows in order, and each row is drawn again as its step starts and finishes; the speech
//! model's row shows a track while it downloads. A failed row has the button that fixes it.
//!
//! A window started with no project to open (`task-2305`) draws a Project card above the steps, with the folder
//! chosen and `Open Folder...` and `Create Project...`. The chosen folder is opened when setup has run, and with
//! nothing left to run the primary button is `Open Project`.
//!
//! It is built from the same furniture as the Report an Issue dialog: `components::realm::dialog`'s surface,
//! heading and footer, and one of `components::settings_kit`'s cards. Like every component it decides
//! nothing: it draws the plan it is handed, flips the tick boxes a person presses, and answers what was
//! pressed. `app::setup` does the rest.

use egui::{Pos2, Rect, Vec2};
use rux::components::round_check;
use rux::components::{Status, StatusWell, Track};
use rux::{Rux, Style};
use unluminous_cli::setup::State;

use crate::app::setup::{Plan, Row};
use crate::components::modal;
use crate::components::realm::dialog;
use crate::components::settings_kit::{self as kit, Column};

/// The modal's id, which is what `modal move`, `modal size` and `modal zoom` name it by.
pub const ID: &str = "unluminous-setup";
const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 680.0;

/// How tall one step's row is: its name on one line and its sentence on the next.
const ROW: f32 = 46.0;
/// The gap between two rows.
const ROW_GAP: f32 = 8.0;
/// The tick box and the state's well.
const CHECK: f32 = 22.0;
const WELL: f32 = 26.0;
/// How wide the state's word is given.
const WORD: f32 = 70.0;

/// What the dialog shows besides the plan.
pub struct Showing {
    /// A run is going on.
    pub running: bool,
    /// The plan is still being worked out.
    pub looking: bool,
    /// Something to say at the foot.
    pub problem: Option<String>,
    /// Whether this window may take the steps at all, which only the released binary may.
    pub can_run: bool,
    /// `Some` in a window waiting for a project, holding the folder chosen so far. `task-2305`.
    pub project: Option<Option<String>>,
}

/// What a person pressed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub close: bool,
    pub run: bool,
    pub stop: bool,
    /// The button on the failed row with this name.
    pub fix: Option<String>,
    /// The Project card's `Open Folder...`.
    pub open_folder: bool,
    /// The Project card's `Create Project...`.
    pub create_project: bool,
    /// `Open Project`, pressed with nothing left to run.
    pub open_project: bool,
}

/// Draw the dialog for `plan`. Answers what was pressed.
pub fn show(
    ctx: &egui::Context,
    plan: &mut Plan,
    showing: &Showing,
    dont_show: &mut bool,
) -> Outcome {
    let mut outcome = Outcome::default();
    let (_, escaped) = modal::show_unframed(ctx, ID, WIDTH, HEIGHT, |ui, area| {
        dialog::surface(ui, "setup", area);
        let inner = area.shrink2(Vec2::new(dialog::PAD_X, dialog::PAD_Y));
        let heading = "Set up Unluminous";
        if kit::layer(ui, "setup-heading", inner, |rux| dialog::heading(rux, inner, heading)) {
            outcome.close = true;
        }
        let named = ui.interact(
            Rect::from_min_size(inner.min, Vec2::new(240.0, dialog::CLOSE)),
            ui.id().with("setup-heading-name"),
            egui::Sense::hover(),
        );
        named.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, heading));
        let top = inner.top() + dialog::CLOSE + 8.0;
        let lead = match showing.project {
            Some(_) => "Choose the folder Unluminous opens. Nothing is opened until you do. Untick any step you do not want, then press Run Setup, and the folder opens when setup has run.",
            None => "Unluminous works best with these connected. Untick anything you do not want, then press Run Setup. You can come back here from Unluminous -> Set Up Unluminous.",
        };
        let below = kit::layer(ui, "setup-lead", inner, |rux| {
            kit::paragraph(rux, Pos2::new(inner.left(), top), inner.width(), lead)
        }) + 6.0;
        let foot = inner.bottom() - dialog::BUTTON - dialog::GAP;
        let region = Rect::from_min_max(
            Pos2::new(inner.left() - dialog::PAD_X + 4.0, below),
            Pos2::new(inner.right() + dialog::PAD_X - 4.0, foot - 6.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(region));
        child.set_clip_rect(region.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical()
            .id_salt("setup-steps")
            .max_height(region.height())
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(&mut child, |ui| {
                let room = ui.available_rect_before_wrap();
                let mut column = Column::new(inner.left(), inner.right(), room.top() + 10.0);
                if let Some(chosen) = &showing.project {
                    project(ui, &mut column, chosen.as_deref(), showing, &mut outcome);
                }
                steps(ui, &mut column, plan, showing, &mut outcome);
                let drew = (column.pen - kit::CARD_GAP + 14.0 - room.top()).max(0.0);
                ui.allocate_space(Vec2::new(room.width(), drew));
            });
        footer(ui, inner, foot, plan, showing, dont_show, &mut outcome);
    });
    if escaped {
        outcome.close = true;
    }
    outcome
}

/// The Project card: the folder chosen, or that none is, and the two ways of choosing one.
fn project(
    ui: &mut egui::Ui,
    column: &mut Column,
    chosen: Option<&str>,
    showing: &Showing,
    outcome: &mut Outcome,
) {
    let help = "The folder this window opens. Unluminous reads and indexes what is in it, so choose a project rather than a whole drive.";
    kit::card(ui, column, "Project", Some(help), |card| {
        card.row("Folder", None, |row| {
            row.path("Project folder", chosen.unwrap_or("No folder chosen yet"), f32::MAX);
        });
        if showing.running {
            return;
        }
        card.row("", None, |row| {
            if row.button("Open Folder...") {
                outcome.open_folder = true;
            }
            if row.button("Create Project...") {
                outcome.create_project = true;
            }
        });
    });
}

/// The card of steps, and the two lines under it.
fn steps(
    ui: &mut egui::Ui,
    column: &mut Column,
    plan: &mut Plan,
    showing: &Showing,
    outcome: &mut Outcome,
) {
    kit::card(ui, column, "", None, |card| {
        if showing.looking {
            card.help("Looking at what is set up on this machine...");
            return;
        }
        let count = plan.rows.len();
        let height = count as f32 * ROW + count.saturating_sub(1) as f32 * ROW_GAP;
        card.custom(height, |rux, rect| {
            for (index, row) in plan.rows.iter_mut().enumerate() {
                let top = rect.top() + index as f32 * (ROW + ROW_GAP);
                let place =
                    Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(rect.width(), ROW));
                step_row(rux, place, row, showing, outcome);
            }
        });
        if !plan.not_found.is_empty() {
            card.help(&format!("Not found on this machine: {}.", plan.not_found.join(", ")));
        }
        if !plan.keys.is_empty() {
            card.help(&plan.keys);
        }
    });
}

/// One step: the tick box, the state, the name and the sentence, and a button when it failed.
fn step_row(
    rux: &mut Rux<'_>,
    rect: Rect,
    row: &mut Row,
    showing: &Showing,
    outcome: &mut Outcome,
) {
    let theme = rux.theme();
    let middle = rect.top() + 14.0;
    // The tick box, for a step that has something to do. A step with nothing to do has none, so a row that
    // is finished cannot be ticked into doing nothing.
    let check =
        Rect::from_center_size(Pos2::new(rect.left() + CHECK / 2.0, middle), Vec2::splat(CHECK));
    let tickable = row.can_be_ticked()
        && matches!(row.state, State::Would | State::Failed | State::Declined)
        && !showing.running;
    if tickable {
        let label = format!("Set up {}", row.title);
        if round_check(rux, check, row.ticked, &label).clicked() {
            row.ticked = !row.ticked;
        }
    }
    let well = Rect::from_center_size(
        Pos2::new(check.right() + 14.0 + WELL / 2.0, middle),
        Vec2::splat(WELL),
    );
    match status_of(row) {
        Some(status) => {
            StatusWell::new(status).diameter(WELL).show(rux, well);
        }
        None => {
            let chat = theme.chat;
            rux.chrome.surface(well, WELL / 2.0, chat.well, chat.carved_sm());
            rux.painter().line_segment(
                [well.center() - Vec2::new(3.0, 0.0), well.center() + Vec2::new(3.0, 0.0)],
                egui::Stroke::new(1.7, theme.ink.i400),
            );
        }
    }
    let left = well.right() + 12.0;
    let fix = fix_label(row).filter(|_| showing.can_run && !showing.running);
    let fix_width = fix.map(|label| kit::button_width(rux, label, false) + 12.0).unwrap_or(0.0);
    let right = rect.right() - WORD - fix_width;
    let title = rux.elided(kit::label_style(), &row.title, theme.ink.i900, right - left);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left, middle),
        title,
        kit::label_style(),
        theme.ink.i900,
    );
    // The state's word, at the right, in the colour of its well.
    let word_colour = match status_of(row) {
        Some(status) => StatusWell::colour(rux, status),
        None => theme.ink.i500,
    };
    let word = rux.text(Style::sans(12.0), row.word(), word_colour);
    let at = Pos2::new(rect.right() - word.size().x, middle);
    rux::text::draw_left_capitals(rux.painter(), at, word, Style::sans(12.0), word_colour);
    let sentence_top = middle + 12.0;
    let message = plain(&row.message);
    // While a step says how far through it is, the track takes the right of the sentence's line, so what it
    // is doing stays readable beside how far it has got.
    let (sentence_right, track) = match row.fraction.filter(|_| row.running) {
        Some(fraction) => {
            let width = ((rect.right() - left) * 0.42).min(260.0);
            let track = Rect::from_min_size(
                Pos2::new(rect.right() - width, sentence_top + 3.0),
                Vec2::new(width, 12.0),
            );
            (track.left() - 14.0, Some((track, fraction)))
        }
        None => (rect.right() - fix_width, None),
    };
    let sentence = rux.elided(kit::help_style(), &message, theme.ink.i500, sentence_right - left);
    rux.painter().galley(Pos2::new(left, sentence_top), sentence, theme.ink.i500);
    if let Some((track, fraction)) = track {
        Track::new(fraction, 1.0).show(rux, track);
    }
    if let Some(label) = fix {
        let width = kit::button_width(rux, label, false);
        let button = Rect::from_min_size(
            Pos2::new(rect.right() - WORD - width, middle - kit::BUTTON / 2.0 + 6.0),
            Vec2::new(width, kit::BUTTON - 8.0),
        );
        if kit::button(rux, button, label, kit::Kind::Chip) {
            outcome.fix = Some(row.name.clone());
        }
    }
    // Named as a whole, so a test and a screen reader can find the step and hear its state.
    let named = format!("{}: {}", row.title, row.word());
    let response = rux.ui.interact(
        rect,
        rux.ui.id().with(("setup-step", row.name.as_str())),
        egui::Sense::hover(),
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, named.clone()));
}

/// `words` with the backticks a command line sentence marks code with taken out, since the dialog draws
/// them as plain words.
fn plain(words: &str) -> String {
    words.replace('`', "")
}

/// The well's state, or `None` for the dash a step with nothing to report is drawn with.
fn status_of(row: &Row) -> Option<Status> {
    if row.running {
        return Some(Status::Running);
    }
    if row.queued {
        return Some(Status::Waiting);
    }
    match row.state {
        State::Done | State::Already => Some(Status::Done),
        State::Failed => Some(Status::Failed),
        State::Left => Some(Status::NeedsOk),
        _ => None,
    }
}

/// The button a failed row offers.
fn fix_label(row: &Row) -> Option<&'static str> {
    if row.state != State::Failed {
        return None;
    }
    Some(match row.name.as_str() {
        "microphone" if row.message.contains("Settings") || row.message.contains("switch") => {
            "Open Settings"
        }
        "path" if row.message.contains("temporary copy") => "Show in Finder",
        _ => "Try Again",
    })
}

/// `Don't show this again` at the left of the foot, and `Not Now` and `Run Setup` at the right.
fn footer(
    ui: &mut egui::Ui,
    inner: Rect,
    foot: f32,
    plan: &Plan,
    showing: &Showing,
    dont_show: &mut bool,
    outcome: &mut Outcome,
) {
    let footer = Rect::from_min_max(Pos2::new(inner.left(), foot), inner.max);
    let anything =
        plan.rows.iter().any(|row| row.ticked && row.can_be_ticked() && row.needs_doing());
    // A window waiting for a project opens the folder chosen, so with nothing left to run its button says so, and
    // it waits for a folder to be chosen.
    let chosen = showing.project.as_ref().map(Option::is_some);
    let primary = match (showing.running, anything, chosen) {
        (true, _, _) => ("Stop", true),
        (false, true, _) => ("Run Setup", showing.can_run && !showing.looking),
        (false, false, Some(chosen)) => ("Open Project", chosen),
        (false, false, None) => ("Done", true),
    };
    let pressed = kit::layer(ui, "setup-footer", footer, |rux| {
        let theme = rux.theme();
        let check = Rect::from_min_size(
            Pos2::new(inner.left(), inner.bottom() - dialog::BUTTON / 2.0 - CHECK / 2.0),
            Vec2::splat(CHECK),
        );
        if round_check(rux, check, *dont_show, "Don't show this again").clicked() {
            *dont_show = !*dont_show;
        }
        let words = rux.text(kit::label_style(), "Don't show this again", theme.ink.i700);
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(check.right() + 10.0, check.center().y),
            words,
            kit::label_style(),
            theme.ink.i700,
        );
        if let Some(problem) = &showing.problem {
            let left = check.right() + 190.0;
            let said = rux.elided(
                kit::help_style(),
                problem,
                theme.chat.slower,
                inner.right() - 220.0 - left,
            );
            rux.painter().galley(Pos2::new(left, check.top() + 2.0), said, theme.chat.slower);
        }
        dialog::footer(rux, inner, &[("Not Now", true)], primary)
    });
    match pressed {
        Some(0) => outcome.close = true,
        Some(_) => match primary.0 {
            "Stop" => outcome.stop = true,
            "Run Setup" => outcome.run = true,
            "Open Project" => outcome.open_project = true,
            _ => outcome.close = true,
        },
        None => {}
    }
    if primary.1 && modal::Confirm::Enter.pressed(ui) {
        match primary.0 {
            "Run Setup" => outcome.run = true,
            "Open Project" => outcome.open_project = true,
            _ => {}
        }
    }
}
