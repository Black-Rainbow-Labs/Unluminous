//! What an empty conversation shows: which agent is listening, what it can see, and where you were.
//!
//! `task-2211` took away the four starter chips ("Explain this file", "Find the bug", ...): a list of
//! things somebody might ask is a list of things they did not ask. What replaces them is what a person
//! needs before the first question and cannot see anywhere else in the window:
//!
//! 1. **Who is answering.** The agent's name, set large, on the project it is working in.
//! 2. **Whether it can.** A card of rows, each led by a status well or a mark in a well: the agent
//!    ready or not installed, the access it has, the project, and the file showing. These are the
//!    facts a person needs to trust an answer about code.
//! 3. **Where you were.** The last three conversations, each a row that opens it. They are the person's
//!    own, so they are not suggestions.
//! 4. **The keys.** One quiet line over the composer.
//!
//! It is drawn with the Agent-Chat design's parts, as `blocks` is (`task-2235`): the agent's avatar
//! raised from the pane, a card holding the rows, and the earlier conversations in a well. There are
//! no lamps. It gives up its sections from the bottom as the pane gets shorter, so a narrow column or
//! a strip along the bottom still reads as a composed page.

use egui::{Pos2, Rect, Sense, Vec2};
use rux::components::{card_header, chat_card, chat_raised_sm, chat_well, Lead, Status as State};
use rux::{Icon, Style};
use unluminous_chat::Wire;

use super::Act;
use crate::services::agent_chat::Parts;
use crate::services::plugin_ui::Look;

/// The headline: who is answering, and on what.
const HEADLINE: Style = Style::sans(19.0).semibold().tracking(-0.02).leading(1.2);
/// The line under it.
const LEDE: Style = Style::sans(12.5).leading(1.55);
/// How old a conversation is, at the right of its row.
const DETAIL: Style = Style::sans(11.0);
/// A conversation's name in the list.
const ROW: Style = Style::sans(12.5);

/// The name an agent goes by, which is what the person installed rather than what the row is called.
pub fn agent_name(wire: Wire, name: &str) -> String {
    match wire {
        Wire::ClaudeCli => "Claude Code".to_owned(),
        Wire::CodexCli => "Codex".to_owned(),
        _ if name.is_empty() => "The model".to_owned(),
        _ => name.to_owned(),
    }
}

/// How long ago `changed` was, in the fewest characters that say it: `now`, `5m`, `2h`, `3d`, `6w`.
pub fn ago(changed: u64, now: u64) -> String {
    let seconds = now.saturating_sub(changed);
    match seconds {
        0..=59 => "now".to_owned(),
        60..=3_599 => format!("{}m", seconds / 60),
        3_600..=86_399 => format!("{}h", seconds / 3_600),
        86_400..=1_209_599 => format!("{}d", seconds / 86_400),
        _ => format!("{}w", seconds / 604_800),
    }
}

/// One row of the status card: what leads it, the value, and what the value is.
struct Status {
    lead: Lead,
    label: &'static str,
    value: String,
    detail: String,
}

/// Draw the welcome into `area`, and say what was pressed.
pub fn show(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let mut acts = Vec::new();
    let still = parts.state.still;
    let kept = parts.state.blocks_rux.get_or_insert_with(|| {
        let theme = crate::theme::rux_theme();
        rux::RuxState::deterministic(theme)
    });
    kept.set_zoom(look.scale());
    kept.set_still(still);
    let provider = parts.configuration.provider().cloned();
    let chosen = parts.configuration.provider().and_then(|chosen| {
        parts.configuration.providers.iter().position(|one| one.name == chosen.name)
    });
    let problem = chosen.and_then(|at| parts.readiness.get(at).cloned().flatten());
    let history: Vec<(String, String, u64)> = parts
        .history
        .iter()
        .take(3)
        .map(|one| (one.id.clone(), one.name.clone(), one.changed))
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let project = parts.project.map(|path| path.to_path_buf());
    let showing = parts.showing.map(|path| path.to_path_buf());
    let permission = parts.configuration.permission;
    let tools = parts.configuration.tools;
    let agent = look.palette.agent;
    // **Under the pane's own id**, which is how the bubbles are named too: a canvas can hold several
    // chat nodes, each showing a welcome, and two widgets with one id are one widget to egui.
    let id = ui.id().with("agent-chat-welcome");
    // **The page takes a press over the whole of itself**, before anything on it is added, so a row
    // added later still wins the point it is drawn on. On a canvas, a press nothing in a node takes
    // falls through to the canvas, which then chooses no node at all: the starter chips this page
    // replaced happened to sit where people click, and its status screen does not.
    let _ = ui.interact(area, id.with("ground"), egui::Sense::click());
    crate::theme::in_step(kept);
    rux::layer(ui, kept, id, area, |rux| {
        let theme = rux.theme();
        let zoom = rux.zoom();
        let z = move |v: f32| v * zoom;
        let width = (area.width() - z(32.0)).min(z(420.0));
        let left = area.center().x - width / 2.0;
        let wide = width >= z(300.0);

        // Who is answering.
        let name = match &provider {
            Some(one) => agent_name(one.wire, &one.name),
            None => "No agent".to_owned(),
        };
        let place = project
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());
        let headline = match (&place, problem.is_some(), provider.is_some()) {
            (_, _, false) => "Choose an agent to begin".to_owned(),
            (_, true, _) => format!("{name} is not ready"),
            (Some(place), false, true) => format!("{name}, on {place}"),
            (None, false, true) => format!("{name} is ready"),
        };
        let lede = match (&problem, provider.is_some()) {
            (Some(why), _) => why.clone(),
            (None, false) => "Settings, Agent-Chat is where an agent or an address is added.".to_owned(),
            (None, true) => "Answers come back as text, charts, tables, checklists and small tools you can use in the conversation.".to_owned(),
        };

        // What it can see, and whether it can answer.
        let ready = problem.is_none() && provider.is_some();
        let mut rows = vec![Status {
            lead: Lead::Status(if ready { State::Done } else { State::NeedsOk }),
            label: "Agent",
            value: match (provider.is_some(), ready) {
                (false, _) => "none".to_owned(),
                (true, true) => "ready".to_owned(),
                (true, false) => "not ready".to_owned(),
            },
            detail: provider
                .as_ref()
                .map(|one| match one.model.trim().is_empty() {
                    true => one.wire.name().to_owned(),
                    false => one.model.trim().to_owned(),
                })
                .unwrap_or_default(),
        }];
        if let Some(one) = &provider {
            let (value, detail) = match one.is_a_program() {
                true => (
                    match permission {
                        unluminous_chat::Permission::Read => "read only",
                        unluminous_chat::Permission::Edit => "can edit",
                        unluminous_chat::Permission::Full => "full",
                    }
                    .to_owned(),
                    permission.name().to_owned(),
                ),
                false => (
                    match tools {
                        true => "Unluminous's commands".to_owned(),
                        false => "words only".to_owned(),
                    },
                    "tools".to_owned(),
                ),
            };
            rows.push(Status { lead: Lead::Mark(Icon::Key), label: "Access", value, detail });
        }
        if let Some(path) = &project {
            rows.push(Status {
                lead: Lead::Mark(Icon::Folder),
                label: "Project",
                value: place.clone().unwrap_or_default(),
                detail: path.parent().map(|p| p.display().to_string()).unwrap_or_default(),
            });
        }
        rows.push(Status {
            lead: Lead::Mark(Icon::File),
            label: "Showing",
            value: showing
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "nothing".to_owned()),
            detail: match (&showing, &project) {
                (Some(file), Some(root)) => file
                    .parent()
                    .and_then(|folder| folder.strip_prefix(root).ok())
                    .map(|folder| folder.display().to_string().replace('\\', "/"))
                    .filter(|folder| !folder.is_empty())
                    .map(|folder| format!("{folder}/"))
                    .unwrap_or_default(),
                _ => String::new(),
            },
        });

        // Measure, then give sections up from the bottom until it fits.
        let headline_style = rux.zs(HEADLINE);
        let lede_style = rux.zs(LEDE);
        let mark = z(56.0);
        let headline_galley =
            rux::text::wrapped(rux.painter(), headline_style, &headline, theme.ink.i900, width);
        let lede_galley = rux::text::wrapped(
            rux.painter(),
            lede_style,
            &lede,
            theme.ink.i500,
            width.min(z(360.0)),
        );
        // `padding: 12px; gap: 14px` round 34 point rows, which is the canvas's Status card.
        let row = z(34.0);
        let screen_height =
            z(12.0) * 2.0 + row * rows.len() as f32 + z(14.0) * (rows.len() as f32 - 1.0);
        let history_height = match history.is_empty() {
            true => 0.0,
            false => z(22.0) + z(16.0) + z(36.0) * history.len() as f32,
        };
        let top_part = mark + z(16.0) + headline_galley.size().y + z(6.0) + lede_galley.size().y;
        let keys_height = z(24.0);
        let room = area.height() - keys_height;
        let show_screen = top_part + z(22.0) + screen_height <= room;
        let show_history =
            show_screen && top_part + z(22.0) + screen_height + z(22.0) + history_height <= room;
        let show_mark = top_part <= room;
        let total = top_part
            + if show_screen { z(22.0) + screen_height } else { 0.0 }
            + if show_history && !history.is_empty() { z(22.0) + history_height } else { 0.0 };
        // Held a little above the middle, which is where the eye rests in a column.
        let mut pen = area.top() + ((room - total) * 0.38).max(z(12.0));

        if show_mark {
            // The agent's avatar, larger: a raised disc with the spark in the agent's colour.
            let disc = Rect::from_min_size(Pos2::new(left, pen), Vec2::splat(mark));
            chat_raised_sm(rux, disc, mark / 2.0);
            rux.mark(rux::Mark::new(Icon::Spark, z(24.0)).stroke(2.0), disc.center(), agent);
            pen += mark + z(16.0);
            rux.painter().galley(Pos2::new(left, pen), headline_galley.clone(), theme.ink.i900);
            pen += headline_galley.size().y + z(6.0);
            rux.painter().galley(Pos2::new(left, pen), lede_galley.clone(), theme.ink.i500);
            pen += lede_galley.size().y;
        }

        if show_screen {
            pen += z(22.0);
            let card = Rect::from_min_size(Pos2::new(left, pen), Vec2::new(width, screen_height));
            chat_card(rux, card, z(22.0));
            for (index, status) in rows.iter().enumerate() {
                let top = card.top() + z(12.0) + (row + z(14.0)) * index as f32;
                let line = Rect::from_min_size(
                    Pos2::new(card.left() + z(12.0), top),
                    Vec2::new(card.width() - z(24.0), row),
                );
                let detail = match (wide, status.detail.is_empty()) {
                    (true, false) => format!("{} \u{b7} {}", status.label, status.detail),
                    _ => status.label.to_owned(),
                };
                card_header(rux, line, status.lead, &status.value, &detail, None);
            }
            pen += screen_height;
        }

        if show_history && !history.is_empty() {
            pen += z(22.0);
            let small = rux.zs(Style::sans(12.0));
            let galley = rux::text::layout(rux.painter(), small, "Earlier", theme.ink.i500);
            rux.painter().galley(Pos2::new(left + z(4.0), pen), galley, theme.ink.i500);
            pen += z(22.0);
            let row_style = rux.zs(ROW);
            let detail_style = rux.zs(DETAIL);
            let list = Rect::from_min_size(
                Pos2::new(left, pen),
                Vec2::new(width, z(16.0) + z(36.0) * history.len() as f32),
            );
            chat_well(rux, list, z(22.0));
            pen += z(8.0);
            for (index, (conversation, name, changed)) in history.iter().enumerate() {
                let rect = Rect::from_min_size(
                    Pos2::new(left + z(4.0), pen),
                    Vec2::new(width - z(8.0), z(36.0)),
                );
                let response =
                    rux.ui.interact(rect, id.with(("earlier", conversation)), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Conversation: {name}"),
                    )
                });
                let hovered = response.hovered();
                if hovered {
                    rux.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let painter = rux.painter().clone();
                if index > 0 {
                    let hairline = match theme.dark {
                        true => egui::Color32::from_white_alpha(12), // any ground: the canvas's hairline on the dark well
                        false => egui::Color32::from_black_alpha(14), // any ground: the same line on the light well
                    };
                    painter.hline(
                        rect.left() + z(10.0)..=rect.right() - z(10.0),
                        rect.top(),
                        egui::Stroke::new(1.0, hairline),
                    );
                }
                let age = ago(*changed, now);
                let age_galley = rux::text::layout(&painter, detail_style, &age, theme.ink.i400);
                let room = rect.width() - z(12.0) - age_galley.size().x - z(24.0);
                let ink = if hovered { theme.chat.sky } else { theme.ink.i900 };
                let words = rux::text::elided(&painter, row_style, name, ink, room);
                rux::text::draw_left_capitals(
                    &painter,
                    Pos2::new(rect.left() + z(12.0), rect.center().y),
                    words,
                    row_style,
                    ink,
                );
                rux::text::draw_left_capitals(
                    &painter,
                    Pos2::new(rect.right() - z(12.0) - age_galley.size().x, rect.center().y),
                    age_galley,
                    detail_style,
                    theme.ink.i400,
                );
                if response.clicked() {
                    acts.push(Act::Open(conversation.clone()));
                }
                pen += z(36.0);
            }
        }

        // The keys, one quiet line just above the composer.
        let hint = match wide {
            true => {
                "Enter sends  \u{b7}  Shift Enter for a new line  \u{b7}  paste or drop a picture"
            }
            false => "Enter sends  \u{b7}  Shift Enter, new line",
        };
        let style = rux.zs(Style::sans(10.5));
        let galley =
            rux::text::elided(rux.painter(), style, hint, theme.ink.i300, area.width() - z(16.0));
        let at = Pos2::new(
            area.center().x - galley.size().x / 2.0,
            area.bottom() - keys_height + z(4.0),
        );
        rux.painter().galley(at, galley, theme.ink.i300);
    });
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_age_is_written_in_the_fewest_characters_that_say_it() {
        assert_eq!(ago(100, 130), "now");
        assert_eq!(ago(0, 300), "5m");
        assert_eq!(ago(0, 7_200), "2h");
        assert_eq!(ago(0, 3 * 86_400), "3d");
        assert_eq!(ago(0, 30 * 86_400), "4w");
        assert_eq!(ago(500, 100), "now", "a clock that went backwards is not a negative age");
    }

    #[test]
    fn an_agent_goes_by_the_name_of_the_program() {
        assert_eq!(agent_name(Wire::ClaudeCli, "claude"), "Claude Code");
        assert_eq!(agent_name(Wire::CodexCli, "codex"), "Codex");
        assert_eq!(agent_name(Wire::OpenAi, "local"), "local");
    }
}
