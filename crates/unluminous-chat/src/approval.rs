//! Asking the person before an agent does something it was not already allowed to do (`task-2392`).
//!
//! *"In agent chat we also need to allow for user interaction that allows the model to prompt to allow."* An agent
//! run for the chat pane used to have nobody to ask: Claude Code ran with `--print`, where a tool its permission
//! mode did not allow was refused, and an ACP agent's `session/request_permission` was answered from
//! `chat.permission` with no person involved. Two of the agents can ask a host instead, and this module is what
//! they ask through:
//!
//! - **Claude Code** with `--input-format stream-json --permission-prompt-tool stdio` writes a `control_request`
//!   whose `subtype` is `can_use_tool` and waits for a `control_response` on its standard input. Measured on
//!   Claude Code 2.1.312: the request names the tool, its input and the permission rules the agent suggests
//!   adding, and an answer of `{"behavior":"allow","updatedInput":...}` runs the tool.
//! - **An ACP agent** asks with `session/request_permission`, naming the options it offers, and waits for the
//!   answer to that request.
//!
//! Codex, OpenCode and Pi have no way for a host to answer from `exec` or `run`, so they are never asked; what
//! they may do is their mode, decided before the turn starts.
//!
//! A [`Question`] goes to the pane as `Reply::Asked`, the person's choice comes back through [`Answers`], and
//! the thread running the turn writes the answer to the agent and says `Reply::Answered`.

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;

/// One thing an agent asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    /// The agent's own id for the request, which the answer carries back.
    pub id: String,
    /// The tool's name, such as `Bash` or `Write`, which decides how its input is laid out.
    pub tool: String,
    /// One line saying what it is, in the agent's words when it gave any.
    pub title: String,
    /// The tool's input as JSON text, laid out by the pane the way a tool call's arguments are.
    pub input: String,
    /// What the person may answer, in the order they are offered. The first is the one `Enter` chooses.
    pub choices: Vec<Choice>,
}

/// One answer the person may give.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub kind: Kind,
}

/// What a choice does, which is what decides how it is drawn and what a refusal says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

impl Kind {
    pub fn allows(self) -> bool {
        matches!(self, Self::AllowOnce | Self::AllowAlways)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow_once",
            Self::AllowAlways => "allow_always",
            Self::RejectOnce => "reject_once",
            Self::RejectAlways => "reject_always",
        }
    }

    /// ACP's `PermissionOptionKind`, which is these four names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "allow_once" => Some(Self::AllowOnce),
            "allow_always" => Some(Self::AllowAlways),
            "reject_once" => Some(Self::RejectOnce),
            "reject_always" => Some(Self::RejectAlways),
            _ => None,
        }
    }
}

impl Question {
    /// The choice `id` names, or one whose label or kind is `id`, so `plugins run agent-chat answer allow`
    /// works without knowing an agent's own option ids.
    pub fn choice(&self, id: &str) -> Option<&Choice> {
        let id = id.trim();
        let wanted = id.to_ascii_lowercase();
        self.choices
            .iter()
            .find(|choice| choice.id == id)
            .or_else(|| {
                self.choices.iter().find(|choice| choice.label.to_ascii_lowercase() == wanted)
            })
            .or_else(|| self.choices.iter().find(|choice| choice.kind.name() == wanted))
            .or_else(|| match wanted.as_str() {
                "allow" | "yes" => {
                    self.choices.iter().find(|choice| choice.kind == Kind::AllowOnce)
                }
                "always" => self.choices.iter().find(|choice| choice.kind == Kind::AllowAlways),
                "deny" | "no" | "reject" => {
                    self.choices.iter().find(|choice| choice.kind == Kind::RejectOnce)
                }
                _ => wanted
                    .parse::<usize>()
                    .ok()
                    .and_then(|number| number.checked_sub(1))
                    .and_then(|index| self.choices.get(index)),
            })
    }
}

/// The person's answers, handed from the window to the thread running the turn.
///
/// Shared by the client and every turn it starts. The thread looks for an answer between the lines it reads,
/// which is at least ten times a second, so nothing has to wake it.
#[derive(Debug, Default)]
pub struct Answers {
    given: Mutex<Vec<(String, String)>>,
}

impl Answers {
    /// The person chose `choice` for the question `question`.
    pub fn give(&self, question: &str, choice: &str) {
        let mut given = self.given.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        given.retain(|(id, _)| id != question);
        given.push((question.to_owned(), choice.to_owned()));
    }

    /// Every answer given since the last time this was asked.
    pub fn take(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.given.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

/// How long the thread running a turn waits for a line before it looks for an answer again.
pub const LOOK_AGAIN: std::time::Duration = std::time::Duration::from_millis(100);

/// The lines of `output`, read on a thread of their own so the turn's thread can look for the person's answers
/// while the agent is quietly waiting for one. The channel closes when the output does.
pub fn lines_of(output: impl Read + Send + 'static) -> Receiver<String> {
    let (to, from) = channel();
    let _ = std::thread::Builder::new().name("unluminous-chat agent output".to_owned()).spawn(
        move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if to.send(line).is_err() {
                    break;
                }
            }
        },
    );
    from
}

/// The question Claude Code's `can_use_tool` control request asks.
///
/// **Allow for this session is offered only when the agent suggested a rule**: its `permission_suggestions`
/// are what `Allow, and don't ask again` adds, and what it suggests is the honest description of what that
/// choice will do. A `setMode` suggestion switches the session's mode, such as `acceptEdits` after an edit.
pub fn claude_question(request_id: &str, request: &serde_json::Value) -> Question {
    let tool = request["tool_name"].as_str().unwrap_or("a tool").to_owned();
    let shown = request["display_name"].as_str().unwrap_or(&tool).to_owned();
    let title = match request["description"].as_str().filter(|said| !said.trim().is_empty()) {
        Some(said) => format!("{shown}: {}", said.trim()),
        None => shown.clone(),
    };
    let mut choices =
        vec![Choice { id: "allow".to_owned(), label: "Allow".to_owned(), kind: Kind::AllowOnce }];
    if let Some(label) = request["permission_suggestions"]
        .as_array()
        .and_then(|suggestions| suggestions.first())
        .map(suggestion_label)
    {
        choices.push(Choice { id: "always".to_owned(), label, kind: Kind::AllowAlways });
    }
    choices.push(Choice {
        id: "deny".to_owned(),
        label: "Deny".to_owned(),
        kind: Kind::RejectOnce,
    });
    Question {
        id: request_id.to_owned(),
        tool,
        title,
        input: serde_json::to_string(&request["input"]).unwrap_or_default(),
        choices,
    }
}

/// What a person reads for one of Claude Code's permission suggestions.
fn suggestion_label(suggestion: &serde_json::Value) -> String {
    match suggestion["type"].as_str().unwrap_or_default() {
        "setMode" => match suggestion["mode"].as_str().unwrap_or_default() {
            "acceptEdits" => "Allow edits for this session".to_owned(),
            "bypassPermissions" => "Allow everything for this session".to_owned(),
            mode => format!("Allow, and use {mode} for this session"),
        },
        "addRules" => {
            let rule = suggestion["rules"]
                .as_array()
                .and_then(|rules| rules.first())
                .map(|rule| match rule["ruleContent"].as_str().filter(|said| !said.is_empty()) {
                    Some(content) => {
                        format!("{}({content})", rule["toolName"].as_str().unwrap_or_default())
                    }
                    None => rule["toolName"].as_str().unwrap_or_default().to_owned(),
                })
                .unwrap_or_default();
            match rule.is_empty() {
                true => "Always allow this".to_owned(),
                false => format!("Always allow {rule}"),
            }
        }
        "addDirectories" => "Allow this folder for this session".to_owned(),
        _ => "Allow for this session".to_owned(),
    }
}

/// The `control_response` line that answers Claude Code's `can_use_tool` request with the person's choice.
pub fn claude_answer(
    request_id: &str,
    request: &serde_json::Value,
    kind: Kind,
) -> serde_json::Value {
    let response = match kind {
        Kind::AllowOnce => {
            serde_json::json!({ "behavior": "allow", "updatedInput": request["input"] })
        }
        Kind::AllowAlways => serde_json::json!({
            "behavior": "allow",
            "updatedInput": request["input"],
            "updatedPermissions": request["permission_suggestions"],
        }),
        Kind::RejectOnce | Kind::RejectAlways => serde_json::json!({
            "behavior": "deny",
            "message": "The person declined this in Unluminous.",
        }),
    };
    serde_json::json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response },
    })
}

/// The line written after a choice, kept in the answer's thinking so the conversation says what was decided.
pub fn decided(question: &Question, choice: &Choice) -> String {
    let verb = match choice.kind.allows() {
        true => "Allowed",
        false => "Denied",
    };
    match choice.kind {
        Kind::AllowAlways | Kind::RejectAlways => {
            format!("{verb} {} ({}).\n", question.title, choice.label)
        }
        _ => format!("{verb} {}.\n", question.title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The request Claude Code 2.1.312 really wrote when asked to create a file with `--permission-mode manual`.
    fn write_request() -> serde_json::Value {
        json!({
            "subtype": "can_use_tool",
            "tool_name": "Write",
            "display_name": "Write",
            "input": { "file_path": "C:\\probe\\hello.txt", "content": "hi" },
            "description": "hello.txt",
            "permission_suggestions": [
                { "type": "setMode", "mode": "acceptEdits", "destination": "session" }
            ],
            "tool_use_id": "toolu_01",
        })
    }

    #[test]
    fn claude_codes_request_becomes_a_question_with_the_choices_it_supports() {
        let question = claude_question("r1", &write_request());
        assert_eq!(question.id, "r1");
        assert_eq!(question.tool, "Write");
        assert_eq!(question.title, "Write: hello.txt");
        let labels: Vec<&str> = question.choices.iter().map(|one| one.label.as_str()).collect();
        assert_eq!(labels, ["Allow", "Allow edits for this session", "Deny"]);
        assert!(question.input.contains("hello.txt"));
    }

    #[test]
    fn a_request_with_no_suggestion_offers_no_always() {
        let mut request = write_request();
        request["permission_suggestions"] = json!([]);
        let question = claude_question("r1", &request);
        let kinds: Vec<Kind> = question.choices.iter().map(|one| one.kind).collect();
        assert_eq!(kinds, [Kind::AllowOnce, Kind::RejectOnce]);
    }

    #[test]
    fn each_answer_is_the_control_response_claude_code_reads() {
        let request = write_request();
        let allow = claude_answer("r1", &request, Kind::AllowOnce);
        assert_eq!(allow["type"], "control_response");
        assert_eq!(allow["response"]["request_id"], "r1");
        assert_eq!(allow["response"]["response"]["behavior"], "allow");
        assert_eq!(allow["response"]["response"]["updatedInput"]["content"], "hi");
        let always = claude_answer("r1", &request, Kind::AllowAlways);
        assert_eq!(always["response"]["response"]["updatedPermissions"][0]["mode"], "acceptEdits");
        let deny = claude_answer("r1", &request, Kind::RejectOnce);
        assert_eq!(deny["response"]["response"]["behavior"], "deny");
    }

    #[test]
    fn a_choice_is_found_by_id_label_kind_word_or_number() {
        let question = claude_question("r1", &write_request());
        assert_eq!(question.choice("deny").map(|one| one.kind), Some(Kind::RejectOnce));
        assert_eq!(question.choice("Allow").map(|one| one.kind), Some(Kind::AllowOnce));
        assert_eq!(question.choice("always").map(|one| one.kind), Some(Kind::AllowAlways));
        assert_eq!(question.choice("allow_always").map(|one| one.kind), Some(Kind::AllowAlways));
        assert_eq!(question.choice("2").map(|one| one.kind), Some(Kind::AllowAlways));
        assert_eq!(question.choice("no").map(|one| one.kind), Some(Kind::RejectOnce));
        assert!(question.choice("9").is_none());
    }

    #[test]
    fn an_answer_given_twice_keeps_the_last_and_is_taken_once() {
        let answers = Answers::default();
        answers.give("r1", "allow");
        answers.give("r1", "deny");
        answers.give("r2", "allow");
        assert_eq!(
            answers.take(),
            [("r1".to_owned(), "deny".to_owned()), ("r2".to_owned(), "allow".to_owned())]
        );
        assert!(answers.take().is_empty());
    }
}
