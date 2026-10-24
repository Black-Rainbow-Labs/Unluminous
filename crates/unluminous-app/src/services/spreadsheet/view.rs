//! What one spreadsheet tab is looking at: the sheet, the selection, where it is scrolled, the cell being
//! typed into, and which of the tab's menus and panels is open.

use unluminous_sheet::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};

use super::axis::Axis;
use super::config::Paper;

/// The text being typed into a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub sheet: usize,
    pub at: CellRef,
    pub text: String,
    /// Started by typing over the cell, where the arrow keys finish the cell and move, as in Excel's Enter
    /// mode. Started with F2 or a double click, they move the caret instead.
    pub typed: bool,
    /// Whether the box in the cell or the one in the formula bar takes the keyboard.
    pub in_bar: bool,
    /// The reference the arrow keys or a click put into a formula, as a char range of `text`, so moving it
    /// again replaces it.
    pub pointing: Option<(usize, usize, Range)>,
    /// Give the box the keyboard once, with the caret at `caret` or at the end.
    pub focus: bool,
    pub caret: Option<usize>,
    /// The function the list is offering, by its row.
    pub chosen: usize,
}

/// One level of the Format panel, and the path down to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelPath(pub Vec<String>);

/// A small box asking for one or two words, for a sheet's new name, a note, a link and the like.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    /// What the answer is for, which is also what pressing OK does: a command word and its leading arguments.
    pub verb: Vec<String>,
    pub title: String,
    pub label: String,
    pub text: String,
    /// A second field, for a link's shown text or a rule's second value.
    pub second: Option<(String, String)>,
    pub focus: bool,
}

/// The find bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Find {
    pub text: String,
    pub replace: Option<String>,
    pub whole_book: bool,
    pub case: bool,
    /// Which of the matches is chosen.
    pub at: usize,
    pub focus: bool,
}

/// A drag in progress on the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    /// Choosing cells.
    Select,
    /// Choosing whole columns from this one.
    Columns(u32),
    /// Choosing whole rows from this one.
    Rows(u32),
    /// Resizing a column from its right edge: the column and its width at the start.
    ColumnEdge(u32, f32),
    /// Resizing a row from its bottom edge.
    RowEdge(u32, f32),
    /// Pulling the fill handle.
    Fill,
    /// Pointing at cells for a formula being typed.
    Point,
}

/// The axes of a sheet, built for one revision of the workbook.
#[derive(Debug, Clone)]
pub struct Axes {
    pub revision: u64,
    pub sheet: usize,
    pub columns: Axis,
    pub rows: Axis,
}

/// What a tab is looking at.
#[derive(Debug, Clone)]
pub struct View {
    pub sheet: usize,
    /// The active cell.
    pub cursor: CellRef,
    /// The other corner of the range being chosen.
    pub anchor: CellRef,
    /// Ranges chosen before the current one, with Ctrl and a click.
    pub extra: Vec<Range>,
    /// The range kept chosen while Enter or Tab walks the active cell through it, when the active cell is
    /// not at one of its corners.
    pub held: Option<Range>,
    /// How far the cells past the frozen ones are scrolled, in points at a zoom of one.
    pub scroll: egui::Vec2,
    pub zoom: f32,
    pub edit: Option<Edit>,
    pub panel: Option<PanelPath>,
    /// The right click menu, where it was opened.
    pub menu: Option<egui::Pos2>,
    /// The workbook menu under the `...` button, and what is typed in its search box.
    pub book_menu: Option<String>,
    pub find: Option<Find>,
    /// The cells copied or cut, drawn with a marching outline until a paste or Escape.
    pub marquee: Option<(usize, Range, bool)>,
    pub prompt: Option<Prompt>,
    pub paper: Paper,
    pub drag: Option<Drag>,
    /// The sheet tab being renamed in place, and the name so far.
    pub renaming: Option<(usize, String)>,
    /// The right click menu of a sheet tab.
    pub sheet_menu: Option<(usize, egui::Pos2)>,
    /// The list under a filter button: its column and where it opened.
    pub filter_menu: Option<(u32, egui::Pos2)>,
    /// The chart chosen with a click, by its place in the sheet's list.
    pub chart: Option<usize>,
    /// The list under a cell with a list rule, opened with its arrow or Alt+Down.
    pub choices: bool,
    /// Scroll the active cell into view on the next frame.
    pub reveal: bool,
    /// Give the grid the keyboard on the next frame.
    pub take_keys: bool,
    pub axes: Option<Axes>,
    /// The grid's rectangle and where its cells start, last frame, for a command that asks where a cell is.
    pub drawn: Option<egui::Rect>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            sheet: 0,
            cursor: CellRef::new(0, 0),
            anchor: CellRef::new(0, 0),
            extra: Vec::new(),
            held: None,
            scroll: egui::Vec2::ZERO,
            zoom: 1.0,
            edit: None,
            panel: None,
            menu: None,
            book_menu: None,
            find: None,
            marquee: None,
            prompt: None,
            paper: Paper::Theme,
            drag: None,
            renaming: None,
            sheet_menu: None,
            filter_menu: None,
            chart: None,
            choices: false,
            reveal: false,
            take_keys: false,
            axes: None,
            drawn: None,
        }
    }
}

impl View {
    /// The range being chosen now.
    pub fn range(&self) -> Range {
        self.held.unwrap_or(Range::new(self.anchor, self.cursor))
    }

    /// Every range chosen, the current one last.
    pub fn ranges(&self) -> Vec<Range> {
        let mut all = self.extra.clone();
        all.push(self.range());
        all
    }

    /// Choose one cell.
    pub fn select_cell(&mut self, at: CellRef) {
        self.held = None;
        self.cursor = at;
        self.anchor = at;
        self.extra.clear();
        self.reveal = true;
        self.choices = false;
    }

    /// Choose a range, with the active cell at its top left.
    pub fn select_range(&mut self, range: Range) {
        self.held = None;
        self.anchor = range.last;
        self.cursor = range.first;
        self.extra.clear();
        self.reveal = true;
        self.choices = false;
    }

    /// Choose whole columns from `first` to `last`.
    pub fn select_columns(&mut self, first: u32, last: u32) {
        self.held = None;
        self.anchor = CellRef::new(MAX_ROWS - 1, last);
        self.cursor = CellRef::new(0, first);
        self.extra.clear();
    }

    /// Choose whole rows from `first` to `last`.
    pub fn select_rows(&mut self, first: u32, last: u32) {
        self.held = None;
        self.anchor = CellRef::new(last, MAX_COLS - 1);
        self.cursor = CellRef::new(first, 0);
        self.extra.clear();
    }

    /// Move the active cell, keeping the anchor when the range is being extended.
    pub fn move_to(&mut self, at: CellRef, extend: bool) {
        if extend {
            self.cursor = at;
        } else {
            self.select_cell(at);
        }
        self.reveal = true;
    }

    /// The axes of the sheet showing, built again when the workbook or the sheet changed.
    pub fn axes(&mut self, sheet: &unluminous_sheet::Sheet, revision: u64) -> &Axes {
        let stale = self
            .axes
            .as_ref()
            .is_none_or(|axes| axes.revision != revision || axes.sheet != self.sheet);
        if stale {
            self.axes = Some(Axes {
                revision,
                sheet: self.sheet,
                columns: Axis::columns(sheet),
                rows: Axis::rows(sheet),
            });
        }
        self.axes.as_ref().expect("just built")
    }

    /// Close every menu and panel that floats over the grid, which Escape and a click on the grid do.
    pub fn close_the_menus(&mut self) {
        self.menu = None;
        self.book_menu = None;
        self.sheet_menu = None;
        self.filter_menu = None;
        self.choices = false;
    }
}

/// Whether the spreadsheet takes a key chord itself while its tab has the keyboard, rather than letting a
/// menu entry with the same chord act on it. These are Excel's own chords, listed in the TDD §8.5.
pub fn claims_key(key: egui::Key, modifiers: egui::Modifiers) -> bool {
    use egui::Key::*;
    let command = modifiers.command || modifiers.ctrl;
    if command && modifiers.shift && key == P {
        return true;
    }
    if command {
        return matches!(
            key,
            Z | Y
                | A
                | D
                | R
                | B
                | I
                | U
                | F
                | H
                | G
                | K
                | L
                | T
                | Space
                | Minus
                | Plus
                | Equals
                | Enter
                | Home
                | End
                | ArrowUp
                | ArrowDown
                | ArrowLeft
                | ArrowRight
                | PageUp
                | PageDown
                | Semicolon
                | Num1
                | Num2
                | Num3
                | Num4
                | Num5
                | Num6
                | Num9
                | Num0
                | Backtick
                | Colon
        );
    }
    if modifiers.alt {
        return matches!(key, Equals | Plus | ArrowDown | Enter);
    }
    matches!(key, F2 | F3 | F4 | F9 | F11 | Delete | Space)
}

/// Where Ctrl and an arrow goes from `at`: to the end of the run of filled cells it is in, or to the next
/// filled cell past a gap, or to the edge of the sheet, as Excel's Ctrl+arrow does.
pub fn edge_of_data(sheet: &unluminous_sheet::Sheet, at: CellRef, rows: i64, cols: i64) -> CellRef {
    let filled = |c: CellRef| !sheet.value_at(c).is_empty();
    let step = |c: CellRef| c.offset(rows, cols);
    let used = sheet.used_range().map_or(CellRef::new(0, 0), |r| r.last);
    // Past the last used row or column there is nothing to find, so the edge of the sheet is the answer.
    let beyond = |c: CellRef| (rows > 0 && c.row > used.row) || (cols > 0 && c.col > used.col);
    let Some(next) = step(at) else { return at };
    if filled(at) && filled(next) {
        let mut here = next;
        while let Some(n) = step(here) {
            if !filled(n) {
                break;
            }
            here = n;
        }
        return here;
    }
    let mut here = next;
    loop {
        if filled(here) {
            return here;
        }
        if beyond(here) {
            return CellRef::new(
                if rows > 0 {
                    MAX_ROWS - 1
                } else if rows < 0 {
                    0
                } else {
                    here.row
                },
                if cols > 0 {
                    MAX_COLS - 1
                } else if cols < 0 {
                    0
                } else {
                    here.col
                },
            );
        }
        match step(here) {
            Some(n) => here = n,
            None => return here,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::edit::Session;
    use unluminous_sheet::Workbook;

    #[test]
    fn ctrl_and_an_arrow_stops_at_the_edges_of_the_data() {
        let mut s = Session::new(Workbook::new(), 0.0);
        for row in [0, 1, 2, 5, 6] {
            s.set_input(0, CellRef::new(row, 0), "1").unwrap();
        }
        let sheet = &s.book.sheets[0];
        assert_eq!(edge_of_data(sheet, CellRef::new(0, 0), 1, 0), CellRef::new(2, 0));
        assert_eq!(edge_of_data(sheet, CellRef::new(2, 0), 1, 0), CellRef::new(5, 0));
        assert_eq!(edge_of_data(sheet, CellRef::new(6, 0), 1, 0), CellRef::new(MAX_ROWS - 1, 0));
        assert_eq!(edge_of_data(sheet, CellRef::new(6, 0), -1, 0), CellRef::new(5, 0));
        assert_eq!(edge_of_data(sheet, CellRef::new(0, 0), 0, 1), CellRef::new(0, MAX_COLS - 1));
    }
}
