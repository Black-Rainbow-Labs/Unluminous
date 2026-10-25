//! The quick layouts of Excel's `Chart Design`, `Quick Layout` gallery, read from Excel by
//! `tools/excel-oracle/charts-layouts.ps1`: for each family of chart, which elements layout 1, 2, 3 and
//! so on switch on and off.

use crate::chartspec::*;
use crate::model::Chart;

/// What a layout switches on, for one chart family.
#[derive(Clone, Copy)]
struct Layout {
    title: bool,
    legend: Option<LegendPosition>,
    data_table: bool,
    labels: Option<(LabelPosition, bool, bool, bool, bool)>,
    category_axis: bool,
    value_axis: bool,
    category_title: bool,
    value_title: bool,
    category_major: bool,
    category_minor: bool,
    value_major: bool,
    value_minor: bool,
    trendline: bool,
    drop_lines: bool,
    /// The gap width and overlap of a column or bar chart, which some layouts change.
    gap: Option<(u32, i32)>,
}

const COLUMN: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, -25)) },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, -25)) },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 0)) },
    Layout { title: true, legend: None, data_table: true, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: None, data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: Some((300, 0)) },
    Layout { title: true, legend: None, data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((0, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 40)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
];

const STACKED: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((55, 100)) },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::Center, true, false, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((95, 100)) },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 100)) },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: Some((LabelPosition::Center, true, false, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 100)) },
    Layout { title: true, legend: None, data_table: true, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((95, 100)) },
    Layout { title: true, legend: None, data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((55, 100)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: Some((75, 100)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((300, 100)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((300, 0)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 100)) },
];

const BAR: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, -25)) },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, -25)) },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 0)) },
    Layout { title: true, legend: None, data_table: true, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: None, data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: Some((300, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((75, 40)) },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: Some((150, 0)) },
];

const LINE: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::Right, true, false, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: None, data_table: true, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: None, data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: true, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: Some((LabelPosition::Center, true, false, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: Some((LabelPosition::Right, true, false, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: None, data_table: false, labels: None, category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
];

const AREA: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: Some((LabelPosition::Center, true, false, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: None, data_table: true, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: true, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
];

const PIE: &[Layout] = &[
    Layout { title: true, legend: None, data_table: false, labels: Some((LabelPosition::BestFit, false, true, true, false)), category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::BestFit, false, false, true, false)), category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: None, data_table: false, labels: Some((LabelPosition::BestFit, true, true, false, false)), category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: None, data_table: false, labels: Some((LabelPosition::BestFit, false, true, false, false)), category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: Some((LabelPosition::BestFit, false, false, true, false)), category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: false, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
];

const SCATTER: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::Right, true, true, false, false)), category_axis: true, value_axis: false, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: true, category_minor: true, value_major: true, value_minor: true, trendline: true, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: false, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: Some((LabelPosition::Right, true, true, false, false)), category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: Some((LabelPosition::Right, true, true, false, false)), category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: true, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Bottom), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: true, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: true, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: true, value_title: true, category_major: true, category_minor: true, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: true, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
];

const RADAR: &[Layout] = &[
    Layout { title: true, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: false, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: false, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
    Layout { title: true, legend: Some(LegendPosition::Top), data_table: false, labels: Some((LabelPosition::OutsideEnd, true, false, false, false)), category_axis: false, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: true, trendline: false, drop_lines: false, gap: None },
    Layout { title: false, legend: Some(LegendPosition::Right), data_table: false, labels: None, category_axis: false, value_axis: true, category_title: false, value_title: false, category_major: false, category_minor: false, value_major: true, value_minor: false, trendline: false, drop_lines: false, gap: None },
];

/// The layouts Excel offers a chart of a type.
fn layouts_for(ty: ChartType) -> Option<&'static [Layout]> {
    use ChartType as T;
    Some(match ty {
        T::ColumnStacked | T::ColumnStacked100 => STACKED,
        T::BarClustered | T::BarStacked | T::BarStacked100 | T::Bar3DClustered | T::Bar3DStacked | T::Bar3DStacked100 => BAR,
        T::Line | T::LineStacked | T::LineStacked100 | T::LineMarkers | T::LineMarkersStacked | T::LineMarkersStacked100 | T::Line3D => LINE,
        T::Area | T::AreaStacked | T::AreaStacked100 | T::Area3D | T::Area3DStacked | T::Area3DStacked100 => AREA,
        T::Pie | T::PieExploded | T::Pie3D | T::Pie3DExploded | T::PieOfPie | T::BarOfPie | T::Doughnut | T::DoughnutExploded => PIE,
        T::ScatterMarkers | T::ScatterSmooth | T::ScatterSmoothNoMarkers | T::ScatterLines | T::ScatterLinesNoMarkers | T::Bubble | T::Bubble3D => SCATTER,
        T::Radar | T::RadarMarkers | T::RadarFilled => RADAR,
        T::Treemap | T::Sunburst | T::Histogram | T::Pareto | T::BoxWhisker | T::Waterfall | T::Funnel | T::Map => return None,
        _ => COLUMN,
    })
}

/// How many quick layouts Excel offers a chart of a type.
pub fn layout_count(ty: ChartType) -> u32 {
    layouts_for(ty).map_or(0, |l| l.len() as u32)
}

/// Apply quick layout `n` (counted from 1) to a chart: titles, legend, labels, axes and gridlines switch to
/// what the layout says, and the chart's formatting is left alone.
pub fn apply(chart: &mut Chart, n: u32) -> Result<(), String> {
    let ty = chart.chart_type();
    let table = layouts_for(ty).ok_or("Quick layouts are not available for this type of chart.")?;
    let layout = n
        .checked_sub(1)
        .and_then(|i| table.get(i as usize))
        .ok_or_else(|| format!("This chart type has {} quick layouts.", table.len()))?;
    set_title(chart, layout);
    chart.spec.legend = layout.legend.map(|p| Legend { position: p, ..chart.spec.legend.take().unwrap_or_default() });
    chart.spec.data_table = layout.data_table.then(DataTable::default);
    for s in &mut chart.series {
        s.fmt.labels = layout.labels.map(|(position, value, category, percent, series)| DataLabels {
            fields: LabelFields {
                position: Some(position).filter(|_| !matches!(ty, ChartType::Pie3D | ChartType::Pie3DExploded | ChartType::Doughnut | ChartType::DoughnutExploded)),
                show_value: value,
                show_category: category,
                show_percent: percent,
                show_series: series,
                ..LabelFields::default()
            },
            show_leader_lines: matches!(ty, ChartType::Pie | ChartType::PieExploded).then_some(true),
            ..DataLabels::default()
        });
        if !layout.trendline {
            s.fmt.trendlines.clear();
        }
    }
    if layout.trendline {
        if let Some(s) = chart.series.first_mut().filter(|s| s.fmt.trendlines.is_empty()) {
            s.fmt.trendlines.push(Trendline::default());
        }
    }
    set_axes(chart, layout);
    for g in &mut chart.spec.groups {
        g.drop_lines = layout.drop_lines;
        if let (GroupKind::Bar, false, Some((gap, overlap))) = (g.kind, g.three_d, layout.gap) {
            if g.grouping == Grouping::Clustered {
                g.gap_width = Some(gap);
                g.overlap = Some(overlap);
            }
        }
    }
    Ok(())
}

fn set_title(chart: &mut Chart, layout: &Layout) {
    if layout.title {
        // Excel puts a title element with no words of its own, which shows "Chart Title" until it is edited.
        chart.spec.title_deleted = false;
        chart.spec.title_auto = chart.title.is_none();
    } else {
        chart.title = None;
        chart.spec.title_deleted = true;
        chart.spec.title_auto = false;
    }
}

/// The primary category and value axes (the first and second axis of the first group) follow the layout.
fn set_axes(chart: &mut Chart, layout: &Layout) {
    let Some(group) = chart.spec.groups.first() else { return };
    let ids = group.axes.clone();
    for (position, id) in ids.iter().take(2).enumerate() {
        let is_value = position == 1;
        let Some(axis) = chart.spec.axis_mut(*id) else { continue };
        let (shown, titled, major, minor) = if is_value {
            (layout.value_axis, layout.value_title, layout.value_major, layout.value_minor)
        } else {
            (layout.category_axis, layout.category_title, layout.category_major, layout.category_minor)
        };
        axis.deleted = !shown;
        axis.major_gridlines = major.then(|| axis.major_gridlines.take().unwrap_or_default());
        axis.minor_gridlines = minor.then(|| axis.minor_gridlines.take().unwrap_or_default());
        axis.title = titled.then(|| {
            axis.title.take().unwrap_or(AxisTitle { text: "Axis Title".to_owned(), format: TitleFormat::default() })
        });
    }
}
