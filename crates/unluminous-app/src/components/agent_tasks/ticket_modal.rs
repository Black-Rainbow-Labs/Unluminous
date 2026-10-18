//! One ticket, in full: the modal the board opens a card into.
//!
//! ## The look is the Agent-Tasks design (`task-2241`)
//!
//! The Claude Design canvas "Unluminous", page Agent-Tasks, artboards "Edit task" and "New task"
//! (`TaskModal.dc.html`). One surface thirty points round on the pane's own ground, with a long soft shadow
//! under it and no title bar or footer: a header row of the ticket's key in a carved chip, the title as a field
//! and a round close button; then two columns. On the left the description in a carved well, the agent, the
//! todos with a track of how many are done, and the comments, each under a small heading. On the right
//! a raised card holding `Start Work` and the fields that say what the ticket is, each a chip with a mark and a
//! chevron, and the priority as a small pill switch. A new ticket's card ends in `Create task`.
//!
//! Every surface is `rux`: `rux::components::board` for the chips, the tick boxes and the round buttons, and
//! `Select::chip` for the fields. `components::modal::show_unframed` is the dialog, so it keeps the dragging,
//! the resizing and the zoom every other modal has.
//!
//! ## One type size, whatever the editor is
//!
//! `task-2193`: the labels were drawn from the editor's own font size and the fields were not, so on a machine
//! whose editor is set large a field's name was twice the size of what was in it. Everything here is set in
//! the design's sizes, and what Unluminous's own helpers draw (the description, the comments) is drawn with a
//! [`Look`] fixed at [`BODY`] points through `Look::at_a_fixed_size`.
//!
//! ## The heights add up
//!
//! The four sections on the left are laid out one under another. Each says what it **wants** and what it can be
//! cut to, and the shortfall is taken from them in order: the terminal, then the description down to three
//! lines, then the comments, and the todos last. The description, the comments and the todos all scroll. A
//! description somebody has dragged is cut last of all, so the drag is what decides its height (`task-2260`).
//!
//! The description, the terminal and the comments each have a grip along their bottom edge, and a height
//! dragged there is what that section then wants (`task-2214`, `task-2255`). Comments given a height of their
//! own leave whatever room is spare under them, so their bottom edge stays under the pointer. The comments' grip
//! is the strip under the section and has no corner, because the `Comment` button is in that corner; the box a
//! comment is written in has a corner mark of its own (`task-2260`).
//!
//! ## The agent is a chat first and a terminal second (`task-2261`)
//!
//! The section under the description is `Agent`, with two tabs on its heading row. The todos are under it and
//! above the comments, shut until somebody opens them, and have no field to add one: they are the plan the agent
//! writes (`task-2263`). `Chat` is the side panel's own
//! Agent-Chat pane showing the conversation the agent in the ticket's terminal is having, read from Claude
//! Code's transcript by `services::agent_tasks::ticket_chat`; what is typed there is typed into the terminal.
//! `Terminal` is the terminal itself. A ticket whose agent's transcript cannot be read opens on the terminal.
//! The round chip beside the close button puts the card of fields away and gives the left column the whole
//! width.
//!
//! ## Every field writes through one function
//!
//! The controls down the right are calls to `AgentTasks::edit_field`, so there is one place a column is written.
//! `Model` and `Effort` are **absent** for a ticket assigned to a person rather than disabled, which is
//! Unluminous's rule.

use egui::{Pos2, Rect, Vec2};
use rux::components::{ChipButton, PillSwitch, Select, SelectLead};
use rux::icon::{Icon, Mark};
use rux::text::Style;

use crate::components::controls::TextBoxes;
use crate::components::modal;
use crate::services::agent_tasks::model::{Assignee, Priority, Status, Task};
use crate::services::agent_tasks::ticket_chat::{self, AgentTab};
use crate::services::agent_tasks::{clock, AgentTasks, Field, TicketKit, EFFORTS};
use crate::services::plugin_ui::{Look, Request};

/// The id the modal is drawn under, which is what its place and its zoom are kept against.
pub const MODAL_ID: &str = "agent-tasks-ticket";

/// The type size the description and the comments are set in: the design's 13 points.
pub const BODY: f32 = 13.0;

/// The smallest it will ask for, whatever the window is.
const SMALLEST_WIDTH: f32 = 720.0;
const SMALLEST_HEIGHT: f32 = 520.0;

/// How big the modal asks to be in this window: the window less `modal::MARGIN_SHARE` of it on every side.
///
/// `task-2280`: *"The default modal size is too small ... It should fill most of the screen. Use margin of 3%."*
/// It was nine tenths of the window and never more than 1000 by 860 points, so on a large screen a ticket
/// was a dialog in the middle of a mostly empty window. It is now the size every working dialog opens at,
/// `modal::starting_size`, and the description takes the room it gains.
pub fn size(ctx: &egui::Context, _look: &Look<'_>) -> (f32, f32) {
    let window = ctx.content_rect().size();
    let size = modal::starting_size(window, MODAL_ID, Vec2::new(SMALLEST_WIDTH, SMALLEST_HEIGHT));
    (size.x, size.y)
}

/// `padding: 22px 24px 24px 28px` round the dialog, and the header row's height and the gap under it.
const PAD_TOP: f32 = 22.0;
const PAD_RIGHT: f32 = 24.0;
const PAD_BOTTOM: f32 = 24.0;
const PAD_LEFT: f32 = 28.0;
const HEADER: f32 = 36.0;
const HEADER_GAP: f32 = 22.0;
/// How round the dialog is.
const RADIUS: f32 = 30.0;
/// How wide the card of fields down the right is, and the dialog width below which it goes underneath.
const ASIDE: f32 = 270.0;
const TWO_COLUMNS: f32 = 700.0;
/// Between the two columns, between one section and the next, and under a section's heading.
const GUTTER: f32 = 28.0;
const SECTION_GAP: f32 = 20.0;
const LABEL_GAP: f32 = 10.0;
/// A section's heading: 12 point semibold words.
const LABEL: f32 = 15.0;
/// The todos' heading row, which holds the 24 point fold disc.
const TODO_HEAD: f32 = 24.0;
/// One todo row, the gap between two, and the list's padding.
const TODO_ROW: f32 = 36.0;
const TODO_GAP: f32 = 4.0;
const TODO_PAD: f32 = 8.0;
/// The description's well when nobody has dragged its edge, and the least it can be made.
const DESCRIPTION: f32 = 92.0;
const DESCRIPTION_LEAST: f32 = 76.0;
/// The smallest a description can be dragged to: one line of words and the well's own padding. `task-2277`: *"I
/// should be able to resize it to be 1-2 lines tall, if I so desire."* [`DESCRIPTION_LEAST`] stays what the budget
/// cuts an undragged description to.
pub const DESCRIPTION_SMALLEST: f32 = 46.0;
/// How tall the strip under the description that resizes it is.
const DESCRIPTION_GRIP: f32 = 8.0;
/// The terminal: the strip that says there is none, and the room a live one wants.
const TERMINAL_EMPTY: f32 = 58.0;
const TERMINAL_SHARE: f32 = 0.34;
const TERMINAL_SMALLEST: f32 = 150.0;
const TERMINAL_LEAST: f32 = 64.0;
const TERMINAL_LARGEST: f32 = 420.0;
/// The chat wants more of the column than the terminal did, because it is where the agent is read.
const CHAT_SHARE: f32 = 0.45;
const CHAT_SMALLEST: f32 = 220.0;
const CHAT_LARGEST: f32 = 560.0;
/// The least the chat is cut to: a few rows of the conversation and its composer.
const CHAT_LEAST: f32 = 200.0;
/// The tab switch on the Agent heading: each tab 18 points tall in a track 26 tall.
const TAB_HEIGHT: f32 = 18.0;
const TAB_PAD: f32 = 10.0;
/// The least the comments can be dragged to: the box that writes one and a little of the first comment.
const COMMENTS_LEAST: f32 = 90.0;
/// What the comments want and the least they can be given: the box that writes one and about one comment.
const COMMENTS: f32 = 260.0;
const COMMENT_ROUGHLY: f32 = 56.0;

/// What the modal reported.
#[derive(Debug, Default)]
pub struct Outcome {
    pub requests: Vec<Request>,
    /// The modal was closed, by its cross, by `Escape`, by a click outside it, or by `Create task`.
    pub closed: bool,
}

/// Draw the ticket that is open in the detail, as a modal. Does nothing when none is.
pub fn show(board: &mut AgentTasks, ctx: &egui::Context, look: &Look<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let Some(task) = board.detail().task.clone() else {
        return outcome;
    };
    // A ticket nobody has named yet is a new one: `Add Task` created the row before anybody typed.
    let new = board.detail().is_new;
    let (width, height) = size(ctx, look);
    let look = look.clone().at_a_fixed_size(BODY).flat();
    // Taken out of the board for the length of the drawing, because the drawing needs the board too.
    let mut kit = board.ticket_kit.take().unwrap_or_default();
    // **`Tab` walks the text boxes, and nothing else.** `task-2214`. egui's own walk visits every widget that
    // can take the keyboard, and in this modal that is mostly ones that draw nothing to say they have it. So
    // egui's walk is cancelled here, before any widget in the modal has been drawn, and the box to move to is
    // chosen once every box has said where it is.
    let tab = ctx.input(|input| {
        let pressed = input.key_pressed(egui::Key::Tab)
            && !input.modifiers.command
            && !input.modifiers.alt
            && !input.modifiers.ctrl;
        pressed.then_some(input.modifiers.shift)
    });
    if tab.is_some() {
        ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    }
    kit.tab_order.clear();
    let (inner, should_close) = modal::show_unframed(ctx, MODAL_ID, width, height, |ui, area| {
        // **The agent's terminal is drawn by Unluminous's own glyph engine**, which `crisp` does not reach, so it
        // is told the transform the modal is drawn through. `task-2198`, `task-2216`.
        let transform = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
        let was = look.renderer.composite_through(transform, true);
        let drawn = contents(board, &mut kit, ui, area, &look, &task, new);
        look.renderer.restore_compositing(was);
        drawn
    });
    // **Once a frame, however many passes it takes.** egui draws a frame a second time when a layout changes
    // under it, and hands the second pass the same input, so one press of `Tab` moved the keyboard two boxes.
    let frame = ctx.cumulative_frame_nr();
    let tab = tab.filter(|_| kit.tab_frame != Some(frame));
    if tab.is_some() {
        kit.tab_frame = Some(frame);
    }
    if let Some(backwards) = tab {
        let focused = ctx.memory(|memory| memory.focused());
        if let Some(next) = the_next_box(&kit.tab_order, focused, backwards) {
            ctx.memory_mut(|memory| memory.request_focus(next));
        }
    }
    kit.rux.end_frame();
    board.ticket_kit = Some(kit);
    outcome.requests = inner.requests;
    outcome.closed = inner.closed || should_close;
    // A new ticket closed with nothing typed into it is thrown away, because `Add Task` made the row before
    // anybody asked for one. One with a title is kept, which is what `Create task` does too.
    if outcome.closed && new && board.detail().title_draft.trim().is_empty() {
        if let Err(problem) = board.discard_the_ticket() {
            outcome.requests.push(Request::Message(problem));
        }
    }
    outcome
}

/// Which text box `Tab` moves the keyboard to, given the boxes in the order they were drawn.
///
/// The one after the box that has the keyboard, or the one before it when `Shift` is held, going round at
/// either end. With the keyboard anywhere else in the modal, `Tab` goes to the first box and `Shift+Tab` to the
/// last, which is what a browser does when nothing in a form has been focused yet.
pub fn the_next_box(
    order: &[egui::Id],
    focused: Option<egui::Id>,
    backwards: bool,
) -> Option<egui::Id> {
    if order.is_empty() {
        return None;
    }
    let last = order.len() - 1;
    let at = focused.and_then(|id| order.iter().position(|known| *known == id));
    let next = match (at, backwards) {
        (None, false) => 0,
        (None, true) => last,
        (Some(at), false) => match at == last {
            true => 0,
            false => at + 1,
        },
        (Some(at), true) => match at {
            0 => last,
            at => at - 1,
        },
    };
    order.get(next).copied()
}

/// The dialog's surface, its header row, and the two columns under it.
fn contents(
    board: &mut AgentTasks,
    kit: &mut TicketKit,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    task: &Task,
    new: bool,
) -> Outcome {
    let mut outcome = Outcome::default();
    let state = &kit.rux;
    crate::theme::in_step(state);
    let selects = &mut kit.selects;
    let order = &mut kit.tab_order;
    let main_id = ui.id().with("agent-tasks-ticket-rux");
    let (requests, closed) = rux::layer(
        ui,
        state,
        main_id,
        area.expand(crate::components::modal::SHADOW_REACH),
        |rux| {
            let chat = rux.theme().chat;
            rux.chrome.surface(area, RADIUS, chat.ground, crate::components::modal::shadow());
            let mut requests = Vec::new();
            let inner = Rect::from_min_max(
                Pos2::new(area.min.x + PAD_LEFT, area.min.y + PAD_TOP),
                Pos2::new(area.max.x - PAD_RIGHT, area.max.y - PAD_BOTTOM),
            );
            let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER));
            let (closed, said) = header_row(board, rux, order, header, task, new);
            requests.extend(said);
            let body =
                Rect::from_min_max(Pos2::new(inner.min.x, header.max.y + HEADER_GAP), inner.max);
            if board.fields_shut {
                // The card of fields is put away, so the column takes the whole width (`task-2261`). A new ticket
                // keeps `Create task`, which is on that card, so it is drawn under the column instead.
                let foot = match new {
                    true => 44.0 + SECTION_GAP,
                    false => 0.0,
                };
                let main = Rect::from_min_max(body.min, Pos2::new(body.max.x, body.max.y - foot));
                requests.extend(main_column(board, rux, order, main, look, task, new));
                let pressed = new && {
                    let at = Rect::from_min_size(
                        Pos2::new(body.max.x - ASIDE, body.max.y - 44.0),
                        Vec2::new(ASIDE, 44.0),
                    );
                    action_pill(rux, at, "Create task", Icon::Plus, true).clicked()
                };
                let create = pressed && press_create(board, rux.ui, &mut requests);
                (requests, closed || create)
            } else if body.width() >= TWO_COLUMNS {
                let aside = Rect::from_min_max(Pos2::new(body.max.x - ASIDE, body.min.y), body.max);
                let main =
                    Rect::from_min_max(body.min, Pos2::new(aside.min.x - GUTTER, body.max.y));
                requests.extend(main_column(board, rux, order, main, look, task, new));
                let (more, pressed) = fields(board, rux, selects, aside, task, new);
                requests.extend(more);
                let create = pressed && press_create(board, rux.ui, &mut requests);
                (requests, closed || create)
            } else {
                // One column: the card of fields first and never more than half the height, scrolled inside it,
                // then the rest, which is what the browser board's own narrow layout does.
                let aside = Rect::from_min_max(
                    body.min,
                    Pos2::new(body.max.x, body.min.y + (body.height() * 0.45).min(330.0)),
                );
                let rest =
                    Rect::from_min_max(Pos2::new(body.min.x, aside.max.y + SECTION_GAP), body.max);
                let mut column = Vec::new();
                let (more, pressed) = fields(board, rux, selects, aside, task, new);
                requests.extend(more);
                let create = pressed && press_create(board, rux.ui, &mut requests);
                if rest.height() > 120.0 {
                    requests.extend(main_column(board, rux, &mut column, rest, look, task, new));
                }
                order.splice(0..0, column);
                (requests, closed || create)
            }
        },
    );
    outcome.requests.extend(requests);
    outcome.closed |= closed;
    // **The command key with Enter closes it**, not Enter on its own: the description and the comment box are
    // multiline fields where Enter is a new line, which is the commit panel's reason for the same choice.
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command_only())
    {
        outcome.closed = true;
    }
    outcome
}

/// What `Create task` does, and whether the modal closes because of it.
///
/// `task-2263`. A ticket with no title stays open with its title field outlined in red and the keyboard put in
/// it; any other failure is a notice that goes by itself after three seconds, because the person is looking at
/// the dialog they pressed the button in.
fn press_create(board: &mut AgentTasks, ui: &egui::Ui, requests: &mut Vec<Request>) -> bool {
    match board.create_the_new_ticket() {
        Ok(_) => true,
        Err(_) if board.detail().title_missing => {
            ui.ctx().memory_mut(|memory| {
                memory.request_focus(ui.id().with("agent-tasks-ticket-title"));
            });
            false
        }
        Err(problem) => {
            requests.push(not_created(&problem));
            false
        }
    }
}

/// The notice for a ticket that could not be created: red, with a cross, gone after three seconds.
pub fn not_created(problem: &str) -> Request {
    Request::Notice {
        text: format!("The ticket was not created: {problem}"),
        kind: crate::components::toast::Kind::Brief,
    }
}

/// The key in a carved chip, the title as a field, and the round close button. Answers whether it was closed.
fn header_row(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    order: &mut Vec<egui::Id>,
    area: Rect,
    task: &Task,
    new: bool,
) -> (bool, Vec<Request>) {
    let mut requests = Vec::new();
    let chat = rux.theme().chat;
    let theme = rux.theme();
    // `padding: 7px 12px` round the key in the code face, or `New task` in the ordinary one.
    let (said, style) = match new {
        true => ("New task".to_owned(), Style::sans(12.0)),
        false => (task.key.clone(), Style::mono(12.0)),
    };
    let galley = rux.text(style, &said, theme.ink.i500);
    let chip = Rect::from_min_size(
        Pos2::new(area.min.x, area.center().y - (galley.size().y + 14.0) / 2.0),
        Vec2::new(galley.size().x + 24.0, galley.size().y + 14.0),
    );
    rux.chrome.surface(chip, chip.height() / 2.0, chat.well, chat.carved_sm());
    rux::text::draw_centred(rux.painter(), chip, galley, theme.ink.i500);
    let close =
        Rect::from_center_size(Pos2::new(area.max.x - 18.0, area.center().y), Vec2::splat(36.0));
    let closed =
        ChipButton::new(Icon::X, 36.0, "Close").mark(12.0, Some(1.8)).show(rux, close).clicked();
    // **The card of fields folds away** (`task-2261`: *"the right side panel should be collapsable so i can
    // maximize usage of the modal for description etc."*). A chevron pointing at the card puts it away; pointing
    // back, it brings it back.
    let fold = close.translate(Vec2::new(-(36.0 + 10.0), 0.0));
    let shut = board.fields_shut;
    let name = match shut {
        true => "Show the fields",
        false => "Hide the fields",
    };
    let turn = match shut {
        true => std::f32::consts::PI,
        false => 0.0,
    };
    if ChipButton::new(Icon::ChevRight, 36.0, name)
        .mark(11.0, Some(1.8))
        .turn(turn)
        .show(rux, fold)
        .clicked()
    {
        board.fields_shut = !shut;
    }
    // The title, typed into in place: 20 point semibold, with no frame round it, saved as it is typed.
    let font = Style::sans(20.0).semibold().tracking(-0.015).font();
    let field = Rect::from_min_max(
        Pos2::new(chip.max.x + 16.0, area.min.y),
        Pos2::new(fold.min.x - 16.0, area.max.y),
    );
    let id = rux.ui.id().with("agent-tasks-ticket-title");
    let text = crate::components::controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        field,
        0.0,
        id,
        "Ticket title field",
        &font,
    );
    let mut title = board.detail().title_draft.clone();
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = child.text_box(
        egui::TextEdit::singleline(&mut title)
            .id(id)
            .hint_text(crate::components::controls::placeholder(
                "Task title",
                &font,
                theme.ink.i400,
            ))
            .font(font.clone())
            .frame(egui::Frame::NONE)
            .desired_width(text.width())
            .text_color(theme.ink.i900),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Ticket title")
    });
    // In the `Tab` walk for a new ticket, where the title is the first thing to type. A ticket that exists is
    // opened to work on, so its walk starts at the description.
    if new {
        order.push(id);
    }
    if response.changed() {
        board.detail_mut().title_draft = title;
        if let Err(problem) = board.save_the_title() {
            requests.push(match new {
                true => not_created(&problem),
                false => Request::Message(problem),
            });
        }
    }
    super::enter_was_used(rux.ui, &response);
    if board.detail().title_missing {
        title_is_missing(rux, field);
    }
    (closed, requests)
}

/// The title field outlined in the design's failed colour, with the sentence that says why under it.
///
/// `task-2263`. The outline is a 1.5 point round rectangle a little outside the field, and the sentence sits in
/// the gap between the header row and the columns, so nothing moves when it appears.
fn title_is_missing(rux: &mut rux::Rux<'_>, field: Rect) {
    let red = rux.theme().chat.slower;
    let ring = field.expand2(Vec2::new(8.0, 2.0));
    rux.painter().rect_stroke(
        ring,
        ring.height() / 2.0,
        egui::Stroke::new(1.5, red),
        egui::StrokeKind::Outside,
    );
    let said = crate::services::agent_tasks::TITLE_MISSING;
    let galley = rux.text(Style::sans(11.5), said, red);
    let at = Pos2::new(ring.min.x + 12.0, ring.max.y + 4.0);
    let size = galley.size();
    rux.painter().galley(at, galley, red);
    let response = rux.ui.interact(
        Rect::from_min_size(at, size),
        rux.ui.id().with("agent-tasks-title-missing"),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, said));
}

/// A section's heading: the design's 12 point semibold words in the quieter ink, named so a test finds it.
fn label(rux: &mut rux::Rux<'_>, at: Pos2, name: &str) -> f32 {
    let theme = rux.theme();
    let style = Style::sans(12.0).semibold();
    let galley = rux.text(style, name, theme.ink.i500);
    let size = galley.size();
    rux.painter().galley(at, galley, theme.ink.i500);
    let response = rux.ui.interact(
        Rect::from_min_size(at, size),
        rux.ui.id().with(("agent-tasks-heading", name)),
        egui::Sense::hover(),
    );
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, named.clone()));
    size.x
}

/// How much room each section of the left column is given, worked out before any of it is drawn.
struct Budget {
    description: f32,
    todos: f32,
    terminal: f32,
    comments: f32,
}

/// Share the column's height out, taking the shortfall from the terminal, then the description down to three
/// lines, then the comments, and the todos last, because the todos are the plan an agent is working through.
fn budget(board: &AgentTasks, _rux: &rux::Rux<'_>, area: Rect, task: &Task, new: bool) -> Budget {
    let room = area.height();
    // A new ticket has no todos to show: they are written by the agent working it (`task-2263`).
    let todos_open = board.todos_open && !new;
    let rows = board.detail().todos.len() as f32;
    let list = |rows: f32| TODO_PAD * 2.0 + rows * (TODO_ROW + TODO_GAP) + 2.0;
    let (todo_want, todo_least) = match todos_open && rows > 0.0 {
        true => (list(rows.min(5.0)), list(rows.min(1.0))),
        false => (0.0, 0.0),
    };
    let chatting = board.agent_tab_for(task) == AgentTab::Chat && board.ticket_chat.is_some();
    let live = board.terminal_for(task.id).is_some() || chatting;
    let terminal_open = !board.terminal_shut;
    let (share, smallest, largest) = match chatting {
        true => (CHAT_SHARE, CHAT_SMALLEST, CHAT_LARGEST),
        false => (TERMINAL_SHARE, TERMINAL_SMALLEST, TERMINAL_LARGEST),
    };
    let (terminal_want, terminal_least) = match (new, live, terminal_open) {
        (true, _, _) | (false, _, false) => (0.0, 0.0),
        (false, true, true) => (
            board
                .terminal_height
                .map(|height| height.max(TERMINAL_LEAST))
                .unwrap_or_else(|| (room * share).clamp(smallest, largest)),
            match chatting {
                true => CHAT_LEAST.min(board.terminal_height.unwrap_or(CHAT_LEAST)),
                false => TERMINAL_LEAST,
            },
        ),
        (false, false, true) => (TERMINAL_EMPTY, TERMINAL_EMPTY),
    };
    let composer = super::detail::composer_room(board) + DESCRIPTION_GRIP;
    let (comment_want, comment_least) = match new || board.comments_shut {
        true => (0.0, 0.0),
        false => {
            let used = board.detail().comments.len() as f32 * COMMENT_ROUGHLY + composer;
            let least = (composer + COMMENT_ROUGHLY).min(used);
            // The comments always want room for the composer, so a comment box dragged taller takes its room from
            // the sections above rather than from the comments list alone (`task-2260`).
            match board.comments_height {
                Some(height) => {
                    let want = height.max(COMMENTS_LEAST).max(composer);
                    (want, least.min(want))
                }
                None => (COMMENTS.min(used).max(composer), least),
            }
        }
    };
    // Every heading of a ticket that exists is a fold, as tall as its round chevron (`task-2269`).
    let headings = match new {
        true => LABEL + LABEL_GAP,
        false => (TODO_HEAD + LABEL_GAP) * 4.0 + SECTION_GAP * 3.0 + 2.0,
    };
    let description_shut = board.description_shut && !new;
    let description_least = match (description_shut, board.description_height) {
        (true, _) => 0.0,
        (false, Some(dragged)) => DESCRIPTION_LEAST.min(dragged.max(DESCRIPTION_SMALLEST)),
        (false, None) => DESCRIPTION_LEAST,
    };
    let description_want = match (new, description_shut) {
        (true, _) => (room - headings - todo_want).max(DESCRIPTION_LEAST),
        (false, true) => 0.0,
        (false, false) => match board.description_height {
            Some(dragged) => dragged.max(DESCRIPTION_SMALLEST),
            None => DESCRIPTION.max(DESCRIPTION_LEAST),
        },
    };
    let wanted = headings + description_want + todo_want + terminal_want + comment_want;
    let mut short = (wanted - room).max(0.0);
    let spare = (room - wanted).max(0.0);
    let give = |want: f32, least: f32, short: &mut f32| -> f32 {
        let spare = (want - least).max(0.0).min(*short);
        *short -= spare;
        want - spare
    };
    // **A description somebody dragged is cut after the comments and the todos rather than before them**
    // (`task-2260`). Cut second, a drag that made it taller in a dialog with no room to spare was taken straight
    // back by this budget, so the edge did not move and the resize looked as though it did nothing.
    let dragged = board.description_height.is_some();
    // **The chat is where the agent is read, so it keeps its room before the comments do** (`task-2261`). The
    // terminal is cut first, as it always was; the chat is cut after the comments and the description.
    let comments_first = chatting.then(|| give(comment_want, comment_least, &mut short));
    let description_first =
        (chatting && !dragged).then(|| give(description_want, description_least, &mut short));
    let terminal = give(terminal_want, terminal_least, &mut short);
    let description = match (dragged, description_first) {
        (true, _) => description_want,
        (false, Some(given)) => given,
        (false, None) => give(description_want, description_least, &mut short),
    };
    // **Spare room goes to the description first, up to the height of what is written in it** (`task-2280`: *"The
    // contents of the modal should fill the space when possible. e.g. the description of task in agent task
    // should grow to help the contents fit."*). A modal that fills the window has room to show a long
    // description whole, and the comments have only their composer to show until somebody comments.
    let grows = !dragged && !description_shut && !new;
    let to_description = match grows {
        true => (description_fits(_rux, board, area.width()) - description).clamp(0.0, spare),
        false => 0.0,
    };
    let description = description + to_description;
    let spare = spare - to_description;
    // Spare room goes to the comments, unless somebody has given them a height of their own: then it stays under
    // them, so the edge they were dragged by is where the pointer let go of it.
    let spare = match board.comments_height {
        Some(_) => 0.0,
        None => spare,
    };
    let comments = match comments_first {
        Some(given) => given + spare,
        None => give(comment_want, comment_least, &mut short) + spare,
    };
    let todos = give(todo_want, todo_least, &mut short);
    // When every section is at its least and the dialog is still short, the others go below theirs before the
    // description does, and whatever is still short comes off the description, which scrolls.
    // With the chat showing, the comments and the todos go below theirs before the chat does.
    let (terminal, comments, todos) = match chatting {
        true => {
            // The todos keep their least, one row, because the plan is what the chat is read against.
            let comments = give(comments, composer.min(comments), &mut short);
            (give(terminal, 0.0, &mut short), comments, todos)
        }
        false => {
            let terminal = give(terminal, 0.0, &mut short);
            let comments = give(comments, composer.min(comments), &mut short);
            (terminal, comments, give(todos, list(1.0).min(todos), &mut short))
        }
    };
    let description = match dragged {
        true => give(description, description_least, &mut short),
        false => description,
    };
    let description = (description - short).max(0.0);
    // Room left over by the two rules below. It goes to the description, which scrolls, unless somebody dragged
    // the description to a height of its own: then the description stays where the pointer let go of it and the
    // room goes to the comments, or to the agent's section when the comments are folded (`task-2277`). Given to
    // the description, a drag smaller was handed straight back, so it stopped well short of one or two lines.
    let mut left_over = 0.0;
    // **The strip that says there is no terminal is whole or absent** (`task-2260`). Its words are two lines tall,
    // so cut below its height they ran over its own edges.
    let terminal = match !live && terminal > 0.0 && terminal < TERMINAL_EMPTY {
        true => {
            left_over += terminal;
            0.0
        }
        false => terminal,
    };
    // **The todos are given whole rows**: a row cut through its middle reads as a drawing fault rather than as a
    // list that scrolls.
    let todos = match todos_open && rows > 0.0 && todos > 0.0 {
        true => {
            let fit = ((todos - list(0.0)) / (TODO_ROW + TODO_GAP)).floor().clamp(0.0, rows);
            let whole = list(fit).min(todos);
            left_over += todos - whole;
            whole
        }
        false => todos,
    };
    // **A shut section gives its room to the other of the two that take what is spare**, the comments or the
    // description, so the dialog does not end in an empty band (`task-2269`).
    let comments_shut = board.comments_shut && !new;
    let (description, comments) = match (description_shut, comments_shut) {
        (true, false) => (0.0, comments + description + left_over),
        (true, true) => (0.0, 0.0),
        (false, false) if dragged => (description, comments + left_over),
        (false, true) if dragged && live && terminal > 0.0 => {
            return Budget {
                description,
                todos,
                terminal: terminal + comments + left_over,
                comments: 0.0,
            };
        }
        (false, true) if dragged => (description, 0.0),
        (false, true) => (description + comments + left_over, 0.0),
        (false, false) => (description + left_over, comments),
    };
    Budget { description, todos, terminal, comments }
}

/// How tall the description's well has to be to show all of its words without scrolling: the words laid out at
/// the width the well gives them, and the well's own padding above and below.
fn description_fits(rux: &rux::Rux<'_>, board: &AgentTasks, width: f32) -> f32 {
    let text = board.description_text();
    let wrap = (width - 18.0 - 32.0).max(1.0);
    let font = egui::FontId::proportional(BODY);
    let words =
        rux.ui.ctx().fonts_mut(|fonts| fonts.layout(text, font, egui::Color32::WHITE, wrap)); // any ground: only measured, never drawn
    words.size().y + 28.0 + DESCRIPTION_GRIP
}

/// A section's heading that folds: the round chevron before it, named `name`, and `words` beside it. Answers
/// whether the chevron was pressed, and where the words end so a control can go after them.
///
/// The Agent and Todos headings' own shape, for the description and the comments (`task-2269`). The words are
/// a heading of their own when `labelled`, so a test can find them by what they say.
fn fold_heading(
    rux: &mut rux::Rux<'_>,
    at: Pos2,
    name: &str,
    words: &str,
    open: bool,
    labelled: bool,
) -> (bool, f32) {
    let disc = Rect::from_min_size(at, Vec2::splat(TODO_HEAD));
    let turn = match open {
        true => std::f32::consts::PI,
        false => 0.0,
    };
    let pressed = ChipButton::new(Icon::ChevDown, TODO_HEAD, name)
        .mark(10.0, Some(1.6))
        .turn(turn)
        .folds(open)
        .show(rux, disc)
        .clicked();
    let left = disc.max.x + 10.0;
    let width = match labelled {
        true => {
            let top = disc.center().y - LABEL / 2.0 - 1.0;
            label(rux, Pos2::new(left, top), words)
        }
        false => {
            let theme = rux.theme();
            let style = Style::sans(12.0).semibold();
            let galley = rux.text(style, words, theme.ink.i500);
            let width = galley.size().x;
            rux::text::draw_left_capitals(
                rux.painter(),
                Pos2::new(left, disc.center().y),
                galley,
                style,
                theme.ink.i500,
            );
            width
        }
    };
    (pressed, left + width)
}

/// How wide the description's microphone is.
const MIC: f32 = 24.0;

/// The description's microphone: a round chip that is carved in while dictating, with a ring that follows how
/// loud the microphone is, and the chat prompt's own mark. Answers whether it was pressed.
fn description_microphone(
    rux: &mut rux::Rux<'_>,
    mic: Rect,
    voice: &crate::services::plugin_ui::VoiceView,
) -> bool {
    use crate::components::controls::WithHint;
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    let response = rux.ui.interact(mic, rux.ui.id().with("agent-tasks-microphone"), egui::Sense::click());
    let hovered = response.hovered();
    match voice.dictating || response.is_pointer_button_down_on() {
        true => rux.chrome.surface(mic, MIC / 2.0, chat.well, chat.carved_sm()),
        false => rux.chrome.surface(
            mic,
            MIC / 2.0,
            rux::Fill::gradient(chat.chip, mic),
            chat.raised_chip(),
        ),
    }
    let tint = match (&voice.refused, voice.dictating, hovered) {
        (Some(_), _, _) => ink.i300,
        (None, true, _) => chat.sky,
        (None, false, true) => chat.sky,
        (None, false, false) => ink.i500,
    };
    if voice.dictating {
        let strength = (0.35 + voice.level * 6.0).min(1.0);
        rux.painter().circle_stroke(
            mic.center(),
            MIC / 2.0 + 1.5,
            egui::Stroke::new(1.5, chat.sky.gamma_multiply(strength)),
        );
    }
    if hovered {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    crate::theme::icon::scaled(rux.painter(), mic.center(), tint, 0.9, crate::theme::icon::microphone);
    let hint = match (&voice.refused, voice.dictating) {
        (Some(why), _) => format!("The microphone cannot be used: {why}"),
        (None, true) => "Stop dictating".to_owned(),
        (None, false) => "Dictate: speak and the words fill the description".to_owned(),
    };
    let response = response.with_hint(hint);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, voice.dictating, "Dictate description")
    });
    response.clicked()
}

/// The description, the todos, the terminal and the comments.
fn main_column(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    order: &mut Vec<egui::Id>,
    area: Rect,
    look: &Look<'_>,
    task: &Task,
    new: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let theme = rux.theme();
    let chat = theme.chat;
    let budget = budget(board, rux, area, task, new);
    let mut pen = area.min.y;

    // ------------------------------------------------------------------ the description
    // A ticket that exists folds its description, as it folds its agent and its todos (`task-2269`). A new ticket
    // is being written, so its description is always open.
    let description_open = new || !board.description_shut;
    let (row, heading_right, heading_middle) = match new {
        true => {
            let width = label(rux, Pos2::new(area.min.x, pen), "Description");
            (LABEL, area.min.x + width, pen + LABEL / 2.0)
        }
        false => {
            let (pressed, right) = fold_heading(
                rux,
                Pos2::new(area.min.x, pen),
                "Description",
                "Description",
                description_open,
                false,
            );
            if pressed {
                board.description_shut = description_open;
            }
            (TODO_HEAD, right, pen + TODO_HEAD / 2.0)
        }
    };
    // **The microphone, after the heading** (`task-2293`: *"add a mic button to the top left of description in
    // task modal ... similar to agent chat"*). The same dictation as the chat's prompt, with the same settings and
    // model, filling the description as it is spoken. Absent while `voice.button` is off, as the chat's is.
    if description_open && board.voice.button {
        let mic = Rect::from_center_size(
            Pos2::new(heading_right + 8.0 + MIC / 2.0, heading_middle),
            Vec2::splat(MIC),
        );
        if description_microphone(rux, mic, &board.voice) {
            requests.push(AgentTasks::dictation_request(!board.voice.dictating));
        }
    }
    // The two view buttons, on the heading's own row and right aligned, which is where a section's controls go.
    let buttons_top = match new {
        true => pen - 2.0,
        false => pen + (TODO_HEAD - 18.0) / 2.0,
    };
    if description_open {
        if let Some(rendered) = super::raw_or_rendered(
            rux.ui,
            look,
            Rect::from_min_size(Pos2::new(area.min.x, buttons_top), Vec2::new(area.width(), 18.0)),
            "the description",
            board.detail().description_rendered,
        ) {
            board.show_the_description_rendered(rendered);
        }
    }
    pen += row + LABEL_GAP;
    let well = Rect::from_min_size(
        Pos2::new(area.min.x, pen),
        Vec2::new(area.width(), budget.description),
    );
    if description_open && well.height() > 24.0 {
        rux.chrome.surface(well, 20.0, chat.well, chat.carved_sm());
        if !board.detail().description_rendered {
            order.push(rux.ui.id().with("agent-tasks-description"));
        }
        // `padding: 14px 32px 14px 18px`: the right hand room is where the resize mark sits.
        let inside = Rect::from_min_max(
            Pos2::new(well.min.x + 18.0, well.min.y + 14.0),
            Pos2::new(well.max.x - 32.0, well.max.y - 14.0),
        );
        requests.extend(super::description::in_a_well(
            board,
            rux.ui,
            well,
            inside,
            look,
            theme.ink.i900,
        ));
        let grip =
            Grip { salt: "agent-tasks-description-grip", name: "Resize description", corner: true };
        if let Some(height) =
            section_grip(rux, well, DESCRIPTION_SMALLEST, area.max.y - well.min.y, grip)
        {
            board.description_height = height;
        }
    }
    pen = well.max.y + SECTION_GAP;

    // A new ticket is its title, its description and its fields. Its todos are written by the agent that works
    // it, and the agent and the comments come once it exists (`task-2263`).
    if new {
        return requests;
    }

    // ------------------------------------------------------------------ the agent: its chat and its terminal
    let tab = board.agent_tab_for(task);
    let live = board.terminal_for(task.id).is_some();
    let attached =
        board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
    let title = match attached {
        true => format!("Agent \u{b7} live \u{b7} {}", task.key),
        false => "Agent".to_owned(),
    };
    // **The terminal folds**, as the todos do, and with the same round disc before its heading: a ticket whose
    // agent has written a screenful is unreadable until it is put away. The design draws the fold for the todos
    // only; the terminal has it too because it is the section that most needs it.
    let open = !board.terminal_shut;
    let disc = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::splat(TODO_HEAD));
    let turn = match open {
        true => std::f32::consts::PI,
        false => 0.0,
    };
    if ChipButton::new(Icon::ChevDown, TODO_HEAD, "Agent")
        .mark(10.0, Some(1.6))
        .turn(turn)
        .folds(open)
        .show(rux, disc)
        .clicked()
    {
        board.terminal_shut = open;
    }
    let style = Style::sans(12.0).semibold();
    let words = rux.text(style, &title, theme.ink.i500);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(disc.max.x + 10.0, disc.center().y),
        words,
        style,
        theme.ink.i500,
    );
    // The two tabs, at the right hand end of the heading row, which is where a section's controls go.
    let (chosen, tabs_left) = agent_tabs(rux, Pos2::new(area.max.x, disc.center().y), tab);
    if let Some(chosen) = chosen {
        board.agent_tab = Some(chosen);
    }
    let tab = board.agent_tab_for(task);
    // `To Side Pane`, before the tabs while the Chat tab shows a conversation (`task-2269`): the same agent and the
    // same conversation in the Agent-Chat side pane, with the modal closed so the files can be seen beside it.
    if tab == AgentTab::Chat && board.ticket_chat.is_some() {
        let pill = rux::components::PillButton::new(TO_SIDE_PANE).mark(Icon::ArrowRight);
        let width = pill.measure(rux).x;
        let at = Rect::from_center_size(
            Pos2::new(tabs_left - 10.0 - width / 2.0, disc.center().y),
            Vec2::new(width, TAB_HEIGHT + 8.0),
        );
        let words_end = disc.max.x + 10.0 + rux.measure(style, &title).x;
        if at.left() > words_end + 8.0 && pill.show(rux, at).clicked() {
            let key = task.key.clone();
            match board.to_the_side_pane(&key) {
                Ok(said) => requests.push(Request::Message(said.message)),
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
    }
    pen += TODO_HEAD + LABEL_GAP;
    let chatting = tab == AgentTab::Chat && board.ticket_chat.is_some();
    let shown = live || chatting;
    if open && budget.terminal > 24.0 {
        let at = Rect::from_min_size(
            Pos2::new(area.min.x, pen),
            Vec2::new(area.width(), budget.terminal),
        );
        rux.chrome.surface(at, 20.0, chat.well, chat.carved_sm());
        match (tab, live, chatting) {
            (AgentTab::Chat, _, true) => requests.extend(agent_chat(board, rux, look, at)),
            (AgentTab::Chat, _, false) if task.session_id.is_some() => {
                let agent = board.agent_of(task);
                let said = match ticket_chat::chat_is_readable(agent) {
                    true => format!(
                        "{} names no folder that is there, so its agent's conversation cannot be found. Its                          work is in the Terminal tab.",
                        task.key
                    ),
                    false => ticket_chat::why_the_chat_is_empty(agent),
                };
                note_in_a_well(rux, at, &said);
            }
            (AgentTab::Terminal, true, _) => {
                let grid = at.shrink2(Vec2::new(10.0, 10.0));
                let focused = board.terminal_focused;
                requests
                    .extend(super::detail::terminal(board, rux.ui, look, grid, task.id, focused));
            }
            _ => requests.extend(no_terminal(board, rux, at, task)),
        }
        // A menu the agent's terminal is waiting on is never in the transcript, so the Chat tab draws it over the
        // conversation with a button for each option (`task-2263`).
        if tab == AgentTab::Chat {
            if let Some(choice) = board.choice_for(task.id) {
                requests.extend(choice_card(board, rux, at, task, &choice));
            }
        }
        if shown {
            let grip =
                Grip { salt: "agent-tasks-terminal-grip", name: "Resize agent", corner: true };
            let most = area.max.y - at.min.y - LABEL - 12.0 - SECTION_GAP;
            if let Some(height) = section_grip(rux, at, TERMINAL_LEAST, most, grip) {
                board.terminal_height = height;
            }
        }
        pen = at.max.y;
    }
    pen += SECTION_GAP;

    // ------------------------------------------------------------------ the todos
    // Under the agent and above the comments, shut until somebody opens them, and with no field to add one: the
    // todos are the plan the agent writes and ticks as it goes (`task-2263`).
    let open = board.todos_open;
    let head = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(area.width(), TODO_HEAD));
    let disc = Rect::from_min_size(head.min, Vec2::splat(TODO_HEAD));
    let turn = match open {
        true => std::f32::consts::PI,
        false => 0.0,
    };
    let fold = ChipButton::new(Icon::ChevDown, TODO_HEAD, "Todos")
        .mark(10.0, Some(1.6))
        .turn(turn)
        .folds(open)
        .show(rux, disc);
    if fold.clicked() {
        board.todos_open = !open;
    }
    let words = rux.text(Style::sans(12.0).semibold(), "Todos", theme.ink.i500);
    let words_width = words.size().x;
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(disc.max.x + 10.0, head.center().y),
        words,
        Style::sans(12.0).semibold(),
        theme.ink.i500,
    );
    if task.todo_count > 0 {
        let count = format!("{}/{}", task.todo_done_count, task.todo_count);
        let count_galley = rux.text(Style::sans(11.0), &count, theme.ink.i400);
        let left = disc.max.x + 10.0 + words_width + 12.0;
        let track = Rect::from_min_size(
            Pos2::new(left, head.center().y - 4.0),
            Vec2::new((area.max.x - left - count_galley.size().x - 10.0).clamp(0.0, 180.0), 8.0),
        );
        if track.width() > 16.0 {
            todo_track(rux, track, task);
        }
        rux::text::draw_left_capitals(
            rux.painter(),
            Pos2::new(track.max.x + 10.0, head.center().y),
            count_galley,
            Style::sans(11.0),
            theme.ink.i400,
        );
    }
    pen = head.max.y + LABEL_GAP;
    if open && budget.todos > 0.0 {
        let at =
            Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(area.width(), budget.todos));
        rux.chrome.surface(at, 20.0, chat.well, chat.carved_sm());
        requests.extend(super::detail::todo_rows(board, rux, at.shrink(TODO_PAD), look));
        pen = at.max.y;
    }
    pen += SECTION_GAP;

    // ------------------------------------------------------------------ the comments
    let title = format!("Comments \u{b7} {}", task.comment_count);
    let comments_open = !board.comments_shut;
    let (pressed, _) =
        fold_heading(rux, Pos2::new(area.min.x, pen), "Comments", &title, comments_open, true);
    if pressed {
        board.comments_shut = comments_open;
    }
    pen += TODO_HEAD + LABEL_GAP;
    let comments_at = Rect::from_min_max(
        Pos2::new(area.min.x, pen),
        Pos2::new(area.max.x, (pen + budget.comments).min(area.max.y)),
    );
    if comments_open && comments_at.height() > 20.0 {
        // The grip sits inside the bottom of the section rather than under it, because the section usually ends
        // at the dialog's own padding and a strip under it would be outside the room it was given. The comments
        // and the composer are laid out above that strip, so the strip is under the buttons rather than over
        // them (`task-2260`: *"the buttons aren't clickable because the resize icon"*).
        let well = Rect::from_min_max(
            comments_at.min,
            Pos2::new(comments_at.max.x, comments_at.max.y - DESCRIPTION_GRIP),
        );
        requests.extend(super::detail::comment_section(board, rux, order, well, look));
        let grip =
            Grip { salt: "agent-tasks-comments-grip", name: "Resize comments", corner: false };
        let most = area.max.y - well.min.y - DESCRIPTION_GRIP;
        if let Some(height) = section_grip(rux, well, COMMENTS_LEAST, most, grip) {
            board.comments_height = height.map(|height| height + DESCRIPTION_GRIP);
        }
    }
    requests
}

/// The words on the button that moves a ticket's chat to the Agent-Chat side pane (`task-2269`).
const TO_SIDE_PANE: &str = "To Side Pane";

/// The Agent section's two tabs, a carved track with the chosen tab raised out of it, ending at `right` and
/// centred on its `y`. Answers a new choice, and where the track starts so a control can go before it.
///
/// The look of `settings_kit::segmented`, at the height of a section's heading, and named `Chat tab` and
/// `Terminal tab` so that neither shares a name with another control in the window.
fn agent_tabs(rux: &mut rux::Rux<'_>, right: Pos2, chosen: AgentTab) -> (Option<AgentTab>, f32) {
    let theme = rux.theme();
    let chat = theme.chat;
    let style = Style::sans(11.0);
    let tabs = [(AgentTab::Chat, "Chat"), (AgentTab::Terminal, "Terminal")];
    let widths: Vec<f32> =
        tabs.iter().map(|(_, word)| rux.measure(style, word).x + TAB_PAD * 2.0).collect();
    let width = widths.iter().sum::<f32>() + 3.0 + 8.0;
    let track = Rect::from_center_size(
        Pos2::new(right.x - width / 2.0, right.y),
        Vec2::new(width, TAB_HEIGHT + 8.0),
    );
    rux.chrome.surface(track, track.height() / 2.0, chat.well, chat.carved_sm());
    let mut x = track.left() + 4.0;
    let mut picked = None;
    for ((tab, word), width) in tabs.into_iter().zip(widths) {
        let pill = Rect::from_min_size(
            Pos2::new(x, track.center().y - TAB_HEIGHT / 2.0),
            Vec2::new(width, TAB_HEIGHT),
        );
        let on = tab == chosen;
        let response = rux.ui.interact(
            pill,
            rux.ui.id().with(("agent-tasks-agent-tab", word)),
            egui::Sense::click(),
        );
        if on {
            rux.chrome.surface(
                pill,
                TAB_HEIGHT / 2.0,
                rux::Fill::gradient(chat.chip, pill),
                chat.raised_chip(),
            );
        } else if response.hovered() {
            rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let ink = match on {
            true => theme.ink.i900,
            false => theme.ink.i500,
        };
        let galley = rux.text(style, word, ink);
        rux::text::draw_centred(rux.painter(), pill, galley, ink);
        let named = format!("{word} tab");
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, named.clone())
        });
        if response.clicked() && !on {
            picked = Some(tab);
        }
        x = pill.right() + 3.0;
    }
    (picked, track.left())
}

/// The ticket's conversation in the Agent-Chat pane, inside the section's well (`task-2261`).
///
/// The side panel's own drawing, `components::agent_chat::pane`, handed the chat that reads the ticket's
/// transcript. It is taken off the board for the length of the drawing because the pane needs it mutably.
fn agent_chat(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    look: &Look<'_>,
    at: Rect,
) -> Vec<Request> {
    let Some(mut chat) = board.ticket_chat.take() else {
        return Vec::new();
    };
    let inside = at.shrink2(Vec2::new(2.0, 4.0));
    let mut child = rux.ui.new_child(
        egui::UiBuilder::new().max_rect(inside).id_salt(("agent-tasks-ticket-chat", chat.task_id)),
    );
    child.set_clip_rect(inside.intersect(rux.ui.clip_rect()));
    let requests = crate::components::agent_chat::pane(&mut chat.chat, &mut child, look);
    board.ticket_chat = Some(chat);
    requests
}

/// A raised card along the bottom of the Chat tab: the question the agent's terminal is waiting on, and a pill
/// button for each option, the last of them filled in the action's gradient. Pressing one types the arrow keys and
/// Enter that choose it, through the same `choice` command an agent uses.
///
/// Drawn in a `rux` layer of its own, opened after the chat, so it sits over the conversation rather than under it.
fn choice_card(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    at: Rect,
    task: &Task,
    choice: &crate::services::agent_tasks::screen_choice::Choice,
) -> Vec<Request> {
    let state = rux.state;
    // The buttons' row is the card's width less its padding. A label wider than that is shortened, and the
    // buttons wrap onto as many rows as they need: laid in one row, every button past the first full row was
    // pushed to the card's left edge and drawn over the others (`task-2268`).
    let room = (at.width() - 24.0 - 28.0).max(40.0);
    let labels: Vec<String> = choice
        .options
        .iter()
        .map(|option| label_that_fits(rux, option, room - BUTTON_PADDING))
        .collect();
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| (rux.measure(Style::sans(12.0), label).x + BUTTON_PADDING).min(room))
        .collect();
    let placed = lay_the_buttons_out(&widths, room);
    let rows = placed.iter().map(|button| button.row + 1).max().unwrap_or(1);
    let height = CHOICE_HEIGHT + (rows - 1) as f32 * (BUTTON_HEIGHT + BUTTON_GAP);
    let card = Rect::from_min_max(
        Pos2::new(at.min.x + 12.0, (at.max.y - 12.0 - height).max(at.min.y + 8.0)),
        Pos2::new(at.max.x - 12.0, at.max.y - 12.0),
    );
    let id = rux.ui.id().with(("agent-tasks-choice", task.id));
    let pressed = rux::layer(rux.ui, state, id, card.expand(40.0), |rux| {
        let chat = rux.theme().chat;
        let theme = rux.theme();
        rux.chrome.surface(card, 22.0, rux::Fill::gradient(chat.card, card), chat.raised());
        let asked = match choice.question.is_empty() {
            true => "The agent is waiting for an answer in its terminal.".to_owned(),
            false => format!("The agent is asking: {}", choice.question),
        };
        let words =
            rux.elided(Style::sans(12.5).semibold(), &asked, theme.ink.i900, card.width() - 36.0);
        rux.painter().galley(
            Pos2::new(card.min.x + 18.0, card.min.y + 14.0),
            words,
            theme.ink.i900,
        );
        let first_row =
            card.max.y - 14.0 - rows as f32 * BUTTON_HEIGHT - (rows - 1) as f32 * BUTTON_GAP;
        let mut pressed = None;
        for (index, (label, button_at)) in labels.iter().zip(&placed).enumerate() {
            let mut button = rux::components::PillButton::new(label);
            if index + 1 == choice.options.len() {
                button = button.primary();
            }
            let top = first_row + button_at.row as f32 * (BUTTON_HEIGHT + BUTTON_GAP);
            let left = card.min.x + 14.0 + button_at.left;
            let place = Rect::from_min_size(
                Pos2::new(left, top),
                Vec2::new(button_at.width, BUTTON_HEIGHT),
            );
            if button.show(rux, place).clicked() {
                pressed = Some(index);
            }
        }
        pressed
    });
    match pressed {
        Some(index) => match board.answer_a_choice(&task.key, &(index + 1).to_string()) {
            Ok(answer) => vec![Request::Message(answer.message)],
            Err(problem) => vec![Request::Message(problem)],
        },
        None => Vec::new(),
    }
}

/// How tall the card holding a waiting menu is: its question and one row of buttons. Each further row adds a
/// button's height and the gap between rows.
const CHOICE_HEIGHT: f32 = 92.0;

/// How tall an option's button is.
const BUTTON_HEIGHT: f32 = 34.0;

/// The space between two buttons, across a row and between rows.
const BUTTON_GAP: f32 = 8.0;

/// What a button adds round its label.
const BUTTON_PADDING: f32 = 36.0;

/// Where one option's button goes: its row, counted from the top, and how far from the left of the buttons' room
/// it starts.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ButtonPlace {
    row: usize,
    left: f32,
    width: f32,
}

/// Put buttons `widths` wide into rows `room` wide, in order, each row against the right edge so the last option
/// is at the bottom right, where the action is.
///
/// A button wider than the room is given the whole row. No two buttons overlap, which is what one row with no
/// limit did once the options were wider than the card.
fn lay_the_buttons_out(widths: &[f32], room: f32) -> Vec<ButtonPlace> {
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (index, width) in widths.iter().enumerate() {
        let width = width.min(room);
        let row = rows.last_mut().expect("there is always a row");
        let needed = match row.is_empty() {
            true => width,
            false => used + BUTTON_GAP + width,
        };
        if !row.is_empty() && needed > room {
            rows.push(vec![index]);
            used = width;
        } else {
            row.push(index);
            used = needed;
        }
    }
    let mut placed = vec![ButtonPlace { row: 0, left: 0.0, width: 0.0 }; widths.len()];
    for (row_index, row) in rows.iter().enumerate() {
        let mut right = room;
        for &index in row.iter().rev() {
            let width = widths[index].min(room);
            placed[index] = ButtonPlace { row: row_index, left: right - width, width };
            right -= width + BUTTON_GAP;
        }
    }
    placed
}

/// `option`, shortened with three dots when it is wider than `room` at the buttons' size.
fn label_that_fits(rux: &mut rux::Rux<'_>, option: &str, room: f32) -> String {
    if rux.measure(Style::sans(12.0), option).x <= room {
        return option.to_owned();
    }
    let mut kept: Vec<char> = option.chars().collect();
    while kept.pop().is_some() {
        let shortened = format!("{}...", kept.iter().collect::<String>().trim_end());
        if rux.measure(Style::sans(12.0), &shortened).x <= room {
            return shortened;
        }
    }
    "...".to_owned()
}

/// One sentence in the middle of the section's well, for a tab with nothing to show.
fn note_in_a_well(rux: &mut rux::Rux<'_>, at: Rect, said: &str) {
    let theme = rux.theme();
    let disc =
        Rect::from_center_size(Pos2::new(at.min.x + 14.0 + 17.0, at.center().y), Vec2::splat(34.0));
    rux::components::chip_disc(rux, disc);
    terminal_mark(rux, disc.center(), theme.ink.i400);
    let left = disc.max.x + 14.0;
    let galley = rux::text::wrapped(
        rux.painter(),
        Style::sans(12.0).leading(1.5),
        said,
        theme.ink.i400,
        (at.max.x - 14.0 - left).max(40.0),
    );
    let top = at.center().y - galley.size().y / 2.0;
    rux.painter().with_clip_rect(at).galley(Pos2::new(left, top), galley, theme.ink.i400);
}

/// The todos' track: the share of them done, in the action's gradient.
fn todo_track(rux: &mut rux::Rux<'_>, track: Rect, task: &Task) {
    let chat = rux.theme().chat;
    rux.chrome.surface(track, track.height() / 2.0, chat.well, chat.carved_sm());
    let fraction = rux::components::TaskCard::fraction(
        task.todo_done_count.max(0) as u32,
        task.todo_count.max(0) as u32,
    );
    let length = track.width() * fraction - 4.0;
    if length > 0.5 {
        let fill = Rect::from_min_size(
            track.min + Vec2::splat(2.0),
            Vec2::new(length, track.height() - 4.0),
        );
        rux.chrome.rect(fill, fill.height() / 2.0, rux::Fill::gradient(chat.action, fill));
    }
}

/// The strip that says a ticket has no terminal, with `Resume session` when there is a conversation to hand
/// back.
fn no_terminal(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    at: Rect,
    task: &Task,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let theme = rux.theme();
    let disc =
        Rect::from_center_size(Pos2::new(at.min.x + 14.0 + 17.0, at.center().y), Vec2::splat(34.0));
    rux::components::chip_disc(rux, disc);
    terminal_mark(rux, disc.center(), theme.ink.i400);
    let resumable =
        crate::services::agent_tasks::agent::can_resume(task.assignee, task.session_id.as_deref());
    let mut right = at.max.x - 14.0;
    if resumable {
        let button = rux::components::PillButton::new("Resume session");
        let width = button.measure(rux).x;
        let place = Rect::from_min_size(
            Pos2::new(right - width, at.center().y - 17.0),
            Vec2::new(width, 34.0),
        );
        if button.show(rux, place).clicked() {
            match board.command_now("resume", std::slice::from_ref(&task.key)) {
                Ok(answer) if !answer.message.is_empty() => {
                    requests.push(Request::Message(answer.message))
                }
                Ok(_) => {}
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
        right = place.min.x - 14.0;
    }
    let said =
        "No terminal for this task yet. Start Work launches its agent; Resume session hands its \
                conversation back.";
    let left = disc.max.x + 14.0;
    let galley = rux::text::wrapped(
        rux.painter(),
        Style::sans(12.0).leading(1.5),
        said,
        theme.ink.i400,
        (right - left).max(40.0),
    );
    let top = at.center().y - galley.size().y / 2.0;
    rux.painter().with_clip_rect(at).galley(Pos2::new(left, top), galley, theme.ink.i400);
    requests
}

/// The terminal's mark: a chevron and an underscore, `M3.5 5l3 3-3 3M8.5 11h4` in a 16 unit box.
fn terminal_mark(rux: &rux::Rux<'_>, centre: Pos2, colour: egui::Color32) {
    let at = |x: f32, y: f32| Pos2::new(centre.x + x - 8.0, centre.y + y - 8.0);
    let stroke = egui::Stroke::new(1.7, colour);
    rux.painter().add(egui::Shape::line(vec![at(3.5, 5.0), at(6.5, 8.0), at(3.5, 11.0)], stroke));
    rux.painter().line_segment([at(8.5, 11.0), at(12.5, 11.0)], stroke);
}

/// Which section a [`section_grip`] resizes: the salt its id and its remembered start are made from, and the
/// name a test and assistive technology find it by.
struct Grip {
    salt: &'static str,
    name: &'static str,
    /// Whether the grip takes the well's bottom right corner and draws the diagonal mark there. The comments'
    /// grip does not, because that corner is where the `Comment` button is: its grip is the strip under the
    /// section and nothing else.
    corner: bool,
}

/// The corner of a section's well that resizes it, answering the height to keep when it was dragged.
///
/// `task-2214` for the description, `task-2255` for the terminal and the comments. The design draws a small
/// diagonal mark in the well's bottom right corner, which is where a browser puts a resizable box's handle; the
/// strip under the well takes the drag as well, so the edge can be caught anywhere along it. A double click
/// answers `Some(None)`, which puts the section back to the height the modal chooses. `least` is the least it
/// may be made and `most` the furthest it may reach.
fn section_grip(
    rux: &mut rux::Rux<'_>,
    well: Rect,
    least: f32,
    most: f32,
    grip: Grip,
) -> Option<Option<f32>> {
    let corner = Rect::from_min_max(
        well.max - Vec2::new(24.0, 22.0),
        well.max + Vec2::new(0.0, DESCRIPTION_GRIP),
    );
    let strip = Rect::from_min_size(
        Pos2::new(well.min.x, well.max.y),
        Vec2::new(well.width(), DESCRIPTION_GRIP),
    );
    let grabbed = match grip.corner {
        true => corner.union(strip),
        false => strip,
    };
    let response =
        rux.ui.interact(grabbed, rux.ui.id().with(grip.salt), egui::Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, grip.name));
    let active = response.hovered() || response.dragged();
    if active {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let theme = rux.theme();
    let tint = match active {
        true => theme.chat.sky,
        false => theme.ink.i400,
    };
    // `M10.5 4.5l-6 6M10.5 8l-2.5 2.5` in a 12 point box, `right: 12px; bottom: 10px`. A grip with no corner
    // shows a short bar in the middle of its strip instead, while the pointer is on it.
    match grip.corner {
        true => {
            let origin = Pos2::new(well.max.x - 12.0 - 12.0, well.max.y - 10.0 - 12.0);
            let at = |x: f32, y: f32| origin + Vec2::new(x, y);
            let stroke = egui::Stroke::new(1.5, tint);
            rux.painter().line_segment([at(10.5, 4.5), at(4.5, 10.5)], stroke);
            rux.painter().line_segment([at(10.5, 8.0), at(8.0, 10.5)], stroke);
        }
        false if active => {
            let middle = strip.center();
            let stroke = egui::Stroke::new(2.0, tint);
            rux.painter().line_segment(
                [middle - Vec2::new(18.0, 0.0), middle + Vec2::new(18.0, 0.0)],
                stroke,
            );
        }
        false => {}
    }
    if response.double_clicked() {
        return Some(None);
    }
    // **Measured from where the press began**, against the height the well had then. egui reports a drag only
    // once the pointer has moved past its threshold, so adding up deltas lost the first few points of every drag.
    let started = rux.ui.id().with((grip.salt, "start"));
    if response.drag_started() {
        rux.ui.ctx().data_mut(|data| data.insert_temp(started, well.height()));
    }
    if response.dragged() {
        let from = rux.ui.ctx().data(|data| data.get_temp::<f32>(started)).unwrap_or(well.height());
        let travelled = rux.ui.ctx().input(|input| {
            match (input.pointer.press_origin(), input.pointer.interact_pos()) {
                (Some(origin), Some(now)) => now.y - origin.y,
                _ => 0.0,
            }
        });
        let height = (from + travelled).clamp(least, most.max(least));
        if (height - well.height()).abs() > 0.1 {
            return Some(Some(height));
        }
    }
    None
}

/// The card of fields down the right: `Start Work`, what the ticket is, and its priority. Answers whatever was
/// asked for, and whether `Create task` was pressed.
///
/// `padding: 18px; gap: 16px` on the card gradient with `--out`, `border-radius: 26px`. **It scrolls**, because
/// in a dialog dragged towards its smallest it cannot be made to fit, and the date and `Delete task`
/// are under the priority, past the design's own fields.
fn fields(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    area: Rect,
    task: &Task,
    new: bool,
) -> (Vec<Request>, bool) {
    let chat = rux.theme().chat;
    rux.chrome.surface(area, 26.0, rux::Fill::gradient(chat.card, area), chat.raised());
    let mut requests = Vec::new();
    let mut create = false;
    let state = rux.state;
    let inner = area.shrink(18.0);
    // `Create task` sits at the foot of a new ticket's card, outside the scrolling part.
    let foot = match new {
        true => 44.0 + 16.0,
        false => 0.0,
    };
    let list = Rect::from_min_max(inner.min, Pos2::new(inner.max.x, inner.max.y - foot));
    // **The design's 16 points between fields when they fit, and 12 when they would not.** The design has five
    // fields; a ticket for an agent has six, because `Effort` is one, and in a short window the priority would
    // otherwise be under the fold.
    let fields = match task.assignee.is_an_agent() {
        true => 6.0,
        false => 4.0,
    };
    let start = match new {
        true => 0.0,
        false => 44.0,
    };
    let wanted = |gap: f32| start + gap + (fields + 1.0) * (13.0 + 7.0 + 38.0 + gap);
    let gap = match wanted(16.0) <= list.height() {
        true => 16.0,
        false => 12.0,
    };
    // The scrolling part reaches past the controls by the reach of their shadows, so a raised chip is not cut
    // along its edge. `task-2200`.
    let reach = Vec2::new(12.0, 10.0);
    let viewport = list.expand2(reach);
    let mut inside = rux.ui.new_child(egui::UiBuilder::new().max_rect(viewport));
    inside.set_clip_rect(viewport.intersect(rux.ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("agent-tasks-ticket-fields")
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut inside, |ui| {
            let top = ui.cursor().min.y + reach.y;
            let content =
                Rect::from_min_size(Pos2::new(list.min.x, top), Vec2::new(list.width(), 1200.0));
            let used = rux::layer(
                ui,
                state,
                egui::Id::new("agent-tasks-ticket-fields-rux"),
                content.expand(24.0),
                |rux| {
                    let (asked, used) =
                        field_column(board, rux, selects, content, task, new, gap);
                    requests = asked;
                    used
                },
            );
            ui.expand_to_include_rect(Rect::from_min_size(
                Pos2::new(list.min.x, top),
                Vec2::new(list.width(), (used - top + reach.y).max(0.0)),
            ));
        });
    if new {
        let at = Rect::from_min_size(
            Pos2::new(inner.min.x, inner.max.y - 44.0),
            Vec2::new(inner.width(), 44.0),
        );
        create = action_pill(rux, at, "Create task", Icon::Plus, true).clicked();
    }
    (requests, create)
}

/// The design's 44 point action pill: `Start Work` and `Create task`, in the action's gradient.
fn action_pill(
    rux: &mut rux::Rux<'_>,
    at: Rect,
    label: &str,
    icon: Icon,
    primary: bool,
) -> egui::Response {
    let mut button = rux::components::PillButton::new(label).large().mark(icon);
    if primary {
        button = button.primary();
    }
    button.show(rux, Rect::from_center_size(at.center(), Vec2::new(at.width(), 44.0)))
}

/// The fields themselves, answering where the last of them ended.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn field_column(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    area: Rect,
    task: &Task,
    new: bool,
    gap: f32,
) -> (Vec<Request>, f32) {
    let mut requests = Vec::new();
    let theme = rux.theme();
    let width = area.width();
    let mut pen = area.min.y;

    // ---------------------------------------------------------------- the one thing to press
    //
    // At the top, where somebody opening a ticket to start an agent looks first. A ticket with an agent already
    // running offers `Stop`, and a new one offers nothing because it has no title yet.
    if !new {
        let attached =
            board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
        let (label, command, icon) = match () {
            _ if attached => ("Stop", "stop", Icon::Pause),
            _ if task.session_id.is_none() => ("Start Work", "start", Icon::Play),
            _ if crate::services::agent_tasks::agent::can_resume(
                task.assignee,
                task.session_id.as_deref(),
            ) =>
            {
                ("Resume session", "resume", Icon::Play)
            }
            // **`Start Work again`, not `Resume session`.** Codex and OpenCode name their own sessions, and a
            // ticket recorded with a marker from before `task-2285` has nothing that finds the one they chose.
            _ => ("Start Work again", "start", Icon::Play),
        };
        let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, 44.0));
        if action_pill(rux, at, label, icon, !attached).clicked() {
            match board.command_now(command, std::slice::from_ref(&task.key)) {
                Ok(answer) if !answer.message.is_empty() => {
                    requests.push(Request::Message(answer.message))
                }
                Ok(_) => {}
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
        pen += 44.0 + gap;
    }

    // ---------------------------------------------------------------- what the ticket is
    let options: Vec<(String, String)> = Status::ALL
        .iter()
        .map(|status| (status.name().to_owned(), status_name(*status).to_owned()))
        .collect();
    if let Some(chosen) = choice(
        rux,
        selects,
        &mut pen,
        area.min.x,
        width,
        "Status",
        SelectLead::Lane(lane_mark(task.status)),
        &options,
        task.status.name(),
        None,
        gap,
    ) {
        if let Some(status) = Status::parse(&chosen) {
            if let Err(problem) = board.move_card(task.id, status, i64::MAX) {
                requests.push(Request::Message(problem));
            }
        }
    }

    // **The endpoints that run an agent, then a person** (`task-2247`): what a ticket is given to is one of
    // the endpoints in `Settings -> AI Configuration`, by name. A ticket that names one no longer there keeps it
    // in the list, so opening a ticket cannot quietly change who it belongs to.
    let said = crate::services::agent_tasks::AgentTasks::assignee_said(task);
    let mut names = board.assignee_choices();
    if !names.contains(&said) {
        names.insert(0, said.clone());
    }
    let options: Vec<(String, String)> =
        names.into_iter().map(|name| (name.clone(), name)).collect();
    let who = match task.assignee {
        Assignee::Human => SelectLead::Mark(Icon::User, theme.ink.i400),
        _ => SelectLead::Mark(Icon::Spark, crate::theme::color::agent()),
    };
    if let Some(chosen) = choice(
        rux, selects, &mut pen, area.min.x, width, "Assignee", who, &options, &said, None, gap,
    ) {
        requests.extend(write(board, task, Field::Assignee(chosen)));
    }

    // **Absent** for a ticket assigned to a person rather than disabled, which is Unluminous's rule.
    if task.assignee.is_an_agent() {
        // A dropdown rather than a text field: `task-28` found an agent could not be started because a model
        // identifier had to be typed from memory. `models_for` keeps whatever the row already names in the list.
        let model = task.model.clone().unwrap_or_default();
        let models: Vec<(String, String)> =
            crate::services::agent_tasks::agent::models_for(task.assignee, task.model.as_deref())
                .into_iter()
                .map(|name| (name.clone(), name))
                .collect();
        if let Some(chosen) = choice(
            rux,
            selects,
            &mut pen,
            area.min.x,
            width,
            "Model",
            SelectLead::Mark(Icon::Cube, theme.ink.i400),
            &models,
            &model,
            Some("The agent's default"),
            gap,
        ) {
            requests.extend(write(board, task, Field::Model(chosen)));
        }
        let efforts: Vec<(String, String)> =
            EFFORTS.iter().map(|level| ((*level).to_owned(), (*level).to_owned())).collect();
        if let Some(chosen) = choice(
            rux,
            selects,
            &mut pen,
            area.min.x,
            width,
            "Effort",
            SelectLead::Mark(Icon::Spark, theme.ink.i400),
            &efforts,
            task.effort.as_deref().unwrap_or(""),
            Some("Model default"),
            gap,
        ) {
            requests.extend(write(board, task, Field::Effort(chosen)));
        }
    }

    // The projects this window knows about, which is the list `File -> Open Recent` draws. A ticket may still
    // name one this window has never opened, so whatever the row says is kept in the list the way a model is.
    let project = task.project.clone().unwrap_or_default();
    let projects: Vec<(String, String)> = board
        .known_projects(task.project.as_deref())
        .into_iter()
        .map(|path| (path.clone(), crate::services::paths::the_useful_end_of(&path)))
        .collect();
    if let Some(chosen) = choice(
        rux,
        selects,
        &mut pen,
        area.min.x,
        width,
        "Project",
        SelectLead::Mark(Icon::Folder, theme.ink.i400),
        &projects,
        &project,
        Some("The folder this window has open"),
        gap,
    ) {
        requests.extend(write(board, task, Field::Project(chosen)));
    }

    let epics: Vec<(String, String)> =
        board.board().epics.iter().map(|epic| (epic.id.to_string(), epic.name.clone())).collect();
    if let Some(chosen) = choice(
        rux,
        selects,
        &mut pen,
        area.min.x,
        width,
        "Epic",
        SelectLead::Mark(Icon::Layers, theme.ink.i400),
        &epics,
        &task.epic_id.map(|id| id.to_string()).unwrap_or_default(),
        Some("No epic"),
        gap,
    ) {
        requests.extend(write(board, task, Field::Epic(chosen)));
    }

    // The priority, as a small pill switch: the design's `Low`, `Medium` and `High`.
    pen = caption(rux, pen, area.min.x, "Priority");
    let names: Vec<String> =
        Priority::ALL.iter().map(|priority| priority_name(*priority).to_owned()).collect();
    let chosen = Priority::ALL.iter().position(|priority| *priority == task.priority);
    let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, 38.0));
    if let Some(index) = PillSwitch::new(&names, chosen, "Priority").small().show(rux, at) {
        if Some(index) != chosen {
            let priority = Priority::ALL[index];
            requests.extend(write(board, task, Field::Priority(priority.name().to_owned())));
        }
    }
    pen += 38.0 + gap;

    if new {
        return (requests, pen);
    }

    // ---------------------------------------------------------------- when it was made
    //
    // The JIRA field that was here is gone (`task-2293`): *"Get rid of the JIRA input. We don't need to track this
    // any longer."*
    pen = caption(rux, pen, area.min.x, "Created");
    let now = clock::now();
    let said = clock::relative(&task.created_at, &now);
    let galley = rux.text(Style::sans(12.5), &said, theme.ink.i700);
    let tall = galley.size().y;
    rux.painter().galley(Pos2::new(area.min.x, pen), galley, theme.ink.i700);
    pen += tall + 20.0;

    // ------------------------------------------------------------ and the one thing that destroys work
    //
    // Last, as a ghost pill whose words are in the slower colour, which the design keeps for things that fail
    // or are taken away. One press deletes the ticket with its todos and comments. `task-2198`.
    let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, 36.0));
    if delete_button(rux, at).clicked() {
        if let Err(problem) = board.discard_the_ticket() {
            requests.push(Request::Message(problem));
        }
    }
    pen += 36.0 + 12.0;
    (requests, pen)
}

/// `Delete task`: a ghost pill with a bin, in the slower colour, washed under the pointer.
fn delete_button(rux: &mut rux::Rux<'_>, at: Rect) -> egui::Response {
    let response =
        rux.ui.interact(at, rux.ui.id().with("agent-tasks-delete"), egui::Sense::click());
    let chat = rux.theme().chat;
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        rux.wash(at, at.height() / 2.0, chat.slower.gamma_multiply(0.12));
    }
    let style = Style::sans(12.5).medium();
    let galley = rux.text(style, "Delete task", chat.slower);
    let width = galley.size().x + 14.0 + 8.0;
    let left = at.center().x - width / 2.0;
    rux.mark(Mark::new(Icon::Trash, 14.0), Pos2::new(left + 7.0, at.center().y), chat.slower);
    rux::text::draw_left_capitals(
        rux.painter(),
        Pos2::new(left + 22.0, at.center().y),
        galley,
        style,
        chat.slower,
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Delete task"));
    response
}

/// What the modal calls a status: the lane's name in the design's sentence case.
fn status_name(status: Status) -> &'static str {
    match status {
        Status::New => "New",
        Status::QaFailed => "QA failed",
        Status::InProgress => "In progress",
        Status::AgentDone => "Agent done",
    }
}

/// The mark a status is drawn with, which is its lane's.
fn lane_mark(status: Status) -> rux::components::LaneMark {
    match status {
        Status::New => rux::components::LaneMark::New,
        Status::QaFailed => rux::components::LaneMark::Failed,
        Status::InProgress => rux::components::LaneMark::Running,
        Status::AgentDone => rux::components::LaneMark::Done,
    }
}

/// What the priority switch calls each priority.
fn priority_name(priority: Priority) -> &'static str {
    match priority {
        Priority::Low => "Low",
        Priority::Medium => "Medium",
        Priority::High => "High",
    }
}

/// A field's caption, `font-size: 11px` in the quiet ink, answering where the control under it goes.
fn caption(rux: &mut rux::Rux<'_>, pen: f32, left: f32, name: &str) -> f32 {
    let theme = rux.theme();
    let galley = rux.text(Style::sans(11.0), name, theme.ink.i400);
    let tall = galley.size().y;
    rux.painter().galley(Pos2::new(left, pen), galley, theme.ink.i400);
    pen + tall + 7.0
}

/// A named value chosen from a list, answering what was chosen when it changed.
///
/// `rux`'s `Select` drawn as the design's chip, with the field's name over it. `options` is `(value, said)`
/// pairs: the value written to the row and the words a person reads. `empty` is what the list calls holding
/// nothing, for a field that may, and it is the first row; `None` means the field is required.
#[allow(clippy::too_many_arguments)]
fn choice(
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    pen: &mut f32,
    left: f32,
    width: f32,
    name: &'static str,
    lead: SelectLead,
    options: &[(String, String)],
    chosen: &str,
    empty: Option<&str>,
    gap: f32,
) -> Option<String> {
    *pen = caption(rux, *pen, left, name);
    let mut values: Vec<String> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    if let Some(empty) = empty {
        values.push(String::new());
        said.push(empty.to_owned());
    }
    for (value, words) in options {
        values.push(value.clone());
        said.push(words.clone());
    }
    let selected = values.iter().position(|value| value == chosen);
    let at = Rect::from_min_size(Pos2::new(left, *pen), Vec2::new(width, 38.0));
    let state = selects.entry(name).or_default();
    let outcome = Select::new(&said, selected)
        .label(name)
        .placeholder("\u{2014}")
        .chip(lead)
        .show(rux, at, state);
    *pen += 38.0 + gap;
    outcome.chosen.and_then(|index| values.get(index).cloned()).filter(|value| {
        Some(value.as_str()) != selected.and_then(|at| values.get(at)).map(String::as_str)
    })
}

/// Write one field, and report what could not be written.
fn write(board: &mut AgentTasks, task: &Task, field: Field) -> Vec<Request> {
    match board.edit_field(task.id, field) {
        Ok(()) => Vec::new(),
        Err(problem) => vec![Request::Message(problem)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::text_renderer::TextRenderer;
    use crate::settings::Settings;

    /// A `Context` answers `content_rect()` from the size a pass began with — `begin_pass` is what
    /// reads it, and it is the only thing this needs; no `Ui` is ever built.
    fn a_context(width: f32, height: f32) -> egui::Context {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(width, height),
            )),
            ..Default::default()
        });
        context
    }

    /// `task-2280`: a ticket fills the window less three percent of it on every side, however large the window.
    #[test]
    fn a_ticket_fills_the_window_less_its_margin() {
        let renderer = TextRenderer::new();
        let look = Look::of(&Settings::new(), &renderer);
        let (width, height) = size(&a_context(2560.0, 1440.0), &look);
        assert!((width - 2560.0 * 0.94).abs() < 0.01, "{width}");
        assert!((height - 1440.0 * 0.94).abs() < 0.01, "{height}");
    }

    #[test]
    fn a_window_smaller_than_the_floor_asks_for_no_more_than_the_window_itself() {
        // `modal::fit` clamps anything larger than the window, so a modal that asked for more than a
        // small window has would simply be shrunk back down there — the floor costs nothing to keep.
        let renderer = TextRenderer::new();
        let look = Look::of(&Settings::new(), &renderer);
        let context = a_context(500.0, 400.0);
        let (width, height) = size(&context, &look);
        assert!(width <= 500.0, "{width} should not exceed the window's own width");
        assert!(height <= 400.0, "{height} should not exceed the window's own height");
    }

    /// `task-2214`: `Tab` goes to the next box and round the end, `Shift+Tab` the other way, and with no box
    /// focused they go to the first and the last.
    #[test]
    fn tab_goes_to_the_next_box_and_round_the_end() {
        let [a, b, c] = ["a", "b", "c"].map(egui::Id::new);
        let order = [a, b, c];
        assert_eq!(the_next_box(&order, None, false), Some(a));
        assert_eq!(the_next_box(&order, None, true), Some(c));
        assert_eq!(the_next_box(&order, Some(a), false), Some(b));
        assert_eq!(the_next_box(&order, Some(c), false), Some(a), "round the end");
        assert_eq!(the_next_box(&order, Some(a), true), Some(c), "round the start");
        assert_eq!(the_next_box(&order, Some(egui::Id::new("button")), false), Some(a));
        assert_eq!(the_next_box(&[], Some(a), false), None, "a modal with no boxes moves nothing");
    }

    /// `task-2193`: the labels followed the editor's font and the fields did not. The dialog's look is set at
    /// one size whatever the editor is, so the two cannot drift apart again.
    #[test]
    fn the_dialog_is_set_in_one_size_whatever_the_editor_is_set_to() {
        let renderer = TextRenderer::new();
        let mut settings = Settings::new();
        settings.font_size = 32.0;
        let look = Look::of(&settings, &renderer).at_a_fixed_size(BODY);
        assert_eq!(look.font_size, BODY);
        assert_eq!(look.scale(), 1.0, "nothing in the dialog is scaled up with the editor");
    }

    /// `task-2268`: the options of a menu read off the screen were laid in one row, and once the row was full
    /// every later button was pushed to the card's left edge and drawn over the others. They wrap onto rows now,
    /// and no two overlap whatever their widths.
    #[test]
    fn the_buttons_of_a_waiting_menu_wrap_onto_rows_and_never_overlap() {
        let room = 400.0;
        for widths in [
            vec![90.0, 120.0],
            vec![300.0, 300.0, 300.0],
            vec![900.0, 80.0, 700.0, 60.0],
            vec![150.0; 7],
        ] {
            let placed = lay_the_buttons_out(&widths, room);
            assert_eq!(placed.len(), widths.len());
            for (index, button) in placed.iter().enumerate() {
                assert!(
                    button.left >= -0.001,
                    "{widths:?}: button {index} starts left of the room"
                );
                assert!(
                    button.left + button.width <= room + 0.001,
                    "{widths:?}: {index} ends past it"
                );
                for other in &placed[index + 1..] {
                    let apart = other.row != button.row
                        || other.left >= button.left + button.width
                        || button.left >= other.left + other.width;
                    assert!(apart, "{widths:?}: {button:?} overlaps {other:?}");
                }
            }
            let last = placed.last().expect("a button");
            assert!(
                (last.left + last.width - room).abs() < 0.001,
                "the last option is at the right"
            );
        }
        let one_row = lay_the_buttons_out(&[90.0, 120.0], room);
        assert!(one_row.iter().all(|button| button.row == 0), "two short options share a row");
    }
}
