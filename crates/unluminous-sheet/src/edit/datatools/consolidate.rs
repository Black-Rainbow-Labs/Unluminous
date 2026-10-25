//! Consolidate: one result from the same cells of several ranges, or from the cells with the same labels.
//!
//! With neither label option, the result cell at some row and column combines the cells at that row and
//! column of every source. With the left column as labels, a result row combines the rows of the sources
//! that carry its label, whatever their position; with the top row as labels, a result column combines the
//! columns that carry its label. The labels are put in the result, in the order they first appear. Only
//! numbers are combined.

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::formula::Formula;
use crate::model::*;
use crate::value::{ErrorKind, Value};

use super::displayed;

/// The function a consolidation combines its numbers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidateFunction {
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNumbers,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

impl ConsolidateFunction {
    /// The worksheet function that does the same, for the formulas a consolidation with links writes.
    pub fn worksheet_function(self) -> &'static str {
        match self {
            ConsolidateFunction::Sum => "SUM",
            ConsolidateFunction::Count => "COUNTA",
            ConsolidateFunction::Average => "AVERAGE",
            ConsolidateFunction::Max => "MAX",
            ConsolidateFunction::Min => "MIN",
            ConsolidateFunction::Product => "PRODUCT",
            ConsolidateFunction::CountNumbers => "COUNT",
            ConsolidateFunction::StdDev => "STDEV",
            ConsolidateFunction::StdDevP => "STDEVP",
            ConsolidateFunction::Var => "VAR",
            ConsolidateFunction::VarP => "VARP",
        }
    }

    /// Combine the values found. Count counts every cell that is not empty and the others use the numbers.
    /// `None` when there is nothing to show.
    fn combine(self, found: &[Value]) -> Option<Value> {
        if self == ConsolidateFunction::Count {
            return (!found.is_empty()).then_some(Value::Number(found.len() as f64));
        }
        let numbers: Vec<f64> = found
            .iter()
            .filter_map(|v| if let Value::Number(n) = v { Some(*n) } else { None })
            .collect();
        let numbers = numbers.as_slice();
        let n = numbers.len() as f64;
        if numbers.is_empty() {
            return None;
        }
        let sum: f64 = numbers.iter().sum();
        let mean = sum / n;
        let squares: f64 = numbers.iter().map(|x| (x - mean) * (x - mean)).sum();
        let divide_error = || Value::Error(ErrorKind::Div0);
        Some(match self {
            ConsolidateFunction::Sum => Value::Number(sum),
            ConsolidateFunction::Count | ConsolidateFunction::CountNumbers => Value::Number(n),
            ConsolidateFunction::Average => Value::Number(mean),
            ConsolidateFunction::Max => {
                Value::Number(numbers.iter().copied().fold(f64::MIN, f64::max))
            }
            ConsolidateFunction::Min => {
                Value::Number(numbers.iter().copied().fold(f64::MAX, f64::min))
            }
            ConsolidateFunction::Product => Value::Number(numbers.iter().product()),
            ConsolidateFunction::StdDev if n < 2.0 => divide_error(),
            ConsolidateFunction::Var if n < 2.0 => divide_error(),
            ConsolidateFunction::StdDev => Value::Number((squares / (n - 1.0)).sqrt()),
            ConsolidateFunction::Var => Value::Number(squares / (n - 1.0)),
            ConsolidateFunction::StdDevP => Value::Number((squares / n).sqrt()),
            ConsolidateFunction::VarP => Value::Number(squares / n),
        })
    }
}

/// A request to consolidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Consolidation {
    /// The top left cell of the result.
    pub destination: CellRef,
    /// The ranges to combine, each with the sheet it is on.
    pub sources: Vec<(usize, Range)>,
    pub function: ConsolidateFunction,
    /// The top row of each source holds the labels of its columns.
    pub top_row: bool,
    /// The left column of each source holds the labels of its rows.
    pub left_column: bool,
    /// Write formulas that refer to the source cells, grouped in an outline, rather than values.
    pub create_links: bool,
    /// The name of the workbook, which Excel writes beside every linked row. Used only with links.
    pub book_name: String,
}

/// One row of a consolidation with links.
enum Written {
    /// A row of links to one line of one source.
    Detail { row: u32, cells: Vec<Option<(usize, CellRef)>> },
    /// The row that combines the detail rows above it.
    Summary { row: u32, first: u32, last: u32, label: Option<String> },
}

/// A sheet name as a formula writes it before the `!`.
fn quoted(name: &str) -> String {
    let plain = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// The labels of a result axis, in the order they first appear, or just positions.
#[derive(Debug, Clone)]
struct Axis {
    /// For each result line, its label as the sources write it (`None` when lines are matched by position).
    labels: Vec<Option<String>>,
}

impl Session {
    /// Combine the numbers of several ranges into a result starting at `destination`.
    pub fn consolidate(&mut self, sheet: usize, request: &Consolidation) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if request.sources.is_empty() {
            return Err("Reference is not valid.".to_owned());
        }
        for (s, _) in &request.sources {
            self.check_sheet(*s)?;
        }
        let rows = self.axis(request, true);
        let columns = self.axis(request, false);
        if request.create_links {
            return self.consolidate_with_links(sheet, request, &rows, &columns);
        }
        let top = u32::from(request.top_row);
        let left = u32::from(request.left_column);
        let area = Range::new(
            request.destination,
            CellRef::new(
                request.destination.row + top + rows.labels.len().max(1) as u32 - 1,
                request.destination.col + left + columns.labels.len().max(1) as u32 - 1,
            ),
        );
        self.refuse_array_member(sheet, area)?;
        self.refuse_locked(sheet, area)?;
        self.remember_whole("Consolidate");
        let mut writes: Vec<(CellRef, Value)> = Vec::new();
        for (i, row) in rows.labels.iter().enumerate() {
            if let Some(label) = row.as_ref().filter(|_| request.left_column) {
                let at =
                    CellRef::new(request.destination.row + top + i as u32, request.destination.col);
                writes.push((at, Value::text(label)));
            }
        }
        for (j, column) in columns.labels.iter().enumerate() {
            if let Some(label) = column.as_ref().filter(|_| request.top_row) {
                let at = CellRef::new(
                    request.destination.row,
                    request.destination.col + left + j as u32,
                );
                writes.push((at, Value::text(label)));
            }
        }
        for i in 0..rows.labels.len() {
            for j in 0..columns.labels.len() {
                let numbers = self.numbers_for(request, &rows, &columns, i, j);
                if let Some(value) = request.function.combine(&numbers) {
                    let at = CellRef::new(
                        request.destination.row + top + i as u32,
                        request.destination.col + left + j as u32,
                    );
                    writes.push((at, value));
                }
            }
        }
        let s = &mut self.book.sheets[sheet];
        for (at, value) in writes {
            let style = s.cell(at).map_or(0, |c| c.style);
            s.set(at, Cell { value, style, ..Default::default() });
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// Consolidate with links: for each result row one row per source line that feeds it, hidden at outline
    /// level 2 and holding links to the source cells, then a row of the function over those rows.
    fn consolidate_with_links(
        &mut self,
        sheet: usize,
        request: &Consolidation,
        rows: &Axis,
        columns: &Axis,
    ) -> Result<(), String> {
        let dest = request.destination;
        let left = if request.left_column { 2 } else { 0 };
        let mut layout: Vec<Written> = Vec::new();
        let mut cursor = dest.row + u32::from(request.top_row);
        for (i, label) in rows.labels.iter().enumerate() {
            let details = self.detail_rows(request, rows, columns, i);
            if details.is_empty() {
                continue;
            }
            let start = cursor;
            for cells in details {
                layout.push(Written::Detail { row: cursor, cells });
                cursor += 1;
            }
            layout.push(Written::Summary {
                row: cursor,
                first: start,
                last: cursor - 1,
                label: label.clone(),
            });
            cursor += 1;
        }
        let width = columns.labels.len().max(1) as u32;
        let area = Range::new(
            dest,
            CellRef::new(cursor.max(dest.row + 1) - 1, dest.col + left + width - 1),
        );
        self.refuse_array_member(sheet, area)?;
        self.refuse_locked(sheet, area)?;
        self.remember_whole("Consolidate");
        self.write_link_headers(sheet, request, columns);
        for item in &layout {
            self.write_link_row(sheet, request, item, left, width);
        }
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// The labels along the top, when the source rows carry them.
    fn write_link_headers(&mut self, sheet: usize, request: &Consolidation, columns: &Axis) {
        if !request.top_row {
            return;
        }
        let left = if request.left_column { 2 } else { 0 };
        for (j, label) in columns.labels.iter().enumerate() {
            if let Some(label) = label {
                let at = CellRef::new(
                    request.destination.row,
                    request.destination.col + left + j as u32,
                );
                self.set_plain(sheet, at, Cell { value: Value::text(label), ..Default::default() });
            }
        }
    }

    /// Write one row of a consolidation with links.
    fn write_link_row(
        &mut self,
        sheet: usize,
        request: &Consolidation,
        item: &Written,
        left: u32,
        width: u32,
    ) {
        let col0 = request.destination.col;
        match item {
            Written::Detail { row, cells } => {
                if request.left_column {
                    let name =
                        Cell { value: Value::text(&request.book_name), ..Default::default() };
                    self.set_plain(sheet, CellRef::new(*row, col0 + 1), name);
                }
                for (j, source) in cells.iter().enumerate() {
                    let Some((s, at)) = source else { continue };
                    let text = format!(
                        "={}!${}${}",
                        quoted(&self.book.sheets[*s].name),
                        crate::coord::column_name(at.col),
                        at.row + 1
                    );
                    if let Ok(f) = Formula::parse(&text) {
                        self.set_plain(
                            sheet,
                            CellRef::new(*row, col0 + left + j as u32),
                            Cell { formula: Some(f), ..Default::default() },
                        );
                    }
                }
                let info = self.book.sheets[sheet].rows.entry(*row).or_default();
                info.outline = 1;
                info.hidden = true;
            }
            Written::Summary { row, first, last, label } => {
                if let (Some(label), true) = (label, request.left_column) {
                    self.set_plain(
                        sheet,
                        CellRef::new(*row, col0),
                        Cell { value: Value::text(label), ..Default::default() },
                    );
                }
                for j in 0..width {
                    let col = col0 + left + j;
                    if !(*first..=*last).any(|r| {
                        self.book.sheets[sheet]
                            .cell(CellRef::new(r, col))
                            .is_some_and(|c| c.formula.is_some())
                    }) {
                        continue;
                    }
                    let letter = crate::coord::column_name(col);
                    let range = if first == last {
                        format!("{letter}{}", first + 1)
                    } else {
                        format!("{letter}{}:{letter}{}", first + 1, last + 1)
                    };
                    let text = format!("={}({range})", request.function.worksheet_function());
                    if let Ok(f) = Formula::parse(&text) {
                        self.set_plain(
                            sheet,
                            CellRef::new(*row, col),
                            Cell { formula: Some(f), ..Default::default() },
                        );
                    }
                }
                self.book.sheets[sheet].rows.entry(*row).or_default().collapsed = true;
            }
        }
    }

    /// Put a cell, keeping the style already there.
    fn set_plain(&mut self, sheet: usize, at: CellRef, mut cell: Cell) {
        cell.style = self.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
        self.book.sheets[sheet].set(at, cell);
    }

    /// The rows that feed result row `i`: for each, the source cell for every result column, when it holds a number.
    fn detail_rows(
        &self,
        request: &Consolidation,
        rows: &Axis,
        columns: &Axis,
        i: usize,
    ) -> Vec<Vec<Option<(usize, CellRef)>>> {
        let mut out = Vec::new();
        for (sheet, range) in &request.sources {
            for r in self.lines_for(*sheet, *range, request, rows, i, true) {
                let cells: Vec<Option<(usize, CellRef)>> = (0..columns.labels.len())
                    .map(|j| {
                        let c =
                            *self.lines_for(*sheet, *range, request, columns, j, false).first()?;
                        let at = CellRef::new(r, c);
                        matches!(self.book.sheets[*sheet].value_at(at), Value::Number(_))
                            .then_some((*sheet, at))
                    })
                    .collect();
                out.push(cells);
            }
        }
        out
    }

    /// The lines of the result along one axis: the labels in order of first appearance, or positions.
    fn axis(&self, request: &Consolidation, rows: bool) -> Axis {
        let labelled = if rows { request.left_column } else { request.top_row };
        let skip_other = if rows { request.top_row } else { request.left_column };
        if !labelled {
            let most = request
                .sources
                .iter()
                .map(|(_, r)| {
                    (if rows { r.rows() } else { r.cols() }).saturating_sub(u32::from(skip_other))
                })
                .max()
                .unwrap_or(0);
            return Axis { labels: vec![None; most as usize] };
        }
        let mut labels: Vec<Option<String>> = Vec::new();
        for (s, range) in &request.sources {
            for line in self.label_lines(*s, *range, request, rows) {
                let text = line.1;
                if text.is_empty() {
                    continue;
                }
                if !labels.iter().flatten().any(|l| l.to_lowercase() == text.to_lowercase()) {
                    labels.push(Some(text));
                }
            }
        }
        Axis { labels }
    }

    /// The labelled lines of one source along an axis: the index of the line within the data, and its label.
    fn label_lines(
        &self,
        sheet: usize,
        range: Range,
        request: &Consolidation,
        rows: bool,
    ) -> Vec<(u32, String)> {
        let s = &self.book.sheets[sheet];
        let (first_data, count) = if rows {
            (u32::from(request.top_row), range.rows())
        } else {
            (u32::from(request.left_column), range.cols())
        };
        (first_data..count)
            .map(|line| {
                let at = if rows {
                    CellRef::new(range.first.row + line, range.first.col)
                } else {
                    CellRef::new(range.first.row, range.first.col + line)
                };
                let cell = s.cell(at).cloned().unwrap_or_default();
                (line - first_data, displayed(&self.book, &cell))
            })
            .collect()
    }

    /// The numbers the sources have for one result cell.
    fn numbers_for(
        &self,
        request: &Consolidation,
        rows: &Axis,
        columns: &Axis,
        i: usize,
        j: usize,
    ) -> Vec<Value> {
        let mut numbers = Vec::new();
        for (sheet, range) in &request.sources {
            let row_lines = self.lines_for(*sheet, *range, request, rows, i, true);
            let column_lines = self.lines_for(*sheet, *range, request, columns, j, false);
            let s = &self.book.sheets[*sheet];
            for r in &row_lines {
                for c in &column_lines {
                    let value = s.value_at(CellRef::new(*r, *c));
                    if !matches!(value, Value::Empty) {
                        numbers.push(value);
                    }
                }
            }
        }
        numbers
    }

    /// The rows (or columns) of one source that belong to result line `index`.
    fn lines_for(
        &self,
        sheet: usize,
        range: Range,
        request: &Consolidation,
        axis: &Axis,
        index: usize,
        rows: bool,
    ) -> Vec<u32> {
        let skip = u32::from(if rows { request.top_row } else { request.left_column });
        let origin = if rows { range.first.row } else { range.first.col };
        let count = if rows { range.rows() } else { range.cols() };
        match axis.labels.get(index).and_then(|l| l.as_ref()) {
            None => {
                let line = index as u32;
                if line + skip < count {
                    vec![origin + skip + line]
                } else {
                    Vec::new()
                }
            }
            Some(label) => self
                .label_lines(sheet, range, request, rows)
                .into_iter()
                .filter(|(_, text)| text.to_lowercase() == label.to_lowercase())
                .map(|(line, _)| origin + skip + line)
                .collect(),
        }
    }
}
