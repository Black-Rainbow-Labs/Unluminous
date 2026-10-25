//! What If analysis as editing operations: Goal Seek, data tables and the Scenario Manager with its
//! summary report. The arithmetic and the file forms are in [`crate::whatif`].

use crate::calc::Engine;
use crate::coord::{CellRef, Range};
use crate::model::{Cell, DataTable, RowInfo, Sheet, Workbook};
use crate::styles::{Alignment, Border, Color, Edge, Fill, Font, Xf};
use crate::value::Value;
use crate::whatif::{seek, InputCell, Scenario, Scenarios, DEFAULT_MAX_CHANGE};

use super::{Guard, Session};

/// What Excel says when Goal Seek is given a cell it cannot use.
const BAD_REFERENCE: &str = "Reference isn't valid.";
/// What Excel says when a data table is given an input cell it cannot use.
const BAD_INPUT: &str = "Input cell reference is not valid.";
/// What Excel says about a change to a part of a data table.
pub const DATA_TABLE_PART: &str = "Cannot change part of a data table.";
/// The most cells a scenario can change.
pub const MOST_CHANGING_CELLS: usize = 32;

/// Where Goal Seek ended and what it left in the changing cell.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalSeekResult {
    /// The formula is within MaxChange of the goal: Excel says "Goal Seeking found a solution".
    pub found: bool,
    /// The value left in the changing cell.
    pub value: f64,
    /// The formula's value there.
    pub target: f64,
    /// How many points were tried.
    pub iterations: usize,
}

impl Session {
    // ------------------------------------------------------------------------------------------------
    // Goal Seek.

    /// Goal Seek: change the value of cell `by` until the formula in cell `set` comes to `to`. The cell is
    /// left at the last value tried, whether or not a solution was found, as Excel leaves it; undo puts the
    /// old value back. Both cells must be on the sheet, `set` must hold a formula that comes to a number and
    /// `by` a number or nothing.
    pub fn goal_seek(
        &mut self,
        sheet: usize,
        set: CellRef,
        to: f64,
        by: CellRef,
    ) -> Result<GoalSeekResult, String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Write(&[Range::cell(by)]))?;
        let s = &self.book.sheets[sheet];
        let target_ok = set != by
            && s.cell(set).is_some_and(|c| c.formula.is_some() && c.value.as_number().is_some());
        let start = match s.cell(by) {
            None => Some(0.0),
            Some(c) if c.formula.is_some() => None,
            Some(c) if c.value.is_empty() => Some(0.0),
            Some(c) => c.value.as_number().filter(|_| matches!(c.value, Value::Number(_))),
        };
        let (true, Some(x0)) = (target_ok, start) else { return Err(BAD_REFERENCE.to_owned()) };
        let tolerance = self.max_change();
        self.remember_cells("Goal seek", sheet, &[Range::cell(by)], false);
        let outcome = seek(x0, to, tolerance, |x| self.evaluate_with(sheet, set, by, x));
        let Some(outcome) = outcome else {
            self.evaluate_with(sheet, set, by, x0);
            return Err(BAD_REFERENCE.to_owned());
        };
        let target = self.evaluate_with(sheet, set, by, outcome.x).unwrap_or(outcome.value);
        Ok(GoalSeekResult {
            found: outcome.found,
            value: outcome.x,
            target,
            iterations: outcome.iterations,
        })
    }

    /// Application.MaxChange: the workbook's iteration setting, or Excel's default.
    fn max_change(&self) -> f64 {
        self.book
            .calc
            .attrs
            .iter()
            .find(|(k, _)| k == "iterateDelta")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_CHANGE)
    }

    /// Put a number in a cell, compute what depends on it and read a formula's number. Computes whether
    /// or not the workbook calculates by itself, because Goal Seek has to.
    fn evaluate_with(&mut self, sheet: usize, read: CellRef, put: CellRef, x: f64) -> Option<f64> {
        let cell = self.book.sheets[sheet].cells.entry((put.row, put.col)).or_default();
        cell.value = Value::Number(x);
        cell.formula = None;
        self.engine.changed(&self.book, sheet, put);
        let seed = self.seed();
        self.engine.recalculate(&mut self.book, self.now, seed);
        match self.book.sheets[sheet].value_at(read) {
            Value::Number(n) if n.is_finite() => Some(n),
            _ => None,
        }
    }

    // ------------------------------------------------------------------------------------------------
    // Data tables.

    /// Data, What If Analysis, Data Table (`Range.Table`): `range` is the block that holds the input
    /// values and the formulas, and its first row and first column are those headers. With both input
    /// cells the table has two variables, the first row's values going into `row_input` and the first
    /// column's into `col_input`, and the formula in the top left corner. With one input cell the values
    /// run along the first row (`row_input`) or down the first column (`col_input`) and the formulas sit
    /// down the first column or along the first row. The rest of the block is filled with results.
    pub fn create_data_table(
        &mut self,
        sheet: usize,
        range: Range,
        row_input: Option<CellRef>,
        col_input: Option<CellRef>,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        if range.rows() < 2 || range.cols() < 2 {
            return Err("The selection is too small for a data table.".to_owned());
        }
        if row_input.is_none() && col_input.is_none() {
            return Err(BAD_INPUT.to_owned());
        }
        let results =
            Range::new(CellRef::new(range.first.row + 1, range.first.col + 1), range.last);
        if [row_input, col_input].into_iter().flatten().any(|c| range.contains(c)) {
            return Err(BAD_INPUT.to_owned());
        }
        self.refuse_array_member(sheet, range)?;
        if self.book.sheets[sheet].data_tables.iter().any(|t| t.range.intersects(&results)) {
            return Err(DATA_TABLE_PART.to_owned());
        }
        let table = match (row_input, col_input) {
            (Some(r), Some(c)) => DataTable {
                range: results,
                two_d: true,
                row_input: true,
                r1: Some(r),
                r2: Some(c),
                attrs: Vec::new(),
            },
            (Some(r), None) => DataTable {
                range: results,
                two_d: false,
                row_input: true,
                r1: Some(r),
                r2: None,
                attrs: Vec::new(),
            },
            (None, c) => DataTable {
                range: results,
                two_d: false,
                row_input: false,
                r1: c,
                r2: None,
                attrs: Vec::new(),
            },
        };
        self.remember_whole("Data table");
        let s = &mut self.book.sheets[sheet];
        for at in results.cells() {
            let cell = s.cells.entry((at.row, at.col)).or_default();
            cell.value = Value::Number(0.0);
            cell.formula = None;
        }
        s.data_tables.push(table);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(())
    }

    /// The data table a cell belongs to.
    pub fn data_table_at(&self, sheet: usize, at: CellRef) -> Option<&DataTable> {
        self.book.sheets.get(sheet)?.data_tables.iter().find(|t| t.range.contains(at))
    }

    /// Take a data table away, leaving its results as plain values.
    pub fn remove_data_table(&mut self, sheet: usize, at: CellRef) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        let Some(index) =
            self.book.sheets[sheet].data_tables.iter().position(|t| t.range.contains(at))
        else {
            return Err("There is no data table at that cell.".to_owned());
        };
        self.remember_whole("Remove data table");
        self.book.sheets[sheet].data_tables.remove(index);
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------
    // Scenarios.

    /// The scenarios of a sheet.
    pub fn scenarios(&self, sheet: usize) -> Scenarios {
        self.book.sheets.get(sheet).map(|s| s.scenarios()).unwrap_or_default()
    }

    /// The text Excel writes in a comment: `Created by Jason on 10/24/2026`.
    fn stamp(&self, verb: &str, user: &str) -> String {
        let (y, m, d) = crate::dates::date(self.now, self.book.date1904).unwrap_or((1900, 1, 1));
        format!("{verb} by {user} on {m}/{d}/{y}")
    }

    /// Cells named by some ranges, in row order, without repeats.
    fn changing_cells(ranges: &[Range]) -> Result<Vec<CellRef>, String> {
        let mut cells: Vec<CellRef> = ranges.iter().flat_map(|r| r.cells()).collect();
        cells.sort();
        cells.dedup();
        if cells.is_empty() {
            return Err("Enter the cells the scenario changes.".to_owned());
        }
        if cells.len() > MOST_CHANGING_CELLS {
            return Err(format!("A scenario can change at most {MOST_CHANGING_CELLS} cells."));
        }
        Ok(cells)
    }

    /// What a value is called in a scenario file: a number as digits, a flag as TRUE or FALSE, text as it is.
    fn scenario_text(value: &Value) -> String {
        match value {
            Value::Number(n) => n.to_string(),
            Value::Empty => String::new(),
            other => other.to_text(),
        }
    }

    /// Scenario Manager, Add. The scenario sets `changing` to `values`, or to what the cells hold now when
    /// no values are given. The comment is `Created by <user> on <date>` unless one is given. Excel keeps the
    /// scenario's user name and, when asked, locks it against changes and hides it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_scenario(
        &mut self,
        sheet: usize,
        name: &str,
        changing: &[Range],
        values: Option<&[Value]>,
        comment: Option<&str>,
        user: &str,
        locked: bool,
        hidden: bool,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 255 {
            return Err("Enter a name for the scenario.".to_owned());
        }
        let mut scenarios = self.scenarios(sheet);
        if scenarios.list.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
            return Err(format!("A scenario named {name} already exists."));
        }
        let cells = Self::changing_cells(changing)?;
        let texts = self.scenario_values(sheet, &cells, values)?;
        self.remember_whole("Scenario");
        scenarios.list.push(Scenario {
            name: name.to_owned(),
            locked,
            hidden,
            user: Some(user.to_owned()),
            comment: Some(comment.map_or_else(|| self.stamp("Created", user), str::to_owned)),
            cells: cells
                .iter()
                .zip(texts)
                .map(|(at, value)| InputCell {
                    at: *at,
                    value,
                    deleted: false,
                    undone: false,
                    num_fmt_id: None,
                })
                .collect(),
        });
        self.book.sheets[sheet].set_scenarios(&scenarios);
        Ok(())
    }

    /// The text a scenario stores for each changing cell: the values given, or the cells' own.
    fn scenario_values(
        &self,
        sheet: usize,
        cells: &[CellRef],
        values: Option<&[Value]>,
    ) -> Result<Vec<String>, String> {
        match values {
            Some(values) if values.len() != cells.len() => Err(format!(
                "The scenario changes {} cells and was given {} values.",
                cells.len(),
                values.len()
            )),
            Some(values) => Ok(values.iter().map(Self::scenario_text).collect()),
            None => Ok(cells
                .iter()
                .map(|c| Self::scenario_text(&self.book.sheets[sheet].value_at(*c)))
                .collect()),
        }
    }

    /// Scenario Manager, Edit (`Scenario.ChangeScenario`). A new name, changing cells or values, or a
    /// comment replaces what the scenario had. Without a comment of its own, Excel appends a line to
    /// the old one: `Modified by <user> on <date>`.
    #[allow(clippy::too_many_arguments)]
    pub fn edit_scenario(
        &mut self,
        sheet: usize,
        name: &str,
        new_name: Option<&str>,
        changing: Option<&[Range]>,
        values: Option<&[Value]>,
        comment: Option<&str>,
        user: &str,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        let mut scenarios = self.scenarios(sheet);
        let Some(index) = scenarios.list.iter().position(|s| s.name.eq_ignore_ascii_case(name))
        else {
            return Err(format!("There is no scenario named {name}."));
        };
        if scenarios.list[index].locked {
            self.allowed(sheet, Guard::Scenarios)?;
        }
        let cells = match changing {
            Some(r) => Self::changing_cells(r)?,
            None => scenarios.list[index].cells.iter().map(|c| c.at).collect(),
        };
        let texts = match (values, changing) {
            (None, None) => scenarios.list[index].cells.iter().map(|c| c.value.clone()).collect(),
            _ => self.scenario_values(sheet, &cells, values)?,
        };
        if let Some(new) = new_name {
            let new = new.trim();
            if new.is_empty()
                || scenarios
                    .list
                    .iter()
                    .enumerate()
                    .any(|(i, s)| i != index && s.name.eq_ignore_ascii_case(new))
            {
                return Err(format!("A scenario named {new} already exists."));
            }
        }
        let stamp = self.stamp("Modified", user);
        self.remember_whole("Edit scenario");
        scenarios.current = index as u32;
        let s = &mut scenarios.list[index];
        if let Some(new) = new_name {
            s.name = new.trim().to_owned();
        }
        s.cells = cells
            .iter()
            .zip(texts)
            .map(|(at, value)| InputCell {
                at: *at,
                value,
                deleted: false,
                undone: false,
                num_fmt_id: None,
            })
            .collect();
        s.comment = Some(match comment {
            Some(c) => c.to_owned(),
            None => match s.comment.take() {
                Some(old) if !old.is_empty() => format!("{old}\n{stamp}"),
                _ => stamp,
            },
        });
        self.book.sheets[sheet].set_scenarios(&scenarios);
        Ok(())
    }

    /// Scenario Manager, Delete.
    pub fn delete_scenario(&mut self, sheet: usize, name: &str) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        let mut scenarios = self.scenarios(sheet);
        let Some(index) = scenarios.list.iter().position(|s| s.name.eq_ignore_ascii_case(name))
        else {
            return Err(format!("There is no scenario named {name}."));
        };
        if scenarios.list[index].locked {
            self.allowed(sheet, Guard::Scenarios)?;
        }
        self.remember_whole("Delete scenario");
        scenarios.list.remove(index);
        scenarios.current = scenarios.current.min(scenarios.list.len().saturating_sub(1) as u32);
        scenarios.show = scenarios.show.filter(|s| (*s as usize) < scenarios.list.len());
        self.book.sheets[sheet].set_scenarios(&scenarios);
        Ok(())
    }

    /// Scenario Manager, Show: put the scenario's values in the cells it changes.
    pub fn show_scenario(&mut self, sheet: usize, name: &str) -> Result<(), String> {
        self.check_sheet(sheet)?;
        let mut scenarios = self.scenarios(sheet);
        let Some(index) = scenarios.list.iter().position(|s| s.name.eq_ignore_ascii_case(name))
        else {
            return Err(format!("There is no scenario named {name}."));
        };
        let cells: Vec<Range> =
            scenarios.list[index].cells.iter().map(|c| Range::cell(c.at)).collect();
        self.allowed(sheet, Guard::Scenarios)?;
        self.allowed(sheet, Guard::Write(&cells))?;
        self.remember_whole("Show scenario");
        for input in scenarios.list[index].cells.clone() {
            let value = crate::input::read(&input.value, self.book.date1904, self.this_year).value;
            let cell =
                self.book.sheets[sheet].cells.entry((input.at.row, input.at.col)).or_default();
            cell.value = value;
            cell.formula = None;
            self.engine.changed(&self.book, sheet, input.at);
        }
        scenarios.current = index as u32;
        scenarios.show = Some(index as u32);
        self.book.sheets[sheet].set_scenarios(&scenarios);
        self.recalculate();
        Ok(())
    }

    /// Scenario Manager, Summary (`Scenarios.CreateSummary` with the standard summary): a new sheet before
    /// this one listing each changing cell and each result cell under every scenario. Answers the index of
    /// the new sheet, which becomes the sheet showing.
    pub fn scenario_summary(&mut self, sheet: usize, results: &[Range]) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        self.allow_structure_change()?;
        let mut scenarios = self.scenarios(sheet);
        if scenarios.list.is_empty() {
            return Err("There are no scenarios on this sheet to summarize.".to_owned());
        }
        let result_cells = Self::result_cells(results)?;
        let report = SummaryData::collect(self, sheet, &scenarios, &result_cells);
        self.remember_whole("Scenario summary");
        let name = self.next_summary_name();
        let built = report.build(&mut self.book, &name);
        self.book.sheets.insert(sheet, built);
        self.reindex_after_sheet_move(|i| if i >= sheet { i + 1 } else { i });
        self.book.active_sheet = sheet;
        scenarios.results = results.to_vec();
        self.book.sheets[sheet + 1].set_scenarios(&scenarios);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(sheet)
    }

    /// `Scenario Summary`, or the first `Scenario Summary 2`, `Scenario Summary 3` that is free.
    fn next_summary_name(&self) -> String {
        (1..)
            .map(|n| match n {
                1 => "Scenario Summary".to_owned(),
                n => format!("Scenario Summary {n}"),
            })
            .find(|n| self.book.sheet_index(n).is_none())
            .unwrap_or_default()
    }

    fn result_cells(results: &[Range]) -> Result<Vec<CellRef>, String> {
        let cells: Vec<CellRef> = results.iter().flat_map(|r| r.cells()).collect();
        if cells.len() > 251 {
            return Err("A summary report can show at most 251 result cells.".to_owned());
        }
        Ok(cells)
    }
}

/// What a summary report shows, worked out before the sheet is built.
struct SummaryData {
    /// How each changing cell and then each result cell is named in the report.
    labels: Vec<String>,
    names: Vec<String>,
    comments: Vec<String>,
    /// For each changing cell, its current value and the value in each scenario (`None` where the scenario
    /// does not change the cell).
    changing_values: Vec<(Value, Vec<Option<Value>>)>,
    /// For each result cell, its current value and its value under each scenario.
    result_values: Vec<(Value, Vec<Value>)>,
    /// The number format code of each changing and result cell, changing cells first.
    formats: Vec<String>,
}

impl SummaryData {
    fn collect(
        session: &Session,
        sheet: usize,
        scenarios: &Scenarios,
        result_cells: &[CellRef],
    ) -> Self {
        let book = &session.book;
        let mut changing: Vec<CellRef> =
            scenarios.list.iter().flat_map(|s| s.cells.iter().map(|c| c.at)).collect();
        changing.sort();
        changing.dedup();
        let parsed = |text: &str| crate::input::read(text, book.date1904, session.this_year).value;
        let changing_values = changing
            .iter()
            .map(|at| {
                let per: Vec<Option<Value>> = scenarios
                    .list
                    .iter()
                    .map(|s| s.cells.iter().find(|c| c.at == *at).map(|c| parsed(&c.value)))
                    .collect();
                (book.sheets[sheet].value_at(*at), per)
            })
            .collect();
        let under = Self::results_under_each(session, sheet, scenarios, &changing, result_cells);
        let all = changing.iter().chain(result_cells.iter());
        let formats = all
            .clone()
            .map(|at| {
                let style = book.sheets[sheet].cell(*at).map_or(0, |c| c.style);
                book.styles.format_code(style)
            })
            .collect();
        let labels = all.map(|at| Self::label(book, sheet, *at)).collect();
        Self {
            labels,
            names: scenarios.list.iter().map(|s| s.name.clone()).collect(),
            comments: scenarios
                .list
                .iter()
                .map(|s| s.comment.clone().unwrap_or_default())
                .collect(),
            changing_values,
            result_values: result_cells
                .iter()
                .enumerate()
                .map(|(i, at)| (book.sheets[sheet].value_at(*at), under[i].clone()))
                .collect(),
            formats,
        }
    }

    /// The value of each result cell with each scenario applied, in a copy of the workbook.
    fn results_under_each(
        session: &Session,
        sheet: usize,
        scenarios: &Scenarios,
        changing: &[CellRef],
        results: &[CellRef],
    ) -> Vec<Vec<Value>> {
        let mut scratch = session.book.clone();
        let mut engine = Engine::new(&scratch);
        let originals: Vec<Option<Cell>> = changing
            .iter()
            .map(|at| scratch.sheets[sheet].cells.get(&(at.row, at.col)).cloned())
            .collect();
        let mut out = vec![Vec::new(); results.len()];
        for scenario in &scenarios.list {
            for (at, original) in changing.iter().zip(&originals) {
                match original {
                    Some(c) => scratch.sheets[sheet].cells.insert((at.row, at.col), c.clone()),
                    None => scratch.sheets[sheet].cells.remove(&(at.row, at.col)),
                };
                engine.changed(&scratch, sheet, *at);
            }
            for input in &scenario.cells {
                let value =
                    crate::input::read(&input.value, scratch.date1904, session.this_year).value;
                let cell =
                    scratch.sheets[sheet].cells.entry((input.at.row, input.at.col)).or_default();
                cell.value = value;
                cell.formula = None;
                engine.changed(&scratch, sheet, input.at);
            }
            engine.recalculate(&mut scratch, session.now, 1);
            for (i, at) in results.iter().enumerate() {
                out[i].push(scratch.sheets[sheet].value_at(*at));
            }
        }
        out
    }

    /// How a cell is named in the report: the defined name that is exactly that cell, else `$B$1`.
    fn label(book: &Workbook, sheet: usize, at: CellRef) -> String {
        let own = &book.sheets[sheet].name;
        let absolute = format!("${}${}", crate::coord::column_name(at.col), at.row + 1);
        let wanted = format!("{own}!{absolute}").to_lowercase();
        let named = |scope: Option<usize>| {
            book.names
                .iter()
                .find(|n| n.scope == scope && n.formula.replace('\'', "").to_lowercase() == wanted)
        };
        named(Some(sheet)).or_else(|| named(None)).map_or(absolute, |n| n.name.clone())
    }

    /// How many lines text takes in a column Excel's summary report makes 12.125 characters wide at 8 points.
    fn lines_of(text: &str) -> usize {
        const WIDTH: usize = 18;
        text.split('\n')
            .map(|paragraph| {
                let mut lines = 1;
                let mut used = 0usize;
                for word in paragraph.split_whitespace() {
                    let len = word.chars().count();
                    if used == 0 {
                        used = len;
                    } else if used + 1 + len <= WIDTH {
                        used += 1 + len;
                    } else {
                        lines += 1;
                        used = len;
                    }
                }
                lines
            })
            .sum()
    }

    /// Build the report sheet, adding the formats it uses to the workbook's styles.
    fn build(&self, book: &mut Workbook, name: &str) -> Sheet {
        let mut report = Report::new(self, book, name);
        report.title_and_headings();
        report.comments_row();
        report.section("Changing Cells:");
        report.changing_rows(book);
        report.section("Result Cells:");
        report.result_rows(book);
        report.notes();
        report.columns();
        report.sheet
    }
}

/// The report sheet being built, row by row. Rows and columns are 0 based, the report starts in column B.
struct Report<'a> {
    data: &'a SummaryData,
    sheet: Sheet,
    looks: Looks,
    /// The last column with a scenario in it.
    last_col: u32,
    /// The next row to write.
    row: u32,
    /// Which of the data's formats and labels the next changing or result cell uses.
    item: usize,
}

impl<'a> Report<'a> {
    fn new(data: &'a SummaryData, book: &mut Workbook, name: &str) -> Self {
        let mut sheet = Sheet::new(name);
        sheet.show_grid = false;
        sheet.outline.summary_below = false;
        Self {
            data,
            sheet,
            looks: Looks::new(book),
            last_col: 3 + data.names.len() as u32,
            row: 0,
            item: 0,
        }
    }

    fn put(&mut self, row: u32, col: u32, value: Value, style: u32) {
        self.sheet.set(CellRef::new(row, col), Cell { value, style, ..Default::default() });
    }

    fn row_info(&mut self, row: u32, info: RowInfo) {
        self.sheet.rows.insert(row, info);
    }

    /// Rows 1 to 3: the title bar and the headings with the scenarios' names.
    fn title_and_headings(&mut self) {
        let thick = vec![("thickBot".to_owned(), "1".to_owned())];
        self.row_info(0, RowInfo { height: Some(15.0), attrs: thick, ..Default::default() });
        self.row_info(1, RowInfo { height: Some(15.75), ..Default::default() });
        let l = &self.looks;
        let (title_left, title_right, head_left, head_right) =
            (l.title_left, l.title_right, l.head_left, l.head_right);
        self.put(1, 1, Value::text("Scenario Summary"), title_left);
        self.put(1, 2, Value::Empty, title_left);
        for col in 3..=self.last_col {
            self.put(1, col, Value::Empty, title_right);
        }
        let collapsed = RowInfo { height: Some(15.75), collapsed: true, ..Default::default() };
        self.row_info(2, collapsed);
        self.put(2, 1, Value::Empty, head_left);
        self.put(2, 2, Value::Empty, head_left);
        self.put(2, 3, Value::text("Current Values:"), head_right);
        for (i, n) in self.data.names.iter().enumerate() {
            self.put(2, 4 + i as u32, Value::text(n), head_right);
        }
    }

    /// Row 4: the scenarios' comments, in a row that starts hidden and as tall as the longest comment.
    fn comments_row(&mut self) {
        let lines = self.data.comments.iter().map(|c| SummaryData::lines_of(c)).max().unwrap_or(1);
        let height = (lines as f64 * 11.25).max(15.0);
        self.row_info(
            3,
            RowInfo { height: Some(height), hidden: true, outline: 1, ..Default::default() },
        );
        let (item_left, plain, comment) =
            (self.looks.item_left, self.looks.plain, self.looks.comment);
        self.put(3, 1, Value::Empty, item_left);
        self.put(3, 2, Value::Empty, item_left);
        self.put(3, 3, Value::Empty, plain);
        for (i, c) in self.data.comments.iter().enumerate() {
            self.put(3, 4 + i as u32, Value::text(c), comment);
        }
        self.row = 4;
    }

    /// A section heading row ("Changing Cells:") across the report.
    fn section(&mut self, title: &str) {
        let row = self.row;
        self.row_info(row, RowInfo { height: Some(15.0), ..Default::default() });
        let (left, right) = (self.looks.section_left, self.looks.section_right);
        self.put(row, 1, Value::text(title), left);
        self.put(row, 2, Value::Empty, left);
        for col in 3..=self.last_col {
            self.put(row, col, Value::Empty, right);
        }
        self.row += 1;
    }

    /// One line per changing cell: its label, its current value and its value in each scenario, gray where
    /// the scenario changes the cell.
    fn changing_rows(&mut self, book: &mut Workbook) {
        for (current, per) in &self.data.changing_values {
            let row = self.row;
            let code = &self.data.formats[self.item];
            let plain = self.looks.value(book, code, false, false);
            let gray = self.looks.value(book, code, true, false);
            self.row_info(row, RowInfo { height: Some(15.0), outline: 1, ..Default::default() });
            let left = self.looks.item_left;
            self.put(row, 1, Value::Empty, left);
            self.put(row, 2, Value::text(&self.data.labels[self.item]), left);
            self.put(row, 3, current.clone(), plain);
            for (j, value) in per.iter().enumerate() {
                let (shown, style) = match value {
                    Some(v) => (v.clone(), gray),
                    None => (current.clone(), plain),
                };
                self.put(row, 4 + j as u32, shown, style);
            }
            self.row += 1;
            self.item += 1;
        }
    }

    /// One line per result cell: its label, its current value and its value under each scenario. The last
    /// has the medium line under it that ends the report.
    fn result_rows(&mut self, book: &mut Workbook) {
        let count = self.data.result_values.len();
        for (i, (current, per)) in self.data.result_values.iter().enumerate() {
            let last = i + 1 == count;
            let row = self.row;
            let style = self.looks.value(book, &self.data.formats[self.item], false, last);
            let left = if last { self.looks.item_last_left } else { self.looks.item_left };
            let attrs =
                if last { vec![("thickBot".to_owned(), "1".to_owned())] } else { Vec::new() };
            let height = Some(if last { 15.75 } else { 15.0 });
            self.row_info(row, RowInfo { height, outline: 1, attrs, ..Default::default() });
            self.put(row, 1, Value::Empty, left);
            self.put(row, 2, Value::text(&self.data.labels[self.item]), left);
            self.put(row, 3, current.clone(), style);
            for (j, v) in per.iter().enumerate() {
                self.put(row, 4 + j as u32, v.clone(), style);
            }
            self.row += 1;
            self.item += 1;
        }
    }

    /// The three lines of notes under the report.
    fn notes(&mut self) {
        for note in [
            "Notes:  Current Values column represents values of changing cells at",
            "time Scenario Summary Report was created.  Changing cells for each",
            "scenario are highlighted in gray.",
        ] {
            let row = self.row;
            self.put(row, 1, Value::text(note), 0);
            self.row += 1;
        }
    }

    /// The label column fits its labels when they are all short addresses, and the value columns are
    /// 12.125 wide and can be collapsed with the outline.
    fn columns(&mut self) {
        if self.data.labels.iter().all(|l| l.starts_with('$') && l.len() <= 4) {
            self.sheet.edit_column(2, |c| {
                c.width = Some(5.25);
                c.best_fit = true;
                c.custom_width = true;
            });
        }
        for col in 3..=self.last_col {
            self.sheet.edit_column(col, |c| {
                c.width = Some(12.125);
                c.best_fit = true;
                c.custom_width = true;
                c.outline = 1;
            });
        }
    }
}

/// The formats a summary report uses, made in the workbook's styles on first use. Each is an index into
/// the workbook's cell formats; the fonts, fills and borders are the ones Excel's report uses.
struct Looks {
    title_left: u32,
    title_right: u32,
    head_left: u32,
    head_right: u32,
    item_left: u32,
    item_last_left: u32,
    section_left: u32,
    section_right: u32,
    comment: u32,
    plain: u32,
    /// The fill of a changing cell in the scenario that changes it.
    gray: u32,
    /// The border under the last row of the report.
    last_edge: u32,
}

/// Add an item to a list unless an equal one is there. Answers its index.
fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    crate::styles::Styles::intern(list, item)
}

impl Looks {
    fn new(book: &mut Workbook) -> Self {
        let styles = &mut book.styles;
        let fonts = Self::fonts(styles);
        let fills = [(20, 24), (22, 24), (22, 7)].map(|(fg, bg)| {
            let fill = Fill {
                pattern: "solid".to_owned(),
                fg: Color::Indexed(fg),
                bg: Color::Indexed(bg),
                raw: None,
            };
            intern(&mut styles.fills, fill)
        });
        let edge = |s: &str| Edge::line(s, Color::Indexed(64));
        let borders = [
            Border { top: edge("medium"), ..Default::default() },
            Border { bottom: edge("medium"), ..Default::default() },
            Border { bottom: edge("thin"), ..Default::default() },
            Border { top: edge("thin"), bottom: edge("thin"), ..Default::default() },
        ]
        .map(|b| intern(&mut styles.borders, b));
        let align = |h: &str| Alignment { horizontal: Some(h.to_owned()), ..Default::default() };
        let wrap = Alignment { vertical: Some("top".to_owned()), wrap: true, ..Default::default() };
        let mut make = |font: u32, fill: u32, border: u32, align: Alignment| {
            intern(&mut styles.xfs, Xf { font, fill, border, align, ..Default::default() })
        };
        Looks {
            title_left: make(fonts[0], fills[0], borders[0], align("left")),
            title_right: make(fonts[3], fills[0], borders[0], align("right")),
            head_left: make(fonts[0], fills[0], borders[2], align("left")),
            head_right: make(fonts[3], fills[0], borders[2], align("right")),
            item_left: make(fonts[1], fills[1], 0, align("left")),
            item_last_left: make(fonts[1], fills[1], borders[1], align("left")),
            section_left: make(fonts[2], fills[1], borders[3], align("left")),
            section_right: make(0, 0, borders[3], Alignment::default()),
            comment: make(fonts[4], 0, 0, wrap),
            plain: make(0, 0, 0, Alignment::default()),
            gray: fills[2],
            last_edge: borders[1],
        }
    }

    /// The five fonts of the report, set in the workbook's body font.
    fn fonts(styles: &mut crate::styles::Styles) -> [u32; 5] {
        let face = styles.theme_font.clone().unwrap_or_else(|| Font::default().name);
        let font = |bold: bool, size: f64, color: Color, family: Option<u32>| Font {
            name: face.clone(),
            size,
            bold,
            color,
            family,
            scheme: Some("minor".to_owned()),
            ..Font::default()
        };
        [
            font(true, 12.0, Color::Indexed(9), Some(2)),
            font(true, 11.0, Color::Indexed(8), Some(2)),
            font(true, 11.0, Color::Indexed(18), Some(2)),
            font(false, 10.0, Color::Indexed(9), Some(2)),
            font(false, 8.0, Color::Theme(1, 0.0), None),
        ]
        .map(|f| intern(&mut styles.fonts, f))
    }

    /// The format of a value cell: the source cell's number format, a gray fill where a scenario changes the
    /// cell, and the medium line under the last row of the report.
    fn value(&self, book: &mut Workbook, code: &str, gray: bool, last: bool) -> u32 {
        let num_fmt = book.styles.format_id(code);
        let xf = Xf {
            num_fmt,
            fill: if gray { self.gray } else { 0 },
            border: if last { self.last_edge } else { 0 },
            ..Default::default()
        };
        intern(&mut book.styles.xfs, xf)
    }
}
