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
        Value::Number(n) if *n < 0.0 || n.fract() != 0.0 || *n >= 1e10 => {
            return Err(ErrorKind::Num)
        }
        Value::Number(n) => format!("{}", *n as i64),
        Value::Error(e) => return Err(*e),
        Value::Bool(_) => return Err(ErrorKind::Value),
        other => other.to_text(),
    };
    // Only the digits of the base: a sign or a space is refused, as Excel refuses them.
    let text = text.as_str();
    if text.len() > 10 || !text.chars().all(|c| c.is_digit(base)) {
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
    if !(0.0..281_474_976_710_656.0).contains(&x) || x.fract() != 0.0 {
        return Err(ErrorKind::Num);
    }
    // Nothing shifted is nothing, however far; otherwise Excel refuses a shift of more than 53 bits.
    if x == 0.0 {
        return Ok(Value::Number(0.0));
    }
    if by.abs() > 53.0 {
        return Err(ErrorKind::Num);
    }
    let by = if left { by } else { -by };
    // Excel gives 0 for a shift left of 49 to 53 bits, where the result is over the largest it allows.
    let out = if by >= 49.0 {
        0
    } else if by >= 0.0 {
        (x as u64) << by as u32
    } else {
        (x as u64) >> (-by) as u32
    };
    if out as f64 >= 281_474_976_710_656.0 {
        return Err(ErrorKind::Num);
    }
    Ok(Value::Number(out as f64))
}

/// What a unit measures. Units of different kinds cannot be converted into each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mass,
    Length,
    Time,
    Pressure,
    Force,
    Energy,
    Power,
    Magnetism,
    Volume,
    Area,
    Speed,
    Information,
}

/// Which prefixes a unit takes. A squared or cubed unit takes them squared or cubed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prefixes {
    No,
    /// The metric prefixes, raised to this power.
    Metric(i32),
    /// The metric prefixes and the binary ones, `ki` and the rest.
    Binary,
}

use Kind::*;
use Prefixes::{Binary, Metric, No};

/// Units as Excel's CONVERT has them: the name, what it measures, how many of the base unit of that kind
/// it is (gram, metre, second, pascal, newton, joule, watt, tesla, litre, square metre, bit, metre a
/// second) and which prefixes it takes. The factors are the ones Excel answers with, read from Excel.
#[allow(clippy::excessive_precision)]
const UNITS: &[(&str, Kind, f64, Prefixes)] = &[
    ("g", Mass, 1.0, Metric(1)),
    ("sg", Mass, 14593.902937206363, No),
    ("lbm", Mass, 453.59237, No),
    ("u", Mass, 1.660538782e-24, Metric(1)),
    ("ozm", Mass, 28.349523125, No),
    ("grain", Mass, 0.06479891, No),
    ("cwt", Mass, 45359.237, No),
    ("shweight", Mass, 45359.237, No),
    ("uk_cwt", Mass, 50802.34544, No),
    ("lcwt", Mass, 50802.34544, No),
    ("hweight", Mass, 50802.34544, No),
    ("stone", Mass, 6350.29318, No),
    ("ton", Mass, 907184.74, No),
    ("uk_ton", Mass, 1016046.9088, No),
    ("LTON", Mass, 1016046.9088, No),
    ("m", Length, 1.0, Metric(1)),
    ("mi", Length, 1609.344, No),
    ("Nmi", Length, 1852.0, No),
    ("in", Length, 0.0254, No),
    ("ft", Length, 0.3048, No),
    ("yd", Length, 0.9144, No),
    ("ang", Length, 1e-10, Metric(1)),
    ("ell", Length, 1.143, No),
    ("ly", Length, 9460730472580800.0, Metric(1)),
    ("parsec", Length, 30856775812815532.0, Metric(1)),
    ("pc", Length, 30856775812815532.0, Metric(1)),
    ("Pica", Length, 0.0003527777777777778, No),
    ("pica", Length, 0.004233333333333334, No),
    ("Picapt", Length, 0.0003527777777777778, No),
    ("yr", Time, 31557600.0, No),
    ("day", Time, 86400.0, No),
    ("d", Time, 86400.0, No),
    ("hr", Time, 3600.0, No),
    ("mn", Time, 60.0, No),
    ("min", Time, 60.0, No),
    ("sec", Time, 1.0, Metric(1)),
    ("s", Time, 1.0, Metric(1)),
    ("Pa", Pressure, 1.0, Metric(1)),
    ("p", Pressure, 1.0, Metric(1)),
    ("atm", Pressure, 101325.0, Metric(1)),
    ("at", Pressure, 101325.0, Metric(1)),
    ("mmHg", Pressure, 133.322, Metric(1)),
    ("psi", Pressure, 6894.757293168362, No),
    ("Torr", Pressure, 133.32236842105263, No),
    ("N", Force, 1.0, Metric(1)),
    ("dyn", Force, 1e-05, Metric(1)),
    ("dy", Force, 1e-05, Metric(1)),
    ("lbf", Force, 4.4482216152605005, No),
    ("pond", Force, 0.00980665, Metric(1)),
    ("J", Energy, 1.0, Metric(1)),
    ("e", Energy, 1e-07, Metric(1)),
    ("c", Energy, 4.184, Metric(1)),
    ("cal", Energy, 4.1868, Metric(1)),
    ("eV", Energy, 1.602176487e-19, Metric(1)),
    ("ev", Energy, 1.602176487e-19, Metric(1)),
    ("HPh", Energy, 2684519.537696173, No),
    ("hh", Energy, 2684519.537696173, No),
    ("Wh", Energy, 3600.0, Metric(1)),
    ("wh", Energy, 3600.0, Metric(1)),
    ("flb", Energy, 1.3558179483314003, No),
    ("BTU", Energy, 1055.05585262, No),
    ("btu", Energy, 1055.05585262, No),
    ("HP", Power, 745.6998715822702, No),
    ("h", Power, 745.6998715822702, No),
    ("PS", Power, 735.49875, No),
    ("W", Power, 1.0, Metric(1)),
    ("w", Power, 1.0, Metric(1)),
    ("T", Magnetism, 1.0, Metric(1)),
    ("ga", Magnetism, 0.0001, Metric(1)),
    ("tsp", Volume, 0.00492892159375, No),
    ("tspm", Volume, 0.005, No),
    ("tbs", Volume, 0.01478676478125, No),
    ("oz", Volume, 0.0295735295625, No),
    ("cup", Volume, 0.2365882365, No),
    ("pt", Volume, 0.473176473, No),
    ("us_pt", Volume, 0.473176473, No),
    ("uk_pt", Volume, 0.56826125, Metric(1)),
    ("qt", Volume, 0.946352946, No),
    ("uk_qt", Volume, 1.1365225, No),
    ("gal", Volume, 3.785411784, No),
    ("uk_gal", Volume, 4.54609, No),
    ("l", Volume, 1.0, Metric(1)),
    ("L", Volume, 1.0, Metric(1)),
    ("lt", Volume, 1.0, Metric(1)),
    ("ang3", Volume, 1e-27, Metric(3)),
    ("ang^3", Volume, 1e-27, Metric(3)),
    ("barrel", Volume, 158.987294928, No),
    ("bushel", Volume, 35.23907016688, No),
    ("regton", Volume, 2831.6846592, No),
    ("GRT", Volume, 2831.6846592, No),
    ("ft3", Volume, 28.316846592, No),
    ("ft^3", Volume, 28.316846592, No),
    ("in3", Volume, 0.016387064, No),
    ("in^3", Volume, 0.016387064, No),
    ("ly3", Volume, 8.467866646237152e50, No),
    ("ly^3", Volume, 8.467866646237152e50, No),
    ("m3", Volume, 1000.0, Metric(3)),
    ("m^3", Volume, 1000.0, Metric(3)),
    ("mi3", Volume, 4168181825440.5796, No),
    ("mi^3", Volume, 4168181825440.5796, No),
    ("yd3", Volume, 764.554857984, No),
    ("yd^3", Volume, 764.554857984, No),
    ("Nmi3", Volume, 6352182208000.0, No),
    ("Nmi^3", Volume, 6352182208000.0, No),
    ("Pica3", Volume, 4.39039566186557e-08, No),
    ("Pica^3", Volume, 4.39039566186557e-08, No),
    ("Picapt3", Volume, 4.39039566186557e-08, No),
    ("Picapt^3", Volume, 4.39039566186557e-08, No),
    ("MTON", Volume, 1132.67386368, No),
    ("ar", Area, 100.0, Metric(1)),
    ("Morgen", Area, 2500.0, No),
    ("ang2", Area, 1e-20, Metric(2)),
    ("ang^2", Area, 1e-20, Metric(2)),
    ("ft2", Area, 0.09290304, No),
    ("ft^2", Area, 0.09290304, No),
    ("in2", Area, 0.00064516, No),
    ("in^2", Area, 0.00064516, No),
    ("ly2", Area, 8.95054210748189e31, No),
    ("ly^2", Area, 8.95054210748189e31, No),
    ("m2", Area, 1.0, Metric(2)),
    ("m^2", Area, 1.0, Metric(2)),
    ("mi2", Area, 2589988.110336, No),
    ("mi^2", Area, 2589988.110336, No),
    ("Nmi2", Area, 3429904.0, No),
    ("Nmi^2", Area, 3429904.0, No),
    ("Pica2", Area, 1.2445216049382715e-07, No),
    ("Pica^2", Area, 1.2445216049382715e-07, No),
    ("Picapt2", Area, 1.2445216049382715e-07, No),
    ("Picapt^2", Area, 1.2445216049382715e-07, No),
    ("yd2", Area, 0.83612736, No),
    ("yd^2", Area, 0.83612736, No),
    ("uk_acre", Area, 4046.8564224, No),
    ("us_acre", Area, 4046.8726098742522, No),
    ("ha", Area, 10000.0, No),
    ("bit", Information, 1.0, Binary),
    ("byte", Information, 8.0, Binary),
    ("admkn", Speed, 0.5147733333333333, No),
    ("kn", Speed, 0.5144444444444445, No),
    ("m/h", Speed, 0.0002777777777777778, Metric(1)),
    ("m/hr", Speed, 0.0002777777777777778, Metric(1)),
    ("m/s", Speed, 1.0, Metric(1)),
    ("m/sec", Speed, 1.0, Metric(1)),
    ("mph", Speed, 0.44704, Metric(1)),
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
    ("e", 1e1),
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
];

const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("ki", 1_024.0),
    ("Mi", 1_048_576.0),
    ("Gi", 1_073_741_824.0),
    ("Ti", 1_099_511_627_776.0),
    ("Pi", 1_125_899_906_842_624.0),
    ("Ei", 1_152_921_504_606_846_976.0),
    ("Zi", 1_180_591_620_717_411_303_424.0),
    ("Yi", 1_208_925_819_614_629_174_706_176.0),
];

/// Every way a name can be read as a unit: the unit itself, and a prefix on a unit that takes one. A name
/// can be two things, `pc` is a parsec and a pico calorie, and the conversion uses the reading that fits.
fn readings(name: &str) -> Vec<(Kind, f64)> {
    let mut found: Vec<(Kind, f64)> = UNITS
        .iter()
        .filter(|(unit, ..)| *unit == name)
        .map(|(_, kind, factor, _)| (*kind, *factor))
        .collect();
    let metric = PREFIXES.iter();
    let binary = BINARY_PREFIXES.iter();
    for (prefix, scale) in metric.chain(binary) {
        let Some(rest) = name.strip_prefix(prefix) else { continue };
        let binary_prefix = prefix.ends_with('i');
        for (_, kind, factor, takes) in UNITS.iter().filter(|(unit, ..)| *unit == rest) {
            match takes {
                Metric(power) if !binary_prefix => found.push((*kind, factor * scale.powi(*power))),
                Binary => found.push((*kind, factor * scale)),
                _ => {}
            }
        }
    }
    found
}

/// A temperature unit, with the metric prefix a kelvin may take: the unit's name and the scale.
fn temperature(name: &str) -> Option<(&'static str, f64)> {
    let plain = |n: &str| match n {
        "C" | "cel" => Some("C"),
        "F" | "fah" => Some("F"),
        "K" | "kel" => Some("K"),
        "Rank" => Some("Rank"),
        "Reau" => Some("Reau"),
        _ => None,
    };
    if let Some(unit) = plain(name) {
        return Some((unit, 1.0));
    }
    PREFIXES.iter().find_map(|(prefix, scale)| {
        let rest = name.strip_prefix(prefix)?;
        (rest == "K" || rest == "kel").then_some(("K", *scale))
    })
}

fn temperature_to_kelvin(x: f64, unit: &str) -> f64 {
    match unit {
        "C" => x + 273.15,
        "F" => (x - 32.0) * 5.0 / 9.0 + 273.15,
        "Rank" => x * 5.0 / 9.0,
        "Reau" => x * 1.25 + 273.15,
        _ => x,
    }
}

fn kelvin_to(x: f64, unit: &str) -> f64 {
    match unit {
        "C" => x - 273.15,
        "F" => (x - 273.15) * 9.0 / 5.0 + 32.0,
        "Rank" => x * 9.0 / 5.0,
        "Reau" => (x - 273.15) / 1.25,
        _ => x,
    }
}

pub fn convert_units(x: f64, from: &str, to: &str) -> Option<f64> {
    if let (Some((a, scale_a)), Some((b, scale_b))) = (temperature(from), temperature(to)) {
        return Some(kelvin_to(temperature_to_kelvin(x * scale_a, a), b) / scale_b);
    }
    let (from, to) = (readings(from), readings(to));
    from.iter().find_map(|(kind_a, factor_a)| {
        to.iter().find(|(kind_b, _)| kind_b == kind_a).map(|(_, factor_b)| x * factor_a / factor_b)
    })
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
