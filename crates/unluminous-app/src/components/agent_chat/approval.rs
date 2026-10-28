//! The card an agent's question is answered on: what it wants to do, what that would run, and the choices
//! (`task-2392`).
//!
//! *"In agent chat we also need to allow for user interaction that allows the model to prompt to allow."* The card
//! sits above the prompt box rather than in the conversation, because the agent has stopped until it is answered
//! and the answer is the next thing to do: scrolled up through an earlier answer, the person still sees it. It is
//! the Agent-Chat design's own parts: a raised card, the `NeedsOk` status well that `rux` has for "paused until
//! somebody allows it", a carved well holding what would run in the code face, and the choices as pills, the first
//! primary and a refusal a ghost. With two waiting it says `1 of 2` and shows the oldest.
//!
//! The keys are the choices' numbers, `1`, `2` and `3`, read only while no text box has the keyboard, so typing in
//! the prompt box is never taken for an answer.

use egui::{Pos2, Rect, Vec2};
use rux::components::{card_header, chat_card, chat_well, Lead, PillButton, Status};
use serde_json::Value;
use unluminous_chat::approval::{Kind, Question};

use super::kit;
use super::Act;
use crate::services::plugin_ui::Look;

/// The card's padding, between the header and the preview, and above the choices.
const PAD: f32 = 14.0;
const HEADER: f32 = 40.0;
const GAP: f32 = 10.0;
/// The most lines of what would run that the card shows.
pub const PREVIEW_LINES: usize = 6;
/// One line of the preview, in the code face.
const LINE: f32 = 16.0;
/// The carved well's padding round the preview.
const WELL_PAD: f32 = 10.0;
const BUTTON: f32 = 36.0;
const BETWEEN_BUTTONS: f32 = 8.0;
/// How far the card's shadow reaches.
const REACH: f32 = kit::CARD_REACH;

/// How tall the card is for `question`, in unscaled points, with its choices on one row or one a row.
pub fn height(question: &Question, stacked: bool) -> f32 {
    let lines = preview(question).len();
    let well = match lines {
        0 => 0.0,
        lines => lines as f32 * LINE + WELL_PAD * 2.0 + GAP,
    };
    let buttons = match stacked {
        true => question.choices.len() as f32 * (BUTTON + BETWEEN_BUTTONS) - BETWEEN_BUTTONS,
        false => BUTTON,
    };
    PAD + HEADER + GAP + well + buttons + PAD
}

/// Draw the card for the oldest of `asking` into `rect` and answer the choice pressed, or the key pressed.
///
/// @param speaker - the agent's name, which the header says is asking
pub fn show(
    pane: &mut crate::services::agent_chat::PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    asking: &[Question],
    speaker: &str,
    stacked: bool,
) -> Vec<Act> {
    let Some(question) = asking.first() else {
        return Vec::new();
    };
    let mut acts = Vec::new();
    let lines = preview(question);
    let who = match speaker.trim().is_empty() {
        true => "The agent".to_owned(),
        false => speaker.trim().to_owned(),
    };
    let title = format!("{who} wants to use {}", question.tool);
    let detail = match asking.len() {
        1 => question.title.clone(),
        waiting => format!("{} \u{b7} 1 of {waiting}", question.title),
    };
    // A press anywhere on the card is the card's, so the conversation under it does not take it.
    let _ = ui.interact(rect, ui.id().with("agent-chat-approval"), egui::Sense::click_and_drag());
    let pressed = kit::layer(pane, look, ui, "approval", rect, REACH, |rux| {
        let theme = rux.theme();
        chat_card(rux, rect, rux.z(rux::components::CARD_RADIUS));
        let inner = rect.shrink(rux.z(PAD));
        let row = Rect::from_min_size(inner.min, Vec2::new(inner.width(), rux.z(HEADER)));
        card_header(rux, row, Lead::Status(Status::NeedsOk), &title, &detail, Some(theme.ink.i500));
        let mut top = row.bottom() + rux.z(GAP);
        if !lines.is_empty() {
            let well = Rect::from_min_size(
                Pos2::new(inner.left(), top),
                Vec2::new(inner.width(), rux.z(lines.len() as f32 * LINE + WELL_PAD * 2.0)),
            );
            chat_well(rux, well, rux.z(rux::components::WELL_RADIUS));
            let style = rux.zs(rux::Style::mono(11.5));
            let room = well.width() - rux.z(WELL_PAD * 2.0);
            let painter = rux.painter().with_clip_rect(well);
            for (index, line) in lines.iter().enumerate() {
                let ink = match line.chars().next() {
                    Some('+') => theme.chat.sky,
                    Some('-') => theme.ink.i400,
                    _ => theme.ink.i700,
                };
                let galley = rux.elided(style, line, ink, room);
                let at = Pos2::new(
                    well.left() + rux.z(WELL_PAD),
                    well.top() + rux.z(WELL_PAD + index as f32 * LINE),
                );
                painter.galley(at, galley, ink);
            }
            top = well.bottom() + rux.z(GAP);
        }
        let mut pressed = None;
        let mut left = inner.left();
        for (index, choice) in question.choices.iter().enumerate() {
            let label = format!("{}  {}", index + 1, choice.label);
            let button = PillButton::new(&label);
            let button = match (index, choice.kind) {
                (_, Kind::RejectOnce | Kind::RejectAlways) => button.ghost(),
                (0, _) => button.primary(),
                _ => button,
            };
            let size = button.measure(rux);
            let place = match stacked {
                true => Rect::from_min_size(
                    Pos2::new(inner.left(), top),
                    Vec2::new(inner.width(), rux.z(BUTTON)),
                ),
                false => {
                    Rect::from_min_size(Pos2::new(left, top), Vec2::new(size.x, rux.z(BUTTON)))
                }
            };
            if button.show(rux, place).clicked() {
                pressed = Some(choice.id.clone());
            }
            left = place.right() + rux.z(BETWEEN_BUTTONS);
            if stacked {
                top = place.bottom() + rux.z(BETWEEN_BUTTONS);
            }
        }
        pressed
    });
    let pressed = pressed.or_else(|| key_pressed(ui, question));
    if let Some(choice) = pressed {
        acts.push(Act::Answer(question.id.clone(), choice));
    }
    acts
}

/// Whether the choices fit on one row of `width` unscaled points.
///
/// Reckoned from the number of characters at the pills' own size rather than laid out, because the card's height
/// has to be known before the `rux` layer that would measure them is open. Seven and a half points a character
/// over-reckons the 12.5 point face, so a row that is called a fit does fit.
pub fn fits_on_one_row(question: &Question, width: f32) -> bool {
    let total: f32 = question
        .choices
        .iter()
        .map(|choice| (choice.label.chars().count() + 3) as f32 * 7.5 + 36.0 + BETWEEN_BUTTONS)
        .sum();
    total <= width - PAD * 2.0
}

/// The choice whose number was pressed, while no text box has the keyboard.
fn key_pressed(ui: &egui::Ui, question: &Question) -> Option<String> {
    if crate::app::text_box_has_the_keyboard(ui.ctx()) {
        return None;
    }
    const KEYS: [egui::Key; 4] =
        [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4];
    ui.ctx().input_mut(|input| {
        question.choices.iter().zip(KEYS).find_map(|(choice, key)| {
            input.consume_key(egui::Modifiers::NONE, key).then(|| choice.id.clone())
        })
    })
}

/// What the agent would run, as a few lines: a command after `$`, a file's path and the start of what would be
/// written to it, an edit as the lines it takes out and puts in, and anything else as its JSON laid out.
pub fn preview(question: &Question) -> Vec<String> {
    let input: Value = serde_json::from_str(&question.input).unwrap_or(Value::Null);
    let text = |key: &str| input.get(key).and_then(Value::as_str).filter(|said| !said.is_empty());
    let mut lines = Vec::new();
    if let Some(command) = text("command").or_else(|| text("cmd")) {
        for (index, line) in command.lines().enumerate() {
            lines.push(match index {
                0 => format!("$ {line}"),
                _ => format!("  {line}"),
            });
        }
    } else if let Some(path) =
        text("file_path").or_else(|| text("path")).or_else(|| text("filePath"))
    {
        lines.push(the_end_of(path));
        if let (Some(old), Some(new)) = (text("old_string"), text("new_string")) {
            lines.extend(old.lines().take(2).map(|line| format!("- {line}")));
            lines.extend(new.lines().take(3).map(|line| format!("+ {line}")));
        } else if let Some(content) = text("content") {
            lines.extend(content.lines().map(|line| format!("+ {line}")));
        }
    } else if let Some(url) = text("url") {
        lines.push(url.to_owned());
    } else if !matches!(input, Value::Null) && input != Value::Object(Default::default()) {
        let laid_out = serde_json::to_string_pretty(&input).unwrap_or_default();
        lines.extend(laid_out.lines().map(str::to_owned));
    }
    if lines.len() > PREVIEW_LINES {
        lines.truncate(PREVIEW_LINES - 1);
        lines.push("\u{2026}".to_owned());
    }
    lines
}

/// The most of a path the preview shows, from its end.
const PATH_SHOWN: usize = 56;

/// A path cut to its last [`PATH_SHOWN`] characters, because the file's name is at the end and the folders a
/// person already knows are at the start.
fn the_end_of(path: &str) -> String {
    let characters: Vec<char> = path.chars().collect();
    match characters.len() > PATH_SHOWN {
        true => format!(
            "\u{2026}{}",
            characters[characters.len() - PATH_SHOWN..].iter().collect::<String>()
        ),
        false => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_chat::approval::Choice;

    fn question(tool: &str, input: Value) -> Question {
        Question {
            id: "r1".to_owned(),
            tool: tool.to_owned(),
            title: tool.to_owned(),
            input: input.to_string(),
            choices: vec![Choice {
                id: "allow".to_owned(),
                label: "Allow".to_owned(),
                kind: Kind::AllowOnce,
            }],
        }
    }

    #[test]
    fn a_command_is_shown_as_it_would_be_typed() {
        let lines =
            preview(&question("Bash", serde_json::json!({ "command": "git status\ngit diff" })));
        assert_eq!(lines, ["$ git status", "  git diff"]);
    }

    #[test]
    fn a_write_shows_its_file_and_the_start_of_what_it_writes() {
        let content = (1..=20).map(|line| format!("line {line}")).collect::<Vec<_>>().join("\n");
        let lines = preview(&question(
            "Write",
            serde_json::json!({ "file_path": "src/a.rs", "content": content }),
        ));
        assert_eq!(lines.len(), PREVIEW_LINES);
        assert_eq!(lines[0], "src/a.rs");
        assert_eq!(lines[1], "+ line 1");
        assert_eq!(lines.last().map(String::as_str), Some("\u{2026}"));
    }

    #[test]
    fn a_long_path_shows_its_end_where_the_files_name_is() {
        let path = format!("C:\\{}\\project\\hello.txt", "folder\\".repeat(12));
        let lines =
            preview(&question("Write", serde_json::json!({ "file_path": path, "content": "hi" })));
        assert!(
            lines[0].starts_with('\u{2026}') && lines[0].ends_with("project\\hello.txt"),
            "{lines:?}"
        );
    }

    #[test]
    fn an_edit_shows_what_it_takes_out_and_puts_in() {
        let lines = preview(&question(
            "Edit",
            serde_json::json!({ "file_path": "a.rs", "old_string": "let a = 1;", "new_string": "let a = 2;" }),
        ));
        assert_eq!(lines, ["a.rs", "- let a = 1;", "+ let a = 2;"]);
    }

    #[test]
    fn anything_else_is_its_json_laid_out_and_nothing_is_nothing() {
        let lines = preview(&question("Fetch", serde_json::json!({ "query": "x" })));
        assert_eq!(lines, ["{", "  \"query\": \"x\"", "}"]);
        assert!(preview(&question("Think", serde_json::json!({}))).is_empty());
    }

    #[test]
    fn the_card_is_taller_for_a_preview_and_for_choices_one_a_row() {
        let bare = question("Think", serde_json::json!({}));
        let command = question("Bash", serde_json::json!({ "command": "ls" }));
        assert!(height(&command, false) > height(&bare, false));
        assert!(height(&bare, true) >= height(&bare, false));
    }
}
