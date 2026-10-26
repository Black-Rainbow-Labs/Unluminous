//! The `.xlsx` and `.xlsm` file: SpreadsheetML in a zip, read into a [`Workbook`] and written back.
//!
//! **What is not modelled is kept.** Every part the writer does not regenerate is carried as bytes with
//! its relationship and its content type, and every worksheet and workbook element the model does not
//! hold is carried as its XML and written back in the place the schema puts it. A chart, a pivot table,
//! a picture or a macro project in a file is therefore still there after Unluminous saves it.

use crate::model::Relationship;

pub mod audit;
pub mod cf;
pub mod chart;
pub mod chart_drawing;
pub mod chart_read;
pub mod chart_write;
pub mod chartex;
pub mod read;
pub mod threads;
pub mod write;
pub mod xmltree;

pub use read::read;
pub use write::write;

pub const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const PKG_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
pub const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
pub const SPREADSHEET_DRAWING_NS: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

pub const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub const REL_WORKSHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
pub const REL_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
pub const REL_SHARED_STRINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
pub const REL_THEME: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub const REL_METADATA: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
pub const REL_CALC_CHAIN: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";
pub const REL_COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
pub const REL_VML: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
pub const REL_HYPERLINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
pub const REL_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
pub const REL_DRAWING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
pub const REL_CHART: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
pub const REL_CORE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
pub const REL_APP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";

pub const CT_WORKBOOK: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
pub const CT_WORKBOOK_MACRO: &str = "application/vnd.ms-excel.sheet.macroEnabled.main+xml";
pub const CT_WORKSHEET: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
pub const CT_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
pub const CT_SHARED_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
pub const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";
pub const CT_METADATA: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";
pub const CT_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
pub const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";
pub const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
pub const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
pub const CT_CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
pub const CT_CORE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
pub const CT_APP: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";
pub const CT_RELS: &str = "application/vnd.openxmlformats-package.relationships+xml";

/// Escape text for an attribute or element, and write the control characters XML cannot hold the way
/// Office does: `_x0007_`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {
                out.push_str(&format!("_x{:04X}_", c as u32))
            }
            c => out.push(c),
        }
    }
    out
}

/// Office's escapes for characters XML cannot hold: `_x000D_` back into a character.
pub fn unescape_office(text: &str) -> String {
    if !text.contains("_x") {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' && i + 6 < bytes.len() && bytes[i + 1] == b'x' && bytes[i + 6] == b'_' {
            if let Ok(code) = u32::from_str_radix(&text[i + 2..i + 6], 16) {
                if let Some(c) = char::from_u32(code) {
                    out.push(c);
                    i += 7;
                    continue;
                }
            }
        }
        let c = text[i..].chars().next().unwrap_or('_');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// A part's path from a relationship target, relative to the folder of the part that holds the
/// relationship, or absolute from the package root.
pub fn resolve(from_part: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_owned();
    }
    let mut parts: Vec<&str> = from_part.split('/').collect();
    parts.pop();
    for segment in target.split('/') {
        match segment {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// A target for a relationship from `from_part` to `to_part`, relative where Excel writes it relative.
pub fn relative(from_part: &str, to_part: &str) -> String {
    let from: Vec<&str> = from_part.split('/').collect();
    let to: Vec<&str> = to_part.split('/').collect();
    let base = &from[..from.len() - 1];
    let common = base.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out: Vec<String> = Vec::new();
    for _ in common..base.len() {
        out.push("..".to_owned());
    }
    out.extend(to[common..].iter().map(|s| s.to_string()));
    out.join("/")
}

/// The `.rels` part that holds a part's relationships.
pub fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((folder, name)) => format!("{folder}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Read a `.rels` part.
pub fn parse_rels(xml: &str) -> Vec<Relationship> {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|n| n.has_tag_name("Relationship"))
        .map(|n| Relationship {
            id: n.attribute("Id").unwrap_or("").to_owned(),
            kind: n.attribute("Type").unwrap_or("").to_owned(),
            target: n.attribute("Target").unwrap_or("").to_owned(),
            external: n.attribute("TargetMode") == Some("External"),
        })
        .collect()
}

/// Write a `.rels` part.
pub fn write_rels(rels: &[Relationship]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for r in rels {
        out.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
            escape(&r.id),
            escape(&r.kind),
            escape(&r.target),
            if r.external { " TargetMode=\"External\"" } else { "" }
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// A relationship id not in use: `rId1`, `rId2`, ...
pub fn next_id(rels: &[Relationship]) -> String {
    (1..)
        .map(|n| format!("rId{n}"))
        .find(|id| !rels.iter().any(|r| r.id == *id))
        .unwrap_or_default()
}

/// The namespace declarations of an element, as `(prefix, uri)`, prefix empty for the default.
pub fn namespaces_of(node: roxmltree::Node) -> Vec<(String, String)> {
    node.namespaces().map(|ns| (ns.name().unwrap_or("").to_owned(), ns.uri().to_owned())).collect()
}

/// The text of an element's attributes as written, for keeping them: `name="value"` pairs with their
/// prefixes recovered from the document's declarations.
pub fn attrs_of(node: roxmltree::Node, skip: &[&str]) -> Vec<(String, String)> {
    node.attributes()
        .filter(|a| !(a.namespace().is_none() && skip.contains(&a.name())))
        .map(|a| {
            let name = match a.namespace() {
                Some(ns) => match node.lookup_prefix(ns) {
                    Some(prefix) if !prefix.is_empty() => format!("{prefix}:{}", a.name()),
                    _ => a.name().to_owned(),
                },
                None => a.name().to_owned(),
            };
            (name, a.value().to_owned())
        })
        .collect()
}

/// Attributes as text, each with a leading space.
pub fn attrs_text(attrs: &[(String, String)]) -> String {
    attrs.iter().map(|(k, v)| format!(" {k}=\"{}\"", escape(v))).collect()
}

/// The XML of a node as it was written in the source.
pub fn raw<'a>(source: &'a str, node: roxmltree::Node) -> &'a str {
    &source[node.range()]
}

/// The namespace declarations as attribute text, for a root element whose kept children use them.
pub fn namespaces_text(namespaces: &[(String, String)], default: &str) -> String {
    let mut out = format!(" xmlns=\"{default}\"");
    for (prefix, uri) in namespaces {
        if prefix.is_empty() {
            continue;
        }
        out.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape(uri)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relationship_targets_resolve_and_relativise() {
        assert_eq!(resolve("xl/workbook.xml", "worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml");
        assert_eq!(
            resolve("xl/worksheets/sheet1.xml", "../drawings/drawing1.xml"),
            "xl/drawings/drawing1.xml"
        );
        assert_eq!(resolve("xl/workbook.xml", "/xl/styles.xml"), "xl/styles.xml");
        assert_eq!(relative("xl/worksheets/sheet1.xml", "xl/comments1.xml"), "../comments1.xml");
        assert_eq!(
            relative("xl/workbook.xml", "xl/worksheets/sheet2.xml"),
            "worksheets/sheet2.xml"
        );
        assert_eq!(rels_path("xl/worksheets/sheet1.xml"), "xl/worksheets/_rels/sheet1.xml.rels");
    }

    #[test]
    fn control_characters_round_trip_through_office_escapes() {
        let text = "a\u{7}b&<c>";
        assert_eq!(escape(text), "a_x0007_b&amp;&lt;c&gt;");
        assert_eq!(unescape_office("a_x0007_b"), "a\u{7}b");
    }
}
