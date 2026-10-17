//! The modal a right click on the canvas opens: search at the top, a list of node kinds under it.
//!
//! `task-1904` asks for it and shows it — a search field, a scrolling list, a highlighted row. Four
//! kinds is a short list today; the field is there because the ticket says *"we'll have so many
//! similar components"*, and a list that gets long later should not need a second design then.
//!
//! It is `components::modal::show_unframed`, so it has the dragging, the resizing and the zoom every dialog
//! in Unluminous has, and since `task-2250` it is drawn with the parts of `components::realm::dialog`, which
//! are the New Data Source dialog's.
//!
//! ## The Shape row opens a list of shapes beside the dialog
//!
//! `task-2284` added whiteboard shapes as one more kind, and there are nine of them, so the Shape row does not
//! add a node itself: pointing at it or pressing it opens a flyout to the right of the dialog, level with the
//! row, with each shape's outline drawn beside its name. There is no room on the right of a dialog near the
//! window's right edge, so there it opens on the left. Typing a shape's name in the search opens the flyout
//! on that shape. Right opens it from the keyboard, Up and Down walk it, Enter chooses and Left closes it.

use egui::{Pos2, Rect, Vec2};
use rux::Style;

use crate::components::realm::dialog;
use crate::components::{modal, settings_kit};
use crate::services::realm::{Kind, ShapeKind};

/// The size it opens at. Narrow, because the list is names rather than sentences.
const WIDTH: f32 = 460.0;
/// How tall one row is.
///
/// A name and two lines of summary, which is what the longest of the four needs: at 46 the second
/// line of the terminal's own description ran into the row under it, which the first picture showed.
const ROW: f32 = 58.0;

/// Tall enough for every kind, **worked out rather than chosen**.
///
/// `task-1905`: at 360 the body ran from y=60 to y=300, the field left the list starting at y=98, and
/// the fourth row of 60 ended at y=334 — so [`show_the_rows`] broke out of its loop and `Kind::Editor`
/// was never drawn at all. The modal read as a list of three kinds with a large empty realm under it,
/// and the report it produced guessed the kind was missing from the modal rather than off the bottom of
/// it. A number typed here is a number the next kind added would break again, so this is the arithmetic
/// `modal::body` and `show_the_rows` really do.
const HEIGHT: f32 = dialog::PAD_Y
    + dialog::CLOSE
    + dialog::GAP
    + dialog::SEARCH
    + dialog::GAP
    + ROW * Kind::ALL.len() as f32
    + dialog::PAD_Y;

/// The flyout of shapes: its width, its padding, its heading and one row.
const FLYOUT_WIDTH: f32 = 230.0;
const FLYOUT_PAD: f32 = 8.0;
const FLYOUT_HEADING: f32 = 26.0;
const FLYOUT_ROW: f32 = 34.0;
/// The gap between the dialog and the flyout.
const FLYOUT_GAP: f32 = 10.0;

/// What the person has typed and which row is picked out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub filter: String,
    /// Which row the arrow keys are on, counted within what the filter left.
    pub highlighted: usize,
    /// Where on the canvas the node will go, in world points, which is where the right click was.
    pub at: egui::Pos2,
    /// Whether the flyout of shapes beside the Shape row is open. `task-2284`.
    pub flyout: bool,
    /// Whether the keyboard is in the flyout, which is what Right puts it in and Left takes it out of.
    pub in_the_flyout: bool,
    /// Which shape in the flyout is picked out, counted within what the filter left.
    pub shape_highlighted: usize,
    /// Where the Shape row and the dialog were drawn on the screen last frame, which the flyout is placed
    /// beside. Screen points, because the flyout is a layer of its own over the dialog's.
    pub shape_row: Option<Rect>,
    pub dialog: Option<Rect>,
    /// The first row drawn, when the window is too short for every row. `task-2284`.
    pub first: usize,
    /// Whether the arrow keys moved the highlight this frame, so the list follows it.
    pub follow: bool,
}

/// What the modal reported.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// A kind was chosen, so a node of it goes on the canvas.
    pub chose: Option<Kind>,
    /// A shape was chosen in the flyout, so a shape node goes on the canvas and is typed into.
    pub shape: Option<ShapeKind>,
    pub closed: bool,
}

/// Which kinds the filter leaves, in the order they are drawn.
///
/// A plain case insensitive match on the name and on the one line under it, which is what a list of
/// four wants. `services::file_search`'s subsequence ranking is for a list of two thousand.
///
/// The Shape row matches the name of any shape as well, so `diamond` finds it.
pub fn matching(filter: &str) -> Vec<Kind> {
    let wanted = filter.trim().to_lowercase();
    Kind::ALL
        .into_iter()
        .filter(|kind| {
            wanted.is_empty()
                || kind.label().to_lowercase().contains(&wanted)
                || kind.summary().to_lowercase().contains(&wanted)
                || (*kind == Kind::Shape && names_a_shape(&wanted))
        })
        .collect()
}

/// Whether the search names one of the shapes.
fn names_a_shape(wanted: &str) -> bool {
    !wanted.is_empty()
        && ShapeKind::ALL.into_iter().any(|shape| shape.label().to_lowercase().contains(wanted))
}

/// The shapes the flyout lists for the search: the ones whose name holds it, or all of them when it names
/// none, which is what a search for `shape` itself wants.
pub fn shapes_matching(filter: &str) -> Vec<ShapeKind> {
    let wanted = filter.trim().to_lowercase();
    let hits: Vec<ShapeKind> = ShapeKind::ALL
        .into_iter()
        .filter(|shape| shape.label().to_lowercase().contains(&wanted))
        .collect();
    match hits.is_empty() {
        true => ShapeKind::ALL.to_vec(),
        false => hits,
    }
}

/// Draw the modal and report what was chosen.
///
/// `task-2250` drew it in the Settings design's language: see `components::realm::dialog`. Each kind wears
/// the mark the rail and the node's own header use for it.
pub fn show(ctx: &egui::Context, state: &mut State) -> Outcome {
    let mut outcome = Outcome::default();
    let (chosen, closed) =
        modal::show_unframed(ctx, "unluminous-add-node", WIDTH, HEIGHT, |ui, area| {
            dialog::surface(ui, "add-node", area);
            let mut chosen = None;
            let inner = inner_of(area);
            let found = matching(&state.filter);
            state.highlighted = state.highlighted.min(found.len().saturating_sub(1));
            let to_screen = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
            state.dialog = Some(to_screen * area);
            state.shape_row = None;
            walk_with_the_arrow_keys(ui, state, &found);
            settings_kit::layer(ui, "add-node", inner, |rux| {
                if dialog::heading(rux, inner, "Add a node") {
                    chosen = Some(None);
                }
                let well = Rect::from_min_size(
                    Pos2::new(inner.left(), inner.top() + dialog::CLOSE + dialog::GAP),
                    Vec2::new(inner.width(), dialog::SEARCH),
                );
                let response =
                    dialog::search(rux, well, "Search nodes", "Search nodes", &mut state.filter);
                // The keyboard starts in the field, which is what a modal opened to be typed into means.
                if !response.has_focus() && rux.ui.memory(|memory| memory.focused().is_none()) {
                    response.request_focus();
                }
                let list = Rect::from_min_max(
                    Pos2::new(inner.left(), well.bottom() + dialog::GAP),
                    inner.max,
                );
                if let Some(kind) = show_the_rows(rux, list, &found, state) {
                    chosen = Some(Some(Choice::Kind(kind)));
                }
            });
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                match found.get(state.highlighted) {
                    // **Enter on the Shape row opens its flyout, and Enter in the flyout chooses.**
                    Some(Kind::Shape) if state.flyout => {
                        let shapes = shapes_matching(&state.filter);
                        if let Some(shape) =
                            shapes.get(state.shape_highlighted.min(shapes.len() - 1))
                        {
                            chosen = Some(Some(Choice::Shape(*shape)));
                        }
                    }
                    Some(Kind::Shape) => {
                        state.flyout = true;
                        state.in_the_flyout = true;
                    }
                    Some(kind) => chosen = Some(Some(Choice::Kind(*kind))),
                    None => {}
                }
            }
            // A search that names a shape opens the flyout on it, which is what makes `diamond` and Enter
            // add a diamond.
            if !state.filter.trim().is_empty() && names_a_shape(&state.filter.trim().to_lowercase())
            {
                state.flyout = found.get(state.highlighted) == Some(&Kind::Shape) || state.flyout;
            }
            chosen
        });
    // The flyout is drawn after the dialog, on a layer of its own over it.
    let shape = match (state.flyout, state.shape_row, state.dialog) {
        (true, Some(row), Some(dialog)) => show_the_flyout(ctx, state, row, dialog),
        _ => None,
    };
    match (chosen, shape) {
        (_, Some(shape)) | (Some(Some(Choice::Shape(shape))), None) => {
            outcome.shape = Some(shape);
            outcome.closed = true;
        }
        (Some(Some(Choice::Kind(kind))), None) => {
            outcome.chose = Some(kind);
            outcome.closed = true;
        }
        (Some(None), None) => outcome.closed = true,
        (None, None) => outcome.closed = closed,
    }
    outcome
}

/// What a press in the dialog chose.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Choice {
    Kind(Kind),
    Shape(ShapeKind),
}

/// Where the flyout goes: to the right of the dialog, its first row level with the Shape row, or to the left
/// of the dialog when there is no room on the right. Kept inside the window either way.
pub fn flyout_rect(row: Rect, dialog: Rect, window: Rect, shapes: usize) -> Rect {
    let height = flyout_height(shapes);
    let right = dialog.right() + FLYOUT_GAP;
    let left = match right + FLYOUT_WIDTH > window.right() - 8.0 {
        true => dialog.left() - FLYOUT_GAP - FLYOUT_WIDTH,
        false => right,
    };
    let top = (row.top() - FLYOUT_PAD - FLYOUT_HEADING + 4.0)
        .min(window.bottom() - height - 8.0)
        .max(window.top() + 8.0);
    Rect::from_min_size(Pos2::new(left, top), Vec2::new(FLYOUT_WIDTH, height))
}

/// How tall the flyout is for `shapes` rows, with the line between the polygons and the last two.
fn flyout_height(shapes: usize) -> f32 {
    let separator = if shapes == ShapeKind::ALL.len() { 12.0 } else { 0.0 };
    FLYOUT_PAD * 2.0 + FLYOUT_HEADING + FLYOUT_ROW * shapes as f32 + separator
}

/// The flyout of shapes, and the shape pressed in it.
fn show_the_flyout(
    ctx: &egui::Context,
    state: &mut State,
    row: Rect,
    dialog: Rect,
) -> Option<ShapeKind> {
    let shapes = shapes_matching(&state.filter);
    state.shape_highlighted = state.shape_highlighted.min(shapes.len().saturating_sub(1));
    let rect = flyout_rect(row, dialog, ctx.content_rect(), shapes.len());
    let mut chosen = None;
    egui::Area::new(egui::Id::new("unluminous-add-node-shapes"))
        .order(egui::Order::Tooltip)
        .fixed_pos(rect.min)
        .constrain(false)
        .interactable(true)
        .show(ctx, |ui| {
            let (area, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
            settings_kit::layer_reaching(ui, "add-node-shapes", area, modal::SHADOW_REACH, |rux| {
                let theme = rux.theme();
                let chat = theme.chat;
                rux.chrome.surface(
                    area,
                    18.0,
                    rux::Fill::gradient(chat.card, area),
                    modal::shadow(),
                );
                let inner = area.shrink(FLYOUT_PAD);
                let heading = Style::sans(10.5).tracking(0.12);
                let galley = rux.text(heading, "SHAPES", theme.ink.i300);
                rux.painter().galley(
                    Pos2::new(inner.left() + 10.0, inner.top() + 7.0),
                    galley,
                    theme.ink.i300,
                );
                let mut top = inner.top() + FLYOUT_HEADING;
                for (index, shape) in shapes.iter().enumerate() {
                    // A line between the seven polygons and the star and the sticky note, as the design draws.
                    if shapes.len() == ShapeKind::ALL.len() && *shape == ShapeKind::Star {
                        crate::components::chrome_kit::groove(
                            rux.painter(),
                            Pos2::new(inner.left() + 8.0, top + 5.0),
                            Pos2::new(inner.right() - 8.0, top + 5.0),
                        );
                        top += 12.0;
                    }
                    let line = Rect::from_min_size(
                        Pos2::new(inner.left(), top),
                        Vec2::new(inner.width(), FLYOUT_ROW),
                    );
                    top += FLYOUT_ROW;
                    let response = rux.ui.interact(
                        line,
                        rux.ui.id().with(("add-node-shape", shape.name())),
                        egui::Sense::click(),
                    );
                    if response.hovered() {
                        state.shape_highlighted = index;
                    }
                    let lit = index == state.shape_highlighted
                        && (state.in_the_flyout || response.hovered());
                    if lit {
                        rux.chrome.surface(line, 12.0, chat.well, chat.carved_sm());
                    }
                    let mark = Rect::from_center_size(
                        Pos2::new(line.left() + 21.0, line.center().y),
                        match shape {
                            ShapeKind::Rectangle
                            | ShapeKind::Rounded
                            | ShapeKind::Parallelogram => Vec2::new(18.0, 13.0),
                            _ => Vec2::new(16.0, 16.0),
                        },
                    );
                    crate::components::realm::shape::mark(rux.painter(), *shape, mark, chat.sky);
                    let ink = if lit { theme.ink.i900 } else { theme.ink.i700 };
                    let galley = rux.text(Style::sans(12.5), shape.label(), ink);
                    let height = galley.size().y;
                    rux.painter().galley(
                        Pos2::new(line.left() + 42.0, line.center().y - height / 2.0),
                        galley,
                        ink,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Shape: {}", shape.label()),
                        )
                    });
                    if response.clicked() {
                        chosen = Some(*shape);
                    }
                }
            });
        });
    chosen
}

/// A small chevron at the top or the bottom of the list when there are rows above or below what is drawn.
fn scroll_marks(rux: &mut rux::Rux<'_>, area: Rect, above: bool, below: bool) {
    let tint = rux.theme().ink.i300;
    let stroke = egui::Stroke::new(1.6, tint);
    let middle = area.center().x;
    if above {
        let at = Pos2::new(middle, area.top() - 7.0);
        rux.painter().line_segment([at + Vec2::new(-5.0, 2.5), at + Vec2::new(0.0, -2.5)], stroke);
        rux.painter().line_segment([at + Vec2::new(0.0, -2.5), at + Vec2::new(5.0, 2.5)], stroke);
    }
    if below {
        let at = Pos2::new(middle, area.bottom() + 7.0);
        rux.painter().line_segment([at + Vec2::new(-5.0, -2.5), at + Vec2::new(0.0, 2.5)], stroke);
        rux.painter().line_segment([at + Vec2::new(0.0, 2.5), at + Vec2::new(5.0, -2.5)], stroke);
    }
}

/// The room inside the dialog's padding.
fn inner_of(area: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(area.left() + dialog::PAD_X, area.top() + dialog::PAD_Y),
        Pos2::new(area.right() - dialog::PAD_X, area.bottom() - dialog::PAD_Y),
    )
}

/// Up and down walk the list, and neither wraps.
///
/// Not wrapping is the explorer's own answer: a list of four that jumped from the bottom to the top
/// would be a list whose end nobody can feel. While the keyboard is in the flyout of shapes they walk that
/// instead, Right puts the keyboard in it from the Shape row and Left takes it back out.
fn walk_with_the_arrow_keys(ui: &egui::Ui, state: &mut State, found: &[Kind]) {
    let rows = found.len();
    if rows == 0 {
        state.highlighted = 0;
        return;
    }
    let (down, up, right, left) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowRight),
            input.key_pressed(egui::Key::ArrowLeft),
        )
    });
    let on_the_shape_row = found.get(state.highlighted) == Some(&Kind::Shape);
    if right && on_the_shape_row {
        state.flyout = true;
        state.in_the_flyout = true;
        return;
    }
    if left && state.in_the_flyout {
        state.in_the_flyout = false;
        return;
    }
    if state.in_the_flyout && state.flyout {
        let shapes = shapes_matching(&state.filter).len();
        if down {
            state.shape_highlighted = (state.shape_highlighted + 1).min(shapes.saturating_sub(1));
        }
        if up {
            state.shape_highlighted = state.shape_highlighted.saturating_sub(1);
        }
        return;
    }
    if down {
        state.highlighted = (state.highlighted + 1).min(rows - 1);
    }
    if up {
        state.highlighted = state.highlighted.saturating_sub(1);
    }
    if down || up {
        state.flyout = found.get(state.highlighted) == Some(&Kind::Shape) && state.flyout;
        state.follow = true;
    }
}

/// Which rows are drawn: from `first`, as many as fit in a list `height` points tall.
///
/// **A window too short for every kind scrolls the list a row at a time** (`task-2284`). Eleven kinds is 638
/// points of rows, which is more than a window 740 points tall leaves the dialog, and the last of them is the
/// Shape row. The wheel over the list moves it, and a highlight moved with the arrow keys is kept in it.
pub fn rows_drawn(first: usize, rows: usize, height: f32) -> std::ops::Range<usize> {
    let fits = (((height + 4.0) / ROW).floor() as usize).max(1);
    let first = first.min(rows.saturating_sub(fits));
    first..(first + fits).min(rows)
}

/// The rows themselves: the kind's mark in a chip, its name, the line under it, and the raised row on
/// whichever is highlighted.
fn show_the_rows(
    rux: &mut rux::Rux<'_>,
    area: Rect,
    found: &[Kind],
    state: &mut State,
) -> Option<Kind> {
    let mut chosen = None;
    let theme = rux.theme();
    if found.is_empty() {
        let galley = rux.text(Style::sans(12.0), "Nothing is called that.", theme.ink.i400);
        rux.painter().galley(
            Pos2::new(area.left() + 4.0, area.top() + 8.0),
            galley,
            theme.ink.i400,
        );
        return None;
    }
    let fits = rows_drawn(0, found.len(), area.height()).len();
    if state.follow {
        if state.highlighted < state.first {
            state.first = state.highlighted;
        } else if state.highlighted >= state.first + fits {
            state.first = state.highlighted + 1 - fits;
        }
        state.follow = false;
    }
    let pointer_in = rux.ui.rect_contains_pointer(area);
    let wheel = rux.ui.input(|input| input.smooth_scroll_delta.y);
    if pointer_in && wheel.abs() > 0.5 {
        state.first = match wheel < 0.0 {
            true => state.first + 1,
            false => state.first.saturating_sub(1),
        };
    }
    let drawn = rows_drawn(state.first, found.len(), area.height());
    state.first = drawn.start;
    scroll_marks(rux, area, drawn.start > 0, drawn.end < found.len());
    for (index, kind) in found.iter().enumerate().skip(drawn.start).take(drawn.len()) {
        let row = Rect::from_min_size(
            Pos2::new(area.left(), area.top() + (index - drawn.start) as f32 * ROW),
            Vec2::new(area.width(), ROW - 4.0),
        );
        let shape_row = *kind == Kind::Shape;
        let on = index == state.highlighted || (shape_row && state.flyout);
        let response = dialog::row(rux, row, kind.name(), on);
        if response.hovered() {
            state.highlighted = index;
            // **Pointing at the Shape row opens its flyout, and pointing at any other row closes it.**
            state.flyout = shape_row;
            state.in_the_flyout = false;
        }
        if shape_row {
            let to_screen =
                rux.ui.ctx().layer_transform_to_global(rux.ui.layer_id()).unwrap_or_default();
            state.shape_row = Some(to_screen * row);
            // The chevron that says the row opens something rather than adding a node.
            let tint = if state.flyout { theme.chat.sky } else { theme.ink.i400 };
            let centre = Pos2::new(row.right() - 18.0, row.center().y);
            rux.painter().line_segment(
                [centre + Vec2::new(-2.5, -5.0), centre + Vec2::new(2.5, 0.0)],
                egui::Stroke::new(1.7, tint),
            );
            rux.painter().line_segment(
                [centre + Vec2::new(2.5, 0.0), centre + Vec2::new(-2.5, 5.0)],
                egui::Stroke::new(1.7, tint),
            );
        }
        let disc = Rect::from_center_size(
            Pos2::new(row.left() + 14.0 + 15.0, row.center().y),
            Vec2::splat(30.0),
        );
        rux::components::chip_disc(rux, disc);
        crate::components::realm::kind_mark_of(*kind)(rux.painter(), disc.center(), theme.chat.sky);
        let left = disc.right() + 12.0;
        let name_style = Style::sans(13.0).semibold();
        let galley = rux.text(name_style, kind.label(), theme.ink.i900);
        let name_height = galley.size().y;
        rux.painter().galley(Pos2::new(left, row.top() + 7.0), galley, theme.ink.i900);
        let room_on_the_right = if shape_row { 34.0 } else { 14.0 };
        let galley = rux::text::wrapped(
            rux.painter(),
            Style::sans(11.0).leading(1.3),
            kind.summary(),
            theme.ink.i400,
            row.right() - room_on_the_right - left,
        );
        rux.painter().galley(
            Pos2::new(left, row.top() + 7.0 + name_height + 1.0),
            galley,
            theme.ink.i400,
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, kind.label())
        });
        if response.clicked() {
            match shape_row {
                true => state.flyout = true,
                false => chosen = Some(*kind),
            }
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind fits inside the body, which is what makes it drawn rather than merely offered.
    ///
    /// `task-1905`: this is the arithmetic `show_the_rows` does, asserted rather than looked at. It
    /// fails at the 360 the modal opened at, where the fourth row ended six points past the body and
    /// the loop that draws the rows gave up — so `Kind::Editor` was in `matching("")`, which the test
    /// below already asserted, and on the screen it was nowhere.
    #[test]
    fn every_kind_has_room_to_be_drawn_in_the_modal() {
        let area = Rect::from_min_size(Pos2::ZERO, Vec2::new(WIDTH, HEIGHT));
        let body = inner_of(area);
        let list_top = body.top() + dialog::CLOSE + dialog::GAP + dialog::SEARCH + dialog::GAP;
        let last = list_top + (Kind::ALL.len() - 1) as f32 * ROW + ROW - 4.0;
        assert!(
            last <= body.bottom(),
            "the last of {} kinds ends at {last} and the body ends at {}",
            Kind::ALL.len(),
            body.bottom(),
        );
        // And not so much taller than it needs to be that a kind could be added without anybody
        // noticing the modal had stopped fitting: one row of slack, no more.
        assert!(body.bottom() - last < ROW, "the modal is more than a row taller than its list");
    }

    /// A list too long for the dialog is drawn from the first row the wheel or the keys moved it to, and
    /// never past its end.
    #[test]
    fn a_list_too_long_for_the_window_is_drawn_a_window_at_a_time() {
        assert_eq!(rows_drawn(0, 11, ROW * 11.0), 0..11, "it all fits");
        assert_eq!(rows_drawn(0, 11, ROW * 8.0), 0..8);
        assert_eq!(rows_drawn(3, 11, ROW * 8.0), 3..11, "the last row is reachable");
        assert_eq!(rows_drawn(9, 11, ROW * 8.0), 3..11, "and never past the end");
        assert_eq!(rows_drawn(0, 2, 10.0), 0..1, "at least one row is drawn");
    }

    /// `task-2284`: the shapes are found by their own names, and the flyout lists the ones a search names.
    #[test]
    fn a_shapes_name_finds_the_shape_row_and_narrows_the_flyout() {
        assert_eq!(matching("diamond"), vec![Kind::Shape]);
        assert!(matching("").ends_with(&[Kind::Shape]), "Shape is the last row");
        assert_eq!(shapes_matching("dia"), vec![ShapeKind::Diamond]);
        assert_eq!(shapes_matching(""), ShapeKind::ALL.to_vec());
        assert_eq!(
            shapes_matching("shape"),
            ShapeKind::ALL.to_vec(),
            "a search for the row lists them all"
        );
    }

    /// The flyout opens to the right of the dialog, level with the row, and on the left when the right has no
    /// room.
    #[test]
    fn the_flyout_opens_on_the_right_unless_there_is_no_room_there() {
        let window = Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0));
        let dialog = Rect::from_min_size(Pos2::new(470.0, 80.0), Vec2::new(WIDTH, HEIGHT));
        let row = Rect::from_min_size(Pos2::new(490.0, 600.0), Vec2::new(420.0, ROW));
        let right = flyout_rect(row, dialog, window, ShapeKind::ALL.len());
        assert!(right.left() > dialog.right(), "to the right: {right:?}");
        assert!(right.bottom() <= window.bottom() - 7.0, "kept inside the window");
        let crowded = Rect::from_min_size(Pos2::new(1100.0, 80.0), Vec2::new(WIDTH, HEIGHT));
        let left = flyout_rect(row, crowded, window, ShapeKind::ALL.len());
        assert!(left.right() < crowded.left(), "to the left when the right has no room: {left:?}");
        let level = flyout_rect(
            Rect::from_min_size(Pos2::new(490.0, 200.0), Vec2::new(420.0, ROW)),
            dialog,
            window,
            9,
        );
        assert!(
            (level.top() + FLYOUT_PAD + FLYOUT_HEADING - 200.0).abs() < 6.0,
            "level with the row"
        );
    }

    #[test]
    fn the_filter_matches_a_name_or_the_line_under_it() {
        assert_eq!(matching(""), Kind::ALL.to_vec());
        assert_eq!(matching("brow"), vec![Kind::Browser]);
        assert_eq!(matching("BROWSER"), vec![Kind::Browser], "case does not matter");
        // The summaries are what make a search useful on a list nobody has memorised: "shell" is not
        // in the word `Terminal`.
        assert_eq!(matching("shell"), vec![Kind::Terminal]);
        assert!(matching("nothing like this").is_empty());
    }
}
