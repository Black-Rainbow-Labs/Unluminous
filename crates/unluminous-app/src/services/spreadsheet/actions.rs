//! Every change a spreadsheet tab can make, as a value, and the one function that makes it.
//!
//! The grid, the bar, the Format panel, the menus and an agent's `plugins run spreadsheet` verbs all
//! describe what they want as an [`Act`] and hand it to [`apply`], so a change made with the pointer and
//! the same change asked for by an agent are one change, through `unluminous_sheet::edit::Session`.

use unluminous_sheet::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use unluminous_sheet::edit::{clip_to_used, Clear, Clip, Direction, PasteMode, Shift};
use unluminous_sheet::model::{
    CfRule, Chart, ChartKind, ChartSeries, FilterCriteria, LinkTarget, Validation, Visibility,
};
use unluminous_sheet::styles::{BorderEdit, Color, Edge, Sides, StyleChange};
use unluminous_sheet::{Sheet, Value};

use super::Book;

/// Cells this window copied, with the text that went on the clipboard for them.
#[derive(Debug, Clone)]
pub struct Copied {
    /// The book they came from.
    pub key: String,
    pub clip: Clip,
    pub text: String,
}

/// Which sides a border goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderSides {
    /// Every side of every cell.
    All,
    /// Round the outside of the range.
    Outline,
    /// The sides between the cells, not round the outside.
    Inside,
    Top,
    Bottom,
    Left,
    Right,
    /// Thick round the outside.
    ThickOutline,
    /// None at all.
    None,
}

impl BorderSides {
    pub fn parse(word: &str) -> Option<BorderSides> {
        Some(match word {
            "all" => BorderSides::All,
            "outline" | "outside" | "box" => BorderSides::Outline,
            "inside" => BorderSides::Inside,
            "top" => BorderSides::Top,
            "bottom" => BorderSides::Bottom,
            "left" => BorderSides::Left,
            "right" => BorderSides::Right,
            "thick" | "thick-outline" => BorderSides::ThickOutline,
            "none" | "off" => BorderSides::None,
            _ => return None,
        })
    }
}

/// One change, or one move of the selection.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)] // an Act is made once per key press, so the size of one is of no cost
pub enum Act {
    /// Choose these ranges; the last is the current one and its first cell is the active cell.
    Select(Vec<Range>),
    ShowSheet(usize),
    /// Type into one cell of the sheet showing, as a person does.
    Type {
        at: CellRef,
        text: String,
    },
    /// Type the same into every chosen cell, which is Ctrl+Enter.
    TypeEverywhere(String),
    /// Several cells at once from the top left of the selection: rows of text, as a paste of text does.
    TypeRows {
        at: CellRef,
        rows: Vec<Vec<String>>,
    },
    /// A legacy array formula over the selection, which is Ctrl+Shift+Enter.
    ArrayFormula(String),
    Clear(Clear),
    Style(StyleChange),
    Border {
        sides: BorderSides,
        style: String,
        color: Color,
    },
    Insert(Shift),
    Delete(Shift),
    Fill(Direction),
    /// Pull the selection's pattern on to a larger range, which the fill handle does.
    FillTo(Range),
    Copy {
        cut: bool,
    },
    Paste(PasteMode),
    /// Paste text that came from another program, split into cells by tabs and lines.
    PasteText(String),
    /// A table another program put on the clipboard as HTML.
    PasteHtml(String),
    Sort {
        column: Option<u32>,
        ascending: bool,
    },
    /// Turn the filter buttons on over the selection's region, or off.
    Filter,
    FilterColumn {
        col: u32,
        criteria: Option<FilterCriteria>,
    },
    RemoveDuplicates,
    TextToColumns(String),
    Merge {
        center: bool,
    },
    Unmerge,
    /// Column width in characters, or the default.
    Width(Option<f64>),
    /// Row height in points, or the default.
    Height(Option<f64>),
    /// Fit the chosen columns to what is in them.
    AutoFitColumns,
    /// Fit the chosen rows to the tallest text in them.
    AutoFitRows,
    Hide {
        rows: bool,
    },
    Unhide {
        rows: bool,
    },
    /// Freeze the rows above and the columns left of the active cell.
    Freeze,
    FreezeAt {
        rows: u32,
        cols: u32,
    },
    Unfreeze,
    Gridlines(bool),
    Undo,
    Redo,
    AutoSum,
    Note(Option<String>),
    Link {
        target: Option<String>,
        text: Option<String>,
    },
    Name {
        name: String,
        refers_to: Option<String>,
    },
    Validation(Validation),
    ClearValidation,
    Conditional(CfRule),
    ClearConditional,
    Table(Option<String>),
    AddSheet,
    RenameSheet(usize, String),
    DeleteSheet(usize),
    MoveSheet(usize, usize),
    DuplicateSheet(usize),
    HideSheet(usize),
    UnhideSheet(usize),
    TabColor(usize, Option<Color>),
    Replace {
        find: String,
        with: String,
        case: bool,
        whole_book: bool,
    },
    Recalculate,
    Zoom(f32),
    Chart {
        kind: ChartKind,
        title: Option<String>,
    },
    /// Do the last change again on the selection, as F4 does, or redo when there is something to redo, as
    /// Ctrl+Y does.
    Repeat {
        redo_first: bool,
    },
}

impl Act {
    /// Whether F4 repeats this change: one that changes the workbook and makes sense again on other cells.
    pub fn repeats(&self) -> bool {
        !matches!(
            self,
            Act::Select(_)
                | Act::ShowSheet(_)
                | Act::Copy { .. }
                | Act::Undo
                | Act::Redo
                | Act::Repeat { .. }
                | Act::Zoom(_)
                | Act::Recalculate
                | Act::RenameSheet(..)
                | Act::Name { .. }
        )
    }
}

/// What a change answered: a line for the status bar.
pub type Said = Result<String, String>;

/// The block of cells around `at` that have something in them, which sort, filter and table work on when
/// one cell is chosen: Excel's current region.
pub fn current_region(sheet: &Sheet, at: CellRef) -> Range {
    let filled = |r: u32, c: u32| !sheet.value_at(CellRef::new(r, c)).is_empty();
    let mut region = Range::cell(at);
    loop {
        let mut grown = region;
        let (top, bottom, left, right) =
            (region.first.row, region.last.row, region.first.col, region.last.col);
        let row_has =
            |r: u32| (left.saturating_sub(1)..=(right + 1).min(MAX_COLS - 1)).any(|c| filled(r, c));
        let col_has =
            |c: u32| (top.saturating_sub(1)..=(bottom + 1).min(MAX_ROWS - 1)).any(|r| filled(r, c));
        if top > 0 && row_has(top - 1) {
            grown.first.row -= 1;
        }
        if bottom + 1 < MAX_ROWS && row_has(bottom + 1) {
            grown.last.row += 1;
        }
        if left > 0 && col_has(left - 1) {
            grown.first.col -= 1;
        }
        if right + 1 < MAX_COLS && col_has(right + 1) {
            grown.last.col += 1;
        }
        if grown == region || grown.len() > 4_000_000 {
            return region;
        }
        region = grown;
    }
}

/// The range a command that wants a block of data acts on: the selection, or its current region when it is
/// one cell.
pub fn block(book: &Book) -> Range {
    let range = book.view.range();
    if range.len() == 1 {
        current_region(&book.session.book.sheets[book.view.sheet], range.first)
    } else {
        clip_to_used(&book.session.book.sheets[book.view.sheet], range)
    }
}

/// The ranges at the edges of a range, for an outline border.
fn edges(range: Range) -> [(Range, Sides); 4] {
    let r = range;
    [
        (Range::new(r.first, CellRef::new(r.first.row, r.last.col)), Sides::Top),
        (Range::new(CellRef::new(r.last.row, r.first.col), r.last), Sides::Bottom),
        (Range::new(r.first, CellRef::new(r.last.row, r.first.col)), Sides::Left),
        (Range::new(CellRef::new(r.first.row, r.last.col), r.last), Sides::Right),
    ]
}

/// The author a note is written under: the person signed in to this machine.
pub fn author() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Unluminous".to_owned())
}

/// Make one change. `copied` is the cells this window copied last, which Copy sets and Paste reads.
pub fn apply(book: &mut Book, copied: &mut Option<Copied>, act: Act) -> Said {
    let act = match act {
        Act::Repeat { redo_first } if redo_first && book.session.can_redo() => Act::Redo,
        Act::Repeat { .. } => match book.last.clone() {
            Some(last) => again(book, last),
            None => return Err("There is nothing to repeat.".to_owned()),
        },
        other => other,
    };
    let remembered = act.repeats().then(|| act.clone());
    let said = apply_inner(book, copied, act);
    if said.is_ok() {
        if let Some(act) = remembered {
            book.last = Some(act);
        }
    }
    book.changed();
    said
}

/// The last change made again where the selection is now: typing goes into the active cell.
fn again(book: &Book, last: Act) -> Act {
    match last {
        Act::Type { text, .. } => Act::Type { at: book.view.cursor, text },
        other => other,
    }
}

fn apply_inner(book: &mut Book, copied: &mut Option<Copied>, act: Act) -> Said {
    let sheet = book.view.sheet;
    let ranges = book.view.ranges();
    let range = book.view.range();
    let cursor = book.view.cursor;
    let s = &mut book.session;
    match act {
        Act::Select(chosen) => {
            let Some(last) = chosen.last().copied() else { return Ok(String::new()) };
            book.view.held = None;
            book.view.extra = chosen[..chosen.len() - 1].to_vec();
            book.view.cursor = last.first;
            book.view.anchor = last.last;
            book.view.reveal = true;
            Ok(String::new())
        }
        Act::ShowSheet(index) => {
            if index >= s.book.sheets.len() {
                return Err(format!("there is no sheet {}", index + 1));
            }
            if s.book.sheets[index].state != Visibility::Visible {
                s.set_sheet_visibility(index, Visibility::Visible)?;
            }
            // Each sheet keeps its own selection and scroll, as Excel's do.
            super::keep_view(&mut book.session, &book.view);
            book.view.sheet = index;
            super::restore_view(&book.session.book, &mut book.view);
            book.view.edit = None;
            Ok(String::new())
        }
        Act::Type { at, text } => {
            let value = typed_value(s, sheet, at, &text);
            s.check_validation(sheet, at, &value)?;
            s.set_input(sheet, at, &text)?;
            Ok(String::new())
        }
        Act::TypeEverywhere(text) => {
            let mut inputs = Vec::new();
            for r in &ranges {
                let r =
                    if r.len() > 100_000 { clip_to_used(&s.book.sheets[sheet], *r) } else { *r };
                inputs.extend(r.cells().map(|at| (at, text.clone())));
            }
            // A formula typed into many cells moves with each, as Ctrl+Enter does.
            if let Some(formula) = text.strip_prefix('=') {
                let parsed = unluminous_sheet::formula::Formula::parse(formula)
                    .map_err(|e| format!("That formula has a problem: {e}"))?;
                inputs = inputs
                    .into_iter()
                    .map(|(at, _)| {
                        let moved = unluminous_sheet::formula::shifted(
                            &parsed.expr,
                            at.row as i64 - cursor.row as i64,
                            at.col as i64 - cursor.col as i64,
                        );
                        (at, format!("={}", unluminous_sheet::formula::print::print(&moved, false)))
                    })
                    .collect();
            }
            s.set_inputs(sheet, &inputs)?;
            Ok(format!("Filled {} cells", inputs.len()))
        }
        Act::TypeRows { at, rows } => {
            let mut inputs = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                for (j, text) in row.iter().enumerate() {
                    if let Some(to) = at.offset(i as i64, j as i64) {
                        inputs.push((to, text.clone()));
                    }
                }
            }
            s.set_inputs(sheet, &inputs)?;
            let height = rows.len().max(1) as i64;
            let width = rows.iter().map(Vec::len).max().unwrap_or(1).max(1) as i64;
            if let Some(last) = at.offset(height - 1, width - 1) {
                book.view.select_range(Range::new(at, last));
            }
            Ok(String::new())
        }
        Act::ArrayFormula(text) => {
            s.set_array_formula(sheet, range, &text)?;
            Ok(String::new())
        }
        Act::Clear(what) => {
            s.clear(sheet, &ranges, what)?;
            Ok(String::new())
        }
        Act::Style(change) => {
            // A larger font grows the rows it is in, as Excel does for a row nobody set a height on.
            let taller = matches!(
                &change,
                StyleChange::Font(unluminous_sheet::styles::FontEdit { size: Some(_), .. })
                    | StyleChange::CellStyle(_)
            );
            match taller {
                true => s.set_style_fitting_rows(sheet, &ranges, &change, &fitted_height)?,
                false => s.set_style(sheet, &ranges, &change)?,
            }
            Ok(String::new())
        }
        Act::Border { sides, style, color } => {
            let edge = Edge::line(&style, color);
            let none = || BorderEdit { sides: Sides::None, edge: Edge::default() };
            for r in &ranges {
                let r = clip_whole(&s.book.sheets[sheet], *r);
                match sides {
                    BorderSides::All => s.set_style(
                        sheet,
                        &[r],
                        &StyleChange::Border(BorderEdit { sides: Sides::All, edge: edge.clone() }),
                    )?,
                    BorderSides::None => s.set_style(sheet, &[r], &StyleChange::Border(none()))?,
                    BorderSides::Outline | BorderSides::ThickOutline => {
                        let edge = if sides == BorderSides::ThickOutline {
                            Edge::line("thick", color)
                        } else {
                            edge.clone()
                        };
                        for (part, side) in edges(r) {
                            s.set_style(
                                sheet,
                                &[part],
                                &StyleChange::Border(BorderEdit {
                                    sides: side,
                                    edge: edge.clone(),
                                }),
                            )?;
                        }
                    }
                    BorderSides::Inside => {
                        for row in r.first.row..r.last.row {
                            let part = Range::new(
                                CellRef::new(row, r.first.col),
                                CellRef::new(row, r.last.col),
                            );
                            s.set_style(
                                sheet,
                                &[part],
                                &StyleChange::Border(BorderEdit {
                                    sides: Sides::Bottom,
                                    edge: edge.clone(),
                                }),
                            )?;
                        }
                        for col in r.first.col..r.last.col {
                            let part = Range::new(
                                CellRef::new(r.first.row, col),
                                CellRef::new(r.last.row, col),
                            );
                            s.set_style(
                                sheet,
                                &[part],
                                &StyleChange::Border(BorderEdit {
                                    sides: Sides::Right,
                                    edge: edge.clone(),
                                }),
                            )?;
                        }
                    }
                    BorderSides::Top
                    | BorderSides::Bottom
                    | BorderSides::Left
                    | BorderSides::Right => {
                        let index = match sides {
                            BorderSides::Top => 0,
                            BorderSides::Bottom => 1,
                            BorderSides::Left => 2,
                            _ => 3,
                        };
                        let (part, side) = edges(r)[index];
                        s.set_style(
                            sheet,
                            &[part],
                            &StyleChange::Border(BorderEdit { sides: side, edge: edge.clone() }),
                        )?;
                    }
                }
            }
            Ok(String::new())
        }
        Act::Insert(shift) => {
            s.insert(sheet, range, shift)?;
            Ok(String::new())
        }
        Act::Delete(shift) => {
            s.delete(sheet, range, shift)?;
            book.view.anchor = book.view.cursor;
            Ok(String::new())
        }
        Act::Fill(direction) => {
            for r in &ranges {
                s.fill(sheet, clip_whole(&s.book.sheets[sheet], *r), direction)?;
            }
            Ok(String::new())
        }
        Act::FillTo(target) => {
            s.fill_series(sheet, range, target)?;
            book.view.select_range(target);
            book.view.lane_b.after = Some(super::view_b::After {
                kind: super::view_b::AfterKind::Fill,
                sheet,
                target,
                dest: target,
                source: range,
                clip: None,
                revision: book.revision + 1,
                open: false,
            });
            Ok(String::new())
        }
        Act::Copy { cut } => {
            if ranges.len() > 1 {
                return Err("That command cannot be used on several ranges at once.".to_owned());
            }
            let clipped = clip_to_used(&s.book.sheets[sheet], range);
            let source =
                if range.is_whole_columns() || range.is_whole_rows() { clipped } else { range };
            let clip = s.copy(sheet, source, cut);
            let text = clip.to_text(&s.book);
            *copied = Some(Copied { key: book.key.clone(), clip, text: text.clone() });
            book.view.marquee = Some((sheet, source, cut));
            Ok(text)
        }
        Act::Paste(mode) => {
            let Some(from) = copied.clone() else {
                return Err("Nothing has been copied.".to_owned());
            };
            if from.key != book.key && from.clip.cut {
                return Err(
                    "Cells cut in one workbook are pasted by copying them instead.".to_owned()
                );
            }
            let pasted = s.paste(&from.clip, sheet, range, mode)?;
            if from.clip.cut {
                *copied = None;
                book.view.marquee = None;
            }
            book.view.select_range(pasted);
            // `apply` counts this change once more, so the button is valid for the next revision.
            book.view.lane_b.after = Some(super::view_b::After {
                kind: super::view_b::AfterKind::Paste,
                sheet,
                target: pasted,
                dest: range,
                source: from.clip.range,
                clip: Some(from.clip.clone()),
                revision: book.revision + 1,
                open: false,
            });
            Ok(String::new())
        }
        Act::PasteText(text) => {
            // Text from another program: tab separated cells, entered the way Excel enters pasted text, so a
            // cell that begins with `=` is text unless the person asks for formulas.
            if text.is_empty() {
                return Ok(String::new());
            }
            let options = unluminous_sheet::edit::clipboard::ExternalPaste::default();
            let pasted = s.paste_text(sheet, cursor, &text, options)?;
            book.view.select_range(pasted);
            Ok(String::new())
        }
        Act::PasteHtml(html) => {
            let options = unluminous_sheet::edit::clipboard::ExternalPaste::default();
            let pasted = s.paste_html(sheet, cursor, &html, options)?;
            book.view.select_range(pasted);
            Ok(String::new())
        }
        Act::Sort { column, ascending } => {
            let region = block(book);
            let s = &mut book.session;
            let header = has_header(&s.book.sheets[sheet], region);
            let col = column.unwrap_or(cursor.col).clamp(region.first.col, region.last.col)
                - region.first.col;
            s.sort(sheet, region, &[(col, ascending)], header)?;
            Ok(format!("Sorted {} {}", region.a1(), if ascending { "A to Z" } else { "Z to A" }))
        }
        Act::Filter => {
            if s.book.sheets[sheet].filter.is_some() {
                s.set_filter(sheet, None)?;
                return Ok("Filter off".to_owned());
            }
            let region = block(book);
            book.session.set_filter(sheet, Some(region))?;
            Ok(format!("Filter on {}", region.a1()))
        }
        Act::FilterColumn { col, criteria } => {
            s.filter_column(sheet, col, criteria)?;
            Ok(String::new())
        }
        Act::RemoveDuplicates => {
            let region = block(book);
            let s = &mut book.session;
            let header = has_header(&s.book.sheets[sheet], region);
            let columns: Vec<u32> = (0..region.cols()).collect();
            let removed = s.remove_duplicates(sheet, region, &columns, header)?;
            Ok(format!("Removed {removed} duplicate rows"))
        }
        Act::TextToColumns(delimiter) => {
            s.text_to_columns(sheet, range, &delimiter)?;
            Ok(String::new())
        }
        Act::Merge { center } => {
            s.merge(sheet, range, center)?;
            Ok(String::new())
        }
        Act::Unmerge => {
            let merges: Vec<Range> = s.book.sheets[sheet]
                .merges
                .iter()
                .filter(|m| m.intersects(&range))
                .copied()
                .collect();
            for m in merges {
                s.unmerge(sheet, m)?;
            }
            Ok(String::new())
        }
        Act::Width(width) => {
            for r in &ranges {
                s.set_column_width(sheet, r.first.col, r.last.col, width)?;
            }
            Ok(String::new())
        }
        Act::Height(height) => {
            for r in &ranges {
                s.set_row_height(sheet, r.first.row, r.last.row, height)?;
            }
            Ok(String::new())
        }
        Act::AutoFitColumns => {
            for r in &ranges {
                // Whole columns fit everything in them; a block of cells fits only its own rows, as
                // Excel's AutoFit Column Width does, so a long title above a table leaves it alone.
                let rows = (r.first.row > 0 || r.last.row < MAX_ROWS - 1)
                    .then_some((r.first.row, r.last.row));
                for col in r.first.col..=r.last.col.min(r.first.col + 512) {
                    let width = fitted_width(&s.book, sheet, col, rows);
                    s.set_column_width(sheet, col, col, width)?;
                }
            }
            Ok(String::new())
        }
        Act::AutoFitRows => {
            for r in &ranges {
                let rows = clip_whole(&s.book.sheets[sheet], *r);
                for row in rows.first.row..=rows.last.row.min(rows.first.row + 4096) {
                    let height = fitted_height(&s.book, sheet, row);
                    s.set_row_height(sheet, row, row, height)?;
                }
            }
            Ok(String::new())
        }
        Act::Hide { rows } => {
            for r in &ranges {
                if rows {
                    s.hide_rows(sheet, r.first.row, r.last.row, true)?;
                } else {
                    s.hide_columns(sheet, r.first.col, r.last.col, true)?;
                }
            }
            Ok(String::new())
        }
        Act::Unhide { rows } => {
            for r in &ranges {
                // Unhiding a range that is one row or column unhides the hidden ones beside it, which is how a
                // person reaches a hidden row they cannot click.
                let (first, last) = if rows {
                    (r.first.row.saturating_sub(1), r.last.row + 1)
                } else {
                    (r.first.col.saturating_sub(1), r.last.col + 1)
                };
                if rows {
                    s.hide_rows(sheet, first, last.min(MAX_ROWS - 1), false)?;
                } else {
                    s.hide_columns(sheet, first, last.min(MAX_COLS - 1), false)?;
                }
            }
            Ok(String::new())
        }
        Act::Freeze => {
            let at = book.view.cursor;
            s.freeze(sheet, at.row, at.col)?;
            Ok(format!("Froze the rows above and the columns left of {}", at.a1()))
        }
        Act::FreezeAt { rows, cols } => {
            s.freeze(sheet, rows, cols)?;
            Ok(String::new())
        }
        Act::Unfreeze => {
            s.freeze(sheet, 0, 0)?;
            Ok("Panes unfrozen".to_owned())
        }
        Act::Gridlines(shown) => {
            s.set_gridlines(sheet, shown)?;
            Ok(String::new())
        }
        Act::Undo => match s.undo() {
            Some(label) => Ok(format!("Undid {label}")),
            None => Err("There is nothing to undo.".to_owned()),
        },
        Act::Redo => match s.redo() {
            Some(label) => Ok(format!("Redid {label}")),
            None => Err("There is nothing to redo.".to_owned()),
        },
        Act::AutoSum => {
            let formula = s.autosum(sheet, cursor);
            s.set_input(sheet, cursor, &formula)?;
            Ok(formula)
        }
        Act::Note(text) => {
            s.set_note(sheet, cursor, text.as_deref(), &author())?;
            Ok(String::new())
        }
        Act::Link { target, text } => {
            let target = target.map(|t| {
                if t.contains("://")
                    || t.starts_with("mailto:")
                    || t.contains('\\')
                    || (t.contains('.') && !t.contains('!'))
                {
                    LinkTarget::External(t)
                } else {
                    LinkTarget::Location(t)
                }
            });
            if let Some(text) = &text {
                if target.is_some() && s.book.sheets[sheet].value_at(cursor).is_empty() {
                    s.set_input(sheet, cursor, text)?;
                }
            }
            s.set_link(sheet, Range::cell(cursor), target, text)?;
            Ok(String::new())
        }
        Act::Name { name, refers_to } => match refers_to {
            Some(to) => {
                s.set_name(&name, &to, None)?;
                Ok(format!("{name} is {to}"))
            }
            None => {
                s.delete_name(&name, None)?;
                Ok(format!("{name} deleted"))
            }
        },
        Act::Validation(mut validation) => {
            if validation.ranges.is_empty() {
                validation.ranges = ranges.clone();
            }
            s.add_validation(sheet, validation)?;
            Ok(String::new())
        }
        Act::ClearValidation => {
            for r in &ranges {
                s.clear_validation(sheet, *r)?;
            }
            Ok(String::new())
        }
        Act::Conditional(rule) => {
            let covered: Vec<Range> =
                ranges.iter().map(|r| clip_whole(&s.book.sheets[sheet], *r)).collect();
            s.add_conditional(sheet, covered, rule)?;
            Ok(String::new())
        }
        Act::ClearConditional => {
            for r in &ranges {
                s.clear_conditional(sheet, *r)?;
            }
            Ok(String::new())
        }
        Act::Table(name) => {
            let region = block(book);
            let made = book.session.add_table(sheet, region, name.as_deref())?;
            Ok(format!("Made table {made} over {}", region.a1()))
        }
        Act::AddSheet => {
            let at = s.add_sheet(None, Some(sheet + 1))?;
            book.view.sheet = at;
            book.view.select_cell(CellRef::new(0, 0));
            book.view.scroll = egui::Vec2::ZERO;
            Ok(format!("Added {}", book.session.book.sheets[at].name))
        }
        Act::RenameSheet(index, name) => {
            s.rename_sheet(index, &name)?;
            Ok(String::new())
        }
        Act::DeleteSheet(index) => {
            s.delete_sheet(index)?;
            book.view.sheet = book.view.sheet.min(book.session.book.sheets.len() - 1);
            if book.session.book.sheets[book.view.sheet].state != Visibility::Visible {
                book.view.sheet = book
                    .session
                    .book
                    .sheets
                    .iter()
                    .position(|s| s.state == Visibility::Visible)
                    .unwrap_or(0);
            }
            book.view.select_cell(CellRef::new(0, 0));
            Ok(String::new())
        }
        Act::MoveSheet(from, to) => {
            s.move_sheet(from, to)?;
            if book.view.sheet == from {
                book.view.sheet = to;
            }
            Ok(String::new())
        }
        Act::DuplicateSheet(index) => {
            let at = s.duplicate_sheet(index)?;
            book.view.sheet = at;
            Ok(format!("Made {}", book.session.book.sheets[at].name))
        }
        Act::HideSheet(index) => {
            s.set_sheet_visibility(index, Visibility::Hidden)?;
            if book.view.sheet == index {
                book.view.sheet = book
                    .session
                    .book
                    .sheets
                    .iter()
                    .position(|s| s.state == Visibility::Visible)
                    .unwrap_or(0);
            }
            Ok(String::new())
        }
        Act::UnhideSheet(index) => {
            s.set_sheet_visibility(index, Visibility::Visible)?;
            book.view.sheet = index;
            Ok(String::new())
        }
        Act::TabColor(index, color) => {
            s.set_tab_color(index, color)?;
            Ok(String::new())
        }
        Act::Replace { find, with, case, whole_book } => {
            let count =
                s.replace(if whole_book { None } else { Some(sheet) }, &find, &with, case)?;
            Ok(format!("Replaced {count}"))
        }
        Act::Recalculate => {
            s.now = super::now_serial();
            s.recalculate_all();
            Ok("Recalculated".to_owned())
        }
        Act::Zoom(zoom) => {
            book.view.zoom = zoom.clamp(0.25, 4.0);
            Ok(format!("{}%", (book.view.zoom * 100.0).round()))
        }
        Act::Chart { kind, title } => {
            let region = block(book);
            let chart = chart_over(&book.session.book.sheets[sheet], region, kind, title)?;
            book.session.add_chart(sheet, chart)?;
            Ok(format!("Made a {} chart of {}", kind.name(), region.a1()))
        }
        Act::Repeat { .. } => Ok(String::new()),
    }
}

/// A whole row or column chosen is limited to the cells in use, for a change that walks cells.
fn clip_whole(sheet: &Sheet, range: Range) -> Range {
    if range.is_whole_columns() || range.is_whole_rows() || range.len() > 1_000_000 {
        clip_to_used(sheet, range)
    } else {
        range
    }
}

/// Whether the first row of a block reads as a header: text over at least one column holding numbers or
/// dates, which is the guess Excel makes.
pub fn has_header(sheet: &Sheet, range: Range) -> bool {
    if range.rows() < 2 {
        return false;
    }
    (range.first.col..=range.last.col).any(|c| {
        let top = sheet.value_at(CellRef::new(range.first.row, c));
        let next = sheet.value_at(CellRef::new(range.first.row + 1, c));
        matches!(top, Value::Text(_)) && !matches!(next, Value::Text(_)) && !next.is_empty()
    }) || (range.first.col..=range.last.col)
        .all(|c| matches!(sheet.value_at(CellRef::new(range.first.row, c)), Value::Text(_)))
        && (range.first.col..=range.last.col).any(|c| {
            !matches!(sheet.value_at(CellRef::new(range.first.row + 1, c)), Value::Text(_))
        })
}

/// The value typing `text` into a cell would give, for a validation rule to judge before it is written.
fn typed_value(
    s: &unluminous_sheet::edit::Session,
    sheet: usize,
    at: CellRef,
    text: &str,
) -> Value {
    if text.starts_with('=') {
        return s.evaluate_at(sheet, at, text).unwrap_or(Value::Empty);
    }
    unluminous_sheet::input::read(text, s.book.date1904, s.this_year).value
}

/// The width that fits a column's widest shown value, in the file's characters: seven points a character,
/// which is the default font's digit, plus the cell's padding. With `rows`, only the cells in those rows
/// are measured.
pub fn fitted_width(
    book: &unluminous_sheet::Workbook,
    sheet: usize,
    col: u32,
    rows: Option<(u32, u32)>,
) -> Option<f64> {
    let s = &book.sheets[sheet];
    let mut widest: f64 = 0.0;
    for (&(row, c), cell) in &s.cells {
        if c != col || cell.value.is_empty() || rows.is_some_and(|(a, b)| row < a || row > b) {
            continue;
        }
        let code = book.styles.format_code(cell.style);
        let text = unluminous_sheet::numfmt::Format::parse(&code)
            .render(&cell.value, book.date1904, 255)
            .text;
        let font = book.styles.font_of(cell.style);
        // Measured in the face the grid drew it in, plus the eight pixels Excel's AutoFit leaves, when the
        // grid has drawn that face; counted at seven pixels a character otherwise.
        let measured = text
            .lines()
            .map(|line| super::measure::width(&font.name, font.bold, font.italic, font.size, line))
            .collect::<Option<Vec<f64>>>()
            .map(|widths| widths.into_iter().fold(0.0, f64::max) + 8.0);
        let counted = || {
            let scale = font.size / 11.0 * if font.bold { 1.1 } else { 1.0 };
            let longest = text.lines().map(|line| line.chars().count()).max().unwrap_or(0);
            (longest as f64 * scale).ceil() * 7.0 + 12.0
        };
        if !text.is_empty() {
            widest = widest.max(measured.unwrap_or_else(counted));
        }
    }
    if widest <= 0.0 {
        return None;
    }
    Some(super::axis::points_to_width(widest.ceil() as f32))
}

/// The height that fits a row's tallest text, in points: the font's line, times the lines a wrapped cell
/// breaks into, as Excel's AutoFit works it out. `None` is the default height, for a row with nothing
/// taller than the default font in it.
pub fn fitted_height(book: &unluminous_sheet::Workbook, sheet: usize, row: u32) -> Option<f64> {
    let s = &book.sheets[sheet];
    let mut tallest: f64 = 0.0;
    for (&(_, col), cell) in s.cells.range((row, 0)..=(row, u32::MAX)) {
        if cell.value.is_empty() {
            continue;
        }
        let font = book.styles.font_of(cell.style);
        let align = book.styles.xf(cell.style).align;
        let code = book.styles.format_code(cell.style);
        let text = unluminous_sheet::numfmt::Format::parse(&code)
            .render(&cell.value, book.date1904, 255)
            .text;
        let mut lines = text.lines().count().max(1);
        if align.wrap {
            let width_points = super::axis::Axis::columns(s).size(col) as f64;
            let chars_per_line = ((width_points - 5.0) / (font.size * 0.55)).max(1.0);
            lines = text
                .lines()
                .map(|line| (line.chars().count() as f64 / chars_per_line).ceil().max(1.0) as usize)
                .sum::<usize>()
                .max(1);
        }
        // A line is about one and a third of the font's size, as Excel's default rows are 15 points for 11.
        let line = (font.size * 4.0 / 3.0 * 4.0).round() / 4.0 + 0.5;
        tallest = tallest.max(line * lines as f64);
    }
    (tallest > s.default_row_height + 0.01).then_some(tallest)
}

/// A chart over a block: the first column as the categories and every other column a series, the first row
/// naming them when it is a header.
pub fn chart_over(
    sheet: &Sheet,
    region: Range,
    kind: ChartKind,
    title: Option<String>,
) -> Result<Chart, String> {
    if region.cols() < 2 && region.rows() < 2 {
        return Err("A chart needs a block of at least two cells.".to_owned());
    }
    let quoted = unluminous_sheet::formula::print::sheet_name(&sheet.name);
    let absolute = |r: Range| {
        let a = |at: CellRef| {
            format!("${}${}", unluminous_sheet::coord::column_name(at.col), at.row + 1)
        };
        if r.len() == 1 {
            format!("{quoted}!{}", a(r.first))
        } else {
            format!("{quoted}!{}:{}", a(r.first), a(r.last))
        }
    };
    let header = has_header(sheet, region);
    let first_row = region.first.row + u32::from(header);
    let text_first_column = region.cols() > 1
        && (first_row..=region.last.row)
            .any(|r| matches!(sheet.value_at(CellRef::new(r, region.first.col)), Value::Text(_)));
    let categories = (text_first_column
        || region.cols() > 1 && kind != ChartKind::Scatter
        || kind == ChartKind::Scatter && region.cols() > 1)
        .then(|| {
            Range::new(
                CellRef::new(first_row, region.first.col),
                CellRef::new(region.last.row, region.first.col),
            )
        });
    let value_columns: Vec<u32> = if categories.is_some() {
        (region.first.col + 1..=region.last.col).collect()
    } else {
        (region.first.col..=region.last.col).collect()
    };
    let mut series = Vec::new();
    for col in value_columns {
        let values = Range::new(CellRef::new(first_row, col), CellRef::new(region.last.row, col));
        series.push(ChartSeries {
            name: header.then(|| absolute(Range::cell(CellRef::new(region.first.row, col)))),
            categories: categories.map(absolute),
            values: absolute(values),
            ..Default::default()
        });
    }
    if series.is_empty() {
        return Err("A chart needs a column of numbers.".to_owned());
    }
    // Placed to the right of the block, eight columns wide and fifteen rows tall, as Excel places one.
    let from = CellRef::new(region.first.row, region.last.col + 2);
    let to = CellRef::new(region.first.row + 15, region.last.col + 10);
    Ok(Chart {
        from: (from, 0, 0),
        to: (to, 0, 0),
        kind,
        title,
        series,
        part: String::new(),
        new: true,
        spec: Default::default(),
    })
}
