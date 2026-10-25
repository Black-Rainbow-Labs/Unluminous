//! The data tools, outline, rules, what if, protection, style and review verbs driven the way an agent
//! drives them, each checked by reading the workbook back.

use serde_json::json;

use super::*;

fn opened(name: &str) -> Spreadsheet {
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-a-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut sheet = Spreadsheet::new();
    sheet.open(&Context { project: Some(folder), ..Context::default() }).unwrap();
    go(&mut sheet, "new a.xlsx");
    sheet
}

/// Words with a `~` standing for a space, so a value with a space in it is one word.
fn words(line: &str) -> Vec<String> {
    line.split(' ').map(|w| w.replace('~', " ")).collect()
}

fn go(sheet: &mut Spreadsheet, line: &str) -> Answer {
    let w = words(line);
    sheet.command(&w[0], &w[1..]).unwrap_or_else(|e| panic!("`{line}` failed: {e}"))
}

fn refused(sheet: &mut Spreadsheet, line: &str) -> String {
    let w = words(line);
    match sheet.command(&w[0], &w[1..]) {
        Ok(_) => panic!("`{line}` should have been refused"),
        Err(e) => e,
    }
}

/// A sheet with a small table of items, prices and counts.
fn with_items(name: &str) -> Spreadsheet {
    let mut sheet = opened(name);
    go(&mut sheet, "write A1 [[\"Item\",\"Price\",\"Count\"],[\"Tea\",3.5,4],[\"Cake\",4.25,2],[\"Scone\",2,6],[\"Tea\",3.5,1]]");
    sheet
}

fn column(sheet: &mut Spreadsheet, range: &str) -> Vec<serde_json::Value> {
    let answer = go(sheet, &format!("read {range}"));
    answer.value["rows"].as_array().unwrap().iter().map(|row| row[0].clone()).collect()
}

fn hidden_rows(sheet: &mut Spreadsheet) -> Vec<u32> {
    let book = sheet.current_book().unwrap();
    let s = &book.session.book.sheets[book.view.sheet];
    s.rows.iter().filter(|(_, r)| r.hidden).map(|(i, _)| *i).collect()
}

#[test]
fn sorting_by_levels_orders_rows_by_each_level_in_turn() {
    let mut sheet = with_items("sort");
    go(&mut sheet, "sort-levels A1:C5 header B:desc");
    assert_eq!(column(&mut sheet, "B2:B5"), vec![json!(4.25), json!(3.5), json!(3.5), json!(2.0)]);
    go(&mut sheet, "sort-levels A1:C5 header A C:desc");
    assert_eq!(column(&mut sheet, "C2:C5"), vec![json!(2.0), json!(6.0), json!(4.0), json!(1.0)]);
    assert!(refused(&mut sheet, "sort-levels A1:C5 header").contains("needs a level"));
}

#[test]
fn sorting_by_a_colour_puts_those_cells_first() {
    let mut sheet = with_items("sort-colour");
    go(&mut sheet, "format A4 fill #FF0000");
    go(&mut sheet, "sort-levels A1:C5 header A:asc:cell-color=#FF0000");
    assert_eq!(column(&mut sheet, "A2:A2"), vec![json!("Scone")]);
}

#[test]
fn the_sort_popup_adds_copies_moves_and_applies_levels() {
    let mut sheet = with_items("draft");
    go(&mut sheet, "select A1:C5");
    go(&mut sheet, "sort-draft add C");
    go(&mut sheet, "sort-draft set 2 order desc");
    go(&mut sheet, "sort-draft copy 2");
    go(&mut sheet, "sort-draft up 3");
    go(&mut sheet, "sort-draft delete 3");
    let shown = go(&mut sheet, "sort-draft show");
    assert_eq!(shown.value["levels"].as_array().unwrap().len(), 2);
    assert_eq!(shown.value["levels"][1]["order"], json!("descending"));
    go(&mut sheet, "sort-draft options header");
    go(&mut sheet, "sort-draft apply");
    assert_eq!(
        column(&mut sheet, "A2:A5"),
        vec![json!("Cake"), json!("Scone"), json!("Tea"), json!("Tea")]
    );
    assert_eq!(column(&mut sheet, "C4:C5"), vec![json!(4.0), json!(1.0)]);
}

#[test]
fn filtering_by_each_kind_hides_the_rows_that_fail() {
    let mut sheet = with_items("filter");
    go(&mut sheet, "filter-column B values 3.5,2");
    assert_eq!(hidden_rows(&mut sheet), vec![2]);
    go(&mut sheet, "filter-clear");
    assert!(hidden_rows(&mut sheet).is_empty());
    go(&mut sheet, "filter-column A contains ea");
    assert_eq!(hidden_rows(&mut sheet), vec![2, 3]);
    go(&mut sheet, "filter-column A clear");
    go(&mut sheet, "filter-column B number > 3 and < 4");
    assert_eq!(hidden_rows(&mut sheet), vec![2, 3]);
    go(&mut sheet, "filter-column B clear");
    go(&mut sheet, "filter-column C top 2");
    assert_eq!(hidden_rows(&mut sheet), vec![2, 4]);
    go(&mut sheet, "filter-column C clear");
    go(&mut sheet, "filter-column C above-average");
    assert_eq!(hidden_rows(&mut sheet), vec![2, 4]);
    go(&mut sheet, "filter-column C clear");
    go(&mut sheet, "format A3 fill #00FF00");
    go(&mut sheet, "filter-column A color cell #00FF00");
    assert_eq!(hidden_rows(&mut sheet), vec![1, 3, 4]);
    go(&mut sheet, "filter-column A clear");
    go(&mut sheet, "write C2 9");
    go(&mut sheet, "filter-column C between 5 10");
    go(&mut sheet, "filter-reapply");
    assert_eq!(hidden_rows(&mut sheet).len(), 2);
}

#[test]
fn an_advanced_filter_keeps_the_rows_a_criteria_range_admits() {
    let mut sheet = with_items("advanced");
    go(&mut sheet, "write E1 [[\"Item\"],[\"Tea\"]]");
    go(&mut sheet, "advanced-filter A1:C5 criteria E1:E2");
    assert_eq!(hidden_rows(&mut sheet), vec![2, 3]);
    go(&mut sheet, "advanced-filter show-all");
    assert!(hidden_rows(&mut sheet).is_empty());
    go(&mut sheet, "advanced-filter A1:C5 criteria E1:E2 copy-to G1 unique");
    assert_eq!(column(&mut sheet, "G1:G2"), vec![json!("Item"), json!("Tea")]);
}

#[test]
fn remove_duplicates_and_text_to_columns_take_their_options() {
    let mut sheet = with_items("dedupe");
    let answer = go(&mut sheet, "remove-duplicates A1:C5 columns A,B header");
    assert!(answer.message.starts_with("1 duplicate"));
    go(&mut sheet, "write E1 [[\"p-10-q\"],[\"ab12cd\"]]");
    go(&mut sheet, "text-to-columns E1:E1 delimited other -");
    assert_eq!(go(&mut sheet, "cell F1").value["value"], json!(10.0));
    assert_eq!(go(&mut sheet, "cell G1").value["value"], json!("q"));
    go(&mut sheet, "text-to-columns E2:E2 fixed 2,4 formats text,text,skip");
    assert_eq!(go(&mut sheet, "cell E2").value["value"], json!("ab"));
    assert_eq!(go(&mut sheet, "cell F2").value["value"], json!("12"));
}

#[test]
fn consolidate_sums_by_label() {
    let mut sheet = opened("consolidate");
    go(&mut sheet, "write A1 [[\"a\",1],[\"b\",2]]");
    go(&mut sheet, "write D1 [[\"a\",10],[\"c\",5]]");
    go(&mut sheet, "consolidate G1 sum A1:B2 D1:E2 left-column");
    let labels = column(&mut sheet, "G1:G3");
    assert_eq!(labels.len(), 3);
    assert_eq!(go(&mut sheet, "cell H1").value["value"], json!(11.0));
}

#[test]
fn table_commands_resize_total_and_rename() {
    let mut sheet = with_items("tables");
    go(&mut sheet, "table A1:C5 Sales");
    go(&mut sheet, "table totals Sales on");
    go(&mut sheet, "table total Sales Count sum");
    let listed = go(&mut sheet, "table list");
    assert_eq!(listed.value[0]["totals"], json!(true));
    go(&mut sheet, "table rename Sales Orders");
    go(&mut sheet, "table style Orders style TableStyleMedium9 column-stripes on");
    go(&mut sheet, "table column Orders add Extra");
    assert_eq!(go(&mut sheet, "table list").value[0]["columns"].as_array().unwrap().len(), 4);
    go(&mut sheet, "table to-range Orders");
    assert_eq!(go(&mut sheet, "table list").value, json!([]));
}

#[test]
fn grouping_builds_and_folds_an_outline() {
    let mut sheet = with_items("outline");
    go(&mut sheet, "group rows 2:3");
    let read = go(&mut sheet, "outline").value;
    assert_eq!(read["rowLevels"], json!(1));
    go(&mut sheet, "outline hide rows 4");
    assert_eq!(hidden_rows(&mut sheet), vec![1, 2]);
    go(&mut sheet, "outline show rows 4");
    assert!(hidden_rows(&mut sheet).is_empty());
    go(&mut sheet, "outline level 1");
    assert_eq!(hidden_rows(&mut sheet), vec![1, 2]);
    go(&mut sheet, "ungroup rows 2:3");
    go(&mut sheet, "group columns B:C");
    assert_eq!(go(&mut sheet, "outline").value["columnLevels"], json!(1));
    go(&mut sheet, "outline clear");
    assert_eq!(go(&mut sheet, "outline").value["columnLevels"], json!(0));
    assert!(refused(&mut sheet, "ungroup rows 2:3").len() > 3);
}

#[test]
fn subtotal_inserts_rows_and_remove_takes_them_away() {
    let mut sheet = with_items("subtotal");
    go(&mut sheet, "sort-levels A1:C5 header A");
    go(&mut sheet, "subtotal A1:C5 at A function sum add C");
    let items = column(&mut sheet, "A1:A9");
    assert!(items.contains(&json!("Tea Total")) && items.contains(&json!("Grand Total")));
    go(&mut sheet, "subtotal remove A1:C9");
    assert_eq!(column(&mut sheet, "A1:A5").len(), 5);
}

#[test]
fn rules_are_added_ordered_edited_and_read_back() {
    let mut sheet = with_items("rules");
    go(&mut sheet, "rules add B2:B5 greater 3 fill #FF0000");
    go(&mut sheet, "rules add C2:C5 icons 3Arrows reverse");
    go(&mut sheet, "rules add B2:B5 bar #00FF00 border #006600 solid");
    let read = go(&mut sheet, "rules").value;
    assert_eq!(read.as_array().unwrap().len(), 3);
    assert_eq!(read[0]["dataBar"], json!("#00FF00"));
    assert_eq!(read[1]["iconSet"]["name"], json!("3Arrows"));
    let last = read[2]["priority"].as_i64().unwrap();
    go(&mut sheet, &format!("rules move {last} first"));
    assert_eq!(go(&mut sheet, "rules").value[0]["kind"], json!("cellIs"));
    go(&mut sheet, "rules stop 1 on");
    go(&mut sheet, "rules applies 1 B2:B3");
    assert_eq!(go(&mut sheet, "rules B5").value.as_array().unwrap().len(), 1);
    go(&mut sheet, "rules delete 1");
    assert_eq!(go(&mut sheet, "rules").value.as_array().unwrap().len(), 2);
    go(&mut sheet, "conditional C2:C5 stddev 1 above");
    assert_eq!(go(&mut sheet, "rules").value.as_array().unwrap().len(), 3);
}

#[test]
fn goal_seek_data_tables_and_scenarios() {
    let mut sheet = opened("whatif");
    go(&mut sheet, "write A1 [[5],[0]]");
    go(&mut sheet, "formula B1 =A1*2");
    let found = go(&mut sheet, "goal-seek B1 40 A1");
    assert_eq!(found.value["found"], json!(true));
    assert!((go(&mut sheet, "cell A1").value["value"].as_f64().unwrap() - 20.0).abs() < 0.01);
    go(&mut sheet, "write D1 [[\"\",1,2],[3,0,0],[4,0,0]]");
    go(&mut sheet, "formula D1 =A1*2");
    go(&mut sheet, "data-table D1:F3 row A1");
    go(&mut sheet, "data-table remove E2");
    go(&mut sheet, "scenario add Low A1 values 1 comment~a~low~case");
    go(&mut sheet, "scenario add High A1 values 100");
    go(&mut sheet, "scenario show Low");
    assert_eq!(go(&mut sheet, "cell A1").value["value"], json!(1.0));
    assert_eq!(go(&mut sheet, "scenario list").value.as_array().unwrap().len(), 2);
    go(&mut sheet, "scenario delete High");
    go(&mut sheet, "scenario summary B1");
    assert!(go(&mut sheet, "sheets").value.as_array().unwrap().len() >= 2);
}

#[test]
fn protecting_a_sheet_refuses_edits_until_the_password_is_given() {
    let mut sheet = with_items("protect");
    go(&mut sheet, "protect sheet password secret allow format-cells,sort");
    let read = go(&mut sheet, "protection").value;
    assert_eq!(read["sheet"]["protected"], json!(true));
    assert_eq!(read["sheet"]["allow"]["formatCells"], json!(true));
    assert_eq!(read["sheet"]["password"], json!(true));
    assert!(refused(&mut sheet, "write A1 changed").contains("protected"));
    assert!(refused(&mut sheet, "unprotect password wrong").contains("password"));
    go(&mut sheet, "unprotect password secret");
    go(&mut sheet, "write A1 changed");
    go(&mut sheet, "edit-range add Open A2:A3");
    assert_eq!(go(&mut sheet, "edit-range list").value[0]["title"], json!("Open"));
    go(&mut sheet, "protect book structure");
    assert_eq!(go(&mut sheet, "protection").value["workbook"]["structure"], json!(true));
    assert!(refused(&mut sheet, "sheet add Extra").contains("protected"));
    go(&mut sheet, "unprotect book");
}

#[test]
fn styles_and_themes_are_applied_by_name() {
    let mut sheet = with_items("styles");
    go(&mut sheet, "select A1");
    go(&mut sheet, "style apply Heading~1");
    assert_eq!(go(&mut sheet, "styles").value["cell"], json!("Heading 1"));
    go(&mut sheet, "style new Mine from A1 include font,fill");
    go(&mut sheet, "style modify Mine fill #FFFF00 bold");
    assert!(go(&mut sheet, "styles").value["styles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["name"] == "Mine"));
    go(&mut sheet, "style delete Mine");
    assert_eq!(
        go(&mut sheet, "styles").value["styles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["builtIn"] == json!(true))
            .count()
            >= 47,
        true
    );
    go(&mut sheet, "theme Facet");
    assert!(go(&mut sheet, "theme").value["colors"]["slots"].as_array().unwrap().len() >= 12);
    go(&mut sheet, "theme colors set accent1=FF0000");
    go(&mut sheet, "theme fonts Georgia Arial");
    assert_eq!(go(&mut sheet, "theme").value["fonts"]["major"], json!("Georgia"));
    assert!(go(&mut sheet, "theme list").value["themes"].as_array().unwrap().len() >= 11);
}

#[test]
fn threaded_comments_and_notes() {
    let mut sheet = with_items("comments");
    go(&mut sheet, "comment new A2 First~thoughts");
    go(&mut sheet, "comment reply A2 Agreed");
    go(&mut sheet, "comment edit A2 1 Agreed~entirely");
    let read = go(&mut sheet, "comments A2").value;
    assert_eq!(read["threads"][0]["replies"], json!(1));
    go(&mut sheet, "comment resolve A2");
    assert_eq!(go(&mut sheet, "comments").value["threads"][0]["resolved"], json!(true));
    go(&mut sheet, "comment reopen A2");
    go(&mut sheet, "comment delete A2 1");
    go(&mut sheet, "comment delete A2");
    assert_eq!(go(&mut sheet, "comments").value["threads"], json!([]));
    go(&mut sheet, "note B2 a~note");
    go(&mut sheet, "note B2 show");
    assert_eq!(go(&mut sheet, "comments B2").value["notes"][0]["shown"], json!(true));
    go(&mut sheet, "note B2 box 100 50 200 80");
    go(&mut sheet, "note B2 hide");
    go(&mut sheet, "note next");
    go(&mut sheet, "note convert");
    assert_eq!(go(&mut sheet, "comments").value["threads"].as_array().unwrap().len(), 1);
}

#[test]
fn find_and_replace_take_every_option() {
    let mut sheet = with_items("find");
    go(&mut sheet, "formula E1 =B2*2");
    assert_eq!(go(&mut sheet, "find T*a").value.as_array().unwrap().len(), 2);
    assert_eq!(go(&mut sheet, "find tea whole case").value.as_array().unwrap().len(), 0);
    assert_eq!(go(&mut sheet, "find B2 formulas").value.as_array().unwrap().len(), 1);
    assert_eq!(
        go(&mut sheet, "find T* whole by-columns within A1:A3").value.as_array().unwrap().len(),
        1
    );
    let done = go(&mut sheet, "replace Tea Chai whole");
    assert_eq!(done.message, "2 replaced");
    assert_eq!(column(&mut sheet, "A2:A2"), vec![json!("Chai")]);
}

#[test]
fn go_to_special_chooses_the_cells_of_a_kind() {
    let mut sheet = with_items("special");
    go(&mut sheet, "formula E1 =B2*2");
    go(&mut sheet, "formula E3 =B3*2");
    let found = go(&mut sheet, "goto-special formulas").value;
    assert_eq!(found["cells"], json!(2));
    assert_eq!(go(&mut sheet, "selection").value["ranges"], json!(["E1", "E3"]));
    assert!(refused(&mut sheet, "goto-special notes").contains("No cells"));
    go(&mut sheet, "select A1");
    assert_eq!(go(&mut sheet, "goto-special region").value["areas"], json!(["A1:C5"]));
}

#[test]
fn tracing_checking_and_evaluating_a_formula() {
    let mut sheet = with_items("audit");
    go(&mut sheet, "formula E1 =B2+B3");
    go(&mut sheet, "formula F1 =E1*2");
    go(&mut sheet, "select F1");
    let traced = go(&mut sheet, "trace precedents all").value;
    assert_eq!(traced["added"], json!(3));
    go(&mut sheet, "trace clear");
    go(&mut sheet, "select B2");
    go(&mut sheet, "trace dependents");
    assert_eq!(sheet.current_book().unwrap().view.audit.arrows.len(), 1);
    go(&mut sheet, "formula G1 =1/0");
    go(&mut sheet, "select G1");
    let errors = go(&mut sheet, "error-check").value;
    assert_eq!(errors[0]["rule"], json!("eval-error"));
    go(&mut sheet, "error-check ignore G1 eval-error");
    assert_eq!(go(&mut sheet, "error-check").value, json!([]));
    go(&mut sheet, "error-check reset");
    let start = go(&mut sheet, "evaluate F1").value;
    let step = go(&mut sheet, "evaluate evaluate").value;
    assert_ne!(step["text"], start["text"]);
    go(&mut sheet, "evaluate finish");
    go(&mut sheet, "evaluate close");
    go(&mut sheet, "watch add F1");
    assert_eq!(go(&mut sheet, "watch list").value[0]["cell"], json!("F1"));
    go(&mut sheet, "watch remove F1");
    assert_eq!(go(&mut sheet, "statistics").value["formulas"], json!(3));
}

#[test]
fn the_read_back_verbs_answer_json() {
    let mut sheet = with_items("readback");
    go(&mut sheet, "validate B2:B5 whole > 0");
    assert_eq!(go(&mut sheet, "validation").value[0]["kind"], json!("whole"));
    assert_eq!(go(&mut sheet, "calc").value["mode"], json!("automatic"));
    assert_eq!(go(&mut sheet, "views").value["view"], json!("normal"));
    assert_eq!(go(&mut sheet, "protection").value["sheet"], json!(null));
    assert_eq!(go(&mut sheet, "outline").value["rowLevels"], json!(0));
}
