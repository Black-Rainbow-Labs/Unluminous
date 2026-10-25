//! The criteria Excel's AutoFilter method makes of its arguments, and the filter steps of a scenario.

use serde_json::Value as Json;
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::{DateGroup, DateGrouping, FilterCriteria};
use unluminous_sheet::styles::{Color, Dxf};

use super::operations::Context;
use super::range_of;

/// The name of a dynamic filter from the number Excel's AutoFilter method takes.
fn dynamic_name(code: i64) -> String {
    const NAMES: [&str; 16] = [
        "today",
        "yesterday",
        "tomorrow",
        "thisWeek",
        "lastWeek",
        "nextWeek",
        "thisMonth",
        "lastMonth",
        "nextMonth",
        "thisQuarter",
        "lastQuarter",
        "nextQuarter",
        "thisYear",
        "lastYear",
        "nextYear",
        "yearToDate",
    ];
    match code {
        1..=16 => NAMES[code as usize - 1].to_owned(),
        17..=20 => format!("Q{}", code - 16),
        21..=32 => format!("M{}", code - 20),
        33 => "aboveAverage".to_owned(),
        _ => "belowAverage".to_owned(),
    }
}

/// An operator and an operand from the text of a criterion: `>=5`, `<>x`, `=a*`, `abc`.
fn split(text: &str) -> (&'static str, String) {
    for (prefix, name) in [
        (">=", "greaterThanOrEqual"),
        ("<=", "lessThanOrEqual"),
        ("<>", "notEqual"),
        (">", "greaterThan"),
        ("<", "lessThan"),
        ("=", "equal"),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return (name, rest.to_owned());
        }
    }
    ("equal", text.to_owned())
}

/// Whether a pattern has a wildcard that is not escaped.
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

fn unescape(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A date or a time written in a criterion is stored as its serial number; other values as written.
fn stored_operand(operand: &str) -> String {
    if operand.parse::<f64>().is_ok() {
        return operand.to_owned();
    }
    match unluminous_sheet::input::number_from_text(operand, false) {
        Some(n) if operand.contains(['/', ':']) => format!("{n}"),
        _ => operand.to_owned(),
    }
}

fn criterion_of(text: &str) -> (String, String) {
    let (op, operand) = split(text);
    if op == "notEqual" && operand.is_empty() {
        return (op.to_owned(), " ".to_owned());
    }
    (op.to_owned(), stored_operand(&operand))
}

/// Whether a criterion is a plain equals, which Excel keeps as an item of a list of values.
fn plain_equal(text: &str) -> Option<String> {
    let (op, operand) = split(text);
    (op == "equal" && !operand.is_empty() && !has_wildcard(&operand)).then(|| unescape(&operand))
}

fn text_of(v: &Json) -> Option<String> {
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn two_criteria(first: &str, second: Option<String>, operator: i64) -> FilterCriteria {
    match second {
        None if first == "=" => FilterCriteria::Values { values: Vec::new(), blank: true },
        None => match plain_equal(first) {
            Some(v) => FilterCriteria::Values { values: vec![v], blank: false },
            None => FilterCriteria::Custom { conditions: vec![criterion_of(first)], and: false },
        },
        Some(second) => match (plain_equal(first), plain_equal(&second)) {
            (Some(a), Some(b)) if operator == 2 => {
                FilterCriteria::Values { values: vec![a, b], blank: false }
            }
            _ => FilterCriteria::Custom {
                conditions: vec![criterion_of(first), criterion_of(&second)],
                and: operator == 1,
            },
        },
    }
}

fn value_list(step: &Json) -> FilterCriteria {
    let values: Vec<String> =
        step["criteria1"].as_array().into_iter().flatten().filter_map(text_of).collect();
    let blank = values.iter().any(|v| v.is_empty() || v == "=");
    let values: Vec<String> = values.into_iter().filter(|v| !v.is_empty() && v != "=").collect();
    let pairs = step["criteria2"].as_array().cloned().unwrap_or_default();
    let groups: Vec<DateGroup> =
        pairs.chunks(2).filter_map(|p| date_group(p[0].as_i64()?, p.get(1)?.as_str()?)).collect();
    if groups.is_empty() {
        FilterCriteria::Values { values, blank }
    } else {
        FilterCriteria::Dates { values, groups, blank }
    }
}

/// One year, month, day item of the date tree from a grouping code and a date.
fn date_group(grouping: i64, date: &str) -> Option<DateGroup> {
    let n = unluminous_sheet::input::number_from_text(date, false)?;
    let (y, m, d) = unluminous_sheet::dates::date(n, false)?;
    let (month, day, kind) = match grouping {
        0 => (None, None, DateGrouping::Year),
        1 => (Some(m), None, DateGrouping::Month),
        _ => (Some(m), Some(d), DateGrouping::Day),
    };
    Some(DateGroup {
        year: y as i32,
        month,
        day,
        hour: None,
        minute: None,
        second: None,
        grouping: kind,
    })
}

/// The criteria Excel's AutoFilter method makes of its arguments.
fn criteria_of(s: &mut Session, step: &Json) -> Result<FilterCriteria, String> {
    let operator = step["operator"].as_i64().unwrap_or(0);
    let first = text_of(&step["criteria1"]);
    match operator {
        0..=2 => {
            let first = first.ok_or("no criteria")?;
            Ok(two_criteria(&first, text_of(&step["criteria2"]), operator))
        }
        3..=6 => Ok(FilterCriteria::Top10 {
            top: operator == 3 || operator == 5,
            percent: operator >= 5,
            value: first.unwrap_or_default().parse().map_err(|_| "bad number".to_owned())?,
            threshold: None,
        }),
        7 => Ok(value_list(step)),
        8 | 9 => {
            let cell = operator == 8;
            let color = step["criteria1"]
                .as_str()
                .map(|hex| Color::Rgb(0xFF00_0000 | u32::from_str_radix(hex, 16).unwrap()))
                .unwrap_or(Color::Auto);
            let dxf = if cell {
                s.add_differential_format(Dxf { fill: Some(color), ..Dxf::default() })
            } else {
                s.add_differential_format(Dxf { font_color: Some(color), ..Dxf::default() })
            };
            Ok(FilterCriteria::Color { cell, dxf })
        }
        10 => {
            let id = step["iconId"].as_i64().unwrap() as i32 - 1;
            Ok(FilterCriteria::Icon { set: "3Arrows".to_owned(), id })
        }
        12 => Ok(FilterCriteria::Color {
            cell: true,
            dxf: s.add_differential_format(Dxf { fill: Some(Color::Auto), ..Dxf::default() }),
        }),
        13 => Ok(FilterCriteria::Color {
            cell: false,
            dxf: s.add_differential_format(Dxf { font_color: Some(Color::Auto), ..Dxf::default() }),
        }),
        14 => Ok(FilterCriteria::Icon { set: "3Arrows".to_owned(), id: -1 }),
        _ => Ok(FilterCriteria::Dynamic {
            kind: dynamic_name(step["criteria1"].as_i64().unwrap()),
            value: None,
            max: None,
        }),
    }
}

pub fn run_filter(s: &mut Session, sheet: usize, step: &Json, ctx: &Context) -> Result<(), String> {
    let known = match step["table"].as_str() {
        Some(name) => Some(s.book.table(name).ok_or("no such table")?.range),
        None => s.book.sheets[sheet].filter.as_ref().map(|f| f.range),
    };
    let field = step["field"].as_u64().unwrap() as u32;
    if let Some(table) = step["table"].as_str() {
        let criteria = if step["criteria1"].is_null() && step["operator"].is_null() {
            None
        } else {
            Some(criteria_of(s, step)?)
        };
        return s.table_filter_column(table, field - 1, criteria);
    }
    if step["criteria1"].is_null() && step["operator"].is_null() {
        let range = known.ok_or("no filter")?;
        return s.filter_column(sheet, range.first.col + field - 1, None);
    }
    let criteria = criteria_of(s, step)?;
    let range = match known {
        Some(r) => r,
        None => {
            let r = range_of(step["range"].as_str().unwrap());
            s.set_filter(sheet, Some(r))?;
            r
        }
    };
    let col = range.first.col + field - 1;
    match &ctx.lights {
        Some(lights) => s.filter_column_with(sheet, col, Some(criteria), lights),
        None => s.filter_column(sheet, col, Some(criteria)),
    }
}
