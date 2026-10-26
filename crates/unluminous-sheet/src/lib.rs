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
//! - [`pages`] is page setup and printing: the setup a sheet keeps and the pages it makes.
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
pub mod cellstyles;
pub mod cfb;
pub mod chartbuild;
pub mod chartdata;
pub mod chartlayout;
pub mod chartspec;
pub mod charttype;
pub mod plot;
pub mod conditional;
pub mod coord;
pub mod dates;
pub mod edit;
pub mod eval;
pub mod fixedwidth;
pub mod formula;
pub mod ftab;
pub mod functions;
pub mod html;
pub mod input;
pub mod json;
pub mod model;
pub mod numfmt;
pub mod objects;
pub mod ods;
pub mod outline;
pub mod pages;
pub mod pivot;
pub mod protect;
pub mod refshift;
pub mod styles;
pub mod theme;
pub mod value;
pub mod whatif;
pub mod xls;
pub mod xlsb;
pub mod xlsx;
pub mod xml2003;
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
    /// Excel 97 to 2003 (BIFF8). Read only.
    Xls,
    /// Excel binary workbook. Read only.
    Xlsb,
    /// Comma separated values. Saved as Excel's "CSV UTF-8" unless [`SaveOptions`] says otherwise.
    Csv,
    /// Tab separated values with the extension `.tsv` or `.tab`.
    Tsv,
    /// Excel's "Unicode Text": tab separated, UTF-16.
    Txt,
    /// Excel's "Formatted Text (Space delimited)": read by finding the columns in the text.
    Prn,
    /// An OpenDocument spreadsheet.
    Ods,
    /// A web page with a table for each sheet. Saved only.
    Html,
    /// XML Spreadsheet 2003. Read only.
    Xml,
    /// JSON: an array of objects or of arrays. Read only.
    Json,
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
            "txt" => Ok(FileKind::Txt),
            "prn" => Ok(FileKind::Prn),
            "ods" => Ok(FileKind::Ods),
            "html" | "htm" => Ok(FileKind::Html),
            "xml" => Ok(FileKind::Xml),
            "json" => Ok(FileKind::Json),
            "xls" => Ok(FileKind::Xls),
            "xlsb" => Ok(FileKind::Xlsb),
            other => Err(format!("A .{other} file is not a workbook this version opens.")),
        }
    }

    /// Whether a file of this kind holds one sheet, so saving a workbook of several writes only the one showing.
    pub fn holds_one_sheet(&self) -> bool {
        matches!(self, FileKind::Csv | FileKind::Tsv | FileKind::Txt | FileKind::Prn)
    }

    /// Whether this version can write files of this kind.
    pub fn can_save(&self) -> bool {
        !matches!(self, FileKind::Xml | FileKind::Json | FileKind::Xls | FileKind::Xlsb)
    }
}

/// What a text file is saved with, where Excel offers a choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// How the text is encoded: UTF-8 with a byte order mark for a CSV (Excel's "CSV UTF-8") and UTF-16
    /// for a `.txt` file (Excel's "Unicode Text") unless this says otherwise. `Windows1252` is Excel's
    /// "CSV (Comma delimited)" and "Text (Tab delimited)", and `Utf8` is a CSV with no byte order mark.
    pub encoding: Option<csv::Encoding>,
}

/// Read a workbook from a file's bytes.
pub fn open(bytes: &[u8], kind: FileKind, name: &str, this_year: i64) -> Result<Workbook, String> {
    match kind {
        FileKind::Xlsx | FileKind::Xlsm if cfb::is_compound(bytes) => xls::read(bytes),
        FileKind::Xlsx | FileKind::Xlsm => xlsx::read(bytes),
        FileKind::Xls => xls::read(bytes),
        FileKind::Xlsb => xlsb::read(bytes),
        FileKind::Csv | FileKind::Tsv | FileKind::Txt => {
            let text = csv::decode(bytes);
            Ok(csv::read(&text, matches!(kind, FileKind::Tsv | FileKind::Txt), name, this_year))
        }
        FileKind::Prn => Ok(fixedwidth::read(&csv::decode(bytes), name, this_year)),
        FileKind::Ods => ods::read(bytes),
        FileKind::Xml => xml2003::read(bytes),
        FileKind::Json => json::read(&csv::decode(bytes), name),
        FileKind::Html => {
            Err("A web page is not a workbook this version opens. It can be saved as one."
                .to_owned())
        }
    }
}

/// A workbook as the bytes of a file. A CSV, tab separated or fixed width file holds the sheet that is
/// showing.
pub fn save(book: &Workbook, kind: FileKind) -> Result<Vec<u8>, String> {
    save_with(book, kind, &SaveOptions::default())
}

/// A workbook as the bytes of a file, with the encoding a text file asks for.
pub fn save_with(
    book: &Workbook,
    kind: FileKind,
    options: &SaveOptions,
) -> Result<Vec<u8>, String> {
    let sheet = book.active_sheet.min(book.sheets.len().saturating_sub(1));
    match kind {
        FileKind::Xlsx | FileKind::Xlsm => xlsx::write(book),
        FileKind::Xls | FileKind::Xlsb => Err(
            "Excel saves a workbook in an older format only when it is asked to; this version saves it as .xlsx beside the original. Use Save As and choose .xlsx.".to_owned(),
        ),
        FileKind::Csv => Ok(csv::encode(
            &csv::write(book, sheet, ','),
            options.encoding.unwrap_or(csv::Encoding::Utf8Bom),
        )),
        FileKind::Tsv => Ok(csv::encode(
            &csv::write(book, sheet, '\t'),
            options.encoding.unwrap_or(csv::Encoding::Utf8),
        )),
        FileKind::Txt => Ok(csv::encode(
            &csv::write(book, sheet, '\t'),
            options.encoding.unwrap_or(csv::Encoding::Utf16Le),
        )),
        FileKind::Prn => Ok(csv::encode(
            &fixedwidth::write(book, sheet),
            options.encoding.unwrap_or(csv::Encoding::Windows1252),
        )),
        FileKind::Ods => ods::write(book),
        FileKind::Html => Ok(html::write(book).into_bytes()),
        FileKind::Xml | FileKind::Json => Err(
            "This version reads that kind of file but does not write it. Save as .xlsx or .ods."
                .to_owned(),
        ),
    }
}
