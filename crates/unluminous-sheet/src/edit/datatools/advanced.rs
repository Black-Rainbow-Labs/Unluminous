//! Advanced Filter: a list filtered by a criteria range, in place or copied to another place.
//!
//! The first row of the criteria range names fields of the list, and each row below it is a set of tests
//! that all must hold; a record is kept when any row holds. A test in a column whose title is not a field of
//! the list is a formula, written for the first record, that is worked out for each record. A text test
//! matches the start of the text unless it begins with `=`, and `?`, `*` and `~` work as they do in
//! SUMIF. The comparison ignores case.

use crate::edit::Guard;
use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::formula::{self, Formula};
use crate::functions::criteria::wildcard_match;
use crate::input::number_from_text;
use crate::model::*;
use crate::value::Value;

use super::collate;
use super::displayed;

const EXTRACT_MESSAGE: &str = "The extract range has a missing or invalid field name.";

impl Session {
    /// Filter a list (its first row holds the field names) by a criteria range. In place the rows that
    /// fail are hidden; with `copy_to` the records that pass are copied there, headers first. `unique`
    /// leaves out a record that repeats an earlier one.
    pub fn advanced_filter(
        &mut self,
        sheet: usize,
        list: Range,
        criteria: Option<Range>,
        copy_to: Option<Range>,
        unique: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        if list.rows() < 2 && criteria.is_some() {
            return Err("The list range is not valid.".to_owned());
        }
        let fields = self.field_names(sheet, list);
        let kept = self.matching_records(sheet, list, &fields, criteria);
        let columns = match copy_to {
            Some(target) => Some(self.extract_columns(sheet, target, list, &fields)?),
            None => None,
        };
        if let Some(target) = copy_to {
            self.refuse_locked(sheet, target)?;
        }
        self.remember_whole("Advanced filter");
        let kept = self.without_repeats(sheet, list, columns.as_deref(), criteria, unique, kept);
        match (copy_to, columns) {
            (Some(target), Some(columns)) => {
                self.copy_records(sheet, list, target, &columns, &kept);
                self.name_filter_ranges(
                    sheet,
                    list,
                    criteria,
                    Some(extract_row(target, columns.len())),
                );
            }
            _ => {
                // The list becomes Excel's filter database, which replaces the sheet's AutoFilter.
                self.book.sheets[sheet].filter = None;
                self.hide_unmatched(sheet, list, &kept);
                self.name_filter_ranges(sheet, list, criteria, None);
            }
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Show every row of the sheet, which is what Data > Clear does after an Advanced Filter.
    pub fn show_all_rows(&mut self, sheet: usize) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Show all rows");
        let s = &mut self.book.sheets[sheet];
        for info in s.rows.values_mut() {
            info.hidden = false;
        }
        s.rows.retain(|_, info| *info != RowInfo::default());
        self.recalculate_all();
        Ok(())
    }

    /// The field names of a list, as the header row shows them.
    fn field_names(&self, sheet: usize, list: Range) -> Vec<String> {
        (list.first.col..=list.last.col)
            .map(|c| {
                let cell = self.book.sheets[sheet].cell(CellRef::new(list.first.row, c)).cloned();
                cell.map(|cell| displayed(&self.book, &cell)).unwrap_or_default()
            })
            .collect()
    }

    /// The rows of the list that pass the criteria, in order.
    fn matching_records(
        &self,
        sheet: usize,
        list: Range,
        fields: &[String],
        criteria: Option<Range>,
    ) -> Vec<u32> {
        (list.first.row + 1..=list.last.row)
            .filter(|row| criteria.is_none_or(|c| self.record_passes(sheet, list, fields, c, *row)))
            .collect()
    }

    /// Whether any row of the criteria range holds for one record.
    fn record_passes(
        &self,
        sheet: usize,
        list: Range,
        fields: &[String],
        criteria: Range,
        record: u32,
    ) -> bool {
        let rows: Vec<u32> = (criteria.first.row + 1..=criteria.last.row).collect();
        if rows.is_empty() {
            return true;
        }
        rows.iter().any(|row| {
            (criteria.first.col..=criteria.last.col).all(|col| {
                self.criterion_holds(sheet, list, fields, criteria, CellRef::new(*row, col), record)
            })
        })
    }

    /// One cell of a criteria row against one record.
    fn criterion_holds(
        &self,
        sheet: usize,
        list: Range,
        fields: &[String],
        criteria: Range,
        at: CellRef,
        record: u32,
    ) -> bool {
        let s = &self.book.sheets[sheet];
        let Some(cell) = s.cell(at).filter(|c| !c.value.is_empty() || c.formula.is_some()) else {
            return true;
        };
        let title = s.cell(CellRef::new(criteria.first.row, at.col)).cloned().unwrap_or_default();
        let title = displayed(&self.book, &title);
        let field = fields.iter().position(|f| !title.is_empty() && f.eq_ignore_ascii_case(&title));
        match field {
            Some(offset) => {
                let value = s.value_at(CellRef::new(record, list.first.col + offset as u32));
                matches_criterion(&cell.value, &value, self.book.date1904)
            }
            None => match &cell.formula {
                // A formula written for the first record, worked out for this one.
                Some(f) => {
                    let delta = record as i64 - (list.first.row + 1) as i64;
                    let moved = Formula::from_expr(formula::shifted(&f.expr, delta, 0), f.dynamic);
                    match self.evaluate_at(sheet, at, &moved.display()) {
                        Ok(Value::Bool(b)) => b,
                        Ok(Value::Number(n)) => n != 0.0,
                        _ => false,
                    }
                }
                None => false,
            },
        }
    }

    /// The columns of the list that are copied, from the header row of the extract range.
    fn extract_columns(
        &self,
        sheet: usize,
        target: Range,
        _list: Range,
        fields: &[String],
    ) -> Result<Vec<u32>, String> {
        let s = &self.book.sheets[sheet];
        let titles: Vec<String> = (target.first.col..=target.last.col)
            .map(|c| {
                let cell = s.cell(CellRef::new(target.first.row, c)).cloned().unwrap_or_default();
                displayed(&self.book, &cell)
            })
            .collect();
        if titles.iter().all(String::is_empty) && target.first.col == target.last.col {
            return Ok((0..fields.len() as u32).collect());
        }
        titles
            .iter()
            .map(|t| {
                fields
                    .iter()
                    .position(|f| !t.is_empty() && f.eq_ignore_ascii_case(t))
                    .map(|p| p as u32)
                    .ok_or_else(|| EXTRACT_MESSAGE.to_owned())
            })
            .collect::<Result<Vec<u32>, String>>()
    }

    /// Leave out the records that repeat an earlier one when only unique records were asked for. Copying
    /// with no criteria range keeps every record, as Excel does.
    fn without_repeats(
        &self,
        sheet: usize,
        list: Range,
        columns: Option<&[u32]>,
        criteria: Option<Range>,
        unique: bool,
        kept: Vec<u32>,
    ) -> Vec<u32> {
        let subset = columns.is_some_and(|c| c.len() < list.cols() as usize);
        if !unique || (subset && criteria.is_none()) {
            return kept;
        }
        let s = &self.book.sheets[sheet];
        let all: Vec<u32> = (0..list.cols()).collect();
        let columns = columns.unwrap_or(&all);
        let mut seen: Vec<Vec<String>> = Vec::new();
        let mut out = Vec::new();
        for row in kept {
            let key: Vec<String> = columns
                .iter()
                .map(|c| {
                    let cell = s.cell(CellRef::new(row, list.first.col + c)).cloned();
                    cell.map(|cell| displayed(&self.book, &cell).to_lowercase()).unwrap_or_default()
                })
                .collect();
            if !seen.contains(&key) {
                seen.push(key);
                out.push(row);
            }
        }
        out
    }

    /// Hide the records of the list that were not kept and show the ones that were.
    fn hide_unmatched(&mut self, sheet: usize, list: Range, kept: &[u32]) {
        let s = &mut self.book.sheets[sheet];
        for r in list.first.row + 1..=list.last.row {
            let info = s.rows.entry(r).or_default();
            info.hidden = !kept.contains(&r);
            if *info == RowInfo::default() {
                s.rows.remove(&r);
            }
        }
    }

    /// Copy the kept records to the extract range: values and formats, headers first.
    fn copy_records(
        &mut self,
        sheet: usize,
        list: Range,
        target: Range,
        columns: &[u32],
        kept: &[u32],
    ) {
        let mut cells: Vec<((u32, u32), Cell)> = Vec::new();
        let s = &self.book.sheets[sheet];
        let rows = std::iter::once(list.first.row).chain(kept.iter().copied());
        for (i, row) in rows.enumerate() {
            for (j, offset) in columns.iter().enumerate() {
                let from = (row, list.first.col + offset);
                let to = (target.first.row + i as u32, target.first.col + j as u32);
                let source = s.cells.get(&from).cloned().unwrap_or_default();
                cells.push((to, Cell { formula: None, rich: None, extra: None, ..source }));
            }
        }
        let s = &mut self.book.sheets[sheet];
        // Excel empties the extract columns below the header before it copies.
        let (first, last) = (target.first.col, target.first.col + columns.len() as u32 - 1);
        s.cells.retain(|&(r, c), _| !(r > target.first.row && (first..=last).contains(&c)));
        for (key, cell) in cells {
            if cell.is_blank() {
                s.cells.remove(&key);
            } else {
                s.cells.insert(key, cell);
            }
        }
    }

    /// The names Excel keeps for the list, the criteria range and the extract range of a sheet.
    fn name_filter_ranges(
        &mut self,
        sheet: usize,
        list: Range,
        criteria: Option<Range>,
        extract: Option<Range>,
    ) {
        let name = crate::formula::print::sheet_name(&self.book.sheets[sheet].name);
        let absolute = |r: Range| {
            if r.first == r.last {
                return format!(
                    "{name}!${}${}",
                    crate::coord::column_name(r.first.col),
                    r.first.row + 1
                );
            }
            format!(
                "{name}!${}${}:${}${}",
                crate::coord::column_name(r.first.col),
                r.first.row + 1,
                crate::coord::column_name(r.last.col),
                r.last.row + 1
            )
        };
        let put = |book: &mut Workbook, key: &str, range: Option<Range>, hidden: bool| {
            book.names.retain(|n| !(n.name == key && n.scope == Some(sheet)));
            if let Some(r) = range {
                book.names.push(DefinedName {
                    name: key.to_owned(),
                    scope: Some(sheet),
                    formula: absolute(r),
                    hidden,
                    attrs: Vec::new(),
                });
            }
        };
        put(&mut self.book, "_xlnm._FilterDatabase", Some(list), true);
        put(&mut self.book, "_xlnm.Criteria", criteria, false);
        put(&mut self.book, "_xlnm.Extract", extract, false);
    }
}

/// The header row of the extract range as it was written.
fn extract_row(target: Range, columns: usize) -> Range {
    Range::new(target.first, CellRef::new(target.first.row, target.first.col + columns as u32 - 1))
}

/// Whether a record's value passes one criteria cell.
fn matches_criterion(criterion: &Value, value: &Value, date1904: bool) -> bool {
    match criterion {
        Value::Number(n) => matches!(value, Value::Number(v) if v == n),
        Value::Bool(b) => matches!(value, Value::Bool(v) if v == b),
        Value::Text(t) => matches_text(t, value, date1904),
        _ => false,
    }
}

/// A text criterion: an operator and an operand, or text that must start a text.
fn matches_text(criterion: &str, value: &Value, date1904: bool) -> bool {
    let (op, operand) = split_operator(criterion);
    let text = value.to_text();
    match op {
        "" => match number_from_text(operand, date1904) {
            Some(n) => matches!(value, Value::Number(v) if *v == n),
            None => {
                !matches!(value, Value::Number(_) | Value::Bool(_) | Value::Empty)
                    && wildcard_match(&format!("{operand}*"), &text)
            }
        },
        "=" | "<>" => {
            let equal = if operand.is_empty() {
                value.is_empty()
            } else {
                match number_from_text(operand, date1904) {
                    Some(n) if matches!(value, Value::Number(_)) => {
                        matches!(value, Value::Number(v) if *v == n)
                    }
                    _ => !value.is_empty() && wildcard_match(operand, &text),
                }
            };
            if op == "=" {
                equal
            } else {
                !equal
            }
        }
        _ => relational(op, operand, value, date1904),
    }
}

fn relational(op: &str, operand: &str, value: &Value, date1904: bool) -> bool {
    let order = match (number_from_text(operand, date1904), value) {
        (Some(n), Value::Number(v)) => v.partial_cmp(&n),
        (None, Value::Text(t)) => Some(collate::compare(t, operand, false)),
        _ => None,
    };
    use std::cmp::Ordering::*;
    matches!(
        (op, order),
        (">", Some(Greater))
            | ("<", Some(Less))
            | (">=", Some(Greater | Equal))
            | ("<=", Some(Less | Equal))
    )
}

fn split_operator(text: &str) -> (&str, &str) {
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(rest) = text.strip_prefix(op) {
            return (op, rest);
        }
    }
    ("", text)
}
