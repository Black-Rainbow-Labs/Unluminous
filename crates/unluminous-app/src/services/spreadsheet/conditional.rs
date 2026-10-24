//! What a sheet's conditional formats do to each cell, for drawing.
//!
//! A rule is prepared once per revision of the workbook: its formulas parsed and, for the rules that
//! compare a cell with the others (colour scales, data bars, top and bottom, above average, duplicates), the
//! numbers they compare against worked out over the cells the rule covers. Asking what a cell looks like is
//! then a walk of the rules that cover it, highest priority first.
//!
//! Icon sets and time periods are kept in the file and not drawn.

use std::collections::HashMap;

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::clip_to_used;
use unluminous_sheet::eval::{compare, evaluate_expr, to_bool};
use unluminous_sheet::formula::{shifted, Expr};
use unluminous_sheet::model::{CfRule, Cfvo};
use unluminous_sheet::{Value, Workbook};

use super::display::Rgb;

/// What the rules that apply to a cell change about how it is drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Effect {
    pub fill: Option<Rgb>,
    pub color: Option<Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<bool>,
    pub number_format: Option<String>,
    /// A data bar: how much of the cell it fills, from 0 to 1, and its colour.
    pub bar: Option<(f32, Rgb)>,
}

impl Effect {
    pub fn is_none(&self) -> bool {
        *self == Effect::default()
    }
}

#[derive(Debug, Clone)]
struct Prepared {
    ranges: Vec<Range>,
    origin: CellRef,
    rule: CfRule,
    formulas: Vec<Expr>,
    /// The numbers among the covered cells, sorted.
    numbers: Vec<f64>,
    /// How often each value occurs among the covered cells, for duplicate and unique rules.
    counts: HashMap<String, usize>,
}

/// A sheet's rules, prepared.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    pub revision: u64,
    pub sheet: usize,
    prepared: Vec<Prepared>,
}

fn key_of(v: &Value) -> String {
    match v {
        Value::Text(t) => format!("t{}", t.to_lowercase()),
        other => format!("v{}", other.to_text()),
    }
}

impl Rules {
    /// Prepare the rules of one sheet.
    pub fn prepare(book: &Workbook, sheet: usize, revision: u64) -> Rules {
        let s = &book.sheets[sheet];
        let mut prepared = Vec::new();
        for format in &s.conditional {
            let Some(first) = format.ranges.first() else { continue };
            let origin = format.ranges.iter().fold(first.first, |a, r| {
                CellRef::new(a.row.min(r.first.row), a.col.min(r.first.col))
            });
            for rule in &format.rules {
                let formulas = rule
                    .formulas
                    .iter()
                    .filter_map(|f| {
                        unluminous_sheet::formula::parser::parse(f.trim_start_matches('=')).ok()
                    })
                    .collect();
                let compares = matches!(
                    rule.kind.as_str(),
                    "colorScale"
                        | "dataBar"
                        | "top10"
                        | "aboveAverage"
                        | "duplicateValues"
                        | "uniqueValues"
                );
                let mut numbers = Vec::new();
                let mut counts = HashMap::new();
                if compares {
                    for range in &format.ranges {
                        let r = clip_to_used(s, *range);
                        for at in r.cells().take(200_000) {
                            let v = s.value_at(at);
                            if let Value::Number(n) = v {
                                numbers.push(n);
                            }
                            if !v.is_empty() {
                                *counts.entry(key_of(&v)).or_insert(0) += 1;
                            }
                        }
                    }
                    numbers.sort_by(|a, b| a.total_cmp(b));
                }
                prepared.push(Prepared {
                    ranges: format.ranges.clone(),
                    origin,
                    rule: rule.clone(),
                    formulas,
                    numbers,
                    counts,
                });
            }
        }
        prepared.sort_by_key(|p| p.rule.priority);
        Rules { revision, sheet, prepared }
    }

    pub fn is_empty(&self) -> bool {
        self.prepared.is_empty()
    }

    /// What the rules do to one cell.
    pub fn effect(&self, book: &Workbook, now: f64, at: CellRef) -> Effect {
        let mut effect = Effect::default();
        if self.prepared.is_empty() {
            return effect;
        }
        let s = &book.sheets[self.sheet];
        let value = s.value_at(at);
        for p in &self.prepared {
            if !p.ranges.iter().any(|r| r.contains(at)) {
                continue;
            }
            let matched = match p.rule.kind.as_str() {
                "colorScale" => {
                    if effect.fill.is_none() {
                        effect.fill = color_scale(p, book, &value);
                    }
                    continue;
                }
                "dataBar" => {
                    if effect.bar.is_none() {
                        effect.bar = data_bar(p, book, &value);
                    }
                    continue;
                }
                _ => matches(p, book, self.sheet, now, at, &value),
            };
            if !matched {
                continue;
            }
            if let Some(dxf) = p.rule.dxf.and_then(|i| book.styles.dxfs.get(i as usize)) {
                let theme = &book.styles.theme;
                if effect.fill.is_none() {
                    effect.fill = dxf.fill.and_then(|c| c.resolve(theme));
                }
                if effect.color.is_none() {
                    effect.color = dxf.font_color.and_then(|c| c.resolve(theme));
                }
                effect.bold = effect.bold.or(dxf.bold);
                effect.italic = effect.italic.or(dxf.italic);
                effect.strike = effect.strike.or(dxf.strike);
                if effect.underline.is_none() {
                    effect.underline =
                        dxf.underline.map(|u| u != unluminous_sheet::styles::Underline::None);
                }
                if effect.number_format.is_none() {
                    effect.number_format = dxf.num_fmt.clone();
                }
            }
            if p.rule.stop {
                break;
            }
        }
        effect
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
    let expr = p.formulas.get(index)?;
    let moved =
        shifted(expr, at.row as i64 - p.origin.row as i64, at.col as i64 - p.origin.col as i64);
    Some(evaluate_expr(book, sheet, at, &moved, now))
}

fn matches(
    p: &Prepared,
    book: &Workbook,
    sheet: usize,
    now: f64,
    at: CellRef,
    value: &Value,
) -> bool {
    let rule = &p.rule;
    let text = || value.to_text().to_lowercase();
    let needle = || rule.text.clone().unwrap_or_default().to_lowercase();
    match rule.kind.as_str() {
        "cellIs" => {
            if value.is_empty()
                && !matches!(rule.operator.as_deref(), Some("equal") | Some("notEqual"))
            {
                // An empty cell is compared as zero, as Excel does.
            }
            let a = formula_value(p, 0, book, sheet, now, at).unwrap_or(Value::Empty);
            let v = if value.is_empty() { Value::Number(0.0) } else { value.clone() };
            let ord = compare(&v, &a);
            use std::cmp::Ordering::*;
            match rule.operator.as_deref().unwrap_or("equal") {
                "equal" => ord == Equal,
                "notEqual" => ord != Equal,
                "greaterThan" => ord == Greater,
                "lessThan" => ord == Less,
                "greaterThanOrEqual" => ord != Less,
                "lessThanOrEqual" => ord != Greater,
                "between" | "notBetween" => {
                    let b = formula_value(p, 1, book, sheet, now, at).unwrap_or(Value::Empty);
                    let (low, high) = if compare(&a, &b) == Greater { (b, a) } else { (a, b) };
                    let inside = compare(&v, &low) != Less && compare(&v, &high) != Greater;
                    inside == (rule.operator.as_deref() == Some("between"))
                }
                _ => false,
            }
        }
        "expression" => {
            formula_value(p, 0, book, sheet, now, at).is_some_and(|v| to_bool(&v).unwrap_or(false))
        }
        "containsText" => text().contains(&needle()),
        "notContainsText" => !text().contains(&needle()),
        "beginsWith" => text().starts_with(&needle()),
        "endsWith" => text().ends_with(&needle()),
        "containsBlanks" => value.to_text().trim().is_empty(),
        "notContainsBlanks" => !value.to_text().trim().is_empty(),
        "containsErrors" => value.is_error(),
        "notContainsErrors" => !value.is_error(),
        "duplicateValues" => {
            !value.is_empty() && p.counts.get(&key_of(value)).copied().unwrap_or(0) > 1
        }
        "uniqueValues" => {
            !value.is_empty() && p.counts.get(&key_of(value)).copied().unwrap_or(0) == 1
        }
        "top10" => {
            let Value::Number(n) = value else { return false };
            let count = p.numbers.len();
            if count == 0 {
                return false;
            }
            let rank = rule.rank.unwrap_or(10) as usize;
            let take = if rule.percent {
                ((count * rank) as f64 / 100.0).floor().max(1.0) as usize
            } else {
                rank
            }
            .min(count);
            if rule.bottom {
                *n <= p.numbers[take - 1]
            } else {
                *n >= p.numbers[count - take]
            }
        }
        "aboveAverage" => {
            let Value::Number(n) = value else { return false };
            if p.numbers.is_empty() {
                return false;
            }
            let mean = p.numbers.iter().sum::<f64>() / p.numbers.len() as f64;
            let above = rule.above_average.unwrap_or(true);
            let equal = rule.attrs.iter().any(|(k, v)| k == "equalAverage" && v == "1");
            match (above, equal) {
                (true, false) => *n > mean,
                (true, true) => *n >= mean,
                (false, false) => *n < mean,
                (false, true) => *n <= mean,
            }
        }
        _ => false,
    }
}

/// The number a colour scale or data bar threshold stands for, among the covered numbers.
fn threshold(p: &Prepared, book: &Workbook, cfvo: &Cfvo, low: bool) -> Option<f64> {
    let numbers = &p.numbers;
    let (min, max) = (*numbers.first()?, *numbers.last()?);
    let given =
        || cfvo.value.as_deref().and_then(|v| v.trim_start_matches('=').parse::<f64>().ok());
    Some(match cfvo.kind.as_str() {
        "min" | "autoMin" => min,
        "max" | "autoMax" => max,
        "num" => match given() {
            Some(n) => n,
            None => {
                let expr = unluminous_sheet::formula::parser::parse(
                    cfvo.value.as_deref()?.trim_start_matches('='),
                )
                .ok()?;
                evaluate_expr(book, 0, p.origin, &expr, 0.0).as_number()?
            }
        },
        "percent" => min + (max - min) * given().unwrap_or(if low { 0.0 } else { 100.0 }) / 100.0,
        "percentile" => {
            let k = given().unwrap_or(50.0) / 100.0;
            let position = k * (numbers.len() - 1) as f64;
            let below = position.floor() as usize;
            let above = (below + 1).min(numbers.len() - 1);
            numbers[below] + (numbers[above] - numbers[below]) * (position - below as f64)
        }
        "formula" => {
            let expr = unluminous_sheet::formula::parser::parse(
                cfvo.value.as_deref()?.trim_start_matches('='),
            )
            .ok()?;
            evaluate_expr(book, 0, p.origin, &expr, 0.0).as_number()?
        }
        _ => {
            if low {
                min
            } else {
                max
            }
        }
    })
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    (lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

fn color_scale(p: &Prepared, book: &Workbook, value: &Value) -> Option<Rgb> {
    let Value::Number(n) = value else { return None };
    let stops = p.rule.color_scale.as_ref()?;
    let theme = &book.styles.theme;
    let points: Vec<(f64, Rgb)> = stops
        .iter()
        .enumerate()
        .filter_map(|(i, (cfvo, color))| {
            Some((threshold(p, book, cfvo, i == 0)?, color.resolve(theme)?))
        })
        .collect();
    if points.len() < 2 {
        return None;
    }
    if *n <= points[0].0 {
        return Some(points[0].1);
    }
    for pair in points.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if *n <= b {
            let t = if b > a { (n - a) / (b - a) } else { 1.0 };
            return Some(mix(ca, cb, t));
        }
    }
    Some(points.last()?.1)
}

fn data_bar(p: &Prepared, book: &Workbook, value: &Value) -> Option<(f32, Rgb)> {
    let Value::Number(n) = value else { return None };
    let (low, high, color) = p.rule.data_bar.as_ref()?;
    let a = threshold(p, book, low, true)?;
    let b = threshold(p, book, high, false)?;
    let fraction = if b > a { ((n - a) / (b - a)).clamp(0.0, 1.0) } else { 1.0 };
    // Excel draws the shortest bar at a tenth of the cell, so the smallest value still shows one.
    let shown = 0.1 + 0.9 * fraction;
    Some((shown as f32, color.resolve(&book.styles.theme).unwrap_or((0x63, 0x8E, 0xC6))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::edit::Session;
    use unluminous_sheet::styles::Color;

    fn rule(kind: &str) -> CfRule {
        CfRule {
            kind: kind.to_owned(),
            dxf: None,
            priority: 1,
            stop: false,
            operator: None,
            text: None,
            rank: None,
            percent: false,
            bottom: false,
            above_average: None,
            formulas: Vec::new(),
            color_scale: None,
            data_bar: None,
            attrs: Vec::new(),
            raw_children: String::new(),
        }
    }

    #[test]
    fn a_colour_scale_runs_from_the_smallest_to_the_largest() {
        let mut s = Session::new(Workbook::new(), 46318.5);
        for (i, n) in ["1", "2", "3"].iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, 0), n).unwrap();
        }
        let mut r = rule("colorScale");
        let cfvo = |k: &str| Cfvo { kind: k.to_owned(), value: None };
        r.color_scale =
            Some(vec![(cfvo("min"), Color::rgb(0, 0, 0)), (cfvo("max"), Color::rgb(200, 100, 0))]);
        s.add_conditional(0, vec![Range::parse("A1:A3").unwrap()], r).unwrap();
        let rules = Rules::prepare(&s.book, 0, 1);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(1, 0)).fill, Some((100, 50, 0)));
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(2, 0)).fill, Some((200, 100, 0)));
    }

    #[test]
    fn a_cell_rule_compares_with_its_formula_moved_to_each_cell() {
        let mut s = Session::new(Workbook::new(), 46318.5);
        s.set_input(0, CellRef::new(0, 0), "5").unwrap();
        s.set_input(0, CellRef::new(1, 0), "15").unwrap();
        s.set_input(0, CellRef::new(0, 1), "10").unwrap();
        s.set_input(0, CellRef::new(1, 1), "10").unwrap();
        let mut r = rule("expression");
        r.formulas = vec!["A1>B1".to_owned()];
        r.dxf = Some(0);
        s.book.styles.dxfs.push(unluminous_sheet::styles::Dxf {
            fill: Some(Color::rgb(255, 0, 0)),
            ..Default::default()
        });
        s.add_conditional(0, vec![Range::parse("A1:A2").unwrap()], r).unwrap();
        let rules = Rules::prepare(&s.book, 0, 1);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(0, 0)).fill, None);
        assert_eq!(rules.effect(&s.book, 0.0, CellRef::new(1, 0)).fill, Some((255, 0, 0)));
    }
}
