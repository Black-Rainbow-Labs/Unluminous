//! Formula text into a tree.
//!
//! A recursive descent parser working on the characters directly. Excel's grammar has two places where a
//! lexer on its own cannot decide: a space is the intersection operator between two references and
//! nothing anywhere else, and `A1`, `LOG10` and `Sheet1` are the same shape until the character after them
//! says whether they are a cell, a function or a sheet. Reading the characters with one look ahead
//! answers both.
//!
//! Precedence, loosest first: comparison, `&`, `+ -`, `* /`, `^`, `%`, unary `-`, then the reference
//! operators `:`, space and `,` inside parentheses. Unary minus binds tighter than `^`, so `-2^2` is 4,
//! which is Excel's rule and not mathematics'.

use std::sync::Arc;

use super::ast::{BinOp, Expr, Point, Reference, Shape, TableRef};
use crate::coord::{column_number, MAX_COLS, MAX_ROWS};
use crate::value::ErrorKind;

/// Parse formula text without its leading `=`.
pub fn parse(text: &str) -> Result<Expr, String> {
    let mut parser = Parser { chars: text.chars().collect(), at: 0, depth: 0 };
    parser.skip_space();
    if parser.at >= parser.chars.len() {
        return Err("the formula is empty".to_owned());
    }
    let expr = parser.comparison()?;
    parser.skip_space();
    if parser.at < parser.chars.len() {
        return Err(format!(
            "there is something after the end of the formula: `{}`",
            parser.chars[parser.at..].iter().collect::<String>()
        ));
    }
    Ok(expr)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    /// How many parentheses deep, where a comma between references is a union.
    depth: usize,
}

const NESTING: usize = 64;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    fn skip_space(&mut self) -> bool {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c == ' ' || c == '\n' || c == '\r' || c == '\t') {
            self.at += 1;
        }
        self.at > start
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char) -> Result<(), String> {
        self.skip_space();
        if self.eat(c) {
            Ok(())
        } else {
            Err(match self.peek() {
                Some(found) => format!("expected `{c}` but found `{found}`"),
                None => format!("expected `{c}` before the end of the formula"),
            })
        }
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        let mut left = self.concat()?;
        loop {
            self.skip_space();
            let op = match (self.peek(), self.peek_at(1)) {
                (Some('<'), Some('>')) => (BinOp::Ne, 2),
                (Some('<'), Some('=')) => (BinOp::Le, 2),
                (Some('>'), Some('=')) => (BinOp::Ge, 2),
                (Some('<'), _) => (BinOp::Lt, 1),
                (Some('>'), _) => (BinOp::Gt, 1),
                (Some('='), _) => (BinOp::Eq, 1),
                _ => return Ok(left),
            };
            self.at += op.1;
            let right = self.concat()?;
            left = Expr::Binary(op.0, Box::new(left), Box::new(right));
        }
    }

    fn concat(&mut self) -> Result<Expr, String> {
        let mut left = self.additive()?;
        loop {
            self.skip_space();
            if !self.eat('&') {
                return Ok(left);
            }
            let right = self.additive()?;
            left = Expr::Binary(BinOp::Concat, Box::new(left), Box::new(right));
        }
    }

    fn additive(&mut self) -> Result<Expr, String> {
        let mut left = self.multiplicative()?;
        loop {
            self.skip_space();
            let op = match self.peek() {
                Some('+') => BinOp::Add,
                Some('-') => BinOp::Sub,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, String> {
        let mut left = self.power()?;
        loop {
            self.skip_space();
            let op = match self.peek() {
                Some('*') => BinOp::Mul,
                Some('/') => BinOp::Div,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.power()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn power(&mut self) -> Result<Expr, String> {
        let mut left = self.percent()?;
        loop {
            self.skip_space();
            if !self.eat('^') {
                return Ok(left);
            }
            let right = self.percent()?;
            left = Expr::Binary(BinOp::Pow, Box::new(left), Box::new(right));
        }
    }

    fn percent(&mut self) -> Result<Expr, String> {
        let mut expr = self.unary()?;
        loop {
            let save = self.at;
            self.skip_space();
            if self.eat('%') {
                expr = Expr::Percent(Box::new(expr));
            } else {
                self.at = save;
                return Ok(expr);
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        self.skip_space();
        if self.eat('-') {
            let inner = self.unary()?;
            return Ok(Expr::Neg(Box::new(inner)));
        }
        if self.eat('+') {
            let inner = self.unary()?;
            return Ok(Expr::Plus(Box::new(inner)));
        }
        if self.eat('@') {
            let inner = self.unary()?;
            return Ok(Expr::Implicit(Box::new(inner)));
        }
        self.intersection()
    }

    /// `A1:C3 B2:D4`. A space is only an operator between two things that can be references.
    fn intersection(&mut self) -> Result<Expr, String> {
        let mut left = self.range()?;
        loop {
            let save = self.at;
            if !self.skip_space() {
                return Ok(left);
            }
            let starts_a_reference = match self.peek() {
                Some(c) => {
                    c.is_alphabetic()
                        || c == '$'
                        || c == '\''
                        || c == '('
                        || c == '_'
                        || c.is_ascii_digit() && is_reference_like(&left)
                }
                None => false,
            };
            if !starts_a_reference || !is_reference_like(&left) {
                self.at = save;
                return Ok(left);
            }
            let right = self.range()?;
            left = Expr::Intersect(Box::new(left), Box::new(right));
        }
    }

    /// `A1:INDEX(B:B,3)`: the range operator between two expressions that are references.
    fn range(&mut self) -> Result<Expr, String> {
        let mut left = self.postfix()?;
        loop {
            if self.peek() != Some(':') {
                return Ok(left);
            }
            self.at += 1;
            let right = self.postfix()?;
            left = Expr::Binary(BinOp::Range, Box::new(left), Box::new(right));
        }
    }

    /// A primary followed by `#` for a spill or by an argument list for a LAMBDA called in place.
    fn postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;
        loop {
            if self.peek() == Some('#')
                && matches!(expr, Expr::Ref(_) | Expr::Name(_) | Expr::Call(..))
            {
                self.at += 1;
                expr = Expr::Spill(Box::new(expr));
                continue;
            }
            if self.peek() == Some('(') && is_callable(&expr) {
                self.at += 1;
                let args = self.arguments()?;
                expr = Expr::Invoke(Box::new(expr), args);
                continue;
            }
            return Ok(expr);
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        self.skip_space();
        let Some(c) = self.peek() else {
            return Err("the formula ends where a value was expected".to_owned());
        };
        match c {
            '(' => {
                self.at += 1;
                self.depth += 1;
                if self.depth > NESTING {
                    return Err("the formula is nested too deeply".to_owned());
                }
                let first = self.comparison()?;
                self.skip_space();
                let mut items = vec![first];
                while self.eat(',') {
                    items.push(self.comparison()?);
                    self.skip_space();
                }
                self.expect(')')?;
                self.depth -= 1;
                Ok(match items.len() {
                    1 => Expr::Paren(Box::new(items.remove(0))),
                    _ => Expr::Union(items),
                })
            }
            '"' => self.string().map(Expr::Text),
            '{' => self.array(),
            '#' => self.error(),
            '\'' => {
                let sheet = self.quoted_sheet()?;
                self.sheet_reference(sheet)
            }
            '[' => self.bracketed(),
            c if c.is_ascii_digit() || c == '.' => self.number_or_rows(),
            '$' if matches!(self.peek_at(1), Some(d) if d.is_ascii_digit()) => {
                self.number_or_rows()
            }
            '$' => self.word_or_reference(),
            c if c.is_alphabetic() || c == '_' || c == '\\' => self.word_or_reference(),
            other => Err(format!("`{other}` cannot start a value")),
        }
    }

    fn string(&mut self) -> Result<Arc<str>, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("a piece of text has no closing quote".to_owned()),
                Some('"') => {
                    self.at += 1;
                    if self.peek() == Some('"') {
                        out.push('"');
                        self.at += 1;
                    } else {
                        return Ok(Arc::from(out.as_str()));
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
    }

    fn array(&mut self) -> Result<Expr, String> {
        self.at += 1;
        let mut rows = vec![Vec::new()];
        loop {
            self.skip_space();
            let negative = self.eat('-');
            self.skip_space();
            let item = match self.peek() {
                Some('"') => Expr::Text(self.string()?),
                Some('#') => self.error()?,
                Some(c) if c.is_ascii_digit() || c == '.' => {
                    let n = self.number()?;
                    Expr::Number(if negative { -n } else { n })
                }
                Some(c) if c.is_alphabetic() => {
                    let word = self.word();
                    match word.to_ascii_uppercase().as_str() {
                        "TRUE" => Expr::Bool(true),
                        "FALSE" => Expr::Bool(false),
                        _ => return Err(format!("`{word}` cannot be in an array constant")),
                    }
                }
                _ => {
                    return Err(
                        "an array constant holds numbers, text, TRUE, FALSE and errors".to_owned()
                    )
                }
            };
            rows.last_mut().map(|row| row.push(item));
            self.skip_space();
            match self.peek() {
                Some(',') => self.at += 1,
                Some(';') => {
                    self.at += 1;
                    rows.push(Vec::new());
                }
                Some('}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err("an array constant has no closing brace".to_owned()),
            }
        }
        let width = rows[0].len();
        if rows.iter().any(|row| row.len() != width) {
            return Err("every row of an array constant needs the same number of values".to_owned());
        }
        Ok(Expr::Array(rows))
    }

    fn error(&mut self) -> Result<Expr, String> {
        let rest: String = self.chars[self.at..].iter().take(16).collect();
        let upper = rest.to_ascii_uppercase();
        for kind in ErrorKind::ALL {
            if upper.starts_with(kind.text()) {
                self.at += kind.text().chars().count();
                // `#REF!` followed by `!A1`, which is what a deleted sheet leaves.
                if kind == ErrorKind::Ref && self.peek() == Some('!') {
                    self.at += 1;
                    let _ = self.word();
                }
                return Ok(Expr::Error(kind));
            }
        }
        Err(format!(
            "`{}` is not an error value",
            rest.split(|c: char| c == ',' || c == ')').next().unwrap_or("")
        ))
    }

    fn number(&mut self) -> Result<f64, String> {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.peek() == Some('.') {
            self.at += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E'))
            && (matches!(self.peek_at(1), Some(c) if c.is_ascii_digit())
                || matches!(self.peek_at(1), Some('+' | '-'))
                    && matches!(self.peek_at(2), Some(c) if c.is_ascii_digit()))
        {
            self.at += 2;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.at += 1;
            }
        }
        let text: String = self.chars[start..self.at].iter().collect();
        crate::value::parse_literal(&text).ok_or_else(|| format!("`{text}` is not a number"))
    }

    /// A number, or a run of whole rows such as `3:5`.
    fn number_or_rows(&mut self) -> Result<Expr, String> {
        let start = self.at;
        if let Some(rows) = self.try_rows(None)? {
            return Ok(rows);
        }
        self.at = start;
        Ok(Expr::Number(self.number()?))
    }

    /// `3:5` or `$3:$5`, or nothing when the characters here are not that.
    fn try_rows(
        &mut self,
        sheet: Option<(Arc<str>, Option<Arc<str>>)>,
    ) -> Result<Option<Expr>, String> {
        let start = self.at;
        let Some((a, a_abs)) = self.row_number() else {
            self.at = start;
            return Ok(None);
        };
        if self.peek() != Some(':') {
            self.at = start;
            return Ok(None);
        }
        self.at += 1;
        let Some((b, b_abs)) = self.row_number() else {
            self.at = start;
            return Ok(None);
        };
        let (mut first, mut last) = Reference::whole_rows(a.min(b), a.max(b));
        first.row_abs = a_abs;
        last.row_abs = b_abs;
        let (sheet, sheet_last) = sheet.map_or((None, None), |(s, l)| (Some(s), l));
        Ok(Some(Expr::Ref(Reference { sheet, sheet_last, first, last, shape: Shape::Rows })))
    }

    fn row_number(&mut self) -> Option<(u32, bool)> {
        let abs = self.eat('$');
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start || matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '.') {
            return None;
        }
        let text: String = self.chars[start..self.at].iter().collect();
        let row: u32 = text.parse().ok()?;
        (1..=MAX_ROWS).contains(&row).then(|| (row - 1, abs))
    }

    /// Letters, digits, `_`, `.`, `\`, `?` and `$`, the characters a name or a reference is made of.
    fn word(&mut self) -> String {
        let start = self.at;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?' | '$'))
        {
            self.at += 1;
        }
        self.chars[start..self.at].iter().collect()
    }

    fn quoted_sheet(&mut self) -> Result<Arc<str>, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("a quoted sheet name has no closing quote".to_owned()),
                Some('\'') => {
                    self.at += 1;
                    if self.peek() == Some('\'') {
                        out.push('\'');
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
        // `'Sheet 1:Sheet 3'!A1` names a 3D span inside one pair of quotes.
        Ok(Arc::from(out.as_str()))
    }

    /// After a sheet name: `!` and then a reference.
    fn sheet_reference(&mut self, sheet: Arc<str>) -> Result<Expr, String> {
        let (sheet, sheet_last) = match sheet.split_once(':') {
            Some((a, b)) if !a.is_empty() && !b.is_empty() => (Arc::from(a), Some(Arc::from(b))),
            _ => (sheet, None),
        };
        if !self.eat('!') {
            return Err(format!("`{sheet}` should be followed by `!` and a cell"));
        }
        if self.peek() == Some('#') {
            return self.error();
        }
        if let Some(rows) = self.try_rows(Some((sheet.clone(), sheet_last.clone())))? {
            return Ok(rows);
        }
        let word = self.word();
        if word.is_empty() {
            return Err(format!("`{sheet}!` should be followed by a cell"));
        }
        match self.reference_from_word(&word, Some(sheet.clone()), sheet_last.clone())? {
            Some(expr) => Ok(expr),
            // A name scoped to another sheet: `Sheet2!Total`.
            None => Ok(Expr::Name(Arc::from(format!("{}!{word}", sheet).as_str()))),
        }
    }

    /// A word that is a cell, a range, whole columns, a function call, a sheet, a name or TRUE and FALSE.
    fn word_or_reference(&mut self) -> Result<Expr, String> {
        let start = self.at;
        let word = self.word();
        if word.is_empty() {
            return Err(format!("`{}` cannot start a value", self.peek().unwrap_or(' ')));
        }
        // A function call.
        if self.peek() == Some('(') {
            self.at += 1;
            let mut args = self.arguments()?;
            let name = canonical_function(&word);
            // A file writes `A1#` and `@A1:A9` as these two functions.
            match (name.as_str(), args.len()) {
                ("ANCHORARRAY", 1) => return Ok(Expr::Spill(Box::new(args.remove(0)))),
                ("SINGLE", 1) => return Ok(Expr::Implicit(Box::new(args.remove(0)))),
                _ => {}
            }
            return Ok(Expr::Call(Arc::from(name.as_str()), args));
        }
        // A sheet name: `Sheet1!A1`, or the first of a 3D span `Sheet1:Sheet3!A1`.
        if self.peek() == Some('!') {
            return self.sheet_reference(Arc::from(word.as_str()));
        }
        if self.peek() == Some(':') {
            let save = self.at;
            self.at += 1;
            let second = self.word();
            if !second.is_empty() && self.peek() == Some('!') {
                return self.sheet_reference(Arc::from(format!("{word}:{second}").as_str()));
            }
            self.at = save;
        }
        // A table: `Table1[Amount]`.
        if self.peek() == Some('[') {
            let inner = self.bracket_text()?;
            return Ok(Expr::Table(table_ref(Some(Arc::from(word.as_str())), &inner)));
        }
        if let Some(expr) = self.reference_from_word(&word, None, None)? {
            return Ok(expr);
        }
        match word.to_ascii_uppercase().as_str() {
            "TRUE" => return Ok(Expr::Bool(true)),
            "FALSE" => return Ok(Expr::Bool(false)),
            _ => {}
        }
        if word.contains('$') || word.starts_with('.') {
            self.at = start;
            return Err(format!("`{word}` is not a cell, a range or a name"));
        }
        // An external reference `[1]Sheet1!A1` never reaches here; this is a defined name or a parameter.
        let name = word.strip_prefix("_xlpm.").unwrap_or(&word);
        Ok(Expr::Name(Arc::from(name)))
    }

    /// The reference a word starts, reading `:` and a second corner when there is one.
    fn reference_from_word(
        &mut self,
        word: &str,
        sheet: Option<Arc<str>>,
        sheet_last: Option<Arc<str>>,
    ) -> Result<Option<Expr>, String> {
        if let Some(first) = point(word) {
            let save = self.at;
            if self.eat(':') {
                let second = self.word();
                if let Some(last) = point(&second) {
                    let (first, last) = ordered(first, last);
                    return Ok(Some(Expr::Ref(Reference {
                        sheet,
                        sheet_last,
                        first,
                        last,
                        shape: Shape::Range,
                    })));
                }
                self.at = save;
            }
            return Ok(Some(Expr::Ref(Reference {
                sheet,
                sheet_last,
                first,
                last: first,
                shape: Shape::Cell,
            })));
        }
        // Whole columns: `A:C`, `$A:$C`.
        if let Some((a, a_abs)) = column_part(word) {
            let save = self.at;
            if self.eat(':') {
                let second = self.word();
                if let Some((b, b_abs)) = column_part(&second) {
                    let (mut first, mut last) = Reference::whole_columns(a.min(b), a.max(b));
                    first.col_abs = a_abs;
                    last.col_abs = b_abs;
                    return Ok(Some(Expr::Ref(Reference {
                        sheet,
                        sheet_last,
                        first,
                        last,
                        shape: Shape::Columns,
                    })));
                }
            }
            self.at = save;
        }
        Ok(None)
    }

    /// Arguments after `(`, up to and including `)`. An argument left out is [`Expr::Missing`].
    fn arguments(&mut self) -> Result<Vec<Expr>, String> {
        self.depth += 1;
        if self.depth > NESTING {
            return Err("the formula is nested too deeply".to_owned());
        }
        let outer = std::mem::replace(&mut self.depth, 0);
        let mut args = Vec::new();
        self.skip_space();
        if self.eat(')') {
            self.depth = outer - 1;
            return Ok(args);
        }
        loop {
            self.skip_space();
            if matches!(self.peek(), Some(',') | Some(')')) {
                args.push(Expr::Missing);
            } else {
                args.push(self.comparison()?);
            }
            self.skip_space();
            if self.eat(',') {
                continue;
            }
            if self.eat(')') {
                break;
            }
            return Err(match self.peek() {
                Some(c) => format!("expected `,` or `)` in a function's arguments but found `{c}`"),
                None => "a function's arguments have no closing parenthesis".to_owned(),
            });
        }
        self.depth = outer - 1;
        Ok(args)
    }

    /// `[...]` at the start of a value: an external workbook reference or a table reference with no name.
    fn bracketed(&mut self) -> Result<Expr, String> {
        let start = self.at;
        let inner = self.bracket_text()?;
        // `[1]Sheet1!A1` and `[Book.xlsx]Sheet1!A1`.
        if matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '\'' || c == '_') {
            if self.peek() == Some('\'') {
                let _ = self.quoted_sheet()?;
            } else {
                let _ = self.word();
            }
            if self.eat('!') {
                let _ = self.word();
                if self.eat(':') {
                    let _ = self.word();
                }
            }
            let text: String = self.chars[start..self.at].iter().collect();
            return Ok(Expr::External(Arc::from(text.as_str())));
        }
        Ok(Expr::Table(table_ref(None, &inner)))
    }

    /// The text between `[` and its matching `]`, with the brackets, honouring `'` escapes.
    fn bracket_text(&mut self) -> Result<String, String> {
        let start = self.at;
        let mut depth = 0;
        loop {
            match self.peek() {
                None => return Err("a `[` has no matching `]`".to_owned()),
                Some('\'') => self.at += 2,
                Some('[') => {
                    depth += 1;
                    self.at += 1;
                }
                Some(']') => {
                    depth -= 1;
                    self.at += 1;
                    if depth == 0 {
                        break;
                    }
                }
                Some(_) => self.at += 1,
            }
        }
        Ok(self.chars[start..self.at].iter().collect())
    }
}

/// Whether an expression can be one side of an intersection.
fn is_reference_like(expr: &Expr) -> bool {
    match expr {
        Expr::Ref(_) | Expr::Name(_) | Expr::Table(_) | Expr::Union(_) | Expr::Intersect(..) => {
            true
        }
        Expr::Binary(BinOp::Range, ..) => true,
        Expr::Paren(inner) => is_reference_like(inner),
        Expr::Call(name, _) => matches!(&**name, "INDEX" | "OFFSET" | "INDIRECT" | "CHOOSE" | "IF"),
        _ => false,
    }
}

fn is_callable(expr: &Expr) -> bool {
    matches!(expr, Expr::Call(name, _) if &**name == "LAMBDA")
        || matches!(expr, Expr::Paren(inner) if is_callable(inner))
}

fn ordered(a: Point, b: Point) -> (Point, Point) {
    let first = Point {
        row: a.row.min(b.row),
        col: a.col.min(b.col),
        row_abs: if a.row <= b.row { a.row_abs } else { b.row_abs },
        col_abs: if a.col <= b.col { a.col_abs } else { b.col_abs },
    };
    let last = Point {
        row: a.row.max(b.row),
        col: a.col.max(b.col),
        row_abs: if a.row <= b.row { b.row_abs } else { a.row_abs },
        col_abs: if a.col <= b.col { b.col_abs } else { a.col_abs },
    };
    (first, last)
}

/// `$B$4` into a point, or `None` when the word is not a cell.
fn point(word: &str) -> Option<Point> {
    let bytes = word.as_bytes();
    let mut at = 0;
    let col_abs = bytes.first() == Some(&b'$');
    if col_abs {
        at += 1;
    }
    let letters = at;
    while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
        at += 1;
    }
    let col = column_number(&word[letters..at])?;
    let row_abs = bytes.get(at) == Some(&b'$');
    if row_abs {
        at += 1;
    }
    let digits = &word[at..];
    if digits.is_empty() || digits.len() > 7 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row: u32 = digits.parse().ok()?;
    if row == 0 || row > MAX_ROWS || col >= MAX_COLS {
        return None;
    }
    Some(Point { row: row - 1, col, row_abs, col_abs })
}

/// `$A` into a column and whether it is absolute.
fn column_part(word: &str) -> Option<(u32, bool)> {
    let (abs, letters) = match word.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, word),
    };
    column_number(letters).map(|col| (col, abs))
}

/// A function's name as the library knows it: upper case, without the prefixes newer functions are
/// written with in a file.
pub fn canonical_function(name: &str) -> String {
    let mut upper = name.to_ascii_uppercase();
    for prefix in ["_XLFN._XLWS.", "_XLFN.", "_XLWS.", "_XLUDF."] {
        if let Some(rest) = upper.strip_prefix(prefix) {
            upper = rest.to_owned();
        }
    }
    upper
}

/// Read the inside of a structured reference.
fn table_ref(table: Option<Arc<str>>, text: &str) -> TableRef {
    let inner = &text[1..text.len().saturating_sub(1)];
    let mut items = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    let mut this_row = false;
    let add_column = |name: &str, columns: &mut Vec<String>| {
        let name = unescape_column(name.trim());
        if !name.is_empty() {
            columns.push(name);
        }
    };
    if !inner.contains('[') {
        let body = inner.trim();
        if let Some(rest) = body.strip_prefix('@') {
            this_row = true;
            add_column(rest, &mut columns);
        } else if let Some(special) = body.strip_prefix('#') {
            items.push(Arc::from(special.to_ascii_lowercase().as_str()));
        } else {
            add_column(body, &mut columns);
        }
    } else {
        // `[[#Headers],[Name]:[Total]]` or `[@[Unit price]]`.
        let mut rest = inner.trim();
        if let Some(after) = rest.strip_prefix('@') {
            this_row = true;
            rest = after.trim();
        }
        let mut parts = Vec::new();
        let mut depth = 0;
        let mut current = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\'' => {
                    current.push(c);
                    if let Some(n) = chars.next() {
                        current.push(n);
                    }
                }
                '[' => {
                    depth += 1;
                    if depth > 1 {
                        current.push(c);
                    }
                }
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        parts.push(std::mem::take(&mut current));
                    } else {
                        current.push(c);
                    }
                }
                ':' if depth == 0 => parts.push(":".to_owned()),
                _ if depth > 0 => current.push(c),
                _ => {}
            }
        }
        for part in parts {
            if part == ":" {
                continue;
            }
            match part.strip_prefix('#') {
                Some(special) => items.push(Arc::from(special.to_ascii_lowercase().as_str())),
                None => add_column(&part, &mut columns),
            }
        }
    }
    if this_row {
        items.push(Arc::from("this row"));
    }
    TableRef {
        table,
        items,
        first_column: columns.first().map(|c| Arc::from(c.as_str())),
        last_column: columns.get(1).or(columns.first()).map(|c| Arc::from(c.as_str())),
        text: Arc::from(text),
    }
}

fn unescape_column(name: &str) -> String {
    let mut out = String::new();
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::print::print;

    fn round(text: &str) -> String {
        print(&parse(text).unwrap_or_else(|e| panic!("{text}: {e}")), false)
    }

    #[test]
    fn formulas_print_back_the_way_excel_writes_them() {
        for (text, printed) in [
            ("1+2*3", "1+2*3"),
            ("(1+2)*3", "(1+2)*3"),
            ("-2^2", "-2^2"),
            ("sum(a1:b2,c3)", "SUM(A1:B2,C3)"),
            ("$A$1+A$2+$a3", "$A$1+A$2+$A3"),
            ("Sheet2!A1", "Sheet2!A1"),
            ("'My sheet'!A1:B2", "'My sheet'!A1:B2"),
            ("'it''s'!A1", "'it''s'!A1"),
            ("SUM(Sheet1:Sheet3!A1)", "SUM(Sheet1:Sheet3!A1)"),
            ("SUM(A:A)", "SUM(A:A)"),
            ("SUM($3:5)", "SUM($3:5)"),
            ("\"a\"\"b\"&\"c\"", "\"a\"\"b\"&\"c\""),
            ("{1,2;3,4}", "{1,2;3,4}"),
            ("{\"a\",TRUE,-1.5}", "{\"a\",TRUE,-1.5}"),
            ("IF(A1,,2)", "IF(A1,,2)"),
            ("_xlfn.XLOOKUP(1,A:A,B:B)", "XLOOKUP(1,A:A,B:B)"),
            ("_xlfn._xlws.FILTER(A1:A9,B1:B9>0)", "FILTER(A1:A9,B1:B9>0)"),
            ("A1#", "A1#"),
            ("@A1:A9", "@A1:A9"),
            ("SUM(A1:C3 B2:D4)", "SUM(A1:C3 B2:D4)"),
            ("SUM((A1,B2))", "SUM((A1,B2))"),
            ("50%", "50%"),
            ("A1<>B1", "A1<>B1"),
            ("Tax_Rate*2", "Tax_Rate*2"),
            ("LAMBDA(x,x+1)(2)", "LAMBDA(x,x+1)(2)"),
            ("Table1[Amount]", "Table1[Amount]"),
            ("SUM(Table1[[#Data],[Amount]])", "SUM(Table1[[#Data],[Amount]])"),
            ("#REF!+1", "#REF!+1"),
            ("1.5E+3", "1500"),
            ("A1:INDEX(B:B,3)", "A1:INDEX(B:B,3)"),
            ("LOG10(100)", "LOG10(100)"),
            ("TRUE", "TRUE"),
        ] {
            assert_eq!(round(text), printed, "{text}");
        }
    }

    #[test]
    fn a_reversed_range_is_written_top_left_first() {
        assert_eq!(round("B3:A1"), "A1:B3");
    }

    #[test]
    fn broken_formulas_are_refused_with_a_reason() {
        for text in ["", "1+", "SUM(1,2", "\"open", "{1,2;3}", "1 2", "A1:", ")"] {
            assert!(parse(text).is_err(), "{text} should be refused");
        }
    }

    #[test]
    fn structured_references_name_the_table_the_items_and_the_columns() {
        let Expr::Table(t) = parse("Table1[[#Headers],[Name]:[Total]]").unwrap() else { panic!() };
        assert_eq!(t.table.as_deref(), Some("Table1"));
        assert_eq!(t.items, vec![Arc::from("headers")]);
        assert_eq!(t.first_column.as_deref(), Some("Name"));
        assert_eq!(t.last_column.as_deref(), Some("Total"));
        let Expr::Table(t) = parse("[@[Unit price]]").unwrap() else { panic!() };
        assert_eq!(t.items, vec![Arc::from("this row")]);
        assert_eq!(t.first_column.as_deref(), Some("Unit price"));
    }
}
