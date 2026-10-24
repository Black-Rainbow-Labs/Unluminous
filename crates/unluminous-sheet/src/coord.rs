//! Where a cell is: a row and a column, and a rectangle of them.
//!
//! Zero based everywhere in the model, and one based only where a reference is read or written as text,
//! which is the one place a person or a file sees it.

/// How many rows a sheet has, which is Excel's limit since 2007.
pub const MAX_ROWS: u32 = 1_048_576;
/// How many columns a sheet has: A to XFD.
pub const MAX_COLS: u32 = 16_384;

/// One cell's place on a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct CellRef {
    pub row: u32,
    pub col: u32,
}

impl CellRef {
    pub fn new(row: u32, col: u32) -> Self {
        Self { row, col }
    }

    /// Read `B4`, `$B$4` or `b4`. The dollars are ignored here; [`crate::formula`] keeps them.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let bytes = text.as_bytes();
        let mut at = 0;
        if bytes.get(at) == Some(&b'$') {
            at += 1;
        }
        let letters_start = at;
        while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
            at += 1;
        }
        let col = column_number(&text[letters_start..at])?;
        if bytes.get(at) == Some(&b'$') {
            at += 1;
        }
        let digits = &text[at..];
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 7 {
            return None;
        }
        let row: u32 = digits.parse().ok()?;
        if row == 0 || row > MAX_ROWS {
            return None;
        }
        Some(Self { row: row - 1, col })
    }

    /// `B4`.
    pub fn a1(&self) -> String {
        format!("{}{}", column_name(self.col), self.row + 1)
    }

    /// This cell moved by a number of rows and columns, or `None` off the sheet.
    pub fn offset(&self, rows: i64, cols: i64) -> Option<Self> {
        let row = self.row as i64 + rows;
        let col = self.col as i64 + cols;
        if row < 0 || col < 0 || row >= MAX_ROWS as i64 || col >= MAX_COLS as i64 {
            return None;
        }
        Some(Self { row: row as u32, col: col as u32 })
    }
}

impl std::fmt::Display for CellRef {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}", self.a1())
    }
}

/// A rectangle of cells, `first` at the top left and `last` at the bottom right, both included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Range {
    pub first: CellRef,
    pub last: CellRef,
}

impl Range {
    /// The rectangle with these two corners, in either order.
    pub fn new(a: CellRef, b: CellRef) -> Self {
        Self {
            first: CellRef::new(a.row.min(b.row), a.col.min(b.col)),
            last: CellRef::new(a.row.max(b.row), a.col.max(b.col)),
        }
    }

    pub fn cell(at: CellRef) -> Self {
        Self { first: at, last: at }
    }

    /// Read `A1:B2`, `B4`, `A:C` or `3:5`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        match text.split_once(':') {
            None => CellRef::parse(text).map(Self::cell),
            Some((a, b)) => {
                if let (Some(a), Some(b)) = (CellRef::parse(a), CellRef::parse(b)) {
                    return Some(Self::new(a, b));
                }
                let strip = |s: &str| s.trim().replace('$', "");
                let (a, b) = (strip(a), strip(b));
                if let (Some(a), Some(b)) = (column_number(&a), column_number(&b)) {
                    return Some(Self::new(CellRef::new(0, a), CellRef::new(MAX_ROWS - 1, b)));
                }
                let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
                if a == 0 || b == 0 || a > MAX_ROWS || b > MAX_ROWS {
                    return None;
                }
                Some(Self::new(CellRef::new(a - 1, 0), CellRef::new(b - 1, MAX_COLS - 1)))
            }
        }
    }

    /// `A1:B2`, or `B4` for one cell, or `A:C` and `3:5` for whole columns and rows.
    pub fn a1(&self) -> String {
        if self.first == self.last {
            return self.first.a1();
        }
        if self.first.row == 0 && self.last.row == MAX_ROWS - 1 {
            return format!("{}:{}", column_name(self.first.col), column_name(self.last.col));
        }
        if self.first.col == 0 && self.last.col == MAX_COLS - 1 {
            return format!("{}:{}", self.first.row + 1, self.last.row + 1);
        }
        format!("{}:{}", self.first.a1(), self.last.a1())
    }

    pub fn rows(&self) -> u32 {
        self.last.row - self.first.row + 1
    }

    pub fn cols(&self) -> u32 {
        self.last.col - self.first.col + 1
    }

    pub fn len(&self) -> u64 {
        self.rows() as u64 * self.cols() as u64
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn contains(&self, at: CellRef) -> bool {
        at.row >= self.first.row
            && at.row <= self.last.row
            && at.col >= self.first.col
            && at.col <= self.last.col
    }

    pub fn intersects(&self, other: &Range) -> bool {
        self.first.row <= other.last.row
            && other.first.row <= self.last.row
            && self.first.col <= other.last.col
            && other.first.col <= self.last.col
    }

    pub fn intersection(&self, other: &Range) -> Option<Range> {
        if !self.intersects(other) {
            return None;
        }
        Some(Range {
            first: CellRef::new(
                self.first.row.max(other.first.row),
                self.first.col.max(other.first.col),
            ),
            last: CellRef::new(
                self.last.row.min(other.last.row),
                self.last.col.min(other.last.col),
            ),
        })
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, other: &Range) -> Range {
        Range {
            first: CellRef::new(
                self.first.row.min(other.first.row),
                self.first.col.min(other.first.col),
            ),
            last: CellRef::new(
                self.last.row.max(other.last.row),
                self.last.col.max(other.last.col),
            ),
        }
    }

    pub fn is_whole_columns(&self) -> bool {
        self.first.row == 0 && self.last.row == MAX_ROWS - 1
    }

    pub fn is_whole_rows(&self) -> bool {
        self.first.col == 0 && self.last.col == MAX_COLS - 1
    }

    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = CellRef> + '_ {
        (self.first.row..=self.last.row).flat_map(move |row| {
            (self.first.col..=self.last.col).map(move |col| CellRef::new(row, col))
        })
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}", self.a1())
    }
}

/// `A` for 0, `Z` for 25, `AA` for 26, `XFD` for 16,383.
pub fn column_name(col: u32) -> String {
    let mut n = col as u64 + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.push(b'A' + rem);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The column a name like `AB` names, or `None` past `XFD` or for anything that is not letters.
pub fn column_number(name: &str) -> Option<u32> {
    if name.is_empty() || name.len() > 3 || !name.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut n: u32 = 0;
    for b in name.bytes() {
        n = n * 26 + (b.to_ascii_uppercase() - b'A' + 1) as u32;
    }
    (1..=MAX_COLS).contains(&n).then(|| n - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_go_from_a_to_xfd_and_back() {
        for (col, name) in [
            (0, "A"),
            (25, "Z"),
            (26, "AA"),
            (51, "AZ"),
            (52, "BA"),
            (701, "ZZ"),
            (702, "AAA"),
            (16383, "XFD"),
        ] {
            assert_eq!(column_name(col), name);
            assert_eq!(column_number(name), Some(col));
        }
        assert_eq!(column_number("XFE"), None);
        assert_eq!(column_number("A1"), None);
    }

    #[test]
    fn a_reference_reads_with_or_without_dollars_and_refuses_row_zero() {
        assert_eq!(CellRef::parse("B4"), Some(CellRef::new(3, 1)));
        assert_eq!(CellRef::parse("$b$4"), Some(CellRef::new(3, 1)));
        assert_eq!(CellRef::parse("A0"), None);
        assert_eq!(CellRef::parse("A1048577"), None);
        assert_eq!(CellRef::parse("XFD1048576"), Some(CellRef::new(MAX_ROWS - 1, MAX_COLS - 1)));
    }

    #[test]
    fn ranges_read_and_print_in_every_shape() {
        for text in ["A1:C3", "B4", "A:C", "3:5"] {
            assert_eq!(Range::parse(text).unwrap().a1(), text);
        }
        assert_eq!(Range::parse("C3:A1").unwrap().a1(), "A1:C3");
        let r = Range::parse("B2:D4").unwrap();
        assert!(r.contains(CellRef::new(3, 3)));
        assert!(!r.contains(CellRef::new(4, 3)));
        assert_eq!(r.len(), 9);
        assert_eq!(r.intersection(&Range::parse("C3:F9").unwrap()).unwrap().a1(), "C3:D4");
    }
}
