//! Formulas as an OpenDocument file writes them (`of:=SUM([.A1:.B2];5)`) and as Excel writes them
//! (`SUM(A1:B2,5)`), translated in both directions as text.
//!
//! The OpenFormula syntax differs in five ways that matter here: a reference sits in square brackets with
//! the sheet and the cell joined by a dot (`[$Sheet2.A1:.B2]`), arguments are separated by semicolons,
//! an inline array has semicolons between columns and a bar between rows, `!` is the intersection
//! operator and `~` the union, and a name that belongs to one sheet is written with two dollar signs.
//! Functions Excel added later carry the prefix `COM.MICROSOFT.`, which is how Excel itself writes them.

use crate::value::ErrorKind;

/// The prefixes a function name carries in an OpenDocument file for the functions its own standard
/// does not define.
const PREFIXES: [&str; 4] =
    ["COM.MICROSOFT.", "ORG.OPENOFFICE.", "ORG.LIBREOFFICE.", "COM.SUN.STAR.SHEET."];

/// Functions Excel defines differently from OpenDocument, which Excel therefore writes with the
/// `COM.MICROSOFT.` prefix even though they are old.
const EXCEL_MEANING: [&str; 4] = ["FLOOR", "CEILING", "WEEKNUM", "ERROR.TYPE"];

/// Translate the body of an `of:` formula (after `of:=`) into Excel's syntax.
pub fn to_a1(body: &str) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut braces = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => i = copy_string(&chars, i, &mut out),
            '[' => {
                let end = bracket_end(&chars, i);
                out.push_str(&reference_to_a1(&chars[i + 1..end.min(chars.len())]));
                i = end + 1;
            }
            '$' if chars.get(i + 1) == Some(&'$') => i += 2,
            ';' => {
                out.push(',');
                i += 1;
            }
            '|' if braces > 0 => {
                out.push(';');
                i += 1;
            }
            '{' => {
                braces += 1;
                out.push(c);
                i += 1;
            }
            '}' => {
                braces = braces.saturating_sub(1);
                out.push(c);
                i += 1;
            }
            '!' => {
                out.push(' ');
                i += 1;
            }
            '~' => {
                out.push(',');
                i += 1;
            }
            '#' => i = copy_error(&chars, i, &mut out),
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || "_.".contains(chars[i])) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if chars.get(i) == Some(&'(') {
                    out.push_str(&function_from_of(&word));
                } else {
                    out.push_str(&word);
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The function name Excel writes for an OpenDocument one: the prefix is dropped when the rest is a
/// function this version knows, and kept when it is not so the formula stays as the file wrote it.
fn function_from_of(word: &str) -> String {
    let upper = word.to_ascii_uppercase();
    // Excel writes every function newer than its first versions with this prefix, so the name behind it
    // is Excel's own whether or not this version has the function.
    if let Some(rest) = upper.strip_prefix("COM.MICROSOFT.") {
        return rest.to_owned();
    }
    for prefix in PREFIXES {
        if let Some(rest) = upper.strip_prefix(prefix) {
            if crate::functions::find(rest).is_some() {
                return rest.to_owned();
            }
            return upper;
        }
    }
    upper
}

/// Copy a string literal, doubled quotes included, and answer the index after it.
fn copy_string(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push('"');
    let mut i = start + 1;
    while i < chars.len() {
        out.push(chars[i]);
        if chars[i] == '"' {
            if chars.get(i + 1) == Some(&'"') {
                out.push('"');
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}

/// Copy an error literal such as `#DIV/0!`, and answer the index after it.
fn copy_error(chars: &[char], start: usize, out: &mut String) -> usize {
    let rest: String = chars[start..].iter().collect();
    for kind in ErrorKind::ALL {
        if rest.to_ascii_uppercase().starts_with(kind.text()) {
            out.push_str(kind.text());
            return start + kind.text().chars().count();
        }
    }
    out.push('#');
    start + 1
}

/// The index of the `]` that closes the bracket at `start`, skipping quoted sheet names.
fn bracket_end(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    let mut quoted = false;
    while i < chars.len() {
        match chars[i] {
            '\'' => quoted = !quoted,
            ']' if !quoted => return i,
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

/// One address inside a reference: the sheet it names, if any, and its cell text.
struct Address {
    sheet: Option<String>,
    external: bool,
    cell: String,
}

/// Read a sheet name at `i`: quoted with doubled quotes, or bare, after an optional `$`.
fn read_sheet(chars: &[char], i: &mut usize) -> String {
    if chars.get(*i) == Some(&'$') {
        *i += 1;
    }
    let mut name = String::new();
    if chars.get(*i) == Some(&'\'') {
        *i += 1;
        while *i < chars.len() {
            if chars[*i] == '\'' {
                if chars.get(*i + 1) == Some(&'\'') {
                    name.push('\'');
                    *i += 2;
                    continue;
                }
                *i += 1;
                break;
            }
            name.push(chars[*i]);
            *i += 1;
        }
    } else {
        while *i < chars.len() && !".:".contains(chars[*i]) {
            name.push(chars[*i]);
            *i += 1;
        }
    }
    name
}

/// Read one address: `.A1`, `Sheet1.A1`, `$'My Sheet'.$A$1` or `'file:///a.xlsx'#$Sheet1.A1`.
fn read_address(chars: &[char], i: &mut usize) -> Address {
    let mut sheet = None;
    let mut external = false;
    if chars.get(*i) != Some(&'.') {
        let name = read_sheet(chars, i);
        if chars.get(*i) == Some(&'#') {
            *i += 1;
            external = true;
            sheet = Some(read_sheet(chars, i));
        } else {
            sheet = Some(name);
        }
    }
    if chars.get(*i) == Some(&'.') {
        *i += 1;
    }
    let mut cell = String::new();
    while *i < chars.len() && chars[*i] != ':' {
        cell.push(chars[*i]);
        *i += 1;
    }
    Address { sheet, external, cell }
}

/// The text of a reference inside brackets, in Excel's syntax.
fn reference_to_a1(inner: &[char]) -> String {
    let mut i = 0;
    let first = read_address(inner, &mut i);
    let second = (inner.get(i) == Some(&':')).then(|| {
        i += 1;
        read_address(inner, &mut i)
    });
    if first.cell.starts_with('#') {
        return first.cell;
    }
    let mut cells = first.cell.clone();
    if let Some(second) = &second {
        let across_sheets = second.sheet.as_ref().is_some_and(|s| Some(s) != first.sheet.as_ref());
        if !across_sheets || second.cell != first.cell {
            cells = format!("{}:{}", first.cell, second.cell);
        }
    }
    let Some(sheet) = &first.sheet else { return cells };
    if first.external {
        return format!("[1]{}!{cells}", crate::formula::print::sheet_name(sheet));
    }
    let mut prefix = crate::formula::print::sheet_name(sheet);
    if let Some(last) = second.as_ref().and_then(|s| s.sheet.as_ref()).filter(|s| *s != sheet) {
        let joined = format!("{sheet}:{last}");
        prefix = if prefix == *sheet && crate::formula::print::sheet_name(last) == *last {
            joined
        } else {
            format!("'{}'", joined.replace('\'', "''"))
        };
    }
    format!("{prefix}!{cells}")
}

// ---------------------------------------------------------------------------------------------------
// Excel to OpenDocument.

/// Translate Excel's formula text (as written to a file, with `_xlfn.` prefixes) into the body of an `of:`
/// formula. `local_names` are the names that belong to the formula's own sheet.
pub fn from_a1(body: &str, local_names: &[String]) -> String {
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut braces = 0usize;
    let mut after_reference = false;
    while i < chars.len() {
        let c = chars[i];
        let was_reference = std::mem::take(&mut after_reference);
        match c {
            '"' => i = copy_string(&chars, i, &mut out),
            '\'' => {
                let (text, next) = quoted_reference(&chars, i);
                out.push_str(&text);
                after_reference = true;
                i = next;
            }
            '#' => i = error_to_of(&chars, i, &mut out),
            '[' => i = external_to_of(&chars, i, &mut out),
            ',' => {
                out.push(';');
                i += 1;
            }
            ';' if braces > 0 => {
                out.push('|');
                i += 1;
            }
            '{' => {
                braces += 1;
                out.push(c);
                i += 1;
            }
            '}' => {
                braces = braces.saturating_sub(1);
                out.push(c);
                i += 1;
            }
            ' ' => {
                let next_starts_operand =
                    chars.get(i + 1).is_some_and(|n| n.is_alphanumeric() || "_$'(".contains(*n));
                out.push(if was_reference && next_starts_operand { '!' } else { ' ' });
                after_reference = was_reference && !next_starts_operand;
                i += 1;
            }
            ')' => {
                out.push(c);
                after_reference = true;
                i += 1;
            }
            c if c.is_alphanumeric() || "_$.".contains(c) => {
                let (text, next, is_reference) = word_to_of(&chars, i, local_names);
                out.push_str(&text);
                after_reference = is_reference;
                i = next;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Translate an error literal: `#REF!` becomes a broken reference and `#N/A` a call.
fn error_to_of(chars: &[char], start: usize, out: &mut String) -> usize {
    let rest: String = chars[start..].iter().collect();
    for kind in ErrorKind::ALL {
        if rest.starts_with(kind.text()) {
            match kind {
                ErrorKind::Ref => out.push_str("[.#REF!]"),
                ErrorKind::NA => out.push_str("NA()"),
                other => out.push_str(other.text()),
            }
            return start + kind.text().chars().count();
        }
    }
    out.push('#');
    start + 1
}

/// A reference to another workbook, `[1]Sheet1!$A$1`, as a reference to a file by the number Excel gave it.
fn external_to_of(chars: &[char], start: usize, out: &mut String) -> usize {
    let end = chars[start..].iter().position(|c| *c == ']').map_or(chars.len(), |p| start + p);
    let number: String = chars[(start + 1).min(end)..end].iter().collect();
    let (sheet, after) = read_word(chars, end + 1);
    if chars.get(after) != Some(&'!') {
        out.push_str("[.#REF!]");
        return (end + 1).min(chars.len()).max(start + 1);
    }
    let (cells, next) = read_cells(chars, after + 1);
    let (first, last) = cells.split_once(':').map_or((cells.as_str(), None), |(a, b)| (a, Some(b)));
    let last = last.map(|l| format!(":.{l}")).unwrap_or_default();
    out.push_str(&format!(
        "['file:///external{number}.xlsx'#${}.{first}{last}]",
        of_sheet_name(&sheet)
    ));
    next
}

/// A reference that starts with a quoted sheet name: `'My Sheet'!A1:B2`.
fn quoted_reference(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start + 1;
    let mut name = String::new();
    while i < chars.len() {
        if chars[i] == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                name.push('\'');
                i += 2;
                continue;
            }
            i += 1;
            break;
        }
        name.push(chars[i]);
        i += 1;
    }
    if chars.get(i) != Some(&'!') {
        return (format!("'{}'", name.replace('\'', "''")), i);
    }
    i += 1;
    let (cells, next) = read_cells(chars, i);
    (of_reference(Some(&name), &cells), next)
}

/// The cell text after a sheet name: `A1`, `A1:B2`, `A:A` or `1:1`.
fn read_cells(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    while i < chars.len() && (chars[i].is_alphanumeric() || "$:".contains(chars[i])) {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

/// An `of:` reference for a sheet and cells.
fn of_reference(sheet: Option<&str>, cells: &str) -> String {
    let (first, last) = match cells.split_once(':') {
        Some((a, b)) => (a, Some(b)),
        None => (cells, None),
    };
    let sheet_text = sheet.map(of_sheet_name).unwrap_or_default();
    let mut out = format!("[{sheet_text}.{first}");
    if let Some(last) = last {
        out.push_str(&format!(":.{last}"));
    }
    out.push(']');
    out
}

/// A sheet name as an `of:` reference writes it: quoted unless it is plain.
fn of_sheet_name(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// Whether text is a cell, such as `$B$4`.
fn is_cell(text: &str) -> bool {
    crate::coord::CellRef::parse(text).is_some()
}

/// Whether text is a column, such as `$C`.
fn is_column(text: &str) -> bool {
    crate::coord::column_number(text.trim_start_matches('$')).is_some()
}

/// Whether text is a row, such as `$12`.
fn is_row(text: &str) -> bool {
    let digits = text.trim_start_matches('$');
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Read a run of word characters at `start`.
fn read_word(chars: &[char], start: usize) -> (String, usize) {
    let mut i = start;
    while i < chars.len() && (chars[i].is_alphanumeric() || "_$.".contains(chars[i])) {
        i += 1;
    }
    (chars[start..i].iter().collect(), i)
}

/// Translate the word at `start`: a function, a sheet qualified reference, a cell or range, a number
/// or a name. Answers the text, the index after it and whether it ended a reference.
fn word_to_of(chars: &[char], start: usize, local_names: &[String]) -> (String, usize, bool) {
    let (word, i) = read_word(chars, start);
    match chars.get(i) {
        Some('(') => (function_to_of(&word), i, false),
        Some('!') => {
            let (cells, next) = read_cells(chars, i + 1);
            if cells.is_empty() {
                return (word, i, false);
            }
            if is_cell(&cells) || cells.contains(':') || is_column(&cells) || is_row(&cells) {
                return (of_reference(Some(&word), &cells), next, true);
            }
            (format!("$${cells}"), next, false)
        }
        Some(':') if three_dimensional(chars, i) => {
            let (last, after) = read_word(chars, i + 1);
            let (cells, next) = read_cells(chars, after + 1);
            let (from, to) = cells.split_once(':').unwrap_or((&cells, &cells));
            let (first, second) = (of_sheet_name(&word), of_sheet_name(&last));
            (format!("[{first}.{from}:{second}.{to}]"), next, true)
        }
        _ => reference_or_word(chars, &word, i, local_names),
    }
}

/// Whether `Sheet1:Sheet3!A1` starts at the colon at `colon`.
fn three_dimensional(chars: &[char], colon: usize) -> bool {
    let (_, after) = read_word(chars, colon + 1);
    chars.get(after) == Some(&'!') && after > colon + 1
}

/// A word with no sheet in front of it: a cell or range, a number or a name.
fn reference_or_word(
    chars: &[char],
    word: &str,
    next: usize,
    local_names: &[String],
) -> (String, usize, bool) {
    let upper_start = word.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.');
    if chars.get(next) == Some(&':') {
        let (second, after) = read_word(chars, next + 1);
        let pair = (is_cell(word) && is_cell(&second))
            || (is_column(word) && is_column(&second) && !is_cell(word))
            || (is_row(word) && is_row(&second));
        if pair {
            return (of_reference(None, &format!("{word}:{second}")), after, true);
        }
    }
    if is_cell(word) {
        return (of_reference(None, word), next, true);
    }
    if upper_start {
        return number_word(chars, word, next);
    }
    let name = local_names.iter().any(|n| n.eq_ignore_ascii_case(word));
    (if name { format!("$${word}") } else { word.to_owned() }, next, false)
}

/// A number, with the sign and digits of an exponent that the word ended before.
fn number_word(chars: &[char], word: &str, next: usize) -> (String, usize, bool) {
    let exponent =
        word.ends_with(['E', 'e']) && word[..word.len() - 1].bytes().any(|b| b.is_ascii_digit());
    if exponent && matches!(chars.get(next), Some('+') | Some('-')) {
        let mut text = word.to_owned();
        let mut i = next;
        text.push(chars[i]);
        i += 1;
        while i < chars.len() && chars[i].is_ascii_digit() {
            text.push(chars[i]);
            i += 1;
        }
        return (text, i, false);
    }
    (word.to_owned(), next, false)
}

/// The function name an `of:` formula uses for one Excel writes.
fn function_to_of(word: &str) -> String {
    let upper = word.to_ascii_uppercase();
    let bare = upper
        .strip_prefix("_XLFN._XLWS.")
        .or_else(|| upper.strip_prefix("_XLFN."))
        .or_else(|| word.strip_prefix("_xlfn._xlws."))
        .or_else(|| word.strip_prefix("_xlfn."));
    match bare {
        Some(rest) => format!("COM.MICROSOFT.{}", rest.to_ascii_uppercase()),
        None if EXCEL_MEANING.contains(&upper.as_str()) => format!("COM.MICROSOFT.{upper}"),
        None => upper,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_translate_with_their_sheets_and_absolute_marks() {
        for (of, a1) in [
            ("SUM([.A1:.B2])", "SUM(A1:B2)"),
            ("[$Sheet2.A1]*2", "Sheet2!A1*2"),
            ("SUM([Sheet1.$N$2:.$N$5])", "SUM(Sheet1!$N$2:$N$5)"),
            ("[$'My Sheet'.$A$1:.$B$2]", "'My Sheet'!$A$1:$B$2"),
            ("SUM([Sheet1.A1:Sheet3.A1])", "SUM(Sheet1:Sheet3!A1)"),
            ("[.A:.A]+[.1:.1]", "A:A+1:1"),
            ("[.#REF!]", "#REF!"),
            ("[.A1:.A2]![.C1:.C2]", "A1:A2 C1:C2"),
        ] {
            assert_eq!(to_a1(of), a1, "{of}");
        }
    }

    #[test]
    fn separators_arrays_names_and_function_prefixes_translate() {
        assert_eq!(to_a1("IF([.A1]>1;\"a;b\";\"c\")"), "IF(A1>1,\"a;b\",\"c\")");
        assert_eq!(to_a1("SUM({1;2|3;4})"), "SUM({1,2;3,4})");
        assert_eq!(to_a1("$$LocalName*2+Global1"), "LocalName*2+Global1");
        assert_eq!(to_a1("COM.MICROSOFT.SEQUENCE(3;2)"), "SEQUENCE(3,2)");
        assert_eq!(to_a1("COM.MICROSOFT.FLOOR([.H1];1)"), "FLOOR(H1,1)");
        // A function this version does not have keeps the name the file gave it.
        assert_eq!(to_a1("ORG.OPENOFFICE.NOTHING(1)"), "ORG.OPENOFFICE.NOTHING(1)");
        assert_eq!(to_a1("\"a\"+1"), "\"a\"+1");
        assert_eq!(to_a1("1/0"), "1/0");
    }

    #[test]
    fn excel_formulas_translate_to_the_open_formula_syntax() {
        let none: Vec<String> = Vec::new();
        for (a1, of) in [
            ("SUM(A1:B2)", "SUM([.A1:.B2])"),
            ("Sheet2!A1*2", "[Sheet2.A1]*2"),
            ("SUM(Sheet1!$N$2:$N$5)", "SUM([Sheet1.$N$2:.$N$5])"),
            ("'My Sheet'!$A$1:$B$2", "['My Sheet'.$A$1:.$B$2]"),
            ("SUM(Sheet1:Sheet3!A1)", "SUM([Sheet1.A1:Sheet3.A1])"),
            ("A:A+1:1", "[.A:.A]+[.1:.1]"),
            ("IF(A1>1,\"a,b\",\"c\")", "IF([.A1]>1;\"a,b\";\"c\")"),
            ("SUM({1,2;3,4})", "SUM({1;2|3;4})"),
            ("_xlfn.SEQUENCE(3,2)", "COM.MICROSOFT.SEQUENCE(3;2)"),
            ("_xlfn._xlws.SORT(A1:A9)", "COM.MICROSOFT.SORT([.A1:.A9])"),
            ("FLOOR(A1,1)", "COM.MICROSOFT.FLOOR([.A1];1)"),
            ("A1:A2 C1:C2", "[.A1:.A2]![.C1:.C2]"),
            ("1.5E+3+#REF!", "1.5E+3+[.#REF!]"),
            ("#N/A", "NA()"),
            ("TODAY()+(1-5)*4", "TODAY()+(1-5)*4"),
        ] {
            assert_eq!(from_a1(a1, &none), of, "{a1}");
        }
        assert_eq!(from_a1("LocalName*2", &["LocalName".to_owned()]), "$$LocalName*2");
        assert_eq!(from_a1("Sheet1!LocalName", &none), "$$LocalName");
    }
}
