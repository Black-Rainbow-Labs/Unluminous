//! Every drawn mark, on one sheet, large enough to look at.
//!
//! `task-1949` changed ten of them at once, and there was no way to see any of them: an icon is
//! twelve points across, so in a picture of the whole window each one is a dozen pixels and a change
//! to it is a smudge moving. Every other accepted picture in this repository has that problem — the
//! rail is in dozens of them and the marks on it are unreadable in all of them, which is why a
//! folder and a chat bubble could be redrawn without a single test noticing.
//!
//! So this draws them at **eight pixels a point**, which is the one thing the four layers of tests
//! did not have: a picture of an icon that a person can actually judge. It is an accepted picture
//! like any other, so redrawing a mark tomorrow fails here and the failure is a sheet somebody has
//! to open — which is what `design/icons.md` asks for and what
//! `crates/unluminous-app/tests/snapshots` is for.
//!
//! The sheet is the **material** set, because that is the one a window opens in. The `classic` set
//! gets its own, because "the marks Unluminous shipped with" is a promise that somebody has to be able
//! to check.

mod common;

use common::*;

use egui::{Color32, Pos2, Rect, Vec2};
use unluminous_app::theme::{color, icon, IconSet};

/// How many pixels one point of the sheet is drawn at.
///
/// Eight rather than four, because the question these pictures answer is whether a stroke meets a
/// stroke and whether a corner is where it was meant to be, and at four the answer is still a guess.
const ZOOM: f32 = 8.0;
/// How much room one mark is given, in points. The rail's own buttons are twenty four.
const CELL: f32 = 24.0;
/// How many marks to a row.
const ACROSS: usize = 8;

/// One mark: what it is called and what draws it.
type Mark = (&'static str, fn(&egui::Painter, Pos2, Color32));

/// The marks in the order somebody reads them: the rail's top group, then its bottom group, then the
/// title bar, then the rest of the window.
fn sheet() -> Vec<Mark> {
    vec![
        ("folder", icon::folder),
        ("file", icon::editing_area),
        ("branch", icon::branch),
        ("space", icon::space),
        ("chat", icon::chat),
        ("board", icon::board),
        ("database", icon::database),
        ("table", icon::table),
        ("terminal", icon::terminal),
        ("run", icon::run),
        ("bug", icon::bug),
        ("debug-run", icon::debug_run),
        ("stop", icon::stop),
        ("rerun", icon::rerun),
        ("resume", icon::resume),
        ("clear", icon::clear),
        ("magnifier", icon::magnifier),
        ("plus", icon::plus),
        ("cross", icon::cross),
        ("tick", icon::tick),
        ("bin", icon::bin),
        ("clock", icon::clock),
        ("copy", icon::copy),
        ("comment", icon::comment),
        ("image", icon::image),
        ("font", icon::font),
        ("undo", icon::undo),
        ("key", icon::key),
        ("stack", icon::stack),
        ("diamond", icon::diamond),
        ("zoom-in", icon::zoom_in),
        ("zoom-out", icon::zoom_out),
    ]
}

/// Draw the sheet into a harness at [`ZOOM`] pixels a point and take the picture.
fn draw_the_sheet(set: IconSet, name: &str) {
    let down = sheet().len().div_ceil(ACROSS);
    // The harness lays its `Ui` out inside egui's own margin, so the sheet is given the room the grid
    // needs **plus** that margin — without it the last column is drawn off the right hand edge, which
    // is a picture that silently stops holding the thing it was taken of.
    let margin = 24.0;
    let size = Vec2::new(CELL * ACROSS as f32 + margin, CELL * down as f32 + margin);
    let mut harness = builder()
        .with_pixels_per_point(ZOOM)
        .with_size(size)
        .build_ui(move |ui| {
            // The set is chosen inside the closure because the thread drawing a frame is not
            // necessarily the thread that built the harness, and `theme::activate` is thread-local
            // on purpose — see `theme::ACTIVE`.
            let mut theme = unluminous_app::theme::active();
            theme.icons = set;
            unluminous_app::theme::activate(theme);
            let area = ui.max_rect();
            let painter = ui.painter_at(area);
            painter.rect_filled(area, egui::CornerRadius::ZERO, color::editor());
            for (index, (_, draw)) in sheet().into_iter().enumerate() {
                let centre = Pos2::new(
                    area.left() + CELL * (index % ACROSS) as f32 + CELL / 2.0,
                    area.top() + CELL * (index / ACROSS) as f32 + CELL / 2.0,
                );
                // The cell's own edge, so a mark that has grown past the room the rail gives it is
                // visible as a mark crossing a line rather than as one that looks a little large.
                painter.rect_stroke(
                    Rect::from_center_size(centre, Vec2::splat(CELL)),
                    egui::CornerRadius::ZERO,
                    egui::Stroke::new(0.2, color::divider()),
                    egui::StrokeKind::Inside,
                );
                draw(&painter, centre, color::icon());
            }
        });
    harness.run();
    harness.snapshot(shot(name).as_str());
}

#[test]
fn every_drawn_mark_of_the_material_set_on_one_sheet() {
    draw_the_sheet(IconSet::Material, "icons_material");
}

#[test]
fn every_drawn_mark_of_the_classic_set_on_one_sheet() {
    draw_the_sheet(IconSet::Classic, "icons_classic");
}
