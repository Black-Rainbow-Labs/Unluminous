//! The clipboard and fill, page setup and print, pivot table and file format verbs driven as an agent drives
//! them (`task-2360`, the `wiringb` lane).

use serde_json::json;

use super::*;

/// A provider opened on a scratch project folder of its own, and that folder.
fn opened(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-b-{name}-{}", std::process::id()));
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

fn refused(sheet: &mut Spreadsheet, line: &str) -> String {
    let words: Vec<String> = line.split(' ').map(str::to_owned).collect();
    sheet.command(&words[0], &words[1..]).expect_err(line)
}

/// The number in a cell.
fn number(sheet: &mut Spreadsheet, cell: &str) -> f64 {
    run(sheet, &format!("cell {cell}")).value["value"].as_f64().unwrap_or(f64::NAN)
}

fn text(sheet: &mut Spreadsheet, cell: &str) -> String {
    run(sheet, &format!("cell {cell}")).value["value"].as_str().unwrap_or("").to_owned()
}

/// A workbook with a column of numbers and a column of ten times as many.
fn with_columns(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let (mut sheet, folder) = opened(name);
    run(&mut sheet, "new data.xlsx");
    run(&mut sheet, "write A1 [[1,10],[2,20],[3,30]]");
    (sheet, folder)
}

// ------------------------------------------------------------------------------------------------ paste

#[test]
fn paste_special_adds_the_copied_numbers_to_the_numbers_under_them() {
    let (mut sheet, _folder) = with_columns("paste-add");
    run(&mut sheet, "copy A1:A3");
    run(&mut sheet, "paste-special B1 add");
    assert_eq!(
        [number(&mut sheet, "B1"), number(&mut sheet, "B2"), number(&mut sheet, "B3")],
        [11.0, 22.0, 33.0]
    );
    run(&mut sheet, "undo");
    run(&mut sheet, "copy A1:A3");
    run(&mut sheet, "paste B1 multiply");
    assert_eq!(number(&mut sheet, "B3"), 90.0, "the old `paste` takes the new words too");
}

#[test]
fn paste_special_skips_blanks_transposes_and_pastes_only_values() {
    let (mut sheet, _folder) = with_columns("paste-kinds");
    run(&mut sheet, "formula C1 =A1+A2");
    run(&mut sheet, "copy C1");
    run(&mut sheet, "paste-special D1 values");
    let cell = run(&mut sheet, "cell D1");
    assert_eq!(cell.value["formula"], json!(null), "values only");
    assert_eq!(cell.value["value"], json!(3.0));
    run(&mut sheet, "copy A1:A3");
    run(&mut sheet, "paste-special F1 transpose");
    assert_eq!(number(&mut sheet, "H1"), 3.0, "a column became a row");
    run(&mut sheet, "copy A1:A3");
    run(&mut sheet, "paste-special B2 skip-blanks");
    assert_eq!(number(&mut sheet, "B2"), 1.0);
}

#[test]
fn paste_special_with_no_words_opens_the_popup_and_paste_link_names_the_source() {
    let (mut sheet, _folder) = with_columns("paste-link");
    run(&mut sheet, "copy A1:A2");
    run(&mut sheet, "paste-special");
    assert!(sheet.current_book().unwrap().view.lane_b.paste_draft.is_some());
    run(&mut sheet, "paste-link D1");
    assert_eq!(run(&mut sheet, "cell D2").value["formula"], json!("=A2"));
    assert_eq!(number(&mut sheet, "D2"), 2.0);
    assert!(refused(&mut sheet, "paste-special D1 sideways").contains("sideways"));
}

#[test]
fn insert_copied_makes_room_and_the_paste_options_button_pastes_again() {
    let (mut sheet, _folder) = with_columns("insert-copied");
    run(&mut sheet, "copy A1:A2");
    run(&mut sheet, "insert-copied A3 down");
    assert_eq!(
        [number(&mut sheet, "A3"), number(&mut sheet, "A4"), number(&mut sheet, "A5")],
        [1.0, 2.0, 3.0]
    );
    run(&mut sheet, "undo");
    run(&mut sheet, "formula D1 =A1*2");
    run(&mut sheet, "copy D1");
    run(&mut sheet, "paste E1");
    assert_eq!(run(&mut sheet, "cell E1").value["formula"], json!("=B1*2"));
    run(&mut sheet, "paste-options values");
    assert_eq!(run(&mut sheet, "cell E1").value["formula"], json!(null));
    run(&mut sheet, "write G1 5");
    assert!(refused(&mut sheet, "paste-options formulas").contains("gone"), "the workbook changed");
}

#[test]
fn text_from_another_program_is_pasted_as_typed_text() {
    let (mut sheet, _folder) = opened("paste-text");
    run(&mut sheet, "new t.xlsx");
    run(&mut sheet, "paste-text A1 a\\tb\\n=1+1\\t7");
    assert_eq!(text(&mut sheet, "B1"), "b");
    assert_eq!(text(&mut sheet, "A2"), "=1+1", "a formula from outside is text");
    run(&mut sheet, "paste-text A4 formulas =1+1");
    assert_eq!(number(&mut sheet, "A4"), 2.0);
    run(&mut sheet, "paste-html A6 <table><tr><td>x</td><td>9</td></tr></table>");
    assert_eq!(number(&mut sheet, "B6"), 9.0);
}

// -------------------------------------------------------------------------------------------------- fill

#[test]
fn the_fill_handle_menu_fills_a_series_a_copy_or_a_trend() {
    let (mut sheet, _folder) = opened("fill-handle");
    run(&mut sheet, "new f.xlsx");
    run(&mut sheet, "write A1 [[1],[2]]");
    run(&mut sheet, "select A1:A2");
    run(&mut sheet, "fill-handle A1:A5 series");
    assert_eq!(number(&mut sheet, "A5"), 5.0);
    run(&mut sheet, "fill-options copy");
    assert_eq!(
        [number(&mut sheet, "A3"), number(&mut sheet, "A4")],
        [1.0, 2.0],
        "copy cells repeats"
    );
    run(&mut sheet, "write C1 [[2],[4]]");
    run(&mut sheet, "select C1:C2");
    run(&mut sheet, "fill-handle C1:C4 growth");
    assert_eq!(number(&mut sheet, "C4"), 16.0);
    run(&mut sheet, "select C1:C2");
    run(&mut sheet, "fill-handle C1:C4 linear");
    assert_eq!(number(&mut sheet, "C4"), 8.0);
    run(&mut sheet, "write E1 [[1],[2]]");
    run(&mut sheet, "select E1:E2");
    run(&mut sheet, "fill-handle E1:E6 series control");
    assert_eq!(
        [number(&mut sheet, "E3"), number(&mut sheet, "E4"), number(&mut sheet, "E5")],
        [1.0, 2.0, 1.0],
        "Ctrl with the handle copies numbers"
    );
}

#[test]
fn the_series_form_justify_and_flash_fill_write_what_excel_writes() {
    let (mut sheet, _folder) = opened("series");
    run(&mut sheet, "new s.xlsx");
    run(&mut sheet, "write A1 1");
    run(&mut sheet, "series A1:A5 columns linear step 2");
    assert_eq!(number(&mut sheet, "A5"), 9.0);
    run(&mut sheet, "write B1 1");
    run(&mut sheet, "select B1:B4");
    run(&mut sheet, "fill series growth step 3");
    assert_eq!(number(&mut sheet, "B4"), 27.0);
    run(&mut sheet, "write D1 [[\"Ann Lee\"],[\"Bob Ray\"],[\"Cy Doe\"]]");
    run(&mut sheet, "write E1 Lee, Ann");
    let made = run(&mut sheet, "flash-fill E1:E3");
    assert!(made.message.contains("E"), "{}", made.message);
    assert_eq!(text(&mut sheet, "E2"), "Ray, Bob");
    run(&mut sheet, "write G1 [[\"the quick\"],[\"brown fox\"],[\"jumps over\"]]");
    run(&mut sheet, "select G1:G3");
    run(&mut sheet, "justify");
    assert!(text(&mut sheet, "G1").contains("quick"));
}

#[test]
fn fill_across_worksheets_copies_the_range_to_the_others() {
    let (mut sheet, _folder) = with_columns("across");
    run(&mut sheet, "sheet add Two");
    run(&mut sheet, "sheet show Sheet1");
    run(&mut sheet, "fill-across A1:B3 Two contents");
    run(&mut sheet, "sheet show Two");
    assert_eq!(number(&mut sheet, "B3"), 30.0);
}

// --------------------------------------------------------------------------------------------- pages

/// A workbook with a few hundred rows, so that it prints on several pages.
fn long_workbook(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let (mut sheet, folder) = opened(name);
    run(&mut sheet, "new long.xlsx");
    let rows: Vec<serde_json::Value> = (1..=120).map(|i| json!([format!("Row {i}"), i])).collect();
    run(&mut sheet, &format!("write A1 {}", serde_json::Value::Array(rows)));
    (sheet, folder)
}

#[test]
fn page_setup_changes_are_read_back_and_change_the_pages() {
    let (mut sheet, _folder) = long_workbook("page-setup");
    let before = run(&mut sheet, "page-setup").value["pages"].as_u64().unwrap();
    assert!(before >= 2, "{before} pages");
    run(&mut sheet, "page-setup orientation landscape");
    run(&mut sheet, "page-setup paper A4");
    run(&mut sheet, "page-setup margins narrow");
    run(&mut sheet, "page-setup gridlines on");
    run(&mut sheet, "page-setup header Report");
    run(&mut sheet, "page-setup footer right Page &P of &N");
    run(&mut sheet, "page-setup order over");
    let read = run(&mut sheet, "page-setup").value;
    assert_eq!(read["orientation"], json!("landscape"));
    assert_eq!(read["paper"]["name"], json!("A4"));
    assert_eq!(read["margins"]["left"], json!(0.25));
    assert_eq!(read["gridlines"], json!(true));
    assert_eq!(read["header"]["odd"], json!("&CReport"));
    assert_eq!(read["footer"]["odd"], json!("&RPage &P of &N"));
    assert_eq!(read["pageOrder"], json!("over"));
    run(&mut sheet, "page-setup fit 1 1");
    assert_eq!(run(&mut sheet, "page-setup").value["pages"], json!(1), "fit to one page");
    run(&mut sheet, "page-setup scale 100");
    assert!(run(&mut sheet, "page-setup").value["pages"].as_u64().unwrap() > 1);
}

#[test]
fn print_area_titles_and_breaks_are_set_and_read_back() {
    let (mut sheet, _folder) = long_workbook("print-area");
    run(&mut sheet, "print-area set A1:B20");
    assert_eq!(run(&mut sheet, "print-area").value, json!(["A1:B20"]));
    assert_eq!(run(&mut sheet, "page-setup").value["pages"], json!(1));
    run(&mut sheet, "print-area clear");
    run(&mut sheet, "print-titles rows 1:2");
    assert_eq!(run(&mut sheet, "print-titles").value["rows"], json!("1:2"));
    run(&mut sheet, "page-break insert A30");
    assert!(run(&mut sheet, "page-break").value["rows"].as_array().unwrap().contains(&json!(30)));
    run(&mut sheet, "page-break remove A30");
    run(&mut sheet, "page-break insert A40");
    run(&mut sheet, "page-break reset");
    assert!(run(&mut sheet, "page-setup").value["manualRowBreaks"].as_array().unwrap().is_empty());
    run(&mut sheet, "print-titles clear");
    assert_eq!(run(&mut sheet, "print-titles").value["rows"], json!(null));
}

#[test]
fn the_pdf_has_the_pages_the_paginator_lays_out_and_the_text_on_each() {
    let (mut sheet, folder) = long_workbook("pdf");
    run(&mut sheet, "page-setup footer center Page &P of &N");
    run(&mut sheet, "page-setup header left &A");
    let preview = run(&mut sheet, "print-preview").value;
    let pages = preview["pages"].as_u64().unwrap() as usize;
    assert!(pages >= 2);
    let book = sheet.current_book().unwrap();
    let (bytes, layout) = print::pdf_bytes(book);
    assert_eq!(layout.page_count(), pages);
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(text.matches("/Type /Page ").count(), pages, "one PDF page for each page laid out");
    assert!(text.contains(&format!("/Count {pages}")));
    let mut measurer = print::Measurer::new();
    let drawn = print::page_texts(&print::pdf_pages(book, &layout, &mut measurer));
    assert_eq!(drawn.len(), pages);
    assert!(drawn[0].contains("Row 1"), "{}", drawn[0]);
    assert!(drawn[0].contains("Sheet1"), "the header is on the first page");
    assert!(drawn[0].contains(&format!("Page 1 of {pages}")), "{}", drawn[0]);
    assert!(drawn[pages - 1].contains("Row 120"), "the last row is on the last page");
    assert!(drawn[pages - 1].contains(&format!("Page {pages} of {pages}")));
    assert!(!drawn[1].contains("Row 1 "), "page two starts after page one");
    let file = folder.join("out.pdf");
    run(&mut sheet, &format!("export pdf {}", file.display()));
    assert!(std::fs::read(&file).unwrap().starts_with(b"%PDF-1.7"));
    let printed = run(&mut sheet, &format!("print {}", folder.join("printed.pdf").display()));
    assert_eq!(printed.value["pages"].as_u64().unwrap() as usize, pages);
}

#[test]
fn the_preview_steps_through_the_pages_and_closes() {
    let (mut sheet, _folder) = long_workbook("preview");
    let first = run(&mut sheet, "print-preview open");
    assert_eq!(first.value["page"], json!(1));
    let second = run(&mut sheet, "print-preview next");
    assert_eq!(second.value["page"], json!(2));
    assert!(second.value["text"].as_str().unwrap().contains("Row"));
    assert_eq!(run(&mut sheet, "print-preview previous").value["page"], json!(1));
    run(&mut sheet, "print-preview close");
    assert!(sheet.current_book().unwrap().view.lane_b.preview.is_none());
}

// ------------------------------------------------------------------------------------------------ pivots

/// A workbook with sales to summarise.
fn with_sales(name: &str) -> (Spreadsheet, std::path::PathBuf) {
    let (mut sheet, folder) = opened(name);
    run(&mut sheet, "new sales.xlsx");
    run(&mut sheet, "write A1 [[\"Region\",\"Item\",\"Units\"],[\"East\",\"Tea\",5],[\"East\",\"Cake\",3],[\"West\",\"Tea\",7],[\"West\",\"Cake\",1],[\"East\",\"Tea\",2]]");
    (sheet, folder)
}

#[test]
fn a_pivot_table_is_made_from_the_data_and_summarises_it() {
    let (mut sheet, _folder) = with_sales("pivot");
    run(&mut sheet, "select A1");
    let made = run(&mut sheet, "pivot create at E1");
    assert_eq!(made.value["name"], json!("PivotTable1"));
    run(&mut sheet, "pivot add Region rows");
    run(&mut sheet, "pivot add Units values sum");
    let read = run(&mut sheet, "read E1:F5 values").value;
    let rows = read["rows"].as_array().unwrap();
    let flat: Vec<String> =
        rows.iter().flat_map(|r| r.as_array().unwrap().iter().map(|v| v.to_string())).collect();
    let joined = flat.join(" ");
    assert!(joined.contains("East") && joined.contains("10"), "{joined}");
    assert!(joined.contains("West") && joined.contains("8"), "{joined}");
    assert!(joined.contains("18"), "the grand total: {joined}");
    let listed = run(&mut sheet, "pivots").value;
    assert_eq!(listed[0]["rows"], json!(["Region"]));
    assert_eq!(listed[0]["values"][0]["name"], json!("Sum of Units"));
    assert_eq!(listed[0]["source"], json!("Sheet1!A1:C6"));
}

#[test]
fn value_field_settings_columns_totals_and_layout_change_the_pivot() {
    let (mut sheet, _folder) = with_sales("pivot-settings");
    run(&mut sheet, "pivot create A1:C6 at E1 name Sales");
    run(&mut sheet, "pivot add Region rows in Sales");
    run(&mut sheet, "pivot add Item columns in Sales");
    run(&mut sheet, "pivot add Units values count in Sales");
    run(&mut sheet, "pivot value 1 summarize sum show percent-of-total name Share in Sales");
    let table = run(&mut sheet, "pivot fields in Sales").value;
    assert_eq!(table["values"][0]["name"], json!("Share"));
    assert_eq!(table["values"][0]["showAs"], json!("percent-of-total"));
    run(&mut sheet, "pivot layout tabular in Sales");
    run(&mut sheet, "pivot totals none in Sales");
    let table = run(&mut sheet, "pivot fields in Sales").value;
    assert_eq!(table["layout"], json!("tabular"));
    assert_eq!(table["grandTotals"], json!({ "rows": false, "columns": false }));
    run(&mut sheet, "pivot move Item rows in Sales");
    assert_eq!(run(&mut sheet, "pivots").value[0]["rows"], json!(["Region", "Item"]));
    run(&mut sheet, "pivot remove Item in Sales");
    assert_eq!(run(&mut sheet, "pivots").value[0]["rows"], json!(["Region"]));
    assert!(refused(&mut sheet, "pivot add Nothing rows in Sales").contains("Nothing"));
}

#[test]
fn pivot_filters_sorts_slicers_and_refresh_follow_the_data() {
    let (mut sheet, _folder) = with_sales("pivot-filter");
    run(&mut sheet, "pivot create A1:C6 at E1 name Sales");
    run(&mut sheet, "pivot add Item rows in Sales");
    run(&mut sheet, "pivot add Units values sum in Sales");
    run(&mut sheet, "pivot sort Item descending in Sales");
    assert_eq!(text(&mut sheet, "E2"), "Tea");
    run(&mut sheet, "slicer add Item in Sales name Items");
    let selected = run(&mut sheet, "slicer select Items Cake");
    let items = selected.value["items"].as_array().unwrap();
    assert!(items.iter().any(|i| i["name"] == "Tea" && i["selected"] == false));
    assert!(!read_text(&mut sheet, "E1:F4").contains("Tea"), "the slicer hid Tea");
    run(&mut sheet, "slicer select Items all");
    assert!(read_text(&mut sheet, "E1:F4").contains("Tea"));
    run(&mut sheet, "pivot items Item hide Cake in Sales");
    assert!(!read_text(&mut sheet, "E1:F4").contains("Cake"));
    run(&mut sheet, "pivot items Item all in Sales");
    run(&mut sheet, "pivot filter Item contains ea in Sales");
    assert!(!read_text(&mut sheet, "E1:F4").contains("Cake"));
    run(&mut sheet, "pivot filter Item clear in Sales");
    run(&mut sheet, "write C2 100");
    run(&mut sheet, "pivot refresh in Sales");
    assert!(
        read_text(&mut sheet, "E1:F6").contains("109"),
        "the refreshed pivot sees the new number"
    );
    run(&mut sheet, "pivot refresh-all");
    assert_eq!(run(&mut sheet, "slicer list").value.as_array().unwrap().len(), 1);
    run(&mut sheet, "slicer delete Items");
    assert_eq!(run(&mut sheet, "slicer list").value.as_array().unwrap().len(), 0);
}

/// Every value in a range, as one string.
fn read_text(sheet: &mut Spreadsheet, range: &str) -> String {
    run(sheet, &format!("read {range} values")).value["rows"].to_string()
}

#[test]
fn clicking_a_pivot_cell_in_a_formula_gives_getpivotdata_and_it_reads_the_same_figure() {
    let (mut sheet, _folder) = with_sales("pivot-getdata");
    run(&mut sheet, "pivot create A1:C6 at E1 name Sales");
    run(&mut sheet, "pivot add Region rows in Sales");
    run(&mut sheet, "pivot add Units values sum in Sales");
    let found = run(&mut sheet, "find East book").value;
    let at = found[0].as_str().unwrap().split('!').nth(1).unwrap().to_owned();
    let row: u32 = at[1..].parse().unwrap();
    let figure = format!("F{row}");
    let formula = run(&mut sheet, &format!("getpivotdata {figure}")).value["formula"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(formula.starts_with("GETPIVOTDATA(\"Sum of Units\",$E$1,"), "{formula}");
    assert!(formula.contains("\"Region\",\"East\""), "{formula}");
    run(&mut sheet, &format!("formula J1 ={formula}"));
    assert_eq!(number(&mut sheet, "J1"), number(&mut sheet, &figure));
    assert_eq!(number(&mut sheet, "J1"), 10.0);
}

#[test]
fn pivot_groups_calculated_fields_and_timelines_are_refused_or_made_in_the_engine_words() {
    let (mut sheet, _folder) = opened("pivot-more");
    run(&mut sheet, "new dates.xlsx");
    run(
        &mut sheet,
        "write A1 [[\"Day\",\"Units\"],[\"2026-01-05\",1],[\"2026-02-10\",2],[\"2026-02-20\",3]]",
    );
    run(&mut sheet, "format B2:B4 number 0");
    run(&mut sheet, "pivot create A1:B4 at D1 name P");
    run(&mut sheet, "pivot add Units values sum in P");
    run(&mut sheet, "pivot add Day rows in P");
    assert!(refused(&mut sheet, "timeline add Units in P").contains("no dates"));
    run(&mut sheet, "pivot calculated-field Double =Units*2 in P");
    assert!(run(&mut sheet, "pivots").value[0]["fields"].to_string().contains("Double"));
}

// ------------------------------------------------------------------------------------------------ files

#[test]
fn save_as_writes_each_format_by_its_extension() {
    let (mut sheet, folder) = with_columns("save-as");
    run(&mut sheet, "save-as plain.csv plain");
    assert_eq!(std::fs::read(folder.join("plain.csv")).unwrap()[..3], *b"1,1");
    run(&mut sheet, "save-as text.txt");
    let bytes = std::fs::read(folder.join("text.txt")).unwrap();
    assert_eq!(&bytes[..2], &[0xFF, 0xFE], "Unicode text is UTF-16");
    run(&mut sheet, "save-as sheet.ods");
    assert!(std::fs::read(folder.join("sheet.ods")).unwrap().starts_with(b"PK"));
    run(&mut sheet, "save-as back.xlsx");
    run(&mut sheet, "save-as page.html");
    assert!(std::fs::read_to_string(folder.join("page.html")).unwrap().contains("<table"));
    run(&mut sheet, "save-as print.pdf");
    assert!(std::fs::read(folder.join("print.pdf")).unwrap().starts_with(b"%PDF"));
    assert!(sheet.current_book().unwrap().path.ends_with("back.xlsx"), "html and pdf are copies");
    run(&mut sheet, "export csv copy.csv");
    assert!(folder.join("copy.csv").exists());
    assert!(refused(&mut sheet, "save-as data.json").contains(".json"));
}

#[test]
fn a_workbook_in_a_legacy_format_must_be_saved_as_xlsx() {
    let (mut sheet, folder) = with_columns("legacy");
    let book = sheet.current_book().unwrap();
    book.kind = unluminous_sheet::FileKind::Xls;
    book.path = folder.join("old.xls");
    let message = refused(&mut sheet, "save");
    assert!(message.contains("old.xlsx") && message.contains("save-as"), "{message}");
    run(&mut sheet, "save-as-xlsx");
    assert!(folder.join("old.xlsx").exists());
    let book = sheet.current_book().unwrap();
    assert_eq!(book.kind, unluminous_sheet::FileKind::Xlsx);
    run(&mut sheet, "save");
}

#[test]
fn other_formats_open_in_a_tab() {
    let (mut sheet, folder) = opened("open-formats");
    std::fs::write(folder.join("data.json"), "[{\"a\":1,\"b\":\"x\"},{\"a\":2,\"b\":\"y\"}]")
        .unwrap();
    run(&mut sheet, "open data.json");
    assert_eq!(number(&mut sheet, "A2"), 1.0);
    std::fs::write(folder.join("cols.prn"), "Name    Qty\nTea       4\n").unwrap();
    run(&mut sheet, "open cols.prn");
    assert_eq!(number(&mut sheet, "B2"), 4.0);
    std::fs::write(folder.join("tab.txt"), "a\tb\n1\t2\n").unwrap();
    run(&mut sheet, "open tab.txt");
    assert_eq!(number(&mut sheet, "B2"), 2.0);
}

#[test]
fn links_charts_and_sparklines_are_read_back() {
    let (mut sheet, _folder) = with_columns("readback");
    run(&mut sheet, "link D1 https://example.com Example");
    let links = run(&mut sheet, "links").value;
    assert_eq!(links[0]["target"], json!("https://example.com"));
    run(&mut sheet, "chart column A1:B3 Sizes");
    let charts = run(&mut sheet, "charts").value;
    assert_eq!(charts[0]["title"], json!("Sizes"));
    assert_eq!(charts[0]["kind"], json!("column"));
    assert!(run(&mut sheet, "sparklines").value.as_array().unwrap().is_empty());
    let xml = "<extLst><ext><x14:sparklineGroups><x14:sparklineGroup type=\"column\"><x14:sparklines><x14:sparkline><xm:f>Data!B2:E2</xm:f><xm:sqref>F2</xm:sqref></x14:sparkline></x14:sparklines></x14:sparklineGroup></x14:sparklineGroups></ext></extLst>";
    let found = verbs_b_files::sparklines_in("Data", xml);
    assert_eq!(found[0]["kind"], json!("column"));
    assert_eq!(found[0]["source"], json!("Data!B2:E2"));
    assert_eq!(found[0]["location"], json!("F2"));
}

#[test]
fn header_and_footer_sections_split_and_join() {
    let parts = verbs_b_page::split_sections("&LLeft&CMiddle &&&P&RRight");
    assert_eq!(parts, ["Left".to_owned(), "Middle &&&P".to_owned(), "Right".to_owned()]);
    assert_eq!(verbs_b_page::join_sections(&parts), "&LLeft&CMiddle &&&P&RRight");
}

// ------------------------------------------------------------------------------------------------ drawing

/// Draw one frame of the tab with no window behind it, as `a_tab_draws_a_frame_without_a_window` does.
fn draw_a_frame(sheet: &mut Spreadsheet) {
    let key = sheet.current.clone().unwrap();
    let settings = crate::settings::Settings::default();
    let renderer = crate::services::text_renderer::TextRenderer::new();
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1400.0, 900.0),
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

#[test]
fn every_popup_of_this_lane_draws_a_frame() {
    let (mut sheet, _folder) = with_sales("frames");
    run(&mut sheet, "pivot create A1:C6 at E1 name Sales");
    run(&mut sheet, "pivot add Region rows in Sales");
    run(&mut sheet, "pivot add Units values sum in Sales");
    run(&mut sheet, "slicer add Region in Sales at J1");
    run(&mut sheet, "timeline list");
    run(&mut sheet, "copy A1:A3");
    run(&mut sheet, "paste-special A8 transpose");
    for setup in [
        "paste-special",
        "view page-break",
        "view layout",
        "print-preview open",
        "pivot pane on",
        "select E2",
    ] {
        run(&mut sheet, setup);
        draw_a_frame(&mut sheet);
    }
    run(&mut sheet, "select A1:A2");
    run(&mut sheet, "fill-handle A1:A5 copy");
    assert!(
        sheet.current_book().unwrap().view.lane_b.after.is_some(),
        "Auto Fill Options is offered"
    );
    draw_a_frame(&mut sheet);
}
