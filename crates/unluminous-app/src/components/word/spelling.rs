//! Spell check on the pages: the red wavy line under a misspelled or repeated word, the right click on
//! one, and the card that offers what it could be, Ignore All and Add to Dictionary.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
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
    let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("word-spelling"));
    let painter = ui.ctx().layer_painter(layer);
    let title = title_of(&menu);
    let labels: Vec<&str> = rows.iter().map(|(label, _)| label.as_str()).collect();
    let size = card_size(&painter, look, &title, &labels, area.width());
    let (row, head, width) = (size.row, size.head, size.width);
    let scale = look.scale();
    let height = head + row * rows.len() as f32 + 8.0 * scale;
    let below =
        origin + Vec2::new(line.x_of(start.offset) * zoom, (line.top + line.height) * zoom + 4.0);
    let x = below.x.clamp(area.left() + 4.0, (area.right() - width - 4.0).max(area.left() + 4.0));
    // Above the word when there is no room under it.
    let y = if below.y + height > area.bottom() {
        origin.y + line.top * zoom - height - 4.0
    } else {
        below.y
    };
    let card = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
    super::overlays::remember_popup(ui, card);
    painter.rect_filled(card.translate(Vec2::new(0.0, 3.0)), 8.0, Color32::from_black_alpha(80)); // any ground: the shadow under a card
    painter.rect(
        card,
        8.0,
        look.palette.menu,
        Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        card.left_top() + Vec2::new(12.0, head / 2.0 + 2.0),
        Align2::LEFT_CENTER,
        title,
        title_font(look),
        look.palette.text_dim,
    );
    let mut chosen = None;
    for (n, (label, choice)) in rows.into_iter().enumerate() {
        let r = Rect::from_min_size(
            card.left_top() + Vec2::new(4.0, head + 4.0 * scale + row * n as f32),
            Vec2::new(width - 8.0, row),
        );
        let response = ui.interact(r, ui.id().with(("word-spelling-row", n)), Sense::click());
        if response.hovered() {
            painter.rect_filled(r, 5.0, look.palette.selected_row);
        }
        let strong = matches!(choice, Choice::Replace(_));
        let font = FontId::proportional(look.font_size);
        let ink = if strong { look.palette.text_strong } else { look.palette.text };
        painter.text(r.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, &label, font, ink);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        if response.clicked() {
            chosen = Some(choice);
        }
    }
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

/// The font of the card's top line.
fn title_font(look: &Look<'_>) -> FontId {
    FontId::proportional(look.font_size * 0.85)
}

/// How tall a row and the top line are, and how wide the card is.
struct CardSize {
    row: f32,
    head: f32,
    width: f32,
}

/// Size the card to what it says (`task-2341`). The text follows the editor's font size, so a fixed width
/// let the top line run out past the card's right edge at a large font. The card is at least 220 points
/// wide, wide enough for its longest line, and never wider than the area it is drawn in.
fn card_size(
    painter: &egui::Painter,
    look: &Look<'_>,
    title: &str,
    labels: &[&str],
    room: f32,
) -> CardSize {
    let scale = look.scale();
    let measure = |text: &str, font: FontId| {
        painter.layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER).size().x
    };
    let title_width = measure(title, title_font(look)) + 24.0;
    let label_width = labels
        .iter()
        .map(|label| measure(label, FontId::proportional(look.font_size)) + 24.0)
        .fold(0.0, f32::max);
    let width =
        (220.0 * scale).max(title_width).max(label_width).min((room - 8.0).max(220.0 * scale));
    CardSize {
        row: (26.0 * scale).max(look.font_size * 1.6),
        head: (26.0 * scale).max(look.font_size * 0.85 * 1.8),
        width,
    }
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
