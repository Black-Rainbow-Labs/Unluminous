//! Text functions. Positions count characters, not bytes, from 1.

use super::{num, number, opt, opt_num, r, s, text_of, Function, MANY};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("CONCAT", 1, MANY, "text1, [text2], ...", "Joins text, ranges included.", |ctx, a| {
        join(ctx, a, "", true)
    }),
    s("CONCATENATE", 1, MANY, "text1, [text2], ...", "Joins text.", |_, a| {
        let mut out = String::new();
        for v in a {
            out.push_str(&text_of(v)?);
        }
        limit(out)
    }),
    r(
        "TEXTJOIN",
        3,
        MANY,
        "delimiter, ignore_empty, text1, [text2], ...",
        "Joins text with a delimiter between.",
        textjoin,
    ),
    s("LEFT", 1, 2, "text, [num_chars]", "The first characters.", |ctx, a| {
        let t = text_of(&a[0])?;
        let n = opt_num(ctx, a, 1, 1.0)?;
        if n < 0.0 {
            return Err(ErrorKind::Value);
        }
        Ok(Value::text(t.chars().take(n as usize).collect::<String>()))
    }),
    s("RIGHT", 1, 2, "text, [num_chars]", "The last characters.", |ctx, a| {
        let t = text_of(&a[0])?;
        let n = opt_num(ctx, a, 1, 1.0)?;
        if n < 0.0 {
            return Err(ErrorKind::Value);
        }
        let count = t.chars().count();
        Ok(Value::text(t.chars().skip(count.saturating_sub(n as usize)).collect::<String>()))
    }),
    s("MID", 3, 3, "text, start_num, num_chars", "Characters from the middle.", |ctx, a| {
        let t = text_of(&a[0])?;
        let (start, n) = (num(ctx, &a[1])?, num(ctx, &a[2])?);
        if start < 1.0 || n < 0.0 {
            return Err(ErrorKind::Value);
        }
        Ok(Value::text(t.chars().skip(start as usize - 1).take(n as usize).collect::<String>()))
    }),
    s("LEN", 1, 1, "text", "How many characters.", |_, a| {
        Ok(Value::Number(text_of(&a[0])?.chars().count() as f64))
    }),
    s(
        "FIND",
        2,
        3,
        "find_text, within_text, [start_num]",
        "Where one text starts in another, case sensitive.",
        |ctx, a| find(ctx, a, true),
    ),
    s(
        "SEARCH",
        2,
        3,
        "find_text, within_text, [start_num]",
        "Where one text starts in another, with wildcards and without regard to case.",
        |ctx, a| find(ctx, a, false),
    ),
    s(
        "REPLACE",
        4,
        4,
        "old_text, start_num, num_chars, new_text",
        "Replaces characters at a position.",
        |ctx, a| {
            let t: Vec<char> = text_of(&a[0])?.chars().collect();
            let (start, n) = (num(ctx, &a[1])?, num(ctx, &a[2])?);
            if start < 1.0 || n < 0.0 {
                return Err(ErrorKind::Value);
            }
            let start = (start as usize - 1).min(t.len());
            let end = (start + n as usize).min(t.len());
            let mut out: String = t[..start].iter().collect();
            out.push_str(&text_of(&a[3])?);
            out.extend(&t[end..]);
            limit(out)
        },
    ),
    s(
        "SUBSTITUTE",
        3,
        4,
        "text, old_text, new_text, [instance_num]",
        "Replaces text with other text.",
        |ctx, a| {
            let t = text_of(&a[0])?;
            let old = text_of(&a[1])?;
            let new = text_of(&a[2])?;
            if old.is_empty() {
                return Ok(Value::text(t));
            }
            match a.get(3) {
                None => limit(t.replace(&old, &new)),
                Some(v) => {
                    let n = num(ctx, v)?.trunc();
                    if n < 1.0 {
                        return Err(ErrorKind::Value);
                    }
                    match t.match_indices(&old).nth(n as usize - 1) {
                        Some((at, _)) => {
                            limit(format!("{}{}{}", &t[..at], new, &t[at + old.len()..]))
                        }
                        None => Ok(Value::text(t)),
                    }
                }
            }
        },
    ),
    s("UPPER", 1, 1, "text", "Capitals.", |_, a| {
        Ok(Value::text(text_of(&a[0])?.chars().map(simple_upper).collect::<String>()))
    }),
    s("LOWER", 1, 1, "text", "Lower case.", |_, a| Ok(Value::text(lower_text(&text_of(&a[0])?)))),
    s("PROPER", 1, 1, "text", "The first letter of each word a capital.", |_, a| {
        Ok(Value::text(proper_text(&text_of(&a[0])?)))
    }),
    s("TRIM", 1, 1, "text", "Removes spaces but single ones between words.", |_, a| {
        Ok(Value::text(
            text_of(&a[0])?.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "),
        ))
    }),
    s("CLEAN", 1, 1, "text", "Removes characters that do not print.", |_, a| {
        Ok(Value::text(text_of(&a[0])?.chars().filter(|c| *c as u32 >= 32).collect::<String>()))
    }),
    s("REPT", 2, 2, "text, number_times", "Text repeated.", |ctx, a| {
        let t = text_of(&a[0])?;
        let n = num(ctx, &a[1])?;
        if n < 0.0 {
            return Err(ErrorKind::Value);
        }
        if t.chars().count() as f64 * n.trunc() > 32_767.0 {
            return Err(ErrorKind::Value);
        }
        Ok(Value::text(t.repeat(n as usize)))
    }),
    s("EXACT", 2, 2, "text1, text2", "Whether two texts are the same, case included.", |_, a| {
        Ok(Value::Bool(text_of(&a[0])? == text_of(&a[1])?))
    }),
    s("TEXT", 2, 2, "value, format_text", "A value shown with a number format.", |ctx, a| {
        let code = text_of(&a[1])?;
        let value = match &a[0] {
            Value::Text(t) => match crate::input::number_from_text(t, ctx.date1904()) {
                Some(n) => Value::Number(n),
                None => a[0].clone(),
            },
            Value::Bool(b) => Value::text(if *b { "TRUE" } else { "FALSE" }),
            // An empty cell is shown as the number 0.
            Value::Empty => Value::Number(0.0),
            other => other.clone(),
        };
        Ok(Value::text(crate::numfmt::text(&value, &code, ctx.date1904())?))
    }),
    s("VALUE", 1, 1, "text", "Text that looks like a number, as a number.", |ctx, a| match &a[0] {
        Value::Number(n) => number(*n),
        Value::Empty => number(0.0),
        Value::Bool(_) => Err(ErrorKind::Value),
        v => num(ctx, v).and_then(number),
    }),
    s(
        "NUMBERVALUE",
        1,
        3,
        "text, [decimal_separator], [group_separator]",
        "Text as a number, with the separators given.",
        |ctx, a| {
            let mut t = text_of(&a[0])?;
            let decimal = opt(a, 1).map(text_of).transpose()?.unwrap_or_else(|| ".".to_owned());
            if decimal.is_empty() {
                return Err(ErrorKind::Value);
            }
            // Without a group separator it is the one that is not the decimal separator: `.` when that is `,`.
            let other = if decimal.starts_with(',') { "." } else { "," };
            let groups = opt(a, 2).map(text_of).transpose()?.unwrap_or_else(|| other.to_owned());
            if groups.is_empty() || groups.chars().next() == decimal.chars().next() {
                return Err(ErrorKind::Value);
            }
            t.retain(|c| !c.is_whitespace());
            if let Some(g) = groups.chars().next() {
                t = t.replace(g, "");
            }
            if let Some(d) = decimal.chars().next() {
                t = t.replace(d, ".");
            }
            if t.is_empty() {
                return number(0.0);
            }
            let percents = t.chars().rev().take_while(|c| *c == '%').count();
            let body = &t[..t.len() - percents];
            match body.parse::<f64>() {
                Ok(n) => number(n / 100f64.powi(percents as i32)),
                // Anything else is read as typing reads it: `(5)`, `$5`, and a date once the spaces
                // are gone.
                Err(_) => crate::input::number_from_text(&t, ctx.date1904())
                    .ok_or(ErrorKind::Value)
                    .and_then(number),
            }
        },
    ),
    s(
        "FIXED",
        1,
        3,
        "number, [decimals], [no_commas]",
        "A number rounded and shown as text.",
        |ctx, a| {
            let n = num(ctx, &a[0])?;
            let d = opt_num(ctx, a, 1, 2.0)?.trunc();
            let no_commas = super::opt_bool(a, 2, false)?;
            let rounded = super::math::round_to(n, d, super::math::Rounding::Half);
            let decimals = d.max(0.0) as usize;
            let code = format!(
                "{}{}",
                if no_commas { "0" } else { "#,##0" },
                if decimals > 0 { format!(".{}", "0".repeat(decimals)) } else { String::new() }
            );
            Ok(Value::text(
                crate::numfmt::Format::parse(&code)
                    .render(&Value::Number(rounded), false, 255)
                    .text,
            ))
        },
    ),
    s("DOLLAR", 1, 2, "number, [decimals]", "A number shown as currency text.", |ctx, a| {
        let n = num(ctx, &a[0])?;
        let d = opt_num(ctx, a, 1, 2.0)?.trunc();
        let rounded = super::math::round_to(n, d, super::math::Rounding::Half);
        let decimals = d.max(0.0) as usize;
        let body = format!(
            "\"$\"#,##0{}",
            if decimals > 0 { format!(".{}", "0".repeat(decimals)) } else { String::new() }
        );
        // A negative number is in parentheses even when it rounds to nothing: DOLLAR(-0.001) is ($0.00).
        let shown = crate::numfmt::Format::parse(&body)
            .render(&Value::Number(rounded.abs()), false, 255)
            .text;
        Ok(Value::text(if n < 0.0 { format!("({shown})") } else { shown }))
    }),
    s(
        "CHAR",
        1,
        1,
        "number",
        "The character with a code, from the Windows character set.",
        |ctx, a| {
            let n = num(ctx, &a[0])?.trunc();
            if !(1.0..=255.0).contains(&n) {
                return Err(ErrorKind::Value);
            }
            Ok(Value::text(windows_1252(n as u8).to_string()))
        },
    ),
    s(
        "CODE",
        1,
        1,
        "text",
        "The code of the first character, in the Windows character set.",
        |_, a| {
            let t = text_of(&a[0])?;
            let c = t.chars().next().ok_or(ErrorKind::Value)?;
            Ok(Value::Number((1..=255u8).find(|b| windows_1252(*b) == c).map_or(63.0, f64::from)))
        },
    ),
    s("UNICHAR", 1, 1, "number", "The Unicode character with a number.", |ctx, a| {
        let n = num(ctx, &a[0])?.trunc();
        if !(1.0..=1_114_111.0).contains(&n) {
            return Err(ErrorKind::Value);
        }
        // A surrogate and the last two code points of each plane are not characters: #N/A.
        let code = n as u32;
        if (0xD800..=0xDFFF).contains(&code) || code & 0xFFFE == 0xFFFE {
            return Err(ErrorKind::NA);
        }
        char::from_u32(code).map(|c| Value::text(c.to_string())).ok_or(ErrorKind::NA)
    }),
    s("UNICODE", 1, 1, "text", "The Unicode number of the first character.", |_, a| {
        text_of(&a[0])?
            .chars()
            .next()
            .map(|c| Value::Number(c as u32 as f64))
            .ok_or(ErrorKind::Value)
    }),
    s("T", 1, 1, "value", "Text, or empty text for anything else.", |_, a| match &a[0] {
        Value::Text(t) => Ok(Value::Text(t.clone())),
        Value::Error(e) => Err(*e),
        _ => Ok(Value::text("")),
    }),
    r(
        "TEXTBEFORE",
        2,
        6,
        "text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found]",
        "The text before a delimiter.",
        |ctx, a| text_around(ctx, a, true),
    ),
    r(
        "TEXTAFTER",
        2,
        6,
        "text, delimiter, [instance_num], [match_mode], [match_end], [if_not_found]",
        "The text after a delimiter.",
        |ctx, a| text_around(ctx, a, false),
    ),
    r(
        "TEXTSPLIT",
        2,
        6,
        "text, col_delimiter, [row_delimiter], [ignore_empty], [match_mode], [pad_with]",
        "Text split into columns and rows.",
        textsplit,
    ),
    s("VALUETOTEXT", 1, 2, "value, [format]", "Any value as text.", |ctx, a| {
        let strict = opt_num(ctx, a, 1, 0.0)? == 1.0;
        Ok(Value::text(value_text(&a[0], strict)))
    }),
    r("ARRAYTOTEXT", 1, 2, "array, [format]", "An array as text.", |ctx, a| {
        let v = ctx.eval(&a[0]);
        let array = match ctx.array(v) {
            Ok(x) => x,
            Err(e) => return Val::error(e),
        };
        let strict = a
            .get(1)
            .map(|e| {
                let v = ctx.eval(e);
                let v = ctx.single(v);
                ctx.number(&v).unwrap_or(0.0) == 1.0
            })
            .unwrap_or(false);
        let text = if strict {
            let rows: Vec<String> = array
                .rows_iter()
                .map(|row| row.iter().map(|v| value_text(v, true)).collect::<Vec<_>>().join(","))
                .collect();
            format!("{{{}}}", rows.join(";"))
        } else {
            array.data.iter().map(|v| value_text(v, false)).collect::<Vec<_>>().join(", ")
        };
        Val::text(text)
    }),
    s("ASC", 1, 1, "text", "Full width characters as half width.", |_, a| {
        Ok(Value::text(text_of(&a[0])?))
    }),
];

fn case_mapped(table: &[(u32, u32)], c: char) -> char {
    table
        .binary_search_by_key(&(c as u32), |(from, _)| *from)
        .ok()
        .and_then(|i| char::from_u32(table[i].1))
        .unwrap_or(c)
}

/// A character in upper case the way Excel maps it: one character to one, and only the characters its
/// tables know.
pub fn simple_upper(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_uppercase()
    } else {
        case_mapped(super::casemap::UPPER, c)
    }
}

/// A character in lower case the way Excel maps it. The capital sigma is the final sigma unless a letter
/// follows it.
pub fn simple_lower(c: char, letter_follows: bool) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else if c == '\u{3A3}' {
        if letter_follows {
            '\u{3C3}'
        } else {
            '\u{3C2}'
        }
    } else {
        case_mapped(super::casemap::LOWER, c)
    }
}

fn lower_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .map(|i| simple_lower(chars[i], chars.get(i + 1).is_some_and(|c| c.is_alphabetic())))
        .collect()
}

/// PROPER: the first letter of each run of letters in capitals and the rest in lower case.
fn proper_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for (i, c) in chars.iter().enumerate() {
        if c.is_alphabetic() {
            let letter_follows = chars.get(i + 1).is_some_and(|n| n.is_alphabetic());
            // A first letter goes through lower case on the way, which is how a dotted capital I becomes I.
            out.push(if after_letter {
                simple_lower(*c, letter_follows)
            } else {
                simple_upper(simple_lower(*c, true))
            });
            after_letter = true;
        } else {
            out.push(*c);
            after_letter = false;
        }
    }
    out
}

fn limit(text: String) -> Result<Value, ErrorKind> {
    if text.chars().count() > 32_767 {
        Err(ErrorKind::Value)
    } else {
        Ok(Value::text(text))
    }
}

fn value_text(v: &Value, strict: bool) -> String {
    match v {
        Value::Text(t) if strict => format!("\"{}\"", t.replace('"', "\"\"")),
        other => crate::eval::to_text(other).unwrap_or_else(|e| e.text().to_owned()),
    }
}

fn join(ctx: &mut Ctx, args: &[Expr], delimiter: &str, skip_empty: bool) -> Val {
    let mut parts = Vec::new();
    for arg in args {
        let v = ctx.eval(arg);
        let array = match ctx.array(v) {
            Ok(a) => a,
            Err(e) => return Val::error(e),
        };
        for value in array.data {
            if let Value::Error(e) = value {
                return Val::error(e);
            }
            if skip_empty && value.is_empty() {
                continue;
            }
            parts.push(value.to_text());
        }
    }
    match limit(parts.join(delimiter)) {
        Ok(v) => Val::Value(v),
        Err(e) => Val::error(e),
    }
}

fn textjoin(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let delimiters = ctx.eval(&args[0]);
    let delimiters = match ctx.array(delimiters) {
        Ok(a) => a.data.iter().map(Value::to_text).collect::<Vec<_>>(),
        Err(e) => return Val::error(e),
    };
    let ignore = ctx.eval(&args[1]);
    let ignore = ctx.single(ignore);
    let ignore = match crate::eval::to_bool(&ignore) {
        Ok(b) => b,
        Err(e) => return Val::error(e),
    };
    let mut parts = Vec::new();
    for arg in &args[2..] {
        let v = ctx.eval(arg);
        let array = match ctx.array(v) {
            Ok(a) => a,
            Err(e) => return Val::error(e),
        };
        for value in array.data {
            if let Value::Error(e) = value {
                return Val::error(e);
            }
            if ignore && (value.is_empty() || matches!(&value, Value::Text(t) if t.is_empty())) {
                continue;
            }
            parts.push(value.to_text());
        }
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 && !delimiters.is_empty() {
            out.push_str(&delimiters[(i - 1) % delimiters.len()]);
        }
        out.push_str(part);
    }
    match limit(out) {
        Ok(v) => Val::Value(v),
        Err(e) => Val::error(e),
    }
}

fn find(ctx: &mut Ctx, a: &[Value], case_sensitive: bool) -> Result<Value, ErrorKind> {
    let needle = text_of(&a[0])?;
    let hay = text_of(&a[1])?;
    let start = opt_num(ctx, a, 2, 1.0)?.trunc();
    let hay_chars: Vec<char> = hay.chars().collect();
    if start < 1.0
        || start as usize > hay_chars.len() + usize::from(needle.is_empty())
        || start as usize > hay_chars.len() && !needle.is_empty()
    {
        return Err(ErrorKind::Value);
    }
    let from = start as usize - 1;
    if needle.is_empty() {
        return Ok(Value::Number(start));
    }
    if case_sensitive {
        let tail: String = hay_chars[from..].iter().collect();
        return tail
            .find(&needle)
            .map(|byte| Value::Number((from + tail[..byte].chars().count() + 1) as f64))
            .ok_or(ErrorKind::Value);
    }
    for i in from..hay_chars.len() {
        for end in i..=hay_chars.len() {
            let candidate: String = hay_chars[i..end].iter().collect();
            if super::criteria::wildcard_match(&needle, &candidate) {
                return Ok(Value::Number(i as f64 + 1.0));
            }
        }
    }
    Err(ErrorKind::Value)
}

/// TEXTBEFORE and TEXTAFTER. The delimiter may be an array, which means any of these, so it is not
/// lifted over; the text and the other arguments are.
fn text_around(ctx: &mut Ctx, args: &[Expr], before: bool) -> Val {
    let delimiters = ctx.eval(&args[1]);
    let delimiters: Vec<String> = match ctx.array(delimiters) {
        Ok(a) => a.data.iter().map(Value::to_text).collect(),
        Err(e) => return Val::error(e),
    };
    let mut all = vec![ctx.eval(&args[0])];
    all.extend(args[2..].iter().map(|e| ctx.eval(e)));
    ctx.lift_vals(all, |ctx, v| {
        text_around_one(ctx, v, &delimiters, before).unwrap_or_else(Value::Error)
    })
}

/// Where the delimiters are in a text, as the first character and the number of characters of each, left to
/// right, a longer delimiter being taken where two start at the same place.
fn delimiter_places(hay: &[char], needles: &[Vec<char>]) -> Vec<(usize, usize)> {
    if needles.iter().all(Vec::is_empty) {
        return (0..=hay.len()).map(|at| (at, 0)).collect();
    }
    let mut places = Vec::new();
    let mut at = 0;
    while at < hay.len() {
        let longest = needles
            .iter()
            .filter(|n| !n.is_empty() && hay[at..].starts_with(n))
            .map(Vec::len)
            .max();
        match longest {
            Some(length) => {
                places.push((at, length));
                at += length;
            }
            None => at += 1,
        }
    }
    places
}

/// `a` is the text, then the instance, the match mode, whether the end of the text matches and what to
/// give when nothing does.
fn text_around_one(
    ctx: &mut Ctx,
    a: &[Value],
    delimiters: &[String],
    before: bool,
) -> Result<Value, ErrorKind> {
    let text = text_of(&a[0])?;
    let given = |ctx: &mut Ctx, i: usize, default: f64| match opt(a, i) {
        Some(v) => num(ctx, v),
        None => Ok(default),
    };
    let instance = given(ctx, 1, 1.0)?.trunc() as i64;
    let ignore_case = given(ctx, 2, 0.0)? == 1.0;
    let match_end = given(ctx, 3, 0.0)? == 1.0;
    if instance == 0 {
        return Err(ErrorKind::Value);
    }
    let fold = |c: char| if ignore_case { simple_lower(c, true) } else { c };
    let chars: Vec<char> = text.chars().collect();
    let folded: Vec<char> = chars.iter().map(|c| fold(*c)).collect();
    let needles: Vec<Vec<char>> =
        delimiters.iter().map(|d| d.chars().map(fold).collect()).collect();
    let mut places = delimiter_places(&folded, &needles);
    if match_end {
        if instance > 0 {
            places.push((chars.len(), 0));
        } else {
            places.insert(0, (0, 0));
        }
    }
    let pick = if instance > 0 {
        places.get(instance as usize - 1)
    } else {
        places.len().checked_sub((-instance) as usize).and_then(|i| places.get(i))
    };
    match pick {
        Some(&(at, length)) => {
            let piece =
                if before { &chars[..at] } else { &chars[(at + length).min(chars.len())..] };
            Ok(Value::text(piece.iter().collect::<String>()))
        }
        None => match a.get(4) {
            Some(v) => Ok(v.clone()),
            None => Err(ErrorKind::NA),
        },
    }
}

fn textsplit(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut scalars = Vec::new();
    for e in args {
        let v = ctx.eval(e);
        scalars.push(v);
    }
    let text = match ctx.single(scalars[0].clone()) {
        Value::Error(e) => return Val::error(e),
        v => v.to_text(),
    };
    // A delimiter that is left out is not one; one that is given as empty text or an empty cell is refused.
    let delims = |ctx: &mut Ctx, at: usize| -> Result<Vec<String>, ErrorKind> {
        match (args.get(at), scalars.get(at)) {
            (Some(Expr::Missing), _) | (None, _) | (_, None) => Ok(Vec::new()),
            (_, Some(v)) => {
                let all = ctx.array(v.clone())?.data;
                if all.iter().any(|d| d.is_empty() || matches!(d, Value::Text(t) if t.is_empty())) {
                    return Err(ErrorKind::Value);
                }
                Ok(all.iter().map(Value::to_text).collect())
            }
        }
    };
    let cols = match delims(ctx, 1) {
        Ok(d) => d,
        Err(e) => return Val::error(e),
    };
    let rows = match delims(ctx, 2) {
        Ok(d) => d,
        Err(e) => return Val::error(e),
    };
    if cols.is_empty() && rows.is_empty() {
        return Val::error(ErrorKind::Value);
    }
    let ignore_empty = scalars
        .get(3)
        .map(|v| ctx.single(v.clone()))
        .is_some_and(|v| crate::eval::to_bool(&v).unwrap_or(false));
    let ignore_case = scalars
        .get(4)
        .map(|v| ctx.single(v.clone()))
        .is_some_and(|v| ctx.number(&v).unwrap_or(0.0) == 1.0);
    let pad = scalars.get(5).map(|v| ctx.single(v.clone())).unwrap_or(Value::Error(ErrorKind::NA));
    let split = |s: &str, delims: &[String]| -> Vec<String> {
        if delims.is_empty() {
            return vec![s.to_owned()];
        }
        let mut out = Vec::new();
        let mut current = String::new();
        let hay = if ignore_case { s.to_lowercase() } else { s.to_owned() };
        let mut i = 0;
        'outer: while i < s.len() {
            for d in delims {
                let d2 = if ignore_case { d.to_lowercase() } else { d.clone() };
                if hay[i..].starts_with(&d2) {
                    out.push(std::mem::take(&mut current));
                    i += d2.len();
                    continue 'outer;
                }
            }
            let c = s[i..].chars().next().unwrap_or(' ');
            current.push(c);
            i += c.len_utf8();
        }
        out.push(current);
        if ignore_empty {
            out.retain(|p| !p.is_empty());
        }
        out
    };
    let lines = split(&text, &rows);
    let table: Vec<Vec<Value>> = lines
        .iter()
        .map(|line| split(line, &cols).into_iter().map(Value::text).collect())
        .collect();
    let width = table.iter().map(Vec::len).max().unwrap_or(1);
    let mut data = Vec::new();
    for row in &table {
        for c in 0..width {
            data.push(row.get(c).cloned().unwrap_or_else(|| pad.clone()));
        }
    }
    Val::Array(Array::new(table.len(), width, data))
}

/// Windows-1252, which is what CHAR and CODE use on Windows.
pub fn windows_1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013,
        0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(HIGH[(b - 0x80) as usize] as u32).unwrap_or('?'),
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn text_functions_answer_what_excel_answers() {
        check(&[
            ("LEFT(\"Hello\",2)", "He"),
            ("RIGHT(\"Hello\",3)", "llo"),
            ("MID(\"Hello\",2,3)", "ell"),
            ("LEN(\"héllo\")", "5"),
            ("FIND(\"l\",\"Hello\")", "3"),
            ("FIND(\"L\",\"Hello\")", "#VALUE!"),
            ("SEARCH(\"L\",\"Hello\")", "3"),
            ("SEARCH(\"l?o\",\"Hello\")", "3"),
            ("SUBSTITUTE(\"a-b-c\",\"-\",\"+\")", "a+b+c"),
            ("SUBSTITUTE(\"a-b-c\",\"-\",\"+\",2)", "a-b+c"),
            ("REPLACE(\"abcdef\",2,3,\"X\")", "aXef"),
            ("UPPER(\"abc\")", "ABC"),
            ("PROPER(\"hello wORLD o'neil\")", "Hello World O'Neil"),
            ("TRIM(\"  a   b  \")", "a b"),
            ("REPT(\"ab\",3)", "ababab"),
            ("EXACT(\"a\",\"A\")", "FALSE"),
            ("TEXT(1234.5,\"#,##0.00\")", "1,234.50"),
            ("TEXT(0.25,\"0%\")", "25%"),
            ("TEXT(46318,\"yyyy-mm-dd\")", "2026-10-23"),
            ("VALUE(\"1,234.5\")", "1234.5"),
            ("VALUE(\"abc\")", "#VALUE!"),
            ("FIXED(1234.567,1)", "1,234.6"),
            ("DOLLAR(1234.567)", "$1,234.57"),
            ("DOLLAR(-5)", "($5.00)"),
            ("CHAR(65)", "A"),
            ("CODE(\"A\")", "65"),
            ("CONCAT(\"a\",\"b\",1)", "ab1"),
            ("TEXTJOIN(\", \",TRUE,\"a\",\"\",\"b\")", "a, b"),
            ("TEXTBEFORE(\"a-b-c\",\"-\")", "a"),
            ("TEXTAFTER(\"a-b-c\",\"-\",-1)", "c"),
            ("TEXTAFTER(\"abc\",\"x\")", "#N/A"),
            ("\"a\"&1&TRUE", "a1TRUE"),
            ("NUMBERVALUE(\"2.500,27\",\",\",\".\")", "2500.27"),
        ]);
    }
}
