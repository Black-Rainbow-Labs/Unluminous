//! What a sheet's conditional formats do to a cell.
//!
//! A [`RuleSet`] is prepared once for a sheet: each rule's formulas are parsed and, for the rules that
//! compare a cell with the others (colour scales, data bars, icon sets, top and bottom, above and below
//! average, duplicates), the numbers they compare against are worked out over the cells the rule covers.
//! Asking what a cell looks like is then a walk of the rules that cover it, in priority order, stopping at
//! a rule marked stop if true.
//!
//! The results are values and indexes, not colours on a screen: the formats to lay over the cell's own
//! (indexes into the workbook's differential formats), the colour a colour scale gives it, the shape of a
//! data bar, and the icon an icon set picks. The window turns those into pixels.

use std::collections::HashMap;

use crate::coord::{CellRef, Range};
use crate::edit::clip_to_used;
use crate::eval::{compare, evaluate_expr, to_bool};
use crate::formula::{parser, shifted, Expr};
use crate::model::{CfRule, Cfvo, DataBarExt, IconSetRule};
use crate::styles::Color;
use crate::value::Value;
use crate::Workbook;

/// A colour as its three channels.
pub type Rgb = (u8, u8, u8);

/// The data bar a cell gets: where it starts and ends across the cell.
#[derive(Debug, Clone, PartialEq)]
pub struct BarShape {
    /// The bar's left and right edges as fractions of the cell's width, `start <= end`.
    pub start: f64,
    pub end: f64,
    /// The value is below zero, so the bar uses the negative colours.
    pub negative: bool,
    pub fill: Rgb,
    pub border: Option<Rgb>,
    pub gradient: bool,
    /// Where the axis is drawn as a fraction of the cell's width, and its colour.
    pub axis: Option<(f64, Rgb)>,
    pub show_value: bool,
}

/// The icon an icon set picks for a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconPick {
    /// The set the icon is drawn from, which is the rule's own unless a custom icon was chosen.
    pub set: String,
    /// Which icon of that set, the first being the lowest.
    pub index: usize,
    pub show_value: bool,
    /// No icon for this position (a custom choice of `No Cell Icon`).
    pub hidden: bool,
}

/// What the rules that cover a cell do to it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    /// The differential formats to lay over the cell's own, those of the rules that hold, highest priority
    /// first. A property set by an earlier one is not changed by a later one.
    pub formats: Vec<u32>,
    pub scale: Option<Rgb>,
    /// How many of `formats` have a higher priority than the colour scale, which they win a fill from.
    pub scale_after: usize,
    pub bar: Option<BarShape>,
    pub icon: Option<IconPick>,
}

/// The cell properties the rules that hold set, the first to set one winning.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub fill: Option<Rgb>,
    pub font_color: Option<Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<bool>,
    pub number_format: Option<String>,
}

impl Applied {
    pub fn is_none(&self) -> bool {
        *self == Applied::default()
    }

    /// The formats and the colour scale merged into one look: where two rules set the same property, the
    /// one with the higher priority sets it, and a rule that leaves a property alone leaves it to the next.
    pub fn look(&self, book: &Workbook) -> Look {
        let mut look = Look::default();
        let theme = &book.styles.theme;
        for (i, &index) in self.formats.iter().enumerate() {
            if i == self.scale_after && look.fill.is_none() {
                look.fill = self.scale;
            }
            let Some(dxf) = book.styles.dxfs.get(index as usize) else { continue };
            if look.fill.is_none() {
                look.fill = dxf.fill.and_then(|c| c.resolve(theme));
            }
            if look.font_color.is_none() {
                look.font_color = dxf.font_color.and_then(|c| c.resolve(theme));
            }
            look.bold = look.bold.or(dxf.bold);
            look.italic = look.italic.or(dxf.italic);
            look.strike = look.strike.or(dxf.strike);
            if look.underline.is_none() {
                look.underline = dxf.underline.map(|u| u != crate::styles::Underline::None);
            }
            if look.number_format.is_none() {
                look.number_format = dxf.num_fmt.clone();
            }
        }
        if look.fill.is_none() {
            look.fill = self.scale;
        }
        look
    }
}

#[derive(Debug, Clone)]
struct Prepared {
    ranges: Vec<Range>,
    origin: CellRef,
    rule: CfRule,
    formulas: Vec<Option<Expr>>,
    /// The numbers among the covered cells, sorted.
    numbers: Vec<f64>,
    /// How often each value occurs among the covered cells, for duplicate and unique rules.
    counts: HashMap<String, usize>,
    /// Colour scale, data bar and icon set thresholds as numbers, and for above average rules the mean and
    /// the standard deviation.
    points: Vec<f64>,
    mean: f64,
    deviation: f64,
}

/// A sheet's rules, prepared.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub sheet: usize,
    prepared: Vec<Prepared>,
}

fn key_of(v: &Value) -> String {
    // Excel compares what the cells show: the number 1 and the text "1" are the same, and case does not matter.
    // Text that reads as a number is that number: "01" is the same as 1.
    if let Value::Text(t) = v {
        if let Ok(n) = t.parse::<f64>() {
            return crate::value::number_to_text(n);
        }
    }
    v.to_text().to_lowercase()
}

/// The cell a format's relative references are written against: the top left of its ranges.
fn origin_of(ranges: &[Range]) -> CellRef {
    let first = ranges.first().map_or(CellRef::new(0, 0), |r| r.first);
    ranges.iter().fold(first, |a, r| CellRef::new(a.row.min(r.first.row), a.col.min(r.first.col)))
}

/// Whether a kind of rule compares a cell with the others covered.
fn compares_with_range(kind: &str) -> bool {
    matches!(
        kind,
        "colorScale"
            | "dataBar"
            | "iconSet"
            | "top10"
            | "aboveAverage"
            | "duplicateValues"
            | "uniqueValues"
    )
}

impl RuleSet {
    /// Prepare the rules of one sheet.
    pub fn prepare(book: &Workbook, sheet: usize) -> RuleSet {
        let s = &book.sheets[sheet];
        let mut prepared = Vec::new();
        for format in &s.conditional {
            if format.ranges.is_empty() {
                continue;
            }
            let origin = origin_of(&format.ranges);
            for rule in &format.rules {
                prepared.push(prepare_rule(book, sheet, &format.ranges, origin, rule));
            }
        }
        prepared.sort_by_key(|p| p.rule.priority);
        RuleSet { sheet, prepared }
    }

    pub fn is_empty(&self) -> bool {
        self.prepared.is_empty()
    }

    /// What the rules do to one cell. `now` is the serial `TODAY()` and `NOW()` answer.
    pub fn apply(&self, book: &Workbook, now: f64, at: CellRef) -> Applied {
        let mut applied = Applied::default();
        if self.prepared.is_empty() {
            return applied;
        }
        let value = book.sheets[self.sheet].value_at(at);
        for p in &self.prepared {
            if !p.ranges.iter().any(|r| r.contains(at)) {
                continue;
            }
            let held = apply_rule(p, book, self.sheet, now, at, &value, &mut applied);
            if held && p.rule.stop {
                break;
            }
        }
        applied
    }
}

fn prepare_rule(
    book: &Workbook,
    sheet: usize,
    ranges: &[Range],
    origin: CellRef,
    rule: &CfRule,
) -> Prepared {
    let mut rule = rule.clone();
    if rule.formulas.is_empty() {
        rule.formulas = default_formulas(&rule, origin);
    }
    let formulas =
        rule.formulas.iter().map(|f| parser::parse(f.trim_start_matches('=')).ok()).collect();
    let mut p = Prepared {
        ranges: ranges.to_vec(),
        origin,
        rule,
        formulas,
        numbers: Vec::new(),
        counts: HashMap::new(),
        points: Vec::new(),
        mean: 0.0,
        deviation: 0.0,
    };
    if compares_with_range(&p.rule.kind) {
        collect_covered(book, sheet, &mut p);
        p.points = threshold_points(book, sheet, &p);
        summarise(&mut p);
    }
    p
}

/// The numbers among the cells a rule covers, sorted, and how often each value occurs.
fn collect_covered(book: &Workbook, sheet: usize, p: &mut Prepared) {
    let s = &book.sheets[sheet];
    for range in &p.ranges {
        let r = clip_to_used(s, *range);
        for at in r.cells().take(500_000) {
            let v = s.value_at(at);
            if let Value::Number(n) = v {
                p.numbers.push(n);
            }
            if !v.is_empty() {
                *p.counts.entry(key_of(&v)).or_insert(0) += 1;
            }
        }
    }
    p.numbers.sort_by(|a, b| a.total_cmp(b));
}

/// The mean and the sample standard deviation of the covered numbers.
fn summarise(p: &mut Prepared) {
    let n = p.numbers.len() as f64;
    if p.numbers.is_empty() {
        return;
    }
    p.mean = p.numbers.iter().sum::<f64>() / n;
    if p.numbers.len() > 1 {
        let squares: f64 = p.numbers.iter().map(|x| (x - p.mean).powi(2)).sum();
        p.deviation = (squares / (n - 1.0)).sqrt();
    }
}

// ---------------------------------------------------------------------------------------------------
// The formulas Excel writes for the rules that are a formula in disguise.

/// The formulas Excel stores for a rule that has none of its own: the text rules, the blank and error
/// rules and the time periods, written for the first cell of the range.
pub fn default_formulas(rule: &CfRule, origin: CellRef) -> Vec<String> {
    let a = origin.a1();
    let text = rule.text.clone().unwrap_or_default();
    let quoted = format!("\"{}\"", text.replace('"', "\"\""));
    let one = match rule.kind.as_str() {
        "containsText" => format!("NOT(ISERROR(SEARCH({quoted},{a})))"),
        "notContainsText" => format!("ISERROR(SEARCH({quoted},{a}))"),
        "beginsWith" => format!("LEFT({a},LEN({quoted}))={quoted}"),
        "endsWith" => format!("RIGHT({a},LEN({quoted}))={quoted}"),
        "containsBlanks" => format!("LEN(TRIM({a}))=0"),
        "notContainsBlanks" => format!("LEN(TRIM({a}))>0"),
        "containsErrors" => format!("ISERROR({a})"),
        "notContainsErrors" => format!("NOT(ISERROR({a}))"),
        "timePeriod" => match time_period_formula(rule.time_period.as_deref().unwrap_or(""), &a) {
            Some(f) => f,
            None => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    vec![one]
}

/// The formula Excel writes for a time period rule over a cell.
pub fn time_period_formula(period: &str, a: &str) -> Option<String> {
    Some(match period {
        "today" => format!("FLOOR({a},1)=TODAY()"),
        "yesterday" => format!("FLOOR({a},1)=TODAY()-1"),
        "tomorrow" => format!("FLOOR({a},1)=TODAY()+1"),
        "last7Days" => format!("AND(TODAY()-FLOOR({a},1)<=6,FLOOR({a},1)<=TODAY())"),
        "thisMonth" => format!("AND(MONTH({a})=MONTH(TODAY()),YEAR({a})=YEAR(TODAY()))"),
        "lastMonth" => format!(
            "AND(MONTH({a})=MONTH(EDATE(TODAY(),0-1)),YEAR({a})=YEAR(EDATE(TODAY(),0-1)))"
        ),
        "nextMonth" => format!(
            "AND(MONTH({a})=MONTH(EDATE(TODAY(),0+1)),YEAR({a})=YEAR(EDATE(TODAY(),0+1)))"
        ),
        "thisWeek" => format!(
            "AND(TODAY()-ROUNDDOWN({a},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({a},0)-TODAY()<=7-WEEKDAY(TODAY()))"
        ),
        "lastWeek" => format!(
            "AND(TODAY()-ROUNDDOWN({a},0)>=(WEEKDAY(TODAY())),TODAY()-ROUNDDOWN({a},0)<(WEEKDAY(TODAY())+7))"
        ),
        "nextWeek" => format!(
            "AND(ROUNDDOWN({a},0)-TODAY()>(7-WEEKDAY(TODAY())),ROUNDDOWN({a},0)-TODAY()<(15-WEEKDAY(TODAY())))"
        ),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------------
// Applying one rule.

/// Apply one rule to a cell, recording what it does. Answers whether the rule holds for the cell.
fn apply_rule(
    p: &Prepared,
    book: &Workbook,
    sheet: usize,
    now: f64,
    at: CellRef,
    value: &Value,
    applied: &mut Applied,
) -> bool {
    match p.rule.kind.as_str() {
        "colorScale" => {
            let colour = color_scale(p, book, value);
            if applied.scale.is_none() && colour.is_some() {
                applied.scale = colour;
                applied.scale_after = applied.formats.len();
            }
            colour.is_some()
        }
        "dataBar" => {
            let bar = data_bar(p, book, value);
            if applied.bar.is_none() {
                applied.bar = bar.clone();
            }
            bar.is_some()
        }
        "iconSet" => {
            let icon = icon_for(p, value);
            if applied.icon.is_none() {
                applied.icon = icon.clone();
            }
            icon.is_some()
        }
        _ => {
            let held = holds(p, book, sheet, now, at, value);
            if held {
                if let Some(dxf) = p.rule.dxf {
                    applied.formats.push(dxf);
                }
            }
            held
        }
    }
}

/// The value of a rule's formula moved to a cell.
fn formula_value(
    p: &Prepared,
    index: usize,
    book: &Workbook,
    sheet: usize,
    now: f64,
    at: CellRef,
) -> Option<Value> {
    let expr = p.formulas.get(index)?.as_ref()?;
    let moved =
        shifted(expr, at.row as i64 - p.origin.row as i64, at.col as i64 - p.origin.col as i64);
    Some(evaluate_expr(book, sheet, at, &moved, now))
}

/// Whether a rule of one of the kinds that formats a cell outright holds for it.
fn holds(
    p: &Prepared,
    book: &Workbook,
    sheet: usize,
    now: f64,
    at: CellRef,
    value: &Value,
) -> bool {
    let rule = &p.rule;
    match rule.kind.as_str() {
        "cellIs" => cell_is(p, book, sheet, now, at, value),
        // A cell with an error in it is not blank, whatever `LEN(TRIM())` makes of it.
        "notContainsBlanks" if value.is_error() => true,
        "expression" | "containsText" | "notContainsText" | "beginsWith" | "endsWith"
        | "containsBlanks" | "notContainsBlanks" | "containsErrors" | "notContainsErrors"
        | "timePeriod" => {
            formula_value(p, 0, book, sheet, now, at).is_some_and(|v| to_bool(&v).unwrap_or(false))
        }
        "duplicateValues" => {
            !value.is_empty() && p.counts.get(&key_of(value)).copied().unwrap_or(0) > 1
        }
        "uniqueValues" => {
            !value.is_empty() && p.counts.get(&key_of(value)).copied().unwrap_or(0) == 1
        }
        "top10" => top_or_bottom(p, value),
        "aboveAverage" => above_or_below_average(p, value),
        _ => false,
    }
}

fn cell_is(
    p: &Prepared,
    book: &Workbook,
    sheet: usize,
    now: f64,
    at: CellRef,
    value: &Value,
) -> bool {
    use std::cmp::Ordering::*;
    let a = formula_value(p, 0, book, sheet, now, at).unwrap_or(Value::Empty);
    if value.is_error() || a.is_error() {
        return false;
    }
    let ord = compare(value, &a);
    match p.rule.operator.as_deref().unwrap_or("equal") {
        "equal" => ord == Equal,
        "notEqual" => ord != Equal,
        "greaterThan" => ord == Greater,
        "lessThan" => ord == Less,
        "greaterThanOrEqual" => ord != Less,
        "lessThanOrEqual" => ord != Greater,
        op @ ("between" | "notBetween") => {
            let b = formula_value(p, 1, book, sheet, now, at).unwrap_or(Value::Empty);
            let (low, high) = if compare(&a, &b) == Greater { (b, a) } else { (a, b) };
            let inside = compare(value, &low) != Less && compare(value, &high) != Greater;
            inside == (op == "between")
        }
        _ => false,
    }
}

/// Top or bottom N, or N percent: the cells at or beyond the Nth value.
fn top_or_bottom(p: &Prepared, value: &Value) -> bool {
    let Value::Number(n) = value else { return false };
    let count = p.numbers.len();
    if count == 0 {
        return false;
    }
    let rank = p.rule.rank.unwrap_or(10) as usize;
    let take = if p.rule.percent {
        ((count * rank) as f64 / 100.0).floor().max(1.0) as usize
    } else {
        rank
    }
    .clamp(1, count);
    if p.rule.bottom {
        *n <= p.numbers[take - 1]
    } else {
        *n >= p.numbers[count - take]
    }
}

/// Above or below the average, optionally by some standard deviations, optionally including the average.
fn above_or_below_average(p: &Prepared, value: &Value) -> bool {
    let Value::Number(n) = value else { return false };
    if p.numbers.is_empty() {
        return false;
    }
    let steps = f64::from(p.rule.std_dev.unwrap_or(0));
    let above = p.rule.above_average.unwrap_or(true);
    let equal = p.rule.equal_average;
    let edge = if above { p.mean + steps * p.deviation } else { p.mean - steps * p.deviation };
    match (above, equal) {
        (true, false) => *n > edge,
        (true, true) => *n >= edge,
        (false, false) => *n < edge,
        (false, true) => *n <= edge,
    }
}

// ---------------------------------------------------------------------------------------------------
// Thresholds, colour scales, data bars and icon sets.

/// The number a threshold stands for among the covered numbers.
fn threshold(p: &Prepared, book: &Workbook, sheet: usize, cfvo: &Cfvo, low: bool) -> Option<f64> {
    let numbers = &p.numbers;
    // Without numbers only a threshold that is a number or a formula means anything.
    let (min, max) = match (numbers.first(), numbers.last()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (f64::NAN, f64::NAN),
    };
    let given =
        || cfvo.value.as_deref().and_then(|v| v.trim().trim_start_matches('=').parse::<f64>().ok());
    let evaluated = || {
        let expr = parser::parse(cfvo.value.as_deref()?.trim_start_matches('=')).ok()?;
        evaluate_expr(book, sheet, p.origin, &expr, 0.0).as_number()
    };
    Some(match cfvo.kind.as_str() {
        "min" | "autoMin" => min,
        "max" | "autoMax" => max,
        "num" => given().or_else(evaluated)?,
        "percent" => min + (max - min) * given().unwrap_or(if low { 0.0 } else { 100.0 }) / 100.0,
        "percentile" if !numbers.is_empty() => percentile(numbers, given().unwrap_or(50.0) / 100.0),
        "formula" => evaluated()?,
        _ => {
            if low {
                min
            } else {
                max
            }
        }
    })
}

/// PERCENTILE.INC of sorted numbers.
fn percentile(numbers: &[f64], k: f64) -> f64 {
    let position = k.clamp(0.0, 1.0) * (numbers.len() - 1) as f64;
    let below = position.floor() as usize;
    let above = (below + 1).min(numbers.len() - 1);
    numbers[below] + (numbers[above] - numbers[below]) * (position - below as f64)
}

/// The thresholds of a colour scale, a data bar or an icon set as numbers, in the order the rule lists them.
fn threshold_points(book: &Workbook, sheet: usize, p: &Prepared) -> Vec<f64> {
    let rule = &p.rule;
    let cfvos: Vec<&Cfvo> = if let Some(scale) = &rule.color_scale {
        scale.iter().map(|(c, _)| c).collect()
    } else if let Some((lo, hi, _)) = &rule.data_bar {
        vec![lo, hi]
    } else if let Some(set) = &rule.icon_set {
        set.thresholds.iter().map(|(c, _)| c).collect()
    } else {
        return Vec::new();
    };
    let mut points: Vec<f64> = cfvos
        .iter()
        .enumerate()
        .map(|(i, c)| threshold(p, book, sheet, c, i == 0).unwrap_or(f64::NAN))
        .collect();
    // An automatic threshold reaches zero: with every number positive the bars start at zero, so the
    // smallest number still has a bar, as Excel draws them.
    if let (Some(o), Some((lo, hi, _)), [low, high]) = (
        rule.data_bar_options.as_ref().filter(|o| o.has_extension),
        &rule.data_bar,
        &mut points[..],
    ) {
        if (o.auto_min && lo.kind == "min") || lo.kind == "autoMin" {
            *low = low.min(0.0);
        }
        if (o.auto_max && hi.kind == "max") || hi.kind == "autoMax" {
            *high = high.max(0.0);
        }
    }
    points
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn rgb_of(book: &Workbook, c: &Color) -> Option<Rgb> {
    c.resolve(&book.styles.theme)
}

/// The colour a colour scale gives a number: the colours of the stops either side of it, mixed.
fn color_scale(p: &Prepared, book: &Workbook, value: &Value) -> Option<Rgb> {
    let Value::Number(n) = value else { return None };
    let stops = p.rule.color_scale.as_ref()?;
    let pairs: Vec<(f64, Rgb)> = stops
        .iter()
        .zip(&p.points)
        .filter(|(_, v)| !v.is_nan())
        .filter_map(|((_, colour), v)| Some((*v, rgb_of(book, colour)?)))
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    // Thresholds out of order are put in order, and the colours keep their places: a middle threshold
    // below the lowest one is the lowest.
    let mut values: Vec<f64> = pairs.iter().map(|(v, _)| *v).collect();
    values.sort_by(f64::total_cmp);
    let points: Vec<(f64, Rgb)> = values.into_iter().zip(pairs.iter().map(|(_, c)| *c)).collect();
    let (first, last) = (points[0], points[points.len() - 1]);
    // A number at or past the highest threshold has the highest colour, even when every number is the same.
    if *n >= last.0 {
        return Some(last.1);
    }
    if *n <= first.0 {
        return Some(first.1);
    }
    for pair in points.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if *n <= b {
            let t = if b > a { (n - a) / (b - a) } else { 1.0 };
            return Some(mix(ca, cb, t));
        }
    }
    points.last().map(|(_, c)| *c)
}

/// The shortest and longest bar as fractions of the cell: the extension's lengths when the rule has one.
fn bar_lengths(o: &DataBarExt) -> (f64, f64) {
    if o.has_extension {
        (f64::from(o.min_length) / 100.0, f64::from(o.max_length) / 100.0)
    } else {
        (f64::from(o.main_min_length) / 100.0, f64::from(o.main_max_length) / 100.0)
    }
}

/// Where the axis of a data bar sits across the cell, when the values reach below zero.
fn axis_fraction(o: &DataBarExt, lo: f64, hi: f64) -> Option<f64> {
    if lo >= 0.0 {
        return None;
    }
    match o.axis_position.as_str() {
        "none" => None,
        "middle" => Some(0.5),
        _ => Some(if hi <= 0.0 { 1.0 } else { (-lo / (hi - lo)).clamp(0.0, 1.0) }),
    }
}

fn data_bar(p: &Prepared, book: &Workbook, value: &Value) -> Option<BarShape> {
    let Value::Number(n) = value else { return None };
    let (_, _, colour) = p.rule.data_bar.as_ref()?;
    let (lo, hi) = (*p.points.first()?, *p.points.get(1)?);
    if lo.is_nan() || hi.is_nan() {
        return None;
    }
    let options = p.rule.data_bar_options.clone().unwrap_or_default();
    let (shortest, longest) = bar_lengths(&options);
    let positive = rgb_of(book, colour).unwrap_or((0x63, 0x8E, 0xC6));
    let negative = *n < 0.0;
    let (start, end) = match axis_fraction(&options, lo, hi) {
        Some(axis) => {
            let scale = if negative { axis } else { 1.0 - axis };
            let reach = if negative { lo.abs() } else { hi.abs() };
            let part = if reach > 0.0 { (n.abs() / reach).clamp(0.0, 1.0) } else { 1.0 };
            let length = scale * (shortest + (longest - shortest) * part);
            if negative {
                (axis - length, axis)
            } else {
                (axis, axis + length)
            }
        }
        None => {
            let part = if hi > lo { ((n - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 1.0 };
            (0.0, shortest + (longest - shortest) * part)
        }
    };
    let axis_at = axis_fraction(&options, lo, hi);
    let (start, end) =
        if options.direction == "rightToLeft" { (1.0 - end, 1.0 - start) } else { (start, end) };
    let fill = if negative && !options.negative_same_fill {
        options.negative_fill.as_ref().and_then(|c| rgb_of(book, c)).unwrap_or((0xFF, 0, 0))
    } else {
        positive
    };
    let border = if !options.border {
        None
    } else if negative && !options.negative_same_border {
        options.negative_border.as_ref().and_then(|c| rgb_of(book, c))
    } else {
        Some(options.border_color.as_ref().and_then(|c| rgb_of(book, c)).unwrap_or(positive))
    };
    let axis = axis_at.map(|a| {
        let a = if options.direction == "rightToLeft" { 1.0 - a } else { a };
        (a, options.axis_color.as_ref().and_then(|c| rgb_of(book, c)).unwrap_or((0, 0, 0)))
    });
    Some(BarShape {
        start,
        end,
        negative,
        fill,
        border,
        gradient: options.gradient,
        axis,
        show_value: options.show_value,
    })
}

/// The icon a number earns: the highest icon whose threshold it reaches, counted from the lowest.
pub fn icon_index(set: &IconSetRule, points: &[f64], n: f64) -> usize {
    let mut index = 0;
    for (i, ((_, gte), point)) in set.thresholds.iter().zip(points).enumerate().skip(1) {
        if point.is_nan() {
            continue;
        }
        let reached = if *gte { n >= *point } else { n > *point };
        if reached {
            index = i;
        }
    }
    index
}

fn icon_for(p: &Prepared, value: &Value) -> Option<IconPick> {
    let Value::Number(n) = value else { return None };
    let set = p.rule.icon_set.as_ref()?;
    if p.points.is_empty() {
        return None;
    }
    let count = set.thresholds.len();
    let mut index = icon_index(set, &p.points, *n);
    if set.reverse {
        index = count.saturating_sub(1).saturating_sub(index);
    }
    match set.custom.get(index) {
        Some(Some((name, id))) => Some(IconPick {
            set: name.clone(),
            index: *id as usize,
            show_value: set.show_value,
            hidden: false,
        }),
        Some(None) => Some(IconPick {
            set: set.name.clone(),
            index,
            show_value: set.show_value,
            hidden: true,
        }),
        None => Some(IconPick {
            set: set.name.clone(),
            index,
            show_value: set.show_value,
            hidden: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Session;
    use crate::model::{CfRule, Cfvo, IconSetRule};
    use crate::styles::Dxf;

    fn session_with(values: &[f64]) -> Session {
        let mut s = Session::new(Workbook::new(), 46318.5);
        for (i, v) in values.iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, 0), &v.to_string()).unwrap();
        }
        s
    }

    fn cfvo(kind: &str, value: Option<&str>) -> Cfvo {
        Cfvo { kind: kind.to_owned(), value: value.map(str::to_owned) }
    }

    fn range(text: &str) -> Range {
        Range::parse(text).unwrap()
    }

    #[test]
    fn a_colour_scale_runs_from_the_smallest_to_the_largest() {
        let mut s = session_with(&[1.0, 2.0, 3.0]);
        let mut r = CfRule::new("colorScale");
        r.color_scale = Some(vec![
            (cfvo("min", None), Color::rgb(0, 0, 0)),
            (cfvo("max", None), Color::rgb(200, 100, 0)),
        ]);
        s.add_conditional(0, vec![range("A1:A3")], r).unwrap();
        let rules = RuleSet::prepare(&s.book, 0);
        assert_eq!(rules.apply(&s.book, 0.0, CellRef::new(1, 0)).scale, Some((100, 50, 0)));
        assert_eq!(rules.apply(&s.book, 0.0, CellRef::new(2, 0)).scale, Some((200, 100, 0)));
    }

    #[test]
    fn an_expression_rule_moves_its_formula_to_each_cell() {
        let mut s = session_with(&[5.0, 15.0]);
        s.set_input(0, CellRef::new(0, 1), "10").unwrap();
        s.set_input(0, CellRef::new(1, 1), "10").unwrap();
        s.book.styles.dxfs.push(Dxf { fill: Some(Color::rgb(255, 0, 0)), ..Default::default() });
        let mut r = CfRule::new("expression");
        r.formulas = vec!["A1>B1".to_owned()];
        r.dxf = Some(0);
        s.add_conditional(0, vec![range("A1:A2")], r).unwrap();
        let rules = RuleSet::prepare(&s.book, 0);
        assert!(rules.apply(&s.book, 0.0, CellRef::new(0, 0)).formats.is_empty());
        assert_eq!(rules.apply(&s.book, 0.0, CellRef::new(1, 0)).formats, vec![0]);
    }

    #[test]
    fn an_icon_set_picks_the_highest_threshold_reached() {
        let set = IconSetRule::new("3Arrows");
        let points = [0.0, 33.0, 67.0];
        assert_eq!(icon_index(&set, &points, 10.0), 0);
        assert_eq!(icon_index(&set, &points, 33.0), 1);
        assert_eq!(icon_index(&set, &points, 66.9), 1);
        assert_eq!(icon_index(&set, &points, 67.0), 2);
        let mut strict = IconSetRule::new("3Arrows");
        strict.thresholds[1].1 = false;
        assert_eq!(icon_index(&strict, &points, 33.0), 0);
        assert_eq!(icon_index(&strict, &points, 33.5), 1);
    }

    #[test]
    fn a_stop_if_true_rule_hides_the_rules_after_it() {
        let mut s = session_with(&[1.0, 9.0]);
        s.book.styles.dxfs.push(Dxf { bold: Some(true), ..Default::default() });
        s.book.styles.dxfs.push(Dxf { italic: Some(true), ..Default::default() });
        let mut first = CfRule::new("cellIs");
        first.operator = Some("greaterThan".to_owned());
        first.formulas = vec!["5".to_owned()];
        first.dxf = Some(0);
        first.stop = true;
        let mut second = first.clone();
        second.dxf = Some(1);
        second.stop = false;
        s.add_conditional(0, vec![range("A1:A2")], first).unwrap();
        s.add_conditional(0, vec![range("A1:A2")], second).unwrap();
        let rules = RuleSet::prepare(&s.book, 0);
        assert_eq!(rules.apply(&s.book, 0.0, CellRef::new(1, 0)).formats, vec![0]);
    }
}
