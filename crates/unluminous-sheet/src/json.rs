//! JSON as a table: an array of objects becomes a table with the keys as its headers, and an array of
//! arrays becomes rows.
//!
//! This is the shape Excel's own JSON import gives for the same two documents. Values keep their JSON types
//! (a string that looks like a number stays text), and an object or array inside a value is written into its
//! cell as JSON text.

use crate::coord::CellRef;
use crate::model::{Cell, Sheet, Workbook};
use crate::styles::{FontEdit, StyleChange};
use crate::value::Value;

/// One JSON value. Objects keep their keys in the order the document gave them.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// Parse a JSON document.
pub fn parse(text: &str) -> Result<Json, String> {
    let chars: Vec<char> = text.trim_start_matches('\u{FEFF}').chars().collect();
    let mut at = 0;
    let value = value(&chars, &mut at)?;
    skip_space(&chars, &mut at);
    if at < chars.len() {
        return Err(format!(
            "unexpected {:?} after the end of the JSON value, at character {at}",
            chars[at]
        ));
    }
    Ok(value)
}

fn skip_space(chars: &[char], at: &mut usize) {
    while chars.get(*at).is_some_and(|c| c.is_whitespace()) {
        *at += 1;
    }
}

fn value(chars: &[char], at: &mut usize) -> Result<Json, String> {
    skip_space(chars, at);
    match chars.get(*at) {
        None => Err("the JSON ends before a value".to_owned()),
        Some('{') => object(chars, at),
        Some('[') => array(chars, at),
        Some('"') => string(chars, at).map(Json::Text),
        Some(c) if c.is_ascii_digit() || *c == '-' => number(chars, at),
        Some(_) => word(chars, at),
    }
}

fn object(chars: &[char], at: &mut usize) -> Result<Json, String> {
    *at += 1;
    let mut members = Vec::new();
    loop {
        skip_space(chars, at);
        match chars.get(*at) {
            Some('}') => {
                *at += 1;
                return Ok(Json::Object(members));
            }
            Some(',') => *at += 1,
            Some('"') => {
                let key = string(chars, at)?;
                skip_space(chars, at);
                if chars.get(*at) != Some(&':') {
                    return Err(format!(
                        "expected a colon after the key {key:?}, at character {}",
                        *at
                    ));
                }
                *at += 1;
                members.push((key, value(chars, at)?));
            }
            _ => return Err(format!("expected a key or a closing brace, at character {}", *at)),
        }
    }
}

fn array(chars: &[char], at: &mut usize) -> Result<Json, String> {
    *at += 1;
    let mut items = Vec::new();
    loop {
        skip_space(chars, at);
        match chars.get(*at) {
            Some(']') => {
                *at += 1;
                return Ok(Json::Array(items));
            }
            Some(',') => *at += 1,
            None => return Err("the JSON ends inside an array".to_owned()),
            _ => items.push(value(chars, at)?),
        }
    }
}

fn string(chars: &[char], at: &mut usize) -> Result<String, String> {
    *at += 1;
    let mut out = String::new();
    while let Some(&c) = chars.get(*at) {
        *at += 1;
        match c {
            '"' => return Ok(out),
            '\\' => {
                let escape = chars.get(*at).copied().ok_or("the JSON ends inside an escape")?;
                *at += 1;
                match escape {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => out.push(unicode_escape(chars, at)?),
                    other => out.push(other),
                }
            }
            c => out.push(c),
        }
    }
    Err("the JSON ends inside a string".to_owned())
}

/// The character a `\uXXXX` escape names, joining a surrogate pair.
fn unicode_escape(chars: &[char], at: &mut usize) -> Result<char, String> {
    let read_unit = |at: &mut usize| -> Result<u32, String> {
        let hex: String =
            chars.get(*at..*at + 4).ok_or("the JSON ends inside a \\u escape")?.iter().collect();
        *at += 4;
        u32::from_str_radix(&hex, 16).map_err(|_| format!("{hex:?} is not a \\u escape"))
    };
    let high = read_unit(at)?;
    if (0xD800..0xDC00).contains(&high)
        && chars.get(*at) == Some(&'\\')
        && chars.get(*at + 1) == Some(&'u')
    {
        *at += 2;
        let low = read_unit(at)?;
        let combined = 0x10000 + ((high - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
        return Ok(char::from_u32(combined).unwrap_or('\u{FFFD}'));
    }
    Ok(char::from_u32(high).unwrap_or('\u{FFFD}'))
}

fn number(chars: &[char], at: &mut usize) -> Result<Json, String> {
    let start = *at;
    while chars.get(*at).is_some_and(|c| c.is_ascii_digit() || "+-.eE".contains(*c)) {
        *at += 1;
    }
    let text: String = chars[start..*at].iter().collect();
    text.parse::<f64>().map(Json::Number).map_err(|_| format!("{text:?} is not a number"))
}

fn word(chars: &[char], at: &mut usize) -> Result<Json, String> {
    let start = *at;
    while chars.get(*at).is_some_and(|c| c.is_ascii_alphabetic()) {
        *at += 1;
    }
    match chars[start..*at].iter().collect::<String>().as_str() {
        "true" => Ok(Json::Bool(true)),
        "false" => Ok(Json::Bool(false)),
        "null" => Ok(Json::Null),
        other => Err(format!("unexpected {other:?} at character {start}")),
    }
}

/// A value as compact JSON text.
pub fn compact(json: &Json) -> String {
    match json {
        Json::Null => "null".to_owned(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => crate::formula::print::number(*n),
        Json::Text(t) => format!("{t:?}"),
        Json::Array(items) => {
            format!("[{}]", items.iter().map(compact).collect::<Vec<_>>().join(","))
        }
        Json::Object(members) => format!(
            "{{{}}}",
            members
                .iter()
                .map(|(k, v)| format!("{k:?}:{}", compact(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

/// A cell value for a JSON value: scalars keep their types, and anything nested is JSON text.
fn cell_value(json: &Json) -> Value {
    match json {
        Json::Null => Value::Empty,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => Value::Number(*n),
        Json::Text(t) => Value::text(t),
        nested => Value::text(compact(nested)),
    }
}

/// The list of rows a document holds: the document itself when it is an array, and otherwise the first
/// array in an object.
fn rows_of(json: &Json) -> (Option<String>, Vec<Json>) {
    match json {
        Json::Array(items) => (None, items.clone()),
        Json::Object(members) => {
            if let Some((key, Json::Array(items))) =
                members.iter().find(|(_, v)| matches!(v, Json::Array(_)))
            {
                return (Some(key.clone()), items.clone());
            }
            (None, vec![json.clone()])
        }
        scalar => (None, vec![scalar.clone()]),
    }
}

/// A workbook of one sheet from JSON text.
pub fn read(text: &str, sheet_name: &str) -> Result<Workbook, String> {
    let document = parse(text)?;
    let (key, rows) = rows_of(&document);
    let mut book = Workbook::new();
    let name = key.as_deref().unwrap_or(sheet_name);
    let mut sheet = Sheet::new(name.chars().take(31).collect::<String>());
    let bold = book
        .styles
        .derive(0, &StyleChange::Font(FontEdit { bold: Some(true), ..Default::default() }));
    let objects = rows.iter().any(|r| matches!(r, Json::Object(_)));
    let mut next_row = 0u32;
    let mut keys: Vec<String> = Vec::new();
    if objects {
        for row in &rows {
            if let Json::Object(members) = row {
                for (k, _) in members {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
            }
        }
        for (c, k) in keys.iter().enumerate() {
            sheet.set(
                CellRef::new(0, c as u32),
                Cell { value: Value::text(k), style: bold, ..Default::default() },
            );
        }
        next_row = 1;
    }
    for row in &rows {
        match row {
            Json::Object(members) => {
                for (k, v) in members {
                    let col = keys.iter().position(|x| x == k).unwrap_or(0) as u32;
                    sheet.set(CellRef::new(next_row, col), Cell::value(cell_value(v)));
                }
            }
            Json::Array(items) => {
                for (c, v) in items.iter().enumerate() {
                    sheet.set(CellRef::new(next_row, c as u32), Cell::value(cell_value(v)));
                }
            }
            scalar => sheet.set(CellRef::new(next_row, 0), Cell::value(cell_value(scalar))),
        }
        next_row += 1;
    }
    book.sheets = vec![sheet];
    Ok(book)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_array_of_objects_becomes_a_table_with_the_keys_as_headers() {
        let book = read(
            r#"[{"name":"Tea","qty":3,"ok":true},{"name":"Milk","extra":null,"qty":1.5}]"#,
            "data",
        )
        .unwrap();
        let s = &book.sheets[0];
        let at = |r, c| s.value_at(CellRef::new(r, c));
        assert_eq!(at(0, 0), Value::text("name"));
        assert_eq!(at(0, 1), Value::text("qty"));
        assert_eq!(at(0, 2), Value::text("ok"));
        assert_eq!(at(0, 3), Value::text("extra"));
        assert_eq!(at(1, 1), Value::Number(3.0));
        assert_eq!(at(1, 2), Value::Bool(true));
        assert_eq!(at(2, 1), Value::Number(1.5));
        assert_eq!(at(2, 3), Value::Empty);
    }

    #[test]
    fn an_array_of_arrays_becomes_rows_and_nested_values_become_text() {
        let book = read(r#"[["a",1,{"x":[1,2]}],["b","2"]]"#, "rows").unwrap();
        let s = &book.sheets[0];
        assert_eq!(s.value_at(CellRef::new(0, 2)), Value::text("{\"x\":[1,2]}"));
        assert_eq!(s.value_at(CellRef::new(1, 1)), Value::text("2"));
        assert_eq!(s.name, "rows");
    }

    #[test]
    fn an_object_holding_an_array_uses_the_array_and_names_the_sheet_after_its_key() {
        let book = read(r#"{"items":[{"a":1}],"total":1}"#, "x").unwrap();
        assert_eq!(book.sheets[0].name, "items");
        assert_eq!(book.sheets[0].value_at(CellRef::new(1, 0)), Value::Number(1.0));
    }

    #[test]
    fn escapes_and_errors_read_as_the_standard_says() {
        assert_eq!(parse(r#""aé\n😀""#).unwrap(), Json::Text("a\u{e9}\n\u{1F600}".to_owned()));
        assert!(parse("[1,").unwrap_err().contains("ends"));
        assert!(parse("[1] x").unwrap_err().contains("after the end"));
    }
}
