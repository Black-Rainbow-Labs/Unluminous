//! The agent's half of the components an answer holds: reading them, checking them, and pressing them.
//!
//! Everything a person can do with a component an agent can do too, through
//! `unluminous-cli plugins run agent-chat <verb>`, and by the same functions the pointer reaches
//! (`tasks/task-2211-agent-chat-intelligent-ui-tdd.md` §6.6):
//!
//! | Verb | What it does |
//! |---|---|
//! | `components [name]` | the reference, or one component with its fields and example |
//! | `validate <json>` | whether a block reads, and every problem with its path |
//! | `gallery` | opens a conversation showing every component |
//! | `press <message> <label>` | presses a button, a choice or a form's submit |
//! | `tick <message> <item>` | toggles a checklist item, by its words or its number |
//! | `set <message> <name> <value>` | sets a calculator's input or a form's field |
//! | `tab <message> <label>` | chooses a tab |
//!
//! `plugins view agent-chat` carries a `components` list: each block's type, title, whether it read,
//! what is wrong when it did not, and what it holds now, a calculator's outputs included. An agent can
//! therefore read what a component shows without a screenshot.

use serde_json::{json, Value};
use unluminous_chat::rich::component::{Action, Component, Kind};
use unluminous_chat::rich::{self, catalogue, Problem, Segment};
use unluminous_chat::{Message, Role};

use super::AgentChat;
use crate::components::agent_chat::blocks::{self, act_of, BlockState};
use crate::components::agent_chat::message::{block_key, segments_of};
use crate::components::agent_chat::Act;
use crate::services::plugin_ui::{Answer, Request};

/// One block an answer holds: where it is, and what it read as.
pub struct Found {
    pub message: u64,
    pub segment: usize,
    pub key: String,
    pub source: String,
    pub read: Result<Component, Vec<Problem>>,
}

/// Every block in the conversation's answers, in order.
pub fn blocks_in(messages: &[Message]) -> Vec<Found> {
    let mut out = Vec::new();
    for message in messages.iter().filter(|one| one.role == Role::Assistant) {
        let text = message.text();
        for (segment, one) in segments_of(message, &text).into_iter().enumerate() {
            if let Segment::Block { source, finished } = one {
                let read = rich::read_block(&source, finished);
                out.push(Found {
                    message: message.id,
                    segment,
                    key: block_key(message.id, segment),
                    source,
                    read,
                });
            }
        }
    }
    out
}

/// Visit `component` and everything inside it, with the key its state is kept under.
///
/// The keys are the ones `blocks::component_at` draws with: a child of a card, a stack or a row of
/// columns is `<key>/<index>`, and a component in a tab is `<key>/<tab>/<index>`.
pub fn walk(component: &Component, key: &str, visit: &mut dyn FnMut(&Component, &str)) {
    visit(component, key);
    match &component.kind {
        Kind::Card { children, .. } | Kind::Stack { children } => {
            for (index, child) in children.iter().enumerate() {
                walk(child, &format!("{key}/{index}"), visit);
            }
        }
        Kind::Columns { columns } => {
            for (index, child) in columns.iter().enumerate() {
                walk(child, &format!("{key}/{index}"), visit);
            }
        }
        Kind::Tabs { tabs } => {
            for (tab, one) in tabs.iter().enumerate() {
                for (index, child) in one.children.iter().enumerate() {
                    walk(child, &format!("{key}/{tab}/{index}"), visit);
                }
            }
        }
        _ => {}
    }
}

/// The catalogue as data, for `components` and for a test.
fn reference_value() -> Value {
    json!(catalogue::ENTRIES
        .iter()
        .map(|entry| json!({
            "name": entry.name,
            "group": entry.group,
            "when": entry.when,
            "fields": entry.fields.iter().map(|field| json!({
                "name": field.name, "kind": field.kind, "required": field.required, "about": field.about,
            })).collect::<Vec<Value>>(),
            "example": entry.example,
        }))
        .collect::<Vec<Value>>())
}

/// What one component holds now, for the view: a checklist's ticks, a form's values, a calculator's
/// inputs and outputs, which button was pressed, which tab is chosen.
fn state_value(component: &Component, state: Option<&BlockState>) -> Value {
    let empty = BlockState::default();
    let state = state.unwrap_or(&empty);
    match &component.kind {
        Kind::Checklist { items } => json!({
            "items": items.iter().enumerate().map(|(index, item)| json!({
                "label": item.label,
                "done": state.ticks.get(&index).copied().unwrap_or(item.done),
            })).collect::<Vec<Value>>(),
        }),
        Kind::Form { fields, .. } => json!({
            "values": fields.iter().map(|field| (field.name.clone(), json!(state.values.get(&field.name).cloned().unwrap_or_else(|| field.value.clone())))).collect::<serde_json::Map<String, Value>>(),
            "sent": state.pressed.is_some(),
        }),
        Kind::Calculator { inputs, outputs, .. } => {
            let values: std::collections::HashMap<String, f64> = inputs
                .iter()
                .map(|input| {
                    (
                        input.name.clone(),
                        state.inputs.get(&input.name).copied().unwrap_or(input.value),
                    )
                })
                .collect();
            let lookup = |name: &str| values.get(name).copied();
            json!({
                "inputs": values,
                "outputs": outputs.iter().map(|output| json!({
                    "label": output.label,
                    "value": output.expr.as_ref().and_then(|expr| expr.eval(&lookup).ok()),
                    "shown": output.expr.as_ref().and_then(|expr| expr.eval(&lookup).ok()).map(|value| rich::format::number(value, if output.format.is_empty() { "number" } else { &output.format }, &output.unit)),
                })).collect::<Vec<Value>>(),
            })
        }
        Kind::Tabs { tabs } => json!({
            "tab": tabs.get(state.tab).map(|tab| tab.label.clone()),
        }),
        Kind::Table { .. } => {
            json!({ "sort": state.sort.map(|(by, down)| json!({ "column": by, "descending": down })) })
        }
        Kind::Actions { .. } | Kind::Choices { .. } => json!({ "pressed": state.pressed }),
        _ => json!({}),
    }
}

impl AgentChat {
    /// Every block in the conversation as data, for `plugins view agent-chat`.
    pub fn components_value(&self) -> Value {
        json!(blocks_in(&self.session.chat.messages)
            .iter()
            .map(|found| match &found.read {
                Ok(component) => {
                    let mut inside = Vec::new();
                    walk(component, &found.key, &mut |one, key| {
                        inside.push(json!({
                            "type": one.type_name(),
                            "title": one.title,
                            "state": state_value(one, self.ui.blocks.get(key)),
                        }));
                    });
                    json!({
                        "message": found.message,
                        "segment": found.segment,
                        "type": component.type_name(),
                        "title": component.title,
                        "read": true,
                        "components": inside,
                    })
                }
                Err(problems) => json!({
                    "message": found.message,
                    "segment": found.segment,
                    "read": false,
                    "problems": problems.iter().map(Problem::line).collect::<Vec<String>>(),
                }),
            })
            .collect::<Vec<Value>>())
    }

    /// Answer one of the component verbs, or `None` when `command` is not one of them.
    pub fn component_command(
        &mut self,
        command: &str,
        arguments: &[String],
    ) -> Option<Result<Answer, String>> {
        let rest = || arguments.join(" ");
        Some(match command {
            "components" => Ok(match arguments.first().map(String::as_str) {
                None | Some("") => Answer::said(catalogue::reference()).with(reference_value()),
                Some(name) => match catalogue::describe(name) {
                    Some(text) => Answer::said(text).with(
                        reference_value()
                            .as_array()
                            .and_then(|all| all.iter().find(|one| one["name"] == name).cloned())
                            .unwrap_or(Value::Null),
                    ),
                    None => {
                        return Some(Err(format!(
                            "\"{name}\" is not a component. The components are {}.",
                            rich::component::TYPES.join(", ")
                        )))
                    }
                },
            }),
            "validate" => {
                let source = rest();
                let problems = rich::validate(&source);
                let errors = problems.iter().filter(|p| p.is_error()).count();
                let lines: Vec<String> = problems.iter().map(Problem::line).collect();
                Ok(Answer::said(match (errors, lines.is_empty()) {
                    (0, true) => "valid".to_owned(),
                    (0, false) => format!("valid, with notes: {}", lines.join("; ")),
                    _ => format!("not valid: {}", lines.join("; ")),
                })
                .with(json!({
                    "valid": errors == 0,
                    "problems": problems.iter().map(|p| json!({ "path": p.path, "message": p.message, "error": p.is_error() })).collect::<Vec<Value>>(),
                })))
            }
            "gallery" => {
                self.open_the_gallery();
                Ok(Answer::said("the component gallery")
                    .with(json!({ "id": self.session.chat.id })))
            }
            "press" => self.press(arguments),
            "tick" => self.tick(arguments),
            "set" => self.set(arguments),
            "tab" => self.choose_tab(arguments),
            _ => return None,
        })
    }

    /// Start a conversation holding one answer per component, which is the example set.
    pub fn open_the_gallery(&mut self) {
        self.new_conversation();
        for (question, answer) in catalogue::gallery() {
            let id = self.session.chat.next_id();
            self.session.chat.push(Message::said(id, Role::User, question));
            let id = self.session.chat.next_id();
            self.session.chat.push(Message::said(id, Role::Assistant, &answer));
        }
        self.session.chat.name = "Component gallery".to_owned();
        // Opened at the top, which is where a gallery starts. A wheel rather than an offset, because the
        // conversation sticks to its bottom and an offset written there is overwritten before the frame
        // ends; see `PaneState::wheel`.
        self.ui.jump_to_bottom = false;
        self.ui.wheel = Some(1.0e6);
        self.dirty = true;
        self.write_the_conversation();
    }

    /// The message an argument names: its id, or `last` for the newest answer.
    fn named_message(&self, word: Option<&String>) -> Result<u64, String> {
        let word = word.map(String::as_str).unwrap_or("last");
        if word == "last" {
            return self
                .session
                .chat
                .messages
                .iter()
                .rev()
                .find(|one| one.role == Role::Assistant)
                .map(|one| one.id)
                .ok_or_else(|| "there is no answer yet.".to_owned());
        }
        word.parse()
            .map_err(|_| format!("\"{word}\" is not a message id; give a number or `last`."))
    }

    /// Every component in one message, with its key.
    fn components_of(&self, message: u64) -> Result<Vec<(Component, String)>, String> {
        let mut out = Vec::new();
        for found in
            blocks_in(&self.session.chat.messages).into_iter().filter(|one| one.message == message)
        {
            if let Ok(component) = found.read {
                walk(&component, &found.key, &mut |one, key| {
                    out.push((one.clone(), key.to_owned()))
                });
            }
        }
        match out.is_empty() {
            true => Err(format!("message {message} holds no component that reads.")),
            false => Ok(out),
        }
    }

    /// Carry out what a press asked for, the way the pane's own `apply` does.
    fn carry_out(&mut self, act: Act) -> Result<String, String> {
        match act {
            Act::SendWords(words) => self.send_words(&words).map(|_| format!("sent \"{words}\"")),
            Act::Fill(words) => {
                self.draft = words.clone();
                Ok(format!("put \"{words}\" in the composer"))
            }
            Act::Copy(words) => {
                self.asking.push(Request::Copy(words.clone()));
                Ok("copied".to_owned())
            }
            Act::OpenFile(path, line) => {
                let asked = self.open_a_file(&path, line)?;
                self.asking.extend(asked);
                Ok(format!("opened {path}"))
            }
            _ => Ok(String::new()),
        }
    }

    /// `press <message> <label>`: a button, a choice, or a form's submit.
    fn press(&mut self, arguments: &[String]) -> Result<Answer, String> {
        let message = self.named_message(arguments.first())?;
        let label = arguments.get(1..).map(|rest| rest.join(" ")).unwrap_or_default();
        let mut labels = Vec::new();
        for (component, key) in self.components_of(message)? {
            let found: Option<(Act, String)> = match &component.kind {
                Kind::Actions { items } => {
                    labels.extend(items.iter().map(|b| b.label.clone()));
                    items
                        .iter()
                        .find(|b| b.label == label)
                        .and_then(|b| b.action.as_ref().map(|a| (act_of(a), b.label.clone())))
                }
                Kind::Choices { options, .. } => {
                    labels.extend(options.iter().cloned());
                    options
                        .iter()
                        .find(|o| **o == label)
                        .map(|o| (act_of(&Action::Send(o.clone())), o.clone()))
                }
                Kind::Form { fields, submit } => {
                    labels.push(submit.clone());
                    (*submit == label).then(|| {
                        let state = self.ui.blocks.get(&key).cloned().unwrap_or_default();
                        (
                            Act::SendWords(blocks::form_message(&component.title, fields, &state)),
                            submit.clone(),
                        )
                    })
                }
                _ => None,
            };
            if let Some((act, pressed)) = found {
                self.ui.blocks.entry(key).or_default().pressed = Some(pressed);
                let said = self.carry_out(act)?;
                return Ok(Answer::said(format!("pressed {label}: {said}"))
                    .with(json!({ "pressed": label })));
            }
        }
        Err(format!("message {message} has no button \"{label}\". It has: {}.", labels.join(", ")))
    }

    /// `tick <message> <item>`: toggle a checklist item, named by its words or its number from 1.
    fn tick(&mut self, arguments: &[String]) -> Result<Answer, String> {
        let message = self.named_message(arguments.first())?;
        let item = arguments.get(1..).map(|rest| rest.join(" ")).unwrap_or_default();
        for (component, key) in self.components_of(message)? {
            if let Kind::Checklist { items } = &component.kind {
                let at = item
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .filter(|n| *n < items.len())
                    .or_else(|| items.iter().position(|one| one.label == item));
                if let Some(at) = at {
                    let state = self.ui.blocks.entry(key).or_default();
                    let now = !state.ticks.get(&at).copied().unwrap_or(items[at].done);
                    state.ticks.insert(at, now);
                    return Ok(Answer::said(format!(
                        "{} {}",
                        if now { "ticked" } else { "unticked" },
                        items[at].label
                    ))
                    .with(json!({ "item": items[at].label, "done": now })));
                }
                return Err(format!(
                    "the checklist has no item \"{item}\". It has: {}.",
                    items.iter().map(|one| one.label.as_str()).collect::<Vec<&str>>().join(", ")
                ));
            }
        }
        Err(format!("message {message} holds no checklist."))
    }

    /// `set <message> <name> <value>`: a calculator's input or a form's field.
    fn set(&mut self, arguments: &[String]) -> Result<Answer, String> {
        let message = self.named_message(arguments.first())?;
        let name = arguments.get(1).cloned().unwrap_or_default();
        let value = arguments.get(2..).map(|rest| rest.join(" ")).unwrap_or_default();
        let mut names = Vec::new();
        for (component, key) in self.components_of(message)? {
            match &component.kind {
                Kind::Calculator { inputs, .. } => {
                    names.extend(inputs.iter().map(|i| i.name.clone()));
                    if let Some(input) = inputs.iter().find(|i| i.name == name) {
                        let number: f64 = value
                            .trim()
                            .parse()
                            .map_err(|_| format!("{name} takes a number, not \"{value}\"."))?;
                        let number = number.clamp(input.min, input.max);
                        self.ui.blocks.entry(key).or_default().inputs.insert(name.clone(), number);
                        return Ok(Answer::said(format!(
                            "{name} is {}",
                            rich::format::plain(number)
                        ))
                        .with(json!({ "name": name, "value": number })));
                    }
                }
                Kind::Form { fields, .. } => {
                    names.extend(fields.iter().map(|f| f.name.clone()));
                    if fields.iter().any(|f| f.name == name) {
                        self.ui
                            .blocks
                            .entry(key)
                            .or_default()
                            .values
                            .insert(name.clone(), value.clone());
                        return Ok(Answer::said(format!("{name} is {value}"))
                            .with(json!({ "name": name, "value": value })));
                    }
                }
                _ => {}
            }
        }
        Err(format!(
            "message {message} has no input or field \"{name}\". It has: {}.",
            names.join(", ")
        ))
    }

    /// `tab <message> <label>`: choose a tab, by its label or its number from 1.
    fn choose_tab(&mut self, arguments: &[String]) -> Result<Answer, String> {
        let message = self.named_message(arguments.first())?;
        let wanted = arguments.get(1..).map(|rest| rest.join(" ")).unwrap_or_default();
        for (component, key) in self.components_of(message)? {
            if let Kind::Tabs { tabs } = &component.kind {
                let at = wanted
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .filter(|n| *n < tabs.len())
                    .or_else(|| tabs.iter().position(|tab| tab.label == wanted));
                return match at {
                    Some(at) => {
                        self.ui.blocks.entry(key).or_default().tab = at;
                        Ok(Answer::said(format!("showing {}", tabs[at].label))
                            .with(json!({ "tab": tabs[at].label })))
                    }
                    None => Err(format!(
                        "there is no tab \"{wanted}\". The tabs are: {}.",
                        tabs.iter().map(|tab| tab.label.as_str()).collect::<Vec<&str>>().join(", ")
                    )),
                };
            }
        }
        Err(format!("message {message} holds no tabs."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walking_a_component_gives_the_keys_the_drawing_uses() {
        let source = r#"{"type": "tabs", "tabs": [{"label": "a", "children": [{"type": "callout", "text": "x"}]}, {"label": "b", "children": [{"type": "checklist", "items": ["one"]}]}]}"#;
        let component = rich::read_block(source, true).unwrap();
        let mut keys = Vec::new();
        walk(&component, "block-1-0", &mut |one, key| {
            keys.push(format!("{} {key}", one.type_name()))
        });
        assert_eq!(
            keys,
            vec!["tabs block-1-0", "callout block-1-0/0/0", "checklist block-1-0/1/0"]
        );
    }

    #[test]
    fn the_reference_lists_every_component_with_an_example() {
        let reference = reference_value();
        let all = reference.as_array().unwrap();
        assert_eq!(all.len(), rich::component::TYPES.len());
        assert!(all
            .iter()
            .all(|one| one["example"].as_str().is_some_and(|e| e.contains("\"type\""))));
    }
}
