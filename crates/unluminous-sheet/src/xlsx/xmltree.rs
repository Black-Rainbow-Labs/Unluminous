//! A small XML tree that writes back what it read: element and attribute names keep their prefixes, attributes
//! keep their order, and text is kept as it was.
//!
//! A drawing holds shapes whose properties this version does not model (effects, gradients, text styles).
//! Reading one into this tree, changing the few things a person changed and writing the tree back keeps
//! everything else Excel wrote. `roxmltree` does not do this because it resolves prefixes away.

use super::escape;

/// One node of a tree.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Elem(Elem),
    Text(String),
}

/// An element: its qualified name, attributes (namespace declarations included) and children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Elem {
    /// The name as written, with its prefix: `xdr:twoCellAnchor`.
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Elem {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_owned(), ..Default::default() }
    }

    /// The name without its prefix.
    pub fn local(&self) -> &str {
        local_name(&self.name)
    }

    /// The prefix of the name, empty when it has none.
    pub fn prefix(&self) -> &str {
        self.name.split_once(':').map_or("", |(p, _)| p)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The attribute with this local name whatever its prefix (`r:embed` is found by `embed`).
    pub fn attr_local(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| local_name(n) == name).map(|(_, v)| v.as_str())
    }

    pub fn set_attr(&mut self, name: &str, value: &str) {
        match self.attrs.iter_mut().find(|(n, _)| n == name) {
            Some((_, v)) => *v = value.to_owned(),
            None => self.attrs.push((name.to_owned(), value.to_owned())),
        }
    }

    pub fn remove_attr(&mut self, name: &str) {
        self.attrs.retain(|(n, _)| n != name);
    }

    /// The first child element with this local name.
    pub fn child(&self, local: &str) -> Option<&Elem> {
        self.elems().find(|e| e.local() == local)
    }

    pub fn child_mut(&mut self, local: &str) -> Option<&mut Elem> {
        self.children.iter_mut().find_map(|n| match n {
            Node::Elem(e) if e.local() == local => Some(e),
            _ => None,
        })
    }

    /// The child elements.
    pub fn elems(&self) -> impl Iterator<Item = &Elem> {
        self.children.iter().filter_map(|n| match n {
            Node::Elem(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn elems_mut(&mut self) -> impl Iterator<Item = &mut Elem> {
        self.children.iter_mut().filter_map(|n| match n {
            Node::Elem(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The text directly inside the element.
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|n| match n {
                Node::Text(t) => Some(t.as_str()),
                Node::Elem(_) => None,
            })
            .collect()
    }

    pub fn set_text(&mut self, text: &str) {
        self.children = vec![Node::Text(text.to_owned())];
    }

    /// The first descendant (this element excluded) with this local name.
    pub fn find(&self, local: &str) -> Option<&Elem> {
        for e in self.elems() {
            if e.local() == local {
                return Some(e);
            }
            if let Some(found) = e.find(local) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_mut(&mut self, local: &str) -> Option<&mut Elem> {
        for e in self.elems_mut() {
            if e.local() == local {
                return Some(e);
            }
            if let Some(found) = e.find_mut(local) {
                return Some(found);
            }
        }
        None
    }

    /// Every descendant with this local name, depth first.
    pub fn find_all<'a>(&'a self, local: &str, out: &mut Vec<&'a Elem>) {
        for e in self.elems() {
            if e.local() == local {
                out.push(e);
            }
            e.find_all(local, out);
        }
    }

    pub fn remove_children(&mut self, local: &str) {
        self.children.retain(|n| !matches!(n, Node::Elem(e) if e.local() == local));
    }

    pub fn push(&mut self, child: Elem) {
        self.children.push(Node::Elem(child));
    }

    /// Put `child` among the children where the schema's sequence `order` (local names) says, replacing a
    /// child of the same name. A child whose name `order` does not list goes last.
    pub fn put_in_order(&mut self, child: Elem, order: &[&str]) {
        self.remove_children(child.local());
        let rank = |name: &str| order.iter().position(|o| *o == name).unwrap_or(order.len());
        let mine = rank(child.local());
        let at = self
            .children
            .iter()
            .position(|n| matches!(n, Node::Elem(e) if rank(e.local()) > mine))
            .unwrap_or(self.children.len());
        self.children.insert(at, Node::Elem(child));
    }

    /// The tree as XML text, with no declaration.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (n, v) in &self.attrs {
            out.push(' ');
            out.push_str(n);
            out.push_str("=\"");
            out.push_str(&escape(v));
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            match c {
                Node::Elem(e) => e.write(out),
                Node::Text(t) => out.push_str(&escape_text(t)),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
}

/// The name without a prefix.
pub fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            e if e.starts_with("#x") => {
                if let Some(c) = u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            e if e.starts_with('#') => {
                if let Some(c) = e[1..].parse::<u32>().ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            _ => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Read one element and everything inside it. Comments and processing instructions are dropped.
pub fn parse(xml: &str) -> Option<Elem> {
    let mut p = Parser { s: xml.as_bytes(), text: xml, at: 0 };
    p.skip_prolog();
    p.element()
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn skip_prolog(&mut self) {
        loop {
            self.skip_space();
            if self.text[self.at..].starts_with("<?") || self.text[self.at..].starts_with("<!--") {
                let end = if self.text[self.at..].starts_with("<?") { "?>" } else { "-->" };
                match self.text[self.at..].find(end) {
                    Some(i) => self.at += i + end.len(),
                    None => return,
                }
            } else {
                return;
            }
        }
    }

    fn skip_space(&mut self) {
        while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn name(&mut self) -> String {
        let start = self.at;
        while self.at < self.s.len()
            && !self.s[self.at].is_ascii_whitespace()
            && !matches!(self.s[self.at], b'=' | b'>' | b'/')
        {
            self.at += 1;
        }
        self.text[start..self.at].to_owned()
    }

    fn element(&mut self) -> Option<Elem> {
        if self.s.get(self.at) != Some(&b'<') {
            return None;
        }
        self.at += 1;
        let mut elem = Elem::new(&self.name());
        loop {
            self.skip_space();
            match self.s.get(self.at)? {
                b'/' => {
                    self.at += 2;
                    return Some(elem);
                }
                b'>' => {
                    self.at += 1;
                    break;
                }
                _ => {
                    let name = self.name();
                    self.skip_space();
                    if self.s.get(self.at) != Some(&b'=') {
                        return None;
                    }
                    self.at += 1;
                    self.skip_space();
                    let quote = *self.s.get(self.at)?;
                    self.at += 1;
                    let end = self.text[self.at..].find(quote as char)? + self.at;
                    elem.attrs.push((name, unescape(&self.text[self.at..end])));
                    self.at = end + 1;
                }
            }
        }
        self.content(&mut elem)?;
        Some(elem)
    }

    fn content(&mut self, elem: &mut Elem) -> Option<()> {
        loop {
            let rest = &self.text[self.at..];
            if rest.starts_with("</") {
                let end = rest.find('>')?;
                self.at += end + 1;
                return Some(());
            }
            if rest.starts_with("<!--") {
                self.at += rest.find("-->")? + 3;
            } else if rest.starts_with("<![CDATA[") {
                let end = rest.find("]]>")?;
                elem.children.push(Node::Text(rest[9..end].to_owned()));
                self.at += end + 3;
            } else if rest.starts_with("<?") {
                self.at += rest.find("?>")? + 2;
            } else if rest.starts_with('<') {
                elem.children.push(Node::Elem(self.element()?));
            } else {
                let end = rest.find('<')?;
                let text = unescape(&rest[..end]);
                // Whitespace between elements is layout, not content, unless the element holds only it.
                let only_space = text.trim().is_empty();
                let closes = rest[end..].starts_with("</");
                if !only_space || (elem.children.is_empty() && closes) {
                    elem.children.push(Node::Text(text));
                }
                self.at += end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_writes_back_what_it_read() {
        let xml = "<a:p xmlns:a=\"u\" x=\"1 &amp; 2\"><a:r><a:t>Hi &lt;there&gt;</a:t></a:r><a:e/></a:p>";
        assert_eq!(parse(xml).unwrap().to_xml(), xml);
    }

    #[test]
    fn an_element_is_put_where_the_schema_says() {
        let mut e = parse("<s><a/><c/></s>").unwrap();
        e.put_in_order(Elem::new("b"), &["a", "b", "c"]);
        assert_eq!(e.to_xml(), "<s><a/><b/><c/></s>");
        e.put_in_order(Elem::new("a"), &["a", "b", "c"]);
        assert_eq!(e.to_xml(), "<s><a/><b/><c/></s>");
    }

    #[test]
    fn text_that_is_only_spaces_is_kept_inside_a_text_element() {
        let e = parse("<a:t xml:space=\"preserve\"> </a:t>").unwrap();
        assert_eq!(e.text(), " ");
    }
}
