//! The popups of the clipboard and fill, page layout and print features (`task-2360`, the `wiringb` lane): the
//! Paste Special popup, the Paste Options and Auto Fill Options buttons, the page breaks and pages of the page
//! break preview and page layout view, and the print preview. Each one asks for a change by pushing the words
//! of a verb, as a menu entry does, so a press and an agent's call are the same call.

use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use unluminous_sheet::edit::{PasteKind, PasteOperation};
use unluminous_sheet::model::ViewKind;
use unluminous_sheet::pages::layout::PageLayout;
use unluminous_word::layout::{Item, Pages};

use super::grid::Frame;
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::catalogue::Item as Entry;
use crate::services::spreadsheet::view_b::{AfterKind, PasteDraft};
use crate::services::spreadsheet::{catalogue_b, print, Book};

type Words = Vec<Vec<String>>;

fn say(text: &str) -> Vec<String> {
    text.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// Draw every popup and mark this lane owns that is open.
pub fn show(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    page_marks(ui, book, frame, look);
    if book.view.lane_b.paste_draft.is_some() {
        paste_special_popup(ui, book, frame, look, words);
    }
    after_button(ui, book, frame, look, words);
    if book.view.lane_b.preview.is_some() {
        super::preview::show(ui, book, frame, look, words);
    }
    super::panes_b_pivot::show(ui, book, frame, look, words);
}

/// A floating card with the palette's menu colours.
pub fn card(
    ui: &egui::Ui,
    look: &Look<'_>,
    id: egui::Id,
    at: Pos2,
    add: impl FnOnce(&mut egui::Ui),
) -> Rect {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .fill(look.palette.menu)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(10))
                .show(ui, add);
        })
        .response
        .rect
}

/// A button with an accessible name.
pub fn button(ui: &mut egui::Ui, label: &str, name: &str) -> bool {
    let r = ui.button(label);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    r.clicked()
}

// -------------------------------------------------------------------------------------- Paste Special

/// The letters Excel shows underlined in the Paste Special box, with what each chooses.
pub const PASTE_LETTERS: [(char, &str, PasteKind); 12] = [
    ('A', "All", PasteKind::All),
    ('F', "Formulas", PasteKind::Formulas),
    ('V', "Values", PasteKind::Values),
    ('T', "Formats", PasteKind::Formats),
    ('C', "Comments and notes", PasteKind::Comments),
    ('N', "Validation", PasteKind::Validation),
    ('H', "All using source theme", PasteKind::AllUsingSourceTheme),
    ('X', "All except borders", PasteKind::AllExceptBorders),
    ('W', "Column widths", PasteKind::ColumnWidths),
    ('R', "Formulas and number formats", PasteKind::FormulasAndNumberFormats),
    ('U', "Values and number formats", PasteKind::ValuesAndNumberFormats),
    ('G', "All, merging conditional formats", PasteKind::AllMergingConditionalFormats),
];

/// The operation letters.
pub const OPERATION_LETTERS: [(char, &str, PasteOperation); 5] = [
    ('O', "None", PasteOperation::None),
    ('D', "Add", PasteOperation::Add),
    ('S', "Subtract", PasteOperation::Subtract),
    ('M', "Multiply", PasteOperation::Multiply),
    ('I', "Divide", PasteOperation::Divide),
];

/// The Paste Special popup under the selection: the kinds and operations with their letters, the two ticks,
/// and Paste Link, with OK and Cancel.
fn paste_special_popup(
    ui: &egui::Ui,
    book: &mut Book,
    frame: Frame,
    look: &Look<'_>,
    words: &mut Words,
) {
    let anchor = frame.range(book, book.view.range()).left_bottom() + Vec2::new(0.0, 4.0);
    let Some(mut draft) = book.view.lane_b.paste_draft else { return };
    let before = draft;
    let mut finish: Option<bool> = None;
    let id = egui::Id::new(("spreadsheet-paste-special", &book.key));
    card(ui, look, id, anchor, |ui| {
        ui.set_width(300.0);
        ui.label(egui::RichText::new("Paste Special").strong().color(look.palette.text_strong));
        ui.label(egui::RichText::new("Press a letter, then Enter").color(look.palette.text_dim));
        ui.separator();
        for (letter, label, kind) in PASTE_LETTERS {
            let on = draft.kind == kind && !draft.link;
            if ui.selectable_label(on, format!("{label}  ({letter})")).clicked() {
                draft.kind = kind;
                draft.link = false;
            }
        }
        ui.separator();
        for (letter, label, operation) in OPERATION_LETTERS {
            if ui
                .selectable_label(draft.operation == operation, format!("{label}  ({letter})"))
                .clicked()
            {
                draft.operation = operation;
            }
        }
        ui.separator();
        ui.checkbox(&mut draft.skip_blanks, "Skip blanks  (B)");
        ui.checkbox(&mut draft.transpose, "Transpose  (E)");
        if ui.selectable_label(draft.link, "Paste Link  (L)").clicked() {
            draft.link = !draft.link;
        }
        ui.horizontal(|ui| {
            if button(ui, "OK", "Paste Special OK") {
                finish = Some(true);
            }
            if button(ui, "Cancel", "Paste Special Cancel") {
                finish = Some(false);
            }
        });
    });
    match finish {
        Some(true) => {
            words.push(draft.words());
            book.view.lane_b.paste_draft = None;
        }
        Some(false) => book.view.lane_b.paste_draft = None,
        None => {
            if draft != before {
                book.view.lane_b.paste_draft = Some(draft);
            }
        }
    }
}

/// Apply a letter pressed while the popup is open: choose the kind or operation, switch an option, or paste
/// the link. Returns `true` when the letter meant something to the popup.
pub fn press_letter(draft: &mut PasteDraft, letter: char) -> bool {
    let upper = letter.to_ascii_uppercase();
    if let Some((_, _, kind)) = PASTE_LETTERS.iter().find(|(l, _, _)| *l == upper) {
        draft.kind = *kind;
        draft.link = false;
        return true;
    }
    if let Some((_, _, operation)) = OPERATION_LETTERS.iter().find(|(l, _, _)| *l == upper) {
        draft.operation = *operation;
        return true;
    }
    match upper {
        'B' => draft.skip_blanks = !draft.skip_blanks,
        'E' => draft.transpose = !draft.transpose,
        'L' => draft.link = !draft.link,
        _ => return false,
    }
    true
}

// -------------------------------------------------------------------- Paste Options and Auto Fill Options

/// The small button beside the cells a paste or a fill just wrote, with its menu.
fn after_button(ui: &egui::Ui, book: &mut Book, frame: Frame, look: &Look<'_>, words: &mut Words) {
    let Some(after) = book.view.lane_b.after.clone() else { return };
    if after.revision != book.revision || after.sheet != book.view.sheet {
        book.view.lane_b.after = None;
        return;
    }
    let cell = frame.range(book, after.target);
    let spot =
        Rect::from_min_size(cell.right_bottom() + Vec2::new(2.0, 2.0), Vec2::new(24.0, 18.0));
    if !frame.cells.intersects(spot) {
        return;
    }
    let (label, name, items) = match after.kind {
        AfterKind::Paste => ("\u{1F4CB}", "Paste Options", catalogue_b::paste_options_items()),
        AfterKind::Fill => ("\u{2795}", "Auto Fill Options", catalogue_b::fill_options_items()),
    };
    let id = egui::Id::new(("spreadsheet-after", &book.key));
    let r = ui.interact(spot, id.with("button"), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    ui.painter().rect_filled(spot, 3, look.palette.menu);
    ui.painter().rect_stroke(
        spot,
        3,
        Stroke::new(1.0, look.palette.control_border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        spot.center(),
        Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        look.palette.text,
    );
    egui::Popup::from_toggle_button_response(&r).id(id.with("menu")).show(|ui| {
        for item in &items {
            if let Entry::Run { label, words: w, .. } = item {
                if button(ui, label, label) {
                    words.push(w.clone());
                }
            } else if matches!(item, Entry::Line) {
                ui.separator();
            }
        }
    });
}

// ------------------------------------------------------------------------------ page break and layout

/// The pages of the sheet showing, laid out again only when the workbook changed.
pub fn cached_layout(ui: &egui::Ui, book: &Book) -> Arc<PageLayout> {
    let id = egui::Id::new(("spreadsheet-pages", &book.key));
    if let Some((revision, sheet, layout)) =
        ui.data(|d| d.get_temp::<(u64, usize, Arc<PageLayout>)>(id))
    {
        if revision == book.revision && sheet == book.view.sheet {
            return layout;
        }
    }
    let layout = Arc::new(print::layout_of(book));
    ui.data_mut(|d| d.insert_temp(id, (book.revision, book.view.sheet, layout.clone())));
    layout
}

/// The page breaks of the page break preview, and the pages of the page layout view: dashed lines where the
/// pages break by themselves, solid ones where a person broke them, and the page numbers behind the cells.
fn page_marks(ui: &egui::Ui, book: &Book, frame: Frame, look: &Look<'_>) {
    let kind = book.session.book.sheets[book.view.sheet].view.kind;
    if kind == ViewKind::Normal || book.view.axes.is_none() {
        return;
    }
    let layout = cached_layout(ui, book);
    let painter = ui.painter().with_clip_rect(frame.cells);
    let blue = Color32::from_rgb(0x1F, 0x6F, 0xD0);
    for area in &layout.areas {
        let whole = frame.range(book, area.range);
        for b in area
            .row_breaks
            .iter()
            .filter(|b| b.at > area.range.first.row && b.at <= area.range.last.row)
        {
            let y = frame.y(book, b.at);
            line(&painter, Pos2::new(whole.left(), y), Pos2::new(whole.right(), y), blue, b.manual);
        }
        for b in area
            .col_breaks
            .iter()
            .filter(|b| b.at > area.range.first.col && b.at <= area.range.last.col)
        {
            let x = frame.x(book, b.at);
            line(&painter, Pos2::new(x, whole.top()), Pos2::new(x, whole.bottom()), blue, b.manual);
        }
        if kind == ViewKind::PageBreakPreview {
            painter.rect_stroke(whole, 0, Stroke::new(2.0, blue), egui::StrokeKind::Outside);
        }
    }
    for page in &layout.pages {
        let rect = frame.range(book, page.body);
        if kind == ViewKind::PageBreakPreview {
            let size = (rect.width().min(rect.height()) / 3.0).clamp(18.0, 120.0);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("Page {}", page.number),
                egui::FontId::proportional(size),
                look.palette.text_faint.gamma_multiply(0.45),
            );
        } else {
            page_layout_strips(&painter, rect, page, look);
        }
    }
}

fn line(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32, solid: bool) {
    if solid {
        painter.line_segment([a, b], Stroke::new(2.0, color));
    } else {
        painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, color), 6.0, 4.0));
    }
}

/// The header above a page's cells and the footer below them, in the page layout view.
fn page_layout_strips(
    painter: &egui::Painter,
    rect: Rect,
    page: &unluminous_sheet::pages::layout::Page,
    look: &Look<'_>,
) {
    use unluminous_sheet::pages::headerfooter::Sections;
    let join = |s: &Sections| {
        [&s.left, &s.center, &s.right]
            .iter()
            .map(|runs| Sections::plain(runs))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("   ")
    };
    let font = egui::FontId::proportional(10.0);
    painter.text(
        rect.left_top() + Vec2::new(4.0, -4.0),
        Align2::LEFT_BOTTOM,
        join(&page.header),
        font.clone(),
        look.palette.text_dim,
    );
    painter.text(
        rect.left_bottom() + Vec2::new(4.0, 4.0),
        Align2::LEFT_TOP,
        join(&page.footer),
        font,
        look.palette.text_dim,
    );
}

/// The print preview's pages as the PDF writer has them, made again only when the workbook changed.
pub fn cached_pages(ui: &egui::Ui, book: &Book, layout: &PageLayout) -> Arc<Pages> {
    let id = egui::Id::new(("spreadsheet-preview-pages", &book.key));
    if let Some((revision, sheet, pages)) = ui.data(|d| d.get_temp::<(u64, usize, Arc<Pages>)>(id))
    {
        if revision == book.revision && sheet == book.view.sheet {
            return pages;
        }
    }
    let mut measurer = print::Measurer::new();
    let pages = Arc::new(print::pdf_pages(book, layout, &mut measurer));
    ui.data_mut(|d| d.insert_temp(id, (book.revision, book.view.sheet, pages.clone())));
    pages
}

/// Draw one page of a printout into a rectangle: the paper, then each item of the page.
pub fn paint_page(painter: &egui::Painter, rect: Rect, page: &unluminous_word::layout::Page) {
    painter.rect_filled(rect, 0, Color32::WHITE);
    let scale = rect.width() / page.width.max(1.0);
    let at = |x: f32, y: f32| rect.min + Vec2::new(x * scale, y * scale);
    let painter = painter.with_clip_rect(rect);
    for item in &page.items {
        match item {
            Item::Fill { rect: r, color } => {
                let area = Rect::from_min_size(at(r.x, r.y), Vec2::new(r.w * scale, r.h * scale));
                painter.rect_filled(area, 0, Color32::from_rgb(color.0, color.1, color.2));
            }
            Item::Rule { x0, y0, x1, y1, width, color, .. } => {
                painter.line_segment(
                    [at(*x0, *y0), at(*x1, *y1)],
                    Stroke::new(
                        (width * scale).max(0.5),
                        Color32::from_rgb(color.0, color.1, color.2),
                    ),
                );
            }
            Item::Text(t) => {
                let c = t.style.color;
                painter.text(
                    at(t.xs.first().copied().unwrap_or(0.0), t.baseline),
                    Align2::LEFT_BOTTOM,
                    &t.text,
                    egui::FontId::proportional((t.style.size * scale).clamp(3.0, 80.0)),
                    Color32::from_rgb(c.r, c.g, c.b),
                );
            }
            Item::Picture { .. } | Item::Placeholder { .. } => {}
        }
    }
}

/// The words of a chord pressed in the preview or the popup, shared with the keys.
pub fn preview_words(how: &str) -> Vec<String> {
    say(&format!("print-preview {how}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_letters_of_paste_special_choose_what_excel_chooses() {
        let mut draft = PasteDraft::default();
        for letter in ['v', 'D', 'b', 'E'] {
            assert!(press_letter(&mut draft, letter));
        }
        assert_eq!(draft.words(), say("paste-special values add skip-blanks transpose"));
        assert!(press_letter(&mut draft, 'x'));
        assert_eq!(draft.kind, PasteKind::AllExceptBorders);
        assert!(press_letter(&mut draft, 'l'));
        assert_eq!(draft.words(), say("paste-special link"));
        assert!(!press_letter(&mut draft, 'z'), "z means nothing to the popup");
    }

    #[test]
    fn every_letter_of_the_popup_is_different() {
        let mut seen = std::collections::HashSet::new();
        for (letter, _, _) in PASTE_LETTERS {
            assert!(seen.insert(letter));
        }
        for (letter, _, _) in OPERATION_LETTERS {
            assert!(seen.insert(letter));
        }
        assert!(seen.insert('B') && seen.insert('E') && seen.insert('L'));
    }
}
