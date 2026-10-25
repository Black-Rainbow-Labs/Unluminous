//! Computing one formula.
//!
//! A formula's tree is walked with a [`Ctx`]: the workbook, the cell being computed, and the LET and
//! LAMBDA names in scope. Every node evaluates to a [`Val`]. A reference stays a reference until something
//! needs its values, so `ROW`, `OFFSET`, `INDEX` and `ISREF` see the reference itself and SUM can read a
//! whole column without making a million values.
//!
//! **Lifting.** A function that takes one value, given a range or an array, is computed once for each
//! value and returns an array of the answers, which is how `=ABS(A1:A3)` spills three numbers. Operators do
//! the same. Two arrays of different sizes are stretched the way Excel stretches them: a single row or
//! column repeats, and anything past the edge of the smaller one is `#N/A`.

use std::cell::Cell as StdCell;
use std::cmp::Ordering;
use std::sync::Arc;

use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::{BinOp, Expr, Reference, Shape, TableRef};
use crate::model::{Sheet, Workbook};
use crate::value::{number_to_text, Array, ErrorKind, Value};

/// A rectangle of cells on one sheet, or the same rectangle across a run of sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub sheet: usize,
    pub sheet_last: usize,
    pub range: Range,
}

impl Area {
    pub fn new(sheet: usize, range: Range) -> Self {
        Self { sheet, sheet_last: sheet, range }
    }

    pub fn is_3d(&self) -> bool {
        self.sheet_last != self.sheet
    }
}

/// A LAMBDA: its parameters, its body, and the LET names it was made inside.
#[derive(Debug, Clone, PartialEq)]
pub struct Lambda {
    pub params: Vec<Arc<str>>,
    /// How many of the leading parameters must be given; the rest were written `[name]` and may be left out.
    pub required: usize,
    pub body: Arc<Expr>,
    pub scope: Vec<(Arc<str>, Val)>,
}

/// What an expression evaluates to.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Value(Value),
    Array(Array),
    Ref(Area),
    /// Several references, from a union `(A1,C3)`.
    Refs(Vec<Area>),
    Lambda(Arc<Lambda>),
}

impl Val {
    pub fn error(kind: ErrorKind) -> Self {
        Val::Value(Value::Error(kind))
    }

    pub fn number(n: f64) -> Self {
        if n.is_finite() {
            Val::Value(Value::Number(n))
        } else {
            Val::error(ErrorKind::Num)
        }
    }

    pub fn text(text: impl AsRef<str>) -> Self {
        Val::Value(Value::text(text))
    }

    pub fn as_error(&self) -> Option<ErrorKind> {
        match self {
            Val::Value(Value::Error(e)) => Some(*e),
            _ => None,
        }
    }
}

impl From<Value> for Val {
    fn from(value: Value) -> Self {
        Val::Value(value)
    }
}

impl From<Result<Value, ErrorKind>> for Val {
    fn from(result: Result<Value, ErrorKind>) -> Self {
        match result {
            Ok(v) => Val::Value(v),
            Err(e) => Val::error(e),
        }
    }
}

/// Things a whole calculation shares: the time it started and the random numbers.
#[derive(Debug)]
pub struct Env {
    /// NOW() as a serial, the same for every cell in one calculation.
    pub now: f64,
    pub seed: StdCell<u64>,
}

impl Env {
    pub fn new(now: f64, seed: u64) -> Self {
        crate::input::use_clock(now);
        Self { now, seed: StdCell::new(seed | 1) }
    }

    /// A number in [0, 1), from a xorshift generator.
    pub fn random(&self) -> f64 {
        let mut x = self.seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// How deep names and LAMBDAs may call each other before the formula is refused.
const DEEPEST: u32 = 200;

/// Everything evaluating one formula needs.
pub struct Ctx<'a> {
    pub book: &'a Workbook,
    pub sheet: usize,
    pub at: CellRef,
    pub scope: Vec<(Arc<str>, Val)>,
    pub env: &'a Env,
    /// Set when OFFSET or INDIRECT was used, which makes the formula's precedents unknown in advance.
    pub dynamic: bool,
    /// A formula from a file written before dynamic arrays: a range given where one value is wanted is
    /// reduced to the cell in the formula's own row or column, rather than lifted over.
    pub legacy: bool,
    depth: u32,
}

impl<'a> Ctx<'a> {
    pub fn new(book: &'a Workbook, sheet: usize, at: CellRef, env: &'a Env) -> Self {
        Self { book, sheet, at, scope: Vec::new(), env, dynamic: false, legacy: false, depth: 0 }
    }

    pub fn date1904(&self) -> bool {
        self.book.date1904
    }

    pub fn sheet_ref(&self, index: usize) -> Option<&'a Sheet> {
        self.book.sheets.get(index)
    }

    /// Evaluate an expression.
    pub fn eval(&mut self, expr: &Expr) -> Val {
        match expr {
            Expr::Number(n) => Val::number(*n),
            Expr::Text(t) => Val::Value(Value::Text(t.clone())),
            Expr::Bool(b) => Val::Value(Value::Bool(*b)),
            Expr::Error(e) => Val::error(*e),
            Expr::Missing => Val::Value(Value::Empty),
            Expr::Array(rows) => {
                let mut data = Vec::new();
                for row in rows {
                    for item in row {
                        data.push(match item {
                            Expr::Neg(inner) => match self.eval(inner) {
                                Val::Value(Value::Number(n)) => Value::Number(-n),
                                other => self.single(other),
                            },
                            other => {
                                let v = self.eval(other);
                                self.single(v)
                            }
                        });
                    }
                }
                Val::Array(Array::new(rows.len(), rows.first().map_or(0, Vec::len), data))
            }
            Expr::Ref(r) => self.reference(r),
            Expr::Name(name) => self.name(name),
            Expr::External(_) => Val::error(ErrorKind::Ref),
            Expr::Paren(inner) => self.eval(inner),
            Expr::Neg(inner) => {
                let v = self.eval(inner);
                self.map1(v, |ctx, x| ctx.number(&x).map(|n| Value::Number(-n)).into_value())
            }
            Expr::Plus(inner) => self.eval(inner),
            Expr::Percent(inner) => {
                let v = self.eval(inner);
                self.map1(v, |ctx, x| ctx.number(&x).map(|n| Value::Number(n / 100.0)).into_value())
            }
            Expr::Binary(BinOp::Range, a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                match (a, b) {
                    (Val::Ref(x), Val::Ref(y))
                        if x.sheet == y.sheet && !x.is_3d() && !y.is_3d() =>
                    {
                        Val::Ref(Area::new(x.sheet, x.range.union(&y.range)))
                    }
                    (Val::Value(Value::Error(e)), _) | (_, Val::Value(Value::Error(e))) => {
                        Val::error(e)
                    }
                    _ => Val::error(ErrorKind::Value),
                }
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                let op = *op;
                self.map2(a, b, move |ctx, x, y| binary(ctx, op, &x, &y))
            }
            Expr::Call(name, args) => self.call(name, args),
            Expr::Invoke(f, args) => {
                let f = self.eval(f);
                match f {
                    Val::Lambda(lambda) => self.invoke(&lambda, args),
                    Val::Value(Value::Error(e)) => Val::error(e),
                    _ => Val::error(ErrorKind::Value),
                }
            }
            Expr::Spill(inner) => self.spill(inner),
            Expr::Implicit(inner) => {
                let v = self.eval(inner);
                Val::Value(self.intersect(v))
            }
            Expr::Table(t) => self.table(t),
            Expr::Union(items) => {
                let mut areas = Vec::new();
                for item in items {
                    match self.eval(item) {
                        Val::Ref(a) => areas.push(a),
                        Val::Refs(more) => areas.extend(more),
                        Val::Value(Value::Error(e)) => return Val::error(e),
                        _ => return Val::error(ErrorKind::Value),
                    }
                }
                Val::Refs(areas)
            }
            Expr::Intersect(a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                match (a, b) {
                    (Val::Ref(x), Val::Ref(y)) if x.sheet == y.sheet => {
                        match x.range.intersection(&y.range) {
                            Some(r) => Val::Ref(Area::new(x.sheet, r)),
                            None => Val::error(ErrorKind::Null),
                        }
                    }
                    (Val::Value(Value::Error(e)), _) | (_, Val::Value(Value::Error(e))) => {
                        Val::error(e)
                    }
                    _ => Val::error(ErrorKind::Value),
                }
            }
        }
    }

    /// A reference written in the formula, on the formula's own sheet or the one it names.
    pub fn reference(&mut self, r: &Reference) -> Val {
        if r.sheet.is_none() {
            if let Some(found) = self.reference_in_scope(r) {
                return found;
            }
        }
        let sheet = match &r.sheet {
            None => self.sheet,
            Some(name) => match self.book.sheet_index(name) {
                Some(index) => index,
                None => return Val::error(ErrorKind::Ref),
            },
        };
        let sheet_last = match &r.sheet_last {
            None => sheet,
            Some(name) => match self.book.sheet_index(name) {
                Some(index) => index,
                None => return Val::error(ErrorKind::Ref),
            },
        };
        let (sheet, sheet_last) = (sheet.min(sheet_last), sheet.max(sheet_last));
        let range = match r.shape {
            Shape::Columns => {
                Range::new(CellRef::new(0, r.first.col), CellRef::new(MAX_ROWS - 1, r.last.col))
            }
            Shape::Rows => {
                Range::new(CellRef::new(r.first.row, 0), CellRef::new(r.last.row, MAX_COLS - 1))
            }
            _ => r.range(),
        };
        Val::Ref(Area { sheet, sheet_last, range })
    }

    /// A LET name may look like a cell, `LET(A1, 2, A1)`, and the name wins over the cell. While such a name
    /// is in scope Excel cannot tell the cells of the formula from names, so every other cell written in
    /// the formula is `#NAME?`.
    fn reference_in_scope(&self, r: &Reference) -> Option<Val> {
        if !self.scope.iter().any(|(n, _)| looks_like_a_cell(n)) {
            return None;
        }
        if r.shape == Shape::Cell {
            let name = cell_name(r.first.row, r.first.col);
            if let Some((_, val)) =
                self.scope.iter().rev().find(|(n, _)| n.eq_ignore_ascii_case(&name))
            {
                return Some(val.clone());
            }
        }
        Some(Val::error(ErrorKind::Name))
    }

    fn name(&mut self, name: &Arc<str>) -> Val {
        if let Some((_, val)) = self.scope.iter().rev().find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return val.clone();
        }
        let (scope_sheet, bare) = match name.split_once('!') {
            Some((sheet, rest)) => match self.book.sheet_index(sheet.trim_matches('\'')) {
                Some(index) => (index, rest),
                None => return Val::error(ErrorKind::Ref),
            },
            None => (self.sheet, &**name),
        };
        if let Some(defined) = self.book.name(bare, scope_sheet) {
            if self.depth > DEEPEST {
                return Val::error(ErrorKind::Num);
            }
            let expr = match crate::formula::parser::parse(defined.formula.trim_start_matches('='))
            {
                Ok(expr) => expr,
                Err(_) => return Val::error(ErrorKind::Name),
            };
            let outer = std::mem::take(&mut self.scope);
            self.depth += 1;
            let v = self.eval(&expr);
            self.depth -= 1;
            self.scope = outer;
            return v;
        }
        if let Some(table) = self.book.table(bare) {
            return match table.data_range() {
                Some(range) => Val::Ref(Area::new(table.sheet, range)),
                None => Val::error(ErrorKind::Ref),
            };
        }
        Val::error(ErrorKind::Name)
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Val {
        if let Some(function) = crate::functions::find(name) {
            if args.len() < function.min || args.len() > function.max {
                return Val::error(ErrorKind::Value);
            }
            return match function.imp {
                crate::functions::Imp::Scalar(f) => {
                    self.lift(args, |ctx, values| f(ctx, values).unwrap_or_else(Value::Error))
                }
                crate::functions::Imp::Raw(f) => f(self, args),
            };
        }
        // A LAMBDA bound to a name or a LET variable, called by that name.
        let name: Arc<str> = Arc::from(name);
        match self.name(&name) {
            Val::Lambda(lambda) => self.invoke(&lambda, args),
            Val::Value(Value::Error(ErrorKind::Name))
            | Val::Value(Value::Error(ErrorKind::Ref)) => Val::error(ErrorKind::Name),
            _ => Val::error(ErrorKind::Value),
        }
    }

    /// Call a LAMBDA with argument expressions.
    pub fn invoke(&mut self, lambda: &Lambda, args: &[Expr]) -> Val {
        let values: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
        self.invoke_with(lambda, values)
    }

    /// Call a LAMBDA with values already computed, which is what MAP and REDUCE do.
    pub fn invoke_with(&mut self, lambda: &Lambda, values: Vec<Val>) -> Val {
        if values.len() > lambda.params.len() || values.len() < lambda.required {
            return Val::error(ErrorKind::Value);
        }
        if self.depth > DEEPEST {
            return Val::error(ErrorKind::Num);
        }
        let mut scope = lambda.scope.clone();
        for (i, param) in lambda.params.iter().enumerate() {
            // A parameter left out is an empty union, which is what ISOMITTED looks for.
            let value = values.get(i).cloned().unwrap_or(Val::Refs(Vec::new()));
            scope.push((param.clone(), value));
        }
        let outer = std::mem::replace(&mut self.scope, scope);
        self.depth += 1;
        let result = self.eval(&lambda.body);
        self.depth -= 1;
        self.scope = outer;
        result
    }

    fn spill(&mut self, inner: &Expr) -> Val {
        match self.eval(inner) {
            Val::Ref(area) if area.range.first == area.range.last && !area.is_3d() => {
                let Some(sheet) = self.book.sheets.get(area.sheet) else {
                    return Val::error(ErrorKind::Ref);
                };
                let anchor = (area.range.first.row, area.range.first.col);
                match sheet.spills.range_of(anchor) {
                    Some(range) => Val::Ref(Area::new(area.sheet, range)),
                    None if sheet.cell(area.range.first).is_some_and(|c| c.formula.is_some()) => {
                        Val::Ref(area)
                    }
                    None => Val::error(ErrorKind::Ref),
                }
            }
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Ref),
        }
    }

    fn table(&mut self, t: &TableRef) -> Val {
        match table_area(self.book, t, self.sheet, Some(self.at)) {
            Ok((sheet, range)) => Val::Ref(Area::new(sheet, range)),
            Err(e) => Val::error(e),
        }
    }

    /// One value out of anything: a single cell, the first of an array, the cell of a range in the
    /// formula's own row or column.
    pub fn single(&mut self, val: Val) -> Value {
        match val {
            Val::Value(v) => v,
            Val::Array(a) => a.data.first().cloned().unwrap_or(Value::Error(ErrorKind::Value)),
            Val::Ref(area) if area.range.first == area.range.last && !area.is_3d() => {
                self.cell_value(area.sheet, area.range.first)
            }
            other => self.intersect(other),
        }
    }

    /// Implicit intersection: the cell of a range in the formula's own row or column.
    pub fn intersect(&mut self, val: Val) -> Value {
        match val {
            Val::Value(v) => v,
            Val::Array(a) => a.data.first().cloned().unwrap_or(Value::Error(ErrorKind::Value)),
            Val::Ref(area) if !area.is_3d() => {
                let r = area.range;
                if r.first == r.last {
                    return self.cell_value(area.sheet, r.first);
                }
                if r.first.col == r.last.col
                    && r.first.row <= self.at.row
                    && self.at.row <= r.last.row
                {
                    return self.cell_value(area.sheet, CellRef::new(self.at.row, r.first.col));
                }
                if r.first.row == r.last.row
                    && r.first.col <= self.at.col
                    && self.at.col <= r.last.col
                {
                    return self.cell_value(area.sheet, CellRef::new(r.first.row, self.at.col));
                }
                Value::Error(ErrorKind::Value)
            }
            Val::Lambda(_) => Value::Error(ErrorKind::Calc),
            _ => Value::Error(ErrorKind::Value),
        }
    }

    pub fn cell_value(&self, sheet: usize, at: CellRef) -> Value {
        self.book.sheets.get(sheet).map_or(Value::Error(ErrorKind::Ref), |s| s.value_at(at))
    }

    /// The rows and columns a value has, or `None` for one that cannot be lifted over.
    pub fn dims(&self, val: &Val) -> Option<(usize, usize)> {
        match val {
            Val::Value(_) => Some((1, 1)),
            Val::Array(a) => Some((a.rows, a.cols)),
            Val::Ref(area) if !area.is_3d() => {
                Some((area.range.rows() as usize, area.range.cols() as usize))
            }
            Val::Refs(areas) if areas.is_empty() => Some((1, 1)),
            _ => None,
        }
    }

    /// The value at one place of something lifted over, stretching a single row or column.
    pub fn element(&self, val: &Val, row: usize, col: usize) -> Value {
        match val {
            Val::Value(v) => v.clone(),
            Val::Array(a) => a.stretched(row, col),
            Val::Ref(area) => {
                let r = area.range;
                let (rows, cols) = (r.rows() as usize, r.cols() as usize);
                let rr = if rows == 1 { 0 } else { row };
                let cc = if cols == 1 { 0 } else { col };
                if rr >= rows || cc >= cols {
                    return Value::Error(ErrorKind::NA);
                }
                self.cell_value(
                    area.sheet,
                    CellRef::new(r.first.row + rr as u32, r.first.col + cc as u32),
                )
            }
            Val::Lambda(_) => Value::Error(ErrorKind::Calc),
            // An omitted LAMBDA parameter reads as an empty cell.
            Val::Refs(ref areas) if areas.is_empty() => Value::Empty,
            Val::Refs(_) => Value::Error(ErrorKind::Value),
        }
    }

    /// Apply a function of single values over arguments, lifting over any that are ranges or arrays.
    pub fn lift(&mut self, args: &[Expr], f: impl Fn(&mut Ctx, &[Value]) -> Value) -> Val {
        let vals: Vec<Val> = args.iter().map(|a| self.eval(a)).collect();
        self.lift_vals(vals, f)
    }

    pub fn lift_vals(&mut self, vals: Vec<Val>, f: impl Fn(&mut Ctx, &[Value]) -> Value) -> Val {
        let vals: Vec<Val> = if self.legacy {
            vals.into_iter()
                .map(|v| match v {
                    Val::Ref(area) if area.range.len() > 1 => {
                        Val::Value(self.intersect(Val::Ref(area)))
                    }
                    other => other,
                })
                .collect()
        } else {
            vals
        };
        let mut rows = 1;
        let mut cols = 1;
        for v in &vals {
            match self.dims(v) {
                Some((r, c)) => {
                    rows = rows.max(r);
                    cols = cols.max(c);
                }
                None => {
                    return Val::error(match v {
                        Val::Lambda(_) => ErrorKind::Calc,
                        _ => ErrorKind::Value,
                    })
                }
            }
        }
        if rows * cols == 1 {
            let values: Vec<Value> = vals.iter().map(|v| self.element(v, 0, 0)).collect();
            return Val::Value(f(self, &values));
        }
        if rows * cols > 4_000_000 {
            return Val::error(ErrorKind::Num);
        }
        let mut data = Vec::with_capacity(rows * cols);
        let mut values = Vec::with_capacity(vals.len());
        for r in 0..rows {
            for c in 0..cols {
                values.clear();
                values.extend(vals.iter().map(|v| self.element(v, r, c)));
                data.push(f(self, &values));
            }
        }
        Val::Array(Array::new(rows, cols, data))
    }

    fn map1(&mut self, v: Val, f: impl Fn(&mut Ctx, Value) -> Value) -> Val {
        self.lift_vals(vec![v], |ctx, values| f(ctx, values[0].clone()))
    }

    fn map2(&mut self, a: Val, b: Val, f: impl Fn(&mut Ctx, Value, Value) -> Value) -> Val {
        self.lift_vals(vec![a, b], |ctx, values| f(ctx, values[0].clone(), values[1].clone()))
    }

    /// A value as a number, the way arithmetic coerces it.
    pub fn number(&self, v: &Value) -> Result<f64, ErrorKind> {
        to_number(v, self.date1904())
    }

    /// Turn anything into an array of its values: what a range becomes when a function wants values.
    pub fn array(&mut self, val: Val) -> Result<Array, ErrorKind> {
        match val {
            Val::Value(Value::Error(e)) => Err(e),
            Val::Value(v) => Ok(Array::single(v)),
            Val::Array(a) => Ok(a),
            Val::Ref(area) if !area.is_3d() => {
                let r = area.range;
                if r.len() > 4_000_000 {
                    // A whole column or row: what is past the used range is empty.
                    let sheet = self.book.sheets.get(area.sheet).ok_or(ErrorKind::Ref)?;
                    let used = sheet.used_range();
                    let clipped = match used {
                        Some(u) => Range::new(
                            r.first,
                            CellRef::new(
                                r.last.row.min(u.last.row.max(r.first.row)),
                                r.last.col.min(u.last.col.max(r.first.col)),
                            ),
                        ),
                        None => Range::cell(r.first),
                    };
                    return Ok(self.read_range(area.sheet, clipped));
                }
                Ok(self.read_range(area.sheet, r))
            }
            Val::Lambda(_) => Err(ErrorKind::Calc),
            _ => Err(ErrorKind::Value),
        }
    }

    pub fn read_range(&self, sheet: usize, r: Range) -> Array {
        let mut data = Vec::with_capacity(r.len() as usize);
        let s = &self.book.sheets[sheet];
        for row in r.first.row..=r.last.row {
            for col in r.first.col..=r.last.col {
                data.push(s.value_at(CellRef::new(row, col)));
            }
        }
        Array::new(r.rows() as usize, r.cols() as usize, data)
    }

    /// Every value a set of arguments holds, each with whether it came from a reference or an array
    /// (`true`) or was written directly as an argument (`false`). Empty cells are left out.
    pub fn values(&mut self, args: &[Expr]) -> Vec<(Value, bool)> {
        let mut out = Vec::new();
        for arg in args {
            let v = self.eval(arg);
            self.values_of(v, &mut out);
        }
        out
    }

    pub fn values_of(&mut self, v: Val, out: &mut Vec<(Value, bool)>) {
        match v {
            Val::Value(v) => out.push((v, false)),
            Val::Array(a) => out.extend(a.data.into_iter().map(|v| (v, true))),
            Val::Ref(area) => self.area_values(&area, &mut |v| out.push((v.clone(), true))),
            Val::Refs(areas) => {
                for area in areas {
                    self.area_values(&area, &mut |v| out.push((v.clone(), true)));
                }
            }
            Val::Lambda(_) => out.push((Value::Error(ErrorKind::Calc), false)),
        }
    }

    /// Call `visit` for every non-empty value in an area, row by row, reading only cells that exist.
    pub fn area_values(&self, area: &Area, visit: &mut impl FnMut(&Value)) {
        for sheet in area.sheet..=area.sheet_last {
            let Some(s) = self.book.sheets.get(sheet) else { continue };
            let r = area.range;
            for (&(_, col), cell) in s.cells.range((r.first.row, 0)..=(r.last.row, MAX_COLS)) {
                if col >= r.first.col && col <= r.last.col && !cell.value.is_empty() {
                    visit(&cell.value);
                }
            }
            for (&(row, col), value) in
                s.spills.values.range((r.first.row, 0)..=(r.last.row, MAX_COLS))
            {
                if col >= r.first.col
                    && col <= r.last.col
                    && !s.cells.contains_key(&(row, col))
                    && !value.is_empty()
                {
                    visit(value);
                }
            }
        }
    }

    /// How many cells an area has, empty ones included.
    pub fn area_size(area: &Area) -> u64 {
        area.range.len() * (area.sheet_last - area.sheet + 1) as u64
    }
}

/// A result type's error turned into a value, for closures that build values.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Result<Value, ErrorKind> {
    fn into_value(self) -> Value {
        self.unwrap_or_else(Value::Error)
    }
}

/// A value as a number: empty is 0, a logical is 1 or 0, text is read the way typing reads it.
pub fn to_number(v: &Value, date1904: bool) -> Result<f64, ErrorKind> {
    match v {
        Value::Empty => Ok(0.0),
        Value::Number(n) => Ok(*n),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Text(t) => crate::input::number_from_text(t, date1904).ok_or(ErrorKind::Value),
        Value::Error(e) => Err(*e),
    }
}

/// A value as text: a number as Excel writes it with no format.
pub fn to_text(v: &Value) -> Result<String, ErrorKind> {
    match v {
        Value::Error(e) => Err(*e),
        Value::Number(n) => Ok(number_to_text(*n)),
        other => Ok(other.to_text()),
    }
}

/// A value as a logical: a number is true when it is not zero, and only the words TRUE and FALSE are
/// logicals among text.
pub fn to_bool(v: &Value) -> Result<bool, ErrorKind> {
    match v {
        Value::Empty => Ok(false),
        Value::Number(n) => Ok(*n != 0.0),
        Value::Bool(b) => Ok(*b),
        Value::Text(t) => match t.to_ascii_uppercase().as_str() {
            "TRUE" => Ok(true),
            "FALSE" => Ok(false),
            _ => Err(ErrorKind::Value),
        },
        Value::Error(e) => Err(*e),
    }
}

/// Excel's order between any two values: numbers, then text without regard to case, then logicals.
/// Empty is compared as 0 against a number, as "" against text and as FALSE against a logical.
pub fn compare(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Number(_) | Value::Empty => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            Value::Error(_) => 3,
        }
    }
    match (a, b) {
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Empty, Value::Text(t)) => {
            if t.is_empty() {
                Ordering::Equal
            } else {
                Ordering::Less
            }
        }
        (Value::Text(t), Value::Empty) => {
            if t.is_empty() {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        (Value::Empty, Value::Bool(x)) => false.cmp(x),
        (Value::Bool(x), Value::Empty) => x.cmp(&false),
        (Value::Empty, Value::Number(n)) => 0.0f64.partial_cmp(n).unwrap_or(Ordering::Equal),
        (Value::Number(n), Value::Empty) => n.partial_cmp(&0.0).unwrap_or(Ordering::Equal),
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Text(x), Value::Text(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// Text compared the way Excel sorts it: without regard to case.
pub fn compare_text(a: &str, b: &str) -> Ordering {
    let mut x = a.chars().flat_map(char::to_lowercase);
    let mut y = b.chars().flat_map(char::to_lowercase);
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                if p != q {
                    return p.cmp(&q);
                }
            }
        }
    }
}

/// Whether two values are equal the way `=` asks: text without regard to case.
pub fn equal(a: &Value, b: &Value) -> bool {
    compare(a, b) == Ordering::Equal
}

fn binary(ctx: &mut Ctx, op: BinOp, a: &Value, b: &Value) -> Value {
    if let Value::Error(e) = a {
        return Value::Error(*e);
    }
    if let Value::Error(e) = b {
        return Value::Error(*e);
    }
    let arithmetic = |f: fn(f64, f64) -> Result<f64, ErrorKind>| -> Value {
        let x = match ctx.number(a) {
            Ok(x) => x,
            Err(e) => return Value::Error(e),
        };
        let y = match ctx.number(b) {
            Ok(y) => y,
            Err(e) => return Value::Error(e),
        };
        match f(x, y) {
            Ok(n) if n.is_finite() => Value::Number(if n == 0.0 { 0.0 } else { n }),
            Ok(_) => Value::Error(ErrorKind::Num),
            Err(e) => Value::Error(e),
        }
    };
    match op {
        BinOp::Add => arithmetic(|x, y| Ok(x + y)),
        BinOp::Sub => arithmetic(|x, y| Ok(x - y)),
        BinOp::Mul => arithmetic(|x, y| Ok(x * y)),
        BinOp::Div => arithmetic(|x, y| if y == 0.0 { Err(ErrorKind::Div0) } else { Ok(x / y) }),
        BinOp::Pow => arithmetic(power),
        BinOp::Concat => match (to_text(a), to_text(b)) {
            (Ok(x), Ok(y)) => Value::text(x + &y),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        },
        BinOp::Eq => Value::Bool(equal(a, b)),
        BinOp::Ne => Value::Bool(!equal(a, b)),
        BinOp::Lt => Value::Bool(compare(a, b) == Ordering::Less),
        BinOp::Le => Value::Bool(compare(a, b) != Ordering::Greater),
        BinOp::Gt => Value::Bool(compare(a, b) == Ordering::Greater),
        BinOp::Ge => Value::Bool(compare(a, b) != Ordering::Less),
        BinOp::Range => Value::Error(ErrorKind::Value),
    }
}

/// `x^y` with Excel's refusals: 0^0 and a negative number to a fractional power are `#NUM!`.
pub fn power(x: f64, y: f64) -> Result<f64, ErrorKind> {
    if x == 0.0 && y == 0.0 {
        return Err(ErrorKind::Num);
    }
    if x == 0.0 && y < 0.0 {
        return Err(ErrorKind::Div0);
    }
    if x < 0.0 && y.fract() != 0.0 {
        // An odd root of a negative number is allowed when 1/y is an odd whole number.
        let inverse = 1.0 / y;
        if (inverse.round() - inverse).abs() < 1e-10 && inverse.round() as i64 % 2 != 0 {
            return Ok(-(-x).powf(y));
        }
        return Err(ErrorKind::Num);
    }
    Ok(x.powf(y))
}

/// Evaluate formula text as though it were in a cell, without changing the workbook: what the
/// `eval` command and the oracle tests use. The answer is the value the cell would show, the top left of
/// an array, with the array itself when there is one.
/// One value from a parsed formula evaluated at a cell, which is what a conditional format rule's formula
/// is, moved to each cell it covers.
/// The name a cell is written with when it is also a LET name: `A1`.
pub fn cell_name(row: u32, col: u32) -> String {
    format!("{}{}", crate::coord::column_name(col), row + 1)
}

/// Whether a name is a cell address, which a LET name may be.
fn looks_like_a_cell(name: &str) -> bool {
    CellRef::parse(name).is_some_and(|at| cell_name(at.row, at.col).eq_ignore_ascii_case(name))
}

pub fn evaluate_expr(book: &Workbook, sheet: usize, at: CellRef, expr: &Expr, now: f64) -> Value {
    let env = Env::new(now, 0x2545_F491_4F6C_DD1D);
    let mut ctx = Ctx::new(book, sheet, at, &env);
    let v = ctx.eval(expr);
    match v {
        Val::Lambda(_) => Value::Error(ErrorKind::Calc),
        other => ctx.single(other),
    }
}

pub fn evaluate(
    book: &Workbook,
    sheet: usize,
    at: CellRef,
    formula: &str,
    now: f64,
) -> Result<(Value, Option<Array>), String> {
    let expr = crate::formula::parser::parse(formula.trim().trim_start_matches('='))?;
    let env = Env::new(now, 0x2545_F491_4F6C_DD1D);
    let mut ctx = Ctx::new(book, sheet, at, &env);
    let v = ctx.eval(&expr);
    Ok(match v {
        Val::Array(a) => spilled(at, a),
        Val::Ref(area) if area.range.len() > 1 && !area.is_3d() => {
            if !fits(at, area.range.rows() as usize, area.range.cols() as usize) {
                return Ok((Value::Error(ErrorKind::Spill), None));
            }
            let a = ctx.array(Val::Ref(area)).unwrap_or_else(|e| Array::single(Value::Error(e)));
            spilled(at, a)
        }
        Val::Lambda(_) => (Value::Error(ErrorKind::Calc), None),
        other => {
            let v = ctx.single(other);
            (if v.is_empty() { Value::Number(0.0) } else { v }, None)
        }
    })
}

/// The cells a structured reference names: its table's sheet and the rows and columns its items and
/// columns choose. `at` is the cell the formula is in, which `[#This Row]` and `[@Col]` need; with no cell
/// they name the whole data body, which is what a dependency is on.
pub fn table_area(
    book: &Workbook,
    t: &TableRef,
    sheet: usize,
    at: Option<CellRef>,
) -> Result<(usize, Range), ErrorKind> {
    let table = match (&t.table, at) {
        (Some(name), _) => book.table(name),
        (None, Some(at)) => book.table_at(sheet, at),
        (None, None) => None,
    };
    let table = table.ok_or(ErrorKind::Ref)?;
    let range = table.range;
    let has = |item: &str| t.items.iter().any(|i| &**i == item);
    let data_first = range.first.row + u32::from(table.header_row);
    let data_last = range.last.row.saturating_sub(u32::from(table.totals_row));
    let (first_row, last_row) = if has("all") {
        (range.first.row, range.last.row)
    } else if has("this row") {
        match at {
            Some(at) if at.row < data_first || at.row > data_last => return Err(ErrorKind::Value),
            Some(at) => (at.row, at.row),
            None => (data_first, data_last),
        }
    } else {
        let headers = has("headers");
        let totals = has("totals");
        let data = has("data") || !headers && !totals;
        if headers && !table.header_row || totals && !table.totals_row {
            return Err(ErrorKind::Ref);
        }
        let first = if headers {
            range.first.row
        } else if data {
            data_first
        } else {
            data_last + 1
        };
        let last = if totals {
            range.last.row
        } else if data {
            data_last
        } else {
            range.first.row
        };
        (first, last)
    };
    let column = |name: &Option<Arc<str>>| -> Option<u32> {
        match name {
            None => None,
            Some(n) => table
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(n))
                .map(|i| range.first.col + i as u32),
        }
    };
    let missing = |name: &Option<Arc<str>>| name.is_some() && column(name).is_none();
    if missing(&t.first_column) || missing(&t.last_column) || last_row < first_row {
        return Err(ErrorKind::Ref);
    }
    let first_col = column(&t.first_column).unwrap_or(range.first.col);
    let last_col = column(&t.last_column).or(column(&t.first_column)).unwrap_or(range.last.col);
    Ok((
        table.sheet,
        Range::new(CellRef::new(first_row, first_col), CellRef::new(last_row, last_col)),
    ))
}

/// An array answer anchored at a cell: its first value and the array, or `#SPILL!` when the array would
/// run past the last row or column of the sheet.
fn spilled(at: CellRef, array: Array) -> (Value, Option<Array>) {
    if !fits(at, array.rows, array.cols) {
        return (Value::Error(ErrorKind::Spill), None);
    }
    (array.data.first().cloned().unwrap_or(Value::Empty), Some(array))
}

/// Whether a block of cells anchored at `at` stays inside the sheet.
pub fn fits(at: CellRef, rows: usize, cols: usize) -> bool {
    rows == 0 || cols == 0 || at.offset(rows as i64 - 1, cols as i64 - 1).is_some()
}
