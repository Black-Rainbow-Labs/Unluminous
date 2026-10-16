//! The `Run Configurations` modal: the list on the left, the fields on the right. `Directory` has a
//! folder button that opens the platform's picker, and `Environment` is a table of key and value rows.
//!
//! `tasks/task-1683-run-configurations-tdd.md` §7. Built from `components::modal` — the frame, the
//! header, the body rectangle, the footer and the buttons every other modal is built from — so
//! dragging it, resizing it and answering it with Enter all come for free. A tenth modal that drew
//! its own header would be a tenth modal that almost agreed with the other nine.
//!
//! Two columns, the Settings window's own shape: the configurations down the left with `Add` and
//! `Remove` under them, and the chosen one's four fields on the right. A **temporary** shows the
//! same fields plus a `Save` button that promotes it, because that is the one thing a temporary can
//! be that a permanent cannot.
//!
//! Every field has a plain name — `Run configuration name`, `Run configuration command` — because
//! the screenshot tests find controls by name rather than by position, and a control with no name
//! cannot be tested at all.
//!
//! **Nothing here changes what is on disk.** The fields are edited in place on the model the window
//! holds, and `UnluminousApp` writes `.unluminous/run-configurations.conf` when the dialog closes — by the
//! released binary only, which is the rule the project state already keeps.

use egui::{Pos2, Rect, Vec2};

use crate::components::modal;
use crate::services::run_configurations::{self, Configuration, Origin, RunConfigurations};
use crate::theme::color;

/// How large the modal is before it is shrunk to fit a small Unluminous window.
///
/// Wide enough for a command line to be read without scrolling — which is the field people will
/// actually look at — and tall enough for a list of a dozen configurations beside four fields.
const WIDTH: f32 = 720.0;
const HEIGHT: f32 = 500.0;
/// How far in the fields start, which is how wide the widest label is.
const LABEL_WIDTH: f32 = 104.0;
/// How wide the delete button at the end of an environment row is, with its gap.
const DELETE_WIDTH: f32 = 32.0;
/// What the folder button beside the directory field is called, which is what a test and an agent ask for.
pub const CHOOSE_DIRECTORY: &str = "Choose directory";
/// What the button under the environment table is called.
pub const ADD_VARIABLE: &str = "Add variable";
/// How wide the list of configurations is.
const LIST_WIDTH: f32 = 220.0;
/// How tall the strip of `Add` and `Remove` under the list is.
const LIST_FOOTER: f32 = 34.0;

/// What the dialog is showing: whether it is open, and which configuration is chosen.
///
/// The chosen one is held **by name** rather than by index, because adding, removing and promoting
/// all move the indices under it and a stale index is a dialog editing the wrong row. A name that
/// nothing holds any more simply chooses nothing, which is what the list already draws.
#[derive(Debug, Clone, Default)]
pub struct RunDialog {
    pub open: bool,
    pub chosen: Option<String>,
    /// The environment table's rows for `env_for`, kept while they are edited so that a row with no
    /// key yet is not lost between frames. Only rows with a key reach the configuration.
    pub env_rows: Vec<(String, String)>,
    /// Which configuration `env_rows` belongs to.
    pub env_for: Option<String>,
    /// The row whose key field takes focus on the next frame, which is the one `Add variable` made.
    pub focus_row: Option<usize>,
}

impl RunDialog {
    /// Open it, choosing `name` — the widget's own selection, so the dialog opens on what the
    /// person was looking at.
    pub fn open(&mut self, name: Option<String>) {
        self.open = true;
        if name.is_some() {
            self.chosen = name;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

/// What the dialog asked the window to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DialogOutcome {
    /// The modal was closed, so what was edited is written down.
    pub closed: bool,
    /// A configuration was edited, added, removed or promoted, so the file is written.
    pub changed: bool,
    /// `Remove` was pressed on a configuration whose run is still going: the window asks the
    /// question first, because silently killing a server somebody is watching is worse than one
    /// extra click.
    pub confirm_removal: Option<String>,
    /// `Remove` was pressed on one that is not running, which needs no question.
    pub remove: Option<String>,
    /// The folder button beside `Directory` was pressed, which is what opens the platform's folder
    /// picker. The window answers it, because the picker is the window's to open.
    pub browse_directory: bool,
}

/// Draw the dialog, editing `configurations` in place.
///
/// `running` says which configurations have a run going, which is the one thing the dialog cannot
/// work out for itself and is what decides whether removing one asks a question first.
pub fn show(
    ctx: &egui::Context,
    dialog: &mut RunDialog,
    configurations: &mut RunConfigurations,
    running: &[String],
) -> DialogOutcome {
    let mut outcome = DialogOutcome::default();
    if !dialog.open {
        return outcome;
    }
    let (inner, should_close) =
        modal::show(ctx, "unluminous-run-configurations", WIDTH, HEIGHT, |ui, area| {
            contents(ui, area, dialog, configurations, running)
        });
    outcome = inner;
    if outcome.closed || should_close {
        dialog.close();
        outcome.closed = true;
    }
    outcome
}

fn contents(
    ui: &mut egui::Ui,
    area: Rect,
    dialog: &mut RunDialog,
    configurations: &mut RunConfigurations,
    running: &[String],
) -> DialogOutcome {
    let mut outcome = DialogOutcome::default();
    if modal::header(ui, area, "Run Configurations") {
        outcome.closed = true;
    }
    let body = modal::body(area);

    // A name that nothing holds any more chooses nothing, and with nothing chosen the first
    // configuration is, so opening the dialog always lands on something to edit.
    if dialog.chosen.as_deref().and_then(|name| configurations.find(name)).is_none() {
        dialog.chosen =
            configurations.listed().first().map(|(_, configuration)| configuration.name.clone());
    }

    let list = Rect::from_min_max(
        body.min,
        Pos2::new(body.left() + LIST_WIDTH, body.bottom() - LIST_FOOTER),
    );
    show_list(ui, list, dialog, configurations, running);
    let strip = Rect::from_min_max(
        Pos2::new(list.left(), list.bottom()),
        Pos2::new(list.right(), body.bottom()),
    );
    show_list_buttons(ui, strip, dialog, configurations, running, &mut outcome);

    let fields = Rect::from_min_max(Pos2::new(list.right() + 20.0, body.top()), body.max);
    show_fields(ui, fields, dialog, configurations, &mut outcome);

    // One button, and it says `Done`: every change takes effect as it is made, which is the choice
    // the Settings window already made and for the same reason — a dialog with `Apply` has to hold
    // a second copy of everything and decide what to do when the two disagree.
    if modal::footer(ui, area, &[("Done", true)]).is_some() {
        outcome.closed = true;
    }
    outcome
}

/// The list down the left.
fn show_list(
    ui: &mut egui::Ui,
    area: Rect,
    dialog: &mut RunDialog,
    configurations: &RunConfigurations,
    running: &[String],
) {
    modal::well(ui, area);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink(6.0)));
    child.set_clip_rect(area);
    let mut chosen: Option<String> = None;
    egui::ScrollArea::vertical().id_salt("run-configuration-list").show(&mut child, |ui| {
        for (origin, configuration) in configurations.listed() {
            let name = configuration.name.clone();
            let is_chosen = dialog.chosen.as_deref() == Some(name.as_str());
            let is_running = running.contains(&name);
            // A temporary is drawn in the quiet colour, exactly as it is in the widget's flyout:
            // it is something that was run rather than something somebody kept.
            let tint = match origin {
                Origin::Permanent => color::text_control(),
                Origin::Temporary | Origin::Suggested => color::text_dim(),
            };
            let label = name.clone();
            let response = modal::row(ui, &name, &name, is_chosen, move |painter, row| {
                let mut left = row.left() + 12.0;
                if is_running {
                    crate::theme::icon::state_dot(
                        painter,
                        Pos2::new(left + 4.0, row.center().y),
                        color::git_added(),
                    );
                }
                left += 14.0;
                modal::label(painter, row, left, &label, tint, 12.5);
            });
            if response.clicked() {
                chosen = Some(name);
            }
        }
        if configurations.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("  Nothing yet. Press Add.")
                    .size(11.5)
                    .color(color::text_faint()),
            );
        }
    });
    if let Some(name) = chosen {
        dialog.chosen = Some(name);
    }
}

/// `Add` and `Remove`, under the list.
fn show_list_buttons(
    ui: &mut egui::Ui,
    area: Rect,
    dialog: &mut RunDialog,
    configurations: &mut RunConfigurations,
    running: &[String],
    outcome: &mut DialogOutcome,
) {
    let width = (area.width() - 8.0) / 2.0;
    let add = Rect::from_min_size(Pos2::new(area.left(), area.top() + 4.0), Vec2::new(width, 26.0));
    if modal::button(ui, add, "Add", true, false) {
        let name = configurations.unused_name("Unnamed");
        configurations.add_permanent(Configuration::new(&name, "cargo run"));
        dialog.chosen = Some(name);
        outcome.changed = true;
    }
    let remove =
        Rect::from_min_size(Pos2::new(add.right() + 8.0, area.top() + 4.0), Vec2::new(width, 26.0));
    let chosen = dialog.chosen.clone();
    if modal::button(ui, remove, "Remove", chosen.is_some(), false) {
        if let Some(name) = chosen {
            // Removing one whose run is still going asks the question first, with the same
            // confirmation the git dialogs use: silently killing a server somebody is watching is
            // worse than one extra click.
            if running.contains(&name) {
                outcome.confirm_removal = Some(name);
            } else {
                outcome.remove = Some(name);
            }
        }
    }
}

/// The fields on the right, and the `Save` a temporary has.
///
/// The column scrolls, because the environment table grows by a row for every variable and a
/// dialog that clipped its own `Add variable` button would be one nobody could finish using.
fn show_fields(
    ui: &mut egui::Ui,
    area: Rect,
    dialog: &mut RunDialog,
    configurations: &mut RunConfigurations,
    outcome: &mut DialogOutcome,
) {
    let Some(name) = dialog.chosen.clone() else {
        modal::note(
            ui,
            area,
            area.top() + 4.0,
            "Choose a configuration, or press Add to make one.",
        );
        return;
    };
    let origin = configurations.find(&name).map(|(origin, _)| origin);
    let Some(configuration) = configurations.find_mut(&name) else {
        return;
    };
    sync_environment_rows(dialog, &name, configuration);
    let mut save = false;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    child.set_clip_rect(area);
    egui::ScrollArea::vertical().id_salt("run-configuration-fields").auto_shrink(false).show(
        &mut child,
        |ui| {
            let width = (area.width() - 14.0).max(200.0);
            let column = Rect::from_min_size(ui.cursor().min, Vec2::new(width, area.height()));
            let mut end = draw_fields(ui, column, dialog, configuration, outcome);
            if origin == Some(Origin::Temporary) {
                save = draw_save_button(ui, column, end);
                end += 40.0;
            }
            ui.allocate_space(Vec2::new(width, end - column.top() + 8.0));
        },
    );
    if save {
        configurations.promote(&name);
        outcome.changed = true;
    }
}

/// Make the dialog's table rows match the configuration's `env` text.
///
/// The rows are rebuilt when a different configuration is chosen, and when the text was changed by
/// something other than this table (`run set` from the command line). Rows with an empty key are
/// kept while they are being edited, because dropping one the moment it is added would make
/// `Add variable` appear to do nothing; they are left out of the stored text.
fn sync_environment_rows(dialog: &mut RunDialog, name: &str, configuration: &Configuration) {
    let held = run_configurations::format_env(&rows_with_a_key(&dialog.env_rows));
    let stored = run_configurations::format_env(&run_configurations::parse_env(&configuration.env));
    if dialog.env_for.as_deref() != Some(name) || held != stored {
        dialog.env_rows = run_configurations::parse_env(&configuration.env);
        dialog.env_for = Some(name.to_owned());
        dialog.focus_row = None;
    }
}

/// The rows that have a key, which are the ones that are stored.
fn rows_with_a_key(rows: &[(String, String)]) -> Vec<(String, String)> {
    rows.iter().filter(|(key, _)| !key.trim().is_empty()).cloned().collect()
}

/// Draw the three text fields, the table and the notes from the top of `column`, answering where
/// the last of them ends.
fn draw_fields(
    ui: &mut egui::Ui,
    column: Rect,
    dialog: &mut RunDialog,
    configuration: &mut Configuration,
    outcome: &mut DialogOutcome,
) -> f32 {
    let mut pen = column.top() + 4.0;
    let name = ("Name", "Run configuration name");
    pen = text_row(ui, column, pen, name, &mut configuration.name, false, outcome);
    let command = ("Command", "Run configuration command");
    pen = text_row(ui, column, pen, command, &mut configuration.command, false, outcome);
    let directory = ("Directory", "Run configuration directory");
    pen = text_row(ui, column, pen, directory, &mut configuration.directory, true, outcome);
    pen = draw_environment(ui, column, pen, dialog, configuration, outcome);
    draw_notes(ui, column, pen)
}

/// One labelled single line field, with the folder button after it when `browse` is set.
fn text_row(
    ui: &mut egui::Ui,
    column: Rect,
    pen: f32,
    names: (&str, &str),
    value: &mut String,
    browse: bool,
    outcome: &mut DialogOutcome,
) -> f32 {
    let (label, plain) = names;
    let row = Rect::from_min_size(Pos2::new(column.left(), pen), Vec2::new(column.width(), 26.0));
    modal::label(ui.painter(), row, row.left(), label, color::text_control(), 12.5);
    let after = if browse { 32.0 } else { 0.0 };
    let field = Rect::from_min_size(
        Pos2::new(row.left() + LABEL_WIDTH, row.top()),
        Vec2::new((row.width() - LABEL_WIDTH - after).max(120.0), 26.0),
    );
    if modal::field(ui, field, plain, value).changed() {
        outcome.changed = true;
    }
    if browse {
        let button =
            Rect::from_min_size(Pos2::new(field.right() + 6.0, row.top()), Vec2::splat(26.0));
        let draw = crate::theme::icon::folder;
        if crate::components::controls::icon_button(ui, button, CHOOSE_DIRECTORY, draw) {
            outcome.browse_directory = true;
        }
    }
    pen + 36.0
}

/// The `Environment` table: a key field and a value field per variable, a delete button on each
/// row, and `Add variable` under them. Every change is written back to `env` straight away.
fn draw_environment(
    ui: &mut egui::Ui,
    column: Rect,
    top: f32,
    dialog: &mut RunDialog,
    configuration: &mut Configuration,
    outcome: &mut DialogOutcome,
) -> f32 {
    let label = Rect::from_min_size(Pos2::new(column.left(), top), Vec2::new(column.width(), 22.0));
    modal::label(ui.painter(), label, label.left(), "Environment", color::text_control(), 12.5);
    let left = column.left() + LABEL_WIDTH;
    let split = left + (column.right() - left - DELETE_WIDTH) * 0.4;
    modal::label(ui.painter(), label, left + 10.0, "Key", color::text_dim(), 11.5);
    modal::label(ui.painter(), label, split + 16.0, "Value", color::text_dim(), 11.5);
    let mut pen = top + 24.0;
    let mut changed = false;
    let mut delete: Option<usize> = None;
    for index in 0..dialog.env_rows.len() {
        let line = Rect::from_min_max(Pos2::new(left, pen), Pos2::new(column.right(), pen + 26.0));
        let focus = dialog.focus_row == Some(index);
        let (edited, deleted) =
            environment_row(ui, line, index, &mut dialog.env_rows[index], focus);
        changed |= edited;
        if deleted {
            delete = Some(index);
        }
        pen += 32.0;
    }
    dialog.focus_row = None;
    if let Some(index) = delete {
        dialog.env_rows.remove(index);
        changed = true;
    }
    let add = Rect::from_min_size(Pos2::new(left, pen + 2.0), Vec2::new(130.0, 26.0));
    if modal::button(ui, add, ADD_VARIABLE, true, false) {
        dialog.env_rows.push((String::new(), String::new()));
        dialog.focus_row = Some(dialog.env_rows.len() - 1);
        changed = true;
    }
    if changed {
        configuration.env = run_configurations::format_env(&rows_with_a_key(&dialog.env_rows));
        outcome.changed = true;
    }
    pen + 40.0
}

/// One variable: its key, its value and its delete button. Answers whether either field was
/// edited, and whether delete was pressed.
fn environment_row(
    ui: &mut egui::Ui,
    line: Rect,
    index: usize,
    row: &mut (String, String),
    focus: bool,
) -> (bool, bool) {
    let number = index + 1;
    let split = line.left() + (line.width() - DELETE_WIDTH) * 0.4;
    let key = Rect::from_min_max(line.min, Pos2::new(split, line.bottom()));
    let value_min = Pos2::new(split + 6.0, line.top());
    let value =
        Rect::from_min_max(value_min, Pos2::new(line.right() - DELETE_WIDTH, line.bottom()));
    let key_response = modal::field(ui, key, &format!("Environment key {number}"), &mut row.0);
    if focus {
        key_response.request_focus();
    }
    let value_response =
        modal::field(ui, value, &format!("Environment value {number}"), &mut row.1);
    let button = Rect::from_min_size(Pos2::new(line.right() - 26.0, line.top()), Vec2::splat(26.0));
    let name = format!("Delete environment variable {number}");
    let deleted =
        crate::components::controls::icon_button(ui, button, &name, crate::theme::icon::cross);
    (key_response.changed() || value_response.changed(), deleted)
}

/// What the fields mean, said once here rather than left to be discovered. The second note is
/// the one that matters: no shell runs the command line.
fn draw_notes(ui: &mut egui::Ui, column: Rect, pen: f32) -> f32 {
    let pen = modal::note(
        ui,
        column,
        pen + 4.0,
        "The command is one line: the first word is the program and the rest are its arguments. \
         The directory is relative to the project, and empty means the project itself. A folder \
         chosen inside the project is stored relative to it. A variable with no key is dropped.",
    );
    modal::note(
        ui,
        column,
        pen,
        "No shell runs the command, so nothing is expanded and && is an argument. \
         Write pwsh -Command ... to ask for one.",
    )
}

/// `Save` and its explanation, for a configuration somebody ran without keeping. Answers whether
/// `Save` was pressed.
fn draw_save_button(ui: &mut egui::Ui, column: Rect, top: f32) -> bool {
    let save = Rect::from_min_size(Pos2::new(column.left(), top + 4.0), Vec2::new(120.0, 26.0));
    let pressed = modal::button(ui, save, "Save", true, false);
    let beside = Rect::from_min_max(Pos2::new(save.right() + 12.0, top), column.max);
    modal::note(
        ui,
        beside,
        save.top() + 5.0,
        "This one was made by running something. Save keeps it in the project.",
    );
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_it_chooses_what_the_widget_had_chosen() {
        let mut dialog = RunDialog::default();
        dialog.open(Some("Dev server".to_owned()));
        assert!(dialog.open);
        assert_eq!(dialog.chosen.as_deref(), Some("Dev server"));
        // Opening it again with nothing chosen leaves the choice where it was, so a second visit
        // lands where the first one left off.
        dialog.close();
        dialog.open(None);
        assert_eq!(dialog.chosen.as_deref(), Some("Dev server"));
    }

    #[test]
    fn the_chosen_one_is_held_by_name_so_adding_and_removing_cannot_move_it() {
        // An index would be stale the moment something above it was removed, and a stale index is
        // a dialog editing the wrong row.
        let mut configurations = RunConfigurations::new();
        configurations.add_permanent(Configuration::new("one", "cargo run"));
        configurations.add_permanent(Configuration::new("two", "cargo test"));
        let mut dialog = RunDialog::default();
        dialog.open(Some("two".to_owned()));
        configurations.remove("one");
        assert_eq!(
            configurations
                .find(dialog.chosen.as_deref().expect("a choice"))
                .map(|(_, c)| c.name.clone()),
            Some("two".to_owned())
        );
    }
}
