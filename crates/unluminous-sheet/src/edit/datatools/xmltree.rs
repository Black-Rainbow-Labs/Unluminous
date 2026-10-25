//! A small XML tree for the parts of a table that are edited in place.
//!
//! A table part is read, changed and written back with everything it holds that Excel put there kept: the
//! namespace declarations, the `xr:uid` attributes, the extension list. `roxmltree` drops prefixes, so this
//! reads the text itself. Attribute values and text are kept as the file wrote them (escaped), and the
//! setters escape what they are given. The parser accepts what Excel writes, which is plain well formed XML
//! with no doctype.

/// One element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Element {
    /// The name as written, with its prefix.
    pub name: String,
    /// Attributes in file order, values escaped.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

/// A child of an element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Node {
    Element(Element),
    /// Text between elements, escaped as written.
    Text(String),
}

/// A document: the prolog (the `<?xml ...?>` line, kept) and the root element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Document {
    pub prolog: String,
    pub root: Element,
}

impl Element {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_owned(), attrs: Vec::new(), children: Vec::new() }
    }

    /// The unescaped value of an attribute.
    pub fn attr(&self, name: &str) -> Option<String> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| unescape(v))
    }

    /// Set an attribute, keeping its place when it exists and appending it otherwise.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        let escaped = escape(value);
        match self.attrs.iter_mut().find(|(k, _)| k == name) {
            Some((_, v)) => *v = escaped,
            None => self.attrs.push((name.to_owned(), escaped)),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(k, _)| k != name);
    }

    /// The first child element with this name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Element> {
        self.children.iter_mut().find_map(|n| match n {
            Node::Element(e) if e.name == name => Some(e),
            _ => None,
        })
    }

    /// Every child element, in order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// Remove every child element with this name.
    pub fn remove_children(&mut self, name: &str) {
        self.children.retain(|n| !matches!(n, Node::Element(e) if e.name == name));
    }

    /// Put an element in the place the schema gives it: after the last child whose name is in `before`
    /// (the elements that must come first), replacing an element of the same name when there is one.
    pub fn put_child(&mut self, element: Element, before: &[&str]) {
        self.remove_children(&element.name);
        let at = self
            .children
            .iter()
            .rposition(|n| matches!(n, Node::Element(e) if before.contains(&e.name.as_str())))
            .map_or(0, |i| i + 1);
        self.children.insert(at, Node::Element(element));
    }

    /// The text directly inside, unescaped.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Text(t) => Some(unescape(t)),
                Node::Element(_) => None,
            })
            .collect()
    }

    pub fn set_text(&mut self, text: &str) {
        self.children = vec![Node::Text(escape(text))];
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs {
            out.push_str(&format!(" {k}=\"{v}\""));
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            match c {
                Node::Element(e) => e.write(out),
                Node::Text(t) => out.push_str(t),
            }
        }
        out.push_str(&format!("</{}>", self.name));
    }
}

impl Document {
    /// Read a part. `None` when it is not well formed.
    pub fn parse(xml: &str) -> Option<Document> {
        let mut p = Parser { text: xml.as_bytes(), src: xml, at: 0 };
        let prolog = p.prolog();
        let root = p.element()?;
        Some(Document { prolog, root })
    }

    /// The part as text.
    pub fn to_xml(&self) -> String {
        let mut out = self.prolog.clone();
        self.root.write(&mut out);
        out
    }
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let replacement = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match replacement {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

struct Parser<'a> {
    text: &'a [u8],
    src: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn prolog(&mut self) -> String {
        let start = self.at;
        loop {
            self.skip_space();
            if self.starts("<?") {
                match self.src[self.at..].find("?>") {
                    Some(i) => self.at += i + 2,
                    None => break,
                }
            } else if self.starts("<!--") {
                match self.src[self.at..].find("-->") {
                    Some(i) => self.at += i + 3,
                    None => break,
                }
            } else {
                break;
            }
        }
        self.src[start..self.at].to_owned()
    }

    fn starts(&self, s: &str) -> bool {
        self.src[self.at..].starts_with(s)
    }

    fn skip_space(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.at;
        while self.at < self.text.len()
            && !self.text[self.at].is_ascii_whitespace()
            && !matches!(self.text[self.at], b'=' | b'>' | b'/')
        {
            self.at += 1;
        }
        (self.at > start).then(|| self.src[start..self.at].to_owned())
    }

    fn element(&mut self) -> Option<Element> {
        if !self.starts("<") {
            return None;
        }
        self.at += 1;
        let mut e = Element::new(&self.name()?);
        loop {
            self.skip_space();
            if self.starts("/>") {
                self.at += 2;
                return Some(e);
            }
            if self.starts(">") {
                self.at += 1;
                break;
            }
            let key = self.name()?;
            self.skip_space();
            if !self.starts("=") {
                return None;
            }
            self.at += 1;
            self.skip_space();
            let quote = *self.text.get(self.at)?;
            if quote != b'"' && quote != b'\'' {
                return None;
            }
            self.at += 1;
            let start = self.at;
            while *self.text.get(self.at)? != quote {
                self.at += 1;
            }
            e.attrs.push((key, self.src[start..self.at].to_owned()));
            self.at += 1;
        }
        self.children(&mut e)?;
        Some(e)
    }

    fn children(&mut self, e: &mut Element) -> Option<()> {
        loop {
            if self.starts("</") {
                let end = self.src[self.at..].find('>')? + self.at;
                self.at = end + 1;
                return Some(());
            }
            if self.starts("<!--") {
                self.at += self.src[self.at..].find("-->")? + 3;
            } else if self.starts("<![CDATA[") {
                let start = self.at + 9;
                let end = self.src[start..].find("]]>")? + start;
                e.children.push(Node::Text(escape(&self.src[start..end])));
                self.at = end + 3;
            } else if self.starts("<") {
                e.children.push(Node::Element(self.element()?));
            } else {
                let end = self.src[self.at..].find('<').map_or(self.src.len(), |i| i + self.at);
                e.children.push(Node::Text(self.src[self.at..end].to_owned()));
                self.at = end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_part_reads_and_writes_back_the_same_text() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<table xmlns=\"http://x\" xmlns:xr=\"http://y\" id=\"1\" xr:uid=\"{A}\" name=\"T\" ref=\"A1:B2\"><autoFilter ref=\"A1:B2\"/><tableColumns count=\"1\"><tableColumn id=\"1\" name=\"a &amp; b\"><calculatedColumnFormula>[@x]*2</calculatedColumnFormula></tableColumn></tableColumns></table>";
        let doc = Document::parse(xml).unwrap();
        assert_eq!(doc.to_xml(), xml);
        let col = doc.root.child("tableColumns").unwrap().elements().next().unwrap();
        assert_eq!(col.attr("name").unwrap(), "a & b");
        assert_eq!(col.child("calculatedColumnFormula").unwrap().text(), "[@x]*2");
    }

    #[test]
    fn a_child_goes_where_the_schema_puts_it() {
        let mut doc =
            Document::parse("<t><autoFilter/><tableColumns/><tableStyleInfo/></t>").unwrap();
        doc.root.put_child(Element::new("sortState"), &["autoFilter"]);
        assert_eq!(
            doc.to_xml(),
            "<t><autoFilter/><sortState/><tableColumns/><tableStyleInfo/></t>"
        );
    }
}
