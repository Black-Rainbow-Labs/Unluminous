//! Filter criteria as Excel's AutoFilter method is given them, and the comparison of the XML Excel saved.

use serde_json::Value as Json;
use unluminous_sheet::edit::Session;

/// Remove the attributes with a prefix (`xr:uid`, `xmlns:xlrd2`), which Excel adds and nothing reads.
fn strip_prefixed_attributes(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(i) = rest.find(' ') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let name_end = rest[1..].find(['=', ' ', '>', '/']).map_or(rest.len(), |e| e + 1);
        let name = &rest[1..name_end];
        if name.contains(':') && rest[name_end..].starts_with("=\"") {
            let value_end =
                rest[name_end + 2..].find('"').map_or(rest.len(), |e| name_end + 2 + e + 1);
            rest = &rest[value_end..];
        } else {
            out.push(' ');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn canonical_node(node: roxmltree::Node, out: &mut String) {
    if !node.is_element() {
        return;
    }
    out.push('<');
    out.push_str(node.tag_name().name());
    let mut attrs: Vec<(String, String)> = node
        .attributes()
        .filter(|a| a.namespace().is_none())
        .map(|a| (a.name().to_owned(), a.value().to_owned()))
        .collect();
    attrs.sort();
    for (k, v) in attrs {
        out.push_str(&format!(" {k}=\"{v}\""));
    }
    out.push('>');
    for child in node.children() {
        if child.is_text() {
            let t = child.text().unwrap_or("").trim();
            out.push_str(t);
        } else {
            canonical_node(child, out);
        }
    }
    out.push_str(&format!("</{}>", node.tag_name().name()));
}

/// The XML with attributes in a fixed order and nothing that is a prefixed attribute or whitespace.
pub fn canonical(xml: &str) -> String {
    let cleaned = strip_prefixed_attributes(xml);
    let cleaned = match cleaned.strip_prefix("<?xml") {
        Some(rest) => {
            rest.find("?>").map_or(cleaned.as_str(), |i| &rest[i + 2..]).trim_start().to_owned()
        }
        None => cleaned,
    };
    match roxmltree::Document::parse(&cleaned) {
        Ok(doc) => {
            let mut out = String::new();
            canonical_node(doc.root_element(), &mut out);
            out
        }
        Err(e) => format!("unreadable XML ({e}): {cleaned}"),
    }
}

/// The `autoFilter` element of a sheet part, as text.
fn auto_filter_of(part: &str) -> String {
    let Some(start) = part.find("<autoFilter") else { return String::new() };
    let rest = &part[start..];
    let head_end = rest.find('>').unwrap_or(rest.len());
    if rest[..head_end].ends_with('/') {
        return rest[..=head_end].to_owned();
    }
    let end = rest.find("</autoFilter>").map_or(rest.len(), |e| e + "</autoFilter>".len());
    rest[..end].to_owned()
}

pub fn compare_xml(s: &Session, expected: &Json, scenario: &Json) -> Vec<String> {
    let mut out = Vec::new();
    if expected["xml"].is_null() || scenario["noXml"].as_bool() == Some(true) {
        return out;
    }
    let bytes = match unluminous_sheet::xlsx::write(&s.book) {
        Ok(b) => b,
        Err(e) => return vec![format!("the engine could not write the workbook: {e}")],
    };
    let entries = unluminous_sheet::zip::read(&bytes).unwrap();
    let text_of = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| String::from_utf8_lossy(&e.data).into_owned())
            .unwrap_or_default()
    };
    for (i, sheet) in expected["xml"]["sheets"].as_array().into_iter().flatten().enumerate() {
        let theirs = sheet["autoFilter"].as_str().unwrap_or("");
        let part = format!("xl/worksheets/sheet{}.xml", i + 1);
        let mine = auto_filter_of(&text_of(&part));
        let (theirs, mine) = (
            if theirs.is_empty() { String::new() } else { canonical(theirs) },
            if mine.is_empty() { String::new() } else { canonical(&mine) },
        );
        if theirs != mine {
            out.push(format!("{part} autoFilter\n   Excel:  {theirs}\n   engine: {mine}"));
        }
    }
    let theirs = expected["xml"]["names"].as_str().unwrap_or("");
    let mine = {
        let book = text_of("xl/workbook.xml");
        match (book.find("<definedNames>"), book.find("</definedNames>")) {
            (Some(a), Some(b)) => book[a..b + "</definedNames>".len()].to_owned(),
            _ => String::new(),
        }
    };
    let (theirs, mine) = (
        if theirs.is_empty() { String::new() } else { canonical(theirs) },
        if mine.is_empty() { String::new() } else { canonical(&mine) },
    );
    // A refused operation leaves the workbook as it was; Excel still records the names.
    if theirs != mine && expected["error"].as_str().unwrap_or("").is_empty() {
        out.push(format!(
            "definedNames
   Excel:  {theirs}
   engine: {mine}"
        ));
    }
    for (i, table) in expected["xml"]["tables"].as_array().into_iter().flatten().enumerate() {
        let part = format!("xl/tables/table{}.xml", i + 1);
        let theirs = canonical(table["xml"].as_str().unwrap_or(""));
        let mine_text = text_of(&part);
        let mine = if mine_text.is_empty() { String::new() } else { canonical(&mine_text) };
        if theirs != mine {
            out.push(format!("{part}\n   Excel:  {theirs}\n   engine: {mine}"));
        }
    }
    out
}
