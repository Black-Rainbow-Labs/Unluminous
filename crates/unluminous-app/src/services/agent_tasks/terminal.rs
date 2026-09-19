//! A ticket's own terminal: the pseudoterminal an agent runs in, and what it has written.
//!
//! **Split out of `mod.rs` by `task-1984` §3.6.** It is a `unluminous_terminal::Session`, which is the
//! same session and the same emulator the terminal tile draws, so there is no second terminal stack
//! inside a program that already has one. The board being replaced ran a daemon in another process
//! for this; §2.3 of the design says what that bought and what replaces it.

use super::*;

/// A ticket's own terminal.
///
/// A `unluminous_terminal::Session`, which is the same session and the same emulator the terminal tile
/// draws, so there is no second terminal stack inside a program that already has one. The board being
/// replaced runs a daemon in another process for this; §2.3 of the design says what that bought and
/// what replaces it.
pub struct TicketTerminal {
    pub task_id: i64,
    pub session: unluminous_terminal::Session,
    /// The conversation the agent in it is having, which is what a later resume names.
    pub session_id: String,
    /// The instant it last printed anything, so the watchdog can tell a working agent from a stopped
    /// one without reading the screen.
    pub last_output_at: String,
    /// Set while it has been paused, because a frozen process cannot answer and its silence means
    /// nothing.
    pub paused: bool,
    /// The lines waiting for the agent's prompt, oldest first.
    ///
    /// A fresh agent has not drawn its prompt yet, and characters typed before it does go nowhere, so the
    /// handoff waits rather than being sent with the spawn — and anything asked for while it waits waits
    /// behind it, in order, which is what keeps a comment from arriving before the line that says which
    /// ticket it is about. `Session` has no timer of its own, so the waiting is held here and
    /// [`TicketTerminal::pump`] is what checks it: one place rather than a thread per ticket.
    pub(super) pending: Vec<String>,
    /// The earliest the queue may be typed, which is [`agent::ready_after`].
    ///
    /// **A floor, not the signal.** It used to be the whole of it, at 1800 ms for Claude, and that lost the
    /// handoff every time: measured on a real window, `claude` took about ten seconds to print its banner,
    /// so the line was typed into a program that had not drawn a prompt and vanished. The agent then sat at
    /// its banner for ever while the ticket said `in_progress`, and a line sent by hand afterwards worked —
    /// which is what proved the terminal was fine and the timing was not.
    pub(super) ready_at: std::time::Instant,
    /// The latest the queue may wait, whatever the session is doing.
    ///
    /// An agent whose prompt animates — a cycling tip, a spinner — never goes quiet, so a rule that waited
    /// only for quiet would wait for ever. See [`TicketTerminal::the_prompt_is_ready`].
    pub(super) give_up_at: std::time::Instant,
    /// When the return for a line already written is due, or `None` when nothing is owed one.
    ///
    /// See [`TicketTerminal::type_line`] for why the return is a write of its own.
    pub(super) submit_at: Option<std::time::Instant>,
    /// How much the session had written the last time it was looked at, so "it printed something" is a
    /// comparison rather than a reading of the screen.
    pub(super) written: usize,
    /// True once the session has printed anything at all. A session that has printed nothing has not
    /// started, and its silence is not the quiet of a prompt waiting for input.
    pub(super) has_printed: bool,
    /// When the session last stopped changing, or `None` while it is still printing.
    pub(super) quiet_since: Option<std::time::Instant>,
}

/// How long a started agent has to be quiet before its prompt is taken to be ready.
///
/// The signal is "it printed its banner and then stopped", which is what a program waiting for input looks
/// like from outside. Reading the screen for a prompt marker was the other candidate and is still refused
/// for the reason `agent::ready_after` gives: a marker in a character grid is a marker a colour scheme or a
/// narrow terminal can move, and every agent spells its prompt differently.
const PROMPT_SETTLES_AFTER: std::time::Duration = std::time::Duration::from_millis(800);

/// How long the queue waits for quiet before giving up and typing anyway.
///
/// Generous, because the cost of waiting is a slow handoff and the cost of not waiting is a lost one.
pub(super) const PROMPT_CEILING: std::time::Duration = std::time::Duration::from_secs(45);

/// The variables that name the Claude Code session a program was started *from*.
///
/// Cleared for a ticket's agent, because that agent is a conversation of its own — see the comment
/// beside the use. Named one at a time rather than by a prefix, so a variable somebody's own profile
/// sets for a reason is not swept up with them.
pub(super) const AGENT_SESSION_MARKERS: &[&str] = &[
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDECODE",
];

/// How long after a line is written its return is pressed.
///
/// Far enough apart that the terminal user interface reads two separate arrivals rather than one burst.
/// Measured against a real `claude`: written together, a 900 character handoff became
/// `❯ [Pasted text #1 +1 lines]` and was never sent. A quarter of a second is longer than any paste
/// debounce and short enough that nobody watching the ticket sees a pause.
const SUBMIT_AFTER: std::time::Duration = std::time::Duration::from_millis(250);

impl std::fmt::Debug for TicketTerminal {
    /// Written by hand because `unluminous_terminal::Session` holds a channel and a thread and has no
    /// `Debug`, and the provider needs one so that a test can print it.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("TicketTerminal")
            .field("task_id", &self.task_id)
            .field("session_id", &self.session_id)
            .field("alive", &self.session.is_running())
            .field("paused", &self.paused)
            .finish()
    }
}

impl TicketTerminal {
    /// Read whatever the agent has written, send the handoff line once its prompt is ready, and record
    /// whether anything was printed.
    ///
    /// Called once a frame while the board is showing and once per watchdog tick, which is what makes
    /// `last_output_at` mean what the watchdog reads it as.
    pub fn pump(&mut self, now: &str) -> bool {
        self.session.pump();
        // The **last screenful** rather than the whole scrollback. `written_text(None)` rebuilds every line
        // the session has ever printed, which on a busy agent is megabytes, and this runs once a frame per
        // ticket. What the question needs is whether anything moved, and the tail answers that: an agent
        // that printed changed its last screenful, and one that printed exactly the same screenful twice is
        // an agent that printed.
        let tail = self.session.written_text(Some(TAIL_LINES));
        let written =
            tail.len() ^ (tail.as_bytes().iter().map(|byte| *byte as usize).sum::<usize>() << 8);
        let printed = written != self.written;
        if printed {
            self.written = written;
            self.last_output_at = now.to_owned();
            self.has_printed = true;
            self.quiet_since = None;
        } else if self.quiet_since.is_none() {
            self.quiet_since = Some(std::time::Instant::now());
        }
        // The return owed for a line already written, first: nothing else may be typed into a prompt
        // that has not been sent, or the two would go as one message.
        if self.submit_at.is_some_and(|due| std::time::Instant::now() >= due) {
            self.submit_at = None;
            self.session.send(b"\r".to_vec());
        }
        // Then **one** line, not all of them, because each needs its own return after it.
        let waiting = !self.pending.is_empty() || self.submit_at.is_some();
        if self.submit_at.is_none() && !self.pending.is_empty() && self.the_prompt_is_ready() {
            let line = self.pending.remove(0);
            self.type_line(&line);
        }
        printed || waiting
    }

    /// Whether a line is still waiting for the agent's prompt, which is what `terminal` reports.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// [`Self::the_prompt_is_ready`], for the `terminal` command to report.
    pub fn prompt_is_ready(&self) -> bool {
        self.the_prompt_is_ready()
    }

    /// Whether the agent is at a prompt that will take what is typed into it.
    ///
    /// Three parts, and each answers a way this was wrong before. The **floor** is
    /// [`agent::ready_after`], because a session that has printed nothing yet is not quiet, it has not
    /// started. Then it has to have **printed something and gone quiet** for [`PROMPT_SETTLES_AFTER`],
    /// which is what a program waiting for input looks like from outside and is the part that was missing:
    /// with a fixed delay alone, a `claude` that took ten seconds to draw its banner was typed into after
    /// 1800 milliseconds and the line was lost. And the **ceiling** is [`PROMPT_CEILING`], so an agent whose
    /// prompt never stops animating still gets its handoff.
    fn the_prompt_is_ready(&self) -> bool {
        let now = std::time::Instant::now();
        if now < self.ready_at {
            return false;
        }
        if now >= self.give_up_at {
            return true;
        }
        self.has_printed
            && self
                .quiet_since
                .is_some_and(|since| now.duration_since(since) >= PROMPT_SETTLES_AFTER)
    }

    /// Write a line into the agent, and press return a beat later.
    ///
    /// **The return is a second write, and that is the whole of why anything works.** Both used to go out
    /// together, and measured against a real `claude` the prompt then read
    /// `❯ [Pasted text #1 +1 lines]` and sat there: a burst that arrives in one read is a *paste* to a
    /// terminal user interface, and a paste whose bytes include the terminator is a multi-line paste,
    /// which claude keeps as an attachment for the person to submit rather than submitting itself. So the
    /// handoff was typed, correctly, into a prompt that never sent it — `queued` said false,
    /// `prompt_ready` said true, the board said the ticket was being worked on, and the agent had been
    /// handed nothing. Nothing on the screen said why, which is what the `terminal` command is for.
    ///
    /// Written apart by [`SUBMIT_AFTER`] the return arrives as its own keystroke, which is what a person
    /// pasting a prompt and pressing Enter does.
    pub fn type_line(&mut self, line: &str) {
        self.session.send(line.as_bytes().to_vec());
        self.submit_at = Some(std::time::Instant::now() + SUBMIT_AFTER);
    }

    /// Send a line, or queue it when the prompt is not ready or a return is still owed.
    ///
    /// Answers whether it was queued. An agent that has just been started has not drawn its prompt, and
    /// characters typed before it does go nowhere, so a line written straight in would look sent and would
    /// not be. Everything waits in one queue, in the order it was asked for, which is also what keeps a
    /// comment from arriving before the handoff that says which ticket it is about — and now also what
    /// keeps a second line from being written into a prompt whose return has not gone yet, which would
    /// send the two as one message.
    pub fn queue(&mut self, line: &str) -> bool {
        match self.pending.is_empty() && self.submit_at.is_none() && self.the_prompt_is_ready() {
            true => {
                self.type_line(line);
                false
            }
            false => {
                self.pending.push(line.to_owned());
                true
            }
        }
    }
}
