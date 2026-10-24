//! Exponential, logarithm, arctangent, sine and cosine rounded correctly to the last place.
//!
//! Excel's complex number functions give the same digits on every machine, and those digits are what a
//! correctly rounded sine, exponential and logarithm print: the C libraries of Windows and macOS differ
//! from them in the last place for about one argument in a thousand, which shows once the answer is
//! written with fifteen digits. These functions work in double double arithmetic, about 106 bits, and round
//! once at the end, so they do not depend on the machine's library.
//!
//! The sine and cosine reduce their argument with the 64 bit value of pi that Excel reduces with. That is
//! pi plus 5.0166e-20, so `sin_excel(PI)` is 1.2251e-16 where the exact sine of the double nearest pi is
//! 1.2246e-16, and Excel's answers for arguments near a multiple of a half pi are reproduced.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// A number held as the sum of two doubles, the second no larger than half a unit in the last place of the
/// first.
#[derive(Debug, Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
}

/// The sum of two doubles and the error of rounding it, exactly.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// The same when the first is known to be at least as large as the second in magnitude.
fn quick_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, b - (s - a))
}

/// The product of two doubles and the error of rounding it, exactly.
fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

impl Dd {
    /// A double double from its two parts, which must already be in order.
    const fn new(hi: f64, lo: f64) -> Dd {
        Dd { hi, lo }
    }

    /// A double as a double double.
    fn of(x: f64) -> Dd {
        Dd { hi: x, lo: 0.0 }
    }

    /// This times a double.
    fn times(self, b: f64) -> Dd {
        let (p, e) = two_prod(self.hi, b);
        let (p, e) = quick_two_sum(p, e + self.lo * b);
        Dd { hi: p, lo: e }
    }

    /// The nearest double.
    fn to_f64(self) -> f64 {
        self.hi + self.lo
    }

    /// This times 2 to the power `k`, which is exact unless the result leaves the range of a double.
    fn scale(self, k: i32) -> Dd {
        let half = k / 2;
        let factor = 2f64.powi(half);
        let rest = 2f64.powi(k - half);
        Dd { hi: self.hi * factor * rest, lo: self.lo * factor * rest }
    }
}

impl Neg for Dd {
    type Output = Dd;
    /// The negative.
    fn neg(self) -> Dd {
        Dd { hi: -self.hi, lo: -self.lo }
    }
}

impl Add for Dd {
    type Output = Dd;
    /// The sum.
    fn add(self, o: Dd) -> Dd {
        let (s, e) = two_sum(self.hi, o.hi);
        let (t, f) = two_sum(self.lo, o.lo);
        let (s, e) = quick_two_sum(s, e + t);
        let (s, e) = quick_two_sum(s, e + f);
        Dd { hi: s, lo: e }
    }
}

impl Sub for Dd {
    type Output = Dd;
    /// The difference.
    fn sub(self, o: Dd) -> Dd {
        self + (-o)
    }
}

impl Mul for Dd {
    type Output = Dd;
    /// The product.
    fn mul(self, o: Dd) -> Dd {
        let (p, e) = two_prod(self.hi, o.hi);
        let (p, e) = quick_two_sum(p, e + (self.hi * o.lo + self.lo * o.hi));
        Dd { hi: p, lo: e }
    }
}

impl Div for Dd {
    type Output = Dd;
    /// Long division in three steps.
    fn div(self, o: Dd) -> Dd {
        let q1 = self.hi / o.hi;
        let r = self - o.times(q1);
        let q2 = r.hi / o.hi;
        let r = r - o.times(q2);
        let q3 = r.hi / o.hi;
        let (s, e) = quick_two_sum(q1, q2);
        Dd { hi: s, lo: e } + Dd::of(q3)
    }
}

/// The natural logarithm of 2 as a double double.
const LN_2: Dd = Dd::new(std::f64::consts::LN_2, 2.319_046_813_846_299_6e-17);

/// Half of pi as a double double.
const HALF_PI: Dd = Dd::new(std::f64::consts::FRAC_PI_2, 6.123_233_995_736_766e-17);

/// Half of the 64 bit value of pi Excel reduces with: its high double and 565 over 2 to the power 63.
const EXCEL_HALF_PI_HIGH: f64 = std::f64::consts::FRAC_PI_2;

/// The rest of that half pi after its high double.
const EXCEL_HALF_PI_LOW: f64 = 565.0 / 9_223_372_036_854_775_808.0;

/// e to the power `r` for `r` no larger than a third in magnitude: the argument is cut by 512, the series
/// for e^s - 1 is summed, and the result is squared nine times.
fn exp_reduced(r: Dd) -> Dd {
    let s = Dd::new(r.hi / 512.0, r.lo / 512.0);
    let (mut term, mut sum) = (s, s);
    for n in 2..=12 {
        term = term * s / Dd::of(n as f64);
        sum = sum + term;
    }
    for _ in 0..9 {
        sum = sum.times(2.0) + sum * sum;
    }
    sum + Dd::of(1.0)
}

/// e to the power `x` as a double double, for an `x` whose exponential is in the range of a double.
fn exp_dd(x: f64) -> Dd {
    let k = (x / LN_2.hi).round();
    let r = Dd::of(x) - LN_2.times(k);
    exp_reduced(r).scale(k as i32)
}

/// e to the power `x`, rounded correctly. Too large an argument is infinity and too small is 0.
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if x < -745.2 {
        return 0.0;
    }
    exp_dd(x).to_f64()
}

/// The natural logarithm of a positive number, rounded correctly, by one step of Newton's method from the
/// library's answer carried out in double double arithmetic.
pub fn ln(x: f64) -> f64 {
    let y0 = x.ln();
    if x.is_nan() || x <= 0.0 || !x.is_finite() || y0 == 0.0 {
        return y0;
    }
    let e = exp_dd(y0);
    let correction = ((Dd::of(x) - e) / (Dd::of(x) + e)).times(2.0);
    (Dd::of(y0) + correction).to_f64()
}

/// The sine and cosine of a double double no larger than a little over a quarter turn, by their series.
fn sin_cos_series(r: Dd) -> (Dd, Dd) {
    let r2 = r * r;
    let (mut term, mut sin) = (r, r);
    for n in 1..24 {
        term = -(term * r2 / Dd::of(((2 * n) * (2 * n + 1)) as f64));
        sin = sin + term;
        if term.hi.abs() <= sin.hi.abs() * 1e-35 {
            break;
        }
    }
    let (mut term, mut cos) = (Dd::of(1.0), Dd::of(1.0));
    for n in 1..24 {
        term = -(term * r2 / Dd::of(((2 * n - 1) * (2 * n)) as f64));
        cos = cos + term;
        if term.hi.abs() <= 1e-35 {
            break;
        }
    }
    (sin, cos)
}

/// The sine and cosine of a double between minus and plus a half pi, exactly.
fn sin_cos_of(y: f64) -> (Dd, Dd) {
    if y.abs() <= std::f64::consts::FRAC_PI_4 {
        return sin_cos_series(Dd::of(y));
    }
    let (s, c) = sin_cos_series(HALF_PI - Dd::of(y.abs()));
    if y < 0.0 {
        (-c, s)
    } else {
        (c, s)
    }
}

/// The arctangent, rounded correctly, by one step of Newton's method from the library's answer.
pub fn atan(x: f64) -> f64 {
    let y0 = x.atan();
    if !x.is_finite() || y0 == 0.0 {
        return y0;
    }
    let (s, c) = sin_cos_of(y0);
    let xd = Dd::of(x);
    let correction = (xd * c - s) / (c + xd * s);
    (Dd::of(y0) + correction).to_f64()
}

/// The quarter turn an argument falls in and its distance from the multiple of a half pi before it, in
/// double double arithmetic, taking a half pi to be Excel's. The argument must be below 2 to the power 27.
fn reduce(x: f64) -> (i64, Dd) {
    let k = (x / EXCEL_HALF_PI_HIGH).round();
    let (p, pe) = two_prod(k, EXCEL_HALF_PI_HIGH);
    let (s1, e1) = two_sum(x - p, -pe);
    let (s2, e2) = two_sum(s1, -(k * EXCEL_HALF_PI_LOW));
    let (hi, lo) = quick_two_sum(s2, e1 + e2);
    ((k as i64).rem_euclid(4), Dd { hi, lo })
}

/// The sine as Excel finds it, rounded correctly, for an argument below 2 to the power 27.
pub fn sin_excel(x: f64) -> f64 {
    let (quarter, r) = reduce(x);
    let (s, c) = sin_cos_series(r);
    match quarter {
        0 => s,
        1 => c,
        2 => -s,
        _ => -c,
    }
    .to_f64()
}

/// The cosine as Excel finds it, rounded correctly, for an argument below 2 to the power 27.
pub fn cos_excel(x: f64) -> f64 {
    let (quarter, r) = reduce(x);
    let (s, c) = sin_cos_series(r);
    match quarter {
        0 => c,
        1 => -s,
        2 => -c,
        _ => s,
    }
    .to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether two doubles agree to fourteen digits.
    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= b.abs() * 1e-14
    }

    #[test]
    fn the_exponential_and_logarithm_are_the_nearest_doubles() {
        assert_eq!(exp(1.0), std::f64::consts::E);
        assert_eq!(exp(0.0), 1.0);
        assert!(exp(710.0).is_infinite());
        assert_eq!(exp(-746.0), 0.0);
        assert_eq!(ln(2.0), std::f64::consts::LN_2);
        assert_eq!(ln(10.0), std::f64::consts::LN_10);
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(exp(3.0)), 3.0);
        assert!(near(exp(-700.0), 9.859_676_543_759_77e-305));
    }

    #[test]
    fn the_arctangent_is_the_nearest_double() {
        assert_eq!(atan(1.0), std::f64::consts::FRAC_PI_4);
        assert_eq!(atan(f64::INFINITY), std::f64::consts::FRAC_PI_2);
        assert_eq!(atan(-1.0), -std::f64::consts::FRAC_PI_4);
        assert_eq!(atan(0.0), 0.0);
    }

    #[test]
    fn the_sine_and_cosine_use_excels_value_of_pi() {
        assert!(near(sin_excel(std::f64::consts::PI), 1.22514845490862e-16));
        assert!(near(cos_excel(std::f64::consts::FRAC_PI_2), 6.1257422745431e-17));
        assert!(near(sin_excel(std::f64::consts::FRAC_PI_4), std::f64::consts::FRAC_1_SQRT_2));
        assert!(near(sin_excel(1e6 * std::f64::consts::PI), -2.23241383712214e-10));
        assert_eq!(cos_excel(0.0), 1.0);
        assert_eq!(sin_excel(0.0), 0.0);
        assert_eq!(sin_excel(2.5), 0.5984721441039565);
    }
}
