//! A reader of XPath 1.0 paths over `roxmltree`, for FILTERXML.
//!
//! It reads absolute and relative paths, `//`, every axis a path uses, name tests (a prefix is ignored, as
//! Excel ignores namespaces), `*`, `text()`, `node()`, `@name`, `.`, `..`, predicates by position or by any
//! expression, unions with `|`, and the operators and functions a predicate is written with. An expression
//! that does not give a set of nodes is not an answer here, which is how FILTERXML sees it.

use std::cmp::Ordering;

use roxmltree::Node;

/// What went wrong reading or running a path. FILTERXML turns every kind into `#VALUE!`.
#[derive(Debug)]
pub struct XPathError;

type Result<T> = std::result::Result<T, XPathError>;

/// A node of the document: an element, text or the root, or one attribute of an element.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum XNode<'a, 'i> {
    Node(Node<'a, 'i>),
    Attr(Node<'a, 'i>, usize),
}

impl<'a, 'i> XNode<'a, 'i> {
    /// Where the node stands in the document: its element, then its attributes after it.
    fn order(&self) -> (Node<'a, 'i>, usize) {
        match self {
            XNode::Node(n) => (*n, 0),
            XNode::Attr(n, i) => (*n, i + 1),
        }
    }

    /// The string value XPath gives a node: the text inside an element, or an attribute's value.
    pub fn string_value(&self) -> String {
        match self {
            XNode::Attr(n, i) => {
                n.attributes().nth(*i).map(|a| a.value().to_owned()).unwrap_or_default()
            }
            XNode::Node(n) if n.is_text() => n.text().unwrap_or("").to_owned(),
            XNode::Node(n) => {
                n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect()
            }
        }
    }

    fn local_name(&self) -> String {
        match self {
            XNode::Attr(n, i) => {
                n.attributes().nth(*i).map(|a| a.name().to_owned()).unwrap_or_default()
            }
            XNode::Node(n) if n.is_element() => n.tag_name().name().to_owned(),
            _ => String::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The expression tree

#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    Child,
    Descendant,
    DescendantOrSelf,
    Itself,
    Parent,
    Ancestor,
    AncestorOrSelf,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
    Attribute,
}

#[derive(Debug, Clone)]
enum Test {
    Name(String),
    Any,
    Text,
    Node,
    Comment,
}

#[derive(Debug, Clone)]
struct Step {
    axis: Axis,
    test: Test,
    predicates: Vec<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone)]
enum Expr {
    Number(f64),
    Text(String),
    /// A path: whether it starts at the root, and its steps.
    Path(bool, Vec<Step>),
    /// A value, filtered by predicates, then followed by steps.
    Filter(Box<Expr>, Vec<Expr>, Vec<Step>),
    Union(Vec<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Negate(Box<Expr>),
    Call(String, Vec<Expr>),
}

// ---------------------------------------------------------------------------------------------------
// Reading a path

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(f64),
    Text(String),
    Name(String),
    Slash,
    DoubleSlash,
    Pipe,
    LBracket,
    RBracket,
    LParen,
    RParen,
    At,
    Dot,
    DotDot,
    Star,
    Comma,
    Op(Op),
    ColonColon,
}

/// Split a path into tokens.
fn tokens(text: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        let (tok, used) = match c {
            c if c.is_whitespace() => {
                at += 1;
                continue;
            }
            '/' if next == Some('/') => (Tok::DoubleSlash, 2),
            '/' => (Tok::Slash, 1),
            '|' => (Tok::Pipe, 1),
            '[' => (Tok::LBracket, 1),
            ']' => (Tok::RBracket, 1),
            '(' => (Tok::LParen, 1),
            ')' => (Tok::RParen, 1),
            '@' => (Tok::At, 1),
            ',' => (Tok::Comma, 1),
            '*' => (Tok::Star, 1),
            '+' => (Tok::Op(Op::Add), 1),
            '-' => (Tok::Op(Op::Sub), 1),
            '=' => (Tok::Op(Op::Eq), 1),
            '!' if next == Some('=') => (Tok::Op(Op::Ne), 2),
            '<' if next == Some('=') => (Tok::Op(Op::Le), 2),
            '<' => (Tok::Op(Op::Lt), 1),
            '>' if next == Some('=') => (Tok::Op(Op::Ge), 2),
            '>' => (Tok::Op(Op::Gt), 1),
            ':' if next == Some(':') => (Tok::ColonColon, 2),
            '.' if next == Some('.') => (Tok::DotDot, 2),
            '.' if !next.is_some_and(|n| n.is_ascii_digit()) => (Tok::Dot, 1),
            '\'' | '"' => string_token(&chars, at)?,
            c if c.is_ascii_digit() || c == '.' => number_token(&chars, at)?,
            c if c.is_alphabetic() || c == '_' => name_token(&chars, at),
            _ => return Err(XPathError),
        };
        out.push(tok);
        at += used;
    }
    Ok(out)
}

fn string_token(chars: &[char], at: usize) -> Result<(Tok, usize)> {
    let quote = chars[at];
    let end = chars[at + 1..].iter().position(|c| *c == quote).ok_or(XPathError)?;
    Ok((Tok::Text(chars[at + 1..at + 1 + end].iter().collect()), end + 2))
}

fn number_token(chars: &[char], at: usize) -> Result<(Tok, usize)> {
    let end = chars[at..]
        .iter()
        .position(|c| !(c.is_ascii_digit() || *c == '.'))
        .unwrap_or(chars.len() - at);
    let text: String = chars[at..at + end].iter().collect();
    Ok((Tok::Number(text.parse().map_err(|_| XPathError)?), end))
}

/// A name: letters, digits, `_`, `-` and `.`, with one `:` allowed for a prefix.
fn name_token(chars: &[char], at: usize) -> (Tok, usize) {
    let mut end = at;
    while let Some(&c) = chars.get(end) {
        let colon = c == ':'
            && chars.get(end + 1) != Some(&':')
            && chars.get(end + 1).is_some_and(|n| n.is_alphabetic());
        if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') || colon {
            end += 1;
        } else {
            break;
        }
    }
    (Tok::Name(chars[at..end].iter().collect()), end - at)
}

/// How deeply brackets and parentheses may nest in a path. The reader is recursive, so this keeps a path of
/// thousands of opening brackets from running it out of stack.
const MOST_NESTED: usize = 64;

/// The deepest nesting of brackets and parentheses in a list of tokens.
fn nesting(toks: &[Tok]) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    for tok in toks {
        match tok {
            Tok::LBracket | Tok::LParen => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            Tok::RBracket | Tok::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

/// Read a path into its tree.
fn parse(text: &str) -> Result<Expr> {
    let toks = tokens(text)?;
    if nesting(&toks) > MOST_NESTED {
        return Err(XPathError);
    }
    let mut p = Parser { toks, at: 0 };
    let expr = p.or()?;
    if p.at != p.toks.len() {
        return Err(XPathError);
    }
    Ok(expr)
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Tok> {
        self.toks.get(self.at + ahead)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == Some(tok) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok) -> Result<()> {
        if self.eat(tok) {
            Ok(())
        } else {
            Err(XPathError)
        }
    }

    /// Whether the next token is the operator word `word` (`and`, `or`, `div`, `mod`).
    fn eat_word(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Name(n)) if n == word) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<Expr> {
        let mut left = self.and()?;
        while self.eat_word("or") {
            left = Expr::Binary(Op::Or, Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr> {
        let mut left = self.equality()?;
        while self.eat_word("and") {
            left = Expr::Binary(Op::And, Box::new(left), Box::new(self.equality()?));
        }
        Ok(left)
    }

    /// One level of binary operators: the tokens that are operators here, read left to right.
    fn level(&mut self, ops: &[Op], next: fn(&mut Parser) -> Result<Expr>) -> Result<Expr> {
        let mut left = next(self)?;
        while let Some(Tok::Op(op)) = self.peek().cloned() {
            if !ops.contains(&op) {
                break;
            }
            self.at += 1;
            left = Expr::Binary(op, Box::new(left), Box::new(next(self)?));
        }
        Ok(left)
    }

    fn equality(&mut self) -> Result<Expr> {
        self.level(&[Op::Eq, Op::Ne], Parser::relational)
    }

    fn relational(&mut self) -> Result<Expr> {
        self.level(&[Op::Lt, Op::Le, Op::Gt, Op::Ge], Parser::additive)
    }

    fn additive(&mut self) -> Result<Expr> {
        self.level(&[Op::Add, Op::Sub], Parser::multiplicative)
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat(&Tok::Star) {
                Op::Mul
            } else if self.eat_word("div") {
                Op::Div
            } else if self.eat_word("mod") {
                Op::Mod
            } else {
                return Ok(left);
            };
            left = Expr::Binary(op, Box::new(left), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr> {
        if self.eat(&Tok::Op(Op::Sub)) {
            return Ok(Expr::Negate(Box::new(self.unary()?)));
        }
        self.union()
    }

    fn union(&mut self) -> Result<Expr> {
        let first = self.path()?;
        if self.peek() != Some(&Tok::Pipe) {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.eat(&Tok::Pipe) {
            parts.push(self.path()?);
        }
        Ok(Expr::Union(parts))
    }

    /// A location path, or a value (a literal, a number, a call, a parenthesised expression) with
    /// predicates and steps after it.
    fn path(&mut self) -> Result<Expr> {
        let primary = match self.peek().cloned() {
            Some(Tok::Number(n)) => Some(Expr::Number(n)),
            Some(Tok::Text(t)) => Some(Expr::Text(t)),
            Some(Tok::LParen) => None,
            Some(Tok::Name(n)) if self.peek_at(1) == Some(&Tok::LParen) && !is_node_type(&n) => {
                None
            }
            _ => return self.location(),
        };
        let value = match primary {
            Some(v) => {
                self.at += 1;
                v
            }
            None => self.primary()?,
        };
        let predicates = self.predicates()?;
        let steps = if matches!(self.peek(), Some(Tok::Slash) | Some(Tok::DoubleSlash)) {
            self.relative_steps(true)?
        } else {
            Vec::new()
        };
        if predicates.is_empty() && steps.is_empty() {
            Ok(value)
        } else {
            Ok(Expr::Filter(Box::new(value), predicates, steps))
        }
    }

    /// A parenthesised expression or a function call.
    fn primary(&mut self) -> Result<Expr> {
        if self.eat(&Tok::LParen) {
            let inner = self.or()?;
            self.expect(&Tok::RParen)?;
            return Ok(inner);
        }
        let Some(Tok::Name(name)) = self.peek().cloned() else { return Err(XPathError) };
        self.at += 2;
        let mut args = Vec::new();
        if !self.eat(&Tok::RParen) {
            loop {
                args.push(self.or()?);
                if self.eat(&Tok::RParen) {
                    break;
                }
                self.expect(&Tok::Comma)?;
            }
        }
        Ok(Expr::Call(name, args))
    }

    fn location(&mut self) -> Result<Expr> {
        if matches!(self.peek(), Some(Tok::Slash) | Some(Tok::DoubleSlash)) {
            let steps = self.relative_steps(true)?;
            return Ok(Expr::Path(true, steps));
        }
        Ok(Expr::Path(false, self.relative_steps(false)?))
    }

    /// Steps separated by `/` and `//`. With `leading` the first step is preceded by one of them.
    fn relative_steps(&mut self, leading: bool) -> Result<Vec<Step>> {
        let mut steps = Vec::new();
        let mut separator = if leading { self.separator() } else { Some(false) };
        if leading && self.at >= self.toks.len() {
            return Ok(steps);
        }
        while let Some(deep) = separator {
            if deep {
                steps.push(Step {
                    axis: Axis::DescendantOrSelf,
                    test: Test::Node,
                    predicates: Vec::new(),
                });
            }
            steps.push(self.step()?);
            separator = self.separator();
        }
        Ok(steps)
    }

    /// A `/` (`Some(false)`) or `//` (`Some(true)`) if one comes next.
    fn separator(&mut self) -> Option<bool> {
        if self.eat(&Tok::Slash) {
            Some(false)
        } else if self.eat(&Tok::DoubleSlash) {
            Some(true)
        } else {
            None
        }
    }

    fn step(&mut self) -> Result<Step> {
        let step = |axis, test| Step { axis, test, predicates: Vec::new() };
        if self.eat(&Tok::Dot) {
            return Ok(step(Axis::Itself, Test::Node));
        }
        if self.eat(&Tok::DotDot) {
            return Ok(step(Axis::Parent, Test::Node));
        }
        let mut axis = Axis::Child;
        if self.eat(&Tok::At) {
            axis = Axis::Attribute;
        } else if matches!(self.peek_at(1), Some(Tok::ColonColon)) {
            let Some(Tok::Name(name)) = self.peek().cloned() else { return Err(XPathError) };
            axis = axis_named(&name)?;
            self.at += 2;
        }
        let test = self.node_test()?;
        Ok(Step { axis, test, predicates: self.predicates()? })
    }

    fn node_test(&mut self) -> Result<Test> {
        if self.eat(&Tok::Star) {
            return Ok(Test::Any);
        }
        let Some(Tok::Name(name)) = self.peek().cloned() else { return Err(XPathError) };
        self.at += 1;
        if is_node_type(&name) && self.eat(&Tok::LParen) {
            self.expect(&Tok::RParen)?;
            return Ok(match name.as_str() {
                "text" => Test::Text,
                "comment" => Test::Comment,
                _ => Test::Node,
            });
        }
        Ok(Test::Name(name))
    }

    fn predicates(&mut self) -> Result<Vec<Expr>> {
        let mut out = Vec::new();
        while self.eat(&Tok::LBracket) {
            out.push(self.or()?);
            self.expect(&Tok::RBracket)?;
        }
        Ok(out)
    }
}

fn is_node_type(name: &str) -> bool {
    matches!(name, "node" | "text" | "comment" | "processing-instruction")
}

fn axis_named(name: &str) -> Result<Axis> {
    Ok(match name {
        "child" => Axis::Child,
        "descendant" => Axis::Descendant,
        "descendant-or-self" => Axis::DescendantOrSelf,
        "self" => Axis::Itself,
        "parent" => Axis::Parent,
        "ancestor" => Axis::Ancestor,
        "ancestor-or-self" => Axis::AncestorOrSelf,
        "following-sibling" => Axis::FollowingSibling,
        "preceding-sibling" => Axis::PrecedingSibling,
        "following" => Axis::Following,
        "preceding" => Axis::Preceding,
        "attribute" => Axis::Attribute,
        _ => return Err(XPathError),
    })
}

// ---------------------------------------------------------------------------------------------------
// Running a path

/// What an expression is worth.
#[derive(Clone)]
enum Val<'a, 'i> {
    Nodes(Vec<XNode<'a, 'i>>),
    Number(f64),
    Text(String),
    Bool(bool),
}

/// Where an expression is being asked: a node, its place among its siblings in the set and the set's size.
#[derive(Clone, Copy)]
struct At<'a, 'i> {
    node: XNode<'a, 'i>,
    position: usize,
    size: usize,
}

/// Run a path against a document. The nodes it selects, in document order, or an error when it selects
/// something that is not a set of nodes.
pub fn select<'a, 'i>(root: Node<'a, 'i>, path: &str) -> Result<Vec<XNode<'a, 'i>>> {
    let expr = parse(path)?;
    let start = At { node: XNode::Node(root), position: 1, size: 1 };
    match eval(&expr, start, root)? {
        Val::Nodes(nodes) => Ok(nodes),
        _ => Err(XPathError),
    }
}

fn eval<'a, 'i>(e: &Expr, at: At<'a, 'i>, root: Node<'a, 'i>) -> Result<Val<'a, 'i>> {
    match e {
        Expr::Number(n) => Ok(Val::Number(*n)),
        Expr::Text(t) => Ok(Val::Text(t.clone())),
        Expr::Path(absolute, steps) => {
            let start = if *absolute { XNode::Node(root) } else { at.node };
            Ok(Val::Nodes(run_steps(vec![start], steps, root)?))
        }
        Expr::Filter(inner, predicates, steps) => {
            let Val::Nodes(nodes) = eval(inner, at, root)? else { return Err(XPathError) };
            let nodes = apply_predicates(nodes, predicates, root)?;
            Ok(Val::Nodes(run_steps(nodes, steps, root)?))
        }
        Expr::Union(parts) => {
            let mut all = Vec::new();
            for part in parts {
                let Val::Nodes(nodes) = eval(part, at, root)? else { return Err(XPathError) };
                all.extend(nodes);
            }
            Ok(Val::Nodes(in_document_order(all)))
        }
        Expr::Negate(inner) => Ok(Val::Number(-number(&eval(inner, at, root)?))),
        Expr::Binary(op, a, b) => binary(*op, eval(a, at, root)?, || eval(b, at, root)),
        Expr::Call(name, args) => call(name, args, at, root),
    }
}

/// Sort nodes into document order and drop repeats.
fn in_document_order<'a, 'i>(mut nodes: Vec<XNode<'a, 'i>>) -> Vec<XNode<'a, 'i>> {
    nodes.sort_by(|a, b| a.order().cmp(&b.order()));
    nodes.dedup();
    nodes
}

fn run_steps<'a, 'i>(
    mut nodes: Vec<XNode<'a, 'i>>,
    steps: &[Step],
    root: Node<'a, 'i>,
) -> Result<Vec<XNode<'a, 'i>>> {
    for step in steps {
        let mut next = Vec::new();
        for node in &nodes {
            let found: Vec<XNode> = axis_nodes(*node, step.axis)
                .into_iter()
                .filter(|n| matches_test(n, &step.test, step.axis))
                .collect();
            next.extend(apply_predicates(found, &step.predicates, root)?);
        }
        nodes = in_document_order(next);
    }
    Ok(nodes)
}

/// Keep the nodes each predicate accepts: a number picks the node at that position, anything else is
/// true or false.
fn apply_predicates<'a, 'i>(
    mut nodes: Vec<XNode<'a, 'i>>,
    predicates: &[Expr],
    root: Node<'a, 'i>,
) -> Result<Vec<XNode<'a, 'i>>> {
    for predicate in predicates {
        let size = nodes.len();
        let mut kept = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            let at = At { node: *node, position: i + 1, size };
            let keep = match eval(predicate, at, root)? {
                Val::Number(n) => n == (i + 1) as f64,
                other => boolean(&other),
            };
            if keep {
                kept.push(*node);
            }
        }
        nodes = kept;
    }
    Ok(nodes)
}

/// The nodes along an axis from a node, in the order the axis counts them (nearest first on a reverse axis).
fn axis_nodes<'a, 'i>(from: XNode<'a, 'i>, axis: Axis) -> Vec<XNode<'a, 'i>> {
    let XNode::Node(n) = from else {
        let XNode::Attr(owner, _) = from else { return Vec::new() };
        return match axis {
            Axis::Itself | Axis::AncestorOrSelf => vec![from],
            Axis::Parent => vec![XNode::Node(owner)],
            Axis::Ancestor => owner.ancestors().map(XNode::Node).collect(),
            _ => Vec::new(),
        };
    };
    let wrap = |it: Vec<Node<'a, 'i>>| it.into_iter().map(XNode::Node).collect::<Vec<_>>();
    match axis {
        Axis::Child => wrap(n.children().collect()),
        Axis::Descendant => wrap(n.descendants().skip(1).collect()),
        Axis::DescendantOrSelf => wrap(n.descendants().collect()),
        Axis::Itself => vec![from],
        Axis::Parent => wrap(n.parent().into_iter().collect()),
        Axis::Ancestor => wrap(n.ancestors().skip(1).collect()),
        Axis::AncestorOrSelf => wrap(n.ancestors().collect()),
        Axis::FollowingSibling => wrap(n.next_siblings().skip(1).collect()),
        Axis::PrecedingSibling => wrap(n.prev_siblings().skip(1).collect()),
        Axis::Following => wrap(
            n.document()
                .descendants()
                .filter(|d| *d > n && !d.ancestors().any(|a| a == n))
                .collect(),
        ),
        Axis::Preceding => {
            let mut before: Vec<_> = n
                .document()
                .descendants()
                .filter(|d| *d < n && !n.ancestors().any(|a| a == *d))
                .collect();
            before.reverse();
            wrap(before)
        }
        Axis::Attribute => (0..n.attributes().len()).map(|i| XNode::Attr(n, i)).collect(),
    }
}

/// Whether a node passes a node test on an axis. A name matches the element's name, or the attribute's on
/// the attribute axis; a prefix on the name is ignored.
fn matches_test(node: &XNode, test: &Test, axis: Axis) -> bool {
    match (node, test) {
        (XNode::Attr(..), Test::Name(name)) => {
            axis == Axis::Attribute && local(name) == node.local_name()
        }
        (XNode::Attr(..), Test::Any | Test::Node) => {
            axis == Axis::Attribute || matches!(test, Test::Node)
        }
        (XNode::Attr(..), _) => false,
        (XNode::Node(n), Test::Name(name)) => n.is_element() && local(name) == n.tag_name().name(),
        (XNode::Node(n), Test::Any) => n.is_element(),
        (XNode::Node(n), Test::Text) => n.is_text(),
        (XNode::Node(n), Test::Comment) => n.is_comment(),
        (XNode::Node(n), Test::Node) => n.is_element() || n.is_text() || n.is_root(),
    }
}

/// The part of a name after its prefix.
fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

// ---------------------------------------------------------------------------------------------------
// Values and operators

fn string(v: &Val) -> String {
    match v {
        Val::Nodes(n) => n.first().map(XNode::string_value).unwrap_or_default(),
        Val::Number(n) => number_text(*n),
        Val::Text(t) => t.clone(),
        Val::Bool(b) => b.to_string(),
    }
}

/// A number as XPath writes it: whole numbers with no point.
fn number_text(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_owned()
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn parse_number(text: &str) -> f64 {
    let t = text.trim();
    let valid = !t.is_empty()
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
        && t.matches('-').count() <= 1
        && !t[1..].contains('-');
    if valid {
        t.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

fn number(v: &Val) -> f64 {
    match v {
        Val::Number(n) => *n,
        Val::Bool(b) => f64::from(u8::from(*b)),
        other => parse_number(&string(other)),
    }
}

fn boolean(v: &Val) -> bool {
    match v {
        Val::Nodes(n) => !n.is_empty(),
        Val::Number(n) => *n != 0.0 && !n.is_nan(),
        Val::Text(t) => !t.is_empty(),
        Val::Bool(b) => *b,
    }
}

fn binary<'a, 'i>(
    op: Op,
    left: Val<'a, 'i>,
    right: impl FnOnce() -> Result<Val<'a, 'i>>,
) -> Result<Val<'a, 'i>> {
    match op {
        Op::Or if boolean(&left) => return Ok(Val::Bool(true)),
        Op::And if !boolean(&left) => return Ok(Val::Bool(false)),
        _ => {}
    }
    let right = right()?;
    Ok(match op {
        Op::Or | Op::And => Val::Bool(boolean(&right)),
        Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            Val::Bool(compare(op, &left, &right))
        }
        Op::Add => Val::Number(number(&left) + number(&right)),
        Op::Sub => Val::Number(number(&left) - number(&right)),
        Op::Mul => Val::Number(number(&left) * number(&right)),
        Op::Div => Val::Number(number(&left) / number(&right)),
        Op::Mod => Val::Number(number(&left) % number(&right)),
    })
}

/// XPath's comparison: a set is true if any of its nodes satisfies the comparison.
fn compare(op: Op, a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Nodes(x), Val::Nodes(y)) => x.iter().any(|p| {
            y.iter()
                .any(|q| compare(op, &Val::Text(p.string_value()), &Val::Text(q.string_value())))
        }),
        (Val::Nodes(x), other) => x.iter().any(|p| compare_atoms(op, &node_as(p, other), other)),
        (other, Val::Nodes(y)) => y.iter().any(|q| compare_atoms(op, other, &node_as(q, other))),
        _ => compare_atoms(op, a, b),
    }
}

/// A node as the kind of value it is being compared with.
fn node_as<'a, 'i>(node: &XNode, like: &Val<'a, 'i>) -> Val<'a, 'i> {
    match like {
        Val::Number(_) => Val::Number(parse_number(&node.string_value())),
        Val::Bool(_) => Val::Bool(true),
        _ => Val::Text(node.string_value()),
    }
}

fn compare_atoms(op: Op, a: &Val, b: &Val) -> bool {
    let equality = matches!(op, Op::Eq | Op::Ne);
    let order = if equality && matches!((a, b), (Val::Bool(_), _) | (_, Val::Bool(_))) {
        boolean(a).cmp(&boolean(b)).into()
    } else if equality && !matches!((a, b), (Val::Number(_), _) | (_, Val::Number(_))) {
        string(a).cmp(&string(b)).into()
    } else {
        number(a).partial_cmp(&number(b))
    };
    matches!(
        (op, order),
        (Op::Eq, Some(Ordering::Equal))
            | (Op::Ne, Some(Ordering::Less | Ordering::Greater) | None)
            | (Op::Lt, Some(Ordering::Less))
            | (Op::Gt, Some(Ordering::Greater))
            | (Op::Le, Some(Ordering::Less | Ordering::Equal))
            | (Op::Ge, Some(Ordering::Greater | Ordering::Equal))
    )
}

/// A function call in a predicate.
fn call<'a, 'i>(
    name: &str,
    args: &[Expr],
    at: At<'a, 'i>,
    root: Node<'a, 'i>,
) -> Result<Val<'a, 'i>> {
    let vals = args.iter().map(|a| eval(a, at, root)).collect::<Result<Vec<_>>>()?;
    let text = |i: usize| vals.get(i).map(string).unwrap_or_else(|| at.node.string_value());
    let num =
        |i: usize| vals.get(i).map(number).unwrap_or_else(|| parse_number(&at.node.string_value()));
    Ok(match (name, vals.len()) {
        ("position", 0) => Val::Number(at.position as f64),
        ("last", 0) => Val::Number(at.size as f64),
        ("count", 1) => match &vals[0] {
            Val::Nodes(n) => Val::Number(n.len() as f64),
            _ => return Err(XPathError),
        },
        ("not", 1) => Val::Bool(!boolean(&vals[0])),
        ("true", 0) => Val::Bool(true),
        ("false", 0) => Val::Bool(false),
        ("boolean", 1) => Val::Bool(boolean(&vals[0])),
        ("string", _) => Val::Text(text(0)),
        ("number", _) => Val::Number(num(0)),
        ("contains", 2) => Val::Bool(text(0).contains(&text(1))),
        ("starts-with", 2) => Val::Bool(text(0).starts_with(&text(1))),
        ("string-length", _) => Val::Number(text(0).chars().count() as f64),
        ("normalize-space", _) => {
            Val::Text(text(0).split_whitespace().collect::<Vec<_>>().join(" "))
        }
        ("concat", n) if n >= 2 => Val::Text((0..n).map(text).collect()),
        ("substring-before", 2) => {
            Val::Text(text(0).split_once(&text(1)).map(|(a, _)| a.to_owned()).unwrap_or_default())
        }
        ("substring-after", 2) => {
            Val::Text(text(0).split_once(&text(1)).map(|(_, b)| b.to_owned()).unwrap_or_default())
        }
        ("name" | "local-name", _) => Val::Text(match vals.first() {
            Some(Val::Nodes(n)) => n.first().map(XNode::local_name).unwrap_or_default(),
            _ => at.node.local_name(),
        }),
        ("sum", 1) => match &vals[0] {
            Val::Nodes(n) => Val::Number(n.iter().map(|x| parse_number(&x.string_value())).sum()),
            _ => return Err(XPathError),
        },
        ("floor", 1) => Val::Number(number(&vals[0]).floor()),
        ("ceiling", 1) => Val::Number(number(&vals[0]).ceil()),
        ("round", 1) => Val::Number(number(&vals[0]).round()),
        _ => return Err(XPathError),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The string values of what a path selects in an XML text.
    fn values(xml: &str, path: &str) -> Option<Vec<String>> {
        let doc = roxmltree::Document::parse(xml).ok()?;
        let nodes = select(doc.root(), path).ok()?;
        Some(nodes.iter().map(XNode::string_value).collect())
    }

    #[test]
    fn paths_predicates_and_unions_select_in_document_order() {
        let xml = "<a><b id='x'>1</b><b id='y'>2</b><c>3</c></a>";
        let cases: [(&str, &[&str]); 9] = [
            ("//b", &["1", "2"]),
            ("//b[2]", &["2"]),
            ("//b[last()]", &["2"]),
            ("//b[@id='y']", &["2"]),
            ("//b/@id", &["x", "y"]),
            ("//c | //b", &["1", "2", "3"]),
            ("/a/*", &["1", "2", "3"]),
            ("//b[. > 1]", &["2"]),
            ("//b[1]/following-sibling::*", &["2", "3"]),
        ];
        for (path, want) in cases {
            assert_eq!(values(xml, path).unwrap(), want, "{path}");
        }
        assert!(values(xml, "count(//b)").is_none());
        assert!(values(xml, "//b[").is_none());
        assert!(values(xml, &format!("{}1{}", "(".repeat(5000), ")".repeat(5000))).is_none());
    }
}
