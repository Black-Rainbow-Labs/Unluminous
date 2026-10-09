//! Jupyter notebooks in a real window: a notebook drawn with every common kind of output, the two
//! modes and their keys, saving, outputs that follow their cell through an edit and an undo, and a
//! cell run in a real kernel when the machine has a Python with ipykernel. `task-2220`.
//!
//! The pictures open `fixtures/notebooks/outputs.ipynb`, whose outputs were made by running it in a
//! real kernel and are saved in the file, so what is drawn is the same on every run and no kernel is
//! started to draw it. `_agent_output/task-2220-notebooks/make_window_fixture.py` wrote it.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::notebook::Mode;
use unluminous_app::UnluminousApp;

/// A folder holding a copy of the fixture notebook, under a name of its own so a test can change it.
fn a_notebook_folder(name: &str) -> std::path::PathBuf {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/notebooks/outputs.ipynb");
    let folder = fixture(name, &[("readme.md", "# A project with a notebook\n")]);
    std::fs::copy(&source, folder.join("outputs.ipynb")).expect("copy the notebook");
    folder
}

/// A window on a copy of the fixture notebook, with the notebook open.
fn a_notebook_window(name: &str) -> (std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = a_notebook_folder(name);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open outputs.ipynb");
    steady(&mut harness);
    (folder, harness)
}

/// How many cells the notebook that is showing has.
fn cells(harness: &Harness<'static, UnluminousApp>) -> usize {
    harness.state().files.active().notebook.as_ref().map(|tab| tab.len()).unwrap_or(0)
}

/// The mode the notebook that is showing is in.
fn mode(harness: &Harness<'static, UnluminousApp>) -> Mode {
    harness.state().files.active().notebook.as_ref().map(|tab| tab.mode).unwrap_or_default()
}

#[test]
fn a_notebook_is_drawn_with_its_markdown_rendered_and_every_kind_of_output() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-drawn");
    assert_eq!(cells(&harness), 7);
    harness.snapshot(shot("notebook_top"));
    did(&mut harness, "editor scroll --bottom");
    steady(&mut harness);
    harness.snapshot(shot("notebook_bottom"));
}

#[test]
fn a_traceback_opens_under_its_error_and_the_view_reports_the_cell_state() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-traceback");
    let status = did(&mut harness, "notebook status");
    let rows = status["cells"].as_array().expect("cells");
    assert_eq!(rows[3]["state"], "error", "the cell that divided by nought failed");
    assert_eq!(rows[5]["outputs"], "1 output(s): table", "pandas' frame is read as a table");
    let cell = did(&mut harness, "notebook cell 4");
    assert!(cell["outputText"].as_str().unwrap().contains("ZeroDivisionError: division by zero"));
    did(&mut harness, "notebook select 4");
    // Cell 4's text is lines 14 to 16 of the notebook's text; its error is drawn under line 16.
    did(&mut harness, "editor scroll --line 14");
    steady(&mut harness);
    harness.get_by_label_contains("Show the traceback").click();
    steady(&mut harness);
    harness.snapshot(shot("notebook_traceback"));
}

#[test]
fn a_notebook_opens_in_command_mode_and_the_letters_act_on_cells() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-keys");
    assert!(
        matches!(mode(&harness), Mode::Command { anchor: 0, head: 0 }),
        "the first cell is chosen"
    );
    harness.key_press(egui::Key::B);
    steady(&mut harness);
    assert_eq!(cells(&harness), 8, "B adds a code cell below");
    assert_eq!(mode(&harness), Mode::Edit, "and puts the caret in it");
    harness.input_mut().events.push(egui::Event::Text("total = 1".to_owned()));
    steady(&mut harness);
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(matches!(mode(&harness), Mode::Command { .. }), "Escape leaves the cell");
    harness.key_press(egui::Key::M);
    steady(&mut harness);
    let status = did(&mut harness, "notebook status");
    assert_eq!(status["cells"][1]["kind"], "markdown", "M makes it Markdown");
    harness.key_press(egui::Key::D);
    harness.key_press(egui::Key::D);
    steady(&mut harness);
    assert_eq!(cells(&harness), 7, "D D deletes it");
    harness.key_press(egui::Key::Z);
    steady(&mut harness);
    assert_eq!(cells(&harness), 8, "Z brings it back");
    assert!(harness.state().document().text().to_string().contains("total = 1"));
}

#[test]
fn the_keys_of_edit_mode_type_and_the_notebook_keeps_its_markers_whole() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-typing");
    did(&mut harness, "notebook select 2 --edit");
    let before = did(&mut harness, "notebook status")["cells"].as_array().unwrap().len();
    // Backspace at the first character of a cell would join it to its marker; it does nothing.
    harness.key_press(egui::Key::Backspace);
    steady(&mut harness);
    assert_eq!(did(&mut harness, "notebook status")["cells"].as_array().unwrap().len(), before);
    harness.key_press_modifiers(Modifiers::SHIFT | Modifiers::ALT, egui::Key::B);
    steady(&mut harness);
    assert_eq!(cells(&harness), before + 1, "Alt+Shift+B adds a cell below in edit mode");
}

#[test]
fn a_notebook_opened_and_saved_unchanged_is_written_back_byte_for_byte() {
    let (folder, mut harness) = a_notebook_window("unluminous-notebook-save");
    let before = std::fs::read(folder.join("outputs.ipynb")).expect("read it");
    did(&mut harness, "tab save-as copy.ipynb");
    let after = std::fs::read(folder.join("copy.ipynb")).expect("read the copy");
    assert_eq!(after, before, "the notebook came back as it went in");
}

#[test]
fn a_cell_deleted_and_brought_back_with_undo_keeps_its_outputs() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-undo");
    did(&mut harness, "notebook edit delete --cell 2");
    assert_eq!(cells(&harness), 6);
    did(&mut harness, "editor undo");
    steady(&mut harness);
    let cell = did(&mut harness, "notebook cell 2");
    assert_eq!(cell["executionCount"], 1);
    assert!(cell["outputText"].as_str().unwrap().contains("quarters: 4"), "{cell}");
}

#[test]
fn an_edited_cell_is_saved_with_its_outputs_and_the_file_is_still_a_notebook() {
    let (folder, mut harness) = a_notebook_window("unluminous-notebook-edit-save");
    did(&mut harness, "notebook source --cell 2 totals = [1, 2]");
    did(&mut harness, "notebook add --at 8 print(42)");
    did(&mut harness, "tab save");
    let written = std::fs::read_to_string(folder.join("outputs.ipynb")).expect("read it");
    let json: serde_json::Value = serde_json::from_str(&written).expect("still JSON");
    let cells = json["cells"].as_array().expect("cells");
    assert_eq!(cells.len(), 8);
    assert_eq!(cells[1]["source"], serde_json::json!(["totals = [1, 2]"]));
    assert_eq!(
        cells[1]["outputs"][0]["text"],
        serde_json::json!(["quarters: 4\n"]),
        "the outputs stay until it runs again"
    );
    assert_eq!(cells[7]["source"], serde_json::json!(["print(42)"]));
}

/// A Python with ipykernel, when this machine has one. `UNLUMINOUS_TEST_PYTHON` names one directly.
fn a_python_with_ipykernel() -> Option<std::path::PathBuf> {
    if let Ok(named) = std::env::var("UNLUMINOUS_TEST_PYTHON") {
        return Some(std::path::PathBuf::from(named));
    }
    unluminous_jupyter::kernel::find_pythons(None)
        .into_iter()
        .find(|python| python.has_ipykernel)
        .map(|python| python.path)
}

#[test]
fn a_cell_runs_in_a_real_kernel_and_its_variables_and_input_are_read_back() {
    let Some(python) = a_python_with_ipykernel() else {
        println!("No Python with ipykernel on this machine, so no kernel was started.");
        return;
    };
    let (_, mut harness) = a_notebook_window("unluminous-notebook-kernel");
    did(&mut harness, &format!("notebook kernel choose --python {}", python.display()));
    did(&mut harness, "notebook add --at 8 name = input()\\nprint(name * 2)");
    did(&mut harness, "notebook run 2");
    let ran = wait_for(&mut harness, |harness| {
        did(harness, "notebook status")["cells"][1]["state"] == "ok"
    });
    assert!(ran, "cell 2 ran: {}", did(&mut harness, "notebook status"));
    let cell = did(&mut harness, "notebook cell 2");
    assert!(cell["outputText"].as_str().unwrap().contains("quarters: 4"), "{cell}");
    assert!(cell["outputText"].as_str().unwrap().contains("19"), "the sum is the result: {cell}");
    did(&mut harness, "notebook run 8");
    let asked = wait_for(&mut harness, |harness| {
        !did(harness, "notebook status")["waitingForInput"].is_null()
    });
    assert!(asked, "the cell asked for input");
    did(&mut harness, "notebook input Ada");
    let answered = wait_for(&mut harness, |harness| {
        did(harness, "notebook cell 8")["outputText"]
            .as_str()
            .is_some_and(|text| text.contains("AdaAda"))
    });
    assert!(answered, "{}", did(&mut harness, "notebook cell 8"));
    did(&mut harness, "notebook kernel shut-down");
}

/// Pump the window until `done` says so, for up to a minute: a kernel starts in a second or two
/// alone and in tens of seconds on a loaded machine.
fn wait_for(
    harness: &mut Harness<'static, UnluminousApp>,
    mut done: impl FnMut(&mut Harness<'static, UnluminousApp>) -> bool,
) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < until {
        pump(harness);
        if done(harness) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}
