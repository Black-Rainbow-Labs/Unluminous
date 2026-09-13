//! The rule that keeps the documentation honest.
//!
//! `task-1661` asks that any new feature come with a CLI command **and be documented**. A rule
//! nothing enforces is a rule that lasts until the first busy afternoon, so this is the enforcement:
//! every command in the catalogue must have a heading of its own in `unluminous-cli/docs/commands.md`,
//! with its usage line under it, and nothing may be documented that no longer exists.
//!
//! It fails loudly and says exactly what to add, because the person it is talking to has just
//! written the command and is about to write the paragraph.

use crate::catalogue::{self, Command};

/// Read the written reference. It sits beside this crate, so its path is relative to the manifest.
fn reference() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/commands.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|problem| panic!("could not read {}: {problem}", path.display()))
}

/// Read the protocol document, the same way.
fn protocol() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/protocol.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|problem| panic!("could not read {}: {problem}", path.display()))
}

/// Every command that waits for an answer rather than replying with whatever the window already
/// knows at the top of its next frame -- which is exactly the commands with a `timeout` or a `wait`
/// flag of their own, since that is the flag a caller reaches for to say how long to wait.
fn waiting_commands() -> Vec<&'static Command> {
    catalogue::COMMANDS
        .iter()
        .filter(|command| command.flag("timeout").is_some() || command.flag("wait").is_some())
        .collect()
}

/// The heading a command is documented under: `### tab open`.
fn heading(command: &Command) -> String {
    format!("### {}", command.typed())
}

#[test]
fn every_command_has_a_section_in_the_written_reference() {
    let text = reference();
    let missing: Vec<String> = catalogue::COMMANDS
        .iter()
        .filter(|command| !text.contains(&heading(command)))
        .map(heading)
        .collect();
    assert!(
        missing.is_empty(),
        "unluminous-cli/docs/commands.md is missing {} command{}:\n{}\n\
         Add a `### <area> <verb>` section for each, with its usage line and an example.",
        missing.len(),
        if missing.len() == 1 { "" } else { "s" },
        missing.join("\n")
    );
}

#[test]
fn every_commands_usage_line_is_written_out_where_it_is_documented() {
    // The usage line is the one thing a reader copies, so it has to be the real one rather than a
    // remembered one. It is generated from the catalogue, so this is checking that the paragraph
    // was written against the command as it is now.
    let text = reference();
    let wrong: Vec<String> = catalogue::COMMANDS
        .iter()
        .filter(|command| !text.contains(&command.usage()))
        .map(|command| format!("{}\n    expected: {}", command.typed(), command.usage()))
        .collect();
    assert!(
        wrong.is_empty(),
        "unluminous-cli/docs/commands.md does not carry the current usage line for:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn every_area_heading_appears_once() {
    // `examples/reference.rs` starts a new `## <heading>` each time the area changes as it walks
    // `COMMANDS` in order, so two runs of the same area with a different one between them print its
    // heading twice. That happened to `editor`, split by `update check`, and to `space`, split by
    // `input`, until `task-1922` WP2 made every area a single contiguous run. A heading that comes
    // back here means an area has come apart again.
    let text = reference();
    let mut areas: Vec<&'static str> = vec![""];
    areas.extend(catalogue::areas());
    for area in areas {
        let heading = format!("## {}\n", catalogue::area_title(area));
        let seen = text.matches(&heading).count();
        assert_eq!(
            seen, 1,
            "{heading:?} appears {seen} times in unluminous-cli/docs/commands.md -- COMMANDS no \
             longer keeps every {area:?} command together"
        );
    }
}

#[test]
fn nothing_is_documented_that_no_longer_exists() {
    let text = reference();
    let stale: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("### "))
        .map(|line| line.trim_start_matches("### ").trim())
        .filter(|name| catalogue::find(name).is_none())
        .collect();
    assert!(
        stale.is_empty(),
        "unluminous-cli/docs/commands.md documents commands that do not exist: {}",
        stale.join(", ")
    );
}

#[test]
fn the_protocol_document_names_exactly_the_commands_that_wait() {
    // `docs/protocol.md` used to say "Four commands are answered later than the frame they arrived
    // on" and name four, by hand, while the catalogue already held thirteen call sites naming a
    // `timeout` or a `wait` flag. This reads the same paragraph back and checks it against the
    // catalogue rather than trusting the sentence to have been kept up to date by hand a second
    // time. The anchor phrase has to stay in the document for this to find the list at all.
    // Markdown wraps this paragraph's lines by hand, so the anchors below are matched against the
    // text with every run of whitespace collapsed to one space rather than against the file's own
    // line breaks, which are not meaningful and move whenever the paragraph is reflowed.
    let text = protocol().split_whitespace().collect::<Vec<_>>().join(" ");
    let anchor = "flag of its own:";
    let after_start = text.find(anchor).unwrap_or_else(|| {
        panic!(
            "unluminous-cli/docs/protocol.md has no {anchor:?} -- \
             the paragraph naming the commands that wait has moved or been reworded"
        )
    });
    let list_start = after_start + anchor.len();
    let list_end = text[list_start..].find("Each has a timeout").unwrap_or_else(|| {
        panic!(
            "unluminous-cli/docs/protocol.md's waiting-commands paragraph has no closing sentence"
        )
    }) + list_start;
    let listed_text = &text[list_start..list_end];
    let mut named: Vec<String> = Vec::new();
    let mut rest = listed_text;
    while let Some(open) = rest.find('`') {
        rest = &rest[open + 1..];
        let close = rest.find('`').expect("a closing backtick to match the opening one");
        named.push(rest[..close].to_owned());
        rest = &rest[close + 1..];
    }
    let mut expected: Vec<String> =
        waiting_commands().iter().map(|command| command.typed()).collect();
    named.sort();
    expected.sort();
    assert_eq!(
        named, expected,
        "unluminous-cli/docs/protocol.md's list of commands that wait does not match the catalogue"
    );
}

#[test]
fn the_reference_leads_with_what_an_agent_needs_before_anything_else() {
    // The document's whole purpose is to be handed to an agent, so the first thing in it has to be
    // the two facts that make every other line usable: how to find out what commands exist, and
    // that a program should always ask for JSON.
    let text = reference();
    let opening: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
    assert!(opening.contains("--json"), "the opening should say to pass --json");
    assert!(
        opening.contains("unluminous-cli commands"),
        "the opening should say how to list the commands"
    );
}
