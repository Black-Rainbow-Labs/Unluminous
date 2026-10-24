//! Reading and writing `.xlsx` files: what Excel wrote reads back, what Unluminous writes reads back, the
//! parts that are not modelled come out byte for byte, and Excel opens what Unluminous writes.
//!
//! The fixtures in `fixtures/` were made by Excel through `tools/excel-oracle/make-fixtures.ps1`. The last
//! test starts Excel through `tools/excel-oracle/inspect.ps1` when Excel is installed, and passes with a
//! printed line when it is not; `UNLUMINOUS_REQUIRE_EXCEL=1` makes a missing Excel a failure.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{Session, Shift};
use unluminous_sheet::model::{LinkTarget, Workbook};
use unluminous_sheet::styles::{BorderEdit, Color, Edge, FontEdit, Sides, StyleChange};
use unluminous_sheet::value::Value;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// Every cell as text: its value, its formula and its number format, so two workbooks can be compared.
fn cells(book: &Workbook) -> Vec<String> {
    let mut out = Vec::new();
    for s in &book.sheets {
        for (&(r, c), cell) in &s.cells {
            let formula = cell.formula.as_ref().map(|f| f.display()).unwrap_or_default();
            out.push(format!(
                "{}!{} {:?} {} {}",
                s.name,
                CellRef::new(r, c),
                cell.value,
                formula,
                book.styles.format_code(cell.style)
            ));
        }
        for (&(r, c), v) in &s.spills.values {
            out.push(format!("{}!{} spilled {:?}", s.name, CellRef::new(r, c), v));
        }
    }
    out
}

#[test]
fn a_workbook_excel_wrote_reads_with_its_features() {
    let book = unluminous_sheet::xlsx::read(&fixture("features.xlsx")).unwrap();
    assert_eq!(
        book.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["Data", "Summary", "Hidden"]
    );
    let data = &book.sheets[0];
    assert_eq!(data.frozen, Some((1, 0)));
    assert_eq!(data.merges, vec![Range::parse("A15:C15").unwrap()]);
    assert!(data.is_row_hidden(15));
    assert_eq!(data.conditional.iter().map(|c| c.rules.len()).sum::<usize>(), 4);
    assert_eq!(data.validations.len(), 1);
    assert_eq!(data.notes.len(), 1);
    assert_eq!(data.links.len(), 2);
    assert!(data.tab_color.is_some());
    assert_eq!(book.tables[0].name, "Fruit");
    assert_eq!(book.sheets[2].state, unluminous_sheet::model::Visibility::Hidden);
    // Shared formulas are expanded, one formula a cell.
    assert_eq!(
        data.cell(CellRef::parse("E5").unwrap()).unwrap().formula.as_ref().unwrap().display(),
        "=C5*D5"
    );
    // A dynamic array formula and what it spilled.
    assert_eq!(data.spills.range_of((1, 8)), Some(Range::parse("I2:J4").unwrap()));
    assert!(book.kept.parts.iter().any(|(p, _)| p == "xl/charts/chart1.xml"));
}

#[test]
fn what_excel_wrote_comes_back_the_same_after_unluminous_writes_it() {
    for name in ["features.xlsx", "blank.xlsx"] {
        let mut first = unluminous_sheet::xlsx::read(&fixture(name)).unwrap();
        unluminous_sheet::calc::recalculate_all(&mut first, 46318.5);
        let bytes = unluminous_sheet::xlsx::write(&first).unwrap();
        let second = unluminous_sheet::xlsx::read(&bytes).unwrap();
        assert_eq!(cells(&first), cells(&second), "{name}");
        // The parts Unluminous does not model are byte for byte the parts Excel wrote.
        for (path, kept) in &first.kept.parts {
            let again = second
                .kept
                .parts
                .iter()
                .find(|(p, _)| p == path)
                .unwrap_or_else(|| panic!("{name}: {path} was lost"));
            assert_eq!(kept, &again.1, "{name}: {path} changed");
        }
    }
}

#[test]
fn computing_a_workbook_excel_wrote_gives_the_values_excel_saved() {
    let saved = unluminous_sheet::xlsx::read(&fixture("features.xlsx")).unwrap();
    let mut computed = saved.clone();
    unluminous_sheet::calc::recalculate_all(&mut computed, 46318.5);
    for (s, (a, b)) in saved.sheets.iter().zip(&computed.sheets).enumerate() {
        for (key, cell) in &a.cells {
            if cell.formula.is_none() {
                continue;
            }
            let ours = &b.cells[key].value;
            let same = match (&cell.value, ours) {
                (Value::Number(x), Value::Number(y)) => (x - y).abs() <= 1e-9 * x.abs().max(1.0),
                (x, y) => x == y,
            };
            assert!(
                same,
                "sheet {s} {}: Excel saved {:?}, Unluminous computed {:?}",
                CellRef::new(key.0, key.1),
                cell.value,
                ours
            );
        }
    }
}

/// A workbook built with the editing operations: formulas, formats, a merge, a frozen pane, a note, links,
/// a validation list, a conditional format, a table, a second sheet and a dynamic array.
pub fn built_for_example() -> Session {
    built()
}

fn built() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    let at = |a: &str| CellRef::parse(a).unwrap();
    let rows = [
        ["Item", "Qty", "Price", "Total"],
        ["Tea", "3", "2.5", "=B2*C2"],
        ["Coffee", "2", "4", "=B3*C3"],
        ["Cake", "5", "3.25", "=B4*C4"],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            s.set_input(0, CellRef::new(r as u32, c as u32), v).unwrap();
        }
    }
    s.set_input(0, at("D5"), "=SUM(D2:D4)").unwrap();
    s.set_input(0, at("F1"), "=SORT(A2:A4)").unwrap();
    s.set_input(0, at("G1"), "=SEQUENCE(2,2)").unwrap();
    s.set_input(0, at("H1"), "10/23/2026").unwrap();
    s.set_input(0, at("H2"), "=TEXT(H1,\"dddd\")").unwrap();
    s.set_input(0, at("H3"), "=XLOOKUP(\"Cake\",A2:A4,D2:D4)").unwrap();
    s.set_input(0, at("H4"), "=LET(x,B2:B4,SUM(x*2))").unwrap();
    s.set_input(0, at("H5"), "12.5%").unwrap();
    s.set_style(
        0,
        &[Range::parse("A1:D1").unwrap()],
        &StyleChange::Font(FontEdit {
            bold: Some(true),
            color: Some(Color::rgb(0x1F, 0x4E, 0x79)),
            ..Default::default()
        }),
    )
    .unwrap();
    s.set_style(
        0,
        &[Range::parse("A1:D1").unwrap()],
        &StyleChange::Fill(Some(Color::rgb(0xDD, 0xEB, 0xF7))),
    )
    .unwrap();
    s.set_style(
        0,
        &[Range::parse("C2:D5").unwrap()],
        &StyleChange::NumberFormat("$#,##0.00".to_owned()),
    )
    .unwrap();
    s.set_style(
        0,
        &[Range::parse("A5:D5").unwrap()],
        &StyleChange::Border(BorderEdit {
            sides: Sides::Top,
            edge: Edge::line("double", Color::Auto),
        }),
    )
    .unwrap();
    s.set_input(0, at("A7"), "Merged").unwrap();
    s.merge(0, Range::parse("A7:C7").unwrap(), true).unwrap();
    s.freeze(0, 1, 0).unwrap();
    s.set_column_width(0, 0, 0, Some(16.0)).unwrap();
    s.set_row_height(0, 6, 6, Some(24.0)).unwrap();
    s.set_note(0, at("A2"), Some("Green tea"), "Jason").unwrap();
    s.set_link(
        0,
        Range::parse("A9").unwrap(),
        Some(LinkTarget::External("https://example.com/".to_owned())),
        Some("Example".to_owned()),
    )
    .unwrap();
    s.set_link(
        0,
        Range::parse("A10").unwrap(),
        Some(LinkTarget::Location("Notes!A1".to_owned())),
        Some("To notes".to_owned()),
    )
    .unwrap();
    let mut validation = unluminous_sheet::model::Validation {
        ranges: vec![Range::parse("B12").unwrap()],
        kind: "list".to_owned(),
        operator: None,
        formula1: Some("\"Low,Medium,High\"".to_owned()),
        formula2: None,
        allow_blank: true,
        show_dropdown: true,
        prompt_title: None,
        prompt: None,
        error_title: None,
        error: None,
        error_style: None,
        attrs: Vec::new(),
    };
    validation.prompt = Some("Pick one".to_owned());
    s.add_validation(0, validation).unwrap();
    let dxf = {
        s.book.styles.dxfs.push(unluminous_sheet::styles::Dxf {
            fill: Some(Color::rgb(0xC6, 0xEF, 0xCE)),
            font_color: Some(Color::rgb(0, 0x61, 0)),
            ..Default::default()
        });
        (s.book.styles.dxfs.len() - 1) as u32
    };
    let rule = unluminous_sheet::model::CfRule {
        kind: "cellIs".to_owned(),
        dxf: Some(dxf),
        priority: 1,
        stop: false,
        operator: Some("greaterThan".to_owned()),
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: vec!["10".to_owned()],
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
    };
    s.add_conditional(0, vec![Range::parse("D2:D4").unwrap()], rule).unwrap();
    s.add_table(0, Range::parse("A1:D4").unwrap(), Some("Orders")).unwrap();
    s.set_input(0, at("H6"), "=SUM(Orders[Qty])").unwrap();
    let notes = s.add_sheet(Some("Notes"), None).unwrap();
    s.set_input(notes, at("A1"), "Total from orders").unwrap();
    s.set_input(notes, at("B1"), "=Sheet1!D5").unwrap();
    s.set_input(notes, at("A2"), "Line one\nline two").unwrap();
    s.set_tab_color(notes, Some(Color::rgb(0xFF, 0xC0, 0))).unwrap();
    s.book.active_sheet = 0;
    s.insert(0, Range::parse("12:12").unwrap(), Shift::WholeRows).unwrap();
    s
}

#[test]
fn a_workbook_unluminous_built_reads_back_the_same() {
    let session = built();
    let bytes = unluminous_sheet::xlsx::write(&session.book).unwrap();
    let mut back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    // A blocked spill is saved as #VALUE!, as Excel saves it, and is #SPILL! again once computed.
    unluminous_sheet::calc::recalculate_all(&mut back, 46318.5);
    assert_eq!(cells(&session.book), cells(&back));
    assert_eq!(back.sheets[0].notes.len(), 1);
    assert_eq!(back.sheets[0].links.len(), 2);
    assert_eq!(
        back.sheets[0].validations[0].ranges,
        vec![Range::parse("B13").unwrap()],
        "the inserted row moved it"
    );
    assert_eq!(back.tables[0].name, "Orders");
    assert_eq!(back.sheets[1].name, "Notes");
}

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// Ask Excel what it sees in a file.
fn inspect(path: &Path) -> serde_json::Value {
    let script = repo_root().join("tools/excel-oracle/inspect.ps1");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .arg("-Recalculate")
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{FEFF}');
    serde_json::from_str(text).unwrap_or_else(|e| {
        panic!(
            "Excel's answer did not read: {e}\n{text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn excel_opens_what_unluminous_writes_and_computes_the_same_values() {
    if !excel_is_installed() {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
            "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
        );
        println!(
            "Excel is not installed here, so what it makes of Unluminous's files was not checked"
        );
        return;
    }
    let session = built();
    let folder = std::env::temp_dir().join(format!("unluminous-sheet-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("built.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap();
    let excel = inspect(&path);
    assert_eq!(excel["repaired"], false, "Excel repaired the file: {}", excel["problem"]);
    let mut wrong = Vec::new();
    for (index, sheet) in excel["sheets"].as_array().unwrap().iter().enumerate() {
        for cell in sheet["cells"].as_array().unwrap() {
            let at = CellRef::parse(cell["cell"].as_str().unwrap()).unwrap();
            let ours = session.book.sheets[index].value_at(at);
            let theirs = cell["value"].as_str().unwrap();
            let same = match (cell["kind"].as_str().unwrap(), &ours) {
                ("number", Value::Number(n)) => {
                    (theirs.parse::<f64>().unwrap() - n).abs() <= 1e-9 * n.abs().max(1.0)
                }
                ("text", Value::Text(t)) => &**t == theirs,
                ("bool", Value::Bool(b)) => theirs.eq_ignore_ascii_case(&b.to_string()),
                ("error", Value::Error(e)) => e.text() == theirs,
                ("empty", Value::Empty) => true,
                _ => false,
            };
            if !same {
                wrong.push(format!(
                    "{}!{at}: Excel {} {theirs}, Unluminous {ours:?}",
                    sheet["name"], cell["kind"]
                ));
            }
        }
    }
    let data = &excel["sheets"][0]["features"];
    assert_eq!(data["merged"], serde_json::json!(["A7:C7"]));
    assert_eq!(data["frozen"], "True|1|0");
    assert_eq!(data["notes"], serde_json::json!(["A2|Green tea"]));
    assert_eq!(data["tables"], serde_json::json!(["Orders|A1:D4"]));
    assert_eq!(data["conditional"], 1);
    assert!(
        data["validations"].as_array().unwrap().iter().any(|v| v == "B13|3|Low,Medium,High"),
        "{}",
        data["validations"]
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&folder);
}
