//! Reading what an `.ods` sheet carries besides its cells: validations, conditional formats, names,
//! filters and the window settings.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use roxmltree::{Document, Node};

use super::cellstyle::CellStyles;
use super::read::Collected;
use super::{attr, child, split_address, NS_CALCEXT, NS_CONFIG, NS_TABLE, NS_TEXT};
use crate::coord::{CellRef, Range};
use crate::model::{
    AutoFilter, CfRule, Cfvo, ConditionalFormat, DefinedName, Sheet, Validation, Workbook,
};
use crate::styles::{Color, Styles};

/// The workbook level definitions the cells refer to by name.
pub struct Features<'d> {
    pub date1904: bool,
    validations: HashMap<String, Node<'d, 'd>>,
}

impl<'d> Features<'d> {
    /// Gather the validations and the date origin from the spreadsheet element.
    pub fn new(body: Node<'d, 'd>) -> Self {
        let mut validations = HashMap::new();
        if let Some(list) = child(body, "content-validations") {
            for v in list
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "content-validation")
            {
                if let Some(name) = attr(v, NS_TABLE, "name") {
                    validations.insert(name.to_owned(), v);
                }
            }
        }
        let date1904 = child(body, "calculation-settings")
            .and_then(|c| child(c, "null-date"))
            .and_then(|n| attr(n, NS_TABLE, "date-value"))
            .is_some_and(|d| d.starts_with("1904"));
        Self { date1904, validations }
    }

    /// Attach the validations and conditional formats the cells of a sheet named.
    pub fn finish_sheet(
        &self,
        table: Node<'d, 'd>,
        sheet: &mut Sheet,
        collected: Collected,
        cells: &mut CellStyles<'d>,
        styles: &mut Styles,
    ) {
        let mut names: Vec<_> = collected.validated.into_iter().collect();
        names.sort();
        for (name, positions) in names {
            if let Some(node) = self.validations.get(&name) {
                sheet.validations.push(read_validation(*node, cells_to_ranges(&positions)));
            }
        }
        let mut priority = 1;
        if let Some(block) = child(table, "conditional-formats") {
            for format in block
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "conditional-format")
            {
                if let Some(read) = read_calcext_format(format, cells, styles, &mut priority) {
                    sheet.conditional.push(read);
                }
            }
            return;
        }
        let mut mapped: Vec<_> = collected.mapped.into_iter().collect();
        mapped.sort();
        for (style, positions) in mapped {
            let ranges = cells_to_ranges(&positions);
            let rules: Vec<CfRule> = cells
                .maps(&style)
                .into_iter()
                .filter_map(|(condition, apply, base)| {
                    let first = ranges.first()?.first;
                    let rule = rule_from_condition(&condition, &base, first, &mut priority)?;
                    Some(with_dxf(rule, &apply, cells, styles))
                })
                .collect();
            if !rules.is_empty() {
                sheet.conditional.push(ConditionalFormat { ranges, rules });
            }
        }
    }

    /// The defined names: the workbook's, each sheet's, and the print areas.
    pub fn read_names(&self, body: Node<'d, 'd>, sheets: &[String], book: &mut Workbook) {
        if let Some(list) = child(body, "named-expressions") {
            read_name_list(list, None, book);
        }
        for (index, table) in
            body.children().filter(|n| n.is_element() && n.tag_name().name() == "table").enumerate()
        {
            if let Some(list) = child(table, "named-expressions") {
                read_name_list(list, Some(index), book);
            }
            let has_area =
                book.names.iter().any(|n| n.name == "_xlnm.Print_Area" && n.scope == Some(index));
            if let (Some(ranges), false) = (attr(table, NS_TABLE, "print-ranges"), has_area) {
                let parts: Vec<String> = ranges
                    .split_whitespace()
                    .map(|r| absolute_reference(&super::formula::to_a1(&format!("[{r}]"))))
                    .collect();
                book.names.push(DefinedName {
                    name: "_xlnm.Print_Area".to_owned(),
                    scope: Some(index),
                    formula: parts.join(","),
                    hidden: false,
                    attrs: Vec::new(),
                });
            }
        }
        let _ = sheets;
    }

    /// The filter buttons of database ranges.
    pub fn read_database_ranges(&self, body: Node<'d, 'd>, book: &mut Workbook) {
        let Some(list) = child(body, "database-ranges") else { return };
        for range in
            list.children().filter(|n| n.is_element() && n.tag_name().name() == "database-range")
        {
            if attr(range, NS_TABLE, "display-filter-buttons") != Some("true") {
                continue;
            }
            let Some(address) = attr(range, NS_TABLE, "target-range-address") else { continue };
            let (sheet_name, _) = split_address(address);
            let a1 = super::formula::to_a1(&format!("[{address}]"));
            let cells = a1.rsplit_once('!').map_or(a1.as_str(), |(_, c)| c);
            if let (Some(index), Some(range)) = (book.sheet_index(&sheet_name), Range::parse(cells))
            {
                book.sheets[index].filter =
                    Some(AutoFilter { range, columns: BTreeMap::new(), sort: None });
            }
        }
    }
}

/// `Sheet!A1:B2` with its cells made absolute, as a defined name holds a print area.
fn absolute_reference(reference: &str) -> String {
    let Some((sheet, cells)) = reference.rsplit_once('!') else { return reference.to_owned() };
    let Some(range) = Range::parse(cells) else { return reference.to_owned() };
    let abs = |c: CellRef| format!("${}${}", crate::coord::column_name(c.col), c.row + 1);
    if range.first == range.last {
        format!("{sheet}!{}", abs(range.first))
    } else {
        format!("{sheet}!{}:{}", abs(range.first), abs(range.last))
    }
}

/// Read a list of named ranges and named expressions.
fn read_name_list(list: Node, scope: Option<usize>, book: &mut Workbook) {
    for n in list.children().filter(|n| n.is_element()) {
        let Some(name) = attr(n, NS_TABLE, "name") else { continue };
        let formula = match n.tag_name().name() {
            "named-range" => attr(n, NS_TABLE, "cell-range-address").map(|a| {
                a.split_whitespace()
                    .map(|r| super::formula::to_a1(&format!("[{r}]")))
                    .collect::<Vec<_>>()
                    .join(",")
            }),
            "named-expression" => attr(n, NS_TABLE, "expression").map(|e| {
                super::formula::to_a1(
                    e.strip_prefix("of:=").or_else(|| e.strip_prefix('=')).unwrap_or(e),
                )
            }),
            _ => None,
        };
        let Some(formula) = formula else { continue };
        let name = if scope.is_some() && matches!(name, "Print_Area" | "Print_Titles") {
            format!("_xlnm.{name}")
        } else {
            name.to_owned()
        };
        if book.names.iter().any(|d| d.name == name && d.scope == scope) {
            continue;
        }
        book.names.push(DefinedName { name, scope, formula, hidden: false, attrs: Vec::new() });
    }
}

/// The rectangles that cover exactly these cells.
pub fn cells_to_ranges(positions: &[(u32, u32)]) -> Vec<Range> {
    let set: BTreeSet<(u32, u32)> = positions.iter().copied().collect();
    let mut runs: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for (row, col) in set {
        let row_runs = runs.entry(row).or_default();
        match row_runs.last_mut() {
            Some((_, last)) if *last + 1 == col => *last = col,
            _ => row_runs.push((col, col)),
        }
    }
    let mut open: Vec<(u32, u32, u32, u32)> = Vec::new();
    let mut done: Vec<Range> = Vec::new();
    for (row, row_runs) in runs {
        let mut next: Vec<(u32, u32, u32, u32)> = Vec::new();
        for (first, last) in row_runs {
            match open
                .iter()
                .position(|(a, b, _, bottom)| *a == first && *b == last && *bottom + 1 == row)
            {
                Some(i) => {
                    let (a, b, top, _) = open.remove(i);
                    next.push((a, b, top, row));
                }
                None => next.push((first, last, row, row)),
            }
        }
        done.extend(open.drain(..).map(|(a, b, top, bottom)| rect(top, a, bottom, b)));
        open = next;
    }
    done.extend(open.into_iter().map(|(a, b, top, bottom)| rect(top, a, bottom, b)));
    done.sort_by_key(|r| (r.first.row, r.first.col));
    done
}

fn rect(top: u32, left: u32, bottom: u32, right: u32) -> Range {
    Range::new(CellRef::new(top, left), CellRef::new(bottom, right))
}

// ---------------------------------------------------------------------------------------------------
// Validations.

/// A data validation from its definition.
fn read_validation(node: Node, ranges: Vec<Range>) -> Validation {
    let condition = attr(node, NS_TABLE, "condition").unwrap_or("");
    let condition = condition.strip_prefix("of:").unwrap_or(condition);
    let mut validation = Validation {
        ranges,
        kind: "custom".to_owned(),
        operator: None,
        formula1: None,
        formula2: None,
        allow_blank: attr(node, NS_TABLE, "allow-empty-cell") != Some("false"),
        show_dropdown: attr(node, NS_TABLE, "display-list") != Some("none"),
        prompt_title: None,
        prompt: None,
        error_title: None,
        error: None,
        error_style: None,
        attrs: Vec::new(),
    };
    let base = attr(node, NS_TABLE, "base-cell-address").unwrap_or("");
    let _ = base;
    for part in split_and(condition) {
        apply_condition_part(&part, &mut validation);
    }
    if let Some(help) =
        child(node, "help-message").filter(|h| attr(*h, NS_TABLE, "display") != Some("false"))
    {
        validation.prompt_title = attr(help, NS_TABLE, "title").map(str::to_owned);
        validation.prompt = message_text(help);
    }
    if let Some(error) =
        child(node, "error-message").filter(|h| attr(*h, NS_TABLE, "display") != Some("false"))
    {
        validation.error_title = attr(error, NS_TABLE, "title").map(str::to_owned);
        validation.error = message_text(error);
        validation.error_style = match attr(error, NS_TABLE, "message-type") {
            Some("warning") => Some("warning".to_owned()),
            Some("information") => Some("information".to_owned()),
            _ => None,
        };
    }
    validation
}

/// The text of a help or error message.
fn message_text(node: Node) -> Option<String> {
    let text: Vec<String> = node
        .children()
        .filter(|n| {
            n.is_element()
                && n.tag_name().name() == "p"
                && n.tag_name().namespace() == Some(NS_TEXT)
        })
        .map(super::read::paragraph_text)
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

/// Split a condition at the `and`s outside quotes and brackets.
fn split_and(condition: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut depth) = (false, 0i32);
    let chars: Vec<char> = condition.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            _ => {}
        }
        if !quoted
            && depth == 0
            && c == ' '
            && chars[i..].iter().take(5).collect::<String>() == " and "
        {
            parts.push(std::mem::take(&mut current).trim().to_owned());
            i += 5;
            continue;
        }
        current.push(c);
        i += 1;
    }
    parts.push(current.trim().to_owned());
    parts
}

/// The arguments of a call's parentheses, split at the separators outside quotes and brackets.
fn call_args(text: &str) -> Vec<String> {
    let (Some(open), Some(close)) = (text.find('('), text.rfind(')')) else { return Vec::new() };
    let inner = &text[open + 1..close];
    let mut args = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut depth) = (false, 0i32);
    for c in inner.chars() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' | '{' if !quoted => depth += 1,
            ')' | ']' | '}' if !quoted => depth -= 1,
            _ => {}
        }
        if !quoted && depth == 0 && (c == ';' || c == ',') {
            args.push(std::mem::take(&mut current).trim().to_owned());
        } else {
            current.push(c);
        }
    }
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current.trim().to_owned());
    }
    args
}

/// One part of a validation condition: a kind, a comparison or a list.
fn apply_condition_part(part: &str, v: &mut Validation) {
    let lower = part.to_ascii_lowercase();
    let name = lower.split('(').next().unwrap_or("");
    match name {
        "cell-content-is-whole-number" => v.kind = "whole".to_owned(),
        "cell-content-is-decimal-number" => v.kind = "decimal".to_owned(),
        "cell-content-is-date" => v.kind = "date".to_owned(),
        "cell-content-is-time" => v.kind = "time".to_owned(),
        "cell-content-is-between" | "cell-content-is-not-between" => {
            if v.kind == "custom" {
                v.kind = "decimal".to_owned();
            }
            let args = call_args(part);
            v.operator = Some(
                if name.ends_with("not-between") { "notBetween" } else { "between" }.to_owned(),
            );
            v.formula1 = args.first().map(|a| value_text(a));
            v.formula2 = args.get(1).map(|a| value_text(a));
        }
        "cell-content-text-length-is-between" | "cell-content-text-length-is-not-between" => {
            v.kind = "textLength".to_owned();
            let args = call_args(part);
            v.operator = Some(
                if name.ends_with("not-between") { "notBetween" } else { "between" }.to_owned(),
            );
            v.formula1 = args.first().map(|a| value_text(a));
            v.formula2 = args.get(1).map(|a| value_text(a));
        }
        "cell-content-is-in-list" => {
            v.kind = "list".to_owned();
            v.formula1 = Some(list_text(&call_args(part)));
        }
        "is-true-formula" => {
            v.kind = "custom".to_owned();
            let args =
                part.find('(').map(|o| &part[o + 1..part.len().saturating_sub(1)]).unwrap_or("");
            v.formula1 = Some(super::formula::to_a1(args));
        }
        "cell-content-text-length" | "cell-content" => comparison(part, name, v),
        _ => {}
    }
}

/// `cell-content()>0.5` or `cell-content-text-length()<=5`.
fn comparison(part: &str, name: &str, v: &mut Validation) {
    let Some(rest) = part.split_once("()").map(|(_, r)| r.trim()) else { return };
    if name == "cell-content-text-length" {
        v.kind = "textLength".to_owned();
    } else if v.kind == "custom" {
        v.kind = "decimal".to_owned();
    }
    let operator: String = rest.chars().take_while(|c| "<>=!".contains(*c)).collect();
    let value = value_text(rest[operator.len()..].trim());
    v.operator = Some(
        match operator.as_str() {
            "=" => "equal",
            "!=" | "<>" => "notEqual",
            "<" => "lessThan",
            ">" => "greaterThan",
            "<=" => "lessThanOrEqual",
            ">=" => "greaterThanOrEqual",
            _ => "equal",
        }
        .to_owned(),
    );
    v.formula1 = Some(value);
}

/// A value or formula in a condition as Excel writes it.
fn value_text(text: &str) -> String {
    super::formula::to_a1(text)
}

/// A list's items as the comma separated quoted text Excel stores, or a range.
fn list_text(args: &[String]) -> String {
    if args.len() == 1 && args[0].starts_with('[') {
        return super::formula::to_a1(&args[0]);
    }
    let items: Vec<String> =
        args.iter().map(|a| a.trim_matches('"').replace("\"\"", "\"")).collect();
    format!("\"{}\"", items.join(","))
}

// ---------------------------------------------------------------------------------------------------
// Conditional formats.

/// Add the differential format a rule's style names.
fn with_dxf(mut rule: CfRule, style: &str, cells: &CellStyles, styles: &mut Styles) -> CfRule {
    let dxf = cells.dxf(style);
    let index = match styles.dxfs.iter().position(|d| *d == dxf) {
        Some(i) => i,
        None => {
            styles.dxfs.push(dxf);
            styles.dxfs.len() - 1
        }
    };
    rule.dxf = Some(index as u32);
    rule
}

fn blank_rule(kind: &str, priority: &mut i32) -> CfRule {
    let rule = CfRule {
        kind: kind.to_owned(),
        dxf: None,
        priority: *priority,
        stop: false,
        operator: None,
        text: None,
        rank: None,
        percent: false,
        bottom: false,
        above_average: None,
        formulas: Vec::new(),
        color_scale: None,
        data_bar: None,
        attrs: Vec::new(),
        raw_children: String::new(),
        std_dev: None,
        equal_average: false,
        time_period: None,
        icon_set: None,
        data_bar_options: None,
        x14: None,
    };
    *priority += 1;
    rule
}

/// Formula text moved from the cell it is relative to onto the first cell of the format's range.
fn relative_formula(open_formula: &str, base: &str, first: CellRef) -> String {
    let a1 = super::formula::to_a1(open_formula);
    let (_, base_cell) = split_address(base);
    let Some(base_cell) = CellRef::parse(&base_cell) else { return a1 };
    let (rows, cols) =
        (first.row as i64 - base_cell.row as i64, first.col as i64 - base_cell.col as i64);
    if rows == 0 && cols == 0 {
        return a1;
    }
    match crate::formula::Formula::parse_kept(&a1) {
        Ok(f) => crate::formula::print::print(&crate::formula::shifted(&f.expr, rows, cols), true),
        Err(_) => a1,
    }
}

/// A rule from a style map's condition: `is-true-formula(...)`, `cell-content()>5`, `cell-content-is-between(1,5)`.
fn rule_from_condition(
    condition: &str,
    base: &str,
    first: CellRef,
    priority: &mut i32,
) -> Option<CfRule> {
    let condition = condition.strip_prefix("of:").unwrap_or(condition).trim();
    let name = condition.split('(').next().unwrap_or("");
    match name {
        "is-true-formula" => {
            let inner = &condition[condition.find('(')? + 1..condition.rfind(')')?];
            let mut rule = blank_rule("expression", priority);
            rule.formulas = vec![relative_formula(inner, base, first)];
            Some(rule)
        }
        "cell-content" => {
            let rest = condition.split_once("()")?.1.trim();
            let operator: String = rest.chars().take_while(|c| "<>=!".contains(*c)).collect();
            let mut rule = blank_rule("cellIs", priority);
            rule.operator = Some(operator_name(&operator).to_owned());
            rule.formulas = vec![relative_formula(rest[operator.len()..].trim(), base, first)];
            Some(rule)
        }
        "cell-content-is-between" | "cell-content-is-not-between" => {
            let mut rule = blank_rule("cellIs", priority);
            rule.operator = Some(
                if name.ends_with("not-between") { "notBetween" } else { "between" }.to_owned(),
            );
            rule.formulas =
                call_args(condition).iter().map(|a| relative_formula(a, base, first)).collect();
            Some(rule)
        }
        "is-error" => {
            let mut rule = blank_rule("containsErrors", priority);
            rule.formulas = vec![format!("ISERROR({})", first.a1())];
            Some(rule)
        }
        _ => None,
    }
}

/// The name Excel gives a comparison operator.
fn operator_name(op: &str) -> &'static str {
    match op {
        "=" | "==" => "equal",
        "!=" | "<>" => "notEqual",
        "<" => "lessThan",
        ">" => "greaterThan",
        "<=" => "lessThanOrEqual",
        ">=" => "greaterThanOrEqual",
        _ => "equal",
    }
}

/// A conditional format from `calcext:conditional-format`.
fn read_calcext_format(
    node: Node,
    cells: &CellStyles,
    styles: &mut Styles,
    priority: &mut i32,
) -> Option<ConditionalFormat> {
    let target = attr(node, NS_CALCEXT, "target-range-address")?;
    let ranges: Vec<Range> = target
        .split_whitespace()
        .filter_map(|r| {
            let a1 = super::formula::to_a1(&format!("[{r}]"));
            Range::parse(a1.rsplit_once('!').map_or(a1.as_str(), |(_, c)| c))
        })
        .collect();
    let first = ranges.first()?.first;
    let mut rules = Vec::new();
    for entry in node.children().filter(|n| n.is_element()) {
        let rule = match entry.tag_name().name() {
            "condition" => {
                let value = attr(entry, NS_CALCEXT, "value").unwrap_or("");
                let base = attr(entry, NS_CALCEXT, "base-cell-address").unwrap_or("");
                let rule = calcext_condition(value, base, first, priority);
                let style = attr(entry, NS_CALCEXT, "apply-style-name");
                rule.map(|r| match style {
                    Some(s) => with_dxf(r, s, cells, styles),
                    None => r,
                })
            }
            "color-scale" => Some(read_color_scale(entry, priority)),
            "data-bar" => Some(read_data_bar(entry, priority)),
            _ => None,
        };
        rules.extend(rule);
    }
    (!rules.is_empty()).then_some(ConditionalFormat { ranges, rules })
}

/// A rule from the value of a `calcext:condition`: `>5`, `between(1,5)`, `formula-is(A1>2)`, `duplicate`.
fn calcext_condition(
    value: &str,
    base: &str,
    first: CellRef,
    priority: &mut i32,
) -> Option<CfRule> {
    let value = value.trim();
    let name = value.split('(').next().unwrap_or("");
    let first_text = first.a1();
    match name {
        "between" | "not-between" => {
            let mut rule = blank_rule("cellIs", priority);
            rule.operator =
                Some(if name == "not-between" { "notBetween" } else { "between" }.to_owned());
            rule.formulas =
                call_args(value).iter().map(|a| relative_formula(a, base, first)).collect();
            Some(rule)
        }
        "formula-is" => {
            let inner = &value[value.find('(')? + 1..value.rfind(')')?];
            let mut rule = blank_rule("expression", priority);
            rule.formulas = vec![relative_formula(inner, base, first)];
            Some(rule)
        }
        "duplicate" => Some(blank_rule("duplicateValues", priority)),
        "unique" => Some(blank_rule("uniqueValues", priority)),
        "above-average" | "below-average" | "above-equal-average" | "below-equal-average" => {
            let mut rule = blank_rule("aboveAverage", priority);
            rule.above_average = Some(name.starts_with("above"));
            if name.ends_with("equal-average") {
                rule.attrs.push(("equalAverage".to_owned(), "1".to_owned()));
            }
            Some(rule)
        }
        "top-elements" | "bottom-elements" | "top-percent" | "bottom-percent" => {
            let mut rule = blank_rule("top10", priority);
            rule.rank = call_args(value).first().and_then(|a| a.parse().ok());
            rule.bottom = name.starts_with("bottom");
            rule.percent = name.ends_with("percent");
            Some(rule)
        }
        "begins-with" | "ends-with" | "contains-text" | "not-contains-text" => {
            let text = call_args(value)
                .first()
                .map(|a| a.trim_matches('"').to_owned())
                .unwrap_or_default();
            let (kind, operator, formula) = match name {
                "begins-with" => (
                    "beginsWith",
                    "beginsWith",
                    format!("LEFT({first_text},LEN(\"{text}\"))=\"{text}\""),
                ),
                "ends-with" => (
                    "endsWith",
                    "endsWith",
                    format!("RIGHT({first_text},LEN(\"{text}\"))=\"{text}\""),
                ),
                "contains-text" => (
                    "containsText",
                    "containsText",
                    format!("NOT(ISERROR(SEARCH(\"{text}\",{first_text})))"),
                ),
                _ => (
                    "notContainsText",
                    "notContains",
                    format!("ISERROR(SEARCH(\"{text}\",{first_text}))"),
                ),
            };
            let mut rule = blank_rule(kind, priority);
            rule.operator = Some(operator.to_owned());
            rule.text = Some(text);
            rule.formulas = vec![formula];
            Some(rule)
        }
        "is-error" => {
            let mut rule = blank_rule("containsErrors", priority);
            rule.formulas = vec![format!("ISERROR({first_text})")];
            Some(rule)
        }
        "is-no-error" => {
            let mut rule = blank_rule("notContainsErrors", priority);
            rule.formulas = vec![format!("NOT(ISERROR({first_text}))")];
            Some(rule)
        }
        _ => {
            let operator: String = value.chars().take_while(|c| "<>=!".contains(*c)).collect();
            if operator.is_empty() {
                return None;
            }
            let mut rule = blank_rule("cellIs", priority);
            rule.operator = Some(operator_name(&operator).to_owned());
            rule.formulas = vec![relative_formula(value[operator.len()..].trim(), base, first)];
            Some(rule)
        }
    }
}

/// A threshold of a colour scale or a data bar.
fn read_cfvo(entry: Node) -> Cfvo {
    let kind = match attr(entry, NS_CALCEXT, "type").unwrap_or("minimum") {
        "minimum" | "auto-minimum" => "min",
        "maximum" | "auto-maximum" => "max",
        "percentile" => "percentile",
        "percent" => "percent",
        "formula" => "formula",
        _ => "num",
    };
    Cfvo { kind: kind.to_owned(), value: attr(entry, NS_CALCEXT, "value").map(str::to_owned) }
}

fn read_color_scale(node: Node, priority: &mut i32) -> CfRule {
    let mut rule = blank_rule("colorScale", priority);
    let entries: Vec<(Cfvo, Color)> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "color-scale-entry")
        .map(|e| {
            let color =
                attr(e, NS_CALCEXT, "color").and_then(Color::parse_hex).unwrap_or(Color::Auto);
            (read_cfvo(e), color)
        })
        .collect();
    rule.color_scale = Some(entries);
    rule
}

fn read_data_bar(node: Node, priority: &mut i32) -> CfRule {
    let mut rule = blank_rule("dataBar", priority);
    let points: Vec<Cfvo> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "formatting-entry")
        .map(read_cfvo)
        .collect();
    let color = attr(node, NS_CALCEXT, "positive-color")
        .and_then(Color::parse_hex)
        .unwrap_or_else(|| Color::rgb(0x63, 0x8E, 0xC6));
    if points.len() >= 2 {
        rule.data_bar = Some((points[0].clone(), points[1].clone(), color));
    }
    rule
}

// ---------------------------------------------------------------------------------------------------
// Settings.

/// The direct `config:config-item` children of an element, by name.
fn items(node: Node) -> HashMap<String, String> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "config-item")
        .filter_map(|n| {
            Some((attr(n, NS_CONFIG, "name")?.to_owned(), n.text().unwrap_or("").to_owned()))
        })
        .collect()
}

/// The active sheet, and each sheet's frozen panes, zoom and gridlines, from `settings.xml`.
pub fn read_settings(doc: &Document, book: &mut Workbook) {
    for view in doc.descendants().filter(|n| {
        n.is_element()
            && n.tag_name().name() == "config-item-map-entry"
            && child(*n, "config-item-map-named")
                .is_some_and(|m| attr(m, NS_CONFIG, "name") == Some("Tables"))
    }) {
        let top = items(view);
        if let Some(active) = top.get("ActiveTable").and_then(|n| book.sheet_index(n)) {
            book.active_sheet = active;
        }
        let Some(tables) = view
            .children()
            .find(|n| n.is_element() && attr(*n, NS_CONFIG, "name") == Some("Tables"))
        else {
            continue;
        };
        for entry in tables.children().filter(|n| n.is_element()) {
            let Some(index) = attr(entry, NS_CONFIG, "name").and_then(|n| book.sheet_index(n))
            else {
                continue;
            };
            let table = items(entry);
            let number =
                |key: &str| table.get(key).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
            let cols = if number("HorizontalSplitMode") == 2 {
                number("HorizontalSplitPosition")
            } else {
                0
            };
            let rows =
                if number("VerticalSplitMode") == 2 { number("VerticalSplitPosition") } else { 0 };
            let sheet = &mut book.sheets[index];
            sheet.frozen = (rows > 0 || cols > 0).then_some((rows, cols));
            if let Some(zoom) = table.get("ZoomValue").and_then(|z| z.trim().parse::<u32>().ok()) {
                sheet.zoom = zoom;
            }
            if let Some(grid) = table.get("ShowGrid") {
                sheet.show_grid = grid.trim() != "false";
            }
        }
        break;
    }
}
