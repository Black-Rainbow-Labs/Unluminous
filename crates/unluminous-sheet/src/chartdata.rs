//! The cells a chart reads: resolving a series formula to values, laying a range out as series the way
//! Excel does when a chart is made from a selection, and choosing the charts Excel recommends.

use crate::chartspec::ChartType;
use crate::coord::{column_name, CellRef, Range};
use crate::model::{ChartSeries, Workbook};
use crate::numfmt::Format;
use crate::value::Value;

/// One rectangle of cells a formula names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefPart {
    pub sheet: usize,
    pub range: Range,
}

/// Split a formula at the top level commas of a union: `(A!$B$1,A!$C$1)`.
fn split_union(formula: &str) -> Vec<String> {
    let inner = formula.strip_prefix('(').and_then(|f| f.strip_suffix(')')).unwrap_or(formula);
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut quoted = false;
    let mut current = String::new();
    for c in inner.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                current.push(c);
            }
            '(' if !quoted => {
                depth += 1;
                current.push(c);
            }
            ')' if !quoted => {
                depth -= 1;
                current.push(c);
            }
            ',' if !quoted && depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    parts
}

/// The sheet name and area of one reference, with the sheet's quotes taken off.
fn split_sheet(part: &str) -> (Option<String>, &str) {
    match part.rfind('!') {
        Some(at) => {
            let name = part[..at].trim_matches('\'').replace("''", "'");
            (Some(name), &part[at + 1..])
        }
        None => (None, part),
    }
}

/// A whole column or row reference (`A:A`, `3:5`) as a range limited to the sheet's used cells.
fn area_range(book: &Workbook, sheet: usize, area: &str) -> Option<Range> {
    let area = area.replace('$', "");
    if let Some(range) = Range::parse(&area) {
        return Some(range);
    }
    let used = book.sheets[sheet].used_range()?;
    let (a, b) = area.split_once(':').unwrap_or((&area, &area));
    if let (Some(x), Some(y)) = (crate::coord::column_number(a), crate::coord::column_number(b)) {
        return Some(Range::new(CellRef::new(0, x.min(y)), CellRef::new(used.last.row, x.max(y))));
    }
    if let (Ok(x), Ok(y)) = (a.parse::<u32>(), b.parse::<u32>()) {
        return Some(Range::new(
            CellRef::new(x.min(y) - 1, 0),
            CellRef::new(x.max(y) - 1, used.last.col),
        ));
    }
    None
}

/// The rectangles a series formula names, or `None` when it is not a reference.
pub fn parse_refs(book: &Workbook, current: usize, formula: &str) -> Option<Vec<RefPart>> {
    let formula = formula.trim().trim_start_matches('=');
    if formula.is_empty() || formula.starts_with('{') || formula.starts_with('"') {
        return None;
    }
    let mut out = Vec::new();
    for part in split_union(formula) {
        let (name, area) = split_sheet(part.trim());
        let sheet = match name {
            Some(n) => book.sheet_index(&n)?,
            None => current,
        };
        match area_range(book, sheet, area) {
            Some(range) => out.push(RefPart { sheet, range }),
            None => {
                // A defined name: the range it names.
                let named = book.names.iter().find(|n| n.name.eq_ignore_ascii_case(area))?;
                out.extend(parse_refs(book, current, &named.formula)?);
            }
        }
    }
    Some(out)
}

/// Whether a formula is a reference to cells, which Excel needs for a series' values.
pub fn is_reference(book: &Workbook, current: usize, formula: &str) -> bool {
    parse_refs(book, current, formula).is_some()
}

/// The cells a formula gives, in order.
#[derive(Debug, Clone, Default)]
pub struct Resolved {
    pub values: Vec<Value>,
    /// Whether each cell is in a hidden row or column.
    pub hidden: Vec<bool>,
    /// Whether the cells are formatted as dates.
    pub dates: bool,
    /// The rows and columns of the first rectangle.
    pub shape: (u32, u32),
    /// Each cell as its format shows it, which is what a category label or a data label says.
    pub texts: Vec<String>,
    /// The number format of the first number cell, which a chart's axis follows when it is linked to the source.
    pub format: String,
}

fn array_constant(formula: &str) -> Option<Vec<Value>> {
    let inner = formula.trim().strip_prefix('{')?.strip_suffix('}')?;
    Some(
        inner
            .split([',', ';'])
            .map(|item| {
                let item = item.trim();
                match item.parse::<f64>() {
                    Ok(n) => Value::Number(n),
                    Err(_) => Value::Text(item.trim_matches('"').replace("\"\"", "\"").into()),
                }
            })
            .collect(),
    )
}

/// Whether a column of a sheet is hidden.
fn column_hidden(book: &Workbook, sheet: usize, col: u32) -> bool {
    book.sheets[sheet].columns.iter().any(|c| c.hidden && c.first <= col && col <= c.last)
}

/// The values a formula gives: the cells of its references, or an array written in the formula.
pub fn resolve(book: &Workbook, current: usize, formula: &str) -> Resolved {
    if let Some(values) = array_constant(formula) {
        let hidden = vec![false; values.len()];
        let texts = values.iter().map(Value::to_text).collect();
        return Resolved {
            shape: (1, values.len() as u32),
            values,
            hidden,
            dates: false,
            texts,
            format: String::new(),
        };
    }
    let Some(parts) = parse_refs(book, current, formula) else { return Resolved::default() };
    let mut out = Resolved::default();
    for (i, part) in parts.iter().enumerate() {
        let sheet = &book.sheets[part.sheet];
        let used = sheet.used_range();
        let last_row = used.map_or(part.range.first.row, |u| u.last.row).min(part.range.last.row);
        let last_col = used.map_or(part.range.first.col, |u| u.last.col).min(part.range.last.col);
        let range = Range::new(
            part.range.first,
            CellRef::new(last_row.max(part.range.first.row), last_col.max(part.range.first.col)),
        );
        if i == 0 {
            out.shape = (range.rows(), range.cols());
        }
        for at in range.cells() {
            let cell = sheet.cell(at);
            let value = sheet.value_at(at);
            let mut text = value.to_text();
            if let (Some(cell), Value::Number(_)) = (cell, &value) {
                let code = book.styles.format_code(cell.style);
                let format = Format::parse(&code);
                if !out.dates {
                    out.dates = format.is_date();
                }
                text = format.render(&value, book.date1904, 11).text;
                if out.format.is_empty() {
                    out.format = code;
                }
            }
            out.texts.push(text);
            let row_hidden = sheet.rows.get(&at.row).is_some_and(|r| r.hidden);
            out.hidden.push(row_hidden || column_hidden(book, part.sheet, at.col));
            out.values.push(value);
        }
    }
    out
}

/// The text of a cell as a category label.
pub fn label(book: &Workbook, sheet: usize, at: CellRef) -> String {
    let s = &book.sheets[sheet];
    let value = s.value_at(at);
    match (&value, s.cell(at)) {
        (Value::Number(_), Some(cell)) => {
            Format::parse(&book.styles.format_code(cell.style))
                .render(&value, book.date1904, 11)
                .text
        }
        _ => value.to_text(),
    }
}

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Number(_))
}

fn is_text(v: &Value) -> bool {
    matches!(v, Value::Text(_))
}

fn absolute(sheet: &str, range: Range) -> String {
    let name = if sheet.chars().all(|c| c.is_alphanumeric() || c == '_') && !sheet.is_empty() {
        sheet.to_owned()
    } else {
        format!("'{}'", sheet.replace('\'', "''"))
    };
    let p = |c: CellRef| format!("${}${}", column_name(c.col), c.row + 1);
    if range.first == range.last {
        format!("{name}!{}", p(range.first))
    } else {
        format!("{name}!{}:{}", p(range.first), p(range.last))
    }
}

/// How the series of a chart made from a range lie in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotBy {
    Rows,
    Columns,
}

/// What Excel finds in a selection: where the names, the categories and the numbers are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// The first row holds series names.
    pub header_row: bool,
    /// The first column holds categories.
    pub label_column: bool,
    pub plot_by: PlotBy,
    /// The categories are dates, which makes the category axis a date axis.
    pub dates: bool,
}

/// Work out the layout of a selection as Excel does when it makes a chart from it.
pub fn shape_of(book: &Workbook, sheet: usize, range: Range) -> Shape {
    let s = &book.sheets[sheet];
    let value = |r: u32, c: u32| s.value_at(CellRef::new(r, c));
    let top_left_empty = matches!(value(range.first.row, range.first.col), Value::Empty);
    let rows = range.rows();
    let cols = range.cols();
    let date_cell = |r: u32, c: u32| {
        s.cell(CellRef::new(r, c)).is_some_and(|cell| {
            is_number(&cell.value) && Format::parse(&book.styles.format_code(cell.style)).is_date()
        })
    };
    // The first column holds categories when it is text, empty at the top, or dates.
    let first_col: Vec<Value> =
        (range.first.row..=range.last.row).map(|r| value(r, range.first.col)).collect();
    let body_first_col = if rows > 1 { &first_col[1..] } else { &first_col[..] };
    let text_column = !body_first_col.is_empty() && body_first_col.iter().all(is_text);
    let date_column = cols > 1
        && rows > 1
        && (range.first.row + 1..=range.last.row).all(|r| date_cell(r, range.first.col));
    let label_column = cols > 1 && (top_left_empty || text_column || date_column);
    // The first row holds series names when it is text over numbers (or the corner is empty).
    let first_row: Vec<Value> =
        (range.first.col..=range.last.col).map(|c| value(range.first.row, c)).collect();
    let from = usize::from(label_column);
    let row_cells = &first_row[from.min(first_row.len())..];
    let header_row = rows > 1
        && (top_left_empty
            || (!row_cells.is_empty()
                && row_cells.iter().all(|v| is_text(v) || matches!(v, Value::Empty))));
    let data_rows = rows - u32::from(header_row);
    let data_cols = cols - u32::from(label_column);
    // Measured against Excel: a wide block is plotted by rows only when its corner cell is empty, which is how
    // a block with names across the top and categories down the side is written; otherwise by columns.
    let plot_by =
        if data_rows < data_cols && top_left_empty { PlotBy::Rows } else { PlotBy::Columns };
    let dates = label_column && date_column;
    Shape { header_row, label_column, plot_by, dates }
}

/// Lay a selection out as series: names, categories and values formulas, by columns or by rows.
pub fn layout_series(
    book: &Workbook,
    sheet: usize,
    range: Range,
    shape: Shape,
) -> Vec<ChartSeries> {
    let name = &book.sheets[sheet].name;
    let top = range.first.row + u32::from(shape.header_row);
    let left = range.first.col + u32::from(shape.label_column);
    match shape.plot_by {
        PlotBy::Columns => series_by_columns(name, range, shape, top, left),
        PlotBy::Rows => series_by_rows(name, range, shape, top, left),
    }
}

/// One series for each column of the selection, after the label column when there is one.
fn series_by_columns(
    name: &str,
    range: Range,
    shape: Shape,
    top: u32,
    left: u32,
) -> Vec<ChartSeries> {
    let categories = shape.label_column.then(|| {
        absolute(
            name,
            Range::new(
                CellRef::new(top, range.first.col),
                CellRef::new(range.last.row, range.first.col),
            ),
        )
    });
    let mut out = Vec::new();
    for col in left..=range.last.col {
        let values =
            absolute(name, Range::new(CellRef::new(top, col), CellRef::new(range.last.row, col)));
        let title = shape
            .header_row
            .then(|| absolute(name, Range::cell(CellRef::new(range.first.row, col))));
        out.push(ChartSeries::new(title, categories.clone(), values));
    }
    out
}

/// One series for each row of the selection, after the header row when there is one.
fn series_by_rows(name: &str, range: Range, shape: Shape, top: u32, left: u32) -> Vec<ChartSeries> {
    // By rows, the first row's cells are the categories and the first column's are the names.
    let categories = shape.header_row.then(|| {
        absolute(
            name,
            Range::new(
                CellRef::new(range.first.row, left),
                CellRef::new(range.first.row, range.last.col),
            ),
        )
    });
    let mut out = Vec::new();
    for row in top..=range.last.row {
        let values =
            absolute(name, Range::new(CellRef::new(row, left), CellRef::new(row, range.last.col)));
        let title = shape
            .label_column
            .then(|| absolute(name, Range::cell(CellRef::new(row, range.first.col))));
        out.push(ChartSeries::new(title, categories.clone(), values));
    }
    out
}

/// Lay a selection of several areas out as series, as Excel does for `B4:B9,D4:D9`: when every area spans
/// the same rows, the areas' columns are read as one block, so a text column first is the categories and
/// each column after it is a series named by its top cell. `None` when the areas do not line up that way.
pub fn layout_areas(
    book: &Workbook,
    sheet: usize,
    areas: &[Range],
) -> Option<(Vec<ChartSeries>, bool)> {
    let first = *areas.first()?;
    if areas.iter().any(|a| a.first.row != first.first.row || a.last.row != first.last.row) {
        return None;
    }
    let s = &book.sheets[sheet];
    let name = &s.name;
    let columns: Vec<u32> = areas.iter().flat_map(|a| a.first.col..=a.last.col).collect();
    let (top, bottom) = (first.first.row, first.last.row);
    let value = |r: u32, c: u32| s.value_at(CellRef::new(r, c));
    let label = columns.len() > 1 && {
        let c = columns[0];
        matches!(value(top, c), Value::Empty) || (top + 1..=bottom).all(|r| is_text(&value(r, c)))
    };
    let series_columns = &columns[usize::from(label)..];
    let header = bottom > top
        && (matches!(value(top, columns[0]), Value::Empty) && label
            || series_columns
                .iter()
                .all(|&c| matches!(value(top, c), Value::Text(_) | Value::Empty)));
    let body = top + u32::from(header);
    let column = |c: u32| Range::new(CellRef::new(body, c), CellRef::new(bottom, c));
    let categories = label.then(|| absolute(name, column(columns[0])));
    let series = series_columns
        .iter()
        .map(|&c| {
            let title = header.then(|| absolute(name, Range::cell(CellRef::new(top, c))));
            ChartSeries::new(title, categories.clone(), absolute(name, column(c)))
        })
        .collect();
    let dates = label
        && (body..=bottom).all(|r| {
            s.cell(CellRef::new(r, columns[0])).is_some_and(|cell| {
                is_number(&cell.value)
                    && Format::parse(&book.styles.format_code(cell.style)).is_date()
            })
        });
    Some((series, dates))
}

/// The charts Excel recommends for a selection, best first. There are five for most data.
pub fn recommend(book: &Workbook, sheet: usize, range: Range) -> Vec<ChartType> {
    use ChartType as T;
    let shape = shape_of(book, sheet, range);
    let series = layout_series(book, sheet, range, shape);
    let count = series.len();
    let points = match shape.plot_by {
        PlotBy::Columns => range.rows() - u32::from(shape.header_row),
        PlotBy::Rows => range.cols() - u32::from(shape.label_column),
    };
    let numeric_pair = !shape.label_column && count == 2 && range.cols() == 2 && !shape.header_row
        || (!shape.label_column && range.cols() == 2 && shape.header_row);
    if shape.dates {
        return vec![T::LineMarkers, T::ColumnClustered, T::Line, T::AreaStacked, T::ScatterLines];
    }
    if numeric_pair {
        return vec![T::ScatterMarkers, T::ColumnClustered, T::Line, T::BarClustered, T::Area];
    }
    if count == 1 {
        let mut out = vec![T::ColumnClustered, T::Line, T::BarClustered, T::Area];
        if points <= 8 {
            out.insert(1, T::Pie);
        } else {
            out.push(T::Pie);
        }
        out.truncate(5);
        return out;
    }
    if count <= 3 {
        return vec![
            T::ColumnClustered,
            T::LineMarkers,
            T::ColumnStacked,
            T::BarClustered,
            T::AreaStacked,
        ];
    }
    vec![T::ColumnClustered, T::LineMarkers, T::ColumnStacked, T::AreaStacked, T::BarClustered]
}

/// Lay a selection out for a scatter or bubble chart: the first column holds X, and each other column is a
/// Y series; a bubble chart takes its sizes from the column after each Y, or draws equal bubbles.
pub fn layout_xy(
    book: &Workbook,
    sheet: usize,
    range: Range,
    header_row: bool,
    bubble: bool,
) -> Vec<ChartSeries> {
    let name = &book.sheets[sheet].name;
    let top = range.first.row + u32::from(header_row);
    let column = |col: u32| {
        absolute(name, Range::new(CellRef::new(top, col), CellRef::new(range.last.row, col)))
    };
    let title = |col: u32| {
        header_row.then(|| absolute(name, Range::cell(CellRef::new(range.first.row, col))))
    };
    if range.cols() == 1 {
        return vec![ChartSeries::new(title(range.first.col), None, column(range.first.col))];
    }
    let x = column(range.first.col);
    let mut out = Vec::new();
    let mut col = range.first.col + 1;
    while col <= range.last.col {
        let mut s = ChartSeries::new(title(col), Some(x.clone()), column(col));
        col += 1;
        if bubble {
            let count = range.last.row + 1 - top;
            s.sizes = Some(if col <= range.last.col {
                let sizes = column(col);
                col += 1;
                sizes
            } else {
                format!("{{{}}}", vec!["1"; count as usize].join(","))
            });
        }
        out.push(s);
    }
    out
}
