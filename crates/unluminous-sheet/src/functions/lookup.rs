//! Lookup and reference functions, and the dynamic array functions that reshape arrays.

use std::cmp::Ordering;

use super::criteria::wildcard_match;
use super::{r, Function, MANY};
use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::eval::{compare, equal, Area, Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "VLOOKUP",
        3,
        4,
        "lookup_value, table_array, col_index_num, [range_lookup]",
        "Looks down the first column and returns a value from another column.",
        |ctx, a| vh_lookup(ctx, a, true),
    ),
    r(
        "HLOOKUP",
        3,
        4,
        "lookup_value, table_array, row_index_num, [range_lookup]",
        "Looks along the first row and returns a value from another row.",
        |ctx, a| vh_lookup(ctx, a, false),
    ),
    r(
        "LOOKUP",
        2,
        3,
        "lookup_value, lookup_vector, [result_vector]",
        "Looks up a value in a sorted row or column.",
        lookup,
    ),
    r(
        "XLOOKUP",
        3,
        6,
        "lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode]",
        "Finds a value and returns what is beside it.",
        xlookup,
    ),
    r(
        "MATCH",
        2,
        3,
        "lookup_value, lookup_array, [match_type]",
        "The position of a value in a row or column.",
        match_fn,
    ),
    r(
        "XMATCH",
        2,
        4,
        "lookup_value, lookup_array, [match_mode], [search_mode]",
        "The position of a value in a row or column.",
        xmatch,
    ),
    r(
        "INDEX",
        2,
        4,
        "array, row_num, [column_num], [area_num]",
        "The value or reference at a row and column of a range.",
        index,
    ),
    r(
        "CHOOSE",
        2,
        MANY,
        "index_num, value1, [value2], ...",
        "One of a list of values by number.",
        choose,
    ),
    r(
        "CHOOSECOLS",
        2,
        MANY,
        "array, col_num1, [col_num2], ...",
        "Some columns of an array.",
        |ctx, a| choose_lines(ctx, a, false),
    ),
    r(
        "CHOOSEROWS",
        2,
        MANY,
        "array, row_num1, [row_num2], ...",
        "Some rows of an array.",
        |ctx, a| choose_lines(ctx, a, true),
    ),
    r(
        "OFFSET",
        3,
        5,
        "reference, rows, cols, [height], [width]",
        "A reference moved and resized.",
        offset,
    ),
    r("INDIRECT", 1, 2, "ref_text, [a1]", "The reference written in text.", indirect),
    r(
        "ADDRESS",
        2,
        5,
        "row_num, column_num, [abs_num], [a1], [sheet_text]",
        "A cell address as text.",
        address,
    ),
    r("ROW", 0, 1, "[reference]", "The row number of a reference.", |ctx, a| row_col(ctx, a, true)),
    r("COLUMN", 0, 1, "[reference]", "The column number of a reference.", |ctx, a| {
        row_col(ctx, a, false)
    }),
    r("ROWS", 1, 1, "array", "How many rows.", |ctx, a| count_lines(ctx, a, true)),
    r("COLUMNS", 1, 1, "array", "How many columns.", |ctx, a| count_lines(ctx, a, false)),
    r("AREAS", 1, 1, "reference", "How many areas a reference has.", |ctx, a| {
        match ctx.eval(&a[0]) {
            Val::Ref(_) => Val::number(1.0),
            Val::Refs(areas) => Val::number(areas.len() as f64),
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        }
    }),
    r("TRANSPOSE", 1, 1, "array", "Rows turned into columns.", |ctx, a| {
        with_array(ctx, &a[0], |a| Ok(a.transpose()))
    }),
    r(
        "FILTER",
        2,
        3,
        "array, include, [if_empty]",
        "The rows or columns that meet a condition.",
        filter,
    ),
    r("SORT", 1, 4, "array, [sort_index], [sort_order], [by_col]", "An array sorted.", sort),
    r(
        "SORTBY",
        2,
        MANY,
        "array, by_array1, [sort_order1], ...",
        "An array sorted by other arrays.",
        sortby,
    ),
    r("UNIQUE", 1, 3, "array, [by_col], [exactly_once]", "The distinct rows or columns.", unique),
    r(
        "TAKE",
        2,
        3,
        "array, rows, [columns]",
        "Rows or columns from the start or the end.",
        |ctx, a| take_drop(ctx, a, true),
    ),
    r(
        "DROP",
        2,
        3,
        "array, rows, [columns]",
        "An array without rows or columns at the start or the end.",
        |ctx, a| take_drop(ctx, a, false),
    ),
    r("VSTACK", 1, MANY, "array1, [array2], ...", "Arrays one under another.", |ctx, a| {
        stack(ctx, a, true)
    }),
    r("HSTACK", 1, MANY, "array1, [array2], ...", "Arrays side by side.", |ctx, a| {
        stack(ctx, a, false)
    }),
    r("TOCOL", 1, 3, "array, [ignore], [scan_by_column]", "An array as one column.", |ctx, a| {
        to_line(ctx, a, true)
    }),
    r("TOROW", 1, 3, "array, [ignore], [scan_by_column]", "An array as one row.", |ctx, a| {
        to_line(ctx, a, false)
    }),
    r(
        "WRAPROWS",
        2,
        3,
        "vector, wrap_count, [pad_with]",
        "A row or column wrapped into rows.",
        |ctx, a| wrap(ctx, a, true),
    ),
    r(
        "WRAPCOLS",
        2,
        3,
        "vector, wrap_count, [pad_with]",
        "A row or column wrapped into columns.",
        |ctx, a| wrap(ctx, a, false),
    ),
    r("EXPAND", 2, 4, "array, rows, [columns], [pad_with]", "An array grown to a size.", expand),
    r(
        "HYPERLINK",
        1,
        2,
        "link_location, [friendly_name]",
        "A link; the cell shows the friendly name.",
        |ctx, a| {
            let shown = a.get(1).unwrap_or(&a[0]);
            let v = ctx.eval(shown);
            Val::Value(ctx.single(v))
        },
    ),
    r("FORMULATEXT", 1, 1, "reference", "A cell's formula as text.", |ctx, a| {
        match ctx.eval(&a[0]) {
            Val::Ref(area) => match ctx.book.sheets[area.sheet]
                .cell(area.range.first)
                .and_then(|c| c.formula.as_ref())
            {
                Some(f) => Val::text(f.display()),
                None => Val::error(ErrorKind::NA),
            },
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        }
    }),
];

fn scalar(ctx: &mut Ctx, e: &Expr) -> Value {
    let v = ctx.eval(e);
    ctx.single(v)
}

fn number_arg(ctx: &mut Ctx, args: &[Expr], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match args.get(i) {
        None | Some(Expr::Missing) => Ok(default),
        Some(e) => {
            let v = scalar(ctx, e);
            if v.is_empty() {
                return Ok(default);
            }
            ctx.number(&v)
        }
    }
}

fn array_of(ctx: &mut Ctx, e: &Expr) -> Result<Array, ErrorKind> {
    let v = ctx.eval(e);
    ctx.array(v)
}

fn with_array(ctx: &mut Ctx, e: &Expr, f: impl FnOnce(Array) -> Result<Array, ErrorKind>) -> Val {
    match array_of(ctx, e).and_then(f) {
        Ok(a) => Val::Array(a),
        Err(e) => Val::error(e),
    }
}

/// Exact equality for lookups: text without regard to case, with wildcards when asked.
fn matches_exactly(target: &Value, candidate: &Value, wildcards: bool) -> bool {
    match (target, candidate) {
        (Value::Text(t), Value::Text(c)) if wildcards && t.contains(['*', '?', '~']) => {
            wildcard_match(t, c)
        }
        (Value::Number(_), Value::Text(_)) | (Value::Text(_), Value::Number(_)) => false,
        (Value::Empty, _) | (_, Value::Empty) => false,
        _ => equal(target, candidate),
    }
}

fn same_type(a: &Value, b: &Value) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Excel's binary search for an approximate match in ascending data: the last position whose value is not
/// greater than the target, among values of the target's type.
fn approximate(values: &[Value], target: &Value, descending: bool) -> Option<usize> {
    if values.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i64, values.len() as i64 - 1);
    let mut found = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        // Skip down over values of another type, as Excel does.
        let mut probe = mid;
        while probe >= lo && !same_type(&values[probe as usize], target) {
            probe -= 1;
        }
        if probe < lo {
            lo = mid + 1;
            continue;
        }
        let order = compare(&values[probe as usize], target);
        let not_past =
            if descending { order != Ordering::Less } else { order != Ordering::Greater };
        if not_past {
            found = Some(probe as usize);
            lo = mid + 1;
        } else {
            hi = probe - 1;
        }
    }
    found
}

fn vh_lookup(ctx: &mut Ctx, args: &[Expr], vertical: bool) -> Val {
    let target = scalar(ctx, &args[0]);
    if let Value::Error(e) = target {
        return Val::error(e);
    }
    let table = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let index = match number_arg(ctx, args, 2, 1.0) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return Val::error(e),
    };
    let approx = match args.get(3) {
        None => true,
        Some(Expr::Missing) => false,
        Some(e) => {
            let v = scalar(ctx, e);
            match crate::eval::to_bool(&v) {
                Ok(b) => b,
                Err(e) => return Val::error(e),
            }
        }
    };
    let lines = if vertical { table.cols } else { table.rows };
    if index < 1 {
        return Val::error(ErrorKind::Value);
    }
    let keys: Vec<Value> = if vertical {
        (0..table.rows).map(|r| table.get(r, 0).clone()).collect()
    } else {
        (0..table.cols).map(|c| table.get(0, c).clone()).collect()
    };
    let hit = if approx {
        approximate(&keys, &target, false)
    } else {
        keys.iter().position(|k| matches_exactly(&target, k, true))
    };
    match hit {
        // Measured: a value that is not found is #N/A even when the index is past the table.
        Some(_) if index as usize > lines => Val::error(ErrorKind::Ref),
        Some(i) => {
            let v = if vertical {
                table.get(i, index as usize - 1)
            } else {
                table.get(index as usize - 1, i)
            };
            Val::Value(v.clone())
        }
        None => Val::error(ErrorKind::NA),
    }
}

fn lookup(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let target = scalar(ctx, &args[0]);
    let vector = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    // The array form looks along the longer side's first line and returns from its last.
    let (keys, results): (Vec<Value>, Vec<Value>) = match args.get(2) {
        Some(e) => {
            let result = match array_of(ctx, e) {
                Ok(a) => a,
                Err(e) => return Val::error(e),
            };
            (vector.data.clone(), result.data)
        }
        None => {
            if vector.cols > vector.rows {
                (
                    (0..vector.cols).map(|c| vector.get(0, c).clone()).collect(),
                    (0..vector.cols).map(|c| vector.get(vector.rows - 1, c).clone()).collect(),
                )
            } else {
                (
                    (0..vector.rows).map(|r| vector.get(r, 0).clone()).collect(),
                    (0..vector.rows).map(|r| vector.get(r, vector.cols - 1).clone()).collect(),
                )
            }
        }
    };
    match approximate(&keys, &target, false) {
        Some(i) => Val::Value(results.get(i).cloned().unwrap_or(Value::Error(ErrorKind::NA))),
        None => Val::error(ErrorKind::NA),
    }
}

/// Find a value in a list by XLOOKUP's match and search modes.
fn find_in(values: &[Value], target: &Value, match_mode: i64, search_mode: i64) -> Option<usize> {
    let order: Box<dyn Iterator<Item = usize>> = if search_mode == -1 || search_mode == -2 {
        Box::new((0..values.len()).rev())
    } else {
        Box::new(0..values.len())
    };
    match search_mode {
        2 | -2 => {
            // Binary search over sorted data.
            let descending = search_mode == -2;
            let hit = approximate(values, target, descending)?;
            return match match_mode {
                0 | 2 => (matches_exactly(target, &values[hit], match_mode == 2)).then_some(hit),
                -1 => Some(hit),
                _ => {
                    if matches_exactly(target, &values[hit], false) {
                        Some(hit)
                    } else if hit + 1 < values.len() {
                        Some(hit + 1)
                    } else {
                        None
                    }
                }
            };
        }
        _ => {}
    }
    let mut best: Option<usize> = None;
    for i in order {
        let v = &values[i];
        match match_mode {
            0 => {
                if matches_exactly(target, v, false) {
                    return Some(i);
                }
            }
            2 => {
                if matches_exactly(target, v, true) {
                    return Some(i);
                }
            }
            -1 | 1 => {
                if matches_exactly(target, v, false) {
                    return Some(i);
                }
                if !same_type(v, target) {
                    continue;
                }
                let order = compare(v, target);
                let fits = if match_mode == -1 {
                    order == Ordering::Less
                } else {
                    order == Ordering::Greater
                };
                if fits {
                    let better = match best {
                        None => true,
                        Some(b) => {
                            let o = compare(v, &values[b]);
                            if match_mode == -1 {
                                o == Ordering::Greater
                            } else {
                                o == Ordering::Less
                            }
                        }
                    };
                    if better {
                        best = Some(i);
                    }
                }
            }
            _ => return None,
        }
    }
    best
}

fn xlookup(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let target = ctx.eval(&args[0]);
    let lookup = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    if lookup.rows != 1 && lookup.cols != 1 {
        return Val::error(ErrorKind::Value);
    }
    let returns = match array_of(ctx, &args[2]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let vertical = lookup.cols == 1 && !(lookup.rows == 1 && returns.rows == 1 && returns.cols > 1);
    if vertical && returns.rows != lookup.rows || !vertical && returns.cols != lookup.cols {
        return Val::error(ErrorKind::Value);
    }
    let not_found = args.get(3).filter(|e| !matches!(e, Expr::Missing)).map(|e| ctx.eval(e));
    let match_mode = number_arg(ctx, args, 4, 0.0).unwrap_or(0.0).trunc() as i64;
    let search_mode = number_arg(ctx, args, 5, 1.0).unwrap_or(1.0).trunc() as i64;
    if !matches!(match_mode, -1..=2) || !matches!(search_mode, -2 | -1 | 1 | 2) {
        return Val::error(ErrorKind::Value);
    }
    let one = |ctx: &mut Ctx, t: &Value| -> Val {
        if let Value::Error(e) = t {
            return Val::error(*e);
        }
        match find_in(&lookup.data, t, match_mode, search_mode) {
            Some(i) => {
                if vertical {
                    if returns.cols == 1 {
                        Val::Value(returns.get(i, 0).clone())
                    } else {
                        Val::Array(Array::row(
                            (0..returns.cols).map(|c| returns.get(i, c).clone()).collect(),
                        ))
                    }
                } else if returns.rows == 1 {
                    Val::Value(returns.get(0, i).clone())
                } else {
                    Val::Array(Array::column(
                        (0..returns.rows).map(|r| returns.get(r, i).clone()).collect(),
                    ))
                }
            }
            None => match &not_found {
                Some(v) => v.clone(),
                None => {
                    let _ = ctx;
                    Val::error(ErrorKind::NA)
                }
            },
        }
    };
    match ctx.dims(&target) {
        Some((1, 1)) => {
            let t = ctx.element(&target, 0, 0);
            one(ctx, &t)
        }
        _ => ctx.lift_vals(vec![target], |ctx, t| {
            let v = one(ctx, &t[0]);
            ctx.single(v)
        }),
    }
}

fn match_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let target = ctx.eval(&args[0]);
    let list = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    if list.rows != 1 && list.cols != 1 {
        return Val::error(ErrorKind::NA);
    }
    let kind = match number_arg(ctx, args, 2, 1.0) {
        Ok(n) => n.signum() as i64 * i64::from(n != 0.0),
        Err(e) => return Val::error(e),
    };
    ctx.lift_vals(vec![target], |_, t| {
        let t = &t[0];
        if let Value::Error(e) = t {
            return Value::Error(*e);
        }
        let hit = match kind {
            0 => list.data.iter().position(|v| matches_exactly(t, v, true)),
            1 => approximate(&list.data, t, false),
            _ => approximate(&list.data, t, true),
        };
        hit.map_or(Value::Error(ErrorKind::NA), |i| Value::Number(i as f64 + 1.0))
    })
}

fn xmatch(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let target = ctx.eval(&args[0]);
    let list = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    if list.rows != 1 && list.cols != 1 {
        return Val::error(ErrorKind::Value);
    }
    let match_mode = number_arg(ctx, args, 2, 0.0).unwrap_or(0.0).trunc() as i64;
    let search_mode = number_arg(ctx, args, 3, 1.0).unwrap_or(1.0).trunc() as i64;
    ctx.lift_vals(vec![target], |_, t| {
        if let Value::Error(e) = &t[0] {
            return Value::Error(*e);
        }
        find_in(&list.data, &t[0], match_mode, search_mode)
            .map_or(Value::Error(ErrorKind::NA), |i| Value::Number(i as f64 + 1.0))
    })
}

fn index(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let source = ctx.eval(&args[0]);
    let row = match number_arg(ctx, args, 1, 0.0) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return Val::error(e),
    };
    let col = match number_arg(ctx, args, 2, 0.0) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return Val::error(e),
    };
    let area_num = match number_arg(ctx, args, 3, 1.0) {
        Ok(n) => n.trunc() as usize,
        Err(e) => return Val::error(e),
    };
    if row < 0 || col < 0 {
        return Val::error(ErrorKind::Value);
    }
    let area = match source {
        Val::Ref(a) => Some(a),
        Val::Refs(areas) => match areas.get(area_num.wrapping_sub(1)) {
            Some(a) => Some(*a),
            None => return Val::error(ErrorKind::Ref),
        },
        Val::Value(Value::Error(e)) => return Val::error(e),
        _ => None,
    };
    match area {
        Some(area) => {
            let r = area.range;
            let (rows, cols) = (r.rows() as i64, r.cols() as i64);
            // One row or one column: a single number is the position along it.
            let (row, col) = if args.len() == 2 && rows == 1 { (1, row) } else { (row, col) };
            if row > rows || col > cols {
                return Val::error(ErrorKind::Ref);
            }
            let (first_row, last_row) = if row == 0 {
                (r.first.row, r.last.row)
            } else {
                (r.first.row + row as u32 - 1, r.first.row + row as u32 - 1)
            };
            let (first_col, last_col) = if col == 0 {
                (r.first.col, r.last.col)
            } else {
                (r.first.col + col as u32 - 1, r.first.col + col as u32 - 1)
            };
            Val::Ref(Area::new(
                area.sheet,
                Range::new(CellRef::new(first_row, first_col), CellRef::new(last_row, last_col)),
            ))
        }
        None => {
            let again = source_again(ctx, &args[0]);
            let a = match ctx.array(again) {
                Ok(a) => a,
                Err(e) => return Val::error(e),
            };
            let (row, col) = if args.len() == 2 && a.rows == 1 {
                (1, row)
            } else if args.len() == 2 && a.cols == 1 {
                (row, 1)
            } else {
                (row, col)
            };
            if row as usize > a.rows || col as usize > a.cols {
                return Val::error(ErrorKind::Ref);
            }
            match (row, col) {
                (0, 0) => Val::Array(a),
                (0, c) => Val::Array(Array::column(
                    (0..a.rows).map(|r| a.get(r, c as usize - 1).clone()).collect(),
                )),
                (r, 0) => Val::Array(Array::row(
                    (0..a.cols).map(|c| a.get(r as usize - 1, c).clone()).collect(),
                )),
                (r, c) => Val::Value(a.get(r as usize - 1, c as usize - 1).clone()),
            }
        }
    }
}

fn source_again(ctx: &mut Ctx, e: &Expr) -> Val {
    ctx.eval(e)
}

fn choose(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let index = ctx.eval(&args[0]);
    match ctx.dims(&index) {
        Some((1, 1)) => {
            let v = ctx.element(&index, 0, 0);
            match ctx.number(&v) {
                Ok(n) => {
                    let i = n.trunc() as usize;
                    if i < 1 || i >= args.len() {
                        return Val::error(ErrorKind::Value);
                    }
                    ctx.eval(&args[i])
                }
                Err(e) => Val::error(e),
            }
        }
        _ => {
            let options: Vec<Val> = args[1..].iter().map(|e| ctx.eval(e)).collect();
            let mut all = vec![index];
            all.extend(options);
            ctx.lift_vals(all, |ctx, v| match ctx.number(&v[0]) {
                Ok(n) => {
                    let i = n.trunc() as usize;
                    v.get(i).filter(|_| i >= 1).cloned().unwrap_or(Value::Error(ErrorKind::Value))
                }
                Err(e) => Value::Error(e),
            })
        }
    }
}

fn choose_lines(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let count = if rows { a.rows } else { a.cols } as i64;
    let mut picks = Vec::new();
    for e in &args[1..] {
        let v = ctx.eval(e);
        let numbers = match ctx.array(v) {
            Ok(n) => n,
            Err(e) => return Val::error(e),
        };
        for n in numbers.data {
            let n = match ctx.number(&n) {
                Ok(n) => n.trunc() as i64,
                Err(e) => return Val::error(e),
            };
            let i = if n < 0 { count + n } else { n - 1 };
            if n == 0 || i < 0 || i >= count {
                return Val::error(ErrorKind::Value);
            }
            picks.push(i as usize);
        }
    }
    let mut data = Vec::new();
    if rows {
        for r in &picks {
            for c in 0..a.cols {
                data.push(a.get(*r, c).clone());
            }
        }
        Val::Array(Array::new(picks.len(), a.cols, data))
    } else {
        for r in 0..a.rows {
            for c in &picks {
                data.push(a.get(r, *c).clone());
            }
        }
        Val::Array(Array::new(a.rows, picks.len(), data))
    }
}

fn offset(ctx: &mut Ctx, args: &[Expr]) -> Val {
    ctx.dynamic = true;
    let area = match ctx.eval(&args[0]) {
        Val::Ref(a) => a,
        Val::Value(Value::Error(e)) => return Val::error(e),
        _ => return Val::error(ErrorKind::Value),
    };
    let mut n = [0.0; 4];
    let defaults = [0.0, 0.0, area.range.rows() as f64, area.range.cols() as f64];
    for (i, slot) in n.iter_mut().enumerate() {
        match number_arg(ctx, args, i + 1, defaults[i]) {
            Ok(v) => *slot = v.trunc(),
            Err(e) => return Val::error(e),
        }
    }
    let (rows, cols, height, width) = (n[0] as i64, n[1] as i64, n[2] as i64, n[3] as i64);
    if height == 0 || width == 0 {
        return Val::error(ErrorKind::Ref);
    }
    let top = area.range.first.row as i64 + rows;
    let left = area.range.first.col as i64 + cols;
    let (top, bottom) = if height > 0 { (top, top + height - 1) } else { (top + height + 1, top) };
    let (left, right) = if width > 0 { (left, left + width - 1) } else { (left + width + 1, left) };
    if top < 0 || left < 0 || bottom >= MAX_ROWS as i64 || right >= MAX_COLS as i64 {
        return Val::error(ErrorKind::Ref);
    }
    Val::Ref(Area::new(
        area.sheet,
        Range::new(
            CellRef::new(top as u32, left as u32),
            CellRef::new(bottom as u32, right as u32),
        ),
    ))
}

fn indirect(ctx: &mut Ctx, args: &[Expr]) -> Val {
    ctx.dynamic = true;
    let text = scalar(ctx, &args[0]);
    let text = match crate::eval::to_text(&text) {
        Ok(t) => t,
        Err(e) => return Val::error(e),
    };
    let a1 = match args.get(1) {
        Some(e) => {
            let v = scalar(ctx, e);
            crate::eval::to_bool(&v).unwrap_or(true)
        }
        None => true,
    };
    let written = if a1 {
        text.trim().to_owned()
    } else {
        match r1c1_to_a1(text.trim(), ctx.at) {
            Some(t) => t,
            None => return Val::error(ErrorKind::Ref),
        }
    };
    match crate::formula::parser::parse(&written) {
        Ok(expr @ (Expr::Ref(_) | Expr::Name(_) | Expr::Table(_))) => match ctx.eval(&expr) {
            v @ Val::Ref(_) => v,
            _ => Val::error(ErrorKind::Ref),
        },
        _ => Val::error(ErrorKind::Ref),
    }
}

/// `R2C3`, `R[1]C[-1]`, `Sheet1!R1C1:R2C2` written as A1.
fn r1c1_to_a1(text: &str, at: CellRef) -> Option<String> {
    let (sheet, body) = match text.rsplit_once('!') {
        Some((s, b)) => (Some(s), b),
        None => (None, text),
    };
    let one = |part: &str| -> Option<String> {
        let upper = part.to_ascii_uppercase();
        let rest = upper.strip_prefix('R')?;
        let c_at = rest.find('C')?;
        let (row_part, col_part) = (&rest[..c_at], &rest[c_at + 1..]);
        let resolve = |p: &str, base: u32| -> Option<(u32, bool)> {
            if p.is_empty() {
                return Some((base, false));
            }
            if let Some(inner) = p.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                let off: i64 = inner.parse().ok()?;
                let v = base as i64 + off;
                return (v >= 0).then_some((v as u32, false));
            }
            let v: u32 = p.parse().ok()?;
            (v >= 1).then_some((v - 1, true))
        };
        let (row, row_abs) = resolve(row_part, at.row)?;
        let (col, col_abs) = resolve(col_part, at.col)?;
        Some(format!(
            "{}{}{}{}",
            if col_abs { "$" } else { "" },
            crate::coord::column_name(col),
            if row_abs { "$" } else { "" },
            row + 1
        ))
    };
    let a1 = match body.split_once(':') {
        Some((a, b)) => format!("{}:{}", one(a)?, one(b)?),
        None => one(body)?,
    };
    Some(match sheet {
        Some(s) => format!("{s}!{a1}"),
        None => a1,
    })
}

fn address(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let row = number_arg(ctx, args, 0, 0.0).map(f64::trunc);
    let col = number_arg(ctx, args, 1, 0.0).map(f64::trunc);
    let abs = number_arg(ctx, args, 2, 1.0).map(f64::trunc);
    let (row, col, abs) = match (row, col, abs) {
        (Ok(r), Ok(c), Ok(a)) => (r, c, a as i64),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return Val::error(e),
    };
    let a1 = match args.get(3).filter(|e| !matches!(e, Expr::Missing)) {
        Some(e) => {
            let v = scalar(ctx, e);
            crate::eval::to_bool(&v).unwrap_or(true)
        }
        None => true,
    };
    if row < 1.0
        || col < 1.0
        || row > MAX_ROWS as f64
        || col > MAX_COLS as f64
        || !(1..=4).contains(&abs)
    {
        return Val::error(ErrorKind::Value);
    }
    let (row_abs, col_abs) = match abs {
        1 => (true, true),
        2 => (true, false),
        3 => (false, true),
        _ => (false, false),
    };
    let body = if a1 {
        format!(
            "{}{}{}{}",
            if col_abs { "$" } else { "" },
            crate::coord::column_name(col as u32 - 1),
            if row_abs { "$" } else { "" },
            row as u32
        )
    } else {
        format!(
            "R{}C{}",
            if row_abs { format!("{}", row as u32) } else { format!("[{}]", row as u32) },
            if col_abs { format!("{}", col as u32) } else { format!("[{}]", col as u32) }
        )
    };
    match args.get(4) {
        Some(e) => {
            let v = scalar(ctx, e);
            let name = v.to_text();
            Val::text(format!("{}!{body}", crate::formula::print::sheet_name(&name)))
        }
        None => Val::text(body),
    }
}

fn row_col(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Val {
    let area = match args.first() {
        None => return Val::number(if rows { ctx.at.row } else { ctx.at.col } as f64 + 1.0),
        Some(e) => match ctx.eval(e) {
            Val::Ref(a) => a,
            Val::Value(Value::Error(e)) => return Val::error(e),
            _ => return Val::error(ErrorKind::Value),
        },
    };
    let r = area.range;
    if rows {
        if r.rows() == 1 || r.is_whole_columns() {
            return Val::number(r.first.row as f64 + 1.0);
        }
        Val::Array(Array::column(
            (r.first.row..=r.last.row).map(|x| Value::Number(x as f64 + 1.0)).collect(),
        ))
    } else {
        if r.cols() == 1 || r.is_whole_rows() {
            return Val::number(r.first.col as f64 + 1.0);
        }
        Val::Array(Array::row(
            (r.first.col..=r.last.col).map(|x| Value::Number(x as f64 + 1.0)).collect(),
        ))
    }
}

fn count_lines(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Val {
    let v = ctx.eval(&args[0]);
    match ctx.dims(&v) {
        Some((r, c)) => Val::number(if rows { r } else { c } as f64),
        None => match v {
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        },
    }
}

fn filter(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let include = match array_of(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let by_rows = include.cols == 1 && include.rows == a.rows;
    let by_cols = include.rows == 1 && include.cols == a.cols;
    if !by_rows && !by_cols {
        return Val::error(ErrorKind::Value);
    }
    let mut keep = Vec::new();
    for v in &include.data {
        match v {
            Value::Error(e) => return Val::error(*e),
            v => keep.push(
                crate::eval::to_bool(v).unwrap_or(false)
                    || matches!(v, Value::Number(n) if *n != 0.0),
            ),
        }
    }
    let mut data = Vec::new();
    let result = if by_rows {
        let rows: Vec<usize> = (0..a.rows).filter(|r| keep[*r]).collect();
        for r in &rows {
            for c in 0..a.cols {
                data.push(a.get(*r, c).clone());
            }
        }
        Array::new(rows.len(), a.cols, data)
    } else {
        let cols: Vec<usize> = (0..a.cols).filter(|c| keep[*c]).collect();
        for r in 0..a.rows {
            for c in &cols {
                data.push(a.get(r, *c).clone());
            }
        }
        Array::new(a.rows, cols.len(), data)
    };
    if result.data.is_empty() {
        return match args.get(2) {
            Some(e) => ctx.eval(e),
            None => Val::error(ErrorKind::Calc),
        };
    }
    Val::Array(result)
}

/// Order rows (or columns) by keys, each ascending or descending, stably.
fn order_by(count: usize, keys: &[(Vec<Value>, bool)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&x, &y| {
        for (key, descending) in keys {
            let o = sort_compare(&key[x], &key[y]);
            if o != Ordering::Equal {
                return if *descending { o.reverse() } else { o };
            }
        }
        Ordering::Equal
    });
    order
}

/// The order SORT uses: numbers, text, logicals, errors, then empty cells last.
pub fn sort_compare(a: &Value, b: &Value) -> Ordering {
    let rank = |v: &Value| match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        Value::Empty => 4,
    };
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => crate::eval::compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

fn sort(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let by_col = match args.get(3) {
        Some(e) => {
            let v = scalar(ctx, e);
            crate::eval::to_bool(&v).unwrap_or(false)
        }
        None => false,
    };
    let indexes =
        match args.get(1).filter(|e| !matches!(e, Expr::Missing)).map(|e| array_of(ctx, e)) {
            Some(Ok(x)) => x
                .data
                .iter()
                .map(|v| ctx.number(v).map(|n| n.trunc() as i64))
                .collect::<Result<Vec<_>, _>>(),
            Some(Err(e)) => Err(e),
            None => Ok(vec![1]),
        };
    let orders = match args.get(2).filter(|e| !matches!(e, Expr::Missing)).map(|e| array_of(ctx, e))
    {
        Some(Ok(x)) => x
            .data
            .iter()
            .map(|v| ctx.number(v).map(|n| n.trunc() as i64))
            .collect::<Result<Vec<_>, _>>(),
        Some(Err(e)) => Err(e),
        None => Ok(vec![1]),
    };
    let (indexes, orders) = match (indexes, orders) {
        (Ok(i), Ok(o)) => (i, o),
        (Err(e), _) | (_, Err(e)) => return Val::error(e),
    };
    let lines = if by_col { a.rows } else { a.cols };
    let count = if by_col { a.cols } else { a.rows };
    let mut keys = Vec::new();
    for (k, index) in indexes.iter().enumerate() {
        if *index < 1 || *index as usize > lines {
            return Val::error(ErrorKind::Value);
        }
        let order = orders.get(k).or(orders.last()).copied().unwrap_or(1);
        if order != 1 && order != -1 {
            return Val::error(ErrorKind::Value);
        }
        let key: Vec<Value> = (0..count)
            .map(|i| {
                if by_col {
                    a.get(*index as usize - 1, i).clone()
                } else {
                    a.get(i, *index as usize - 1).clone()
                }
            })
            .collect();
        keys.push((key, order == -1));
    }
    let order = order_by(count, &keys);
    Val::Array(reorder(&a, &order, by_col))
}

fn reorder(a: &Array, order: &[usize], by_col: bool) -> Array {
    let mut data = Vec::with_capacity(a.data.len());
    if by_col {
        for r in 0..a.rows {
            for c in order {
                data.push(a.get(r, *c).clone());
            }
        }
    } else {
        for r in order {
            for c in 0..a.cols {
                data.push(a.get(*r, c).clone());
            }
        }
    }
    Array::new(a.rows, a.cols, data)
}

fn sortby(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let mut keys = Vec::new();
    let mut by_col = None;
    let mut i = 1;
    while i < args.len() {
        let key = match array_of(ctx, &args[i]) {
            Ok(k) => k,
            Err(e) => return Val::error(e),
        };
        let order = match number_arg(ctx, args, i + 1, 1.0) {
            Ok(o) => o.trunc() as i64,
            Err(e) => return Val::error(e),
        };
        if order != 1 && order != -1 {
            return Val::error(ErrorKind::Value);
        }
        let cols = key.rows == 1 && key.cols == a.cols && a.rows != a.cols
            || key.rows == 1 && key.cols > 1;
        let this_by_col = cols && key.cols == a.cols;
        if !this_by_col && (key.cols != 1 || key.rows != a.rows) {
            return Val::error(ErrorKind::Value);
        }
        if by_col.is_some_and(|b| b != this_by_col) {
            return Val::error(ErrorKind::Value);
        }
        by_col = Some(this_by_col);
        keys.push((key.data, order == -1));
        i += 2;
    }
    let by_col = by_col.unwrap_or(false);
    let count = if by_col { a.cols } else { a.rows };
    let order = order_by(count, &keys);
    Val::Array(reorder(&a, &order, by_col))
}

fn unique(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let flag = |ctx: &mut Ctx, i: usize| -> bool {
        args.get(i).filter(|e| !matches!(e, Expr::Missing)).is_some_and(|e| {
            let v = scalar(ctx, e);
            crate::eval::to_bool(&v).unwrap_or(false)
        })
    };
    let by_col = flag(ctx, 1);
    let once = flag(ctx, 2);
    let count = if by_col { a.cols } else { a.rows };
    let line = |i: usize| -> Vec<Value> {
        if by_col {
            (0..a.rows).map(|r| a.get(r, i).clone()).collect()
        } else {
            (0..a.cols).map(|c| a.get(i, c).clone()).collect()
        }
    };
    let same = |x: &[Value], y: &[Value]| {
        x.iter().zip(y).all(|(p, q)| {
            equal(p, q) && std::mem::discriminant(p) == std::mem::discriminant(q)
                || p.is_empty() && q.is_empty()
        })
    };
    let mut seen: Vec<(Vec<Value>, usize)> = Vec::new();
    for i in 0..count {
        let l = line(i);
        match seen.iter_mut().find(|(s, _)| same(s, &l)) {
            Some((_, n)) => *n += 1,
            None => seen.push((l, 1)),
        }
    }
    let kept: Vec<Vec<Value>> =
        seen.into_iter().filter(|(_, n)| !once || *n == 1).map(|(l, _)| l).collect();
    if kept.is_empty() {
        return Val::error(ErrorKind::Calc);
    }
    if by_col {
        let rows = a.rows;
        let mut data = Vec::new();
        for r in 0..rows {
            for l in &kept {
                data.push(l[r].clone());
            }
        }
        Val::Array(Array::new(rows, kept.len(), data))
    } else {
        Val::Array(Array::from_rows(kept))
    }
}

fn take_drop(ctx: &mut Ctx, args: &[Expr], take: bool) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let rows = match args.get(1).filter(|e| !matches!(e, Expr::Missing)) {
        Some(_) => match number_arg(ctx, args, 1, 0.0) {
            Ok(n) => Some(n.trunc() as i64),
            Err(e) => return Val::error(e),
        },
        None => None,
    };
    let cols = match args.get(2).filter(|e| !matches!(e, Expr::Missing)) {
        Some(_) => match number_arg(ctx, args, 2, 0.0) {
            Ok(n) => Some(n.trunc() as i64),
            Err(e) => return Val::error(e),
        },
        None => None,
    };
    let span = |n: Option<i64>, len: usize| -> (usize, usize) {
        let len_i = len as i64;
        match n {
            None => (0, len),
            Some(n) if take => {
                if n >= 0 {
                    (0, n.min(len_i) as usize)
                } else {
                    ((len_i + n).max(0) as usize, len)
                }
            }
            Some(n) => {
                if n >= 0 {
                    (n.min(len_i) as usize, len)
                } else {
                    (0, (len_i + n).max(0) as usize)
                }
            }
        }
    };
    let (r0, r1) = span(rows, a.rows);
    let (c0, c1) = span(cols, a.cols);
    if r1 <= r0 || c1 <= c0 {
        return Val::error(ErrorKind::Calc);
    }
    let mut data = Vec::new();
    for r in r0..r1 {
        for c in c0..c1 {
            data.push(a.get(r, c).clone());
        }
    }
    Val::Array(Array::new(r1 - r0, c1 - c0, data))
}

fn stack(ctx: &mut Ctx, args: &[Expr], vertical: bool) -> Val {
    let mut arrays = Vec::new();
    for e in args {
        match array_of(ctx, e) {
            Ok(a) => arrays.push(a),
            Err(e) => return Val::error(e),
        }
    }
    let na = Value::Error(ErrorKind::NA);
    if vertical {
        let cols = arrays.iter().map(|a| a.cols).max().unwrap_or(0);
        let rows: usize = arrays.iter().map(|a| a.rows).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for a in &arrays {
            for r in 0..a.rows {
                for c in 0..cols {
                    data.push(if c < a.cols { a.get(r, c).clone() } else { na.clone() });
                }
            }
        }
        Val::Array(Array::new(rows, cols, data))
    } else {
        let rows = arrays.iter().map(|a| a.rows).max().unwrap_or(0);
        let cols: usize = arrays.iter().map(|a| a.cols).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for a in &arrays {
                for c in 0..a.cols {
                    data.push(if r < a.rows { a.get(r, c).clone() } else { na.clone() });
                }
            }
        }
        Val::Array(Array::new(rows, cols, data))
    }
}

fn to_line(ctx: &mut Ctx, args: &[Expr], column: bool) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let ignore = number_arg(ctx, args, 1, 0.0).unwrap_or(0.0) as i64;
    let by_col = args.get(2).is_some_and(|e| {
        let v = scalar(ctx, e);
        crate::eval::to_bool(&v).unwrap_or(false)
    });
    let mut values = Vec::new();
    let mut push = |v: &Value| {
        let skip = match ignore {
            1 => v.is_empty(),
            2 => v.is_error(),
            3 => v.is_empty() || v.is_error(),
            _ => false,
        };
        if !skip {
            values.push(if v.is_empty() { Value::Number(0.0) } else { v.clone() });
        }
    };
    if by_col {
        for c in 0..a.cols {
            for r in 0..a.rows {
                push(a.get(r, c));
            }
        }
    } else {
        for v in &a.data {
            push(v);
        }
    }
    if values.is_empty() {
        return Val::error(ErrorKind::Calc);
    }
    Val::Array(if column { Array::column(values) } else { Array::row(values) })
}

fn wrap(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    if a.rows != 1 && a.cols != 1 {
        return Val::error(ErrorKind::Value);
    }
    let n = match number_arg(ctx, args, 1, 0.0) {
        Ok(n) if n >= 1.0 => n as usize,
        Ok(_) => return Val::error(ErrorKind::Num),
        Err(e) => return Val::error(e),
    };
    let pad = args.get(2).map(|e| scalar(ctx, e)).unwrap_or(Value::Error(ErrorKind::NA));
    let lines = a.data.len().div_ceil(n);
    let mut data = vec![pad; lines * n];
    for (i, v) in a.data.iter().enumerate() {
        let (line, at) = (i / n, i % n);
        let index = if rows { line * n + at } else { at * lines + line };
        data[index] = v.clone();
    }
    Val::Array(if rows { Array::new(lines, n, data) } else { Array::new(n, lines, data) })
}

fn expand(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let a = match array_of(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let rows = number_arg(ctx, args, 1, a.rows as f64).unwrap_or(a.rows as f64) as usize;
    let cols = number_arg(ctx, args, 2, a.cols as f64).unwrap_or(a.cols as f64) as usize;
    if rows < a.rows || cols < a.cols {
        return Val::error(ErrorKind::Value);
    }
    let pad = args.get(3).map(|e| scalar(ctx, e)).unwrap_or(Value::Error(ErrorKind::NA));
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            data.push(if r < a.rows && c < a.cols { a.get(r, c).clone() } else { pad.clone() });
        }
    }
    Val::Array(Array::new(rows, cols, data))
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{book, eval_on, shown};

    #[test]
    fn lookups_answer_what_excel_answers() {
        let b = book(&[
            ("A1", "apple"),
            ("B1", "1"),
            ("A2", "banana"),
            ("B2", "2"),
            ("A3", "cherry"),
            ("B3", "3"),
            ("D1", "10"),
            ("D2", "20"),
            ("D3", "30"),
        ]);
        let cases = [
            ("VLOOKUP(\"banana\",A1:B3,2,FALSE)", "2"),
            ("VLOOKUP(\"BAN*\",A1:B3,2,FALSE)", "2"),
            ("VLOOKUP(\"kiwi\",A1:B3,2,FALSE)", "#N/A"),
            ("VLOOKUP(25,D1:D3,1)", "20"),
            ("VLOOKUP(5,D1:D3,1)", "#N/A"),
            ("VLOOKUP(\"a\",A1:B3,3,FALSE)", "#N/A"),
            ("HLOOKUP(2,{1,2,3;\"a\",\"b\",\"c\"},2,FALSE)", "b"),
            ("MATCH(\"cherry\",A1:A3,0)", "3"),
            ("MATCH(25,D1:D3)", "2"),
            ("MATCH(25,{30,20,10},-1)", "1"),
            ("INDEX(A1:B3,2,2)", "2"),
            ("INDEX(D1:D3,3)", "30"),
            ("SUM(INDEX(A1:D3,0,4))", "60"),
            ("XLOOKUP(\"cherry\",A1:A3,B1:B3)", "3"),
            ("XLOOKUP(\"kiwi\",A1:A3,B1:B3,\"none\")", "none"),
            ("XLOOKUP(25,D1:D3,A1:A3,,-1)", "banana"),
            ("XLOOKUP(25,D1:D3,A1:A3,,1)", "cherry"),
            ("XMATCH(30,D1:D3)", "3"),
            ("CHOOSE(2,\"a\",\"b\",\"c\")", "b"),
            ("OFFSET(A1,1,1)", "2"),
            ("SUM(OFFSET(D1,0,0,3,1))", "60"),
            ("INDIRECT(\"B\"&2)", "2"),
            ("INDIRECT(\"R3C2\",FALSE)", "3"),
            ("ADDRESS(2,3)", "$C$2"),
            ("ADDRESS(2,3,4)", "C2"),
            ("ADDRESS(2,3,1,FALSE)", "R2C3"),
            ("ROW(B7)", "7"),
            ("COLUMN(C1)", "3"),
            ("ROWS(A1:B3)", "3"),
            ("COLUMNS(A1:B3)", "2"),
            ("LOOKUP(2.5,B1:B3,A1:A3)", "banana"),
            ("SUM(FILTER(D1:D3,D1:D3>15))", "50"),
            ("INDEX(SORT(D1:D3,1,-1),1)", "30"),
            ("INDEX(SORTBY(A1:A3,D1:D3,-1),1)", "cherry"),
            ("ROWS(UNIQUE({1;2;2;3}))", "3"),
            ("SUM(TAKE(D1:D3,2))", "30"),
            ("SUM(DROP(D1:D3,1))", "50"),
            ("ROWS(VSTACK(D1:D3,D1:D3))", "6"),
            ("COLUMNS(HSTACK(D1:D3,B1:B3))", "2"),
            ("ROWS(TOCOL(A1:B3))", "6"),
            ("COLUMNS(WRAPROWS(SEQUENCE(1,6),4))", "4"),
            ("SUM(TRANSPOSE(D1:D3))", "60"),
            ("SUM(CHOOSECOLS(A1:D3,2,4))", "66"),
            ("ROWS(EXPAND(D1:D3,5))", "5"),
        ];
        let mut wrong = Vec::new();
        for (f, want) in cases {
            let got = shown(&eval_on(&b, f));
            if got != want {
                wrong.push(format!("{f}: expected {want}, got {got}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
