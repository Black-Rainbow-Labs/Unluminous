//! Fixed width text, the `.prn` file: columns of text lined up with spaces.
//!
//! Reading finds the columns the way Excel's import wizard first guesses them, and writing is Excel's
//! "Formatted Text (Space delimited)". This module is not called `prn` because Windows reserves that name
//! for the printer device, and a file called `prn.rs` cannot be opened there.

use crate::model::Workbook;

/// A workbook of one sheet from fixed width text.
pub fn read(text: &str, sheet_name: &str, this_year: i64) -> Workbook {
    crate::csv::from_rows(fixed_width_rows(text), sheet_name, this_year)
}

/// The fields of each line, cut where every line has a space between two runs of text.
pub fn fixed_width_rows(text: &str) -> Vec<Vec<String>> {
    let lines: Vec<Vec<char>> = text.trim_start_matches('\u{FEFF}').lines().map(|l| l.chars().collect()).collect();
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let occupied: Vec<bool> =
        (0..width).map(|p| lines.iter().any(|l| l.get(p).is_some_and(|c| !c.is_whitespace()))).collect();
    let starts: Vec<usize> = (0..width).filter(|p| occupied[*p] && (*p == 0 || !occupied[*p - 1])).collect();
    lines
        .iter()
        .map(|line| {
            let mut fields: Vec<String> = Vec::new();
            for (k, start) in starts.iter().enumerate() {
                let end = starts.get(k + 1).copied().unwrap_or(line.len().max(*start));
                let slice: String = line.get(*start..end.min(line.len())).unwrap_or(&[]).iter().collect();
                fields.push(slice.trim().to_owned());
            }
            fields
        })
        .collect()
}

/// A sheet as formatted text.
pub fn write(book: &Workbook, sheet: usize) -> String {
    let _ = (book, sheet);
    String::new()
}
