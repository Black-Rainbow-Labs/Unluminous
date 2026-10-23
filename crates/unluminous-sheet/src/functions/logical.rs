//! Logical functions, LET, LAMBDA and the functions that call a LAMBDA over an array.
//!
//! IF, IFS, IFERROR, IFNA, CHOOSE and SWITCH evaluate only the branch they take when the condition is a
//! single value, which is what makes `=IF(A1=0,0,1/A1)` answer 0 rather than `#DIV/0!` and what keeps a
//! recursive LAMBDA from running for ever. Given an array condition they take both branches and choose
//! value by value.

use std::sync::Arc;

use super::{r, s, Function, MANY};
use crate::eval::{to_bool, Ctx, Lambda, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    r("IF", 1, 3, "logical_test, [value_if_true], [value_if_false]", "One value if a test is true and another if it is false.", if_fn),
    r("IFS", 2, MANY, "logical_test1, value_if_true1, ...", "The value for the first test that is true.", ifs),
    r("IFERROR", 2, 2, "value, value_if_error", "A value, or another one when it is an error.", |ctx, a| if_error(ctx, a, false)),
    r("IFNA", 2, 2, "value, value_if_na", "A value, or another one when it is #N/A.", |ctx, a| if_error(ctx, a, true)),
    r("AND", 1, MANY, "logical1, [logical2], ...", "TRUE when every argument is true.", |ctx, a| logicals(ctx, a, |t, n| t == n)),
    r("OR", 1, MANY, "logical1, [logical2], ...", "TRUE when any argument is true.", |ctx, a| logicals(ctx, a, |t, _| t > 0)),
    r("XOR", 1, MANY, "logical1, [logical2], ...", "TRUE when an odd number of arguments are true.", |ctx, a| logicals(ctx, a, |t, _| t % 2 == 1)),
    s("NOT", 1, 1, "logical", "The opposite of a logical.", |_, a| to_bool(&a[0]).map(|b| Value::Bool(!b))),
    s("TRUE", 0, 0, "", "The logical value TRUE.", |_, _| Ok(Value::Bool(true))),
    s("FALSE", 0, 0, "", "The logical value FALSE.", |_, _| Ok(Value::Bool(false))),
    r("SWITCH", 3, MANY, "expression, value1, result1, ..., [default]", "The result for the first value that matches.", switch),
    r("LET", 3, MANY, "name1, value1, ..., calculation", "Names values to use in a calculation.", let_fn),
    r("LAMBDA", 1, MANY, "[parameter1, ...], calculation", "A function made from a calculation.", lambda),
    r("ISOMITTED", 1, 1, "argument", "TRUE when a LAMBDA's argument was left out.", |ctx, a| {
        Val::Value(Value::Bool(matches!(ctx.eval(&a[0]), Val::Refs(ref areas) if areas.is_empty())))
    }),
    r("MAP", 2, MANY, "array1, [array2, ...], lambda", "A LAMBDA applied to each value of arrays.", map),
    r("REDUCE", 3, 3, "initial_value, array, lambda", "An array reduced to one value by a LAMBDA.", |ctx, a| fold(ctx, a, false)),
    r("SCAN", 3, 3, "initial_value, array, lambda", "Every step of reducing an array with a LAMBDA.", |ctx, a| fold(ctx, a, true)),
    r("BYROW", 2, 2, "array, lambda", "A LAMBDA applied to each row, as a column.", |ctx, a| by(ctx, a, true)),
    r("BYCOL", 2, 2, "array, lambda", "A LAMBDA applied to each column, as a row.", |ctx, a| by(ctx, a, false)),
    r("MAKEARRAY", 3, 3, "rows, columns, lambda", "An array made by calling a LAMBDA with each row and column number.", make_array),
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
    for pair in args.chunks(2) {
        let test = ctx.eval(&pair[0]);
        let v = ctx.single(test);
        match to_bool(&v) {
            Ok(true) => return ctx.eval(&pair[1]),
            Ok(false) => {}
            Err(e) => return Val::error(e),
        }
    }
    Val::error(ErrorKind::NA)
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
            ctx.lift_vals(vec![value, fallback], |_, v| if caught(&v[0]) { v[1].clone() } else { v[0].clone() })
        }
    }
}

/// AND, OR and XOR: count the true values among logicals and numbers, text in ranges being skipped.
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
            (Value::Text(_), false) => match to_bool(&v) {
                Ok(b) => {
                    seen += 1;
                    trues += usize::from(b);
                }
                Err(e) => return Val::error(e),
            },
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
    let value = ctx.single(value);
    if let Value::Error(e) = value {
        return Val::error(e);
    }
    let pairs = &args[1..];
    for pair in pairs.chunks(2) {
        if pair.len() == 1 {
            return ctx.eval(&pair[0]);
        }
        let candidate = ctx.eval(&pair[0]);
        let candidate = ctx.single(candidate);
        if crate::eval::equal(&value, &candidate) && !matches!((&value, &candidate), (Value::Empty, Value::Text(_))) {
            return ctx.eval(&pair[1]);
        }
    }
    Val::error(ErrorKind::NA)
}

fn let_fn(ctx: &mut Ctx, args: &[Expr]) -> Val {
    if args.len() % 2 == 0 {
        return Val::error(ErrorKind::Value);
    }
    let depth = ctx.scope.len();
    for pair in args[..args.len() - 1].chunks(2) {
        let Expr::Name(name) = &pair[0] else {
            ctx.scope.truncate(depth);
            return Val::error(ErrorKind::Value);
        };
        let value = ctx.eval(&pair[1]);
        ctx.scope.push((name.clone(), value));
    }
    let result = ctx.eval(&args[args.len() - 1]);
    ctx.scope.truncate(depth);
    result
}

fn lambda(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let mut params = Vec::new();
    for p in &args[..args.len() - 1] {
        match p {
            Expr::Name(n) => params.push(n.clone()),
            _ => return Val::error(ErrorKind::Value),
        }
    }
    Val::Lambda(Arc::new(Lambda { params, body: Arc::new(args[args.len() - 1].clone()), scope: ctx.scope.clone() }))
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

fn map(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let f = match lambda_arg(ctx, &args[args.len() - 1]) {
        Ok(f) => f,
        Err(e) => return Val::error(e),
    };
    let mut arrays = Vec::new();
    for e in &args[..args.len() - 1] {
        match array_arg(ctx, e) {
            Ok(a) => arrays.push(a),
            Err(e) => return Val::error(e),
        }
    }
    let rows = arrays.iter().map(|a| a.rows).max().unwrap_or(1);
    let cols = arrays.iter().map(|a| a.cols).max().unwrap_or(1);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let values = arrays.iter().map(|a| Val::Value(a.stretched(r, c))).collect();
            let v = ctx.invoke_with(&f, values);
            data.push(ctx.single(v));
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
    let mut steps = Vec::new();
    for v in &array.data {
        acc = ctx.invoke_with(&f, vec![acc, Val::Value(v.clone())]);
        if keep_steps {
            let single = ctx.single(acc.clone());
            steps.push(single);
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
    let count = if rows { array.rows } else { array.cols };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let slice = if rows {
            Array::row((0..array.cols).map(|c| array.get(i, c).clone()).collect())
        } else {
            Array::column((0..array.rows).map(|r| array.get(r, i).clone()).collect())
        };
        let v = ctx.invoke_with(&f, vec![Val::Array(slice)]);
        out.push(ctx.single(v));
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
    let (rows, cols) = (dims[0] as usize, dims[1] as usize);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let v = ctx.invoke_with(&f, vec![Val::Value(Value::Number(r as f64 + 1.0)), Val::Value(Value::Number(c as f64 + 1.0))]);
            data.push(ctx.single(v));
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
        ]);
    }
}
