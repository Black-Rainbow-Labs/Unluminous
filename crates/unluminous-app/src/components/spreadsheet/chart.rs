//! Charts drawn over the cells: column, bar, line, area, pie, doughnut and scatter, in the workbook
//! theme's accent colours, from the values their series formulas give now. A chart read from a file whose
//! kind this version does not draw is a card naming its kind.

use egui::{Color32, CornerRadius, Pos2, Rect, Shape, Stroke, Vec2};
use unluminous_sheet::model::{Chart, ChartKind};
use unluminous_sheet::styles::Color as SheetColor;
use unluminous_sheet::Value;

use super::grid::{Frame, Ink};
use crate::services::plugin_ui::Look;
use crate::services::spreadsheet::Book;

/// One series' numbers, its categories and its name, as they are now.
struct Data {
    name: String,
    categories: Vec<String>,
    values: Vec<Option<f64>>,
}

/// The values a chart formula gives, flattened.
fn values_of(book: &Book, formula: &str) -> Vec<Value> {
    let sheet = book.view.sheet;
    let at = unluminous_sheet::CellRef::new(0, 0);
    match unluminous_sheet::eval::evaluate(
        &book.session.book,
        sheet,
        at,
        &format!("={formula}"),
        book.session.now,
    ) {
        Ok((_, Some(array))) => array.data,
        Ok((value, None)) => vec![value],
        Err(_) => Vec::new(),
    }
}

fn series_data(book: &Book, chart: &Chart) -> Vec<Data> {
    chart
        .series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let name = s
                .name
                .as_ref()
                .and_then(|f| values_of(book, f).into_iter().next())
                .map(|v| v.to_text())
                .unwrap_or_else(|| format!("Series {}", i + 1));
            let categories = s
                .categories
                .as_ref()
                .map(|f| values_of(book, f).iter().map(|v| v.to_text()).collect())
                .unwrap_or_default();
            let values = values_of(book, &s.values).iter().map(|v| v.as_number()).collect();
            Data { name, categories, values }
        })
        .collect()
}

/// The workbook theme's accent colour for a series.
fn accent(book: &Book, index: usize) -> Color32 {
    let theme = &book.session.book.styles.theme;
    let (r, g, b) =
        SheetColor::Theme(4 + (index % 6) as u32, 0.0).resolve(theme).unwrap_or((0x44, 0x72, 0xC4));
    Color32::from_rgb(r, g, b)
}

/// Round numbers for an axis: from, to and the step between gridlines.
fn axis(low: f64, high: f64) -> (f64, f64, f64) {
    let (low, high) = if low == high { (low - 1.0, high + 1.0) } else { (low, high) };
    let raw = (high - low) / 5.0;
    let power = 10f64.powf(raw.abs().log10().floor());
    let step =
        [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * power).find(|s| *s >= raw).unwrap_or(raw);
    ((low / step).floor() * step, (high / step).ceil() * step, step)
}

fn short(n: f64) -> String {
    unluminous_sheet::numfmt::general(n, 8)
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

/// Draw every chart of the sheet showing. Answers the chart pressed, by its place in the sheet's list.
pub fn show(
    ui: &egui::Ui,
    book: &Book,
    frame: Frame,
    ink: &Ink,
    look: &Look<'_>,
    chosen: Option<usize>,
) -> Option<usize> {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let painter = ui.painter_at(frame.cells);
    let mut pressed = None;
    for (index, chart) in sheet.charts.iter().enumerate() {
        let rect = rect_of(book, frame, chart);
        if !rect.intersects(frame.cells) {
            continue;
        }
        let response = ui.interact(
            rect.intersect(frame.cells),
            ui.id().with(("chart", &book.key, index)),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Chart {}", index + 1))
        });
        if response.clicked() || response.secondary_clicked() {
            pressed = Some(index);
        }
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
    }
    pressed
}

fn draw_one(
    painter: &egui::Painter,
    book: &Book,
    chart: &Chart,
    area: Rect,
    ink: &Ink,
    look: &Look<'_>,
    zoom: f32,
) {
    let data = series_data(book, chart);
    let font = |size: f32| egui::FontId::proportional((size * zoom).clamp(7.0, 30.0));
    let mut top = area.top();
    let title = match &chart.title {
        Some(t) if t.starts_with('=') => {
            values_of(book, &t[1..]).into_iter().next().map(|v| v.to_text())
        }
        Some(t) => Some(t.clone()),
        None if data.len() == 1 && !matches!(chart.kind, ChartKind::Other(_)) => {
            Some(data[0].name.clone())
        }
        None => None,
    };
    if let Some(title) = &title {
        painter.text(
            Pos2::new(area.center().x, top),
            egui::Align2::CENTER_TOP,
            title,
            font(14.0),
            ink.text,
        );
        top += 22.0 * zoom;
    }
    if let ChartKind::Other(name) = chart.kind {
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            format!("A {name} chart, kept as Excel made it"),
            font(12.0),
            look.palette.text_dim,
        );
        return;
    }
    if data.is_empty() {
        return;
    }
    let pie = matches!(chart.kind, ChartKind::Pie | ChartKind::Doughnut);
    let legend_names: Vec<(String, Color32)> = if pie {
        data[0].categories.iter().enumerate().map(|(i, c)| (c.clone(), accent(book, i))).collect()
    } else {
        data.iter().enumerate().map(|(i, d)| (d.name.clone(), accent(book, i))).collect()
    };
    let legend = legend_names.len() > 1 || pie;
    let bottom = if legend { area.bottom() - 20.0 * zoom } else { area.bottom() };
    if legend {
        let mut x = area.left();
        for (name, color) in &legend_names {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(x, area.bottom() - 13.0 * zoom),
                    Vec2::splat(9.0 * zoom),
                ),
                2,
                *color,
            );
            let galley = painter.layout_no_wrap(name.clone(), font(11.0), ink.text);
            let w = galley.size().x;
            painter.galley(
                Pos2::new(x + 13.0 * zoom, area.bottom() - 15.0 * zoom),
                galley,
                ink.text,
            );
            x += w + 26.0 * zoom;
            if x > area.right() {
                break;
            }
        }
    }
    let plot = Rect::from_min_max(
        Pos2::new(area.left(), top + 4.0),
        Pos2::new(area.right(), bottom - 4.0),
    );
    if pie {
        draw_pie(painter, book, &data[0], plot, chart.kind == ChartKind::Doughnut, ink);
        return;
    }
    let numbers: Vec<f64> = data.iter().flat_map(|d| d.values.iter().flatten().copied()).collect();
    if numbers.is_empty() {
        return;
    }
    let (low, high, step) = if chart.kind == ChartKind::Scatter {
        axis(
            numbers.iter().copied().fold(f64::INFINITY, f64::min),
            numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    } else {
        axis(
            numbers.iter().copied().fold(0.0, f64::min),
            numbers.iter().copied().fold(0.0, f64::max),
        )
    };
    let horizontal = chart.kind == ChartKind::Bar;
    // Room for the axis labels.
    let label_width = 40.0 * zoom;
    let label_height = 16.0 * zoom;
    let inner = Rect::from_min_max(
        Pos2::new(plot.left() + if horizontal { 60.0 * zoom } else { label_width }, plot.top()),
        Pos2::new(plot.right(), plot.bottom() - label_height),
    );
    let scale = |v: f64| -> f32 { ((v - low) / (high - low)) as f32 };
    // Gridlines and the value labels.
    let mut tick = low;
    while tick <= high + step / 2.0 {
        let t = scale(tick);
        if horizontal {
            let x = inner.left() + inner.width() * t;
            painter.line_segment(
                [Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())],
                Stroke::new(1.0, ink.grid),
            );
            painter.text(
                Pos2::new(x, inner.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                short(tick),
                font(10.0),
                look.palette.text_dim,
            );
        } else {
            let y = inner.bottom() - inner.height() * t;
            painter.line_segment(
                [Pos2::new(inner.left(), y), Pos2::new(inner.right(), y)],
                Stroke::new(1.0, ink.grid),
            );
            painter.text(
                Pos2::new(inner.left() - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                short(tick),
                font(10.0),
                look.palette.text_dim,
            );
        }
        tick += step;
    }
    let count = data.iter().map(|d| d.values.len()).max().unwrap_or(0).max(1);
    let categories: Vec<String> = data[0].categories.clone();
    if chart.kind == ChartKind::Scatter {
        for (si, d) in data.iter().enumerate() {
            let xs: Vec<f64> = categories.iter().map(|c| c.parse::<f64>().unwrap_or(0.0)).collect();
            let (xl, xh) = if xs.is_empty() {
                (0.0, count as f64)
            } else {
                (
                    xs.iter().copied().fold(f64::INFINITY, f64::min),
                    xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                )
            };
            let (xl, xh, _) = axis(xl, xh);
            for (i, v) in d.values.iter().enumerate() {
                let Some(v) = v else { continue };
                let x = xs.get(i).copied().unwrap_or(i as f64 + 1.0);
                let p = Pos2::new(
                    inner.left() + inner.width() * ((x - xl) / (xh - xl)) as f32,
                    inner.bottom() - inner.height() * scale(*v),
                );
                painter.circle_filled(p, 3.5 * zoom, accent(book, si));
            }
        }
        return;
    }
    // Category labels.
    let band =
        if horizontal { inner.height() / count as f32 } else { inner.width() / count as f32 };
    for (i, c) in categories.iter().enumerate().take(count) {
        let middle = band * (i as f32 + 0.5);
        if horizontal {
            painter.text(
                Pos2::new(inner.left() - 4.0, inner.top() + middle),
                egui::Align2::RIGHT_CENTER,
                crate::components::controls::truncate_chars(c, 10, 8),
                font(10.0),
                look.palette.text_dim,
            );
        } else if band > 18.0 * zoom || i % ((18.0 * zoom / band).ceil() as usize).max(1) == 0 {
            painter.text(
                Pos2::new(inner.left() + middle, inner.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                crate::components::controls::truncate_chars(c, 10, 8),
                font(10.0),
                look.palette.text_dim,
            );
        }
    }
    let zero = scale(0.0f64.clamp(low, high));
    match chart.kind {
        ChartKind::Column | ChartKind::Bar => {
            let groups = data.len() as f32;
            let width = band * 0.64 / groups;
            for (si, d) in data.iter().enumerate() {
                for (i, v) in d.values.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let start = band * i as f32 + band * 0.18 + width * si as f32;
                    let (a, b) = (zero.min(scale(*v)), zero.max(scale(*v)));
                    let r = if horizontal {
                        Rect::from_min_max(
                            Pos2::new(inner.left() + inner.width() * a, inner.top() + start),
                            Pos2::new(
                                inner.left() + inner.width() * b,
                                inner.top() + start + width,
                            ),
                        )
                    } else {
                        Rect::from_min_max(
                            Pos2::new(inner.left() + start, inner.bottom() - inner.height() * b),
                            Pos2::new(
                                inner.left() + start + width,
                                inner.bottom() - inner.height() * a,
                            ),
                        )
                    };
                    painter.rect_filled(r, 0, accent(book, si));
                }
            }
        }
        ChartKind::Line | ChartKind::Area => {
            for (si, d) in data.iter().enumerate() {
                let points: Vec<Pos2> = d
                    .values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| {
                        v.map(|v| {
                            Pos2::new(
                                inner.left() + band * (i as f32 + 0.5),
                                inner.bottom() - inner.height() * scale(v),
                            )
                        })
                    })
                    .collect();
                let color = accent(book, si);
                if chart.kind == ChartKind::Area && points.len() > 1 {
                    let base = inner.bottom() - inner.height() * zero;
                    // One quad a step, each convex, so the fill is right whatever the line does.
                    for pair in points.windows(2) {
                        let quad = vec![
                            pair[0],
                            pair[1],
                            Pos2::new(pair[1].x, base),
                            Pos2::new(pair[0].x, base),
                        ];
                        painter.add(Shape::convex_polygon(
                            quad,
                            color.gamma_multiply(0.8),
                            Stroke::NONE,
                        ));
                    }
                }
                painter.add(Shape::line(points.clone(), Stroke::new(2.25 * zoom.max(0.6), color)));
                if chart.kind == ChartKind::Line {
                    for p in points {
                        painter.circle_filled(p, 3.0 * zoom, color);
                    }
                }
            }
        }
        _ => {}
    }
    painter.line_segment(
        [
            Pos2::new(inner.left(), inner.bottom() - inner.height() * zero),
            Pos2::new(inner.right(), inner.bottom() - inner.height() * zero),
        ],
        Stroke::new(1.0, look.palette.text_faint),
    );
}

fn draw_pie(
    painter: &egui::Painter,
    book: &Book,
    data: &Data,
    plot: Rect,
    doughnut: bool,
    ink: &Ink,
) {
    let values: Vec<f64> = data.values.iter().map(|v| v.unwrap_or(0.0).max(0.0)).collect();
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return;
    }
    let radius = plot.width().min(plot.height()) / 2.0 - 4.0;
    let centre = plot.center();
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, v) in values.iter().enumerate() {
        let sweep = v / total * std::f64::consts::TAU;
        let color = accent(book, i);
        // Each slice drawn as fans of at most a quarter turn, which are convex.
        let pieces = ((sweep / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
        for p in 0..pieces {
            let a0 = angle + sweep * p as f64 / pieces as f64;
            let a1 = angle + sweep * (p + 1) as f64 / pieces as f64;
            let steps = 24;
            let mut points = vec![centre];
            for s in 0..=steps {
                let a = a0 + (a1 - a0) * s as f64 / steps as f64;
                points.push(centre + Vec2::new(a.cos() as f32, a.sin() as f32) * radius);
            }
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        angle += sweep;
    }
    // The lines between slices, in the paper's colour.
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for v in &values {
        let edge = centre + Vec2::new(angle.cos() as f32, angle.sin() as f32) * radius;
        painter.line_segment([centre, edge], Stroke::new(1.5, ink.paper));
        angle += v / total * std::f64::consts::TAU;
    }
    if doughnut {
        painter.circle_filled(centre, radius * 0.55, ink.paper);
    }
}
