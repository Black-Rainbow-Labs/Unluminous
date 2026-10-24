//! A workbook as an HTML page: one table for each sheet, the text of each cell as it shows, the formatting
//! written inline, and merged cells as `colspan` and `rowspan`. Writing only.

use std::collections::HashSet;

use crate::coord::CellRef;
use crate::model::{LinkTarget, Sheet, Visibility, Workbook};
use crate::numfmt::Format;
use crate::styles::{Edge, Underline};
use crate::value::Value;

/// The pixels one character of the default font is wide, for turning column widths into CSS.
const PIXELS_PER_CHARACTER: f64 = 7.0;

/// Escape text for HTML, with a line break as `<br>`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\r' => {}
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

/// The workbook as a page, every visible sheet in its own table under its name.
pub fn write(book: &Workbook) -> String {
    let title = book.sheets.first().map_or("", |s| s.name.as_str());
    let mut out = format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>\ntable {{ border-collapse: collapse; }}\ntd {{ padding: 1px 4px; vertical-align: bottom; }}\n</style>\n</head>\n<body>\n",
        escape(title)
    );
    for (index, sheet) in book.sheets.iter().enumerate() {
        if sheet.state != Visibility::Visible || sheet.kept_part.is_some() {
            continue;
        }
        out.push_str(&format!("<h2>{}</h2>\n", escape(&sheet.name)));
        out.push_str(&table(book, index, sheet));
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// One sheet as a table.
fn table(book: &Workbook, index: usize, sheet: &Sheet) -> String {
    let Some(used) = sheet.used_range() else { return "<table></table>\n".to_owned() };
    let mut out = String::from("<table>\n");
    let last_col = sheet.merges.iter().map(|m| m.last.col).fold(used.last.col, u32::max);
    let last_row = sheet.merges.iter().map(|m| m.last.row).fold(used.last.row, u32::max);
    let columns: Vec<u32> = (0..=last_col).filter(|c| !sheet.is_column_hidden(*c)).collect();
    for c in &columns {
        let width = sheet.column(*c).and_then(|i| i.width).or(sheet.default_col_width).unwrap_or(8.43);
        out.push_str(&format!("<col style=\"width:{}px\">\n", (width * PIXELS_PER_CHARACTER + 5.0).round()));
    }
    let covered = covered(sheet);
    for r in 0..=last_row {
        if sheet.is_row_hidden(r) {
            continue;
        }
        out.push_str("<tr>");
        for c in &columns {
            let at = CellRef::new(r, *c);
            if covered.contains(&(r, *c)) {
                continue;
            }
            out.push_str(&cell(book, index, sheet, at));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

/// The cells under the top left cell of each merge.
fn covered(sheet: &Sheet) -> HashSet<(u32, u32)> {
    let mut set = HashSet::new();
    for merge in &sheet.merges {
        for at in merge.cells().filter(|at| *at != merge.first) {
            set.insert((at.row, at.col));
        }
    }
    set
}

/// One `td`.
fn cell(book: &Workbook, index: usize, sheet: &Sheet, at: CellRef) -> String {
    let stored = sheet.cell(at);
    let style = stored.map_or(0, |c| c.style);
    let value = sheet.value_at(at);
    let code = book.styles.format_code(style);
    let shown = Format::parse(&code).render(&value, book.date1904, 11);
    let mut attrs = String::new();
    if let Some(merge) = sheet.merges.iter().find(|m| m.first == at) {
        if merge.cols() > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", merge.cols()));
        }
        if merge.rows() > 1 {
            attrs.push_str(&format!(" rowspan=\"{}\"", merge.rows()));
        }
    }
    if let Some(note) = sheet.notes.get(&(at.row, at.col)) {
        attrs.push_str(&format!(" title=\"{}\"", escape(&note.text).replace("<br>", "&#10;")));
    }
    let css = css(book, style, &value, shown.numeric);
    if !css.is_empty() {
        attrs.push_str(&format!(" style=\"{css}\""));
    }
    let text = escape(&shown.text);
    let body = match link_at(sheet, at) {
        Some(href) => format!("<a href=\"{}\">{text}</a>", escape(&href)),
        None => text,
    };
    let _ = index;
    format!("<td{attrs}>{body}</td>")
}

/// The address a cell links to.
fn link_at(sheet: &Sheet, at: CellRef) -> Option<String> {
    let link = sheet.links.iter().find(|l| l.range.contains(at))?;
    Some(match &link.target {
        LinkTarget::External(url) => url.clone(),
        LinkTarget::Location(place) => format!("#{place}"),
    })
}

/// The inline style of a cell: font, colours, alignment and borders.
fn css(book: &Workbook, xf: u32, value: &Value, numeric: bool) -> String {
    let styles = &book.styles;
    let (font, fill, border, format) = (styles.font_of(xf), styles.fill_of(xf), styles.border_of(xf), styles.xf(xf));
    let theme = &styles.theme;
    let default = styles.font_of(0);
    let mut css = Vec::new();
    if font.name != default.name {
        css.push(format!("font-family:'{}'", font.name));
    }
    if font.size != default.size {
        css.push(format!("font-size:{}pt", font.size));
    }
    if font.bold {
        css.push("font-weight:bold".to_owned());
    }
    if font.italic {
        css.push("font-style:italic".to_owned());
    }
    let mut decorations = Vec::new();
    if font.underline != Underline::None {
        decorations.push("underline");
    }
    if font.strike {
        decorations.push("line-through");
    }
    if !decorations.is_empty() {
        css.push(format!("text-decoration:{}", decorations.join(" ")));
    }
    if font.color != default.color {
        if let Some((r, g, b)) = font.color.resolve(theme) {
            css.push(format!("color:#{r:02X}{g:02X}{b:02X}"));
        }
    }
    if let Some((r, g, b)) = fill.paint().and_then(|c| c.resolve(theme)) {
        css.push(format!("background:#{r:02X}{g:02X}{b:02X}"));
    }
    match format.align.horizontal.as_deref() {
        Some("left") => css.push("text-align:left".to_owned()),
        Some("right") => css.push("text-align:right".to_owned()),
        Some("center") | Some("centerContinuous") => css.push("text-align:center".to_owned()),
        Some("justify") | Some("distributed") => css.push("text-align:justify".to_owned()),
        _ if numeric && !matches!(value, Value::Text(_)) => css.push("text-align:right".to_owned()),
        _ => {}
    }
    match format.align.vertical.as_deref() {
        Some("top") => css.push("vertical-align:top".to_owned()),
        Some("center") => css.push("vertical-align:middle".to_owned()),
        _ => {}
    }
    if format.align.wrap {
        css.push("white-space:pre-wrap".to_owned());
    }
    for (name, edge) in [("left", &border.left), ("right", &border.right), ("top", &border.top), ("bottom", &border.bottom)] {
        if let Some(text) = border_css(edge, theme) {
            css.push(format!("border-{name}:{text}"));
        }
    }
    css.join(";")
}

/// An edge as a CSS border.
fn border_css(edge: &Edge, theme: &[u32]) -> Option<String> {
    if edge.is_none() {
        return None;
    }
    let (width, line) = match edge.style.as_str() {
        "medium" | "mediumDashed" | "mediumDashDot" | "mediumDashDotDot" => ("2px", "solid"),
        "thick" => ("3px", "solid"),
        "double" => ("3px", "double"),
        "dotted" | "hair" => ("1px", "dotted"),
        "dashed" | "dashDot" | "dashDotDot" | "slantDashDot" => ("1px", "dashed"),
        _ => ("1px", "solid"),
    };
    let color = edge.color.resolve(theme).map_or("#000000".to_owned(), |(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
    Some(format!("{width} {line} {color}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Cell;
    use crate::styles::{FontEdit, StyleChange};

    #[test]
    fn a_sheet_is_a_table_with_shown_text_merges_and_inline_formatting() {
        let mut book = Workbook::new();
        let bold = book.styles.derive(0, &StyleChange::Font(FontEdit { bold: Some(true), ..Default::default() }));
        let money = book.styles.derive(0, &StyleChange::NumberFormat("#,##0.00".to_owned()));
        let s = &mut book.sheets[0];
        s.set(CellRef::new(0, 0), Cell { value: Value::text("A & B"), style: bold, ..Default::default() });
        s.set(CellRef::new(1, 0), Cell { value: Value::Number(1234.5), style: money, ..Default::default() });
        s.merges.push(crate::coord::Range::parse("A1:B1").unwrap());
        let html = write(&book);
        assert!(html.contains("<td colspan=\"2\" style=\"font-weight:bold\">A &amp; B</td>"), "{html}");
        assert!(html.contains("1,234.50</td>"));
        assert_eq!(html.matches("<td").count(), 3, "the merged cell's second half is not written");
    }
}
