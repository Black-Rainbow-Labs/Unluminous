//! Financial functions: loans and annuities, investment returns and depreciation.

use super::{num, number, numbers_at, opt_num, r, s, Function, MANY};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s("PMT", 3, 5, "rate, nper, pv, [fv], [type]", "The payment for a loan.", |ctx, a| {
        let (rate, n, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        let (fv, kind) = (opt_num(ctx, a, 3, 0.0)?, opt_num(ctx, a, 4, 0.0)? != 0.0);
        number(pmt(rate, n, pv, fv, kind)?)
    }),
    s("IPMT", 4, 6, "rate, per, nper, pv, [fv], [type]", "The interest part of a payment.", |ctx, a| {
        let (rate, per, n, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?, num(ctx, &a[3])?);
        let (fv, kind) = (opt_num(ctx, a, 4, 0.0)?, opt_num(ctx, a, 5, 0.0)? != 0.0);
        number(ipmt(rate, per, n, pv, fv, kind)?)
    }),
    s("PPMT", 4, 6, "rate, per, nper, pv, [fv], [type]", "The principal part of a payment.", |ctx, a| {
        let (rate, per, n, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?, num(ctx, &a[3])?);
        let (fv, kind) = (opt_num(ctx, a, 4, 0.0)?, opt_num(ctx, a, 5, 0.0)? != 0.0);
        number(pmt(rate, n, pv, fv, kind)? - ipmt(rate, per, n, pv, fv, kind)?)
    }),
    s("PV", 3, 5, "rate, nper, pmt, [fv], [type]", "The present value of an investment.", |ctx, a| {
        let (rate, n, payment) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        let (fv, kind) = (opt_num(ctx, a, 3, 0.0)?, opt_num(ctx, a, 4, 0.0)? != 0.0);
        if rate == 0.0 {
            return number(-fv - payment * n);
        }
        let k = (1.0 + rate).powf(n);
        number((-fv - payment * (1.0 + rate * f64::from(u8::from(kind))) * (k - 1.0) / rate) / k)
    }),
    s("FV", 3, 5, "rate, nper, pmt, [pv], [type]", "The future value of an investment.", |ctx, a| {
        let (rate, n, payment) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        let (pv, kind) = (opt_num(ctx, a, 3, 0.0)?, opt_num(ctx, a, 4, 0.0)? != 0.0);
        number(fv(rate, n, payment, pv, kind))
    }),
    s("NPER", 3, 5, "rate, pmt, pv, [fv], [type]", "How many periods an investment takes.", |ctx, a| {
        let (rate, payment, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        let (fv, kind) = (opt_num(ctx, a, 3, 0.0)?, opt_num(ctx, a, 4, 0.0)? != 0.0);
        if rate == 0.0 {
            if payment == 0.0 {
                return Err(ErrorKind::Num);
            }
            return number(-(pv + fv) / payment);
        }
        let adjusted = payment * (1.0 + rate * f64::from(u8::from(kind))) / rate;
        let ratio = (adjusted - fv) / (adjusted + pv);
        if ratio <= 0.0 {
            return Err(ErrorKind::Num);
        }
        number(ratio.ln() / (1.0 + rate).ln())
    }),
    s("RATE", 3, 6, "nper, pmt, pv, [fv], [type], [guess]", "The interest rate per period.", |ctx, a| {
        let (n, payment, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        let (fv_, kind) = (opt_num(ctx, a, 3, 0.0)?, opt_num(ctx, a, 4, 0.0)? != 0.0);
        let guess = opt_num(ctx, a, 5, 0.1)?;
        let f = |r: f64| fv_ - fv(r, n, payment, pv, kind);
        newton(f, guess).map(Value::Number).ok_or(ErrorKind::Num)
    }),
    r("NPV", 2, MANY, "rate, value1, [value2], ...", "The net present value of payments.", |ctx, a| {
        let rate = match scalar(ctx, &a[0]) {
            Ok(r) => r,
            Err(e) => return Val::error(e),
        };
        let flows = match super::gather(ctx, &a[1..], super::Gather::Numbers) {
            Ok(f) => f,
            Err(e) => return Val::error(e),
        };
        Val::number(flows.iter().enumerate().map(|(i, c)| c / (1.0 + rate).powi(i as i32 + 1)).sum())
    }),
    r("XNPV", 3, 3, "rate, values, dates", "The net present value of payments on dates.", |ctx, a| {
        let rate = match scalar(ctx, &a[0]) {
            Ok(r) => r,
            Err(e) => return Val::error(e),
        };
        match dated(ctx, &a[1], &a[2]) {
            Ok((flows, dates)) => Val::number(xnpv(rate, &flows, &dates)),
            Err(e) => Val::error(e),
        }
    }),
    r("IRR", 1, 2, "values, [guess]", "The internal rate of return.", |ctx, a| {
        let flows: Vec<f64> = match numbers_at(ctx, &a[0]) {
            Ok((v, ..)) => v.into_iter().flatten().collect(),
            Err(e) => return Val::error(e),
        };
        let guess = match a.get(1) {
            Some(e) => scalar(ctx, e).unwrap_or(0.1),
            None => 0.1,
        };
        if !flows.iter().any(|f| *f > 0.0) || !flows.iter().any(|f| *f < 0.0) {
            return Val::error(ErrorKind::Num);
        }
        let f = |r: f64| flows.iter().enumerate().map(|(i, c)| c / (1.0 + r).powi(i as i32)).sum::<f64>();
        newton(f, guess).map_or(Val::error(ErrorKind::Num), Val::number)
    }),
    r("XIRR", 2, 3, "values, dates, [guess]", "The internal rate of return of payments on dates.", |ctx, a| {
        let (flows, dates) = match dated(ctx, &a[0], &a[1]) {
            Ok(x) => x,
            Err(e) => return Val::error(e),
        };
        let guess = match a.get(2) {
            Some(e) => scalar(ctx, e).unwrap_or(0.1),
            None => 0.1,
        };
        newton(|r| xnpv(r, &flows, &dates), guess).map_or(Val::error(ErrorKind::Num), Val::number)
    }),
    r("MIRR", 3, 3, "values, finance_rate, reinvest_rate", "The modified internal rate of return.", |ctx, a| {
        let flows: Vec<f64> = match numbers_at(ctx, &a[0]) {
            Ok((v, ..)) => v.into_iter().flatten().collect(),
            Err(e) => return Val::error(e),
        };
        let (fr, rr) = match (scalar(ctx, &a[1]), scalar(ctx, &a[2])) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(e), _) | (_, Err(e)) => return Val::error(e),
        };
        let n = flows.len() as f64;
        let pos: f64 = flows.iter().enumerate().filter(|(_, c)| **c > 0.0).map(|(i, c)| c * (1.0 + rr).powf(n - 1.0 - i as f64)).sum();
        let neg: f64 = flows.iter().enumerate().filter(|(_, c)| **c < 0.0).map(|(i, c)| c / (1.0 + fr).powi(i as i32)).sum();
        if pos == 0.0 || neg == 0.0 {
            return Val::error(ErrorKind::Div0);
        }
        Val::number((-pos / neg).powf(1.0 / (n - 1.0)) - 1.0)
    }),
    s("SLN", 3, 3, "cost, salvage, life", "Straight line depreciation.", |ctx, a| {
        let (c, s, l) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        if l == 0.0 {
            return Err(ErrorKind::Div0);
        }
        number((c - s) / l)
    }),
    s("SYD", 4, 4, "cost, salvage, life, per", "Sum of years' digits depreciation.", |ctx, a| {
        let (c, s, l, p) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?, num(ctx, &a[3])?);
        if l <= 0.0 || p <= 0.0 || p > l {
            return Err(ErrorKind::Num);
        }
        number((c - s) * (l - p + 1.0) * 2.0 / (l * (l + 1.0)))
    }),
    s("DDB", 4, 5, "cost, salvage, life, period, [factor]", "Double declining balance depreciation.", |ctx, a| {
        let (c, s, l, p) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?, num(ctx, &a[3])?);
        let factor = opt_num(ctx, a, 4, 2.0)?;
        if c < 0.0 || s < 0.0 || l <= 0.0 || p <= 0.0 || p > l || factor <= 0.0 {
            return Err(ErrorKind::Num);
        }
        let mut book = c;
        let mut dep = 0.0;
        for _ in 0..p.ceil() as i64 {
            dep = (book * factor / l).min((book - s).max(0.0));
            book -= dep;
        }
        number(dep)
    }),
    s("DB", 4, 5, "cost, salvage, life, period, [month]", "Fixed declining balance depreciation.", |ctx, a| {
        let (c, s, l, p) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?.trunc(), num(ctx, &a[3])?.trunc());
        let month = opt_num(ctx, a, 4, 12.0)?.trunc();
        if c < 0.0 || s < 0.0 || l <= 0.0 || p <= 0.0 || month < 1.0 || month > 12.0 || p > l + 1.0 || month == 12.0 && p > l {
            return Err(ErrorKind::Num);
        }
        let rate = if c == 0.0 { 0.0 } else { super::math::round_to(1.0 - (s / c).powf(1.0 / l), 3.0, super::math::Rounding::Half) };
        let mut total = 0.0;
        let mut dep = c * rate * month / 12.0;
        if p == 1.0 {
            return number(dep);
        }
        total += dep;
        for k in 2..=p as i64 {
            dep = if k as f64 == l + 1.0 { (c - total) * rate * (12.0 - month) / 12.0 } else { (c - total) * rate };
            total += dep;
        }
        number(dep)
    }),
    s("EFFECT", 2, 2, "nominal_rate, npery", "The effective annual interest rate.", |ctx, a| {
        let (r, n) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
        if r <= 0.0 || n < 1.0 {
            return Err(ErrorKind::Num);
        }
        number((1.0 + r / n).powf(n) - 1.0)
    }),
    s("NOMINAL", 2, 2, "effect_rate, npery", "The nominal annual interest rate.", |ctx, a| {
        let (r, n) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
        if r <= 0.0 || n < 1.0 {
            return Err(ErrorKind::Num);
        }
        number(n * ((1.0 + r).powf(1.0 / n) - 1.0))
    }),
    s("CUMIPMT", 6, 6, "rate, nper, pv, start_period, end_period, type", "The interest paid between two periods.", |ctx, a| cumulative(ctx, a, true)),
    s("CUMPRINC", 6, 6, "rate, nper, pv, start_period, end_period, type", "The principal paid between two periods.", |ctx, a| cumulative(ctx, a, false)),
    s("ISPMT", 4, 4, "rate, per, nper, pv", "The interest paid in a period of a straight line loan.", |ctx, a| {
        let (r, per, n, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?, num(ctx, &a[3])?);
        if n == 0.0 {
            return Err(ErrorKind::Div0);
        }
        number(pv * r * (per / n - 1.0))
    }),
    s("PDURATION", 3, 3, "rate, pv, fv", "The periods for an investment to reach a value.", |ctx, a| {
        let (r, pv, fv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        if r <= 0.0 || pv <= 0.0 || fv <= 0.0 {
            return Err(ErrorKind::Num);
        }
        number((fv.ln() - pv.ln()) / (1.0 + r).ln())
    }),
    s("RRI", 3, 3, "nper, pv, fv", "The equivalent interest rate for growth.", |ctx, a| {
        let (n, pv, fv) = (num(ctx, &a[0])?, num(ctx, &a[1])?, num(ctx, &a[2])?);
        if n <= 0.0 || pv == 0.0 {
            return Err(ErrorKind::Num);
        }
        number((fv / pv).powf(1.0 / n) - 1.0)
    }),
    r("FVSCHEDULE", 2, 2, "principal, schedule", "The future value after a series of rates.", |ctx, a| {
        let p = match scalar(ctx, &a[0]) {
            Ok(p) => p,
            Err(e) => return Val::error(e),
        };
        match super::gather(ctx, &a[1..2], super::Gather::Numbers) {
            Ok(rates) => Val::number(rates.iter().fold(p, |acc, r| acc * (1.0 + r))),
            Err(e) => Val::error(e),
        }
    }),
];

fn scalar(ctx: &mut Ctx, e: &Expr) -> Result<f64, ErrorKind> {
    let v = ctx.eval(e);
    let v = ctx.single(v);
    ctx.number(&v)
}

pub fn pmt(rate: f64, n: f64, pv: f64, fv: f64, at_start: bool) -> Result<f64, ErrorKind> {
    if n == 0.0 {
        return Err(ErrorKind::Num);
    }
    if rate == 0.0 {
        return Ok(-(pv + fv) / n);
    }
    let k = (1.0 + rate).powf(n);
    Ok(-(rate * (pv * k + fv)) / ((1.0 + rate * f64::from(u8::from(at_start))) * (k - 1.0)))
}

pub fn fv(rate: f64, n: f64, payment: f64, pv: f64, at_start: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + payment * n);
    }
    let k = (1.0 + rate).powf(n);
    -(pv * k + payment * (1.0 + rate * f64::from(u8::from(at_start))) * (k - 1.0) / rate)
}

fn ipmt(rate: f64, per: f64, n: f64, pv: f64, fv_: f64, at_start: bool) -> Result<f64, ErrorKind> {
    if per < 1.0 || per > n {
        return Err(ErrorKind::Num);
    }
    let payment = pmt(rate, n, pv, fv_, at_start)?;
    if at_start && per == 1.0 {
        return Ok(0.0);
    }
    let balance = if at_start { fv(rate, per - 2.0, payment, pv, true) - payment } else { fv(rate, per - 1.0, payment, pv, false) };
    Ok(balance * rate)
}

fn cumulative(ctx: &mut Ctx, a: &[Value], interest: bool) -> Result<Value, ErrorKind> {
    let (rate, n, pv) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), num(ctx, &a[2])?);
    let (start, end, kind) = (num(ctx, &a[3])?.trunc(), num(ctx, &a[4])?.trunc(), num(ctx, &a[5])?.trunc());
    if rate <= 0.0 || n <= 0.0 || pv <= 0.0 || start < 1.0 || end < start || end > n || !(kind == 0.0 || kind == 1.0) {
        return Err(ErrorKind::Num);
    }
    let at_start = kind == 1.0;
    let mut total = 0.0;
    let payment = pmt(rate, n, pv, 0.0, at_start)?;
    for per in start as i64..=end as i64 {
        let i = ipmt(rate, per as f64, n, pv, 0.0, at_start)?;
        total += if interest { i } else { payment - i };
    }
    number(total)
}

/// Newton's method from a guess, with a numerical derivative.
fn newton(f: impl Fn(f64) -> f64, guess: f64) -> Option<f64> {
    let mut x = guess;
    for _ in 0..100 {
        let y = f(x);
        if !y.is_finite() {
            return None;
        }
        let h = (x.abs() * 1e-7).max(1e-10);
        let slope = (f(x + h) - f(x - h)) / (2.0 * h);
        if slope == 0.0 || !slope.is_finite() {
            return None;
        }
        let next = x - y / slope;
        if (next - x).abs() < 1e-12 * next.abs().max(1.0) {
            return Some(next);
        }
        x = next;
        if x <= -1.0 {
            x = -0.99;
        }
    }
    None
}

fn dated(ctx: &mut Ctx, values: &Expr, dates: &Expr) -> Result<(Vec<f64>, Vec<f64>), ErrorKind> {
    let (v, ..) = numbers_at(ctx, values)?;
    let (d, ..) = numbers_at(ctx, dates)?;
    if v.len() != d.len() || v.is_empty() {
        return Err(ErrorKind::Num);
    }
    let flows: Option<Vec<f64>> = v.into_iter().collect();
    let dates: Option<Vec<f64>> = d.into_iter().map(|d| d.map(f64::floor)).collect();
    match (flows, dates) {
        (Some(f), Some(d)) => Ok((f, d)),
        _ => Err(ErrorKind::Value),
    }
}

fn xnpv(rate: f64, flows: &[f64], dates: &[f64]) -> f64 {
    flows.iter().zip(dates).map(|(c, d)| c / (1.0 + rate).powf((d - dates[0]) / 365.0)).sum()
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn financial_functions_answer_what_excel_answers() {
        check(&[
            ("PMT(0.08/12,10,10000)", "-1037.03208936"),
            ("PMT(0.08/12,10,10000,0,1)", "-1030.16432718"),
            ("IPMT(0.1/12,1,36,8000)", "-66.6666666667"),
            ("PPMT(0.1/12,1,24,2000)", "-75.6231860084"),
            ("PV(0.08/12,12*20,500)", "-59777.1458512"),
            ("FV(0.06/12,10,-200,-500,1)", "2581.40337406"),
            ("NPER(0.12/12,-100,-1000,10000,1)", "59.6738656743"),
            ("RATE(48,-200,8000)", "0.0077014724882"),
            ("NPV(0.1,-10000,3000,4200,6800)", "1188.44341234"),
            ("IRR({-70000,12000,15000,18000,21000,26000})", "0.0866309480365"),
            ("SLN(30000,7500,10)", "2250"),
            ("SYD(30000,7500,10,1)", "4090.90909091"),
            ("DDB(2400,300,10*365,1)", "1.31506849315"),
            ("DB(1000000,100000,6,1,7)", "186083.333333"),
            ("EFFECT(0.0525,4)", "0.0535426673708"),
            ("NOMINAL(0.053543,4)", "0.0525003198684"),
            ("CUMIPMT(0.09/12,30*12,125000,13,24,0)", "-11135.2321308"),
        ]);
    }
}
