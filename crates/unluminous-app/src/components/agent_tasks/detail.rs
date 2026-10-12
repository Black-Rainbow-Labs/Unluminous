//! One ticket: its title, its description, its todos, its comments and its terminal.
//!
//! The description is markdown and Unluminous already reads markdown, so it is drawn by the same reader the
//! Markdown preview uses rather than by a second one written here. That is the largest single saving in
//! the plugin and it falls out of the plugin being inside a text editor.

use egui::{CornerRadius, Pos2, Rect, Sense, Vec2};

use crate::services::agent_tasks::{clock, AgentTasks};
use crate::services::plugin_ui::{Look, Request};
use crate::theme::crisp::CrispPainter;

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

/// The todo rows and the row that adds one, for the modal, which lays its sections out itself.
///
/// `task-2241`, the Agent-Tasks design: inside the carved well the modal has drawn, one row 36 points tall a
/// todo with a round tick box, raised while it is open and carved in with a tick once it is done, and the words
/// beside it, dimmed once it is done. The whole row ticks it. Under them a raised pill on the pane's own ground
/// holds the box a todo is typed into and a round `+` that adds it.
pub(crate) fn todo_rows(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    area: Rect,
    _look: &Look<'_>,
) -> Vec<Request> {
    use rux::components::ChipButton;
    use rux::icon::{Icon, Mark};
    use rux::text::Style;
    let mut requests = Vec::new();
    let theme = rux.theme();
    let chat = theme.chat;
    let todos = board.detail().todos.clone();
    const ROW: f32 = 36.0;
    const GAP: f32 = 4.0;
    const ADD: f32 = 38.0;
    let rows = Rect::from_min_max(area.min, Pos2::new(area.max.x, area.max.y - ADD - 2.0 - GAP));
    // How far the list is scrolled, before it is drawn: the wheel over this section moves the todos rather than
    // the lane behind them.
    let content = todos.len() as f32 * (ROW + GAP);
    board.scroll_the_todos(rux.ui, rows, (content - rows.height()).max(0.0));
    let mut pen = rows.min.y - board.todo_scroll;
    let mut toggled = None;
    let mut removed = None;
    let cut = content > rows.height() + 0.5;
    if cut {
        rux.chrome.clip(rows, 14.0);
    }
    let mut list_ui = rux.ui.new_child(egui::UiBuilder::new().max_rect(rows));
    list_ui.set_clip_rect(rows.intersect(rux.ui.clip_rect()));
    let mut list = rux::Rux { ui: &mut list_ui, state: rux.state, chrome: rux.chrome };
    for todo in &todos {
        // Scrolled rather than stopped: a todo past the fold can still be ticked or removed.
        let row = Rect::from_min_size(Pos2::new(rows.min.x, pen), Vec2::new(rows.width(), ROW));
        pen += ROW + GAP;
        if row.max.y < rows.min.y || row.min.y > rows.max.y {
            continue;
        }
        let whole = list.ui.interact(
            row,
            list.ui.id().with(("agent-tasks-modal-todo-row", todo.id)),
            Sense::click(),
        );
        if whole.hovered() {
            list.wash(row, 14.0, theme.interaction.hover);
        }
        let check = Rect::from_center_size(
            Pos2::new(row.min.x + 10.0 + 11.0, row.center().y),
            Vec2::splat(22.0),
        );
        // Named as a tick box with the todo's words, which is what a test and a screen reader find it by.
        let ticked = rux::components::round_check(&mut list, check, todo.done, &todo.text);
        if ticked.clicked() || whole.clicked() {
            toggled = Some((todo.id, !todo.done));
        }
        let ink = match todo.done {
            true => theme.ink.i400,
            false => theme.ink.i900,
        };
        let style = Style::sans(13.0);
        let words =
            list.elided(style, &todo.text, ink, (row.max.x - 34.0 - (check.max.x + 12.0)).max(0.0));
        rux::text::draw_left_capitals(
            list.painter(),
            Pos2::new(check.max.x + 12.0, row.center().y),
            words,
            style,
            ink,
        );
        // A cross that removes it, quiet until the pointer is on it. The browser's todos are read only because
        // the agent writes its own plan; here they can be written, and a list that can be added to and not
        // removed from is half a list.
        let cross =
            Rect::from_center_size(Pos2::new(row.max.x - 16.0, row.center().y), Vec2::splat(22.0));
        let response = list.ui.interact(
            cross,
            list.ui.id().with(("agent-tasks-remove-todo", todo.id)),
            Sense::click(),
        );
        let named = format!("Remove {}", todo.text);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, named.clone())
        });
        let tint = match (response.hovered(), whole.hovered()) {
            (true, _) => chat.slower,
            (false, true) => theme.ink.i500,
            (false, false) => theme.ink.i400.gamma_multiply(0.5),
        };
        list.mark(Mark::new(Icon::X, 10.0).stroke(1.6), cross.center(), tint);
        if response.clicked() {
            removed = Some(todo.id);
        }
    }
    if cut {
        rux.chrome.unclip();
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
    // The pill that adds one, at the foot of the well: `padding: 4px 4px 4px 14px` on the pane's own ground.
    let pill =
        Rect::from_min_size(Pos2::new(area.min.x, area.max.y - ADD), Vec2::new(area.width(), ADD));
    rux.chrome.surface(pill, ADD / 2.0, chat.ground, chat.raised_chip());
    let add = Rect::from_center_size(
        Pos2::new(pill.max.x - 4.0 - 15.0, pill.center().y),
        Vec2::splat(30.0),
    );
    let font = egui::FontId::new(12.5, egui::FontFamily::Proportional);
    let todo_id = rux.ui.id().with("agent-tasks-todo-draft");
    let field = Rect::from_min_max(
        Pos2::new(pill.min.x + 14.0, pill.min.y),
        Pos2::new(add.min.x - 10.0, pill.max.y),
    );
    let text = crate::components::controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        field,
        0.0,
        todo_id,
        "Todo field",
        &font,
    );
    let mut draft = board.detail().todo_draft.clone();
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = child.add(
        egui::TextEdit::singleline(&mut draft)
            .id(todo_id)
            .frame(egui::Frame::NONE)
            .hint_text(crate::components::controls::placeholder(
                "Add a todo",
                &font,
                theme.ink.i400,
            ))
            .font(font.clone())
            .desired_width(text.width())
            .text_color(theme.ink.i900),
    );
    if response.changed() {
        board.detail_mut().todo_draft = draft;
    }
    let pressed = ChipButton::new(Icon::Plus, 30.0, "Add todo")
        .mark(12.0, Some(1.8))
        .show(rux, add)
        .clicked();
    if super::enter_was_used_and_pressed(rux.ui, &response) || pressed {
        if let Err(problem) = board.post_the_todo() {
            requests.push(Request::Message(problem));
        }
        response.request_focus();
    }
    requests
}

/// How tall a comment's header row is: its author and time, and the buttons beside them.
const HEAD: f32 = 22.0;
/// How much of that row's right hand end the two view buttons take, with the gap before what is left of them.
const VIEW_BUTTONS: f32 = 18.0 * 2.0 + 4.0 + 6.0;
/// How far a comment's words are from the left: the 30 point disc and the 12 point gap after it.
const BESIDE_THE_DISC: f32 = 42.0;

/// How tall the pill a comment is written in is: `padding: 5px` round a 34 point button.
const COMPOSER: f32 = 44.0;

/// How wide a comment's small button is for `label`: its word and a little room either side.
fn comment_button_width(ui: &egui::Ui, look: &Look<'_>, label: &str) -> f32 {
    let font = egui::FontId::proportional(look.less(2.0));
    ui.painter().crisp_layout_no_wrap(label.to_owned(), font, look.palette.accent).size().x + 16.0
}

/// One of the small buttons on a comment's own header row: `Edit`, `Send to terminal`, `Save` and `Cancel`.
///
/// `task-2214`. Words in the sky colour with no frame until the pointer is on one, when a pill comes up behind
/// them, which is how a quiet action beside text reads everywhere else. `strong` is the one that commits, `Save`.
#[allow(clippy::too_many_arguments)]
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
    let chat = crate::theme::rux_theme().chat;
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
        _ => chat.sky,
    };
    let font = egui::FontId::proportional(look.less(2.0));
    let said = painter.crisp_layout_no_wrap(label.to_owned(), font.clone(), tint);
    let top = crate::theme::crisp::top_centring_capitals(painter, &font, area.center().y);
    painter.crisp_galley(Pos2::new(area.center().x - said.size().x / 2.0, top), said, tint);
    let named = announced.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), named.clone())
    });
    response.clicked()
}

/// The two buttons in the comment pill: `Comment & Send to Agent` with no surface, and `Comment` on a chip.
///
/// `task-2253`: *"for comment, the buttons should be "Comment & Send to Agent" and "Comment". Comment and send
/// should both add the comment, as well as send it to the terminal. Comment should just add the comment."*
/// They are the board's `comment-send` and `comment` commands, which already did exactly that.
fn composer_buttons() -> (rux::components::PillButton<'static>, rux::components::PillButton<'static>)
{
    use rux::components::PillButton;
    (PillButton::new("Comment"), PillButton::new("Comment & Send to Agent").ghost())
}

/// How much of a comment section the pill that writes a comment takes, with the gap above it. The modal's
/// budget reads this.
pub(crate) fn composer_room(_rux: &rux::Rux<'_>, _width: f32) -> f32 {
    COMPOSER + 12.0
}

/// The comments and the pill that posts one, for the modal.
///
/// `task-2241`, the Agent-Tasks design: each comment is a round chip with who wrote it (the spark in the agent's
/// colour, or a figure for a person), the author and how long ago beside it, and the words under that. `order` is
/// the modal's `Tab` walk, and the comment box adds itself to it.
pub(crate) fn comment_section(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    order: &mut Vec<egui::Id>,
    area: Rect,
    look: &Look<'_>,
) -> Vec<Request> {
    use rux::icon::{Icon, Mark};
    // The discs are drawn through a `Rux` made inside the scrolling list below, on this same state.
    crate::theme::in_step(rux.state);
    let mut requests = Vec::new();
    let theme = rux.theme();
    let chat = theme.chat;
    let (post, send) = composer_buttons();
    let post_width = post.measure(rux).x;
    let send_width = send.measure(rux).x;
    let comments = board.detail().comments.clone();
    let room_for_comments = area.max.y - COMPOSER - 12.0;
    let editing = board.detail().editing_comment;
    let mut edited = board.detail().comment_edit.clone();
    // Which comments are being read as their source, and the cache the rendered ones are drawn from. Taken out
    // of `board` before the closure below, because `board.markdown` is borrowed for the whole of it.
    let raw_comments = board.detail().comments_raw.clone();
    let now = clock::now();
    let agent = look.palette.agent;
    // **Newest first, and scrolled**: on a ticket with a real conversation on it the newest comment is the one
    // somebody opened the ticket to read.
    let list = Rect::from_min_max(area.min, Pos2::new(area.max.x, room_for_comments));
    let words_left = area.min.x + BESIDE_THE_DISC;
    let words_width = (area.width() - BESIDE_THE_DISC).max(40.0);
    // The discs are decoration, so the decoration is cut to the list while it scrolls.
    rux.chrome.clip(list, 0.0);
    let chrome = rux.chrome;
    let state = rux.state;
    let ui = &mut *rux.ui;
    let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(list));
    // Cut to the list's own rectangle, so a section squeezed below the height of one comment never draws that
    // comment over the box under it.
    inside.set_clip_rect(list.intersect(ui.clip_rect()));
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
            let mut view_change: Option<(i64, bool)> = None;
            let markdown = &mut board.markdown;
            for comment in comments.iter().rev() {
                let mine = comment.author == crate::services::agent_tasks::model::Author::Human;
                // The round chip saying who wrote it.
                let disc = Rect::from_min_size(Pos2::new(area.min.x, pen), Vec2::splat(30.0));
                {
                    let mut drawn = rux::Rux { ui: &mut *ui, state, chrome };
                    rux::components::chip_disc(&mut drawn, disc);
                    let (icon, ink) = match mine {
                        true => (Icon::User, theme.ink.i400),
                        false => (Icon::Spark, agent),
                    };
                    drawn.mark(Mark::new(icon, 13.0).stroke(2.0), disc.center(), ink);
                }
                // `claude · just now`: the author in the stronger ink, the time in the quieter one.
                let when = clock::relative(&comment.created_at, &now);
                let author = comment.author.name();
                let mut job = egui::text::LayoutJob::default();
                let font = egui::FontId::proportional(11.5);
                job.append(
                    author,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::new(
                            11.5,
                            egui::FontFamily::Name(crate::theme::BOLD_FAMILY.into()),
                        ),
                        color: theme.ink.i700,
                        ..Default::default()
                    },
                );
                job.append(
                    &format!(" \u{b7} {when}"),
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: theme.ink.i400,
                        ..Default::default()
                    },
                );
                let head = painter.crisp_layout_job(job);
                let head_top =
                    crate::theme::crisp::top_centring_capitals(&painter, &font, pen + HEAD / 2.0);
                painter.crisp_galley(Pos2::new(words_left, head_top), head, theme.ink.i700);
                // The two view buttons on the comment's own header row, at its right hand end, the same pair the
                // description has. `task-2200`.
                if let Some(as_markdown) = super::raw_or_rendered(
                    ui,
                    look,
                    Rect::from_min_size(
                        Pos2::new(words_left, pen + (HEAD - 18.0) / 2.0),
                        Vec2::new(words_width, 18.0),
                    ),
                    &format!("comment {}", comment.id),
                    !raw_comments.contains(&comment.id),
                ) {
                    view_change = Some((comment.id, !as_markdown));
                }
                pen += HEAD;
                let being_edited = editing == Some(comment.id);
                let height = match being_edited {
                    // A field the height of two comment lines. Multiline, because a comment is prose and
                    // `Enter` in it has to make a line rather than save.
                    true => {
                        let at = Rect::from_min_size(
                            Pos2::new(words_left, pen),
                            Vec2::new(words_width, 48.0),
                        );
                        painter.rect_filled(at, CornerRadius::same(12), chat.well);
                        let edit_id = ui.id().with("agent-tasks-comment-edit");
                        let response = ui.put(
                            crate::components::controls::field_takes_the_whole_rectangle_at(
                                ui,
                                at,
                                8.0,
                                edit_id,
                                "Comment edit field",
                                &egui::FontId::proportional(look.font_size),
                            ),
                            egui::TextEdit::multiline(&mut edited)
                                .id(edit_id)
                                .frame(egui::Frame::NONE)
                                .font(egui::FontId::proportional(look.font_size))
                                .text_color(theme.ink.i900),
                        );
                        if response.changed() {
                            typed = true;
                        }
                        48.0
                    }
                    // **Rendered by default.** `task-28`: a comment is read far more often than it is written.
                    false if !raw_comments.contains(&comment.id) => {
                        let colors = crate::components::markdown_text::Colors {
                            text: theme.ink.i900,
                            strong: look.palette.text_strong,
                            code: look.palette.added,
                            link: chat.sky,
                            quiet: look.palette.text_dim,
                            rule: look.palette.divider,
                        };
                        let key = format!("comment-{}", comment.id);
                        let made = markdown.rendered(
                            &key,
                            &comment.body,
                            look.renderer,
                            &look.font_family,
                            look.font_size,
                            colors,
                            words_width,
                            None,
                        );
                        // **The whole comment**, `task-2200`: the list scrolls, so a comment can be as tall as it is.
                        let height = made.height();
                        let block = Rect::from_min_size(
                            Pos2::new(words_left, pen),
                            Vec2::new(words_width, height),
                        );
                        crate::components::markdown_text::show(ui, block, made, look.renderer, 0.0);
                        height
                    }
                    false => {
                        let galley = painter.crisp_layout(
                            comment.body.clone(),
                            egui::FontId::proportional(look.font_size),
                            theme.ink.i900,
                            words_width,
                        );
                        let height = galley.size().y;
                        let block = Rect::from_min_size(
                            Pos2::new(words_left, pen),
                            Vec2::new(words_width, height),
                        );
                        painter.with_clip_rect(block).crisp_galley(
                            block.min,
                            galley,
                            theme.ink.i900,
                        );
                        height
                    }
                };
                // A comment that is being read rather than edited has a right click menu that copies it.
                // `task-2198`.
                if !being_edited {
                    let block = Rect::from_min_size(
                        Pos2::new(words_left, pen),
                        Vec2::new(words_width, height),
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
                // A person's own comment can be sent to the agent on its own, and changed. Neither is offered on
                // an agent's comment: what an agent said is a record.
                if mine {
                    let row = pen - HEAD + (HEAD - 18.0) / 2.0;
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
                    match being_edited {
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
                pen += height.max(30.0 - HEAD) + 14.0;
            }
            // The room the comments really took, so the scroll area knows how far there is to scroll.
            ui.allocate_space(Vec2::new(area.width(), (pen - top).max(0.0)));
            (resend, edit, save, cancel, typed, view_change, copy)
        });
    rux.chrome.unclip();
    // **The pill goes directly under the comments**, `task-2200`, and with more comments than fit it sits where
    // the list ends.
    let used = output.content_size.y.min(list.height()).max(0.0);
    let gap = match used > 0.0 {
        true => 12.0,
        false => 0.0,
    };
    let box_top = (list.min.y + used + gap).min(area.max.y - COMPOSER).max(area.min.y);
    let (resend, edit, save, cancel, typed, view_change, copy) = output.inner;
    if let Some(text) = copy {
        requests.push(Request::Copy(text));
    }
    if let Some((id, raw)) = view_change {
        board.show_the_comment_raw(id, raw);
    }
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
    // The pill: carved in, `padding: 5px 5px 5px 18px`, the box, then `Comment & Send to Agent` with no surface
    // and `Comment` on a chip. Both are dimmed while the box is empty, because neither can do anything with no
    // words.
    let pill =
        Rect::from_min_size(Pos2::new(area.min.x, box_top), Vec2::new(area.width(), COMPOSER));
    rux.chrome.surface(pill, COMPOSER / 2.0, chat.well, chat.carved_sm());
    let post_at = Rect::from_min_size(
        Pos2::new(pill.max.x - 5.0 - post_width, pill.min.y + 5.0),
        Vec2::new(post_width, COMPOSER - 10.0),
    );
    let send_at = Rect::from_min_size(
        Pos2::new(post_at.min.x - 8.0 - send_width, pill.min.y + 5.0),
        Vec2::new(send_width, COMPOSER - 10.0),
    );
    let font = egui::FontId::proportional(12.5);
    let draft_id = rux.ui.id().with(("rux-text-input", egui::Id::new("agent-tasks-comment-draft")));
    let field = Rect::from_min_max(
        Pos2::new(pill.min.x + 18.0, pill.min.y),
        Pos2::new(send_at.min.x - 8.0, pill.max.y),
    );
    let text = crate::components::controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        field,
        0.0,
        draft_id,
        "Comment field",
        &font,
    );
    let mut draft = board.detail().draft.clone();
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = child.add(
        egui::TextEdit::singleline(&mut draft)
            .id(draft_id)
            .frame(egui::Frame::NONE)
            .hint_text(crate::components::controls::placeholder(
                "Add a comment",
                &font,
                theme.ink.i400,
            ))
            .font(font.clone())
            .desired_width(text.width())
            .text_color(theme.ink.i900),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Comment"));
    order.push(draft_id);
    if response.changed() {
        board.detail_mut().draft = draft.clone();
    }
    // Enter posts, and is taken out of the frame so nothing drawn after the box takes it as its own.
    let entered = super::enter_was_used_and_pressed(rux.ui, &response);
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
