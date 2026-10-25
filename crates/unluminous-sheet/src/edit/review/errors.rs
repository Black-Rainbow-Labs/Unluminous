//! Error Checking: the green triangles in a cell's corner, by Excel's rules.
//!
//! Each rule says which cells it flags and gives Excel's sentence for them. The errors a person told Excel
//! to ignore are kept in the sheet as `ignoredErrors` and are left out of the answer.

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::formula::{Expr, Shape};
use crate::model::{Cell, IgnoredError, Sheet, Workbook};
use crate::value::Value;

/// One of Excel's error checking rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorRule {
    /// A formula that results in an error.
    EvalError,
    /// Text that reads as a date with a two digit year.
    TwoDigitTextYear,
    /// A number formatted as text or typed with an apostrophe.
    NumberAsText,
    /// A formula that differs from the formulas around it.
    InconsistentFormula,
    /// A formula over a range with more numbers beside it.
    OmittedCells,
    /// A formula in an unlocked cell.
    UnlockedFormula,
    /// A formula that refers to empty cells.
    EmptyCellReference,
    /// A cell of a table's calculated column that does not have the column's formula.
    InconsistentCalculatedColumn,
}

impl ErrorRule {
    pub const ALL: [ErrorRule; 8] = [
        ErrorRule::EvalError,
        ErrorRule::TwoDigitTextYear,
        ErrorRule::NumberAsText,
        ErrorRule::InconsistentFormula,
        ErrorRule::OmittedCells,
        ErrorRule::UnlockedFormula,
        ErrorRule::EmptyCellReference,
        ErrorRule::InconsistentCalculatedColumn,
    ];

    /// The number Excel's `Range.Errors.Item` gives the rule.
    pub fn number(self) -> u8 {
        match self {
            ErrorRule::EvalError => 1,
            ErrorRule::TwoDigitTextYear => 2,
            ErrorRule::NumberAsText => 3,
            ErrorRule::InconsistentFormula => 4,
            ErrorRule::OmittedCells => 5,
            ErrorRule::UnlockedFormula => 6,
            ErrorRule::EmptyCellReference => 7,
            ErrorRule::InconsistentCalculatedColumn => 9,
        }
    }

    /// The attribute of an `ignoredError` element that switches the rule off.
    pub fn attribute(self) -> &'static str {
        match self {
            ErrorRule::EvalError => "evalError",
            ErrorRule::TwoDigitTextYear => "twoDigitTextYear",
            ErrorRule::NumberAsText => "numberStoredAsText",
            ErrorRule::InconsistentFormula => "formula",
            ErrorRule::OmittedCells => "formulaRange",
            ErrorRule::UnlockedFormula => "unlockedFormula",
            ErrorRule::EmptyCellReference => "emptyCellReference",
            ErrorRule::InconsistentCalculatedColumn => "calculatedColumn",
        }
    }

    /// What Excel says about a cell the rule flags.
    pub fn message(self) -> &'static str {
        match self {
            ErrorRule::EvalError => "This cell contains a formula that results in an error.",
            ErrorRule::TwoDigitTextYear => "This cell contains a date with a two-digit year that is stored as text.",
            ErrorRule::NumberAsText => "The number in this cell is formatted as text or preceded by an apostrophe.",
            ErrorRule::InconsistentFormula => "The formula in this cell differs from the formulas in this area of the spreadsheet.",
            ErrorRule::OmittedCells => "The formula in this cell refers to a range that has additional numbers adjacent to it.",
            ErrorRule::UnlockedFormula => "This cell contains a formula and is not locked.",
            ErrorRule::EmptyCellReference => "The formula in this cell refers to empty cells.",
            ErrorRule::InconsistentCalculatedColumn => "The formula in this cell differs from the calculated column formula in the table.",
        }
    }
}

/// Which rules are on. Excel has every rule on but the one for empty cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorOptions {
    rules: [bool; 8],
}

impl Default for ErrorOptions {
    fn default() -> Self {
        let mut rules = [true; 8];
        rules[ErrorRule::EmptyCellReference as usize] = false;
        Self { rules }
    }
}

impl ErrorOptions {
    /// Every rule on, the empty cell rule too.
    pub fn all() -> Self {
        Self { rules: [true; 8] }
    }

    pub fn with(mut self, rule: ErrorRule, on: bool) -> Self {
        self.rules[rule as usize] = on;
        self
    }

    pub fn is_on(&self, rule: ErrorRule) -> bool {
        self.rules[rule as usize]
    }
}

/// A cell an error checking rule flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorFinding {
    pub sheet: usize,
    pub at: CellRef,
    pub rule: ErrorRule,
}

impl ErrorFinding {
    pub fn message(&self) -> &'static str {
        self.rule.message()
    }
}

impl Session {
    /// Every cell the rules flag on a sheet, or on all of them, in reading order, with the cells the
    /// person told Excel to ignore left out. `within` limits the check to a range.
    pub fn check_errors(
        &self,
        sheet: Option<usize>,
        within: Option<Range>,
        options: ErrorOptions,
    ) -> Vec<ErrorFinding> {
        let mut out = Vec::new();
        for (si, s) in self.book.sheets.iter().enumerate() {
            if sheet.is_some_and(|only| only != si) {
                continue;
            }
            for (&(row, col), cell) in &s.cells {
                let at = CellRef::new(row, col);
                if within.is_some_and(|w| !w.contains(at)) {
                    continue;
                }
                for rule in ErrorRule::ALL.into_iter().filter(|r| options.is_on(*r)) {
                    if flags(&self.book, si, s, at, cell, rule) && !is_ignored(s, at, rule) {
                        out.push(ErrorFinding { sheet: si, at, rule });
                    }
                }
            }
        }
        out
    }

    /// Ignore one rule for some cells: Ignore Error.
    pub fn ignore_error(
        &mut self,
        sheet: usize,
        range: Range,
        rule: ErrorRule,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Ignore error");
        let attribute = rule.attribute().to_owned();
        let rows = &mut self.book.sheets[sheet].ignored_errors;
        match rows.iter_mut().find(|r| r.ranges == [range]) {
            Some(row) if !row.rules.contains(&attribute) => row.rules.push(attribute),
            Some(_) => {}
            None => rows.push(IgnoredError { ranges: vec![range], rules: vec![attribute] }),
        }
        Ok(())
    }

    /// Forget every ignored error of a sheet: Reset Ignored Errors.
    pub fn reset_ignored_errors(&mut self, sheet: usize) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.remember_whole("Reset ignored errors");
        self.book.sheets[sheet].ignored_errors.clear();
        Ok(())
    }
}

fn is_ignored(s: &Sheet, at: CellRef, rule: ErrorRule) -> bool {
    s.ignored_errors.iter().any(|row| {
        row.ranges.iter().any(|r| r.contains(at)) && row.rules.iter().any(|a| a == rule.attribute())
    })
}

/// Whether a rule flags a cell.
fn flags(book: &Workbook, si: usize, s: &Sheet, at: CellRef, cell: &Cell, rule: ErrorRule) -> bool {
    match rule {
        ErrorRule::EvalError => {
            cell.formula.is_some()
                && matches!(cell.value, Value::Error(e) if e != crate::value::ErrorKind::NA)
        }
        ErrorRule::NumberAsText => number_as_text(book, cell),
        ErrorRule::TwoDigitTextYear => two_digit_year(cell),
        ErrorRule::UnlockedFormula => {
            cell.formula.is_some() && book.styles.xf(cell.style).locked == Some(false)
        }
        ErrorRule::InconsistentFormula => inconsistent(s, at, cell),
        ErrorRule::OmittedCells => omitted_cells(s, at, cell),
        ErrorRule::EmptyCellReference => refers_to_empty(book, si, cell),
        ErrorRule::InconsistentCalculatedColumn => off_the_column(book, si, at, cell),
    }
}

fn text_of(cell: &Cell) -> Option<&str> {
    match (&cell.value, &cell.formula) {
        (Value::Text(t), None) => Some(t),
        _ => None,
    }
}

fn number_as_text(book: &Workbook, cell: &Cell) -> bool {
    let Some(text) = text_of(cell) else { return false };
    let typed = crate::input::read(text.trim(), book.date1904, 2000);
    matches!(typed.value, Value::Number(_)) && typed.format.is_none_or(|f| !is_date_format(f))
}

/// Whether a number format the typing of a value gives is one for dates or times.
fn is_date_format(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    ["yy", "dd", "mm", "hh", ":", "d-m", "m/d"].iter().any(|p| lower.contains(p))
}

/// Text that reads as a date written with two digits for the year: `1/2/30`, `5-Jan-25` or `Jan 5 25`.
fn two_digit_year(cell: &Cell) -> bool {
    const MONTHS: [&str; 12] =
        ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let Some(text) = text_of(cell) else { return false };
    let parts: Vec<&str> = text.trim().split(['/', '-', ' ']).filter(|p| !p.is_empty()).collect();
    let numeric = |p: &str, most: u32| p.parse::<u32>().is_ok_and(|n| (1..=most).contains(&n));
    let month_name =
        |p: &str| p.len() >= 3 && MONTHS.contains(&p[..3].to_ascii_lowercase().as_str());
    let [a, b, year] = parts[..] else { return false };
    if year.len() != 2 || !year.bytes().all(|c| c.is_ascii_digit()) {
        return false;
    }
    (numeric(a, 12) && numeric(b, 31))
        || (numeric(a, 31) && month_name(b))
        || (month_name(a) && numeric(b, 31))
}

/// The formula in a cell compared with the formula in another as if one were copied to the other.
fn same_formula(a: &Cell, a_at: CellRef, b: &Cell, b_at: CellRef) -> Option<bool> {
    let (fa, fb) = (a.formula.as_ref()?, b.formula.as_ref()?);
    let moved = crate::formula::shifted(
        &fa.expr,
        b_at.row as i64 - a_at.row as i64,
        b_at.col as i64 - a_at.col as i64,
    );
    Some(moved == *fb.expr)
}

/// A formula with a formula on both sides of it, in a row or a column, that match each other and not it.
fn inconsistent(s: &Sheet, at: CellRef, cell: &Cell) -> bool {
    if cell.formula.is_none() {
        return false;
    }
    [(1i64, 0i64), (0, 1)].into_iter().any(|(dr, dc)| {
        let (Some(before), Some(after)) = (at.offset(-dr, -dc), at.offset(dr, dc)) else {
            return false;
        };
        let (Some(a), Some(b)) = (s.cell(before), s.cell(after)) else { return false };
        same_formula(a, before, b, after) == Some(true)
            && same_formula(a, before, cell, at) == Some(false)
    })
}

/// A formula with a range in it where the cell just past the range holds a number.
fn omitted_cells(s: &Sheet, at: CellRef, cell: &Cell) -> bool {
    let Some(formula) = cell.formula.as_ref() else { return false };
    let mut omits = false;
    formula.expr.walk(&mut |e| {
        let Expr::Ref(r) = e else { return };
        if r.shape != Shape::Range || r.sheet.is_some() {
            return;
        }
        let range = r.range();
        let numeric = |p: Option<CellRef>| {
            p.is_some_and(|p| {
                matches!(s.value_at(p), Value::Number(_)) && p != at && !range.contains(p)
            })
        };
        let beside = if range.first.col == range.last.col {
            [range.first.offset(-1, 0), range.last.offset(1, 0)]
        } else if range.first.row == range.last.row {
            [range.first.offset(0, -1), range.last.offset(0, 1)]
        } else {
            [None, None]
        };
        omits = omits || beside.into_iter().any(numeric);
    });
    omits
}

/// A formula that reads a cell nothing is in.
fn refers_to_empty(book: &Workbook, si: usize, cell: &Cell) -> bool {
    let Some(formula) = cell.formula.as_ref() else { return false };
    crate::calc::precedents_of(book, si, &formula.expr).into_iter().any(|(first, last, range)| {
        first == last
            && book
                .sheets
                .get(first)
                .is_some_and(|t| range.len() < 100_000 && reads_empty(t, range))
    })
}

/// Whether a range read by a formula is empty in the way Excel flags: every cell of it is empty, or just
/// the last one is, which is the range that reaches one cell past its data.
fn reads_empty(sheet: &Sheet, range: Range) -> bool {
    let empty = |p: CellRef| {
        sheet.value_at(p).is_empty() && sheet.cell(p).is_none_or(|c| c.formula.is_none())
    };
    range.cells().all(empty)
        || (range.len() > 1
            && empty(range.last)
            && range.cells().filter(|p| *p != range.last).all(|p| !empty(p)))
}

/// A cell in a table column whose formula differs from the column's calculated column formula.
fn off_the_column(book: &Workbook, si: usize, at: CellRef, cell: &Cell) -> bool {
    let Some(table) = book.table_at(si, at) else { return false };
    let Some(data) = table.data_range() else { return false };
    if !data.contains(at) {
        return false;
    }
    let column = (at.col - table.range.first.col) as usize;
    let Some(wanted) = calculated_formula(&table.xml, column) else { return false };
    match &cell.formula {
        Some(f) => crate::formula::print::print(&f.expr, false) != wanted.trim_start_matches('='),
        None => true,
    }
}

/// The calculated column formula of the `n`th column of a table, from the table's XML.
fn calculated_formula(xml: &str, n: usize) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let columns = doc.descendants().find(|d| d.tag_name().name() == "tableColumns")?;
    let column = columns.children().filter(roxmltree::Node::is_element).nth(n)?;
    let formula = column.children().find(|c| c.tag_name().name() == "calculatedColumnFormula")?;
    let text = formula.text()?.to_owned();
    crate::formula::parser::parse(&text).ok().map(|e| crate::formula::print::print(&e, false))
}
