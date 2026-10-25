//! REGEXTEST, REGEXEXTRACT and REGEXREPLACE.
//!
//! Excel runs a PCRE2 flavoured engine with Unicode properties. The pattern is rewritten into the syntax
//! `fancy-regex` reads (`translate`) and then run by it, which supports lookaround, back references, atomic
//! groups and possessive quantifiers, and hands the plain parts to the `regex` crate. A pattern that backtracks
//! for too long, is too large or is not valid is `#VALUE!`, which is also what Excel answers.
//!
//! What was measured against Excel 16, and is reproduced here:
//!
//! * `.` does not match `\n` or `\r`; `$` matches at the end or before a final line break (`\n`, `\r` or
//!   `\r\n`), and with `(?m)` before any `\n` or `\r`; `^` with `(?m)` matches after any `\n` or `\r`.
//! * An unmatched `{` is a literal, and `{,n}` is `{0,n}`.
//! * `[[:alpha:]]` and the other POSIX classes follow Unicode letters and digits.
//! * After an empty match the next search starts one character on, so `x*` finds four matches in `abc`.
//! * A replacement reads `$1`, `$01`, `${1}`, `${name}`, `$<name>`, `$name`, `$0`, `$&`, `` $` ``, `$'`,
//!   `$_` and `$$`, and `\1`, `\n`, `\t`, `\r`, `\xHH`, `\uHHHH`, `\\` and a backslash before a punctuation
//!   mark. Anything else, or a group the pattern does not have, is `#VALUE!`.
//!
//! What is not reproduced: `\K`, recursion `(?R)`, conditionals, `(*VERB)` and the `(?n)` and `(?J)` flags,
//! which are `#VALUE!` here, and the empty then non empty retry PCRE makes after an empty match.

use fancy_regex::{Regex, RegexBuilder};

use super::{int, opt, r, s, text_of, Function};
use crate::eval::{Ctx, Val};
use crate::formula::Expr;
use crate::value::{Array, ErrorKind, Value};

pub const FUNCTIONS: &[Function] = &[
    s(
        "REGEXTEST",
        2,
        3,
        "text, pattern, [case_sensitivity]",
        "TRUE when the text matches the regular expression.",
        |ctx, a| {
            let re = compile(&text_of(&a[1])?, caseless(ctx, a, 2)?)?;
            let text = text_of(&a[0])?;
            Ok(Value::Bool(re.re.is_match(&text).map_err(|_| ErrorKind::Value)?))
        },
    ),
    r(
        "REGEXEXTRACT",
        2,
        4,
        "text, pattern, [return_mode], [case_sensitivity]",
        "The first match, every match, or the capture groups of the first match.",
        extract,
    ),
    s(
        "REGEXREPLACE",
        3,
        5,
        "text, pattern, replacement, [occurrence], [case_sensitivity]",
        "Replaces matches of a regular expression, using $1 for groups.",
        |ctx, a| {
            let re = compile(&text_of(&a[1])?, caseless(ctx, a, 4)?)?;
            let template = Template::parse(&text_of(&a[2])?, &re)?;
            let occurrence = if opt(a, 3).is_some() { int(ctx, &a[3])? } else { 0 };
            replace(&re, &text_of(&a[0])?, &template, occurrence)
        },
    ),
];

/// A compiled pattern with what the replacement and the capture group results need to know about it.
struct Pattern {
    re: Regex,
    /// How many capture groups, not counting the whole match.
    groups: usize,
    /// The name of each group that has one, with its number.
    names: Vec<(String, usize)>,
    /// The pattern held to a match that starts where the text does and is not empty, which is how PCRE
    /// tries again after an empty match before it moves on a character.
    not_empty: Option<Regex>,
}

/// The case sensitivity argument at `at`: 0 (the default) matches case, 1 does not. Anything else is `#VALUE!`.
fn caseless(ctx: &Ctx, args: &[Value], at: usize) -> Result<bool, ErrorKind> {
    match opt(args, at) {
        None => Ok(false),
        Some(v) => match ctx.number(v)?.trunc() as i64 {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ErrorKind::Value),
        },
    }
}

/// Translate and compile a pattern, with limits on its size and on how long it may backtrack.
fn compile(pattern: &str, caseless: bool) -> Result<Pattern, ErrorKind> {
    if pattern.chars().count() > 8192 {
        return Err(ErrorKind::Value);
    }
    let mut translated = Translator::new(pattern).run()?;
    if caseless {
        translated.insert_str(0, "(?i)");
    }
    let re = RegexBuilder::new(&translated)
        .backtrack_limit(400_000)
        .delegate_size_limit(4_000_000)
        .delegate_dfa_size_limit(4_000_000)
        .build()
        .map_err(|_| ErrorKind::Value)?;
    let groups = re.captures_len().saturating_sub(1);
    let names =
        re.capture_names().enumerate().filter_map(|(i, n)| n.map(|n| (n.to_owned(), i))).collect();
    let not_empty = RegexBuilder::new(&format!(r"\A(?:{translated})(?!\A)"))
        .backtrack_limit(400_000)
        .delegate_size_limit(4_000_000)
        .delegate_dfa_size_limit(4_000_000)
        .build()
        .ok();
    Ok(Pattern { re, groups, names, not_empty })
}

// ---------------------------------------------------------------------------------------------------
// Matching

/// One match with the text of each group (`None` for a group that took no part).
struct Found {
    start: usize,
    end: usize,
    groups: Vec<Option<(usize, usize)>>,
}

/// Every match in the text, searching on from the end of one and one character past an empty one, as PCRE
/// does with a global search.
fn all_matches(p: &Pattern, text: &str) -> Result<Vec<Found>, ErrorKind> {
    let mut out = Vec::new();
    let mut at = 0;
    while at <= text.len() {
        let Some(caps) = p.re.captures_from_pos(text, at).map_err(|_| ErrorKind::Value)? else {
            break;
        };
        let whole = caps.get(0).ok_or(ErrorKind::Value)?;
        let groups = (1..=p.groups).map(|i| caps.get(i).map(|m| (m.start(), m.end()))).collect();
        out.push(Found { start: whole.start(), end: whole.end(), groups });
        at = if whole.end() > whole.start() { whole.end() } else { next_char(text, whole.end()) };
        // After an empty match PCRE looks for one that is not empty at the same place before moving on.
        if whole.end() == whole.start() && whole.end() < text.len() {
            let from = whole.end();
            if let Some(again) = &p.not_empty {
                if let Some(caps) = again.captures(&text[from..]).map_err(|_| ErrorKind::Value)? {
                    let m = caps.get(0).ok_or(ErrorKind::Value)?;
                    let groups = (1..=p.groups)
                        .map(|i| caps.get(i).map(|g| (g.start() + from, g.end() + from)))
                        .collect();
                    out.push(Found { start: from, end: from + m.end(), groups });
                    at = from + m.end();
                }
            }
        }
        if out.len() > 1_000_000 {
            return Err(ErrorKind::Value);
        }
    }
    Ok(out)
}

/// The byte offset one character past `at`, or past the end when there is none.
fn next_char(text: &str, at: usize) -> usize {
    text[at.min(text.len())..].chars().next().map_or(text.len() + 1, |c| at + c.len_utf8())
}

// ---------------------------------------------------------------------------------------------------
// REGEXEXTRACT

/// REGEXEXTRACT over arguments that may be ranges: each pair of text and pattern gives one value, and a
/// match list or a group list is cut to its first value when the arguments are arrays.
fn extract(ctx: &mut Ctx, args: &[Expr]) -> Val {
    let vals: Vec<Val> = args.iter().map(|a| ctx.eval(a)).collect();
    let single = vals.iter().all(|v| ctx.dims(v) == Some((1, 1)));
    if single {
        let values: Vec<Value> = vals.iter().map(|v| ctx.element(v, 0, 0)).collect();
        return match extract_one(ctx, &values) {
            Ok(array) if array.data.len() == 1 => Val::Value(array.data[0].clone()),
            Ok(array) => Val::Array(array),
            Err(e) => Val::error(e),
        };
    }
    ctx.lift_vals(vals, |ctx, values| match extract_one(ctx, values) {
        Ok(array) => array.data.first().cloned().unwrap_or(Value::Error(ErrorKind::Calc)),
        Err(e) => Value::Error(e),
    })
}

/// The result of one REGEXEXTRACT: a single value as a 1 by 1 array, or a row of values.
fn extract_one(ctx: &Ctx, a: &[Value]) -> Result<Array, ErrorKind> {
    let text = text_of(&a[0])?;
    let pattern = text_of(&a[1])?;
    let mode = if opt(a, 2).is_some() { ctx.number(&a[2])?.trunc() as i64 } else { 0 };
    let caseless = caseless(ctx, a, 3)?;
    if !(0..=2).contains(&mode) {
        return Err(ErrorKind::Value);
    }
    let p = compile(&pattern, caseless)?;
    if mode == 2 && p.groups == 0 {
        return Err(ErrorKind::Value);
    }
    let found = all_matches(&p, &text)?;
    let first = found.first().ok_or(ErrorKind::NA)?;
    let slice = |range: (usize, usize)| Value::text(&text[range.0..range.1]);
    Ok(match mode {
        0 => Array::single(slice((first.start, first.end))),
        1 => Array::row(found.iter().map(|m| slice((m.start, m.end))).collect()),
        _ => Array::row(
            first.groups.iter().map(|g| g.map_or(Value::Error(ErrorKind::NA), slice)).collect(),
        ),
    })
}

// ---------------------------------------------------------------------------------------------------
// REGEXREPLACE

/// One piece of a replacement.
enum Part {
    Text(String),
    /// A group by number; 0 is the whole match.
    Group(usize),
    Before,
    After,
    Subject,
}

/// A replacement read once against the pattern it goes with.
struct Template(Vec<Part>);

impl Template {
    /// Read a replacement, refusing a group the pattern does not have or an escape it does not know.
    fn parse(text: &str, p: &Pattern) -> Result<Template, ErrorKind> {
        let chars: Vec<char> = text.chars().collect();
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut at = 0;
        while at < chars.len() {
            let (part, used) = match chars[at] {
                '$' => dollar(&chars[at + 1..], p)?,
                '\\' => backslash(&chars[at + 1..], p)?,
                c => {
                    literal.push(c);
                    at += 1;
                    continue;
                }
            };
            match part {
                Part::Text(t) => literal.push_str(&t),
                other => {
                    if !literal.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut literal)));
                    }
                    parts.push(other);
                }
            }
            at += 1 + used;
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        Ok(Template(parts))
    }

    /// The replacement text for one match.
    fn fill(&self, text: &str, m: &Found) -> String {
        let mut out = String::new();
        for part in &self.0 {
            match part {
                Part::Text(t) => out.push_str(t),
                Part::Group(0) => out.push_str(&text[m.start..m.end]),
                Part::Group(i) => {
                    if let Some(Some((a, b))) = m.groups.get(i - 1) {
                        out.push_str(&text[*a..*b]);
                    }
                }
                Part::Before => out.push_str(&text[..m.start]),
                Part::After => out.push_str(&text[m.end..]),
                Part::Subject => out.push_str(text),
            }
        }
        out
    }
}

/// What follows a `$` in a replacement, and how many characters it used.
fn dollar(rest: &[char], p: &Pattern) -> Result<(Part, usize), ErrorKind> {
    match rest.first() {
        Some('$') => Ok((Part::Text("$".to_owned()), 1)),
        Some('&') => Ok((Part::Group(0), 1)),
        Some('`') => Ok((Part::Before, 1)),
        Some('\'') => Ok((Part::After, 1)),
        Some('_') => Ok((Part::Subject, 1)),
        Some('{') => braced(rest, p, '}'),
        Some('<') => {
            let (part, used) = braced(rest, p, '>')?;
            match part {
                Part::Group(_) if rest[1..used - 1].iter().all(char::is_ascii_digit) => {
                    Err(ErrorKind::Value)
                }
                other => Ok((other, used)),
            }
        }
        Some(c) if c.is_ascii_digit() => {
            let digits: String = rest.iter().take_while(|c| c.is_ascii_digit()).collect();
            Ok((group_number(digits.parse().map_err(|_| ErrorKind::Value)?, p)?, digits.len()))
        }
        Some(c) if c.is_alphabetic() => {
            let name: String =
                rest.iter().take_while(|c| c.is_alphanumeric() || **c == '_').collect();
            Ok((group_named(&name, p)?, name.chars().count()))
        }
        _ => Err(ErrorKind::Value),
    }
}

/// `{name}`, `{3}`, or `<name>` after a `$`: the group it names and the characters used.
fn braced(rest: &[char], p: &Pattern, close: char) -> Result<(Part, usize), ErrorKind> {
    let end = rest.iter().position(|c| *c == close).ok_or(ErrorKind::Value)?;
    let inner: String = rest[1..end].iter().collect();
    let part = match inner.parse::<usize>() {
        Ok(n) => group_number(n, p)?,
        Err(_) => group_named(&inner, p)?,
    };
    Ok((part, end + 1))
}

fn group_number(n: usize, p: &Pattern) -> Result<Part, ErrorKind> {
    if n <= p.groups {
        Ok(Part::Group(n))
    } else {
        Err(ErrorKind::Value)
    }
}

fn group_named(name: &str, p: &Pattern) -> Result<Part, ErrorKind> {
    p.names.iter().find(|(n, _)| n == name).map(|(_, i)| Part::Group(*i)).ok_or(ErrorKind::Value)
}

/// What follows a backslash in a replacement, and how many characters it used.
fn backslash(rest: &[char], p: &Pattern) -> Result<(Part, usize), ErrorKind> {
    let text = |s: &str| Ok((Part::Text(s.to_owned()), 1));
    match rest.first() {
        Some('n') => text("\n"),
        Some('t') => text("\t"),
        Some('r') => text("\r"),
        Some(c) if c.is_ascii_punctuation() => text(&c.to_string()),
        Some('x') => hex_escape(&rest[1..], 2),
        Some('u') => hex_escape(&rest[1..], 4),
        Some(c) if c.is_ascii_digit() && *c != '0' => {
            Ok((group_number(c.to_digit(10).unwrap_or(0) as usize, p)?, 1))
        }
        _ => Err(ErrorKind::Value),
    }
}

/// `\xHH` or `\uHHHH`: the character, and the digits used plus the letter.
fn hex_escape(rest: &[char], width: usize) -> Result<(Part, usize), ErrorKind> {
    let digits: String = rest.iter().take(width).collect();
    let code = u32::from_str_radix(&digits, 16).map_err(|_| ErrorKind::Value)?;
    let c = char::from_u32(code).ok_or(ErrorKind::Value)?;
    Ok((Part::Text(c.to_string()), width + 1))
}

/// REGEXREPLACE: every match, or the `occurrence`th counting from the start (positive) or the end (negative).
fn replace(
    p: &Pattern,
    text: &str,
    template: &Template,
    occurrence: i64,
) -> Result<Value, ErrorKind> {
    let found = all_matches(p, text)?;
    let chosen: Vec<&Found> = match occurrence {
        0 => found.iter().collect(),
        n if n > 0 => found.get(n as usize - 1).into_iter().collect(),
        n => found
            .len()
            .checked_sub(n.unsigned_abs() as usize)
            .and_then(|i| found.get(i))
            .into_iter()
            .collect(),
    };
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    for m in chosen {
        out.push_str(&text[done..m.start]);
        out.push_str(&template.fill(text, m));
        done = m.end;
    }
    out.push_str(&text[done..]);
    if out.chars().count() > 32_767 {
        return Err(ErrorKind::Value);
    }
    Ok(Value::text(out))
}

// ---------------------------------------------------------------------------------------------------
// Translating a pattern

/// The flags that change what a piece of the pattern means to the translator.
#[derive(Clone, Copy)]
struct Flags {
    multiline: bool,
    dotall: bool,
    extended: bool,
}

/// Rewrites a PCRE2 pattern into the syntax `fancy-regex` reads.
struct Translator {
    chars: Vec<char>,
    at: usize,
    out: String,
    /// The flags in force, one entry for each open group, the outermost first.
    flags: Vec<Flags>,
    /// How many capture groups have been opened, which a relative back reference counts from.
    groups: usize,
}

/// Where a line break may stand: `\n`, `\r` and the two together.
const BREAK_END: &str = "(?=(?:\\r\\n|[\\n\\r])?\\z)";
const SPACE_H: &str = "\\t \\x{a0}\\x{1680}\\x{180e}\\x{2000}-\\x{200a}\\x{202f}\\x{205f}\\x{3000}";
const SPACE_V: &str = "\\n\\x0b\\x0c\\r\\x{85}\\x{2028}\\x{2029}";

impl Translator {
    fn new(pattern: &str) -> Self {
        let start = Flags { multiline: false, dotall: false, extended: false };
        Self {
            chars: pattern.chars().collect(),
            at: 0,
            out: String::new(),
            flags: vec![start],
            groups: 0,
        }
    }

    /// The flags in force where the translator is.
    fn current(&self) -> Flags {
        self.flags[self.flags.len() - 1]
    }

    fn set_current(&mut self, flags: Flags) {
        let last = self.flags.len() - 1;
        self.flags[last] = flags;
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    /// Translate the whole pattern.
    fn run(mut self) -> Result<String, ErrorKind> {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            match c {
                '\\' => self.escape()?,
                '[' => self.class()?,
                '(' => self.open()?,
                ')' => {
                    if self.flags.len() > 1 {
                        self.flags.pop();
                    }
                    self.out.push(')');
                }
                '.' => {
                    self.out.push_str(if self.current().dotall { "(?s:.)" } else { "[^\\n\\r]" })
                }
                '^' => self.out.push_str(if self.current().multiline {
                    "(?:\\A|(?<=[\\n\\r]))"
                } else {
                    "\\A"
                }),
                '$' => self.out.push_str(if self.current().multiline {
                    "(?=[\\n\\r]|\\z)"
                } else {
                    BREAK_END
                }),
                '{' => self.brace(),
                '#' if self.current().extended => self.skip_comment(),
                other => self.out.push(other),
            }
        }
        Ok(self.out)
    }

    /// A `#` comment in extended mode runs to the end of the line and is dropped.
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            if c == '\n' {
                break;
            }
        }
    }

    /// A `{` is a repeat only as `{n}`, `{n,}`, `{n,m}` or `{,m}`; anywhere else it is a literal brace.
    fn brace(&mut self) {
        let rest: String = self.chars[self.at..].iter().take(24).collect();
        let Some(end) = rest.find('}') else { return self.out.push_str("\\{") };
        let body = &rest[..end];
        let (low, high) = match body.split_once(',') {
            Some((a, b)) => (a, Some(b)),
            None => (body, None),
        };
        let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
        let valid = digits(low)
            && high.is_none_or(digits)
            && !(low.is_empty() && high.is_none_or(str::is_empty));
        if !valid {
            return self.out.push_str("\\{");
        }
        self.at += body.chars().count() + 1;
        let low = if low.is_empty() { "0" } else { low };
        match high {
            None => self.out.push_str(&format!("{{{low}}}")),
            Some(high) => self.out.push_str(&format!("{{{low},{high}}}")),
        }
    }

    /// A group opening: a comment, a flag change, or a group of any other kind.
    fn open(&mut self) -> Result<(), ErrorKind> {
        if self.peek(0) != Some('?') {
            if self.peek(0) != Some('*') {
                self.groups += 1;
            }
            self.flags.push(self.current());
            self.out.push('(');
            return Ok(());
        }
        if self.peek(1) == Some('#') {
            while let Some(c) = self.peek(0) {
                self.at += 1;
                if c == ')' {
                    break;
                }
            }
            return Ok(());
        }
        let letters: String = self.chars[self.at + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic() || **c == '-')
            .collect();
        let after = self.peek(1 + letters.chars().count());
        if !letters.is_empty() && matches!(after, Some(')') | Some(':')) {
            return self.flag_group(&letters, after == Some(')'));
        }
        if self.is_capturing_name() {
            self.groups += 1;
        }
        self.flags.push(self.current());
        self.out.push('(');
        Ok(())
    }

    /// Whether the group being opened is a named capture group, `(?<name>` or `(?P<name>`.
    fn is_capturing_name(&self) -> bool {
        match (self.peek(1), self.peek(2)) {
            (Some('<'), Some(c)) => c != '=' && c != '!',
            (Some('P'), Some('<')) => true,
            _ => false,
        }
    }

    /// `(?flags)` changes the flags of the group it is in; `(?flags:` opens a group with them.
    fn flag_group(&mut self, letters: &str, bare: bool) -> Result<(), ErrorKind> {
        if letters.matches('x').count() > 1 {
            return Err(ErrorKind::Value);
        }
        let mut flags = self.current();
        let (mut on, mut off, mut negative) = (String::new(), String::new(), false);
        for c in letters.chars() {
            match c {
                '-' => negative = true,
                'm' => flags.multiline = !negative,
                's' => flags.dotall = !negative,
                'x' => {
                    flags.extended = !negative;
                    (if negative { &mut off } else { &mut on }).push(c);
                }
                'i' | 'U' => (if negative { &mut off } else { &mut on }).push(c),
                _ => return Err(ErrorKind::Value),
            }
        }
        self.at += 1 + letters.chars().count() + 1;
        let spec = format!("{on}{}{off}", if off.is_empty() { "" } else { "-" });
        if bare {
            self.set_current(flags);
            if !spec.is_empty() {
                self.out.push_str(&format!("(?{spec})"));
            }
        } else {
            self.flags.push(flags);
            self.out.push_str(&format!("(?{spec}:"));
        }
        Ok(())
    }

    /// A backslash escape outside a class.
    fn escape(&mut self) -> Result<(), ErrorKind> {
        let c = self.peek(0).ok_or(ErrorKind::Value)?;
        self.at += 1;
        match c {
            'Q' => self.quoted(),
            'E' => {}
            'Z' => self.out.push_str(BREAK_END),
            'R' => self.out.push_str(&format!("(?:\\r\\n|[{SPACE_V}])")),
            'N' => self.out.push_str("[^\\n\\r]"),
            'h' => self.out.push_str(&format!("[{SPACE_H}]")),
            'H' => self.out.push_str(&format!("[^{SPACE_H}]")),
            'v' => self.out.push_str(&format!("[{SPACE_V}]")),
            'V' => self.out.push_str(&format!("[^{SPACE_V}]")),
            'e' => self.out.push_str("\\x1b"),
            'g' => self.relative_reference()?,
            'K' | 'G' | 'C' | 'X' => return Err(ErrorKind::Value),
            other => {
                self.out.push('\\');
                self.out.push(other);
                self.braced_escape(other)?;
            }
        }
        Ok(())
    }

    /// The braces after `\p`, `\P`, `\x` and `\k` belong to the escape and are copied whole. A `^` after the
    /// brace of a property says the property is negated, which is written with the other letter instead.
    fn braced_escape(&mut self, letter: char) -> Result<(), ErrorKind> {
        if !matches!(letter, 'p' | 'P' | 'x' | 'k') || self.peek(0) != Some('{') {
            return Ok(());
        }
        let Some(end) = self.chars[self.at..].iter().position(|c| *c == '}') else { return Ok(()) };
        let mut inner: String = self.chars[self.at + 1..self.at + end].iter().collect();
        if matches!(letter, 'p' | 'P') {
            if inner.starts_with('^') {
                inner.remove(0);
                self.out.pop();
                self.out.push(if letter == 'p' { 'P' } else { 'p' });
            }
            // The regex syntax reads `IsLatin` as `Latin`; Excel does not know the name.
            if inner.len() > 2 && inner[..2].eq_ignore_ascii_case("is") {
                return Err(ErrorKind::Value);
            }
        }
        self.out.push_str(&format!("{{{inner}}}"));
        self.at += end + 1;
        Ok(())
    }

    /// `\Q...\E`: everything up to `\E` is a literal.
    fn quoted(&mut self) {
        while let Some(c) = self.peek(0) {
            self.at += 1;
            if c == '\\' && self.peek(0) == Some('E') {
                self.at += 1;
                break;
            }
            if c.is_ascii_punctuation() || c.is_whitespace() {
                self.out.push('\\');
            }
            self.out.push(c);
        }
    }

    /// `\g1`, `\g{1}` and `\g{-1}` as the numbered back reference they name.
    fn relative_reference(&mut self) -> Result<(), ErrorKind> {
        let braced = self.peek(0) == Some('{');
        if braced {
            self.at += 1;
        }
        let mut text = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_ascii_digit() || (c == '-' && text.is_empty()) {
                text.push(c);
                self.at += 1;
            } else {
                break;
            }
        }
        if braced {
            if self.peek(0) != Some('}') {
                return Err(ErrorKind::Value);
            }
            self.at += 1;
        }
        let n: i64 = text.parse().map_err(|_| ErrorKind::Value)?;
        let number = if n < 0 { self.groups as i64 + 1 + n } else { n };
        if number < 1 {
            return Err(ErrorKind::Value);
        }
        self.out.push_str(&format!("\\{number}"));
        Ok(())
    }

    /// A character class, copied through to its closing bracket with POSIX names and `\h` and `\v` rewritten.
    fn class(&mut self) -> Result<(), ErrorKind> {
        self.out.push('[');
        let mut depth = 1;
        if self.peek(0) == Some('^') {
            self.at += 1;
            self.out.push('^');
        }
        if self.peek(0) == Some(']') {
            self.at += 1;
            self.out.push_str("\\]");
        }
        while depth > 0 {
            let c = self.peek(0).ok_or(ErrorKind::Value)?;
            self.at += 1;
            match c {
                '[' if self.peek(0) == Some(':') => self.posix()?,
                '[' => {
                    depth += 1;
                    self.out.push('[');
                }
                ']' => {
                    depth -= 1;
                    self.out.push(']');
                }
                '\\' => self.class_escape()?,
                other => self.out.push(other),
            }
        }
        Ok(())
    }

    /// An escape inside a class.
    fn class_escape(&mut self) -> Result<(), ErrorKind> {
        let c = self.peek(0).ok_or(ErrorKind::Value)?;
        self.at += 1;
        match c {
            'Q' => self.quoted(),
            'E' => {}
            'h' => self.out.push_str(SPACE_H),
            'v' => self.out.push_str(SPACE_V),
            'e' => self.out.push_str("\\x1b"),
            'R' | 'H' | 'V' | 'N' | 'K' | 'G' | 'C' | 'X' => return Err(ErrorKind::Value),
            other => {
                self.out.push('\\');
                self.out.push(other);
                self.braced_escape(other)?;
            }
        }
        Ok(())
    }

    /// `[:name:]` inside a class, as the Unicode property the name means.
    fn posix(&mut self) -> Result<(), ErrorKind> {
        let rest: String = self.chars[self.at..].iter().collect();
        let end = rest.find(":]").ok_or(ErrorKind::Value)?;
        let name = &rest[1..end];
        let (negated, name) = match name.strip_prefix('^') {
            Some(n) => (true, n),
            None => (false, name),
        };
        let set = posix_set(name, negated).ok_or(ErrorKind::Value)?;
        self.at += rest[..end + 2].chars().count();
        self.out.push_str(&set);
        Ok(())
    }
}

/// The class contents a POSIX class name stands for, with Unicode letters and digits as PCRE2 reads them
/// when Unicode properties are on.
fn posix_set(name: &str, negated: bool) -> Option<String> {
    let property = |p: &str| format!("\\{}{{{p}}}", if negated { 'P' } else { 'p' });
    let escape = |e: char| format!("\\{}", if negated { e.to_ascii_uppercase() } else { e });
    Some(match name {
        "alpha" => property("Alphabetic"),
        "digit" => property("Nd"),
        "upper" => property("Lu"),
        "lower" => property("Ll"),
        "punct" if !negated => "\\p{P}!-/:-@\\[-`{-~".to_owned(),
        "cntrl" => property("Cc"),
        "space" => escape('s'),
        "word" => escape('w'),
        "alnum" if !negated => "\\p{Alphabetic}\\p{Nd}".to_owned(),
        "xdigit" if !negated => "0-9A-Fa-f".to_owned(),
        "blank" if !negated => SPACE_H.to_owned(),
        "ascii" => {
            if negated {
                "^\\x00-\\x7f".to_owned()
            } else {
                "\\x00-\\x7f".to_owned()
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::functions::testing::{check, eval};
    use crate::value::{ErrorKind, Value};

    #[test]
    fn test_reads_the_pattern_the_way_excel_does() {
        check(&[
            ("REGEXTEST(\"abc\",\"b\")", "TRUE"),
            ("REGEXTEST(\"abc\",\"B\")", "FALSE"),
            ("REGEXTEST(\"abc\",\"B\",1)", "TRUE"),
            ("REGEXTEST(\"foobar\",\"foo(?=bar)\")", "TRUE"),
            ("REGEXTEST(\"a\"&CHAR(10),\"a$\")", "TRUE"),
            ("REGEXTEST(\"a\"&CHAR(10)&CHAR(10),\"a$\")", "FALSE"),
            ("REGEXTEST(\"a{1,2\",\"a{1,2\")", "TRUE"),
            ("REGEXTEST(\"é\",\"[[:alpha:]]\")", "TRUE"),
            ("REGEXTEST(\"abab\",\"(ab)\\1\")", "TRUE"),
            ("REGEXTEST(\"a\"&CHAR(13)&\"b\",\"(?m)^b\")", "TRUE"),
        ]);
        assert_eq!(eval("REGEXTEST(\"abc\",\"(\")"), Value::Error(ErrorKind::Value));
        assert_eq!(eval("REGEXTEST(\"abc\",\"b\",2)"), Value::Error(ErrorKind::Value));
        assert_eq!(
            eval("REGEXTEST(\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab\",\"(a+)+$\")"),
            Value::Error(ErrorKind::Value)
        );
    }

    #[test]
    fn extract_and_replace_follow_the_return_modes_and_the_templates() {
        check(&[
            ("REGEXEXTRACT(\"abc123def456\",\"\\d+\")", "123"),
            ("COLUMNS(REGEXEXTRACT(\"abc123def456\",\"(\\w)(\\d)\",1))", "4"),
            ("INDEX(REGEXEXTRACT(\"abc123def456\",\"(\\w)(\\d)\",2),1,2)", "1"),
            ("COLUMNS(REGEXEXTRACT(\"abc\",\"x*\",1))", "4"),
            ("REGEXREPLACE(\"abcb\",\"b\",\"X\",-2)", "aXcb"),
            ("REGEXREPLACE(\"abc\",\"(a)(b)\",\"$2$1\")", "bac"),
            ("REGEXREPLACE(\"abc\",\"(a)(b)\",\"\\2\\1\")", "bac"),
            ("REGEXREPLACE(\"abc\",\"\",\"-\")", "-a-b-c-"),
            ("REGEXREPLACE(\"aaa\",\"a*\",\"X\")", "XX"),
            ("REGEXREPLACE(\"abc\",\"(?<n>b)\",\"[$<n>]\")", "a[b]c"),
            ("REGEXREPLACE(\"abc\",\"(b)\",\"$01\")", "abc"),
        ]);
        assert_eq!(eval("REGEXEXTRACT(\"abc\",\"\\d+\")"), Value::Error(ErrorKind::NA));
        assert_eq!(eval("REGEXEXTRACT(\"abc\",\"\\d+\",2)"), Value::Error(ErrorKind::Value));
        assert_eq!(eval("REGEXREPLACE(\"abc\",\"(b)\",\"$10\")"), Value::Error(ErrorKind::Value));
        assert_eq!(eval("REGEXREPLACE(\"abc\",\"(b)\",\"$\")"), Value::Error(ErrorKind::Value));
    }
}
