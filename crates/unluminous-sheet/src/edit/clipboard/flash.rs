//! Flash Fill: learn from the examples a person typed beside some data how the text is made, and make it
//! for the other rows.
//!
//! A pattern is a join of pieces. A piece is a constant, or a stretch of one of the neighbouring columns
//! (from one place to another, optionally in capitals, lower case or proper case). A place in a column is
//! named by counting characters, or by a delimiter ("after the second slash") or by a kind of word ("the
//! start of the third run of letters"). The patterns that make every example are searched, and the
//! simplest is used. When no pattern makes every example there is nothing to fill.

use std::collections::HashMap;

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::Cell;
use crate::numfmt::Format;
use crate::value::Value;

/// Excel's refusal when the examples fit no pattern.
const NO_PATTERN: &str = "Flash Fill didn't recognize a pattern. Enter more examples, or check the ones you entered, and try again.";

/// The most pieces a pattern is made of.
const MOST_PIECES: usize = 12;

/// A kind of run of characters that places are counted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Class {
    Digits,
    Letters,
    Upper,
    Lower,
    AlphaNumeric,
    Word,
}

impl Class {
    const ALL: [Class; 6] =
        [Class::Digits, Class::Letters, Class::Upper, Class::Lower, Class::AlphaNumeric, Class::Word];

    fn holds(self, c: char) -> bool {
        match self {
            Class::Digits => c.is_ascii_digit(),
            Class::Letters => c.is_alphabetic(),
            Class::Upper => c.is_uppercase(),
            Class::Lower => c.is_lowercase(),
            Class::AlphaNumeric => c.is_alphanumeric(),
            Class::Word => !c.is_whitespace(),
        }
    }

    /// The start and end of every run of this kind.
    fn runs(self, chars: &[char]) -> Vec<(usize, usize)> {
        let mut runs = Vec::new();
        let mut start = None;
        for (i, c) in chars.iter().enumerate() {
            match (self.holds(*c), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    runs.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            runs.push((s, chars.len()));
        }
        runs
    }
}

/// A place in a text.
#[derive(Debug, Clone, PartialEq)]
enum Pos {
    /// This many characters from the start.
    FromStart(usize),
    /// This many characters from the end.
    FromEnd(usize),
    /// Just after the k-th delimiter (counting from the end when k is negative).
    After(char, i32),
    /// Just before the k-th delimiter.
    Before(char, i32),
    /// The start of the k-th run of a kind.
    RunStart(Class, i32),
    /// The end of the k-th run of a kind.
    RunEnd(Class, i32),
}

/// Where a stretch ends: at a place, or a number of characters on.
#[derive(Debug, Clone, PartialEq)]
enum Until {
    At(Pos),
    Len(usize),
}

/// How the stretch is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Letters {
    AsIs,
    Upper,
    Lower,
    Proper,
}

/// One piece of a pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Constant(String),
    Part { column: usize, from: Pos, until: Until, letters: Letters },
}

fn pick(k: i32, count: usize) -> Option<usize> {
    let count = count as i32;
    let index = if k > 0 { k - 1 } else { count + k };
    (0..count).contains(&index).then_some(index as usize)
}

impl Pos {
    /// The index in the text this place names, or `None` when the text has no such place.
    fn index(&self, chars: &[char]) -> Option<usize> {
        let len = chars.len();
        match self {
            Pos::FromStart(n) => (*n <= len).then_some(*n),
            Pos::FromEnd(n) => len.checked_sub(*n),
            Pos::After(c, k) | Pos::Before(c, k) => {
                let found: Vec<usize> =
                    chars.iter().enumerate().filter(|(_, x)| *x == c).map(|(i, _)| i).collect();
                let at = found[pick(*k, found.len())?];
                Some(if matches!(self, Pos::After(..)) { at + 1 } else { at })
            }
            Pos::RunStart(class, k) | Pos::RunEnd(class, k) => {
                let runs = class.runs(chars);
                let (s, e) = runs[pick(*k, runs.len())?];
                Some(if matches!(self, Pos::RunStart(..)) { s } else { e })
            }
        }
    }

    /// How complicated a place is; simpler places are preferred.
    fn cost(&self) -> u32 {
        match self {
            Pos::FromStart(0) | Pos::FromEnd(0) => 1,
            Pos::FromStart(_) | Pos::FromEnd(_) => 6,
            Pos::After(_, k) | Pos::Before(_, k) => match k.unsigned_abs() {
                1 => 1,
                n => 2 + (n - 1),
            },
            Pos::RunStart(_, k) | Pos::RunEnd(_, k) => 2 + (k.unsigned_abs() - 1),
        }
    }

    /// Every way of naming an index in a text.
    fn naming(chars: &[char], index: usize) -> Vec<Pos> {
        let mut out = vec![Pos::FromStart(index), Pos::FromEnd(chars.len() - index)];
        let mut delimiters: Vec<char> = chars.iter().copied().filter(|c| !c.is_alphanumeric()).collect();
        delimiters.sort_unstable();
        delimiters.dedup();
        for d in delimiters {
            let found: Vec<usize> =
                chars.iter().enumerate().filter(|(_, x)| **x == d).map(|(i, _)| i).collect();
            for (n, at) in found.iter().enumerate() {
                let (k, back) = (n as i32 + 1, n as i32 - found.len() as i32);
                if *at + 1 == index {
                    out.extend([Pos::After(d, k), Pos::After(d, back)]);
                }
                if *at == index {
                    out.extend([Pos::Before(d, k), Pos::Before(d, back)]);
                }
            }
        }
        for class in Class::ALL {
            let runs = class.runs(chars);
            for (n, (s, e)) in runs.iter().enumerate() {
                let (k, back) = (n as i32 + 1, n as i32 - runs.len() as i32);
                if *s == index {
                    out.extend([Pos::RunStart(class, k), Pos::RunStart(class, back)]);
                }
                if *e == index {
                    out.extend([Pos::RunEnd(class, k), Pos::RunEnd(class, back)]);
                }
            }
        }
        out
    }
}

fn proper(text: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in text.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = !c.is_alphanumeric() && c != '\'';
        }
    }
    out
}

impl Letters {
    fn write(self, text: &str) -> String {
        match self {
            Letters::AsIs => text.to_owned(),
            Letters::Upper => text.to_uppercase(),
            Letters::Lower => text.to_lowercase(),
            Letters::Proper => proper(text),
        }
    }
}

impl Piece {
    /// The text this piece makes from the neighbouring columns of a row.
    fn make(&self, row: &[Vec<char>]) -> Option<String> {
        match self {
            Piece::Constant(text) => Some(text.clone()),
            Piece::Part { column, from, until, letters } => {
                let chars = row.get(*column)?;
                let start = from.index(chars)?;
                let end = match until {
                    Until::At(p) => p.index(chars)?,
                    Until::Len(n) => (start + n <= chars.len()).then_some(start + n)?,
                };
                (start < end).then(|| letters.write(&chars[start..end].iter().collect::<String>()))
            }
        }
    }

    fn cost(&self) -> u32 {
        match self {
            // Letters and digits are better taken from the data, so a constant that holds them is dear.
            Piece::Constant(text) => {
                4 + text.chars().map(|c| if c.is_alphanumeric() { 8 } else { 1 }).sum::<u32>()
            }
            Piece::Part { from, until, letters, .. } => {
                let end = match until {
                    Until::At(p) => p.cost(),
                    Until::Len(_) => 4,
                };
                1 + from.cost() + end + 2 * u32::from(*letters != Letters::AsIs)
            }
        }
    }
}

/// How many ways of naming each end of a stretch are tried.
const NAMINGS_KEPT: usize = 24;

/// Every piece that makes `target` from the first example's row.
fn pieces_making(target: &str, row: &[Vec<char>]) -> Vec<Piece> {
    let want: Vec<char> = target.chars().collect();
    let mut out = vec![Piece::Constant(target.to_owned())];
    for (column, chars) in row.iter().enumerate() {
        for letters in [Letters::AsIs, Letters::Upper, Letters::Lower, Letters::Proper] {
            for start in 0..chars.len().saturating_sub(want.len().saturating_sub(1)) {
                let end = start + want.len();
                if end > chars.len() {
                    break;
                }
                let got: String = chars[start..end].iter().collect();
                if letters.write(&got) != target {
                    continue;
                }
                out.extend(parts_for(column, chars, start, end, letters));
            }
        }
    }
    out
}

/// The ways of saying that a stretch of a text runs from `start` to `end`.
fn parts_for(column: usize, chars: &[char], start: usize, end: usize, letters: Letters) -> Vec<Piece> {
    let mut froms = Pos::naming(chars, start);
    froms.sort_by_key(Pos::cost);
    froms.truncate(NAMINGS_KEPT);
    let mut untils: Vec<Until> = Pos::naming(chars, end).into_iter().map(Until::At).collect();
    untils.sort_by_key(|u| match u {
        Until::At(p) => p.cost(),
        Until::Len(_) => 4,
    });
    untils.truncate(NAMINGS_KEPT);
    untils.push(Until::Len(end - start));
    let mut out = Vec::new();
    for from in &froms {
        for until in &untils {
            out.push(Piece::Part { column, from: from.clone(), until: until.clone(), letters });
        }
    }
    out
}

/// One example: the neighbouring columns of its row, and what was typed.
struct Example {
    row: Vec<Vec<char>>,
    output: Vec<char>,
}

/// The cheapest pattern that makes every example, searched over where each example's output has got to.
struct Search<'a> {
    examples: &'a [Example],
    memo: HashMap<Vec<usize>, Option<(u32, Vec<Piece>)>>,
    candidates: HashMap<(usize, usize), Vec<Piece>>,
}

impl Search<'_> {
    fn best(&mut self, at: Vec<usize>, depth: usize) -> Option<(u32, Vec<Piece>)> {
        if at.iter().zip(self.examples).all(|(a, e)| *a == e.output.len()) {
            return Some((0, Vec::new()));
        }
        if at.iter().zip(self.examples).any(|(a, e)| *a >= e.output.len()) || depth >= MOST_PIECES {
            return None;
        }
        if let Some(done) = self.memo.get(&at) {
            return done.clone();
        }
        let mut best: Option<(u32, Vec<Piece>)> = None;
        let first = &self.examples[0];
        for b in at[0] + 1..=first.output.len() {
            let pieces = self.pieces_for(at[0], b);
            for piece in pieces {
                let Some(next) = self.advance(&at, &piece) else { continue };
                let Some((tail_cost, tail)) = self.best(next, depth + 1) else { continue };
                let cost = piece.cost() + tail_cost;
                if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                    let mut program = vec![piece];
                    program.extend(tail);
                    best = Some((cost, program));
                }
            }
        }
        self.memo.insert(at, best.clone());
        best
    }

    fn pieces_for(&mut self, a: usize, b: usize) -> Vec<Piece> {
        if let Some(found) = self.candidates.get(&(a, b)) {
            return found.clone();
        }
        let target: String = self.examples[0].output[a..b].iter().collect();
        let found = pieces_making(&target, &self.examples[0].row);
        self.candidates.insert((a, b), found.clone());
        found
    }

    /// Where every example has got to after a piece, or `None` when the piece does not make what the
    /// example has next.
    fn advance(&self, at: &[usize], piece: &Piece) -> Option<Vec<usize>> {
        let mut next = Vec::with_capacity(at.len());
        for (a, e) in at.iter().zip(self.examples) {
            let made: Vec<char> = piece.make(&e.row)?.chars().collect();
            if made.is_empty() || e.output.len() < a + made.len() || e.output[*a..a + made.len()] != made[..] {
                return None;
            }
            next.push(a + made.len());
        }
        Some(next)
    }
}

/// Learn how the examples were made and make the outputs for other rows. Each example is the text of the
/// neighbouring columns of its row and the text typed. `rows` are the neighbouring columns of the rows to
/// fill; a row the pattern cannot be made for gets `None`.
pub fn flash_fill_examples(
    examples: &[(Vec<String>, String)],
    rows: &[Vec<String>],
) -> Result<Vec<Option<String>>, String> {
    let chars = |row: &[String]| row.iter().map(|c| c.chars().collect::<Vec<char>>()).collect::<Vec<_>>();
    let examples: Vec<Example> = examples
        .iter()
        .map(|(row, out)| Example { row: chars(row), output: out.chars().collect() })
        .collect();
    if examples.is_empty() {
        return Err(NO_PATTERN.to_owned());
    }
    let mut search = Search { examples: &examples, memo: HashMap::new(), candidates: HashMap::new() };
    let Some((_, program)) = search.best(vec![0; examples.len()], 0) else {
        return Err(NO_PATTERN.to_owned());
    };
    Ok(rows
        .iter()
        .map(|row| {
            let row = chars(row);
            if row.iter().all(Vec::is_empty) {
                return None;
            }
            program.iter().map(|p| p.make(&row)).collect::<Option<Vec<String>>>().map(|v| v.concat())
        })
        .collect())
}

impl Session {
    /// Flash Fill the cells of one column: the cells that already hold text are the examples, and the empty
    /// cells beside data get the text the pattern makes. The data is in the columns next to it on the left
    /// (or on the right when there are none on the left).
    pub fn flash_fill(&mut self, sheet: usize, range: Range) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        if range.cols() != 1 {
            return Err("Flash Fill fills one column.".to_owned());
        }
        let column = range.first.col;
        let inputs = self.flash_input_columns(sheet, range);
        if inputs.is_empty() || range.rows() < 2 {
            return Err(NO_PATTERN.to_owned());
        }
        let (examples, targets) = self.flash_rows(sheet, range, &inputs);
        if examples.is_empty() {
            return Err(NO_PATTERN.to_owned());
        }
        let rows: Vec<Vec<String>> = targets.iter().map(|(_, row)| row.clone()).collect();
        let made = flash_fill_examples(&examples, &rows)?;
        // Examples that are numbers make numbers; examples that are text make text, whatever it looks like.
        let typed = self.flash_examples_are_numbers(sheet, range);
        self.remember_cells("Flash Fill", sheet, &[range], true);
        for ((r, _), text) in targets.iter().zip(made) {
            if let Some(text) = text {
                let at = CellRef::new(*r, column);
                if typed {
                    self.enter_text(sheet, at, &text, false);
                } else {
                    self.enter_plain_text(sheet, at, &text);
                }
            }
        }
        self.recalculate();
        Ok(range)
    }

    /// Ctrl+E: Flash Fill the column of a cell as far as the data beside it goes.
    pub fn flash_fill_down(&mut self, sheet: usize, at: CellRef) -> Result<Range, String> {
        self.check_sheet(sheet)?;
        let s = &self.book.sheets[sheet];
        let filled = |r: u32, c: u32| s.cells.get(&(r, c)).is_some_and(|x| !x.value.is_empty());
        let beside = [at.col.checked_sub(1), Some(at.col + 1)].into_iter().flatten().find(|c| filled(at.row, *c));
        let Some(beside) = beside else { return Err(NO_PATTERN.to_owned()) };
        let (mut first, mut last) = (at.row, at.row);
        while first > 0 && filled(first - 1, beside) {
            first -= 1;
        }
        while filled(last + 1, beside) {
            last += 1;
        }
        let range = Range::new(CellRef::new(first, at.col), CellRef::new(last, at.col));
        self.flash_fill(sheet, range)
    }

    /// The columns beside the output column that hold the data: the ones to the left that have anything in
    /// the rows, up to the first that has not, or else the ones to the right.
    fn flash_input_columns(&self, sheet: usize, range: Range) -> Vec<u32> {
        let s = &self.book.sheets[sheet];
        let has_data = |c: u32| {
            (range.first.row..=range.last.row)
                .any(|r| s.cells.get(&(r, c)).is_some_and(|x| !x.value.is_empty()))
        };
        let mut left = Vec::new();
        let mut c = range.first.col;
        while c > 0 && has_data(c - 1) && left.len() < 8 {
            c -= 1;
            left.push(c);
        }
        left.reverse();
        if !left.is_empty() {
            return left;
        }
        let mut right = Vec::new();
        let mut c = range.first.col + 1;
        while c < crate::coord::MAX_COLS && has_data(c) && right.len() < 8 {
            right.push(c);
            c += 1;
        }
        right
    }

    /// The examples (rows whose output cell holds text) and the rows to fill (empty output beside data).
    #[allow(clippy::type_complexity)]
    fn flash_rows(
        &self,
        sheet: usize,
        range: Range,
        inputs: &[u32],
    ) -> (Vec<(Vec<String>, String)>, Vec<(u32, Vec<String>)>) {
        let (mut examples, mut targets) = (Vec::new(), Vec::new());
        for r in range.first.row..=range.last.row {
            let row: Vec<String> = inputs.iter().map(|c| self.flash_text(sheet, CellRef::new(r, *c))).collect();
            let typed = self.flash_text(sheet, CellRef::new(r, range.first.col));
            if row.iter().all(String::is_empty) {
                continue;
            }
            if typed.is_empty() {
                targets.push((r, row));
            } else {
                examples.push((row, typed));
            }
        }
        (examples, targets)
    }

    /// Whether every example is a number.
    fn flash_examples_are_numbers(&self, sheet: usize, range: Range) -> bool {
        let s = &self.book.sheets[sheet];
        let filled: Vec<&Cell> = (range.first.row..=range.last.row)
            .filter_map(|r| s.cell(CellRef::new(r, range.first.col)))
            .filter(|c| !c.value.is_empty())
            .collect();
        !filled.is_empty() && filled.iter().all(|c| matches!(c.value, Value::Number(_)))
    }

    /// A cell's text as it is shown.
    fn flash_text(&self, sheet: usize, at: CellRef) -> String {
        let Some(cell) = self.book.sheets[sheet].cell(at) else { return String::new() };
        let code = self.book.styles.format_code(cell.style);
        Format::parse(&code).render(&cell.value, self.book.date1904, 255).text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learn(rows: &[(&[&str], &str)], more: &[&[&str]]) -> Vec<Option<String>> {
        let owned = |r: &[&str]| r.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
        let examples: Vec<(Vec<String>, String)> = rows.iter().map(|(r, o)| (owned(r), (*o).to_owned())).collect();
        let rest: Vec<Vec<String>> = more.iter().map(|r| owned(r)).collect();
        flash_fill_examples(&examples, &rest).unwrap()
    }

    #[test]
    fn the_first_word_of_a_name() {
        let out = learn(&[(&["John Smith"], "John")], &[&["Grace Hopper"], &["Ada Lovelace"]]);
        assert_eq!(out, [Some("Grace".to_owned()), Some("Ada".to_owned())]);
    }

    #[test]
    fn last_name_first_with_a_comma() {
        let out = learn(&[(&["John Smith"], "Smith, John")], &[&["Mary Jones"]]);
        assert_eq!(out, [Some("Jones, Mary".to_owned())]);
    }

    #[test]
    fn two_columns_joined_with_a_literal() {
        let out = learn(&[(&["John", "Smith"], "Mr. John Smith")], &[&["Mary", "Jones"]]);
        assert_eq!(out, [Some("Mr. Mary Jones".to_owned())]);
    }

    #[test]
    fn capitals_lower_case_and_proper_case() {
        assert_eq!(learn(&[(&["john smith"], "JOHN SMITH")], &[&["mary jones"]])[0].as_deref(), Some("MARY JONES"));
        assert_eq!(learn(&[(&["JOHN SMITH"], "john smith")], &[&["MARY JONES"]])[0].as_deref(), Some("mary jones"));
        assert_eq!(learn(&[(&["jOHN sMITH"], "John Smith")], &[&["mARY jONES"]])[0].as_deref(), Some("Mary Jones"));
    }

    #[test]
    fn digits_are_taken_out_of_text() {
        let out = learn(&[(&["ABC-123"], "123")], &[&["XYZ-9"], &["QRS-4567"]]);
        assert_eq!(out, [Some("9".to_owned()), Some("4567".to_owned())]);
    }

    #[test]
    fn a_phone_number_is_laid_out_by_position() {
        let out = learn(&[(&["5551234567"], "(555) 123-4567")], &[&["4155550100"]]);
        assert_eq!(out, [Some("(415) 555-0100".to_owned())]);
    }

    #[test]
    fn spaces_round_a_word_are_not_part_of_it() {
        let out = learn(&[(&["  a  "], "a")], &[&[" b"], &["c  "]]);
        assert_eq!(out, [Some("b".to_owned()), Some("c".to_owned())]);
    }

    #[test]
    fn examples_that_fit_no_pattern_are_refused() {
        let examples = vec![(vec!["abc".to_owned()], "zzz".to_owned()), (vec!["def".to_owned()], "q".to_owned())];
        assert!(flash_fill_examples(&examples, &[]).is_err());
    }
}
