//! Least squares fits of any number of variables: LINEST, LOGEST, GROWTH and TREND.
//!
//! The fit is a Householder QR decomposition that takes the column with the most left in it at every
//! step, which is how Excel finds a column that is a combination of the others: when what is left of
//! the best remaining column falls below 6.74e-7 of the first one (measured against Excel by
//! shifting a column by amounts either side), that column and every one after it are dropped. A
//! dropped column gets the coefficient 0 and the standard error 0 and takes a degree of freedom away
//! from the count. With an intercept the columns and the values are centred first, so a column that
//! is a constant has nothing left in it, and the intercept is worked out from the means afterwards.

use super::{r, Function};
use crate::eval::{to_bool, Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "LINEST",
        1,
        4,
        "known_y's, [known_x's], [const], [stats]",
        "The straight line, or plane, that best fits the points.",
        |ctx, a| fit_table(ctx, a, false),
    ),
    r(
        "LOGEST",
        1,
        4,
        "known_y's, [known_x's], [const], [stats]",
        "The exponential curve that best fits the points.",
        |ctx, a| fit_table(ctx, a, true),
    ),
    r(
        "TREND",
        1,
        4,
        "known_y's, [known_x's], [new_x's], [const]",
        "Values on the line that best fits the points.",
        |ctx, a| predict(ctx, a, false),
    ),
    r(
        "GROWTH",
        1,
        4,
        "known_y's, [known_x's], [new_x's], [const]",
        "Values on the exponential curve that best fits the points.",
        |ctx, a| predict(ctx, a, true),
    ),
];

/// What is left of a column, against the first column's, below which it counts as a combination of
/// the columns before it.
const DROP_RATIO: f64 = 6.74e-7;

/// The known values of a regression: the y values and one list of numbers for each x variable.
struct Known {
    y: Vec<f64>,
    x: Vec<Vec<f64>>,
    /// The y values were given in a row, so each variable is a row of x as well.
    by_rows: bool,
}

/// A fitted line: a coefficient and a standard error for every x variable, in the order given.
struct Fit {
    coefficients: Vec<f64>,
    errors: Vec<f64>,
    intercept: f64,
    intercept_error: f64,
    constant: bool,
    kept: usize,
    ssreg: f64,
    ssresid: f64,
    df: f64,
}

/// A list of values as numbers: an error anywhere is the answer, anything else that is not a number
/// is a `#VALUE!`.
fn numbers_in(values: &[Value]) -> Result<Vec<f64>, ErrorKind> {
    if let Some(Value::Error(e)) = values.iter().find(|v| v.is_error()) {
        return Err(*e);
    }
    values
        .iter()
        .map(|v| match v {
            Value::Number(n) => Ok(*n),
            _ => Err(ErrorKind::Value),
        })
        .collect()
}

/// An argument evaluated to an array, leaving out nothing.
fn array_of(ctx: &mut Ctx, e: &Expr) -> Result<Array, ErrorKind> {
    let v = ctx.eval(e);
    ctx.array(v)
}

/// The argument at `i`, unless it was left out or left empty between commas.
fn present(args: &[Expr], i: usize) -> Option<&Expr> {
    args.get(i).filter(|e| !matches!(e, Expr::Missing))
}

/// A yes or no argument, with a default for one that is left out.
fn flag(ctx: &mut Ctx, args: &[Expr], i: usize, default: bool) -> Result<bool, ErrorKind> {
    let Some(e) = present(args, i) else { return Ok(default) };
    let v = ctx.eval(e);
    let v = ctx.single(v);
    if v.is_empty() {
        Ok(false)
    } else {
        to_bool(&v)
    }
}

/// The y values and the x variables an argument list names, checked against each other.
fn read_known(ctx: &mut Ctx, args: &[Expr]) -> Result<Known, ErrorKind> {
    let y = array_of(ctx, &args[0])?;
    if y.rows != 1 && y.cols != 1 {
        return Err(ErrorKind::Ref);
    }
    let by_rows = y.rows == 1 && y.cols > 1;
    let values = numbers_in(&y.data)?;
    let n = values.len();
    let x = match present(args, 1) {
        Some(e) => {
            let x = array_of(ctx, e)?;
            let (points, variables) = if by_rows { (x.cols, x.rows) } else { (x.rows, x.cols) };
            if points != n || variables == 0 {
                return Err(ErrorKind::Ref);
            }
            variables_of(&x, by_rows)?
        }
        None => vec![(1..=n).map(|i| i as f64).collect()],
    };
    Ok(Known { y: values, x, by_rows })
}

/// An array split into one list of numbers for each variable.
fn variables_of(x: &Array, by_rows: bool) -> Result<Vec<Vec<f64>>, ErrorKind> {
    let numbers = numbers_in(&x.data)?;
    Ok(if by_rows {
        numbers.chunks(x.cols).map(<[f64]>::to_vec).collect()
    } else {
        (0..x.cols).map(|c| (0..x.rows).map(|r| numbers[r * x.cols + c]).collect()).collect()
    })
}

fn mean_of(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn norm_from(column: &[f64], start: usize) -> f64 {
    column[start..].iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// The position among `order[step..]` whose column has the most left below row `step`, and how much.
/// With an intercept a tie goes to the later column and without one to the earlier, as Excel does.
fn largest_remaining(work: &[Vec<f64>], order: &[usize], step: usize, tie_to_last: bool) -> (usize, f64) {
    let mut best = (step, -1.0);
    for pos in step..order.len() {
        let norm = norm_from(&work[order[pos]], step);
        if norm > best.1 || (tie_to_last && norm == best.1) {
            best = (pos, norm);
        }
    }
    best
}

/// One Householder step: turn column `order[step]` into a single value on the diagonal and apply the
/// same reflection to the columns after it and to the right hand side.
fn reflect(work: &mut [Vec<f64>], rhs: &mut [f64], order: &[usize], step: usize, norm: f64) {
    let column = order[step];
    let alpha = if work[column][step] > 0.0 { -norm } else { norm };
    let mut v = work[column][step..].to_vec();
    v[0] -= alpha;
    let vv: f64 = v.iter().map(|x| x * x).sum();
    let apply = |target: &mut [f64]| {
        let dot: f64 = v.iter().zip(target.iter()).map(|(a, b)| a * b).sum();
        let scale = 2.0 * dot / vv;
        for (t, a) in target.iter_mut().zip(&v) {
            *t -= scale * a;
        }
    };
    for &other in &order[step + 1..] {
        apply(&mut work[other][step..]);
    }
    apply(&mut rhs[step..]);
    work[column][step] = alpha;
}

/// Reduce the columns to upper triangular form, returning their final order and how many are kept.
fn pivoted_qr(work: &mut [Vec<f64>], rhs: &mut [f64], tie_to_last: bool) -> (Vec<usize>, usize) {
    let mut order: Vec<usize> = (0..work.len()).collect();
    let (mut rank, mut first) = (0, 0.0);
    for step in 0..work.len().min(rhs.len()) {
        let (pos, norm) = largest_remaining(work, &order, step, tie_to_last);
        if step == 0 {
            first = norm;
        }
        if norm == 0.0 || norm <= DROP_RATIO * first {
            break;
        }
        order.swap(step, pos);
        reflect(work, rhs, &order, step, norm);
        rank += 1;
    }
    (order, rank)
}

/// The inverse of an upper triangular matrix.
fn invert_upper(r: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = r.len();
    let mut inverse = vec![vec![0.0; n]; n];
    for j in 0..n {
        inverse[j][j] = 1.0 / r[j][j];
        for i in (0..j).rev() {
            let sum: f64 = (i + 1..=j).map(|l| r[i][l] * inverse[l][j]).sum();
            inverse[i][j] = -sum / r[i][i];
        }
    }
    inverse
}

/// R^-1 times its transpose, which is the covariance of the coefficients before scaling.
fn covariance(inverse: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = inverse.len();
    let mut c = vec![vec![0.0; n]; n];
    for a in 0..n {
        for b in 0..n {
            c[a][b] = (a.max(b)..n).map(|l| inverse[a][l] * inverse[b][l]).sum();
        }
    }
    c
}

/// The least squares fit of `y` on the columns, with or without an intercept.
fn fit(y: &[f64], columns: &[Vec<f64>], constant: bool) -> Fit {
    let (n, k) = (y.len(), columns.len());
    let means: Vec<f64> = columns.iter().map(|c| if constant { mean_of(c) } else { 0.0 }).collect();
    let y_mean = if constant { mean_of(y) } else { 0.0 };
    let mut work: Vec<Vec<f64>> =
        columns.iter().zip(&means).map(|(c, m)| c.iter().map(|v| v - m).collect()).collect();
    let mut rhs: Vec<f64> = y.iter().map(|v| v - y_mean).collect();
    let (order, rank) = pivoted_qr(&mut work, &mut rhs, constant);
    let triangle: Vec<Vec<f64>> =
        (0..rank).map(|i| (0..rank).map(|p| if p >= i { work[order[p]][i] } else { 0.0 }).collect()).collect();
    let mut beta = vec![0.0; rank];
    for i in (0..rank).rev() {
        let tail: f64 = (i + 1..rank).map(|p| triangle[i][p] * beta[p]).sum();
        beta[i] = (rhs[i] - tail) / triangle[i][i];
    }
    let ssresid: f64 = rhs[rank..n].iter().map(|v| v * v).sum();
    let ssreg: f64 = rhs[..rank].iter().map(|v| v * v).sum();
    let df = n.saturating_sub(rank + usize::from(constant)) as f64;
    let sey = if df > 0.0 { (ssresid / df).sqrt() } else { 0.0 };
    let cov = covariance(&invert_upper(&triangle));
    let mut coefficients = vec![0.0; k];
    let mut errors = vec![0.0; k];
    for p in 0..rank {
        coefficients[order[p]] = beta[p];
        errors[order[p]] = sey * cov[p][p].sqrt();
    }
    let kept_means: Vec<f64> = order[..rank].iter().map(|&c| means[c]).collect();
    let intercept = y_mean - (0..rank).map(|p| beta[p] * kept_means[p]).sum::<f64>();
    let spread: f64 = (0..rank)
        .map(|a| (0..rank).map(|b| kept_means[a] * cov[a][b] * kept_means[b]).sum::<f64>())
        .sum();
    let intercept_error = sey * (1.0 / n as f64 + spread).sqrt();
    Fit { coefficients, errors, intercept, intercept_error, constant, kept: rank, ssreg, ssresid, df }
}

/// The coefficients and, when asked for, the five rows of statistics, as Excel lays them out.
fn table(fit: &Fit, with_stats: bool, exponential: bool) -> Array {
    let n = |v: f64| Value::Number(v);
    let shape = |v: f64| n(if exponential { v.exp() } else { v });
    let na = || Value::Error(ErrorKind::NA);
    let mut first: Vec<Value> = fit.coefficients.iter().rev().map(|c| shape(*c)).collect();
    first.push(shape(fit.intercept));
    if !with_stats {
        return Array::row(first);
    }
    let mut second: Vec<Value> = fit.errors.iter().rev().map(|e| n(*e)).collect();
    second.push(if fit.constant { n(fit.intercept_error) } else { na() });
    let total = fit.ssreg + fit.ssresid;
    let r2 = if total == 0.0 { 1.0 } else { fit.ssreg / total };
    let sey = if fit.df > 0.0 { (fit.ssresid / fit.df).sqrt() } else { 0.0 };
    let f = if fit.ssresid == 0.0 || fit.df == 0.0 || fit.kept == 0 {
        Value::Error(ErrorKind::Num)
    } else {
        n((fit.ssreg / fit.kept as f64) / (fit.ssresid / fit.df))
    };
    // The shorter rows are filled out with #N/A to the width of the first.
    Array::from_rows(vec![
        first,
        second,
        vec![n(r2), n(sey)],
        vec![f, n(fit.df)],
        vec![n(fit.ssreg), n(fit.ssresid)],
    ])
}

/// The values to fit: for an exponential curve, the logarithms of y, which must all be positive.
fn fit_values(known: &Known, exponential: bool) -> Result<Vec<f64>, ErrorKind> {
    if !exponential {
        return Ok(known.y.clone());
    }
    if known.y.iter().any(|v| *v <= 0.0) {
        return Err(ErrorKind::Num);
    }
    Ok(known.y.iter().map(|v| v.ln()).collect())
}

fn fit_table(ctx: &mut Ctx, args: &[Expr], exponential: bool) -> Val {
    match fit_table_array(ctx, args, exponential) {
        Ok(array) => Val::Array(array),
        Err(e) => Val::error(e),
    }
}

fn fit_table_array(ctx: &mut Ctx, args: &[Expr], exponential: bool) -> Result<Array, ErrorKind> {
    let known = read_known(ctx, args)?;
    let constant = flag(ctx, args, 2, true)?;
    let with_stats = flag(ctx, args, 3, false)?;
    let y = fit_values(&known, exponential)?;
    let fitted = fit(&y, &known.x, constant);
    Ok(table(&fitted, with_stats, exponential))
}

fn predict(ctx: &mut Ctx, args: &[Expr], exponential: bool) -> Val {
    match predictions(ctx, args, exponential) {
        Ok(array) => Val::Array(array),
        Err(e) => Val::error(e),
    }
}

/// The new points: one list of variable values for each point, and the shape the answer takes.
fn new_points(ctx: &mut Ctx, args: &[Expr], known: &Known) -> Result<(Vec<Vec<f64>>, usize, usize), ErrorKind> {
    let k = known.x.len();
    let Some(e) = present(args, 2) else {
        let n = known.y.len();
        let points = (0..n).map(|i| known.x.iter().map(|v| v[i]).collect()).collect();
        return Ok(if known.by_rows { (points, 1, n) } else { (points, n, 1) });
    };
    let new = array_of(ctx, e)?;
    let numbers = numbers_in(&new.data)?;
    if k == 1 {
        return Ok((numbers.iter().map(|v| vec![*v]).collect(), new.rows, new.cols));
    }
    if known.by_rows {
        if new.rows != k {
            return Err(ErrorKind::Ref);
        }
        let points = (0..new.cols).map(|c| (0..k).map(|r| numbers[r * new.cols + c]).collect()).collect();
        return Ok((points, 1, new.cols));
    }
    if new.cols != k {
        return Err(ErrorKind::Ref);
    }
    Ok((numbers.chunks(k).map(<[f64]>::to_vec).collect(), new.rows, 1))
}

fn predictions(ctx: &mut Ctx, args: &[Expr], exponential: bool) -> Result<Array, ErrorKind> {
    let known = read_known(ctx, args)?;
    let constant = flag(ctx, args, 3, true)?;
    let y = fit_values(&known, exponential)?;
    let fitted = fit(&y, &known.x, constant);
    let (points, rows, cols) = new_points(ctx, args, &known)?;
    let data = points
        .iter()
        .map(|p| {
            let line = fitted.intercept + fitted.coefficients.iter().zip(p).map(|(m, x)| m * x).sum::<f64>();
            Value::Number(if exponential { line.exp() } else { line })
        })
        .collect();
    Ok(Array::new(rows, cols, data))
}
