//! One ticket: its title, its description, its todos, its comments and its terminal.
//!
//! The description is markdown and Unluminous already reads markdown, so it is drawn by the same reader the
//! Markdown preview uses rather than by a second one written here. That is the largest single saving in
//! the plugin and it falls out of the plugin being inside a text editor.

use egui::{CornerRadius, Pos2, Rect, Sense, Vec2};

use super::{clipped, text};
use crate::services::agent_tasks::{clock, AgentTasks};
use crate::services::plugin_ui::{Look, Request};
use crate::theme::crisp::CrispPainter;
use crate::theme::icon;

// **The in-place ticket is gone.** This file used to draw a whole ticket inside the board's own rectangle —
// the pane's narrow column, and the right hand half of the tab — and `task-1771` asked for that to stop: a
// ticket is the modal and nothing else, so a board that split itself in two the moment an agent read a
// ticket is a board that rearranged itself under somebody's hands. `show` and `todos_and_comments` went
// with it, along with the three measurements only they used. What is left is what the modal lays out, which
// was always shared between the two and is now called from one place.

/// The ticket's own terminal: the real one.
///
/// `components::terminal_panel::grid` is what the terminal tile and the run tile are both made of, so this is
/// the same emulator, the same colours, the same selection, the same clipboard rules and **the keyboard into
/// the program**, which is the whole of what the ticket meant by terminal chat: an agent asking a question
/// deserves an answer typed at it. Painting a picture of a terminal instead, which is what this was, gave a
/// board that could watch an agent and not talk to it.
pub(crate) fn terminal(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    task_id: i64,
    focused: bool,
) -> Vec<Request> {
    let painter = ui.painter().clone();
    painter.rect_filled(
        Rect::from_min_size(area.min, Vec2::new(area.width(), 1.0)),
        0,
        look.palette.divider,
    );
    let grid_area = Rect::from_min_max(Pos2::new(area.min.x, area.min.y + 1.0), area.max);
    let monospace = look.monospace_size;
    let opacity = look.opacity;
    let renderer = look.renderer;
    let mut selecting = board.terminal_selecting;
    let outcome = {
        let session = board.terminal_for_mut(task_id).map(|terminal| &mut terminal.session);
        crate::components::terminal_panel::grid(
            ui,
            grid_area,
            session,
            &mut selecting,
            focused,
            "agent-tasks-terminal",
            "No terminal for this ticket. Press Start to launch its agent, or Resume session to hand its \
             conversation back.",
            renderer,
            monospace,
            opacity,
        )
    };
    board.terminal_selecting = selecting;
    let mut requests: Vec<Request> = Vec::new();
    if outcome.take_focus {
        // Both halves, because they are two different things. `UnluminousApp::focus` is the one value that says who in
        // the window has the keyboard, and a plugin that set only its own flag left the editing area holding the
        // keys as well, so one press reached both. And this flag is which part of the **board** the keys go to,
        // which the window cannot know: it hands the keyboard over the same way when somebody clicks the lanes.
        requests.push(Request::TakeTheKeyboard(true));
        board.focus_the_terminal(true);
    }
    if let Some(text) = outcome.copy {
        requests.push(Request::Copy(text));
    }
    // No repaint asked for here. The session has the window's own waker, so it asks for a frame when it prints,
    // and a terminal that is alive and quiet needs none: asking every frame while an agent sits at its prompt
    // was the window drawing for ever for nothing.
    requests
}

/// The todo rows on their own, for the modal, which lays its sections out itself.
///
/// The same rows the pane draws, so a todo ticked in one place is a todo ticked in the other: this is the one
/// function that draws them and the pane and the modal both call it.
pub(crate) fn todo_rows(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let painter = ui.painter().clone();
    let todos = board.detail().todos.clone();
    // How far the list is scrolled, before it is drawn: the wheel over this section moves the todos rather than
    // the lane behind them.
    let content = todos.len() as f32 * look.row_height;
    let room = area.height() - look.row_height;
    board.scroll_the_todos(ui, area, (content - room).max(0.0));
    let mut pen = area.min.y - board.todo_scroll;
    let mut toggled = None;
    let mut removed = None;
    for todo in &todos {
        // Scrolled rather than stopped: a hundred todos used to draw as many as fit and then nothing, so a todo
        // past the fold could not be ticked or removed.
        if pen + look.row_height < area.min.y {
            pen += look.row_height;
            continue;
        }
        if pen + look.row_height > area.max.y - look.row_height {
            break;
        }
        let box_at = Rect::from_min_size(Pos2::new(area.min.x, pen + 4.0), Vec2::splat(14.0));
        let response = ui.interact(
            box_at.expand(3.0),
            ui.id().with(("agent-tasks-modal-todo", todo.id)),
            Sense::click(),
        );
        // Named as a tick box, for the reason the pane's todos are: see `todo_section`.
        let said = todo.text.clone();
        let ticked = todo.done;
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                ui.is_enabled(),
                ticked,
                said.clone(),
            )
        });
        painter.rect(
            box_at,
            CornerRadius::same(3),
            match todo.done {
                true => look.palette.accent,
                false => look.palette.field,
            },
            egui::Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
        if todo.done {
            icon::tick(&painter, box_at.center(), look.palette.text_strong);
        }
        if response.clicked() {
            toggled = Some((todo.id, !todo.done));
        }
        let tint = match todo.done {
            true => look.palette.text_dim,
            false => look.palette.text,
        };
        clipped(
            &painter,
            Pos2::new(area.min.x + 22.0, pen + 2.0),
            &todo.text,
            look.font_size - 0.5,
            tint,
            area.width() - 52.0,
            1,
        );
        // A cross that removes it. The browser's todos are read only because the agent writes its own plan; here
        // they can be written, and a list that can be added to and not removed from is half a list.
        let cross = Rect::from_center_size(
            Pos2::new(area.max.x - 12.0, pen + look.row_height / 2.0 - 2.0),
            Vec2::splat(18.0),
        );
        if crate::components::controls::icon_button(
            ui,
            cross,
            &format!("Remove {}", todo.text),
            icon::cross,
        ) {
            removed = Some(todo.id);
        }
        pen += look.row_height;
    }
    if let Some((id, done)) = toggled {
        if let Err(problem) = board.set_todo(id, done) {
            requests.push(Request::Message(problem));
        }
    }
    if let Some(id) = removed {
        if let Err(problem) = board.remove_the_todo(id) {
            requests.push(Request::Message(problem));
        }
    }
    // The box that adds one, at the foot of the section.
    let at = Rect::from_min_size(
        Pos2::new(area.min.x, area.max.y - look.row_height),
        Vec2::new(area.width(), look.row_height),
    );
    painter.rect(
        at,
        CornerRadius::same(look.corner_radius as u8),
        look.palette.field,
        egui::Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    let mut draft = board.detail().todo_draft.clone();
    let todo_id = ui.id().with("agent-tasks-todo-draft");
    let response = ui.put(
        crate::components::controls::field_takes_the_whole_rectangle_at(
            ui,
            at,
            8.0,
            todo_id,
            "Todo field",
            &egui::FontId::proportional(look.font_size - 0.5),
        ),
        egui::TextEdit::singleline(&mut draft)
            .id(todo_id)
            .frame(egui::Frame::NONE)
            .hint_text(crate::components::controls::placeholder(
                "Add a todo",
                &egui::FontId::proportional(look.font_size - 0.5),
                look.palette.text_faint,
            ))
            .font(egui::FontId::proportional(look.font_size - 0.5))
            .text_color(look.palette.text),
    );
    if response.changed() {
        board.detail_mut().todo_draft = draft;
    }
    if super::enter_was_used_and_pressed(ui, &response) {
        if let Err(problem) = board.post_the_todo() {
            requests.push(Request::Message(problem));
        }
        response.request_focus();
    }
    requests
}

/// The terminal, with the header the browser board puts over it: the word, whether it is attached, and the
/// button that hands the conversation back when it is not.
/// `heading` says whether to draw a row naming the section. The ticket modal draws its own — a disclosure
/// that folds the terminal away, with the ticket's key and whether it is live in it — so it asks for none,
/// and the grid takes the whole rectangle. `task-1771`: one heading, not two under each other.
pub(crate) fn terminal_section(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
    task: &crate::services::agent_tasks::model::Task,
    heading: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let painter = ui.painter().clone();
    let attached =
        board.terminal_for(task.id).is_some_and(|terminal| terminal.session.is_running());
    let mut top = area.min.y;
    if heading {
        let head = Rect::from_min_size(area.min, Vec2::new(area.width(), 22.0));
        let mut pen = head.min.x;
        pen += text(
            &painter,
            Pos2::new(pen, head.min.y),
            "Terminal",
            look.font_size - 1.5,
            look.palette.text_dim,
        );
        let (said, tint) = match attached {
            true => ("attached", look.palette.added),
            false => ("detached", look.palette.text_faint),
        };
        text(&painter, Pos2::new(pen + 8.0, head.min.y), said, look.font_size - 2.0, tint);
        // Only a ticket that already has a session gets a button here. Starting an agent is Start work's
        // job, so there is no second control that does it.
        if !attached && task.session_id.is_some() {
            let at = Rect::from_min_size(
                Pos2::new(area.max.x - 110.0, head.min.y - 3.0),
                Vec2::new(110.0, 22.0),
            );
            if crate::components::controls::choice_button(ui, at, "Resume session", false) {
                match board.command_now("resume", std::slice::from_ref(&task.key)) {
                    Ok(answer) if !answer.message.is_empty() => {
                        requests.push(Request::Message(answer.message))
                    }
                    Ok(_) => {}
                    Err(problem) => requests.push(Request::Message(problem)),
                }
            }
        }
        top = head.max.y + 2.0;
    }
    let grid = Rect::from_min_max(Pos2::new(area.min.x, top), area.max);
    requests.extend(terminal(board, ui, look, grid, task.id, board.terminal_focused));
    requests
}

/// How tall a comment's header row is: its author and time, and the buttons beside them.
const HEAD: f32 = 22.0;
/// How much of that row's right hand end the two view buttons take, with the gap before what is left of them.
const VIEW_BUTTONS: f32 = 18.0 * 2.0 + 4.0 + 6.0;

/// How tall the box a comment is written in is, and the two buttons beside or under it.
const COMPOSER: f32 = 36.0;
/// How wide the comment box has to be before the two buttons go beside it rather than under it.
const COMPOSER_ON_ONE_ROW: f32 = 300.0;

/// How tall the box that writes a comment and its two buttons are together, in a section `width` wide.
fn composer_height(width: f32, buttons: f32) -> f32 {
    match width - buttons >= COMPOSER_ON_ONE_ROW {
        true => COMPOSER,
        false => COMPOSER * 2.0 + 8.0,
    }
}

/// How wide a comment's small button is for `label`: its word and a little room either side.
fn comment_button_width(ui: &egui::Ui, look: &Look<'_>, label: &str) -> f32 {
    let font = egui::FontId::proportional(look.less(3.0));
    ui.painter().crisp_layout_no_wrap(label.to_owned(), font, look.palette.accent).size().x + 16.0
}

/// One of the small buttons on a comment's own header row: `Edit`, `Send to terminal`, `Save` and `Cancel`.
///
/// `task-2214`: *"Post comment & send to terminal buttons need better styling."* These were the window's choice
/// buttons, a grey outline round grey words, four of them on every comment a person wrote, which made a list of
/// comments a list of boxes. They are now words in the accent colour with no frame until the pointer is on one,
/// when a pill comes up behind them, which is how a quiet action beside text reads everywhere else. `strong` is
/// the one that commits, `Save`, which is filled.
fn comment_button(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    label: &str,
    announced: &str,
    strong: bool,
    comment: i64,
) -> bool {
    // The comment's id is in the widget's id, because two comments by one author in the same minute announce
    // the same words.
    let id = ui.id().with(("agent-tasks-comment-button", comment, label));
    let response = ui.interact(area, id, Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    let round = CornerRadius::same((area.height() / 2.0) as u8);
    match (strong, hovered) {
        (true, _) => {
            painter.rect_filled(area, round, look.palette.board_accent);
        }
        (false, true) => {
            painter.rect_filled(area, round, look.palette.control);
        }
        (false, false) => {}
    }
    let tint = match (strong, hovered) {
        (true, _) | (_, true) => look.palette.text_strong,
        _ => look.palette.accent,
    };
    let font = egui::FontId::proportional(look.less(3.0));
    let said = painter.crisp_layout_no_wrap(label.to_owned(), font.clone(), tint);
    let top = crate::theme::crisp::top_centring_capitals(painter, &font, area.center().y);
    painter.crisp_galley(Pos2::new(area.center().x - said.size().x / 2.0, top), said, tint);
    let named = announced.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), named.clone())
    });
    response.clicked()
}

/// The comments and the box that posts one, for the modal.
///
/// `order` is the modal's `Tab` walk, and the comment box adds itself to it.
pub(crate) fn comment_section(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    order: &mut Vec<egui::Id>,
    area: Rect,
    look: &Look<'_>,
) -> Vec<Request> {
    use rux::components::{Button, ButtonSize, TextInput};
    let mut requests = Vec::new();
    // The two buttons are measured first, because whether they fit beside the box decides how tall the box and
    // the buttons are together, and that decides how much of the section the comments get.
    let post =
        Button::new("Post comment").primary().icon(rux::icon::Icon::Chat).size(ButtonSize::Small);
    let send = Button::new("Send to terminal")
        .trailing(rux::icon::Icon::ArrowRight)
        .size(ButtonSize::Small);
    let post_width = post.measure(rux).x;
    let send_width = send.measure(rux).x;
    let buttons = post_width + send_width + 8.0 * 3.0;
    let box_height = composer_height(area.width(), buttons);
    let ui = &mut *rux.ui;
    let comments = board.detail().comments.clone();
    // **No count line.** The heading above this section carries it — `COMMENTS \u{b7} 3` — and a section that
    // said how many comments it held immediately under a heading that said the same thing was one fact drawn
    // twice, in a column where every point of height is being argued over. `task-1771`.
    let pen = area.min.y;
    let room_for_comments = area.max.y - box_height - 6.0;
    let editing = board.detail().editing_comment;
    let mut edited = board.detail().comment_edit.clone();
    // Which comments are being read as their source, and the cache the rendered ones are drawn from. Taken out of
    // `board` before the closure below, because `board.markdown` is borrowed for the whole of it while
    // `board.detail()` would be borrowed again inside — two fields of one struct, so they are named separately.
    let raw_comments = board.detail().comments_raw.clone();
    let now = clock::now();
    // **Newest first, and scrolled**, which is two faults gone. The modal drew them oldest first and simply
    // stopped when it ran out of room, so on a ticket with a real conversation on it the newest comment — the one
    // somebody opened the ticket to read — was the one that could not be seen, and there was no way to reach it.
    // Newest first also makes the modal agree with the pane, which already ordered them this way: switching from
    // one to the other used to reverse the conversation.
    //
    // The scroll is a real `ScrollArea` around the same drawing. Positions inside it are worked out from where
    // the area put its cursor, so the blocks move with the scroll rather than being drawn at fixed places.
    let list =
        Rect::from_min_max(Pos2::new(area.min.x, pen), Pos2::new(area.max.x, room_for_comments));
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(list));
    let output = egui::ScrollArea::vertical()
        .id_salt("agent-tasks-comments")
        .max_height(list.height().max(0.0))
        .show(&mut inside, |ui| {
            let painter = ui.painter().clone();
            let mut pen = ui.cursor().min.y;
            let top = pen;
            let mut resend: Option<String> = None;
            let mut edit: Option<i64> = None;
            let mut save = false;
            let mut cancel = false;
            let mut typed = false;
            let mut copy: Option<String> = None;
            // Which comment's view was changed, acted on once the loop has finished for the reason every other
            // change in it is: `board` is borrowed for the drawing.
            let mut view_change: Option<(i64, bool)> = None;
            let markdown = &mut board.markdown;
            for comment in comments.iter().rev() {
                // **A header row tall enough for its buttons** (`task-2200`). The row used to advance by one line
                // of its small type while the view buttons on it are eighteen points tall, so their bottom edge
                // sat over the first line of the comment.
                text(
                    &painter,
                    Pos2::new(area.min.x, pen + (HEAD - (look.font_size - 2.5)) / 2.0 - 1.0),
                    &format!(
                        "{} · {}",
                        comment.author.name(),
                        clock::relative(&comment.created_at, &now)
                    ),
                    look.font_size - 2.5,
                    look.palette.text_faint,
                );
                // The two view buttons on the comment's own header row, at its right hand end, the same pair the
                // description has. `task-2200` asked for them *"all the way at the right of the comment"*; a person's
                // own comment carries `Edit` and `Send` to their left.
                if let Some(as_markdown) = super::raw_or_rendered(
                    ui,
                    look,
                    Rect::from_min_size(
                        Pos2::new(area.min.x, pen + (HEAD - 18.0) / 2.0),
                        Vec2::new(area.width(), 18.0),
                    ),
                    &format!("comment {}", comment.id),
                    !raw_comments.contains(&comment.id),
                ) {
                    view_change = Some((comment.id, !as_markdown));
                }
                pen += HEAD;
                let mine = comment.author == crate::services::agent_tasks::model::Author::Human;
                let being_edited = editing == Some(comment.id);
                let height = match being_edited {
                    // A field the height of two comment lines, which is what an edit needs and what the section has room
                    // for. Multiline, because a comment is prose and `Enter` in it has to make a line rather than save.
                    true => {
                        let at = Rect::from_min_size(
                            Pos2::new(area.min.x, pen),
                            Vec2::new(area.width(), 48.0),
                        );
                        painter.rect(
                            at,
                            CornerRadius::same(look.corner_radius as u8),
                            look.palette.field,
                            egui::Stroke::new(1.0, look.palette.control_border),
                            egui::StrokeKind::Inside,
                        );
                        let edit_id = ui.id().with("agent-tasks-comment-edit");
                        let response = ui.put(
                            crate::components::controls::field_takes_the_whole_rectangle_at(
                                ui,
                                at,
                                6.0,
                                edit_id,
                                "Comment edit field",
                                &egui::FontId::proportional(look.font_size - 1.0),
                            ),
                            egui::TextEdit::multiline(&mut edited)
                                .id(edit_id)
                                .frame(egui::Frame::NONE)
                                .font(egui::FontId::proportional(look.font_size - 1.0))
                                .text_color(look.palette.text),
                        );
                        if response.changed() {
                            typed = true;
                        }
                        48.0
                    }
                    // **Rendered by default.** `task-28`: a comment is read far more often than it is written, and an
                    // agent's comments are markdown with headings, lists and code in them, so the source is the second
                    // view rather than the first. `raw` below is the source, unchanged from what this always drew.
                    false if !raw_comments.contains(&comment.id) => {
                        let colors = crate::components::markdown_text::Colors {
                            text: look.palette.text_control,
                            strong: look.palette.text_strong,
                            code: look.palette.added,
                            link: look.palette.accent,
                            quiet: look.palette.text_dim,
                            rule: look.palette.divider,
                        };
                        let key = format!("comment-{}", comment.id);
                        let made = markdown.rendered(
                            &key,
                            &comment.body,
                            look.renderer,
                            &look.font_family,
                            look.font_size - 1.0,
                            colors,
                            area.width(),
                            None,
                        );
                        // **The whole comment**, `task-2200`: *"the comments are clipped and I cant scroll"*. Each one
                        // was cut to sixty points, so a long comment lost everything after its third line and the list
                        // around it had nothing more to scroll to. The list scrolls, so a comment can be as tall as it is.
                        let height = made.height();
                        let block = Rect::from_min_size(
                            Pos2::new(area.min.x, pen),
                            Vec2::new(area.width(), height),
                        );
                        crate::components::markdown_text::show(ui, block, made, look.renderer, 0.0);
                        height
                    }
                    false => {
                        let galley = painter.crisp_layout(
                            comment.body.clone(),
                            egui::FontId::proportional(look.font_size - 1.0),
                            look.palette.text_control,
                            area.width(),
                        );
                        // Clipped to what was laid out for, which is now the whole galley. See the rendered arm.
                        let height = galley.size().y;
                        let block = Rect::from_min_size(
                            Pos2::new(area.min.x, pen),
                            Vec2::new(area.width(), height),
                        );
                        painter.with_clip_rect(block).crisp_galley(
                            block.min,
                            galley,
                            look.palette.text_control,
                        );
                        height
                    }
                };
                // A comment that is being read rather than edited has a right click menu that copies it, which
                // is `task-2198`'s *"right click ... text and see a menu with text options, like copy."*
                if !being_edited {
                    let block = Rect::from_min_size(
                        Pos2::new(area.min.x, pen),
                        Vec2::new(area.width(), height),
                    );
                    let id = ui.id().with(("agent-tasks-comment-menu", comment.id));
                    if crate::components::controls::read_only_menu(
                        ui,
                        block,
                        id,
                        &[("Copy Comment", true)],
                    ) == Some(0)
                    {
                        copy = Some(comment.body.clone());
                    }
                }
                // A human's own comment can be sent to the agent on its own, which is the browser's `Send to terminal` on
                // each comment: a comment written before the agent was running still has to be able to reach it. And it
                // can be changed, which is the browser's `Edit`. Neither is drawn on an agent's comment: what an agent
                // said is a record, and the store refuses to change one whatever is pressed.
                if mine {
                    let row = pen - HEAD + (HEAD - 18.0) / 2.0;
                    // Left of the two view buttons, which take the row's right hand forty points. Laid out from the
                    // right, each as wide as its word.
                    let mut right = area.max.x - VIEW_BUTTONS;
                    let mut place = |label: &str| {
                        let width = comment_button_width(ui, look, label);
                        let at = Rect::from_min_size(
                            Pos2::new(right - width, row),
                            Vec2::new(width, 18.0),
                        );
                        right = at.min.x - 4.0;
                        at
                    };
                    let when = clock::relative(&comment.created_at, &now);
                    let author = comment.author.name();
                    match being_edited {
                        // `Save` and `Cancel` in place of the two, because while a comment is being edited those are the
                        // only two things to do with it.
                        true => {
                            let cancel_at = place("Cancel");
                            let save_at = place("Save");
                            if comment_button(ui, look, save_at, "Save", "Save", true, comment.id) {
                                save = true;
                            }
                            if comment_button(
                                ui, look, cancel_at, "Cancel", "Cancel", false, comment.id,
                            ) {
                                cancel = true;
                            }
                        }
                        false => {
                            let send_at = place("Send to terminal");
                            let edit_at = place("Edit");
                            // Named for which comment, because a ticket has several and every one of these said only
                            // `Edit`: a screen reader met four controls with one name between them. The author and
                            // when it was written is what the heading above the comment says and what tells two
                            // comments by the same author apart.
                            if comment_button(
                                ui,
                                look,
                                edit_at,
                                "Edit",
                                &format!("Edit the comment by {author} {when}"),
                                false,
                                comment.id,
                            ) {
                                edit = Some(comment.id);
                            }
                            if comment_button(
                                ui,
                                look,
                                send_at,
                                "Send to terminal",
                                &format!("Send the comment by {author} {when} to the terminal"),
                                false,
                                comment.id,
                            ) {
                                resend = Some(comment.body.clone());
                            }
                        }
                    }
                }
                pen += height + 8.0;
            }
            // The room the comments really took, so the scroll area knows how far there is to scroll. Without it
            // an area whose content is painted rather than laid out believes it has nothing in it and never
            // scrolls.
            ui.allocate_space(Vec2::new(area.width(), (pen - top).max(0.0)));
            (resend, edit, save, cancel, typed, view_change, copy)
        });
    // **The box goes directly under the comments**, `task-2200`: *"too large of a margin between the bottom of
    // comments and the add comment input"*. The section is given the same height whatever is in it, so a ticket
    // with one short comment had the rest of that height empty between it and the box. With more comments than
    // fit, the list fills the section and the box sits where it always did.
    let used = output.content_size.y.min(list.height()).max(0.0);
    let box_top = (list.min.y + used + 6.0).min(area.max.y - box_height);
    let (resend, edit, save, cancel, typed, view_change, copy) = output.inner;
    if let Some(text) = copy {
        requests.push(Request::Copy(text));
    }
    if let Some((id, raw)) = view_change {
        board.show_the_comment_raw(id, raw);
    }
    // Acted on after the drawing, because the comments are being read while they are drawn.
    if typed {
        board.detail_mut().comment_edit = edited;
    }
    if let Some(id) = edit {
        board.edit_the_comment(id);
    }
    if cancel {
        board.stop_editing_the_comment();
    }
    if save {
        match board.save_the_comment() {
            Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
            Ok(_) => {}
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    if let Some(body) = resend {
        match board.send_a_comment(&body) {
            Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
            Ok(_) => {}
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    // **The box and its two buttons are `rux` controls**, the same field the JIRA key is typed into and the
    // same buttons as the rest of the dialog. `task-2214`: *"Post comment & send to terminal buttons need better
    // styling."* They were two flat egui buttons twenty two points tall in a dialog whose every other control is
    // `rux`, so they read as something left over from an older dialog. `Post comment` is the primary action of the
    // section and is drawn as one; `Send to terminal` is the second choice and is drawn as an ordinary button.
    // Both are dimmed while the box is empty, because neither can do anything with no words.
    let one_row = box_height <= COMPOSER;
    let field_at = Rect::from_min_size(
        Pos2::new(area.min.x, box_top),
        Vec2::new(
            match one_row {
                true => area.width() - buttons + 8.0,
                false => area.width(),
            },
            COMPOSER,
        ),
    );
    let buttons_top = match one_row {
        true => box_top,
        false => box_top + COMPOSER + 8.0,
    };
    let send_at = Rect::from_min_size(
        Pos2::new(area.max.x - send_width, buttons_top),
        Vec2::new(send_width, COMPOSER),
    );
    let post_at = Rect::from_min_size(
        Pos2::new(send_at.min.x - 8.0 - post_width, buttons_top),
        Vec2::new(post_width, COMPOSER),
    );
    let mut draft = board.detail().draft.clone();
    let typed = TextInput::new(&mut draft)
        .hint("Add a comment")
        .style(rux::text::Style::CONTROL)
        .pad(rux::layout::Pad::axes(8.0, 12.0))
        .label("Comment")
        .id_salt("agent-tasks-comment-draft")
        .show(rux, field_at);
    let draft_id = rux.ui.id().with(("rux-text-input", egui::Id::new("agent-tasks-comment-draft")));
    order.push(draft_id);
    if typed.changed {
        board.detail_mut().draft = draft.clone();
    }
    // Enter posts, and is taken out of the frame so nothing drawn after the box takes it as its own.
    let entered = typed.submitted;
    if entered {
        rux.ui.ctx().input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
    }
    let written = !draft.trim().is_empty();
    let posting = post.enabled(written).show(rux, post_at).clicked() || (entered && written);
    let sending = send.enabled(written).show(rux, send_at).clicked();
    if posting || sending {
        match board.post_the_comment(sending) {
            Ok(said) if !said.is_empty() => requests.push(Request::Message(said)),
            Ok(_) => {}
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    requests
}
