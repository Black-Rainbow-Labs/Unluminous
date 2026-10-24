//! Find and Replace with Excel's options.
//!
//! A search looks at the formulas, the values as they are shown, the words of notes or the words of
//! threaded comments. It can go along rows or down columns, match case or not, match the whole cell or part
//! of it, take `?`, `*` and `~` as wildcards, and keep only the cells whose format is the format of another
//! cell. It answers every match in the order Excel's Find and Find Next visit them.

use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::formula::print::print;
use crate::model::{Cell, Sheet, Workbook};
use crate::numfmt::Format;
use crate::value::Value;

/// What a search looks at in each cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookIn {
    /// A formula's text, or what was typed into a cell with none.
    Formulas,
    /// What the cell shows.
    Values,
    /// The words of the cell's note.
    Notes,
    /// The words of the cell's threaded comment and its replies.
    Comments,
}

/// Where a search goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Sheet(usize),
    Workbook,
}

/// Every option of Excel's Find and Replace dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct FindOptions {
    /// The text to find. `?` is any one character, `*` is any run of characters and `~` makes the next
    /// character itself. Empty with a format finds the cells with that format.
    pub what: String,
    pub look_in: LookIn,
    /// Down the columns rather than along the rows.
    pub by_columns: bool,
    pub match_case: bool,
    /// The whole cell must match rather than part of it.
    pub entire_cell: bool,
    /// Keep the cells whose number format, font and fill are those of this cell format (an index into the
    /// workbook's cell formats).
    pub format: Option<u32>,
    pub scope: Scope,
    /// Search only these cells of the sheet, a selection.
    pub within: Option<Vec<Range>>,
}

impl FindOptions {
    /// Find some text anywhere in a sheet, by rows, ignoring case, in part of a cell.
    pub fn new(what: &str, scope: Scope) -> Self {
        Self {
            what: what.to_owned(),
            look_in: LookIn::Formulas,
            by_columns: false,
            match_case: false,
            entire_cell: false,
            format: None,
            scope,
            within: None,
        }
    }
}

/// A cell a search found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub sheet: usize,
    pub at: CellRef,
}

/// One piece of a search pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Literal(char),
    Any,
    Run,
}

/// A search pattern read with Excel's wildcards.
fn pattern_of(what: &str, case: bool) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut chars = what.chars();
    while let Some(c) = chars.next() {
        pieces.push(match c {
            '?' => Piece::Any,
            '*' => Piece::Run,
            '~' => match chars.next() {
                Some(next) => Piece::Literal(fold(next, case)),
                None => Piece::Literal('~'),
            },
            other => Piece::Literal(fold(other, case)),
        });
    }
    pieces
}

fn fold(c: char, case: bool) -> char {
    if case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// The end of the longest match of a pattern that starts at a place in the text.
fn match_from(pattern: &[Piece], text: &[char], start: usize, case: bool) -> Option<usize> {
    match pattern.split_first() {
        None => Some(start),
        Some((Piece::Run, rest)) => {
            (start..=text.len()).rev().find_map(|end| match_from(rest, text, end, case))
        }
        Some((Piece::Any, rest)) => {
            if start < text.len() {
                match_from(rest, text, start + 1, case)
            } else {
                None
            }
        }
        Some((Piece::Literal(l), rest)) => {
            if start < text.len() && fold(text[start], case) == *l {
                match_from(rest, text, start + 1, case)
            } else {
                None
            }
        }
    }
}

/// Where a pattern first matches in some text, as character positions: the whole text when `whole`.
pub fn wildcard_find(
    what: &str,
    text: &str,
    match_case: bool,
    whole: bool,
) -> Option<(usize, usize)> {
    let pattern = pattern_of(what, match_case);
    let chars: Vec<char> = text.chars().collect();
    if whole {
        return (match_whole(&pattern, &chars, 0, 0, match_case)).then_some((0, chars.len()));
    }
    (0..=chars.len())
        .find_map(|start| match_from(&pattern, &chars, start, match_case).map(|end| (start, end)))
}

/// Whether the pattern matches all of the text from a place in each.
fn match_whole(pattern: &[Piece], text: &[char], p: usize, t: usize, case: bool) -> bool {
    match pattern.get(p) {
        None => t == text.len(),
        Some(Piece::Run) => {
            (t..=text.len()).any(|end| match_whole(pattern, text, p + 1, end, case))
        }
        Some(Piece::Any) => t < text.len() && match_whole(pattern, text, p + 1, t + 1, case),
        Some(Piece::Literal(l)) => {
            t < text.len()
                && fold(text[t], case) == *l
                && match_whole(pattern, text, p + 1, t + 1, case)
        }
    }
}

/// The text with every non overlapping match of the pattern replaced, or `None` when nothing matched.
pub fn wildcard_replace(
    what: &str,
    text: &str,
    with: &str,
    match_case: bool,
    whole: bool,
) -> Option<String> {
    if what.is_empty() {
        return None;
    }
    if whole {
        return wildcard_find(what, text, match_case, true).map(|_| with.to_owned());
    }
    let pattern = pattern_of(what, match_case);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let (mut at, mut found) = (0, false);
    while at <= chars.len() {
        match match_from(&pattern, &chars, at, match_case) {
            Some(end) if end > at || at == chars.len() => {
                found = true;
                out.push_str(with);
                if end == at {
                    break;
                }
                at = end;
            }
            _ => {
                if at < chars.len() {
                    out.push(chars[at]);
                }
                at += 1;
            }
        }
    }
    found.then_some(out)
}

/// What a cell holds as a search sees it with Look in set to formulas: a formula's text, or what a
/// person would type to get the value.
pub fn formula_text(book: &Workbook, cell: &Cell) -> String {
    match &cell.formula {
        Some(f) => format!("={}", print(&f.expr, false)),
        None => typed_text(book, &cell.value),
    }
}

/// A value as the formula bar shows it for a cell with no formula.
fn typed_text(_book: &Workbook, value: &Value) -> String {
    value.to_text()
}

/// What a cell shows, with its number format applied and no limit on the width.
pub fn shown_value(book: &Workbook, cell: &Cell) -> String {
    let code = book.styles.format_code(cell.style);
    Format::parse(&code).render(&cell.value, book.date1904, 255).text
}

/// Whether a cell's format is the format of a template: the same number format, font and fill.
fn same_format(book: &Workbook, a: u32, b: u32) -> bool {
    let styles = &book.styles;
    styles.format_code(a) == styles.format_code(b)
        && styles.font_of(a) == styles.font_of(b)
        && styles.fill_of(a) == styles.fill_of(b)
}

/// The texts of a cell a search with these options looks at.
fn texts_of(
    book: &Workbook,
    sheet: &Sheet,
    key: (u32, u32),
    cell: Option<&Cell>,
    look: LookIn,
) -> Vec<String> {
    match look {
        LookIn::Formulas => cell.map(|c| formula_text(book, c)).into_iter().collect(),
        LookIn::Values => match cell {
            Some(c) => vec![shown_value(book, c)],
            None => sheet.spills.values.get(&key).map(|v| v.to_text()).into_iter().collect(),
        },
        LookIn::Notes => sheet.notes.get(&key).map(|n| n.text.clone()).into_iter().collect(),
        LookIn::Comments => sheet
            .threads
            .get(&key)
            .map(|t| t.messages().map(|m| m.text.clone()).collect())
            .unwrap_or_default(),
    }
}

/// The keys of the cells a search looks at on one sheet, in the order it visits them.
fn keys_in_order(sheet: &Sheet, opts: &FindOptions) -> Vec<(u32, u32)> {
    let mut keys: Vec<(u32, u32)> = sheet.cells.keys().copied().collect();
    keys.extend(sheet.spills.values.keys().copied());
    keys.extend(sheet.notes.keys().copied());
    keys.extend(sheet.threads.keys().copied());
    keys.sort_unstable();
    keys.dedup();
    if let Some(within) = &opts.within {
        keys.retain(|(r, c)| within.iter().any(|range| range.contains(CellRef::new(*r, *c))));
    }
    if opts.by_columns {
        keys.sort_by_key(|&(r, c)| (c, r));
    }
    keys
}

/// Every cell the options find, in the order Find and Find Next visit them: sheet by sheet, and within a
/// sheet along the rows or down the columns.
pub fn find_all(book: &Workbook, opts: &FindOptions) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (si, sheet) in book.sheets.iter().enumerate() {
        if matches!(opts.scope, Scope::Sheet(only) if only != si) {
            continue;
        }
        for key in keys_in_order(sheet, opts) {
            let cell = sheet.cells.get(&key);
            if let Some(template) = opts.format {
                if !cell.is_some_and(|c| same_format(book, c.style, template)) {
                    continue;
                }
            }
            let hit = if opts.what.is_empty() {
                opts.format.is_some()
            } else {
                texts_of(book, sheet, key, cell, opts.look_in).iter().any(|text| {
                    wildcard_find(&opts.what, text, opts.match_case, opts.entire_cell).is_some()
                })
            };
            if hit {
                hits.push(Hit { sheet: si, at: CellRef::new(key.0, key.1) });
            }
        }
    }
    hits
}

impl Session {
    /// Every cell a search finds, in Excel's order.
    pub fn find_cells(&self, opts: &FindOptions) -> Vec<Hit> {
        find_all(&self.book, opts)
    }

    /// The match after a cell, or before it, wrapping round the sheet, the way Find Next goes. `None` when
    /// nothing matches.
    pub fn find_next(
        &self,
        opts: &FindOptions,
        after: Option<Hit>,
        backwards: bool,
    ) -> Option<Hit> {
        let hits = find_all(&self.book, opts);
        let key = |h: &Hit| {
            (h.sheet, if opts.by_columns { (h.at.col, h.at.row) } else { (h.at.row, h.at.col) })
        };
        let Some(from) = after else {
            return if backwards { hits.last().copied() } else { hits.first().copied() };
        };
        let here = key(&from);
        if backwards {
            hits.iter().rev().find(|h| key(h) < here).or(hits.last()).copied()
        } else {
            hits.iter().find(|h| key(h) > here).or(hits.first()).copied()
        }
    }

    /// Replace in every cell the options find, in the formulas or what was typed. Notes and comments are
    /// not replaced into, as in Excel. Answers how many cells changed.
    pub fn replace_cells(&mut self, opts: &FindOptions, with: &str) -> Result<usize, String> {
        if opts.what.is_empty() {
            return Err("There is nothing to find.".to_owned());
        }
        let mut formulas = opts.clone();
        formulas.look_in = LookIn::Formulas;
        let hits = find_all(&self.book, &formulas);
        self.replace_in_hits(&hits, opts, with)
    }

    /// Replace in one cell: the Replace button.
    pub fn replace_cell(
        &mut self,
        opts: &FindOptions,
        with: &str,
        hit: Hit,
    ) -> Result<bool, String> {
        Ok(self.replace_in_hits(&[hit], opts, with)? > 0)
    }

    fn replace_in_hits(
        &mut self,
        hits: &[Hit],
        opts: &FindOptions,
        with: &str,
    ) -> Result<usize, String> {
        let mut by_sheet: std::collections::BTreeMap<usize, Vec<(CellRef, String)>> =
            Default::default();
        for hit in hits {
            let Some(cell) = self.book.sheets[hit.sheet].cell(hit.at) else { continue };
            let source = formula_text(&self.book, cell);
            if let Some(next) =
                wildcard_replace(&opts.what, &source, with, opts.match_case, opts.entire_cell)
            {
                if next != source {
                    by_sheet.entry(hit.sheet).or_default().push((hit.at, next));
                }
            }
        }
        let mut count = 0;
        for (si, inputs) in by_sheet {
            count += inputs.len();
            self.set_inputs(si, &inputs)?;
        }
        Ok(count)
    }
}
