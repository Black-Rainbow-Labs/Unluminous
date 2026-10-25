//! The navigation pane: the document's headings, indented by level, each one a link to its place.

use crate::components::controls::WithHint;
use egui::{Rect, RichText, Stroke, Vec2};
use unluminous_word::position::Pos;

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Doc;

pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = requests;
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    ui.painter().line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, look.palette.divider),
    );
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 10.0))));
    inner.set_clip_rect(rect);
    inner.label(RichText::new("Navigation").strong());
    inner.add_space(4.0);
    // Headings, Pages and Results, as Word's Navigation pane has them.
    inner.horizontal(|ui| {
        for (n, name) in ["Headings", "Pages", "Results"].iter().enumerate() {
            if ui.selectable_label(doc.view.navigation_tab == n as u8, *name).clicked() {
                doc.view.navigation_tab = n as u8;
            }
        }
    });
    inner.add_space(4.0);
    match doc.view.navigation_tab {
        1 => return pages(&mut inner, look, doc),
        2 => return results(&mut inner, look, doc),
        _ => {}
    }
    let headings = doc.session.toc_entries(9, &doc.pages);
    let current = doc.session.selection.head.path.clone();
    egui::ScrollArea::vertical().id_salt("word-navigation").auto_shrink([false, false]).show(&mut inner, |ui| {
        if headings.is_empty() {
            ui.label(RichText::new("Headings appear here. Give a paragraph a Heading style, or type # and a space at its start.").weak());
        }
        for (level, text, page, path) in headings {
            ui.horizontal(|ui| {
                ui.add_space(12.0 * f32::from(level.saturating_sub(1)));
                let here = current.first() >= path.first() && path == current;
                let label = ui.selectable_label(here, RichText::new(&text).size(look.font_size * if level == 1 { 1.0 } else { 0.92 }));
                if label.with_hint(format!("Page {page}")).clicked() {
                    doc.session.select(Pos::new(path.clone(), 0), Pos::new(path, 0));
                    doc.view.reveal = true;
                }
            });
        }
    });
}

/// The Pages tab: a small picture of each page, its text drawn as grey lines, with its number under
/// it, a press going to it (TDD 5.5). Only the pictures on the screen are drawn.
fn pages(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc) {
    let mut chosen = None;
    let current = doc.pages.find(&doc.session.selection.head).map(|(i, _)| i);
    egui::ScrollArea::vertical().id_salt("word-navigation-pages").auto_shrink([false, false]).show(
        ui,
        |ui| {
            let width = (ui.available_width() - 20.0).clamp(60.0, 160.0);
            for (index, page) in doc.pages.pages.iter().enumerate() {
                let scale = width / page.width.max(1.0);
                let height = page.height * scale;
                let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height + 22.0), egui::Sense::click());
                let sheet = Rect::from_min_size(egui::pos2(rect.center().x - width / 2.0, rect.top()), Vec2::new(width, height));
                if ui.is_rect_visible(sheet) {
                    paint_thumbnail(ui.painter(), page, sheet, scale, current == Some(index));
                }
                ui.painter().text(
                    egui::pos2(rect.center().x, sheet.bottom() + 11.0),
                    egui::Align2::CENTER_CENTER,
                    page.number.to_string(),
                    egui::FontId::proportional(look.font_size * 0.85),
                    look.palette.text_dim,
                );
                response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Page {}", page.number)));
                if response.clicked() {
                    chosen = page.first.clone().or(Some(Pos::new(vec![0], 0)));
                }
            }
        },
    );
    if let Some(at) = chosen {
        doc.session.select(at.clone(), at);
        doc.view.reveal = true;
    }
}

/// One page drawn small: the sheet, a grey line for each run of text, a box for each picture and shape,
/// and the rules.
fn paint_thumbnail(painter: &egui::Painter, page: &unluminous_word::layout::Page, sheet: Rect, scale: f32, here: bool) {
    use unluminous_word::layout::Item;
    let paper = crate::theme::color::word_paper();
    painter.rect_filled(sheet.translate(Vec2::new(0.0, 1.5)), 1.0, egui::Color32::from_black_alpha(60)); // any ground: the sheet's shadow
    painter.rect_filled(sheet, 0.0, paper);
    let at = |x: f32, y: f32| sheet.min + Vec2::new(x * scale, y * scale);
    let grey = egui::Color32::from_gray(150); // any ground: text drawn on the white sheet
    for item in &page.items {
        match item {
            Item::Text(text) => {
                let (Some(first), Some(last)) = (text.xs.first(), text.xs.last()) else { continue };
                let size = (text.style.size * scale * 0.55).max(0.6);
                let y = text.baseline - text.style.size * 0.35;
                let width = (last - first + text.style.size * 0.5).max(1.0);
                let color = if text.style.bold { egui::Color32::from_gray(110) } else { grey }; // any ground: on the sheet
                painter.rect_filled(Rect::from_min_size(at(*first, y), Vec2::new(width * scale, size)), 0.0, color);
            }
            Item::Picture { rect, .. } | Item::Shape { rect, .. } | Item::Placeholder { rect, .. } => {
                painter.rect_filled(Rect::from_min_size(at(rect.x, rect.y), Vec2::new(rect.w * scale, rect.h * scale)), 0.0, egui::Color32::from_gray(205)); // any ground: on the sheet
            }
            Item::Fill { rect, color } => {
                painter.rect_filled(
                    Rect::from_min_size(at(rect.x, rect.y), Vec2::new(rect.w * scale, rect.h * scale)),
                    0.0,
                    egui::Color32::from_rgb(color.0, color.1, color.2),
                );
            }
            Item::Rule { x0, y0, x1, y1, .. } => {
                painter.line_segment([at(*x0, *y0), at(*x1, *y1)], Stroke::new(0.5, grey));
            }
            Item::Object { .. } => {}
        }
    }
    if here {
        painter.rect_stroke(sheet.expand(2.0), 2.0, Stroke::new(2.0, crate::theme::color::accent()), egui::StrokeKind::Outside);
    }
}

/// The Results tab: every match of the find bar's search, with the words around it.
fn results(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc) {
    let Some(find) = doc.view.find.clone().filter(|f| !f.needle.is_empty()) else {
        ui.label(
            RichText::new("Search the document with Ctrl+F and the matches are listed here.")
                .weak(),
        );
        return;
    };
    let found = doc.session.search(&find.search()).unwrap_or_default();
    ui.label(RichText::new(format!("{} results", found.len())).color(look.palette.text_dim));
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("word-navigation-results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for hit in found.iter().take(500) {
                let text = unluminous_word::position::paragraph(
                    &doc.session.document,
                    &hit.anchor.story,
                    &hit.anchor.path,
                )
                .map(|p| p.flat_text())
                .unwrap_or_default();
                let start = hit.anchor.offset.min(text.len());
                let end = hit.head.offset.min(text.len()).max(start);
                let before: String = text[..start]
                    .chars()
                    .rev()
                    .take(24)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let after: String = text[end..].chars().take(24).collect();
                let label = format!("{before}[{}]{after}", &text[start..end]);
                if ui
                    .selectable_label(
                        doc.session.selection == *hit,
                        RichText::new(label).size(look.font_size * 0.9),
                    )
                    .clicked()
                {
                    chosen = Some(hit.clone());
                }
            }
        });
    if let Some(hit) = chosen {
        doc.session.selection = hit;
        doc.view.reveal = true;
    }
}
