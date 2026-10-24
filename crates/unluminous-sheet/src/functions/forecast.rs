//! FORECAST.ETS, FORECAST.ETS.CONFINT, FORECAST.ETS.SEASONALITY and FORECAST.ETS.STAT: argument reading
//! for the exponential smoothing in `ets`, which says how closely each answer follows Excel's.

use super::dist::normal_inverse;
use super::ets::{self, Aggregation, Fit, Series};
use super::{flat, r, Function};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "FORECAST.ETS",
        3,
        6,
        "target_date, values, timeline, [seasonality], [data_completion], [aggregation]",
        "A future value by exponential smoothing.",
        |ctx, a| finish(forecast(ctx, a)),
    ),
    r(
        "FORECAST.ETS.CONFINT",
        3,
        7,
        "target_date, values, timeline, [confidence_level], [seasonality], [data_completion], [aggregation]",
        "The confidence interval around a value forecast by exponential smoothing.",
        |ctx, a| finish(confidence_interval(ctx, a)),
    ),
    r(
        "FORECAST.ETS.SEASONALITY",
        2,
        4,
        "values, timeline, [data_completion], [aggregation]",
        "The length of the repeating pattern exponential smoothing finds in the values.",
        |ctx, a| finish(seasonality(ctx, a)),
    ),
    r(
        "FORECAST.ETS.STAT",
        3,
        6,
        "values, timeline, statistic_type, [seasonality], [data_completion], [aggregation]",
        "A statistic of the exponential smoothing: its gains, its errors or the step of the timeline.",
        |ctx, a| finish(statistic(ctx, a)),
    ),
];

fn finish(result: Result<f64, ErrorKind>) -> Val {
    result.map_or_else(Val::error, Val::number)
}

/// The number at `i`, if the argument was given and is not empty.
fn optional(ctx: &mut Ctx, args: &[Expr], i: usize) -> Result<Option<f64>, ErrorKind> {
    let Some(e) = args.get(i).filter(|e| !matches!(e, Expr::Missing)) else { return Ok(None) };
    let v = ctx.eval(e);
    let v = ctx.single(v);
    if v.is_empty() {
        return Ok(None);
    }
    ctx.number(&v).map(Some)
}

fn required(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let v = ctx.eval(e);
    let v = ctx.single(v);
    ctx.number(&v)
}

/// How the repeating pattern is chosen.
enum Pattern {
    Detect,
    Fixed(usize),
}

fn pattern_from(code: Option<f64>) -> Result<Pattern, ErrorKind> {
    match code {
        None => Ok(Pattern::Detect),
        Some(c) if c < 0.0 || c.fract() != 0.0 => Err(ErrorKind::Num),
        Some(1.0) => Ok(Pattern::Detect),
        Some(c) => Ok(Pattern::Fixed(c as usize)),
    }
}

fn completion_from(code: Option<f64>) -> Result<bool, ErrorKind> {
    match code {
        None => Ok(true),
        Some(0.0) => Ok(false),
        Some(1.0) => Ok(true),
        Some(_) => Err(ErrorKind::Num),
    }
}

/// The timeline and values of a call as points, leaving out any point where either is not a number.
fn points_of(ctx: &mut Ctx, values: &Expr, timeline: &Expr) -> Result<Vec<(f64, f64)>, ErrorKind> {
    let (y, ..) = flat(ctx, values)?;
    let (t, ..) = flat(ctx, timeline)?;
    if y.len() != t.len() {
        return Err(ErrorKind::NA);
    }
    let mut points = Vec::new();
    for (time, value) in t.iter().zip(&y) {
        for v in [time, value] {
            if let Value::Error(e) = v {
                return Err(*e);
            }
        }
        if let (Value::Number(time), Value::Number(value)) = (time, value) {
            points.push((*time, *value));
        }
    }
    Ok(points)
}

/// The series of a call and the model fitted to it.
struct Prepared {
    series: Series,
    fit: Fit,
    date1904: bool,
}

fn prepared(
    ctx: &mut Ctx,
    values: &Expr,
    timeline: &Expr,
    options: [Option<f64>; 3],
) -> Result<Prepared, ErrorKind> {
    let pattern = pattern_from(options[0])?;
    let interpolate = completion_from(options[1])?;
    let how = match options[2] {
        None => Aggregation::Average,
        Some(code) => Aggregation::from_code(code)?,
    };
    let date1904 = ctx.date1904();
    let points = points_of(ctx, values, timeline)?;
    let series = ets::prepare(&points, how, interpolate, date1904)?;
    let period = match pattern {
        Pattern::Detect => ets::detect_period(&series.y),
        Pattern::Fixed(p) => p,
    };
    let fit = ets::fit(&series.y, period);
    Ok(Prepared { series, fit, date1904 })
}

/// Read the three optional arguments that follow `first`.
fn options_at(ctx: &mut Ctx, args: &[Expr], first: usize) -> Result<[Option<f64>; 3], ErrorKind> {
    Ok([
        optional(ctx, args, first)?,
        optional(ctx, args, first + 1)?,
        optional(ctx, args, first + 2)?,
    ])
}

/// How many steps from the first point the target is. A target a whole step or more before the first
/// point is `#NUM!`, and one less than a step before it is read as the first point (measured: Excel
/// answers for 9 on a timeline that starts at 10 in steps of 10, and refuses 0 on one that starts at 1).
fn target_position(p: &Prepared, target: f64) -> Result<f64, ErrorKind> {
    let position = p.series.position(target, p.date1904).ok_or(ErrorKind::Num)?;
    if position <= -1.0 {
        return Err(ErrorKind::Num);
    }
    Ok(position.max(0.0))
}

fn forecast(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let target = required(ctx, &args[0])?;
    let options = options_at(ctx, args, 3)?;
    let p = prepared(ctx, &args[1], &args[2], options)?;
    Ok(p.fit.value_at(target_position(&p, target)?))
}

fn confidence_interval(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let target = required(ctx, &args[0])?;
    let level = optional(ctx, args, 3)?.unwrap_or(0.95);
    let options = options_at(ctx, args, 4)?;
    if !(0.0..1.0).contains(&level) {
        return Err(ErrorKind::Num);
    }
    let p = prepared(ctx, &args[1], &args[2], options)?;
    let last = (p.fit.length - 1) as f64;
    let ahead = target_position(&p, target)? - last;
    if ahead <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let z = if level == 0.0 { 0.0 } else { normal_inverse(1.0 - (1.0 - level) / 2.0) };
    Ok(p.fit.interval(ahead.ceil() as usize, p.fit.rmse_all(&p.series.y), z))
}

fn seasonality(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let completion = optional(ctx, args, 2)?;
    let aggregation = optional(ctx, args, 3)?;
    let p = prepared(ctx, &args[0], &args[1], [None, completion, aggregation])?;
    Ok(p.fit.period as f64)
}

fn statistic(ctx: &mut Ctx, args: &[Expr]) -> Result<f64, ErrorKind> {
    let kind = required(ctx, &args[2])?;
    let options = options_at(ctx, args, 3)?;
    if !(1.0..=8.0).contains(&kind) || kind.fract() != 0.0 {
        return Err(ErrorKind::Num);
    }
    let p = prepared(ctx, &args[0], &args[1], options)?;
    let y = &p.series.y;
    Ok(match kind as u8 {
        1 => p.fit.gains.alpha,
        2 => p.fit.gains.beta,
        3 => p.fit.gains.gamma,
        4 => p.fit.mase(y),
        5 => p.fit.smape(y),
        6 => p.fit.mae(y),
        7 => p.fit.rmse(y),
        _ => p.series.step_size,
    })
}
