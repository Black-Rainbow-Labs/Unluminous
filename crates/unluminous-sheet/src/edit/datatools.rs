//! The data tools of the Data tab: sorting by levels, filters, Advanced Filter, Remove Duplicates, Text to
//! Columns, Consolidate and the table design operations.
//!
//! Every operation is a method of [`Session`](crate::edit::Session), one undo step, refuses what Excel
//! refuses with Excel's words and leaves the workbook as it was when it refuses. `tests/datatools.rs` runs
//! each one against the answers Excel gave to the same steps (`tests/fixtures/datatools`).

mod advanced;
pub(crate) mod collate;
mod consolidate;
mod dataops;
mod filtering;
mod header;
mod sorting;
mod tables;
mod xmltree;

pub use consolidate::{ConsolidateFunction, Consolidation};
pub use dataops::{ColumnFormat, DateOrder, Delimiters, SplitMode, TextToColumns};
pub use sorting::{CellDisplay, DirectFormats, HeaderMode, SortLevel, SortOn, SortOptions};

use crate::model::{Cell, Workbook};

/// The text a cell shows, with nothing added to it for a clipboard.
pub(crate) fn displayed(book: &Workbook, cell: &Cell) -> String {
    let code = book.styles.format_code(cell.style);
    crate::numfmt::Format::parse(&code).render(&cell.value, book.date1904, 255).text
}

pub use tables::{TableOptions, TableStyle, TotalFunction};
