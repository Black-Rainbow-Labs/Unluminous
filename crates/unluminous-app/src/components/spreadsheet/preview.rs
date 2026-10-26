//! The print preview (`task-2360`, the `wiringb` lane): each page as the paginator lays it out, with its
//! header and footer, over the grid. It draws the pages `print::pdf_pages` makes, which are the pages the PDF
//! has, so what is seen here is what is exported and printed. The buttons push the words of the verbs
//! `print-preview`, `print`, `export` and `panel`, as a menu entry's do.

use egui::{Align, Layout, Rect, Sense, Vec2};

use super::grid::Frame;
use super::panes_b::{button, cached_layout, cached_pages, paint_page, preview_words};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::Book;

type Words = Vec<Vec<String>>;

/// Draw the preview over the grid and its headings.
pub fn show(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    let area = frame.cells.union(frame.column_header).union(frame.row_header);
    let layout = cached_layout(ui, book);
    let pages = cached_pages(ui, book, &layout);
    let total = pages.pages.len();
    let Some(preview) = book.view.lane_b.preview.as_mut() else { return };
    preview.pages = total;
    preview.page = preview.page.min(total.saturating_sub(1));
    let current = preview.page;
    let id = egui::Id::new(("spreadsheet-print-preview", &book.key));
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(area.min).show(ui.ctx(), |ui| {
        let (rect, _) = ui.allocate_exact_size(area.size(), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0, look.palette.panel);
        let bar = Rect::from_min_size(
            rect.min + Vec2::new(8.0, 6.0),
            Vec2::new(rect.width() - 16.0, 28.0),
        );
        toolbar(ui, bar, current, total, words);
        let room =
            Rect::from_min_max(rect.min + Vec2::new(12.0, 42.0), rect.max - Vec2::new(12.0, 12.0));
        if let Some(page) = pages.pages.get(current) {
            let scale = (room.width() / page.width).min(room.height() / page.height).max(0.05);
            let size = Vec2::new(page.width * scale, page.height * scale);
            let at = Rect::from_center_size(room.center(), size);
            ui.painter().rect_filled(
                at.translate(Vec2::new(3.0, 3.0)),
                0,
                crate::services::spreadsheet::excel_ink::PREVIEW_SHADOW,
            );
            paint_page(ui.painter(), at, page);
        } else {
            ui.painter().text(
                room.center(),
                egui::Align2::CENTER_CENTER,
                "There is nothing to print.",
                egui::FontId::proportional(14.0),
                look.palette.text_dim,
            );
        }
    });
}

/// The buttons above the page: previous and next, the page number, Page Setup, Print, PDF and Close.
fn toolbar(ui: &mut egui::Ui, bar: Rect, current: usize, total: usize, words: &mut Words) {
    let mut row = ui.new_child(
        egui::UiBuilder::new().max_rect(bar).layout(Layout::left_to_right(Align::Center)),
    );
    if button(&mut row, "\u{25C0}", "Previous page") && current > 0 {
        words.push(preview_words("previous"));
    }
    row.label(format!("Page {} of {}", current + 1, total.max(1)));
    if button(&mut row, "\u{25B6}", "Next page") && current + 1 < total {
        words.push(preview_words("next"));
    }
    row.separator();
    if button(&mut row, "Page Setup", "Page Setup") {
        words.push(vec!["panel".to_owned(), "Page Layout".to_owned()]);
    }
    if button(&mut row, "Print", "Print") {
        words.push(vec!["print".to_owned()]);
    }
    if button(&mut row, "Save as PDF", "Save as PDF") {
        words.push(vec!["export".to_owned(), "pdf".to_owned()]);
    }
    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if button(ui, "Close Print Preview", "Close Print Preview") {
            words.push(preview_words("close"));
        }
    });
}
