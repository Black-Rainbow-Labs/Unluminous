//! The Agent-Chat pane: the one raised card, its header, the conversation and the composer.
//!
//! `_agent_output/task-1767-agent-chat/reference-chat.png` is the picture this is measured against
//! and `tasks/task-1767-agent-chat-tdd.md` §1 says how it was made. The structure is the ai-service
//! LLM chat page's own — `ChatPanel` wrapping `ChatHeader`, `ChatConversation` and `ChatComposer` —
//! wearing the dark neumorphic palette the Agent-Tasks board is drawn in.
//!
//! ## The drawing changes nothing, and `pane` is where the change happens
//!
//! Every function here that draws takes a rectangle, draws, and reports an [`Act`]; not one of them
//! changes the conversation. [`pane`] is the provider's own entry rather than a component — it is what
//! `UiProvider::pane` calls — and it is the one place the acts are applied, after everything has been
//! drawn. That split is what lets the whole surface be drawn from a borrow of the conversation while
//! the things that change it need a mutable one.
//!
//! ## The ground is the window's
//!
//! `show_the_plugin_panes` fills the pane and reserves the decoration's slot before this is called. A
//! second ground painted here would go into the painter *after* that slot and wash the decoration
//! out, which is the fault `task-1765` records for the board. So the only surface painted here is the
//! panel itself, through `Chrome::raised`.

pub mod blocks;
pub mod composer;
pub mod message;
pub mod settings_page;
pub mod welcome;

use egui::{CornerRadius, Pos2, Rect, Stroke, Vec2};

use crate::components::controls;
use crate::services::agent_chat::{AgentChat, ModelSelect, Parts};
use crate::services::plugin_ui::{Look, Request};
use crate::services::vello_canvas::{Fill, Lift};
use crate::theme::crisp::CrispPainter;
use crate::theme::icon;

/// A colour moved towards black, which is the far end of a button's own gradient.
///
/// `components::agent_tasks` already has one and it is the same arithmetic; it is re-exported here so
/// that both plugins darken a colour the same way rather than by two functions that agree today.
pub(crate) use crate::components::agent_tasks::darken;

/// The gap round the panel, which is the gap the explorer already leaves.
pub const PAD: f32 = 8.0;
/// The panel's own padding, from `ChatPanel.module.css`.
pub const INNER: f32 = 10.0;
/// The panel's corner radius: `--r-lg`.
pub const RADIUS: f32 = 18.0;
/// The header row, from `ChatHeader.module.css`'s padding plus its 13 point name.
pub const HEADER: f32 = 32.0;
/// Between two rows of the conversation.
///
/// Half of `ChatConversation.module.css`'s own `gap: 14px`, since `task-2200`: *"There's too much margin
/// between messages. it should be about half."* A bubble here carries more padding of its own than the
/// reference's does, so the reference's gap read as twice as much.
pub const GAP: f32 = 7.0;
/// How far in from the card's edge the conversation's own rows sit.
///
/// **Two points, where it used to be the card's full ten.** `task-1848`: "the margin on the sides of the
/// messages is too large. It should be much smaller." The card keeps [`INNER`] for its header and its
/// composer, which are controls and want room round them, and the conversation is given nearly the whole
/// width — a message is text to read, and every point spent on margin here is a point taken off the line
/// length twice over, because a bubble is then capped at a share of what is left.
///
/// Not zero: a bubble's own shadow reaches a little past its edge, and at zero the right-hand one was
/// clipped by the card.
pub const LIST_INSET: f32 = 2.0;

/// How tall the model selector's trigger is. It fits inside [`HEADER`] with room above and below.
pub const MODEL_SELECT_HEIGHT: f32 = 24.0;
/// The most the model selector's trigger may take across the header.
const MODEL_SELECT_WIDEST: f32 = 180.0;

/// What the drawing reported, applied by [`pane`] once everything has been drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Send,
    Stop,
    New,
    Open(String),
    Remove(String),
    Choose(String),
    /// A picture dropped on the pane.
    Dropped(std::path::PathBuf),
    /// Ctrl/Cmd+V in the composer: ask the window for whatever picture is on the clipboard.
    Paste,
    Detach(u64),
    Copy(String),
    ShowHistory(bool),
    /// Open or close one tool block, by its call id.
    ToggleTool(String),
    /// Open or close a run of tool calls, by `message::run_key`.
    ToggleGroup(String),
    /// Open or close one message's thinking.
    ToggleThinking(u64),
    /// Select the whole of one message's words, which is what the menu's `Select All` means.
    SelectAll(u64),
    /// A component's button asked for these words to be sent as the person's next message.
    SendWords(String),
    /// A component's button asked for these words to be put in the composer.
    Fill(String),
    /// A component asked for a project file to be opened, at a line when it named one.
    OpenFile(String, Option<u32>),
}

/// Draw the pane, and act on what was pressed.
pub fn pane(chat: &mut AgentChat, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Request> {
    let area = ui.available_rect_before_wrap();
    let mut acts = Vec::new();
    if area.width() > 40.0 && area.height() > 40.0 {
        acts = surface(chat.parts(), ui, look, area);
    }
    // The pointer's own answer to a picture dropped on the pane. `egui` collects what the window
    // manager handed over, and a path that is a picture is attached exactly as the `+` would attach
    // it — which is the third way in the ticket's own list, after the button and the clipboard.
    for path in dropped_pictures(ui, area) {
        acts.push(Act::Dropped(path));
    }
    // **A paste is claimed before the pane loop reads it**, which is the one-frame ordering the
    // Markdown preview's own copy already uses. What `egui` reports of Ctrl/Cmd+V when the clipboard
    // holds a picture rather than text is nothing at all, so the chord is read off the key going back
    // up and the window is asked for the picture - see `pasting`.
    if pasting(ui, chat.parts().state.prompt_focused) {
        acts.push(Act::Paste);
    }
    // **A selection is about the message it was made in, and a press somewhere else ends it.**
    // Without this, words selected in an answer would go on being what `Ctrl`/`Cmd`+`C` copied for
    // the rest of the session, including from the editing area beside the pane. Not while the right
    // click menu is open, because the press that chooses one of its rows is a press outside the pane
    // and would throw away the very selection the row is about.
    if chat.ui.menu.is_none() && a_press_landed_outside(ui, area) {
        chat.ui.selection = None;
    }
    if let Some(text) = copying(ui, chat) {
        acts.push(Act::Copy(text));
    }
    apply(chat, acts)
}

/// Whether the primary button went down this frame somewhere that is not this pane.
///
/// The pointer through [`controls::pointer_in`], because a chat node on the canvas is drawn into a
/// layer carrying the camera and the frame's raw position is the window's — the two agree only while
/// the camera sits at one on the origin, which is where a test leaves it and where nobody leaves a
/// canvas. `task-2003` wrote that down for the board's own wheel.
fn a_press_landed_outside(ui: &egui::Ui, area: Rect) -> bool {
    let pressed = ui.ctx().input(|input| input.pointer.primary_pressed());
    pressed && controls::pointer_in(ui).is_none_or(|at| !area.contains(at))
}

/// What `Ctrl`/`Cmd`+`C` should copy out of this pane, taking the event so nothing else copies too.
///
/// `task-2060`: *"I should be able to select & copy text in a message. e.g. select/highlight a sub
/// section, then right click and see copy option, or press Ctrl/CMD+C."* egui delivers a copy as an
/// `egui::Event::Copy` rather than as a key press — which is why `Copy` is marked in `actions::menus`
/// as not coming from the keyboard — so the event is what has to be claimed, and removing it from
/// the frame's input is what stops the editing area copying its own selection a moment later. That
/// is `UnluminousApp::route_the_preview_copy`'s rule, kept here.
///
/// **Not while a text box has the keyboard.** The composer is a text box in this same pane, and a
/// copy made while the caret is in it is that box's own.
fn copying(ui: &egui::Ui, chat: &mut AgentChat) -> Option<String> {
    if crate::app::text_box_has_the_keyboard(ui.ctx()) {
        return None;
    }
    let text = chat.selected_text()?;
    let took = ui.ctx().input_mut(|input| {
        let before = input.events.len();
        input.events.retain(|event| !matches!(event, egui::Event::Copy));
        before != input.events.len()
    });
    took.then_some(text)
}

/// Everything inside the pane, from a borrow of the conversation.
fn surface(mut parts: Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let panel = area.shrink(PAD * scale);
    let radius = RADIUS * scale;
    // **One raised card holds the whole chat**, which is `ChatPanel.module.css`'s `.panel`. With the
    // decoration off it is the flat bordered panel every list in Unluminous draws, so switching the
    // renderer off in the manifest or in `plugins.chrome` really withdraws the depth.
    if look.chrome.is_recording() {
        look.chrome.raised(panel, radius, Fill::Solid(look.palette.board_lane), Lift::Small);
    } else {
        ui.painter().rect(
            panel,
            CornerRadius::same(radius as u8),
            look.ground(look.palette.board_lane),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
    }
    let inner = panel.shrink(INNER * scale);
    if inner.width() < 40.0 {
        return acts;
    }

    let header_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HEADER * scale));
    acts.extend(header(&mut parts, ui, look, header_rect));

    let composer_height = composer::height(&parts, look, inner.width());
    let composer_rect = Rect::from_min_size(
        Pos2::new(inner.left(), inner.bottom() - composer_height),
        Vec2::new(inner.width(), composer_height),
    );
    // The conversation is given back most of the card's side padding — see [`LIST_INSET`]. The header and
    // the composer keep `inner`, because a control wants room round it and a paragraph does not.
    let side = (INNER - LIST_INSET) * scale;
    let body = Rect::from_min_max(
        Pos2::new(inner.left() - side, header_rect.bottom() + 4.0 * scale),
        Pos2::new(inner.right() + side, composer_rect.top() - 4.0 * scale),
    );
    // Forgotten before it is drawn again, so a pane too short to hold a list does not go on answering a
    // wheel with where one used to be. See `PaneState::list_rect`.
    parts.state.list_rect = None;
    if body.height() > 20.0 {
        // The two lists are drawn **over** the conversation rather than in a popup, because egui keeps
        // at most one popup open at a time — the rule that already shaped the flyouts, the colour wheel
        // and the completion list — and a pane that could not open its own history while a menu was up
        // would be a pane whose history is unreachable at the moment somebody wants it.
        if parts.state.history_open {
            acts.extend(history_list(&mut parts, ui, look, body));
        } else {
            acts.extend(conversation(&mut parts, ui, look, body));
        }
    }
    // Drawn after the conversation and before the composer takes `parts`. A popup is a layer of its
    // own, so where it sits in this order decides nothing about what it is drawn over — what it
    // decides is that the rows are read on the frame after the right click that opened it, which is
    // `controls::field_menu`'s own shape.
    acts.extend(message_menu(&mut parts, ui, look));
    acts.extend(composer::show(parts, ui, look, composer_rect));
    acts
}

/// How wide a message's right click menu is. Three short rows, so it is a field menu's width.
const MENU_WIDTH: f32 = 200.0;

/// The right click menu over a message: copy the selection, copy the message, select all of it.
///
/// `task-2060` asks for the first of those in as many words. The other two are what a menu opened on
/// a block of text has to offer beside it: `Copy` is dimmed with nothing selected, which is the style
/// guide's distinction — a control that will apply the moment something is selected is dimmed, and
/// one that can never apply is absent.
///
/// **What `Copy` copies is the words as they are drawn**, out of the rendered markdown, so a heading
/// comes back without its hashes. `Copy Message` is the source, which is what the button beside the
/// bubble has always copied and what somebody pasting an answer back into a file wants.
fn message_menu(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>) -> Vec<Act> {
    let Some(menu) = parts.state.menu else {
        return Vec::new();
    };
    let mut acts = Vec::new();
    let _ = look;
    let key = format!("message-{}", menu.message);
    let selected = parts
        .state
        .selection
        .filter(|one| one.message == menu.message && !one.range.is_empty())
        .and_then(|one| parts.state.rendered.slice(&key, one.range.range()));
    // The message's own source, whether it is in the conversation or still waiting in the queue.
    let source = parts
        .session
        .chat
        .message(menu.message)
        .or_else(|| parts.queued.iter().find(|one| one.id == menu.message))
        .map(unluminous_chat::Message::text);
    let mut close = false;
    let popup = egui::Popup::new(
        egui::Id::new("agent-chat-message-menu"),
        ui.ctx().clone(),
        menu.at,
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .frame(
        egui::Frame::popup(ui.style())
            .fill(crate::theme::color::menu())
            .stroke(Stroke::new(1.0, crate::theme::color::control_border()))
            .inner_margin(6),
    )
    .width(MENU_WIDTH);
    if let Some(response) = popup.show(|ui| {
        if controls::menu_row(ui, "Copy", "", selected.is_some(), false, 0.0) {
            if let Some(text) = selected.clone() {
                acts.push(Act::Copy(text));
            }
            close = true;
        }
        if controls::menu_row(ui, "Copy Message", "", source.is_some(), false, 0.0) {
            if let Some(text) = source.clone() {
                acts.push(Act::Copy(text));
            }
            close = true;
        }
        if controls::menu_row(ui, "Select All", "", true, false, 0.0) {
            acts.push(Act::SelectAll(menu.message));
            close = true;
        }
    }) {
        close |= response.response.should_close();
    }
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if close {
        parts.state.menu = None;
    }
    acts
}

/// The header: the conversation's name, the model selector, history and new.
///
/// **Everything in it is sized by the pane's zoom**, `task-2200`: zoomed in, the title grew while the
/// select, its words and the two buttons' marks stayed the size they are at one. The select takes the zoom
/// through `rux::components::Select::zoom` and the buttons through `controls::icon_button_at`.
///
/// **There is no state dot.** `task-2200`: *"I don't want the dot at all. Just have the title"*. What the
/// dot said is still said by the stop disc in the composer while an answer arrives and by the failure row
/// when one fails.
fn header(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let painter = ui.painter_at(area);
    let middle = area.center().y;

    // The two buttons first, from the right, then the select to their left, so the title knows how much
    // room is left for it.
    // Two round keys the height of the model selector, so the three controls share one middle and one
    // height and read as one row of instrument keys (`task-2219`).
    let button = MODEL_SELECT_HEIGHT * scale;
    let new = Rect::from_center_size(
        Pos2::new(area.right() - 2.0 * scale - button / 2.0, middle),
        Vec2::splat(button),
    );
    let history = Rect::from_center_size(
        Pos2::new(new.center().x - button - 8.0 * scale, middle),
        Vec2::splat(button),
    );
    let names: Vec<String> =
        parts.configuration.providers.iter().map(|one| one.name.clone()).collect();
    let chip_width = model_select_width(&painter, &names, scale);
    let chip_height = MODEL_SELECT_HEIGHT * scale;
    let chip = Rect::from_min_size(
        Pos2::new(history.left() - 8.0 * scale - chip_width, middle - chip_height / 2.0),
        Vec2::new(chip_width, chip_height),
    );

    // **One line, cut with an ellipsis before it reaches the select**, because a conversation named after
    // a long first sentence has to lose its end rather than run under the select or gain a second line.
    let font = egui::FontId::proportional(look.font_size * 0.82);
    let left = area.left() + 2.0 * scale;
    let right = match names.is_empty() {
        true => history.left(),
        false => chip.left(),
    } - 10.0 * scale;
    let room = (right - left).max(1.0);
    let mut job = egui::text::LayoutJob::single_section(
        parts.session.chat.display_name().to_owned(),
        egui::TextFormat {
            font_id: font.clone(),
            color: look.palette.text_strong,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: room,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let title = painter.crisp_layout_job(job);
    // Placed by its capitals, so it shares a middle with the select's words and the buttons' marks.
    let top = crate::theme::crisp::top_centring_capitals(&painter, &font, middle);
    painter
        .with_clip_rect(Rect::from_min_max(
            Pos2::new(left, area.top()),
            Pos2::new(right, area.bottom()),
        ))
        .crisp_galley(Pos2::new(left, top), title, look.palette.text_strong);

    // **The model selector is `rux`'s `Select`**, the dropdown from Black Rainbow Labs' component
    // library. `task-2096` asks for a dropdown menu from that library rather than a new one: the chip
    // this replaces opened a list drawn over the whole conversation. The menu opens under the trigger on
    // a foreground layer of its own, and a press anywhere else closes it. Its menu takes the theme's
    // smallest raised shadow rather than the reference's largest, which spread a dark blur over the
    // messages under it (`task-2200`).
    let chosen = parts
        .configuration
        .provider()
        .and_then(|chosen| names.iter().position(|name| *name == chosen.name));
    if !names.is_empty() && chip.left() > left {
        let select = parts.state.model_select.get_or_insert_with(ModelSelect::new);
        let id = ui.id().with("agent-chat-model-select");
        // The layer is the trigger and room round it for its shadow. The menu opens a layer of its own.
        crate::theme::in_step(&select.rux);
        let outcome = rux::layer(ui, &select.rux, id, chip.expand(24.0 * scale), |rux| {
            let quiet = rux.theme().elevation.raised_sm;
            rux::components::Select::new(&names, chosen)
                .label("Model")
                .placeholder("No endpoint")
                .zoom(scale)
                .menu_elevation(quiet)
                .show(rux, chip, &mut select.menu)
        });
        select.rux.end_frame();
        if let Some(name) = outcome.chosen.and_then(|index| names.get(index)) {
            acts.push(Act::Choose(name.clone()));
        }
    }

    // **Two round raised keys**, `task-2219`, where there were two ghost buttons with no surface until the
    // pointer was on them: beside a raised model selector they read as two loose marks. The history key
    // stays pressed in, with its mark in the accent, while the history is open.
    let open = parts.state.history_open;
    if header_key(ui, look, history, "Conversations", icon::clock, open) {
        acts.push(Act::ShowHistory(!open));
    }
    if header_key(ui, look, new, "New Conversation", icon::plus, false) {
        acts.push(Act::New);
    }

    // The hairline under it, which is the reference's `border-bottom`.
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(area.left(), area.bottom() - 1.0),
            Pos2::new(area.right(), area.bottom()),
        ),
        0,
        look.palette.divider,
    );
    acts
}

/// How wide the model selector's trigger is: the widest endpoint name, plus what the trigger and the
/// menu under it each take round the words, whichever is more, all at the pane's zoom.
///
/// The widest rather than the chosen one, so the trigger stays the same width when a different endpoint
/// is chosen and nothing beside it moves. The words are measured at the size `rux::Style::CONTROL`
/// sets them in, times the zoom, which is what `Select::zoom` draws them at.
fn model_select_width(painter: &egui::Painter, names: &[String], scale: f32) -> f32 {
    let style = rux::Style::CONTROL;
    let style = style.at(style.size * scale);
    let widest =
        names.iter().map(|name| rux::text::measure(painter, style, name).x).fold(0.0_f32, f32::max);
    // **Wide enough for the menu's rows, not only for the trigger's words.** `rux` draws the menu
    // exactly as wide as its trigger, and a row gives its words that width less six points each side
    // of the menu, twelve each side of the row and eighteen for the tick. Sized for the trigger alone,
    // which needs only `padding: 9px 12px`, an eight point gap and the thirteen point chevron, every
    // row was cut to three letters: `cla…`, `cod…`, `loc…`, seen on the installed 0.56.0.
    let trigger = widest + (12.0 * 2.0 + 8.0 + 13.0) * scale;
    let row = widest + (6.0 * 2.0 + 12.0 * 2.0 + 18.0 + 4.0) * scale;
    trigger.max(row).min(MODEL_SELECT_WIDEST * scale)
}

/// The conversation: every message, scrolled, with the empty state when there is nothing.
fn conversation(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    // Copied out of the parts so the conversation can be read while the little the drawing remembers
    // is written into. Both are borrows of different fields, which is what makes this legal at all.
    let session: &unluminous_chat::Session = parts.session;
    // Asked for once and then cleared, which is what `reveal_caret` does: a jump that ran on every
    // frame would make the conversation impossible to scroll at all.
    let jump = std::mem::take(&mut parts.state.jump_to_bottom);
    // Where the list a wheel belongs to is, for the reason `PaneState::list_rect` gives.
    parts.state.list_rect = Some(area);
    let wheel = parts.state.wheel.take();
    if session.chat.messages.is_empty() && parts.queued.is_empty() {
        parts.state.scrolled = 0.0;
        parts.state.scrollable = 0.0;
        return welcome::show(parts, ui, look, area);
    }
    let mut acts = Vec::new();
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area));
    // Intersected rather than replaced, so a chat node scrolled half off the canvas does not draw its
    // transcript over whatever is beside the pane. See `agent_tasks::lanes::show`.
    body.set_clip_rect(area.intersect(ui.clip_rect()));
    // **The decoration is cut to the conversation, and it has to be.** A `Chrome` records absolute
    // rectangles into one canvas that covers the whole pane, so a bubble scrolled half out of view
    // recorded its whole surface and the canvas painted it over the header above. `egui`'s own clip
    // rectangle cannot reach the canvas; `Decor::Clip` is the one thing that can. Measured on a real
    // window: a message scrolled off the top was drawn across the pane's own name.
    //
    // **Cut above and below, not at the sides** (`task-2200`). The rows sit [`LIST_INSET`] from the card's
    // edge, and a bubble from the person is raised at `Lift::Medium`, whose shadow reaches well past two
    // points: cut at the list's own sides, the shadow down its right edge stopped in a hard vertical line.
    // A shadow spilling sideways onto the card's padding is what it would do on the reference page.
    let reach = crate::services::vello_canvas::Lift::Medium.reach() * look.scale();
    look.chrome.clip(area.expand2(Vec2::new(reach, 0.0)), 0.0);
    let mut scroller = egui::ScrollArea::vertical()
        .id_salt("agent-chat-conversation")
        // **Stuck to the bottom while an answer is arriving, and unstuck the moment somebody scrolls
        // up** — which is `ChatPage.tsx`'s own `shouldAutoScroll` rule. egui's own stickiness does
        // exactly that: it follows while the view is already at the bottom and stops when it is not.
        .stick_to_bottom(true)
        // Nothing is done about dragging the contents to scroll them, and that is deliberate:
        // `ScrollSource::drag` is `OnTouch` by default, so a drag with a pointer already selects
        // words rather than scrolling — which is what `task-2060` asks for — and forcing it off
        // would take scrolling away from a touch screen to fix something that is not broken there.
        .auto_shrink([false, false]);
    if jump {
        // What stickiness will not do is go *back* to the bottom once somebody has scrolled away, and
        // sending, opening a conversation and starting a new one all have to.
        //
        // **A finite offset, not `f32::MAX`.** `task-1848` reported pressing Enter scrolling the
        // conversation to the *top*. `vertical_scroll_offset` is written straight into `state.offset.y`
        // before the pass and is only clamped against `max_offset` at the end of it, so for the whole of
        // the pass every piece of arithmetic that reads the offset — the bar's handle, the fade areas,
        // `paint_fade_areas_impl` — is working with 3.4e38. What it is asked for now is past the bottom of
        // the conversation as it was last frame and therefore lands at the bottom once clamped, while
        // being an ordinary number all the way through.
        //
        // `scrolled` is where the conversation was left, measured at the end of the previous frame, and
        // one pane's height past it is further than any single message can have added.
        scroller = scroller.vertical_scroll_offset(parts.state.scrolled.max(0.0) + area.height());
    } else if let Some(offset) = parts.state.scroll_to.take() {
        // A zoom moved everything, so the point that was under the pointer is put back under it. The same
        // one-shot shape, worked out by `AgentChat::zoomed`. `task-1771`.
        scroller = scroller.vertical_scroll_offset(offset.max(0.0));
    }
    let scrolled = scroller.show(&mut body, |ui| {
        // The wheel the window read over a canvas node, handed to `egui` the way a wheel it read itself
        // would have been. See `PaneState::wheel` for why it is a delta and not an offset, and
        // `ScrollAnimation::none` because a wheel moves the page now rather than gliding to it.
        if let Some(wheel) = wheel {
            ui.scroll_with_delta_animation(
                Vec2::new(0.0, wheel),
                egui::style::ScrollAnimation::none(),
            );
        }
        let width = area.width();
        // The conversation, and then whatever was sent while this answer was arriving. A queued
        // question is **not** in the conversation — see `AgentChat::queued` for why — so it is drawn
        // after it, in the order it was sent, which is where it will be asked.
        let queued = parts.queued.iter().map(|one| (one, true));
        let messages = session.chat.messages.iter().map(|one| (one, false)).chain(queued);
        for row in rows_of(messages) {
            match row {
                Row::Said(one, waiting) => {
                    // Worked out once and handed to the drawing, because the height has to be known
                    // before the rectangle can be allocated and running it twice built the message's
                    // text twice. See `message::Shape`.
                    let shape =
                        message::shape(one, parts.state, look, width, waiting, false, Some(ui));
                    if shape.height <= 0.0 {
                        continue;
                    }
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(width, shape.height), egui::Sense::hover());
                    // **Only what can be seen is drawn**, which is `task-1666`'s rule and, here, also
                    // what keeps the decoration's canvas the size of the pane: a bubble scrolled a
                    // thousand points away would otherwise record shadows a thousand points outside it.
                    if rect.intersects(ui.clip_rect()) {
                        acts.extend(message::show(one, shape, parts.state, ui, look, rect));
                    }
                }
                Row::Tools(tools) => {
                    let height = message::run_height(&tools, parts.state, look, width);
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
                    if rect.intersects(ui.clip_rect()) {
                        acts.extend(message::run_show(&tools, parts.state, ui, look, rect));
                    }
                }
            }
            ui.add_space(GAP * look.scale());
        }
    });
    // The canvases of components that were not drawn this frame are given back. Once a frame, after
    // every row has had its turn, which is what `rux::RuxState::end_frame` asks for.
    if let Some(kept) = &parts.state.blocks_rux {
        kept.end_frame();
    }
    // Where the conversation was left, so a zoom can put it back where it was. See `PaneState::scrolled`.
    parts.state.scrolled = scrolled.state.offset.y;
    // And how far it could be scrolled, so `plugins view agent-chat` can answer whether it is at the
    // bottom. Never negative: a conversation shorter than the pane has nowhere to go.
    parts.state.scrollable = (scrolled.content_size.y - area.height()).max(0.0);
    look.chrome.unclip();
    acts
}

/// One row of the conversation: something said, or a run of tool calls between two things said.
#[derive(Debug)]
enum Row<'a> {
    /// A message, drawn without its tool calls, and whether it is a question waiting its turn.
    Said(&'a unluminous_chat::Message, bool),
    Tools(Vec<&'a unluminous_chat::model::ToolCall>),
}

/// The conversation as the rows it is drawn in.
///
/// `task-2193`: the tool calls an answer made are gathered into one run per gap between two things
/// said, however many rounds they took — a model that calls three tools, says nothing, calls four more
/// and then answers made seven blocks in three messages, and that is one run of seven. A message with
/// nothing in it but its calls is not a row of its own. A tool result is the copy that goes back up the
/// wire and is drawn inside the call it answers, so it is never a row.
fn rows_of<'a>(
    messages: impl Iterator<Item = (&'a unluminous_chat::Message, bool)>,
) -> Vec<Row<'a>> {
    use unluminous_chat::Role;
    let mut rows = Vec::new();
    let mut run: Vec<&unluminous_chat::model::ToolCall> = Vec::new();
    let flush = |run: &mut Vec<&'a unluminous_chat::model::ToolCall>, rows: &mut Vec<Row<'a>>| {
        if !run.is_empty() {
            rows.push(Row::Tools(std::mem::take(run)));
        }
    };
    for (one, waiting) in messages {
        if one.role == Role::Tool {
            continue;
        }
        let says_something = waiting
            || one.role != Role::Assistant
            || !one.text().trim().is_empty()
            || !one.thinking.is_empty()
            || one.failure.is_some()
            || one
                .parts
                .iter()
                .any(|part| matches!(part, unluminous_chat::model::Part::Picture { .. }));
        if says_something {
            flush(&mut run, &mut rows);
            rows.push(Row::Said(one, waiting));
        }
        run.extend(one.tools.iter());
    }
    flush(&mut run, &mut rows);
    rows
}

/// The conversations kept, drawn over the conversation area.
/// A round key in the header: raised, or pressed in while what it opens is open.
///
/// The hover changes the mark and not the decoration, so moving the pointer across the header rasterises
/// nothing, which is the rule `services::vello_canvas` records about a hover.
fn header_key(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    name: &str,
    draw: fn(&egui::Painter, Pos2, egui::Color32),
    down: bool,
) -> bool {
    let scale = look.scale();
    let response = ui.interact(rect, ui.id().with(("agent-chat-key", name)), egui::Sense::click());
    let response = controls::WithHint::with_hint(response, name);
    let radius = rect.height() / 2.0;
    if look.chrome.is_recording() {
        match down {
            true => look.chrome.sunken(rect, radius, look.palette.board_well, Lift::Small),
            false => {
                look.chrome.raised(rect, radius, Fill::Solid(look.palette.board_card), Lift::Small)
            }
        }
    } else {
        ui.painter().circle(
            rect.center(),
            radius,
            look.ground(look.palette.board_card),
            Stroke::new(1.0, look.palette.control_border),
        );
    }
    let held = response.is_pointer_button_down_on();
    let tint = match (down, response.hovered() || held) {
        (true, _) => look.palette.accent,
        (false, true) => look.palette.text_strong,
        (false, false) => look.palette.text_dim,
    };
    let nudge = if held { Vec2::new(0.0, 0.5 * scale) } else { Vec2::ZERO };
    icon::scaled(ui.painter(), rect.center() + nudge, tint, scale * 0.95, draw);
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, down, name));
    response.clicked()
}

fn history_list(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let session: &unluminous_chat::Session = parts.session;
    let history = parts.history;
    let mut acts = Vec::new();
    let scale = look.scale();
    parts.state.list_rect = Some(area);
    let put = parts.state.scroll_to.take();
    let wheel = parts.state.wheel.take();
    let painter = ui.painter_at(area);
    if history.is_empty() {
        controls::centred_line(
            &painter,
            area,
            area.top() + 20.0 * scale,
            "No conversations yet.",
            look.font_size * 0.85,
            look.palette.text_dim,
        );
    }
    // **Two lines a row**, `task-2219`: the name, and under it the agent, how many messages and how long
    // ago, which is what tells two conversations with similar names apart. The chosen row is a card, a
    // hovered row is a wash, and the cross that removes a conversation is drawn only on the row under the
    // pointer, because a column of crosses is a column of invitations to delete something.
    let title_size = look.font_size * 0.85;
    let detail_size = look.font_size * 0.72;
    let row = (title_size * 1.3 + detail_size * 1.35 + 16.0 * scale).max(look.row_height * scale);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let mut body = ui.new_child(egui::UiBuilder::new().max_rect(area));
    body.set_clip_rect(area.intersect(ui.clip_rect()));
    let mut scroller =
        egui::ScrollArea::vertical().id_salt("agent-chat-history").auto_shrink([false, false]);
    if let Some(offset) = put {
        scroller = scroller.vertical_scroll_offset(offset.max(0.0));
    }
    let scrolled = scroller.show(&mut body, |ui| {
        if let Some(wheel) = wheel {
            ui.scroll_with_delta_animation(
                Vec2::new(0.0, wheel),
                egui::style::ScrollAnimation::none(),
            );
        }
        ui.add_space(4.0 * scale);
        for one in history {
            let (slot, _) = ui.allocate_exact_size(
                Vec2::new(area.width(), row + 4.0 * scale),
                egui::Sense::hover(),
            );
            if !slot.intersects(ui.clip_rect()) {
                continue;
            }
            let rect = Rect::from_min_size(slot.min, Vec2::new(slot.width(), row))
                .shrink2(Vec2::new(4.0 * scale, 0.0));
            let chosen = one.id == session.chat.id;
            let response = ui.interact(
                rect,
                ui.id().with(("agent-chat-history", &one.id)),
                egui::Sense::click(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    format!("Conversation: {}", one.name),
                )
            });
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let painter = ui.painter_at(rect.expand(1.0));
            let corner = CornerRadius::same((8.0 * scale).round().clamp(0.0, 255.0) as u8);
            if chosen {
                painter.rect(
                    rect,
                    corner,
                    look.ground(look.palette.board_card),
                    Stroke::new(1.0, look.palette.divider),
                    egui::StrokeKind::Inside,
                );
                // A short bar of the accent against the card's left edge says which one is open.
                let bar = Rect::from_center_size(
                    Pos2::new(rect.left() + 1.5 * scale, rect.center().y),
                    Vec2::new(3.0 * scale, rect.height() * 0.46),
                );
                painter.rect_filled(bar, CornerRadius::same(2), look.palette.accent);
            } else if response.hovered() {
                painter.rect_filled(
                    rect,
                    corner,
                    crate::theme::color::hover_wash().gamma_multiply(0.05),
                );
            }
            let hovered = response.hovered()
                || ui.rect_contains_pointer(rect) && ui.ctx().pointer_hover_pos().is_some();
            let cross = Rect::from_center_size(
                Pos2::new(rect.right() - 14.0 * scale, rect.center().y),
                Vec2::splat(20.0 * scale),
            );
            let text_right = match hovered {
                true => cross.left() - 4.0 * scale,
                false => rect.right() - 10.0 * scale,
            };
            let left = rect.left() + 12.0 * scale;
            let clipped = painter
                .with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(text_right, rect.max.y)));
            let block = title_size * 1.3 + detail_size * 1.35;
            let top = rect.center().y - block / 2.0;
            clipped.text(
                Pos2::new(left, top),
                egui::Align2::LEFT_TOP,
                &one.name,
                egui::FontId::proportional(title_size),
                match chosen {
                    true => look.palette.text_strong,
                    false => look.palette.text,
                },
            );
            let messages = match one.messages {
                1 => "1 message".to_owned(),
                count => format!("{count} messages"),
            };
            let detail = format!(
                "{}  \u{00B7}  {}  \u{00B7}  {}",
                one.provider,
                messages,
                welcome::ago(one.changed, now)
            );
            clipped.text(
                Pos2::new(left, top + title_size * 1.3),
                egui::Align2::LEFT_TOP,
                detail,
                egui::FontId::proportional(detail_size),
                look.palette.text_faint,
            );
            if response.clicked() {
                acts.push(Act::Open(one.id.clone()));
            }
            if hovered
                && crate::components::controls::icon_button_at(
                    ui,
                    cross,
                    &format!("Remove conversation: {}", one.name),
                    icon::cross,
                    scale,
                )
            {
                acts.push(Act::Remove(one.id.clone()));
            }
        }
    });
    // Read back where the list ended up and how far it can go, which is the pair the wheel over a canvas
    // node is measured from. See `PaneState::scrolled`.
    parts.state.scrolled = scrolled.state.offset.y;
    parts.state.scrollable = (scrolled.content_size.y - area.height()).max(0.0);
    acts
}

/// Do what the drawing reported, and answer what the window has to do.
fn apply(chat: &mut AgentChat, acts: Vec<Act>) -> Vec<Request> {
    let mut requests = Vec::new();
    for act in acts {
        match act {
            Act::Send => {
                // **A notice rather than the status bar.** `task-1848` reported this as "nothing happens,
                // no error": every reason `send` refuses — nothing typed, no endpoint configured, a
                // program that is not installed, a key that is not set — went to the status bar, which is
                // a sentence in the smallest text at the far bottom edge of the window, replaced by
                // whatever was reported next. It is the one control on this pane and the person is looking
                // straight at it.
                if let Err(problem) = chat.send() {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Stop => chat.stop(),
            Act::New => {
                chat.new_conversation();
                chat.ui.history_open = false;
            }
            Act::Dropped(path) => {
                // Same reason as `Act::Send`: a picture that could not be attached is a thing somebody
                // just did, and they are watching the pane rather than the status bar.
                if let Err(problem) = chat.attach(&path) {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Paste => requests.push(Request::ClipboardPicture {
                id: crate::services::agent_chat::CLIPBOARD.to_owned(),
            }),
            Act::Open(id) => {
                if let Err(problem) = chat.open_conversation(&id) {
                    requests.push(Request::Message(problem));
                }
                chat.ui.history_open = false;
            }
            Act::Remove(id) => {
                if let Err(problem) = chat.remove_conversation(&id) {
                    requests.push(Request::Message(problem));
                }
            }
            Act::Choose(name) => {
                if let Err(problem) = chat.configuration_mut().choose(&name) {
                    requests.push(Request::Message(problem));
                }
                if let Err(problem) = chat.save_the_configuration() {
                    requests.push(Request::Message(problem));
                }
            }
            Act::Detach(id) => chat.remove_attachment(id),
            Act::Copy(text) if !text.is_empty() => requests.push(Request::Copy(text)),
            Act::Copy(_) => {}
            Act::ShowHistory(open) => {
                chat.ui.history_open = open;
            }
            Act::ToggleTool(id) => match chat.ui.opened_tools.iter().position(|one| *one == id) {
                Some(at) => {
                    chat.ui.opened_tools.remove(at);
                }
                None => chat.ui.opened_tools.push(id),
            },
            Act::ToggleGroup(key) => {
                if !chat.ui.opened_groups.remove(&key) {
                    chat.ui.opened_groups.insert(key);
                }
            }
            Act::SelectAll(id) => chat.select_the_whole_message(id),
            Act::SendWords(words) => {
                // Through the same path as typing and pressing send, so it queues behind an answer
                // that is still arriving and refuses with a notice when nothing can answer it.
                if let Err(problem) = chat.send_words(&words) {
                    requests.push(Request::Notice {
                        text: problem,
                        kind: crate::components::toast::Kind::Problem,
                    });
                }
            }
            Act::Fill(words) => {
                chat.draft = words;
                chat.ui.focus_the_prompt = true;
            }
            Act::OpenFile(path, line) => match chat.open_a_file(&path, line) {
                Ok(asked) => requests.extend(asked),
                Err(problem) => requests.push(Request::Notice {
                    text: problem,
                    kind: crate::components::toast::Kind::Problem,
                }),
            },
            Act::ToggleThinking(id) => {
                match chat.ui.opened_thinking.iter().position(|one| *one == id) {
                    Some(at) => {
                        chat.ui.opened_thinking.remove(at);
                    }
                    None => chat.ui.opened_thinking.push(id),
                }
            }
        }
    }
    requests
}

/// Whether the composer was pasted into this frame.
///
/// Only while a field in this pane has the keyboard, so a paste meant for the editing area is not
/// taken.
///
/// **It is the key going back up that says so, and it took reading `egui-winit` to find out why.**
/// This used to watch for an *empty* `egui::Event::Paste`, on the reasoning that a picture is not text
/// and would therefore arrive as a paste with nothing in it. `egui-winit` does not do that. Its
/// `on_keyboard_input` recognises the paste chord, reads the clipboard's **text**, and pushes
/// `Event::Paste` only `if !contents.is_empty()` — then returns, so the `V` key press is swallowed as
/// well. With a picture on the clipboard `get_text()` fails outright and the frame carries **no event
/// at all**. So the condition that was being watched for could never once have happened, and pasting a
/// picture into this pane has never worked. `task-1771`.
///
/// The `return` is only taken while the key is **down**. The release comes back through the ordinary
/// path as `Event::Key { key: V, pressed: false, modifiers }`, with the modifier still held, which is
/// the one report of the chord that reaches Unluminous. Asking the window for a picture on the way up is a
/// keystroke late and nobody can tell; a clipboard with no picture on it answers `None` and nothing
/// happens, which is what makes it safe to ask after an ordinary text paste as well.
fn pasting(ui: &egui::Ui, in_the_composer: bool) -> bool {
    // **This pane's own field, not any field.** `text_box_has_the_keyboard` answers whether *some* box in
    // the window has the keys, and with this pane showing beside a focused explorer filter that was enough
    // to attach the clipboard's picture to a conversation nobody was typing into. The composer says whether
    // it has them — see `agent_chat::PaneState::prompt_focused`.
    if !in_the_composer {
        return false;
    }
    ui.ctx().input(|input| is_a_paste_chord(&input.events))
}

/// Whether this frame's events carry the paste chord as Unluminous can actually see it.
///
/// Its own function so a test can hold the two event shapes side by side: the one that was being
/// watched for and never arrives, and the one that does.
fn is_a_paste_chord(events: &[egui::Event]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            egui::Event::Key { key: egui::Key::V, pressed: false, modifiers, .. }
                if modifiers.command
        )
    })
}

/// Whether a file the window manager dropped belongs to the pane at `area`.
///
/// `None` means the platform reported no pointer through the drag, which Windows does not: a file is
/// carried over a window through OLE and no cursor movement is sent at all, so `egui` can still be
/// holding a position from before the drag started. The system is asked first, and a drop that still
/// cannot be placed belongs here rather than nowhere.
fn belongs_here(pointer: Option<Pos2>, area: Rect) -> bool {
    pointer.is_none_or(|at| area.contains(at))
}

/// Every picture the window manager dropped on this pane this frame.
///
/// `egui` collects what was dropped and hands over a path; a file that is not a picture is left
/// alone rather than refused, because a drag that landed on the wrong pane should do nothing rather
/// than say something.
///
/// **`area` is the pane's own rectangle, taken before anything was drawn in it.** It used to ask the `Ui`
/// for what was left, which is a different rectangle by the time the composer has been laid out.
///
/// **A drop with no pointer is this pane's.** Windows carries a file over a window through OLE, which
/// reports no cursor movement at all, so `egui` can be holding a position from before the drag began — and
/// gating on it silently threw the picture away. The folder pane reads `dropped_files` too since
/// `task-2194`, which is why the system is asked where the pointer is before this falls back.
///
/// **`hover_pos` alone, and not `latest_pos` behind it.** The zoom asks for both because a wheel event
/// arrives with no pointer at all and the *last place it was seen* is still the honest answer. A drop is the
/// opposite case: `latest_pos` during an OLE drag is a position from before the drag began, so falling back
/// to it is falling back to a stale answer that reads as a confident one — which threw the picture away
/// exactly where this was meant to stop it. Found by the `task-1771` review.
fn dropped_pictures(ui: &egui::Ui, area: Rect) -> Vec<std::path::PathBuf> {
    // **Asked of the system first** (`task-2194`), which knows where the pointer is during a drop when
    // egui does not. Since the folder pane takes dropped files as well, a drop nobody could place would
    // otherwise be attached here and copied into a folder there at once.
    let pointer = crate::services::system_files::pointer(ui.ctx())
        .or_else(|| ui.ctx().input(|input| input.pointer.hover_pos()));
    if !belongs_here(pointer, area) {
        return Vec::new();
    }
    ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|dropped| dropped.path().to_path_buf())
            .filter(|path| {
                matches!(
                    path.extension()
                        .and_then(|kind| kind.to_str())
                        .map(str::to_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "gif" | "webp")
                )
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2193`: the calls between two things said are one run, however many rounds they took, and a
    /// message that is nothing but calls is not a row of its own.
    #[test]
    fn the_tool_calls_between_two_things_said_are_one_run() {
        use unluminous_chat::model::{Message, ToolCall};
        use unluminous_chat::Role;
        let question = Message::said(1, Role::User, "Make three tickets");
        let mut first = Message::said(2, Role::Assistant, "Reading the board first.");
        first.tools.push(ToolCall::new("a", "unluminous_plugins", "{}"));
        first.tools.push(ToolCall::new("b", "unluminous_plugins", "{}"));
        let result = Message::said(3, Role::Tool, "{}");
        let mut quiet = Message::new(4, Role::Assistant);
        quiet.tools.push(ToolCall::new("c", "unluminous_editor", "{}"));
        let answer = Message::said(5, Role::Assistant, "Done: task-1, task-2 and task-3.");
        let messages = [&question, &first, &result, &quiet, &answer];
        let rows = rows_of(messages.iter().map(|one| (*one, false)));
        let shape: Vec<String> = rows
            .iter()
            .map(|row| match row {
                Row::Said(one, _) => format!("said {}", one.id),
                Row::Tools(tools) => format!(
                    "run {}",
                    tools.iter().map(|tool| tool.id.as_str()).collect::<Vec<_>>().join("")
                ),
            })
            .collect();
        assert_eq!(shape, vec!["said 1", "said 2", "run abc", "said 5"]);
    }

    /// `task-1771`: pasting a picture into the composer had never worked, and this is why.
    #[test]
    fn a_paste_is_seen_on_the_key_going_up_and_never_as_an_empty_paste_event() {
        let chord = |pressed: bool, command: bool| egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers { command, ..Default::default() },
        };
        // What this used to watch for. `egui-winit` pushes `Event::Paste` only when the clipboard held
        // **text**, so with a picture on it the frame carries nothing at all — and the condition below is
        // one no window ever satisfies.
        assert!(!is_a_paste_chord(&[egui::Event::Paste(String::new())]));
        // The press is swallowed by the same early return that reads the clipboard.
        assert!(!is_a_paste_chord(&[chord(true, true)]));
        // The release is not, and the modifier is still down on it. This is the one report that arrives.
        assert!(is_a_paste_chord(&[chord(false, true)]));
        // A `V` with nothing held is a letter somebody typed.
        assert!(!is_a_paste_chord(&[chord(false, false)]));
    }

    /// A drop the platform cannot place is this pane's, because nothing else in Unluminous wants one.
    #[test]
    fn a_drop_with_no_pointer_still_lands_on_the_pane() {
        let area = Rect::from_min_size(Pos2::new(100.0, 40.0), Vec2::new(400.0, 600.0));
        assert!(belongs_here(Some(Pos2::new(200.0, 300.0)), area), "over the pane");
        assert!(!belongs_here(Some(Pos2::new(20.0, 300.0)), area), "over something else");
        // Windows carries a file through OLE and sends no cursor movement, so the last position `egui`
        // holds can be from before the drag began. Refusing there threw the picture away silently.
        assert!(belongs_here(None, area), "nowhere in particular");
    }
}
