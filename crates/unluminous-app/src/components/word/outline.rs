//! Outline view (task-2359, TDD 5.5): the body's headings by level, with its body text as first lines,
//! and the tools Word's Outlining tab has: promote and demote, to Heading 1 and to body text, move up
//! and down, expand and collapse, and which levels to show. Every change writes the paragraphs' real
//! styles, so the outline is the document.
//!
//! A row put under a collapsed heading is hidden; a heading moved while collapsed takes its whole
//! section with it. Typing goes to the paragraph the caret is in, as in Print Layout.
//!
//! `task-2366`: the tools are a strip of pills above the paper, and a pill whose tool cannot do anything
//! where the caret is (Demote on body text, Move Up on the first paragraph) is drawn quiet and takes no
//! press, through `actions::applies`, rather than answering with an error. The marks beside the rows are
//! drawn, because the characters Word uses for them are not in every face.

use egui::{Pos2, Rect, RichText, Sense, Vec2};

use super::kit;
use crate::components::controls::WithHint;
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
    let z = kit::zoom(ui.ctx());
    // One row of tools when the tab is wide enough, and the level and the switch on a second row when not.
    let tool_rows = if tools_width(ui) + 24.0 * z <= rect.width() { 1.0 } else { 2.0 };
    let strip = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width(), TOOLS * z * tool_rows - (tool_rows - 1.0) * 8.0 * z),
    );
    let paper = Rect::from_min_max(Pos2::new(rect.left(), strip.bottom()), rect.max);
    ui.painter().rect_filled(strip, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), strip.left_bottom(), strip.right_bottom());
    toolbar(ui, doc, strip, requests);
    ui.painter().rect_filled(paper, 0.0, crate::theme::color::word_paper());
    let mut inner =
        ui.new_child(egui::UiBuilder::new().max_rect(paper.shrink2(Vec2::new(24.0, 12.0) * z)));
    inner.set_clip_rect(paper);
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
                    13.0 * z * (1.3 - 0.05 * f32::from(row.level))
                } else {
                    13.0 * z
                };
                let text = if row.level == 10 && doc.view.first_line_only {
                    row.text.lines().next().unwrap_or_default().chars().take(90).collect::<String>()
                } else {
                    row.text.clone()
                };
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    // A chevron for a heading with something under it, pointing down while it is open and
                    // right while it is folded; a short bar for a heading with nothing under it; a dot for
                    // body text. A press on a chevron folds or opens the heading.
                    let folded = doc.view.outline_collapsed.contains(&row.path);
                    if row_mark(ui, row.level, row.has_children, folded, size) && row.has_children {
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
                        // A soft wash of the accent behind the row the caret is in.
                        ui.painter().rect_filled(
                            response.rect.expand2(Vec2::new(6.0, 2.0) * z),
                            6.0 * z,
                            crate::theme::color::accent().gamma_multiply(0.12),
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

/// How tall the strip of tools is, before the tab's zoom.
const TOOLS: f32 = 46.0;

/// The mark beside a row, drawn: a chevron for a heading with something under it, a short bar for a heading
/// with nothing under it, a dot for body text. Answers whether it was pressed.
fn row_mark(ui: &mut egui::Ui, level: u8, has_children: bool, folded: bool, size: f32) -> bool {
    let z = kit::zoom(ui.ctx());
    let ink = egui::Color32::from_gray(110); // any ground: the outline is drawn on paper
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size.max(14.0 * z)), Sense::click());
    let centre = rect.center();
    let painter = ui.painter();
    let stroke = egui::Stroke::new(1.6 * z, ink);
    match (level < 10, has_children) {
        (true, true) => {
            let r = 3.5 * z;
            let points = match folded {
                true => vec![
                    centre + Vec2::new(-r * 0.5, -r),
                    centre + Vec2::new(r * 0.6, 0.0),
                    centre + Vec2::new(-r * 0.5, r),
                ],
                false => vec![
                    centre + Vec2::new(-r, -r * 0.5),
                    centre + Vec2::new(0.0, r * 0.6),
                    centre + Vec2::new(r, -r * 0.5),
                ],
            };
            let _ = painter.add(egui::Shape::line(points, stroke));
            if response.hovered() {
                let ring = egui::Stroke::new(1.0, ink.gamma_multiply(0.6));
                painter.circle_stroke(centre, 7.0 * z, ring);
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        (true, false) => {
            let half = Vec2::new(3.5 * z, 0.0);
            painter.line_segment([centre - half, centre + half], stroke);
        }
        _ => {
            painter.circle_filled(centre, 2.0 * z, ink);
        }
    }
    let name = match (level < 10, has_children, folded) {
        (true, true, true) => "Expand heading",
        (true, true, false) => "Collapse heading",
        (true, false, _) => "Heading",
        _ => "Body text",
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    response.clicked()
}

/// The tools, each as its words on the pill, its name, and the action it runs.
const OUTLINE_TOOLS: [(&str, &str, &str); 8] = [
    ("To Heading 1", "Promote to Heading 1", "outline-to-heading-1"),
    ("Promote", "Promote", "promote-heading"),
    ("Demote", "Demote", "demote-heading"),
    ("To Body Text", "Demote to Body Text", "outline-to-body"),
    ("Move Up", "Move Up", "outline-move-up"),
    ("Move Down", "Move Down", "outline-move-down"),
    ("Expand", "Expand", "outline-expand"),
    ("Collapse", "Collapse", "outline-collapse"),
];

/// How wide the whole strip of tools is on one row: the pills, the gaps between their groups, the level and
/// the switch.
fn tools_width(ui: &egui::Ui) -> f32 {
    let z = kit::zoom(ui.ctx());
    let pills: f32 =
        OUTLINE_TOOLS.iter().map(|(label, ..)| kit::pill_width(ui, label) + 5.0 * z).sum();
    pills + 16.0 * z + 12.0 * z + 118.0 * z + 14.0 * z + 40.0 * z + 8.0 * z + 100.0 * z
}

/// The Outlining tools, as pills across `strip`, then the level to show and whether body text shows its
/// first line only, on the same row or on a second one when the tab is narrow.
fn toolbar(ui: &mut egui::Ui, doc: &mut Doc, strip: Rect, requests: &mut Vec<Request>) {
    let z = kit::zoom(ui.ctx());
    let two_rows = strip.height() > TOOLS * z + 1.0;
    let row_height = (TOOLS - 8.0) * z;
    let mut middle = match two_rows {
        true => strip.top() + 4.0 * z + row_height / 2.0,
        false => strip.center().y,
    };
    let mut x = strip.left() + 12.0 * z;
    for (n, (label, name, id)) in OUTLINE_TOOLS.iter().enumerate() {
        // A little more room between the groups: the level, the moves, the folds.
        if n == 4 || n == 6 {
            x += 8.0 * z;
        }
        let width = kit::pill_width(ui, label);
        let rect = Rect::from_min_size(
            Pos2::new(x, middle - kit::PILL * z / 2.0),
            Vec2::new(width, kit::PILL * z),
        );
        x += width + 5.0 * z;
        let enabled = crate::services::word::actions::applies(id, doc);
        if kit::pill_at(ui, rect, label, name, false, enabled).with_hint(*name).clicked() {
            super::run_action(doc, id, requests);
        }
    }
    match two_rows {
        true => {
            x = strip.left() + 12.0 * z;
            middle += row_height;
        }
        false => x += 12.0 * z,
    }
    let level = doc.view.outline_level;
    let shown = if level >= 10 { "All levels".to_owned() } else { format!("Level {level}") };
    let chip = Rect::from_min_size(
        Pos2::new(x, middle - kit::CHIP * z / 2.0),
        Vec2::new(118.0 * z, kit::CHIP * z),
    );
    let chosen = kit::select_chip(ui, chip, "Show level", &shown, 150.0 * z, |ui| {
        let mut chosen = None;
        for n in 1..=10u8 {
            let label = if n == 10 { "All levels".to_owned() } else { format!("Level {n}") };
            if crate::components::controls::menu_row(ui, &label, "", true, level == n, 0.0) {
                chosen = Some(n);
            }
        }
        chosen
    });
    if let Some(n) = chosen {
        doc.view.outline_level = n;
    }
    x = chip.right() + 14.0 * z;
    let track = Rect::from_min_size(Pos2::new(x, middle - 11.0 * z), Vec2::new(40.0 * z, 22.0 * z));
    let label = Rect::from_min_size(
        Pos2::new(track.right() + 8.0 * z, middle - 11.0 * z),
        Vec2::new(110.0 * z, 22.0 * z),
    );
    let mut first = doc.view.first_line_only;
    kit::layer(ui, "outline-first-line", track, |rux| {
        crate::components::settings_kit::switch(
            rux,
            track,
            "Show first line only",
            &mut first,
            label,
        );
    });
    doc.view.first_line_only = first;
    let ink = kit::ink();
    let size = 12.0 * z;
    let galley = kit::words(ui, "First line only", size, ink.i700, label.width());
    kit::draw_by_capitals(ui.painter(), label.left(), middle, galley, size, ink.i700);
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
