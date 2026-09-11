//! What a connection carries, and the one rule that stops it going round for ever.
//!
//! `task-1904`: *"If I have 2 terminal nodes connected, they should be able to pipe text back and
//! forth."* Back and forth is two edges, one each way — and a shell **echoes what is typed into it**,
//! so a line sent from A to B comes straight back out of B's output and would be sent to A, which
//! echoes it, and neither program ever printed anything again.
//!
//! The rule is one sentence: **a line that arrived through a pipe is never sent back out.** It is
//! exactly the echo and nothing else, because the echo of a line is that line. Everything the program
//! itself writes still goes.
//!
//! ## Reading what a program wrote
//!
//! `unluminous_terminal::Session` parses the bytes it reads straight into the emulator on its own
//! thread, so there is no stream of raw output to hold on to: what a program wrote is read back out of
//! the terminal's own scrollback by `Session::written_text`. That answers with the **last so many
//! lines**, so following it needs an anchor — the last line already sent — and everything after that
//! line is new. [`Tap`] is that anchor and the echo list together.
//!
//! Two things follow and both are deliberate. A pipe turned on **starts from the end**, so wiring two
//! terminals does not empty one into the other; and a program that outruns the tail between two
//! readings has its anchor lost, which is answered by sending the whole tail once rather than by
//! guessing — a pipe is read several times a second and a program would have to write hundreds of
//! lines between two readings to do it.
//!
//! This file is those rules and nothing else, so the loop is a unit test with no window, no terminal
//! and no process.

use std::collections::VecDeque;

/// How many lines of a program's output are read at a time.
///
/// Bounded, because `written_text` builds a string for each line it is asked for and this runs several
/// times a second. Four hundred is far more than a program writes between two readings.
pub const TAIL: usize = 400;

/// How many piped lines a node remembers having been sent.
///
/// Bounded, because a node that ran for a day would otherwise remember every line ever piped into it.
/// Sixty four is far more than the number of lines that can be in flight between a write and the echo
/// of it, which is what the list is really for.
pub const REMEMBERED: usize = 64;

/// What a node has recently been sent through a pipe, waiting to see its own echo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Echoes {
    lines: VecDeque<String>,
}

impl Echoes {
    /// Remember that `line` was typed into this node, so its echo is not sent on.
    fn sent(&mut self, line: &str) {
        if self.lines.len() >= REMEMBERED {
            self.lines.pop_front();
        }
        self.lines.push_back(line.to_owned());
    }

    /// Whether `line` is the echo of something piped in, taking it off the list when it is.
    ///
    /// Taken rather than merely read, so a program that really does print the same line twice has the
    /// second one forwarded. One echo for one send.
    fn take_the_echo_of(&mut self, line: &str) -> bool {
        match self.lines.iter().position(|waiting| waiting == line) {
            Some(at) => {
                self.lines.remove(at);
                true
            }
            None => false,
        }
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// What one node's outgoing pipes have already read, and what has been typed into it.
///
/// One of these a node rather than one an edge: what a program wrote is the node's, and every pipe
/// out of it carries the same lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tap {
    /// The last line that has been sent on. Empty before anything has been read.
    anchor: String,
    echoes: Echoes,
}

impl Tap {
    /// Start following a program's output from where it is now.
    ///
    /// **From the end**, so turning a pipe on does not empty one terminal's history into another.
    pub fn following(tail: &[String]) -> Tap {
        Tap { anchor: tail.last().cloned().unwrap_or_default(), echoes: Echoes::default() }
    }

    /// Remember that `line` was typed into this node, so its echo is not sent back out.
    pub fn sent(&mut self, line: &str) {
        self.echoes.sent(line);
    }

    /// The lines of `tail` that have not been sent yet, with the echoes of piped lines taken out.
    ///
    /// Moves the anchor to the end of `tail`, so a line is sent exactly once.
    pub fn take(&mut self, tail: &[String]) -> Vec<String> {
        let fresh = new_lines(tail, &self.anchor);
        let sending: Vec<String> = fresh
            .iter()
            .filter(|line| !self.echoes.take_the_echo_of(line))
            .filter(|line| !line.trim().is_empty())
            .cloned()
            .collect();
        if let Some(last) = tail.last() {
            self.anchor = last.clone();
        }
        sending
    }

    /// True when nothing is waiting to see its own echo, which is what a settled pipe looks like.
    pub fn is_settled(&self) -> bool {
        self.echoes.is_empty()
    }
}

/// The part of `tail` written after `anchor`.
///
/// The **last** occurrence of the anchor is the one used, because a program that printed the same
/// line twice printed the second one more recently. An anchor that is not in the tail at all means the
/// program outran the reading, and the whole tail is new — which is the honest answer, since the lines
/// in between are gone and guessing which of what is left was already sent would send some of it twice.
pub fn new_lines<'a>(tail: &'a [String], anchor: &str) -> &'a [String] {
    if anchor.is_empty() {
        return tail;
    }
    match tail.iter().rposition(|line| line == anchor) {
        Some(at) => &tail[at + 1..],
        None => tail,
    }
}

/// The lines in `text`, trimmed of the carriage returns a Windows program writes.
///
/// `Session::written_text` answers with the rows of the terminal joined by newlines, so this is the
/// reading of that answer and the one place it is done.
pub fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(|line| line.trim_end_matches('\r').to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(of: &[&str]) -> Vec<String> {
        of.iter().map(|line| (*line).to_owned()).collect()
    }

    #[test]
    fn a_pipe_starts_from_the_end_rather_than_emptying_a_history_into_the_other_terminal() {
        let history = lines(&["one", "two", "three"]);
        let mut tap = Tap::following(&history);
        assert!(tap.take(&history).is_empty(), "nothing written since the pipe was turned on");
        let more = lines(&["one", "two", "three", "four"]);
        assert_eq!(tap.take(&more), lines(&["four"]));
    }

    #[test]
    fn what_a_program_wrote_is_forwarded_exactly_once() {
        let mut tap = Tap::following(&[]);
        let first = lines(&["hello"]);
        assert_eq!(tap.take(&first), lines(&["hello"]));
        assert!(tap.take(&first).is_empty(), "the same line is not sent twice");
        let second = lines(&["hello", "world"]);
        assert_eq!(tap.take(&second), lines(&["world"]));
    }

    #[test]
    fn a_line_piped_between_two_terminals_wired_both_ways_does_not_go_round_for_ever() {
        // A is wired to B and B is wired to A. A's program writes `build`, which is typed into B; B's
        // shell echoes it, and without the echo rule that echo is typed into A, which echoes it, and
        // neither program ever prints anything again.
        let mut a = Tap::following(&[]);
        let mut b = Tap::following(&[]);

        let from_a = a.take(&lines(&["build"]));
        assert_eq!(from_a, lines(&["build"]));
        for line in &from_a {
            b.sent(line);
        }

        // B echoes it, and then answers with something of its own.
        let from_b = b.take(&lines(&["build", "done"]));
        assert_eq!(from_b, lines(&["done"]), "the echo was dropped and the answer was not");
        for line in &from_b {
            a.sent(line);
        }

        // A echoes B's answer. Nothing comes back out, so the loop has ended.
        assert!(a.take(&lines(&["build", "done"])).is_empty(), "the loop would run for ever");
        assert!(a.is_settled() && b.is_settled(), "nothing is left waiting for an echo");
    }

    #[test]
    fn a_program_that_really_prints_the_same_line_twice_has_the_second_one_forwarded() {
        // One echo for one send: the list is taken from rather than read, so a repeat that was not an
        // echo is not swallowed as though it were.
        let mut tap = Tap::following(&[]);
        tap.sent("again");
        assert_eq!(tap.take(&lines(&["again", "again"])), lines(&["again"]));
    }

    #[test]
    fn blank_lines_are_not_sent() {
        // A shell writes a blank line between an answer and its next prompt, and a pipe that sent
        // those would press Enter in the other terminal several times a second.
        let mut tap = Tap::following(&[]);
        assert_eq!(tap.take(&lines(&["", "  ", "real"])), lines(&["real"]));
    }

    #[test]
    fn the_last_occurrence_of_the_anchor_is_the_one_followed() {
        // A prompt is written again and again, so the anchor is very often a line that appears many
        // times. The most recent one is the one that was last sent.
        let tail = lines(&["$", "ls", "$", "cat x", "$"]);
        assert!(new_lines(&tail, "$").is_empty());
        assert_eq!(new_lines(&tail, "ls"), &tail[2..]);
        assert_eq!(new_lines(&tail, ""), &tail[..]);
    }

    #[test]
    fn a_program_that_outran_the_reading_has_its_whole_tail_sent_once_rather_than_guessed_at() {
        let mut tap = Tap::following(&lines(&["old"]));
        let raced = lines(&["far", "past", "the", "anchor"]);
        assert_eq!(tap.take(&raced), raced, "the lines in between are gone; guessing would send some twice");
        assert!(tap.take(&raced).is_empty(), "and the anchor has caught up");
    }

    #[test]
    fn the_list_of_echoes_is_bounded() {
        let mut echoes = Echoes::default();
        for line in 0..REMEMBERED * 3 {
            echoes.sent(&line.to_string());
        }
        assert_eq!(echoes.lines.len(), REMEMBERED);
        assert!(echoes.take_the_echo_of(&(REMEMBERED * 3 - 1).to_string()));
        assert!(!echoes.take_the_echo_of("0"), "the oldest were dropped");
    }

    #[test]
    fn the_carriage_returns_a_windows_program_writes_are_taken_off() {
        assert_eq!(lines_of("one\r\ntwo\nthree"), lines(&["one", "two", "three"]));
        assert!(lines_of("").is_empty());
    }
}
