//! The Office 2016 chart types, held in `chartEx` parts: treemap, sunburst, histogram, Pareto, box and
//! whisker, waterfall, funnel and map.
//!
//! The data of each series is named by `_xlchart.v1.N` defined names, which are resolved to the ranges
//! they name when the part is read, so a series is a range formula like any other. The part is written
//! with those ranges directly.

use roxmltree::{Document, Node};

use super::chart_read::{child, children, rich_text, Parsed};
use super::{escape, raw};
use crate::chartspec::*;
use crate::model::ChartSeries;

const CX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";
/// The namespace of `chartEx` parts' root element.
pub const NS: &str = CX_NS;

fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute(name)
}

fn flag_attr(n: Node, name: &str) -> Option<bool> {
    attr(n, name).map(|v| matches!(v, "1" | "true"))
}

/// The ranges the `cx:data` elements name, by id: (categories, values).
struct Data {
    id: u32,
    categories: Option<String>,
    values: Option<String>,
}

fn read_data(chart_data: Node, resolve: &dyn Fn(&str) -> String) -> Vec<Data> {
    let mut out = Vec::new();
    for data in children(chart_data, "data") {
        let id = attr(data, "id").and_then(|v| v.parse().ok()).unwrap_or(0);
        let formula = |name: &str| {
            children(data, name)
                .next()
                .and_then(|d| child(d, "f"))
                .and_then(|f| f.text())
                .map(|t| resolve(t.trim()))
        };
        out.push(Data { id, categories: formula("strDim"), values: formula("numDim") });
    }
    out
}

fn read_series_name(tx: Node, resolve: &dyn Fn(&str) -> String) -> Option<String> {
    let data = child(tx, "txData")?;
    match child(data, "f").and_then(|f| f.text()) {
        Some(f) => Some(resolve(f.trim())),
        None => child(data, "v").and_then(|v| v.text()).map(|t| format!("\"{}\"", t.replace('"', "\"\""))),
    }
}

/// Read a `chartEx` part. `resolve` turns a formula that is a defined name into the range it names.
pub fn parse(xml: &str, resolve: &dyn Fn(&str) -> String) -> Option<Parsed> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    let chart = child(root, "chart")?;
    let data = child(root, "chartData").map(|d| read_data(d, resolve)).unwrap_or_default();
    let mut ex = ExSpec::default();
    let mut spec = ChartSpec::default();
    let mut title = None;
    if let Some(t) = child(chart, "title") {
        ex.title_pos = attr(t, "pos").map(str::to_owned);
        ex.title_align = attr(t, "align").map(str::to_owned);
        title = child(t, "tx").map(|tx| match child(tx, "rich") {
            Some(rich) => rich_text(rich),
            None => {
                let d = child(tx, "txData");
                match d.and_then(|d| child(d, "f")).and_then(|f| f.text()) {
                    Some(f) => format!("={}", resolve(f.trim())),
                    None => d.and_then(|d| child(d, "v")).and_then(|v| v.text()).unwrap_or("").to_owned(),
                }
            }
        });
    }
    if let Some(l) = child(chart, "legend") {
        ex.legend = true;
        ex.legend_pos = attr(l, "pos").map(str::to_owned);
        ex.legend_align = attr(l, "align").map(str::to_owned);
        ex.legend_overlay = flag_attr(l, "overlay").unwrap_or(false);
    }
    let mut series = Vec::new();
    if let Some(plot) = child(chart, "plotArea") {
        if let Some(region) = child(plot, "plotAreaRegion") {
            let mut seen: Vec<(u32, usize)> = Vec::new();
            for s in children(region, "series") {
                read_series(xml, s, &data, &mut ex, &mut series, &mut seen, resolve);
            }
        }
        ex.axes = children(plot, "axis").map(|a| read_axis(xml, a)).collect();
    }
    spec.raw.namespaces = super::namespaces_of(root);
    spec.ex = Some(ex);
    Some(Parsed { title: title.filter(|t| !t.is_empty()), series, spec })
}

fn read_axis(xml: &str, a: Node) -> ExAxis {
    let value_axis = child(a, "valScaling").is_some();
    let scaling = child(a, if value_axis { "valScaling" } else { "catScaling" });
    ExAxis {
        id: attr(a, "id").and_then(|v| v.parse().ok()).unwrap_or(0),
        value_axis,
        gap_width: scaling.and_then(|s| attr(s, "gapWidth")).map(str::to_owned),
        min: scaling.and_then(|s| attr(s, "min")).and_then(|v| v.parse().ok()),
        max: scaling.and_then(|s| attr(s, "max")).and_then(|v| v.parse().ok()),
        major_unit: scaling.and_then(|s| attr(s, "majorUnit")).and_then(|v| v.parse().ok()),
        minor_unit: scaling.and_then(|s| attr(s, "minorUnit")).and_then(|v| v.parse().ok()),
        hidden: flag_attr(a, "hidden").unwrap_or(false),
        title: child(a, "title").map(|t| raw(xml, t).to_owned()),
        major_gridlines: child(a, "majorGridlines").is_some(),
        minor_gridlines: child(a, "minorGridlines").is_some(),
        tick_labels: child(a, "tickLabels").is_some(),
        num_fmt: child(a, "numFmt").map(|n| raw(xml, n).to_owned()),
        raw: None,
    }
}

fn read_series(
    xml: &str,
    s: Node,
    data: &[Data],
    ex: &mut ExSpec,
    series: &mut Vec<ChartSeries>,
    seen: &mut Vec<(u32, usize)>,
    resolve: &dyn Fn(&str) -> String,
) {
    let layout = attr(s, "layoutId").and_then(ExLayout::from_id).unwrap_or_default();
    let data_id: u32 = child(s, "dataId").and_then(|d| attr(d, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut e = ExSeries {
        layout,
        unique_id: attr(s, "uniqueId").unwrap_or("").to_owned(),
        format_index: attr(s, "formatIdx").and_then(|v| v.parse().ok()).unwrap_or(0),
        hidden: flag_attr(s, "hidden").unwrap_or(false),
        sp_pr: child(s, "spPr").map(|n| raw(xml, n).to_owned()),
        ..ExSeries::default()
    };
    // Two series of a Pareto chart share one data element, and so share one model series.
    let existing = seen.iter().find(|(id, _)| *id == data_id).map(|(_, i)| *i);
    let found = data.iter().find(|d| d.id == data_id);
    let index = existing.unwrap_or_else(|| {
        series.push(ChartSeries {
            name: child(s, "tx").and_then(|tx| read_series_name(tx, resolve)),
            categories: found.and_then(|d| d.categories.clone()),
            values: found.and_then(|d| d.values.clone()).unwrap_or_default(),
            ..ChartSeries::default()
        });
        seen.push((data_id, series.len() - 1));
        series.len() - 1
    });
    e.data = index;
    if let Some(labels) = child(s, "dataLabels") {
        e.label_position = attr(labels, "pos").map(str::to_owned);
        e.label_num_fmt = child(labels, "numFmt").and_then(|n| attr(n, "formatCode")).map(str::to_owned);
        e.labels = child(labels, "visibility").map(|v| {
            (
                flag_attr(v, "seriesName").unwrap_or(false),
                flag_attr(v, "categoryName").unwrap_or(false),
                flag_attr(v, "value").unwrap_or(false),
            )
        });
        e.tx_pr = child(labels, "txPr").map(|n| raw(xml, n).to_owned());
    }
    e.points = children(s, "dataPt")
        .map(|p| {
            (
                attr(p, "idx").and_then(|v| v.parse().ok()).unwrap_or(0),
                child(p, "spPr").map(|n| raw(xml, n).to_owned()).unwrap_or_default(),
            )
        })
        .collect();
    if let Some(pr) = child(s, "layoutPr") {
        read_layout_options(pr, &mut e);
    }
    e.ext = child(s, "extLst").map(|n| raw(xml, n).to_owned());
    ex.series.push(e);
}

fn read_layout_options(pr: Node, e: &mut ExSeries) {
    if let Some(b) = child(pr, "binning") {
        let by = match (child(b, "binSize"), child(b, "binCount")) {
            (Some(w), _) => attr(w, "val").and_then(|v| v.parse().ok()).map(BinBy::Width),
            (_, Some(c)) => attr(c, "val").and_then(|v| v.parse().ok()).map(BinBy::Count),
            _ => None,
        };
        e.binning = Some(Binning {
            by: by.unwrap_or(BinBy::Automatic),
            overflow: attr(b, "overflow").and_then(|v| v.parse().ok()),
            underflow: attr(b, "underflow").and_then(|v| v.parse().ok()),
            right_closed: attr(b, "intervalClosed") != Some("l"),
        });
    }
    if let Some(sub) = child(pr, "subtotals") {
        e.subtotals = children(sub, "idx").filter_map(|i| attr(i, "val")?.parse().ok()).collect();
    }
    e.parent_labels = child(pr, "parentLabelLayout").and_then(|p| attr(p, "val")).map(str::to_owned);
    e.aggregation = child(pr, "aggregation").is_some();
    if child(pr, "statistics").is_some() || child(pr, "visibility").is_some() {
        let mut o = BoxOptions::default();
        if let Some(q) = child(pr, "statistics").and_then(|s| attr(s, "quartileMethod")) {
            o.exclusive_median = q == "exclusive";
        }
        if let Some(v) = child(pr, "visibility") {
            o.mean_line = flag_attr(v, "meanLine").unwrap_or(o.mean_line);
            o.mean_marker = flag_attr(v, "meanMarker").unwrap_or(o.mean_marker);
            o.inner_points = flag_attr(v, "nonoutliers").unwrap_or(o.inner_points);
            o.outlier_points = flag_attr(v, "outliers").unwrap_or(o.outlier_points);
        }
        e.box_options = Some(o);
    }
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Write a `chartEx` part.
pub fn write(chart: &crate::model::Chart) -> String {
    let Some(ex) = &chart.spec.ex else { return String::new() };
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str(&format!("<cx:chartSpace xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:cx=\"{CX_NS}\">"));
    out.push_str("<cx:chartData>");
    for (i, s) in chart.series.iter().enumerate() {
        out.push_str(&format!("<cx:data id=\"{i}\">"));
        if let Some(c) = &s.categories {
            out.push_str(&format!("<cx:strDim type=\"cat\"><cx:f>{}</cx:f></cx:strDim>", escape(c)));
        }
        let kind = match ex.series.first().map(|e| e.layout) {
            Some(ExLayout::Treemap | ExLayout::Sunburst) => "size",
            _ => "val",
        };
        out.push_str(&format!("<cx:numDim type=\"{kind}\"><cx:f>{}</cx:f></cx:numDim></cx:data>", escape(&s.values)));
    }
    out.push_str("</cx:chartData><cx:chart>");
    out.push_str(&title_xml(chart, ex));
    out.push_str("<cx:plotArea><cx:plotAreaRegion>");
    for (i, e) in ex.series.iter().enumerate() {
        out.push_str(&series_xml(chart, e, i));
    }
    out.push_str("</cx:plotAreaRegion>");
    for a in &ex.axes {
        out.push_str(&axis_xml(a));
    }
    out.push_str("</cx:plotArea>");
    if ex.legend {
        out.push_str(&format!(
            "<cx:legend pos=\"{}\" align=\"{}\" overlay=\"{}\"/>",
            ex.legend_pos.as_deref().unwrap_or("t"),
            ex.legend_align.as_deref().unwrap_or("ctr"),
            u8::from(ex.legend_overlay)
        ));
    }
    out.push_str("</cx:chart></cx:chartSpace>");
    out
}

fn title_xml(chart: &crate::model::Chart, ex: &ExSpec) -> String {
    let Some(title) = &chart.title else { return String::new() };
    let body = match title.strip_prefix('=') {
        Some(f) => format!("<cx:txData><cx:f>{}</cx:f></cx:txData>", escape(f)),
        None => format!("<cx:txData><cx:v>{}</cx:v></cx:txData>", escape(title)),
    };
    format!(
        "<cx:title pos=\"{}\" align=\"{}\" overlay=\"0\"><cx:tx>{body}</cx:tx></cx:title>",
        ex.title_pos.as_deref().unwrap_or("t"),
        ex.title_align.as_deref().unwrap_or("ctr")
    )
}

fn axis_xml(a: &ExAxis) -> String {
    let mut out = format!("<cx:axis id=\"{}\"{}>", a.id, if a.hidden { " hidden=\"1\"" } else { "" });
    let mut scaling = String::new();
    if let Some(g) = &a.gap_width {
        scaling.push_str(&format!(" gapWidth=\"{g}\""));
    }
    for (name, v) in [("max", a.max), ("min", a.min), ("majorUnit", a.major_unit), ("minorUnit", a.minor_unit)] {
        if let Some(v) = v {
            scaling.push_str(&format!(" {name}=\"{}\"", num(v)));
        }
    }
    out.push_str(&format!("<cx:{0}{scaling}/>", if a.value_axis { "valScaling" } else { "catScaling" }));
    out.push_str(a.title.as_deref().unwrap_or(""));
    if a.major_gridlines {
        out.push_str("<cx:majorGridlines/>");
    }
    if a.minor_gridlines {
        out.push_str("<cx:minorGridlines/>");
    }
    if a.tick_labels {
        out.push_str("<cx:tickLabels/>");
    }
    out.push_str(a.num_fmt.as_deref().unwrap_or(""));
    out.push_str("</cx:axis>");
    out
}

fn series_xml(chart: &crate::model::Chart, e: &ExSeries, position: usize) -> String {
    let data = chart.series.get(e.data);
    let mut out = format!("<cx:series layoutId=\"{}\"", e.layout.id());
    if e.hidden {
        out.push_str(" hidden=\"1\"");
    }
    let unique = if e.unique_id.is_empty() {
        format!("{{{:08X}-0000-4000-8000-{:012X}}}", position + 1, position + 1)
    } else {
        e.unique_id.clone()
    };
    out.push_str(&format!(" uniqueId=\"{}\" formatIdx=\"{}\">", escape(&unique), e.format_index));
    if let Some(name) = data.and_then(|d| d.name.as_ref()) {
        out.push_str(&match name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
            Some(t) => format!("<cx:tx><cx:txData><cx:v>{}</cx:v></cx:txData></cx:tx>", escape(t)),
            None => format!("<cx:tx><cx:txData><cx:f>{}</cx:f></cx:txData></cx:tx>", escape(name)),
        });
    }
    out.push_str(e.sp_pr.as_deref().unwrap_or(""));
    for (idx, sp) in &e.points {
        out.push_str(&format!("<cx:dataPt idx=\"{idx}\">{sp}</cx:dataPt>"));
    }
    if let Some((name, category, value)) = e.labels {
        let pos = e.label_position.as_deref().map(|p| format!(" pos=\"{p}\"")).unwrap_or_default();
        out.push_str(&format!("<cx:dataLabels{pos}>"));
        if let Some(code) = &e.label_num_fmt {
            out.push_str(&format!("<cx:numFmt formatCode=\"{}\" sourceLinked=\"0\"/>", escape(code)));
        }
        out.push_str(e.tx_pr.as_deref().unwrap_or(""));
        out.push_str(&format!(
            "<cx:visibility seriesName=\"{}\" categoryName=\"{}\" value=\"{}\"/></cx:dataLabels>",
            u8::from(name),
            u8::from(category),
            u8::from(value)
        ));
    }
    out.push_str(&format!("<cx:dataId val=\"{}\"/>", e.data));
    out.push_str(&layout_xml(e));
    if e.layout == ExLayout::ParetoLine {
        out.push_str("<cx:axisId val=\"2\"/>");
    }
    out.push_str(e.ext.as_deref().unwrap_or(""));
    out.push_str("</cx:series>");
    out
}

fn layout_xml(e: &ExSeries) -> String {
    let mut inner = String::new();
    if let Some(p) = &e.parent_labels {
        inner.push_str(&format!("<cx:parentLabelLayout val=\"{p}\"/>"));
    }
    if let Some(o) = &e.box_options {
        inner.push_str(&format!(
            "<cx:visibility meanLine=\"{}\" meanMarker=\"{}\" nonoutliers=\"{}\" outliers=\"{}\"/>",
            u8::from(o.mean_line),
            u8::from(o.mean_marker),
            u8::from(o.inner_points),
            u8::from(o.outlier_points)
        ));
        inner.push_str(&format!(
            "<cx:statistics quartileMethod=\"{}\"/>",
            if o.exclusive_median { "exclusive" } else { "inclusive" }
        ));
    }
    if e.aggregation {
        inner.push_str("<cx:aggregation/>");
    }
    if let Some(b) = &e.binning {
        let mut attrs = format!(" intervalClosed=\"{}\"", if b.right_closed { "r" } else { "l" });
        if let Some(u) = b.underflow {
            attrs.push_str(&format!(" underflow=\"{}\"", num(u)));
        }
        if let Some(o) = b.overflow {
            attrs.push_str(&format!(" overflow=\"{}\"", num(o)));
        }
        match b.by {
            BinBy::Width(w) => inner.push_str(&format!("<cx:binning{attrs}><cx:binSize val=\"{}\"/></cx:binning>", num(w))),
            BinBy::Count(c) => inner.push_str(&format!("<cx:binning{attrs}><cx:binCount val=\"{c}\"/></cx:binning>")),
            _ => inner.push_str(&format!("<cx:binning{attrs}/>")),
        }
    }
    if e.layout == ExLayout::Waterfall {
        inner.push_str("<cx:subtotals>");
        for i in &e.subtotals {
            inner.push_str(&format!("<cx:idx val=\"{i}\"/>"));
        }
        inner.push_str("</cx:subtotals>");
    }
    if let Some(g) = &e.geography {
        inner.push_str(g);
    }
    if inner.is_empty() {
        "<cx:layoutPr/>".to_owned()
    } else {
        format!("<cx:layoutPr>{inner}</cx:layoutPr>")
    }
}
