//! Go To Special: every option Excel's dialog has.
//!
//! The cells come back in reading order. A selection of one cell means the whole sheet, as it does in
//! Excel, and a selection of more means just those cells. When nothing qualifies the answer is Excel's
//! sentence, "No cells were found."

use crate::coord::{CellRef, Range};
use crate::edit::{clip_to_used, Session};
use crate::model::{Cell, Sheet};
use crate::value::Value;

/// Which kinds of value Constants and Formulas take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kinds {
    pub numbers: bool,
    pub text: bool,
    pub logicals: bool,
    pub errors: bool,
}

impl Kinds {
    /// Every kind, which is what the dialog does with all four boxes ticked.
    pub const ALL: Kinds = Kinds { numbers: true, text: true, logicals: true, errors: true };

    /// Whether a value is one of the kinds. A formula's empty result counts as text, as Excel counts it.
    pub fn admits(&self, value: &Value) -> bool {
        match value {
            Value::Number(_) => self.numbers,
            Value::Text(_) | Value::Empty => self.text,
            Value::Bool(_) => self.logicals,
            Value::Error(_) => self.errors,
        }
    }
}

/// One option of the Go To Special dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// Cells with a note.
    Notes,
    /// Cells with a threaded comment.
    Comments,
    Constants(Kinds),
    Formulas(Kinds),
    Blanks,
    CurrentRegion,
    CurrentArray,
    RowDifferences,
    ColumnDifferences,
    Precedents {
        all_levels: bool,
    },
    Dependents {
        all_levels: bool,
    },
    LastCell,
    VisibleCells,
    /// Cells with a conditional format: all of them, or those with the same ones as the active cell.
    ConditionalFormats {
        same: bool,
    },
    /// Cells with data validation: all of them, or those with the same as the active cell.
    Validation {
        same: bool,
    },
}

/// An object drawn over the cells: a chart, a picture or a shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawingObject {
    /// `chart`, `picture`, `shape`, `group`, `connector` or `control`.
    pub kind: &'static str,
    pub from: CellRef,
    pub to: CellRef,
}

const NOT_FOUND: &str = "No cells were found.";

impl Session {
    /// The cells Go To Special selects. `selection` is what is selected and `active` the cell with the
    /// cursor, which the options that compare or start from a cell use.
    pub fn special_cells(
        &self,
        sheet: usize,
        selection: &[Range],
        active: CellRef,
        kind: Special,
    ) -> Result<Vec<CellRef>, String> {
        self.check_sheet(sheet)?;
        let s = &self.book.sheets[sheet];
        let area = search_area(s, selection);
        let found = match kind {
            Special::Notes => {
                cells_where(s, &area, |s, at| s.notes.contains_key(&(at.row, at.col)))
            }
            Special::Comments => {
                cells_where(s, &area, |s, at| s.threads.contains_key(&(at.row, at.col)))
            }
            Special::Constants(kinds) => cells_where(s, &area, |s, at| {
                s.cell(at).is_some_and(|c| {
                    c.formula.is_none() && !c.value.is_empty() && kinds.admits(&c.value)
                })
            }),
            Special::Formulas(kinds) => cells_where(s, &area, |s, at| {
                s.cell(at).is_some_and(|c| c.formula.is_some() && kinds.admits(&c.value))
            }),
            Special::Blanks => blank_cells(s, &area),
            Special::CurrentRegion => vec![current_region(s, active)]
                .into_iter()
                .flat_map(|r| r.cells().collect::<Vec<_>>())
                .collect(),
            Special::CurrentArray => self.current_array(sheet, active),
            Special::RowDifferences => differences(s, selection, active, true),
            Special::ColumnDifferences => differences(s, selection, active, false),
            Special::Precedents { all_levels } => {
                self.traced_cells(sheet, selection, active, all_levels, true)
            }
            Special::Dependents { all_levels } => {
                self.traced_cells(sheet, selection, active, all_levels, false)
            }
            Special::LastCell => last_cell(s).into_iter().collect(),
            Special::VisibleCells => cells_in(&area).filter(|at| visible(s, *at)).collect(),
            Special::ConditionalFormats { same } => formats_where(s, &area, active, same),
            Special::Validation { same } => validation_where(self, sheet, &area, active, same),
        };
        if found.is_empty() {
            return Err(NOT_FOUND.to_owned());
        }
        Ok(found)
    }

    /// The range of the legacy array formula a cell is part of, or the range a dynamic array filled.
    fn current_array(&self, sheet: usize, at: CellRef) -> Vec<CellRef> {
        let s = &self.book.sheets[sheet];
        if let Some(range) = s.array_formulas.iter().find(|r| r.contains(at)) {
            return range.cells().collect();
        }
        let anchor = s.spills.owner.get(&(at.row, at.col)).copied().unwrap_or((at.row, at.col));
        s.spills.range_of(anchor).map(|r| r.cells().collect()).unwrap_or_default()
    }

    /// The cells on the sheet that precedents or dependents of the selection are, as Excel selects them.
    fn traced_cells(
        &self,
        sheet: usize,
        selection: &[Range],
        active: CellRef,
        all_levels: bool,
        precedents: bool,
    ) -> Vec<CellRef> {
        let from: Vec<Range> =
            if selection.is_empty() { vec![Range::cell(active)] } else { selection.to_vec() };
        let mut cells = std::collections::BTreeSet::new();
        for range in from {
            let reaches = if precedents {
                self.precedents(sheet, range, all_levels)
            } else {
                self.dependents(sheet, range, all_levels)
            };
            for reach in reaches.iter().filter(|r| !r.other_sheet) {
                let clipped = clip_to_used(&self.book.sheets[sheet], reach.range);
                cells.extend(clipped.cells());
            }
        }
        cells.into_iter().collect()
    }

    /// The objects drawn over a sheet's cells, in the order the drawing holds them.
    pub fn drawing_objects(&self, sheet: usize) -> Vec<DrawingObject> {
        let Some(s) = self.book.sheets.get(sheet) else { return Vec::new() };
        let mut out = kept_drawing_objects(&self.book, s);
        for chart in s.charts.iter().filter(|c| c.new) {
            out.push(DrawingObject { kind: "chart", from: chart.from.0, to: chart.to.0 });
        }
        out
    }
}

/// The objects in the drawing part a sheet keeps.
fn kept_drawing_objects(book: &crate::model::Workbook, s: &Sheet) -> Vec<DrawingObject> {
    let Some(rel) = s.raw.rels.iter().find(|r| r.kind == crate::xlsx::REL_DRAWING) else {
        return Vec::new();
    };
    let path = crate::xlsx::resolve(&s.raw.part, &rel.target);
    let Some(bytes) = book.kept.get(&path) else { return Vec::new() };
    let Ok(text) = std::str::from_utf8(bytes) else { return Vec::new() };
    let Ok(doc) = roxmltree::Document::parse(text) else { return Vec::new() };
    let mut out = Vec::new();
    for anchor in doc.root_element().children().filter(roxmltree::Node::is_element) {
        let corner = |name: &str| -> CellRef {
            let node = anchor.children().find(|n| n.is_element() && n.tag_name().name() == name);
            let number = |tag: &str| {
                node.and_then(|n| {
                    n.children().find(|c| c.is_element() && c.tag_name().name() == tag)
                })
                .and_then(|c| c.text())
                .and_then(|t| t.trim().parse::<u32>().ok())
                .unwrap_or(0)
            };
            CellRef::new(number("row"), number("col"))
        };
        let kind = anchor.children().filter(roxmltree::Node::is_element).find_map(|n| {
            match n.tag_name().name() {
                "pic" => Some("picture"),
                "graphicFrame" => Some("chart"),
                "sp" => Some("shape"),
                "grpSp" => Some("group"),
                "cxnSp" => Some("connector"),
                "AlternateContent" => Some("control"),
                _ => None,
            }
        });
        if let Some(kind) = kind {
            out.push(DrawingObject { kind, from: corner("from"), to: corner("to") });
        }
    }
    out
}

/// The cells a special search looks in: the selection, or the whole sheet when one cell is selected.
fn search_area(s: &Sheet, selection: &[Range]) -> Vec<Range> {
    let single = selection.is_empty() || (selection.len() == 1 && selection[0].len() == 1);
    if single {
        return used_with_formats(s).into_iter().collect();
    }
    selection.iter().map(|r| clip_to_used(s, *r)).collect()
}

/// The rectangle from A1 to the last row and column anything is in, formatting and spills included.
fn used_with_formats(s: &Sheet) -> Option<Range> {
    let mut last = None::<(u32, u32)>;
    for &(r, c) in s.cells.keys().chain(s.spills.values.keys()) {
        last = Some(last.map_or((r, c), |(lr, lc)| (lr.max(r), lc.max(c))));
    }
    let (row, col) = last?;
    Some(Range::new(CellRef::new(0, 0), CellRef::new(row, col)))
}

fn cells_in(area: &[Range]) -> impl Iterator<Item = CellRef> + '_ {
    area.iter().flat_map(|r| r.cells())
}

fn cells_where(s: &Sheet, area: &[Range], test: impl Fn(&Sheet, CellRef) -> bool) -> Vec<CellRef> {
    let mut out: Vec<CellRef> = Vec::new();
    for range in area {
        // Only the places something is stored can qualify, so a range of a million cells is not walked.
        let mut keys: Vec<(u32, u32)> = s
            .cells
            .range((range.first.row, 0)..=(range.last.row, u32::MAX))
            .map(|(&k, _)| k)
            .collect();
        keys.extend(s.notes.keys().copied());
        keys.extend(s.threads.keys().copied());
        keys.sort_unstable();
        keys.dedup();
        out.extend(
            keys.into_iter()
                .map(|(r, c)| CellRef::new(r, c))
                .filter(|at| range.contains(*at) && test(s, *at)),
        );
    }
    out.sort();
    out.dedup();
    out
}

fn is_blank(s: &Sheet, at: CellRef) -> bool {
    s.value_at(at).is_empty() && s.cell(at).is_none_or(|c| c.formula.is_none())
}

fn blank_cells(s: &Sheet, area: &[Range]) -> Vec<CellRef> {
    cells_in(area).filter(|at| is_blank(s, *at)).collect()
}

fn visible(s: &Sheet, at: CellRef) -> bool {
    !s.is_row_hidden(at.row) && !s.is_column_hidden(at.col)
}

/// The last cell of the used range: the last row and the last column anything is in.
fn last_cell(s: &Sheet) -> Option<CellRef> {
    used_with_formats(s).map(|r| r.last)
}

/// The block of cells around a cell that blank rows and columns bound.
pub fn current_region(s: &Sheet, active: CellRef) -> Range {
    let filled = |at: CellRef| !is_blank(s, at);
    let mut region = Range::cell(active);
    loop {
        let mut next = region;
        let (r0, r1) = (region.first.row.saturating_sub(1), region.last.row.saturating_add(1));
        let (c0, c1) = (region.first.col.saturating_sub(1), region.last.col.saturating_add(1));
        for row in r0..=r1.min(crate::coord::MAX_ROWS - 1) {
            for col in c0..=c1.min(crate::coord::MAX_COLS - 1) {
                let at = CellRef::new(row, col);
                if !region.contains(at) && filled(at) {
                    next = next.union(&Range::cell(at));
                }
            }
        }
        let grown = next;
        for merge in s.merges.iter().filter(|m| m.intersects(&grown)) {
            next = next.union(merge);
        }
        if next == region {
            return region;
        }
        region = next;
    }
}

/// Whether two cells hold the same thing: the same formula as if one were copied to the other, or the same
/// constant.
fn same_content(a: Option<&Cell>, a_at: CellRef, b: Option<&Cell>, b_at: CellRef) -> bool {
    match (a.and_then(|c| c.formula.as_ref()), b.and_then(|c| c.formula.as_ref())) {
        (Some(fa), Some(fb)) => {
            let moved = crate::formula::shifted(
                &fa.expr,
                b_at.row as i64 - a_at.row as i64,
                b_at.col as i64 - a_at.col as i64,
            );
            moved == *fb.expr
        }
        (None, None) => {
            let (va, vb) = (a.map(|c| &c.value), b.map(|c| &c.value));
            match (va, vb) {
                (Some(x), Some(y)) => crate::eval::equal(x, y) && x.is_empty() == y.is_empty(),
                (None, None) => true,
                (Some(x), None) | (None, Some(x)) => x.is_empty(),
            }
        }
        _ => false,
    }
}

/// The cells of the selection that differ from the comparison cell of their row (or column): the one in the
/// active cell's column (or row).
fn differences(s: &Sheet, selection: &[Range], active: CellRef, by_row: bool) -> Vec<CellRef> {
    let mut out = Vec::new();
    for range in selection.iter().filter(|r| r.len() > 1) {
        for at in range.cells() {
            let base = if by_row {
                CellRef::new(at.row, active.col)
            } else {
                CellRef::new(active.row, at.col)
            };
            if base == at {
                continue;
            }
            if !same_content(s.cell(base), base, s.cell(at), at) {
                out.push(at);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn formats_where(s: &Sheet, area: &[Range], active: CellRef, same: bool) -> Vec<CellRef> {
    let rules_at = |at: CellRef| -> Vec<&crate::model::CfRule> {
        s.conditional
            .iter()
            .filter(|cf| cf.ranges.iter().any(|r| r.contains(at)))
            .flat_map(|cf| cf.rules.iter())
            .collect()
    };
    let wanted = rules_at(active);
    if same && wanted.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for range in area {
        for cf in &s.conditional {
            for covered in cf.ranges.iter().filter_map(|r| r.intersection(range)) {
                for at in covered.cells() {
                    let rules = rules_at(at);
                    if !rules.is_empty() && (!same || rules == wanted) {
                        out.push(at);
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn validation_where(
    session: &Session,
    sheet: usize,
    area: &[Range],
    active: CellRef,
    same: bool,
) -> Vec<CellRef> {
    let wanted = session.validation_at(sheet, active).map(shape_of);
    if same && wanted.is_none() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for range in area {
        for v in &session.book.sheets[sheet].validations {
            for covered in v.ranges.iter().filter_map(|r| r.intersection(range)) {
                for at in covered.cells() {
                    if !same || Some(shape_of(v)) == wanted {
                        out.push(at);
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A validation without the cells it covers, so two can be compared.
fn shape_of(v: &crate::model::Validation) -> crate::model::Validation {
    crate::model::Validation { ranges: Vec::new(), ..v.clone() }
}

/// Rectangles of cells that together make up some cells, reading order: each row's runs joined to the same
/// run in the row above, as Excel's areas are.
pub fn areas_of(cells: &[CellRef]) -> Vec<Range> {
    let mut runs: Vec<Range> = Vec::new();
    let mut sorted = cells.to_vec();
    sorted.sort();
    for at in sorted {
        match runs.last_mut() {
            Some(last) if last.last.row == at.row && last.last.col + 1 == at.col => {
                last.last = at;
            }
            _ => runs.push(Range::cell(at)),
        }
    }
    let mut areas: Vec<Range> = Vec::new();
    for run in runs {
        match areas.iter_mut().find(|a| {
            a.last.row + 1 == run.first.row
                && a.first.col == run.first.col
                && a.last.col == run.last.col
        }) {
            Some(area) => area.last.row = run.first.row,
            None => areas.push(run),
        }
    }
    areas
}
