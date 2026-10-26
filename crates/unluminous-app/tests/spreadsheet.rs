//! Excel workbooks in a real window: a workbook drawn with its formats, typing into cells with the keys
//! Excel uses, the Format panel's levels, the menus, saving, and the agent's verbs reaching the same tab.
//! `task-2338`.
//!
//! The pictures open `crates/unluminous-sheet/tests/fixtures/features.xlsx`, which Excel made, so what is
//! drawn is a workbook as Excel writes one.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

/// A folder holding a copy of Excel's feature workbook and a small CSV file.
fn a_workbook_folder(name: &str) -> std::path::PathBuf {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../unluminous-sheet/tests/fixtures/features.xlsx");
    let folder = fixture(
        name,
        &[
            ("readme.md", "# A project with a workbook\n"),
            ("prices.csv", "Item,Price,Count\nTea,3.5,4\nCake,4.25,2\nScone,2,6\n"),
        ],
    );
    std::fs::copy(&source, folder.join("features.xlsx")).expect("copy the workbook");
    folder
}

/// A window on a copy of the feature workbook, with the workbook open.
fn a_workbook_window(
    name: &str,
    file: &str,
) -> (std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = a_workbook_folder(name);
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {file}"));
    steady(&mut harness);
    (folder, harness)
}

/// Type text the way a keyboard does, one text event.
fn type_text(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    harness.event(egui::Event::Text(text.to_owned()));
    steady(harness);
}

/// Press a key with modifiers.
fn press(harness: &mut Harness<'static, UnluminousApp>, key: egui::Key, modifiers: Modifiers) {
    harness.key_press_modifiers(modifiers, key);
    steady(harness);
}

/// The plugin's view, as `plugins view spreadsheet` answers it.
fn view(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    did(harness, "plugins view spreadsheet")
}

#[test]
fn a_workbook_opens_in_a_tab_of_its_own_and_is_drawn_with_its_formats() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-drawn", "features.xlsx");
    let v = view(&mut harness);
    assert_eq!(v["current"]["sheet"], "Data", "the workbook opens on the sheet Excel saved it on");
    harness.snapshot(shot("spreadsheet_drawn"));
}

#[test]
fn typing_a_formula_and_pressing_enter_writes_it_and_moves_down() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-typing", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select D2");
    type_text(&mut harness, "=B2*C2");
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=B2*C2");
    harness.snapshot(shot("spreadsheet_typing"));
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["selection"]["active"], "D3", "Enter moves down");
    let cell = did(&mut harness, "plugins run spreadsheet cell D2");
    assert_eq!(cell["value"], serde_json::json!(14.0));
    assert!(v["current"]["modified"].as_bool().unwrap(), "the tab has unsaved changes");
    press(&mut harness, egui::Key::Z, Modifiers::COMMAND);
    let cell = did(&mut harness, "plugins run spreadsheet cell D2");
    assert_eq!(cell["value"], serde_json::Value::Null, "Ctrl+Z undoes the cell");
}

/// In the page layout view the cells sit on their sheets of paper and are typed into with the keys of the
/// normal view: a letter starts editing, Enter writes the cell and moves down.
#[test]
fn a_cell_in_the_page_layout_view_is_typed_into_and_read_back() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-layout-typing", "prices.csv");
    did(&mut harness, "plugins run spreadsheet view layout");
    steady(&mut harness);
    did(&mut harness, "plugins run spreadsheet select D2");
    type_text(&mut harness, "=B2*C2");
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=B2*C2");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["selection"]["active"], "D3", "Enter moves down");
    let cell = did(&mut harness, "plugins run spreadsheet cell D2");
    assert_eq!(cell["value"], serde_json::json!(14.0));
}

#[test]
fn the_format_panel_goes_down_a_level_at_a_time() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-panel", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select A1:C1");
    harness.get_by_label("Format").click();
    steady(&mut harness);
    harness.snapshot(shot("spreadsheet_panel"));
    harness.get_by_label("Number").click();
    steady(&mut harness);
    harness.get_by_label_contains("Currency").click();
    steady(&mut harness);
    let cell = did(&mut harness, "plugins run spreadsheet cell B1");
    assert_eq!(cell["number_format"], "\"$\"#,##0.00");
    harness.snapshot(shot("spreadsheet_panel_number"));
}

#[test]
fn the_right_click_menu_and_the_workbook_menu_are_drawn_from_one_tree() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-menus", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select B2:B4");
    right_click_at(&mut harness, egui::pos2(430.0, 186.0));
    steady(&mut harness);
    harness.snapshot(shot("spreadsheet_cell_menu"));
    harness.get_by_label("Insert").hover();
    steady(&mut harness);
    harness.snapshot(shot("spreadsheet_cell_menu_insert"));
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    harness.get_by_label("Workbook menu").click();
    steady(&mut harness);
    harness.snapshot(shot("spreadsheet_book_menu"));
    type_text(&mut harness, "currency");
    harness.snapshot(shot("spreadsheet_book_menu_search"));
}

#[test]
fn the_function_list_opens_while_a_function_name_is_typed() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-functions", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select D2");
    type_text(&mut harness, "=SU");
    harness.snapshot(shot("spreadsheet_function_list"));
    type_text(&mut harness, "M");
    press(&mut harness, egui::Key::Tab, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(", "Tab puts the first function in");
    type_text(&mut harness, "B2:B4)");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let cell = did(&mut harness, "plugins run spreadsheet cell D2");
    assert_eq!(cell["value"], serde_json::json!(9.75));
}

#[test]
fn ctrl_s_saves_the_workbook_and_closing_a_changed_tab_saves_it_too() {
    let (folder, mut harness) = a_workbook_window("unluminous-sheet-saving", "prices.csv");
    did(&mut harness, "plugins run spreadsheet write D1 Total");
    let v = view(&mut harness);
    assert!(v["current"]["modified"].as_bool().unwrap());
    press(&mut harness, egui::Key::S, Modifiers::COMMAND);
    let text = std::fs::read_to_string(folder.join("prices.csv")).unwrap();
    assert!(text.starts_with("Item,Price,Count,Total\r\n"), "saved: {text}");
    did(&mut harness, "plugins run spreadsheet write D2 =B2*C2");
    did(&mut harness, "tab close");
    let text = std::fs::read_to_string(folder.join("prices.csv")).unwrap();
    assert!(text.contains("Tea,3.5,4,14\r\n"), "closing wrote it: {text}");
}

#[test]
fn copy_and_paste_with_the_keys_moves_a_formula_along() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-clipboard", "prices.csv");
    did(&mut harness, "plugins run spreadsheet formula D2 =B2*C2");
    did(&mut harness, "plugins run spreadsheet select D2");
    harness.event(egui::Event::Copy);
    steady(&mut harness);
    did(&mut harness, "plugins run spreadsheet select D3:D4");
    harness.event(egui::Event::Paste("14\r\n".to_owned()));
    steady(&mut harness);
    let cell = did(&mut harness, "plugins run spreadsheet cell D4");
    assert_eq!(cell["formula"], "=B4*C4", "the copied formula is pasted, not its shown value");
    harness.snapshot(shot("spreadsheet_pasted"));
}

#[test]
fn a_chart_and_light_paper() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-chart", "prices.csv");
    did(&mut harness, "plugins run spreadsheet chart column A1:B4 Prices");
    did(&mut harness, "plugins run spreadsheet conditional C2:C4 bar #638EC6");
    did(&mut harness, "plugins run spreadsheet format A1:C1 bold fill #DDEBF7");
    did(&mut harness, "plugins run spreadsheet paper light");
    steady(&mut harness);
    harness.snapshot(shot("spreadsheet_chart_light"));
}

#[test]
fn switching_the_plugin_off_opens_a_csv_file_as_text() {
    let folder = a_workbook_folder("unluminous-sheet-off");
    let mut harness = harness_in(&folder);
    did(&mut harness, "plugins disable spreadsheet");
    did(&mut harness, "tab open prices.csv");
    steady(&mut harness);
    let text = did(&mut harness, "editor text");
    assert!(text["text"].as_str().unwrap().starts_with("Item,Price,Count"), "{text}");
    let opened = did(&mut harness, "tab open features.xlsx");
    assert_eq!(
        opened["rendered"], false,
        "a workbook is not shown while its plugin is off: {opened}"
    );
}
