//! The text Excel shows for a trendline's equation and R squared.

use crate::chartspec::TrendKind;

/// A coefficient as Excel's trendline label shows it: five significant digits but at most four decimals,
/// trailing zeros dropped, and exponent form for very large and very small numbers.
pub fn coefficient(v: f64) -> String {
    coefficient_with(v, 0)
}

/// A coefficient that carries its own minus sign, which Excel gives one digit fewer (the label has room for
/// six characters), except for the leading coefficient of a polynomial.
fn leading(v: f64) -> String {
    coefficient_with(v, usize::from(v < 0.0))
}

/// `coefficient` with `fewer` decimals taken off.
fn coefficient_with(v: f64, fewer: usize) -> String {
    if v == 0.0 {
        return "0".to_owned();
    }
    let magnitude = v.abs().log10().floor() as i32;
    if !(-4..=5).contains(&magnitude) {
        // Very large and very small numbers are shown as one digit and an exponent.
        let text = format!("{:.0e}", v);
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exp: i32 = exponent.parse().unwrap_or(0);
        return format!("{mantissa}E{}{:02}", if exp < 0 { "-" } else { "+" }, exp.abs());
    }
    let decimals = ((4 - magnitude).clamp(0, 4) as usize).saturating_sub(fewer);
    let text = rounded_half_away(v, decimals);
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// `v` with `decimals` decimals, rounding a tie away from zero in the 15 significant digits Excel keeps, so
/// that 20.8865 gives 20.887 even though its nearest binary value is a little below.
fn rounded_half_away(v: f64, decimals: usize) -> String {
    let scientific = format!("{:.14e}", v.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let mut digits: Vec<u8> =
        mantissa.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
    // The digit at index k has the value 10^(exponent - k); keep indexes up to the last wanted decimal.
    let keep = exponent + 1 + decimals as i32;
    if keep < 0 {
        return format!("{:.*}", decimals, 0.0);
    }
    let keep = keep as usize;
    if keep < digits.len() {
        let round_up = digits[keep] >= 5;
        digits.truncate(keep);
        if round_up {
            let mut i = keep;
            loop {
                if i == 0 {
                    digits.insert(0, 1);
                    break;
                }
                i -= 1;
                if digits[i] == 9 {
                    digits[i] = 0;
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
    }
    let carried = digits.len() as i32 - keep as i32;
    let point = exponent + 1 + carried;
    let mut whole = String::new();
    let mut fraction = String::new();
    for (i, d) in digits.iter().enumerate() {
        if (i as i32) < point {
            whole.push((b'0' + d) as char);
        } else {
            fraction.push((b'0' + d) as char);
        }
    }
    while (whole.len() as i32) < point {
        whole.push('0');
    }
    if whole.is_empty() {
        whole.push('0');
    }
    while fraction.len() < decimals {
        fraction.insert(0, '0');
    }
    let sign = if v < 0.0 { "-" } else { "" };
    if decimals == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

fn signed(v: f64, term: &str) -> String {
    let sign = if v < 0.0 { "- " } else { "+ " };
    format!(" {sign}{}{term}", coefficient(v.abs()))
}

/// The equation of a fitted curve: `y = 2x + 1`.
pub fn text(kind: TrendKind, c: &[f64], _intercept: Option<f64>) -> String {
    match kind {
        TrendKind::Linear => {
            let slope = if c[0] == 1.0 {
                "x".to_owned()
            } else if c[0] == -1.0 {
                "-x".to_owned()
            } else {
                format!("{}x", coefficient(c[0]))
            };
            if c[1] == 0.0 {
                format!("y = {slope}")
            } else {
                format!("y = {slope}{}", signed(c[1], ""))
            }
        }
        TrendKind::Exponential => format!("y = {}e{}x", leading(c[0]), leading(c[1])),
        TrendKind::Logarithmic => format!("y = {}ln(x){}", leading(c[0]), signed(c[1], "")),
        TrendKind::Power => format!(
            "y = {}x{}",
            if (c[0] - 1.0).abs() < 1e-9 { String::new() } else { leading(c[0]) },
            caret_of(c[1])
        ),
        TrendKind::Polynomial(order) => {
            let _ = order;
            let order = c.len().saturating_sub(1);
            let mut out = String::from("y = ");
            for (i, k) in c.iter().enumerate() {
                let power = order - i;
                let term = match power {
                    0 => String::new(),
                    1 => "x".to_owned(),
                    p => format!("x{}", superscript(p)),
                };
                if i == 0 {
                    let shown = if (*k - 1.0).abs() < 1e-9 && !term.is_empty() {
                        String::new()
                    } else {
                        coefficient(*k)
                    };
                    out.push_str(&format!("{shown}{term}"));
                } else {
                    out.push_str(&signed(*k, &term));
                }
            }
            out
        }
        TrendKind::MovingAverage(_) => String::new(),
    }
}

/// An exponent as Excel's label text has it: the digits on the line after the `x`.
fn caret_of(v: f64) -> String {
    leading(v)
}

fn superscript(p: usize) -> String {
    p.to_string()
}

/// The R squared line: `R² = 0.9876`.
pub fn r_squared(r2: f64) -> String {
    format!("R\u{B2} = {}", coefficient(r2))
}
