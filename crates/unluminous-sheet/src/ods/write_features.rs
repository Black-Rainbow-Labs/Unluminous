//! Writing what an `.ods` sheet carries besides its cells: validations, conditional formats, names,
//! rich text and the window settings.

use std::collections::HashMap;

use super::cellstyle::number;
use super::formula::from_a1;
use super::{NS_CONFIG, NS_OFFICE};
use crate::coord::{CellRef, Range};
use crate::formula::{Expr, TableRef};
use crate::model::{CfRule, ConditionalFormat, DefinedName, Table, Validation, Workbook};
use crate::styles::{Color, Dxf, Underline};
use crate::xlsx::escape;

/// A sheet name as an `.ods` address writes it.
pub fn sheet_address(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit());
    if plain {
        name.to_owned()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// A range as `Sheet.A1:Sheet.B2`.
pub fn range_address(sheet: &str, range: Range) -> String {
    let name = sheet_address(sheet);
    if range.first == range.last {
        format!("{name}.{}", range.first.a1())
    } else {
        format!("{name}.{}:{name}.{}", range.first.a1(), range.last.a1())
    }
}

/// An operator's `cell-content()` spelling.
fn operator_symbol(op: &str) -> &'static str {
    match op {
        "notEqual" => "!=",
        "lessThan" => "<",
        "greaterThan" => ">",
        "lessThanOrEqual" => "<=",
        "greaterThanOrEqual" => ">=",
        _ => "=",
    }
}

// ---------------------------------------------------------------------------------------------------
// Formulas.

/// The `of:` body of a formula expression written in a cell of `sheet`.
pub fn formula_body(
    expr: &Expr,
    text: &str,
    frozen: bool,
    book: &Workbook,
    sheet: usize,
    at: CellRef,
) -> String {
    let local: Vec<String> =
        book.names.iter().filter(|n| n.scope == Some(sheet)).map(|n| n.name.clone()).collect();
    if frozen {
        return from_a1(text, &local);
    }
    let expanded = expand_tables(expr, book, sheet, at);
    from_a1(&crate::formula::print::print(&expanded, true), &local)
}

/// The expression with each structured reference replaced by the cells it names.
fn expand_tables(expr: &Expr, book: &Workbook, sheet: usize, at: CellRef) -> Expr {
    let again = |e: &Expr| expand_tables(e, book, sheet, at);
    let boxed = |e: &Expr| Box::new(again(e));
    match expr {
        Expr::Table(t) => {
            table_reference(book, sheet, at, t).map_or_else(|| expr.clone(), Expr::Ref)
        }
        Expr::Neg(e) => Expr::Neg(boxed(e)),
        Expr::Plus(e) => Expr::Plus(boxed(e)),
        Expr::Percent(e) => Expr::Percent(boxed(e)),
        Expr::Spill(e) => Expr::Spill(boxed(e)),
        Expr::Implicit(e) => Expr::Implicit(boxed(e)),
        Expr::Paren(e) => Expr::Paren(boxed(e)),
        Expr::Binary(op, a, b) => Expr::Binary(*op, boxed(a), boxed(b)),
        Expr::Intersect(a, b) => Expr::Intersect(boxed(a), boxed(b)),
        Expr::Call(name, args) => Expr::Call(name.clone(), args.iter().map(again).collect()),
        Expr::Union(args) => Expr::Union(args.iter().map(again).collect()),
        Expr::Invoke(f, args) => Expr::Invoke(boxed(f), args.iter().map(again).collect()),
        other => other.clone(),
    }
}

/// The cells a structured reference names, as a reference on the table's sheet.
fn table_reference(
    book: &Workbook,
    sheet: usize,
    at: CellRef,
    t: &TableRef,
) -> Option<crate::formula::Reference> {
    let table: &Table = match &t.table {
        Some(name) => book.table(name)?,
        None => book.table_at(sheet, at)?,
    };
    let range = table.range;
    let has = |item: &str| t.items.iter().any(|i| &**i == item);
    let data_first = range.first.row + u32::from(table.header_row);
    let data_last = range.last.row.saturating_sub(u32::from(table.totals_row));
    let (first_row, last_row) = if has("all") {
        (range.first.row, range.last.row)
    } else if has("this row") {
        (at.row, at.row)
    } else if has("headers") {
        (range.first.row, range.first.row)
    } else if has("totals") {
        (range.last.row, range.last.row)
    } else {
        (data_first, data_last)
    };
    let column = |name: &Option<std::sync::Arc<str>>, default: u32| match name {
        None => Some(default),
        Some(n) => table
            .columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(n))
            .map(|i| range.first.col + i as u32),
    };
    let first_col = column(&t.first_column, range.first.col)?;
    let last_col = column(&t.last_column, range.last.col)?;
    let area = Range::new(CellRef::new(first_row, first_col), CellRef::new(last_row, last_col));
    let name = std::sync::Arc::<str>::from(book.sheets.get(table.sheet)?.name.as_str());
    Some(crate::formula::Reference::from_range(area, Some(name)))
}

// ---------------------------------------------------------------------------------------------------
// Validations.

/// The `table:content-validation` element for a validation, named `name`.
pub fn validation_xml(v: &Validation, name: &str, sheet: &str) -> String {
    let base = v.ranges.first().map_or(CellRef::new(0, 0), |r| r.first);
    let condition = validation_condition(v);
    let mut xml = format!(
        "<table:content-validation table:name=\"{name}\" table:condition=\"{}\"",
        escape(&condition)
    );
    if !v.allow_blank {
        xml.push_str(" table:allow-empty-cell=\"false\"");
    }
    if !v.show_dropdown {
        xml.push_str(" table:display-list=\"none\"");
    }
    xml.push_str(&format!(
        " table:base-cell-address=\"{}\">",
        range_address(sheet, Range::cell(base))
    ));
    xml.push_str(&message_xml(
        "help-message",
        v.prompt_title.as_deref(),
        v.prompt.as_deref(),
        None,
    ));
    xml.push_str(&message_xml(
        "error-message",
        v.error_title.as_deref(),
        v.error.as_deref(),
        v.error_style.as_deref(),
    ));
    xml.push_str("</table:content-validation>");
    xml
}

/// A help or error message element.
fn message_xml(tag: &str, title: Option<&str>, text: Option<&str>, style: Option<&str>) -> String {
    let shown = title.is_some() || text.is_some();
    let mut xml = format!("<table:{tag}");
    if let Some(t) = title {
        xml.push_str(&format!(" table:title=\"{}\"", escape(t)));
    }
    xml.push_str(&format!(" table:display=\"{shown}\""));
    if let Some(s) = style.filter(|s| *s != "stop") {
        xml.push_str(&format!(" table:message-type=\"{s}\""));
    }
    match text {
        Some(t) => {
            let paragraphs: String =
                t.split('\n').map(|p| format!("<text:p>{}</text:p>", escape(p))).collect();
            xml.push_str(&format!(">{paragraphs}</table:{tag}>"));
        }
        None => xml.push_str("/>"),
    }
    xml
}

/// The condition text of a validation.
fn validation_condition(v: &Validation) -> String {
    let arg = |f: &Option<String>| from_a1(f.as_deref().unwrap_or("0"), &[]);
    let type_part = match v.kind.as_str() {
        "whole" => "cell-content-is-whole-number()",
        "decimal" => "cell-content-is-decimal-number()",
        "date" => "cell-content-is-date()",
        "time" => "cell-content-is-time()",
        _ => "",
    };
    let compare = |between: &str, op: &str| match v.operator.as_deref() {
        Some("between") => {
            format!("cell-content-is-between({},{})", arg(&v.formula1), arg(&v.formula2))
        }
        Some("notBetween") => {
            format!("cell-content-is-not-between({},{})", arg(&v.formula1), arg(&v.formula2))
        }
        Some(o) => format!("{op}{}{}", operator_symbol(o), arg(&v.formula1)),
        None => between.to_owned(),
    };
    match v.kind.as_str() {
        "list" => format!(
            "of:cell-content-is-in-list({})",
            list_arguments(v.formula1.as_deref().unwrap_or(""))
        ),
        "custom" => format!("of:is-true-formula({})", arg(&v.formula1)),
        "textLength" => {
            let text = match v.operator.as_deref() {
                Some("between") => format!(
                    "cell-content-text-length-is-between({},{})",
                    arg(&v.formula1),
                    arg(&v.formula2)
                ),
                Some("notBetween") => {
                    format!(
                        "cell-content-text-length-is-not-between({},{})",
                        arg(&v.formula1),
                        arg(&v.formula2)
                    )
                }
                Some(o) => {
                    format!("cell-content-text-length(){}{}", operator_symbol(o), arg(&v.formula1))
                }
                None => "cell-content-text-length()>=0".to_owned(),
            };
            format!("of:{text}")
        }
        _ => {
            let comparison = compare("", "cell-content()");
            if comparison.is_empty() {
                format!("of:{type_part}")
            } else {
                format!("of:{type_part} and {comparison}")
            }
        }
    }
}

/// The arguments of `cell-content-is-in-list`: quoted items, or a range.
fn list_arguments(formula: &str) -> String {
    let trimmed = formula.trim();
    match trimmed.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(items) => items
            .split(',')
            .map(|i| format!("\"{}\"", i.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(";"),
        None => from_a1(trimmed, &[]),
    }
}

// ---------------------------------------------------------------------------------------------------
// Conditional formats.

/// The `of:` condition a rule becomes in a `style:map`, or `None` when no condition says it.
pub fn map_condition(rule: &CfRule, range: Range, sheet: &str) -> Option<String> {
    let first = range.first.a1();
    let all = range_text(range);
    let arg = |i: usize| from_a1(rule.formulas.get(i).map_or("0", String::as_str), &[]);
    let text = match rule.kind.as_str() {
        "cellIs" => match rule.operator.as_deref() {
            Some("between") => format!("cell-content-is-between({},{})", arg(0), arg(1)),
            Some("notBetween") => format!("cell-content-is-not-between({},{})", arg(0), arg(1)),
            Some(o) => format!("cell-content(){}{}", operator_symbol(o), arg(0)),
            None => return None,
        },
        "expression" | "containsText" | "notContainsText" | "beginsWith" | "endsWith"
        | "containsBlanks" | "notContainsBlanks" | "containsErrors" | "notContainsErrors" => {
            format!("is-true-formula({})", arg(0))
        }
        "duplicateValues" => format!("is-true-formula(COUNTIF({all};[.{first}])>1)"),
        "uniqueValues" => format!("is-true-formula(COUNTIF({all};[.{first}])=1)"),
        "aboveAverage" => {
            let op = if rule.above_average.unwrap_or(true) { ">" } else { "<" };
            format!("is-true-formula([.{first}]{op}AVERAGE({all}))")
        }
        "top10" if !rule.percent => {
            let order = if rule.bottom { ";1" } else { "" };
            format!("is-true-formula(RANK([.{first}];{all}{order})<={})", rule.rank.unwrap_or(10))
        }
        _ => return None,
    };
    let _ = sheet;
    Some(format!("of:{text}"))
}

/// A range as an absolute reference in `of:` syntax.
fn range_text(range: Range) -> String {
    let abs = |c: CellRef| format!("${}${}", crate::coord::column_name(c.col), c.row + 1);
    format!("[.{}:.{}]", abs(range.first), abs(range.last))
}

/// A differential format as a table cell style.
pub fn dxf_style_xml(dxf: &Dxf, name: &str, data_style: Option<&str>) -> String {
    let mut text = String::new();
    if let Some(Color::Rgb(argb)) = dxf.font_color {
        text.push_str(&format!(" fo:color=\"#{:06X}\"", argb & 0x00FF_FFFF));
    }
    if let Some(b) = dxf.bold {
        text.push_str(if b { " fo:font-weight=\"bold\"" } else { " fo:font-weight=\"normal\"" });
    }
    if let Some(i) = dxf.italic {
        text.push_str(if i { " fo:font-style=\"italic\"" } else { " fo:font-style=\"normal\"" });
    }
    if let Some(s) = dxf.strike {
        text.push_str(if s {
            " style:text-line-through-style=\"solid\""
        } else {
            " style:text-line-through-style=\"none\""
        });
    }
    if let Some(u) = dxf.underline {
        text.push_str(if u == Underline::None {
            " style:text-underline-style=\"none\""
        } else {
            " style:text-underline-style=\"solid\" style:text-underline-type=\"single\""
        });
    }
    let data = data_style.map(|d| format!(" style:data-style-name=\"{d}\"")).unwrap_or_default();
    let mut xml = format!("<style:style style:name=\"{name}\" style:family=\"table-cell\"{data}>");
    if let Some(Color::Rgb(argb)) = dxf.fill {
        xml.push_str(&format!(
            "<style:table-cell-properties fo:background-color=\"#{:06X}\"/>",
            argb & 0x00FF_FFFF
        ));
    }
    if !text.is_empty() {
        xml.push_str(&format!("<style:text-properties{text}/>"));
    }
    xml.push_str("</style:style>");
    xml
}

/// The `calcext:conditional-formats` element for the formats of a sheet, with the style each rule applies.
pub fn calcext_xml(
    formats: &[ConditionalFormat],
    sheet: &str,
    style_of: &HashMap<u32, String>,
) -> String {
    let mut xml = String::new();
    for format in formats {
        let Some(first) = format.ranges.first() else { continue };
        let target: Vec<String> = format.ranges.iter().map(|r| range_address(sheet, *r)).collect();
        let mut inner = String::new();
        for rule in &format.rules {
            inner.push_str(&calcext_rule(rule, *first, sheet, style_of));
        }
        if !inner.is_empty() {
            xml.push_str(&format!(
                "<calcext:conditional-format calcext:target-range-address=\"{}\">{inner}</calcext:conditional-format>",
                target.join(" ")
            ));
        }
    }
    if xml.is_empty() {
        return xml;
    }
    format!("<calcext:conditional-formats>{xml}</calcext:conditional-formats>")
}

/// One rule as a `calcext` element.
fn calcext_rule(
    rule: &CfRule,
    range: Range,
    sheet: &str,
    style_of: &HashMap<u32, String>,
) -> String {
    let base = range_address(sheet, Range::cell(range.first));
    let style = rule
        .dxf
        .and_then(|d| style_of.get(&d))
        .map(|s| format!(" calcext:apply-style-name=\"{s}\""))
        .unwrap_or_default();
    let condition = |value: String| {
        format!(
            "<calcext:condition{style} calcext:value=\"{}\" calcext:base-cell-address=\"{base}\"/>",
            escape(&value)
        )
    };
    let arg = |i: usize| from_a1(rule.formulas.get(i).map_or("0", String::as_str), &[]);
    match rule.kind.as_str() {
        "cellIs" => match rule.operator.as_deref() {
            Some("between") => condition(format!("between({},{})", arg(0), arg(1))),
            Some("notBetween") => condition(format!("not-between({},{})", arg(0), arg(1))),
            Some(o) => condition(format!("{}{}", operator_symbol(o), arg(0))),
            None => String::new(),
        },
        "expression" => condition(format!("formula-is({})", arg(0))),
        "duplicateValues" => condition("duplicate".to_owned()),
        "uniqueValues" => condition("unique".to_owned()),
        "aboveAverage" => condition(
            if rule.above_average.unwrap_or(true) { "above-average" } else { "below-average" }
                .to_owned(),
        ),
        "top10" => {
            let name = format!(
                "{}-{}",
                if rule.bottom { "bottom" } else { "top" },
                if rule.percent { "percent" } else { "elements" }
            );
            condition(format!("{name}({})", rule.rank.unwrap_or(10)))
        }
        "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
            let name = match rule.kind.as_str() {
                "containsText" => "contains-text",
                "notContainsText" => "not-contains-text",
                "beginsWith" => "begins-with",
                _ => "ends-with",
            };
            condition(format!("{name}(\"{}\")", rule.text.clone().unwrap_or_default()))
        }
        "colorScale" => color_scale_xml(rule),
        "dataBar" => data_bar_xml(rule),
        _ => String::new(),
    }
}

/// A threshold as a `calcext` entry's attributes.
fn cfvo_attrs(v: &crate::model::Cfvo, first: bool) -> String {
    let kind = match v.kind.as_str() {
        "min" => {
            if first {
                "minimum"
            } else {
                "auto-minimum"
            }
        }
        "max" => "maximum",
        "percentile" => "percentile",
        "percent" => "percent",
        "formula" => "formula",
        _ => "number",
    };
    let value =
        v.value.as_ref().map(|v| format!(" calcext:value=\"{}\"", escape(v))).unwrap_or_default();
    format!("calcext:type=\"{kind}\"{value}")
}

fn color_scale_xml(rule: &CfRule) -> String {
    let Some(entries) = &rule.color_scale else { return String::new() };
    let items: String = entries
        .iter()
        .enumerate()
        .map(|(i, (v, c))| {
            let hex = match c {
                Color::Rgb(argb) => format!("#{:06X}", argb & 0x00FF_FFFF),
                _ => "#FFFFFF".to_owned(),
            };
            format!(
                "<calcext:color-scale-entry {} calcext:color=\"{hex}\"/>",
                cfvo_attrs(v, i == 0)
            )
        })
        .collect();
    format!("<calcext:color-scale>{items}</calcext:color-scale>")
}

fn data_bar_xml(rule: &CfRule) -> String {
    let Some((low, high, color)) = &rule.data_bar else { return String::new() };
    let hex = match color {
        Color::Rgb(argb) => format!("#{:06X}", argb & 0x00FF_FFFF),
        _ => "#638EC6".to_owned(),
    };
    format!(
        "<calcext:data-bar calcext:positive-color=\"{hex}\"><calcext:formatting-entry {}/><calcext:formatting-entry {}/></calcext:data-bar>",
        cfvo_attrs(low, true),
        cfvo_attrs(high, false)
    )
}

// ---------------------------------------------------------------------------------------------------
// Names.

/// The named ranges and expressions in a list, and the print ranges the names holds for a sheet.
pub fn names_xml(names: &[&DefinedName], book: &Workbook, base_sheet: &str) -> String {
    let mut xml = String::new();
    for name in names {
        let shown = name.name.strip_prefix("_xlnm.").unwrap_or(&name.name);
        if name.name.starts_with("_xlnm.") && !matches!(shown, "Print_Area" | "Print_Titles") {
            continue;
        }
        let base = format!("{base_sheet}.$A$1");
        match simple_range(&name.formula, book) {
            Some((sheet, range)) => xml.push_str(&format!(
                "<table:named-range table:name=\"{}\" table:cell-range-address=\"{}\" table:base-cell-address=\"{}\"/>",
                escape(shown),
                absolute_address(&sheet, range),
                absolute_address(&sheet, Range::cell(range.first)),
            )),
            None => xml.push_str(&format!(
                "<table:named-expression table:name=\"{}\" table:expression=\"of:={}\" table:base-cell-address=\"{}\"/>",
                escape(shown),
                escape(&from_a1(&name.formula, &[])),
                escape(&base),
            )),
        }
    }
    if xml.is_empty() {
        xml
    } else {
        format!("<table:named-expressions>{xml}</table:named-expressions>")
    }
}

/// A range address with absolute cells: `N.$A$1:N.$A$3`.
fn absolute_address(sheet: &str, range: Range) -> String {
    let abs = |c: CellRef| format!("${}${}", crate::coord::column_name(c.col), c.row + 1);
    let name = sheet_address(sheet);
    if range.first == range.last {
        format!("{name}.{}", abs(range.first))
    } else {
        format!("{name}.{}:{name}.{}", abs(range.first), abs(range.last))
    }
}

/// The sheet and cells of a formula that is only a reference to cells.
pub fn simple_range(formula: &str, book: &Workbook) -> Option<(String, Range)> {
    let (sheet, cells) = formula.rsplit_once('!')?;
    let sheet = sheet.trim_matches('\'').replace("''", "'");
    book.sheet_index(&sheet)?;
    let range = Range::parse(&cells.replace('$', ""))?;
    Some((sheet, range))
}

/// The print ranges a name holds, as `Sheet.A1:Sheet.B2` addresses.
pub fn print_ranges(formula: &str, book: &Workbook) -> Option<String> {
    let mut parts = Vec::new();
    for piece in formula.split(',') {
        let (sheet, range) = simple_range(piece, book)?;
        parts.push(range_address(&sheet, range));
    }
    Some(parts.join(" "))
}

// ---------------------------------------------------------------------------------------------------
// Rich text.

/// The spans of a rich string: each run's text and the text style properties it names.
pub fn rich_runs(raw: &str) -> Vec<(String, String)> {
    let Ok(doc) = roxmltree::Document::parse(raw.trim()) else { return Vec::new() };
    let mut runs = Vec::new();
    for node in doc.root_element().children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "t" => runs.push((node.text().unwrap_or("").to_owned(), String::new())),
            "r" => {
                let text = node
                    .children()
                    .find(|c| c.tag_name().name() == "t")
                    .and_then(|t| t.text())
                    .unwrap_or("");
                let props = node
                    .children()
                    .find(|c| c.tag_name().name() == "rPr")
                    .map(run_style)
                    .unwrap_or_default();
                runs.push((text.to_owned(), props));
            }
            _ => {}
        }
    }
    runs
}

/// A run's `rPr` as text properties.
fn run_style(rpr: roxmltree::Node) -> String {
    let mut out = String::new();
    for p in rpr.children().filter(|n| n.is_element()) {
        match p.tag_name().name() {
            "b" => out.push_str(" fo:font-weight=\"bold\""),
            "i" => out.push_str(" fo:font-style=\"italic\""),
            "strike" => out.push_str(" style:text-line-through-style=\"solid\""),
            "u" => out.push_str(
                " style:text-underline-style=\"solid\" style:text-underline-type=\"single\"",
            ),
            "sz" => {
                if let Some(v) = p.attribute("val").and_then(|v| v.parse::<f64>().ok()) {
                    out.push_str(&format!(" fo:font-size=\"{}pt\"", number(v)));
                }
            }
            "color" => {
                if let Some(rgb) = p.attribute("rgb").and_then(Color::parse_hex) {
                    if let Color::Rgb(argb) = rgb {
                        out.push_str(&format!(" fo:color=\"#{:06X}\"", argb & 0x00FF_FFFF));
                    }
                }
            }
            "rFont" => {
                if let Some(v) = p.attribute("val") {
                    out.push_str(&format!(" style:font-name=\"{}\"", escape(v)));
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// Settings.

/// `settings.xml`: the active sheet, and each sheet's frozen panes, zoom and gridlines.
pub fn settings_xml(book: &Workbook) -> String {
    let item = |name: &str, kind: &str, value: &str| {
        format!("<config:config-item config:name=\"{name}\" config:type=\"{kind}\">{}</config:config-item>", escape(value))
    };
    let mut tables = String::new();
    for sheet in &book.sheets {
        let (rows, cols) = sheet.frozen.unwrap_or((0, 0));
        let mut items = String::new();
        items.push_str(&item("HorizontalSplitMode", "short", if cols > 0 { "2" } else { "0" }));
        items.push_str(&item("VerticalSplitMode", "short", if rows > 0 { "2" } else { "0" }));
        items.push_str(&item("HorizontalSplitPosition", "int", &cols.to_string()));
        items.push_str(&item("VerticalSplitPosition", "int", &rows.to_string()));
        items.push_str(&item(
            "ActiveSplitRange",
            "short",
            if rows > 0 && cols > 0 { "3" } else { "2" },
        ));
        items.push_str(&item("PositionLeft", "int", "0"));
        items.push_str(&item("PositionRight", "int", &cols.to_string()));
        items.push_str(&item("PositionTop", "int", "0"));
        items.push_str(&item("PositionBottom", "int", &rows.to_string()));
        items.push_str(&item("ZoomType", "short", "0"));
        items.push_str(&item("ZoomValue", "int", &sheet.zoom.to_string()));
        items.push_str(&item(
            "ShowGrid",
            "boolean",
            if sheet.show_grid { "true" } else { "false" },
        ));
        tables.push_str(&format!(
            "<config:config-item-map-entry config:name=\"{}\">{items}</config:config-item-map-entry>",
            escape(&sheet.name)
        ));
    }
    let active = book.sheets.get(book.active_sheet).map(|s| s.name.as_str()).unwrap_or("");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-settings xmlns:office=\"{NS_OFFICE}\" xmlns:config=\"{NS_CONFIG}\" office:version=\"1.3\"><office:settings><config:config-item-set config:name=\"ooo:view-settings\"><config:config-item-map-indexed config:name=\"Views\"><config:config-item-map-entry>{}{}<config:config-item-map-named config:name=\"Tables\">{tables}</config:config-item-map-named></config:config-item-map-entry></config:config-item-map-indexed></config:config-item-set></office:settings></office:document-settings>",
        item("ViewId", "string", "view1"),
        item("ActiveTable", "string", active),
    )
}
