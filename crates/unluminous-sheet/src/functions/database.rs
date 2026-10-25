//! The database functions: DSUM, DCOUNT, DGET and their kin, over a range whose first row names the
//! fields.
//!
//! A criteria range has a header row naming fields and one or more rows under it. The conditions in
//! one row must all hold and a record is chosen when any row holds, so rows are ORed and columns
//! ANDed. Text with no operator in front of it chooses cells that begin with it, `=Ap` chooses the cell
//! `Ap` exactly, and the wildcards and the comparison operators work as they do in COUNTIF. A column
//! whose header names no field has no field to compare with, so its cell is read as a yes or no: a
//! formula there is worked out for each record with its references moved down to that record, the way
//! Excel does it, and any other cell says yes when it holds TRUE, a number that is not 0 or the text
//! TRUE.

use super::criteria::Criterion;
use super::stats::{mean, population_variance, sample_variance};
use super::{r, Function};
use crate::coord::CellRef;
use crate::eval::{Area, Ctx, Val};
use crate::formula::{shifted, Expr, Formula};
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "DAVERAGE",
        3,
        3,
        "database, field, criteria",
        "The mean of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Average),
    ),
    r(
        "DCOUNT",
        3,
        3,
        "database, [field], criteria",
        "How many numbers the field has in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Count),
    ),
    r(
        "DCOUNTA",
        3,
        3,
        "database, [field], criteria",
        "How many values the field has in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::CountAll),
    ),
    r(
        "DGET",
        3,
        3,
        "database, field, criteria",
        "The field of the one record that meets the criteria.",
        |ctx, a| database(ctx, a, Summary::Get),
    ),
    r(
        "DMAX",
        3,
        3,
        "database, field, criteria",
        "The largest value of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Max),
    ),
    r(
        "DMIN",
        3,
        3,
        "database, field, criteria",
        "The smallest value of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Min),
    ),
    r(
        "DPRODUCT",
        3,
        3,
        "database, field, criteria",
        "The product of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Product),
    ),
    r(
        "DSTDEV",
        3,
        3,
        "database, field, criteria",
        "The sample standard deviation of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::StDev),
    ),
    r(
        "DSTDEVP",
        3,
        3,
        "database, field, criteria",
        "The population standard deviation of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::StDevP),
    ),
    r(
        "DSUM",
        3,
        3,
        "database, field, criteria",
        "The sum of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Sum),
    ),
    r(
        "DVAR",
        3,
        3,
        "database, field, criteria",
        "The sample variance of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::Var),
    ),
    r(
        "DVARP",
        3,
        3,
        "database, field, criteria",
        "The population variance of the field in the records that meet the criteria.",
        |ctx, a| database(ctx, a, Summary::VarP),
    ),
];

/// What a database function does with the field of the records that were chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Summary {
    Average,
    Count,
    CountAll,
    Get,
    Max,
    Min,
    Product,
    StDev,
    StDevP,
    Sum,
    Var,
    VarP,
}

/// One condition in a criteria row.
enum Condition {
    /// The record's cell in this database column must meet the criterion.
    Field(usize, Criterion),
    /// A yes or no that does not depend on the record.
    Fixed(bool),
    /// A formula worked out for each record.
    Computed(Formula),
}

/// The cells of a database and where its records start.
struct Table {
    cells: Array,
}

impl Table {
    fn records(&self) -> usize {
        self.cells.rows - 1
    }

    fn header(&self, column: usize) -> &Value {
        self.cells.get(0, column)
    }

    fn cell(&self, record: usize, column: usize) -> &Value {
        self.cells.get(record + 1, column)
    }
}

/// A reference argument as the area it names. Anything that is not a reference to one place is
/// `#VALUE!`, and an error is passed on.
fn area_of(ctx: &mut Ctx, e: &Expr) -> Result<Area, ErrorKind> {
    match ctx.eval(e) {
        Val::Ref(area) if !area.is_3d() => Ok(area),
        Val::Value(Value::Error(e)) => Err(e),
        _ => Err(ErrorKind::Value),
    }
}

/// The values of an area with at least two rows, a header and a record.
fn table_of(ctx: &mut Ctx, area: Area) -> Result<Array, ErrorKind> {
    let cells = ctx.array(Val::Ref(area))?;
    if cells.rows < 2 {
        return Err(ErrorKind::Value);
    }
    Ok(cells)
}

/// The text of a header cell, for comparing field names without regard to case.
fn name_of(v: &Value) -> Option<String> {
    match v {
        Value::Empty => None,
        other => Some(other.to_text().to_lowercase()),
    }
}

/// The database column a header names, if any.
fn column_named(table: &Table, header: &Value) -> Option<usize> {
    let wanted = name_of(header)?;
    (0..table.cells.cols).find(|&c| name_of(table.header(c)).as_deref() == Some(wanted.as_str()))
}

/// The database column a field argument picks: a name, or a position counted from 1. `None` is a
/// field that was left out, which is allowed for the two counts.
fn field_column(
    ctx: &mut Ctx,
    args: &[Expr],
    table: &Table,
    summary: Summary,
) -> Result<Option<usize>, ErrorKind> {
    let counts = matches!(summary, Summary::Count | Summary::CountAll);
    if matches!(args[1], Expr::Missing) {
        return if counts { Ok(None) } else { Err(ErrorKind::Value) };
    }
    let v = ctx.eval(&args[1]);
    let v = ctx.single(v);
    let position = match &v {
        Value::Error(e) => return Err(*e),
        Value::Text(_) => return column_named(table, &v).map(Some).ok_or(ErrorKind::Value),
        Value::Number(n) => n.trunc(),
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Empty => return Err(ErrorKind::Value),
    };
    if position < 1.0 || position > table.cells.cols as f64 {
        return Err(ErrorKind::Value);
    }
    Ok(Some(position as usize - 1))
}

/// The pattern of a criterion with a tilde that escapes nothing made into a literal tilde, because
/// only `*`, `?` and `~` can follow one.
fn escape_loose_tildes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '~' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('*') | Some('?') | Some('~') => {
                out.push('~');
                out.push(chars.next().unwrap_or('~'));
            }
            _ => out.push_str("~~"),
        }
    }
    out
}

/// A criteria cell as a criterion: text with no operator chooses what begins with it.
fn criterion_of(cell: &Value, date1904: bool) -> Criterion {
    let Value::Text(text) = cell else { return Criterion::parse(cell, date1904) };
    let operators = ["<>", "<=", ">=", "<", ">", "="];
    let (operator, rest) = match operators.iter().find(|o| text.starts_with(**o)) {
        Some(o) => (*o, &text[o.len()..]),
        None => ("", &text[..]),
    };
    let mut pattern = escape_loose_tildes(rest);
    let plain = operator.is_empty()
        && !rest.is_empty()
        && !rest.eq_ignore_ascii_case("true")
        && !rest.eq_ignore_ascii_case("false")
        && crate::value::ErrorKind::parse(rest).is_none()
        && crate::input::number_from_text(rest, date1904).is_none();
    if plain {
        pattern.push('*');
    }
    Criterion::parse(&Value::text(format!("{operator}{pattern}")), date1904)
}

/// A cell read as a yes or no.
fn truth(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0,
        Value::Text(t) => t.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// The condition one criteria cell makes, or none when the cell is blank.
fn condition_at(
    ctx: &Ctx,
    area: &Area,
    table: &Table,
    cells: &Array,
    row: usize,
    column: usize,
) -> Option<Condition> {
    let value = cells.get(row, column);
    let at = CellRef::new(area.range.first.row + row as u32, area.range.first.col + column as u32);
    let formula =
        ctx.sheet_ref(area.sheet).and_then(|s| s.cell(at)).and_then(|c| c.formula.clone());
    let field = column_named(table, cells.get(0, column));
    if let Some(field) = field {
        return match value {
            Value::Empty => None,
            Value::Text(t) if t.is_empty() => None,
            _ => Some(Condition::Field(field, criterion_of(value, ctx.date1904()))),
        };
    }
    match formula {
        Some(f) => Some(Condition::Computed(f)),
        None if value.is_empty() => None,
        None => Some(Condition::Fixed(truth(value))),
    }
}

/// Every criteria row as its list of conditions, ready to test records against.
fn criteria_rows(ctx: &Ctx, area: &Area, table: &Table, cells: &Array) -> Vec<Vec<Condition>> {
    (1..cells.rows)
        .map(|row| {
            (0..cells.cols)
                .filter_map(|col| condition_at(ctx, area, table, cells, row, col))
                .collect()
        })
        .collect()
}

/// Whether one condition holds for a record.
fn holds(ctx: &mut Ctx, condition: &Condition, table: &Table, record: usize) -> bool {
    match condition {
        Condition::Field(column, criterion) => criterion.matches(table.cell(record, *column)),
        Condition::Fixed(yes) => *yes,
        Condition::Computed(formula) => {
            let moved = shifted(&formula.expr, record as i64, 0);
            let v = ctx.eval(&moved);
            truth(&ctx.single(v))
        }
    }
}

/// The records that meet the criteria, as their positions among the records.
fn chosen_records(ctx: &mut Ctx, table: &Table, rows: &[Vec<Condition>]) -> Vec<usize> {
    (0..table.records())
        .filter(|&record| rows.iter().any(|row| row.iter().all(|c| holds(ctx, c, table, record))))
        .collect()
}

/// The numbers among the chosen cells; an error among them is the answer.
fn numbers_among(cells: &[Value]) -> Result<Vec<f64>, ErrorKind> {
    let mut numbers = Vec::new();
    for v in cells {
        match v {
            Value::Number(n) => numbers.push(*n),
            Value::Error(e) => return Err(*e),
            _ => {}
        }
    }
    Ok(numbers)
}

/// The summary of the numbers in the chosen cells.
fn summarise(cells: &[Value], summary: Summary) -> Result<Value, ErrorKind> {
    let numbers = numbers_among(cells)?;
    let n = numbers.len();
    let result = match summary {
        Summary::Sum => numbers.iter().sum(),
        Summary::Average => mean(&numbers).ok_or(ErrorKind::Div0)?,
        Summary::Max => numbers.iter().copied().reduce(f64::max).unwrap_or(0.0),
        Summary::Min => numbers.iter().copied().reduce(f64::min).unwrap_or(0.0),
        Summary::Product if n == 0 => 0.0,
        Summary::Product => numbers.iter().product(),
        Summary::Var => sample_variance(&numbers).ok_or(ErrorKind::Div0)?,
        Summary::StDev => sample_variance(&numbers).ok_or(ErrorKind::Div0)?.sqrt(),
        Summary::VarP => population_variance(&numbers).ok_or(ErrorKind::Div0)?,
        Summary::StDevP => population_variance(&numbers).ok_or(ErrorKind::Div0)?.sqrt(),
        Summary::Count | Summary::CountAll | Summary::Get => unreachable!("handled before"),
    };
    if result.is_finite() {
        Ok(Value::Number(result))
    } else {
        Err(ErrorKind::Num)
    }
}

/// The one chosen cell, which must be the only record and must not be empty.
fn the_only(cells: &[Value]) -> Result<Value, ErrorKind> {
    match cells {
        [] => Err(ErrorKind::Value),
        [one] if one.is_empty() => Err(ErrorKind::Value),
        [one] => Ok(one.clone()),
        _ => Err(ErrorKind::Num),
    }
}

fn database(ctx: &mut Ctx, args: &[Expr], summary: Summary) -> Val {
    // Criteria written as an array is spread over, as Excel's dynamic arrays do, and each element of it
    // is a value where a range is needed, so each answer is #VALUE!.
    if let Val::Array(array) = ctx.eval(&args[2]) {
        if array.data.len() > 1 {
            let errors = vec![Value::Error(ErrorKind::Value); array.data.len()];
            return Val::Array(crate::value::Array::new(array.rows, array.cols, errors));
        }
    }
    match database_value(ctx, args, summary) {
        Ok(v) => Val::Value(v),
        Err(e) => Val::error(e),
    }
}

fn database_value(ctx: &mut Ctx, args: &[Expr], summary: Summary) -> Result<Value, ErrorKind> {
    let db_area = area_of(ctx, &args[0])?;
    let table = Table { cells: table_of(ctx, db_area)? };
    let column = field_column(ctx, args, &table, summary)?;
    let crit_area = area_of(ctx, &args[2])?;
    let criteria = table_of(ctx, crit_area)?;
    let rows = criteria_rows(ctx, &crit_area, &table, &criteria);
    let records = chosen_records(ctx, &table, &rows);
    let Some(column) = column else { return Ok(Value::Number(records.len() as f64)) };
    let cells: Vec<Value> = records.iter().map(|&r| table.cell(r, column).clone()).collect();
    match summary {
        Summary::Count => {
            Ok(Value::Number(cells.iter().filter(|v| matches!(v, Value::Number(_))).count() as f64))
        }
        Summary::CountAll => {
            Ok(Value::Number(cells.iter().filter(|v| !v.is_empty()).count() as f64))
        }
        Summary::Get => the_only(&cells),
        _ => summarise(&cells, summary),
    }
}
