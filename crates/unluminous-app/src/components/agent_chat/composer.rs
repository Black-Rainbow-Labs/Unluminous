//! The composer: the pill of tools, what has been used, the attachments and the prompt.
//!
//! `ChatComposer.module.css`, `ChatComposerToolbar.module.css`, `ChatTool.module.css` and
//! `PromptInput.module.css` are what this is measured against. The pill is a **pressed** well holding
//! round buttons that are flat until they are switched on, at which point each is **raised** and
//! wears its own accent — which is `ChatTool`'s whole design, and the reason a toolbar of six reads
//! as a row of states rather than as a row of buttons.
//!
//! ## The send button is the stop button
//!
//! While an answer is arriving there is nothing to send and something to stop, so the one disc at the
//! end of the prompt is whichever applies. That is `PromptInput`'s own `onStopButtonPress`, and it is
//! Unluminous's rule about a control that cannot apply being absent rather than present and refusing.
//!
//! ## There is no context meter and no token count
//!
//! The page this is modelled on draws a bar of how much of the model's context has been used, and it
//! can: its own server knows the window the model was loaded with. Unluminous does not — a URL and a model
//! name say nothing about a context length — so a bar here would be a fraction of a number nobody
//! measured. A row of the tokens in and out stood in its place until `task-2193`: *"The in/out is not
//! needed."* It was a number somebody had to learn to ignore, under the one control they came to use.
//! `plugins view agent-chat` still answers it, for whoever is counting.
//!
//! ## The words start at the top of the well
//!
//! `task-2193`: *"The prompt input is not formatting well when I add new lines. It has a giant space above
//! the text as I enter new lines. It should vertically align at the top."* The box was handed a strip one
//! line tall centred in the well, and a box with four lines in it laid out around that strip — so the well
//! grew downwards while the words grew both ways. The first line now sits where a one line prompt has it,
//! and every line after it goes below, so the field grows the way a page does.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use super::Act;
use crate::services::agent_chat::Parts;
use crate::services::plugin_ui::Look;
use crate::services::vello_canvas::{Fill, Lift};
use crate::theme::icon;

/// One button in the tool pill: its name, its icon, whether it is switched on, the accent it wears
/// while it is on, and the act pressing it performs.
type ToolButtonRow = (&'static str, fn(&egui::Painter, Pos2, Color32), bool, Color32, Act);

/// The pill of tools, and one round button in it.
const PILL: f32 = 28.0;
const TOOL: f32 = 22.0;
/// A thumbnail of an attached picture, and the row it sits in.
const THUMB: f32 = 38.0;
/// The prompt well when there is one line in it, and the most it grows to.
///
/// **Forty-eight**, which leaves eight points above and below the send disc. It was sixty-eight, after
/// the reference's forty-two read as cramped beside a thirty-two point disc, and `task-2200` reported the
/// sixty-eight as *"too much padding on the prompt input"*: one line of text sat in a well three lines
/// tall.
const PROMPT: f32 = 48.0;
const PROMPT_ROWS: usize = 6;
/// The disc at the end of the prompt.
const SEND: f32 = 32.0;
/// How far the disc's edge is from the well's right edge. The well's corners are eighteen points round,
/// so at five the disc sat against the curve; `task-2200` reported it as too close to the right.
const SEND_INSET: f32 = 10.0;
/// Between the composer's rows.
const GAP: f32 = 8.0;

/// How tall the composer is, which the pane measures back from its own bottom.
pub fn height(parts: &Parts<'_>, look: &Look<'_>, width: f32) -> f32 {
    let scale = look.scale();
    let mut total = PILL + GAP + prompt_height(parts, look, width);
    if !parts.attachments.is_empty() {
        total += THUMB + GAP * 0.5;
    }
    total * scale
}

/// How tall the prompt well is, in unscaled points: one line, grown by one line for each line typed.
///
/// A line is as tall as the last frame measured it, so the well grows by exactly what the box inside it
/// grows by and there is no slack to collect above or below the words.
fn prompt_height(parts: &Parts<'_>, look: &Look<'_>, width: f32) -> f32 {
    let per_line = match parts.state.prompt_row > 0.0 {
        true => parts.state.prompt_row / look.scale(),
        false => look.font_size * 0.9 * 1.3 / look.scale(),
    };
    let lines =
        prompt_lines(parts.draft, look, width).max(parts.state.prompt_lines).clamp(1, PROMPT_ROWS);
    PROMPT + (lines.saturating_sub(1) as f32) * per_line
}

/// What the field says while it is empty, in whichever form fits on one line.
///
/// `long_is` is how wide [`LONG_HINT`] really lays out at the size it will be drawn — measured by the
/// caller, which has the fonts, so this is arithmetic a test can check with no window.
///
/// **The placeholder names the two ways a picture goes up**, because the button that used to do it is
/// gone: `task-1848` asked for it to go and for drag and drop and paste to be the routes, and a control
/// removed with nothing said in its place is a feature nobody finds. It says it only while nothing is
/// attached, so it is a hint rather than a label.
///
/// **And only where there is room for it.** A hint too wide for its field wraps, and a box that grows to
/// hold three wrapped lines is taller than the well measured for one — so at a chat node's smallest size
/// the last line of it was drawn below the node's own bottom edge. `task-1914`'s sweep found that. Where
/// the long form does not fit, the short one is the whole of what somebody needs.
fn hint(long_is: f32, width: f32, nothing_attached: bool) -> &'static str {
    if !nothing_attached {
        return SHORT_HINT;
    }
    match long_is <= width - 8.0 {
        true => LONG_HINT,
        false => SHORT_HINT,
    }
}

/// The long form, which names the two ways a picture goes up.
const LONG_HINT: &str = "Ask anything… or drop or paste a picture";
/// The short form, for a field the long one would wrap in.
const SHORT_HINT: &str = "Ask anything…";

/// How many lines the draft takes at roughly the width of the field.
///
/// **One reckoning, read twice**, and that is what makes the box sit in the middle of its well. The
/// well is measured from this by [`prompt_height`], and the box inside it is asked for exactly this
/// many rows by [`prompt`], so `Ui::put` centres a box that is the height of the text it holds.
///
/// Two answers to it is what `task-1914` reported as *"the placeholder text isn't vertically
/// centered"*: the box used to be asked for however many rows happened to **fit** in the well, which
/// at the well's own one-line height is two — and egui lays text out at the top of the box it is
/// given, so an empty field drew its hint against its top edge with a row of nothing under it.
///
/// It counts a wrap at roughly the width of the field rather than laying the text out, because the
/// well's height has to be known before anything is drawn. `PROMPT_ROWS` is where it stops growing.
fn prompt_lines(draft: &str, look: &Look<'_>, width: f32) -> usize {
    let across = ((width - SEND - 40.0) / (look.font_size * 0.48)).max(8.0);
    // `split` rather than `lines`, because `lines` does not count the empty line after a final line
    // break — and that empty line is exactly where the caret is the moment `Shift+Enter` is pressed.
    draft
        .split('\n')
        .map(|line| ((line.chars().count() as f32 / across).ceil() as usize).max(1))
        .sum::<usize>()
        .clamp(1, PROMPT_ROWS)
}

/// Draw the composer and say what was pressed.
pub fn show(mut parts: Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let mut pen = area.top();

    acts.extend(pill(
        &parts,
        ui,
        look,
        Rect::from_min_size(Pos2::new(area.left(), pen), Vec2::new(area.width(), PILL * scale)),
    ));
    pen += (PILL + GAP) * scale;

    if !parts.attachments.is_empty() {
        acts.extend(thumbnails(
            &mut parts,
            ui,
            look,
            Rect::from_min_size(
                Pos2::new(area.left(), pen),
                Vec2::new(area.width(), THUMB * scale),
            ),
        ));
        pen += (THUMB + GAP * 0.5) * scale;
    }

    let well = Rect::from_min_max(Pos2::new(area.left(), pen), area.max);
    acts.extend(prompt(parts, ui, look, well));
    acts
}

/// The pressed pill of round buttons.
fn pill(parts: &Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    // Two, and each is a **state** rather than a command: whether the model may drive the window, and a
    // picture waiting to go up with the next message. There was a third, whether the answer arrives a
    // token at a time, drawn as a play triangle; `task-2193` asked what it was for and asked for the
    // answer always to stream, so it is gone. That is `ChatTool`'s own design — a pill of states reads at a glance where a pill of
    // buttons does not. Anything that is not a state is elsewhere: new and the history are in the
    // header, and stop is the send button. The history had a second button here and it was taken
    // away, because two controls doing one thing in one pane is one too many.
    //
    // **The tools switch is absent when the row is a command-line agent**, which is the absent-control
    // rule rather than tidiness: `claude` and `codex` bring their own tools, so handing them Unluminous's
    // would be offering a switch that does nothing. What is left is the attachment, which means the same
    // thing either way.
    let an_agent = parts.configuration.provider().is_some_and(|one| one.is_a_program());
    let mut tools: Vec<ToolButtonRow> = Vec::new();
    if !an_agent {
        tools.push((
            "Unluminous tools",
            icon::terminal,
            parts.configuration.tools,
            crate::theme::color::agent(),
            Act::ToggleTools,
        ));
    }
    // **No button that opens a file dialog.** `task-1848`: "get rid of the file picker icon, just allow
    // drag and drop or paste". Both of those already work and neither needs a control drawn for it, and
    // the placeholder below says so, which is what stops a removed button being a lost feature.
    //
    // A picture that *is* attached still lights the pill, so there is something on the screen saying one
    // is waiting to go — that is what the button's lit state used to say and it has to survive it.
    if !parts.attachments.is_empty() {
        tools.push((
            "Attached picture",
            icon::image,
            true,
            look.palette.board_accent,
            // Pressing it takes the newest attachment away, which is the only thing left for it to do
            // now that it is not how one is chosen. Each attachment also has its own cross on the strip
            // above the composer; this is the quick way to undo the one just dropped.
            Act::Detach(parts.attachments.last().map(|one| one.id).unwrap_or_default()),
        ));
    }
    let buttons = tools.len();
    if buttons == 0 {
        return acts;
    }
    let width = (TOOL * buttons as f32 + 4.0 * (buttons as f32 + 1.0)) * scale;
    let pill = Rect::from_center_size(
        Pos2::new(area.center().x, area.center().y),
        Vec2::new(width, PILL * scale),
    );
    if look.chrome.is_recording() {
        look.chrome.sunken(pill, PILL * scale / 2.0, look.palette.board_well, Lift::Small);
    } else {
        ui.painter().rect(
            pill,
            CornerRadius::same((PILL * scale / 2.0) as u8),
            look.ground(look.palette.board_well),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
    }
    let mut centre = Pos2::new(pill.left() + (4.0 + TOOL / 2.0) * scale, pill.center().y);
    for (name, drawing, on, accent, act) in tools {
        let at = Rect::from_center_size(centre, Vec2::splat(TOOL * scale));
        let response =
            ui.interact(at, ui.id().with(("agent-chat-tool-button", name)), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name.to_owned())
        });
        if on {
            if look.chrome.is_recording() {
                look.chrome.raised(
                    at,
                    TOOL * scale / 2.0,
                    Fill::Solid(look.palette.board_card),
                    Lift::Small,
                );
            } else {
                ui.painter().circle_filled(centre, TOOL * scale / 2.0, look.palette.board_card);
            }
        }
        let tint = match (on, response.hovered()) {
            (true, _) => accent,
            (false, true) => look.palette.text_strong,
            (false, false) => look.palette.text_dim,
        };
        icon::scaled(&ui.painter_at(area), centre, tint, scale, drawing);
        if response.clicked() {
            acts.push(act);
        }
        centre.x += (TOOL + 4.0) * scale;
    }
    acts
}

/// The pictures waiting to go up, each with a cross that takes it off again.
fn thumbnails(parts: &mut Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let mut left = area.left() + 2.0 * scale;
    // Copied out so the pictures can be uploaded into the state while the list is walked; both are
    // borrows of different fields, which is what makes this legal at all.
    let attachments: &[crate::services::agent_chat::Attachment] = parts.attachments;
    for attachment in attachments {
        let at = Rect::from_min_size(Pos2::new(left, area.top()), Vec2::splat(THUMB * scale));
        if at.right() > area.right() {
            break;
        }
        let key = format!("attachment-{}", attachment.id);
        if !parts.state.pictures.contains_key(&key) {
            if let Ok(image) = crate::services::picture::decode_bytes(&attachment.bytes) {
                let texture = crate::services::picture::upload(
                    ui.ctx(),
                    key.clone(),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                parts.state.pictures.insert(key.clone(), texture);
            }
        }
        if look.chrome.is_recording() {
            look.chrome.raised(at, 8.0 * scale, Fill::Solid(look.palette.board_well), Lift::Small);
        } else {
            ui.painter().rect_filled(
                at,
                CornerRadius::same((8.0 * scale) as u8),
                look.palette.board_well,
            );
        }
        match parts.state.pictures.get(&key) {
            Some(texture) => {
                ui.painter_at(area).image(
                    texture.id(),
                    at.shrink(2.0),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            // A picture that will not decode still says it is attached, because it will still be
            // sent — the server is the one that decides whether it can read it.
            None => icon::scaled(
                &ui.painter_at(area),
                at.center(),
                look.palette.text_dim,
                scale,
                icon::image,
            ),
        }
        let cross = Rect::from_center_size(
            Pos2::new(at.right() - 2.0 * scale, at.top() + 2.0 * scale),
            Vec2::splat(14.0 * scale),
        );
        if crate::components::controls::icon_button_at(
            ui,
            cross,
            &format!("Take off {}", attachment.name),
            icon::cross,
            scale,
        ) {
            acts.push(Act::Detach(attachment.id));
        }
        left += (THUMB + 6.0) * scale;
    }
    acts
}

/// The prompt well: the field, and the disc that sends or stops.
fn prompt(parts: Parts<'_>, ui: &mut egui::Ui, look: &Look<'_>, area: Rect) -> Vec<Act> {
    let scale = look.scale();
    let mut acts = Vec::new();
    let radius = (area.height() / 2.0).min(18.0 * scale);
    // **Deeper than anything else in the pane**, which is `PromptInput.module.css`'s `--e-pressed`:
    // the field somebody types into is the one thing pressed furthest into the page.
    if look.chrome.is_recording() {
        look.chrome.sunken(area, radius, look.palette.board_well, Lift::Medium);
    } else {
        ui.painter().rect(
            area,
            CornerRadius::same(radius as u8),
            look.ground(look.palette.board_well),
            Stroke::new(1.0, look.palette.control_border),
            egui::StrokeKind::Inside,
        );
    }
    let busy = parts.session.is_busy();
    let ready_to_send = !parts.draft.trim().is_empty() || !parts.attachments.is_empty();
    // **Centred on the bottom line of the well.** `task-2060`: *"The send button is not vertically
    // alighned (its a bit too low)."* It was five points off the bottom of a sixty-eight point well,
    // which puts a thirty-two point disc thirteen points below the middle. With one line typed the
    // bottom line is the whole well, so the disc is in its middle; with more, the words grow down from
    // the top and the disc stays beside the last of them, which is where a person finishing a message
    // is looking.
    let middle = area.bottom() - PROMPT * scale / 2.0;
    let disc = Rect::from_center_size(
        Pos2::new(area.right() - (SEND / 2.0 + SEND_INSET) * scale, middle),
        Vec2::splat(SEND * scale),
    );
    // **While an answer is arriving there are two things to do, so there are two discs.** Stopping
    // is the red one at the end, where the one disc has always been; sending is a second one beside
    // it, and it appears only when there is something to send — which is what makes a queued
    // question reachable with the pointer as well as with `Enter`. `task-2060`.
    let second = busy && ready_to_send;
    let send_disc = match second {
        true => Rect::from_center_size(
            Pos2::new(disc.center().x - (SEND + 6.0) * scale, middle),
            Vec2::splat(SEND * scale),
        ),
        false => disc,
    };
    let field = Rect::from_min_max(
        Pos2::new(area.left() + 12.0 * scale, area.top() + 6.0 * scale),
        Pos2::new(send_disc.left() - 8.0 * scale, area.bottom() - 6.0 * scale),
    );

    parts.state.prompt_focused = false;
    if field.width() > 30.0 {
        // As many rows as there is text, so the box is the height of what is in it and `Ui::put`
        // centres it in the well. See [`prompt_lines`].
        let rows = prompt_lines(parts.draft, look, area.width());
        // How wide the long hint really is, measured rather than guessed at. See [`hint`].
        let measured = ui.ctx().fonts_mut(|fonts| {
            let font = egui::FontId::proportional(look.font_size * 0.9);
            fonts.layout_no_wrap(LONG_HINT.to_owned(), font, Color32::PLACEHOLDER).size().x
        });
        let prompt_id = ui.id().with("agent-chat-prompt");
        // **The strip is measured at the size the box really sets its text in** — `task-2004`. The
        // pair without `_at` measures a single row at a fraction of the field's own height, and this
        // well is as tall as the draft in it, so it would have asked for letters half the height of a
        // four line message.
        let prompt_font = egui::FontId::proportional(look.font_size * 0.9);
        // **The box starts where a one line prompt's line is and grows downwards** — see the module
        // comment. The first line is centred in the well's first `PROMPT` points, and the box is laid
        // out top down inside the rest, so a second line goes under the first rather than pushing it up.
        let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(&prompt_font));
        parts.state.prompt_row = row;
        let first = area.top() + ((PROMPT * scale - row) / 2.0).max(4.0 * scale);
        let words = Rect::from_min_max(
            Pos2::new(field.left(), first),
            Pos2::new(field.right(), field.bottom().max(first + row)),
        );
        crate::components::controls::claim_the_field(ui, field, prompt_id, "Prompt field");
        let mut inside = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(words)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        // **The text box is cut to the field and scrolls inside it** (`task-2200`: *"prompt input text can
        // escape out of the input field ... even the cursor blinker and new lines are escaping"*). The well
        // stops growing at `PROMPT_ROWS` lines, and the box inside it went on growing past that and past any
        // line the wrap estimate missed, drawing its words and its caret below the well and over the pane's
        // edge. In a scrolling area as tall as the field, a long draft scrolls and egui keeps the caret in view.
        inside.set_clip_rect(field.intersect(ui.clip_rect()));
        let response = egui::ScrollArea::vertical()
            .id_salt("agent-chat-prompt-scroll")
            .max_height(words.height())
            .auto_shrink([false, true])
            .show(&mut inside, |inside| {
                inside.add(
                    egui::TextEdit::multiline(parts.draft)
                        .id(prompt_id)
                        .frame(egui::Frame::NONE)
                        // **The placeholder names the two ways a picture goes up**, because the button that used
                        // to do it is gone: `task-1848` asked for it to go and for drag and drop and paste to be
                        // the routes. A control removed with nothing said in its place is a feature nobody finds.
                        // It says it only while nothing is attached, so it is a hint rather than a label.
                        .hint_text(crate::components::controls::placeholder(
                            hint(measured, field.width(), parts.attachments.is_empty()),
                            &prompt_font,
                            look.palette.text_faint,
                        ))
                        .desired_width(field.width())
                        .desired_rows(rows)
                        .font(prompt_font.clone())
                        .text_color(look.palette.text),
                )
            })
            .inner;
        // How many lines the box really took, for the next frame's well. See `PaneState::prompt_lines`.
        parts.state.prompt_lines =
            ((response.rect.height() / row.max(1.0)).round() as usize).max(1);
        // Named, because every control in Unluminous has a plain name and a test finds one by it. Its hint
        // text is not a name: it is what the field says when it is empty.
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Message"));
        // Recorded so the paste knows whose key press it is reading — see `PaneState::prompt_focused`.
        parts.state.prompt_focused = response.has_focus();
        // **Enter sends and Shift+Enter is a new line**, which is what the page this copies does and
        // what everybody expects of a chat.
        //
        // The modifiers are compared **for real** rather than through `consume_key`, which matches by
        // `Modifiers::matches_logically`: that only asks whether the modifiers the *pattern* names are
        // held, so a pattern of `NONE` takes `Shift+Enter` too — the trap `task-1678` recorded and
        // `task-1682` recorded again. And on Windows `Ctrl+Enter` arrives with **both** `ctrl` and
        // `command` set, which `is_none` excludes and an equality test against `Modifiers::NONE` would
        // not.
        //
        // The field has already put a new line in the draft by the time this runs, because `TextEdit`
        // reads the frame's events first. That costs nothing: `AgentChat::send` trims the end of what
        // it is given, so the line break the field added never reaches the message.
        //
        // **It sends while an answer is arriving too**, because there it queues rather than being
        // refused — see `AgentChat::send`. A chord that worked while the pane was idle and silently
        // did nothing while it was busy would be the one moment somebody most wants to add a
        // sentence. `task-2060`.
        if response.has_focus() {
            let send =
                ui.input(|input| input.key_pressed(egui::Key::Enter) && input.modifiers.is_none());
            if send {
                acts.push(Act::Send);
            }
        }
    }

    // The disc at the end of the prompt: stop while an answer is arriving, send otherwise — and
    // both while an answer is arriving and something has been typed, because then there really are
    // two things to do and the send one queues.
    let accent = (look.palette.board_accent, super::darken(look.palette.board_accent, 0.15));
    let red = (crate::theme::color::close(), crate::theme::color::close().gamma_multiply(0.75));
    if busy
        && one_disc(ui, look, area, disc, "agent-chat-stop", "Stop answering", true, red, stop_mark)
    {
        acts.push(Act::Stop);
    }
    if !busy || second {
        // Nothing to send: the disc is there but quiet, because a button that vanished as the field
        // emptied would make the field jump about while somebody was typing in it. **Inert rather
        // than gone**, which is Unluminous's rule about a control that cannot apply here.
        let colours = match ready_to_send {
            true => accent,
            false => (look.palette.board_card, look.palette.board_card),
        };
        if one_disc(
            ui,
            look,
            area,
            send_disc,
            "agent-chat-send",
            "Send",
            ready_to_send,
            colours,
            send_arrow,
        ) {
            acts.push(Act::Send);
        }
    }
    acts
}

/// One of the discs at the end of the prompt: a gradient circle with a mark on it, and whether it
/// was pressed.
///
/// One function rather than two arms of an `if`, because since `task-2060` there can be two of them
/// on the same row and a second copy of the gradient, the glow and the tint is two places to get the
/// same button wrong.
#[allow(clippy::too_many_arguments)]
fn one_disc(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    area: Rect,
    disc: Rect,
    id: &'static str,
    name: &'static str,
    enabled: bool,
    (start, end): (Color32, Color32),
    mark: fn(&egui::Painter, Pos2, Color32, f32),
) -> bool {
    let scale = look.scale();
    // **It senses a click only when it can do something**, which is Unluminous's rule about a control
    // that cannot apply.
    let sense = match enabled {
        true => Sense::click(),
        false => Sense::hover(),
    };
    let response = ui.interact(disc, ui.id().with(id), sense);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.to_owned())
    });
    if look.chrome.is_recording() {
        if enabled {
            // The glow under the primary button. The reference's own is `4px 4px 12px
            // rgba(29,79,219,0.35)`; `task-2200` reported that as too much blur round a disc this small,
            // so it is tighter and fainter: a rim of colour rather than a cloud.
            look.chrome.glow(disc, disc.width() / 2.0, start.gamma_multiply(0.28), 3.0 * scale);
        }
        look.chrome.disc(disc.center(), disc.width() / 2.0, Fill::diagonal(disc, start, end));
    } else {
        ui.painter().circle_filled(disc.center(), disc.width() / 2.0, start);
    }
    let tint = match enabled {
        // The palette's own white rather than `Color32::WHITE`: the palette is closed, and a colour
        // written out here is a colour no theme can reach.
        true => look.palette.text_strong,
        false => look.palette.text_faint,
    };
    mark(&ui.painter_at(area), disc.center(), tint, scale);
    response.clicked()
}

/// The stop square, at the shape [`one_disc`] hands its mark.
fn stop_mark(painter: &egui::Painter, centre: Pos2, tint: Color32, scale: f32) {
    icon::scaled(painter, centre, tint, scale, icon::stop);
}

/// The arrow on the send button: a shaft and two strokes, drawn rather than lettered.
fn send_arrow(painter: &egui::Painter, centre: Pos2, tint: Color32, scale: f32) {
    let half = 5.0 * scale;
    let stroke = Stroke::new(1.8 * scale, tint);
    painter.line_segment(
        [Pos2::new(centre.x, centre.y + half), Pos2::new(centre.x, centre.y - half)],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(centre.x - half * 0.75, centre.y - half * 0.2),
            Pos2::new(centre.x, centre.y - half),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(centre.x + half * 0.75, centre.y - half * 0.2),
            Pos2::new(centre.x, centre.y - half),
        ],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2193`: a line break typed at the end of the draft is a line the caret is on, so the well grows
    /// for it at once rather than on the first letter typed after it.
    #[test]
    fn a_line_break_at_the_end_of_the_draft_is_a_line() {
        let renderer = crate::services::text_renderer::TextRenderer::new();
        let look = Look::of(&crate::settings::Settings::new(), &renderer);
        assert_eq!(prompt_lines("one line", &look, 600.0), 1);
        assert_eq!(prompt_lines("one line\n", &look, 600.0), 2);
        assert_eq!(prompt_lines("one\ntwo\nthree", &look, 600.0), 3);
    }

    /// A hint too wide for its field is the short one, because a hint that wraps grows the box past the
    /// well measured for it — which at a chat node's smallest size drew its last line below the node.
    #[test]
    fn the_hint_is_the_long_one_only_where_it_fits_on_one_line() {
        assert_eq!(hint(300.0, 420.0, true), LONG_HINT, "there is room for the long form");
        assert_eq!(hint(300.0, 240.0, true), SHORT_HINT, "and there is not");
        // Exactly at the edge counts as fitting, and eight points inside it does not.
        assert_eq!(hint(300.0, 308.0, true), LONG_HINT);
        assert_eq!(hint(300.0, 307.0, true), SHORT_HINT);
        // With something attached it is a hint rather than a label, so it never names the picture.
        assert_eq!(hint(300.0, 900.0, false), SHORT_HINT);
    }
}
