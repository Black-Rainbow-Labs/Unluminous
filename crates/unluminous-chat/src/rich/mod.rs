//! Answers that are interfaces: the components an agent writes into its answer, read while they arrive.
//!
//! `tasks/task-2211-agent-chat-intelligent-ui-tdd.md` is the design. An agent writes a component as a
//! fenced block whose language is `ui`, holding one JSON object:
//!
//! ````markdown
//! The build got slower in three places.
//!
//! ```ui
//! {"type": "chart", "kind": "bar", "labels": ["core", "app"], "series": [{"name": "s", "values": [41, 212]}]}
//! ```
//! ````
//!
//! This module is the half of that which has no window behind it: where the blocks are in the text
//! ([`segments`]), what a block that has only half arrived says ([`repair`]), what a block means
//! ([`component`]), what a calculator's expressions evaluate to ([`expr`]), how a number is written
//! ([`format`]), and the one list of components every reference, example and check is made from
//! ([`catalogue`]). Everything here is a unit test with no window, because it is the part an agent's
//! bytes reach first and the part most likely to be wrong.
//!
//! ## Why a fence holding JSON
//!
//! A model writes JSON correctly, because tool arguments are written in it. A fence keeps the answer
//! readable anywhere else: in a terminal, a transcript or a ticket it is a code block. And the fence
//! says where a component starts before its JSON is complete, which is what lets the pane draw a
//! component while it is still arriving.

pub mod catalogue;
pub mod component;
pub mod expr;
pub mod format;
pub mod repair;

pub use component::{Component, Kind, Problem};

/// The language a fence has to name for its body to be read as a component.
pub const LANGUAGE: &str = "ui";

/// One run of an answer: words to draw as markdown, or a component.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Markdown, with the blank lines at either end taken off.
    Markdown(String),
    /// The body of a ```` ```ui ```` fence, and whether its closing fence has arrived.
    Block { source: String, finished: bool },
}

/// Whether `text` holds a component at all, which is the question asked before anything is split.
///
/// A plain answer is the common case and this keeps it free: one search over the text and no copy.
pub fn has_blocks(text: &str) -> bool {
    text.contains("```ui") || text.contains("````ui")
}

/// Split an answer into its markdown and its components, in the order they were written.
///
/// A ```` ```ui ```` line opens a block and a line of at least as many backticks and nothing else
/// closes it, which is the rule the markdown reader keeps for any fence. A block still open when the
/// text ends is `finished: false`, which is what an answer still arriving looks like. **A fence of
/// another language is followed too**, so a ```` ```ui ```` written inside a longer fence of markdown,
/// which is how a document shows an example of one, stays text.
pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut words: Vec<&str> = Vec::new();
    let mut block: Option<(usize, Vec<&str>)> = None;
    let mut other: Option<usize> = None;
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some((ticks, body)) = block.as_mut() {
            if closes(line, *ticks) {
                let source = body.join("\n");
                out.push(Segment::Block { source, finished: true });
                block = None;
            } else {
                body.push(line);
            }
            continue;
        }
        if let Some(ticks) = other {
            if closes(line, ticks) {
                other = None;
            }
            words.push(line);
            continue;
        }
        match opens(line) {
            Some((ticks, true)) => {
                push_words(&mut out, &mut words);
                block = Some((ticks, Vec::new()));
            }
            Some((ticks, false)) => {
                other = Some(ticks);
                words.push(line);
            }
            None => words.push(line),
        }
    }
    push_words(&mut out, &mut words);
    if let Some((_, body)) = block {
        out.push(Segment::Block { source: body.join("\n"), finished: false });
    }
    out
}

/// Put the words gathered so far into `out` as one markdown segment, if they say anything.
fn push_words(out: &mut Vec<Segment>, words: &mut Vec<&str>) {
    let joined = words.join("\n");
    words.clear();
    let trimmed = joined.trim_matches(|c: char| c == '\n' || c == '\r');
    if !trimmed.trim().is_empty() {
        out.push(Segment::Markdown(trimmed.trim_end().to_owned()));
    }
}

/// Whether `line` opens a fence: how many backticks it has, and whether its language is [`LANGUAGE`].
///
/// Up to three spaces of indent, three or more backticks, then the language word. The CommonMark rule,
/// so a block the editor's own preview reads as code is a block here.
fn opens(line: &str) -> Option<(usize, bool)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let ticks = rest.len() - rest.trim_start_matches('`').len();
    if ticks < 3 {
        return None;
    }
    let info = rest[ticks..].trim();
    if info.contains('`') {
        return None;
    }
    let language = info.split_whitespace().next().unwrap_or("");
    Some((ticks, language == LANGUAGE))
}

/// Whether `line` closes a fence opened with `ticks` backticks.
fn closes(line: &str, ticks: usize) -> bool {
    let trimmed = line.trim();
    let indent = line.len() - line.trim_start_matches(' ').len();
    indent <= 3 && trimmed.len() >= ticks && trimmed.chars().all(|c| c == '`')
}

/// Read one block into a component, for drawing.
///
/// A block still arriving is repaired first and read leniently, so it draws whatever has come so far.
/// A finished block is read as it is, strictly: what it says is what it is.
pub fn read_block(source: &str, finished: bool) -> Result<Component, Vec<Problem>> {
    if !finished {
        let repaired = repair::repair(source);
        if repaired.trim().is_empty() {
            return Ok(Component::pending());
        }
        let value: serde_json::Value = match serde_json::from_str(&repaired) {
            Ok(value) => value,
            Err(_) => return Ok(Component::pending()),
        };
        return Ok(Component::read(&value, false).0);
    }
    let value: serde_json::Value = serde_json::from_str(source.trim()).map_err(|error| {
        vec![Problem::error(
            "",
            format!(
                "the block is not JSON: {error} (line {}, column {})",
                error.line(),
                error.column()
            ),
        )]
    })?;
    let (component, problems) = Component::read(&value, true);
    match problems.iter().any(|problem| problem.is_error()) {
        true => Err(problems),
        false => Ok(component),
    }
}

/// Check one block the way `plugins run agent-chat validate` does: every problem, notes included.
pub fn validate(source: &str) -> Vec<Problem> {
    match serde_json::from_str::<serde_json::Value>(source.trim()) {
        Ok(value) => Component::read(&value, true).1,
        Err(error) => vec![Problem::error(
            "",
            format!("not JSON: {error} (line {}, column {})", error.line(), error.column()),
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_split_into_its_words_and_its_components_in_order() {
        let text = "Before.\n\n```ui\n{\"type\": \"callout\", \"text\": \"x\"}\n```\n\nAfter.";
        assert_eq!(
            segments(text),
            vec![
                Segment::Markdown("Before.".into()),
                Segment::Block {
                    source: "{\"type\": \"callout\", \"text\": \"x\"}".into(),
                    finished: true
                },
                Segment::Markdown("After.".into()),
            ]
        );
    }

    #[test]
    fn a_block_still_arriving_is_unfinished_and_nothing_after_it_is_lost() {
        let text = "Here:\n```ui\n{\"type\": \"cha";
        assert_eq!(
            segments(text),
            vec![
                Segment::Markdown("Here:".into()),
                Segment::Block { source: "{\"type\": \"cha".into(), finished: false },
            ]
        );
    }

    #[test]
    fn a_ui_fence_inside_another_fence_is_an_example_and_stays_text() {
        let text = "````markdown\n```ui\n{}\n```\n````";
        assert_eq!(segments(text), vec![Segment::Markdown(text.into())]);
        // An ordinary fence of another language is text too.
        let code = "```json\n{\"type\": \"chart\"}\n```";
        assert_eq!(segments(code), vec![Segment::Markdown(code.into())]);
    }

    #[test]
    fn a_longer_fence_closes_only_on_as_many_backticks() {
        let text = "````ui\n{\"type\": \"callout\", \"text\": \"```\"}\n````";
        assert_eq!(
            segments(text),
            vec![Segment::Block {
                source: "{\"type\": \"callout\", \"text\": \"```\"}".into(),
                finished: true
            }]
        );
    }

    #[test]
    fn carriage_returns_do_not_hide_a_fence() {
        let text = "a\r\n```ui\r\n{}\r\n```\r\nb";
        let got = segments(text);
        assert_eq!(got.len(), 3, "{got:?}");
    }

    #[test]
    fn a_plain_answer_has_no_blocks_and_costs_one_search() {
        assert!(!has_blocks("Just words, and ```rust\nfn main() {}\n```"));
        assert!(has_blocks("```ui\n{}"));
    }

    #[test]
    fn a_finished_block_that_is_not_json_says_where() {
        let problems = read_block("{\"type\": \"callout\",, }", true).unwrap_err();
        assert!(problems[0].message.contains("line 1"), "{problems:?}");
    }
}
