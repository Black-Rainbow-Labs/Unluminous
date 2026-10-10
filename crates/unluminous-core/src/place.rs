//! What kind of place the caret is in, read backwards from it: after a `.`, after a type annotation,
//! at the start of a statement, inside a call's brackets. `task-2231` §5.1.
//!
//! A sibling of `imports::context_at` and `expressions::at`, and like them it reads only the text just
//! before the caret, so a half typed line further up cannot change the answer. It is a heuristic and
//! says so: [`Place::Unknown`] is an answer, and nothing depends on it being right except the order of
//! rows that already matched. What it reads from the language is the manifest's own keys —
//! `language.members`, `language.annotation`, `language.returns` and `language.containers` — so a
//! language that names none of them is always [`Place::Unknown`] or a statement or an expression.

use crate::syntax::Grammar;

/// The kind of place a completion is offered at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Place {
    /// Nothing could be said.
    #[default]
    Unknown,
    /// The start of a statement: the start of a line, or after `{`, `}` or `;`.
    Statement,
    /// Anywhere a value is written that is not one of the others.
    Expression,
    /// Where a type is written: after `:` or `->`, after `impl`, inside `<…>`.
    Type,
    /// Where a pattern is matched: after `case` in a match arm. Not read yet; reserved for a language
    /// that says how its patterns are written.
    Pattern,
    /// Inside an import statement.
    Import,
    /// Inside a call's brackets, after `(` or `,`.
    Argument,
    /// After a member separator, such as `.` or `::`.
    Member,
}

impl Place {
    /// The word the command line prints.
    pub fn name(self) -> &'static str {
        match self {
            Place::Unknown => "unknown",
            Place::Statement => "statement",
            Place::Expression => "expression",
            Place::Type => "type",
            Place::Pattern => "pattern",
            Place::Import => "import",
            Place::Argument => "argument",
            Place::Member => "member",
        }
    }
}

/// The value in front of a member separator: `self.layout.` is the segments `self` and `layout` and
/// the separator `.`. A segment keeps a call's or an index's brackets as written: `Foo::new().` is
/// `Foo` and `new()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receiver {
    pub segments: Vec<String>,
    /// The separator before the caret: `.`, `::` or `?.`.
    pub separator: String,
}

/// How far back the reading looks. A place is decided by what is on the caret's own line and the end
/// of the line before; an expression longer than this is not one anybody completes inside.
const LOOK_BACK: usize = 400;

/// The place a word starting at `start` is being typed in.
///
/// @param text - the document's text
/// @param start - where the word being typed starts: the caret, less the stem
/// @param grammar - the language
pub fn at(text: &str, start: usize, grammar: &Grammar) -> Place {
    if start > text.len() || !text.is_char_boundary(start) {
        return Place::Unknown;
    }
    if crate::imports::context_at(text, start, grammar).is_some() {
        return Place::Import;
    }
    let from = floor(text, start.saturating_sub(LOOK_BACK));
    let before = &text[from..start];
    if member_separator(before, grammar).is_some() {
        return Place::Member;
    }
    let line = before.rsplit('\n').next().unwrap_or(before);
    let lead = line.trim_end();
    if is_type_place(lead, grammar) && !is_a_literal_field(before, lead, grammar) {
        return Place::Type;
    }
    if lead.trim_start().is_empty() || lead.ends_with(['{', '}', ';']) {
        return Place::Statement;
    }
    if innermost_open(line) == Some('(') && lead.ends_with(['(', ',']) {
        return Place::Argument;
    }
    Place::Expression
}

/// The member separator the text ends with, longest first, when there is something in front of it to
/// be a member of: a name, a closing bracket, a quote, or a `?` or `!` after one. `3.` is a number and
/// `..` is a range, so neither is a member access.
///
/// @param before - the text before the word being typed
/// @param grammar - the language
pub fn member_separator<'a>(before: &str, grammar: &'a Grammar) -> Option<&'a str> {
    let mut separators: Vec<&String> = grammar.completion.members.iter().collect();
    separators.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let separator = separators.into_iter().find(|s| before.ends_with(s.as_str()))?;
    let rest = &before[..before.len() - separator.len()];
    if separator == "." && rest.ends_with('.') {
        return None;
    }
    let last = rest.chars().last()?;
    if matches!(last, ')' | ']' | '"' | '\'' | '>' | '?' | '!' | '`') {
        return Some(separator.as_str());
    }
    if !grammar.is_word_character(last, false) {
        return None;
    }
    let word: String =
        rest.chars().rev().take_while(|c| grammar.is_word_character(*c, false)).collect();
    (!word.chars().all(|c| c.is_ascii_digit())).then_some(separator.as_str())
}

/// The receiver before a member separator that the text ends with, or `None`.
///
/// Read backwards over names joined by member separators, each name allowed a call's or an index's
/// brackets after it. It stops at anything else, so `x = a.b.` reads `a` and `b`.
///
/// @param text - the document's text
/// @param start - where the word being typed starts, just after the separator
/// @param grammar - the language
pub fn receiver_at(text: &str, start: usize, grammar: &Grammar) -> Option<Receiver> {
    let from = floor(text, start.min(text.len()).saturating_sub(LOOK_BACK));
    let before = &text[from..start.min(text.len())];
    let separator = member_separator(before, grammar)?.to_owned();
    let mut rest = &before[..before.len() - separator.len()];
    let mut segments = Vec::new();
    loop {
        let (segment, left) = segment_before(rest, grammar)?;
        segments.push(segment);
        rest = left;
        let joined = grammar
            .completion
            .members
            .iter()
            .filter(|s| rest.ends_with(s.as_str()))
            .max_by_key(|s| s.len());
        match joined {
            Some(sep) if !rest[..rest.len() - sep.len()].trim_end().is_empty() => {
                rest = &rest[..rest.len() - sep.len()];
            }
            _ => break,
        }
    }
    segments.reverse();
    Some(Receiver { segments, separator })
}

/// The one segment a text ends with: a name, and any balanced brackets after it. Answers the segment
/// as written and the text before it.
///
/// @param text - the text
/// @param grammar - the language
fn segment_before<'a>(text: &'a str, grammar: &Grammar) -> Option<(String, &'a str)> {
    let mut end = text.len();
    // Brackets after the name: a call or an index, kept as written.
    while text[..end].ends_with([')', ']']) {
        let open = matching_open(&text[..end])?;
        end = open;
    }
    let name_start = text[..end]
        .char_indices()
        .rev()
        .take_while(|(_, c)| grammar.is_word_character(*c, false))
        .last()
        .map(|(i, _)| i)?;
    let segment = text[name_start..].trim();
    (!segment.is_empty()).then(|| (segment.to_owned(), &text[..name_start]))
}

/// Whether a `name:` the caret follows is a field of a literal, whose value comes next, rather than a
/// field of a definition, whose type comes next: Rust's `Layout { width: │` and TypeScript's
/// `{ title: │` against `struct Layout { width: │` and `interface Card { title: │`.
///
/// The `name:` has to start its line or follow `{` or `,`, and the innermost brace still open before it
/// decides: a brace on a line that names a container (`language.containers`) or that follows `:` opens a
/// definition or a type literal; any other brace opens a literal.
///
/// @param before - the text before the word being typed
/// @param lead - the caret's line up to the word, without trailing spaces
/// @param grammar - the language
fn is_a_literal_field(before: &str, lead: &str, grammar: &Grammar) -> bool {
    let Some(mark) = grammar.completion.annotation.as_deref() else { return false };
    let Some(key) = lead.strip_suffix(mark).map(str::trim_end) else { return false };
    let name_start = key
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '$')
        .last()
        .map(|(i, _)| i);
    let Some(name_start) = name_start else { return false };
    let ahead = key[..name_start].trim_end();
    if !(ahead.is_empty() || ahead.ends_with(['{', ','])) {
        return false;
    }
    let Some(open) = innermost_unclosed_brace(before) else { return false };
    let head_start = before[..open].rfind(['\n', ';', '}']).map_or(0, |at| at + 1);
    let head = before[head_start..open].trim();
    let names_a_container = head
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| grammar.completion.containers.iter().any(|c| c == word));
    !(names_a_container || head.ends_with(mark))
}

/// Where the innermost `{` still open at the end of the text is.
///
/// @param text - the text before the caret
fn innermost_unclosed_brace(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices().rev() {
        match c {
            '}' => depth += 1,
            '{' if depth == 0 => return Some(i),
            '{' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Where the bracket that the text's last character closes opens.
///
/// @param text - text ending in `)` or `]`
fn matching_open(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in text.char_indices().rev() {
        match c {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the end of a line is a place a type is written: after the annotation or return mark, after
/// a container word such as `impl`, or inside an unclosed `<`. A `&`, `&mut` or `*` between them and
/// the word changes nothing.
///
/// @param lead - the line up to the word, without trailing spaces
/// @param grammar - the language
fn is_type_place(lead: &str, grammar: &Grammar) -> bool {
    let keys = &grammar.completion;
    let mut lead = lead;
    loop {
        let trimmed = lead.trim_end();
        let stripped = trimmed
            .strip_suffix("mut")
            .filter(|rest| rest.ends_with(|c: char| c == '&' || c.is_whitespace()))
            .or_else(|| trimmed.strip_suffix('&'))
            .or_else(|| trimmed.strip_suffix('*'));
        match stripped {
            Some(rest) => lead = rest,
            None => break,
        }
    }
    let lead = lead.trim_end();
    if let Some(mark) = &keys.annotation {
        if lead.ends_with(mark.as_str()) && !(mark == ":" && lead.ends_with("::")) {
            return true;
        }
    }
    if keys.returns.as_ref().is_some_and(|mark| lead.ends_with(mark.as_str())) {
        return true;
    }
    let last_word: String =
        lead.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let last_word: String = last_word.chars().rev().collect();
    if !last_word.is_empty() && keys.containers.contains(&last_word) {
        return true;
    }
    !keys.containers.is_empty() && innermost_open(lead) == Some('<') && lead.ends_with(['<', ','])
}

/// The innermost bracket a line leaves open, counting `()`, `[]`, `{}` and `<>`.
///
/// @param line - the line
fn innermost_open(line: &str) -> Option<char> {
    let mut stack = Vec::new();
    let bytes = line.as_bytes();
    for (i, c) in line.char_indices() {
        match c {
            '(' | '[' | '{' | '<' => stack.push(c),
            ')' | ']' | '}' => {
                stack.pop();
            }
            // `->` and `=>` are arrows, not brackets.
            '>' if i > 0 && (bytes[i - 1] == b'-' || bytes[i - 1] == b'=') => {}
            '>' if stack.last() == Some(&'<') => {
                stack.pop();
            }
            _ => {}
        }
    }
    stack.last().copied()
}

/// The largest character boundary at or below a byte.
///
/// @param text - the text
/// @param at - the byte
fn floor(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::CompletionKeys;

    /// Rust as far as these questions need it.
    fn rust() -> Grammar {
        Grammar {
            language: "Rust".to_owned(),
            keywords: ["fn", "let", "impl", "use", "pub", "mut"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            line_comment: Some("//".to_owned()),
            strings: vec!['"'],
            operators: "+-*/%=<>!&|^?:;,.#".chars().collect(),
            numbers: true,
            completion: CompletionKeys {
                members: vec![".".to_owned(), "::".to_owned()],
                annotation: Some(":".to_owned()),
                returns: Some("->".to_owned()),
                containers: ["impl", "trait", "struct", "enum", "mod"]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
                ..CompletionKeys::default()
            },
            ..Grammar::default()
        }
    }

    /// The place at the `|` of a line.
    fn place(written: &str) -> Place {
        let start = written.find('|').expect("a caret");
        let text = written.replace('|', "");
        at(&text, start, &rust())
    }

    #[test]
    fn the_places_a_caret_can_be_in_are_told_apart() {
        let table = [
            ("fn f() {\n    self.|", Place::Member),
            ("let x = Layout::|", Place::Member),
            ("let x = foo().bar.|", Place::Member),
            ("fn draw(ui: &mut |", Place::Type),
            ("fn draw() -> |", Place::Type),
            ("impl |", Place::Type),
            ("let v: Vec<|", Place::Type),
            ("let v: HashMap<String, |", Place::Type),
            ("fn f() {\n    |", Place::Statement),
            ("    let a = 1; |", Place::Statement),
            ("    draw(a, |", Place::Argument),
            ("    let a = |", Place::Expression),
            ("    let a = 3.|", Place::Expression),
            ("    for i in 0..|", Place::Expression),
            // A field of a literal takes a value; a field of a definition takes a type.
            ("    let l = Layout {
        width: |", Place::Expression),
            ("    let l = Layout { width: 1.0, height: |", Place::Expression),
            ("pub struct Layout {
    width: |", Place::Type),
            ("fn f() {
    let width: |", Place::Type),
        ];
        for (written, expected) in table {
            assert_eq!(place(written), expected, "{written:?}");
        }
    }

    #[test]
    fn a_receiver_is_the_names_before_the_separator() {
        let read = |written: &str| {
            let start = written.find('|').unwrap();
            let text = written.replace('|', "");
            receiver_at(&text, start, &rust())
        };
        let r = read("    let a = self.layout.|").unwrap();
        assert_eq!(r.segments, vec!["self", "layout"]);
        assert_eq!(r.separator, ".");
        let r = read("    Foo::new().|").unwrap();
        assert_eq!(r.segments, vec!["Foo", "new()"]);
        let r = read("    items[0].|").unwrap();
        assert_eq!(r.segments, vec!["items[0]"]);
        assert_eq!(read("    3.|"), None, "a number is not a receiver");
        assert_eq!(read("    let a = |"), None);
    }
}
