//! Information functions: what kind of value something is.

use super::{num, r, s, text_of, Function};
use crate::coord::column_name;
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s("ISBLANK", 1, 1, "value", "TRUE for an empty cell.", |_, a| Ok(Value::Bool(a[0].is_empty()))),
    s("ISNUMBER", 1, 1, "value", "TRUE for a number.", |_, a| {
        Ok(Value::Bool(matches!(a[0], Value::Number(_))))
    }),
    s("ISTEXT", 1, 1, "value", "TRUE for text.", |_, a| {
        Ok(Value::Bool(matches!(a[0], Value::Text(_))))
    }),
    s("ISNONTEXT", 1, 1, "value", "TRUE for anything but text.", |_, a| {
        Ok(Value::Bool(!matches!(a[0], Value::Text(_))))
    }),
    s("ISLOGICAL", 1, 1, "value", "TRUE for TRUE or FALSE.", |_, a| {
        Ok(Value::Bool(matches!(a[0], Value::Bool(_))))
    }),
    s("ISERROR", 1, 1, "value", "TRUE for any error.", |_, a| Ok(Value::Bool(a[0].is_error()))),
    s("ISERR", 1, 1, "value", "TRUE for any error but #N/A.", |_, a| {
        Ok(Value::Bool(matches!(a[0], Value::Error(e) if e != ErrorKind::NA)))
    }),
    s("ISNA", 1, 1, "value", "TRUE for #N/A.", |_, a| {
        Ok(Value::Bool(a[0] == Value::Error(ErrorKind::NA)))
    }),
    s("ISEVEN", 1, 1, "number", "TRUE for an even number.", |ctx, a| {
        Ok(Value::Bool(parity_number(ctx, &a[0])?.trunc() % 2.0 == 0.0))
    }),
    s("ISODD", 1, 1, "number", "TRUE for an odd number.", |ctx, a| {
        Ok(Value::Bool(parity_number(ctx, &a[0])?.trunc() % 2.0 != 0.0))
    }),
    r("ISREF", 1, 1, "value", "TRUE for a reference.", |ctx, a| {
        Val::Value(Value::Bool(matches!(ctx.eval(&a[0]), Val::Ref(_) | Val::Refs(_))))
    }),
    r("ISFORMULA", 1, 1, "reference", "TRUE for a cell holding a formula.", |ctx, a| {
        match ctx.eval(&a[0]) {
            Val::Ref(area) => Val::Value(Value::Bool(
                ctx.book.sheets[area.sheet]
                    .cell(area.range.first)
                    .is_some_and(|c| c.formula.is_some()),
            )),
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        }
    }),
    s("NA", 0, 0, "", "The #N/A error.", |_, _| Err(ErrorKind::NA)),
    s("ERROR.TYPE", 1, 1, "error_val", "The number of an error.", |_, a| match &a[0] {
        Value::Error(e) => Ok(Value::Number(e.code())),
        _ => Err(ErrorKind::NA),
    }),
    r(
        "TYPE",
        1,
        1,
        "value",
        "The type of a value: 1 number, 2 text, 4 logical, 16 error, 64 array.",
        |ctx, a| {
            let literal_array = matches!(a[0], Expr::Array(_));
            let v = ctx.eval(&a[0]);
            match v {
                Val::Array(_) if literal_array => Val::number(64.0),
                Val::Array(arr) if arr.data.len() > 1 => Val::number(64.0),
                Val::Ref(area) if area.range.len() > 1 => Val::number(64.0),
                Val::Lambda(_) => Val::number(128.0),
                other => {
                    let v = ctx.single(other);
                    Val::number(v.type_number())
                }
            }
        },
    ),
    s("N", 1, 1, "value", "A value as a number: text is 0.", |_, a| match &a[0] {
        Value::Number(n) => Ok(Value::Number(*n)),
        Value::Bool(b) => Ok(Value::Number(f64::from(u8::from(*b)))),
        Value::Error(e) => Err(*e),
        _ => Ok(Value::Number(0.0)),
    }),
    r(
        "CELL",
        1,
        2,
        "info_type, [reference]",
        "Information about a cell: address, row, col, contents, type, width.",
        cell,
    ),
    r("SHEET", 0, 1, "[value]", "The number of a sheet.", |ctx, a| match a.first() {
        None => Val::number(ctx.sheet as f64 + 1.0),
        Some(e) => match ctx.eval(e) {
            Val::Ref(area) => Val::number(area.sheet as f64 + 1.0),
            Val::Value(Value::Text(t)) => ctx
                .book
                .sheet_index(&t)
                .map_or(Val::error(ErrorKind::NA), |i| Val::number(i as f64 + 1.0)),
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        },
    }),
    r("SHEETS", 0, 1, "[reference]", "How many sheets.", |ctx, a| match a.first() {
        None => Val::number(ctx.book.sheets.len() as f64),
        Some(e) => match ctx.eval(e) {
            Val::Ref(area) => Val::number((area.sheet_last - area.sheet + 1) as f64),
            Val::Value(Value::Error(e)) => Val::error(e),
            _ => Val::error(ErrorKind::Value),
        },
    }),
];

/// The number ISEVEN and ISODD test: a logical is refused, where arithmetic would take it as 1 or 0.
fn parity_number(ctx: &Ctx, v: &Value) -> Result<f64, ErrorKind> {
    match v {
        Value::Bool(_) => Err(ErrorKind::Value),
        other => num(ctx, other),
    }
}

fn cell(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let info = ctx.eval(&args[0]);
    let info = ctx.single(info);
    let info = match text_of(&info) {
        Ok(t) => t.to_ascii_lowercase(),
        Err(e) => return Val::error(e),
    };
    let (sheet, at) = match args.get(1).map(|e| ctx.eval(e)) {
        Some(Val::Ref(area)) => (area.sheet, area.range.first),
        Some(Val::Value(Value::Error(e))) => return Val::error(e),
        Some(_) => return Val::error(ErrorKind::Value),
        None => (ctx.sheet, ctx.at),
    };
    let value = ctx.cell_value(sheet, at);
    let style = ctx.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
    match info.as_str() {
        "address" => Val::text(format!("${}${}", column_name(at.col), at.row + 1)),
        "row" => Val::number(at.row as f64 + 1.0),
        "col" => Val::number(at.col as f64 + 1.0),
        "contents" => Val::Value(value),
        "type" => Val::text(match value {
            Value::Empty => "b",
            Value::Text(_) => "l",
            _ => "v",
        }),
        "width" => column_width(ctx, sheet, at.col),
        "filename" => Val::text(""),
        "format" => Val::text(format_letter(&ctx.book.styles.format_code(style))),
        "color" => Val::number(f64::from(u8::from(negative_has_a_colour(
            &ctx.book.styles.format_code(style),
        )))),
        "parentheses" => Val::number(f64::from(u8::from(
            ctx.book.styles.format_code(style).split(';').next().is_some_and(|s| s.contains('(')),
        ))),
        "prefix" => Val::text(label_prefix(ctx, style, &value)),
        "protect" => {
            Val::number(if ctx.book.styles.xf(style).locked.unwrap_or(true) { 1.0 } else { 0.0 })
        }
        _ => Val::error(ErrorKind::Value),
    }
}

/// CELL("width"): the column's width as a whole number, and whether it is still the default width.
fn column_width(ctx: &Ctx, sheet: usize, col: u32) -> Val {
    let s = &ctx.book.sheets[sheet];
    let own = s.column(col).and_then(|c| c.width);
    let width = own.or(s.default_col_width).unwrap_or(8.43);
    Val::Array(crate::value::Array::row(vec![
        Value::Number(width.trunc()),
        Value::Bool(own.is_none()),
    ]))
}

/// CELL("prefix"): the character that says how a label is aligned, and nothing for a number.
fn label_prefix(ctx: &Ctx, style: u32, value: &Value) -> &'static str {
    if !matches!(value, Value::Text(_)) {
        return "";
    }
    match ctx.book.styles.xf(style).align.horizontal.as_deref() {
        Some("center") => "^",
        Some("right") => "\"",
        Some("fill") => "\\",
        _ => "'",
    }
}

/// Whether a number format colours its negative numbers, which CELL("color") reports.
fn negative_has_a_colour(code: &str) -> bool {
    code.split(';').nth(1).is_some_and(|s| s.contains('['))
}

/// CELL("format")'s short code for a number format.
fn format_letter(code: &str) -> String {
    if code == "General" {
        return "G".to_owned();
    }
    if crate::numfmt::Format::parse(code).is_date() {
        return date_letter(code).to_owned();
    }
    let positive = code.split(';').next().unwrap_or("");
    let decimals = positive
        .split('.')
        .nth(1)
        .map_or(0, |d| d.chars().take_while(|c| matches!(*c, '0' | '#' | '?')).count());
    let kind = if positive.contains('%') {
        "P"
    } else if positive.contains("E+") {
        "S"
    } else if positive.contains('$') {
        "C"
    } else if positive.contains(',') {
        ","
    } else {
        "F"
    };
    let colour = if negative_has_a_colour(code) { "-" } else { "" };
    format!("{kind}{decimals}{colour}")
}

/// The D code of a date or time format; a format Excel has no code for is D1.
fn date_letter(code: &str) -> &'static str {
    match code.to_ascii_lowercase().as_str() {
        "d-mmm-yy" | "dd-mmm-yy" => "D1",
        "d-mmm" | "dd-mmm" => "D2",
        "mmm-yy" => "D3",
        "m/d/yy" | "m/d/yyyy" | "mm/dd/yy" | "m/d/yy h:mm" | "m/d/yyyy h:mm" => "D4",
        "mm/dd" => "D5",
        "h:mm:ss am/pm" => "D6",
        "h:mm am/pm" => "D7",
        "h:mm:ss" => "D8",
        "h:mm" => "D9",
        _ => "D1",
    }
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{book, eval_on, shown};

    #[test]
    fn information_answers_what_excel_answers() {
        let b = book(&[("A1", "5"), ("A2", "text")]);
        for (f, want) in [
            ("ISBLANK(A3)", "TRUE"),
            ("ISBLANK(A1)", "FALSE"),
            ("ISNUMBER(A1)", "TRUE"),
            ("ISTEXT(A2)", "TRUE"),
            ("ISERROR(1/0)", "TRUE"),
            ("ISERR(NA())", "FALSE"),
            ("ISNA(NA())", "TRUE"),
            ("ISREF(A1)", "TRUE"),
            ("ISREF(1)", "FALSE"),
            ("TYPE(A2)", "2"),
            ("TYPE({1,2})", "64"),
            ("ERROR.TYPE(1/0)", "2"),
            ("N(TRUE)", "1"),
            ("CELL(\"address\",B7)", "$B$7"),
            ("CELL(\"row\",B7)", "7"),
            ("SHEET()", "1"),
            ("SHEETS()", "1"),
            ("ISEVEN(4)", "TRUE"),
        ] {
            assert_eq!(shown(&eval_on(&b, f)), want, "{f}");
        }
    }
}
