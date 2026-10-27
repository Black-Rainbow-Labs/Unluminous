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

/// The chart named `Chart 1`, found by the name its control carries.
fn the_chart(harness: &Harness<'static, UnluminousApp>) -> egui::Rect {
    harness.get_by_label("Chart 1").rect()
}

#[test]
fn dragging_a_chart_moves_it_and_dragging_its_corner_sizes_it() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-chart-drag", "prices.csv");
    did(&mut harness, "plugins run spreadsheet chart column A1:C4 Prices");
    steady(&mut harness);
    let before = did(&mut harness, "plugins run spreadsheet chart-info 1");
    let area = the_chart(&harness);
    drag(&mut harness, area.center(), area.center() + egui::vec2(70.0, 40.0));
    let moved = did(&mut harness, "plugins run spreadsheet chart-info 1");
    let gained = |after: &serde_json::Value, key: &str| {
        after[key].as_f64().unwrap() - before[key].as_f64().unwrap()
    };
    assert!(gained(&moved, "left") > 40.0, "dragging right moved it right: {before} then {moved}");
    assert!(gained(&moved, "top") > 20.0, "dragging down moved it down: {before} then {moved}");
    assert!(gained(&moved, "width").abs() < 3.0, "moving keeps the width");
    assert!(gained(&moved, "height").abs() < 3.0, "moving keeps the height");
    assert!(
        did(&mut harness, "plugins run spreadsheet cell A1")["value"] == "Item",
        "the drag did not touch the cells"
    );
    let area = the_chart(&harness);
    let corner = area.right_bottom() - egui::vec2(6.0, 6.0);
    drag(&mut harness, corner, corner - egui::vec2(60.0, 30.0));
    let sized = did(&mut harness, "plugins run spreadsheet chart-info 1");
    let shrank = |key: &str| moved[key].as_f64().unwrap() - sized[key].as_f64().unwrap();
    assert!(
        shrank("width") > 35.0,
        "dragging the corner in made it narrower: {moved} then {sized}"
    );
    assert!(shrank("height") > 15.0, "and shorter: {moved} then {sized}");
    assert!(
        (sized["left"].as_f64().unwrap() - moved["left"].as_f64().unwrap()).abs() < 3.0,
        "the far corner stays put"
    );
    did(&mut harness, "plugins run spreadsheet undo");
    let undone = did(&mut harness, "plugins run spreadsheet chart-info 1");
    assert_eq!(undone["width"], moved["width"], "the whole drag is one undo step");
}

#[test]
fn f11_makes_a_chart_sheet_and_a_right_click_on_a_chart_opens_its_menu() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-chart-sheet", "prices.csv");
    did(&mut harness, "plugins run spreadsheet chart line A1:C4");
    steady(&mut harness);
    let area = the_chart(&harness);
    right_click_at(&mut harness, area.center());
    steady(&mut harness);
    harness.get_by_label("Change chart type");
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    did(&mut harness, "plugins run spreadsheet select A1:C4");
    press(&mut harness, egui::Key::F11, Modifiers::NONE);
    let charts = did(&mut harness, "plugins run spreadsheet charts");
    let sheets: Vec<&str> =
        charts.as_array().unwrap().iter().map(|c| c["sheet"].as_str().unwrap()).collect();
    assert_eq!(sheets.len(), 2, "F11 made a second chart: {charts}");
    assert_ne!(sheets[0], sheets[1], "on a sheet of its own");
    steady(&mut harness);
    harness.get_by_label("Chart 1");
}

/// Press at `from`, move to `to` in small steps the way a hand does, and let go, holding `modifiers`.
fn drag_in_steps(
    harness: &mut Harness<'static, UnluminousApp>,
    from: egui::Pos2,
    to: egui::Pos2,
    modifiers: Modifiers,
) {
    harness.input_mut().events.push(egui::Event::ModifiersChanged(modifiers));
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    steady(harness);
    for step in 1..=6 {
        let p = from + (to - from) * (step as f32 / 6.0);
        harness.input_mut().events.push(egui::Event::PointerMoved(p));
        steady(harness);
    }
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    steady(harness);
    harness.input_mut().events.push(egui::Event::ModifiersChanged(Modifiers::NONE));
    steady(harness);
}

/// Excel's Point mode: after `=SUM(` a drag over cells puts the whole range into the formula, a drag over
/// column headers puts whole columns, Shift and a click grows the reference and Ctrl and a click adds
/// another after a comma. `task-2382`.
#[test]
fn a_drag_while_typing_a_formula_points_at_the_whole_range() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-point-drag", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select D2");
    type_text(&mut harness, "=SUM(");
    let b2 = did(&mut harness, "plugins run spreadsheet where B2");
    let b4 = did(&mut harness, "plugins run spreadsheet where B4");
    let at = |v: &serde_json::Value| {
        egui::pos2(v["x"].as_f64().unwrap() as f32, v["y"].as_f64().unwrap() as f32)
    };
    drag_in_steps(&mut harness, at(&b2), at(&b4), Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B2:B4", "the drag points at B2:B4");
    assert_eq!(v["current"]["mode"], "Point");
    let c3 = did(&mut harness, "plugins run spreadsheet where C3");
    drag_in_steps(&mut harness, at(&c3), at(&c3), Modifiers::SHIFT);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B2:C3", "Shift and a click grows it from where it started");
    let a2 = did(&mut harness, "plugins run spreadsheet where A2");
    drag_in_steps(&mut harness, at(&a2), at(&a2), Modifiers::COMMAND);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B2:C3,A2", "Ctrl and a click adds one");
    type_text(&mut harness, ")");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let cell = did(&mut harness, "plugins run spreadsheet cell D2");
    assert_eq!(cell["value"], serde_json::json!(13.75));
    // Whole columns from their headers.
    did(&mut harness, "plugins run spreadsheet select E2");
    type_text(&mut harness, "=SUM(");
    let b1 = at(&did(&mut harness, "plugins run spreadsheet where B1"));
    let c1 = at(&did(&mut harness, "plugins run spreadsheet where C1"));
    let header = |p: egui::Pos2| p - egui::vec2(0.0, 19.0);
    drag_in_steps(&mut harness, header(b1), header(c1), Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B:C", "the headers point at whole columns");
}

/// Where a cell was drawn, from the `where` verb.
fn cell_at(harness: &mut Harness<'static, UnluminousApp>, cell: &str) -> egui::Pos2 {
    let v = did(harness, &format!("plugins run spreadsheet where {cell}"));
    egui::pos2(v["x"].as_f64().unwrap() as f32, v["y"].as_f64().unwrap() as f32)
}

/// Excel's data entry: Tab, Tab, then Enter goes to the next row of the column the Tabs started in.
#[test]
fn enter_after_a_run_of_tabs_goes_back_to_the_column_it_started_in() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-tab-enter", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select A5");
    for word in ["Jam", "1.5"] {
        type_text(&mut harness, word);
        press(&mut harness, egui::Key::Tab, Modifiers::NONE);
    }
    type_text(&mut harness, "3");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["selection"]["active"], "A6", "Enter goes back to column A");
    let cell = did(&mut harness, "plugins run spreadsheet cell C5");
    assert_eq!(cell["value"], serde_json::json!(3.0));
    // Without typing, the same walk through the cells.
    did(&mut harness, "plugins run spreadsheet select B2");
    press(&mut harness, egui::Key::Tab, Modifiers::NONE);
    press(&mut harness, egui::Key::Tab, Modifiers::NONE);
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["selection"]["active"], "B3");
}

/// Alt+= puts AutoSum's guess into the cell with the range pointed at, so an arrow can change it before
/// Enter writes it; on a block it writes a total under each column.
#[test]
fn autosum_points_at_its_guess_and_totals_a_block() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-autosum", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select B5");
    press(&mut harness, egui::Key::Equals, Modifiers::ALT);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B2:B4)");
    assert_eq!(v["current"]["mode"], "Point");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let cell = did(&mut harness, "plugins run spreadsheet cell B5");
    assert_eq!(cell["value"], serde_json::json!(9.75));
    did(&mut harness, "plugins run spreadsheet select C2:C4");
    press(&mut harness, egui::Key::Equals, Modifiers::ALT);
    let cell = did(&mut harness, "plugins run spreadsheet cell C5");
    assert_eq!(cell["value"], serde_json::json!(12.0), "a block gets its total under it");
}

/// Ctrl+Shift+A writes the argument names, F9 turns a chosen part into its value, and Ctrl and an arrow
/// points at the edge of the data.
#[test]
fn the_formula_keys_excel_has_work_while_typing() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-formula-keys", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select E2");
    type_text(&mut harness, "=SUM");
    press(&mut harness, egui::Key::A, Modifiers::COMMAND | Modifiers::SHIFT);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(number1,[number2],...)");
    press(&mut harness, egui::Key::Escape, Modifiers::NONE);
    assert_eq!(view(&mut harness)["current"]["editing"], serde_json::Value::Null, "Escape leaves the cell");
    type_text(&mut harness, "=2*B2");
    press(&mut harness, egui::Key::F9, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=7", "F9 with nothing chosen works out all of it");
    press(&mut harness, egui::Key::Escape, Modifiers::NONE);
    type_text(&mut harness, "=SUM(");
    press(&mut harness, egui::Key::ArrowLeft, Modifiers::NONE);
    press(&mut harness, egui::Key::ArrowLeft, Modifiers::NONE);
    press(&mut harness, egui::Key::ArrowLeft, Modifiers::NONE);
    press(&mut harness, egui::Key::ArrowDown, Modifiers::COMMAND | Modifiers::SHIFT);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=SUM(B2:B4", "Ctrl+Shift+Down grows to the data's edge");
}

/// Excel's range finder: a reference's coloured outline is carried by its border and resized by a corner,
/// and the formula follows.
#[test]
fn a_references_outline_is_dragged_to_move_and_resize_it() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-range-finder", "prices.csv");
    did(&mut harness, "plugins run spreadsheet select E2");
    type_text(&mut harness, "=B2+1");
    // The bottom right corner of B2's outline pulled down to B4.
    let b2 = did(&mut harness, "plugins run spreadsheet where B2");
    let corner = egui::pos2(
        (b2["left"].as_f64().unwrap() + b2["width"].as_f64().unwrap()) as f32,
        (b2["top"].as_f64().unwrap() + b2["height"].as_f64().unwrap()) as f32,
    );
    let b4 = cell_at(&mut harness, "B4");
    drag_in_steps(&mut harness, corner, b4 + egui::vec2(10.0, 4.0), Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=B2:B4+1", "the corner resizes it");
    // Its left border carried one column right.
    let b3 = cell_at(&mut harness, "B3");
    let left = egui::pos2(b2["left"].as_f64().unwrap() as f32 + 1.0, b3.y);
    let c3 = cell_at(&mut harness, "C3");
    drag_in_steps(&mut harness, left, egui::pos2(c3.x - 25.0, c3.y), Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=C2:C4+1", "the border carries it");
}

/// A click on another sheet's tab while a formula takes a reference keeps the formula open, and a click on
/// a cell there writes the sheet's name before it; Enter goes back to the formula's own sheet.
#[test]
fn pointing_reaches_a_cell_on_another_sheet() {
    let (_, mut harness) = a_workbook_window("unluminous-sheet-point-sheets", "features.xlsx");
    did(&mut harness, "plugins run spreadsheet select H20");
    type_text(&mut harness, "=");
    harness.get_by_label("Sheet Summary").click();
    steady(&mut harness);
    let v = view(&mut harness);
    assert_eq!(v["current"]["sheet"], "Summary");
    assert_eq!(v["current"]["editing"]["text"], "=Summary!");
    let a1 = cell_at(&mut harness, "A1");
    drag_in_steps(&mut harness, a1, a1, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["editing"]["text"], "=Summary!A1");
    press(&mut harness, egui::Key::Enter, Modifiers::NONE);
    let v = view(&mut harness);
    assert_eq!(v["current"]["sheet"], "Data", "Enter goes back to the formula's sheet");
    let cell = did(&mut harness, "plugins run spreadsheet cell H20");
    assert_eq!(cell["formula"], "=Summary!A1");
}
