//! How a turn to a command-line agent starts: by resuming the agent's own session, or by giving a new
//! session the conversation it has not seen.
//!
//! `task-2285`: *"When unluminous restarts the agent chat conversation and messages are there, but the agent
//! isn't aware of them ... The goal is to restore conversation, but only when needed, so we don't waste
//! tokens. Ideally we'd resume session using the native resume conversation flags."*
//!
//! The native flags were already in use (`unluminous_chat::agent::command_line`). What was missing was the
//! session id on disk, and a way on when no session of this agent holds the conversation. So there are three
//! ways a turn starts, decided here with no agent behind them so a test can ask:
//!
//! - **Resume**: the conversation has a session from the agent being asked. Only the newest message goes, and
//!   the agent's own `--resume` carries the rest. This is every turn after a restart once the id is written
//!   down.
//! - **Fresh**: nothing came before this question.
//! - **Replay**: messages came before and no session of this agent holds them. That is a conversation written
//!   before sessions were kept, one carried on with a different agent, or a resume the agent refused. The
//!   earlier messages are written out once in front of the question, the agent starts a session of its own,
//!   and every turn after that is a resume. So the replay is paid once and only when nothing else works.
//!
//! The decision is a pure function of the conversation and the provider's name. Whether the agent then
//! refuses the resume is learned from its answer (`unluminous_chat::agent::refused_to_resume`), and
//! `AgentChat` asks again as a replay when it does.

use unluminous_chat::model::{Message, Part, Role};
use unluminous_chat::Conversation;

/// The most characters of earlier conversation a replay carries, about 6,000 tokens.
///
/// A replay happens at most once per conversation and agent, and only when the agent has no session. The
/// newest messages are the ones a follow up question is about, so the oldest are left out first, and the
/// block says how many.
pub const REPLAY_LIMIT: usize = 24_000;

/// How one turn starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restore {
    /// Resume this session of the agent being asked, sending only the newest message.
    Resume(String),
    /// Nothing came before the newest message.
    Fresh,
    /// Earlier messages exist and no session of this agent holds them.
    Replay,
}

impl Restore {
    /// The word `plugins run agent-chat state` reports.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Resume(_) => "resume",
            Self::Fresh => "fresh",
            Self::Replay => "replay",
        }
    }
}

/// How a turn to `provider` starts for this conversation, whose newest message is the question.
pub fn decide(chat: &Conversation, provider: &str) -> Restore {
    let session = chat.session.trim();
    // A session with no provider recorded beside it is one this window learned before `session_provider`
    // existed, in this same run, from whichever agent was being asked; it is trusted, as it always was.
    let ours = chat.session_provider.is_empty() || chat.session_provider == provider;
    if !session.is_empty() && ours {
        return Restore::Resume(session.to_owned());
    }
    match earlier(chat).next().is_some() {
        true => Restore::Replay,
        false => Restore::Fresh,
    }
}

/// The messages before the newest question that say something.
fn earlier(chat: &Conversation) -> impl Iterator<Item = &Message> {
    let newest = chat.messages.iter().rposition(|message| message.role == Role::User);
    chat.messages
        .iter()
        .take(newest.unwrap_or(chat.messages.len()))
        .filter(|message| message.compacted.is_none())
        .filter(|message| message.has_content() || !message.tools.is_empty())
}

/// The earlier conversation written out for an agent that has not seen it, to go in front of the question.
///
/// Each message is its speaker and its words. A tool call is its name, and a picture its name, because
/// neither can be sent again as text. A tool's own result message is left out for the same reason.
pub fn replay(chat: &Conversation) -> String {
    let lines: Vec<String> = earlier(chat).filter_map(one_message).collect();
    let mut kept = Vec::new();
    let mut used = 0;
    for line in lines.iter().rev() {
        if used + line.len() > REPLAY_LIMIT && !kept.is_empty() {
            break;
        }
        used += line.len();
        kept.push(line.as_str());
    }
    kept.reverse();
    let left_out = lines.len() - kept.len();
    let mut out = String::from(
        "This conversation began earlier in Unluminous's chat pane, in a session you no longer have. \
         Here is what was said, oldest first, so you can carry it on. Answer the message after it.\n\n\
         <earlier-conversation>\n",
    );
    if left_out > 0 {
        out.push_str(&format!("({left_out} older messages are left out.)\n\n"));
    }
    out.push_str(&kept.join("\n\n"));
    out.push_str("\n</earlier-conversation>");
    out
}

/// One message as a paragraph of the replay, or nothing for a message that has nothing to replay.
fn one_message(message: &Message) -> Option<String> {
    let speaker = match message.role {
        Role::User => "Person",
        Role::Assistant => "You",
        Role::Tool => return None,
    };
    let mut words = message.text().trim().to_owned();
    let pictures: Vec<&str> = message
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Picture { name, .. } => Some(name.as_str()),
            Part::Text(_) => None,
        })
        .collect();
    if !pictures.is_empty() {
        words.push_str(&format!("\n[attached: {}]", pictures.join(", ")));
    }
    if !message.tools.is_empty() {
        let names: Vec<&str> = message.tools.iter().map(|tool| tool.name.as_str()).collect();
        words.push_str(&format!("\n[ran: {}]", names.join(", ")));
    }
    let words = words.trim();
    match words.is_empty() {
        true => None,
        false => Some(format!("{speaker}: {words}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_chat::ToolCall;

    fn a_conversation(said: &[(Role, &str)]) -> Conversation {
        let mut chat = Conversation::new("c", "Claude Code");
        for (role, words) in said {
            let id = chat.next_id();
            chat.push(Message::said(id, *role, *words));
        }
        chat
    }

    #[test]
    fn a_session_from_the_agent_being_asked_is_resumed() {
        let mut chat = a_conversation(&[
            (Role::User, "Remember teal."),
            (Role::Assistant, "Done."),
            (Role::User, "What was it?"),
        ]);
        chat.session = "s-1".to_owned();
        chat.session_provider = "Claude Code".to_owned();
        assert_eq!(decide(&chat, "Claude Code"), Restore::Resume("s-1".to_owned()));
    }

    #[test]
    fn a_session_from_another_agent_is_not_handed_to_this_one() {
        let mut chat = a_conversation(&[
            (Role::User, "Remember teal."),
            (Role::Assistant, "Done."),
            (Role::User, "What was it?"),
        ]);
        chat.session = "s-1".to_owned();
        chat.session_provider = "Claude Code".to_owned();
        assert_eq!(decide(&chat, "Codex"), Restore::Replay);
    }

    #[test]
    fn the_first_question_is_fresh_and_one_with_history_but_no_session_is_a_replay() {
        assert_eq!(
            decide(&a_conversation(&[(Role::User, "Hello")]), "Claude Code"),
            Restore::Fresh
        );
        let reopened = a_conversation(&[
            (Role::User, "Remember teal."),
            (Role::Assistant, "Done."),
            (Role::User, "What was it?"),
        ]);
        assert_eq!(decide(&reopened, "Claude Code"), Restore::Replay);
    }

    #[test]
    fn the_replay_holds_what_came_before_and_not_the_question() {
        let chat = a_conversation(&[
            (Role::User, "Remember teal."),
            (Role::Assistant, "Done."),
            (Role::User, "What was it?"),
        ]);
        let said = replay(&chat);
        assert!(said.contains("Person: Remember teal."), "{said}");
        assert!(said.contains("You: Done."), "{said}");
        assert!(!said.contains("What was it?"), "the question goes after the block, once: {said}");
        assert!(!said.contains("left out"), "{said}");
    }

    #[test]
    fn the_replay_keeps_the_newest_messages_and_says_how_many_it_left_out() {
        let long = "x".repeat(REPLAY_LIMIT / 3);
        let mut said = Vec::new();
        for _ in 0..6 {
            said.push((Role::User, long.as_str()));
        }
        said.push((Role::Assistant, "the newest answer"));
        said.push((Role::User, "and now?"));
        let block = replay(&a_conversation(&said));
        assert!(block.contains("the newest answer"), "the newest is always kept");
        assert!(block.contains("older messages are left out"), "{}", &block[..400]);
        assert!(block.len() < REPLAY_LIMIT + 1_000, "{}", block.len());
    }

    #[test]
    fn tools_and_pictures_are_named_and_notes_and_tool_results_are_skipped() {
        let mut chat = Conversation::new("c", "Claude Code");
        let mut asked = Message::said(1, Role::User, "What is this?");
        asked.parts.push(Part::Picture {
            media: "image/png".to_owned(),
            bytes: vec![1, 2, 3],
            name: "shot.png".to_owned(),
        });
        chat.push(asked);
        let mut answered = Message::said(2, Role::Assistant, "A board.");
        answered.tools.push(ToolCall::new("t1", "Read", "{}"));
        chat.push(answered);
        chat.push(Message::said(3, Role::Tool, "the file's whole text"));
        let mut note = Message::new(4, Role::Assistant);
        note.compacted = Some(40_000);
        chat.push(note);
        chat.push(Message::said(5, Role::User, "And now?"));
        let block = replay(&chat);
        assert!(block.contains("[attached: shot.png]"), "{block}");
        assert!(block.contains("[ran: Read]"), "{block}");
        assert!(!block.contains("the file's whole text"), "{block}");
        assert!(!block.contains("bytes"), "{block}");
    }
}
