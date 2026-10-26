//! Reviewing a workbook: comments and notes, finding things, going to special cells, and auditing formulas.
//!
//! Each file is one group of [`Session`](super::Session) methods and the plain types they answer with:
//!
//! - [`comments`] has threaded comments and notes: new, reply, edit, resolve, delete, show and hide, the
//!   size of a note's box, and converting notes to threaded comments.
//! - [`search`] is Find and Replace with Excel's options.
//! - [`special`] is Go To Special.
//! - [`trace`] has precedents, dependents and Trace Error.
//! - [`errors`] is Error Checking, with the errors a person told Excel to ignore.
//! - [`evaluate`] is the Evaluate Formula stepper.
//! - [`stats`] has the Watch Window, Workbook Statistics and Create Names from Selection.

pub mod comments;
pub mod errors;
pub mod evaluate;
pub mod notebox;
pub mod search;
pub mod special;
pub mod stats;
pub mod trace;
