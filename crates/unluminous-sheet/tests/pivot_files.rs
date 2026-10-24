//! Pivot tables in files: what Excel wrote is read into the model and laid out again to the cells Excel
//! had, and what is written here is read back the same.

use std::collections::BTreeMap;
use std::path::PathBuf;

use unluminous_sheet::edit::Session;
use unluminous_sheet::{CellRef, Value, Workbook};

fn fixture(name: &str) -> Vec<u8> {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", "pivot", name].iter().collect();
    std::fs::read(path).unwrap()
}

/// The cells of a table with their number formats.
fn snapshot(book: &Workbook, t: usize) -> BTreeMap<(u32, u32), (Value, String)> {
    let table = &book.pivots.tables[t];
    let sheet = &book.sheets[table.sheet];
    let range = table.full_range();
    let mut out = BTreeMap::new();
    for at in range.cells() {
        if let Some(cell) = sheet.cell(at) {
            if !cell.value.is_empty() {
                out.insert((at.row, at.col), (cell.value.clone(), book.styles.format_code(cell.style)));
            }
        }
    }
    out
}

fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-300),
        _ => a == b,
    }
}

fn differences(before: &BTreeMap<(u32, u32), (Value, String)>, after: &BTreeMap<(u32, u32), (Value, String)>) -> Vec<String> {
    let mut out = Vec::new();
    for (k, (v, f)) in before {
        let at = CellRef::new(k.0, k.1).a1();
        match after.get(k) {
            None => out.push(format!("{at}: Excel has {v:?}, we have nothing")),
            Some((v2, f2)) => {
                if !same(v, v2) {
                    out.push(format!("{at}: Excel has {v:?}, we have {v2:?}"));
                } else if f != f2 {
                    out.push(format!("{at}: Excel format {f}, ours {f2}"));
                }
            }
        }
    }
    for k in after.keys() {
        if !before.contains_key(k) {
            out.push(format!("{}: we have {:?} and Excel nothing", CellRef::new(k.0, k.1).a1(), after[k].0));
        }
    }
    out
}

#[test]
fn the_pivot_in_the_probe_file_is_read_and_laid_out_as_excel_did() {
    let book = unluminous_sheet::xlsx::read(&fixture("17-pivot.xlsx")).unwrap();
    assert_eq!(book.pivots.tables.len(), 1, "the pivot table is understood");
    assert_eq!(book.pivots.caches.len(), 1);
    let table = &book.pivots.tables[0];
    assert_eq!(table.name, "PT1");
    assert_eq!(table.location.a1(), "A3:B8");
    assert_eq!(book.pivots.caches[0].records.len(), 8);
    let before = snapshot(&book, 0);
    let mut session = Session::new(book, 46319.5);
    session.pivot_refresh(0).unwrap();
    let after = snapshot(&session.book, 0);
    let problems = differences(&before, &after);
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(before.len(), 12);
}

#[test]
fn every_pivot_excel_built_is_read_and_laid_out_again_to_its_cells() {
    let book = unluminous_sheet::xlsx::read(&fixture("excel-pivots.xlsx")).unwrap();
    let sheets_with_pivots = book.sheets.iter().filter(|s| s.name.starts_with("P_")).count();
    assert_eq!(book.pivots.tables.len(), sheets_with_pivots, "every pivot table is understood");
    let mut session = Session::new(book, 46319.5);
    let mut failed = Vec::new();
    for t in 0..session.book.pivots.tables.len() {
        let name = session.book.sheets[session.book.pivots.tables[t].sheet].name.clone();
        let before = snapshot(&session.book, t);
        if let Err(e) = session.pivot_refresh(t) {
            failed.push(format!("{name}: {e}"));
            continue;
        }
        let problems = differences(&before, &snapshot(&session.book, t));
        if !problems.is_empty() {
            failed.push(format!("{name}: {}", problems.iter().take(4).cloned().collect::<Vec<_>>().join("; ")));
        }
    }
    assert!(failed.is_empty(), "{} of {} differ:\n{}", failed.len(), session.book.pivots.tables.len(), failed.join("\n"));
}

#[test]
fn a_pivot_nothing_changed_is_written_back_byte_for_byte() {
    let bytes = fixture("17-pivot.xlsx");
    let book = unluminous_sheet::xlsx::read(&bytes).unwrap();
    let written = unluminous_sheet::xlsx::write(&book).unwrap();
    let parts = |bytes: &[u8]| -> BTreeMap<String, Vec<u8>> {
        unluminous_sheet::zip::read(bytes).unwrap().into_iter().map(|e| (e.name, e.data)).collect()
    };
    let before = parts(&bytes);
    let after = parts(&written);
    for name in [
        "xl/pivotTables/pivotTable1.xml",
        "xl/pivotCache/pivotCacheDefinition1.xml",
        "xl/pivotCache/pivotCacheRecords1.xml",
    ] {
        assert_eq!(before.get(name), after.get(name), "{name} is written as it was read");
    }
    assert!(String::from_utf8_lossy(&after["xl/workbook.xml"]).contains("<pivotCache cacheId=\"3\""));
}
