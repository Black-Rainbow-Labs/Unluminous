//! Sparklines drawn inside their cells: lines, columns, and wins and losses, with the markers and the high,
//! low, first, last and negative points a group switches on.
//!
//! [`draw_sparkline`] draws one into a rectangle from the numbers the engine gives it
//! (`unluminous_sheet::objects::sparkline::sparkline_data`); [`draw_sparklines`] is the loop the grid calls
//! once a frame. [`plot_points`] and [`bar_rects`] are the geometry, and are tested with no window.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use unluminous_sheet::objects::sparkline::{
    EmptyCells, SparklineData, SparklineGroup, SparklineKind,
};
use unluminous_sheet::styles::Color;

use super::grid::{Frame, Ink};
use crate::services::spreadsheet::Book;

/// The gap between a cell's edge and the plot, in points: measured off Excel, whose plot of a 60 point row
/// starts 4 pixels from the top and ends 5 from the bottom.
const INSET: f32 = 4.0;

/// The plot area inside a cell.
pub fn plot_area(cell: Rect, zoom: f32) -> Rect {
    cell.shrink2(Vec2::new(INSET * zoom, INSET * zoom))
}

/// The position of every value along the plot, `None` for a gap, left to right (right to left when the group
/// says so). A line puts the first value at the left edge and the last at the right.
pub fn plot_points(data: &SparklineData, group: &SparklineGroup, plot: Rect) -> Vec<Option<Pos2>> {
    let n = data.values.len();
    data.values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let v = (*v)?;
            let along = if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
            let along = if group.right_to_left { 1.0 - along } else { along };
            let down = data.fraction_down(v) as f32;
            Some(Pos2::new(plot.left() + along * plot.width(), plot.top() + down * plot.height()))
        })
        .collect()
}

/// The rectangle of every column or win/loss block along the plot, `None` for a gap or a zero. A column rises
/// from the baseline to its value and is always at least one point tall; a win/loss block is half the plot
/// high above the middle for a positive value and below it for a negative one.
pub fn bar_rects(data: &SparklineData, group: &SparklineGroup, plot: Rect) -> Vec<Option<Rect>> {
    let n = data.values.len().max(1);
    let slot = plot.width() / n as f32;
    let width = (slot * 0.8).max(1.0);
    (0..data.values.len())
        .map(|i| {
            let v = data.values[i]?;
            let index = if group.right_to_left { n - 1 - i } else { i };
            let x = plot.left() + slot * index as f32 + (slot - width) / 2.0;
            let (top, bottom) = match group.kind {
                SparklineKind::WinLoss => {
                    if v == 0.0 {
                        return None;
                    }
                    let mid = plot.center().y;
                    let half = plot.height() / 2.0 - 1.0;
                    if v > 0.0 {
                        (mid - half, mid - 1.0)
                    } else {
                        (mid + 1.0, mid + half)
                    }
                }
                _ => {
                    let (a, b) = data.bar_span(i)?;
                    let (top, bottom) = (
                        plot.top() + a as f32 * plot.height(),
                        plot.top() + b as f32 * plot.height(),
                    );
                    (top.min(bottom - 1.0), bottom)
                }
            };
            Some(Rect::from_min_max(Pos2::new(x, top), Pos2::new(x + width, bottom)))
        })
        .collect()
}

fn colour(c: &Option<Color>, theme: &[u32], fallback: Color32) -> Color32 {
    c.as_ref()
        .and_then(|c| c.resolve(theme))
        .map_or(fallback, |(r, g, b)| Color32::from_rgb(r, g, b))
}

/// Draw one sparkline into the cell it is in.
pub fn draw_sparkline(
    painter: &Painter,
    cell: Rect,
    data: &SparklineData,
    group: &SparklineGroup,
    theme: &[u32],
    zoom: f32,
    ink: &Ink,
) {
    let plot = plot_area(cell, zoom);
    if plot.width() < 2.0 || plot.height() < 2.0 {
        return;
    }
    let series = colour(&group.colors.series, theme, ink.accent);
    let negative = colour(&group.colors.negative, theme, Color32::from_rgb(0xD0, 0, 0));
    if group.show_axis && data.axis_min < 0.0 && data.axis_max > 0.0 {
        let y = plot.top() + data.fraction_down(0.0) as f32 * plot.height();
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0, colour(&group.colors.axis, theme, ink.text)),
        );
    }
    let points = plot_points(data, group, plot);
    if group.kind == SparklineKind::Line {
        draw_line(painter, &points, group, series, zoom);
    } else {
        for (i, rect) in bar_rects(data, group, plot).into_iter().enumerate() {
            let Some(rect) = rect else { continue };
            let is_negative = data.values[i].is_some_and(|v| v < 0.0);
            let fill = if is_negative && group.negative { negative } else { series };
            painter.rect_filled(rect, 0, fill);
        }
    }
    draw_points(painter, data, group, &points, plot, theme, zoom, series, negative);
}

fn draw_line(
    painter: &Painter,
    points: &[Option<Pos2>],
    group: &SparklineGroup,
    series: Color32,
    zoom: f32,
) {
    let width = group.weight.unwrap_or(0.75) as f32 * 96.0 / 72.0 * zoom;
    let stroke = Stroke::new(width.max(1.0), series);
    let mut run: Vec<Pos2> = Vec::new();
    for p in points {
        match p {
            Some(p) => run.push(*p),
            None if group.empty == EmptyCells::Span => {}
            None => {
                if run.len() > 1 {
                    painter.add(Shape::line(std::mem::take(&mut run), stroke));
                }
                run.clear();
            }
        }
    }
    if run.len() > 1 {
        painter.add(Shape::line(run, stroke));
    }
}

/// The marker, high, low, first, last and negative points the group switches on.
#[allow(clippy::too_many_arguments)]
fn draw_points(
    painter: &Painter,
    data: &SparklineData,
    group: &SparklineGroup,
    points: &[Option<Pos2>],
    plot: Rect,
    theme: &[u32],
    zoom: f32,
    series: Color32,
    negative: Color32,
) {
    let radius = 2.5 * zoom;
    let dot = |i: usize, c: Color32| {
        let at = match group.kind {
            SparklineKind::Line => points.get(i).copied().flatten(),
            _ => bar_rects(data, group, plot).get(i).copied().flatten().map(|r| {
                if data.values[i].is_some_and(|v| v < 0.0) {
                    r.center_bottom()
                } else {
                    r.center_top()
                }
            }),
        };
        if let Some(at) = at {
            painter.circle_filled(at, radius, c);
        }
    };
    if group.kind == SparklineKind::Line {
        if group.markers {
            let c = colour(&group.colors.markers, theme, series);
            (0..points.len()).for_each(|i| dot(i, c));
        }
        if group.negative {
            data.negatives.iter().for_each(|i| dot(*i, negative));
        }
    }
    for (on, at, c) in [
        (group.first, data.first, &group.colors.first),
        (group.last, data.last, &group.colors.last),
        (group.high, data.high, &group.colors.high),
        (group.low, data.low, &group.colors.low),
    ] {
        if let (true, Some(i)) = (on, at) {
            if group.kind == SparklineKind::Line {
                dot(i, colour(c, theme, series));
            } else if let Some(Some(r)) = bar_rects(data, group, plot).get(i) {
                painter.rect_filled(*r, 0, colour(c, theme, series));
            }
        }
    }
}

/// Draw every sparkline whose cell shows.
pub fn draw_sparklines(ui: &egui::Ui, book: &Book, frame: Frame, ink: &Ink) {
    let sheet = &book.session.book.sheets[book.view.sheet];
    if sheet.sparklines.is_empty() {
        return;
    }
    let painter = ui.painter_at(frame.cells);
    let theme = &book.session.book.styles.theme;
    for (g, group) in sheet.sparklines.iter().enumerate() {
        for (i, sp) in group.sparklines.iter().enumerate() {
            let cell = frame.cell(book, sp.location);
            if !cell.intersects(frame.cells) {
                continue;
            }
            if let Some(data) = unluminous_sheet::objects::sparkline::sparkline_data(
                &book.session.book,
                book.view.sheet,
                g,
                i,
            ) {
                draw_sparkline(&painter, cell, &data, group, theme, frame.zoom, ink);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::coord::{CellRef, Range};
    use unluminous_sheet::edit::Session;
    use unluminous_sheet::model::Workbook;

    fn data_for(kind: SparklineKind, values: [f64; 4]) -> (SparklineData, SparklineGroup) {
        let mut s = Session::new(Workbook::new(), 46318.5);
        for (c, v) in values.iter().enumerate() {
            s.set_value(0, CellRef::new(0, c as u32), unluminous_sheet::Value::Number(*v)).unwrap();
        }
        let g = s
            .insert_sparklines(
                0,
                kind,
                0,
                Range::parse("A1:D1").unwrap(),
                Range::parse("F1").unwrap(),
            )
            .unwrap();
        (
            unluminous_sheet::objects::sparkline::sparkline_data(&s.book, 0, g, 0).unwrap(),
            s.book.sheets[0].sparklines[g].clone(),
        )
    }

    #[test]
    fn a_line_runs_from_the_left_edge_to_the_right_and_the_highest_value_is_at_the_top() {
        let (data, group) = data_for(SparklineKind::Line, [5.0, 7.0, 9.0, 6.0]);
        let plot = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(90.0, 60.0));
        let p = plot_points(&data, &group, plot);
        assert_eq!(p[0], Some(Pos2::new(0.0, 60.0)));
        assert_eq!(p[3].unwrap().x, 90.0);
        assert_eq!(p[2].unwrap().y, 0.0);
    }

    #[test]
    fn a_right_to_left_group_reverses_the_plot() {
        let (data, mut group) = data_for(SparklineKind::Line, [5.0, 7.0, 9.0, 6.0]);
        group.right_to_left = true;
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(90.0, 60.0));
        assert_eq!(plot_points(&data, &group, plot)[0].unwrap().x, 90.0);
    }

    #[test]
    fn columns_rise_from_the_baseline_and_a_win_loss_splits_about_the_middle() {
        let (data, group) = data_for(SparklineKind::Column, [1.0, -2.0, 3.0, -4.0]);
        let plot = Rect::from_min_size(Pos2::ZERO, Vec2::new(80.0, 70.0));
        let bars = bar_rects(&data, &group, plot);
        let zero = 3.0 / 7.0 * 70.0;
        assert!((bars[0].unwrap().bottom() - zero).abs() < 0.01, "a positive bar ends at zero");
        assert!((bars[1].unwrap().top() - zero).abs() < 0.01, "a negative bar starts at zero");
        let (data, group) = data_for(SparklineKind::WinLoss, [1.0, -2.0, 0.0, 4.0]);
        let bars = bar_rects(&data, &group, plot);
        assert!(bars[0].unwrap().bottom() < 35.0 && bars[1].unwrap().top() > 35.0);
        assert!(bars[2].is_none(), "a zero has no block");
        assert_eq!(bars[0].unwrap().height(), bars[3].unwrap().height());
    }
}
