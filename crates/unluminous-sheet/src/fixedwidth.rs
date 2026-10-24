//! Fixed width text, the `.prn` file: columns of text lined up with spaces.
//!
//! Reading finds the columns the way Excel's import wizard first guesses them, and writing is Excel's
//! "Formatted Text (Space delimited)". This module is not called `prn` because Windows reserves that name
//! for the printer device, and a file called `prn.rs` cannot be opened there.

use crate::coord::CellRef;
use crate::model::{Sheet, Workbook};
use crate::numfmt::Format;
use crate::value::Value;

/// A workbook of one sheet from fixed width text.
pub fn read(text: &str, sheet_name: &str, this_year: i64) -> Workbook {
    crate::csv::from_rows(fixed_width_rows(text), sheet_name, this_year)
}

/// The fields of each line, cut where every line has a space between two runs of text.
pub fn fixed_width_rows(text: &str) -> Vec<Vec<String>> {
    let lines: Vec<Vec<char>> =
        text.trim_start_matches('\u{FEFF}').lines().map(|l| l.chars().collect()).collect();
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let occupied: Vec<bool> = (0..width)
        .map(|p| lines.iter().any(|l| l.get(p).is_some_and(|c| !c.is_whitespace())))
        .collect();
    let starts: Vec<usize> =
        (0..width).filter(|p| occupied[*p] && (*p == 0 || !occupied[*p - 1])).collect();
    lines
        .iter()
        .map(|line| {
            let mut fields: Vec<String> = Vec::new();
            for (k, start) in starts.iter().enumerate() {
                let end = starts.get(k + 1).copied().unwrap_or(line.len().max(*start));
                let slice: String =
                    line.get(*start..end.min(line.len())).unwrap_or(&[]).iter().collect();
                fields.push(slice.trim().to_owned());
            }
            fields
        })
        .collect()
}

/// How a cell's text sits in its column.
#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
    Centre,
}

/// One cell's text and where it sits.
struct Piece {
    text: String,
    align: Align,
}

/// A column's width in characters, and in pixels: what Excel's formatted text file gives it is the number of
/// characters of the default font that fit after the five pixels of padding.
fn column_size(book: &Workbook, sheet: &Sheet, col: u32) -> (usize, f64) {
    let per = crate::ods::pixels_per_character(book);
    let units = sheet
        .column(col)
        .and_then(|c| c.width)
        .or(sheet.default_col_width)
        .unwrap_or(crate::ods::default_column_pixels(book) / per);
    let pixels = (units * per).round();
    (((pixels - 5.0) / per).floor().max(1.0) as usize, pixels)
}

/// About how wide text is drawn, in pixels, in a font whose digits are `digit` pixels wide.
fn drawn_width(text: &str, digit: f64) -> f64 {
    text.chars()
        .map(|c| match c {
            '0'..='9' => digit,
            '.' | ',' | ':' | ';' | '\'' | ' ' | '|' | '!' | 'i' | 'l' | 'j' | 't' | 'f' | 'I' => {
                digit * 0.4
            }
            '-' | '(' | ')' | '/' | 'r' => digit * 0.5,
            _ => digit * 0.9,
        })
        .sum()
}

/// A General number cut to fit `chars` characters, or `None` when it cannot be.
fn general_in(value: f64, chars: usize, date1904: bool) -> Option<String> {
    let chars = chars.min(crate::csv::GENERAL_WIDTH);
    let text = if chars >= 5 {
        let plain = Format::parse("General").render(&Value::Number(value), date1904, chars).text;
        // A very small number written plainly would lose its digits, so it is written with an exponent.
        let lost = value != 0.0
            && value.abs() < 1e-4
            && plain.parse::<f64>().is_ok_and(|p| (p - value).abs() > value.abs() * 1e-9);
        if lost {
            exponent_form(value, chars)
        } else {
            plain
        }
    } else {
        let whole = value.abs().trunc().to_string().len() + usize::from(value < 0.0);
        let decimals = chars.saturating_sub(whole + 1);
        let fixed = format!("{value:.decimals$}");
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            fixed
        }
    };
    (text.chars().count() <= chars).then_some(text)
}

/// A number as `1.23E-05` with as many digits as fit in `chars`.
fn exponent_form(value: f64, chars: usize) -> String {
    let decimals = chars.saturating_sub(6 + usize::from(value < 0.0));
    let text = format!("{value:.decimals$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    format!("{mantissa}E{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// A cell's piece of the line, or `None` for an empty cell.
fn piece(book: &Workbook, index: usize, at: CellRef, chars: usize, pixels: f64) -> Option<Piece> {
    let sheet = &book.sheets[index];
    let value = sheet.value_at(at);
    let style = sheet.cell(at).map_or(0, |c| c.style);
    let format = Format::parse(&book.styles.format_code(style));
    match &value {
        Value::Empty => None,
        Value::Text(t) => Some(Piece { text: t.to_string(), align: Align::Left }),
        Value::Bool(_) | Value::Error(_) => Some(Piece {
            text: format.render(&value, book.date1904, chars).text,
            align: Align::Centre,
        }),
        Value::Number(n) => {
            let hashes = || Piece { text: "#".repeat(chars), align: Align::Right };
            if format.is_general() {
                return Some(
                    general_in(*n, chars, book.date1904)
                        .map_or_else(hashes, |text| Piece { text, align: Align::Right }),
                );
            }
            let shown = format.render(&value, book.date1904, chars);
            let mut text = shown.text.clone();
            if let Some((fill, at)) = shown.fill {
                let missing = chars.saturating_sub(text.chars().count());
                let at = at.min(text.len());
                text.insert_str(at, &fill.to_string().repeat(missing));
            }
            let digit = crate::ods::pixels_per_character(book);
            let fits = drawn_width(&text, digit) + 5.0 <= pixels;
            if text.chars().count() > chars && !fits {
                return Some(hashes());
            }
            Some(Piece { text, align: Align::Right })
        }
    }
}

/// The text of a piece laid into `room` characters.
fn place(piece: &Piece, room: usize) -> String {
    let text: String = piece.text.chars().take(room).collect();
    let spare = room - text.chars().count();
    match piece.align {
        Align::Left => format!("{text}{}", " ".repeat(spare)),
        Align::Right => format!("{}{text}", " ".repeat(spare)),
        Align::Centre => {
            format!("{}{text}{}", " ".repeat(spare / 2), " ".repeat(spare - spare / 2))
        }
    }
}

/// A sheet as Excel's "Formatted Text (Space delimited)": every column as wide as the column is, text on
/// the left, numbers on the right, logical values and errors in the middle, and what does not fit cut off.
/// A text cell runs on into the empty cells after it, and a line ends where its last cell does.
pub fn write(book: &Workbook, index: usize) -> String {
    let Some(sheet) = book.sheets.get(index) else { return String::new() };
    let Some(used) = sheet.used_range() else { return String::new() };
    let sizes: Vec<(usize, f64)> =
        (0..=used.last.col).map(|c| column_size(book, sheet, c)).collect();
    let mut out = String::new();
    for r in 0..=used.last.row {
        let pieces: Vec<Option<Piece>> = (0..=used.last.col)
            .map(|c| {
                piece(book, index, CellRef::new(r, c), sizes[c as usize].0, sizes[c as usize].1)
            })
            .collect();
        out.push_str(&line(&pieces, &sizes));
        out.push_str("\r\n");
    }
    out
}

/// One line from the pieces of its cells.
fn line(pieces: &[Option<Piece>], sizes: &[(usize, f64)]) -> String {
    let Some(last) = pieces.iter().rposition(|p| p.is_some()) else { return String::new() };
    let mut out = String::new();
    let mut c = 0;
    while c <= last {
        let Some(p) = &pieces[c] else {
            out.push_str(&" ".repeat(sizes[c].0));
            c += 1;
            continue;
        };
        let mut room = sizes[c].0;
        let mut next = c + 1;
        if p.align == Align::Left {
            while p.text.chars().count() > room && next < pieces.len() && pieces[next].is_none() {
                room += sizes[next].0;
                next += 1;
            }
            if next > last {
                room = room.max(p.text.chars().count());
            }
        }
        let placed = place(p, room);
        out.push_str(if c == last && p.align == Align::Left {
            placed.trim_end_matches(' ')
        } else {
            &placed
        });
        c = next;
    }
    out
}
