//! CSV and TSV: a workbook of one sheet, read the way Excel reads one and written as the values show.

use crate::coord::CellRef;
use crate::model::{Cell, Sheet, Workbook};

/// The separator a file uses: a tab, a semicolon or a comma, by which the first line holds most of
/// outside quotes. A `.tsv` file is always tab separated.
pub fn separator(text: &str, tsv: bool) -> char {
    if tsv {
        return '\t';
    }
    let first = text.lines().next().unwrap_or("");
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for c in first.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            for (sep, n) in counts.iter_mut() {
                if c == *sep {
                    *n += 1;
                }
            }
        }
    }
    counts.iter().max_by_key(|(_, n)| *n).filter(|(_, n)| *n > 0).map_or(',', |(c, _)| *c)
}

/// Split text into rows of fields by RFC 4180: quotes around a field, doubled quotes inside one, and line
/// breaks inside quotes kept.
pub fn split(text: &str, sep: char) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == sep => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            c => field.push(c),
        }
    }
    if any && (!field.is_empty() || !row.is_empty()) {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// A workbook from CSV text, each field typed the way Excel types it.
pub fn read(text: &str, tsv: bool, sheet_name: &str, this_year: i64) -> Workbook {
    let sep = separator(text, tsv);
    let mut book = Workbook::new();
    let mut sheet = Sheet::new(sheet_name.chars().take(31).collect::<String>());
    for (r, row) in split(text, sep).into_iter().enumerate() {
        for (c, field) in row.into_iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let at = CellRef::new(r as u32, c as u32);
            if field.starts_with('=') && field.len() > 1 {
                if let Ok(f) = crate::formula::Formula::parse(&field) {
                    sheet.set(at, Cell { formula: Some(f), ..Default::default() });
                    continue;
                }
            }
            let typed = crate::input::read(&field, false, this_year);
            let mut cell = Cell::value(typed.value);
            if let Some(code) = typed.format {
                cell.style = book
                    .styles
                    .derive(0, &crate::styles::StyleChange::NumberFormat(code.to_owned()));
            }
            sheet.set(at, cell);
        }
    }
    book.sheets = vec![sheet];
    book
}

/// A sheet as CSV text: each cell as it shows, quoted when it has to be, rows ended with CRLF.
pub fn write(book: &Workbook, sheet: usize, sep: char) -> String {
    let s = &book.sheets[sheet];
    let Some(used) = s.used_range() else { return String::new() };
    let mut out = String::new();
    for r in 0..=used.last.row {
        let mut fields = Vec::new();
        for c in 0..=used.last.col {
            let at = CellRef::new(r, c);
            let cell = match s.cell(at) {
                Some(cell) => cell.clone(),
                None => Cell::value(s.value_at(at)),
            };
            let code = book.styles.format_code(cell.style);
            let text =
                crate::numfmt::Format::parse(&code).render(&cell.value, book.date1904, 255).text;
            fields.push(if text.contains([sep, '"', '\n', '\r']) {
                format!("\"{}\"", text.replace('"', "\"\""))
            } else {
                text
            });
        }
        // Trailing empty fields are written, as Excel writes them, so every row has the same width.
        out.push_str(&fields.join(&sep.to_string()));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_fields_hold_separators_quotes_and_line_breaks() {
        let rows = split("a,\"b,c\",\"say \"\"hi\"\"\"\r\n1,\"two\nlines\",3\r\n", ',');
        assert_eq!(rows, vec![vec!["a", "b,c", "say \"hi\""], vec!["1", "two\nlines", "3"]]);
        assert_eq!(separator("a;b;c\n1;2;3", false), ';');
        assert_eq!(separator("a\tb", false), '\t');
    }

    #[test]
    fn fields_are_typed_and_written_back_as_shown() {
        let book = read("Name,Amount,When\nTea,\"1,250.50\",10/23/2026\n", false, "data", 2026);
        let s = &book.sheets[0];
        assert_eq!(s.value_at(CellRef::new(1, 1)), crate::value::Value::Number(1250.5));
        let text = write(&book, 0, ',');
        assert_eq!(text, "Name,Amount,When\r\nTea,\"1,250.50\",10/23/2026\r\n");
    }
}
