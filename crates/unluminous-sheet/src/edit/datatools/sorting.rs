//! Sorting a range by several levels, the way Excel's Sort does it.
//!
//! A level sorts by values, by the colour a cell is filled with, by its font colour or by its icon, ascending
//! or descending, and a values level may follow a custom list. The sort is stable: rows that are equal on
//! every level keep the order they had. Empty cells go last whichever way the level runs. Rows that are
//! hidden stay where they are, and the visible rows are put in order among the places the visible rows had.
//! A formula that moves with its row keeps its relative references, so `=A2*2` moved from row 2 to row 4
//! reads `=A4*2`; a reference that names a sheet is not changed.

use std::cmp::Ordering;

use super::collate::{self, Key};
use super::displayed;
use crate::coord::{CellRef, Range};
use crate::edit::{clip_to_used, Session};
use crate::formula::{self, Expr, Formula};
use crate::model::*;
use crate::styles::{Color, Dxf};
use crate::value::Value;

/// What a sort level compares.
#[derive(Debug, Clone, PartialEq)]
pub enum SortOn {
    Values,
    /// Cells filled with this colour first (or last when descending); `None` is the cells with no fill.
    CellColor(Option<Color>),
    /// Cells whose text is this colour; `None` is automatic text.
    FontColor(Option<Color>),
    /// Cells showing this icon of an icon set (`3TrafficLights1`, 2); `None` is the cells with no icon.
    Icon(Option<(String, u32)>),
}

/// One level of a sort.
#[derive(Debug, Clone, PartialEq)]
pub struct SortLevel {
    /// The column that is compared (the row, when sorting left to right), as a sheet column or row index.
    pub key: u32,
    pub on: SortOn,
    pub descending: bool,
    /// The items of a custom list such as `Low, Medium, High`: cells equal to one come first in the list's
    /// order, and the rest follow in the usual order. Used by a values level.
    pub custom_list: Option<Vec<String>>,
}

impl SortLevel {
    pub fn values(key: u32, descending: bool) -> Self {
        Self { key, on: SortOn::Values, descending, custom_list: None }
    }
}

/// Whether the first row (column) of the range is a header that stays where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeaderMode {
    /// Decide the way Excel does ([`Session::guess_header`]).
    #[default]
    Guess,
    Yes,
    No,
}

/// The options of the Sort dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortOptions {
    pub header: HeaderMode,
    pub case_sensitive: bool,
    /// Sort the columns by the values in a row, rather than the rows by the values in a column.
    pub left_to_right: bool,
    /// Record the levels on the filter or the table the range is the data of, which is what Excel does for a
    /// sort made from the filter's menu or the Sort dialog. A sort made through the Worksheet.Sort method
    /// leaves nothing behind.
    pub remember: bool,
}

impl Default for SortOptions {
    fn default() -> Self {
        Self {
            header: HeaderMode::Guess,
            case_sensitive: false,
            left_to_right: false,
            remember: true,
        }
    }
}

/// What a cell shows on top of its own format: the conditional formats the window evaluates. The default
/// answers from the cell's own format, which is all the engine knows.
pub trait CellDisplay {
    fn fill(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<Color> {
        let style = book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        book.styles.fill_of(style).paint()
    }

    fn font_color(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<Color> {
        let style = book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        let color = book.styles.font_of(style).color;
        (!matches!(color, Color::Auto | Color::Theme(1, _))).then_some(color)
    }

    /// The icon set and the icon (counted from 0 at the lowest) a cell shows, if any.
    fn icon(&self, _book: &Workbook, _sheet: usize, _at: CellRef) -> Option<(String, u32)> {
        None
    }
}

/// The cell display that knows only each cell's own format.
pub struct DirectFormats;

impl CellDisplay for DirectFormats {}

pub(crate) const MERGE_MESSAGE: &str = "To do this, all the merged cells need to be the same size.";
const REFERENCE_MESSAGE: &str = "The sort reference is not valid. Make sure that it's within the data you want to sort, and the first Sort By box isn't the same or blank.";
const MOST_LEVELS: usize = 64;

/// A sort key read from a cell.
#[derive(Debug, Clone)]
enum Ranked {
    Empty,
    Number(f64),
    Text(Key),
    Bool(bool),
    Error,
}

impl Ranked {
    fn rank(&self) -> u8 {
        match self {
            Ranked::Number(_) => 0,
            Ranked::Text(_) => 1,
            Ranked::Bool(_) => 2,
            Ranked::Error => 3,
            Ranked::Empty => 4,
        }
    }

    fn of(value: &Value) -> Ranked {
        match value {
            Value::Empty => Ranked::Empty,
            Value::Number(n) => Ranked::Number(*n),
            Value::Text(t) => Ranked::Text(collate::key(t)),
            Value::Bool(b) => Ranked::Bool(*b),
            Value::Error(_) => Ranked::Error,
        }
    }
}

/// What one level contributes to the order of one row.
#[derive(Debug, Clone)]
struct LevelKey {
    ranked: Ranked,
    /// Where the cell's text stands in the level's custom list.
    listed: Option<usize>,
    /// For a colour or icon level: whether the cell shows the colour or icon sorted on top.
    matches: bool,
}

/// The sort region split into the lines that move.
struct Plan {
    /// The first and last line (row, or column when sorting left to right) of the data, header excluded.
    first: u32,
    last: u32,
    /// The lines in one unit: rows that are merged in blocks are sorted a block at a time.
    block: u32,
    /// The extent across the lines.
    across: (u32, u32),
    columns: bool,
}

impl Plan {
    fn at(&self, line: u32, across: u32) -> (u32, u32) {
        if self.columns {
            (across, line)
        } else {
            (line, across)
        }
    }
}

impl Session {
    /// Sort the rows of a range by some of its columns, each ascending or descending. `header` leaves the
    /// first row where it is. `keys` are column offsets within the range with `true` for ascending.
    pub fn sort(
        &mut self,
        sheet: usize,
        range: Range,
        keys: &[(u32, bool)],
        header: bool,
    ) -> Result<(), String> {
        if keys.is_empty() {
            return Err("sort needs a column to sort by".to_owned());
        }
        let levels: Vec<SortLevel> = keys
            .iter()
            .map(|(offset, ascending)| SortLevel::values(range.first.col + offset, !*ascending))
            .collect();
        let options = SortOptions {
            header: if header { HeaderMode::Yes } else { HeaderMode::No },
            remember: false,
            ..SortOptions::default()
        };
        self.sort_levels(sheet, range, &levels, &options)
    }

    /// Sort a range by levels, knowing only each cell's own format.
    pub fn sort_levels(
        &mut self,
        sheet: usize,
        range: Range,
        levels: &[SortLevel],
        options: &SortOptions,
    ) -> Result<(), String> {
        self.sort_with(sheet, range, levels, options, &DirectFormats)
    }

    /// Sort a range by levels. `display` answers for the colours and icons conditional formats give.
    pub fn sort_with(
        &mut self,
        sheet: usize,
        range: Range,
        levels: &[SortLevel],
        options: &SortOptions,
        display: &dyn CellDisplay,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if levels.is_empty() || levels.len() > MOST_LEVELS {
            return Err(REFERENCE_MESSAGE.to_owned());
        }
        let range = clip_to_used(&self.book.sheets[sheet], range);
        self.refuse_array_member(sheet, range)?;
        self.refuse_locked(sheet, range)?;
        let header = match options.header {
            HeaderMode::Yes => true,
            HeaderMode::No => false,
            HeaderMode::Guess => self.guess_header(sheet, range, options.left_to_right),
        };
        let plan = self.sort_plan(sheet, range, header, options.left_to_right)?;
        let keys = self.read_keys(sheet, &plan, levels, display)?;
        let Some(order) = self.order_of(sheet, &plan, levels, &keys, options) else {
            return Ok(());
        };
        self.remember_whole("Sort");
        let dxfs = self.dxfs_for(levels);
        self.move_lines(sheet, &plan, &order);
        if options.remember {
            self.remember_sort(sheet, &plan, levels, options, &dxfs);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Whether Excel would treat the first row (column) of a range as a header. The answer is yes when a
    /// column has text in its first cell and something that is not text in the second, or when the first
    /// row is formatted differently from the second.
    pub fn guess_header(&self, sheet: usize, range: Range, left_to_right: bool) -> bool {
        super::header::guess(&self.book, sheet, range, left_to_right)
    }

    /// Work out which lines move and how the merged cells constrain them.
    fn sort_plan(
        &self,
        sheet: usize,
        range: Range,
        header: bool,
        columns: bool,
    ) -> Result<Plan, String> {
        let (first_line, last_line, across) = if columns {
            (range.first.col, range.last.col, (range.first.row, range.last.row))
        } else {
            (range.first.row, range.last.row, (range.first.col, range.last.col))
        };
        let first = first_line + u32::from(header);
        let mut plan = Plan { first, last: last_line, block: 1, across, columns };
        if first > last_line {
            return Ok(plan);
        }
        let data = if columns {
            Range::new(CellRef::new(across.0, first), CellRef::new(across.1, last_line))
        } else {
            Range::new(CellRef::new(first, across.0), CellRef::new(last_line, across.1))
        };
        plan.block = self.block_size(sheet, data, columns)?;
        Ok(plan)
    }

    /// The number of lines a merged block spans when every cell of the data is in a merged block of one
    /// size; one when there are no merged cells. Anything else is refused as Excel refuses it.
    fn block_size(&self, sheet: usize, data: Range, columns: bool) -> Result<u32, String> {
        let s = &self.book.sheets[sheet];
        let merges: Vec<Range> = s.merges.iter().copied().filter(|m| m.intersects(&data)).collect();
        if merges.is_empty() {
            return Ok(1);
        }
        let (rows, cols) = (merges[0].rows(), merges[0].cols());
        let uniform = merges.iter().all(|m| m.rows() == rows && m.cols() == cols);
        let inside = merges.iter().all(|m| data.contains(m.first) && data.contains(m.last));
        let covers = merges.iter().map(|m| m.len()).sum::<u64>() == data.len();
        if !(uniform && inside && covers) {
            return Err(MERGE_MESSAGE.to_owned());
        }
        Ok(if columns { cols } else { rows })
    }

    /// Read what each level compares for the first line of every unit.
    fn read_keys(
        &self,
        sheet: usize,
        plan: &Plan,
        levels: &[SortLevel],
        display: &dyn CellDisplay,
    ) -> Result<Vec<Vec<LevelKey>>, String> {
        for level in levels {
            if level.key < plan.across.0 || level.key > plan.across.1 {
                return Err(REFERENCE_MESSAGE.to_owned());
            }
        }
        let mut keys = Vec::new();
        let mut line = plan.first;
        while line <= plan.last && plan.first <= plan.last {
            let row: Vec<LevelKey> = levels
                .iter()
                .map(|level| {
                    let (r, c) = plan.at(line, level.key);
                    self.level_key(sheet, CellRef::new(r, c), level, display)
                })
                .collect();
            keys.push(row);
            line += plan.block;
        }
        Ok(keys)
    }

    fn level_key(
        &self,
        sheet: usize,
        at: CellRef,
        level: &SortLevel,
        display: &dyn CellDisplay,
    ) -> LevelKey {
        let book = &self.book;
        let theme = &book.styles.theme;
        let same = |shown: Option<Color>, wanted: &Option<Color>| match (shown, wanted) {
            (None, None) => true,
            (Some(a), Some(b)) => a.resolve(theme) == b.resolve(theme),
            _ => false,
        };
        let matches = match &level.on {
            SortOn::Values => false,
            SortOn::CellColor(color) => same(display.fill(book, sheet, at), color),
            SortOn::FontColor(color) => same(display.font_color(book, sheet, at), color),
            SortOn::Icon(icon) => display.icon(book, sheet, at) == *icon,
        };
        let cell = book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let listed = level.custom_list.as_ref().and_then(|list| {
            let text = displayed(book, &cell);
            list.iter().position(|item| item.trim().eq_ignore_ascii_case(text.trim()))
        });
        LevelKey { ranked: Ranked::of(&cell.value), listed, matches }
    }

    /// The units in their new order, as the index of the unit that goes in each place. `None` when the
    /// order is the one there is.
    fn order_of(
        &self,
        sheet: usize,
        plan: &Plan,
        levels: &[SortLevel],
        keys: &[Vec<LevelKey>],
        options: &SortOptions,
    ) -> Option<Vec<usize>> {
        if plan.first > plan.last {
            return None;
        }
        let s = &self.book.sheets[sheet];
        let hidden = |line: u32| {
            if plan.columns {
                s.is_column_hidden(line)
            } else {
                s.is_row_hidden(line)
            }
        };
        let movable: Vec<usize> =
            (0..keys.len()).filter(|u| !hidden(plan.first + *u as u32 * plan.block)).collect();
        let mut sorted = movable.clone();
        sorted.sort_by(|a, b| compare_units(levels, &keys[*a], &keys[*b], options.case_sensitive));
        if sorted == movable {
            return None;
        }
        // Every unit stays in its place unless it is movable; the sorted movable units fill the movable
        // places in order.
        let mut order: Vec<usize> = (0..keys.len()).collect();
        for (place, unit) in movable.iter().zip(sorted) {
            order[*place] = unit;
        }
        Some(order)
    }

    /// Move the cells of each unit to the place `order` gives it.
    fn move_lines(&mut self, sheet: usize, plan: &Plan, order: &[usize]) {
        let mut moved: Vec<((u32, u32), Cell)> = Vec::new();
        let mut notes: Vec<((u32, u32), Note)> = Vec::new();
        let mut links: Vec<(usize, Range)> = Vec::new();
        let mut touched: Vec<(u32, u32)> = Vec::new();
        let s = &self.book.sheets[sheet];
        for (place, unit) in order.iter().enumerate() {
            if place == *unit {
                continue;
            }
            for within in 0..plan.block {
                let from_line = plan.first + *unit as u32 * plan.block + within;
                let to_line = plan.first + place as u32 * plan.block + within;
                let delta = to_line as i64 - from_line as i64;
                for across in plan.across.0..=plan.across.1 {
                    let from = plan.at(from_line, across);
                    let to = plan.at(to_line, across);
                    touched.push(to);
                    if let Some(mut cell) = s.cells.get(&from).cloned() {
                        cell.formula =
                            cell.formula.map(|f| shifted_formula(&f, delta, plan.columns));
                        moved.push((to, cell));
                    }
                    if let Some(note) = s.notes.get(&from) {
                        notes.push((to, note.clone()));
                    }
                    for (i, link) in s.links.iter().enumerate() {
                        if link.range == Range::cell(CellRef::new(from.0, from.1)) {
                            links.push((i, Range::cell(CellRef::new(to.0, to.1))));
                        }
                    }
                }
            }
        }
        let s = &mut self.book.sheets[sheet];
        for key in &touched {
            s.cells.remove(key);
            s.notes.remove(key);
        }
        for (key, cell) in moved {
            s.cells.insert(key, cell);
        }
        for (key, note) in notes {
            s.notes.insert(key, note);
        }
        for (i, range) in links {
            s.links[i].range = range;
        }
        s.notes_changed = true;
    }

    /// The differential formats the colour levels need, one for each, in level order.
    fn dxfs_for(&mut self, levels: &[SortLevel]) -> Vec<Option<u32>> {
        levels
            .iter()
            .map(|level| match &level.on {
                SortOn::CellColor(color) => {
                    Some(self.dxf_with(Dxf {
                        fill: Some(color.unwrap_or(Color::Auto)),
                        ..Dxf::default()
                    }))
                }
                SortOn::FontColor(color) => Some(self.dxf_with(Dxf {
                    font_color: Some(color.unwrap_or(Color::Auto)),
                    ..Dxf::default()
                })),
                _ => None,
            })
            .collect()
    }

    /// The index of an equal differential format, adding it when there is none. A colour filter or a colour
    /// sort names its colour by the differential format that holds it.
    pub fn add_differential_format(&mut self, dxf: Dxf) -> u32 {
        self.dxf_with(dxf)
    }

    /// The index of an equal differential format, adding it when there is none.
    pub(crate) fn dxf_with(&mut self, dxf: Dxf) -> u32 {
        let dxfs = &mut self.book.styles.dxfs;
        if let Some(i) = dxfs.iter().position(|d| *d == dxf) {
            return i as u32;
        }
        dxfs.push(dxf);
        (dxfs.len() - 1) as u32
    }

    /// Write the levels onto the filter or the table the sorted data belongs to.
    fn remember_sort(
        &mut self,
        sheet: usize,
        plan: &Plan,
        levels: &[SortLevel],
        options: &SortOptions,
        dxfs: &[Option<u32>],
    ) {
        if plan.first > plan.last {
            return;
        }
        let data = if plan.columns {
            Range::new(
                CellRef::new(plan.across.0, plan.first),
                CellRef::new(plan.across.1, plan.last),
            )
        } else {
            Range::new(
                CellRef::new(plan.first, plan.across.0),
                CellRef::new(plan.last, plan.across.1),
            )
        };
        let state = SortState {
            range: data,
            conditions: levels
                .iter()
                .zip(dxfs)
                .map(|(level, dxf)| condition_of(level, data, plan.columns, *dxf))
                .collect(),
            columns: plan.columns,
            case_sensitive: options.case_sensitive,
            attrs: Vec::new(),
        };
        if let Some(filter) = self.book.sheets[sheet].filter.as_mut() {
            let r = filter.range;
            let is_data = r.first.row + 1 == data.first.row
                && r.last.row == data.last.row
                && r.first.col <= data.first.col
                && data.last.col <= r.last.col;
            if is_data && !plan.columns {
                filter.sort = Some(state.clone());
            }
        }
        for i in 0..self.book.tables.len() {
            let t = &self.book.tables[i];
            if t.sheet == sheet && t.data_range() == Some(data) && !plan.columns {
                super::tables::set_sort_state(&mut self.book.tables[i], &state);
            }
        }
    }
}

/// The `sortCondition` of a level over a data range.
fn condition_of(level: &SortLevel, data: Range, columns: bool, dxf: Option<u32>) -> SortCondition {
    let range = if columns {
        Range::new(CellRef::new(level.key, data.first.col), CellRef::new(level.key, data.last.col))
    } else {
        Range::new(CellRef::new(data.first.row, level.key), CellRef::new(data.last.row, level.key))
    };
    let (by, icon_set, icon_id) = match &level.on {
        SortOn::Values => (SortBy::Value, None, None),
        SortOn::CellColor(_) => (SortBy::CellColor, None, None),
        SortOn::FontColor(_) => (SortBy::FontColor, None, None),
        SortOn::Icon(icon) => {
            (SortBy::Icon, icon.as_ref().map(|i| i.0.clone()), icon.as_ref().map(|i| i.1))
        }
    };
    SortCondition {
        range,
        descending: level.descending,
        by,
        custom_list: level.custom_list.as_ref().map(|l| l.join(",")),
        dxf,
        icon_set,
        icon_id,
    }
}

/// Compare two units on every level in turn.
fn compare_units(levels: &[SortLevel], a: &[LevelKey], b: &[LevelKey], case: bool) -> Ordering {
    for (i, level) in levels.iter().enumerate() {
        let o = compare_level(level, &a[i], &b[i], case);
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

fn compare_level(level: &SortLevel, a: &LevelKey, b: &LevelKey, case: bool) -> Ordering {
    let ordered = match level.on {
        SortOn::Values => {
            // Empty cells go last whichever way the level runs.
            match (&a.ranked, &b.ranked) {
                (Ranked::Empty, Ranked::Empty) => return Ordering::Equal,
                (Ranked::Empty, _) => return Ordering::Greater,
                (_, Ranked::Empty) => return Ordering::Less,
                _ => {}
            }
            compare_values(level, a, b, case)
        }
        // The cells that match come first; the rest keep their order.
        _ => (!a.matches).cmp(&!b.matches),
    };
    if level.descending {
        ordered.reverse()
    } else {
        ordered
    }
}

fn compare_values(level: &SortLevel, a: &LevelKey, b: &LevelKey, case: bool) -> Ordering {
    if level.custom_list.is_some() {
        match (a.listed, b.listed) {
            (Some(x), Some(y)) => return x.cmp(&y),
            (Some(_), None) => return Ordering::Less,
            (None, Some(_)) => return Ordering::Greater,
            (None, None) => {}
        }
    }
    match (&a.ranked, &b.ranked) {
        (Ranked::Number(x), Ranked::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Ranked::Text(x), Ranked::Text(y)) => collate::compare_keys(x, y, case),
        (Ranked::Bool(x), Ranked::Bool(y)) => x.cmp(y),
        (x, y) => x.rank().cmp(&y.rank()),
    }
}

/// A formula moved by `delta` lines: relative references in it move by the same amount, and references that
/// name a sheet are left as they are.
pub(super) fn shifted_formula(f: &Formula, delta: i64, columns: bool) -> Formula {
    let (rows, cols) = if columns { (0, delta) } else { (delta, 0) };
    let expr = f.expr.map_refs(&mut |r| {
        if r.sheet.is_some() {
            return Some(r.clone());
        }
        match formula::shifted(&Expr::Ref(r.clone()), rows, cols) {
            Expr::Ref(moved) => Some(moved),
            _ => None,
        }
    });
    Formula::from_expr(expr, f.dynamic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Session;

    /// Icons by value: 5 and under shows the first icon, over 5 the second.
    struct Halves;

    impl CellDisplay for Halves {
        fn icon(&self, book: &Workbook, sheet: usize, at: CellRef) -> Option<(String, u32)> {
            match book.sheets[sheet].value_at(at) {
                Value::Number(n) => Some(("3Arrows".to_owned(), u32::from(n > 5.0))),
                _ => None,
            }
        }
    }

    fn column(values: &[f64]) -> Session {
        let mut s = Session::new(Workbook::new(), 46000.5);
        for (i, v) in values.iter().enumerate() {
            s.set_value(0, CellRef::new(i as u32 + 1, 0), Value::Number(*v)).unwrap();
        }
        s.set_value(0, CellRef::new(0, 0), Value::text("h")).unwrap();
        s
    }

    fn read(s: &Session, n: u32) -> Vec<f64> {
        (1..=n)
            .map(|r| match s.book.sheets[0].value_at(CellRef::new(r, 0)) {
                Value::Number(v) => v,
                _ => f64::NAN,
            })
            .collect()
    }

    #[test]
    fn a_sort_by_icon_puts_the_cells_showing_that_icon_first_and_keeps_the_others_in_order() {
        let mut s = column(&[3.0, 9.0, 1.0, 7.0]);
        let level = SortLevel {
            key: 0,
            on: SortOn::Icon(Some(("3Arrows".to_owned(), 1))),
            descending: false,
            custom_list: None,
        };
        let options =
            SortOptions { header: HeaderMode::Yes, remember: false, ..SortOptions::default() };
        s.sort_with(0, Range::parse("A1:A5").unwrap(), &[level], &options, &Halves).unwrap();
        assert_eq!(read(&s, 4), vec![9.0, 7.0, 3.0, 1.0]);
    }

    #[test]
    fn a_sort_by_no_icon_puts_the_cells_without_one_first() {
        let mut s = column(&[3.0, 9.0, 1.0, 7.0]);
        s.set_value(0, CellRef::new(2, 0), Value::text("note")).unwrap();
        let level =
            SortLevel { key: 0, on: SortOn::Icon(None), descending: false, custom_list: None };
        let options =
            SortOptions { header: HeaderMode::Yes, remember: false, ..SortOptions::default() };
        s.sort_with(0, Range::parse("A1:A5").unwrap(), &[level], &options, &Halves).unwrap();
        assert_eq!(s.book.sheets[0].value_at(CellRef::new(1, 0)), Value::text("note"));
    }

    #[test]
    fn a_sort_is_one_undo_step() {
        let mut s = column(&[3.0, 1.0, 2.0]);
        s.sort(0, Range::parse("A1:A4").unwrap(), &[(0, true)], true).unwrap();
        assert_eq!(read(&s, 3), vec![1.0, 2.0, 3.0]);
        s.undo().unwrap();
        assert_eq!(read(&s, 3), vec![3.0, 1.0, 2.0]);
    }

    #[test]
    fn more_than_sixty_four_levels_are_refused() {
        let mut s = column(&[3.0, 1.0]);
        let levels: Vec<SortLevel> = (0..65).map(|_| SortLevel::values(0, false)).collect();
        assert!(s
            .sort_levels(0, Range::parse("A1:A3").unwrap(), &levels, &SortOptions::default())
            .is_err());
    }
}
