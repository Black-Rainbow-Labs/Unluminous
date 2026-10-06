//! A ticket's description: written as its markdown source, or read rendered.
//!
//! The source is an `egui` text box and the rendered view is `components::markdown_text`, which is
//! `unluminous_core::markdown` and the editor's own painter — so a fenced code block sits on a panel and a
//! table has rules, exactly as in a `.md` file's preview.
//!
//! ## The box is the draft and the row is what was saved
//!
//! The box holds what somebody is typing. Every edit writes the whole text onto the row, which is what
//! "saves as you type" means and what the browser board does with a six hundred millisecond debounce. There is
//! no debounce here because there is no network: writing a column of a local row is a hundred
//! microseconds, measured by `examples/board_cost.rs`.

use egui::Rect;

use crate::services::agent_tasks::AgentTasks;
use crate::services::plugin_ui::{Look, Request};

/// The description inside a well the caller has already drawn, as either view, scrolled inside `inside`.
///
/// `task-2193`: *"The Description overlaps the todos and terminal."* The source view was a `TextEdit` put
/// at a rectangle, and a `TextEdit` holding more lines than fit grows past the rectangle it was put at:
/// nothing clipped it and nothing scrolled it, so a long description was drawn over the todos and the
/// terminal under it. Here it is inside a `ScrollArea` cut to the well, so a description of any length
/// stays in its own room and scrolls there.
///
/// `well` is the whole drawn well, which takes a press anywhere in it; `inside` is the room the words get.
pub fn in_a_well(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    well: Rect,
    inside: Rect,
    look: &Look<'_>,
    ink: egui::Color32,
) -> Vec<Request> {
    let mut requests = Vec::new();
    if board.detail().description_rendered {
        rendered_text(board, ui, well, inside, look);
        // The rendered view is read rather than typed into, so its menu copies the description's source.
        let id = ui.id().with("agent-tasks-description-menu");
        if crate::components::controls::read_only_menu(ui, well, id, &[("Copy Description", true)])
            == Some(0)
        {
            requests.push(Request::Copy(board.description_text()));
        }
        return requests;
    }
    let mut text = board.description_text();
    let description_id = ui.id().with("agent-tasks-description");
    crate::components::controls::claim_the_field(ui, well, description_id, "Description field");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inside));
    child.set_clip_rect(inside.intersect(ui.clip_rect()));
    let font = egui::FontId::proportional(look.font_size);
    let row = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font));
    let changed = egui::ScrollArea::vertical()
        .id_salt("agent-tasks-description-scroll")
        .auto_shrink([false, false])
        .max_height(inside.height())
        .show(&mut child, |ui| {
            let response = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(description_id)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .hint_text(crate::components::controls::placeholder(
                        "What needs doing, in markdown.",
                        &font,
                        look.palette.text_faint,
                    ))
                    .desired_width(inside.width())
                    // At least the well's own height, so a press anywhere below the last line still
                    // lands in the box and puts the caret at the end.
                    .desired_rows(((inside.height() / row.max(1.0)) as usize).max(1))
                    .font(font.clone())
                    .text_color(ink),
            );
            response.changed()
        })
        .inner;
    if changed && board.detail().task.as_ref().is_some_and(|task| task.description != text) {
        if let Err(problem) = board.save_the_description(&text) {
            requests.push(Request::Message(problem));
        }
    }
    requests
}

/// The description as markdown, painted into `inside` with no panel of its own and scrolled with the wheel.
fn rendered_text(
    board: &mut AgentTasks,
    ui: &mut egui::Ui,
    well: Rect,
    inside: Rect,
    look: &Look<'_>,
) {
    use crate::components::markdown_text;
    let source = board.description_text();
    if source.trim().is_empty() {
        super::text(
            ui.painter(),
            inside.min,
            "Nothing written yet.",
            look.font_size,
            look.palette.text_faint,
        );
        return;
    }
    let colors = markdown_text::Colors {
        text: look.palette.text,
        strong: look.palette.text_strong,
        code: look.palette.added,
        link: look.palette.accent,
        quiet: look.palette.text_dim,
        rule: look.palette.divider,
    };
    let family = look.font_family.clone();
    let scroll = board.description_scroll;
    let made = board.markdown.rendered(
        "description",
        &source,
        look.renderer,
        &family,
        look.font_size,
        colors,
        inside.width(),
        None,
    );
    let height = made.height();
    markdown_text::show(ui, inside, made, look.renderer, scroll);
    let over = crate::components::controls::pointer_in(ui).is_some_and(|at| well.contains(at));
    if over {
        let wheel = ui.ctx().input(|input| input.smooth_scroll_delta.y);
        if wheel != 0.0 {
            board.description_scroll =
                (scroll - wheel).clamp(0.0, (height - inside.height()).max(0.0));
        }
    }
}
