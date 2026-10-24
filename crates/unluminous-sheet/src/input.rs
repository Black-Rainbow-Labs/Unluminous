//! What a person types into a cell, read the way Excel reads it.
//!
//! `12` is a number, `50%` is 0.5 shown as a percent, `$1,200` is 1200 shown as currency, `10/23/2026` is
//! a date, `2:30 PM` is a time, `TRUE` is a logical, `#N/A` is an error, a leading apostrophe makes the
//! rest text, and anything else is text. The same reading turns text into a number inside a formula:
//! `="50%"*2` is 1, and it is the one reading `DATEVALUE`, `TIMEVALUE`, `VALUE`, `NUMBERVALUE` and the
//! criteria of `COUNTIF` use. The conventions are the United States English ones Excel uses on this
//! machine, and every rule here was measured against Excel (`tools/excel-oracle/typed.ps1`).
//!
//! A date written without a year takes the year of the calculation's clock, which [`use_clock`] sets for
//! the thread that is calculating.

use std::cell::Cell;

use crate::dates;
use crate::value::{parse_literal, ErrorKind, Value};

/// A typed entry: the value, and the number format Excel would give the cell for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Typed {
    pub value: Value,
    pub format: Option<&'static str>,
}

/// A text that reads as a number: its value and the format it implies (`None` is General).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Found {
    value: f64,
    format: Option<&'static str>,
}

/// The largest number a typed entry may be; above it the text stays text.
const LARGEST: f64 = 9.999_999_999_999_99e307;
/// The smallest nonzero magnitude a typed entry may have; below it the text stays text.
const SMALLEST: f64 = 2.225_073_858_507_201_4e-308;
/// The most hours a typed time may have.
const MOST_HOURS: f64 = 9999.0;

const DOLLAR_WHOLE: &str = "$#,##0_);[Red]($#,##0)";
const DOLLAR_CENTS: &str = "$#,##0.00_);[Red]($#,##0.00)";

thread_local! {
    static YEAR: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Tell this thread what the date is, from a serial: the year a date typed without one takes.
pub fn use_clock(now: f64) {
    if let Some((year, _, _)) = dates::date(now, false) {
        YEAR.with(|y| y.set(Some(year)));
    }
}

/// The year of the calculation's clock, or of the system clock when none has been set.
fn current_year() -> i64 {
    YEAR.with(Cell::get).unwrap_or_else(|| {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        dates::date(dates::from_unix(seconds, 0, false), false).map_or(2026, |(y, _, _)| y)
    })
}

/// Read what was typed into a cell that is not a formula.
pub fn read(text: &str, date1904: bool, this_year: i64) -> Typed {
    if let Some(rest) = text.strip_prefix('\'') {
        return Typed { value: Value::text(rest), format: None };
    }
    if text.is_empty() {
        return Typed { value: Value::Empty, format: None };
    }
    if text.eq_ignore_ascii_case("TRUE") {
        return Typed { value: Value::Bool(true), format: None };
    }
    if text.eq_ignore_ascii_case("FALSE") {
        return Typed { value: Value::Bool(false), format: None };
    }
    if let Some(kind) = typed_error(text) {
        return Typed { value: Value::Error(kind), format: None };
    }
    match number(text, date1904, this_year, true) {
        Some(found) => Typed { value: Value::Number(found.value), format: found.format },
        None => Typed { value: Value::text(text), format: None },
    }
}

/// The errors that can be typed into a cell. `#SPILL!` and `#CALC!` stay text, as they do in Excel.
fn typed_error(text: &str) -> Option<ErrorKind> {
    let upper = text.to_ascii_uppercase();
    ErrorKind::ALL
        .into_iter()
        .filter(|kind| !matches!(kind, ErrorKind::Spill | ErrorKind::Calc | ErrorKind::GettingData))
        .find(|kind| kind.text() == upper)
}

/// Text as a number, the way arithmetic on text does it: `"1,234"`, `"$5"`, `"50%"`, a date or a time.
pub fn number_from_text(text: &str, date1904: bool) -> Option<f64> {
    number(text, date1904, current_year(), false).map(|found| found.value)
}

/// Text as a date or a time and nothing else, which is what `DATEVALUE` and `TIMEVALUE` read: a plain
/// number is not a date.
pub fn date_time_from_text(text: &str, date1904: bool) -> Option<f64> {
    date_time_text(text, date1904, current_year(), false).map(|found| found.value)
}

/// `typed` is for what a person types into a cell, which differs from text read in a formula in one thing: a
/// date or a time with a space before it is left as text.
fn number(text: &str, date1904: bool, this_year: i64, typed: bool) -> Option<Found> {
    let trimmed = text.trim_matches(' ');
    if trimmed.is_empty() {
        return None;
    }
    plain_number(trimmed).or_else(|| date_time_text(text, date1904, this_year, typed))
}

/// A date or time. Spaces after it are ignored, and spaces before it too unless it was typed.
fn date_time_text(text: &str, date1904: bool, this_year: i64, typed: bool) -> Option<Found> {
    if typed && text.starts_with(' ') {
        return None;
    }
    date_time(text.trim_matches(' '), date1904, this_year)
}

// ---------------------------------------------------------------------------------------------------
// Numbers.

/// A number with an optional sign, currency, thousands separators, percent, exponent, a fraction or
/// parentheses around it.
fn plain_number(text: &str) -> Option<Found> {
    if let Some(inner) = text.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        let found = decorated(inner.trim_matches(' '), false)?;
        return Some(Found { value: -found.value, ..found });
    }
    decorated(text, true)
}

/// What stands round the digits: a sign, a currency symbol, a percent sign.
#[derive(Default)]
struct Marks {
    negative: bool,
    signed: bool,
    currency: Option<char>,
    percent: bool,
    parenthesised: bool,
}

/// The digits with the marks round them removed, then the value and format they imply.
fn decorated(text: &str, sign_allowed: bool) -> Option<Found> {
    let mut marks = Marks::default();
    let mut rest = text;
    loop {
        rest = rest.trim_start_matches(' ');
        let Some(first) = rest.chars().next() else { return None };
        match first {
            '+' | '-' if sign_allowed && !marks.signed => {
                marks.signed = true;
                marks.negative = first == '-';
            }
            '$' | '€' if marks.currency.is_none() => marks.currency = Some(first),
            '%' if !marks.percent => marks.percent = true,
            _ => break,
        }
        rest = &rest[first.len_utf8()..];
    }
    rest = strip_suffixes(rest, &mut marks)?;
    if marks.currency.is_some() && !marks.signed {
        if let Some(inner) = rest.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
            marks.parenthesised = true;
            rest = inner.trim_matches(' ');
        }
    }
    if marks.percent && marks.currency.is_some() {
        return None;
    }
    let core = core_number(rest)?;
    finish(core, &marks)
}

/// Take a percent sign or a euro sign from the end of the digits.
fn strip_suffixes<'a>(text: &'a str, marks: &mut Marks) -> Option<&'a str> {
    let mut rest = text.trim_end_matches(' ');
    loop {
        if let Some(before) = rest.strip_suffix('%') {
            if marks.percent {
                return None;
            }
            marks.percent = true;
            rest = before.trim_end_matches(' ');
        } else if let Some(before) = rest.strip_suffix('€') {
            if marks.currency.is_some() {
                return None;
            }
            marks.currency = Some('€');
            rest = before.trim_end_matches(' ');
        } else {
            return Some(rest);
        }
    }
}

/// Apply the marks to the digits that were read, and choose the format.
fn finish(core: Core, marks: &Marks) -> Option<Found> {
    let mut value = core.value;
    if marks.percent {
        value /= 100.0;
    }
    if marks.negative != marks.parenthesised {
        value = -value;
    }
    if !in_range(value, core.mantissa_is_zero) {
        return None;
    }
    let format = if core.exponent {
        Some("0.00E+00")
    } else if marks.percent {
        Some(if core.decimals || core.fraction.is_some() { "0.00%" } else { "0%" })
    } else if let Some(bottom) = core.fraction {
        Some(if bottom >= 10 { "# ??/??" } else { "# ?/?" })
    } else if marks.currency == Some('$') {
        Some(if core.decimals { DOLLAR_CENTS } else { DOLLAR_WHOLE })
    } else if marks.currency.is_some() {
        None
    } else if core.grouped {
        Some(if core.decimals { "#,##0.00" } else { "#,##0" })
    } else {
        None
    };
    Some(Found { value, format })
}

/// Whether a typed number is one Excel keeps as a number rather than as text.
fn in_range(value: f64, mantissa_is_zero: bool) -> bool {
    if !value.is_finite() || value.abs() > LARGEST {
        return false;
    }
    mantissa_is_zero || value.abs() >= SMALLEST
}

/// Digits as they were typed.
struct Core {
    value: f64,
    grouped: bool,
    decimals: bool,
    exponent: bool,
    mantissa_is_zero: bool,
    /// The denominator, when the digits were a whole number and a fraction.
    fraction: Option<u32>,
}

/// Digits: `1,234.5e3`, or a whole number and a fraction, `3 1/4`.
fn core_number(text: &str) -> Option<Core> {
    if text.contains(' ') {
        return mixed_fraction(text);
    }
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let (whole, decimals) = match mantissa.split_once('.') {
        Some((w, d)) => (w, Some(d)),
        None => (mantissa, None),
    };
    let grouped = whole.contains(',');
    if grouped && !groups_are_thousands(whole) {
        return None;
    }
    let digits: String = whole.chars().filter(|c| *c != ',').collect();
    if digits.is_empty() && decimals.is_none_or(str::is_empty) {
        return None;
    }
    let all_digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !all_digits(&digits) || decimals.is_some_and(|d| !all_digits(d)) {
        return None;
    }
    if let Some(e) = exponent {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        if e.is_empty() || !all_digits(e) {
            return None;
        }
    }
    let literal = format!(
        "{}{}{}",
        if digits.is_empty() { "0" } else { &digits },
        decimals.map(|d| format!(".{d}")).unwrap_or_default(),
        exponent.map(|e| format!("e{e}")).unwrap_or_default()
    );
    let mantissa_is_zero = digits.chars().chain(decimals.unwrap_or("").chars()).all(|c| c == '0');
    Some(Core {
        value: parse_literal(&literal)?,
        grouped,
        decimals: decimals.is_some(),
        exponent: exponent.is_some(),
        mantissa_is_zero,
        fraction: None,
    })
}

/// Thousands separators sit every three digits: one to three digits, then groups of exactly three.
fn groups_are_thousands(whole: &str) -> bool {
    let groups: Vec<&str> = whole.split(',').collect();
    !groups[0].is_empty() && groups[0].len() <= 3 && groups[1..].iter().all(|g| g.len() == 3)
}

/// `1 1/2`: a whole number, one space and a fraction. A bare `3/4` is a date, so the whole is needed.
fn mixed_fraction(text: &str) -> Option<Core> {
    let (whole, fraction) = text.split_once(' ')?;
    let (top, bottom) = fraction.split_once('/')?;
    let all_digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !all_digits(whole) || !all_digits(top) || !all_digits(bottom) {
        return None;
    }
    let bottom: u32 = bottom.parse().ok().filter(|b| (1..=32_767).contains(b))?;
    let value = whole.parse::<f64>().ok()? + top.parse::<f64>().ok()? / f64::from(bottom);
    Some(Core {
        value,
        grouped: false,
        decimals: false,
        exponent: false,
        mantissa_is_zero: value == 0.0,
        fraction: Some(bottom),
    })
}

// ---------------------------------------------------------------------------------------------------
// Dates and times.

/// A date, a time, or both.
fn date_time(text: &str, date1904: bool, this_year: i64) -> Option<Found> {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let (date_words, clock) = split_clock(&words)?;
    let date = if date_words.is_empty() {
        None
    } else {
        Some(parse_date(&date_words.join(" "), this_year)?)
    };
    let date = match date {
        Some((y, m, d, format)) => Some((dates::serial(y, m, d, date1904)?, format)),
        None => None,
    };
    match (date, clock) {
        (None, None) => None,
        (None, Some(clock)) => {
            Some(Found { value: clock.fraction_of_day(), format: clock.format() })
        }
        (Some((serial, format)), None) => Some(Found { value: serial, format: Some(format) }),
        (Some((serial, _)), Some(clock)) => Some(Found {
            value: serial + clock.fraction_of_day(),
            format: clock.with_date_format(),
        }),
    }
}

/// The words that are not the time, and the time. The time is the one word holding a colon, with the
/// `AM` or `PM` after it, or a whole number followed by `AM` or `PM`.
fn split_clock<'a>(words: &[&'a str]) -> Option<(Vec<&'a str>, Option<Clock>)> {
    let colons: Vec<usize> =
        words.iter().enumerate().filter(|(_, w)| w.contains(':')).map(|(i, _)| i).collect();
    let (at, length) = match colons.as_slice() {
        [] => match words.iter().position(|w| meridiem_of(w).is_some()) {
            Some(i) if i > 0 && words[i - 1].chars().all(|c| c.is_ascii_digit()) => (i - 1, 2),
            Some(_) => return None,
            None => return Some((words.to_vec(), None)),
        },
        [i] => (*i, if words.get(i + 1).is_some_and(|w| meridiem_of(w).is_some()) { 2 } else { 1 }),
        _ => return None,
    };
    let clock = parse_clock(words[at], words.get(at + 1).filter(|_| length == 2).copied())?;
    let mut rest = words[..at].to_vec();
    rest.extend_from_slice(&words[at + length..]);
    Some((rest, Some(clock)))
}

/// `Some(true)` for PM and `Some(false)` for AM, in the four spellings Excel reads.
fn meridiem_of(word: &str) -> Option<bool> {
    match word.to_ascii_lowercase().as_str() {
        "am" | "a" => Some(false),
        "pm" | "p" => Some(true),
        _ => None,
    }
}

/// A time of day as typed.
#[derive(Debug, Clone, Copy)]
struct Clock {
    hours: f64,
    minutes: f64,
    seconds: f64,
    parts: u8,
    fraction: bool,
    meridiem: bool,
}

impl Clock {
    /// The time as a fraction of a day, to the millisecond.
    fn fraction_of_day(&self) -> f64 {
        let total = self.hours * 3600.0 + self.minutes * 60.0 + self.seconds;
        ((total * 1000.0).round() / 1000.0) / 86_400.0
    }

    /// Whether a part is past what a clock shows: 60 minutes or seconds, 24 hours.
    fn runs_over(&self) -> bool {
        self.minutes >= 60.0 || self.seconds >= 60.0
    }

    /// The format Excel gives a time typed on its own.
    fn format(&self) -> Option<&'static str> {
        if self.fraction {
            return Some("mm:ss.0");
        }
        if self.runs_over() {
            return None;
        }
        Some(match (self.meridiem, self.parts, self.hours >= 24.0) {
            (true, 3, _) => "h:mm:ss AM/PM",
            (true, _, _) => "h:mm AM/PM",
            (false, _, true) => "[h]:mm:ss",
            (false, 3, false) => "h:mm:ss",
            (false, _, false) => "h:mm",
        })
    }

    /// The format Excel gives a date and a time typed together.
    fn with_date_format(&self) -> Option<&'static str> {
        if self.fraction {
            Some("mm:ss.0")
        } else if self.runs_over() || self.hours >= 24.0 {
            None
        } else {
            Some("m/d/yyyy h:mm")
        }
    }
}

/// `14:30`, `2:30 pm`, `14:30:15.5`, `2 pm`, `1:`, `25:00`. The first word holds the clock and the
/// second, when there is one, is `AM` or `PM`.
fn parse_clock(clock: &str, meridiem: Option<&str>) -> Option<Clock> {
    let pm = match meridiem {
        Some(word) => Some(meridiem_of(word)?),
        None => None,
    };
    let mut parts: Vec<&str> = clock.split(':').collect();
    let trailing_colon = parts.len() >= 2 && parts.last().is_some_and(|p| p.is_empty());
    if trailing_colon {
        parts.pop();
    }
    if parts.len() == 1 && trailing_colon {
        parts.push("0");
    }
    let whole = |s: &str| -> Option<f64> {
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).then(|| s.parse().ok()).flatten()
    };
    let last = parts.last().copied().unwrap_or("");
    let fraction = last.contains('.');
    let (hours, minutes, seconds) = match parts.len() {
        1 => (whole(parts[0])?, 0.0, 0.0),
        2 if fraction => (0.0, whole(parts[0])?, seconds_of(last)?),
        2 => (whole(parts[0])?, whole(parts[1])?, 0.0),
        3 => (whole(parts[0])?, whole(parts[1])?, seconds_of(last)?),
        _ => return None,
    };
    if parts.len() == 1 && pm.is_none() || hours > MOST_HOURS {
        return None;
    }
    if (minutes >= 60.0 || seconds >= 60.0) && (hours >= 24.0 || pm.is_some()) {
        return None;
    }
    let hours = match pm {
        Some(pm) => {
            if hours > 12.0 {
                return None;
            }
            hours % 12.0 + if pm { 12.0 } else { 0.0 }
        }
        None => hours,
    };
    Some(Clock {
        hours,
        minutes,
        seconds,
        parts: if parts.len() == 1 { 2 } else { parts.len() as u8 },
        fraction,
        meridiem: pm.is_some(),
    })
}

/// Seconds as digits with an optional fraction.
fn seconds_of(text: &str) -> Option<f64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) {
        return None;
    }
    text.parse().ok()
}

//// A piece of a date: a run of digits or a run of letters.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Field<'a> {
    Number(&'a str),
    Word(&'a str),
}

/// What stands between two fields: nothing, a space, a dash, a slash, or a comma and a space.
type Gap = Option<char>;

/// The fields of a date and the gaps between them, so `Jan2`, `Jan 2` and `Jan-2` read alike. Runs of
/// spaces are one space, a comma has to be followed by a space, and any other character, or a gap at the
/// start or the end, makes it not a date.
fn fields(text: &str) -> Option<(Vec<Field<'_>>, Vec<Gap>)> {
    let mut found = Vec::new();
    let mut gaps: Vec<Gap> = Vec::new();
    let mut between = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if !c.is_ascii_alphanumeric() {
            if !matches!(c, ' ' | '/' | '-' | ',') {
                return None;
            }
            between.push(c);
            continue;
        }
        let digits = c.is_ascii_digit();
        let mut end = start + c.len_utf8();
        while let Some(&(at, next)) = chars.peek() {
            if !next.is_ascii_alphanumeric() || next.is_ascii_digit() != digits {
                break;
            }
            end = at + next.len_utf8();
            chars.next();
        }
        if !found.is_empty() {
            gaps.push(gap_of(&between)?);
        } else if !between.is_empty() {
            return None;
        }
        between.clear();
        let run = &text[start..end];
        found.push(if digits { Field::Number(run) } else { Field::Word(run) });
    }
    between.is_empty().then_some((found, gaps))
}

fn gap_of(between: &str) -> Option<Gap> {
    match between {
        "" => Some(None),
        s if s.chars().all(|c| c == ' ') => Some(Some(' ')),
        "-" => Some(Some('-')),
        "/" => Some(Some('/')),
        s => {
            // A comma, with spaces before it or not and at least one after.
            let after = s.trim_start_matches(' ').strip_prefix(',')?;
            (after.starts_with(' ') && after.chars().all(|c| c == ' ')).then_some(Some(','))
        }
    }
}

/// The month a word names: the first three letters or more of its name, so `Jan`, `Sept` and `January`.
fn month_named(word: &str) -> Option<i64> {
    let lower = word.to_ascii_lowercase();
    if lower.len() < 3 {
        return None;
    }
    dates::MONTHS
        .iter()
        .position(|m| m.to_ascii_lowercase().starts_with(&lower))
        .map(|i| i as i64 + 1)
}

/// A date as year, month, day and the format Excel gives it.
type Day = (i64, i64, i64, &'static str);

/// The date at the start of a text, in every form Excel reads.
fn parse_date(text: &str, this_year: i64) -> Option<Day> {
    use Field::{Number as N, Word as W};
    let (found, gaps) = fields(text)?;
    let plain = |g: Gap| matches!(g, None | Some(' ' | '-' | '/'));
    match (found.as_slice(), gaps.as_slice()) {
        ([N(a), N(b), N(c)], [Some(x), Some(y)]) if x == y && matches!(x, '/' | '-') => {
            numeric_date(a, b, c)
        }
        ([N(a), N(b)], [Some('/' | '-')]) => month_and_number(number_of(a)?, b, this_year, true),
        ([W(m), N(n)], [g]) if plain(*g) => month_and_number(month_named(m)?, n, this_year, true),
        ([N(n), W(m)], [g]) if plain(*g) => month_and_number(month_named(m)?, n, this_year, false),
        ([W(m), N(d), N(y)], [None | Some(' '), Some(',')]) => {
            month_day_year(month_named(m)?, d, y)
        }
        ([N(d), W(m), N(y)], [g, h])
            if plain(*g) && plain(*h) && (g.is_none() || h.is_none() || g == h) =>
        {
            month_day_year(month_named(m)?, d, y)
        }
        _ => None,
    }
}

fn number_of(digits: &str) -> Option<i64> {
    digits.parse().ok()
}

/// Whether a day exists, counting 29 February 1900 as Excel does.
fn day_exists(year: i64, month: i64, day: i64) -> bool {
    if !(1..=12).contains(&month) || day < 1 {
        return false;
    }
    let last = if year == 1900 && month == 2 {
        29
    } else {
        i64::from(dates::days_in_month(year, month as u32))
    };
    day <= last
}

/// A year as typed: one or two digits with the century chosen round 1930, or four digits from 1900.
fn year_of(digits: &str) -> Option<i64> {
    let n = number_of(digits)?;
    match digits.len() {
        1 | 2 => Some(if n < 30 { 2000 + n } else { 1900 + n }),
        4 if (1900..=9999).contains(&n) => Some(n),
        _ => None,
    }
}

/// `1/2/2026`, `1-2-26` or `2026-1-2`.
fn numeric_date(a: &str, b: &str, c: &str) -> Option<Day> {
    let (year, month, day) = if a.len() == 4 {
        if b.len() > 2 || c.len() > 2 {
            return None;
        }
        (year_of(a)?, number_of(b)?, number_of(c)?)
    } else {
        (year_of(c)?, number_of(a)?, number_of(b)?)
    };
    day_exists(year, month, day).then_some((year, month, day, "m/d/yyyy"))
}

/// A month and a number: the day when the month has it this year, otherwise the year, so `Jan 5` is
/// the fifth and `Jan 32` is January 1932. A number written before the month is only ever a day.
fn month_and_number(month: i64, digits: &str, this_year: i64, year_allowed: bool) -> Option<Day> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let n = number_of(digits)?;
    if digits.len() <= 2 && day_exists(this_year, month, n) {
        return Some((this_year, month, n, "d-mmm"));
    }
    if !year_allowed {
        return None;
    }
    Some((year_of(digits)?, month, 1, "mmm-yy"))
}

/// `Jan 2, 2026` and `2-Jan-26`.
fn month_day_year(month: i64, day: &str, year: &str) -> Option<Day> {
    let (year, day) = (year_of(year)?, number_of(day)?);
    day_exists(year, month, day).then_some((year, month, day, "d-mmm-yy"))
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
        assert_eq!(n("1,000."), (1000.0, Some("#,##0.00")));
        assert_eq!(n("50%"), (0.5, Some("0%")));
        assert_eq!(n("$3"), (3.0, Some(DOLLAR_WHOLE)));
        assert_eq!(n("(100)"), (-100.0, None));
        assert_eq!(n("1e3"), (1000.0, Some("0.00E+00")));
        assert_eq!(n("1 1/2"), (1.5, Some("# ?/?")));
        assert_eq!(n(".5"), (0.5, None));
        assert_eq!(n("€5"), (5.0, None));
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
        assert_eq!(n("1/2"), (46024.0, Some("d-mmm")));
        assert_eq!(n("25:00"), (25.0 / 24.0, Some("[h]:mm:ss")));
        assert_eq!(n("2/29/1900"), (60.0, Some("m/d/yyyy")));
    }

    #[test]
    fn everything_else_is_text() {
        for text in [
            "abc",
            "1,23",
            "12abc",
            "1.2.3",
            "13/45/2026",
            "--5",
            "$",
            "1e308",
            "true ",
            "January 2 2026",
            "12/31/1899",
        ] {
            assert!(matches!(read(text, false, 2026).value, Value::Text(_)), "{text}");
        }
        assert_eq!(read("'123", false, 2026).value, Value::text("123"));
        assert_eq!(read("true", false, 2026).value, Value::Bool(true));
        assert_eq!(read("#n/a", false, 2026).value, Value::Error(ErrorKind::NA));
        assert!(matches!(read("#SPILL!", false, 2026).value, Value::Text(_)));
    }
}
