//! Complex numbers and Bessel functions: `COMPLEX`, the `IM` functions and `BESSELI`, `BESSELJ`, `BESSELK`
//! and `BESSELY`.
//!
//! Every behaviour here was measured against Excel 16 rather than taken from the documentation, because
//! these thirty functions are the old Analysis ToolPak code and differ from the rest of the library:
//!
//! * A complex number is text such as `3+4i`. A number argument is a complex number with no imaginary part,
//!   an empty cell is 0, a logical is `#VALUE!` and text that is not a complex number is `#NUM!`. The text is
//!   read with the rules typing a number uses, so `1,000`, `5%` and `(5)` work and only fifteen digits are kept.
//! * The suffix is `i` or `j` in lower case and a result keeps the suffix its inputs used. Mixing the two is
//!   `#VALUE!` for `IMSUM` and `IMPRODUCT` and `#NUM!` for `IMSUB` and `IMDIV`.
//! * An argument that is a range of several cells is `#VALUE!` and an array supplies its first element,
//!   except for `IMSUM` and `IMPRODUCT`, which read every value in order and stop at the first fault.
//! * A result is written with fifteen significant digits and no exponent until it would need more than
//!   nineteen decimals or twenty one integer digits. Anything smaller than the smallest normal number is 0.
//! * Sine and cosine reduce their argument with a value of pi that carries 64 bits of mantissa, so
//!   `IMSIN(PI())` is 1.22514845490862E-16 and not the 1.2246E-16 of the exact sine, and an argument of
//!   134217728 or more is `#NUM!`. `precise.rs` has them, with the exponential, logarithm and arctangent,
//!   rounded correctly so the last digit of an answer is the same on every machine.
//! * The angle of a number is the arctangent of the ratio of its parts with pi added or taken away for a
//!   negative real part, its modulus for the logarithm and root functions is the square root of the sum of
//!   the squares of its parts, and `IMABS` scales by the larger part. Each is the way Excel finds them, and
//!   each differs from the textbook formula in the last digit for some numbers.
//! * `IMSIN` and `IMCOS` build their hyperbolic functions from `e` to the power and its reciprocal, the
//!   tangent and cotangent use the formulas in sines and cosines of twice the parts, and the secant, cosecant
//!   and their hyperbolic forms are found from squares. Where Excel's own rounding of these differs for a
//!   few in a hundred arguments, the formula here is the closest of those tried.
//! * The Bessel functions are the polynomial approximations from Numerical Recipes with the typing errors
//!   Excel's copy of them has, so they agree with Excel to the last digit and with the true functions to
//!   about eight digits. The first kind of order 1 for an argument of 8 or more repeats one coefficient,
//!   and the first kind of order 0 has two digits of one coefficient swapped.

use super::precise;
use super::{r, Function};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{significant_digits, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("COMPLEX", 2, 3, "real_num, i_num, [suffix]", "A complex number from its parts.", complex),
    r("IMABS", 1, 1, "inumber", "The absolute value of a complex number.", |c, a| {
        number_of(c, a, modulus)
    }),
    r("IMARGUMENT", 1, 1, "inumber", "The angle of a complex number in radians.", |c, a| {
        number_of(c, a, argument)
    }),
    r("IMREAL", 1, 1, "inumber", "The real part of a complex number.", |c, a| {
        number_of(c, a, |z| Ok(z.0))
    }),
    r("IMAGINARY", 1, 1, "inumber", "The imaginary part of a complex number.", |c, a| {
        number_of(c, a, |z| Ok(z.1))
    }),
    r("IMCONJUGATE", 1, 1, "inumber", "The conjugate of a complex number.", |c, a| {
        text_of(c, a, |z| Ok((z.0, -z.1)))
    }),
    r("IMSIN", 1, 1, "inumber", "The sine of a complex number.", |c, a| text_of(c, a, sin)),
    r("IMCOS", 1, 1, "inumber", "The cosine of a complex number.", |c, a| text_of(c, a, cos)),
    r("IMTAN", 1, 1, "inumber", "The tangent of a complex number.", |c, a| text_of(c, a, tan)),
    r("IMCOT", 1, 1, "inumber", "The cotangent of a complex number.", |c, a| text_of(c, a, cot)),
    r("IMSEC", 1, 1, "inumber", "The secant of a complex number.", |c, a| text_of(c, a, sec)),
    r("IMCSC", 1, 1, "inumber", "The cosecant of a complex number.", |c, a| text_of(c, a, csc)),
    r("IMSINH", 1, 1, "inumber", "The hyperbolic sine of a complex number.", |c, a| {
        text_of(c, a, sinh)
    }),
    r("IMCOSH", 1, 1, "inumber", "The hyperbolic cosine of a complex number.", |c, a| {
        text_of(c, a, cosh)
    }),
    r("IMSECH", 1, 1, "inumber", "The hyperbolic secant of a complex number.", |c, a| {
        text_of(c, a, sech)
    }),
    r("IMCSCH", 1, 1, "inumber", "The hyperbolic cosecant of a complex number.", |c, a| {
        text_of(c, a, csch)
    }),
    r("IMEXP", 1, 1, "inumber", "The exponential of a complex number.", |c, a| text_of(c, a, exp)),
    r("IMLN", 1, 1, "inumber", "The natural logarithm of a complex number.", |c, a| {
        text_of(c, a, |z| ln_scaled(z, 1.0))
    }),
    r("IMLOG10", 1, 1, "inumber", "The base 10 logarithm of a complex number.", |c, a| {
        text_of(c, a, |z| ln_scaled(z, LOG10_E))
    }),
    r("IMLOG2", 1, 1, "inumber", "The base 2 logarithm of a complex number.", |c, a| {
        text_of(c, a, |z| ln_scaled(z, LOG2_E))
    }),
    r("IMSQRT", 1, 1, "inumber", "The square root of a complex number.", |c, a| {
        text_of(c, a, sqrt)
    }),
    r("IMPOWER", 2, 2, "inumber, number", "A complex number raised to a power.", power),
    r("IMDIV", 2, 2, "inumber1, inumber2", "The quotient of two complex numbers.", |c, a| {
        text_of_two(c, a, divide)
    }),
    r("IMSUB", 2, 2, "inumber1, inumber2", "The difference of two complex numbers.", |c, a| {
        text_of_two(c, a, |x, y| Ok((x.0 - y.0, x.1 - y.1)))
    }),
    r("IMSUM", 1, 255, "inumber1, [inumber2], ...", "The sum of complex numbers.", |c, a| {
        accumulate(c, a, |x, y| (x.0 + y.0, x.1 + y.1), ErrorKind::Num)
    }),
    r(
        "IMPRODUCT",
        1,
        255,
        "inumber1, [inumber2], ...",
        "The product of complex numbers.",
        |c, a| accumulate(c, a, times, ErrorKind::Value),
    ),
    r("BESSELJ", 2, 2, "x, n", "The Bessel function of the first kind.", |c, a| {
        bessel(c, a, bessj)
    }),
    r("BESSELY", 2, 2, "x, n", "The Bessel function of the second kind.", |c, a| {
        bessel(c, a, bessy)
    }),
    r("BESSELI", 2, 2, "x, n", "The modified Bessel function of the first kind.", |c, a| {
        bessel(c, a, bessi)
    }),
    r("BESSELK", 2, 2, "x, n", "The modified Bessel function of the second kind.", |c, a| {
        bessel(c, a, bessk)
    }),
];

/// The factors that turn a natural logarithm into a base 10 and a base 2 logarithm. Excel's second is one
/// unit in the last place above the true value of the logarithm of e to base 2.
const LOG10_E: f64 = std::f64::consts::LOG10_E;
const LOG2_E: f64 = 1.4426950408889636;

/// A complex number as its real and imaginary parts.
type Pair = (f64, f64);

/// A complex number read from a worksheet value, with the unit letter the text used, if it used one.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Complex {
    re: f64,
    im: f64,
    unit: Option<char>,
}

/// The value an argument supplies: a single cell, or the first element of an array. A range of several
/// cells outside the formula's own row or column is `#VALUE!`, and a left out argument is `#N/A`.
fn scalar(ctx: &mut Ctx, expr: &Expr) -> Result<Value, ErrorKind> {
    if matches!(expr, Expr::Missing) {
        return Err(ErrorKind::NA);
    }
    let val = ctx.eval(expr);
    match ctx.single(val) {
        Value::Error(e) => Err(e),
        other => Ok(other),
    }
}

/// The text a function answers with, or the error it ends in.
fn text_val(result: Result<String, ErrorKind>) -> Val {
    Val::Value(result.map_or_else(Value::Error, Value::text))
}

/// The number a function answers with, or the error it ends in.
fn number_val(result: Result<f64, ErrorKind>) -> Val {
    Val::Value(match result {
        Ok(n) if n.is_finite() => Value::Number(if n == 0.0 { 0.0 } else { n }),
        Ok(_) => Value::Error(ErrorKind::Num),
        Err(e) => Value::Error(e),
    })
}

/// A function of one complex number that answers with a complex number as text.
fn text_of(ctx: &mut Ctx, args: &[Expr], f: fn(Pair) -> Result<Pair, ErrorKind>) -> Val {
    text_val(complex_arg(ctx, &args[0]).and_then(|z| show(f((z.re, z.im))?, z.unit)))
}

/// A function of one complex number that answers with a number.
fn number_of(ctx: &mut Ctx, args: &[Expr], f: fn(Pair) -> Result<f64, ErrorKind>) -> Val {
    number_val(complex_arg(ctx, &args[0]).and_then(|z| f((z.re, z.im))))
}

/// A function of two complex numbers, which must use the same unit letter or be `#NUM!`.
fn text_of_two(ctx: &mut Ctx, args: &[Expr], f: fn(Pair, Pair) -> Result<Pair, ErrorKind>) -> Val {
    let result = complex_arg(ctx, &args[0]).and_then(|x| {
        let y = complex_arg(ctx, &args[1])?;
        let unit = merge_units(x.unit, y.unit).ok_or(ErrorKind::Num)?;
        show(f((x.re, x.im), (y.re, y.im))?, unit)
    });
    text_val(result)
}

/// The complex number an argument holds.
fn complex_arg(ctx: &mut Ctx, expr: &Expr) -> Result<Complex, ErrorKind> {
    let value = scalar(ctx, expr)?;
    to_complex(&value, ctx.date1904())
}

/// The unit two operands share: either one's letter when the other has none, and `None` for a clash.
fn merge_units(a: Option<char>, b: Option<char>) -> Option<Option<char>> {
    match (a, b) {
        (Some(x), Some(y)) if x != y => None,
        (Some(x), _) | (_, Some(x)) => Some(Some(x)),
        _ => Some(None),
    }
}

// ---------------------------------------------------------------------------------------------------
// Reading a complex number.

/// A worksheet value as a complex number.
fn to_complex(value: &Value, date1904: bool) -> Result<Complex, ErrorKind> {
    match value {
        Value::Empty => Ok(Complex { re: 0.0, im: 0.0, unit: None }),
        Value::Number(n) => Ok(Complex { re: *n, im: 0.0, unit: None }),
        Value::Bool(_) => Err(ErrorKind::Value),
        Value::Error(e) => Err(*e),
        Value::Text(text) => parse_complex(text, date1904),
    }
}

/// The largest magnitude Excel reads from text as a number.
const LARGEST_TYPED: f64 = 9.99999999999999e307;

/// One number inside a complex number, read the way typing reads it. Spaces round it are ignored.
fn number_part(text: &str, date1904: bool) -> Option<f64> {
    let n = crate::input::number_from_text(text, date1904)?;
    if !n.is_finite() || n.abs() > LARGEST_TYPED {
        return None;
    }
    if n != 0.0 && n.abs() < f64::MIN_POSITIVE {
        return (n.abs() >= 1e-309).then_some(0.0);
    }
    if n == 0.0 && underflowed(text) {
        return None;
    }
    Some(if n == 0.0 { 0.0 } else { n })
}

/// Whether a number written as text has a mantissa that is not zero, so that reading it as 0 means its
/// exponent was too small.
fn underflowed(text: &str) -> bool {
    let mantissa = text.split(['e', 'E']).next().unwrap_or("");
    mantissa.chars().any(|c| ('1'..='9').contains(&c))
}

/// Text such as `3+4i`, `-i`, `2j` or `1,000` as a complex number. Anything else is `#NUM!`.
fn parse_complex(text: &str, date1904: bool) -> Result<Complex, ErrorKind> {
    let s = text.trim();
    if let Some(re) = number_part(s, date1904) {
        return Ok(Complex { re, im: 0.0, unit: None });
    }
    let unit = match s.chars().last() {
        Some(c @ ('i' | 'j')) => c,
        _ => return Err(ErrorKind::Num),
    };
    let body = &s[..s.len() - 1];
    let (re, im) = split_parts(body, date1904).ok_or(ErrorKind::Num)?;
    Ok(Complex { re, im, unit: Some(unit) })
}

/// The real and imaginary parts of what is left of a complex number once its unit letter is taken off.
fn split_parts(body: &str, date1904: bool) -> Option<Pair> {
    for at in sign_positions(body) {
        let Some(re) = number_part(&body[..at], date1904) else { continue };
        let rest = &body[at + 1..];
        let magnitude = if rest.is_empty() { Some(1.0) } else { number_part(rest, date1904) };
        if let Some(m) = magnitude {
            let im = if body.as_bytes()[at] == b'-' { -m } else { m };
            return Some((normal_zero(re), normal_zero(im)));
        }
    }
    let im = match body {
        "" | "+" => 1.0,
        "-" => -1.0,
        other => number_part(other, date1904)?,
    };
    Some((0.0, normal_zero(im)))
}

/// A zero with its sign taken off, so a part written `-0` is not a negative zero.
fn normal_zero(n: f64) -> f64 {
    if n == 0.0 {
        0.0
    } else {
        n
    }
}

/// Where a `+` or `-` could begin the imaginary part: not first, and not the sign of an exponent.
fn sign_positions(body: &str) -> Vec<usize> {
    let bytes = body.as_bytes();
    (1..bytes.len())
        .filter(|&at| matches!(bytes[at], b'+' | b'-'))
        .filter(|&at| !is_exponent_sign(bytes, at))
        .collect()
}

/// Whether the sign at `at` follows the `e` of a number such as `1e+3`.
fn is_exponent_sign(bytes: &[u8], at: usize) -> bool {
    at >= 2
        && matches!(bytes[at - 1], b'e' | b'E')
        && (bytes[at - 2].is_ascii_digit() || bytes[at - 2] == b'.')
}

// ---------------------------------------------------------------------------------------------------
// Writing a complex number.

/// A complex number as Excel writes it, or `#NUM!` when a part is not finite.
fn show(z: Pair, unit: Option<char>) -> Result<String, ErrorKind> {
    let (re, im) = (flush(z.0), flush(z.1));
    if !re.is_finite() || !im.is_finite() {
        return Err(ErrorKind::Num);
    }
    let unit = unit.unwrap_or('i');
    let re_text = digits_text(re);
    let im_text = match digits_text(im).as_str() {
        "0" => return Ok(re_text),
        "1" => unit.to_string(),
        "-1" => format!("-{unit}"),
        other => format!("{other}{unit}"),
    };
    Ok(match (re_text.as_str(), im_text.starts_with('-')) {
        ("0", _) => im_text,
        (_, true) => format!("{re_text}{im_text}"),
        _ => format!("{re_text}+{im_text}"),
    })
}

/// Anything below the smallest normal number is 0, which is what Excel's arithmetic does here.
fn flush(x: f64) -> f64 {
    if x.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        x
    }
}

/// A number with fifteen significant digits. It is written out in full up to twenty one digits before the
/// point and nineteen after it, and in scientific notation beyond that.
fn digits_text(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned();
    }
    let (digits, exponent) = significant_digits(x.abs(), 15);
    let decimals = digits.len() as i32 - 1 - exponent;
    let body = if exponent > 20 || decimals > 19 {
        scientific(&digits, exponent)
    } else {
        positional(&digits, exponent)
    };
    if x < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// Digits and a power of ten as `1.5E-19`, the exponent with at least two figures.
fn scientific(digits: &str, exponent: i32) -> String {
    let mantissa = match digits.len() {
        1 => digits.to_owned(),
        _ => format!("{}.{}", &digits[..1], &digits[1..]),
    };
    format!("{mantissa}E{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// Digits and a power of ten written out with no exponent.
fn positional(digits: &str, exponent: i32) -> String {
    if exponent < 0 {
        return format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize));
    }
    let whole = exponent as usize + 1;
    if digits.len() <= whole {
        format!("{digits}{}", "0".repeat(whole - digits.len()))
    } else {
        format!("{}.{}", &digits[..whole], &digits[whole..])
    }
}

// ---------------------------------------------------------------------------------------------------
// COMPLEX, IMSUM, IMPRODUCT and IMPOWER, which read their arguments in their own ways.

/// `COMPLEX(real_num, i_num, [suffix])`.
fn complex(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let result = (|| {
        let re = number_arg(ctx, &args[0])?;
        let im = number_arg(ctx, &args[1])?;
        let unit = match args.get(2) {
            None | Some(Expr::Missing) => 'i',
            Some(expr) => suffix_of(&scalar(ctx, expr)?)?,
        };
        show((re, im), Some(unit))
    })();
    text_val(result)
}

/// The suffix argument of `COMPLEX`: `i`, `j`, or nothing, which means `i`.
fn suffix_of(value: &Value) -> Result<char, ErrorKind> {
    match value {
        Value::Empty => Ok('i'),
        Value::Text(t) => match &**t {
            "" | "i" => Ok('i'),
            "j" => Ok('j'),
            _ => Err(ErrorKind::Value),
        },
        _ => Err(ErrorKind::Value),
    }
}

/// An argument that must be a number: text is read as typing reads it, a logical is `#VALUE!`.
fn number_arg(ctx: &mut Ctx, expr: &Expr) -> Result<f64, ErrorKind> {
    let value = scalar(ctx, expr)?;
    number_value(&value, ctx.date1904())
}

/// A value as the number a plain numeric argument means.
fn number_value(value: &Value, date1904: bool) -> Result<f64, ErrorKind> {
    match value {
        Value::Empty => Ok(0.0),
        Value::Number(n) => Ok(*n),
        Value::Bool(_) => Err(ErrorKind::Value),
        Value::Error(e) => Err(*e),
        Value::Text(t) => crate::input::number_from_text(t, date1904).ok_or(ErrorKind::Value),
    }
}

/// `IMPOWER(inumber, number)`.
fn power(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let result = complex_arg(ctx, &args[0]).and_then(|z| {
        let n = number_arg(ctx, &args[1])?;
        show(raise((z.re, z.im), n)?, z.unit)
    });
    text_val(result)
}

/// `IMSUM` and `IMPRODUCT`: every value of every argument in order, the first fault ending the work. A
/// left out argument is skipped unless it is the first. `overflow` is the error a result too large for a
/// number is, which differs between the two.
fn accumulate(
    ctx: &mut Ctx,
    args: &[Expr],
    op: fn(Pair, Pair) -> Pair,
    overflow: ErrorKind,
) -> Val {
    let result = (|| {
        let date1904 = ctx.date1904();
        let mut total: Option<Pair> = None;
        let mut unit = None;
        for (index, expr) in args.iter().enumerate() {
            if matches!(expr, Expr::Missing) {
                if index == 0 {
                    return Err(ErrorKind::NA);
                }
                continue;
            }
            for value in values_of(ctx, expr)? {
                let z = to_complex(&value, date1904)?;
                unit = merge_units(unit, z.unit).ok_or(ErrorKind::Value)?;
                let next = (z.re, z.im);
                total = Some(total.map_or(next, |t| op(t, next)));
            }
        }
        let total = total.unwrap_or((0.0, 0.0));
        if !total.0.is_finite() || !total.1.is_finite() {
            return Err(overflow);
        }
        show(total, unit)
    })();
    text_val(result)
}

/// Every value an argument holds, a range and an array row by row, empty cells included.
fn values_of(ctx: &mut Ctx, expr: &Expr) -> Result<Vec<Value>, ErrorKind> {
    match ctx.eval(expr) {
        Val::Value(v) => Ok(vec![v]),
        Val::Refs(areas) => {
            let mut out = Vec::new();
            for area in areas {
                out.extend(ctx.array(Val::Ref(area))?.data);
            }
            Ok(out)
        }
        other => Ok(ctx.array(other)?.data),
    }
}

// ---------------------------------------------------------------------------------------------------
// Arithmetic.

/// The product of two complex numbers.
fn times(x: Pair, y: Pair) -> Pair {
    (x.0 * y.0 - x.1 * y.1, x.0 * y.1 + x.1 * y.0)
}

/// The quotient of two complex numbers by Smith's method, which keeps an intermediate value from
/// overflowing when the parts are large or small. A divisor of zero is `#NUM!`.
fn divide(n: Pair, d: Pair) -> Result<Pair, ErrorKind> {
    let ((a, b), (c, e)) = (n, d);
    if c == 0.0 && e == 0.0 {
        return Err(ErrorKind::Num);
    }
    let quotient = if c.abs() >= e.abs() {
        let r = e / c;
        let den = c + e * r;
        ((a + b * r) / den, (b - a * r) / den)
    } else {
        let r = c / e;
        let den = c * r + e;
        ((a * r + b) / den, (b * r - a) / den)
    };
    finite(quotient)
}

/// A pair, or `#NUM!` when a part is infinite or not a number.
fn finite(z: Pair) -> Result<Pair, ErrorKind> {
    if z.0.is_finite() && z.1.is_finite() {
        Ok(z)
    } else {
        Err(ErrorKind::Num)
    }
}

/// `IMABS`: the larger part times the square root of one plus the square of the ratio of the two, which is
/// how Excel finds it and does not overflow for large parts.
fn modulus(z: Pair) -> Result<f64, ErrorKind> {
    let (a, b) = (z.0.abs(), z.1.abs());
    let (larger, smaller) = (a.max(b), a.min(b));
    if larger == 0.0 {
        return Ok(0.0);
    }
    let ratio = smaller / larger;
    Ok(larger * (1.0 + ratio * ratio).sqrt())
}

/// The angle of a complex number from the arctangent of the ratio of its parts, with pi added or taken away
/// when the real part is negative. Excel finds it this way, which differs from the two argument arctangent
/// in the last digit for half the numbers with a negative real part.
fn angle(z: Pair) -> f64 {
    let (a, b) = z;
    if a > 0.0 {
        precise::atan(b / a)
    } else if a < 0.0 {
        precise::atan(b / a) + if b >= 0.0 { std::f64::consts::PI } else { -std::f64::consts::PI }
    } else if b == 0.0 {
        0.0
    } else {
        std::f64::consts::FRAC_PI_2.copysign(b)
    }
}

/// `IMARGUMENT`: the angle, and `#DIV/0!` for zero, which has none.
fn argument(z: Pair) -> Result<f64, ErrorKind> {
    if z.0 == 0.0 && z.1 == 0.0 {
        return Err(ErrorKind::Div0);
    }
    Ok(angle(z))
}

/// The modulus as the logarithm and power functions find it. Squaring the parts overflows for large ones
/// and underflows for small ones, so those answer `inf` and 0, but a number with only a real or only an
/// imaginary part is its own modulus.
fn modulus_by_squares(z: Pair) -> f64 {
    let (a, b) = z;
    if b == 0.0 {
        a.abs()
    } else if a == 0.0 {
        b.abs()
    } else {
        flush(a * a + b * b).sqrt()
    }
}

/// `IMLN`, and the other logarithms through `scale`, the base's conversion factor.
fn ln_scaled(z: Pair, scale: f64) -> Result<Pair, ErrorKind> {
    let r = modulus_by_squares(z);
    if r == 0.0 || !r.is_finite() {
        return Err(ErrorKind::Num);
    }
    Ok((precise::ln(r) * scale, angle(z) * scale))
}

/// `IMSQRT`: the square root of the modulus at half the angle, the modulus found by squaring the parts
/// even when one is zero. When both squares are finite and only their sum is too large Excel answers 0,
/// and when a square is too large it is `#NUM!`.
fn sqrt(z: Pair) -> Result<Pair, ErrorKind> {
    let (a2, b2) = (z.0 * z.0, z.1 * z.1);
    let r = flush(a2 + b2).sqrt();
    if r == 0.0 {
        return Ok((0.0, 0.0));
    }
    if !r.is_finite() {
        return if a2.is_finite() && b2.is_finite() { Ok((0.0, 0.0)) } else { Err(ErrorKind::Num) };
    }
    let (root, half) = (r.sqrt(), angle(z) / 2.0);
    Ok((root * trig_cos(half)?, root * trig_sin(half)?))
}

/// A positive number to a power the way Excel finds it: by repeated squaring for a whole number, with a
/// negative one as the reciprocal, the square root for one half, and otherwise as e to the power of the
/// exponent times the logarithm.
fn real_power(r: f64, n: f64) -> f64 {
    if n == 0.5 {
        return r.sqrt();
    }
    if n.fract() != 0.0 || n.abs() >= 1e15 {
        return precise::exp(n * precise::ln(r));
    }
    let (mut m, mut base, mut result) = (n.abs() as u64, r, 1.0);
    while m > 0 {
        if m & 1 == 1 {
            result *= base;
        }
        base *= base;
        m >>= 1;
    }
    if n < 0.0 {
        1.0 / result
    } else {
        result
    }
}

/// `IMPOWER` with a real exponent: the modulus to the power at the angle times the power.
fn raise(z: Pair, n: f64) -> Result<Pair, ErrorKind> {
    let r = modulus_by_squares(z);
    if r == 0.0 {
        return if n > 0.0 { Ok((0.0, 0.0)) } else { Err(ErrorKind::Num) };
    }
    let magnitude = real_power(r, n);
    if !magnitude.is_finite() {
        return Err(ErrorKind::Num);
    }
    let theta = angle(z) * n;
    finite((magnitude * trig_cos(theta)?, magnitude * trig_sin(theta)?))
}

// ---------------------------------------------------------------------------------------------------
// Trigonometry.

/// The argument size at which Excel's sine and cosine give up: 2 to the power 27.
const TRIG_LIMIT: f64 = 134_217_728.0;

/// Excel's sine, `#NUM!` for an argument too large to reduce.
fn trig_sin(x: f64) -> Result<f64, ErrorKind> {
    if x.abs() >= TRIG_LIMIT {
        return Err(ErrorKind::Num);
    }
    Ok(precise::sin_excel(x))
}

/// Excel's cosine, `#NUM!` for an argument too large to reduce.
fn trig_cos(x: f64) -> Result<f64, ErrorKind> {
    if x.abs() >= TRIG_LIMIT {
        return Err(ErrorKind::Num);
    }
    Ok(precise::cos_excel(x))
}

/// The largest exponent the sine and cosine of a complex number accept for the imaginary part.
const EXP_LIMIT: f64 = 709.72;

/// Beyond this the squares of the hyperbolic functions overflow, and the reciprocal functions are found
/// from the exponentials alone.
const SQUARE_LIMIT: f64 = 350.0;

/// The exponential, with a result too large for a number held at the largest one as Excel does inside the
/// reciprocal functions.
fn clamped_exp(x: f64) -> f64 {
    let e = precise::exp(x);
    if e.is_infinite() {
        f64::MAX
    } else {
        e
    }
}

/// Below this a hyperbolic function is summed from its series, where the difference or sum of two
/// exponentials would lose the digits.
const SERIES_BELOW: f64 = 0.01;

/// The hyperbolic sine and cosine of a small number from their series.
fn hyperbolic_series(x: f64) -> (f64, f64) {
    let (mut sinh, mut sinh_term) = (x, x);
    let (mut cosh, mut cosh_term) = (1.0, 1.0);
    for k in 1..6 {
        cosh_term *= x * x / ((2 * k - 1) as f64 * (2 * k) as f64);
        cosh += cosh_term;
        sinh_term *= x * x / ((2 * k) as f64 * (2 * k + 1) as f64);
        sinh += sinh_term;
    }
    (sinh, cosh)
}

/// The hyperbolic sine and cosine as `IMSINH` and `IMCOSH` find them: from the series for a small
/// argument, and otherwise from `e` to the power and to its negative.
fn hyperbolics(x: f64) -> (f64, f64) {
    if x.abs() < SERIES_BELOW {
        return hyperbolic_series(x);
    }
    let (e, em) = (precise::exp(x), precise::exp(-x));
    ((e - em) / 2.0, (e + em) / 2.0)
}

/// The same as `IMTAN` and `IMCOT` find them, with the negative power found as the reciprocal.
fn hyperbolics_by_reciprocal(x: f64) -> (f64, f64) {
    if x.abs() < SERIES_BELOW {
        return hyperbolic_series(x);
    }
    let e = precise::exp(x);
    ((e - 1.0 / e) / 2.0, (e + 1.0 / e) / 2.0)
}

/// The hyperbolic sine.
fn sinh_of(x: f64) -> f64 {
    hyperbolics(x).0
}

/// The hyperbolic cosine.
fn cosh_of(x: f64) -> f64 {
    hyperbolics(x).1
}

/// `e` to the power `b` and its reciprocal, the pair the sine and cosine of a complex number are built from.
fn exp_pair(b: f64) -> Result<(f64, f64), ErrorKind> {
    if b.abs() > EXP_LIMIT {
        return Err(ErrorKind::Num);
    }
    let e = precise::exp(b);
    Ok((e, 1.0 / e))
}

/// `IMSIN`: sin a cosh b + i cos a sinh b, with the hyperbolic functions made from exponentials.
fn sin(z: Pair) -> Result<Pair, ErrorKind> {
    let (eb, emb) = exp_pair(z.1)?;
    finite((trig_sin(z.0)? * (eb + emb) / 2.0, trig_cos(z.0)? * (eb - emb) / 2.0))
}

/// `IMCOS`: cos a cosh b - i sin a sinh b.
fn cos(z: Pair) -> Result<Pair, ErrorKind> {
    let (eb, emb) = exp_pair(z.1)?;
    finite((trig_cos(z.0)? * (eb + emb) / 2.0, -(trig_sin(z.0)? * (eb - emb) / 2.0)))
}

/// `IMSINH`: sinh a cos b + i cosh a sin b.
fn sinh(z: Pair) -> Result<Pair, ErrorKind> {
    finite((sinh_of(z.0) * trig_cos(z.1)?, cosh_of(z.0) * trig_sin(z.1)?))
}

/// `IMCOSH`: cosh a cos b + i sinh a sin b.
fn cosh(z: Pair) -> Result<Pair, ErrorKind> {
    finite((cosh_of(z.0) * trig_cos(z.1)?, sinh_of(z.0) * trig_sin(z.1)?))
}

/// `IMEXP`: e to the real part times cos b + i sin b. With an imaginary part a result too large is held at
/// the largest number, as Excel does; a real argument that is too large is `#NUM!`.
fn exp(z: Pair) -> Result<Pair, ErrorKind> {
    if z.1 == 0.0 {
        if z.0 > EXP_LIMIT {
            return Err(ErrorKind::Num);
        }
        return Ok((precise::exp(z.0), 0.0));
    }
    let e = clamped_exp(z.0);
    finite((e * trig_cos(z.1)?, e * trig_sin(z.1)?))
}

/// The hyperbolic sine and cosine of twice an imaginary part, or `None` once `e` to that power is 0 or
/// infinite and the answers of `IMTAN` and `IMCOT` have become plus or minus i.
fn double_hyperbolics(b: f64) -> Option<(f64, f64)> {
    let e2 = precise::exp(2.0 * b);
    if e2 == 0.0 || e2.is_infinite() {
        return None;
    }
    Some(hyperbolics_by_reciprocal(2.0 * b))
}

/// `IMTAN`: sin a over cos a for a real number, otherwise (sin 2a + i sinh 2b) over (cos 2a + cosh 2b),
/// which is plus or minus i once cosh 2b overflows.
fn tan(z: Pair) -> Result<Pair, ErrorKind> {
    let (s, c) = (trig_sin(z.0)?, trig_cos(z.0)?);
    if z.1 == 0.0 {
        return if c == 0.0 { Err(ErrorKind::Num) } else { Ok((s / c, 0.0)) };
    }
    let Some((sh2, ch2)) = double_hyperbolics(z.1) else { return Ok((0.0, z.1.signum())) };
    let den = (1.0 - 2.0 * s * s) + ch2;
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((2.0 * s * c / den, sh2 / den))
}

/// `IMCOT`: cos a over sin a for a real number, otherwise (sin 2a - i sinh 2b) over (cosh 2b - cos 2a).
/// A real argument too large for the sine is 0, which is what Excel answers.
fn cot(z: Pair) -> Result<Pair, ErrorKind> {
    if z.1 == 0.0 && z.0.abs() >= TRIG_LIMIT {
        return Ok((0.0, 0.0));
    }
    let (s, c) = (trig_sin(z.0)?, trig_cos(z.0)?);
    if z.1 == 0.0 {
        return if s == 0.0 { Err(ErrorKind::Num) } else { Ok((c / s, 0.0)) };
    }
    let Some((sh2, ch2)) = double_hyperbolics(z.1) else { return Ok((0.0, -z.1.signum())) };
    let den = ch2 - (2.0 * c * c - 1.0);
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((2.0 * s * c / den, -sh2 / den))
}

/// `e` to a power and to its negative, and the hyperbolic cosine and sine made from them.
fn halves(x: f64) -> (f64, f64) {
    let (e, em) = (precise::exp(x), precise::exp(-x));
    ((e + em) / 2.0, (e - em) / 2.0)
}

/// The reciprocal of a real number, `#NUM!` for zero.
fn reciprocal(x: f64) -> Result<Pair, ErrorKind> {
    if x == 0.0 {
        Err(ErrorKind::Num)
    } else {
        Ok((1.0 / x, 0.0))
    }
}

/// Two over a complex number whose parts are built from `e` to a power. A divisor of exactly zero is
/// `#NUM!`.
fn two_over(den: Pair) -> Result<Pair, ErrorKind> {
    divide((2.0, 0.0), den)
}

/// `IMSEC`: 1 over cos a for a real number, otherwise 2 cos a cosh b + 2i sin a sinh b over
/// (cosh² b + sinh² b + cos 2a). Beyond `SQUARE_LIMIT` it is 2 over (e^iz + e^-iz).
fn sec(z: Pair) -> Result<Pair, ErrorKind> {
    let (c, s) = (trig_cos(z.0)?, trig_sin(z.0)?);
    if z.1 == 0.0 {
        return reciprocal(c);
    }
    if z.1.abs() > SQUARE_LIMIT {
        let (e, em) = (clamped_exp(z.1), clamped_exp(-z.1));
        return two_over(((em + e) * c, (em - e) * s));
    }
    let (ch, sh) = halves(z.1);
    let den = (ch * ch + sh * sh) + (2.0 * c * c - 1.0);
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((2.0 * c * ch / den, 2.0 * s * sh / den))
}

/// `IMCSC`: 1 over sin a for a real number, otherwise 2 sin a cosh b - 2i cos a sinh b over
/// (cosh 2b - cos 2a). Beyond `SQUARE_LIMIT` it is 2i over (e^iz - e^-iz).
fn csc(z: Pair) -> Result<Pair, ErrorKind> {
    let (c, s) = (trig_cos(z.0)?, trig_sin(z.0)?);
    if z.1 == 0.0 {
        return reciprocal(s);
    }
    if z.1.abs() > SQUARE_LIMIT {
        let (e, em) = (clamped_exp(z.1), clamped_exp(-z.1));
        return divide((0.0, 2.0), ((em - e) * c, (em + e) * s));
    }
    let (ch, sh) = halves(z.1);
    let e2 = precise::exp(2.0 * z.1);
    let em2 = 1.0 / e2;
    let den = (e2 + em2) / 2.0 - (1.0 - 2.0 * s * s);
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((2.0 * s * ch / den, -2.0 * c * sh / den))
}

/// `IMSECH`: 1 over cosh a for a real number, otherwise (cos b cosh a - i sin b sinh a) over
/// (cos² b + sinh² a). Beyond `SQUARE_LIMIT` it is 2 over (e^z + e^-z).
fn sech(z: Pair) -> Result<Pair, ErrorKind> {
    let (c, s) = (trig_cos(z.1)?, trig_sin(z.1)?);
    if z.1 == 0.0 {
        return Ok((1.0 / cosh_of(z.0), 0.0));
    }
    if z.0.abs() > SQUARE_LIMIT {
        let (e, em) = (clamped_exp(z.0), clamped_exp(-z.0));
        return two_over(((e + em) * c, (e - em) * s));
    }
    let (ch, sh) = halves(z.0);
    let den = c * c + sh * sh;
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((c * ch / den, -(s * sh) / den))
}

/// `IMCSCH`: 1 over sinh a for a real number, otherwise (cos b sinh a - i sin b cosh a) over
/// (sin² b + sinh² a). Beyond `SQUARE_LIMIT` it is 2 over (e^z - e^-z).
fn csch(z: Pair) -> Result<Pair, ErrorKind> {
    let (c, s) = (trig_cos(z.1)?, trig_sin(z.1)?);
    if z.1 == 0.0 {
        return reciprocal(sinh_of(z.0));
    }
    if z.0.abs() > SQUARE_LIMIT {
        let (e, em) = (clamped_exp(z.0), clamped_exp(-z.0));
        return two_over(((e - em) * c, (e + em) * s));
    }
    let (ch, sh) = halves(z.0);
    let den = s * s + sh * sh;
    if den == 0.0 {
        return Err(ErrorKind::Num);
    }
    finite((c * sh / den, -(s * ch) / den))
}

// ---------------------------------------------------------------------------------------------------
// Bessel functions.

/// The Bessel functions take an order and a real number.
type BesselFn = fn(i64, f64) -> Result<f64, ErrorKind>;

/// `BESSELJ(x, n)` and its relatives: `x` is a number and `n` an order that is truncated, not rounded.
fn bessel(ctx: &mut Ctx, args: &[Expr], f: BesselFn) -> Val {
    let result = (|| {
        let x = number_arg(ctx, &args[0])?;
        let n = number_arg(ctx, &args[1])?;
        if !(0.0..100_000_000.0).contains(&n) {
            return Err(ErrorKind::Num);
        }
        let value = flush(f(n as i64, x)?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ErrorKind::Num)
        }
    })();
    number_val(result)
}

/// Two over pi as the Numerical Recipes routines type it, to nine digits, which is what the answers are
/// made with.
#[allow(clippy::approx_constant)]
const TWO_OVER_PI: f64 = 0.636619772;

/// The order at which the Numerical Recipes routines stop adding a safety margin to their start.
const ACCURACY: f64 = 40.0;
const BIG: f64 = 1e10;
const SMALL: f64 = 1e-10;

/// The sine and cosine of the phase `x - offset` of a large argument. For order 0 the test for an argument
/// too large to reduce is made on `x` itself and for order 1 on the phase, which is what Excel does.
fn phase(x: f64, offset: f64, order_one: bool) -> Result<(f64, f64), ErrorKind> {
    if !order_one && x.abs() >= TRIG_LIMIT {
        return Err(ErrorKind::Num);
    }
    Ok((trig_sin(x - offset)?, trig_cos(x - offset)?))
}

/// Order 0 and 1 of the first kind for an argument of 8 or more: a modulus and a phase from two
/// polynomials. `which` picks order 0 or 1. Both copies of the coefficients carry the faults in Excel's.
fn bessj_large(ax: f64, order_one: bool) -> Result<f64, ErrorKind> {
    let z = 8.0 / ax;
    let y = z * z;
    if order_one {
        let (s, c) = phase(ax, 2.356194491, true)?;
        let p = 1.0
            + y * (0.183105e-2
                + y * (-0.3516396496e-4
                    + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6)))));
        let q = 0.04687499995
            + y * (-0.2002690873e-3
                + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        return Ok((TWO_OVER_PI / ax).sqrt() * (c * p - z * s * q));
    }
    let (s, c) = phase(ax, 0.785398164, false)?;
    let p = 1.0
        + y * (-0.1098628267e-2
            + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
    let q = -0.1562499995e-1
        + y * (0.1430488765e-3
            + y * (-0.6911147651e-5 + y * (0.7621095161e-6 - y * 0.934935152e-7)));
    Ok((TWO_OVER_PI / ax).sqrt() * (c * p - z * s * q))
}

/// The first kind of order 0.
fn bessj0(x: f64) -> Result<f64, ErrorKind> {
    let ax = x.abs();
    if ax >= 8.0 {
        return bessj_large(ax, false);
    }
    let y = x * x;
    let top = 57568490574.0
        + y * (-13362590354.0
            + y * (651619640.7 + y * (-11214424.18 + y * (77392.33017 + y * (-184.9052456)))));
    let bottom = 57568490411.0
        + y * (1029532985.0 + y * (9494680.718 + y * (59272.64853 + y * (267.8532712 + y))));
    Ok(top / bottom)
}

/// The first kind of order 1.
fn bessj1(x: f64) -> Result<f64, ErrorKind> {
    let ax = x.abs();
    if ax >= 8.0 {
        let value = bessj_large(ax, true)?;
        return Ok(if x < 0.0 { -value } else { value });
    }
    let y = x * x;
    let top = x
        * (72362614232.0
            + y * (-7895059235.0
                + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * (-30.16036606))))));
    let bottom = 144725228442.0
        + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y))));
    Ok(top / bottom)
}

/// The first kind of order `n`: upward from orders 0 and 1 when `x` is larger than `n`, otherwise
/// downward from a start far above `n` and scaled by the sum identity.
fn bessj(n: i64, x: f64) -> Result<f64, ErrorKind> {
    match n {
        0 => return bessj0(x),
        1 => return bessj1(x),
        _ => {}
    }
    let ax = x.abs();
    if ax == 0.0 {
        return Ok(0.0);
    }
    let value = if ax > n as f64 { bessj_upward(n, ax)? } else { bessj_downward(n, ax) };
    Ok(if x < 0.0 && n & 1 == 1 { -value } else { value })
}

/// Order `n` of the first kind by the recurrence upward from orders 0 and 1.
fn bessj_upward(n: i64, ax: f64) -> Result<f64, ErrorKind> {
    let tox = 2.0 / ax;
    let (mut below, mut at) = (bessj0(ax)?, bessj1(ax)?);
    for j in 1..n {
        let above = j as f64 * tox * at - below;
        below = at;
        at = above;
    }
    Ok(at)
}

/// Order `n` of the first kind by Miller's downward recurrence, with the values scaled down whenever they
/// grow past `BIG`.
fn bessj_downward(n: i64, ax: f64) -> f64 {
    let tox = 2.0 / ax;
    let m = 2 * ((n + (ACCURACY * n as f64).sqrt() as i64) / 2);
    let (mut odd, mut answer, mut sum) = (false, 0.0, 0.0);
    let (mut above, mut at) = (0.0, 1.0);
    for j in (1..=m).rev() {
        let below = j as f64 * tox * at - above;
        above = at;
        at = below;
        if at.abs() > BIG {
            at *= SMALL;
            above *= SMALL;
            answer *= SMALL;
            sum *= SMALL;
        }
        if odd {
            sum += at;
        }
        odd = !odd;
        if j == n {
            answer = above;
        }
    }
    answer / (2.0 * sum - at)
}

/// The second kind of order 0.
fn bessy0(x: f64) -> Result<f64, ErrorKind> {
    if x < 8.0 {
        let y = x * x;
        let top = -2957821389.0
            + y * (7062834065.0
                + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))));
        let bottom = 40076544269.0
            + y * (745249964.8
                + y * (7189466.438 + y * (47447.26470 + y * (226.1030244 + y * 1.0))));
        return Ok(top / bottom + TWO_OVER_PI * bessj0(x)? * x.ln());
    }
    let z = 8.0 / x;
    let y = z * z;
    let (s, c) = phase(x, 0.785398164, false)?;
    let p = 1.0
        + y * (-0.1098628627e-2
            + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
    let q = -0.1562499995e-1
        + y * (0.1430488765e-3
            + y * (-0.6911147651e-5 + y * (0.7621095161e-6 + y * (-0.934945152e-7))));
    Ok((TWO_OVER_PI / x).sqrt() * (s * p + z * c * q))
}

/// The second kind of order 1.
fn bessy1(x: f64) -> Result<f64, ErrorKind> {
    if x < 8.0 {
        let y = x * x;
        let top = x
            * (-0.4900604943e13
                + y * (0.1275274390e13
                    + y * (-0.5153438139e11
                        + y * (0.7349264551e9 + y * (-0.4237922726e7 + y * 0.8511937935e4)))));
        let bottom = 0.2499580570e14
            + y * (0.4244419664e12
                + y * (0.3733650367e10
                    + y * (0.2245904002e8 + y * (0.1020426050e6 + y * (0.3549632885e3 + y)))));
        return Ok(top / bottom + TWO_OVER_PI * (bessj1(x)? * x.ln() - 1.0 / x));
    }
    let z = 8.0 / x;
    let y = z * z;
    let (s, c) = phase(x, 2.356194491, true)?;
    let p = 1.0
        + y * (0.183105e-2
            + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
    let q = 0.04687499995
        + y * (-0.2002690873e-3
            + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
    Ok((TWO_OVER_PI / x).sqrt() * (s * p + z * c * q))
}

/// The second kind of order `n`, by the recurrence upward from orders 0 and 1. `x` must be positive.
fn bessy(n: i64, x: f64) -> Result<f64, ErrorKind> {
    if x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    match n {
        0 => return bessy0(x),
        1 => return bessy1(x),
        _ => {}
    }
    let tox = 2.0 / x;
    let (mut below, mut at) = (bessy0(x)?, bessy1(x)?);
    for j in 1..n {
        let above = j as f64 * tox * at - below;
        below = at;
        at = above;
        if !at.is_finite() {
            return Err(ErrorKind::Num);
        }
    }
    Ok(at)
}

/// The modified first kind of order 0.
fn bessi0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        return 1.0
            + y * (3.5156229
                + y * (3.0899424
                    + y * (1.2067492 + y * (0.2659732 + y * (0.360768e-1 + y * 0.45813e-2)))));
    }
    let y = 3.75 / ax;
    (ax.exp() / ax.sqrt())
        * (0.39894228
            + y * (0.1328592e-1
                + y * (0.225319e-2
                    + y * (-0.157565e-2
                        + y * (0.916281e-2
                            + y * (-0.2057706e-1
                                + y * (0.2635537e-1 + y * (-0.1647633e-1 + y * 0.392377e-2))))))))
}

/// The modified first kind of order 1.
fn bessi1(x: f64) -> f64 {
    let ax = x.abs();
    let value = if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        ax * (0.5
            + y * (0.87890594
                + y * (0.51498869
                    + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3))))))
    } else {
        let y = 3.75 / ax;
        let tail = 0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2));
        let tail = 0.39894228
            + y * (-0.3988024e-1
                + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * tail))));
        tail * (ax.exp() / ax.sqrt())
    };
    if x < 0.0 {
        -value
    } else {
        value
    }
}

/// The modified first kind of order `n`, by Miller's downward recurrence scaled by order 0.
fn bessi(n: i64, x: f64) -> Result<f64, ErrorKind> {
    match n {
        0 => return Ok(bessi0(x)),
        1 => return Ok(bessi1(x)),
        _ => {}
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    let tox = 2.0 / x.abs();
    let (mut above, mut answer, mut at) = (0.0, 0.0, 1.0);
    for j in (1..=2 * (n + (ACCURACY * n as f64).sqrt() as i64)).rev() {
        let below = above + j as f64 * tox * at;
        above = at;
        at = below;
        if at.abs() > BIG {
            answer *= SMALL;
            at *= SMALL;
            above *= SMALL;
        }
        if j == n {
            answer = above;
        }
    }
    let value = answer * (bessi0(x) / at);
    Ok(if x < 0.0 && n & 1 == 1 { -value } else { value })
}

/// The modified second kind of order 0. `x` must be positive.
fn bessk0(x: f64) -> f64 {
    if x <= 2.0 {
        let y = x * x / 4.0;
        return (-(x / 2.0).ln() * bessi0(x))
            + (-0.57721566
                + y * (0.42278420
                    + y * (0.23069756
                        + y * (0.3488590e-1
                            + y * (0.262698e-2 + y * (0.10750e-3 + y * 0.74e-5))))));
    }
    let y = 2.0 / x;
    ((-x).exp() / x.sqrt())
        * (1.25331414
            + y * (-0.7832358e-1
                + y * (0.2189568e-1
                    + y * (-0.1062446e-1
                        + y * (0.587872e-2 + y * (-0.251540e-2 + y * 0.53208e-3))))))
}

/// The modified second kind of order 1. `x` must be positive.
fn bessk1(x: f64) -> f64 {
    if x <= 2.0 {
        let y = x * x / 4.0;
        return ((x / 2.0).ln() * bessi1(x))
            + (1.0 / x)
                * (1.0
                    + y * (0.15443144
                        + y * (-0.67278579
                            + y * (-0.18156897
                                + y * (-0.1919402e-1 + y * (-0.110404e-2 + y * (-0.4686e-4)))))));
    }
    let y = 2.0 / x;
    ((-x).exp() / x.sqrt())
        * (1.25331414
            + y * (0.23498619
                + y * (-0.3655620e-1
                    + y * (0.1504268e-1
                        + y * (-0.780353e-2 + y * (0.325614e-2 + y * (-0.68245e-3)))))))
}

/// The modified second kind of order `n`, by the recurrence upward from orders 0 and 1.
fn bessk(n: i64, x: f64) -> Result<f64, ErrorKind> {
    if x <= 0.0 {
        return Err(ErrorKind::Num);
    }
    match n {
        0 => return Ok(bessk0(x)),
        1 => return Ok(bessk1(x)),
        _ => {}
    }
    let tox = 2.0 / x;
    let (mut below, mut at) = (bessk0(x), bessk1(x));
    for j in 1..n {
        let above = below + j as f64 * tox * at;
        below = at;
        at = above;
        if !at.is_finite() {
            return Err(ErrorKind::Num);
        }
    }
    Ok(at)
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn the_parts_of_a_complex_number_are_read_and_written() {
        check(&[
            ("COMPLEX(3,4)", "3+4i"),
            ("COMPLEX(3,4,\"j\")", "3+4j"),
            ("COMPLEX(0,1)", "i"),
            ("COMPLEX(0,-1)", "-i"),
            ("COMPLEX(0,0)", "0"),
            ("COMPLEX(1,2,\"k\")", "#VALUE!"),
            ("IMABS(\"3 + 4i\")", "5"),
            ("IMABS(\"3+4I\")", "#NUM!"),
            ("IMAGINARY(\"1--4i\")", "4"),
            ("IMSUM(\"1+i\",\"1+j\")", "#VALUE!"),
            ("IMSUB(\"1+i\",\"1+j\")", "#NUM!"),
            ("IMPRODUCT(\"1+2i\",\"3+4i\")", "-5+10i"),
            ("IMDIV(\"3+4i\",\"1-2i\")", "-1+2i"),
            ("IMSQRT(\"-1\")", "6.1257422745431E-17+i"),
            ("IMPOWER(\"i\",2)", "-1+1.22514845490862E-16i"),
        ]);
    }

    #[test]
    fn a_number_is_written_out_until_it_is_long() {
        check(&[
            ("COMPLEX(1E20,1E-18)", "100000000000000000000+0.000000000000000001i"),
            ("COMPLEX(1E21,1.5E-19)", "1E+21+1.5E-19i"),
            ("COMPLEX(1.23456789012345E-6,1)", "1.23456789012345E-06+i"),
        ]);
    }

    #[test]
    fn the_bessel_functions_carry_excels_errors() {
        check(&[
            ("BESSELJ(0,0)", "1.00000000283"),
            ("BESSELJ(8,1)", "0.234628386507"),
            ("BESSELJ(2,-1)", "#NUM!"),
            ("BESSELK(0,0)", "#NUM!"),
            ("BESSELK(2,0)", "0.11389388"),
        ]);
    }
}
