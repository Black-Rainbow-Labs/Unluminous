//! Typing into a cell: the box drawn over the cell, the list of functions that opens while a formula's
//! function name is typed, the hint naming the arguments of the function the caret is in, and the coloured
//! outline round each cell a formula refers to.

use egui::{Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, Rect, Stroke, Vec2};

use super::grid::{Frame, Ink};
use super::keys::{self, Offered};
use crate::components::controls::{TextBoxes, WithHint};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::display;
use crate::services::spreadsheet::formula_text;
use crate::services::spreadsheet::Book;

/// The id of the box over the cell.
pub fn cell_box_id(key: &str) -> egui::Id {
    egui::Id::new(("spreadsheet-cell-editor", key))
}

/// The id of the box in the formula bar.
pub fn bar_box_id(key: &str) -> egui::Id {
    egui::Id::new(("spreadsheet-bar-editor", key))
}

/// Which box has the caret: the formula bar's when it was clicked into, otherwise the cell's.
pub fn box_with_the_caret(ctx: &egui::Context, book: &Book) -> egui::Id {
    let bar = bar_box_id(&book.key);
    if ctx.memory(|m| m.has_focus(bar)) || book.view.edit.as_ref().is_some_and(|e| e.in_bar) {
        bar
    } else {
        cell_box_id(&book.key)
    }
}

/// The functions to offer for the word before the caret.
pub fn offered(ctx: &egui::Context, book: &Book, list_on: bool) -> Offered {
    let Some(edit) = &book.view.edit else { return Offered::default() };
    if !list_on {
        return Offered::default();
    }
    let id = box_with_the_caret(ctx, book);
    let caret = keys::caret_of(ctx, id).unwrap_or(edit.text.chars().count());
    let Some((start, word)) = formula_text::word_before(&edit.text, caret) else {
        return Offered::default();
    };
    let upper = word.to_ascii_uppercase();
    let names: Vec<&'static str> = unluminous_sheet::functions::all()
        .iter()
        .map(|f| f.name)
        .filter(|name| name.starts_with(&upper))
        .take(8)
        .collect();
    Offered { start, names }
}

/// Draw the box over the cell being typed into.
pub fn show_cell_box(ui: &mut egui::Ui, book: &mut Book, frame: Frame, ink: &Ink) {
    let key = book.key.clone();
    let id = cell_box_id(&key);
    let zoom = frame.zoom;
    let Some(edit) = book.view.edit.as_ref() else { return };
    if edit.sheet != book.view.sheet {
        return;
    }
    let at = edit.at;
    let sheet = &book.session.book.sheets[book.view.sheet];
    let cell = match sheet.merge_at(at) {
        Some(m) => frame.range(book, m),
        None => frame.cell(book, at),
    };
    let style = display::style_at(sheet, at);
    let font = book.session.book.styles.font_of(style);
    let size = (font.size as f32 * 4.0 / 3.0 * zoom).clamp(6.0, 72.0);
    let font_id = egui::FontId::proportional(size);
    // The box grows to the right with what is typed, as Excel's does, and down for each line.
    let text = edit.text.clone();
    let galley = ui.painter().layout_no_wrap(text.clone(), font_id.clone(), ink.text);
    let lines = text.split('\n').count().max(1) as f32;
    let width = (galley.size().x + 12.0 * zoom)
        .max(cell.width())
        .min(frame.cells.right() - cell.left())
        .max(cell.width());
    let height = (galley.size().y.max(size * 1.2) * 1.0 + 4.0)
        .max(cell.height())
        .max(lines * size * 1.25 + 4.0);
    let rect = Rect::from_min_size(cell.min, Vec2::new(width, height));
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0, ink.paper);
    painter.rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(2.0, ink.accent),
        egui::StrokeKind::Outside,
    );
    let edit = book.view.edit.as_mut().expect("checked");
    let wanted_focus = edit.focus && !edit.in_bar;
    let inner = rect.shrink2(Vec2::new(3.0 * zoom, 1.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let response = child.text_box(
        egui::TextEdit::multiline(&mut edit.text)
            .id(id)
            .frame(egui::Frame::NONE)
            .font(font_id)
            .text_color(ink.text)
            .desired_rows(1)
            .desired_width(inner.width())
            .lock_focus(true)
            .return_key(Some(KeyboardShortcut::new(Modifiers::ALT, Key::Enter))),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Cell being typed into")
    });
    if wanted_focus {
        response.request_focus();
        keys::set_caret(ui.ctx(), id, edit.caret.take().unwrap_or(edit.text.chars().count()));
        edit.focus = false;
    }
}

/// Draw the list of functions under the box, and answer the name clicked, if one was.
pub fn show_function_list(
    ui: &mut egui::Ui,
    book: &Book,
    frame: Frame,
    look: &Look<'_>,
    offered: &Offered,
) -> Option<usize> {
    let edit = book.view.edit.as_ref()?;
    if offered.names.is_empty() {
        return None;
    }
    let anchor = if edit.in_bar {
        frame.cells.left_top() + Vec2::new(0.0, 2.0)
    } else {
        frame.cell(book, edit.at).left_bottom() + Vec2::new(0.0, 4.0)
    };
    let mut clicked = None;
    egui::Area::new(egui::Id::new(("spreadsheet-function-list", &book.key)))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .show(ui.ctx(), |ui| {
            crate::components::controls::menu_frame(ui).show(ui, |ui| {
                ui.set_width(360.0);
                for (i, name) in offered.names.iter().enumerate() {
                    let about = unluminous_sheet::functions::find(name).map_or("", |f| f.about);
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 26.0),
                        egui::Sense::click(),
                    );
                    let chosen = i == edit.chosen;
                    if chosen || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(8),
                            if chosen { look.palette.selected_row } else { look.palette.control },
                        );
                    }
                    ui.painter().text(
                        rect.left_center() + Vec2::new(10.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        *name,
                        egui::FontId::monospace(12.5),
                        look.palette.text_strong,
                    );
                    let about_short = crate::components::controls::truncate_chars(about, 44, 40);
                    ui.painter().text(
                        rect.left_center() + Vec2::new(120.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        about_short,
                        egui::FontId::proportional(11.5),
                        look.palette.text_dim,
                    );
                    let response = response.with_hint(about);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Function {name}"),
                        )
                    });
                    if response.clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    clicked
}

/// The hint naming the arguments of the function the caret is in.
pub fn show_argument_hint(ui: &egui::Ui, book: &Book, frame: Frame, look: &Look<'_>, caret: usize) {
    let Some(edit) = &book.view.edit else { return };
    let Some((name, argument)) = formula_text::function_at(&edit.text, caret) else { return };
    let Some(function) = unluminous_sheet::functions::find(&name) else { return };
    let anchor = if edit.in_bar {
        frame.cells.left_top() + Vec2::new(4.0, 4.0)
    } else {
        frame.cell(book, edit.at).left_top() - Vec2::new(0.0, 26.0)
    };
    egui::Area::new(egui::Id::new(("spreadsheet-argument-hint", &book.key)))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(look.palette.menu)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    // The argument the caret is in is set in the strong ink.
                    let mut job = egui::text::LayoutJob::default();
                    let quiet = egui::TextFormat {
                        font_id: egui::FontId::proportional(12.0),
                        color: look.palette.text_dim,
                        ..Default::default()
                    };
                    let strong = egui::TextFormat {
                        font_id: egui::FontId::proportional(12.0),
                        color: look.palette.text_strong,
                        ..Default::default()
                    };
                    job.append(&format!("{}(", function.name), 0.0, quiet.clone());
                    for (i, part) in function.args.split(", ").enumerate() {
                        if i > 0 {
                            job.append(", ", 0.0, quiet.clone());
                        }
                        let is_it = i == argument || (part.contains("...") && argument >= i);
                        job.append(part, 0.0, if is_it { strong.clone() } else { quiet.clone() });
                    }
                    job.append(")", 0.0, quiet);
                    ui.label(job);
                });
        });
}

/// The colours a formula's references are outlined in, in turn.
pub fn reference_colour(index: usize) -> Color32 {
    let (r, g, b) = display::REFERENCE_COLOURS[index % display::REFERENCE_COLOURS.len()];
    Color32::from_rgb(r, g, b)
}

/// Outline the cells the formula being typed refers to, each in its own colour.
pub fn show_reference_outlines(ui: &egui::Ui, book: &Book, frame: Frame) {
    let Some(edit) = &book.view.edit else { return };
    if !edit.text.starts_with('=') {
        return;
    }
    let painter = ui.painter_at(frame.cells);
    let here = &book.session.book.sheets[book.view.sheet].name;
    for (i, reference) in formula_text::references(&edit.text).iter().enumerate() {
        if reference.sheet.as_ref().is_some_and(|s| !s.eq_ignore_ascii_case(here)) {
            continue;
        }
        let rect = frame.range(book, reference.range);
        let colour = reference_colour(i);
        painter.rect_filled(rect, 0, colour.gamma_multiply(0.12));
        painter.rect_stroke(rect, 0, Stroke::new(1.5, colour), egui::StrokeKind::Inside);
        for corner in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()] {
            painter.rect_filled(Rect::from_center_size(corner, Vec2::splat(5.0)), 0, colour);
        }
    }
}
