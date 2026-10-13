//! The ticket modal's Chat tab: the agent in the ticket's terminal, as an Agent-Chat conversation.
//!
//! `task-2261`, and `tasks/task-2261-ticket-agent-chat-tdd.md` §1 is the design. The agent working a ticket is
//! `claude` in the ticket's own terminal, started with `--session-id <the id on the row>`, and Claude Code writes
//! that conversation to `~/.claude/projects/<folder>/<id>.jsonl` as it goes. A [`TicketChat`] reads that file into
//! an `AgentChat` in mirror mode, which the modal draws with the side panel's own pane, and takes what the pane's
//! composer asked for so the board can type it into the terminal. The chat and the terminal are therefore two
//! views of one process: nothing here starts an agent or writes to the transcript.
//!
//! The file is appended to and never rewritten, so it is read from where the last read stopped, at most every
//! [`LOOK_EVERY`]. A line Claude Code was half way through writing is kept and finished on the next read.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use unluminous_chat::transcript::{self, Transcript};

use super::model::{Assignee, Task};
use crate::services::agent_chat::AgentChat;

/// How often the transcript's length is looked at.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

/// How long a message typed into the chat is shown as waiting before it is taken to have arrived anyway.
///
/// The transcript records what the terminal received, and a long message typed into Claude Code's prompt can be
/// recorded differently from how it was typed (a paste, a trailing space). Matching by the words is the rule, and
/// this is what keeps a message that never matches from waiting for ever once newer prompts have arrived.
const HEARD_AFTER: std::time::Duration = std::time::Duration::from_secs(20);

/// The variable a test sets to name Claude Code's own folder, so it never reads the person's `~/.claude`.
pub const CLAUDE_HOME_VARIABLE: &str = "UNLUMINOUS_CLAUDE_HOME";

/// Claude Code's own folder: [`CLAUDE_HOME_VARIABLE`], then `CLAUDE_CONFIG_DIR`, then `~/.claude`.
pub fn claude_home() -> Option<PathBuf> {
    for variable in [CLAUDE_HOME_VARIABLE, "CLAUDE_CONFIG_DIR"] {
        if let Some(named) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(named));
        }
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".claude"))
}

/// Which tab of the ticket modal's Agent section is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTab {
    Chat,
    Terminal,
}

impl AgentTab {
    /// The word `agent-tab` takes and answers.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Terminal => "terminal",
        }
    }

    pub fn parse(said: &str) -> Option<Self> {
        match said.trim().to_ascii_lowercase().as_str() {
            "chat" => Some(Self::Chat),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    /// The tab a ticket opens on when nobody has chosen one: the chat for an agent whose transcript Unluminous
    /// can read, which is Claude, and the terminal for every other.
    pub fn default_for(agent: Assignee) -> Self {
        match chat_is_readable(agent) {
            true => Self::Chat,
            false => Self::Terminal,
        }
    }
}

/// Whether the Chat tab can show `agent`'s conversation.
///
/// Claude Code writes its transcript under the session id the ticket chose, so the file can be found. Codex and
/// OpenCode name their own sessions, so the id on the ticket names nothing they wrote: the reason
/// `agent::can_resume` gives, about the same fact.
pub fn chat_is_readable(agent: Assignee) -> bool {
    matches!(agent, Assignee::Claude)
}

/// What the Chat tab says for an agent whose conversation it cannot read.
pub fn why_the_chat_is_empty(agent: Assignee) -> String {
    format!(
        "{} keeps its conversation where Unluminous does not read it, so its work is in the Terminal tab.",
        agent.name()
    )
}

/// The open ticket's conversation, read from its agent's transcript.
pub struct TicketChat {
    pub task_id: i64,
    /// The session the transcript belongs to. A ticket started again with a new session gets a new chat.
    pub session: String,
    /// The folder the agent was started in, which is where Claude Code files the transcript.
    folder: PathBuf,
    pub chat: AgentChat,
    transcript: Transcript,
    /// The file, once it has been found.
    path: Option<PathBuf>,
    /// How much of it has been read, and the part of a line after that.
    offset: u64,
    partial: String,
    /// When the file was last looked at.
    looked: Option<std::time::Instant>,
    /// When each message still shown as waiting was typed, oldest first, beside `AgentChat::queued`.
    sent: Vec<std::time::Instant>,
}

impl std::fmt::Debug for TicketChat {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("TicketChat")
            .field("task_id", &self.task_id)
            .field("session", &self.session)
            .field("path", &self.path)
            .field("offset", &self.offset)
            .field("messages", &self.chat.session().chat.messages.len())
            .finish()
    }
}

impl TicketChat {
    /// A chat for `task`, whose agent is `agent` and runs in `folder`. `chat_folder` is the Agent-Chat plugin's own
    /// folder, so the pane looks the way the side panel does.
    pub fn new(
        task: &Task,
        agent: Assignee,
        session: &str,
        folder: &Path,
        chat_folder: Option<&Path>,
    ) -> Self {
        let speaker = task.endpoint.clone().unwrap_or_else(|| agent.name().to_owned());
        // No title, so the pane draws no header: the modal's own heading says whose conversation it is.
        let title = String::new();
        let empty = format!(
            "{} has not said anything on {} yet. What it says, and the tools it runs, will be here as it works.",
            agent.name(),
            task.key
        );
        Self {
            task_id: task.id,
            session: session.to_owned(),
            folder: folder.to_path_buf(),
            chat: AgentChat::mirroring(&title, &speaker, &empty, chat_folder),
            transcript: Transcript::new(),
            path: None,
            offset: 0,
            partial: String::new(),
            looked: None,
            sent: Vec::new(),
        }
    }

    /// The transcript file, once found.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the transcript says the agent is working on a prompt.
    pub fn turn_open(&self) -> bool {
        self.transcript.turn_open()
    }

    /// The model the agent last named.
    pub fn model(&self) -> &str {
        &self.transcript.model
    }

    /// Read whatever has been added to the transcript, and say whether the agent is working. Answers whether
    /// anything changed.
    ///
    /// `running` is whether the ticket's terminal is alive: a transcript whose last turn never ended belongs to
    /// an agent that was stopped, not one that is still thinking.
    pub fn follow(&mut self, running: bool, now: std::time::Instant) -> bool {
        let due = self.looked.is_none_or(|looked| now.duration_since(looked) >= LOOK_EVERY);
        let mut changed = false;
        if due {
            self.looked = Some(now);
            changed = self.read_what_was_added();
        }
        let before = self.chat.session().is_busy();
        self.chat.mirrored().follow(running && self.transcript.turn_open());
        // A message typed into the chat stops waiting once the transcript has it.
        for prompt in self.transcript.take_new_prompts() {
            if self.chat.the_mirror_heard(&prompt) {
                // Which one was taken is not known here, and the oldest is the likeliest: lines arrive in order.
                if !self.sent.is_empty() {
                    self.sent.remove(0);
                }
            } else if self.sent.first().is_some_and(|at| now.duration_since(*at) >= HEARD_AFTER)
                && self.chat.the_mirror_heard_the_oldest()
            {
                self.sent.remove(0);
            }
            changed = true;
        }
        changed || before != self.chat.session().is_busy()
    }

    /// Record that the chat typed a message, so it can stop waiting if the transcript never echoes it exactly.
    pub fn typed(&mut self, now: std::time::Instant) {
        self.sent.push(now);
    }

    /// Find the file when it has not been found, and read what was added to it since the last read.
    fn read_what_was_added(&mut self) -> bool {
        if self.path.is_none() {
            let Some(home) = claude_home() else { return false };
            self.path = transcript::find_claude_transcript(&home, &self.folder, &self.session);
        }
        let Some(path) = self.path.clone() else { return false };
        let Ok(mut file) = std::fs::File::open(&path) else { return false };
        let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if length < self.offset {
            // Shorter than what was read: rewritten, which Claude Code does not do, so start again rather than
            // read from the middle of a line.
            self.offset = 0;
            self.partial.clear();
            self.transcript = Transcript::new();
            let session = self.chat.mirrored();
            session.chat.messages.clear();
        }
        if length == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return false;
        }
        let mut added = Vec::new();
        if file.read_to_end(&mut added).is_err() {
            return false;
        }
        self.offset += added.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&added));
        let mut any = false;
        let session = self.chat.mirrored();
        while let Some(end) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=end).collect();
            self.transcript.line(&mut session.chat, line.trim_end());
            any = true;
        }
        any
    }
}

/// The last `count` messages of a conversation as data, which is what `plugins run agent-tasks chat` answers.
pub fn messages_json(chat: &unluminous_chat::Conversation, count: usize) -> Vec<serde_json::Value> {
    let start = chat.messages.len().saturating_sub(count);
    chat.messages[start..]
        .iter()
        .filter(|message| !message.is_empty() || message.compacted.is_some())
        .map(|message| {
            serde_json::json!({
                "role": match message.role {
                    unluminous_chat::Role::User => "user",
                    unluminous_chat::Role::Assistant => "assistant",
                    unluminous_chat::Role::Tool => "tool",
                },
                "text": message.text(),
                "tools": message
                    .tools
                    .iter()
                    .map(|call| serde_json::json!({
                        "name": call.name,
                        "running": call.is_running(),
                        "failed": call.failed,
                    }))
                    .collect::<Vec<serde_json::Value>>(),
                "compacted": message.compacted,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claude_ticket_opens_on_the_chat_and_every_other_on_the_terminal() {
        assert_eq!(AgentTab::default_for(Assignee::Claude), AgentTab::Chat);
        assert_eq!(AgentTab::default_for(Assignee::Codex), AgentTab::Terminal);
        assert_eq!(AgentTab::default_for(Assignee::OpenCode), AgentTab::Terminal);
        assert_eq!(AgentTab::parse(" Terminal "), Some(AgentTab::Terminal));
        assert_eq!(AgentTab::parse("chat"), Some(AgentTab::Chat));
        assert_eq!(AgentTab::parse("both"), None);
    }
}
