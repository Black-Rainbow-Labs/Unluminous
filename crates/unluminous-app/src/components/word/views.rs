//! The ruler over the pages (task-2359, TDD 5.5; redrawn in `task-2366`).
//!
//! The ruler is in inches from the left margin of the page the caret is on. It shows the margins, the
//! paragraph's first line, hanging, left and right indents and its tab stops, and each can be dragged:
//! a press on the ruler adds a tab stop of the kind the selector at its left shows, a stop dragged off
//! the ruler is taken away. The strip that used to sit under the pages is gone: what it said is in the
//! window's status bar (`services::word::status`).

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_word::props::TabKind;

use crate::components::controls::WithHint;
use crate::services::plugin_ui::Look;
use crate::services::word::Doc;

/// How tall the ruler is, before the tab's zoom.
pub const RULER: f32 = 26.0;

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
const KINDS: [TabKind; 5] =
    [TabKind::Left, TabKind::Center, TabKind::Right, TabKind::Decimal, TabKind::Bar];

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
fn tab_mark(painter: &egui::Painter, x: f32, base: f32, kind: TabKind, ink: Color32, z: f32) {
    let stroke = Stroke::new(1.8 * z, ink);
    let top = base - 7.0 * z;
    painter.line_segment([Pos2::new(x, top), Pos2::new(x, base)], stroke);
    let arm = 6.0 * z;
    match kind {
        TabKind::Left => {
            painter.line_segment([Pos2::new(x, base), Pos2::new(x + arm, base)], stroke);
        }
        TabKind::Right => {
            painter.line_segment([Pos2::new(x - arm, base), Pos2::new(x, base)], stroke);
        }
        TabKind::Center | TabKind::Decimal => {
            let half = arm * 0.7;
            painter.line_segment([Pos2::new(x - half, base), Pos2::new(x + half, base)], stroke);
            if kind == TabKind::Decimal {
                painter.circle_filled(Pos2::new(x + 3.0 * z, base - 3.5 * z), 1.3 * z, ink);
            }
        }
        TabKind::Bar | TabKind::Clear => {}
    }
}

/// A triangle for an indent mark, pointing down from `tip_y` when `down` and up otherwise, `scale` times
/// the design's size.
fn indent_triangle(
    x: f32,
    base_y: f32,
    down: bool,
    scale: f32,
    fill: Color32,
    edge: Stroke,
) -> Shape {
    let half = 5.0 * scale;
    let tall = 6.5 * scale;
    let tip = if down { base_y + tall } else { base_y - tall };
    Shape::convex_polygon(
        vec![Pos2::new(x - half, base_y), Pos2::new(x + half, base_y), Pos2::new(x, tip)],
        fill,
        edge,
    )
}

/// What a drag on the ruler is moving, in words, with where it is: `Left indent 0.50 in`.
fn drag_words(grip: &RulerGrip, at_points: f32) -> String {
    let what = match grip {
        RulerGrip::First => "First line indent",
        RulerGrip::Hanging => "Hanging indent",
        RulerGrip::Left => "Left indent",
        RulerGrip::Right => "Right indent",
        RulerGrip::Tab(_) => "Tab stop",
    };
    format!("{what} {:.2} in", at_points / 72.0)
}

/// The ruler: `rect` is its strip, `pages` the pages' area it sits over.
///
/// `task-2366` redrew it: the page is a rounded band with the margins recessed, the ticks are hairlines in
/// three lengths with the inches numbered, the indent marks are filled in the accent and grow under the
/// pointer, a drag shows a dashed guide down the page and a small card saying what is moving and where, and
/// the tab kind selector is a round chip.
pub fn ruler(ui: &mut egui::Ui, look: &Look<'_>, doc: &mut Doc, rect: Rect, pages: Rect) {
    let z = super::kit::zoom(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, look.ground(look.palette.panel));
    crate::components::chrome_kit::groove(ui.painter(), rect.left_bottom(), rect.right_bottom());
    selector(ui, doc, rect);
    let painter = ui.painter_at(rect);
    let Some((index, _)) = doc.pages.find(&doc.session.selection.head) else { return };
    let (tops, _) = super::page_view::page_tops(doc, pages.width());
    let Some(top) = tops.get(index) else { return };
    let page = &doc.pages.pages[index];
    let zoom = doc.view.zoom;
    let page_left = pages.left() + top.x;
    let page_right = page_left + page.width * zoom;
    let column_left = page_left + page.body.x * zoom;
    let column_right = page_left + (page.body.x + page.body.w) * zoom;
    // The page as a rounded band, the margins recessed and the text column the paper itself.
    let band = Rect::from_x_y_ranges(
        page_left..=page_right,
        rect.top() + 5.0 * z..=rect.bottom() - 5.0 * z,
    );
    let paper = crate::theme::color::word_paper();
    let radius = 5.0 * z;
    painter.rect_filled(band, radius, paper.gamma_multiply(0.35));
    painter.rect_filled(
        Rect::from_x_y_ranges(column_left..=column_right, band.y_range()),
        0.0,
        paper,
    );
    // Ticks every eighth of an inch from the left margin, longer at the quarters and halves, the inches
    // numbered.
    let tick_ink = Color32::from_gray(140); // any ground: drawn on the paper band
    let number_ink = Color32::from_gray(105); // any ground: drawn on the paper band
    let inch = 72.0 * zoom;
    let mut n = ((page_left - column_left) / (inch / 8.0)).floor() as i32;
    loop {
        let x = column_left + n as f32 * inch / 8.0;
        if x > page_right {
            break;
        }
        if x >= page_left + 2.0 && x <= page_right - 2.0 {
            let tall = match n.rem_euclid(8) {
                0 => 0.0,
                4 => 6.0,
                2 | 6 => 4.0,
                _ => 2.5,
            } * z;
            if n % 8 == 0 && n != 0 {
                painter.text(
                    Pos2::new(x, band.center().y),
                    Align2::CENTER_CENTER,
                    (n / 8).abs().to_string(),
                    FontId::proportional(9.5 * z),
                    number_ink,
                );
            } else if tall > 0.0 {
                painter.line_segment(
                    [
                        Pos2::new(x, band.center().y - tall / 2.0),
                        Pos2::new(x, band.center().y + tall / 2.0),
                    ],
                    Stroke::new(1.0, tick_ink),
                );
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
    let accent = crate::theme::rux_theme().chat.sky;
    let edge = Stroke::new(1.0, Color32::from_black_alpha(100)); // any ground: on the paper band
    let first_x = to_x(left + first);
    let hang_x = to_x(left);
    let right_x = to_x(page.body.w - right);
    let (y_top, y_bottom) = (band.top(), band.bottom());
    doc.view.ruler_marks = Some([first_x, hang_x, right_x, y_top, y_bottom]);
    // Which mark the pointer is over, so it can be drawn larger and the pointer can say it moves.
    let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
    let near = |x: f32, upper: bool| {
        hover.is_some_and(|p| (p.x - x).abs() <= 6.0 * z && (p.y < band.center().y) == upper)
    };
    let grow = |on: bool| if on { 1.2 * z } else { z };
    let held = |grip: RulerGrip| dragging.as_ref().is_some_and(|d| d.grip == grip);
    let first_on = near(first_x, true) || held(RulerGrip::First);
    let hang_on = near(hang_x, false) || held(RulerGrip::Hanging) || held(RulerGrip::Left);
    let right_on = near(right_x, false) || held(RulerGrip::Right);
    painter.add(indent_triangle(first_x, y_top, true, grow(first_on), accent, edge));
    painter.add(indent_triangle(hang_x, y_bottom - 3.0 * z, false, grow(hang_on), accent, edge));
    let box_size = Vec2::new(10.0, 4.0) * grow(hang_on);
    painter.rect(
        Rect::from_center_size(Pos2::new(hang_x, y_bottom), box_size),
        1.0,
        accent,
        edge,
        egui::StrokeKind::Middle,
    );
    painter.add(indent_triangle(right_x, y_bottom - 3.0 * z, false, grow(right_on), accent, edge));
    if first_on || hang_on || right_on {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    for stop in &look_now.tabs {
        let x = to_x(match &dragging {
            Some(RulerDrag { grip: RulerGrip::Tab(p), at, .. })
                if (*p - stop.position).abs() < 0.5 =>
            {
                *at
            }
            _ => stop.position,
        });
        let off = matches!(&dragging, Some(RulerDrag { grip: RulerGrip::Tab(p), off: true, .. }) if (*p - stop.position).abs() < 0.5);
        if !off {
            tab_mark(&painter, x, y_bottom - 2.0 * z, stop.kind, accent, z);
        }
    }
    // The pointer.
    let strip = Rect::from_x_y_ranges(page_left..=page_right, rect.y_range());
    let response =
        ui.interact(strip, ui.id().with(("word-ruler", &doc.key)), Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Ruler"));
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else { return };
    let snap = |points: f32| (points / 4.5).round() * 4.5;
    if response.drag_started()
        || response.is_pointer_button_down_on()
            && doc.view.ruler_drag.is_none()
            && ui.input(|i| i.pointer.primary_pressed())
    {
        let grips = [
            (RulerGrip::First, first_x, pointer.y < band.center().y),
            (RulerGrip::Left, hang_x, pointer.y > y_bottom - 2.0 * z),
            (RulerGrip::Hanging, hang_x, pointer.y >= band.center().y),
            (RulerGrip::Right, right_x, true),
        ];
        let grip = grips
            .iter()
            .find(|(_, x, side)| (pointer.x - x).abs() <= 6.0 * z && *side)
            .map(|(g, _, _)| g.clone())
            .or_else(|| {
                look_now
                    .tabs
                    .iter()
                    .find(|t| (to_x(t.position) - pointer.x).abs() <= 5.0 * z)
                    .map(|t| RulerGrip::Tab(t.position))
            });
        if let Some(grip) = grip {
            doc.view.ruler_drag = Some(RulerDrag { grip, at: to_points(pointer.x), off: false });
        }
    }
    if let Some(drag) = &mut doc.view.ruler_drag {
        drag.at = snap(to_points(pointer.x));
        drag.off = (pointer.y - rect.center().y).abs() > rect.height() * 1.5;
        let held = drag.clone();
        drag_guide(ui, &held, pointer, rect, pages, accent);
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

/// The tab kind selector at the ruler's left: a round chip carrying the kind a press on the ruler adds.
/// A press steps to the next kind.
fn selector(ui: &mut egui::Ui, doc: &mut Doc, rect: Rect) {
    let z = super::kit::zoom(ui.ctx());
    let size = (rect.height() - 6.0 * z).min(20.0 * z);
    let chip = Rect::from_min_size(
        Pos2::new(rect.left() + 8.0 * z, rect.center().y - size / 2.0),
        Vec2::splat(size),
    );
    let kind_now = doc.view.ruler_tab;
    let response = ui
        .interact(chip, ui.id().with(("word-ruler-kind", &doc.key)), Sense::click())
        .with_hint(format!("{}: press to change", kind_label(kind_now)));
    let hovered = response.hovered();
    let ink = super::kit::ink();
    super::kit::layer(ui, ("ruler-kind", &doc.key), chip, |rux| {
        rux::components::chip_disc(rux, chip);
        let tint = if hovered { rux.theme().chat.sky } else { ink.i700 };
        tab_mark(
            rux.painter(),
            chip.center().x - 1.0 * z,
            chip.center().y + 3.5 * z,
            kind_now,
            tint,
            z * 0.85,
        );
    });
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Tab kind"));
    if response.clicked() {
        let next = KINDS.iter().position(|k| *k == kind_now).map_or(0, |i| (i + 1) % KINDS.len());
        doc.view.ruler_tab = KINDS[next];
    }
}

/// While something on the ruler is dragged: a dashed accent line down the page under the pointer, and a
/// small card beside the pointer saying what is moving and where.
fn drag_guide(
    ui: &egui::Ui,
    drag: &RulerDrag,
    pointer: Pos2,
    rect: Rect,
    pages: Rect,
    accent: Color32,
) {
    let z = super::kit::zoom(ui.ctx());
    let layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with("word-ruler-guide"));
    let painter = ui.ctx().layer_painter(layer);
    let dash = 5.0 * z;
    let mut y = rect.bottom();
    while y < pages.bottom() {
        let end = (y + dash).min(pages.bottom());
        painter.line_segment(
            [Pos2::new(pointer.x, y), Pos2::new(pointer.x, end)],
            Stroke::new(1.2 * z, accent),
        );
        y += dash * 2.0;
    }
    let words = match (&drag.grip, drag.off) {
        (RulerGrip::Tab(_), true) => "Let go to take the tab stop away".to_owned(),
        _ => drag_words(&drag.grip, drag.at),
    };
    let ink = super::kit::ink();
    let chat = super::kit::chat();
    let size = 11.5 * z;
    let galley = super::kit::words(ui, &words, size, ink.i900, 260.0 * z);
    let card = Rect::from_min_size(
        Pos2::new(pointer.x + 12.0 * z, rect.bottom() + 8.0 * z),
        Vec2::new(galley.size().x + 20.0 * z, 26.0 * z),
    );
    let corner = egui::CornerRadius::same((13.0 * z).round() as u8);
    painter.add(
        egui::Shadow { offset: [0, 6], blur: 14, spread: 0, color: chat.dark.gamma_multiply(0.8) }
            .as_shape(card, corner),
    );
    painter.rect_filled(card, corner, chat.card.from);
    super::kit::draw_by_capitals(
        &painter,
        card.left() + 10.0 * z,
        card.center().y,
        galley,
        size,
        ink.i900,
    );
}

/// The left, right and first line indents a drag on an indent mark leaves, in points.
pub fn after_drag(
    drag: &RulerDrag,
    look: &unluminous_word::props::ParaLook,
    column: f32,
) -> (f32, f32, f32) {
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
fn finish_drag(
    doc: &mut Doc,
    drag: &RulerDrag,
    look: &unluminous_word::props::ParaLook,
    column: f32,
) {
    match &drag.grip {
        RulerGrip::Tab(position) if drag.off => doc.session.remove_tab_stop(*position),
        RulerGrip::Tab(position) => doc.session.move_tab_stop(*position, drag.at.max(0.0)),
        _ => {
            let (left, right, first) = after_drag(drag, look, column);
            doc.session.set_indents(left, right, first);
        }
    }
}
