//! Spell check on the pages: the red wavy line under a misspelled or repeated word, the right click on
//! one, and the card that offers what it could be, Ignore All and Add to Dictionary.

use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_word::position::{self, Pos};
use unluminous_word::spell::Kind;

use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{spelling, Doc};

/// The wavy lines under one page's underlined words. The word the caret is in is left alone while it is
/// typed, because Word waits until the word is finished.
pub fn paint(painter: &egui::Painter, doc: &Doc, index: usize, origin: Pos2, zoom: f32) {
    let Some(page) = doc.pages.pages.get(index) else { return };
    let selection = &doc.session.selection;
    let typing = selection.is_empty().then_some(&selection.head);
    for line in &page.carets {
        let Some(paragraph) = position::paragraph(&doc.session.document, &line.story, &line.path)
        else {
            continue;
        };
        let found = spelling::underlined(paragraph);
        for (range, _) in found.iter() {
            if range.end <= line.start || range.start >= line.end {
                continue;
            }
            let in_it = typing.is_some_and(|at| {
                at.story == line.story
                    && at.path == line.path
                    && range.start <= at.offset
                    && at.offset <= range.end
            });
            if in_it {
                continue;
            }
            let from = line.x_of(range.start.max(line.start));
            let to = line.x_of(range.end.min(line.end));
            let at = origin + Vec2::new(from * zoom, (line.baseline + 1.5) * zoom);
            wavy(painter, at, (to - from) * zoom, zoom);
        }
    }
}

/// A red wavy line `width` long starting at `at`.
fn wavy(painter: &egui::Painter, at: Pos2, width: f32, zoom: f32) {
    if width <= 0.0 {
        return;
    }
    let step = (1.6 * zoom).max(1.2);
    let height = (1.1 * zoom).max(0.9);
    let mut points = Vec::new();
    let mut x = 0.0;
    let mut up = false;
    while x < width {
        points.push(at + Vec2::new(x, if up { -height } else { height } * 0.5));
        x += step;
        up = !up;
    }
    points.push(at + Vec2::new(width, if up { -height } else { height } * 0.5));
    painter.add(Shape::line(
        points,
        Stroke::new((0.75 * zoom).max(1.0), crate::theme::color::word_spelling()),
    ));
}

/// A right click on the pages. On an underlined word it selects the word and opens its card. Anywhere
/// else outside the selection it moves the caret there, so the bubble that follows is for that place.
pub fn right_click(doc: &mut Doc, at: Pos) {
    doc.view.slash = None;
    doc.view.spell_menu = None;
    if doc.view.spelling {
        if let Some(word) = spelling::underlined_at(doc, &at) {
            spelling::open_menu(doc, word);
            return;
        }
    }
    let (from, to) = doc.session.selection.ordered();
    let inside = at.story == from.story && *from <= at && at <= *to;
    if !inside {
        doc.session.select(at.clone(), at);
    }
}

/// One row of the card: what it says, and what it does.
enum Choice {
    Replace(String),
    DeleteRepeated,
    Ignore,
    Add,
}

/// The card under an underlined word, when one is open.
pub fn show_menu(
    ui: &mut egui::Ui,
    look: &Look<'_>,
    doc: &mut Doc,
    area: Rect,
    requests: &mut Vec<Request>,
) {
    let _ = look;
    if !spelling::menu_is_current(doc) {
        doc.view.spell_menu = None;
        return;
    }
    let Some(menu) = doc.view.spell_menu.clone() else { return };
    let start = Pos {
        story: menu.word.story.clone(),
        path: menu.word.path.clone(),
        offset: menu.word.range.start,
    };
    let Some((index, line)) = doc.pages.find(&start) else { return };
    let (tops, _) = super::page_view::page_tops(doc, area.width());
    let zoom = doc.view.zoom;
    let origin = area.min + Vec2::new(tops[index].x, tops[index].y - doc.view.scroll);
    let mut rows: Vec<(String, Choice)> = match menu.word.kind {
        Kind::Spelling if menu.suggestions.is_empty() => Vec::new(),
        Kind::Spelling => {
            menu.suggestions.iter().map(|s| (s.clone(), Choice::Replace(s.clone()))).collect()
        }
        Kind::Repeated => vec![("Delete Repeated Word".to_owned(), Choice::DeleteRepeated)],
    };
    rows.push(("Ignore All".to_owned(), Choice::Ignore));
    if menu.word.kind == Kind::Spelling {
        rows.push(("Add to Dictionary".to_owned(), Choice::Add));
    }
    let z = super::kit::zoom(ui.ctx());
    let title = title_of(&menu);
    let labels: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    let size = card_size(ui, &title, &labels, area.width());
    let (row, head, width) = (size.row, size.head, size.width);
    let height = head + row * rows.len() as f32 + 16.0;
    let below =
        origin + Vec2::new(line.x_of(start.offset) * zoom, (line.top + line.height) * zoom + 4.0);
    let x = below.x.clamp(area.left() + 4.0, (area.right() - width - 4.0).max(area.left() + 4.0));
    // Above the word when there is no room under it.
    let y = if below.y + height > area.bottom() {
        origin.y + line.top * zoom - height - 4.0
    } else {
        below.y
    };
    let ink = super::kit::ink();
    let mut chosen = None;
    let shown = egui::Area::new(ui.id().with("word-spelling"))
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(x, y))
        .show(ui.ctx(), |ui| {
            crate::components::controls::menu_frame(ui).show(ui, |ui| {
                ui.set_width(width - 16.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                let (top, _) =
                    ui.allocate_exact_size(Vec2::new(width - 16.0, head), Sense::hover());
                let small = 11.5 * z;
                let galley = super::kit::words(ui, &title, small, ink.i400, top.width() - 20.0);
                let left = top.left() + 10.0 * z;
                super::kit::draw_by_capitals(
                    ui.painter(),
                    left,
                    top.center().y,
                    galley,
                    small,
                    ink.i400,
                );
                for (label, choice) in rows {
                    let strong = matches!(choice, Choice::Replace(_));
                    let clicked = match strong {
                        true => super::kit::styled_row(ui, &label, 12.5 * z, true, false),
                        false => {
                            crate::components::controls::menu_row(ui, &label, "", true, false, 0.0)
                        }
                    };
                    if clicked {
                        chosen = Some(choice);
                    }
                }
            });
        });
    super::overlays::remember_popup(ui, shown.response.rect);
    if let Some(choice) = chosen {
        choose(doc, choice, &menu.word.word, requests);
    }
}

/// The line at the top of the card, which says what is wrong with the word.
fn title_of(menu: &crate::services::word::SpellMenu) -> String {
    match menu.word.kind {
        Kind::Spelling if menu.suggestions.is_empty() => {
            format!("“{}”: no suggestions", menu.word.word)
        }
        Kind::Spelling => format!("“{}” is not in the dictionary", menu.word.word),
        Kind::Repeated => format!("“{}” is repeated", menu.word.word),
    }
}

/// How tall a row and the top line are, and how wide the card is.
struct CardSize {
    row: f32,
    head: f32,
    width: f32,
}

/// Size the card to what it says (`task-2341`): at least 240 points wide, wide enough for its longest line,
/// and never wider than the area it is drawn in. The sizes are the design's times the tab's zoom
/// (`task-2366`).
fn card_size(ui: &egui::Ui, title: &str, labels: &[&str], room: f32) -> CardSize {
    let z = super::kit::zoom(ui.ctx());
    let measure = |text: &str, size: f32| {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::PLACEHOLDER)
                .size()
                .x
        })
    };
    let title_width = measure(title, 11.5 * z) + 40.0 * z;
    let label_width =
        labels.iter().map(|label| measure(label, 12.5 * z) + 72.0 * z).fold(0.0, f32::max);
    let width = (240.0 * z).max(title_width).max(label_width).min((room - 8.0).max(240.0 * z));
    CardSize { row: crate::components::controls::MENU_ROW * z, head: 28.0 * z, width }
}

/// Do what a row of the card says.
fn choose(doc: &mut Doc, choice: Choice, word: &str, requests: &mut Vec<Request>) {
    match choice {
        Choice::Replace(with) => spelling::replace(doc, &with),
        Choice::DeleteRepeated => spelling::delete_repeated(doc),
        Choice::Ignore => {
            spelling::ignore(word);
            doc.view.spell_menu = None;
        }
        Choice::Add => {
            if let Err(problem) = spelling::add_word(word) {
                requests.push(Request::Notice {
                    text: problem,
                    kind: crate::components::toast::Kind::Problem,
                });
            }
            doc.view.spell_menu = None;
        }
    }
    requests.push(Request::TakeTheKeyboard(true));
}
