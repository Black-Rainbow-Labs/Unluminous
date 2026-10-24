//! Copy and paste in full: Paste Special with every option Excel has, Paste Link, Insert Copied Cells,
//! paste from other programs, and the fill commands (series, Flash Fill and the rest).
//!
//! Excel is the oracle for all of it. `tests/clipboard.rs` replays scenarios that `tools/excel-oracle/
//! edit-oracle.ps1` ran through Excel's own object model and compares every cell.

mod extras;
mod external;
mod fill;
mod flash;
mod html;
mod insert_copied;
mod lists;
mod operate;
mod paste;
mod series;
mod snapshot;
mod trend;

pub use fill::{AcrossSheets, FillKind, SeriesDate, SeriesKind, SeriesSpec};
pub use external::{parse_text_grid, ExternalPaste};
pub use flash::flash_fill_examples;
pub use lists::CustomLists;

use std::collections::BTreeMap;

use crate::coord::{CellRef, Range};
use crate::model::{ColumnInfo, Hyperlink, Note, Validation};
use crate::styles::{Dxf, Font};
use crate::styles::{Alignment, Border, Fill, Xf};

/// What Paste Special takes from the cells copied, in the order of Excel's `XlPasteType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PasteKind {
    /// Everything: values, formulas, formats, notes, validation and the rest.
    All,
    /// Formulas and constants, with the formats left as they are.
    Formulas,
    /// Values only; formulas become the numbers and text they show.
    Values,
    /// Formats only, with the conditional formats and merged cells.
    Formats,
    /// Notes only.
    Comments,
    /// Data validation only.
    Validation,
    /// Everything, with the theme colours of the source kept as they look.
    AllUsingSourceTheme,
    /// Everything except borders.
    AllExceptBorders,
    /// The widths of the copied columns.
    ColumnWidths,
    /// Formulas and the number formats.
    FormulasAndNumberFormats,
    /// Values and the number formats.
    ValuesAndNumberFormats,
    /// Everything, adding the conditional formats to the ones already there.
    AllMergingConditionalFormats,
}

impl PasteKind {
    /// Every kind, which a Paste Special popup lists.
    pub const ALL_KINDS: [PasteKind; 12] = [
        PasteKind::All,
        PasteKind::Formulas,
        PasteKind::Values,
        PasteKind::Formats,
        PasteKind::Comments,
        PasteKind::Validation,
        PasteKind::AllUsingSourceTheme,
        PasteKind::AllExceptBorders,
        PasteKind::ColumnWidths,
        PasteKind::FormulasAndNumberFormats,
        PasteKind::ValuesAndNumberFormats,
        PasteKind::AllMergingConditionalFormats,
    ];

    /// The value of Excel's `XlPasteType` for this kind.
    pub fn excel_code(self) -> i32 {
        match self {
            PasteKind::All => -4104,
            PasteKind::Formulas => -4123,
            PasteKind::Values => -4163,
            PasteKind::Formats => -4122,
            PasteKind::Comments => -4144,
            PasteKind::Validation => 6,
            PasteKind::AllUsingSourceTheme => 13,
            PasteKind::AllExceptBorders => 7,
            PasteKind::ColumnWidths => 8,
            PasteKind::FormulasAndNumberFormats => 11,
            PasteKind::ValuesAndNumberFormats => 12,
            PasteKind::AllMergingConditionalFormats => 14,
        }
    }

    /// The kind with an Excel `XlPasteType` value.
    pub fn from_excel_code(code: i32) -> Option<Self> {
        Self::ALL_KINDS.into_iter().find(|k| k.excel_code() == code)
    }

    /// The words an agent writes for the kind: `values`, `all-except-borders` and so on.
    pub fn name(self) -> &'static str {
        match self {
            PasteKind::All => "all",
            PasteKind::Formulas => "formulas",
            PasteKind::Values => "values",
            PasteKind::Formats => "formats",
            PasteKind::Comments => "comments",
            PasteKind::Validation => "validation",
            PasteKind::AllUsingSourceTheme => "source-theme",
            PasteKind::AllExceptBorders => "all-except-borders",
            PasteKind::ColumnWidths => "widths",
            PasteKind::FormulasAndNumberFormats => "formulas-and-number-formats",
            PasteKind::ValuesAndNumberFormats => "values-and-number-formats",
            PasteKind::AllMergingConditionalFormats => "merge-conditional",
        }
    }

    /// The kind written as an agent writes it; a few shorter spellings are accepted too.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase().replace([' ', '_'], "-");
        Self::ALL_KINDS.into_iter().find(|k| k.name() == text).or(match text.as_str() {
            "notes" | "comment" | "note" => Some(PasteKind::Comments),
            "widths" | "column-widths" => Some(PasteKind::ColumnWidths),
            "number-formats" | "formats-and-values" => None,
            "theme" | "using-source-theme" => Some(PasteKind::AllUsingSourceTheme),
            "no-borders" | "except-borders" => Some(PasteKind::AllExceptBorders),
            "merge-cf" | "merging-conditional-formats" => {
                Some(PasteKind::AllMergingConditionalFormats)
            }
            _ => None,
        })
    }

    /// Whether the paste writes the cells' contents (values or formulas).
    pub(crate) fn writes_content(self) -> bool {
        !matches!(
            self,
            PasteKind::Formats
                | PasteKind::Comments
                | PasteKind::Validation
                | PasteKind::ColumnWidths
        )
    }

    /// Whether formulas stay formulas.
    pub(crate) fn keeps_formulas(self) -> bool {
        self.writes_content()
            && !matches!(self, PasteKind::Values | PasteKind::ValuesAndNumberFormats)
    }

    /// Whether the notes, the data validation, the links and the merged cells of the source come too.
    pub(crate) fn is_everything(self) -> bool {
        matches!(
            self,
            PasteKind::All
                | PasteKind::AllUsingSourceTheme
                | PasteKind::AllExceptBorders
                | PasteKind::AllMergingConditionalFormats
        )
    }
}

/// What Paste Special does to the numbers already in the cells it pastes over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PasteOperation {
    /// Replace them.
    None,
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl PasteOperation {
    /// The value of Excel's `XlPasteSpecialOperation`.
    pub fn excel_code(self) -> i32 {
        match self {
            PasteOperation::None => -4142,
            PasteOperation::Add => 2,
            PasteOperation::Subtract => 3,
            PasteOperation::Multiply => 4,
            PasteOperation::Divide => 5,
        }
    }

    /// The operation with an Excel `XlPasteSpecialOperation` value.
    pub fn from_excel_code(code: i32) -> Option<Self> {
        [
            PasteOperation::None,
            PasteOperation::Add,
            PasteOperation::Subtract,
            PasteOperation::Multiply,
            PasteOperation::Divide,
        ]
        .into_iter()
        .find(|o| o.excel_code() == code)
    }

    /// The word an agent writes: `none`, `add`, `subtract`, `multiply` or `divide`.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim().to_ascii_lowercase().as_str() {
            "none" | "" => PasteOperation::None,
            "add" | "+" => PasteOperation::Add,
            "subtract" | "-" => PasteOperation::Subtract,
            "multiply" | "*" => PasteOperation::Multiply,
            "divide" | "/" => PasteOperation::Divide,
            _ => return None,
        })
    }
}

/// Every choice of Paste Special: what to take, what to do with the numbers underneath, whether blank
/// cells leave the cells under them alone, and whether rows turn into columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteSpecial {
    pub kind: PasteKind,
    pub operation: PasteOperation,
    pub skip_blanks: bool,
    pub transpose: bool,
}

impl Default for PasteSpecial {
    fn default() -> Self {
        Self {
            kind: PasteKind::All,
            operation: PasteOperation::None,
            skip_blanks: false,
            transpose: false,
        }
    }
}

impl PasteSpecial {
    /// A paste of one kind with no operation, no skipping and no transposing.
    pub fn of(kind: PasteKind) -> Self {
        Self { kind, ..Self::default() }
    }
}

impl From<super::PasteMode> for PasteSpecial {
    fn from(mode: super::PasteMode) -> Self {
        use super::PasteMode;
        match mode {
            PasteMode::All => Self::of(PasteKind::All),
            PasteMode::Values => Self::of(PasteKind::Values),
            PasteMode::Formulas => Self::of(PasteKind::Formulas),
            PasteMode::Formats => Self::of(PasteKind::Formats),
            PasteMode::Transpose => Self { transpose: true, ..Self::default() },
        }
    }
}

/// How a copied cell format looks, kept with the clip so a paste into another workbook can make the same
/// format there.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipStyle {
    pub xf: Xf,
    pub font: Font,
    pub fill: Fill,
    pub border: Border,
    pub format_code: String,
    pub alignment: Alignment,
}

/// A conditional format cut down to the copied block, with positions counted from its top left cell.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipRules {
    pub ranges: Vec<Range>,
    pub rules: Vec<crate::model::CfRule>,
}

/// Everything a copy carries besides the cells themselves. Positions are counted from the top left cell of
/// the copied block, and rows are counted after rows a filter hides were left out.
#[derive(Debug, Clone, Default)]
pub struct ClipExtras {
    pub notes: Vec<(u32, u32, Note)>,
    pub validations: Vec<(Range, Validation)>,
    pub conditional: Vec<ClipRules>,
    pub merges: Vec<Range>,
    pub links: Vec<Hyperlink>,
    /// The column info of each copied column, by offset from the first column.
    pub widths: Vec<(u32, ColumnInfo)>,
    /// The height of each copied row that has one, by offset from the first row.
    pub heights: Vec<(u32, f64)>,
    pub whole_columns: bool,
    pub whole_rows: bool,
    /// The format of every cell format the copied cells use, by index in the source workbook.
    pub styles: BTreeMap<u32, ClipStyle>,
    /// The source workbook's theme colours.
    pub theme: Vec<u32>,
    /// The differential formats the conditional formats name, by index in the source workbook.
    pub dxfs: BTreeMap<u32, Dxf>,
    /// For each row of the block, its offset from the first row copied. Empty unless rows were left out
    /// because a filter hides them, which is how Excel copies a filtered range.
    pub rows: Vec<u32>,
    /// Whether the source workbook counts dates from 1904.
    pub date1904: bool,
    /// The offsets from the first row copied of the rows that are hidden, which the text for other programs
    /// leaves out, and the same for columns.
    pub hidden_rows: Vec<u32>,
    pub hidden_columns: Vec<u32>,
}

impl super::Clip {
    /// The rows and columns of the block as pasted: rows a filter hid are not counted.
    pub fn block_size(&self) -> (u32, u32) {
        let rows = if self.extras.rows.is_empty() {
            self.range.rows()
        } else {
            self.extras.rows.len() as u32
        };
        (rows, self.range.cols())
    }

    /// The row on the source sheet of a row counted in the block.
    pub fn source_row(&self, row: u32) -> u32 {
        let offset = self.extras.rows.get(row as usize).copied().unwrap_or(row);
        self.range.first.row + offset
    }

    /// The cell on the source sheet that a cell counted in the block was copied from.
    pub fn source_cell(&self, row: u32, col: u32) -> CellRef {
        CellRef::new(self.source_row(row), self.range.first.col + col)
    }
}
