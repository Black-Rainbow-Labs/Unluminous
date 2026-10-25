//! Drawing one chart from what [`unluminous_sheet::plot::plot`] worked out.
//!
//! Every chart type Excel inserts is drawn here: columns, bars, lines, areas, scatter, bubbles, stock
//! charts, pies, doughnuts, radar, surfaces and the newer treemap, sunburst, histogram, Pareto, box and
//! whisker, waterfall and funnel charts. A 3D type is drawn as its flat kind with depth shading, and a map
//! chart is a card naming it. The functions take a painter, a rectangle and the computed data, and decide
//! nothing about the workbook.
//!
//! The arithmetic that places bars, points and labels is in [`geometry`], with no window behind it.

use egui::epaint::TextShape;
use egui::{Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use unluminous_sheet::chartspec::*;
use unluminous_sheet::model::Chart;
use unluminous_sheet::plot::{BoxStats, ExtendedPlot, Plot, PlotAxis, PlotSeries, WaterfallBar, WaterfallKind};

/// The colours a chart is drawn in.
#[derive(Debug, Clone)]
pub struct Colours {
    pub paper: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub grid: Color32,
    /// The workbook theme's six accent colours, which series take in turn.
    pub accents: [Color32; 6],
}

impl Colours {
    /// The colour of series number `i`: the accents in turn, darker and then lighter on each lap.
    pub fn series(&self, i: usize) -> Color32 {
        let base = self.accents[i % 6];
        match i / 6 % 3 {
            0 => base,
            1 => shade(base, 0.6),
            _ => lighten(base, 0.4),
        }
    }
}

/// A colour with each channel scaled, as Excel's `lumMod` darkens an accent.
pub fn shade(c: Color32, by: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r() as f32 * by) as u8,
        (c.g() as f32 * by) as u8,
        (c.b() as f32 * by) as u8,
        c.a(),
    )
}

/// A colour moved towards white.
pub fn lighten(c: Color32, by: f32) -> Color32 {
    let mix = |v: u8| (v as f32 + (255.0 - v as f32) * by) as u8;
    Color32::from_rgba_unmultiplied(mix(c.r()), mix(c.g()), mix(c.b()), c.a())
}

/// The sizes text and marks are drawn at, for a zoom.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub zoom: f32,
}

impl Metrics {
    fn font(&self, size: f32) -> FontId {
        FontId::proportional((size * self.zoom).clamp(6.0, 36.0))
    }

    fn label(&self) -> FontId {
        self.font(9.0)
    }

    fn title(&self) -> FontId {
        self.font(14.0)
    }

    fn line(&self) -> f32 {
        1.0 * self.zoom.max(0.7)
    }
}

/// The part of a theme or literal colour an `spPr` fill names, when it names one.
pub fn fill_of(sp_pr: &Option<String>, accents: &[Color32; 6]) -> Option<Color32> {
    let xml = sp_pr.as_deref()?;
    let start = xml.find("<a:solidFill>")?;
    let end = xml[start..].find("</a:solidFill>")? + start;
    let body = &xml[start..end];
    let mut colour = if let Some(at) = body.find("srgbClr val=\"") {
        let hex = &body[at + 13..at + 19];
        let v = u32::from_str_radix(hex, 16).ok()?;
        Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    } else if let Some(at) = body.find("schemeClr val=\"") {
        let name = &body[at + 15..];
        let name = &name[..name.find('"')?];
        match name {
            "accent1" => accents[0],
            "accent2" => accents[1],
            "accent3" => accents[2],
            "accent4" => accents[3],
            "accent5" => accents[4],
            "accent6" => accents[5],
            "tx1" | "dk1" => Color32::BLACK,
            "bg1" | "lt1" => Color32::WHITE,
            _ => Color32::GRAY,
        }
    } else {
        return None;
    };
    if let Some(at) = body.find("lumMod val=\"") {
        let v: f32 = body[at + 12..].split('"').next()?.parse().ok()?;
        colour = shade(colour, v / 100_000.0);
    }
    if let Some(at) = body.find("lumOff val=\"") {
        let v: f32 = body[at + 12..].split('"').next()?.parse().ok()?;
        colour = lighten(colour, v / 100_000.0);
    }
    if let Some(at) = body.find("alpha val=\"") {
        let v: f32 = body[at + 11..].split('"').next()?.parse().ok()?;
        colour = colour.gamma_multiply(v / 100_000.0);
    }
    Some(colour)
}

/// Whether an `spPr` turns its fill off.
fn no_fill(sp_pr: &Option<String>) -> bool {
    sp_pr.as_deref().is_some_and(|x| x.trim_start_matches("<c:spPr>").starts_with("<a:noFill/>"))
}

/// Whether an `spPr` turns its line off.
fn no_line(sp_pr: &Option<String>) -> bool {
    sp_pr.as_deref().is_some_and(|x| x.contains("<a:ln") && {
        let at = x.find("<a:ln").unwrap_or(0);
        x[at..].find("</a:ln>").is_some_and(|e| x[at..at + e].contains("<a:noFill/>"))
    })
}

/// Placing things: bars in their bands, points on their scales, labels beside them, with no window.
pub mod geometry {
    use egui::{Pos2, Rect};

    /// Where one series' bar sits inside a category band: its offset from the band's start and its width.
    ///
    /// `gap` is Excel's gap width (a percentage of one bar) and `overlap` its overlap (-100 to 100).
    pub fn bar_slot(band: f32, series: usize, count: usize, gap: f32, overlap: f32) -> (f32, f32) {
        let count = count.max(1) as f32;
        let ov = overlap / 100.0;
        // band = width * (count - (count - 1) * ov + gap / 100)
        let width = band / (count - (count - 1.0) * ov + gap / 100.0);
        let used = width * (count - (count - 1.0) * ov);
        let start = (band - used) / 2.0;
        (start + series as f32 * width * (1.0 - ov), width)
    }

    /// A value's place along an axis of a given length: 0 at the minimum end.
    pub fn along(value: f64, min: f64, max: f64, log: bool, reverse: bool) -> f32 {
        let t = if log {
            let (a, b, v) = (min.max(f64::MIN_POSITIVE).ln(), max.max(f64::MIN_POSITIVE).ln(), value.max(f64::MIN_POSITIVE).ln());
            (v - a) / (b - a)
        } else if max == min {
            0.5
        } else {
            (value - min) / (max - min)
        };
        let t = t as f32;
        if reverse {
            1.0 - t
        } else {
            t
        }
    }

    /// The centre of a category band.
    pub fn band_centre(index: usize, count: usize, length: f32) -> f32 {
        length * (index as f32 + 0.5) / count.max(1) as f32
    }

    /// The rectangle of a vertical bar from `a` to `b` (as fractions of the plot height) in a plot.
    pub fn column(plot: Rect, offset: f32, width: f32, a: f32, b: f32) -> Rect {
        let (lo, hi) = (a.min(b), a.max(b));
        Rect::from_min_max(
            Pos2::new(plot.left() + offset, plot.bottom() - plot.height() * hi),
            Pos2::new(plot.left() + offset + width, plot.bottom() - plot.height() * lo),
        )
    }

    /// The rectangle of a horizontal bar from `a` to `b` (fractions of the plot width).
    pub fn bar(plot: Rect, offset: f32, width: f32, a: f32, b: f32) -> Rect {
        let (lo, hi) = (a.min(b), a.max(b));
        Rect::from_min_max(
            Pos2::new(plot.left() + plot.width() * lo, plot.top() + offset),
            Pos2::new(plot.left() + plot.width() * hi, plot.top() + offset + width),
        )
    }

    /// The angles (radians, clockwise from twelve o'clock) a pie's slices span, from a first angle.
    pub fn slices(values: &[f64], first_degrees: f32) -> Vec<(f64, f64)> {
        let total: f64 = values.iter().map(|v| v.max(0.0)).sum();
        let mut at = f64::from(first_degrees).to_radians();
        values
            .iter()
            .map(|v| {
                let sweep = if total > 0.0 { v.max(0.0) / total * std::f64::consts::TAU } else { 0.0 };
                let span = (at, at + sweep);
                at += sweep;
                span
            })
            .collect()
    }

    /// A point on a circle at a clockwise angle from twelve o'clock.
    pub fn on_circle(centre: Pos2, radius: f32, angle: f64) -> Pos2 {
        Pos2::new(centre.x + radius * angle.sin() as f32, centre.y - radius * angle.cos() as f32)
    }

    /// Lay out values as a treemap in a rectangle: each rectangle's area is its share, using the
    /// squarified rule so that the rectangles stay close to square. The values must be positive.
    pub fn treemap(values: &[f64], area: Rect) -> Vec<Rect> {
        let mut order: Vec<usize> = (0..values.len()).collect();
        order.sort_by(|a, b| values[*b].partial_cmp(&values[*a]).unwrap_or(std::cmp::Ordering::Equal));
        let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
        let mut out = vec![Rect::NOTHING; values.len()];
        if total <= 0.0 {
            return out;
        }
        let scale = f64::from(area.width() * area.height()) / total;
        let mut free = area;
        let mut row: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < order.len() {
            let short = free.width().min(free.height()).max(1.0);
            let mut candidate = row.clone();
            candidate.push(order[i]);
            if row.is_empty() || worst(values, &candidate, scale, short) <= worst(values, &row, scale, short) {
                row = candidate;
                i += 1;
            } else {
                free = place_row(values, &row, scale, free, &mut out);
                row.clear();
            }
        }
        if !row.is_empty() {
            place_row(values, &row, scale, free, &mut out);
        }
        out
    }

    fn worst(values: &[f64], row: &[usize], scale: f64, short: f32) -> f64 {
        let sum: f64 = row.iter().map(|i| values[*i].max(0.0) * scale).sum();
        if sum <= 0.0 {
            return f64::MAX;
        }
        let side = f64::from(short);
        row.iter()
            .map(|i| {
                let a = values[*i].max(0.0) * scale;
                (side * side * a / (sum * sum)).max(sum * sum / (side * side * a.max(f64::MIN_POSITIVE)))
            })
            .fold(0.0, f64::max)
    }

    fn place_row(values: &[f64], row: &[usize], scale: f64, free: Rect, out: &mut [Rect]) -> Rect {
        let sum: f64 = row.iter().map(|i| values[*i].max(0.0) * scale).sum();
        let horizontal = free.width() >= free.height();
        let thickness = if horizontal { (sum / f64::from(free.height())) as f32 } else { (sum / f64::from(free.width())) as f32 };
        let mut at = if horizontal { free.top() } else { free.left() };
        for i in row {
            let share = (values[*i].max(0.0) * scale / sum.max(f64::MIN_POSITIVE)) as f32;
            let len = share * if horizontal { free.height() } else { free.width() };
            out[*i] = if horizontal {
                Rect::from_min_size(Pos2::new(free.left(), at), egui::vec2(thickness, len))
            } else {
                Rect::from_min_size(Pos2::new(at, free.top()), egui::vec2(len, thickness))
            };
            at += len;
        }
        if horizontal {
            Rect::from_min_max(Pos2::new(free.left() + thickness, free.top()), free.max)
        } else {
            Rect::from_min_max(Pos2::new(free.left(), free.top() + thickness), free.max)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn clustered_bars_fill_the_band_less_the_gap() {
            // Two series, gap 100%, no overlap: four bar widths in all, one of them the gap.
            let (offset0, width) = bar_slot(90.0, 0, 2, 100.0, 0.0);
            let (offset1, _) = bar_slot(90.0, 1, 2, 100.0, 0.0);
            assert!((width - 30.0).abs() < 1e-4);
            assert!((offset0 - 15.0).abs() < 1e-4 && (offset1 - 45.0).abs() < 1e-4);
        }

        #[test]
        fn fully_overlapped_bars_share_one_slot() {
            let (a, wa) = bar_slot(100.0, 0, 3, 50.0, 100.0);
            let (b, wb) = bar_slot(100.0, 2, 3, 50.0, 100.0);
            assert_eq!((a, wa), (b, wb));
            assert!((wa - 100.0 / 1.5).abs() < 1e-3);
        }

        #[test]
        fn a_value_lands_between_the_ends_of_its_scale() {
            assert_eq!(along(5.0, 0.0, 10.0, false, false), 0.5);
            assert_eq!(along(5.0, 0.0, 10.0, false, true), 0.5);
            assert_eq!(along(2.0, 0.0, 10.0, false, true), 0.8);
            assert!((along(100.0, 1.0, 10_000.0, true, false) - 0.5).abs() < 1e-6);
        }

        #[test]
        fn a_column_stands_on_the_bottom_of_the_plot() {
            let plot = Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(110.0, 110.0));
            let r = column(plot, 20.0, 10.0, 0.0, 0.5);
            assert_eq!((r.left(), r.right(), r.top(), r.bottom()), (30.0, 40.0, 60.0, 110.0));
            let r = bar(plot, 20.0, 10.0, 0.0, 0.25);
            assert_eq!((r.left(), r.right(), r.top(), r.bottom()), (10.0, 35.0, 30.0, 40.0));
        }

        #[test]
        fn pie_slices_cover_the_circle_in_proportion() {
            let spans = slices(&[1.0, 1.0, 2.0], 0.0);
            assert!((spans[2].1 - std::f64::consts::TAU).abs() < 1e-9);
            assert!((spans[2].1 - spans[2].0 - std::f64::consts::PI).abs() < 1e-9);
            let p = on_circle(Pos2::new(0.0, 0.0), 10.0, 0.0);
            assert!((p.x).abs() < 1e-4 && (p.y + 10.0).abs() < 1e-4);
        }

        #[test]
        fn a_treemap_gives_each_value_its_share_of_the_area() {
            let area = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
            let rects = treemap(&[6.0, 3.0, 1.0], area);
            let total = area.width() * area.height();
            for (r, share) in rects.iter().zip([0.6, 0.3, 0.1]) {
                assert!((r.width() * r.height() / total - share).abs() < 1e-3, "{rects:?}");
            }
        }
    }
}

use geometry as geo;

/// Draw a chart.
pub fn draw(painter: &egui::Painter, area: Rect, plot: &Plot, chart: &Chart, colours: &Colours, zoom: f32) {
    let m = Metrics { zoom };
    let spec = &chart.spec;
    let mut top = area.top();
    if !plot.title.is_empty() || (spec.title_auto && !plot.series.is_empty()) {
        // An empty title element shows the series name for one series and a placeholder otherwise.
        let title = match (plot.title.is_empty(), plot.series.len()) {
            (false, _) => plot.title.clone(),
            (true, 1) => plot.series[0].name.clone(),
            (true, _) => "Chart Title".to_owned(),
        };
        painter.text(Pos2::new(area.center().x, top + 2.0), egui::Align2::CENTER_TOP, title, m.title(), colours.text);
        top += 24.0 * zoom;
    }
    let mut body = Rect::from_min_max(Pos2::new(area.left(), top), area.max);
    if let Some(legend) = &spec.legend {
        body = draw_legend(painter, body, legend, plot, chart, colours, &m);
    }
    if let Some(ex) = &plot.extended {
        draw_extended(painter, body, ex, chart, colours, &m);
        return;
    }
    if plot.series.is_empty() {
        return;
    }
    match spec.groups.first().map(|g| g.kind) {
        Some(GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie) => draw_pies(painter, body, plot, chart, colours, &m),
        Some(GroupKind::Radar) => draw_radar(painter, body, plot, chart, colours, &m),
        Some(GroupKind::Surface) => draw_surface(painter, body, plot, chart, colours, &m),
        _ => draw_cartesian(painter, body, plot, chart, colours, &m),
    }
}

// --- Legend -------------------------------------------------------------------------------------------

/// What the legend lists: series names, or for a pie the categories.
fn legend_entries(plot: &Plot, chart: &Chart, colours: &Colours) -> Vec<(String, Color32)> {
    let pie = chart.spec.groups.first().is_some_and(|g| matches!(g.kind, GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie));
    if pie {
        return plot.series.first().map_or_else(Vec::new, |s| {
            s.labels.iter().enumerate().map(|(i, l)| (l.clone(), point_colour(chart, s, i, colours))).collect()
        });
    }
    let mut out: Vec<(String, Color32)> = plot.series.iter().map(|s| (s.name.clone(), series_colour(chart, s, colours))).collect();
    for s in &plot.series {
        for t in &s.trendlines {
            out.push((t.name.clone(), series_colour(chart, s, colours)));
        }
    }
    out
}

fn draw_legend(
    painter: &egui::Painter,
    body: Rect,
    legend: &Legend,
    plot: &Plot,
    chart: &Chart,
    colours: &Colours,
    m: &Metrics,
) -> Rect {
    let entries: Vec<(String, Color32)> = legend_entries(plot, chart, colours)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !legend.deleted_entries.contains(&(*i as u32)))
        .map(|(_, e)| e)
        .collect();
    if entries.is_empty() {
        return body;
    }
    let font = m.label();
    let widths: Vec<f32> = entries.iter().map(|(n, _)| painter.layout_no_wrap(n.clone(), font.clone(), colours.text).size().x).collect();
    let key = 9.0 * m.zoom;
    let row = 14.0 * m.zoom;
    let vertical = matches!(legend.position, LegendPosition::Left | LegendPosition::Right | LegendPosition::TopRight);
    let draw_entry = |at: Pos2, name: &str, colour: Color32| {
        painter.rect_filled(Rect::from_min_size(Pos2::new(at.x, at.y + 2.0), Vec2::splat(key)), 1, colour);
        painter.text(Pos2::new(at.x + key + 4.0, at.y), egui::Align2::LEFT_TOP, name, font.clone(), colours.text);
    };
    if vertical {
        let width = widths.iter().copied().fold(0.0, f32::max) + key + 12.0;
        let (x, rest) = match legend.position {
            LegendPosition::Left => (body.left(), Rect::from_min_max(Pos2::new(body.left() + width, body.top()), body.max)),
            _ => (body.right() - width, Rect::from_min_max(body.min, Pos2::new(body.right() - width, body.bottom()))),
        };
        let y0 = body.center().y - row * entries.len() as f32 / 2.0;
        for (i, (name, colour)) in entries.iter().enumerate() {
            draw_entry(Pos2::new(x, y0 + row * i as f32), name, *colour);
        }
        return if legend.overlay { body } else { rest };
    }
    let total: f32 = widths.iter().map(|w| w + key + 16.0).sum();
    let mut x = body.center().x - total.min(body.width()) / 2.0;
    let y = if legend.position == LegendPosition::Top { body.top() + 2.0 } else { body.bottom() - row };
    for ((name, colour), w) in entries.iter().zip(&widths) {
        if x + w + key > body.right() {
            break;
        }
        draw_entry(Pos2::new(x, y), name, *colour);
        x += w + key + 16.0;
    }
    if legend.overlay {
        body
    } else if legend.position == LegendPosition::Top {
        Rect::from_min_max(Pos2::new(body.left(), body.top() + row + 4.0), body.max)
    } else {
        Rect::from_min_max(body.min, Pos2::new(body.right(), body.bottom() - row - 4.0))
    }
}

// --- Colours of series and points ---------------------------------------------------------------------

fn series_colour(chart: &Chart, s: &PlotSeries, colours: &Colours) -> Color32 {
    let fmt = &chart.series[s.index].fmt;
    fill_of(&fmt.sp_pr, &colours.accents)
        .or_else(|| line_colour(&fmt.sp_pr, &colours.accents))
        .unwrap_or_else(|| colours.series(s.index))
}

/// The colour of a line in an `spPr`.
fn line_colour(sp_pr: &Option<String>, accents: &[Color32; 6]) -> Option<Color32> {
    let xml = sp_pr.as_deref()?;
    let at = xml.find("<a:ln")?;
    let inner = &xml[at..];
    let end = inner.find("</a:ln>")?;
    fill_of(&Some(format!("<c:spPr>{}</c:spPr>", &inner[inner.find('>')? + 1..end])), accents)
}

fn point_colour(chart: &Chart, s: &PlotSeries, point: usize, colours: &Colours) -> Color32 {
    let fmt = &chart.series[s.index].fmt;
    fmt.points
        .iter()
        .find(|p| p.index as usize == point)
        .and_then(|p| fill_of(&p.sp_pr, &colours.accents))
        .unwrap_or_else(|| colours.series(point))
}

// --- Cartesian charts ---------------------------------------------------------------------------------

/// The room the axes' labels and titles take around the plot.
fn axis_room(plot: &Plot, chart: &Chart, m: &Metrics, horizontal_bars: bool) -> (f32, f32, f32, f32) {
    let (mut left, mut bottom, mut right, mut top) = (8.0 * m.zoom, 8.0 * m.zoom, 8.0 * m.zoom, 6.0 * m.zoom);
    let char_w = 5.5 * m.zoom;
    for (axis, p) in chart.spec.axes.iter().zip(&plot.axes) {
        if axis.deleted {
            continue;
        }
        let labels_width = if p.kind == AxisKind::Value {
            p.ticks.iter().map(|t| t.1.chars().count()).max().unwrap_or(2) as f32 * char_w + 6.0
        } else if horizontal_bars {
            p.categories.iter().map(|c| c.chars().count().min(12)).max().unwrap_or(2) as f32 * char_w + 6.0
        } else {
            0.0
        };
        let title = if axis.title.is_some() { 16.0 * m.zoom } else { 0.0 };
        let label_h = if axis.tick_labels == TickLabels::Hidden { 0.0 } else { 14.0 * m.zoom };
        match axis.position {
            AxisPosition::Left => left = left.max(8.0 * m.zoom + labels_width * f32::from(axis.tick_labels != TickLabels::Hidden) + title),
            AxisPosition::Right => right = right.max(8.0 * m.zoom + labels_width * f32::from(axis.tick_labels != TickLabels::Hidden) + title),
            AxisPosition::Bottom => bottom = bottom.max(label_h + title + 4.0),
            AxisPosition::Top => top = top.max(label_h + title + 4.0),
        }
    }
    if chart.spec.data_table.is_some() {
        bottom += (plot.series.len() as f32 + 1.0) * 14.0 * m.zoom;
    }
    (left, bottom, right, top)
}

/// The place of the plot area and the maps from values to the screen for a pair of axes.
struct Frame2d {
    inner: Rect,
}

fn draw_cartesian(painter: &egui::Painter, body: Rect, plot: &Plot, chart: &Chart, colours: &Colours, m: &Metrics) {
    let spec = &chart.spec;
    let first = spec.groups.first().cloned().unwrap_or_default();
    let horizontal_bars = first.kind == GroupKind::Bar && first.horizontal;
    let (left, bottom, right, top) = axis_room(plot, chart, m, horizontal_bars);
    let inner = Rect::from_min_max(
        Pos2::new(body.left() + left, body.top() + top),
        Pos2::new(body.right() - right, body.bottom() - bottom),
    );
    if inner.width() < 20.0 || inner.height() < 20.0 {
        return;
    }
    let frame = Frame2d { inner };
    draw_axes(painter, &frame, plot, chart, colours, m, horizontal_bars);
    // Areas behind bars behind lines, each group in the order the chart lists it.
    let rank = |g: &Group| match g.kind {
        GroupKind::Area => 0,
        GroupKind::Bar => 1,
        _ => 2,
    };
    let mut order: Vec<usize> = (0..spec.groups.len()).collect();
    order.sort_by_key(|g| rank(&spec.groups[*g]));
    for g in order {
        draw_group(painter, &frame, plot, chart, g, colours, m);
    }
    if let Some(table) = &spec.data_table {
        draw_data_table(painter, body, inner, plot, chart, table, colours, m);
    }
}

/// The axis a group is drawn against: its category (or X) axis and its value (or Y) axis.
fn group_axes<'a>(plot: &'a Plot, chart: &Chart, group: &Group) -> (Option<&'a PlotAxis>, Option<&'a PlotAxis>) {
    let find = |id: Option<&u32>| id.and_then(|id| plot.axes.iter().find(|a| a.id == *id));
    let _ = chart;
    (find(group.axes.first()), find(group.axes.get(1)))
}

fn draw_axes(painter: &egui::Painter, f: &Frame2d, plot: &Plot, chart: &Chart, colours: &Colours, m: &Metrics, horizontal_bars: bool) {
    let inner = f.inner;
    painter.rect_filled(inner, 0, colours.paper);
    for (axis, p) in chart.spec.axes.iter().zip(&plot.axes) {
        let vertical = matches!(axis.position, AxisPosition::Left | AxisPosition::Right);
        if let (Some(scale), true) = (p.scale, p.kind == AxisKind::Value) {
            if axis.deleted {
                continue;
            }
            for (v, text) in &p.ticks {
                let t = geo::along(*v, scale.min, scale.max, scale.log, p.reverse);
                let (a, b) = if vertical {
                    let y = inner.bottom() - inner.height() * t;
                    (Pos2::new(inner.left(), y), Pos2::new(inner.right(), y))
                } else {
                    let x = inner.left() + inner.width() * t;
                    (Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom()))
                };
                if axis.major_gridlines.is_some() {
                    painter.line_segment([a, b], Stroke::new(m.line(), colours.grid));
                }
                if axis.tick_labels != TickLabels::Hidden {
                    let (pos, anchor) = if vertical {
                        match axis.position {
                            AxisPosition::Left => (Pos2::new(inner.left() - 5.0, a.y), egui::Align2::RIGHT_CENTER),
                            _ => (Pos2::new(inner.right() + 5.0, a.y), egui::Align2::LEFT_CENTER),
                        }
                    } else {
                        (Pos2::new(a.x, inner.bottom() + 3.0), egui::Align2::CENTER_TOP)
                    };
                    painter.text(pos, anchor, text, m.label(), colours.dim);
                }
            }
            if axis.minor_gridlines.is_some() {
                let minor = scale.minor.max(f64::MIN_POSITIVE);
                let mut v = scale.min;
                let mut guard = 0;
                while v <= scale.max && guard < 500 && !scale.log {
                    let t = geo::along(v, scale.min, scale.max, false, p.reverse);
                    let (a, b) = if vertical {
                        let y = inner.bottom() - inner.height() * t;
                        (Pos2::new(inner.left(), y), Pos2::new(inner.right(), y))
                    } else {
                        let x = inner.left() + inner.width() * t;
                        (Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom()))
                    };
                    painter.line_segment([a, b], Stroke::new(0.5 * m.line(), colours.grid));
                    v += minor;
                    guard += 1;
                }
            }
        } else if !axis.deleted && p.kind != AxisKind::Value && axis.tick_labels != TickLabels::Hidden {
            let count = p.categories.len().max(1);
            let length = if vertical { inner.height() } else { inner.width() };
            let band = length / count as f32;
            let step = ((14.0 * m.zoom * if vertical { 1.0 } else { 3.0 }) / band).ceil().max(1.0) as usize;
            for (i, c) in p.categories.iter().enumerate().step_by(step) {
                let along = geo::band_centre(i, count, length);
                let text = unluminous_sheet_trim(c, if vertical { 12 } else { 10 });
                if vertical {
                    let y = if p.reverse { inner.top() + along } else { inner.bottom() - along };
                    painter.text(Pos2::new(inner.left() - 5.0, y), egui::Align2::RIGHT_CENTER, text, m.label(), colours.dim);
                } else {
                    let x = if p.reverse { inner.right() - along } else { inner.left() + along };
                    painter.text(Pos2::new(x, inner.bottom() + 3.0), egui::Align2::CENTER_TOP, text, m.label(), colours.dim);
                }
            }
        }
        if let Some(title) = &axis.title {
            draw_axis_title(painter, inner, axis, &title.text, colours, m);
        }
    }
    // The axis lines, where each axis is crossed.
    let line = Stroke::new(m.line(), colours.dim);
    for (axis, p) in chart.spec.axes.iter().zip(&plot.axes) {
        if axis.deleted {
            continue;
        }
        let vertical = matches!(axis.position, AxisPosition::Left | AxisPosition::Right);
        let other = plot.axes.iter().find(|o| o.id == axis.cross_axis);
        let at = other.and_then(|o| o.scale.map(|s| (s, o.cross_at, o.reverse)));
        let _ = (p, at);
        let pos = axis_line_position(inner, axis, plot);
        if vertical {
            painter.line_segment([Pos2::new(pos, inner.top()), Pos2::new(pos, inner.bottom())], line);
        } else {
            painter.line_segment([Pos2::new(inner.left(), pos), Pos2::new(inner.right(), pos)], line);
        }
    }
}

/// The screen position of an axis' line: where the axis it crosses says it should be.
fn axis_line_position(inner: Rect, axis: &Axis, plot: &Plot) -> f32 {
    let vertical = matches!(axis.position, AxisPosition::Left | AxisPosition::Right);
    let other = plot.axes.iter().find(|o| o.id == axis.cross_axis);
    let own = plot.axes.iter().find(|o| o.id == axis.id);
    let fallback = match axis.position {
        AxisPosition::Left => inner.left(),
        AxisPosition::Right => inner.right(),
        AxisPosition::Top => inner.top(),
        AxisPosition::Bottom => inner.bottom(),
    };
    let (Some(other), Some(own)) = (other, own) else { return fallback };
    let Some(scale) = other.scale else {
        return if axis.crosses == Crosses::Maximum {
            if vertical { inner.right() } else { inner.top() }
        } else {
            fallback
        };
    };
    let value = match axis.crosses {
        Crosses::Automatic => own.cross_at.clamp(scale.min, scale.max),
        Crosses::Minimum => scale.min,
        Crosses::Maximum => scale.max,
        Crosses::At(v) => v,
    };
    let t = geo::along(value, scale.min, scale.max, scale.log, other.reverse);
    if vertical {
        inner.left() + inner.width() * t
    } else {
        inner.bottom() - inner.height() * t
    }
}

fn unluminous_sheet_trim(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}\u{2026}")
    }
}

fn draw_axis_title(painter: &egui::Painter, inner: Rect, axis: &Axis, text: &str, colours: &Colours, m: &Metrics) {
    let font = m.font(10.0);
    match axis.position {
        AxisPosition::Left | AxisPosition::Right => {
            let galley = painter.layout_no_wrap(text.to_owned(), font, colours.text);
            let size = galley.size();
            let x = if axis.position == AxisPosition::Left { inner.left() - inner.left().min(inner.left() - 0.0) } else { 0.0 };
            let _ = x;
            let centre_x = if axis.position == AxisPosition::Left { inner.left() - 38.0 * m.zoom } else { inner.right() + 38.0 * m.zoom };
            let pos = Pos2::new(centre_x - size.y / 2.0, inner.center().y + size.x / 2.0);
            painter.add(Shape::Text(TextShape::new(pos, galley, colours.text).with_angle(-std::f32::consts::FRAC_PI_2)));
        }
        AxisPosition::Bottom => {
            painter.text(Pos2::new(inner.center().x, inner.bottom() + 20.0 * m.zoom), egui::Align2::CENTER_TOP, text, font, colours.text);
        }
        AxisPosition::Top => {
            painter.text(Pos2::new(inner.center().x, inner.top() - 18.0 * m.zoom), egui::Align2::CENTER_BOTTOM, text, font, colours.text);
        }
    }
}

/// Maps a group's values to screen points.
struct Mapper {
    inner: Rect,
    horizontal: bool,
    cat_count: usize,
    /// The category axis runs from the far end.
    cat_reverse: bool,
    value: Option<(f64, f64, bool, bool)>,
    x_scale: Option<(f64, f64, bool, bool)>,
}

impl Mapper {
    fn new(f: &Frame2d, plot: &Plot, chart: &Chart, group: &Group) -> Self {
        let (cat, val) = group_axes(plot, chart, group);
        let scale = |a: Option<&PlotAxis>| a.and_then(|a| a.scale.map(|s| (s.min, s.max, s.log, a.reverse)));
        let cat_count = cat.map_or(0, |c| c.categories.len()).max(plot.series.iter().filter(|s| s.group == chart.spec.groups.iter().position(|g| g == group).unwrap_or(0)).map(|s| s.values.len()).max().unwrap_or(0));
        Mapper {
            inner: f.inner,
            horizontal: group.kind == GroupKind::Bar && group.horizontal,
            cat_count,
            cat_reverse: cat.is_some_and(|c| c.reverse),
            value: scale(val),
            x_scale: if matches!(group.kind, GroupKind::Scatter | GroupKind::Bubble) { scale(cat) } else { None },
        }
    }

    /// The fraction along the value axis.
    fn v(&self, value: f64) -> f32 {
        let (min, max, log, rev) = self.value.unwrap_or((0.0, 1.0, false, false));
        geo::along(value, min, max, log, rev)
    }

    /// A point of a scatter or line chart: X is a value for scatter, a category position otherwise.
    fn point(&self, x: f64, y: f64, index: usize) -> Pos2 {
        let fx = match self.x_scale {
            Some((min, max, log, rev)) => geo::along(x, min, max, log, rev),
            None => self.band_fraction(index),
        };
        let fy = self.v(y);
        if self.horizontal {
            Pos2::new(self.inner.left() + self.inner.width() * fy, self.inner.bottom() - self.inner.height() * fx)
        } else {
            Pos2::new(self.inner.left() + self.inner.width() * fx, self.inner.bottom() - self.inner.height() * fy)
        }
    }

    /// How far along the category axis the middle of a category is, from the bottom or the left.
    fn band_fraction(&self, index: usize) -> f32 {
        let f = (index as f32 + 0.5) / self.cat_count.max(1) as f32;
        if self.cat_reverse {
            1.0 - f
        } else {
            f
        }
    }

    /// The place of a category's band along the axis, counted from the left of a column chart or the top of
    /// a bar chart: bar charts list their first category at the bottom.
    fn slot(&self, index: usize) -> usize {
        if self.cat_reverse != self.horizontal {
            self.cat_count.saturating_sub(1 + index)
        } else {
            index
        }
    }
}

fn draw_group(painter: &egui::Painter, f: &Frame2d, plot: &Plot, chart: &Chart, g: usize, colours: &Colours, m: &Metrics) {
    let group = &chart.spec.groups[g];
    let map = Mapper::new(f, plot, chart, group);
    let members: Vec<&PlotSeries> = plot.series.iter().filter(|s| s.group == g).collect();
    if members.is_empty() {
        return;
    }
    match group.kind {
        GroupKind::Bar => draw_bars(painter, f, &map, &members, group, chart, colours, m),
        GroupKind::Area => draw_areas(painter, &map, &members, group, chart, colours, m),
        GroupKind::Line => draw_lines(painter, &map, &members, group, chart, colours, m),
        GroupKind::Scatter => draw_scatter(painter, &map, &members, group, chart, colours, m),
        GroupKind::Bubble => draw_bubbles(painter, &map, &members, chart, colours, m),
        GroupKind::Stock => draw_stock(painter, &map, &members, group, chart, colours, m),
        _ => {}
    }
    for s in &members {
        draw_overlays(painter, &map, s, group, chart, colours, m);
    }
}

fn draw_bars(painter: &egui::Painter, f: &Frame2d, map: &Mapper, members: &[&PlotSeries], group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    let stacked = matches!(group.grouping, Grouping::Stacked | Grouping::PercentStacked);
    let gap = group.gap_width.unwrap_or(150) as f32;
    let overlap = if stacked { 100.0 } else { group.overlap.unwrap_or(0) as f32 };
    let length = if map.horizontal { f.inner.height() } else { f.inner.width() };
    let band = length / map.cat_count.max(1) as f32;
    let depth = if group.three_d { 7.0 * m.zoom } else { 0.0 };
    let zero = map.v(0.0f64.clamp(map.value.map_or(0.0, |v| v.0), map.value.map_or(1.0, |v| v.1)));
    for (si, s) in members.iter().enumerate() {
        let colour = series_colour(chart, s, colours);
        let fmt = &chart.series[s.index].fmt;
        let (offset, width) = geo::bar_slot(band, if stacked { 0 } else { si }, if stacked { 1 } else { members.len() }, gap, overlap);
        for (i, v) in s.values.iter().enumerate() {
            let Some(v) = v else { continue };
            let (a, b) = if stacked { (map.v(s.base[i]), map.v(s.top[i].unwrap_or(0.0))) } else { (zero, map.v(*v)) };
            let slot = map.slot(i) as f32 * band + offset;
            let rect = if map.horizontal { geo::bar(f.inner, slot, width, a, b) } else { geo::column(f.inner, slot, width, a, b) };
            let fill = fmt
                .points
                .iter()
                .find(|p| p.index as usize == i)
                .and_then(|p| fill_of(&p.sp_pr, &colours.accents))
                .unwrap_or(colour);
            if !no_fill(&fmt.sp_pr) {
                painter.rect_filled(rect, 0, fill);
            }
            if depth > 0.0 {
                draw_depth(painter, rect, depth, fill);
            }
        }
    }
}

/// The right and top faces of a 3D bar, darker than its front.
fn draw_depth(painter: &egui::Painter, front: Rect, depth: f32, colour: Color32) {
    let (dx, dy) = (depth, -depth * 0.6);
    painter.add(Shape::convex_polygon(
        vec![front.right_top(), front.right_top() + Vec2::new(dx, dy), front.right_bottom() + Vec2::new(dx, dy), front.right_bottom()],
        shade(colour, 0.7),
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![front.left_top(), front.left_top() + Vec2::new(dx, dy), front.right_top() + Vec2::new(dx, dy), front.right_top()],
        lighten(colour, 0.25),
        Stroke::NONE,
    ));
}

fn draw_areas(painter: &egui::Painter, map: &Mapper, members: &[&PlotSeries], group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    let stacked = matches!(group.grouping, Grouping::Stacked | Grouping::PercentStacked);
    let zero = map.v(0.0f64.clamp(map.value.map_or(0.0, |v| v.0), map.value.map_or(1.0, |v| v.1)));
    let _ = m;
    for s in members {
        let colour = series_colour(chart, s, colours);
        let top: Vec<Pos2> = s.top.iter().enumerate().filter_map(|(i, v)| v.map(|v| map.point(s.x[i], v, i))).collect();
        let base: Vec<Pos2> = (0..s.top.len())
            .filter(|i| s.top[*i].is_some())
            .map(|i| map.point(s.x[i], if stacked { s.base[i] } else { 0.0 }, i))
            .collect();
        let _ = zero;
        for i in 1..top.len() {
            let quad = vec![top[i - 1], top[i], base[i], base[i - 1]];
            painter.add(Shape::convex_polygon(quad, colour.gamma_multiply(0.85), Stroke::NONE));
        }
        if top.len() > 1 {
            painter.add(Shape::line(top, Stroke::new(1.5 * m.zoom.max(0.7), colour)));
        }
    }
}

fn draw_lines(painter: &egui::Painter, map: &Mapper, members: &[&PlotSeries], group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    for s in members {
        let colour = series_colour(chart, s, colours);
        let fmt = &chart.series[s.index].fmt;
        let stacked = matches!(group.grouping, Grouping::Stacked | Grouping::PercentStacked);
        let points: Vec<Option<Pos2>> = (0..s.values.len())
            .map(|i| {
                let y = if stacked { s.top[i] } else { s.values[i] };
                y.map(|y| map.point(s.x[i], y, i))
            })
            .collect();
        let smooth = fmt.smooth.unwrap_or(false);
        draw_polyline(painter, &points, Stroke::new(2.25 * m.zoom.max(0.7), colour), smooth, matches!(chart.spec.blanks_as(), Blanks::Span));
        let marks = fmt.marker.as_ref().and_then(|mk| mk.symbol.as_deref()) != Some("none") && group.marker != Some(false);
        if marks {
            let size = fmt.marker.as_ref().and_then(|mk| mk.size).unwrap_or(5) as f32 * m.zoom;
            for p in points.iter().flatten() {
                draw_marker(painter, *p, fmt.marker.as_ref().and_then(|mk| mk.symbol.as_deref()), size, colour);
            }
        }
    }
    if group.hi_low_lines || group.drop_lines {
        let all: Vec<&&PlotSeries> = members.iter().collect();
        let n = all.iter().map(|s| s.values.len()).max().unwrap_or(0);
        for i in 0..n {
            let ys: Vec<f64> = all.iter().filter_map(|s| s.values.get(i).copied().flatten()).collect();
            if let (Some(lo), Some(hi)) = (ys.iter().copied().reduce(f64::min), ys.iter().copied().reduce(f64::max)) {
                let x = members[0].x.get(i).copied().unwrap_or(0.0);
                let a = map.point(x, if group.drop_lines { map.value.map_or(0.0, |v| v.0) } else { lo }, i);
                let b = map.point(x, hi, i);
                painter.line_segment([a, b], Stroke::new(m.line(), colours.dim));
            }
        }
    }
}

/// Join points with a line, breaking it at gaps unless the chart spans them; smooth lines are rounded
/// with a Catmull-Rom curve through the points.
fn draw_polyline(painter: &egui::Painter, points: &[Option<Pos2>], stroke: Stroke, smooth: bool, span: bool) {
    let mut run: Vec<Pos2> = Vec::new();
    let flush = |run: &mut Vec<Pos2>| {
        if run.len() > 1 {
            let path = if smooth { catmull(run) } else { run.clone() };
            painter.add(Shape::line(path, stroke));
        }
        run.clear();
    };
    for p in points {
        match p {
            Some(p) => run.push(*p),
            None if span => {}
            None => flush(&mut run),
        }
    }
    flush(&mut run);
}

/// A smooth path through some points.
pub fn catmull(points: &[Pos2]) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];
        for s in 0..8 {
            let t = s as f32 / 8.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push(Pos2::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
        }
    }
    out.push(points[points.len() - 1]);
    out
}

fn draw_marker(painter: &egui::Painter, at: Pos2, symbol: Option<&str>, size: f32, colour: Color32) {
    let r = size / 2.0;
    match symbol.unwrap_or("circle") {
        "square" => {
            painter.rect_filled(Rect::from_center_size(at, Vec2::splat(size)), 0, colour);
        }
        "diamond" => {
            painter.add(Shape::convex_polygon(
                vec![at + Vec2::new(0.0, -r), at + Vec2::new(r, 0.0), at + Vec2::new(0.0, r), at + Vec2::new(-r, 0.0)],
                colour,
                Stroke::NONE,
            ));
        }
        "triangle" => {
            painter.add(Shape::convex_polygon(
                vec![at + Vec2::new(0.0, -r), at + Vec2::new(r, r), at + Vec2::new(-r, r)],
                colour,
                Stroke::NONE,
            ));
        }
        "x" | "plus" | "star" => {
            let s = Stroke::new(1.5, colour);
            if symbol == Some("x") || symbol == Some("star") {
                painter.line_segment([at + Vec2::new(-r, -r), at + Vec2::new(r, r)], s);
                painter.line_segment([at + Vec2::new(-r, r), at + Vec2::new(r, -r)], s);
            }
            if symbol == Some("plus") || symbol == Some("star") {
                painter.line_segment([at + Vec2::new(-r, 0.0), at + Vec2::new(r, 0.0)], s);
                painter.line_segment([at + Vec2::new(0.0, -r), at + Vec2::new(0.0, r)], s);
            }
        }
        "dash" => {
            painter.line_segment([at + Vec2::new(-r, 0.0), at + Vec2::new(r, 0.0)], Stroke::new(2.0, colour));
        }
        "none" => {}
        _ => {
            painter.circle_filled(at, r, colour);
        }
    }
}

fn draw_scatter(painter: &egui::Painter, map: &Mapper, members: &[&PlotSeries], group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    let style = group.scatter_style.as_deref().unwrap_or("lineMarker");
    for s in members {
        let colour = series_colour(chart, s, colours);
        let fmt = &chart.series[s.index].fmt;
        let points: Vec<Option<Pos2>> =
            (0..s.values.len()).map(|i| s.values[i].map(|y| map.point(s.x[i], y, i))).collect();
        let lines = !no_line(&fmt.sp_pr) && style != "marker" && style != "none";
        if lines {
            draw_polyline(painter, &points, Stroke::new(2.0 * m.zoom.max(0.7), colour), fmt.smooth.unwrap_or(style.starts_with("smooth")), false);
        }
        let symbol = fmt.marker.as_ref().and_then(|mk| mk.symbol.as_deref());
        if symbol != Some("none") {
            let size = fmt.marker.as_ref().and_then(|mk| mk.size).unwrap_or(5) as f32 * m.zoom;
            for p in points.iter().flatten() {
                draw_marker(painter, *p, symbol, size.max(4.0), colour);
            }
        }
    }
}

fn draw_bubbles(painter: &egui::Painter, map: &Mapper, members: &[&PlotSeries], chart: &Chart, colours: &Colours, m: &Metrics) {
    let biggest = members.iter().flat_map(|s| s.sizes.iter().flatten().copied()).fold(0.0f64, f64::max).max(f64::MIN_POSITIVE);
    let max_radius = map.inner.width().min(map.inner.height()) / 8.0;
    for s in members {
        let colour = series_colour(chart, s, colours).gamma_multiply(0.75);
        for i in 0..s.values.len() {
            let Some(y) = s.values[i] else { continue };
            let size = s.sizes.get(i).copied().flatten().unwrap_or(1.0).max(0.0);
            let radius = (max_radius * (size / biggest).sqrt() as f32).max(3.0 * m.zoom);
            painter.circle_filled(map.point(s.x[i], y, i), radius, colour);
        }
    }
}

fn draw_stock(painter: &egui::Painter, map: &Mapper, members: &[&PlotSeries], group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    // Open, high, low, close in that order; high, low, close with three series.
    let (open, high, low, close) = match members.len() {
        4 => (Some(members[0]), members[1], members[2], members[3]),
        3 => (None, members[0], members[1], members[2]),
        _ => return,
    };
    let n = high.values.len();
    let band = map.inner.width() / map.cat_count.max(1) as f32;
    let ink = colours.text;
    let _ = chart;
    for i in 0..n {
        let (Some(h), Some(l), Some(c)) = (high.values[i], low.values[i], close.values[i]) else { continue };
        let x = map.point(high.x[i], h, i).x;
        painter.line_segment([Pos2::new(x, map.point(0.0, h, i).y), Pos2::new(x, map.point(0.0, l, i).y)], Stroke::new(m.line() * 1.2, ink));
        match open.and_then(|o| o.values[i]) {
            Some(o) if group.up_down_bars.is_some() => {
                let (top, bottom) = (map.point(0.0, o.max(c), i).y, map.point(0.0, o.min(c), i).y);
                let rect = Rect::from_min_max(Pos2::new(x - band * 0.25, top), Pos2::new(x + band * 0.25, bottom.max(top + 1.0)));
                let fill = if c >= o { colours.paper } else { colours.text };
                painter.rect(rect, 0, fill, Stroke::new(m.line(), ink), egui::StrokeKind::Inside);
            }
            Some(o) => {
                painter.line_segment([Pos2::new(x - band * 0.2, map.point(0.0, o, i).y), Pos2::new(x, map.point(0.0, o, i).y)], Stroke::new(m.line(), ink));
                painter.line_segment([Pos2::new(x, map.point(0.0, c, i).y), Pos2::new(x + band * 0.2, map.point(0.0, c, i).y)], Stroke::new(m.line(), ink));
            }
            None => {
                painter.line_segment([Pos2::new(x, map.point(0.0, c, i).y), Pos2::new(x + band * 0.2, map.point(0.0, c, i).y)], Stroke::new(m.line(), ink));
            }
        }
    }
}

/// Trendlines, error bars and data labels of a series.
fn draw_overlays(painter: &egui::Painter, map: &Mapper, s: &PlotSeries, group: &Group, chart: &Chart, colours: &Colours, m: &Metrics) {
    let colour = series_colour(chart, s, colours);
    let source = &chart.series[s.index];
    for (t, spec) in s.trendlines.iter().zip(&source.fmt.trendlines) {
        let points: Vec<Pos2> = t
            .samples
            .iter()
            .map(|(x, y)| {
                // Category charts fit against 1, 2, 3, so an X is already a band position.
                if map.x_scale.is_some() {
                    map.point(*x, *y, 0)
                } else {
                    let fx = (*x as f32 - 0.5) / map.cat_count.max(1) as f32;
                    let fy = map.v(*y);
                    Pos2::new(map.inner.left() + map.inner.width() * fx, map.inner.bottom() - map.inner.height() * fy)
                }
            })
            .collect();
        painter.add(Shape::line(points.clone(), Stroke::new(1.5 * m.zoom.max(0.7), colour.gamma_multiply(0.8))));
        if (spec.show_equation || spec.show_r_squared) && !t.equation.is_empty() {
            let mut text = String::new();
            if spec.show_equation {
                text.push_str(&t.equation);
            }
            if spec.show_r_squared {
                if let Some(r2) = t.r_squared {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&unluminous_sheet::plot::equation::r_squared(r2));
                }
            }
            if let Some(last) = points.last() {
                painter.text(Pos2::new(last.x.min(map.inner.right() - 4.0), last.y - 4.0), egui::Align2::RIGHT_BOTTOM, text, m.label(), colours.text);
            }
        }
    }
    for e in &s.error_bars {
        for (i, v) in s.values.iter().enumerate() {
            let Some(v) = v else { continue };
            let plus = e.amounts.plus.get(i).copied().unwrap_or(0.0);
            let minus = e.amounts.minus.get(i).copied().unwrap_or(0.0);
            let x = s.x[i];
            let (a, b) = if e.direction == ErrorDirection::X {
                (map.point(x - minus, *v, i), map.point(x + plus, *v, i))
            } else {
                (map.point(x, v - minus, i), map.point(x, v + plus, i))
            };
            painter.line_segment([a, b], Stroke::new(m.line(), colours.text));
            if !e.no_cap {
                let cap = 3.0 * m.zoom;
                let (da, db) = if e.direction == ErrorDirection::X { (Vec2::new(0.0, cap), Vec2::new(0.0, cap)) } else { (Vec2::new(cap, 0.0), Vec2::new(cap, 0.0)) };
                painter.line_segment([a - da, a + da], Stroke::new(m.line(), colours.text));
                painter.line_segment([b - db, b + db], Stroke::new(m.line(), colours.text));
            }
        }
    }
    let labels = source.fmt.labels.as_ref().or(group.labels.as_ref());
    if let Some(l) = labels.filter(|l| !l.delete && l.fields.shows_anything()) {
        draw_labels(painter, map, s, group, l, colours, m);
    }
}

fn draw_labels(painter: &egui::Painter, map: &Mapper, s: &PlotSeries, group: &Group, l: &DataLabels, colours: &Colours, m: &Metrics) {
    for (i, v) in s.values.iter().enumerate() {
        let Some(v) = v else { continue };
        let point_label = l.points.iter().find(|p| p.index as usize == i);
        if point_label.is_some_and(|p| p.delete) {
            continue;
        }
        let f = point_label.map_or(&l.fields, |p| &p.fields);
        let mut parts = Vec::new();
        if f.show_series {
            parts.push(s.name.clone());
        }
        if f.show_category {
            parts.push(s.labels.get(i).cloned().unwrap_or_default());
        }
        if f.show_value {
            parts.push(unluminous_sheet::numfmt::general(*v, 8));
        }
        let text = parts.join(f.separator.as_deref().unwrap_or(", "));
        if text.is_empty() {
            continue;
        }
        let y = if matches!(group.grouping, Grouping::Stacked | Grouping::PercentStacked) { s.top[i].unwrap_or(*v) } else { *v };
        let at = map.point(s.x[i], y, i);
        let (pos, anchor) = match (f.position, group.kind == GroupKind::Bar) {
            (Some(LabelPosition::Center), _) => (map.point(s.x[i], (s.base[i] + y) / 2.0, i), egui::Align2::CENTER_CENTER),
            (Some(LabelPosition::InsideEnd), true) => (Pos2::new(at.x, at.y + 3.0), egui::Align2::CENTER_TOP),
            (Some(LabelPosition::Below), _) => (Pos2::new(at.x, at.y + 4.0), egui::Align2::CENTER_TOP),
            (Some(LabelPosition::Left), _) => (Pos2::new(at.x - 5.0, at.y), egui::Align2::RIGHT_CENTER),
            (Some(LabelPosition::Right), _) => (Pos2::new(at.x + 5.0, at.y), egui::Align2::LEFT_CENTER),
            _ => (Pos2::new(at.x, at.y - 3.0), egui::Align2::CENTER_BOTTOM),
        };
        painter.text(pos, anchor, text, m.label(), colours.text);
    }
}

fn draw_data_table(painter: &egui::Painter, body: Rect, inner: Rect, plot: &Plot, chart: &Chart, table: &DataTable, colours: &Colours, m: &Metrics) {
    let rows = plot.series.len() + 1;
    let row_h = 14.0 * m.zoom;
    let top = inner.bottom() + 16.0 * m.zoom;
    let cats: Vec<String> = plot.series.first().map(|s| s.labels.clone()).unwrap_or_default();
    let cols = cats.len().max(1);
    let col_w = inner.width() / cols as f32;
    let stroke = Stroke::new(m.line(), colours.grid);
    for r in 0..=rows {
        if table.horizontal_borders || (table.outline && (r == 0 || r == rows)) {
            painter.line_segment([Pos2::new(inner.left(), top + row_h * r as f32), Pos2::new(inner.right(), top + row_h * r as f32)], stroke);
        }
    }
    for c in 0..=cols {
        if table.vertical_borders || (table.outline && (c == 0 || c == cols)) {
            painter.line_segment([Pos2::new(inner.left() + col_w * c as f32, top), Pos2::new(inner.left() + col_w * c as f32, top + row_h * rows as f32)], stroke);
        }
    }
    for (c, label) in cats.iter().enumerate() {
        painter.text(Pos2::new(inner.left() + col_w * (c as f32 + 0.5), top + row_h / 2.0), egui::Align2::CENTER_CENTER, unluminous_sheet_trim(label, 8), m.label(), colours.text);
    }
    for (r, s) in plot.series.iter().enumerate() {
        painter.text(Pos2::new(inner.left() - 4.0, top + row_h * (r as f32 + 1.5)), egui::Align2::RIGHT_CENTER, unluminous_sheet_trim(&s.name, 12), m.label(), colours.text);
        for (c, v) in s.values.iter().enumerate() {
            if let Some(v) = v {
                painter.text(Pos2::new(inner.left() + col_w * (c as f32 + 0.5), top + row_h * (r as f32 + 1.5)), egui::Align2::CENTER_CENTER, unluminous_sheet::numfmt::general(*v, 8), m.label(), colours.text);
            }
        }
    }
    let _ = (body, chart);
}

// --- Pie, doughnut, radar, surface -------------------------------------------------------------------

fn draw_pies(painter: &egui::Painter, body: Rect, plot: &Plot, chart: &Chart, colours: &Colours, m: &Metrics) {
    let group = chart.spec.groups.first().cloned().unwrap_or_default();
    let Some(series) = plot.series.first() else { return };
    let values: Vec<f64> = series.values.iter().map(|v| v.unwrap_or(0.0)).collect();
    let (first_pie, second_pie): (Vec<usize>, Vec<usize>) = if group.kind == GroupKind::OfPie {
        let n = values.len();
        let tail = ((n as f32 / 3.0).ceil() as usize).clamp(1, n.saturating_sub(1).max(1));
        ((0..n - tail).collect(), (n - tail..n).collect())
    } else {
        ((0..values.len()).collect(), Vec::new())
    };
    let three_d = group.three_d;
    let squash = if three_d { 0.62 } else { 1.0 };
    let (main_area, side_area) = if second_pie.is_empty() {
        (body, None)
    } else {
        let w = body.width() * 0.55;
        (Rect::from_min_size(body.min, Vec2::new(w, body.height())), Some(Rect::from_min_max(Pos2::new(body.left() + w, body.top()), body.max)))
    };
    let mut primary = values.clone();
    if !second_pie.is_empty() {
        let other: f64 = second_pie.iter().map(|i| values[*i].max(0.0)).sum();
        primary = first_pie.iter().map(|i| values[*i]).collect();
        primary.push(other);
    }
    let radius = (main_area.width().min(main_area.height() / squash) / 2.0 - 6.0).max(4.0);
    let centre = main_area.center();
    pie(painter, centre, radius, squash, &primary, chart, series, group.first_slice_angle.unwrap_or(0) as f32, group.kind == GroupKind::Doughnut, group.hole_size.unwrap_or(50) as f32 / 100.0, three_d, colours, m, &first_pie, true);
    if let Some(side) = side_area {
        let rest: Vec<f64> = second_pie.iter().map(|i| values[*i]).collect();
        let r2 = (radius * group.second_size.unwrap_or(75) as f32 / 100.0).min(side.width() / 2.0 - 4.0);
        pie(painter, side.center(), r2, squash, &rest, chart, series, 0.0, false, 0.0, three_d, colours, m, &second_pie, false);
    }
}

#[allow(clippy::too_many_arguments)]
fn pie(
    painter: &egui::Painter,
    centre: Pos2,
    radius: f32,
    squash: f32,
    values: &[f64],
    chart: &Chart,
    series: &PlotSeries,
    first: f32,
    doughnut: bool,
    hole: f32,
    three_d: bool,
    colours: &Colours,
    m: &Metrics,
    point_index: &[usize],
    has_other: bool,
) {
    let spans = geo::slices(values, first);
    let fmt = &chart.series[series.index].fmt;
    let explosion = fmt.explosion.unwrap_or(0) as f32 / 100.0;
    let thickness = if three_d { 10.0 * m.zoom } else { 0.0 };
    let place = |p: Pos2| Pos2::new(p.x, centre.y + (p.y - centre.y) * squash);
    for pass in 0..2 {
        for (k, (a0, a1)) in spans.iter().enumerate() {
            let other = has_other && k == spans.len() - 1 && point_index.len() < spans.len();
            let index = point_index.get(k).copied().unwrap_or(k);
            let colour = if other { colours.dim } else { point_colour(chart, series, index, colours) };
            let mid = (a0 + a1) / 2.0;
            let shift = fmt.points.iter().find(|p| p.index as usize == index).and_then(|p| p.explosion).map_or(explosion, |e| e as f32 / 100.0);
            let offset = geo::on_circle(Pos2::ZERO, radius * shift, mid).to_vec2();
            let pieces = (((a1 - a0) / std::f64::consts::FRAC_PI_2).ceil() as usize).max(1);
            for p in 0..pieces {
                let (b0, b1) = (a0 + (a1 - a0) * p as f64 / pieces as f64, a0 + (a1 - a0) * (p + 1) as f64 / pieces as f64);
                let mut ring = Vec::new();
                let inner_r = if doughnut { radius * hole } else { 0.0 };
                for s in 0..=20 {
                    let a = b0 + (b1 - b0) * s as f64 / 20.0;
                    ring.push(place(geo::on_circle(centre, radius, a) + offset));
                }
                if doughnut {
                    for s in (0..=20).rev() {
                        let a = b0 + (b1 - b0) * s as f64 / 20.0;
                        ring.push(place(geo::on_circle(centre, inner_r, a) + offset));
                    }
                } else {
                    ring.push(place(centre + offset));
                }
                if pass == 0 && three_d {
                    let down: Vec<Pos2> = ring.iter().map(|q| *q + Vec2::new(0.0, thickness)).collect();
                    painter.add(Shape::convex_polygon(down, shade(colour, 0.65), Stroke::NONE));
                } else if pass == 1 {
                    painter.add(Shape::convex_polygon(ring, colour, Stroke::new(1.0, colours.paper)));
                }
            }
        }
    }
    if let Some(l) = fmt.labels.as_ref().filter(|l| !l.delete && l.fields.shows_anything()) {
        let total: f64 = values.iter().map(|v| v.max(0.0)).sum();
        for (k, (a0, a1)) in spans.iter().enumerate() {
            let index = point_index.get(k).copied().unwrap_or(k);
            let mut parts = Vec::new();
            if l.fields.show_series {
                parts.push(series.name.clone());
            }
            if l.fields.show_category {
                parts.push(series.labels.get(index).cloned().unwrap_or_default());
            }
            if l.fields.show_value {
                parts.push(unluminous_sheet::numfmt::general(values[k], 8));
            }
            if l.fields.show_percent && total > 0.0 {
                parts.push(format!("{:.0}%", values[k].max(0.0) / total * 100.0));
            }
            if parts.is_empty() {
                continue;
            }
            let mid = (a0 + a1) / 2.0;
            let outside = matches!(l.fields.position, Some(LabelPosition::OutsideEnd | LabelPosition::BestFit));
            let r = if doughnut { radius * (1.0 + hole) / 2.0 } else if outside { radius * 1.12 } else { radius * 0.65 };
            let p = place(geo::on_circle(centre, r, mid));
            painter.text(p, egui::Align2::CENTER_CENTER, parts.join(l.fields.separator.as_deref().unwrap_or(", ")), m.label(), colours.text);
        }
    }
}

fn draw_radar(painter: &egui::Painter, body: Rect, plot: &Plot, chart: &Chart, colours: &Colours, m: &Metrics) {
    let group = chart.spec.groups.first().cloned().unwrap_or_default();
    let count = plot.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    if count < 3 {
        return;
    }
    let centre = body.center();
    let radius = body.width().min(body.height()) / 2.0 - 18.0 * m.zoom;
    let value_axis = plot.axes.iter().find(|a| a.kind == AxisKind::Value);
    let (min, max) = value_axis.and_then(|a| a.scale).map_or((0.0, 1.0), |s| (s.min, s.max));
    let ring = |frac: f32| -> Vec<Pos2> { (0..count).map(|i| geo::on_circle(centre, radius * frac, std::f64::consts::TAU * i as f64 / count as f64)).collect() };
    if let Some(a) = value_axis {
        for (v, _) in &a.ticks {
            let frac = geo::along(*v, min, max, false, false);
            painter.add(Shape::closed_line(ring(frac), Stroke::new(m.line(), colours.grid)));
        }
    }
    for i in 0..count {
        let angle = std::f64::consts::TAU * i as f64 / count as f64;
        painter.line_segment([centre, geo::on_circle(centre, radius, angle)], Stroke::new(m.line(), colours.grid));
        let label = plot.series[0].labels.get(i).cloned().unwrap_or_default();
        let p = geo::on_circle(centre, radius + 8.0 * m.zoom, angle);
        painter.text(p, egui::Align2::CENTER_CENTER, unluminous_sheet_trim(&label, 10), m.label(), colours.dim);
    }
    for s in &plot.series {
        let colour = series_colour(chart, s, colours);
        let pts: Vec<Pos2> = (0..count)
            .map(|i| geo::on_circle(centre, radius * geo::along(s.values.get(i).copied().flatten().unwrap_or(min), min, max, false, false), std::f64::consts::TAU * i as f64 / count as f64))
            .collect();
        if group.radar_style.as_deref() == Some("filled") {
            for i in 0..pts.len() {
                let tri = vec![centre, pts[i], pts[(i + 1) % pts.len()]];
                painter.add(Shape::convex_polygon(tri, colour.gamma_multiply(0.55), Stroke::NONE));
            }
        }
        painter.add(Shape::closed_line(pts.clone(), Stroke::new(2.0 * m.zoom.max(0.7), colour)));
        if group.radar_style.as_deref() == Some("marker") {
            for p in pts {
                painter.circle_filled(p, 3.0 * m.zoom, colour);
            }
        }
    }
}

fn draw_surface(painter: &egui::Painter, body: Rect, plot: &Plot, chart: &Chart, colours: &Colours, m: &Metrics) {
    let group = chart.spec.groups.first().cloned().unwrap_or_default();
    let rows = plot.series.len();
    let cols = plot.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    if rows < 2 || cols < 2 {
        return;
    }
    let value_axis = plot.axes.iter().find(|a| a.kind == AxisKind::Value);
    let (min, max) = value_axis.and_then(|a| a.scale).map_or((0.0, 1.0), |s| (s.min, s.max));
    let bands = value_axis.map_or(5, |a| a.ticks.len().saturating_sub(1)).max(1);
    let colour_of = |v: f64| {
        let t = ((v - min) / (max - min).max(f64::MIN_POSITIVE)).clamp(0.0, 0.999);
        let band = (t * bands as f64) as usize;
        colours.series(band)
    };
    let wire = group.wireframe;
    let three_d = group.three_d;
    let area = body.shrink(12.0 * m.zoom);
    let value = |r: usize, c: usize| plot.series[r].values.get(c).copied().flatten().unwrap_or(min);
    // Top view fills the plot; the 3D view tilts it into an oblique projection.
    let point = |r: usize, c: usize| -> Pos2 {
        let (fx, fy) = (c as f32 / (cols - 1) as f32, r as f32 / (rows - 1) as f32);
        if three_d {
            let lift = geo::along(value(r, c), min, max, false, false) * area.height() * 0.45;
            let x = area.left() + area.width() * 0.1 + fx * area.width() * 0.62 + fy * area.width() * 0.25;
            let y = area.bottom() - area.height() * 0.1 - fy * area.height() * 0.28 - lift;
            Pos2::new(x, y)
        } else {
            Pos2::new(area.left() + area.width() * fx, area.top() + area.height() * (1.0 - fy))
        }
    };
    for r in 0..rows - 1 {
        for c in 0..cols - 1 {
            let quad = vec![point(r, c), point(r, c + 1), point(r + 1, c + 1), point(r + 1, c)];
            let mean = (value(r, c) + value(r, c + 1) + value(r + 1, c) + value(r + 1, c + 1)) / 4.0;
            let fill = if wire { Color32::TRANSPARENT } else { colour_of(mean) };
            let convex = Shape::convex_polygon(quad.clone(), fill, Stroke::new(m.line(), if wire { colours.dim } else { shade(colour_of(mean), 0.6) }));
            painter.add(convex);
        }
    }
}

// --- The newer chart types ----------------------------------------------------------------------------

fn draw_extended(painter: &egui::Painter, body: Rect, ex: &ExtendedPlot, chart: &Chart, colours: &Colours, m: &Metrics) {
    let area = body.shrink(8.0 * m.zoom);
    match ex {
        ExtendedPlot::Histogram { bins } => {
            let counts: Vec<f64> = bins.iter().map(|b| b.count as f64).collect();
            let labels: Vec<String> = bins.iter().map(|b| b.label.clone()).collect();
            draw_columns(painter, area, &labels, &counts, colours.series(0), 0.0, colours, m);
        }
        ExtendedPlot::Pareto { rows } => draw_pareto(painter, area, rows, colours, m),
        ExtendedPlot::BoxWhisker { boxes } => {
            let options = chart.spec.ex.as_ref().and_then(|e| e.series.first()).and_then(|s| s.box_options).unwrap_or_default();
            draw_boxes(painter, area, boxes, &options, colours, m);
        }
        ExtendedPlot::Waterfall { bars } => draw_waterfall(painter, area, bars, colours, m),
        ExtendedPlot::Funnel { items } => draw_funnel(painter, area, items, colours, m),
        ExtendedPlot::Hierarchy { leaves } => draw_hierarchy(painter, area, leaves, chart, colours, m),
        ExtendedPlot::Map { regions } => {
            let words = format!("A map chart of {} regions, which needs Bing Maps to draw", regions.len());
            painter.text(area.center(), egui::Align2::CENTER_CENTER, words, m.font(11.0), colours.dim);
        }
    }
}

/// The columns of a Pareto chart and the line of running shares over them.
fn draw_pareto(painter: &egui::Painter, area: Rect, rows: &[(String, f64, f64)], colours: &Colours, m: &Metrics) {
    let labels: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let values: Vec<f64> = rows.iter().map(|r| r.1).collect();
    let plot = draw_columns(painter, area, &labels, &values, colours.series(0), 0.1, colours, m);
    let share_at = |i: usize, share: f64| {
        Pos2::new(plot.left() + plot.width() * (i as f32 + 0.5) / rows.len().max(1) as f32, plot.bottom() - plot.height() * share as f32)
    };
    let points: Vec<Pos2> = rows.iter().enumerate().map(|(i, r)| share_at(i, r.2)).collect();
    painter.add(Shape::line(points.clone(), Stroke::new(2.0 * m.zoom.max(0.7), colours.series(1))));
    for p in points {
        painter.circle_filled(p, 3.0 * m.zoom, colours.series(1));
    }
    painter.text(Pos2::new(plot.right() + 3.0, plot.top()), egui::Align2::LEFT_TOP, "100%", m.label(), colours.dim);
}

/// Boxes with whiskers, medians, means and outliers.
fn draw_boxes(painter: &egui::Painter, plot: Rect, boxes: &[(String, BoxStats)], options: &BoxOptions, colours: &Colours, m: &Metrics) {
    let ends: Vec<f64> = boxes.iter().flat_map(|b| [b.1.minimum, b.1.maximum]).collect();
    let (lo, hi) = (ends.iter().copied().fold(f64::MAX, f64::min), ends.iter().copied().fold(f64::MIN, f64::max));
    let (lo, hi) = if lo >= hi { (lo - 1.0, hi + 1.0) } else { (lo - (hi - lo) * 0.05, hi + (hi - lo) * 0.05) };
    let y = |v: f64| plot.bottom() - plot.height() * ((v - lo) / (hi - lo)) as f32;
    let band = plot.width() / boxes.len().max(1) as f32;
    let line = Stroke::new(m.line(), colours.text);
    for (i, (label, b)) in boxes.iter().enumerate() {
        let cx = plot.left() + band * (i as f32 + 0.5);
        let half = band * 0.28;
        painter.line_segment([Pos2::new(cx, y(b.low)), Pos2::new(cx, y(b.q1))], line);
        painter.line_segment([Pos2::new(cx, y(b.q3)), Pos2::new(cx, y(b.high))], line);
        for edge in [b.low, b.high] {
            painter.line_segment([Pos2::new(cx - half / 2.0, y(edge)), Pos2::new(cx + half / 2.0, y(edge))], line);
        }
        let body = Rect::from_min_max(Pos2::new(cx - half, y(b.q3)), Pos2::new(cx + half, y(b.q1)));
        painter.rect(body, 0, colours.series(i).gamma_multiply(0.8), line, egui::StrokeKind::Inside);
        painter.line_segment([Pos2::new(cx - half, y(b.median)), Pos2::new(cx + half, y(b.median))], Stroke::new(1.5 * m.zoom.max(0.7), colours.text));
        if options.mean_marker {
            painter.text(Pos2::new(cx, y(b.mean)), egui::Align2::CENTER_CENTER, "\u{D7}", m.font(12.0), colours.text);
        }
        if options.outlier_points {
            for o in &b.outliers {
                painter.circle_stroke(Pos2::new(cx, y(*o)), 3.0 * m.zoom, line);
            }
        }
        painter.text(Pos2::new(cx, plot.bottom() + 3.0), egui::Align2::CENTER_TOP, label, m.label(), colours.dim);
    }
}

/// Floating bars joined by the running total.
fn draw_waterfall(painter: &egui::Painter, area: Rect, bars: &[(String, WaterfallBar)], colours: &Colours, m: &Metrics) {
    let (lo, hi) = bars.iter().fold((0.0f64, 0.0f64), |(l, h), (_, b)| (l.min(b.start).min(b.end), h.max(b.start).max(b.end)));
    let hi = if hi <= lo { lo + 1.0 } else { hi * 1.05 };
    let plot = Rect::from_min_max(area.min, Pos2::new(area.right(), area.bottom() - 14.0 * m.zoom));
    let y = |v: f64| plot.bottom() - plot.height() * ((v - lo) / (hi - lo)) as f32;
    let band = plot.width() / bars.len().max(1) as f32;
    let mut previous: Option<Pos2> = None;
    for (i, (label, b)) in bars.iter().enumerate() {
        let colour = match b.kind {
            WaterfallKind::Increase => colours.series(0),
            WaterfallKind::Decrease => colours.series(1),
            WaterfallKind::Total => colours.series(2),
        };
        let x0 = plot.left() + band * i as f32 + band * 0.15;
        let rect = Rect::from_min_max(Pos2::new(x0, y(b.start.max(b.end))), Pos2::new(x0 + band * 0.7, y(b.start.min(b.end))));
        painter.rect_filled(rect, 0, colour);
        if let Some(p) = previous {
            painter.line_segment([p, Pos2::new(x0, y(b.start))], Stroke::new(m.line(), colours.dim));
        }
        previous = Some(Pos2::new(x0 + band * 0.7, y(b.end)));
        painter.text(Pos2::new(x0 + band * 0.35, plot.bottom() + 2.0), egui::Align2::CENTER_TOP, unluminous_sheet_trim(label, 8), m.label(), colours.dim);
    }
}

/// Centred bars as wide as their values.
fn draw_funnel(painter: &egui::Painter, area: Rect, items: &[(String, f64)], colours: &Colours, m: &Metrics) {
    let biggest = items.iter().map(|i| i.1).fold(f64::MIN_POSITIVE, f64::max);
    let band = area.height() / items.len().max(1) as f32;
    for (i, (label, v)) in items.iter().enumerate() {
        let w = area.width() * 0.8 * (*v / biggest).max(0.0) as f32;
        let rect = Rect::from_center_size(Pos2::new(area.center().x, area.top() + band * (i as f32 + 0.5)), Vec2::new(w, band * 0.85));
        painter.rect_filled(rect, 0, colours.series(0));
        painter.text(Pos2::new(area.left(), rect.center().y), egui::Align2::LEFT_CENTER, unluminous_sheet_trim(label, 12), m.label(), colours.dim);
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, unluminous_sheet::numfmt::general(*v, 8), m.label(), Color32::WHITE);
    }
}

/// Columns with category labels under them; returns the plot rectangle they were drawn in.
#[allow(clippy::too_many_arguments)]
fn draw_columns(painter: &egui::Painter, area: Rect, labels: &[String], values: &[f64], colour: Color32, gap: f32, colours: &Colours, m: &Metrics) -> Rect {
    let plot = Rect::from_min_max(Pos2::new(area.left() + 30.0 * m.zoom, area.top()), Pos2::new(area.right() - 24.0 * m.zoom, area.bottom() - 16.0 * m.zoom));
    let scale = unluminous_sheet::plot::scale::value_scale(0.0, values.iter().copied().fold(0.0, f64::max), unluminous_sheet::plot::Fixed::default(), plot.height() as f64);
    for (v, text) in unluminous_sheet::plot::scale::ticks(&scale).into_iter().map(|v| (v, unluminous_sheet::numfmt::general(v, 8))) {
        let y = plot.bottom() - plot.height() * geo::along(v, scale.min, scale.max, false, false);
        painter.line_segment([Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)], Stroke::new(m.line(), colours.grid));
        painter.text(Pos2::new(plot.left() - 4.0, y), egui::Align2::RIGHT_CENTER, text, m.label(), colours.dim);
    }
    let band = plot.width() / values.len().max(1) as f32;
    for (i, v) in values.iter().enumerate() {
        let w = band * (1.0 - gap.max(0.05));
        let x = plot.left() + band * i as f32 + (band - w) / 2.0;
        let top = plot.bottom() - plot.height() * geo::along(*v, scale.min, scale.max, false, false);
        painter.rect_filled(Rect::from_min_max(Pos2::new(x, top), Pos2::new(x + w, plot.bottom())), 0, colour);
        if band > 24.0 * m.zoom || i % 2 == 0 {
            painter.text(Pos2::new(x + w / 2.0, plot.bottom() + 2.0), egui::Align2::CENTER_TOP, unluminous_sheet_trim(&labels[i], 9), m.label(), colours.dim);
        }
    }
    plot
}

fn draw_hierarchy(painter: &egui::Painter, area: Rect, leaves: &[(Vec<String>, f64)], chart: &Chart, colours: &Colours, m: &Metrics) {
    let sunburst = chart.spec.ex.as_ref().and_then(|e| e.series.first()).is_some_and(|s| s.layout == ExLayout::Sunburst);
    // Group the leaves by their first level.
    let mut parents: Vec<(String, Vec<(String, f64)>)> = Vec::new();
    for (path, v) in leaves {
        let top = path.first().cloned().unwrap_or_default();
        let leaf = path.last().cloned().unwrap_or_default();
        match parents.iter_mut().find(|p| p.0 == top) {
            Some(p) => p.1.push((leaf, *v)),
            None => parents.push((top, vec![(path.last().cloned().unwrap_or_default(), *v)])),
        }
    }
    let totals: Vec<f64> = parents.iter().map(|p| p.1.iter().map(|l| l.1.max(0.0)).sum()).collect();
    if sunburst {
        let centre = area.center();
        let radius = area.width().min(area.height()) / 2.0 - 4.0;
        let spans = geo::slices(&totals, 0.0);
        for (pi, (parent, children)) in parents.iter().enumerate() {
            ring_segment(painter, centre, radius * 0.25, radius * 0.55, spans[pi].0, spans[pi].1, colours.series(pi), colours);
            let leaf_spans = geo::slices(&children.iter().map(|c| c.1).collect::<Vec<_>>(), 0.0);
            for (ci, (name, _)) in children.iter().enumerate() {
                let (a, b) = (spans[pi].0 + leaf_spans[ci].0 * (spans[pi].1 - spans[pi].0) / std::f64::consts::TAU, spans[pi].0 + leaf_spans[ci].1 * (spans[pi].1 - spans[pi].0) / std::f64::consts::TAU);
                ring_segment(painter, centre, radius * 0.55, radius, a, b, lighten(colours.series(pi), 0.3), colours);
                if children.len() == 1 && name == parent {
                    continue;
                }
            }
            let mid = (spans[pi].0 + spans[pi].1) / 2.0;
            painter.text(geo::on_circle(centre, radius * 0.4, mid), egui::Align2::CENTER_CENTER, unluminous_sheet_trim(parent, 8), m.label(), Color32::WHITE);
        }
        return;
    }
    let outer = geo::treemap(&totals, area);
    for (pi, (parent, children)) in parents.iter().enumerate() {
        let r = outer[pi];
        if r == Rect::NOTHING {
            continue;
        }
        let inner = geo::treemap(&children.iter().map(|c| c.1).collect::<Vec<_>>(), r.shrink(1.0));
        for (ci, (name, v)) in children.iter().enumerate() {
            let cell = inner[ci];
            if cell == Rect::NOTHING {
                continue;
            }
            painter.rect(cell, 0, shade(colours.series(pi), 0.85 + 0.15 * (ci % 3) as f32 / 2.0), Stroke::new(1.0, colours.paper), egui::StrokeKind::Inside);
            if cell.width() > 30.0 && cell.height() > 14.0 {
                painter.text(cell.left_top() + Vec2::new(3.0, 2.0), egui::Align2::LEFT_TOP, unluminous_sheet_trim(name, (cell.width() / 6.0) as usize), m.label(), Color32::WHITE);
            }
            let _ = v;
        }
        let _ = parent;
    }
}

#[allow(clippy::too_many_arguments)]
fn ring_segment(painter: &egui::Painter, centre: Pos2, inner: f32, outer: f32, a0: f64, a1: f64, colour: Color32, colours: &Colours) {
    let pieces = (((a1 - a0) / 0.3).ceil() as usize).max(1);
    for p in 0..pieces {
        let (b0, b1) = (a0 + (a1 - a0) * p as f64 / pieces as f64, a0 + (a1 - a0) * (p + 1) as f64 / pieces as f64);
        let quad = vec![geo::on_circle(centre, outer, b0), geo::on_circle(centre, outer, b1), geo::on_circle(centre, inner, b1), geo::on_circle(centre, inner, b0)];
        painter.add(Shape::convex_polygon(quad, colour, Stroke::new(0.5, colours.paper)));
    }
}
