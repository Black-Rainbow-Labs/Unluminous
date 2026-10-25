//! A document's properties as Word's Properties dialog shows them (task-2359, TDD 5.6): the summary in
//! `docProps/core.xml` and `docProps/app.xml`, the statistics, and the custom properties in
//! `docProps/custom.xml`, each with a name, a type and a value.

use crate::model::Document;
use crate::xml::{self, El, Node};

const APP: &str = "docProps/app.xml";
const CUSTOM: &str = "docProps/custom.xml";
const APP_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const CUSTOM_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const VT_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
/// The format id every custom property carries, which Word writes.
const FMTID: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";

/// One custom property: its name, its type (`text`, `number`, `date`, `yes or no`) and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Custom {
    pub name: String,
    pub kind: String,
    pub value: String,
}

fn part_root(document: &Document, name: &str) -> Option<El> {
    xml::parse_bytes(document.package.part(name)?).ok()
}

/// One property of `docProps/app.xml`, such as `Company`, `Manager`, `TotalTime` or `Pages`.
pub fn extended(document: &Document, name: &str) -> Option<String> {
    let root = part_root(document, APP)?;
    let value = root.elements().find(|e| e.local() == name).map(El::text);
    value.filter(|t| !t.is_empty())
}

/// Set a property of `docProps/app.xml`, making the part when the document has none.
pub fn set_extended(document: &mut Document, name: &str, value: &str) {
    let mut root = part_root(document, APP).unwrap_or_else(|| El::new("Properties"));
    let existing = root.elements_mut().find(|e| e.local() == name);
    match existing {
        Some(el) => el.children = vec![Node::Text(value.to_owned())],
        None => root.children.push(Node::El(El::new(name).with_text(value))),
    }
    write_part(
        document,
        APP,
        &root,
        APP_NS,
        "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        "extended-properties",
    );
}

/// Every custom property.
pub fn custom(document: &Document) -> Vec<Custom> {
    let Some(root) = part_root(document, CUSTOM) else { return Vec::new() };
    let properties = root
        .elements()
        .filter(|e| e.local() == "property")
        .filter_map(|p| {
            let name = p.attr("name")?.to_owned();
            let value = p.elements().next()?;
            let kind = match value.local() {
                "lpwstr" | "lpstr" | "bstr" => "text",
                "i4" | "i8" | "r8" | "int" | "decimal" => "number",
                "filetime" | "date" => "date",
                "bool" => "yes or no",
                other => other,
            };
            Some(Custom { name, kind: kind.to_owned(), value: value.text() })
        })
        .collect();
    properties
}

/// Set a custom property, `kind` being `text`, `number`, `date` (as `2026-10-24` or a full time) or
/// `yes or no` (`yes` or `no`).
pub fn set_custom(
    document: &mut Document,
    name: &str,
    kind: &str,
    value: &str,
) -> Result<(), String> {
    let (element, value) = match kind {
        "text" => ("vt:lpwstr", value.to_owned()),
        "number" => {
            let number: f64 = value.parse().map_err(|_| format!("{value} is not a number"))?;
            if number.fract() == 0.0 && number.abs() < 2_147_483_648.0 {
                ("vt:i4", format!("{}", number as i64))
            } else {
                ("vt:r8", value.to_owned())
            }
        }
        "date" => (
            "vt:filetime",
            if value.len() == 10 { format!("{value}T00:00:00Z") } else { value.to_owned() },
        ),
        "yes or no" | "bool" | "boolean" => (
            "vt:bool",
            match value.to_ascii_lowercase().as_str() {
                "yes" | "true" | "1" | "on" => "true".to_owned(),
                _ => "false".to_owned(),
            },
        ),
        other => {
            return Err(format!(
                "a custom property is text, number, date or yes or no, not {other}"
            ))
        }
    };
    let mut root = part_root(document, CUSTOM).unwrap_or_else(|| El::new("Properties"));
    root.children.retain(|n| !matches!(n, Node::El(e) if e.attr("name") == Some(name)));
    let next = root
        .elements()
        .filter_map(|e| e.attr("pid").and_then(|p| p.parse::<i64>().ok()))
        .max()
        .map_or(2, |p| p + 1);
    root.children.push(Node::El(
        El::new("property")
            .with("fmtid", FMTID)
            .with("pid", next.to_string())
            .with("name", name)
            .with_child(El::new(element).with_text(value)),
    ));
    write_part(
        document,
        CUSTOM,
        &root,
        CUSTOM_NS,
        "application/vnd.openxmlformats-officedocument.custom-properties+xml",
        "custom-properties",
    );
    Ok(())
}

/// Take a custom property away.
pub fn remove_custom(document: &mut Document, name: &str) -> Result<(), String> {
    let mut root =
        part_root(document, CUSTOM).ok_or_else(|| format!("there is no custom property {name}"))?;
    let before = root.children.len();
    root.children.retain(|n| !matches!(n, Node::El(e) if e.attr("name") == Some(name)));
    if root.children.len() == before {
        return Err(format!("there is no custom property {name}"));
    }
    write_part(
        document,
        CUSTOM,
        &root,
        CUSTOM_NS,
        "application/vnd.openxmlformats-officedocument.custom-properties+xml",
        "custom-properties",
    );
    Ok(())
}

/// Write a properties part with its declaration, its content type and the package's relationship to it.
/// The parser keeps no default namespace, so the part's own (`namespace`) and `vt` are declared here.
fn write_part(
    document: &mut Document,
    name: &str,
    root: &El,
    namespace: &str,
    content_type: &str,
    kind: &str,
) {
    let mut text =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    text.push_str(&format!("<Properties xmlns=\"{namespace}\" xmlns:vt=\"{VT_NS}\">"));
    for child in root.elements() {
        child.write(&mut text);
    }
    text.push_str("</Properties>");
    document.package.set_part(name, text.into_bytes());
    document.package.set_override(name, content_type);
    let exists = document.package.rels.get("").is_some_and(|r| r.of_kind(kind).next().is_some());
    if !exists {
        document.package.rels_mut("").add(kind, name, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_and_extended_properties_are_written_and_read_back() {
        let mut document = crate::new::blank();
        set_custom(&mut document, "Project", "text", "Unluminous").unwrap();
        set_custom(&mut document, "Pages printed", "number", "12").unwrap();
        set_custom(&mut document, "Reviewed", "yes or no", "yes").unwrap();
        set_extended(&mut document, "Company", "Black Rainbow Labs");
        let again = crate::read::open(&crate::write::save(&document).unwrap()).unwrap();
        let custom = custom(&again);
        assert_eq!(custom.len(), 3);
        assert_eq!(
            custom[0],
            Custom { name: "Project".into(), kind: "text".into(), value: "Unluminous".into() }
        );
        assert_eq!(custom[1].value, "12");
        assert_eq!(custom[2].value, "true");
        assert_eq!(extended(&again, "Company").as_deref(), Some("Black Rainbow Labs"));
        let mut again = again;
        remove_custom(&mut again, "Project").unwrap();
        assert_eq!(super::custom(&again).len(), 2);
    }
}
