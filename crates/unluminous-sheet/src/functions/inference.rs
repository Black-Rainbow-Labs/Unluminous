//! Inverse distributions, Fisher's transform, a confidence interval from Student's t and the
//! hypothesis tests, with the older names Excel keeps for them.
//!
//! The inverses are found by bisection on the cumulative distribution, run until the bracket is two
//! neighbouring doubles, so they are limited only by how well the distribution itself is computed. A
//! probability above one half is solved from its complement, which keeps the digits of a result near
//! 1 or near a tail that a subtraction from 1 would lose.

use super::dist::{beta_i, gamma_p, gamma_q, normal_cdf, normal_inverse, t_cdf};
use super::stats::{mean, sample_variance};
use super::{gather, int, num, number, numbers_at, opt, r, s, Function, Gather};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s(
        "BETA.INV",
        3,
        5,
        "probability, alpha, beta, [A], [B]",
        "The inverse of the cumulative beta distribution.",
        beta_inv,
    ),
    s(
        "BETAINV",
        3,
        5,
        "probability, alpha, beta, [A], [B]",
        "The inverse of the cumulative beta distribution.",
        beta_inv,
    ),
    s(
        "BETADIST",
        3,
        5,
        "x, alpha, beta, [A], [B]",
        "The cumulative beta distribution.",
        beta_dist_legacy,
    ),
    s(
        "F.INV",
        3,
        3,
        "probability, deg_freedom1, deg_freedom2",
        "The inverse of the F distribution.",
        |ctx, a| f_inverse_value(ctx, a, false),
    ),
    s(
        "F.INV.RT",
        3,
        3,
        "probability, deg_freedom1, deg_freedom2",
        "The inverse of the right tail of the F distribution.",
        |ctx, a| f_inverse_value(ctx, a, true),
    ),
    s(
        "FINV",
        3,
        3,
        "probability, deg_freedom1, deg_freedom2",
        "The inverse of the right tail of the F distribution.",
        |ctx, a| f_inverse_value(ctx, a, true),
    ),
    s(
        "GAMMA.INV",
        3,
        3,
        "probability, alpha, beta",
        "The inverse of the gamma distribution.",
        gamma_inv,
    ),
    s(
        "GAMMAINV",
        3,
        3,
        "probability, alpha, beta",
        "The inverse of the gamma distribution.",
        gamma_inv,
    ),
    s(
        "LOGINV",
        3,
        3,
        "probability, mean, standard_dev",
        "The inverse of the lognormal distribution.",
        |ctx, a| {
            let (p, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
            if p <= 0.0 || p >= 1.0 || sd <= 0.0 {
                return Err(ErrorKind::Num);
            }
            number((m + sd * normal_inverse(p)).exp())
        },
    ),
    s(
        "LOGNORMDIST",
        3,
        3,
        "x, mean, standard_dev",
        "The cumulative lognormal distribution.",
        |ctx, a| {
            let (x, m, sd) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
            if x <= 0.0 || sd <= 0.0 {
                return Err(ErrorKind::Num);
            }
            number(normal_cdf((x.ln() - m) / sd))
        },
    ),
    s(
        "CONFIDENCE.T",
        3,
        3,
        "alpha, standard_dev, size",
        "The confidence interval for a mean, Student's t distribution.",
        confidence_t,
    ),
    s("FISHER", 1, 1, "x", "Fisher's transformation.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        if x <= -1.0 || x >= 1.0 {
            return Err(ErrorKind::Num);
        }
        number(0.5 * ((1.0 + x) / (1.0 - x)).ln())
    }),
    s("FISHERINV", 1, 1, "y", "The inverse of Fisher's transformation.", |ctx, a| {
        number(num(ctx, &a[0])?.tanh())
    }),
    r(
        "CHISQ.TEST",
        2,
        2,
        "actual_range, expected_range",
        "The chi-squared test for independence.",
        chisq_test,
    ),
    r(
        "CHITEST",
        2,
        2,
        "actual_range, expected_range",
        "The chi-squared test for independence.",
        chisq_test,
    ),
    r("F.TEST", 2, 2, "array1, array2", "The two tailed probability the variances differ.", f_test),
    r("FTEST", 2, 2, "array1, array2", "The two tailed probability the variances differ.", f_test),
    r(
        "T.TEST",
        4,
        4,
        "array1, array2, tails, type",
        "The probability from a Student's t test.",
        t_test,
    ),
    r(
        "TTEST",
        4,
        4,
        "array1, array2, tails, type",
        "The probability from a Student's t test.",
        t_test,
    ),
    r(
        "Z.TEST",
        2,
        3,
        "array, x, [sigma]",
        "The one tailed probability from a z test.",
        z_test,
    ),
    r(
        "ZTEST",
        2,
        3,
        "array, x, [sigma]",
        "The one tailed probability from a z test.",
        z_test,
    ),
    r(
        "PROB",
        3,
        4,
        "x_range, prob_range, lower_limit, [upper_limit]",
        "The probability that values are between two limits.",
        prob,
    ),
];

/// Solves `f(x) = target` for an increasing `f`, bisecting from `lo` to `hi` until the bracket is two
/// neighbouring doubles. With `open_ended` the upper end is doubled until `f` reaches the target.
fn solve_increasing(f: impl Fn(f64) -> f64, target: f64, lo: f64, hi: f64, open_ended: bool) -> f64 {
    let (mut lo, mut hi) = (lo, hi);
    if open_ended {
        let mut doublings = 0;
        while f(hi) < target && doublings < 1100 {
            lo = hi;
            hi *= 2.0;
            doublings += 1;
        }
    }
    for _ in 0..2200 {
        let mid = lo + (hi - lo) / 2.0;
        if mid <= lo || mid >= hi {
            break;
        }
        if f(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo + (hi - lo) / 2.0
}

/// The x with I_x(a, b) = p and its complement 1 - x, each found directly so neither loses digits.
fn beta_inverse_pair(p: f64, a: f64, b: f64) -> (f64, f64) {
    if p <= 0.5 {
        let x = solve_increasing(|x| beta_i(x, a, b), p, 0.0, 1.0, false);
        (x, 1.0 - x)
    } else {
        let y = solve_increasing(|y| beta_i(y, b, a), 1.0 - p, 0.0, 1.0, false);
        (1.0 - y, y)
    }
}

/// A number argument that is left empty falls back to `default`.
fn number_or(ctx: &Ctx, a: &[Value], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match opt(a, i) {
        Some(v) => num(ctx, v),
        None => Ok(default),
    }
}

fn beta_inv(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, alpha, beta) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    let (low, high) = (number_or(ctx, a, 3, 0.0)?, number_or(ctx, a, 4, 1.0)?);
    if p <= 0.0 || p >= 1.0 || alpha <= 0.0 || beta <= 0.0 || low >= high {
        return Err(ErrorKind::Num);
    }
    number(low + (high - low) * beta_inverse_pair(p, alpha, beta).0)
}

fn beta_dist_legacy(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (x, alpha, beta) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    let (low, high) = (number_or(ctx, a, 3, 0.0)?, number_or(ctx, a, 4, 1.0)?);
    if alpha <= 0.0 || beta <= 0.0 || low == high || x < low || x > high {
        return Err(ErrorKind::Num);
    }
    number(beta_i((x - low) / (high - low), alpha, beta))
}

/// F.INV, or F.INV.RT and FINV when `right` is set.
fn f_inverse_value(ctx: &mut Ctx, a: &[Value], right: bool) -> Result<Value, ErrorKind> {
    let (p, d1, d2) = (num(ctx, &a[0])?, int(ctx, &a[1])? as f64, int(ctx, &a[2])? as f64);
    let in_range = if right { p > 0.0 && p <= 1.0 } else { (0.0..1.0).contains(&p) };
    if !in_range || d1 < 1.0 || d2 < 1.0 {
        return Err(ErrorKind::Num);
    }
    if p == 0.0 || (right && p == 1.0) {
        return number(0.0);
    }
    let x = if right {
        let (u, t) = beta_inverse_pair(p, d2 / 2.0, d1 / 2.0);
        d2 * t / (d1 * u)
    } else {
        let (t, u) = beta_inverse_pair(p, d1 / 2.0, d2 / 2.0);
        d2 * t / (d1 * u)
    };
    number(x)
}

fn gamma_inv(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (p, alpha, beta) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
    if !(0.0..1.0).contains(&p) || alpha <= 0.0 || beta <= 0.0 {
        return Err(ErrorKind::Num);
    }
    if p == 0.0 {
        return number(0.0);
    }
    let start = alpha.max(1.0);
    let x = if p <= 0.5 {
        solve_increasing(|x| gamma_p(alpha, x), p, 0.0, start, true)
    } else {
        solve_increasing(|x| -gamma_q(alpha, x), -(1.0 - p), 0.0, start, true)
    };
    number(x * beta)
}

/// The t that leaves `alpha` in both tails together, for `df` degrees of freedom.
fn t_two_tail_inverse(alpha: f64, df: f64) -> f64 {
    let (u, w) = beta_inverse_pair(alpha, df / 2.0, 0.5);
    (df * w / u).sqrt()
}

fn confidence_t(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let (alpha, sd, size) = (num(ctx, &a[0])?, num(ctx, &a[1])?, int(ctx, &a[2])?);
    if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || size < 1 {
        return Err(ErrorKind::Num);
    }
    if size == 1 {
        return Err(ErrorKind::Div0);
    }
    let n = size as f64;
    number(t_two_tail_inverse(alpha, n - 1.0) * sd / n.sqrt())
}

/// A formula argument read as one number.
fn scalar_number(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let v = ctx.eval(e);
    let v = ctx.single(v);
    ctx.number(&v)
}

/// The numbers in an array argument, in order, leaving out text, logicals and empty cells.
fn numbers_of(ctx: &mut Ctx, e: &Expr) -> Result<Vec<f64>, ErrorKind> {
    gather(ctx, std::slice::from_ref(e), Gather::Numbers)
}

/// A function result as the value a raw function returns.
fn finish(result: Result<f64, ErrorKind>) -> Val {
    result.map_or_else(Val::error, Val::number)
}

fn chisq_test(ctx: &mut Ctx, args: &[Expr]) -> Val {
    finish(chisq_test_value(ctx, args))
}

fn chisq_test_value(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let (actual, rows, cols) = numbers_at(ctx, &args[0])?;
    let (expected, ..) = numbers_at(ctx, &args[1])?;
    if actual.len() != expected.len() {
        return Err(ErrorKind::NA);
    }
    let df = if rows == 1 || cols == 1 { (rows * cols) as f64 - 1.0 } else { ((rows - 1) * (cols - 1)) as f64 };
    if df < 1.0 {
        return Err(ErrorKind::NA);
    }
    let (mut statistic, mut pairs) = (0.0, 0);
    for (observed, wanted) in actual.iter().zip(&expected) {
        let (Some(o), Some(e)) = (observed, wanted) else { continue };
        if *e < 0.0 {
            return Err(ErrorKind::Num);
        }
        if *e == 0.0 {
            return Err(ErrorKind::Div0);
        }
        statistic += (o - e) * (o - e) / e;
        pairs += 1;
    }
    if pairs == 0 {
        return Err(ErrorKind::Div0);
    }
    Ok(gamma_q(df / 2.0, statistic / 2.0))
}

fn f_test(ctx: &mut Ctx, args: &[Expr]) -> Val {
    finish(f_test_value(ctx, args))
}

fn f_test_value(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let first = numbers_of(ctx, &args[0])?;
    let second = numbers_of(ctx, &args[1])?;
    if first.len() < 2 || second.len() < 2 {
        return Err(ErrorKind::Div0);
    }
    let (v1, v2) = (sample_variance(&first).unwrap_or(0.0), sample_variance(&second).unwrap_or(0.0));
    if v1 == 0.0 || v2 == 0.0 {
        return Err(ErrorKind::Div0);
    }
    let (d1, d2, f) = ((first.len() - 1) as f64, (second.len() - 1) as f64, v1 / v2);
    let right = beta_i(d2 / (d2 + d1 * f), d2 / 2.0, d1 / 2.0);
    let left = beta_i(d1 * f / (d1 * f + d2), d1 / 2.0, d2 / 2.0);
    Ok((2.0 * right.min(left)).min(1.0))
}

fn t_test(ctx: &mut Ctx, args: &[Expr]) -> Val {
    finish(t_test_value(ctx, args))
}

fn t_test_value(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let (first, ..) = numbers_at(ctx, &args[0])?;
    let (second, ..) = numbers_at(ctx, &args[1])?;
    let tails = scalar_number(ctx, &args[2])?.trunc();
    let kind = scalar_number(ctx, &args[3])?.trunc();
    if tails != 1.0 && tails != 2.0 || !(1.0..4.0).contains(&kind) {
        return Err(ErrorKind::Num);
    }
    let (t, df) = match kind as u8 {
        1 => paired_t(&first, &second)?,
        2 => pooled_t(&present(&first), &present(&second))?,
        _ => welch_t(&present(&first), &present(&second))?,
    };
    let both = beta_i(df / (df + t * t), df / 2.0, 0.5);
    Ok(if tails == 1.0 { both / 2.0 } else { both })
}

/// The numbers among values that may be missing.
fn present(values: &[Option<f64>]) -> Vec<f64> {
    values.iter().flatten().copied().collect()
}

/// The t statistic and its degrees of freedom for paired samples, dropping pairs with a gap.
fn paired_t(first: &[Option<f64>], second: &[Option<f64>]) -> Result<(f64, f64), ErrorKind> {
    if first.len() != second.len() {
        return Err(ErrorKind::NA);
    }
    let differences: Vec<f64> = first
        .iter()
        .zip(second)
        .filter_map(|(a, b)| Some((*a)? - (*b)?))
        .collect();
    let n = differences.len() as f64;
    let variance = sample_variance(&differences).unwrap_or(0.0);
    if differences.len() < 2 || variance == 0.0 {
        return Err(ErrorKind::Div0);
    }
    Ok((mean(&differences).unwrap_or(0.0) / (variance / n).sqrt(), n - 1.0))
}

/// The t statistic for two samples assumed to have the same variance.
fn pooled_t(first: &[f64], second: &[f64]) -> Result<(f64, f64), ErrorKind> {
    if first.len() < 2 || second.len() < 2 {
        return Err(ErrorKind::Div0);
    }
    let (n1, n2) = (first.len() as f64, second.len() as f64);
    let (v1, v2) = (sample_variance(first).unwrap_or(0.0), sample_variance(second).unwrap_or(0.0));
    let pooled = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
    let error = pooled * (1.0 / n1 + 1.0 / n2);
    if error == 0.0 {
        return Err(ErrorKind::Div0);
    }
    let difference = mean(first).unwrap_or(0.0) - mean(second).unwrap_or(0.0);
    Ok((difference / error.sqrt(), n1 + n2 - 2.0))
}

/// The t statistic for two samples of unequal variance, with Welch's degrees of freedom.
fn welch_t(first: &[f64], second: &[f64]) -> Result<(f64, f64), ErrorKind> {
    if first.len() < 2 || second.len() < 2 {
        return Err(ErrorKind::Div0);
    }
    let (n1, n2) = (first.len() as f64, second.len() as f64);
    let a = sample_variance(first).unwrap_or(0.0) / n1;
    let b = sample_variance(second).unwrap_or(0.0) / n2;
    if a + b == 0.0 {
        return Err(ErrorKind::Div0);
    }
    let difference = mean(first).unwrap_or(0.0) - mean(second).unwrap_or(0.0);
    let df = (a + b) * (a + b) / (a * a / (n1 - 1.0) + b * b / (n2 - 1.0));
    Ok((difference / (a + b).sqrt(), df))
}

fn z_test(ctx: &mut Ctx, args: &[Expr]) -> Val {
    finish(z_test_value(ctx, args))
}

fn z_test_value(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let data = numbers_of(ctx, &args[0])?;
    let x = scalar_number(ctx, &args[1])?;
    let sigma = match args.get(2) {
        Some(e) => Some(scalar_number(ctx, e)?),
        None => None,
    };
    match data.len() {
        0 => return Err(ErrorKind::NA),
        1 => return Err(ErrorKind::Div0),
        _ => {}
    }
    let deviation = match sigma {
        Some(s) if s <= 0.0 => return Err(ErrorKind::Num),
        Some(s) => s,
        None => match sample_variance(&data) {
            Some(v) if v > 0.0 => v.sqrt(),
            _ => return Err(ErrorKind::Div0),
        },
    };
    let z = (mean(&data).unwrap_or(0.0) - x) / (deviation / (data.len() as f64).sqrt());
    Ok(normal_cdf(-z))
}

fn prob(ctx: &mut Ctx, args: &[Expr]) -> Val {
    finish(prob_value(ctx, args))
}

fn prob_value(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let (xs, ..) = numbers_at(ctx, &args[0])?;
    let (ps, ..) = numbers_at(ctx, &args[1])?;
    if xs.len() != ps.len() {
        return Err(ErrorKind::NA);
    }
    let lower = scalar_number(ctx, &args[2])?;
    let upper = match args.get(3) {
        Some(e) => Some(scalar_number(ctx, e)?),
        None => None,
    };
    let pairs: Vec<(f64, f64)> = xs.iter().zip(&ps).filter_map(|(x, p)| Some(((*x)?, (*p)?))).collect();
    let total: f64 = pairs.iter().map(|(_, p)| p).sum();
    if (total - 1.0).abs() > 9e-16 {
        return Err(ErrorKind::Num);
    }
    let inside = |x: f64| match upper {
        Some(u) => x >= lower && x <= u,
        None => x == lower,
    };
    Ok(pairs.iter().filter(|(x, _)| inside(*x)).map(|(_, p)| p).sum())
}
