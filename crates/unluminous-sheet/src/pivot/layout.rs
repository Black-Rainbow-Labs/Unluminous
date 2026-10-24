//! The rectangle a pivot table fills: its labels, its captions and its figures, in the places Excel puts
//! them for the compact, outline and tabular forms.

use super::acc::CellVal;
use super::compute::Calc;
use super::items::{Line, LineKind, Lines, Seg};
use super::table::{AxisEntry, PivotTable};
use super::{Form, Func};
use crate::value::Value;

/// What a laid out cell is, so a pivot style can be drawn over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A filter field's name or the item it shows.
    PageField,
    PageItem,
    /// The corner and the field captions: Row Labels, Column Labels, a field's name.
    Caption,
    /// An item above a column.
    ColLabel,
    /// An item at the left of a row.
    RowLabel,
    Data,
    /// The label of a subtotal row or column.
    SubtotalLabel,
    SubtotalData,
    GrandLabel,
    GrandData,
    BlankRow,
}

/// One cell of the laid out table.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidCell {
    /// Relative to the first cell of the whole table, filter area included.
    pub row: u32,
    pub col: u32,
    pub value: Value,
    pub role: Role,
    /// The number format code, if the cell has one.
    pub format: Option<String>,
    /// How far in the label is indented.
    pub indent: u32,
}

/// The whole table laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Laid {
    pub rows: u32,
    pub cols: u32,
    /// Rows above the table's own cells taken by the filter fields and the empty row after them.
    pub page_rows: u32,
    pub cells: Vec<LaidCell>,
    pub lines: Lines,
    pub first_header_row: u32,
    pub first_data_row: u32,
    pub first_data_col: u32,
}

/// Which field a level of an axis is.
#[derive(Debug, Clone, Copy)]
enum Level {
    /// The field, and its place among the axis's real fields.
    Field(usize, usize),
    Values,
}

/// Lay a table out from its calculation.
pub fn lay_out(calc: &Calc) -> Laid {
    let table = calc.table;
    let rows_levels = levels(&table.row_axis());
    let cols_levels = levels(&table.col_axis());
    let n_data = table.data.len();
    let row_lines = build_lines(calc, true, &rows_levels);
    let col_lines = build_lines(calc, false, &cols_levels);
    let shape = Shape::of(table, &rows_levels, &cols_levels, row_lines.len(), col_lines.len());
    let mut laid = Laid {
        rows: shape.height + shape.data_rows,
        cols: shape.width + shape.data_cols,
        ..Default::default()
    };
    laid.first_header_row = shape.first_header_row;
    laid.first_data_row = shape.height;
    laid.first_data_col = shape.width;
    let mut out = Out { cells: Vec::new() };
    corner_cells(calc, &shape, &rows_levels, &cols_levels, &mut out);
    col_header_cells(calc, &shape, &cols_levels, &col_lines, &mut out);
    row_label_cells(calc, &shape, &rows_levels, &row_lines, &mut out);
    data_cells(calc, &shape, n_data, &row_lines, &col_lines, &mut out);
    laid.cells = out.cells;
    laid.lines = Lines { rows: row_lines, cols: col_lines };
    page_cells(calc, &mut laid);
    laid
}

fn levels(axis: &[AxisEntry]) -> Vec<Level> {
    let mut real = 0;
    axis.iter()
        .map(|e| match e {
            AxisEntry::Field(f) => {
                real += 1;
                Level::Field(*f, real - 1)
            }
            AxisEntry::Values => Level::Values,
        })
        .collect()
}

struct Out {
    cells: Vec<LaidCell>,
}

impl Out {
    fn put(&mut self, row: u32, col: u32, value: Value, role: Role) {
        self.cells.push(LaidCell { row, col, value, role, format: None, indent: 0 });
    }

    fn put_labelled(&mut self, row: u32, col: u32, label: (Value, Option<String>), role: Role) {
        self.cells.push(LaidCell { row, col, value: label.0, role, format: label.1, indent: 0 });
    }

    fn put_text(&mut self, row: u32, col: u32, text: &str, role: Role) {
        self.put(row, col, Value::text(text), role);
    }
}

// ----------------------------------------------------------------------------------------------------
// Shape.

/// The sizes the pieces of the table have.
struct Shape {
    /// Label columns.
    width: u32,
    /// Header rows.
    height: u32,
    data_rows: u32,
    data_cols: u32,
    /// For each row level, the label column it uses.
    label_col: Vec<u32>,
    has_real_cols: bool,
    first_header_row: u32,
    /// The label column of the data row when there are no row fields.
    data_row_label: bool,
}

impl Shape {
    fn of(table: &PivotTable, rows: &[Level], cols: &[Level], n_rows: usize, n_cols: usize) -> Shape {
        let n_data = table.data.len();
        let has_real_cols = cols.iter().any(|l| matches!(l, Level::Field(..)));
        let mut label_col = Vec::new();
        let mut width = 0u32;
        let mut previous_compact = false;
        for level in rows {
            let form = match level {
                Level::Field(f, _) => table.fields[*f].form,
                Level::Values => table.options.form,
            };
            if form == Form::Compact && previous_compact {
                label_col.push(width - 1);
            } else {
                label_col.push(width);
                width += 1;
            }
            previous_compact = form == Form::Compact;
        }
        let mut data_row_label = false;
        if rows.is_empty() && has_real_cols && n_data == 1 {
            width = 1;
            data_row_label = true;
        }
        let height = if has_real_cols { 1 + cols.len() as u32 } else { 1 };
        let first_header_row = if has_real_cols || !cols.iter().any(|l| matches!(l, Level::Values)) {
            1
        } else {
            0
        };
        Shape {
            width,
            height,
            data_rows: n_rows as u32,
            data_cols: n_cols as u32,
            label_col,
            has_real_cols,
            first_header_row,
            data_row_label,
        }
    }
}

// ----------------------------------------------------------------------------------------------------
// Lines.

struct Builder<'a, 'b> {
    calc: &'a Calc<'b>,
    rows: bool,
    levels: &'a [Level],
    out: Vec<Line>,
}

fn build_lines(calc: &Calc, rows: bool, levels: &[Level]) -> Vec<Line> {
    let table = calc.table;
    let n_data = table.data.len();
    let axis = if rows { &calc.rows } else { &calc.cols };
    let mut b = Builder { calc, rows, levels, out: Vec::new() };
    if levels.is_empty() {
        if n_data >= 1 {
            b.out.push(line(LineKind::Leaf, Vec::new(), 0, None, 0));
        }
        return b.out;
    }
    let mut path = Vec::new();
    b.walk(0, 0, None, &mut path);
    let has_real = levels.iter().any(|l| matches!(l, Level::Field(..)));
    let shown = if rows { table.options.grand_total_row } else { table.options.grand_total_col };
    if has_real && shown {
        let has_values = levels.iter().any(|l| matches!(l, Level::Values));
        if has_values {
            for d in 0..n_data {
                b.out.push(line(LineKind::Grand, vec![Seg::Data(d)], 0, Some(d), 0));
            }
        } else {
            b.out.push(line(LineKind::Grand, Vec::new(), 0, None, 0));
        }
    }
    let _ = axis;
    b.out
}

fn line(kind: LineKind, path: Vec<Seg>, node: usize, data: Option<usize>, level: usize) -> Line {
    Line {
        kind,
        path,
        node,
        data,
        func: None,
        level,
        values: !matches!(kind, LineKind::Blank),
        shown: true,
    }
}

impl Builder<'_, '_> {
    fn field_form(&self, field: usize) -> Form {
        self.calc.table.fields[field].form
    }

    /// Add the lines for the items under `node` at level `li`.
    fn walk(&mut self, li: usize, node: usize, data: Option<usize>, path: &mut Vec<Seg>) {
        if li == self.levels.len() {
            self.out.push(line(LineKind::Leaf, path.clone(), node, data, li));
            return;
        }
        match self.levels[li] {
            Level::Values => self.walk_values(li, node, path),
            Level::Field(f, k) => self.walk_field(li, f, k, node, data, path),
        }
    }

    fn walk_values(&mut self, li: usize, node: usize, path: &mut Vec<Seg>) {
        let last = li == self.levels.len() - 1;
        for d in 0..self.calc.table.data.len() {
            path.push(Seg::Data(d));
            if last {
                self.out.push(line(LineKind::Leaf, path.clone(), node, Some(d), li));
            } else {
                let mut header = line(LineKind::Header, path.clone(), node, Some(d), li);
                header.values = false;
                if self.values_form_has_headers() {
                    self.out.push(header);
                }
                self.walk(li + 1, node, Some(d), path);
            }
            path.pop();
        }
    }

    fn values_form_has_headers(&self) -> bool {
        self.rows && self.calc.table.options.form != Form::Tabular
    }

    fn walk_field(
        &mut self,
        li: usize,
        f: usize,
        k: usize,
        node: usize,
        data: Option<usize>,
        path: &mut Vec<Seg>,
    ) {
        let axis = if self.rows { &self.calc.rows } else { &self.calc.cols };
        let children = axis.nodes[node].children.clone();
        let pf = &self.calc.table.fields[f];
        let last = li == self.levels.len() - 1;
        let more_real = self.levels[li + 1..].iter().any(|l| matches!(l, Level::Field(..)));
        let values_after = self.levels[li + 1..].iter().any(|l| matches!(l, Level::Values));
        let form = if self.rows { self.field_form(f) } else { Form::Outline };
        let _ = k;
        for child in children {
            let item = axis.nodes[child].item;
            path.push(Seg::Item(item));
            let is_leaf_line = last || axis.nodes[child].children.is_empty() && !more_real && !values_after;
            if is_leaf_line {
                self.out.push(line(LineKind::Leaf, path.clone(), child, data, li));
            } else {
                let subtotals_on = more_real && pf.subtotals.is_on();
                let top_values = self.rows
                    && subtotals_on
                    && pf.subtotals.is_plain()
                    && pf.subtotal_top
                    && form != Form::Tabular
                    && !values_after;
                if self.rows && form != Form::Tabular {
                    let mut header = line(LineKind::Header, path.clone(), child, data, li);
                    header.values = top_values;
                    self.out.push(header);
                }
                self.walk(li + 1, child, data, path);
                if subtotals_on && !top_values {
                    self.subtotal_lines(li, child, data, values_after, path);
                }
            }
            if self.rows && pf.blank_row {
                self.out.push(line(LineKind::Blank, path.clone(), child, None, li));
            }
            path.pop();
        }
    }

    fn subtotal_lines(&mut self, li: usize, node: usize, data: Option<usize>, values_after: bool, path: &[Seg]) {
        let f = match self.levels[li] {
            Level::Field(f, _) => f,
            Level::Values => return,
        };
        let pf = &self.calc.table.fields[f];
        let mut funcs: Vec<Option<Func>> = Vec::new();
        if pf.subtotals.automatic {
            funcs.push(None);
        }
        for func in Func::ALL {
            if pf.subtotals.funcs.contains(&func) {
                funcs.push(Some(func));
            }
        }
        for func in funcs {
            let datas: Vec<Option<usize>> = if values_after {
                (0..self.calc.table.data.len()).map(Some).collect()
            } else {
                vec![data]
            };
            for d in datas {
                let mut p = path.to_vec();
                if values_after {
                    p.push(Seg::Data(d.unwrap_or(0)));
                }
                let mut l = line(LineKind::Subtotal, p, node, d, li);
                l.func = func;
                self.out.push(l);
            }
        }
    }
}

// ----------------------------------------------------------------------------------------------------
// Labels.

fn seg_label(calc: &Calc, levels: &[Level], at: usize, seg: Seg) -> String {
    match (seg, levels.get(at)) {
        (Seg::Item(pos), Some(Level::Field(f, _))) => calc.label(*f, pos),
        (Seg::Data(d), _) => calc.table.data[d].name.clone(),
        _ => String::new(),
    }
}

/// The text of a subtotal or grand total line.
fn total_label(calc: &Calc, levels: &[Level], l: &Line) -> String {
    let table = calc.table;
    if l.kind == LineKind::Grand {
        let caption = table.options.grand_total_caption.clone().unwrap_or_else(|| "Grand Total".to_owned());
        return match l.path.first() {
            Some(Seg::Data(d)) => format!("Total {}", table.data[*d].name),
            _ => caption,
        };
    }
    // A subtotal: the item at its level, then what it adds up.
    let item = l.path.get(l.level).map(|s| seg_label(calc, levels, l.level, *s)).unwrap_or_default();
    let word = match (l.func, l.path.get(l.level + 1)) {
        (Some(f), _) => f.subtotal_label().to_owned(),
        (None, Some(Seg::Data(d))) => table.data[*d].name.clone(),
        _ => "Total".to_owned(),
    };
    format!("{item} {word}")
}

fn row_label_cells(calc: &Calc, shape: &Shape, levels: &[Level], lines: &[Line], out: &mut Out) {
    let mut previous: Option<&Line> = None;
    for (i, l) in lines.iter().enumerate() {
        let row = shape.height + i as u32;
        match l.kind {
            LineKind::Blank => {}
            LineKind::Grand => {
                out.put_text(row, 0, &total_label(calc, levels, l), Role::GrandLabel);
            }
            LineKind::Subtotal => {
                let col = shape.label_col.get(l.level).copied().unwrap_or(0);
                out.put_text(row, col, &total_label(calc, levels, l), Role::SubtotalLabel);
            }
            LineKind::Header | LineKind::Leaf => {
                row_item_labels(calc, shape, levels, l, previous, row, out);
                previous = Some(l);
            }
        }
    }
    if shape.data_row_label && lines.len() == 1 {
        if let Some(d) = calc.table.data.first() {
            out.put_text(shape.height, 0, &d.name, Role::RowLabel);
        }
    }
}

fn row_item_labels(
    calc: &Calc,
    shape: &Shape,
    levels: &[Level],
    l: &Line,
    previous: Option<&Line>,
    row: u32,
    out: &mut Out,
) {
    let depth = l.path.len();
    if depth == 0 {
        return;
    }
    // Compact and outline fields label the line at its own level; tabular fields repeat down the
    // columns of every level that changed.
    let own = depth - 1;
    for at in 0..depth {
        let form = form_at(calc, levels, at);
        let want = if at == own && form != Form::Tabular {
            true
        } else if form == Form::Tabular {
            repeat_at(calc, levels, at) || changed(previous, l, at)
        } else {
            form == Form::Outline && repeat_at(calc, levels, at)
        };
        if want {
            let col = shape.label_col.get(at).copied().unwrap_or(0);
            let value = label_value(calc, levels, at, l.path[at]);
            out.put_labelled(row, col, value, Role::RowLabel);
        }
    }
}

fn form_at(calc: &Calc, levels: &[Level], at: usize) -> Form {
    match levels.get(at) {
        Some(Level::Field(f, _)) => calc.table.fields[*f].form,
        _ => calc.table.options.form,
    }
}

fn repeat_at(calc: &Calc, levels: &[Level], at: usize) -> bool {
    match levels.get(at) {
        Some(Level::Field(f, _)) => calc.table.fields[*f].repeat_labels,
        _ => false,
    }
}

/// Whether a line's step differs from the previous line's, or any step before it does.
fn changed(previous: Option<&Line>, l: &Line, at: usize) -> bool {
    match previous {
        None => true,
        Some(p) => p.path.len() <= at || p.path[..=at] != l.path[..=at],
    }
}

/// The value of an item's label: a number or a date stays a number with its format.
fn label_value(calc: &Calc, levels: &[Level], at: usize, seg: Seg) -> (Value, Option<String>) {
    match (seg, levels.get(at)) {
        (Seg::Item(pos), Some(Level::Field(f, _))) => match calc.item_at(*f, pos) {
            super::cache::CacheValue::Blank => (Value::text("(blank)"), None),
            v @ (super::cache::CacheValue::Number(_) | super::cache::CacheValue::Date(_)) => {
                (v.to_value(), Some(calc.field_format(*f)))
            }
            v => (v.to_value(), None),
        },
        _ => (Value::text(seg_label(calc, levels, at, seg)), None),
    }
}

// ----------------------------------------------------------------------------------------------------
// Captions and column headers.

fn corner_cells(calc: &Calc, shape: &Shape, rows: &[Level], cols: &[Level], out: &mut Out) {
    let table = calc.table;
    let opts = &table.options;
    let last_header = shape.height - 1;
    if !opts.show_headers {
        return;
    }
    let n_data = table.data.len();
    // Column captions.
    if shape.has_real_cols {
        if opts.form == Form::Compact {
            let caption = opts.col_header_caption.clone().unwrap_or_else(|| "Column Labels".to_owned());
            out.put_text(0, shape.width, &caption, Role::Caption);
        } else {
            for (i, level) in cols.iter().enumerate() {
                let text = match level {
                    Level::Field(f, _) => table.caption(calc.cache, *f),
                    Level::Values => opts.data_caption.clone(),
                };
                out.put_text(0, shape.width + i as u32, &text, Role::Caption);
            }
        }
        if n_data == 1 && !rows.is_empty() {
            out.put_text(0, 0, &table.data[0].name, Role::Caption);
        }
    } else if n_data == 1 {
        out.put_text(0, shape.width, &table.data[0].name, Role::Caption);
    }
    // Row captions.
    let mut written: Vec<u32> = Vec::new();
    for (i, level) in rows.iter().enumerate() {
        let col = shape.label_col[i];
        let form = form_at(calc, rows, i);
        let text = if form == Form::Compact {
            if written.contains(&col) {
                continue;
            }
            match level {
                Level::Values if rows.len() == 1 => opts.data_caption.clone(),
                _ => opts.row_header_caption.clone().unwrap_or_else(|| "Row Labels".to_owned()),
            }
        } else {
            match level {
                Level::Field(f, _) => table.caption(calc.cache, *f),
                Level::Values => opts.data_caption.clone(),
            }
        };
        written.push(col);
        out.put_text(last_header, col, &text, Role::Caption);
    }
}

fn col_header_cells(calc: &Calc, shape: &Shape, levels: &[Level], lines: &[Line], out: &mut Out) {
    if !shape.has_real_cols {
        // Only the value fields: their names are the header.
        if levels.iter().any(|l| matches!(l, Level::Values)) {
            for (j, l) in lines.iter().enumerate() {
                if let Some(Seg::Data(d)) = l.path.last() {
                    out.put_text(0, shape.width + j as u32, &calc.table.data[*d].name, Role::ColLabel);
                }
            }
        }
        return;
    }
    let mut previous: Option<&Line> = None;
    for (j, l) in lines.iter().enumerate() {
        let col = shape.width + j as u32;
        match l.kind {
            LineKind::Leaf => {
                for at in 0..l.path.len() {
                    let first = at == l.path.len() - 1 || changed_col(previous, l, at);
                    if first {
                        let value = label_value(calc, levels, at, l.path[at]);
                        out.put_labelled(1 + at as u32, col, value, Role::ColLabel);
                    }
                }
                previous = Some(l);
            }
            LineKind::Subtotal => {
                let text = total_label(calc, levels, l);
                out.put_text(1 + l.level as u32, col, &text, Role::SubtotalLabel);
                previous = Some(l);
            }
            LineKind::Grand => {
                out.put_text(1, col, &total_label(calc, levels, l), Role::GrandLabel);
                previous = Some(l);
            }
            _ => {}
        }
    }
}

fn changed_col(previous: Option<&Line>, l: &Line, at: usize) -> bool {
    match previous {
        Some(p) if p.kind == LineKind::Leaf => p.path.len() <= at || p.path[..=at] != l.path[..=at],
        _ => true,
    }
}

// ----------------------------------------------------------------------------------------------------
// Figures.

fn data_cells(calc: &Calc, shape: &Shape, n_data: usize, rows: &[Line], cols: &[Line], out: &mut Out) {
    if n_data == 0 {
        return;
    }
    let table = calc.table;
    for (i, rl) in rows.iter().enumerate() {
        if !rl.values {
            continue;
        }
        for (j, cl) in cols.iter().enumerate() {
            if !cl.values {
                continue;
            }
            let d = rl.data.or(cl.data).or(if n_data == 1 { Some(0) } else { None });
            let Some(d) = d else { continue };
            let func = rl.func.or(cl.func);
            let v = calc.shown(rl.node, cl.node, d, func);
            let role = if rl.kind == LineKind::Grand || cl.kind == LineKind::Grand {
                Role::GrandData
            } else if rl.is_total() || cl.is_total() {
                Role::SubtotalData
            } else {
                Role::Data
            };
            let value = match v {
                CellVal::Empty => match &table.options.missing_caption {
                    Some(text) if !text.is_empty() => Value::text(text),
                    _ => continue,
                },
                CellVal::Num(n) => Value::Number(n),
                CellVal::Err(e) => match &table.options.error_caption {
                    Some(text) => Value::text(text),
                    None => Value::Error(e),
                },
            };
            let format = table.data[d].num_fmt.clone();
            out.cells.push(LaidCell {
                row: shape.height + i as u32,
                col: shape.width + j as u32,
                value,
                role,
                format,
                indent: 0,
            });
        }
    }
}

// ----------------------------------------------------------------------------------------------------
// The filter area.

/// Move the table down by the filter area and add its cells.
fn page_cells(calc: &Calc, laid: &mut Laid) {
    let table = calc.table;
    if table.pages.is_empty() {
        return;
    }
    let page_rows = table.pages.len() as u32 + 1;
    for cell in &mut laid.cells {
        cell.row += page_rows;
    }
    for (k, page) in table.pages.iter().enumerate() {
        let name = table.caption(calc.cache, page.field);
        laid.cells.push(LaidCell {
            row: k as u32,
            col: 0,
            value: Value::text(&name),
            role: Role::PageField,
            format: None,
            indent: 0,
        });
        laid.cells.push(LaidCell {
            row: k as u32,
            col: 1,
            value: Value::text(&calc.page_text(page)),
            role: Role::PageItem,
            format: None,
            indent: 0,
        });
    }
    laid.page_rows = page_rows;
    laid.rows += page_rows;
}

impl Laid {
    /// The columns the filter area needs.
    pub fn page_cols(&self) -> u32 {
        if self.page_rows > 0 {
            2
        } else {
            0
        }
    }
}
