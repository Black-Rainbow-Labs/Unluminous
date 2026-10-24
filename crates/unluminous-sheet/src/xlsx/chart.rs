//! Charts: reading the ones a file has, so they can be drawn, and writing the ones made in Unluminous.
//!
//! **A chart read from a file is kept exactly as it was.** Its drawing and chart parts are written back as
//! the bytes they were, so everything Excel put in them, the colours, the fonts, the trendlines, the data
//! labels, survives. What is read here is only enough to draw it: where it sits, its kind, its title and
//! the formulas of its series.
//!
//! **A chart made here is written as new parts** in Excel's own default shape: no explicit colours, so
//! Excel draws it in the workbook theme's accents, as it draws a chart a person inserts.

use roxmltree::{Document, Node};

use super::{escape, parse_rels, rels_path, resolve};
use crate::coord::CellRef;
use crate::model::{Chart, ChartKind, ChartSeries, Picture};

/// English Metric Units in one pixel at 96 dots an inch.
pub const EMU_PER_PIXEL: i64 = 9525;

fn first<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.descendants().find(|d| d.is_element() && d.tag_name().name() == name)
}

fn number(n: Option<Node>) -> i64 {
    n.and_then(|n| n.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

/// Where an anchor's corner is: a cell and offsets into it in EMUs.
fn corner(n: Node) -> (CellRef, i64, i64) {
    let col = number(first(n, "col")) as u32;
    let row = number(first(n, "row")) as u32;
    (CellRef::new(row, col), number(first(n, "colOff")), number(first(n, "rowOff")))
}

/// The kind a chart part's plot area holds first.
fn kind_of(plot: Node) -> ChartKind {
    for child in plot.children().filter(Node::is_element) {
        let name = child.tag_name().name();
        let kind = match name {
            "barChart" | "bar3DChart" => {
                let horizontal =
                    first(child, "barDir").and_then(|d| d.attribute("val")) == Some("bar");
                if horizontal {
                    ChartKind::Bar
                } else {
                    ChartKind::Column
                }
            }
            "lineChart" | "line3DChart" | "stockChart" => ChartKind::Line,
            "areaChart" | "area3DChart" => ChartKind::Area,
            "pieChart" | "pie3DChart" | "ofPieChart" => ChartKind::Pie,
            "doughnutChart" => ChartKind::Doughnut,
            "scatterChart" | "bubbleChart" => ChartKind::Scatter,
            "radarChart" => ChartKind::Other("radar"),
            "surfaceChart" | "surface3DChart" => ChartKind::Other("surface"),
            _ => continue,
        };
        return kind;
    }
    ChartKind::Other("chart")
}

/// The text of a rich text block: its `a:t` runs joined.
fn rich_text(n: Node) -> String {
    let mut out = String::new();
    for p in n.descendants().filter(|d| d.is_element() && d.tag_name().name() == "p") {
        if !out.is_empty() {
            out.push(' ');
        }
        for t in p.descendants().filter(|d| d.is_element() && d.tag_name().name() == "t") {
            out.push_str(t.text().unwrap_or(""));
        }
    }
    out
}

/// A series' formula from one of its parts: `tx`, `cat`, `val`, `xVal` or `yVal`.
fn formula_in(ser: Node, part: &str) -> Option<String> {
    let holder = ser.children().find(|c| c.is_element() && c.tag_name().name() == part)?;
    if let Some(f) = first(holder, "f") {
        return f.text().map(|t| t.trim().to_owned());
    }
    // A name written in place, `<c:tx><c:v>Sales</c:v></c:tx>`, as a text formula.
    first(holder, "v").and_then(|v| v.text()).map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
}

/// Read one chart part.
pub fn read_chart(xml: &str) -> Option<(ChartKind, Option<String>, Vec<ChartSeries>)> {
    let doc = Document::parse(xml).ok()?;
    let chart = first(doc.root_element(), "chart")?;
    let plot = first(chart, "plotArea")?;
    let kind = kind_of(plot);
    let title = chart
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "title")
        .map(|t| match first(t, "f") {
            Some(f) => format!("={}", f.text().unwrap_or("")),
            None => rich_text(t),
        })
        .filter(|t| !t.is_empty());
    let mut series = Vec::new();
    for ser in plot.descendants().filter(|d| d.is_element() && d.tag_name().name() == "ser") {
        let values = formula_in(ser, "val").or_else(|| formula_in(ser, "yVal"));
        let Some(values) = values else { continue };
        series.push(ChartSeries {
            name: formula_in(ser, "tx"),
            categories: formula_in(ser, "cat").or_else(|| formula_in(ser, "xVal")),
            values,
        });
    }
    Some((kind, title, series))
}

/// The charts a sheet's drawing holds, read for drawing. `text` reads a part of the package.
pub fn read_charts(
    sheet_part: &str,
    drawing_target: &str,
    text: &dyn Fn(&str) -> Option<String>,
) -> Vec<Chart> {
    let drawing_part = resolve(sheet_part, drawing_target);
    let Some(xml) = text(&drawing_part) else { return Vec::new() };
    let Ok(doc) = Document::parse(&xml) else { return Vec::new() };
    let rels = parse_rels(&text(&rels_path(&drawing_part)).unwrap_or_default());
    let mut charts = Vec::new();
    for anchor in doc.root_element().children().filter(Node::is_element) {
        let name = anchor.tag_name().name();
        if name != "twoCellAnchor" && name != "oneCellAnchor" && name != "absoluteAnchor" {
            continue;
        }
        let Some(reference) = anchor.descendants().find(|d| {
            d.is_element()
                && d.tag_name().name() == "chart"
                && d.tag_name().namespace() == Some(super::CHART_NS)
        }) else {
            continue;
        };
        let Some(id) =
            reference.attributes().find(|a| a.name() == "id").map(|a| a.value().to_owned())
        else {
            continue;
        };
        let Some(rel) = rels.iter().find(|r| r.id == id) else { continue };
        let chart_part = resolve(&drawing_part, &rel.target);
        let Some(chart_xml) = text(&chart_part) else { continue };
        let Some((kind, title, series)) = read_chart(&chart_xml) else { continue };
        let from = first(anchor, "from").map_or((CellRef::new(0, 0), 0, 0), corner);
        let to = match first(anchor, "to") {
            Some(to) => corner(to),
            None => {
                // A one cell anchor gives its size: about eight columns by fifteen rows is where it ends.
                let ext = first(anchor, "ext");
                let cx = ext
                    .and_then(|e| e.attribute("cx"))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(4_572_000);
                let cy = ext
                    .and_then(|e| e.attribute("cy"))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(2_743_200);
                let cols = (cx / EMU_PER_PIXEL / 64).max(1) as i64;
                let rows = (cy / EMU_PER_PIXEL / 20).max(1) as i64;
                (from.0.offset(rows, cols).unwrap_or(from.0), 0, 0)
            }
        };
        charts.push(Chart { from, to, kind, title, series, part: chart_part, new: false });
    }
    charts
}

/// The pictures a sheet's drawing places over its cells.
pub fn read_pictures(
    sheet_part: &str,
    drawing_target: &str,
    text: &dyn Fn(&str) -> Option<String>,
) -> Vec<Picture> {
    let drawing_part = resolve(sheet_part, drawing_target);
    let Some(xml) = text(&drawing_part) else { return Vec::new() };
    let Ok(doc) = Document::parse(&xml) else { return Vec::new() };
    let rels = parse_rels(&text(&rels_path(&drawing_part)).unwrap_or_default());
    let mut pictures = Vec::new();
    for anchor in doc.root_element().children().filter(Node::is_element) {
        if !matches!(anchor.tag_name().name(), "twoCellAnchor" | "oneCellAnchor") {
            continue;
        }
        let Some(pic) = anchor.children().find(|c| c.is_element() && c.tag_name().name() == "pic")
        else {
            continue;
        };
        let Some(blip) = first(pic, "blip") else { continue };
        let Some(id) =
            blip.attributes().find(|a| a.name() == "embed").map(|a| a.value().to_owned())
        else {
            continue;
        };
        let Some(rel) = rels.iter().find(|r| r.id == id) else { continue };
        let from = first(anchor, "from").map_or((CellRef::new(0, 0), 0, 0), corner);
        let to = match first(anchor, "to") {
            Some(to) => corner(to),
            None => {
                let ext = first(anchor, "ext");
                let cx = ext
                    .and_then(|e| e.attribute("cx"))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(1_828_800);
                let cy = ext
                    .and_then(|e| e.attribute("cy"))
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(1_828_800);
                (from.0, from.1 + cx, from.2 + cy)
            }
        };
        pictures.push(Picture { from, to, part: resolve(&drawing_part, &rel.target) });
    }
    pictures
}

/// A formula reference for a chart, written as the file writes one.
fn reference(tag: &str, formula: &str, text: bool) -> String {
    let inner = if text { "strRef" } else { "numRef" };
    format!("<c:{tag}><c:{inner}><c:f>{}</c:f></c:{inner}></c:{tag}>", escape(formula))
}

/// The chart part of a chart made here.
pub fn chart_xml(chart: &Chart, text_categories: bool) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    out.push_str("<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">");
    out.push_str("<c:roundedCorners val=\"0\"/><c:chart>");
    match &chart.title {
        Some(title) => {
            out.push_str("<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr/></a:pPr><a:r><a:t>");
            out.push_str(&escape(title));
            out.push_str("</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title><c:autoTitleDeleted val=\"0\"/>");
        }
        None => out.push_str(&format!(
            "<c:autoTitleDeleted val=\"{}\"/>",
            if chart.series.len() == 1 { 0 } else { 1 }
        )),
    }
    out.push_str("<c:plotArea><c:layout/>");
    let series = |out: &mut String, scatter: bool, line: bool| {
        for (i, s) in chart.series.iter().enumerate() {
            out.push_str(&format!("<c:ser><c:idx val=\"{i}\"/><c:order val=\"{i}\"/>"));
            if let Some(name) = &s.name {
                out.push_str(&reference("tx", name, true));
            }
            if line || scatter {
                out.push_str("<c:marker><c:symbol val=\"circle\"/><c:size val=\"5\"/></c:marker>");
            }
            if scatter {
                if let Some(x) = &s.categories {
                    out.push_str(&reference("xVal", x, text_categories));
                }
                out.push_str(&reference("yVal", &s.values, false));
                out.push_str("<c:smooth val=\"0\"/>");
            } else {
                if let Some(c) = &s.categories {
                    out.push_str(&reference("cat", c, text_categories));
                }
                out.push_str(&reference("val", &s.values, false));
                if line {
                    out.push_str("<c:smooth val=\"0\"/>");
                }
            }
            out.push_str("</c:ser>");
        }
    };
    let axes = "<c:axId val=\"500000001\"/><c:axId val=\"500000002\"/>";
    match chart.kind {
        ChartKind::Column | ChartKind::Bar => {
            let dir = if chart.kind == ChartKind::Bar { "bar" } else { "col" };
            out.push_str(&format!("<c:barChart><c:barDir val=\"{dir}\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>"));
            series(&mut out, false, false);
            out.push_str(&format!(
                "<c:gapWidth val=\"219\"/><c:overlap val=\"-27\"/>{axes}</c:barChart>"
            ));
        }
        ChartKind::Line => {
            out.push_str("<c:lineChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>");
            series(&mut out, false, true);
            out.push_str(&format!("<c:marker val=\"1\"/>{axes}</c:lineChart>"));
        }
        ChartKind::Area => {
            out.push_str("<c:areaChart><c:grouping val=\"standard\"/><c:varyColors val=\"0\"/>");
            series(&mut out, false, false);
            out.push_str(&format!("{axes}</c:areaChart>"));
        }
        ChartKind::Pie => {
            out.push_str("<c:pieChart><c:varyColors val=\"1\"/>");
            series(&mut out, false, false);
            out.push_str("<c:firstSliceAng val=\"0\"/></c:pieChart>");
        }
        ChartKind::Doughnut => {
            out.push_str("<c:doughnutChart><c:varyColors val=\"1\"/>");
            series(&mut out, false, false);
            out.push_str("<c:firstSliceAng val=\"0\"/><c:holeSize val=\"75\"/></c:doughnutChart>");
        }
        ChartKind::Scatter | ChartKind::Other(_) => {
            out.push_str(
                "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>",
            );
            series(&mut out, true, false);
            out.push_str(&format!("{axes}</c:scatterChart>"));
        }
    }
    let has_axes = !matches!(chart.kind, ChartKind::Pie | ChartKind::Doughnut);
    if has_axes {
        let scatter = matches!(chart.kind, ChartKind::Scatter | ChartKind::Other(_));
        let (cat_pos, val_pos) = if chart.kind == ChartKind::Bar { ("l", "b") } else { ("b", "l") };
        let common = "<c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>";
        if scatter {
            out.push_str(&format!("<c:valAx><c:axId val=\"500000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}<c:crossAx val=\"500000002\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>"));
        } else {
            out.push_str(&format!("<c:catAx><c:axId val=\"500000001\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}<c:crossAx val=\"500000002\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>"));
        }
        out.push_str(&format!("<c:valAx><c:axId val=\"500000002\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/><c:majorGridlines/><c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>{common}<c:crossAx val=\"500000001\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"{}\"/></c:valAx>", if scatter { "midCat" } else { "between" }));
    }
    out.push_str("</c:plotArea>");
    if chart.series.len() > 1 || !has_axes {
        out.push_str("<c:legend><c:legendPos val=\"b\"/><c:overlay val=\"0\"/></c:legend>");
    }
    out.push_str(
        "<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>",
    );
    out
}

/// One anchor in a drawing, holding a chart by its relationship id.
pub fn anchor_xml(chart: &Chart, rel_id: &str, shape_id: u32) -> String {
    let corner = |tag: &str, (at, dx, dy): (CellRef, i64, i64)| {
        format!("<xdr:{tag}><xdr:col>{}</xdr:col><xdr:colOff>{dx}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{dy}</xdr:rowOff></xdr:{tag}>", at.col, at.row)
    };
    format!(
        "<xdr:twoCellAnchor editAs=\"oneCell\">{}{}<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{shape_id}\" name=\"Chart {}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rel_id}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>",
        corner("from", chart.from),
        corner("to", chart.to),
        shape_id.saturating_sub(1),
    )
}

/// A drawing part holding these anchors.
pub fn drawing_xml(anchors: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{anchors}</xdr:wsDr>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chart_written_here_reads_back_as_the_same_chart() {
        let chart = Chart {
            from: (CellRef::new(1, 4), 0, 0),
            to: (CellRef::new(16, 12), 0, 0),
            kind: ChartKind::Column,
            title: Some("Sales & costs".to_owned()),
            series: vec![ChartSeries {
                name: Some("Sheet1!$B$1".to_owned()),
                categories: Some("Sheet1!$A$2:$A$4".to_owned()),
                values: "Sheet1!$B$2:$B$4".to_owned(),
            }],
            part: String::new(),
            new: true,
        };
        let (kind, title, series) = read_chart(&chart_xml(&chart, true)).unwrap();
        assert_eq!(kind, ChartKind::Column);
        assert_eq!(title.as_deref(), Some("Sales & costs"));
        assert_eq!(series, chart.series);
    }
}
