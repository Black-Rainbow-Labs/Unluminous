//! What a drawing needs to draw one chart: each series' numbers and categories read from the cells now,
//! the scale Excel would choose for each axis, trendline fits, error bar sizes, and the figures behind the
//! newer chart types (histogram bins, Pareto shares, box quartiles, waterfall running totals).
//!
//! [`plot`] computes it; nothing here draws. The window's chart painter turns a [`Plot`] into shapes.

pub mod equation;
pub mod scale;
pub mod stats;
pub mod trend;

use crate::chartdata::{self, Resolved};
use crate::chartspec::*;
use crate::model::{Chart, Workbook};
use crate::value::Value;

pub use scale::{AxisScale, Fixed};
pub use stats::{Bin, BinKind, BoxStats, ErrorAmounts, WaterfallBar, WaterfallKind};
pub use trend::TrendResult;

/// How much room the plot has, in points. It decides how many gridlines Excel would draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotSize {
    pub width: f64,
    pub height: f64,
}

impl Default for PlotSize {
    fn default() -> Self {
        Self { width: 360.0, height: 216.0 }
    }
}

/// Error bars worked out for a series.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorResult {
    pub direction: ErrorDirection,
    pub amounts: ErrorAmounts,
    pub no_cap: bool,
}

/// One series, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotSeries {
    /// Index into the chart's series.
    pub index: usize,
    pub group: usize,
    pub secondary: bool,
    pub name: String,
    /// The Y values; `None` is an empty cell, a gap unless the chart plots blanks as zero.
    pub values: Vec<Option<f64>>,
    /// The X positions: 1, 2, 3 for a category chart, the X values for a scatter or bubble chart.
    pub x: Vec<f64>,
    pub sizes: Vec<Option<f64>>,
    pub labels: Vec<String>,
    /// For a stacked series, where each point's bar starts and ends; otherwise zero and the value.
    pub base: Vec<f64>,
    pub top: Vec<Option<f64>>,
    pub trendlines: Vec<TrendResult>,
    pub error_bars: Vec<ErrorResult>,
}

/// An axis, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotAxis {
    pub id: u32,
    pub kind: AxisKind,
    pub scale: Option<AxisScale>,
    /// The gridline and label positions of a value axis, with the label each carries.
    pub ticks: Vec<(f64, String)>,
    pub minor_ticks: Vec<f64>,
    /// The labels of a category axis.
    pub categories: Vec<String>,
    /// The value on this axis where the axis it crosses is drawn.
    pub cross_at: f64,
    pub reverse: bool,
}

/// The figures behind a `chartEx` chart.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtendedPlot {
    Histogram {
        bins: Vec<Bin>,
    },
    Pareto {
        rows: Vec<(String, f64, f64)>,
    },
    BoxWhisker {
        boxes: Vec<(String, BoxStats)>,
    },
    Waterfall {
        bars: Vec<(String, WaterfallBar)>,
    },
    Funnel {
        items: Vec<(String, f64)>,
    },
    /// A treemap or sunburst: each leaf with the path of names above it.
    Hierarchy {
        leaves: Vec<(Vec<String>, f64)>,
    },
    /// A map chart is drawn as a card naming its regions.
    Map {
        regions: Vec<(String, f64)>,
    },
}

/// Everything a drawing needs for one chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    pub chart_type: ChartType,
    pub title: String,
    pub series: Vec<PlotSeries>,
    pub axes: Vec<PlotAxis>,
    pub extended: Option<ExtendedPlot>,
    pub blanks: Blanks,
}

fn text_of(book: &Workbook, sheet: usize, formula: &Option<String>, fallback: String) -> String {
    let Some(f) = formula else { return fallback };
    if let Some(t) = f.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return t.replace("\"\"", "\"");
    }
    let r = chartdata::resolve(book, sheet, f);
    let joined = r.texts.iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        fallback
    } else {
        joined
    }
}

fn number_of(v: &Value, blanks: Blanks) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Empty => (blanks == Blanks::Zero).then_some(0.0),
        Value::Text(_) => Some(0.0),
        _ => None,
    }
}

/// Keep what a mask lets through.
fn keep<T: Clone>(items: &[T], mask: &[bool]) -> Vec<T> {
    items
        .iter()
        .zip(mask.iter().chain(std::iter::repeat(&true)))
        .filter(|(_, k)| **k)
        .map(|(i, _)| i.clone())
        .collect()
}

/// Read one series from the cells now.
fn read_series(book: &Workbook, sheet: usize, chart: &Chart, index: usize) -> PlotSeries {
    let s = &chart.series[index];
    let spec = &chart.spec;
    let blanks = spec.blanks_as();
    let values = chartdata::resolve(book, sheet, &s.values);
    let cats =
        s.categories.as_deref().map(|c| chartdata::resolve(book, sheet, c)).unwrap_or_default();
    let sizes = s.sizes.as_deref().map(|c| chartdata::resolve(book, sheet, c)).unwrap_or_default();
    let n = values.values.len();
    let mut mask: Vec<bool> = (0..n)
        .map(|i| !(spec.visible_only() && values.hidden.get(i).copied().unwrap_or(false)))
        .collect();
    for (i, m) in mask.iter_mut().enumerate() {
        if s.fmt.filtered_categories.contains(&(i as u32)) {
            *m = false;
        }
    }
    let group = spec.groups.get(s.group);
    let scatter = group.is_some_and(|g| matches!(g.kind, GroupKind::Scatter | GroupKind::Bubble));
    let ys: Vec<Option<f64>> = values.values.iter().map(|v| number_of(v, blanks)).collect();
    let ys = keep(&ys, &mask);
    let labels: Vec<String> = if cats.texts.len() == n {
        keep(&cats.texts, &mask)
    } else {
        (1..=ys.len()).map(|i| i.to_string()).collect()
    };
    let numeric_cats = cats.values.len() == n
        && n > 0
        && cats.values.iter().all(|v| matches!(v, Value::Number(_)));
    let x: Vec<f64> = if scatter && numeric_cats {
        keep(&cats.values.iter().map(|v| v.as_number().unwrap_or(0.0)).collect::<Vec<_>>(), &mask)
    } else if numeric_cats && cats.dates {
        keep(&cats.values.iter().map(|v| v.as_number().unwrap_or(0.0)).collect::<Vec<_>>(), &mask)
    } else {
        (1..=ys.len()).map(|i| i as f64).collect()
    };
    let sizes_v: Vec<Option<f64>> =
        keep(&sizes.values.iter().map(|v| number_of(v, Blanks::Gap)).collect::<Vec<_>>(), &mask);
    let name = text_of(book, sheet, &s.name, format!("Series{}", index + 1));
    PlotSeries {
        index,
        group: s.group,
        secondary: spec.is_secondary(s.group),
        name,
        base: vec![0.0; ys.len()],
        top: ys.clone(),
        values: ys,
        x,
        sizes: sizes_v,
        labels,
        trendlines: Vec::new(),
        error_bars: Vec::new(),
    }
}

/// Stack the series of each stacked group: each point's bar runs from the total of the series before it.
fn stack(chart: &Chart, series: &mut [PlotSeries]) {
    for (g, group) in chart.spec.groups.iter().enumerate() {
        if !matches!(group.grouping, Grouping::Stacked | Grouping::PercentStacked)
            || matches!(
                group.kind,
                GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie | GroupKind::Stock
            )
        {
            continue;
        }
        let members: Vec<usize> = (0..series.len()).filter(|i| series[*i].group == g).collect();
        let points = members.iter().map(|i| series[*i].values.len()).max().unwrap_or(0);
        for p in 0..points {
            let total: f64 = members
                .iter()
                .filter_map(|i| series[*i].values.get(p).copied().flatten())
                .map(f64::abs)
                .sum();
            let (mut up, mut down) = (0.0, 0.0);
            for &i in &members {
                let Some(Some(v)) = series[i].values.get(p).copied() else { continue };
                let scale = if group.grouping == Grouping::PercentStacked && total != 0.0 {
                    1.0 / total
                } else {
                    1.0
                };
                let v = v * scale;
                let (base, top) = if v >= 0.0 {
                    let b = up;
                    up += v;
                    (b, up)
                } else {
                    let b = down;
                    down += v;
                    (b, down)
                };
                series[i].base[p] = base;
                series[i].top[p] = Some(top);
            }
        }
    }
}

/// The trendlines and error bars of each series.
fn add_fits(book: &Workbook, sheet: usize, chart: &Chart, series: &mut [PlotSeries]) {
    for s in series.iter_mut() {
        let source = &chart.series[s.index];
        for t in &source.fmt.trendlines {
            let pairs: Vec<(f64, f64)> =
                s.x.iter()
                    .copied()
                    .zip(s.values.iter())
                    .filter_map(|(x, y)| y.map(|y| (x, y)))
                    .collect();
            let (xs, ys): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
            if let Some(fit) = trend::fit(t, &xs, &ys, &s.name) {
                s.trendlines.push(fit);
            }
        }
        for bars in &source.fmt.error_bars {
            let custom = |src: &Option<ErrorSource>| match src {
                Some(ErrorSource::Range(f)) => chartdata::resolve(book, sheet, f)
                    .values
                    .iter()
                    .map(|v| v.as_number().unwrap_or(0.0))
                    .collect(),
                Some(ErrorSource::Numbers(n)) => n.clone(),
                None => Vec::new(),
            };
            let amounts =
                stats::error_amounts(bars, &s.values, (custom(&bars.plus), custom(&bars.minus)));
            s.error_bars.push(ErrorResult {
                direction: bars.direction.unwrap_or(ErrorDirection::Y),
                amounts,
                no_cap: bars.no_cap,
            });
        }
    }
}

/// The smallest and largest of some numbers.
fn extent(values: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    values.filter(|v| v.is_finite()).fold(None, |acc, v| match acc {
        None => Some((v, v)),
        Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
    })
}

/// The data extents along the Y (or X) direction of the series on an axis.
fn data_extent(
    chart: &Chart,
    series: &[PlotSeries],
    axis: &Axis,
    horizontal_x: bool,
) -> Option<(f64, f64)> {
    let mut points: Vec<f64> = Vec::new();
    for s in series {
        let group = chart.spec.groups.get(s.group)?;
        if !group.axes.contains(&axis.id) {
            continue;
        }
        if horizontal_x {
            points.extend(s.x.iter().copied());
            continue;
        }
        if group.grouping == Grouping::PercentStacked {
            points.extend([0.0, 1.0]);
            continue;
        }
        let stacked = matches!(group.grouping, Grouping::Stacked);
        if stacked {
            points.extend(s.top.iter().flatten().copied());
            points.extend(s.base.iter().copied());
        } else {
            points.extend(s.values.iter().flatten().copied());
        }
        for e in &s.error_bars {
            if e.direction == ErrorDirection::Y {
                for (i, v) in s.values.iter().enumerate() {
                    if let Some(v) = v {
                        points.push(v + e.amounts.plus.get(i).copied().unwrap_or(0.0));
                        points.push(v - e.amounts.minus.get(i).copied().unwrap_or(0.0));
                    }
                }
            }
        }
    }
    let (lo, hi) = extent(points.into_iter())?;
    Some((lo, hi))
}

fn fixed_of(a: &Axis) -> Fixed {
    Fixed { min: a.min, max: a.max, major: a.major_unit, minor: a.minor_unit, log_base: a.log_base }
}

/// The text of a tick under the axis' number format: the one set on the axis, or the source cells' own.
fn tick_text(value: f64, axis: &Axis, source_format: &str) -> String {
    let code = match &axis.num_fmt {
        Some((code, linked)) if !*linked => code.as_str(),
        _ if source_format.is_empty() => "General",
        _ => source_format,
    };
    crate::numfmt::Format::parse(code).render(&Value::Number(value), false, 11).text
}

/// The scale of a value axis and whether the axis belongs to a 100 percent stacked group.
fn value_axis_scale(
    chart: &Chart,
    series: &[PlotSeries],
    axis: &Axis,
    is_x_of_scatter: bool,
    length: f64,
) -> (AxisScale, bool) {
    let spec = &chart.spec;
    let percent = spec
        .groups
        .iter()
        .any(|g| g.axes.contains(&axis.id) && g.grouping == Grouping::PercentStacked);
    let (lo, hi) = data_extent(chart, series, axis, is_x_of_scatter).unwrap_or((0.0, 1.0));
    let fixed = fixed_of(axis);
    let scale = if percent {
        // A 100 percent chart runs from 0 to 100 percent in tenths, whatever the data.
        AxisScale {
            min: fixed.min.unwrap_or(0.0),
            max: fixed.max.unwrap_or(1.0),
            major: fixed.major.unwrap_or(if length < 150.0 { 0.2 } else { 0.1 }),
            minor: fixed.minor.unwrap_or(0.02),
            log: false,
        }
    } else {
        scale::value_scale(lo, hi, fixed, length)
    };
    (scale, percent)
}

/// The figures of one axis: its scale and ticks for a value axis, its labels for a category axis.
fn plot_axis(
    chart: &Chart,
    series: &[PlotSeries],
    size: PlotSize,
    format: &str,
    axis: &Axis,
) -> PlotAxis {
    let spec = &chart.spec;
    let scatter = spec.groups.iter().any(|g| {
        g.axes.contains(&axis.id) && matches!(g.kind, GroupKind::Scatter | GroupKind::Bubble)
    });
    let is_x_of_scatter = scatter && spec.groups.iter().any(|g| g.axes.first() == Some(&axis.id));
    let horizontal = axis.position == AxisPosition::Bottom || axis.position == AxisPosition::Top;
    let length = if horizontal { size.width } else { size.height };
    let mut pa = PlotAxis {
        id: axis.id,
        kind: axis.kind,
        scale: None,
        ticks: Vec::new(),
        minor_ticks: Vec::new(),
        categories: Vec::new(),
        cross_at: 0.0,
        reverse: axis.reverse,
    };
    if axis.kind == AxisKind::Value {
        let (scale, percent) = value_axis_scale(chart, series, axis, is_x_of_scatter, length);
        let tick_format =
            if percent && axis.num_fmt.as_ref().is_none_or(|f| f.1) { "0%" } else { format };
        pa.ticks = scale::ticks(&scale)
            .into_iter()
            .map(|v| (v, tick_text(v, axis, tick_format)))
            .collect();
        pa.scale = Some(scale);
    } else {
        let longest = series
            .iter()
            .filter(|s| spec.groups.get(s.group).is_some_and(|g| g.axes.contains(&axis.id)))
            .max_by_key(|s| s.labels.len());
        pa.categories = longest.map(|s| s.labels.clone()).unwrap_or_default();
    }
    pa
}

/// Set where each axis is crossed by the other: the value on the crossing axis.
fn set_cross_points(spec: &ChartSpec, out: &mut [PlotAxis]) {
    let scales: Vec<(u32, Option<AxisScale>)> = out.iter().map(|a| (a.id, a.scale)).collect();
    for (i, axis) in spec.axes.iter().enumerate() {
        let other = scales.iter().find(|(id, _)| *id == axis.cross_axis).and_then(|(_, s)| *s);
        let own = &spec.axes[i];
        // `crosses` on an axis says where this axis crosses the other one, in the other's values.
        out[i].cross_at = match (own.crosses, other) {
            (Crosses::At(v), _) => v,
            (Crosses::Minimum, Some(s)) => s.min,
            (Crosses::Maximum, Some(s)) => s.max,
            (_, Some(s)) => 0.0f64.clamp(s.min, s.max),
            _ => 0.0,
        };
    }
}

/// The figures of every axis of a chart: scales, ticks, categories and crossing points.
fn plot_axes(
    book: &Workbook,
    chart: &Chart,
    series: &[PlotSeries],
    size: PlotSize,
    format: &str,
) -> Vec<PlotAxis> {
    let mut out: Vec<PlotAxis> =
        chart.spec.axes.iter().map(|axis| plot_axis(chart, series, size, format, axis)).collect();
    let _ = book;
    set_cross_points(&chart.spec, &mut out);
    out
}

/// Compute everything a drawing needs for one chart on a sheet.
pub fn plot(book: &Workbook, sheet: usize, chart: &Chart, size: PlotSize) -> Plot {
    let chart_type = chart.chart_type();
    let title = chart.title.as_deref().map(|t| match t.strip_prefix('=') {
        Some(f) => chartdata::resolve(book, sheet, f).texts.join(" "),
        None => t.to_owned(),
    });
    let mut out = Plot {
        chart_type,
        title: title.unwrap_or_default(),
        series: Vec::new(),
        axes: Vec::new(),
        extended: None,
        blanks: chart.spec.blanks_as(),
    };
    if chart.spec.ex.is_some() {
        out.extended = Some(extended(book, sheet, chart));
        return out;
    }
    let mut series: Vec<PlotSeries> = (0..chart.series.len())
        .filter(|i| !chart.series[*i].fmt.filtered)
        .map(|i| read_series(book, sheet, chart, i))
        .collect();
    stack(chart, &mut series);
    add_fits(book, sheet, chart, &mut series);
    let format = chart
        .series
        .first()
        .map(|s| chartdata::resolve(book, sheet, &s.values).format)
        .unwrap_or_default();
    out.axes = plot_axes(book, chart, &series, size, &format);
    out.series = series;
    out
}

/// The boxes of a box and whisker chart, grouping the values by their labels.
fn box_whisker_plot(
    pairs: Vec<(String, f64)>,
    cats: &Resolved,
    values: &Resolved,
    numbers: &[f64],
    options: &BoxOptions,
) -> ExtendedPlot {
    let mut groups: Vec<(String, Vec<f64>)> = Vec::new();
    for (label, v) in pairs {
        match groups.iter_mut().find(|(l, _)| *l == label) {
            Some(g) => g.1.push(v),
            None => groups.push((label, vec![v])),
        }
    }
    if cats.texts.is_empty()
        || groups.len() == values.values.len() && groups.iter().all(|g| g.1.len() == 1)
    {
        groups = vec![(String::new(), numbers.to_vec())];
    }
    ExtendedPlot::BoxWhisker {
        boxes: groups
            .into_iter()
            .filter_map(|(l, v)| stats::box_stats(&v, options).map(|b| (l, b)))
            .collect(),
    }
}

/// The leaves of a treemap or sunburst: each row's category cells make its path.
fn hierarchy_plot(pairs: Vec<(String, f64)>, cats: &Resolved, values: &Resolved) -> ExtendedPlot {
    let (rows, cols) = cats.shape;
    let leaves = if cols > 1 && rows as usize * cols as usize == cats.texts.len() {
        (0..rows as usize)
            .map(|r| {
                let path =
                    (0..cols as usize).map(|c| cats.texts[r * cols as usize + c].clone()).collect();
                (path, values.values.get(r).and_then(Value::as_number).unwrap_or(0.0))
            })
            .collect()
    } else {
        pairs.into_iter().map(|(l, v)| (vec![l], v)).collect()
    };
    ExtendedPlot::Hierarchy { leaves }
}

/// The figures of a `chartEx` chart.
fn extended(book: &Workbook, sheet: usize, chart: &Chart) -> ExtendedPlot {
    let ex = chart.spec.ex.clone().unwrap_or_default();
    let first = ex.series.first().cloned().unwrap_or_default();
    let s = chart.series.get(first.data);
    let values: Resolved =
        s.map(|s| chartdata::resolve(book, sheet, &s.values)).unwrap_or_default();
    let cats: Resolved = s
        .and_then(|s| s.categories.as_deref())
        .map(|c| chartdata::resolve(book, sheet, c))
        .unwrap_or_default();
    let numbers: Vec<f64> = values.values.iter().filter_map(Value::as_number).collect();
    let labels: Vec<String> = if cats.texts.len() == values.values.len() {
        cats.texts.clone()
    } else {
        (1..=values.values.len()).map(|i| i.to_string()).collect()
    };
    let pairs = || -> Vec<(String, f64)> {
        labels
            .iter()
            .cloned()
            .zip(values.values.iter().map(|v| v.as_number().unwrap_or(0.0)))
            .collect()
    };
    match first.layout {
        ExLayout::ClusteredColumn if ex.series.iter().any(|e| e.layout == ExLayout::ParetoLine) => {
            let nums: Vec<f64> = pairs().iter().map(|p| p.1).collect();
            ExtendedPlot::Pareto { rows: stats::pareto(&labels, &nums) }
        }
        ExLayout::ClusteredColumn | ExLayout::ParetoLine => ExtendedPlot::Histogram {
            bins: stats::histogram(&numbers, &first.binning.unwrap_or_default()),
        },
        ExLayout::BoxWhisker => box_whisker_plot(
            pairs(),
            &cats,
            &values,
            &numbers,
            &first.box_options.unwrap_or_default(),
        ),
        ExLayout::Waterfall => {
            let nums: Vec<f64> = pairs().iter().map(|p| p.1).collect();
            let bars = stats::waterfall(&nums, &first.subtotals);
            ExtendedPlot::Waterfall { bars: labels.into_iter().zip(bars).collect() }
        }
        ExLayout::Funnel => ExtendedPlot::Funnel { items: pairs() },
        ExLayout::Treemap | ExLayout::Sunburst => hierarchy_plot(pairs(), &cats, &values),
        ExLayout::RegionMap => ExtendedPlot::Map { regions: pairs() },
    }
}
