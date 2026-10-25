//! GROUPBY, PIVOTBY, PERCENTOF and TRIMRANGE.
//!
//! GROUPBY and PIVOTBY read their records (the rows of the row fields, the column fields and the values),
//! group them, and apply a function to each group. A function is a name such as `SUM`, a LAMBDA of one
//! parameter (the values of the group) or two (the group and all the values it is compared with, which is
//! what PERCENTOF reads), or `HSTACK` of several of them for several columns of results.
//!
//! What was measured against Excel 16 and is reproduced here:
//!
//! * Keys sort numbers, then text without regard to case, then logicals, and a blank cell last. Groups are
//!   sorted among their siblings, so a sort on a value orders each level by the group's own value, with ties
//!   in ascending key order. A sort column that stands for a field below the level being sorted is skipped.
//! * A sort list of more than one column is accepted only when every column is a row field.
//! * The headers argument: 0 none, 1 the first row is headers and hidden, 2 headers are made up, 3 the
//!   first row is headers and shown. Left out, the first row is taken as headers when it is all text above
//!   numbers, and it is shown only when several functions give a row of function names above it.
//! * Totals: 1 one at the end, -1 one at the start, and 2 or more (or their negatives) add a subtotal after
//!   (or before) each group down to that many fields. The label is `Total`, or `Grand Total` with subtotals.
//! * A pivot with several columns of values and one function leaves its total columns blank.
//! * A LAMBDA is named `CUSTOM` in the row of function names.

use std::cmp::Ordering;
use std::sync::Arc;

use super::{gather, r, Function, Gather};
use crate::coord::{CellRef, Range, MAX_COLS};
use crate::eval::{to_text, Area, Ctx, Lambda, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "GROUPBY",
        3,
        8,
        "row_fields, values, function, [field_headers], [total_depth], [sort_order], [filter_array], [field_relationship]",
        "Groups rows by their fields and applies a function to the values of each group.",
        groupby,
    ),
    r(
        "PIVOTBY",
        4,
        11,
        "row_fields, col_fields, values, function, [field_headers], [row_total_depth], [row_sort_order], [col_total_depth], [col_sort_order], [filter_array], [relative_to]",
        "Groups rows and columns by their fields and applies a function to the values of each cell.",
        pivotby,
    ),
    r("PERCENTOF", 2, 2, "data_subset, data_all", "The sum of a subset as a share of the sum of all.", percentof),
    r(
        "TRIMRANGE",
        1,
        3,
        "range, [trim_rows], [trim_cols]",
        "A range without its empty edge rows and columns: 0 none, 1 leading, 2 trailing, 3 both.",
        |ctx, a| trim_expr(ctx, a, None),
    ),
];

/// The trim operators `A1.:.B9`, `A1.:B9` and `A1:.B9` as the functions a file writes them as. They are known
/// to the evaluator and the parser and are not offered in the list of functions.
pub const OPERATORS: &[Function] = &[
    r("_TRO_ALL", 1, 1, "range", "A range without its empty edges, written A1.:.B9.", |ctx, a| {
        trim_expr(ctx, a, Some(3))
    }),
    r(
        "_TRO_LEADING",
        1,
        1,
        "range",
        "A range without its leading empty rows and columns, written A1.:B9.",
        |ctx, a| trim_expr(ctx, a, Some(1)),
    ),
    r(
        "_TRO_TRAILING",
        1,
        1,
        "range",
        "A range without its trailing empty rows and columns, written A1:.B9.",
        |ctx, a| trim_expr(ctx, a, Some(2)),
    ),
];

// ---------------------------------------------------------------------------------------------------
// PERCENTOF

/// PERCENTOF: the sum of the subset over the sum of all, summed the way SUM sums its arguments.
fn percentof(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let subset = gather(ctx, &args[..1], Gather::Numbers);
    let all = gather(ctx, &args[1..], Gather::Numbers);
    match (subset, all) {
        (Err(e), _) | (_, Err(e)) => Val::error(e),
        (Ok(s), Ok(a)) => {
            let total: f64 = a.iter().sum();
            if total == 0.0 {
                Val::error(ErrorKind::Div0)
            } else {
                Val::number(s.iter().sum::<f64>() / total)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// TRIMRANGE

/// TRIMRANGE, and the three trim operators that are it with fixed modes. `fixed` is the mode for both
/// directions; without it the modes are the second and third arguments, 3 when left out.
fn trim_expr(ctx: &mut Ctx, args: &[Expr], fixed: Option<i64>) -> Val {
    let (rows, cols) = match fixed {
        Some(mode) => (mode, mode),
        None => match (trim_mode(ctx, args.get(1)), trim_mode(ctx, args.get(2))) {
            (Ok(r), Ok(c)) => (r, c),
            (Err(e), _) | (_, Err(e)) => return Val::error(e),
        },
    };
    match ctx.eval(&args[0]) {
        Val::Ref(area) if !area.is_3d() => trim_area(ctx, area, rows, cols),
        Val::Array(a) => trim_array(a, rows, cols),
        Val::Value(Value::Error(e)) => Val::error(e),
        Val::Ref(_) | Val::Refs(_) | Val::Lambda(_) => Val::error(ErrorKind::Value),
        other => other,
    }
}

/// A trim mode: 0 none, 1 leading, 2 trailing, 3 both. A whole number only, 3 when left out.
fn trim_mode(ctx: &mut Ctx, arg: Option<&Expr>) -> Result<i64, ErrorKind> {
    let Some(expr) = arg.filter(|e| !matches!(e, Expr::Missing)) else { return Ok(3) };
    let v = ctx.eval(expr);
    let v = ctx.single(v);
    let n = ctx.number(&v)?;
    if n.fract() != 0.0 || !(0.0..=3.0).contains(&n) {
        return Err(ErrorKind::Value);
    }
    Ok(n as i64)
}

/// The first and last of `0..count` that hold something, trimmed from the sides `mode` names.
fn trimmed(used: &[bool], mode: i64) -> Option<(usize, usize)> {
    let first = used.iter().position(|u| *u)?;
    let last = used.iter().rposition(|u| *u)?;
    let from = if mode & 1 != 0 { first } else { 0 };
    let to = if mode & 2 != 0 { last } else { used.len() - 1 };
    Some((from, to))
}

/// A range without its empty edge rows and columns. All empty is `#REF!`.
fn trim_area(ctx: &Ctx, area: Area, rows: i64, cols: i64) -> Val {
    if rows == 0 && cols == 0 {
        return Val::Ref(area);
    }
    let Some(sheet) = ctx.book.sheets.get(area.sheet) else { return Val::error(ErrorKind::Ref) };
    let r = area.range;
    let (height, width) = (r.rows() as usize, r.cols() as usize);
    let mut row_used = vec![false; height];
    let mut col_used = vec![false; width];
    let mut mark = |row: u32, col: u32, value: &Value| {
        if !value.is_empty()
            && (r.first.row..=r.last.row).contains(&row)
            && (r.first.col..=r.last.col).contains(&col)
        {
            row_used[(row - r.first.row) as usize] = true;
            col_used[(col - r.first.col) as usize] = true;
        }
    };
    for (&(row, col), cell) in sheet.cells.range((r.first.row, 0)..=(r.last.row, MAX_COLS)) {
        mark(row, col, &cell.value);
    }
    for (&(row, col), value) in sheet.spills.values.range((r.first.row, 0)..=(r.last.row, MAX_COLS))
    {
        mark(row, col, value);
    }
    let (Some((r0, r1)), Some((c0, c1))) = (trimmed(&row_used, rows), trimmed(&col_used, cols))
    else {
        return Val::error(ErrorKind::Ref);
    };
    let first = CellRef::new(r.first.row + r0 as u32, r.first.col + c0 as u32);
    let last = CellRef::new(r.first.row + r1 as u32, r.first.col + c1 as u32);
    Val::Ref(Area {
        sheet: area.sheet,
        sheet_last: area.sheet_last,
        range: Range::new(first, last),
    })
}

/// An array without its empty edge rows and columns.
fn trim_array(a: Array, rows: i64, cols: i64) -> Val {
    let row_used: Vec<bool> =
        (0..a.rows).map(|r| (0..a.cols).any(|c| !a.get(r, c).is_empty())).collect();
    let col_used: Vec<bool> =
        (0..a.cols).map(|c| (0..a.rows).any(|r| !a.get(r, c).is_empty())).collect();
    let (Some((r0, r1)), Some((c0, c1))) = (trimmed(&row_used, rows), trimmed(&col_used, cols))
    else {
        return Val::error(ErrorKind::Ref);
    };
    let mut data = Vec::new();
    for r in r0..=r1 {
        for c in c0..=c1 {
            data.push(a.get(r, c).clone());
        }
    }
    Val::Array(Array::new(r1 - r0 + 1, c1 - c0 + 1, data))
}

// ---------------------------------------------------------------------------------------------------
// Functions applied to groups

/// A function GROUPBY applies to a group.
#[derive(Clone)]
enum Func {
    /// A library function by name, such as SUM.
    Named(&'static str),
    Lambda(Arc<Lambda>),
}

impl Func {
    /// The name shown over the column of this function's results.
    fn title(&self) -> &'static str {
        match self {
            Func::Named(n) => n,
            Func::Lambda(_) => "CUSTOM",
        }
    }
}

/// The functions an argument names: one, or the several `HSTACK` puts side by side.
fn read_functions(ctx: &mut Ctx, expr: &Expr) -> Result<Vec<Func>, ErrorKind> {
    match expr {
        Expr::Call(name, parts) if &**name == "HSTACK" && !parts.is_empty() => {
            parts.iter().map(|p| read_function(ctx, p)).collect()
        }
        other => Ok(vec![read_function(ctx, other)?]),
    }
}

/// One function: a library function written by name, or a LAMBDA.
fn read_function(ctx: &mut Ctx, expr: &Expr) -> Result<Func, ErrorKind> {
    let expr = match expr {
        Expr::Paren(inner) => inner,
        other => other,
    };
    if let Expr::Name(name) = expr {
        let bound = ctx.scope.iter().any(|(n, _)| n.eq_ignore_ascii_case(name));
        if let (false, Some(f)) = (bound, super::find(&name.to_ascii_uppercase())) {
            let needs = if f.name == "PERCENTOF" { 2 } else { 1 };
            return if f.min <= needs && needs <= f.max {
                Ok(Func::Named(f.name))
            } else {
                Err(ErrorKind::Value)
            };
        }
    }
    match ctx.eval(expr) {
        Val::Lambda(l) if (1..=2).contains(&l.params.len()) => Ok(Func::Lambda(l)),
        Val::Value(Value::Error(e)) => Err(e),
        _ => Err(ErrorKind::Value),
    }
}

/// One result of a function over the values of a group, with all the values it is compared with. A function
/// that answers more than one value makes the whole result `#CALC!`.
fn apply(ctx: &mut Ctx, f: &Func, group: Vec<Value>, all: Vec<Value>) -> Result<Value, ErrorKind> {
    let v = match f {
        Func::Named(name) => call_named(ctx, name, group, all),
        Func::Lambda(l) => {
            let mut values = vec![Val::Array(Array::column(group))];
            if l.params.len() == 2 {
                values.push(Val::Array(Array::column(all)));
            }
            ctx.invoke_with(l, values)
        }
    };
    one_value(ctx, v)
}

/// A library function called with the group as one argument (and the whole as a second for PERCENTOF).
fn call_named(ctx: &mut Ctx, name: &'static str, group: Vec<Value>, all: Vec<Value>) -> Val {
    let (gx, gw): (Arc<str>, Arc<str>) = (Arc::from("_GROUP_VALUES_"), Arc::from("_ALL_VALUES_"));
    let mark = ctx.scope.len();
    ctx.scope.push((gx.clone(), Val::Array(Array::column(group))));
    let mut args = vec![Expr::Name(gx)];
    if name == "PERCENTOF" {
        ctx.scope.push((gw.clone(), Val::Array(Array::column(all))));
        args.push(Expr::Name(gw));
    }
    let v = ctx.eval(&Expr::Call(Arc::from(name), args));
    ctx.scope.truncate(mark);
    v
}

/// A function's result as the one value a cell holds; an array of several is `#CALC!`.
fn one_value(ctx: &mut Ctx, v: Val) -> Result<Value, ErrorKind> {
    match v {
        Val::Value(x) => Ok(x),
        Val::Array(a) if a.data.len() == 1 => Ok(a.data[0].clone()),
        Val::Ref(area) if area.range.len() == 1 && !area.is_3d() => {
            Ok(ctx.cell_value(area.sheet, area.range.first))
        }
        _ => Err(ErrorKind::Calc),
    }
}

// ---------------------------------------------------------------------------------------------------
// Reading the arguments

/// An optional argument, `None` when it is absent or left empty.
fn present(args: &[Expr], at: usize) -> Option<&Expr> {
    args.get(at).filter(|e| !matches!(e, Expr::Missing))
}

/// A whole number argument; left out it is `default`. A logical is 0 or 1 when `logical` allows it.
fn whole(
    ctx: &mut Ctx,
    args: &[Expr],
    at: usize,
    default: i64,
    logical: bool,
) -> Result<i64, ErrorKind> {
    let Some(expr) = present(args, at) else { return Ok(default) };
    let v = ctx.eval(expr);
    match ctx.single(v) {
        Value::Empty => Ok(default),
        Value::Error(e) => Err(e),
        Value::Bool(b) if logical => Ok(i64::from(b)),
        Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e9 => Ok(n as i64),
        _ => Err(ErrorKind::Value),
    }
}

/// An argument that is a range or an array, as an array.
fn table(ctx: &mut Ctx, expr: &Expr) -> Result<Array, ErrorKind> {
    let v = ctx.eval(expr);
    ctx.array(v)
}

/// The sort order argument: whole numbers in a list, no logicals.
fn sort_list(ctx: &mut Ctx, args: &[Expr], at: usize) -> Result<Vec<i64>, ErrorKind> {
    let Some(expr) = present(args, at) else { return Ok(Vec::new()) };
    let array = table(ctx, expr)?;
    let mut out = Vec::new();
    for v in &array.data {
        match v {
            Value::Number(n) if n.fract() == 0.0 && *n != 0.0 && n.abs() < 1e6 => {
                out.push(*n as i64)
            }
            Value::Error(e) => return Err(*e),
            Value::Empty => {}
            _ => return Err(ErrorKind::Value),
        }
    }
    Ok(out)
}

/// The filter argument: a column with a logical or number for each record.
fn filter_list(
    ctx: &mut Ctx,
    args: &[Expr],
    at: usize,
    records: usize,
) -> Result<Option<Vec<bool>>, ErrorKind> {
    let Some(expr) = present(args, at) else { return Ok(None) };
    let array = table(ctx, expr)?;
    if array.rows != records || array.cols != 1 {
        return Err(ErrorKind::Value);
    }
    let mut out = Vec::with_capacity(records);
    for v in array.data {
        out.push(match v {
            Value::Bool(b) => b,
            Value::Number(n) => n != 0.0,
            Value::Error(e) => return Err(e),
            _ => return Err(ErrorKind::Value),
        });
    }
    Ok(Some(out))
}

/// Everything GROUPBY and PIVOTBY work from, once the headers are cut off and the filter is applied.
struct Data {
    /// The key columns of each axis: records by fields.
    rows: Vec<Vec<Value>>,
    cols: Vec<Vec<Value>>,
    /// The value columns, each with a value for every record.
    values: Vec<Vec<Value>>,
    funcs: Vec<Func>,
    /// For each column of results, the value column and the function it pairs.
    outputs: Vec<(usize, usize)>,
    /// The header text of every row field, column field and value column, when the data had headers.
    headers: Option<Headers>,
    /// How the headers are written above the result.
    show: Show,
    row_width: usize,
    col_width: usize,
}

struct Headers {
    rows: Vec<Value>,
    cols: Vec<Value>,
    values: Vec<Value>,
}

/// Which header rows the result gets.
#[derive(Clone, Copy, PartialEq)]
enum Show {
    No,
    Generated,
    FromData,
}

/// The records and columns of the arguments, ready for grouping.
struct Source {
    row_fields: Array,
    col_fields: Option<Array>,
    values: Array,
}

/// Cut the header row off the arguments, work out which records are filtered in, and pair functions with
/// values. `mode` is the headers argument as given, `None` when it was left out.
fn prepare(
    source: Source,
    mode: Option<i64>,
    filter: Option<Vec<bool>>,
    funcs: Vec<Func>,
) -> Result<Data, ErrorKind> {
    let Source { row_fields, col_fields, values } = source;
    let multi = funcs.len() > 1;
    let detected = mode.is_none() && looks_like_headers(&values);
    let consume = matches!(mode, Some(1) | Some(3)) || detected;
    let show = match mode {
        Some(2) => Show::Generated,
        Some(3) => Show::FromData,
        None if detected && multi => Show::FromData,
        _ => Show::No,
    };
    let skip = usize::from(consume);
    let headers = consume.then(|| Headers {
        rows: row_fields.rows_iter().next().map(<[Value]>::to_vec).unwrap_or_default(),
        cols: col_fields
            .as_ref()
            .and_then(|c| c.rows_iter().next().map(<[Value]>::to_vec))
            .unwrap_or_default(),
        values: values.rows_iter().next().map(<[Value]>::to_vec).unwrap_or_default(),
    });
    let keep: Vec<usize> =
        (skip..row_fields.rows).filter(|i| filter.as_ref().is_none_or(|f| f[*i])).collect();
    if keep.is_empty() {
        return Err(ErrorKind::Value);
    }
    let pick =
        |a: &Array, i: usize| a.rows_iter().nth(i).map(<[Value]>::to_vec).unwrap_or_default();
    let nv = values.cols;
    let outputs = pair_outputs(nv, funcs.len())?;
    Ok(Data {
        rows: keep.iter().map(|i| pick(&row_fields, *i)).collect(),
        cols: col_fields
            .as_ref()
            .map_or_else(Vec::new, |c| keep.iter().map(|i| pick(c, *i)).collect()),
        values: (0..nv).map(|c| keep.iter().map(|i| values.get(*i, c).clone()).collect()).collect(),
        funcs,
        outputs,
        headers,
        show,
        row_width: row_fields.cols,
        col_width: col_fields.as_ref().map_or(0, |c| c.cols),
    })
}

/// Whether the first row of the values looks like headers: all text, with a number below in each column.
fn looks_like_headers(values: &Array) -> bool {
    if values.rows < 2 {
        return false;
    }
    (0..values.cols).all(|c| {
        matches!(values.get(0, c), Value::Text(_))
            && (1..values.rows).any(|r| matches!(values.get(r, c), Value::Number(_)))
    })
}

/// How value columns and functions pair up: one function for every column, one column for every function,
/// or each column with its own function. Anything else is `#VALUE!`.
fn pair_outputs(values: usize, funcs: usize) -> Result<Vec<(usize, usize)>, ErrorKind> {
    match (values, funcs) {
        (v, 1) => Ok((0..v).map(|i| (i, 0)).collect()),
        (1, f) => Ok((0..f).map(|i| (0, i)).collect()),
        (v, f) if v == f => Ok((0..v).map(|i| (i, i)).collect()),
        _ => Err(ErrorKind::Value),
    }
}

// ---------------------------------------------------------------------------------------------------
// Keys

/// Two keys name one group: numbers by value, text without regard to case.
fn same_key(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => a == b,
    }
}

/// The order of keys and of values being sorted: numbers, text without regard to case, logicals, errors,
/// and empty cells last.
fn compare_values(a: &Value, b: &Value) -> Ordering {
    let rank = |v: &Value| match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        Value::Empty => 4,
    };
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// The records whose keys start with `prefix`.
fn records_with(keys: &[Vec<Value>], prefix: &[Value]) -> Vec<usize> {
    (0..keys.len())
        .filter(|i| prefix.iter().enumerate().all(|(k, v)| same_key(&keys[*i][k], v)))
        .collect()
}

/// The records both lists hold, in order.
fn both(a: &[usize], b: &[usize]) -> Vec<usize> {
    a.iter().copied().filter(|i| b.binary_search(i).is_ok()).collect()
}

// ---------------------------------------------------------------------------------------------------
// Axes: the groups down the rows or across the columns, in order, with their totals

/// One line of an axis: a group, a subtotal for the groups above it, or the total of everything.
#[derive(Clone)]
struct Item {
    prefix: Vec<Value>,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Leaf,
    Subtotal,
    Total,
}

/// The fields of one axis, how deep its totals go and how it is sorted.
struct Axis<'a> {
    keys: &'a [Vec<Value>],
    width: usize,
    depth: i64,
    sort: Vec<i64>,
    /// Columns a sort may name after the key columns: the results of the functions.
    outputs: usize,
}

impl Axis<'_> {
    /// Check the totals depth and the sort list against the fields.
    fn check(&self) -> Result<(), ErrorKind> {
        if self.depth.unsigned_abs() as usize > self.width {
            return Err(ErrorKind::Value);
        }
        let columns = (self.width + self.outputs) as i64;
        if self.sort.iter().any(|s| s.abs() > columns) {
            return Err(ErrorKind::Value);
        }
        let on_keys = self.sort.iter().all(|s| s.unsigned_abs() as usize <= self.width);
        let mut seen: Vec<u64> = self.sort.iter().map(|s| s.unsigned_abs()).collect();
        seen.sort_unstable();
        seen.dedup();
        if self.sort.len() > 1 && (!on_keys || seen.len() != self.sort.len()) {
            return Err(ErrorKind::Value);
        }
        Ok(())
    }
}

/// The items of an axis in display order. `aggregate` gives the results of the functions for a group of
/// records, which a sort on a value needs.
fn axis_items(
    ctx: &mut Ctx,
    axis: &Axis,
    aggregate: &mut Aggregate,
) -> Result<Vec<Item>, ErrorKind> {
    let all: Vec<usize> = (0..axis.keys.len()).collect();
    let mut out = Vec::new();
    let before = axis.depth < 0;
    if axis.depth != 0 && before {
        out.push(Item { prefix: Vec::new(), kind: Kind::Total });
    }
    expand(ctx, axis, aggregate, &all, &mut Vec::new(), &mut out)?;
    if axis.depth != 0 && !before {
        out.push(Item { prefix: Vec::new(), kind: Kind::Total });
    }
    Ok(out)
}

/// What a sort on a value needs: the results of the functions for a group of records.
type Aggregate<'a> = dyn FnMut(&mut Ctx, &[usize]) -> Result<Vec<Value>, ErrorKind> + 'a;

/// The groups at one level below `prefix`, sorted, each followed (or preceded) by its subtotal and its
/// own groups.
fn expand(
    ctx: &mut Ctx,
    axis: &Axis,
    aggregate: &mut Aggregate,
    records: &[usize],
    prefix: &mut Vec<Value>,
    out: &mut Vec<Item>,
) -> Result<(), ErrorKind> {
    let level = prefix.len();
    let mut groups: Vec<(Value, Vec<usize>)> = Vec::new();
    for &i in records {
        let key = &axis.keys[i][level];
        match groups.iter_mut().find(|(k, _)| same_key(k, key)) {
            Some((_, list)) => list.push(i),
            None => groups.push((key.clone(), vec![i])),
        }
    }
    let needs_values = axis.sort.iter().any(|s| s.unsigned_abs() as usize > axis.width);
    let results: Vec<Vec<Value>> = if needs_values {
        groups.iter().map(|(_, list)| aggregate(ctx, list)).collect::<Result<_, _>>()?
    } else {
        Vec::new()
    };
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| {
        compare_groups(axis, level, (&groups[a].0, results.get(a)), (&groups[b].0, results.get(b)))
    });
    let subtotal = level + 1 < axis.width && ((level + 1) as i64) < axis.depth.abs();
    for g in order {
        prefix.push(groups[g].0.clone());
        let item = |prefix: &[Value], kind| Item { prefix: prefix.to_vec(), kind };
        if subtotal && axis.depth < 0 {
            out.push(item(prefix, Kind::Subtotal));
        }
        if level + 1 == axis.width {
            out.push(item(prefix, Kind::Leaf));
        } else {
            expand(ctx, axis, aggregate, &groups[g].1, prefix, out)?;
        }
        if subtotal && axis.depth > 0 {
            out.push(item(prefix, Kind::Subtotal));
        }
        prefix.pop();
    }
    Ok(())
}

/// How two groups at a level compare under the sort list; ties and sorts that do not apply at this level
/// fall back to ascending order of the key.
fn compare_groups(
    axis: &Axis,
    level: usize,
    a: (&Value, Option<&Vec<Value>>),
    b: (&Value, Option<&Vec<Value>>),
) -> Ordering {
    for &spec in &axis.sort {
        let column = spec.unsigned_abs() as usize - 1;
        let order = if column < axis.width {
            if column != level {
                continue;
            }
            compare_values(a.0, b.0)
        } else {
            match (
                a.1.and_then(|r| r.get(column - axis.width)),
                b.1.and_then(|r| r.get(column - axis.width)),
            ) {
                (Some(x), Some(y)) => compare_values(x, y),
                _ => Ordering::Equal,
            }
        };
        let order = if spec < 0 { order.reverse() } else { order };
        if order != Ordering::Equal {
            return order;
        }
    }
    compare_values(a.0, b.0)
}

// ---------------------------------------------------------------------------------------------------
// Results

/// The results of every function for a group of records, with `all` as the whole it is compared with.
fn results_for(
    ctx: &mut Ctx,
    d: &Data,
    records: &[usize],
    all: &[usize],
) -> Result<Vec<Value>, ErrorKind> {
    d.outputs
        .iter()
        .map(|(column, f)| {
            let pick = |list: &[usize]| {
                list.iter().map(|i| d.values[*column][*i].clone()).collect::<Vec<_>>()
            };
            apply(ctx, &d.funcs[*f], pick(records), pick(all))
        })
        .collect()
}

/// The label of the total line: `Total`, or `Grand Total` when there are subtotals as well.
fn total_label(depth: i64) -> &'static str {
    if depth.abs() >= 2 {
        "Grand Total"
    } else {
        "Total"
    }
}

/// The key cells of a line: the group's keys, then blank text; the total's label then blank text.
fn key_cells(item: &Item, width: usize, depth: i64) -> Vec<Value> {
    let mut cells: Vec<Value> = item.prefix.clone();
    if item.kind == Kind::Total {
        cells.push(Value::text(total_label(depth)));
    }
    cells.resize(width, Value::text(""));
    cells
}

/// The names of the functions, when there are several.
fn function_titles(d: &Data) -> Option<Vec<Value>> {
    (d.funcs.len() > 1)
        .then(|| d.outputs.iter().map(|(_, f)| Value::text(d.funcs[*f].title())).collect())
}

/// The header cells over the value columns: made up, or the data's own repeated for each function.
fn value_headers(d: &Data) -> Vec<Value> {
    d.outputs
        .iter()
        .map(|(column, _)| match (d.show, &d.headers) {
            (Show::FromData, Some(h)) => h.values.get(*column).cloned().unwrap_or(Value::Empty),
            _ => Value::text(format!("Value {}", column + 1)),
        })
        .collect()
}

/// The header cells over the row fields.
fn row_headers(d: &Data) -> Vec<Value> {
    (0..d.row_width)
        .map(|i| match (d.show, &d.headers) {
            (Show::FromData, Some(h)) => h.rows.get(i).cloned().unwrap_or(Value::Empty),
            _ => Value::text(format!("Row Field {}", i + 1)),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------
// GROUPBY

/// GROUPBY.
fn groupby(ctx: &mut Ctx, args: &[Expr]) -> Val {
    match run_groupby(ctx, args) {
        Ok(array) => Val::Array(array),
        Err(e) => Val::error(e),
    }
}

/// Everything GROUPBY does, with the first error as the result.
fn run_groupby(ctx: &mut Ctx, args: &[Expr]) -> Result<Array, ErrorKind> {
    let row_fields = table(ctx, &args[0])?;
    let values = table(ctx, &args[1])?;
    let funcs = read_functions(ctx, &args[2])?;
    let mode = header_mode(ctx, args, 3)?;
    let depth = whole(ctx, args, 4, 1, true)?;
    let depth_given = present(args, 4).is_some();
    let sort = sort_list(ctx, args, 5)?;
    if row_fields.rows != values.rows {
        return Err(ErrorKind::Value);
    }
    let filter = filter_list(ctx, args, 6, values.rows)?;
    let relationship = whole(ctx, args, 7, 0, true)?;
    if !(0..=1).contains(&relationship) || (relationship == 1 && depth.abs() >= 2) {
        return Err(ErrorKind::Value);
    }
    let source = Source { row_fields, col_fields: None, values };
    let d = prepare(source, mode, filter, funcs)?;
    let depth = if depth_given { depth } else { default_depth(d.row_width, &sort) };
    let axis = Axis { keys: &d.rows, width: d.row_width, depth, sort, outputs: d.outputs.len() };
    axis.check()?;
    let everything: Vec<usize> = (0..d.rows.len()).collect();
    let items = axis_items(ctx, &axis, &mut |ctx, list| results_for(ctx, &d, list, &everything))?;
    groupby_rows(ctx, &d, &items, depth, &everything)
}

/// The depth of totals when the argument is left out: one grand total, except that a sort on a value with
/// several row fields also shows the subtotals of every level, which is what Excel does.
fn default_depth(width: usize, sort: &[i64]) -> i64 {
    if width >= 2 && sort.iter().any(|s| s.unsigned_abs() as usize > width) {
        width as i64
    } else {
        1
    }
}

/// The headers argument: left out is `None`; otherwise 0 to 3.
fn header_mode(ctx: &mut Ctx, args: &[Expr], at: usize) -> Result<Option<i64>, ErrorKind> {
    if present(args, at).is_none() {
        return Ok(None);
    }
    let mode = whole(ctx, args, at, 0, true)?;
    if (0..=3).contains(&mode) {
        Ok(Some(mode))
    } else {
        Err(ErrorKind::Value)
    }
}

/// The lines of a GROUPBY result.
fn groupby_rows(
    ctx: &mut Ctx,
    d: &Data,
    items: &[Item],
    depth: i64,
    everything: &[usize],
) -> Result<Array, ErrorKind> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let blanks = vec![Value::text(""); d.row_width];
    if let Some(titles) = function_titles(d) {
        rows.push([blanks.clone(), titles].concat());
    }
    if d.show != Show::No {
        rows.push([row_headers(d), value_headers(d)].concat());
    }
    for item in items {
        let records = if item.kind == Kind::Total {
            everything.to_vec()
        } else {
            records_with(&d.rows, &item.prefix)
        };
        let results = results_for(ctx, d, &records, everything)?;
        rows.push([key_cells(item, d.row_width, depth), results].concat());
    }
    Ok(Array::from_rows(rows))
}

// ---------------------------------------------------------------------------------------------------
// PIVOTBY

/// PIVOTBY.
fn pivotby(ctx: &mut Ctx, args: &[Expr]) -> Val {
    match run_pivotby(ctx, args) {
        Ok(array) => Val::Array(array),
        Err(e) => Val::error(e),
    }
}

/// The arguments PIVOTBY reads after the data, which decide the shape of the result.
struct PivotOptions {
    row_depth: i64,
    row_sort: Vec<i64>,
    col_depth: i64,
    col_sort: Vec<i64>,
    relative_to: i64,
}

/// Everything PIVOTBY does, with the first error as the result.
fn run_pivotby(ctx: &mut Ctx, args: &[Expr]) -> Result<Array, ErrorKind> {
    let row_fields = table(ctx, &args[0])?;
    let col_fields = table(ctx, &args[1])?;
    let values = table(ctx, &args[2])?;
    let funcs = read_functions(ctx, &args[3])?;
    let mode = header_mode(ctx, args, 4)?;
    let options = PivotOptions {
        row_depth: whole(ctx, args, 5, 1, true)?,
        row_sort: sort_list(ctx, args, 6)?,
        col_depth: whole(ctx, args, 7, 1, true)?,
        col_sort: sort_list(ctx, args, 8)?,
        relative_to: whole(ctx, args, 10, 0, false)?,
    };
    if row_fields.rows != values.rows
        || col_fields.rows != values.rows
        || !(0..=4).contains(&options.relative_to)
    {
        return Err(ErrorKind::Value);
    }
    let filter = filter_list(ctx, args, 9, values.rows)?;
    let source = Source { row_fields, col_fields: Some(col_fields), values };
    let d = prepare(source, mode, filter, funcs)?;
    pivot_result(ctx, &d, options)
}

/// The items of both axes and the cells between them.
fn pivot_result(ctx: &mut Ctx, d: &Data, o: PivotOptions) -> Result<Array, ErrorKind> {
    let everything: Vec<usize> = (0..d.rows.len()).collect();
    let row_axis = Axis {
        keys: &d.rows,
        width: d.row_width,
        depth: o.row_depth,
        sort: o.row_sort.clone(),
        outputs: d.outputs.len(),
    };
    // A sort of the columns can name only the first result, the total row under it.
    let col_axis = Axis {
        keys: &d.cols,
        width: d.col_width,
        depth: o.col_depth,
        sort: o.col_sort.clone(),
        outputs: 1,
    };
    row_axis.check()?;
    col_axis.check()?;
    let rows = axis_items(ctx, &row_axis, &mut |ctx, list| results_for(ctx, d, list, &everything))?;
    let cols = axis_items(ctx, &col_axis, &mut |ctx, list| results_for(ctx, d, list, &everything))?;
    let row_records: Vec<Vec<usize>> =
        rows.iter().map(|i| records_of(&d.rows, i, &everything)).collect();
    let col_records: Vec<Vec<usize>> =
        cols.iter().map(|i| records_of(&d.cols, i, &everything)).collect();
    let mut lines = pivot_headers(d, &cols, o.col_depth);
    for (r, item) in rows.iter().enumerate() {
        let mut line = key_cells(item, d.row_width, o.row_depth);
        for (c, column) in cols.iter().enumerate() {
            let cell = both(&row_records[r], &col_records[c]);
            let all = compared_with(
                d,
                o.relative_to,
                (item, &row_records[r]),
                (column, &col_records[c]),
                &everything,
            );
            line.extend(pivot_cell(ctx, d, &cell, &all, column.kind != Kind::Leaf)?);
        }
        lines.push(line);
    }
    Ok(Array::from_rows(lines))
}

/// The records an item stands for.
fn records_of(keys: &[Vec<Value>], item: &Item, everything: &[usize]) -> Vec<usize> {
    if item.kind == Kind::Total {
        everything.to_vec()
    } else {
        records_with(keys, &item.prefix)
    }
}

/// The records a cell is compared with: the column (0), the row (1), everything (2), the row within the
/// parent of the column (3), or the column within the parent of the row (4).
fn compared_with(
    d: &Data,
    relative_to: i64,
    row: (&Item, &Vec<usize>),
    col: (&Item, &Vec<usize>),
    everything: &[usize],
) -> Vec<usize> {
    let parent = |keys: &[Vec<Value>], item: &Item| -> Vec<usize> {
        match item.prefix.len() {
            0 | 1 => everything.to_vec(),
            n => records_with(keys, &item.prefix[..n - 1]),
        }
    };
    match relative_to {
        0 => col.1.clone(),
        1 => row.1.clone(),
        2 => everything.to_vec(),
        3 => both(row.1, &parent(&d.cols, col.0)),
        _ => both(&parent(&d.rows, row.0), col.1),
    }
}

/// The cells of one column item: a result for each function, or blank text when no record reaches it.
/// With several columns of values and one function Excel leaves the total columns blank.
fn pivot_cell(
    ctx: &mut Ctx,
    d: &Data,
    records: &[usize],
    all: &[usize],
    total_column: bool,
) -> Result<Vec<Value>, ErrorKind> {
    let blank = records.is_empty() || (total_column && d.values.len() > 1 && d.funcs.len() == 1);
    if blank {
        return Ok(vec![Value::text(""); d.outputs.len()]);
    }
    results_for(ctx, d, records, all)
}

/// The header lines above a pivot: the column field names, the column keys, the function names and the
/// value headers, each as far as the headers argument and the functions ask for them.
fn pivot_headers(d: &Data, cols: &[Item], col_depth: i64) -> Vec<Vec<Value>> {
    let width = d.row_width;
    let blank = |n: usize| vec![Value::text(""); n];
    let mut lines: Vec<Vec<Value>> = Vec::new();
    if d.show != Show::No {
        lines.push(column_field_line(d, cols.len()));
    }
    for level in 0..d.col_width {
        let mut line = blank(width);
        for item in cols {
            let label = col_label(item, level, col_depth);
            line.extend(std::iter::repeat_n(label, d.outputs.len()));
        }
        lines.push(line);
    }
    if let Some(titles) = function_titles(d) {
        let mut line = blank(width);
        for _ in cols {
            line.extend(titles.clone());
        }
        lines.push(line);
    }
    if d.show != Show::No {
        let mut line = row_headers(d);
        for _ in cols {
            line.extend(value_headers(d));
        }
        lines.push(line);
    }
    lines
}

/// The line naming the column fields: their names in one cell over the first column.
fn column_field_line(d: &Data, columns: usize) -> Vec<Value> {
    let names: Vec<String> = (0..d.col_width)
        .map(|i| match (d.show, &d.headers) {
            (Show::FromData, Some(h)) => {
                to_text(h.cols.get(i).unwrap_or(&Value::Empty)).unwrap_or_default()
            }
            _ => format!("Column Field {}", i + 1),
        })
        .collect();
    let mut line = vec![Value::text(""); d.row_width + columns * d.outputs.len()];
    if columns > 0 {
        line[d.row_width] = Value::text(names.join(", "));
    }
    line
}

/// The cell over a column at one level of the column fields.
fn col_label(item: &Item, level: usize, depth: i64) -> Value {
    match item.kind {
        Kind::Total if level == 0 => Value::text(total_label(depth)),
        Kind::Total => Value::text(""),
        _ => item.prefix.get(level).cloned().unwrap_or_else(|| Value::text("")),
    }
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{book, eval_on, shown};

    /// The formula as text of its whole array, which is how the oracle compares arrays.
    fn array(formula: &str) -> String {
        let data = [("A1", "apple"), ("A2", "banana"), ("A3", "apple"), ("A4", "cherry")];
        let numbers = [("B1", "10"), ("B2", "20"), ("B3", "30"), ("B4", "40")];
        let b = book(&[data.as_slice(), numbers.as_slice()].concat());
        shown(&eval_on(&b, &format!("ARRAYTOTEXT({formula},1)")))
    }

    #[test]
    fn groupby_groups_sorts_and_totals() {
        assert_eq!(
            array("GROUPBY(A1:A4,B1:B4,SUM)"),
            r#"{"apple",40;"banana",20;"cherry",40;"Total",100}"#
        );
        assert_eq!(
            array("GROUPBY(A1:A4,B1:B4,SUM,,-1,-2)"),
            r#"{"Total",100;"apple",40;"cherry",40;"banana",20}"#
        );
        assert_eq!(array("GROUPBY(A1:A4,B1:B4,SUM,,0)"), r#"{"apple",40;"banana",20;"cherry",40}"#);
        assert_eq!(
            array("GROUPBY(A1:A4,B1:B4,SUM,2)"),
            r#"{"Row Field 1","Value 1";"apple",40;"banana",20;"cherry",40;"Total",100}"#
        );
        assert_eq!(
            array("GROUPBY(A1:A4,B1:B4,HSTACK(SUM,COUNT))"),
            r#"{"","SUM","COUNT";"apple",40,2;"banana",20,1;"cherry",40,1;"Total",100,4}"#
        );
        assert_eq!(
            array("GROUPBY(A1:A4,B1:B4,LAMBDA(x,SUM(x)*2),,,,B1:B4>10)"),
            r#"{"apple",60;"banana",40;"cherry",80;"Total",180}"#
        );
        assert_eq!(array("GROUPBY(A1:A4,B1:B4,LAMBDA(x,x))"), "#CALC!");
        assert_eq!(array("GROUPBY(A1:A4,B1:B4,SUM,,5)"), "#VALUE!");
        assert_eq!(array("GROUPBY(A1:A4,B1:B4,ROUND)"), "#VALUE!");
    }

    #[test]
    fn pivotby_puts_the_second_field_across_and_totals_both_ways() {
        assert_eq!(
            array("PIVOTBY(A1:A4,B1:B4>25,B1:B4,SUM)"),
            r#"{"",FALSE,TRUE,"Total";"apple",10,30,40;"banana",20,"",20;"cherry","",40,40;"Total",30,70,100}"#
        );
        assert_eq!(
            array("PIVOTBY(A1:A4,B1:B4>25,B1:B4,PERCENTOF,,,,,,,2)"),
            r#"{"",FALSE,TRUE,"Total";"apple",0.1,0.3,0.4;"banana",0.2,"",0.2;"cherry","",0.4,0.4;"Total",0.3,0.7,1}"#
        );
    }

    #[test]
    fn percentof_and_trimrange_work_on_what_is_in_the_sheet() {
        let b = book(&[("B1", "10"), ("B2", "20"), ("B4", "40"), ("D3", "1")]);
        for (f, want) in [
            ("PERCENTOF(B1:B2,B1:B4)", "0.428571428571"),
            ("PERCENTOF(B1,0)", "#DIV/0!"),
            ("ROWS(TRIMRANGE(A1:D9))", "4"),
            ("COLUMNS(TRIMRANGE(A1:D9))", "3"),
            ("ROWS(TRIMRANGE(A1:D9,1,1))", "9"),
            ("ROWS(TRIMRANGE(A1:D9,2,0))", "4"),
            ("COLUMNS(TRIMRANGE(A1:D9,0,1))", "3"),
            ("ROWS(TRIMRANGE(F1:G4))", "#REF!"),
            ("ROWS(TRIMRANGE(A1:D9,4))", "#VALUE!"),
            ("ROWS(B1.:.B9)", "4"),
            ("ROWS(B1:.B9)", "4"),
            ("ROWS(B1.:B9)", "9"),
        ] {
            assert_eq!(shown(&eval_on(&b, f)), want, "{f}");
        }
    }
}
