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
    /// Backspace was pressed, so AutoComplete does not finish the word, as in Excel.
    pub no_complete: bool,
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
    /// The fields of a larger box, such as Paste Special or Sort, in place of `text` and `second`.
    pub fields: Vec<Field>,
}

impl Prompt {
    /// A box of several fields that runs `verb` with what they say when OK is pressed.
    pub fn form(verb: Vec<String>, title: impl Into<String>, fields: Vec<Field>) -> Prompt {
        Prompt {
            verb,
            title: title.into(),
            label: String::new(),
            text: String::new(),
            second: None,
            focus: true,
            fields,
        }
    }

    /// The words OK runs: the verb, then what the fields say.
    pub fn answer(&self) -> Vec<String> {
        let mut words = self.verb.clone();
        if self.fields.is_empty() {
            words.push(self.text.clone());
            if let Some((_, second)) = &self.second {
                if !second.is_empty() {
                    words.push(second.clone());
                }
            }
            return words;
        }
        for field in &self.fields {
            field.push_words(&mut words);
        }
        words
    }

    /// The field a word names: its key, or its label with spaces written as dashes.
    pub fn field_mut(&mut self, name: &str) -> Option<&mut Field> {
        let wanted = name.to_lowercase().replace(' ', "-");
        self.fields.iter_mut().find(|f| {
            f.key.eq_ignore_ascii_case(&wanted)
                || f.label.to_lowercase().replace(' ', "-") == wanted
        })
    }
}

/// What a field of a box takes.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    /// Typed text.
    Text,
    /// One of these, each a label and the word it puts into the command.
    Choice(Vec<(String, String)>),
    /// A switch: on puts the field's key into the command.
    Check,
}

/// One field of a box.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub label: String,
    /// The word the command reads the field by.
    pub key: String,
    pub kind: FieldKind,
    /// The text typed, the word of the choice made, or `on` for a switch that is on.
    pub value: String,
    /// The value goes into the command alone, without its key in front.
    pub positional: bool,
    /// The value goes into the command even when it is empty, so the fields after it keep their places,
    /// as a function's arguments do.
    pub always: bool,
}

impl Field {
    pub fn text(label: &str, key: &str, value: &str) -> Field {
        Field {
            label: label.to_owned(),
            key: key.to_owned(),
            kind: FieldKind::Text,
            value: value.to_owned(),
            positional: false,
            always: false,
        }
    }

    /// A text field that keeps its place in the command even when it is empty.
    pub fn argument(label: &str, value: &str) -> Field {
        Field { always: true, ..Field::bare(label, value) }
    }

    /// A text field whose value goes into the command with no key in front.
    pub fn bare(label: &str, value: &str) -> Field {
        Field {
            positional: true,
            ..Field::text(label, &label.to_lowercase().replace(' ', "-"), value)
        }
    }

    pub fn choice(label: &str, key: &str, options: &[(&str, &str)], value: &str) -> Field {
        Field {
            label: label.to_owned(),
            key: key.to_owned(),
            kind: FieldKind::Choice(
                options.iter().map(|(l, w)| ((*l).to_owned(), (*w).to_owned())).collect(),
            ),
            value: value.to_owned(),
            positional: true,
            always: false,
        }
    }

    /// A choice whose options are made at run time, such as the sheets of the workbook.
    pub fn choices(label: &str, key: &str, options: Vec<(String, String)>, value: &str) -> Field {
        Field {
            label: label.to_owned(),
            key: key.to_owned(),
            kind: FieldKind::Choice(options),
            value: value.to_owned(),
            positional: true,
            always: false,
        }
    }

    pub fn check(label: &str, key: &str, on: bool) -> Field {
        Field {
            label: label.to_owned(),
            key: key.to_owned(),
            kind: FieldKind::Check,
            value: if on { "on".to_owned() } else { String::new() },
            positional: false,
            always: false,
        }
    }

    pub fn is_on(&self) -> bool {
        self.value == "on"
    }

    /// Put what the field says into the command's words.
    fn push_words(&self, words: &mut Vec<String>) {
        match &self.kind {
            FieldKind::Check => {
                if self.is_on() {
                    words.push(self.key.clone());
                }
            }
            FieldKind::Choice(_) => {
                if !self.value.is_empty() {
                    words.push(self.value.clone());
                }
            }
            FieldKind::Text => {
                let value = self.value.trim();
                if value.is_empty() && !self.always {
                    return;
                }
                if !self.positional {
                    words.push(self.key.clone());
                }
                words.push(value.to_owned());
            }
        }
    }
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
    /// Pulling the fill handle; with the right button the series menu opens at the end.
    Fill { right: bool },
    /// Carrying the chosen cells by their border: where in the range they were picked up, and whether the
    /// right button carries them, which asks what to do at the end.
    Move { rows: u32, cols: u32, right: bool },
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
    /// How far the cells past the frozen ones are scrolled, in points at a zoom of one. In a split window
    /// this is the bottom right pane's scroll.
    pub scroll: egui::Vec2,
    /// A split window's other scroll: across for the panes at the left, down for the panes at the top.
    pub split_scroll: egui::Vec2,
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
    /// The Sort popup's levels, trace arrows, the Evaluate Formula stepper and the review panels.
    pub audit: super::view_a::AuditState,
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
    /// Extend or Add mode, which F8 and Shift+F8 turn on, shown in the status area.
    pub selecting: SelectMode,
    /// End was pressed, so the next arrow goes to the edge of the data.
    pub end_mode: bool,
    /// The formula bar shows several lines, which Ctrl+Shift+U turns on and off.
    pub tall_bar: bool,
    /// Where Enter moves the active cell, from the plugin's settings.
    pub enter: super::config::EnterDirection,
    /// The figures the status area shows about the selection, from the plugin's settings.
    pub aggregates: Vec<String>,
    /// Whether typing completes from the column, from the plugin's settings.
    pub autocomplete: bool,
    /// The menu a right drag opened where it let go.
    pub drop_menu: Option<(egui::Pos2, Vec<super::catalogue::Item>)>,
}

/// How the arrow keys change the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectMode {
    #[default]
    Normal,
    /// The arrows grow the selection, as if Shift were held.
    Extend,
    /// The next range chosen is added to the ones already chosen.
    Add,
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
            split_scroll: egui::Vec2::ZERO,
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
            audit: Default::default(),
            chart: None,
            choices: false,
            reveal: false,
            take_keys: false,
            axes: None,
            drawn: None,
            selecting: SelectMode::Normal,
            end_mode: false,
            tall_bar: false,
            enter: super::config::EnterDirection::Down,
            aggregates: super::config::Config::default().aggregates,
            autocomplete: true,
            drop_menu: None,
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

    /// Begin typing into a cell with some text, as typing over it does.
    pub fn begin_edit(&mut self, at: CellRef, text: String) {
        self.edit = Some(Edit {
            sheet: self.sheet,
            at,
            text,
            typed: true,
            in_bar: false,
            pointing: None,
            focus: true,
            caret: None,
            chosen: 0,
            no_complete: true,
        });
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
        self.drop_menu = None;
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
                | Num7
                | Num8
                | E
                | O
                | P
                | Q
                | V
                | Quote
                | Period
                | Backspace
                | F2
                | F3
                | F9
        );
    }
    if modifiers.alt {
        return matches!(
            key,
            Equals
                | Plus
                | ArrowDown
                | ArrowLeft
                | ArrowRight
                | Enter
                | Q
                | F1
                | F10
                | PageUp
                | PageDown
        );
    }
    matches!(key, F2 | F3 | F4 | F5 | F7 | F8 | F9 | F10 | F11 | F12 | Delete | Space | End)
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
