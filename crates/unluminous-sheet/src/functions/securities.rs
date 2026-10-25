//! Bonds and other securities: coupon schedules, prices, yields, durations, discount securities,
//! treasury bills, fractional dollars and the depreciation functions that count periods by date or by
//! variable declining balance.
//!
//! Dates are whole serials: Excel truncates a date argument before it uses it. Day counts follow Excel's
//! bases: 0 is 30/360 as the US NASD counts it, 1 actual/actual, 2 actual/360, 3 actual/365 and 4 the
//! European 30/360. The 30/360 count and the year fraction are the ones YEARFRAC uses, so the two cannot
//! disagree.
//!
//! Excel is the reference for every figure, and `tests/oracle/securities.txt` holds the formulas checked
//! against it. Where Excel's own rules are irregular the code follows what was measured, and says so
//! beside the rule. What is not reproduced:
//!
//! - ODDFPRICE and ODDFYIELD for a first coupon period longer than one regular period on the bases other
//!   than 30/360 and actual/actual. Excel adds a whole period to the discounting exponent for some
//!   settlement dates there and not for others, and no rule for which was found.
//! - ODDLPRICE and ODDLYIELD when the last interest date is the last day of a month, on a few bases.
//! - VDB with a fractional start period late in the asset's life and a factor other than 2.
//! - The COUPDAYS period length on basis 1 when a leap February and a maturity on the 28th meet.

use super::date::{days360, yearfrac};
use super::{num as plain_num, number, opt_bool, opt_num as plain_opt_num, s, Function};
use crate::dates;
use crate::eval::Ctx;
use crate::value::{ErrorKind, Value};

type Ymd = (i64, u32, u32);
type Res = Result<Value, ErrorKind>;

pub const FUNCTIONS: &[Function] = &[
    s(
        "COUPNUM",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "The number of coupons left to pay.",
        coupnum_fn,
    ),
    s(
        "COUPPCD",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "The coupon date before settlement.",
        couppcd_fn,
    ),
    s(
        "COUPNCD",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "The coupon date after settlement.",
        coupncd_fn,
    ),
    s(
        "COUPDAYBS",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "Days from the coupon period start to settlement.",
        coupdaybs_fn,
    ),
    s(
        "COUPDAYS",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "Days in the coupon period holding settlement.",
        coupdays_fn,
    ),
    s(
        "COUPDAYSNC",
        3,
        4,
        "settlement, maturity, frequency, [basis]",
        "Days from settlement to the next coupon.",
        coupdaysnc_fn,
    ),
    s(
        "PRICE",
        6,
        7,
        "settlement, maturity, rate, yld, redemption, frequency, [basis]",
        "The price of a coupon security per 100 of face value.",
        price_fn,
    ),
    s(
        "YIELD",
        6,
        7,
        "settlement, maturity, rate, pr, redemption, frequency, [basis]",
        "The yield of a coupon security.",
        yield_fn,
    ),
    s(
        "DURATION",
        5,
        6,
        "settlement, maturity, coupon, yld, frequency, [basis]",
        "The Macaulay duration of a coupon security.",
        duration_fn,
    ),
    s(
        "MDURATION",
        5,
        6,
        "settlement, maturity, coupon, yld, frequency, [basis]",
        "The modified duration of a coupon security.",
        mduration_fn,
    ),
    s(
        "ACCRINT",
        6,
        8,
        "issue, first_interest, settlement, rate, par, frequency, [basis], [calc_method]",
        "Accrued interest of a security paying periodic interest.",
        accrint_fn,
    ),
    s(
        "ACCRINTM",
        4,
        5,
        "issue, settlement, rate, par, [basis]",
        "Accrued interest of a security paid at maturity.",
        accrintm_fn,
    ),
    s(
        "DISC",
        4,
        5,
        "settlement, maturity, pr, redemption, [basis]",
        "The discount rate of a security.",
        disc_fn,
    ),
    s(
        "PRICEDISC",
        4,
        5,
        "settlement, maturity, discount, redemption, [basis]",
        "The price of a discounted security.",
        pricedisc_fn,
    ),
    s(
        "YIELDDISC",
        4,
        5,
        "settlement, maturity, pr, redemption, [basis]",
        "The annual yield of a discounted security.",
        yielddisc_fn,
    ),
    s(
        "RECEIVED",
        4,
        5,
        "settlement, maturity, investment, discount, [basis]",
        "The amount received at maturity of a fully invested security.",
        received_fn,
    ),
    s(
        "INTRATE",
        4,
        5,
        "settlement, maturity, investment, redemption, [basis]",
        "The interest rate of a fully invested security.",
        intrate_fn,
    ),
    s(
        "PRICEMAT",
        5,
        6,
        "settlement, maturity, issue, rate, yld, [basis]",
        "The price of a security paying interest at maturity.",
        pricemat_fn,
    ),
    s(
        "YIELDMAT",
        5,
        6,
        "settlement, maturity, issue, rate, pr, [basis]",
        "The yield of a security paying interest at maturity.",
        yieldmat_fn,
    ),
    s(
        "TBILLPRICE",
        3,
        3,
        "settlement, maturity, discount",
        "The price of a treasury bill per 100.",
        tbillprice_fn,
    ),
    s(
        "TBILLYIELD",
        3,
        3,
        "settlement, maturity, pr",
        "The yield of a treasury bill.",
        tbillyield_fn,
    ),
    s(
        "TBILLEQ",
        3,
        3,
        "settlement, maturity, discount",
        "The bond equivalent yield of a treasury bill.",
        tbilleq_fn,
    ),
    s(
        "ODDFPRICE",
        8,
        9,
        "settlement, maturity, issue, first_coupon, rate, yld, redemption, frequency, [basis]",
        "The price of a security with an odd first period.",
        oddfprice_fn,
    ),
    s(
        "ODDFYIELD",
        8,
        9,
        "settlement, maturity, issue, first_coupon, rate, pr, redemption, frequency, [basis]",
        "The yield of a security with an odd first period.",
        oddfyield_fn,
    ),
    s(
        "ODDLPRICE",
        7,
        8,
        "settlement, maturity, last_interest, rate, yld, redemption, frequency, [basis]",
        "The price of a security with an odd last period.",
        oddlprice_fn,
    ),
    s(
        "ODDLYIELD",
        7,
        8,
        "settlement, maturity, last_interest, rate, pr, redemption, frequency, [basis]",
        "The yield of a security with an odd last period.",
        oddlyield_fn,
    ),
    s(
        "DOLLARDE",
        2,
        2,
        "fractional_dollar, fraction",
        "A dollar price written as a fraction, as a decimal.",
        dollarde_fn,
    ),
    s(
        "DOLLARFR",
        2,
        2,
        "decimal_dollar, fraction",
        "A decimal dollar price written as a fraction.",
        dollarfr_fn,
    ),
    s(
        "AMORLINC",
        6,
        7,
        "cost, date_purchased, first_period, salvage, period, rate, [basis]",
        "French straight line depreciation for a period.",
        amorlinc_fn,
    ),
    s(
        "AMORDEGRC",
        6,
        7,
        "cost, date_purchased, first_period, salvage, period, rate, [basis]",
        "French declining balance depreciation for a period.",
        amordegrc_fn,
    ),
    s(
        "VDB",
        5,
        7,
        "cost, salvage, life, start_period, end_period, [factor], [no_switch]",
        "Variable declining balance depreciation.",
        vdb_fn,
    ),
];

// ---------------------------------------------------------------------------------------------------
// Argument reading.

/// A number argument. Excel's bond functions refuse a logical value where a number belongs.
fn num(ctx: &Ctx, v: &Value) -> Result<f64, ErrorKind> {
    if matches!(v, Value::Bool(_)) {
        return Err(ErrorKind::Value);
    }
    plain_num(ctx, v)
}

/// The optional number argument at `i`, or `default` when it was left out.
fn opt_num(ctx: &Ctx, args: &[Value], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match args.get(i) {
        None => Ok(default),
        Some(v) => num(ctx, v),
    }
}

/// A date argument: truncated to a whole serial, which must be a date the workbook can hold.
fn day(ctx: &Ctx, v: &Value) -> Result<i64, ErrorKind> {
    if matches!(v, Value::Bool(_)) {
        return Err(ErrorKind::Value);
    }
    let n = num(ctx, v)?.floor();
    if n < 0.0 || dates::date(n, ctx.date1904()).is_none() {
        return Err(ErrorKind::Num);
    }
    Ok(n as i64)
}

/// The optional day count basis at `i`, which must be a whole number from 0 to 4 once truncated.
fn basis_at(ctx: &Ctx, a: &[Value], i: usize) -> Result<i64, ErrorKind> {
    let b = opt_num(ctx, a, i, 0.0)?.trunc();
    if (0.0..=4.0).contains(&b) {
        Ok(b as i64)
    } else {
        Err(ErrorKind::Num)
    }
}

/// The par value at `i`, which is 1000 when it was left out or left empty.
fn par_at(ctx: &Ctx, a: &[Value], i: usize) -> Result<f64, ErrorKind> {
    match a.get(i) {
        None | Some(Value::Empty) => Ok(1000.0),
        Some(v) => num(ctx, v),
    }
}

/// A coupon frequency: 1 annual, 2 semiannual or 4 quarterly.
fn frequency(ctx: &Ctx, v: &Value) -> Result<i64, ErrorKind> {
    let f = num(ctx, v)?.trunc();
    if f == 1.0 || f == 2.0 || f == 4.0 {
        Ok(f as i64)
    } else {
        Err(ErrorKind::Num)
    }
}

/// The settlement and maturity arguments.
fn settle_and_maturity(ctx: &Ctx, a: &[Value]) -> Result<(i64, i64), ErrorKind> {
    Ok((day(ctx, &a[0])?, day(ctx, &a[1])?))
}

/// The calendar in force for the basis at `i`.
fn calendar(ctx: &Ctx, a: &[Value], i: usize) -> Result<Cal, ErrorKind> {
    Ok(Cal { d1904: ctx.date1904(), basis: basis_at(ctx, a, i)? })
}

/// The arguments shared by the COUP functions, with the coupon period they describe.
fn coupon_period(ctx: &Ctx, a: &[Value]) -> Result<(Cal, i64, i64, i64), ErrorKind> {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let freq = frequency(ctx, &a[2])?;
    let cal = calendar(ctx, a, 3)?;
    if settle >= mat {
        return Err(ErrorKind::Num);
    }
    Ok((cal, settle, mat, freq))
}

/// COUPNUM: how many coupons are still to be paid between settlement and maturity.
fn coupnum_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    number(cal.dates(settle, mat, freq).count as f64)
}

/// COUPPCD: the coupon date on or before settlement.
fn couppcd_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    number(cal.dates(settle, mat, freq).previous as f64)
}

/// COUPNCD: the first coupon date after settlement.
fn coupncd_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    number(cal.dates(settle, mat, freq).next as f64)
}

/// COUPDAYBS: the days from the start of the coupon period to settlement.
fn coupdaybs_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    number(period(cal, settle, mat, freq).a)
}

/// COUPDAYS: the days in the coupon period that holds settlement.
fn coupdays_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    if cal.basis == 1 {
        return number(cal.actual_period_length(settle, mat, 12 / freq) as f64);
    }
    number(period(cal, settle, mat, freq).e)
}

/// COUPDAYSNC: the days from settlement to the next coupon date.
fn coupdaysnc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, freq) = coupon_period(ctx, a)?;
    number(period(cal, settle, mat, freq).dsc)
}

// ---------------------------------------------------------------------------------------------------
// Calendars and coupon periods.

/// A day count basis in a workbook's date system.
#[derive(Clone, Copy)]
struct Cal {
    d1904: bool,
    basis: i64,
}

/// The coupon dates either side of settlement and how many coupons are left.
struct CouponDates {
    previous: i64,
    next: i64,
    count: i64,
}

impl Cal {
    /// The year, month and day of a serial in the workbook's date system.
    fn ymd(self, serial: i64) -> Ymd {
        dates::date(serial as f64, self.d1904).unwrap_or((1900, 1, 1))
    }

    /// Days between two dates under the basis: 30/360 for bases 0 and 4, the real count otherwise.
    fn days(self, from: i64, to: i64) -> f64 {
        match self.basis {
            0 => days360(self.ymd(from), self.ymd(to), false) as f64,
            4 => days360(self.ymd(from), self.ymd(to), true) as f64,
            _ => (to - from) as f64,
        }
    }

    /// The years between two dates under the basis, the same figure YEARFRAC gives.
    fn year_fraction(self, from: i64, to: i64) -> Result<f64, ErrorKind> {
        yearfrac(from as f64, to as f64, self.basis, self.d1904)
    }

    /// A date `months` before `anchor`. With `snap` a date on the last day of its month stays on the last
    /// day of the month it lands in. Any other day, and every day when `snap` is off, is cut back to the
    /// length of the month it lands in.
    fn shift_back(self, anchor: i64, months: i64, snap: bool) -> i64 {
        let (y, m, d) = self.ymd(anchor);
        let last = d == dates::days_in_month(y, m);
        let total = y * 12 + i64::from(m) - 1 - months;
        let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
        let length = i64::from(dates::days_in_month(ny, nm));
        let nd = if snap && last { length } else { i64::from(d).min(length) };
        dates::serial(ny, i64::from(nm), nd, self.d1904).map_or(0, |s| s as i64)
    }

    /// A date `months` before `anchor`, keeping a last day of the month on the last day of the month.
    fn months_before(self, anchor: i64, months: i64) -> i64 {
        self.shift_back(anchor, months, true)
    }

    /// The length in days of the coupon period holding settlement, as COUPDAYS counts it for basis 1.
    /// Excel walks back from maturity along two schedules, one that keeps a last day of the month on the
    /// last day and one that carries the day it was cut back to, takes the earliest date after settlement
    /// on either, and measures back from it.
    fn actual_period_length(self, settle: i64, mat: i64, step: i64) -> i64 {
        let next = [true, false]
            .into_iter()
            .map(|snap| {
                let mut date = mat;
                loop {
                    let earlier = self.shift_back(date, step, snap);
                    if earlier <= settle {
                        return date;
                    }
                    date = earlier;
                }
            })
            .min()
            .unwrap_or(mat);
        next - self.shift_back(next, step, true)
    }

    /// The coupon dates round settlement, counted back from maturity in steps of `12 / freq` months.
    fn dates(self, settle: i64, mat: i64, freq: i64) -> CouponDates {
        let step = 12 / freq;
        let (sy, sm, _) = self.ymd(settle);
        let (my, mm, _) = self.ymd(mat);
        let gap = (my * 12 + i64::from(mm)) - (sy * 12 + i64::from(sm));
        let mut k = (gap / step - 1).max(0);
        while self.months_before(mat, (k + 1) * step) > settle {
            k += 1;
        }
        while k > 0 && self.months_before(mat, k * step) <= settle {
            k -= 1;
        }
        CouponDates {
            previous: self.months_before(mat, (k + 1) * step),
            next: self.months_before(mat, k * step),
            count: k + 1,
        }
    }
}

/// The figures every price and yield formula is written in: coupons left, days from the start of the
/// coupon period to settlement, days in the period, and days from settlement to the next coupon.
struct Period {
    n: f64,
    a: f64,
    e: f64,
    dsc: f64,
}

/// The coupon period figures for a security at settlement.
fn period(cal: Cal, settle: i64, mat: i64, freq: i64) -> Period {
    let c = cal.dates(settle, mat, freq);
    let f = freq as f64;
    let e = match cal.basis {
        1 => (c.next - c.previous) as f64,
        3 => 365.0 / f,
        _ => 360.0 / f,
    };
    let a = cal.days(c.previous, settle);
    let dsc = if cal.basis == 0 { e - a } else { cal.days(settle, c.next) };
    Period { n: c.count as f64, a, e, dsc }
}

// ---------------------------------------------------------------------------------------------------
// Coupon securities.

/// The price per 100 of a security paying `rate` a year in `freq` coupons, discounted at `yld`.
fn coupon_price(p: &Period, rate: f64, yld: f64, red: f64, freq: f64) -> f64 {
    let coupon = 100.0 * rate / freq;
    let k = 1.0 + yld / freq;
    let ahead = (p.e - p.a) / p.e;
    if p.n == 1.0 {
        return (red + coupon) / (1.0 + ahead * yld / freq) - p.a / p.e * coupon;
    }
    let mut value = red / k.powf(p.n - 1.0 + ahead);
    for j in 0..p.n as i64 {
        value += coupon / k.powf(j as f64 + ahead);
    }
    value - coupon * p.a / p.e
}

/// The yield at which a price, which falls as the yield rises, equals `target`.
fn solve_yield(price: impl Fn(f64) -> f64, target: f64, freq: f64) -> Result<f64, ErrorKind> {
    let g = |y: f64| price(y) - target;
    let mut low = -freq + 1e-9;
    let mut high = 1.0;
    if !g(low).is_finite() || g(low) < 0.0 {
        return Err(ErrorKind::Num);
    }
    while g(high) > 0.0 {
        high *= 2.0;
        if high > 1e9 {
            return Err(ErrorKind::Num);
        }
    }
    for _ in 0..300 {
        let mid = 0.5 * (low + high);
        if g(mid) > 0.0 {
            low = mid;
        } else {
            high = mid;
        }
    }
    Ok(0.5 * (low + high))
}

/// PRICE: the price per 100 of face value of a security paying periodic interest.
fn price_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let (rate, yld, red) = (num(ctx, &a[2])?, num(ctx, &a[3])?, num(ctx, &a[4])?);
    let freq = frequency(ctx, &a[5])?;
    let cal = calendar(ctx, a, 6)?;
    if settle >= mat || rate < 0.0 || yld < 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(coupon_price(&period(cal, settle, mat, freq), rate, yld, red, freq as f64))
}

/// YIELD: the annual yield of a security paying periodic interest, solved from its price.
fn yield_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let (rate, pr, red) = (num(ctx, &a[2])?, num(ctx, &a[3])?, num(ctx, &a[4])?);
    let freq = frequency(ctx, &a[5])?;
    let cal = calendar(ctx, a, 6)?;
    if settle >= mat || rate < 0.0 || pr <= 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let p = period(cal, settle, mat, freq);
    let f = freq as f64;
    if p.n == 1.0 {
        // With one coupon left Excel solves for the yield directly. On the actual/365 basis it counts the
        // days of the period as they are, where the price formula uses a year of 365 days over the frequency.
        let (e, dsc) = if cal.basis == 3 { (p.a + p.dsc, p.dsc) } else { (p.e, p.e - p.a) };
        let coupon = rate / f;
        let base = pr / 100.0 + p.a / e * coupon;
        return number(((red / 100.0 + coupon) - base) / base * f * e / dsc);
    }
    number(solve_yield(|y| coupon_price(&p, rate, y, red, f), pr, f)?)
}

/// Macaulay duration in years, with the yield and frequency it was worked out for.
fn duration_of(ctx: &Ctx, a: &[Value]) -> Result<(f64, f64, f64), ErrorKind> {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let (coupon, yld) = (num(ctx, &a[2])?, num(ctx, &a[3])?);
    let freq = frequency(ctx, &a[4])?;
    let cal = calendar(ctx, a, 5)?;
    if settle >= mat || coupon < 0.0 || yld < 0.0 {
        return Err(ErrorKind::Num);
    }
    let f = freq as f64;
    let p = period(cal, settle, mat, freq);
    let ahead = (p.e - p.a) / p.e;
    let c = 100.0 * coupon / f;
    let k = 1.0 + yld / f;
    let (mut weighted, mut value) = (0.0, 0.0);
    for t in 0..p.n as i64 {
        let cash = if t as f64 + 1.0 == p.n { c + 100.0 } else { c };
        let years = t as f64 + ahead;
        weighted += years * cash / k.powf(years);
        value += cash / k.powf(years);
    }
    Ok((weighted / value / f, yld, f))
}

/// DURATION: the Macaulay duration in years.
fn duration_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    number(duration_of(ctx, a)?.0)
}

/// MDURATION: the modified Macaulay duration in years.
fn mduration_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (d, yld, f) = duration_of(ctx, a)?;
    number(d / (1.0 + yld / f))
}

// ---------------------------------------------------------------------------------------------------
// Accrued interest.

/// ACCRINTM: the interest accrued by a security that pays at maturity.
fn accrintm_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (issue, settle) = (day(ctx, &a[0])?, day(ctx, &a[1])?);
    let (rate, par) = (num(ctx, &a[2])?, par_at(ctx, a, 3)?);
    let cal = calendar(ctx, a, 4)?;
    if issue > settle || rate <= 0.0 || par <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(par * rate * cal.year_fraction(issue, settle)?)
}

/// ACCRINT: the interest accrued by a security that pays periodic interest.
fn accrint_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (issue, first, settle) = (day(ctx, &a[0])?, day(ctx, &a[1])?, day(ctx, &a[2])?);
    let (rate, par) = (num(ctx, &a[3])?, par_at(ctx, a, 4)?);
    let freq = frequency(ctx, &a[5])?;
    let cal = calendar(ctx, a, 6)?;
    let from_issue = opt_bool(a, 7, true)?;
    if issue >= settle || rate <= 0.0 || par <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let units = Accrual { cal, first, freq }.units(issue, settle, from_issue);
    number(par * rate / freq as f64 * units)
}

/// The coupon periods an accrual is counted in: quasi coupon dates step back from the first interest date.
struct Accrual {
    cal: Cal,
    first: i64,
    freq: i64,
}

impl Accrual {
    /// The quasi coupon date `k` periods before the first interest date.
    fn quasi(&self, k: i64) -> i64 {
        self.cal.months_before(self.first, k * (12 / self.freq))
    }

    /// The length in days of a coupon period that ends at or after the first interest date: the real
    /// length of the period that ends at it on basis 1, and a year over the frequency otherwise.
    fn normal_length(&self) -> f64 {
        match self.cal.basis {
            1 => (self.first - self.quasi(1)) as f64,
            3 => 365.0 / self.freq as f64,
            _ => 360.0 / self.freq as f64,
        }
    }

    /// The length of the quasi period `k`, which ends at the quasi date `k - 1`.
    fn length_of(&self, k: i64) -> f64 {
        match self.cal.basis {
            1 => (self.quasi(k - 1) - self.quasi(k)) as f64,
            3 => 365.0 / self.freq as f64,
            _ => 360.0 / self.freq as f64,
        }
    }

    /// Accrued interest from `issue` to `settle` in units of one coupon period.
    ///
    /// Up to the first interest date the quasi periods inside the span count as one each, except that the
    /// period ending at that date counts by its days when settlement is not past it. A settlement before
    /// the first interest date takes the days back from that date off the total at the normal length.
    /// Beyond the first interest date whole periods count as one and the last part counts against the
    /// normal length. Counting from the first interest date instead leaves out the whole quasi periods
    /// between issue and the last two.
    fn units(&self, issue: i64, settle: i64, from_issue: bool) -> f64 {
        let total = if settle <= self.first {
            if self.cal.basis == 1 {
                self.odd_part(issue, self.first, true)
                    - self.cal.days(settle, self.first) / self.normal_length()
            } else {
                self.odd_part(issue, settle, true)
            }
        } else if from_issue {
            self.odd_part(issue, self.first, false) + self.regular_part(settle)
        } else {
            self.odd_part(issue, self.first, true)
                + self.cal.days(self.first, settle) / self.normal_length()
        };
        if from_issue {
            return total;
        }
        let left_out = (2..).take_while(|&k| k < 5000 && self.quasi(k) >= issue).count();
        total - left_out as f64
    }

    /// The quasi periods between `start` and `end`, which is at most the first interest date.
    fn odd_part(&self, start: i64, end: i64, by_days_at_the_end: bool) -> f64 {
        let mut total = 0.0;
        let mut k = 1;
        while self.quasi(k - 1) > start && k < 5000 {
            let (lo, hi) = (self.quasi(k).max(start), self.quasi(k - 1).min(end));
            if hi > lo {
                let whole = lo == self.quasi(k) && hi == self.quasi(k - 1);
                let counted_by_days = !whole || (k == 1 && by_days_at_the_end);
                total +=
                    if counted_by_days { self.cal.days(lo, hi) / self.length_of(k) } else { 1.0 };
            }
            k += 1;
        }
        total
    }

    /// The part of the accrual past the first interest date: whole coupon periods and then the days into
    /// the next one.
    fn regular_part(&self, settle: i64) -> f64 {
        let step = 12 / self.freq;
        let mut whole = 0.0;
        let mut previous = self.first;
        let mut j = 1;
        loop {
            let next = self.cal.months_before(self.first, -j * step);
            if next >= settle || next <= previous {
                break;
            }
            whole += 1.0;
            previous = next;
            j += 1;
        }
        whole + self.cal.days(previous, settle) / self.normal_length()
    }
}

// ---------------------------------------------------------------------------------------------------
// Securities that pay at maturity, and treasury bills.

/// Settlement, maturity, two numbers and a basis, the shape of the discount security functions.
fn discount_args(ctx: &Ctx, a: &[Value]) -> Result<(Cal, i64, i64, f64, f64), ErrorKind> {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let (x, y) = (num(ctx, &a[2])?, num(ctx, &a[3])?);
    let cal = calendar(ctx, a, 4)?;
    if settle >= mat {
        return Err(ErrorKind::Num);
    }
    Ok((cal, settle, mat, x, y))
}

/// DISC: the discount rate of a security bought at a price and redeemed at maturity.
fn disc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, pr, red) = discount_args(ctx, a)?;
    if pr <= 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number((red - pr) / red / cal.year_fraction(settle, mat)?)
}

/// PRICEDISC: the price per 100 of a security sold at a discount rate.
fn pricedisc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, disc, red) = discount_args(ctx, a)?;
    if disc <= 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(red - disc * red * cal.year_fraction(settle, mat)?)
}

/// YIELDDISC: the annual yield of a security sold at a discount.
fn yielddisc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, pr, red) = discount_args(ctx, a)?;
    if pr <= 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number((red / pr - 1.0) / cal.year_fraction(settle, mat)?)
}

/// RECEIVED: the amount received at maturity for a security bought at a discount.
fn received_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, inv, disc) = discount_args(ctx, a)?;
    if inv <= 0.0 || disc <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(inv / (1.0 - disc * cal.year_fraction(settle, mat)?))
}

/// INTRATE: the interest rate of a security bought for an amount and redeemed for another.
fn intrate_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, inv, red) = discount_args(ctx, a)?;
    if inv <= 0.0 || red <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number((red - inv) / inv / cal.year_fraction(settle, mat)?)
}

/// The arguments of PRICEMAT and YIELDMAT: settlement, maturity, issue, rate, a price or yield, basis.
fn maturity_args(ctx: &Ctx, a: &[Value]) -> Result<(Cal, i64, i64, i64, f64, f64), ErrorKind> {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let issue = day(ctx, &a[2])?;
    let (rate, x) = (num(ctx, &a[3])?, num(ctx, &a[4])?);
    let cal = calendar(ctx, a, 5)?;
    if settle >= mat || issue >= settle || rate < 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok((cal, settle, mat, issue, rate, x))
}

/// A span of days as a fraction of a year for PRICEMAT and YIELDMAT. These use one year length for every
/// span, the one YEARFRAC works out for the stretch from issue to settlement, so a span that crosses
/// several years is not divided by its own average.
fn over_issue_year(
    cal: Cal,
    issue: i64,
    settle: i64,
    from: i64,
    to: i64,
) -> Result<f64, ErrorKind> {
    let year = match cal.basis {
        1 => (settle - issue) as f64 / cal.year_fraction(issue, settle)?,
        3 => 365.0,
        _ => 360.0,
    };
    Ok(cal.days(from, to) / year)
}

/// PRICEMAT: the price per 100 of a security that pays its interest at maturity.
fn pricemat_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, issue, rate, yld) = maturity_args(ctx, a)?;
    if yld < 0.0 {
        return Err(ErrorKind::Num);
    }
    let im = over_issue_year(cal, issue, settle, issue, mat)?;
    let sm = over_issue_year(cal, issue, settle, settle, mat)?;
    let is = over_issue_year(cal, issue, settle, issue, settle)?;
    number((100.0 + im * rate * 100.0) / (1.0 + sm * yld) - is * rate * 100.0)
}

/// YIELDMAT: the annual yield of a security that pays its interest at maturity.
fn yieldmat_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cal, settle, mat, issue, rate, pr) = maturity_args(ctx, a)?;
    if pr <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let im = over_issue_year(cal, issue, settle, issue, mat)?;
    let sm = over_issue_year(cal, issue, settle, settle, mat)?;
    let is = over_issue_year(cal, issue, settle, issue, settle)?;
    let base = pr / 100.0 + is * rate;
    number(((1.0 + im * rate) - base) / base / sm)
}

/// Days to maturity and the rate or price of a treasury bill.
fn bill_args(ctx: &Ctx, a: &[Value]) -> Result<(f64, f64), ErrorKind> {
    let (settle, mat) = settle_and_maturity(ctx, a)?;
    let x = num(ctx, &a[2])?;
    if settle >= mat || mat - settle > 365 || x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    Ok(((mat - settle) as f64, x))
}

/// TBILLPRICE: the price per 100 of a treasury bill.
fn tbillprice_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (days, disc) = bill_args(ctx, a)?;
    let price = 100.0 * (1.0 - disc * days / 360.0);
    if price <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(price)
}

/// TBILLYIELD: the yield of a treasury bill.
fn tbillyield_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (days, pr) = bill_args(ctx, a)?;
    number((100.0 - pr) / pr * 360.0 / days)
}

/// The bond equivalent yield of a bill. Up to half a year it is the simple ratio. Beyond that the bill
/// is treated as a bond with one coupon and the yield solves a quadratic in the price.
fn tbilleq_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (days, disc) = bill_args(ctx, a)?;
    let price = 100.0 * (1.0 - disc * days / 360.0);
    if price <= 0.0 {
        return Err(ErrorKind::Num);
    }
    if days <= 182.0 {
        return number(365.0 * disc / (360.0 - disc * days));
    }
    let x = days / 365.0;
    let b = 2.0 * x - 1.0;
    let root = x * x - b * (1.0 - 100.0 / price);
    if root < 0.0 {
        return Err(ErrorKind::Num);
    }
    number((-2.0 * x + 2.0 * root.sqrt()) / b)
}

// ---------------------------------------------------------------------------------------------------
// Odd first and last periods.

/// ODDFPRICE: the price per 100 of a security with an odd first coupon period.
fn oddfprice_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let o = OddFirst::read(ctx, a)?;
    number(o.price(o.x))
}

/// ODDFYIELD: the yield of a security with an odd first coupon period, solved from its price.
fn oddfyield_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let o = OddFirst::read(ctx, a)?;
    if o.x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    number(solve_yield(|y| o.price(y), o.x, o.freq)?)
}

/// A security with an odd first coupon period; `x` is the yield for ODDFPRICE and the price for ODDFYIELD.
struct OddFirst {
    cal: Cal,
    settle: i64,
    mat: i64,
    issue: i64,
    first: i64,
    rate: f64,
    x: f64,
    red: f64,
    freq: f64,
}

impl OddFirst {
    /// Reads and checks the arguments of ODDFPRICE and ODDFYIELD.
    fn read(ctx: &Ctx, a: &[Value]) -> Result<Self, ErrorKind> {
        let settle = day(ctx, &a[0])?;
        let (mat, issue, first) = (day(ctx, &a[1])?, day(ctx, &a[2])?, day(ctx, &a[3])?);
        let (rate, x, red) = (num(ctx, &a[4])?, num(ctx, &a[5])?, num(ctx, &a[6])?);
        let freq = frequency(ctx, &a[7])?;
        let cal = calendar(ctx, a, 8)?;
        if !(mat > first && first > settle && settle > issue) || rate < 0.0 || x < 0.0 || red <= 0.0
        {
            return Err(ErrorKind::Num);
        }
        // The first coupon has to fall on the coupon schedule that counts back from maturity.
        let (fy, fm, _) = cal.ymd(first);
        let (my, mm, _) = cal.ymd(mat);
        if ((my - fy) * 12 + i64::from(mm) - i64::from(fm)) % (12 / freq) != 0 {
            return Err(ErrorKind::Num);
        }
        Ok(OddFirst { cal, settle, mat, issue, first, rate, x, red, freq: freq as f64 })
    }

    /// The price at a yield.
    fn price(&self, yld: f64) -> f64 {
        let cal = self.cal;
        let freq = self.freq as i64;
        let coupon = 100.0 * self.rate / self.freq;
        let k = 1.0 + yld / self.freq;
        let n = 1 + cal.dates(self.first, self.mat, freq).count;
        let acc = Accrual { cal, first: self.first, freq };
        let ahead = acc.odd_part(self.settle, self.first, true);
        let odd = acc.odd_part(self.issue, self.first, false);
        let accrued = acc.units(self.issue, self.settle, true);
        let mut value = self.red / k.powf(n as f64 - 1.0 + ahead);
        value += coupon * odd / k.powf(ahead);
        for j in 2..=n {
            value += coupon / k.powf(j as f64 - 1.0 + ahead);
        }
        value - coupon * accrued
    }
}

/// The arguments of ODDLPRICE and ODDLYIELD.
struct OddLast {
    cal: Cal,
    settle: i64,
    mat: i64,
    last: i64,
    rate: f64,
    x: f64,
    red: f64,
    freq: f64,
}

impl OddLast {
    /// Reads and checks the arguments of ODDLPRICE and ODDLYIELD.
    fn read(ctx: &Ctx, a: &[Value]) -> Result<Self, ErrorKind> {
        let (settle, mat, last) = (day(ctx, &a[0])?, day(ctx, &a[1])?, day(ctx, &a[2])?);
        let (rate, x, red) = (num(ctx, &a[3])?, num(ctx, &a[4])?, num(ctx, &a[5])?);
        let freq = frequency(ctx, &a[6])?;
        let cal = calendar(ctx, a, 7)?;
        if !(mat > settle && settle > last) || rate < 0.0 || x < 0.0 || red <= 0.0 {
            return Err(ErrorKind::Num);
        }
        Ok(OddLast { cal, settle, mat, last, rate, x, red, freq: freq as f64 })
    }

    /// Coupon periods from `from` to `to`. Quasi periods run forward from the last interest date, each
    /// date a step after the one before it with its day cut back to the month when it does not fit.
    /// A period inside the span counts as one and a part of a period counts by its days over the length
    /// of the period, which is its real length except on 30/360.
    fn periods(&self, from: i64, to: i64) -> f64 {
        let step = (12.0 / self.freq) as i64;
        let (mut total, mut start) = (0.0, self.last);
        while start < to {
            let end = self.cal.shift_back(start, -step, false);
            if end <= start {
                break;
            }
            let (lo, hi) = (start.max(from), end.min(to));
            if hi > lo {
                total += if lo == start && hi == end {
                    1.0
                } else {
                    let length = match self.cal.basis {
                        0 | 4 => 360.0 / self.freq,
                        _ => (end - start) as f64,
                    };
                    self.cal.days(lo, hi) / length
                };
            }
            start = end;
        }
        total
    }

    /// Periods from the last interest date to maturity, from settlement to maturity, and from the last
    /// interest date to settlement.
    fn spans(&self) -> (f64, f64, f64) {
        (
            self.periods(self.last, self.mat),
            self.periods(self.settle, self.mat),
            self.periods(self.last, self.settle),
        )
    }
}

/// ODDLPRICE: the price per 100 of a security with an odd last coupon period.
fn oddlprice_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let o = OddLast::read(ctx, a)?;
    let (dc, dsc, ai) = o.spans();
    let coupon = 100.0 * o.rate / o.freq;
    number((o.red + dc * coupon) / (dsc * o.x / o.freq + 1.0) - ai * coupon)
}

/// ODDLYIELD: the yield of a security with an odd last coupon period.
fn oddlyield_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let o = OddLast::read(ctx, a)?;
    if o.x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    let (dc, dsc, ai) = o.spans();
    let coupon = 100.0 * o.rate / o.freq;
    let base = o.x + ai * coupon;
    number(((o.red + dc * coupon) - base) / base * o.freq / dsc)
}

// ---------------------------------------------------------------------------------------------------
// Fractional dollars.

/// The number of decimal digits that hold a fraction's denominator.
fn fraction_digits(fraction: f64) -> f64 {
    fraction.log10().ceil()
}

/// The denominator of a fractional price: whole, not negative, and not zero.
fn fraction_arg(ctx: &Ctx, v: &Value) -> Result<f64, ErrorKind> {
    let f = num(ctx, v)?.trunc();
    if f < 0.0 {
        return Err(ErrorKind::Num);
    }
    if f == 0.0 {
        return Err(ErrorKind::Div0);
    }
    Ok(f)
}

/// DOLLARDE: a price written as dollars and a fraction, as a decimal.
fn dollarde_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let dollar = num(ctx, &a[0])?;
    let fraction = fraction_arg(ctx, &a[1])?;
    let whole = dollar.abs().trunc();
    let part = dollar.abs() - whole;
    let value = whole + part * 10f64.powf(fraction_digits(fraction)) / fraction;
    number(value.copysign(dollar))
}

/// DOLLARFR: a decimal price as dollars and a fraction.
fn dollarfr_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let dollar = num(ctx, &a[0])?;
    let fraction = fraction_arg(ctx, &a[1])?;
    let whole = dollar.abs().trunc();
    let part = dollar.abs() - whole;
    let value = whole + part * fraction / 10f64.powf(fraction_digits(fraction));
    number(value.copysign(dollar))
}

// ---------------------------------------------------------------------------------------------------
// Depreciation.

/// The arguments of the French depreciation functions.
struct French {
    cal: Cal,
    cost: f64,
    purchased: i64,
    first: i64,
    salvage: f64,
    period: f64,
    rate: f64,
}

impl French {
    /// Reads and checks the arguments of AMORLINC and AMORDEGRC.
    fn read(ctx: &Ctx, a: &[Value]) -> Result<Self, ErrorKind> {
        let cost = num(ctx, &a[0])?;
        let (purchased, first) = (day(ctx, &a[1])?, day(ctx, &a[2])?);
        let (salvage, period, rate) = (num(ctx, &a[3])?, num(ctx, &a[4])?, num(ctx, &a[5])?);
        let cal = calendar(ctx, a, 6)?;
        let bad = cost <= 0.0 || salvage < 0.0 || salvage > cost || period < 0.0 || rate <= 0.0;
        if bad || cal.basis == 2 || purchased > first {
            return Err(ErrorKind::Num);
        }
        Ok(French { cal, cost, purchased, first, salvage, period: period.trunc(), rate })
    }
}

/// AMORLINC: French straight line depreciation for one period.
fn amorlinc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let f = French::read(ctx, a)?;
    let one = f.cost * f.rate;
    let first_period = f.cal.year_fraction(f.purchased, f.first)? * f.rate * f.cost;
    let full = ((f.cost - f.salvage - first_period) / one).trunc();
    let result = if f.period == 0.0 {
        first_period
    } else if f.period <= full {
        one
    } else if f.period == full + 1.0 {
        f.cost - f.salvage - one * full - first_period
    } else {
        0.0
    };
    number(result)
}

/// The multiplier AMORDEGRC puts on the rate for an asset of this life in years: Excel gives none to a
/// life of two years or less, which is an error.
fn amor_coefficient(life: f64) -> Option<f64> {
    if life <= 2.0 {
        None
    } else if life <= 4.0 + 1e-9 {
        Some(1.5)
    } else if life < 6.0 - 1e-9 {
        Some(2.0)
    } else {
        Some(2.5)
    }
}

/// AMORDEGRC: French declining balance depreciation for one period.
fn amordegrc_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let f = French::read(ctx, a)?;
    let life = 1.0 / f.rate;
    let rate = f.rate * amor_coefficient(life).ok_or(ErrorKind::Num)?;
    let periods = life.ceil();
    let first =
        (f.cal.year_fraction(f.purchased, f.first)? * rate * f.cost).min(f.cost - f.salvage);
    if f.period == 0.0 {
        return number(first.round());
    }
    // The book value and the room left above salvage are carried unrounded; only the answer is rounded.
    let (mut cost, mut rest) = (f.cost - first, f.cost - first - f.salvage);
    let (mut step, mut half) = (0.0, None);
    for k in 1..=f.period as i64 {
        if k as f64 >= periods || rest < 0.0 {
            return number(0.0);
        }
        // The last two periods each take half of the book value the first of them started with.
        if k as f64 >= periods - 2.0 {
            step = *half.get_or_insert(cost * 0.5);
        } else {
            step = rate * cost;
        }
        rest -= step;
        cost -= step;
    }
    number(step.round())
}

/// Declining balance depreciation for one whole period, never taking the book value below salvage.
fn declining_term(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> f64 {
    let mut rate = factor / life;
    let old = if rate >= 1.0 {
        rate = 1.0;
        if period == 1.0 {
            cost
        } else {
            0.0
        }
    } else {
        cost * (1.0 - rate).powf(period - 1.0)
    };
    let new = cost * (1.0 - rate).powf(period);
    let term = if new < salvage { old - salvage } else { old - new };
    // Past the first period a book value already under salvage depreciates by nothing. In the first
    // period Excel lets a salvage above the cost through as a negative figure.
    if period > 1.0 {
        term.max(0.0)
    } else {
        term
    }
}

/// Depreciation over `span` periods, switching to straight line over `life1` when that is larger.
fn declining_with_switch(
    cost: f64,
    salvage: f64,
    life: f64,
    life1: f64,
    span: f64,
    factor: f64,
) -> f64 {
    let end = span.ceil();
    let mut remaining = cost - salvage;
    let mut straight = 0.0;
    let mut on_straight = false;
    let mut total = 0.0;
    for i in 1..=end as i64 {
        let mut term;
        if on_straight {
            term = straight;
        } else {
            let declining = declining_term(cost, salvage, life, i as f64, factor);
            straight = remaining / (life1 - (i - 1) as f64);
            if straight > declining && remaining > 0.0 {
                term = straight;
                on_straight = true;
            } else {
                term = declining;
                remaining -= declining;
            }
        }
        if i as f64 == end {
            term *= span + 1.0 - end;
        }
        total += term;
    }
    total
}

/// VDB: variable declining balance depreciation between two periods.
fn vdb_fn(ctx: &mut Ctx, a: &[Value]) -> Res {
    let (cost, salvage, life) =
        (plain_num(ctx, &a[0])?, plain_num(ctx, &a[1])?, plain_num(ctx, &a[2])?);
    let (start, end) = (plain_num(ctx, &a[3])?, plain_num(ctx, &a[4])?);
    let factor = plain_opt_num(ctx, a, 5, 2.0)?;
    let no_switch = opt_bool(a, 6, false)?;
    let bad = start < 0.0 || end < start || end > life || cost < 0.0 || salvage < 0.0;
    if bad || factor < 0.0 || life <= 0.0 {
        return Err(ErrorKind::Num);
    }
    if no_switch {
        return number(vdb_no_switch(cost, salvage, life, start, end, factor));
    }
    let before = declining_with_switch(cost, salvage, life, life, start, factor);
    number(declining_with_switch(cost - before, salvage, life, life - start, end - start, factor))
}

/// Declining balance depreciation between two periods that never switches to straight line.
fn vdb_no_switch(cost: f64, salvage: f64, life: f64, start: f64, end: f64, factor: f64) -> f64 {
    let (first, last) = (start.floor(), end.ceil());
    let mut total = 0.0;
    let mut i = first + 1.0;
    while i <= last {
        let mut term = declining_term(cost, salvage, life, i, factor);
        if i == first + 1.0 {
            term *= end.min(first + 1.0) - start;
        } else if i == last {
            term *= end + 1.0 - last;
        }
        total += term;
        i += 1.0;
    }
    total
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn securities_answer_what_excel_answers() {
        check(&[
            ("COUPNUM(DATE(2023,3,15),DATE(2028,9,30),2,0)", "12"),
            ("COUPPCD(DATE(2023,3,15),DATE(2028,9,30),2,0)", "44834"),
            ("COUPDAYBS(DATE(2023,3,15),DATE(2028,9,30),2,0)", "165"),
            ("PRICE(DATE(2023,3,15),DATE(2028,9,30),0.0575,0.065,100,2,0)", "96.5526918497"),
            ("YIELD(DATE(2023,3,15),DATE(2028,9,30),0.0575,95.04287,100,2,0)", "0.0683892325548"),
            ("DISC(DATE(2023,2,15),DATE(2023,6,10),97.975,100,0)", "0.0633913043478"),
            (
                "ACCRINT(DATE(2023,3,1),DATE(2023,8,31),DATE(2023,5,1),0.1,1000,2,0)",
                "16.6666666667",
            ),
            ("TBILLPRICE(DATE(2023,3,31),DATE(2023,6,30),0.09)", "97.725"),
            ("TBILLEQ(DATE(2023,3,31),DATE(2024,3,30),0.0914)", "0.0996515523697"),
            ("DOLLARDE(1.02,16)", "1.125"),
            ("DOLLARFR(1.125,16)", "1.02"),
            ("AMORDEGRC(2400,DATE(2008,8,19),DATE(2008,12,31),300,1,0.15,1)", "776"),
            ("VDB(2400,300,10,0,1)", "480"),
            ("COUPNUM(DATE(2023,9,30),DATE(2023,3,15),2,0)", "#NUM!"),
            ("PRICE(DATE(2023,3,15),DATE(2028,9,30),0.05,0.06,100,3,0)", "#NUM!"),
            ("DISC(DATE(2023,2,15),TRUE,97.5,100)", "#VALUE!"),
        ]);
    }

    #[test]
    fn dates_at_the_end_of_the_calendar_neither_hang_nor_panic() {
        use crate::functions::testing::eval;
        for formula in [
            "ACCRINT(DATE(9999,1,1),DATE(9999,6,30),DATE(9999,12,31),0.1,1000,2,0)",
            "ODDLPRICE(DATE(9999,11,1),DATE(9999,12,31),DATE(9999,6,30),0.05,0.06,100,2,1)",
            "PRICE(DATE(9999,1,1),DATE(9999,12,31),0.05,0.06,100,2,1)",
            "COUPDAYS(DATE(9999,12,30),DATE(9999,12,31),4,1)",
            "ODDFPRICE(DATE(9999,2,1),DATE(9999,12,31),DATE(9999,1,1),DATE(9999,6,30),0.05,0.06,100,2,2)",
            "ACCRINT(DATE(1900,1,1),DATE(1900,3,1),DATE(9999,12,31),0.1,1000,4,1)",
        ] {
            let _ = eval(formula);
        }
    }
}
