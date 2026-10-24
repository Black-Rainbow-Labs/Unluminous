//! What a cell holds, and the rectangle of values a formula can return.

use std::sync::Arc;

/// Excel's error values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ErrorKind {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    GettingData,
    Spill,
    Calc,
    /// A service the workbook needs is switched off or not available to this copy of the program.
    Blocked,
    Connect,
    Field,
    Unknown,
    Python,
    Busy,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 16] = [
        ErrorKind::Null,
        ErrorKind::Div0,
        ErrorKind::Value,
        ErrorKind::Ref,
        ErrorKind::Name,
        ErrorKind::Num,
        ErrorKind::NA,
        ErrorKind::GettingData,
        ErrorKind::Spill,
        ErrorKind::Calc,
        ErrorKind::Blocked,
        ErrorKind::Connect,
        ErrorKind::Field,
        ErrorKind::Unknown,
        ErrorKind::Python,
        ErrorKind::Busy,
    ];

    /// How the error is written in a cell and in a file.
    pub fn text(&self) -> &'static str {
        match self {
            ErrorKind::Null => "#NULL!",
            ErrorKind::Div0 => "#DIV/0!",
            ErrorKind::Value => "#VALUE!",
            ErrorKind::Ref => "#REF!",
            ErrorKind::Name => "#NAME?",
            ErrorKind::Num => "#NUM!",
            ErrorKind::NA => "#N/A",
            ErrorKind::GettingData => "#GETTING_DATA",
            ErrorKind::Spill => "#SPILL!",
            ErrorKind::Calc => "#CALC!",
            ErrorKind::Blocked => "#BLOCKED!",
            ErrorKind::Connect => "#CONNECT!",
            ErrorKind::Field => "#FIELD!",
            ErrorKind::Unknown => "#UNKNOWN!",
            ErrorKind::Python => "#PYTHON!",
            ErrorKind::Busy => "#BUSY!",
        }
    }

    /// How a cell in a file holds the error. Excel opens a file with `#BUSY!` in a cell as it is, and has
    /// to repair one that holds any of the other newer kinds there: it writes those as `#VALUE!` beside
    /// value metadata that names the real kind, so that is what is written. `#GETTING_DATA` is a state of
    /// a calculation in progress and is not worth saving as such.
    pub fn file_text(&self) -> &'static str {
        match self {
            ErrorKind::GettingData
            | ErrorKind::Spill
            | ErrorKind::Calc
            | ErrorKind::Blocked
            | ErrorKind::Connect
            | ErrorKind::Field
            | ErrorKind::Unknown
            | ErrorKind::Python => "#VALUE!",
            other => other.text(),
        }
    }

    /// The error written as text, in any case.
    pub fn parse(text: &str) -> Option<Self> {
        let upper = text.trim().to_ascii_uppercase();
        Self::ALL.into_iter().find(|kind| kind.text() == upper)
    }

    /// What `ERROR.TYPE` answers.
    pub fn code(&self) -> f64 {
        match self {
            ErrorKind::Null => 1.0,
            ErrorKind::Div0 => 2.0,
            ErrorKind::Value => 3.0,
            ErrorKind::Ref => 4.0,
            ErrorKind::Name => 5.0,
            ErrorKind::Num => 6.0,
            ErrorKind::NA => 7.0,
            ErrorKind::GettingData => 8.0,
            ErrorKind::Spill => 9.0,
            ErrorKind::Calc => 14.0,
            ErrorKind::Connect => 10.0,
            ErrorKind::Blocked => 11.0,
            ErrorKind::Unknown => 12.0,
            ErrorKind::Field => 13.0,
            // Measured on Excel 16: a file holding #BUSY! reads as an error that ERROR.TYPE answers 0 for.
            // Excel does not accept #PYTHON! in a cell of a file, so 19 is its documented number.
            ErrorKind::Python => 19.0,
            ErrorKind::Busy => 0.0,
        }
    }

    /// Why a cell shows this error, for the line under the formula bar.
    pub fn why(&self) -> &'static str {
        match self {
            ErrorKind::Null => "Two ranges that do not cross were intersected.",
            ErrorKind::Div0 => "A number was divided by zero or by an empty cell.",
            ErrorKind::Value => "A value is the wrong type for what was asked of it.",
            ErrorKind::Ref => "A reference points at a cell that no longer exists.",
            ErrorKind::Name => "A name or a function is not one this workbook knows.",
            ErrorKind::Num => "A number is out of range for this calculation.",
            ErrorKind::NA => "A value is not available, usually a lookup that found nothing.",
            ErrorKind::GettingData => "The value is still being worked out.",
            ErrorKind::Spill => "The result needs more cells than are free beside the formula.",
            ErrorKind::Calc => "The calculation has no answer, such as an empty array.",
            ErrorKind::Blocked => {
                "The function needs a service of Excel's that this program does not have."
            }
            ErrorKind::Connect => "A connection to a data source could not be made.",
            ErrorKind::Field => "A linked data type has no field of that name.",
            ErrorKind::Unknown => "The value is of a kind this program does not know.",
            ErrorKind::Python => "A Python cell failed to run.",
            ErrorKind::Busy => "A service is still working on the value.",
        }
    }
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(self.text())
    }
}

/// One cell's value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Empty,
    Number(f64),
    Text(Arc<str>),
    Bool(bool),
    Error(ErrorKind),
}

impl Value {
    pub fn text(text: impl AsRef<str>) -> Self {
        Value::Text(Arc::from(text.as_ref()))
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Value::Empty)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The type number `TYPE` answers: 1 number, 2 text, 4 logical, 16 error. An empty cell is a number.
    pub fn type_number(&self) -> f64 {
        match self {
            Value::Empty | Value::Number(_) => 1.0,
            Value::Text(_) => 2.0,
            Value::Bool(_) => 4.0,
            Value::Error(_) => 16.0,
        }
    }

    /// The value as plain text with no number format: what `=A1&""` gives.
    pub fn to_text(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Number(n) => number_to_text(*n),
            Value::Text(t) => t.to_string(),
            Value::Bool(true) => "TRUE".to_owned(),
            Value::Bool(false) => "FALSE".to_owned(),
            Value::Error(e) => e.text().to_owned(),
        }
    }
}

/// A number as Excel turns it into text when it is joined to a string: fifteen significant digits, no
/// trailing zeros, and scientific notation once the plain form would be longer than twenty characters
/// without its sign. Measured against Excel: 1E-18 is written out and 1E-19 is not, and 1E+20 is the first
/// large one in scientific notation.
pub fn number_to_text(n: f64) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    if !n.is_finite() {
        return "#NUM!".to_owned();
    }
    let (digits, exponent) = significant_digits(n.abs(), 15);
    let sign = if n < 0.0 { "-" } else { "" };
    let fixed = if exponent >= 0 {
        let whole_len = exponent as usize + 1;
        if digits.len() <= whole_len {
            format!("{digits}{}", "0".repeat(whole_len - digits.len()))
        } else {
            format!("{}.{}", &digits[..whole_len], &digits[whole_len..])
        }
    } else {
        format!("0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    };
    if fixed.len() <= 20 {
        return format!("{sign}{fixed}");
    }
    let mantissa = match digits.len() {
        1 => digits.clone(),
        _ => format!("{}.{}", &digits[..1], &digits[1..]),
    };
    format!("{sign}{mantissa}E{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// The significant digits of a positive number rounded to `count` of them, without trailing zeros, and
/// the power of ten of the first one.
pub fn significant_digits(n: f64, count: usize) -> (String, i32) {
    let text = format!("{:.*e}", count.saturating_sub(1), n);
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    (if digits.is_empty() { "0".to_owned() } else { digits.to_owned() }, exponent)
}

/// A number written as text, kept to fifteen significant digits the way Excel keeps a number typed or
/// written in a formula: the digits after the fifteenth are dropped, not rounded. Measured: Excel reads
/// `1234567.890123456789` as 1234567.89012345.
pub fn parse_literal(text: &str) -> Option<f64> {
    let text = text.trim();
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    };
    let mut seen = 0;
    let mut started = false;
    let mut out = String::with_capacity(text.len());
    for c in mantissa.chars() {
        if c.is_ascii_digit() {
            if c != '0' {
                started = true;
            }
            if started {
                seen += 1;
            }
            out.push(if seen > 15 { '0' } else { c });
        } else {
            out.push(c);
        }
    }
    out.push_str(exponent);
    out.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// `n` rounded to `digits` significant digits, which is the precision Excel keeps.
pub fn round_significant(n: f64, digits: i32) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return n;
    }
    let magnitude = n.abs().log10().floor() as i32;
    let scale = digits - 1 - magnitude;
    if !(-300..=300).contains(&scale) {
        return n;
    }
    let text = format!("{:.*e}", (digits - 1).max(0) as usize, n);
    text.parse().unwrap_or(n)
}

/// A rectangle of values: what an array constant, a range read as values and a spilling function are.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<Value>,
}

impl Array {
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Self {
        debug_assert_eq!(rows * cols, data.len());
        Self { rows, cols, data }
    }

    pub fn filled(rows: usize, cols: usize, value: Value) -> Self {
        Self { rows, cols, data: vec![value; rows * cols] }
    }

    pub fn single(value: Value) -> Self {
        Self { rows: 1, cols: 1, data: vec![value] }
    }

    pub fn column(values: Vec<Value>) -> Self {
        Self { rows: values.len(), cols: 1, data: values }
    }

    pub fn row(values: Vec<Value>) -> Self {
        Self { rows: 1, cols: values.len(), data: values }
    }

    pub fn from_rows(rows: Vec<Vec<Value>>) -> Self {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let count = rows.len();
        let mut data = Vec::with_capacity(count * cols);
        for mut row in rows {
            row.resize(cols, Value::Error(ErrorKind::NA));
            data.extend(row);
        }
        Self { rows: count, cols, data }
    }

    pub fn get(&self, row: usize, col: usize) -> &Value {
        &self.data[row * self.cols + col]
    }

    /// The value at a place, repeating a single row or column across the other direction the way Excel
    /// stretches an array to fit, and `#N/A` past its edge.
    pub fn stretched(&self, row: usize, col: usize) -> Value {
        let r = if self.rows == 1 { 0 } else { row };
        let c = if self.cols == 1 { 0 } else { col };
        if r < self.rows && c < self.cols {
            self.get(r, c).clone()
        } else {
            Value::Error(ErrorKind::NA)
        }
    }

    pub fn rows_iter(&self) -> impl Iterator<Item = &[Value]> {
        self.data.chunks(self.cols.max(1))
    }

    pub fn transpose(&self) -> Array {
        let mut data = Vec::with_capacity(self.data.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                data.push(self.get(r, c).clone());
            }
        }
        Array { rows: self.cols, cols: self.rows, data }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_turn_into_text_the_way_excel_joins_them() {
        for (n, text) in [
            (0.0, "0"),
            (1.0, "1"),
            (-2.5, "-2.5"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.333333333333333"),
            (123456789012345.0, "123456789012345"),
            (1e15, "1000000000000000"),
            (1.5e20, "1.5E+20"),
            (0.000001, "0.000001"),
            (1e-10, "0.0000000001"),
            (2.0 / 3.0 * 1e-5, "6.66666666666667E-06"),
            (1e-18, "0.000000000000000001"),
            (1e-19, "1E-19"),
            (1e20, "1E+20"),
            (12345678901234500000.0, "12345678901234500000"),
            (-1.0 / 7.0 * 1e-3, "-0.000142857142857143"),
            (123456789012345000000.0, "1.23456789012345E+20"),
        ] {
            assert_eq!(number_to_text(n), text, "{n}");
        }
    }

    #[test]
    fn every_error_reads_back_from_its_own_text() {
        for kind in ErrorKind::ALL {
            assert_eq!(ErrorKind::parse(kind.text()), Some(kind));
        }
        assert_eq!(ErrorKind::parse("#n/a"), Some(ErrorKind::NA));
    }

    #[test]
    fn an_array_stretches_a_single_row_or_column_and_is_na_past_its_edge() {
        let row = Array::row(vec![Value::Number(1.0), Value::Number(2.0)]);
        assert_eq!(row.stretched(5, 1), Value::Number(2.0));
        assert_eq!(row.stretched(0, 2), Value::Error(ErrorKind::NA));
    }
    #[test]
    fn error_type_numbers_are_the_ones_excel_gives() {
        let codes: Vec<f64> = ErrorKind::ALL.iter().map(ErrorKind::code).collect();
        assert_eq!(
            codes,
            vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 14.0, 11.0, 10.0, 13.0, 12.0, 19.0,
                0.0
            ]
        );
    }

    #[test]
    fn only_the_errors_excel_reads_in_a_cell_are_written_as_they_are() {
        let kept: Vec<ErrorKind> =
            ErrorKind::ALL.into_iter().filter(|k| k.file_text() == k.text()).collect();
        assert_eq!(
            kept,
            vec![
                ErrorKind::Null,
                ErrorKind::Div0,
                ErrorKind::Value,
                ErrorKind::Ref,
                ErrorKind::Name,
                ErrorKind::Num,
                ErrorKind::NA,
                ErrorKind::Busy
            ]
        );
    }
}
