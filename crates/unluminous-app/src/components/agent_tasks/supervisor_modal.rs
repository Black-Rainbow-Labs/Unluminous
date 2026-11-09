//! The supervisor's modal: its terminal, its skill file, and what it is started on.
//!
//! `task-2253` made it: *"a modal should open that has the terminal window, along with a message input and send
//! button."* `task-2556` gave it two more views and redrew it in the Agent-Tasks design the ticket modal is drawn
//! in, with `rux` on `modal::show_unframed`, so the two dialogs the board opens look like one family:
//!
//! - **Terminal**, the supervisor's terminal, a message box and `Send`, and `Start supervisor` / `Stop supervisor`.
//! - **Skill**, *"an editable supervisor skill file so I can modify just like in file preview pane, and saving it
//!   immediately gets applied as the skill the supervisor uses."* The file's Markdown source beside its rendered
//!   preview, laid out `Edit`, `Split` or `Preview`, with `Save & apply`, `Revert`, `Reset to default` and `Open in
//!   a tab`. `services::agent_tasks::supervisor` is what saving does.
//! - **Settings**, the gear in the header: the endpoint, model and effort the supervisor is started with.
//!
//! Every control here has a word of `plugins run agent-tasks supervisor` beside it that reaches the same method.

use egui::{Pos2, Rect, Vec2};
use rux::components::{ChipButton, PillButton, PillSwitch, RoundButton, RoundSize, SelectLead};
use rux::icon::Icon;
use rux::text::Style;

use crate::components::controls::TextBoxes;
use crate::components::modal;
use crate::services::agent_tasks::{
    AgentTasks, SkillLayout, SupervisorView, TicketKit, EFFORTS, SUPERVISOR,
};
use crate::services::plugin_ui::{EmbeddedFile, EmbeddedView, Look, Request};

/// The modal's id, which is also what its place and its zoom are kept against.
pub const MODAL_ID: &str = "agent-tasks-supervisor";
/// The smallest it asks for, whatever the window is.
const SMALLEST_WIDTH: f32 = 720.0;
const SMALLEST_HEIGHT: f32 = 520.0;
/// The ticket modal's padding, header and roundness, so the two dialogs are the same shape.
const PAD: f32 = 24.0;
const HEADER: f32 = 36.0;
const HEADER_GAP: f32 = 20.0;
const RADIUS: f32 = 30.0;
/// The row under a view: the message box and `Send`, or the skill's buttons.
const ROW: f32 = 38.0;
const GAP: f32 = 14.0;
/// The tab switch in the header: each tab 22 points tall in a track 30 tall.
const TAB_HEIGHT: f32 = 22.0;
const TAB_PAD: f32 = 14.0;
/// How wide the Settings view's column of fields is.
const FIELDS: f32 = 360.0;

/// What the modal reported: the requests for the window, and whether it was closed.
#[derive(Debug, Default)]
pub struct Outcome {
    pub requests: Vec<Request>,
    pub closed: bool,
}

/// How big the modal asks to be: the size every working dialog opens at, as the ticket modal does.
fn size(ctx: &egui::Context) -> Vec2 {
    let window = ctx.content_rect().size();
    modal::starting_size(window, MODAL_ID, Vec2::new(SMALLEST_WIDTH, SMALLEST_HEIGHT))
}

/// Draw the supervisor's modal.
///
/// @param board - the board, which holds the supervisor's terminal, its skill and its settings
/// @param ctx - the window's context
/// @param look - how the board is drawn
pub fn show(board: &mut AgentTasks, ctx: &egui::Context, look: &Look<'_>) -> Outcome {
    let size = size(ctx);
    let look = look.clone().at_a_fixed_size(super::ticket_modal::BODY).flat();
    let mut kit = board.supervisor_kit.take().unwrap_or_default();
    let (inner, should_close) = modal::show_unframed(ctx, MODAL_ID, size.x, size.y, |ui, area| {
        // The terminal is drawn by Unluminous's own glyph engine, which is told the transform the modal is drawn
        // through, as the ticket modal's terminal is.
        let transform = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
        let was = look.renderer.composite_through(transform, true);
        let drawn = contents(board, &mut kit, ui, area, &look);
        look.renderer.restore_compositing(was);
        drawn
    });
    kit.rux.end_frame();
    board.supervisor_kit = Some(kit);
    Outcome { requests: inner.requests, closed: inner.closed || should_close }
}

/// The dialog's surface, its header row, and the view under it.
fn contents(
    board: &mut AgentTasks,
    kit: &mut TicketKit,
    ui: &mut egui::Ui,
    area: Rect,
    look: &Look<'_>,
) -> Outcome {
    let state = &kit.rux;
    crate::theme::in_step(state);
    let selects = &mut kit.selects;
    let id = ui.id().with("agent-tasks-supervisor-rux");
    rux::layer(ui, state, id, area.expand(modal::SHADOW_REACH), |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(area, RADIUS, chat.ground, modal::shadow());
        let inner = area.shrink(PAD);
        let header = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER));
        let mut outcome = Outcome { closed: header_row(board, rux, header), ..Outcome::default() };
        let body = Rect::from_min_max(Pos2::new(inner.min.x, header.max.y + HEADER_GAP), inner.max);
        outcome.requests = match board.supervisor_view {
            SupervisorView::Terminal => terminal_view(board, rux, body, look),
            SupervisorView::Skill => skill_view(board, rux, body),
            SupervisorView::Settings => settings_view(board, rux, selects, body),
        };
        outcome
    })
}

/// The header: a chip that says whether the supervisor runs, the title, then the tabs, the gear and the close
/// button at the right hand end. Answers whether the modal was closed.
fn header_row(board: &mut AgentTasks, rux: &mut rux::Rux<'_>, area: Rect) -> bool {
    let chip_end = status_chip(rux, area, board.supervisor_is_running());
    let close =
        Rect::from_center_size(Pos2::new(area.max.x - 18.0, area.center().y), Vec2::splat(36.0));
    let closed =
        ChipButton::new(Icon::X, 36.0, "Close").mark(12.0, Some(1.8)).show(rux, close).clicked();
    // The gear, pressed in while the Settings view shows, as the Chat pane's gear is while its settings are open.
    let gear_side = RoundSize::Small.diameter();
    let gear = Rect::from_center_size(
        Pos2::new(close.min.x - 10.0 - gear_side / 2.0, area.center().y),
        Vec2::splat(gear_side),
    );
    let settings = board.supervisor_view == SupervisorView::Settings;
    if RoundButton::mark(Icon::Settings, 15.0, "Supervisor settings")
        .size(RoundSize::Small)
        .on(settings)
        .show(rux, gear)
        .clicked()
    {
        board.supervisor_view = match settings {
            true => SupervisorView::Terminal,
            false => SupervisorView::Settings,
        };
    }
    let (picked, tabs_left) =
        view_tabs(rux, Pos2::new(gear.min.x - 14.0, area.center().y), board.supervisor_view);
    if let Some(view) = picked {
        board.supervisor_view = view;
    }
    let style = Style::sans(20.0).semibold().tracking(-0.015);
    let ink = rux.theme().ink.i900;
    let room = (tabs_left - 16.0 - (chip_end + 16.0)).max(0.0);
    let title = fit(rux, style, "Chat with supervisor", room);
    let galley = rux.text(style, &title, ink);
    let at = Pos2::new(chip_end + 16.0, area.center().y - galley.size().y / 2.0);
    rux.painter().galley(at, galley, ink);
    closed
}

/// The chip at the start of the header: `Supervisor` with a dot, the faster colour while it runs. Answers where
/// it ends.
fn status_chip(rux: &mut rux::Rux<'_>, area: Rect, running: bool) -> f32 {
    let chat = rux.theme().chat;
    let theme = rux.theme();
    let said = match running {
        true => "Supervisor \u{b7} running",
        false => "Supervisor \u{b7} stopped",
    };
    let style = Style::sans(12.0);
    let galley = rux.text(style, said, theme.ink.i500);
    let chip = Rect::from_min_size(
        Pos2::new(area.min.x, area.center().y - (galley.size().y + 14.0) / 2.0),
        Vec2::new(galley.size().x + 24.0 + 14.0, galley.size().y + 14.0),
    );
    rux.chrome.surface(chip, chip.height() / 2.0, chat.well, chat.carved_sm());
    let dot = Pos2::new(chip.min.x + 16.0, chip.center().y);
    let colour = match running {
        true => chat.faster,
        false => theme.ink.i400,
    };
    rux.painter().circle_filled(dot, 4.0, colour);
    let text = Rect::from_min_max(Pos2::new(dot.x + 8.0, chip.min.y), chip.max);
    rux::text::draw_centred(rux.painter(), text, galley, theme.ink.i500);
    let response = rux.ui.interact(
        chip,
        rux.ui.id().with("agent-tasks-supervisor-state"),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, said));
    chip.max.x
}

/// `words`, cut with an ellipsis to fit `room`.
fn fit(rux: &rux::Rux<'_>, style: Style, words: &str, room: f32) -> String {
    if rux.measure(style, words).x <= room {
        return words.to_owned();
    }
    let mut cut: String = words.to_owned();
    while !cut.is_empty() && rux.measure(style, &format!("{cut}\u{2026}")).x > room {
        cut.pop();
    }
    format!("{cut}\u{2026}")
}

/// The two tabs, `Terminal` and `Skill`, as the ticket modal's carved track with the chosen tab raised out of it,
/// ending at `right`. While the Settings view shows, neither is raised. Answers a new choice and where the track
/// starts.
fn view_tabs(
    rux: &mut rux::Rux<'_>,
    right: Pos2,
    chosen: SupervisorView,
) -> (Option<SupervisorView>, f32) {
    let theme = rux.theme();
    let chat = theme.chat;
    let style = Style::sans(12.0);
    let tabs =
        [(SupervisorView::Terminal, "Terminal"), (SupervisorView::Skill, "Supervisor Skill")];
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
    for ((view, word), width) in tabs.into_iter().zip(widths) {
        let pill = Rect::from_min_size(
            Pos2::new(x, track.center().y - TAB_HEIGHT / 2.0),
            Vec2::new(width, TAB_HEIGHT),
        );
        if tab(rux, pill, word, view == chosen) && view != chosen {
            picked = Some(view);
        }
        x = pill.right() + 3.0;
    }
    (picked, track.left())
}

/// One tab of [`view_tabs`]. Answers whether it was pressed.
fn tab(rux: &mut rux::Rux<'_>, pill: Rect, word: &str, on: bool) -> bool {
    let theme = rux.theme();
    let chat = theme.chat;
    let response = rux.ui.interact(
        pill,
        rux.ui.id().with(("agent-tasks-supervisor-tab", word)),
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
    let galley = rux.text(Style::sans(12.0), word, ink);
    rux::text::draw_centred(rux.painter(), pill, galley, ink);
    let named = format!("{word} tab");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, named.clone())
    });
    response.clicked()
}

// ---------------------------------------------------------------------------------------------- the terminal

/// The Terminal view: the supervisor's terminal in a carved well, the message row, and the footer.
fn terminal_view(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    body: Rect,
    look: &Look<'_>,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let chat = rux.theme().chat;
    let running = board.supervisor_is_running();
    let well = Rect::from_min_max(body.min, Pos2::new(body.max.x, body.max.y - 2.0 * (ROW + GAP)));
    rux.chrome.surface(well, 20.0, chat.well, chat.carved_sm());
    match running {
        true => {
            let grid = well.shrink(12.0);
            let focused = board.terminal_focused;
            requests.extend(super::detail::terminal(board, rux.ui, look, grid, SUPERVISOR, focused));
        }
        false => note(
            rux,
            well,
            "The supervisor is not running. Start supervisor launches it in the board's project folder on the \
             endpoint and model in the gear's settings, and tells it to follow the skill in the Skill tab. \
             Anything typed below is sent to it.",
        ),
    }
    let row =
        Rect::from_min_size(Pos2::new(body.min.x, well.max.y + GAP), Vec2::new(body.width(), ROW));
    requests.extend(message_row(board, rux, row, running));
    let foot =
        Rect::from_min_size(Pos2::new(body.min.x, row.max.y + GAP), Vec2::new(body.width(), ROW));
    requests.extend(terminal_footer(board, rux, foot, running));
    requests
}

/// Words in a carved well, beside the terminal's disc, for a view with nothing else to show.
fn note(rux: &mut rux::Rux<'_>, at: Rect, said: &str) {
    let theme = rux.theme();
    let disc =
        Rect::from_center_size(Pos2::new(at.min.x + 18.0 + 17.0, at.center().y), Vec2::splat(34.0));
    rux::components::chip_disc(rux, disc);
    rux.mark(rux::icon::Mark::new(Icon::Terminal, 15.0), disc.center(), theme.ink.i400);
    let left = disc.max.x + 16.0;
    let galley = rux::text::wrapped(
        rux.painter(),
        Style::sans(12.5).leading(1.5),
        said,
        theme.ink.i400,
        (at.max.x - 24.0 - left).clamp(40.0, 640.0),
    );
    let top = at.center().y - galley.size().y / 2.0;
    rux.painter().with_clip_rect(at).galley(Pos2::new(left, top), galley, theme.ink.i400);
}

/// The message box and `Send`. Enter sends too, and the box keeps the keyboard for the next message.
fn message_row(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    row: Rect,
    running: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let send = PillButton::new("Send").mark(Icon::Send).primary();
    let send_width = send.measure(rux).x.max(96.0);
    let field = Rect::from_min_max(row.min, Pos2::new(row.max.x - send_width - 12.0, row.max.y));
    let id = rux.ui.id().with("agent-tasks-supervisor-message");
    let response = pill_field(
        rux,
        field,
        id,
        "Message to the supervisor",
        "Message to the supervisor",
        &mut board.supervisor_draft,
    );
    let entered =
        response.lost_focus() && rux.ui.input(|input| input.key_pressed(egui::Key::Enter));
    let written = !board.supervisor_draft.trim().is_empty();
    let at = Rect::from_min_size(
        Pos2::new(row.max.x - send_width, row.min.y),
        Vec2::new(send_width, row.height()),
    );
    let pressed = send.enabled(running && written).show(rux, at).clicked();
    if (pressed || (entered && written)) && running {
        let said = board.send_the_supervisor_draft();
        requests.push(Request::Message(said.unwrap_or_else(|problem| problem)));
        response.request_focus();
    }
    requests
}

/// A one line box in a carved pill, as the board's search is. Answers the box's response.
fn pill_field(
    rux: &mut rux::Rux<'_>,
    field: Rect,
    id: egui::Id,
    name: &str,
    hint: &str,
    text: &mut String,
) -> egui::Response {
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    rux.chrome.surface(field, field.height() / 2.0, chat.well, chat.carved_sm());
    let font = Style::sans(13.0).font();
    let inside = Rect::from_min_max(
        Pos2::new(field.min.x + 18.0, field.min.y),
        Pos2::new(field.max.x - 18.0, field.max.y),
    );
    let text_at = crate::components::controls::field_takes_the_whole_rectangle_at(
        rux.ui,
        inside,
        0.0,
        id,
        &format!("{name} field"),
        &font,
    );
    let mut child = rux.ui.new_child(egui::UiBuilder::new().max_rect(text_at));
    let response = child.text_box(
        egui::TextEdit::singleline(text)
            .id(id)
            .hint_text(crate::components::controls::placeholder(hint, &font, ink.i400))
            .font(font.clone())
            .frame(egui::Frame::NONE)
            .desired_width(text_at.width())
            .text_color(ink.i900),
    );
    let named = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, named.clone()));
    response
}

/// What the supervisor runs on, said on the left, and `Start supervisor` or `Stop supervisor` on the right.
fn terminal_footer(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    foot: Rect,
    running: bool,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let toggle = match running {
        true => PillButton::new("Stop supervisor").mark(Icon::Pause),
        false => PillButton::new("Start supervisor").mark(Icon::Play).primary(),
    };
    let width = toggle.measure(rux).x;
    let at = Rect::from_min_size(Pos2::new(foot.max.x - width, foot.min.y), Vec2::new(width, ROW));
    if toggle.show(rux, at).clicked() {
        let said = match running {
            true => Ok(board.stop_the_supervisor().message),
            false => board.start_the_supervisor().map(|answer| answer.message),
        };
        requests.push(Request::Message(said.unwrap_or_else(|problem| problem)));
    }
    let said = runs_on(board);
    caption_line(rux, Pos2::new(foot.min.x + 4.0, foot.center().y), &said, at.min.x - 16.0);
    requests
}

/// `Runs on claude · claude-opus-4-1 · high`, which is what the gear's settings come to.
fn runs_on(board: &AgentTasks) -> String {
    let (agent, endpoint) = board.supervisor_agent();
    let configuration = board.configuration();
    let model =
        configuration.supervisor_model.clone().unwrap_or_else(|| "its default model".to_owned());
    let mut said =
        format!("Runs on {} \u{b7} {model}", endpoint.unwrap_or_else(|| agent.name().to_owned()));
    if let Some(effort) = &configuration.supervisor_effort {
        said.push_str(&format!(" \u{b7} {effort} effort"));
    }
    said
}

/// A line of quiet words, its middle at `at.y`, cut to end before `right`.
fn caption_line(rux: &mut rux::Rux<'_>, at: Pos2, said: &str, right: f32) {
    let style = Style::sans(12.0);
    let ink = rux.theme().ink.i400;
    let words = fit(rux, style, said, (right - at.x).max(0.0));
    let galley = rux.text(style, &words, ink);
    let top = at.y - galley.size().y / 2.0;
    rux.painter().galley(Pos2::new(at.x, top), galley, ink);
}

// ---------------------------------------------------------------------------------------------- the skill

/// The Supervisor Skill view: the file's name, path and state with the layout switch, the window's own editor on
/// the file, and the row of buttons.
///
/// *"I want the edit skill in the modal to be an actual file editor, with line numbers etc, just like in file editor
/// pane."* The well is drawn here and the editor inside it is the window's: this reserves the room and names the
/// file in [`AgentTasks::embedded_file`], and `app::modal_editor` draws the tab holding the file there.
fn skill_view(board: &mut AgentTasks, rux: &mut rux::Rux<'_>, body: Rect) -> Vec<Request> {
    let mut requests = Vec::new();
    let path = match board.ensure_the_supervisor_skill_file() {
        Ok(path) => path,
        Err(problem) => {
            note(rux, body, &problem);
            return requests;
        }
    };
    let top = Rect::from_min_size(body.min, Vec2::new(body.width(), ROW));
    skill_toolbar(board, rux, top);
    let foot =
        Rect::from_min_size(Pos2::new(body.min.x, body.max.y - ROW), Vec2::new(body.width(), ROW));
    let room = Rect::from_min_max(
        Pos2::new(body.min.x, top.max.y + GAP),
        Pos2::new(body.max.x, foot.min.y - GAP),
    );
    let chat = rux.theme().chat;
    rux.chrome.surface(room, 20.0, chat.well, chat.carved_sm());
    let view = match board.supervisor_skill.layout {
        SkillLayout::Edit => EmbeddedView::Source,
        SkillLayout::Split => EmbeddedView::Split,
        SkillLayout::Preview => EmbeddedView::Preview,
    };
    let area = room.shrink2(Vec2::new(8.0, 10.0));
    board.supervisor_editor_area = Some(area);
    board.embedded_file = Some(EmbeddedFile { path, layer: rux.ui.layer_id(), area, view });
    requests.extend(skill_buttons(board, rux, foot));
    requests
}

/// `SKILL.md` in a carved chip, its path, whether it is saved, and the layout switch at the right.
fn skill_toolbar(board: &mut AgentTasks, rux: &mut rux::Rux<'_>, row: Rect) {
    let chat = rux.theme().chat;
    let theme = rux.theme();
    let style = Style::mono(12.0);
    let galley = rux.text(style, "SKILL.md", theme.ink.i500);
    let chip = Rect::from_min_size(
        Pos2::new(row.min.x, row.center().y - (galley.size().y + 14.0) / 2.0),
        Vec2::new(galley.size().x + 24.0, galley.size().y + 14.0),
    );
    rux.chrome.surface(chip, chip.height() / 2.0, chat.well, chat.carved_sm());
    rux::text::draw_centred(rux.painter(), chip, galley, theme.ink.i500);
    let names: Vec<String> =
        SkillLayout::ALL.iter().map(|layout| layout.label().to_owned()).collect();
    let chosen =
        SkillLayout::ALL.iter().position(|layout| *layout == board.supervisor_skill.layout);
    let switch = PillSwitch::new(&names, chosen, "Skill layout").small();
    let width = switch.measure(rux).x.max(210.0);
    let at = Rect::from_min_size(
        Pos2::new(row.max.x - width, row.center().y - 17.0),
        Vec2::new(width, 34.0),
    );
    if let Some(index) = switch.show(rux, at) {
        board.supervisor_skill.layout = SkillLayout::ALL[index];
    }
    let mut x = chip.max.x + 12.0;
    let (state, colour) = skill_state(board, &chat, theme);
    let badge = rux.text(Style::sans(11.5).semibold(), state, colour);
    let badge_at = Pos2::new(x, row.center().y - badge.size().y / 2.0);
    x += badge.size().x + 14.0;
    rux.painter().galley(badge_at, badge, colour);
    let response = rux.ui.interact(
        Rect::from_min_size(
            Pos2::new(chip.max.x + 12.0, row.min.y),
            Vec2::new(x - chip.max.x - 12.0, row.height()),
        ),
        rux.ui.id().with("agent-tasks-supervisor-skill-state"),
        egui::Sense::hover(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, state));
    let path = board.supervisor_skill_path().display().to_string();
    caption_line(rux, Pos2::new(x, row.center().y), &path, at.min.x - 16.0);
}

/// `Unsaved` in the sky colour, or `Shipped skill` or `Saved` in the quiet ink.
fn skill_state(
    board: &AgentTasks,
    chat: &rux::theme::Chat,
    theme: &rux::Theme,
) -> (&'static str, egui::Color32) {
    let editor = &board.supervisor_skill;
    match (editor.unsaved(), editor.is_default) {
        (true, _) => ("Unsaved", chat.sky),
        (false, true) => ("Shipped skill", theme.ink.i400),
        (false, false) => ("Saved", chat.faster),
    }
}

/// `Open in a tab` and `Reset to default` on the left, what the last save did in the middle, and `Revert` and
/// `Save & apply` on the right. `Ctrl`/`Cmd`+`S` in the editor is the window's own `Save`, and applies the same way.
fn skill_buttons(board: &mut AgentTasks, rux: &mut rux::Rux<'_>, row: Rect) -> Vec<Request> {
    let mut requests = Vec::new();
    let unsaved = board.supervisor_skill.unsaved();
    let shipped = !unsaved && board.supervisor_skill.is_default;
    let mut left = row.min.x;
    if pill(rux, &mut left, row, PillButton::new("Open in a tab").mark(Icon::File), true) {
        // *"If I click open in tab, it should close the modal and ensure the file editor pane is open with the
        // file."* The window moves the tab out of the modal into a pane and shows the editing area.
        requests.push(Request::OpenFile(board.supervisor_skill_path()));
        board.supervisor_open = false;
    }
    if pill(
        rux,
        &mut left,
        row,
        PillButton::new("Reset to default").mark(Icon::Rerun).ghost(),
        !shipped,
    ) {
        requests.push(saved(board.reset_the_supervisor_skill()));
    }
    let save = PillButton::new("Save & apply").mark(Icon::Check).primary();
    let save_width = save.measure(rux).x;
    let save_at = Rect::from_min_size(
        Pos2::new(row.max.x - save_width, row.min.y),
        Vec2::new(save_width, ROW),
    );
    if save.enabled(unsaved).show(rux, save_at).clicked() {
        requests.push(board.save_the_supervisor_skill_in_the_editor());
    }
    let revert = PillButton::new("Revert").ghost();
    let revert_width = revert.measure(rux).x;
    let revert_at = Rect::from_min_size(
        Pos2::new(save_at.min.x - 10.0 - revert_width, row.min.y),
        Vec2::new(revert_width, ROW),
    );
    if revert.enabled(unsaved).show(rux, revert_at).clicked() {
        requests.push(board.revert_the_supervisor_skill());
    }
    if let Some((said, worked)) = board.supervisor_skill.said.clone() {
        let colour = match worked {
            true => rux.theme().ink.i400,
            false => rux.theme().chat.slower,
        };
        let style = Style::sans(12.0);
        let words = fit(rux, style, &said, (revert_at.min.x - 16.0 - (left + 6.0)).max(0.0));
        let galley = rux.text(style, &words, colour);
        let top = row.center().y - galley.size().y / 2.0;
        rux.painter().galley(Pos2::new(left + 6.0, top), galley, colour);
    }
    requests
}

/// One pill in a row laid out from the left, moving `left` past it. Answers whether it was pressed.
fn pill(
    rux: &mut rux::Rux<'_>,
    left: &mut f32,
    row: Rect,
    button: PillButton<'_>,
    usable: bool,
) -> bool {
    let width = button.measure(rux).x;
    let at = Rect::from_min_size(Pos2::new(*left, row.min.y), Vec2::new(width, ROW));
    *left = at.max.x + 10.0;
    button.enabled(usable).show(rux, at).clicked()
}

/// What a reset, a restart or a failure says, for the status bar.
fn saved(answer: Result<crate::services::plugin_ui::Answer, String>) -> Request {
    Request::Message(match answer {
        Ok(answer) => answer.message,
        Err(problem) => problem,
    })
}

// ---------------------------------------------------------------------------------------------- the settings

/// The Settings view: the endpoint, model and effort the supervisor is started with, in the ticket modal's chip
/// fields, and beside them what a change does and `Restart supervisor`.
fn settings_view(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    body: Rect,
) -> Vec<Request> {
    let mut requests = Vec::new();
    let mut pen = body.min.y;
    let left = body.min.x;
    let width = FIELDS.min(body.width());
    let theme = rux.theme();
    let heading =
        rux.text(Style::sans(14.0).semibold(), "What the supervisor runs on", theme.ink.i900);
    let tall = heading.size().y;
    rux.painter().galley(Pos2::new(left, pen), heading, theme.ink.i900);
    pen += tall + 18.0;
    requests.extend(endpoint_field(board, rux, selects, &mut pen, left, width));
    requests.extend(model_field(board, rux, selects, &mut pen, left, width));
    requests.extend(effort_field(board, rux, selects, &mut pen, left, width));
    let aside = Rect::from_min_max(Pos2::new(left + width + 40.0, body.min.y), body.max);
    if aside.width() > 220.0 {
        requests.extend(settings_aside(board, rux, aside));
    } else {
        let below = Rect::from_min_max(Pos2::new(left, pen + 6.0), body.max);
        requests.extend(settings_aside(board, rux, below));
    }
    requests
}

/// The `Endpoint` field: the board's own endpoint first, then every endpoint that runs an agent.
fn endpoint_field(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    pen: &mut f32,
    left: f32,
    width: f32,
) -> Vec<Request> {
    let mut names = board.assignee_choices();
    names.retain(|name| name != "human");
    let current = board.configuration().supervisor_endpoint.clone().unwrap_or_default();
    if !current.is_empty() && !names.contains(&current) {
        names.insert(0, current.clone());
    }
    let options: Vec<(String, String)> =
        names.into_iter().map(|name| (name.clone(), name)).collect();
    let (_, board_endpoint) = board.default_assignee();
    let empty = format!(
        "The board's endpoint ({})",
        board_endpoint.unwrap_or_else(|| board.configuration().agent.name().to_owned())
    );
    let lead = SelectLead::Mark(Icon::Spark, crate::theme::color::agent());
    let chosen = super::ticket_modal::choice(
        rux,
        selects,
        pen,
        left,
        width,
        "Endpoint",
        lead,
        &options,
        &current,
        Some(&empty),
        18.0,
    );
    match chosen {
        Some(name) => vec![saved(board.set_the_supervisor_endpoint(Some(&name)))],
        None => Vec::new(),
    }
}

/// The `Model` field: the models the supervisor's agent has, and whatever the setting already names.
fn model_field(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    pen: &mut f32,
    left: f32,
    width: f32,
) -> Vec<Request> {
    let options: Vec<(String, String)> =
        board.supervisor_models().into_iter().map(|name| (name.clone(), name)).collect();
    let current = board.configuration().supervisor_model.clone().unwrap_or_default();
    let lead = SelectLead::Mark(Icon::Cube, rux.theme().ink.i400);
    let chosen = super::ticket_modal::choice(
        rux,
        selects,
        pen,
        left,
        width,
        "Model",
        lead,
        &options,
        &current,
        Some("The endpoint's default"),
        18.0,
    );
    match chosen {
        Some(model) => vec![saved(board.set_the_supervisor_model(Some(&model)))],
        None => Vec::new(),
    }
}

/// The `Effort` field.
fn effort_field(
    board: &mut AgentTasks,
    rux: &mut rux::Rux<'_>,
    selects: &mut std::collections::HashMap<&'static str, rux::components::SelectState>,
    pen: &mut f32,
    left: f32,
    width: f32,
) -> Vec<Request> {
    let options: Vec<(String, String)> =
        EFFORTS.iter().map(|level| ((*level).to_owned(), (*level).to_owned())).collect();
    let current = board.configuration().supervisor_effort.clone().unwrap_or_default();
    let lead = SelectLead::Mark(Icon::Spark, rux.theme().ink.i400);
    let chosen = super::ticket_modal::choice(
        rux,
        selects,
        pen,
        left,
        width,
        "Effort",
        lead,
        &options,
        &current,
        Some("The agent's default"),
        18.0,
    );
    match chosen {
        Some(level) => vec![saved(board.set_the_supervisor_effort(Some(&level)))],
        None => Vec::new(),
    }
}

/// What a change does, and `Restart supervisor` while one runs, on a carved card. It says nothing about the skill:
/// the Supervisor Skill tab is beside the gear (`task-2556`).
fn settings_aside(board: &mut AgentTasks, rux: &mut rux::Rux<'_>, area: Rect) -> Vec<Request> {
    let mut requests = Vec::new();
    let chat = rux.theme().chat;
    let theme = rux.theme();
    let card = Rect::from_min_size(
        area.min,
        Vec2::new(area.width().min(460.0), 150.0_f32.min(area.height())),
    );
    rux.chrome.surface(card, 22.0, chat.well, chat.carved_sm());
    let inside = card.shrink(20.0);
    let running = board.supervisor_is_running();
    let words = match running {
        true => "A change here is used the next time the supervisor starts. It is running now, so Restart \
                 supervisor starts it again on these settings.",
        false => "A change here is used the next time the supervisor starts.",
    };
    let galley = rux::text::wrapped(
        rux.painter(),
        Style::sans(12.5).leading(1.5),
        words,
        theme.ink.i500,
        inside.width(),
    );
    let tall = galley.size().y;
    rux.painter().with_clip_rect(card).galley(inside.min, galley, theme.ink.i500);
    let row = Rect::from_min_size(
        Pos2::new(inside.min.x, inside.min.y + tall + 16.0),
        Vec2::new(inside.width(), ROW),
    );
    let mut left = row.min.x;
    if running
        && pill(
            rux,
            &mut left,
            row,
            PillButton::new("Restart supervisor").mark(Icon::Rerun).primary(),
            true,
        )
    {
        requests.push(saved(board.restart_the_supervisor()));
    }
    requests
}
