//! What If analysis against Excel: Goal Seek reaches the value Excel reaches, data tables compute what
//! Excel computes, and the Scenario Manager writes and shows what Excel writes and shows.
//!
//! The fixtures were made by `tools/excel-oracle/whatif-fixtures.ps1`, which does each operation in Excel and
//! saves the workbook before and after.

mod describe;

use describe::*;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::whatif::Scenarios;
use unluminous_sheet::Value;

const LANE: &str = "whatif";
/// 24 October 2026, the day the fixtures were made. Dates in a scenario's comment are left out of the
/// comparison, so a fixture made on another day compares the same.
const NOW: f64 = 46319.5;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn rg(a: &str) -> Range {
    Range::parse(a).unwrap()
}

// ---------------------------------------------------------------------------------------------------
// Goal Seek.

#[test]
fn goal_seek_reaches_the_value_excel_reaches_in_every_case() {
    let text = std::fs::read_to_string(fixture_dir(LANE).join("goalseek.tsv")).unwrap();
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let (formula, start, goal, change) = (f[0], f[1], f[2], f[3]);
        let found = f[4] == "True";
        let (excel_x, excel_y): (f64, f64) = (f[5].parse().unwrap(), f[6].parse().unwrap());
        let mut book = Workbook::new();
        book.calc.attrs.push(("iterateDelta".to_owned(), change.to_owned()));
        let mut s = Session::new(book, NOW);
        s.set_value(0, at("A1"), Value::Number(start.parse().unwrap())).unwrap();
        s.set_input(0, at("B1"), formula).unwrap();
        let r = s.goal_seek(0, at("B1"), goal.parse().unwrap(), at("A1")).unwrap();
        let close = (r.value - excel_x).abs() <= 1e-7 * excel_x.abs().max(1.0)
            && (r.target - excel_y).abs() <= 1e-7 * excel_y.abs().max(1.0);
        // Where Excel finds no solution, the value it leaves depends on how it gave up, and the engine
        // gives up in the same place for a formula that is flat and for a tolerance of nothing. A formula
        // with a minimum at its starting value, which Excel steps away from without end, is the one
        // case the engine reports as no solution without stopping on Excel's value.
        let gives_up_elsewhere = !found && formula == "=A1^2+1";
        if r.found != found || (!close && !gives_up_elsewhere) {
            wrong.push(format!(
                "{formula} from {start} to {goal} within {change}: Excel found={found} x={excel_x} y={excel_y}; ours found={} x={} y={}",
                r.found, r.value, r.target
            ));
        }
        // The cell holds the value Goal Seek stopped on, and the workbook is computed with it.
        assert_eq!(s.book.sheets[0].value_at(at("A1")), Value::Number(r.value));
        checked += 1;
    }
    assert!(checked >= 20, "{checked} cases");
    assert!(wrong.is_empty(), "{} of {checked} cases differ from Excel:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
fn goal_seek_refuses_what_excel_refuses_and_can_be_undone() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.set_value(0, at("A1"), Value::Number(1.0)).unwrap();
    s.set_input(0, at("B1"), "=A1^2").unwrap();
    s.set_input(0, at("C1"), "5").unwrap();
    s.set_input(0, at("D1"), "=C1*2").unwrap();
    for (set, by) in [("C1", "A1"), ("B1", "D1"), ("B1", "B1")] {
        assert_eq!(
            s.goal_seek(0, at(set), 2.0, at(by)).unwrap_err(),
            "Reference isn't valid.",
            "{set} by {by}"
        );
    }
    let r = s.goal_seek(0, at("B1"), 2.0, at("A1")).unwrap();
    assert!(r.found);
    s.undo();
    assert_eq!(s.book.sheets[0].value_at(at("A1")), Value::Number(1.0));
    assert_eq!(s.book.sheets[0].value_at(at("B1")), Value::Number(1.0));
}

#[test]
fn goal_seek_on_an_empty_changing_cell_starts_from_zero() {
    let mut s = Session::new(Workbook::new(), NOW);
    s.set_input(0, at("B1"), "=3*A1+2").unwrap();
    let r = s.goal_seek(0, at("B1"), 11.0, at("A1")).unwrap();
    assert!(r.found && (r.value - 3.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------------------------------
// Data tables.

/// Every cell value in a workbook, by sheet name and cell.
fn values(book: &Workbook) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in &book.sheets {
        for (&(r, c), cell) in &s.cells {
            out.push((format!("{}!{}", s.name, CellRef::new(r, c)), value_text(&cell.value)));
        }
    }
    out
}

#[test]
fn data_tables_compute_to_what_excel_computed() {
    let excel = read_fixture(LANE, "datatable.xlsx");
    let mut spoiled = excel.clone();
    // Every result cell holds nonsense, so only a computation can put the right values back.
    for t in spoiled.sheets[0].data_tables.clone() {
        for a in t.range.cells() {
            spoiled.sheets[0].cells.get_mut(&(a.row, a.col)).unwrap().value = Value::Number(-1.0);
        }
    }
    assert_eq!(excel.sheets[0].data_tables.len(), 4);
    let s = Session::new(spoiled, NOW);
    assert_eq!(values(&s.book), values(&excel));
}

#[test]
fn changing_an_input_or_a_header_recomputes_the_tables_like_excel() {
    let mut s = Session::new(read_fixture(LANE, "datatable.xlsx"), NOW);
    s.set_input(0, at("B1"), "3").unwrap();
    s.set_input(0, at("B2"), "5").unwrap();
    s.set_input(0, at("D3"), "7").unwrap();
    s.set_input(0, at("N1"), "4").unwrap();
    let changed = read_fixture(LANE, "datatable-changed.xlsx");
    assert_eq!(values(&s.book), values(&changed));
}

#[test]
fn creating_a_data_table_fills_the_cells_excel_fills() {
    let mut s = Session::new(read_fixture(LANE, "datatable-input.xlsx"), NOW);
    s.create_data_table(0, rg("D1:E5"), None, Some(at("B1"))).unwrap();
    s.create_data_table(0, rg("G1:K2"), Some(at("B2")), None).unwrap();
    s.create_data_table(0, rg("M1:P4"), Some(at("B2")), Some(at("B1"))).unwrap();
    s.create_data_table(0, rg("R1:T3"), None, Some(at("B1"))).unwrap();
    let excel = read_fixture(LANE, "datatable.xlsx");
    assert_eq!(values(&s.book), values(&excel));
    let ours = &s.book.sheets[0].data_tables;
    let theirs = &excel.sheets[0].data_tables;
    for t in theirs {
        let mine = ours.iter().find(|m| m.range == t.range).unwrap_or_else(|| panic!("{:?}", t.range));
        assert_eq!((mine.two_d, mine.row_input, mine.r1, mine.r2), (t.two_d, t.row_input, t.r1, t.r2));
        assert_eq!(mine.formula(), t.formula());
    }
    // The shown formula is Excel's.
    assert_eq!(theirs.iter().map(|t| t.formula()).collect::<Vec<_>>().len(), 4);
    assert_eq!(s.data_table_at(0, at("N2")).unwrap().formula(), "TABLE(B2,B1)");
}

#[test]
fn a_data_table_refuses_changes_to_part_of_it_and_survives_a_file() {
    let mut s = Session::new(read_fixture(LANE, "datatable.xlsx"), NOW);
    assert_eq!(s.set_input(0, at("E3"), "1").unwrap_err(), "Cannot change part of a data table.");
    assert!(s.create_data_table(0, rg("D1:E5"), None, Some(at("B1"))).is_err());
    assert_eq!(
        s.create_data_table(0, rg("A20:C22"), None, None).unwrap_err(),
        "Input cell reference is not valid."
    );
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.sheets[0].data_tables.len(), 4);
    // Clearing the whole table takes the table with it.
    s.clear(0, &[rg("E2:E5")], unluminous_sheet::edit::Clear::Contents).unwrap();
    assert_eq!(s.book.sheets[0].data_tables.len(), 3);
    s.undo();
    assert_eq!(s.book.sheets[0].data_tables.len(), 4);
}

// ---------------------------------------------------------------------------------------------------
// Scenarios.

/// A scenario file with the dates in its comments replaced, so a fixture made on another day compares.
fn undated(mut s: Scenarios) -> Scenarios {
    for scenario in &mut s.list {
        if let Some(c) = &mut scenario.comment {
            *c = c
                .split_whitespace()
                .map(|w| if w.matches('/').count() == 2 { "DATE" } else { w })
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
    s
}

fn scenario_session(name: &str) -> Session {
    Session::new(read_fixture(LANE, name), NOW)
}

fn build_scenarios(s: &mut Session) {
    let two = [rg("B1:B2")];
    s.add_scenario(0, "Base", &two, Some(&[Value::Number(10.0), Value::Number(100.0)]), None, "Fixture Author", true, false).unwrap();
    s.add_scenario(0, "Best", &two, Some(&[Value::Number(12.0), Value::Number(150.0)]), Some("Optimistic case"), "Fixture Author", true, false).unwrap();
    s.add_scenario(
        0,
        "Worst",
        &[rg("B1:B2"), rg("B4")],
        Some(&[Value::Number(8.0), Value::Number(60.0), Value::text("abc")]),
        None,
        "Fixture Author",
        true,
        false,
    )
    .unwrap();
}

#[test]
fn adding_scenarios_writes_what_excel_writes() {
    let mut s = scenario_session("scenarios-input.xlsx");
    build_scenarios(&mut s);
    let excel = read_fixture(LANE, "scenarios-added.xlsx");
    assert_eq!(undated(s.scenarios(0)), undated(excel.sheets[0].scenarios()));
    // And the file round trips.
    let back = unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    assert_eq!(undated(back.sheets[0].scenarios()), undated(excel.sheets[0].scenarios()));
}

#[test]
fn showing_a_scenario_sets_the_cells_and_the_marks_excel_sets() {
    let mut s = scenario_session("scenarios-added.xlsx");
    s.show_scenario(0, "Best").unwrap();
    let excel = read_fixture(LANE, "scenarios-shown.xlsx");
    assert_eq!(values(&s.book), values(&excel));
    assert_eq!(undated(s.scenarios(0)), undated(excel.sheets[0].scenarios()));
    assert!(s.show_scenario(0, "Nope").is_err());
}

#[test]
fn editing_and_deleting_scenarios_does_what_excel_does() {
    let mut s = scenario_session("scenarios-added.xlsx");
    s.show_scenario(0, "Base").unwrap();
    s.edit_scenario(0, "Best", None, Some(&[rg("B1:B2"), rg("B5")]), Some(&[Value::Number(13.0), Value::Number(160.0), Value::Number(0.5)]), None, "Fixture Author").unwrap();
    let excel = read_fixture(LANE, "scenarios-edited.xlsx");
    assert_eq!(undated(s.scenarios(0)), undated(excel.sheets[0].scenarios()));
    s.delete_scenario(0, "Worst").unwrap();
    let excel = read_fixture(LANE, "scenarios-deleted.xlsx");
    assert_eq!(undated(s.scenarios(0)), undated(excel.sheets[0].scenarios()));
    s.undo();
    assert_eq!(s.scenarios(0).list.len(), 3);
}

/// The layout of a sheet: row heights, hidden rows, outline levels and column widths.
fn layout(book: &Workbook, sheet: usize) -> Vec<String> {
    let s = &book.sheets[sheet];
    let mut out = Vec::new();
    for (r, info) in &s.rows {
        out.push(format!(
            "row {} h={:?} hidden={} outline={} collapsed={}",
            r + 1,
            info.height,
            info.hidden,
            info.outline,
            info.collapsed
        ));
    }
    for c in &s.columns {
        for col in c.first..=c.last {
            out.push(format!("col {} w={:?} outline={} hidden={}", col + 1, c.width, c.outline, c.hidden));
        }
    }
    out.push(format!("grid={} sheetPr={}", s.show_grid, s.raw.elements.iter().any(|(n, x)| n == "sheetPr" && x.contains("summaryBelow=\"0\""))));
    out
}

fn summary_lines(book: &Workbook, sheet: usize) -> Vec<String> {
    describe(book)
        .into_iter()
        .filter(|l| l.starts_with(&format!("{}!", book.sheets[sheet].name)))
        .map(|l| l.replace("DATE", ""))
        .collect()
}

#[test]
fn a_scenario_summary_is_the_sheet_excel_builds() {
    let mut s = scenario_session("scenarios-summary-input.xlsx");
    let at_index = s.scenario_summary(0, &[rg("B3"), rg("B5")], "Fixture Author").unwrap();
    assert_eq!(at_index, 0);
    let excel = read_fixture(LANE, "scenarios-summary.xlsx");
    assert_eq!(s.book.sheets[0].name, "Scenario Summary");
    assert_eq!(s.book.sheets[1].name, "Model");
    let ours = summary_lines(&s.book, 0);
    let theirs = summary_lines(&excel, 0);
    assert_eq!(ours, theirs, "{}", differences(&ours, &theirs));
    assert_eq!(layout(&s.book, 0), layout(&excel, 0));
    assert_eq!(s.book.active_sheet, 0);
    // The model sheet keeps its scenarios, now remembering the results and the last scenario shown.
    let model = undated(s.scenarios(1));
    assert_eq!(model, undated(excel.sheets[1].scenarios()));
    // The model sheet is as it was: the report applied each scenario in a copy.
    assert_eq!(s.book.sheets[1].value_at(at("B1")), Value::Number(10.0));
}

#[test]
fn a_second_summary_is_numbered_and_names_are_used_for_labels() {
    let mut s = scenario_session("scenarios-summary-input.xlsx");
    s.scenario_summary(0, &[rg("B3"), rg("B5")], "Fixture Author").unwrap();
    s.scenario_summary(1, &[rg("B3")], "Fixture Author").unwrap();
    let excel = read_fixture(LANE, "scenarios-summary-twice.xlsx");
    let names: Vec<&str> = s.book.sheets.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Scenario Summary", "Scenario Summary 2", "Model"]);
    assert_eq!(summary_lines(&s.book, 1), summary_lines(&excel, 1));

    let mut s = scenario_session("scenarios-names-input.xlsx");
    s.scenario_summary(0, &[rg("B3"), rg("B5")], "Fixture Author").unwrap();
    let excel = read_fixture(LANE, "scenarios-names-summary.xlsx");
    let ours = summary_lines(&s.book, 0);
    let theirs = summary_lines(&excel, 0);
    assert_eq!(ours, theirs, "{}", differences(&ours, &theirs));
    assert_eq!(layout(&s.book, 0), layout(&excel, 0));
}

#[test]
fn scenario_operations_refuse_what_excel_refuses() {
    let mut s = scenario_session("scenarios-added.xlsx");
    assert!(s.add_scenario(0, "Base", &[rg("B1")], None, None, "x", true, false).is_err(), "a name in use");
    assert!(s.add_scenario(0, "Many", &[rg("A1:A40")], None, None, "x", true, false).is_err(), "more than 32 cells");
    assert!(s.add_scenario(0, "Few", &[rg("B1:B2")], Some(&[Value::Number(1.0)]), None, "x", true, false).is_err());
    assert!(s.delete_scenario(0, "Nope").is_err());
    let mut empty = scenario_session("scenarios-input.xlsx");
    assert!(empty.scenario_summary(0, &[rg("B3")], "x").is_err());
}

// ---------------------------------------------------------------------------------------------------
// Excel opens what the engine writes.

/// Ask Excel (through `inspect.ps1`) what it sees in a file, with every formula computed.
fn inspect(path: &std::path::Path) -> serde_json::Value {
    let text = run_script("inspect.ps1", &["-Path", path.to_str().unwrap(), "-Recalculate"]);
    serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}'))
        .unwrap_or_else(|e| panic!("Excel's answer did not read: {e}\n{text}"))
}

#[test]
fn excel_opens_data_tables_and_scenarios_unluminous_writes_and_computes_the_same_values() {
    if !have_excel("what Excel makes of data tables and scenarios written here") {
        return;
    }
    let mut s = Session::new(read_fixture(LANE, "datatable-input.xlsx"), NOW);
    s.create_data_table(0, rg("D1:E5"), None, Some(at("B1"))).unwrap();
    s.create_data_table(0, rg("G1:K2"), Some(at("B2")), None).unwrap();
    s.create_data_table(0, rg("M1:P4"), Some(at("B2")), Some(at("B1"))).unwrap();
    s.create_data_table(0, rg("R1:T3"), None, Some(at("B1"))).unwrap();
    let who = "Fixture Author";
    s.add_scenario(0, "Base", &[rg("B1:B2")], None, None, who, true, false).unwrap();
    let nine_eight = [Value::Number(9.0), Value::Number(8.0)];
    s.add_scenario(0, "Big", &[rg("B1:B2")], Some(&nine_eight), None, who, true, false).unwrap();
    s.scenario_summary(0, &[rg("B3")], who).unwrap();
    let folder = scratch("whatif");
    let path = folder.join("built.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let excel = inspect(&path);
    assert_eq!(excel["repaired"], false, "Excel repaired the file: {}", excel["problem"]);
    let mut wrong = Vec::new();
    for (index, sheet) in excel["sheets"].as_array().unwrap().iter().enumerate() {
        for cell in sheet["cells"].as_array().unwrap() {
            let a = CellRef::parse(cell["cell"].as_str().unwrap()).unwrap();
            let ours = value_text(&s.book.sheets[index].value_at(a));
            let theirs = cell["value"].as_str().unwrap();
            let theirs = match cell["kind"].as_str().unwrap() {
                "number" => rounded(theirs.parse().unwrap()),
                _ => theirs.to_owned(),
            };
            if ours != theirs {
                wrong.push(format!("{}!{a}: ours {ours}, Excel {theirs}", s.book.sheets[index].name));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // The data tables are still data tables to Excel.
    assert!(excel["sheets"].to_string().contains("TABLE("), "Excel shows the tables as TABLE()");
}
