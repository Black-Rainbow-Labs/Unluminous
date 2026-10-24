//! What If analysis: Goal Seek, data tables and the Scenario Manager.
//!
//! **Goal Seek** finds the value of one cell that makes a formula cell reach a goal. [`seek`] follows the
//! steps Excel 16 takes, which were measured by lowering Application.MaxChange just below each step's error
//! so that Excel stopped at each point in turn (`tools/excel-oracle/goalseek-chain.ps1`):
//!
//! 1. The first step is 1% of the starting value (0.01 from zero), in the direction a probe one step up
//!    says the formula must move.
//! 2. Each later step is the secant step from the last two points to the goal, at most ten times the
//!    previous step for the second step and twice the previous step after that.
//! 3. A point where the formula is an error is not used: the step is halved until the formula has a value.
//! 4. A flat formula doubles its step. Excel gives up after 25 such steps, and after 100 steps in all.
//! 5. It stops as soon as the formula is within MaxChange of the goal, which is 0.001 unless the workbook's
//!    iteration settings say otherwise.
//!
//! **Data tables** (`Range.Table`) are computed by [`compute_data_tables`], which puts each header value
//! in the input cell, computes the workbook and reads the formula.
//!
//! **Scenarios** are stored in the worksheet's `scenarios` element; [`Scenarios`] reads and writes it.

use crate::calc::Engine;
use crate::coord::{CellRef, Range};
use crate::model::{DataTable, Workbook};
use crate::value::Value;
use crate::xlsx::{attrs_text, escape, unescape_office};

// ---------------------------------------------------------------------------------------------------
// Goal Seek.

/// How many steps Goal Seek takes before it gives up.
pub const GOAL_SEEK_MAX_ITERATIONS: usize = 100;

/// Excel's default for how close to the goal Goal Seek must get (Application.MaxChange).
pub const DEFAULT_MAX_CHANGE: f64 = 0.001;

/// Where Goal Seek ended.
#[derive(Debug, Clone, PartialEq)]
pub struct SeekOutcome {
    /// The value of the changing cell Goal Seek stopped on.
    pub x: f64,
    /// The formula's value there.
    pub value: f64,
    /// The formula is within the tolerance of the goal.
    pub found: bool,
    /// How many points were tried after the starting value.
    pub iterations: usize,
}

/// Which way the first step goes: the way that moves the formula toward the goal, judged by a probe one
/// step up. A formula that does not move at the probe is stepped down.
fn first_direction(
    x0: f64,
    f0: f64,
    goal: f64,
    h: f64,
    f: &mut impl FnMut(f64) -> Option<f64>,
) -> f64 {
    let Some(probe) = f(x0 + h) else { return -1.0 };
    let rise = probe - f0;
    if rise == 0.0 || (goal - f0) * rise > 0.0 {
        if rise == 0.0 {
            -1.0
        } else {
            1.0
        }
    } else {
        -1.0
    }
}

/// Run Goal Seek from `x0`. `f` answers the formula's value for a value of the changing cell, or `None`
/// where the formula is not a number. Answers `None` when the formula is not a number at the start.
pub fn seek(
    x0: f64,
    goal: f64,
    tolerance: f64,
    mut f: impl FnMut(f64) -> Option<f64>,
) -> Option<SeekOutcome> {
    let f0 = f(x0)?;
    let hit = |v: f64| (v - goal).abs() <= tolerance;
    let outcome = |x: f64, value: f64, found: bool, iterations: usize| {
        Some(SeekOutcome { x, value, found, iterations })
    };
    if hit(f0) {
        return outcome(x0, f0, true, 0);
    }
    let h = if x0 == 0.0 { 0.01 } else { 0.01 * x0.abs() };
    let first = first_direction(x0, f0, goal, h, &mut f) * h;
    let x1 = x0 + first;
    let Some(f1) = f(x1) else { return outcome(x0, f0, false, 0) };
    let (mut prev, mut cur) = ((x0, f0), (x1, f1));
    let (mut last_step, mut n, mut flat) = (first, 1usize, 0u32);
    loop {
        if hit(cur.1) {
            return outcome(cur.0, cur.1, true, n);
        }
        if n >= GOAL_SEEK_MAX_ITERATIONS {
            return outcome(cur.0, cur.1, false, n);
        }
        let slope = (cur.1 - prev.1) / (cur.0 - prev.0);
        let mut step = if slope == 0.0 || !slope.is_finite() {
            flat += 1;
            if flat > 25 {
                return outcome(cur.0, cur.1, false, n);
            }
            if flat == 1 {
                last_step
            } else {
                last_step * 2.0
            }
        } else {
            flat = 0;
            let cap = last_step.abs() * if n == 1 { 10.0 } else { 2.0 };
            ((goal - cur.1) / slope).clamp(-cap, cap)
        };
        if step == 0.0 || !step.is_finite() {
            return outcome(cur.0, cur.1, false, n);
        }
        let mut next = None;
        for _ in 0..60 {
            let x = cur.0 + step;
            if x == cur.0 {
                break;
            }
            if let Some(v) = f(x) {
                next = Some((x, v));
                break;
            }
            step /= 2.0;
        }
        let Some(next) = next else { return outcome(cur.0, cur.1, false, n) };
        last_step = next.0 - cur.0;
        prev = cur;
        cur = next;
        n += 1;
    }
}

// ---------------------------------------------------------------------------------------------------
// Data tables.

impl DataTable {
    /// The formula Excel shows in every cell of the table: `TABLE(row input, column input)`.
    pub fn formula(&self) -> String {
        let a1 = |c: Option<CellRef>| c.map(|c| c.a1()).unwrap_or_default();
        if self.two_d {
            format!("TABLE({},{})", a1(self.r1), a1(self.r2))
        } else if self.row_input {
            format!("TABLE({},)", a1(self.r1))
        } else {
            format!("TABLE(,{})", a1(self.r1))
        }
    }

    /// The cell holding the formula the table computes for a result cell, and the input cells to fill and
    /// the cells to take their values from.
    fn plan(&self, row: u32, col: u32) -> Option<(CellRef, Vec<(CellRef, CellRef)>)> {
        let first = self.range.first;
        if first.row == 0 || first.col == 0 {
            return None;
        }
        let top = |c: u32| CellRef::new(first.row - 1, c);
        let left = |r: u32| CellRef::new(r, first.col - 1);
        if self.two_d {
            let mut sets = Vec::new();
            if let Some(r1) = self.r1 {
                sets.push((r1, top(col)));
            }
            if let Some(r2) = self.r2 {
                sets.push((r2, left(row)));
            }
            Some((CellRef::new(first.row - 1, first.col - 1), sets))
        } else if self.row_input {
            Some((left(row), self.r1.map(|r1| vec![(r1, top(col))]).unwrap_or_default()))
        } else {
            Some((top(col), self.r1.map(|r1| vec![(r1, left(row))]).unwrap_or_default()))
        }
    }
}

/// Whether any sheet has a data table.
pub fn has_data_tables(book: &Workbook) -> bool {
    book.sheets.iter().any(|s| !s.data_tables.is_empty())
}

/// Compute every data table. Each result cell is what the table's formula comes to with the input cells
/// holding that cell's header values. Answers the cells whose value changed, as (sheet, row, column).
pub fn compute_data_tables(book: &mut Workbook, now: f64, seed: u64) -> Vec<(usize, u32, u32)> {
    let tables: Vec<(usize, DataTable)> = book
        .sheets
        .iter()
        .enumerate()
        .flat_map(|(i, s)| s.data_tables.iter().cloned().map(move |t| (i, t)))
        .collect();
    let mut changed = Vec::new();
    for (sheet, table) in tables {
        changed.extend(compute_data_table(book, sheet, &table, now, seed));
    }
    changed
}

fn compute_data_table(
    book: &mut Workbook,
    sheet: usize,
    table: &DataTable,
    now: f64,
    seed: u64,
) -> Vec<(usize, u32, u32)> {
    let mut scratch = book.clone();
    let mut engine = Engine::new(&scratch);
    let mut results = Vec::new();
    for at in table.range.cells() {
        let Some((formula, sets)) = table.plan(at.row, at.col) else { continue };
        for (input, from) in &sets {
            let value = book.sheets[sheet].value_at(*from);
            let cell = scratch.sheets[sheet].cells.entry((input.row, input.col)).or_default();
            cell.value = value;
            cell.formula = None;
            engine.changed(&scratch, sheet, *input);
        }
        engine.recalculate_formulas(&mut scratch, now, seed);
        results.push((at, scratch.sheets[sheet].value_at(formula)));
    }
    let mut changed = Vec::new();
    for (at, value) in results {
        let value = if value.is_empty() { Value::Number(0.0) } else { value };
        let cell = book.sheets[sheet].cells.entry((at.row, at.col)).or_default();
        if cell.value != value {
            cell.value = value;
            changed.push((sheet, at.row, at.col));
        }
    }
    changed
}

// ---------------------------------------------------------------------------------------------------
// Scenarios.

/// One changing cell of a scenario and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct InputCell {
    pub at: CellRef,
    /// The value as text: a number as Excel writes it, or the text itself.
    pub value: String,
    pub deleted: bool,
    pub undone: bool,
    pub num_fmt_id: Option<u32>,
}

/// One scenario: a name and the values it gives some cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub locked: bool,
    pub hidden: bool,
    pub user: Option<String>,
    pub comment: Option<String>,
    pub cells: Vec<InputCell>,
}

/// The `scenarios` element of a worksheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scenarios {
    /// The index of the scenario shown last.
    pub current: u32,
    /// The index of the scenario shown on screen, once one was shown.
    pub show: Option<u32>,
    /// The result cells of the last summary report.
    pub results: Vec<Range>,
    pub list: Vec<Scenario>,
}

/// An attribute value as Office writes it: a line break as `_x000a_`.
fn office_attr(text: &str) -> String {
    escape(text).replace('\n', "_x000a_").replace('\r', "_x000d_")
}

impl Scenarios {
    /// Read a `scenarios` element.
    pub fn parse(xml: &str) -> Option<Self> {
        let doc = roxmltree::Document::parse(xml).ok()?;
        let root = doc.root_element();
        let list = root
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "scenario")
            .map(|s| Scenario {
                name: unescape_office(s.attribute("name").unwrap_or("")),
                locked: s.attribute("locked").is_some_and(|v| v == "1" || v == "true"),
                hidden: s.attribute("hidden").is_some_and(|v| v == "1" || v == "true"),
                user: s.attribute("user").map(unescape_office),
                comment: s.attribute("comment").map(unescape_office),
                cells: s
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "inputCells")
                    .filter_map(|c| {
                        Some(InputCell {
                            at: CellRef::parse(c.attribute("r")?)?,
                            value: unescape_office(c.attribute("val").unwrap_or("")),
                            deleted: c.attribute("deleted").is_some_and(|v| v == "1" || v == "true"),
                            undone: c.attribute("undone").is_some_and(|v| v == "1" || v == "true"),
                            num_fmt_id: c.attribute("numFmtId").and_then(|v| v.parse().ok()),
                        })
                    })
                    .collect(),
            })
            .collect();
        Some(Self {
            current: root.attribute("current").and_then(|v| v.parse().ok()).unwrap_or(0),
            show: root.attribute("show").and_then(|v| v.parse().ok()),
            results: root
                .attribute("sqref")
                .unwrap_or("")
                .split_whitespace()
                .filter_map(Range::parse)
                .collect(),
            list,
        })
    }

    /// The element as Excel writes it, empty when there are no scenarios.
    pub fn to_xml(&self) -> String {
        if self.list.is_empty() {
            return String::new();
        }
        let mut attrs = vec![("current".to_owned(), self.current.to_string())];
        if let Some(show) = self.show {
            attrs.push(("show".to_owned(), show.to_string()));
        }
        if !self.results.is_empty() {
            let refs: Vec<String> = self.results.iter().map(Range::a1).collect();
            attrs.push(("sqref".to_owned(), refs.join(" ")));
        }
        let mut out = format!("<scenarios{}>", attrs_text(&attrs));
        for s in &self.list {
            out.push_str(&format!("<scenario name=\"{}\"", office_attr(&s.name)));
            if s.locked {
                out.push_str(" locked=\"1\"");
            }
            if s.hidden {
                out.push_str(" hidden=\"1\"");
            }
            out.push_str(&format!(" count=\"{}\"", s.cells.len()));
            if let Some(user) = &s.user {
                out.push_str(&format!(" user=\"{}\"", office_attr(user)));
            }
            if let Some(comment) = &s.comment {
                out.push_str(&format!(" comment=\"{}\"", office_attr(comment)));
            }
            out.push('>');
            for c in &s.cells {
                out.push_str(&format!("<inputCells r=\"{}\"", c.at.a1()));
                if c.deleted {
                    out.push_str(" deleted=\"1\"");
                }
                if c.undone {
                    out.push_str(" undone=\"1\"");
                }
                out.push_str(&format!(" val=\"{}\"", office_attr(&c.value)));
                if let Some(id) = c.num_fmt_id {
                    out.push_str(&format!(" numFmtId=\"{id}\""));
                }
                out.push_str("/>");
            }
            out.push_str("</scenario>");
        }
        out.push_str("</scenarios>");
        out
    }
}

impl crate::model::Sheet {
    /// The sheet's scenarios.
    pub fn scenarios(&self) -> Scenarios {
        self.raw
            .elements
            .iter()
            .find(|(n, _)| n == "scenarios")
            .and_then(|(_, xml)| Scenarios::parse(xml))
            .unwrap_or_default()
    }

    /// Replace the sheet's scenarios.
    pub fn set_scenarios(&mut self, scenarios: &Scenarios) {
        self.raw.elements.retain(|(n, _)| n != "scenarios");
        let xml = scenarios.to_xml();
        if !xml.is_empty() {
            self.raw.elements.push(("scenarios".to_owned(), xml));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel's iterates for x squared to reach 2 from 1, found by lowering MaxChange below each error.
    #[test]
    fn goal_seek_takes_the_steps_excel_takes() {
        let mut tried = Vec::new();
        let out = seek(1.0, 2.0, 1e-12, |x| {
            tried.push(x);
            Some(x * x)
        })
        .unwrap();
        assert!(out.found);
        // The points Excel visited, in order (the probe at 1.01 above the start is not an iterate).
        for expected in [1.01, 1.11, 1.31, 1.4273140495867769, 1.4137148076023127, 1.4142112625265164] {
            assert!(
                tried.iter().any(|x| (x - expected).abs() < 1e-12),
                "{expected} was not tried: {tried:?}"
            );
        }
    }

    #[test]
    fn goal_seek_stops_when_within_the_tolerance_like_excel() {
        let out = seek(1.0, 2.0, 0.001, |x| Some(x * x)).unwrap();
        assert!(out.found);
        assert!((out.x - 1.4142112625265164).abs() < 1e-12, "{}", out.x);
    }

    #[test]
    fn a_line_is_solved_in_one_secant_step() {
        let out = seek(0.0, 11.0, 0.001, |x| Some(3.0 * x + 2.0)).unwrap();
        assert!(out.found && (out.x - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_step_into_an_error_is_halved() {
        // LN from 1 to -5 steps below zero, where LN is an error, and halves the step as Excel does.
        let mut tried = Vec::new();
        let _ = seek(1.0, -5.0, 0.001, |x| {
            tried.push(x);
            (x > 0.0).then(|| x.ln())
        });
        assert!(tried.iter().any(|x| (x - 0.29).abs() < 1e-9));
        assert!(tried.iter().any(|x| (x - 0.09).abs() < 1e-9), "{tried:?}");
    }

    #[test]
    fn a_flat_formula_doubles_its_steps_and_gives_up() {
        let out = seek(3.0, 2.0, 0.001, |_| Some(1.0)).unwrap();
        assert!(!out.found);
        assert!((out.x - (3.0 - 0.03 * 33_554_432.0)).abs() < 1e-3, "{}", out.x);
    }

    #[test]
    fn scenarios_read_and_write_what_excel_wrote() {
        let xml = "<scenarios current=\"1\" show=\"1\"><scenario name=\"Base\" locked=\"1\" count=\"2\" user=\"Oracle\" comment=\"Created by Oracle on 10/24/2026\"><inputCells r=\"B1\" val=\"10\"/><inputCells r=\"B2\" val=\"100\"/></scenario><scenario name=\"Best\" locked=\"1\" count=\"1\" user=\"Oracle\" comment=\"a_x000a_b\"><inputCells r=\"B1\" val=\"12\"/></scenario></scenarios>";
        let parsed = Scenarios::parse(xml).unwrap();
        assert_eq!(parsed.list.len(), 2);
        assert_eq!(parsed.list[1].comment.as_deref(), Some("a\nb"));
        assert_eq!(parsed.to_xml(), xml);
    }

    #[test]
    fn a_data_table_names_its_inputs_as_excel_shows_them() {
        let table = DataTable {
            range: Range::parse("N2:P4").unwrap(),
            two_d: true,
            row_input: true,
            r1: CellRef::parse("B2"),
            r2: CellRef::parse("B1"),
            attrs: Vec::new(),
        };
        assert_eq!(table.formula(), "TABLE(B2,B1)");
        let column = DataTable { two_d: false, row_input: false, r2: None, ..table.clone() };
        assert_eq!(column.formula(), "TABLE(,B2)");
        let row = DataTable { two_d: false, row_input: true, r2: None, ..table };
        assert_eq!(row.formula(), "TABLE(B2,)");
    }
}
