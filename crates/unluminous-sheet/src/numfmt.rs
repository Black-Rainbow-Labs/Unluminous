//! Number formats: a format code such as `#,##0.00;[Red](#,##0.00)` or `d-mmm-yy h:mm`, and showing a
//! value with one.
//!
//! A code has up to four sections, for positive numbers, negative numbers, zero and text, or up to three
//! with conditions in brackets. A section is either a number section (digit placeholders `0 # ?`, a decimal
//! point, thousands separators, percent, an exponent, a fraction), a date and time section, or a text
//! section with `@`. Literal text is quoted or escaped, `_x` leaves the width of `x` as space, `*x` repeats
//! `x` to fill the cell, and `[Red]` colours the result.

use std::sync::Arc;

use crate::dates;
use crate::value::{round_significant, significant_digits, ErrorKind, Value};

/// A value shown with a format.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub text: String,
    /// A colour the section names: `Red`, `Blue`, or `Color12`.
    pub color: Option<Arc<str>>,
    /// Numbers and dates are right aligned by default, text left.
    pub numeric: bool,
    /// The character `*` asked to repeat, and where in `text` the repetition goes.
    pub fill: Option<(char, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Literal(String),
    /// `_x`: a space the width of a character.
    Space,
    Fill(char),
    Digit(char),
    Point,
    /// A comma between digit placeholders, which groups thousands.
    Group,
    Percent,
    Exponent {
        plus: bool,
    },
    Slash,
    Text,
    General,
    Year(u8),
    Month(u8),
    Day(u8),
    Hour(u8),
    Minute(u8),
    Second(u8),
    /// Fractional seconds: how many digits.
    Subsecond(u8),
    AmPm {
        long: bool,
        upper: bool,
    },
    ElapsedHours(u8),
    ElapsedMinutes(u8),
    ElapsedSeconds(u8),
}

#[derive(Debug, Clone, PartialEq)]
enum Condition {
    Lt(f64),
    Le(f64),
    Gt(f64),
    Ge(f64),
    Eq(f64),
    Ne(f64),
}

impl Condition {
    fn holds(&self, n: f64) -> bool {
        match *self {
            Condition::Lt(x) => n < x,
            Condition::Le(x) => n <= x,
            Condition::Gt(x) => n > x,
            Condition::Ge(x) => n >= x,
            Condition::Eq(x) => n == x,
            Condition::Ne(x) => n != x,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Section {
    tokens: Vec<Token>,
    color: Option<Arc<str>>,
    condition: Option<Condition>,
    /// Commas after the last digit placeholder, each dividing by a thousand.
    scale: u32,
    percent: u32,
    is_date: bool,
}

/// A parsed format code.
#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    sections: Vec<Section>,
}

impl Format {
    pub fn parse(code: &str) -> Format {
        let mut sections = Vec::new();
        for part in split_sections(code) {
            sections.push(parse_section(&part));
        }
        if sections.is_empty() {
            sections.push(parse_section("General"));
        }
        Format { sections }
    }

    /// Whether this format shows dates or times, which is what decides a cell holds a date.
    pub fn is_date(&self) -> bool {
        self.sections.first().is_some_and(|s| s.is_date)
    }

    /// Whether the first section is General.
    pub fn is_general(&self) -> bool {
        self.sections.first().is_some_and(|s| s.tokens.contains(&Token::General))
    }

    pub fn is_text(&self) -> bool {
        self.sections.len() == 1
            && self.sections[0].tokens.contains(&Token::Text)
            && !self.sections[0]
                .tokens
                .iter()
                .any(|t| matches!(t, Token::Digit(_) | Token::General))
    }

    /// Show a value. `width` is how many characters General may use, 11 for a standard column.
    pub fn render(&self, value: &Value, date1904: bool, width: usize) -> Rendered {
        match value {
            Value::Empty => {
                Rendered { text: String::new(), color: None, numeric: false, fill: None }
            }
            Value::Number(n) => self.render_number(*n, date1904, width),
            Value::Text(t) => self.render_text(t),
            Value::Bool(b) => Rendered {
                text: if *b { "TRUE" } else { "FALSE" }.to_owned(),
                color: None,
                numeric: false,
                fill: None,
            },
            Value::Error(e) => {
                Rendered { text: e.text().to_owned(), color: None, numeric: false, fill: None }
            }
        }
    }

    fn render_text(&self, text: &str) -> Rendered {
        let section = match self.sections.len() {
            4 => Some(&self.sections[3]),
            _ => self
                .sections
                .iter()
                .find(|s| s.tokens.contains(&Token::Text) && s.condition.is_none())
                .filter(|_| {
                    self.sections.len() == 1 || self.sections[0].tokens.contains(&Token::Text)
                }),
        };
        let Some(section) = section else {
            return Rendered { text: text.to_owned(), color: None, numeric: false, fill: None };
        };
        let mut out = String::new();
        let mut fill = None;
        for token in &section.tokens {
            match token {
                Token::Text => out.push_str(text),
                Token::Literal(l) => out.push_str(l),
                Token::Space => out.push(' '),
                Token::Fill(c) => fill = Some((*c, out.len())),
                _ => {}
            }
        }
        Rendered { text: out, color: section.color.clone(), numeric: false, fill }
    }

    fn choose(&self, n: f64) -> (&Section, bool) {
        let has_conditions = self.sections.iter().any(|s| s.condition.is_some());
        if has_conditions {
            for (i, section) in self.sections.iter().enumerate() {
                match &section.condition {
                    Some(c) if c.holds(n) => return (section, i > 0 && n < 0.0),
                    Some(_) => {}
                    None => return (section, i > 0 && n < 0.0 && self.sections.len() > 2),
                }
            }
            // Excel's defaults: [>=0] and [<0] style when nothing matched.
            return (&self.sections[self.sections.len() - 1], false);
        }
        let numeric: Vec<&Section> = self.sections.iter().take(3).collect();
        match numeric.len() {
            1 => (numeric[0], false),
            2 => {
                if n < 0.0 {
                    (numeric[1], true)
                } else {
                    (numeric[0], false)
                }
            }
            _ => {
                if n > 0.0 {
                    (numeric[0], false)
                } else if n < 0.0 {
                    (numeric[1], true)
                } else {
                    (numeric[2], false)
                }
            }
        }
    }

    fn render_number(&self, n: f64, date1904: bool, width: usize) -> Rendered {
        if self.sections.len() == 4
            && self.sections[3].tokens.contains(&Token::Text)
            && self.sections[..3].is_empty()
        {
            return self.render_text(&crate::value::number_to_text(n));
        }
        let (section, signless) = self.choose(n);
        let value = if signless { n.abs() } else { n };
        // A text only section shows a number as General inside the literal text.
        if section.is_date {
            return match render_date(section, value, date1904) {
                Some(text) => {
                    Rendered { text, color: section.color.clone(), numeric: true, fill: None }
                }
                None => Rendered {
                    text: "#".repeat(width.max(1)),
                    color: None,
                    numeric: true,
                    fill: None,
                },
            };
        }
        let (text, fill) = render_numeric(section, value, width);
        Rendered { text, color: section.color.clone(), numeric: true, fill }
    }
}

fn split_sections(code: &str) -> Vec<String> {
    let mut sections = vec![String::new()];
    let mut chars = code.chars().peekable();
    let mut quoted = false;
    let mut bracket = false;
    while let Some(c) = chars.next() {
        let current = sections.last_mut().unwrap();
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            '\\' if !quoted => {
                current.push(c);
                if let Some(n) = chars.next() {
                    current.push(n);
                }
            }
            '[' if !quoted => {
                bracket = true;
                current.push(c);
            }
            ']' if !quoted => {
                bracket = false;
                current.push(c);
            }
            ';' if !quoted && !bracket => sections.push(String::new()),
            _ => current.push(c),
        }
    }
    sections
}

fn parse_section(text: &str) -> Section {
    let mut section = Section::default();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut tokens: Vec<Token> = Vec::new();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let starts = |at: usize, word: &str| -> bool {
        word.chars().enumerate().all(|(k, w)| lower.get(at + k) == Some(&w))
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut literal = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    literal.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Literal(literal));
                i += 1;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    tokens.push(Token::Literal(n.to_string()));
                }
                i += 2;
            }
            '_' => {
                tokens.push(Token::Space);
                i += 2;
            }
            '*' => {
                if let Some(n) = chars.get(i + 1) {
                    tokens.push(Token::Fill(*n));
                }
                i += 2;
            }
            '[' => {
                let end = chars[i..].iter().position(|&x| x == ']').map_or(chars.len(), |p| i + p);
                let inside: String = chars[i + 1..end].iter().collect();
                bracket(&inside, &mut section, &mut tokens);
                i = end + 1;
            }
            '0' | '#' | '?' => {
                tokens.push(Token::Digit(c));
                i += 1;
            }
            '.' => {
                // Fractional seconds: `.0` after seconds.
                if tokens
                    .iter()
                    .rev()
                    .find(|t| !matches!(t, Token::Literal(_)))
                    .is_some_and(|t| matches!(t, Token::Second(_) | Token::ElapsedSeconds(_)))
                    && chars.get(i + 1) == Some(&'0')
                {
                    let mut k = 0;
                    while chars.get(i + 1 + k) == Some(&'0') {
                        k += 1;
                    }
                    tokens.push(Token::Subsecond(k as u8));
                    i += 1 + k;
                } else {
                    tokens.push(Token::Point);
                    i += 1;
                }
            }
            ',' => {
                tokens.push(Token::Group);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                section.percent += 1;
                i += 1;
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+') | Some('-')) => {
                tokens.push(Token::Exponent { plus: chars[i + 1] == '+' });
                i += 2;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '@' => {
                tokens.push(Token::Text);
                i += 1;
            }
            _ if starts(i, "general") => {
                tokens.push(Token::General);
                i += 7;
            }
            _ if starts(i, "am/pm") => {
                tokens.push(Token::AmPm { long: true, upper: c == 'A' });
                section.is_date = true;
                i += 5;
            }
            _ if starts(i, "a/p") => {
                tokens.push(Token::AmPm { long: false, upper: c == 'A' });
                section.is_date = true;
                i += 3;
            }
            'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'h' | 'H' | 's' | 'S' | 'e' | 'g' | 'G' | 'b'
            | 'B' => {
                let lc = c.to_ascii_lowercase();
                let mut k = 1;
                while lower.get(i + k) == Some(&lc) {
                    k += 1;
                }
                let n = k.min(255) as u8;
                section.is_date =
                    section.is_date || matches!(lc, 'y' | 'm' | 'd' | 'h' | 's' | 'e');
                tokens.push(match lc {
                    'y' | 'e' => Token::Year(n),
                    'm' => Token::Month(n),
                    'd' => Token::Day(n),
                    'h' => Token::Hour(n),
                    's' => Token::Second(n),
                    _ => Token::Literal(String::new()),
                });
                i += k;
            }
            other => {
                tokens.push(Token::Literal(other.to_string()));
                i += 1;
            }
        }
    }
    // A month token next to hours or seconds is minutes.
    for k in 0..tokens.len() {
        if let Token::Month(n) = tokens[k] {
            if n > 2 {
                continue;
            }
            let before =
                tokens[..k].iter().rev().find(|t| !matches!(t, Token::Literal(_) | Token::Space));
            let after =
                tokens[k + 1..].iter().find(|t| !matches!(t, Token::Literal(_) | Token::Space));
            let minutes = matches!(before, Some(Token::Hour(_)) | Some(Token::ElapsedHours(_)))
                || matches!(after, Some(Token::Second(_)) | Some(Token::ElapsedSeconds(_)));
            if minutes {
                tokens[k] = Token::Minute(n);
            }
        }
    }
    // Commas: between digit placeholders they group thousands; trailing ones scale.
    let last_digit = tokens.iter().rposition(|t| matches!(t, Token::Digit(_)));
    let first_digit = tokens.iter().position(|t| matches!(t, Token::Digit(_)));
    let point = tokens.iter().position(|t| *t == Token::Point);
    let mut cleaned = Vec::with_capacity(tokens.len());
    for (k, token) in tokens.into_iter().enumerate() {
        if token == Token::Group {
            let inside = match (first_digit, last_digit) {
                (Some(f), Some(l)) => k > f && k < l && point.is_none_or(|p| k < p),
                _ => false,
            };
            if inside {
                cleaned.push(Token::Group);
            } else if last_digit.is_some_and(|l| k > l) {
                section.scale += 1;
            } else {
                cleaned.push(Token::Literal(",".to_owned()));
            }
            continue;
        }
        cleaned.push(token);
    }
    if section.is_date && cleaned.iter().any(|t| matches!(t, Token::Digit('#') | Token::Digit('?')))
    {
        section.is_date = false;
    }
    section.tokens = cleaned;
    section
}

fn bracket(inside: &str, section: &mut Section, tokens: &mut Vec<Token>) {
    let lower = inside.to_ascii_lowercase();
    const COLORS: [&str; 8] =
        ["black", "blue", "cyan", "green", "magenta", "red", "white", "yellow"];
    if let Some(c) = COLORS.iter().find(|c| **c == lower) {
        let mut name = c.to_string();
        name[..1].make_ascii_uppercase();
        section.color = Some(Arc::from(name.as_str()));
        return;
    }
    if let Some(n) = lower.strip_prefix("color") {
        if n.parse::<u32>().is_ok() {
            section.color = Some(Arc::from(format!("Color{n}").as_str()));
            return;
        }
    }
    if lower.chars().all(|c| c == 'h') && !lower.is_empty() {
        tokens.push(Token::ElapsedHours(lower.len() as u8));
        section.is_date = true;
        return;
    }
    if lower.chars().all(|c| c == 'm') && !lower.is_empty() {
        tokens.push(Token::ElapsedMinutes(lower.len() as u8));
        section.is_date = true;
        return;
    }
    if lower.chars().all(|c| c == 's') && !lower.is_empty() {
        tokens.push(Token::ElapsedSeconds(lower.len() as u8));
        section.is_date = true;
        return;
    }
    if let Some(rest) = inside.strip_prefix('$') {
        // `[$€-409]`: the text before the dash is shown.
        let symbol = rest.split('-').next().unwrap_or("");
        if !symbol.is_empty() {
            tokens.push(Token::Literal(symbol.to_owned()));
        }
        return;
    }
    let ops = [("<=", 1), (">=", 2), ("<>", 3), ("<", 4), (">", 5), ("=", 6)];
    for (op, kind) in ops {
        if let Some(n) = inside.strip_prefix(op).and_then(|n| n.trim().parse::<f64>().ok()) {
            section.condition = Some(match kind {
                1 => Condition::Le(n),
                2 => Condition::Ge(n),
                3 => Condition::Ne(n),
                4 => Condition::Lt(n),
                5 => Condition::Gt(n),
                _ => Condition::Eq(n),
            });
            return;
        }
    }
}

/// The General format: as many significant digits as fit in `width` characters.
pub fn general(n: f64, width: usize) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    let width = width.max(5);
    // Measured: the sign is not counted, so -1/3 is -0.333333333.
    let negative = n < 0.0;
    let room = width;
    let abs = n.abs();
    let exponent = abs.log10().floor() as i32;
    // Plain notation when the whole part fits and the number is not so small it would show as zeros.
    if exponent < room as i32 && exponent >= -5 {
        let whole_len = if exponent >= 0 { exponent as usize + 1 } else { 1 };
        let decimals = room.saturating_sub(whole_len + 1);
        let digits_allowed = if exponent >= 0 {
            (decimals + whole_len).min(15)
        } else {
            (decimals as i32 + exponent + 1).max(1) as usize
        };
        let (digits, e) = significant_digits(abs, digits_allowed.min(15));
        let text = place_digits(&digits, e);
        if text.len() <= room {
            return format!("{}{text}", if negative { "-" } else { "" });
        }
    }
    // Scientific: d.dddE+xx within the room.
    let exponent_text_len = 4;
    let mantissa_digits = room.saturating_sub(exponent_text_len + 1).clamp(1, 15);
    let (digits, e) = significant_digits(abs, mantissa_digits);
    let mantissa = if digits.len() > 1 {
        format!("{}.{}", &digits[..1], &digits[1..])
    } else {
        digits.clone()
    };
    format!(
        "{}{mantissa}E{}{:02}",
        if negative { "-" } else { "" },
        if e < 0 { '-' } else { '+' },
        e.abs()
    )
}

fn place_digits(digits: &str, exponent: i32) -> String {
    if exponent >= 0 {
        let whole = exponent as usize + 1;
        if digits.len() <= whole {
            format!("{digits}{}", "0".repeat(whole - digits.len()))
        } else {
            format!("{}.{}", &digits[..whole], &digits[whole..])
        }
    } else {
        format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    }
}

fn render_numeric(section: &Section, value: f64, width: usize) -> (String, Option<(char, usize)>) {
    let tokens = &section.tokens;
    let mut v = value * 100f64.powi(section.percent as i32) / 1000f64.powi(section.scale as i32);
    let mut fill = None;
    let has_digits = tokens.iter().any(|t| matches!(t, Token::Digit(_)));
    if tokens.contains(&Token::General) || !has_digits && !tokens.contains(&Token::Text) {
        // General, or a section of literal text only.
        let mut out = String::new();
        for t in tokens {
            match t {
                Token::General => out.push_str(&general(v, width)),
                Token::Literal(l) => out.push_str(l),
                Token::Space => out.push(' '),
                Token::Fill(c) => fill = Some((*c, out.len())),
                Token::Percent => out.push('%'),
                _ => {}
            }
        }
        if !tokens.contains(&Token::General) && out.is_empty() {
            return (String::new(), fill);
        }
        return (out, fill);
    }
    // Scientific.
    if let Some(e_at) = tokens.iter().position(|t| matches!(t, Token::Exponent { .. })) {
        return (render_scientific(section, v, e_at), fill);
    }
    // Fractions.
    if let Some(slash) = tokens.iter().position(|t| *t == Token::Slash) {
        if tokens[..slash].iter().any(|t| matches!(t, Token::Digit(_))) {
            return (render_fraction(section, v, slash), fill);
        }
    }
    let point = tokens.iter().position(|t| *t == Token::Point);
    let int_tokens: Vec<char> = tokens[..point.unwrap_or(tokens.len())]
        .iter()
        .filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None })
        .collect();
    let dec_tokens: Vec<char> = match point {
        Some(p) => tokens[p + 1..]
            .iter()
            .filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None })
            .collect(),
        None => Vec::new(),
    };
    let grouped = tokens.contains(&Token::Group);
    let decimals = dec_tokens.len();
    let negative = v < 0.0;
    v = v.abs();
    // Round on the decimal value, as ROUND does.
    let rounded = crate::functions::math::round_to(
        v,
        decimals as f64,
        crate::functions::math::Rounding::Half,
    );
    let whole = rounded.trunc();
    let whole_digits = if whole == 0.0 { String::new() } else { format_whole(whole) };
    let frac_digits: String = if decimals > 0 {
        let frac = round_significant(rounded - whole, 15);
        let text = format!("{:.*}", decimals, frac);
        text.split_once('.').map(|(_, d)| d.to_owned()).unwrap_or_default()
    } else {
        String::new()
    };
    // Integer part: fill placeholders from the right.
    let mut int_out: Vec<char> = Vec::new();
    let digits: Vec<char> = whole_digits.chars().collect();
    let placeholders = int_tokens.len();
    let extra = digits.len().saturating_sub(placeholders);
    let mut di = 0;
    for (k, p) in int_tokens.iter().enumerate() {
        if k == 0 && extra > 0 {
            int_out.extend(&digits[..extra]);
            di = extra;
        }
        let from_right = placeholders - k;
        let available = digits.len() - di;
        if available >= from_right {
            int_out.push(digits[di]);
            di += 1;
        } else {
            match p {
                '0' => int_out.push('0'),
                '?' => int_out.push(' '),
                _ => {}
            }
        }
    }
    if placeholders == 0 && !digits.is_empty() {
        int_out.extend(&digits);
    }
    let mut int_text: String = int_out.iter().collect();
    if grouped {
        int_text = group(&int_text);
    }
    // Decimal part: trailing zeros shown for 0, dropped for #, spaces for ?.
    let mut dec_out: Vec<char> = frac_digits.chars().collect();
    for k in (0..dec_out.len()).rev() {
        if dec_out[k] != '0' {
            break;
        }
        match dec_tokens.get(k) {
            Some('#') => {
                dec_out.pop();
            }
            Some('?') => dec_out[k] = ' ',
            _ => break,
        }
    }
    // Lay the tokens out.
    let mut out = String::new();
    let mut int_written = false;
    let mut dec_written = false;
    let mut dec_index = 0;
    let shows_negative = negative && rounded != 0.0;
    if shows_negative {
        out.push('-');
    }
    for (k, t) in tokens.iter().enumerate() {
        let in_int = point.is_none_or(|p| k < p);
        match t {
            Token::Digit(_) if in_int && !int_written => {
                out.push_str(&int_text);
                int_written = true;
            }
            Token::Digit(_) if !dec_written => {
                let take: String = dec_out.iter().skip(dec_index).collect();
                out.push_str(&take);
                dec_index = dec_out.len();
                dec_written = true;
            }
            Token::Point => out.push('.'),
            Token::Group => {}
            Token::Percent => out.push('%'),
            Token::Literal(l) => out.push_str(l),
            Token::Space => out.push(' '),
            Token::Fill(c) => fill = Some((*c, out.len())),
            Token::Text => out.push_str(&crate::value::number_to_text(value)),
            _ => {}
        }
    }
    (out, fill)
}

fn format_whole(n: f64) -> String {
    if n < 1e15 {
        format!("{}", n as u64)
    } else {
        let (digits, e) = significant_digits(n, 15);
        place_digits(&digits, e)
    }
}

fn group(int_text: &str) -> String {
    let digits: Vec<char> = int_text.chars().collect();
    let digit_count = digits.iter().filter(|c| c.is_ascii_digit()).count();
    let mut out = String::new();
    let mut seen = 0;
    for c in digits.iter() {
        out.push(*c);
        if c.is_ascii_digit() {
            seen += 1;
            let left = digit_count - seen;
            if left > 0 && left % 3 == 0 {
                out.push(',');
            }
        }
    }
    out
}

fn render_scientific(section: &Section, v: f64, e_at: usize) -> String {
    let tokens = &section.tokens;
    let Token::Exponent { plus } = tokens[e_at] else { return String::new() };
    let mantissa_tokens = &tokens[..e_at];
    let exponent_digits =
        tokens[e_at + 1..].iter().filter(|t| matches!(t, Token::Digit(_))).count().max(1);
    let point = mantissa_tokens.iter().position(|t| *t == Token::Point);
    let int_places = mantissa_tokens[..point.unwrap_or(mantissa_tokens.len())]
        .iter()
        .filter(|t| matches!(t, Token::Digit(_)))
        .count()
        .max(1);
    let dec_tokens: Vec<char> = match point {
        Some(p) => mantissa_tokens[p + 1..]
            .iter()
            .filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None })
            .collect(),
        None => Vec::new(),
    };
    let negative = v < 0.0;
    let abs = v.abs();
    let mut exponent = if abs == 0.0 { 0 } else { abs.log10().floor() as i32 };
    // With more than one integer placeholder, the exponent is a multiple of their count (engineering).
    if int_places > 1 {
        exponent = exponent.div_euclid(int_places as i32) * int_places as i32;
    }
    let mut mantissa = if abs == 0.0 { 0.0 } else { abs / 10f64.powi(exponent) };
    mantissa = crate::functions::math::round_to(
        mantissa,
        dec_tokens.len() as f64,
        crate::functions::math::Rounding::Half,
    );
    if mantissa >= 10f64.powi(int_places as i32) {
        mantissa /= 10f64.powi(int_places as i32);
        exponent += int_places as i32;
        mantissa = crate::functions::math::round_to(
            mantissa,
            dec_tokens.len() as f64,
            crate::functions::math::Rounding::Half,
        );
    }
    let mut text = format!("{:.*}", dec_tokens.len(), mantissa);
    // Trailing optional decimals.
    if let Some((w, d)) = text.clone().split_once('.') {
        let mut d: Vec<char> = d.chars().collect();
        while let (Some('0'), Some('#')) = (d.last(), dec_tokens.get(d.len().wrapping_sub(1))) {
            d.pop();
        }
        text = if d.is_empty() && !dec_tokens.contains(&'0') {
            format!("{w}.")
        } else {
            format!("{w}.{}", d.iter().collect::<String>())
        };
    }
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    let mut placed = false;
    for t in mantissa_tokens {
        match t {
            Token::Digit(_) | Token::Point if !placed => {
                out.push_str(&text);
                placed = true;
            }
            Token::Literal(l) => out.push_str(l),
            Token::Space => out.push(' '),
            _ => {}
        }
    }
    let sign = if exponent < 0 {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    out.push('E');
    out.push_str(sign);
    out.push_str(&format!("{:0width$}", exponent.abs(), width = exponent_digits));
    for t in &tokens[e_at + 1..] {
        match t {
            Token::Literal(l) => out.push_str(l),
            Token::Space => out.push(' '),
            _ => {}
        }
    }
    out
}

fn render_fraction(section: &Section, v: f64, slash: usize) -> String {
    let tokens = &section.tokens;
    // The denominator: placeholders after the slash, or a fixed number.
    let after: Vec<&Token> = tokens[slash + 1..].iter().collect();
    let den_digits: Vec<char> = after
        .iter()
        .take_while(|t| matches!(t, Token::Digit(_)))
        .filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None })
        .collect();
    let fixed: Option<u64> = {
        let lit: String = after
            .iter()
            .take_while(|t| matches!(t, Token::Literal(l) if l.chars().all(|c| c.is_ascii_digit())))
            .map(|t| if let Token::Literal(l) = t { l.clone() } else { String::new() })
            .collect();
        lit.parse().ok()
    };
    // Is there a whole number part? Digits, then a space literal, then the numerator digits.
    let before = &tokens[..slash];
    let numerator_start =
        before.iter().rposition(|t| !matches!(t, Token::Digit(_))).map_or(0, |p| p + 1);
    let has_whole = before[..numerator_start].iter().any(|t| matches!(t, Token::Digit(_)));
    let negative = v < 0.0;
    let abs = v.abs();
    let (mut whole, frac) = if has_whole { (abs.trunc(), abs.fract()) } else { (0.0, abs) };
    let (num, den) = match fixed {
        Some(d) => ((frac * d as f64).round(), d as f64),
        None => {
            let max_den = 10u64.pow(den_digits.len().max(1) as u32) - 1;
            best_fraction(frac, max_den)
        }
    };
    let (mut num, den) = (num, den);
    if num == den && has_whole {
        whole += 1.0;
        num = 0.0;
    }
    let num_places = before[numerator_start..].len();
    let mut out = String::new();
    if negative && (whole != 0.0 || num != 0.0) {
        out.push('-');
    }
    if has_whole {
        let mut wrote = false;
        for t in &before[..numerator_start] {
            match t {
                Token::Digit(_) if !wrote => {
                    if whole != 0.0 || num == 0.0 {
                        out.push_str(&format!("{}", whole as u64));
                    }
                    wrote = true;
                }
                Token::Literal(l) => {
                    if whole != 0.0 || num == 0.0 {
                        out.push_str(l)
                    } else {
                        out.push_str(&" ".repeat(l.len()))
                    }
                }
                Token::Space => out.push(' '),
                _ => {}
            }
        }
        if num == 0.0 {
            // A whole number with no fraction leaves room the width of the fraction.
            let width =
                num_places + 1 + den_digits.len().max(fixed.map_or(0, |d| d.to_string().len()));
            out.push_str(&" ".repeat(width));
            return out;
        }
    }
    let num_text = format!("{}", num as u64);
    out.push_str(&format!("{:>width$}", num_text, width = num_places));
    out.push('/');
    let den_text = format!("{}", den as u64);
    if fixed.is_some() {
        out.push_str(&den_text);
    } else {
        out.push_str(&format!("{:<width$}", den_text, width = den_digits.len()));
    }
    out
}

/// The fraction nearest a value with a denominator up to `max_den`.
fn best_fraction(x: f64, max_den: u64) -> (f64, f64) {
    let mut best = (x.round(), 1.0);
    let mut best_error = (x - x.round()).abs();
    for den in 1..=max_den {
        let num = (x * den as f64).round();
        let error = (x - num / den as f64).abs();
        if error < best_error - 1e-12 {
            best = (num, den as f64);
            best_error = error;
        }
    }
    best
}

fn render_date(section: &Section, value: f64, date1904: bool) -> Option<String> {
    if value < 0.0 {
        return None;
    }
    let subsecond_digits = section
        .tokens
        .iter()
        .find_map(|t| if let Token::Subsecond(n) = t { Some(*n) } else { None })
        .unwrap_or(0);
    // Round to the precision shown, which is how 10:59:59.6 shows as 11:00:00.
    let unit = 86_400.0 * 10f64.powi(subsecond_digits as i32);
    let rounded = (value * unit).round() / unit;
    let (y, m, d) = dates::date(rounded, date1904)?;
    let fraction = rounded - rounded.floor();
    let total_ms = (fraction * 86_400_000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = total_ms / 60_000 % 60;
    let seconds = total_ms / 1000 % 60;
    let millis = total_ms % 1000;
    let twelve = section.tokens.iter().any(|t| matches!(t, Token::AmPm { .. }));
    let weekday = dates::weekday(rounded, date1904) as usize;
    let mut out = String::new();
    for t in &section.tokens {
        match t {
            Token::Year(n) => {
                if *n <= 2 {
                    out.push_str(&format!("{:02}", y.rem_euclid(100)));
                } else {
                    out.push_str(&format!("{:04}", y));
                }
            }
            Token::Month(n) => out.push_str(&match n {
                1 => format!("{m}"),
                2 => format!("{m:02}"),
                3 => dates::MONTHS[m as usize - 1][..3].to_owned(),
                5 => dates::MONTHS[m as usize - 1][..1].to_owned(),
                _ => dates::MONTHS[m as usize - 1].to_owned(),
            }),
            Token::Day(n) => out.push_str(&match n {
                1 => format!("{d}"),
                2 => format!("{d:02}"),
                3 => dates::DAYS[weekday][..3].to_owned(),
                _ => dates::DAYS[weekday].to_owned(),
            }),
            Token::Hour(n) => {
                let h = if twelve {
                    let h = hours % 12;
                    if h == 0 {
                        12
                    } else {
                        h
                    }
                } else {
                    hours
                };
                out.push_str(&if *n >= 2 { format!("{h:02}") } else { format!("{h}") });
            }
            Token::Minute(n) => {
                out.push_str(&if *n >= 2 { format!("{minutes:02}") } else { format!("{minutes}") })
            }
            Token::Second(n) => {
                out.push_str(&if *n >= 2 { format!("{seconds:02}") } else { format!("{seconds}") })
            }
            Token::Subsecond(n) => {
                let digits = format!("{millis:03}");
                out.push('.');
                out.push_str(&digits[..(*n as usize).min(3)]);
            }
            Token::AmPm { long, upper } => {
                let pm = hours >= 12;
                let text = match (long, pm) {
                    (true, false) => "AM",
                    (true, true) => "PM",
                    (false, false) => "A",
                    (false, true) => "P",
                };
                out.push_str(&if *upper { text.to_owned() } else { text.to_ascii_lowercase() });
            }
            Token::ElapsedHours(n) => {
                let h = (rounded * 24.0).floor() as i64;
                out.push_str(&format!("{:0width$}", h, width = *n as usize));
            }
            Token::ElapsedMinutes(n) => {
                let mins = (rounded * 1440.0).floor() as i64;
                out.push_str(&format!("{:0width$}", mins, width = *n as usize));
            }
            Token::ElapsedSeconds(n) => {
                let secs = (rounded * 86_400.0).floor() as i64;
                out.push_str(&format!("{:0width$}", secs, width = *n as usize));
            }
            Token::Literal(l) => out.push_str(l),
            Token::Space => out.push(' '),
            Token::Digit(c) => out.push(*c),
            Token::Point => out.push('.'),
            Token::Slash => out.push('/'),
            Token::Percent => out.push('%'),
            _ => {}
        }
    }
    Some(out)
}

/// Show a value with a format code, for TEXT.
pub fn text(value: &Value, code: &str, date1904: bool) -> Result<String, ErrorKind> {
    if let Value::Error(e) = value {
        return Err(*e);
    }
    let format = Format::parse(code);
    Ok(format.render(value, date1904, 11).text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: f64, code: &str) -> String {
        Format::parse(code).render(&Value::Number(n), false, 11).text
    }

    #[test]
    fn number_sections_show_what_excel_shows() {
        let cases: &[(f64, &str, &str)] = &[
            (1234.567, "0.00", "1234.57"),
            (1234.567, "#,##0.00", "1,234.57"),
            (1234567.0, "#,##0", "1,234,567"),
            (0.5, "0%", "50%"),
            (0.1234, "0.00%", "12.34%"),
            (-5.0, "0.00;(0.00)", "(5.00)"),
            (0.0, "0;-0;\"zero\"", "zero"),
            (5.0, "0;-0;\"zero\"", "5"),
            (1234.0, "#,##0,", "1"),
            (1234567.0, "0.0,,\"M\"", "1.2M"),
            (12345.678, "0.00E+00", "1.23E+04"),
            (0.000123, "0.00E+00", "1.23E-04"),
            (1.5, "# ?/?", "1 1/2"),
            (0.75, "?/?", "3/4"),
            (2.333, "# ??/??", "2  1/3 "),
            (5.0, "00000", "00005"),
            (3.1, "#.##", "3.1"),
            (3.0, "#.##", "3."),
            (0.5, "#.##", ".5"),
            (-1234.5, "$#,##0.00", "-$1,234.50"),
            (1234.5, "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)", "$1,234.50 "),
            (-1234.5, "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)", "($1,234.50)"),
            (150.0, "[>=100]\"big\";\"small\"", "big"),
            (50.0, "[>=100]\"big\";\"small\"", "small"),
        ];
        let mut wrong = Vec::new();
        for (n, code, want) in cases {
            let got = r(*n, code);
            if got != *want {
                wrong.push(format!("{n} as {code}: expected [{want}] got [{got}]"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn dates_and_times_show_what_excel_shows() {
        let when = 46318.0 + 14.5 / 24.0 + 5.0 / 86400.0;
        let cases: &[(&str, &str)] = &[
            ("m/d/yyyy", "10/23/2026"),
            ("yyyy-mm-dd", "2026-10-23"),
            ("d-mmm-yy", "23-Oct-26"),
            ("dddd, mmmm d, yyyy", "Friday, October 23, 2026"),
            ("h:mm AM/PM", "2:30 PM"),
            ("hh:mm:ss", "14:30:05"),
            ("mmm", "Oct"),
            ("mmmmm", "O"),
            ("h:mm", "14:30"),
            ("[h]:mm", "1111646:30"),
        ];
        for (code, want) in cases {
            assert_eq!(
                &Format::parse(code).render(&Value::Number(when), false, 11).text,
                want,
                "{code}"
            );
        }
        assert_eq!(
            Format::parse("mm:ss.0").render(&Value::Number(1.25 / 86400.0), false, 11).text,
            "00:01.3"
        );
    }

    #[test]
    fn general_fits_eleven_characters() {
        assert_eq!(general(1.0 / 3.0, 11), "0.333333333");
        assert_eq!(general(-1.0 / 3.0, 11), "-0.333333333");
        assert_eq!(general(123456789012.0, 11), "1.23457E+11");
        assert_eq!(general(1234567890.0, 11), "1234567890");
        assert_eq!(general(0.1 + 0.2, 11), "0.3");
        assert_eq!(general(100.0, 11), "100");
    }

    #[test]
    fn text_sections_wrap_text() {
        let f = Format::parse("0;-0;0;\"[\"@\"]\"");
        assert_eq!(f.render(&Value::text("hi"), false, 11).text, "[hi]");
        assert_eq!(Format::parse("0.00").render(&Value::text("hi"), false, 11).text, "hi");
        assert!(Format::parse("m/d/yyyy").is_date());
        assert!(!Format::parse("#,##0").is_date());
    }
}
