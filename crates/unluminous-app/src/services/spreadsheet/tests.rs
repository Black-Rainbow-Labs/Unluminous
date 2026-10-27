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
                // A box of fields runs what OK would run with the fields as they open.
                catalogue::Item::Form { words, fields, title, .. } => out.push(
                    view::Prompt::form(words.clone(), title.clone(), fields.clone()).answer(),
                ),
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

#[test]
fn cells_carried_by_their_border_move_and_copy_as_excel_moves_them() {
    let (mut sheet, _folder) = with_data("move");
    run(&mut sheet, "formula D2 =B2*C2");
    run(&mut sheet, "select B2:C3");
    run(&mut sheet, "move-cells F2");
    let cell = run(&mut sheet, "cell D2");
    assert_eq!(
        cell.value["formula"],
        serde_json::json!("=F2*G2"),
        "a formula follows the cells it names"
    );
    assert_eq!(run(&mut sheet, "cell F2").value["value"], serde_json::json!(3.5));
    run(&mut sheet, "select F2:G2");
    run(&mut sheet, "move-cells F8 copy values");
    assert_eq!(run(&mut sheet, "cell G8").value["value"], serde_json::json!(4.0));
    assert_eq!(
        run(&mut sheet, "cell F2").value["value"],
        serde_json::json!(3.5),
        "a copy leaves the source"
    );
    run(&mut sheet, "select A2:A3");
    run(&mut sheet, "move-cells H2 link");
    assert_eq!(run(&mut sheet, "cell H3").value["formula"], serde_json::json!("=A3"));
}

#[test]
fn autofit_rows_fits_the_tallest_text() {
    let (mut sheet, _folder) = with_data("autofit");
    run(&mut sheet, "format A2 size 24");
    run(&mut sheet, "autofit rows A2");
    let book = sheet.current_book().unwrap();
    let height = book.session.book.sheets[0].rows.get(&1).and_then(|r| r.height).unwrap();
    assert!(height > 30.0 && height < 34.0, "a 24 point row is about 32 points tall, not {height}");
}

/// The literal words a usage line offers after its verb, such as `on` and `off` in
/// `show-formulas [on|off|toggle]`: every alternative written with `|` that is a plain lower case word.
fn offered_words(usage: &str) -> Vec<String> {
    let usage = usage.split(": ").next().unwrap_or("");
    let mut words = Vec::new();
    for token in usage.split_whitespace().skip(1) {
        let parts: Vec<&str> = token.split('|').collect();
        if parts.len() < 2 {
            continue;
        }
        for (i, part) in parts.iter().enumerate() {
            // Only the bracket that opens the group may lead and only the one that closes it may trail;
            // `[scope]|edit` is an optional argument beside a word, not two words.
            let mut word = *part;
            if i == 0 {
                word = word.trim_start_matches('[');
            }
            if i + 1 == parts.len() {
                word = word.trim_end_matches(',').trim_end_matches(']');
            }
            if !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                words.push(word.to_owned());
            }
        }
    }
    words
}

#[test]
fn every_verb_in_the_help_is_one_the_provider_runs_and_takes_the_words_it_lists() {
    for (verb, usage) in super::commands::LIST {
        // Each verb starts from the same data, because one may close or change the workbook.
        let (mut sheet, _folder) = with_data("help");
        let unknown = format!("there is no `{verb}` command");
        if let Err(e) = sheet.command(verb, &[]) {
            assert!(!e.contains(&unknown), "`{verb}` is in the help but nothing runs it");
        }
        for word in offered_words(usage) {
            if let Err(e) = sheet.command(verb, std::slice::from_ref(&word)) {
                assert!(
                    !e.contains(&format!("not `{word}`")),
                    "`{verb}` lists `{word}` in its help and refuses it: {e}"
                );
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ task-2378
// What recording the workbook beside Excel found.

#[test]
fn a_file_whose_path_holds_a_space_is_made_opened_and_saved_whole() {
    let (mut sheet, folder) = opened("spaced");
    run(&mut sheet, "new Trip budget.xlsx");
    assert!(folder.join("Trip budget.xlsx").exists(), "the file is named by every word");
    run(&mut sheet, "write A1 1");
    run(&mut sheet, "save-as Trip budget copy.xlsx");
    assert!(folder.join("Trip budget copy.xlsx").exists());
    run(&mut sheet, "save-as Trip budget.csv plain");
    assert!(
        folder.join("Trip budget.csv").exists(),
        "the encoding is the last word, not the file's"
    );
    let full = folder.join("Trip budget.xlsx").display().to_string();
    run(&mut sheet, &format!("open {full}"));
    assert_eq!(sheet.current_book().unwrap().path, folder.join("Trip budget.xlsx"));
}

#[test]
fn autofit_on_a_block_fits_only_the_rows_it_covers() {
    let (mut sheet, _folder) = with_data("autofit-block");
    run(&mut sheet, "write A8 A_title_far_longer_than_anything_in_the_table_below_it");
    run(&mut sheet, "autofit columns A1:A5");
    let width = |sheet: &mut Spreadsheet| {
        sheet.current_book().unwrap().session.book.sheets[0]
            .column(0)
            .and_then(|c| c.width)
            .unwrap()
    };
    let narrow = width(&mut sheet);
    run(&mut sheet, "autofit columns A:A");
    let wide = width(&mut sheet);
    assert!(narrow < 10.0 && wide > 30.0, "{narrow} then {wide}");
}

#[test]
fn a_bigger_font_makes_its_row_taller_in_one_undo_step() {
    let (mut sheet, _folder) = with_data("taller");
    run(&mut sheet, "format A1 size 20");
    let height = |sheet: &mut Spreadsheet| {
        sheet.current_book().unwrap().session.book.sheets[0].rows.get(&0).and_then(|r| r.height)
    };
    let grown = height(&mut sheet).expect("the row grew");
    assert!(grown > 25.0 && grown < 29.0, "Excel makes a 20 point row 26.25 points, not {grown}");
    run(&mut sheet, "undo");
    assert_eq!(height(&mut sheet), None);
    let cell = run(&mut sheet, "cell A1");
    assert_eq!(cell.value["font"]["size"], serde_json::json!(11.0), "the one step took both back");
}

#[test]
fn writing_a_cell_drops_the_words_still_being_typed_into_it() {
    let (mut sheet, _folder) = with_data("stale-edit");
    run(&mut sheet, "select C8");
    run(&mut sheet, "edit Bigg");
    run(&mut sheet, "write C8 Biggest climb");
    assert!(sheet.current_book().unwrap().view.edit.is_none());
    assert_eq!(run(&mut sheet, "cell C8").value["value"], serde_json::json!("Biggest climb"));
}

#[test]
fn a_chart_over_two_areas_is_named_by_the_first_and_choosing_cells_lets_go_of_it() {
    let (mut sheet, _folder) = with_data("two-areas");
    run(&mut sheet, "chart column A1:A5,C1:C5 Count");
    let charts = run(&mut sheet, "charts");
    let series = &charts.value[0]["series"];
    assert_eq!(series, &serde_json::json!(["Sheet1!$C$2:$C$5"]), "{charts:?}");
    assert!(sheet.current_book().unwrap().view.chart.is_some());
    run(&mut sheet, "select A1");
    assert!(sheet.current_book().unwrap().view.chart.is_none());
}

/// AutoSum on a block writes a total under each column, in the block's empty last row when it has one,
/// and across for a block one row tall (`task-2382`).
#[test]
fn autosum_on_a_block_totals_each_column_where_excel_puts_it() {
    let (mut sheet, _) = with_data("autosum-block");
    let at = |a: &str| unluminous_sheet::CellRef::parse(a).unwrap();
    let range = |a: &str| unluminous_sheet::Range::parse(a).unwrap();
    let book = sheet.current_book().unwrap();
    let s = &book.session.book.sheets[0];
    assert_eq!(
        actions::autosum_totals(s, range("B2:C5")),
        vec![(at("B6"), "=SUM(B2:B5)".to_owned()), (at("C6"), "=SUM(C2:C5)".to_owned())]
    );
    assert_eq!(
        actions::autosum_totals(s, range("B2:C6")),
        vec![(at("B6"), "=SUM(B2:B5)".to_owned()), (at("C6"), "=SUM(C2:C5)".to_owned())],
        "an empty last row takes the totals"
    );
    assert_eq!(
        actions::autosum_totals(s, range("B2:C2")),
        vec![(at("D2"), "=SUM(B2:C2)".to_owned())]
    );
    assert!(actions::autosum_totals(s, range("A2:A5")).is_empty(), "text has nothing to add up");
}

/// Formula AutoComplete offers names that start with the letters first, then names that hold them, and never
/// offers a function for what is a cell's address.
#[test]
fn the_function_list_offers_starts_then_insides_and_defined_names() {
    let (mut sheet, _) = with_data("offered");
    run(&mut sheet, "names new Prices Sheet1!$B$2:$B$5");
    let book = sheet.current_book().unwrap();
    let offered = crate::components::spreadsheet::editor::offered_names(book, "PRI");
    assert_eq!(&offered[..4], ["PRICE", "PRICEDISC", "PRICEMAT", "Prices"], "{offered:?}");
    let offered = crate::components::spreadsheet::editor::offered_names(book, "DAYS");
    assert_eq!(offered[0], "DAYS");
    assert!(offered.iter().any(|n| n == "NETWORKDAYS"), "{offered:?}");
    assert!(crate::components::spreadsheet::editor::offered_names(book, "C2").is_empty());
}
