//! The arithmetic a calculator's outputs and charts are written in.
//!
//! A calculator is how an agent builds a small tool for the question: inputs a person moves and
//! outputs worked out from them. The outputs are expressions such as `fv(rate/100/12, years*12,
//! monthly)`, and this is what reads and evaluates them.
//!
//! **Nothing is in scope but the numbers it is handed.** The names are the calculator's inputs and,
//! inside a chart, its `x`; the functions are the list below; there is no way to reach a file, a
//! variable of the process or anything else. Evaluation is bounded at [`STEPS`] so an expression
//! cannot cost a frame, and parsing at [`DEEPEST`] levels of nesting so one cannot exhaust the stack.
//!
//! The grammar, loosest first: `c ? a : b`, `||`, `&&`, `== !=`, `< <= > >=`, `+ -`, `* / %`, unary
//! `- ! +`, `^` (right to left, so `-2^2` is `-4`), then numbers, names, calls and parentheses. A
//! comparison or a logical operator answers 1 or 0.

/// How many nodes one evaluation may visit.
pub const STEPS: usize = 10_000;
/// How deeply an expression may nest.
pub const DEEPEST: usize = 64;
/// How many tokens an expression may have. No calculator needs a tenth of it.
pub const LONGEST: usize = 2_000;

/// The functions an expression may call, with how many arguments each takes.
pub const FUNCTIONS: &[(&str, usize, usize)] = &[
    ("abs", 1, 1),
    ("min", 1, 16),
    ("max", 1, 16),
    ("round", 1, 2),
    ("floor", 1, 1),
    ("ceil", 1, 1),
    ("sqrt", 1, 1),
    ("pow", 2, 2),
    ("exp", 1, 1),
    ("ln", 1, 1),
    ("log10", 1, 1),
    ("clamp", 3, 3),
    ("if", 3, 3),
    ("fv", 3, 4),
    ("pmt", 3, 3),
];

/// A parsed expression, kept with the text it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub source: String,
    node: Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Number(f64),
    Name(String),
    Unary(char, Box<Node>),
    Binary(&'static str, Box<Node>, Box<Node>),
    Choose(Box<Node>, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

/// Whether `name` can be a name in an expression.
pub fn is_a_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Op(&'static str),
}

const OPERATORS: &[&str] = &[
    "&&", "||", "==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "%", "^", "!", "(", ")", ",",
    "?", ":",
];

/// Cut `source` into tokens.
fn tokens(source: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
        } else if c.is_ascii_digit()
            || (c == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit))
        {
            let start = at;
            while at < chars.len()
                && (chars[at].is_ascii_digit() || chars[at] == '.' || chars[at] == '_')
            {
                at += 1;
            }
            if at < chars.len() && (chars[at] == 'e' || chars[at] == 'E') {
                let mut look = at + 1;
                if look < chars.len() && (chars[look] == '+' || chars[look] == '-') {
                    look += 1;
                }
                if look < chars.len() && chars[look].is_ascii_digit() {
                    at = look;
                    while at < chars.len() && chars[at].is_ascii_digit() {
                        at += 1;
                    }
                }
            }
            let text: String = chars[start..at].iter().filter(|c| **c != '_').collect();
            let number = text.parse::<f64>().map_err(|_| format!("\"{text}\" is not a number"))?;
            out.push(Token::Number(number));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_ascii_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            out.push(Token::Name(chars[start..at].iter().collect()));
        } else {
            let rest: String = chars[at..chars.len().min(at + 2)].iter().collect();
            let Some(op) = OPERATORS.iter().find(|op| rest.starts_with(**op)) else {
                return Err(format!("\"{c}\" is not part of an expression"));
            };
            at += op.chars().count();
            out.push(Token::Op(op));
        }
    }
    Ok(out)
}

/// A Pratt parser over the tokens.
struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

/// How tightly an infix operator binds, or `None` for a token that is not one.
fn binding(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 2,
        "&&" => 3,
        "==" | "!=" => 4,
        "<" | "<=" | ">" | ">=" => 5,
        "+" | "-" => 6,
        "*" | "/" | "%" => 7,
        "^" => 9,
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn expect(&mut self, op: &str) -> Result<(), String> {
        match self.peek() {
            Some(Token::Op(found)) if *found == op => {
                self.at += 1;
                Ok(())
            }
            _ => Err(format!("expected \"{op}\"")),
        }
    }

    /// An expression whose operators bind at least as tightly as `floor`.
    fn expression(&mut self, floor: u8) -> Result<Node, String> {
        self.depth += 1;
        if self.depth > DEEPEST {
            return Err(format!("nested more than {DEEPEST} deep"));
        }
        let mut left = self.prefix()?;
        while let Some(Token::Op(op)) = self.peek().cloned() {
            if op == "?" {
                if floor > 1 {
                    break;
                }
                self.at += 1;
                let yes = self.expression(1)?;
                self.expect(":")?;
                let no = self.expression(1)?;
                left = Node::Choose(Box::new(left), Box::new(yes), Box::new(no));
                continue;
            }
            let Some(power) = binding(op) else { break };
            if power < floor {
                break;
            }
            self.at += 1;
            // `^` is right to left, every other operator left to right.
            let next = if op == "^" { power } else { power + 1 };
            let right = self.expression(next)?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        self.depth -= 1;
        Ok(left)
    }

    /// A number, a name, a call, a bracketed expression, or a prefix operator applied to one.
    fn prefix(&mut self) -> Result<Node, String> {
        match self.peek().cloned() {
            None => Err("the expression ends too soon".to_owned()),
            Some(Token::Number(number)) => {
                self.at += 1;
                Ok(Node::Number(number))
            }
            Some(Token::Name(name)) => {
                self.at += 1;
                if self.peek() != Some(&Token::Op("(")) {
                    return Ok(Node::Name(name));
                }
                self.at += 1;
                let Some((_, least, most)) = FUNCTIONS.iter().find(|(known, _, _)| *known == name)
                else {
                    let names: Vec<&str> = FUNCTIONS.iter().map(|(name, _, _)| *name).collect();
                    return Err(format!(
                        "\"{name}\" is not a function; the functions are {}",
                        names.join(", ")
                    ));
                };
                let mut arguments = Vec::new();
                if self.peek() != Some(&Token::Op(")")) {
                    loop {
                        arguments.push(self.expression(1)?);
                        match self.peek() {
                            Some(Token::Op(",")) => self.at += 1,
                            _ => break,
                        }
                    }
                }
                self.expect(")")?;
                if arguments.len() < *least || arguments.len() > *most {
                    return Err(format!(
                        "{name} takes {least} to {most} arguments, not {}",
                        arguments.len()
                    ));
                }
                Ok(Node::Call(name, arguments))
            }
            Some(Token::Op("(")) => {
                self.at += 1;
                let inside = self.expression(1)?;
                self.expect(")")?;
                Ok(inside)
            }
            Some(Token::Op(op @ ("-" | "!" | "+"))) => {
                self.at += 1;
                // Tighter than `*` and looser than `^`, so `-2^2` is `-4`.
                let operand = self.expression(8)?;
                Ok(Node::Unary(op.chars().next().unwrap_or('-'), Box::new(operand)))
            }
            Some(Token::Op(op)) => Err(format!("\"{op}\" cannot start an expression")),
        }
    }
}

impl Expr {
    /// Parse `source`, or say what is wrong with it.
    pub fn parse(source: &str) -> Result<Self, String> {
        let tokens = tokens(source)?;
        // A chain of `+` is as deep as it is long once it is a tree, so length is bounded too.
        if tokens.len() > LONGEST {
            return Err(format!("longer than {LONGEST} pieces"));
        }
        let mut parser = Parser { tokens, at: 0, depth: 0 };
        let node = parser.expression(1)?;
        if parser.at < parser.tokens.len() {
            return Err("there is more after the end of the expression".to_owned());
        }
        Ok(Self { source: source.to_owned(), node })
    }

    /// Every name the expression reads, so a calculator can say which ones it was not given.
    pub fn names(&self) -> Vec<String> {
        fn walk(node: &Node, out: &mut Vec<String>) {
            match node {
                Node::Number(_) => {}
                Node::Name(name) => {
                    if !out.contains(name) {
                        out.push(name.clone());
                    }
                }
                Node::Unary(_, inner) => walk(inner, out),
                Node::Binary(_, a, b) => {
                    walk(a, out);
                    walk(b, out);
                }
                Node::Choose(a, b, c) => {
                    walk(a, out);
                    walk(b, out);
                    walk(c, out);
                }
                Node::Call(_, arguments) => arguments.iter().for_each(|one| walk(one, out)),
            }
        }
        let mut out = Vec::new();
        walk(&self.node, &mut out);
        out
    }

    /// Evaluate with `lookup` answering the value of each name.
    pub fn eval(&self, lookup: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
        let mut steps = 0;
        let value = eval(&self.node, lookup, &mut steps)?;
        match value.is_finite() {
            true => Ok(value),
            false => Err("the answer is not a finite number".to_owned()),
        }
    }
}

/// One step of evaluation.
fn eval(
    node: &Node,
    lookup: &dyn Fn(&str) -> Option<f64>,
    steps: &mut usize,
) -> Result<f64, String> {
    *steps += 1;
    if *steps > STEPS {
        return Err(format!("took more than {STEPS} steps"));
    }
    let truth = |flag: bool| if flag { 1.0 } else { 0.0 };
    Ok(match node {
        Node::Number(number) => *number,
        Node::Name(name) => match name.as_str() {
            "pi" => std::f64::consts::PI,
            "e" => std::f64::consts::E,
            _ => lookup(name).ok_or_else(|| format!("\"{name}\" is not one of the inputs"))?,
        },
        Node::Unary(op, inner) => {
            let value = eval(inner, lookup, steps)?;
            match op {
                '-' => -value,
                '!' => truth(value == 0.0),
                _ => value,
            }
        }
        Node::Binary(op, a, b) => {
            let a = eval(a, lookup, steps)?;
            // `&&` and `||` look at the right only when they have to, like everywhere else.
            match *op {
                "&&" if a == 0.0 => return Ok(0.0),
                "||" if a != 0.0 => return Ok(1.0),
                _ => {}
            }
            let b = eval(b, lookup, steps)?;
            match *op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" if b == 0.0 => return Err("divides by zero".to_owned()),
                "/" => a / b,
                "%" if b == 0.0 => return Err("divides by zero".to_owned()),
                "%" => a % b,
                "^" => a.powf(b),
                "<" => truth(a < b),
                "<=" => truth(a <= b),
                ">" => truth(a > b),
                ">=" => truth(a >= b),
                "==" => truth((a - b).abs() < 1e-9),
                "!=" => truth((a - b).abs() >= 1e-9),
                "&&" | "||" => truth(b != 0.0),
                _ => return Err(format!("\"{op}\" is not an operator")),
            }
        }
        Node::Choose(condition, yes, no) => match eval(condition, lookup, steps)? != 0.0 {
            true => eval(yes, lookup, steps)?,
            false => eval(no, lookup, steps)?,
        },
        Node::Call(name, arguments) => {
            if name == "if" {
                return match eval(&arguments[0], lookup, steps)? != 0.0 {
                    true => eval(&arguments[1], lookup, steps),
                    false => eval(&arguments[2], lookup, steps),
                };
            }
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments {
                values.push(eval(argument, lookup, steps)?);
            }
            call(name, &values)?
        }
    })
}

/// Apply one of [`FUNCTIONS`] to its arguments, already evaluated.
fn call(name: &str, values: &[f64]) -> Result<f64, String> {
    let at = |i: usize| values.get(i).copied().unwrap_or(0.0);
    Ok(match name {
        "abs" => at(0).abs(),
        "min" => values.iter().copied().fold(f64::INFINITY, f64::min),
        "max" => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "round" => {
            let digits = at(1).clamp(0.0, 12.0) as i32;
            let factor = 10f64.powi(digits);
            (at(0) * factor).round() / factor
        }
        "floor" => at(0).floor(),
        "ceil" => at(0).ceil(),
        "sqrt" if at(0) < 0.0 => return Err("sqrt of a negative number".to_owned()),
        "sqrt" => at(0).sqrt(),
        "pow" => at(0).powf(at(1)),
        "exp" => at(0).exp(),
        "ln" | "log10" if at(0) <= 0.0 => {
            return Err(format!("{name} of a number that is not positive"))
        }
        "ln" => at(0).ln(),
        "log10" => at(0).log10(),
        "clamp" => at(0).clamp(at(1).min(at(2)), at(1).max(at(2))),
        // What `periods` payments of `payment`, on top of `present`, come to at `rate` a period.
        "fv" => {
            let (rate, periods, payment, present) = (at(0), at(1), at(2), at(3));
            match rate == 0.0 {
                true => present + payment * periods,
                false => {
                    let growth = (1.0 + rate).powf(periods);
                    present * growth + payment * (growth - 1.0) / rate
                }
            }
        }
        // The payment a period that pays off `principal` in `periods` at `rate`.
        "pmt" => {
            let (rate, periods, principal) = (at(0), at(1), at(2));
            if periods <= 0.0 {
                return Err("pmt needs at least one period".to_owned());
            }
            match rate == 0.0 {
                true => principal / periods,
                false => principal * rate / (1.0 - (1.0 + rate).powf(-periods)),
            }
        }
        _ => return Err(format!("\"{name}\" is not a function")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(source: &str) -> f64 {
        let lookup = |name: &str| match name {
            "x" => Some(3.0),
            "rate" => Some(5.0),
            _ => None,
        };
        Expr::parse(source).unwrap().eval(&lookup).unwrap()
    }

    #[test]
    fn precedence_is_what_arithmetic_says_it_is() {
        assert_eq!(value("1 + 2 * 3"), 7.0);
        assert_eq!(value("(1 + 2) * 3"), 9.0);
        assert_eq!(value("-2^2"), -4.0);
        assert_eq!(value("2^3^2"), 512.0, "right to left");
        assert_eq!(value("10 - 4 - 3"), 3.0, "left to right");
        assert_eq!(value("x > 2 ? 10 : 20"), 10.0);
        assert_eq!(value("x > 2 && rate < 4"), 0.0);
        assert_eq!(value("1_000 * 2"), 2000.0);
    }

    #[test]
    fn the_functions_answer_what_a_spreadsheet_would() {
        assert_eq!(value("round(2.345, 2)"), 2.35);
        assert_eq!(value("max(1, x, 2)"), 3.0);
        assert_eq!(value("clamp(15, 0, 10)"), 10.0);
        // 300 a month for a year at nothing is 3,600; at 1% a month it is a little more.
        assert_eq!(value("fv(0, 12, 300)"), 3600.0);
        assert!((value("fv(0.01, 12, 300)") - 3804.75).abs() < 0.01);
        // A 100,000 loan over 360 months at 0.5% a month.
        assert!((value("pmt(0.005, 360, 100000)") - 599.55).abs() < 0.01);
    }

    #[test]
    fn a_mistake_is_a_sentence_and_never_a_panic() {
        assert!(Expr::parse("1 +").is_err());
        assert!(Expr::parse("foo(1)").unwrap_err().contains("not a function"));
        assert!(Expr::parse("pow(1)").unwrap_err().contains("2 to 2"));
        assert!(Expr::parse("1 $ 2").is_err());
        let missing = Expr::parse("y + 1").unwrap().eval(&|_| None).unwrap_err();
        assert!(missing.contains("\"y\""), "{missing}");
        assert!(Expr::parse("1 / 0").unwrap().eval(&|_| None).is_err());
    }

    #[test]
    fn an_expression_cannot_nest_or_run_without_bound() {
        let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
        assert!(Expr::parse(&deep).unwrap_err().contains("nested"));
        let long = vec!["1"; 20_000].join("+");
        let parsed = Expr::parse(&long);
        // Either the parser or the evaluator refuses it; neither may hang or overflow.
        if let Ok(expr) = parsed {
            assert!(expr.eval(&|_| None).is_err());
        }
    }

    #[test]
    fn the_names_an_expression_reads_are_listed_once() {
        let expr = Expr::parse("fv(rate/100, years, monthly) + years").unwrap();
        assert_eq!(expr.names(), vec!["rate", "years", "monthly"]);
    }
}
