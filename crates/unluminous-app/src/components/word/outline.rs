//! Outline view (task-2359, TDD 5.5): the body's headings by level, with its body text as first lines,
//! and the tools Word's Outlining tab has: promote and demote, to Heading 1 and to body text, move up
//! and down, expand and collapse, and which levels to show. Every change writes the paragraphs' real
//! styles, so the outline is the document.
//!
//! A row put under a collapsed heading is hidden; a heading moved while collapsed takes its whole
//! section with it. Typing goes to the paragraph the caret is in, as in Print Layout.

use egui::{Rect, RichText, Sense, Stroke, Vec2};
use unluminous_word::position::{self, Pos, StoryId};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::Doc;

/// One body paragraph as the outline shows it.
struct Row {
    path: Vec<u32>,
    /// 1 to 9 for a heading, 10 for body text.
    level: u8,
    text: String,
    /// A heading with something under it, which a press on its mark folds.
    has_children: bool,
}

fn rows(doc: &Doc) -> Vec<Row> {
    let resolver = unluminous_word::styles::Resolver::new(&doc.session.document);
    let mut out: Vec<Row> = doc
        .session
        .document
        .body
        .iter()
        .enumerate()
        .map(|(i, block)| match block {
            unluminous_word::model::Block::Paragraph(p) => Row {
                path: vec![i as u32],
                level: resolver.paragraph(&p.props, None).outline_level.map_or(10, |l| l + 1),
                text: p.visible_text(),
                has_children: false,
            },
            unluminous_word::model::Block::Table(_) => {
                Row { path: vec![i as u32], level: 10, text: "[Table]".into(), has_children: false }
            }
            _ => Row { path: vec![i as u32], level: 10, text: String::new(), has_children: false },
        })
        .collect();
    for i in 0..out.len() {
        let level = out[i].level;
        out[i].has_children = level < 10 && out.get(i + 1).is_some_and(|next| next.level > level);
    }
    out
}

/// Which rows show: a level the outline shows, not under a collapsed heading.
fn shown(
    rows: &[Row],
    show_level: u8,
    collapsed: &std::collections::HashSet<Vec<u32>>,
) -> Vec<bool> {
    let mut out = Vec::with_capacity(rows.len());
    let mut hidden_under: Option<u8> = None;
    for row in rows {
        if let Some(level) = hidden_under {
            if row.level <= level {
                hidden_under = None;
            }
        }
        let visible = hidden_under.is_none()
            && (row.level <= show_level || show_level >= 10 && row.level == 10);
        out.push(visible);
        if hidden_under.is_none() && row.level < 10 && collapsed.contains(&row.path) {
            hidden_under = Some(row.level);
        }
    }
    out
}

/// Draw the outline into `rect`.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    rect: Rect,
    requests: &mut Vec<Request>,
) {
    ui.painter().rect_filled(rect, 0.0, crate::theme::color::word_paper());
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(16.0, 8.0))));
    inner.set_clip_rect(rect);
    toolbar(&mut inner, look, doc, requests);
    inner.add_space(6.0);
    let all = rows(doc);
    let visible = shown(&all, doc.view.outline_level, &doc.view.outline_collapsed);
    let current = doc.session.selection.head.path.first().copied();
    let ink = egui::Color32::from_gray(30); // any ground: the outline is drawn on paper
    let mut chosen: Option<Vec<u32>> = None;
    let mut fold: Option<Vec<u32>> = None;
    egui::ScrollArea::vertical()
        .id_salt(("word-outline", &doc.key))
        .auto_shrink([false, false])
        .show(&mut inner, |ui| {
            for (row, show) in all.iter().zip(&visible) {
                if !show {
                    continue;
                }
                let indent = 22.0 * f32::from(row.level.min(10).saturating_sub(1).min(8));
                let size = if row.level < 10 {
                    look.font_size * (1.25 - 0.04 * f32::from(row.level))
                } else {
                    look.font_size
                };
                let text = if row.level == 10 && doc.view.first_line_only {
                    row.text.lines().next().unwrap_or_default().chars().take(90).collect::<String>()
                } else {
                    row.text.clone()
                };
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    // A plus for a heading with something under it, a minus for one with nothing, a circle
                    // for body text, as Word marks them; a press on a plus folds or opens the heading.
                    let mark = match (row.level < 10, row.has_children) {
                        (true, true) if doc.view.outline_collapsed.contains(&row.path) => {
                            "\u{2295}"
                        }
                        (true, true) => "\u{2296}",
                        (true, false) => "\u{2212}",
                        _ => "\u{25E6}",
                    };
                    let mark_response = ui.add(
                        egui::Label::new(RichText::new(mark).color(ink)).sense(Sense::click()),
                    );
                    if mark_response.clicked() && row.has_children {
                        fold = Some(row.path.clone());
                    }
                    let mut words =
                        RichText::new(if text.is_empty() { " ".to_owned() } else { text })
                            .size(size)
                            .color(ink);
                    if row.level < 10 {
                        words = words.strong();
                    }
                    let here = current == row.path.first().copied();
                    let response = ui.add(egui::Label::new(words).sense(Sense::click()).wrap());
                    if here {
                        ui.painter().rect_stroke(
                            response.rect.expand(2.0),
                            3.0,
                            Stroke::new(1.0, crate::theme::color::accent()),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if response.clicked() {
                        chosen = Some(row.path.clone());
                    }
                });
            }
        });
    if let Some(path) = fold {
        if !doc.view.outline_collapsed.remove(&path) {
            doc.view.outline_collapsed.insert(path);
        }
    }
    if let Some(path) = chosen {
        let at = Pos { story: StoryId::Body, path, offset: 0 };
        doc.session.select(at.clone(), at);
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The Outlining tools.
fn toolbar(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, requests: &mut Vec<Request>) {
    let _ = look;
    ui.horizontal_wrapped(|ui| {
        for (label, id) in [
            ("Promote to Heading 1", "outline-to-heading-1"),
            ("Promote", "promote-heading"),
            ("Demote", "demote-heading"),
            ("Demote to Body Text", "outline-to-body"),
            ("Move Up", "outline-move-up"),
            ("Move Down", "outline-move-down"),
            ("Expand", "outline-expand"),
            ("Collapse", "outline-collapse"),
        ] {
            if ui.button(label).clicked() {
                super::run_action(doc, id, requests);
            }
        }
        let mut level = doc.view.outline_level;
        egui::ComboBox::from_id_salt(("word-outline-level", &doc.key))
            .selected_text(if level >= 10 {
                "All Levels".to_owned()
            } else {
                format!("Level {level}")
            })
            .show_ui(ui, |ui| {
                crate::components::controls::set_in_the_list_size(ui);
                for n in 1..=9u8 {
                    ui.selectable_value(&mut level, n, format!("Level {n}"));
                }
                ui.selectable_value(&mut level, 10, "All Levels");
            });
        doc.view.outline_level = level;
        ui.checkbox(&mut doc.view.first_line_only, "Show first line only");
        if ui.button("Close Outline View").clicked() {
            doc.view.mode = crate::services::word::Mode::Print;
            doc.laid = u64::MAX;
        }
    });
}

/// Fold or open the heading at the caret, as Expand and Collapse do.
pub fn fold_at_caret(doc: &mut Doc, open: bool) {
    let head = doc.session.selection.head.clone();
    if head.story != StoryId::Body || head.path.len() != 1 {
        return;
    }
    if open {
        doc.view.outline_collapsed.remove(&head.path);
    } else if position::paragraph(&doc.session.document, &head.story, &head.path).is_some() {
        doc.view.outline_collapsed.insert(head.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_collapsed_heading_hides_what_is_under_it_and_the_level_hides_body_text() {
        let row = |path: u32, level: u8| Row {
            path: vec![path],
            level,
            text: String::new(),
            has_children: false,
        };
        let rows = vec![row(0, 1), row(1, 10), row(2, 2), row(3, 10), row(4, 1)];
        let none = std::collections::HashSet::new();
        assert_eq!(shown(&rows, 10, &none), vec![true; 5]);
        assert_eq!(shown(&rows, 2, &none), vec![true, false, true, false, true]);
        let collapsed: std::collections::HashSet<Vec<u32>> = [vec![0]].into_iter().collect();
        assert_eq!(shown(&rows, 10, &collapsed), vec![true, false, false, false, true]);
    }
}
