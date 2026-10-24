//! CSV and TSV: a workbook of one sheet, read the way Excel reads one and written as the values show.

use crate::coord::CellRef;
use crate::model::{Cell, Sheet, Workbook};

/// How text is stored in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-8 with a byte order mark: Excel's "CSV UTF-8".
    Utf8Bom,
    /// UTF-8 with no byte order mark.
    Utf8,
    /// Windows-1252, the code page of Excel's "CSV (Comma delimited)" and "Text (Tab delimited)" on a
    /// Western Windows. A character it has no code for is written as `?`.
    Windows1252,
    /// UTF-16 little endian with a byte order mark: Excel's "Unicode Text".
    Utf16Le,
}

/// File bytes as text: a byte order mark says UTF-8 or UTF-16, and a file with none is UTF-8 when it is
/// valid UTF-8 and Windows-1252 otherwise, which is what Excel does with a text file it opens.
pub fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => String::from_utf8(bytes.to_vec())
            .unwrap_or_else(|_| bytes.iter().map(|b| crate::functions::text::windows_1252(*b)).collect()),
    }
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| unit([pair[0], pair[1]])).collect();
    String::from_utf16_lossy(&units)
}

/// Text as the bytes of a file.
pub fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8Bom => [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat(),
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Windows1252 => text.chars().map(windows_1252_byte).collect(),
        Encoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            for unit in text.encode_utf16() {
                out.extend(unit.to_le_bytes());
            }
            out
        }
    }
}

/// The Windows-1252 byte for a character, or `?` when the code page has none.
fn windows_1252_byte(c: char) -> u8 {
    if (c as u32) < 0x80 {
        return c as u8;
    }
    (0x80..=0xFFu8)
        .find(|b| crate::functions::text::windows_1252(*b) == c)
        .unwrap_or(b'?')
}

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
    from_rows(split(text, sep), sheet_name, this_year)
}

/// A workbook of one sheet from rows of fields, each field typed the way Excel types a typed entry.
pub fn from_rows(rows: Vec<Vec<String>>, sheet_name: &str, this_year: i64) -> Workbook {
    let mut book = Workbook::new();
    let mut sheet = Sheet::new(sheet_name.chars().take(31).collect::<String>());
    for (r, row) in rows.into_iter().enumerate() {
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

/// How many characters a General number may use in a text file. Excel writes the same eleven whatever the
/// width of the column: `0.333333333`, `1.23457E+20`, `123456789.1`.
pub const GENERAL_WIDTH: usize = 11;

/// The text Excel writes for a cell: what the cell shows, with a General number cut to eleven characters. A
/// text cell is written as its text, so `007` stays `007`.
pub fn cell_text(book: &Workbook, sheet: usize, at: CellRef) -> String {
    let s = &book.sheets[sheet];
    let style = s.cell(at).map_or(0, |cell| cell.style);
    let code = book.styles.format_code(style);
    crate::numfmt::Format::parse(&code).render(&s.value_at(at), book.date1904, GENERAL_WIDTH).text
}

/// A field as it goes in the file: in quotes, with its own quotes doubled, when it holds the separator, a
/// comma, a tab, a quote or a line break. Excel quotes a field with a comma or a tab in a tab separated file
/// too.
pub fn quote_field(text: &str, sep: char) -> String {
    if text.contains([sep, ',', '\t', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// A sheet as text with a separator: each cell as it shows, quoted when it has to be, rows ended with CRLF.
/// Every row has as many fields as the sheet's used range has columns, and a blank row is a row of empty
/// fields.
pub fn write(book: &Workbook, sheet: usize, sep: char) -> String {
    let Some(used) = book.sheets.get(sheet).and_then(|s| s.used_range()) else { return String::new() };
    let mut out = String::new();
    for r in 0..=used.last.row {
        let fields: Vec<String> = (0..=used.last.col)
            .map(|c| quote_field(&cell_text(book, sheet, CellRef::new(r, c)), sep))
            .collect();
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
