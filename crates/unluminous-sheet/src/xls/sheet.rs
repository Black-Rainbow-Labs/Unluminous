//! One worksheet of a BIFF8 workbook: the records between its `BOF` and its `EOF`.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::features;
use super::globals::Globals;
use super::id;
use super::ptg::{decode, error_text, Decoded, Dialect, Env, Origin};
use super::records::{Reader, Record};
use super::styles::Palette;
use crate::coord::{CellRef, Range};
use crate::formula::{Expr, Formula};
use crate::model::{Cell, ColumnInfo, Note, RowInfo, Sheet};
use crate::styles::Dxf;
use crate::value::{ErrorKind, Value};

/// What reading a sheet needs from the workbook.
pub struct Context<'a> {
    pub globals: &'a Globals,
    pub env: &'a Env<'a>,
    /// The model's index for each stored format index.
    pub xf_map: &'a [u32],
    pub palette: &'a Palette,
    /// The differences conditional formats make, which every sheet adds to.
    pub dxfs: &'a DxfPool,
}

/// The differential formats of the workbook, added to while its sheets are read.
#[derive(Default)]
pub struct DxfPool(RefCell<Vec<Dxf>>);

impl DxfPool {
    /// The index of a format, adding it when it is new.
    pub fn push(&self, dxf: Dxf) -> u32 {
        let mut list = self.0.borrow_mut();
        if let Some(index) = list.iter().position(|d| *d == dxf) {
            return index as u32;
        }
        list.push(dxf);
        (list.len() - 1) as u32
    }

    pub fn into_inner(self) -> Vec<Dxf> {
        self.0.into_inner()
    }
}

/// A formula cell whose text waits for a shared or array formula that is defined after it.
struct Pending {
    row: u32,
    col: u32,
    exp: (u32, u32),
}

/// A shared or array formula: its tokens and the cells it covers.
struct Stored {
    rgce: Vec<u8>,
    rgcb: Vec<u8>,
    range: Range,
}

/// The state of reading one sheet.
struct State {
    last_formula: Option<(u32, u32)>,
    pending_string: Option<(u32, u32)>,
    pending: Vec<Pending>,
    shared: HashMap<(u32, u32), Stored>,
    arrays: HashMap<(u32, u32), Stored>,
    current_object: Option<u16>,
    text_boxes: HashMap<u16, String>,
    notes: Vec<(u32, u32, u16, String)>,
    tips: Vec<(Range, String)>,
    features: features::Collector,
}

/// Read a sheet from the records that start at its `BOF`.
pub fn read(records: &[Record], sheet: &mut Sheet, ctx: &Context, index: usize) {
    let mut state = State {
        last_formula: None,
        pending_string: None,
        pending: Vec::new(),
        shared: HashMap::new(),
        arrays: HashMap::new(),
        current_object: None,
        text_boxes: HashMap::new(),
        notes: Vec::new(),
        tips: Vec::new(),
        features: features::Collector::default(),
    };
    let mut depth = 0;
    for record in records {
        match record.id {
            id::BOF => depth += 1,
            id::EOF => {
                depth -= 1;
                if depth <= 0 {
                    break;
                }
            }
            _ if depth > 1 => {}
            _ => record_in_sheet(record, sheet, ctx, &mut state),
        }
    }
    resolve_pending(sheet, ctx, &mut state);
    attach_notes(sheet, &state);
    state.features.finish(sheet, ctx, index);
    apply_link_tips(sheet, &state.tips);
}

fn record_in_sheet(record: &Record, sheet: &mut Sheet, ctx: &Context, state: &mut State) {
    match record.id {
        id::ROW => row_record(record, sheet, ctx),
        id::COLINFO => column_record(record, sheet, ctx),
        id::DEFCOLWIDTH => {
            if let Some(b) = record.data.get(..2) {
                let width = u16::from_le_bytes([b[0], b[1]]) as f64;
                sheet.default_col_width = (width != 8.0).then_some(width);
            }
        }
        id::DEFAULTROWHEIGHT => {
            if let Some(b) = record.data.get(2..4) {
                sheet.default_row_height = u16::from_le_bytes([b[0], b[1]]) as f64 / 20.0;
            }
        }
        id::MERGEDCELLS => merged_cells(record, sheet),
        id::WINDOW2 => window_record(record, sheet, &mut state.features),
        id::PANE => state.features.pane = pane_record(record),
        id::SCL => scale_record(record, sheet),
        id::LABELSST | id::LABEL | id::RSTRING => label_record(record, sheet, ctx),
        id::NUMBER | id::RK | id::MULRK | id::BOOLERR | id::BLANK | id::MULBLANK => {
            plain_cells(record, sheet, ctx)
        }
        id::FORMULA => formula_record(record, sheet, ctx, state),
        id::STRING => {
            if let (Some((row, col)), Some(text)) =
                (state.pending_string.take(), Reader::new(&record.data).string())
            {
                if let Some(cell) = sheet.cells.get_mut(&(row, col)) {
                    cell.value = Value::text(text);
                }
            }
        }
        id::SHRFMLA => stored_formula(record, state, false),
        id::ARRAY => stored_formula(record, state, true),
        id::OBJ => state.current_object = object_id(record),
        id::TXO => {
            if let (Some(object), Some(text)) = (state.current_object, text_box(record)) {
                state.text_boxes.insert(object, text);
            }
        }
        id::NOTE => note_record(record, state),
        id::LINKTIP => features::link_tip(record, &mut state.tips),
        _ => state.features.record(record, sheet, ctx),
    }
}

/// The model's format index for a stored one.
fn style_of(ctx: &Context, ixfe: u16) -> u32 {
    ctx.xf_map.get(ixfe as usize).copied().unwrap_or(0)
}

fn put(sheet: &mut Sheet, row: u32, col: u32, cell: Cell) {
    sheet.cells.insert((row, col), cell);
}

fn row_record(record: &Record, sheet: &mut Sheet, ctx: &Context) {
    let mut r = Reader::new(&record.data);
    let (Some(row), Some(_first), Some(_last), Some(height)) = (r.u16(), r.u16(), r.u16(), r.u16())
    else {
        return;
    };
    let (Some(_), Some(_), Some(flags), Some(ixfe)) = (r.u16(), r.u16(), r.u16(), r.u16()) else {
        return;
    };
    let info = RowInfo {
        height: (height & 0x8000 == 0).then(|| (height & 0x7FFF) as f64 / 20.0),
        hidden: flags & 0x20 != 0,
        style: (flags & 0x80 != 0).then(|| style_of(ctx, ixfe & 0x0FFF)),
        outline: (flags & 7) as u8,
        collapsed: flags & 0x10 != 0,
        attrs: Vec::new(),
    };
    if info != RowInfo::default() {
        sheet.rows.insert(row as u32, info);
    }
}

fn column_record(record: &Record, sheet: &mut Sheet, ctx: &Context) {
    let mut r = Reader::new(&record.data);
    let (Some(first), Some(last), Some(width), Some(ixfe), Some(flags)) =
        (r.u16(), r.u16(), r.u16(), r.u16(), r.u16())
    else {
        return;
    };
    sheet.columns.push(ColumnInfo {
        first: first as u32,
        last: (last as u32).min(255),
        width: Some(width as f64 / 256.0),
        hidden: flags & 1 != 0,
        style: style_of(ctx, ixfe),
        custom_width: flags & 2 != 0,
        outline: ((flags >> 8) & 7) as u8,
        collapsed: flags & 0x1000 != 0,
        best_fit: flags & 4 != 0,
    });
}

fn merged_cells(record: &Record, sheet: &mut Sheet) {
    let mut r = Reader::new(&record.data);
    let Some(count) = r.u16() else { return };
    for _ in 0..count {
        let (Some(r1), Some(r2), Some(c1), Some(c2)) = (r.u16(), r.u16(), r.u16(), r.u16()) else {
            return;
        };
        sheet.merges.push(Range::new(
            CellRef::new(r1 as u32, c1 as u32),
            CellRef::new(r2 as u32, c2 as u32),
        ));
    }
}

fn window_record(record: &Record, sheet: &mut Sheet, features: &mut features::Collector) {
    let Some(flags) = record.data.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]])) else {
        return;
    };
    sheet.show_grid = flags & 0x02 != 0;
    features.frozen = flags & 0x08 != 0;
    if let Some(b) = record.data.get(12..14) {
        let zoom = u16::from_le_bytes([b[0], b[1]]) as u32;
        if zoom > 0 {
            sheet.zoom = zoom;
        }
    }
}

/// The columns and rows a pane record splits at.
fn pane_record(record: &Record) -> Option<(u32, u32)> {
    let mut r = Reader::new(&record.data);
    let x = r.u16()? as u32;
    let y = r.u16()? as u32;
    Some((y, x))
}

fn scale_record(record: &Record, sheet: &mut Sheet) {
    let mut r = Reader::new(&record.data);
    if let (Some(num), Some(den)) = (r.u16(), r.u16()) {
        if den != 0 {
            sheet.zoom = (num as u32 * 100) / den as u32;
        }
    }
}

fn label_record(record: &Record, sheet: &mut Sheet, ctx: &Context) {
    let mut r = Reader::new(&record.data);
    let (Some(row), Some(col), Some(ixfe)) = (r.u16(), r.u16(), r.u16()) else { return };
    let text = match record.id {
        id::LABELSST => {
            let Some(index) = r.u32() else { return };
            ctx.globals.strings.get(index as usize).cloned().unwrap_or_default()
        }
        _ => r.string().unwrap_or_default(),
    };
    put(
        sheet,
        row as u32,
        col as u32,
        Cell { value: Value::text(text), style: style_of(ctx, ixfe), ..Default::default() },
    );
}

/// The number an `RK` value holds.
pub fn rk_value(rk: u32) -> f64 {
    let value = if rk & 2 != 0 {
        (rk as i32 >> 2) as f64
    } else {
        f64::from_bits(((rk & 0xFFFF_FFFC) as u64) << 32)
    };
    if rk & 1 != 0 {
        value / 100.0
    } else {
        value
    }
}

/// A value stored as a boolean or an error.
fn bool_or_error(value: u8, is_error: bool) -> Value {
    if is_error {
        Value::Error(ErrorKind::parse(error_text(value)).unwrap_or(ErrorKind::Value))
    } else {
        Value::Bool(value != 0)
    }
}

fn plain_cells(record: &Record, sheet: &mut Sheet, ctx: &Context) {
    let mut r = Reader::new(&record.data);
    let Some(row) = r.u16() else { return };
    let Some(col) = r.u16() else { return };
    let row = row as u32;
    let col = col as u32;
    let cell =
        |value: Value, ixfe: u16| Cell { value, style: style_of(ctx, ixfe), ..Default::default() };
    match record.id {
        id::NUMBER => {
            if let (Some(ixfe), Some(v)) = (r.u16(), r.f64()) {
                put(sheet, row, col, cell(Value::Number(v), ixfe));
            }
        }
        id::RK => {
            if let (Some(ixfe), Some(rk)) = (r.u16(), r.u32()) {
                put(sheet, row, col, cell(Value::Number(rk_value(rk)), ixfe));
            }
        }
        id::BOOLERR => {
            if let (Some(ixfe), Some(v), Some(e)) = (r.u16(), r.u8(), r.u8()) {
                put(sheet, row, col, cell(bool_or_error(v, e != 0), ixfe));
            }
        }
        id::BLANK => {
            if let Some(ixfe) = r.u16() {
                put(sheet, row, col, cell(Value::Empty, ixfe));
            }
        }
        id::MULRK => {
            let end = record.data.len().saturating_sub(2);
            let mut at = col;
            while r.at + 6 <= end {
                if let (Some(ixfe), Some(rk)) = (r.u16(), r.u32()) {
                    put(sheet, row, at, cell(Value::Number(rk_value(rk)), ixfe));
                    at += 1;
                }
            }
        }
        id::MULBLANK => {
            let end = record.data.len().saturating_sub(2);
            let mut at = col;
            while r.at + 2 <= end {
                if let Some(ixfe) = r.u16() {
                    put(sheet, row, at, cell(Value::Empty, ixfe));
                    at += 1;
                }
            }
        }
        _ => {}
    }
}

/// Turn a decoded formula into the model's formula, frozen when it cannot be computed here.
pub fn formula_from(decoded: Result<Decoded, String>) -> Option<Formula> {
    match decoded {
        Ok(Decoded::Text(text)) => Some(crate::xlsx::read::file_formula(&text, false)),
        Ok(_) => None,
        Err(_) => Some(Formula {
            text: Arc::from("#NAME?"),
            expr: Arc::new(Expr::Error(ErrorKind::Name)),
            dynamic: false,
            array: None,
            array_member: None,
            frozen: true,
            attrs: Default::default(),
        }),
    }
}

fn formula_record(record: &Record, sheet: &mut Sheet, ctx: &Context, state: &mut State) {
    let mut r = Reader::new(&record.data);
    let (Some(row), Some(col), Some(ixfe)) = (r.u16(), r.u16(), r.u16()) else { return };
    let Some(cached) = r.take(8) else { return };
    let (Some(_flags), Some(_chn), Some(cce)) = (r.u16(), r.u32(), r.u16()) else { return };
    let Some(rgce) = r.take(cce as usize) else { return };
    let rgcb = r.rest();
    let (row, col) = (row as u32, col as u32);
    let mut pending_string = false;
    let value = if cached[6] == 0xFF && cached[7] == 0xFF {
        match cached[0] {
            0 => {
                pending_string = true;
                Value::Empty
            }
            1 => bool_or_error(cached[2], false),
            2 => bool_or_error(cached[2], true),
            _ => Value::text(""),
        }
    } else {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(cached);
        Value::Number(f64::from_le_bytes(bytes))
    };
    state.last_formula = Some((row, col));
    state.pending_string = pending_string.then_some((row, col));
    let decoded = decode(Dialect::Biff8, rgce, rgcb, ctx.env, Origin { row, col });
    let mut cell = Cell { value, style: style_of(ctx, ixfe), ..Default::default() };
    match decoded {
        Ok(Decoded::Exp { row: er, col: ec }) => {
            state.pending.push(Pending { row, col, exp: (er, ec) })
        }
        Ok(Decoded::Table) => {}
        other => cell.formula = formula_from(other),
    }
    put(sheet, row, col, cell);
}

/// A shared or array formula record, which belongs to the formula cell before it.
fn stored_formula(record: &Record, state: &mut State, array: bool) {
    let mut r = Reader::new(&record.data);
    let (Some(r1), Some(r2), Some(c1), Some(c2)) = (r.u16(), r.u16(), r.u8(), r.u8()) else {
        return;
    };
    if array {
        let (Some(_flags), Some(_chn)) = (r.u16(), r.u32()) else { return };
    } else if r.skip(2).is_none() {
        return;
    }
    let Some(cce) = r.u16() else { return };
    let Some(rgce) = r.take(cce as usize) else { return };
    let stored = Stored {
        rgce: rgce.to_vec(),
        rgcb: r.rest().to_vec(),
        range: Range::new(CellRef::new(r1 as u32, c1 as u32), CellRef::new(r2 as u32, c2 as u32)),
    };
    let anchor = state.last_formula.unwrap_or((r1 as u32, c1 as u32));
    let map = if array { &mut state.arrays } else { &mut state.shared };
    map.insert(
        (r1 as u32, c1 as u32),
        Stored { rgce: stored.rgce.clone(), rgcb: stored.rgcb.clone(), range: stored.range },
    );
    map.insert(anchor, stored);
}

/// Give the cells that named a shared or array formula their formula.
fn resolve_pending(sheet: &mut Sheet, ctx: &Context, state: &mut State) {
    for p in std::mem::take(&mut state.pending) {
        let key = p.exp;
        if let Some(shared) = state.shared.get(&key) {
            let decoded = decode(
                Dialect::Biff8,
                &shared.rgce,
                &shared.rgcb,
                ctx.env,
                Origin { row: p.row, col: p.col },
            );
            if let Some(cell) = sheet.cells.get_mut(&(p.row, p.col)) {
                cell.formula = formula_from(decoded);
            }
        } else if let Some(array) = state.arrays.get(&key) {
            if (p.row, p.col) != key
                && (p.row, p.col) != (array.range.first.row, array.range.first.col)
            {
                continue;
            }
            let origin = Origin { row: array.range.first.row, col: array.range.first.col };
            let decoded = decode(Dialect::Biff8, &array.rgce, &array.rgcb, ctx.env, origin);
            if let Some(mut formula) = formula_from(decoded) {
                formula.array = Some(array.range);
                sheet.array_formulas.push(array.range);
                if let Some(cell) = sheet.cells.get_mut(&(p.row, p.col)) {
                    cell.formula = Some(formula);
                }
            }
        }
    }
}

/// The identifier an `OBJ` record gives its object.
fn object_id(record: &Record) -> Option<u16> {
    let mut r = Reader::new(&record.data);
    if r.u16()? != 0x0015 {
        return None;
    }
    r.skip(4)?;
    r.u16()
}

/// The text of a `TXO` record, which sits in the continuation after it.
fn text_box(record: &Record) -> Option<String> {
    let mut r = Reader::new(&record.data);
    r.skip(10)?;
    let cch = r.u16()? as usize;
    let first = *record.starts.first()?;
    let mut text = String::new();
    let mut counted = 0;
    let mut starts = record.starts.clone();
    starts.push(record.data.len());
    for window in starts.windows(2) {
        if counted >= cch {
            break;
        }
        let part = record.data.get(window[0]..window[1])?;
        let mut reader = Reader::new(part);
        let flags = reader.u8()?;
        let wide = flags & 1 != 0;
        let available = part.len().saturating_sub(1) / if wide { 2 } else { 1 };
        let take = (cch - counted).min(available);
        text.push_str(&reader.chars(take, wide)?);
        counted += take;
    }
    let _ = first;
    Some(text)
}

fn note_record(record: &Record, state: &mut State) {
    let mut r = Reader::new(&record.data);
    let (Some(row), Some(col), Some(_flags), Some(object)) = (r.u16(), r.u16(), r.u16(), r.u16())
    else {
        return;
    };
    let author = r.string().unwrap_or_default();
    state.notes.push((row as u32, col as u32, object, author));
}

fn attach_notes(sheet: &mut Sheet, state: &State) {
    let mut notes: BTreeMap<(u32, u32), Note> = BTreeMap::new();
    for (row, col, object, author) in &state.notes {
        let text = state.text_boxes.get(object).cloned().unwrap_or_default();
        notes.insert((*row, *col), Note { author: author.clone(), text, visible: false, attrs: Vec::new() });
    }
    // The notes are written out again when the sheet is saved as an .xlsx file.
    sheet.notes_changed = !notes.is_empty();
    sheet.notes = notes;
}

/// Give each link the tooltip a `LINKTIP` record gave the same cells.
fn apply_link_tips(sheet: &mut Sheet, tips: &[(Range, String)]) {
    for link in &mut sheet.links {
        link.tooltip =
            tips.iter().find(|(range, _)| *range == link.range).map(|(_, text)| text.clone());
    }
}
