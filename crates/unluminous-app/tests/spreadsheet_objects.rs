//! Pictures, shapes and text boxes in a real window: clicked to choose them, dragged to move and size them,
//! drawn by dragging out a box, and nudged and deleted with the keys. `task-2360`, the `wiringc` lane.
//!
//! The pointer is driven with `input`, which feeds the window the events a mouse makes, so a drag here is the
//! drag a person makes and its result is read back through the same `objects` verb an agent uses.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

/// A window on a small workbook with the workbook open.
fn a_window(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = fixture(name, &[("prices.csv", "Item,Price\nTea,3.5\nCake,4.25\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open prices.csv");
    steady(&mut harness);
    harness
}

fn sheet(harness: &mut Harness<'static, UnluminousApp>, words: &str) -> serde_json::Value {
    let value = did(harness, &format!("plugins run spreadsheet {words}"));
    steady(harness);
    value
}

/// The objects as `objects` answers them, after the window has drawn them.
fn objects(harness: &mut Harness<'static, UnluminousApp>) -> Vec<serde_json::Value> {
    steady(harness);
    sheet(harness, "objects").as_array().cloned().unwrap_or_default()
}

/// The middle of an object in the window's points.
fn middle(object: &serde_json::Value) -> (f64, f64) {
    let s = &object["screen"];
    (
        s["x"].as_f64().unwrap() + s["width"].as_f64().unwrap() / 2.0,
        s["y"].as_f64().unwrap() + s["height"].as_f64().unwrap() / 2.0,
    )
}

#[test]
fn a_shape_is_chosen_by_a_click_moved_by_a_drag_and_deleted_with_the_key() {
    let mut harness = a_window("unluminous-sheet-objects-drag");
    sheet(&mut harness, "shape rectangle 200 120 120 60");
    sheet(&mut harness, "object select none");
    let before = objects(&mut harness);
    assert_eq!(before.len(), 1);
    let (x, y) = middle(&before[0]);
    drove(&mut harness, &format!("input click {x} {y}"));
    steady(&mut harness);
    assert_eq!(objects(&mut harness)[0]["chosen"], true, "a click chooses the shape");
    drove(&mut harness, &format!("input drag {x} {y} --to-x {} --to-y {}", x + 80.0, y + 50.0));
    steady(&mut harness);
    let after = objects(&mut harness);
    let moved = after[0]["left"].as_f64().unwrap() - before[0]["left"].as_f64().unwrap();
    assert!(moved > 20.0, "the drag moved the shape right by {moved} points");
    sheet(&mut harness, "undo");
    let undone = objects(&mut harness);
    assert!(
        (undone[0]["left"].as_f64().unwrap() - before[0]["left"].as_f64().unwrap()).abs() < 0.5,
        "the whole drag is one undo step"
    );
    harness.key_press_modifiers(Modifiers::NONE, egui::Key::Delete);
    steady(&mut harness);
    assert!(objects(&mut harness).is_empty(), "Delete removes the chosen shape");
}

#[test]
fn the_arrow_keys_nudge_a_chosen_shape() {
    let mut harness = a_window("unluminous-sheet-objects-nudge");
    sheet(&mut harness, "shape ellipse 200 120 80 80");
    let left = objects(&mut harness)[0]["left"].as_f64().unwrap();
    harness.key_press_modifiers(Modifiers::SHIFT, egui::Key::ArrowRight);
    steady(&mut harness);
    let nudged = objects(&mut harness)[0]["left"].as_f64().unwrap();
    assert!(
        (nudged - left - 7.5).abs() < 0.3,
        "a shifted arrow moves ten pixels: {left} to {nudged}"
    );
}

#[test]
fn dragging_out_a_box_draws_the_shape_that_was_asked_for() {
    let mut harness = a_window("unluminous-sheet-objects-draw");
    sheet(&mut harness, "shape draw ellipse");
    steady(&mut harness);
    drove(&mut harness, "input drag 700 300 --to-x 820 --to-y 370");
    steady(&mut harness);
    let found = objects(&mut harness);
    assert_eq!(found.len(), 1, "the drag drew one shape");
    assert!(found[0]["width"].as_f64().unwrap() > 40.0);
    assert_eq!(found[0]["preset"], "ellipse");
    let state = sheet(&mut harness, "shape draw off");
    let _ = state;
}
