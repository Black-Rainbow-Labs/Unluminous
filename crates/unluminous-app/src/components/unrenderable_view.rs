//! The editing area when the tab holds a file Unluminous cannot show (`task-2270`).
//!
//! *"ensure we can open nearly all file types. If not renderable, just open a tab that says 'Unable to render
//! this type of file'."* The tab says that, names the file, says why and how large it is, and offers the two
//! things a person can still do with it: open it in the program the operating system opens it with, and show it
//! in the file manager. Like every component it changes nothing itself and reports which button was pressed.

use egui::{Align2, FontId, Pos2, Rect, Vec2};

use crate::app::files::Unrenderable;
use crate::theme::color;
use crate::theme::crisp::CrispPainter;

/// The sentence the tab is about.
pub const UNABLE: &str = "Unable to render this type of file";

/// What was pressed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub open_with_the_system: bool,
    pub reveal: bool,
}

/// How a size is said: bytes, kilobytes or megabytes, with one figure after the point past a kilobyte.
pub fn size_words(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} bytes"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// Draw the tab into `area`.
pub fn show(ui: &mut egui::Ui, area: Rect, file: &Unrenderable) -> Outcome {
    let mut outcome = Outcome::default();
    ui.painter().rect_filled(area, 0.0, color::editor());
    let name =
        file.path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    let middle = area.center();
    crate::theme::icon::file_mark(
        ui.painter(),
        Pos2::new(middle.x, middle.y - 70.0),
        false,
        color::text_dim(),
        2.0,
    );
    ui.painter().crisp_text(
        Pos2::new(middle.x, middle.y - 34.0),
        Align2::CENTER_CENTER,
        UNABLE,
        FontId::proportional(16.0),
        color::text_strong(),
    );
    ui.painter().crisp_text(
        Pos2::new(middle.x, middle.y - 10.0),
        Align2::CENTER_CENTER,
        &name,
        FontId::monospace(12.5),
        color::text(),
    );
    let why = match file.size {
        Some(size) => format!("{} \u{00b7} {}", file.reason, size_words(size)),
        None => file.reason.clone(),
    };
    let words = ui.painter().layout(
        why,
        FontId::proportional(12.0),
        color::text_dim(),
        (area.width() - 48.0).clamp(80.0, 520.0),
    );
    let top = middle.y + 8.0;
    let at = Pos2::new(middle.x - words.size().x / 2.0, top);
    let below = top + words.size().y + 18.0;
    ui.painter().galley(at, words, color::text_dim());
    let row = Rect::from_center_size(Pos2::new(middle.x, below + 17.0), Vec2::new(380.0, 34.0));
    crate::components::chrome_kit::layer(ui, ("unrenderable", &name), row, 12.0, |rux| {
        let open = rux::components::PillButton::new("Open in Default App").primary();
        let reveal = rux::components::PillButton::new("Reveal in File Manager");
        let (open_width, reveal_width) =
            (open.measure(rux).x.max(150.0), reveal.measure(rux).x.max(150.0));
        let left = middle.x - (open_width + reveal_width + 10.0) / 2.0;
        let reveal_at =
            Rect::from_min_size(Pos2::new(left, row.top()), Vec2::new(reveal_width, 34.0));
        let open_at = Rect::from_min_size(
            Pos2::new(reveal_at.right() + 10.0, row.top()),
            Vec2::new(open_width, 34.0),
        );
        outcome.reveal = reveal.show(rux, reveal_at).clicked();
        outcome.open_with_the_system = open.show(rux, open_at).clicked();
    });
    ui.interact(area, ui.id().with(("unrenderable-area", &name)), egui::Sense::hover())
        .widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("{UNABLE}: {name}"))
        });
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_said_in_the_unit_that_reads_best() {
        assert_eq!(size_words(512), "512 bytes");
        assert_eq!(size_words(2048), "2.0 KB");
        assert_eq!(size_words(5 * 1_048_576 + 524_288), "5.5 MB");
    }
}
