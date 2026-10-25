//! How a column or row of seed cells continues when it is filled: Excel's AutoFill rules.
//!
//! The seeds are split into runs of neighbours that go together: numbers, dates, names from one list, or
//! text that ends in a number with the same words in front. Each run continues as a series on its own, and
//! the filled cells cycle through the runs in the order of the seeds, so `1, 2, a` continues `3, 4, a`.

use super::fill::FillKind;
use super::lists::{Case, CustomLists, ListKind};
use super::trend::{growth_fit, linear_fit, round15};
use crate::dates;
use crate::numfmt::Format;
use crate::value::Value;

/// What a fill needs to know besides the seeds.
pub(super) struct Context<'a> {
    pub kind: FillKind,
    pub lists: &'a CustomLists,
    pub date1904: bool,
    /// The fill starts from one cell, which a plain fill repeats if it holds a number.
    pub single_cell: bool,
}

/// How a seed looks to a fill.
#[derive(Debug, Clone)]
pub(super) struct Seed {
    pub value: Value,
    pub formula: bool,
    pub date: Option<DateShape>,
}

/// Whether a number's format shows a date or only a time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DateShape {
    Date,
    TimeOnly,
}

impl DateShape {
    /// The shape a number format code shows, or `None` for a format that shows no date or time.
    pub(super) fn of_format(code: &str) -> Option<DateShape> {
        if !Format::parse(code).is_date() {
            return None;
        }
        let mut bare = String::new();
        let mut quoted = false;
        for c in code.to_ascii_lowercase().chars() {
            match c {
                '"' => quoted = !quoted,
                _ if !quoted => bare.push(c),
                _ => {}
            }
        }
        let has_time = bare.contains(['h', 's']);
        let has_day = bare.contains(['d', 'y']) || (bare.contains('m') && !has_time);
        Some(if has_time && !has_day { DateShape::TimeOnly } else { DateShape::Date })
    }
}

/// A set of neighbouring seeds that continue as one series.
#[derive(Debug, Clone)]
pub(super) struct Piece {
    pub start: usize,
    pub len: usize,
    pub run: Run,
}

/// How one run of seeds continues.
#[derive(Debug, Clone)]
pub(super) enum Run {
    /// Repeat the seed (a formula moves its references).
    Copy,
    /// A straight line: `a + b * index`.
    Linear {
        a: f64,
        b: f64,
    },
    /// An exponential curve: `exp(c + m * index)`.
    Growth {
        c: f64,
        m: f64,
    },
    /// A growth fit that cannot be made: the seeds stay and the rest is zero.
    Zeros,
    Days {
        first: f64,
        step: f64,
    },
    Months {
        first: f64,
        step: i64,
        end_of_month: bool,
    },
    Weekdays {
        first: f64,
        step: i64,
    },
    /// The block of dates repeats, moved on by some months each time round.
    Shift {
        dates: Vec<f64>,
        months: i64,
        end_of_month: bool,
    },
    Names {
        list: Vec<String>,
        first: i64,
        step: i64,
        case: Case,
        weekdays_only: bool,
    },
    Counted(Counted),
}

/// Text with a number in it that counts on: `Item 5`, `1st`, `3 apples`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Counted {
    mode: CountMode,
    prefix: String,
    suffix: String,
    number: i64,
    width: usize,
    step: i64,
    quarter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CountMode {
    Trailing,
    Leading,
    Ordinal,
}

impl Counted {
    /// Read a seed text, or `None` when no number in it counts on.
    fn read(text: &str) -> Option<Counted> {
        let lower = text.to_ascii_lowercase();
        for ending in ["st", "nd", "rd", "th"] {
            if let Some(digits) = lower.strip_suffix(ending) {
                if !digits.is_empty()
                    && digits.len() <= 15
                    && digits.bytes().all(|b| b.is_ascii_digit())
                {
                    let shown_suffix = &text[text.len() - 2..];
                    return Some(Self::make(CountMode::Ordinal, "", shown_suffix, digits));
                }
            }
        }
        let digits_at_start = text.bytes().take_while(u8::is_ascii_digit).count();
        if digits_at_start > 0 && digits_at_start <= 15 && text[digits_at_start..].starts_with(' ')
        {
            return Some(Self::make(
                CountMode::Leading,
                "",
                &text[digits_at_start..],
                &text[..digits_at_start],
            ));
        }
        let digits_at_end = text.bytes().rev().take_while(u8::is_ascii_digit).count();
        if digits_at_end > 0 && digits_at_end <= 15 {
            let split = text.len() - digits_at_end;
            return Some(Self::make(CountMode::Trailing, &text[..split], "", &text[split..]));
        }
        None
    }

    fn make(mode: CountMode, prefix: &str, suffix: &str, digits: &str) -> Counted {
        let word = prefix.trim_end().to_ascii_lowercase();
        let quarter = mode == CountMode::Trailing
            && ["q", "qtr", "quarter"].contains(&word.as_str())
            && matches!(digits, "1" | "2" | "3" | "4");
        Counted {
            mode,
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
            number: digits.parse().unwrap_or(0),
            width: if digits.starts_with('0') { digits.len() } else { 0 },
            step: 1,
            quarter,
        }
    }

    /// Whether two seeds are the same words round a different number.
    fn goes_with(&self, other: &Counted) -> bool {
        self.mode == other.mode
            && self.prefix == other.prefix
            && self.quarter == other.quarter
            && (self.mode != CountMode::Leading || self.suffix == other.suffix)
    }

    /// The text for the number `index` steps from the first seed.
    fn text_at(&self, index: i64) -> String {
        let mut n = self.number + self.step * index;
        if self.quarter {
            n = (n - 1).rem_euclid(4) + 1;
        }
        let n = n.abs();
        let digits =
            if self.width > 1 { format!("{n:0width$}", width = self.width) } else { n.to_string() };
        match self.mode {
            CountMode::Ordinal => format!("{digits}{}", ordinal_suffix(n, &self.suffix)),
            CountMode::Trailing => format!("{}{digits}", self.prefix),
            CountMode::Leading => format!("{digits}{}", self.suffix),
        }
    }
}

/// `st`, `nd`, `rd` or `th` for a number, in the case the seed wrote its suffix.
fn ordinal_suffix(n: i64, seed: &str) -> String {
    let suffix = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    if seed.chars().all(|c| c.is_uppercase()) {
        suffix.to_uppercase()
    } else {
        suffix.to_owned()
    }
}

/// What a seed belongs with.
#[derive(Debug, Clone, PartialEq)]
enum Family {
    Number,
    Date,
    /// Text found in these lists, the newest list first.
    Name(Vec<usize>),
    Counted(Counted),
    Other,
}

fn family_of(seed: &Seed, ctx: &Context) -> Family {
    if seed.formula {
        return Family::Other;
    }
    let family = match &seed.value {
        Value::Number(_) if seed.date == Some(DateShape::TimeOnly) && ctx.kind.is_by_unit() => {
            Family::Other
        }
        Value::Number(_) if seed.date.is_some() => Family::Date,
        Value::Number(_) => Family::Number,
        Value::Text(t) => match ctx.lists.containing(t) {
            lists if !lists.is_empty() => Family::Name(lists),
            _ => Counted::read(t).map_or(Family::Other, Family::Counted),
        },
        _ => Family::Other,
    };
    allowed(family, ctx)
}

/// Each kind of fill continues only some families; the rest repeat.
fn allowed(family: Family, ctx: &Context) -> Family {
    let list_kind = |lists: &[usize]| ctx.lists.kind_of(lists[0]);
    let keep = match (&family, ctx.kind) {
        (_, FillKind::Copy | FillKind::Formats) => false,
        (_, FillKind::Default | FillKind::Series | FillKind::Values) => true,
        (
            Family::Date,
            FillKind::Days | FillKind::Weekdays | FillKind::Months | FillKind::Years,
        ) => true,
        (Family::Name(l), FillKind::Days | FillKind::Weekdays) => list_kind(l) == ListKind::Days,
        (Family::Name(l), FillKind::Months) => list_kind(l) == ListKind::Months,
        (Family::Number, FillKind::LinearTrend | FillKind::GrowthTrend) => true,
        _ => false,
    };
    if keep {
        family
    } else {
        Family::Other
    }
}

/// Split the seeds into runs and work out how each continues.
pub(super) fn plan(seeds: &[Seed], ctx: &Context) -> Vec<Piece> {
    let families: Vec<Family> = seeds.iter().map(|s| family_of(s, ctx)).collect();
    let mut pieces = Vec::new();
    let mut i = 0;
    while i < seeds.len() {
        let mut family = families[i].clone();
        let mut j = i + 1;
        while let Some(merged) = j.lt(&seeds.len()).then(|| join(&family, &families[j])).flatten() {
            family = merged;
            j += 1;
        }
        pieces.push(Piece { start: i, len: j - i, run: run_for(&family, &seeds[i..j], ctx) });
        i = j;
    }
    pieces
}

/// What two neighbouring seeds are together, or `None` when they do not go together.
fn join(a: &Family, b: &Family) -> Option<Family> {
    match (a, b) {
        (Family::Number, Family::Number) => Some(Family::Number),
        (Family::Date, Family::Date) => Some(Family::Date),
        (Family::Name(x), Family::Name(y)) => {
            let both: Vec<usize> = x.iter().filter(|l| y.contains(l)).copied().collect();
            (!both.is_empty()).then_some(Family::Name(both))
        }
        (Family::Counted(x), Family::Counted(y)) => x.goes_with(y).then(|| a.clone()),
        _ => None,
    }
}

fn run_for(family: &Family, seeds: &[Seed], ctx: &Context) -> Run {
    match family {
        Family::Number => number_run(seeds, ctx),
        Family::Date => date_run(seeds, ctx),
        Family::Name(lists) => names_run(lists[0], seeds, ctx),
        Family::Counted(_) => counted_run(seeds),
        Family::Other => Run::Copy,
    }
}

fn numbers_of(seeds: &[Seed]) -> Vec<f64> {
    seeds.iter().filter_map(|s| s.value.as_number()).collect()
}

fn number_run(seeds: &[Seed], ctx: &Context) -> Run {
    let y = numbers_of(seeds);
    if y.len() == 1 {
        let steps_by_one = match ctx.kind {
            FillKind::Series | FillKind::LinearTrend => true,
            FillKind::Default | FillKind::Values => !ctx.single_cell,
            _ => false,
        };
        return if steps_by_one { Run::Linear { a: y[0], b: 1.0 } } else { Run::Copy };
    }
    if ctx.kind == FillKind::GrowthTrend {
        return match growth_fit(&y) {
            Some((c, m)) => Run::Growth { c, m },
            None => Run::Zeros,
        };
    }
    let (a, b) = linear_fit(&y);
    Run::Linear { a, b }
}

// ---------------------------------------------------------------------------------------------------
// Dates.

fn date_run(seeds: &[Seed], ctx: &Context) -> Run {
    let v = numbers_of(seeds);
    let first = v[0];
    let time_only = seeds[0].date == Some(DateShape::TimeOnly);
    let months = month_step(&v, ctx.date1904);
    let day_step = even_step(&v);
    match ctx.kind {
        FillKind::Days => match (v.len(), day_step) {
            (1, _) => Run::Days { first, step: 1.0 },
            (_, Some(step)) => Run::Days { first, step },
            _ => Run::Copy,
        },
        FillKind::Weekdays => weekday_run(&v, day_step.is_some(), ctx.date1904),
        FillKind::Months => match (v.len(), months) {
            (1, _) => Run::Months { first, step: 1, end_of_month: false },
            (_, Some((step, eom))) => Run::Months { first, step, end_of_month: eom },
            _ if day_step.is_some() => shifted_run(v, 1, ctx.date1904),
            _ => Run::Copy,
        },
        FillKind::Years => match (v.len(), months) {
            (1, _) => Run::Months { first, step: 12, end_of_month: false },
            (_, Some((step, eom))) if step % 12 == 0 => {
                Run::Months { first, step, end_of_month: eom }
            }
            _ if day_step.is_some() || months.is_some() => shifted_run(v, 12, ctx.date1904),
            _ => Run::Copy,
        },
        _ if v.len() == 1 => Run::Days { first, step: if time_only { 1.0 / 24.0 } else { 1.0 } },
        _ => match (months, day_step) {
            (Some((step, eom)), _) => Run::Months { first, step, end_of_month: eom },
            (None, Some(step)) => Run::Days { first, step },
            _ => Run::Copy,
        },
    }
}

/// The step between dates when it is the same each time.
fn even_step(v: &[f64]) -> Option<f64> {
    let step = v.get(1)? - v[0];
    v.windows(2).all(|w| ((w[1] - w[0]) - step).abs() < 1e-9).then_some(step)
}

/// The months between dates when they are the same day of the month (or all the last day of it) and the
/// same number of months apart, and whether the last day of the month is kept.
fn month_step(v: &[f64], date1904: bool) -> Option<(i64, bool)> {
    if v.len() < 2 {
        return None;
    }
    let parts: Vec<(i64, u32, u32)> =
        v.iter().map(|d| dates::date(*d, date1904)).collect::<Option<_>>()?;
    let index = |p: &(i64, u32, u32)| p.0 * 12 + p.1 as i64 - 1;
    let step = index(&parts[1]) - index(&parts[0]);
    let even = parts.windows(2).all(|w| index(&w[1]) - index(&w[0]) == step);
    let last_days = parts.iter().all(|p| p.2 == dates::days_in_month(p.0, p.1));
    let same_day = parts.iter().all(|p| p.2 == parts[0].2);
    (even && step != 0 && (same_day || last_days))
        .then_some((step, last_days && (!same_day || parts[0].2 >= 28)))
}

/// The block of dates, moved on by some months each time round. Dates that are all the last day of their
/// month stay on the last day.
fn shifted_run(dates: Vec<f64>, months: i64, date1904: bool) -> Run {
    let all_last_days = dates.len() > 1
        && dates.iter().all(|d| {
            dates::date(*d, date1904).is_some_and(|(y, m, day)| day == dates::days_in_month(y, m))
        });
    Run::Shift { dates, months, end_of_month: all_last_days }
}

fn weekday_run(v: &[f64], even: bool, date1904: bool) -> Run {
    if v.len() > 1 && !even {
        return Run::Copy;
    }
    let first = v[0];
    let step = if v.len() == 1 { 1 } else { weekdays_between(v[0], v[1], date1904).max(1) };
    Run::Weekdays { first, step }
}

fn is_weekend(serial: f64, date1904: bool) -> bool {
    matches!(dates::weekday(serial, date1904), 0 | 6)
}

fn weekdays_between(a: f64, b: f64, date1904: bool) -> i64 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let count = (lo.floor() as i64 + 1..=hi.floor() as i64)
        .filter(|d| !is_weekend(*d as f64, date1904))
        .count() as i64;
    if a <= b {
        count
    } else {
        -count
    }
}

/// A date some weekdays from another, skipping Saturdays and Sundays.
pub(super) fn add_weekdays(serial: f64, n: i64, date1904: bool) -> f64 {
    let fraction = serial - serial.floor();
    let day = serial.floor();
    let weekday = dates::weekday(day, date1904) as i64;
    // Monday is 0 and Friday is 4. A weekend day counts from the Friday before when going forward and
    // from the Monday after when going back.
    let (base, position) = match (weekday, n >= 0) {
        (0, true) => (day - 2.0, 4),
        (6, true) => (day - 1.0, 4),
        (0, false) => (day + 1.0, 0),
        (6, false) => (day + 2.0, 0),
        (w, _) => (day, w - 1),
    };
    let total = position + n;
    let (weeks, offset) = (total.div_euclid(5), total.rem_euclid(5));
    base - position as f64 + (weeks * 7 + offset) as f64 + fraction
}

/// A date some months from another, keeping its day of the month or the last day of the month.
pub(super) fn add_months(serial: f64, months: i64, end_of_month: bool, date1904: bool) -> f64 {
    let fraction = serial - serial.floor();
    let Some((y, m, d)) = dates::date(serial.floor(), date1904) else { return serial };
    let total = y * 12 + (m as i64 - 1) + months;
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let last = dates::days_in_month(ny, nm);
    let day = if end_of_month { last } else { d.min(last) };
    dates::serial(ny, nm as i64, day as i64, date1904).unwrap_or(serial.floor()) + fraction
}

// ---------------------------------------------------------------------------------------------------
// Names and counted text.

fn names_run(list: usize, seeds: &[Seed], ctx: &Context) -> Run {
    let texts: Vec<String> = seeds.iter().map(|s| s.value.to_text()).collect();
    let items = ctx.lists.lists()[list].clone();
    let size = items.len() as i64;
    let positions: Vec<i64> = texts
        .iter()
        .map(|t| items.iter().position(|i| i.eq_ignore_ascii_case(t.trim())).unwrap_or(0) as i64)
        .collect();
    let weekdays_only = ctx.kind == FillKind::Weekdays;
    let (positions, size) = if weekdays_only {
        (positions.iter().map(|p| weekday_slot(*p)).collect::<Vec<_>>(), 5)
    } else {
        (positions, size)
    };
    let case = Case::of(&texts[0]);
    let step =
        if positions.len() == 1 { 1 } else { (positions[1] - positions[0]).rem_euclid(size) };
    let even = positions.windows(2).all(|w| (w[1] - w[0]).rem_euclid(size) == step);
    if !even || (positions.len() > 1 && step == 0) {
        return Run::Copy;
    }
    Run::Names { list: items, first: positions[0], step, case, weekdays_only }
}

/// A day of the week, counted from Sunday, as its place among the five weekdays: Saturday and Sunday count
/// as Friday, so the next weekday after them is Monday.
fn weekday_slot(day: i64) -> i64 {
    match day {
        1..=5 => day - 1,
        _ => 4,
    }
}

fn counted_run(seeds: &[Seed]) -> Run {
    let mut counted: Vec<Counted> =
        seeds.iter().filter_map(|s| Counted::read(&s.value.to_text())).collect();
    let numbers: Vec<i64> = counted.iter().map(|c| c.number).collect();
    let step = if numbers.len() == 1 { 1 } else { numbers[1] - numbers[0] };
    let even = numbers.windows(2).all(|w| w[1] - w[0] == step);
    if !even || (numbers.len() > 1 && step == 0) {
        return Run::Copy;
    }
    let mut first = counted.swap_remove(0);
    first.step = step;
    Run::Counted(first)
}

// ---------------------------------------------------------------------------------------------------
// Values.

/// The value `index` places from the start of a run, or `None` when the seed is repeated. `cycle` counts the
/// times the seeds were gone through and `offset` is the place in the run.
pub(super) fn value_at(
    run: &Run,
    index: i64,
    cycle: i64,
    offset: usize,
    ctx: &Context,
) -> Option<Value> {
    Some(match run {
        Run::Copy => return None,
        Run::Linear { a, b } => Value::Number(round15(a + b * index as f64)),
        Run::Growth { c, m } => Value::Number(round15((c + m * index as f64).exp())),
        Run::Zeros => Value::Number(0.0),
        Run::Days { first, step } => Value::Number(round15(first + step * index as f64)),
        Run::Months { first, step, end_of_month } => {
            Value::Number(add_months(*first, step * index, *end_of_month, ctx.date1904))
        }
        Run::Weekdays { first, step } => {
            Value::Number(add_weekdays(*first, step * index, ctx.date1904))
        }
        Run::Shift { dates, months, end_of_month } => {
            Value::Number(add_months(dates[offset], months * cycle, *end_of_month, ctx.date1904))
        }
        Run::Names { list, first, step, case, weekdays_only } => {
            let size = if *weekdays_only { 5 } else { list.len() as i64 };
            let slot = (first + step * index).rem_euclid(size) as usize;
            let item = if *weekdays_only { &list[slot + 1] } else { &list[slot] };
            Value::text(case.apply(item))
        }
        Run::Counted(c) => Value::text(c.text_at(index)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_follow_english() {
        let c = Counted::read("1st").unwrap();
        let texts: Vec<String> =
            [0, 1, 2, 3, 10, 11, 12, 20].iter().map(|i| c.text_at(*i)).collect();
        assert_eq!(texts, ["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st"]);
    }

    #[test]
    fn a_quarter_wraps_after_four_and_other_text_does_not() {
        assert_eq!(Counted::read("Q4").unwrap().text_at(1), "Q1");
        assert_eq!(Counted::read("Q5").unwrap().text_at(1), "Q6");
        assert_eq!(Counted::read("Week 52").unwrap().text_at(1), "Week 53");
    }

    #[test]
    fn leading_zeros_keep_their_width_and_a_leading_number_counts() {
        assert_eq!(Counted::read("Item 099").unwrap().text_at(1), "Item 100");
        assert_eq!(Counted::read("x007").unwrap().text_at(2), "x009");
        assert_eq!(Counted::read("1 apple").unwrap().text_at(1), "2 apple");
        assert!(Counted::read("A1B").is_none());
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        // 5 January 2023 is a Thursday: one weekday on is Friday, two is Monday.
        let thursday = dates::serial(2023, 1, 5, false).unwrap();
        assert_eq!(add_weekdays(thursday, 2, false), thursday + 4.0);
        assert_eq!(add_weekdays(thursday, -4, false), thursday - 6.0);
        let sunday = thursday + 3.0;
        assert_eq!(add_weekdays(sunday, 1, false), sunday + 1.0);
    }

    #[test]
    fn months_keep_the_day_and_clip_it_to_the_month() {
        let start = dates::serial(2023, 1, 31, false).unwrap();
        let feb = add_months(start, 1, false, false);
        assert_eq!(dates::date(feb, false), Some((2023, 2, 28)));
        let mar = add_months(start, 2, false, false);
        assert_eq!(dates::date(mar, false), Some((2023, 3, 31)));
    }
}
