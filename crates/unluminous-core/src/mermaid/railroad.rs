//! Railroad diagrams: `railroad-beta`, `railroad-ebnf-beta`, `railroad-abnf-beta` and
//! `railroad-peg-beta`.
//!
//! A grammar drawn as one track for each rule. The four keywords choose a notation, and all four are
//! read into the same expression tree (`Expr`), so there is one layout and one painter:
//!
//! - **EBNF** in the W3C style (`A?`, `A*`, `A+`, `A B`) and the ISO 14977 style (`[ A ]`, `{ A }`,
//!   `A , B`, `? special ?`, `A - B`), with `=` or `::=` and `(* *)` or `/* */` comments;
//! - **ABNF** with `/`, the repetition prefixes `*A`, `1*A`, `2*4A` and `3A`, `%x41` style values, and
//!   `=/` to add alternatives to a rule already written;
//! - **PEG** with `<-`, ordered choice `/`, the predicates `&A` and `!A`, `.` and `#` comments;
//! - **the intermediate form** written as constructor calls, such as `sequence(terminal("a"), ...)`.
//!
//! Every rule ends with `;`. `Source::read` splits the text into lines, so a rule that runs over
//! several lines is put back together here before it is read, and each token keeps the number of the
//! line it came from for an error message.
//!
//! Decisions a reader might disagree with:
//!
//! - **A repetition is drawn as a loop under the item**, and a repetition that may happen zero times is
//!   an optional wrapped round that loop, so there is one drawing of a loop and one of a bypass.
//! - **`A - B`, `&A` and `!A` are drawn as a dashed box** holding the words `except B`, `followed by A`
//!   or `not followed by A`. A railroad has no standard picture for them.
//! - **Only the top level sequence of a rule is wrapped** onto further lines when it is too wide. A wide
//!   choice is left wide, because breaking one apart would hide that its branches are alternatives.
//! - **Nothing is refused for naming a rule that is never defined.** A grammar excerpt very often
//!   refers to rules written somewhere else.

use crate::style::CharStyle;

use super::parts;
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How wide a rule's top level sequence may grow before it is wrapped onto another line.
const WRAP_LIMIT: f32 = 560.0;
/// The track between two items in a sequence.
const GAP: f32 = 20.0;
/// The room a choice leaves at each end for its curves.
const CHOICE_PAD: f32 = 20.0;
/// The room a loop leaves at each end for its curves.
const LOOP_PAD: f32 = 32.0;
/// The radius of the curves in a choice.
const CHOICE_RADIUS: f32 = 8.0;
/// The radius of the curves in a loop and in a wrapped rule.
const LOOP_RADIUS: f32 = 10.0;
/// The space between one branch of a choice and the next.
const BRANCH_GAP: f32 = 12.0;
/// The smallest distance a branch or a loop is put below the main track, so its curves have room.
const LEAST_DROP: f32 = 34.0;
/// The room between two rows of a wrapped rule.
const ROW_GAP: f32 = 28.0;
/// The room between one rule and the next.
const RULE_GAP: f32 = 34.0;
/// The track between the start mark and the first item, and the last item and the end mark.
const LEAD: f32 = 28.0;
/// The room a wrapped rule keeps at each side for the line that returns to the next row.
const RETURN_PAD: f32 = 24.0;
/// How far a nested construct may go, so a line of opening brackets cannot overflow the stack.
const MAX_DEPTH: usize = 48;
/// The most characters a predicate or an exception shows of what it is about.
const DESCRIPTION_LIMIT: usize = 36;

// ---------------------------------------------------------------------------------------------
// The expression tree.
// ---------------------------------------------------------------------------------------------

/// What a rule is made of, in every notation.
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Terminal(String),
    NonTerminal(String),
    Special(String),
    Sequence(Vec<Expr>),
    Choice(Vec<Expr>),
    Optional(Box<Expr>),
    /// `min` to `max` repetitions of `body`, with something between them when `separator` is set.
    Repeat {
        body: Box<Expr>,
        separator: Option<Box<Expr>>,
        min: u32,
        max: Option<u32>,
    },
    Except(Box<Expr>, Box<Expr>),
    And(Box<Expr>),
    Not(Box<Expr>),
}

impl Expr {
    /// A sequence, or the item itself when there is only one.
    fn sequence(mut items: Vec<Expr>) -> Expr {
        if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::Sequence(items)
        }
    }

    /// A choice, or the alternative itself when there is only one.
    fn choice(mut alternatives: Vec<Expr>) -> Expr {
        if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            Expr::Choice(alternatives)
        }
    }

    /// A repetition, written as the simpler form when it is exactly one or at most one.
    fn repeat(body: Expr, separator: Option<Expr>, min: u32, max: Option<u32>) -> Expr {
        if separator.is_none() && (min, max) == (1, Some(1)) {
            return body;
        }
        if separator.is_none() && (min, max) == (0, Some(1)) {
            return Expr::Optional(Box::new(body));
        }
        Expr::Repeat { body: Box::new(body), separator: separator.map(Box::new), min, max }
    }
}

/// One rule.
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    name: String,
    expr: Expr,
}

/// A problem found while reading, before it is tied to a line of the source.
#[derive(Debug, Clone, PartialEq)]
struct Fault {
    line: usize,
    reason: String,
}

fn fault(line: usize, reason: impl Into<String>) -> Fault {
    Fault { line, reason: reason.into() }
}

/// Which notation the keyword asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Notation {
    Ir,
    Ebnf,
    Abnf,
    Peg,
}

/// The notation a diagram keyword names, ignoring case.
fn notation_of(keyword: &str) -> Notation {
    let keyword = keyword.to_ascii_lowercase();
    if keyword.contains("ebnf") {
        Notation::Ebnf
    } else if keyword.contains("abnf") {
        Notation::Abnf
    } else if keyword.contains("peg") {
        Notation::Peg
    } else {
        Notation::Ir
    }
}

// ---------------------------------------------------------------------------------------------
// Tokens.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Ident(String),
    Str(String),
    Num(u32),
    Sym(char),
    /// `::=` or `<-`.
    Assign,
    /// `? text ?` in EBNF and `<text>` in ABNF.
    Special(String),
    /// An ABNF value such as `%x30-39`, or a PEG character class such as `[a-z]`.
    Raw(String),
}

#[derive(Debug, Clone, PartialEq)]
struct Tok {
    kind: Kind,
    line: usize,
}

/// Turns the characters of the rules, each with its line number, into tokens.
struct Lexer<'a> {
    chars: &'a [(char, usize)],
    at: usize,
    notation: Notation,
    toks: Vec<Tok>,
}

/// Read all the rules' text into tokens.
fn lex(chars: &[(char, usize)], notation: Notation) -> Result<Vec<Tok>, Fault> {
    let mut lexer = Lexer { chars, at: 0, notation, toks: Vec::new() };
    lexer.run()?;
    Ok(lexer.toks)
}

impl Lexer<'_> {
    fn char_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).map(|(character, _)| *character)
    }

    fn line_here(&self) -> usize {
        self.chars.get(self.at).or(self.chars.last()).map_or(1, |(_, line)| *line)
    }

    fn starts_with(&self, words: &str) -> bool {
        words.chars().enumerate().all(|(offset, wanted)| self.char_at(offset) == Some(wanted))
    }

    /// Whether only blanks come before the current character on its line.
    fn first_on_line(&self) -> bool {
        self.chars[..self.at]
            .iter()
            .rev()
            .take_while(|(character, _)| *character != '\n')
            .all(|(character, _)| character.is_whitespace())
    }

    fn run(&mut self) -> Result<(), Fault> {
        while let Some(character) = self.char_at(0) {
            let line = self.line_here();
            if character.is_whitespace() {
                self.at += 1;
                continue;
            }
            if self.comment()? {
                continue;
            }
            let kind = if character == '"' || character == '\'' {
                Some(Kind::Str(self.quoted(character, line)?))
            } else if character.is_ascii_digit() {
                Some(Kind::Num(self.number()))
            } else if character.is_alphabetic() || character == '_' {
                Some(Kind::Ident(self.ident()))
            } else {
                self.symbol(character, line)?
            };
            if let Some(kind) = kind {
                self.toks.push(Tok { kind, line });
            }
        }
        Ok(())
    }

    /// Skip a comment if one starts here, and say whether one did.
    fn comment(&mut self) -> Result<bool, Fault> {
        let line = self.line_here();
        let block = match self.notation {
            Notation::Ebnf if self.starts_with("(*") => Some(("(*", "*)")),
            Notation::Ebnf if self.starts_with("/*") => Some(("/*", "*/")),
            _ => None,
        };
        if let Some((open, close)) = block {
            self.at += 2;
            while self.at < self.chars.len() && !self.starts_with(close) {
                self.at += 1;
            }
            if self.at >= self.chars.len() {
                return Err(fault(line, format!("a comment opened with `{open}` never closes")));
            }
            self.at += 2;
            return Ok(true);
        }
        let to_line_end = match self.notation {
            Notation::Peg => self.char_at(0) == Some('#'),
            Notation::Abnf => self.char_at(0) == Some(';') && self.first_on_line(),
            _ => false,
        };
        if to_line_end {
            while self.char_at(0).is_some_and(|character| character != '\n') {
                self.at += 1;
            }
        }
        Ok(to_line_end)
    }

    /// Read a quoted string, whose opening quote is at the current character.
    fn quoted(&mut self, quote: char, line: usize) -> Result<String, Fault> {
        self.at += 1;
        let mut text = String::new();
        loop {
            match (self.char_at(0), self.char_at(1)) {
                (None, _) => {
                    return Err(fault(line, "quoted text opened here never closes"));
                }
                (Some(character), _) if character == quote => {
                    self.at += 1;
                    return Ok(text);
                }
                (Some('\\'), Some(next)) if next == quote => {
                    text.push(quote);
                    self.at += 2;
                }
                (Some(character), _) => {
                    text.push(character);
                    self.at += 1;
                }
            }
        }
    }

    fn number(&mut self) -> u32 {
        let mut value: u32 = 0;
        while let Some(digit) = self.char_at(0).and_then(|character| character.to_digit(10)) {
            value = value.saturating_mul(10).saturating_add(digit);
            self.at += 1;
        }
        value
    }

    /// A name. A hyphen inside one is part of it in EBNF and ABNF, where `local-part` is ordinary.
    fn ident(&mut self) -> String {
        let hyphens = matches!(self.notation, Notation::Ebnf | Notation::Abnf);
        let mut name = String::new();
        while let Some(character) = self.char_at(0) {
            let joins = character == '-'
                && hyphens
                && self.char_at(1).is_some_and(|next| next.is_alphanumeric() || next == '_');
            if character.is_alphanumeric() || character == '_' || joins {
                name.push(character);
                self.at += 1;
            } else {
                break;
            }
        }
        name
    }

    /// Everything that is not a name, a number or a string. `None` when the characters were only
    /// punctuation that carries no meaning of its own, such as the `%s` before a quoted string.
    fn symbol(&mut self, character: char, line: usize) -> Result<Option<Kind>, Fault> {
        match (self.notation, character) {
            (_, ':') if self.starts_with("::=") => {
                self.at += 3;
                Ok(Some(Kind::Assign))
            }
            (Notation::Peg, '<') if self.char_at(1) == Some('-') => {
                self.at += 2;
                Ok(Some(Kind::Assign))
            }
            (Notation::Abnf, '<') => {
                self.read_until('>', line, "prose between `<` and `>`").map(Some)
            }
            (Notation::Abnf, '%') => self.abnf_value(),
            (Notation::Peg, '[') => self.character_class(line).map(Some),
            (Notation::Ebnf, '?') if self.opens_special() => {
                self.at += 1;
                let words = self.read_until('?', line, "a special sequence")?;
                Ok(Some(words))
            }
            (
                _,
                '=' | '|' | '/' | ',' | '(' | ')' | '[' | ']' | '{' | '}' | '?' | '*' | '+' | '&'
                | '!' | '.' | '-' | ';',
            ) => {
                self.at += 1;
                Ok(Some(Kind::Sym(character)))
            }
            _ => Err(fault(line, format!("`{character}` is not part of this notation"))),
        }
    }

    /// Read up to `close`, giving a special sequence. The opening character is already consumed
    /// for EBNF and is the current character for ABNF.
    fn read_until(&mut self, close: char, line: usize, what: &str) -> Result<Kind, Fault> {
        if self.notation == Notation::Abnf {
            self.at += 1;
        }
        let mut words = String::new();
        while let Some(character) = self.char_at(0) {
            self.at += 1;
            if character == close {
                return Ok(Kind::Special(words.trim().to_owned()));
            }
            words.push(if character == '\n' { ' ' } else { character });
        }
        Err(fault(line, format!("{what} opened here never closes")))
    }

    /// A PEG character class such as `[a-z]`, kept as written.
    fn character_class(&mut self, line: usize) -> Result<Kind, Fault> {
        let mut text = String::new();
        while let Some(character) = self.char_at(0) {
            self.at += 1;
            text.push(character);
            if character == ']' {
                return Ok(Kind::Raw(text));
            }
        }
        Err(fault(line, "a character class opened here never closes"))
    }

    /// An ABNF value such as `%x41`, `%d65` or `%x30-39`, or the `%s` or `%i` of a quoted string.
    fn abnf_value(&mut self) -> Result<Option<Kind>, Fault> {
        let marks_a_string = self.char_at(1).is_some_and(|c| "sSiI".contains(c))
            && matches!(self.char_at(2), Some('"' | '\''));
        if marks_a_string {
            self.at += 2;
            return Ok(None);
        }
        let mut text = String::from("%");
        self.at += 1;
        while let Some(character) = self.char_at(0) {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                text.push(character);
                self.at += 1;
            } else {
                break;
            }
        }
        Ok(Some(Kind::Raw(text)))
    }

    /// Whether a `?` here starts an EBNF special sequence rather than ending an optional item.
    ///
    /// Where an item is expected it can only be a special sequence. After an item it is one only when
    /// it is set apart by a space and another `?` follows before the rule ends.
    fn opens_special(&self) -> bool {
        let after_item = matches!(
            self.toks.last().map(|tok| &tok.kind),
            Some(
                Kind::Ident(_)
                    | Kind::Str(_)
                    | Kind::Special(_)
                    | Kind::Sym(')' | ']' | '}' | '?' | '*' | '+')
            )
        );
        if !after_item {
            return true;
        }
        let spaced = self.at > 0 && self.chars[self.at - 1].0.is_whitespace();
        let closes = self.chars[self.at + 1..]
            .iter()
            .map(|(character, _)| *character)
            .take_while(|character| *character != ';')
            .any(|character| character == '?');
        spaced && closes
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing.
// ---------------------------------------------------------------------------------------------

type Parsed<T> = Result<T, Fault>;

/// Reads the tokens of one rule.
struct Parser {
    toks: Vec<Tok>,
    at: usize,
    notation: Notation,
    depth: usize,
    /// The line the rule began on, used when a rule ends sooner than it should.
    line: usize,
}

/// Split the tokens at each `;` and read every rule.
fn parse_rules(toks: Vec<Tok>, notation: Notation) -> Parsed<Vec<Rule>> {
    let mut rules: Vec<Rule> = Vec::new();
    for statement in toks.split(|tok| tok.kind == Kind::Sym(';')) {
        let Some(first) = statement.first() else {
            continue;
        };
        let mut parser =
            Parser { toks: statement.to_vec(), at: 0, notation, depth: 0, line: first.line };
        let (rule, adds) = parser.rule()?;
        match rules.iter_mut().find(|known| adds && known.name == rule.name) {
            Some(known) => {
                let earlier = std::mem::replace(&mut known.expr, Expr::Sequence(Vec::new()));
                let mut all = match earlier {
                    Expr::Choice(alternatives) => alternatives,
                    single => vec![single],
                };
                match rule.expr {
                    Expr::Choice(more) => all.extend(more),
                    single => all.push(single),
                }
                known.expr = Expr::choice(all);
            }
            None => rules.push(rule),
        }
    }
    Ok(rules)
}

impl Parser {
    fn peek(&self) -> Option<&Kind> {
        self.toks.get(self.at).map(|tok| &tok.kind)
    }

    fn here(&self) -> usize {
        self.toks.get(self.at).map_or(self.line, |tok| tok.line)
    }

    fn eat(&mut self, symbol: char) -> bool {
        let found = self.peek() == Some(&Kind::Sym(symbol));
        if found {
            self.at += 1;
        }
        found
    }

    fn expect(&mut self, symbol: char) -> Parsed<()> {
        if self.eat(symbol) {
            Ok(())
        } else {
            Err(fault(self.here(), format!("expected `{symbol}` here")))
        }
    }

    /// Go one level deeper into brackets or calls, refusing when it is absurdly deep.
    fn enter(&mut self) -> Parsed<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(fault(self.here(), "this is nested too deeply to draw"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// `name = definition`. The flag says the rule adds alternatives (`=/`) to an earlier one.
    fn rule(&mut self) -> Parsed<(Rule, bool)> {
        let shape = "a rule looks like `name = definition ;`";
        let Some(Kind::Ident(name)) = self.peek().cloned() else {
            return Err(fault(self.here(), shape));
        };
        self.at += 1;
        if !self.eat('=') && self.peek() != Some(&Kind::Assign) {
            return Err(fault(self.here(), shape));
        }
        if self.peek() == Some(&Kind::Assign) {
            self.at += 1;
        }
        let adds = self.notation == Notation::Abnf && self.eat('/');
        let expr = match self.notation {
            Notation::Ir => self.ir_expr()?,
            Notation::Ebnf => self.ebnf_choice()?,
            Notation::Abnf | Notation::Peg => self.slash_choice()?,
        };
        if let Some(tok) = self.toks.get(self.at) {
            return Err(fault(
                tok.line,
                format!("did not expect {} here", describe_token(&tok.kind)),
            ));
        }
        Ok((Rule { name, expr }, adds))
    }

    fn at_sequence_end(&self) -> bool {
        matches!(self.peek(), None | Some(Kind::Sym('|' | '/' | ')' | ']' | '}')))
    }

    // EBNF ---------------------------------------------------------------------------------

    fn ebnf_choice(&mut self) -> Parsed<Expr> {
        let mut alternatives = vec![self.ebnf_sequence()?];
        while self.eat('|') {
            alternatives.push(self.ebnf_sequence()?);
        }
        Ok(Expr::choice(alternatives))
    }

    fn ebnf_sequence(&mut self) -> Parsed<Expr> {
        let mut items = Vec::new();
        while !self.at_sequence_end() {
            if self.eat(',') {
                continue;
            }
            items.push(self.ebnf_term()?);
        }
        Ok(Expr::sequence(items))
    }

    /// An item, and the `- exception` that may follow it.
    fn ebnf_term(&mut self) -> Parsed<Expr> {
        let item = self.ebnf_factor()?;
        if self.eat('-') {
            let excluded = self.ebnf_factor()?;
            return Ok(Expr::Except(Box::new(item), Box::new(excluded)));
        }
        Ok(item)
    }

    /// An item with its `?`, `*` and `+` suffixes, or a count and `*` in front of it.
    fn ebnf_factor(&mut self) -> Parsed<Expr> {
        if let Some(Kind::Num(count)) = self.peek().cloned() {
            self.at += 1;
            self.expect('*')?;
            let item = self.ebnf_primary()?;
            return Ok(Expr::repeat(item, None, count, Some(count)));
        }
        let item = self.ebnf_primary()?;
        self.suffixes(item)
    }

    fn ebnf_primary(&mut self) -> Parsed<Expr> {
        let line = self.here();
        let Some(tok) = self.toks.get(self.at).cloned() else {
            return Err(fault(line, "the rule ends where an item should be"));
        };
        self.at += 1;
        match tok.kind {
            Kind::Ident(name) => Ok(Expr::NonTerminal(name)),
            Kind::Str(words) => Ok(Expr::Terminal(words)),
            Kind::Special(words) => Ok(Expr::Special(words)),
            Kind::Sym('(') => self.group(')', Self::ebnf_choice),
            Kind::Sym('[') => {
                let inner = self.group(']', Self::ebnf_choice)?;
                Ok(Expr::Optional(Box::new(inner)))
            }
            Kind::Sym('{') => {
                let inner = self.group('}', Self::ebnf_choice)?;
                Ok(Expr::repeat(inner, None, 0, None))
            }
            other => Err(fault(
                line,
                format!(
                    "expected a name, a quoted string or a bracket, and found {}",
                    describe_token(&other)
                ),
            )),
        }
    }

    /// The inside of a bracket whose opener was just read, up to its closer.
    fn group(&mut self, close: char, inner: fn(&mut Self) -> Parsed<Expr>) -> Parsed<Expr> {
        self.enter()?;
        let expr = inner(self)?;
        self.expect(close)?;
        self.leave();
        Ok(expr)
    }

    /// The `?`, `*` and `+` after an item.
    fn suffixes(&mut self, mut item: Expr) -> Parsed<Expr> {
        loop {
            item = if self.eat('?') {
                Expr::Optional(Box::new(item))
            } else if self.eat('*') {
                Expr::repeat(item, None, 0, None)
            } else if self.eat('+') {
                Expr::repeat(item, None, 1, None)
            } else {
                return Ok(item);
            };
        }
    }

    // ABNF and PEG -------------------------------------------------------------------------

    fn slash_choice(&mut self) -> Parsed<Expr> {
        let mut alternatives = vec![self.slash_sequence()?];
        while self.eat('/') {
            alternatives.push(self.slash_sequence()?);
        }
        Ok(Expr::choice(alternatives))
    }

    fn slash_sequence(&mut self) -> Parsed<Expr> {
        let mut items = Vec::new();
        while !self.at_sequence_end() {
            items.push(if self.notation == Notation::Abnf {
                self.abnf_repetition()?
            } else {
                self.peg_prefix()?
            });
        }
        Ok(Expr::sequence(items))
    }

    /// `[min] [*] [max] element`, where a count alone means exactly that many.
    fn abnf_repetition(&mut self) -> Parsed<Expr> {
        let first = self.take_count();
        let (min, max) = if self.eat('*') {
            (first.unwrap_or(0), self.take_count())
        } else {
            match first {
                Some(count) => (count, Some(count)),
                None => (1, Some(1)),
            }
        };
        let element = self.abnf_element()?;
        Ok(Expr::repeat(element, None, min, max))
    }

    fn take_count(&mut self) -> Option<u32> {
        let Some(Kind::Num(count)) = self.peek().cloned() else {
            return None;
        };
        self.at += 1;
        Some(count)
    }

    fn abnf_element(&mut self) -> Parsed<Expr> {
        let line = self.here();
        let Some(tok) = self.toks.get(self.at).cloned() else {
            return Err(fault(line, "the rule ends where an item should be"));
        };
        self.at += 1;
        match tok.kind {
            Kind::Ident(name) => Ok(Expr::NonTerminal(name)),
            Kind::Str(words) | Kind::Raw(words) => Ok(Expr::Terminal(words)),
            Kind::Special(words) => Ok(Expr::Special(words)),
            Kind::Sym('(') => self.group(')', Self::slash_choice),
            Kind::Sym('[') => {
                let inner = self.group(']', Self::slash_choice)?;
                Ok(Expr::Optional(Box::new(inner)))
            }
            other => Err(fault(
                line,
                format!(
                    "expected a name, a quoted string or a bracket, and found {}",
                    describe_token(&other)
                ),
            )),
        }
    }

    /// An item with `&` or `!` in front of it.
    fn peg_prefix(&mut self) -> Parsed<Expr> {
        if self.eat('&') {
            return Ok(Expr::And(Box::new(self.peg_prefix()?)));
        }
        if self.eat('!') {
            return Ok(Expr::Not(Box::new(self.peg_prefix()?)));
        }
        let item = self.peg_primary()?;
        self.suffixes(item)
    }

    fn peg_primary(&mut self) -> Parsed<Expr> {
        let line = self.here();
        let Some(tok) = self.toks.get(self.at).cloned() else {
            return Err(fault(line, "the rule ends where an item should be"));
        };
        self.at += 1;
        match tok.kind {
            Kind::Ident(name) => Ok(Expr::NonTerminal(name)),
            Kind::Str(words) | Kind::Raw(words) => Ok(Expr::Terminal(words)),
            Kind::Sym('.') => Ok(Expr::Special("any character".to_owned())),
            Kind::Sym('(') => self.group(')', Self::slash_choice),
            other => Err(fault(
                line,
                format!(
                    "expected a name, a quoted string or a bracket, and found {}",
                    describe_token(&other)
                ),
            )),
        }
    }

    // The intermediate form ----------------------------------------------------------------

    /// A constructor call, or a bare quoted string or name.
    fn ir_expr(&mut self) -> Parsed<Expr> {
        let line = self.here();
        let Some(tok) = self.toks.get(self.at).cloned() else {
            return Err(fault(line, "the rule ends where an expression should be"));
        };
        self.at += 1;
        match tok.kind {
            Kind::Str(words) => Ok(Expr::Terminal(words)),
            Kind::Ident(name) if self.peek() == Some(&Kind::Sym('(')) => {
                self.at += 1;
                self.enter()?;
                let call = self.ir_call(&name, line)?;
                self.leave();
                Ok(call)
            }
            Kind::Ident(name) => Ok(Expr::NonTerminal(name)),
            other => Err(fault(
                line,
                format!(
                    "expected a constructor such as `terminal(\"a\")`, and found {}",
                    describe_token(&other)
                ),
            )),
        }
    }

    /// The arguments of the constructor `name`, whose opening bracket was just read.
    fn ir_call(&mut self, name: &str, line: usize) -> Parsed<Expr> {
        let key: String =
            name.chars().filter(|c| *c != '_').collect::<String>().to_ascii_lowercase();
        if matches!(key.as_str(), "terminal" | "nonterminal" | "special") {
            let words = match self.toks.get(self.at).map(|tok| tok.kind.clone()) {
                Some(Kind::Str(words) | Kind::Ident(words)) => words,
                _ => {
                    return Err(fault(
                        line,
                        format!("`{name}` takes the text in quotes, as in `{name}(\"a\")`"),
                    ))
                }
            };
            self.at += 1;
            self.expect(')')?;
            return Ok(match key.as_str() {
                "terminal" => Expr::Terminal(words),
                "nonterminal" => Expr::NonTerminal(words),
                _ => Expr::Special(words),
            });
        }
        let mut args = Vec::new();
        if self.peek() != Some(&Kind::Sym(')')) {
            args.push(self.ir_expr()?);
            while self.eat(',') {
                args.push(self.ir_expr()?);
            }
        }
        self.expect(')')?;
        build_call(&key, name, args, line)
    }
}

/// Make the expression a constructor call stands for from its arguments.
fn build_call(key: &str, name: &str, mut args: Vec<Expr>, line: usize) -> Parsed<Expr> {
    let count = args.len();
    let wrong = |wanted: &str| fault(line, format!("`{name}` takes {wanted}"));
    match key {
        "sequence" => Ok(Expr::Sequence(args)),
        "choice" => Ok(Expr::choice(args)),
        "optional" if count == 1 => Ok(Expr::Optional(Box::new(args.remove(0)))),
        "zeroormore" | "oneormore" if count == 1 || count == 2 => {
            let min = u32::from(key == "oneormore");
            let separator = if count == 2 { args.pop() } else { None };
            Ok(Expr::repeat(args.remove(0), separator, min, None))
        }
        "except" if count == 2 => {
            let excluded = args.remove(1);
            Ok(Expr::Except(Box::new(args.remove(0)), Box::new(excluded)))
        }
        "and" if count == 1 => Ok(Expr::And(Box::new(args.remove(0)))),
        "not" if count == 1 => Ok(Expr::Not(Box::new(args.remove(0)))),
        "optional" | "and" | "not" => Err(wrong("one argument")),
        "zeroormore" | "oneormore" => Err(wrong("one argument and, if wanted, a separator")),
        "except" => Err(wrong("two arguments")),
        _ => Err(fault(
            line,
            format!(
                "`{name}` is not a constructor. The ones that exist are terminal, nonterminal, \
                 special, sequence, choice, optional, zeroOrMore, oneOrMore, except, and and not."
            ),
        )),
    }
}

/// A token in words, for an error message.
fn describe_token(kind: &Kind) -> String {
    match kind {
        Kind::Ident(name) => format!("`{name}`"),
        Kind::Str(words) => format!("\"{words}\""),
        Kind::Num(number) => format!("`{number}`"),
        Kind::Sym(symbol) => format!("`{symbol}`"),
        Kind::Assign => "an assignment".to_owned(),
        Kind::Special(words) | Kind::Raw(words) => format!("`{words}`"),
    }
}

/// The diagram, read.
#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    rules: Vec<Rule>,
    title: Option<String>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    if diagram.rules.is_empty() {
        return Err(Problem::whole(
            "a railroad diagram needs a rule such as `digit = \"0\" | \"1\" ;`.",
        ));
    }
    Ok(draw(&diagram, source, options))
}

/// Tie a fault to the line of the source it came from.
fn problem(source: &Source, fault: Fault) -> Problem {
    let line = source
        .lines
        .iter()
        .find(|line| line.number == fault.line)
        .cloned()
        .unwrap_or(Line { number: fault.line, text: String::new(), indent: 0 });
    Problem::at(&line, fault.reason)
}

/// Whether the text after a word `title` is the start of a rule named `title`.
fn starts_a_definition(rest: &str) -> bool {
    rest.starts_with('=') || rest.starts_with("::=") || rest.starts_with("<-")
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let notation = notation_of(&source.keyword);
    let mut diagram = Diagram::default();
    let mut chars: Vec<(char, usize)> = Vec::new();
    for line in source.statements() {
        if let Some(rest) = line.after_word("title") {
            if !starts_a_definition(rest) {
                diagram.title = Some(source::label(rest));
                continue;
            }
        }
        chars.extend(line.text.chars().map(|character| (character, line.number)));
        chars.push(('\n', line.number));
    }
    let toks = lex(&chars, notation).map_err(|found| problem(source, found))?;
    diagram.rules = parse_rules(toks, notation).map_err(|found| problem(source, found))?;
    Ok(diagram)
}

// ---------------------------------------------------------------------------------------------
// Layout.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Terminal,
    NonTerminal,
    Special,
}

/// A piece of track, measured. The main track runs `up` below the top and `down` above the bottom.
#[derive(Debug, Clone)]
struct Node {
    width: f32,
    up: f32,
    down: f32,
    kind: NodeKind,
}

#[derive(Debug, Clone)]
enum NodeKind {
    /// Bare track.
    Empty,
    Box {
        shape: Shape,
        label: Label,
    },
    Sequence(Vec<Node>),
    /// Branches stacked down the page, each `offsets[i]` below the main track.
    Choice {
        branches: Vec<Node>,
        offsets: Vec<f32>,
    },
    /// The body on the main track and the way back `back` below it.
    Loop {
        body: Box<Node>,
        separator: Option<Box<Node>>,
        caption: Option<Label>,
        back: f32,
    },
}

/// What layout needs to know about the options.
struct Ctx<'a, 'b> {
    options: &'a Options<'b>,
    style: CharStyle,
    small: CharStyle,
}

impl Node {
    fn empty() -> Node {
        Node { width: 0.0, up: 0.0, down: 0.0, kind: NodeKind::Empty }
    }

    /// Items one after another, or the item itself when there is one, or bare track when none.
    fn sequence(items: Vec<Node>) -> Node {
        let mut items: Vec<Node> =
            items.into_iter().filter(|node| !matches!(node.kind, NodeKind::Empty)).collect();
        match items.len() {
            0 => return Node::empty(),
            1 => return items.remove(0),
            _ => {}
        }
        let width =
            items.iter().map(|node| node.width).sum::<f32>() + GAP * (items.len() - 1) as f32;
        let up = items.iter().map(|node| node.up).fold(0.0, f32::max);
        let down = items.iter().map(|node| node.down).fold(0.0, f32::max);
        Node { width, up, down, kind: NodeKind::Sequence(items) }
    }

    /// Branches one under another; the first is on the main track.
    fn choice(branches: Vec<Node>) -> Node {
        let mut offsets = vec![0.0_f32];
        for index in 1..branches.len() {
            let previous = &branches[index - 1];
            let mut offset = offsets[index - 1] + previous.down + BRANCH_GAP + branches[index].up;
            if index == 1 {
                offset = offset.max(LEAST_DROP * 0.8);
            }
            offsets.push(offset);
        }
        let widest = branches.iter().map(|node| node.width).fold(0.0, f32::max);
        let last = branches.len().saturating_sub(1);
        let down = branches.last().map_or(0.0, |node| offsets[last] + node.down);
        let up = branches.first().map_or(0.0, |node| node.up);
        Node {
            width: widest + CHOICE_PAD * 2.0,
            up,
            down,
            kind: NodeKind::Choice { branches, offsets },
        }
    }

    /// A body repeated, with the way back under it.
    fn looped(body: Node, separator: Option<Node>, caption: Option<Label>) -> Node {
        let (sep_up, sep_down) = separator.as_ref().map_or((0.0, 0.0), |node| (node.up, node.down));
        let inner = body.width.max(separator.as_ref().map_or(0.0, |node| node.width));
        let back = (body.down + BRANCH_GAP + sep_up).max(LEAST_DROP);
        let caption_height = caption.as_ref().map_or(0.0, |label| label.height + 3.0);
        Node {
            width: inner + LOOP_PAD * 2.0,
            up: body.up,
            down: back + sep_down + caption_height,
            kind: NodeKind::Loop {
                body: Box::new(body),
                separator: separator.map(Box::new),
                caption,
                back,
            },
        }
    }
}

/// A box round `words`.
fn boxed(shape: Shape, words: &str, ctx: &Ctx) -> Node {
    let made = parts::measure_box(words, &ctx.style, ctx.options.metrics, Size::new(32.0, 30.0));
    Node {
        width: made.rect.width,
        up: made.rect.height / 2.0,
        down: made.rect.height / 2.0,
        kind: NodeKind::Box { shape, label: made.label },
    }
}

/// The words a predicate or an exception shows for what it is about.
fn describe(expr: &Expr) -> String {
    let words = match expr {
        Expr::Terminal(words) => format!("\"{words}\""),
        Expr::NonTerminal(name) => name.clone(),
        Expr::Special(words) => format!("? {words} ?"),
        Expr::Sequence(items) => items.iter().map(describe).collect::<Vec<_>>().join(" "),
        Expr::Choice(items) => items.iter().map(describe).collect::<Vec<_>>().join(" | "),
        Expr::Optional(inner) => format!("[{}]", describe(inner)),
        Expr::Repeat { body, .. } => format!("{{{}}}", describe(body)),
        Expr::Except(kept, excluded) => format!("{} - {}", describe(kept), describe(excluded)),
        Expr::And(inner) => format!("&{}", describe(inner)),
        Expr::Not(inner) => format!("!{}", describe(inner)),
    };
    if words.chars().count() > DESCRIPTION_LIMIT {
        let kept: String = words.chars().take(DESCRIPTION_LIMIT - 1).collect();
        format!("{kept}…")
    } else {
        words
    }
}

/// Measure an expression as track.
fn layout(expr: &Expr, ctx: &Ctx) -> Node {
    match expr {
        Expr::Terminal(words) => boxed(Shape::Terminal, words, ctx),
        Expr::NonTerminal(name) => boxed(Shape::NonTerminal, name, ctx),
        Expr::Special(words) => boxed(Shape::Special, words, ctx),
        Expr::Sequence(items) => {
            Node::sequence(flatten(items).iter().map(|item| layout(item, ctx)).collect())
        }
        Expr::Choice(alternatives) => {
            Node::choice(alternatives.iter().map(|item| layout(item, ctx)).collect())
        }
        Expr::Optional(inner) => Node::choice(vec![Node::empty(), layout(inner, ctx)]),
        Expr::Repeat { body, separator, min, max } => {
            layout_repeat(body, separator.as_deref(), *min, *max, ctx)
        }
        Expr::Except(kept, excluded) => Node::sequence(vec![
            layout(kept, ctx),
            boxed(Shape::Special, &format!("except {}", describe(excluded)), ctx),
        ]),
        Expr::And(inner) => boxed(Shape::Special, &format!("followed by {}", describe(inner)), ctx),
        Expr::Not(inner) => {
            boxed(Shape::Special, &format!("not followed by {}", describe(inner)), ctx)
        }
    }
}

/// The items of a sequence with the sequences inside it spliced in, since `(a b) c` is `a b c`.
fn flatten(items: &[Expr]) -> Vec<Expr> {
    let mut flat = Vec::new();
    for item in items {
        match item {
            Expr::Sequence(inner) => flat.extend(flatten(inner)),
            other => flat.push(other.clone()),
        }
    }
    flat
}

/// A repetition: a loop, inside a bypass when it may happen no times.
fn layout_repeat(
    body: &Expr,
    separator: Option<&Expr>,
    min: u32,
    max: Option<u32>,
    ctx: &Ctx,
) -> Node {
    let words = match (min, max) {
        (0, None) | (1, None) => None,
        (low, Some(high)) if low == high => Some(format!("{low} times")),
        (0, Some(high)) => Some(format!("up to {high} times")),
        (low, Some(high)) => Some(format!("{low} to {high} times")),
        (low, None) => Some(format!("at least {low} times")),
    };
    let caption =
        words.map(|words| text::measure_unwrapped(&words, &ctx.small, ctx.options.metrics));
    let looped = Node::looped(layout(body, ctx), separator.map(|item| layout(item, ctx)), caption);
    if min == 0 {
        Node::choice(vec![Node::empty(), looped])
    } else {
        looped
    }
}

// ---------------------------------------------------------------------------------------------
// Rules: wrapping, and the marks at each end.
// ---------------------------------------------------------------------------------------------

/// One rule, measured.
struct Block {
    name: String,
    name_label: Label,
    rows: Vec<Node>,
    /// The widest row.
    widest: f32,
}

impl Block {
    fn wrapped(&self) -> bool {
        self.rows.len() > 1
    }

    /// The room beside the rows for the marks and, when wrapped, the return line.
    fn side(&self) -> f32 {
        6.0 + LEAD + if self.wrapped() { RETURN_PAD } else { 0.0 }
    }

    fn width(&self) -> f32 {
        (self.widest + self.side() * 2.0).max(self.name_label.width)
    }

    /// How tall everything under the name is, which is every row and the room between rows.
    fn track_height(&self) -> f32 {
        let rows: f32 = self.rows.iter().map(|row| row.up + row.down).sum();
        rows + ROW_GAP * (self.rows.len() - 1) as f32
    }
}

/// Measure a rule, wrapping its top level sequence when it is too wide.
fn layout_rule(rule: &Rule, ctx: &Ctx) -> Block {
    let nodes: Vec<Node> = match &rule.expr {
        Expr::Sequence(items) => flatten(items).iter().map(|item| layout(item, ctx)).collect(),
        other => vec![layout(other, ctx)],
    };
    let rows = pack_rows(nodes);
    let widest = rows.iter().map(|row| row.width).fold(0.0, f32::max);
    let name_style = ctx.options.style(1.0, true);
    Block {
        name: rule.name.clone(),
        name_label: text::measure_unwrapped(&rule.name, &name_style, ctx.options.metrics),
        rows,
        widest,
    }
}

/// Put the items on rows no wider than the limit, each row at least one item.
fn pack_rows(nodes: Vec<Node>) -> Vec<Node> {
    let mut rows: Vec<Node> = Vec::new();
    let mut current: Vec<Node> = Vec::new();
    let mut width = 0.0_f32;
    for node in nodes {
        if !current.is_empty() && width + GAP + node.width > WRAP_LIMIT {
            rows.push(Node::sequence(std::mem::take(&mut current)));
            width = 0.0;
        }
        width += node.width + if current.is_empty() { 0.0 } else { GAP };
        current.push(node);
    }
    rows.push(Node::sequence(current));
    rows
}

// ---------------------------------------------------------------------------------------------
// Painting.
// ---------------------------------------------------------------------------------------------

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let ctx = Ctx { options, style: options.style(1.0, false), small: options.style(0.8, false) };
    let blocks: Vec<Block> = diagram.rules.iter().map(|rule| layout_rule(rule, &ctx)).collect();
    let widest = blocks.iter().map(Block::width).fold(0.0, f32::max);
    let width = parts::MARGIN * 2.0 + widest;
    let mut scene = Scene::new();
    let mut titled = source.clone();
    if titled.title.is_none() {
        titled.title.clone_from(&diagram.title);
    }
    let mut y = parts::title(&mut scene, &titled, options, width) + parts::MARGIN;
    for block in &blocks {
        y += paint_rule(&mut scene, &ctx, block, y) + RULE_GAP;
    }
    scene.claim(Rect::new(0.0, 0.0, width, y - RULE_GAP + parts::MARGIN));
    parts::finish(&mut scene);
    scene
}

fn track(scene: &mut Scene, ctx: &Ctx, points: Vec<Point>) {
    let flat = points.windows(2).all(|pair| pair[0].distance(pair[1]) < 0.01);
    if points.len() >= 2 && !flat {
        scene.add(Item::Line {
            points,
            stroke: Stroke::new(ctx.options.theme.line, parts::LINE),
            dash: Dash::Solid,
        });
    }
}

fn curved(scene: &mut Scene, ctx: &Ctx, points: &[Point], radius: f32) {
    track(scene, ctx, parts::rounded(points, radius, 0.0));
}

/// Draw one rule from `top` and say how tall it was.
fn paint_rule(scene: &mut Scene, ctx: &Ctx, block: &Block, top: f32) -> f32 {
    let theme = &ctx.options.theme;
    let name_style = parts::text_style(ctx.options, 1.0, true, theme.text);
    parts::one_line(
        scene,
        &block.name,
        Point::new(parts::MARGIN, top),
        &name_style,
        Anchor::Start,
        block.name_label.width,
    );
    let first = top + block.name_label.height + 6.0;
    let left = parts::MARGIN + block.side();
    let mut tracks = Vec::new();
    let mut y = first;
    for (index, row) in block.rows.iter().enumerate() {
        y += row.up;
        tracks.push(y);
        paint_node(scene, ctx, row, left, y);
        y += row.down + if index + 1 < block.rows.len() { ROW_GAP } else { 0.0 };
    }
    paint_ends(scene, ctx, block, left, &tracks);
    paint_returns(scene, ctx, block, left, &tracks);
    block.name_label.height + 6.0 + block.track_height()
}

/// The start mark, the end mark and the track joining them to the first and last items.
fn paint_ends(scene: &mut Scene, ctx: &Ctx, block: &Block, left: f32, tracks: &[f32]) {
    let theme = &ctx.options.theme;
    let start = Point::new(left - LEAD, tracks[0]);
    let last = block.rows.len() - 1;
    let end = Point::new(left + block.rows[last].width + LEAD, tracks[last]);
    track(scene, ctx, vec![start, Point::new(left, tracks[0])]);
    track(scene, ctx, vec![Point::new(left + block.rows[last].width, tracks[last]), end]);
    scene.add(Item::Circle {
        centre: start,
        radius: 4.0,
        fill: Some(Paint::solid(theme.line)),
        stroke: None,
    });
    scene.add(Item::Circle {
        centre: end,
        radius: 6.0,
        fill: None,
        stroke: Some(Stroke::new(theme.line, parts::LINE)),
    });
    scene.add(Item::Circle {
        centre: end,
        radius: 3.0,
        fill: Some(Paint::solid(theme.line)),
        stroke: None,
    });
}

/// The line from the end of each row but the last back to the start of the next one.
fn paint_returns(scene: &mut Scene, ctx: &Ctx, block: &Block, left: f32, tracks: &[f32]) {
    let right = left + block.widest + RETURN_PAD * 0.75;
    let back = left - RETURN_PAD * 0.75;
    for index in 0..block.rows.len().saturating_sub(1) {
        let (row, next) = (&block.rows[index], &block.rows[index + 1]);
        let middle = ((tracks[index] + row.down) + (tracks[index + 1] - next.up)) / 2.0;
        let path = [
            Point::new(left + row.width, tracks[index]),
            Point::new(right, tracks[index]),
            Point::new(right, middle),
            Point::new(back, middle),
            Point::new(back, tracks[index + 1]),
            Point::new(left, tracks[index + 1]),
        ];
        curved(scene, ctx, &path, LOOP_RADIUS);
    }
}

/// Draw `node` with its main track at height `y` and its left edge at `x`.
fn paint_node(scene: &mut Scene, ctx: &Ctx, node: &Node, x: f32, y: f32) {
    match &node.kind {
        NodeKind::Empty => {}
        NodeKind::Box { shape, label } => {
            let rect = Rect::new(x, y - node.up, node.width, node.up + node.down);
            paint_box(scene, ctx, *shape, label, rect);
        }
        NodeKind::Sequence(items) => paint_sequence(scene, ctx, items, x, y),
        NodeKind::Choice { branches, offsets } => {
            paint_choice(scene, ctx, node.width, branches, offsets, x, y);
        }
        NodeKind::Loop { body, separator, caption, back } => {
            let looped = LoopParts {
                body,
                separator: separator.as_deref(),
                caption: caption.as_ref(),
                back: *back,
            };
            paint_loop(scene, ctx, node.width, &looped, x, y);
        }
    }
}

fn paint_sequence(scene: &mut Scene, ctx: &Ctx, items: &[Node], x: f32, y: f32) {
    let mut at = x;
    for (index, item) in items.iter().enumerate() {
        paint_node(scene, ctx, item, at, y);
        at += item.width;
        if index + 1 < items.len() {
            track(scene, ctx, vec![Point::new(at, y), Point::new(at + GAP, y)]);
            at += GAP;
        }
    }
}

/// Every branch gets a curve down from the main track, its item, and a curve back up.
fn paint_choice(
    scene: &mut Scene,
    ctx: &Ctx,
    width: f32,
    branches: &[Node],
    offsets: &[f32],
    x: f32,
    y: f32,
) {
    let inner_left = x + CHOICE_PAD;
    let inner_right = x + width - CHOICE_PAD;
    let end = x + width;
    for (branch, offset) in branches.iter().zip(offsets) {
        let level = y + offset;
        let branch_end = inner_left + branch.width;
        if *offset == 0.0 {
            track(scene, ctx, vec![Point::new(x, y), Point::new(inner_left, y)]);
            track(scene, ctx, vec![Point::new(branch_end, y), Point::new(end, y)]);
        } else {
            let down = [
                Point::new(x, y),
                Point::new(x + CHOICE_PAD / 2.0, y),
                Point::new(x + CHOICE_PAD / 2.0, level),
                Point::new(inner_left, level),
            ];
            let up = [
                Point::new(branch_end, level),
                Point::new(end - CHOICE_PAD / 2.0, level),
                Point::new(end - CHOICE_PAD / 2.0, y),
                Point::new(end, y),
            ];
            curved(scene, ctx, &down, CHOICE_RADIUS);
            curved(scene, ctx, &up, CHOICE_RADIUS);
            if branch_end < inner_right - 0.01 {
                track(
                    scene,
                    ctx,
                    vec![Point::new(branch_end, level), Point::new(inner_right, level)],
                );
            }
        }
        paint_node(scene, ctx, branch, inner_left, level);
    }
}

/// The parts of a loop, borrowed from its node.
struct LoopParts<'a> {
    body: &'a Node,
    separator: Option<&'a Node>,
    caption: Option<&'a Label>,
    back: f32,
}

/// The body on the main track, and the way back underneath it running right to left.
fn paint_loop(scene: &mut Scene, ctx: &Ctx, width: f32, looped: &LoopParts, x: f32, y: f32) {
    let theme = &ctx.options.theme;
    let body_x = x + LOOP_PAD + (width - LOOP_PAD * 2.0 - looped.body.width) / 2.0;
    let (near, far) = (x + LOOP_PAD / 2.0, x + width - LOOP_PAD / 2.0);
    let low = y + looped.back;
    track(scene, ctx, vec![Point::new(x, y), Point::new(body_x, y)]);
    track(scene, ctx, vec![Point::new(body_x + looped.body.width, y), Point::new(x + width, y)]);
    let path = [
        Point::new(far - LOOP_RADIUS, y),
        Point::new(far, y),
        Point::new(far, low),
        Point::new(near, low),
        Point::new(near, y),
        Point::new(near + LOOP_RADIUS, y),
    ];
    curved(scene, ctx, &path, LOOP_RADIUS);
    let middle = (y + low) / 2.0;
    scene.add(Item::Polygon {
        points: parts::arrow_head(Point::new(near, middle - 5.0), Point::new(0.0, -1.0), 8.0),
        fill: Some(Paint::solid(theme.line)),
        stroke: None,
    });
    paint_node(scene, ctx, looped.body, body_x, y);
    let mut caption_top = low;
    if let Some(separator) = looped.separator {
        let separator_x = x + (width - separator.width) / 2.0;
        paint_node(scene, ctx, separator, separator_x, low);
        caption_top = low + separator.down;
    }
    if let Some(caption) = looped.caption {
        let style = parts::text_style(ctx.options, 0.8, false, theme.dim);
        parts::label_at(
            scene,
            caption,
            Point::new(x + width / 2.0, caption_top + 3.0),
            &style,
            Anchor::Middle,
        );
    }
}

/// A box with its words in it: pill shaped for a terminal, square for a non terminal, dashed for a
/// special sequence.
fn paint_box(scene: &mut Scene, ctx: &Ctx, shape: Shape, label: &Label, rect: Rect) {
    let theme = &ctx.options.theme;
    let (radius, stroke) = match shape {
        Shape::Terminal => {
            (rect.height.min(rect.width) / 2.0, Some(Stroke::new(theme.accent, parts::LINE)))
        }
        Shape::NonTerminal => (2.0, Some(Stroke::new(theme.node_stroke, parts::LINE))),
        Shape::Special => (0.0, None),
    };
    scene.add(Item::Rect { rect, radius, fill: Some(theme.node_fill), stroke });
    if shape == Shape::Special {
        let corners = vec![
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right(), rect.top()),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.left(), rect.bottom()),
            Point::new(rect.left(), rect.top()),
        ];
        scene.add(Item::Line {
            points: corners,
            stroke: Stroke::new(theme.dim, parts::LINE),
            dash: parts::DASH,
        });
    }
    parts::centred_label(
        scene,
        label,
        rect,
        &parts::text_style(ctx.options, 1.0, false, theme.text),
    );
}

#[cfg(test)]
mod tests {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    fn rules(text: &str) -> Vec<Rule> {
        read(&Source::read(text).expect("a diagram")).expect("it should read").rules
    }

    fn expr_of(text: &str) -> Expr {
        rules(text).remove(0).expr
    }

    fn t(words: &str) -> Expr {
        Expr::Terminal(words.to_owned())
    }

    fn n(name: &str) -> Expr {
        Expr::NonTerminal(name.to_owned())
    }

    fn repeat(body: Expr, min: u32, max: Option<u32>) -> Expr {
        Expr::repeat(body, None, min, max)
    }

    #[test]
    fn an_ebnf_choice_and_sequence_are_read() {
        let found = expr_of("railroad-ebnf-beta\nx = \"a\" b | \"c\" ;\n");
        assert_eq!(found, Expr::Choice(vec![Expr::Sequence(vec![t("a"), n("b")]), t("c")]));
    }

    #[test]
    fn the_w3c_suffixes_and_the_iso_brackets_mean_the_same() {
        let w3c = expr_of("railroad-ebnf-beta\nx = a? b* c+ ;\n");
        assert_eq!(
            w3c,
            Expr::Sequence(vec![
                Expr::Optional(Box::new(n("a"))),
                repeat(n("b"), 0, None),
                repeat(n("c"), 1, None)
            ])
        );
        let iso = expr_of("railroad-ebnf-beta\nx = [ a ] , { b } ;\n");
        assert_eq!(
            iso,
            Expr::Sequence(vec![Expr::Optional(Box::new(n("a"))), repeat(n("b"), 0, None)])
        );
    }

    #[test]
    fn ebnf_reads_the_other_assignment_the_comments_the_special_sequence_and_the_exception() {
        let text = "railroad-ebnf-beta\n(* a comment *) x ::= a /* another */ - b ;\n\
            y = ? any visible character ? ;\nz = a ? ;\n";
        let found = rules(text);
        assert_eq!(found[0].expr, Expr::Except(Box::new(n("a")), Box::new(n("b"))));
        assert_eq!(found[1].expr, Expr::Special("any visible character".to_owned()));
        assert_eq!(
            found[2].expr,
            Expr::Optional(Box::new(n("a"))),
            "a `?` after an item is a suffix"
        );
    }

    #[test]
    fn a_rule_may_run_over_several_lines_and_a_rule_may_be_called_title() {
        let text = "railroad-ebnf-beta\ntitle Names\ntitle = a\n  | b\n  | c ;\nnext = d ;\n";
        let diagram = read(&Source::read(text).expect("a diagram")).expect("it reads");
        assert_eq!(diagram.title.as_deref(), Some("Names"));
        assert_eq!(diagram.rules.len(), 2);
        assert_eq!(diagram.rules[0].name, "title");
        assert_eq!(diagram.rules[0].expr, Expr::Choice(vec![n("a"), n("b"), n("c")]));
    }

    #[test]
    fn abnf_repetition_prefixes_are_read() {
        let text = "railroad-abnf-beta\na = *x 1*y 2*4z 3w *2v ;\n";
        let Expr::Sequence(items) = expr_of(text) else {
            panic!("a sequence");
        };
        assert_eq!(items[0], repeat(n("x"), 0, None));
        assert_eq!(items[1], repeat(n("y"), 1, None));
        assert_eq!(items[2], repeat(n("z"), 2, Some(4)));
        assert_eq!(items[3], repeat(n("w"), 3, Some(3)));
        assert_eq!(items[4], repeat(n("v"), 0, Some(2)));
    }

    #[test]
    fn abnf_values_comments_and_added_alternatives_are_read() {
        let text = "railroad-abnf-beta\n; a comment line\ndigit = %x30-39 ;\n\
            digit =/ \"a\" / [ \"b\" ] ;\nname = <prose here> ;\n";
        let found = rules(text);
        assert_eq!(found.len(), 2, "the added alternatives belong to the first rule");
        assert_eq!(
            found[0].expr,
            Expr::Choice(vec![t("%x30-39"), t("a"), Expr::Optional(Box::new(t("b")))])
        );
        assert_eq!(found[1].expr, Expr::Special("prose here".to_owned()));
    }

    #[test]
    fn peg_predicates_suffixes_and_any_are_read() {
        let text = "railroad-peg-beta\n# a comment\nid <- !kw letter+ / &\"x\" . ;\n";
        let found = expr_of(text);
        let Expr::Choice(branches) = found else {
            panic!("a choice");
        };
        assert_eq!(
            branches[0],
            Expr::Sequence(vec![Expr::Not(Box::new(n("kw"))), repeat(n("letter"), 1, None)])
        );
        assert_eq!(
            branches[1],
            Expr::Sequence(vec![
                Expr::And(Box::new(t("x"))),
                Expr::Special("any character".to_owned())
            ])
        );
    }

    #[test]
    fn the_intermediate_form_is_read_from_constructor_calls() {
        let text = "railroad-beta\nx = sequence(\n  terminal(\"a\"),\n  zeroOrMore(nonterminal(\"b\"), terminal(\",\")),\n\
            optional(special(\"s\")), choice(terminal(\"c\"), terminal(\"d\"))\n) ;\n";
        let Expr::Sequence(items) = expr_of(text) else {
            panic!("a sequence");
        };
        assert_eq!(items[0], t("a"));
        assert_eq!(items[1], Expr::repeat(n("b"), Some(t(",")), 0, None));
        assert_eq!(items[2], Expr::Optional(Box::new(Expr::Special("s".to_owned()))));
        assert_eq!(items[3], Expr::Choice(vec![t("c"), t("d")]));
    }

    #[test]
    fn the_keyword_is_matched_without_regard_to_case() {
        assert_eq!(notation_of("Railroad-EBNF-Beta"), Notation::Ebnf);
        assert_eq!(notation_of("railroad-peg-beta"), Notation::Peg);
        assert_eq!(notation_of("railroad-beta"), Notation::Ir);
    }

    #[test]
    fn a_line_that_is_not_a_rule_is_refused_with_its_line_number() {
        let problem = check::refused("railroad-ebnf-beta\nok = \"a\" ;\nbad \"b\" ;\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("name = definition"));
    }

    #[test]
    fn a_string_that_never_closes_and_a_stray_bracket_are_refused() {
        let open = check::refused("railroad-peg-beta\nx <- \"a ;\n", &options());
        assert!(open.reason.contains("never closes"));
        let stray = check::refused("railroad-ebnf-beta\nx = a ) ;\n", &options());
        assert!(stray.reason.contains("did not expect"));
        let missing = check::refused("railroad-ebnf-beta\nx = ( a ;\n", &options());
        assert_eq!(missing.line, Some(2));
    }

    #[test]
    fn an_unknown_constructor_names_the_ones_that_exist() {
        let problem = check::refused("railroad-beta\nx = wobble(terminal(\"a\")) ;\n", &options());
        assert!(problem.reason.contains("zeroOrMore"));
    }

    #[test]
    fn a_diagram_with_no_rules_is_refused_and_a_deep_nest_does_not_overflow() {
        check::refused("railroad-ebnf-beta\ntitle Nothing\n", &options());
        let deep = format!("railroad-ebnf-beta\nx = {}a{} ;\n", "(".repeat(5000), ")".repeat(5000));
        let problem = check::refused(&deep, &options());
        assert!(problem.reason.contains("deeply"));
    }

    #[test]
    fn a_source_cut_off_anywhere_never_panics() {
        let full = "railroad-ebnf-beta\ntitle \"T\"\nx = a ( \"b\" | c )* [ d ] ? s ? - e ;\n";
        for end in 0..=full.len() {
            if let Some(part) = full.get(..end) {
                let _ = super::super::render(part, &options());
            }
        }
    }

    #[test]
    fn a_full_ebnf_grammar_is_drawn_with_every_name_and_nothing_overlapping() {
        let text = "railroad-ebnf-beta\ntitle \"Arithmetic\"\n\
            expression = term ( ( \"+\" | \"-\" ) term )* ;\n\
            term = factor ( ( \"*\" | \"/\" ) factor )* ;\n\
            factor = number | \"(\" expression \")\" ;\nnumber = digit+ ;\n\
            digit = \"0\" | \"1\" | \"2\" ;\n";
        let scene = check::drawn(
            text,
            &options(),
            &["Arithmetic", "expression", "term", "factor", "number", "digit", "+", "("],
        );
        check::no_two_rectangles_overlap(&scene.rects());
    }

    #[test]
    fn a_choice_stacks_its_branches_below_each_other() {
        let scene = check::drawn(
            "railroad-ebnf-beta\nsign = \"+\" | \"-\" | \"~\" ;\n",
            &options(),
            &["+", "-", "~"],
        );
        let rects = scene.rects();
        assert_eq!(rects.len(), 3);
        assert!(rects[0].bottom() < rects[1].top() && rects[1].bottom() < rects[2].top());
    }

    #[test]
    fn a_repetition_has_a_loop_underneath_and_a_caption_when_counted() {
        let plain = check::drawn("railroad-abnf-beta\nx = 1*a ;\n", &options(), &["a"]);
        let counted =
            check::drawn("railroad-abnf-beta\nx = 2*4a ;\n", &options(), &["a", "2 to 4 times"]);
        let loops = |scene: &Scene| {
            scene.items.iter().filter(|item| matches!(item, Item::Polygon { .. })).count()
        };
        assert_eq!(loops(&plain), 1, "the arrowhead on the way back");
        assert_eq!(loops(&counted), 1);
    }

    #[test]
    fn a_separator_is_drawn_on_the_way_back_and_below_the_body() {
        let scene = check::drawn(
            "railroad-beta\nx = oneOrMore(nonterminal(\"item\"), terminal(\",\")) ;\n",
            &options(),
            &["item", ","],
        );
        let rects = scene.rects();
        assert!(rects[1].top() > rects[0].bottom(), "the separator sits under the item");
    }

    #[test]
    fn a_predicate_and_an_exception_are_dashed_boxes_with_words() {
        check::drawn(
            "railroad-peg-beta\nx <- !kw &\"a\" letter ;\n",
            &options(),
            &["not followed by kw", "followed by \"a\""],
        );
        check::drawn("railroad-ebnf-beta\nx = a - b ;\n", &options(), &["except b"]);
    }

    #[test]
    fn a_rule_that_is_too_wide_wraps_onto_a_second_row() {
        let items: Vec<String> = (0..14).map(|index| format!("\"item{index}\"")).collect();
        let text = format!("railroad-ebnf-beta\nlong = {} ;\n", items.join(" "));
        let scene = check::drawn(&text, &options(), &["item0", "item13"]);
        let rects = scene.rects();
        let tops: std::collections::BTreeSet<i32> =
            rects.iter().map(|rect| rect.top() as i32).collect();
        assert!(tops.len() >= 2, "the items sit on more than one row");
        let widest = rects.iter().map(|rect| rect.right()).fold(0.0, f32::max);
        assert!(widest < WRAP_LIMIT + 200.0, "and none runs far past the limit: {widest}");
        check::no_two_rectangles_overlap(&rects);
    }

    #[test]
    fn the_documentation_examples_draw() {
        let examples = [
            "railroad-ebnf-beta\ntitle \"JSON Grammar\"\n\njson = element ;\n\
             element = object | array | string | number | \"true\" | \"false\" | \"null\" ;\n\
             object = \"{\" [ member ( \",\" member )* ] \"}\" ;\n\
             array = \"[\" [ element ( \",\" element )* ] \"]\" ;\nmember = string \":\" element ;\n",
            "railroad-abnf-beta\ntitle \"Email Address\"\n\naddress = local-part \"@\" domain ;\n\
             local-part = 1*( ALPHA / DIGIT / \".\" / \"-\" ) ;\ndomain = label *( \".\" label ) ;\n\
             label = 1*( ALPHA / DIGIT / \"-\" ) ;\n",
            "railroad-peg-beta\ntitle \"Calculator Grammar\"\n\nExpression <- Term ((\"+\" / \"-\") Term)* ;\n\
             Term <- Factor ((\"*\" / \"/\") Factor)* ;\nFactor <- Number / \"(\" Expression \")\" ;\n\
             Number <- Digit+ ;\nDigit <- \"0\" / \"1\" ;\n",
            "railroad-beta\ntitle Expression Grammar\n\nexpression = sequence(\n    nonterminal(\"term\"),\n\
             \x20   zeroOrMore(sequence(\n        choice(terminal(\"+\"), terminal(\"-\")),\n\
             \x20       nonterminal(\"term\")\n    ))\n) ;\nterm = nonterminal(\"x\") ;\n",
        ];
        for example in examples {
            let scene = check::drawn(example, &options(), &[]);
            check::no_two_rectangles_overlap(&scene.rects());
        }
    }
}
