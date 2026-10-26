//! The statistics behind the newer chart types and error bars: histogram bins, Pareto percentages, box and
//! whisker quartiles, waterfall running totals, and the size of error bars.

use crate::chartspec::{
    BinBy, Binning, BoxOptions, ErrorAmount, ErrorBars, ErrorSides, ErrorSource,
};
use crate::functions::stats::{percentile_exc, percentile_inc};

/// Which kind of bin a histogram bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinKind {
    Normal,
    Underflow,
    Overflow,
}

/// One bar of a histogram.
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub from: f64,
    pub to: f64,
    pub count: usize,
    pub kind: BinKind,
    pub label: String,
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

/// The sample standard deviation.
pub fn stdev(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64).sqrt()
}

fn number_text(n: f64) -> String {
    crate::numfmt::general(n, 10)
}

/// Excel's automatic bin width, from the normal reference rule: 3.5 standard deviations over the cube root
/// of the count.
fn automatic_width(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let width = 3.5 * stdev(values) / n.cbrt();
    if width > 0.0 {
        width
    } else {
        1.0
    }
}

/// Group values into histogram bins as Excel's Format Axis pane does: by an automatic width, a width, a
/// count of bins, or one bin for each distinct value, with optional overflow and underflow bins.
pub fn histogram(values: &[f64], options: &Binning) -> Vec<Bin> {
    let values: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return Vec::new();
    }
    if options.by == BinBy::Category {
        return categorical(&values);
    }
    let low = options.underflow.unwrap_or(f64::NEG_INFINITY);
    let high = options.overflow.unwrap_or(f64::INFINITY);
    let inner: Vec<f64> = values.iter().copied().filter(|v| *v >= low && *v <= high).collect();
    let mut bins = Vec::new();
    if let Some(u) = options.underflow {
        let count = values.iter().filter(|v| **v < u).count();
        bins.push(Bin {
            from: f64::NEG_INFINITY,
            to: u,
            count,
            kind: BinKind::Underflow,
            label: format!("\u{2264}{}", number_text(u)),
        });
    }
    if !inner.is_empty() {
        bins.extend(normal_bins(&inner, options));
    }
    if let Some(o) = options.overflow {
        let count = values.iter().filter(|v| **v > o).count();
        bins.push(Bin {
            from: o,
            to: f64::INFINITY,
            count,
            kind: BinKind::Overflow,
            label: format!(">{}", number_text(o)),
        });
    }
    bins
}

/// The bins between the underflow and overflow limits: a count of values and a label for each.
fn normal_bins(inner: &[f64], options: &Binning) -> Vec<Bin> {
    let min = inner.iter().copied().fold(f64::MAX, f64::min);
    let max = inner.iter().copied().fold(f64::MIN, f64::max);
    let width = match options.by {
        BinBy::Width(w) if w > 0.0 => w,
        BinBy::Count(c) if c > 0 => ((max - min) / f64::from(c)).max(f64::MIN_POSITIVE),
        _ => automatic_width(inner),
    };
    let count = match options.by {
        BinBy::Count(c) if c > 0 => c as usize,
        _ => (((max - min) / width).ceil() as usize).max(1),
    };
    let mut bins = Vec::new();
    for i in 0..count {
        let from = min + width * i as f64;
        let to = if i + 1 == count && matches!(options.by, BinBy::Count(_)) {
            max
        } else {
            from + width
        };
        let contains = |v: f64| {
            if options.right_closed {
                (v > from && v <= to) || (i == 0 && v == from)
            } else {
                (v >= from && v < to) || (i + 1 == count && v == to)
            }
        };
        let n = inner.iter().filter(|v| contains(**v)).count();
        let (open, close) = if options.right_closed {
            (if i == 0 { "[" } else { "(" }, "]")
        } else {
            ("[", if i + 1 == count { "]" } else { ")" })
        };
        bins.push(Bin {
            from,
            to,
            count: n,
            kind: BinKind::Normal,
            label: format!("{open}{}, {}{close}", number_text(from), number_text(to)),
        });
    }
    bins
}

fn categorical(values: &[f64]) -> Vec<Bin> {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut bins: Vec<Bin> = Vec::new();
    for v in sorted {
        match bins.last_mut() {
            Some(b) if b.from == v => b.count += 1,
            _ => bins.push(Bin {
                from: v,
                to: v,
                count: 1,
                kind: BinKind::Normal,
                label: number_text(v),
            }),
        }
    }
    bins
}

/// The columns of a Pareto chart and the cumulative share each reaches: categories sorted from the
/// largest total down, and the running percentage of the grand total as a fraction.
pub fn pareto(categories: &[String], values: &[f64]) -> Vec<(String, f64, f64)> {
    let mut totals: Vec<(String, f64)> = Vec::new();
    for (c, v) in categories.iter().zip(values) {
        match totals.iter_mut().find(|(name, _)| name == c) {
            Some(t) => t.1 += v,
            None => totals.push((c.clone(), *v)),
        }
    }
    totals.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let grand: f64 = totals.iter().map(|t| t.1).sum();
    let mut running = 0.0;
    totals
        .into_iter()
        .map(|(name, v)| {
            running += v;
            (name, v, if grand == 0.0 { 0.0 } else { running / grand })
        })
        .collect()
}

/// The figures a box and whisker chart draws for one group of values.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxStats {
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub mean: f64,
    /// The ends of the whiskers: the farthest values within 1.5 times the interquartile range.
    pub low: f64,
    pub high: f64,
    pub outliers: Vec<f64>,
    pub minimum: f64,
    pub maximum: f64,
}

/// Quartiles, whiskers and outliers of a group of values. With an exclusive median the quartiles are
/// `QUARTILE.EXC`, otherwise `QUARTILE.INC`.
pub fn box_stats(values: &[f64], options: &BoxOptions) -> Option<BoxStats> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |k: f64| {
        let r = if options.exclusive_median {
            percentile_exc(&sorted, k)
        } else {
            percentile_inc(&sorted, k)
        };
        r.ok()
    };
    // Exclusive quartiles are undefined for very small groups; Excel then falls back to the data's ends.
    let first = sorted[0];
    let last = sorted[sorted.len() - 1];
    let q1 = q(0.25).unwrap_or(first);
    let median = percentile_inc(&sorted, 0.5).unwrap_or(first);
    let q3 = q(0.75).unwrap_or(last);
    let fence = 1.5 * (q3 - q1);
    let inside: Vec<f64> =
        sorted.iter().copied().filter(|v| *v >= q1 - fence && *v <= q3 + fence).collect();
    let outliers = sorted.iter().copied().filter(|v| *v < q1 - fence || *v > q3 + fence).collect();
    Some(BoxStats {
        q1,
        median,
        q3,
        mean: mean(&sorted),
        low: inside.first().copied().unwrap_or(first),
        high: inside.last().copied().unwrap_or(last),
        outliers,
        minimum: first,
        maximum: last,
    })
}

/// How a waterfall bar moved the running total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterfallKind {
    Increase,
    Decrease,
    Total,
}

/// One bar of a waterfall chart: from where the running total was to where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterfallBar {
    pub start: f64,
    pub end: f64,
    pub kind: WaterfallKind,
}

/// Running totals for a waterfall chart. A point named in `totals` is drawn from zero to the running total
/// at that point, and does not add its own value to it.
pub fn waterfall(values: &[f64], totals: &[u32]) -> Vec<WaterfallBar> {
    let mut running = 0.0;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if totals.contains(&(i as u32)) {
                // A total shows what has built up; a value written in a total's row replaces the running total.
                if *v != 0.0 && v.is_finite() && i == 0 {
                    running = *v;
                }
                return WaterfallBar { start: 0.0, end: running, kind: WaterfallKind::Total };
            }
            let start = running;
            running += v;
            WaterfallBar {
                start,
                end: running,
                kind: if *v >= 0.0 { WaterfallKind::Increase } else { WaterfallKind::Decrease },
            }
        })
        .collect()
}

/// How far an error bar reaches on each side of each point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ErrorAmounts {
    pub plus: Vec<f64>,
    pub minus: Vec<f64>,
}

/// The size of error bars for points: `custom` gives the numbers of a custom plus and minus.
pub fn error_amounts(
    bars: &ErrorBars,
    values: &[Option<f64>],
    custom: (Vec<f64>, Vec<f64>),
) -> ErrorAmounts {
    let defined: Vec<f64> = values.iter().flatten().copied().collect();
    let amount = bars.value.unwrap_or(1.0);
    let each = |i: usize, y: f64| -> (f64, f64) {
        match bars.amount {
            ErrorAmount::Fixed => (amount, amount),
            ErrorAmount::Percentage => (y.abs() * amount / 100.0, y.abs() * amount / 100.0),
            ErrorAmount::StandardDeviation => (stdev(&defined) * amount, stdev(&defined) * amount),
            ErrorAmount::StandardError => {
                let se = stdev(&defined) / (defined.len() as f64).sqrt();
                (se, se)
            }
            ErrorAmount::Custom => (
                custom.0.get(i).or(custom.0.last()).copied().unwrap_or(0.0),
                custom.1.get(i).or(custom.1.last()).copied().unwrap_or(0.0),
            ),
        }
    };
    let mut out = ErrorAmounts::default();
    for (i, v) in values.iter().enumerate() {
        let (p, m) = each(i, v.unwrap_or(0.0));
        out.plus.push(if bars.sides == ErrorSides::Minus { 0.0 } else { p });
        out.minus.push(if bars.sides == ErrorSides::Plus { 0.0 } else { m });
    }
    out
}

/// The range formula a custom error amount reads, if it is one.
pub fn custom_source(source: &Option<ErrorSource>) -> Option<&str> {
    match source {
        Some(ErrorSource::Range(f)) => Some(f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_histogram_by_width_counts_right_closed_intervals() {
        let values = [1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 6.0, 6.0, 9.0];
        let options = Binning { by: BinBy::Width(3.0), ..Binning::default() };
        let bins = histogram(&values, &options);
        let counts: Vec<usize> = bins.iter().map(|b| b.count).collect();
        assert_eq!(counts, vec![5, 3, 1]);
        assert_eq!(bins[0].label, "[1, 4]");
        assert_eq!(bins[1].label, "(4, 7]");
    }

    #[test]
    fn overflow_and_underflow_bins_take_the_ends() {
        let values = [1.0, 2.0, 5.0, 6.0, 7.0, 20.0, 30.0];
        let options = Binning {
            by: BinBy::Count(2),
            overflow: Some(10.0),
            underflow: Some(2.0),
            ..Binning::default()
        };
        let bins = histogram(&values, &options);
        assert_eq!(bins.first().unwrap().kind, BinKind::Underflow);
        assert_eq!(bins.first().unwrap().count, 1);
        assert_eq!(bins.last().unwrap().kind, BinKind::Overflow);
        assert_eq!(bins.last().unwrap().count, 2);
        assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 7);
    }

    #[test]
    fn pareto_sorts_and_accumulates() {
        let cats: Vec<String> = ["a", "b", "c", "a"].iter().map(|s| s.to_string()).collect();
        let rows = pareto(&cats, &[1.0, 5.0, 2.0, 2.0]);
        assert_eq!(rows[0].0, "b");
        assert!((rows[2].2 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_box_has_quartiles_whiskers_and_outliers() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 40.0];
        let inclusive =
            box_stats(&values, &BoxOptions { exclusive_median: false, ..BoxOptions::default() })
                .unwrap();
        assert_eq!((inclusive.q1, inclusive.median, inclusive.q3), (3.0, 5.0, 7.0));
        assert_eq!(inclusive.outliers, vec![40.0]);
        assert_eq!(inclusive.high, 8.0);
        let exclusive = box_stats(&values, &BoxOptions::default()).unwrap();
        assert_eq!((exclusive.q1, exclusive.q3), (2.5, 7.5));
    }

    #[test]
    fn a_waterfall_keeps_a_running_total() {
        let bars = waterfall(&[100.0, -30.0, 20.0, 0.0], &[3]);
        assert_eq!((bars[1].start, bars[1].end), (100.0, 70.0));
        assert_eq!(bars[3].kind, WaterfallKind::Total);
        assert_eq!(bars[3].end, 90.0);
    }

    #[test]
    fn error_bars_have_the_size_their_kind_gives() {
        let values = [Some(10.0), Some(20.0), Some(30.0)];
        let fixed = ErrorBars { value: Some(2.0), ..ErrorBars::default() };
        assert_eq!(error_amounts(&fixed, &values, (vec![], vec![])).plus, vec![2.0, 2.0, 2.0]);
        let percent = ErrorBars {
            amount: ErrorAmount::Percentage,
            value: Some(10.0),
            ..ErrorBars::default()
        };
        assert_eq!(error_amounts(&percent, &values, (vec![], vec![])).minus, vec![1.0, 2.0, 3.0]);
        let sd = ErrorBars {
            amount: ErrorAmount::StandardDeviation,
            value: Some(1.0),
            ..ErrorBars::default()
        };
        assert_eq!(error_amounts(&sd, &values, (vec![], vec![])).plus[0], 10.0);
    }
}
