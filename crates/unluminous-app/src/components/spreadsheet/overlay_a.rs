//! What is drawn over the grid for the outline, formula auditing and conditional formats (`task-2360`):
//! the outline marks and level buttons in the headers, the trace arrows, the green error triangles and
//! their menu, and the data bars and icon sets inside cells. Presses on the outline marks, the level
//! buttons and the triangles are answered with the words of a verb, as a menu entry is.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use unluminous_sheet::conditional::{BarShape, IconPick};
use unluminous_sheet::coord::{column_name, CellRef, Range};
use unluminous_sheet::edit::review::errors::ErrorOptions;
use unluminous_sheet::outline::{self, Dim};

use super::grid::{rgb, GridOut, Ink, Layout, Pane};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::catalogue::Item;
use crate::services::spreadsheet::Book;

/// The most cells whose errors are checked in one frame.
const MOST_CHECKED: u32 = 6000;

/// Draw everything this file draws over the cells and headers, and read its presses.
pub fn draw(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    layout: &Layout,
    panes: &[Pane],
    _ink: &Ink,
    out: &mut GridOut,
) {
    let Some(pane) = panes.last() else { return };
    let frame = pane.frame;
    outline_marks(ui, look, book, layout, frame, out);
    trace_arrows(ui, book, frame);
    error_triangles(ui, book, frame, out);
}

/// A data bar's fill and border rectangles inside its cell, from the bar the rules picked.
pub fn data_bar(
    cell: Rect,
    shape: &BarShape,
    fills: &mut Vec<(Rect, Color32)>,
    edges: &mut Vec<(Rect, Color32)>,
) {
    let width = cell.width() - 2.0;
    let left = cell.left() + 1.0 + width * shape.start as f32;
    let right = cell.left() + 1.0 + width * shape.end as f32;
    let bar = Rect::from_min_max(
        Pos2::new(left, cell.top() + 2.0),
        Pos2::new(right.max(left + 1.0), cell.bottom() - 2.0),
    );
    fills.push((bar, rgb(shape.fill)));
    if let Some(border) = shape.border {
        edges.push((bar, rgb(border)));
    }
    if let Some((at, colour)) = shape.axis {
        let x = cell.left() + 1.0 + width * at as f32;
        let axis = Rect::from_min_max(
            Pos2::new(x, cell.top() + 1.0),
            Pos2::new(x + 1.0, cell.bottom() - 1.0),
        );
        edges.push((axis, rgb(colour)));
    }
}

/// The colour of the nth icon of a set of `count`: red at the lowest, green at the highest.
fn icon_colour(index: usize, count: usize) -> Color32 {
    if count < 2 {
        return Color32::from_rgb(0x9B, 0x9B, 0x9B);
    }
    let t = index as f32 / (count - 1) as f32;
    if t < 0.34 {
        Color32::from_rgb(0xE0, 0x4B, 0x4B)
    } else if t < 0.67 {
        Color32::from_rgb(0xE8, 0xB8, 0x2E)
    } else {
        Color32::from_rgb(0x3C, 0xB3, 0x71)
    }
}

/// An icon of a set at the left of its cell: an arrow, a light, a flag, a star, a triangle or a rating bar.
pub fn icon(painter: &egui::Painter, cell: Rect, pick: &IconPick) {
    let count = unluminous_sheet::model::icon_count(&pick.set);
    let colour = icon_colour(pick.index, count);
    let side = (cell.height() - 4.0).clamp(6.0, 16.0);
    let box_ = Rect::from_min_size(
        Pos2::new(cell.left() + 2.0, cell.center().y - side / 2.0),
        Vec2::splat(side),
    );
    let c = box_.center();
    let r = side / 2.0;
    let set = pick.set.as_str();
    if set.contains("Arrows") {
        let t = pick.index as f32 / (count.max(2) - 1) as f32;
        let tilt = 1.0 - 2.0 * t;
        let tip = c + Vec2::new(r * 0.9, -r * tilt * 0.9);
        let tail = c - Vec2::new(r * 0.9, -r * tilt * 0.9);
        let along = (tip - tail).normalized();
        let across = Vec2::new(-along.y, along.x);
        painter.line_segment([tail, tip], Stroke::new(2.0, colour));
        painter.add(Shape::convex_polygon(
            vec![
                tip + along * 1.0,
                tip - along * r * 0.8 + across * r * 0.6,
                tip - along * r * 0.8 - across * r * 0.6,
            ],
            colour,
            Stroke::NONE,
        ));
    } else if set.contains("Triangles") {
        let up = pick.index + 1 == count;
        let pts = if up {
            vec![c + Vec2::new(0.0, -r), c + Vec2::new(r, r), c + Vec2::new(-r, r)]
        } else if pick.index == 0 {
            vec![c + Vec2::new(0.0, r), c + Vec2::new(r, -r), c + Vec2::new(-r, -r)]
        } else {
            vec![c + Vec2::new(-r, 0.0), c + Vec2::new(r, -r * 0.4), c + Vec2::new(r, r * 0.4)]
        };
        painter.add(Shape::convex_polygon(pts, colour, Stroke::NONE));
    } else if set.contains("Flags") {
        painter.line_segment(
            [c + Vec2::new(-r * 0.6, -r), c + Vec2::new(-r * 0.6, r)],
            Stroke::new(1.5, colour),
        );
        painter.add(Shape::convex_polygon(
            vec![
                c + Vec2::new(-r * 0.6, -r),
                c + Vec2::new(r, -r * 0.4),
                c + Vec2::new(-r * 0.6, r * 0.2),
            ],
            colour,
            Stroke::NONE,
        ));
    } else if set.contains("Stars") {
        let filled = (pick.index as f32 + 1.0) / count as f32;
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            if filled > 0.4 {
                "\u{2605}"
            } else {
                "\u{2606}"
            },
            egui::FontId::proportional(side),
            colour,
        );
    } else if set.contains("Rating") || set.contains("Boxes") {
        let bars = pick.index + 1;
        for i in 0..count {
            let h = side * (i as f32 + 1.0) / count as f32;
            let x = box_.left() + side * i as f32 / count as f32;
            let on = i < bars;
            let bar = Rect::from_min_max(
                Pos2::new(x, box_.bottom() - h),
                Pos2::new(x + side / count as f32 - 1.0, box_.bottom()),
            );
            painter.rect_filled(bar, 0, if on { colour } else { Color32::from_gray(0xB0) });
        }
    } else {
        painter.circle_filled(c, r * 0.85, colour);
        if set.contains("Symbols") || set.contains("Signs") {
            painter.circle_stroke(c, r * 0.85, Stroke::new(1.0, Color32::from_gray(0x40)));
        }
    }
}

/// The outline: a vertical rule and a plus or minus box for every group on the screen, in the headers, and
/// a button for each level in the corner.
fn outline_marks(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    layout: &Layout,
    frame: super::grid::Frame,
    out: &mut GridOut,
) {
    let index = book.view.sheet;
    let sheet = &book.session.book.sheets[index];
    if !sheet.outline.show_symbols || !layout.headings {
        return;
    }
    let (rows, columns) = book.session.outline_depth(index);
    if rows == 0 && columns == 0 {
        return;
    }
    let ink = Stroke::new(1.0, look.palette.text_faint);
    let (first_row, last_row, first_col, last_col) = frame.visible(book);
    let painter = ui.painter_at(layout.area);
    let side = 9.0 * frame.zoom.clamp(0.8, 1.5);
    for row in first_row..=last_row {
        let l = outline::line(sheet, Dim::Rows, row);
        let cell = frame.cell(book, CellRef::new(row, 0));
        if cell.height() <= 0.0 {
            continue;
        }
        let x = frame.row_header.left() + 3.0;
        if l.level > 0 {
            let tick = x + 2.0 * l.level as f32;
            painter
                .line_segment([Pos2::new(tick, cell.top()), Pos2::new(tick, cell.bottom())], ink);
        }
        if outline::extent(sheet, Dim::Rows, row).is_some() {
            let b =
                Rect::from_min_size(Pos2::new(x, cell.center().y - side / 2.0), Vec2::splat(side));
            let words = [
                "outline",
                if l.collapsed { "show" } else { "hide" },
                "rows",
                &(row + 1).to_string(),
            ];
            outline_box(ui, look, book, b, l.collapsed, &words, out, ("row", row));
        }
    }
    for col in first_col..=last_col {
        let l = outline::line(sheet, Dim::Columns, col);
        let cell = frame.cell(book, CellRef::new(0, col));
        if cell.width() <= 0.0 {
            continue;
        }
        let y = frame.column_header.top() + 2.0;
        if l.level > 0 {
            let tick = y + 2.0 * l.level as f32;
            painter
                .line_segment([Pos2::new(cell.left(), tick), Pos2::new(cell.right(), tick)], ink);
        }
        if outline::extent(sheet, Dim::Columns, col).is_some() {
            let b =
                Rect::from_min_size(Pos2::new(cell.center().x - side / 2.0, y), Vec2::splat(side));
            let words = [
                "outline",
                if l.collapsed { "show" } else { "hide" },
                "columns",
                &column_name(col),
            ];
            outline_box(ui, look, book, b, l.collapsed, &words, out, ("column", col));
        }
    }
    level_buttons(ui, look, book, layout, rows.max(columns), out);
}

/// One plus or minus box, which presses into the words given.
#[allow(clippy::too_many_arguments)]
fn outline_box(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    rect: Rect,
    collapsed: bool,
    words: &[&str],
    out: &mut GridOut,
    key: (&str, u32),
) {
    let painter = ui.painter_at(rect.expand(1.0));
    painter.rect_filled(rect, 1, look.palette.panel);
    painter.rect_stroke(
        rect,
        1,
        Stroke::new(1.0, look.palette.text_faint),
        egui::StrokeKind::Inside,
    );
    let c = rect.center();
    let r = rect.width() * 0.25;
    painter.line_segment(
        [c - Vec2::new(r, 0.0), c + Vec2::new(r, 0.0)],
        Stroke::new(1.0, look.palette.text),
    );
    if collapsed {
        painter.line_segment(
            [c - Vec2::new(0.0, r), c + Vec2::new(0.0, r)],
            Stroke::new(1.0, look.palette.text),
        );
    }
    let id = ui.id().with(("outline-box", &book.key, key));
    let response = ui.interact(rect, id, Sense::click());
    let name = format!("{} group {}", if collapsed { "Expand" } else { "Collapse" }, key.1 + 1);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    if response.clicked() {
        out.words.push(words.iter().map(|w| (*w).to_owned()).collect());
    }
}

/// The level buttons 1 to depth + 1 in the corner.
fn level_buttons(
    ui: &egui::Ui,
    look: &Look<'_>,
    book: &Book,
    layout: &Layout,
    depth: u8,
    out: &mut GridOut,
) {
    let corner = Rect::from_min_max(layout.area.min, layout.cells.min);
    let side = (corner.height() - 4.0).clamp(6.0, 14.0);
    let painter = ui.painter_at(corner);
    for n in 1..=(depth as usize + 1).min(8) {
        let rect = Rect::from_min_size(
            Pos2::new(corner.left() + 2.0 + (n - 1) as f32 * (side + 1.0), corner.top() + 2.0),
            Vec2::splat(side),
        );
        if rect.right() > corner.right() {
            break;
        }
        painter.rect_stroke(
            rect,
            1,
            Stroke::new(1.0, look.palette.text_faint),
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            n.to_string(),
            egui::FontId::proportional(side * 0.7),
            look.palette.text,
        );
        let response =
            ui.interact(rect, ui.id().with(("outline-level", &book.key, n)), Sense::click());
        let name = format!("Show outline level {n}");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
        if response.clicked() {
            out.words.push(vec!["outline".into(), "level".into(), n.to_string(), n.to_string()]);
        }
    }
}

/// The trace arrows of this sheet: a blue arrow from each source to its target, dashed to a sheet mark when
/// the source is on another sheet, red for Trace Error.
fn trace_arrows(ui: &egui::Ui, book: &Book, frame: super::grid::Frame) {
    let arrows = &book.view.audit.arrows;
    if arrows.is_empty() {
        return;
    }
    let painter = ui.painter_at(frame.cells);
    let here = book.view.sheet;
    for a in arrows {
        let colour = if a.error {
            Color32::from_rgb(0xD0, 0x30, 0x30)
        } else {
            Color32::from_rgb(0x2A, 0x5F, 0xC9)
        };
        let stroke = Stroke::new(1.5, colour);
        if a.to_sheet != here {
            continue;
        }
        let to = frame.cell(book, a.to);
        let target = to.left_center() + Vec2::new(3.0, 0.0);
        if a.other_sheet || a.from_sheet != here {
            let from = target - Vec2::new(26.0, 0.0);
            dashed(&painter, from, target, stroke);
            painter.rect_stroke(
                Rect::from_center_size(from, Vec2::splat(8.0)),
                0,
                Stroke::new(1.0, colour),
                egui::StrokeKind::Inside,
            );
            head(&painter, from, target, colour);
            continue;
        }
        let source = frame.range(book, a.from);
        let start = source.center();
        if a.from.len() > 1 {
            painter.rect_stroke(source, 0, Stroke::new(1.0, colour), egui::StrokeKind::Inside);
        }
        painter.circle_filled(start, 2.5, colour);
        painter.line_segment([start, target], stroke);
        head(&painter, start, target, colour);
    }
}

fn dashed(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.add(Shape::dashed_line(&[a, b], stroke, 4.0, 3.0));
}

fn head(painter: &egui::Painter, from: Pos2, tip: Pos2, colour: Color32) {
    let v = tip - from;
    if v.length() < 1.0 {
        return;
    }
    let along = v.normalized();
    let across = Vec2::new(-along.y, along.x);
    painter.add(Shape::convex_polygon(
        vec![tip, tip - along * 8.0 + across * 4.0, tip - along * 8.0 - across * 4.0],
        colour,
        Stroke::NONE,
    ));
}

/// A green triangle in the corner of each cell Error Checking flags, which opens the error's menu.
fn error_triangles(ui: &egui::Ui, book: &Book, frame: super::grid::Frame, out: &mut GridOut) {
    let (first_row, last_row, first_col, last_col) = frame.visible(book);
    let area = Range::new(CellRef::new(first_row, first_col), CellRef::new(last_row, last_col));
    if area.len() as u32 > MOST_CHECKED {
        return;
    }
    let index = book.view.sheet;
    let found = book.session.check_errors(Some(index), Some(area), ErrorOptions::default());
    let painter = ui.painter_at(frame.cells);
    let size = (6.0 * frame.zoom).clamp(4.0, 9.0);
    for f in found {
        let cell = frame.cell(book, f.at);
        if cell.width() <= size || cell.height() <= size {
            continue;
        }
        let corner = cell.left_top();
        painter.add(Shape::convex_polygon(
            vec![corner, corner + Vec2::new(size, 0.0), corner + Vec2::new(0.0, size)],
            Color32::from_rgb(0x21, 0x9E, 0x5A),
            Stroke::NONE,
        ));
        let spot = Rect::from_min_size(corner, Vec2::splat(size + 3.0));
        let id = ui.id().with(("error-triangle", &book.key, f.at.row, f.at.col));
        let response = ui.interact(spot, id, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Error indicator")
        });
        if response.clicked() {
            out.drop_menu = Some((spot.right_bottom(), error_menu(f.at, f.rule, f.message())));
        }
    }
}

/// The menu of an error indicator: what it says, then ignore, trace and the options.
pub fn error_menu(
    at: CellRef,
    rule: unluminous_sheet::edit::review::errors::ErrorRule,
    message: &str,
) -> Vec<Item> {
    let name = crate::services::spreadsheet::verbs_a_review::rule_name(rule);
    let run = |label: &str, text: &str| Item::Run {
        label: label.to_owned(),
        keys: "",
        words: text.split(' ').map(str::to_owned).collect(),
        checked: false,
    };
    vec![
        run(message, &format!("select {}", at.a1())),
        run("Ignore error", &format!("error-check ignore {} {name}", at.a1())),
        run("Trace error", &format!("trace error {}", at.a1())),
        run("Evaluate formula", &format!("evaluate {}", at.a1())),
        run("Error checking options", "error-check all"),
    ]
}

/// The rows the filter button's menu has besides sorting and the value list: text, number, date, colour,
/// top, average, reapply and clear filters. Each opens a small box for its value, or runs at once.
pub fn filter_extras(ui: &mut egui::Ui, column: &str) -> Option<super::menus::Picked> {
    use crate::components::controls;
    use crate::services::spreadsheet::view::{Field, Prompt};
    let ask = |kind: &str, title: &str, label: &str| {
        let mut verb = vec!["filter-column".to_owned(), column.to_owned()];
        verb.extend(kind.split(' ').map(str::to_owned));
        super::menus::Picked::Ask(Prompt::form(verb, title, vec![Field::bare(label, "")]))
    };
    let words =
        |w: &[&str]| super::menus::Picked::Words(w.iter().map(|s| (*s).to_owned()).collect());
    let mut picked = None;
    let rows: Vec<(&str, super::menus::Picked)> = vec![
        ("Text filter: equals...", ask("equals", "Equals", "Text")),
        ("Text filter: begins with...", ask("begins", "Begins with", "Text")),
        ("Text filter: ends with...", ask("ends", "Ends with", "Text")),
        ("Text filter: contains...", ask("contains", "Contains", "Text")),
        ("Text filter: does not contain...", ask("not-contains", "Does not contain", "Text")),
        (
            "Number filter: comparison...",
            ask("number", "Number filter", "Comparison and value, such as > 10"),
        ),
        (
            "Date filter: year, month or period...",
            ask("dates", "Date filter", "2026, 2026-03, today, thisWeek, Q1"),
        ),
        ("Top 10...", ask("top", "Top items", "How many")),
        ("Above average", words(&["filter-column", column, "above-average"])),
        ("Below average", words(&["filter-column", column, "below-average"])),
        ("Filter by cell color...", ask("color cell", "Cell color", "Color, #RRGGBB or none")),
        ("Filter by font color...", ask("color font", "Font color", "Color, #RRGGBB or none")),
        ("Filter by icon...", ask("icon", "Icon", "Icon, such as 3Arrows/1")),
        ("Reapply", words(&["filter-reapply"])),
        ("Clear all filters", words(&["filter-clear"])),
    ];
    for (label, action) in rows {
        if controls::menu_row(ui, label, "", true, false, 0.0) && picked.is_none() {
            picked = Some(action);
        }
    }
    picked
}

/// Whether the pointer is on an outline box or a level button, so the press folds and selects nothing.
pub fn over_outline(
    ui: &egui::Ui,
    book: &Book,
    layout: &Layout,
    frame: super::grid::Frame,
) -> bool {
    let Some(p) = ui.ctx().pointer_interact_pos() else { return false };
    let index = book.view.sheet;
    let sheet = &book.session.book.sheets[index];
    if !sheet.outline.show_symbols || !layout.headings {
        return false;
    }
    let corner = Rect::from_min_max(layout.area.min, layout.cells.min);
    let (rows, columns) = book.session.outline_depth(index);
    if rows == 0 && columns == 0 {
        return false;
    }
    if corner.contains(p) {
        return true;
    }
    let side = 9.0 * frame.zoom.clamp(0.8, 1.5);
    if frame.row_header.contains(p) && p.x < frame.row_header.left() + 3.0 + side {
        let at = frame.at(book, Pos2::new(frame.cells.left() + 1.0, p.y));
        return outline::extent(sheet, Dim::Rows, at.row).is_some();
    }
    if frame.column_header.contains(p) && p.y < frame.column_header.top() + 2.0 + side {
        let at = frame.at(book, Pos2::new(p.x, frame.cells.top() + 1.0));
        return outline::extent(sheet, Dim::Columns, at.col).is_some();
    }
    false
}
