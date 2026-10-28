//! A menu the agent in a ticket's terminal is waiting on, read off its screen, and the keys that answer it.
//!
//! `task-2263`: *"I can't select yes. I want that to be auto accepted as yes, and if not possible the agent chat
//! should also show buttons to allow it."* `folder_trust` answers the folder question before the agent starts, so
//! this is for every other menu an agent can stop on: a folder whose trust could not be recorded, the bypass
//! permissions question, a plan to approve. Claude Code draws each of them the same way, as a list of options with
//! `❯` in front of the one the arrow keys are on:
//!
//! ```text
//!  Do you trust the files in this folder?
//!
//!  ❯ 1. Yes, proceed
//!    2. No, exit
//! ```
//!
//! The ticket's Chat tab reads the transcript, and a menu is never in the transcript, so the Chat tab showed
//! nothing while the agent waited. [`read`] finds the menu on the last screenful and [`keys_for`] is what pressing
//! one of its options types: the arrow keys from the option the cursor is on to the one chosen, then Enter, each a
//! separate write so the terminal user interface reads keystrokes rather than a paste.
//!
//! Reading the screen is refused for the prompt itself (`terminal::PROMPT_SETTLES_AFTER` says why), and the same
//! risk applies here, so the rule is narrow: two or more option lines in one column, exactly one of them carrying
//! the cursor, and not inside the input box, whose single line also starts with `❯`.

/// One menu on the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// The line above the options that says what is being asked, when there is one.
    pub question: String,
    /// The options, top to bottom, with any `1.` in front taken off.
    pub options: Vec<String>,
    /// Which option the cursor is on.
    pub cursor: usize,
}

/// How a comment saying the agent is waiting on a menu starts (`task-2392`). The ticket modal draws the menu's options
/// as buttons under the newest such comment while the menu is still on the screen.
pub const WAITING: &str = "**Waiting for your answer**";

/// How a comment saying which option was chosen starts. A waiting comment with one of these after it has been
/// answered.
pub const ANSWERED: &str = "**Answered**";

/// The comment posted on a ticket when its agent stops on `choice` (`task-2392`: *"we need the same thing in task of
/// agent task, where a comment is added (as well as the agent chat dialog) that allows the user to select the action
/// to take."*). The options are a numbered list, so the comment reads on its own in the board's other views and in
/// `task <key>`, and the number is what `choice <key> <n>` takes.
pub fn comment_for(choice: &Choice, key: &str) -> String {
    let asked = match choice.question.trim().is_empty() {
        true => "The agent is waiting for an answer in its terminal.".to_owned(),
        false => choice.question.trim().to_owned(),
    };
    let options: Vec<String> = choice
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| format!("{}. {option}", index + 1))
        .collect();
    format!(
        "{WAITING} \u{b7} {asked}\n\n{}\n\nChoose below, on the Chat tab, or with `plugins run agent-tasks choice {key} <number>`.",
        options.join("\n")
    )
}

/// The comment posted when `option` was chosen.
pub fn answered_comment(option: &str) -> String {
    format!("{ANSWERED} \u{b7} {option}")
}

/// Whether the newest of `bodies` that is about a menu is a question nobody has answered yet.
pub fn the_newest_is_waiting<'a>(bodies: impl DoubleEndedIterator<Item = &'a str>) -> bool {
    bodies
        .rev()
        .find(|body| body.starts_with(WAITING) || body.starts_with(ANSWERED))
        .is_some_and(|body| body.starts_with(WAITING))
}

/// The cursor Claude Code and Codex draw in front of the option the arrow keys are on.
const CURSOR: [char; 2] = ['❯', '›'];

/// How many lines from the bottom of the screen a menu is looked for in.
pub const LOOK_BACK: usize = 30;

/// The menu on `screen`, the last screenful of a terminal, or `None` when it is not waiting on one.
pub fn read(screen: &str) -> Option<Choice> {
    let lines: Vec<String> = screen.lines().map(without_the_border).collect();
    let start = lines.len().saturating_sub(LOOK_BACK);
    let cursor_at = (start..lines.len()).rev().find(|&at| cursor_column(&lines[at]).is_some())?;
    let column = cursor_column(&lines[cursor_at])?;
    let (first, last) = the_block(&lines, cursor_at, column);
    if last == first {
        return None;
    }
    // The input box is drawn between two rules, so a cursor line with a rule straight above or below it is the
    // prompt, which a person types into and never answers with a button.
    let ruled = |at: Option<usize>| {
        at.and_then(|at| lines.get(at)).is_some_and(|line| is_a_plain_rule(line))
    };
    if ruled(first.checked_sub(1)) || ruled(Some(last + 1)) {
        return None;
    }
    // **And the input box is not drawn while Claude Code waits on a menu**, so a cursor line with the input box
    // anywhere below it is not a menu either. It is a message somebody typed, which Claude Code echoes with `❯`
    // in front and its wrapped lines indented under it, exactly like a list of options. `task-2268` drew such an
    // echo as a menu: the "Brewed for 12s" line above it as the question, and each wrapped line as a button.
    if lines[last + 1..].iter().any(|line| is_a_plain_rule(line)) {
        return None;
    }
    let options: Vec<String> =
        (first..=last).map(|at| without_a_number(option_text(&lines[at], column))).collect();
    if options.iter().any(|option| option.is_empty()) {
        return None;
    }
    let question = lines[..first]
        .iter()
        .rev()
        .map(|line| line.trim())
        .find(|line| !line.is_empty() && !is_a_rule(line))
        .unwrap_or_default()
        .to_owned();
    Some(Choice { question, options, cursor: cursor_at - first })
}

/// The keys that move the cursor from `choice.cursor` to `option` and press Enter, one write each.
pub fn keys_for(choice: &Choice, option: usize) -> Result<Vec<Vec<u8>>, String> {
    if option >= choice.options.len() {
        return Err(format!(
            "there is no option {} to choose: the menu has {} ({})",
            option + 1,
            choice.options.len(),
            choice.options.join(", ")
        ));
    }
    let (key, steps): (&[u8], usize) = match option >= choice.cursor {
        true => (b"\x1b[B", option - choice.cursor),
        false => (b"\x1b[A", choice.cursor - option),
    };
    let mut keys: Vec<Vec<u8>> = (0..steps).map(|_| key.to_vec()).collect();
    keys.push(b"\r".to_vec());
    Ok(keys)
}

/// A line with a box's side drawn round it taken away, so a menu inside a frame reads like one outside it.
fn without_the_border(line: &str) -> String {
    let trimmed = line.trim_end();
    let trimmed = trimmed.strip_suffix('│').unwrap_or(trimmed).trim_end();
    match trimmed.trim_start().strip_prefix('│') {
        Some(rest) => format!(" {rest}"),
        None => trimmed.to_owned(),
    }
}

/// Where the cursor is on a line that starts with one followed by words, counted in characters.
fn cursor_column(line: &str) -> Option<usize> {
    let indent = line.chars().take_while(|c| c.is_whitespace()).count();
    let mut rest = line.chars().skip(indent);
    let first = rest.next()?;
    let words: String = rest.collect();
    (CURSOR.contains(&first) && words.starts_with(' ') && !words.trim().is_empty())
        .then_some(indent)
}

/// The first and last line of the options around the cursor's line: the lines next to it whose words start in the
/// same column, with nothing but spaces in front.
fn the_block(lines: &[String], cursor_at: usize, column: usize) -> (usize, usize) {
    let is_an_option = |line: &String| {
        let indent = line.chars().take_while(|c| c.is_whitespace()).count();
        indent == column + 2 || indent == column + 1
    };
    let mut first = cursor_at;
    while first > 0 && is_an_option(&lines[first - 1]) {
        first -= 1;
    }
    let mut last = cursor_at;
    while last + 1 < lines.len() && is_an_option(&lines[last + 1]) {
        last += 1;
    }
    (first, last)
}

/// An option's words: the line after the cursor's column, with the cursor taken off when it is there.
fn option_text(line: &str, column: usize) -> String {
    let rest: String = line.chars().skip(column).collect();
    let rest = rest.trim_start_matches(CURSOR).trim();
    rest.to_owned()
}

/// `1. Yes, proceed` as `Yes, proceed`.
fn without_a_number(option: String) -> String {
    let digits = option.chars().take_while(char::is_ascii_digit).count();
    match digits > 0 && option[digits..].starts_with(". ") {
        true => option[digits + 2..].trim().to_owned(),
        false => option,
    }
}

/// A rule with no corners, which is what Claude Code draws the input box between. A menu in a framed box has
/// corners on its top and bottom lines, so it is not mistaken for the input box.
fn is_a_plain_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 12 && trimmed.chars().all(|c| matches!(c, '─' | '━' | '-'))
}

/// A line drawn as a horizontal rule, with or without corners.
fn is_a_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 12
        && trimmed.chars().all(|c| matches!(c, '─' | '━' | '-' | '╭' | '╮' | '╰' | '╯' | '═'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trust question exactly as the ticket quotes it.
    #[test]
    fn the_folder_question_is_read_with_the_cursor_on_no() {
        let screen = "\n Claude Code'll be able to read, edit, and execute files here.\n\n Security guide\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";
        let choice = read(screen).expect("a menu");
        assert_eq!(choice.options, ["No, exit", "Yes, I trust this folder"]);
        assert_eq!(choice.cursor, 0);
        assert_eq!(choice.question, "Security guide");
        assert_eq!(keys_for(&choice, 1).unwrap(), vec![b"\x1b[B".to_vec(), b"\r".to_vec()]);
    }

    #[test]
    fn a_numbered_menu_in_a_box_is_read_and_answered_upwards() {
        let screen = "╭──────────────────────────────╮\n│ Do you want to proceed?      │\n│   1. Yes                     │\n│ ❯ 2. No, and tell Claude     │\n╰──────────────────────────────╯\n";
        let choice = read(screen).expect("a menu");
        assert_eq!(choice.question, "Do you want to proceed?");
        assert_eq!(choice.options, ["Yes", "No, and tell Claude"]);
        assert_eq!(choice.cursor, 1);
        assert_eq!(keys_for(&choice, 0).unwrap(), vec![b"\x1b[A".to_vec(), b"\r".to_vec()]);
        assert!(keys_for(&choice, 2).is_err());
    }

    #[test]
    fn the_prompt_between_its_rules_is_not_a_menu() {
        let screen = "● Done.\n\n────────────────────────────────────\n❯ fix the bug\n────────────────────────────────────\n  ⏵⏵ bypass permissions on\n";
        assert_eq!(read(screen), None);
        let empty =
            "────────────────────────────────────\n❯ \n────────────────────────────────────\n";
        assert_eq!(read(empty), None);
    }

    /// `task-2268`: a long message typed into the agent is echoed with `❯` in front and its wrapped lines
    /// indented under it, above the input box. It was read as a menu whose question was Claude Code's status line
    /// and whose options were the wrapped lines.
    #[test]
    fn a_message_echoed_above_the_input_box_is_not_a_menu() {
        let screen = "● task-29 has nothing left to do.\n\n✻ Brewed for 12s · done 11:57 AM\n\n❯ Read the ticket, its todos and its newest comments, and work out whether\n  anything is left. Post a comment saying what you found and what the operator\n  has to decide or provide, then set the status to agent_done.\n\n────────────────────────────────────────────\n❯ \n────────────────────────────────────────────\n  ⏵⏵ bypass permissions on\n";
        assert_eq!(read(screen), None);
        // And a real menu, which Claude Code draws in place of the input box, is still read.
        let menu = "✻ Brewed for 12s\n\n Do you want to make this edit?\n ❯ 1. Yes\n   2. Yes, and don't ask again\n   3. No, and tell Claude\n\n Esc to cancel\n";
        let choice = read(menu).expect("a menu");
        assert_eq!(choice.options.len(), 3);
        assert_eq!(choice.question, "Do you want to make this edit?");
    }

    #[test]
    fn a_single_line_with_a_cursor_is_not_a_menu() {
        assert_eq!(read("some output\n ❯ only one\n\nmore"), None);
        assert_eq!(read("nothing waiting here\n"), None);
    }

    /// `task-2392`: a menu is posted on the ticket as a comment whose options are numbered as `choice` takes them,
    /// and a comment saying what was chosen ends the wait.
    #[test]
    fn a_menu_is_a_comment_and_an_answer_ends_it() {
        let choice = Choice {
            question: "Do you want to make this edit to a.rs?".to_owned(),
            options: vec![
                "Yes".to_owned(),
                "No, and tell Claude what to do differently".to_owned(),
            ],
            cursor: 0,
        };
        let body = comment_for(&choice, "task-7");
        assert!(body.starts_with(WAITING));
        assert!(body.contains(
            "1. Yes
2. No, and tell Claude"
        ));
        assert!(body.contains("choice task-7 <number>"));
        let answered = answered_comment("Yes");
        assert!(the_newest_is_waiting(["hello", body.as_str()].into_iter()));
        assert!(!the_newest_is_waiting([body.as_str(), answered.as_str(), "thanks"].into_iter()));
        assert!(!the_newest_is_waiting(["nothing about a menu"].into_iter()));
    }
}
