//! What opens over the pages: the `/` menu, the `Ctrl+/` command search, the bubble over a selection, and
//! the find bar.
//!
//! The `/` menu and the search are both lists of the same `actions::ACTIONS`, filtered by what has been
//! typed; Up and Down choose, Enter runs, Escape closes.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{actions, Doc, Level};

/// How many rows a menu shows.
const ROWS: usize = 9;

/// The keys the open menu takes before the document does. Answers whether it took this one.
pub fn takes_key(doc: &mut Doc, key: egui::Key, requests: &mut Vec<Request>) -> bool {
    if let Some((query, chosen)) = doc.view.palette.as_mut() {
        let found = actions::search(query, false);
        match key {
            egui::Key::ArrowDown => *chosen = (*chosen + 1).min(found.len().saturating_sub(1)),
            egui::Key::ArrowUp => *chosen = chosen.saturating_sub(1),
            egui::Key::Escape => doc.view.palette = None,
            egui::Key::Enter => {
                let id = found.get(*chosen).map(|a| a.id);
                doc.view.palette = None;
                if let Some(id) = id {
                    super::run_action(doc, id, requests);
                }
            }
            egui::Key::Backspace => {
                query.pop();
                *chosen = 0;
            }
            _ => return false,
        }
        return true;
    }
    if let Some((at, query, chosen)) = doc.view.slash.clone() {
        let found = actions::search(&query, true);
        match key {
            egui::Key::ArrowDown => doc.view.slash = Some((at, query, (chosen + 1).min(found.len().saturating_sub(1)))),
            egui::Key::ArrowUp => doc.view.slash = Some((at, query, chosen.saturating_sub(1))),
            egui::Key::Escape => doc.view.slash = None,
            egui::Key::Enter | egui::Key::Tab => {
                let Some(id) = found.get(chosen).map(|a| a.id) else {
                    doc.view.slash = None;
                    return false;
                };
                run_slash(doc, &at, &query, id, requests);
            }
            egui::Key::Backspace => {
                if query.is_empty() {
                    doc.view.slash = None;
                    return false;
                }
                let mut query = query;
                query.pop();
                doc.view.slash = Some((at, query, 0));
                return false;
            }
            _ => return false,
        }
        return true;
    }
    false
}

/// Take the `/` and what was typed after it out, then run the chosen command.
fn run_slash(doc: &mut Doc, at: &unluminous_word::position::Pos, query: &str, id: &str, requests: &mut Vec<Request>) {
    doc.view.slash = None;
    let typed = 1 + query.len();
    doc.session.select(at.clone(), at.at(at.offset + typed));
    doc.session.backspace(unluminous_word::edit::Unit::Character);
    super::run_action(doc, id, requests);
}

/// Draw whichever of the four is open.
pub fn show(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    if doc.view.palette.is_some() {
        palette(ui, look, doc, area, requests);
    } else if doc.view.slash.is_some() {
        slash(ui, look, doc, area, requests);
    } else if !doc.session.selection.is_empty() && !doc.view.dragging {
        bubble(ui, look, doc, area, requests);
    }
    if doc.view.find.is_some() {
        find_bar(ui, look, doc, area, requests);
    }
}

/// A list of commands in a card at `at`, the chosen one lit. Answers the one clicked.
fn list(ui: &mut egui::Ui, look: &Look<'_>, name: &str, at: Pos2, width: f32, found: &[&'static actions::Action], chosen: usize, header: Option<&str>) -> Option<&'static str> {
    let scale = look.scale();
    let row = 26.0 * scale;
    let head = if header.is_some() { 34.0 * scale } else { 0.0 };
    let shown = found.len().min(ROWS);
    let first = chosen.saturating_sub(ROWS - 1);
    let height = head + row * shown.max(1) as f32 + 8.0 * scale;
    let card = Rect::from_min_size(at, Vec2::new(width, height));
    let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with(("word-list", name)));
    let painter = ui.ctx().layer_painter(layer);
    painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), 8.0, Color32::from_black_alpha(80));
    painter.rect(card, 8.0, look.palette.menu, Stroke::new(1.0, look.palette.control_border), egui::StrokeKind::Inside);
    if let Some(header) = header {
        painter.text(card.left_top() + Vec2::new(12.0, head / 2.0), Align2::LEFT_CENTER, header, FontId::proportional(look.font_size), look.palette.text);
        painter.line_segment([Pos2::new(card.left(), card.top() + head), Pos2::new(card.right(), card.top() + head)], Stroke::new(1.0, look.palette.divider));
    }
    if found.is_empty() {
        painter.text(card.left_top() + Vec2::new(12.0, head + row / 2.0 + 4.0), Align2::LEFT_CENTER, "No command has that name", FontId::proportional(look.font_size * 0.95), look.palette.text_dim);
        return None;
    }
    let mut clicked = None;
    for (offset, action) in found.iter().skip(first).take(ROWS).enumerate() {
        let index = first + offset;
        let r = Rect::from_min_size(card.left_top() + Vec2::new(4.0, head + 4.0 * scale + row * offset as f32), Vec2::new(width - 8.0, row));
        let response = ui.interact(r, ui.id().with(("word-list-row", name, index)), Sense::click());
        if index == chosen || response.hovered() {
            painter.rect_filled(r, 5.0, look.palette.selected_row);
        }
        painter.text(r.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, action.label, FontId::proportional(look.font_size), look.palette.text_strong);
        painter.text(r.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, action.level.name(), FontId::proportional(look.font_size * 0.8), look.palette.text_faint);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, action.label));
        if response.clicked() {
            clicked = Some(action.id);
        }
    }
    clicked
}

/// `Ctrl+/`: every command, searched by name.
fn palette(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    // Typed letters filter; keys::type_text leaves them alone while the search is open.
    let typed: String = ui.input(|i| i.events.iter().filter_map(|e| match e { egui::Event::Text(t) => Some(t.clone()), _ => None }).collect());
    let Some((query, chosen)) = doc.view.palette.as_mut() else { return };
    if !typed.is_empty() {
        query.push_str(&typed);
        *chosen = 0;
    }
    let (query, chosen) = (query.clone(), *chosen);
    let found = actions::search(&query, false);
    let width = (420.0 * look.scale()).min(area.width() - 32.0);
    let at = Pos2::new(area.center().x - width / 2.0, area.top() + 24.0);
    let header = if query.is_empty() { "Type to find a command…".to_owned() } else { query.clone() };
    if let Some(id) = list(ui, look, "palette", at, width, &found, chosen, Some(&header)) {
        doc.view.palette = None;
        super::run_action(doc, id, requests);
    }
}

/// The `/` menu, under the caret.
fn slash(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    let Some((at, query, chosen)) = doc.view.slash.clone() else { return };
    // The menu closes when the caret leaves the paragraph it was opened in.
    if doc.session.selection.head.path != at.path {
        doc.view.slash = None;
        return;
    }
    let found = actions::search(&query, true);
    let place = caret_point(doc, area).unwrap_or(area.center());
    let width = 260.0 * look.scale();
    let place = Pos2::new(place.x.min(area.right() - width - 8.0), (place.y + 6.0).min(area.bottom() - 260.0));
    if let Some(id) = list(ui, look, "slash", place, width, &found, chosen, None) {
        run_slash(doc, &at, &query, id, requests);
    }
}

/// Where the caret is on the screen, below its line.
fn caret_point(doc: &Doc, area: Rect) -> Option<Pos2> {
    let (index, line) = doc.pages.find(&doc.session.selection.head)?;
    let (tops, _) = super::page_view::page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    Some(origin + Vec2::new(line.x_of(doc.session.selection.head.offset) * zoom, (line.top + line.height) * zoom))
}

/// The small bar over a selection: bold, italic, underline, highlight, link, comment.
fn bubble(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    let start = doc.session.selection.ordered().0.clone();
    let Some((index, line)) = doc.pages.find(&start) else { return };
    let (tops, _) = super::page_view::page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    let scale = look.scale();
    let buttons: [(&str, &str); 6] = [("bold", "B"), ("italic", "I"), ("underline", "U"), ("highlight", "▍"), ("link", "🔗"), ("comment", "💬")];
    let size = 26.0 * scale;
    let width = size * buttons.len() as f32 + 8.0;
    let x = (origin.x + line.x_of(start.offset) * zoom).clamp(area.left() + 4.0, area.right() - width - 4.0);
    let y = origin.y + line.top * zoom - size - 10.0;
    if y < area.top() || y > area.bottom() {
        return;
    }
    let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, size + 6.0));
    let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("word-bubble"));
    let painter = ui.ctx().layer_painter(layer);
    painter.rect_filled(card.translate(Vec2::new(0.0, 2.0)), 8.0, Color32::from_black_alpha(70));
    painter.rect(card, 8.0, look.palette.menu, Stroke::new(1.0, look.palette.control_border), egui::StrokeKind::Inside);
    let run = doc.session.look_at_caret();
    for (n, (id, label)) in buttons.iter().enumerate() {
        let r = Rect::from_min_size(card.left_top() + Vec2::new(4.0 + size * n as f32, 3.0), Vec2::splat(size));
        let response = ui.interact(r, ui.id().with(("word-bubble", *id)), Sense::click());
        let on = match *id {
            "bold" => run.bold,
            "italic" => run.italic,
            "underline" => run.underline != unluminous_word::props::Underline::None,
            "highlight" => run.highlight.is_some(),
            _ => false,
        };
        if on || response.hovered() {
            painter.rect_filled(r, 5.0, look.palette.selected_row);
        }
        let colour = if *id == "highlight" { Color32::from_rgb(255, 230, 0) } else { look.palette.text_strong };
        painter.text(r.center(), Align2::CENTER_CENTER, *label, FontId::proportional(look.font_size), colour);
        let name = match *id {
            "highlight" => "Highlight",
            "link" => "Link (Ctrl+K)",
            "comment" => "Comment",
            other => other,
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        if response.on_hover_text(name).clicked() {
            match *id {
                "highlight" => {
                    let on = run.highlight.is_none();
                    doc.session.format(move |rpr| unluminous_word::props::set_val(rpr, "w:highlight", on.then_some("yellow")));
                }
                "link" => {
                    doc.view.panel = vec![Level::Root, Level::Insert];
                    doc.view.fields.insert("link", String::new());
                }
                "comment" => super::run_action(doc, "comment", requests),
                other => super::run_action(doc, other, requests),
            }
            requests.push(Request::TakeTheKeyboard(true));
        }
    }
}

/// Find, and Replace with it, at the top right of the pages.
fn find_bar(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, area: Rect, requests: &mut Vec<Request>) {
    let scale = look.scale();
    let replace = doc.view.find.as_ref().is_some_and(|f| f.replace);
    let width = 360.0 * scale;
    let height = if replace { 76.0 } else { 42.0 } * scale;
    let rect = Rect::from_min_size(Pos2::new(area.right() - width - 16.0, area.top() + 10.0), Vec2::new(width, height));
    let mut close = false;
    let mut go: Option<bool> = None;
    let mut replace_one = false;
    let mut replace_all = false;
    egui::Area::new(ui.id().with("word-find"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width - 16.0);
                let Some(find) = doc.view.find.as_mut() else { return };
                ui.horizontal(|ui| {
                    let field = ui.add(egui::TextEdit::singleline(&mut find.needle).hint_text("Find").desired_width(width * 0.45));
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go = Some(true);
                    }
                    if !field.has_focus() && find.needle.is_empty() {
                        field.request_focus();
                    }
                    ui.toggle_value(&mut find.case, "Aa").on_hover_text("Match case");
                    ui.toggle_value(&mut find.word, "W").on_hover_text("Whole words");
                    if ui.button("↑").on_hover_text("Previous").clicked() {
                        go = Some(false);
                    }
                    if ui.button("↓").on_hover_text("Next").clicked() {
                        go = Some(true);
                    }
                    if ui.button("✕").on_hover_text("Close").clicked() {
                        close = true;
                    }
                });
                if find.replace {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut find.with).hint_text("Replace with").desired_width(width * 0.45));
                        replace_one = ui.button("Replace").clicked();
                        replace_all = ui.button("All").clicked();
                    });
                }
            });
        });
    let Some(find) = doc.view.find.clone() else { return };
    let matches = doc.session.find(&find.needle, find.case, find.word);
    if let Some(forward) = go {
        if !matches.is_empty() {
            let head = doc.session.selection.ordered().1.clone();
            let next = if forward {
                matches.iter().position(|m| m.anchor >= head).unwrap_or(0)
            } else {
                matches.iter().rposition(|m| m.head < *doc.session.selection.ordered().0).unwrap_or(matches.len() - 1)
            };
            doc.session.selection = matches[next].clone();
            doc.view.reveal = true;
        }
    }
    if replace_one && !matches.is_empty() {
        let current = matches.iter().position(|m| *m == doc.session.selection).unwrap_or(0);
        doc.session.selection = matches[current].clone();
        doc.session.type_text(&find.with);
    }
    if replace_all {
        let count = doc.session.replace_all(&find.needle, &find.with, find.case, find.word);
        requests.push(Request::Message(format!("{count} replaced")));
    }
    if !find.needle.is_empty() {
        let label = format!("{} found", matches.len());
        ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, ui.id().with("word-find-count"))).text(
            Pos2::new(rect.left() - 8.0, rect.top() + 21.0 * scale),
            Align2::RIGHT_CENTER,
            label,
            FontId::proportional(look.font_size * 0.85),
            look.palette.text_dim,
        );
    }
    if close {
        doc.view.find = None;
        requests.push(Request::TakeTheKeyboard(true));
    }
}
