//! Reading a `chart` part into the model.
//!
//! Everything the model names is read into [`ChartSpec`] and the series; everything else inside the
//! elements it does read is kept as the XML it was, so [`super::chart_write`] can put it back.

use roxmltree::{Document, Node};

use super::raw;
use crate::chartspec::*;
use crate::model::ChartSeries;

/// A chart part read: its title, series and everything else.
pub struct Parsed {
    pub title: Option<String>,
    pub series: Vec<ChartSeries>,
    pub spec: ChartSpec,
}

/// The first child element with a local name.
pub(super) fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// The child elements with a local name.
pub(super) fn children<'a, 'b>(n: Node<'a, 'b>, name: &str) -> std::vec::IntoIter<Node<'a, 'b>> {
    let all: Vec<Node<'a, 'b>> =
        n.children().filter(|c| c.is_element() && c.tag_name().name() == name).collect();
    all.into_iter()
}

/// The `val` attribute of a child element.
pub(super) fn val<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.attribute("val"))
}

/// A `CT_Boolean` child: present with no value means true.
pub(super) fn flag(n: Node, name: &str) -> Option<bool> {
    child(n, name).map(|c| !matches!(c.attribute("val"), Some("0") | Some("false")))
}

fn number<T: std::str::FromStr>(n: Node, name: &str) -> Option<T> {
    val(n, name).and_then(|v| v.trim().parse().ok())
}

/// The XML of a child element as it was written.
fn raw_child(xml: &str, n: Node, name: &str) -> Option<String> {
    child(n, name).map(|c| raw(xml, c).to_owned())
}

/// The words of a rich text block: paragraphs joined by line breaks.
pub(super) fn rich_text(n: Node) -> String {
    let mut paragraphs = Vec::new();
    for p in n.descendants().filter(|d| d.is_element() && d.tag_name().name() == "p") {
        let mut line = String::new();
        for t in p.descendants().filter(|d| d.is_element()) {
            match t.tag_name().name() {
                "t" => line.push_str(t.text().unwrap_or("")),
                "br" => line.push('\n'),
                _ => {}
            }
        }
        paragraphs.push(line);
    }
    paragraphs.join("\n")
}

/// The formula or the words of a `c:tx` element: `=formula` for a reference.
fn tx_text(tx: Node) -> String {
    if let Some(f) = tx.descendants().find(|d| d.is_element() && d.tag_name().name() == "f") {
        return format!("={}", f.text().unwrap_or("").trim());
    }
    match child(tx, "rich") {
        Some(rich) => rich_text(rich),
        None => String::new(),
    }
}

/// A title element: its words and its formatting.
fn read_title(xml: &str, title: Node) -> (String, TitleFormat) {
    let text = child(title, "tx").map(tx_text).unwrap_or_default();
    let format = TitleFormat {
        rich: raw_child(xml, title, "tx"),
        layout: raw_child(xml, title, "layout"),
        overlay: flag(title, "overlay").unwrap_or(false),
        sp_pr: raw_child(xml, title, "spPr"),
        tx_pr: raw_child(xml, title, "txPr"),
    };
    (text, format)
}

/// Quote words as a text formula.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// A series' formula from `tx`, `cat`, `val`, `xVal`, `yVal` or `bubbleSize`.
///
/// A reference gives its formula; numbers or words written into the chart give an array constant.
fn formula_in(ser: Node, part: &str) -> Option<String> {
    let holder = child(ser, part)?;
    let reference = holder
        .descendants()
        .find(|d| d.is_element() && matches!(d.tag_name().name(), "f" | "sqref"));
    if let Some(f) = reference {
        return f.text().map(|t| t.trim().to_owned());
    }
    if part == "tx" {
        return holder
            .descendants()
            .find(|d| d.is_element() && d.tag_name().name() == "v")
            .and_then(|v| v.text())
            .map(quoted);
    }
    let lit = holder.children().find(|c| {
        c.is_element() && matches!(c.tag_name().name(), "numLit" | "strLit" | "multiLvlStrRef")
    })?;
    let numeric = lit.tag_name().name() == "numLit";
    let items: Vec<String> = children(lit, "pt")
        .filter_map(|pt| child(pt, "v").and_then(|v| v.text()).map(str::to_owned))
        .map(|t| if numeric { t } else { quoted(&t) })
        .collect();
    (!items.is_empty()).then(|| format!("{{{}}}", items.join(",")))
}

/// Read one chart part.
pub fn parse(xml: &str) -> Option<Parsed> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "chartSpace" {
        return None;
    }
    let chart = child(root, "chart")?;
    let plot = child(chart, "plotArea")?;
    let mut spec = ChartSpec::default();
    let mut series = Vec::new();
    read_space(xml, root, &mut spec);
    let title = read_chart_level(xml, chart, &mut spec);
    read_plot(xml, plot, &mut spec, &mut series);
    Some(Parsed { title, series, spec })
}

/// The chart space's own children.
fn read_space(xml: &str, root: Node, spec: &mut ChartSpec) {
    let raw_part = &mut spec.raw;
    raw_part.date1904 = raw_child(xml, root, "date1904");
    raw_part.lang = raw_child(xml, root, "lang");
    raw_part.clr_map_ovr = raw_child(xml, root, "clrMapOvr");
    raw_part.pivot_source = raw_child(xml, root, "pivotSource");
    raw_part.protection = raw_child(xml, root, "protection");
    raw_part.sp_pr = raw_child(xml, root, "spPr");
    raw_part.tx_pr = raw_child(xml, root, "txPr");
    raw_part.external_data = raw_child(xml, root, "externalData");
    raw_part.print_settings = raw_child(xml, root, "printSettings");
    raw_part.user_shapes = raw_child(xml, root, "userShapes");
    raw_part.ext = raw_child(xml, root, "extLst");
    raw_part.namespaces = super::namespaces_of(root).into_iter().collect();
    raw_part.ignorable = root
        .attributes()
        .find(|a| a.name() == "Ignorable")
        .map(|a| a.value().to_owned());
    spec.pivot = raw_part.pivot_source.is_some();
    spec.rounded_corners = flag(root, "roundedCorners");
    spec.style = read_style(root);
}

/// The chart style number: `c:style`, or Office 2010's `c14:style` which is 100 more.
fn read_style(root: Node) -> Option<u32> {
    let mut modern = None;
    for d in root.descendants().filter(|d| d.is_element() && d.tag_name().name() == "style") {
        let value: Option<u32> = d.attribute("val").and_then(|v| v.parse().ok());
        if d.tag_name().namespace() == Some(super::CHART_NS) {
            return value;
        }
        modern = value.map(|v| v.saturating_sub(100));
    }
    modern
}

/// The children of `c:chart` other than the plot area; the title's words are returned.
fn read_chart_level(xml: &str, chart: Node, spec: &mut ChartSpec) -> Option<String> {
    let mut title_text = None;
    if let Some(title) = child(chart, "title") {
        let (text, format) = read_title(xml, title);
        spec.title = format;
        spec.title_auto = child(title, "tx").is_none();
        title_text = Some(text).filter(|t| !t.is_empty());
    }
    spec.title_deleted = flag(chart, "autoTitleDeleted").unwrap_or(false);
    spec.raw.pivot_formats = raw_child(xml, chart, "pivotFmts");
    spec.raw.view_3d = raw_child(xml, chart, "view3D");
    spec.raw.floor = raw_child(xml, chart, "floor");
    spec.raw.side_wall = raw_child(xml, chart, "sideWall");
    spec.raw.back_wall = raw_child(xml, chart, "backWall");
    spec.raw.chart_ext = raw_child(xml, chart, "extLst");
    spec.legend = child(chart, "legend").map(|l| read_legend(xml, l));
    spec.plot_visible_only = flag(chart, "plotVisOnly");
    spec.blanks = val(chart, "dispBlanksAs").map(Blanks::from_xml);
    spec.labels_over_max = flag(chart, "showDLblsOverMax");
    title_text
}

fn read_legend(xml: &str, legend: Node) -> Legend {
    Legend {
        position: val(legend, "legendPos").map(LegendPosition::from_xml).unwrap_or_default(),
        overlay: flag(legend, "overlay").unwrap_or(false),
        deleted_entries: children(legend, "legendEntry")
            .filter(|e| flag(*e, "delete").unwrap_or(false))
            .filter_map(|e| number(e, "idx"))
            .collect(),
        layout: raw_child(xml, legend, "layout"),
        sp_pr: raw_child(xml, legend, "spPr"),
        tx_pr: raw_child(xml, legend, "txPr"),
    }
}

/// The plot area: chart groups, axes and the data table.
fn read_plot(xml: &str, plot: Node, spec: &mut ChartSpec, series: &mut Vec<ChartSeries>) {
    spec.raw.plot_layout = raw_child(xml, plot, "layout");
    spec.raw.plot_sp_pr = raw_child(xml, plot, "spPr");
    for node in plot.children().filter(Node::is_element) {
        let name = node.tag_name().name();
        if let Some((kind, three_d)) = GroupKind::from_element(name) {
            let group_index = spec.groups.len();
            let group = read_group(xml, node, kind, three_d, group_index, series);
            spec.groups.push(group);
            continue;
        }
        match name {
            "catAx" | "valAx" | "dateAx" | "serAx" => spec.axes.push(read_axis(xml, node)),
            "dTable" => spec.data_table = Some(read_data_table(xml, node)),
            _ => {}
        }
    }
}

fn read_data_table(xml: &str, node: Node) -> DataTable {
    DataTable {
        horizontal_borders: flag(node, "showHorzBorder").unwrap_or(false),
        vertical_borders: flag(node, "showVertBorder").unwrap_or(false),
        outline: flag(node, "showOutline").unwrap_or(false),
        legend_keys: flag(node, "showKeys").unwrap_or(false),
        sp_pr: raw_child(xml, node, "spPr"),
        tx_pr: raw_child(xml, node, "txPr"),
    }
}

fn read_group(
    xml: &str,
    node: Node,
    kind: GroupKind,
    three_d: bool,
    index: usize,
    series: &mut Vec<ChartSeries>,
) -> Group {
    let mut g = Group { kind, three_d, grouping: Grouping::Standard, ..Group::default() };
    g.horizontal = val(node, "barDir") == Some("bar");
    g.grouping = match val(node, "grouping") {
        Some(g) => Grouping::from_xml(g),
        None if kind == GroupKind::Bar => Grouping::Clustered,
        None => Grouping::Standard,
    };
    g.vary_colors = flag(node, "varyColors");
    g.gap_width = number(node, "gapWidth");
    g.overlap = number(node, "overlap");
    g.gap_depth = number(node, "gapDepth");
    g.shape = val(node, "shape").map(str::to_owned);
    g.series_lines = child(node, "serLines").is_some();
    g.drop_lines = child(node, "dropLines").is_some();
    g.hi_low_lines = child(node, "hiLowLines").is_some();
    g.up_down_bars = child(node, "upDownBars").map(|u| number(u, "gapWidth").unwrap_or(150));
    g.marker = flag(node, "marker");
    g.smooth = flag(node, "smooth");
    g.scatter_style = val(node, "scatterStyle").map(str::to_owned);
    g.radar_style = val(node, "radarStyle").map(str::to_owned);
    g.wireframe = flag(node, "wireframe").unwrap_or(false);
    g.bubble_scale = number(node, "bubbleScale");
    g.show_negative_bubbles = flag(node, "showNegBubbles");
    g.size_represents = val(node, "sizeRepresents").map(str::to_owned);
    g.bubble_3d = flag(node, "bubble3D").unwrap_or(false)
        || children(node, "ser").any(|s| flag(s, "bubble3D") == Some(true));
    g.of_pie = if val(node, "ofPieType") == Some("bar") { OfPieKind::Bar } else { OfPieKind::Pie };
    g.split_by = match val(node, "splitType") {
        Some("val") => SplitBy::Value,
        Some("percent") => SplitBy::Percent,
        Some("cust") => SplitBy::Custom,
        Some("pos") => SplitBy::Position,
        _ => SplitBy::Auto,
    };
    g.split_at = number(node, "splitPos");
    g.second_size = number(node, "secondPieSize");
    g.custom_split = raw_child(xml, node, "custSplit");
    g.first_slice_angle = number(node, "firstSliceAng");
    g.hole_size = number(node, "holeSize");
    g.labels = child(node, "dLbls").map(|d| read_labels(xml, d));
    g.raw_lines = [
        raw_child(xml, node, "dropLines"),
        raw_child(xml, node, "hiLowLines"),
        raw_child(xml, node, "upDownBars"),
        raw_child(xml, node, "serLines"),
    ];
    g.band_formats = raw_child(xml, node, "bandFmts");
    g.axes = children(node, "axId").filter_map(|a| a.attribute("val")?.parse().ok()).collect();
    let (ext, filtered) = split_filtered(xml, node);
    g.ext = ext;
    for ser in children(node, "ser") {
        let mut s = read_series(xml, ser, kind);
        s.group = index;
        series.push(s);
    }
    for ser in filtered {
        let mut s = read_series(xml, ser, kind);
        s.group = index;
        s.fmt.filtered = true;
        series.push(s);
    }
    g
}

/// A group's extension list without the series Excel filtered out of the chart, and those series.
///
/// Excel keeps a filtered series in a `c15:filtered...Series` extension element of its group; it is written
/// again from the model, so the rest of the list is what is kept.
fn split_filtered<'a, 'b>(xml: &str, group: Node<'a, 'b>) -> (Option<String>, Vec<Node<'a, 'b>>) {
    let Some(list) = child(group, "extLst") else { return (None, Vec::new()) };
    let mut kept = String::new();
    let mut filtered = Vec::new();
    for ext in children(list, "ext") {
        let holders: Vec<Node> = ext
            .children()
            .filter(|c| c.is_element() && c.tag_name().name().starts_with("filtered") && c.tag_name().name().ends_with("Series"))
            .collect();
        if holders.is_empty() {
            kept.push_str(raw(xml, ext));
        } else {
            for holder in holders {
                filtered.extend(children(holder, "ser"));
            }
        }
    }
    ((!kept.is_empty()).then_some(kept), filtered)
}

fn read_series(xml: &str, ser: Node, kind: GroupKind) -> ChartSeries {
    let scatter = matches!(kind, GroupKind::Scatter | GroupKind::Bubble);
    let values = formula_in(ser, "val").or_else(|| formula_in(ser, "yVal")).unwrap_or_default();
    let categories = formula_in(ser, "cat").or_else(|| formula_in(ser, "xVal"));
    let mut fmt = SeriesFormat {
        idx: number(ser, "idx"),
        order: number(ser, "order"),
        sp_pr: raw_child(xml, ser, "spPr"),
        marker: child(ser, "marker").map(|m| read_marker(xml, m)),
        labels: child(ser, "dLbls").map(|d| read_labels(xml, d)),
        smooth: flag(ser, "smooth"),
        explosion: number(ser, "explosion"),
        invert_if_negative: flag(ser, "invertIfNegative"),
        bubble_3d: flag(ser, "bubble3D"),
        shape: val(ser, "shape").map(str::to_owned),
        picture_options: raw_child(xml, ser, "pictureOptions"),
        ..SeriesFormat::default()
    };
    let (ext, hidden_cells) = split_category_filter(xml, ser);
    fmt.ext = ext;
    fmt.filtered_categories = hidden_cells.iter().filter_map(|c| category_position(categories.as_deref(), c)).collect();
    fmt.points = children(ser, "dPt").map(|p| read_point(xml, p)).collect();
    fmt.trendlines = children(ser, "trendline").map(|t| read_trendline(xml, t)).collect();
    fmt.error_bars = children(ser, "errBars").map(|e| read_error_bars(xml, e, scatter)).collect();
    ChartSeries {
        name: formula_in(ser, "tx"),
        categories,
        values,
        sizes: formula_in(ser, "bubbleSize"),
        group: 0,
        fmt,
    }
}

/// A series' extension list without the categories Excel filtered out of it, and the cells of those.
///
/// Excel keeps the filtered categories of a series as `c15:categoryFilterException` elements, each naming
/// the cell of one category; they are written again from the model.
fn split_category_filter(xml: &str, ser: Node) -> (Option<String>, Vec<String>) {
    let Some(list) = child(ser, "extLst") else { return (None, Vec::new()) };
    let mut kept = String::new();
    let mut cells = Vec::new();
    for ext in children(list, "ext") {
        let holder = ext.children().find(|c| c.is_element() && c.tag_name().name() == "categoryFilterExceptions");
        match holder {
            Some(h) => cells.extend(
                h.children()
                    .filter(|c| c.is_element())
                    .filter_map(|e| e.children().find(|c| c.is_element() && c.tag_name().name() == "sqref"))
                    .filter_map(|n| n.text().map(str::to_owned)),
            ),
            None => kept.push_str(raw(xml, ext)),
        }
    }
    ((!kept.is_empty()).then(|| format!("<c:extLst>{kept}</c:extLst>")), cells)
}

/// Where a cell falls in a categories formula that is one row or one column: its position from the start.
fn category_position(categories: Option<&str>, cell: &str) -> Option<u32> {
    let categories = categories?;
    let area = categories.rsplit_once('!').map_or(categories, |(_, a)| a).replace('$', "");
    let range = crate::coord::Range::parse(&area)?;
    let at = crate::coord::CellRef::parse(cell.rsplit_once('!').map_or(cell, |(_, c)| c))?;
    if !range.contains(at) {
        return None;
    }
    Some(if range.rows() > 1 { at.row - range.first.row } else { at.col - range.first.col })
}

fn read_marker(xml: &str, m: Node) -> Marker {
    Marker {
        symbol: val(m, "symbol").map(str::to_owned),
        size: number(m, "size"),
        sp_pr: raw_child(xml, m, "spPr"),
    }
}

fn read_point(xml: &str, p: Node) -> PointFormat {
    PointFormat {
        index: number(p, "idx").unwrap_or(0),
        invert_if_negative: flag(p, "invertIfNegative"),
        marker: child(p, "marker").map(|m| read_marker(xml, m)),
        bubble_3d: flag(p, "bubble3D"),
        explosion: number(p, "explosion"),
        sp_pr: raw_child(xml, p, "spPr"),
        ext: raw_child(xml, p, "extLst"),
    }
}

fn read_trendline(xml: &str, t: Node) -> Trendline {
    let order: u32 = number(t, "order").unwrap_or(2);
    let period: u32 = number(t, "period").unwrap_or(2);
    let kind = match val(t, "trendlineType") {
        Some("exp") => TrendKind::Exponential,
        Some("log") => TrendKind::Logarithmic,
        Some("poly") => TrendKind::Polynomial(order),
        Some("power") => TrendKind::Power,
        Some("movingAvg") => TrendKind::MovingAverage(period),
        _ => TrendKind::Linear,
    };
    Trendline {
        name: child(t, "name").and_then(|n| n.text()).map(str::to_owned),
        kind,
        forward: number(t, "forward").unwrap_or(0.0),
        backward: number(t, "backward").unwrap_or(0.0),
        intercept: number(t, "intercept"),
        show_r_squared: flag(t, "dispRSqr").unwrap_or(false),
        show_equation: flag(t, "dispEq").unwrap_or(false),
        sp_pr: raw_child(xml, t, "spPr"),
        label: raw_child(xml, t, "trendlineLbl"),
    }
}

fn read_error_source(n: Node) -> Option<ErrorSource> {
    if let Some(f) = n.descendants().find(|d| d.is_element() && d.tag_name().name() == "f") {
        return Some(ErrorSource::Range(f.text().unwrap_or("").trim().to_owned()));
    }
    let lit = child(n, "numLit")?;
    let numbers = children(lit, "pt")
        .filter_map(|pt| child(pt, "v")?.text()?.trim().parse().ok())
        .collect();
    Some(ErrorSource::Numbers(numbers))
}

fn read_error_bars(xml: &str, e: Node, _scatter: bool) -> ErrorBars {
    ErrorBars {
        direction: match val(e, "errDir") {
            Some("x") => Some(ErrorDirection::X),
            Some("y") => Some(ErrorDirection::Y),
            _ => None,
        },
        sides: match val(e, "errBarType") {
            Some("plus") => ErrorSides::Plus,
            Some("minus") => ErrorSides::Minus,
            _ => ErrorSides::Both,
        },
        amount: match val(e, "errValType") {
            Some("percentage") => ErrorAmount::Percentage,
            Some("stdDev") => ErrorAmount::StandardDeviation,
            Some("stdErr") => ErrorAmount::StandardError,
            Some("cust") => ErrorAmount::Custom,
            _ => ErrorAmount::Fixed,
        },
        no_cap: flag(e, "noEndCap").unwrap_or(false),
        value: number(e, "val"),
        plus: child(e, "plus").and_then(read_error_source),
        minus: child(e, "minus").and_then(read_error_source),
        sp_pr: raw_child(xml, e, "spPr"),
    }
}

fn read_label_fields(xml: &str, n: Node) -> LabelFields {
    LabelFields {
        num_fmt: child(n, "numFmt").map(|f| {
            (
                f.attribute("formatCode").unwrap_or("General").to_owned(),
                !matches!(f.attribute("sourceLinked"), Some("0") | Some("false")),
            )
        }),
        sp_pr: raw_child(xml, n, "spPr"),
        tx_pr: raw_child(xml, n, "txPr"),
        position: val(n, "dLblPos").and_then(LabelPosition::from_xml),
        show_legend_key: flag(n, "showLegendKey").unwrap_or(false),
        show_value: flag(n, "showVal").unwrap_or(false),
        show_category: flag(n, "showCatName").unwrap_or(false),
        show_series: flag(n, "showSerName").unwrap_or(false),
        show_percent: flag(n, "showPercent").unwrap_or(false),
        show_bubble_size: flag(n, "showBubbleSize").unwrap_or(false),
        separator: child(n, "separator").map(|s| s.text().unwrap_or("").to_owned()),
    }
}

fn read_labels(xml: &str, d: Node) -> DataLabels {
    let points = children(d, "dLbl")
        .map(|l| PointLabel {
            index: number(l, "idx").unwrap_or(0),
            delete: flag(l, "delete").unwrap_or(false),
            layout: raw_child(xml, l, "layout"),
            text: raw_child(xml, l, "tx"),
            fields: read_label_fields(xml, l),
            ext: raw_child(xml, l, "extLst"),
        })
        .collect();
    DataLabels {
        points,
        delete: flag(d, "delete").unwrap_or(false),
        fields: read_label_fields(xml, d),
        show_leader_lines: flag(d, "showLeaderLines"),
        leader_lines: raw_child(xml, d, "leaderLines"),
        ext: raw_child(xml, d, "extLst"),
    }
}

fn read_gridlines(xml: &str, n: Node) -> Gridlines {
    Gridlines { sp_pr: raw_child(xml, n, "spPr") }
}

fn read_axis(xml: &str, node: Node) -> Axis {
    let kind = match node.tag_name().name() {
        "valAx" => AxisKind::Value,
        "dateAx" => AxisKind::Date,
        "serAx" => AxisKind::Series,
        _ => AxisKind::Category,
    };
    let mut a = Axis { kind, ..Axis::default() };
    a.id = number(node, "axId").unwrap_or(0);
    if let Some(scaling) = child(node, "scaling") {
        a.min = number(scaling, "min");
        a.max = number(scaling, "max");
        a.log_base = number(scaling, "logBase");
        a.reverse = val(scaling, "orientation") == Some("maxMin");
    }
    a.deleted = flag(node, "delete").unwrap_or(false);
    a.position = val(node, "axPos").map(AxisPosition::from_xml).unwrap_or_default();
    a.major_gridlines = child(node, "majorGridlines").map(|g| read_gridlines(xml, g));
    a.minor_gridlines = child(node, "minorGridlines").map(|g| read_gridlines(xml, g));
    a.title = child(node, "title").map(|t| {
        let (text, format) = read_title(xml, t);
        AxisTitle { text, format }
    });
    a.num_fmt = child(node, "numFmt").map(|f| {
        (
            f.attribute("formatCode").unwrap_or("General").to_owned(),
            !matches!(f.attribute("sourceLinked"), Some("0") | Some("false")),
        )
    });
    a.major_tick = val(node, "majorTickMark").map(TickMark::from_xml).unwrap_or(TickMark::Outside);
    a.minor_tick = val(node, "minorTickMark").map(TickMark::from_xml).unwrap_or(TickMark::Outside);
    a.tick_labels = val(node, "tickLblPos").map(TickLabels::from_xml).unwrap_or_default();
    a.sp_pr = raw_child(xml, node, "spPr");
    a.tx_pr = raw_child(xml, node, "txPr");
    a.cross_axis = number(node, "crossAx").unwrap_or(0);
    a.crosses = match (val(node, "crosses"), number::<f64>(node, "crossesAt")) {
        (_, Some(at)) => Crosses::At(at),
        (Some("min"), _) => Crosses::Minimum,
        (Some("max"), _) => Crosses::Maximum,
        _ => Crosses::Automatic,
    };
    a.between = val(node, "crossBetween").map(|v| v == "between");
    a.major_unit = number(node, "majorUnit");
    a.minor_unit = number(node, "minorUnit");
    a.display_units = raw_child(xml, node, "dispUnits");
    a.auto = flag(node, "auto");
    a.label_align = val(node, "lblAlgn").map(str::to_owned);
    a.label_offset = number(node, "lblOffset");
    a.label_skip = number(node, "tickLblSkip");
    a.mark_skip = number(node, "tickMarkSkip");
    a.no_multi_level = flag(node, "noMultiLvlLbl");
    a.base_unit = val(node, "baseTimeUnit").and_then(TimeUnit::from_xml);
    a.major_time_unit = val(node, "majorTimeUnit").and_then(TimeUnit::from_xml);
    a.minor_time_unit = val(node, "minorTimeUnit").and_then(TimeUnit::from_xml);
    a.ext = raw_child(xml, node, "extLst");
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:roundedCorners val="0"/><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Sales</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="stacked"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:strRef><c:f>S!$B$1</c:f></c:strRef></c:tx><c:trendline><c:trendlineType val="poly"/><c:order val="3"/><c:dispEq val="1"/></c:trendline><c:cat><c:strRef><c:f>S!$A$2:$A$4</c:f></c:strRef></c:cat><c:val><c:numRef><c:f>S!$B$2:$B$4</c:f></c:numRef></c:val></c:ser><c:gapWidth val="50"/><c:overlap val="100"/><c:axId val="1"/><c:axId val="2"/></c:barChart><c:catAx><c:axId val="1"/><c:scaling><c:orientation val="maxMin"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:logBase val="10"/><c:orientation val="minMax"/><c:max val="1000"/><c:min val="1"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:majorGridlines/><c:crossAx val="1"/><c:crossesAt val="5"/><c:majorUnit val="10"/></c:valAx></c:plotArea><c:legend><c:legendPos val="t"/></c:legend><c:plotVisOnly val="1"/><c:dispBlanksAs val="span"/></c:chart></c:chartSpace>"#;

    #[test]
    fn a_stacked_column_chart_reads_into_the_model() {
        let p = parse(SAMPLE).unwrap();
        assert_eq!(p.title.as_deref(), Some("Sales"));
        assert_eq!(p.series.len(), 1);
        assert_eq!(p.series[0].values, "S!$B$2:$B$4");
        assert_eq!(p.series[0].fmt.trendlines[0].kind, TrendKind::Polynomial(3));
        assert!(p.series[0].fmt.trendlines[0].show_equation);
        let g = &p.spec.groups[0];
        assert_eq!(g.grouping, Grouping::Stacked);
        assert_eq!(g.overlap, Some(100));
        assert_eq!(p.spec.chart_type(), ChartType::ColumnStacked);
        assert_eq!(p.spec.blanks_as(), Blanks::Span);
        assert_eq!(p.spec.legend.as_ref().unwrap().position, LegendPosition::Top);
        let value = p.spec.axis(2).unwrap();
        assert_eq!((value.min, value.max, value.log_base), (Some(1.0), Some(1000.0), Some(10.0)));
        assert_eq!(value.crosses, Crosses::At(5.0));
        assert!(p.spec.axis(1).unwrap().reverse);
    }
}
