//! Remove Duplicates and Text to Columns.

use crate::edit::Guard;
use crate::coord::{CellRef, Range};
use crate::edit::{clip_to_used, Session};
use crate::model::*;
use crate::styles::StyleChange;
use crate::value::Value;

use super::displayed;
use super::sorting::HeaderMode;

impl Session {
    /// Remove the rows of a range whose values in some columns repeat an earlier row. `columns` are
    /// offsets within the range. Answers how many were removed.
    pub fn remove_duplicates(
        &mut self,
        sheet: usize,
        range: Range,
        columns: &[u32],
        header: bool,
    ) -> Result<usize, String> {
        let mode = if header { HeaderMode::Yes } else { HeaderMode::No };
        self.remove_duplicates_with(sheet, range, columns, mode)
    }

    /// Remove Duplicates with the header left to Excel's guess when asked.
    pub fn remove_duplicates_with(
        &mut self,
        sheet: usize,
        range: Range,
        columns: &[u32],
        header: HeaderMode,
    ) -> Result<usize, String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        self.refuse_uneven_merges(sheet, range)?;
        let range = clip_to_used(&self.book.sheets[sheet], range);
        let header = match header {
            HeaderMode::Yes => true,
            HeaderMode::No => false,
            HeaderMode::Guess => self.guess_header(sheet, range, false),
        };
        let first = range.first.row + u32::from(header);
        self.remember_whole("Remove duplicates");
        let keep_rows = self.rows_without_repeats(sheet, range, columns, first);
        let removed = (range.last.row + 1 - first) as usize - keep_rows.len();
        self.move_kept_rows(sheet, range, first, &keep_rows);
        self.engine.rebuild(&self.book);
        self.recalculate_all();
        Ok(removed)
    }

    /// Merged cells in the range must all be one size, as for a sort.
    fn refuse_uneven_merges(&self, sheet: usize, range: Range) -> Result<(), String> {
        let merges: Vec<Range> = self.book.sheets[sheet]
            .merges
            .iter()
            .copied()
            .filter(|m| m.intersects(&range))
            .collect();
        let same =
            merges.iter().all(|m| m.rows() == merges[0].rows() && m.cols() == merges[0].cols());
        // Cells that are not merged count as a size of their own.
        if merges.is_empty() || (same && merges.iter().map(|m| m.len()).sum::<u64>() == range.len())
        {
            Ok(())
        } else {
            Err(super::sorting::MERGE_MESSAGE.to_owned())
        }
    }

    /// The rows from `first` whose values in the chosen columns have not appeared in an earlier row. Two
    /// cells match when their values and the text they show are the same, ignoring case.
    fn rows_without_repeats(
        &self,
        sheet: usize,
        range: Range,
        columns: &[u32],
        first: u32,
    ) -> Vec<u32> {
        let s = &self.book.sheets[sheet];
        let mut seen: Vec<Vec<(Value, String)>> = Vec::new();
        let mut keep_rows = Vec::new();
        for r in first..=range.last.row {
            let key: Vec<(Value, String)> = columns
                .iter()
                .map(|c| {
                    let at = CellRef::new(r, range.first.col + c);
                    let shown =
                        s.cell(at).map(|cell| displayed(&self.book, cell)).unwrap_or_default();
                    (s.value_at(at), fold_case(&shown))
                })
                .collect();
            let repeats = seen
                .iter()
                .any(|k| k.iter().zip(&key).all(|(a, b)| same_value(&a.0, &b.0) && a.1 == b.1));
            if !repeats {
                seen.push(key);
                keep_rows.push(r);
            }
        }
        keep_rows
    }

    /// Move the kept rows up over the removed ones, with their notes and links, and empty the rows left over.
    fn move_kept_rows(&mut self, sheet: usize, range: Range, first: u32, keep_rows: &[u32]) {
        let s = &self.book.sheets[sheet];
        let mut kept: Vec<((u32, u32), Cell)> = Vec::new();
        let mut notes: Vec<((u32, u32), Note)> = Vec::new();
        let mut links: Vec<(usize, Range)> = Vec::new();
        for (i, r) in keep_rows.iter().enumerate() {
            let to_row = first + i as u32;
            let delta = to_row as i64 - *r as i64;
            for c in range.first.col..=range.last.col {
                if let Some(cell) = s.cells.get(&(*r, c)) {
                    let mut cell = cell.clone();
                    cell.formula =
                        cell.formula.map(|f| super::sorting::shifted_formula(&f, delta, false));
                    kept.push(((to_row, c), cell));
                }
                if let Some(note) = s.notes.get(&(*r, c)) {
                    notes.push(((to_row, c), note.clone()));
                }
                for (n, link) in s.links.iter().enumerate() {
                    if link.range == Range::cell(CellRef::new(*r, c)) {
                        links.push((n, Range::cell(CellRef::new(to_row, c))));
                    }
                }
            }
        }
        let s = &mut self.book.sheets[sheet];
        for r in first..=range.last.row {
            for c in range.first.col..=range.last.col {
                s.cells.remove(&(r, c));
                s.notes.remove(&(r, c));
            }
        }
        for (k, cell) in kept {
            s.cells.insert(k, cell);
        }
        for (k, note) in notes {
            s.notes.insert(k, note);
        }
        for (n, r) in links {
            s.links[n].range = r;
        }
        s.notes_changed = true;
    }
}

/// Text with case ignored the way Excel ignores it: `ß` counts as `ss`.
fn fold_case(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// Whether two values count as the same for Remove Duplicates.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Error(x), Value::Error(y)) => x == y,
        (Value::Text(x), Value::Text(y)) => fold_case(x) == fold_case(y),
        _ => std::mem::discriminant(a) == std::mem::discriminant(b) && crate::eval::equal(a, b),
    }
}

/// How Text to Columns cuts the text.
#[derive(Debug, Clone, PartialEq)]
pub enum SplitMode {
    Delimited(Delimiters),
    /// The positions (counted from 0, in characters) where a new column starts.
    FixedWidth(Vec<usize>),
}

/// The delimiters of a delimited split.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Delimiters {
    pub tab: bool,
    pub semicolon: bool,
    pub comma: bool,
    pub space: bool,
    /// Any other characters, each of which delimits.
    pub other: String,
    /// Treat consecutive delimiters as one.
    pub consecutive_as_one: bool,
    /// The character that wraps a field holding a delimiter: `"`, `'` or none.
    pub qualifier: Option<char>,
}

impl Delimiters {
    fn is_delimiter(&self, c: char) -> bool {
        (self.tab && c == '\t')
            || (self.semicolon && c == ';')
            || (self.comma && c == ',')
            || (self.space && c == ' ')
            || self.other.contains(c)
    }

    fn any(&self) -> bool {
        self.tab || self.semicolon || self.comma || self.space || !self.other.is_empty()
    }
}

/// What a column of the result is read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnFormat {
    #[default]
    General,
    Text,
    Date(DateOrder),
    /// The part of the text in this column is not put anywhere.
    Skip,
}

/// The order of the day, month and year in a text read as a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    Mdy,
    Dmy,
    Ymd,
    Myd,
    Dym,
    Ydm,
}

/// The whole of a Text to Columns request.
#[derive(Debug, Clone, PartialEq)]
pub struct TextToColumns {
    pub mode: SplitMode,
    /// What each result column is read as; a column past the end of the list is General.
    pub formats: Vec<ColumnFormat>,
    /// The top left cell of the result; the first cell of the range when `None`.
    pub destination: Option<CellRef>,
}

impl TextToColumns {
    /// A split at one delimiter, every column read as General.
    pub fn at(delimiter: &str) -> Self {
        let d = Delimiters {
            tab: delimiter == "\t",
            semicolon: delimiter == ";",
            comma: delimiter == ",",
            space: delimiter == " ",
            other: if matches!(delimiter, "\t" | ";" | "," | " ") {
                String::new()
            } else {
                delimiter.to_owned()
            },
            ..Delimiters::default()
        };
        Self { mode: SplitMode::Delimited(d), formats: Vec::new(), destination: None }
    }
}

/// Cut a text into fields at the delimiters, honouring the qualifier.
fn split_delimited(text: &str, d: &Delimiters) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut fields: Vec<String> = Vec::new();
    let mut at = 0;
    loop {
        let mut field = String::new();
        if d.qualifier.is_some_and(|q| chars.get(at) == Some(&q)) {
            let q = d.qualifier.unwrap_or('"');
            at += 1;
            while at < chars.len() {
                if chars[at] == q {
                    if chars.get(at + 1) == Some(&q) {
                        field.push(q);
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                field.push(chars[at]);
                at += 1;
            }
            // Anything between the closing qualifier and the next delimiter belongs to the field.
            while at < chars.len() && !d.is_delimiter(chars[at]) {
                field.push(chars[at]);
                at += 1;
            }
        } else {
            while at < chars.len() && !d.is_delimiter(chars[at]) {
                field.push(chars[at]);
                at += 1;
            }
        }
        fields.push(field);
        if at >= chars.len() {
            break;
        }
        // Step over the delimiter, and the ones after it when they count as one.
        at += 1;
        if d.consecutive_as_one {
            while at < chars.len() && d.is_delimiter(chars[at]) {
                at += 1;
            }
        }
        if at >= chars.len() {
            fields.push(String::new());
            break;
        }
    }
    fields
}

/// Cut a text at fixed positions.
fn split_fixed(text: &str, breaks: &[usize]) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut starts: Vec<usize> = breaks.to_vec();
    starts.sort_unstable();
    starts.dedup();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, start)| {
            let end = starts.get(i + 1).copied().unwrap_or(chars.len()).min(chars.len());
            let start = (*start).min(chars.len());
            chars[start..end.max(start)].iter().collect::<String>().trim().to_owned()
        })
        .collect()
}

impl Session {
    /// Split the text of a column into the columns to its right at a delimiter.
    pub fn text_to_columns(
        &mut self,
        sheet: usize,
        range: Range,
        delimiter: &str,
    ) -> Result<(), String> {
        if delimiter.is_empty() {
            return Err("text to columns needs a delimiter".to_owned());
        }
        self.text_to_columns_with(sheet, range, &TextToColumns::at(delimiter))
    }

    /// Text to Columns with everything its wizard offers.
    pub fn text_to_columns_with(
        &mut self,
        sheet: usize,
        range: Range,
        options: &TextToColumns,
    ) -> Result<(), String> {
        self.check_sheet(sheet)?;
        self.allowed(sheet, Guard::Forbidden)?;
        if range.cols() > 1 {
            return Err("You can only convert one column at a time.".to_owned());
        }
        if let SplitMode::Delimited(d) = &options.mode {
            // With no delimiter chosen Excel splits nothing.
            if !d.any() {
                return Ok(());
            }
        }
        let range = clip_to_used(&self.book.sheets[sheet], range);
        let origin = options.destination.unwrap_or(range.first);
        let mut writes: Vec<(CellRef, String, ColumnFormat)> = Vec::new();
        let mut widest = 1u32;
        for (i, r) in (range.first.row..=range.last.row).enumerate() {
            let cell = self.book.sheets[sheet].cell(CellRef::new(r, range.first.col)).cloned();
            let Some(cell) = cell.filter(|c| !c.value.is_empty() && c.formula.is_none()) else {
                continue;
            };
            let text = displayed(&self.book, &cell);
            let fields = match &options.mode {
                SplitMode::Delimited(d) => split_delimited(&text, d),
                SplitMode::FixedWidth(breaks) => split_fixed(&text, breaks),
            };
            let mut column = 0u32;
            for (j, field) in fields.iter().enumerate() {
                let format = options.formats.get(j).copied().unwrap_or_default();
                if format == ColumnFormat::Skip {
                    continue;
                }
                let at = CellRef::new(origin.row + i as u32, origin.col + column);
                column += 1;
                widest = widest.max(column);
                writes.push((at, field.clone(), format));
            }
        }
        let block = Range::new(
            origin,
            CellRef::new(origin.row + range.rows() - 1, origin.col + widest - 1),
        );
        self.refuse_array_member(sheet, block)?;
        self.refuse_locked(sheet, block)?;
        self.remember_cells("Text to columns", sheet, &[block, range], true);
        // The whole block is replaced, so a row with fewer fields loses what was beyond them too.
        for row in block.first.row..=block.last.row {
            for col in block.first.col..=block.last.col {
                self.clear_values_at(sheet, CellRef::new(row, col));
            }
        }
        for (at, field, format) in writes {
            self.write_field(sheet, at, &field, format)?;
        }
        self.recalculate();
        Ok(())
    }

    /// Empty a cell of what it holds, keeping its format.
    fn clear_values_at(&mut self, sheet: usize, at: CellRef) {
        let Some(old) = self.book.sheets[sheet].cell(at).cloned() else { return };
        if old.value.is_empty() && old.formula.is_none() {
            return;
        }
        self.book.sheets[sheet]
            .set(at, Cell { style: old.style, extra: old.extra, ..Default::default() });
        self.engine.changed(&self.book, sheet, at);
    }

    /// Put one field of a split into a cell, as its column format reads it.
    fn write_field(
        &mut self,
        sheet: usize,
        at: CellRef,
        field: &str,
        format: ColumnFormat,
    ) -> Result<(), String> {
        let old = self.book.sheets[sheet].cells.get(&(at.row, at.col)).cloned().unwrap_or_default();
        let mut style = old.style;
        let text = self.read_field(field, format);
        let (value, number_format) = if format == ColumnFormat::Text && !field.is_empty() {
            style = self.book.styles.derive(style, &StyleChange::NumberFormat("@".to_owned()));
            (Value::text(field), None)
        } else {
            let (value, _, number_format) = self.prepare(sheet, at, &text)?;
            (value, number_format)
        };
        // A date written with a month name shows as `d-mmm-yy`, as it does when typed.
        let by_name = matches!(format, ColumnFormat::Date(_))
            && !matches!(value, Value::Text(_))
            && date_with_month_name(field).is_some();
        let number_format = if by_name { Some("d-mmm-yy") } else { number_format };
        if let Some(code) = number_format {
            if self.book.styles.format_code(style) == "General" {
                style = self.book.styles.derive(style, &StyleChange::NumberFormat(code.to_owned()));
            }
        }
        self.book.sheets[sheet]
            .set(at, Cell { value, formula: None, style, rich: None, extra: old.extra.clone() });
        self.engine.changed(&self.book, sheet, at);
        Ok(())
    }

    /// What typing a field of a split into a cell of its column format means.
    fn read_field(&self, field: &str, format: ColumnFormat) -> String {
        match format {
            ColumnFormat::Text if field.is_empty() => String::new(),
            ColumnFormat::Text => format!("'{field}"),
            ColumnFormat::General | ColumnFormat::Skip => field.to_owned(),
            ColumnFormat::Date(order) => match date_in_order(field, order)
                .or_else(|| date_with_month_name(field))
            {
                Some(typed) => typed,
                // Text that is not a date in that order is read as it would be typed, and stays text when it
                // is not a date that way either.
                None => match crate::input::read(field, self.book.date1904, self.this_year).value {
                    Value::Number(_)
                        if looks_like_a_date(field) || field.chars().any(char::is_alphabetic) =>
                    {
                        field.to_owned()
                    }
                    _ if field.is_empty() => String::new(),
                    _ => format!("'{field}"),
                },
            },
        }
    }
}

/// Whether a text is written as numbers joined by `/`, `-` or `.`.
fn looks_like_a_date(text: &str) -> bool {
    date_parts(text).is_some()
}

/// The three numbers of a date written with `/`, `-` or `.` between them.
fn date_parts(text: &str) -> Option<[i64; 3]> {
    let parts: Vec<&str> = text.trim().split(['/', '-', '.']).collect();
    if parts.len() != 3
        || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let n = |i: usize| parts[i].parse::<i64>().ok();
    Some([n(0)?, n(1)?, n(2)?])
}

/// A date written with the name of the month (`Jan 2 2024`, `2 January 2024`) as `m/d/yyyy`.
fn date_with_month_name(text: &str) -> Option<String> {
    const MONTHS: [&str; 12] =
        ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let parts: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '-')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let month = parts.iter().position(|p| p.len() >= 3 && p.chars().all(char::is_alphabetic))?;
    let name = parts[month].to_lowercase();
    let m = MONTHS.iter().position(|m| name.starts_with(m))? as i64 + 1;
    let numbers: Vec<i64> = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != month)
        .map(|(_, p)| p.parse::<i64>().ok())
        .collect::<Option<_>>()?;
    let (d, y) = if numbers[0] > 31 || parts[0].len() == 4 {
        (numbers[1], numbers[0])
    } else {
        (numbers[0], numbers[1])
    };
    let y = if y < 30 {
        2000 + y
    } else if y < 100 {
        1900 + y
    } else {
        y
    };
    (d >= 1 && d <= i64::from(crate::dates::days_in_month(y, m as u32)))
        .then(|| format!("{m}/{d}/{y}"))
}

/// A text read as a date whose parts come in this order, written the way a person types it in a US
/// locale (`m/d/yyyy`), or `None` when the parts do not make a date.
fn date_in_order(text: &str, order: DateOrder) -> Option<String> {
    let [a, b, c] = date_parts(text)?;
    let (y, m, d) = match order {
        DateOrder::Mdy => (c, a, b),
        DateOrder::Dmy => (c, b, a),
        DateOrder::Ymd => (a, b, c),
        DateOrder::Myd => (b, a, c),
        DateOrder::Dym => (b, c, a),
        DateOrder::Ydm => (a, c, b),
    };
    // Two digit years: 00 to 29 are 2000 to 2029 and 30 to 99 are 1930 to 1999.
    let y = if y < 30 {
        2000 + y
    } else if y < 100 {
        1900 + y
    } else {
        y
    };
    if !(1..=12).contains(&m) || d < 1 || d > i64::from(crate::dates::days_in_month(y, m as u32)) {
        return None;
    }
    Some(format!("{m}/{d}/{y}"))
}
