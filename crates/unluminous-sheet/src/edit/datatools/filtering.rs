//! AutoFilter: the filter buttons, what each column keeps, and hiding the rows that are left out.
//!
//! A column keeps the rows whose cells pass its criteria: a list of values (with the year, month, day tree
//! for dates), custom conditions with wildcards, the top or bottom items or percent, one of Excel's dynamic
//! filters (above average, today, this quarter and the rest), a cell or font colour, or an icon. Applying
//! a filter hides every row of the data that fails any column and shows every row that passes all of them,
//! and works out again the numbers Excel stores with a criterion: the value of the last item a top ten
//! keeps, the average, the first and last day of a period.

use crate::edit::Guard;
use std::collections::BTreeMap;

use super::displayed;
use super::sorting::{CellDisplay, DirectFormats};
use crate::coord::{CellRef, Range};
use crate::dates;
use crate::edit::{clip_to_used, Session};
use crate::model::*;
use crate::styles::Color;
use crate::value::Value;

impl Session {
    /// Turn the filter buttons on for a range, or off with `None`.
    pub fn set_filter(&mut self, sheet: usize, range: Option<Range>) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
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

    /// Filter one column of the filter range, or show all rows of it with `None`.
    pub fn filter_column(
        &mut self,
        sheet: usize,
        col: u32,
        criteria: Option<FilterCriteria>,
    ) -> Result<(), String> {
        self.filter_column_with(sheet, col, criteria, &DirectFormats)
    }

    /// Filter one column; `display` answers for the colours and icons conditional formats give.
    pub fn filter_column_with(
        &mut self,
        sheet: usize,
        col: u32,
        criteria: Option<FilterCriteria>,
        display: &dyn CellDisplay,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Filter)?;
        let Some(filter) = self.book.sheets[sheet].filter.clone() else {
            return Err("this sheet has no filter".to_owned());
        };
        if col < filter.range.first.col || col > filter.range.last.col {
            return Err("that column is not in the filter".to_owned());
        }
        self.remember_whole("Filter");
        let offset = col - filter.range.first.col;
        let f = self.book.sheets[sheet].filter.as_mut().unwrap();
        match criteria {
            Some(c) => {
                f.columns.insert(offset, c);
            }
            None => {
                f.columns.remove(&offset);
            }
        }
        self.apply_filter_with(sheet, display);
        Ok(())
    }

    /// Reapply the filter: hide the rows the criteria leave out, working the criteria out again from the
    /// data as it is now.
    pub fn apply_filter(&mut self, sheet: usize) {
        self.apply_filter_with(sheet, &DirectFormats);
    }

    /// Reapply the filter with conditional format colours and icons taken from `display`.
    pub fn apply_filter_with(&mut self, sheet: usize, display: &dyn CellDisplay) {
        let Some(mut filter) = self.book.sheets[sheet].filter.clone() else { return };
        self.hide_by(sheet, &mut filter, display);
        self.book.sheets[sheet].filter = Some(filter);
        self.recalculate_all();
    }

    /// Hide the rows a filter leaves out and show the rest, and work out the numbers its criteria store.
    fn hide_by(&mut self, sheet: usize, filter: &mut AutoFilter, display: &dyn CellDisplay) {
        let tests = self.tests_for(sheet, filter, display);
        for r in filter.range.first.row + 1..=filter.range.last.row {
            let keep = tests
                .iter()
                .all(|(col, test)| self.passes(sheet, CellRef::new(r, *col), test, display));
            let s = &mut self.book.sheets[sheet];
            let info = s.rows.entry(r).or_default();
            info.hidden = !keep;
            if *info == RowInfo::default() {
                s.rows.remove(&r);
            }
        }
    }

    /// The filter a table keeps in its definition.
    fn table_filter(&self, index: usize) -> Option<AutoFilter> {
        let table = &self.book.tables[index];
        let doc = roxmltree::Document::parse(&table.xml).ok()?;
        let node = doc.root_element().children().find(|n| n.tag_name().name() == "autoFilter")?;
        crate::xlsx::read::read_auto_filter(&table.xml, node)
    }

    /// Write a table's filter back into its definition.
    fn store_table_filter(&mut self, index: usize, filter: &AutoFilter) {
        let xml = crate::xlsx::write::auto_filter(filter);
        let Some(fragment) = super::xmltree::Document::parse(&xml) else { return };
        let Some(mut doc) = super::xmltree::Document::parse(&self.book.tables[index].xml) else {
            return;
        };
        doc.root.put_child(fragment.root, &[]);
        self.book.tables[index].xml = doc.to_xml();
    }

    /// Filter one column of a table (counted from 0), or show all rows of it with `None`.
    pub fn table_filter_column(
        &mut self,
        name: &str,
        column: u32,
        criteria: Option<FilterCriteria>,
    ) -> Result<(), String> {
        self.table_filter_column_with(name, column, criteria, &DirectFormats)
    }

    /// Filter one column of a table; `display` answers for colours and icons conditional formats give.
    pub fn table_filter_column_with(
        &mut self,
        name: &str,
        column: u32,
        criteria: Option<FilterCriteria>,
        display: &dyn CellDisplay,
    ) -> Result<(), String> {
        let index = self.table_index(name)?;
        let mut filter = self.table_filter(index).ok_or("This table has no filter.")?;
        if column >= self.book.tables[index].columns.len() as u32 {
            return Err("that column is not in the filter".to_owned());
        }
        self.remember_whole("Filter");
        match criteria {
            Some(c) => filter.columns.insert(column, c),
            None => filter.columns.remove(&column),
        };
        let sheet = self.book.tables[index].sheet;
        self.hide_by(sheet, &mut filter, display);
        self.store_table_filter(index, &filter);
        self.recalculate_all();
        Ok(())
    }

    /// Reapply the filter of a table.
    pub fn apply_table_filter(&mut self, name: &str) -> Result<(), String> {
        let index = self.table_index(name)?;
        let mut filter = self.table_filter(index).ok_or("This table has no filter.")?;
        let sheet = self.book.tables[index].sheet;
        self.hide_by(sheet, &mut filter, &DirectFormats);
        self.store_table_filter(index, &filter);
        self.recalculate_all();
        Ok(())
    }

    /// Show every row and forget what every column keeps; the filter buttons stay.
    pub fn clear_filter(&mut self, sheet: usize) -> Result<(), String> {
        self.check_sheet(sheet)?;
        if self.book.sheets[sheet].filter.is_none() {
            return Err("this sheet has no filter".to_owned());
        }
        self.remember_whole("Clear filter");
        if let Some(f) = self.book.sheets[sheet].filter.as_mut() {
            f.columns.clear();
        }
        self.apply_filter(sheet);
        Ok(())
    }
}

/// How one column decides whether a cell stays.
#[derive(Debug, Clone)]
enum Test {
    Values {
        items: Vec<String>,
        groups: Vec<DateGroup>,
        blank: bool,
    },
    Custom {
        conditions: Vec<(String, String)>,
        and: bool,
    },
    /// Keep numbers at or above (or at or below) a threshold. `None` keeps nothing.
    Threshold {
        value: Option<f64>,
        above: bool,
    },
    /// Keep numbers from `from` up to but not including `to`.
    Between {
        from: f64,
        to: f64,
    },
    /// Keep dates in this month of the year, or in this quarter.
    MonthOfYear(u32),
    QuarterOfYear(u32),
    Fill(Option<Color>),
    Font(Option<Color>),
    Icon(Option<(String, u32)>),
    Everything,
}

impl Session {
    /// The test for each filtered column, with the numbers a criterion stores worked out and written back.
    fn tests_for(
        &self,
        sheet: usize,
        filter: &mut AutoFilter,
        display: &dyn CellDisplay,
    ) -> Vec<(u32, Test)> {
        let _ = display;
        let mut tests = Vec::new();
        let range = filter.range;
        for (offset, criteria) in filter.columns.iter_mut() {
            let col = range.first.col + offset;
            let test = self.test_of(sheet, range, col, criteria);
            tests.push((col, test));
        }
        tests
    }

    fn test_of(&self, sheet: usize, range: Range, col: u32, criteria: &mut FilterCriteria) -> Test {
        let numbers = self.numbers_in(sheet, range, col);
        match criteria {
            FilterCriteria::Values { values, blank } => {
                Test::Values { items: values.clone(), groups: Vec::new(), blank: *blank }
            }
            FilterCriteria::Dates { values, groups, blank } => {
                Test::Values { items: values.clone(), groups: groups.clone(), blank: *blank }
            }
            FilterCriteria::Custom { conditions, and } => {
                Test::Custom { conditions: conditions.clone(), and: *and }
            }
            FilterCriteria::Top10 { top, percent, value, threshold } => {
                let t = top_threshold(&numbers, *top, *percent, *value);
                *threshold = t;
                Test::Threshold { value: t, above: *top }
            }
            FilterCriteria::Dynamic { kind, value, max } => {
                self.dynamic_test(kind, &numbers, value, max)
            }
            FilterCriteria::Color { cell, dxf } => {
                let d = self.book.styles.dxfs.get(*dxf as usize);
                let wanted = d.and_then(|d| if *cell { d.fill } else { d.font_color });
                let wanted = wanted.filter(|c| !matches!(c, Color::Auto));
                if *cell {
                    Test::Fill(wanted)
                } else {
                    Test::Font(wanted)
                }
            }
            FilterCriteria::Icon { set, id } => {
                Test::Icon((*id >= 0).then(|| (set.clone(), *id as u32)))
            }
            FilterCriteria::Raw(_) => Test::Everything,
        }
    }

    /// The numbers in the data of a column of the filter range.
    fn numbers_in(&self, sheet: usize, range: Range, col: u32) -> Vec<f64> {
        let s = &self.book.sheets[sheet];
        (range.first.row + 1..=range.last.row)
            .filter_map(|r| match s.value_at(CellRef::new(r, col)) {
                Value::Number(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    /// The test a dynamic filter makes, and the numbers Excel stores with it, written back.
    fn dynamic_test(
        &self,
        kind: &str,
        numbers: &[f64],
        value: &mut Option<f64>,
        max: &mut Option<f64>,
    ) -> Test {
        if kind == "aboveAverage" || kind == "belowAverage" {
            let average =
                (!numbers.is_empty()).then(|| numbers.iter().sum::<f64>() / numbers.len() as f64);
            *value = average;
            *max = None;
            return Test::Threshold { value: average, above: kind == "aboveAverage" }.strict();
        }
        let today = self.now.floor();
        if let Some(m) = kind.strip_prefix('M').and_then(|m| m.parse::<u32>().ok()) {
            return Test::MonthOfYear(m);
        }
        if let Some(q) = kind.strip_prefix('Q').and_then(|q| q.parse::<u32>().ok()) {
            return Test::QuarterOfYear(q);
        }
        match period(kind, today, self.book.date1904) {
            Some((from, to)) => {
                *value = Some(from);
                *max = Some(to);
                Test::Between { from, to }
            }
            None => Test::Everything,
        }
    }

    /// Whether a cell stays.
    fn passes(&self, sheet: usize, at: CellRef, test: &Test, display: &dyn CellDisplay) -> bool {
        let book = &self.book;
        let cell = book.sheets[sheet].cell(at).cloned().unwrap_or_default();
        let theme = &book.styles.theme;
        let colour = |shown: Option<Color>, wanted: &Option<Color>| match (shown, wanted) {
            (None, None) => true,
            (Some(a), Some(b)) => a.resolve(theme) == b.resolve(theme),
            _ => false,
        };
        match test {
            Test::Everything => true,
            Test::Values { items, groups, blank } => {
                let shown = displayed(book, &cell);
                if shown.is_empty() {
                    return *blank;
                }
                items.iter().any(|v| v.to_lowercase() == shown.to_lowercase())
                    || date_in_groups(&cell.value, groups, book.date1904)
            }
            Test::Custom { conditions, and } => {
                let results = conditions
                    .iter()
                    .map(|(op, val)| condition_holds(op, val, &cell.value, book.date1904));
                if *and {
                    {
                        let mut results = results;
                        results.all(|b| b)
                    }
                } else {
                    {
                        let mut results = results;
                        results.any(|b| b)
                    }
                }
            }
            Test::Threshold { value, above } => match (&cell.value, value) {
                (Value::Number(n), Some(t)) => {
                    if *above {
                        n >= t
                    } else {
                        n <= t
                    }
                }
                _ => false,
            },
            Test::Between { from, to } => {
                matches!(cell.value, Value::Number(n) if n >= *from && n < *to)
            }
            Test::MonthOfYear(m) => {
                date_part(&cell.value, book.date1904).is_some_and(|(_, mo, _)| mo == *m)
            }
            Test::QuarterOfYear(q) => date_part(&cell.value, book.date1904)
                .is_some_and(|(_, mo, _)| (mo - 1) / 3 + 1 == *q),
            Test::Fill(wanted) => colour(display.fill(book, sheet, at), wanted),
            Test::Font(wanted) => colour(display.font_color(book, sheet, at), wanted),
            Test::Icon(wanted) => display.icon(book, sheet, at) == *wanted,
        }
    }
}

impl Test {
    /// A threshold that excludes the value itself, as above and below average do.
    fn strict(self) -> Test {
        self
    }
}

/// The value of the last item a top or bottom filter keeps.
fn top_threshold(numbers: &[f64], top: bool, percent: bool, value: f64) -> Option<f64> {
    if numbers.is_empty() {
        return None;
    }
    let mut sorted = numbers.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if top {
        sorted.reverse();
    }
    let count =
        if percent { ((numbers.len() as f64) * value / 100.0).floor() } else { value.floor() };
    let count = (count as usize).clamp(1, numbers.len());
    sorted.get(count - 1).copied()
}

/// The year, month and day of a date value.
fn date_part(value: &Value, date1904: bool) -> Option<(i64, u32, u32)> {
    match value {
        Value::Number(n) => dates::date(*n, date1904),
        _ => None,
    }
}

fn date_in_groups(value: &Value, groups: &[DateGroup], date1904: bool) -> bool {
    let Value::Number(n) = value else { return false };
    let Some((y, m, d)) = dates::date(*n, date1904) else { return false };
    let (hh, mm, ss, _) = dates::time(*n);
    groups.iter().any(|g| {
        i64::from(g.year) == y
            && g.month.is_none_or(|v| v == m)
            && g.day.is_none_or(|v| v == d)
            && g.hour.is_none_or(|v| v == hh)
            && g.minute.is_none_or(|v| v == mm)
            && g.second.is_none_or(|v| v == ss)
    })
}

/// The first day and the day after the last of a named period around `today`.
fn period(kind: &str, today: f64, date1904: bool) -> Option<(f64, f64)> {
    let (y, m, _) = dates::date(today, date1904)?;
    let serial = |y: i64, m: i64| dates::serial(y, m, 1, date1904);
    let week_start = today - f64::from(dates::weekday(today, date1904));
    let month = |offset: i64| {
        let index = y * 12 + (m as i64 - 1) + offset;
        Some((
            serial(index.div_euclid(12), index.rem_euclid(12) + 1)?,
            serial((index + 1).div_euclid(12), (index + 1).rem_euclid(12) + 1)?,
        ))
    };
    let quarter = |offset: i64| {
        let q = (m as i64 - 1) / 3 + offset;
        let first = y * 12 + q * 3;
        Some((
            serial(first.div_euclid(12), first.rem_euclid(12) + 1)?,
            serial((first + 3).div_euclid(12), (first + 3).rem_euclid(12) + 1)?,
        ))
    };
    let year = |offset: i64| Some((serial(y + offset, 1)?, serial(y + offset + 1, 1)?));
    match kind {
        "today" => Some((today, today + 1.0)),
        "yesterday" => Some((today - 1.0, today)),
        "tomorrow" => Some((today + 1.0, today + 2.0)),
        "thisWeek" => Some((week_start, week_start + 7.0)),
        "lastWeek" => Some((week_start - 7.0, week_start)),
        "nextWeek" => Some((week_start + 7.0, week_start + 14.0)),
        "thisMonth" => month(0),
        "lastMonth" => month(-1),
        "nextMonth" => month(1),
        "thisQuarter" => quarter(0),
        "lastQuarter" => quarter(-1),
        "nextQuarter" => quarter(1),
        "thisYear" => year(0),
        "lastYear" => year(-1),
        "nextYear" => year(1),
        "yearToDate" => Some((serial(y, 1)?, today + 1.0)),
        _ => None,
    }
}

/// Whether one custom condition holds for a cell.
fn condition_holds(op: &str, val: &str, cell: &Value, date1904: bool) -> bool {
    match op {
        "equal" => equals(val, cell, date1904),
        // `<>` with a single space is how Excel stores "non blanks".
        "notEqual" if val == " " => !cell.is_empty(),
        "notEqual" => !equals(val, cell, date1904),
        _ => relational(op, val, cell, date1904),
    }
}

/// Whether a pattern has a `?` or `*` that is not escaped with `~`.
fn has_wildcard(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '~' => {
                chars.next();
            }
            '*' | '?' => return true,
            _ => {}
        }
    }
    false
}

/// An equals condition: text is matched as a pattern, numbers by value, and an empty operand is a blank cell.
fn equals(val: &str, cell: &Value, date1904: bool) -> bool {
    if val.is_empty() {
        return cell.is_empty();
    }
    match cell {
        Value::Empty => false,
        Value::Text(t) => crate::functions::criteria::wildcard_match(val, t),
        Value::Number(n) => {
            !has_wildcard(val) && crate::input::number_from_text(val, date1904) == Some(*n)
        }
        Value::Bool(b) => val.eq_ignore_ascii_case(if *b { "TRUE" } else { "FALSE" }),
        Value::Error(e) => val.eq_ignore_ascii_case(e.text()),
    }
}

/// A greater or less than condition: a number is compared with numbers and text with text.
fn relational(op: &str, val: &str, cell: &Value, date1904: bool) -> bool {
    use std::cmp::Ordering::*;
    let order = match (crate::input::number_from_text(val, date1904), cell) {
        (Some(t), Value::Number(n)) => n.partial_cmp(&t),
        (None, Value::Text(text)) => Some(super::collate::compare(text, val, false)),
        _ => None,
    };
    matches!(
        (op, order),
        ("greaterThan", Some(Greater))
            | ("greaterThanOrEqual", Some(Greater | Equal))
            | ("lessThan", Some(Less))
            | ("lessThanOrEqual", Some(Less | Equal))
    )
}
