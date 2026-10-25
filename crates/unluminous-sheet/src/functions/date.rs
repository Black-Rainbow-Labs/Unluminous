//! Dates and times.

use super::{num, number, opt_num, r, s, Function};
use crate::dates;
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s("DATE", 3, 3, "year, month, day", "The serial number of a date.", |ctx, a| {
        let (y, m, d) =
            (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?.trunc());
        if !(0.0..10000.0).contains(&y) {
            return Err(ErrorKind::Num);
        }
        dates::serial(y as i64, m as i64, d as i64, ctx.date1904())
            .map(Value::Number)
            .ok_or(ErrorKind::Num)
    }),
    s("TIME", 3, 3, "hour, minute, second", "A time as a fraction of a day.", |ctx, a| {
        let (h, m, sec) =
            (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?.trunc());
        let total = h * 3600.0 + m * 60.0 + sec;
        if total < 0.0 || h > 32767.0 || m > 32767.0 || sec > 32767.0 {
            return Err(ErrorKind::Num);
        }
        number((total / 86_400.0).fract())
    }),
    s("DATEVALUE", 1, 1, "date_text", "A date written as text, as a serial number.", |ctx, a| {
        let n = date_time_text(ctx, &a[0])?;
        number(n.floor())
    }),
    s(
        "TIMEVALUE",
        1,
        1,
        "time_text",
        "A time written as text, as a fraction of a day.",
        |ctx, a| {
            let n = date_time_text(ctx, &a[0])?;
            number(n.fract())
        },
    ),
    r("NOW", 0, 0, "", "The date and time now. Changes on every calculation.", |ctx, _| {
        Val::number(ctx.env.now)
    }),
    r("TODAY", 0, 0, "", "Today's date. Changes on every calculation.", |ctx, _| {
        Val::number(ctx.env.now.floor())
    }),
    s("YEAR", 1, 1, "serial_number", "The year of a date.", |ctx, a| {
        part(ctx, a, |(y, _, _)| y as f64)
    }),
    s("MONTH", 1, 1, "serial_number", "The month of a date, 1 to 12.", |ctx, a| {
        part(ctx, a, |(_, m, _)| m as f64)
    }),
    s("DAY", 1, 1, "serial_number", "The day of the month.", |ctx, a| {
        part(ctx, a, |(_, _, d)| d as f64)
    }),
    s("HOUR", 1, 1, "serial_number", "The hour of a time, 0 to 23.", |ctx, a| {
        clock(ctx, a, |(h, _, _)| h)
    }),
    s("MINUTE", 1, 1, "serial_number", "The minute of a time.", |ctx, a| {
        clock(ctx, a, |(_, m, _)| m)
    }),
    s("SECOND", 1, 1, "serial_number", "The second of a time.", |ctx, a| {
        clock(ctx, a, |(_, _, s)| s)
    }),
    s(
        "WEEKDAY",
        1,
        2,
        "serial_number, [return_type]",
        "The day of the week as a number.",
        |ctx, a| {
            let n = serial(ctx, &a[0])?;
            let kind = opt_num(ctx, a, 1, 1.0)?.trunc() as i64;
            let w = dates::weekday(n, ctx.date1904()) as i64; // 0 Sunday
            let answer = match kind {
                1 | 17 => w + 1,
                2 | 11 => (w + 6) % 7 + 1,
                3 => (w + 6) % 7,
                12 => (w + 5) % 7 + 1,
                13 => (w + 4) % 7 + 1,
                14 => (w + 3) % 7 + 1,
                15 => (w + 2) % 7 + 1,
                16 => (w + 1) % 7 + 1,
                _ => return Err(ErrorKind::Num),
            };
            number(answer as f64)
        },
    ),
    s("WEEKNUM", 1, 2, "serial_number, [return_type]", "The week of the year.", |ctx, a| {
        let n = serial(ctx, &a[0])?;
        if matches!(a.get(1), Some(Value::Bool(_))) {
            return Err(ErrorKind::Value);
        }
        let kind = opt_num(ctx, a, 1, 1.0)?.trunc() as i64;
        if kind == 21 {
            return number(iso_week(n, ctx.date1904()) as f64);
        }
        let start = match kind {
            1 | 17 => 0,
            2 | 11 => 1,
            12 => 2,
            13 => 3,
            14 => 4,
            15 => 5,
            16 => 6,
            // Excel also takes a return type above 21 and counts the week from Monday, as for 2; the
            // numbers that are not a return type at all (0, 3 to 10, 18 to 20, negative) are refused.
            22.. => 1,
            _ => return Err(ErrorKind::Num),
        };
        let (y, _, _) = dates::date(n, ctx.date1904()).ok_or(ErrorKind::Num)?;
        let jan1 = dates::serial(y, 1, 1, ctx.date1904()).ok_or(ErrorKind::Num)?;
        let jan1_weekday = dates::weekday(jan1, ctx.date1904()) as i64;
        let offset = (jan1_weekday - start).rem_euclid(7);
        number(((n.floor() - jan1) as i64 + offset).div_euclid(7) as f64 + 1.0)
    }),
    s("ISOWEEKNUM", 1, 1, "date", "The ISO week of the year.", |ctx, a| {
        let n = serial(ctx, &a[0])?;
        number(iso_week(n, ctx.date1904()) as f64)
    }),
    s("EDATE", 2, 2, "start_date, months", "The same day some months away.", |ctx, a| {
        let n = serial(ctx, &a[0])?;
        let months = num(ctx, &a[1])?.trunc() as i64;
        let (y, m, d) = dates::date(n, ctx.date1904()).ok_or(ErrorKind::Num)?;
        let total = y * 12 + m as i64 - 1 + months;
        let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
        let day = d.min(dates::days_in_month(ny, nm));
        dates::serial(ny, nm as i64, day as i64, ctx.date1904())
            .map(Value::Number)
            .ok_or(ErrorKind::Num)
    }),
    s(
        "EOMONTH",
        2,
        2,
        "start_date, months",
        "The last day of a month some months away.",
        |ctx, a| {
            let n = serial(ctx, &a[0])?;
            let months = num(ctx, &a[1])?.trunc() as i64;
            let (y, m, _) = dates::date(n, ctx.date1904()).ok_or(ErrorKind::Num)?;
            let total = y * 12 + m as i64 - 1 + months;
            let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
            dates::serial(ny, nm as i64, dates::days_in_month(ny, nm) as i64, ctx.date1904())
                .map(Value::Number)
                .ok_or(ErrorKind::Num)
        },
    ),
    s("DAYS", 2, 2, "end_date, start_date", "The days between two dates.", |ctx, a| {
        number(serial(ctx, &a[0])?.floor() - serial(ctx, &a[1])?.floor())
    }),
    s(
        "DAYS360",
        2,
        3,
        "start_date, end_date, [method]",
        "The days between two dates in a 360 day year.",
        |ctx, a| {
            let (s1, s2) = (serial(ctx, &a[0])?, serial(ctx, &a[1])?);
            let european = super::opt_bool(a, 2, false)?;
            let (y1, m1, d1) = dates::date(s1, ctx.date1904()).ok_or(ErrorKind::Num)?;
            let (y2, m2, d2) = dates::date(s2, ctx.date1904()).ok_or(ErrorKind::Num)?;
            number(days360((y1, m1, d1), (y2, m2, d2), european) as f64)
        },
    ),
    s(
        "DATEDIF",
        3,
        3,
        "start_date, end_date, unit",
        "The difference between dates in years, months or days.",
        |ctx, a| {
            let (s1, s2) = (serial(ctx, &a[0])?.floor(), serial(ctx, &a[1])?.floor());
            if s1 > s2 {
                return Err(ErrorKind::Num);
            }
            let unit = super::text_of(&a[2])?.to_ascii_uppercase();
            let (y1, m1, d1) = dates::date(s1, ctx.date1904()).ok_or(ErrorKind::Num)?;
            let (y2, m2, d2) = dates::date(s2, ctx.date1904()).ok_or(ErrorKind::Num)?;
            let months = (y2 - y1) * 12 + m2 as i64 - m1 as i64 - i64::from(d2 < d1);
            number(match unit.as_str() {
                "Y" => (months / 12) as f64,
                "M" => months as f64,
                "D" => s2 - s1,
                "MD" => {
                    if d2 >= d1 {
                        (d2 - d1) as f64
                    } else {
                        // Excel counts from the start day in the month before the end, rolling over, so
                        // 31 January to 1 March is -2: 1 March less 31 February, which is 3 March.
                        s2 - dates::serial(y2, m2 as i64 - 1, d1 as i64, ctx.date1904())
                            .ok_or(ErrorKind::Num)?
                    }
                }
                "YM" => (months % 12) as f64,
                "YD" => days_ignoring_years((y1, m1, d1), (y2, m2, d2)) as f64,
                _ => return Err(ErrorKind::Num),
            })
        },
    ),
    r(
        "NETWORKDAYS",
        2,
        3,
        "start_date, end_date, [holidays]",
        "The working days between two dates.",
        |ctx, a| networkdays(ctx, &a[0], &a[1], None, a.get(2)),
    ),
    r(
        "NETWORKDAYS.INTL",
        2,
        4,
        "start_date, end_date, [weekend], [holidays]",
        "The working days between two dates, with a chosen weekend.",
        |ctx, a| networkdays(ctx, &a[0], &a[1], a.get(2), a.get(3)),
    ),
    r(
        "WORKDAY",
        2,
        3,
        "start_date, days, [holidays]",
        "The date some working days away.",
        |ctx, a| workday(ctx, &a[0], &a[1], None, a.get(2)),
    ),
    r(
        "WORKDAY.INTL",
        2,
        4,
        "start_date, days, [weekend], [holidays]",
        "The date some working days away, with a chosen weekend.",
        |ctx, a| workday(ctx, &a[0], &a[1], a.get(2), a.get(3)),
    ),
    s(
        "YEARFRAC",
        2,
        3,
        "start_date, end_date, [basis]",
        "The fraction of a year between two dates.",
        |ctx, a| {
            let (s1, s2) = (serial(ctx, &a[0])?.floor(), serial(ctx, &a[1])?.floor());
            let basis = opt_num(ctx, a, 2, 0.0)?.trunc() as i64;
            number(yearfrac(s1, s2, basis, ctx.date1904())?)
        },
    ),
];

fn serial(ctx: &Ctx, v: &Value) -> Result<f64, ErrorKind> {
    let n = num(ctx, v)?;
    // The last day Excel keeps is 31 December 9999, which is 2,958,465 and, in the 1904 system, 1,462 fewer.
    let last = 2_958_466.0 - if ctx.date1904() { 1462.0 } else { 0.0 };
    if n < 0.0 || n >= last {
        return Err(ErrorKind::Num);
    }
    Ok(n)
}

fn part(ctx: &mut Ctx, a: &[Value], f: fn((i64, u32, u32)) -> f64) -> Result<Value, ErrorKind> {
    let n = serial(ctx, &a[0])?;
    let parts = dates::date(n, ctx.date1904()).ok_or(ErrorKind::Num)?;
    number(f(parts))
}

fn clock(ctx: &mut Ctx, a: &[Value], f: fn((f64, f64, f64)) -> f64) -> Result<Value, ErrorKind> {
    let n = serial(ctx, &a[0])?;
    let (h, m, sec, ms) = dates::time(n);
    // Excel rounds to the nearest second for the parts.
    let total = (h * 3600 + m * 60 + sec) as f64 + if ms >= 500 { 1.0 } else { 0.0 };
    let total = total % 86_400.0;
    number(f(((total / 3600.0).floor(), (total / 60.0).floor() % 60.0, total % 60.0)))
}

fn iso_week(n: f64, date1904: bool) -> i64 {
    let day = n.floor();
    let weekday = (dates::weekday(day, date1904) as i64 + 6) % 7; // Monday 0
    let thursday = day - weekday as f64 + 3.0;
    // Day 0 and the days of its week belong to the last ISO week of 1899.
    if thursday < 1.0 {
        return 52;
    }
    let (y, _, _) = dates::date(thursday, date1904).unwrap_or((1900, 1, 1));
    let jan1 = dates::serial(y, 1, 1, date1904).unwrap_or(1.0);
    ((thursday - jan1) / 7.0).floor() as i64 + 1
}

pub(super) fn days360(start: (i64, u32, u32), end: (i64, u32, u32), european: bool) -> i64 {
    let (y1, m1, mut d1) = (start.0, start.1 as i64, start.2 as i64);
    let (y2, m2, mut d2) = (end.0, end.1 as i64, end.2 as i64);
    if european {
        d1 = d1.min(30);
        d2 = d2.min(30);
    } else {
        let last_of_feb = |y: i64, m: i64, d: i64| m == 2 && d == dates::days_in_month(y, 2) as i64;
        if last_of_feb(y1, m1, d1) {
            if last_of_feb(y2, m2, d2) {
                d2 = 30;
            }
            d1 = 30;
        }
        if d2 == 31 && d1 >= 30 {
            d2 = 30;
        }
        if d1 == 31 {
            d1 = 30;
        }
    }
    (y2 - y1) * 360 + (m2 - m1) * 30 + (d2 - d1)
}

/// DATEDIF's "YD" as Excel counts it, which is not the days between the dates with the years taken out.
/// It adds the lengths of the whole months from the start's month on, taking each from the calendar of
/// the year it falls in counting from the start's year, and then the days left over. When the end day is
/// before the start day the leftover borrows the length of the month before the end's month, in the
/// end's year. That is why 15 January 2020 to 10 October 2026 is 269 days: the months from January to
/// August are those of 2020.
fn days_ignoring_years(start: (i64, u32, u32), end: (i64, u32, u32)) -> i64 {
    let (sy, sm, sd) = start;
    let (ey, em, ed) = end;
    let mut months = (em as i64 - sm as i64).rem_euclid(12);
    let borrows = ed < sd;
    if borrows {
        months = if months == 0 { 11 } else { months - 1 };
    }
    let mut days: i64 = (0..months)
        .map(|k| {
            let at = sm as i64 - 1 + k;
            i64::from(dates::days_in_month(sy + at.div_euclid(12), at.rem_euclid(12) as u32 + 1))
        })
        .sum();
    days += ed as i64 - sd as i64;
    if borrows {
        let (year, month) = if em == 1 { (ey - 1, 12) } else { (ey, em - 1) };
        days += i64::from(dates::days_in_month(year, month));
    }
    days
}

/// Which days of the week are the weekend, Monday first.
fn weekend_mask(ctx: &mut Ctx, weekend: Option<&Expr>) -> Result<[bool; 7], ErrorKind> {
    let Some(e) = weekend.filter(|e| !matches!(e, Expr::Missing)) else {
        return Ok([false, false, false, false, false, true, true]);
    };
    let v = ctx.eval(e);
    let v = ctx.single(v);
    match v {
        Value::Text(t) => {
            if t.len() != 7 || !t.chars().all(|c| c == '0' || c == '1') {
                return Err(ErrorKind::Value);
            }
            let mut mask = [false; 7];
            for (i, c) in t.chars().enumerate() {
                mask[i] = c == '1';
            }
            Ok(mask)
        }
        other => {
            let code = ctx.number(&other)?.trunc() as i64;
            let mut mask = [false; 7];
            match code {
                1..=7 => {
                    // 1 is Saturday and Sunday, 2 Sunday and Monday, and so on.
                    let first = (code + 4) % 7; // index of the first weekend day, Monday 0
                    mask[first as usize] = true;
                    mask[((first + 1) % 7) as usize] = true;
                }
                11..=17 => mask[((code - 11 + 6) % 7) as usize] = true,
                _ => return Err(ErrorKind::Num),
            }
            Ok(mask)
        }
    }
}

fn holidays(ctx: &mut Ctx, e: Option<&Expr>) -> Result<Vec<f64>, ErrorKind> {
    let Some(e) = e.filter(|e| !matches!(e, Expr::Missing)) else { return Ok(Vec::new()) };
    let v = ctx.eval(e);
    let a = ctx.array(v)?;
    let mut out = Vec::new();
    for v in a.data {
        match v {
            Value::Number(n) => out.push(n.floor()),
            Value::Error(e) => return Err(e),
            Value::Empty => {}
            _ => return Err(ErrorKind::Value),
        }
    }
    Ok(out)
}

fn scalar_serial(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let v = ctx.eval(e);
    let v = ctx.single(v);
    Ok(serial(ctx, &v)?.floor())
}

fn networkdays(
    ctx: &mut Ctx,
    start: &Expr,
    end: &Expr,
    weekend: Option<&Expr>,
    extra: Option<&Expr>,
) -> Val {
    let go = |ctx: &mut Ctx| -> Result<f64, ErrorKind> {
        let (a, b) = (scalar_serial(ctx, start)?, scalar_serial(ctx, end)?);
        let mask = weekend_mask(ctx, weekend)?;
        let off = holidays(ctx, extra)?;
        let (lo, hi, sign) = if a <= b { (a, b, 1.0) } else { (b, a, -1.0) };
        let mut count = 0.0;
        let mut d = lo;
        while d <= hi {
            let w = (dates::weekday(d, ctx.date1904()) as usize + 6) % 7;
            if !mask[w] && !off.contains(&d) {
                count += 1.0;
            }
            d += 1.0;
        }
        Ok(count * sign)
    };
    go(ctx).map_or_else(Val::error, Val::number)
}

fn workday(
    ctx: &mut Ctx,
    start: &Expr,
    days: &Expr,
    weekend: Option<&Expr>,
    extra: Option<&Expr>,
) -> Val {
    // An array of day counts gives an array of dates, as Excel's dynamic arrays do.
    let counts = ctx.eval(days);
    if let Val::Array(array) = &counts {
        if array.data.len() > 1 {
            let mut data = Vec::with_capacity(array.data.len());
            for v in array.data.clone() {
                let one = Val::Value(v);
                data.push(match workday_for(ctx, start, one, weekend, extra) {
                    Ok(d) => Value::Number(d),
                    Err(e) => Value::Error(e),
                });
            }
            return Val::Array(crate::value::Array::new(array.rows, array.cols, data));
        }
    }
    workday_for(ctx, start, counts, weekend, extra).map_or_else(Val::error, Val::number)
}

/// The date `days` working days from `start`.
fn workday_for(
    ctx: &mut Ctx,
    start: &Expr,
    days: Val,
    weekend: Option<&Expr>,
    extra: Option<&Expr>,
) -> Result<f64, ErrorKind> {
    let go = |ctx: &mut Ctx| -> Result<f64, ErrorKind> {
        let s0 = scalar_serial(ctx, start)?;
        let v = ctx.single(days);
        let n = ctx.number(&v)?.trunc();
        let mask = weekend_mask(ctx, weekend)?;
        let off = holidays(ctx, extra)?;
        if n != 0.0 && mask.iter().all(|m| *m) {
            return Err(ErrorKind::Value);
        }
        let step = if n >= 0.0 { 1.0 } else { -1.0 };
        let mut left = n.abs();
        let mut d = s0;
        while left > 0.0 {
            d += step;
            let w = (dates::weekday(d, ctx.date1904()) as usize + 6) % 7;
            if !mask[w] && !off.contains(&d) {
                left -= 1.0;
            }
            if d < 0.0 {
                return Err(ErrorKind::Num);
            }
        }
        Ok(d)
    };
    go(ctx)
}

/// Days between two dates in the US (NASD) 30/360 count that YEARFRAC basis 0 and the bond functions use.
/// It differs from the DAYS360 rule in how the last day of February is treated.
pub(super) fn days360_nasd(d1: (i64, u32, u32), d2: (i64, u32, u32)) -> i64 {
    let (y1, m1, mut dd1) = (d1.0, d1.1 as i64, d1.2 as i64);
    let (y2, m2, mut dd2) = (d2.0, d2.1 as i64, d2.2 as i64);
    if dd1 == 31 && dd2 == 31 {
        dd1 = 30;
        dd2 = 30;
    } else if dd1 == 31 {
        dd1 = 30;
    } else if dd1 == 30 && dd2 == 31 {
        dd2 = 30;
    } else if m1 == 2
        && m2 == 2
        && dd1 == dates::days_in_month(y1, 2) as i64
        && dd2 == dates::days_in_month(y2, 2) as i64
    {
        dd1 = 30;
        dd2 = 30;
    } else if m1 == 2 && dd1 == dates::days_in_month(y1, 2) as i64 {
        dd1 = 30;
    }
    (y2 - y1) * 360 + (m2 - m1) * 30 + (dd2 - dd1)
}

/// The fraction of a year between two serials under a day count basis from 0 to 4.
pub(super) fn yearfrac(s1: f64, s2: f64, basis: i64, date1904: bool) -> Result<f64, ErrorKind> {
    let (a, b) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
    let d1 = dates::date(a, date1904).ok_or(ErrorKind::Num)?;
    let d2 = dates::date(b, date1904).ok_or(ErrorKind::Num)?;
    Ok(match basis {
        0 => days360_nasd(d1, d2) as f64 / 360.0,
        1 => {
            let days = b - a;
            if d1.0 == d2.0 {
                days / if dates::is_leap(d1.0) { 366.0 } else { 365.0 }
            } else if d2.0 == d1.0 + 1 && (d2.1, d2.2) <= (d1.1, d1.2) {
                let leap = dates::is_leap(d1.0) && (d1.1 <= 2)
                    || dates::is_leap(d2.0) && (d2.1 > 2 || d2.1 == 2 && d2.2 == 29);
                days / if leap { 366.0 } else { 365.0 }
            } else {
                let years = (d2.0 - d1.0 + 1) as f64;
                let start = dates::serial(d1.0, 1, 1, date1904).ok_or(ErrorKind::Num)?;
                let end = dates::serial(d2.0 + 1, 1, 1, date1904).ok_or(ErrorKind::Num)?;
                days / ((end - start) / years)
            }
        }
        2 => (b - a) / 360.0,
        3 => (b - a) / 365.0,
        4 => days360(d1, d2, true) as f64 / 360.0,
        _ => return Err(ErrorKind::Num),
    })
}

/// The serial a date or time written as text stands for. A number is not text, so it is not a date.
fn date_time_text(ctx: &Ctx, value: &Value) -> Result<f64, ErrorKind> {
    match value {
        Value::Text(t) => {
            crate::input::date_time_from_text(t, ctx.date1904()).ok_or(ErrorKind::Value)
        }
        _ => Err(ErrorKind::Value),
    }
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn dates_answer_what_excel_answers() {
        check(&[
            ("DATE(2026,10,23)", "46318"),
            ("DATE(2026,14,1)", "46419"),
            ("YEAR(46318)", "2026"),
            ("MONTH(46318)", "10"),
            ("DAY(46318)", "23"),
            ("TIME(14,30,0)", "0.604166666667"),
            ("HOUR(0.604166666667)", "14"),
            ("MINUTE(0.604166666667)", "30"),
            ("WEEKDAY(46318)", "6"),
            ("WEEKDAY(46318,2)", "5"),
            ("EDATE(DATE(2026,1,31),1)", "46081"),
            ("EOMONTH(DATE(2026,1,15),1)", "46081"),
            ("DATEDIF(DATE(2020,1,1),DATE(2026,10,23),\"Y\")", "6"),
            ("DATEDIF(DATE(2020,1,1),DATE(2026,10,23),\"M\")", "81"),
            ("DATEDIF(DATE(2026,1,31),DATE(2026,3,1),\"MD\")", "-2"),
            ("DAYS(DATE(2026,12,25),DATE(2026,10,23))", "63"),
            ("NETWORKDAYS(DATE(2026,10,19),DATE(2026,10,30))", "10"),
            ("WORKDAY(DATE(2026,10,23),1)", "46321"),
            ("ISOWEEKNUM(DATE(2026,10,23))", "43"),
            ("WEEKNUM(DATE(2026,10,23))", "43"),
            ("DAYS360(DATE(2026,1,30),DATE(2026,12,31))", "330"),
            ("YEARFRAC(DATE(2026,1,1),DATE(2026,7,1))", "0.5"),
            ("DATEVALUE(\"10/23/2026\")", "46318"),
        ]);
    }
}
