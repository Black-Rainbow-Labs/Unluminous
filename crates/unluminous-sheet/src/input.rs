//! What a person types into a cell, read the way Excel reads it.
//!
//! `12` is a number, `50%` is 0.5 shown as a percent, `$1,200` is 1200 shown as currency, `10/23/2026` is
//! a date, `2:30 PM` is a time, `TRUE` is a logical, `#N/A` is an error, a leading apostrophe makes the
//! rest text, and anything else is text. The same reading turns text into a number inside a formula:
//! `="50%"*2` is 1. The conventions are the United States English ones Excel uses on this machine.

use crate::dates;
use crate::value::{parse_literal, ErrorKind, Value};

/// A typed entry: the value, and the number format Excel would give the cell for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Typed {
    pub value: Value,
    pub format: Option<&'static str>,
}

/// Read what was typed into a cell that is not a formula.
pub fn read(text: &str, date1904: bool, this_year: i64) -> Typed {
    if let Some(rest) = text.strip_prefix('\'') {
        return Typed { value: Value::text(rest), format: None };
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Typed {
            value: if text.is_empty() { Value::Empty } else { Value::text(text) },
            format: None,
        };
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "TRUE" => return Typed { value: Value::Bool(true), format: None },
        "FALSE" => return Typed { value: Value::Bool(false), format: None },
        _ => {}
    }
    if let Some(kind) = ErrorKind::parse(trimmed) {
        return Typed { value: Value::Error(kind), format: None };
    }
    match number(trimmed, date1904, this_year) {
        Some((n, format)) => Typed { value: Value::Number(n), format },
        None => Typed { value: Value::text(text), format: None },
    }
}

/// Text as a number, the way arithmetic on text does it: `"1,234"`, `"$5"`, `"50%"`, a date or a time.
pub fn number_from_text(text: &str, date1904: bool) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    number(trimmed, date1904, 2000).map(|(n, _)| n)
}

fn number(text: &str, date1904: bool, this_year: i64) -> Option<(f64, Option<&'static str>)> {
    if let Some(found) = plain_number(text) {
        return Some(found);
    }
    if let Some(found) = fraction(text) {
        return Some(found);
    }
    if let Some(found) = date_time(text, date1904, this_year) {
        return Some(found);
    }
    None
}

/// A number with an optional sign, currency, thousands separators, percent, exponent or parentheses.
fn plain_number(text: &str) -> Option<(f64, Option<&'static str>)> {
    let mut s = text;
    let mut negative = false;
    let mut parens = false;
    if let Some(inner) = s.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        parens = true;
        negative = true;
        s = inner.trim();
    }
    let mut sign = |s: &mut &str| {
        if let Some(rest) = s.strip_prefix('-') {
            negative = !negative;
            *s = rest.trim_start();
            true
        } else if let Some(rest) = s.strip_prefix('+') {
            *s = rest.trim_start();
            true
        } else {
            false
        }
    };
    sign(&mut s);
    let mut currency = false;
    if let Some(rest) = s.strip_prefix('$') {
        currency = true;
        s = rest.trim_start();
        sign(&mut s);
    }
    let mut percent = false;
    if let Some(rest) = s.strip_suffix('%') {
        percent = true;
        s = rest.trim_end();
    }
    if s.is_empty() || parens && percent {
        return None;
    }
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(at) => (&s[..at], Some(&s[at + 1..])),
        None => (s, None),
    };
    let (whole, decimals) = match mantissa.split_once('.') {
        Some((w, d)) => (w, Some(d)),
        None => (mantissa, None),
    };
    // Thousands separators must sit every three digits.
    let grouped = whole.contains(',');
    if grouped {
        let groups: Vec<&str> = whole.split(',').collect();
        if groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3) {
            return None;
        }
    }
    let digits: String = whole.chars().filter(|c| *c != ',').collect();
    if digits.is_empty() && decimals.is_none_or(str::is_empty) {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit())
        || decimals.is_some_and(|d| !d.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    if let Some(e) = exponent {
        let e = e.strip_prefix('+').unwrap_or(e);
        let e = e.strip_prefix('-').unwrap_or(e);
        if e.is_empty() || !e.chars().all(|c| c.is_ascii_digit()) || grouped || currency {
            return None;
        }
    }
    let literal = format!(
        "{}{}{}",
        if digits.is_empty() { "0" } else { &digits },
        decimals.map(|d| format!(".{d}")).unwrap_or_default(),
        exponent.map(|e| format!("e{e}")).unwrap_or_default()
    );
    let mut n = parse_literal(&literal)?;
    if negative {
        n = -n;
    }
    let has_decimals = decimals.is_some_and(|d| !d.is_empty());
    let format = if percent {
        n /= 100.0;
        Some(if has_decimals { "0.00%" } else { "0%" })
    } else if currency {
        Some(if has_decimals {
            "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)"
        } else {
            "\"$\"#,##0_);\\(\"$\"#,##0\\)"
        })
    } else if exponent.is_some() {
        Some("0.00E+00")
    } else if grouped {
        Some(if has_decimals { "#,##0.00" } else { "#,##0" })
    } else {
        None
    };
    Some((n, format))
}

/// `1 1/2`, `3/4` is a date in Excel (March 4th), so only a whole number and a fraction reads as one.
fn fraction(text: &str) -> Option<(f64, Option<&'static str>)> {
    let (whole, frac) = text.split_once(' ')?;
    let (top, bottom) = frac.trim().split_once('/')?;
    let negative = whole.starts_with('-');
    let whole: f64 = whole.trim_start_matches('-').parse().ok()?;
    let top: f64 = top.parse().ok()?;
    let bottom: f64 = bottom.parse().ok()?;
    if bottom == 0.0 || top >= bottom || whole.fract() != 0.0 {
        return None;
    }
    let value = whole + top / bottom;
    Some((
        if negative { -value } else { value },
        Some(if bottom >= 10.0 { "# ??/??" } else { "# ?/?" }),
    ))
}

fn month_number(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    if lower.len() < 3 {
        return None;
    }
    dates::MONTHS
        .iter()
        .position(|m| m.to_ascii_lowercase().starts_with(&lower))
        .map(|i| i as u32 + 1)
}

/// A date, a time, or both.
fn date_time(text: &str, date1904: bool, this_year: i64) -> Option<(f64, Option<&'static str>)> {
    // A time on its own or after a date: the last token holds a colon, or AM/PM follows it.
    let lower = text.to_ascii_lowercase();
    let (date_part, time_part) = split_time(&lower);
    let time = match time_part {
        Some(t) => Some(parse_time(t)?),
        None => None,
    };
    let date = match date_part.trim() {
        "" => None,
        d => Some(parse_date(d, date1904, this_year)?),
    };
    match (date, time) {
        (None, None) => None,
        (None, Some((t, format))) => Some((t, Some(format))),
        (Some((d, format)), None) => Some((d, Some(format))),
        (Some((d, _)), Some((t, _))) => Some((d + t, Some("m/d/yyyy h:mm"))),
    }
}

fn split_time(text: &str) -> (&str, Option<&str>) {
    // Find where the time begins: the first whitespace separated token containing ':', or a number
    // followed by am or pm.
    let mut start = None;
    let mut at = 0;
    for token in text.split(' ') {
        if token.contains(':')
            || token.ends_with("am")
            || token.ends_with("pm")
            || token == "am"
            || token == "pm"
            || token == "a"
            || token == "p"
        {
            if token.contains(':') || token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                start = Some(at);
                break;
            }
            if (token == "am" || token == "pm" || token == "a" || token == "p") && at > 0 {
                // `2 pm`: the time starts at the number before it.
                let before = text[..at].trim_end();
                let number_start = before.rfind(' ').map_or(0, |i| i + 1);
                start = Some(number_start);
                break;
            }
        }
        at += token.len() + 1;
    }
    match start {
        Some(s) => (&text[..s], Some(text[s..].trim())),
        None => (text, None),
    }
}

/// `14:30`, `2:30 pm`, `14:30:15.5`, `2 pm`. Answers the fraction of a day and its format.
fn parse_time(text: &str) -> Option<(f64, &'static str)> {
    let text = text.trim();
    let (clock, meridiem) =
        if let Some(rest) = text.strip_suffix("am").or_else(|| text.strip_suffix(" a")) {
            (rest.trim(), Some(false))
        } else if let Some(rest) = text.strip_suffix("pm").or_else(|| text.strip_suffix(" p")) {
            (rest.trim(), Some(true))
        } else {
            (text, None)
        };
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let mut hours: f64 =
        parts[0].parse().ok().filter(|_| parts[0].chars().all(|c| c.is_ascii_digit()))?;
    let minutes: f64 = match parts.get(1) {
        Some(m) if m.chars().all(|c| c.is_ascii_digit()) => m.parse().ok()?,
        Some(_) => return None,
        None => 0.0,
    };
    let seconds: f64 = match parts.get(2) {
        Some(s) => s.parse().ok().filter(|_| s.chars().all(|c| c.is_ascii_digit() || c == '.'))?,
        None => 0.0,
    };
    if parts.len() == 1 && meridiem.is_none() {
        return None;
    }
    if minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }
    if let Some(pm) = meridiem {
        if !(1.0..=12.0).contains(&hours) && hours != 0.0 {
            return None;
        }
        if hours == 12.0 {
            hours = 0.0;
        }
        if pm {
            hours += 12.0;
        }
    }
    let fraction = (hours * 3600.0 + minutes * 60.0 + seconds) / 86_400.0;
    let format = match (meridiem.is_some(), parts.len() == 3) {
        (true, true) => "h:mm:ss AM/PM",
        (true, false) => "h:mm AM/PM",
        (false, true) => "h:mm:ss",
        (false, false) => "h:mm",
    };
    Some((fraction, format))
}

/// `10/23/2026`, `10/23`, `2026-10-23`, `23-Oct-2026`, `Oct 23, 2026`, `October 2026`.
fn parse_date(text: &str, date1904: bool, this_year: i64) -> Option<(f64, &'static str)> {
    let text = text.trim().trim_end_matches(',');
    let year_of = |y: &str| -> Option<i64> {
        let n: i64 = y.parse().ok()?;
        if !y.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(match y.len() {
            1 | 2 => {
                if n < 30 {
                    2000 + n
                } else {
                    1900 + n
                }
            }
            4 => n,
            _ => return None,
        })
    };
    let number = |s: &str| -> Option<i64> {
        s.chars().all(|c| c.is_ascii_digit()).then(|| s.parse().ok()).flatten()
    };
    let make = |y: i64, m: i64, d: i64, format: &'static str| -> Option<(f64, &'static str)> {
        if !(1..=12).contains(&m) || d < 1 || d > dates::days_in_month(y, m as u32) as i64 {
            return None;
        }
        dates::serial(y, m, d, date1904).map(|s| (s, format))
    };
    // ISO: 2026-10-23.
    let dashed: Vec<&str> = text.split('-').collect();
    if dashed.len() == 3 && dashed[0].len() == 4 {
        let (y, m, d) = (number(dashed[0])?, number(dashed[1])?, number(dashed[2])?);
        return make(y, m, d, "m/d/yyyy");
    }
    // Slashes: m/d/y or m/d.
    let slashed: Vec<&str> = text.split('/').collect();
    if slashed.len() == 3 {
        let (m, d, y) = (number(slashed[0])?, number(slashed[1])?, year_of(slashed[2])?);
        if slashed[0].len() == 4 {
            return make(number(slashed[0])?, number(slashed[1])?, number(slashed[2])?, "m/d/yyyy");
        }
        return make(y, m, d, "m/d/yyyy");
    }
    if slashed.len() == 2 {
        let (m, d) = (number(slashed[0])?, number(slashed[1])?);
        if d > 31 && slashed[1].len() == 4 {
            return make(d, m, 1, "mmm-yy");
        }
        return make(this_year, m, d, "d-mmm");
    }
    // 23-Oct-2026, 23-Oct, Oct-26.
    if dashed.len() == 3 {
        let (d, m, y) = (number(dashed[0])?, month_number(dashed[1])? as i64, year_of(dashed[2])?);
        return make(y, m, d, "d-mmm-yy");
    }
    if dashed.len() == 2 {
        if let Some(m) = month_number(dashed[1]) {
            return make(this_year, m as i64, number(dashed[0])?, "d-mmm");
        }
        if let Some(m) = month_number(dashed[0]) {
            return make(year_of(dashed[1])?, m as i64, 1, "mmm-yy");
        }
        return None;
    }
    // Oct 23, 2026 / October 23 2026 / 23 October 2026 / October 2026.
    let words: Vec<&str> = text.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    match words.as_slice() {
        [m, d, y] if month_number(m).is_some() => {
            make(year_of(y)?, month_number(m)? as i64, number(d)?, "d-mmm-yy")
        }
        [d, m, y] if month_number(m).is_some() => {
            make(year_of(y)?, month_number(m)? as i64, number(d)?, "d-mmm-yy")
        }
        [m, d] if month_number(m).is_some() && d.len() <= 2 => {
            make(this_year, month_number(m)? as i64, number(d)?, "d-mmm")
        }
        [m, y] if month_number(m).is_some() && y.len() == 4 => {
            make(year_of(y)?, month_number(m)? as i64, 1, "mmm-yy")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(text: &str) -> (f64, Option<&'static str>) {
        match read(text, false, 2026) {
            Typed { value: Value::Number(n), format } => (n, format),
            other => panic!("{text} read as {other:?}"),
        }
    }

    #[test]
    fn numbers_read_with_the_format_excel_would_give_them() {
        assert_eq!(n("12"), (12.0, None));
        assert_eq!(n("-1.5"), (-1.5, None));
        assert_eq!(n("1,234.5"), (1234.5, Some("#,##0.00")));
        assert_eq!(n("50%"), (0.5, Some("0%")));
        assert_eq!(n("$3"), (3.0, Some("\"$\"#,##0_);\\(\"$\"#,##0\\)")));
        assert_eq!(n("(100)"), (-100.0, None));
        assert_eq!(n("1e3"), (1000.0, Some("0.00E+00")));
        assert_eq!(n("1 1/2"), (1.5, Some("# ?/?")));
        assert_eq!(n(".5"), (0.5, None));
    }

    #[test]
    fn dates_and_times_read_as_serials() {
        assert_eq!(n("10/23/2026"), (46318.0, Some("m/d/yyyy")));
        assert_eq!(n("2026-10-23"), (46318.0, Some("m/d/yyyy")));
        assert_eq!(n("23-Oct-2026"), (46318.0, Some("d-mmm-yy")));
        assert_eq!(n("Oct 23, 2026"), (46318.0, Some("d-mmm-yy")));
        assert_eq!(n("14:30"), (14.5 / 24.0, Some("h:mm")));
        assert_eq!(n("2:30 PM"), (14.5 / 24.0, Some("h:mm AM/PM")));
        assert_eq!(n("12:00 am"), (0.0, Some("h:mm AM/PM")));
        assert_eq!(n("10/23/2026 18:00"), (46318.75, Some("m/d/yyyy h:mm")));
    }

    #[test]
    fn everything_else_is_text() {
        for text in ["abc", "1,23", "12abc", "1.2.3", "13/45/2026", "--5", "$"] {
            assert!(matches!(read(text, false, 2026).value, Value::Text(_)), "{text}");
        }
        assert_eq!(read("'123", false, 2026).value, Value::text("123"));
        assert_eq!(read("true", false, 2026).value, Value::Bool(true));
        assert_eq!(read("#n/a", false, 2026).value, Value::Error(ErrorKind::NA));
    }
}
