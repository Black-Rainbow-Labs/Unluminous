//! The keyboard in a spreadsheet tab: moving and choosing cells, Excel's chords, and the keys that finish
//! or steer typing in a cell. The keys this reads are taken out of the frame, so nothing behind the tab
//! acts on them as well.

use egui::{Event, Key, Modifiers};
use unluminous_sheet::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use unluminous_sheet::edit::{Clear, Direction, PasteMode, Shift};
use unluminous_sheet::styles::{FontEdit, StyleChange, Underline};

use crate::services::spreadsheet::actions::{current_region, Act};
use crate::services::spreadsheet::display;
use crate::services::spreadsheet::formula_text;
use crate::services::spreadsheet::view::{edge_of_data, Edit, Find, PanelPath, Prompt, SelectMode};
use crate::services::spreadsheet::Book;

/// What a key asked for beyond moving the selection.
#[derive(Debug, Default)]
pub struct Keys {
    pub acts: Vec<Act>,
    /// The words of a command, run after the frame as a menu entry's are.
    pub words: Vec<Vec<String>>,
    /// Put the name box in charge of the keyboard.
    pub name_box: bool,
}

/// The words of a command written as one string.
fn say(text: &str) -> Vec<String> {
    text.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// The rows the grid shows, which Page Up and Page Down move by, as Excel's do.
fn page_rows(book: &Book) -> i64 {
    let Some(drawn) = book.view.drawn else { return 20 };
    let rows = (drawn.height() / (20.0 * book.view.zoom.max(0.1))).floor() as i64;
    rows.max(1)
}

/// The columns the grid shows, which Alt+Page Up and Alt+Page Down move by.
fn page_columns(book: &Book) -> i64 {
    let Some(drawn) = book.view.drawn else { return 8 };
    let cols = (drawn.width() / (64.0 * book.view.zoom.max(0.1))).floor() as i64;
    cols.max(1)
}

/// Read the keys meant for the grid while no cell is being typed into.
pub fn grid(ctx: &egui::Context, book: &mut Book, copied: Option<&str>, keys: &mut Keys) {
    let events = ctx.input_mut(|input| {
        let mut taken = Vec::new();
        input.events.retain(|event| {
            let ours = match event {
                Event::Text(text) => !text.chars().all(char::is_control),
                Event::Copy | Event::Cut | Event::Paste(_) => true,
                Event::Key { key, pressed: true, modifiers, .. } => grid_key(*key, *modifiers),
                _ => false,
            };
            if ours {
                taken.push(event.clone());
            }
            !ours
        });
        taken
    });
    for event in events {
        match event {
            Event::Text(text) => {
                // Typing over a cell starts with what was typed. A date or a number typed this way replaces the
                // cell; a formula typed this way starts in the mode where the arrows point at cells.
                start_typing(book, text);
            }
            Event::Copy => keys.acts.push(Act::Copy { cut: false }),
            Event::Cut => keys.acts.push(Act::Copy { cut: true }),
            Event::Paste(text) => {
                let ours = copied.is_some_and(|c| same_text(c, &text));
                keys.acts.push(if ours {
                    Act::Paste(PasteMode::All)
                } else {
                    Act::PasteText(text)
                });
            }
            Event::Key { key, modifiers, .. } => press(book, key, modifiers, keys),
            _ => {}
        }
    }
}

/// Clipboard text compared without caring how its lines end.
fn same_text(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n").trim_end() == b.replace("\r\n", "\n").trim_end()
}

/// Whether the grid reads this key while no cell is being typed into.
fn grid_key(key: Key, m: Modifiers) -> bool {
    use Key::*;
    let command = m.command || m.ctrl;
    match key {
        ArrowUp | ArrowDown | ArrowLeft | ArrowRight | Enter | Tab | Home | End | PageUp
        | PageDown | Delete | Backspace | Escape | F2 | F3 | F4 | F5 | F7 | F8 | F9 | F10 | F11
        | F12 => true,
        Space => command || m.shift,
        _ if command => {
            crate::services::spreadsheet::view::claims_key(key, m)
                || matches!(key, Exclamationmark | Num7 | Num8)
        }
        Equals | Plus | Q | F1 if m.alt => true,
        _ => false,
    }
}

/// Begin typing into the active cell with `text`.
pub fn start_typing(book: &mut Book, text: String) {
    let at = book.view.cursor;
    book.view.edit = Some(Edit {
        sheet: book.view.sheet,
        at,
        text,
        typed: true,
        in_bar: false,
        pointing: None,
        focus: true,
        caret: None,
        chosen: 0,
    });
    book.view.marquee = None;
}

/// Begin typing into the active cell with what it holds, as F2 and a double click do.
pub fn start_editing(book: &mut Book, in_bar: bool) {
    let at = book.view.cursor;
    let s = &book.session.book;
    let text = display::input_text(s, &s.sheets[book.view.sheet], at);
    book.view.edit = Some(Edit {
        sheet: book.view.sheet,
        at,
        text,
        typed: false,
        in_bar,
        pointing: None,
        focus: true,
        caret: None,
        chosen: 0,
    });
}

/// The block a whole column or row choice covers when it is extended across: rows or columns.
fn whole_rows(range: Range) -> bool {
    range.first.col == 0 && range.last.col == MAX_COLS - 1
}

fn whole_columns(range: Range) -> bool {
    range.first.row == 0 && range.last.row == MAX_ROWS - 1
}

/// Move the active cell, or the end of the range when Shift is held.
fn step(book: &mut Book, rows: i64, cols: i64, extend: bool, jump: bool) {
    let axes = book.view.axes.clone();
    if extend && book.view.held.is_some() {
        book.view.held = None;
        book.view.anchor = book.view.cursor;
    }
    let from = if extend { book.view.anchor } else { book.view.cursor };
    let sheet = &book.session.book.sheets[book.view.sheet];
    let to = if jump {
        edge_of_data(sheet, from, rows, cols)
    } else {
        let mut row = from.row;
        let mut col = from.col;
        if let Some(axes) = &axes {
            for _ in 0..rows.unsigned_abs() {
                row = axes.rows.next_shown(row, rows.signum());
            }
            for _ in 0..cols.unsigned_abs() {
                col = axes.columns.next_shown(col, cols.signum());
            }
        } else {
            row = (row as i64 + rows).clamp(0, MAX_ROWS as i64 - 1) as u32;
            col = (col as i64 + cols).clamp(0, MAX_COLS as i64 - 1) as u32;
        }
        // Out of a merged cell, the step goes past the whole merge.
        let mut to = CellRef::new(row, col);
        if !extend {
            if let Some(m) = sheet.merge_at(from) {
                if rows > 0 {
                    to.row = (m.last.row + 1).min(MAX_ROWS - 1);
                }
                if cols > 0 {
                    to.col = (m.last.col + 1).min(MAX_COLS - 1);
                }
            }
            if let Some(m) = sheet.merge_at(to) {
                to = m.first;
            }
        }
        to
    };
    if extend {
        // Shift keeps the active cell and moves the far corner.
        book.view.held = None;
        book.view.anchor = to;
        book.view.reveal = true;
    } else {
        book.view.select_cell(to);
    }
}

/// Move within the selection, as Enter and Tab do when more than one cell is chosen: the range stays chosen
/// and the active cell walks through it, wrapping at its edges.
pub fn step_within(book: &mut Book, rows: i64, cols: i64) {
    let range = book.view.range();
    if range.len() <= 1 || !book.view.extra.is_empty() {
        step(book, rows, cols, false, false);
        return;
    }
    let at = book.view.cursor;
    let (mut row, mut col) = (at.row as i64, at.col as i64);
    let (top, bottom, left, right) = (
        range.first.row as i64,
        range.last.row as i64,
        range.first.col as i64,
        range.last.col as i64,
    );
    if rows != 0 {
        row += rows;
        if row > bottom {
            row = top;
            col = if col + 1 > right { left } else { col + 1 };
        } else if row < top {
            row = bottom;
            col = if col - 1 < left { right } else { col - 1 };
        }
    } else {
        col += cols;
        if col > right {
            col = left;
            row = if row + 1 > bottom { top } else { row + 1 };
        } else if col < left {
            col = right;
            row = if row - 1 < top { bottom } else { row - 1 };
        }
    }
    book.view.held = Some(range);
    book.view.cursor = CellRef::new(row as u32, col as u32);
    book.view.anchor = book.view.cursor;
    book.view.reveal = true;
}

/// One key press on the grid.
fn press(book: &mut Book, key: Key, m: Modifiers, keys: &mut Keys) {
    use Key::*;
    let command = m.command || m.ctrl;
    if moves(book, key, m) {
        return;
    }
    if command && m.alt {
        return alt_chords(book, key, m, keys);
    }
    if m.alt {
        return alt_keys(book, key, m, keys);
    }
    if function_key(book, key, m, keys) {
        return;
    }
    if command {
        return command_chords(book, key, m, keys);
    }
    match key {
        Delete => keys.acts.push(Act::Clear(Clear::Contents)),
        Backspace => start_typing(book, String::new()),
        Escape => {
            book.view.marquee = None;
            book.view.close_the_menus();
            book.view.panel = None;
            book.view.selecting = SelectMode::Normal;
            book.view.end_mode = false;
        }
        Space if m.shift => {
            let range = book.view.range();
            book.view.select_rows(range.first.row, range.last.row)
        }
        _ => {}
    }
}

/// The keys that move the active cell or grow the selection. Answers whether the key was one of them.
fn moves(book: &mut Book, key: Key, m: Modifiers) -> bool {
    use Key::*;
    let command = m.command || m.ctrl;
    // End mode: the next arrow goes to the edge of the data, as Ctrl and the arrow does.
    let jump = command || std::mem::take(&mut book.view.end_mode) && is_arrow(key);
    let extend = m.shift || book.view.selecting == SelectMode::Extend;
    if book.view.selecting == SelectMode::Add && is_arrow(key) && !m.shift {
        let range = book.view.range();
        if !book.view.extra.contains(&range) {
            book.view.extra.push(range);
        }
        book.view.held = None;
    }
    let keep_extra = book.view.selecting == SelectMode::Add;
    let extra = book.view.extra.clone();
    match key {
        ArrowUp => step(book, -1, 0, extend, jump),
        ArrowDown if m.alt => return false,
        ArrowDown => step(book, 1, 0, extend, jump),
        ArrowLeft => step(book, 0, -1, extend, jump),
        ArrowRight => step(book, 0, 1, extend, jump),
        Enter if !command && !m.alt => step_within(book, if m.shift { -1 } else { 1 }, 0),
        Tab if !command => step_within(book, 0, if m.shift { -1 } else { 1 }),
        Home if command => {
            let frozen = book.session.book.sheets[book.view.sheet].frozen.unwrap_or((0, 0));
            book.view.move_to(CellRef::new(frozen.0, frozen.1), m.shift);
        }
        Home => {
            let row = if m.shift { book.view.anchor.row } else { book.view.cursor.row };
            book.view.move_to(CellRef::new(row, 0), m.shift);
        }
        End if command => {
            let last = book.session.book.sheets[book.view.sheet]
                .used_range()
                .map_or(CellRef::new(0, 0), |r| r.last);
            book.view.move_to(last, m.shift);
        }
        End => book.view.end_mode = true,
        PageDown | PageUp if !command && !m.alt => {
            let rows = page_rows(book);
            let rows = if key == PageDown { rows } else { -rows };
            step(book, rows, 0, extend, false);
            book.view.scroll.y = (book.view.scroll.y + rows as f32 * 20.0).max(0.0);
        }
        _ => return false,
    }
    if keep_extra {
        book.view.extra = extra;
    }
    true
}

fn is_arrow(key: Key) -> bool {
    matches!(key, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight)
}

/// The function keys, alone or with Shift. Answers whether the key was one of them.
fn function_key(book: &mut Book, key: Key, m: Modifiers, keys: &mut Keys) -> bool {
    use Key::*;
    let command = m.command || m.ctrl;
    let shift = m.shift;
    match key {
        F2 if command && shift => keys.words.push(say("comment new")),
        F2 if command => keys.words.push(say("print-preview")),
        F2 if shift => keys.words.push(say("note edit")),
        F2 => start_editing(book, false),
        F3 if command && shift => keys.words.push(say("names from-selection")),
        F3 if command => keys.words.push(say("panel Names")),
        F3 if shift => keys.words.push(say("insert-function")),
        F3 => keys.words.push(say("names paste")),
        F4 => keys.acts.push(Act::Repeat { redo_first: false }),
        F5 => keys.words.push(say("goto")),
        F7 => keys.words.push(say("spelling")),
        F8 if shift => book.view.selecting = toggle_mode(book.view.selecting, SelectMode::Add),
        F8 => book.view.selecting = toggle_mode(book.view.selecting, SelectMode::Extend),
        F9 if command && shift => keys.words.push(say("calculate rebuild")),
        F9 if command => keys.words.push(say("calculate full")),
        F9 if shift => keys.words.push(say("calculate sheet")),
        F9 => keys.acts.push(Act::Recalculate),
        F10 if shift => keys.words.push(say("menu open")),
        F11 if shift => keys.acts.push(Act::AddSheet),
        F11 => keys.words.push(say("chart sheet")),
        F12 => keys.words.push(say("save-as")),
        _ => return false,
    }
    true
}

fn toggle_mode(now: SelectMode, wanted: SelectMode) -> SelectMode {
    if now == wanted {
        SelectMode::Normal
    } else {
        wanted
    }
}

/// Chords with Alt and no Ctrl.
fn alt_keys(book: &mut Book, key: Key, m: Modifiers, keys: &mut Keys) {
    use Key::*;
    match key {
        ArrowDown => keys.words.push(say("autocomplete list")),
        Equals | Plus => keys.acts.push(Act::AutoSum),
        Q => {
            book.view.book_menu = Some(String::new());
            book.view.menu = None;
        }
        F1 if m.shift => keys.acts.push(Act::AddSheet),
        F1 => keys.words.push(say("chart default")),
        F10 if m.shift => keys.words.push(say("error-menu")),
        PageDown | PageUp => {
            let cols = page_columns(book);
            step(book, 0, if key == PageDown { cols } else { -cols }, m.shift, false);
        }
        ArrowRight if m.shift => keys.words.push(say("group")),
        ArrowLeft if m.shift => keys.words.push(say("ungroup")),
        _ => {}
    }
}

/// Chords with Ctrl and Alt together.
fn alt_chords(book: &mut Book, key: Key, m: Modifiers, keys: &mut Keys) {
    use Key::*;
    match key {
        V => keys.words.push(say("paste special")),
        Plus | Equals => keys.acts.push(Act::Zoom(book.view.zoom + 0.1)),
        Minus => keys.acts.push(Act::Zoom(book.view.zoom - 0.1)),
        F9 if m.shift => keys.words.push(say("calculate rebuild")),
        F9 => keys.words.push(say("calculate full")),
        _ => {}
    }
}

/// Chords with Ctrl: Excel's own, each the same command a menu entry runs.
fn command_chords(book: &mut Book, key: Key, m: Modifiers, keys: &mut Keys) {
    use Key::*;
    let shift = m.shift;
    let range = book.view.range();
    match key {
        Space if shift => book
            .view
            .select_range(Range::new(CellRef::new(0, 0), CellRef::new(MAX_ROWS - 1, MAX_COLS - 1))),
        Space => book.view.select_columns(range.first.col, range.last.col),
        Z => keys.acts.push(Act::Undo),
        Y => keys.acts.push(Act::Repeat { redo_first: true }),
        A if shift => {}
        A => select_all(book),
        D => keys.acts.push(Act::Fill(Direction::Down)),
        R => keys.acts.push(Act::Fill(Direction::Right)),
        E => keys.words.push(say("flash-fill")),
        B => keys.acts.push(bold(book)),
        I => keys.acts.push(italic(book)),
        Num3 if !shift => keys.acts.push(italic(book)),
        U if shift => book.view.tall_bar = !book.view.tall_bar,
        U => keys.acts.push(underline(book)),
        Num4 if !shift => keys.acts.push(underline(book)),
        Num5 if !shift => keys.acts.push(toggle(
            book,
            |f| f.strike,
            |v| FontEdit { strike: Some(v), ..Default::default() },
        )),
        Num1 | Exclamationmark if shift => keys.acts.push(number_format("#,##0.00")),
        Num1 => book.view.panel = Some(PanelPath(Vec::new())),
        Num2 if shift => keys.acts.push(number_format("h:mm AM/PM")),
        Num3 if shift => keys.acts.push(number_format("d-mmm-yy")),
        Num4 if shift => keys.acts.push(number_format("\"$\"#,##0.00")),
        Num5 if shift => keys.acts.push(number_format("0%")),
        Num6 if shift => keys.acts.push(number_format("0.00E+00")),
        Num6 => keys.words.push(say("objects toggle")),
        Num7 if shift => keys.acts.push(Act::Border {
            sides: crate::services::spreadsheet::actions::BorderSides::Outline,
            style: "thin".to_owned(),
            color: unluminous_sheet::styles::Color::Auto,
        }),
        Num8 if shift => select_current_region(book),
        Num8 => keys.words.push(say("outline symbols toggle")),
        Num9 => keys.acts.push(if shift {
            Act::Unhide { rows: true }
        } else {
            Act::Hide { rows: true }
        }),
        Num0 => keys.acts.push(if shift {
            Act::Unhide { rows: false }
        } else {
            Act::Hide { rows: false }
        }),
        Backtick if shift => keys.acts.push(number_format("General")),
        Backtick => keys.words.push(say("show-formulas toggle")),
        Quote => from_above(book, shift),
        Semicolon | Colon => type_now(book, shift || key == Colon, keys),
        Minus if shift => keys.words.push(say("border outline none")),
        Minus => insert_or_delete(book, false, keys),
        Plus | Equals => insert_or_delete(book, true, keys),
        F if shift => book.view.panel = Some(PanelPath(vec!["Font".to_owned()])),
        F => {
            book.view.find =
                Some(Find { focus: true, ..book.view.find.clone().unwrap_or_default() })
        }
        H => {
            book.view.find = Some(Find {
                focus: true,
                replace: Some(String::new()),
                ..book.view.find.clone().unwrap_or_default()
            })
        }
        G if shift => keys.words.push(say("statistics")),
        G => keys.words.push(say("goto")),
        K => ask_for_a_link(book),
        L if shift => keys.acts.push(Act::Filter),
        O if shift => keys.words.push(say("goto-special notes")),
        P if shift => book.view.panel = Some(PanelPath(vec!["Font".to_owned()])),
        P => keys.words.push(say("print-preview")),
        Q => keys.words.push(say("quick-analysis")),
        T | L => keys.acts.push(Act::Table(None)),
        Backspace => book.view.reveal = true,
        Period => next_corner(book),
        PageDown if shift => keys.words.push(say("select-sheets next")),
        PageUp if shift => keys.words.push(say("select-sheets previous")),
        PageDown => keys.acts.push(Act::ShowSheet(next_sheet(book, 1))),
        PageUp => keys.acts.push(Act::ShowSheet(next_sheet(book, -1))),
        _ => {}
    }
}

/// Ctrl+A: the block of data first, then the whole sheet, as Excel's goes.
fn select_all(book: &mut Book) {
    let range = book.view.range();
    let region = current_region(&book.session.book.sheets[book.view.sheet], book.view.cursor);
    let cursor = book.view.cursor;
    if region.len() > 1 && range != region {
        book.view.select_range(region);
        book.view.cursor = cursor;
        book.view.anchor = if cursor == region.first { region.last } else { region.first };
    } else {
        book.view
            .select_range(Range::new(CellRef::new(0, 0), CellRef::new(MAX_ROWS - 1, MAX_COLS - 1)));
    }
}

/// Ctrl+Shift+*: the current region, keeping the active cell.
fn select_current_region(book: &mut Book) {
    let cursor = book.view.cursor;
    let region = current_region(&book.session.book.sheets[book.view.sheet], cursor);
    book.view.select_range(region);
    book.view.cursor = cursor;
    book.view.anchor = if cursor == region.first { region.last } else { region.first };
}

/// Ctrl+Period: the active cell moves to the next corner of the selection, clockwise.
fn next_corner(book: &mut Book) {
    let r = book.view.range();
    let corners = [
        r.first,
        CellRef::new(r.first.row, r.last.col),
        r.last,
        CellRef::new(r.last.row, r.first.col),
    ];
    let at = corners.iter().position(|c| *c == book.view.cursor).unwrap_or(0);
    let next = corners[(at + 1) % 4];
    let opposite = corners[(at + 3) % 4];
    book.view.cursor = next;
    book.view.anchor = CellRef::new(
        if next.row == r.first.row { r.last.row } else { r.first.row },
        if next.col == r.first.col { r.last.col } else { r.first.col },
    );
    let _ = opposite;
    book.view.reveal = true;
}

/// Ctrl+' starts typing the formula of the cell above, and Ctrl+Shift+" its value, as Excel does.
fn from_above(book: &mut Book, value: bool) {
    let at = book.view.cursor;
    let Some(above) = at.offset(-1, 0) else { return };
    let s = &book.session.book;
    let sheet = &s.sheets[book.view.sheet];
    let text = if value {
        display::shown(s, sheet, above, 255).text
    } else {
        display::input_text(s, sheet, above)
    };
    start_typing(book, text);
}

/// Ctrl+; types today's date and Ctrl+Shift+; the time now.
fn type_now(book: &Book, time: bool, keys: &mut Keys) {
    let now = crate::services::clock::date_time();
    let text = if time {
        let (hour, half) = match now.hour {
            0 => (12, "AM"),
            1..=11 => (now.hour, "AM"),
            12 => (12, "PM"),
            h => (h - 12, "PM"),
        };
        format!("{hour}:{:02} {half}", now.minute)
    } else {
        format!("{}/{}/{}", now.month, now.day, now.year)
    };
    keys.acts.push(Act::Type { at: book.view.cursor, text });
}

/// Ctrl+Plus and Ctrl+Minus: whole rows or columns at once, and otherwise a box asking which way the cells
/// beside move, as Excel's Insert and Delete dialogs ask.
fn insert_or_delete(book: &Book, insert: bool, keys: &mut Keys) {
    let range = book.view.range();
    let whole = if whole_rows(range) {
        Some(Shift::WholeRows)
    } else if whole_columns(range) {
        Some(Shift::WholeColumns)
    } else {
        None
    };
    match (whole, insert) {
        (Some(shift), true) => keys.acts.push(Act::Insert(shift)),
        (Some(shift), false) => keys.acts.push(Act::Delete(shift)),
        (None, true) => keys.words.push(say("insert ask")),
        (None, false) => keys.words.push(say("delete ask")),
    }
}

/// Ctrl+K: the box asking for a link's address and text.
fn ask_for_a_link(book: &mut Book) {
    let at = book.view.cursor;
    book.view.prompt = Some(Prompt {
        verb: vec!["link".to_owned(), at.a1()],
        title: format!("Link on {}", at.a1()),
        label: "Address or place".to_owned(),
        text: String::new(),
        second: Some(("Text to show".to_owned(), String::new())),
        focus: true,
        fields: Vec::new(),
    });
}

fn number_format(code: &str) -> Act {
    Act::Style(StyleChange::NumberFormat(code.to_owned()))
}

fn bold(book: &Book) -> Act {
    toggle(book, |f| f.bold, |v| FontEdit { bold: Some(v), ..Default::default() })
}

fn italic(book: &Book) -> Act {
    toggle(book, |f| f.italic, |v| FontEdit { italic: Some(v), ..Default::default() })
}

fn underline(book: &Book) -> Act {
    toggle(
        book,
        |f| f.underline != Underline::None,
        |v| FontEdit {
            underline: Some(if v { Underline::Single } else { Underline::None }),
            ..Default::default()
        },
    )
}

/// The next shown sheet before or after the one showing.
fn next_sheet(book: &Book, step: i64) -> usize {
    let count = book.session.book.sheets.len() as i64;
    let mut at = book.view.sheet as i64;
    for _ in 0..count {
        at += step;
        if at < 0 || at >= count {
            return book.view.sheet;
        }
        if book.session.book.sheets[at as usize].state
            == unluminous_sheet::model::Visibility::Visible
        {
            return at as usize;
        }
    }
    book.view.sheet
}

/// A font change that turns a property on unless the active cell already has it, as Ctrl+B does.
fn toggle(
    book: &Book,
    has: fn(&unluminous_sheet::styles::Font) -> bool,
    edit: fn(bool) -> FontEdit,
) -> Act {
    let s = &book.session.book;
    let style = display::style_at(&s.sheets[book.view.sheet], book.view.cursor);
    let on = has(&s.styles.font_of(style));
    Act::Style(StyleChange::Font(edit(!on)))
}

/// What the keys did while a cell is being typed into.
#[derive(Debug, Default, PartialEq)]
pub enum Finish {
    #[default]
    Typing,
    /// Write the text and move this way.
    Commit {
        rows: i64,
        cols: i64,
    },
    /// Write the text into every chosen cell.
    CommitEverywhere,
    /// Write it as a legacy array formula over the selection.
    CommitArray,
    Cancel,
}

/// What the function list is showing, which Up, Down and Tab steer while it is open.
#[derive(Debug, Clone, Default)]
pub struct Offered {
    pub start: usize,
    pub names: Vec<&'static str>,
}

/// Read the keys that finish or steer typing, before the box draws and takes the rest.
pub fn typing(ctx: &egui::Context, book: &mut Book, box_id: egui::Id, offered: &Offered) -> Finish {
    let Some(edit) = book.view.edit.as_ref() else { return Finish::Typing };
    let typed = edit.typed;
    let formula = edit.text.starts_with('=');
    let caret = caret_of(ctx, box_id).unwrap_or(edit.text.chars().count());
    let pointing_here = formula
        && (edit.pointing.is_some_and(|(_, end, _)| end == caret)
            || formula_text::points_at(&edit.text, caret));
    let list_open = !offered.names.is_empty();
    let mut finish = Finish::Typing;
    let mut moves: Vec<(i64, i64, bool)> = Vec::new();
    let mut cycle = false;
    let mut chosen = edit.chosen as i64;
    let mut complete = false;
    let mut toggle_mode = false;
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            let Event::Key { key, pressed: true, modifiers: m, .. } = event else { return true };
            let command = m.command || m.ctrl;
            match key {
                Key::Escape => {
                    finish = Finish::Cancel;
                    false
                }
                Key::Enter if m.alt => true,
                Key::Enter if list_open && !command && !m.shift => {
                    complete = true;
                    false
                }
                Key::Enter => {
                    finish = if command && m.shift {
                        Finish::CommitArray
                    } else if command {
                        Finish::CommitEverywhere
                    } else {
                        Finish::Commit { rows: if m.shift { -1 } else { 1 }, cols: 0 }
                    };
                    false
                }
                Key::Tab if list_open => {
                    complete = true;
                    false
                }
                Key::Tab => {
                    finish = Finish::Commit { rows: 0, cols: if m.shift { -1 } else { 1 } };
                    false
                }
                Key::ArrowDown | Key::ArrowUp if list_open => {
                    chosen += if *key == Key::ArrowDown { 1 } else { -1 };
                    false
                }
                Key::F4 => {
                    cycle = true;
                    false
                }
                Key::F2 => {
                    toggle_mode = true;
                    false
                }
                Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
                    if typed && !command =>
                {
                    let (rows, cols) = match key {
                        Key::ArrowUp => (-1, 0),
                        Key::ArrowDown => (1, 0),
                        Key::ArrowLeft => (0, -1),
                        _ => (0, 1),
                    };
                    if pointing_here {
                        moves.push((rows, cols, m.shift));
                    } else {
                        finish = Finish::Commit { rows, cols };
                    }
                    false
                }
                _ => true,
            }
        });
    });
    let Some(edit) = book.view.edit.as_mut() else { return finish };
    if toggle_mode {
        edit.typed = !edit.typed;
    }
    if list_open {
        edit.chosen = chosen.rem_euclid(offered.names.len() as i64) as usize;
    }
    if complete {
        if let Some(name) = offered.names.get(edit.chosen) {
            let chars: Vec<char> = edit.text.chars().collect();
            let mut text: String = chars[..offered.start].iter().collect();
            text.push_str(name);
            text.push('(');
            let new_caret = text.chars().count();
            text.extend(chars[caret.min(chars.len())..].iter());
            edit.text = text;
            set_caret(ctx, box_id, new_caret);
            edit.chosen = 0;
        }
    }
    if cycle {
        if let Some((text, new_caret)) = formula_text::cycle_reference(&edit.text, caret) {
            edit.text = text;
            set_caret(ctx, box_id, new_caret);
        }
    }
    for (rows, cols, extend) in moves {
        let (start, end, range) = edit.pointing.unwrap_or((caret, caret, Range::cell(edit.at)));
        let base = if edit.pointing.is_some() { range } else { Range::cell(edit.at) };
        let moved = if extend {
            Range::new(base.first, base.last.offset(rows, cols).unwrap_or(base.last))
        } else {
            let to = base.first.offset(rows, cols).unwrap_or(base.first);
            Range::cell(to)
        };
        let new_end = put_reference(edit, start, end, moved);
        set_caret(ctx, box_id, new_end);
    }
    finish
}

/// Put a reference into the formula being typed, replacing the one pointed at before, and answer where it
/// ends.
pub fn put_reference(edit: &mut Edit, start: usize, end: usize, range: Range) -> usize {
    let chars: Vec<char> = edit.text.chars().collect();
    let shown = range.a1();
    let mut text: String = chars[..start.min(chars.len())].iter().collect();
    text.push_str(&shown);
    let new_end = text.chars().count();
    text.extend(chars[end.min(chars.len())..].iter());
    edit.text = text;
    edit.pointing = Some((start, new_end, range));
    new_end
}

/// Where the caret is in a text box, in characters.
pub fn caret_of(ctx: &egui::Context, id: egui::Id) -> Option<usize> {
    let state = egui::TextEdit::load_state(ctx, id)?;
    Some(state.cursor.char_range()?.primary.index.0)
}

/// Put the caret of a text box at a character.
pub fn set_caret(ctx: &egui::Context, id: egui::Id, at: usize) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
    state.store(ctx, id);
}
