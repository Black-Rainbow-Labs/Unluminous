//! The four lanes.
//!
//! Which lane a card is in and where a drag lands are `services::agent_tasks::board`; this paints them.
//!
//! ## The look is the Agent-Tasks design (`task-2241`)
//!
//! The Claude Design canvas "Unluminous", page Agent-Tasks, artboard "Agent-Tasks board · Dark". Each lane is
//! a well carved into the page, `border-radius: 26px` with `--in`, the four of them sharing the width 22 points
//! apart and running the whole height of the board. A lane's heading is a round chip holding a mark for the
//! lane, its name and a count. A card is extruded from the well with `--out`: its title, a track of its todos,
//! and at its foot its key, who has it and one round button. All of it is `rux::components::board`, drawn
//! through `kit::layer`, and none of the colours are chosen here.
//!
//! What the design took away from a card, and why it is not drawn: the epic's coloured edge and chip, the
//! priority chevron and the comment count. They are all still in the ticket modal, which is one click away.

use egui::{Pos2, Rect, Vec2};

use super::kit;
use crate::services::agent_tasks::model::{Assignee, Status, Task};
use crate::services::agent_tasks::{board as arithmetic, AgentTasks};
use crate::services::plugin_ui::{Look, Request};

/// The narrowest a lane is squeezed to before the board scrolls sideways instead, and the gap between two.
///
/// A pane can be 420 points wide down the side of the window, which holds one lane and a sliver, so below the
/// point where four lanes fit they are narrowed as far as [`LANE_MIN`] and the board scrolls sideways after
/// that. Narrowing further would leave a lane too thin for a card's title, which is worse than scrolling.
const LANE_MIN: f32 = 240.0;
const GAP: f32 = 22.0;
/// The gap between two cards in a lane: the lane's own `gap: 14px`.
const CARD_GAP: f32 = 14.0;
/// `padding: 6px 26px 26px` round the lanes.
const ABOVE: f32 = 6.0;
const BELOW: f32 = 26.0;
/// A lane's padding, and how far below its top its cards start: the padding, the heading's two points, the
/// 30 point chip and the gap under it. The same arithmetic as `rux::components::Lane::heading`.
const LANE_PAD: f32 = 14.0;
const LANE_HEADING: f32 = 14.0 + 2.0 + 30.0 + 14.0;
/// How round a lane is, for the clip its cards are cut to while it is scrolled.
const LANE_RADIUS: f32 = 26.0;

/// What a lane is called on the board: the design writes them in sentence case.
fn lane_name(status: Status) -> &'static str {
    match status {
        Status::New => "New",
        Status::QaFailed => "QA failed",
        Status::InProgress => "In progress",
        Status::AgentDone => "Agent done",
    }
}

/// The mark in a lane's chip.
fn lane_mark(status: Status) -> rux::components::LaneMark {
    match status {
        Status::New => rux::components::LaneMark::New,
        Status::QaFailed => rux::components::LaneMark::Failed,
        Status::InProgress => rux::components::LaneMark::Running,
        Status::AgentDone => rux::components::LaneMark::Done,
    }
}

/// How tall a run of cards is: one gap **between** each pair and none after the last.
fn cards_tall(heights: &[f32], look: &Look<'_>) -> f32 {
    match heights.len() {
        0 => 0.0,
        held => heights.iter().sum::<f32>() + (held - 1) as f32 * CARD_GAP * look.scale(),
    }
}

/// What the window knows about a ticket that its row does not: whether its agent is running here, and
/// whether Start would do anything.
fn live_for(board: &AgentTasks, task: &Task) -> Live {
    let attached =
        board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
    Live {
        attached,
        // Start claims an unclaimed ticket. One that already has a session is resumed instead, and one
        // assigned to a person is handed to the configured agent — which is still a start, so it counts.
        //
        // **And a Codex ticket whose process has gone can be started again.** Codex names its own sessions, so
        // the id on the ticket is only Unluminous's marker that a worker was here and there is nothing to resume.
        can_start: task.session_id.is_none()
            || (!attached
                && !crate::services::agent_tasks::agent::can_resume(task.assignee)
                && task.assignee.is_an_agent()),
        can_resume: !attached
            && task.session_id.is_some()
            && crate::services::agent_tasks::agent::can_resume(task.assignee),
    }
}

/// What the window knows about a ticket, which decides what the round button on its card does.
///
/// Unluminous's rule is that a control which cannot apply is absent, so a card for a person's ticket with
/// nothing to resume carries no button at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Live {
    /// A terminal for this ticket is running in this window.
    pub attached: bool,
    /// Start would do something: the ticket is not already claimed and is not a person's.
    pub can_start: bool,
    /// The ticket has a conversation its agent can be brought back into.
    pub can_resume: bool,
}

/// What the round button on a card does: the design's play and pause.
fn card_action(live: Live) -> (rux::components::CardAction, &'static str) {
    match (live.attached, live.can_start, live.can_resume) {
        // Pause is an interrupt: the agent stops what it is doing and its terminal stays, so the work can be
        // carried on with a word. Closing the terminal is the modal's `Stop`.
        (true, _, _) => (rux::components::CardAction::Pause, "interrupt"),
        (false, true, _) => (rux::components::CardAction::Start, "start"),
        (false, false, true) => (rux::components::CardAction::Start, "resume"),
        _ => (rux::components::CardAction::None, ""),
    }
}

/// What a card's round button is called, which is what a screen reader says and what a test presses.
fn action_label(command: &str, key: &str) -> String {
    match command {
        "interrupt" => format!("Pause {key}"),
        "resume" => format!("Resume {key}"),
        _ => format!("Start {key}"),
    }
}

/// The board: four lanes across, scrolling sideways when the pane is narrower than they are.
pub fn show(
    board: &mut AgentTasks,
    state: &rux::RuxState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Request> {
    let mut requests = Vec::new();
    // **Clicking the board gives it the keyboard**, which is what makes the arrow keys reach it at all. Added
    // before anything else is drawn, because egui gives a press to the last widget that overlaps it: a background
    // added first is under every card and every button, so it answers only for a press nothing else wanted.
    let background =
        ui.interact(area, ui.id().with("agent-tasks-board-background"), egui::Sense::click());
    background.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, ui.is_enabled(), "Board background")
    });
    if background.clicked() {
        requests.push(Request::TakeTheKeyboard(true));
        // And away from a ticket's terminal, if it had them: the keys now move the ring on the board.
        board.focus_the_terminal(false);
    }
    let scale = look.scale();
    let side = super::SIDE * scale;
    let room = area.width() - side * 2.0;
    let lanes = Status::ALL.len() as f32;
    let gap = GAP * scale;
    // The four share the width, which is what the design's grid does, and are squeezed no narrower than
    // `LANE_MIN`: past that the board scrolls sideways.
    let lane_width = ((room - gap * (lanes - 1.0)) / lanes).max(LANE_MIN * scale);
    let content = lane_width * lanes + gap * (lanes - 1.0);
    let scroll = board.scroll_the_lanes(ui, area, content - room);
    let card_width = lane_width - LANE_PAD * scale * 2.0;
    let mut to_open = None;
    let mut to_act: Option<(&'static str, String)> = None;
    let mut dragged = None;
    let mut dropped: Option<(Status, i64)> = None;
    // In this `Ui`'s own points, because a board drawn on a canvas node is inside a layer carrying the
    // camera and the lane rectangles below are in the canvas's points. `task-2003`.
    let pointer = crate::components::controls::pointer_in(ui);
    let released = ui.ctx().input(|input| input.pointer.any_released());
    let query = board.query().to_owned();
    let dragging = board.dragging;

    // Laid out in three passes, and the order is a borrow rather than a preference: the geometry needs
    // nothing, the scroll offsets change the provider, and the drawing reads it.
    //
    // First: where each lane is, and how tall each of its cards is. **A card is as tall as its title**, one
    // line or two, which is what the design draws, so the heights are measured before anything is placed.
    let top = area.min.y + ABOVE * scale;
    let bottom = (area.max.y - BELOW * scale).max(top);
    let mut geometry: Vec<(Status, Rect, Vec<f32>)> = Vec::new();
    for (index, status) in Status::ALL.into_iter().enumerate() {
        let left = area.min.x + side + index as f32 * (lane_width + gap) - scroll;
        if left > area.max.x || left + lane_width < area.min.x {
            continue;
        }
        // **The lane keeps its full width and is clipped by the pane's edge**, rather than being shrunk to what
        // is visible, so a lane half off the edge slides behind it instead of reflowing its cards.
        let heights = board
            .board()
            .lane(status)
            .map(|lane| {
                lane.tasks
                    .iter()
                    .filter(|task| arithmetic::matches(task, &query))
                    .map(|task| {
                        rux::components::TaskCard::height_of(
                            ui.painter(),
                            scale,
                            task.display_title(),
                            card_width,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let lane_area =
            Rect::from_min_max(Pos2::new(left, top), Pos2::new(left + lane_width, bottom));
        geometry.push((status, lane_area, heights));
    }
    let cards_area_of = |lane: Rect| {
        Rect::from_min_max(
            Pos2::new(lane.min.x + LANE_PAD * scale, lane.min.y + LANE_HEADING * scale),
            Pos2::new(lane.max.x - LANE_PAD * scale, lane.max.y - LANE_PAD * scale),
        )
    };
    // Second: how far each of them is scrolled, which is the only thing here that changes anything.
    let downs: Vec<f32> = geometry
        .iter()
        .map(|(status, lane_area, heights)| {
            let most = (cards_tall(heights, look) - cards_area_of(*lane_area).height()).max(0.0);
            // The visible part, because a lane keeps its full width even when it runs off the edge and a wheel
            // over the pane next door is not a wheel over this lane.
            board.scroll_a_lane(ui, *status, lane_area.intersect(area), most)
        })
        .collect();
    // The keyboard, between the geometry and the drawing, because moving the ring needs the counts each lane
    // holds **after** the search has narrowed it and the drawing needs to know where the ring ended up.
    //
    // Only when the window says this plugin has the keys, and only when nothing is typing.
    let counts = board.lane_counts();
    let mut open_the_chosen = None;
    if look.has_the_keyboard
        && !crate::app::text_box_has_the_keyboard(ui.ctx())
        && !crate::app::a_modal_has_the_keyboard(ui.ctx())
    {
        // **Consumed, not just read.** A key the board acts on has to be taken out of the frame's input, or
        // whatever is drawn after the board sees the same press.
        let (across, down, enter) = ui.ctx().input_mut(|input| {
            let none = egui::Modifiers::NONE;
            (
                i64::from(input.consume_key(none, egui::Key::ArrowRight))
                    - i64::from(input.consume_key(none, egui::Key::ArrowLeft)),
                i64::from(input.consume_key(none, egui::Key::ArrowDown))
                    - i64::from(input.consume_key(none, egui::Key::ArrowUp)),
                input.consume_key(none, egui::Key::Enter),
            )
        });
        if across != 0 || down != 0 {
            board.move_the_choice(&counts, across, down);
            if let Some((lane, _)) = board.chosen {
                board.show_the_lane(lane, lane_width, gap, room, content - room);
            }
        }
        if enter {
            open_the_chosen = board.the_chosen_ticket(&counts);
        }
    }
    let ring = board.chosen;

    // Third: the drawing, which only reads.
    let snapshot = board.board();
    let agent = look.palette.agent;
    for (index, (status, lane_area, heights)) in geometry.iter().enumerate() {
        let status = *status;
        let lane_area = *lane_area;
        let cards: Vec<&Task> = snapshot
            .lane(status)
            .map(|lane| {
                lane.tasks.iter().filter(|task| arithmetic::matches(task, &query)).collect()
            })
            .unwrap_or_default();
        let lives: Vec<Live> = cards.iter().map(|task| live_for(board, task)).collect();
        let cards_area = cards_area_of(lane_area);
        let content = cards_tall(heights, look);
        let down = downs.get(index).copied().unwrap_or(0.0);
        let mut tops = Vec::new();
        let mut pen = cards_area.min.y - down;
        for height in heights {
            tops.push(pen);
            pen += height + CARD_GAP * scale;
        }
        let cut = content > cards_area.height();
        // The lane is drawn into a layer of its own, wide enough for its cards' shadows to fall into the gap
        // beside it. It is clipped to the pane, so a lane half off the edge is cut by it.
        let pane_clip = ui.clip_rect().intersect(area);
        kit::layer(state, ui, ("lane", status.name()), lane_area, gap / 2.0, |rux| {
            rux::components::Lane::new(lane_name(status), cards.len(), lane_mark(status))
                .show(rux, lane_area);
            // **The cards are cut to the lane while it is scrolled**, both what `egui` draws and the
            // decoration, so a card scrolled up under the heading does not draw its body over the lane's name.
            // A lane that is not scrolled is left uncut, so its top card keeps the shadow above it.
            let room = Rect::from_min_max(
                Pos2::new(lane_area.min.x, cards_area.min.y),
                Pos2::new(lane_area.max.x, lane_area.max.y),
            );
            if cut {
                rux.chrome.clip(room, rux::Corners::bottom(LANE_RADIUS * scale));
            }
            let mut cards_ui = rux.ui.new_child(egui::UiBuilder::new().max_rect(cards_area));
            // Intersected with whatever the caller had cut this `Ui` to, which on the canvas is the node's own
            // part of the pane. `Ui::set_clip_rect` is an assignment. `task-1914`.
            let clip = match cut {
                true => room.intersect(pane_clip),
                false => lane_area.expand(gap / 2.0).intersect(pane_clip),
            };
            cards_ui.set_clip_rect(clip);
            let mut inner = rux::Rux { ui: &mut cards_ui, state: rux.state, chrome: rux.chrome };
            for (row, task) in cards.iter().enumerate() {
                let height = heights[row];
                let card_top = tops[row];
                if card_top + height < cards_area.min.y || card_top > cards_area.max.y {
                    continue;
                }
                let at = Rect::from_min_size(
                    Pos2::new(cards_area.min.x, card_top),
                    Vec2::new(cards_area.width(), height),
                );
                // The card's own press and drag, claimed before the card is drawn so the round button drawn
                // inside it is what takes a press on the button.
                let response = inner.ui.interact(
                    at,
                    inner.ui.id().with(("agent-tasks-card", task.id)),
                    egui::Sense::click_and_drag(),
                );
                let name = format!("{} {}", task.key, task.display_title());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.clone())
                });
                let (action, command) = card_action(lives[row]);
                let label = action_label(command, &task.key);
                let who = match task.assignee {
                    Assignee::Human => rux::components::Assignee::Person,
                    _ => rux::components::Assignee::Agent(agent),
                };
                let pressed = rux::components::TaskCard::new(task.display_title(), &task.key)
                    .todos(task.todo_done_count.max(0) as u32, task.todo_count.max(0) as u32)
                    .assignee(who)
                    .action(action, &label)
                    .hovered(response.hovered() || dragging == Some(task.id))
                    .chosen(ring == Some((status, row)))
                    .show(&mut inner, at);
                if pressed.action {
                    to_act = Some((command, task.key.clone()));
                } else if response.clicked() {
                    to_open = Some(task.id);
                }
                if response.drag_started() {
                    dragged = Some(task.id);
                }
            }
            if cut {
                rux.chrome.unclip();
            }
        });
        // How much of the lane is out of sight: two points of the divider down the lane's inside edge. A mark
        // rather than a control, for the reason `task-1984` §3.6 gave: four real scrollbars would take a tenth
        // of every lane's width for bars nobody drags.
        if content > cards_area.height() {
            let track = Rect::from_min_max(
                Pos2::new(lane_area.max.x - 7.0 * scale, cards_area.min.y),
                Pos2::new(lane_area.max.x - 5.0 * scale, cards_area.max.y),
            );
            let share = (cards_area.height() / content).clamp(0.08, 1.0);
            let at = down / (content - cards_area.height());
            ui.painter().rect_filled(track, 1.0, look.palette.divider);
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(
                        track.min.x,
                        track.min.y + (track.height() - track.height() * share) * at,
                    ),
                    Vec2::new(track.width(), track.height() * share),
                ),
                1.0,
                look.palette.text_faint,
            );
        }
        // Where a card being dragged would land in this lane, drawn as the accent line every drop target in
        // Unluminous is drawn as.
        if let (Some(_), Some(pointer)) = (dragging, pointer) {
            if lane_area.contains(pointer) {
                // The middle of each card, so a card goes after every card whose middle the pointer has
                // passed. `position_at` takes tops and one height; a middle with a height of nothing is that.
                let middles: Vec<f32> =
                    tops.iter().zip(heights).map(|(top, height)| top + height / 2.0).collect();
                let among_visible = arithmetic::position_at(&middles, 0.0, pointer.y);
                // The index among the cards that are **drawn**, turned into the index among all of them, so a
                // drop while a search narrows the lane does not reorder tickets nobody can see.
                let position = arithmetic::among_all(
                    snapshot.lane(status).map(|lane| lane.tasks.as_slice()).unwrap_or_default(),
                    &query,
                    among_visible,
                );
                let y = match tops.get(among_visible as usize) {
                    Some(top) => top - CARD_GAP * scale / 2.0,
                    None => pen - CARD_GAP * scale / 2.0,
                };
                ui.painter().rect_filled(
                    Rect::from_min_size(
                        Pos2::new(cards_area.min.x, y - 1.0),
                        Vec2::new(cards_area.width(), 2.0),
                    ),
                    1.0,
                    look.palette.accent,
                );
                if released {
                    dropped = Some((status, position));
                }
            }
        }
    }
    if let Some(id) = dragged {
        board.dragging = Some(id);
    }
    // How far there is left to scroll, as a thin bar along the bottom when there is any, which is the only
    // thing that says the lanes go on past the edge.
    if content > room {
        let track = Rect::from_min_max(
            Pos2::new(area.min.x + side, area.max.y - 12.0 * scale),
            Pos2::new(area.max.x - side, area.max.y - 7.0 * scale),
        );
        let share = (room / content).clamp(0.1, 1.0);
        let at = scroll / (content - room);
        let thumb = Rect::from_min_size(
            Pos2::new(track.min.x + (track.width() - track.width() * share) * at, track.min.y),
            Vec2::new(track.width() * share, track.height()),
        );
        ui.painter().rect_filled(track, 2.0, look.palette.divider);
        ui.painter().rect_filled(thumb, 2.0, look.palette.text_dim);
    }
    if let Some((status, position)) = dropped {
        let moved = board.dragging.take();
        if let Some(id) = moved {
            if let Err(problem) = board.move_card(id, status, position) {
                requests.push(Request::Message(problem));
            }
        }
    } else if released {
        board.dragging = None;
    }
    if let Some((command, key)) = to_act {
        match board.command_now(command, &[key]) {
            Ok(answer) => requests.push(Request::Message(answer.message)),
            Err(problem) => requests.push(Request::Message(problem)),
        }
        requests.push(Request::Repaint);
    }
    // Enter opens the ticket the ring is on, which is the one thing the keyboard can do beyond moving.
    if let Some(id) = open_the_chosen {
        to_open = Some(id);
    }
    if let Some(id) = to_open {
        // **Always the modal**, whatever the window's size. `components::modal` already fits a dialog to the room
        // there is and never lets it grow past the window.
        if let Err(problem) = board.open_the_modal(id) {
            requests.push(Request::Message(problem));
        }
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::agent_tasks::model::{Assignee, Priority, Source, Task};
    use crate::services::plugin_ui::Look;
    use crate::services::text_renderer::TextRenderer;
    use crate::settings::Settings;

    /// A minimal ticket, for the fields `live_for` and the heights below actually read. Every field
    /// still has to be given something, because `Task` has no `Default` — a ticket that could be
    /// half built is a ticket a caller could forget to finish.
    fn a_task(session_id: Option<&str>, assignee: Assignee) -> Task {
        Task {
            id: 1,
            key: "task-1".to_owned(),
            title: "A ticket".to_owned(),
            description: String::new(),
            priority: Priority::Medium,
            status: Status::InProgress,
            assignee,
            model: None,
            effort: None,
            epic_id: None,
            sprint_id: None,
            position: 0,
            project: None,
            session_id: session_id.map(str::to_owned),
            heartbeat_at: None,
            lease_minutes: None,
            watchdog_strikes: 0,
            watchdog_nudges: 0,
            watchdog_nudged_at: None,
            source: Source::Local,
            jira_key: None,
            jira_url: None,
            jira_status: None,
            jira_issue_type: None,
            created_at: String::new(),
            updated_at: String::new(),
            todo_count: 0,
            todo_done_count: 0,
            comment_count: 0,
        }
    }

    /// The look a test measures against: the default font, so `Look::scale()` is exactly 1.0 and the
    /// `AT_DEFAULT` constants in this file are the numbers read back.
    fn a_look(renderer: &TextRenderer) -> Look<'_> {
        Look::of(&Settings::new(), renderer)
    }

    #[test]
    fn a_run_of_cards_has_one_gap_between_each_pair_and_none_after_the_last() {
        let renderer = TextRenderer::new();
        let look = a_look(&renderer);
        assert_eq!(cards_tall(&[], &look), 0.0, "an empty lane has nothing to be tall for");
        assert_eq!(cards_tall(&[120.0], &look), 120.0, "one card, and no trailing gap");
        assert_eq!(cards_tall(&[120.0, 140.0], &look), 120.0 + CARD_GAP + 140.0);
    }

    #[test]
    fn a_running_agent_is_paused_and_a_finished_one_is_resumed() {
        use rux::components::CardAction;
        let running = Live { attached: true, can_start: false, can_resume: false };
        assert_eq!(card_action(running), (CardAction::Pause, "interrupt"));
        let fresh = Live { attached: false, can_start: true, can_resume: false };
        assert_eq!(card_action(fresh), (CardAction::Start, "start"));
        let finished = Live { attached: false, can_start: false, can_resume: true };
        assert_eq!(card_action(finished), (CardAction::Start, "resume"));
        assert_eq!(card_action(Live::default()).0, CardAction::None);
        assert_eq!(action_label("resume", "task-7"), "Resume task-7");
    }

    /// `live_for`'s `attached` half needs a real terminal session, which this board has none of; every
    /// case here is therefore the board with no terminal running, which is `attached: false` for any
    /// ticket. What is exercised is the other half: whether Start could do anything.
    #[test]
    fn a_ticket_with_no_session_can_always_be_started() {
        let board = AgentTasks::new();
        let task = a_task(None, Assignee::Claude);
        assert!(live_for(&board, &task).can_start, "nothing has claimed it yet");
    }

    #[test]
    fn a_claude_ticket_with_a_session_offers_resume_rather_than_start() {
        let board = AgentTasks::new();
        let task = a_task(Some("session-1"), Assignee::Claude);
        let live = live_for(&board, &task);
        assert!(!live.can_start, "Claude resumes the session it has, so Start would do nothing");
        assert!(live.can_resume, "and the card offers to resume it");
    }

    #[test]
    fn a_codex_ticket_whose_session_has_gone_can_be_started_again() {
        // Codex names its own sessions, so the id on the ticket is only a marker that a worker was
        // here once — there is nothing behind it to resume. See `agent::can_resume`.
        let board = AgentTasks::new();
        let task = a_task(Some("codex-session"), Assignee::Codex);
        assert!(
            live_for(&board, &task).can_start,
            "a dead Codex session can only be started afresh"
        );
    }

    #[test]
    fn a_persons_own_ticket_offers_no_start_button_at_all() {
        let board = AgentTasks::new();
        let task = a_task(Some("whatever"), Assignee::Human);
        assert!(
            !live_for(&board, &task).can_start,
            "a person is not launched in a terminal, so Start has nothing to do"
        );
    }
}
