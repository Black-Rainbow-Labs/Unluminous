//! Logical functions, LET, LAMBDA and the functions that call a LAMBDA over an array.
//!
//! IF, IFS, IFERROR, IFNA, CHOOSE and SWITCH evaluate only the branch they take when the condition is a
//! single value, which is what makes `=IF(A1=0,0,1/A1)` answer 0 rather than `#DIV/0!` and what keeps a
//! recursive LAMBDA from running for ever. Given an array condition they take both branches and choose
//! value by value.

use std::sync::Arc;

use super::{r, s, Function, MANY};
use crate::eval::{cell_name, equal, to_bool, Ctx, Lambda, Val};
use crate::formula::{Expr, Shape};
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r(
        "IF",
        1,
        3,
        "logical_test, [value_if_true], [value_if_false]",
        "One value if a test is true and another if it is false.",
        if_fn,
    ),
    r(
        "IFS",
        2,
        MANY,
        "logical_test1, value_if_true1, ...",
        "The value for the first test that is true.",
        ifs,
    ),
    r(
        "IFERROR",
        2,
        2,
        "value, value_if_error",
        "A value, or another one when it is an error.",
        |ctx, a| if_error(ctx, a, false),
    ),
    r("IFNA", 2, 2, "value, value_if_na", "A value, or another one when it is #N/A.", |ctx, a| {
        if_error(ctx, a, true)
    }),
    r(
        "AND",
        1,
        MANY,
        "logical1, [logical2], ...",
        "TRUE when every argument is true.",
        |ctx, a| logicals(ctx, a, |t, n| t == n),
    ),
    r("OR", 1, MANY, "logical1, [logical2], ...", "TRUE when any argument is true.", |ctx, a| {
        logicals(ctx, a, |t, _| t > 0)
    }),
    r(
        "XOR",
        1,
        MANY,
        "logical1, [logical2], ...",
        "TRUE when an odd number of arguments are true.",
        |ctx, a| logicals(ctx, a, |t, _| t % 2 == 1),
    ),
    s("NOT", 1, 1, "logical", "The opposite of a logical.", |_, a| {
        to_bool(&a[0]).map(|b| Value::Bool(!b))
    }),
    s("TRUE", 0, 0, "", "The logical value TRUE.", |_, _| Ok(Value::Bool(true))),
    s("FALSE", 0, 0, "", "The logical value FALSE.", |_, _| Ok(Value::Bool(false))),
    r(
        "SWITCH",
        3,
        MANY,
        "expression, value1, result1, ..., [default]",
        "The result for the first value that matches.",
        switch,
    ),
    r(
        "LET",
        3,
        MANY,
        "name1, value1, ..., calculation",
        "Names values to use in a calculation.",
        let_fn,
    ),
    r(
        "LAMBDA",
        1,
        MANY,
        "[parameter1, ...], calculation",
        "A function made from a calculation.",
        lambda,
    ),
    r("ISOMITTED", 1, 1, "argument", "TRUE when a LAMBDA's argument was left out.", |ctx, a| {
        Val::Value(Value::Bool(matches!(ctx.eval(&a[0]), Val::Refs(ref areas) if areas.is_empty())))
    }),
    r(
        "MAP",
        2,
        MANY,
        "array1, [array2, ...], lambda",
        "A LAMBDA applied to each value of arrays.",
        map,
    ),
    r(
        "REDUCE",
        3,
        3,
        "initial_value, array, lambda",
        "An array reduced to one value by a LAMBDA.",
        |ctx, a| fold(ctx, a, false),
    ),
    r(
        "SCAN",
        3,
        3,
        "initial_value, array, lambda",
        "Every step of reducing an array with a LAMBDA.",
        |ctx, a| fold(ctx, a, true),
    ),
    r("BYROW", 2, 2, "array, lambda", "A LAMBDA applied to each row, as a column.", |ctx, a| {
        by(ctx, a, true)
    }),
    r("BYCOL", 2, 2, "array, lambda", "A LAMBDA applied to each column, as a row.", |ctx, a| {
        by(ctx, a, false)
    }),
    r(
        "MAKEARRAY",
        3,
        3,
        "rows, columns, lambda",
        "An array made by calling a LAMBDA with each row and column number.",
        make_array,
    ),
];

fn if_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let test = ctx.eval(&args[0]);
    let branch = |ctx: &mut Ctx, i: usize, default: bool| -> Val {
        match args.get(i) {
            None => Val::Value(Value::Bool(default)),
            Some(Expr::Missing) => Val::Value(Value::Number(0.0)),
            Some(e) => ctx.eval(e),
        }
    };
    match ctx.dims(&test) {
        Some((1, 1)) => {
            let v = ctx.element(&test, 0, 0);
            match to_bool(&v) {
                Ok(true) => branch(ctx, 1, true),
                Ok(false) => branch(ctx, 2, false),
                Err(e) => Val::error(e),
            }
        }
        Some(_) => {
            let yes = branch(ctx, 1, true);
            let no = branch(ctx, 2, false);
            ctx.lift_vals(vec![test, yes, no], |_, v| match to_bool(&v[0]) {
                Ok(true) => v[1].clone(),
                Ok(false) => v[2].clone(),
                Err(e) => Value::Error(e),
            })
        }
        None => Val::error(ErrorKind::Value),
    }
}

fn ifs(ctx: &mut Ctx, args: &[Expr]) -> Val {
    if args.len() % 2 != 0 {
        return Val::error(ErrorKind::Value);
    }
    for (k, pair) in args.chunks(2).enumerate() {
        let test = ctx.eval(&pair[0]);
        if ctx.dims(&test).is_some_and(|d| d != (1, 1)) {
            return ifs_over_arrays(ctx, test, &args[2 * k + 1..]);
        }
        let v = ctx.single(test);
        match to_bool(&v) {
            Ok(true) => return ctx.eval(&pair[1]),
            Ok(false) => {}
            Err(e) => return Val::error(e),
        }
    }
    Val::error(ErrorKind::NA)
}

/// IFS once a test is an array: every remaining test and value is computed and each position of the
/// answer takes the value of the first test that is true there. `rest` starts at the first value.
fn ifs_over_arrays(ctx: &mut Ctx, first_test: Val, rest: &[Expr]) -> Val {
    let mut all = vec![first_test];
    all.extend(rest.iter().map(|e| ctx.eval(e)));
    ctx.lift_vals(all, |_, v| {
        for pair in v.chunks(2) {
            match to_bool(&pair[0]) {
                Ok(true) => return pair[1].clone(),
                Ok(false) => {}
                Err(e) => return Value::Error(e),
            }
        }
        Value::Error(ErrorKind::NA)
    })
}

fn if_error(ctx: &mut Ctx, args: &[Expr], only_na: bool) -> Val {
    let value = ctx.eval(&args[0]);
    let caught = |v: &Value| match v {
        Value::Error(ErrorKind::NA) => true,
        Value::Error(_) => !only_na,
        _ => false,
    };
    match &value {
        Val::Value(v) if caught(v) => ctx.eval(&args[1]),
        Val::Value(Value::Empty) => Val::Value(Value::Number(0.0)),
        Val::Value(_) | Val::Lambda(_) => value,
        Val::Ref(area) if area.range.first == area.range.last => {
            let v = ctx.single(value.clone());
            if caught(&v) {
                ctx.eval(&args[1])
            } else {
                Val::Value(if v.is_empty() { Value::Number(0.0) } else { v })
            }
        }
        _ => {
            let fallback = ctx.eval(&args[1]);
            ctx.lift_vals(vec![value, fallback], |_, v| {
                if caught(&v[0]) {
                    v[1].clone()
                } else {
                    v[0].clone()
                }
            })
        }
    }
}

/// AND, OR and XOR: count the true values among logicals and numbers, text that is not TRUE or FALSE
/// being skipped.
fn logicals(ctx: &mut Ctx, args: &[Expr], decide: fn(usize, usize) -> bool) -> Val {
    let values = ctx.values(args);
    let (mut trues, mut seen) = (0, 0);
    for (v, from_range) in values {
        match (&v, from_range) {
            (Value::Error(e), _) => return Val::error(*e),
            (Value::Bool(b), _) => {
                seen += 1;
                trues += usize::from(*b);
            }
            (Value::Number(n), _) => {
                seen += 1;
                trues += usize::from(*n != 0.0);
            }
            // Text that is not TRUE or FALSE is left out, as text in a range is.
            (Value::Text(_), false) => {
                if let Ok(b) = to_bool(&v) {
                    seen += 1;
                    trues += usize::from(b);
                }
            }
            _ => {}
        }
    }
    if seen == 0 {
        return Val::error(ErrorKind::Value);
    }
    Val::Value(Value::Bool(decide(trues, seen)))
}

fn switch(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let value = ctx.eval(&args[0]);
    if ctx.dims(&value).is_some_and(|d| d != (1, 1)) {
        return switch_over_arrays(ctx, value, &args[1..]);
    }
    let value = ctx.single(value);
    if let Value::Error(e) = value {
        return Val::error(e);
    }
    let pairs = &args[1..];
    for (k, pair) in pairs.chunks(2).enumerate() {
        if pair.len() == 1 {
            return ctx.eval(&pair[0]);
        }
        let candidate = ctx.eval(&pair[0]);
        if ctx.dims(&candidate).is_some_and(|d| d != (1, 1)) {
            return switch_over_arrays(ctx, Val::Value(value), &pairs[2 * k..]);
        }
        let candidate = ctx.single(candidate);
        if matches_switch(&value, &candidate) {
            return ctx.eval(&pair[1]);
        }
    }
    Val::error(ErrorKind::NA)
}

/// Whether SWITCH's expression equals one of its values.
fn matches_switch(value: &Value, candidate: &Value) -> bool {
    equal(value, candidate) && !matches!((value, candidate), (Value::Empty, Value::Text(_)))
}

/// SWITCH once the expression or a value is an array: everything is computed and each position of the
/// answer is what SWITCH gives for the values at that position. `rest` starts at a value.
fn switch_over_arrays(ctx: &mut Ctx, expression: Val, rest: &[Expr]) -> Val {
    let mut all = vec![expression];
    all.extend(rest.iter().map(|e| ctx.eval(e)));
    ctx.lift_vals(all, |_, v| {
        if let Value::Error(e) = &v[0] {
            return Value::Error(*e);
        }
        for pair in v[1..].chunks(2) {
            match pair {
                [default] => return default.clone(),
                [candidate, result] if matches_switch(&v[0], candidate) => return result.clone(),
                _ => {}
            }
        }
        Value::Error(ErrorKind::NA)
    })
}

fn let_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    if args.len() % 2 == 0 {
        return Val::error(ErrorKind::Value);
    }
    let depth = ctx.scope.len();
    for pair in args[..args.len() - 1].chunks(2) {
        let Some(name) = let_name(&pair[0]) else {
            ctx.scope.truncate(depth);
            return Val::error(ErrorKind::Value);
        };
        let value = ctx.eval(&pair[1]);
        ctx.scope.push((name, value));
    }
    let result = ctx.eval(&args[args.len() - 1]);
    ctx.scope.truncate(depth);
    result
}

/// The name a LET gives a value. Excel lets it look like a cell, `LET(A1, 2, A1)`.
fn let_name(e: &Expr) -> Option<Arc<str>> {
    match e {
        Expr::Name(n) => Some(n.clone()),
        Expr::Ref(r)
            if r.sheet.is_none()
                && r.shape == Shape::Cell
                && !r.first.row_abs
                && !r.first.col_abs =>
        {
            Some(Arc::from(cell_name(r.first.row, r.first.col).as_str()))
        }
        _ => None,
    }
}

/// The name of a LAMBDA parameter written `[name]`, which may be left out when the LAMBDA is called.
fn optional_parameter(e: &Expr) -> Option<Arc<str>> {
    match e {
        Expr::Table(t)
            if t.table.is_none() && t.items.is_empty() && t.last_column == t.first_column =>
        {
            t.first_column.clone().filter(|_| t.text.starts_with('[') && !t.text.starts_with("[@"))
        }
        _ => None,
    }
}

fn lambda(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut params = Vec::new();
    let mut required = None;
    for p in &args[..args.len() - 1] {
        match (p, optional_parameter(p)) {
            (_, Some(name)) => {
                required.get_or_insert(params.len());
                params.push(name);
            }
            (Expr::Name(n), None) if required.is_none() => params.push(n.clone()),
            _ => return Val::error(ErrorKind::Value),
        }
    }
    Val::Lambda(Arc::new(Lambda {
        required: required.unwrap_or(params.len()),
        params,
        body: Arc::new(args[args.len() - 1].clone()),
        scope: ctx.scope.clone(),
    }))
}

fn lambda_arg(ctx: &mut Ctx, expr: &Expr) -> Result<Arc<Lambda>, ErrorKind> {
    match ctx.eval(expr) {
        Val::Lambda(l) => Ok(l),
        Val::Value(Value::Error(e)) => Err(e),
        _ => Err(ErrorKind::Value),
    }
}

fn array_arg(ctx: &mut Ctx, expr: &Expr) -> Result<Array, ErrorKind> {
    let v = ctx.eval(expr);
    ctx.array(v)
}

/// Whether a LAMBDA can be called with this many values: at least its required parameters and no more
/// than it has. The helpers below refuse the whole call otherwise, not each cell of it.
fn takes(f: &Lambda, count: usize) -> bool {
    count >= f.required && count <= f.params.len()
}

/// What a LAMBDA gave back for one cell of a result. A LAMBDA that gives several values where one cell
/// is wanted makes the whole result `#CALC!`.
fn one_value(ctx: &mut Ctx, v: Val) -> Result<Value, ErrorKind> {
    match v {
        Val::Array(a) if a.data.len() > 1 => Err(ErrorKind::Calc),
        Val::Ref(area) if area.range.len() > 1 => Err(ErrorKind::Calc),
        Val::Lambda(_) => Err(ErrorKind::Calc),
        other => Ok(ctx.single(other)),
    }
}

fn map(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let f = match lambda_arg(ctx, &args[args.len() - 1]) {
        Ok(f) => f,
        Err(e) => return Val::error(e),
    };
    let mut arrays = Vec::new();
    for e in &args[..args.len() - 1] {
        match ctx.eval(e) {
            Val::Lambda(_) => return Val::error(ErrorKind::Value),
            v => match ctx.array(v) {
                Ok(a) => arrays.push(a),
                Err(e) => return Val::error(e),
            },
        }
    }
    if !takes(&f, arrays.len()) {
        return Val::error(ErrorKind::Value);
    }
    let rows = arrays.iter().map(|a| a.rows).max().unwrap_or(1);
    let cols = arrays.iter().map(|a| a.cols).max().unwrap_or(1);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let values = arrays.iter().map(|a| Val::Value(a.stretched(r, c))).collect();
            let v = ctx.invoke_with(&f, values);
            match one_value(ctx, v) {
                Ok(v) => data.push(v),
                Err(e) => return Val::error(e),
            }
        }
    }
    Val::Array(Array::new(rows, cols, data))
}

fn fold(ctx: &mut Ctx, args: &[Expr], keep_steps: bool) -> Val {
    let mut acc = ctx.eval(&args[0]);
    let array = match array_arg(ctx, &args[1]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let f = match lambda_arg(ctx, &args[2]) {
        Ok(f) => f,
        Err(e) => return Val::error(e),
    };
    if !takes(&f, 2) {
        return Val::error(ErrorKind::Value);
    }
    let mut steps = Vec::new();
    for v in &array.data {
        acc = ctx.invoke_with(&f, vec![acc, Val::Value(v.clone())]);
        if keep_steps {
            match one_value(ctx, acc.clone()) {
                Ok(single) => steps.push(single),
                Err(e) => return Val::error(e),
            }
        }
    }
    if keep_steps {
        Val::Array(Array::new(array.rows, array.cols, steps))
    } else {
        acc
    }
}

fn by(ctx: &mut Ctx, args: &[Expr], rows: bool) -> Val {
    let array = match array_arg(ctx, &args[0]) {
        Ok(a) => a,
        Err(e) => return Val::error(e),
    };
    let f = match lambda_arg(ctx, &args[1]) {
        Ok(f) => f,
        Err(e) => return Val::error(e),
    };
    if !takes(&f, 1) {
        return Val::error(ErrorKind::Value);
    }
    let count = if rows { array.rows } else { array.cols };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let slice = if rows {
            Array::row((0..array.cols).map(|c| array.get(i, c).clone()).collect())
        } else {
            Array::column((0..array.rows).map(|r| array.get(r, i).clone()).collect())
        };
        let v = ctx.invoke_with(&f, vec![Val::Array(slice)]);
        match one_value(ctx, v) {
            Ok(v) => out.push(v),
            Err(e) => return Val::error(e),
        }
    }
    Val::Array(if rows { Array::column(out) } else { Array::row(out) })
}

fn make_array(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut dims = [0.0; 2];
    for (i, d) in dims.iter_mut().enumerate() {
        let v = ctx.eval(&args[i]);
        let v = ctx.single(v);
        match ctx.number(&v) {
            Ok(n) => *d = n.trunc(),
            Err(e) => return Val::error(e),
        }
    }
    if dims[0] < 1.0 || dims[1] < 1.0 {
        return Val::error(ErrorKind::Value);
    }
    let f = match lambda_arg(ctx, &args[2]) {
        Ok(f) => f,
        Err(e) => return Val::error(e),
    };
    if !takes(&f, 2) {
        return Val::error(ErrorKind::Value);
    }
    let (rows, cols) = (dims[0] as usize, dims[1] as usize);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let numbers = vec![
                Val::Value(Value::Number(r as f64 + 1.0)),
                Val::Value(Value::Number(c as f64 + 1.0)),
            ];
            let v = ctx.invoke_with(&f, numbers);
            match one_value(ctx, v) {
                Ok(v) => data.push(v),
                Err(e) => return Val::error(e),
            }
        }
    }
    Val::Array(Array::new(rows, cols, data))
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::check;

    #[test]
    fn logic_and_lambdas_answer_what_excel_answers() {
        check(&[
            ("IF(1>0,\"yes\",\"no\")", "yes"),
            ("IF(FALSE,1/0,2)", "2"),
            ("IF(TRUE,1)", "1"),
            ("IF(FALSE,1)", "FALSE"),
            ("IFERROR(1/0,\"none\")", "none"),
            ("IFNA(NA(),0)", "0"),
            ("IFNA(1/0,0)", "#DIV/0!"),
            ("AND(TRUE,1,\"TRUE\")", "TRUE"),
            ("OR(FALSE,0)", "FALSE"),
            ("XOR(TRUE,TRUE,TRUE)", "TRUE"),
            ("NOT(0)", "TRUE"),
            ("IFS(1>2,\"a\",2>1,\"b\")", "b"),
            ("IFS(1>2,\"a\")", "#N/A"),
            ("SWITCH(2,1,\"one\",2,\"two\",\"other\")", "two"),
            ("SWITCH(9,1,\"one\",\"other\")", "other"),
            ("LET(x,2,y,x*3,x+y)", "8"),
            ("LAMBDA(x,x*x)(4)", "16"),
            ("LET(sq,LAMBDA(x,x*x),sq(5))", "25"),
            ("REDUCE(0,{1,2,3},LAMBDA(a,b,a+b))", "6"),
            ("SUM(MAP({1,2,3},LAMBDA(x,x*10)))", "60"),
            ("SUM(MAKEARRAY(2,2,LAMBDA(r,c,r*c)))", "9"),
            ("SUM(BYROW({1,2;3,4},LAMBDA(r,SUM(r))))", "10"),
            ("LAMBDA(x,[y],ISOMITTED(y))(1)", "TRUE"),
            ("LAMBDA(x,[y],ISOMITTED(y))(1,2)", "FALSE"),
            ("LAMBDA([x],ISOMITTED(x))()", "TRUE"),
            ("LAMBDA(x,y,x+y)(1)", "#VALUE!"),
            ("LAMBDA(x,x)(1,2)", "#VALUE!"),
            ("MAP({1,2,3},LAMBDA(x,{1,2}))", "#CALC!"),
            ("MAP({1,2,3},LAMBDA(x,x),LAMBDA(y,y))", "#VALUE!"),
            ("MAKEARRAY(2,2,LAMBDA(r,r))", "#VALUE!"),
            ("BYROW({1,2;3,4},LAMBDA(r,r))", "#CALC!"),
            ("LET(A1,2,A1+1)", "3"),
            ("LET(A1,2,B1)", "#NAME?"),
            ("AND(TRUE,\"x\")", "TRUE"),
            ("AND(\"x\")", "#VALUE!"),
            ("SUM(IFS({TRUE,FALSE},1,TRUE,2))", "3"),
            ("SUM(SWITCH({1,2},1,10,2,20))", "30"),
        ]);
    }
}
