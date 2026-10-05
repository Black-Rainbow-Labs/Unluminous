//! One ticket, in full: the modal the board opens a card into.
//!
//! `tasks/agent-tasks-ui-tdd.md` §2.4 is the list this is measured against, and §5 is the design. Two columns
//! inside one frame, which is what the browser board does, and the frame is `components::modal`'s — the same
//! header, body and footer the Settings window and the nine git dialogs are made of, with the dragging and
//! resizing `modal::show` already owns.
//!
//! ## What is inside the frame is `rux`
//!
//! `task-2193`: *"The modal is very hard to read, has inconsistent font sizes that don't relatively fit (e.g.
//! labels are way larger than input text). Use blackrainbowlabs-rux and do a thorough design that is highly
//! polished."* The labels were drawn from the editor's own font size and the fields were not, so on a machine
//! whose editor is set large a field's name was twice the size of what was in it.
//!
//! So everything in the body is set in one type scale, `rux`'s: the start button, the dropdowns, the fields,
//! the section headings, the wells the description, the todos and the terminal sit in, and the footer's
//! buttons are `rux` components, and what is still drawn by Unluminous's own helpers — the todo rows, the
//! comments, the two view buttons — is drawn with a [`Look`] fixed at [`BODY`] points through
//! `Look::at_a_fixed_size`, so it is in proportion with them. The editor's font no longer reaches into the
//! dialog at all, which is what makes the dialog look the same on every machine.
//!
//! ## The description scrolls inside its own well
//!
//! *"The Description overlaps the todos and terminal."* A `TextEdit` holding more lines than its rectangle
//! grows past it, and nothing clipped the old one. `description::in_a_well` puts it in a `ScrollArea` cut to
//! the well, and the heights below are shared out so the sections can never be given more than there is.
//!
//! ## It is a dialog, not the whole window
//!
//! *"When an agent creates a task it fills the whole window."* It asked for nine tenths of the window, which
//! on a large display is the whole of it. It is at most [`LARGEST`] now, and an agent making a ticket does not
//! open it at all — see the `new-task` command.
//!
//! ## Every field writes through one function
//!
//! Seven controls down the right are seven calls to `AgentTasks::edit_field`, so there is one place a column
//! is written and no second path to drift from it. `Model` and `Effort` are **absent** for a ticket assigned
//! to a person rather than disabled, which is Unluminous's rule and the one place this deliberately differs from
//! the browser.

use egui::{Pos2, Rect, Vec2};
use rux::components::{Button, ButtonSize, ButtonVariant, Select, SubGroup, TextInput, Well};
use rux::icon::Icon;
use rux::layout::Pad;
use rux::text::Style;

use crate::components::modal;
use crate::services::agent_tasks::model::{Assignee, Priority, Status, Task};
use crate::services::agent_tasks::{clock, AgentTasks, Field, TicketKit, EFFORTS};
use crate::services::plugin_ui::{Look, Request};

/// The type size the whole dialog is set in: `rux`'s own body size, which its fields and buttons match.
pub const BODY: f32 = 14.0;

/// The most room the modal asks for, however large the window is.
pub const LARGEST: Vec2 = Vec2::new(1180.0, 820.0);

/// How much of the window it may take before [`LARGEST`] stops it.
const WINDOW_SHARE: f32 = 0.9;

/// The smallest it will ask for, whatever the window is.
///
/// A window dragged down to a few hundred points would otherwise ask for a modal too small to hold the
/// two columns, and `modal::fit` already clamps anything larger than the window, so a floor above the
/// window's own size costs nothing and reads correctly at every size in between.
const SMALLEST_WIDTH: f32 = 720.0;
const SMALLEST_HEIGHT: f32 = 520.0;

/// How big the modal asks to be in this window: nine tenths of it, and never more than [`LARGEST`].
pub fn size(ctx: &egui::Context, _look: &Look<'_>) -> (f32, f32) {
    let window = ctx.content_rect().size();
    let side = |window: f32, largest: f32, smallest: f32| {
        (window * WINDOW_SHARE).min(largest).max(smallest.min(window))
    };
    (side(window.x, LARGEST.x, SMALLEST_WIDTH), side(window.y, LARGEST.y, SMALLEST_HEIGHT))
}

/// How wide the column of fields down the right is, and the dialog width below which it goes underneath.
const ASIDE: f32 = 290.0;
const TWO_COLUMNS: f32 = 700.0;
/// Between the two columns, and between one section and the next.
const GUTTER: f32 = 28.0;
const GAP: f32 = 16.0;
/// A field's caption, the gap under it, and the control.
const CAPTION: f32 = 14.0;
const CONTROL: f32 = 34.0;
/// One section's heading row.
const HEADING: f32 = 22.0;
/// How much of the room left under the description the agent's terminal takes, and its two bounds.
const TERMINAL_SHARE: f32 = 0.34;
const TERMINAL_SMALLEST: f32 = 150.0;
const TERMINAL_LEAST: f32 = 90.0;
const TERMINAL_LARGEST: f32 = 520.0;
/// What the comments want, and the least they can be given.
const COMMENTS: f32 = 190.0;
const COMMENTS_LEAST: f32 = 100.0;
/// The least the description can be given.
const DESCRIPTION_LEAST: f32 = 140.0;
/// What one section after the description takes before its body: eight points, `SubGroup`'s hairline and the
/// fourteen points under it, its twenty point heading, and eight more.
const SECTION: f32 = 8.0 + 14.0 + 20.0 + 8.0;

/// A field's caption: small mono capitals, which is `rux`'s heading face.
const CAPTION_STYLE: Style = Style::mono(10.5).medium().tracking(0.12).upper();
/// A quiet line under a field saying what it is for.
const HELP_STYLE: Style = Style::sans(12.0).leading(1.4);

/// What the modal reported.
#[derive(Debug, Default)]
pub struct Outcome {
    pub requests: Vec<Request>,
    /// The modal was closed, by its cross, by `Escape`, by a click outside it, or by its footer.
    pub closed: bool,
}

/// Draw the ticket that is open in the detail, as a modal. Does nothing when none is.
pub fn show(board: &mut AgentTasks, ctx: &egui::Context, look: &Look<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let Some(task) = board.detail().task.clone() else {
        return outcome;
    };
    // A ticket nobody has named yet is a new one, and the footer says so: `Discard` deletes the row rather than
    // closing the modal, because `+ Add Task` created it before anybody typed.
    let new = board.detail().is_new;
    let (width, height) = size(ctx, look);
    // **One type scale for the whole dialog**, whatever the editor is set to — see the module comment.
    let look = look.clone().at_a_fixed_size(BODY).flat();
    // Taken out of the board for the length of the drawing, because the drawing needs the board too.
    let mut kit = board.ticket_kit.take().unwrap_or_default();
    let (inner, should_close) =
        modal::show(ctx, "agent-tasks-ticket", width, height, |ui, area| {
            contents(board, &mut kit, ui, area, &look, &task, new)
        });
    kit.rux.end_frame();
    board.ticket_kit = Some(kit);
    outcome.requests = inner.requests;
    outcome.closed = inner.closed || should_close;
    outcome
}

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
    // **The key and the title**, which is what the page this is modelled on puts in its header: the key in a
    // dim monospaced face and the title beside it.
    let (key, heading) = match new {
        true => (None, "New task".to_owned()),
        false => (Some(task.key.as_str()), board.detail().title_draft.clone()),
    };
    if modal::header_of(ui, area, key, &heading) {
        outcome.closed = true;
    }
    let body = modal::body(area);
    let footer = Rect::from_min_max(Pos2::new(area.min.x, body.max.y), area.max);

    let main_id = ui.id().with("agent-tasks-ticket-rux");
    let state = &kit.rux;
    let selects = &mut kit.selects;
    let (requests, closed) = rux::layer(ui, state, main_id, area, |rux| {
        let mut requests = Vec::new();
        if body.width() >= TWO_COLUMNS {
            let split = (body.max.x - ASIDE).round();
            let main = Rect::from_min_max(body.min, Pos2::new(split - GUTTER, body.max.y));
            let aside = Rect::from_min_max(Pos2::new(split, body.min.y), body.max);
            let divider = split - GUTTER / 2.0;
            rux.chrome.line(
                Pos2::new(divider, body.min.y),
                Pos2::new(divider, body.max.y),
                1.0,
                rux.theme().surface.sunken,
            );
            requests.extend(main_column(board, rux, main, look, task, new));
            requests.extend(fields(board, rux, selects, aside, look, task, new));
        } else {
            // One column: the fields first and never more than half the height, scrolled inside it, then the
            // rest — which is what the browser board's own narrow layout does.
            let fields_at = Rect::from_min_max(
                body.min,
                Pos2::new(body.max.x, body.min.y + (body.height() * 0.45).min(330.0)),
            );
            let rest = Rect::from_min_max(Pos2::new(body.min.x, fields_at.max.y + GAP), body.max);
            requests.extend(fields(board, rux, selects, fields_at, look, task, new));
            if rest.height() > 120.0 {
                requests.extend(main_column(board, rux, rest, look, task, new));
            }
        }
        let (closed, more) = footer_row(board, rux, footer, new);
        requests.extend(more);
        (requests, closed)
    });
    outcome.requests.extend(requests);
    outcome.closed |= closed;
    outcome
}

/// The footer: what a new ticket says about saving, and the buttons.
///
/// `rux` buttons, because the dialog above them is `rux`. A new ticket gets `Discard` and `Done`; one that
/// exists gets `Close`. **The command key with Enter presses the last of them**, not Enter on its own: the
/// description and the comment box are multiline fields where Enter is a new line, which is the commit
/// panel's reason for the same choice.
fn footer_row(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    footer: Rect,
    new: bool,
) -> (bool, Vec<Request>) {
    let mut requests = Vec::new();
    let theme = rux.theme();
    rux.chrome.line(
        Pos2::new(footer.left() + 20.0, footer.top()),
        Pos2::new(footer.right() - 20.0, footer.top()),
        1.0,
        theme.surface.sunken,
    );
    let middle = footer.center().y;
    let height = 36.0;
    let labels: &[&str] = match new {
        true => &["Discard", "Done"],
        false => &["Close"],
    };
    let mut right = footer.right() - 20.0;
    let mut pressed = None;
    for (index, label) in labels.iter().enumerate().rev() {
        let mut button = Button::new(label).size(ButtonSize::Small);
        if index == labels.len() - 1 && new {
            button = button.primary();
        }
        let width = button.measure(rux).x.max(96.0);
        let at = Rect::from_min_size(
            Pos2::new(right - width, middle - height / 2.0),
            Vec2::new(width, height),
        );
        if button.show(rux, at).clicked() {
            pressed = Some(index);
        }
        right = at.left() - 10.0;
    }
    let chord = rux
        .ctx()
        .input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.command_only());
    if chord {
        pressed = Some(labels.len() - 1);
    }
    if new {
        let galley = rux.text(Style::sans(12.0), "Starts saving as you type", theme.ink.i400);
        rux::text::draw_left_centre(
            rux.ui.painter(),
            Pos2::new(footer.left() + 22.0, middle),
            galley,
            theme.ink.i400,
        );
    }
    let closed = match (new, pressed) {
        (_, None) => false,
        (true, Some(0)) => match board.discard_the_ticket() {
            Ok(()) => true,
            Err(problem) => {
                requests.push(Request::Message(problem));
                false
            }
        },
        _ => true,
    };
    (closed, requests)
}

/// The description, the todos, the terminal and the comments.
///
/// ## The heights add up, and that is the whole of this function's difficulty
///
/// The four sections are laid out one under another, so a budget that overflows draws the last section off
/// the bottom edge and the one before it over its own buttons. Every section says what it **wants** and what it
/// can be cut to, and the shortfall is taken from them in order — the terminal first, then the comments, then
/// the todos, and the description last, because a ticket is opened to write in far more often than to read an
/// agent's scrollback. Whatever is left over goes to the description, which scrolls inside whatever it gets.
fn main_column(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    area: Rect,
    look: &Look<'_>,
    task: &Task,
    new: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let theme = rux.theme();
    let mut pen = area.min.y;

    // The title, which is in the header on a ticket that exists. A **new** one has no title yet and this is
    // where it is typed, because a header is not a field.
    if new {
        let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(area.width(), 46.0));
        let mut title = board.detail().title_draft.clone();
        let typed = TextInput::new(&mut title)
            .hint("What needs doing?")
            .style(Style::sans(16.0).medium())
            .label("Ticket title")
            .id_salt("agent-tasks-ticket-title")
            .show(rux, at);
        if typed.changed {
            board.detail_mut().title_draft = title;
            if let Err(problem) = board.save_the_title() {
                requests.push(Request::Message(problem));
            }
        }
        pen = at.max.y + GAP;
    }

    let todos_open = !board.todos_shut;
    let terminal_open = !board.terminal_shut;
    let room = area.max.y - pen;
    // A section that is shut wants its heading and nothing else, which is what makes shutting one worth doing.
    // The todos want a row each and one more for the box that adds one, inside the well's own padding; the
    // least they can be given is one todo and that box, because a todo list with no room for a todo is a strip.
    let row = look.row_height;
    let (todo_want, todo_least) = match (new, todos_open) {
        (true, _) | (_, false) => (0.0, 0.0),
        _ => {
            let rows = board.detail().todos.len() as f32;
            let well = 12.0;
            (((rows + 1.0) * row + well).min(6.0 * row + well), (rows.min(2.0) + 1.0) * row + well)
        }
    };
    let (terminal_want, terminal_least) = match (new, terminal_open) {
        (true, _) | (_, false) => (0.0, 0.0),
        _ => ((room * TERMINAL_SHARE).clamp(TERMINAL_SMALLEST, TERMINAL_LARGEST), TERMINAL_LEAST),
    };
    let (comment_want, comment_least) = match new {
        true => (0.0, 0.0),
        false => (COMMENTS, COMMENTS_LEAST),
    };
    // The headings and the gaps round them, which are room nothing else can have.
    let headings = match new {
        true => HEADING + 8.0,
        false => HEADING + 8.0 + SECTION * 3.0,
    };
    let description_want = (room - headings - todo_want - terminal_want - comment_want).max(0.0);
    let mut short = (headings
        + description_want.max(DESCRIPTION_LEAST)
        + todo_want
        + terminal_want
        + comment_want
        - room)
        .max(0.0);
    let give = |want: f32, least: f32, short: &mut f32| -> f32 {
        let spare = (want - least).max(0.0).min(*short);
        *short -= spare;
        want - spare
    };
    let terminal_height = give(terminal_want, terminal_least, &mut short);
    let comment_height = give(comment_want, comment_least, &mut short);
    let todo_height = give(todo_want, todo_least, &mut short);
    let description_height =
        give(description_want.max(DESCRIPTION_LEAST), DESCRIPTION_LEAST, &mut short);
    // **Whatever is still short comes off the description**, the one section that scrolls, so a modal dragged
    // down to its smallest still adds up rather than running off the bottom.
    let description_height = (description_height - short).max(0.0);

    // ------------------------------------------------------------------ the description
    heading_row(rux, Pos2::new(area.min.x, pen), "Description", Icon::Docs, theme.accent.blue);
    // The two view buttons, on the heading's own row and right aligned, which is where a section's controls go.
    if let Some(rendered) = super::raw_or_rendered(
        rux.ui,
        look,
        Rect::from_min_size(Pos2::new(area.min.x, pen + 1.0), Vec2::new(area.width(), 18.0)),
        "the description",
        board.detail().description_rendered,
    ) {
        board.show_the_description_rendered(rendered);
    }
    pen += HEADING + 8.0;
    let well = Rect::from_min_size(
        Pos2::new(area.min.x, pen),
        Vec2::new(area.width(), description_height),
    );
    if well.height() > 24.0 {
        let inside = Well::new().pad(Pad::axes(12.0, 14.0)).show(rux, well);
        requests.extend(super::description::in_a_well(
            board,
            rux.ui,
            well,
            inside,
            look,
            theme.ink.i900,
        ));
    }
    pen = well.max.y;
    if new {
        return requests;
    }

    // ------------------------------------------------------------------ the todos
    //
    // **Todos and the terminal fold**, which is what the page this is modelled on does and what makes a ticket
    // with a long conversation on it readable at all. The flags are the provider's, so a section left shut
    // stays shut while the board is refreshed under it.
    let title = format!("Todos \u{b7} {}/{}", task.todo_done_count, task.todo_count);
    let (toggled, body) =
        folding_section(rux, &mut pen, area, &title, Icon::Check, theme.accent.mint, todos_open);
    if toggled {
        board.todos_shut = todos_open;
    }
    if todos_open && todo_height > 0.0 {
        let at = Rect::from_min_size(body, Vec2::new(area.width(), todo_height));
        let inside = Well::new().shallow().pad(Pad::axes(6.0, 12.0)).show(rux, at);
        requests.extend(super::detail::todo_rows(board, rux.ui, inside, look));
        pen = at.max.y;
    }

    // ------------------------------------------------------------------ the agent's terminal
    let attached =
        board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
    let title = match attached {
        true => format!("Agent terminal \u{b7} live \u{b7} {}", task.key),
        false => "Agent terminal".to_owned(),
    };
    let accent = match attached {
        true => theme.semantic.success,
        false => theme.ink.i400,
    };
    let (toggled, body) =
        folding_section(rux, &mut pen, area, &title, Icon::Spark, accent, terminal_open);
    if toggled {
        board.terminal_shut = terminal_open;
    }
    if terminal_open && terminal_height > 0.0 {
        let at = Rect::from_min_size(body, Vec2::new(area.width(), terminal_height));
        let inside = Well::new().pad(Pad::all(6.0)).show(rux, at);
        if inside.height() > 20.0 {
            requests
                .extend(super::detail::terminal_section(board, rux.ui, inside, look, task, false));
        }
        pen = at.max.y;
    }

    // ------------------------------------------------------------------ the comments
    //
    // A heading that does not fold, with the same hairline over it, because the comments are what the other
    // sections are folded to make room for.
    rux.chrome.line(
        Pos2::new(area.min.x, pen + 8.0),
        Pos2::new(area.max.x, pen + 8.0),
        1.0,
        theme.surface.sunken,
    );
    let title = format!("Comments \u{b7} {}", task.comment_count);
    heading_row(
        rux,
        Pos2::new(area.min.x, pen + 8.0 + 14.0),
        &title,
        Icon::Chat,
        theme.accent.violet,
    );
    pen += SECTION;
    let comments_at = Rect::from_min_max(
        Pos2::new(area.min.x, pen),
        Pos2::new(area.max.x, (pen + comment_height).min(area.max.y)),
    );
    if comments_at.height() > 20.0 {
        requests.extend(super::detail::comment_section(board, rux.ui, comments_at, look));
    }
    requests
}

/// A section that folds: `rux`'s `SubGroup`, a hairline over a heading that is the whole fold control.
///
/// Moves `pen` past the heading and answers whether it was pressed and where the section's body starts.
fn folding_section(
    rux: &mut rux::Rux<'_>,
    pen: &mut f32,
    area: Rect,
    title: &str,
    icon: Icon,
    accent: egui::Color32,
    open: bool,
) -> (bool, Pos2) {
    let group = SubGroup::new(title, icon, accent, open);
    let head = group.head_height(rux);
    let shown = group.show(
        rux,
        Rect::from_min_size(Pos2::new(area.min.x, *pen + 8.0), Vec2::new(area.width(), head)),
    );
    *pen += SECTION;
    (shown.toggled, Pos2::new(area.min.x, *pen))
}

/// A section's heading that does not fold: its mark in the section's accent, and its name in capitals.
///
/// **Named**, because every control and every heading over one has a plain name a test and an agent find it
/// by, in the case a person reads; the drawing is what shouts.
fn heading_row(rux: &mut rux::Rux<'_>, at: Pos2, name: &str, icon: Icon, accent: egui::Color32) {
    let theme = rux.theme();
    let middle = at.y + HEADING / 2.0;
    rux.mark(rux::icon::Mark::new(icon, 13.0), Pos2::new(at.x + 6.5, middle), accent);
    let galley = rux.text(CAPTION_STYLE, name, theme.ink.i700);
    let width = galley.size().x;
    rux::text::draw_left_centre(
        rux.ui.painter(),
        Pos2::new(at.x + 21.0, middle),
        galley,
        theme.ink.i700,
    );
    let area = Rect::from_min_size(at, Vec2::new(width + 21.0, HEADING));
    let response = rux.ui.interact(
        area,
        rux.ui.id().with(("agent-tasks-heading", name)),
        egui::Sense::hover(),
    );
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, named.clone()));
}

/// Everything that is a property of the ticket rather than its contents.
///
/// **In the order the page this is modelled on has them**: the one button somebody opens a ticket to press is
/// at the top, then the seven things about the ticket, then the JIRA issue and when it was made, and `Delete
/// task` last and in the coral the reference keeps for destruction.
///
/// **It scrolls, because it cannot be made to fit** in a dialog dragged towards its smallest — every one of
/// these is a thing a ticket needs before an agent can be started. The column is drawn into a `rux` layer of its
/// own inside the scrolling area, because the decoration has to move with what it decorates.
fn fields(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    area: Rect,
    look: &Look<'_>,
    task: &Task,
    new: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let state = rux.state;
    let mut inside = rux.ui.new_child(egui::UiBuilder::new().max_rect(area));
    inside.set_clip_rect(area.intersect(rux.ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("agent-tasks-ticket-fields")
        .auto_shrink([false, false])
        .show(&mut inside, |ui| {
            let top = ui.cursor().min.y;
            let content = Rect::from_min_size(
                Pos2::new(area.min.x, top),
                Vec2::new(area.width() - 10.0, 900.0),
            );
            let used = rux::layer(
                ui,
                state,
                egui::Id::new("agent-tasks-ticket-fields-rux"),
                content.expand(24.0),
                |rux| {
                    let (asked, used) = field_column(board, rux, selects, content, look, task, new);
                    requests = asked;
                    used
                },
            );
            // What the scrollbar measures itself against: the column's own rectangle, included rather than
            // allocated after whatever is already there. The JIRA field's text box is put into this `Ui` and
            // has already moved its cursor, so allocating the height again counted most of the column twice
            // and let it scroll a screenful past its last control.
            ui.expand_to_include_rect(Rect::from_min_size(
                Pos2::new(area.min.x, top),
                Vec2::new(area.width() - 10.0, (used - top).max(0.0)),
            ));
        });
    requests
}

/// The fields themselves, answering where the last of them ended.
#[allow(clippy::too_many_lines)]
fn field_column(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    area: Rect,
    _look: &Look<'_>,
    task: &Task,
    new: bool,
) -> (Vec<Request>, f32) {
    let mut requests = Vec::new();
    let theme = rux.theme();
    let width = area.width();
    let mut pen = area.min.y;

    // ---------------------------------------------------------------- the one thing to press
    //
    // At the top, which is where the reference puts it and where somebody opening a ticket to start an agent
    // looks first. Absent when it cannot apply: a ticket with an agent already running offers `Stop` instead,
    // and a new one offers nothing at all because it has no title yet.
    if !new {
        let attached =
            board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
        let (label, command, icon) = match () {
            _ if attached => ("Stop", "stop", Icon::Pause),
            _ if task.session_id.is_none() => ("Start Work", "start", Icon::Play),
            _ if crate::services::agent_tasks::agent::can_resume(task.assignee) => {
                ("Resume session", "resume", Icon::Play)
            }
            // **`Start Work again`, not `Resume session`.** Codex names its own sessions, so the id on a
            // Codex ticket is only Unluminous's marker that a worker was here and there is no conversation to
            // hand back. The label says `again` because it is a new conversation.
            _ => ("Start Work again", "start", Icon::Play),
        };
        let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, 44.0));
        let button = match attached {
            true => Button::new(label).variant(ButtonVariant::Danger),
            false => Button::new(label).primary(),
        };
        if button.icon(icon).size(ButtonSize::Large).stretch().show(rux, at).clicked() {
            match board.command_now(command, std::slice::from_ref(&task.key)) {
                Ok(answer) if !answer.message.is_empty() => {
                    requests.push(Request::Message(answer.message))
                }
                Ok(_) => {}
                Err(problem) => requests.push(Request::Message(problem)),
            }
        }
        pen += 44.0 + 18.0;
    }

    // ---------------------------------------------------------------- what the ticket is
    if !new {
        let options: Vec<(String, String)> = Status::ALL
            .iter()
            .map(|status| (status.name().to_owned(), status.label().to_owned()))
            .collect();
        if let Some(chosen) = choice(
            rux,
            selects,
            &mut pen,
            area.min.x,
            width,
            "Status",
            &options,
            task.status.name(),
            None,
        ) {
            if let Some(status) = Status::parse(&chosen) {
                if let Err(problem) = board.move_card(task.id, status, i64::MAX) {
                    requests.push(Request::Message(problem));
                }
            }
        }
    }

    let options: Vec<(String, String)> = Assignee::ALL
        .iter()
        .map(|assignee| (assignee.name().to_owned(), assignee.name().to_owned()))
        .collect();
    if let Some(chosen) = choice(
        rux,
        selects,
        &mut pen,
        area.min.x,
        width,
        "Assignee",
        &options,
        task.assignee.name(),
        None,
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
            &models,
            &model,
            Some("The agent's default"),
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
            &efforts,
            task.effort.as_deref().unwrap_or(""),
            Some("Model default"),
        ) {
            requests.extend(write(board, task, Field::Effort(chosen)));
        }
        pen = help(rux, pen, area.min.x, width, "How hard the agent thinks before it answers.");
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
        &projects,
        &project,
        Some("The folder this window has open"),
    ) {
        requests.extend(write(board, task, Field::Project(chosen)));
    }
    pen = help(rux, pen, area.min.x, width, "The folder the agent's terminal opens in.");

    let options: Vec<(String, String)> = Priority::ALL
        .iter()
        .map(|priority| (priority.name().to_owned(), priority.name().to_owned()))
        .collect();
    if let Some(chosen) = choice(
        rux,
        selects,
        &mut pen,
        area.min.x,
        width,
        "Priority",
        &options,
        task.priority.name(),
        None,
    ) {
        requests.extend(write(board, task, Field::Priority(chosen)));
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
        &epics,
        &task.epic_id.map(|id| id.to_string()).unwrap_or_default(),
        Some("No epic"),
    ) {
        requests.extend(write(board, task, Field::Epic(chosen)));
    }

    if new {
        return (requests, pen);
    }

    // ---------------------------------------------------------------- the JIRA issue, and when it was made
    //
    // **It does not sync.** There is no HTTP client in Unluminous, so the key is a field somebody types. Copy
    // hands over the row's own `jira_url` when it has one and the key otherwise.
    pen = caption(rux, pen, area.min.x, "JIRA");
    let mut key = task.jira_key.clone().unwrap_or_default();
    let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, CONTROL));
    let typed = TextInput::new(&mut key)
        .hint("No issue")
        .style(Style::CONTROL)
        .pad(Pad::axes(8.0, 12.0))
        .label("JIRA")
        .id_salt("agent-tasks-ticket-jira")
        .show(rux, at);
    if typed.changed {
        requests.extend(write(board, task, Field::JiraKey(key.clone())));
    }
    pen += CONTROL + 8.0;
    if !key.trim().is_empty() {
        let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width.min(150.0), 30.0));
        if Button::new("Copy issue link")
            .icon(Icon::Copy)
            .size(ButtonSize::Mini)
            .show(rux, at)
            .clicked()
        {
            requests.push(Request::Copy(board.jira_link(&key)));
            requests.push(Request::Message(format!("copied the link to {key}")));
        }
        pen += 30.0 + 8.0;
    }
    pen += 8.0;

    pen = caption(rux, pen, area.min.x, "Created");
    let now = clock::now();
    let said = clock::relative(&task.created_at, &now);
    let galley = rux.text(Style::sans(13.0), &said, theme.ink.i700);
    let tall = galley.size().y;
    rux.ui.painter().galley(Pos2::new(area.min.x, pen), galley, theme.ink.i700);
    pen += tall + 24.0;

    // ------------------------------------------------------------ and the one thing that destroys work
    //
    // Last, in the coral `rux` keeps for destruction and never as a filled button. Pressed once it says what it
    // will do, pressed twice it does it: deleting a ticket takes its todos and comments with it, so it is the
    // one control here that asks.
    let asking = board.delete_asked;
    let at = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::new(width, CONTROL));
    match asking {
        false => {
            if Button::new("Delete task")
                .variant(ButtonVariant::Danger)
                .icon(Icon::Trash)
                .stretch()
                .show(rux, at)
                .clicked()
            {
                board.delete_asked = true;
            }
        }
        true => {
            let half = (width - 10.0) / 2.0;
            let keep = Rect::from_min_size(at.min, Vec2::new(half, CONTROL));
            let really = Rect::from_min_size(
                Pos2::new(at.min.x + half + 10.0, at.min.y),
                Vec2::new(half, CONTROL),
            );
            if Button::new("Keep it").stretch().show(rux, keep).clicked() {
                board.delete_asked = false;
            }
            if Button::new("Delete for good")
                .variant(ButtonVariant::Danger)
                .icon(Icon::Trash)
                .stretch()
                .show(rux, really)
                .clicked()
            {
                board.delete_asked = false;
                if let Err(problem) = board.discard_the_ticket() {
                    requests.push(Request::Message(problem));
                }
            }
        }
    }
    pen += CONTROL + 12.0;
    (requests, pen)
}

/// A field's caption, answering where the control under it goes.
fn caption(rux: &mut rux::Rux<'_>, pen: f32, left: f32, name: &str) -> f32 {
    let theme = rux.theme();
    let galley = rux.text(CAPTION_STYLE, name, theme.ink.i500);
    rux.ui.painter().galley(Pos2::new(left + 2.0, pen), galley, theme.ink.i500);
    pen + CAPTION + 6.0
}

/// A quiet line under a field saying what it is for, answering where the next field goes.
fn help(rux: &mut rux::Rux<'_>, pen: f32, left: f32, width: f32, said: &str) -> f32 {
    let theme = rux.theme();
    let galley = rux::text::wrapped(rux.ui.painter(), HELP_STYLE, said, theme.ink.i400, width);
    let tall = galley.size().y;
    rux.ui.painter().galley(Pos2::new(left + 2.0, pen - 8.0), galley, theme.ink.i400);
    pen - 8.0 + tall + 14.0
}

/// A named value chosen from a list, answering what was chosen when it changed.
///
/// `rux`'s `Select`, with the field's name in capitals over it. `options` is `(value, said)` pairs — the value
/// written to the row and the words a person reads. `empty` is what the list calls holding nothing, for a field
/// that may, and it is the first row; `None` means the field is required.
#[allow(clippy::too_many_arguments)]
fn choice(
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    pen: &mut f32,
    left: f32,
    width: f32,
    name: &'static str,
    options: &[(String, String)],
    chosen: &str,
    empty: Option<&str>,
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
    let at = Rect::from_min_size(Pos2::new(left, *pen), Vec2::new(width, CONTROL));
    let state = selects.entry(name).or_default();
    let outcome = Select::new(&said, selected).label(name).placeholder("—").show(rux, at, state);
    *pen += CONTROL + 12.0;
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

    /// `task-2193`: on a large display the ticket filled the window. Nine tenths of a small window, and never
    /// more than [`LARGEST`] of a large one.
    #[test]
    fn a_large_window_gets_a_dialog_rather_than_a_ticket_that_fills_it() {
        let renderer = TextRenderer::new();
        let look = Look::of(&Settings::new(), &renderer);
        assert_eq!(size(&a_context(2560.0, 1440.0), &look), (LARGEST.x, LARGEST.y));
        assert_eq!(size(&a_context(1000.0, 800.0), &look), (900.0, 720.0));
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
}
