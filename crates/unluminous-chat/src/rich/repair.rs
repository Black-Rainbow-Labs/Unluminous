//! Closing a JSON document that was cut off, so a component can be drawn while it is still arriving.
//!
//! The OpenAI page this ticket starts from describes "a compiler that processes the interface as the
//! model generates it", and this is the part of that a client can do: take whatever prefix of the JSON
//! has arrived and turn it into the largest complete document it implies. The rules:
//!
//! - a string **value** cut off is closed, so a title appears letter by letter;
//! - a **key** cut off, or a key with no value yet, is dropped with the comma before it;
//! - a number cut after its `-`, `.` or `e` is cut back to the last digit;
//! - `t`, `tr`, `f`, `nul` and the rest are completed, since each can only become one literal;
//! - every open `[` and `{` is closed, innermost first.
//!
//! **A component only grows.** Each rule above answers with a document that the full text extends,
//! so the list a component reads from a prefix is never longer than the one it reads from a longer
//! prefix. That is what keeps a table from losing a row it drew a moment ago, and
//! `a_component_only_grows_as_its_text_arrives` in the catalogue checks it at every byte of every
//! example.

/// The largest complete JSON document `text` is the start of, or an empty string when there is none.
pub fn repair(text: &str) -> String {
    let mut scan =
        Scan { bytes: text.as_bytes(), at: 0, stack: Vec::new(), safe: 0, safe_stack: Vec::new() };
    match scan.document() {
        End::Complete => text[..scan.at].to_owned(),
        End::InValueString { upto } => {
            let mut out = text[..upto].to_owned();
            out.push('"');
            close(&mut out, &scan.stack);
            out
        }
        End::Truncated => {
            let mut out = text[..scan.safe].to_owned();
            close(&mut out, &scan.safe_stack);
            out
        }
        End::Invalid => String::new(),
    }
}

/// Append the closing bracket of every container still open, innermost first.
fn close(out: &mut String, stack: &[Container]) {
    for container in stack.iter().rev() {
        out.push(match container {
            Container::Object => '}',
            Container::Array => ']',
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Container {
    Object,
    Array,
}

/// How the scan ended.
enum End {
    /// The whole document was there.
    Complete,
    /// It ended inside a string that is a value, which can be closed where the text stops.
    InValueString { upto: usize },
    /// It ended somewhere else; what can be kept is up to the last safe point.
    Truncated,
    /// What arrived is not the start of any JSON document.
    Invalid,
}

/// A single pass over the bytes, remembering the last point at which closing every open container
/// would give a valid document.
struct Scan<'a> {
    bytes: &'a [u8],
    at: usize,
    stack: Vec<Container>,
    safe: usize,
    safe_stack: Vec<Container>,
}

/// What a value turned out to be.
enum Value {
    Done,
    /// A string value cut off; `upto` is where it can be closed.
    OpenString {
        upto: usize,
    },
    Truncated,
    Invalid,
}

impl Scan<'_> {
    /// Read one whole document.
    fn document(&mut self) -> End {
        self.skip_space();
        if self.at >= self.bytes.len() {
            return End::Truncated;
        }
        match self.value() {
            Value::Done => End::Complete,
            Value::OpenString { upto } => End::InValueString { upto },
            Value::Truncated => End::Truncated,
            Value::Invalid => End::Invalid,
        }
    }

    /// Mark the current point as one where closing every container gives a valid document.
    fn mark_safe(&mut self) {
        self.safe = self.at;
        self.safe_stack = self.stack.clone();
    }

    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Read one value of any kind.
    fn value(&mut self) -> Value {
        self.skip_space();
        match self.peek() {
            None => Value::Truncated,
            Some(b'{') => self.container(Container::Object),
            Some(b'[') => self.container(Container::Array),
            Some(b'"') => match self.string() {
                Ok(()) => Value::Done,
                Err(Some(upto)) => Value::OpenString { upto },
                Err(None) => Value::Truncated,
            },
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Value::Invalid,
        }
    }

    /// Read an object or an array, from its opening bracket.
    fn container(&mut self, kind: Container) -> Value {
        self.at += 1;
        self.stack.push(kind);
        self.mark_safe();
        let closer = match kind {
            Container::Object => b'}',
            Container::Array => b']',
        };
        let mut first = true;
        loop {
            self.skip_space();
            match self.peek() {
                None => return Value::Truncated,
                Some(c) if c == closer => {
                    self.at += 1;
                    self.stack.pop();
                    self.mark_safe();
                    return Value::Done;
                }
                Some(b',') if !first => {
                    self.at += 1;
                    self.skip_space();
                }
                Some(_) if first => {}
                Some(_) => return Value::Invalid,
            }
            first = false;
            if kind == Container::Object {
                self.skip_space();
                match self.peek() {
                    None => return Value::Truncated,
                    Some(b'"') => {}
                    // A trailing comma before the brace is what an answer cut after `,` then `}`
                    // looks like nowhere; it is simply not JSON.
                    Some(_) => return Value::Invalid,
                }
                if self.string().is_err() {
                    return Value::Truncated;
                }
                self.skip_space();
                match self.peek() {
                    None => return Value::Truncated,
                    Some(b':') => self.at += 1,
                    Some(_) => return Value::Invalid,
                }
            }
            match self.value() {
                Value::Done => self.mark_safe(),
                other => return other,
            }
        }
    }

    /// Read a string, from its opening quote.
    ///
    /// `Err(Some(upto))` is a string cut off, with `upto` the point at which a closing quote makes it a
    /// valid string: a backslash with nothing after it, or a `\u` escape with fewer than four digits,
    /// is left out.
    fn string(&mut self) -> Result<(), Option<usize>> {
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(Some(self.at)),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    let escape = self.at;
                    match self.bytes.get(self.at + 1) {
                        None => {
                            self.at = self.bytes.len();
                            return Err(Some(escape));
                        }
                        Some(b'u') => {
                            let digits = &self.bytes[self.at + 2..];
                            let have =
                                digits.iter().take(4).take_while(|c| c.is_ascii_hexdigit()).count();
                            if have < 4 {
                                // Cut off inside the escape, which a closing quote cannot finish; or a
                                // `\u` that is not an escape at all, which nothing can.
                                return match self.at + 2 + have >= self.bytes.len() {
                                    true => {
                                        self.at = self.bytes.len();
                                        Err(Some(escape))
                                    }
                                    false => Err(None),
                                };
                            }
                            self.at += 6;
                        }
                        Some(_) => self.at += 2,
                    }
                }
                Some(_) => self.at += 1,
            }
        }
    }

    /// Read `true`, `false` or `null`, completing a prefix of one that the text ends on.
    fn literal(&mut self, word: &[u8]) -> Value {
        let rest = &self.bytes[self.at..];
        let have = rest.iter().zip(word).take_while(|(a, b)| a == b).count();
        if have == word.len() {
            self.at += have;
            return Value::Done;
        }
        if self.at + have == self.bytes.len() {
            // A prefix of exactly one literal, at the end of the text. Only the literal can follow,
            // so the value is as good as there.
            self.at = self.bytes.len();
            return Value::Truncated;
        }
        Value::Invalid
    }

    /// Read a number, keeping the longest prefix of it that is a number when the text ends inside one.
    fn number(&mut self) -> Value {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E') {
                self.at += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or("");
        let complete = text.parse::<f64>().is_ok() && text.ends_with(|c: char| c.is_ascii_digit());
        if self.at == self.bytes.len() {
            // The text ends inside the number. A number that already reads is kept as it is: more
            // digits can only follow it, which changes the value but never the shape.
            return match complete {
                true => Value::Done,
                false => Value::Truncated,
            };
        }
        match complete {
            true => Value::Done,
            false => Value::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::repair;

    fn reads(text: &str) -> serde_json::Value {
        let repaired = repair(text);
        serde_json::from_str(&repaired).unwrap_or_else(|e| panic!("{text:?} -> {repaired:?}: {e}"))
    }

    #[test]
    fn a_whole_document_is_left_as_it_is() {
        assert_eq!(repair("{\"a\": [1, 2]}"), "{\"a\": [1, 2]}");
    }

    #[test]
    fn a_string_value_cut_off_is_closed_so_it_appears_letter_by_letter() {
        assert_eq!(reads("{\"title\": \"Rel"), serde_json::json!({"title": "Rel"}));
        assert_eq!(reads("[\"a\", \"b"), serde_json::json!(["a", "b"]));
    }

    #[test]
    fn a_key_with_no_value_yet_is_dropped_with_its_comma() {
        assert_eq!(reads("{\"a\": 1, \"ti"), serde_json::json!({"a": 1}));
        assert_eq!(reads("{\"a\": 1, \"title\""), serde_json::json!({"a": 1}));
        assert_eq!(reads("{\"a\": 1, \"title\":"), serde_json::json!({"a": 1}));
        assert_eq!(reads("{\"a\": 1,"), serde_json::json!({"a": 1}));
    }

    #[test]
    fn a_number_or_a_literal_cut_off_keeps_what_it_can() {
        assert_eq!(reads("[1, 2, -"), serde_json::json!([1, 2]));
        assert_eq!(reads("[1, 2.5"), serde_json::json!([1, 2.5]));
        assert_eq!(reads("[1, 2."), serde_json::json!([1]));
        assert_eq!(reads("[true, fa"), serde_json::json!([true]));
    }

    #[test]
    fn an_escape_cut_in_half_is_left_out() {
        assert_eq!(reads("[\"a\\"), serde_json::json!(["a"]));
        assert_eq!(reads("[\"a\\u00"), serde_json::json!(["a"]));
        assert_eq!(reads("[\"a\\n"), serde_json::json!(["a\n"]));
    }

    #[test]
    fn nested_containers_are_closed_innermost_first() {
        assert_eq!(
            reads("{\"series\": [{\"name\": \"x\", \"values\": [1, 2"),
            serde_json::json!({"series": [{"name": "x", "values": [1, 2]}]})
        );
    }

    #[test]
    fn nothing_yet_and_nonsense_both_answer_empty() {
        assert_eq!(repair(""), "");
        assert_eq!(repair("   "), "");
        assert_eq!(repair("hello"), "");
    }
}
