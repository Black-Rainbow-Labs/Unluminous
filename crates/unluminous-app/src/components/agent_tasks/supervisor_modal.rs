//! The supervisor's modal: its terminal, a box a message is typed into, and `Send`.
//!
//! `task-2253`: *"We are missing a chat with supervisor button on agent tasks. when I click that, a modal should
//! open that has the terminal window, along with a message input and send button. Very similar to how our ai
//! service tasks board works."* The board being replaced opens the running supervisor's own terminal in a modal,
//! so anything typed reaches the live agent; this does the same, with the box under the terminal for a message
//! typed as a whole line and sent with Enter or `Send`. The terminal itself takes the keyboard too, when it is
//! clicked, which is how a person answers a question the agent asked with a single key.
//!
//! The modal is drawn from the window's own furniture, `components::modal`, so it is dragged, resized and zoomed
//! the way every dialog is. What it shows comes from `services::agent_tasks`, and every control here has a
//! command beside it: `plugins run agent-tasks supervisor start|stop|send|open|close`.

use egui::{Pos2, Rect, Vec2};

use crate::components::modal;
use crate::services::agent_tasks::{AgentTasks, SUPERVISOR};
use crate::services::plugin_ui::{Look, Request};

/// The modal's id, which is also what it is found by in egui's memory.
pub const MODAL_ID: &str = "agent-tasks-supervisor";
/// How large the modal asks to be.
const WIDTH: f32 = 860.0;
const HEIGHT: f32 = 600.0;
/// How tall the row holding the message box and `Send` is.
const MESSAGE_ROW: f32 = 30.0;
/// How wide `Send` is.
const SEND_WIDTH: f32 = 90.0;

/// What the modal reported: the requests for the window, and whether it was closed.
#[derive(Debug, Default)]
pub struct Outcome {
    pub requests: Vec<Request>,
    pub closed: bool,
}

/// Draw the supervisor's modal.
///
/// @param board - the board, which holds the supervisor's terminal
/// @param ctx - the window's context
/// @param look - how the board is drawn
pub fn show(board: &mut AgentTasks, ctx: &egui::Context, look: &Look<'_>) -> Outcome {
    let (inner, should_close) = modal::show(ctx, MODAL_ID, WIDTH, HEIGHT, |ui, area| {
        let mut outcome = Outcome::default();
        let running = board.supervisor_is_running();
        let title = match running {
            true => "Chat with supervisor \u{b7} running",
            false => "Chat with supervisor",
        };
        outcome.closed |= modal::header(ui, area, title);
        let body = modal::body(area);
        let terminal =
            Rect::from_min_max(body.min, Pos2::new(body.max.x, body.max.y - MESSAGE_ROW - 12.0));
        let focused = board.terminal_focused;
        match running {
            true => outcome.requests.extend(crate::components::agent_tasks::detail::terminal(
                board, ui, look, terminal, SUPERVISOR, focused,
            )),
            false => {
                modal::note(
                    ui,
                    terminal,
                    terminal.top(),
                    "The supervisor is not running. Start supervisor launches the board's agent in the board's \
                     project folder and tells it to work the tickets in QA Failed and then New, one at a time. \
                     Anything typed here is sent to it.",
                );
            }
        }
        outcome.requests.extend(message_row(board, ui, body, running));
        let toggle = match running {
            true => "Stop supervisor",
            false => "Start supervisor",
        };
        match modal::footer(ui, area, &[(toggle, true), ("Close", true)]) {
            Some(0) => {
                let said = match running {
                    true => Ok(board.stop_the_supervisor().message),
                    false => board.start_the_supervisor().map(|answer| answer.message),
                };
                outcome.requests.push(Request::Message(said.unwrap_or_else(|problem| problem)));
            }
            Some(_) => outcome.closed = true,
            None => {}
        }
        outcome
    });
    Outcome { requests: inner.requests, closed: inner.closed || should_close }
}

/// The message box and `Send`, along the bottom of the body. Enter sends too.
fn message_row(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    body: Rect,
    running: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let row = Rect::from_min_max(Pos2::new(body.min.x, body.max.y - MESSAGE_ROW), body.max);
    let field = Rect::from_min_max(row.min, Pos2::new(row.max.x - SEND_WIDTH - 10.0, row.max.y));
    let send_at = Rect::from_min_size(
        Pos2::new(row.max.x - SEND_WIDTH, row.min.y + 1.0),
        Vec2::new(SEND_WIDTH, MESSAGE_ROW - 2.0),
    );
    let mut draft = board.supervisor_draft.clone();
    let response = modal::field(ui, field, "Message to the supervisor", &mut draft);
    if response.changed() {
        board.supervisor_draft = draft.clone();
    }
    let entered = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    let written = !draft.trim().is_empty();
    let pressed = modal::button(ui, send_at, "Send", running && written, true);
    if (pressed || (entered && written)) && running {
        let said = board.send_the_supervisor_draft();
        requests.push(Request::Message(said.unwrap_or_else(|problem| problem)));
        // The box keeps the keyboard, so a second message is typed straight after the first.
        response.request_focus();
    }
    requests
}
