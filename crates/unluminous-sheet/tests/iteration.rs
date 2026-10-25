//! Iterative calculation and precision as displayed, against workbooks Excel calculated.
//!
//! `tools/excel-oracle/make-iterative.ps1` builds the fixtures. For the circular workbooks it enters the
//! formulas with iteration off, writes the value of every formula cell to `<name>.start.json`, switches
//! iteration on and calculates once. This test puts the start values back, calculates the workbook the same
//! way and compares every formula cell with the value Excel saved. For precision as displayed the fixture
//! holds the numbers as Excel stored them, and the test types the same text under the same formats.

use std::collections::BTreeMap;
use std::path::Path;

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::styles::StyleChange;
use unluminous_sheet::value::Value;
use unluminous_sheet::xlsx;

const NOW: f64 = 46318.5;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// Every number a workbook holds, by sheet name and cell.
fn numbers(book: &Workbook, formulas_only: bool) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for sheet in &book.sheets {
        for (&(row, col), cell) in &sheet.cells {
            if formulas_only && cell.formula.is_none() {
                continue;
            }
            if let Value::Number(n) = cell.value {
                out.insert(format!("{}!{}", sheet.name, CellRef::new(row, col)), n);
            }
        }
    }
    out
}

/// Put the values Excel's cells held before it calculated into the formula cells.
fn restore_starts(book: &mut Workbook, name: &str) {
    let starts: BTreeMap<String, f64> =
        serde_json::from_slice(&fixture(&format!("{name}.start.json"))).unwrap();
    for sheet in &mut book.sheets {
        for (&(row, col), cell) in sheet.cells.iter_mut() {
            let key = format!("{}!{}", sheet.name, CellRef::new(row, col));
            if cell.formula.is_some() {
                cell.value = Value::Number(*starts.get(&key).unwrap_or(&0.0));
            }
        }
    }
}

/// Calculate a circular workbook from Excel's start values and answer what it computed and what Excel saved.
fn calculated_against_excel(name: &str) -> (BTreeMap<String, f64>, BTreeMap<String, f64>) {
    let mut book = xlsx::read(&fixture(&format!("{name}.xlsx"))).unwrap();
    let saved = numbers(&book, true);
    restore_starts(&mut book, name);
    let session = Session::new(book, NOW);
    (numbers(&session.book, true), saved)
}

fn assert_same(ours: &BTreeMap<String, f64>, excel: &BTreeMap<String, f64>) {
    assert_eq!(ours.keys().collect::<Vec<_>>(), excel.keys().collect::<Vec<_>>());
    for (cell, e) in excel {
        let n = ours[cell];
        let close = (n - e).abs() <= 1e-9 * e.abs().max(1.0);
        assert!(close, "{cell}: Excel saved {e}, calculated {n}");
    }
}

#[test]
fn the_settings_are_read_from_the_file() {
    let book = xlsx::read(&fixture("iterative.xlsx")).unwrap();
    assert!(book.calc.iterate);
    assert_eq!(book.calc.iterate_count, 100);
    assert_eq!(book.calc.iterate_delta, 0.001);
    assert!(book.calc.full_precision);
    let few = xlsx::read(&fixture("iterative-few.xlsx")).unwrap();
    assert_eq!(few.calc.iterate_count, 5);
    assert_eq!(few.calc.iterate_delta, 0.0000001);
    let shown = xlsx::read(&fixture("precision.xlsx")).unwrap();
    assert!(!shown.calc.full_precision);
}

#[test]
fn the_settings_survive_saving() {
    for name in ["iterative.xlsx", "iterative-few.xlsx", "precision.xlsx"] {
        let book = xlsx::read(&fixture(name)).unwrap();
        let again = xlsx::read(&xlsx::write(&book).unwrap()).unwrap();
        assert_eq!(again.calc, book.calc, "{name}");
    }
}

#[test]
fn circular_references_settle_the_way_excel_settles_them() {
    let (ours, excel) = calculated_against_excel("iterative");
    assert_same(&ours, &excel);
}

#[test]
fn a_few_passes_stop_where_excel_stops() {
    let (ours, excel) = calculated_against_excel("iterative-few");
    assert_same(&ours, &excel);
}

#[test]
fn without_iteration_a_circular_cell_is_zero() {
    let mut book = xlsx::read(&fixture("iterative.xlsx")).unwrap();
    book.calc.iterate = false;
    let session = Session::new(book, NOW);
    assert_eq!(session.book.sheets[0].cell(CellRef::new(0, 0)).unwrap().value, Value::Number(0.0));
    assert!(session.book.circular.is_some());
}

#[test]
fn switching_iteration_on_settles_a_circular_workbook() {
    let mut session = Session::new(Workbook::new(), NOW);
    session.set_input(0, CellRef::new(0, 0), "=B1+1").unwrap();
    session.set_input(0, CellRef::new(0, 1), "=A1*0.5").unwrap();
    assert!(session.book.circular.is_some());
    session.set_iteration(true, 100, 0.0000001);
    assert!(session.book.circular.is_none());
    let a = session.book.sheets[0].cell(CellRef::new(0, 0)).unwrap().value.clone();
    let Value::Number(a) = a else { panic!("A1 is a number") };
    assert!((a - 2.0).abs() < 1e-6, "A1 is {a}");
    session.undo();
    assert!(!session.book.calc.iterate);
}

#[test]
fn a_manual_workbook_waits_for_a_calculation() {
    let mut session = Session::new(Workbook::new(), NOW);
    session.set_automatic_calculation(false);
    session.set_input(0, CellRef::new(0, 0), "1").unwrap();
    session.set_input(0, CellRef::new(0, 1), "=A1+1").unwrap();
    assert_eq!(session.book.sheets[0].cell(CellRef::new(0, 1)).unwrap().value, Value::Empty);
    session.recalculate_all();
    assert_eq!(session.book.sheets[0].cell(CellRef::new(0, 1)).unwrap().value, Value::Number(2.0));
    session.set_automatic_calculation(true);
    assert!(session.book.calc.automatic);
}

/// The text typed into each cell of `precision.xlsx` and the format the cell had.
const SHOWN: [(&str, &str, &str); 14] = [
    ("A1", "0.00", "1.23456"),
    ("A2", "0.0%", "0.12345"),
    ("A3", "#,##0", "1234.5"),
    ("A4", "0", "2.5"),
    ("A5", "0.000", "-2.0005"),
    ("A6", "#,##0.0,", "123456"),
    ("A7", "$#,##0.00", "9.995"),
    ("A8", "General", "0.1234567890123456"),
    ("A9", "0.00E+00", "12345.6789"),
    ("A10", "0.00", "=1/3"),
    ("A11", "0%", "=2/3"),
    ("A12", "0.0", "=A1*3"),
    ("A13", "# ?/?", "0.3333"),
    ("A14", "m/d/yyyy", "46023.75"),
];

#[test]
fn a_number_is_stored_as_it_is_shown() {
    let excel = numbers(&xlsx::read(&fixture("precision.xlsx")).unwrap(), false);
    let mut session = Session::new(Workbook::new(), NOW);
    session.set_precision_as_displayed(true);
    for (cell, format, text) in SHOWN {
        let at = CellRef::parse(cell).unwrap();
        session
            .set_style(0, &[Range::cell(at)], &StyleChange::NumberFormat(format.to_owned()))
            .unwrap();
        session.set_input(0, at, text).unwrap();
    }
    let ours = numbers(&session.book, false);
    assert_same(
        &ours.into_iter().map(|(k, v)| (k.replace("Sheet1", "Shown"), v)).collect(),
        &excel,
    );
}

#[test]
fn turning_precision_as_displayed_on_rounds_what_is_there() {
    let mut session = Session::new(Workbook::new(), NOW);
    let a1 = CellRef::new(0, 0);
    session
        .set_style(0, &[Range::cell(a1)], &StyleChange::NumberFormat("0.00".to_owned()))
        .unwrap();
    session.set_input(0, a1, "1.23456").unwrap();
    session.set_input(0, CellRef::new(0, 1), "=A1*10").unwrap();
    session.set_precision_as_displayed(true);
    let value = |s: &Session, at: CellRef| s.book.sheets[0].cell(at).unwrap().value.clone();
    assert_eq!(value(&session, a1), Value::Number(1.23));
    assert_eq!(value(&session, CellRef::new(0, 1)), Value::Number(12.3));
    session.set_precision_as_displayed(false);
    session.set_input(0, a1, "1.23456").unwrap();
    assert_eq!(value(&session, a1), Value::Number(1.23456));
}
