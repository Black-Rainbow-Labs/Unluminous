//! Math and trigonometry.

use super::criteria::Criterion;
use super::{gather, num, number, opt_num, r, s, unary, Function, Gather, MANY};
use crate::eval::{Area, Ctx, Val};
use crate::formula::Expr;
use crate::value::{round_significant, Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("SUM", 1, MANY, "number1, [number2], ...", "Adds its arguments.", |ctx, args| {
        gather(ctx, args, Gather::Numbers).map_or_else(Val::error, |n| Val::number(n.iter().sum()))
    }),
    r("PRODUCT", 1, MANY, "number1, [number2], ...", "Multiplies its arguments.", |ctx, args| {
        gather(ctx, args, Gather::Numbers).map_or_else(Val::error, |n| Val::number(if n.is_empty() { 0.0 } else { n.iter().product() }))
    }),
    r("SUMSQ", 1, MANY, "number1, [number2], ...", "The sum of the squares of its arguments.", |ctx, args| {
        gather(ctx, args, Gather::Numbers).map_or_else(Val::error, |n| Val::number(n.iter().map(|x| x * x).sum()))
    }),
    r("SUMIF", 2, 3, "range, criteria, [sum_range]", "Adds the cells that meet a criterion.", sumif),
    r("SUMIFS", 3, MANY, "sum_range, criteria_range1, criteria1, ...", "Adds the cells that meet every criterion.", sumifs),
    r("SUMPRODUCT", 1, MANY, "array1, [array2], ...", "Multiplies matching items of arrays and adds the products.", sumproduct),
    r("SUMX2MY2", 2, 2, "array_x, array_y", "The sum of the differences of squares.", |ctx, a| pairs(ctx, a, |x, y| x * x - y * y)),
    r("SUMX2PY2", 2, 2, "array_x, array_y", "The sum of the sums of squares.", |ctx, a| pairs(ctx, a, |x, y| x * x + y * y)),
    r("SUMXMY2", 2, 2, "array_x, array_y", "The sum of squares of differences.", |ctx, a| pairs(ctx, a, |x, y| (x - y) * (x - y))),
    s("ABS", 1, 1, "number", "The absolute value.", |ctx, a| unary(ctx, a, |x| Ok(x.abs()))),
    s("SIGN", 1, 1, "number", "1, 0 or -1 for the sign.", |ctx, a| unary(ctx, a, |x| Ok(if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 }))),
    s("INT", 1, 1, "number", "Rounds down to a whole number.", |ctx, a| unary(ctx, a, |x| Ok(x.floor()))),
    s("TRUNC", 1, 2, "number, [num_digits]", "Cuts off the decimals.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let d = opt_num(ctx, a, 1, 0.0)?.trunc();
        number(round_to(x, d, Rounding::Down))
    }),
    s("ROUND", 2, 2, "number, num_digits", "Rounds to a number of digits, halves away from zero.", |ctx, a| {
        number(round_to(num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), Rounding::Half))
    }),
    s("ROUNDUP", 2, 2, "number, num_digits", "Rounds away from zero.", |ctx, a| {
        number(round_to(num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), Rounding::Up))
    }),
    s("ROUNDDOWN", 2, 2, "number, num_digits", "Rounds toward zero.", |ctx, a| {
        number(round_to(num(ctx, &a[0])?, num(ctx, &a[1])?.trunc(), Rounding::Down))
    }),
    s("MROUND", 2, 2, "number, multiple", "Rounds to the nearest multiple.", |ctx, a| {
        let (x, m) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if m == 0.0 {
            return number(0.0);
        }
        if x * m < 0.0 {
            return Err(ErrorKind::Num);
        }
        number(round_to(x / m, 0.0, Rounding::Half) * m)
    }),
    s("CEILING", 2, 2, "number, significance", "Rounds up to a multiple of the significance.", |ctx, a| {
        let (x, m) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if m == 0.0 {
            return number(0.0);
        }
        if x > 0.0 && m < 0.0 {
            return Err(ErrorKind::Num);
        }
        number(clean(x / m).ceil() * m)
    }),
    s("FLOOR", 2, 2, "number, significance", "Rounds down to a multiple of the significance.", |ctx, a| {
        let (x, m) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if m == 0.0 {
            return if x == 0.0 { number(0.0) } else { Err(ErrorKind::Div0) };
        }
        if x > 0.0 && m < 0.0 {
            return Err(ErrorKind::Num);
        }
        number(clean(x / m).floor() * m)
    }),
    s("CEILING.MATH", 1, 3, "number, [significance], [mode]", "Rounds up to a multiple; mode rounds negatives away from zero.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let m = opt_num(ctx, a, 1, 1.0)?.abs();
        let mode = opt_num(ctx, a, 2, 0.0)?;
        if m == 0.0 {
            return number(0.0);
        }
        let q = clean(x / m);
        number(if x < 0.0 && mode != 0.0 { q.floor() } else { q.ceil() } * m)
    }),
    s("FLOOR.MATH", 1, 3, "number, [significance], [mode]", "Rounds down to a multiple; mode rounds negatives toward zero.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let m = opt_num(ctx, a, 1, 1.0)?.abs();
        let mode = opt_num(ctx, a, 2, 0.0)?;
        if m == 0.0 {
            return number(0.0);
        }
        let q = clean(x / m);
        number(if x < 0.0 && mode != 0.0 { q.ceil() } else { q.floor() } * m)
    }),
    s("CEILING.PRECISE", 1, 2, "number, [significance]", "Rounds up to a multiple, whatever the signs.", precise_up),
    s("ISO.CEILING", 1, 2, "number, [significance]", "Rounds up to a multiple, whatever the signs.", precise_up),
    s("FLOOR.PRECISE", 1, 2, "number, [significance]", "Rounds down to a multiple, whatever the signs.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let m = opt_num(ctx, a, 1, 1.0)?.abs();
        if m == 0.0 {
            return number(0.0);
        }
        number(clean(x / m).floor() * m)
    }),
    s("EVEN", 1, 1, "number", "Rounds away from zero to an even whole number.", |ctx, a| {
        unary(ctx, a, |x| {
            let up = (x.abs() / 2.0).ceil() * 2.0;
            Ok(if x < 0.0 { -up } else { up })
        })
    }),
    s("ODD", 1, 1, "number", "Rounds away from zero to an odd whole number.", |ctx, a| {
        unary(ctx, a, |x| {
            let v = x.abs().ceil();
            let up = if v % 2.0 == 0.0 { v + 1.0 } else { v };
            let up = if up == 0.0 { 1.0 } else { up };
            Ok(if x < 0.0 { -up } else { up })
        })
    }),
    s("MOD", 2, 2, "number, divisor", "The remainder after division, with the divisor's sign.", |ctx, a| {
        let (x, d) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if d == 0.0 {
            return Err(ErrorKind::Div0);
        }
        // Measured: Excel refuses once the quotient reaches 2^40.
        if (x / d).abs() >= 1_099_511_627_776.0 {
            return Err(ErrorKind::Num);
        }
        let m = x - d * (x / d).floor();
        number(if (m - d).abs() < d.abs() * 1e-15 { 0.0 } else { m })
    }),
    s("QUOTIENT", 2, 2, "numerator, denominator", "The whole part of a division.", |ctx, a| {
        let (x, d) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if d == 0.0 {
            return Err(ErrorKind::Div0);
        }
        number((x / d).trunc())
    }),
    s("POWER", 2, 2, "number, power", "A number raised to a power.", |ctx, a| number(crate::eval::power(num(ctx, &a[0])?, num(ctx, &a[1])?)?)),
    s("SQRT", 1, 1, "number", "The square root.", |ctx, a| unary(ctx, a, |x| if x < 0.0 { Err(ErrorKind::Num) } else { Ok(x.sqrt()) })),
    s("SQRTPI", 1, 1, "number", "The square root of the number times pi.", |ctx, a| {
        unary(ctx, a, |x| if x < 0.0 { Err(ErrorKind::Num) } else { Ok((x * std::f64::consts::PI).sqrt()) })
    }),
    s("EXP", 1, 1, "number", "e raised to a power.", |ctx, a| unary(ctx, a, |x| Ok(x.exp()))),
    s("LN", 1, 1, "number", "The natural logarithm.", |ctx, a| unary(ctx, a, |x| if x <= 0.0 { Err(ErrorKind::Num) } else { Ok(x.ln()) })),
    s("LOG10", 1, 1, "number", "The base 10 logarithm.", |ctx, a| unary(ctx, a, |x| if x <= 0.0 { Err(ErrorKind::Num) } else { Ok(x.log10()) })),
    s("LOG", 1, 2, "number, [base]", "The logarithm in a base, 10 when left out.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let b = opt_num(ctx, a, 1, 10.0)?;
        if x <= 0.0 || b <= 0.0 {
            return Err(ErrorKind::Num);
        }
        if b == 1.0 {
            return Err(ErrorKind::Div0);
        }
        number(if b == 10.0 { x.log10() } else { x.ln() / b.ln() })
    }),
    s("PI", 0, 0, "", "Pi to fifteen digits.", |_, _| number(std::f64::consts::PI)),
    s("SIN", 1, 1, "number", "The sine of an angle in radians.", |ctx, a| unary(ctx, a, |x| big_angle(x).map(f64::sin))),
    s("COS", 1, 1, "number", "The cosine of an angle in radians.", |ctx, a| unary(ctx, a, |x| big_angle(x).map(f64::cos))),
    s("TAN", 1, 1, "number", "The tangent of an angle in radians.", |ctx, a| unary(ctx, a, |x| big_angle(x).map(f64::tan))),
    s("ASIN", 1, 1, "number", "The arcsine, in radians.", |ctx, a| unary(ctx, a, |x| if x.abs() > 1.0 { Err(ErrorKind::Num) } else { Ok(x.asin()) })),
    s("ACOS", 1, 1, "number", "The arccosine, in radians.", |ctx, a| unary(ctx, a, |x| if x.abs() > 1.0 { Err(ErrorKind::Num) } else { Ok(x.acos()) })),
    s("ATAN", 1, 1, "number", "The arctangent, in radians.", |ctx, a| unary(ctx, a, |x| Ok(x.atan()))),
    s("ATAN2", 2, 2, "x_num, y_num", "The angle of a point from the x axis, in radians.", |ctx, a| {
        let (x, y) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
        if x == 0.0 && y == 0.0 {
            return Err(ErrorKind::Div0);
        }
        number(y.atan2(x))
    }),
    s("SINH", 1, 1, "number", "The hyperbolic sine.", |ctx, a| unary(ctx, a, |x| Ok(x.sinh()))),
    s("COSH", 1, 1, "number", "The hyperbolic cosine.", |ctx, a| unary(ctx, a, |x| Ok(x.cosh()))),
    s("TANH", 1, 1, "number", "The hyperbolic tangent.", |ctx, a| unary(ctx, a, |x| Ok(x.tanh()))),
    s("ASINH", 1, 1, "number", "The inverse hyperbolic sine.", |ctx, a| unary(ctx, a, |x| Ok(x.asinh()))),
    s("ACOSH", 1, 1, "number", "The inverse hyperbolic cosine.", |ctx, a| unary(ctx, a, |x| if x < 1.0 { Err(ErrorKind::Num) } else { Ok(x.acosh()) })),
    s("ATANH", 1, 1, "number", "The inverse hyperbolic tangent.", |ctx, a| unary(ctx, a, |x| if x.abs() >= 1.0 { Err(ErrorKind::Num) } else { Ok(x.atanh()) })),
    s("COT", 1, 1, "number", "The cotangent.", |ctx, a| unary(ctx, a, |x| if x == 0.0 { Err(ErrorKind::Div0) } else { big_angle(x).map(|x| 1.0 / x.tan()) })),
    s("COTH", 1, 1, "number", "The hyperbolic cotangent.", |ctx, a| unary(ctx, a, |x| if x == 0.0 { Err(ErrorKind::Div0) } else { Ok(1.0 / x.tanh()) })),
    s("CSC", 1, 1, "number", "The cosecant.", |ctx, a| unary(ctx, a, |x| if x == 0.0 { Err(ErrorKind::Div0) } else { big_angle(x).map(|x| 1.0 / x.sin()) })),
    s("CSCH", 1, 1, "number", "The hyperbolic cosecant.", |ctx, a| unary(ctx, a, |x| if x == 0.0 { Err(ErrorKind::Div0) } else { Ok(1.0 / x.sinh()) })),
    s("SEC", 1, 1, "number", "The secant.", |ctx, a| unary(ctx, a, |x| big_angle(x).map(|x| 1.0 / x.cos()))),
    s("SECH", 1, 1, "number", "The hyperbolic secant.", |ctx, a| unary(ctx, a, |x| Ok(1.0 / x.cosh()))),
    s("ACOT", 1, 1, "number", "The arccotangent.", |ctx, a| unary(ctx, a, |x| Ok(std::f64::consts::FRAC_PI_2 - x.atan()))),
    s("ACOTH", 1, 1, "number", "The inverse hyperbolic cotangent.", |ctx, a| unary(ctx, a, |x| if x.abs() <= 1.0 { Err(ErrorKind::Num) } else { Ok(0.5 * ((x + 1.0) / (x - 1.0)).ln()) })),
    s("DEGREES", 1, 1, "angle", "Radians to degrees.", |ctx, a| unary(ctx, a, |x| Ok(x.to_degrees()))),
    s("RADIANS", 1, 1, "angle", "Degrees to radians.", |ctx, a| unary(ctx, a, |x| Ok(x.to_radians()))),
    s("FACT", 1, 1, "number", "The factorial.", |ctx, a| unary(ctx, a, |x| factorial(x.trunc()))),
    s("FACTDOUBLE", 1, 1, "number", "The double factorial.", |ctx, a| {
        unary(ctx, a, |x| {
            let n = x.trunc();
            if n < -1.0 {
                return Err(ErrorKind::Num);
            }
            let mut out = 1.0;
            let mut k = n;
            while k > 1.0 {
                out *= k;
                k -= 2.0;
            }
            Ok(out)
        })
    }),
    s("COMBIN", 2, 2, "number, number_chosen", "The number of combinations.", |ctx, a| {
        number(combin(num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc())?)
    }),
    s("COMBINA", 2, 2, "number, number_chosen", "The number of combinations with repetitions.", |ctx, a| {
        let (n, k) = (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc());
        if n < 0.0 || k < 0.0 || n == 0.0 && k > 0.0 {
            return Err(ErrorKind::Num);
        }
        if n == 0.0 && k == 0.0 {
            return number(1.0);
        }
        number(combin(n + k - 1.0, k)?)
    }),
    s("PERMUT", 2, 2, "number, number_chosen", "The number of permutations.", |ctx, a| {
        let (n, k) = (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc());
        if n < 0.0 || k < 0.0 || k > n {
            return Err(ErrorKind::Num);
        }
        let mut out = 1.0;
        for i in 0..k as i64 {
            out *= n - i as f64;
        }
        number(out)
    }),
    s("PERMUTATIONA", 2, 2, "number, number_chosen", "The number of permutations with repetitions.", |ctx, a| {
        let (n, k) = (num(ctx, &a[0])?.trunc(), num(ctx, &a[1])?.trunc());
        if n < 0.0 || k < 0.0 {
            return Err(ErrorKind::Num);
        }
        number(n.powf(k))
    }),
    s("MULTINOMIAL", 1, MANY, "number1, [number2], ...", "The multinomial of numbers.", |ctx, a| {
        let mut total = 0.0;
        let mut denominator = 1.0;
        for v in a {
            let n = num(ctx, v)?.trunc();
            if n < 0.0 {
                return Err(ErrorKind::Num);
            }
            total += n;
            denominator *= factorial(n)?;
        }
        number(factorial(total)? / denominator)
    }),
    r("GCD", 1, MANY, "number1, [number2], ...", "The greatest common divisor.", |ctx, args| {
        let n = match gather(ctx, args, Gather::Numbers) {
            Ok(n) => n,
            Err(e) => return Val::error(e),
        };
        if n.iter().any(|x| *x < 0.0) {
            return Val::error(ErrorKind::Num);
        }
        Val::number(n.iter().map(|x| x.trunc()).fold(0.0, gcd))
    }),
    r("LCM", 1, MANY, "number1, [number2], ...", "The least common multiple.", |ctx, args| {
        let n = match gather(ctx, args, Gather::Numbers) {
            Ok(n) => n,
            Err(e) => return Val::error(e),
        };
        if n.iter().any(|x| *x < 0.0) {
            return Val::error(ErrorKind::Num);
        }
        Val::number(n.iter().map(|x| x.trunc()).fold(1.0, |acc, x| if acc == 0.0 || x == 0.0 { 0.0 } else { acc / gcd(acc, x) * x }))
    }),
    r("RAND", 0, 0, "", "A random number from 0 up to 1. Changes on every calculation.", |ctx, _| Val::number(ctx.env.random())),
    s("RANDBETWEEN", 2, 2, "bottom, top", "A random whole number between two numbers. Changes on every calculation.", |ctx, a| {
        let (lo, hi) = (num(ctx, &a[0])?.ceil(), num(ctx, &a[1])?.floor());
        if lo > hi {
            return Err(ErrorKind::Num);
        }
        number(lo + (ctx.env.random() * (hi - lo + 1.0)).floor())
    }),
    r("RANDARRAY", 0, 5, "[rows], [columns], [min], [max], [whole_number]", "An array of random numbers.", randarray),
    r("SEQUENCE", 1, 4, "rows, [columns], [start], [step]", "An array of numbers in sequence.", sequence),
    r("SUBTOTAL", 2, MANY, "function_num, ref1, [ref2], ...", "An aggregate that skips other subtotals, and hidden rows from 101 on.", subtotal),
    r("AGGREGATE", 3, MANY, "function_num, options, ref1, [ref2], ...", "An aggregate that can skip errors, hidden rows and subtotals.", aggregate),
    r("MMULT", 2, 2, "array1, array2", "The matrix product.", mmult),
    r("MDETERM", 1, 1, "array", "The matrix determinant.", |ctx, a| match square(ctx, &a[0]) {
        Ok(m) => Val::number(determinant(m)),
        Err(e) => Val::error(e),
    }),
    r("MINVERSE", 1, 1, "array", "The inverse matrix.", |ctx, a| match square(ctx, &a[0]) {
        Ok(m) => match inverse(m) {
            Some(inv) => {
                let n = inv.len();
                Val::Array(Array::new(n, n, inv.into_iter().flatten().map(Value::Number).collect()))
            }
            None => Val::error(ErrorKind::Num),
        },
        Err(e) => Val::error(e),
    }),
    r("MUNIT", 1, 1, "dimension", "The identity matrix.", |ctx, a| {
        let n = match scalar_arg(ctx, a, 0, 0.0) {
            Ok(n) => n.trunc(),
            Err(e) => return Val::error(e),
        };
        if n < 1.0 || n > 4096.0 {
            return Val::error(ErrorKind::Value);
        }
        let n = n as usize;
        Val::Array(Array::new(n, n, (0..n * n).map(|i| Value::Number(if i / n == i % n { 1.0 } else { 0.0 })).collect()))
    }),
    s("ROMAN", 1, 2, "number, [form]", "A number as Roman numerals.", |ctx, a| {
        let n = num(ctx, &a[0])?.trunc();
        let form = opt_num(ctx, a, 1, 0.0)?;
        if !(0.0..=3999.0).contains(&n) || !(0.0..=4.0).contains(&form) {
            return Err(ErrorKind::Value);
        }
        Ok(Value::text(roman(n as u32, form as u32)))
    }),
    s("ARABIC", 1, 1, "text", "Roman numerals as a number.", |_, a| {
        let t = super::text_of(&a[0])?;
        arabic(t.trim()).map(|n| Value::Number(n as f64)).ok_or(ErrorKind::Value)
    }),
    s("BASE", 2, 3, "number, radix, [min_length]", "A number written in another base.", |ctx, a| {
        let n = num(ctx, &a[0])?.trunc();
        let radix = num(ctx, &a[1])?.trunc();
        let min = opt_num(ctx, a, 2, 0.0)?.trunc();
        if n < 0.0 || n >= 2f64.powi(53) || !(2.0..=36.0).contains(&radix) || min < 0.0 || min > 255.0 {
            return Err(ErrorKind::Num);
        }
        let mut digits = Vec::new();
        let mut x = n as u64;
        let r = radix as u64;
        loop {
            digits.push(std::char::from_digit((x % r) as u32, r as u32).unwrap_or('0').to_ascii_uppercase());
            x /= r;
            if x == 0 {
                break;
            }
        }
        while digits.len() < min as usize {
            digits.push('0');
        }
        Ok(Value::text(digits.iter().rev().collect::<String>()))
    }),
    s("DECIMAL", 2, 2, "text, radix", "A number written in a base, read as a number.", |ctx, a| {
        let t = super::text_of(&a[0])?;
        let radix = num(ctx, &a[1])?.trunc();
        if !(2.0..=36.0).contains(&radix) {
            return Err(ErrorKind::Num);
        }
        let mut n = 0.0;
        for c in t.trim().chars() {
            let d = c.to_digit(radix as u32).ok_or(ErrorKind::Num)?;
            n = n * radix + d as f64;
        }
        number(n)
    }),
    r("SERIESSUM", 4, 4, "x, n, m, coefficients", "The sum of a power series.", |ctx, a| {
        let mut scalars = Vec::new();
        for e in &a[..3] {
            let v = ctx.eval(e);
            let v = ctx.single(v);
            match ctx.number(&v) {
                Ok(n) => scalars.push(n),
                Err(e) => return Val::error(e),
            }
        }
        let coefficients = match gather(ctx, &a[3..], Gather::Numbers) {
            Ok(c) => c,
            Err(e) => return Val::error(e),
        };
        let (x, n, m) = (scalars[0], scalars[1], scalars[2]);
        Val::number(coefficients.iter().enumerate().map(|(i, c)| c * x.powf(n + i as f64 * m)).sum())
    }),
];

fn precise_up(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let x = num(ctx, &a[0])?;
    let m = opt_num(ctx, a, 1, 1.0)?.abs();
    if m == 0.0 {
        return number(0.0);
    }
    number(clean(x / m).ceil() * m)
}

/// A quotient rounded to fifteen significant digits, so 1.0000000000000002 is ceiled to 1 rather than 2.
fn clean(q: f64) -> f64 {
    round_significant(q, 15)
}

/// Excel refuses the trigonometric functions for angles too large to have meaningful digits.
fn big_angle(x: f64) -> Result<f64, ErrorKind> {
    if x.abs() >= 134_217_728.0 {
        Err(ErrorKind::Num)
    } else {
        Ok(x)
    }
}

#[derive(Clone, Copy)]
pub enum Rounding {
    Half,
    Up,
    Down,
}

/// Round to `digits` decimal places the way Excel does: on the fifteen digit decimal value, so
/// ROUND(2.675, 2) is 2.68 although 2.675 is stored as 2.67499999...
pub fn round_to(x: f64, digits: f64, how: Rounding) -> f64 {
    let digits = digits.clamp(-308.0, 308.0) as i32;
    let scale = 10f64.powi(digits.abs());
    let scaled = if digits >= 0 { x * scale } else { x / scale };
    let scaled = round_significant(scaled, 15);
    let rounded = match how {
        Rounding::Half => {
            let r = scaled.abs().round();
            if scaled < 0.0 {
                -r
            } else {
                r
            }
        }
        Rounding::Up => {
            let r = scaled.abs().ceil();
            if scaled < 0.0 {
                -r
            } else {
                r
            }
        }
        Rounding::Down => scaled.trunc(),
    };
    let out = if digits >= 0 { rounded / scale } else { rounded * scale };
    if out == 0.0 {
        0.0
    } else {
        out
    }
}

fn factorial(n: f64) -> Result<f64, ErrorKind> {
    if n < 0.0 {
        return Err(ErrorKind::Num);
    }
    if n > 170.0 {
        return Err(ErrorKind::Num);
    }
    Ok((1..=n as u64).fold(1.0, |acc, k| acc * k as f64))
}

pub fn combin(n: f64, k: f64) -> Result<f64, ErrorKind> {
    if n < 0.0 || k < 0.0 || k > n {
        return Err(ErrorKind::Num);
    }
    let k = k.min(n - k);
    let mut out = 1.0;
    for i in 0..k as i64 {
        out = out * (n - i as f64) / (i as f64 + 1.0);
    }
    Ok(out.round())
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a, b);
    while b != 0.0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn roman(n: u32, form: u32) -> String {
    if n == 0 {
        return String::new();
    }
    // Form 0 is classic; the concise forms allow more subtractive pairs, in steps up to form 4.
    let mut table: Vec<(u32, &str)> = vec![
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let extra: &[(u32, &str)] = match form {
        1 => &[(950, "LM"), (450, "LD"), (95, "VC"), (45, "VL")],
        2 => &[(990, "XM"), (950, "LM"), (490, "XD"), (450, "LD"), (99, "IC"), (95, "VC"), (49, "IL"), (45, "VL")],
        3 => &[(995, "VM"), (990, "XM"), (950, "LM"), (495, "VD"), (490, "XD"), (450, "LD"), (99, "IC"), (95, "VC"), (49, "IL"), (45, "VL")],
        4 => &[(999, "IM"), (995, "VM"), (990, "XM"), (950, "LM"), (499, "ID"), (495, "VD"), (490, "XD"), (450, "LD"), (99, "IC"), (95, "VC"), (49, "IL"), (45, "VL")],
        _ => &[],
    };
    table.extend_from_slice(extra);
    table.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = String::new();
    let mut n = n;
    for (value, letters) in table {
        while n >= value {
            out.push_str(letters);
            n -= value;
        }
    }
    out
}

fn arabic(text: &str) -> Option<i64> {
    if text.is_empty() {
        return Some(0);
    }
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = |c: char| match c.to_ascii_uppercase() {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    };
    let digits: Option<Vec<i64>> = body.chars().map(value).collect();
    let digits = digits?;
    let mut total = 0;
    for (i, d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|next| next > d) {
            total -= d;
        } else {
            total += d;
        }
    }
    Some(if negative { -total } else { total })
}

fn sumif(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let range = ctx.eval(&args[0]);
    let Val::Ref(area) = range else {
        return match range {
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        };
    };
    let sum_area = match args.get(2).map(|e| ctx.eval(e)) {
        None => area,
        Some(Val::Ref(a)) => a,
        Some(Val::Value(Value::Error(e))) => return Val::error(e),
        Some(_) => return Val::error(ErrorKind::Value),
    };
    let crit = ctx.eval(&args[1]);
    let (tests, rows, cols) = match clipped(ctx, &area) {
        Ok(x) => x,
        Err(e) => return Val::error(e),
    };
    let sums = ctx.read_range(sum_area.sheet, aligned(&sum_area, rows, cols));
    let date1904 = ctx.date1904();
    ctx.lift_vals(vec![crit], |_, c| {
        let criterion = Criterion::parse(&c[0], date1904);
        let mut total = 0.0;
        for (i, t) in tests.iter().enumerate() {
            if criterion.matches(t) {
                match sums.data.get(i) {
                    Some(Value::Number(n)) => total += n,
                    Some(Value::Error(e)) => return Value::Error(*e),
                    _ => {}
                }
            }
        }
        Value::Number(total)
    })
}

/// The criteria ranges and criteria of a *IFS function, matched cell by cell. `first` is the range every
/// criteria range must be the size of, the sum range for SUMIFS. Answers which positions meet every
/// criterion and the rows and columns read, a whole column being clipped to the rows in use.
pub fn ifs_mask(ctx: &mut Ctx, pairs: &[Expr], first: Option<Area>) -> Result<(Vec<bool>, usize, usize), ErrorKind> {
    if pairs.len() % 2 != 0 || pairs.is_empty() {
        return Err(ErrorKind::Value);
    }
    let date1904 = ctx.date1904();
    let mut shape: Option<(Area, usize, usize)> = None;
    if let Some(area) = first {
        let (_, rr, cc) = clipped(ctx, &area)?;
        shape = Some((area, rr, cc));
    }
    let mut mask: Option<Vec<bool>> = None;
    for pair in pairs.chunks(2) {
        let area = match ctx.eval(&pair[0]) {
            Val::Ref(area) => area,
            Val::Value(Value::Error(e)) => return Err(e),
            _ => return Err(ErrorKind::Value),
        };
        let crit = ctx.eval(&pair[1]);
        let crit = ctx.single(crit);
        if let Value::Error(e) = crit {
            return Err(e);
        }
        let (base, rr, cc) = match shape {
            Some(s) => s,
            None => {
                let (_, rr, cc) = clipped(ctx, &area)?;
                shape = Some((area, rr, cc));
                (area, rr, cc)
            }
        };
        if area.range.rows() != base.range.rows() || area.range.cols() != base.range.cols() {
            return Err(ErrorKind::Value);
        }
        let values = ctx.read_range(area.sheet, aligned(&area, rr, cc));
        let criterion = Criterion::parse(&crit, date1904);
        let m = mask.get_or_insert_with(|| vec![true; rr * cc]);
        for (i, v) in values.data.iter().enumerate() {
            if m[i] && !criterion.matches(v) {
                m[i] = false;
            }
        }
    }
    let (_, rr, cc) = shape.ok_or(ErrorKind::Value)?;
    Ok((mask.unwrap_or_default(), rr, cc))
}

fn sumifs(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let Val::Ref(sum_area) = ctx.eval(&args[0]) else { return Val::error(ErrorKind::Value) };
    let (mask, r, c) = match ifs_mask(ctx, &args[1..], Some(sum_area)) {
        Ok(m) => m,
        Err(e) => return Val::error(e),
    };
    let sums = ctx.read_range(sum_area.sheet, aligned(&sum_area, r, c));
    let mut total = 0.0;
    for (i, keep) in mask.iter().enumerate() {
        if *keep {
            match sums.data.get(i) {
                Some(Value::Number(n)) => total += n,
                Some(Value::Error(e)) => return Val::error(*e),
                _ => {}
            }
        }
    }
    Val::number(total)
}

/// An area's values as a flat list, a whole column clipped to the rows in use, and the shape read.
pub fn clipped(ctx: &mut Ctx, area: &Area) -> Result<(Vec<Value>, usize, usize), ErrorKind> {
    let a = ctx.array(Val::Ref(*area))?;
    Ok((a.data, a.rows, a.cols))
}

/// The same shape as another range, starting at this area's top left.
pub fn aligned(area: &Area, rows: usize, cols: usize) -> crate::coord::Range {
    let first = area.range.first;
    let last = crate::coord::CellRef::new(
        (first.row + rows.max(1) as u32 - 1).min(crate::coord::MAX_ROWS - 1),
        (first.col + cols.max(1) as u32 - 1).min(crate::coord::MAX_COLS - 1),
    );
    crate::coord::Range::new(first, last)
}

fn sumproduct(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut arrays = Vec::new();
    for arg in args {
        let v = ctx.eval(arg);
        match ctx.array(v) {
            Ok(a) => arrays.push(a),
            Err(e) => return Val::error(e),
        }
    }
    let (rows, cols) = (arrays[0].rows, arrays[0].cols);
    if arrays.iter().any(|a| a.rows != rows || a.cols != cols) {
        return Val::error(ErrorKind::Value);
    }
    let mut total = 0.0;
    for i in 0..rows * cols {
        let mut product = 1.0;
        for a in &arrays {
            match &a.data[i] {
                Value::Number(n) => product *= n,
                Value::Error(e) => return Val::error(*e),
                _ => product = 0.0,
            }
        }
        total += product;
    }
    Val::number(total)
}

fn pairs(ctx: &mut Ctx, args: &[Expr], f: fn(f64, f64) -> f64) -> Val {
    let (x, ..) = match super::numbers_at(ctx, &args[0]) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    let (y, ..) = match super::numbers_at(ctx, &args[1]) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    if x.len() != y.len() {
        return Val::error(ErrorKind::NA);
    }
    Val::number(x.iter().zip(&y).filter_map(|(a, b)| Some(f((*a)?, (*b)?))).sum())
}

fn scalar_arg(ctx: &mut Ctx, args: &[Expr], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match args.get(i) {
        None | Some(Expr::Missing) => Ok(default),
        Some(e) => {
            let v = ctx.eval(e);
            let v = ctx.single(v);
            if v.is_empty() {
                return Ok(default);
            }
            ctx.number(&v)
        }
    }
}

fn randarray(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let rows = match scalar_arg(ctx, args, 0, 1.0) {
        Ok(n) => n.trunc(),
        Err(e) => return Val::error(e),
    };
    let cols = match scalar_arg(ctx, args, 1, 1.0) {
        Ok(n) => n.trunc(),
        Err(e) => return Val::error(e),
    };
    let (lo, hi) = match (scalar_arg(ctx, args, 2, 0.0), scalar_arg(ctx, args, 3, 1.0)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Val::error(e),
    };
    let whole = match args.get(4) {
        Some(e) if !matches!(e, Expr::Missing) => {
            let v = ctx.eval(e);
            let v = ctx.single(v);
            crate::eval::to_bool(&v).unwrap_or(false)
        }
        _ => false,
    };
    if rows < 1.0 || cols < 1.0 || lo > hi {
        return Val::error(if rows == 0.0 || cols == 0.0 { ErrorKind::Calc } else { ErrorKind::Value });
    }
    let (rows, cols) = (rows as usize, cols as usize);
    let data = (0..rows * cols)
        .map(|_| {
            let r = ctx.env.random();
            Value::Number(if whole { (lo.ceil() + (r * (hi.floor() - lo.ceil() + 1.0)).floor()).min(hi.floor()) } else { lo + r * (hi - lo) })
        })
        .collect();
    Val::Array(Array::new(rows, cols, data))
}

fn sequence(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let values = [1.0, 1.0, 1.0, 1.0];
    let mut got = values;
    for (i, slot) in got.iter_mut().enumerate() {
        match scalar_arg(ctx, args, i, values[i]) {
            Ok(n) => *slot = n,
            Err(e) => return Val::error(e),
        }
    }
    let (rows, cols) = (got[0].trunc(), got[1].trunc());
    if rows < 0.0 || cols < 0.0 {
        return Val::error(ErrorKind::Value);
    }
    if rows == 0.0 || cols == 0.0 {
        return Val::error(ErrorKind::Calc);
    }
    if rows * cols > 1_048_576.0 * 16_384.0 {
        return Val::error(ErrorKind::Num);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    let data = (0..rows * cols).map(|i| Value::Number(got[2] + i as f64 * got[3])).collect();
    Val::Array(Array::new(rows, cols, data))
}

/// Whether a cell holds a SUBTOTAL or AGGREGATE, which SUBTOTAL leaves out so totals of totals do not count
/// twice.
fn is_subtotal_cell(ctx: &Ctx, sheet: usize, at: crate::coord::CellRef) -> bool {
    ctx.book.sheets[sheet].cell(at).and_then(|c| c.formula.as_ref()).is_some_and(|f| {
        let mut found = false;
        f.expr.walk(&mut |e| {
            if let Expr::Call(name, _) = e {
                if &**name == "SUBTOTAL" || &**name == "AGGREGATE" {
                    found = true;
                }
            }
        });
        found
    })
}

/// The values a SUBTOTAL or AGGREGATE reads: by cell, so hidden rows and nested subtotals can be left out.
fn subtotal_values(ctx: &mut Ctx, args: &[Expr], skip_hidden: bool, skip_errors: bool, skip_nested: bool) -> Result<Vec<Value>, ErrorKind> {
    let mut out = Vec::new();
    for arg in args {
        match ctx.eval(arg) {
            Val::Ref(area) => collect_area(ctx, &area, skip_hidden, skip_errors, skip_nested, &mut out)?,
            Val::Refs(areas) => {
                for area in areas {
                    collect_area(ctx, &area, skip_hidden, skip_errors, skip_nested, &mut out)?;
                }
            }
            Val::Value(Value::Error(e)) if !skip_errors => return Err(e),
            Val::Array(a) => out.extend(a.data.into_iter().filter(|v| !(skip_errors && v.is_error()))),
            Val::Value(v) => out.push(v),
            _ => return Err(ErrorKind::Value),
        }
    }
    Ok(out)
}

fn collect_area(ctx: &Ctx, area: &Area, skip_hidden: bool, skip_errors: bool, skip_nested: bool, out: &mut Vec<Value>) -> Result<(), ErrorKind> {
    for sheet in area.sheet..=area.sheet_last {
        let s = &ctx.book.sheets[sheet];
        let r = area.range;
        let cells: Vec<((u32, u32), Value)> = s
            .cells
            .range((r.first.row, 0)..=(r.last.row, crate::coord::MAX_COLS))
            .filter(|((_, col), _)| *col >= r.first.col && *col <= r.last.col)
            .map(|(k, c)| (*k, c.value.clone()))
            .chain(
                s.spills
                    .values
                    .range((r.first.row, 0)..=(r.last.row, crate::coord::MAX_COLS))
                    .filter(|((row, col), _)| *col >= r.first.col && *col <= r.last.col && !s.cells.contains_key(&(*row, *col)))
                    .map(|(k, v)| (*k, v.clone())),
            )
            .collect();
        for ((row, col), value) in cells {
            if value.is_empty() {
                continue;
            }
            let hidden = s.rows.get(&row).is_some_and(|info| info.hidden);
            let filtered = hidden && s.filter.as_ref().is_some_and(|f| f.range.contains(crate::coord::CellRef::new(row, col)));
            if (skip_hidden && hidden) || filtered {
                continue;
            }
            if skip_nested && is_subtotal_cell(ctx, sheet, crate::coord::CellRef::new(row, col)) {
                continue;
            }
            if let Value::Error(e) = value {
                if skip_errors {
                    continue;
                }
                return Err(e);
            }
            out.push(value);
        }
    }
    Ok(())
}

/// One of SUBTOTAL's and AGGREGATE's eleven aggregates over values read from ranges.
fn aggregate_of(which: u32, values: &[Value]) -> Result<f64, ErrorKind> {
    let numbers: Vec<f64> = values.iter().filter_map(Value::as_number).collect();
    use super::stats::{mean, sample_variance, population_variance};
    Ok(match which {
        1 => mean(&numbers).ok_or(ErrorKind::Div0)?,
        2 => numbers.len() as f64,
        3 => values.iter().filter(|v| !v.is_empty()).count() as f64,
        4 => if numbers.is_empty() { 0.0 } else { numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max) },
        5 => if numbers.is_empty() { 0.0 } else { numbers.iter().copied().fold(f64::INFINITY, f64::min) },
        6 => if numbers.is_empty() { 0.0 } else { numbers.iter().product() },
        7 => sample_variance(&numbers).ok_or(ErrorKind::Div0)?.sqrt(),
        8 => population_variance(&numbers).ok_or(ErrorKind::Div0)?.sqrt(),
        9 => numbers.iter().sum(),
        10 => sample_variance(&numbers).ok_or(ErrorKind::Div0)?,
        11 => population_variance(&numbers).ok_or(ErrorKind::Div0)?,
        _ => return Err(ErrorKind::Value),
    })
}

fn subtotal(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let which = match scalar_arg(ctx, args, 0, 0.0) {
        Ok(n) => n.trunc() as u32,
        Err(e) => return Val::error(e),
    };
    let (base, skip_hidden) = match which {
        1..=11 => (which, false),
        101..=111 => (which - 100, true),
        _ => return Val::error(ErrorKind::Value),
    };
    match subtotal_values(ctx, &args[1..], skip_hidden, false, true) {
        Ok(values) => aggregate_of(base, &values).map_or_else(Val::error, Val::number),
        Err(e) => Val::error(e),
    }
}

fn aggregate(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let which = match scalar_arg(ctx, args, 0, 0.0) {
        Ok(n) => n.trunc() as u32,
        Err(e) => return Val::error(e),
    };
    let options = match scalar_arg(ctx, args, 1, 0.0) {
        Ok(n) => n.trunc() as u32,
        Err(e) => return Val::error(e),
    };
    if options > 7 || !(1..=19).contains(&which) {
        return Val::error(ErrorKind::Value);
    }
    let skip_nested = matches!(options, 0..=3);
    let skip_hidden = matches!(options, 1 | 3 | 5 | 7);
    let skip_errors = matches!(options, 2 | 3 | 6 | 7);
    if which <= 13 {
        let values = match subtotal_values(ctx, &args[2..], skip_hidden, skip_errors, skip_nested) {
            Ok(v) => v,
            Err(e) => return Val::error(e),
        };
        return match which {
            1..=11 => aggregate_of(which, &values).map_or_else(Val::error, Val::number),
            12 => {
                let n: Vec<f64> = values.iter().filter_map(Value::as_number).collect();
                super::stats::median(&n).map_or(Val::error(ErrorKind::Num), Val::number)
            }
            _ => {
                let n: Vec<f64> = values.iter().filter_map(Value::as_number).collect();
                super::stats::modes(&n).first().map_or(Val::error(ErrorKind::NA), |m| Val::number(*m))
            }
        };
    }
    // 14 to 19 take an array and a k.
    let values = match subtotal_values(ctx, &args[2..3], skip_hidden, skip_errors, skip_nested) {
        Ok(v) => v,
        Err(e) => return Val::error(e),
    };
    let k = match scalar_arg(ctx, args, 3, f64::NAN) {
        Ok(k) if !k.is_nan() => k,
        Ok(_) => return Val::error(ErrorKind::Value),
        Err(e) => return Val::error(e),
    };
    let mut n: Vec<f64> = values.iter().filter_map(Value::as_number).collect();
    n.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let answer = match which {
        14 => super::stats::kth(&n, k, true),
        15 => super::stats::kth(&n, k, false),
        16 => super::stats::percentile_inc(&n, k),
        17 => super::stats::percentile_inc(&n, k / 4.0),
        18 => super::stats::percentile_exc(&n, k),
        _ => super::stats::percentile_exc(&n, k / 4.0),
    };
    answer.map_or_else(Val::error, Val::number)
}

fn matrix(ctx: &mut Ctx, expr: &Expr) -> Result<Vec<Vec<f64>>, ErrorKind> {
    let v = ctx.eval(expr);
    let a = ctx.array(v)?;
    let mut rows = Vec::new();
    for row in a.rows_iter() {
        let mut out = Vec::new();
        for v in row {
            match v {
                Value::Number(n) => out.push(*n),
                Value::Error(e) => return Err(*e),
                _ => return Err(ErrorKind::Value),
            }
        }
        rows.push(out);
    }
    Ok(rows)
}

fn square(ctx: &mut Ctx, expr: &Expr) -> Result<Vec<Vec<f64>>, ErrorKind> {
    let m = matrix(ctx, expr)?;
    if m.is_empty() || m.iter().any(|row| row.len() != m.len()) {
        return Err(ErrorKind::Value);
    }
    Ok(m)
}

fn mmult(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let (a, b) = match (matrix(ctx, &args[0]), matrix(ctx, &args[1])) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Val::error(e),
    };
    let (n, m, p) = (a.len(), b.len(), b.first().map_or(0, Vec::len));
    if a.first().map_or(0, Vec::len) != m {
        return Val::error(ErrorKind::Value);
    }
    let mut data = Vec::with_capacity(n * p);
    for row in &a {
        for j in 0..p {
            data.push(Value::Number((0..m).map(|k| row[k] * b[k][j]).sum()));
        }
    }
    Val::Array(Array::new(n, p, data))
}

pub fn determinant(mut m: Vec<Vec<f64>>) -> f64 {
    let n = m.len();
    let mut det = 1.0;
    for col in 0..n {
        let pivot = (col..n).max_by(|&a, &b| m[a][col].abs().partial_cmp(&m[b][col].abs()).unwrap_or(std::cmp::Ordering::Equal));
        let Some(pivot) = pivot else { return 0.0 };
        if m[pivot][col] == 0.0 {
            return 0.0;
        }
        if pivot != col {
            m.swap(pivot, col);
            det = -det;
        }
        det *= m[col][col];
        for row in col + 1..n {
            let factor = m[row][col] / m[col][col];
            for k in col..n {
                m[row][k] -= factor * m[col][k];
            }
        }
    }
    det
}

pub fn inverse(m: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m.into_iter().enumerate().map(|(i, mut row)| {
        row.extend((0..n).map(|j| if i == j { 1.0 } else { 0.0 }));
        row
    }).collect();
    for col in 0..n {
        let pivot = (col..n).max_by(|&x, &y| a[x][col].abs().partial_cmp(&a[y][col].abs()).unwrap_or(std::cmp::Ordering::Equal))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(pivot, col);
        let p = a[col][col];
        for k in 0..2 * n {
            a[col][k] /= p;
        }
        for row in 0..n {
            if row != col {
                let factor = a[row][col];
                for k in 0..2 * n {
                    a[row][k] -= factor * a[col][k];
                }
            }
        }
    }
    Some(a.into_iter().map(|row| row[n..].to_vec()).collect())
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn math_functions_answer_what_excel_answers() {
        check(&[
            ("SUM(1,2,3)", "6"),
            ("SUM(\"3\",TRUE)", "4"),
            ("ROUND(2.675,2)", "2.68"),
            ("ROUND(-2.5,0)", "-3"),
            ("ROUND(1234.5678,-2)", "1200"),
            ("ROUNDUP(3.2,0)", "4"),
            ("ROUNDDOWN(-3.9,0)", "-3"),
            ("MOD(-3,2)", "1"),
            ("MOD(3,-2)", "-1"),
            ("MOD(5,0)", "#DIV/0!"),
            ("INT(-2.5)", "-3"),
            ("TRUNC(-2.5)", "-2"),
            ("CEILING(2.5,1)", "3"),
            ("CEILING(-2.5,-2)", "-4"),
            ("CEILING(-2.5,2)", "-2"),
            ("FLOOR(2.5,2)", "2"),
            ("CEILING.MATH(-5.5,2,1)", "-6"),
            ("MROUND(10,3)", "9"),
            ("EVEN(1.5)", "2"),
            ("ODD(-2)", "-3"),
            ("FACT(5)", "120"),
            ("COMBIN(8,2)", "28"),
            ("PERMUT(5,2)", "20"),
            ("GCD(24,36)", "12"),
            ("LCM(4,6)", "12"),
            ("POWER(2,10)", "1024"),
            ("SQRT(-1)", "#NUM!"),
            ("LOG(8,2)", "3"),
            ("ATAN2(1,1)", "0.785398163397"),
            ("ROMAN(1999)", "MCMXCIX"),
            ("ARABIC(\"MCMXCIX\")", "1999"),
            ("BASE(255,16)", "FF"),
            ("DECIMAL(\"FF\",16)", "255"),
            ("SUMPRODUCT({1,2,3},{4,5,6})", "32"),
            ("SUMSQ(3,4)", "25"),
            ("MDETERM({1,2;3,4})", "-2"),
            ("QUOTIENT(7,2)", "3"),
            ("-2^2", "4"),
            ("2^-1", "0.5"),
            ("(-8)^(1/3)", "-2"),
            ("0^0", "#NUM!"),
            ("10/0", "#DIV/0!"),
            ("\"2\"+\"3\"", "5"),
            ("\"a\"+1", "#VALUE!"),
            ("50%*4", "2"),
        ]);
    }
}
