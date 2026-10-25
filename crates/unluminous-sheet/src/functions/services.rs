//! The functions that need a service of Excel's: a data connection, a cloud model, a stock feed, an add in.
//!
//! The engine knows their names, argument counts and prefixes so a workbook that uses them loads, shows them
//! in the completion list and writes them back as it read them. A cell read from a file that holds one keeps
//! the value the file saved and is not recalculated (the reader marks the formula frozen). A formula entered
//! here evaluates to `#BLOCKED!`, which is the error Excel gives a function whose service is switched off.

use super::{offline, Function};
use crate::eval::Ctx;
use crate::value::{ErrorKind, Value};

/// The cube functions, which read an OLAP data source.
pub const CUBE: &[Function] = &[
    offline("CUBEKPIMEMBER", 3, 4, "connection, kpi_name, kpi_property, [caption]", NEEDS_CUBE).answering(cube_kpi),
    offline("CUBEMEMBER", 2, 3, "connection, member_expression, [caption]", NEEDS_CUBE).answering(cube),
    offline("CUBEMEMBERPROPERTY", 3, 3, "connection, member_expression, property", NEEDS_CUBE).answering(cube),
    offline("CUBERANKEDMEMBER", 3, 4, "connection, set_expression, rank, [caption]", NEEDS_CUBE).answering(cube),
    offline(
        "CUBESET",
        2,
        5,
        "connection, set_expression, [caption], [sort_order], [sort_by]",
        NEEDS_CUBE,
    ).answering(cube),
    offline("CUBESETCOUNT", 1, 1, "set", NEEDS_CUBE).answering(cube),
    offline("CUBEVALUE", 1, 255, "connection, [member_expression1], ...", NEEDS_CUBE).answering(cube),
];

const NEEDS_CUBE: &str = "Needs Excel's service: a connection to an OLAP data source.";

/// The functions of add ins and macros.
pub const ADD_IN: &[Function] = &[
    offline(
        "CALL",
        1,
        255,
        "register_id, [argument1], ...",
        "Needs Excel's service: calls a procedure in a library.",
    ),
    offline(
        "EUROCONVERT",
        3,
        5,
        "number, source, target, [full_precision], [triangulation_precision]",
        "Needs Excel's service: the Euro Currency Tools add in.",
    ).answering(not_in_this_excel),
    offline(
        "REGISTER.ID",
        2,
        3,
        "module_text, procedure, [type_text]",
        "Needs Excel's service: the register id of a procedure in a library.",
    ),
];

/// The functions that look something up outside the workbook or inside a pivot table.
pub const LOOKUP: &[Function] = &[
    offline(
        "GETPIVOTDATA",
        2,
        255,
        "data_field, pivot_table, [field1, item1], ...",
        "Needs Excel's service: reads a pivot table.",
    ),
    offline(
        "IMAGE",
        1,
        5,
        "source, [alt_text], [sizing], [height], [width]",
        "Needs Excel's service: fetches a picture from a web address.",
    ).answering(web_address),
    offline(
        "RTD",
        3,
        255,
        "prog_id, server, topic1, [topic2], ...",
        "Needs Excel's service: real time data from a COM server.",
    ).answering(no_server),
];

/// The functions of the text group that call a cloud service.
pub const TEXT: &[Function] = &[
    offline(
        "DETECTLANGUAGE",
        1,
        1,
        "text",
        "Needs Excel's service: the language a text is written in.",
    ).answering(blocked),
    offline(
        "TRANSLATE",
        1,
        3,
        "text, [source_language], [target_language]",
        "Needs Excel's service: translates a text.",
    ).answering(blocked),
];

/// The function of the information group that reads a stock feed.
pub const INFORMATION: &[Function] = &[offline(
    "STOCKHISTORY",
    2,
    255,
    "stock, start_date, [end_date], [interval], [headers], [property0], ...",
    "Needs Excel's service: the price history of a stock.",
).answering(stock_history)];

/// The web function that calls out to a web address.
pub const WEB: &[Function] =
    &[offline("WEBSERVICE", 1, 1, "url", "Needs Excel's service: the answer of a web service.").answering(web_address)];

/// The functions documented apart from the groups.
pub const OTHER: &[Function] = &[
    offline(
        "COPILOT",
        1,
        255,
        "prompt_part1, [context1], [prompt_part2], ...",
        "Needs Excel's service: asks Copilot.",
    ).answering(not_in_this_excel),
    offline(
        "PY",
        1,
        2,
        "python_code, [return_type]",
        "Needs Excel's service: runs Python in the cloud.",
    ),
];

#[cfg(test)]
mod tests {
    #[test]
    fn python_is_blocked_because_nothing_is_sent_to_the_service() {
        crate::functions::testing::check(&[("PY(\"1\")", "#BLOCKED!"), ("PY(\"x\",0)", "#BLOCKED!")]);
    }

    use crate::functions::testing::eval;
    use crate::functions::{all, find};
    use crate::value::{ErrorKind, Value};

    #[test]
    fn a_function_that_needs_excels_service_is_known_and_answers_as_excel_does_without_it() {
        let names = [
            "CALL",
            "COPILOT",
            "CUBEKPIMEMBER",
            "CUBEMEMBER",
            "CUBEMEMBERPROPERTY",
            "CUBERANKEDMEMBER",
            "CUBESET",
            "CUBESETCOUNT",
            "CUBEVALUE",
            "DETECTLANGUAGE",
            "EUROCONVERT",
            "GETPIVOTDATA",
            "IMAGE",
            "PY",
            "REGISTER.ID",
            "RTD",
            "STOCKHISTORY",
            "TRANSLATE",
            "WEBSERVICE",
        ];
        for name in names {
            let f = find(name).unwrap_or_else(|| panic!("{name} is not known"));
            assert!(f.offline && f.about.starts_with("Needs Excel's service"), "{name}");
        }
        assert_eq!(all().iter().filter(|f| f.offline).count(), names.len());
        assert_eq!(eval("WEBSERVICE(\"https://example.com\")"), Value::Error(ErrorKind::Blocked));
        assert_eq!(eval("TRANSLATE(\"a\",\"fr\")"), Value::Error(ErrorKind::Blocked));
        // With no connection a cube function is #N/A, as Excel answers it (tests/oracle/services.txt).
        assert_eq!(eval("CUBESETCOUNT(\"a\")"), Value::Error(ErrorKind::NA));
        assert_eq!(eval("WEBSERVICE()"), Value::Error(ErrorKind::Value));
    }
}

/// The first error among the arguments, which Excel answers with before it looks for the service.
fn first_error(args: &[Value]) -> Option<ErrorKind> {
    args.iter().find_map(|v| if let Value::Error(e) = v { Some(*e) } else { None })
}

/// A cube function with no connection: an error argument, otherwise `#N/A`.
fn cube(_: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    Err(first_error(args).unwrap_or(ErrorKind::NA))
}

/// CUBEKPIMEMBER is `#VALUE!` for a property that is text and not a number, before it looks for the
/// connection.
fn cube_kpi(ctx: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    if let Some(e) = first_error(args) {
        return Err(e);
    }
    if matches!(&args[2], Value::Text(_)) && ctx.number(&args[2]).is_err() {
        return Err(ErrorKind::Value);
    }
    Err(ErrorKind::NA)
}

/// A function this Excel does not have, which it reads as an unknown name whatever the arguments.
fn not_in_this_excel(_: &mut Ctx, _: &[Value]) -> Result<Value, ErrorKind> {
    Err(ErrorKind::Name)
}

/// RTD with no server to start: `#N/A`, even for error arguments.
fn no_server(_: &mut Ctx, _: &[Value]) -> Result<Value, ErrorKind> {
    Err(ErrorKind::NA)
}

/// A function that sends its text to a service: an error argument, otherwise `#BLOCKED!`.
fn blocked(_: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    Err(first_error(args).unwrap_or(ErrorKind::Blocked))
}

/// IMAGE and WEBSERVICE: an error argument, `#VALUE!` for a source that is not a web address, and
/// `#BLOCKED!` for one that is, because nothing is fetched.
fn web_address(_: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    if let Some(e) = first_error(args) {
        return Err(e);
    }
    match &args[0] {
        Value::Text(t) if t.starts_with("http://") || t.starts_with("https://") => {
            Err(ErrorKind::Blocked)
        }
        _ => Err(ErrorKind::Value),
    }
}

/// STOCKHISTORY: an error argument, `#VALUE!` for a start date that is not a date, otherwise
/// `#BLOCKED!`.
fn stock_history(ctx: &mut Ctx, args: &[Value]) -> Result<Value, ErrorKind> {
    if let Some(e) = first_error(args) {
        return Err(e);
    }
    if ctx.number(&args[1]).is_err() {
        return Err(ErrorKind::Value);
    }
    Err(ErrorKind::Blocked)
}
