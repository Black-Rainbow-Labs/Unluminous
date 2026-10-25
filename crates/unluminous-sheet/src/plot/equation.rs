//! The text Excel shows for a trendline's equation and R squared.

use crate::chartspec::TrendKind;

/// A coefficient as Excel's trendline label shows it: five significant digits, and exponent form for very
/// large and very small numbers.
pub fn coefficient(v: f64) -> String {
    if v == 0.0 {
        return "0".to_owned();
    }
    let magnitude = v.abs().log10().floor() as i32;
    if !(-5..=9).contains(&magnitude) {
        let text = format!("{:.4e}", v);
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = if mantissa.contains('.') { mantissa.trim_end_matches('0').trim_end_matches('.') } else { mantissa };
        let exp: i32 = exponent.parse().unwrap_or(0);
        return format!("{mantissa}E{}{:02}", if exp < 0 { "-" } else { "+" }, exp.abs());
    }
    let decimals = (4 - magnitude).clamp(0, 15) as usize;
    let text = format!("{v:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
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
            let slope = if c[0] == 1.0 { "x".to_owned() } else if c[0] == -1.0 { "-x".to_owned() } else { format!("{}x", coefficient(c[0])) };
            if c[1] == 0.0 {
                format!("y = {slope}")
            } else {
                format!("y = {slope}{}", signed(c[1], ""))
            }
        }
        TrendKind::Exponential => format!("y = {}e{}x", coefficient(c[0]), coefficient(c[1])),
        TrendKind::Logarithmic => format!("y = {}ln(x){}", coefficient(c[0]), signed(c[1], "")),
        TrendKind::Power => format!("y = {}x{}", coefficient(c[0]), caret(c[1])),
        TrendKind::Polynomial(order) => {
            let mut out = String::from("y = ");
            for (i, k) in c.iter().enumerate() {
                let power = order as usize - i;
                let term = match power {
                    0 => String::new(),
                    1 => "x".to_owned(),
                    p => format!("x{}", superscript(p)),
                };
                if i == 0 {
                    out.push_str(&format!("{}{term}", coefficient(*k)));
                } else {
                    out.push_str(&signed(*k, &term));
                }
            }
            out
        }
        TrendKind::MovingAverage(_) => String::new(),
    }
}

fn caret(v: f64) -> String {
    superscript_number(v)
}

fn superscript_number(v: f64) -> String {
    coefficient(v).chars().map(sup).collect()
}

fn superscript(p: usize) -> String {
    p.to_string().chars().map(sup).collect()
}

fn sup(c: char) -> char {
    match c {
        '0' => '\u{2070}',
        '1' => '\u{B9}',
        '2' => '\u{B2}',
        '3' => '\u{B3}',
        '4' => '\u{2074}',
        '5' => '\u{2075}',
        '6' => '\u{2076}',
        '7' => '\u{2077}',
        '8' => '\u{2078}',
        '9' => '\u{2079}',
        '-' => '\u{207B}',
        '.' => '\u{B7}',
        other => other,
    }
}

/// The R squared line: `R² = 0.9876`.
pub fn r_squared(r2: f64) -> String {
    format!("R\u{B2} = {}", coefficient(r2))
}
