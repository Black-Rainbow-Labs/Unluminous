//! The lines of a pivot's rows and the columns of its columns, as a layout made them.
//!
//! A line is one row of the row area or one column of the column area. The same list is what the layout
//! writes cells from and what `rowItems` and `colItems` are written from.

use super::Func;

/// One step of the path to a line: an item of a field, or one of the value fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    /// The position of the item in the field's items.
    Item(usize),
    /// The value field a line belongs to, when the Values entry is on this axis.
    Data(usize),
}

/// What a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A line with an item of every field on its axis.
    Leaf,
    /// The row of an item that has items under it.
    Header,
    /// The total of the items under an item.
    Subtotal,
    Grand,
    /// An empty row after a group.
    Blank,
}

/// One line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: LineKind,
    /// The items from the outermost field in, with the value field where the Values entry is.
    pub path: Vec<Seg>,
    /// The node of the axis tree whose records the line adds up. The root for a grand total.
    pub node: usize,
    /// The value field the line shows, when it is fixed by the line.
    pub data: Option<usize>,
    /// A subtotal's function when it is not the value field's own.
    pub func: Option<Func>,
    /// The index among the axis entries of the field a subtotal is for.
    pub level: usize,
    /// Whether the line shows figures.
    pub values: bool,
    /// Whether the line is shown (a grand total that is switched off is still computed).
    pub shown: bool,
}

impl Line {
    pub fn is_total(&self) -> bool {
        matches!(self.kind, LineKind::Subtotal | LineKind::Grand)
    }
}

/// The lines of both axes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lines {
    pub rows: Vec<Line>,
    pub cols: Vec<Line>,
}
