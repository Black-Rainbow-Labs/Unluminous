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

pub mod coord;
pub mod dates;
pub mod eval;
pub mod formula;
pub mod functions;
pub mod input;
pub mod model;
pub mod numfmt;
pub mod styles;
pub mod value;
pub mod zip;

pub use coord::{CellRef, Range};
pub use model::{Cell, Sheet, Workbook};
pub use value::{Array, ErrorKind, Value};
