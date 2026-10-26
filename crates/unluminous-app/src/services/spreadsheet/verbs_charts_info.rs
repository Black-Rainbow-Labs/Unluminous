//! The chart read back (`task-2360`, the `chartwire` lane): `chart-types` lists the type words, and
//! `chart-info <n>` answers what a chart is and what is drawn for it, as the drawing sees it: the type, each
//! series with its figures and trendline equations, each axis with the scale `plot::plot` worked out, the
//! legend, the place and the name. An agent uses it to check a change without taking a picture.

use serde_json::{json, Value};
use unluminous_sheet::chartspec::ChartType;
use unluminous_sheet::model::Chart;
use unluminous_sheet::objects::geometry;
use unluminous_sheet::objects::EMU_PER_POINT;
use unluminous_sheet::plot::{plot, ExtendedPlot, Plot, PlotAxis, PlotSeries, PlotSize};

use super::verbs_a::Out;
use super::verbs_charts::chart_number;
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// `chart-types`: every word `chart` takes for a type, with Excel's number for it.
pub fn types() -> Out {
    let all: Vec<Value> = ChartType::ALL
        .iter()
        .chain([ChartType::Combo].iter())
        .map(
            |t| json!({ "type": t.name(), "excel": t.excel_number(), "extended": t.is_extended() }),
        )
        .collect();
    let words: Vec<&str> = ChartType::ALL.iter().map(|t| t.name()).collect();
    Ok(Answer::said(format!("{} chart types: {}", all.len(), words.join(" "))).with(json!(all)))
}

/// `chart-info <n>`: the chart and what is drawn for it.
pub fn info(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (index, _) = chart_number(args)?;
    let at = book.view.sheet;
    let chart = book.session.book.sheets[at]
        .charts
        .get(index)
        .ok_or_else(|| format!("there is no chart {} on this sheet", index + 1))?;
    let drawn = plot(&book.session.book, at, chart, PlotSize::default());
    let value = describe(book, at, chart, &drawn, index);
    let text = format!(
        "Chart {}: {}, {} series",
        index + 1,
        chart.chart_type().name(),
        chart.series.len()
    );
    Ok(Answer::said(text).with(value))
}

/// Everything about one chart as data.
fn describe(book: &Book, at: usize, chart: &Chart, drawn: &Plot, index: usize) -> Value {
    let sheet = &book.session.book.sheets[at];
    let (x0, y0) = geometry::point_of(sheet, chart.from);
    let (x1, y1) = geometry::point_of(sheet, chart.to);
    let pt = |v: i64| (v as f64 / EMU_PER_POINT as f64 * 100.0).round() / 100.0;
    json!({
        "number": index + 1,
        "type": chart.chart_type().name(),
        "title": drawn.title,
        "name": chart.spec.name,
        "alt": chart.spec.alt_text,
        "style": chart.spec.style,
        "from": chart.from.0.a1(),
        "to": chart.to.0.a1(),
        "left": pt(x0), "top": pt(y0), "width": pt(x1 - x0), "height": pt(y1 - y0),
        "legend": chart.spec.legend.as_ref().map(|l| format!("{:?}", l.position).to_lowercase()),
        "dataTable": chart.spec.data_table.is_some(),
        "hiddenCellsPlotted": !chart.spec.visible_only(),
        "emptyCells": format!("{:?}", chart.spec.blanks_as()).to_lowercase(),
        "groups": groups(chart),
        "series": series(chart, drawn),
        "axes": drawn.axes.iter().map(axis).collect::<Vec<_>>(),
        "extended": drawn.extended.as_ref().map(extended),
    })
}

/// The chart groups of the plot area.
fn groups(chart: &Chart) -> Vec<Value> {
    chart
        .spec
        .groups
        .iter()
        .map(|g| {
            json!({
                "kind": format!("{:?}", g.kind).to_lowercase(),
                "grouping": format!("{:?}", g.grouping).to_lowercase(),
                "gapWidth": g.gap_width, "overlap": g.overlap,
                "holeSize": g.hole_size, "firstSliceAngle": g.first_slice_angle,
                "varyColours": g.vary_colors,
            })
        })
        .collect()
}

/// The series, each with the formulas it reads and the figures drawn for it.
fn series(chart: &Chart, drawn: &Plot) -> Vec<Value> {
    chart
        .series
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let plotted = drawn.series.iter().find(|p| p.index == i);
            json!({
                "number": i + 1,
                "name": s.name, "values": s.values, "categories": s.categories, "sizes": s.sizes,
                "group": s.group,
                "hidden": s.fmt.filtered,
                "secondary": plotted.map(|p| p.secondary),
                "labels": s.fmt.labels.as_ref().is_some_and(|l| !l.delete),
                "drawn": plotted.map(|p| p.values.clone()),
                "trendlines": plotted.map(trendlines),
                "errorBars": s.fmt.error_bars.len(),
            })
        })
        .collect()
}

/// The trendlines fitted to a series: their equations and how well they fit.
fn trendlines(p: &PlotSeries) -> Vec<Value> {
    p.trendlines
        .iter()
        .map(|t| {
            json!({
                "name": t.name, "kind": t.kind.xml(), "equation": t.equation,
                "rSquared": t.r_squared, "coefficients": t.coefficients,
            })
        })
        .collect()
}

/// An axis with the scale it is drawn at.
fn axis(a: &PlotAxis) -> Value {
    json!({
        "id": a.id,
        "kind": format!("{:?}", a.kind).to_lowercase(),
        "reverse": a.reverse,
        "min": a.scale.map(|s| s.min), "max": a.scale.map(|s| s.max),
        "major": a.scale.map(|s| s.major), "minor": a.scale.map(|s| s.minor),
        "log": a.scale.map(|s| s.log),
        "ticks": a.ticks.iter().map(|(_, text)| text.clone()).collect::<Vec<_>>(),
        "categories": a.categories,
    })
}

/// What an Office 2016 chart holds, by kind and count.
fn extended(e: &ExtendedPlot) -> Value {
    let (kind, items) = match e {
        ExtendedPlot::Histogram { bins } => ("histogram", bins.len()),
        ExtendedPlot::Pareto { rows } => ("pareto", rows.len()),
        ExtendedPlot::BoxWhisker { boxes } => ("box-whisker", boxes.len()),
        ExtendedPlot::Waterfall { bars } => ("waterfall", bars.len()),
        ExtendedPlot::Funnel { items } => ("funnel", items.len()),
        ExtendedPlot::Hierarchy { leaves } => ("hierarchy", leaves.len()),
        ExtendedPlot::Map { regions } => ("map", regions.len()),
    };
    json!({ "kind": kind, "items": items })
}
