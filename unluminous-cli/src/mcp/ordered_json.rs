//! A JSON document whose objects keep their keys in the order the file had them.
//!
//! `serde_json::Value` keeps an object's keys sorted, because the workspace builds `serde_json`
//! without `preserve_order` and turning that on would change the order of every object Unluminous
//! writes. So an edit through `Value` gave a person's `opencode.json` back with `$schema` moved from
//! the top to the middle and every other key alphabetised (`task-2278`). The content was the same and
//! the file was not the one they wrote.
//!
//! This keeps exactly what an edit of one entry needs: objects as ordered lists of pairs, arrays, and
//! every other value as the text it was written as, so a number such as `120` is written back as
//! `120`. The file is laid out again with two spaces of indent, which is how Claude Code and OpenCode
//! both write their own files. Comments are not JSON and are refused by the reader, as they are by
//! `serde_json`.

use serde_json::Value;

/// One value.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Object(Vec<(String, Node)>),
    Array(Vec<Node>),
    /// A string, number, `true`, `false` or `null`, as the JSON text it was written as.
    Scalar(String),
}

impl Node {
    /// Read a document. The text has to be one JSON value with nothing after it.
    pub fn parse(text: &str) -> Result<Node, String> {
        // `serde_json` decides whether the text is JSON at all, so the two readers cannot disagree
        // about what is valid. This reader then only has to find where each value starts and ends.
        serde_json::from_str::<Value>(text).map_err(|problem| problem.to_string())?;
        let mut reader = Reader { bytes: text.as_bytes(), at: 0 };
        reader.value()
    }

    /// The document laid out with two spaces of indent and a line break at the end.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    /// The same value as `serde_json` would hold it, for reading a field.
    pub fn to_value(&self) -> Value {
        serde_json::from_str(&self.to_text()).unwrap_or(Value::Null)
    }

    /// A node holding `value`. An object made this way has its keys in `serde_json`'s order, which is
    /// fine for an entry this program writes.
    pub fn from_value(value: &Value) -> Node {
        match value {
            Value::Object(map) => Node::Object(
                map.iter().map(|(key, value)| (key.clone(), Node::from_value(value))).collect(),
            ),
            Value::Array(items) => Node::Array(items.iter().map(Node::from_value).collect()),
            other => Node::Scalar(other.to_string()),
        }
    }

    /// The member called `key`, made as an empty object at the end when there is none, and replaced
    /// by one when it is not an object.
    pub fn object_member(&mut self, key: &str) -> Option<&mut Node> {
        let Node::Object(pairs) = self else {
            return None;
        };
        let at = match pairs.iter().position(|(name, _)| name == key) {
            Some(at) => at,
            None => {
                pairs.push((key.to_owned(), Node::Object(Vec::new())));
                pairs.len() - 1
            }
        };
        if !matches!(pairs[at].1, Node::Object(_)) {
            pairs[at].1 = Node::Object(Vec::new());
        }
        Some(&mut pairs[at].1)
    }

    /// Set `key` to `value`, in the place it already has or at the end, or take it out with `None`.
    pub fn set(&mut self, key: &str, value: Option<Node>) {
        let Node::Object(pairs) = self else {
            return;
        };
        let at = pairs.iter().position(|(name, _)| name == key);
        match (at, value) {
            (Some(at), Some(value)) => pairs[at].1 = value,
            (None, Some(value)) => pairs.push((key.to_owned(), value)),
            (Some(at), None) => {
                pairs.remove(at);
            }
            (None, None) => {}
        }
    }

    fn write(&self, out: &mut String, depth: usize) {
        let indent = |out: &mut String, depth: usize| {
            for _ in 0..depth {
                out.push_str("  ");
            }
        };
        match self {
            Node::Scalar(text) => out.push_str(text),
            Node::Object(pairs) if pairs.is_empty() => out.push_str("{}"),
            Node::Array(items) if items.is_empty() => out.push_str("[]"),
            Node::Object(pairs) => {
                out.push_str("{\n");
                for (index, (key, value)) in pairs.iter().enumerate() {
                    indent(out, depth + 1);
                    out.push_str(&Value::String(key.clone()).to_string());
                    out.push_str(": ");
                    value.write(out, depth + 1);
                    if index + 1 < pairs.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, depth);
                out.push('}');
            }
            Node::Array(items) => {
                out.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                    if index + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, depth);
                out.push(']');
            }
        }
    }
}

/// Walks text that `serde_json` has already said is valid, so it only has to find boundaries.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Result<Node, String> {
        self.skip_space();
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Node::Scalar(self.string_text()?)),
            Some(_) => {
                let start = self.at;
                while self.at < self.bytes.len()
                    && !matches!(self.bytes[self.at], b',' | b'}' | b']')
                    && !self.bytes[self.at].is_ascii_whitespace()
                {
                    self.at += 1;
                }
                Ok(Node::Scalar(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned()))
            }
            None => Err("the text ended where a value was expected".to_owned()),
        }
    }

    /// A string with its quotation marks, exactly as written.
    fn string_text(&mut self) -> Result<String, String> {
        let start = self.at;
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    return Ok(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned());
                }
                _ => self.at += 1,
            }
        }
        Err("a string was not closed".to_owned())
    }

    fn object(&mut self) -> Result<Node, String> {
        self.at += 1;
        let mut pairs = Vec::new();
        loop {
            self.skip_space();
            if self.bytes.get(self.at) == Some(&b'}') {
                self.at += 1;
                return Ok(Node::Object(pairs));
            }
            let key_text = self.string_text()?;
            let key: String =
                serde_json::from_str(&key_text).map_err(|problem| problem.to_string())?;
            self.skip_space();
            self.at += 1; // the colon
            let value = self.value()?;
            pairs.push((key, value));
            self.skip_space();
            if self.bytes.get(self.at) == Some(&b',') {
                self.at += 1;
            }
        }
    }

    fn array(&mut self) -> Result<Node, String> {
        self.at += 1;
        let mut items = Vec::new();
        loop {
            self.skip_space();
            if self.bytes.get(self.at) == Some(&b']') {
                self.at += 1;
                return Ok(Node::Array(items));
            }
            items.push(self.value()?);
            self.skip_space();
            if self.bytes.get(self.at) == Some(&b',') {
                self.at += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_come_back_in_the_order_the_file_had_them() {
        let text = "{\"$schema\": \"x\", \"shell\": \"bash\", \"provider\": {\"z\": 1, \"a\": 120}, \"mcp\": {}}";
        let mut node = Node::parse(text).expect("valid");
        node.object_member("mcp")
            .expect("an object")
            .set("unluminous", Some(Node::Scalar("true".to_owned())));
        let back = node.to_text();
        let order: Vec<usize> =
            ["$schema", "shell", "provider", "\"z\"", "\"a\"", "mcp", "unluminous"]
                .iter()
                .map(|key| back.find(key).expect(key))
                .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{back}");
        assert!(back.contains("\"a\": 120"), "a number is written back as it was: {back}");
    }

    #[test]
    fn a_string_with_escapes_and_a_windows_path_survives() {
        let text = r#"{"command": ["C:\\Program Files\\x.exe", "say \"hi\""], "n": -1.5e3}"#;
        let node = Node::parse(text).expect("valid");
        assert_eq!(node.to_value(), serde_json::from_str::<Value>(text).expect("valid"));
    }

    #[test]
    fn text_that_is_not_json_is_refused() {
        assert!(Node::parse("{ // a comment\n \"a\": 1 }").is_err());
        assert!(Node::parse("{\"a\": 1,}").is_err());
    }

    #[test]
    fn setting_none_takes_a_key_out_and_an_empty_object_stays_compact() {
        let mut node = Node::parse("{\"mcp\": {\"unluminous\": {}}}").expect("valid");
        node.object_member("mcp").expect("object").set("unluminous", None);
        assert_eq!(node.to_text(), "{\n  \"mcp\": {}\n}\n");
    }
}
