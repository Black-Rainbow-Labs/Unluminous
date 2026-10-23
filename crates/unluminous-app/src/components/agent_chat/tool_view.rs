//! What an open tool call shows: its arguments and its answer, as Markdown the chat already renders.
//!
//! `task-2336`: *"The tool calls show json, but the json formatting is ugly and hard to read. We want a
//! premier experience and design that shows well formatted and easy to read tool calls info."* The
//! arguments used to be one pretty printed JSON fence, so a shell command was a quoted string full of
//! `\n`, an edit was two long strings to compare by eye, and every value sat between keys, quotes and
//! braces. Here each kind of value is shown the way a person reads it:
//!
//! - a `command` is a `sh` fence holding the command as it would be typed, under its `description`;
//! - an edit's `old_string` and `new_string` are one `diff` fence, and `MultiEdit`'s list is one each;
//! - a file's `content` is fenced in that file's own language;
//! - every other short value is a line of a list, its name in words and in bold, the value as code;
//! - a long string or a structure has a labelled fence of its own;
//! - the answer is labelled `Result`, in the language of the file a read returned, and cut at
//!   [`ANSWER_LINES`].
//!
//! Arguments that are not a JSON object are shown exactly as they arrived, because they are not
//! anything this can lay out, and they may be a call still arriving.

use serde_json::{Map, Value};
use unluminous_chat::model::ToolCall;

/// The most lines of an answer shown. A tool can answer with a whole file or a whole build log, and the
/// block is rendered again whenever its width changes.
pub const ANSWER_LINES: usize = 400;

/// The longest string shown in the list rather than in a fence of its own.
const SHORT: usize = 90;

/// The keys that are shown on their own above the list, and so are not lines of it.
const SHOWN_APART: [&str; 7] =
    ["command", "cmd", "description", "old_string", "new_string", "content", "edits"];

/// A tool call's arguments as Markdown.
///
/// @param tool - the call, whose arguments are the JSON text the model produced
pub fn arguments_markdown(tool: &ToolCall) -> String {
    let trimmed = tool.arguments.trim();
    let Ok(Value::Object(arguments)) = serde_json::from_str::<Value>(trimmed) else {
        return raw(trimmed);
    };
    if arguments.is_empty() {
        return "*No arguments*".to_owned();
    }
    let language = file_of(&arguments).and_then(extension_of);
    let mut parts = Vec::new();
    if let Some(description) = text_of(&arguments, "description") {
        parts.push(format!("*{}*", inline_words(description)));
    }
    if let Some(command) = text_of(&arguments, "command").or_else(|| text_of(&arguments, "cmd")) {
        parts.push(fence("sh", command));
    }
    if let (Some(old), Some(new)) =
        (text_of(&arguments, "old_string"), text_of(&arguments, "new_string"))
    {
        parts.push(fence("diff", &diff(old, new)));
    }
    if let Some(Value::Array(edits)) = arguments.get("edits") {
        parts.extend(edits_markdown(edits));
    }
    if let Some(content) = text_of(&arguments, "content") {
        parts.push(labelled("Content", &fence(language.as_deref().unwrap_or(""), content)));
    }
    let (rows, apart) = rows_and_blocks(&arguments);
    if !rows.is_empty() {
        parts.push(listed(&rows));
    }
    parts.extend(apart);
    parts.join("\n\n")
}

/// A tool call's answer as Markdown, under a `Result` label.
///
/// @param tool - the call, whose arguments say which file it read when it read one
/// @param answer - what the tool answered
pub fn answer_markdown(tool: &ToolCall, answer: &str) -> String {
    let label = if tool.failed { "Error" } else { "Result" };
    let trimmed = answer.trim_end();
    if trimmed.trim().is_empty() {
        return format!("**{label}**\n\n*Nothing was returned*");
    }
    let (shown, left_out) = first_lines(trimmed, ANSWER_LINES);
    let body = match laid_out_json(shown.trim()) {
        Some(pretty) => fence("json", &pretty),
        None => {
            let arguments = match serde_json::from_str::<Value>(tool.arguments.trim()) {
                Ok(Value::Object(arguments)) => arguments,
                _ => Map::new(),
            };
            // Only a read answers with the file itself. Every other call that names a file answers with a
            // sentence about it, which coloured as the file's language is coloured wrongly.
            let language = match is_a_read(tool) {
                true => file_of(&arguments).and_then(extension_of),
                false => None,
            };
            fence(language.as_deref().unwrap_or(""), shown)
        }
    };
    let mut markdown = format!("**{label}**\n\n{body}");
    if left_out > 0 {
        let plural = if left_out == 1 { "" } else { "s" };
        markdown.push_str(&format!("\n\n*{left_out} more line{plural} not shown*"));
    }
    markdown
}

/// Arguments that are not a JSON object, shown as they came: a JSON value of another kind laid out, and
/// anything else in a fence naming no language.
fn raw(text: &str) -> String {
    match laid_out_json(text) {
        Some(pretty) => fence("json", &pretty),
        None => fence("", text),
    }
}

/// `text` laid out over several lines when it is a JSON object or list, and [`None`] when it is not.
///
/// The limit is not tidiness: this runs whenever the block is rendered again, and a tool can answer
/// with a great deal.
pub fn laid_out_json(text: &str) -> Option<String> {
    const LIMIT: usize = 64_000;
    if text.len() > LIMIT || !text.starts_with(['{', '[']) {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// Whether a call reads a file and answers with its text: Claude Code's `Read`, and the `read` and `view` tools
/// other agents have.
fn is_a_read(tool: &ToolCall) -> bool {
    let name = tool.name.trim_start_matches("unluminous_").to_ascii_lowercase();
    matches!(name.as_str(), "read" | "read_file" | "view" | "readfile" | "cat")
}

/// The string under `key`, when there is one and it is not empty.
fn text_of<'a>(arguments: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    arguments.get(key)?.as_str().filter(|text| !text.trim().is_empty())
}

/// The file a call is about, read from the names agents give it.
fn file_of(arguments: &Map<String, Value>) -> Option<&str> {
    ["file_path", "path", "file", "filePath", "notebook_path"]
        .into_iter()
        .find_map(|key| text_of(arguments, key))
}

/// A file's extension, which is the word a fence names its language with.
fn extension_of(path: &str) -> Option<String> {
    let extension = std::path::Path::new(path.trim()).extension()?.to_str()?.to_lowercase();
    (!extension.is_empty()).then_some(extension)
}

/// The lines of the list, and the values too long or too deep for it, each as a labelled block.
fn rows_and_blocks(arguments: &Map<String, Value>) -> (Vec<(String, String)>, Vec<String>) {
    let mut rows = Vec::new();
    let mut blocks = Vec::new();
    for (key, value) in arguments {
        if SHOWN_APART.contains(&key.as_str()) && shown_apart(arguments, key) {
            continue;
        }
        let name = words_of(key);
        match value {
            Value::String(text) if text.len() <= SHORT && !text.contains('\n') => {
                rows.push((name, code_span(text)));
            }
            Value::String(text) => blocks.push(labelled(&name, &fence("", text))),
            Value::Number(number) => rows.push((name, code_span(&number.to_string()))),
            Value::Bool(flag) => rows.push((name, code_span(&flag.to_string()))),
            Value::Null => rows.push((name, "*none*".to_owned())),
            Value::Array(items) if items.is_empty() => rows.push((name, "*empty*".to_owned())),
            Value::Object(fields) if fields.is_empty() => rows.push((name, "*empty*".to_owned())),
            Value::Array(items) if items.iter().all(short_scalar) => {
                let listed: Vec<String> =
                    items.iter().map(scalar_text).map(|one| code_span(&one)).collect();
                rows.push((name, listed.join(" ")));
            }
            structure => {
                let pretty = serde_json::to_string_pretty(structure).unwrap_or_default();
                blocks.push(labelled(&name, &fence("json", &pretty)));
            }
        }
    }
    (rows, blocks)
}

/// Whether a key that is normally shown above the list really was, which it was not when its value
/// was not the kind expected: a `command` that is a list, or an `old_string` with no `new_string`.
fn shown_apart(arguments: &Map<String, Value>, key: &str) -> bool {
    match key {
        "command" | "cmd" | "description" | "content" => text_of(arguments, key).is_some(),
        "old_string" | "new_string" => {
            text_of(arguments, "old_string").is_some() && text_of(arguments, "new_string").is_some()
        }
        "edits" => matches!(arguments.get("edits"), Some(Value::Array(_))),
        _ => false,
    }
}

/// Whether a value is short enough to sit on a line of the list beside others.
fn short_scalar(value: &Value) -> bool {
    match value {
        Value::String(text) => text.len() <= 40 && !text.contains('\n'),
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    }
}

/// A scalar's text, a string without its quotes.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `MultiEdit`'s list, one diff an edit, numbered when there is more than one.
fn edits_markdown(edits: &[Value]) -> Vec<String> {
    let several = edits.len() > 1;
    edits
        .iter()
        .enumerate()
        .filter_map(|(at, edit)| {
            let old = edit.get("old_string")?.as_str()?;
            let new = edit.get("new_string")?.as_str()?;
            let block = fence("diff", &diff(old, new));
            Some(match several {
                true => labelled(&format!("Edit {}", at + 1), &block),
                false => block,
            })
        })
        .collect()
}

/// The old text as removed lines and the new text as added ones, the lines both share at the start and
/// the end kept as context so a one word change reads as one changed line.
pub fn diff(old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let front = old.iter().zip(new.iter()).take_while(|(a, b)| a == b).count();
    let back = old[front..]
        .iter()
        .rev()
        .zip(new[front..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut lines = Vec::new();
    lines.extend(old[..front].iter().map(|line| format!("  {line}")));
    lines.extend(old[front..old.len() - back].iter().map(|line| format!("- {line}")));
    lines.extend(new[front..new.len() - back].iter().map(|line| format!("+ {line}")));
    lines.extend(old[old.len() - back..].iter().map(|line| format!("  {line}")));
    lines.join("\n")
}

/// A key in words: `file_path` and `filePath` are `File path`.
pub fn words_of(key: &str) -> String {
    let mut words = String::new();
    let mut previous_lower = false;
    for character in key.chars() {
        if character == '_' || character == '-' {
            words.push(' ');
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = character.is_lowercase() || character.is_ascii_digit();
        words.extend(character.to_lowercase());
    }
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = words.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => key.to_owned(),
    }
}

/// The short values, one a line: the name in bold and the value after it. A Markdown table was tried first and
/// drew a ruled grid with an empty header row around two words, which is heavier than what it holds.
fn listed(rows: &[(String, String)]) -> String {
    rows.iter()
        .map(|(name, value)| format!("- **{}** {value}", inline_words(name)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A value as inline code, with enough backticks round it that a backtick inside cannot end it.
fn code_span(text: &str) -> String {
    if text.trim().is_empty() {
        return format!("`{text}`");
    }
    let longest = longest_run(text, '`');
    let ticks = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') { " " } else { "" };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

/// Words that sit inside emphasis, with the characters that would end it escaped.
fn inline_words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace('*', "\\*")
}

/// A fenced block in `language`, with a fence longer than any run of backticks inside it.
pub fn fence(language: &str, body: &str) -> String {
    let ticks = "`".repeat(longest_run(body, '`').max(2) + 1);
    format!("{ticks}{language}\n{}\n{ticks}", body.trim_end_matches('\n'))
}

/// A block with its name above it.
fn labelled(name: &str, block: &str) -> String {
    format!("**{}**\n\n{block}", inline_words(name))
}

/// The longest run of `wanted` in `text`.
fn longest_run(text: &str, wanted: char) -> usize {
    let (mut longest, mut run) = (0, 0);
    for character in text.chars() {
        run = if character == wanted { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

/// The first `most` lines of `text`, and how many were left out.
fn first_lines(text: &str, most: usize) -> (&str, usize) {
    let total = text.lines().count();
    if total <= most {
        return (text, 0);
    }
    let end = text.match_indices('\n').nth(most - 1).map(|(at, _)| at).unwrap_or(text.len());
    (&text[..end], total - most)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall::new("call-1", name, arguments)
    }

    /// A shell command is shown the way it is typed, under what it is for, and the list holds the rest.
    #[test]
    fn a_command_is_a_shell_block_under_its_description() {
        let markdown = arguments_markdown(&call(
            "Bash",
            r#"{"command":"cargo build --release\ncargo test","description":"Build it","timeout":120000}"#,
        ));
        assert_eq!(
            markdown,
            "*Build it*\n\n```sh\ncargo build --release\ncargo test\n```\n\n- **Timeout** `120000`"
        );
    }

    /// An edit is one diff, with the lines both sides share kept as context.
    #[test]
    fn an_edit_is_a_diff_of_its_two_strings() {
        let markdown = arguments_markdown(&call(
            "Edit",
            r#"{"file_path":"src/main.rs","old_string":"fn a() {\n    one();\n}","new_string":"fn a() {\n    two();\n}"}"#,
        ));
        assert!(
            markdown.starts_with("```diff\n  fn a() {\n-     one();\n+     two();\n  }\n```"),
            "{markdown}"
        );
        assert!(markdown.contains("- **File path** `src/main.rs`"), "{markdown}");
        assert!(!markdown.contains("Old string"), "the strings are not shown twice: {markdown}");
    }

    /// `MultiEdit` is one numbered diff an edit.
    #[test]
    fn several_edits_are_numbered_diffs() {
        let markdown = arguments_markdown(&call(
            "MultiEdit",
            r#"{"file_path":"a.rs","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]}"#,
        ));
        assert!(markdown.contains("**Edit 1**\n\n```diff\n- a\n+ b\n```"), "{markdown}");
        assert!(markdown.contains("**Edit 2**\n\n```diff\n- c\n+ d\n```"), "{markdown}");
    }

    /// What is written to a file is fenced in that file's language, so its plugin colours it.
    #[test]
    fn written_content_is_fenced_in_the_files_language() {
        let markdown = arguments_markdown(&call(
            "Write",
            r#"{"file_path":"config/app.json","content":"{\"a\": 1}"}"#,
        ));
        assert!(markdown.contains("**Content**\n\n```json\n{\"a\": 1}\n```"), "{markdown}");
    }

    /// A structure an MCP tool was given is a labelled JSON block, and short lists stay in the list.
    #[test]
    fn a_nested_value_is_a_labelled_json_block() {
        let markdown = arguments_markdown(&call(
            "mcp__unluminous__editor",
            r#"{"command":["a","b"],"arguments":{"path":"x.md","line":3},"tags":["one","two"],"force":true}"#,
        ));
        assert!(markdown.contains("- **Command** `a` `b`"), "{markdown}");
        assert!(markdown.contains("- **Tags** `one` `two`"), "{markdown}");
        assert!(markdown.contains("- **Force** `true`"), "{markdown}");
        assert!(
            markdown.contains(
                "**Arguments**\n\n```json\n{\n  \"line\": 3,\n  \"path\": \"x.md\"\n}\n```"
            ) || markdown.contains(
                "**Arguments**\n\n```json\n{\n  \"path\": \"x.md\",\n  \"line\": 3\n}\n```"
            ),
            "{markdown}"
        );
    }

    /// Arguments still arriving, or not JSON at all, are shown exactly as they came.
    #[test]
    fn arguments_that_are_not_an_object_are_left_as_they_came() {
        assert_eq!(arguments_markdown(&call("x", "{\"path\": ")), "```\n{\"path\":\n```");
        assert_eq!(arguments_markdown(&call("x", "[1,2]")), "```json\n[\n  1,\n  2\n]\n```");
        assert_eq!(arguments_markdown(&call("x", "{}")), "*No arguments*");
    }

    /// A value holding backticks cannot end the code span round it.
    #[test]
    fn a_backtick_stays_inside_its_code_span() {
        let markdown = arguments_markdown(&call("Grep", r#"{"pattern":"a|b`c"}"#));
        assert_eq!(markdown, "- **Pattern** ``a|b`c``");
    }

    /// An edit answers with a sentence about the file, not the file, so it is not coloured as the file's language.
    #[test]
    fn only_a_read_answers_in_the_files_language() {
        let edit = call("Edit", r#"{"file_path":"src/lib.rs","old_string":"a","new_string":"b"}"#);
        assert_eq!(answer_markdown(&edit, "Updated."), "**Result**\n\n```\nUpdated.\n```");
    }

    /// A read's answer is fenced in the file's language, and a long answer is cut and says by how much.
    #[test]
    fn an_answer_is_labelled_in_its_files_language_and_cut() {
        let read = call("Read", r#"{"file_path":"src/lib.rs"}"#);
        assert_eq!(
            answer_markdown(&read, "fn main() {}\n"),
            "**Result**\n\n```rs\nfn main() {}\n```"
        );
        let long: String = (1..=ANSWER_LINES + 3).map(|n| format!("line {n}\n")).collect();
        let markdown = answer_markdown(&call("Bash", r#"{"command":"seq"}"#), &long);
        assert!(markdown.ends_with("*3 more lines not shown*"), "{markdown}");
        assert!(markdown.contains(&format!("line {ANSWER_LINES}\n```")), "{markdown}");
        assert!(!markdown.contains(&format!("line {}", ANSWER_LINES + 1)));
        let json = answer_markdown(&call("x", "{}"), "{\"ok\":true}");
        assert_eq!(json, "**Result**\n\n```json\n{\n  \"ok\": true\n}\n```");
        let mut failed = call("x", "{}");
        failed.failed = true;
        assert!(answer_markdown(&failed, "no such file").starts_with("**Error**"));
    }

    /// A key reads as words whichever way it was written.
    #[test]
    fn a_key_is_named_in_words() {
        assert_eq!(words_of("file_path"), "File path");
        assert_eq!(words_of("filePath"), "File path");
        assert_eq!(words_of("head_limit"), "Head limit");
        assert_eq!(words_of("-n"), "N");
        assert_eq!(words_of("url"), "Url");
    }
}
