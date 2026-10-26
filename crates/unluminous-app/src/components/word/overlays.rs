//! What opens over the pages: the `/` menu, the `Ctrl+/` command search, the bubble over a selection, and
//! the find bar.
//!
//! The `/` menu and the search are both lists of the same `actions::ACTIONS`, filtered by what has been
//! typed; Up and Down choose, Enter runs, Escape closes.
//!
//! Every one of them is the window's menu card (`controls::menu_frame`) with the design's rows, wells, pills
//! and round chips from `kit`, at the design's sizes times the tab's zoom (`task-2366`). No mark is a
//! character: an arrow, a cross or a link drawn as a character is an empty box in a face without it.

use egui::{Pos2, Rect, Sense, Stroke, Vec2};

use super::kit;
use crate::components::controls::{self, TextBoxes, WithHint};
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{actions, Doc, Level};
use crate::theme::svg_icon;

/// How many rows a menu shows.
const ROWS: usize = 10;

/// The keys the open menu takes before the document does. Answers whether it took this one.
pub fn takes_key(doc: &mut Doc, key: egui::Key, requests: &mut Vec<Request>) -> bool {
    if let Some(query) = doc.view.palette.as_ref().map(|(q, _)| q.clone()) {
        let found = actions::search(&query, false)
            .into_iter()
            .filter(|a| actions::applies(a.id, doc))
            .collect::<Vec<_>>();
        let Some((query, chosen)) = doc.view.palette.as_mut() else { return false };
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
        let found = actions::search(&query, true)
            .into_iter()
            .filter(|a| actions::applies(a.id, doc))
            .collect::<Vec<_>>();
        match key {
            egui::Key::ArrowDown => {
                doc.view.slash = Some((at, query, (chosen + 1).min(found.len().saturating_sub(1))))
            }
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
fn run_slash(
    doc: &mut Doc,
    at: &unluminous_word::position::Pos,
    query: &str,
    id: &str,
    requests: &mut Vec<Request>,
) {
    doc.view.slash = None;
    let typed = 1 + query.len();
    doc.session.select(at.clone(), at.at(at.offset + typed));
    doc.session.backspace(unluminous_word::edit::Unit::Character);
    super::run_action(doc, id, requests);
}

/// Draw whichever is open: the command search, the `/` menu, a misspelled word's card, or the bubble
/// over a selection, and the find bar beside any of them.
pub fn show(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    if doc.view.palette.is_some() {
        palette(ui, look, doc, area, requests);
    } else if doc.view.slash.is_some() {
        slash(ui, look, doc, area, requests);
    } else if doc.view.spell_menu.is_some() {
        super::spelling::show_menu(ui, look, doc, area, requests);
    } else if !doc.session.selection.is_empty() && !doc.view.dragging {
        bubble(ui, look, doc, area, requests);
    } else if doc.view.pasted.as_ref().is_some_and(|p| p.end == doc.session.selection.head) {
        paste_options(ui, look, doc, area, requests);
    }
    if doc.view.find.is_some() {
        find_bar(ui, look, doc, area, requests);
    }
    if doc.view.goto.is_some() {
        go_to_box(ui, look, doc, area, requests);
    }
}

/// Go To (Ctrl+G): a card at the top of the pages with one field for `page 3`, `section 2`, `line 40`,
/// `bookmark Intro`, `footnote 1`, `comment 2`, `heading 3`, `table 1` or `picture 2`. A number alone is a page.
fn go_to_box(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    let z = kit::zoom(ui.ctx());
    let width = (380.0 * z).min(area.width() - 24.0);
    let at = Pos2::new(area.center().x - width / 2.0, area.top() + 12.0 * z);
    let mut go = None;
    let mut close = false;
    let shown = egui::Area::new(ui.id().with("word-goto"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).show(ui, |ui| {
                let Some(text) = doc.view.goto.as_mut() else { return };
                let (row, _) =
                    ui.allocate_exact_size(Vec2::new(width - 16.0, 36.0 * z), Sense::hover());
                let chip = 28.0 * z;
                let well = Rect::from_min_max(
                    row.min,
                    Pos2::new(row.right() - chip - 8.0 * z, row.bottom()),
                );
                let field = search_field(
                    ui,
                    well,
                    "Go to",
                    text,
                    "Go to page 3, heading 2, bookmark Intro",
                    Some(kit::Mark::Rux(rux::Icon::ArrowRight)),
                );
                if text.is_empty() && !field.has_focus() {
                    field.request_focus();
                }
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = Some(text.clone());
                }
                let cross = Rect::from_center_size(
                    Pos2::new(row.right() - chip / 2.0, row.center().y),
                    Vec2::splat(chip),
                );
                if kit::round_chip(
                    ui,
                    cross,
                    "Close Go To",
                    "Close",
                    kit::Mark::Rux(rux::Icon::Cross),
                    false,
                )
                .clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    close = true;
                }
            });
        });
    remember_popup(ui, shown.response.rect);
    if let Some(text) = go {
        let mut words = text.split_whitespace();
        let first = words.next().unwrap_or("page").to_owned();
        let rest = words.collect::<Vec<_>>().join(" ");
        let (what, which) =
            if first.parse::<usize>().is_ok() { ("page".to_owned(), first) } else { (first, rest) };
        let pages = doc.pages.clone();
        match doc.session.go_to(&what, &which, &pages) {
            Ok(_) => {
                doc.view.goto = None;
                doc.view.reveal = true;
                requests.push(Request::TakeTheKeyboard(true));
            }
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    if close {
        doc.view.goto = None;
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// A field to type in: a carved well with round ends, an optional mark at its left, and the words of
/// `hint` in the quiet ink while it is empty.
fn search_field(
    ui: &mut egui::Ui,
    well: Rect,
    name: &str,
    text: &mut String,
    hint: &str,
    mark: Option<kit::Mark>,
) -> egui::Response {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    kit::layer(ui, ("field", name), well, |rux| {
        kit::well_surface(rux, well, well.height() / 2.0);
        if let Some(mark) = mark {
            let at = Pos2::new(well.left() + 17.0 * z, well.center().y);
            kit::paint_mark(rux, mark, at, 14.0 * z, ink.i400);
        }
    });
    let left = if mark.is_some() { 34.0 } else { 16.0 } * z;
    let font = egui::FontId::proportional(13.0 * z);
    // One row of the font, in the middle of the well, so the words and the hint are centred in it.
    let row = ui.fonts_mut(|f| f.row_height(&font));
    let inside = controls::field_text_rect_at(well, left, row);
    ui.text_box_at(
        inside,
        egui::TextEdit::singleline(text)
            .id(ui.id().with(("word-field", name)))
            .frame(egui::Frame::NONE)
            .font(font.clone())
            .text_color(ink.i900)
            .hint_text(controls::placeholder(hint, &font, ink.i400)),
    )
}

/// Note that a popup covers `card` this frame, so the page under it does not take a press meant for one
/// of its buttons. A popup is painted on a layer of its own, but its buttons are in the page's layer.
/// The page also takes drags, so egui gives it the press as well, and the page moved the caret, which
/// emptied the selection and closed the bubble before its button saw the release.
pub(super) fn remember_popup(ui: &egui::Ui, card: Rect) {
    let frame = ui.ctx().cumulative_frame_nr();
    let id = egui::Id::new("word-popups");
    ui.ctx().data_mut(|d| {
        let popups: &mut (u64, Vec<Rect>) = d.get_temp_mut_or_default(id);
        if popups.0 != frame {
            *popups = (frame, Vec::new());
        }
        popups.1.push(card);
    });
}

/// Whether a popup drawn this frame or the frame before covers a point. The page reads this before the
/// popups of this frame are drawn, so the frame before is the one that counts.
pub fn a_popup_covers(ui: &egui::Ui, at: Pos2) -> bool {
    let frame = ui.ctx().cumulative_frame_nr();
    let popups: (u64, Vec<Rect>) =
        ui.ctx().data(|d| d.get_temp(egui::Id::new("word-popups")).unwrap_or_default());
    popups.0 + 1 >= frame && popups.1.iter().any(|r| r.contains(at))
}

/// Where a list of commands goes and what it shows.
struct Menu<'a> {
    name: &'a str,
    at: Pos2,
    width: f32,
    found: &'a [&'static actions::Action],
    chosen: usize,
    /// What was typed into the command search, with its well at the top of the card. `None` for the `/`
    /// menu, whose words are typed into the page.
    query: Option<&'a str>,
}

/// A list of commands in the window's menu card, the chosen row carved in as the row under the pointer
/// is. Answers the one clicked.
fn list(ui: &mut egui::Ui, menu: Menu<'_>) -> Option<&'static str> {
    let Menu { name, at, width, found, chosen, query } = menu;
    let z = kit::zoom(ui.ctx());
    let row = controls::MENU_ROW * z;
    let first = chosen.saturating_sub(ROWS - 1);
    let inner = width - 16.0;
    let ink = kit::ink();
    let mut clicked = None;
    let shown = egui::Area::new(ui.id().with(("word-list", name)))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(query) = query {
                    search_display(ui, inner, query);
                    ui.add_space(6.0 * z);
                }
                if found.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(inner, row), Sense::hover());
                    let size = 12.5 * z;
                    let galley = kit::words(ui, "No command has that name", size, ink.i400, inner);
                    let left = rect.left() + 12.0 * z;
                    kit::draw_by_capitals(
                        ui.painter(),
                        left,
                        rect.center().y,
                        galley,
                        size,
                        ink.i400,
                    );
                }
                for (offset, action) in found.iter().skip(first).take(ROWS).enumerate() {
                    let index = first + offset;
                    if command_row(ui, inner, action, index == chosen, (name, index)) {
                        clicked = Some(action.id);
                    }
                }
                if query.is_some() {
                    ui.add_space(6.0 * z);
                    keys_line(ui, inner);
                }
            });
        });
    remember_popup(ui, shown.response.rect);
    clicked
}

/// The command search's well: the search mark, what was typed and a caret, or the hint while nothing is.
fn search_display(ui: &mut egui::Ui, width: f32, query: &str) {
    let z = kit::zoom(ui.ctx());
    let ink = kit::ink();
    let (well, response) = ui.allocate_exact_size(Vec2::new(width, 38.0 * z), Sense::hover());
    kit::layer(ui, "command-search-well", well, |rux| {
        kit::well_surface(rux, well, well.height() / 2.0);
        rux.mark(
            rux::Mark::new(rux::Icon::Search, 15.0 * z),
            Pos2::new(well.left() + 19.0 * z, well.center().y),
            ink.i400,
        );
    });
    let size = 13.5 * z;
    let left = well.left() + 38.0 * z;
    let room = well.right() - 16.0 * z - left;
    let (words, colour) = match query.is_empty() {
        true => ("Find a command", ink.i400),
        false => (query, ink.i900),
    };
    let galley = kit::words(ui, words, size, colour, room);
    let end = left + if query.is_empty() { 0.0 } else { galley.size().x };
    kit::draw_by_capitals(ui.painter(), left, well.center().y, galley, size, colour);
    // A caret that blinks, as a field's does, so the well reads as a place to type.
    let time = ui.input(|i| i.time);
    if (time * 1.6).fract() < 0.6 {
        let middle = well.center().y;
        ui.painter().line_segment(
            [Pos2::new(end + 1.5, middle - 8.0 * z), Pos2::new(end + 1.5, middle + 8.0 * z)],
            Stroke::new(1.5 * z, kit::chat().sky),
        );
    }
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Find a command");
        info.current_text_value = Some(query.to_owned());
        info
    });
}

/// One command in the list: its name, and the panel level it belongs to at the right in the quiet ink.
fn command_row(
    ui: &mut egui::Ui,
    width: f32,
    action: &actions::Action,
    chosen: bool,
    salt: (&str, usize),
) -> bool {
    let z = kit::zoom(ui.ctx());
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(width, controls::MENU_ROW * z), Sense::hover());
    let response = ui.interact(rect, ui.id().with(("word-list-row", salt)), Sense::click());
    let chat = kit::chat();
    let ink = kit::ink();
    if chosen || response.hovered() {
        let painter = ui.painter();
        let corner = egui::CornerRadius::same((10.0 * z).round() as u8);
        painter.rect_filled(rect, corner, chat.well);
        painter.rect_stroke(
            rect.shrink(0.5),
            corner,
            Stroke::new(1.0, chat.dark.gamma_multiply(0.55)),
            egui::StrokeKind::Inside,
        );
    }
    let quiet = 11.5 * z;
    let level = kit::words(ui, action.level.name(), quiet, ink.i400, width * 0.4);
    let level_left = rect.right() - 12.0 * z - level.size().x;
    kit::draw_by_capitals(ui.painter(), level_left, rect.center().y, level, quiet, ink.i400);
    let size = 12.5 * z;
    let room = level_left - rect.left() - 24.0 * z;
    let label = kit::words(ui, action.label, size, ink.i900, room);
    let left = rect.left() + 12.0 * z;
    kit::draw_by_capitals(ui.painter(), left, rect.center().y, label, size, ink.i900);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let named = action.label;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, named));
    response.clicked()
}

/// The line under the command search naming its keys, each in a small carved cap.
fn keys_line(ui: &mut egui::Ui, width: f32) {
    let z = kit::zoom(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0 * z), Sense::hover());
    let ink = kit::ink();
    let chat = kit::chat();
    let size = 11.0 * z;
    let middle = rect.center().y;
    let mut x = rect.left() + 10.0 * z;
    for (key, what) in [("Up", ""), ("Down", "to choose"), ("Enter", "to run"), ("Esc", "to close")]
    {
        let cap = kit::words(ui, key, 10.5 * z, ink.i700, 80.0 * z);
        let cap_rect = Rect::from_min_size(
            Pos2::new(x, middle - 9.0 * z),
            Vec2::new(cap.size().x + 12.0 * z, 18.0 * z),
        );
        let corner = egui::CornerRadius::same((6.0 * z).round() as u8);
        let painter = ui.painter();
        painter.rect_filled(cap_rect, corner, chat.well);
        painter.rect_stroke(
            cap_rect,
            corner,
            Stroke::new(1.0, chat.dark.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        kit::draw_by_capitals(painter, cap_rect.left() + 6.0 * z, middle, cap, 10.5 * z, ink.i700);
        x = cap_rect.right() + 5.0 * z;
        if !what.is_empty() {
            let words = kit::words(ui, what, size, ink.i400, 120.0 * z);
            let next = x + words.size().x + 14.0 * z;
            kit::draw_by_capitals(ui.painter(), x, middle, words, size, ink.i400);
            x = next;
        }
    }
}

/// `Ctrl+/`: every command, searched by name.
fn palette(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    // Typed letters filter; keys::type_text leaves them alone while the search is open.
    let typed: String = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    });
    let Some((query, chosen)) = doc.view.palette.as_mut() else { return };
    if !typed.is_empty() {
        query.push_str(&typed);
        *chosen = 0;
    }
    let (query, chosen) = (query.clone(), *chosen);
    let found = actions::search(&query, false)
        .into_iter()
        .filter(|a| actions::applies(a.id, doc))
        .collect::<Vec<_>>();
    let z = kit::zoom(ui.ctx());
    let width = (460.0 * z).min(area.width() - 32.0);
    let at = Pos2::new(area.center().x - width / 2.0, area.top() + 12.0 * z);
    let menu = Menu { name: "palette", at, width, found: &found, chosen, query: Some(&query) };
    if let Some(id) = list(ui, menu) {
        doc.view.palette = None;
        super::run_action(doc, id, requests);
    }
}

/// The `/` menu, under the caret.
fn slash(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    let Some((at, query, chosen)) = doc.view.slash.clone() else { return };
    // The menu closes when the caret leaves the paragraph it was opened in.
    if doc.session.selection.head.path != at.path {
        doc.view.slash = None;
        return;
    }
    let found = actions::search(&query, true)
        .into_iter()
        .filter(|a| actions::applies(a.id, doc))
        .collect::<Vec<_>>();
    let z = kit::zoom(ui.ctx());
    let place = caret_point(doc, area).unwrap_or(area.center());
    let width = 290.0 * z;
    let tall = (ROWS as f32 * controls::MENU_ROW + 24.0) * z;
    let place = Pos2::new(
        place.x.min(area.right() - width - 8.0),
        (place.y + 6.0).min(area.bottom() - tall),
    );
    let menu = Menu { name: "slash", at: place, width, found: &found, chosen, query: None };
    if let Some(id) = list(ui, menu) {
        run_slash(doc, &at, &query, id, requests);
    }
}

/// Where the caret is on the screen, below its line.
fn caret_point(doc: &Doc, area: Rect) -> Option<Pos2> {
    let (index, line) = doc.pages.find(&doc.session.selection.head)?;
    let (tops, _) = super::page_view::page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    Some(
        origin
            + Vec2::new(
                line.x_of(doc.session.selection.head.offset) * zoom,
                (line.top + line.height) * zoom,
            ),
    )
}

/// Paste Options under the caret straight after a paste: Keep Source Formatting, Merge Formatting and
/// Keep Text Only, as Word offers them, in the window's menu card. It goes away once the caret moves.
fn paste_options(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    use unluminous_word::clipboard::PasteAs;
    let _ = look;
    let Some(at) = caret_point(doc, area) else { return };
    let z = kit::zoom(ui.ctx());
    let current = doc.view.pasted.as_ref().map_or(PasteAs::Source, |p| p.how);
    let choices = [
        (PasteAs::Source, "Keep Source Formatting"),
        (PasteAs::Merge, "Merge Formatting"),
        (PasteAs::Text, "Keep Text Only"),
    ];
    let width = 230.0 * z;
    let place = Pos2::new(at.x.clamp(area.left() + 4.0, area.right() - width - 4.0), at.y + 6.0);
    if place.y > area.bottom() {
        return;
    }
    let mut chosen = None;
    let shown = egui::Area::new(ui.id().with("word-paste-options"))
        .order(egui::Order::Foreground)
        .fixed_pos(place)
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).show(ui, |ui| {
                ui.set_width(width - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                for (how, label) in choices {
                    if controls::menu_row(ui, label, "", true, how == current, 0.0) {
                        chosen = Some(how);
                    }
                }
            });
        });
    remember_popup(ui, shown.response.rect);
    if let Some(how) = chosen {
        if let Err(problem) = crate::services::word::paste_again(doc, how) {
            requests.push(Request::Message(problem));
        }
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The small card over a selection: bold, italic, underline, highlight, link and comment, as one carved
/// track of marks, the ones that are on raised out of it.
fn bubble(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    let start = doc.session.selection.ordered().0.clone();
    let Some((index, line)) = doc.pages.find(&start) else { return };
    let (tops, _) = super::page_view::page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    let z = kit::zoom(ui.ctx());
    let run = doc.session.look_at_caret();
    let buttons: [(&str, &str, kit::Mark, bool); 6] = [
        ("bold", "Bold", kit::Mark::Svg(&svg_icon::BOLD), run.bold),
        ("italic", "Italic", kit::Mark::Svg(&svg_icon::ITALIC), run.italic),
        (
            "underline",
            "Underline",
            kit::Mark::Svg(&svg_icon::UNDERLINE),
            run.underline != unluminous_word::props::Underline::None,
        ),
        ("highlight", "Highlight", kit::Mark::Svg(&svg_icon::HIGHLIGHTER), run.highlight.is_some()),
        ("link", "Link (Ctrl+K)", kit::Mark::Svg(&svg_icon::LINK), false),
        ("comment", "Comment", kit::Mark::Rux(rux::Icon::Chat), false),
    ];
    let option = Vec2::new(32.0 * z, 28.0 * z);
    let track_width = buttons.len() as f32 * (option.x + 2.0 * z) + 4.0 * z;
    let card = Vec2::new(track_width + 16.0, option.y + 6.0 * z + 16.0);
    let x = (origin.x + line.x_of(start.offset) * zoom)
        .clamp(area.left() + 4.0, area.right() - card.x - 4.0);
    let y = origin.y + line.top * zoom - card.y - 8.0 * z;
    if y < area.top() || y > area.bottom() {
        return;
    }
    let mut pressed = None;
    let shown = egui::Area::new(ui.id().with("word-bubble"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(x, y))
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).show(ui, |ui| {
                let (track, _) = ui.allocate_exact_size(
                    Vec2::new(track_width, option.y + 6.0 * z),
                    Sense::hover(),
                );
                let rects: Vec<Rect> = (0..buttons.len())
                    .map(|n| {
                        Rect::from_min_size(
                            Pos2::new(
                                track.left() + 3.0 * z + n as f32 * (option.x + 2.0 * z),
                                track.top() + 3.0 * z,
                            ),
                            option,
                        )
                    })
                    .collect();
                let responses: Vec<egui::Response> = buttons
                    .iter()
                    .zip(&rects)
                    .map(|((id, name, ..), at)| {
                        ui.interact(*at, ui.id().with(("word-bubble", *id)), Sense::click())
                            .with_hint(*name)
                    })
                    .collect();
                let ink = kit::ink();
                kit::layer(ui, "bubble-track", track, |rux| {
                    kit::well_surface(rux, track, track.height() / 2.0);
                    let chat = rux.theme().chat;
                    for (((_, _, mark, on), at), response) in
                        buttons.iter().zip(&rects).zip(&responses)
                    {
                        if *on {
                            kit::chip_surface(rux, *at, at.height() / 2.0, false);
                        }
                        let tint = match (*on, response.hovered()) {
                            (true, _) => ink.i900,
                            (false, true) => chat.sky,
                            (false, false) => ink.i500,
                        };
                        kit::paint_mark(rux, *mark, at.center(), 15.0 * z, tint);
                    }
                });
                for ((id, name, _, on), response) in buttons.iter().zip(responses) {
                    let (on, name) = (*on, *name);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name)
                    });
                    if response.clicked() {
                        pressed = Some(*id);
                    }
                }
            });
        });
    remember_popup(ui, shown.response.rect);
    let Some(id) = pressed else { return };
    match id {
        "highlight" => {
            let on = run.highlight.is_none();
            doc.session.format(move |rpr| {
                unluminous_word::props::set_val(rpr, "w:highlight", on.then_some("yellow"))
            });
        }
        "link" => {
            doc.view.panel = vec![Level::Root, Level::Insert];
            doc.view.fields.insert("link", String::new());
        }
        other => super::run_action(doc, other, requests),
    }
    requests.push(Request::TakeTheKeyboard(true));
}

/// What a press in the find card asked for.
#[derive(Default)]
struct FindAsked {
    close: bool,
    go: Option<bool>,
    replace_one: bool,
    replace_all: bool,
}

/// Find, and Replace with it, in a card at the top right of the pages: a field with how many were found
/// in it, the four ways a search can be narrowed as pills, and round chips for the previous match, the
/// next and closing.
fn find_bar(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    let Some(find) = doc.view.find.clone() else { return };
    let search = find.search();
    let (matches, problem) = match doc.session.search(&search) {
        Ok(found) => (found, None),
        Err(problem) => (Vec::new(), Some(problem)),
    };
    let count = match (find.needle.is_empty(), problem) {
        (true, _) => String::new(),
        (false, Some(problem)) => problem,
        (false, None) => format!("{} found", matches.len()),
    };
    let z = kit::zoom(ui.ctx());
    let width = (452.0 * z).min(area.width() - 24.0);
    let at = Pos2::new(area.right() - width - 14.0 * z, area.top() + 12.0 * z);
    let mut asked = FindAsked::default();
    let shown = egui::Area::new(ui.id().with("word-find"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            controls::menu_frame(ui).show(ui, |ui| {
                if let Some(find) = doc.view.find.as_mut() {
                    find_rows(ui, width - 16.0, find, &count, &mut asked);
                }
            });
        });
    remember_popup(ui, shown.response.rect);
    if let Some(forward) = asked.go {
        if !matches.is_empty() {
            let head = doc.session.selection.ordered().1.clone();
            let next = if forward {
                matches.iter().position(|m| m.anchor >= head).unwrap_or(0)
            } else {
                matches
                    .iter()
                    .rposition(|m| m.head < *doc.session.selection.ordered().0)
                    .unwrap_or(matches.len() - 1)
            };
            doc.session.selection = matches[next].clone();
            doc.view.reveal = true;
        }
    }
    if asked.replace_one && !matches.is_empty() {
        let current = matches.iter().position(|m| *m == doc.session.selection).unwrap_or(0);
        let chosen = matches[current].clone();
        let text = unluminous_word::position::paragraph(
            &doc.session.document,
            &chosen.anchor.story,
            &chosen.anchor.path,
        )
        .map(|p| p.flat_text())
        .unwrap_or_default();
        let groups = unluminous_word::find::find_all(&search.query, &text)
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.start == chosen.anchor.offset)
            .map(|f| f.groups)
            .unwrap_or_default();
        let hit = unluminous_word::find::Found {
            start: chosen.anchor.offset,
            end: chosen.head.offset,
            groups,
        };
        let new = unluminous_word::find::replacement(&search.query, &find.with, &text, &hit);
        doc.session.selection = chosen;
        doc.session.type_text(&new);
    }
    if asked.replace_all {
        match doc.session.replace_matches(&search, &find.with, &Default::default()) {
            Ok(count) => requests.push(Request::Message(format!("{count} replaced"))),
            Err(problem) => requests.push(Request::Message(problem)),
        }
    }
    if asked.close {
        doc.view.find = None;
        requests.push(Request::TakeTheKeyboard(true));
    }
}

/// The find card's one or two rows.
fn find_rows(
    ui: &mut egui::Ui,
    width: f32,
    find: &mut crate::services::word::Find,
    count: &str,
    asked: &mut FindAsked,
) {
    let z = kit::zoom(ui.ctx());
    let height = 34.0 * z;
    let chip = 28.0 * z;
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let middle = row.center().y;
    // From the right: close, next, previous, then the four pills, then the field takes what is left.
    let mut x = row.right();
    let chip_at = |x: &mut f32| {
        *x -= chip;
        let rect = Rect::from_min_size(Pos2::new(*x, middle - chip / 2.0), Vec2::splat(chip));
        *x -= 6.0 * z;
        rect
    };
    let close = chip_at(&mut x);
    let next = chip_at(&mut x);
    let previous = chip_at(&mut x);
    let cross = kit::Mark::Rux(rux::Icon::Cross);
    if kit::round_chip(ui, close, "Close Find", "Close", cross, false).clicked() {
        asked.close = true;
    }
    if kit::round_chip(ui, next, "Next match", "Next", kit::Mark::Rux(rux::Icon::ChevDown), false)
        .clicked()
    {
        asked.go = Some(true);
    }
    let up = kit::Mark::Svg(&UP);
    if kit::round_chip(ui, previous, "Previous match", "Previous", up, false).clicked() {
        asked.go = Some(false);
    }
    x -= 4.0 * z;
    let pills: [(&str, &str, &mut bool); 4] = [
        ("Aa", "Match case", &mut find.case),
        ("W", "Whole words", &mut find.word),
        (
            "?*",
            "Use wildcards: ? one character, * any, [a-z], (group) and \\1",
            &mut find.wildcards,
        ),
        (
            "All",
            "Also search headers, footers, notes, comments and text boxes",
            &mut find.every_story,
        ),
    ];
    for (label, hint, value) in pills.into_iter().rev() {
        let w = kit::pill_width(ui, label).max(30.0 * z);
        x -= w;
        let rect = Rect::from_min_size(Pos2::new(x, middle - 13.0 * z), Vec2::new(w, 26.0 * z));
        x -= 4.0 * z;
        if kit::pill_at(ui, rect, label, hint, *value, true).with_hint(hint).clicked() {
            *value = !*value;
        }
    }
    let well = Rect::from_min_max(row.min, Pos2::new(x - 4.0 * z, row.max.y));
    let field = search_field(
        ui,
        well,
        "Find",
        &mut find.needle,
        "Find",
        Some(kit::Mark::Rux(rux::Icon::Search)),
    );
    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        asked.go = Some(true);
    }
    if !field.has_focus() && find.needle.is_empty() {
        field.request_focus();
    }
    if !count.is_empty() {
        let ink = kit::ink();
        let size = 11.0 * z;
        let galley = kit::words(ui, count, size, ink.i400, well.width() * 0.4);
        let left = well.right() - 14.0 * z - galley.size().x;
        kit::draw_by_capitals(ui.painter(), left, middle, galley, size, ink.i400);
    }
    if !find.replace {
        return;
    }
    ui.add_space(6.0 * z);
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let middle = row.center().y;
    let all_width = kit::pill_width(ui, "Replace All");
    let one_width = kit::pill_width(ui, "Replace");
    let all = Rect::from_min_size(
        Pos2::new(row.right() - all_width, middle - kit::PILL * z / 2.0),
        Vec2::new(all_width, kit::PILL * z),
    );
    let one = Rect::from_min_size(
        Pos2::new(all.left() - 6.0 * z - one_width, middle - kit::PILL * z / 2.0),
        Vec2::new(one_width, kit::PILL * z),
    );
    asked.replace_one = kit::pill_at(ui, one, "Replace", "Replace", false, true).clicked();
    asked.replace_all = kit::pill_at(ui, all, "Replace All", "Replace All", false, true).clicked();
    let well = Rect::from_min_max(row.min, Pos2::new(one.left() - 8.0 * z, row.max.y));
    search_field(ui, well, "Replace with", &mut find.with, "Replace with", None);
}

/// A chevron pointing up, for the previous match: `rux`'s own chevron is drawn pointing down.
static UP: svg_icon::SvgMark =
    svg_icon::SvgMark { view: 24.0, stroke: 1.9, parts: &[svg_icon::Part::Path("m18 15-6-6-6 6")] };
