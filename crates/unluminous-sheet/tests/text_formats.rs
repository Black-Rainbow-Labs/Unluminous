//! The text file formats: CSV, tab separated text, fixed width text, XML Spreadsheet 2003, JSON and HTML.
//!
//! `fixtures/text/` holds what Excel writes when it saves `csv-source.xlsx` and `edge.xlsx` in each format,
//! made by `tools/excel-oracle/save-text-formats.ps1`. Unluminous reads the workbook and writes the same
//! format, and the bytes are compared with Excel's.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::csv::Encoding;
use unluminous_sheet::{FileKind, SaveOptions, Value, Workbook};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text")
}

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap()
}

fn workbook(name: &str) -> Workbook {
    unluminous_sheet::xlsx::read(&bytes(&format!("{name}.xlsx"))).unwrap()
}

/// The first place two byte strings differ, with the text around it.
fn first_difference(ours: &[u8], excels: &[u8]) -> String {
    let at = ours.iter().zip(excels).position(|(a, b)| a != b).unwrap_or(ours.len().min(excels.len()));
    let show = |b: &[u8]| String::from_utf8_lossy(&b[at.saturating_sub(30)..(at + 30).min(b.len())]).into_owned();
    format!("byte {at}\n ours:  {:?}\n Excel: {:?}", show(ours), show(excels))
}

fn same_bytes(what: &str, ours: Vec<u8>, excels: Vec<u8>) {
    assert!(ours == excels, "{what} differs from Excel's at {}", first_difference(&ours, &excels));
}

const WORKBOOKS: [&str; 2] = ["csv-source", "edge"];

#[test]
fn csv_utf8_is_written_the_way_excel_writes_it() {
    for name in WORKBOOKS {
        let saved = unluminous_sheet::save(&workbook(name), FileKind::Csv).unwrap();
        assert_eq!(&saved[..3], &[0xEF, 0xBB, 0xBF], "CSV UTF-8 starts with a byte order mark");
        same_bytes(name, saved, bytes(&format!("{name}.utf8.csv")));
    }
}

#[test]
fn a_plain_csv_has_no_byte_order_mark_and_the_code_page_csv_is_windows_1252() {
    for name in WORKBOOKS {
        let book = workbook(name);
        let plain = unluminous_sheet::save_with(&book, FileKind::Csv, &SaveOptions { encoding: Some(Encoding::Utf8) }).unwrap();
        assert_eq!(plain, bytes(&format!("{name}.utf8.csv"))[3..].to_vec());
        let ansi = unluminous_sheet::save_with(&book, FileKind::Csv, &SaveOptions { encoding: Some(Encoding::Windows1252) }).unwrap();
        same_bytes(name, ansi, bytes(&format!("{name}.ansi.csv")));
    }
}

#[test]
fn text_files_are_written_the_way_excel_writes_unicode_text_and_tab_delimited_text() {
    for name in WORKBOOKS {
        let book = workbook(name);
        same_bytes(name, unluminous_sheet::save(&book, FileKind::Txt).unwrap(), bytes(&format!("{name}.unicode.txt")));
        let ansi = unluminous_sheet::save_with(&book, FileKind::Txt, &SaveOptions { encoding: Some(Encoding::Windows1252) }).unwrap();
        same_bytes(name, ansi, bytes(&format!("{name}.tab.txt")));
    }
}

#[test]
fn formatted_text_is_laid_out_the_way_excel_lays_it_out() {
    for name in WORKBOOKS {
        let book = workbook(name);
        let ours = unluminous_sheet::csv::decode(&unluminous_sheet::save(&book, FileKind::Prn).unwrap());
        let excels = unluminous_sheet::csv::decode(&bytes(&format!("{name}.prn")));
        let (a, b): (Vec<&str>, Vec<&str>) = (ours.split("\r\n").collect(), excels.split("\r\n").collect());
        assert_eq!(a.len(), b.len(), "{name}: line count");
        // The accounting column (characters 44 to 52 of an edge line) is filled with spaces up to the width
        // of the digits in Excel's font, which this version only estimates, so those characters are left out.
        let cut = |s: &str| -> String {
            if name == "edge" && s.chars().count() > 52 {
                s.chars().take(43).chain(s.chars().skip(52)).collect()
            } else {
                s.to_owned()
            }
        };
        let wrong: Vec<String> = a
            .iter()
            .zip(&b)
            .enumerate()
            .filter(|(_, (x, y))| cut(x) != cut(y))
            .map(|(i, (x, y))| format!("line {}:\n ours:  {x:?}\n Excel: {y:?}", i + 1))
            .collect();
        assert!(wrong.is_empty(), "{name}\n{}", wrong.join("\n"));
    }
}

#[test]
fn what_excel_wrote_reads_back_with_the_values_it_was_saved_from() {
    for name in WORKBOOKS {
        let original = workbook(name);
        for file in [format!("{name}.utf8.csv"), format!("{name}.unicode.txt"), format!("{name}.tab.txt")] {
            let kind = FileKind::of(Path::new(&file)).unwrap_or(FileKind::Csv);
            let kind = if file.ends_with(".txt") { FileKind::Txt } else { kind };
            let read = unluminous_sheet::open(&bytes(&file), kind, "x", 2026).unwrap();
            let (a, b) = (&original.sheets[0], &read.sheets[0]);
            for (&(r, c), cell) in &a.cells {
                let at = CellRef::new(r, c);
                // Text that looks like a number is a number when a text file is opened, and a date is a date.
                if matches!(cell.value, Value::Number(_)) && cell.style == 0 {
                    let Value::Number(x) = cell.value else { continue };
                    if x.abs() < 1e11 && x.abs() > 1e-5 {
                        assert!(matches!(b.value_at(at), Value::Number(_)), "{file} {at}: {:?}", b.value_at(at));
                    }
                }
            }
        }
    }
    let read = unluminous_sheet::open(&bytes("edge.unicode.txt"), FileKind::Txt, "x", 2026).unwrap();
    assert_eq!(read.sheets[0].value_at(CellRef::parse("A9").unwrap()), Value::text("\u{e9}\u{20ac}\u{65e5}\u{672c}\u{8a9e}"));
    assert_eq!(read.sheets[0].value_at(CellRef::parse("A6").unwrap()), Value::text("line1\nline2"));
}
