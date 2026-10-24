//! The function library: one table every part of Unluminous reads.
//!
//! The evaluator calls through it, the formula completion list and the argument hint are made from it,
//! `plugins run spreadsheet functions` prints it, and the oracle corpus is generated from it. A function
//! is either **scalar**, a function of single values the evaluator lifts over ranges and arrays, or
//! **raw**, given its argument expressions to evaluate as it needs: lazily for IF, as references for
//! ROW and OFFSET, as whole ranges for SUM and VLOOKUP.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::eval::{to_bool, to_text, Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub mod complex;
pub mod criteria;
pub mod database;
pub mod date;
pub mod discrete;
pub mod dist;
pub mod engineering;
pub mod ets;
pub mod financial;
pub mod forecast;
pub mod grouping;
pub mod inference;
pub mod info;
pub mod logical;
pub mod lookup;
pub mod math;
pub mod precise;
pub mod regex;
pub mod regression;
pub mod services;
pub mod stats;
pub mod text;
pub mod text_bytes;
pub mod web;
mod xpath;

pub type ScalarFn = fn(&mut Ctx, &[Value]) -> Result<Value, ErrorKind>;
pub type RawFn = fn(&mut Ctx, &[Expr]) -> Val;

#[derive(Clone, Copy)]
pub enum Imp {
    Scalar(ScalarFn),
    Raw(RawFn),
}

/// One function.
#[derive(Clone, Copy)]
pub struct Function {
    pub name: &'static str,
    pub min: usize,
    pub max: usize,
    pub imp: Imp,
    /// The arguments as a person reads them, for the hint while typing: `number1, [number2], ...`.
    pub args: &'static str,
    /// One sentence.
    pub about: &'static str,
    pub category: &'static str,
    /// The function needs a service of Excel's (a data connection, a cloud model, a stock feed). A cell
    /// read from a file that holds one keeps the value the file saved, and one entered here is `#BLOCKED!`.
    pub offline: bool,
}

impl std::fmt::Debug for Function {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}({})", self.name, self.args)
    }
}

/// Many arguments: what `max` is for SUM and its kind.
pub const MANY: usize = 255;

/// A scalar function entry.
pub const fn s(
    name: &'static str,
    min: usize,
    max: usize,
    args: &'static str,
    about: &'static str,
    f: ScalarFn,
) -> Function {
    Function { name, min, max, imp: Imp::Scalar(f), args, about, category: "", offline: false }
}

/// A raw function entry.
pub const fn r(
    name: &'static str,
    min: usize,
    max: usize,
    args: &'static str,
    about: &'static str,
    f: RawFn,
) -> Function {
    Function { name, min, max, imp: Imp::Raw(f), args, about, category: "", offline: false }
}

/// A function that needs a service of Excel's: evaluating it is `#BLOCKED!`, and the hint says why.
pub const fn offline(
    name: &'static str,
    min: usize,
    max: usize,
    args: &'static str,
    about: &'static str,
) -> Function {
    Function {
        name,
        min,
        max,
        imp: Imp::Scalar(|_, _| Err(ErrorKind::Blocked)),
        args,
        about,
        category: "",
        offline: true,
    }
}

const CATEGORIES: &[(&str, &[Function])] = &[
    ("Math", math::FUNCTIONS),
    ("Statistical", stats::FUNCTIONS),
    ("Statistical", dist::FUNCTIONS),
    ("Statistical", discrete::FUNCTIONS),
    ("Statistical", inference::FUNCTIONS),
    ("Statistical", regression::FUNCTIONS),
    ("Statistical", forecast::FUNCTIONS),
    ("Database", database::FUNCTIONS),
    ("Logical", logical::FUNCTIONS),
    ("Lookup", lookup::FUNCTIONS),
    ("Lookup", grouping::FUNCTIONS),
    ("Text", text::FUNCTIONS),
    ("Text", text_bytes::FUNCTIONS),
    ("Text", regex::FUNCTIONS),
    ("Web", web::FUNCTIONS),
    ("Cube", services::CUBE),
    ("Add-in and Automation", services::ADD_IN),
    ("Lookup", services::LOOKUP),
    ("Text", services::TEXT),
    ("Information", services::INFORMATION),
    ("Web", services::WEB),
    ("Other", services::OTHER),
    ("Date and time", date::FUNCTIONS),
    ("Financial", financial::FUNCTIONS),
    ("Information", info::FUNCTIONS),
    ("Engineering", engineering::FUNCTIONS),
    ("Engineering", complex::FUNCTIONS),
];

/// Every function, with its category, sorted by name.
pub fn all() -> &'static [Function] {
    static ALL: OnceLock<Vec<Function>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<Function> = CATEGORIES
            .iter()
            .flat_map(|(category, list)| list.iter().map(move |f| Function { category, ..*f }))
            .collect();
        all.sort_by(|a, b| a.name.cmp(b.name));
        all.dedup_by(|a, b| a.name == b.name);
        all
    })
}

/// The function with this name, in upper case.
pub fn find(name: &str) -> Option<&'static Function> {
    static BY_NAME: OnceLock<HashMap<&'static str, &'static Function>> = OnceLock::new();
    BY_NAME
        .get_or_init(|| all().iter().map(|f| (f.name, f)).collect())
        .get(name)
        .copied()
        .or_else(|| grouping::OPERATORS.iter().find(|f| f.name == name))
}

/// Functions whose value can change with nothing in the workbook changing.
pub const VOLATILE: &[&str] =
    &["NOW", "TODAY", "RAND", "RANDBETWEEN", "RANDARRAY", "OFFSET", "INDIRECT", "CELL", "INFO"];

// ---------------------------------------------------------------------------------------------------
// Helpers every category uses.

/// A value as a number, coercing text the way arithmetic does.
pub fn num(ctx: &Ctx, v: &Value) -> Result<f64, ErrorKind> {
    ctx.number(v)
}

/// A value as a whole number, truncated toward zero, which is what Excel does with a count argument.
pub fn int(ctx: &Ctx, v: &Value) -> Result<i64, ErrorKind> {
    let n = num(ctx, v)?;
    if n.abs() > 9.0e15 {
        return Err(ErrorKind::Num);
    }
    Ok(n.trunc() as i64)
}

pub fn text_of(v: &Value) -> Result<String, ErrorKind> {
    to_text(v)
}

pub fn boolean(v: &Value) -> Result<bool, ErrorKind> {
    to_bool(v)
}

/// The optional argument at `i`, or `default` when it was left out or left empty.
pub fn opt(args: &[Value], i: usize) -> Option<&Value> {
    args.get(i).filter(|v| !v.is_empty())
}

pub fn opt_num(ctx: &Ctx, args: &[Value], i: usize, default: f64) -> Result<f64, ErrorKind> {
    match args.get(i) {
        None => Ok(default),
        Some(v) => num(ctx, v),
    }
}

pub fn opt_bool(args: &[Value], i: usize, default: bool) -> Result<bool, ErrorKind> {
    match args.get(i) {
        None => Ok(default),
        Some(Value::Empty) => Ok(false),
        Some(v) => boolean(v),
    }
}

pub fn number(n: f64) -> Result<Value, ErrorKind> {
    if n.is_finite() {
        Ok(Value::Number(if n == 0.0 { 0.0 } else { n }))
    } else {
        Err(ErrorKind::Num)
    }
}

/// How numbers are taken out of arguments by an aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gather {
    /// SUM, AVERAGE, MAX: numbers in ranges; numbers, logicals and numeric text written directly.
    Numbers,
    /// AVERAGEA, MAXA: text in ranges counts as 0 and logicals as 1 or 0.
    All,
    /// MINA, MAXA, VARA: as `All` for cells of a range, but text and logicals inside an array written out
    /// in the formula, such as `{1,"a",TRUE}`, are left out.
    AllInRanges,
    /// MODE: numbers only. Text and logicals are left out wherever they are, including when typed directly.
    OnlyNumbers,
}

/// The numbers an aggregate reads from its arguments. An error anywhere is the answer.
pub fn gather(ctx: &mut Ctx, args: &[Expr], how: Gather) -> Result<Vec<f64>, ErrorKind> {
    let mut out = Vec::new();
    for arg in args {
        let v = ctx.eval(arg);
        gather_val(ctx, v, how, &mut out)?;
    }
    Ok(out)
}

pub fn gather_val(ctx: &mut Ctx, v: Val, how: Gather, out: &mut Vec<f64>) -> Result<(), ErrorKind> {
    let mut values = Vec::new();
    let written_array = matches!(v, Val::Array(_));
    ctx.values_of(v, &mut values);
    let counts_all = how == Gather::All || how == Gather::AllInRanges && !written_array;
    for (value, from_range) in values {
        match (&value, from_range) {
            (Value::Error(e), _) => return Err(*e),
            (Value::Number(n), _) => out.push(*n),
            _ if how == Gather::OnlyNumbers => {}
            (Value::Bool(b), false) => out.push(f64::from(u8::from(*b))),
            (Value::Text(t), false) => {
                out.push(crate::input::number_from_text(t, ctx.date1904()).ok_or(ErrorKind::Value)?)
            }
            (Value::Empty, false) => out.push(0.0),
            (Value::Bool(b), true) if counts_all => out.push(f64::from(u8::from(*b))),
            (Value::Text(_), true) if counts_all => out.push(0.0),
            _ => {}
        }
    }
    Ok(())
}

/// A range or array argument's values as a flat list in row order, with empties as [`Value::Empty`].
pub fn flat(ctx: &mut Ctx, expr: &Expr) -> Result<(Vec<Value>, usize, usize), ErrorKind> {
    let v = ctx.eval(expr);
    let a = ctx.array(v)?;
    Ok((a.data, a.rows, a.cols))
}

/// Numbers of one array argument, in order, keeping their positions: `None` for anything not a number.
/// What CORREL, SUMPRODUCT and the regression functions pair up.
pub fn numbers_at(
    ctx: &mut Ctx,
    expr: &Expr,
) -> Result<(Vec<Option<f64>>, usize, usize), ErrorKind> {
    let (values, rows, cols) = flat(ctx, expr)?;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        match v {
            Value::Number(n) => out.push(Some(n)),
            Value::Error(e) => return Err(e),
            _ => out.push(None),
        }
    }
    Ok((out, rows, cols))
}

/// A function of one number, for the many entries that are only that.
pub fn unary(
    ctx: &mut Ctx,
    args: &[Value],
    f: fn(f64) -> Result<f64, ErrorKind>,
) -> Result<Value, ErrorKind> {
    number(f(num(ctx, &args[0])?)?)
}

#[cfg(test)]
pub mod testing {
    //! Evaluate a formula on a small workbook, for the function tests.

    use crate::coord::CellRef;
    use crate::eval::{Ctx, Env};
    use crate::model::{Cell, Workbook};
    use crate::value::Value;

    /// A workbook whose Sheet1 holds `cells` as typed text, `A1 = 3` style, each read as typed input.
    pub fn book(cells: &[(&str, &str)]) -> Workbook {
        let mut book = Workbook::new();
        for (at, text) in cells {
            let at = CellRef::parse(at).unwrap();
            let typed = crate::input::read(text, false, 2026);
            book.sheets[0].set(at, Cell::value(typed.value));
        }
        book
    }

    /// The value of a formula on a workbook, evaluated at Z100 and intersected the way a legacy cell is.
    pub fn eval_on(book: &Workbook, formula: &str) -> Value {
        let expr = crate::formula::parser::parse(formula.trim_start_matches('=')).unwrap();
        let env = Env::new(46318.5, 42);
        let mut ctx = Ctx::new(book, 0, CellRef::new(99, 25), &env);
        let v = ctx.eval(&expr);
        match v {
            crate::eval::Val::Array(a) if a.rows * a.cols == 1 => a.data[0].clone(),
            crate::eval::Val::Array(a) => Value::text(format!("{:?}", a.data)),
            other => ctx.single(other),
        }
    }

    pub fn eval(formula: &str) -> Value {
        eval_on(&book(&[]), formula)
    }

    /// The value as a test compares it: numbers rounded to twelve significant digits.
    pub fn shown(v: &Value) -> String {
        match v {
            Value::Number(n) => {
                let r = crate::value::round_significant(*n, 12);
                crate::value::number_to_text(r)
            }
            other => other.to_text(),
        }
    }

    pub fn check(cases: &[(&str, &str)]) {
        let mut wrong = Vec::new();
        for (formula, expected) in cases {
            let got = shown(&eval(formula));
            if got != *expected {
                wrong.push(format!("{formula}: expected {expected}, got {got}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
