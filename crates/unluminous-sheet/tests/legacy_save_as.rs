//! Saving what the binary readers made as `.xlsx`, which is what the older formats are saved as.

use std::path::{Path, PathBuf};

use unluminous_sheet::{FileKind, Workbook};

const FIXTURES: &[&str] = &[
    "01-views",
    "02-protection",
    "03-pagesetup",
    "04-outline",
    "05-condfmt",
    "06-validation",
    "07-table",
    "08-filter",
    "09-comments",
    "10-text",
    "11-styles",
    "12-hyperlinks",
    "13-names",
    "15-shapes",
    "16-sparklines",
    "17-pivot",
    "18-external-target",
    "18-formulas",
    "19-macro",
    "csv-source",
    "features",
];

fn legacy_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

fn open(name: &str) -> Workbook {
    let path = legacy_dir().join(name);
    let bytes = std::fs::read(&path).unwrap();
    unluminous_sheet::open(&bytes, FileKind::of(&path).unwrap(), name, 2026).unwrap()
}

/// Every cell as text: its place, value, formula and format.
fn cells(book: &Workbook) -> Vec<String> {
    let mut out = Vec::new();
    for sheet in &book.sheets {
        for (&(r, c), cell) in &sheet.cells {
            out.push(format!(
                "{}!{} {:?} {} {}",
                sheet.name,
                unluminous_sheet::coord::CellRef::new(r, c),
                cell.value,
                cell.formula.as_ref().map(|f| f.display()).unwrap_or_default(),
                book.styles.format_code(cell.style)
            ));
        }
    }
    out
}

#[test]
fn what_a_binary_reader_made_is_saved_as_xlsx_and_reads_back_the_same() {
    for extension in ["xls", "xlsb"] {
        for name in FIXTURES {
            let file = format!("{name}.{extension}");
            let book = open(&file);
            let bytes = unluminous_sheet::save(&book, FileKind::Xlsx)
                .unwrap_or_else(|e| panic!("{file}: {e}"));
            let again =
                unluminous_sheet::xlsx::read(&bytes).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(cells(&book), cells(&again), "{file}");
            assert_eq!(
                book.sheets
                    .iter()
                    .map(|s| (&s.name, s.merges.len(), s.notes.len()))
                    .collect::<Vec<_>>(),
                again
                    .sheets
                    .iter()
                    .map(|s| (&s.name, s.merges.len(), s.notes.len()))
                    .collect::<Vec<_>>(),
                "{file}"
            );
        }
    }
}

/// Writes every fixture as `.xlsx` into the folder in `CONVERTED_TO`, for Excel to open.
#[test]
#[ignore]
fn write_every_fixture_as_xlsx() {
    let Ok(folder) = std::env::var("CONVERTED_TO") else { return };
    std::fs::create_dir_all(&folder).unwrap();
    for extension in ["xls", "xlsb"] {
        for name in FIXTURES.iter().chain(&["functions"]) {
            let file = format!("{name}.{extension}");
            let bytes = unluminous_sheet::save(&open(&file), FileKind::Xlsx).unwrap();
            std::fs::write(Path::new(&folder).join(format!("{name}-{extension}.xlsx")), bytes)
                .unwrap();
        }
    }
}
