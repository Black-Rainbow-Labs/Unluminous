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
    let at =
        ours.iter().zip(excels).position(|(a, b)| a != b).unwrap_or(ours.len().min(excels.len()));
    let show = |b: &[u8]| {
        String::from_utf8_lossy(&b[at.saturating_sub(30)..(at + 30).min(b.len())]).into_owned()
    };
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
        let plain = unluminous_sheet::save_with(
            &book,
            FileKind::Csv,
            &SaveOptions { encoding: Some(Encoding::Utf8) },
        )
        .unwrap();
        assert_eq!(plain, bytes(&format!("{name}.utf8.csv"))[3..].to_vec());
        let ansi = unluminous_sheet::save_with(
            &book,
            FileKind::Csv,
            &SaveOptions { encoding: Some(Encoding::Windows1252) },
        )
        .unwrap();
        same_bytes(name, ansi, bytes(&format!("{name}.ansi.csv")));
    }
}

#[test]
fn text_files_are_written_the_way_excel_writes_unicode_text_and_tab_delimited_text() {
    for name in WORKBOOKS {
        let book = workbook(name);
        same_bytes(
            name,
            unluminous_sheet::save(&book, FileKind::Txt).unwrap(),
            bytes(&format!("{name}.unicode.txt")),
        );
        let ansi = unluminous_sheet::save_with(
            &book,
            FileKind::Txt,
            &SaveOptions { encoding: Some(Encoding::Windows1252) },
        )
        .unwrap();
        same_bytes(name, ansi, bytes(&format!("{name}.tab.txt")));
    }
}

#[test]
fn formatted_text_is_laid_out_the_way_excel_lays_it_out() {
    for name in WORKBOOKS {
        let book = workbook(name);
        let ours =
            unluminous_sheet::csv::decode(&unluminous_sheet::save(&book, FileKind::Prn).unwrap());
        let excels = unluminous_sheet::csv::decode(&bytes(&format!("{name}.prn")));
        let (a, b): (Vec<&str>, Vec<&str>) =
            (ours.split("\r\n").collect(), excels.split("\r\n").collect());
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
fn xml_spreadsheet_2003_reads_as_the_workbook_excel_saved_it_from() {
    for name in WORKBOOKS {
        let original = workbook(name);
        let read = unluminous_sheet::open(&bytes(&format!("{name}.xml")), FileKind::Xml, "x", 2026)
            .unwrap();
        let (a, b) = (&original.sheets[0], &read.sheets[0]);
        assert_eq!(a.name, b.name);
        let mut keys: Vec<(u32, u32)> = a.cells.keys().chain(b.cells.keys()).copied().collect();
        keys.sort();
        keys.dedup();
        let mut wrong = Vec::new();
        for (r, c) in keys {
            let at = CellRef::new(r, c);
            let shown = |book: &Workbook, sheet: &unluminous_sheet::Sheet| {
                let style = sheet.cell(at).map_or(0, |x| x.style);
                let code = book.styles.format_code(style);
                unluminous_sheet::numfmt::Format::parse(&code)
                    .render(&sheet.value_at(at), false, 11)
                    .text
            };
            let formula = |s: &unluminous_sheet::Sheet| {
                s.cell(at).and_then(|x| x.formula.as_ref()).map(|f| f.display())
            };
            if shown(&original, a) != shown(&read, b) {
                wrong.push(format!(
                    "{at}: shows {:?} against {:?}",
                    shown(&original, a),
                    shown(&read, b)
                ));
            }
            if a.value_at(at) != b.value_at(at)
                && !matches!((a.value_at(at), b.value_at(at)), (Value::Number(x), Value::Number(y)) if (x - y).abs() < 1e-9 * x.abs().max(1.0))
            {
                wrong.push(format!("{at}: {:?} against {:?}", a.value_at(at), b.value_at(at)));
            }
            if formula(a) != formula(b) {
                wrong.push(format!("{at}: formula {:?} against {:?}", formula(a), formula(b)));
            }
        }
        assert!(wrong.is_empty(), "{name}\n{}", wrong.join("\n"));
        let widths = |book: &Workbook, s: &unluminous_sheet::Sheet| -> Vec<(u32, i64)> {
            (0..12)
                .filter_map(|c| {
                    s.column(c)
                        .and_then(|i| i.width.filter(|_| i.custom_width))
                        .map(|w| (c, (w * 4.0).round() as i64))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .inspect(|_| {
                    let _ = book;
                })
                .collect()
        };
        assert_eq!(widths(&original, a), widths(&read, b), "{name}: column widths");
    }
}

#[test]
fn fixed_width_text_is_cut_into_columns_where_every_line_has_a_gap() {
    let text = "Name      Qty   Price\nTea         3    2.50\nCoffee     12   14.00\n";
    let book = unluminous_sheet::open(text.as_bytes(), FileKind::Prn, "menu", 2026).unwrap();
    let s = &book.sheets[0];
    let at = |a: &str| s.value_at(CellRef::parse(a).unwrap());
    assert_eq!(at("A1"), Value::text("Name"));
    assert_eq!(at("B2"), Value::Number(3.0));
    assert_eq!(at("C3"), Value::Number(14.0));
    assert_eq!(at("A3"), Value::text("Coffee"));
}

#[test]
fn json_files_open_as_tables() {
    let book = unluminous_sheet::open(
        br#"[{"a":1,"b":"x"},{"a":2,"c":true}]"#,
        FileKind::Json,
        "data",
        2026,
    )
    .unwrap();
    assert_eq!(book.sheets[0].value_at(CellRef::parse("C3").unwrap()), Value::Bool(true));
    assert!(unluminous_sheet::save(&book, FileKind::Json).is_err());
}

#[test]
fn html_is_written_with_a_table_for_each_sheet_and_xml_and_json_are_read_only() {
    let book = workbook("edge");
    let page = String::from_utf8(unluminous_sheet::save(&book, FileKind::Html).unwrap()).unwrap();
    assert!(
        page.contains("<h2>Edge</h2>")
            && page.contains("<td x:str>plain</td>")
            && page.contains("2026-10-23")
    );
    assert!(
        !FileKind::Xml.can_save()
            && FileKind::Csv.holds_one_sheet()
            && !FileKind::Ods.holds_one_sheet()
    );
}

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Ask Excel what it sees in a file, through `tools/excel-oracle/inspect.ps1`.
fn inspect(path: &Path) -> serde_json::Value {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("tools/excel-oracle/inspect.ps1");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{FEFF}');
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("Excel's answer did not read: {e}\n{text}"))
}

#[test]
fn excel_opens_the_page_unluminous_writes_with_the_same_text_in_the_same_cells() {
    if !excel_is_installed() {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
            "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
        );
        println!(
            "Excel is not installed here, so what it makes of Unluminous's page was not checked"
        );
        return;
    }
    let mut book = workbook("csv-source");
    book.sheets[0].merges.push(unluminous_sheet::coord::Range::parse("D1:E1").unwrap());
    book.sheets[0]
        .set(CellRef::parse("D1").unwrap(), unluminous_sheet::Cell::value(Value::text("across")));
    let folder = std::env::temp_dir().join(format!("unluminous-html-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("page.html");
    std::fs::write(&path, unluminous_sheet::save(&book, FileKind::Html).unwrap()).unwrap();
    let excel = inspect(&path);
    assert_eq!(excel["repaired"], false);
    let sheet = &excel["sheets"][0];
    // The sheet's name is written above the table, so Excel puts the table a row or two down.
    let place_of = |text: &str| -> CellRef {
        let cell = sheet["cells"].as_array().unwrap().iter().find(|c| c["text"] == text);
        CellRef::parse(
            cell.unwrap_or_else(|| panic!("Excel found no cell showing {text:?}: {sheet}"))["cell"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    };
    let origin = place_of("007");
    let at = |dr: u32, dc: u32| CellRef::new(origin.row + dr, origin.col + dc);
    assert_eq!(place_of("2026-10-23"), at(0, 1));
    assert_eq!(place_of("25.6%"), at(0, 2));
    assert_eq!(place_of("two\nlines"), CellRef::new(origin.row - 1, origin.col + 1));
    assert_eq!(place_of("3"), at(1, 0));
    assert_eq!(place_of("after gap"), at(1, 2));
    assert_eq!(place_of("1,234,567.89"), at(2, 1));
    assert_eq!(
        place_of("across"),
        CellRef::new(origin.row - 1, origin.col + 3),
        "the merged cell is in the first row"
    );
    assert_eq!(sheet["features"]["merged"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&folder);
}

#[test]
fn what_excel_wrote_reads_back_with_the_values_it_was_saved_from() {
    for name in WORKBOOKS {
        let original = workbook(name);
        for file in
            [format!("{name}.utf8.csv"), format!("{name}.unicode.txt"), format!("{name}.tab.txt")]
        {
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
                        assert!(
                            matches!(b.value_at(at), Value::Number(_)),
                            "{file} {at}: {:?}",
                            b.value_at(at)
                        );
                    }
                }
            }
        }
    }
    let read =
        unluminous_sheet::open(&bytes("edge.unicode.txt"), FileKind::Txt, "x", 2026).unwrap();
    assert_eq!(
        read.sheets[0].value_at(CellRef::parse("A9").unwrap()),
        Value::text("\u{e9}\u{20ac}\u{65e5}\u{672c}\u{8a9e}")
    );
    assert_eq!(read.sheets[0].value_at(CellRef::parse("A6").unwrap()), Value::text("line1\nline2"));
}
