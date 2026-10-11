//! Drawing the Agent-Tasks board.
//!
//! Nothing here decides anything. Which lane a card is in, where a drag lands and what a search matches
//! are all `services::agent_tasks`, and this file paints them and reports what was pressed. That is the
//! division every component in Unluminous makes, and it is what lets the board be tested with no window.
//!
//! ## The look is the window's, and none of it is chosen here
//!
//! Every colour comes from the `Look` the provider was handed, which is `theme::color` with the opacity
//! setting applied. The fonts are the settings' fonts and a row is `look.row_height`. So changing the
//! font size or the transparency in `Settings -> Appearance` changes the board in the same frame, and
//! there is no colour in this file that a plugin chose. The one exception is an epic's own colour, which
//! comes from the data and is confined to a card's left edge and its chip, the same allowance the file
//! icons already have.

pub(crate) mod card;
mod description;
pub(crate) mod detail;
mod kit;
mod lanes;
pub(crate) mod listings;
mod settings_page;
pub mod ticket_modal;

use egui::{Pos2, Rect, Vec2};

use crate::components::controls::WithHint as _;
use crate::services::agent_tasks::{AgentTasks, View};
use crate::services::plugin_ui::{Look, Request};
use crate::theme::crisp::CrispPainter;

/// How tall the strip across the top of the board is: the views, the sprint, the search and `Add Task`.
///
/// `padding: 22px 26px 18px` round a row 44 points tall, in the Agent-Tasks design (`task-2241`). The views
/// used to be a rail down the left; the design puts them in this row as a pill switch.
///
/// Read through `header_height` so a window set to large text gets a header that can hold it. See
/// `Look::scale`.
const HEADER_AT_DEFAULT: f32 = 84.0;

/// How tall the board's own header is in this window.
fn header_height(look: &Look<'_>) -> f32 {
    HEADER_AT_DEFAULT * look.scale()
}

/// How tall a contributed pane's own header is: the strip that names it, closes it and drags it.
///
/// 28 points, which is what a list row is and what the explorer's footer is, so a panel's header is the
/// same height wherever it is.
pub const PANE_HEADER: f32 = 28.0;

/// What a pane's header reported this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct HeaderOutcome {
    /// The panel is in the air, or was right clicked. `app::dock` is what acts on it.
    pub grab: crate::components::dock::Grab,
    /// The close cross was pressed.
    pub closed: bool,
}

/// A contributed pane's header: its name, how many things are in it, and a cross that closes it.
///
/// **The whole strip is the drag handle**, which is what `components::dock::handle` makes it and what every
/// other panel in Unluminous already has: the four drop bands, the strong rectangle showing where it would land
/// and the `Move to` menu on a right click all come with that one call. A pane with no header was a pane
/// that could not be moved and could only be put away from the rail.
pub fn pane_header(
    ui: &mut egui::Ui,
    area: Rect,
    label: &str,
    count: Option<&str>,
    panel: crate::app::dock::Panel,
    opacity: f32,
) -> HeaderOutcome {
    let mut outcome = HeaderOutcome::default();
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0, crate::theme::faded(crate::theme::color::toolbar(), opacity));
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(area.min.x, area.max.y - 1.0), area.max),
        0,
        crate::theme::color::divider(),
    );
    let mut pen = area.min.x + 10.0;
    pen += text(
        &painter,
        Pos2::new(pen, area.center().y - 7.0),
        label,
        12.5,
        crate::theme::color::text_control(),
    );
    if let Some(count) = count {
        text(
            &painter,
            Pos2::new(pen + 8.0, area.center().y - 6.5),
            count,
            11.5,
            crate::theme::color::text_dim(),
        );
    }
    // **The handle first and the cross after it.** egui gives a pointer to the *last* widget that wanted it,
    // and the handle covers the whole strip, so a cross added first was a cross the handle swallowed. That is
    // the order `components::dock::handle`'s own documentation asks for, and it is why every other panel's
    // header adds its controls after the handle.
    outcome.grab = crate::components::dock::handle(ui, area, panel);
    let cross =
        Rect::from_center_size(Pos2::new(area.max.x - 14.0, area.center().y), Vec2::splat(18.0));
    outcome.closed = crate::components::controls::icon_button(
        ui,
        cross,
        // **Prefixed**, because a board tab and an editor tab can be on the screen at the same time
        // and `Close <name>` would be two controls with one name -- which the style guide forbids and
        // which the screenshot tests find controls by (`task-1984` S8).
        &format!("Close board {label}"),
        crate::theme::icon::cross,
    );
    outcome
}
/// The gap round everything, which is the gap the explorer already leaves.
pub(crate) const PAD: f32 = 8.0;

/// How far the header and the lanes are kept from the pane's sides: `padding: 22px 26px 18px` on the header
/// and `6px 26px 26px` on the lanes, in the Agent-Tasks design.
pub(crate) const SIDE: f32 = 26.0;

/// The pane in the rail: the header, then whichever view is chosen.
///
/// A ticket is the modal and nothing else, which is why the pane and the tab draw the same thing. See [`tab`].
pub fn pane(board: &mut AgentTasks, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    page(board, ui, look)
}

/// The tab in the editing area: the header and the chosen view, and nothing else.
///
/// **A ticket is the modal.** This used to be the arrangement the Markdown side by side view uses — the
/// lanes on the left and the open ticket on the right — and `task-1771` reports what that is like to work
/// beside: *"when an agent finishes a task and moves it to agent done, for some reason a side panel is
/// opening up that shows the task. I'm not sure what that is, but I don't want it."* It was not the agent
/// finishing; it was the agent **reading** the ticket, which opened the detail, which split the board. The
/// read no longer opens anything (see the `task` command) and there is no longer a half of the board for it
/// to open into, which is also what the page this board is modelled on does: a row is clicked and a modal
/// comes up over the whole window.
pub fn tab(board: &mut AgentTasks, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    page(board, ui, look)
}

/// The header and the chosen view, drawn with the board's `rux` state.
///
/// **The ground is the window's, not the board's.** It is painted before anything here is drawn, and only
/// the window knows where. See `UnluminousApp::paint_the_chrome`.
///
/// **No pump here.** Every open provider is pumped once a frame by `UnluminousApp::let_the_plugins_catch_up`,
/// which is what makes an agent's output arrive while its board is put away. Pumping again from the drawing
/// meant every terminal was read twice a frame.
///
/// The `rux` state is taken off the board for the length of the drawing and put back at the end, because the
/// lanes read the board while they draw and the state cannot be borrowed from it at the same time.
fn page(board: &mut AgentTasks, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let state = kit::take(board, look);
    let mut requests = Vec::new();
    let header = Rect::from_min_size(area.min, Vec2::new(area.width(), header_height(look)));
    let (said, chosen, spare) = header_row(board, &state, ui, look, header);
    requests.extend(said);
    let body = Rect::from_min_max(Pos2::new(area.min.x, header.max.y), area.max);
    requests.extend(body_for(board, &state, ui, look, body, spare));
    take_the_chosen_view(board, chosen);
    kit::give_back(board, state);
    requests
}

/// Act on a press in the view switch, once the drawing that read the board is over.
fn take_the_chosen_view(board: &mut AgentTasks, chosen: Option<View>) {
    if let Some(view) = chosen {
        board.set_view(view);
        board.close_detail();
    }
}

/// The Settings page.
pub fn settings(board: &mut AgentTasks, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    settings_page::show(board, ui, look)
}

/// Whichever of the four views is chosen.
///
/// `spare` is the room the header left beside a listing's heading, which is where a listing puts the strip that
/// makes a sprint or an epic, so the groups under it start straight under the header.
fn body_for(
    board: &mut AgentTasks,
    state: &rux::RuxState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    spare: Option<Rect>,
) -> Vec<Request> {
    // The listings were laid out beside the rail, eight points in from its edge, and keep that inset now the
    // rail is in the header: the room they were measured in is the room they are given.
    let inset = area.shrink2(Vec2::new(SIDE * look.scale() - PAD, 0.0));
    match board.current_view() {
        View::Board => lanes::show(board, state, ui, look, area),
        // The three listings are `components::agent_tasks::listings` since `task-1771`, which asked for them
        // to be what the page this board is modelled on has rather than a flat column of cards.
        View::Backlog => listings::groups(board, state, ui, look, inset, View::Backlog, spare),
        View::Completed => listings::groups(board, state, ui, look, inset, View::Completed, spare),
        View::Epics => listings::epics(board, state, ui, look, inset, spare),
    }
}

/// The mark for each view in the switch, which is the design's: `storyboard`, `layers`, `check` and `cube`.
fn view_icon(view: View) -> rux::Icon {
    match view {
        View::Board => rux::Icon::Storyboard,
        View::Backlog => rux::Icon::Layers,
        View::Completed => rux::Icon::Check,
        View::Epics => rux::Icon::Cube,
    }
}

/// What the header shows under the sprint's name: how many tickets, and how many of their todos are done.
///
/// `4 tasks · 20 of 21 todos done`, and the share of the todos that are done is what the ring round the
/// sprint draws. A sprint with no todos at all says only how many tickets it has, and its ring is empty.
pub(crate) struct SprintSummary {
    pub line: String,
    pub fraction: f32,
}

pub(crate) fn sprint_summary(board: &crate::services::agent_tasks::model::Board) -> SprintSummary {
    let (done, total) = board.tasks().fold((0_i64, 0_i64), |(done, total), task| {
        (done + task.todo_done_count.max(0), total + task.todo_count.max(0))
    });
    let tasks = card::plural(board.total() as i64, "task");
    match total {
        0 => SprintSummary { line: tasks, fraction: 0.0 },
        total => SprintSummary {
            line: format!("{tasks} \u{b7} {done} of {total} todos done"),
            fraction: (done as f32 / total as f32).clamp(0.0, 1.0),
        },
    }
}

/// The strip across the top: the views, the sprint and its progress, the search box and `Add Task`.
///
/// `task-2241`. The Agent-Tasks design's header: a pill switch of the four views, a 44 point ring with the
/// sprint's name and its counts beside it, then a search well and the `Add Task` pill at the right, 22 points
/// apart. **What gives way when the pane is narrow**: the search box shrinks and then goes, the views lose
/// their words and keep their marks, `Add Task` loses its words and becomes a round button, the sprint's name is
/// cut short, and last the views go. `Add Task` is always drawn, because a board somebody cannot add a ticket to
/// is worse than one whose heading is short.
///
/// On the three listings the heading is the view's own name and there is no ring, no search and no `Add
/// Task`: the search filters lanes and `Add Task` puts a ticket in one, and neither means anything on a page
/// of sprints or of epics.
fn header_row(
    board: &mut AgentTasks,
    state: &rux::RuxState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> (Vec<Request>, Option<View>, Option<Rect>) {
    let mut requests = Vec::new();
    let scale = look.scale();
    let listing = board.current_view() != View::Board;
    let mut spare: Option<Rect> = None;
    let row = Rect::from_min_max(
        Pos2::new(area.min.x + SIDE * scale, area.min.y + 22.0 * scale),
        Pos2::new(area.max.x - SIDE * scale, area.max.y - 18.0 * scale),
    );
    let gap = 22.0 * scale;
    let views: Vec<(rux::Icon, &str)> =
        View::ALL.iter().map(|view| (view_icon(*view), view.label())).collect();
    let chosen_index = View::ALL.iter().position(|view| *view == board.current_view()).unwrap_or(0);
    let summary = sprint_summary(board.board());
    let title = match board.current_view() {
        View::Board => match board.board().sprint.as_ref() {
            Some(sprint) => sprint.name.clone(),
            None => "No active sprint".to_owned(),
        },
        other => other.label().to_owned(),
    };
    let mut query = board.query().to_owned();
    let mut searched = false;
    let mut adding = false;
    let mut picked = None;
    kit::layer(state, ui, "header", area, kit::CHIP_REACH * scale, |rux| {
        let switch = rux::components::ViewSwitch::new(&views, chosen_index);
        let full = switch.measure(rux).x;
        let marks = switch.measure_marks(rux).x;
        let add_label = "Add Task";
        let pill_width = rux::components::PillButton::new(add_label)
            .primary()
            .large()
            .mark(rux::Icon::Plus)
            .measure(rux)
            .x;
        let round = 44.0 * scale;
        let heading_style = rux::Style::sans(22.0 * scale).semibold().tracking(-0.02);
        let sub_style = rux::Style::sans(11.5 * scale);
        let heading_width = rux.measure(heading_style, &title).x;
        let sub_width = match listing {
            true => 0.0,
            false => rux.measure(sub_style, &summary.line).x,
        };
        let ring = match listing {
            true => 0.0,
            false => 44.0 * scale + 14.0 * scale,
        };
        let sprint_width = ring + heading_width.max(sub_width);
        let width = row.width();
        // **What gives way, in order**: the search box shrinks and then goes, the views lose their words, `Add
        // Task` loses its words and becomes a round button, and last the views go. The views are still reachable
        // from the command line and the menu, and a board somebody cannot add a ticket to is worse than one whose
        // views are a command away.
        let (switch_width, add_width, add_round) = match listing {
            true => match (full <= width, marks <= width) {
                (true, _) => (full, 0.0, false),
                (false, true) => (marks, 0.0, false),
                (false, false) => (0.0, 0.0, false),
            },
            false => {
                if full + gap + sprint_width + gap + 140.0 * scale + gap + pill_width <= width {
                    (full, pill_width, false)
                } else if marks + gap + 44.0 * scale + gap + pill_width <= width {
                    (marks, pill_width, false)
                } else if marks + gap + round <= width {
                    (marks, round, true)
                } else {
                    (0.0, round, true)
                }
            }
        };
        let after_switch = match switch_width > 0.0 {
            true => row.min.x + switch_width + gap,
            false => row.min.x,
        };
        // The search box takes what is spare up to the design's 260 points, and goes below 140.
        let left_over = row.max.x - add_width - gap - after_switch - sprint_width - gap;
        let search_width = match listing {
            true => 0.0,
            false => left_over.min(260.0 * scale),
        };
        let show_search = search_width >= 140.0 * scale;
        if switch_width > 0.0 {
            let switch_rect = Rect::from_min_size(row.min, Vec2::new(switch_width, row.height()));
            picked = rux::components::ViewSwitch::new(&views, chosen_index).show(rux, switch_rect);
        }

        // The sprint: the ring, its name and its counts. Cut short rather than wrapped, so a long name gets
        // shorter instead of taller, and drawn only into the room the controls on the right leave.
        let right_edge = row.max.x
            - add_width
            - match (show_search, add_width > 0.0) {
                (true, _) => search_width + gap * 2.0,
                (false, true) => gap,
                (false, false) => 0.0,
            };
        let mut pen = after_switch;
        if !listing && right_edge - pen >= 44.0 * scale {
            let disc = Rect::from_min_size(
                Pos2::new(pen, row.center().y - 22.0 * scale),
                Vec2::splat(44.0 * scale),
            );
            rux::components::SprintRing::new(summary.fraction).show(rux, disc);
            pen = disc.max.x + 14.0 * scale;
        }
        let room = (right_edge - pen).max(0.0);
        if room > 24.0 * scale {
            let ink = rux.theme().ink;
            let heading = rux.elided(heading_style, &title, ink.i900, room);
            match listing {
                true => {
                    let drawn = heading.size().x;
                    rux::text::draw_left_capitals(
                        rux.painter(),
                        Pos2::new(pen, row.center().y),
                        heading,
                        heading_style,
                        ink.i900,
                    );
                    let left = pen + drawn + gap;
                    if row.max.x - left > 120.0 * scale {
                        spare = Some(Rect::from_min_max(Pos2::new(left, row.min.y), row.max));
                    }
                }
                false => {
                    let sub = rux.elided(sub_style, &summary.line, ink.i400, room);
                    let gap_between = 2.0 * scale;
                    let top =
                        row.center().y - (heading.size().y + gap_between + sub.size().y) / 2.0;
                    let below = top + heading.size().y + gap_between;
                    rux.painter().galley(Pos2::new(pen, top), heading, ink.i900);
                    rux.painter().galley(Pos2::new(pen, below), sub, ink.i400);
                }
            }
        }

        if !listing {
            let add_rect = Rect::from_min_size(
                Pos2::new(row.max.x - add_width, row.center().y - 21.0 * scale),
                Vec2::new(add_width, 42.0 * scale),
            );
            adding = match add_round {
                true => rux::components::RoundButton::action(rux::Icon::Plus, 16.0, "Add Task")
                    .show(rux, add_rect)
                    .clicked(),
                false => rux::components::PillButton::new(add_label)
                    .primary()
                    .large()
                    .mark(rux::Icon::Plus)
                    .show(rux, add_rect)
                    .clicked(),
            };
            if show_search {
                let well = Rect::from_min_size(
                    Pos2::new(add_rect.min.x - gap - search_width, row.center().y - 21.0 * scale),
                    Vec2::new(search_width, 42.0 * scale),
                );
                let inside = rux::components::search_well(rux, well);
                searched = search_box(rux.ui, inside, &mut query, scale);
            }
        }
    });

    // What the board last said, in the strip under the row, where there is always room for it.
    if !board.message().is_empty() {
        let painter = ui.painter();
        let said = painter.crisp_layout_no_wrap(
            board.message().to_owned(),
            egui::FontId::proportional(11.0 * scale),
            look.palette.text_dim,
        );
        let at = Pos2::new(row.max.x - said.size().x, row.max.y + 2.0 * scale);
        if at.x > row.min.x {
            painter.crisp_galley(at, said, look.palette.text_dim);
        }
    }
    if searched {
        if let Err(problem) = board.search(&query) {
            requests.push(Request::Message(problem));
        }
    }
    if adding {
        if let Err(problem) = board.command_now("new-task", &[]).map(|_| ()) {
            requests.push(Request::Message(problem));
        }
    }
    (requests, picked.and_then(|index| View::ALL.get(index).copied()), spare)
}

/// The words of the search box, in the well `rux` drew: answers whether the query changed.
///
/// The field the lanes filter by. Everything behind it was built and tested and only the control that sets it
/// was missing once — so the only way to search was the command line, and **an agent that searched left the
/// board filtered with nothing on screen saying so and no way to clear it.**
fn search_box(ui: &mut egui::Ui, inside: Rect, query: &mut String, scale: f32) -> bool {
    let font = egui::FontId::proportional(12.5 * scale);
    let id = ui.id().with("agent-tasks-search");
    let text = crate::components::controls::field_takes_the_whole_rectangle_at(
        ui,
        inside,
        0.0,
        id,
        "Search tasks field",
        &font,
    );
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(text));
    let response = field.add(
        egui::TextEdit::singleline(query)
            .id(id)
            .hint_text(crate::components::controls::placeholder(
                "Search tasks",
                &font,
                crate::theme::color::text_faint(),
            ))
            .font(font.clone())
            .frame(egui::Frame::NONE)
            .desired_width(text.width())
            .text_color(crate::theme::color::text_strong()),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Search tasks")
    });
    // **Enter typed into this field is taken out of the frame.** egui's single-line editor gives up focus on
    // Enter and leaves the press in the input, so by the time the lanes are drawn no text box holds the
    // keyboard any more and the board took the same Enter as "open the ticket the ring is on".
    enter_was_used(ui, &response);
    response.changed()
}

/// A colour moved towards white, and one moved towards black.
///
/// **The amounts are small.** `darken` scales every channel, so a large amount takes the blue out of a
/// blue: at 0.22 the bottom right of the primary button was about thirty levels of blue below the
/// reference's, which the third review measured. The picture's own gradients are shallow — `#567DF9` to
/// `#4365E5` across the whole of `+ Add Task` — and 0.05 and 0.09 reproduce that pair to within a few
/// units a channel.
///
/// What a gradient's two ends are made of. The board never names a second colour for the light end of a
/// button: it lightens and darkens the one it already has, so the palette stays closed and a gradient can
/// never disagree with the flat colour it was derived from.
pub(crate) fn lighten(colour: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |channel: u8| {
        let value = f32::from(channel);
        (value + (255.0 - value) * amount.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_premultiplied(
        mix(colour.r()),
        mix(colour.g()),
        mix(colour.b()),
        colour.a(),
    )
}

pub(crate) fn darken(colour: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |channel: u8| (f32::from(channel) * (1.0 - amount.clamp(0.0, 1.0))).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        mix(colour.r()),
        mix(colour.g()),
        mix(colour.b()),
        colour.a(),
    )
}

/// A label drawn at a position, which is what nearly every line on the board is.
///
/// One function rather than the same four lines in twelve places, and it returns how wide it drew so a
/// caller can put the next thing after it.
pub(crate) fn text(
    painter: &egui::Painter,
    at: Pos2,
    said: &str,
    size: f32,
    tint: egui::Color32,
) -> f32 {
    let galley =
        painter.crisp_layout_no_wrap(said.to_owned(), egui::FontId::proportional(size), tint);
    let width = galley.size().x;
    painter.crisp_galley(at, galley, tint);
    width
}

/// Take Enter out of the frame when a single-line field has just acted on it.
///
/// egui's single-line editor gives up the keyboard when Enter is pressed and does not consume the press, so
/// anything drawn afterwards that listens for Enter sees it and believes no field was involved. The board's ring
/// opens a ticket on Enter and a modal's footer confirms on Enter, so both used to answer a key that was meant
/// for a field. Every field on the board that does something with Enter calls this after it.
pub(crate) fn enter_was_used(ui: &egui::Ui, response: &egui::Response) {
    if response.lost_focus() || response.has_focus() {
        ui.ctx().input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
    }
}

/// Whether a field was left by pressing Enter, taking that Enter out of the frame.
///
/// One function rather than the same two conditions written in four places, and it consumes as well as answers,
/// which is the half that was missing: see [`enter_was_used`].
pub(crate) fn enter_was_used_and_pressed(ui: &egui::Ui, response: &egui::Response) -> bool {
    let pressed = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    if pressed {
        ui.ctx().input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        });
    }
    pressed
}

/// A label wrapped to `width` and cut off with an ellipsis after `lines` of it.
///
/// **It used to wrap without limit**, which is not what the name said and not what any caller wanted: a card
/// title of forty words drew down over the card's own footer and over the card under it, because the caller
/// advanced by one card height whatever was drawn. The same wrapping ran a long todo through the row beneath it
/// and a long scheduled command through the next schedule.
///
/// The two buttons that say whether a piece of markdown is read as its source or as markdown.
///
/// `task-28`. `at` is where the pair goes, right aligned, and the answer is the state the pair was left in, or
/// `None` when neither was pressed.
///
/// The icons are `icon::view_mode`'s own `Raw` and `Preview`, which are the pictures the editor's own view
/// buttons use for exactly this choice — so switching a description between source and markdown looks like
/// switching a `.md` file between them, because it is the same choice.
///
/// There is no check for a markdown plugin, because **there is no markdown plugin**: markdown is built into
/// `unluminous-core`. See `components::markdown_text`.
pub(crate) fn raw_or_rendered(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    at: Rect,
    id: &str,
    rendered: bool,
) -> Option<bool> {
    use crate::app::ViewMode;
    let mut chosen = None;
    let size = 18.0;
    for (index, (mode, wants)) in
        [(ViewMode::Raw, false), (ViewMode::Preview, true)].into_iter().enumerate()
    {
        let button = Rect::from_min_size(
            Pos2::new(at.max.x - size * 2.0 - 4.0 + index as f32 * (size + 4.0), at.min.y),
            Vec2::splat(size),
        );
        let on = rendered == wants;
        // Named for what it does rather than for what it draws, because that is what somebody reads out and what
        // a test asks for. Two controls in one window may not share a name, so the id of the thing being looked
        // at is part of it: a ticket shows one description and several comments at once.
        let name = match wants {
            true => format!("Read {id} as markdown"),
            false => format!("Read {id} as its source"),
        };
        let response = ui.interact(
            button,
            ui.id().with(("agent-tasks-view", id, index)),
            egui::Sense::click(),
        );
        let painter = ui.painter();
        if on {
            painter.rect_filled(
                button,
                egui::CornerRadius::same(look.corner_radius as u8),
                look.palette.selected_row,
            );
        } else if response.hovered() {
            painter.rect_filled(
                button,
                egui::CornerRadius::same(look.corner_radius as u8),
                look.palette.control,
            );
        }
        let tint = match on {
            true => look.palette.text_strong,
            false => look.palette.text_dim,
        };
        crate::theme::icon::view_mode(painter, button.shrink(4.0), mode, tint);
        let said = name.clone();
        response
            .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, &said));
        let _ = response.clone().with_hint(name);
        if response.clicked() && !on {
            chosen = Some(wants);
        }
    }
    chosen
}

/// One value chosen from a list, drawn into `at`, answering what was chosen when it changed.
///
/// `task-28`: "Dropdowns. We need UI dropdowns for values." This is the **only** place the plugin draws a list
/// of values, so a ticket's `Model` and the Settings page's `Default model` are the same control offering the
/// same list — they used to be a text field and a text field, and each would have had to be corrected
/// separately.
///
/// `components::controls::dropdown` is what it is made of, which the toolbar and `Settings -> Appearance`
/// already use, so it opens, closes and reads like every other dropdown in Unluminous. `options` is `(value, said)`
/// pairs: the value written down and the words a person reads, because `agent_done` is not what anybody calls
/// that lane. `empty` is what the list calls holding nothing, for a value that may hold nothing; `None` means
/// the value is required and there is no way to clear it.
///
/// Choosing what is already chosen answers `None`, so nothing is written and nothing is reported.
pub(crate) fn value_dropdown(
    ui: &mut egui::Ui,
    at: Rect,
    name: &str,
    options: &[(String, String)],
    chosen: &str,
    empty: Option<&str>,
) -> Option<String> {
    value_dropdown_over(ui, at, name, options, chosen, empty, true)
}

/// The same, told whether to draw its own ground. See `controls::dropdown_over`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn value_dropdown_over(
    ui: &mut egui::Ui,
    at: Rect,
    name: &str,
    options: &[(String, String)],
    chosen: &str,
    empty: Option<&str>,
    ground: bool,
) -> Option<String> {
    let showing = options
        .iter()
        .find(|(value, _)| value == chosen)
        .map(|(_, said)| said.clone())
        .unwrap_or_else(|| empty.unwrap_or("").to_owned());
    let rows: Vec<(String, String)> = match empty {
        Some(said) => std::iter::once((String::new(), said.to_owned()))
            .chain(options.iter().cloned())
            .collect(),
        None => options.to_vec(),
    };
    let picked =
        crate::components::controls::dropdown_over(ui, at, &showing, name, None, ground, |ui| {
            let mut picked = None;
            for (value, said) in &rows {
                // `selectable_label` rather than a painted row, because the list is inside egui's own popup and this
                // is what every other dropdown in Unluminous puts in one.
                if ui.selectable_label(value == chosen, said).clicked() {
                    picked = Some(value.clone());
                }
            }
            picked
        });
    picked.filter(|value| value != chosen)
}

/// The text is shortened until it fits rather than the drawing being clipped, so that what is cut off is
/// announced by an ellipsis instead of being silently invisible. A first slice keeps the search cheap on a
/// description sized string: how many characters could possibly fit is known from the width and the size, and
/// there is no point laying out the other ten thousand. Only the tests call it on its own now; the board
/// calls [`clipped_in`].
#[cfg(test)]
pub(crate) fn clipped(
    painter: &egui::Painter,
    at: Pos2,
    said: &str,
    size: f32,
    tint: egui::Color32,
    width: f32,
    lines: usize,
) {
    clipped_in(painter, at, said, egui::FontId::proportional(size), tint, width, lines);
}

/// The same, in a face the caller names.
///
/// A card's title is set in the bold face, which is what the picture shows and is the one place on the board
/// where the weight is doing work: it is the line a person reads to find the ticket they want.
pub(crate) fn clipped_in(
    painter: &egui::Painter,
    at: Pos2,
    said: &str,
    font: egui::FontId,
    tint: egui::Color32,
    width: f32,
    lines: usize,
) {
    let size = font.size;
    let most = (width / (size * 0.4)).max(1.0) as usize * lines.max(1) + 8;
    let short = match said.char_indices().nth(most) {
        Some((at, _)) => &said[..at],
        None => said,
    };
    let mut galley = painter.crisp_layout(short.to_owned(), font.clone(), tint, width);
    let room = size * 1.35 * lines.max(1) as f32;
    if galley.size().y > room {
        // Trimmed a character at a time from the end, which is at most a handful of steps from the slice above.
        let mut fits = short;
        while !fits.is_empty() {
            let cut = match fits.char_indices().next_back() {
                Some((at, _)) => at,
                None => break,
            };
            fits = &said[..cut];
            let tried =
                painter.crisp_layout(format!("{}…", fits.trim_end()), font.clone(), tint, width);
            if tried.size().y <= room {
                galley = tried;
                break;
            }
        }
    }
    painter.crisp_galley(at, galley, tint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::text_renderer::TextRenderer;
    use crate::settings::Settings;
    use egui::Color32;

    #[test]
    fn the_boards_own_fixed_heights_scale_with_the_editor_font() {
        let renderer = TextRenderer::new();
        let mut settings = Settings::new();
        settings.font_size = crate::settings::DEFAULT_FONT_SIZE;
        let normal = Look::of(&settings, &renderer);
        // `padding: 22px 26px 18px` round a row 44 points tall.
        assert_eq!(header_height(&normal), 22.0 + 44.0 + 18.0);

        settings.font_size = crate::settings::DEFAULT_FONT_SIZE * 2.0;
        let large = Look::of(&settings, &renderer);
        assert_eq!(header_height(&large), HEADER_AT_DEFAULT * 2.0);
    }

    /// The line under the sprint's name and the share its ring draws, worked out from the todos of every
    /// ticket on the board.
    #[test]
    fn the_sprint_summary_counts_the_todos_of_every_ticket() {
        use crate::services::agent_tasks::model::{Board, Lane, Status};
        let mut board = Board::default();
        assert_eq!(sprint_summary(&board).line, "0 tasks");
        assert_eq!(sprint_summary(&board).fraction, 0.0);
        let mut done = crate::services::agent_tasks::model::Task::for_tests("task-1");
        done.todo_count = 4;
        done.todo_done_count = 3;
        let mut empty = crate::services::agent_tasks::model::Task::for_tests("task-2");
        empty.todo_count = 0;
        board.lanes = vec![
            Lane { status: Status::New, tasks: vec![empty] },
            Lane { status: Status::AgentDone, tasks: vec![done] },
        ];
        let summary = sprint_summary(&board);
        assert_eq!(summary.line, "2 tasks \u{b7} 3 of 4 todos done");
        assert_eq!(summary.fraction, 0.75);
    }

    #[test]
    fn lighten_and_darken_move_a_colour_towards_white_or_black_and_leave_its_alpha() {
        // `theme::closed_palette::no_component_writes_a_colour_of_its_own` refuses a literal RGB
        // triple in `components/`, so the colour under test is one already in the closed palette
        // rather than one invented here — a component cannot choose a colour of its own, and neither
        // can this test.
        let start = crate::theme::color::text_dim().gamma_multiply(0.8);
        let alpha = start.a();
        assert_eq!(lighten(start, 0.0), start, "no amount changes nothing");
        let lit = lighten(start, 0.5);
        assert!(lit.r() > start.r(), "moved towards white");
        assert_eq!(lit.a(), alpha, "alpha is untouched");
        assert_eq!(lighten(start, 1.0).r(), 255, "the whole way is white");

        let dark = darken(start, 0.5);
        assert!(dark.r() < start.r(), "moved towards black");
        assert_eq!(dark.a(), alpha);
        assert_eq!(darken(start, 1.0).r(), 0, "the whole way is black");

        // Out of range amounts are clamped rather than over- or under-shooting.
        assert_eq!(lighten(start, 2.0), lighten(start, 1.0));
        assert_eq!(darken(start, -1.0), start);
    }

    /// A fresh `egui::Context` has no fonts until a pass has begun once — `begin_pass` is what loads
    /// them — and after that a `Painter` can be built and drawn into directly, with no `Ui` anywhere.
    /// That is what makes this a test of the truncation arithmetic rather than of a window.
    fn a_painter() -> egui::Painter {
        let context = egui::Context::default();
        context.begin_pass(egui::RawInput::default());
        egui::Painter::new(context, egui::LayerId::debug(), egui::Rect::EVERYTHING)
    }

    /// What ended up drawn, read back off the layer the painter drew into.
    fn drawn_text(painter: &egui::Painter) -> Vec<String> {
        // `begin_pass` loaded the font atlas into a texture delta that nothing here ever uploads
        // anywhere, and `epaint` panics on drop rather than let one go unhandled in silence — so it
        // is cleared explicitly once the shapes it came with have been read.
        let mut output = painter.ctx().end_pass();
        let drawn = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        output.textures_delta.clear();
        drawn
    }

    #[test]
    fn text_that_fits_is_drawn_exactly_as_it_was_given() {
        let painter = a_painter();
        clipped(&painter, Pos2::ZERO, "a short title", 14.0, Color32::WHITE, 400.0, 1);
        assert_eq!(drawn_text(&painter), vec!["a short title".to_owned()]);
    }

    #[test]
    fn text_that_does_not_fit_is_shortened_with_an_ellipsis() {
        let painter = a_painter();
        let long = "x".repeat(400);
        clipped(&painter, Pos2::ZERO, &long, 14.0, Color32::WHITE, 80.0, 1);
        let drawn = drawn_text(&painter);
        assert_eq!(drawn.len(), 1);
        assert!(drawn[0].ends_with('…'), "{:?}", drawn[0]);
        assert!(drawn[0].len() < long.len(), "it was actually shortened");
    }

    /// Two lines of room hold roughly twice what one does, so the same words that had to be cut short
    /// at one line are left whole once a second line is on offer.
    #[test]
    fn a_second_line_of_room_holds_what_a_single_line_could_not() {
        let painter = a_painter();
        let title = "a title just long enough to wrap onto a second line of a card";
        clipped(&painter, Pos2::new(0.0, 0.0), title, 14.0, Color32::WHITE, 140.0, 1);
        let one_line = drawn_text(&painter);
        assert_eq!(one_line.len(), 1);
        assert!(one_line[0].ends_with('…'), "one line is not room enough for it: {one_line:?}");

        let painter = a_painter();
        clipped(&painter, Pos2::new(0.0, 0.0), title, 14.0, Color32::WHITE, 140.0, 2);
        let two_lines = drawn_text(&painter);
        assert_eq!(two_lines.len(), 1);
        assert!(
            two_lines[0].chars().count() > one_line[0].chars().count(),
            "a second line holds more of it: {one_line:?} against {two_lines:?}"
        );
    }
}
