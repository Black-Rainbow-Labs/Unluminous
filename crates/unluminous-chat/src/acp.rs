//! Talking to an agent through the Agent Client Protocol (`task-2308`).
//!
//! ACP is JSON-RPC 2.0 over an agent program's standard input and output, one message a line, with the editor
//! as the client: `initialize`, then `session/new` (or `session/resume`, or `session/load`), then
//! `session/prompt`, while the agent streams `session/update` notifications and may ask the client for
//! permission before it runs a tool. Zed wrote it, and the agents in the ACP registry speak it, Claude's among
//! them through `@agentclientprotocol/claude-agent-acp`. `tasks/task-2308-acp-tdd.md` is the design.
//!
//! ## One process a turn
//!
//! ACP is designed for an agent that lives as long as the editor. Measured against Claude's adapter, a session
//! carries on in a second process through `session/resume` with nothing replayed, so a turn here has the shape
//! every other agent turn has: start the program, open the session, prompt, read until the answer, end. That
//! keeps [`crate::agent::Running`], the generation and stopping by killing the child exactly as they are, and
//! nothing outlives the window.
//!
//! ## The protocol is a value machine and the transport is thin
//!
//! [`Turn`] holds no process and no thread. It is handed each message the agent sends and answers what to send
//! back, what the pane should be told and whether the turn is over, so a recorded conversation is replayed
//! through it in a test, including what it would have answered. [`run`] is the transport around it.
//!
//! ## Unluminous runs none of the agent's tools
//!
//! The client offers no file system and no terminal, so the agent uses its own tools on the files, which is the
//! rule every agent wire keeps. Any request Unluminous did not offer is answered with JSON-RPC's `-32601`, so an
//! agent is never left waiting for an answer that will not come. What the client does answer is a permission
//! request, from `chat.permission`, because a turn in the pane has nobody to ask.

use std::io::{BufRead, BufReader, Write};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::agent::{Ask, Permission, Running};
use crate::provider::Provider;
use crate::wire::Reply;

/// The protocol version this client speaks. ACP's version is one integer, moved only by a breaking change.
pub const PROTOCOL_VERSION: u64 = 1;

/// JSON-RPC's own code for a method nobody here answers.
const METHOD_NOT_FOUND: i64 = -32601;

/// ACP's code for a session that needs the person to sign in first.
const AUTH_REQUIRED: i64 = -32000;

/// The most of one tool's answer kept for the pane, so a tool that read a whole file does not put the file in
/// the transcript twice over.
const LARGEST_ANSWER: usize = 20_000;

/// How long an agent is given to leave by itself once its input is closed, before it is killed.
const LEAVING: Duration = Duration::from_secs(2);

/// What one turn asks an ACP agent, read off disk where it had to be.
#[derive(Debug, Clone, Default)]
pub struct Asking {
    /// What the person said this turn.
    pub words: String,
    /// The pictures sent with it.
    pub pictures: Vec<Picture>,
    /// The folder the session works in, which ACP requires to be absolute.
    pub folder: String,
    /// The agent's own session to carry on, or empty for a new one.
    pub session: String,
    /// How much the agent may do without asking, which answers its permission requests.
    pub permission: Permission,
    /// The model to ask for, or empty for the agent's own.
    pub model: String,
}

/// A picture as an ACP `image` block needs it, and its path for an agent that takes no images.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub path: String,
    pub media: String,
    pub bytes: Vec<u8>,
}

/// What one message from the agent came to.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    /// Messages to write to the agent, in order.
    pub send: Vec<Value>,
    /// What the pane is told.
    pub replies: Vec<Reply>,
    /// Whether the turn is over, answered or failed.
    pub done: bool,
}

/// What a request this client sent was for, so its answer can be read.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Asked {
    Initialize,
    New,
    Resume,
    Load,
    Model,
    Prompt,
}

/// One tool call the agent reported, as it has been put together so far.
#[derive(Debug, Default)]
struct ToolSeen {
    id: String,
    title: String,
    kind: String,
    input: Value,
    content: Value,
    output: Value,
    shown: bool,
    answered: bool,
}

/// One turn of the protocol. See the module's own comment.
#[derive(Debug)]
pub struct Turn {
    asking: Asking,
    next: u64,
    asked: Vec<(u64, Asked)>,
    /// The agent's answer to `initialize`.
    agent: Value,
    /// The session this turn is in, once one is open.
    pub session: String,
    /// While `session/load` replays the conversation, which the pane already has.
    replaying: bool,
    tools: Vec<ToolSeen>,
    plan: String,
    context: Option<(u64, u64)>,
    started: bool,
    /// The model's name, for `Reply::Started`, when the session said which it is.
    model_name: String,
    /// Whether the turn is over.
    pub ended: bool,
}

impl Turn {
    pub fn new(asking: Asking) -> Self {
        Self {
            asking,
            next: 0,
            asked: Vec::new(),
            agent: Value::Null,
            session: String::new(),
            replaying: false,
            tools: Vec::new(),
            plan: String::new(),
            context: None,
            started: false,
            model_name: String::new(),
            ended: false,
        }
    }

    /// The first message: `initialize`, saying the client offers no file system and no terminal.
    pub fn start(&mut self) -> Vec<Value> {
        vec![self.request(
            Asked::Initialize,
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false,
                },
                "clientInfo": {
                    "name": "unluminous",
                    "title": "Unluminous",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )]
    }

    /// One message from the agent.
    pub fn message(&mut self, message: &Value) -> Step {
        let mut step = Step::default();
        if self.ended {
            return step;
        }
        let method = message["method"].as_str();
        let id = message.get("id").filter(|id| !id.is_null());
        match (method, id) {
            (Some(method), Some(id)) => self.asked_of_us(method, id, &message["params"], &mut step),
            (Some("session/update"), None) => self.update(&message["params"]["update"], &mut step),
            // `_auth/status_update` and anything else the agent says for its own reasons.
            (Some(_), None) => {}
            (None, Some(id)) => self.answered(id, message, &mut step),
            (None, None) => {}
        }
        step.done = self.ended;
        step
    }

    /// A request this client sends, remembered so its answer can be read.
    fn request(&mut self, asked: Asked, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.asked.push((id, asked));
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    /// The agent answered one of this client's requests.
    fn answered(&mut self, id: &Value, message: &Value, step: &mut Step) {
        let Some(at) = self.asked.iter().position(|(one, _)| Some(*one) == id.as_u64()) else {
            return;
        };
        let (_, asked) = self.asked.remove(at);
        if let Some(error) = message.get("error").filter(|error| !error.is_null()) {
            self.refused(asked, error, step);
            return;
        }
        let result = &message["result"];
        match asked {
            Asked::Initialize => self.initialized(result, step),
            Asked::New | Asked::Resume | Asked::Load => self.opened(asked, result, step),
            Asked::Model => {
                self.name_the_model(&result["configOptions"]);
                self.prompt(step);
            }
            Asked::Prompt => self.finished(result, step),
        }
    }

    fn initialized(&mut self, result: &Value, step: &mut Step) {
        let version = result["protocolVersion"].as_u64().unwrap_or(0);
        if version != PROTOCOL_VERSION {
            self.fail(
                step,
                format!(
                    "The agent speaks ACP version {version}, and Unluminous speaks version {PROTOCOL_VERSION}."
                ),
            );
            return;
        }
        self.agent = result.clone();
        let folder = self.asking.folder.clone();
        let session = self.asking.session.trim().to_owned();
        let capabilities = &self.agent["agentCapabilities"];
        let resumes = !capabilities["sessionCapabilities"]["resume"].is_null();
        let loads = capabilities["loadSession"].as_bool().unwrap_or(false);
        let message = match (session.is_empty(), resumes, loads) {
            (true, _, _) => {
                self.request(Asked::New, "session/new", json!({ "cwd": folder, "mcpServers": [] }))
            }
            (false, true, _) => self.request(
                Asked::Resume,
                "session/resume",
                json!({ "sessionId": session, "cwd": folder, "mcpServers": [] }),
            ),
            (false, false, true) => {
                self.replaying = true;
                self.request(
                    Asked::Load,
                    "session/load",
                    json!({ "sessionId": session, "cwd": folder, "mcpServers": [] }),
                )
            }
            // **Said in words `refused_to_resume` knows**, so the pane asks again with the conversation written
            // out in front of the question, which is the one way left to carry it on.
            (false, false, false) => {
                self.fail(
                    step,
                    "Session not found: this agent cannot carry on an earlier session.".to_owned(),
                );
                return;
            }
        };
        step.send.push(message);
    }

    /// A session is open: say which, ask for the model the row names, then prompt.
    fn opened(&mut self, asked: Asked, result: &Value, step: &mut Step) {
        self.replaying = false;
        self.session = match asked {
            Asked::New => result["sessionId"].as_str().unwrap_or_default().to_owned(),
            _ => self.asking.session.trim().to_owned(),
        };
        if self.session.is_empty() {
            self.fail(
                step,
                "The agent opened a session and did not say what it is called.".to_owned(),
            );
            return;
        }
        step.replies.push(Reply::Session(self.session.clone()));
        self.name_the_model(&result["configOptions"]);
        if self.model_name.is_empty() {
            if let Some(model) = result["models"]["currentModelId"].as_str() {
                self.model_name = model.to_owned();
            }
        }
        let wanted = self.asking.model.trim().to_owned();
        if !wanted.is_empty() {
            match self.model_change(result, &wanted) {
                Ok(Some(message)) => {
                    step.send.push(message);
                    return;
                }
                Ok(None) => {}
                Err(why) => {
                    self.fail(step, why);
                    return;
                }
            }
        }
        self.prompt(step);
    }

    /// The request that asks for the model `wanted`, nothing when it is already the one in use, or why it cannot
    /// be had.
    ///
    /// **The config option whose `category` is `model` first**, which is the protocol's own way, and the older
    /// `models` list and `session/set_model` only for an agent that offers no option. A model is matched by its
    /// value or by its name, ignoring case, because a person types `opus` and the page shows `Opus 5.5`.
    fn model_change(&mut self, result: &Value, wanted: &str) -> Result<Option<Value>, String> {
        let matches = |value: &str, name: &str| {
            value.eq_ignore_ascii_case(wanted) || name.eq_ignore_ascii_case(wanted)
        };
        let options = result["configOptions"].as_array().cloned().unwrap_or_default();
        if let Some(option) = options.iter().find(|option| option["category"] == "model") {
            let choices = option["options"].as_array().cloned().unwrap_or_default();
            let Some(choice) = choices.iter().find(|choice| {
                matches(
                    choice["value"].as_str().unwrap_or_default(),
                    choice["name"].as_str().unwrap_or_default(),
                )
            }) else {
                return Err(self.no_such_model(
                    wanted,
                    choices.iter().filter_map(|choice| choice["value"].as_str()),
                ));
            };
            if choice["value"] == option["currentValue"] {
                return Ok(None);
            }
            let session = self.session.clone();
            return Ok(Some(self.request(
                Asked::Model,
                "session/set_config_option",
                json!({
                    "sessionId": session,
                    "configId": option["id"],
                    "value": choice["value"],
                }),
            )));
        }
        let available = result["models"]["availableModels"].as_array().cloned().unwrap_or_default();
        if available.is_empty() {
            return Err(format!(
                "This row names the model `{wanted}`, and the agent offers no choice of model. Leave Model empty."
            ));
        }
        let Some(model) = available.iter().find(|model| {
            matches(
                model["modelId"].as_str().unwrap_or_default(),
                model["name"].as_str().unwrap_or_default(),
            )
        }) else {
            return Err(self.no_such_model(
                wanted,
                available.iter().filter_map(|model| model["modelId"].as_str()),
            ));
        };
        if model["modelId"] == result["models"]["currentModelId"] {
            return Ok(None);
        }
        self.model_name = model["modelId"].as_str().unwrap_or(wanted).to_owned();
        let session = self.session.clone();
        Ok(Some(self.request(
            Asked::Model,
            "session/set_model",
            json!({ "sessionId": session, "modelId": model["modelId"] }),
        )))
    }

    fn no_such_model<'a>(&self, wanted: &str, offered: impl Iterator<Item = &'a str>) -> String {
        format!(
            "The agent does not offer the model `{wanted}`. It offers: {}.",
            offered.collect::<Vec<_>>().join(", ")
        )
    }

    /// The name of the model option's current value, from a list of config options.
    fn name_the_model(&mut self, options: &Value) {
        let Some(option) =
            options.as_array().into_iter().flatten().find(|option| option["category"] == "model")
        else {
            return;
        };
        let current = &option["currentValue"];
        let named = option["options"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|choice| choice["value"] == *current)
            .and_then(|choice| choice["name"].as_str())
            .or_else(|| current.as_str());
        if let Some(named) = named {
            self.model_name = named.to_owned();
        }
    }

    /// Send the question.
    fn prompt(&mut self, step: &mut Step) {
        if !self.started {
            self.started = true;
            step.replies.push(Reply::Started { model: self.model_name.clone() });
        }
        let blocks = self.content();
        let session = self.session.clone();
        let message = self.request(
            Asked::Prompt,
            "session/prompt",
            json!({ "sessionId": session, "prompt": blocks }),
        );
        step.send.push(message);
    }

    /// The question as ACP content: the words, and each picture as an `image` block when the agent takes them
    /// and as its path in the words when it does not.
    fn content(&self) -> Vec<Value> {
        let images = self.agent["agentCapabilities"]["promptCapabilities"]["image"]
            .as_bool()
            .unwrap_or(false);
        let mut words = self.asking.words.clone();
        if !images && !self.asking.pictures.is_empty() {
            let named: Vec<&str> =
                self.asking.pictures.iter().map(|picture| picture.path.as_str()).collect();
            words = match words.trim().is_empty() {
                true => format!("Look at these images: {}", named.join(", ")),
                false => format!("{words}\n\nAttached images: {}", named.join(", ")),
            };
        }
        let mut blocks = vec![json!({ "type": "text", "text": words })];
        if images {
            for picture in &self.asking.pictures {
                blocks.push(json!({
                    "type": "image",
                    "mimeType": picture.media,
                    "data": crate::base64::encode(&picture.bytes),
                }));
            }
        }
        blocks
    }

    /// The answer to the prompt, which is the end of the turn.
    fn finished(&mut self, result: &Value, step: &mut Step) {
        self.ended = true;
        let usage = &result["usage"];
        if usage.is_object() {
            step.replies.push(Reply::Usage {
                input: usage["inputTokens"].as_u64().unwrap_or(0)
                    + usage["cachedReadTokens"].as_u64().unwrap_or(0)
                    + usage["cachedWriteTokens"].as_u64().unwrap_or(0),
                output: usage["outputTokens"].as_u64().unwrap_or(0),
            });
        }
        step.replies.push(Reply::Finished {
            reason: result["stopReason"].as_str().unwrap_or("end_turn").to_owned(),
        });
        if let Some((used, window)) = self.context.take() {
            step.replies.push(Reply::Context { used: Some(used), window: Some(window) });
        }
    }

    /// The agent refused one of this client's requests.
    fn refused(&mut self, asked: Asked, error: &Value, step: &mut Step) {
        let words = words_of(error);
        let why = match asked {
            // **Said in words `refused_to_resume` knows**: Claude's adapter answers an unknown session with
            // `-32002 Resource not found: <id>`, and the pane then asks again as a replay.
            Asked::Resume | Asked::Load => format!("Session not found: {words}"),
            Asked::New if error["code"].as_i64() == Some(AUTH_REQUIRED) => {
                let methods: Vec<&str> = self.agent["authMethods"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|method| method["name"].as_str())
                    .collect();
                match methods.is_empty() {
                    true => format!("The agent needs you to sign in first: {words}"),
                    false => format!(
                        "The agent needs you to sign in first ({}). Sign in with the agent's own command in a terminal, then ask again. {words}",
                        methods.join(", ")
                    ),
                }
            }
            _ => words,
        };
        self.fail(step, why);
    }

    fn fail(&mut self, step: &mut Step, why: String) {
        self.ended = true;
        step.replies.push(Reply::Failed(why));
    }

    /// A request from the agent.
    fn asked_of_us(&mut self, method: &str, id: &Value, params: &Value, step: &mut Step) {
        let answer = match method {
            "session/request_permission" => {
                json!({ "jsonrpc": "2.0", "id": id, "result": { "outcome": self.permit(params, step) } })
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": METHOD_NOT_FOUND, "message": format!("Unluminous does not offer {method}.") },
            }),
        };
        step.send.push(answer);
    }

    /// The answer to a permission request, from `chat.permission`. See §4.4 of the TDD.
    fn permit(&mut self, params: &Value, step: &mut Step) -> Value {
        let call = &params["toolCall"];
        let id = call["toolCallId"].as_str().unwrap_or_default();
        let seen = self.tools.iter().find(|tool| tool.id == id);
        let kind = call["kind"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| seen.map(|tool| tool.kind.clone()))
            .unwrap_or_default();
        let title = call["title"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| seen.map(|tool| tool.title.clone()))
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "a tool".to_owned());
        let allowed = match self.asking.permission {
            Permission::Full => true,
            Permission::Edit => {
                matches!(kind.as_str(), "read" | "edit" | "search" | "think" | "fetch" | "move")
            }
            Permission::Read => matches!(kind.as_str(), "read" | "search" | "think" | "fetch"),
        };
        let options = params["options"].as_array().cloned().unwrap_or_default();
        let pick = |kinds: &[&str]| {
            kinds
                .iter()
                .find_map(|wanted| options.iter().find(|option| option["kind"] == *wanted).cloned())
        };
        let chosen = match allowed {
            true => pick(&["allow_once", "allow_always"]),
            false => pick(&["reject_once", "reject_always"]),
        };
        if !allowed {
            step.replies.push(Reply::Thinking(format!(
                "Refused: {title}, because chat.permission is {}.\n",
                self.asking.permission.name()
            )));
        }
        match chosen.and_then(|option| option["optionId"].as_str().map(str::to_owned)) {
            Some(option) => json!({ "outcome": "selected", "optionId": option }),
            // An agent that offered nothing of the kind wanted is told the turn will not go on with this tool.
            None => json!({ "outcome": "cancelled" }),
        }
    }

    /// One `session/update`.
    fn update(&mut self, update: &Value, step: &mut Step) {
        if self.replaying {
            return;
        }
        match update["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" => {
                let text = text_of_block(&update["content"]);
                if !text.is_empty() {
                    step.replies.push(Reply::Text(text));
                }
            }
            "agent_thought_chunk" => {
                let text = text_of_block(&update["content"]);
                if !text.is_empty() {
                    step.replies.push(Reply::Thinking(text));
                }
            }
            "tool_call" | "tool_call_update" => self.tool(update, step),
            "plan" => {
                let plan = plan_of(update);
                if !plan.is_empty() && plan != self.plan {
                    self.plan = plan.clone();
                    step.replies.push(Reply::Thinking(plan));
                }
            }
            "usage_update" => {
                if let (Some(used), Some(size)) = (update["used"].as_u64(), update["size"].as_u64())
                {
                    self.context = Some((used, size));
                }
            }
            _ => {}
        }
    }

    /// A tool call or a change to one. Shown once there is something to show, and answered once it has ended.
    fn tool(&mut self, update: &Value, step: &mut Step) {
        let id = update["toolCallId"].as_str().unwrap_or_default().to_owned();
        if id.is_empty() {
            return;
        }
        let at = match self.tools.iter().position(|tool| tool.id == id) {
            Some(at) => at,
            None => {
                self.tools.push(ToolSeen { id: id.clone(), ..ToolSeen::default() });
                self.tools.len() - 1
            }
        };
        let tool = &mut self.tools[at];
        if let Some(title) = update["title"].as_str().filter(|title| !title.is_empty()) {
            tool.title = title.to_owned();
        }
        if let Some(kind) = update["kind"].as_str() {
            tool.kind = kind.to_owned();
        }
        if !update["rawInput"].is_null() {
            tool.input = update["rawInput"].clone();
        }
        if update["content"].as_array().is_some_and(|all| !all.is_empty()) {
            tool.content = update["content"].clone();
        }
        if !update["rawOutput"].is_null() {
            tool.output = update["rawOutput"].clone();
        }
        let status = update["status"].as_str().unwrap_or_default();
        let has_input = match &tool.input {
            Value::Null => false,
            Value::Object(fields) => !fields.is_empty(),
            _ => true,
        };
        // **Shown once the call says what it is doing**, which for Claude's adapter is the second message: the
        // first says `Read File` with no input, and the next names the file.
        if !tool.shown && (has_input || matches!(status, "in_progress" | "completed" | "failed")) {
            tool.shown = true;
            let name = match tool.title.is_empty() {
                true => update["name"].as_str().unwrap_or("tool").to_owned(),
                false => tool.title.clone(),
            };
            step.replies.push(Reply::ToolCall {
                id: id.clone(),
                name,
                arguments: match has_input {
                    true => tool.input.to_string(),
                    false => "{}".to_owned(),
                },
            });
        }
        if !tool.answered && matches!(status, "completed" | "failed") {
            tool.answered = true;
            let mut answer = answer_of(&tool.content);
            if answer.is_empty() {
                answer = crate::agent::text_of(&tool.output);
            }
            step.replies.push(Reply::ToolAnswer {
                id,
                answer: cut(answer, LARGEST_ANSWER),
                failed: status == "failed",
            });
        }
    }
}

/// The words of a JSON-RPC error: its message, and the detail an agent put in `data`.
fn words_of(error: &Value) -> String {
    let message = error["message"].as_str().unwrap_or("the agent refused").trim().to_owned();
    let detail = error["data"]["details"]
        .as_str()
        .or_else(|| error["data"]["message"].as_str())
        .or_else(|| error["data"].as_str())
        .map(str::trim)
        .filter(|detail| !detail.is_empty() && *detail != message);
    match detail {
        Some(detail) => format!("{message}: {detail}"),
        None => message,
    }
}

/// The text of one content block, which is all a pane draws of a message chunk.
fn text_of_block(block: &Value) -> String {
    match block["type"].as_str() {
        Some("text") => block["text"].as_str().unwrap_or_default().to_owned(),
        Some("resource_link") => block["uri"].as_str().unwrap_or_default().to_owned(),
        Some("resource") => block["resource"]["text"].as_str().unwrap_or_default().to_owned(),
        _ => String::new(),
    }
}

/// What a tool's content says: its text blocks, and each diff as the lines it took out and put in.
fn answer_of(content: &Value) -> String {
    let mut out = Vec::new();
    for item in content.as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("content") => {
                let text = text_of_block(&item["content"]);
                if !text.is_empty() {
                    out.push(text);
                }
            }
            Some("diff") => {
                let mut diff = format!("{}\n", item["path"].as_str().unwrap_or_default());
                for line in item["oldText"].as_str().unwrap_or_default().lines() {
                    diff.push_str(&format!("- {line}\n"));
                }
                for line in item["newText"].as_str().unwrap_or_default().lines() {
                    diff.push_str(&format!("+ {line}\n"));
                }
                out.push(diff);
            }
            _ => {}
        }
    }
    out.join("\n")
}

/// A plan as a checklist, for the thinking.
fn plan_of(update: &Value) -> String {
    let mut out = String::from("Plan:\n");
    let mut any = false;
    for entry in update["entries"].as_array().into_iter().flatten() {
        let mark = match entry["status"].as_str() {
            Some("completed") => "[x]",
            Some("in_progress") => "[~]",
            _ => "[ ]",
        };
        out.push_str(&format!("- {mark} {}\n", entry["content"].as_str().unwrap_or_default()));
        any = true;
    }
    match any {
        true => out,
        false => String::new(),
    }
}

/// `text` cut to at most `most` bytes, at a character boundary, saying it was cut.
fn cut(text: String, most: usize) -> String {
    if text.len() <= most {
        return text;
    }
    let mut at = most;
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    format!("{}\n[cut: {} more bytes]", &text[..at], text.len() - at)
}

/// The media type of a picture, from its file name.
fn media_of(path: &std::path::Path) -> String {
    let extension =
        path.extension().and_then(|one| one.to_str()).unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    }
    .to_owned()
}

/// What a turn asks, from what the pane hands every agent.
pub fn asking(provider: &Provider, ask: &Ask) -> Asking {
    let folder = ask.folder.clone().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    Asking {
        words: ask.prompt.clone(),
        pictures: ask
            .pictures
            .iter()
            .filter_map(|path| {
                Some(Picture {
                    path: path.display().to_string(),
                    media: media_of(path),
                    bytes: std::fs::read(path).ok()?,
                })
            })
            .collect(),
        folder: folder.display().to_string(),
        session: ask.session.clone(),
        permission: ask.permission,
        model: provider.model.clone(),
    }
}

/// Run one turn against an ACP agent, handing each reply to `on_reply`. The same shape as
/// [`crate::agent::run`], which hands an `acp` row here.
///
/// **A package version the machine's npm cannot find is asked for again with no version** (`task-2308`). The
/// registry pins each agent, and a machine whose npm reads a company mirror can be a release or two behind it:
/// on a Mac, `npx -y @agentclientprotocol/claude-agent-acp@0.94.0` failed with `ETARGET` while the public
/// registry had 0.94.0. Without the pin npx takes the newest version that mirror has.
///
/// **Nothing about it is said in the conversation** (`task-2313`: *"I don't want to see that ... If that's an
/// issue that claude keeps running into we should fix it on install, not show it every single time"*). Adding
/// or updating an agent asks npm which version it can fetch and pins that one, so an agent added since does not
/// get here at all. An endpoint added before that is retried quietly, and the pin is remembered with
/// [`remember_npm_lacks`], so the next turn runs the unpinned package straight away rather than failing first.
pub fn run(
    provider: &Provider,
    ask: &Ask,
    stopping: &AtomicBool,
    running: &Running,
    generation: u64,
    on_reply: &dyn Fn(Reply) -> bool,
) {
    let unpinned = without_a_missing_version(provider);
    if let Some((unpinned, pinned)) = &unpinned {
        if npm_lacks(pinned) {
            run_once(unpinned, ask, stopping, running, generation, on_reply);
            return;
        }
    }
    let missing = std::cell::RefCell::new(None::<String>);
    let first = |reply: Reply| -> bool {
        if let (Reply::Failed(why), Some(_)) = (&reply, &unpinned) {
            if a_version_was_not_found(why) {
                *missing.borrow_mut() = Some(why.clone());
                return true;
            }
        }
        on_reply(reply)
    };
    run_once(provider, ask, stopping, running, generation, &first);
    let (Some(why), Some((unpinned, pinned))) = (missing.into_inner(), unpinned) else {
        return;
    };
    if stopping.load(Ordering::Relaxed) {
        on_reply(Reply::Failed(why));
        return;
    }
    remember_npm_lacks(&pinned);
    run_once(&unpinned, ask, stopping, running, generation, on_reply);
}

/// The pinned npm packages this machine's npm has been found not to have, and whether one was added since the
/// window last asked. See [`run`].
static NPM_LACKS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static NPM_LACKS_ADDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Remember that npm here cannot fetch `pinned` (`@scope/name@1.2.3`), so a turn runs the package unpinned.
pub fn remember_npm_lacks(pinned: &str) {
    let mut lacks = NPM_LACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !lacks.iter().any(|known| known == pinned) {
        lacks.push(pinned.to_owned());
        NPM_LACKS_ADDED.store(true, Ordering::Relaxed);
    }
}

/// Whether npm here has been found not to have `pinned`.
pub fn npm_lacks(pinned: &str) -> bool {
    NPM_LACKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|known| known == pinned)
}

/// Every pin [`remember_npm_lacks`] has been told about, when one was added since the last call; `None`
/// otherwise, which is one atomic load. The window writes them down so the next start knows them too.
pub fn npm_lacks_if_changed() -> Option<Vec<String>> {
    if !NPM_LACKS_ADDED.swap(false, Ordering::Relaxed) {
        return None;
    }
    Some(NPM_LACKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
}

/// Whether npm said a pinned version does not exist where it looked.
pub fn a_version_was_not_found(words: &str) -> bool {
    words.contains("ETARGET") || words.contains("No matching version found")
}

/// The same endpoint with its npx package's version taken off, and the pinned package it named, or `None`
/// when it runs no pinned npm package.
pub fn without_a_missing_version(provider: &Provider) -> Option<(Provider, String)> {
    let program = std::path::Path::new(provider.command.trim())
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if program != "npx" {
        return None;
    }
    let mut changed = provider.clone();
    for argument in changed.args.iter_mut() {
        if argument.starts_with('-') {
            continue;
        }
        // `@scope/name@1.2.3` or `name@1.2.3`: the version is after the last `@` that is not the first character.
        let Some(at) = argument.rfind('@').filter(|at| *at > 0) else { continue };
        let version = &argument[at + 1..];
        if version.is_empty() || !version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let pinned = argument.clone();
        argument.truncate(at);
        return Some((changed, pinned));
    }
    None
}

/// One run of an agent's turn. See [`run`].
fn run_once(
    provider: &Provider,
    ask: &Ask,
    stopping: &AtomicBool,
    running: &Running,
    generation: u64,
    on_reply: &dyn Fn(Reply) -> bool,
) {
    let mut child =
        match crate::agent::start(provider, ask, &crate::agent::command_line(provider, ask)) {
            Ok(child) => child,
            Err(why) => {
                on_reply(Reply::Failed(why));
                return;
            }
        };
    let (Some(output), Some(mut input)) = (child.stdout.take(), child.stdin.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        on_reply(Reply::Failed("the agent gave nothing to read or write.".to_owned()));
        return;
    };
    // Read on a thread of its own, so an agent that writes a lot of logging cannot fill a pipe nobody empties.
    let said = child.stderr.take().map(|errors| {
        std::thread::spawn(move || {
            let mut all = String::new();
            for line in BufReader::new(errors).lines().map_while(Result::ok) {
                if all.len() < 8000 {
                    all.push_str(&line);
                    all.push('\n');
                }
            }
            all
        })
    });
    running.hold(generation, child);
    if stopping.load(Ordering::Relaxed) {
        running.stop(generation);
        let _ = running.take(generation).map(|mut child| child.wait());
        return;
    }
    let trace = std::env::var_os("UNLUMINOUS_ACP_TRACE").is_some();
    let write = |input: &mut std::process::ChildStdin, messages: Vec<Value>| -> bool {
        for message in messages {
            if trace {
                eprintln!("acp > {message}");
            }
            let line = format!("{message}\n");
            if input.write_all(line.as_bytes()).and_then(|()| input.flush()).is_err() {
                return false;
            }
        }
        true
    };
    let mut turn = Turn::new(asking(provider, ask));
    let mut carried_on = write(&mut input, turn.start());
    if carried_on {
        for line in BufReader::new(output).lines() {
            if stopping.load(Ordering::Relaxed) {
                carried_on = false;
                break;
            }
            let Ok(line) = line else { break };
            let line = line.trim();
            if trace {
                eprintln!("acp < {line}");
            }
            let Ok(message) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let step = turn.message(&message);
            if !write(&mut input, step.send) {
                break;
            }
            for reply in step.replies {
                if !on_reply(reply) {
                    carried_on = false;
                    break;
                }
            }
            if !carried_on || step.done {
                break;
            }
        }
    }
    // **The input is closed first**, which is how an ACP agent is told to leave, so it can finish writing its
    // session down before anything kills it.
    drop(input);
    let Some(mut child) = running.take(generation) else {
        return;
    };
    let gone = leave(&mut child);
    if !carried_on || turn.ended {
        return;
    }
    let errors = said.and_then(|thread| thread.join().ok()).unwrap_or_default();
    let ended = match gone {
        Some(status) if !status.success() => {
            format!("{} stopped with {status}.", provider.command.trim())
        }
        _ => "the agent stopped without finishing its answer.".to_owned(),
    };
    let detail = errors.trim();
    on_reply(Reply::Failed(match detail.is_empty() {
        true => ended,
        false => format!("{ended}\n\n{detail}"),
    }));
}

/// Wait up to [`LEAVING`] for an agent whose input is closed to leave by itself, then kill it. Answers how it
/// ended when it left by itself.
fn leave(child: &mut Child) -> Option<std::process::ExitStatus> {
    let until = Instant::now() + LEAVING;
    while Instant::now() < until {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_missing_npx_version_is_taken_off_the_package_and_nothing_else() {
        let mut provider = Provider::defaults()[0].clone();
        provider.wire = crate::provider::Wire::Acp;
        provider.command = "npx".to_owned();
        provider.args =
            vec!["-y".to_owned(), "@agentclientprotocol/claude-agent-acp@0.94.0".to_owned()];
        let (unpinned, pinned) =
            super::without_a_missing_version(&provider).expect("a pinned package");
        assert_eq!(pinned, "@agentclientprotocol/claude-agent-acp@0.94.0");
        assert_eq!(unpinned.args, vec!["-y", "@agentclientprotocol/claude-agent-acp"]);

        provider.args = vec!["-y".to_owned(), "@agentclientprotocol/claude-agent-acp".to_owned()];
        assert!(super::without_a_missing_version(&provider).is_none(), "nothing pinned");
        provider.args = vec!["some-agent@1.2.3".to_owned()];
        assert_eq!(super::without_a_missing_version(&provider).unwrap().0.args, vec!["some-agent"]);
        provider.command = "uvx".to_owned();
        assert!(super::without_a_missing_version(&provider).is_none(), "only npx");

        let said = "npx stopped with exit status: 1.

npm error code ETARGET
npm error notarget No matching version found for @agentclientprotocol/claude-agent-acp@0.94.0.";
        assert!(super::a_version_was_not_found(said));
        assert!(!super::a_version_was_not_found(
            "npx stopped with exit status: 1.

npm error code E404"
        ));
    }

    use super::*;

    fn asking(session: &str, permission: Permission) -> Asking {
        Asking {
            words: "What is in Cargo.toml?".to_owned(),
            folder: "C:/project".to_owned(),
            session: session.to_owned(),
            permission,
            ..Asking::default()
        }
    }

    fn initialized(turn: &mut Turn, capabilities: Value) -> Step {
        turn.start();
        turn.message(&json!({
            "jsonrpc": "2.0", "id": 0,
            "result": { "protocolVersion": 1, "agentCapabilities": capabilities, "authMethods": [] },
        }))
    }

    #[test]
    fn a_new_conversation_opens_a_new_session_and_an_old_one_is_resumed_or_loaded() {
        let mut turn = Turn::new(asking("", Permission::Full));
        let first = turn.start();
        assert_eq!(first[0]["method"], "initialize");
        assert_eq!(first[0]["params"]["clientCapabilities"]["terminal"], false);
        let step = initialized(&mut Turn::new(asking("", Permission::Full)), json!({}));
        assert_eq!(step.send[0]["method"], "session/new");
        assert_eq!(step.send[0]["params"]["cwd"], "C:/project");

        let step = initialized(
            &mut Turn::new(asking("s1", Permission::Full)),
            json!({ "loadSession": true, "sessionCapabilities": { "resume": {} } }),
        );
        assert_eq!(step.send[0]["method"], "session/resume");
        assert_eq!(step.send[0]["params"]["sessionId"], "s1");

        let step = initialized(
            &mut Turn::new(asking("s1", Permission::Full)),
            json!({ "loadSession": true }),
        );
        assert_eq!(step.send[0]["method"], "session/load");

        // Neither: the turn fails in words that make the pane ask again as a replay.
        let step = initialized(&mut Turn::new(asking("s1", Permission::Full)), json!({}));
        assert!(step.done);
        match &step.replies[0] {
            Reply::Failed(why) => assert!(crate::agent::refused_to_resume(why), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn what_session_load_replays_is_not_shown_again() {
        let mut turn = Turn::new(asking("s1", Permission::Full));
        initialized(&mut turn, json!({ "loadSession": true }));
        let replayed = turn.message(&json!({
            "jsonrpc": "2.0", "method": "session/update",
            "params": { "sessionId": "s1", "update": {
                "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "old answer" } } },
        }));
        assert!(replayed.replies.is_empty(), "{replayed:?}");
        let opened = turn.message(&json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
        assert_eq!(opened.replies[0], Reply::Session("s1".to_owned()));
        assert_eq!(opened.send[0]["method"], "session/prompt");
        let live = turn.message(&json!({
            "jsonrpc": "2.0", "method": "session/update",
            "params": { "sessionId": "s1", "update": {
                "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "new" } } },
        }));
        assert_eq!(live.replies, vec![Reply::Text("new".to_owned())]);
    }

    #[test]
    fn a_request_unluminous_did_not_offer_is_answered_method_not_found() {
        let mut turn = Turn::new(asking("", Permission::Full));
        let step = turn.message(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "fs/read_text_file", "params": { "path": "/x" },
        }));
        assert_eq!(step.send[0]["id"], 7);
        assert_eq!(step.send[0]["error"]["code"], METHOD_NOT_FOUND);
        assert!(!step.done);
    }

    fn permission_request(kind: &str) -> Value {
        json!({
            "jsonrpc": "2.0", "id": "p1", "method": "session/request_permission",
            "params": {
                "sessionId": "s1",
                "toolCall": { "toolCallId": "t1", "title": "Run it", "kind": kind },
                "options": [
                    { "optionId": "yes", "name": "Yes", "kind": "allow_once" },
                    { "optionId": "always", "name": "Always", "kind": "allow_always" },
                    { "optionId": "no", "name": "No", "kind": "reject_once" },
                ],
            },
        })
    }

    #[test]
    fn a_permission_is_answered_from_chat_permission() {
        let answer = |permission: Permission, kind: &str| {
            let mut turn = Turn::new(asking("", permission));
            let step = turn.message(&permission_request(kind));
            (
                step.send[0]["result"]["outcome"]["optionId"].as_str().unwrap_or("").to_owned(),
                step.replies,
            )
        };
        assert_eq!(answer(Permission::Full, "execute").0, "yes");
        assert_eq!(answer(Permission::Edit, "edit").0, "yes");
        assert_eq!(answer(Permission::Edit, "execute").0, "no");
        assert_eq!(answer(Permission::Read, "read").0, "yes");
        let (picked, said) = answer(Permission::Read, "edit");
        assert_eq!(picked, "no");
        assert!(
            matches!(&said[0], Reply::Thinking(words) if words.contains("chat.permission is read"))
        );
    }

    #[test]
    fn a_model_the_row_names_is_asked_for_by_its_config_option() {
        let mut turn =
            Turn::new(Asking { model: "Sonnet 5.5".to_owned(), ..asking("", Permission::Full) });
        initialized(&mut turn, json!({}));
        let options = json!([{
            "id": "model", "category": "model", "currentValue": "opus",
            "options": [ { "value": "opus", "name": "Opus 5.5" }, { "value": "sonnet", "name": "Sonnet 5.5" } ],
        }]);
        let opened = turn.message(&json!({
            "jsonrpc": "2.0", "id": 1, "result": { "sessionId": "s9", "configOptions": options },
        }));
        assert_eq!(opened.send[0]["method"], "session/set_config_option");
        assert_eq!(opened.send[0]["params"]["value"], "sonnet");
        let mut changed = options.clone();
        changed[0]["currentValue"] = json!("sonnet");
        let set = turn
            .message(&json!({ "jsonrpc": "2.0", "id": 2, "result": { "configOptions": changed } }));
        assert_eq!(set.replies[0], Reply::Started { model: "Sonnet 5.5".to_owned() });
        assert_eq!(set.send[0]["method"], "session/prompt");

        let mut turn =
            Turn::new(Asking { model: "gpt-9".to_owned(), ..asking("", Permission::Full) });
        initialized(&mut turn, json!({}));
        let refused = turn.message(&json!({
            "jsonrpc": "2.0", "id": 1, "result": { "sessionId": "s9", "configOptions": options },
        }));
        assert!(refused.done);
        assert!(matches!(&refused.replies[1], Reply::Failed(why) if why.contains("opus, sonnet")));
    }

    #[test]
    fn a_picture_is_an_image_block_only_for_an_agent_that_takes_images() {
        let picture = Picture {
            path: "C:/shot.png".to_owned(),
            media: "image/png".to_owned(),
            bytes: vec![1, 2],
        };
        let with = Asking { pictures: vec![picture], ..asking("", Permission::Full) };
        let mut turn = Turn::new(with.clone());
        turn.agent = json!({ "agentCapabilities": { "promptCapabilities": { "image": true } } });
        let blocks = turn.content();
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["data"], crate::base64::encode(&[1, 2]));
        let turn = Turn::new(with);
        let blocks = turn.content();
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0]["text"].as_str().unwrap_or_default().contains("C:/shot.png"));
    }

    #[test]
    fn a_long_answer_is_cut_at_a_character() {
        let text = "é".repeat(20);
        let cut = cut(text, 7);
        assert!(cut.starts_with("ééé"));
        assert!(cut.contains("more bytes"));
    }
}
