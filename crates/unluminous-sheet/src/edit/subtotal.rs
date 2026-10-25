//! Data, Subtotal: a row of `SUBTOTAL` formulas at each change in a column, a grand total, and the outline
//! that folds the rows between.
//!
//! What Excel does was read off its output for each option (the fixtures under `tests/fixtures/outline`).
//! The subtotal row is labelled in the group column with the group's first value and the function's
//! name (`East Total`), the grand total says `Grand Total`, the label cells are bold, and a group is the
//! run of rows with the same value in the group column, compared without regard to case. A row with no
//! value there belongs to no group. The outline is the details one level deeper than their subtotal row
//! and the whole list one level deeper than the grand total.

use super::{Before, Session, Step};
use crate::coord::{column_name, CellRef, Range};
use crate::eval::compare;
use crate::formula::{Axis, Expr, Formula};
use crate::model::{Cell, Sheet};
use crate::outline::{self, Dim};
use crate::styles::{FontEdit, StyleChange};
use crate::value::Value;

/// The function a subtotal row uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtotalFunction {
    Sum,
    /// Counts the cells that are not empty (`SUBTOTAL(3)`).
    Count,
    Average,
    Max,
    Min,
    Product,
    /// Counts the cells that hold numbers (`SUBTOTAL(2)`).
    CountNumbers,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

impl SubtotalFunction {
    /// The first argument of `SUBTOTAL`.
    pub fn code(self) -> u8 {
        match self {
            SubtotalFunction::Average => 1,
            SubtotalFunction::CountNumbers => 2,
            SubtotalFunction::Count => 3,
            SubtotalFunction::Max => 4,
            SubtotalFunction::Min => 5,
            SubtotalFunction::Product => 6,
            SubtotalFunction::StdDev => 7,
            SubtotalFunction::StdDevP => 8,
            SubtotalFunction::Sum => 9,
            SubtotalFunction::Var => 10,
            SubtotalFunction::VarP => 11,
        }
    }

    /// The word after the group's value in the label: `Total`, `Count`, `Average`.
    pub fn label(self) -> &'static str {
        match self {
            SubtotalFunction::Sum => "Total",
            SubtotalFunction::Count | SubtotalFunction::CountNumbers => "Count",
            SubtotalFunction::Average => "Average",
            SubtotalFunction::Max => "Max",
            SubtotalFunction::Min => "Min",
            SubtotalFunction::Product => "Product",
            SubtotalFunction::StdDev => "StdDev",
            SubtotalFunction::StdDevP => "StdDevp",
            SubtotalFunction::Var => "Var",
            SubtotalFunction::VarP => "Varp",
        }
    }

    /// The function a name in a command or a menu stands for.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().replace([' ', '-', '_'], "").as_str() {
            "sum" | "total" => SubtotalFunction::Sum,
            "count" | "counta" => SubtotalFunction::Count,
            "average" | "avg" => SubtotalFunction::Average,
            "max" => SubtotalFunction::Max,
            "min" => SubtotalFunction::Min,
            "product" => SubtotalFunction::Product,
            "countnumbers" | "countnums" => SubtotalFunction::CountNumbers,
            "stddev" | "stdev" => SubtotalFunction::StdDev,
            "stddevp" | "stdevp" => SubtotalFunction::StdDevP,
            "var" => SubtotalFunction::Var,
            "varp" => SubtotalFunction::VarP,
            _ => return None,
        })
    }
}

/// What the Subtotal dialog asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtotalSpec {
    /// The list, its header row first.
    pub range: Range,
    /// "At each change in": the column of the list that groups the rows, counted from 0 at the left of
    /// `range`.
    pub group_by: u32,
    /// "Use function".
    pub function: SubtotalFunction,
    /// "Add subtotal to": the columns of the list, counted from 0 at the left of `range`.
    pub total_columns: Vec<u32>,
    /// "Replace current subtotals".
    pub replace: bool,
    /// "Page break between groups".
    pub page_breaks: bool,
    /// "Summary below data".
    pub summary_below: bool,
}

/// A run of rows with the same value in the group column.
struct Group {
    first: u32,
    last: u32,
    key: String,
}

/// Where a row will be inserted and what goes in it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Insert {
    Grand,
    Group(usize),
}

impl Session {
    /// Subtotal a list as Data, Subtotal does. One undo step; refused with the workbook unchanged when
    /// the list has no rows or a column is outside it.
    pub fn subtotal(&mut self, sheet: usize, spec: &SubtotalSpec) -> Result<(), String> {
        self.check_sheet(sheet)?;
        check_spec(spec)?;
        let backup = Box::new(self.book.clone());
        match self.run_subtotal(sheet, spec) {
            Ok(()) => {
                self.finish_subtotal("Subtotal", backup);
                Ok(())
            }
            Err(e) => {
                self.book = *backup;
                self.engine.rebuild(&self.book);
                Err(e)
            }
        }
    }

    /// Remove All: take out the subtotal rows and the grand total of a list and clear its outline.
    pub fn remove_subtotal(&mut self, sheet: usize, range: Range) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let backup = Box::new(self.book.clone());
        let last = list_end(&self.book.sheets[sheet], range).unwrap_or(range.first.row);
        let removed = self.delete_subtotal_rows(sheet, range, last);
        let end = last - removed;
        outline::clear(&mut self.book.sheets[sheet], Dim::Rows, range.first.row, end);
        self.finish_subtotal("Remove subtotals", backup);
        Ok(())
    }

    fn finish_subtotal(&mut self, label: &str, backup: Box<crate::model::Workbook>) {
        let mut before = *backup;
        before.circular = None;
        self.push(Step { label: label.to_owned(), before: Before::Whole(Box::new(before)) });
        self.engine.rebuild(&self.book);
        self.recalculate_all();
    }

    fn run_subtotal(&mut self, sheet: usize, spec: &SubtotalSpec) -> Result<(), String> {
        let mut range = spec.range;
        let mut last = list_end(&self.book.sheets[sheet], range)
            .filter(|l| *l > range.first.row)
            .ok_or("There are no rows to subtotal.")?;
        if spec.replace {
            let removed = self.delete_subtotal_rows(sheet, range, last);
            last -= removed;
            let header = range.first.row;
            outline::clear(&mut self.book.sheets[sheet], Dim::Rows, header, last);
        }
        let mut group_col = range.first.col + spec.group_by;
        let mut label_col = group_col;
        let mut totals: Vec<u32> = spec.total_columns.iter().map(|t| range.first.col + t).collect();
        if totals.contains(&group_col) {
            // The label would sit on the numbers being totalled, so Excel makes room: a new column at the
            // left of the list takes the labels.
            self.structural_edit(sheet, Axis::Columns, range.first.col, 1, None);
            label_col = range.first.col;
            group_col += 1;
            totals.iter_mut().for_each(|t| *t += 1);
            range = Range::new(
                CellRef::new(range.first.row, range.first.col + 1),
                CellRef::new(range.last.row, range.last.col + 1),
            );
        }
        let plan = self.plan(sheet, spec, range, last, group_col)?;
        self.apply_plan(sheet, spec, &plan, label_col, &totals)
    }

    /// The rows from the first row after the header to `last` that are subtotal rows, deleted. Answers how
    /// many.
    fn delete_subtotal_rows(&mut self, sheet: usize, range: Range, last: u32) -> u32 {
        let subtotal_rows: Vec<u32> = (range.first.row + 1..=last)
            .filter(|r| is_subtotal_row(&self.book.sheets[sheet], *r, range))
            .collect();
        for &r in subtotal_rows.iter().rev() {
            self.structural_edit(sheet, Axis::Rows, r, -1, None);
        }
        subtotal_rows.len() as u32
    }
}

fn check_spec(spec: &SubtotalSpec) -> Result<(), String> {
    let cols = spec.range.cols();
    if spec.range.rows() < 2 {
        return Err(
            "Subtotal needs a list with a header row and at least one row of data.".to_owned()
        );
    }
    if spec.group_by >= cols || spec.total_columns.iter().any(|t| *t >= cols) {
        return Err("A column to subtotal is outside the list.".to_owned());
    }
    if spec.total_columns.is_empty() {
        return Err("Choose at least one column to add the subtotal to.".to_owned());
    }
    Ok(())
}

/// The last row of a list that holds anything in the columns of `range`, after its header.
fn list_end(s: &Sheet, range: Range) -> Option<u32> {
    (range.first.row + 1..=range.last.row).rev().find(|r| !row_is_empty(s, *r, range))
}

fn row_is_empty(s: &Sheet, row: u32, range: Range) -> bool {
    (range.first.col..=range.last.col).all(|c| {
        s.cells.get(&(row, c)).is_none_or(|cell| cell.value.is_empty() && cell.formula.is_none())
    })
}

/// Whether a row of the list is a subtotal row: one of its cells is a `SUBTOTAL` formula.
fn is_subtotal_row(s: &Sheet, row: u32, range: Range) -> bool {
    (range.first.col..=range.last.col).any(|c| {
        s.cells.get(&(row, c)).and_then(|cell| cell.formula.as_ref()).is_some_and(
            |f| matches!(&*f.expr, Expr::Call(name, _) if name.eq_ignore_ascii_case("SUBTOTAL")),
        )
    })
}

/// What a subtotal pass will do, worked out before anything is inserted.
struct Plan {
    groups: Vec<Group>,
    /// Where each inserted row goes, as the original row it goes before, in the order the rows will be.
    inserts: Vec<(u32, Insert)>,
    first_data: u32,
    last_data: u32,
    /// The list already had a grand total, so the outline already folds the whole list.
    had_grand: bool,
}

impl Session {
    /// Find the groups and the rows to insert.
    fn plan(
        &self,
        sheet: usize,
        spec: &SubtotalSpec,
        range: Range,
        last: u32,
        group_col: u32,
    ) -> Result<Plan, String> {
        let s = &self.book.sheets[sheet];
        let header = range.first.row;
        let data: Vec<u32> =
            (header + 1..=last).filter(|r| !is_subtotal_row(s, *r, range)).collect();
        let (Some(&first_data), Some(&last_data)) = (data.first(), data.last()) else {
            return Err("There are no rows to subtotal.".to_owned());
        };
        let groups = self.find_groups(sheet, &data, group_col);
        let level = |r: u32| outline::line(s, Dim::Rows, r).level;
        // The grand total rows already there are the subtotal rows outside every group.
        let had_grand;
        let grand_at;
        if spec.summary_below {
            let tail: Vec<u32> =
                (last_data + 1..=last).filter(|r| is_subtotal_row(s, *r, range)).collect();
            had_grand = tail.iter().any(|r| level(*r) == 0);
            grand_at = tail.iter().copied().find(|r| level(*r) == 0).unwrap_or(last + 1);
        } else {
            let head: Vec<u32> =
                (header + 1..first_data).filter(|r| is_subtotal_row(s, *r, range)).collect();
            had_grand = head.iter().any(|r| level(*r) == 0);
            grand_at = head.iter().rev().find(|r| level(**r) == 0).map_or(header + 1, |r| r + 1);
        }
        let mut inserts: Vec<(u32, Insert)> = groups
            .iter()
            .enumerate()
            .map(|(i, g)| (if spec.summary_below { g.last + 1 } else { g.first }, Insert::Group(i)))
            .collect();
        inserts.push((grand_at, Insert::Grand));
        // Below the data the group's row comes first and the grand total after it; above, the other way.
        let rank = |what: &Insert| match (spec.summary_below, what) {
            (true, Insert::Group(_)) | (false, Insert::Grand) => 0,
            _ => 1,
        };
        inserts.sort_by_key(|(at, what)| (*at, rank(what), *what));
        Ok(Plan { groups, inserts, first_data, last_data, had_grand })
    }

    /// The runs of data rows with the same value in the group column.
    fn find_groups(&self, sheet: usize, data: &[u32], group_col: u32) -> Vec<Group> {
        let s = &self.book.sheets[sheet];
        let mut groups: Vec<Group> = Vec::new();
        let mut open: Option<(Value, usize)> = None;
        let mut previous: Option<u32> = None;
        for &row in data {
            let value = s.value_at(CellRef::new(row, group_col));
            // A subtotal row between two data rows ends the run, since the data rows list leaves it out.
            let contiguous = previous.is_some_and(|p| p + 1 == row);
            previous = Some(row);
            if value.is_empty() {
                open = None;
                continue;
            }
            match &open {
                Some((v, i)) if contiguous && compare(v, &value) == std::cmp::Ordering::Equal => {
                    groups[*i].last = row;
                }
                _ => {
                    let key = plain_text(self, sheet, CellRef::new(row, group_col));
                    groups.push(Group { first: row, last: row, key });
                    open = Some((value, groups.len() - 1));
                }
            }
        }
        groups
    }

    /// Insert the rows, fill them, and fold the outline.
    fn apply_plan(
        &mut self,
        sheet: usize,
        spec: &SubtotalSpec,
        plan: &Plan,
        label_col: u32,
        totals: &[u32],
    ) -> Result<(), String> {
        for (at, _) in plan.inserts.iter().rev() {
            self.structural_edit(sheet, Axis::Rows, *at, 1, None);
        }
        let mapped =
            |row: u32| row + plan.inserts.iter().filter(|(at, _)| *at <= row).count() as u32;
        let new_row = |k: usize| plan.inserts[k].0 + k as u32;
        let bold = self
            .book
            .styles
            .derive(0, &StyleChange::Font(FontEdit { bold: Some(true), ..Default::default() }));
        let first = mapped(plan.first_data);
        let last = mapped(plan.last_data);
        // The subtotal rows by group, in the order of the groups.
        let mut sub_rows: Vec<(usize, u32)> = Vec::new();
        for (k, (_, what)) in plan.inserts.iter().enumerate() {
            let row = new_row(k);
            let (from, to, label) = match what {
                Insert::Group(i) => {
                    let g = &plan.groups[*i];
                    sub_rows.push((*i, row));
                    (
                        mapped(g.first),
                        mapped(g.last),
                        format!("{} {}", g.key, spec.function.label()),
                    )
                }
                Insert::Grand => (first, last, format!("Grand {}", spec.function.label())),
            };
            let rows = Rows { row, from, to };
            self.write_subtotal_row(sheet, &rows, label_col, &label, totals, spec.function, bold);
        }
        sub_rows.sort_unstable();
        let sub_rows: Vec<u32> = sub_rows.into_iter().map(|(_, r)| r).collect();
        self.fold_outline(sheet, spec, plan, &sub_rows, (first, last), &mapped)?;
        if spec.page_breaks {
            let ids: Vec<u32> = if spec.summary_below {
                sub_rows.iter().map(|r| r + 1).collect()
            } else {
                sub_rows.iter().skip(1).copied().collect()
            };
            add_row_breaks(&mut self.book.sheets[sheet], &ids);
        }
        Ok(())
    }

    /// Write one subtotal row: its label, and a `SUBTOTAL` over the rows `from` to `to` in each column.
    #[allow(clippy::too_many_arguments)]
    fn write_subtotal_row(
        &mut self,
        sheet: usize,
        rows: &Rows,
        label_col: u32,
        label: &str,
        totals: &[u32],
        function: SubtotalFunction,
        bold: u32,
    ) {
        let s = &mut self.book.sheets[sheet];
        s.set(
            CellRef::new(rows.row, label_col),
            Cell { value: Value::text(label), style: bold, ..Default::default() },
        );
        for &col in totals {
            let letter = column_name(col);
            let (from, to) = (rows.from + 1, rows.to + 1);
            let text = format!("SUBTOTAL({},{letter}{from}:{letter}{to})", function.code());
            if let Ok(f) = Formula::parse(&text) {
                s.set(CellRef::new(rows.row, col), Cell { formula: Some(f), ..Default::default() });
            }
        }
    }

    /// Set the outline levels of the inserted rows and the rows around them, and where summaries sit.
    fn fold_outline(
        &mut self,
        sheet: usize,
        spec: &SubtotalSpec,
        plan: &Plan,
        sub_rows: &[u32],
        (first, last): (u32, u32),
        mapped: &dyn Fn(u32) -> u32,
    ) -> Result<(), String> {
        let s = &mut self.book.sheets[sheet];
        s.outline.summary_below = spec.summary_below;
        for (g, row) in plan.groups.iter().zip(sub_rows) {
            let (from, to) = (mapped(g.first), mapped(g.last));
            let before = outline::line(s, Dim::Rows, from).level;
            outline::group(s, Dim::Rows, from, to)?;
            set_level(s, *row, before);
        }
        if !plan.had_grand {
            let start = sub_rows.iter().copied().chain([first]).min().unwrap_or(first);
            let end = sub_rows.iter().copied().chain([last]).max().unwrap_or(last);
            outline::group(s, Dim::Rows, start, end)?;
        }
        Ok(())
    }
}

/// A subtotal row and the rows it totals.
struct Rows {
    row: u32,
    from: u32,
    to: u32,
}

/// Set the outline level of a row.
fn set_level(s: &mut Sheet, row: u32, level: u8) {
    let info = s.rows.entry(row).or_default();
    info.outline = level;
    if *info == crate::model::RowInfo::default() {
        s.rows.remove(&row);
    }
}

/// The text of a cell as it is shown, with no quoting: what a label is made from.
fn plain_text(session: &Session, sheet: usize, at: CellRef) -> String {
    let book = &session.book;
    let Some(cell) = book.sheets[sheet].cell(at) else { return String::new() };
    let code = book.styles.format_code(cell.style);
    crate::numfmt::Format::parse(&code).render(&cell.value, book.date1904, 255).text
}

/// Add manual page breaks after some rows (1 based row numbers), to the breaks the sheet has.
fn add_row_breaks(s: &mut Sheet, ids: &[u32]) {
    let mut all: Vec<u32> = ids.to_vec();
    if let Some((_, xml)) = s.raw.elements.iter().find(|(n, _)| n == "rowBreaks") {
        for part in xml.split("<brk ").skip(1) {
            if let Some(id) = part
                .split("id=\"")
                .nth(1)
                .and_then(|t| t.split('"').next())
                .and_then(|t| t.parse().ok())
            {
                all.push(id);
            }
        }
    }
    all.sort_unstable();
    all.dedup();
    let body: String =
        all.iter().map(|id| format!("<brk id=\"{id}\" max=\"16383\" man=\"1\"/>")).collect();
    let xml =
        format!("<rowBreaks count=\"{0}\" manualBreakCount=\"{0}\">{body}</rowBreaks>", all.len());
    s.raw.elements.retain(|(n, _)| n != "rowBreaks");
    s.raw.elements.push(("rowBreaks".to_owned(), xml));
}
