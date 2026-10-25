//! Writing a `chart` part from the model, in the order the schema gives its elements.
//!
//! A chart read from a file and left alone is written back as the bytes it was; this is for a chart made
//! here or changed here. Whatever the model keeps as raw XML goes back where the schema wants it.

use super::escape;
use crate::chartspec::*;
use crate::model::{Chart, ChartSeries};

const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const C15_NS: &str = "http://schemas.microsoft.com/office/drawing/2012/chart";

/// What the writer asks of the workbook: whether a formula's cells hold text.
pub type TextTest<'a> = &'a dyn Fn(&str) -> bool;

fn opt(xml: &Option<String>) -> &str {
    xml.as_deref().unwrap_or("")
}

fn bool_val(tag: &str, on: bool) -> String {
    format!("<c:{tag} val=\"{}\"/>", u8::from(on))
}

fn num_val<T: std::fmt::Display>(tag: &str, v: T) -> String {
    format!("<c:{tag} val=\"{v}\"/>")
}

fn f64_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// The chart part for a chart.
pub fn chart_xml(chart: &Chart, is_text: TextTest) -> String {
    let spec = &chart.spec;
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str(&root_open(spec));
    out.push_str(opt(&spec.raw.date1904));
    out.push_str(opt(&spec.raw.lang));
    out.push_str(&bool_val("roundedCorners", spec.rounded_corners.unwrap_or(false)));
    if let Some(style) = spec.style {
        out.push_str(&style_xml(style));
    }
    out.push_str(opt(&spec.raw.clr_map_ovr));
    out.push_str(opt(&spec.raw.pivot_source));
    out.push_str(opt(&spec.raw.protection));
    out.push_str("<c:chart>");
    out.push_str(&chart_body(chart, is_text));
    out.push_str("</c:chart>");
    out.push_str(opt(&spec.raw.sp_pr));
    out.push_str(opt(&spec.raw.tx_pr));
    out.push_str(opt(&spec.raw.external_data));
    out.push_str(opt(&spec.raw.print_settings));
    out.push_str(opt(&spec.raw.user_shapes));
    out.push_str(opt(&spec.raw.ext));
    out.push_str("</c:chartSpace>");
    out
}

/// The `c:chartSpace` start tag with the namespaces the chart's kept XML may use.
fn root_open(spec: &ChartSpec) -> String {
    let mut namespaces: Vec<(String, String)> = spec.raw.namespaces.clone();
    for (prefix, uri) in [("c", CHART_NS), ("a", DRAWING_NS), ("r", REL_NS)] {
        if !namespaces.iter().any(|(p, _)| p == prefix) {
            namespaces.push((prefix.to_owned(), uri.to_owned()));
        }
    }
    let mut out = String::from("<c:chartSpace");
    for (prefix, uri) in &namespaces {
        if prefix.is_empty() {
            out.push_str(&format!(" xmlns=\"{}\"", escape(uri)));
        } else {
            out.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape(uri)));
        }
    }
    if let Some(ignorable) = &spec.raw.ignorable {
        out.push_str(&format!(" mc:Ignorable=\"{}\"", escape(ignorable)));
        if !namespaces.iter().any(|(p, _)| p == "mc") {
            out.push_str(" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"");
        }
    }
    out.push('>');
    out
}

/// The style number as Excel writes it: the 2010 element with the plain one as its fallback.
fn style_xml(style: u32) -> String {
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice Requires=\"c14\" xmlns:c14=\"http://schemas.microsoft.com/office/drawing/2007/8/2/chart\"><c14:style val=\"{}\"/></mc:Choice><mc:Fallback><c:style val=\"{style}\"/></mc:Fallback></mc:AlternateContent>",
        100 + style
    )
}

/// The children of `c:chart`.
fn chart_body(chart: &Chart, is_text: TextTest) -> String {
    let spec = &chart.spec;
    let mut out = String::new();
    out.push_str(&title_xml(chart.title.as_deref(), &spec.title, spec.title_auto));
    out.push_str(&bool_val("autoTitleDeleted", spec.title_deleted));
    out.push_str(opt(&spec.raw.pivot_formats));
    out.push_str(opt(&spec.raw.view_3d));
    out.push_str(opt(&spec.raw.floor));
    out.push_str(opt(&spec.raw.side_wall));
    out.push_str(opt(&spec.raw.back_wall));
    out.push_str(&plot_area_xml(chart, is_text));
    if let Some(legend) = &spec.legend {
        out.push_str(&legend_xml(legend));
    }
    out.push_str(&bool_val("plotVisOnly", spec.visible_only()));
    out.push_str(&format!("<c:dispBlanksAs val=\"{}\"/>", spec.blanks_as().xml()));
    if let Some(over) = spec.labels_over_max {
        out.push_str(&bool_val("showDLblsOverMax", over));
    }
    out.push_str(opt(&spec.raw.chart_ext));
    out
}

/// The words of a rich text block as paragraphs.
fn rich_xml(text: &str) -> String {
    let mut out = String::from("<c:rich><a:bodyPr/><a:lstStyle/>");
    for line in text.split('\n') {
        out.push_str("<a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:rPr lang=\"en-US\"/><a:t>");
        out.push_str(&escape(line));
        out.push_str("</a:t></a:r></a:p>");
    }
    out.push_str("</c:rich>");
    out
}

/// The `c:tx` of a title: the words as they were read when unchanged, otherwise written fresh.
fn tx_xml(text: &str, format: &TitleFormat) -> String {
    if let Some(formula) = text.strip_prefix('=') {
        return format!("<c:tx><c:strRef><c:f>{}</c:f></c:strRef></c:tx>", escape(formula));
    }
    if let Some(rich) = &format.rich {
        if rich_words(rich) == text {
            return rich.clone();
        }
    }
    format!("<c:tx>{}</c:tx>", rich_xml(text))
}

/// The words in a kept `c:tx` element.
fn rich_words(rich: &str) -> String {
    let wrapped = format!(
        "<w xmlns:c=\"{CHART_NS}\" xmlns:a=\"{DRAWING_NS}\">{rich}</w>"
    );
    let Ok(doc) = roxmltree::Document::parse(&wrapped) else { return String::new() };
    super::chart_read::rich_text(doc.root_element())
}

fn title_xml(text: Option<&str>, format: &TitleFormat, auto: bool) -> String {
    if text.is_none() && !auto {
        return String::new();
    }
    let mut out = String::from("<c:title>");
    if let Some(text) = text {
        out.push_str(&tx_xml(text, format));
    }
    out.push_str(opt(&format.layout));
    out.push_str(&bool_val("overlay", format.overlay));
    out.push_str(opt(&format.sp_pr));
    out.push_str(opt(&format.tx_pr));
    out.push_str("</c:title>");
    out
}

fn axis_title_xml(title: &AxisTitle) -> String {
    title_xml(Some(title.text.as_str()).filter(|t| !t.is_empty()), &title.format, true)
}

fn legend_xml(legend: &Legend) -> String {
    let mut out = format!("<c:legend><c:legendPos val=\"{}\"/>", legend.position.xml());
    for entry in &legend.deleted_entries {
        out.push_str(&format!("<c:legendEntry><c:idx val=\"{entry}\"/><c:delete val=\"1\"/></c:legendEntry>"));
    }
    out.push_str(opt(&legend.layout));
    out.push_str(&bool_val("overlay", legend.overlay));
    out.push_str(opt(&legend.sp_pr));
    out.push_str(opt(&legend.tx_pr));
    out.push_str("</c:legend>");
    out
}

fn plot_area_xml(chart: &Chart, is_text: TextTest) -> String {
    let spec = &chart.spec;
    let mut out = String::from("<c:plotArea>");
    out.push_str(spec.raw.plot_layout.as_deref().unwrap_or("<c:layout/>"));
    let ids = series_ids(chart);
    for (g, group) in spec.groups.iter().enumerate() {
        out.push_str(&group_xml(chart, g, group, &ids, is_text));
    }
    for axis in &spec.axes {
        out.push_str(&axis_xml(axis, spec));
    }
    if let Some(table) = &spec.data_table {
        out.push_str(&data_table_xml(table));
    }
    out.push_str(opt(&spec.raw.plot_sp_pr));
    out.push_str("</c:plotArea>");
    out
}

fn data_table_xml(t: &DataTable) -> String {
    format!(
        "<c:dTable>{}{}{}{}{}{}</c:dTable>",
        bool_val("showHorzBorder", t.horizontal_borders),
        bool_val("showVertBorder", t.vertical_borders),
        bool_val("showOutline", t.outline),
        bool_val("showKeys", t.legend_keys),
        opt(&t.sp_pr),
        opt(&t.tx_pr)
    )
}

/// The `idx` and `order` of each series: the ones it had, when they are all there and different, and its
/// place in the chart otherwise.
fn series_ids(chart: &Chart) -> Vec<(u32, u32)> {
    let usable = {
        let mut seen = std::collections::HashSet::new();
        chart.series.iter().all(|s| {
            matches!((s.fmt.idx, s.fmt.order), (Some(i), Some(_)) if seen.insert(i))
        })
    };
    chart
        .series
        .iter()
        .enumerate()
        .map(|(i, s)| match (usable, s.fmt.idx, s.fmt.order) {
            (true, Some(idx), Some(order)) => (idx, order),
            _ => (i as u32, i as u32),
        })
        .collect()
}

fn group_xml(chart: &Chart, index: usize, g: &Group, ids: &[(u32, u32)], is_text: TextTest) -> String {
    let element = g.kind.element(g.three_d);
    let mut out = format!("<c:{element}>");
    let members: Vec<usize> = chart
        .series
        .iter()
        .enumerate()
        .filter(|(_, s)| s.group == index && !s.fmt.filtered)
        .map(|(i, _)| i)
        .collect();
    out.push_str(&group_head(g));
    let mut sers = String::new();
    for i in members {
        sers.push_str(&series_xml(&chart.series[i], g.kind, ids[i], "c:ser", is_text));
    }
    out.push_str(&sers);
    if let Some(labels) = &g.labels {
        out.push_str(&labels_xml(labels));
    }
    out.push_str(&group_tail(g));
    for id in &g.axes {
        out.push_str(&num_val("axId", id));
    }
    out.push_str(&group_ext(chart, index, ids, g.ext.as_deref(), is_text));
    out.push_str(&format!("</c:{element}>"));
    out
}

/// A group's extension list: its kept extensions and its filtered series.
fn group_ext(chart: &Chart, index: usize, ids: &[(u32, u32)], kept: Option<&str>, is_text: TextTest) -> String {
    let filtered: Vec<usize> = chart
        .series
        .iter()
        .enumerate()
        .filter(|(_, s)| s.group == index && s.fmt.filtered)
        .map(|(i, _)| i)
        .collect();
    if filtered.is_empty() && kept.is_none() {
        return String::new();
    }
    let kind = chart.spec.groups[index].kind;
    let mut out = String::from("<c:extLst>");
    if !filtered.is_empty() {
        let holder = match kind {
            GroupKind::Bar => "filteredBarSeries",
            GroupKind::Line | GroupKind::Stock => "filteredLineSeries",
            GroupKind::Area => "filteredAreaSeries",
            GroupKind::Scatter => "filteredScatterSeries",
            GroupKind::Radar => "filteredRadarSeries",
            GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie => "filteredPieSeries",
            GroupKind::Surface => "filteredSurfaceSeries",
            GroupKind::Bubble => "filteredBubbleSeries",
        };
        out.push_str(&format!(
            "<c:ext uri=\"{{02D57815-91ED-43cb-92C2-25804820EDAC}}\" xmlns:c15=\"{C15_NS}\"><c15:{holder}>"
        ));
        for i in filtered {
            out.push_str(&series_xml(&chart.series[i], kind, ids[i], "c15:ser", is_text));
        }
        out.push_str(&format!("</c15:{holder}></c:ext>"));
    }
    out.push_str(kept.unwrap_or(""));
    out.push_str("</c:extLst>");
    out
}

/// What comes before a group's series.
fn group_head(g: &Group) -> String {
    let mut out = String::new();
    match g.kind {
        GroupKind::Bar => {
            out.push_str(&format!(
                "<c:barDir val=\"{}\"/><c:grouping val=\"{}\"/>",
                if g.horizontal { "bar" } else { "col" },
                g.grouping.xml()
            ));
            out.push_str(&vary(g));
        }
        GroupKind::Line | GroupKind::Area => {
            out.push_str(&format!("<c:grouping val=\"{}\"/>", g.grouping.xml()));
            out.push_str(&vary(g));
        }
        GroupKind::OfPie => {
            let kind = if g.of_pie == OfPieKind::Bar { "bar" } else { "pie" };
            out.push_str(&format!("<c:ofPieType val=\"{kind}\"/>"));
            out.push_str(&vary(g));
        }
        GroupKind::Scatter => {
            let style = g.scatter_style.as_deref().unwrap_or("lineMarker");
            out.push_str(&format!("<c:scatterStyle val=\"{style}\"/>"));
            out.push_str(&vary(g));
        }
        GroupKind::Radar => {
            let style = g.radar_style.as_deref().unwrap_or("standard");
            out.push_str(&format!("<c:radarStyle val=\"{style}\"/>"));
            out.push_str(&vary(g));
        }
        GroupKind::Surface => out.push_str(&bool_val("wireframe", g.wireframe)),
        GroupKind::Pie | GroupKind::Doughnut | GroupKind::Bubble => out.push_str(&vary(g)),
        GroupKind::Stock => {}
    }
    out
}

fn vary(g: &Group) -> String {
    g.vary_colors.map(|v| bool_val("varyColors", v)).unwrap_or_default()
}

/// One of a group's line elements: as it was read, or plain when it was just switched on.
fn line_element(g: &Group, slot: usize, on: bool, plain: &str) -> String {
    if !on {
        return String::new();
    }
    g.raw_lines[slot].clone().unwrap_or_else(|| plain.to_owned())
}

/// What follows a group's series and data labels, before its axis ids.
fn group_tail(g: &Group) -> String {
    let mut out = String::new();
    let lines = |out: &mut String| out.push_str(&line_element(g, 0, g.drop_lines, "<c:dropLines/>"));
    let up_down = || match g.up_down_bars {
        Some(w) => g.raw_lines[2].clone().unwrap_or_else(|| {
            format!("<c:upDownBars><c:gapWidth val=\"{w}\"/><c:upBars/><c:downBars/></c:upDownBars>")
        }),
        None => String::new(),
    };
    match g.kind {
        GroupKind::Bar => {
            if let Some(w) = g.gap_width {
                out.push_str(&num_val("gapWidth", w));
            }
            if g.three_d {
                if let Some(d) = g.gap_depth {
                    out.push_str(&num_val("gapDepth", d));
                }
                if let Some(shape) = &g.shape {
                    out.push_str(&format!("<c:shape val=\"{shape}\"/>"));
                }
            } else {
                if let Some(o) = g.overlap {
                    out.push_str(&num_val("overlap", o));
                }
                out.push_str(&line_element(g, 3, g.series_lines, "<c:serLines/>"));
            }
        }
        GroupKind::Line => {
            lines(&mut out);
            out.push_str(&line_element(g, 1, g.hi_low_lines, "<c:hiLowLines/>"));
            out.push_str(&up_down());
            if g.three_d {
                if let Some(d) = g.gap_depth {
                    out.push_str(&num_val("gapDepth", d));
                }
            } else {
                if let Some(m) = g.marker {
                    out.push_str(&bool_val("marker", m));
                }
                if let Some(s) = g.smooth {
                    out.push_str(&bool_val("smooth", s));
                }
            }
        }
        GroupKind::Stock => {
            lines(&mut out);
            out.push_str(&line_element(g, 1, g.hi_low_lines, "<c:hiLowLines/>"));
            out.push_str(&up_down());
        }
        GroupKind::Area => {
            lines(&mut out);
            if g.three_d {
                if let Some(d) = g.gap_depth {
                    out.push_str(&num_val("gapDepth", d));
                }
            }
        }
        GroupKind::Pie => {
            if !g.three_d {
                out.push_str(&num_val("firstSliceAng", g.first_slice_angle.unwrap_or(0)));
            }
        }
        GroupKind::Doughnut => {
            out.push_str(&num_val("firstSliceAng", g.first_slice_angle.unwrap_or(0)));
            out.push_str(&num_val("holeSize", g.hole_size.unwrap_or(50)));
        }
        GroupKind::OfPie => {
            if let Some(w) = g.gap_width {
                out.push_str(&num_val("gapWidth", w));
            }
            let split = match g.split_by {
                SplitBy::Auto => None,
                SplitBy::Position => Some("pos"),
                SplitBy::Value => Some("val"),
                SplitBy::Percent => Some("percent"),
                SplitBy::Custom => Some("cust"),
            };
            if let Some(split) = split {
                out.push_str(&format!("<c:splitType val=\"{split}\"/>"));
            }
            if let Some(at) = g.split_at {
                out.push_str(&num_val("splitPos", f64_text(at)));
            }
            out.push_str(opt(&g.custom_split));
            if let Some(size) = g.second_size {
                out.push_str(&num_val("secondPieSize", size));
            }
            out.push_str(&line_element(g, 3, g.series_lines, "<c:serLines/>"));
        }
        GroupKind::Bubble => {
            if let Some(s) = g.bubble_scale {
                out.push_str(&num_val("bubbleScale", s));
            }
            if let Some(n) = g.show_negative_bubbles {
                out.push_str(&bool_val("showNegBubbles", n));
            }
            if let Some(r) = &g.size_represents {
                out.push_str(&format!("<c:sizeRepresents val=\"{r}\"/>"));
            }
        }
        GroupKind::Surface => out.push_str(opt(&g.band_formats)),
        GroupKind::Scatter | GroupKind::Radar => {}
    }
    out
}

/// A formula as a series reference, or an array constant as a literal.
fn data_xml(tag: &str, formula: &str, text: bool, filtered: bool) -> String {
    if let Some(inner) = formula.strip_prefix('{').and_then(|f| f.strip_suffix('}')) {
        return literal_xml(tag, inner);
    }
    let kind = if text { "strRef" } else { "numRef" };
    if filtered {
        // A series filtered out of the chart names its cells in an extension, not in `c:f`.
        return format!(
            "<c:{tag}><c:{kind}><c:extLst><c:ext uri=\"{{02D57815-91ED-43cb-92C2-25804820EDAC}}\"><c15:formulaRef><c15:sqref>{}</c15:sqref></c15:formulaRef></c:ext></c:extLst></c:{kind}></c:{tag}>",
            escape(formula)
        );
    }
    format!("<c:{tag}><c:{kind}><c:f>{}</c:f></c:{kind}></c:{tag}>", escape(formula))
}

/// An array constant as a `numLit` or `strLit`.
fn literal_xml(tag: &str, inner: &str) -> String {
    let items: Vec<&str> = inner.split(',').map(str::trim).collect();
    let numeric = items.iter().all(|i| i.parse::<f64>().is_ok());
    let kind = if numeric { "numLit" } else { "strLit" };
    let mut out = format!("<c:{tag}><c:{kind}>");
    if numeric {
        out.push_str("<c:formatCode>General</c:formatCode>");
    }
    out.push_str(&format!("<c:ptCount val=\"{}\"/>", items.len()));
    for (i, item) in items.iter().enumerate() {
        let text = item.trim_matches('"').replace("\"\"", "\"");
        out.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", escape(&text)));
    }
    out.push_str(&format!("</c:{kind}></c:{tag}>"));
    out
}

fn name_xml(name: &str, filtered: bool) -> String {
    if filtered && !name.starts_with('"') {
        return format!("<c:tx>{}</c:tx>", data_xml("x", name, true, true).replace("<c:x>", "").replace("</c:x>", ""));
    }
    if let Some(text) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
        return format!("<c:tx><c:v>{}</c:v></c:tx>", escape(&text.replace("\"\"", "\"")));
    }
    format!("<c:tx><c:strRef><c:f>{}</c:f></c:strRef></c:tx>", escape(name))
}

/// One series, with its elements in the order its kind's schema gives them.
fn series_xml(s: &ChartSeries, kind: GroupKind, ids: (u32, u32), tag: &str, is_text: TextTest) -> String {
    let f = &s.fmt;
    let mut out = format!("<{tag}><c:idx val=\"{}\"/><c:order val=\"{}\"/>", ids.0, ids.1);
    if let Some(name) = &s.name {
        out.push_str(&name_xml(name, tag == "c15:ser"));
    }
    out.push_str(opt(&f.sp_pr));
    let (marker, xy) = match kind {
        GroupKind::Line | GroupKind::Stock | GroupKind::Radar => (true, false),
        GroupKind::Scatter => (true, true),
        GroupKind::Bubble => (false, true),
        _ => (false, false),
    };
    match kind {
        GroupKind::Bar | GroupKind::Bubble => {
            if let Some(v) = f.invert_if_negative {
                out.push_str(&bool_val("invertIfNegative", v));
            }
            if kind == GroupKind::Bar {
                out.push_str(opt(&f.picture_options));
            }
        }
        GroupKind::Area => out.push_str(opt(&f.picture_options)),
        GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie => {
            if let Some(e) = f.explosion {
                out.push_str(&num_val("explosion", e));
            }
        }
        _ => {}
    }
    if marker {
        if let Some(m) = &f.marker {
            out.push_str(&marker_xml(m));
        }
    }
    for p in &f.points {
        out.push_str(&point_xml(p));
    }
    if kind != GroupKind::Surface {
        if let Some(labels) = &f.labels {
            out.push_str(&labels_xml(labels));
        }
    }
    if !matches!(kind, GroupKind::Pie | GroupKind::Doughnut | GroupKind::OfPie | GroupKind::Radar | GroupKind::Surface) {
        for t in &f.trendlines {
            out.push_str(&trendline_xml(t));
        }
        for e in &f.error_bars {
            out.push_str(&error_bars_xml(e, kind));
        }
    }
    let text = s.categories.as_deref().is_some_and(is_text);
    let filtered = tag == "c15:ser";
    if xy {
        if let Some(x) = &s.categories {
            out.push_str(&data_xml("xVal", x, text, filtered));
        }
        out.push_str(&data_xml("yVal", &s.values, false, filtered));
        if let Some(sizes) = &s.sizes {
            out.push_str(&data_xml("bubbleSize", sizes, false, filtered));
        }
        if kind == GroupKind::Bubble {
            if let Some(b) = f.bubble_3d {
                out.push_str(&bool_val("bubble3D", b));
            }
        }
    } else {
        if let Some(c) = &s.categories {
            out.push_str(&data_xml("cat", c, text, filtered));
        }
        out.push_str(&data_xml("val", &s.values, false, filtered));
        if kind == GroupKind::Bar {
            if let Some(shape) = &f.shape {
                out.push_str(&format!("<c:shape val=\"{shape}\"/>"));
            }
        }
    }
    if matches!(kind, GroupKind::Line | GroupKind::Stock | GroupKind::Scatter) {
        if let Some(sm) = f.smooth {
            out.push_str(&bool_val("smooth", sm));
        }
    }
    out.push_str(opt(&f.ext));
    out.push_str(&format!("</{tag}>"));
    out
}

fn marker_xml(m: &Marker) -> String {
    let mut out = String::from("<c:marker>");
    if let Some(symbol) = &m.symbol {
        out.push_str(&format!("<c:symbol val=\"{symbol}\"/>"));
    }
    if let Some(size) = m.size {
        out.push_str(&num_val("size", size));
    }
    out.push_str(opt(&m.sp_pr));
    out.push_str("</c:marker>");
    out
}

fn point_xml(p: &PointFormat) -> String {
    let mut out = format!("<c:dPt><c:idx val=\"{}\"/>", p.index);
    if let Some(v) = p.invert_if_negative {
        out.push_str(&bool_val("invertIfNegative", v));
    }
    if let Some(m) = &p.marker {
        out.push_str(&marker_xml(m));
    }
    if let Some(v) = p.bubble_3d {
        out.push_str(&bool_val("bubble3D", v));
    }
    if let Some(e) = p.explosion {
        out.push_str(&num_val("explosion", e));
    }
    out.push_str(opt(&p.sp_pr));
    out.push_str(opt(&p.ext));
    out.push_str("</c:dPt>");
    out
}

fn trendline_xml(t: &Trendline) -> String {
    let mut out = String::from("<c:trendline>");
    if let Some(name) = &t.name {
        out.push_str(&format!("<c:name>{}</c:name>", escape(name)));
    }
    out.push_str(opt(&t.sp_pr));
    out.push_str(&format!("<c:trendlineType val=\"{}\"/>", t.kind.xml()));
    match t.kind {
        TrendKind::Polynomial(order) => out.push_str(&num_val("order", order)),
        TrendKind::MovingAverage(period) => out.push_str(&num_val("period", period)),
        _ => {}
    }
    if t.forward != 0.0 {
        out.push_str(&num_val("forward", f64_text(t.forward)));
    }
    if t.backward != 0.0 {
        out.push_str(&num_val("backward", f64_text(t.backward)));
    }
    if let Some(at) = t.intercept {
        out.push_str(&num_val("intercept", f64_text(at)));
    }
    out.push_str(&bool_val("dispRSqr", t.show_r_squared));
    out.push_str(&bool_val("dispEq", t.show_equation));
    out.push_str(opt(&t.label));
    out.push_str("</c:trendline>");
    out
}

fn error_source_xml(tag: &str, source: &ErrorSource) -> String {
    match source {
        ErrorSource::Range(f) => {
            format!("<c:{tag}><c:numRef><c:f>{}</c:f></c:numRef></c:{tag}>", escape(f))
        }
        ErrorSource::Numbers(numbers) => {
            let mut out = format!("<c:{tag}><c:numLit><c:formatCode>General</c:formatCode>");
            out.push_str(&format!("<c:ptCount val=\"{}\"/>", numbers.len()));
            for (i, n) in numbers.iter().enumerate() {
                out.push_str(&format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", f64_text(*n)));
            }
            out.push_str(&format!("</c:numLit></c:{tag}>"));
            out
        }
    }
}

fn error_bars_xml(e: &ErrorBars, kind: GroupKind) -> String {
    let mut out = String::from("<c:errBars>");
    let direction = e.direction.or(match kind {
        GroupKind::Line | GroupKind::Area | GroupKind::Stock => Some(ErrorDirection::Y),
        _ => None,
    });
    match direction {
        Some(ErrorDirection::X) => out.push_str("<c:errDir val=\"x\"/>"),
        Some(ErrorDirection::Y) => out.push_str("<c:errDir val=\"y\"/>"),
        None => {}
    }
    let sides = match e.sides {
        ErrorSides::Both => "both",
        ErrorSides::Plus => "plus",
        ErrorSides::Minus => "minus",
    };
    let amount = match e.amount {
        ErrorAmount::Fixed => "fixedVal",
        ErrorAmount::Percentage => "percentage",
        ErrorAmount::StandardDeviation => "stdDev",
        ErrorAmount::StandardError => "stdErr",
        ErrorAmount::Custom => "cust",
    };
    out.push_str(&format!("<c:errBarType val=\"{sides}\"/><c:errValType val=\"{amount}\"/>"));
    out.push_str(&bool_val("noEndCap", e.no_cap));
    if e.amount == ErrorAmount::Custom {
        if let Some(plus) = &e.plus {
            out.push_str(&error_source_xml("plus", plus));
        }
        if let Some(minus) = &e.minus {
            out.push_str(&error_source_xml("minus", minus));
        }
    } else if let Some(v) = e.value {
        out.push_str(&num_val("val", f64_text(v)));
    }
    out.push_str(opt(&e.sp_pr));
    out.push_str("</c:errBars>");
    out
}

fn label_fields_xml(f: &LabelFields) -> String {
    let mut out = String::new();
    if let Some((code, linked)) = &f.num_fmt {
        out.push_str(&format!(
            "<c:numFmt formatCode=\"{}\" sourceLinked=\"{}\"/>",
            escape(code),
            u8::from(*linked)
        ));
    }
    out.push_str(opt(&f.sp_pr));
    out.push_str(opt(&f.tx_pr));
    if let Some(p) = f.position {
        out.push_str(&format!("<c:dLblPos val=\"{}\"/>", p.xml()));
    }
    out.push_str(&bool_val("showLegendKey", f.show_legend_key));
    out.push_str(&bool_val("showVal", f.show_value));
    out.push_str(&bool_val("showCatName", f.show_category));
    out.push_str(&bool_val("showSerName", f.show_series));
    out.push_str(&bool_val("showPercent", f.show_percent));
    out.push_str(&bool_val("showBubbleSize", f.show_bubble_size));
    if let Some(sep) = &f.separator {
        out.push_str(&format!("<c:separator>{}</c:separator>", escape(sep)));
    }
    out
}

/// Data labels for a series or a group.
pub fn labels_xml(d: &DataLabels) -> String {
    let mut out = String::from("<c:dLbls>");
    for p in &d.points {
        out.push_str(&format!("<c:dLbl><c:idx val=\"{}\"/>", p.index));
        if p.delete {
            out.push_str("<c:delete val=\"1\"/>");
        } else {
            out.push_str(opt(&p.layout));
            out.push_str(opt(&p.text));
            out.push_str(&label_fields_xml(&p.fields));
        }
        out.push_str(opt(&p.ext));
        out.push_str("</c:dLbl>");
    }
    if d.delete {
        out.push_str("<c:delete val=\"1\"/>");
    } else {
        out.push_str(&label_fields_xml(&d.fields));
        if let Some(l) = d.show_leader_lines {
            out.push_str(&bool_val("showLeaderLines", l));
        }
        out.push_str(opt(&d.leader_lines));
    }
    out.push_str(opt(&d.ext));
    out.push_str("</c:dLbls>");
    out
}

fn gridlines_xml(tag: &str, g: &Option<Gridlines>) -> String {
    match g {
        None => String::new(),
        Some(g) if g.sp_pr.is_none() => format!("<c:{tag}/>"),
        Some(g) => format!("<c:{tag}>{}</c:{tag}>", opt(&g.sp_pr)),
    }
}

fn axis_xml(a: &Axis, spec: &ChartSpec) -> String {
    let tag = match a.kind {
        AxisKind::Category => "catAx",
        AxisKind::Value => "valAx",
        AxisKind::Date => "dateAx",
        AxisKind::Series => "serAx",
    };
    let mut out = format!("<c:{tag}><c:axId val=\"{}\"/><c:scaling>", a.id);
    if let Some(base) = a.log_base {
        out.push_str(&num_val("logBase", f64_text(base)));
    }
    out.push_str(&format!(
        "<c:orientation val=\"{}\"/>",
        if a.reverse { "maxMin" } else { "minMax" }
    ));
    if let Some(max) = a.max {
        out.push_str(&num_val("max", f64_text(max)));
    }
    if let Some(min) = a.min {
        out.push_str(&num_val("min", f64_text(min)));
    }
    out.push_str("</c:scaling>");
    out.push_str(&bool_val("delete", a.deleted));
    out.push_str(&format!("<c:axPos val=\"{}\"/>", a.position.xml()));
    out.push_str(&gridlines_xml("majorGridlines", &a.major_gridlines));
    out.push_str(&gridlines_xml("minorGridlines", &a.minor_gridlines));
    if let Some(title) = &a.title {
        out.push_str(&axis_title_xml(title));
    }
    if let Some((code, linked)) = &a.num_fmt {
        out.push_str(&format!(
            "<c:numFmt formatCode=\"{}\" sourceLinked=\"{}\"/>",
            escape(code),
            u8::from(*linked)
        ));
    }
    out.push_str(&format!("<c:majorTickMark val=\"{}\"/>", a.major_tick.xml()));
    out.push_str(&format!("<c:minorTickMark val=\"{}\"/>", a.minor_tick.xml()));
    out.push_str(&format!("<c:tickLblPos val=\"{}\"/>", a.tick_labels.xml()));
    out.push_str(opt(&a.sp_pr));
    out.push_str(opt(&a.tx_pr));
    out.push_str(&num_val("crossAx", a.cross_axis));
    match a.crosses {
        Crosses::Automatic => out.push_str("<c:crosses val=\"autoZero\"/>"),
        Crosses::Minimum => out.push_str("<c:crosses val=\"min\"/>"),
        Crosses::Maximum => out.push_str("<c:crosses val=\"max\"/>"),
        Crosses::At(at) => out.push_str(&num_val("crossesAt", f64_text(at))),
    }
    out.push_str(&axis_tail(a, spec));
    out.push_str(opt(&a.ext));
    out.push_str(&format!("</c:{tag}>"));
    out
}

/// The elements a kind of axis has after `crosses`.
fn axis_tail(a: &Axis, spec: &ChartSpec) -> String {
    let mut out = String::new();
    match a.kind {
        AxisKind::Value => {
            let between = a.between.unwrap_or_else(|| between_by_default(a, spec));
            out.push_str(&format!(
                "<c:crossBetween val=\"{}\"/>",
                if between { "between" } else { "midCat" }
            ));
            if let Some(u) = a.major_unit {
                out.push_str(&num_val("majorUnit", f64_text(u)));
            }
            if let Some(u) = a.minor_unit {
                out.push_str(&num_val("minorUnit", f64_text(u)));
            }
            out.push_str(opt(&a.display_units));
        }
        AxisKind::Category => {
            if let Some(v) = a.auto {
                out.push_str(&bool_val("auto", v));
            }
            if let Some(v) = &a.label_align {
                out.push_str(&format!("<c:lblAlgn val=\"{v}\"/>"));
            }
            if let Some(v) = a.label_offset {
                out.push_str(&num_val("lblOffset", v));
            }
            if let Some(v) = a.label_skip {
                out.push_str(&num_val("tickLblSkip", v));
            }
            if let Some(v) = a.mark_skip {
                out.push_str(&num_val("tickMarkSkip", v));
            }
            if let Some(v) = a.no_multi_level {
                out.push_str(&bool_val("noMultiLvlLbl", v));
            }
        }
        AxisKind::Date => {
            if let Some(v) = a.auto {
                out.push_str(&bool_val("auto", v));
            }
            if let Some(v) = a.label_offset {
                out.push_str(&num_val("lblOffset", v));
            }
            if let Some(u) = a.base_unit {
                out.push_str(&format!("<c:baseTimeUnit val=\"{}\"/>", u.xml()));
            }
            if let Some(u) = a.major_unit {
                out.push_str(&num_val("majorUnit", f64_text(u)));
            }
            if let Some(u) = a.major_time_unit {
                out.push_str(&format!("<c:majorTimeUnit val=\"{}\"/>", u.xml()));
            }
            if let Some(u) = a.minor_unit {
                out.push_str(&num_val("minorUnit", f64_text(u)));
            }
            if let Some(u) = a.minor_time_unit {
                out.push_str(&format!("<c:minorTimeUnit val=\"{}\"/>", u.xml()));
            }
        }
        AxisKind::Series => {
            if let Some(v) = a.label_skip {
                out.push_str(&num_val("tickLblSkip", v));
            }
            if let Some(v) = a.mark_skip {
                out.push_str(&num_val("tickMarkSkip", v));
            }
        }
    }
    out
}

/// Whether a value axis crosses between categories when nothing says: area and scatter charts cross at them.
fn between_by_default(a: &Axis, spec: &ChartSpec) -> bool {
    let kinds = spec.groups.iter().filter(|g| g.axes.contains(&a.id)).map(|g| g.kind);
    !kinds.into_iter().any(|k| matches!(k, GroupKind::Area | GroupKind::Scatter | GroupKind::Bubble))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xlsx::chart_read::parse;

    fn column_chart() -> Chart {
        let mut chart = Chart::default();
        chart.title = Some("Sales & costs".to_owned());
        chart.series.push(ChartSeries::new(
            Some("S!$B$1".to_owned()),
            Some("S!$A$2:$A$4".to_owned()),
            "S!$B$2:$B$4".to_owned(),
        ));
        chart.spec.groups.push(Group { axes: vec![1, 2], ..Group::default() });
        chart.spec.axes.push(Axis { id: 1, cross_axis: 2, ..Axis::default() });
        chart.spec.axes.push(Axis {
            id: 2,
            kind: AxisKind::Value,
            position: AxisPosition::Left,
            cross_axis: 1,
            major_gridlines: Some(Gridlines::default()),
            ..Axis::default()
        });
        chart
    }

    #[test]
    fn a_chart_written_here_reads_back_as_the_same_chart() {
        let mut chart = column_chart();
        chart.series[0].fmt.trendlines.push(Trendline {
            kind: TrendKind::Polynomial(3),
            show_equation: true,
            forward: 2.0,
            ..Trendline::default()
        });
        chart.series[0].fmt.error_bars.push(ErrorBars::default());
        chart.spec.legend = Some(Legend::default());
        chart.spec.data_table = Some(DataTable::default());
        chart.spec.axes[1].title = Some(AxisTitle { text: "Units".to_owned(), ..AxisTitle::default() });
        chart.spec.axes[1].log_base = Some(10.0);
        chart.spec.axes[1].min = Some(1.0);
        let xml = chart_xml(&chart, &|_| true);
        let back = parse(&xml).expect("the chart reads back");
        assert_eq!(back.title, chart.title);
        assert_eq!(back.series[0].fmt.trendlines, chart.series[0].fmt.trendlines);
        assert_eq!(back.series[0].fmt.error_bars.len(), 1);
        assert_eq!(back.spec.groups[0].axes, vec![1, 2]);
        assert_eq!(back.spec.axis(2).unwrap().log_base, Some(10.0));
        assert_eq!(back.spec.axis(2).unwrap().title.as_ref().unwrap().text, "Units");
        assert_eq!(back.spec.data_table, chart.spec.data_table);
        assert_eq!(back.spec.legend, chart.spec.legend);
    }
}
