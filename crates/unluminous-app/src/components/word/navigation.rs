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

/// The Pages tab: each page, its number and the first words on it, a press going to it.
fn pages(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc) {
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("word-navigation-pages").auto_shrink([false, false]).show(
        ui,
        |ui| {
            for (index, page) in doc.pages.pages.iter().enumerate() {
                let first = page.first.clone();
                let words = first
                    .as_ref()
                    .and_then(|pos| {
                        unluminous_word::position::paragraph(
                            &doc.session.document,
                            &pos.story,
                            &pos.path,
                        )
                    })
                    .map(|p| p.visible_text().chars().take(40).collect::<String>())
                    .unwrap_or_default();
                let response = ui.selectable_label(
                    false,
                    RichText::new(format!("Page {}  {}", page.number, words))
                        .size(look.font_size * 0.92),
                );
                if response.clicked() {
                    chosen = first.or(Some(Pos::new(vec![0], 0)));
                    let _ = index;
                }
            }
        },
    );
    if let Some(at) = chosen {
        doc.session.select(at.clone(), at);
        doc.view.reveal = true;
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
