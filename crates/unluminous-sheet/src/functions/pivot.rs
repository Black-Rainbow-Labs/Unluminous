//! `GETPIVOTDATA`: a figure out of a pivot table, by the names of its fields and items.

use super::{r, Function, MANY};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[r(
    "GETPIVOTDATA",
    2,
    MANY,
    "data_field, pivot_table, [field1, item1], ...",
    "Returns data stored in a PivotTable report.",
    get_pivot_data,
)];

/// The figure of a value field at the crossing of some items of a pivot table.
fn get_pivot_data(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let data = ctx.eval(&args[0]);
    let data = match ctx.single(data) {
        Value::Text(t) => t.to_string(),
        Value::Error(e) => return Val::error(e),
        other => crate::eval::to_text(&other).unwrap_or_default(),
    };
    let area = match ctx.eval(&args[1]) {
        Val::Ref(a) => a,
        Val::Value(Value::Error(e)) => return Val::error(e),
        _ => return Val::error(ErrorKind::Ref),
    };
    if args.len() % 2 != 0 {
        return Val::error(ErrorKind::Ref);
    }
    let mut pairs = Vec::new();
    for pair in args[2..].chunks(2) {
        let field = ctx.eval(&pair[0]);
        let field = match ctx.single(field) {
            Value::Text(t) => t.to_string(),
            Value::Error(e) => return Val::error(e),
            _ => return Val::error(ErrorKind::Ref),
        };
        let item = ctx.eval(&pair[1]);
        let item = ctx.single(item);
        if let Value::Error(e) = item {
            return Val::error(e);
        }
        pairs.push((field, item));
    }
    ctx.book.pivots.get_pivot_data(ctx.book, area.sheet, area.range.first, &data, &pairs).into()
}
