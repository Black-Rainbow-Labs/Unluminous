//! Building the groups and axes of a chart for each type Excel's Insert Chart dialog offers, as Excel
//! builds them, and turning an existing chart into another type.

use crate::chartspec::*;
use crate::model::{Chart, ChartSeries};

/// The ids of a chart's primary axes, and of its secondary ones.
const PRIMARY: [u32; 3] = [500_000_001, 500_000_002, 500_000_003];
const SECONDARY: [u32; 2] = [500_000_011, 500_000_012];

/// The groups, axes and extra raw parts of a type.
pub struct Built {
    pub groups: Vec<Group>,
    pub axes: Vec<Axis>,
    pub view_3d: Option<String>,
    pub floor_and_walls: bool,
    pub ex: Option<ExSpec>,
}

const VIEW_3D: &str = "<c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:depthPercent val=\"100\"/><c:rAngAx val=\"1\"/></c:view3D>";
const VIEW_3D_PERSPECTIVE: &str = "<c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:depthPercent val=\"100\"/><c:rAngAx val=\"0\"/><c:perspective val=\"30\"/></c:view3D>";
const VIEW_SURFACE: &str = "<c:view3D><c:rotX val=\"15\"/><c:rotY val=\"20\"/><c:depthPercent val=\"100\"/><c:rAngAx val=\"0\"/></c:view3D>";
const VIEW_CONTOUR: &str = "<c:view3D><c:rotX val=\"90\"/><c:rotY val=\"0\"/><c:rAngAx val=\"0\"/><c:perspective val=\"0\"/></c:view3D>";
const VIEW_PIE_3D: &str = "<c:view3D><c:rotX val=\"30\"/><c:rotY val=\"0\"/><c:rAngAx val=\"0\"/></c:view3D>";

/// A category axis as Excel makes one.
pub fn category_axis(id: u32, cross: u32, position: AxisPosition) -> Axis {
    Axis {
        id,
        kind: AxisKind::Category,
        position,
        cross_axis: cross,
        major_tick: TickMark::None,
        minor_tick: TickMark::None,
        num_fmt: Some(("General".to_owned(), true)),
        auto: Some(true),
        label_align: Some("ctr".to_owned()),
        label_offset: Some(100),
        no_multi_level: Some(false),
        ..Axis::default()
    }
}

/// A value axis as Excel makes one.
pub fn value_axis(id: u32, cross: u32, position: AxisPosition, between: bool) -> Axis {
    Axis {
        id,
        kind: AxisKind::Value,
        position,
        cross_axis: cross,
        major_gridlines: Some(Gridlines::default()),
        major_tick: TickMark::None,
        minor_tick: TickMark::None,
        num_fmt: Some(("General".to_owned(), true)),
        between: Some(between),
        ..Axis::default()
    }
}

fn series_axis(id: u32, cross: u32) -> Axis {
    Axis {
        id,
        kind: AxisKind::Series,
        position: AxisPosition::Bottom,
        cross_axis: cross,
        major_tick: TickMark::None,
        minor_tick: TickMark::None,
        ..Axis::default()
    }
}

fn bar(horizontal: bool, grouping: Grouping, three_d: bool) -> Group {
    let stacked = grouping != Grouping::Clustered && grouping != Grouping::Standard;
    Group {
        kind: GroupKind::Bar,
        three_d,
        horizontal,
        grouping,
        vary_colors: Some(false),
        gap_width: Some(if three_d || stacked { 150 } else { 219 }),
        overlap: (!three_d).then_some(if stacked { 100 } else { -27 }),
        ..Group::default()
    }
}

fn line(grouping: Grouping, three_d: bool) -> Group {
    Group {
        kind: GroupKind::Line,
        three_d,
        grouping,
        vary_colors: Some(false),
        marker: (!three_d).then_some(true),
        ..Group::default()
    }
}

fn area(grouping: Grouping, three_d: bool) -> Group {
    Group { kind: GroupKind::Area, three_d, grouping, vary_colors: Some(false), ..Group::default() }
}

/// The chart groups and axes for a type.
pub fn build(ty: ChartType) -> Built {
    use ChartType as T;
    match ty {
        T::ColumnClustered | T::ColumnStacked | T::ColumnStacked100 => flat(bar(false, grouping_of(ty), false), cat_val(true)),
        T::Column3DClustered | T::Column3DStacked | T::Column3DStacked100 | T::Column3D => bar_3d(ty),
        T::Bar3DClustered | T::Bar3DStacked | T::Bar3DStacked100 => bar_3d(ty),
        T::BarClustered | T::BarStacked | T::BarStacked100 => flat(bar(true, grouping_of(ty), false), horizontal_axes()),
        T::Line | T::LineMarkers | T::LineStacked | T::LineMarkersStacked | T::LineStacked100 | T::LineMarkersStacked100 => {
            flat(line(grouping_of(ty), false), cat_val(true))
        }
        T::Line3D => three_d(line(Grouping::Standard, true), cat_val_ser(), VIEW_3D_PERSPECTIVE),
        T::Pie | T::PieExploded | T::Pie3D | T::Pie3DExploded | T::PieOfPie | T::BarOfPie | T::Doughnut | T::DoughnutExploded => {
            pie_family(ty)
        }
        T::Area | T::AreaStacked | T::AreaStacked100 => flat(area(grouping_of(ty), false), cat_val_mid()),
        T::Area3D | T::Area3DStacked | T::Area3DStacked100 => {
            let grouping = grouping_of(ty);
            let axes = if grouping == Grouping::Standard { cat_val_ser() } else { cat_val(true) };
            three_d(area(grouping, true), axes, VIEW_3D_PERSPECTIVE)
        }
        T::ScatterMarkers | T::ScatterSmooth | T::ScatterSmoothNoMarkers | T::ScatterLines | T::ScatterLinesNoMarkers => {
            flat(scatter(ty), scatter_axes())
        }
        T::Bubble | T::Bubble3D => flat(bubble(ty), scatter_axes()),
        T::StockHlc | T::StockOhlc => flat(stock(ty, PRIMARY[..2].to_vec()), cat_val(true)),
        T::StockVhlc | T::StockVohlc => stock_with_volume(ty),
        T::Surface | T::SurfaceWireframe | T::SurfaceTopView | T::SurfaceTopViewWireframe => surface(ty),
        T::Radar | T::RadarMarkers | T::RadarFilled => flat(radar(ty), cat_val(true)),
        T::Treemap | T::Sunburst | T::Histogram | T::Pareto | T::BoxWhisker | T::Waterfall | T::Funnel | T::Map => {
            Built { ex: Some(build_ex(ty)), ..empty() }
        }
        T::Combo => flat(bar(false, Grouping::Clustered, false), cat_val(true)),
    }
}

fn empty() -> Built {
    Built { groups: Vec::new(), axes: Vec::new(), view_3d: None, floor_and_walls: false, ex: None }
}

/// A chart of one group drawn against some axes.
fn flat(mut group: Group, axes: Vec<Axis>) -> Built {
    group.axes = axes.iter().map(|a| a.id).collect();
    Built { groups: vec![group], axes, ..empty() }
}

/// A 3D chart: one group, a view, and the floor and walls Excel draws.
fn three_d(group: Group, axes: Vec<Axis>, view: &str) -> Built {
    Built { view_3d: Some(view.to_owned()), floor_and_walls: true, ..flat(group, axes) }
}

/// How a type stacks its series.
fn grouping_of(ty: ChartType) -> Grouping {
    use ChartType as T;
    match ty {
        T::ColumnStacked | T::BarStacked | T::LineStacked | T::LineMarkersStacked | T::AreaStacked | T::Column3DStacked
        | T::Bar3DStacked | T::Area3DStacked => Grouping::Stacked,
        T::ColumnStacked100 | T::BarStacked100 | T::LineStacked100 | T::LineMarkersStacked100 | T::AreaStacked100
        | T::Column3DStacked100 | T::Bar3DStacked100 | T::Area3DStacked100 => Grouping::PercentStacked,
        T::Line | T::LineMarkers | T::Area | T::Area3D | T::Line3D | T::Column3D => Grouping::Standard,
        _ => Grouping::Clustered,
    }
}

fn cat_val(between: bool) -> Vec<Axis> {
    vec![
        category_axis(PRIMARY[0], PRIMARY[1], AxisPosition::Bottom),
        value_axis(PRIMARY[1], PRIMARY[0], AxisPosition::Left, between),
    ]
}

/// A bar chart's axes: the categories up the left and the values along the bottom.
fn horizontal_axes() -> Vec<Axis> {
    let mut axes = cat_val(true);
    axes[0].position = AxisPosition::Left;
    axes[1].position = AxisPosition::Bottom;
    axes
}

/// Category, value and series axes, for the 3D types that have a depth axis.
fn cat_val_ser() -> Vec<Axis> {
    let mut axes = cat_val(true);
    axes.push(series_axis(PRIMARY[2], PRIMARY[1]));
    axes
}

fn bar_3d(ty: ChartType) -> Built {
    let horizontal = matches!(ty, ChartType::Bar3DClustered | ChartType::Bar3DStacked | ChartType::Bar3DStacked100);
    let grouping = grouping_of(ty);
    let axes = if horizontal {
        horizontal_axes()
    } else if grouping == Grouping::Standard {
        cat_val_ser()
    } else {
        cat_val(true)
    };
    three_d(bar(horizontal, grouping, true), axes, VIEW_3D)
}

fn pie_family(ty: ChartType) -> Built {
    use ChartType as T;
    let group = match ty {
        T::Pie | T::PieExploded => {
            Group { kind: GroupKind::Pie, vary_colors: Some(true), first_slice_angle: Some(0), ..Group::default() }
        }
        T::Pie3D | T::Pie3DExploded => Group { kind: GroupKind::Pie, three_d: true, vary_colors: Some(true), ..Group::default() },
        T::PieOfPie | T::BarOfPie => Group {
            kind: GroupKind::OfPie,
            of_pie: if ty == T::BarOfPie { OfPieKind::Bar } else { OfPieKind::Pie },
            vary_colors: Some(true),
            gap_width: Some(100),
            split_by: SplitBy::Auto,
            second_size: Some(75),
            series_lines: true,
            ..Group::default()
        },
        _ => Group {
            kind: GroupKind::Doughnut,
            vary_colors: Some(true),
            first_slice_angle: Some(0),
            hole_size: Some(50),
            ..Group::default()
        },
    };
    let solid_3d = matches!(ty, T::Pie3D | T::Pie3DExploded);
    Built {
        groups: vec![group],
        view_3d: solid_3d.then(|| VIEW_PIE_3D.to_owned()),
        floor_and_walls: solid_3d,
        ..empty()
    }
}

fn scatter(ty: ChartType) -> Group {
    let style = match ty {
        ChartType::ScatterSmooth => "smoothMarker",
        ChartType::ScatterSmoothNoMarkers => "smooth",
        _ => "lineMarker",
    };
    Group { kind: GroupKind::Scatter, scatter_style: Some(style.to_owned()), vary_colors: Some(false), ..Group::default() }
}

fn bubble(ty: ChartType) -> Group {
    Group {
        kind: GroupKind::Bubble,
        vary_colors: Some(false),
        bubble_scale: Some(100),
        show_negative_bubbles: Some(false),
        bubble_3d: ty == ChartType::Bubble3D,
        ..Group::default()
    }
}

fn stock(ty: ChartType, axes: Vec<u32>) -> Group {
    let with_bars = matches!(ty, ChartType::StockOhlc | ChartType::StockVohlc);
    Group { kind: GroupKind::Stock, hi_low_lines: true, up_down_bars: with_bars.then_some(150), axes, ..Group::default() }
}

/// Volume columns on the primary axes and the stock group on the secondary ones.
fn stock_with_volume(ty: ChartType) -> Built {
    let volume = Group { axes: PRIMARY[..2].to_vec(), ..bar(false, Grouping::Clustered, false) };
    let mut axes = cat_val(true);
    let mut cat2 = category_axis(SECONDARY[0], SECONDARY[1], AxisPosition::Bottom);
    cat2.deleted = true;
    let mut val2 = value_axis(SECONDARY[1], SECONDARY[0], AxisPosition::Right, true);
    val2.major_gridlines = None;
    val2.crosses = Crosses::Maximum;
    axes.extend([val2, cat2]);
    Built { groups: vec![volume, stock(ty, SECONDARY.to_vec())], axes, ..empty() }
}

fn surface(ty: ChartType) -> Built {
    let top = matches!(ty, ChartType::SurfaceTopView | ChartType::SurfaceTopViewWireframe);
    let group = Group {
        kind: GroupKind::Surface,
        three_d: !top,
        wireframe: matches!(ty, ChartType::SurfaceWireframe | ChartType::SurfaceTopViewWireframe),
        ..Group::default()
    };
    three_d(group, cat_val_ser(), if top { VIEW_CONTOUR } else { VIEW_SURFACE })
}

fn radar(ty: ChartType) -> Group {
    let style = match ty {
        ChartType::RadarMarkers => "marker",
        ChartType::RadarFilled => "filled",
        _ => "standard",
    };
    Group { kind: GroupKind::Radar, radar_style: Some(style.to_owned()), vary_colors: Some(false), ..Group::default() }
}

fn cat_val_mid() -> Vec<Axis> {
    vec![
        category_axis(PRIMARY[0], PRIMARY[1], AxisPosition::Bottom),
        value_axis(PRIMARY[1], PRIMARY[0], AxisPosition::Left, false),
    ]
}

fn scatter_axes() -> Vec<Axis> {
    let mut x = value_axis(PRIMARY[0], PRIMARY[1], AxisPosition::Bottom, false);
    x.major_gridlines = None;
    let y = value_axis(PRIMARY[1], PRIMARY[0], AxisPosition::Left, false);
    vec![x, y]
}

/// The `chartEx` details of a new chart of an extended type.
fn build_ex(ty: ChartType) -> ExSpec {
    let layout = match ty {
        ChartType::Treemap => ExLayout::Treemap,
        ChartType::Sunburst => ExLayout::Sunburst,
        ChartType::Histogram | ChartType::Pareto => ExLayout::ClusteredColumn,
        ChartType::BoxWhisker => ExLayout::BoxWhisker,
        ChartType::Waterfall => ExLayout::Waterfall,
        ChartType::Funnel => ExLayout::Funnel,
        _ => ExLayout::RegionMap,
    };
    let mut series = ExSeries { layout, labels: Some((false, false, true)), ..ExSeries::default() };
    match ty {
        ChartType::Histogram => series.binning = Some(Binning::default()),
        ChartType::Pareto => series.aggregation = true,
        ChartType::BoxWhisker => series.box_options = Some(BoxOptions::default()),
        ChartType::Treemap => series.parent_labels = Some("overlapping".to_owned()),
        _ => {}
    }
    let mut ex = ExSpec { legend: false, series: vec![series], ..ExSpec::default() };
    let axis = |id, value| ExAxis { id, value_axis: value, tick_labels: true, major_gridlines: value, ..ExAxis::default() };
    match ty {
        ChartType::Histogram => {
            ex.axes = vec![
                ExAxis { gap_width: Some("0".to_owned()), ..axis(0, false) },
                axis(1, true),
            ]
        }
        ChartType::Pareto => {
            let mut line = ex.series[0].clone();
            line.layout = ExLayout::ParetoLine;
            line.aggregation = false;
            line.labels = None;
            line.format_index = 1;
            ex.series.push(line);
            ex.axes = vec![
                ExAxis { gap_width: Some("0.00999999978".to_owned()), ..axis(0, false) },
                axis(1, true),
                ExAxis { num_fmt: None, ..axis(2, true) },
            ];
        }
        ChartType::Waterfall => ex.axes = vec![ExAxis { gap_width: Some("0.5".to_owned()), ..axis(0, false) }, axis(1, true)],
        ChartType::BoxWhisker => ex.axes = vec![ExAxis { gap_width: Some("1".to_owned()), ..axis(0, false) }, axis(1, true)],
        ChartType::Funnel => ex.axes = vec![ExAxis { gap_width: Some("0.06".to_owned()), ..axis(0, false) }],
        _ => {}
    }
    ex
}

/// Set the series' own formatting to what a type needs: markers or none, smooth lines, explosion.
pub fn style_series(ty: ChartType, s: &mut ChartSeries) {
    use ChartType as T;
    let none = Marker { symbol: Some("none".to_owned()), ..Marker::default() };
    let f = &mut s.fmt;
    f.marker = None;
    f.smooth = None;
    f.explosion = None;
    match ty {
        T::Line | T::LineStacked | T::LineStacked100 => {
            f.marker = Some(none);
            f.smooth = Some(false);
        }
        T::LineMarkers | T::LineMarkersStacked | T::LineMarkersStacked100 => f.smooth = Some(false),
        T::PieExploded | T::Pie3DExploded | T::DoughnutExploded => f.explosion = Some(25),
        T::ScatterMarkers => {
            f.sp_pr = Some("<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>".to_owned());
            f.smooth = Some(false);
        }
        T::ScatterSmooth => f.smooth = Some(true),
        T::ScatterSmoothNoMarkers => {
            f.marker = Some(none);
            f.smooth = Some(true);
        }
        T::ScatterLines => f.smooth = Some(false),
        T::ScatterLinesNoMarkers => {
            f.marker = Some(none);
            f.smooth = Some(false);
        }
        T::StockHlc | T::StockOhlc | T::StockVhlc | T::StockVohlc => {
            f.sp_pr = Some("<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>".to_owned());
            f.marker = Some(none);
            f.smooth = Some(false);
        }
        T::Bubble | T::Bubble3D => f.bubble_3d = Some(ty == T::Bubble3D),
        T::RadarMarkers => {}
        T::Radar => f.marker = Some(none),
        _ => {}
    }
    if !matches!(ty, T::ScatterMarkers | T::StockHlc | T::StockOhlc | T::StockVhlc | T::StockVohlc)
        && f.sp_pr.as_deref().is_some_and(|s| s.contains("<a:noFill/>") && s.len() < 80)
    {
        f.sp_pr = None;
    }
}

/// Make a chart a chart of one type: its groups and axes are replaced, the series are kept, and the
/// titles and options of its axes are kept where the new axes are of the same kind.
pub fn apply_type(chart: &mut Chart, ty: ChartType) {
    let built = build(ty);
    let old_axes = std::mem::take(&mut chart.spec.axes);
    chart.spec.groups = built.groups;
    chart.spec.axes = built.axes;
    for axis in &mut chart.spec.axes {
        if let Some(old) = old_axes.iter().find(|o| o.kind == axis.kind && o.position == axis.position) {
            axis.title = old.title.clone();
            axis.min = old.min;
            axis.max = old.max;
            axis.major_unit = old.major_unit;
            axis.minor_unit = old.minor_unit;
            axis.log_base = old.log_base;
            axis.reverse = old.reverse;
            axis.num_fmt = old.num_fmt.clone();
            if old.major_gridlines.is_none() {
                axis.major_gridlines = None;
            }
            axis.minor_gridlines = old.minor_gridlines.clone();
        }
    }
    chart.spec.raw.view_3d = built.view_3d;
    if built.floor_and_walls {
        let wall = "<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>";
        chart.spec.raw.floor = Some(format!("<c:floor><c:thickness val=\"0\"/>{wall}</c:floor>"));
        chart.spec.raw.side_wall = Some(format!("<c:sideWall><c:thickness val=\"0\"/>{wall}</c:sideWall>"));
        chart.spec.raw.back_wall = Some(format!("<c:backWall><c:thickness val=\"0\"/>{wall}</c:backWall>"));
    } else {
        chart.spec.raw.floor = None;
        chart.spec.raw.side_wall = None;
        chart.spec.raw.back_wall = None;
    }
    chart.spec.ex = built.ex;
    for (i, s) in chart.series.iter_mut().enumerate() {
        s.group = 0;
        s.fmt.idx = Some(i as u32);
        s.fmt.order = Some(i as u32);
        style_series(ty, s);
    }
    if let Some(ex) = &mut chart.spec.ex {
        let template = ex.series.clone();
        ex.series = Vec::new();
        for (i, _) in chart.series.iter().enumerate() {
            for t in &template {
                ex.series.push(ExSeries { data: i, format_index: ex.series.len() as u32, ..t.clone() });
            }
        }
    }
    chart.spec.dirty = true;
    chart.sync_kind();
}

/// Add the secondary axes for a group and return their ids.
pub fn add_secondary_axes(spec: &mut ChartSpec, scatter: bool) -> Vec<u32> {
    if spec.axis(SECONDARY[0]).is_none() {
        let (mut cat, mut val) = if scatter {
            (
                value_axis(SECONDARY[0], SECONDARY[1], AxisPosition::Bottom, false),
                value_axis(SECONDARY[1], SECONDARY[0], AxisPosition::Right, false),
            )
        } else {
            (
                category_axis(SECONDARY[0], SECONDARY[1], AxisPosition::Bottom),
                value_axis(SECONDARY[1], SECONDARY[0], AxisPosition::Right, true),
            )
        };
        cat.deleted = true;
        cat.major_gridlines = None;
        cat.major_tick = TickMark::Outside;
        val.major_gridlines = None;
        val.major_tick = TickMark::Outside;
        val.crosses = Crosses::Maximum;
        // Excel writes the secondary value axis before the secondary category axis.
        spec.axes.push(val);
        spec.axes.push(cat);
    }
    SECONDARY.to_vec()
}

/// The ids of the primary axes a group of a kind is drawn against.
pub fn primary_axes() -> Vec<u32> {
    PRIMARY[..2].to_vec()
}
