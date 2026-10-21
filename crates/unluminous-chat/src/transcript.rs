//! Reading the transcript Claude Code writes as it works, into a [`Conversation`].
//!
//! `task-2261`. A ticket's agent on the Agent-Tasks board is `claude` running in a terminal, started with
//! `--session-id <uuid>`, and Claude Code writes every turn of that conversation to a file of its own, one JSON
//! object a line. The ticket modal's Chat tab is that file read into the conversation the Agent-Chat pane
//! draws, so the chat and the terminal are two views of one agent rather than two agents.
//! `tasks/task-2261-ticket-agent-chat-tdd.md` §1 has the measurements this is built on.
//!
//! Nothing here touches the file system except [`find_claude_transcript`], and nothing here starts a program.
//! [`Transcript::line`] takes one line and applies it, so the file is read a piece at a time as it grows and a
//! test feeds lines it wrote by hand.
//!
//! ## What a transcript line becomes
//!
//! A `user` line whose content is a string is something the person typed, or the handoff the board typed for
//! them. One whose content is a list holds either text, which is the same thing, or `tool_result` blocks, which
//! are the answers to the tools the agent ran. An `assistant` line holds **one** content block, and the blocks of
//! one model message share `message.id`, so the lines are gathered back into one message by that id. Lines marked
//! `isMeta` are what Claude Code expanded a slash command into, and lines marked `isSidechain` belong to a
//! sub-agent; neither is drawn. Every other kind of line (attachments, file snapshots, permission modes, costs)
//! says nothing a chat shows.
//!
//! ## Pi's session file is read the same way (`task-2302`)
//!
//! A Pi ticket is `pi --session-id <uuid>` in a terminal, and Pi writes its session to
//! `~/.pi/agent/sessions/--<folder>--/<time>_<id>.jsonl`. [`Transcript::for_pi`] reads that file. Each `message`
//! line holds one whole message, so nothing has to be gathered: `user` is a prompt, `assistant` is its text,
//! thinking and `toolCall` blocks, and `toolResult` answers the call named by its `toolCallId`. An assistant
//! message whose `stopReason` is `toolUse` keeps the turn open and any other ends it. A `compaction` line is the
//! note drawn where the context was compacted.

use crate::model::{Conversation, Message, Role, ToolCall};

/// What has been read of one transcript so far.
#[derive(Debug, Clone, Default)]
pub struct Transcript {
    /// Which message each model message id was gathered into, newest last. A message's lines arrive together,
    /// so only the last few are ever looked for; the list is kept short.
    gathered: Vec<(String, u64)>,
    /// Whether a turn is open: a prompt has arrived and nothing has said the agent finished answering it.
    turn_open: bool,
    /// The model the agent last named.
    pub model: String,
    /// How many prompts have been read, which is how the board knows a line it typed has been taken.
    pub prompts: usize,
    /// The text of each prompt read since the board last asked, so a message shown as queued can be matched
    /// with the line that recorded it. See [`Transcript::take_new_prompts`].
    new_prompts: Vec<String>,
    /// Whether this is Pi's session file rather than Claude Code's transcript (`task-2302`).
    pi: bool,
    /// When each tool call was made, in milliseconds since 1970, by its id, so its answer can say how long it
    /// took (`task-2321`). Neither file records a duration; both stamp every line with the time it was written.
    calls_started: Vec<(String, u64)>,
}

/// How many recent model message ids are remembered.
const GATHERED: usize = 16;

/// How many calls' start times are remembered. A call is answered long before this many more are made.
const CALLS_TIMED: usize = 256;

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    /// A reader for Pi's session file rather than Claude Code's transcript (`task-2302`).
    pub fn for_pi() -> Self {
        Self { pi: true, ..Self::default() }
    }

    /// Whether the agent is working on a prompt.
    pub fn turn_open(&self) -> bool {
        self.turn_open
    }

    /// The prompts read since this was last called, oldest first.
    pub fn take_new_prompts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.new_prompts)
    }

    /// Apply one line of the transcript to `chat`.
    ///
    /// A line that is not JSON is ignored, which is also what happens to the last line of a file read while
    /// Claude Code was half way through writing it: the caller keeps that part and hands it back whole.
    pub fn line(&mut self, chat: &mut Conversation, line: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return;
        };
        if self.pi {
            self.pi_line(chat, &value);
            return;
        }
        if value["isSidechain"].as_bool() == Some(true) {
            return;
        }
        match value["type"].as_str().unwrap_or_default() {
            "user" => self.user(chat, &value),
            "assistant" => self.assistant(chat, &value),
            "system" => match value["subtype"].as_str().unwrap_or_default() {
                "turn_duration" => self.turn_open = false,
                "compact_boundary" => {
                    let id = chat.next_id();
                    let mut note = Message::new(id, Role::Assistant);
                    note.compacted =
                        Some(value["compactMetadata"]["preTokens"].as_u64().unwrap_or_else(|| {
                            value["compact_metadata"]["pre_tokens"].as_u64().unwrap_or(0)
                        }));
                    chat.push(note);
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// A `user` line: a prompt, or the answers to tools.
    fn user(&mut self, chat: &mut Conversation, value: &serde_json::Value) {
        let content = &value["message"]["content"];
        let meta = value["isMeta"].as_bool() == Some(true);
        let mut said = String::new();
        match content {
            serde_json::Value::String(text) => said.push_str(text),
            serde_json::Value::Array(blocks) => {
                for block in blocks {
                    match block["type"].as_str().unwrap_or_default() {
                        "tool_result" => self.answer(
                            chat,
                            block["tool_use_id"].as_str().unwrap_or_default(),
                            crate::agent::text_of(&block["content"]),
                            block["is_error"].as_bool().unwrap_or(false),
                            written_at(value),
                        ),
                        "text" => {
                            if !said.is_empty() {
                                said.push_str("\n\n");
                            }
                            said.push_str(block["text"].as_str().unwrap_or_default());
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if meta || said.trim().is_empty() {
            return;
        }
        // **An interruption ends the turn and is not something anybody said.** Claude Code records the Escape
        // that stopped it as a user line of its own.
        if said.trim_start().starts_with("[Request interrupted by user") {
            self.turn_open = false;
            return;
        }
        // The output of a local command (`/model`, `/clear`) is Claude Code talking to the person, not the agent.
        if said.trim_start().starts_with("<local-command-") {
            return;
        }
        // **Something Claude Code woke the agent with**, such as a background command finishing, is a turn
        // nobody typed. It opens the turn, because the agent is working again, and it is not drawn as words the
        // person said. A line with no `origin` at all is from a Claude Code older than the field, and is drawn.
        if value["origin"]["kind"].as_str().is_some_and(|kind| kind != "human") {
            self.turn_open = true;
            return;
        }
        let said = slash_command(&said).unwrap_or_else(|| unwrap_a_paste(&said));
        let id = chat.next_id();
        chat.push(Message::said(id, Role::User, said.clone()));
        self.turn_open = true;
        self.prompts += 1;
        self.new_prompts.push(said);
    }

    /// An `assistant` line: one block of a model message.
    fn assistant(&mut self, chat: &mut Conversation, value: &serde_json::Value) {
        let message = &value["message"];
        if let Some(model) = message["model"].as_str().filter(|model| !model.starts_with('<')) {
            self.model = model.to_owned();
        }
        let usage = &message["usage"];
        let used = usage["input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_read_input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
        if used > 0 {
            chat.context_used = Some(used);
        }
        let api_id = message["id"].as_str().unwrap_or_default().to_owned();
        let known = self
            .gathered
            .iter()
            .rev()
            .find(|(id, _)| !api_id.is_empty() && *id == api_id)
            .map(|(_, ours)| *ours)
            .filter(|ours| chat.message(*ours).is_some());
        let ours = match known {
            Some(ours) => ours,
            None => {
                let ours = chat.next_id();
                chat.push(Message::new(ours, Role::Assistant));
                self.gathered.push((api_id, ours));
                if self.gathered.len() > GATHERED {
                    self.gathered.remove(0);
                }
                ours
            }
        };
        let Some(into) = chat.messages.iter_mut().rev().find(|message| message.id == ours) else {
            return;
        };
        let at = written_at(value);
        for block in message["content"].as_array().into_iter().flatten() {
            match block["type"].as_str().unwrap_or_default() {
                "text" => {
                    let text = block["text"].as_str().unwrap_or_default();
                    if !into.text().is_empty() && !text.is_empty() {
                        into.push_text("\n\n");
                    }
                    into.push_text(text);
                }
                "thinking" => {
                    into.thinking.push_str(block["thinking"].as_str().unwrap_or_default())
                }
                "tool_use" => {
                    let id = block["id"].as_str().unwrap_or_default();
                    note_the_start(&mut self.calls_started, id, at);
                    into.tools.push(ToolCall::new(
                        id,
                        block["name"].as_str().unwrap_or_default(),
                        block["input"].to_string(),
                    ))
                }
                _ => {}
            }
        }
        match message["stop_reason"].as_str() {
            Some("end_turn" | "stop_sequence" | "max_tokens" | "refusal") => {
                into.finish = Some("stop".to_owned());
                self.turn_open = false;
            }
            Some("tool_use") => self.turn_open = true,
            _ => {}
        }
    }
}

impl Transcript {
    /// One line of Pi's session file.
    fn pi_line(&mut self, chat: &mut Conversation, value: &serde_json::Value) {
        match value["type"].as_str().unwrap_or_default() {
            "message" => {}
            "compaction" => {
                let id = chat.next_id();
                let mut note = Message::new(id, Role::Assistant);
                note.compacted = Some(value["tokensBefore"].as_u64().unwrap_or(0));
                chat.push(note);
                return;
            }
            _ => return,
        }
        let message = &value["message"];
        match message["role"].as_str().unwrap_or_default() {
            "user" => {
                let said = match &message["content"] {
                    serde_json::Value::String(text) => text.clone(),
                    blocks => crate::agent::text_of(blocks),
                };
                if said.trim().is_empty() {
                    return;
                }
                let id = chat.next_id();
                chat.push(Message::said(id, Role::User, said.clone()));
                self.turn_open = true;
                self.prompts += 1;
                self.new_prompts.push(said);
            }
            "assistant" => self.pi_assistant(chat, message),
            "toolResult" => self.answer(
                chat,
                message["toolCallId"].as_str().unwrap_or_default(),
                crate::agent::text_of(&message["content"]),
                message["isError"].as_bool().unwrap_or(false),
                pi_written_at(value),
            ),
            _ => {}
        }
    }

    /// A Pi assistant message, which is one whole model message on one line.
    fn pi_assistant(&mut self, chat: &mut Conversation, message: &serde_json::Value) {
        if let Some(model) = message["model"].as_str().filter(|model| !model.is_empty()) {
            self.model = model.to_owned();
        }
        let usage = &message["usage"];
        let used = usage["input"].as_u64().unwrap_or(0)
            + usage["cacheRead"].as_u64().unwrap_or(0)
            + usage["cacheWrite"].as_u64().unwrap_or(0);
        if used > 0 {
            chat.context_used = Some(used);
        }
        let id = chat.next_id();
        let mut into = Message::new(id, Role::Assistant);
        let at = message["timestamp"].as_u64().or_else(|| iso_milliseconds(&message["timestamp"]));
        for block in message["content"].as_array().into_iter().flatten() {
            match block["type"].as_str().unwrap_or_default() {
                "text" => {
                    let text = block["text"].as_str().unwrap_or_default();
                    if !into.text().is_empty() && !text.is_empty() {
                        into.push_text("\n\n");
                    }
                    into.push_text(text);
                }
                "thinking" => {
                    into.thinking.push_str(block["thinking"].as_str().unwrap_or_default())
                }
                "toolCall" => {
                    let id = block["id"].as_str().unwrap_or_default();
                    note_the_start(&mut self.calls_started, id, at);
                    into.tools.push(ToolCall::new(
                        id,
                        block["name"].as_str().unwrap_or_default(),
                        block["arguments"].to_string(),
                    ))
                }
                _ => {}
            }
        }
        match message["stopReason"].as_str().unwrap_or_default() {
            "toolUse" => self.turn_open = true,
            "error" | "aborted" => {
                let said = message["errorMessage"].as_str().unwrap_or_default();
                if !said.is_empty() && into.text().is_empty() {
                    into.push_text(said);
                }
                into.finish = Some("stop".to_owned());
                self.turn_open = false;
            }
            _ => {
                into.finish = Some("stop".to_owned());
                self.turn_open = false;
            }
        }
        chat.push(into);
    }
}

impl Transcript {
    /// File `answer` under the newest call with `id` that has not been answered, with how long it took when
    /// both its call and its answer say when they were written.
    fn answer(
        &mut self,
        chat: &mut Conversation,
        id: &str,
        answer: String,
        failed: bool,
        at: Option<u64>,
    ) {
        let started = self.calls_started.iter().rposition(|(called, _)| called == id);
        let started = started.map(|position| self.calls_started.remove(position).1);
        let call = chat.messages.iter_mut().rev().find_map(|message| {
            message.tools.iter_mut().rev().find(|call| call.id == id && call.is_running())
        });
        if let Some(call) = call {
            call.answer = Some(answer);
            call.failed = failed;
            if let (Some(started), Some(at)) = (started, at) {
                call.took = Some(at.saturating_sub(started));
            }
        }
    }
}

/// Remember when the call `id` was made, when the line says.
fn note_the_start(started: &mut Vec<(String, u64)>, id: &str, at: Option<u64>) {
    let Some(at) = at else { return };
    started.push((id.to_owned(), at));
    if started.len() > CALLS_TIMED {
        started.remove(0);
    }
}

/// When a Claude Code transcript line was written: its `timestamp`, in milliseconds since 1970.
fn written_at(line: &serde_json::Value) -> Option<u64> {
    iso_milliseconds(&line["timestamp"])
}

/// When a Pi session line was written: its message's `timestamp`, which Pi writes in milliseconds, or the
/// line's own.
fn pi_written_at(line: &serde_json::Value) -> Option<u64> {
    line["message"]["timestamp"].as_u64().or_else(|| iso_milliseconds(&line["timestamp"]))
}

/// A UTC time written the way both agents write one, `2026-10-19T10:47:49.708Z`, in milliseconds since 1970.
///
/// Only that form: a time with an offset or without the `Z` answers `None`, and the call is then left with no
/// duration rather than a wrong one.
pub fn iso_milliseconds(value: &serde_json::Value) -> Option<u64> {
    let text = value.as_str()?.strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.splitn(3, ':').map(str::parse::<i64>);
    let (hour, minute, second) = (clock.next()?.ok()?, clock.next()?.ok()?, clock.next()?.ok()?);
    let fraction: String = fraction.chars().chain("000".chars()).take(3).collect();
    let milliseconds: i64 = fraction.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from 1970-01-01 to the date, by Howard Hinnant's days_from_civil.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_era_days - 719_468;
    let total = ((days * 24 + hour) * 60 + minute) * 60 + second;
    u64::try_from(total * 1000 + milliseconds).ok()
}

/// A line typed in one burst as Claude Code records it: inside `<pasted_content id="..">` and its closing tag,
/// which is how a paste is kept. What was pasted is what the person (or the board) said, so the tags come off.
fn unwrap_a_paste(said: &str) -> String {
    let trimmed = said.trim();
    let Some(rest) = trimmed.strip_prefix("<pasted_content") else {
        return said.to_owned();
    };
    let Some(open_end) = rest.find('>') else {
        return said.to_owned();
    };
    let inner = &rest[open_end + 1..];
    match inner.rfind("</pasted_content") {
        Some(close) => inner[..close].trim().to_owned(),
        None => said.to_owned(),
    }
}

/// A slash command as Claude Code records it, written back the way it was typed: `/task begin task-2`.
fn slash_command(said: &str) -> Option<String> {
    let between = |tag: &str| {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = said.find(&open)? + open.len();
        let end = said[start..].find(&close)? + start;
        Some(said[start..end].trim().to_owned())
    };
    let name = between("command-name")?;
    let arguments = between("command-args").unwrap_or_default();
    let name = match name.starts_with('/') {
        true => name,
        false => format!("/{name}"),
    };
    Some(match arguments.is_empty() {
        true => name,
        false => format!("{name} {arguments}"),
    })
}

/// The name of the folder Claude Code keeps a project's transcripts in: the project's path with every character
/// that is not an ASCII letter or digit written as `-`.
///
/// `C:\jason\dev\unluminous` is `C--jason-dev-unluminous`, measured against the folders on this machine.
pub fn claude_project_folder(project: &std::path::Path) -> String {
    project
        .to_string_lossy()
        .chars()
        .map(|character| match character.is_ascii_alphanumeric() {
            true => character,
            false => '-',
        })
        .collect()
}

/// Where the transcript of `session`, started in `project`, is, or `None` when it is not on the disk.
///
/// `home` is Claude Code's own folder, `~/.claude`. The folder named by [`claude_project_folder`] is tried first;
/// when the file is not there every folder under `projects` is looked in, because Claude Code shortens a very
/// long name with a hash and the rule for that is not written down anywhere.
pub fn find_claude_transcript(
    home: &std::path::Path,
    project: &std::path::Path,
    session: &str,
) -> Option<std::path::PathBuf> {
    if session.trim().is_empty() || session.contains(['/', '\\', '.']) {
        return None;
    }
    let file = format!("{session}.jsonl");
    let projects = home.join("projects");
    let first = projects.join(claude_project_folder(project)).join(&file);
    if first.is_file() {
        return Some(first);
    }
    std::fs::read_dir(&projects)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(&file))
        .find(|candidate| candidate.is_file())
}

/// The name of the folder Pi keeps a project's sessions in: the path with its leading separator taken off,
/// every `/`, `\` and `:` written as `-`, and `--` on each side (`task-2302`).
///
/// `C:\jason\dev\unluminous` is `--C--jason-dev-unluminous--`, measured on Pi 1.3.0 on this machine.
pub fn pi_project_folder(project: &std::path::Path) -> String {
    let path = project.to_string_lossy();
    let path = path.trim_start_matches(['/', '\\']);
    let named: String = path
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other => other,
        })
        .collect();
    format!("--{named}--")
}

/// Where Pi's session file for `session`, started in `project`, is, or `None` when it is not on the disk.
///
/// `home` is Pi's own folder, `~/.pi/agent`. A file is named `<time>_<id>.jsonl`, so the one whose name ends with
/// the id is taken. The folder named by [`pi_project_folder`] is looked in first and then every other one, the
/// way [`find_claude_transcript`] does.
pub fn find_pi_transcript(
    home: &std::path::Path,
    project: &std::path::Path,
    session: &str,
) -> Option<std::path::PathBuf> {
    if session.trim().is_empty() || session.contains(['/', '\\', '.']) {
        return None;
    }
    let ending = format!("_{session}.jsonl");
    let in_folder = |folder: &std::path::Path| {
        std::fs::read_dir(folder).ok()?.flatten().map(|entry| entry.path()).find(|path| {
            path.is_file()
                && path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(&ending))
        })
    };
    let sessions = home.join("sessions");
    if let Some(found) = in_folder(&sessions.join(pi_project_folder(project))) {
        return Some(found);
    }
    std::fs::read_dir(&sessions)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .find_map(|entry| in_folder(&entry.path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines in the shape a real transcript has them, cut down to what is read.
    const LINES: &[&str] = &[
        r#"{"type":"queue-operation","operation":"enqueue","content":"ignored"}"#,
        r#"{"type":"user","message":{"role":"user","content":"<command-message>task</command-message>\n<command-name>/task</command-name>\n<command-args>begin task-2</command-args>"}}"#,
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":[{"type":"text","text":"Base directory for this skill"}]}}"#,
        r#"{"type":"assistant","message":{"model":"claude-opus-5-5","id":"msg_1","role":"assistant","content":[{"type":"thinking","thinking":"","signature":"x"}],"stop_reason":null,"usage":{"input_tokens":2,"cache_read_input_tokens":70000}}}"#,
        r#"{"type":"assistant","message":{"model":"claude-opus-5-5","id":"msg_1","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"git status"}}],"stop_reason":"tool_use"}}"#,
        r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"On branch main","is_error":false}]}}"#,
        r#"{"type":"attachment","attachment":{"type":"todo"}}"#,
        r#"{"type":"assistant","message":{"model":"claude-opus-5-5","id":"msg_2","role":"assistant","content":[{"type":"tool_use","id":"toolu_2","name":"Read","input":{"file_path":"missing.rs"}}],"stop_reason":"tool_use"}}"#,
        r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_2","type":"tool_result","content":[{"type":"text","text":"File does not exist."}],"is_error":true}]}}"#,
        r#"{"type":"assistant","isSidechain":true,"message":{"id":"msg_side","role":"assistant","content":[{"type":"text","text":"a sub-agent's words"}]}}"#,
        r#"{"type":"assistant","message":{"model":"claude-opus-5-5","id":"msg_3","role":"assistant","content":[{"type":"text","text":"The branch is clean."}],"stop_reason":"end_turn"}}"#,
        r#"{"type":"system","subtype":"turn_duration","durationMs":12000}"#,
    ];

    fn read(lines: &[&str]) -> (Conversation, Transcript) {
        let mut chat = Conversation::new("ticket", "claude");
        let mut transcript = Transcript::new();
        for line in lines {
            transcript.line(&mut chat, line);
        }
        (chat, transcript)
    }

    #[test]
    fn a_turn_is_read_into_the_messages_the_pane_draws() {
        let (chat, transcript) = read(LINES);
        let roles: Vec<Role> = chat.messages.iter().map(|message| message.role).collect();
        assert_eq!(roles, [Role::User, Role::Assistant, Role::Assistant, Role::Assistant]);
        assert_eq!(
            chat.messages[0].text(),
            "/task begin task-2",
            "a slash command as it was typed"
        );
        // The thinking line and the tool line share `msg_1`, so they are one message.
        assert_eq!(chat.messages[1].tools.len(), 1);
        let first = &chat.messages[1].tools[0];
        assert_eq!(first.name, "Bash");
        assert_eq!(first.arguments, r#"{"command":"git status"}"#);
        assert_eq!(first.answer.as_deref(), Some("On branch main"));
        assert!(!first.failed);
        let second = &chat.messages[2].tools[0];
        assert_eq!(second.answer.as_deref(), Some("File does not exist."));
        assert!(second.failed, "an error result is drawn as one");
        assert_eq!(chat.messages[3].text(), "The branch is clean.");
        assert!(!chat.messages.iter().any(|message| message.text().contains("sub-agent")));
        assert!(!chat.messages.iter().any(|message| message.text().contains("Base directory")));
        assert_eq!(transcript.model, "claude-opus-5-5");
        assert_eq!(transcript.prompts, 1);
        assert_eq!(chat.context_used, Some(70002));
        assert!(!transcript.turn_open(), "the turn ended");
    }

    #[test]
    fn a_turn_is_open_from_its_prompt_until_something_says_it_ended() {
        let (_, transcript) = read(&LINES[..2]);
        assert!(transcript.turn_open(), "a prompt opens a turn");
        let (_, transcript) = read(&LINES[..8]);
        assert!(transcript.turn_open(), "a tool call keeps it open");
        let mut lines = LINES[..8].to_vec();
        lines.push(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#);
        let (chat, transcript) = read(&lines);
        assert!(!transcript.turn_open(), "an interruption ends it");
        assert!(!chat.messages.iter().any(|message| message.text().contains("interrupted")));
    }

    #[test]
    fn the_file_read_a_line_at_a_time_or_whole_gives_one_conversation() {
        let (whole, _) = read(LINES);
        let mut chat = Conversation::new("ticket", "claude");
        let mut transcript = Transcript::new();
        // Split where a reader following a growing file would split it: part of a line held back.
        let text = LINES.join("\n");
        let mut held = String::new();
        for piece in text.as_bytes().chunks(37) {
            held.push_str(std::str::from_utf8(piece).expect("ascii"));
            while let Some(end) = held.find('\n') {
                let line: String = held.drain(..=end).collect();
                transcript.line(&mut chat, line.trim_end());
            }
        }
        transcript.line(&mut chat, &held);
        assert_eq!(chat.messages, whole.messages);
    }

    #[test]
    fn a_prompt_typed_later_is_reported_once() {
        let mut chat = Conversation::new("ticket", "claude");
        let mut transcript = Transcript::new();
        for line in LINES {
            transcript.line(&mut chat, line);
        }
        let _ = transcript.take_new_prompts();
        transcript.line(
            &mut chat,
            r#"{"type":"user","message":{"role":"user","content":"and push it"}}"#,
        );
        assert_eq!(transcript.take_new_prompts(), ["and push it"]);
        assert!(transcript.take_new_prompts().is_empty());
        assert!(transcript.turn_open());
    }

    #[test]
    fn a_turn_claude_code_started_itself_is_not_drawn_as_the_persons_words() {
        let mut lines = LINES.to_vec();
        lines.push(r#"{"type":"user","origin":{"kind":"task-notification"},"message":{"role":"user","content":"<task-notification>done</task-notification>"}}"#);
        let (chat, transcript) = read(&lines);
        assert!(!chat.messages.iter().any(|message| message.text().contains("task-notification")));
        assert!(transcript.turn_open(), "the agent is working again");
        assert_eq!(transcript.prompts, 1);
        lines.push(r#"{"type":"user","origin":{"kind":"human"},"message":{"role":"user","content":"thanks"}}"#);
        let (chat, _) = read(&lines);
        assert_eq!(chat.messages.last().map(|message| message.text()).as_deref(), Some("thanks"));
    }

    #[test]
    fn a_pasted_line_is_read_without_the_tags_claude_code_keeps_it_in() {
        let mut lines = LINES.to_vec();
        lines.push(r#"{"type":"user","message":{"role":"user","content":"\n\n<pasted_content id=\"8e7d\">\nYou are working task-2.\n</pasted_content id=\"8e7d\">\n"}}"#);
        let (chat, mut transcript) = read(&lines);
        assert_eq!(
            chat.messages.last().map(|message| message.text()).as_deref(),
            Some("You are working task-2.")
        );
        assert_eq!(
            transcript.take_new_prompts().last().map(String::as_str),
            Some("You are working task-2.")
        );
    }

    #[test]
    fn a_compaction_is_a_note_between_the_messages() {
        let mut lines = LINES.to_vec();
        lines.push(r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"auto","preTokens":180000}}"#);
        let (chat, _) = read(&lines);
        assert_eq!(chat.messages.last().and_then(|message| message.compacted), Some(180000));
    }

    #[test]
    fn the_folder_name_is_the_path_with_everything_but_letters_and_digits_as_dashes() {
        assert_eq!(
            claude_project_folder(std::path::Path::new(
                r"C:\jason\dev\unluminous\.claude\worktrees\task-2260"
            )),
            "C--jason-dev-unluminous--claude-worktrees-task-2260"
        );
        assert_eq!(
            claude_project_folder(std::path::Path::new("/Users/jason/dev/x")),
            "-Users-jason-dev-x"
        );
    }

    #[test]
    fn a_transcript_is_found_where_claude_code_keeps_it_or_anywhere_under_projects() {
        let home =
            std::env::temp_dir().join(format!("unluminous-transcript-{}", std::process::id()));
        let project = std::path::Path::new("/work/project");
        let named = home.join("projects").join(claude_project_folder(project));
        std::fs::create_dir_all(&named).expect("folder");
        std::fs::write(named.join("abc.jsonl"), "{}\n").expect("file");
        let elsewhere = home.join("projects").join("shortened-123");
        std::fs::create_dir_all(&elsewhere).expect("folder");
        std::fs::write(elsewhere.join("def.jsonl"), "{}\n").expect("file");
        assert_eq!(find_claude_transcript(&home, project, "abc"), Some(named.join("abc.jsonl")));
        assert_eq!(
            find_claude_transcript(&home, project, "def"),
            Some(elsewhere.join("def.jsonl"))
        );
        assert_eq!(find_claude_transcript(&home, project, "none"), None);
        assert_eq!(find_claude_transcript(&home, project, "../abc"), None, "an id is not a path");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `task-2302`: Pi's session file, in the shape Pi 1.3.0 wrote it, read into the same conversation.
    #[test]
    fn a_pi_session_file_is_read_into_the_conversation() {
        let lines = [
            r#"{"type":"session","version":3,"id":"7f3c2a10","timestamp":"2026-10-19T12:20:53.309Z","cwd":"C:/work"}"#,
            r#"{"type":"model_change","id":"9707f601","parentId":null,"provider":"local-qwen","modelId":"qwen"}"#,
            r#"{"type":"message","id":"6904e543","message":{"role":"system","content":"","sections":{"preamble":"You are"}}}"#,
            r#"{"type":"message","id":"a0f84348","message":{"role":"user","content":[{"type":"text","text":"What is in note.txt?"}]}}"#,
            r#"{"type":"message","id":"73461ca8","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Read it."},{"type":"toolCall","id":"call_1","name":"read","arguments":{"path":"note.txt"}}],"model":"qwen","usage":{"input":8000,"cacheRead":230,"cacheWrite":0,"output":20},"stopReason":"toolUse"}}"#,
            r#"{"type":"message","id":"b1","message":{"role":"toolResult","toolCallId":"call_1","toolName":"read","content":[{"type":"text","text":"The answer is 42."}],"isError":false}}"#,
        ];
        let mut chat = Conversation::new("ticket", "pi");
        let mut transcript = Transcript::for_pi();
        for line in lines {
            transcript.line(&mut chat, line);
        }
        assert!(transcript.turn_open(), "a tool call keeps the turn open");
        assert_eq!(transcript.prompts, 1);
        assert_eq!(transcript.take_new_prompts(), ["What is in note.txt?"]);
        assert_eq!(transcript.model, "qwen");
        assert_eq!(chat.context_used, Some(8230));
        let call = &chat.messages[1].tools[0];
        assert_eq!(call.name, "read");
        assert_eq!(call.answer.as_deref(), Some("The answer is 42."));
        transcript.line(
            &mut chat,
            r#"{"type":"message","id":"c1","message":{"role":"assistant","content":[{"type":"text","text":"It says 42."}],"model":"qwen","stopReason":"stop"}}"#,
        );
        assert!(!transcript.turn_open(), "an answer that stops ends the turn");
        assert_eq!(
            chat.messages.last().map(|message| message.text()),
            Some("It says 42.".to_owned())
        );
        transcript.line(&mut chat, r#"{"type":"compaction","id":"d1","tokensBefore":50000}"#);
        assert_eq!(chat.messages.last().and_then(|message| message.compacted), Some(50000));
    }

    #[test]
    fn a_pi_session_is_found_by_the_id_at_the_end_of_its_name() {
        let home = std::env::temp_dir().join(format!("unluminous-pi-home-{}", std::process::id()));
        let project = std::path::Path::new(r"C:\work\project");
        assert_eq!(pi_project_folder(project), "--C--work-project--");
        assert_eq!(pi_project_folder(std::path::Path::new("/home/me/code")), "--home-me-code--");
        let folder = home.join("sessions").join(pi_project_folder(project));
        std::fs::create_dir_all(&folder).expect("a folder");
        let file = folder.join("2026-10-19T12-20-53-309Z_7f3c2a10-5b6e.jsonl");
        std::fs::write(&file, "{}\n").expect("a file");
        assert_eq!(find_pi_transcript(&home, project, "7f3c2a10-5b6e"), Some(file.clone()));
        // Found from another folder too, because the folder name is Pi's rule and not Unluminous's.
        assert_eq!(
            find_pi_transcript(&home, std::path::Path::new("D:/elsewhere"), "7f3c2a10-5b6e"),
            Some(file)
        );
        assert_eq!(
            find_pi_transcript(&home, project, "5b6e"),
            None,
            "the whole id, not a piece of it"
        );
        assert_eq!(find_pi_transcript(&home, project, "../x"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `task-2321`: a call's time is the gap between the line that made it and the line that answered it, so a
    /// run of commands no longer reads `0.0s`.
    #[test]
    fn a_tool_call_takes_the_time_between_its_call_and_its_answer() {
        let (chat, _) = read(&[
            r#"{"type":"user","message":{"role":"user","content":"Look"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-19T10:47:49.708Z","message":{"id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo build"}}],"stop_reason":"tool_use"}}"#,
            r#"{"type":"user","timestamp":"2026-10-19T10:48:02.208Z","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"ok","is_error":false}]}}"#,
            r#"{"type":"assistant","message":{"id":"m2","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}],"stop_reason":"tool_use"}}"#,
            r#"{"type":"user","timestamp":"2026-10-19T10:48:03.000Z","message":{"role":"user","content":[{"tool_use_id":"t2","type":"tool_result","content":"ok","is_error":false}]}}"#,
        ]);
        let calls: Vec<&ToolCall> =
            chat.messages.iter().flat_map(|message| &message.tools).collect();
        assert_eq!(calls[0].took, Some(12_500));
        assert_eq!(calls[1].took, None, "a call whose line has no time is left with no time");

        let mut chat = Conversation::new("ticket", "pi");
        let mut transcript = Transcript::for_pi();
        transcript.line(
            &mut chat,
            r#"{"type":"message","message":{"role":"assistant","timestamp":1792412639639,"stopReason":"toolUse","content":[{"type":"toolCall","id":"c1","name":"bash","arguments":{"command":"ls"}}]}}"#,
        );
        transcript.line(
            &mut chat,
            r#"{"type":"message","message":{"role":"toolResult","toolCallId":"c1","timestamp":1792412640039,"isError":true,"content":"no"}}"#,
        );
        assert_eq!(chat.messages[0].tools[0].took, Some(400));
    }

    #[test]
    fn a_utc_time_is_read_to_the_millisecond() {
        let time = |text: &str| iso_milliseconds(&serde_json::Value::from(text));
        assert_eq!(time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(time("2000-03-01T00:00:00.5Z"), Some(951_868_800_500));
        assert_eq!(time("2026-10-19T12:23:59.489Z"), Some(1_792_412_639_489));
        assert_eq!(time("2026-10-19T12:23:59+01:00"), None);
        assert_eq!(time("yesterday"), None);
    }
}
