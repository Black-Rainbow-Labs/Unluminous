//! Excel workbooks: the model, the `.xlsx` file, the formula language and its evaluator, number formats,
//! CSV, and every editing operation, with no user interface dependency.
//!
//! - [`model`] is the workbook: sheets of cells, the formatting they name, and the parts of the file
//!   that are kept rather than understood.
//! - [`zip`] and [`xlsx`] read and write the file, keeping what is not modelled.
//! - [`formula`] parses formula text into a tree and prints it back, moving references when cells move.
//! - [`eval`] and [`functions`] compute a formula, and [`calc`] decides which formulas to compute and in
//!   what order.
//! - [`numfmt`] shows a value the way a format code says, and [`input`] reads what a person types.
//! - [`edit`] is every change a person or an agent makes, each one undoable.
//!
//! `tasks/task-2338-spreadsheet-tdd.md` is the design.

// Three lints that work against code written to match its source: a conversion factor is written as the
// reference gives it, a matrix routine indexes its rows and columns by number as the algorithm does, and a
// match arm that writes something once keeps its `if` inside, because moving it into a guard sends the
// next case to the arm after it (that rewrite broke every number format with a thousands separator).
#![allow(
    clippy::inconsistent_digit_grouping,
    clippy::needless_range_loop,
    clippy::collapsible_match
)]

pub mod calc;
pub mod conditional;
pub mod coord;
pub mod dates;
pub mod edit;
pub mod eval;
pub mod formula;
pub mod functions;
pub mod input;
pub mod model;
pub mod numfmt;
pub mod outline;
pub mod refshift;
pub mod styles;
pub mod value;
pub mod xlsx;
pub mod zip;

pub use coord::{CellRef, Range};
pub use model::{Cell, Sheet, Workbook};
pub use value::{Array, ErrorKind, Value};
pub mod csv;

/// The kinds of file a workbook is opened from and saved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Xlsx,
    Xlsm,
    Csv,
    Tsv,
}

impl FileKind {
    /// The kind a file's extension names, or the reason it cannot be opened.
    pub fn of(path: &std::path::Path) -> Result<FileKind, String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "xlsx" => Ok(FileKind::Xlsx),
            "xlsm" => Ok(FileKind::Xlsm),
            "csv" => Ok(FileKind::Csv),
            "tsv" | "tab" => Ok(FileKind::Tsv),
            "xls" => Err("This is an Excel 97-2003 workbook (.xls), a binary format this version does not read. Excel can save it as .xlsx.".to_owned()),
            "xlsb" => Err("This is an Excel binary workbook (.xlsb), which this version does not read. Excel can save it as .xlsx.".to_owned()),
            "ods" => Err("This is an OpenDocument spreadsheet (.ods), which this version does not read. Excel or LibreOffice can save it as .xlsx.".to_owned()),
            other => Err(format!("A .{other} file is not a workbook this version opens.")),
        }
    }
}

/// Read a workbook from a file's bytes.
pub fn open(bytes: &[u8], kind: FileKind, name: &str, this_year: i64) -> Result<Workbook, String> {
    match kind {
        FileKind::Xlsx | FileKind::Xlsm => xlsx::read(bytes),
        FileKind::Csv | FileKind::Tsv => {
            let text = String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| {
                bytes.iter().map(|b| crate::functions::text::windows_1252(*b)).collect()
            });
            Ok(csv::read(&text, kind == FileKind::Tsv, name, this_year))
        }
    }
}

/// A workbook as the bytes of a file. A CSV holds the sheet that is showing.
pub fn save(book: &Workbook, kind: FileKind) -> Result<Vec<u8>, String> {
    match kind {
        FileKind::Xlsx | FileKind::Xlsm => xlsx::write(book),
        FileKind::Csv => Ok(csv::write(book, book.active_sheet, ',').into_bytes()),
        FileKind::Tsv => Ok(csv::write(book, book.active_sheet, '\t').into_bytes()),
    }
}
