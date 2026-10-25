//! The text functions that count bytes, PHONETIC, and BAHTTEXT.
//!
//! **The byte functions.** `LENB`, `LEFTB`, `MIDB`, `RIGHTB`, `FINDB`, `SEARCHB` and `REPLACEB` count a
//! double byte character as two only when Excel's language is one of the double byte ones (Chinese,
//! Japanese, Korean). In every other language they count characters, which is what this Excel does: measured,
//! `LENB("日本")` is 2 and `LEFTB("日本語",2)` is `日本`. They are the character functions under another name.
//! `DBCS` and `JIS` turn half width characters into full width ones in a double byte language and answer
//! their argument unchanged elsewhere, which is the measured answer here.
//!
//! **PHONETIC** answers the phonetic guide a cell carries, the `rPh` runs of its string, joined. A cell with
//! no guide is `#N/A`, which is what this Excel answers for every cell, with or without one; the reader keeps
//! the runs so a file written for a Japanese Excel gets them back.
//!
//! **BAHTTEXT** writes a number as Thai words for an amount of baht and satang.

use super::{find, num, s, text_of, Function, Imp};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::functions::math::{round_to, Rounding};
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s("LENB", 1, 1, "text", "How many bytes; characters in a single byte language.", |c, a| {
        like("LEN", c, a)
    }),
    s(
        "LEFTB",
        1,
        2,
        "text, [num_bytes]",
        "The first bytes; characters in a single byte language.",
        |c, a| like("LEFT", c, a),
    ),
    s(
        "RIGHTB",
        1,
        2,
        "text, [num_bytes]",
        "The last bytes; characters in a single byte language.",
        |c, a| like("RIGHT", c, a),
    ),
    s(
        "MIDB",
        3,
        3,
        "text, start_num, num_bytes",
        "Bytes from the middle; characters in a single byte language.",
        |c, a| like("MID", c, a),
    ),
    s(
        "FINDB",
        2,
        3,
        "find_text, within_text, [start_num]",
        "Where one text starts in another, case sensitive, counting bytes.",
        |c, a| like("FIND", c, a),
    ),
    s(
        "SEARCHB",
        2,
        3,
        "find_text, within_text, [start_num]",
        "Where one text starts in another, with wildcards, counting bytes.",
        |c, a| like("SEARCH", c, a),
    ),
    s(
        "REPLACEB",
        4,
        4,
        "old_text, start_num, num_bytes, new_text",
        "Replaces bytes at a position; characters in a single byte language.",
        |c, a| like("REPLACE", c, a),
    ),
    s(
        "DBCS",
        1,
        1,
        "text",
        "Full width characters for half width ones in a double byte language.",
        |_, a| Ok(Value::text(text_of(&a[0])?)),
    ),
    s(
        "JIS",
        1,
        1,
        "text",
        "Full width characters for half width ones in a Japanese language.",
        // An English Excel does not have JIS: Excel 16 reads it as an unknown name whatever it is given.
        |_, _| Err(ErrorKind::Name),
    ),
    super::r("PHONETIC", 1, 1, "reference", "The phonetic guide of a cell's text.", phonetic),
    s("BAHTTEXT", 1, 1, "number", "A number as Thai words for baht and satang.", |ctx, a| {
        bahttext(num(ctx, &a[0])?)
    }),
];

/// The character function this byte function is, called with the same arguments.
fn like(name: &str, ctx: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    match find(name).map(|f| f.imp) {
        Some(Imp::Scalar(f)) => f(ctx, args),
        _ => Err(ErrorKind::Name),
    }
}

// ---------------------------------------------------------------------------------------------------
// PHONETIC

/// PHONETIC: the phonetic guide of the first cell of a reference.
fn phonetic(ctx: &mut Ctx, args: &[Expr]) -> Val {
    match ctx.eval(&args[0]) {
        Val::Ref(area) if !area.is_3d() => {
            let first = area.range.first;
            let rich = ctx.book.sheets[area.sheet].cell(first).and_then(|c| c.rich.clone());
            match rich.as_deref().map(phonetic_runs).filter(|runs| !runs.is_empty()) {
                Some(runs) => Val::text(runs),
                None => Val::error(ErrorKind::NA),
            }
        }
        Val::Value(Value::Error(e)) => Val::error(e),
        _ => Val::error(ErrorKind::Value),
    }
}

/// The phonetic runs of a string item's XML, joined in the order the file lists them.
pub fn phonetic_runs(xml: &str) -> String {
    let Ok(doc) = roxmltree::Document::parse(xml.trim()) else { return String::new() };
    let mut out = String::new();
    for run in doc.root_element().children().filter(|n| n.tag_name().name() == "rPh") {
        if let Some(t) = run.children().find(|n| n.tag_name().name() == "t") {
            out.push_str(t.text().unwrap_or(""));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// BAHTTEXT

const DIGITS: [&str; 10] = ["ศูนย์", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"];
/// The word for the place of a digit: tens, hundreds, thousands, ten thousands, hundred thousands.
const PLACES: [&str; 6] = ["", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน"];

/// BAHTTEXT: baht and satang as Thai words, rounded to the satang.
fn bahttext(n: f64) -> Result<Value, ErrorKind> {
    let rounded = if n.abs() < 1e15 { round_to(n.abs(), 2.0, Rounding::Half) } else { n.abs() };
    let (baht, satang) = split_baht(rounded);
    let mut out = String::new();
    if n < 0.0 {
        out.push_str("ลบ");
    }
    if baht != "0" || satang == 0 {
        out.push_str(&thai_number(&baht));
        out.push_str("บาท");
    }
    if satang > 0 {
        out.push_str(&thai_number(&satang.to_string()));
        out.push_str("สตางค์");
    } else {
        out.push_str("ถ้วน");
    }
    Ok(Value::text(out))
}

/// The whole baht as decimal digits and the satang as 0 to 99. A number of fifteen digits or more has
/// only the fifteen significant ones, as Excel reads it.
fn split_baht(rounded: f64) -> (String, u32) {
    if rounded >= 1e15 {
        let (digits, exponent) = crate::value::significant_digits(rounded, 15);
        let width = exponent as usize + 1;
        return (format!("{digits}{}", "0".repeat(width.saturating_sub(digits.len()))), 0);
    }
    let text = format!("{rounded:.2}");
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, "00"));
    (whole.to_owned(), fraction.parse().unwrap_or(0))
}

/// A whole number written as decimal digits, in Thai: groups of six digits each followed by `ล้าน`
/// (million) except the last.
fn thai_number(digits: &str) -> String {
    let digits: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.iter().all(|d| *d == 0) {
        return DIGITS[0].to_owned();
    }
    let mut groups: Vec<&[u32]> = Vec::new();
    let first = digits.len() % 6;
    if first > 0 {
        groups.push(&digits[..first]);
    }
    groups.extend(digits[first..].chunks(6));
    let mut out = String::new();
    let mut above = false;
    for (i, group) in groups.iter().enumerate() {
        out.push_str(&thai_group(group, above));
        above |= group.iter().any(|d| *d != 0);
        if i + 1 < groups.len() {
            out.push_str("ล้าน");
        }
    }
    out
}

/// Up to six digits in Thai. A final 1 is `เอ็ด` when anything stands above it, a tens digit of 2 is `ยี่`
/// and a tens digit of 1 is just `สิบ`.
fn thai_group(group: &[u32], mut above: bool) -> String {
    let mut out = String::new();
    for (i, d) in group.iter().enumerate() {
        let place = group.len() - 1 - i;
        match (*d, place) {
            (0, _) => continue,
            (1, 0) if above => out.push_str("เอ็ด"),
            (1, 1) => out.push_str(PLACES[1]),
            (2, 1) => {
                out.push_str("ยี่");
                out.push_str(PLACES[1]);
            }
            (d, place) => {
                out.push_str(DIGITS[d as usize]);
                out.push_str(PLACES[place]);
            }
        }
        above = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions::testing::{check, eval};

    #[test]
    fn the_byte_functions_count_characters_in_a_single_byte_language() {
        check(&[
            ("LENB(\"日本\")", "2"),
            ("LEFTB(\"日本語\",2)", "日本"),
            ("RIGHTB(\"日本語\",2)", "本語"),
            ("MIDB(\"日本語\",2,1)", "本"),
            ("FINDB(\"本\",\"日本語\")", "2"),
            ("SEARCHB(\"B\",\"abcabc\")", "2"),
            ("REPLACEB(\"日本語\",2,1,\"X\")", "日X語"),
            ("DBCS(\"ｱｲｳ\")", "ｱｲｳ"),
            ("JIS(\"abc\")", "#NAME?"),
        ]);
    }

    #[test]
    fn bahttext_writes_baht_and_satang() {
        check(&[
            ("BAHTTEXT(1234)", "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน"),
            ("BAHTTEXT(0.25)", "ยี่สิบห้าสตางค์"),
            ("BAHTTEXT(-0.5)", "ลบห้าสิบสตางค์"),
            ("BAHTTEXT(1000001000000)", "หนึ่งล้านเอ็ดล้านบาทถ้วน"),
            ("BAHTTEXT(0.015)", "สองสตางค์"),
            ("BAHTTEXT(0)", "ศูนย์บาทถ้วน"),
        ]);
        assert_eq!(eval("BAHTTEXT(\"x\")"), Value::Error(ErrorKind::Value));
    }

    #[test]
    fn the_phonetic_runs_of_a_string_are_joined_in_order() {
        let xml = "<si><t>東京都</t><rPh sb=\"0\" eb=\"2\"><t>トウキョウ</t></rPh><rPh sb=\"2\" eb=\"3\"><t>ト</t></rPh></si>";
        assert_eq!(phonetic_runs(xml), "トウキョウト");
        assert_eq!(phonetic_runs("<si><t>plain</t></si>"), "");
    }
}
