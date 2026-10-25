//! What a tab remembers while the data tools, auditing and review are in use (`task-2360`): the levels of
//! the Sort popup being edited, the trace arrows drawn over the grid, the Evaluate Formula stepper, and
//! which of the review panels are open. It is one field of the view so the view's own file stays small.

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::review::evaluate::Evaluator;

/// One level of the Sort popup, before it is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDraft {
    /// The sheet column compared, or the row when sorting left to right.
    pub key: u32,
    /// `values`, `cell-color`, `font-color` or `icon`.
    pub on: String,
    /// The colour as `#RRGGBB`, the icon as `Set/N`, or `none`; empty for values.
    pub with: String,
    pub descending: bool,
    /// The items of a custom list, joined by `|`.
    pub list: String,
}

impl LevelDraft {
    /// A level that sorts a column by its values, smallest first.
    pub fn values(key: u32) -> Self {
        Self {
            key,
            on: "values".to_owned(),
            with: String::new(),
            descending: false,
            list: String::new(),
        }
    }
}

/// The Sort popup's state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SortDraft {
    pub levels: Vec<LevelDraft>,
    /// `guess`, `header` or `noheader`.
    pub header: String,
    pub case_sensitive: bool,
    pub left_to_right: bool,
    /// Whether the popup is showing as a level of the panel.
    pub open: bool,
}

/// One arrow of Trace Precedents, Trace Dependents or Trace Error.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub from_sheet: usize,
    pub from: Range,
    pub to_sheet: usize,
    pub to: CellRef,
    /// The source is on another sheet, drawn as a dashed arrow to a sheet icon.
    pub other_sheet: bool,
    /// Drawn in red, as Trace Error draws.
    pub error: bool,
}

/// Everything the review and auditing tools keep on a tab.
#[derive(Debug, Clone, Default)]
pub struct AuditState {
    pub sort: SortDraft,
    pub arrows: Vec<Arrow>,
    pub evaluator: Option<Evaluator>,
    pub watch_open: bool,
    pub comments_open: bool,
    /// The cell whose error indicator menu is open.
    pub error_menu: Option<CellRef>,
}
