//! ZenUML: a sequence diagram written as method calls.
//!
//! `A.method(args) { B.nested() }` is a synchronous call that activates `A` while its body runs,
//! `A->B: text` is an asynchronous message, and `if`, `while`, `opt`, `par` and `try` are blocks with
//! braces. Underneath it is exactly the picture Mermaid's own sequence diagram draws, so this module
//! does not draw anything: it rewrites the source into sequence diagram text and hands that to
//! [`super::sequence`], which keeps one way of drawing a lifeline rather than two.
//!
//! | ZenUML | Sequence diagram |
//! | --- | --- |
//! | `A`, `@Actor A`, `A as Alice` | a participant, an actor, an alias |
//! | `A->B: text` | `A->>B: text` |
//! | `x = A.m(args) { … }` | the call, `activate A`, the body, `deactivate A`, a dashed reply `x` |
//! | `return x` | a dashed reply from the callee to its caller |
//! | `new A(args)` | `create participant A` and a `«create»` message |
//! | `while`, `for`, `forEach`, `loop` | `loop … end` |
//! | `if … else if … else` | `alt … else … end` |
//! | `opt`, `par` | `opt … end`, `par … and … end` |
//! | `try … catch … finally` | `critical try … option catch … option finally … end` |
//! | `// comment` | a note over the next message |
//!
//! A call written at the top level has nobody calling it. ZenUML draws those from the left edge with
//! a small figure, and Unluminous gives them an actor called `Client` in the leftmost column.

use super::scene::Scene;
use super::source::{self, Source};
use super::{sequence, Options, Problem};

/// The id of the actor that top level calls come from. Not a name a person writes.
const STARTER: &str = "zenumlStarter";

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let text = translate(source)?;
    let translated =
        Source::read(&text).ok_or_else(|| Problem::whole("This ZenUML diagram is empty."))?;
    sequence::render(&translated, options)
}

/// One piece of the source once its braces have been found.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// A statement, with the line it started on.
    Statement(String, usize),
    Comment(String),
    Open,
    Close,
}

/// Split the source into statements and braces.
///
/// A statement ends at a line break, at a `;`, or at a brace, but never inside brackets or quotes, so
/// `A.m(a, { b })` is one statement and `if(x) { A.m() }` on one line is three tokens and a close.
fn tokens(source: &Source) -> Vec<Token> {
    let mut out = Vec::new();
    for line in source.statements() {
        let text = line.text.trim();
        if let Some(comment) = text.strip_prefix("//") {
            // ZenUML sets a comment as markdown, so `**bold**` loses its marks (`task-2339`).
            out.push(Token::Comment(super::source::without_emphasis(comment.trim())));
            continue;
        }
        let mut current = String::new();
        let mut depth = 0_i32;
        let mut quote: Option<char> = None;
        let flush = |current: &mut String, out: &mut Vec<Token>| {
            let statement = current.trim().trim_end_matches(';').trim().to_owned();
            if !statement.is_empty() {
                out.push(Token::Statement(statement, line.number));
            }
            current.clear();
        };
        for character in text.chars() {
            match (character, quote) {
                (c, Some(open)) if c == open => {
                    quote = None;
                    current.push(c);
                }
                (c, Some(_)) => current.push(c),
                ('"' | '\'', None) => {
                    quote = Some(character);
                    current.push(character);
                }
                ('(', None) => {
                    depth += 1;
                    current.push(character);
                }
                (')', None) => {
                    depth -= 1;
                    current.push(character);
                }
                ('{', None) if depth <= 0 => {
                    flush(&mut current, &mut out);
                    out.push(Token::Open);
                }
                ('}', None) if depth <= 0 => {
                    flush(&mut current, &mut out);
                    out.push(Token::Close);
                }
                (';', None) if depth <= 0 => flush(&mut current, &mut out),
                _ => current.push(character),
            }
        }
        flush(&mut current, &mut out);
    }
    out
}

/// What an open brace belongs to.
#[derive(Debug, Clone, PartialEq)]
enum Frame {
    /// A synchronous call's body: who was called, who called it, and what its result is called.
    Call { callee: String, caller: String, result: Option<String> },
    /// A block word: `loop`, `alt`, `opt`, `par` or `critical`, and for `par` how many statements
    /// have been written in it so far, because each one after the first is a branch of its own.
    Block { word: &'static str, statements: usize },
    /// A brace that opened nothing Unluminous understands, kept so its close still matches.
    Plain,
}

/// The participants in the order they appeared, each with its label and whether it is an actor.
#[derive(Debug, Default)]
struct Cast {
    order: Vec<String>,
    labels: Vec<String>,
    actors: Vec<bool>,
}

impl Cast {
    fn meet(&mut self, id: &str) {
        if !id.is_empty() && !self.order.iter().any(|known| known == id) {
            self.order.push(id.to_owned());
            self.labels.push(id.to_owned());
            self.actors.push(false);
        }
    }

    fn declare(&mut self, id: &str, label: &str, actor: bool) {
        self.meet(id);
        if let Some(at) = self.order.iter().position(|known| known == id) {
            self.labels[at] = label.to_owned();
            self.actors[at] |= actor;
        }
    }
}

/// Everything worked out so far: the participants, the lines of the sequence diagram's body, and
/// what is waiting to be attached to the next message.
#[derive(Default)]
struct Writer {
    cast: Cast,
    body: Vec<String>,
    /// A `// comment` to put over the next message.
    comment: Option<String>,
    /// An `@return` or `@reply` that makes the next asynchronous message a dashed reply.
    reply: bool,
    starter: bool,
}

impl Writer {
    fn message(&mut self, from: &str, to: &str, dashed: bool, words: &str) {
        self.cast.meet(from);
        self.cast.meet(to);
        if let Some(comment) = self.comment.take() {
            let over = if from == to { from.to_owned() } else { format!("{from},{to}") };
            self.body.push(format!("Note over {over}: {comment}"));
        }
        let arrow = if dashed { "-->>" } else { "->>" };
        self.body.push(format!("{from}{arrow}{to}: {}", words.replace(['\n', '\r'], " ")));
    }
}

/// Rewrite the whole of a ZenUML source into sequence diagram text.
fn translate(source: &Source) -> Result<String, Problem> {
    let mut writer = Writer::default();
    let mut title = source.title.clone();
    let mut stack: Vec<Frame> = Vec::new();
    // A block's close, held back one token in case the next statement is its `else` or `catch`.
    let mut closing: Option<Frame> = None;
    let tokens = tokens(source);
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].clone();
        index += 1;
        if let Some(frame) = closing.take() {
            if let Token::Statement(text, _) = &token {
                if let Some(divider) = divider(text, &frame) {
                    writer.body.push(divider);
                    if tokens.get(index) == Some(&Token::Open) {
                        index += 1;
                    }
                    stack.push(frame);
                    continue;
                }
            }
            finish(&mut writer, frame);
        }
        match token {
            // Comments on consecutive lines are one note, a line each.
            Token::Comment(words) => {
                writer.comment = Some(match writer.comment.take() {
                    Some(earlier) => format!("{earlier}<br>{words}"),
                    None => words,
                })
            }
            Token::Close => match stack.pop() {
                Some(frame @ Frame::Block { .. }) => closing = Some(frame),
                Some(frame) => finish(&mut writer, frame),
                None => {}
            },
            Token::Open => stack.push(Frame::Plain),
            Token::Statement(text, number) => {
                let opens = tokens.get(index) == Some(&Token::Open);
                if opens {
                    index += 1;
                }
                if let Some(rest) = text.strip_prefix("title ") {
                    title = Some(source::label(rest));
                    continue;
                }
                count_a_branch(&mut writer, &mut stack);
                let frame = statement(&mut writer, &stack, &text, opens).map_err(|reason| {
                    Problem { line: Some(number), text: text.clone(), reason, unsupported: false }
                })?;
                if opens {
                    stack.push(frame.unwrap_or(Frame::Plain));
                }
            }
        }
    }
    if let Some(frame) = closing.take() {
        finish(&mut writer, frame);
    }
    while let Some(frame) = stack.pop() {
        finish(&mut writer, frame);
    }
    Ok(assemble(&writer, title))
}

/// In a `par` block every statement after the first is a branch of its own, divided by `and`.
fn count_a_branch(writer: &mut Writer, stack: &mut [Frame]) {
    if let Some(Frame::Block { word: "par", statements }) = stack.last_mut() {
        if *statements > 0 {
            writer.body.push("and".to_owned());
        }
        *statements += 1;
    }
}

/// The line that divides a block at `} else {`, `} else if (x) {`, `} catch (e) {` or `} finally {`.
fn divider(text: &str, frame: &Frame) -> Option<String> {
    let Frame::Block { word, .. } = frame else {
        return None;
    };
    let lower = text.to_ascii_lowercase();
    match *word {
        "alt" if lower.starts_with("else") => {
            let rest = text[4..].trim();
            let rest = rest.strip_prefix("if").map(str::trim).unwrap_or(rest);
            Some(format!("else {}", condition(rest)).trim().to_owned())
        }
        "critical" if lower.starts_with("catch") => {
            Some(format!("option catch {}", condition(text[5..].trim())).trim().to_owned())
        }
        "critical" if lower.starts_with("finally") => Some("option finally".to_owned()),
        _ => None,
    }
}

/// The words inside a block's brackets: `(is_sick)` is `is_sick`.
fn condition(text: &str) -> String {
    let text = text.trim();
    match text.strip_prefix('(').and_then(|rest| rest.strip_suffix(')')) {
        Some(inner) => inner.trim().to_owned(),
        None => text.to_owned(),
    }
}

/// Close a frame: a call's body deactivates its callee and replies with its result, a block ends.
fn finish(writer: &mut Writer, frame: Frame) {
    match frame {
        Frame::Call { callee, caller, result } => {
            if let Some(result) = result {
                writer.message(&callee, &caller, true, &result);
            }
            writer.body.push(format!("deactivate {callee}"));
        }
        Frame::Block { .. } => writer.body.push("end".to_owned()),
        Frame::Plain => {}
    }
}

/// Who is calling a statement at this point: the callee of the call whose body it is in, or the
/// starter at the top level.
fn caller(writer: &mut Writer, stack: &[Frame]) -> String {
    for frame in stack.iter().rev() {
        if let Frame::Call { callee, .. } = frame {
            return callee.clone();
        }
    }
    writer.starter = true;
    STARTER.to_owned()
}

/// Read one statement, write what it means, and say what a brace after it opens.
fn statement(
    writer: &mut Writer,
    stack: &[Frame],
    text: &str,
    opens: bool,
) -> Result<Option<Frame>, String> {
    let lower = text.to_ascii_lowercase();
    let word =
        lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '@').next().unwrap_or_default();
    match word {
        "if" => {
            writer.body.push(format!("alt {}", condition(&text[2..])).trim().to_owned());
            return Ok(Some(Frame::Block { word: "alt", statements: 0 }));
        }
        "while" | "for" | "foreach" | "loop" => {
            writer.body.push(format!("loop {}", condition(&text[word.len()..])).trim().to_owned());
            return Ok(Some(Frame::Block { word: "loop", statements: 0 }));
        }
        "opt" => {
            writer.body.push(format!("opt {}", condition(&text[3..])).trim().to_owned());
            return Ok(Some(Frame::Block { word: "opt", statements: 0 }));
        }
        "par" => {
            writer.body.push("par".to_owned());
            return Ok(Some(Frame::Block { word: "par", statements: 0 }));
        }
        "try" => {
            writer.body.push("critical try".to_owned());
            return Ok(Some(Frame::Block { word: "critical", statements: 0 }));
        }
        "@return" | "@reply" => {
            writer.reply = true;
            return Ok(None);
        }
        "return" => {
            let words = text[6..].trim();
            if let Some(Frame::Call { callee, caller, .. }) =
                stack.iter().rev().find(|frame| matches!(frame, Frame::Call { .. }))
            {
                let (callee, caller) = (callee.clone(), caller.clone());
                writer.message(&callee, &caller, true, words);
            }
            return Ok(None);
        }
        _ => {}
    }
    if let Some(rest) = text.strip_prefix('@') {
        // `@Actor Alice`, `@Database Store`: the annotation says which figure to draw.
        let (kind, name) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let (id, label) = alias(name);
        writer.cast.declare(&id, &label, kind.eq_ignore_ascii_case("actor"));
        return Ok(None);
    }
    if let Some(at) = text.find("->") {
        if !text[..at].contains('(') {
            return asynchronous(writer, stack, text, at, opens);
        }
    }
    if assignment(text).is_some()
        || text.starts_with("new ")
        || text.contains('(')
        || text.contains('.')
    {
        return synchronous(writer, stack, text, opens);
    }
    if is_name(text) || text.contains(" as ") {
        let (id, label) = alias(text);
        writer.cast.declare(&id, &label, false);
        return Ok(None);
    }
    // A typed declaration such as `BookService bookService` names a participant too.
    if text.split_whitespace().count() == 2 && text.split_whitespace().all(is_name) {
        let id = text.split_whitespace().last().unwrap_or_default();
        writer.cast.declare(id, id, false);
        return Ok(None);
    }
    Err(format!(
        "`{text}` is not ZenUML Unluminous can read. A message looks like `A->B: text` or `A.method()`."
    ))
}

/// True for a bare identifier, which on its own line declares a participant.
fn is_name(text: &str) -> bool {
    let text = source::unquote(text);
    !text.is_empty()
        && text.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ' ' | '.'))
        && !text.contains("  ")
        && text.split_whitespace().count() == 1
}

/// `A as Alice` into its id and its label, or a bare name into itself twice.
fn alias(text: &str) -> (String, String) {
    match text.split_once(" as ") {
        Some((id, label)) => (source::unquote(id.trim()), source::label(label.trim())),
        None => (source::unquote(text.trim()), source::label(text.trim())),
    }
}

/// `A->B: text`, or `Client->A.method() { … }` where the arrow names who makes a call.
fn asynchronous(
    writer: &mut Writer,
    stack: &[Frame],
    text: &str,
    at: usize,
    opens: bool,
) -> Result<Option<Frame>, String> {
    let from = text[..at].trim().to_owned();
    let rest = text[at + 2..].trim();
    let (to, words) = match rest.split_once(':') {
        Some((to, words)) => (to.trim(), words.trim()),
        None => (rest, ""),
    };
    if from.is_empty() || to.is_empty() {
        return Err("a message needs somebody at each end, as in `A->B: text`.".to_owned());
    }
    // `Client->A.method()` is a synchronous call made by `Client`.
    if words.is_empty() && to.contains('.') {
        if let Some(frame) = call(writer, &from, to, None, opens) {
            return Ok(Some(frame));
        }
    }
    let _ = stack;
    let dashed = std::mem::take(&mut writer.reply);
    writer.message(&from, &source::unquote(to), dashed, words);
    Ok(None)
}

/// `x = A.method(args)`, `A.method()`, `new A(args)` and `a = new A()`.
fn synchronous(
    writer: &mut Writer,
    stack: &[Frame],
    text: &str,
    opens: bool,
) -> Result<Option<Frame>, String> {
    let (result, expression) = match assignment(text) {
        Some((left, right)) => {
            // `SomeType a = …` names its result `a`.
            let name = left.split_whitespace().last().unwrap_or(left).to_owned();
            (Some(name), right)
        }
        None => (None, text),
    };
    let expression = expression.trim();
    let from = caller(writer, stack);
    if let Some(rest) = expression.strip_prefix("new ") {
        let rest = rest.trim();
        let (name, args) = match rest.find('(') {
            Some(open) => (rest[..open].trim(), rest[open..].trim()),
            None => (rest, ""),
        };
        if name.is_empty() {
            return Err("`new` needs the name of what is created, as in `new A()`.".to_owned());
        }
        writer.cast.meet(name);
        writer.body.push(format!("create participant {name}"));
        let words = format!("«create» {}", condition(args)).trim().to_owned();
        writer.message(&from, name, false, &words);
        if let Some(result) = &result {
            writer.message(name, &from, true, result);
        }
        if opens {
            writer.body.push(format!("activate {name}"));
            return Ok(Some(Frame::Call { callee: name.to_owned(), caller: from, result: None }));
        }
        return Ok(None);
    }
    Ok(call(writer, &from, expression, result, opens))
}

/// The two sides of `x = A.m()`, when there is an `=` outside brackets that is not part of `==`.
fn assignment(text: &str) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    let bytes = text.as_bytes();
    for (at, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            '=' if depth == 0 => {
                let before = at.checked_sub(1).map(|b| bytes[b]);
                let after = bytes.get(at + 1).copied();
                if before != Some(b'=')
                    && before != Some(b'!')
                    && before != Some(b'<')
                    && before != Some(b'>')
                    && after != Some(b'=')
                {
                    return Some((text[..at].trim(), text[at + 1..].trim()));
                }
            }
            _ => {}
        }
    }
    None
}

/// Write `from` calling `target.method(args)`, and open its body when a brace follows.
fn call(
    writer: &mut Writer,
    from: &str,
    expression: &str,
    result: Option<String>,
    opens: bool,
) -> Option<Frame> {
    let open = expression.find('(').unwrap_or(expression.len());
    let head = &expression[..open];
    let (target, method) = match head.rfind('.') {
        Some(dot) => (head[..dot].trim(), &expression[dot + 1..]),
        // A method with no object in front of it is the caller calling itself.
        None => (from, expression),
    };
    let target = source::unquote(target);
    if target.is_empty() || method.trim().is_empty() {
        return None;
    }
    writer.message(from, &target, false, method.trim());
    if opens {
        writer.body.push(format!("activate {target}"));
        return Some(Frame::Call { callee: target, caller: from.to_owned(), result });
    }
    if let Some(result) = result {
        writer.message(&target, from, true, &result);
    }
    None
}

/// Put the translation together: a title, the participants in order, then the body.
fn assemble(writer: &Writer, title: Option<String>) -> String {
    let mut out = String::new();
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        out.push_str(&format!("---\ntitle: {title}\n---\n"));
    }
    out.push_str("sequenceDiagram\n");
    let uses_starter = writer.body.iter().any(|line| line.contains(STARTER));
    if uses_starter {
        out.push_str(&format!("actor {STARTER} as Client\n"));
    }
    for (index, id) in writer.cast.order.iter().enumerate() {
        if id == STARTER {
            continue;
        }
        let word = if writer.cast.actors[index] { "actor" } else { "participant" };
        out.push_str(&format!("{word} {id} as {}\n", writer.cast.labels[index]));
    }
    for line in &writer.body {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    fn translated(text: &str) -> String {
        translate(&Source::read(text).expect("a diagram")).expect("it should translate")
    }

    #[test]
    fn an_asynchronous_message_becomes_a_solid_arrow() {
        let text = translated(
            "zenuml\n title Demo\n Alice->John: Hello John, how are you?\n John->Alice: Great!\n",
        );
        assert!(text.contains("title: Demo"), "{text}");
        assert!(text.contains("Alice->>John: Hello John, how are you?"), "{text}");
        assert!(text.contains("participant Alice as Alice"));
    }

    #[test]
    fn a_call_with_a_body_activates_its_callee_and_nests_its_callers() {
        let text = translated(
            "zenuml\n A.method() {\n  B.nested_sync_method()\n  B->C: nested async message\n }\n",
        );
        assert!(text.contains("zenumlStarter->>A: method()"), "{text}");
        assert!(text.contains("activate A"));
        assert!(text.contains("A->>B: nested_sync_method()"), "the body's caller is A: {text}");
        assert!(text.contains("B->>C: nested async message"));
        assert!(text.contains("deactivate A"));
        assert!(text.contains("actor zenumlStarter as Client"));
    }

    #[test]
    fn a_result_and_a_return_are_dashed_replies() {
        let text = translated("zenuml\n a = A.SyncMessage()\n A.Other() {\n  return result\n }\n");
        assert!(text.contains("A-->>zenumlStarter: a"), "{text}");
        assert!(text.contains("A-->>zenumlStarter: result"), "{text}");
    }

    #[test]
    fn the_blocks_become_sequence_blocks_with_their_dividers() {
        let text = translated(
            "zenuml\n Alice->Bob: Hello\n if(is_sick) {\n  Bob->Alice: Not so good\n } else {\n  Bob->Alice: Fine\n }\n while(true) {\n  Bob->Alice: Again\n }\n try {\n  Alice->Bob: Book\n } catch {\n  Bob->Alice: Failed\n } finally {\n  Bob->Alice: Done\n }\n",
        );
        for wanted in
            ["alt is_sick", "else", "loop true", "critical try", "option catch", "option finally"]
        {
            assert!(text.contains(wanted), "{wanted} in {text}");
        }
        assert_eq!(text.matches("\nend\n").count(), 3, "{text}");
    }

    #[test]
    fn each_statement_in_a_par_block_is_a_branch() {
        let text = translated(
            "zenuml\n par {\n  Alice->Bob: Hello guys!\n  Alice->John: Hello guys!\n }\n",
        );
        assert!(
            text.contains("par\nAlice->>Bob: Hello guys!\nand\nAlice->>John: Hello guys!\nend"),
            "{text}"
        );
    }

    #[test]
    fn annotators_and_aliases_declare_participants_in_order() {
        let text =
            translated("zenuml\n @Actor Alice\n @Database Bob\n J as John\n Alice->Bob: Hi\n");
        let alice = text.find("actor Alice as Alice").expect("an actor");
        let bob = text.find("participant Bob as Bob").expect("a participant");
        assert!(alice < bob);
        assert!(text.contains("participant J as John"));
    }

    #[test]
    fn creation_makes_a_participant_and_a_create_message() {
        let text = translated("zenuml\n new A1\n new A2(with, parameters)\n");
        assert!(text.contains("create participant A1"));
        assert!(text.contains("zenumlStarter->>A2: «create» with, parameters"), "{text}");
    }

    #[test]
    fn a_comment_becomes_a_note_over_the_next_message() {
        let text = translated("zenuml\n // a comment on a message.\n BookService.getBook()\n");
        assert!(
            text.contains("Note over zenumlStarter,BookService: a comment on a message."),
            "{text}"
        );
    }

    #[test]
    fn the_reply_example_from_the_documentation_draws() {
        check::drawn(
            "zenuml\n title Reply message\n Client->A.method() {\n  B.method() {\n   if(condition) {\n    return x1\n    // return early\n    @return\n    A->Client: x11\n   }\n  }\n  return x2\n }\n",
            &options(),
            &["Reply message", "Client", "A", "B", "method()", "x1", "x11", "x2"],
        );
    }

    #[test]
    fn a_line_that_is_not_zenuml_says_what_one_looks_like() {
        let problem = check::refused("zenuml\n A.m() {\n  ?? what\n }\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("A->B"));
    }
}
