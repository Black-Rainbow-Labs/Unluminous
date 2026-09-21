//! One row of the conversation: a bubble, its pictures, its tool blocks and its failure.
//!
//! `ChatMessage.module.css` is what this is measured against. A message from the person is right
//! aligned and **raised**; one from the model is left aligned and **pressed**; each has the corner
//! nearest its own side squared off to six points, which is the detail that makes a column of
//! bubbles read as a conversation rather than as a list.
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

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use unluminous_chat::model::{Message, Part, Role, ToolCall};

use super::Act;
use crate::services::agent_chat::PaneState;
use crate::services::plugin_ui::Look;
use crate::services::vello_canvas::{Fill, Lift};
use crate::theme::crisp::CrispPainter;
use crate::theme::icon;

/// A bubble's own padding, from `.messageWrapper`.
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 10.0;
/// A bubble's corner radius, and the squared-off one nearest its own side.
const RADIUS: f32 = 14.0;
const CORNER: f32 = 5.0;
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
const MODEL_SHARE: f32 = 0.94;
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
    /// The `<think>` block's header, and its body when it is open.
    Thinking {
        body: f32,
    },
    /// The words, as markdown.
    Words {
        body: f32,
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
            Self::Thinking { body } => THINKING_ROW + body,
            Self::Words { body } => body + PAD_Y * 2.0,
            Self::Picture { height, .. } => height,
            Self::Tool { body, .. } => TOOL_ROW + body,
            Self::Failure { body } => body + PAD_Y * 2.0,
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
) -> (Vec<Piece>, f32, f32, String) {
    let scale = look.scale();
    let mine = message.role == Role::User;
    let share = match mine {
        true => USER_SHARE,
        false => MODEL_SHARE,
    };
    let most = width * share;
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
    let inside = (bubble - PAD_X * 2.0 * scale).max(24.0);
    // **A tool block, a failure and the thinking are as wide as the row allows, whatever the words
    // above them are.** They are reports rather than speech: sized to their own message, a tool called
    // from a two word answer came out two words wide, with its own caret clipped off the end of it.
    let block = width * BLOCK_SHARE;
    let in_block = (block - 24.0 * scale).max(24.0);

    let mut out = Vec::new();
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
        let body = rendered_height(state, look, &format!("message-{}", message.id), &text, inside);
        out.push(Piece::Words { body });
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
    for (index, tool) in message.tools.iter().enumerate() {
        let body = match state.opened_tools.contains(&tool.id) || tool.is_running() {
            true => tool_body_height(state, look, tool, in_block),
            false => 0.0,
        };
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
pub fn shape(
    message: &Message,
    state: &mut PaneState,
    look: &Look<'_>,
    width: f32,
    queued: bool,
) -> Shape {
    let scale = look.scale();
    let (pieces, bubble, block, text) = pieces(message, state, look, width, queued);
    let height = pieces.iter().map(|piece| piece.height() * scale).sum::<f32>()
        + (pieces.len().saturating_sub(1) as f32) * 6.0 * scale;
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
    let mut pen = area.top();
    for piece in pieces {
        let height = piece.height() * scale;
        // A bubble is as wide as its words and sits on its own side; a report is as wide as the row
        // and always starts at the left.
        let wide =
            matches!(piece, Piece::Tool { .. } | Piece::Failure { .. } | Piece::Thinking { .. });
        let width = match piece {
            Piece::Picture { width, .. } => width,
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
            Piece::Thinking { body } => {
                acts.extend(thinking(message, state, ui, look, rect, body > 0.0))
            }
            Piece::Words { .. } => {
                bubble(ui, look, rect, mine);
                let inside = Rect::from_min_size(
                    rect.min + Vec2::new(PAD_X * scale, PAD_Y * scale),
                    Vec2::new(
                        rect.width() - PAD_X * 2.0 * scale,
                        rect.height() - PAD_Y * 2.0 * scale,
                    ),
                );
                acts.extend(words(message, state, ui, look, rect, inside, &said, mine));
            }
            Piece::Picture { index, .. } => picture(message, state, ui, look, rect, index),
            Piece::Tool { index, body } => {
                if let Some(tool) = message.tools.get(index) {
                    acts.extend(tool_block(tool, state, ui, look, rect, body > 0.0));
                }
            }
            Piece::Failure { .. } => failure(message, state, ui, look, rect),
            Piece::Queued => queued_note(ui, look, rect, mine),
        }
        pen += height + 6.0 * scale;
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
) -> Vec<Act> {
    use crate::services::agent_chat::{MessageMenu, Selected};

    let scale = look.scale();
    let mut acts = Vec::new();
    let key = format!("message-{}", message.id);
    let code = code_colours(look);
    // **The words are selectable**, which is `task-2060`: a message is text to read, and reading
    // includes taking a copy of a part of it. `click_and_drag` rather than `hover` for the reason
    // the Markdown preview senses both - see `UnluminousApp::show_markdown_preview`.
    let response =
        ui.interact(rect, ui.id().with(("agent-chat-bubble", message.id)), Sense::click_and_drag());
    // Read out of the state before the rendered markdown is borrowed from it.
    let was = match state.selection {
        Some(selected) if selected.message == message.id => Some(selected.range),
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
        state.selection = Some(Selected { message: message.id, range: selection });
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
        if crate::components::controls::icon_button(ui, at, "Copy message", icon::copy) {
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

/// The bubble itself: raised for the person, pressed for the model.
fn bubble(ui: &mut egui::Ui, look: &Look<'_>, rect: Rect, mine: bool) {
    let scale = look.scale();
    let radius = RADIUS * scale;
    let squared = (CORNER * scale) as u8;
    // The corner nearest its own side is squared off, which is `.messageWrapper`'s
    // `border-top-left-radius: 6px` and its mirror for a message from the person.
    let corners = match mine {
        true => CornerRadius { nw: radius as u8, ne: squared, sw: radius as u8, se: radius as u8 },
        false => CornerRadius { nw: squared, ne: radius as u8, sw: radius as u8, se: radius as u8 },
    };
    if look.chrome.is_recording() {
        // **The squared corner is the shape, not a patch over it.** `Chrome` used to take one radius, so
        // this squared the corner by painting a flat rectangle of `board_card` across it — flat, so it
        // carried none of the inset shadow the rest of the bubble has, and it read as a lighter block
        // sitting proud of the top left of every answer. `task-1771` reported exactly that. Four radii go
        // down to `vello_cpu` now and the corner is drawn once, in the same pass as the shadows, which is
        // the only way the two can agree.
        let corners = crate::services::vello_canvas::Corners::from(corners);
        match mine {
            // **Raised for the person, pressed for the model**, at `Lift::Medium` rather than
            // `Lift::Small`: the reference's shadow pairs are broad and soft, and at `Small` a bubble
            // came out with a one point dark edge and almost no light side — dark and rounded rather
            // than neumorphic.
            true => look.chrome.raised(
                rect,
                corners,
                Fill::Solid(look.palette.board_card),
                Lift::Medium,
            ),
            false => look.chrome.sunken(rect, corners, look.palette.board_card, Lift::Medium),
        }
    } else {
        ui.painter().rect(
            rect,
            corners,
            look.ground(look.palette.board_card),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
    }
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
    let Some(texture) = state.pictures.get(&key) else {
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
    if look.chrome.is_recording() {
        look.chrome.raised(drawn, 10.0 * scale, Fill::Solid(look.palette.board_well), Lift::Small);
    }
    painter_in(ui, rect).image(
        texture.id(),
        drawn,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
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
    icon::clock(&painter, Pos2::new(right - width - 9.0 * scale, rect.center().y), tint);
}

/// One mark a tool block draws in its ring, at the pane's own scale.
type Mark = fn(&egui::Painter, Pos2, Color32, f32);

/// One tool call: a well with a round icon, the command's name, how long it took, and a caret.
///
/// `StatusTopicEl`'s own shape, in Unluminous's palette: open while it is running and collapsed once it
/// has finished, which is that component's `isTopicOpen` rule.
fn tool_block(
    tool: &ToolCall,
    state: &mut PaneState,
    ui: &mut egui::Ui,
    look: &Look<'_>,
    rect: Rect,
    open: bool,
) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    if look.chrome.is_recording() {
        look.chrome.sunken(rect, 10.0 * scale, look.palette.board_well, Lift::Small);
    } else {
        ui.painter().rect(
            rect,
            CornerRadius::same((10.0 * scale) as u8),
            look.ground(look.palette.board_well),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
    }
    let head = Rect::from_min_size(rect.min, Vec2::new(rect.width(), TOOL_ROW * scale));
    let response = ui.interact(head, ui.id().with(("agent-chat-tool", &tool.id)), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Tool: {}", tool.name))
    });
    let painter = painter_in(ui, rect);
    // The round raised disc, which is `.topicIcon`, with a ring round it in the colour of the state:
    // the board's blue while it runs, git's added green when it worked, the close button's red when
    // it did not.
    //
    // **A wrench rather than a tick.** `task-2060`: *"The checkbox icon for tool call is not good. it
    // should be a wrench icon, and green colored outline."* A tick says *ticked off*, and a tool call
    // is not an item on a checklist - it is the model reaching for a tool. A failure keeps the cross,
    // because there the mark is the whole report and a wrench would say only what kind of thing had
    // gone wrong.
    let disc = Pos2::new(head.left() + 14.0 * scale, head.center().y);
    // **The mark takes the pane's scale, because the ring round it does.** Most icons in this window
    // are drawn at one size whatever the type is, and that is right where the thing beside them is a
    // row of the same height at every zoom. Here the ring is `9.0 * scale`, so a mark that did not
    // scale sat in the middle of it like something that had come loose. `task-2060` drew one at 1.8.
    let (tint, drawing): (Color32, Mark) = match (tool.is_running(), tool.failed) {
        (true, _) => (look.palette.board_accent, icon::wrench_at),
        (false, true) => (crate::theme::color::close(), icon::cross_at),
        (false, false) => (crate::theme::color::git_added(), icon::wrench_at),
    };
    if look.chrome.is_recording() {
        look.chrome.raised(
            Rect::from_center_size(disc, Vec2::splat(18.0 * scale)),
            9.0 * scale,
            Fill::Solid(look.palette.board_card),
            Lift::Small,
        );
    }
    // Drawn whether or not the decoration is recording: the ring is the **state**, and a state that
    // only appeared with `plugins.chrome` on would be a state half the windows cannot see.
    painter.circle_stroke(disc, 9.0 * scale, Stroke::new(1.2 * scale, tint));
    drawing(&painter, disc, tint, scale);
    let mut pen = disc.x + 16.0 * scale;
    // **Centred on the row rather than measured up from its middle by a fraction of the font.**
    // `Align2::LEFT_TOP` with a guessed offset put both of these a point or so high, and a point at
    // sixteen points is the difference `task-2060` reports beside a mark that really is centred.
    let name_font = egui::FontId::proportional(look.font_size * 0.82);
    let name_width = painter
        .layout_no_wrap(tool.name.clone(), name_font.clone(), look.palette.text_control)
        .size()
        .x;
    painter.crisp_text(
        Pos2::new(pen, head.center().y),
        egui::Align2::LEFT_CENTER,
        &tool.name,
        name_font,
        look.palette.text_control,
    );
    pen += name_width + 8.0 * scale;
    let said = match tool.took {
        Some(took) => format!("{:.2}s", took as f32 / 1000.0),
        None => "running".to_owned(),
    };
    painter.crisp_text(
        Pos2::new(pen, head.center().y),
        egui::Align2::LEFT_CENTER,
        said,
        egui::FontId::monospace(look.font_size * 0.68),
        look.palette.text_faint,
    );
    icon::disclosure_at(
        &painter,
        Pos2::new(head.right() - 12.0 * scale, head.center().y),
        open,
        look.palette.text_faint,
        scale,
    );
    if response.clicked() {
        acts.push(Act::ToggleTool(tool.id.clone()));
    }
    if open {
        let inside = Rect::from_min_max(
            Pos2::new(rect.left() + 12.0 * scale, head.bottom()),
            Pos2::new(rect.right() - 12.0 * scale, rect.bottom() - 4.0 * scale),
        );
        let arguments = fenced(&tool.arguments);
        let code = code_colours(look);
        let made =
            rendered(state, look, &format!("tool-args-{}", tool.id), &arguments, inside.width());
        let used = made.height();
        crate::components::markdown_text::show_with(
            ui,
            inside,
            made,
            look.renderer,
            0.0,
            Some(code),
        );
        if let Some(answer) = &tool.answer {
            let below =
                Rect::from_min_max(Pos2::new(inside.left(), inside.top() + used), inside.max);
            let text = fenced(answer);
            let made =
                rendered(state, look, &format!("tool-answer-{}", tool.id), &text, below.width());
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
    acts
}

/// What the server said when it refused, in its own words.
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
    if look.chrome.is_recording() {
        look.chrome.sunken(rect, 10.0 * scale, look.palette.board_well, Lift::Small);
    } else {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same((10.0 * scale) as u8),
            look.palette.board_well,
        );
    }
    // The one red edge, which is the same mark a failed run wears in the run tile.
    painter_in(ui, rect).rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(2.5 * scale, rect.height())),
        CornerRadius::same(1),
        crate::theme::color::close(),
    );
    let inside = rect.shrink2(Vec2::new(PAD_X * scale, PAD_Y * scale));
    let key = format!("failure-{}", message.id);
    let code = code_colours(look);
    let made = rendered(state, look, &key, said, inside.width());
    crate::components::markdown_text::show_with(ui, inside, made, look.renderer, 0.0, Some(code));
}

/// The colours markdown is rendered in here.
fn colours(look: &Look<'_>) -> crate::components::markdown_text::Colors {
    crate::components::markdown_text::Colors {
        // Brighter than the dim body text the first version used: in the reference the depth does the
        // separating and the words are the bright thing on the surface.
        text: look.palette.text,
        strong: look.palette.text_strong,
        // **Blue, which is the reference's own `--accent-blue`.** It was the mint `attached` before,
        // which was the most conspicuously wrong colour in the pane: mint means *running* everywhere
        // else here, on a tool block and on the state dot.
        code: look.palette.board_accent,
        link: look.palette.accent,
        quiet: look.palette.text_dim,
        rule: look.palette.divider,
    }
}

/// What a code background is drawn in: a fence's pressed panel and an inline chip.
///
/// Both are colours Unluminous already has. The panel is the well every field in the window is, and the
/// chip is `CODE_CHIP`, which is what the Markdown preview already paints behind inline code.
fn code_colours(look: &Look<'_>) -> crate::components::markdown_text::CodeColors {
    crate::components::markdown_text::CodeColors {
        panel: look.palette.board_well,
        chip: crate::theme::color::code_chip(),
        radius: 6,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            let (_, bubble, _, _) = pieces(&answer, &mut state, &look, 900.0, false);

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
        assert_eq!(Piece::Words { body: 10.0 }.height(), 10.0 + PAD_Y * 2.0);
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
        let (plain, bubble, _, _) = pieces(&question, &mut state, &look, 400.0, false);
        let (waiting, waiting_bubble, _, _) = pieces(&question, &mut state, &look, 400.0, true);
        assert_eq!(waiting.len(), plain.len() + 1);
        assert_eq!(waiting.last().copied(), Some(Piece::Queued));
        assert_eq!(bubble, waiting_bubble, "the bubble is the same bubble");
    }
}
