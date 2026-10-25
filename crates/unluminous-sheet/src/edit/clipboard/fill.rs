//! The fill commands: dragging the fill handle with every choice of Auto Fill Options, the Series dialog,
//! fill across worksheets, and the extent a double click on the handle fills to.

use super::extras::{moved_rules_in, moved_validation_in, subtract_all};
use super::lists::CustomLists;
use super::paste::Plan;
use super::series::{add_months, add_weekdays, plan, value_at, Context, DateShape, Run, Seed};
use super::trend::{growth_fit, linear_fit, round15};
use super::{PasteKind, PasteSpecial};
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::edit::Session;
use crate::formula::{self, Formula};
use crate::model::Cell;
use crate::value::Value;

/// What a drag of the fill handle does, as Auto Fill Options and `XlAutoFillType` name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillKind {
    /// Continue what the cells show: numbers repeat, everything else counts on.
    Default,
    /// Repeat the cells.
    Copy,
    /// Continue the series, numbers included.
    Series,
    /// Copy only the formats.
    Formats,
    /// Fill without formatting: the values continue and the formats under them stay.
    Values,
    Days,
    Weekdays,
    Months,
    Years,
    /// A straight line through the numbers.
    LinearTrend,
    /// An exponential curve through the numbers.
    GrowthTrend,
}

impl FillKind {
    /// The value of Excel's `XlAutoFillType`.
    pub fn excel_code(self) -> i32 {
        match self {
            FillKind::Default => 0,
            FillKind::Copy => 1,
            FillKind::Series => 2,
            FillKind::Formats => 3,
            FillKind::Values => 4,
            FillKind::Days => 5,
            FillKind::Weekdays => 6,
            FillKind::Months => 7,
            FillKind::Years => 8,
            FillKind::LinearTrend => 9,
            FillKind::GrowthTrend => 10,
        }
    }

    /// The kind with an Excel `XlAutoFillType` value; `xlFlashFill` (11) is [`Session::flash_fill`].
    pub fn from_excel_code(code: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.excel_code() == code)
    }

    /// Every kind, in Excel's order.
    pub const ALL: [FillKind; 11] = [
        FillKind::Default,
        FillKind::Copy,
        FillKind::Series,
        FillKind::Formats,
        FillKind::Values,
        FillKind::Days,
        FillKind::Weekdays,
        FillKind::Months,
        FillKind::Years,
        FillKind::LinearTrend,
        FillKind::GrowthTrend,
    ];

    /// The words an agent writes: `copy`, `series`, `formats`, `without-formatting`, `days` and so on.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text.trim().to_ascii_lowercase().replace([' ', '_'], "-").as_str() {
            "default" | "auto" | "" => FillKind::Default,
            "copy" | "copy-cells" => FillKind::Copy,
            "series" | "fill-series" => FillKind::Series,
            "formats" | "formatting-only" | "fill-formatting-only" => FillKind::Formats,
            "values" | "without-formatting" | "fill-without-formatting" => FillKind::Values,
            "days" | "fill-days" => FillKind::Days,
            "weekdays" | "fill-weekdays" => FillKind::Weekdays,
            "months" | "fill-months" => FillKind::Months,
            "years" | "fill-years" => FillKind::Years,
            "linear" | "linear-trend" => FillKind::LinearTrend,
            "growth" | "growth-trend" => FillKind::GrowthTrend,
            _ => return None,
        })
    }

    /// Whether the fill steps by a unit of time, which only dates and the names of days and months have.
    pub(super) fn is_by_unit(self) -> bool {
        matches!(self, FillKind::Days | FillKind::Weekdays | FillKind::Months | FillKind::Years)
    }

    /// Whether the cells filled take the formats of the cells they continue.
    fn takes_formats(self) -> bool {
        self != FillKind::Values
    }

    /// Whether the cells filled take new contents.
    fn takes_contents(self) -> bool {
        self != FillKind::Formats
    }
}

/// The unit of a date series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesDate {
    Day,
    Weekday,
    Month,
    Year,
}

/// The type of a series in the Series dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesKind {
    /// Add the step each time.
    Linear,
    /// Multiply by the step each time.
    Growth,
    /// Dates: add the step in days, weekdays, months or years.
    Date(SeriesDate),
    /// Continue the first cells as dragging the fill handle does.
    AutoFill,
}

/// The choices of the Series dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeriesSpec {
    /// Series in rows (each row is a series) rather than in columns.
    pub by_rows: bool,
    pub kind: SeriesKind,
    pub step: f64,
    /// The value a series stops at. A single selected cell with a stop value fills as far as the stop.
    pub stop: Option<f64>,
    /// Fit a line (or a curve) through the numbers already there and fill the whole selection from it.
    pub trend: bool,
}

impl SeriesSpec {
    /// A series of the first cell added to by `step`, down the columns.
    pub fn linear(step: f64) -> Self {
        Self { by_rows: false, kind: SeriesKind::Linear, step, stop: None, trend: false }
    }
}

/// What Fill Across Worksheets copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcrossSheets {
    All,
    Contents,
    Formats,
}

/// The cells of one line of a fill: the seeds, the cells to fill in order, and which way they lie.
struct Line {
    seeds: Vec<CellRef>,
    targets: Vec<CellRef>,
    backwards: bool,
}

impl Session {
    /// Extend a block of cells into a larger range the way dragging the fill handle does.
    pub fn fill_series(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
    ) -> Result<(), String> {
        self.fill_series_with(sheet, source, target, FillKind::Default, &CustomLists::default())
    }

    /// Drag the fill handle with one of the Auto Fill Options. Numbers and dates continue their step, text
    /// ending in a number counts on, day and month names and the lists given continue, formulas move their
    /// references, and anything else repeats. `lists` are the custom lists to continue.
    pub fn fill_series_with(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
        kind: FillKind,
        lists: &CustomLists,
    ) -> Result<(), String> {
        self.drag_fill(sheet, source, target, kind, lists, false)
    }

    /// Drag the fill handle with Ctrl held, which does the opposite of a plain drag: cells that count on are
    /// copied, and a single number counts on instead of being copied.
    pub fn fill_series_with_control(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
        lists: &CustomLists,
    ) -> Result<(), String> {
        self.drag_fill(sheet, source, target, FillKind::Default, lists, true)
    }

    fn drag_fill(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
        kind: FillKind,
        lists: &CustomLists,
        control: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if !target.contains(source.first) || !target.contains(source.last) {
            return Err("the range to fill must contain the cells it starts from".to_owned());
        }
        let vertical = target.rows() > source.rows();
        let horizontal = target.cols() > source.cols();
        if vertical && horizontal {
            return Err("A fill goes one way: down, up, right or left.".to_owned());
        }
        if !vertical && !horizontal {
            return Ok(());
        }
        self.refuse_array_member(sheet, target)?;
        self.refuse_locked(sheet, target)?;
        self.remember_whole("Fill series");
        for line in lines_of_drag(source, target, vertical) {
            let kind = if control { self.opposite_of_default(sheet, &line, lists) } else { kind };
            self.fill_line(sheet, &line, kind, lists);
        }
        if kind.takes_formats() {
            self.fill_validation_and_rules(sheet, source, target, vertical);
        }
        self.recalculate();
        Ok(())
    }

    /// What Ctrl+drag does to a line of seeds: a series where a plain drag copies (a single plain number),
    /// and a copy where a plain drag continues.
    fn opposite_of_default(&self, sheet: usize, line: &Line, lists: &CustomLists) -> FillKind {
        let seeds: Vec<Seed> = line
            .seeds
            .iter()
            .map(|at| self.seed_of(&self.book.sheets[sheet].cell(*at).cloned().unwrap_or_default()))
            .collect();
        let ctx = Context {
            kind: FillKind::Default,
            lists,
            date1904: self.book.date1904,
            single_cell: seeds.len() == 1,
        };
        let continues = plan(&seeds, &ctx).iter().any(|p| !matches!(p.run, Run::Copy));
        if continues {
            FillKind::Copy
        } else {
            FillKind::Series
        }
    }

    /// The range a double click on the fill handle fills: down to the last row of the data beside the
    /// source, or across to the last column of the data above it, `None` when nothing is beside it.
    pub fn fill_handle_extent(&self, sheet: usize, source: Range) -> Option<Range> {
        let s = self.book.sheets.get(sheet)?;
        let filled = |r: u32, c: u32| {
            s.cells.get(&(r, c)).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())
        };
        let below = source.last.row + 1;
        let neighbours: Vec<u32> = [source.first.col.checked_sub(1), Some(source.last.col + 1)]
            .into_iter()
            .flatten()
            .filter(|c| *c < MAX_COLS)
            .collect();
        for col in neighbours {
            if !filled(below, col) {
                continue;
            }
            let mut last = below;
            while last + 1 < MAX_ROWS && filled(last + 1, col) {
                last += 1;
            }
            return Some(Range::new(source.first, CellRef::new(last, source.last.col)));
        }
        None
    }

    /// Fill one line of seeds into the cells after (or before) them.
    fn fill_line(&mut self, sheet: usize, line: &Line, kind: FillKind, lists: &CustomLists) {
        let seeds: Vec<Cell> = line
            .seeds
            .iter()
            .map(|at| self.book.sheets[sheet].cell(*at).cloned().unwrap_or_default())
            .collect();
        let infos: Vec<Seed> = seeds.iter().map(|c| self.seed_of(c)).collect();
        let ctx =
            Context { kind, lists, date1904: self.book.date1904, single_cell: seeds.len() == 1 };
        let pieces = plan(&infos, &ctx);
        let length = seeds.len() as i64;
        for (k, at) in line.targets.iter().enumerate() {
            let place = if line.backwards { -(k as i64 + 1) } else { length + k as i64 };
            let (cycle, i) = (place.div_euclid(length), place.rem_euclid(length) as usize);
            let Some(piece) = pieces.iter().find(|p| i >= p.start && i < p.start + p.len) else {
                continue;
            };
            let offset = i - piece.start;
            let index = cycle * piece.len as i64 + offset as i64;
            let value = value_at(&piece.run, index, cycle, offset, &ctx);
            self.fill_cell(sheet, *at, (line.seeds[i], &seeds[i]), value, kind);
        }
    }

    /// How a fill sees a seed cell.
    fn seed_of(&self, cell: &Cell) -> Seed {
        let shape = match cell.value {
            Value::Number(_) => DateShape::of_format(&self.book.styles.format_code(cell.style)),
            _ => None,
        };
        Seed { value: cell.value.clone(), formula: cell.formula.is_some(), date: shape }
    }

    /// Write one filled cell from the seed it continues.
    fn fill_cell(
        &mut self,
        sheet: usize,
        at: CellRef,
        (seed_at, seed): (CellRef, &Cell),
        value: Option<Value>,
        kind: FillKind,
    ) {
        let existing = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let mut cell = existing.clone();
        if kind.takes_contents() {
            cell.rich = None;
            cell.extra = None;
            match &seed.formula {
                Some(f) => {
                    let moved = formula::shifted(
                        &f.expr,
                        at.row as i64 - seed_at.row as i64,
                        at.col as i64 - seed_at.col as i64,
                    );
                    cell.formula = Some(Formula::from_expr(moved, f.dynamic));
                    cell.value = Value::Empty;
                }
                None => {
                    cell.formula = None;
                    cell.rich = if value.is_none() { seed.rich.clone() } else { None };
                    cell.value = value.unwrap_or_else(|| seed.value.clone());
                }
            }
        }
        if kind.takes_formats() {
            cell.style = seed.style;
        }
        self.book.sheets[sheet].set(at, cell);
        self.engine.changed(&self.book, sheet, at);
    }

    /// Data validation and conditional formats go on to the cells filled, as the formats do.
    fn fill_validation_and_rules(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
        vertical: bool,
    ) {
        let clip = self.build_clip(sheet, source, false);
        let new_cells = filled_blocks(source, target, vertical);
        let s = &mut self.book.sheets[sheet];
        for v in &mut s.validations {
            v.ranges = subtract_all(&v.ranges, &new_cells.iter().map(|b| b.1).collect::<Vec<_>>());
        }
        s.validations.retain(|v| !v.ranges.is_empty());
        for cf in &mut s.conditional {
            cf.ranges =
                subtract_all(&cf.ranges, &new_cells.iter().map(|b| b.1).collect::<Vec<_>>());
        }
        s.conditional.retain(|cf| !cf.ranges.is_empty());
        let mut priority =
            s.conditional.iter().flat_map(|c| &c.rules).map(|r| r.priority).max().unwrap_or(0);
        for (origin, limit) in new_cells {
            let (dr, dc) = (origin.0 - source.first.row as i64, origin.1 - source.first.col as i64);
            for (rel, validation) in &clip.extras.validations {
                if let Some(range) = placed(*rel, origin).and_then(|r| r.intersection(&limit)) {
                    s.validations.push(moved_validation_in(validation, range, dr, dc));
                }
            }
            for rules in &clip.extras.conditional {
                let ranges: Vec<Range> = rules
                    .ranges
                    .iter()
                    .filter_map(|r| placed(*r, origin).and_then(|p| p.intersection(&limit)))
                    .collect();
                if !ranges.is_empty() {
                    priority += 1;
                    s.conditional.push(moved_rules_in(rules, ranges, (dr, dc), &mut priority));
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------------
    // The Series dialog.

    /// The Series dialog on a range: each column (or row) is a series started by its first cell.
    pub fn data_series(
        &mut self,
        sheet: usize,
        range: Range,
        spec: &SeriesSpec,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.refuse_array_member(sheet, range)?;
        self.refuse_locked(sheet, range)?;
        self.remember_whole("Series");
        for cells in series_lines(range, spec) {
            self.series_line(sheet, &cells, spec);
        }
        self.recalculate();
        Ok(())
    }

    fn series_line(&mut self, sheet: usize, cells: &[CellRef], spec: &SeriesSpec) {
        let number = |this: &Session, at: CellRef| this.book.sheets[sheet].value_at(at).as_number();
        let first_style = self.book.sheets[sheet].cell(cells[0]).map_or(0, |c| c.style);
        if spec.kind == SeriesKind::AutoFill {
            return self.series_by_autofill(sheet, cells);
        }
        let seeds: Vec<f64> = cells.iter().map_while(|at| number(self, *at)).collect();
        if spec.trend && seeds.len() >= 2 {
            return self.series_by_trend(sheet, cells, &seeds, spec, first_style);
        }
        let Some(&start) = seeds.first() else { return };
        let mut at_step = 0_i64;
        let mut value = start;
        for at in &cells[1..] {
            at_step += 1;
            let next = next_in_series(start, value, at_step, spec, self.book.date1904);
            if stops_before(value, next, spec.stop) {
                break;
            }
            value = next;
            self.put_number(sheet, *at, value, first_style);
        }
    }

    /// Fill the cells after the filled ones at the start of the line as dragging the handle would.
    fn series_by_autofill(&mut self, sheet: usize, cells: &[CellRef]) {
        let filled = cells
            .iter()
            .take_while(|at| {
                self.book.sheets[sheet]
                    .cell(**at)
                    .is_some_and(|c| !c.value.is_empty() || c.formula.is_some())
            })
            .count();
        if filled == 0 || filled == cells.len() {
            return;
        }
        let line = Line {
            seeds: cells[..filled].to_vec(),
            targets: cells[filled..].to_vec(),
            backwards: false,
        };
        self.fill_line(sheet, &line, FillKind::Default, &CustomLists::default());
    }

    /// The best fit line (or curve) through the numbers at the start of the line, which replaces them and
    /// fills the rest.
    fn series_by_trend(
        &mut self,
        sheet: usize,
        cells: &[CellRef],
        seeds: &[f64],
        spec: &SeriesSpec,
        style: u32,
    ) {
        let growth = spec.kind == SeriesKind::Growth;
        let values: Vec<f64> = if growth {
            let Some((c, m)) = growth_fit(seeds) else { return };
            (0..cells.len()).map(|i| round15((c + m * i as f64).exp())).collect()
        } else {
            let (a, b) = linear_fit(seeds);
            (0..cells.len()).map(|i| round15(a + b * i as f64)).collect()
        };
        for (at, v) in cells.iter().zip(values) {
            self.put_number(sheet, *at, v, style);
        }
    }

    fn put_number(&mut self, sheet: usize, at: CellRef, value: f64, style: u32) {
        let old = self.book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let cell = Cell { value: Value::Number(round15(value)), formula: None, style, ..old };
        self.book.sheets[sheet].set(at, Cell { rich: None, extra: None, ..cell });
        self.engine.changed(&self.book, sheet, at);
    }

    // -----------------------------------------------------------------------------------------------
    // Fill across worksheets.

    /// Copy a range of one sheet to the same place on others: everything, the contents without the
    /// formats, or the formats alone. The sheets to fill are the other sheets of a group.
    pub fn fill_across_sheets(
        &mut self,
        sheet: usize,
        range: Range,
        others: &[usize],
        what: AcrossSheets,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        for o in others {
            self.check_sheet(*o)?;
            self.refuse_locked(*o, range)?;
        }
        self.remember_whole("Fill across worksheets");
        let clip = self.build_clip(sheet, range, false);
        let opts = PasteSpecial::of(match what {
            AcrossSheets::All => PasteKind::All,
            AcrossSheets::Contents => PasteKind::Formulas,
            AcrossSheets::Formats => PasteKind::Formats,
        });
        for o in others.iter().filter(|o| **o != sheet) {
            let plan = Plan::new(&clip, range, false)?;
            self.run_paste(&clip, *o, &plan, &opts);
        }
        Ok(())
    }
}

/// The next value of a series, `step_number` steps from the start.
fn next_in_series(
    start: f64,
    value: f64,
    step_number: i64,
    spec: &SeriesSpec,
    date1904: bool,
) -> f64 {
    match spec.kind {
        SeriesKind::Linear => value + spec.step,
        SeriesKind::Growth => value * spec.step,
        SeriesKind::Date(unit) => match unit {
            SeriesDate::Day => start + spec.step * step_number as f64,
            SeriesDate::Weekday => {
                add_weekdays(start, (spec.step * step_number as f64) as i64, date1904)
            }
            SeriesDate::Month => {
                add_months(start, (spec.step * step_number as f64) as i64, false, date1904)
            }
            SeriesDate::Year => {
                add_months(start, (spec.step * 12.0 * step_number as f64) as i64, false, date1904)
            }
        },
        SeriesKind::AutoFill => value,
    }
}

/// Whether a series stops before `next` because it passes the stop value.
fn stops_before(value: f64, next: f64, stop: Option<f64>) -> bool {
    let Some(stop) = stop else { return false };
    if next > value {
        next > stop
    } else if next < value {
        next < stop
    } else {
        false
    }
}

/// The lines of a drag: one for each column (or row) of the source.
fn lines_of_drag(source: Range, target: Range, vertical: bool) -> Vec<Line> {
    let mut out = Vec::new();
    if vertical {
        let backwards = target.first.row < source.first.row;
        let rows: Vec<u32> = if backwards {
            (target.first.row..source.first.row).rev().collect()
        } else {
            (source.last.row + 1..=target.last.row).collect()
        };
        for col in source.first.col..=source.last.col {
            out.push(Line {
                seeds: (source.first.row..=source.last.row).map(|r| CellRef::new(r, col)).collect(),
                targets: rows.iter().map(|r| CellRef::new(*r, col)).collect(),
                backwards,
            });
        }
    } else {
        let backwards = target.first.col < source.first.col;
        let cols: Vec<u32> = if backwards {
            (target.first.col..source.first.col).rev().collect()
        } else {
            (source.last.col + 1..=target.last.col).collect()
        };
        for row in source.first.row..=source.last.row {
            out.push(Line {
                seeds: (source.first.col..=source.last.col).map(|c| CellRef::new(row, c)).collect(),
                targets: cols.iter().map(|c| CellRef::new(row, *c)).collect(),
                backwards,
            });
        }
    }
    out
}

/// The cells of each series of the Series dialog, in order.
fn series_lines(range: Range, spec: &SeriesSpec) -> Vec<Vec<CellRef>> {
    let mut lines = Vec::new();
    if spec.by_rows {
        for row in range.first.row..=range.last.row {
            lines.push((range.first.col..=range.last.col).map(|c| CellRef::new(row, c)).collect());
        }
    } else {
        for col in range.first.col..=range.last.col {
            lines.push((range.first.row..=range.last.row).map(|r| CellRef::new(r, col)).collect());
        }
    }
    if range.len() == 1 {
        if let Some(line) = single_cell_line(range.first, spec) {
            return vec![line];
        }
    }
    lines
}

/// A single selected cell with a stop value fills as far as the stop, down or along.
fn single_cell_line(at: CellRef, spec: &SeriesSpec) -> Option<Vec<CellRef>> {
    spec.stop?;
    let limit = 100_000;
    let cells = (0..limit).map_while(|k| {
        if spec.by_rows {
            (at.col + k < MAX_COLS).then(|| CellRef::new(at.row, at.col + k))
        } else {
            (at.row + k < MAX_ROWS).then(|| CellRef::new(at.row + k, at.col))
        }
    });
    Some(cells.collect())
}

/// The blocks a fill covers, each as the place its top left corner would be if the block were whole (it can
/// begin off the sheet) and the part of it that is in the target. A block is as big as the source.
fn filled_blocks(source: Range, target: Range, vertical: bool) -> Vec<((i64, i64), Range)> {
    let mut out = Vec::new();
    let (rows, cols) = (source.rows() as i64, source.cols() as i64);
    let (r0, c0) = (source.first.row as i64, source.first.col as i64);
    let part = |top: i64, left: i64| {
        let whole = Range::new(
            CellRef::new(top.max(0) as u32, left.max(0) as u32),
            CellRef::new((top + rows - 1).max(0) as u32, (left + cols - 1).max(0) as u32),
        );
        whole.intersection(&target).map(|limit| ((top, left), limit))
    };
    let mut push = |block: Option<((i64, i64), Range)>| out.extend(block);
    if vertical {
        let mut top = source.last.row as i64 + 1;
        while top <= target.last.row as i64 {
            push(part(top, c0));
            top += rows;
        }
        let mut top = r0 - rows;
        while top + rows > target.first.row as i64 {
            push(part(top, c0));
            top -= rows;
        }
    } else {
        let mut left = source.last.col as i64 + 1;
        while left <= target.last.col as i64 {
            push(part(r0, left));
            left += cols;
        }
        let mut left = c0 - cols;
        while left + cols > target.first.col as i64 {
            push(part(r0, left));
            left -= cols;
        }
    }
    out
}

/// A range counted from a block's top left, placed with that corner at the origin, which can be off the
/// sheet; the part of it on the sheet, if any.
fn placed(rel: Range, origin: (i64, i64)) -> Option<Range> {
    let (top, left) = (origin.0 + rel.first.row as i64, origin.1 + rel.first.col as i64);
    let (bottom, right) = (origin.0 + rel.last.row as i64, origin.1 + rel.last.col as i64);
    if bottom < 0 || right < 0 {
        return None;
    }
    Some(Range::new(
        CellRef::new(top.max(0) as u32, left.max(0) as u32),
        CellRef::new(bottom as u32, right as u32),
    ))
}
