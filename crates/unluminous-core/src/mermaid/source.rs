//! Reading the source: the things that are true of every diagram type, done once.
//!
//! Mermaid is line-oriented. Almost every statement in every diagram type is one line, and the
//! things that wrap round all of them — front matter, directives, comments, quoting, indentation —
//! are the same wherever they appear. So they are here rather than in twenty parsers that would
//! each get them slightly differently.
//!
//! What is skipped, and why each one is skipped rather than honoured:
//!
//! - **Front matter**, a `---` block at the top. `title:` is kept and becomes the diagram's title.
//!   `config:` is not: Unluminous has one palette, read out of the design, and a document does not get to
//!   choose the window's colours.
//! - **Directives**, `%%{init: {...}}%%`. The same reason.
//! - **Comments**, a line whose first non-blank characters are `%%`.
//! - **`accTitle:` and `accDescr:`**. `accTitle` becomes the title when there is no other one, which
//!   is what it is for.
//!
//! Indentation is kept, as a count of columns with a tab worth four, because `mindmap`, `treemap`
//! and `kanban` are structured by it. Mermaid's own rule is that only the comparison with the
//! previous line matters and not the absolute amount, and that is the rule used here.

/// One line of a diagram's source, with the parts every parser wants already worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Which line of the original source this was, counting from one, for an error message.
    pub number: usize,
    /// The line with its indentation and its trailing spaces taken off.
    pub text: String,
    /// How far it was indented, in columns, with a tab worth four.
    pub indent: usize,
}

impl Line {
    /// True when the line begins with `word` as a whole word, ignoring case.
    ///
    /// A whole word, so that `state` does not match `stateDiagram` and `end` does not match
    /// `endpoint`. Mermaid's own keywords are matched this way and getting it wrong is the sort of
    /// fault that only shows up on somebody else's diagram.
    pub fn starts_with_word(&self, word: &str) -> bool {
        let Some(rest) = take_word(&self.text, word) else {
            return false;
        };
        rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace())
    }

    /// What follows `word`, trimmed, when the line begins with it.
    pub fn after_word(&self, word: &str) -> Option<&str> {
        let rest = take_word(&self.text, word)?;
        if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()) {
            Some(rest.trim())
        } else {
            None
        }
    }
}

/// The text after `word` when `text` begins with it, ignoring case. `None` when it does not.
fn take_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    if text.len() < word.len() {
        return None;
    }
    let (head, rest) = text.split_at(word.len());
    head.eq_ignore_ascii_case(word).then_some(rest)
}

/// A diagram's source, read into lines with the wrapping removed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Source {
    /// The word that named the diagram, as it was written.
    pub keyword: String,
    /// Whatever followed the keyword on its own line: `LR`, `TB:`, `showData title Pets`.
    pub header: String,
    /// The title, from front matter, from `accTitle`, or from a `title` line the parser handed back.
    pub title: Option<String>,
    /// Every line after the one naming the diagram.
    pub lines: Vec<Line>,
}

impl Source {
    /// Read `text`, or `None` when there is no diagram in it at all.
    ///
    /// Everything before the first statement is dealt with here: the front matter, the directives
    /// and the comments. What comes back begins at the first real line.
    pub fn read(text: &str) -> Option<Source> {
        let mut source = Source::default();
        let mut named = false;
        let mut in_front_matter = false;
        let mut front_matter_seen = false;
        // Whether the last line left a quoted label open, so this one is the rest of it.
        let mut continuing = false;

        for (index, raw) in text.split('\n').enumerate() {
            let number = index + 1;
            let line = raw.trim_end_matches(['\r', ' ', '\t']);
            let body = line.trim_start();

            // Front matter, but only when it is the very first thing: a `---` in the middle of a
            // flowchart is a link, not the start of a block of YAML.
            if body == "---" && !named && !front_matter_seen {
                if in_front_matter {
                    in_front_matter = false;
                    front_matter_seen = true;
                } else {
                    in_front_matter = true;
                }
                continue;
            }
            if in_front_matter {
                if let Some(value) = body.strip_prefix("title:") {
                    source.title = Some(unquote(value.trim()));
                }
                continue;
            }
            if body.is_empty() || is_comment(body) {
                // A blank line inside the body is kept, because `sankey` reads them and because a
                // parser counting lines wants to know they were there. Before the diagram is named
                // there is nothing to keep it in.
                if named && body.is_empty() {
                    source.lines.push(Line { number, text: String::new(), indent: 0 });
                }
                continue;
            }
            if !named {
                let (keyword, header) = split_keyword(body);
                source.keyword = keyword;
                source.header = header;
                named = true;
                continue;
            }
            if let Some(value) = body.strip_prefix("accTitle:") {
                source.title.get_or_insert_with(|| unquote(value.trim()));
                continue;
            }
            if body.starts_with("accDescr") {
                continue;
            }
            // **A quoted label left open carries on over the next lines** (`task-2063`), which Mermaid
            // allows and which a long label is written as: `A["one sentence,` then the rest of it on the
            // lines under it. Read as separate statements, the first line was a bracket with no end and
            // the diagram was refused. The lines are joined with a space, as Mermaid shows them.
            if let Some(open) = source.lines.last_mut().filter(|_| continuing) {
                open.text.push(' ');
                open.text.push_str(body);
                continuing = opens_a_quoted_label(&open.text);
                continue;
            }
            source.lines.push(Line { number, text: body.to_owned(), indent: columns(line) });
            continuing = opens_a_quoted_label(body);
        }
        named.then_some(source)
    }

    /// The lines with the blank ones taken out, which is what most parsers want.
    pub fn statements(&self) -> Vec<&Line> {
        self.lines.iter().filter(|line| !line.text.is_empty()).collect()
    }
}

/// Whether a line leaves a quoted label open: an odd number of `"`, the last of them straight after
/// the bracket that opens a label. Asked only about a label, so a message in a sequence diagram that
/// mentions a five inch `5"` screen does not swallow the lines after it.
fn opens_a_quoted_label(text: &str) -> bool {
    if text.matches('"').count() % 2 == 0 {
        return false;
    }
    let last = text.rfind('"').unwrap_or(0);
    text[..last].trim_end().ends_with(['[', '(', '{', '|'])
}

/// True for a line that is a comment or a directive.
fn is_comment(body: &str) -> bool {
    body.starts_with("%%")
}

/// Split the line naming the diagram into the word and the rest.
///
/// The word runs to the first space, and a trailing colon is dropped because `gitGraph TB:` and
/// `graph LR;` both put punctuation where a parser would rather have none.
fn split_keyword(body: &str) -> (String, String) {
    let end = body.find(char::is_whitespace).unwrap_or(body.len());
    let (word, rest) = body.split_at(end);
    let word = word.trim_end_matches([':', ';']);
    (word.to_owned(), rest.trim().trim_end_matches(';').to_owned())
}

/// How far a line is indented, in columns, with a tab worth four.
fn columns(line: &str) -> usize {
    let mut count = 0;
    for character in line.chars() {
        match character {
            ' ' => count += 1,
            '\t' => count += 4 - count % 4,
            _ => break,
        }
    }
    count
}

/// Take the quotes off a label, if it has any.
pub fn unquote(text: &str) -> String {
    let text = text.trim();
    for quote in ['"', '\''] {
        if text.len() >= 2 && text.starts_with(quote) && text.ends_with(quote) {
            return text[1..text.len() - 1].to_owned();
        }
    }
    text.to_owned()
}

/// Turn what was written into what is shown.
///
/// Four things happen, and all four are Mermaid's own rules:
///
/// - the quotes come off, including the backtick pair inside them that marks a markdown string,
///   whose `**bold**` is then shown as it was written because a diagram label is set in one style;
/// - `<br>`, `<br/>` and `<br />` become line breaks;
/// - `#35;` and `#quot;` style entity codes become the characters they name, and so do HTML's own
///   `&lt;`, `&amp;` and `&#91;`, because Mermaid draws a label as HTML and a person writing one
///   writes whichever they know (`task-2063`);
/// - the tags that only change how words look, `<b>`, `<i>`, `<strong>`, `<em>`, `<u>`, `<code>`,
///   `<small>`, `<sub>`, `<sup>`, `<span>` and `<p>`, come off and leave their words, because a
///   diagram label is set in one style and the tag written out is worse than the emphasis lost;
/// - the whitespace at each end goes.
///
/// A tag this does not know, such as the placeholder in `GET /jobs/<id>`, is left as it was written.
/// Mermaid's own sanitiser would remove it, and with it the word the person meant to show.
pub fn label(text: &str) -> String {
    let text = unquote(text);
    let text = match text.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')) {
        Some(inner) => inner.to_owned(),
        None => text,
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while !rest.is_empty() {
        if let Some(after) = strip_break(rest) {
            out.push('\n');
            rest = after;
            continue;
        }
        if let Some((entity, after)) = strip_entity(rest) {
            out.push(entity);
            rest = after;
            continue;
        }
        if let Some(after) = strip_formatting_tag(rest) {
            rest = after;
            continue;
        }
        let mut characters = rest.chars();
        match characters.next() {
            Some(character) => out.push(character),
            None => break,
        }
        rest = characters.as_str();
    }
    out.trim().to_owned()
}

/// What follows a `<br>` in any of its three spellings, when one is next.
///
/// Compared a byte at a time rather than by slicing the next six bytes, which is what it did: a slice
/// that ends inside a character is no slice at all, so `<br/>…` and `<br/>·`, where a character of
/// more than one byte follows the tag, kept the tag as text. `task-2063` found it in a real document.
fn strip_break(rest: &str) -> Option<&str> {
    ["<br />", "<br/>", "<br>"]
        .into_iter()
        .find(|form| starts_with_ignoring_case(rest, form))
        .map(|form| &rest[form.len()..])
}

/// Whether `text` begins with `prefix`, ignoring ASCII case, without slicing inside a character.
fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// What follows a tag that only changes how words look, when one is next.
fn strip_formatting_tag(rest: &str) -> Option<&str> {
    let body = rest.strip_prefix('<')?;
    let body = body.strip_prefix('/').unwrap_or(body);
    let name_end = body.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(body.len());
    let name = body[..name_end].to_ascii_lowercase();
    const LOOKS: [&str; 12] =
        ["b", "i", "u", "em", "strong", "code", "small", "sub", "sup", "span", "p", "mark"];
    if !LOOKS.contains(&name.as_str()) {
        return None;
    }
    let close = body[name_end..].find('>')?;
    let between = &body[name_end..name_end + close];
    // `<b>` and `<span style="...">`, but not `<bold-thing>` or `<b` running on into the text.
    if !between.is_empty() && !between.starts_with([' ', '/']) {
        return None;
    }
    Some(&body[name_end + close + 1..])
}

/// The character an entity code names, and what follows it.
///
/// `#35;` is a number and `#quot;` is a name, and Mermaid accepts both. A `#` that is not the start
/// of one is an ordinary character, which is what the `None` is for.
fn strip_entity(rest: &str) -> Option<(char, &str)> {
    // HTML's spelling, `&lt;` and `&#91;`, is Mermaid's `#lt;` and `#91;` with another mark in front.
    let body = match rest.strip_prefix('&') {
        Some(after) => after.strip_prefix('#').unwrap_or(after),
        None => rest.strip_prefix('#')?,
    };
    let end = body.find(';')?;
    if end == 0 || end > 8 {
        return None;
    }
    let name = &body[..end];
    let after = &body[end + 1..];
    let character = match name {
        "quot" => '"',
        "apos" => '\'',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "nbsp" => ' ',
        "colon" => ':',
        "semi" => ';',
        "hash" | "num" => '#',
        digits => {
            let value = match digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(value)?
        }
    };
    Some((character, after))
}

/// Split on `separator`, but not inside quotes and not inside a node's brackets.
///
/// `task-2063`. A flowchart joins nodes with `&`, as in `A & B --> C`, and splitting on every `&`
/// broke any label with one in it: `[Pages & Components]`, `[ATT&CK]`, and every `&lt;` and `&#91;`
/// a person wrote to show a bracket. Seven diagrams in the task documents were refused for it. A
/// bracket of any of the three kinds opens a label, and nothing inside one is a separator.
pub fn split_outside_brackets(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    for character in text.chars() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' => quote = Some(character),
            None if matches!(character, '[' | '(' | '{') => depth += 1,
            None if matches!(character, ']' | ')' | '}') => depth = depth.saturating_sub(1),
            None if character == separator && depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            None => {}
        }
        current.push(character);
    }
    parts.push(current);
    parts.into_iter().map(|part| part.trim().to_owned()).collect()
}

/// Split on `separator`, but not where it is inside quotes.
///
/// A comma inside a label is a comma, not the end of the list. Used by every parser that reads a
/// comma separated list of things a person may have quoted.
pub fn split_outside_quotes(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for character in text.chars() {
        match quote {
            Some(open) if character == open => {
                quote = None;
                current.push(character);
            }
            Some(_) => current.push(character),
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                current.push(character);
            }
            None if character == separator => {
                parts.push(std::mem::take(&mut current));
            }
            None => current.push(character),
        }
    }
    parts.push(current);
    parts.into_iter().map(|part| part.trim().to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2063`: a quoted label carries on over the lines under it until its quote closes.
    #[test]
    fn a_quoted_label_carries_on_over_the_next_lines() {
        let source = Source::read(
            "flowchart TB\n  RU[\"one, two,\n      three, four\"]\n  RU --> X\n  Y[\"5\\\" screen\"]\n",
        )
        .expect("a diagram");
        let texts: Vec<&str> = source.statements().iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts[0], "RU[\"one, two, three, four\"]");
        assert_eq!(texts[1], "RU --> X");
        assert_eq!(texts.len(), 3);
        assert!(!opens_a_quoted_label("A->>B: a 5\" screen"), "not a label");
    }

    /// `task-2063`: HTML's entities, a formatting tag, and a `<br/>` with a wide character after it.
    #[test]
    fn a_label_reads_the_markup_a_person_writes() {
        assert_eq!(label("Vec&lt;Theme&gt;"), "Vec<Theme>");
        assert_eq!(label("Appearance &amp; Icons"), "Appearance & Icons");
        assert_eq!(label("&quot;look down&quot;"), "\"look down\"");
        assert_eq!(label("model&#91;1m&#93;"), "model[1m]");
        assert_eq!(label("<b>Full quality</b>"), "Full quality");
        assert_eq!(label("<span style=\"color:red\">red</span> text"), "red text");
        assert_eq!(label("editor()<br/>\u{2026}40 accessors"), "editor()\n\u{2026}40 accessors");
        assert_eq!(label("one<br/>\u{b7} two"), "one\n\u{b7} two");
        assert_eq!(label("GET /jobs/<id>"), "GET /jobs/<id>", "a placeholder is kept as written");
        assert_eq!(label("ATT&CK"), "ATT&CK", "an ampersand that starts no entity is a character");
    }

    #[test]
    fn an_ampersand_inside_a_label_does_not_split_the_statement() {
        assert_eq!(split_outside_brackets("A & B", '&'), vec!["A", "B"]);
        assert_eq!(
            split_outside_brackets("P[Pages & Components]", '&'),
            vec!["P[Pages & Components]"]
        );
        assert_eq!(
            split_outside_brackets("D[driving/&lt;slug&gt;.mp4] & E(x)", '&'),
            vec!["D[driving/&lt;slug&gt;.mp4]", "E(x)"]
        );
    }

    #[test]
    fn the_first_real_line_names_the_diagram() {
        let source = Source::read("flowchart LR\n  A --> B\n").expect("a diagram");
        assert_eq!(source.keyword, "flowchart");
        assert_eq!(source.header, "LR");
        assert_eq!(source.statements().len(), 1);
        assert_eq!(source.statements()[0].text, "A --> B");
        assert_eq!(source.statements()[0].number, 2, "the line number is the one in the file");
    }

    #[test]
    fn front_matter_is_skipped_and_its_title_is_kept() {
        let text = "---\ntitle: A Plan\nconfig:\n  theme: forest\n---\nflowchart TD\n  A --> B\n";
        let source = Source::read(text).expect("a diagram");
        assert_eq!(source.title.as_deref(), Some("A Plan"));
        assert_eq!(source.keyword, "flowchart");
        assert_eq!(source.statements().len(), 1, "none of the YAML is a statement");
    }

    #[test]
    fn a_rule_in_the_middle_of_a_diagram_is_not_front_matter() {
        // `---` is a link in a flowchart. Reading it as the start of a block of YAML would swallow
        // the rest of the file, which is the sort of fault that looks like the renderer is broken.
        let source = Source::read("flowchart LR\n  A --- B\n---\n  C --- D\n").expect("a diagram");
        assert_eq!(source.statements().len(), 3);
    }

    #[test]
    fn comments_and_directives_are_skipped() {
        let text =
            "%%{init: {'theme':'dark'}}%%\nflowchart TD\n%% a note to the reader\n  A --> B\n";
        let source = Source::read(text).expect("a diagram");
        assert_eq!(source.keyword, "flowchart");
        assert_eq!(source.statements().len(), 1);
    }

    #[test]
    fn an_accessible_title_is_used_when_there_is_no_other_one() {
        let source = Source::read("pie\naccTitle: Pets\n\"Dogs\" : 3\n").expect("a diagram");
        assert_eq!(source.title.as_deref(), Some("Pets"));
        assert_eq!(source.statements().len(), 1, "accTitle is not a slice");
    }

    #[test]
    fn indentation_is_counted_in_columns_with_a_tab_worth_four() {
        let source =
            Source::read("mindmap\nroot\n  one\n\ttwo\n        three\n").expect("a diagram");
        let indents: Vec<usize> = source.statements().iter().map(|line| line.indent).collect();
        assert_eq!(indents, vec![0, 2, 4, 8]);
    }

    #[test]
    fn a_trailing_colon_or_semicolon_is_not_part_of_the_keyword() {
        assert_eq!(Source::read("gitGraph TB:\ncommit\n").expect("read").keyword, "gitGraph");
        assert_eq!(Source::read("graph LR;\nA-->B\n").expect("read").header, "LR");
    }

    #[test]
    fn nothing_at_all_is_not_a_diagram() {
        assert_eq!(Source::read(""), None);
        assert_eq!(Source::read("%% only a comment\n\n"), None);
    }

    #[test]
    fn a_word_is_matched_whole_and_not_as_a_prefix() {
        let line = Line { number: 1, text: "stateDiagram-v2".to_owned(), indent: 0 };
        assert!(!line.starts_with_word("state"), "state must not match stateDiagram");
        assert!(line.starts_with_word("stateDiagram-v2"));
        let line = Line { number: 1, text: "state Active".to_owned(), indent: 0 };
        assert!(line.starts_with_word("state"));
        assert_eq!(line.after_word("state"), Some("Active"));
    }

    #[test]
    fn a_label_loses_its_quotes_and_gains_its_line_breaks() {
        assert_eq!(label("\"Hello there\""), "Hello there");
        assert_eq!(label("One<br>Two"), "One\nTwo");
        assert_eq!(label("One<BR/>Two"), "One\nTwo");
        assert_eq!(label("One<br />Two"), "One\nTwo");
    }

    #[test]
    fn an_entity_code_becomes_the_character_it_names() {
        assert_eq!(label("a #35; b"), "a # b");
        assert_eq!(label("#quot;quoted#quot;"), "\"quoted\"");
        assert_eq!(label("#x2764; it"), "\u{2764} it");
        // A hash that is not an entity is an ordinary character, because plenty of labels have one.
        assert_eq!(label("issue #42 is open"), "issue #42 is open");
    }

    #[test]
    fn a_markdown_string_keeps_its_words_and_loses_its_backticks() {
        // Unluminous sets a diagram label in one style, so the marks inside are shown as written rather
        // than dropped: dropping them would silently change what the label says.
        assert_eq!(label("\"`**bold** text`\""), "**bold** text");
    }

    #[test]
    fn a_separator_inside_quotes_does_not_split() {
        assert_eq!(split_outside_quotes("a, b, c", ','), vec!["a", "b", "c"]);
        assert_eq!(split_outside_quotes("\"one, two\", three", ','), vec!["\"one, two\"", "three"]);
    }
}
