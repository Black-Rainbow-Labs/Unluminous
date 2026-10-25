//! The formulas of calculated fields and calculated items.
//!
//! A calculated field's formula is written with field names where a cell formula has references:
//! `Units*Price`, `'Unit price'*1.1`. Every field name stands for the sum of that field over the records
//! of the cell being calculated. A calculated item's formula names items of one field the same way:
//! `North+South`, `Region[North]*2`.

use crate::value::ErrorKind;

/// A parsed formula.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    /// A field or an item, by name.
    Name(String),
    /// A field and an item of it: `Region[North]`.
    Item(String, String),
    Neg(Box<Expr>),
    Binary(char, Box<Expr>, Box<Expr>),
    /// `=`, `<>`, `<`, `>`, `<=` and `>=` as `=`, `n`, `<`, `>`, `l` and `g`.
    Compare(char, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

/// Parse a formula, with or without a leading `=`.
pub fn parse(text: &str) -> Result<Expr, String> {
    let chars: Vec<char> = text.trim().trim_start_matches('=').chars().collect();
    let mut parser = Parser { chars, at: 0 };
    let expr = parser.compare()?;
    parser.skip();
    if parser.at < parser.chars.len() {
        return Err(format!("Unexpected {} in the formula.", parser.chars[parser.at]));
    }
    Ok(expr)
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn skip(&mut self) {
        while self.at < self.chars.len() && self.chars[self.at].is_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.chars.get(self.at).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn compare(&mut self) -> Result<Expr, String> {
        let left = self.sum()?;
        self.skip();
        let op = match (self.chars.get(self.at), self.chars.get(self.at + 1)) {
            (Some('<'), Some('>')) => Some(('n', 2)),
            (Some('<'), Some('=')) => Some(('l', 2)),
            (Some('>'), Some('=')) => Some(('g', 2)),
            (Some('<'), _) => Some(('<', 1)),
            (Some('>'), _) => Some(('>', 1)),
            (Some('='), _) => Some(('=', 1)),
            _ => None,
        };
        match op {
            Some((c, len)) => {
                self.at += len;
                let right = self.sum()?;
                Ok(Expr::Compare(c, Box::new(left), Box::new(right)))
            }
            None => Ok(left),
        }
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut left = self.product()?;
        loop {
            let op = match self.peek() {
                Some(c @ ('+' | '-')) => c,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.product()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut left = self.power()?;
        loop {
            let op = match self.peek() {
                Some(c @ ('*' | '/')) => c,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.power()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn power(&mut self) -> Result<Expr, String> {
        let base = self.unary()?;
        if self.eat('^') {
            let exponent = self.unary()?;
            return Ok(Expr::Binary('^', Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat('-') {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat('+') {
            return self.unary();
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some('(') => {
                self.at += 1;
                let inner = self.compare()?;
                if !self.eat(')') {
                    return Err("A closing bracket is missing.".to_owned());
                }
                Ok(inner)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.at;
                while self.at < self.chars.len()
                    && (self.chars[self.at].is_ascii_digit() || self.chars[self.at] == '.')
                {
                    self.at += 1;
                }
                let text: String = self.chars[start..self.at].iter().collect();
                text.parse().map(Expr::Number).map_err(|_| format!("{text} is not a number."))
            }
            Some(_) => self.name(),
            None => Err("The formula ends too soon.".to_owned()),
        }
    }

    fn name(&mut self) -> Result<Expr, String> {
        let name = self.word()?;
        if self.eat('(') {
            let mut args = Vec::new();
            if !self.eat(')') {
                loop {
                    args.push(self.compare()?);
                    if self.eat(')') {
                        break;
                    }
                    if !self.eat(',') {
                        return Err("A comma or a closing bracket is missing.".to_owned());
                    }
                }
            }
            return Ok(Expr::Call(name.to_ascii_uppercase(), args));
        }
        if self.chars.get(self.at) == Some(&'[') {
            self.at += 1;
            let start = self.at;
            while self.at < self.chars.len() && self.chars[self.at] != ']' {
                self.at += 1;
            }
            let item: String = self.chars[start..self.at].iter().collect();
            self.at += 1;
            return Ok(Expr::Item(name, item.trim_matches('\'').to_owned()));
        }
        Ok(Expr::Name(name))
    }

    /// A name: quoted with single marks, or letters, digits, underscores and full stops.
    fn word(&mut self) -> Result<String, String> {
        self.skip();
        if self.chars.get(self.at) == Some(&'\'') {
            self.at += 1;
            let mut out = String::new();
            while self.at < self.chars.len() {
                let c = self.chars[self.at];
                self.at += 1;
                if c == '\'' {
                    if self.chars.get(self.at) == Some(&'\'') {
                        out.push('\'');
                        self.at += 1;
                        continue;
                    }
                    return Ok(out);
                }
                out.push(c);
            }
            return Err("A quote is not closed.".to_owned());
        }
        let start = self.at;
        while self.at < self.chars.len() {
            let c = self.chars[self.at];
            if c.is_alphanumeric() || c == '_' || c == '.' {
                self.at += 1;
            } else {
                break;
            }
        }
        if start == self.at {
            return Err(format!(
                "Unexpected {} in the formula.",
                self.chars.get(self.at).unwrap_or(&' ')
            ));
        }
        Ok(self.chars[start..self.at].iter().collect())
    }
}

/// Every field name a formula uses.
pub fn names_in(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Name(n) => {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                out.push(n.clone());
            }
        }
        Expr::Item(field, _) => {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(field)) {
                out.push(field.clone());
            }
        }
        Expr::Neg(e) => names_in(e, out),
        Expr::Binary(_, a, b) | Expr::Compare(_, a, b) => {
            names_in(a, out);
            names_in(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| names_in(a, out)),
        Expr::Number(_) => {}
    }
}

/// Every item a formula uses, with the field it names, if it names one.
pub fn items_in(expr: &Expr, out: &mut Vec<(Option<String>, String)>) {
    match expr {
        Expr::Name(n) => out.push((None, n.clone())),
        Expr::Item(field, item) => out.push((Some(field.clone()), item.clone())),
        Expr::Neg(e) => items_in(e, out),
        Expr::Binary(_, a, b) | Expr::Compare(_, a, b) => {
            items_in(a, out);
            items_in(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| items_in(a, out)),
        Expr::Number(_) => {}
    }
}

/// Compute a formula, asking `lookup` for what each name is.
pub fn evaluate(expr: &Expr, lookup: Lookup<'_>) -> Result<f64, ErrorKind> {
    Ok(match expr {
        Expr::Number(n) => *n,
        Expr::Name(n) => lookup(None, n)?,
        Expr::Item(field, item) => lookup(Some(field), item)?,
        Expr::Neg(e) => -evaluate(e, lookup)?,
        Expr::Binary(op, a, b) => {
            let x = evaluate(a, lookup)?;
            let y = evaluate(b, lookup)?;
            let n = match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => {
                    if y == 0.0 {
                        return Err(ErrorKind::Div0);
                    }
                    x / y
                }
                _ => x.powf(y),
            };
            if n.is_finite() {
                n
            } else {
                return Err(ErrorKind::Num);
            }
        }
        Expr::Compare(op, a, b) => {
            let x = evaluate(a, lookup)?;
            let y = evaluate(b, lookup)?;
            let t = match op {
                '=' => x == y,
                'n' => x != y,
                '<' => x < y,
                '>' => x > y,
                'l' => x <= y,
                _ => x >= y,
            };
            if t {
                1.0
            } else {
                0.0
            }
        }
        Expr::Call(name, args) => call(name, args, lookup)?,
    })
}

fn call(name: &str, args: &[Expr], lookup: Lookup<'_>) -> Result<f64, ErrorKind> {
    let arg = |i: usize| -> Result<f64, ErrorKind> {
        args.get(i).ok_or(ErrorKind::Value).and_then(|e| evaluate(e, lookup))
    };
    Ok(match name {
        "IF" => {
            if arg(0)? != 0.0 {
                arg(1)?
            } else if args.len() > 2 {
                arg(2)?
            } else {
                0.0
            }
        }
        "ABS" => arg(0)?.abs(),
        "SQRT" => {
            let x = arg(0)?;
            if x < 0.0 {
                return Err(ErrorKind::Num);
            }
            x.sqrt()
        }
        "ROUND" => {
            let digits = arg(1)?;
            let scale = 10f64.powi(digits as i32);
            (arg(0)? * scale).round() / scale
        }
        "MAX" | "MIN" | "SUM" => {
            let mut values = Vec::new();
            for i in 0..args.len() {
                values.push(arg(i)?);
            }
            match name {
                "MAX" => values.into_iter().fold(f64::NEG_INFINITY, f64::max),
                "MIN" => values.into_iter().fold(f64::INFINITY, f64::min),
                _ => values.into_iter().sum(),
            }
        }
        _ => return Err(ErrorKind::Name),
    })
}

/// A formula written the way Excel stores a calculated item's: every item named with its field.
pub fn print_with_field(expr: &Expr, field: &str) -> String {
    let quote = |s: &str| {
        if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
            s.to_owned()
        } else {
            format!("'{}'", s.replace('\'', "''"))
        }
    };
    match expr {
        Expr::Number(n) => crate::xlsx::write::number_text(*n),
        Expr::Name(n) => format!("{}[{}]", quote(field), quote(n)),
        Expr::Item(f, i) => format!("{}[{}]", quote(f), quote(i)),
        Expr::Neg(e) => format!("-{}", print_with_field(e, field)),
        Expr::Binary(op, a, b) => {
            format!("{}{}{}", print_with_field(a, field), op, print_with_field(b, field))
        }
        Expr::Compare(op, a, b) => {
            let text = match op {
                'n' => "<>".to_owned(),
                'l' => "<=".to_owned(),
                'g' => ">=".to_owned(),
                c => c.to_string(),
            };
            format!("{}{}{}", print_with_field(a, field), text, print_with_field(b, field))
        }
        Expr::Call(name, args) => {
            let args: Vec<String> = args.iter().map(|a| print_with_field(a, field)).collect();
            format!("{name}({})", args.join(","))
        }
    }
}

/// Finds the figure of a field, given the field's name and the name of an item when there is one.
type Lookup<'a> = &'a dyn Fn(Option<&str>, &str) -> Result<f64, ErrorKind>;
