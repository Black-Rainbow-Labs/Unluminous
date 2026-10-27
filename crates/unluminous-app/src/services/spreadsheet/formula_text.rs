//! Reading a formula while it is being typed, which is text that is not yet a formula: where its cell
//! references are (to outline them in colour), which function the caret is inside (for the argument hint),
//! the word before the caret (for the list of functions), whether the caret is where a reference may be
//! pointed in, and the four forms F4 steps a reference through.
//!
//! Positions are counted in characters, which is what `egui`'s text cursor counts in.

use unluminous_sheet::coord::Range;

/// One reference in a formula's text.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    /// Where it is, in characters, end exclusive, sheet name included.
    pub start: usize,
    pub end: usize,
    pub sheet: Option<String>,
    pub range: Range,
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '\\'
}

/// Every cell reference in a formula's text, outside strings.
pub fn references(text: &str) -> Vec<Reference> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
            i += 1;
            continue;
        }
        if quoted {
            i += 1;
            continue;
        }
        // A reference starts where a name could start: not after a letter, a digit or a dot.
        let starts = i == 0 || !is_name_char(chars[i - 1]);
        if starts && (c == '$' || c.is_ascii_alphabetic() || c == '\'') {
            if let Some((end, sheet, range)) = reference_at(&chars, i) {
                out.push(Reference { start: i, end, sheet, range });
                i = end;
                continue;
            }
        }
        // Skip the rest of a word, so `LOG10` is not read as `G10`.
        if is_name_char(c) {
            while i < chars.len() && is_name_char(chars[i]) {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// A reference beginning at `i`: `A1`, `$A$1`, `A1:B2`, `Sheet1!A1`, `'My sheet'!A1:B2`.
fn reference_at(chars: &[char], i: usize) -> Option<(usize, Option<String>, Range)> {
    let mut at = i;
    let mut sheet = None;
    // A sheet name, quoted or plain, ended by `!`.
    if chars[at] == '\'' {
        let mut name = String::new();
        let mut j = at + 1;
        while j < chars.len() {
            if chars[j] == '\'' {
                if chars.get(j + 1) == Some(&'\'') {
                    name.push('\'');
                    j += 2;
                    continue;
                }
                break;
            }
            name.push(chars[j]);
            j += 1;
        }
        if chars.get(j + 1) != Some(&'!') {
            return None;
        }
        sheet = Some(name);
        at = j + 2;
    } else {
        let mut j = at;
        while j < chars.len() && is_name_char(chars[j]) {
            j += 1;
        }
        if chars.get(j) == Some(&'!') && j > at {
            sheet = Some(chars[at..j].iter().collect());
            at = j + 1;
        }
    }
    if let Some((end, range)) = whole_lines_at(chars, at) {
        return Some((end, sheet, range));
    }
    let (end, first) = cell_at(chars, at)?;
    if chars.get(end) == Some(&':') {
        if let Some((end2, last)) = cell_at(chars, end + 1) {
            if !chars.get(end2).is_some_and(|c| is_name_char(*c) || *c == '(') {
                return Some((end2, sheet, Range::new(first, last)));
            }
        }
    }
    if chars.get(end).is_some_and(|c| is_name_char(*c) || *c == '(') {
        return None;
    }
    Some((end, sheet, Range::cell(first)))
}

/// Whole columns `A:C` or whole rows `3:5`, either end possibly with a `$`, beginning at `i`.
fn whole_lines_at(chars: &[char], i: usize) -> Option<(usize, Range)> {
    use unluminous_sheet::coord::{column_number, MAX_COLS, MAX_ROWS};
    let part = |from: usize, letters: bool| -> Option<(usize, String)> {
        let mut j = from;
        if chars.get(j) == Some(&'$') {
            j += 1;
        }
        let start = j;
        while j < chars.len()
            && (if letters { chars[j].is_ascii_alphabetic() } else { chars[j].is_ascii_digit() })
        {
            j += 1;
        }
        (j > start).then(|| (j, chars[start..j].iter().collect()))
    };
    for letters in [true, false] {
        let Some((end, a)) = part(i, letters) else { continue };
        if chars.get(end) != Some(&':') {
            continue;
        }
        let Some((end2, b)) = part(end + 1, letters) else { continue };
        if chars.get(end2).is_some_and(|c| is_name_char(*c) || *c == '(') {
            continue;
        }
        let range = if letters {
            let (a, b) = (column_number(&a)?, column_number(&b)?);
            Range::new(
                unluminous_sheet::CellRef::new(0, a),
                unluminous_sheet::CellRef::new(MAX_ROWS - 1, b),
            )
        } else {
            let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
            if a == 0 || b == 0 || a > MAX_ROWS || b > MAX_ROWS {
                return None;
            }
            Range::new(
                unluminous_sheet::CellRef::new(a - 1, 0),
                unluminous_sheet::CellRef::new(b - 1, MAX_COLS - 1),
            )
        };
        return Some((end2, range));
    }
    None
}

/// One cell, `$A$1`, beginning at `i`.
fn cell_at(chars: &[char], i: usize) -> Option<(usize, unluminous_sheet::CellRef)> {
    let mut j = i;
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let letters_start = j;
    while j < chars.len() && chars[j].is_ascii_alphabetic() && j - letters_start < 3 {
        j += 1;
    }
    if j == letters_start {
        return None;
    }
    let letters: String = chars[letters_start..j].iter().collect();
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let digits_start = j;
    while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
    }
    if j == digits_start {
        return None;
    }
    let digits: String = chars[digits_start..j].iter().collect();
    let at = unluminous_sheet::CellRef::parse(&format!("{letters}{digits}"))?;
    Some((j, at))
}

/// The word being typed before the caret, when it could be the start of a function's name: its start and
/// the letters so far.
pub fn word_before(text: &str, caret: usize) -> Option<(usize, String)> {
    if !text.starts_with('=') {
        return None;
    }
    let chars: Vec<char> = text.chars().take(caret).collect();
    if quoted_at(&chars) {
        return None;
    }
    let mut start = chars.len();
    while start > 0
        && (chars[start - 1].is_alphanumeric()
            || chars[start - 1] == '.'
            || chars[start - 1] == '_')
    {
        start -= 1;
    }
    let word: String = chars[start..].iter().collect();
    if word.is_empty() || !word.chars().next().is_some_and(char::is_alphabetic) {
        return None;
    }
    // A word after a sheet's `!` or a reference's `$` is not a function.
    if start > 0 && matches!(chars[start - 1], '!' | '$' | '\'' | ':') {
        return None;
    }
    Some((start, word))
}

fn quoted_at(chars: &[char]) -> bool {
    chars.iter().filter(|c| **c == '"').count() % 2 == 1
}

/// The function whose brackets the caret is inside, and which of its arguments the caret is in, from 0.
pub fn function_at(text: &str, caret: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().take(caret).collect();
    let mut depth = 0i32;
    let mut argument = 0usize;
    let mut quoted = false;
    let mut i = chars.len();
    // Walk back, counting commas at this depth, to the bracket that opens it.
    while i > 0 {
        i -= 1;
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match c {
            ')' | '}' => depth += 1,
            '{' if depth == 0 => return None,
            '{' => depth -= 1,
            '(' if depth > 0 => depth -= 1,
            '(' => {
                let mut start = i;
                while start > 0
                    && (chars[start - 1].is_alphanumeric()
                        || chars[start - 1] == '.'
                        || chars[start - 1] == '_')
                {
                    start -= 1;
                }
                let name: String = chars[start..i].iter().collect();
                if name.is_empty() {
                    // A bracket that only groups: keep looking outward.
                    argument = 0;
                    continue;
                }
                return Some((name.to_ascii_uppercase(), argument));
            }
            ',' if depth == 0 => argument += 1,
            _ => {}
        }
    }
    None
}

/// Whether the caret is where a reference may be pointed in: after `=`, an operator, `(` or `,`.
pub fn points_at(text: &str, caret: usize) -> bool {
    if !text.starts_with('=') {
        return false;
    }
    let chars: Vec<char> = text.chars().collect();
    let before: Vec<char> =
        chars[..caret.min(chars.len())].iter().copied().filter(|c| *c != ' ').collect();
    if quoted_at(&before) {
        return false;
    }
    let after = chars.get(caret).copied();
    let ends_well = after.is_none_or(|c| {
        matches!(c, ')' | ',' | ' ' | '+' | '-' | '*' | '/' | '&' | '^' | '<' | '>' | '=')
    });
    ends_well
        && before.last().is_some_and(|c| {
            matches!(c, '=' | '(' | ',' | '+' | '-' | '*' | '/' | '^' | '&' | '<' | '>' | ':' | ';')
        })
}

/// The reference at or just before the caret stepped to its next form, A1 to $A$1 to A$1 to $A1 and back,
/// answering the new text and where the caret goes.
pub fn cycle_reference(text: &str, caret: usize) -> Option<(String, usize)> {
    let found = references(text).into_iter().find(|r| r.start <= caret && caret <= r.end)?;
    let chars: Vec<char> = text.chars().collect();
    let inside: String = chars[found.start..found.end].iter().collect();
    let (prefix, body) = match inside.rfind('!') {
        Some(at) => (inside[..=at].to_owned(), inside[at + 1..].to_owned()),
        None => (String::new(), inside.clone()),
    };
    let step = |cell: &str| -> String {
        let letters: String = cell.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        let digits: String = cell.chars().filter(|c| c.is_ascii_digit()).collect();
        let col_abs = cell.starts_with('$');
        let row_abs = cell.trim_start_matches('$').contains('$');
        match (col_abs, row_abs) {
            (false, false) => format!("${letters}${digits}"),
            (true, true) => format!("{letters}${digits}"),
            (false, true) => format!("${letters}{digits}"),
            (true, false) => format!("{letters}{digits}"),
        }
    };
    let stepped = match body.split_once(':') {
        Some((a, b)) => format!("{}:{}", step(a), step(b)),
        None => step(&body),
    };
    let replacement = format!("{prefix}{stepped}");
    let mut out: String = chars[..found.start].iter().collect();
    out.push_str(&replacement);
    out.extend(chars[found.end..].iter());
    Some((out, found.start + replacement.chars().count()))
}

/// The formula with its `index`th reference changed to `range`, as dragging a reference's coloured outline
/// changes it: the sheet name and each corner's `$` marks are kept. Answers the text and where the reference
/// now ends.
pub fn replace_reference(text: &str, index: usize, range: Range) -> Option<(String, usize)> {
    let found = references(text).into_iter().nth(index)?;
    let chars: Vec<char> = text.chars().collect();
    let inside: String = chars[found.start..found.end].iter().collect();
    let (prefix, body) = match inside.rfind('!') {
        Some(at) => (inside[..=at].to_owned(), inside[at + 1..].to_owned()),
        None => (String::new(), inside.clone()),
    };
    let marks = |cell: &str| -> (bool, bool) {
        (cell.starts_with('$'), cell.trim_start_matches('$').contains('$'))
    };
    let (a, b) = match body.split_once(':') {
        Some((a, b)) => (marks(a), marks(b)),
        None => (marks(&body), marks(&body)),
    };
    let write = |cell: unluminous_sheet::CellRef, (col, row): (bool, bool)| -> String {
        format!(
            "{}{}{}{}",
            if col { "$" } else { "" },
            unluminous_sheet::coord::column_name(cell.col),
            if row { "$" } else { "" },
            cell.row + 1
        )
    };
    let shown = match range.a1().contains(':') {
        // Whole columns and rows are written the plain way.
        true if range.is_whole_columns() || range.is_whole_rows() => range.a1(),
        true => format!("{}:{}", write(range.first, a), write(range.last, b)),
        false => write(range.first, a),
    };
    let replacement = format!("{prefix}{shown}");
    let mut out: String = chars[..found.start].iter().collect();
    out.push_str(&replacement);
    out.extend(chars[found.end..].iter());
    Some((out, found.start + replacement.chars().count()))
}

/// What Ctrl+Shift+A puts after a function's name: its arguments' names in brackets, `(number1,[number2],...)`.
pub fn argument_names(args: &str) -> String {
    format!("({})", args.split(", ").collect::<Vec<_>>().join(","))
}

/// A sheet's name as a formula writes it before `!`: quoted when it holds anything but letters, digits,
/// dots and underscores, or starts with a digit.
pub fn sheet_prefix(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    match plain {
        true => format!("{name}!"),
        false => format!("'{}'!", name.replace('\'', "''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_columns_and_rows_are_references_too() {
        let refs = references("=SUM(B:C)+SUM($3:5)");
        let texts: Vec<String> = refs.iter().map(|r| r.range.a1()).collect();
        assert_eq!(texts, vec!["B:C", "3:5"]);
    }

    #[test]
    fn a_reference_dragged_somewhere_else_keeps_its_sheet_and_its_dollars() {
        let to = |a: &str| Range::parse(a).unwrap();
        assert_eq!(
            replace_reference("=SUM(A1:B2)*$C$3", 1, to("D4")),
            Some(("=SUM(A1:B2)*$D$4".to_owned(), 16))
        );
        assert_eq!(
            replace_reference("=SUM(Data!A$1:B2)", 0, to("C1:D5")),
            Some(("=SUM(Data!C$1:D5)".to_owned(), 16))
        );
        assert_eq!(replace_reference("=A1+B1", 0, to("A1:A3")).unwrap().0, "=A1:A3+B1");
        assert_eq!(replace_reference("=A1", 3, to("A2")), None);
    }

    #[test]
    fn argument_names_and_sheet_prefixes_are_written_as_excel_writes_them() {
        assert_eq!(argument_names("number1, [number2], ..."), "(number1,[number2],...)");
        assert_eq!(sheet_prefix("Data"), "Data!");
        assert_eq!(sheet_prefix("My sheet"), "'My sheet'!");
        assert_eq!(sheet_prefix("2024"), "'2024'!");
        assert_eq!(sheet_prefix("Bob's"), "'Bob''s'!");
    }

    #[test]
    fn references_are_found_outside_strings_and_function_names() {
        let refs = references("=SUM(A1:B3)+LOG10(C2)&\"D4\"+'My sheet'!$E$5+Data!F6");
        let texts: Vec<String> =
            refs.iter().map(|r| format!("{:?}{}", r.sheet, r.range.a1())).collect();
        assert_eq!(texts, vec!["NoneA1:B3", "NoneC2", "Some(\"My sheet\")E5", "Some(\"Data\")F6"]);
        assert_eq!(refs[0].start, 5);
        assert_eq!(refs[0].end, 10);
    }

    #[test]
    fn the_caret_knows_its_function_and_argument() {
        assert_eq!(function_at("=IF(A1>2,SUM(B1,", 16), Some(("SUM".to_owned(), 1)));
        assert_eq!(function_at("=IF(A1>2,", 9), Some(("IF".to_owned(), 1)));
        assert_eq!(function_at("=(1+2", 5), None);
        assert_eq!(word_before("=SU", 3), Some((1, "SU".to_owned())));
        assert_eq!(word_before("=A1+VL", 6), Some((4, "VL".to_owned())));
        assert_eq!(word_before("=\"SU", 4), None);
    }

    #[test]
    fn a_reference_may_be_pointed_in_after_an_operator() {
        assert!(points_at("=", 1));
        assert!(points_at("=SUM(", 5));
        assert!(points_at("=A1+", 4));
        assert!(!points_at("=A1", 3));
        assert!(!points_at("hello", 5));
    }

    #[test]
    fn f4_steps_a_reference_through_its_four_forms() {
        let (a, caret) = cycle_reference("=A1+B2", 3).unwrap();
        assert_eq!(a, "=$A$1+B2");
        assert_eq!(caret, 5);
        let (b, _) = cycle_reference(&a, 5).unwrap();
        assert_eq!(b, "=A$1+B2");
        let (c, _) = cycle_reference(&b, 4).unwrap();
        assert_eq!(c, "=$A1+B2");
        let (d, _) = cycle_reference(&c, 4).unwrap();
        assert_eq!(d, "=A1+B2");
        assert_eq!(cycle_reference("=Data!A1:B2", 11).unwrap().0, "=Data!$A$1:$B$2");
    }
}
