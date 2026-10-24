//! Statistics: averages, counts, extremes, ranks, percentiles, spread and simple regression.

use std::cmp::Ordering;

use super::criteria::Criterion;
use super::math::{aligned, clipped, ifs_mask};
use super::{gather, gather_val, num, number, numbers_at, r, s, Function, Gather, MANY};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("AVERAGE", 1, MANY, "number1, [number2], ...", "The arithmetic mean.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| mean(n).ok_or(ErrorKind::Div0))
    }),
    r(
        "AVERAGEA",
        1,
        MANY,
        "value1, [value2], ...",
        "The mean, counting text as 0 and logicals as 1 or 0.",
        |ctx, a| over(ctx, a, Gather::All, |n| mean(n).ok_or(ErrorKind::Div0)),
    ),
    r(
        "AVERAGEIF",
        2,
        3,
        "range, criteria, [average_range]",
        "The mean of the cells that meet a criterion.",
        averageif,
    ),
    r(
        "AVERAGEIFS",
        3,
        MANY,
        "average_range, criteria_range1, criteria1, ...",
        "The mean of the cells that meet every criterion.",
        |ctx, a| ifs_over(ctx, a, |n| mean(n).ok_or(ErrorKind::Div0)),
    ),
    r(
        "MAXIFS",
        3,
        MANY,
        "max_range, criteria_range1, criteria1, ...",
        "The largest of the cells that meet every criterion.",
        |ctx, a| {
            ifs_over(ctx, a, |n| {
                Ok(n.iter().copied().fold(f64::NEG_INFINITY, f64::max).max(if n.is_empty() {
                    0.0
                } else {
                    f64::NEG_INFINITY
                }))
            })
        },
    ),
    r(
        "MINIFS",
        3,
        MANY,
        "min_range, criteria_range1, criteria1, ...",
        "The smallest of the cells that meet every criterion.",
        |ctx, a| {
            ifs_over(ctx, a, |n| {
                Ok(if n.is_empty() { 0.0 } else { n.iter().copied().fold(f64::INFINITY, f64::min) })
            })
        },
    ),
    r("COUNT", 1, MANY, "value1, [value2], ...", "How many numbers there are.", count),
    r("COUNTA", 1, MANY, "value1, [value2], ...", "How many values are not empty.", counta),
    r("COUNTBLANK", 1, 1, "range", "How many cells are empty.", countblank),
    r("COUNTIF", 2, 2, "range, criteria", "How many cells meet a criterion.", countif),
    r(
        "COUNTIFS",
        2,
        MANY,
        "criteria_range1, criteria1, ...",
        "How many cells meet every criterion.",
        |ctx, a| match ifs_mask(ctx, a, None) {
            Ok((mask, ..)) => Val::number(mask.iter().filter(|m| **m).count() as f64),
            Err(e) => Val::error(e),
        },
    ),
    r("MAX", 1, MANY, "number1, [number2], ...", "The largest number.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| Ok(max(n)))
    }),
    r(
        "MAXA",
        1,
        MANY,
        "value1, [value2], ...",
        "The largest value, counting text as 0.",
        |ctx, a| over(ctx, a, Gather::AllInRanges, |n| Ok(max(n))),
    ),
    r("MIN", 1, MANY, "number1, [number2], ...", "The smallest number.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| Ok(min(n)))
    }),
    r(
        "MINA",
        1,
        MANY,
        "value1, [value2], ...",
        "The smallest value, counting text as 0.",
        |ctx, a| over(ctx, a, Gather::AllInRanges, |n| Ok(min(n))),
    ),
    r("MEDIAN", 1, MANY, "number1, [number2], ...", "The middle number.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| median(n).ok_or(ErrorKind::Num))
    }),
    r("MODE", 1, MANY, "number1, [number2], ...", "The most frequent number.", mode_sngl),
    r("MODE.SNGL", 1, MANY, "number1, [number2], ...", "The most frequent number.", mode_sngl),
    r(
        "MODE.MULT",
        1,
        MANY,
        "number1, [number2], ...",
        "Every number that is most frequent, as a column.",
        |ctx, a| match gather(ctx, a, Gather::OnlyNumbers) {
            Ok(n) => {
                let m = modes(&n);
                if m.is_empty() {
                    Val::error(ErrorKind::NA)
                } else {
                    Val::Array(Array::column(m.into_iter().map(Value::Number).collect()))
                }
            }
            Err(e) => Val::error(e),
        },
    ),
    r("LARGE", 2, 2, "array, k", "The k-th largest number.", |ctx, a| ranked(ctx, a, true)),
    r("SMALL", 2, 2, "array, k", "The k-th smallest number.", |ctx, a| ranked(ctx, a, false)),
    r("RANK", 2, 3, "number, ref, [order]", "A number's rank in a list.", |ctx, a| {
        rank(ctx, a, false)
    }),
    r(
        "RANK.EQ",
        2,
        3,
        "number, ref, [order]",
        "A number's rank in a list; ties share the top rank.",
        |ctx, a| rank(ctx, a, false),
    ),
    r(
        "RANK.AVG",
        2,
        3,
        "number, ref, [order]",
        "A number's rank in a list; ties share the average rank.",
        |ctx, a| rank(ctx, a, true),
    ),
    r("PERCENTILE", 2, 2, "array, k", "The k-th percentile, k from 0 to 1.", |ctx, a| {
        sorted_k(ctx, a, percentile_inc)
    }),
    r("PERCENTILE.INC", 2, 2, "array, k", "The k-th percentile, k from 0 to 1.", |ctx, a| {
        sorted_k(ctx, a, percentile_inc)
    }),
    r(
        "PERCENTILE.EXC",
        2,
        2,
        "array, k",
        "The k-th percentile, k between 0 and 1 exclusive.",
        |ctx, a| sorted_k(ctx, a, percentile_exc),
    ),
    r("QUARTILE", 2, 2, "array, quart", "A quartile, 0 to 4.", |ctx, a| {
        sorted_k(ctx, a, quartile_inc)
    }),
    r("QUARTILE.INC", 2, 2, "array, quart", "A quartile, 0 to 4.", |ctx, a| {
        sorted_k(ctx, a, quartile_inc)
    }),
    r("QUARTILE.EXC", 2, 2, "array, quart", "A quartile, 1 to 3.", |ctx, a| {
        sorted_k(ctx, a, |n, q| {
            let q = q.trunc();
            if !(1.0..=3.0).contains(&q) {
                return Err(ErrorKind::Num);
            }
            percentile_exc(n, q / 4.0)
        })
    }),
    r(
        "PERCENTRANK",
        2,
        3,
        "array, x, [significance]",
        "The rank of a value as a percentage.",
        |ctx, a| percentrank(ctx, a, false),
    ),
    r(
        "PERCENTRANK.INC",
        2,
        3,
        "array, x, [significance]",
        "The rank of a value as a percentage, 0 to 1 inclusive.",
        |ctx, a| percentrank(ctx, a, false),
    ),
    r(
        "PERCENTRANK.EXC",
        2,
        3,
        "array, x, [significance]",
        "The rank of a value as a percentage, 0 to 1 exclusive.",
        |ctx, a| percentrank(ctx, a, true),
    ),
    r(
        "STDEV",
        1,
        MANY,
        "number1, [number2], ...",
        "The standard deviation of a sample.",
        |ctx, a| {
            over(ctx, a, Gather::Numbers, |n| {
                sample_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r(
        "STDEV.S",
        1,
        MANY,
        "number1, [number2], ...",
        "The standard deviation of a sample.",
        |ctx, a| {
            over(ctx, a, Gather::Numbers, |n| {
                sample_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r(
        "STDEVA",
        1,
        MANY,
        "value1, [value2], ...",
        "The standard deviation of a sample, counting text as 0.",
        |ctx, a| {
            over(ctx, a, Gather::AllInRanges, |n| {
                sample_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r(
        "STDEVP",
        1,
        MANY,
        "number1, [number2], ...",
        "The standard deviation of a population.",
        |ctx, a| {
            over(ctx, a, Gather::Numbers, |n| {
                population_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r(
        "STDEV.P",
        1,
        MANY,
        "number1, [number2], ...",
        "The standard deviation of a population.",
        |ctx, a| {
            over(ctx, a, Gather::Numbers, |n| {
                population_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r(
        "STDEVPA",
        1,
        MANY,
        "value1, [value2], ...",
        "The standard deviation of a population, counting text as 0.",
        |ctx, a| {
            over(ctx, a, Gather::AllInRanges, |n| {
                population_variance(n).map(f64::sqrt).ok_or(ErrorKind::Div0)
            })
        },
    ),
    r("VAR", 1, MANY, "number1, [number2], ...", "The variance of a sample.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| sample_variance(n).ok_or(ErrorKind::Div0))
    }),
    r("VAR.S", 1, MANY, "number1, [number2], ...", "The variance of a sample.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| sample_variance(n).ok_or(ErrorKind::Div0))
    }),
    r(
        "VARA",
        1,
        MANY,
        "value1, [value2], ...",
        "The variance of a sample, counting text as 0.",
        |ctx, a| over(ctx, a, Gather::AllInRanges, |n| sample_variance(n).ok_or(ErrorKind::Div0)),
    ),
    r("VARP", 1, MANY, "number1, [number2], ...", "The variance of a population.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| population_variance(n).ok_or(ErrorKind::Div0))
    }),
    r("VAR.P", 1, MANY, "number1, [number2], ...", "The variance of a population.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| population_variance(n).ok_or(ErrorKind::Div0))
    }),
    r(
        "VARPA",
        1,
        MANY,
        "value1, [value2], ...",
        "The variance of a population, counting text as 0.",
        |ctx, a| {
            over(ctx, a, Gather::AllInRanges, |n| population_variance(n).ok_or(ErrorKind::Div0))
        },
    ),
    r("AVEDEV", 1, MANY, "number1, [number2], ...", "The mean absolute deviation.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            let m = mean(n).ok_or(ErrorKind::Num)?;
            Ok(n.iter().map(|x| (x - m).abs()).sum::<f64>() / n.len() as f64)
        })
    }),
    r(
        "DEVSQ",
        1,
        MANY,
        "number1, [number2], ...",
        "The sum of squared deviations from the mean.",
        |ctx, a| {
            over(ctx, a, Gather::Numbers, |n| {
                let m = mean(n).ok_or(ErrorKind::Num)?;
                Ok(n.iter().map(|x| (x - m) * (x - m)).sum())
            })
        },
    ),
    r("GEOMEAN", 1, MANY, "number1, [number2], ...", "The geometric mean.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            if n.is_empty() || n.iter().any(|x| *x <= 0.0) {
                return Err(ErrorKind::Num);
            }
            Ok((n.iter().map(|x| x.ln()).sum::<f64>() / n.len() as f64).exp())
        })
    }),
    r("HARMEAN", 1, MANY, "number1, [number2], ...", "The harmonic mean.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            if n.is_empty() {
                return Err(ErrorKind::NA);
            }
            if n.iter().any(|x| *x <= 0.0) {
                return Err(ErrorKind::Num);
            }
            Ok(n.len() as f64 / n.iter().map(|x| 1.0 / x).sum::<f64>())
        })
    }),
    r("SKEW", 1, MANY, "number1, [number2], ...", "The skewness of a sample.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            let c = n.len() as f64;
            if c < 3.0 {
                return Err(ErrorKind::Div0);
            }
            let m = mean(n).unwrap_or(0.0);
            let sd = sample_variance(n).ok_or(ErrorKind::Div0)?.sqrt();
            if sd == 0.0 {
                return Err(ErrorKind::Div0);
            }
            Ok(c / ((c - 1.0) * (c - 2.0)) * n.iter().map(|x| ((x - m) / sd).powi(3)).sum::<f64>())
        })
    }),
    r("SKEW.P", 1, MANY, "number1, [number2], ...", "The skewness of a population.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            let c = n.len() as f64;
            let m = mean(n).ok_or(ErrorKind::Div0)?;
            let sd = population_variance(n).ok_or(ErrorKind::Div0)?.sqrt();
            if sd == 0.0 {
                return Err(ErrorKind::Div0);
            }
            Ok(n.iter().map(|x| ((x - m) / sd).powi(3)).sum::<f64>() / c)
        })
    }),
    r("KURT", 1, MANY, "number1, [number2], ...", "The kurtosis of a sample.", |ctx, a| {
        over(ctx, a, Gather::Numbers, |n| {
            let c = n.len() as f64;
            if c < 4.0 {
                return Err(ErrorKind::Div0);
            }
            let m = mean(n).unwrap_or(0.0);
            let sd = sample_variance(n).ok_or(ErrorKind::Div0)?.sqrt();
            if sd == 0.0 {
                return Err(ErrorKind::Div0);
            }
            let sum: f64 = n.iter().map(|x| ((x - m) / sd).powi(4)).sum();
            Ok(c * (c + 1.0) / ((c - 1.0) * (c - 2.0) * (c - 3.0)) * sum
                - 3.0 * (c - 1.0).powi(2) / ((c - 2.0) * (c - 3.0)))
        })
    }),
    r(
        "TRIMMEAN",
        2,
        2,
        "array, percent",
        "The mean without a share of the largest and smallest.",
        |ctx, a| {
            let mut n = match gather(ctx, &a[..1], Gather::Numbers) {
                Ok(n) => n,
                Err(e) => return Val::error(e),
            };
            let p = match scalar(ctx, &a[1]) {
                Ok(p) => p,
                Err(e) => return Val::error(e),
            };
            if !(0.0..1.0).contains(&p) || n.is_empty() {
                return Val::error(ErrorKind::Num);
            }
            n.sort_by(cmp);
            let k = ((n.len() as f64 * p) / 2.0).floor() as usize;
            mean(&n[k..n.len() - k]).map_or(Val::error(ErrorKind::Num), Val::number)
        },
    ),
    s("STANDARDIZE", 3, 3, "x, mean, standard_dev", "A normalised value.", |ctx, a| {
        let (x, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        if sd <= 0.0 {
            return Err(ErrorKind::Num);
        }
        number((x - m) / sd)
    }),
    r("CORREL", 2, 2, "array1, array2", "The correlation coefficient.", |ctx, a| {
        paired(ctx, a, correl)
    }),
    r("PEARSON", 2, 2, "array1, array2", "The Pearson correlation coefficient.", |ctx, a| {
        paired(ctx, a, correl)
    }),
    r("RSQ", 2, 2, "known_y's, known_x's", "The square of the correlation.", |ctx, a| {
        paired(ctx, a, |y, x| correl(x, y).map(|r| r * r))
    }),
    r("COVAR", 2, 2, "array1, array2", "The population covariance.", |ctx, a| {
        paired(ctx, a, |x, y| covariance(x, y, false))
    }),
    r("COVARIANCE.P", 2, 2, "array1, array2", "The population covariance.", |ctx, a| {
        paired(ctx, a, |x, y| covariance(x, y, false))
    }),
    r("COVARIANCE.S", 2, 2, "array1, array2", "The sample covariance.", |ctx, a| {
        paired(ctx, a, |x, y| covariance(x, y, true))
    }),
    r("SLOPE", 2, 2, "known_y's, known_x's", "The slope of the regression line.", |ctx, a| {
        paired(ctx, a, |y, x| line(x, y).map(|l| l.0))
    }),
    r(
        "INTERCEPT",
        2,
        2,
        "known_y's, known_x's",
        "Where the regression line crosses the y axis.",
        |ctx, a| paired(ctx, a, |y, x| line(x, y).map(|l| l.1)),
    ),
    r("STEYX", 2, 2, "known_y's, known_x's", "The standard error of the predicted y.", |ctx, a| {
        paired(ctx, a, |y, x| {
            let n = x.len() as f64;
            if n < 3.0 {
                return Err(ErrorKind::Div0);
            }
            let (slope, intercept) = line(x, y)?;
            let residual: f64 =
                x.iter().zip(y).map(|(x, y)| (y - (slope * x + intercept)).powi(2)).sum();
            Ok((residual / (n - 2.0)).sqrt())
        })
    }),
    r("FORECAST", 3, 3, "x, known_y's, known_x's", "A value on the regression line.", forecast),
    r(
        "FORECAST.LINEAR",
        3,
        3,
        "x, known_y's, known_x's",
        "A value on the regression line.",
        forecast,
    ),
    r(
        "TREND",
        1,
        4,
        "known_y's, [known_x's], [new_x's], [const]",
        "Values on the regression line.",
        trend,
    ),
    r(
        "FREQUENCY",
        2,
        2,
        "data_array, bins_array",
        "How many values fall in each bin, as a column.",
        frequency,
    ),
];

pub fn cmp(a: &f64, b: &f64) -> Ordering {
    a.partial_cmp(b).unwrap_or(Ordering::Equal)
}

pub fn mean(n: &[f64]) -> Option<f64> {
    if n.is_empty() {
        None
    } else {
        Some(n.iter().sum::<f64>() / n.len() as f64)
    }
}

fn max(n: &[f64]) -> f64 {
    if n.is_empty() {
        0.0
    } else {
        n.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }
}

fn min(n: &[f64]) -> f64 {
    if n.is_empty() {
        0.0
    } else {
        n.iter().copied().fold(f64::INFINITY, f64::min)
    }
}

/// The variance by two passes, which keeps the rounding Excel's own algorithm has.
pub fn sample_variance(n: &[f64]) -> Option<f64> {
    if n.len() < 2 {
        return None;
    }
    let m = mean(n)?;
    Some(n.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n.len() - 1) as f64)
}

pub fn population_variance(n: &[f64]) -> Option<f64> {
    let m = mean(n)?;
    Some(n.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n.len() as f64)
}

pub fn median(n: &[f64]) -> Option<f64> {
    if n.is_empty() {
        return None;
    }
    let mut v = n.to_vec();
    v.sort_by(cmp);
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[mid - 1] + v[mid]) / 2.0 } else { v[mid] })
}

/// The most frequent numbers in the order each first appears, when one appears more than once.
pub fn modes(n: &[f64]) -> Vec<f64> {
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for x in n {
        match counts.iter_mut().find(|(v, _)| v == x) {
            Some((_, c)) => *c += 1,
            None => counts.push((*x, 1)),
        }
    }
    let best = counts.iter().map(|(_, c)| *c).max().unwrap_or(0);
    if best < 2 {
        return Vec::new();
    }
    counts.into_iter().filter(|(_, c)| *c == best).map(|(v, _)| v).collect()
}

/// The k-th largest (or smallest) of numbers already sorted ascending.
pub fn kth(sorted: &[f64], k: f64, largest: bool) -> Result<f64, ErrorKind> {
    let k = k.ceil();
    if k < 1.0 || k > sorted.len() as f64 {
        return Err(ErrorKind::Num);
    }
    let i = k as usize - 1;
    Ok(if largest { sorted[sorted.len() - 1 - i] } else { sorted[i] })
}

pub fn percentile_inc(sorted: &[f64], k: f64) -> Result<f64, ErrorKind> {
    if sorted.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(ErrorKind::Num);
    }
    let pos = k * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let frac = pos - lo as f64;
    Ok(match sorted.get(lo + 1) {
        Some(hi) => sorted[lo] + frac * (hi - sorted[lo]),
        None => sorted[lo],
    })
}

pub fn percentile_exc(sorted: &[f64], k: f64) -> Result<f64, ErrorKind> {
    let n = sorted.len() as f64;
    if sorted.is_empty() || k <= 0.0 || k >= 1.0 {
        return Err(ErrorKind::Num);
    }
    let pos = k * (n + 1.0);
    if pos < 1.0 || pos > n {
        return Err(ErrorKind::Num);
    }
    let lo = pos.floor() as usize;
    let frac = pos - lo as f64;
    Ok(match sorted.get(lo) {
        Some(hi) => sorted[lo - 1] + frac * (hi - sorted[lo - 1]),
        None => sorted[lo - 1],
    })
}

fn quartile_inc(sorted: &[f64], q: f64) -> Result<f64, ErrorKind> {
    let q = q.trunc();
    if !(0.0..=4.0).contains(&q) {
        return Err(ErrorKind::Num);
    }
    percentile_inc(sorted, q / 4.0)
}

fn scalar(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let v = ctx.eval(e);
    let v = ctx.single(v);
    ctx.number(&v)
}

fn over(ctx: &mut Ctx, args: &[Expr], how: Gather, f: fn(&[f64]) -> Result<f64, ErrorKind>) -> Val {
    match gather(ctx, args, how).and_then(|n| f(&n)) {
        Ok(n) => Val::number(n),
        Err(e) => Val::error(e),
    }
}

fn mode_sngl(ctx: &mut Ctx, args: &[Expr]) -> Val {
    match gather(ctx, args, Gather::OnlyNumbers) {
        Ok(n) => modes(&n).first().map_or(Val::error(ErrorKind::NA), |m| Val::number(*m)),
        Err(e) => Val::error(e),
    }
}

fn averageif(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let Val::Ref(area) = ctx.eval(&args[0]) else { return Val::error(ErrorKind::Value) };
    let avg_area = match args.get(2).map(|e| ctx.eval(e)) {
        None => area,
        Some(Val::Ref(a)) => a,
        Some(_) => return Val::error(ErrorKind::Value),
    };
    let crit = ctx.eval(&args[1]);
    let (tests, rows, cols) = match clipped(ctx, &area) {
        Ok(x) => x,
        Err(e) => return Val::error(e),
    };
    let values = ctx.read_range(avg_area.sheet, aligned(&avg_area, rows, cols));
    let date1904 = ctx.date1904();
    ctx.lift_vals(vec![crit], |_, c| {
        let criterion = Criterion::parse(&c[0], date1904);
        let (mut total, mut count) = (0.0, 0);
        for (i, t) in tests.iter().enumerate() {
            if criterion.matches(t) {
                match values.data.get(i) {
                    Some(Value::Number(n)) => {
                        total += n;
                        count += 1;
                    }
                    Some(Value::Error(e)) => return Value::Error(*e),
                    _ => {}
                }
            }
        }
        if count == 0 {
            Value::Error(ErrorKind::Div0)
        } else {
            Value::Number(total / count as f64)
        }
    })
}

fn ifs_over(ctx: &mut Ctx, args: &[Expr], f: fn(&[f64]) -> Result<f64, ErrorKind>) -> Val {
    let area = match ctx.eval(&args[0]) {
        Val::Ref(area) => area,
        Val::Value(Value::Error(e)) => return Val::error(e),
        _ => return Val::error(ErrorKind::Value),
    };
    let (mask, rows, cols) = match ifs_mask(ctx, &args[1..], Some(area)) {
        Ok(m) => m,
        Err(e) => return Val::error(e),
    };
    let values = ctx.read_range(area.sheet, aligned(&area, rows, cols));
    let mut numbers = Vec::new();
    for (i, keep) in mask.iter().enumerate() {
        if *keep {
            match &values.data[i] {
                Value::Number(n) => numbers.push(*n),
                Value::Error(e) => return Val::error(*e),
                _ => {}
            }
        }
    }
    f(&numbers).map_or_else(Val::error, Val::number)
}

fn count(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let values = ctx.values(args);
    let date1904 = ctx.date1904();
    let n = values
        .iter()
        .filter(|(v, from_range)| match (v, from_range) {
            (Value::Number(_), _) => true,
            (Value::Bool(_), false) => true,
            (Value::Text(t), false) => crate::input::number_from_text(t, date1904).is_some(),
            _ => false,
        })
        .count();
    Val::number(n as f64)
}

fn counta(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let values = ctx.values(args);
    Val::number(
        values.iter().filter(|(v, from_range)| !(v.is_empty() && *from_range)).count() as f64
    )
}

fn countblank(ctx: &mut Ctx, args: &[Expr]) -> Val {
    match ctx.eval(&args[0]) {
        Val::Ref(area) => {
            let mut filled = 0u64;
            ctx.area_values(&area, &mut |v| {
                if !matches!(v, Value::Text(t) if t.is_empty()) {
                    filled += 1;
                }
            });
            Val::number((Ctx::area_size(&area) - filled) as f64)
        }
        Val::Value(Value::Error(e)) => Val::error(e),
        _ => Val::error(ErrorKind::Value),
    }
}

fn countif(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let range = ctx.eval(&args[0]);
    let crit = ctx.eval(&args[1]);
    let (values, total) = match range {
        Val::Ref(area) => match clipped(ctx, &area) {
            Ok((v, ..)) => (v, Ctx::area_size(&area)),
            Err(e) => return Val::error(e),
        },
        Val::Array(a) => {
            let total = a.data.len() as u64;
            (a.data, total)
        }
        Val::Value(Value::Error(e)) => return Val::error(e),
        _ => return Val::error(ErrorKind::Value),
    };
    let date1904 = ctx.date1904();
    let unread = total.saturating_sub(values.len() as u64);
    ctx.lift_vals(vec![crit], |_, c| {
        let criterion = Criterion::parse(&c[0], date1904);
        let mut n = values.iter().filter(|v| criterion.matches(v)).count() as u64;
        if unread > 0 && criterion.matches(&Value::Empty) {
            n += unread;
        }
        Value::Number(n as f64)
    })
}

fn ranked(ctx: &mut Ctx, args: &[Expr], largest: bool) -> Val {
    let mut numbers = Vec::new();
    let v = ctx.eval(&args[0]);
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut numbers) {
        return Val::error(e);
    }
    numbers.sort_by(cmp);
    let k = ctx.eval(&args[1]);
    ctx.lift_vals(vec![k], |ctx, k| match ctx.number(&k[0]) {
        Ok(k) => kth(&numbers, k, largest).map_or_else(Value::Error, Value::Number),
        Err(e) => Value::Error(e),
    })
}

fn sorted_k(ctx: &mut Ctx, args: &[Expr], f: fn(&[f64], f64) -> Result<f64, ErrorKind>) -> Val {
    let mut numbers = Vec::new();
    let v = ctx.eval(&args[0]);
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut numbers) {
        return Val::error(e);
    }
    numbers.sort_by(cmp);
    let k = ctx.eval(&args[1]);
    ctx.lift_vals(vec![k], |ctx, k| match ctx.number(&k[0]) {
        Ok(k) => f(&numbers, k).map_or_else(Value::Error, Value::Number),
        Err(e) => Value::Error(e),
    })
}

fn rank(ctx: &mut Ctx, args: &[Expr], average: bool) -> Val {
    let x = ctx.eval(&args[0]);
    let mut numbers = Vec::new();
    let v = ctx.eval(&args[1]);
    if !matches!(v, Val::Ref(_) | Val::Refs(_)) {
        return Val::error(ErrorKind::Value);
    }
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut numbers) {
        return Val::error(e);
    }
    let ascending = match args.get(2) {
        Some(e) => {
            let v = ctx.eval(e);
            let v = ctx.single(v);
            if v.is_empty() {
                false
            } else {
                match ctx.number(&v) {
                    Ok(n) => n != 0.0,
                    Err(e) => return Val::error(e),
                }
            }
        }
        None => false,
    };
    ctx.lift_vals(vec![x], |ctx, x| {
        let x = match ctx.number(&x[0]) {
            Ok(x) => x,
            Err(e) => return Value::Error(e),
        };
        let better = numbers.iter().filter(|n| if ascending { **n < x } else { **n > x }).count();
        let same = numbers.iter().filter(|n| **n == x).count();
        if same == 0 {
            return Value::Error(ErrorKind::NA);
        }
        Value::Number(if average {
            better as f64 + (same as f64 + 1.0) / 2.0
        } else {
            better as f64 + 1.0
        })
    })
}

fn percentrank(ctx: &mut Ctx, args: &[Expr], exclusive: bool) -> Val {
    let mut numbers = Vec::new();
    let v = ctx.eval(&args[0]);
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut numbers) {
        return Val::error(e);
    }
    numbers.sort_by(cmp);
    let x = match scalar(ctx, &args[1]) {
        Ok(x) => x,
        Err(e) => return Val::error(e),
    };
    let significance = match args.get(2) {
        Some(e) => match scalar(ctx, e) {
            Ok(s) => s.trunc(),
            Err(e) => return Val::error(e),
        },
        None => 3.0,
    };
    if numbers.is_empty() {
        return Val::error(ErrorKind::NA);
    }
    if significance < 1.0 {
        return Val::error(ErrorKind::Num);
    }
    let n = numbers.len();
    if x < numbers[0] || x > numbers[n - 1] {
        return Val::error(ErrorKind::NA);
    }
    // The rank of x by interpolating between the values either side of it.
    let below = numbers.iter().filter(|v| **v < x).count();
    let rank = if numbers.contains(&x) {
        below as f64
    } else {
        let lo = numbers[below - 1];
        let hi = numbers[below];
        (below - 1) as f64 + (x - lo) / (hi - lo)
    };
    // Excel answers 1 for a list of one value, in both forms.
    let fraction = if n == 1 {
        1.0
    } else if exclusive {
        (rank + 1.0) / (n as f64 + 1.0)
    } else {
        rank / (n - 1) as f64
    };
    let scale = 10f64.powf(significance);
    Val::number((crate::value::round_significant(fraction * scale, 15)).floor() / scale)
}

/// Two arrays read as pairs of numbers, keeping only positions where both are numbers.
fn paired(
    ctx: &mut Ctx,
    args: &[Expr],
    f: impl Fn(&[f64], &[f64]) -> Result<f64, ErrorKind>,
) -> Val {
    let (a, ..) = match numbers_at(ctx, &args[0]) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    let (b, ..) = match numbers_at(ctx, &args[1]) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    if a.len() != b.len() {
        return Val::error(ErrorKind::NA);
    }
    let (x, y): (Vec<f64>, Vec<f64>) =
        a.iter().zip(&b).filter_map(|(p, q)| Some(((*p)?, (*q)?))).unzip();
    f(&x, &y).map_or_else(Val::error, Val::number)
}

fn correl(x: &[f64], y: &[f64]) -> Result<f64, ErrorKind> {
    let (mx, my) = (mean(x).ok_or(ErrorKind::Div0)?, mean(y).ok_or(ErrorKind::Div0)?);
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    let syy: f64 = y.iter().map(|b| (b - my).powi(2)).sum();
    if sxx == 0.0 || syy == 0.0 {
        return Err(ErrorKind::Div0);
    }
    Ok(sxy / (sxx * syy).sqrt())
}

fn covariance(x: &[f64], y: &[f64], sample: bool) -> Result<f64, ErrorKind> {
    let n = x.len();
    if n == 0 || sample && n < 2 {
        return Err(ErrorKind::Div0);
    }
    let (mx, my) = (mean(x).unwrap_or(0.0), mean(y).unwrap_or(0.0));
    let s: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    Ok(s / if sample { (n - 1) as f64 } else { n as f64 })
}

/// The least squares line through points: slope and intercept.
pub fn line(x: &[f64], y: &[f64]) -> Result<(f64, f64), ErrorKind> {
    let (mx, my) = (mean(x).ok_or(ErrorKind::Div0)?, mean(y).ok_or(ErrorKind::Div0)?);
    let sxx: f64 = x.iter().map(|a| (a - mx).powi(2)).sum();
    if sxx == 0.0 {
        return Err(ErrorKind::Div0);
    }
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let slope = sxy / sxx;
    Ok((slope, my - slope * mx))
}

fn forecast(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let x = ctx.eval(&args[0]);
    let line = match paired_line(ctx, &args[1], &args[2]) {
        Ok(l) => l,
        Err(e) => return Val::error(e),
    };
    ctx.lift_vals(vec![x], |ctx, x| match ctx.number(&x[0]) {
        Ok(x) => Value::Number(line.0 * x + line.1),
        Err(e) => Value::Error(e),
    })
}

fn paired_line(ctx: &mut Ctx, ys: &Expr, xs: &Expr) -> Result<(f64, f64), ErrorKind> {
    let (a, ..) = numbers_at(ctx, ys)?;
    let (b, ..) = numbers_at(ctx, xs)?;
    if a.len() != b.len() {
        return Err(ErrorKind::NA);
    }
    let (y, x): (Vec<f64>, Vec<f64>) =
        a.iter().zip(&b).filter_map(|(p, q)| Some(((*p)?, (*q)?))).unzip();
    line(&x, &y)
}

fn trend(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let (ys, rows, cols) = match numbers_at(ctx, &args[0]) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    let xs: Vec<Option<f64>> = match args.get(1).filter(|e| !matches!(e, Expr::Missing)) {
        Some(e) => match numbers_at(ctx, e) {
            Ok((v, ..)) => v,
            Err(e) => return Val::error(e),
        },
        None => (1..=ys.len()).map(|i| Some(i as f64)).collect(),
    };
    if xs.len() != ys.len() {
        return Val::error(ErrorKind::Ref);
    }
    let with_constant = match args.get(3) {
        Some(e) if !matches!(e, Expr::Missing) => {
            let v = ctx.eval(e);
            let v = ctx.single(v);
            crate::eval::to_bool(&v).unwrap_or(true)
        }
        _ => true,
    };
    let (y, x): (Vec<f64>, Vec<f64>) =
        ys.iter().zip(&xs).filter_map(|(p, q)| Some(((*p)?, (*q)?))).unzip();
    if y.is_empty() {
        return Val::error(ErrorKind::Value);
    }
    let all_alike = x.iter().all(|v| *v == x[0]);
    let (slope, intercept) = if with_constant && all_alike {
        // Every x is the same, so the line is flat through the mean of y, as Excel's least squares gives.
        (0.0, mean(&y).unwrap_or(0.0))
    } else if with_constant {
        match line(&x, &y) {
            Ok(l) => l,
            Err(e) => return Val::error(e),
        }
    } else {
        let sxx: f64 = x.iter().map(|a| a * a).sum();
        if sxx == 0.0 {
            return Val::error(ErrorKind::Div0);
        }
        (x.iter().zip(&y).map(|(a, b)| a * b).sum::<f64>() / sxx, 0.0)
    };
    let (new_x, nr, nc) = match args.get(2).filter(|e| !matches!(e, Expr::Missing)) {
        Some(e) => match numbers_at(ctx, e) {
            Ok(v) => v,
            Err(e) => return Val::error(e),
        },
        None => (xs.clone(), rows, cols),
    };
    let data = new_x
        .iter()
        .map(|x| x.map_or(Value::Error(ErrorKind::Value), |x| Value::Number(slope * x + intercept)))
        .collect();
    Val::Array(Array::new(nr, nc, data))
}

fn frequency(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut data = Vec::new();
    let v = ctx.eval(&args[0]);
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut data) {
        return Val::error(e);
    }
    let mut bins = Vec::new();
    let v = ctx.eval(&args[1]);
    if let Err(e) = gather_val(ctx, v, Gather::Numbers, &mut bins) {
        return Val::error(e);
    }
    let mut order: Vec<usize> = (0..bins.len()).collect();
    order.sort_by(|a, b| cmp(&bins[*a], &bins[*b]));
    let mut counts = vec![0.0; bins.len() + 1];
    for x in data {
        match order.iter().find(|i| x <= bins[**i]) {
            Some(i) => counts[*i] += 1.0,
            None => counts[bins.len()] += 1.0,
        }
    }
    Val::Array(Array::column(counts.into_iter().map(Value::Number).collect()))
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{book, eval_on, shown};

    #[test]
    fn statistics_answer_what_excel_answers() {
        let b = book(&[
            ("A1", "1"),
            ("A2", "2"),
            ("A3", "3"),
            ("A4", "4"),
            ("A5", "text"),
            ("A6", "TRUE"),
            ("A7", ""),
            ("B1", "10"),
            ("B2", "20"),
            ("B3", "30"),
            ("B4", "40"),
            ("C1", "apple"),
            ("C2", "banana"),
            ("C3", "apple"),
            ("C4", "cherry"),
        ]);
        let cases = [
            ("AVERAGE(A1:A7)", "2.5"),
            ("AVERAGEA(A1:A6)", "1.83333333333"),
            ("COUNT(A1:A7)", "4"),
            ("COUNTA(A1:A7)", "6"),
            ("COUNTBLANK(A1:A7)", "1"),
            ("COUNTIF(C1:C4,\"apple\")", "2"),
            ("COUNTIF(A1:A4,\">2\")", "2"),
            ("SUMIF(C1:C4,\"apple\",B1:B4)", "40"),
            ("SUMIFS(B1:B4,C1:C4,\"apple\",A1:A4,\">1\")", "30"),
            ("COUNTIFS(C1:C4,\"a*\",B1:B4,\"<25\")", "1"),
            ("AVERAGEIF(C1:C4,\"apple\",B1:B4)", "20"),
            ("MAXIFS(B1:B4,C1:C4,\"apple\")", "30"),
            ("MAX(A1:A7)", "4"),
            ("MIN(B1:B4)", "10"),
            ("MEDIAN(1,2,3,4)", "2.5"),
            ("MODE(1,2,2,3)", "2"),
            ("LARGE(B1:B4,2)", "30"),
            ("SMALL(B1:B4,1)", "10"),
            ("RANK(20,B1:B4)", "3"),
            ("RANK(20,B1:B4,1)", "2"),
            ("PERCENTILE(B1:B4,0.25)", "17.5"),
            ("QUARTILE(B1:B4,3)", "32.5"),
            ("STDEV(B1:B4)", "12.9099444874"),
            ("STDEVP(B1:B4)", "11.1803398875"),
            ("VAR(1,2,3,4)", "1.66666666667"),
            ("CORREL(A1:A4,B1:B4)", "1"),
            ("SLOPE(B1:B4,A1:A4)", "10"),
            ("INTERCEPT(B1:B4,A1:A4)", "0"),
            ("FORECAST(5,B1:B4,A1:A4)", "50"),
            ("GEOMEAN(1,2,4)", "2"),
            ("HARMEAN(1,2,4)", "1.71428571429"),
            ("PERCENTRANK(B1:B4,25)", "0.5"),
        ];
        let mut wrong = Vec::new();
        for (f, want) in cases {
            let got = shown(&eval_on(&b, f));
            if got != want {
                wrong.push(format!("{f}: expected {want}, got {got}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
