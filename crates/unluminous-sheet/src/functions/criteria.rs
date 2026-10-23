//! The criteria COUNTIF, SUMIF, AVERAGEIF, their plural forms and the database functions share:
//! `5`, `">=10"`, `"<>"`, `"a*"`, `"=apple"`.

use std::cmp::Ordering;

use crate::eval::{compare_text, to_number};
use crate::value::{ErrorKind, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum Target {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(ErrorKind),
    /// `"="` or `""`: an empty cell.
    Empty,
}

/// One criterion, read once and matched against many cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Criterion {
    op: Op,
    target: Target,
    wildcard: bool,
}

impl Criterion {
    pub fn parse(v: &Value, date1904: bool) -> Self {
        match v {
            Value::Number(n) => Self { op: Op::Eq, target: Target::Number(*n), wildcard: false },
            Value::Bool(b) => Self { op: Op::Eq, target: Target::Bool(*b), wildcard: false },
            Value::Error(e) => Self { op: Op::Eq, target: Target::Error(*e), wildcard: false },
            Value::Empty => Self { op: Op::Eq, target: Target::Number(0.0), wildcard: false },
            Value::Text(t) => {
                let text = &**t;
                let (op, rest) = if let Some(r) = text.strip_prefix("<>") {
                    (Op::Ne, r)
                } else if let Some(r) = text.strip_prefix("<=") {
                    (Op::Le, r)
                } else if let Some(r) = text.strip_prefix(">=") {
                    (Op::Ge, r)
                } else if let Some(r) = text.strip_prefix('<') {
                    (Op::Lt, r)
                } else if let Some(r) = text.strip_prefix('>') {
                    (Op::Gt, r)
                } else if let Some(r) = text.strip_prefix('=') {
                    (Op::Eq, r)
                } else {
                    (Op::Eq, text)
                };
                if rest.is_empty() {
                    return Self { op, target: Target::Empty, wildcard: false };
                }
                let upper = rest.to_ascii_uppercase();
                let target = if upper == "TRUE" {
                    Target::Bool(true)
                } else if upper == "FALSE" {
                    Target::Bool(false)
                } else if let Some(e) = ErrorKind::parse(rest) {
                    Target::Error(e)
                } else if let Some(n) = crate::input::number_from_text(rest, date1904) {
                    Target::Number(n)
                } else {
                    Target::Text(rest.to_owned())
                };
                let wildcard = matches!(target, Target::Text(_)) && matches!(op, Op::Eq | Op::Ne) && rest.contains(['*', '?', '~']);
                Self { op, target, wildcard }
            }
        }
    }

    /// Whether a cell's value meets the criterion.
    pub fn matches(&self, cell: &Value) -> bool {
        let hit = match (&self.target, cell) {
            (Target::Empty, Value::Empty) => Some(Ordering::Equal),
            (Target::Empty, Value::Text(t)) if t.is_empty() => Some(Ordering::Equal),
            (Target::Empty, _) => None,
            (Target::Number(n), Value::Number(c)) => c.partial_cmp(n),
            (Target::Number(n), Value::Text(t)) if self.op == Op::Eq => {
                to_number(&Value::Text(t.clone()), false).ok().and_then(|c| c.partial_cmp(n))
            }
            (Target::Text(text), Value::Text(c)) => Some(if self.wildcard {
                if wildcard_match(text, c) {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }
            } else {
                compare_text(c, text)
            }),
            (Target::Text(_), Value::Empty) if self.op == Op::Eq => None,
            (Target::Bool(b), Value::Bool(c)) => Some(c.cmp(b)),
            (Target::Error(e), Value::Error(c)) => Some(if e == c { Ordering::Equal } else { Ordering::Less }),
            _ => None,
        };
        match (self.op, hit) {
            (Op::Ne, None) => true,
            (_, None) => false,
            (Op::Eq, Some(o)) => o == Ordering::Equal,
            (Op::Ne, Some(o)) => o != Ordering::Equal,
            (Op::Lt, Some(o)) => o == Ordering::Less,
            (Op::Le, Some(o)) => o != Ordering::Greater,
            (Op::Gt, Some(o)) => o == Ordering::Greater,
            (Op::Ge, Some(o)) => o != Ordering::Less,
        }
    }
}

/// `*` any run, `?` one character, `~` makes the next one literal. Without regard to case.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    // Tokens: Some(c) a literal, None a star, Some('\0')... use an enum.
    #[derive(Clone, Copy, PartialEq)]
    enum Tok {
        Lit(char),
        One,
        Any,
    }
    let mut toks = Vec::new();
    let mut i = 0;
    while i < p.len() {
        match p[i] {
            '~' if i + 1 < p.len() => {
                toks.push(Tok::Lit(p[i + 1]));
                i += 2;
                continue;
            }
            '*' => toks.push(Tok::Any),
            '?' => toks.push(Tok::One),
            c => toks.push(Tok::Lit(c)),
        }
        i += 1;
    }
    let (mut ti, mut pi) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < toks.len() && (toks[pi] == Tok::One || toks[pi] == Tok::Lit(t[ti])) {
            ti += 1;
            pi += 1;
        } else if pi < toks.len() && toks[pi] == Tok::Any {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < toks.len() && toks[pi] == Tok::Any {
        pi += 1;
    }
    pi == toks.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(criterion: Value, cell: Value) -> bool {
        Criterion::parse(&criterion, false).matches(&cell)
    }

    #[test]
    fn criteria_compare_the_way_countif_does() {
        assert!(m(Value::Number(5.0), Value::Number(5.0)));
        assert!(m(Value::text(">=10"), Value::Number(10.0)));
        assert!(!m(Value::text(">=10"), Value::text("20")));
        assert!(m(Value::text("<>"), Value::Number(1.0)));
        assert!(!m(Value::text("<>"), Value::Empty));
        assert!(m(Value::text("="), Value::Empty));
        assert!(m(Value::text("ap*"), Value::text("Apple")));
        assert!(m(Value::text("a?ple"), Value::text("apple")));
        assert!(!m(Value::text("ap*"), Value::text("grape")));
        assert!(m(Value::text("<>ap*"), Value::text("grape")));
        assert!(m(Value::text("<>ap*"), Value::Empty));
        assert!(m(Value::text("~*"), Value::text("*")));
        assert!(m(Value::text(">b"), Value::text("c")));
        assert!(m(Value::text("TRUE"), Value::Bool(true)));
    }
}
