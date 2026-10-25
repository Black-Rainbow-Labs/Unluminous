//! What a tab remembers for the clipboard and fill buttons, the print preview, the PivotTable Fields pane and
//! the slicers (`task-2360`, the `wiringb` lane). It is one field of the view so the view's own file stays
//! small.

use unluminous_sheet::coord::Range;
use unluminous_sheet::edit::{Clip, PasteKind, PasteOperation};
// `Clip`, `PasteKind` and `PasteOperation` are exported from `edit` itself.

/// The choices of the Paste Special popup before they are applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PasteDraft {
    pub kind: PasteKind,
    pub operation: PasteOperation,
    pub skip_blanks: bool,
    pub transpose: bool,
    /// Paste Link was chosen, which Enter then does in place of a paste.
    pub link: bool,
}

impl Default for PasteDraft {
    fn default() -> Self {
        Self {
            kind: PasteKind::All,
            operation: PasteOperation::None,
            skip_blanks: false,
            transpose: false,
            link: false,
        }
    }
}

impl PasteDraft {
    /// The words of the `paste-special` verb that apply these choices.
    pub fn words(&self) -> Vec<String> {
        let mut words = vec!["paste-special".to_owned()];
        if self.link {
            words.push("link".to_owned());
            return words;
        }
        words.push(self.kind.name().to_owned());
        words.push(
            match self.operation {
                PasteOperation::None => "none",
                PasteOperation::Add => "add",
                PasteOperation::Subtract => "subtract",
                PasteOperation::Multiply => "multiply",
                PasteOperation::Divide => "divide",
            }
            .to_owned(),
        );
        if self.skip_blanks {
            words.push("skip-blanks".to_owned());
        }
        if self.transpose {
            words.push("transpose".to_owned());
        }
        words
    }
}

/// Which button follows the last change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterKind {
    /// Paste Options, after a paste.
    Paste,
    /// Auto Fill Options, after a fill.
    Fill,
}

/// The button offered beside the cells a paste or a fill just wrote, which stays until the workbook changes
/// again.
#[derive(Debug, Clone)]
pub struct After {
    pub kind: AfterKind,
    pub sheet: usize,
    /// The cells written.
    pub target: Range,
    /// The range that was chosen to paste or fill into, which another way of pasting starts from again.
    pub dest: Range,
    /// For a paste, the cells copied; for a fill, the cells the fill started from.
    pub source: Range,
    /// The clip a paste took, so another way of pasting it can be chosen.
    pub clip: Option<Clip>,
    /// The book's revision just after the change; the button is drawn only while it still holds.
    pub revision: u64,
    /// Whether the menu of the button is open.
    pub open: bool,
}

/// One slicer: a list of buttons that filter a pivot table by the items of one field.
#[derive(Debug, Clone, PartialEq)]
pub struct Slicer {
    pub name: String,
    pub pivot: String,
    pub field: String,
    pub sheet: usize,
    /// The top left cell of the slicer's box.
    pub at: unluminous_sheet::coord::CellRef,
}

/// One timeline: a bar over the dates of a pivot table's date field.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pub name: String,
    pub pivot: String,
    pub field: String,
    pub sheet: usize,
    pub at: unluminous_sheet::coord::CellRef,
    /// The first and last date serial the timeline keeps; `None` keeps every date.
    pub span: Option<(f64, f64)>,
}

/// The print preview.
#[derive(Debug, Clone, Default)]
pub struct Preview {
    /// The page showing, from zero.
    pub page: usize,
    /// How many pages the paginator laid out when it last ran.
    pub pages: usize,
}

/// Everything this lane keeps on a tab.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub paste_draft: Option<PasteDraft>,
    pub after: Option<After>,
    pub preview: Option<Preview>,
    /// The PivotTable Fields pane is open, and what is typed in its search box.
    pub pivot_pane: bool,
    pub pivot_search: String,
    pub slicers: Vec<Slicer>,
    pub timelines: Vec<Timeline>,
}
