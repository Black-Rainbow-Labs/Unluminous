//! Precedents, dependents and Trace Error.
//!
//! A cell's precedents are the cells its formula reads, and its dependents are the formulas that read it.
//! Both are answered one level at a time or through every level, as the ranges the formulas name, with the
//! sheet each is on so a reference to another sheet is marked, the way Excel draws a dotted line to a sheet
//! icon. Trace Error follows an error value back to the cells that made it.

use std::collections::{BTreeSet, VecDeque};

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::Workbook;
use crate::value::Value;

/// A range a trace reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reach {
    pub sheet: usize,
    pub range: Range,
    /// It is on another sheet than the cell the trace started from.
    pub other_sheet: bool,
}

/// Where an error came from: the cells that made it, and the arrows from each cell that passes it on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ErrorTrace {
    /// Cells holding the error with no error among what they read: the start of it.
    pub sources: Vec<(usize, CellRef)>,
    /// Pairs of a cell that holds an error and the cell that reads it and shows an error too.
    pub arrows: Vec<((usize, CellRef), (usize, CellRef))>,
}

/// A formula cell's precedents, one entry for each sheet a reference reaches.
type Reads = Vec<(usize, Range)>;

/// The ranges a formula cell reads, with the whole array range for a cell of a legacy array formula.
fn reads_of(book: &Workbook, sheet: usize, at: CellRef) -> Reads {
    let Some(s) = book.sheets.get(sheet) else { return Vec::new() };
    let Some(cell) = s.cell(at) else { return Vec::new() };
    let Some(formula) = cell.formula.as_ref() else {
        return match cell.formula.is_none().then(|| array_owner(s, at)).flatten() {
            Some(owner) => reads_of(book, sheet, owner),
            None => Vec::new(),
        };
    };
    let mut out = Vec::new();
    for (first, last, range) in crate::calc::precedents_of(book, sheet, &formula.expr) {
        for target in first..=last {
            out.push((target, range));
        }
    }
    out
}

/// The top left cell of the legacy array formula a cell is part of.
fn array_owner(s: &crate::model::Sheet, at: CellRef) -> Option<CellRef> {
    s.array_formulas.iter().find(|r| r.contains(at)).map(|r| r.first)
}

/// Every cell of the sheet that holds a formula, with the range it fills: one cell, or an array formula's.
fn formula_cells(book: &Workbook) -> Vec<(usize, CellRef, Range)> {
    let mut out = Vec::new();
    for (si, s) in book.sheets.iter().enumerate() {
        for (&(row, col), cell) in &s.cells {
            let Some(f) = cell.formula.as_ref() else { continue };
            let at = CellRef::new(row, col);
            out.push((si, at, f.array.unwrap_or(Range::cell(at))));
        }
    }
    out
}

fn push_unique(out: &mut Vec<Reach>, start_sheet: usize, sheet: usize, range: Range) {
    let reach = Reach { sheet, range, other_sheet: sheet != start_sheet };
    if !out.contains(&reach) {
        out.push(reach);
    }
}

impl Session {
    /// The ranges the formula in a cell reads, one level.
    pub fn direct_precedents(&self, sheet: usize, at: CellRef) -> Vec<Reach> {
        let mut out = Vec::new();
        for (s, range) in reads_of(&self.book, sheet, at) {
            push_unique(&mut out, sheet, s, range);
        }
        out
    }

    /// The ranges the formulas in some cells read, one level or through every formula those ranges hold.
    pub fn precedents(&self, sheet: usize, from: Range, all_levels: bool) -> Vec<Reach> {
        let mut out = Vec::new();
        let mut queue: VecDeque<(usize, Range)> = VecDeque::from([(sheet, from)]);
        let mut seen: BTreeSet<(usize, Range)> = BTreeSet::new();
        while let Some((s, range)) = queue.pop_front() {
            if !seen.insert((s, range)) {
                continue;
            }
            for at in self.formula_cells_in(s, range) {
                for (target, read) in reads_of(&self.book, s, at) {
                    push_unique(&mut out, sheet, target, read);
                    // The trace does not carry on into another sheet, as Excel's does not.
                    if all_levels && target == sheet {
                        queue.push_back((target, read));
                    }
                }
            }
        }
        out
    }

    /// The formula cells inside a range of a sheet.
    fn formula_cells_in(&self, sheet: usize, range: Range) -> Vec<CellRef> {
        let Some(s) = self.book.sheets.get(sheet) else { return Vec::new() };
        s.cells
            .range((range.first.row, 0)..=(range.last.row, u32::MAX))
            .filter(|(&(r, c), cell)| cell.formula.is_some() && range.contains(CellRef::new(r, c)))
            .map(|(&(r, c), _)| CellRef::new(r, c))
            .collect()
    }

    /// The formulas that read some cells, one level or through every formula that reads those.
    pub fn dependents(&self, sheet: usize, of: Range, all_levels: bool) -> Vec<Reach> {
        let formulas = formula_cells(&self.book);
        let reads: Vec<Reads> =
            formulas.iter().map(|(s, at, _)| reads_of(&self.book, *s, *at)).collect();
        let mut out = Vec::new();
        let mut queue: VecDeque<(usize, Range)> = VecDeque::from([(sheet, of)]);
        let mut done: BTreeSet<usize> = BTreeSet::new();
        while let Some((s, target)) = queue.pop_front() {
            for (i, (fs, _, fills)) in formulas.iter().enumerate() {
                let reads_it =
                    reads[i].iter().any(|(rs, range)| *rs == s && range.intersects(&target));
                if reads_it && done.insert(i) {
                    push_unique(&mut out, sheet, *fs, *fills);
                    if all_levels && *fs == sheet {
                        queue.push_back((*fs, *fills));
                    }
                }
            }
            if !all_levels {
                break;
            }
        }
        out
    }

    /// Follow an error value back to the cells that made it: Trace Error. Fails when the cell holds no
    /// error.
    pub fn trace_error(&self, sheet: usize, at: CellRef) -> Result<ErrorTrace, String> {
        if !self.holds_error(sheet, at) {
            return Err("The selected cell does not contain an error.".to_owned());
        }
        let mut trace = ErrorTrace::default();
        let mut queue: VecDeque<(usize, CellRef)> = VecDeque::from([(sheet, at)]);
        let mut seen: BTreeSet<(usize, CellRef)> = BTreeSet::from([(sheet, at)]);
        while let Some((s, cell)) = queue.pop_front() {
            let feeders = self.error_precedents(s, cell);
            if feeders.is_empty() {
                trace.sources.push((s, cell));
            }
            for feeder in feeders {
                trace.arrows.push((feeder, (s, cell)));
                if seen.insert(feeder) {
                    queue.push_back(feeder);
                }
            }
        }
        trace.arrows.sort();
        trace.sources.sort();
        Ok(trace)
    }

    fn holds_error(&self, sheet: usize, at: CellRef) -> bool {
        matches!(self.book.sheets.get(sheet).map(|s| s.value_at(at)), Some(Value::Error(_)))
    }

    /// The cells a formula reads that hold an error, from the first of its ranges to the last.
    fn error_precedents(&self, sheet: usize, at: CellRef) -> Vec<(usize, CellRef)> {
        let mut out = Vec::new();
        for (s, range) in reads_of(&self.book, sheet, at) {
            let Some(target) = self.book.sheets.get(s) else { continue };
            let mut errors: Vec<(u32, u32)> = target
                .cells
                .range((range.first.row, 0)..=(range.last.row, u32::MAX))
                .filter(|(&(r, c), cell)| {
                    range.contains(CellRef::new(r, c)) && matches!(cell.value, Value::Error(_))
                })
                .map(|(&k, _)| k)
                .collect();
            errors.sort_unstable();
            for (r, c) in errors {
                if !out.contains(&(s, CellRef::new(r, c))) {
                    out.push((s, CellRef::new(r, c)));
                }
            }
        }
        out
    }
}
