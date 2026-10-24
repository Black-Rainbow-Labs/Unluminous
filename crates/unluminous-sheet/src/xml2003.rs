//! XML Spreadsheet 2003, the single file XML format Excel 2002 and 2003 saved and every Excel since opens
//! (`urn:schemas-microsoft-com:office:spreadsheet`). Reading only.
//!
//! A cell names a style by id and holds a typed `Data` element. Formulas are written in R1C1 syntax, with
//! rows and columns relative to the cell (`R[-1]C`), so reading translates them to A1 text first.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::coord::{column_name, CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::Formula;
use crate::model::{
    ColumnInfo, DefinedName, Hyperlink, LinkTarget, Note, RowInfo, Sheet, Validation, Visibility,
    Workbook,
};
use crate::styles::{Alignment, Border, Color, Edge, Fill, Font, Underline, Xf};
use crate::value::{ErrorKind, Value};

/// Read an XML Spreadsheet 2003 file.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    let text = String::from_utf8_lossy(bytes);
    let doc = Document::parse(text.trim_start_matches('\u{FEFF}'))
        .map_err(|e| format!("the file is not valid XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "Workbook" {
        return Err("the file is XML but not an XML Spreadsheet 2003 workbook".to_owned());
    }
    let mut book = Workbook::new();
    book.sheets.clear();
    let styles = read_styles(root, &mut book);
    for sheet in root.children().filter(|n| is(*n, "Worksheet")) {
        let index = book.sheets.len();
        let mut s = read_sheet(sheet, &styles, &mut book, index);
        if let Some(name) = attribute(sheet, "Name") {
            s.name = name.chars().take(31).collect();
        }
        book.sheets.push(s);
    }
    if book.sheets.is_empty() {
        book.sheets.push(Sheet::new("Sheet1"));
    }
    if let Some(names) = root.children().find(|n| is(*n, "Names")) {
        read_names(names, &mut book, None);
    }
    Ok(book)
}

/// Whether a node is an element with a local name.
fn is(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

/// An attribute by its local name, whatever namespace prefix the file used.
fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    if let Some(i) = list.iter().position(|x| *x == item) {
        return i as u32;
    }
    list.push(item);
    (list.len() - 1) as u32
}

/// The cell format index for each style id.
fn read_styles(root: Node, book: &mut Workbook) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    let Some(styles) = root.children().find(|n| is(*n, "Styles")) else { return out };
    for style in styles.children().filter(|n| is(*n, "Style")) {
        let Some(id) = attribute(style, "ID") else { continue };
        if id == "Default" {
            if let Some(font) = style.children().find(|n| is(*n, "Font")) {
                let base = book.styles.fonts.first().cloned().unwrap_or_default();
                let font = font_from(font, base);
                if let Some(first) = book.styles.fonts.first_mut() {
                    *first = font;
                }
            }
            out.insert(id.to_owned(), 0);
            continue;
        }
        let xf = build_xf(style, book);
        out.insert(id.to_owned(), intern(&mut book.styles.xfs, xf));
    }
    out
}

fn build_xf(style: Node, book: &mut Workbook) -> Xf {
    let default_font = book.styles.fonts.first().cloned().unwrap_or_default();
    let mut xf = Xf::default();
    for part in style.children().filter(|n| n.is_element()) {
        match part.tag_name().name() {
            "Font" => {
                xf.font = intern(&mut book.styles.fonts, font_from(part, default_font.clone()));
            }
            "Interior" => xf.fill = intern(&mut book.styles.fills, fill_from(part)),
            "NumberFormat" => {
                if let Some(code) = attribute(part, "Format") {
                    xf.num_fmt = book.styles.format_id(&format_code(code));
                }
            }
            "Alignment" => xf.align = alignment_from(part),
            "Borders" => xf.border = intern(&mut book.styles.borders, border_from(part)),
            "Protection" => {
                if attribute(part, "Protected") == Some("0") {
                    xf.locked = Some(false);
                }
                if attribute(part, "HideFormula") == Some("1") {
                    xf.hidden = Some(true);
                }
            }
            _ => {}
        }
    }
    xf
}

/// The format code for a `NumberFormat`'s `Format`: a code, or one of the names Excel used.
fn format_code(name: &str) -> String {
    match name {
        "General Number" => "General",
        "General Date" => "m/d/yyyy h:mm",
        "Long Date" => "dddd, mmmm d, yyyy",
        "Medium Date" => "d-mmm-yy",
        "Short Date" => "m/d/yyyy",
        "Long Time" => "h:mm:ss AM/PM",
        "Medium Time" => "h:mm AM/PM",
        "Short Time" => "h:mm",
        "Currency" => "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)",
        "Euro Currency" => "[$\u{20AC}-2] #,##0.00",
        "Fixed" => "0.00",
        "Standard" => "#,##0.00",
        "Percent" => "0.00%",
        "Scientific" => "0.00E+00",
        "Yes/No" => "\"Yes\";\"Yes\";\"No\"",
        "True/False" => "\"True\";\"True\";\"False\"",
        "On/Off" => "\"On\";\"On\";\"Off\"",
        other => other,
    }
    .to_owned()
}

fn font_from(node: Node, mut font: Font) -> Font {
    if let Some(name) = attribute(node, "FontName") {
        font.name = name.to_owned();
        font.scheme = None;
    }
    if let Some(size) = attribute(node, "Size").and_then(|s| s.parse().ok()) {
        font.size = size;
    }
    font.bold = attribute(node, "Bold") == Some("1");
    font.italic = attribute(node, "Italic") == Some("1");
    font.strike = attribute(node, "StrikeThrough") == Some("1");
    font.underline = match attribute(node, "Underline") {
        Some("Single") => Underline::Single,
        Some("Double") => Underline::Double,
        Some("SingleAccounting") => Underline::SingleAccounting,
        Some("DoubleAccounting") => Underline::DoubleAccounting,
        _ => Underline::None,
    };
    font.vert_align = match attribute(node, "VerticalAlign") {
        Some("Superscript") => Some("superscript".to_owned()),
        Some("Subscript") => Some("subscript".to_owned()),
        _ => None,
    };
    if let Some(color) = attribute(node, "Color").and_then(Color::parse_hex) {
        font.color = color;
    }
    font
}

fn fill_from(node: Node) -> Fill {
    match (attribute(node, "Pattern"), attribute(node, "Color").and_then(Color::parse_hex)) {
        (Some("None"), _) | (None, None) => Fill::none(),
        (_, Some(color)) => Fill::solid(color),
        _ => Fill::none(),
    }
}

fn alignment_from(node: Node) -> Alignment {
    let horizontal = match attribute(node, "Horizontal") {
        Some("Left") => Some("left"),
        Some("Right") => Some("right"),
        Some("Center") => Some("center"),
        Some("Justify") => Some("justify"),
        Some("Fill") => Some("fill"),
        Some("CenterAcrossSelection") => Some("centerContinuous"),
        Some("Distributed") => Some("distributed"),
        _ => None,
    };
    let vertical = match attribute(node, "Vertical") {
        Some("Top") => Some("top"),
        Some("Center") => Some("center"),
        Some("Justify") => Some("justify"),
        Some("Distributed") => Some("distributed"),
        _ => None,
    };
    let rotate: i32 = attribute(node, "Rotate").and_then(|r| r.parse().ok()).unwrap_or(0);
    Alignment {
        horizontal: horizontal.map(str::to_owned),
        vertical: vertical.map(str::to_owned),
        wrap: attribute(node, "WrapText") == Some("1"),
        indent: attribute(node, "Indent").and_then(|i| i.parse().ok()).unwrap_or(0),
        rotation: if rotate < 0 { 90 - rotate } else { rotate },
        shrink: attribute(node, "ShrinkToFit") == Some("1"),
    }
}

fn border_from(node: Node) -> Border {
    let mut border = Border::default();
    for side in node.children().filter(|n| is(*n, "Border")) {
        let style = match (attribute(side, "LineStyle"), attribute(side, "Weight")) {
            (None, _) | (Some("None"), _) => continue,
            (Some("Continuous"), Some("0")) => "hair",
            (Some("Continuous"), Some("2")) => "medium",
            (Some("Continuous"), Some("3")) => "thick",
            (Some("Continuous"), _) => "thin",
            (Some("Dash"), Some("2")) => "mediumDashed",
            (Some("Dash"), _) => "dashed",
            (Some("Dot"), _) => "dotted",
            (Some("DashDot"), Some("2")) => "mediumDashDot",
            (Some("DashDot"), _) => "dashDot",
            (Some("DashDotDot"), Some("2")) => "mediumDashDotDot",
            (Some("DashDotDot"), _) => "dashDotDot",
            (Some("SlantDashDot"), _) => "slantDashDot",
            (Some("Double"), _) => "double",
            _ => "thin",
        };
        let color =
            attribute(side, "Color").and_then(Color::parse_hex).unwrap_or(Color::Indexed(64));
        let edge = Edge::line(style, color);
        match attribute(side, "Position") {
            Some("Left") => border.left = edge,
            Some("Right") => border.right = edge,
            Some("Top") => border.top = edge,
            Some("Bottom") => border.bottom = edge,
            Some("DiagonalLeft") => {
                border.diagonal = edge;
                border.diagonal_down = true;
            }
            Some("DiagonalRight") => {
                border.diagonal = edge;
                border.diagonal_up = true;
            }
            _ => {}
        }
    }
    border
}

fn read_sheet(
    node: Node,
    styles: &HashMap<String, u32>,
    book: &mut Workbook,
    index: usize,
) -> Sheet {
    let mut sheet = Sheet::new("Sheet1");
    if let Some(table) = node.children().find(|n| is(*n, "Table")) {
        read_table(table, styles, book, &mut sheet);
    }
    if let Some(options) = node.children().find(|n| is(*n, "WorksheetOptions")) {
        read_options(options, &mut sheet, book, index);
    }
    for names in node.children().filter(|n| is(*n, "Names")) {
        read_names(names, book, Some(index));
    }
    for validation in node.children().filter(|n| is(*n, "DataValidation")) {
        if let Some(v) = read_validation(validation) {
            sheet.validations.push(v);
        }
    }
    sheet
}

fn read_table(table: Node, styles: &HashMap<String, u32>, book: &mut Workbook, sheet: &mut Sheet) {
    let default_width = attribute(table, "DefaultColumnWidth").and_then(|w| w.parse::<f64>().ok());
    let pixels = crate::ods::pixels_per_character(book);
    sheet.default_col_width = default_width.map(|w| characters_from_points(w, pixels));
    if let Some(height) = attribute(table, "DefaultRowHeight").and_then(|h| h.parse::<f64>().ok()) {
        sheet.default_row_height = height;
    }
    let mut col = 0u32;
    for column in table.children().filter(|n| is(*n, "Column")) {
        if let Some(index) = attribute(column, "Index").and_then(|i| i.parse::<u32>().ok()) {
            col = index.saturating_sub(1);
        }
        let span = attribute(column, "Span").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let width = attribute(column, "Width")
            .and_then(|w| w.parse::<f64>().ok())
            .map(|w| characters_from_points(w, pixels));
        let hidden = attribute(column, "Hidden") == Some("1");
        let style = attribute(column, "StyleID").and_then(|s| styles.get(s)).copied().unwrap_or(0);
        if width.is_some() || hidden || style != 0 {
            sheet.columns.push(ColumnInfo {
                first: col,
                last: (col + span).min(MAX_COLS - 1),
                width,
                hidden,
                style,
                custom_width: width.is_some(),
                outline: 0,
                collapsed: false,
                best_fit: false,
            });
        }
        col += span + 1;
    }
    let mut row = 0u32;
    for r in table.children().filter(|n| is(*n, "Row")) {
        if let Some(index) = attribute(r, "Index").and_then(|i| i.parse::<u32>().ok()) {
            row = index.saturating_sub(1);
        }
        let span = attribute(r, "Span").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let height = attribute(r, "Height").and_then(|h| h.parse::<f64>().ok());
        let hidden = attribute(r, "Hidden") == Some("1");
        for k in 0..=span {
            if height.is_some() || hidden {
                sheet.rows.insert(row + k, RowInfo { height, hidden, ..Default::default() });
            }
            if k == 0 {
                read_cells(r, row, styles, book, sheet);
            }
        }
        row += span + 1;
        if row >= MAX_ROWS {
            break;
        }
    }
}

/// A width in points as characters of the default font.
fn characters_from_points(points: f64, pixels_per_character: f64) -> f64 {
    let pixels = points * 96.0 / 72.0;
    ((pixels / pixels_per_character) * 100.0).round() / 100.0
}

fn read_cells(
    row: Node,
    r: u32,
    styles: &HashMap<String, u32>,
    book: &mut Workbook,
    sheet: &mut Sheet,
) {
    let mut col = 0u32;
    for cell in row.children().filter(|n| is(*n, "Cell")) {
        if let Some(index) = attribute(cell, "Index").and_then(|i| i.parse::<u32>().ok()) {
            col = index.saturating_sub(1);
        }
        let at = CellRef::new(r, col);
        let style = attribute(cell, "StyleID").and_then(|s| styles.get(s)).copied().unwrap_or(0);
        let value = cell
            .children()
            .find(|n| is(*n, "Data"))
            .map_or(Value::Empty, |d| data_value(d, book.date1904));
        let formula = attribute(cell, "Formula").map(|f| formula_of(f, at));
        let mut made = crate::model::Cell { value, formula, style, rich: None, extra: None };
        if let Some(range) = attribute(cell, "ArrayRange").and_then(|a| array_range(a, at)) {
            if let Some(f) = made.formula.as_mut() {
                f.array = Some(range);
                f.dynamic = false;
            }
            sheet.array_formulas.push(range);
        }
        let across =
            attribute(cell, "MergeAcross").and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
        let down = attribute(cell, "MergeDown").and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
        if across > 0 || down > 0 {
            sheet.merges.push(Range::new(at, CellRef::new(r + down, col + across)));
        }
        if let Some(href) = attribute(cell, "HRef") {
            let target = match href.strip_prefix('#') {
                Some(place) => LinkTarget::Location(place.to_owned()),
                None => LinkTarget::External(href.to_owned()),
            };
            let display = match &made.value {
                Value::Text(t) => Some(t.to_string()),
                _ => None,
            };
            sheet.links.push(Hyperlink { range: Range::cell(at), target, display, tooltip: None });
        }
        if let Some(comment) = cell.children().find(|n| is(*n, "Comment")) {
            let text: String =
                comment.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
            let author = attribute(comment, "Author").unwrap_or("").to_owned();
            sheet.notes.insert((r, col), Note { author, text: text.trim().to_owned() });
        }
        if !made.is_blank() {
            sheet.cells.insert((r, col), std::mem::take(&mut made));
        }
        col += across + 1;
    }
}

/// The range an `ArrayRange` names, in R1C1 syntax.
fn array_range(text: &str, at: CellRef) -> Option<Range> {
    let a1 = r1c1_to_a1(text, at.row, at.col);
    Range::parse(&a1.replace('$', ""))
}

/// A cell's value from its `Data` element.
fn data_value(data: Node, date1904: bool) -> Value {
    let text: String =
        data.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
    match attribute(data, "Type") {
        Some("Number") => text.trim().parse::<f64>().map_or(Value::Empty, Value::Number),
        Some("Boolean") => {
            Value::Bool(text.trim() == "1" || text.trim().eq_ignore_ascii_case("true"))
        }
        Some("Error") => Value::Error(ErrorKind::parse(&text).unwrap_or(ErrorKind::Value)),
        Some("DateTime") => {
            crate::ods::date_serial(text.trim(), date1904).map_or(Value::Empty, Value::Number)
        }
        _ => Value::text(text),
    }
}

/// A formula in R1C1 syntax as a formula.
fn formula_of(text: &str, at: CellRef) -> Formula {
    let body = text.strip_prefix('=').unwrap_or(text);
    let a1 = r1c1_to_a1(body, at.row, at.col);
    crate::ods::kept_formula(&a1)
}

fn read_options(options: Node, sheet: &mut Sheet, book: &mut Workbook, index: usize) {
    let number = |name: &str| {
        options
            .children()
            .find(|n| is(*n, name))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse::<u32>().ok())
    };
    let has = |name: &str| options.children().any(|n| is(n, name));
    if has("FreezePanes") {
        let rows = number("SplitHorizontal").unwrap_or(0);
        let cols = number("SplitVertical").unwrap_or(0);
        if rows > 0 || cols > 0 {
            sheet.frozen = Some((rows, cols));
        }
    }
    if has("DoNotDisplayGridlines") {
        sheet.show_grid = false;
    }
    if let Some(zoom) = number("Zoom") {
        sheet.zoom = zoom;
    }
    if let Some(visible) = options.children().find(|n| is(*n, "Visible")).and_then(|n| n.text()) {
        sheet.state = if visible.trim() == "SheetVeryHidden" {
            Visibility::VeryHidden
        } else {
            Visibility::Hidden
        };
    }
    if has("Selected") {
        book.active_sheet = index;
    }
}

fn read_names(names: Node, book: &mut Workbook, scope: Option<usize>) {
    for name in names.children().filter(|n| is(*n, "NamedRange")) {
        let (Some(n), Some(refers)) = (attribute(name, "Name"), attribute(name, "RefersTo")) else {
            continue;
        };
        let formula = r1c1_to_a1(refers.strip_prefix('=').unwrap_or(refers), 0, 0);
        book.names.push(DefinedName {
            name: n.to_owned(),
            scope,
            formula,
            hidden: attribute(name, "Hidden") == Some("1"),
            attrs: Vec::new(),
        });
    }
}

fn read_validation(node: Node) -> Option<Validation> {
    let text = |name: &str| {
        node.children().find(|n| is(*n, name)).and_then(|n| n.text()).map(|t| t.trim().to_owned())
    };
    let range = text("Range")?;
    let ranges: Vec<Range> = range
        .split(',')
        .filter_map(|r| Range::parse(&r1c1_to_a1(r, 0, 0).replace('$', "")))
        .collect();
    let kind = match text("Type")?.as_str() {
        "List" => "list",
        "Whole" => "whole",
        "Decimal" => "decimal",
        "Date" => "date",
        "Time" => "time",
        "TextLength" => "textLength",
        "Custom" => "custom",
        _ => return None,
    };
    let operator = text("Qualifier").map(|q| {
        match q.as_str() {
            "Between" => "between",
            "NotBetween" => "notBetween",
            "Equal" => "equal",
            "NotEqual" => "notEqual",
            "Greater" => "greaterThan",
            "Less" => "lessThan",
            "GreaterOrEqual" => "greaterThanOrEqual",
            "LessOrEqual" => "lessThanOrEqual",
            _ => "between",
        }
        .to_owned()
    });
    Some(Validation {
        ranges,
        kind: kind.to_owned(),
        operator,
        formula1: text("Value").or_else(|| text("Min")),
        formula2: text("Max"),
        allow_blank: !node
            .children()
            .any(|n| is(n, "IgnoreBlanks") && n.text().is_some_and(|t| t.trim() == "False")),
        show_dropdown: true,
        prompt_title: None,
        prompt: text("InputMessage"),
        error_title: text("ErrorTitle"),
        error: text("ErrorMessage"),
        error_style: None,
        attrs: Vec::new(),
    })
}

// ---------------------------------------------------------------------------------------------------
// R1C1.

/// Translate a formula (or a reference) in R1C1 syntax to A1, for a cell at `row` and `col` (zero based).
pub fn r1c1_to_a1(text: &str, row: u32, col: u32) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        out.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == '\'' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                i += 1;
                if chars[i - 1] == '\'' {
                    if chars.get(i) == Some(&'\'') {
                        out.push('\'');
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }
        let word_start =
            i == 0 || !(chars[i - 1].is_alphanumeric() || "_.$".contains(chars[i - 1]));
        if (c == 'R' || c == 'r' || c == 'C' || c == 'c') && word_start {
            if let Some((reference, next)) = r1c1_reference(&chars, i, row, col) {
                out.push_str(&reference);
                i = next;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A part of an R1C1 reference: `[n]` relative, `n` absolute, or nothing, which is relative by zero.
fn r1c1_part(chars: &[char], at: &mut usize) -> (bool, i64) {
    if chars.get(*at) == Some(&'[') {
        let end = chars[*at..].iter().position(|c| *c == ']').map_or(chars.len(), |p| *at + p);
        let n: i64 = chars[*at + 1..end].iter().collect::<String>().parse().unwrap_or(0);
        *at = (end + 1).min(chars.len());
        return (false, n);
    }
    let start = *at;
    while chars.get(*at).is_some_and(|c| c.is_ascii_digit()) {
        *at += 1;
    }
    if *at > start {
        return (true, chars[start..*at].iter().collect::<String>().parse().unwrap_or(0));
    }
    (false, 0)
}

/// An R1C1 reference at `start`, as A1 text and the index after it, or `None` when the word is not one.
fn r1c1_reference(chars: &[char], start: usize, row: u32, col: u32) -> Option<(String, usize)> {
    let mut at = start;
    let first = chars[at].to_ascii_uppercase();
    at += 1;
    let (mut row_part, mut col_part) = (None, None);
    if first == 'R' {
        row_part = Some(r1c1_part(chars, &mut at));
        if matches!(chars.get(at), Some('C') | Some('c')) {
            at += 1;
            col_part = Some(r1c1_part(chars, &mut at));
        }
    } else {
        col_part = Some(r1c1_part(chars, &mut at));
    }
    if chars.get(at).is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '(' || *c == '.') {
        return None;
    }
    let touches_colon = chars.get(at) == Some(&':') || (start > 0 && chars[start - 1] == ':');
    match (row_part, col_part) {
        (Some(r), Some(c)) => Some((cell_text(r, c, row, col)?, at)),
        (Some(r), None) if touches_colon => Some((row_text(r, row)?, at)),
        (None, Some(c)) if touches_colon => Some((column_text(c, col)?, at)),
        _ => None,
    }
}

fn cell_text(r: (bool, i64), c: (bool, i64), row: u32, col: u32) -> Option<String> {
    let row_number = if r.0 { r.1 } else { row as i64 + 1 + r.1 };
    let col_number = if c.0 { c.1 } else { col as i64 + 1 + c.1 };
    if row_number < 1
        || col_number < 1
        || col_number > MAX_COLS as i64
        || row_number > MAX_ROWS as i64
    {
        return Some("#REF!".to_owned());
    }
    Some(format!(
        "{}{}{}{}",
        if c.0 { "$" } else { "" },
        column_name(col_number as u32 - 1),
        if r.0 { "$" } else { "" },
        row_number
    ))
}

fn row_text(r: (bool, i64), row: u32) -> Option<String> {
    let number = if r.0 { r.1 } else { row as i64 + 1 + r.1 };
    Some(format!("{}{}", if r.0 { "$" } else { "" }, number.max(1)))
}

fn column_text(c: (bool, i64), col: u32) -> Option<String> {
    let number = if c.0 { c.1 } else { col as i64 + 1 + c.1 };
    Some(format!("{}{}", if c.0 { "$" } else { "" }, column_name((number.max(1) - 1) as u32)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_and_absolute_references_become_a1() {
        // The cell is C5: row 4, column 2 when counted from zero.
        for (r1c1, a1) in [
            ("RC[-1]*2", "B5*2"),
            ("R[-1]C", "C4"),
            ("R1C1", "$A$1"),
            ("SUM(R[-4]C:R[-1]C)", "SUM(C1:C4)"),
            ("R2C[1]+RC4", "D$2+$D5"),
            ("Sheet1!R1C1:R3C2", "Sheet1!$A$1:$B$3"),
            ("\"RC\"&R[1]C[1]", "\"RC\"&D6"),
            ("ROUND(RC,0)", "ROUND(C5,0)"),
            ("R[-1]:R[1]", "4:6"),
        ] {
            assert_eq!(r1c1_to_a1(r1c1, 4, 2), a1, "{r1c1}");
        }
    }

    #[test]
    fn a_small_workbook_reads_with_its_styles_merges_and_formulas() {
        let xml = r##"<?xml version="1.0"?>
<Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet" xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet">
 <Styles>
  <Style ss:ID="Default" ss:Name="Normal"><Font ss:FontName="Calibri" ss:Size="11"/></Style>
  <Style ss:ID="s1"><Font ss:Bold="1" ss:Color="#FF0000"/><NumberFormat ss:Format="0.00"/></Style>
 </Styles>
 <Worksheet ss:Name="Data"><Table>
  <Column ss:Width="96"/>
  <Row><Cell ss:StyleID="s1" ss:MergeAcross="1"><Data ss:Type="Number">1.5</Data></Cell></Row>
  <Row ss:Index="3"><Cell ss:Index="2" ss:Formula="=R[-2]C[-1]*2"><Data ss:Type="Number">3</Data></Cell>
   <Cell><Data ss:Type="DateTime">2026-10-23T00:00:00.000</Data></Cell><Cell><Data ss:Type="Boolean">1</Data></Cell></Row>
 </Table>
 <WorksheetOptions><FreezePanes/><FrozenNoSplit/><SplitHorizontal>1</SplitHorizontal></WorksheetOptions></Worksheet>
</Workbook>"##;
        let book = read(xml.as_bytes()).unwrap();
        let s = &book.sheets[0];
        assert_eq!(s.name, "Data");
        assert_eq!(s.value_at(CellRef::new(0, 0)), Value::Number(1.5));
        assert_eq!(s.merges, vec![Range::parse("A1:B1").unwrap()]);
        assert_eq!(
            s.cell(CellRef::new(2, 1)).unwrap().formula.as_ref().unwrap().display(),
            "=A1*2"
        );
        assert_eq!(s.value_at(CellRef::new(2, 2)), Value::Number(46318.0));
        assert_eq!(s.value_at(CellRef::new(2, 3)), Value::Bool(true));
        assert_eq!(s.frozen, Some((1, 0)));
        assert!(book.styles.font_of(s.cell(CellRef::new(0, 0)).unwrap().style).bold);
        assert_eq!(book.styles.format_code(s.cell(CellRef::new(0, 0)).unwrap().style), "0.00");
    }
}
