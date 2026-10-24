//! Engineering functions: number bases, bitwise operations and unit conversion.

use super::{num, number, opt, r, s, text_of, Function};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("DEC2BIN", 1, 2, "number, [places]", "A decimal number in binary.", |ctx, a| {
        strict(ctx, a, |ctx, a| from_decimal(ctx, a, 2))
    }),
    r("DEC2OCT", 1, 2, "number, [places]", "A decimal number in octal.", |ctx, a| {
        strict(ctx, a, |ctx, a| from_decimal(ctx, a, 8))
    }),
    r("DEC2HEX", 1, 2, "number, [places]", "A decimal number in hexadecimal.", |ctx, a| {
        strict(ctx, a, |ctx, a| from_decimal(ctx, a, 16))
    }),
    r("BIN2DEC", 1, 1, "number", "A binary number in decimal.", |ctx, a| {
        strict(ctx, a, |_, a| to_decimal(&a[0], 2).map(Value::Number))
    }),
    r("OCT2DEC", 1, 1, "number", "An octal number in decimal.", |ctx, a| {
        strict(ctx, a, |_, a| to_decimal(&a[0], 8).map(Value::Number))
    }),
    r("HEX2DEC", 1, 1, "number", "A hexadecimal number in decimal.", |ctx, a| {
        strict(ctx, a, |_, a| to_decimal(&a[0], 16).map(Value::Number))
    }),
    r("BIN2OCT", 1, 2, "number, [places]", "A binary number in octal.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 2, 8))
    }),
    r("BIN2HEX", 1, 2, "number, [places]", "A binary number in hexadecimal.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 2, 16))
    }),
    r("OCT2BIN", 1, 2, "number, [places]", "An octal number in binary.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 8, 2))
    }),
    r("OCT2HEX", 1, 2, "number, [places]", "An octal number in hexadecimal.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 8, 16))
    }),
    r("HEX2BIN", 1, 2, "number, [places]", "A hexadecimal number in binary.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 16, 2))
    }),
    r("HEX2OCT", 1, 2, "number, [places]", "A hexadecimal number in octal.", |ctx, a| {
        strict(ctx, a, |ctx, a| convert(ctx, a, 16, 8))
    }),
    s("BITAND", 2, 2, "number1, number2", "Bitwise AND.", |ctx, a| bits(ctx, a, |x, y| x & y)),
    s("BITOR", 2, 2, "number1, number2", "Bitwise OR.", |ctx, a| bits(ctx, a, |x, y| x | y)),
    s("BITXOR", 2, 2, "number1, number2", "Bitwise exclusive OR.", |ctx, a| {
        bits(ctx, a, |x, y| x ^ y)
    }),
    s("BITLSHIFT", 2, 2, "number, shift_amount", "A number shifted left by bits.", |ctx, a| {
        shift(ctx, a, true)
    }),
    s("BITRSHIFT", 2, 2, "number, shift_amount", "A number shifted right by bits.", |ctx, a| {
        shift(ctx, a, false)
    }),
    s("DELTA", 1, 2, "number1, [number2]", "1 when two numbers are equal.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let y = super::opt_num(ctx, a, 1, 0.0)?;
        Ok(Value::Number(if x == y { 1.0 } else { 0.0 }))
    }),
    s("GESTEP", 1, 2, "number, [step]", "1 when a number is at least a step.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let y = super::opt_num(ctx, a, 1, 0.0)?;
        Ok(Value::Number(if x >= y { 1.0 } else { 0.0 }))
    }),
    s("CONVERT", 3, 3, "number, from_unit, to_unit", "A measurement in other units.", |ctx, a| {
        let x = num(ctx, &a[0])?;
        let from = text_of(&a[1])?;
        let to = text_of(&a[2])?;
        number(convert_units(x, &from, &to).ok_or(ErrorKind::NA)?)
    }),
];

/// Run one of the conversion or error functions the way Excel does: each argument must be a single
/// value, so a range or array of several cells is `#VALUE!`, and a logical is `#VALUE!` too.
pub fn strict(
    ctx: &mut Ctx,
    args: &[Expr],
    f: fn(&mut Ctx, &[Value]) -> Result<Value, ErrorKind>,
) -> Val {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        let given = ctx.eval(arg);
        if ctx.dims(&given) != Some((1, 1)) {
            return Val::error(ErrorKind::Value);
        }
        let value = ctx.single(given);
        if matches!(value, Value::Bool(_)) {
            return Val::error(ErrorKind::Value);
        }
        values.push(value);
    }
    match f(ctx, &values) {
        Ok(v) => Val::Value(v),
        Err(e) => Val::error(e),
    }
}

const LIMIT: [(u32, f64); 3] = [(2, 512.0), (8, 536_870_912.0), (16, 549_755_813_888.0)];

fn limit(base: u32) -> f64 {
    LIMIT.iter().find(|(b, _)| *b == base).map_or(0.0, |(_, l)| *l)
}

fn from_decimal(ctx: &mut Ctx, a: &[Value], base: u32) -> Result<Value, ErrorKind> {
    let n = num(ctx, &a[0])?.trunc();
    let l = limit(base);
    if n < -l || n >= l {
        return Err(ErrorKind::Num);
    }
    let places = match opt(a, 1) {
        Some(v) => Some(num(ctx, v)?.trunc()),
        None => None,
    };
    digits(n as i64, base, places)
}

/// A number written in a base, negative numbers in ten digit two's complement, padded to `places`.
fn digits(n: i64, base: u32, places: Option<f64>) -> Result<Value, ErrorKind> {
    let text = if n < 0 {
        let modulus = (base as i64).pow(10);
        to_base((modulus + n) as u64, base)
    } else {
        to_base(n as u64, base)
    };
    if let Some(p) = places.filter(|_| n >= 0) {
        if !(0.0..=10.0).contains(&p) || (p as usize) < text.len() {
            return Err(ErrorKind::Num);
        }
        return Ok(Value::text(format!("{:0>width$}", text, width = p as usize)));
    }
    Ok(Value::text(text))
}

fn to_base(mut n: u64, base: u32) -> String {
    if n == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(
            std::char::from_digit((n % base as u64) as u32, base)
                .unwrap_or('0')
                .to_ascii_uppercase(),
        );
        n /= base as u64;
    }
    out.iter().rev().collect()
}

fn to_decimal(v: &Value, base: u32) -> Result<f64, ErrorKind> {
    let text = match v {
        Value::Number(n) => format!("{}", *n as i64),
        Value::Error(e) => return Err(*e),
        other => other.to_text(),
    };
    let text = text.trim();
    if text.len() > 10 {
        return Err(ErrorKind::Num);
    }
    if text.is_empty() {
        return Ok(0.0);
    }
    let n = i64::from_str_radix(text, base).map_err(|_| ErrorKind::Num)?;
    // Ten digits with the top one set is negative in two's complement.
    if text.len() == 10 {
        let modulus = (base as i64).pow(10);
        if n >= modulus / 2 {
            return Ok((n - modulus) as f64);
        }
    }
    Ok(n as f64)
}

fn convert(ctx: &mut Ctx, a: &[Value], from: u32, to: u32) -> Result<Value, ErrorKind> {
    let n = to_decimal(&a[0], from)?;
    let l = limit(to);
    if n < -l || n >= l {
        return Err(ErrorKind::Num);
    }
    let places = match opt(a, 1) {
        Some(v) => Some(num(ctx, v)?.trunc()),
        None => None,
    };
    digits(n as i64, to, places)
}

fn bits(ctx: &mut Ctx, a: &[Value], f: fn(u64, u64) -> u64) -> Result<Value, ErrorKind> {
    let (x, y) = (num(ctx, &a[0])?, num(ctx, &a[1])?);
    let ok = |v: f64| (0.0..281_474_976_710_656.0).contains(&v) && v.fract() == 0.0;
    if !ok(x) || !ok(y) {
        return Err(ErrorKind::Num);
    }
    Ok(Value::Number(f(x as u64, y as u64) as f64))
}

fn shift(ctx: &mut Ctx, a: &[Value], left: bool) -> Result<Value, ErrorKind> {
    let (x, by) = (num(ctx, &a[0])?, num(ctx, &a[1])?.trunc());
    if !(0.0..281_474_976_710_656.0).contains(&x) || x.fract() != 0.0 || by.abs() > 53.0 {
        return Err(ErrorKind::Num);
    }
    let by = if left { by } else { -by };
    let out = if by >= 0.0 { (x as u64) << by as u32 } else { (x as u64) >> (-by) as u32 };
    if out as f64 >= 281_474_976_710_656.0 {
        return Err(ErrorKind::Num);
    }
    Ok(Value::Number(out as f64))
}

/// Units: name, kind, and the factor to the kind's base unit. Temperature is handled apart.
const UNITS: &[(&str, &str, f64)] = &[
    ("g", "mass", 1.0),
    ("lbm", "mass", 453.59237),
    ("ozm", "mass", 28.349523125),
    ("stone", "mass", 6350.29318),
    ("ton", "mass", 907_184.74),
    ("u", "mass", 1.660_539_040e-24),
    ("m", "length", 1.0),
    ("mi", "length", 1609.344),
    ("Nmi", "length", 1852.0),
    ("in", "length", 0.0254),
    ("ft", "length", 0.3048),
    ("yd", "length", 0.9144),
    ("ang", "length", 1e-10),
    ("ly", "length", 9_460_730_472_580_800.0),
    ("Pica", "length", 0.0254 / 6.0),
    ("pica", "length", 0.0254 / 72.0),
    ("yr", "time", 31_557_600.0),
    ("day", "time", 86_400.0),
    ("d", "time", 86_400.0),
    ("hr", "time", 3600.0),
    ("mn", "time", 60.0),
    ("min", "time", 60.0),
    ("sec", "time", 1.0),
    ("s", "time", 1.0),
    ("Pa", "pressure", 1.0),
    ("p", "pressure", 1.0),
    ("atm", "pressure", 101_325.0),
    ("at", "pressure", 101_325.0),
    ("mmHg", "pressure", 133.322),
    ("psi", "pressure", 6894.757_293_168),
    ("N", "force", 1.0),
    ("dyn", "force", 1e-5),
    ("lbf", "force", 4.448_221_615_260_5),
    ("J", "energy", 1.0),
    ("e", "energy", 1e-7),
    ("cal", "energy", 4.1868),
    ("eV", "energy", 1.602_176_620_8e-19),
    ("HPh", "energy", 2_684_519.537_696_17),
    ("Wh", "energy", 3600.0),
    ("BTU", "energy", 1055.055_852_62),
    ("W", "power", 1.0),
    ("w", "power", 1.0),
    ("HP", "power", 745.699_871_582_270_2),
    ("PS", "power", 735.49875),
    ("T", "magnetism", 1.0),
    ("ga", "magnetism", 1e-4),
    ("l", "volume", 0.001),
    ("L", "volume", 0.001),
    ("lt", "volume", 0.001),
    ("m3", "volume", 1.0),
    ("tsp", "volume", 4.928_921_593_75e-6),
    ("tbs", "volume", 1.478_676_478_125e-5),
    ("oz", "volume", 2.957_352_956_25e-5),
    ("cup", "volume", 2.365_882_365e-4),
    ("pt", "volume", 4.731_764_73e-4),
    ("us_pt", "volume", 4.731_764_73e-4),
    ("qt", "volume", 9.463_529_46e-4),
    ("gal", "volume", 0.003_785_411_784),
    ("m2", "area", 1.0),
    ("ha", "area", 10_000.0),
    ("uk_acre", "area", 4046.856_422_4),
    ("us_acre", "area", 4046.872_609_874_252),
    ("ft2", "area", 0.092_903_04),
    ("in2", "area", 0.000_645_16),
    ("mi2", "area", 2_589_988.110_336),
    ("m/s", "speed", 1.0),
    ("m/sec", "speed", 1.0),
    ("m/h", "speed", 1.0 / 3600.0),
    ("m/hr", "speed", 1.0 / 3600.0),
    ("mph", "speed", 0.447_04),
    ("kn", "speed", 0.514_444_444_444_444_4),
    ("admkn", "speed", 0.514_773_333_333_333_3),
    ("bit", "information", 1.0),
    ("byte", "information", 8.0),
];

const PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24),
    ("Z", 1e21),
    ("E", 1e18),
    ("P", 1e15),
    ("T", 1e12),
    ("G", 1e9),
    ("M", 1e6),
    ("k", 1e3),
    ("h", 1e2),
    ("da", 1e1),
    ("d", 1e-1),
    ("c", 1e-2),
    ("m", 1e-3),
    ("u", 1e-6),
    ("n", 1e-9),
    ("p", 1e-12),
    ("f", 1e-15),
    ("a", 1e-18),
    ("z", 1e-21),
    ("y", 1e-24),
    ("ki", 1024.0),
    ("Mi", 1_048_576.0),
    ("Gi", 1_073_741_824.0),
    ("Ti", 1_099_511_627_776.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("Zi", 1_180_591_620_717_411_303_424.0),
    ("Yi", 1_208_925_819_614_629_174_706_176.0),
];

/// A unit's kind and factor, reading a metric prefix where the unit itself is not known.
fn unit(name: &str) -> Option<(&'static str, f64)> {
    if let Some((_, kind, factor)) = UNITS.iter().find(|(n, ..)| *n == name) {
        return Some((kind, *factor));
    }
    for (prefix, scale) in PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix) {
            if let Some((_, kind, factor)) = UNITS.iter().find(|(n, ..)| *n == rest) {
                // Squared and cubed units take the prefix squared or cubed.
                let power = if rest.ends_with('2') {
                    2
                } else if rest.ends_with('3') {
                    3
                } else {
                    1
                };
                return Some((kind, factor * scale.powi(power)));
            }
        }
    }
    None
}

fn temperature_to_kelvin(x: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "C" | "cel" => x + 273.15,
        "F" | "fah" => (x - 32.0) * 5.0 / 9.0 + 273.15,
        "K" | "kel" => x,
        "Rank" => x * 5.0 / 9.0,
        "Reau" => x * 1.25 + 273.15,
        _ => return None,
    })
}

fn kelvin_to(x: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "C" | "cel" => x - 273.15,
        "F" | "fah" => (x - 273.15) * 9.0 / 5.0 + 32.0,
        "K" | "kel" => x,
        "Rank" => x * 9.0 / 5.0,
        "Reau" => (x - 273.15) / 1.25,
        _ => return None,
    })
}

pub fn convert_units(x: f64, from: &str, to: &str) -> Option<f64> {
    if let (Some(k), Some(_)) = (temperature_to_kelvin(x, from), kelvin_to(0.0, to)) {
        return kelvin_to(k, to);
    }
    let (kind_a, factor_a) = unit(from)?;
    let (kind_b, factor_b) = unit(to)?;
    (kind_a == kind_b).then(|| x * factor_a / factor_b)
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn engineering_answers_what_excel_answers() {
        check(&[
            ("DEC2BIN(9)", "1001"),
            ("DEC2BIN(9,8)", "00001001"),
            ("DEC2BIN(-100)", "1110011100"),
            ("DEC2HEX(255)", "FF"),
            ("DEC2HEX(-54)", "FFFFFFFFCA"),
            ("BIN2DEC(\"1111111111\")", "-1"),
            ("HEX2DEC(\"A5\")", "165"),
            ("OCT2DEC(\"7777777533\")", "-165"),
            ("BIN2HEX(\"11111011\",4)", "00FB"),
            ("BITAND(13,25)", "9"),
            ("BITOR(23,10)", "31"),
            ("BITXOR(5,3)", "6"),
            ("BITLSHIFT(4,2)", "16"),
            ("CONVERT(1,\"lbm\",\"kg\")", "0.45359237"),
            ("CONVERT(68,\"F\",\"C\")", "20"),
            ("CONVERT(1,\"mi\",\"km\")", "1.609344"),
            ("CONVERT(1,\"m\",\"lbm\")", "#N/A"),
            ("DELTA(5,5)", "1"),
            ("GESTEP(5,4)", "1"),
        ]);
    }
}
