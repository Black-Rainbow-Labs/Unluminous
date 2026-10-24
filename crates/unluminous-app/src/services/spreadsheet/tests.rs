//! The provider driven through its commands, as an agent drives it, with no window.

use super::*;

/// A provider opened on a scratch project folder of its own, and that folder.
fn opened(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let folder =
        std::env::temp_dir().join(format!("unluminous-spreadsheet-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut sheet = Spreadsheet::new();
    let context = Context { project: Some(folder.clone()), ..Context::default() };
    sheet.open(&context).unwrap();
    (sheet, folder)
}

fn run(sheet: &mut Spreadsheet, line: &str) -> Answer {
    let words: Vec<String> = line.split(' ').map(str::to_owned).collect();
    sheet.command(&words[0], &words[1..]).unwrap_or_else(|e| panic!("`{line}` failed: {e}"))
}

#[test]
fn an_agent_makes_a_workbook_types_formulas_and_saves_it() {
    let (mut sheet, folder) = opened("budget");
    run(&mut sheet, "new budget.xlsx");
    run(&mut sheet, "write A1 [[\"Item\",\"Cost\"],[\"Tea\",3],[\"Cake\",4.5]]");
    run(&mut sheet, "formula B4 =SUM(B2:B3)");
    let cell = run(&mut sheet, "cell B4");
    assert_eq!(cell.value["value"], serde_json::json!(7.5));
    run(&mut sheet, "format A1:B1 bold fill #FFF2CC");
    run(&mut sheet, "save");
    let bytes = std::fs::read(folder.join("budget.xlsx")).unwrap();
    let book =
        unluminous_sheet::open(&bytes, unluminous_sheet::FileKind::Xlsx, "budget", 2026).unwrap();
    assert_eq!(
        book.sheets[0].value_at(unluminous_sheet::CellRef::new(3, 1)),
        unluminous_sheet::Value::Number(7.5)
    );
}

/// A provider with a small table of data open and nothing saved yet.
fn with_data(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let (mut sheet, folder) = opened(name);
    run(&mut sheet, "new data.xlsx");
    run(&mut sheet, "write A1 [[\"Item\",\"Price\",\"Count\"],[\"Tea\",3.5,4],[\"Cake\",4.25,2],[\"Scone\",2,6],[\"Tea\",3.5,1]]");
    (sheet, folder)
}

#[test]
fn every_entry_in_the_menus_runs_the_words_it_names() {
    let (mut sheet, _folder) = with_data("catalogue");
    let book = sheet.current_book().unwrap();
    let mut entries = catalogue::everything(book);
    entries.extend(catalogue::cell_menu(book));
    entries.extend(catalogue::sheet_menu(book, 0));
    entries.extend(catalogue::panel_root(book));
    fn leaves(items: &[catalogue::Item], out: &mut Vec<Vec<String>>) {
        for item in items {
            match item {
                catalogue::Item::Run { words, .. } => out.push(words.clone()),
                catalogue::Item::Ask { words, second, .. } => {
                    let mut w = words.clone();
                    w.push("5".to_owned());
                    if second.is_some() {
                        w.push("9".to_owned());
                    }
                    out.push(w);
                }
                catalogue::Item::Colors { words, .. } => {
                    let mut w = words.clone();
                    w.push("#FF0000".to_owned());
                    out.push(w);
                }
                catalogue::Item::Sub { items, .. } => leaves(items, out),
                catalogue::Item::Line => {}
            }
        }
    }
    let mut all = Vec::new();
    leaves(&entries, &mut all);
    assert!(all.len() > 150, "the menus offer {} entries", all.len());
    for words in all {
        // Every entry is a verb this provider has. A refusal for a reason (nothing copied yet, a sheet that
        // cannot be deleted) is fine; a verb it does not know is not.
        if words[0] == "save-as"
            || words[0] == "sheet" && words.get(1).map(String::as_str) == Some("delete")
        {
            continue;
        }
        let book = sheet.current_book().unwrap();
        book.view.select_range(unluminous_sheet::Range::parse("A1:C3").unwrap());
        if let Err(problem) = sheet.command(&words[0], &words[1..]) {
            assert!(
                !problem.contains("there is no `"),
                "`{}` is not a command: {problem}",
                words.join(" ")
            );
        }
    }
}

#[test]
fn sorting_filtering_and_rules_act_on_the_block_of_data() {
    let (mut sheet, _folder) = with_data("data-tools");
    run(&mut sheet, "sort A1 B desc");
    let read = run(&mut sheet, "read A2:A5");
    assert_eq!(read.value["rows"], serde_json::json!([["Cake"], ["Tea"], ["Tea"], ["Scone"]]));
    run(&mut sheet, "filter A1");
    run(&mut sheet, "filter A Tea");
    let view = commands::view_value(&sheet);
    assert!(view["current"]["file"].as_str().unwrap().ends_with("data.xlsx"));
    let book = sheet.current_book().unwrap();
    assert!(book.session.book.sheets[0].is_row_hidden(1), "Cake's row is filtered out");
    run(&mut sheet, "filter off");
    run(&mut sheet, "conditional B2:B5 greater 3 fill #FFC7CE");
    let book = sheet.current_book().unwrap();
    let rules = conditional::Rules::prepare(&book.session.book, 0, 1);
    assert_eq!(
        rules.effect(&book.session.book, 0.0, unluminous_sheet::CellRef::new(1, 1)).fill,
        Some((0xFF, 0xC7, 0xCE))
    );
    run(&mut sheet, "remove-duplicates A1:B5");
    let read = run(&mut sheet, "read A1:A5");
    assert_eq!(read.value["rows"].as_array().unwrap().iter().filter(|r| r[0] == "Tea").count(), 1);
}

#[test]
fn validation_refuses_a_value_the_rule_does_not_allow() {
    let (mut sheet, _folder) = with_data("validation");
    run(&mut sheet, "validate C2:C5 whole between 1 10");
    run(&mut sheet, "select C2");
    let refused = sheet.command("type", &["42".to_owned()]);
    assert!(refused.is_err(), "42 is outside 1 to 10");
    run(&mut sheet, "validate E1 list Yes,No");
    let book = sheet.current_book().unwrap();
    assert_eq!(
        book.session.list_choices(0, unluminous_sheet::CellRef::new(0, 4)),
        vec!["Yes", "No"]
    );
}

#[test]
fn copy_and_paste_move_formulas_and_undo_puts_cells_back() {
    let (mut sheet, _folder) = with_data("copy");
    run(&mut sheet, "formula D2 =B2*C2");
    run(&mut sheet, "copy D2");
    run(&mut sheet, "paste D3:D5");
    let cell = run(&mut sheet, "cell D4");
    assert_eq!(cell.value["formula"], "=B4*C4");
    assert_eq!(cell.value["value"], serde_json::json!(12.0));
    run(&mut sheet, "undo");
    let cell = run(&mut sheet, "cell D4");
    assert_eq!(cell.value["value"], serde_json::Value::Null);
    run(&mut sheet, "redo");
    let cell = run(&mut sheet, "cell D4");
    assert_eq!(cell.value["value"], serde_json::json!(12.0));
}

#[test]
fn a_chart_made_here_is_saved_and_read_back() {
    let (mut sheet, folder) = with_data("chart");
    run(&mut sheet, "chart column A1:B5 Prices");
    run(&mut sheet, "save");
    let bytes = std::fs::read(folder.join("data.xlsx")).unwrap();
    let book =
        unluminous_sheet::open(&bytes, unluminous_sheet::FileKind::Xlsx, "data", 2026).unwrap();
    let charts = &book.sheets[0].charts;
    assert_eq!(charts.len(), 1);
    assert_eq!(charts[0].kind, unluminous_sheet::model::ChartKind::Column);
    assert_eq!(charts[0].title.as_deref(), Some("Prices"));
    assert_eq!(charts[0].series[0].values, "Sheet1!$B$2:$B$5");
}

#[test]
fn a_csv_file_is_typed_on_reading_and_saved_as_it_shows() {
    let (mut sheet, folder) = opened("csv");
    std::fs::write(folder.join("prices.csv"), "Item,Price\nTea,3.5\n").unwrap();
    run(&mut sheet, "open prices.csv");
    let cell = run(&mut sheet, "cell B2");
    assert_eq!(cell.value["value"], serde_json::json!(3.5));
    run(&mut sheet, "write C1 Total");
    run(&mut sheet, "formula C2 =B2*2");
    run(&mut sheet, "save");
    assert_eq!(
        std::fs::read_to_string(folder.join("prices.csv")).unwrap(),
        "Item,Price,Total\r\nTea,3.5,7\r\n"
    );
}

#[test]
fn decimals_step_a_format_one_place_at_a_time() {
    let none = unluminous_sheet::Value::Number(1.0);
    assert_eq!(
        commands::step_decimals("General", &unluminous_sheet::Value::Number(1.25), true),
        "0.000"
    );
    assert_eq!(commands::step_decimals("0.00", &none, true), "0.000");
    assert_eq!(commands::step_decimals("0.0", &none, false), "0");
    assert_eq!(commands::step_decimals("#,##0", &none, true), "#,##0.0");
    assert_eq!(
        commands::step_decimals("\"$\"#,##0.00;[Red]\"$\"#,##0.00", &none, false),
        "\"$\"#,##0.0;[Red]\"$\"#,##0.0"
    );
}

#[test]
fn a_tab_draws_a_frame_without_a_window() {
    let (mut sheet, _folder) = with_data("frame");
    let key = sheet.current.clone().unwrap();
    let settings = crate::settings::Settings::default();
    let renderer = crate::services::text_renderer::TextRenderer::new();
    let ctx = egui::Context::default();
    // One frame: a context with no painter behind it cannot take the atlas's second upload.
    for _ in 0..1 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| {
                let look = Look::of(&settings, &renderer).holding_the_keyboard(true);
                sheet.tab_keyed(&key, ui, &look);
            },
        );
        output.textures_delta.clear();
    }
}
