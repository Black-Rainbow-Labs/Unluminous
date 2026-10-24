//! A small XML tree that keeps everything it reads, and writes it back.
//!
//! Every part of a `.docx` Unluminous does not model, and every element inside a modelled one, is held as
//! an [`El`]: its qualified name, its attributes in the order they were read, and its children. Writing
//! one back produces the same element, so a save keeps what nobody understood.
//!
//! **Names carry a prefix, and the prefix for a namespace Word uses is always Word's own.** A file written
//! by another program can call the main namespace `ns0`; reading it as `w` means every element Unluminous
//! makes and every element it kept agree, and the document element declares each canonical prefix once
//! (`write::root_start`). A namespace that is not one of Word's keeps the prefix the source gave it, and
//! the element that declared it declares it again when it is written.

use std::fmt::Write as _;

/// The main WordprocessingML namespace.
pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// Relationships, as an attribute namespace inside a part.
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The prefixes Word itself writes, by namespace. A document element declares all of them.
pub const CANONICAL: &[(&str, &str)] = &[
    ("w", W),
    ("r", R),
    ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"),
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    ("pic", "http://schemas.openxmlformats.org/drawingml/2006/picture"),
    ("mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("v", "urn:schemas-microsoft-com:vml"),
    ("w10", "urn:schemas-microsoft-com:office:word"),
    ("m", "http://schemas.openxmlformats.org/officeDocument/2006/math"),
    ("wp14", "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing"),
    ("w14", "http://schemas.microsoft.com/office/word/2010/wordml"),
    ("w15", "http://schemas.microsoft.com/office/word/2012/wordml"),
    ("wps", "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"),
    ("wpg", "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"),
    ("wpc", "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"),
];

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// One node of the tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    El(El),
    Text(String),
}

/// One element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct El {
    /// `w:p`, `a:graphic`, or a name with no prefix in a part with a default namespace.
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// Namespaces this element declared that are not Word's own, as `(prefix, uri)`, written back on it.
    pub declares: Vec<(String, String)>,
}

impl El {
    /// An element with no attributes and no children.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    /// The same element with one attribute added, for building trees in one expression.
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Self {
        self.set_attr(name, value);
        self
    }

    /// The same element with one child added.
    pub fn with_child(mut self, child: El) -> Self {
        self.children.push(Node::El(child));
        self
    }

    /// The same element holding text.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.children.push(Node::Text(text.into()));
        self
    }

    /// The name without its prefix.
    pub fn local(&self) -> &str {
        local(&self.name)
    }

    pub fn is(&self, name: &str) -> bool {
        self.name == name
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    /// Set an attribute, keeping its place when it was already there.
    pub fn set_attr(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        match self.attrs.iter_mut().find(|(key, _)| key == name) {
            Some(slot) => slot.1 = value,
            None => self.attrs.push((name.to_owned(), value)),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(key, _)| key != name);
    }

    /// The element children, in order.
    pub fn elements(&self) -> impl Iterator<Item = &El> {
        self.children.iter().filter_map(|node| match node {
            Node::El(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut El> {
        self.children.iter_mut().filter_map(|node| match node {
            Node::El(el) => Some(el),
            Node::Text(_) => None,
        })
    }

    /// The first child called `name`.
    pub fn child(&self, name: &str) -> Option<&El> {
        self.elements().find(|el| el.name == name)
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut El> {
        self.elements_mut().find(|el| el.name == name)
    }

    /// Every child called `name`.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a El> + 'a {
        self.elements().filter(move |el| el.name == name)
    }

    /// Take every child called `name` out.
    pub fn remove_children(&mut self, name: &str) {
        self.children.retain(|node| !matches!(node, Node::El(el) if el.name == name));
    }

    /// The child called `name`, made when it is missing, appended at the end.
    ///
    /// Order inside a properties element is put right by [`crate::order::sort`] when it is written, so
    /// a child added here does not have to be put in its place.
    pub fn child_or_insert(&mut self, name: &str) -> &mut El {
        if self.child(name).is_none() {
            self.children.push(Node::El(El::new(name)));
        }
        self.child_mut(name).expect("just inserted")
    }

    /// Replace the child called `name` with `child`, in its place, or append it.
    pub fn put(&mut self, child: El) {
        for node in &mut self.children {
            if let Node::El(el) = node {
                if el.name == child.name {
                    *el = child;
                    return;
                }
            }
        }
        self.children.push(Node::El(child));
    }

    /// All the text inside, in document order.
    pub fn text(&self) -> String {
        let mut out = String::new();
        self.collect_text(&mut out);
        out
    }

    fn collect_text(&self, out: &mut String) {
        for node in &self.children {
            match node {
                Node::Text(text) => out.push_str(text),
                Node::El(el) => el.collect_text(out),
            }
        }
    }

    /// The `w:val` of the child called `name`.
    pub fn val_of(&self, name: &str) -> Option<&str> {
        self.child(name).and_then(|el| el.attr("w:val"))
    }

    /// Every element below this one, depth first, this one included.
    pub fn walk(&self, visit: &mut dyn FnMut(&El)) {
        visit(self);
        for child in self.elements() {
            child.walk(visit);
        }
    }

    /// Write the element as XML.
    pub fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (prefix, uri) in &self.declares {
            let _ = write!(out, " xmlns:{prefix}=\"");
            escape_attr(uri, out);
            out.push('"');
        }
        for (key, value) in &self.attrs {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            escape_attr(value, out);
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for node in &self.children {
            match node {
                Node::Text(text) => escape_text(text, out),
                Node::El(el) => el.write(out),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }

    /// The element as a string of XML.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

/// The part of a qualified name after its prefix.
pub fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

/// Escape text content.
pub fn escape_text(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            // A control character other than tab and the line ends is not allowed in XML 1.0 at all.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
}

/// Escape an attribute value, which is always written in double quotes.
pub fn escape_attr(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
}

/// Parse a whole part and answer its document element.
pub fn parse(text: &str) -> Result<El, String> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let document = roxmltree::Document::parse_with_options(text, options)
        .map_err(|problem| format!("the XML could not be read: {problem}"))?;
    Ok(from_node(document.root_element()))
}

/// Parse the bytes of a part, which Office writes as UTF-8 and occasionally as UTF-16.
pub fn parse_bytes(bytes: &[u8]) -> Result<El, String> {
    parse(&decode(bytes))
}

/// The text of a part's bytes: UTF-8 with or without a byte order mark, or UTF-16 with one.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |little: bool| {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| match little {
                true => u16::from_le_bytes([pair[0], pair[1]]),
                false => u16::from_be_bytes([pair[0], pair[1]]),
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, ..] => utf16(true),
        [0xFE, 0xFF, ..] => utf16(false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The prefix a namespace is written with.
fn prefix_for(node: &roxmltree::Node, uri: &str) -> Option<String> {
    if uri == XML_NAMESPACE {
        return Some("xml".to_owned());
    }
    if let Some((prefix, _)) = CANONICAL.iter().find(|(_, known)| *known == uri) {
        return Some((*prefix).to_owned());
    }
    node.lookup_prefix(uri).map(str::to_owned)
}

fn qualified(node: &roxmltree::Node, uri: Option<&str>, local: &str) -> String {
    match uri.and_then(|uri| prefix_for(node, uri)) {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}:{local}"),
        _ => local.to_owned(),
    }
}

/// Build an [`El`] from a parsed element.
fn from_node(node: roxmltree::Node) -> El {
    let tag = node.tag_name();
    let mut el = El::new(qualified(&node, tag.namespace(), tag.name()));
    el.attrs = node
        .attributes()
        .map(|attribute| {
            let name = match attribute.namespace() {
                Some(uri) => qualified(&node, Some(uri), attribute.name()),
                None => attribute.name().to_owned(),
            };
            (name, attribute.value().to_owned())
        })
        .collect();
    // The namespaces in scope here that the parent did not have, minus Word's own, which the document
    // element declares.
    let inherited: Vec<(Option<&str>, &str)> = node
        .parent_element()
        .map(|parent| parent.namespaces().map(|ns| (ns.name(), ns.uri())).collect())
        .unwrap_or_default();
    for namespace in node.namespaces() {
        let pair = (namespace.name(), namespace.uri());
        let canonical = CANONICAL.iter().any(|(_, uri)| *uri == namespace.uri());
        if !inherited.contains(&pair) && !canonical {
            if let Some(prefix) = namespace.name() {
                el.declares.push((prefix.to_owned(), namespace.uri().to_owned()));
            }
        }
    }
    let has_elements = node.children().any(|child| child.is_element());
    for child in node.children() {
        if child.is_element() {
            el.children.push(Node::El(from_node(child)));
        } else if child.is_text() {
            let text = child.text().unwrap_or_default();
            // Whitespace between elements is layout of the source, not content; text that is the only
            // thing in an element is content, spaces and all (`w:t`, `w:instrText`, `a:t`).
            if has_elements && text.trim().is_empty() {
                continue;
            }
            el.children.push(Node::Text(text.to_owned()));
        }
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_foreign_prefix_for_the_main_namespace_is_read_as_w() {
        let el = parse(&format!(
            r#"<ns0:document xmlns:ns0="{W}"><ns0:body><ns0:p/></ns0:body></ns0:document>"#
        ))
        .unwrap();
        assert_eq!(el.name, "w:document");
        assert_eq!(el.child("w:body").unwrap().child("w:p").unwrap().name, "w:p");
    }

    #[test]
    fn an_unknown_namespace_is_declared_again_where_it_was_declared() {
        let source = format!(
            r#"<w:r xmlns:w="{W}"><x:thing xmlns:x="urn:x" x:a="1"><x:in>hi &amp; bye</x:in></x:thing></w:r>"#
        );
        let el = parse(&source).unwrap();
        assert_eq!(
            el.to_xml(),
            r#"<w:r><x:thing xmlns:x="urn:x" x:a="1"><x:in>hi &amp; bye</x:in></x:thing></w:r>"#
        );
    }

    #[test]
    fn spaces_in_text_are_kept_and_spaces_between_elements_are_not() {
        let el = parse(&format!(
            "<w:r xmlns:w=\"{W}\">\n  <w:t xml:space=\"preserve\">  two  </w:t>\n</w:r>"
        ))
        .unwrap();
        assert_eq!(el.to_xml(), r#"<w:r><w:t xml:space="preserve">  two  </w:t></w:r>"#);
    }

    #[test]
    fn attribute_values_are_escaped_so_they_read_back_the_same() {
        let el = El::new("w:x").with("w:val", "a\"b<c&d\ne");
        let back = parse(&format!(r#"<w:root xmlns:w="{W}">{}</w:root>"#, el.to_xml())).unwrap();
        assert_eq!(back.child("w:x").unwrap().attr("w:val"), Some("a\"b<c&d\ne"));
    }

    #[test]
    fn utf16_and_a_byte_order_mark_are_read() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "<a>é</a>".encode_utf16() {
            bytes.extend(unit.to_le_bytes());
        }
        assert_eq!(parse_bytes(&bytes).unwrap().text(), "é");
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend(b"<a>x</a>");
        assert_eq!(parse_bytes(&bom).unwrap().text(), "x");
    }
}
