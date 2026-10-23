//! The navigation pane: the document's headings, indented by level, each one a link to its place.

use egui::{Rect, RichText, Stroke, Vec2};
use unluminous_word::position::Pos;

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Doc;

pub fn show(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, rect: Rect, requests: &mut Vec<Request>) {
    let _ = requests;
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, look.palette.divider));
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 10.0))));
    inner.set_clip_rect(rect);
    inner.label(RichText::new("Navigation").strong());
    inner.add_space(4.0);
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
                if label.on_hover_text(format!("Page {page}")).clicked() {
                    doc.session.select(Pos::new(path.clone(), 0), Pos::new(path, 0));
                    doc.view.reveal = true;
                }
            });
        }
    });
}
