//! One row of the conversation: a bubble, its pictures, its tool blocks and its failure.
//!
//! Since `task-2235` this is the Agent-Chat design on the Claude Design canvas: a message from the
//! person is a raised bubble on the right, `border-radius: 22px 22px 8px 22px`, and an answer has **no
//! bubble** at all: a round avatar and the agent's name over words set straight on the pane. A run of
//! tool calls is a command card with a status well, and a failure is an error card. The surfaces are
//! `rux`'s chat parts, drawn through [`super::kit`].
//!
//! ## The body is markdown, through the editor's own renderer
//!
//! `components::markdown_text` is `unluminous_core::markdown::render` plus a layout, which is exactly what
//! the editor's own preview is made of — so headings, lists, quotes, tables and fenced code all work
//! and none of it is a second renderer. A fence is coloured by whichever plugin claims its language,
//! through the `CodeHighlighter` the window put on the `Look`.
//!
//! What it does **not** draw is a picture or a Mermaid diagram written inside the text, for the
//! reason `components/markdown_text.rs` already records: resolving those needs two further passes
//! that decode an image and lay a diagram out. A picture *attached* to a message is drawn below the
//! words, which is where the page this is modelled on puts one.
//!
//! ## The height is worked out once and drawn from
//!
//! [`pieces`] is the one place a row's shape is decided, and both [`height`] and [`show`] read it —
//! so a row cannot be measured as one thing and drawn as another, which is the fault that leaves gaps
//! between bubbles or overlaps them.

use egui::{Color32, Pos2, Rect, Sense, Vec2};

use unluminous_chat::model::{Message, Part, Role, ToolCall};
use unluminous_chat::rich::{self, Segment};

use super::{kit, Act};
use crate::services::agent_chat::PaneState;
use crate::services::plugin_ui::Look;
use crate::theme::crisp::CrispPainter;
use crate::theme::icon;

/// A bubble's own padding: `padding: 12px 16px`. An answer has no bubble and no padding.
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 12.0;
/// A bubble's corner radius, and the one at its foot on the person's side: `22px 22px 8px 22px`.
const RADIUS: f32 = 22.0;
const CORNER: f32 = 8.0;
/// The widest a bubble is: `max-width: 300px`.
const BUBBLE_MOST: f32 = 300.0;
/// The avatar row over an answer: a 30 point round avatar and the agent's name.
const SPEAKER: f32 = 30.0;
/// Between the parts of one row: the avatar, the words, a picture.
const PIECE_GAP: f32 = 12.0;
/// How much of the row a bubble may take, by who said it.
///
/// The reference's own are `75%` and `85%`, which is what makes an answer read as speech rather than as a
/// container that happens to hold words — and those are the figures for a page the width of a browser.
/// **In a pane they leave too little.** `task-1848` reports the margins as too large, and a share is the
/// other half of that: at 85% of a 420 point pane less the card's padding, an answer had about 340 points
/// of line, and the eye runs out of words before the end of the sentence.
///
/// So 82 and 94 in a pane. Still short of the full width, because the alignment has to keep saying who
/// spoke: an earlier version used 96 and at 96 the assistant bubble filled the pane and the alignment
/// stopped meaning anything. The difference between 94 and the user's 82 is what carries it now.
const USER_SHARE: f32 = 0.82;
/// How wide a report is: a tool block, a failure, the thinking. Nearly the whole row, because none of
/// them is speech.
const BLOCK_SHARE: f32 = 0.98;
/// The tallest a picture inside a bubble is drawn.
const PICTURE: f32 = 200.0;
/// One tool block's own header.
const TOOL_ROW: f32 = 24.0;
/// The thinking row's own header.
const THINKING_ROW: f32 = 20.0;

/// One part of a row, with the height it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Piece {
    /// The round avatar and the agent's name over an answer.
    Speaker,
    /// The `<think>` block's header, and its body when it is open.
    Thinking {
        body: f32,
    },
    /// One run of the words, as markdown: which segment of the answer it is, and how wide its bubble is.
    ///
    /// An answer with no components is one run, segment zero, which is every answer written before
    /// `task-2211`.
    Words {
        body: f32,
        segment: usize,
        bubble: f32,
        /// The padding above and below the words: a bubble's, or nothing for an answer.
        pad: f32,
    },
    /// A component the agent wrote, which is segment `segment` of the answer.
    Block {
        segment: usize,
        height: f32,
    },
    /// A picture attached to the message, with the width its row is given.
    ///
    /// Its own width rather than the bubble's, because a message that is *only* a picture has no
    /// words to measure and its bubble is therefore the smallest one allowed — which drew a
    /// photograph sixty points across.
    Picture {
        index: usize,
        width: f32,
        height: f32,
    },
    Tool {
        index: usize,
        body: f32,
    },
    Failure {
        body: f32,
    },
    /// The line under a question that has been sent and is waiting its turn.
    Queued,
}

impl Piece {
    fn height(self) -> f32 {
        match self {
            Self::Speaker => SPEAKER,
            Self::Thinking { body } => THINKING_ROW + body,
            Self::Words { body, pad, .. } => body + pad * 2.0,
            Self::Block { height, .. } => height,
            Self::Picture { height, .. } => height,
            Self::Tool { body, .. } => TOOL_ROW + body,
            Self::Failure { body } => body + 94.0,
            Self::Queued => THINKING_ROW,
        }
    }
}

/// What a row is made of, and how wide its bubble is.
///
/// The one place a row's shape is decided. Measured in **unscaled** points and multiplied by
/// `Look::scale` by the caller, so the proportions the design settled on hold at every font size —
/// which is the rule `Look::scale`'s own comment sets out.
fn pieces(
    message: &Message,
    state: &mut PaneState,
    look: &Look<'_>,
    width: f32,
    queued: bool,
    with_tools: bool,
    ui: Option<&mut egui::Ui>,
) -> (Vec<Piece>, f32, f32, String) {
    let scale = look.scale();
    let mine = message.role == Role::User;
    // A bubble is as wide as its words up to 300 points, and an answer's words take the whole row.
    let most = match mine {
        true => (width * USER_SHARE).min(BUBBLE_MOST * scale),
        false => width,
    };
    let text = message.text();
    // **A bubble is as wide as what is in it, up to its share.** A short question drawn at eighty per
    // cent of the pane would not read as a short question. Measured with egui's own layout of the
    // plain text, which is within a point or two of what `unluminous_core` will lay the markdown out at,
    // plus a little slack so the two cannot disagree about where a line wraps.
    // `measure` answers in the pixels the words will really be drawn at, so the padding and the
    // slack added to it are scaled too, and so is the smallest a bubble is allowed to be. They were
    // not, and at the default font size that is invisible because `scale` is 1 — which is why 2,779
    // tests and 363 screenshots, all taken at 16 pt, agree with a bubble that is wrong at 41 pt.
    // Unscaled, the sixty-point floor collides with the scaled padding `show` subtracts from it:
    // at 41 pt that is 61 points taken out of a 60 point bubble, `inside` clamps to 24, and a
    // one-character answer is laid out into less room than one character needs. The bubble is drawn,
    // and it is empty.
    let smallest = 60.0 * scale;
    let natural = match text.is_empty() {
        true => 0.0,
        false => measure(look, &text, most - PAD_X * 2.0 * scale) + (PAD_X * 2.0 + 8.0) * scale,
    };
    let bubble = natural.clamp(smallest.min(most), most).max(smallest.min(most));
    // **A tool block, a failure and the thinking are as wide as the row allows, whatever the words
    // above them are.** They are reports rather than speech: sized to their own message, a tool called
    // from a two word answer came out two words wide, with its own caret clipped off the end of it.
    let block = width * BLOCK_SHARE;
    let in_block = (block - 24.0 * scale).max(24.0);

    let mut out = Vec::new();
    // The avatar and the name over an answer, once, before anything else it said.
    let says = !text.is_empty() || !message.thinking.is_empty() || message.failure.is_some();
    if !mine && message.role == Role::Assistant && says {
        out.push(Piece::Speaker);
    }
    if !message.thinking.is_empty() {
        let body = match state.opened_thinking.contains(&message.id) {
            true => {
                rendered_height(
                    state,
                    look,
                    &format!("think-{}", message.id),
                    &message.thinking,
                    in_block,
                ) + 6.0
            }
            false => 0.0,
        };
        out.push(Piece::Thinking { body });
    }
    if !text.is_empty() {
        out.extend(said(message, &text, state, look, width, most, smallest, ui));
    }
    for (index, part) in message.parts.iter().enumerate() {
        if let Part::Picture { bytes, .. } = part {
            out.push(Piece::Picture {
                index,
                width: most,
                height: picture_height(state, &picture_key(message, index), bytes, most / scale)
                    + 8.0,
            });
        }
    }
    // A message whose tools are drawn as a run of their own — see [`run_height`] — leaves them out here.
    let tools = match with_tools {
        true => message.tools.as_slice(),
        false => &[],
    };
    for (index, tool) in tools.iter().enumerate() {
        // Drawn as a run of one, so a message that carries its own calls shows the same card.
        let body = run_height(&[tool], state, look, block) / scale - TOOL_ROW;
        out.push(Piece::Tool { index, body });
    }
    if let Some(failure) = &message.failure {
        let body =
            rendered_height(state, look, &format!("failure-{}", message.id), failure, in_block);
        out.push(Piece::Failure { body });
    }
    // Under the bubble rather than over it, because it is a note about the question above it.
    if queued {
        out.push(Piece::Queued);
    }
    (out, bubble, block, text)
}

/// A row worked out: what it is made of, how wide each part is, and how tall the whole thing is.
///
/// **Worked out once a frame rather than twice.** The caller has to know a row's height before it can
/// allocate the rectangle to draw it in, and the first version answered that by running the whole of
/// [`pieces`] again — which builds the message's text and looks up every rendered block a second
/// time. Now it is built once and handed to [`show`].
pub struct Shape {
    pieces: Vec<Piece>,
    bubble: f32,
    block: f32,
    /// The message's words, built once. `Message::text` joins its parts, so it allocates.
    text: String,
    /// How tall the row is, in points, at the size the window is set to.
    pub height: f32,
}

/// What this row is made of and how tall it is.
///
/// `queued` is a question that has been sent while an answer was still arriving and is waiting its
/// turn — it is drawn as an ordinary bubble with one quiet line under it saying so. See
/// [`crate::services::agent_chat::AgentChat::send`].
///
/// `with_tools` is false for a message whose tool calls the conversation draws as a run of their own,
/// which is every message from the model since `task-2193` — see [`run_height`].
pub fn shape(
    message: &Message,
    state: &mut PaneState,
    look: &Look<'_>,
    width: f32,
    queued: bool,
    with_tools: bool,
    ui: Option<&mut egui::Ui>,
) -> Shape {
    let scale = look.scale();
    let (pieces, bubble, block, text) = pieces(message, state, look, width, queued, with_tools, ui);
    let height = pieces.iter().map(|piece| piece.height() * scale).sum::<f32>()
        + (pieces.len().saturating_sub(1) as f32) * PIECE_GAP * scale;
    Shape { pieces, bubble, block, text, height }
}

/// Draw the row and say what was pressed.
pub fn show(
    message: &Message,
    shape: Shape,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let Shape { pieces, bubble: bubble_width, block: block_width, text: said, .. } = shape;
    let mine = message.role == Role::User;
    let segments = segments_of(message, &said);
    let mut pen = area.top();
    for piece in pieces {
        let height = piece.height() * scale;
        // A bubble is as wide as its words and sits on its own side; a report is as wide as the row
        // and always starts at the left.
        let wide = matches!(
            piece,
            Piece::Tool { .. }
                | Piece::Failure { .. }
                | Piece::Thinking { .. }
                | Piece::Block { .. }
        );
        let width = match piece {
            Piece::Picture { width, .. } => width,
            Piece::Words { bubble, .. } => bubble,
            _ => match wide {
                true => block_width,
                false => bubble_width,
            },
        };
        let left = match mine && !wide {
            true => area.right() - width,
            false => area.left(),
        };
        let rect = Rect::from_min_size(Pos2::new(left, pen), Vec2::new(width, height));
        match piece {
            Piece::Speaker => speaker(state, ui, look, rect, message.id),
            Piece::Thinking { body } => {
                acts.extend(thinking(message, state, ui, look, rect, body > 0.0))
            }
            Piece::Words { segment, pad, .. } => {
                let pad_x = if mine { PAD_X } else { 0.0 };
                if mine {
                    bubble(state, ui, look, rect, message.id, segment);
                }
                let inside = Rect::from_min_size(
                    rect.min + Vec2::new(pad_x * scale, pad * scale),
                    Vec2::new(
                        rect.width() - pad_x * 2.0 * scale,
                        rect.height() - pad * 2.0 * scale,
                    ),
                );
                let words_of = match segments.get(segment) {
                    Some(Segment::Markdown(words)) => words.as_str(),
                    _ => said.as_str(),
                };
                acts.extend(words(message, state, ui, look, rect, inside, words_of, mine, segment));
            }
            Piece::Block { segment, .. } => {
                if let Some(Segment::Block { source, finished }) = segments.get(segment) {
                    acts.extend(block_show(
                        message.id, segment, source, *finished, state, look, ui, rect,
                    ));
                }
            }
            Piece::Picture { index, .. } => picture(message, state, ui, look, rect, index),
            Piece::Tool { index, body } => {
                if let Some(tool) = message.tools.get(index) {
                    let _ = body;
                    acts.extend(run_show(&[tool], state, ui, look, rect));
                }
            }
            Piece::Failure { .. } => failure(message, state, ui, look, rect),
            Piece::Queued => queued_note(ui, look, rect, mine),
        }
        pen += height + PIECE_GAP * scale;
    }
    acts
}

/// How long the copy button stays up after the pointer has left the message, in seconds.
///
/// `task-2060`: *"I cannot actually select copy of a message because it goes away as soon as i hover
/// off the message. It should stay visible for 2 seconds so I can actually press it."* The button is
/// drawn *beside* the bubble rather than inside it - that is what keeps a column of answers from
/// being a column of buttons - so the pointer has to leave the message to reach it, and a button that
/// went away on the frame it was left could not be pressed at all.
const COPY_LINGER: f64 = 2.0;

/// The bubble's words: the markdown, the selection over it, and the copy button beside it.
///
/// `rect` is the bubble and `inside` is the room its words get. Split out of [`show`] because the
/// selection, the lingering button and the right click menu each have to know exactly where the
/// letters were drawn, which is one number - `markdown_text::Rendered::centring` - and three copies
/// of it would be three chances to disagree about it.
#[allow(clippy::too_many_arguments)]
fn words(
    message: &Message,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    inside: Rect,
    said: &str,
    mine: bool,
    segment: usize,
) -> Vec<Act> {
    use crate::services::agent_chat::{MessageMenu, Selected};

    let scale = look.scale();
    let mut acts = Vec::new();
    let key = words_key(message.id, segment);
    let code = code_colours(look);
    // **The words are selectable**, which is `task-2060`: a message is text to read, and reading
    // includes taking a copy of a part of it. `click_and_drag` rather than `hover` for the reason
    // the Markdown preview senses both - see `UnluminousApp::show_markdown_preview`.
    let response = ui.interact(
        rect,
        ui.id().with(("agent-chat-bubble", message.id, segment)),
        Sense::click_and_drag(),
    );
    // Read out of the state before the rendered markdown is borrowed from it.
    let was = match state.selection {
        Some(selected) if selected.message == message.id && selected.segment == segment => {
            Some(selected.range)
        }
        _ => None,
    };
    let picked = {
        let made = rendered(state, look, &key, said, inside.width());
        // **Where the letters really start.** A line is taller than its glyphs and all of the extra
        // leading is added below the baseline, so a block drawn at the top of a box padded equally
        // above and below sits high - `task-2060`: *"The text in a message isnt perfectly vertically
        // aligned."* `Rendered::centring` is that difference, halved, and it is read here for the
        // drawing, for the selection and for the pointer, so the three cannot disagree.
        let down = made.centring();
        let origin = Pos2::new(inside.left(), inside.top() + down);
        let picked = crate::components::editor_view::read_pointer(
            &response,
            &made.layout,
            &made.text,
            origin,
            was.unwrap_or_else(|| unluminous_core::Selection::caret(0)),
        );
        // What is painted is the selection this frame's drag made rather than the one it started
        // with, which is `select_in_the_preview`'s own ordering.
        if let Some(selection) = picked.or(was) {
            crate::components::editor_view::paint_behind(
                ui,
                &made.layout,
                origin,
                selection.range(),
                crate::theme::color::text_selection(),
                2.0,
            );
        }
        // `show_with` draws at `area.top() - scroll`, so the centring is a negative scroll: the code
        // panels behind the words move with them rather than being a second answer to one question.
        crate::components::markdown_text::show_with(
            ui,
            inside,
            made,
            look.renderer,
            -down,
            Some(code),
        );
        picked
    };
    if let Some(selection) = picked {
        state.selection = Some(Selected { message: message.id, segment, range: selection });
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    // The right click menu: copy what is selected, copy the whole message, select all of it. Opened
    // here and drawn by `components::agent_chat::message_menu` once everything else has been, because
    // a popup is a layer of its own and this one is opened from inside a scrolling area.
    if response.secondary_clicked() {
        // **The window's own points, not the layer's.** A popup is an `egui::Area` and is placed in
        // screen space, so where it opens is asked of the context rather than of this `Ui` — which is
        // exactly the split `controls::field_menu` keeps, and the reason a menu opened inside a chat
        // node on a panned canvas lands under the pointer rather than somewhere else.
        if let Some(at) = ui.ctx().pointer_interact_pos() {
            state.menu = Some(MessageMenu { at, message: message.id });
        }
    }
    // The copy button, which is `.messageActions`: it appears under the pointer rather than sitting
    // there, because a column of bubbles each with a permanent button on it is a column of buttons.
    // **And it stays up for [`COPY_LINGER`] after the pointer has left**, so it can be reached.
    let now = ui.input(|input| input.time);
    if response.contains_pointer() {
        state.copy_shown = Some((message.id, now));
    }
    let up =
        state.copy_shown.is_some_and(|(id, when)| id == message.id && now - when < COPY_LINGER);
    if up {
        let at = Rect::from_center_size(
            Pos2::new(
                match mine {
                    true => rect.left() - 12.0 * scale,
                    false => rect.right() + 12.0 * scale,
                },
                rect.top() + 12.0 * scale,
            ),
            Vec2::splat(18.0 * scale),
        );
        // The pointer resting on the button keeps it up as surely as the pointer on the message
        // does - otherwise the two seconds would run out under a pointer that had just arrived.
        if crate::components::controls::pointer_in(ui).is_some_and(|pointer| at.contains(pointer)) {
            state.copy_shown = Some((message.id, now));
        }
        if crate::components::controls::icon_button_at(ui, at, "Copy message", icon::copy, scale) {
            acts.push(Act::Copy(said.to_owned()));
        }
        // An idle window draws twice a second, so without this the button would linger for up to
        // half a second longer than it was asked to. See `app::HEARTBEAT`.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
    }
    acts
}

/// A painter for `rect`, cut to what the scrolling area can actually show.
///
/// `Ui::painter_at` **sets** the clip rectangle rather than intersecting it, so a row scrolled half
/// out of the conversation drew its whole self over whatever was above the scrolling area — the pane's
/// own header, measured on a real window. One function rather than the same intersection in nine
/// places, which is the reason `controls::field_text` exists.
fn painter_in(ui: &egui::Ui, rect: Rect) -> egui::Painter {
    ui.painter_at(rect.intersect(ui.clip_rect()))
}

/// The person's bubble: a raised card, `border-radius: 22px 22px 8px 22px`, drawn with `rux`'s chat
/// values so it is the same surface as every card in the pane.
fn bubble(
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    message: u64,
    segment: usize,
) {
    kit::layer(state, look, ui, ("bubble", message, segment), rect, kit::SMALL_REACH, |rux| {
        let chat = rux.theme().chat;
        let corners = rux::Corners {
            nw: rux.z(RADIUS),
            ne: rux.z(RADIUS),
            se: rux.z(CORNER),
            sw: rux.z(RADIUS),
        };
        rux.chrome.surface(rect, corners, rux::Fill::gradient(chat.card, rect), chat.raised_sm());
    });
}

/// The round avatar and the agent's name over an answer: a 30 point raised disc with the spark in the
/// agent's colour, and the name at 12.5 points.
fn speaker(state: &mut PaneState, ui: &mut egui::Ui, look: &Look<'_>, rect: Rect, message: u64) {
    let scale = look.scale();
    let disc = Rect::from_min_size(rect.min, Vec2::splat(SPEAKER * scale));
    let agent = look.palette.agent;
    kit::layer(state, look, ui, ("speaker", message), disc, kit::SMALL_REACH, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(
            disc,
            disc.width() / 2.0,
            rux::Fill::gradient(chat.card, disc),
            chat.raised_sm(),
        );
        rux.mark(rux::Mark::new(rux::Icon::Spark, rux.z(14.0)).stroke(2.0), disc.center(), agent);
    });
    let name = match state.speaker.is_empty() {
        true => "agent",
        false => state.speaker.as_str(),
    };
    let painter = painter_in(ui, rect);
    let style = rux::Style::sans(12.5 * scale).semibold();
    let ink = crate::theme::rux_theme().ink.i900;
    let galley = rux::text::layout(&painter, style, name, ink);
    rux::text::draw_left_capitals(
        &painter,
        Pos2::new(disc.right() + 10.0 * scale, disc.center().y),
        galley,
        style,
        ink,
    );
}

/// The `<think>` block: a quiet row that opens.
fn thinking(
    message: &Message,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    open: bool,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), THINKING_ROW * scale));
    let response =
        ui.interact(head, ui.id().with(("agent-chat-thinking", message.id)), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Thinking".to_owned())
    });
    let painter = painter_in(ui, rect);
    icon::disclosure_at(
        &painter,
        Pos2::new(head.left() + 6.0 * scale, head.center().y),
        open,
        look.palette.text_faint,
        scale,
    );
    painter.crisp_text(
        Pos2::new(head.left() + 16.0 * scale, head.center().y),
        egui::Align2::LEFT_CENTER,
        "Thinking",
        egui::FontId::proportional(look.font_size * 0.78),
        look.palette.text_faint,
    );
    if response.clicked() {
        acts.push(Act::ToggleThinking(message.id));
    }
    if open {
        let body = Rect::from_min_max(
            Pos2::new(rect.left() + 16.0 * scale, head.bottom() + 2.0 * scale),
            rect.max,
        );
        let key = format!("think-{}", message.id);
        let code = code_colours(look);
        let made = rendered(state, look, &key, &message.thinking, body.width());
        crate::components::markdown_text::show_with(ui, body, made, look.renderer, 0.0, Some(code));
    }
    acts
}

/// Where a message's picture is cached, by the message and the part.
fn picture_key(message: &Message, index: usize) -> String {
    format!("picture-{}-{index}", message.id)
}

/// How tall a picture is drawn, in unscaled points, at `width` points across.
///
/// **The picture's real height rather than the tallest one may be.** Reserving [`PICTURE`] whatever
/// the picture was left a landscape photograph with a column of empty pane under it, all the way down
/// to the next message. `fit` is the same arithmetic [`picture`] draws with, so the room reserved and
/// the room used are one answer rather than two that can disagree.
fn picture_height(state: &mut PaneState, key: &str, bytes: &[u8], width: f32) -> f32 {
    let size = match state.picture_sizes.get(key) {
        Some(size) => *size,
        None => {
            // A picture that will not even say how big it is shows its name instead, on one row.
            let size = crate::services::picture::dimensions_of(bytes).unwrap_or((width, 20.0));
            state.picture_sizes.insert(key.to_owned(), size);
            size
        }
    };
    fit(size, width).1
}

/// A picture of `size` fitted into `width` points, never enlarged and never taller than [`PICTURE`].
fn fit(size: (f32, f32), width: f32) -> (f32, f32) {
    let room = (width - 4.0).max(1.0);
    let factor = (room / size.0.max(1.0)).min(PICTURE / size.1.max(1.0)).min(1.0);
    (size.0 * factor, size.1 * factor)
}

/// A picture attached to a message, drawn under its words.
fn picture(
    message: &Message,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    index: usize,
) {
    let Some(Part::Picture { media, bytes, name }) = message.parts.get(index) else {
        return;
    };
    let key = picture_key(message, index);
    // **Uploaded once and kept**, keyed on the message and the part rather than on the bytes, so a
    // conversation with twenty pictures in it does not decode twenty pictures on every frame. Through
    // `services::picture::upload`, which shrinks to the card's largest texture first — egui *panics*
    // when handed a bigger one, and a four thousand pixel screenshot is an ordinary thing to attach.
    if !state.pictures.contains_key(&key) {
        match crate::services::picture::decode_bytes(bytes) {
            Ok(image) => {
                let texture = crate::services::picture::upload(
                    ui.ctx(),
                    key.clone(),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                state.pictures.insert(key.clone(), texture);
            }
            Err(_) => {
                // A picture that will not decode shows what it was called, which is the alt text rule
                // the Markdown preview already keeps.
                painter_in(ui, rect).crisp_text(
                    rect.min,
                    egui::Align2::LEFT_TOP,
                    format!("{name} ({media}) could not be drawn"),
                    egui::FontId::proportional(look.font_size * 0.8),
                    look.palette.text_dim,
                );
                return;
            }
        }
    }
    let Some(texture) = state.pictures.get(&key).cloned() else {
        return;
    };
    let scale = look.scale();
    let size = texture.size_vec2();
    // The same arithmetic the row was measured with, so the picture fills the room reserved for it.
    let (wide, tall) = fit((size.x, size.y), rect.width() / scale);
    // On its own side, which is what makes a picture somebody sent read as part of what they said.
    let left = match message.role == Role::User {
        true => rect.right() - (wide + 2.0) * scale,
        false => rect.left() + 2.0 * scale,
    };
    let drawn = Rect::from_min_size(
        Pos2::new(left, rect.top() + 4.0 * scale),
        Vec2::new(wide, tall) * scale,
    );
    kit::layer(state, look, ui, ("picture", message.id, index), drawn, kit::SMALL_REACH, |rux| {
        let chat = rux.theme().chat;
        rux.chrome.surface(drawn, rux.z(17.0), chat.well, chat.raised_sm());
    });
    painter_in(ui, rect).image(
        texture.id(),
        drawn,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE, // any ground: an image's tint, which leaves it as it is
    );
}

/// How tall a tool block's open body is.
fn tool_body_height(state: &mut PaneState, look: &Look<'_>, tool: &ToolCall, width: f32) -> f32 {
    let inside = (width - 24.0).max(24.0);
    let mut height = rendered_height(
        state,
        look,
        &format!("tool-args-{}", tool.id),
        &fenced(&tool.arguments),
        inside,
    );
    if let Some(answer) = &tool.answer {
        height += rendered_height(
            state,
            look,
            &format!("tool-answer-{}", tool.id),
            &fenced(answer),
            inside,
        );
    }
    height + 6.0
}

/// A tool's arguments or its answer, as a fenced block so the markdown renderer sets it in the code
/// font and puts it in a well.
///
/// **JSON is laid out over several lines and fenced as `json`.** `task-2060`: *"The formatting of
/// the agent commands needs to be improved. Pretty print, better text coloring (keyword highlights,
/// etc."* A tool call's arguments arrive as one long line of JSON, and a fence with no language
/// after it names nothing, so the plugin that would colour it is never asked and the whole block
/// is drawn in the one code colour. Named, the JSON plugin tells its strings, its numbers and its
/// three literal values apart, exactly as it does in a `.json` file.
///
/// Anything that is not JSON is left exactly as it arrived, in a fence naming no language: a shell
/// command's output is not JSON, and colouring it as though it were would be colouring it wrongly.
fn fenced(text: &str) -> String {
    let trimmed = text.trim();
    match laid_out_json(trimmed) {
        Some(pretty) => format!("```json\n{pretty}\n```"),
        None => format!("```\n{trimmed}\n```"),
    }
}

/// `text` laid out over several lines when it is JSON worth laying out, and [`None`] when it is not.
///
/// An object or an array only. A bare string, a number or `null` is JSON too and is already one
/// line, so laying it out would change nothing and the fence would claim a language for a word.
///
/// The limit is not tidiness: this runs whenever the block is rendered again, and a tool can answer
/// with a great deal. `shorten_for_a_model` cuts what goes back up the wire at eight thousand
/// characters and this is the same order of size, some way above it.
fn laid_out_json(text: &str) -> Option<String> {
    const LIMIT: usize = 64_000;
    if text.len() > LIMIT || !text.starts_with(['{', '[']) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// The line under a question that is waiting its turn behind an answer.
///
/// A clock and one quiet word, on the side the question is on. It says the thing a person cannot
/// otherwise tell: the message is on the screen, and it has **not** been asked yet. `task-2060`.
fn queued_note(ui: &mut egui::Ui, look: &Look<'_>, rect: Rect, mine: bool) {
    let scale = look.scale();
    let painter = painter_in(ui, rect);
    let font = egui::FontId::proportional(look.font_size * 0.72);
    let tint = look.palette.text_faint;
    let width = painter.layout_no_wrap("Queued".to_owned(), font.clone(), tint).size().x;
    let right = match mine {
        true => rect.right(),
        false => rect.left() + width + 16.0 * scale,
    };
    painter.crisp_text(
        Pos2::new(right - width, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Queued",
        font,
        tint,
    );
    icon::scaled(
        &painter,
        Pos2::new(right - width - 9.0 * scale, rect.center().y),
        tint,
        scale,
        icon::clock,
    );
}

/// A card's header row: as tall as its 34 point status well.
const CARD_HEADER: f32 = 34.0;
/// A card's padding and the gap between its rows: `padding: 12px; gap: 12px`.
const CARD_PAD: f32 = 12.0;
/// One tool's line inside an open run's well.
const TOOL_LINE: f32 = 30.0;
/// The padding inside a card's well: `padding: 12px 14px`.
const WELL_PAD_X: f32 = 14.0;
const WELL_PAD_Y: f32 = 12.0;

/// The key a run of tool calls is opened and shut by: the first call's id, which no other run shares.
pub fn run_key(tools: &[&ToolCall]) -> String {
    tools.first().map(|tool| format!("run-{}", tool.id)).unwrap_or_default()
}

/// What a run's status well says: running while any call is, failed when any failed, and done.
fn run_status(tools: &[&ToolCall]) -> rux::components::Status {
    use rux::components::Status;
    match (tools.iter().any(|tool| tool.is_running()), tools.iter().any(|tool| tool.failed)) {
        (true, _) => Status::Running,
        (false, true) => Status::Failed,
        (false, false) => Status::Done,
    }
}

/// How long a call took, the way the design writes it: `0.2s`, `1.4s`, `2m 58s`.
pub fn took(milliseconds: u64) -> String {
    let seconds = milliseconds as f64 / 1000.0;
    match seconds < 60.0 {
        true => format!("{seconds:.1}s"),
        false => {
            let whole = seconds.round() as u64;
            format!("{}m {:02}s", whole / 60, whole % 60)
        }
    }
}

/// A tool's name without the `unluminous_` every one of Unluminous's own commands starts with.
fn tool_name(tool: &ToolCall) -> &str {
    tool.name.trim_start_matches("unluminous_")
}

/// What a call did, in the words a card shows: the command it ran when it ran one, which is what the
/// design's run card names (`cargo build --release`), and otherwise its name and the file or pattern
/// it was about.
pub fn tool_title(tool: &ToolCall) -> String {
    let arguments: serde_json::Value = serde_json::from_str(&tool.arguments).unwrap_or_default();
    let said = |key: &str| arguments[key].as_str().map(str::trim).filter(|one| !one.is_empty());
    if let Some(command) = said("command").or_else(|| said("cmd")) {
        return command.lines().next().unwrap_or(command).to_owned();
    }
    let about = ["file_path", "path", "file", "pattern", "query", "url"].into_iter().find_map(said);
    match about {
        Some(about) => format!("{} {about}", tool_name(tool)),
        None => tool_name(tool).to_owned(),
    }
}

/// Whether one call's arguments and answer are showing.
fn tool_open(tool: &ToolCall, state: &PaneState) -> bool {
    state.opened_tools.contains(&tool.id) || tool.is_running()
}

/// How tall a run's open body is, in unscaled points: the well and what is in it.
fn run_body(tools: &[&ToolCall], state: &mut PaneState, look: &Look<'_>, inside: f32) -> f32 {
    match tools {
        [] => 0.0,
        [only] => tool_body_height(state, look, only, inside) + WELL_PAD_Y * 2.0,
        many => {
            let mut height = WELL_PAD_Y * 2.0;
            for tool in many {
                height += TOOL_LINE;
                if tool_open(tool, state) {
                    height += tool_body_height(state, look, tool, inside) + 4.0;
                }
            }
            height
        }
    }
}

/// Whether a run's body is showing: one call shows its own, several show the list once opened.
fn run_open(tools: &[&ToolCall], state: &PaneState) -> bool {
    match tools {
        [only] => tool_open(only, state),
        many => state.opened_groups.contains(&run_key(many)),
    }
}

/// How tall a run of tool calls is, in points at the size the window is set to.
///
/// `task-2193`: the calls between two things said are one row, so the words an agent said are not pushed
/// off the top of the pane by the list of what it did to find them. Since `task-2235` that row is the
/// design's command card: a status well, what ran and how long it took, and a chevron that shows the
/// calls in a well carved into the card. One call shows its own arguments and answer there; several
/// show one line each, and a line opens to show that call's.
pub fn run_height(tools: &[&ToolCall], state: &mut PaneState, look: &Look<'_>, width: f32) -> f32 {
    let scale = look.scale();
    if tools.is_empty() {
        return 0.0;
    }
    let inside = (width / scale - (CARD_PAD + WELL_PAD_X) * 2.0).max(24.0) * scale;
    let mut height = CARD_PAD * 2.0 + CARD_HEADER;
    if run_open(tools, state) {
        height += CARD_PAD + run_body(tools, state, look, inside);
    }
    height * scale
}

/// Draw a run of tool calls into `rect`, which [`run_height`] measured.
pub fn run_show(
    tools: &[&ToolCall],
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
) -> Vec<Act> {
    use rux::components::{RoundButton, RoundContent, RoundSize, Status, StatusWell};
    let scale = look.scale();
    let mut acts = Vec::new();
    let open = run_open(tools, state);
    let status = run_status(tools);
    let (title, mono) = match tools {
        [only] => (tool_title(only), true),
        many => (format!("Ran {} tool calls", many.len()), false),
    };
    let detail = match tools.iter().find(|tool| tool.is_running()) {
        Some(tool) => format!("Running {}", tool_name(tool)),
        None => {
            let total: u64 = tools.iter().filter_map(|tool| tool.took).sum();
            let failed = tools.iter().filter(|tool| tool.failed).count();
            match failed {
                0 => took(total),
                1 => format!("{} \u{00B7} 1 failed", took(total)),
                count => format!("{} \u{00B7} {count} failed", took(total)),
            }
        }
    };
    let pad = CARD_PAD * scale;
    let header = Rect::from_min_size(
        rect.min + Vec2::splat(pad),
        Vec2::new(rect.width() - pad * 2.0, CARD_HEADER * scale),
    );
    let well = Rect::from_min_max(
        Pos2::new(header.left(), header.bottom() + pad),
        Pos2::new(header.right(), rect.bottom() - pad),
    );
    let toggle = Rect::from_center_size(
        Pos2::new(header.right() - 15.0 * scale, header.center().y),
        Vec2::splat(30.0 * scale),
    );
    let lines: Vec<Rect> = match (open, tools) {
        (true, [_, _, ..]) => {
            let mut top = well.top() + WELL_PAD_Y * scale;
            let inside = (well.width() - WELL_PAD_X * 2.0 * scale).max(24.0);
            tools
                .iter()
                .map(|tool| {
                    let line = Rect::from_min_size(
                        Pos2::new(well.left(), top),
                        Vec2::new(well.width(), TOOL_LINE * scale),
                    );
                    top += TOOL_LINE * scale;
                    if tool_open(tool, state) {
                        top += (tool_body_height(state, look, tool, inside) + 4.0) * scale;
                    }
                    line
                })
                .collect()
        }
        _ => Vec::new(),
    };
    let detail_colour = match status {
        Status::Failed => Some(kit::chat().slower),
        _ => None,
    };
    let pressed =
        kit::layer(state, look, ui, ("run", run_key(tools)), rect, kit::CARD_REACH, |rux| {
            let chat = rux.theme().chat;
            let theme = rux.theme();
            rux.chrome.surface(
                rect,
                rux.z(22.0),
                rux::Fill::gradient(chat.card, rect),
                chat.raised(),
            );
            let side = rux.z(CARD_HEADER);
            StatusWell::new(status).show(rux, Rect::from_min_size(header.min, Vec2::splat(side)));
            let left = header.left() + side + rux.z(12.0);
            let room = (toggle.left() - rux.z(12.0) - left).max(0.0);
            let title_style = match mono {
                true => rux.zs(rux::Style::mono(12.0)),
                false => rux.zs(rux::Style::sans(12.5)),
            };
            let title = rux.elided(title_style, &title, theme.ink.i900, room);
            let ink = detail_colour.unwrap_or(theme.ink.i400);
            let detail = rux.elided(rux.zs(rux::Style::sans(11.0)), &detail, ink, room);
            let block = title.size().y + rux.z(1.0) + detail.size().y;
            let top = header.center().y - block / 2.0;
            let title_height = title.size().y;
            rux.painter().galley(Pos2::new(left, top), title, theme.ink.i900);
            rux.painter().galley(Pos2::new(left, top + title_height + rux.z(1.0)), detail, ink);
            let (label, turn) = match open {
                true => ("Hide tool calls", std::f32::consts::PI),
                false => ("Show tool calls", 0.0),
            };
            let mark = rux::Mark::new(rux::Icon::ChevDown, 12.0).stroke(2.0).turn(turn);
            let pressed = RoundButton::new(RoundContent::Mark(mark), label)
                .size(RoundSize::Small)
                .show(rux, toggle)
                .clicked();
            if open {
                rux.chrome.surface(well, rux.z(16.0), chat.well, chat.carved_sm());
            }
            // One line a call in an open run: its state as a mark, its name, and how long it took.
            for (tool, line) in tools.iter().zip(&lines) {
                let (icon, colour) = match (tool.is_running(), tool.failed) {
                    (true, _) => (rux::Icon::Clock, chat.sky),
                    (false, true) => (rux::Icon::Cross, chat.slower),
                    (false, false) => (rux::Icon::Tick, chat.faster),
                };
                let centre = Pos2::new(line.left() + rux.z(WELL_PAD_X + 6.0), line.center().y);
                rux.mark(rux::Mark::new(icon, rux.z(16.0)), centre, colour);
                let time = match tool.took {
                    Some(ms) => took(ms),
                    None => "running".to_owned(),
                };
                let time = rux.text(rux.zs(rux::Style::sans(11.0)), &time, theme.ink.i400);
                let right = line.right() - rux.z(WELL_PAD_X);
                let time_width = time.size().x;
                rux::text::draw_left_centre(
                    rux.painter(),
                    Pos2::new(right - time_width, line.center().y),
                    time,
                    theme.ink.i400,
                );
                let start = centre.x + rux.z(14.0);
                let name = rux.elided(
                    rux.zs(rux::Style::mono(11.5)),
                    &tool_title(tool),
                    theme.ink.i500,
                    (right - time_width - rux.z(10.0) - start).max(0.0),
                );
                rux::text::draw_left_centre(
                    rux.painter(),
                    Pos2::new(start, line.center().y),
                    name,
                    theme.ink.i500,
                );
            }
            pressed
        });
    if pressed {
        acts.push(match tools {
            [only] => Act::ToggleTool(only.id.clone()),
            many => Act::ToggleGroup(run_key(many)),
        });
    }
    // Each line opens its own call, and what an open call said is drawn over the well with the editor's
    // own markdown renderer, after the decoration so it sits on top.
    for (tool, line) in tools.iter().zip(&lines) {
        let response =
            ui.interact(*line, ui.id().with(("agent-chat-tool", &tool.id)), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Tool: {}", tool.name),
            )
        });
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            acts.push(Act::ToggleTool(tool.id.clone()));
        }
        if tool_open(tool, state) {
            let body = Rect::from_min_max(
                Pos2::new(line.left() + WELL_PAD_X * scale, line.bottom()),
                Pos2::new(line.right() - WELL_PAD_X * scale, well.bottom()),
            );
            tool_body(tool, state, ui, look, body);
        }
    }
    if open {
        if let [only] = tools {
            let body = well.shrink2(Vec2::new(WELL_PAD_X * scale, WELL_PAD_Y * scale));
            tool_body(only, state, ui, look, body);
        }
    }
    acts
}

/// What a call was asked and what it answered, as fenced blocks, from the top of `area`.
fn tool_body(
    tool: &ToolCall,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
) {
    let code = in_a_well(look);
    let arguments = fenced(&tool.arguments);
    let made = rendered(state, look, &format!("tool-args-{}", tool.id), &arguments, area.width());
    let used = made.height();
    crate::components::markdown_text::show_with(ui, area, made, look.renderer, 0.0, Some(code));
    if let Some(answer) = &tool.answer {
        let below = Rect::from_min_max(Pos2::new(area.left(), area.top() + used), area.max);
        let text = fenced(answer);
        let made = rendered(state, look, &format!("tool-answer-{}", tool.id), &text, below.width());
        crate::components::markdown_text::show_with(
            ui,
            below,
            made,
            look.renderer,
            0.0,
            Some(code),
        );
    }
}

/// An answer that stopped with an error: the design's error card, with the server's own words in its
/// well.
fn failure(
    message: &Message,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
) {
    let Some(said) = &message.failure else {
        return;
    };
    let scale = look.scale();
    let pad = CARD_PAD * scale;
    let header = Rect::from_min_size(
        rect.min + Vec2::splat(pad),
        Vec2::new(rect.width() - pad * 2.0, CARD_HEADER * scale),
    );
    let well = Rect::from_min_max(
        Pos2::new(header.left(), header.bottom() + pad),
        Pos2::new(header.right(), rect.bottom() - pad),
    );
    let speaker = state.speaker.clone();
    kit::layer(state, look, ui, ("failure", message.id), rect, kit::CARD_REACH, |rux| {
        let chat = rux.theme().chat;
        let theme = rux.theme();
        rux.chrome.surface(rect, rux.z(22.0), rux::Fill::gradient(chat.card, rect), chat.raised());
        let side = rux.z(CARD_HEADER);
        rux::components::StatusWell::new(rux::components::Status::Failed)
            .show(rux, Rect::from_min_size(header.min, Vec2::splat(side)));
        let left = header.left() + side + rux.z(12.0);
        let room = (header.right() - left).max(0.0);
        let title = rux.elided(
            rux.zs(rux::Style::sans(12.5)),
            "The answer stopped with an error",
            theme.ink.i900,
            room,
        );
        let detail = rux.elided(rux.zs(rux::Style::sans(11.0)), &speaker, theme.ink.i400, room);
        let block = title.size().y + rux.z(1.0) + detail.size().y;
        let top = header.center().y - block / 2.0;
        let title_height = title.size().y;
        rux.painter().galley(Pos2::new(left, top), title, theme.ink.i900);
        rux.painter().galley(
            Pos2::new(left, top + title_height + rux.z(1.0)),
            detail,
            theme.ink.i400,
        );
        rux.chrome.surface(well, rux.z(16.0), chat.well, chat.carved_sm());
    });
    let inside = well.shrink2(Vec2::new(WELL_PAD_X * scale, WELL_PAD_Y * scale));
    let key = format!("failure-{}", message.id);
    let code = in_a_well(look);
    let made = rendered(state, look, &key, said, inside.width());
    crate::components::markdown_text::show_with(ui, inside, made, look.renderer, 0.0, Some(code));
}

/// The colours markdown is rendered in here.
pub(super) fn colours(look: &Look<'_>) -> crate::components::markdown_text::Colors {
    crate::components::markdown_text::Colors {
        // Brighter than the dim body text the first version used: in the reference the depth does the
        // separating and the words are the bright thing on the surface.
        text: look.palette.text,
        strong: look.palette.text_strong,
        // **Blue, which is the reference's own `--accent-blue`.** It was the mint `attached` before,
        // which was the most conspicuously wrong colour in the pane: mint means *running* everywhere
        // else here, on a tool block and on the state dot.
        code: look.palette.text_strong,
        link: look.palette.accent,
        quiet: look.palette.text_dim,
        rule: look.palette.divider,
    }
}

/// What a code background is drawn in: a fence's pressed panel and an inline chip.
///
/// Both are colours Unluminous already has. The panel is the well every field in the window is, and the
/// chip is `CODE_CHIP`, which is what the Markdown preview already paints behind inline code.
pub(super) fn code_colours(look: &Look<'_>) -> crate::components::markdown_text::CodeColors {
    crate::components::markdown_text::CodeColors {
        panel: look.palette.board_well,
        // No chip behind inline code since `task-2235`: the design sets it in the code font and the
        // strongest ink, on the pane, and nothing else.
        chip: Color32::TRANSPARENT,
        radius: 6,
    }
}

/// The same colours for markdown drawn inside a card's well, where the well is already the panel and a
/// second one inside it would be a box in a box.
fn in_a_well(look: &Look<'_>) -> crate::components::markdown_text::CodeColors {
    crate::components::markdown_text::CodeColors {
        panel: Color32::TRANSPARENT,
        ..code_colours(look)
    }
}

/// The rendered markdown for `key`, made again only when the source or the width has changed.
fn rendered<'a>(
    state: &'a mut PaneState,
    look: &Look<'_>,
    key: &str,
    source: &str,
    width: f32,
) -> &'a crate::components::markdown_text::Rendered {
    state.rendered.rendered(
        key,
        source,
        look.renderer,
        &look.font_family,
        look.font_size * 0.9,
        colours(look),
        width.max(24.0),
        look.highlighter,
    )
}

/// The same, as a height, which is what the measuring pass wants.
///
/// **In unscaled points**, because that is what a [`Piece`] holds — `Piece::height` adds `PAD_Y` and
/// `TOOL_ROW` to it unscaled and every caller multiplies the total by `Look::scale` afterwards, and
/// `Piece::Picture` already divides by the scale for the same reason. The rendered markdown measures
/// itself in the pixels it will be drawn at, so it was the one quantity going into that arithmetic
/// still scaled, and it was therefore multiplied by the scale a second time. At 16 pt the scale is 1
/// and nothing is wrong; at 41 pt a bubble came out two and a half times too tall.
fn rendered_height(
    state: &mut PaneState,
    look: &Look<'_>,
    key: &str,
    source: &str,
    width: f32,
) -> f32 {
    rendered(state, look, key, source, width).height() / look.scale()
}

/// How wide `text` wants to be, up to `most`.
///
/// egui's own layout rather than `unluminous_core`'s, because this is asked before the markdown has been
/// rendered and the answer only has to be within a point or two — the caller adds slack so the two
/// cannot disagree about where a line wraps.
fn measure(look: &Look<'_>, text: &str, most: f32) -> f32 {
    // The longest line's character count against the font's average advance. Laying a galley out
    // properly needs a context this is not given, and the answer only has to tell a two word question
    // from a paragraph — which is all it decides, because the caller clamps it to the share and adds
    // slack so the two layouts cannot disagree about where a line wraps.
    let longest = text.lines().map(|line| line.chars().count()).max().unwrap_or(0) as f32;
    (longest * look.font_size * 0.48).min(most)
}

/// The key one run of an answer's words is rendered and selected under.
///
/// Segment zero keeps the key every answer had before `task-2211`, so a conversation written then is
/// read back and selected exactly as it was.
pub fn words_key(message: u64, segment: usize) -> String {
    match segment {
        0 => format!("message-{message}"),
        _ => format!("message-{message}-{segment}"),
    }
}

/// The key a component an answer holds is read, remembered and drawn under.
pub fn block_key(message: u64, segment: usize) -> String {
    format!("block-{message}-{segment}")
}

/// An answer's words and components, in the order they were written.
///
/// Only an answer is read for components. A person's own question is shown as they wrote it, so a
/// question that quotes a `ui` fence to ask about it is a question rather than a chart.
pub fn segments_of(message: &Message, text: &str) -> Vec<Segment> {
    match message.role != Role::User && rich::has_blocks(text) {
        true => rich::segments(text),
        false => vec![Segment::Markdown(text.to_owned())],
    }
}

/// The words of an answer and the components in it, as pieces in the order they were written.
#[allow(clippy::too_many_arguments)]
fn said(
    message: &Message,
    text: &str,
    state: &mut PaneState,
    look: &Look<'_>,
    width: f32,
    most: f32,
    smallest: f32,
    mut ui: Option<&mut egui::Ui>,
) -> Vec<Piece> {
    let scale = look.scale();
    let mine = message.role == Role::User;
    let (pad_x, pad) = match mine {
        true => (PAD_X, PAD_Y),
        false => (0.0, 0.0),
    };
    let mut out = Vec::new();
    for (segment, one) in segments_of(message, text).iter().enumerate() {
        match one {
            Segment::Markdown(words) => {
                // The person's bubble is as wide as their words; an answer's words take the row.
                let bubble = match mine {
                    true => {
                        let natural = measure(look, words, most - pad_x * 2.0 * scale)
                            + (pad_x * 2.0 + 8.0) * scale;
                        natural.clamp(smallest.min(most), most).max(smallest.min(most))
                    }
                    false => most,
                };
                let inside = (bubble - pad_x * 2.0 * scale).max(24.0);
                let body =
                    rendered_height(state, look, &words_key(message.id, segment), words, inside);
                out.push(Piece::Words { body, segment, bubble, pad });
            }
            Segment::Block { source, finished } => {
                // Measured with the real fonts when a `Ui` is at hand, which is every frame that draws.
                // A test of the bubbles' arithmetic has none, and a component's height is not its question.
                let height = match ui.as_deref_mut() {
                    Some(ui) => {
                        let wide = width * BLOCK_SHARE;
                        block_height(message.id, segment, source, *finished, state, look, ui, wide)
                            / scale
                    }
                    None => 120.0,
                };
                out.push(Piece::Block { segment, height });
            }
        }
    }
    out
}

/// What one block reads as, read again only when its text has changed.
fn read_cached(
    state: &mut PaneState,
    key: &str,
    source: &str,
    finished: bool,
) -> Result<rich::Component, Vec<rich::Problem>> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    finished.hash(&mut hasher);
    let hash = hasher.finish();
    if let Some((known, read)) = state.parsed.get(key) {
        if *known == hash {
            return read.clone();
        }
    }
    let read = rich::read_block(source, finished);
    state.parsed.insert(key.to_owned(), (hash, read.clone()));
    read
}

/// The `rux` state the components are drawn with, made the first time one is.
///
/// Deterministic for the reason `ModelSelect::new` gives: a screenshot of the pane must be the same
/// picture on every machine.
fn blocks_rux() -> rux::RuxState {
    let theme = crate::theme::rux_theme();
    rux::RuxState::deterministic(theme)
}

/// How tall one component is at `width`, in the pixels it will be drawn at.
#[allow(clippy::too_many_arguments)]
fn block_height(
    message: u64,
    segment: usize,
    source: &str,
    finished: bool,
    state: &mut PaneState,
    look: &Look<'_>,
    ui: &mut egui::Ui,
    width: f32,
) -> f32 {
    let key = block_key(message, segment);
    let read = read_cached(state, &key, source, finished);
    // **Measured once for what it is now.** A finished conversation's heights do not change from frame
    // to frame, and measuring lays out every word in every component, so the answer is kept against the
    // text, the width, the zoom and what the component remembers, any of which moving measures again.
    let signature = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        finished.hash(&mut hasher);
        width.to_bits().hash(&mut hasher);
        look.scale().to_bits().hash(&mut hasher);
        let mut remembered: Vec<String> = state
            .blocks
            .iter()
            .filter(|(at, _)| at.starts_with(&key))
            .map(|(at, kept)| format!("{at}{kept:?}"))
            .collect();
        remembered.sort();
        remembered.hash(&mut hasher);
        hasher.finish()
    };
    if let Some((known, height)) = state.heights.get(&key) {
        if *known == signature {
            return *height;
        }
    }
    let height = measure_block(&key, &read, source, state, look, ui, width);
    state.heights.insert(key, (signature, height));
    height
}

/// Lay one component out without drawing it, which is what its height is.
fn measure_block(
    key: &str,
    read: &Result<rich::Component, Vec<rich::Problem>>,
    source: &str,
    state: &mut PaneState,
    look: &Look<'_>,
    ui: &mut egui::Ui,
    width: f32,
) -> f32 {
    let PaneState { blocks_rux: kept, rendered, blocks, scenes, still, .. } = state;
    let rux_state = kept.get_or_insert_with(blocks_rux);
    rux_state.set_zoom(look.scale());
    rux_state.set_still(*still);
    crate::theme::in_step(rux_state);
    let chrome = rux::Chrome::recording();
    let scope = ui.id();
    let mut rux = rux::Rux { ui, state: rux_state, chrome: &chrome };
    let mut kit = super::blocks::Kit {
        rux: &mut rux,
        draw: false,
        look,
        rendered,
        states: blocks,
        scenes,
        acts: Vec::new(),
        scope,
    };
    super::blocks::block(&mut kit, read, source, key, Pos2::ZERO, width)
}

/// Draw one component into `rect`, in a `rux` layer of its own, and say what was pressed.
#[allow(clippy::too_many_arguments)]
fn block_show(
    message: u64,
    segment: usize,
    source: &str,
    finished: bool,
    state: &mut PaneState,
    look: &Look<'_>,
    ui: &mut egui::Ui,
    rect: Rect,
) -> Vec<Act> {
    let key = block_key(message, segment);
    let read = read_cached(state, &key, source, finished);
    let PaneState { blocks_rux: kept, rendered, blocks, scenes, still, .. } = state;
    let rux_state = kept.get_or_insert_with(blocks_rux);
    rux_state.set_zoom(look.scale());
    rux_state.set_still(*still);
    crate::theme::in_step(rux_state);
    // Room round the plate for its shadow, which the layer's canvas would otherwise cut off.
    let reach = 14.0 * look.scale();
    let scope = ui.id();
    let id = scope.with(("agent-chat-block-layer", &key));
    // A component takes a press over its own rectangle, for the reason `welcome::show` gives: on a
    // canvas a press nothing in a node takes falls through and chooses no node. Added before the
    // component's own controls, so they win the points they are drawn on.
    let _ = ui.interact(rect, id.with("ground"), egui::Sense::click());
    rux::layer(ui, rux_state, id, rect.expand(reach), |rux| {
        let mut kit = super::blocks::Kit {
            rux,
            draw: true,
            look,
            rendered,
            states: blocks,
            scenes,
            acts: Vec::new(),
            scope,
        };
        super::blocks::block(&mut kit, &read, source, &key, rect.min, rect.width());
        kit.acts
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2235`: a run card names the command a call ran, which is what the design's card shows.
    #[test]
    fn a_tool_is_named_by_the_command_it_ran() {
        let ran = ToolCall::new("a", "Bash", r#"{"command":"cargo build --release\nmore"}"#);
        assert_eq!(tool_title(&ran), "cargo build --release");
        let read = ToolCall::new("b", "Read", r#"{"file_path":"src/main.rs"}"#);
        assert_eq!(tool_title(&read), "Read src/main.rs");
        let own = ToolCall::new("c", "unluminous_status", "{}");
        assert_eq!(tool_title(&own), "status");
        assert_eq!(took(1400), "1.4s");
        assert_eq!(took(178_000), "2m 58s");
    }

    #[test]
    fn a_bubble_is_as_wide_as_what_is_in_it_up_to_its_share() {
        // A short question drawn at eighty per cent of the pane would not read as a short question,
        // and a long answer has to be allowed the width.
        let settings = crate::settings::Settings::new();
        let renderer = crate::services::text_renderer::TextRenderer::new();
        let look = Look::of(&settings, &renderer);
        let short = measure(&look, "Why?", 400.0);
        let long = measure(&look, &"a word ".repeat(60), 400.0);
        assert!(short < 60.0, "{short}");
        assert_eq!(long, 400.0, "a long line takes the whole share");
    }

    #[test]
    fn a_short_answer_still_has_room_for_its_words_at_a_large_font() {
        // task-1811. The bubble a one-character answer gets is the smallest one allowed, and `show`
        // takes scaled padding back out of it before laying the markdown into what is left. While
        // the smallest was sixty *unscaled* points, that subtraction took more out than there was:
        // at 41 pt the padding is 61 points, `inside` fell to its 24 point floor, and the answer was
        // laid out into less room than one glyph of a 37 pt font needs. The bubble drew; the word in
        // it did not, and neither `agent-chat state` nor `agent-chat last` could see that - both
        // report the answer quite happily. It cost the product video its Agent-Chat beat.
        //
        // Asked of `pieces` rather than of the arithmetic, and at five sizes rather than one,
        // because the whole reason this shipped is that every existing test and all 363 screenshots
        // are taken at 16 pt, where `Look::scale` is 1 and a scaled and an unscaled point are the
        // same thing.
        let renderer = crate::services::text_renderer::TextRenderer::new();
        let mut answer = Message::new(1, Role::Assistant);
        answer.parts.push(Part::Text("7".to_string()));

        for font_size in [16.0_f32, 24.0, 34.0, 41.0, 64.0] {
            let mut settings = crate::settings::Settings::new();
            settings.font_size = font_size;
            let look = Look::of(&settings, &renderer);
            let mut state = PaneState::default();
            let (_, bubble, _, _) = pieces(&answer, &mut state, &look, 900.0, false, true, None);

            // What `show` will lay the words into, computed the way `show` computes it.
            let inside = bubble - PAD_X * 2.0 * look.scale();
            assert!(
                inside > font_size * 0.9,
                "at {font_size} pt the bubble is {bubble} points wide and leaves {inside} inside,                  which is less than one glyph of the {} pt the body is drawn at",
                font_size * 0.9,
            );
        }
    }

    #[test]
    fn a_piece_knows_its_own_height_and_a_collapsed_one_is_just_its_header() {
        assert_eq!(Piece::Tool { index: 0, body: 0.0 }.height(), TOOL_ROW);
        assert_eq!(Piece::Tool { index: 0, body: 30.0 }.height(), TOOL_ROW + 30.0);
        assert_eq!(Piece::Thinking { body: 0.0 }.height(), THINKING_ROW);
        assert_eq!(
            Piece::Words { body: 10.0, segment: 0, bubble: 100.0, pad: PAD_Y }.height(),
            10.0 + PAD_Y * 2.0
        );
        // An answer has no bubble and so no padding (`task-2235`).
        assert_eq!(Piece::Words { body: 10.0, segment: 0, bubble: 100.0, pad: 0.0 }.height(), 10.0);
        assert_eq!(Piece::Speaker.height(), SPEAKER);
        assert_eq!(Piece::Block { segment: 1, height: 42.0 }.height(), 42.0);
    }

    /// `task-2060`: a tool call's arguments are laid out and fenced as JSON so a plugin colours them.
    #[test]
    fn a_tools_arguments_are_laid_out_and_named_as_json() {
        // One long line of JSON becomes several lines, and the fence names the language — which is
        // what makes the JSON plugin colour its strings, its numbers and its three literals.
        assert_eq!(fenced("  {\"a\":1} "), "```json\n{\n  \"a\": 1\n}\n```");
        assert_eq!(fenced("[1,2]"), "```json\n[\n  1,\n  2\n]\n```", "an array is laid out too");
        // Anything that is not JSON arrives as it is, in a fence naming nothing: a shell command's
        // output is not JSON and colouring it as though it were would colour it wrongly.
        assert_eq!(fenced(" total 12\ndrwx "), "```\ntotal 12\ndrwx\n```");
        assert_eq!(
            fenced("{\"path\": "),
            "```\n{\"path\":\n```",
            "arguments cut off mid-stream are not JSON and are shown as they came"
        );
        // A bare string or a number is JSON and is already one line, so it is left alone rather than
        // fenced as a document of one word.
        assert_eq!(fenced("\"ok\""), "```\n\"ok\"\n```");
        assert_eq!(laid_out_json("12"), None);
    }

    /// A block of words sits in the middle of the room reserved for it, not at the top of it.
    ///
    /// `task-2060`: *"The text in a message isnt perfectly vertically aligned."* A line is taller
    /// than its glyphs and all of the extra leading is added below the baseline, so a block drawn at
    /// the top of a box padded equally above and below reads as high by half that leading.
    #[test]
    fn a_rendered_block_says_how_far_down_to_draw_it_for_its_letters_to_be_centred() {
        let renderer = crate::services::text_renderer::TextRenderer::new();
        let colors = crate::components::markdown_text::Colors {
            text: Color32::WHITE,
            strong: Color32::WHITE,
            code: Color32::GREEN,
            link: Color32::BLUE,
            quiet: Color32::GRAY,
            rule: Color32::DARK_GRAY,
        };
        let made = crate::components::markdown_text::render(
            "A line of prose.",
            &renderer,
            "sans-serif",
            14.0,
            colors,
            400.0,
            None,
        );
        let (top, bottom) = made.ink();
        assert!(top >= 0.0 && bottom <= made.height(), "the ink is inside the block");
        assert!(bottom < made.height(), "a line is taller than the letters on it");
        assert!(made.centring() > 0.0, "so there is room under them to move into");
        // And never past halfway, which is what the shift is: half the air that was all at the
        // bottom, moved to the top.
        assert!(made.centring() <= (made.height() - bottom), "{}", made.centring());
    }

    /// A queued question is an ordinary bubble with one quiet line under it. `task-2060`.
    #[test]
    fn a_queued_question_carries_one_extra_row_and_nothing_else_moves() {
        let renderer = crate::services::text_renderer::TextRenderer::new();
        let settings = crate::settings::Settings::new();
        let look = Look::of(&settings, &renderer);
        let mut question = Message::new(1, Role::User);
        question.parts.push(Part::Text("Are you there?".to_string()));

        let mut state = PaneState::default();
        let (plain, bubble, _, _) = pieces(&question, &mut state, &look, 400.0, false, true, None);
        let (waiting, waiting_bubble, _, _) =
            pieces(&question, &mut state, &look, 400.0, true, true, None);
        assert_eq!(waiting.len(), plain.len() + 1);
        assert_eq!(waiting.last().copied(), Some(Piece::Queued));
        assert_eq!(bubble, waiting_bubble, "the bubble is the same bubble");
    }
}
