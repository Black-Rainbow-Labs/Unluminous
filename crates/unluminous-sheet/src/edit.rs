//! Every change a person or an agent makes to a workbook, each one undoable.
//!
//! A [`Session`] is a workbook with its dependency index and its undo history. The window's grid and the
//! `plugins run spreadsheet` verbs both call these functions, so a change made with the pointer and the
//! same change asked for by an agent are one change.
//!
//! **Undo** keeps, for a change to cells, the cells as they were; for a change that moves cells (insert,
//! delete, sort, a sheet operation), the whole workbook as it was. The kept parts of the file are shared,
//! so a copy of a workbook is its cells and nothing else.

use std::collections::BTreeMap;

use crate::calc::Engine;
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::{self, Axis, Expr, Formula};
use crate::model::*;
use crate::styles::{StyleChange, Styles};
use crate::value::Value;

pub mod clipboard;
pub use clipboard::{ClipExtras, PasteKind, PasteOperation, PasteSpecial};

/// What a change put back when undone.
#[derive(Debug, Clone)]
enum Before {
    Cells { sheet: usize, cells: Vec<((u32, u32), Option<Cell>)>, styles: Option<Box<Styles>> },
    Whole(Box<Workbook>),
}

#[derive(Debug, Clone)]
struct Step {
    label: String,
    before: Before,
}

/// What Paste takes from the cells copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasteMode {
    All,
    Values,
    Formulas,
    Formats,
    /// Everything, with rows turned into columns.
    Transpose,
}

/// Cells copied: where they came from and what they held.
#[derive(Debug, Clone)]
pub struct Clip {
    pub sheet: usize,
    pub range: Range,
    pub cells: Vec<(u32, u32, Cell)>,
    /// A cut: pasting moves the cells and the references to them.
    pub cut: bool,
    /// Notes, validation, conditional formats, merges, links, widths and cell formats that came with them.
    pub extras: ClipExtras,
}

impl Clip {
    /// The cells as tab separated text, which is what Excel puts on the clipboard and reads from it.
    pub fn to_text(&self, book: &Workbook) -> String {
        let mut out = String::new();
        let (rows, cols) = self.block_size();
        for r in 0..rows {
            if r > 0 {
                out.push_str("\r\n");
            }
            for c in 0..cols {
                if c > 0 {
                    out.push('\t');
                }
                if let Some((_, _, cell)) =
                    self.cells.iter().find(|(rr, cc, _)| *rr == r && *cc == c)
                {
                    out.push_str(&shown_text(book, cell));
                }
            }
        }
        out.push_str("\r\n");
        out
    }
}

/// A cell as it is shown, with its number format: what copying to another program gives.
pub fn shown_text(book: &Workbook, cell: &Cell) -> String {
    let code = book.styles.format_code(cell.style);
    let text = crate::numfmt::Format::parse(&code).render(&cell.value, book.date1904, 255).text;
    if text.contains(['\t', '\n', '"']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// What a cell is cleared of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clear {
    Contents,
    Formats,
    All,
    Notes,
    Links,
}

/// Which way a fill goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Down,
    Right,
    Up,
    Left,
}

/// Which way the cells beside an inserted or deleted block move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Down,
    Right,
    Up,
    Left,
    WholeRows,
    WholeColumns,
}

/// A workbook being edited.
#[derive(Debug, Clone)]
pub struct Session {
    pub book: Workbook,
    engine: Engine,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Changed since it was opened or saved.
    pub modified: bool,
    /// NOW() as a serial, which the window keeps current.
    pub now: f64,
    /// The year typing `10/23` means.
    pub this_year: i64,
    seed: u64,
}

const UNDO_LIMIT: usize = 100;

impl Session {
    /// Start editing a workbook, computing it first.
    pub fn new(mut book: Workbook, now: f64) -> Self {
        let mut engine = Engine::new(&book);
        engine.mark_all();
        engine.recalculate(&mut book, now, 0x51_7cc1_b727_220a);
        let this_year = crate::dates::date(now, false).map_or(2026, |(y, _, _)| y);
        Self {
            book,
            engine,
            undo: Vec::new(),
            redo: Vec::new(),
            modified: false,
            now,
            this_year,
            seed: 1,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What Undo would undo, for its menu entry.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    fn seed(&mut self) -> u64 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.seed
    }

    /// Compute everything a change affected.
    pub fn recalculate(&mut self) {
        let seed = self.seed();
        if self.book.calc.automatic {
            self.engine.recalculate(&mut self.book, self.now, seed);
        }
    }

    /// Compute every formula, which is what F9 does in manual mode and Ctrl+Alt+F9 always does.
    pub fn recalculate_all(&mut self) {
        let seed = self.seed();
        self.engine.mark_all();
        self.engine.recalculate(&mut self.book, self.now, seed);
    }

    fn push(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.modified = true;
    }

    /// Record the cells of some ranges as they are, before changing them.
    fn remember_cells(&mut self, label: &str, sheet: usize, ranges: &[Range], styles: bool) {
        let s = &self.book.sheets[sheet];
        let mut cells = Vec::new();
        for range in ranges {
            // A range of ordinary size is recorded whole, empty cells included, because the change may fill
            // cells past what is in use now. Only a whole row or column is cut to the cells in use.
            let (last_r, last_c) = if range.len() > 1_000_000 {
                (
                    range.last.row.min(last_row(s).max(range.first.row)),
                    range.last.col.min(last_col(s).max(range.first.col)),
                )
            } else {
                (range.last.row, range.last.col)
            };
            for r in range.first.row..=last_r {
                for c in range.first.col..=last_c {
                    cells.push(((r, c), s.cells.get(&(r, c)).cloned()));
                }
            }
        }
        let styles = styles.then(|| Box::new(self.book.styles.clone()));
        self.push(Step { label: label.to_owned(), before: Before::Cells { sheet, cells, styles } });
    }

    fn remember_whole(&mut self, label: &str) {
        let mut copy = self.book.clone();
        copy.circular = None;
        self.push(Step { label: label.to_owned(), before: Before::Whole(Box::new(copy)) });
    }

    /// Undo the last change.
    pub fn undo(&mut self) -> Option<String> {
        let step = self.undo.pop()?;
        let reverse = self.apply_before(step.before);
        self.redo.push(Step { label: step.label.clone(), before: reverse });
        self.modified = true;
        Some(step.label)
    }

    pub fn redo(&mut self) -> Option<String> {
        let step = self.redo.pop()?;
        let reverse = self.apply_before(step.before);
        self.undo.push(Step { label: step.label.clone(), before: reverse });
        self.modified = true;
        Some(step.label)
    }

    /// Put cells or a workbook back, answering what was there so the opposite can be done.
    fn apply_before(&mut self, before: Before) -> Before {
        match before {
            Before::Cells { sheet, cells, styles } => {
                let mut now = Vec::with_capacity(cells.len());
                let current_styles = styles.as_ref().map(|_| Box::new(self.book.styles.clone()));
                for (key, cell) in cells {
                    let s = &mut self.book.sheets[sheet];
                    now.push((key, s.cells.get(&key).cloned()));
                    match cell {
                        Some(c) => {
                            s.cells.insert(key, c);
                        }
                        None => {
                            s.cells.remove(&key);
                        }
                    }
                    self.engine.changed(&self.book, sheet, CellRef::new(key.0, key.1));
                }
                if let Some(st) = styles {
                    self.book.styles = *st;
                }
                self.recalculate();
                Before::Cells { sheet, cells: now, styles: current_styles }
            }
            Before::Whole(book) => {
                let current = std::mem::replace(&mut self.book, *book);
                self.engine.rebuild(&self.book);
                self.recalculate_all();
                Before::Whole(Box::new(current))
            }
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Cells.

    /// Put what a person typed into a cell: a formula when it starts with `=`, otherwise a value read the
    /// way Excel reads it, with the number format it implies on a cell that has none.
    pub fn set_input(&mut self, sheet: usize, at: CellRef, text: &str) -> Result<(), String> {
        self.set_inputs(sheet, &[(at, text.to_owned())])
    }

    /// Several cells typed at once, as one undo step: what a paste of text and Ctrl+Enter do.
    pub fn set_inputs(&mut self, sheet: usize, inputs: &[(CellRef, String)]) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let mut prepared = Vec::with_capacity(inputs.len());
        for (at, text) in inputs {
            self.refuse_array_member(sheet, Range::cell(*at))?;
            self.refuse_locked(sheet, Range::cell(*at))?;
            prepared.push((*at, self.prepare(sheet, *at, text)?));
        }
        let ranges: Vec<Range> = inputs.iter().map(|(at, _)| Range::cell(*at)).collect();
        self.remember_cells("Typing", sheet, &ranges, true);
        for (at, (value, formula, format)) in prepared {
            let s = &mut self.book.sheets[sheet];
            let old = s.cells.get(&(at.row, at.col)).cloned().unwrap_or_default();
            let mut style = old.style;
            if let Some(code) = format {
                if self.book.styles.format_code(style) == "General" {
                    style =
                        self.book.styles.derive(style, &StyleChange::NumberFormat(code.to_owned()));
                }
            }
            let cell = Cell { value, formula, style, rich: None, extra: old.extra.clone() };
            self.book.sheets[sheet].set(at, cell);
            self.engine.changed(&self.book, sheet, at);
        }
        self.recalculate();
        Ok(())
    }

    fn prepare(
        &self,
        sheet: usize,
        at: CellRef,
        text: &str,
    ) -> Result<(Value, Option<Formula>, Option<&'static str>), String> {
        let style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        let as_text = self.book.styles.format_code(style) == "@";
        if !as_text && text.starts_with('=') && text.len() > 1 {
            let f = Formula::parse(text).map_err(|e| format!("That formula has a problem: {e}"))?;
            return Ok((Value::Empty, Some(f), None));
        }
        if as_text {
            return Ok((
                if text.is_empty() { Value::Empty } else { Value::text(text) },
                None,
                None,
            ));
        }
        let typed = crate::input::read(text, self.book.date1904, self.this_year);
        Ok((typed.value, None, typed.format))
    }

    /// Put a value into a cell directly, with no reading of it: what an agent writing data does.
    pub fn set_value(&mut self, sheet: usize, at: CellRef, value: Value) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.refuse_array_member(sheet, Range::cell(at))?;
        self.remember_cells("Set value", sheet, &[Range::cell(at)], false);
        let s = &mut self.book.sheets[sheet];
        let old = s.cells.get(&(at.row, at.col)).cloned().unwrap_or_default();
        s.set(at, Cell { value, formula: None, style: old.style, rich: None, extra: old.extra });
        self.engine.changed(&self.book, sheet, at);
        self.recalculate();
        Ok(())
    }

    /// Enter a legacy array formula over a range, which is what Ctrl+Shift+Enter does.
    pub fn set_array_formula(
        &mut self,
        sheet: usize,
        range: Range,
        text: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let mut f = Formula::parse(text).map_err(|e| format!("That formula has a problem: {e}"))?;
        f.dynamic = false;
        f.array = Some(range);
        self.remember_cells("Array formula", sheet, &[range], false);
        let s = &mut self.book.sheets[sheet];
        s.array_formulas.retain(|r| !r.intersects(&range));
        s.array_formulas.push(range);
        for at in range.cells() {
            let style = s.cells.get(&(at.row, at.col)).map_or(0, |c| c.style);
            let cell = if at == range.first {
                Cell { formula: Some(f.clone()), style, ..Default::default() }
            } else {
                Cell { value: Value::Number(0.0), style, ..Default::default() }
            };
            s.cells.insert((at.row, at.col), cell);
        }
        self.engine.changed(&self.book, sheet, range.first);
        self.recalculate();
        Ok(())
    }

    /// Clear cells of their contents, their formats, both, their notes or their links.
    pub fn clear(&mut self, sheet: usize, ranges: &[Range], what: Clear) -> Result<(), String> {
        self.check_sheet(sheet)?;
        for r in ranges {
            if what != Clear::Formats {
                self.refuse_array_member(sheet, *r)?;
            }
            self.refuse_locked(sheet, *r)?;
        }
        match what {
            Clear::Notes => {
                self.remember_whole("Clear notes");
                let s = &mut self.book.sheets[sheet];
                s.notes.retain(|(r, c), _| {
                    !ranges.iter().any(|range| range.contains(CellRef::new(*r, *c)))
                });
                s.notes_changed = true;
                return Ok(());
            }
            Clear::Links => {
                self.remember_whole("Clear links");
                let s = &mut self.book.sheets[sheet];
                s.links.retain(|l| !ranges.iter().any(|range| range.intersects(&l.range)));
                return Ok(());
            }
            _ => {}
        }
        self.remember_cells(
            match what {
                Clear::Contents => "Clear contents",
                Clear::Formats => "Clear formats",
                _ => "Clear",
            },
            sheet,
            ranges,
            false,
        );
        let keys: Vec<(u32, u32)> = {
            let s = &self.book.sheets[sheet];
            s.cells
                .keys()
                .filter(|(r, c)| ranges.iter().any(|range| range.contains(CellRef::new(*r, *c))))
                .copied()
                .collect()
        };
        for key in keys {
            let s = &mut self.book.sheets[sheet];
            let Some(cell) = s.cells.get_mut(&key) else { continue };
            match what {
                Clear::Contents => {
                    cell.value = Value::Empty;
                    cell.formula = None;
                    cell.rich = None;
                }
                Clear::Formats => cell.style = 0,
                _ => {
                    cell.value = Value::Empty;
                    cell.formula = None;
                    cell.rich = None;
                    cell.style = 0;
                }
            }
            if cell.is_blank() {
                s.cells.remove(&key);
            }
            self.engine.changed(&self.book, sheet, CellRef::new(key.0, key.1));
        }
        if what != Clear::Formats {
            let s = &mut self.book.sheets[sheet];
            s.array_formulas.retain(|r| !ranges.iter().any(|range| range.contains(r.first)));
        }
        self.recalculate();
        Ok(())
    }

    /// Change one property of the format of every cell in some ranges.
    pub fn set_style(
        &mut self,
        sheet: usize,
        ranges: &[Range],
        change: &StyleChange,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_cells("Format", sheet, ranges, true);
        // Whole rows and columns change the row and column formats rather than every cell.
        for range in ranges {
            let s = &self.book.sheets[sheet];
            let whole_cols = range.is_whole_columns();
            let whole_rows = range.is_whole_rows();
            let keys: Vec<(u32, u32)> = if whole_cols || whole_rows {
                s.cells
                    .keys()
                    .filter(|(r, c)| range.contains(CellRef::new(*r, *c)))
                    .copied()
                    .collect()
            } else {
                range.cells().map(|c| (c.row, c.col)).collect()
            };
            if whole_cols {
                for col in range.first.col..=range.last.col {
                    let base = self.book.sheets[sheet].column(col).map_or(0, |c| c.style);
                    let next = self.book.styles.derive(base, change);
                    self.book.sheets[sheet].edit_column(col, |info| info.style = next);
                }
            }
            if whole_rows {
                for row in range.first.row..=range.last.row {
                    let base =
                        self.book.sheets[sheet].rows.get(&row).and_then(|r| r.style).unwrap_or(0);
                    let next = self.book.styles.derive(base, change);
                    self.book.sheets[sheet].rows.entry(row).or_default().style = Some(next);
                }
            }
            let mut cache: BTreeMap<u32, u32> = BTreeMap::new();
            for key in keys {
                let s = &mut self.book.sheets[sheet];
                let current =
                    s.cells.get(&key).map_or_else(|| inherited_style(s, key), |c| c.style);
                let next = *cache
                    .entry(current)
                    .or_insert_with(|| self.book.styles.derive(current, change));
                let s = &mut self.book.sheets[sheet];
                let cell = s.cells.entry(key).or_default();
                cell.style = next;
                if cell.is_blank() {
                    s.cells.remove(&key);
                }
            }
        }
        self.modified = true;
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------
    // Rows, columns and sheets.

    /// Insert rows or columns, or cells shifting the rest down or right.
    pub fn insert(&mut self, sheet: usize, range: Range, shift: Shift) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let (axis, at, count, band) = match shift {
            Shift::WholeRows | Shift::Down => (
                Axis::Rows,
                range.first.row,
                range.rows() as i64,
                (shift == Shift::Down).then_some((range.first.col, range.last.col)),
            ),
            Shift::WholeColumns | Shift::Right => (
                Axis::Columns,
                range.first.col,
                range.cols() as i64,
                (shift == Shift::Right).then_some((range.first.row, range.last.row)),
            ),
            _ => return Err("insert shifts cells down or right".to_owned()),
        };
        self.refuse_structural(sheet, axis, at, count, band)?;
        self.remember_whole(match axis {
            Axis::Rows => "Insert rows",
            Axis::Columns => "Insert columns",
        });
        self.structural(sheet, axis, at, count, band);
        Ok(())
    }

    /// Delete rows or columns, or cells shifting the rest up or left.
    pub fn delete(&mut self, sheet: usize, range: Range, shift: Shift) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let (axis, at, count, band) = match shift {
            Shift::WholeRows | Shift::Up => (
                Axis::Rows,
                range.first.row,
                -(range.rows() as i64),
                (shift == Shift::Up).then_some((range.first.col, range.last.col)),
            ),
            Shift::WholeColumns | Shift::Left => (
                Axis::Columns,
                range.first.col,
                -(range.cols() as i64),
                (shift == Shift::Left).then_some((range.first.row, range.last.row)),
            ),
            _ => return Err("delete shifts cells up or left".to_owned()),
        };
        self.refuse_structural(sheet, axis, at, count, band)?;
        self.remember_whole(match axis {
            Axis::Rows => "Delete rows",
            Axis::Columns => "Delete columns",
        });
        self.structural(sheet, axis, at, count, band);
        Ok(())
    }

    /// Refuse an insert or a delete that Excel refuses, before anything changes: one that would change part
    /// of a legacy array formula, and one that would delete a table's header row without the table.
    fn refuse_structural(
        &self,
        sheet: usize,
        axis: Axis,
        at: u32,
        count: i64,
        band: Option<(u32, u32)>,
    ) -> Result<(), String> {
        let s = &self.book.sheets[sheet];
        let span = |r: Range| match axis {
            Axis::Rows => (r.first.row, r.last.row, r.first.col, r.last.col),
            Axis::Columns => (r.first.col, r.last.col, r.first.row, r.last.row),
        };
        let in_band = |lo: u32, hi: u32| band.is_none_or(|(b0, b1)| hi >= b0 && lo <= b1);
        for range in &s.array_formulas {
            let (first, last, lo, hi) = span(*range);
            if !in_band(lo, hi) {
                continue;
            }
            let splits = if count > 0 {
                at > first && at <= last
            } else {
                let end = at as i64 - count - 1;
                let touches = (at as i64) <= last as i64 && end >= first as i64;
                let whole = (at as i64) <= first as i64 && end >= last as i64;
                touches && !whole
            };
            if splits {
                return Err("You can't change part of an array.".to_owned());
            }
        }
        if count < 0 {
            let end = at as i64 - count - 1;
            for t in self.book.tables.iter().filter(|t| t.sheet == sheet && t.header_row) {
                let (first, last, lo, hi) = span(t.range);
                if axis != Axis::Rows || !in_band(lo, hi) {
                    continue;
                }
                let header_gone = (at as i64) <= first as i64 && end >= first as i64;
                let all_gone = (at as i64) <= first as i64 && end >= last as i64;
                if header_gone && !all_gone {
                    return Err("This operation is not allowed. The operation is attempting to shift cells in a table on your worksheet.".to_owned());
                }
            }
        }
        Ok(())
    }

    /// Move cells for an insert (`count` positive) or a delete (negative) and rewrite every reference.
    /// `band` limits the move to some columns (rows) for inserting cells rather than whole lines.
    fn structural(
        &mut self,
        sheet: usize,
        axis: Axis,
        at: u32,
        count: i64,
        band: Option<(u32, u32)>,
    ) {
        let name = self.book.sheets[sheet].name.clone();
        crate::refshift::shift_kept(
            &mut self.book,
            &crate::refshift::StructuralEdit::Lines { sheet: name.clone(), axis, at, count, band },
        );
        let limit = match axis {
            Axis::Rows => MAX_ROWS,
            Axis::Columns => MAX_COLS,
        } as i64;
        // Formulas everywhere first, while every cell is still where its formula was written.
        let names: Vec<String> = self.book.sheets.iter().map(|s| s.name.clone()).collect();
        for (si, s) in self.book.sheets.iter_mut().enumerate() {
            for cell in s.cells.values_mut() {
                if let Some(f) = &mut cell.formula {
                    if f.frozen {
                        continue;
                    }
                    let adjusted = match band {
                        None => formula::adjusted(&f.expr, &name, &names[si], axis, at, count),
                        Some(band) => {
                            adjust_band(&f.expr, &name, &names[si], axis, at, count, band)
                        }
                    };
                    let array = f.array.and_then(|r| {
                        if si != sheet {
                            return Some(r);
                        }
                        crate::refshift::shift_range(r, axis, at, count, band)
                    });
                    if adjusted != *f.expr || array != f.array {
                        let (dynamic, attrs) = (f.dynamic, f.attrs.clone());
                        *f = Formula::from_expr(adjusted, dynamic);
                        f.array = array;
                        f.attrs = attrs;
                    }
                }
            }
        }
        for n in &mut self.book.names {
            if let Ok(expr) = formula::parser::parse(&n.formula) {
                let own = n.scope.map_or(name.clone(), |i| names[i].clone());
                let adjusted = formula::adjusted(&expr, &name, &own, axis, at, count);
                if adjusted != expr {
                    n.formula = formula::print::print(&adjusted, true);
                }
            }
        }
        // Then the cells themselves.
        let s = &mut self.book.sheets[sheet];
        let moved = |key: (u32, u32)| -> Option<(u32, u32)> {
            let (pos, other) = match axis {
                Axis::Rows => (key.0, key.1),
                Axis::Columns => (key.1, key.0),
            };
            if let Some((lo, hi)) = band {
                if other < lo || other > hi {
                    return Some(key);
                }
            }
            let pos = pos as i64;
            let at = at as i64;
            let next = if count > 0 {
                if pos >= at {
                    pos + count
                } else {
                    pos
                }
            } else if pos >= at && pos < at - count {
                return None;
            } else if pos >= at - count {
                pos + count
            } else {
                pos
            };
            if next >= limit {
                return None;
            }
            Some(match axis {
                Axis::Rows => (next as u32, key.1),
                Axis::Columns => (key.0, next as u32),
            })
        };
        let cells = std::mem::take(&mut s.cells);
        for (key, cell) in cells {
            if let Some(k) = moved(key) {
                s.cells.insert(k, cell);
            }
        }
        let notes = std::mem::take(&mut s.notes);
        let had_notes = !notes.is_empty();
        for (key, note) in notes {
            if let Some(k) = moved(key) {
                s.notes.insert(k, note);
            }
        }
        if had_notes {
            s.notes_changed = true;
        }
        let shift_range = |r: Range| -> Option<Range> {
            let reference = formula::Reference::from_range(r, None);
            let adjusted = match band {
                None => formula::adjust_reference(&reference, axis, at, count),
                Some(b) => {
                    let (lo, hi) = (b.0, b.1);
                    let (other_lo, other_hi) = match axis {
                        Axis::Rows => (r.first.col, r.last.col),
                        Axis::Columns => (r.first.row, r.last.row),
                    };
                    if other_lo >= lo && other_hi <= hi {
                        formula::adjust_reference(&reference, axis, at, count)
                    } else {
                        Some(reference)
                    }
                }
            };
            adjusted.map(|a| a.range())
        };
        s.merges = s.merges.iter().filter_map(|r| shift_range(*r)).collect();
        s.array_formulas = s.array_formulas.iter().filter_map(|r| shift_range(*r)).collect();
        for cf in &mut s.conditional {
            cf.ranges = cf.ranges.iter().filter_map(|r| shift_range(*r)).collect();
        }
        s.conditional.retain(|cf| !cf.ranges.is_empty());
        for v in &mut s.validations {
            v.ranges = v.ranges.iter().filter_map(|r| shift_range(*r)).collect();
        }
        s.validations.retain(|v| !v.ranges.is_empty());
        s.links = s
            .links
            .iter()
            .filter_map(|l| shift_range(l.range).map(|range| Hyperlink { range, ..l.clone() }))
            .collect();
        if let Some(f) = &s.filter {
            s.filter = shift_range(f.range).map(|range| AutoFilter {
                range,
                columns: f.columns.clone(),
                sort: f.sort.as_ref().and_then(|sort| {
                    let mut sort = sort.clone();
                    sort.range = shift_range(sort.range)?;
                    sort.conditions.retain_mut(|c| match shift_range(c.range) {
                        Some(r) => {
                            c.range = r;
                            true
                        }
                        None => false,
                    });
                    Some(sort)
                }),
            });
        }
        if band.is_none() {
            match axis {
                Axis::Rows => {
                    let rows = std::mem::take(&mut s.rows);
                    for (r, info) in rows {
                        if let Some((nr, _)) = moved((r, 0)) {
                            s.rows.insert(nr, info);
                        }
                    }
                }
                Axis::Columns => {
                    let cols = std::mem::take(&mut s.columns);
                    for mut c in cols {
                        let range = Range::new(CellRef::new(0, c.first), CellRef::new(0, c.last));
                        if let Some(r) = shift_range(range) {
                            c.first = r.first.col;
                            c.last = r.last.col;
                            s.columns.push(c);
                        }
                    }
                }
            }
        }
        s.spills = Spills::default();
        // A table whose every row was deleted goes with them, as it does in Excel.
        self.book.tables.retain_mut(|t| {
            if t.sheet != sheet {
                return true;
            }
            match shift_range(t.range) {
                Some(r) => {
                    t.range = r;
                    true
                }
                None => false,
            }
        });
        self.engine.rebuild(&self.book);
        self.recalculate_all();
    }

    pub fn add_sheet(&mut self, name: Option<&str>, at: Option<usize>) -> Result<usize, String> {
        let name = match name {
            Some(n) => {
                check_sheet_name(n)?;
                if self.book.sheet_index(n).is_some() {
                    return Err(format!("there is already a sheet called {n}"));
                }
                n.to_owned()
            }
            None => self.book.next_sheet_name(),
        };
        self.remember_whole("Add sheet");
        let index = at.unwrap_or(self.book.sheets.len()).min(self.book.sheets.len());
        self.book.sheets.insert(index, Sheet::new(name));
        self.reindex_after_sheet_move(|i| if i >= index { i + 1 } else { i });
        self.book.active_sheet = index;
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(index)
    }

    /// Rename a sheet, rewriting every formula that names it.
    pub fn rename_sheet(&mut self, sheet: usize, name: &str) -> Result<(), String> {
        self.check_sheet(sheet)?;
        check_sheet_name(name)?;
        if self.book.sheet_index(name).is_some_and(|i| i != sheet) {
            return Err(format!("there is already a sheet called {name}"));
        }
        self.remember_whole("Rename sheet");
        let old = self.book.sheets[sheet].name.clone();
        crate::refshift::shift_kept(
            &mut self.book,
            &crate::refshift::StructuralEdit::Rename { from: old.clone(), to: name.to_owned() },
        );
        self.book.sheets[sheet].name = name.to_owned();
        for s in &mut self.book.sheets {
            for cell in s.cells.values_mut() {
                if let Some(f) = &mut cell.formula {
                    if f.frozen {
                        continue;
                    }
                    let renamed = f.expr.rename_sheet(&old, name);
                    if renamed != *f.expr {
                        let (dynamic, array, attrs) = (f.dynamic, f.array, f.attrs.clone());
                        *f = Formula::from_expr(renamed, dynamic);
                        f.array = array;
                        f.attrs = attrs;
                    }
                }
            }
        }
        for n in &mut self.book.names {
            if let Ok(expr) = formula::parser::parse(&n.formula) {
                let renamed = expr.rename_sheet(&old, name);
                if renamed != expr {
                    n.formula = formula::print::print(&renamed, true);
                }
            }
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Delete a sheet. Formulas that named it read `#REF!`.
    pub fn delete_sheet(&mut self, sheet: usize) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if self.book.sheets.iter().filter(|s| s.state == Visibility::Visible).count() <= 1
            && self.book.sheets[sheet].state == Visibility::Visible
        {
            return Err("a workbook needs at least one visible sheet".to_owned());
        }
        self.remember_whole("Delete sheet");
        let name = self.book.sheets[sheet].name.clone();
        crate::refshift::shift_kept(
            &mut self.book,
            &crate::refshift::StructuralEdit::DeleteSheet { name: name.clone() },
        );
        for n in &mut self.book.names {
            if n.scope == Some(sheet) {
                continue;
            }
            n.formula = crate::refshift::shift_formula_text(
                &n.formula,
                "",
                &crate::refshift::StructuralEdit::DeleteSheet { name: name.clone() },
            );
        }
        for s in &mut self.book.sheets {
            for cell in s.cells.values_mut() {
                if let Some(f) = &mut cell.formula {
                    let gone = f.expr.map_refs(&mut |r| match &r.sheet {
                        Some(n) if n.eq_ignore_ascii_case(&name) => None,
                        _ => Some(r.clone()),
                    });
                    if gone != *f.expr {
                        let (dynamic, array, attrs) = (f.dynamic, f.array, f.attrs.clone());
                        *f = Formula::from_expr(gone, dynamic);
                        f.array = array;
                        f.attrs = attrs;
                    }
                }
            }
        }
        self.book.sheets.remove(sheet);
        self.book.tables.retain(|t| t.sheet != sheet);
        self.book.names.retain(|n| n.scope != Some(sheet));
        self.reindex_after_sheet_move(|i| if i > sheet { i - 1 } else { i });
        self.book.active_sheet = self.book.active_sheet.min(self.book.sheets.len() - 1);
        if self.book.sheets[self.book.active_sheet].state != Visibility::Visible {
            self.book.active_sheet =
                self.book.sheets.iter().position(|s| s.state == Visibility::Visible).unwrap_or(0);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Move a sheet to another place among the tabs.
    pub fn move_sheet(&mut self, from: usize, to: usize) -> Result<(), String> {
        self.check_sheet(from)?;
        let to = to.min(self.book.sheets.len() - 1);
        if from == to {
            return Ok(());
        }
        self.remember_whole("Move sheet");
        let sheet = self.book.sheets.remove(from);
        self.book.sheets.insert(to, sheet);
        let map = |i: usize| -> usize {
            if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if to < from && i >= to && i < from {
                i + 1
            } else {
                i
            }
        };
        self.reindex_after_sheet_move(map);
        self.book.active_sheet = to;
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// A copy of a sheet beside it, named `Name (2)`.
    pub fn duplicate_sheet(&mut self, sheet: usize) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Duplicate sheet");
        let mut copy = self.book.sheets[sheet].clone();
        copy.name = (2..)
            .map(|n| format!("{} ({n})", copy.name))
            .find(|n| self.book.sheet_index(n).is_none())
            .unwrap_or_default();
        copy.raw.part = String::new();
        copy.raw.rels.retain(|r| {
            r.kind != crate::xlsx::REL_COMMENTS
                && r.kind != crate::xlsx::REL_VML
                && r.kind != crate::xlsx::REL_DRAWING
        });
        copy.raw
            .elements
            .retain(|(n, _)| n != "drawing" && n != "legacyDrawing" && n != "tableParts");
        copy.notes_changed = true;
        copy.charts.clear();
        copy.state = Visibility::Visible;
        let index = sheet + 1;
        self.book.sheets.insert(index, copy);
        self.reindex_after_sheet_move(|i| if i >= index { i + 1 } else { i });
        self.book.active_sheet = index;
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(index)
    }

    pub fn set_sheet_visibility(&mut self, sheet: usize, state: Visibility) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if state != Visibility::Visible
            && self
                .book
                .sheets
                .iter()
                .enumerate()
                .filter(|(i, s)| *i != sheet && s.state == Visibility::Visible)
                .count()
                == 0
        {
            return Err("a workbook needs at least one visible sheet".to_owned());
        }
        self.remember_whole("Hide sheet");
        self.book.sheets[sheet].state = state;
        if state != Visibility::Visible && self.book.active_sheet == sheet {
            self.book.active_sheet =
                self.book.sheets.iter().position(|s| s.state == Visibility::Visible).unwrap_or(0);
        }
        Ok(())
    }

    pub fn set_tab_color(
        &mut self,
        sheet: usize,
        color: Option<crate::styles::Color>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Tab color");
        self.book.sheets[sheet].tab_color = color;
        Ok(())
    }

    fn reindex_after_sheet_move(&mut self, map: impl Fn(usize) -> usize) {
        for t in &mut self.book.tables {
            t.sheet = map(t.sheet);
        }
        for n in &mut self.book.names {
            n.scope = n.scope.map(&map);
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Layout.

    /// Set column widths, in characters, or `None` for the default.
    pub fn set_column_width(
        &mut self,
        sheet: usize,
        first: u32,
        last: u32,
        width: Option<f64>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Column width");
        for col in first..=last {
            self.book.sheets[sheet].edit_column(col, |c| {
                c.width = width;
                c.custom_width = width.is_some();
            });
        }
        self.modified = true;
        Ok(())
    }

    /// Set row heights, in points, or `None` for the default.
    pub fn set_row_height(
        &mut self,
        sheet: usize,
        first: u32,
        last: u32,
        height: Option<f64>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Row height");
        let s = &mut self.book.sheets[sheet];
        for row in first..=last {
            let info = s.rows.entry(row).or_default();
            info.height = height;
            if *info == RowInfo::default() {
                s.rows.remove(&row);
            }
        }
        Ok(())
    }

    pub fn hide_rows(
        &mut self,
        sheet: usize,
        first: u32,
        last: u32,
        hidden: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if hidden { "Hide rows" } else { "Unhide rows" });
        let s = &mut self.book.sheets[sheet];
        for row in first..=last {
            let info = s.rows.entry(row).or_default();
            info.hidden = hidden;
            if *info == RowInfo::default() {
                s.rows.remove(&row);
            }
        }
        // SUBTOTAL with 101 to 111 reads which rows are hidden.
        self.recalculate_all();
        Ok(())
    }

    pub fn hide_columns(
        &mut self,
        sheet: usize,
        first: u32,
        last: u32,
        hidden: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if hidden { "Hide columns" } else { "Unhide columns" });
        for col in first..=last {
            self.book.sheets[sheet].edit_column(col, |c| c.hidden = hidden);
        }
        Ok(())
    }

    /// Freeze the rows above and the columns left of a cell; A1 unfreezes.
    pub fn freeze(&mut self, sheet: usize, rows: u32, cols: u32) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if rows == 0 && cols == 0 { "Unfreeze" } else { "Freeze panes" });
        let s = &mut self.book.sheets[sheet];
        s.frozen = (rows > 0 || cols > 0).then_some((rows, cols));
        // The panes change, so what was chosen in each pane and where each pane was scrolled no longer
        // name panes the sheet has.
        s.view.split = None;
        s.view.pane_top_left = None;
        s.view.active_pane = None;
        s.view.frozen_split = false;
        s.view.selections.clear();
        Ok(())
    }

    /// Split the window into panes at a cell, as dragging the split bars does, or remove the split.
    /// `x` and `y` are the split's distance from the top left of the grid in twentieths of a point.
    pub fn split(
        &mut self,
        sheet: usize,
        split: Option<crate::model::Split>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if split.is_some() { "Split" } else { "Remove split" });
        let s = &mut self.book.sheets[sheet];
        s.frozen = None;
        s.view.pane_top_left = None;
        s.view.frozen_split = false;
        s.view.selections.clear();
        s.view.active_pane = split.as_ref().map(|sp| {
            match (sp.x > 0.0, sp.y > 0.0) {
                (true, true) => "bottomRight",
                (false, true) => "bottomLeft",
                _ => "topRight",
            }
            .to_owned()
        });
        s.view.split = split;
        Ok(())
    }

    /// Change how a sheet is shown: the view kind, headings, zeros, formulas, right to left.
    pub fn set_view(
        &mut self,
        sheet: usize,
        change: impl FnOnce(&mut crate::model::SheetView),
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Change view");
        change(&mut self.book.sheets[sheet].view);
        Ok(())
    }

    /// Remember the selection and the scroll position, which are saved with the file. Not an undo step.
    pub fn remember_selection(
        &mut self,
        sheet: usize,
        active: CellRef,
        ranges: Vec<Range>,
        top_left: CellRef,
    ) {
        let Some(s) = self.book.sheets.get_mut(sheet) else { return };
        let pane = match s.view.selections.last() {
            Some(last) => last.pane.clone(),
            None => frozen_or_split_pane(s),
        };
        s.view.selections.retain(|sel| sel.pane != pane);
        s.view.selections.push(crate::model::Selection { pane, active, ranges });
        if s.frozen.is_none() && s.view.split.is_none() {
            s.view.top_left = (top_left != CellRef::new(0, 0)).then_some(top_left);
        }
    }

    pub fn set_gridlines(&mut self, sheet: usize, shown: bool) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Gridlines");
        self.book.sheets[sheet].show_grid = shown;
        Ok(())
    }

    pub fn merge(&mut self, sheet: usize, range: Range, center: bool) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if range.len() < 2 {
            return Ok(());
        }
        self.remember_whole("Merge cells");
        let s = &mut self.book.sheets[sheet];
        s.merges.retain(|m| !m.intersects(&range));
        s.merges.push(range);
        // Only the top left cell keeps its value, as in Excel.
        let keys: Vec<(u32, u32)> = s
            .cells
            .keys()
            .filter(|(r, c)| {
                range.contains(CellRef::new(*r, *c))
                    && (*r, *c) != (range.first.row, range.first.col)
            })
            .copied()
            .collect();
        for key in keys {
            if let Some(cell) = s.cells.get_mut(&key) {
                cell.value = Value::Empty;
                cell.formula = None;
            }
        }
        if center {
            let style = s.cells.get(&(range.first.row, range.first.col)).map_or(0, |c| c.style);
            let next = self.book.styles.derive(
                style,
                &StyleChange::Align(crate::styles::AlignEdit {
                    horizontal: Some(Some("center".to_owned())),
                    ..Default::default()
                }),
            );
            self.book.sheets[sheet]
                .cells
                .entry((range.first.row, range.first.col))
                .or_default()
                .style = next;
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    pub fn unmerge(&mut self, sheet: usize, range: Range) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Unmerge cells");
        self.book.sheets[sheet].merges.retain(|m| !m.intersects(&range));
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------
    // Copy, paste, fill, sort.

    pub fn copy(&self, sheet: usize, range: Range, cut: bool) -> Clip {
        self.build_clip(sheet, range, cut)
    }

    /// Paste copied cells with their top left at `to`. A paste into a selection that is a whole multiple of
    /// the copied block repeats it, as Excel does. [`Session::paste_special`] has every other choice.
    pub fn paste(
        &mut self,
        clip: &Clip,
        sheet: usize,
        to: Range,
        mode: PasteMode,
    ) -> Result<Range, String> {
        self.paste_special(clip, sheet, to, &mode.into())
    }

    /// Cut and paste: the cells move and every formula that named them follows.
    fn move_cells(&mut self, clip: &Clip, sheet: usize, to: CellRef) {
        let (dr, dc) = (
            to.row as i64 - clip.range.first.row as i64,
            to.col as i64 - clip.range.first.col as i64,
        );
        let from_name = self.book.sheets[clip.sheet].name.clone();
        let names: Vec<String> = self.book.sheets.iter().map(|s| s.name.clone()).collect();
        // Every reference to the moved block now points at its new place.
        for (si, s) in self.book.sheets.iter_mut().enumerate() {
            for cell in s.cells.values_mut() {
                if let Some(f) = &mut cell.formula {
                    let moved = if clip.sheet == sheet {
                        formula::moved(&f.expr, &from_name, &names[si], clip.range, dr, dc)
                    } else {
                        (*f.expr).clone()
                    };
                    if moved != *f.expr {
                        let (dynamic, array) = (f.dynamic, f.array);
                        *f = Formula::from_expr(moved, dynamic);
                        f.array = array;
                    }
                }
            }
        }
        // Take the cells out, clear the target, and put them in.
        let mut taken = Vec::new();
        {
            let src = &mut self.book.sheets[clip.sheet];
            for (r, c, _) in &clip.cells {
                let key = (clip.range.first.row + r, clip.range.first.col + c);
                if let Some(cell) = src.cells.remove(&key) {
                    taken.push((*r, *c, cell));
                }
            }
        }
        let target = Range::new(
            to,
            CellRef::new(to.row + clip.range.rows() - 1, to.col + clip.range.cols() - 1),
        );
        let dest = &mut self.book.sheets[sheet];
        let keys: Vec<(u32, u32)> = dest
            .cells
            .keys()
            .filter(|(r, c)| target.contains(CellRef::new(*r, *c)))
            .copied()
            .collect();
        for k in keys {
            dest.cells.remove(&k);
        }
        for (r, c, cell) in taken {
            dest.cells.insert((to.row + r, to.col + c), cell);
        }
        // Notes travel with their cells.
        let notes: Vec<((u32, u32), Note)> = self.book.sheets[clip.sheet]
            .notes
            .iter()
            .filter(|((r, c), _)| clip.range.contains(CellRef::new(*r, *c)))
            .map(|(k, n)| (*k, n.clone()))
            .collect();
        for (k, n) in notes {
            self.book.sheets[clip.sheet].notes.remove(&k);
            self.book.sheets[sheet]
                .notes
                .insert(((k.0 as i64 + dr) as u32, (k.1 as i64 + dc) as u32), n);
            self.book.sheets[sheet].notes_changed = true;
            self.book.sheets[clip.sheet].notes_changed = true;
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
    }

    /// Fill a range from its first row (or column): Ctrl+D and Ctrl+R, and Fill Up and Left.
    pub fn fill(&mut self, sheet: usize, range: Range, direction: Direction) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let (source, target) = match direction {
            Direction::Down => (
                Range::new(range.first, CellRef::new(range.first.row, range.last.col)),
                Range::new(CellRef::new(range.first.row + 1, range.first.col), range.last),
            ),
            Direction::Right => (
                Range::new(range.first, CellRef::new(range.last.row, range.first.col)),
                Range::new(CellRef::new(range.first.row, range.first.col + 1), range.last),
            ),
            Direction::Up => (
                Range::new(CellRef::new(range.last.row, range.first.col), range.last),
                Range::new(
                    range.first,
                    CellRef::new(range.last.row.saturating_sub(1), range.last.col),
                ),
            ),
            Direction::Left => (
                Range::new(CellRef::new(range.first.row, range.last.col), range.last),
                Range::new(
                    range.first,
                    CellRef::new(range.last.row, range.last.col.saturating_sub(1)),
                ),
            ),
        };
        if range.rows() < 2 && matches!(direction, Direction::Down | Direction::Up)
            || range.cols() < 2 && matches!(direction, Direction::Right | Direction::Left)
        {
            return Ok(());
        }
        let clip = self.copy(sheet, source, false);
        let block = match direction {
            Direction::Down | Direction::Up => Range::new(target.first, target.last),
            _ => Range::new(target.first, target.last),
        };
        self.paste(&clip, sheet, block, PasteMode::All)?;
        if let Some(step) = self.undo.last_mut() {
            step.label = "Fill".to_owned();
        }
        Ok(())
    }

    /// Extend a block of cells into a larger range the way dragging the fill handle does: numbers and
    /// dates continue their step, text ending in a number counts on, day and month names continue,
    /// formulas move their references, and anything else repeats.
    pub fn fill_series(
        &mut self,
        sheet: usize,
        source: Range,
        target: Range,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if !target.contains(source.first) || !target.contains(source.last) {
            return Err("the range to fill must contain the cells it starts from".to_owned());
        }
        let down = target.rows() > source.rows();
        let up = down && target.first.row < source.first.row;
        let left = !down && target.first.col < source.first.col;
        self.remember_cells("Fill series", sheet, &[target], false);
        let lines = if down { source.cols() } else { source.rows() };
        for line in 0..lines {
            // The source cells along this line, in fill order.
            let sources: Vec<CellRef> = if down {
                (source.first.row..=source.last.row)
                    .map(|r| CellRef::new(r, source.first.col + line))
                    .collect()
            } else {
                (source.first.col..=source.last.col)
                    .map(|c| CellRef::new(source.first.row + line, c))
                    .collect()
            };
            let cells: Vec<Cell> = sources
                .iter()
                .map(|at| self.book.sheets[sheet].cell(*at).cloned().unwrap_or_default())
                .collect();
            let series = Series::of(&cells, self.book.date1904);
            let targets: Vec<CellRef> = if down {
                let col = source.first.col + line;
                if up {
                    (target.first.row..source.first.row)
                        .rev()
                        .map(|r| CellRef::new(r, col))
                        .collect()
                } else {
                    (source.last.row + 1..=target.last.row).map(|r| CellRef::new(r, col)).collect()
                }
            } else {
                let row = source.first.row + line;
                if left {
                    (target.first.col..source.first.col)
                        .rev()
                        .map(|c| CellRef::new(row, c))
                        .collect()
                } else {
                    (source.last.col + 1..=target.last.col).map(|c| CellRef::new(row, c)).collect()
                }
            };
            let backwards = up || left;
            for (k, at) in targets.iter().enumerate() {
                let n = k + 1;
                let template_index =
                    if backwards { (cells.len() - 1) - (k % cells.len()) } else { k % cells.len() };
                let template = &cells[template_index];
                let template_at = sources[template_index];
                let cell = match &template.formula {
                    Some(f) => Cell {
                        formula: Some(Formula::from_expr(
                            formula::shifted(
                                &f.expr,
                                at.row as i64 - template_at.row as i64,
                                at.col as i64 - template_at.col as i64,
                            ),
                            f.dynamic,
                        )),
                        style: template.style,
                        ..Default::default()
                    },
                    None => Cell {
                        value: series.value(n as i64, backwards, &cells, template_index),
                        style: template.style,
                        ..Default::default()
                    },
                };
                self.book.sheets[sheet].set(*at, cell);
                self.engine.changed(&self.book, sheet, *at);
            }
        }
        self.recalculate();
        Ok(())
    }

    /// Sort the rows of a range by some of its columns, each ascending or descending. `header` leaves the
    /// first row where it is.
    pub fn sort(
        &mut self,
        sheet: usize,
        range: Range,
        keys: &[(u32, bool)],
        header: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if keys.is_empty() {
            return Err("sort needs a column to sort by".to_owned());
        }
        if self.book.sheets[sheet].merges.iter().any(|m| m.intersects(&range)) {
            return Err("a range with merged cells cannot be sorted".to_owned());
        }
        let range = clip_to_used(&self.book.sheets[sheet], range);
        let first = range.first.row + u32::from(header);
        if first > range.last.row {
            return Ok(());
        }
        self.remember_whole("Sort");
        let s = &self.book.sheets[sheet];
        let rows: Vec<u32> = (first..=range.last.row).collect();
        let mut order = rows.clone();
        let value = |r: u32, c: u32| s.value_at(CellRef::new(r, range.first.col + c));
        order.sort_by(|a, b| {
            for (col, ascending) in keys {
                let (x, y) = (value(*a, *col), value(*b, *col));
                // Empty cells go last whichever way the sort runs.
                let o = match (x.is_empty(), y.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => return std::cmp::Ordering::Greater,
                    (false, true) => return std::cmp::Ordering::Less,
                    _ => crate::functions::lookup::sort_compare(&x, &y),
                };
                let o = if *ascending { o } else { o.reverse() };
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
        let mut moved: Vec<((u32, u32), Cell)> = Vec::new();
        for (new_index, old_row) in order.iter().enumerate() {
            let new_row = rows[new_index];
            for c in range.first.col..=range.last.col {
                if let Some(mut cell) = self.book.sheets[sheet].cells.get(&(*old_row, c)).cloned() {
                    if let Some(f) = &cell.formula {
                        cell.formula = Some(Formula::from_expr(
                            formula::shifted(&f.expr, new_row as i64 - *old_row as i64, 0),
                            f.dynamic,
                        ));
                    }
                    moved.push(((new_row, c), cell));
                }
            }
        }
        let s = &mut self.book.sheets[sheet];
        for r in first..=range.last.row {
            for c in range.first.col..=range.last.col {
                s.cells.remove(&(r, c));
            }
        }
        for (k, cell) in moved {
            s.cells.insert(k, cell);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Remove the rows of a range whose values in some columns repeat an earlier row. Answers how many
    /// were removed.
    pub fn remove_duplicates(
        &mut self,
        sheet: usize,
        range: Range,
        columns: &[u32],
        header: bool,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        let range = clip_to_used(&self.book.sheets[sheet], range);
        let first = range.first.row + u32::from(header);
        self.remember_whole("Remove duplicates");
        let s = &self.book.sheets[sheet];
        let mut seen: Vec<Vec<Value>> = Vec::new();
        let mut keep_rows = Vec::new();
        for r in first..=range.last.row {
            let key: Vec<Value> =
                columns.iter().map(|c| s.value_at(CellRef::new(r, range.first.col + c))).collect();
            if seen.iter().any(|k| {
                k.iter().zip(&key).all(|(a, b)| {
                    crate::eval::equal(a, b)
                        && std::mem::discriminant(a) == std::mem::discriminant(b)
                })
            }) {
                continue;
            }
            seen.push(key);
            keep_rows.push(r);
        }
        let removed = (range.last.row + 1 - first) as usize - keep_rows.len();
        let mut kept: Vec<((u32, u32), Cell)> = Vec::new();
        for (i, r) in keep_rows.iter().enumerate() {
            for c in range.first.col..=range.last.col {
                if let Some(cell) = s.cells.get(&(*r, c)) {
                    kept.push(((first + i as u32, c), cell.clone()));
                }
            }
        }
        let s = &mut self.book.sheets[sheet];
        for r in first..=range.last.row {
            for c in range.first.col..=range.last.col {
                s.cells.remove(&(r, c));
            }
        }
        for (k, cell) in kept {
            s.cells.insert(k, cell);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(removed)
    }

    /// Split the text of a column into the columns to its right at a delimiter.
    pub fn text_to_columns(
        &mut self,
        sheet: usize,
        range: Range,
        delimiter: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if delimiter.is_empty() {
            return Err("text to columns needs a delimiter".to_owned());
        }
        let range = clip_to_used(&self.book.sheets[sheet], range);
        let mut inputs = Vec::new();
        for r in range.first.row..=range.last.row {
            let v = self.book.sheets[sheet].value_at(CellRef::new(r, range.first.col));
            if let Value::Text(t) = v {
                for (i, part) in t.split(delimiter).enumerate() {
                    inputs.push((CellRef::new(r, range.first.col + i as u32), part.to_owned()));
                }
            }
        }
        self.set_inputs(sheet, &inputs)
    }

    /// Find cells whose value (or formula) holds some text. Answers the cells in row order.
    pub fn find(
        &self,
        sheet: Option<usize>,
        text: &str,
        formulas: bool,
        whole: bool,
        case: bool,
    ) -> Vec<(usize, CellRef)> {
        let needle = if case { text.to_owned() } else { text.to_lowercase() };
        let mut out = Vec::new();
        for (si, s) in self.book.sheets.iter().enumerate() {
            if sheet.is_some_and(|x| x != si) {
                continue;
            }
            for (&(r, c), cell) in &s.cells {
                let hay = match (&cell.formula, formulas) {
                    (Some(f), true) => f.display(),
                    _ => shown_text(&self.book, cell),
                };
                let hay = if case { hay } else { hay.to_lowercase() };
                let hit = if whole { hay == needle } else { hay.contains(&needle) };
                if hit && !needle.is_empty() {
                    out.push((si, CellRef::new(r, c)));
                }
            }
        }
        out
    }

    /// Replace text in cell values and formulas. Answers how many cells changed.
    pub fn replace(
        &mut self,
        sheet: Option<usize>,
        find: &str,
        with: &str,
        case: bool,
    ) -> Result<usize, String> {
        let hits = self.find(sheet, find, true, false, case);
        let mut by_sheet: BTreeMap<usize, Vec<(CellRef, String)>> = BTreeMap::new();
        for (si, at) in hits {
            let cell = self.book.sheets[si].cell(at).cloned().unwrap_or_default();
            let source = match &cell.formula {
                Some(f) => f.display(),
                None => cell.value.to_text(),
            };
            let replaced = if case {
                source.replace(find, with)
            } else {
                replace_ignoring_case(&source, find, with)
            };
            if replaced != source {
                by_sheet.entry(si).or_default().push((at, replaced));
            }
        }
        let mut count = 0;
        for (si, inputs) in by_sheet {
            count += inputs.len();
            self.set_inputs(si, &inputs)?;
        }
        Ok(count)
    }

    // ------------------------------------------------------------------------------------------------
    // Notes, links, names, validation, conditional formats, filters.

    pub fn set_note(
        &mut self,
        sheet: usize,
        at: CellRef,
        text: Option<&str>,
        author: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if text.is_some() { "Note" } else { "Delete note" });
        let s = &mut self.book.sheets[sheet];
        match text {
            Some(t) => {
                s.notes.insert((at.row, at.col), Note::new(author, t));
            }
            None => {
                s.notes.remove(&(at.row, at.col));
            }
        }
        s.notes_changed = true;
        Ok(())
    }

    pub fn set_link(
        &mut self,
        sheet: usize,
        range: Range,
        target: Option<LinkTarget>,
        display: Option<String>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if target.is_some() { "Link" } else { "Remove link" });
        let s = &mut self.book.sheets[sheet];
        s.links.retain(|l| !l.range.intersects(&range));
        if let Some(t) = target {
            s.links.push(Hyperlink { range, target: t, display: display.clone(), tooltip: None });
            // A link with no text shows its address.
            if s.cell(range.first).is_none_or(|c| c.value.is_empty()) {
                let shown = display.unwrap_or_else(|| match &s.links.last().unwrap().target {
                    LinkTarget::External(a) => a.clone(),
                    LinkTarget::Location(l) => l.clone(),
                });
                let style = s.cell(range.first).map_or(0, |c| c.style);
                s.set(range.first, Cell { value: Value::text(shown), style, ..Default::default() });
                self.engine.changed(&self.book, sheet, range.first);
                self.recalculate();
            }
        }
        Ok(())
    }

    /// Add or replace a defined name.
    pub fn set_name(
        &mut self,
        name: &str,
        refers_to: &str,
        scope: Option<usize>,
    ) -> Result<(), String> {
        check_defined_name(name)?;
        let formula = refers_to.trim().trim_start_matches('=');
        let expr = formula::parser::parse(formula)
            .map_err(|e| format!("{refers_to} is not a reference or a formula: {e}"))?;
        self.remember_whole("Name");
        self.book.names.retain(|n| !(n.name.eq_ignore_ascii_case(name) && n.scope == scope));
        self.book.names.push(DefinedName {
            name: name.to_owned(),
            scope,
            formula: formula::print::print(&expr, true),
            hidden: false,
            attrs: Vec::new(),
        });
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    pub fn delete_name(&mut self, name: &str, scope: Option<usize>) -> Result<(), String> {
        if !self.book.names.iter().any(|n| n.name.eq_ignore_ascii_case(name) && n.scope == scope) {
            return Err(format!("there is no name {name}"));
        }
        self.remember_whole("Delete name");
        self.book.names.retain(|n| !(n.name.eq_ignore_ascii_case(name) && n.scope == scope));
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    pub fn add_validation(&mut self, sheet: usize, validation: Validation) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Data validation");
        let s = &mut self.book.sheets[sheet];
        for v in &mut s.validations {
            v.ranges.retain(|r| {
                !validation.ranges.iter().any(|n| n.contains(r.first) && n.contains(r.last))
            });
        }
        s.validations.retain(|v| !v.ranges.is_empty());
        s.validations.push(validation);
        Ok(())
    }

    pub fn clear_validation(&mut self, sheet: usize, range: Range) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Clear validation");
        let s = &mut self.book.sheets[sheet];
        for v in &mut s.validations {
            v.ranges.retain(|r| !r.intersects(&range));
        }
        s.validations.retain(|v| !v.ranges.is_empty());
        Ok(())
    }

    /// The validation rule that applies to a cell.
    pub fn validation_at(&self, sheet: usize, at: CellRef) -> Option<&Validation> {
        self.book
            .sheets
            .get(sheet)?
            .validations
            .iter()
            .find(|v| v.ranges.iter().any(|r| r.contains(at)))
    }

    /// Whether a value meets a cell's validation; the error message when it does not.
    pub fn check_validation(&self, sheet: usize, at: CellRef, value: &Value) -> Result<(), String> {
        let Some(v) = self.validation_at(sheet, at) else { return Ok(()) };
        if value.is_empty() {
            return if v.allow_blank { Ok(()) } else { Err(validation_message(v)) };
        }
        let ok = match v.kind.as_str() {
            "list" => list_items(self, sheet, at, v)
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&value.to_text())),
            "whole" | "decimal" | "date" | "time" | "textLength" => {
                let n = match (v.kind.as_str(), value) {
                    ("textLength", v) => v.to_text().chars().count() as f64,
                    (_, Value::Number(n)) => *n,
                    _ => return Err(validation_message(v)),
                };
                if v.kind == "whole" && n.fract() != 0.0 {
                    false
                } else {
                    let bound = |f: &Option<String>| {
                        f.as_ref()
                            .and_then(|t| self.evaluate_at(sheet, at, t).ok())
                            .and_then(|v| v.as_number())
                    };
                    let (a, b) = (bound(&v.formula1), bound(&v.formula2));
                    match (v.operator.as_deref().unwrap_or("between"), a, b) {
                        ("between", Some(a), Some(b)) => n >= a && n <= b,
                        ("notBetween", Some(a), Some(b)) => n < a || n > b,
                        ("equal", Some(a), _) => n == a,
                        ("notEqual", Some(a), _) => n != a,
                        ("greaterThan", Some(a), _) => n > a,
                        ("lessThan", Some(a), _) => n < a,
                        ("greaterThanOrEqual", Some(a), _) => n >= a,
                        ("lessThanOrEqual", Some(a), _) => n <= a,
                        _ => true,
                    }
                }
            }
            "custom" => v
                .formula1
                .as_ref()
                .and_then(|f| self.evaluate_at(sheet, at, f).ok())
                .is_none_or(|r| matches!(r, Value::Bool(true))),
            _ => true,
        };
        if ok {
            Ok(())
        } else {
            Err(validation_message(v))
        }
    }

    /// The choices of a list validation at a cell, for its dropdown.
    pub fn list_choices(&self, sheet: usize, at: CellRef) -> Vec<String> {
        match self.validation_at(sheet, at) {
            Some(v) if v.kind == "list" => list_items(self, sheet, at, v),
            _ => Vec::new(),
        }
    }

    pub fn add_conditional(
        &mut self,
        sheet: usize,
        ranges: Vec<Range>,
        mut rule: CfRule,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Conditional format");
        let top = self
            .book
            .sheets
            .iter()
            .flat_map(|s| s.conditional.iter())
            .flat_map(|c| c.rules.iter())
            .map(|r| r.priority)
            .max()
            .unwrap_or(0);
        rule.priority = top + 1;
        self.book.sheets[sheet].conditional.push(ConditionalFormat { ranges, rules: vec![rule] });
        Ok(())
    }

    pub fn clear_conditional(&mut self, sheet: usize, range: Range) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Clear rules");
        let s = &mut self.book.sheets[sheet];
        for cf in &mut s.conditional {
            cf.ranges.retain(|r| !r.intersects(&range));
        }
        s.conditional.retain(|cf| !cf.ranges.is_empty());
        Ok(())
    }

    /// Turn the filter buttons on for a range, or off with `None`.
    pub fn set_filter(&mut self, sheet: usize, range: Option<Range>) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole(if range.is_some() { "Filter" } else { "Remove filter" });
        let s = &mut self.book.sheets[sheet];
        if let Some(old) = &s.filter {
            let old = old.range;
            for r in old.first.row + 1..=old.last.row {
                if let Some(info) = s.rows.get_mut(&r) {
                    info.hidden = false;
                }
            }
        }
        s.filter = range.map(|r| AutoFilter {
            range: clip_to_used(s, r),
            columns: BTreeMap::new(),
            sort: None,
        });
        self.recalculate_all();
        Ok(())
    }

    /// Filter one column of the filter range: keep rows whose shown value is one of `values`, or all rows
    /// with `None`.
    pub fn filter_column(
        &mut self,
        sheet: usize,
        col: u32,
        criteria: Option<FilterCriteria>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let Some(filter) = self.book.sheets[sheet].filter.clone() else {
            return Err("this sheet has no filter".to_owned());
        };
        if col < filter.range.first.col || col > filter.range.last.col {
            return Err("that column is not in the filter".to_owned());
        }
        self.remember_whole("Filter");
        let offset = col - filter.range.first.col;
        {
            let s = &mut self.book.sheets[sheet];
            let f = s.filter.as_mut().unwrap();
            match criteria {
                Some(c) => {
                    f.columns.insert(offset, c);
                }
                None => {
                    f.columns.remove(&offset);
                }
            }
        }
        self.apply_filter(sheet);
        Ok(())
    }

    /// Hide the rows the filter's criteria leave out.
    pub fn apply_filter(&mut self, sheet: usize) {
        let Some(filter) = self.book.sheets[sheet].filter.clone() else { return };
        for r in filter.range.first.row + 1..=filter.range.last.row {
            let mut keep = true;
            for (offset, criteria) in &filter.columns {
                let cell = self.book.sheets[sheet]
                    .cell(CellRef::new(r, filter.range.first.col + offset))
                    .cloned()
                    .unwrap_or_default();
                let shown = shown_text(&self.book, &cell);
                let ok = match criteria {
                    FilterCriteria::Values { values, blank } => {
                        if shown.is_empty() {
                            *blank
                        } else {
                            values.iter().any(|v| v.eq_ignore_ascii_case(&shown))
                        }
                    }
                    FilterCriteria::Custom { conditions, and } => {
                        let results: Vec<bool> = conditions
                            .iter()
                            .map(|(op, val)| {
                                custom_matches(op, val, &cell.value, self.book.date1904)
                            })
                            .collect();
                        if *and {
                            results.iter().all(|x| *x)
                        } else {
                            results.iter().any(|x| *x)
                        }
                    }
                    FilterCriteria::Raw(_) => true,
                };
                if !ok {
                    keep = false;
                    break;
                }
            }
            let s = &mut self.book.sheets[sheet];
            let info = s.rows.entry(r).or_default();
            info.hidden = !keep;
            if *info == RowInfo::default() {
                s.rows.remove(&r);
            }
        }
        self.recalculate_all();
    }

    /// Make a table over a range, with a header row of names.
    pub fn add_table(
        &mut self,
        sheet: usize,
        range: Range,
        name: Option<&str>,
    ) -> Result<String, String> {
        self.check_sheet(sheet)?;
        if self.book.tables.iter().any(|t| t.sheet == sheet && t.range.intersects(&range)) {
            return Err("a table cannot overlap another table".to_owned());
        }
        let name = match name {
            Some(n) => {
                check_defined_name(n)?;
                n.to_owned()
            }
            None => (1..)
                .map(|n| format!("Table{n}"))
                .find(|n| self.book.table(n).is_none())
                .unwrap_or_default(),
        };
        self.remember_whole("Table");
        let s = &self.book.sheets[sheet];
        let mut columns = Vec::new();
        for c in range.first.col..=range.last.col {
            let v = s.value_at(CellRef::new(range.first.row, c)).to_text();
            let mut header =
                if v.is_empty() { format!("Column{}", c - range.first.col + 1) } else { v };
            while columns.contains(&header) {
                header.push('2');
            }
            columns.push(header);
        }
        let id = self.book.tables.len() + 1;
        let cols: String = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!("<tableColumn id=\"{}\" name=\"{}\"/>", i + 1, crate::xlsx::escape(c))
            })
            .collect();
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<table xmlns=\"{}\" id=\"{id}\" name=\"{name}\" displayName=\"{name}\" ref=\"{r}\" totalsRowShown=\"0\"><autoFilter ref=\"{r}\"/><tableColumns count=\"{}\">{cols}</tableColumns><tableStyleInfo name=\"TableStyleMedium2\" showFirstColumn=\"0\" showLastColumn=\"0\" showRowStripes=\"1\" showColumnStripes=\"0\"/></table>",
            crate::xlsx::MAIN_NS,
            columns.len(),
            r = range.a1()
        );
        // Header cells hold the names.
        let inputs: Vec<(CellRef, String)> = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (CellRef::new(range.first.row, range.first.col + i as u32), format!("'{c}"))
            })
            .collect();
        self.book.tables.push(Table {
            sheet,
            name: name.clone(),
            range,
            columns,
            header_row: true,
            totals_row: false,
            part: String::new(),
            xml,
        });
        for (at, text) in inputs {
            let s = &mut self.book.sheets[sheet];
            let style = s.cell(at).map_or(0, |c| c.style);
            s.set(
                at,
                Cell {
                    value: Value::text(text.trim_start_matches('\'')),
                    style,
                    ..Default::default()
                },
            );
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(name)
    }

    // ------------------------------------------------------------------------------------------------
    // Questions.

    /// Evaluate a formula as though it were in a cell, without changing anything.
    /// Put a chart on a sheet. It is written as new drawing and chart parts when the workbook is saved.
    pub fn add_chart(&mut self, sheet: usize, chart: Chart) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Chart");
        self.book.sheets[sheet].charts.push(chart);
        self.modified = true;
        Ok(())
    }

    /// Take a chart off a sheet.
    pub fn delete_chart(&mut self, sheet: usize, index: usize) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let Some(chart) = self.book.sheets[sheet].charts.get(index) else {
            return Err(format!("there is no chart {} on this sheet", index + 1));
        };
        if !chart.new {
            return Err(
                "That chart was made in Excel and is kept exactly as it was; delete it in Excel."
                    .to_owned(),
            );
        }
        self.remember_whole("Delete chart");
        self.book.sheets[sheet].charts.remove(index);
        self.modified = true;
        Ok(())
    }

    pub fn evaluate_at(&self, sheet: usize, at: CellRef, formula: &str) -> Result<Value, String> {
        crate::eval::evaluate(&self.book, sheet, at, formula, self.now).map(|(v, _)| v)
    }

    /// The formula AutoSum would put in a cell: the numbers above it, or to its left.
    pub fn autosum(&self, sheet: usize, at: CellRef) -> String {
        let s = &self.book.sheets[sheet];
        let numeric = |r: u32, c: u32| matches!(s.value_at(CellRef::new(r, c)), Value::Number(_));
        let mut top = at.row;
        while top > 0 && numeric(top - 1, at.col) {
            top -= 1;
        }
        if top < at.row {
            return format!(
                "=SUM({})",
                Range::new(CellRef::new(top, at.col), CellRef::new(at.row - 1, at.col)).a1()
            );
        }
        let mut left = at.col;
        while left > 0 && numeric(at.row, left - 1) {
            left -= 1;
        }
        if left < at.col {
            return format!(
                "=SUM({})",
                Range::new(CellRef::new(at.row, left), CellRef::new(at.row, at.col - 1)).a1()
            );
        }
        "=SUM()".to_owned()
    }

    /// Sum, average, count and the rest of a selection, for the status of the selection.
    pub fn summary(&self, sheet: usize, ranges: &[Range]) -> SelectionSummary {
        let s = &self.book.sheets[sheet];
        let mut summary = SelectionSummary::default();
        for range in ranges {
            let r = clip_to_used(s, *range);
            for (&(row, col), cell) in s.cells.range((r.first.row, 0)..=(r.last.row, MAX_COLS)) {
                if col < r.first.col || col > r.last.col || cell.value.is_empty() {
                    continue;
                }
                summary.count += 1;
                if let Value::Number(n) = cell.value {
                    summary.numbers += 1;
                    summary.sum += n;
                    summary.min = Some(summary.min.map_or(n, |m: f64| m.min(n)));
                    summary.max = Some(summary.max.map_or(n, |m: f64| m.max(n)));
                }
                let _ = row;
            }
            for (&(row, col), v) in s.spills.values.range((r.first.row, 0)..=(r.last.row, MAX_COLS))
            {
                if col < r.first.col
                    || col > r.last.col
                    || v.is_empty()
                    || s.cells.contains_key(&(row, col))
                {
                    continue;
                }
                summary.count += 1;
                if let Value::Number(n) = v {
                    summary.numbers += 1;
                    summary.sum += n;
                    summary.min = Some(summary.min.map_or(*n, |m: f64| m.min(*n)));
                    summary.max = Some(summary.max.map_or(*n, |m: f64| m.max(*n)));
                }
            }
        }
        summary
    }

    fn check_sheet(&self, sheet: usize) -> Result<(), String> {
        match self.book.sheets.get(sheet) {
            Some(s) if s.kept_part.is_some() => {
                Err(format!("{} is a chart sheet, which has no cells", s.name))
            }
            Some(_) => Ok(()),
            None => Err(format!("there is no sheet {sheet}")),
        }
    }

    fn refuse_array_member(&self, sheet: usize, range: Range) -> Result<(), String> {
        for a in &self.book.sheets[sheet].array_formulas {
            if a.intersects(&range) && !(range.contains(a.first) && range.contains(a.last)) {
                return Err("You can't change part of an array formula. Select the whole of it to change it.".to_owned());
            }
        }
        Ok(())
    }

    /// A protected sheet refuses changes to its locked cells.
    fn refuse_locked(&self, sheet: usize, range: Range) -> Result<(), String> {
        let s = &self.book.sheets[sheet];
        if !s.raw.elements.iter().any(|(n, x)| n == "sheetProtection" && x.contains("sheet=\"1\""))
        {
            return Ok(());
        }
        for at in range.cells().take(10_000) {
            let style = s.cell(at).map_or(0, |c| c.style);
            if self.book.styles.xf(style).locked.unwrap_or(true) {
                return Err(
                    "The cell is on a protected sheet. Unprotect the sheet in Excel to change it."
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// Sum, average and count of a selection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionSummary {
    pub sum: f64,
    pub count: usize,
    pub numbers: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl SelectionSummary {
    pub fn average(&self) -> Option<f64> {
        (self.numbers > 0).then(|| self.sum / self.numbers as f64)
    }
}

fn last_row(s: &Sheet) -> u32 {
    s.used_range().map_or(0, |r| r.last.row)
}

fn last_col(s: &Sheet) -> u32 {
    s.used_range().map_or(0, |r| r.last.col)
}

/// A range limited to the rows and columns in use, so a whole column is not millions of cells.
pub fn clip_to_used(s: &Sheet, r: Range) -> Range {
    let used = s.used_range().unwrap_or(Range::cell(CellRef::new(0, 0)));
    let last = CellRef::new(
        r.last.row.min(used.last.row.max(r.first.row)),
        r.last.col.min(used.last.col.max(r.first.col)),
    );
    Range::new(r.first, last)
}

/// The style a cell with no entry of its own shows: its row's, or its column's.
fn inherited_style(s: &Sheet, key: (u32, u32)) -> u32 {
    s.rows
        .get(&key.0)
        .and_then(|r| r.style)
        .or_else(|| s.column(key.1).map(|c| c.style))
        .unwrap_or(0)
}

/// Excel's rules for a sheet name.
pub fn check_sheet_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a sheet needs a name".to_owned());
    }
    if name.chars().count() > 31 {
        return Err("a sheet name can be at most 31 characters".to_owned());
    }
    if name.contains([':', '\\', '/', '?', '*', '[', ']']) {
        return Err("a sheet name cannot hold : \\ / ? * [ or ]".to_owned());
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return Err("a sheet name cannot start or end with an apostrophe".to_owned());
    }
    if name.eq_ignore_ascii_case("history") {
        return Err("History is a name Excel keeps for itself".to_owned());
    }
    Ok(())
}

/// Excel's rules for a defined name.
pub fn check_defined_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let first = chars.next().ok_or("a name needs at least one character")?;
    if !(first.is_alphabetic() || first == '_' || first == '\\') {
        return Err("a name starts with a letter, an underscore or a backslash".to_owned());
    }
    if !chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '\\') {
        return Err("a name holds letters, numbers, underscores and periods".to_owned());
    }
    if CellRef::parse(name).is_some()
        || matches!(name.to_ascii_uppercase().as_str(), "R" | "C" | "TRUE" | "FALSE")
    {
        return Err(format!("{name} reads as a cell reference or a value"));
    }
    if name.chars().count() > 255 {
        return Err("a name can be at most 255 characters".to_owned());
    }
    Ok(())
}

fn validation_message(v: &Validation) -> String {
    v.error.clone().unwrap_or_else(|| {
        "This value doesn't match the data validation restrictions defined for this cell."
            .to_owned()
    })
}

fn list_items(session: &Session, sheet: usize, at: CellRef, v: &Validation) -> Vec<String> {
    let Some(source) = &v.formula1 else { return Vec::new() };
    let trimmed = source.trim();
    if trimmed.starts_with('"') {
        return trimmed.trim_matches('"').split(',').map(|s| s.trim().to_owned()).collect();
    }
    // A range or a formula that gives one.
    let Ok(expr) = formula::parser::parse(trimmed) else { return Vec::new() };
    let env = crate::eval::Env::new(session.now, 7);
    let mut ctx = crate::eval::Ctx::new(&session.book, sheet, at, &env);
    let val = ctx.eval(&expr);
    match ctx.array(val) {
        Ok(a) => a.data.iter().filter(|v| !v.is_empty()).map(|v| v.to_text()).collect(),
        Err(_) => Vec::new(),
    }
}

fn custom_matches(op: &str, val: &str, cell: &Value, date1904: bool) -> bool {
    let target = crate::input::number_from_text(val, date1904);
    let text = cell.to_text();
    match (op, target, cell) {
        ("equal", _, _) => crate::functions::criteria::wildcard_match(val, &text),
        ("notEqual", _, _) => !crate::functions::criteria::wildcard_match(val, &text),
        (_, Some(t), Value::Number(n)) => match op {
            "greaterThan" => *n > t,
            "greaterThanOrEqual" => *n >= t,
            "lessThan" => *n < t,
            "lessThanOrEqual" => *n <= t,
            _ => false,
        },
        _ => false,
    }
}

fn replace_ignoring_case(source: &str, find: &str, with: &str) -> String {
    if find.is_empty() {
        return source.to_owned();
    }
    let lower = source.to_lowercase();
    let needle = find.to_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(&needle) {
        out.push_str(&source[at..at + i]);
        out.push_str(with);
        at += i + needle.len();
    }
    out.push_str(&source[at..]);
    out
}

/// The pane a selection belongs to when a sheet has panes: the active pane.
fn frozen_or_split_pane(s: &Sheet) -> Option<String> {
    if s.frozen.is_none() && s.view.split.is_none() {
        return None;
    }
    s.view.active_pane.clone().or_else(|| Some("bottomRight".to_owned()))
}

/// A reference adjustment for inserting or deleting cells in a band of columns (rows) only.
pub(crate) fn adjust_band(
    expr: &Expr,
    sheet: &str,
    own: &str,
    axis: Axis,
    at: u32,
    count: i64,
    band: (u32, u32),
) -> Expr {
    expr.map_refs(&mut |r| {
        let on = match &r.sheet {
            Some(n) => n.eq_ignore_ascii_case(sheet),
            None => own.eq_ignore_ascii_case(sheet),
        };
        let (lo, hi) = match axis {
            Axis::Rows => (r.first.col, r.last.col),
            Axis::Columns => (r.first.row, r.last.row),
        };
        if !on || lo < band.0 || hi > band.1 {
            return Some(r.clone());
        }
        formula::adjust_reference(r, axis, at, count)
    })
}

/// How a fill continues a block of cells.
struct Series {
    kind: SeriesKind,
}

enum SeriesKind {
    /// Numbers (or dates) with a step.
    Linear {
        start: f64,
        step: f64,
    },
    /// Text ending in a number that counts on.
    Counted {
        prefix: String,
        start: i64,
        step: i64,
        width: usize,
    },
    /// Day or month names.
    Names {
        list: &'static [&'static str],
        start: usize,
        step: i64,
        upper: bool,
    },
    Copy,
}

const DAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS_SHORT: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl Series {
    fn of(cells: &[Cell], _date1904: bool) -> Series {
        let numbers: Vec<f64> = cells.iter().filter_map(|c| c.value.as_number()).collect();
        if numbers.len() == cells.len() && cells.iter().all(|c| c.formula.is_none()) {
            if numbers.len() >= 2 {
                // A straight line through the numbers, as Excel's series fill takes.
                let n = numbers.len() as f64;
                let mean_x = (n - 1.0) / 2.0;
                let mean_y = numbers.iter().sum::<f64>() / n;
                let sxx: f64 = (0..numbers.len()).map(|i| (i as f64 - mean_x).powi(2)).sum();
                let sxy: f64 = numbers
                    .iter()
                    .enumerate()
                    .map(|(i, y)| (i as f64 - mean_x) * (y - mean_y))
                    .sum();
                let step = sxy / sxx;
                return Series {
                    kind: SeriesKind::Linear {
                        start: mean_y - step * mean_x + step * (n - 1.0),
                        step,
                    },
                };
            }
            // One number copies, which is Excel's fill without Ctrl.
            return Series { kind: SeriesKind::Copy };
        }
        let texts: Vec<String> = cells.iter().map(|c| c.value.to_text()).collect();
        if cells.iter().all(|c| matches!(c.value, Value::Text(_))) {
            for list in [
                &crate::dates::DAYS[..],
                &DAYS_SHORT[..],
                &crate::dates::MONTHS[..],
                &MONTHS_SHORT[..],
            ] {
                let positions: Vec<Option<usize>> = texts
                    .iter()
                    .map(|t| list.iter().position(|d| d.eq_ignore_ascii_case(t)))
                    .collect();
                if positions.iter().all(Option::is_some) {
                    let p: Vec<i64> = positions.iter().map(|x| x.unwrap() as i64).collect();
                    let step =
                        if p.len() >= 2 { (p[1] - p[0]).rem_euclid(list.len() as i64) } else { 1 };
                    let upper = texts[0].chars().all(|c| !c.is_lowercase());
                    return Series {
                        kind: SeriesKind::Names {
                            list,
                            start: *p.last().unwrap() as usize,
                            step: if step == 0 { 1 } else { step },
                            upper,
                        },
                    };
                }
            }
            // Text ending in a number.
            let split = |t: &str| -> Option<(String, i64, usize)> {
                let digits: String = t
                    .chars()
                    .rev()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                if digits.is_empty() || digits.len() > 15 {
                    return None;
                }
                Some((t[..t.len() - digits.len()].to_owned(), digits.parse().ok()?, digits.len()))
            };
            let parts: Vec<Option<(String, i64, usize)>> = texts.iter().map(|t| split(t)).collect();
            if parts.iter().all(Option::is_some) {
                let p: Vec<(String, i64, usize)> = parts.into_iter().map(Option::unwrap).collect();
                if p.iter().all(|x| x.0 == p[0].0) {
                    let step = if p.len() >= 2 { p[1].1 - p[0].1 } else { 1 };
                    let last = p.last().unwrap();
                    return Series {
                        kind: SeriesKind::Counted {
                            prefix: last.0.clone(),
                            start: last.1,
                            step,
                            width: p[0].2,
                        },
                    };
                }
            }
        }
        Series { kind: SeriesKind::Copy }
    }

    /// The value `n` steps past the block (or before it, going backwards).
    fn value(&self, n: i64, backwards: bool, cells: &[Cell], template: usize) -> Value {
        let len = cells.len() as i64;
        match &self.kind {
            SeriesKind::Linear { start, step } => {
                let x = if backwards {
                    start - step * (len - 1) as f64 - step * n as f64
                } else {
                    start + step * n as f64
                };
                Value::Number(crate::value::round_significant(x, 15))
            }
            SeriesKind::Counted { prefix, start, step, width } => {
                let v =
                    if backwards { start - step * (len - 1) - step * n } else { start + step * n };
                Value::text(format!("{prefix}{:0width$}", v.max(0), width = *width))
            }
            SeriesKind::Names { list, start, step, upper } => {
                let size = list.len() as i64;
                let i = if backwards {
                    (*start as i64 - step * (len - 1) - step * n).rem_euclid(size)
                } else {
                    (*start as i64 + step * n).rem_euclid(size)
                };
                let name = list[i as usize];
                Value::text(if *upper { name.to_uppercase() } else { name.to_owned() })
            }
            SeriesKind::Copy => cells[template].value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(Workbook::new(), 46318.5)
    }

    fn at(a: &str) -> CellRef {
        CellRef::parse(a).unwrap()
    }

    fn shown(s: &Session, a: &str) -> String {
        s.book.sheets[0].value_at(at(a)).to_text()
    }

    #[test]
    fn typing_past_the_cells_in_use_is_undone() {
        let mut s = session();
        s.set_input(0, at("A1"), "1").unwrap();
        s.set_input(0, at("D5"), "=A1*2").unwrap();
        assert_eq!(shown(&s, "D5"), "2");
        s.undo();
        assert_eq!(shown(&s, "D5"), "");
        s.redo();
        assert_eq!(shown(&s, "D5"), "2");
    }

    #[test]
    fn typing_reads_values_and_formulas_and_recalculates() {
        let mut s = session();
        s.set_input(0, at("A1"), "12").unwrap();
        s.set_input(0, at("A2"), "50%").unwrap();
        s.set_input(0, at("A3"), "=A1*A2").unwrap();
        assert_eq!(shown(&s, "A3"), "6");
        assert_eq!(s.book.styles.format_code(s.book.sheets[0].cell(at("A2")).unwrap().style), "0%");
        s.set_input(0, at("A1"), "20").unwrap();
        assert_eq!(shown(&s, "A3"), "10");
        assert!(s.set_input(0, at("A4"), "=SUM(").is_err());
    }

    #[test]
    fn undo_and_redo_put_cells_back_and_recalculate() {
        let mut s = session();
        s.set_input(0, at("A1"), "1").unwrap();
        s.set_input(0, at("B1"), "=A1+1").unwrap();
        s.set_input(0, at("A1"), "5").unwrap();
        assert_eq!(shown(&s, "B1"), "6");
        s.undo();
        assert_eq!(shown(&s, "B1"), "2");
        s.redo();
        assert_eq!(shown(&s, "B1"), "6");
        s.insert(0, Range::parse("1:1").unwrap(), Shift::WholeRows).unwrap();
        assert_eq!(
            s.book.sheets[0].cell(at("B2")).unwrap().formula.as_ref().unwrap().display(),
            "=A2+1"
        );
        s.undo();
        assert_eq!(
            s.book.sheets[0].cell(at("B1")).unwrap().formula.as_ref().unwrap().display(),
            "=A1+1"
        );
    }

    #[test]
    fn inserting_and_deleting_moves_cells_and_references() {
        let mut s = session();
        for (i, v) in ["1", "2", "3", "4"].iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, 0), v).unwrap();
        }
        s.set_input(0, at("B1"), "=SUM(A1:A4)").unwrap();
        s.insert(0, Range::parse("A3:A3").unwrap(), Shift::Down).unwrap();
        assert_eq!(shown(&s, "A5"), "4");
        assert_eq!(
            s.book.sheets[0].cell(at("B1")).unwrap().formula.as_ref().unwrap().display(),
            "=SUM(A1:A5)"
        );
        s.delete(0, Range::parse("1:2").unwrap(), Shift::WholeRows).unwrap();
        assert_eq!(shown(&s, "A3"), "4");
    }

    #[test]
    fn paste_moves_relative_references_and_cut_moves_the_cells() {
        let mut s = session();
        s.set_input(0, at("A1"), "2").unwrap();
        s.set_input(0, at("A2"), "3").unwrap();
        s.set_input(0, at("B1"), "=A1*10").unwrap();
        let clip = s.copy(0, Range::parse("B1").unwrap(), false);
        s.paste(&clip, 0, Range::parse("B2").unwrap(), PasteMode::All).unwrap();
        assert_eq!(shown(&s, "B2"), "30");
        let cut = s.copy(0, Range::parse("A1").unwrap(), true);
        s.paste(&cut, 0, Range::parse("D5").unwrap(), PasteMode::All).unwrap();
        assert_eq!(
            s.book.sheets[0].cell(at("B1")).unwrap().formula.as_ref().unwrap().display(),
            "=D5*10"
        );
        assert_eq!(shown(&s, "B1"), "20");
        assert_eq!(clip.to_text(&s.book), "20\r\n");
    }

    #[test]
    fn a_fill_continues_numbers_dates_names_and_counted_text() {
        let mut s = session();
        s.set_input(0, at("A1"), "1").unwrap();
        s.set_input(0, at("A2"), "3").unwrap();
        s.set_input(0, at("B1"), "Mon").unwrap();
        s.set_input(0, at("C1"), "Item 9").unwrap();
        s.set_input(0, at("D1"), "=A1*2").unwrap();
        s.fill_series(0, Range::parse("A1:A2").unwrap(), Range::parse("A1:A5").unwrap()).unwrap();
        s.fill_series(0, Range::parse("B1:D1").unwrap(), Range::parse("B1:D4").unwrap()).unwrap();
        assert_eq!(shown(&s, "A5"), "9");
        assert_eq!(shown(&s, "B3"), "Wed");
        assert_eq!(shown(&s, "C4"), "Item 12");
        assert_eq!(shown(&s, "D3"), "10");
    }

    #[test]
    fn sorting_orders_rows_and_keeps_the_header() {
        let mut s = session();
        for (i, (a, b)) in
            [("Name", "Score"), ("b", "2"), ("a", "3"), ("c", "1")].iter().enumerate()
        {
            s.set_input(0, CellRef::new(i as u32, 0), a).unwrap();
            s.set_input(0, CellRef::new(i as u32, 1), b).unwrap();
        }
        s.sort(0, Range::parse("A1:B4").unwrap(), &[(1, false)], true).unwrap();
        assert_eq!(shown(&s, "A1"), "Name");
        assert_eq!(shown(&s, "A2"), "a");
        assert_eq!(shown(&s, "A4"), "c");
    }

    #[test]
    fn renaming_a_sheet_rewrites_the_formulas_that_name_it() {
        let mut s = session();
        s.add_sheet(Some("Data"), None).unwrap();
        s.set_input(1, at("A1"), "7").unwrap();
        s.set_input(0, at("A1"), "=Data!A1*2").unwrap();
        s.rename_sheet(1, "My data").unwrap();
        assert_eq!(
            s.book.sheets[0].cell(at("A1")).unwrap().formula.as_ref().unwrap().display(),
            "='My data'!A1*2"
        );
        s.delete_sheet(1).unwrap();
        assert_eq!(shown(&s, "A1"), "#REF!");
        assert!(s.rename_sheet(0, "a/b").is_err());
    }

    #[test]
    fn validation_lists_and_ranges_are_checked() {
        let mut s = session();
        let v = Validation {
            ranges: vec![Range::parse("A1").unwrap()],
            kind: "list".to_owned(),
            operator: None,
            formula1: Some("\"Red,Green,Blue\"".to_owned()),
            formula2: None,
            allow_blank: true,
            show_dropdown: true,
            prompt_title: None,
            prompt: None,
            error_title: None,
            error: None,
            error_style: None,
            attrs: Vec::new(),
        };
        s.add_validation(0, v).unwrap();
        assert_eq!(s.list_choices(0, at("A1")), vec!["Red", "Green", "Blue"]);
        assert!(s.check_validation(0, at("A1"), &Value::text("green")).is_ok());
        assert!(s.check_validation(0, at("A1"), &Value::text("Pink")).is_err());
    }

    #[test]
    fn the_selection_summary_counts_and_sums() {
        let mut s = session();
        s.set_input(0, at("A1"), "2").unwrap();
        s.set_input(0, at("A2"), "4").unwrap();
        s.set_input(0, at("A3"), "x").unwrap();
        let sum = s.summary(0, &[Range::parse("A:A").unwrap()]);
        assert_eq!((sum.sum, sum.count, sum.numbers, sum.average()), (6.0, 3, 2, Some(3.0)));
        s.set_input(0, at("C1"), "1").unwrap();
        s.set_input(0, at("C2"), "2").unwrap();
        assert_eq!(s.autosum(0, at("C3")), "=SUM(C1:C2)");
    }

    #[test]
    fn a_filter_hides_the_rows_it_leaves_out() {
        let mut s = session();
        for (i, v) in ["Fruit", "apple", "pear", "apple"].iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, 0), v).unwrap();
        }
        s.set_filter(0, Some(Range::parse("A1:A4").unwrap())).unwrap();
        s.filter_column(
            0,
            0,
            Some(FilterCriteria::Values { values: vec!["apple".to_owned()], blank: false }),
        )
        .unwrap();
        assert!(s.book.sheets[0].is_row_hidden(2));
        assert!(!s.book.sheets[0].is_row_hidden(1));
    }
}
