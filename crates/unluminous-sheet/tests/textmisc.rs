//! Files that hold what Excel writes for functions this version computes in its own way, does not have, or
//! cannot compute without Excel's services, and the errors a cell may hold.
//!
//! `fixtures/textmisc.xlsx` was made by Excel (the cells by `tools/excel-oracle/inspect.ps1 -SaveAs` over a
//! workbook with a function that exists only in a newer Excel, a hidden `_xlfn.` name and a phonetic guide).
//! The Excel test passes with a printed line when Excel is not installed; `UNLUMINOUS_REQUIRE_EXCEL=1` makes
//! a missing Excel a failure.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::model::{Cell, Workbook};
use unluminous_sheet::value::ErrorKind;
use unluminous_sheet::Value;

fn fixture() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/textmisc.xlsx"))
        .unwrap()
}

fn at(text: &str) -> CellRef {
    CellRef::parse(text).unwrap()
}

/// The text of one part of a written file.
fn part(bytes: &[u8], name: &str) -> String {
    let entries = unluminous_sheet::zip::read(bytes).unwrap();
    let entry =
        entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} is missing"));
    String::from_utf8(entry.data.clone()).unwrap()
}

fn formula(book: &Workbook, cell: &str) -> String {
    book.sheets[0]
        .cell(at(cell))
        .and_then(|c| c.formula.as_ref())
        .map(|f| f.text.to_string())
        .unwrap()
}

#[test]
fn a_function_this_version_lacks_is_written_back_with_its_prefix_and_its_hidden_name() {
    let book = unluminous_sheet::xlsx::read(&fixture()).unwrap();
    assert_eq!(formula(&book, "D1"), "_xlfn.FUTUREFUNC(C1,2)");
    assert!(book.names.iter().any(|n| n.name == "_xlfn.FUTUREFUNC" && n.hidden));
    let written = unluminous_sheet::xlsx::write(&book).unwrap();
    let sheet = part(&written, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<f>_xlfn.FUTUREFUNC(C1,2)</f>"), "{sheet}");
    let workbook = part(&written, "xl/workbook.xml");
    assert!(workbook.contains("name=\"_xlfn.FUTUREFUNC\" hidden=\"1\""), "{workbook}");
}

#[test]
fn functions_this_version_computes_are_written_with_the_prefixes_excel_writes() {
    let book = unluminous_sheet::xlsx::read(&fixture()).unwrap();
    let written = unluminous_sheet::xlsx::write(&book).unwrap();
    let sheet = part(&written, "xl/worksheets/sheet1.xml").replace("&quot;", "\"");
    for expected in [
        "<f>_xlfn.REGEXTEST(\"abc\",\"b\")</f>",
        "<f>ROWS(_xlfn._TRO_TRAILING(C1:C9))</f>",
        "<f>SUM(_xlfn._TRO_ALL(C1:C9))</f>",
        "<f>_xlfn.ENCODEURL(\"a b\")</f>",
        "<f>_xlfn.GROUPBY(B1:B4,C1:C4,_xleta.SUM)</f>",
        "<f>LENB(B1)</f>",
        "<f>PHONETIC(A1)</f>",
    ] {
        assert!(sheet.contains(expected), "{expected} is not in\n{sheet}");
    }
    // The trim operators are shown the way the formula bar writes them.
    let shown = book.sheets[0].cell(at("D3")).and_then(|c| c.formula.as_ref()).unwrap().display();
    assert_eq!(shown, "=ROWS(C1:.C9)");
    let shown = book.sheets[0].cell(at("D4")).and_then(|c| c.formula.as_ref()).unwrap().display();
    assert_eq!(shown, "=SUM(C1.:.C9)");
}

#[test]
fn a_cell_that_needs_excels_service_keeps_the_value_the_file_saved() {
    let mut book = unluminous_sheet::xlsx::read(&fixture()).unwrap();
    let before = book.sheets[0].value_at(at("D5"));
    unluminous_sheet::calc::recalculate_all(&mut book, 46318.5);
    assert_eq!(book.sheets[0].value_at(at("D5")), before);
    assert_eq!(book.sheets[0].value_at(at("D6")), Value::Error(ErrorKind::Value));
    assert_eq!(formula(&book, "D5"), "_xlfn.TRANSLATE(\"hello\",\"fr\")");
    // The functions this version computes are computed.
    assert_eq!(book.sheets[0].value_at(at("D2")), Value::Bool(true));
    assert_eq!(book.sheets[0].value_at(at("D3")), Value::Number(4.0));
    assert_eq!(book.sheets[0].value_at(at("D4")), Value::Number(100.0));
    assert_eq!(book.sheets[0].value_at(at("D9")), Value::text("a%20b"));
    assert_eq!(book.sheets[0].value_at(at("D10")), Value::text("apple"));
}

#[test]
fn a_phonetic_guide_is_kept_and_read_by_phonetic() {
    let mut book = unluminous_sheet::xlsx::read(&fixture()).unwrap();
    unluminous_sheet::calc::recalculate_all(&mut book, 46318.5);
    assert_eq!(book.sheets[0].value_at(at("D7")), Value::text("カンジ"));
    let written = unluminous_sheet::xlsx::write(&book).unwrap();
    let strings = part(&written, "xl/sharedStrings.xml");
    assert!(strings.contains("<rPh sb=\"0\" eb=\"2\"><t>カンジ</t></rPh>"), "{strings}");
    assert!(part(&written, "xl/worksheets/sheet1.xml").contains("ph=\"1\""));
}

#[test]
fn a_file_with_an_error_kind_excel_cannot_read_in_a_cell_writes_it_as_value() {
    let mut book = Workbook::new();
    for (i, kind) in ErrorKind::ALL.iter().enumerate() {
        book.sheets[0].set(CellRef::new(i as u32, 0), Cell::value(Value::Error(*kind)));
    }
    let written = unluminous_sheet::xlsx::write(&book).unwrap();
    let sheet = part(&written, "xl/worksheets/sheet1.xml");
    for kind in ErrorKind::ALL {
        let wanted = format!("<v>{}</v>", kind.file_text());
        assert!(sheet.contains(&wanted), "{kind} is not written as {wanted}");
    }
    assert!(
        !sheet.contains("#SPILL!")
            && !sheet.contains("#BLOCKED!")
            && sheet.contains("<v>#BUSY!</v>")
    );
    let back = unluminous_sheet::xlsx::read(&written).unwrap();
    assert_eq!(back.sheets[0].value_at(at("A16")), Value::Error(ErrorKind::Busy));
    assert_eq!(back.sheets[0].value_at(at("A9")), Value::Error(ErrorKind::Value));
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

#[test]
fn excel_opens_a_file_holding_every_kind_of_error_with_no_repair() {
    if !excel_is_installed() {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
            "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
        );
        println!(
            "Excel is not installed here, so what it makes of the error cells was not checked"
        );
        return;
    }
    let mut book = Workbook::new();
    for (i, kind) in ErrorKind::ALL.iter().enumerate() {
        book.sheets[0].set(CellRef::new(i as u32, 0), Cell::value(Value::Error(*kind)));
    }
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-errors-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("errors.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&book).unwrap()).unwrap();
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(repo_root().join("tools/excel-oracle/inspect.ps1"))
        .arg("-Path")
        .arg(&path)
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let excel: serde_json::Value = serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}'))
        .unwrap_or_else(|e| panic!("Excel's answer did not read: {e}\n{text}"));
    assert_eq!(excel["repaired"], false, "Excel repaired the file: {}", excel["problem"]);
    let cells = excel["sheets"][0]["cells"].as_array().unwrap();
    assert_eq!(cells.len(), ErrorKind::ALL.len());
    assert!(cells.iter().all(|c| c["kind"] == "error"));
    std::fs::remove_file(&path).ok();
}
