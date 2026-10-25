//! Least squares fits of any number of variables: LINEST, LOGEST, GROWTH and TREND.
//!
//! The fit is a Householder QR decomposition that takes the columns from the longest to the shortest,
//! which is how Excel finds a column that is a combination of the others: when what is left of a
//! column after the ones before it falls below 6.74e-7 of the first column's length (measured against
//! Excel by shifting a column by amounts either side), that column is dropped. A dropped column gets
//! the coefficient 0 and the standard error 0 and takes a degree of freedom away from the count. With
//! an intercept the columns and the values are centred first, so a column that is a constant has
//! nothing left in it, and the intercept is worked out from the means afterwards.

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

/// A list of values as numbers. Anything that is not a number, an error included, is `#VALUE!`.
fn numbers_in(values: &[Value]) -> Result<Vec<f64>, ErrorKind> {
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
        return read_flattened(ctx, args, &y);
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

/// Known y values given as a block are read in row order, as one variable of x the same size.
fn read_flattened(ctx: &mut Ctx, args: &[Expr], y: &Array) -> Result<Known, ErrorKind> {
    let Some(e) = present(args, 1) else { return Err(ErrorKind::Ref) };
    let x = array_of(ctx, e)?;
    if x.rows != y.rows || x.cols != y.cols {
        return Err(ErrorKind::Ref);
    }
    Ok(Known { y: numbers_in(&y.data)?, x: vec![numbers_in(&x.data)?], by_rows: false })
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

/// One Householder step: turn `column` into a single value on the diagonal at row `step` and apply the
/// same reflection to the columns after it and to the right hand side.
fn reflect(
    work: &mut [Vec<f64>],
    rhs: &mut [f64],
    column: usize,
    later: &[usize],
    step: usize,
    norm: f64,
) {
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
    for &other in later {
        apply(&mut work[other][step..]);
    }
    apply(&mut rhs[step..]);
    work[column][step] = alpha;
}

/// The columns from the longest to the shortest. With an intercept two of equal length go later one
/// first and without one earlier first, which is the choice Excel makes between a column and a copy.
fn longest_first(work: &[Vec<f64>], later_wins_ties: bool) -> Vec<usize> {
    let lengths: Vec<f64> = work.iter().map(|c| norm_from(c, 0)).collect();
    let mut order: Vec<usize> = (0..work.len()).collect();
    order.sort_by(|&a, &b| {
        let by_length = lengths[b].partial_cmp(&lengths[a]).unwrap_or(std::cmp::Ordering::Equal);
        by_length.then(if later_wins_ties { b.cmp(&a) } else { a.cmp(&b) })
    });
    order
}

/// Reduce the columns to upper triangular form, longest first, leaving out a column that is made of
/// the ones before it. Returns the columns kept, in order.
fn qr_dropping_dependent(
    work: &mut [Vec<f64>],
    rhs: &mut [f64],
    later_wins_ties: bool,
) -> Vec<usize> {
    let candidates = longest_first(work, later_wins_ties);
    let first = candidates.first().map_or(0.0, |&c| norm_from(&work[c], 0));
    let mut kept = Vec::new();
    for (i, &column) in candidates.iter().enumerate() {
        let step = kept.len();
        if step >= rhs.len() {
            break;
        }
        let left = norm_from(&work[column], step);
        if left == 0.0 || left <= DROP_RATIO * first {
            continue;
        }
        reflect(work, rhs, column, &candidates[i + 1..], step, left);
        kept.push(column);
    }
    kept
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
    let order = qr_dropping_dependent(&mut work, &mut rhs, constant);
    let rank = order.len();
    let triangle: Vec<Vec<f64>> = (0..rank)
        .map(|i| (0..rank).map(|p| if p >= i { work[order[p]][i] } else { 0.0 }).collect())
        .collect();
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
    Fit {
        coefficients,
        errors,
        intercept,
        intercept_error,
        constant,
        kept: rank,
        ssreg,
        ssresid,
        df,
    }
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
fn new_points(
    ctx: &mut Ctx,
    args: &[Expr],
    known: &Known,
) -> Result<(Vec<Vec<f64>>, usize, usize), ErrorKind> {
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
        let points =
            (0..new.cols).map(|c| (0..k).map(|r| numbers[r * new.cols + c]).collect()).collect();
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
            let line = fitted.intercept
                + fitted.coefficients.iter().zip(p).map(|(m, x)| m * x).sum::<f64>();
            Value::Number(if exponential { line.exp() } else { line })
        })
        .collect();
    Ok(Array::new(rows, cols, data))
}
