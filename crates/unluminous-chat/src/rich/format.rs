//! How a number a component shows is written: a calculator's outputs, a chart's axis, a stat.
//!
//! Here rather than in the window because it is the kind of thing that is wrong in a way only a test
//! notices: `3600` written `3600.0000000000005`, a money sum without its separators, a percentage
//! written `0.05%` when the model meant five.

/// The formats an output may ask for.
pub const FORMATS: &[&str] = &["number", "integer", "money", "percent", "compact"];

/// A number written plainly: no trailing zeros, at most four decimals, no separators.
///
/// This is how a number in the JSON is turned into text for a table cell or a stat, where the model
/// already chose how it is written.
pub fn plain(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Thousands separators on the whole part of `text`, which is a number written with `.` decimals.
fn grouped(text: &str) -> String {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let (whole, fraction) = match rest.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (rest, None),
    };
    let mut out = String::new();
    for (at, digit) in whole.chars().enumerate() {
        if at > 0 && (whole.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    match fraction {
        Some(fraction) => format!("{sign}{out}.{fraction}"),
        None => format!("{sign}{out}"),
    }
}

/// Whether `unit` goes in front of the number, which is what a currency symbol does.
fn leads(unit: &str) -> bool {
    matches!(unit, "$" | "£" | "€" | "¥" | "₹")
}

/// `value` written in `format` with `unit`.
///
/// `percent` takes the value as already a percentage: `5` is `5%`. A model writing a rate as `0.05`
/// and asking for `percent` gets `0.05%`, which is visibly wrong rather than silently multiplied.
pub fn number(value: f64, format: &str, unit: &str) -> String {
    let body = match format {
        "integer" => grouped(&format!("{:.0}", value.round())),
        "money" => grouped(&format!("{value:.2}")),
        "percent" => format!("{}%", grouped(&trimmed(value, 1))),
        "compact" => compact(value),
        _ => grouped(&trimmed(value, if value.abs() >= 100.0 { 0 } else { 2 })),
    };
    if unit.is_empty() || format == "percent" && unit == "%" {
        return body;
    }
    match leads(unit) {
        true => match body.strip_prefix('-') {
            Some(positive) => format!("-{unit}{positive}"),
            None => format!("{unit}{body}"),
        },
        false if unit == "%" => format!("{body}%"),
        false => format!("{body} {unit}"),
    }
}

/// `value` with at most `digits` decimals and no trailing zeros.
fn trimmed(value: f64, digits: usize) -> String {
    let text = format!("{value:.digits$}");
    match text.contains('.') {
        true => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        false => text,
    }
}

/// A large number in a few characters: `12.3k`, `4.5M`.
fn compact(value: f64) -> String {
    let size = value.abs();
    let (scaled, suffix) = match size {
        s if s >= 1e12 => (value / 1e12, "T"),
        s if s >= 1e9 => (value / 1e9, "B"),
        s if s >= 1e6 => (value / 1e6, "M"),
        s if s >= 1e3 => (value / 1e3, "k"),
        _ => (value, ""),
    };
    format!("{}{suffix}", trimmed(scaled, if scaled.abs() >= 100.0 { 0 } else { 1 }))
}

/// Round numbers to put gridlines at between `low` and `high`: about `count` of them, each a 1, 2 or 5
/// times a power of ten apart, and always including the top of the data.
pub fn ticks(low: f64, high: f64, count: usize) -> Vec<f64> {
    let (low, high) = match (low.is_finite(), high.is_finite()) {
        (true, true) if high > low => (low, high),
        (true, true) => (low.min(0.0), low.max(0.0) + 1.0),
        _ => (0.0, 1.0),
    };
    let rough = (high - low) / count.max(1) as f64;
    let power = 10f64.powf(rough.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .map(|factor| factor * power)
        .find(|step| *step >= rough)
        .unwrap_or(power * 10.0);
    let first = (low / step).floor() * step;
    let mut out = Vec::new();
    let mut at = first;
    while at < high + step * 0.999 && out.len() < 50 {
        out.push((at / step).round() * step);
        at += step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_written_the_way_its_format_says() {
        assert_eq!(number(3600.000_000_000_5, "number", ""), "3,600");
        assert_eq!(number(1234567.891, "money", "$"), "$1,234,567.89");
        assert_eq!(number(-12.5, "money", "$"), "-$12.50");
        assert_eq!(number(5.0, "percent", ""), "5%");
        assert_eq!(number(12345.0, "compact", ""), "12.3k");
        assert_eq!(number(42.0, "number", "ms"), "42 ms");
        assert_eq!(number(2.5, "integer", ""), "3");
        assert_eq!(number(0.126, "number", ""), "0.13");
    }

    #[test]
    fn a_plain_number_loses_its_needless_zeros() {
        assert_eq!(plain(41.0), "41");
        assert_eq!(plain(2.50), "2.5");
        assert_eq!(plain(-0.3333333), "-0.3333");
    }

    #[test]
    fn gridlines_land_on_round_numbers_and_cover_the_data() {
        assert_eq!(ticks(0.0, 212.0, 5), vec![0.0, 50.0, 100.0, 150.0, 200.0, 250.0]);
        assert_eq!(ticks(0.0, 1.0, 4), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        let flat = ticks(5.0, 5.0, 5);
        assert!(flat.first().copied().unwrap() <= 0.0 && *flat.last().unwrap() >= 5.0, "{flat:?}");
    }
}
