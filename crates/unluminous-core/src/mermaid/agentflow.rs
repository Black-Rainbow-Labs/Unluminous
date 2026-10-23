//! Agentflow: `agentflow-beta`, Mermaid's diagram of agents, the flows they run, and their tools.
//!
//! Underneath it is a flowchart with its own words for four things, so this module rewrites the source
//! into flowchart text and the flowchart draws it, which keeps one layout and one set of shapes:
//!
//! - `flow id["Label"] … end` is a subgraph, and `@{ view: collapsed }` on one folds it down to a
//!   single box that its edges end at;
//! - `global … end` keeps the nodes inside it at the top level wherever else they are mentioned,
//!   which is done by declaring them before anything else;
//! - `connector id["Label"]` is an external system, drawn as a stadium;
//! - a node's shape comes from `@{ shape: … }` written after its label, with six aliases of its own
//!   (`task`, `tool`, `input`, `decision`, `refdoc`, `action`) that the flowchart's shape table reads.
//!
//! Metadata the picture has no use for, such as `model`, `instruction`, `params` and `connectorRef`,
//! is read and dropped, over several lines when it is written as a block.

use super::flowchart;
use super::scene::Scene;
use super::source::Source;
use super::{Options, Problem};

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let (text, collapsed) = rewrite(source);
    let rewritten =
        Source::read(&text).ok_or_else(|| Problem::whole("This agentflow diagram is empty."))?;
    flowchart::render_collapsing(&rewritten, options, &collapsed)
}

/// What an open block is, so its `end` can be told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Flow,
    Global,
}

/// The source's statements with a metadata block written over several lines joined into one.
fn joined(source: &Source) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = 0_i32;
    for line in source.statements() {
        let text = line.text.trim();
        if open > 0 {
            if let Some(last) = out.last_mut() {
                let piece = text.trim_end_matches(',');
                if !last.trim_end().ends_with('{') && !piece.starts_with('}') {
                    last.push(',');
                }
                last.push(' ');
                last.push_str(piece);
            }
        } else {
            out.push(text.to_owned());
        }
        open += braces(text);
        open = open.max(0);
    }
    out
}

/// How many more `{` than `}` a line has, outside quotes.
fn braces(text: &str) -> i32 {
    let mut count = 0;
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => quoted = !quoted,
            '{' if !quoted => count += 1,
            '}' if !quoted => count -= 1,
            _ => {}
        }
    }
    count
}

/// The word an id is made of at the start of `text`.
fn leading_id(text: &str) -> &str {
    let end =
        text.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).unwrap_or(text.len());
    &text[..end]
}

/// Rewrite the whole source into flowchart text, and say which flows are to be folded.
fn rewrite(source: &Source) -> (String, Vec<String>) {
    let statements = joined(source);
    // The first pass learns which ids are flows and which are connectors, because a metadata line
    // for either draws nothing and may come before or after it.
    let mut flows = Vec::new();
    let mut connectors = Vec::new();
    for text in &statements {
        if let Some(rest) = word(text, "flow") {
            flows.push(leading_id(rest).to_owned());
        } else if let Some(rest) = word(text, "connector") {
            connectors.push(leading_id(rest).to_owned());
        }
    }
    let mut collapsed = Vec::new();
    let mut hoisted = Vec::new();
    let mut body = Vec::new();
    let mut stack: Vec<Block> = Vec::new();
    for text in &statements {
        let in_global = stack.contains(&Block::Global);
        if text.eq_ignore_ascii_case("end") {
            if stack.pop() == Some(Block::Flow) {
                body.push("end".to_owned());
            }
            continue;
        }
        if text.eq_ignore_ascii_case("global") {
            stack.push(Block::Global);
            continue;
        }
        if let Some(rest) = word(text, "flow") {
            let (head, settings) = split_settings(rest);
            if settings.contains("collapsed") {
                collapsed.push(leading_id(head).to_owned());
            }
            body.push(format!("subgraph {}", head.trim()));
            stack.push(Block::Flow);
            continue;
        }
        if let Some(rest) = word(text, "connector") {
            let (head, _) = split_settings(rest);
            let id = leading_id(head);
            let label = head[id.len()..].trim();
            let label = label.trim_start_matches(['[', '(']).trim_end_matches([']', ')']);
            let label = if label.is_empty() { format!("\"{id}\"") } else { label.to_owned() };
            hoisted.push(format!("{id}([{label}])"));
            continue;
        }
        // `id@{ … }` on its own line: settings for a flow or a connector draw nothing, and a flow's
        // `view: collapsed` is the one setting that changes the picture.
        let id = leading_id(text);
        if !id.is_empty() && text[id.len()..].starts_with("@{") {
            if flows.iter().any(|flow| flow == id) {
                if text.contains("collapsed") {
                    collapsed.push(id.to_owned());
                }
                continue;
            }
            if connectors.iter().any(|connector| connector == id) {
                continue;
            }
        }
        let line = with_labels_inside_settings(text);
        if in_global {
            hoisted.push(line);
        } else {
            body.push(line);
        }
    }
    let mut out = String::new();
    if let Some(title) = source.title.as_deref().filter(|title| !title.trim().is_empty()) {
        out.push_str(&format!("---\ntitle: {title}\n---\n"));
    }
    out.push_str(&format!("flowchart {}\n", source.header.trim()));
    for line in hoisted.iter().chain(&body) {
        out.push_str(line);
        out.push('\n');
    }
    (out, collapsed)
}

/// What follows `keyword` when `text` starts with it as a whole word.
fn word<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let head = text.get(..keyword.len())?;
    let rest = &text[keyword.len()..];
    (head.eq_ignore_ascii_case(keyword) && rest.starts_with(char::is_whitespace))
        .then(|| rest.trim())
}

/// `id["Label"]@{ … }` into the part before the settings and the settings themselves.
fn split_settings(text: &str) -> (&str, &str) {
    match text.find("@{") {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    }
}

/// Move a label written in brackets before `@{ … }` inside the settings, where the flowchart reads it.
///
/// Agentflow writes `draft["Draft the notes"]@{ shape: task }`, and the flowchart's own form puts the
/// settings straight after the id: `draft@{ shape: task, label: "Draft the notes" }`.
fn with_labels_inside_settings(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("@{") {
        let before = &rest[..at];
        let close = rest[at..].find('}').map_or(rest.len(), |end| at + end + 1);
        let settings = &rest[at + 2..close.saturating_sub(1).max(at + 2)];
        match bracketed_label(before) {
            Some((open, label)) => {
                let id_start = before[..open]
                    .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                    .map_or(0, |found| found + 1);
                out.push_str(&before[..id_start]);
                let id = &before[id_start..open];
                let has_label =
                    settings.split(',').any(|part| part.trim_start().starts_with("label"));
                if has_label || label.is_empty() {
                    out.push_str(&format!("{id}@{{{settings}}}"));
                } else {
                    let settings = settings.trim();
                    let joiner = if settings.is_empty() { "" } else { ", " };
                    out.push_str(&format!("{id}@{{ {settings}{joiner}label: {label} }}"));
                }
            }
            None => out.push_str(&rest[..close]),
        }
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// When `before` ends with a bracketed label, where its opening bracket is and the label, quoted.
fn bracketed_label(before: &str) -> Option<(usize, String)> {
    let trimmed = before.trim_end();
    let last = trimmed.chars().last()?;
    let open_char = match last {
        ']' => '[',
        ')' => '(',
        '}' => '{',
        _ => return None,
    };
    let mut depth = 0;
    let mut quoted = false;
    for (at, character) in trimmed.char_indices().rev() {
        if character == '"' {
            quoted = !quoted;
        } else if !quoted && character == last {
            depth += 1;
        } else if !quoted && character == open_char {
            depth -= 1;
            if depth == 0 {
                // A doubled bracket such as `[[x]]` opens one character earlier.
                let mut open = at;
                while open > 0 && trimmed[..open].ends_with(['[', '(', '{']) {
                    open -= 1;
                }
                let inner = trimmed[open..]
                    .trim_start_matches(['[', '(', '{'])
                    .trim_end_matches([']', ')', '}']);
                let label =
                    if inner.starts_with('"') { inner.to_owned() } else { format!("\"{inner}\"") };
                return Some((open, label));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    fn rewritten(text: &str) -> (String, Vec<String>) {
        rewrite(&Source::read(text).expect("a diagram"))
    }

    #[test]
    fn a_flow_is_a_subgraph_and_a_label_moves_inside_the_settings() {
        let (text, _) = rewritten(
            "agentflow-beta TB\n flow reviewer[\"Review Agent\"]\n  changes[\"Gather changes\"]@{ shape: input }\n  changes --> analyse\n end\n",
        );
        assert!(text.starts_with("flowchart TB\n"), "{text}");
        assert!(text.contains("subgraph reviewer[\"Review Agent\"]"), "{text}");
        assert!(text.contains("changes@{ shape: input, label: \"Gather changes\" }"), "{text}");
        assert!(text.contains("\nend\n"));
    }

    #[test]
    fn a_global_node_is_declared_before_any_flow() {
        let (text, _) = rewritten(
            "agentflow-beta TB\n flow summariser[\"Summariser\"]\n  summarise -.- corpus\n end\n global\n  corpus[\"Shared corpus\"]@{ shape: refdoc }\n end\n",
        );
        let corpus = text.find("corpus@{").expect("the corpus is declared");
        let flow = text.find("subgraph").expect("the flow");
        assert!(corpus < flow, "{text}");
    }

    #[test]
    fn settings_for_a_flow_or_a_connector_draw_nothing_and_collapsed_is_remembered() {
        let (text, collapsed) = rewritten(
            "agentflow-beta TB\n connector github[\"GitHub API\"]\n github@{ protocol: \"http\" }\n flow processing[\"Processing\"]\n  validate --> enrich\n end\n processing@{ view: \"collapsed\" }\n researcher@{ instruction: \"x\" }\n",
        );
        assert!(!text.contains("github@"), "{text}");
        assert!(text.contains("github([\"GitHub API\"])"), "{text}");
        assert_eq!(collapsed, vec!["processing".to_owned()]);
    }

    #[test]
    fn a_metadata_block_over_several_lines_is_joined() {
        let source = Source::read(
            "agentflow-beta\n fetch@{\n  shape: tool\n  params: \"city :: String\"\n }\n",
        )
        .unwrap();
        let joined = joined(&source);
        assert_eq!(joined.len(), 1, "{joined:?}");
        assert!(joined[0].contains("shape: tool, params"), "{joined:?}");
    }

    #[test]
    fn the_worked_example_draws_with_every_name() {
        check::drawn(
            "agentflow-beta TB\n  brief[\"Release brief\"]@{ shape: input }\n  flow writer[\"Drafting Agent\"]\n    draft[\"Draft the notes\"]@{ shape: task }\n    lookup[\"changelog_search\"]@{ shape: tool }\n    guide[\"Tone of voice\"]@{ shape: refdoc }\n    draft --> lookup\n    draft -.- guide\n  end\n  flow reviewer[\"Review Agent\"]\n    check[\"Check the claims\"]@{ shape: task }\n    ok[\"Accurate?\"]@{ shape: decision }\n    check --> ok\n  end\n  publish[\"Publish\"]@{ shape: action }\n  brief --> writer\n  writer --> reviewer\n  ok --> publish\n  check --x draft\n",
            &options(),
            &["Release brief", "Drafting Agent", "changelog_search", "Tone of voice", "Accurate?", "Publish"],
        );
    }

    #[test]
    fn a_collapsed_flow_is_one_box_that_its_edges_reach() {
        let scene = check::drawn(
            "agentflow-beta TB\n flow intake[\"Intake\"]\n  receive[\"Receive request\"]@{ shape: input }\n end\n flow processing[\"Processing\"]\n  validate[\"Validate\"]@{ shape: task }\n  enrich[\"Enrich\"]@{ shape: task }\n  validate --> enrich\n end\n processing@{ view: \"collapsed\" }\n receive --> validate\n enrich --> publish\n",
            &options(),
            &["Intake", "Receive request", "Processing", "publish"],
        );
        let texts = scene.texts();
        assert!(!texts.contains(&"Validate"), "the folded flow's members are not drawn: {texts:?}");
    }
}
