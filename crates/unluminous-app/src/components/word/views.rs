//! The ruler over the pages and the status area under them (task-2359, TDD 5.5).
//!
//! The ruler is in inches from the left margin of the page the caret is on. It shows the margins, the
//! paragraph's first line, hanging, left and right indents and its tab stops, and each can be dragged:
//! a press on the ruler adds a tab stop of the kind the selector at its left shows, a stop dragged off
//! the ruler is taken away. The status area says the page, the words, the language and whether changes
//! are tracked, holds the three views and the zoom, and a press on each opens what it is about.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_word::props::TabKind;

use crate::components::controls::WithHint;
use crate::services::plugin_ui::{Look, Request};
use crate::services::word::{Doc, Level, Mode};

/// How tall the ruler is, before the tab's zoom.
pub const RULER: f32 = 24.0;
/// How tall the status area is, before the tab's zoom.
pub const STATUS: f32 = 26.0;

/// What a drag on the ruler holds.
#[derive(Debug, Clone, PartialEq)]
pub enum RulerGrip {
    First,
    Hanging,
    Left,
    Right,
    Tab(f32),
}

/// A drag on the ruler, while it lasts: what is held and where it is, in points from the column's left.
#[derive(Debug, Clone, PartialEq)]
pub struct RulerDrag {
    pub grip: RulerGrip,
    pub at: f32,
    /// The pointer went more than a ruler's height away, so a tab stop let go there is taken away.
    pub off: bool,
}

/// The tab kinds the selector cycles through, in Word's order.
const KINDS: [TabKind; 5] = [TabKind::Left, TabKind::Center, TabKind::Right, TabKind::Decimal, TabKind::Bar];

fn kind_label(kind: TabKind) -> &'static str {
    match kind {
        TabKind::Left => "Left tab",
        TabKind::Center => "Center tab",
        TabKind::Right => "Right tab",
        TabKind::Decimal => "Decimal tab",
        TabKind::Bar => "Bar tab",
        TabKind::Clear => "Clear",
    }
}

/// Draw a tab stop's mark: an L, its mirror, an upside down T, the T with a dot, or a bar.
fn tab_mark(painter: &egui::Painter, x: f32, base: f32, kind: TabKind, ink: Color32) {
    let stroke = Stroke::new(1.5, ink);
    let top = base - 7.0;
    painter.line_segment([Pos2::new(x, top), Pos2::new(x, base)], stroke);
    match kind {
        TabKind::Left => {
            painter.line_segment([Pos2::new(x, base), Pos2::new(x + 6.0, base)], stroke);
        }
        TabKind::Right => {
            painter.line_segment([Pos2::new(x - 6.0, base), Pos2::new(x, base)], stroke);
        }
        TabKind::Center | TabKind::Decimal => {
            painter.line_segment([Pos2::new(x - 4.0, base), Pos2::new(x + 4.0, base)], stroke);
            if kind == TabKind::Decimal {
                painter.circle_filled(Pos2::new(x + 3.0, base - 3.0), 1.2, ink);
            }
        }
        TabKind::Bar | TabKind::Clear => {}
    }
}

/// The ruler: `rect` is its strip, `pages` the pages' area it sits over.
pub fn ruler(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, rect: Rect, pages: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, look.ground(look.palette.panel));
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, look.palette.divider));
    // The tab selector, at the ruler's left, cycling through the kinds a press adds.
    let selector = Rect::from_min_size(rect.min + Vec2::new(4.0, 3.0), Vec2::splat(rect.height() - 6.0));
    let response = ui.interact(selector, ui.id().with(("word-ruler-kind", &doc.key)), Sense::click());
    painter.rect_stroke(selector, 3.0, Stroke::new(1.0, look.palette.divider), egui::StrokeKind::Inside);
    tab_mark(&painter, selector.center().x - 1.0, selector.center().y + 4.0, doc.view.ruler_tab, look.palette.text_strong);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Tab kind"));
    let kind_now = doc.view.ruler_tab;
    if response.with_hint(format!("{}: press to change", kind_label(kind_now))).clicked() {
        let next = KINDS.iter().position(|k| *k == kind_now).map_or(0, |i| (i + 1) % KINDS.len());
        doc.view.ruler_tab = KINDS[next];
    }
    let Some((index, _)) = doc.pages.find(&doc.session.selection.head) else { return };
    let (tops, _) = super::page_view::page_tops(doc, pages.width());
    let Some(top) = tops.get(index) else { return };
    let page = &doc.pages.pages[index];
    let zoom = doc.view.zoom;
    let page_left = pages.left() + top.x;
    let column_left = page_left + page.body.x * zoom;
    let column_right = page_left + (page.body.x + page.body.w) * zoom;
    // The page as a light band, the margins as darker ones beside it.
    let band = Rect::from_x_y_ranges(page_left..=page_left + page.width * zoom, rect.top() + 5.0..=rect.bottom() - 5.0);
    let paper = crate::theme::color::word_paper();
    painter.rect_filled(band, 1.0, paper.gamma_multiply(0.55));
    painter.rect_filled(Rect::from_x_y_ranges(column_left..=column_right, band.y_range()), 0.0, paper);
    // Ticks every eighth of an inch from the left margin, numbered every inch.
    let ink = Color32::from_gray(90); // any ground: drawn on the paper band
    let inch = 72.0 * zoom;
    let mut n = ((page_left - column_left) / (inch / 8.0)).floor() as i32;
    loop {
        let x = column_left + n as f32 * inch / 8.0;
        if x > page_left + page.width * zoom {
            break;
        }
        if x >= page_left {
            let tall = if n % 8 == 0 { 0.0 } else if n % 4 == 0 { 5.0 } else { 3.0 };
            if n % 8 == 0 && n != 0 {
                painter.text(Pos2::new(x, band.center().y), Align2::CENTER_CENTER, (n / 8).abs().to_string(), FontId::proportional(9.0), ink);
            } else if tall > 0.0 {
                painter.line_segment([Pos2::new(x, band.center().y - tall / 2.0), Pos2::new(x, band.center().y + tall / 2.0)], Stroke::new(1.0, ink));
            }
        }
        n += 1;
    }
    let look_now = doc.session.paragraph_at_caret();
    let to_x = |points: f32| column_left + points * zoom;
    let to_points = |x: f32| (x - column_left) / zoom;
    // A drag shows where it is instead of where the paragraph says.
    let dragging = doc.view.ruler_drag.clone();
    let (left, right, first) = match &dragging {
        Some(drag) => after_drag(drag, &look_now, page.body.w),
        None => (look_now.indent_left, look_now.indent_right, look_now.indent_first),
    };
    let accent = Color32::from_gray(60); // any ground: indent marks sit on the paper band
    let first_x = to_x(left + first);
    let hang_x = to_x(left);
    let right_x = to_x(page.body.w - right);
    let (y_top, y_bottom) = (band.top(), band.bottom());
    doc.view.ruler_marks = Some([first_x, hang_x, right_x, y_top, y_bottom]);
    // First line: a triangle pointing down from the top. Hanging: one pointing up, with the left indent's
    // box under it. Right: one pointing up at the right.
    let triangle = |x: f32, down: bool| {
        let (a, b) = if down { (y_top, y_top + 6.0) } else { (y_bottom - 2.0, y_bottom - 8.0) };
        Shape::convex_polygon(vec![Pos2::new(x - 4.5, a), Pos2::new(x + 4.5, a), Pos2::new(x, b)], paper, Stroke::new(1.0, accent))
    };
    painter.add(triangle(first_x, true));
    painter.add(triangle(hang_x, false));
    painter.rect(Rect::from_center_size(Pos2::new(hang_x, y_bottom + 1.0), Vec2::new(9.0, 4.0)), 0.0, paper, Stroke::new(1.0, accent), egui::StrokeKind::Middle);
    painter.add(triangle(right_x, false));
    for stop in &look_now.tabs {
        let x = to_x(match &dragging {
            Some(RulerDrag { grip: RulerGrip::Tab(p), at, .. }) if (*p - stop.position).abs() < 0.5 => *at,
            _ => stop.position,
        });
        if !matches!(&dragging, Some(RulerDrag { grip: RulerGrip::Tab(p), off: true, .. }) if (*p - stop.position).abs() < 0.5) {
            tab_mark(&painter, x, y_bottom - 2.0, stop.kind, accent);
        }
    }
    // The pointer.
    let strip = Rect::from_x_y_ranges(page_left..=page_left + page.width * zoom, rect.y_range());
    let response = ui.interact(strip, ui.id().with(("word-ruler", &doc.key)), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Ruler"));
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else { return };
    let snap = |points: f32| (points / 4.5).round() * 4.5;
    if response.drag_started() || response.is_pointer_button_down_on() && doc.view.ruler_drag.is_none() && ui.input(|i| i.pointer.primary_pressed()) {
        let grips = [
            (RulerGrip::First, first_x, pointer.y < band.center().y),
            (RulerGrip::Left, hang_x, pointer.y > y_bottom - 2.0),
            (RulerGrip::Hanging, hang_x, pointer.y >= band.center().y),
            (RulerGrip::Right, right_x, true),
        ];
        let grip = grips
            .iter()
            .find(|(_, x, side)| (pointer.x - x).abs() <= 6.0 && *side)
            .map(|(g, _, _)| g.clone())
            .or_else(|| {
                look_now.tabs.iter().find(|t| (to_x(t.position) - pointer.x).abs() <= 5.0).map(|t| RulerGrip::Tab(t.position))
            });
        if let Some(grip) = grip {
            doc.view.ruler_drag = Some(RulerDrag { grip, at: to_points(pointer.x), off: false });
        }
    }
    if let Some(drag) = &mut doc.view.ruler_drag {
        drag.at = snap(to_points(pointer.x));
        drag.off = (pointer.y - rect.center().y).abs() > rect.height() * 1.5;
        ui.painter().line_segment(
            [Pos2::new(pointer.x, rect.bottom()), Pos2::new(pointer.x, pages.bottom())],
            Stroke::new(1.0, Color32::from_gray(120)), // any ground: a guide over the page
        );
        if ui.input(|i| i.pointer.primary_released()) {
            let drag = doc.view.ruler_drag.take().expect("held");
            finish_drag(doc, &drag, &look_now, page.body.w);
        }
        return;
    }
    if response.clicked() && pointer.x > column_left && pointer.x < column_right {
        doc.session.add_tab_stop(snap(to_points(pointer.x)), doc.view.ruler_tab);
    }
}

/// The left, right and first line indents a drag on an indent mark leaves, in points.
pub fn after_drag(drag: &RulerDrag, look: &unluminous_word::props::ParaLook, column: f32) -> (f32, f32, f32) {
    let (left, right, first) = (look.indent_left, look.indent_right, look.indent_first);
    match &drag.grip {
        // The first line alone: the left indent stays, so the first line's offset from it changes.
        RulerGrip::First => (left, right, drag.at - left),
        // The hanging indent: the left indent moves and the first line stays where it was.
        RulerGrip::Hanging => (drag.at, right, left + first - drag.at),
        // The box under it moves both together.
        RulerGrip::Left => (drag.at, right, first),
        RulerGrip::Right => (left, (column - drag.at).max(0.0), first),
        RulerGrip::Tab(_) => (left, right, first),
    }
}

/// Make the change a drag on the ruler describes.
fn finish_drag(doc: &mut Doc, drag: &RulerDrag, look: &unluminous_word::props::ParaLook, column: f32) {
    match &drag.grip {
        RulerGrip::Tab(position) if drag.off => doc.session.remove_tab_stop(*position),
        RulerGrip::Tab(position) => doc.session.move_tab_stop(*position, drag.at.max(0.0)),
        _ => {
            let (left, right, first) = after_drag(drag, look, column);
            doc.session.set_indents(left, right, first);
        }
    }
}

/// The status area: page, words, language, track changes, the three views and the zoom.
pub fn status(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, rect: Rect, requests: &mut Vec<Request>) {
    let _ = requests;
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, look.palette.divider));
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    inner.set_clip_rect(rect);
    let size = look.font_size * 0.88;
    let page = doc.pages.find(&doc.session.selection.head).map_or(1, |(i, _)| i + 1);
    let stats = doc.session.stats(Some(&doc.pages));
    let words = if doc.session.selection.is_empty() {
        format!("{} words", stats.words)
    } else {
        let selected = doc.session.selected_text().split_whitespace().count();
        format!("{selected} of {} words", stats.words)
    };
    let tracking = if doc.session.tracking { "Track Changes: On" } else { "Track Changes: Off" };
    let language = doc.session.language_at_caret();
    let item = |ui: &mut egui::Ui, text: &str, name: &str| -> bool {
        let button = egui::Button::new(egui::RichText::new(text).size(size)).frame(false);
        let response = ui.add(button);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        response.clicked()
    };
    if item(&mut inner, &format!("Page {page} of {}", doc.pages.pages.len()), "Status: page") {
        doc.view.goto = Some(String::new());
    }
    if item(&mut inner, &words, "Status: words") {
        doc.view.panel = vec![Level::Root, Level::Review];
    }
    if item(&mut inner, &language, "Status: language") {
        doc.view.panel = vec![Level::Root, Level::Review];
    }
    if item(&mut inner, tracking, "Status: track changes") {
        let on = !doc.session.tracking;
        doc.session.set_tracking(on);
    }
    inner.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let percent = (doc.view.zoom * 100.0).round();
        if item(ui, &format!("{percent}%"), "Status: zoom") {
            doc.view.panel = vec![Level::Root, Level::Document];
        }
        let mut zoom = doc.view.zoom;
        let slider = egui::Slider::new(&mut zoom, 0.1..=5.0).logarithmic(true).show_value(false);
        if ui.add_sized(Vec2::new(110.0, rect.height() - 6.0), slider).changed() {
            doc.view.zoom = zoom;
        }
        ui.add_space(8.0);
        for (mode, label) in [(Mode::Outline, "Outline"), (Mode::Print, "Print Layout"), (Mode::Read, "Read Mode")] {
            let chosen = doc.view.mode == mode;
            let button = egui::Button::new(egui::RichText::new(label).size(size)).selected(chosen).frame(chosen);
            let response = ui.add(button);
            let name = format!("View: {label}");
            response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, &name));
            if response.clicked() {
                doc.view.mode = mode;
                doc.laid = u64::MAX;
                doc.view.reveal = true;
            }
        }
    });
}
