//! The web functions that work on text alone: ENCODEURL and FILTERXML. WEBSERVICE needs the network and is
//! one of the functions that need Excel's service (`services.rs`).

use super::xpath::{select, XNode};
use super::{r, s, text_of, Function};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s(
        "ENCODEURL",
        1,
        1,
        "text",
        "Text with the characters a web address cannot hold written as %XX.",
        |_, a| Ok(Value::text(encode_url(&text_of(&a[0])?))),
    ),
    r("FILTERXML", 2, 2, "xml, xpath", "The parts of an XML text a path selects.", filter_xml),
];

/// The text as percent encoded UTF-8: letters, digits, `-`, `_` and `.` stay and every other byte is `%XX`.
fn encode_url(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// FILTERXML over arguments that may be ranges: one answer for each pair of XML and path, the first value
/// of it when the arguments are arrays.
fn filter_xml(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let vals: Vec<Val> = args.iter().map(|a| ctx.eval(a)).collect();
    if vals.iter().all(|v| ctx.dims(v) == Some((1, 1))) {
        let values: Vec<Value> = vals.iter().map(|v| ctx.element(v, 0, 0)).collect();
        return match run(&values) {
            Ok(array) if array.data.len() == 1 => Val::Value(array.data[0].clone()),
            Ok(array) => Val::Array(array),
            Err(e) => Val::error(e),
        };
    }
    ctx.lift_vals(vals, |_, values| match run(values) {
        Ok(array) => array.data.first().cloned().unwrap_or(Value::Error(ErrorKind::Value)),
        Err(e) => Value::Error(e),
    })
}

/// The values a path selects in an XML text, as a column.
fn run(a: &[Value]) -> Result<Array, ErrorKind> {
    let xml = text_of(&a[0])?;
    let path = text_of(&a[1])?;
    let doc = roxmltree::Document::parse(&xml).map_err(|_| ErrorKind::Value)?;
    let nodes = select(doc.root(), &path).map_err(|_| ErrorKind::Value)?;
    if nodes.is_empty() {
        return Err(ErrorKind::Value);
    }
    Ok(Array::column(nodes.iter().map(node_value).collect()))
}

/// The value of a selected node. An element is the text directly inside it (the text of the elements inside
/// it is not part of it) and an attribute its value, each read as typed into a cell, so `1` is a number and
/// `TRUE` a logical. A node with no text, and a text node, is `#VALUE!`.
fn node_value(node: &XNode) -> Value {
    let text = match node {
        XNode::Attr(..) => node.string_value(),
        XNode::Node(n) if n.is_element() => {
            n.children().filter(|c| c.is_text()).filter_map(|c| c.text()).collect()
        }
        XNode::Node(_) => String::new(),
    };
    let trimmed = text.trim();
    if trimmed.is_empty() || matches!(node, XNode::Node(n) if n.is_text()) {
        return Value::Error(ErrorKind::Value);
    }
    crate::input::read(trimmed, false, 2026).value
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{check, eval};
    use crate::value::{ErrorKind, Value};

    #[test]
    fn encodeurl_keeps_unreserved_characters_and_writes_the_rest_in_hex() {
        check(&[
            ("ENCODEURL(\"a b\")", "a%20b"),
            ("ENCODEURL(\"-_.~\")", "-_.%7E"),
            ("ENCODEURL(\"é\")", "%C3%A9"),
            ("ENCODEURL(\"a+b&c=d\")", "a%2Bb%26c%3Dd"),
        ]);
    }

    #[test]
    fn filterxml_gives_the_text_of_the_nodes_a_path_selects() {
        check(&[
            ("FILTERXML(\"<a><b>1</b><b>2</b></a>\",\"//b[2]\")", "2"),
            ("FILTERXML(\"<a><b id='x'>1</b></a>\",\"//b/@id\")", "x"),
            ("FILTERXML(\"<a><b>x<c>y</c>z</b></a>\",\"//b\")", "xz"),
            ("ROWS(FILTERXML(\"<a><b>1</b><b>2</b></a>\",\"//b\"))", "2"),
        ]);
        assert_eq!(eval("FILTERXML(\"<a><b>1</b></a>\",\"//c\")"), Value::Error(ErrorKind::Value));
        assert_eq!(
            eval("FILTERXML(\"<a><b>1</b></a>\",\"count(//b)\")"),
            Value::Error(ErrorKind::Value)
        );
    }
}
