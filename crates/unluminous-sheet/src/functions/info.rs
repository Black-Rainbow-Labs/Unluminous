//! Information functions: what kind of value something is.

use super::{num, r, s, text_of, Function};
use crate::coord::column_name;
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

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
        Ok(Value::Bool(num(ctx, &a[0])?.trunc() % 2.0 == 0.0))
    }),
    s("ISODD", 1, 1, "number", "TRUE for an odd number.", |ctx, a| {
        Ok(Value::Bool(num(ctx, &a[0])?.trunc() % 2.0 != 0.0))
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
            let v = ctx.eval(&a[0]);
            match v {
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
    s(
        "INFO",
        1,
        1,
        "type_text",
        "Information about the program and the workbook: osversion, system, recalc, ...",
        info,
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
        "width" => cell_width(ctx, sheet, at),
        "color" => format_flag(ctx, sheet, at, Section::Negative, number_color),
        "parentheses" => format_flag(ctx, sheet, at, Section::Positive, has_parenthesis),
        "prefix" => Val::text(label_prefix(ctx, sheet, at, &value)),
        "filename" => Val::text(""),
        "format" => {
            let style = ctx.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
            let code = ctx.book.styles.format_code(style);
            Val::text(format_letter(&code))
        }
        "protect" => {
            let style = ctx.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
            Val::number(if ctx.book.styles.xf(style).locked.unwrap_or(true) { 1.0 } else { 0.0 })
        }
        _ => Val::error(ErrorKind::Value),
    }
}

/// CELL("width"): the column width in characters, and whether it is the default width, as a row of two.
fn cell_width(ctx: &Ctx, sheet: usize, at: crate::coord::CellRef) -> Val {
    let sheet = &ctx.book.sheets[sheet];
    let own = sheet.column(at.col).and_then(|c| c.width);
    let width = own.or(sheet.default_col_width).unwrap_or(8.43);
    Val::Array(Array::row(vec![Value::Number(width.trunc()), Value::Bool(own.is_none())]))
}

/// Which part of a number format a CELL question looks at.
#[derive(Clone, Copy)]
enum Section {
    Positive,
    Negative,
}

/// 1 when one section of the cell's number format satisfies `test`, and 0 when it does not.
fn format_flag(
    ctx: &Ctx,
    sheet: usize,
    at: crate::coord::CellRef,
    which: Section,
    test: fn(&str) -> bool,
) -> Val {
    let style = ctx.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
    let code = ctx.book.styles.format_code(style);
    let index = match which {
        Section::Positive => 0,
        Section::Negative => 1,
    };
    let yes = format_sections(&code).get(index).is_some_and(|s| test(s));
    Val::number(f64::from(u8::from(yes)))
}

/// A number format split at its semicolons, leaving out those inside quotes.
fn format_sections(code: &str) -> Vec<String> {
    let mut sections = vec![String::new()];
    let mut quoted = false;
    for c in code.chars() {
        if c == ';' && !quoted {
            sections.push(String::new());
            continue;
        }
        quoted ^= c == '"';
        if let Some(last) = sections.last_mut() {
            last.push(c);
        }
    }
    sections
}

/// Whether a format section names a color, such as `[Red]`.
fn number_color(section: &str) -> bool {
    const COLORS: [&str; 8] =
        ["black", "blue", "cyan", "green", "magenta", "red", "white", "yellow"];
    section.split('[').skip(1).filter_map(|rest| rest.split(']').next()).any(|name| {
        let name = name.to_ascii_lowercase();
        COLORS.contains(&name.as_str())
            || name.strip_prefix("color").is_some_and(|n| n.trim().parse::<u8>().is_ok())
    })
}

/// Whether a format section writes a parenthesis outside quotes.
fn has_parenthesis(section: &str) -> bool {
    let mut quoted = false;
    for c in section.chars() {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => return true,
            _ => {}
        }
    }
    false
}

/// CELL("prefix"): the quote, caret, double quote or backslash a label is aligned with, none for a number.
fn label_prefix(ctx: &Ctx, sheet: usize, at: crate::coord::CellRef, value: &Value) -> &'static str {
    if !matches!(value, Value::Text(_)) {
        return "";
    }
    let style = ctx.book.sheets[sheet].cell(at).map_or(0, |c| c.style);
    match ctx.book.styles.xf(style).align.horizontal.as_deref() {
        Some("center") | Some("centerContinuous") => "^",
        Some("right") => "\"",
        Some("fill") => "\\",
        _ => "'",
    }
}

/// INFO: facts about the program and the workbook. Types Excel no longer answers are `#N/A`.
fn info(ctx: &mut Ctx, a: &[Value]) -> Result<Value, ErrorKind> {
    let kind = text_of(&a[0])?.to_ascii_lowercase();
    Ok(Value::text(match kind.as_str() {
        "osversion" => os_version(),
        "system" => if cfg!(target_os = "macos") { "mac" } else { "pcdos" }.to_owned(),
        "recalc" => if ctx.book.calc.automatic { "Automatic" } else { "Manual" }.to_owned(),
        "numfile" => return Ok(Value::Number(ctx.book.sheets.len() as f64)),
        "origin" => "$A:$A$1".to_owned(),
        "release" => "16.0".to_owned(),
        "directory" => working_directory(),
        "memavail" | "memused" | "totmem" => return Err(ErrorKind::NA),
        _ => return Err(ErrorKind::Value),
    }))
}

/// The operating system as INFO("osversion") words it.
fn os_version() -> String {
    match std::env::consts::OS {
        "windows" => "Windows (64-bit) NT 10.00".to_owned(),
        "macos" => "Macintosh (64-bit)".to_owned(),
        other => format!("{other} (64-bit)"),
    }
}

/// The current folder with a closing separator, as INFO("directory") gives it.
fn working_directory() -> String {
    let mut text = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
    if !text.ends_with(std::path::MAIN_SEPARATOR) {
        text.push(std::path::MAIN_SEPARATOR);
    }
    text
}

/// CELL("format")'s short code for a number format.
fn format_letter(code: &str) -> String {
    let f = crate::numfmt::Format::parse(code);
    if code == "General" {
        return "G".to_owned();
    }
    if f.is_date() {
        return "D1".to_owned();
    }
    let decimals = code
        .split(';')
        .next()
        .unwrap_or("")
        .split('.')
        .nth(1)
        .map_or(0, |d| d.chars().take_while(|c| *c == '0').count());
    let kind = if code.contains('%') {
        "P"
    } else if code.contains("E+") {
        "S"
    } else if code.contains('$') {
        "C"
    } else if code.contains(',') {
        ","
    } else {
        "F"
    };
    format!("{kind}{decimals}")
}

#[cfg(test)]
mod tests {
    use super::{format_sections, has_parenthesis, number_color};
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
    #[test]
    fn info_and_the_newer_cell_types_answer_what_excel_answers() {
        let b = book(&[("A1", "5"), ("A2", "text")]);
        for (f, want) in [
            ("INFO(\"recalc\")", "Automatic"),
            ("INFO(\"numfile\")", "1"),
            ("INFO(\"origin\")", "$A:$A$1"),
            ("INFO(\"release\")", "16.0"),
            ("INFO(\"RELEASE\")", "16.0"),
            ("INFO(\"bogus\")", "#VALUE!"),
            ("INFO(\"memavail\")", "#N/A"),
            ("INFO(1)", "#VALUE!"),
            ("CELL(\"color\",A1)", "0"),
            ("CELL(\"parentheses\",A1)", "0"),
            ("CELL(\"prefix\",A2)", "'"),
            ("CELL(\"prefix\",A1)", ""),
            ("COLUMNS(CELL(\"width\",A1))", "2"),
            ("INDEX(CELL(\"width\",A1),1,1)", "8"),
            ("INDEX(CELL(\"width\",A1),1,2)", "TRUE"),
            ("CELL(\"sheetname\",A1)", "#VALUE!"),
        ] {
            assert_eq!(shown(&eval_on(&b, f)), want, "{f}");
        }
        for kind in ["osversion", "system", "directory"] {
            let got = shown(&eval_on(&b, &format!("INFO(\"{kind}\")")));
            assert!(!got.is_empty() && !got.starts_with('#'), "{kind}: {got}");
        }
    }

    #[test]
    fn color_and_parentheses_read_the_sections_of_the_number_format() {
        assert!(number_color("[Red]-0"));
        assert!(number_color("[Color 3]0") && number_color("[Color3]0"));
        assert!(!number_color("0.00"));
        assert!(has_parenthesis("(#,##0)"));
        assert!(!has_parenthesis("\"(\"0"));
        assert_eq!(format_sections("0;[Red]-0;\"a;b\""), vec!["0", "[Red]-0", "\"a;b\""]);
    }
}
