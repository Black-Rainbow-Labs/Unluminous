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
//!
//! **The first read is on a thread of its own** (`task-2269`: *"In agent tasks, when the agent chat has a long
//! history, it takes a while for the modal to open."*). A ticket that has run for a day has a transcript of tens of
//! megabytes, and the largest on this machine is 116 MB. Reading and parsing all of it stopped the window's thread
//! for as long as that took, on the frame that opened the modal. So the whole file is read once on a thread into a
//! conversation of its own, handed back and swapped in, and only what is added after that is read on the window's
//! thread, which is a few lines at a time.

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

/// The variable a test sets to name Pi's own folder, so it never reads the person's `~/.pi/agent` (`task-2302`).
pub const PI_HOME_VARIABLE: &str = "UNLUMINOUS_PI_HOME";

/// Pi's own folder: [`PI_HOME_VARIABLE`], then `PI_CODING_AGENT_DIR`, then `~/.pi/agent`.
pub fn pi_home() -> Option<PathBuf> {
    for variable in [PI_HOME_VARIABLE, "PI_CODING_AGENT_DIR"] {
        if let Some(named) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(named));
        }
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".pi").join("agent"))
}

/// Where the conversation of `session` is on the disk: Claude Code's transcript, or Pi's session file when `pi`.
fn find_the_transcript(pi: bool, folder: &Path, session: &str) -> Option<PathBuf> {
    match pi {
        true => pi_home().and_then(|home| transcript::find_pi_transcript(&home, folder, session)),
        false => claude_home()
            .and_then(|home| transcript::find_claude_transcript(&home, folder, session)),
    }
}

/// A reader for the agent's own file.
fn a_reader(pi: bool) -> Transcript {
    match pi {
        true => Transcript::for_pi(),
        false => Transcript::new(),
    }
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
/// Claude Code and Pi write their conversation under the session id the ticket chose, so the file can be found
/// (Pi since `task-2302`). Codex and OpenCode name their own sessions, so the id on the ticket names nothing they
/// wrote: the reason `agent::can_resume` gives, about the same fact.
pub fn chat_is_readable(agent: Assignee) -> bool {
    matches!(agent, Assignee::Claude | Assignee::Pi)
}

/// What the Chat tab says for an agent whose conversation it cannot read.
pub fn why_the_chat_is_empty(agent: Assignee) -> String {
    format!(
        "{} keeps its conversation where Unluminous does not read it, so its work is in the Terminal tab.",
        agent.name()
    )
}

/// What the first read of a transcript came to, handed back from its thread.
struct Loaded {
    path: Option<PathBuf>,
    offset: u64,
    partial: String,
    transcript: Transcript,
    chat: unluminous_chat::Conversation,
}

/// What the Chat tab says while the first read is still going.
const READING: &str = "Reading the conversation\u{2026}";

/// Find `session`'s transcript and read the whole of it into `chat`.
///
/// The one reading of a whole file there is, used on the thread [`TicketChat::follow`] starts and directly by
/// [`TicketChat::read_now`]. A line not yet finished is kept as `partial`, exactly as a later read keeps one.
fn read_the_whole_transcript(
    pi: bool,
    folder: &Path,
    session: &str,
    mut chat: unluminous_chat::Conversation,
) -> Loaded {
    let path = find_the_transcript(pi, folder, session);
    let mut transcript = a_reader(pi);
    let mut offset = 0;
    let mut partial = String::new();
    if let Some(bytes) = path.as_ref().and_then(|path| std::fs::read(path).ok()) {
        offset = bytes.len() as u64;
        let text = String::from_utf8_lossy(&bytes);
        let finished = text.rfind('\n').map(|end| end + 1).unwrap_or(0);
        for line in text[..finished].lines() {
            transcript.line(&mut chat, line.trim_end());
        }
        partial = text[finished..].to_owned();
    }
    Loaded { path, offset, partial, transcript, chat }
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
    /// Whether the agent is Pi, whose session file is read rather than Claude Code's transcript (`task-2302`).
    pi: bool,
    /// The file, once it has been found.
    path: Option<PathBuf>,
    /// How much of it has been read, and the part of a line after that.
    offset: u64,
    partial: String,
    /// When the file was last looked at.
    looked: Option<std::time::Instant>,
    /// When each message still shown as waiting was typed, oldest first, beside `AgentChat::queued`.
    sent: Vec<std::time::Instant>,
    /// The first read, while it is running on its thread.
    loading: Option<std::sync::mpsc::Receiver<Loaded>>,
    /// Whether the first read has been taken in, after which the file is read a few lines at a time.
    read_once: bool,
    /// What the empty conversation says once the first read is in.
    empty: String,
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
        Self::reading(task.id, &task.key, agent.name(), &speaker, session, folder, chat_folder)
    }

    /// The same chat, named by its parts rather than by a ticket, which is how the Agent-Chat side pane makes one
    /// for `To Side Pane` (`task-2269`): that pane has no board to read the ticket from.
    #[allow(clippy::too_many_arguments)]
    pub fn reading(
        task_id: i64,
        key: &str,
        agent: &str,
        speaker: &str,
        session: &str,
        folder: &Path,
        chat_folder: Option<&Path>,
    ) -> Self {
        // No title, so the pane draws no header: whatever shows it says whose conversation it is.
        let title = String::new();
        let empty = format!(
            "{agent} has not said anything on {key} yet. What it says, and the tools it runs, will be here as it works."
        );
        Self {
            task_id,
            session: session.to_owned(),
            folder: folder.to_path_buf(),
            chat: AgentChat::mirroring(&title, speaker, READING, chat_folder),
            transcript: a_reader(agent == Assignee::Pi.name()),
            pi: agent == Assignee::Pi.name(),
            path: None,
            offset: 0,
            partial: String::new(),
            looked: None,
            sent: Vec::new(),
            loading: None,
            read_once: false,
            empty,
        }
    }

    /// Whether the first read of the transcript is still going.
    pub fn is_reading(&self) -> bool {
        !self.read_once
    }

    /// Read the whole transcript now, on this thread, when it has not been read yet. For an answer that has to be
    /// complete when it is given, such as `plugins run agent-tasks chat`.
    pub fn read_now(&mut self) {
        if self.read_once {
            return;
        }
        // A read already running on its thread is waited for rather than done twice.
        let loaded = self.loading.take().and_then(|receiver| receiver.recv().ok());
        let loaded = loaded.unwrap_or_else(|| {
            read_the_whole_transcript(
                self.pi,
                &self.folder,
                &self.session,
                self.chat.session().chat.clone(),
            )
        });
        self.take_in(loaded);
    }

    /// Start the first read on a thread, or take in what it came to. Answers whether it was taken in.
    fn read_the_first_time(&mut self) -> bool {
        let Some(receiver) = &self.loading else {
            let (send, receive) = std::sync::mpsc::channel();
            let folder = self.folder.clone();
            let session = self.session.clone();
            let chat = self.chat.session().chat.clone();
            let pi = self.pi;
            let started = std::thread::Builder::new()
                .name("unluminous-ticket-transcript".to_owned())
                .spawn(move || {
                    let _ = send.send(read_the_whole_transcript(pi, &folder, &session, chat));
                });
            match started {
                Ok(_) => self.loading = Some(receive),
                // No thread to be had: read it here, which is what happened before there was one.
                Err(_) => self.read_now(),
            }
            return self.read_once;
        };
        match receiver.try_recv() {
            Ok(loaded) => {
                self.loading = None;
                self.take_in(loaded);
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // The thread ended without an answer, which only a panic in it does. The file is then read the
                // ordinary way, from its first byte.
                self.loading = None;
                self.read_once = true;
                self.chat.say_while_empty(&self.empty);
                true
            }
        }
    }

    /// Swap in what the first read came to.
    fn take_in(&mut self, loaded: Loaded) {
        self.path = loaded.path;
        self.offset = loaded.offset;
        self.partial = loaded.partial;
        self.transcript = loaded.transcript;
        self.chat.mirrored().chat = loaded.chat;
        self.read_once = true;
        self.chat.say_while_empty(&self.empty);
        // The conversation arrived all at once, so it opens at its newest message.
        self.chat.ui.jump_to_bottom = true;
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
    ///
    /// The first call starts the whole file reading on a thread, and this answers true until it is in, so the
    /// window keeps drawing and shows the conversation on the frame it arrives.
    pub fn follow(&mut self, running: bool, now: std::time::Instant) -> bool {
        if !self.read_once {
            self.read_the_first_time();
            if !self.read_once {
                return true;
            }
            // Read just now, so the next look at the file is a whole interval away.
            self.looked = Some(now);
            self.follow_the_rest(running, now);
            return true;
        }
        self.follow_the_rest(running, now)
    }

    /// [`Self::follow`] once the first read is in.
    fn follow_the_rest(&mut self, running: bool, now: std::time::Instant) -> bool {
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
            self.path = find_the_transcript(self.pi, &self.folder, &self.session);
        }
        let Some(path) = self.path.clone() else { return false };
        let Ok(mut file) = std::fs::File::open(&path) else { return false };
        let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if length < self.offset {
            // Shorter than what was read: rewritten, which Claude Code does not do, so start again rather than
            // read from the middle of a line.
            self.offset = 0;
            self.partial.clear();
            self.transcript = a_reader(self.pi);
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
        // **Every finished line is read in one pass, and what is left after the last one is kept.** Taking one line
        // at a time off the front of `partial` moved the rest of it each time, which is quadratic in what was added:
        // the whole of a 116 MB transcript read that way stopped the window for 43.7 seconds (`task-2269`).
        let Some(last) = self.partial.rfind('\n') else { return false };
        let rest = self.partial.split_off(last + 1);
        let finished = std::mem::replace(&mut self.partial, rest);
        let session = self.chat.mirrored();
        for line in finished.lines() {
            self.transcript.line(&mut session.chat, line.trim_end());
        }
        true
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

    /// One answer with its own message id, as Claude Code writes it.
    fn an_answer(number: usize) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"model":"claude-opus-5-5","id":"msg_{number}","role":"assistant","content":[{{"type":"text","text":"Answer number {number}, with enough words in it to look like an answer."}}],"stop_reason":"end_turn"}}}}"#
        )
    }

    /// Follow `chat` until its first read is in, and then once more after the look interval.
    fn read_it(chat: &mut TicketChat) {
        for _ in 0..200 {
            chat.follow(false, std::time::Instant::now());
            if !chat.is_reading() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!chat.is_reading(), "the first read came back");
    }

    /// `task-2269`: the first read is on a thread and arrives whole; what is added afterwards is read a few lines at
    /// a time, a line written in two halves is read once it is finished, and a large addition is read in one pass
    /// rather than a line at a time off the front of what was read.
    #[test]
    fn a_transcript_is_read_on_a_thread_and_then_followed_as_it_grows() {
        let root = std::env::temp_dir()
            .join(format!("unluminous-ticket-chat-unit-{}", std::process::id()));
        let home = root.join("claude");
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        let folder = home.join("projects").join(transcript::claude_project_folder(&project));
        std::fs::create_dir_all(&folder).unwrap();
        // Only this test in the library sets it, so nothing else reads a different value.
        std::env::set_var(CLAUDE_HOME_VARIABLE, &home);
        let session = "0d7a3c55-0000-4b7f-9f81-826b07131842";
        let file = folder.join(format!("{session}.jsonl"));
        let first: Vec<String> = (1..=3).map(an_answer).collect();
        std::fs::write(&file, first.join("\n") + "\n").unwrap();

        let mut chat =
            TicketChat::reading(7, "task-7", "claude", "claude", session, &project, None);
        assert!(chat.is_reading(), "nothing is read until it is followed");
        read_it(&mut chat);
        assert_eq!(chat.chat.session().chat.messages.len(), 3);

        // Half a line, then the rest of it.
        let fourth = an_answer(4);
        let (head, tail) = fourth.split_at(40);
        let append = |text: &str| {
            use std::io::Write;
            let mut out = std::fs::OpenOptions::new().append(true).open(&file).unwrap();
            out.write_all(text.as_bytes()).unwrap();
        };
        append(head);
        std::thread::sleep(LOOK_EVERY + std::time::Duration::from_millis(20));
        chat.follow(false, std::time::Instant::now());
        assert_eq!(chat.chat.session().chat.messages.len(), 3, "half a line is not read");
        append(&format!("{tail}\n"));
        std::thread::sleep(LOOK_EVERY + std::time::Duration::from_millis(20));
        chat.follow(false, std::time::Instant::now());
        assert_eq!(chat.chat.session().chat.messages.len(), 4, "the finished line is read");

        // Twenty thousand lines at once. Read a line at a time off the front, this is many seconds.
        let many: Vec<String> = (5..20_005).map(an_answer).collect();
        append(&(many.join("\n") + "\n"));
        std::thread::sleep(LOOK_EVERY + std::time::Duration::from_millis(20));
        let started = std::time::Instant::now();
        chat.follow(false, std::time::Instant::now());
        let took = started.elapsed();
        assert_eq!(chat.chat.session().chat.messages.len(), 20_004);
        assert!(took < std::time::Duration::from_secs(2), "read in one pass: {took:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_claude_ticket_opens_on_the_chat_and_every_other_on_the_terminal() {
        assert_eq!(AgentTab::default_for(Assignee::Claude), AgentTab::Chat);
        assert_eq!(AgentTab::default_for(Assignee::Codex), AgentTab::Terminal);
        assert_eq!(AgentTab::default_for(Assignee::OpenCode), AgentTab::Terminal);
        assert_eq!(AgentTab::default_for(Assignee::Pi), AgentTab::Chat);
        assert_eq!(AgentTab::parse(" Terminal "), Some(AgentTab::Terminal));
        assert_eq!(AgentTab::parse("chat"), Some(AgentTab::Chat));
        assert_eq!(AgentTab::parse("both"), None);
    }
}
