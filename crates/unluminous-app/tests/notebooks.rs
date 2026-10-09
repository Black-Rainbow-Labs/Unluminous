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

/// A Python with ipykernel and jupyter_client, which the bridge needs, when this machine has one.
/// `UNLUMINOUS_TEST_PYTHON` names one directly.
fn a_python_with_ipykernel() -> Option<std::path::PathBuf> {
    if let Ok(named) = std::env::var("UNLUMINOUS_TEST_PYTHON") {
        return Some(std::path::PathBuf::from(named));
    }
    unluminous_jupyter::kernel::find_pythons(None)
        .into_iter()
        .find(|python| python.has_ipykernel && python.has_jupyter_client)
        .map(|python| python.path)
}

#[test]
fn a_cell_runs_in_a_real_kernel_and_its_variables_and_input_are_read_back() {
    let Some(python) = kernel_python() else { return };
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

/// Pump the window until `done` says so, for up to two minutes: a kernel starts in a second or two
/// alone and took ninety seconds on a machine busy building.
fn wait_for(
    harness: &mut Harness<'static, UnluminousApp>,
    mut done: impl FnMut(&mut Harness<'static, UnluminousApp>) -> bool,
) -> bool {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while std::time::Instant::now() < until {
        pump(harness);
        if done(harness) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

// ---------------------------------------------------------------------------------------------
// The review's findings, each driven in a window (`task-2227`).

/// A notebook file holding `cells`, each a kind and a source, as Jupyter writes one.
fn notebook_json(cells: &[(&str, &str)]) -> String {
    let cells: Vec<serde_json::Value> = cells
        .iter()
        .enumerate()
        .map(|(at, (kind, source))| {
            let mut cell = serde_json::json!({
                "cell_type": kind,
                "id": format!("cell{at}"),
                "metadata": {},
                "source": source,
            });
            if *kind == "code" {
                cell["execution_count"] = serde_json::Value::Null;
                cell["outputs"] = serde_json::json!([]);
            }
            cell
        })
        .collect();
    let notebook = serde_json::json!({
        "cells": cells,
        "metadata": {"kernelspec": {"display_name": "Python 3", "language": "python", "name": "python3"}},
        "nbformat": 4,
        "nbformat_minor": 5,
    });
    serde_json::to_string_pretty(&notebook).expect("a notebook is JSON") + "\n"
}

/// A window on a notebook of `cells`, in a folder of its own called `name`, with the notebook open.
fn a_window_on(
    name: &str,
    cells: &[(&str, &str)],
) -> (std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = fixture(name, &[("book.ipynb", &notebook_json(cells))]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open book.ipynb");
    steady(&mut harness);
    (folder, harness)
}

#[test]
fn a_percent_line_inside_a_cell_stays_in_it_and_closing_the_tab_writes_nothing() {
    let source = "a = 1\n# %% a comment\n#%%\nb = 2";
    let (folder, mut harness) =
        a_window_on("unluminous-notebook-percent-line", &[("code", source), ("markdown", "# End")]);
    let before = std::fs::read(folder.join("book.ipynb")).expect("read it");
    let status = did(&mut harness, "notebook status");
    assert_eq!(status["cells"].as_array().unwrap().len(), 2, "{status}");
    assert_eq!(status["unsaved"], false, "opening it changed nothing");
    let cell = did(&mut harness, "notebook cell 1");
    assert_eq!(cell["source"], source, "{cell}");
    did(&mut harness, "tab close");
    let after = std::fs::read(folder.join("book.ipynb")).expect("read it again");
    assert_eq!(after, before, "closing the tab did not write the file");
}

#[test]
fn a_notebook_with_windows_line_breaks_is_saved_back_byte_for_byte() {
    let folder = a_notebook_folder("unluminous-notebook-crlf");
    let written = std::fs::read_to_string(folder.join("outputs.ipynb")).expect("read it");
    let crlf = written.replace("\r\n", "\n").replace('\n', "\r\n");
    std::fs::write(folder.join("outputs.ipynb"), &crlf).expect("write the CRLF copy");
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open outputs.ipynb");
    steady(&mut harness);
    did(&mut harness, "tab save-as copy.ipynb");
    let copy = std::fs::read(folder.join("copy.ipynb")).expect("read the copy");
    assert_eq!(copy, crlf.into_bytes(), "the line breaks came back as they went in");
}

#[test]
fn undo_after_changing_a_cells_kind_brings_its_outputs_back() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-kind-undo");
    did(&mut harness, "notebook edit kind --cell 2 --kind markdown");
    assert_eq!(did(&mut harness, "notebook cell 2")["executionCount"], serde_json::Value::Null);
    did(&mut harness, "editor undo");
    steady(&mut harness);
    let cell = did(&mut harness, "notebook cell 2");
    assert_eq!(cell["kind"], "code");
    assert_eq!(cell["executionCount"], 1, "{cell}");
    assert!(cell["outputText"].as_str().unwrap().contains("quarters: 4"), "{cell}");
}

#[test]
fn outputs_go_with_their_cell_when_it_is_moved_merged_or_split() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-move-merge-split");
    did(&mut harness, "notebook edit move --cell 2 --to 5");
    let moved = did(&mut harness, "notebook cell 5");
    assert!(moved["outputText"].as_str().unwrap().contains("quarters: 4"), "{moved}");
    assert!(did(&mut harness, "notebook cell 2")["source"]
        .as_str()
        .unwrap()
        .contains("sys.stderr"));
    // Cells 3 and 4 are now the error and the plot: merged, the first one's outputs are kept.
    did(&mut harness, "notebook edit merge --cell 3 --to 4");
    let merged = did(&mut harness, "notebook cell 3");
    assert!(merged["source"].as_str().unwrap().contains("matplotlib"), "{merged}");
    assert!(merged["outputText"].as_str().unwrap().contains("ZeroDivisionError"), "{merged}");
    assert_eq!(cells(&harness), 6);
    // Split before its third line: the first half keeps the outputs and the id.
    let id = merged["id"].clone();
    did(&mut harness, "notebook edit split --cell 3 --to 3");
    assert_eq!(cells(&harness), 7);
    let first = did(&mut harness, "notebook cell 3");
    assert_eq!(first["id"], id);
    assert!(first["outputText"].as_str().unwrap().contains("ZeroDivisionError"), "{first}");
    assert_eq!(did(&mut harness, "notebook cell 4")["outputText"], "");
}

#[test]
fn a_collapsed_section_hides_its_cells_until_it_is_opened_again() {
    let (_, mut harness) = a_window_on(
        "unluminous-notebook-section",
        &[
            ("markdown", "# One"),
            ("code", "1"),
            ("code", "2"),
            ("markdown", "# Two"),
            ("code", "3"),
        ],
    );
    did(&mut harness, "notebook view collapse-section --cell 2");
    steady(&mut harness);
    let status = did(&mut harness, "notebook status");
    assert_eq!(status["chosen"], serde_json::json!([1, 1]), "the heading is chosen: {status}");
    let hidden: Vec<bool> = status["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cell| cell["hidden"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(hidden, vec![false, true, true, false, false], "{status}");
    assert_eq!(status["cells"][0]["sectionCollapsed"], true);
    did(&mut harness, "notebook view collapse-section --cell 1");
    steady(&mut harness);
    let status = did(&mut harness, "notebook status");
    assert!(
        status["cells"].as_array().unwrap().iter().all(|cell| cell["hidden"] == false),
        "{status}"
    );
}

#[test]
fn tags_are_shown_saved_and_read_back() {
    let (folder, mut harness) =
        a_window_on("unluminous-notebook-tags", &[("code", "x = 1"), ("code", "x")]);
    did(&mut harness, "notebook edit tags --cell 1 --tags parameters,slow");
    did(&mut harness, "tab save");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("book.ipynb")).expect("read it"))
            .expect("JSON");
    assert_eq!(written["cells"][0]["metadata"]["tags"], serde_json::json!(["parameters", "slow"]));
    assert_eq!(
        did(&mut harness, "notebook status")["cells"][0]["tags"],
        serde_json::json!(["parameters", "slow"])
    );
}

// ---------------------------------------------------------------------------------------------
// The review's findings that need a real kernel.

/// The Python for a test that starts a kernel, or `None` to skip it. With
/// `UNLUMINOUS_REQUIRE_KERNEL=1` a machine with none fails the test instead, so a run that has to
/// prove the kernel code cannot pass by skipping it.
fn kernel_python() -> Option<std::path::PathBuf> {
    let found = a_python_with_ipykernel();
    if found.is_none() && std::env::var("UNLUMINOUS_REQUIRE_KERNEL").is_ok_and(|value| value == "1")
    {
        panic!("UNLUMINOUS_REQUIRE_KERNEL is 1 and no Python with ipykernel and jupyter_client was found");
    }
    if found.is_none() {
        println!("No Python with ipykernel on this machine, so no kernel was started.");
    }
    found
}

/// A window on a notebook of `cells`, with its kernel's Python chosen, or `None` when there is none.
fn a_kernel_window_on(
    name: &str,
    cells: &[(&str, &str)],
) -> Option<(std::path::PathBuf, Harness<'static, UnluminousApp>)> {
    let python = kernel_python()?;
    let (folder, mut harness) = a_window_on(name, cells);
    did(&mut harness, &format!("notebook kernel choose --python {}", python.display()));
    Some((folder, harness))
}

/// The run state of cell `number`, counting from 1, as `notebook status` says it.
fn state_of(harness: &mut Harness<'static, UnluminousApp>, number: usize) -> String {
    let status = did(harness, "notebook status");
    status["cells"][number - 1]["state"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn restart_and_run_all_runs_every_cell_again_in_a_new_kernel() {
    let Some((_, mut harness)) = a_kernel_window_on(
        "unluminous-notebook-restart-run-all",
        &[("code", "x = 20"), ("code", "x + 1")],
    ) else {
        return;
    };
    did(&mut harness, "notebook run --all");
    assert!(
        wait_for(&mut harness, |harness| state_of(harness, 2) == "ok"),
        "the first run finished"
    );
    did(&mut harness, "action run notebook-restart-run-all");
    let again = wait_for(&mut harness, |harness| {
        let status = did(harness, "notebook status");
        status["cells"][1]["state"] == "ok" && status["cells"][1]["executionCount"] == 2
    });
    let status = did(&mut harness, "notebook status");
    assert!(again, "both cells ran again, numbered from 1 in the new kernel: {status}");
    assert_eq!(status["cells"][0]["executionCount"], 1, "{status}");
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn interrupting_a_cell_skips_the_cells_queued_behind_it() {
    // It says when it has started, because an interrupt that reaches the kernel before the cell is
    // executing has nothing to stop, in Jupyter as here.
    let slow =
        "import time\nprint('started', flush=True)\nfor _ in range(1200):\n    time.sleep(0.05)";
    let Some((_, mut harness)) = a_kernel_window_on(
        "unluminous-notebook-interrupt",
        &[("code", slow), ("code", "print('after')")],
    ) else {
        return;
    };
    did(&mut harness, "notebook run --all");
    let started = wait_for(&mut harness, |harness| {
        did(harness, "notebook cell 1")["outputText"]
            .as_str()
            .is_some_and(|text| text.contains("started"))
    });
    assert!(
        started,
        "cell 1 started: {}\n{}",
        did(&mut harness, "notebook cell 1"),
        did(&mut harness, "notebook status")
    );
    did(&mut harness, "notebook kernel interrupt");
    let stopped = wait_for(&mut harness, |harness| state_of(harness, 1) == "error");
    let cell = did(&mut harness, "notebook cell 1");
    assert!(stopped, "cell 1 stopped: {cell}");
    assert!(cell["outputText"].as_str().unwrap().contains("KeyboardInterrupt"), "{cell}");
    assert_eq!(state_of(&mut harness, 2), "skipped");
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn a_kernel_killed_from_outside_fails_the_running_cell_and_says_so_in_it() {
    let slow = "import time\nfor _ in range(1200):\n    time.sleep(0.05)";
    let Some((_, mut harness)) =
        a_kernel_window_on("unluminous-notebook-killed", &[("code", slow)])
    else {
        return;
    };
    did(&mut harness, "notebook run 1");
    assert!(wait_for(&mut harness, |harness| state_of(harness, 1) == "running"), "cell 1 started");
    let pid =
        did(&mut harness, "notebook kernel status")["pid"].as_u64().expect("the kernel's pid");
    let killed = match cfg!(windows) {
        true => {
            std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output()
        }
        false => std::process::Command::new("kill").args(["-9", &pid.to_string()]).output(),
    };
    assert!(killed.is_ok_and(|output| output.status.success()), "the kernel was killed");
    let failed = wait_for(&mut harness, |harness| state_of(harness, 1) == "error");
    let cell = did(&mut harness, "notebook cell 1");
    assert!(failed, "{cell}");
    assert!(cell["outputText"].as_str().unwrap().contains("kernel died"), "{cell}");
}

#[test]
fn a_python_without_ipykernel_is_reported_with_what_is_missing() {
    let Some(python) = kernel_python() else { return };
    let bare = std::env::temp_dir().join(format!("unluminous-bare-venv-{}", std::process::id()));
    let made = std::process::Command::new(&python)
        .args(["-m", "venv", "--without-pip"])
        .arg(&bare)
        .output()
        .expect("python runs");
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let bare_python = match cfg!(windows) {
        true => bare.join("Scripts").join("python.exe"),
        false => bare.join("bin").join("python"),
    };
    let (_, mut harness) = a_window_on("unluminous-notebook-no-ipykernel", &[("code", "1 + 1")]);
    did(&mut harness, &format!("notebook kernel choose --python {}", bare_python.display()));
    did(&mut harness, "notebook run 1");
    let reported = wait_for(&mut harness, |harness| {
        did(harness, "notebook kernel status")["state"] == "failed"
    });
    let kernel = did(&mut harness, "notebook kernel status");
    assert!(reported, "{kernel}");
    // A Python with neither package lacks jupyter_client first, because the bridge imports it
    // before it starts a kernel. Installing ipykernel installs jupyter_client with it.
    let missing = kernel["missing"].as_str().unwrap_or_default().to_owned();
    assert!(missing == "ipykernel" || missing == "jupyter_client", "{kernel}");
    assert!(kernel["problem"].as_str().unwrap_or_default().contains(&missing), "{kernel}");
    std::fs::remove_dir_all(&bare).ok();
}

#[test]
fn the_first_debug_cell_on_a_kernel_stops_in_the_cell_and_stopping_leaves_the_kernel_running() {
    let Some((_, mut harness)) = a_kernel_window_on(
        "unluminous-notebook-debug",
        &[("code", "a = 5"), ("code", "b = a * 2\nprint(b)")],
    ) else {
        return;
    };
    did(&mut harness, "notebook run 1");
    assert!(wait_for(&mut harness, |harness| state_of(harness, 1) == "ok"), "cell 1 ran");
    did(&mut harness, "notebook run 2 --debug");
    // Paused, and with the stack read, which is what says where: the two arrive a moment apart.
    let paused = wait_for(&mut harness, |harness| {
        let debug = did(harness, "debug status");
        debug["paused"] == true && !debug["line"].is_null()
    });
    let debug = did(&mut harness, "debug status");
    assert!(paused, "the debugger stopped: {debug}");
    assert_eq!(debug["line"], 1, "on the cell's first line: {debug}");
    did(&mut harness, "debug stop");
    let ended = wait_for(&mut harness, |harness| {
        did(harness, "debug status")["running"] != true
            || did(harness, "debug status")["state"] == "ended"
    });
    assert!(ended, "{}", did(&mut harness, "debug status"));
    // The kernel and its variables are still there: cell 2 runs and reads `a`.
    let kernel = did(&mut harness, "notebook kernel status");
    assert!(
        kernel["state"] == "idle" || kernel["state"] == "busy",
        "the kernel is alive: {kernel}"
    );
    did(&mut harness, "notebook run 2");
    let ran = wait_for(&mut harness, |harness| {
        did(harness, "notebook cell 2")["outputText"]
            .as_str()
            .is_some_and(|text| text.contains("10"))
    });
    assert!(ran, "{}", did(&mut harness, "notebook cell 2"));
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn debug_cell_on_a_notebook_with_no_kernel_starts_one_and_stops_in_the_cell() {
    let Some((_, mut harness)) =
        a_kernel_window_on("unluminous-notebook-debug-cold", &[("code", "c = 3\nd = c + 1")])
    else {
        return;
    };
    did(&mut harness, "notebook run 1 --debug");
    // Paused, and with the stack read, which is what says where: the two arrive a moment apart.
    let paused = wait_for(&mut harness, |harness| {
        let debug = did(harness, "debug status");
        debug["paused"] == true && !debug["line"].is_null()
    });
    let debug = did(&mut harness, "debug status");
    assert!(paused, "the debugger stopped: {debug}");
    assert_eq!(debug["line"], 1, "{debug}");
    did(&mut harness, "debug stop");
    did(&mut harness, "notebook kernel shut-down");
}

// ---------------------------------------------------------------------------------------------
// Rust notebooks, completion from the kernel, and the kernel picker (`task-2229`).

/// Run a command whose answer may be held for the kernel, such as `notebook complete`, and take the
/// answer once it comes. Asked again after each pump, which is safe: the kernel is asked once for a
/// word, and asking again only reads its answer.
fn completed(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> serde_json::Value {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let ctx = harness.ctx.clone();
        if let Some(reply) = harness.state_mut().run_command_line(line, &ctx) {
            note_a_driven_line(line, reply.ok);
            assert!(reply.ok, "`{line}` was refused: {}", reply.message);
            return reply.result;
        }
        assert!(std::time::Instant::now() < until, "`{line}` was never answered");
        pump(harness);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// The names `notebook complete` or `editor complete` answered with.
fn names(answer: &serde_json::Value) -> Vec<String> {
    answer["rows"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row["name"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn a_rust_notebook_is_read_as_rust_and_offers_no_debugger() {
    let folder = fixture("unluminous-notebook-rust-reading", &[("readme.md", "# Rust\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "notebook new scratch.ipynb --language rust");
    let written: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(folder.join("scratch.ipynb")).expect("the notebook was written"),
    )
    .expect("it is JSON");
    assert_eq!(written["metadata"]["kernelspec"]["name"], "rust");
    did(&mut harness, "notebook source --cell 1 let total = 1;");
    // The language's own words are Rust's, not Python's.
    let offered = names(&did(&mut harness, "editor complete --stem imp --limit 0 --json"));
    assert!(offered.contains(&"impl".to_owned()), "{offered:?}");
    assert!(!offered.contains(&"import".to_owned()), "{offered:?}");
    // A line comment is Rust's.
    did(&mut harness, "notebook select 1 --edit");
    did(&mut harness, "editor comment");
    let cell = did(&mut harness, "notebook cell 1");
    assert!(cell["source"].as_str().unwrap_or_default().starts_with("//"), "{cell}");
    // Debugging a cell is debugpy inside ipykernel, so a Rust notebook refuses it.
    assert_eq!(refused(&mut harness, "notebook run 1 --debug"), "not-applicable");
}

/// A Python whose Jupyter has evcxr's kernel registered, or `None` with a line saying so.
fn rust_kernel_python() -> Option<std::path::PathBuf> {
    let python = kernel_python()?;
    let specs = unluminous_jupyter::kernel::list_kernelspecs(&python).unwrap_or_default();
    if !specs.iter().any(|spec| spec.name == "rust") {
        println!("No Rust kernel (evcxr) is registered with Jupyter, so no Rust kernel was started.");
        return None;
    }
    Some(python)
}

#[test]
fn a_rust_notebook_keeps_its_variables_between_cells_and_completes_after_a_dot() {
    let Some(python) = rust_kernel_python() else { return };
    let folder = fixture("unluminous-notebook-rust-kernel", &[("readme.md", "# Rust\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "notebook new rusty.ipynb --language rust");
    did(&mut harness, &format!("notebook kernel choose --python {}", python.display()));
    did(&mut harness, "notebook source --cell 1 let v = vec![1, 2, 3];");
    did(&mut harness, "notebook add --at 2 v.len() * 10");
    did(&mut harness, "notebook run --all");
    let ran = wait_for(&mut harness, |harness| state_of(harness, 2) == "ok");
    assert!(ran, "both cells ran: {}", did(&mut harness, "notebook status"));
    let cell = did(&mut harness, "notebook cell 2");
    assert!(cell["outputText"].as_str().unwrap_or_default().contains("30"), "{cell}");
    // The variables, as evcxr's `:vars` lists them. The answer is held for the kernel, so what is
    // read is the list the Variables panel draws once it has arrived.
    let ctx = harness.ctx.clone();
    harness.state_mut().run_command_line("notebook variables", &ctx);
    note_a_driven_line("notebook variables", true);
    let listed = wait_for(&mut harness, |harness| {
        let tab = harness.state().files.active().notebook.as_deref().expect("a notebook");
        tab.variables.iter().any(|row| row.name == "v" && row.type_name == "Vec<i32>")
    });
    assert!(listed, "v is listed as a Vec<i32>");
    // After a dot, the kernel says what the value has.
    did(&mut harness, "notebook add --at 3 v.it");
    let answer = completed(&mut harness, "notebook complete --cell 3 --json");
    let offered = names(&answer);
    assert!(offered.contains(&"iter".to_owned()), "{offered:?}");
    let iter = answer["rows"].as_array().unwrap().iter().find(|row| row["name"] == "iter").unwrap();
    assert_eq!(iter["source"], "kernel", "{iter}");
    completed(&mut harness, "notebook complete --cell 3 --choose iter");
    let cell = did(&mut harness, "notebook cell 3");
    assert_eq!(cell["source"], "v.iter", "the arguments are not inserted: {cell}");
    // A new word at the same byte is asked about again rather than given the old answer: `v.` after
    // `v.it` offers every method, not the two that matched `it`.
    did(&mut harness, "notebook source --cell 3 v.");
    let offered = names(&completed(&mut harness, "notebook complete --cell 3 --limit 0 --json"));
    assert!(offered.len() > 2 && offered.contains(&"len".to_owned()), "{offered:?}");
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn a_dot_typed_in_a_python_cell_opens_the_kernels_completions() {
    let Some((_, mut harness)) = a_kernel_window_on(
        "unluminous-notebook-dot-completion",
        &[("code", "import os"), ("code", "os")],
    ) else {
        return;
    };
    did(&mut harness, "notebook run 1");
    assert!(wait_for(&mut harness, |harness| state_of(harness, 1) == "ok"), "cell 1 ran");
    did(&mut harness, "notebook select 2 --edit");
    let end = {
        let tab = harness.state().files.active().notebook.as_deref().expect("a notebook");
        tab.spans[1].body_bytes.end
    };
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset: end, extend: false });
    harness.input_mut().events.push(egui::Event::Text(".".to_owned()));
    let opened = wait_for(&mut harness, |harness| {
        harness
            .state()
            .completion()
            .is_some_and(|state| state.rows.iter().any(|row| row.name == "path"))
    });
    assert!(opened, "the popup opened with os.path in it: {:?}", harness.state().completion());
    harness.key_press(egui::Key::Escape);
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn a_magic_completed_from_the_kernel_is_not_given_a_second_percent() {
    let Some((_, mut harness)) =
        a_kernel_window_on("unluminous-notebook-magic-completion", &[("code", "%timei")])
    else {
        return;
    };
    did(&mut harness, "notebook kernel start");
    let started = wait_for(&mut harness, |harness| {
        did(harness, "notebook kernel status")["state"] == "idle"
    });
    assert!(started, "{}", did(&mut harness, "notebook kernel status"));
    let offered = names(&completed(&mut harness, "notebook complete --cell 1 --json"));
    assert!(offered.contains(&"timeit".to_owned()), "{offered:?}");
    completed(&mut harness, "notebook complete --cell 1 --choose timeit");
    assert_eq!(did(&mut harness, "notebook cell 1")["source"], "%timeit");
    did(&mut harness, "notebook kernel shut-down");
}

#[test]
fn the_kernel_picker_keeps_its_words_inside_itself() {
    let (_, mut harness) = a_notebook_window("unluminous-notebook-picker");
    // A window narrow enough that `No kernel yet · starts on the first run` cannot fit in the
    // button, which is the report: the words ran on over the Variables button beside it. They are
    // cut short with an ellipsis now, and the picture shows it. The list itself is not opened here,
    // because which Pythons it lists depends on the machine.
    harness.set_size(egui::vec2(900.0, 640.0));
    steady(&mut harness);
    let kernel = harness.get_by_label("Kernel").rect();
    let variables = harness.get_by_label("Variables").rect();
    assert!(kernel.max.x <= variables.min.x, "{kernel:?} {variables:?}");
    assert!(kernel.width() < 260.0, "the button is too narrow for its words: {kernel:?}");
    harness.snapshot(shot("notebook_kernel_picker"));
}
