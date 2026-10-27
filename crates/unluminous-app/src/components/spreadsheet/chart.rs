//! Charts drawn over the cells: column, bar, line, area, pie, doughnut and scatter, in the workbook
//! theme's accent colours, from the values their series formulas give now. A chart read from a file whose
//! kind this version does not draw is a card naming its kind.

use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use unluminous_sheet::model::Chart;
use unluminous_sheet::plot::PlotSize;
use unluminous_sheet::styles::Color as SheetColor;

use super::chart_drag::{self, Drag, Grab};
use super::chart_paint::{self, Colours};
use super::grid::{Frame, Ink};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::Book;

/// The workbook theme's accent colour for a series.
fn accent(book: &Book, index: usize) -> Color32 {
    let theme = &book.session.book.styles.theme;
    let (r, g, b) =
        SheetColor::Theme(4 + (index % 6) as u32, 0.0).resolve(theme).unwrap_or((0x44, 0x72, 0xC4));
    Color32::from_rgb(r, g, b)
}

/// The colours a chart is drawn in: the paper and ink of the sheet, and the theme's six accents.
fn colours_of(book: &Book, ink: &Ink, look: &Look<'_>) -> Colours {
    // On white paper a chart's words are Excel's grey, the title as well as the labels.
    use crate::services::spreadsheet::excel_ink;
    let (text, dim) = match ink.dark {
        true => (ink.text, look.palette.text_dim),
        false => (excel_ink::CHART_TEXT, excel_ink::CHART_TEXT),
    };
    Colours {
        paper: ink.paper,
        text,
        dim,
        grid: ink.grid,
        accents: std::array::from_fn(|i| accent(book, i)),
    }
}

/// Where a chart sits on the screen.
pub fn rect_of(book: &Book, frame: Frame, chart: &Chart) -> Rect {
    let px = |emu: i64| (emu / unluminous_sheet::xlsx::chart::EMU_PER_PIXEL) as f32 * frame.zoom;
    let from = frame.cell(book, chart.from.0).min + Vec2::new(px(chart.from.1), px(chart.from.2));
    let to = frame.cell(book, chart.to.0).min + Vec2::new(px(chart.to.1), px(chart.to.2));
    Rect::from_min_max(from, to.max(from + Vec2::splat(40.0)))
}

/// Draw the pictures the sheet's drawing places over its cells.
pub fn show_pictures(ui: &egui::Ui, book: &mut Book, frame: Frame) {
    let painter = ui.painter_at(frame.cells);
    let pictures = book.session.book.sheets[book.view.sheet].pictures();
    for picture in pictures {
        let px =
            |emu: i64| (emu / unluminous_sheet::xlsx::chart::EMU_PER_PIXEL) as f32 * frame.zoom;
        let from = frame.cell(book, picture.from.0).min
            + Vec2::new(px(picture.from.1), px(picture.from.2));
        let to = frame.cell(book, picture.to.0).min + Vec2::new(px(picture.to.1), px(picture.to.2));
        let rect = Rect::from_min_max(from, to.max(from + Vec2::splat(4.0)));
        if !rect.intersects(frame.cells) {
            continue;
        }
        if !book.pictures.0.contains_key(&picture.part) {
            let texture = book
                .session
                .book
                .kept
                .get(&picture.part)
                .and_then(|bytes| crate::services::picture::decode_bytes(bytes).ok())
                .map(|image| {
                    crate::services::picture::upload(
                        ui.ctx(),
                        picture.part.clone(),
                        image,
                        egui::TextureOptions::LINEAR,
                    )
                });
            book.pictures.0.insert(picture.part.clone(), texture);
        }
        match book.pictures.0.get(&picture.part).and_then(Option::as_ref) {
            Some(texture) => {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                // any ground: the tint a picture is drawn with, which leaves its own colours as they are.
                let tint = Color32::WHITE;
                painter.image(texture.id(), rect, uv, tint);
            }
            None => {
                painter.rect_stroke(
                    rect,
                    0,
                    Stroke::new(1.0, crate::theme::color::divider()),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

/// Draw the chart of a chart sheet, filling the area; the sheet showing is a chart sheet. Answers whether the
/// chart was pressed and where a right click opened its menu.
pub fn show_chart_sheet(
    ui: &egui::Ui,
    book: &Book,
    area: Rect,
    ink: &Ink,
    look: &Look<'_>,
) -> (bool, Option<Pos2>) {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let Some(chart) = sheet.charts.first() else { return (false, None) };
    let painter = ui.painter_at(area);
    painter.rect_filled(area, 0, ink.paper);
    let response =
        ui.interact(area, ui.id().with(("chart-sheet", &book.key)), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Chart 1".to_owned())
    });
    draw_one(&painter, book, chart, area.shrink(16.0), ink, look, 1.0);
    let menu = response.secondary_clicked().then(|| response.interact_pointer_pos()).flatten();
    (response.clicked() || response.secondary_clicked(), menu)
}
/// What the charts of the sheet showing did this frame.
#[derive(Default)]
pub struct ChartOut {
    /// The chart pressed, by its place in the sheet's list.
    pub pressed: Option<usize>,
    /// Where a right click opened the chart's menu.
    pub menu: Option<Pos2>,
    /// The words of a drag that ended: move or size a chart.
    pub words: Vec<Vec<String>>,
}

/// Draw every chart of the sheet showing and read the pointer on them: a press chooses a chart, a right
/// click opens its menu, and dragging the body or a corner moves or sizes it.
pub fn show(
    ui: &egui::Ui,
    book: &Book,
    frame: Frame,
    ink: &Ink,
    look: &Look<'_>,
    chosen: Option<usize>,
) -> ChartOut {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let painter = ui.painter_at(frame.cells);
    let mut out = ChartOut::default();
    for (index, chart) in sheet.charts.iter().enumerate() {
        let rect = rect_of(book, frame, chart);
        if !rect.intersects(frame.cells) {
            continue;
        }
        let dragging = read_the_pointer(ui, book, frame, rect, index, &mut out);
        painter.rect(
            rect,
            CornerRadius::same(6),
            ink.paper,
            Stroke::new(1.0, ink.grid),
            egui::StrokeKind::Inside,
        );
        if chosen == Some(index) {
            painter.rect_stroke(
                rect.expand(1.0),
                CornerRadius::same(7),
                Stroke::new(2.0, ink.accent),
                egui::StrokeKind::Outside,
            );
        }
        draw_one(
            &painter,
            book,
            chart,
            rect.shrink(10.0 * frame.zoom.min(1.5)),
            ink,
            look,
            frame.zoom,
        );
        if let Some(landing) = dragging {
            painter.rect_stroke(
                landing,
                CornerRadius::same(6),
                Stroke::new(2.0, ink.accent),
                egui::StrokeKind::Outside,
            );
        }
    }
    out
}

/// Read the pointer on one chart. Answers where the chart would land while it is being dragged.
fn read_the_pointer(
    ui: &egui::Ui,
    book: &Book,
    frame: Frame,
    rect: Rect,
    index: usize,
    out: &mut ChartOut,
) -> Option<Rect> {
    let id = ui.id().with(("chart", &book.key, index));
    let response = ui.interact(rect.intersect(frame.cells), id, egui::Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Chart {}", index + 1))
    });
    if response.clicked() || response.drag_started() {
        out.pressed = Some(index);
    }
    if response.secondary_clicked() {
        out.pressed = Some(index);
        out.menu = response.interact_pointer_pos();
    }
    let memory = id.with("drag");
    if response.drag_started_by(egui::PointerButton::Primary) {
        let start = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
        let drag = Drag { grab: chart_drag::grab_at(rect, start), start, rect };
        ui.data_mut(|d| d.insert_temp(memory, drag));
    }
    let drag = ui.data(|d| d.get_temp::<Drag>(memory));
    let pointer = ui.input(|i| i.pointer.latest_pos());
    if let (Some(drag), Some(to)) = (drag, pointer) {
        if response.dragged() {
            ui.ctx().set_cursor_icon(chart_drag::cursor(drag.grab));
            return Some(chart_drag::landing(&drag, to));
        }
        if response.drag_stopped() {
            ui.data_mut(|d| d.remove::<Drag>(memory));
            let words = chart_drag::release_words(book, &frame, index, &drag, to);
            out.words.extend(Some(words).filter(|w| !w.is_empty()));
        }
    } else if let Some(p) = response.hover_pos() {
        if let Grab::Corner(..) = chart_drag::grab_at(rect, p) {
            ui.ctx().set_cursor_icon(chart_drag::cursor(chart_drag::grab_at(rect, p)));
        }
    }
    None
}
/// Draw one chart in its area, from what its series read now.
fn draw_one(
    painter: &egui::Painter,
    book: &Book,
    chart: &Chart,
    area: Rect,
    ink: &Ink,
    look: &Look<'_>,
    zoom: f32,
) {
    let size = PlotSize {
        width: f64::from(area.width() / zoom.max(0.1)),
        height: f64::from(area.height() / zoom.max(0.1)),
    };
    let plot = unluminous_sheet::plot::plot(&book.session.book, book.view.sheet, chart, size);
    chart_paint::draw(painter, area, &plot, chart, &colours_of(book, ink, look), zoom);
}
