//! The charts of a drawing: reading them with their anchors, and rewriting a drawing when charts were
//! moved, removed or added.
//!
//! A drawing holds more than charts (pictures, shapes, controls), and those are carried as the XML they
//! were. Only the anchors of charts are touched, and only for a chart that changed.

use roxmltree::{Document, Node};

use super::{chart_read, chartex, escape, parse_rels, rels_path, resolve};
use crate::coord::CellRef;
use crate::model::{Chart, ChartKind, Relationship};

/// English Metric Units in one pixel at 96 dots an inch.
pub const EMU_PER_PIXEL: i64 = 9525;

const XDR_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

/// The relationship type of a `chartEx` part.
pub const REL_CHART_EX: &str = "http://schemas.microsoft.com/office/2014/relationships/chartEx";
/// The relationship type of a chart sheet.
pub const REL_CHART_SHEET: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet";
/// The content type of a chart sheet part.
pub const CT_CHART_SHEET: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml";
/// The content type of a `chartEx` part.
pub const CT_CHART_EX: &str = "application/vnd.ms-office.chartex+xml";

fn first<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.descendants().find(|d| d.is_element() && d.tag_name().name() == name)
}

fn number(n: Option<Node>) -> i64 {
    n.and_then(|n| n.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

fn corner(n: Node) -> (CellRef, i64, i64) {
    let col = number(first(n, "col")) as u32;
    let row = number(first(n, "row")) as u32;
    (CellRef::new(row, col), number(first(n, "colOff")), number(first(n, "rowOff")))
}

fn is_anchor(n: Node) -> bool {
    n.is_element()
        && matches!(n.tag_name().name(), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")
}

/// The chart reference inside an anchor: the `c:chart` or `cx:chart` element and its relationship id.
fn chart_reference<'a, 'b>(anchor: Node<'a, 'b>) -> Option<(Node<'a, 'b>, String)> {
    let reference = anchor.descendants().find(|d| {
        d.is_element()
            && d.tag_name().name() == "chart"
            && matches!(d.tag_name().namespace(), Some(super::CHART_NS) | Some(chartex::NS))
    })?;
    let id = reference.attributes().find(|a| a.name() == "id")?.value().to_owned();
    Some((reference, id))
}

/// Where a one cell anchor ends: about eight columns by fifteen rows is the size Excel gives a chart.
fn guess_end(anchor: Node, from: (CellRef, i64, i64)) -> (CellRef, i64, i64) {
    let ext = first(anchor, "ext");
    let size = |name: &str, fallback: i64| {
        ext.and_then(|e| e.attribute(name)).and_then(|v| v.parse::<i64>().ok()).unwrap_or(fallback)
    };
    let cols = (size("cx", 4_572_000) / EMU_PER_PIXEL / 64).max(1);
    let rows = (size("cy", 2_743_200) / EMU_PER_PIXEL / 20).max(1);
    (from.0.offset(rows, cols).unwrap_or(from.0), 0, 0)
}

/// The charts a drawing holds, read for drawing and editing. `text` reads a part of the package and
/// `names` turns a defined name into the formula it stands for.
pub fn read_charts(
    sheet_part: &str,
    drawing_target: &str,
    text: &dyn Fn(&str) -> Option<String>,
    names: &dyn Fn(&str) -> Option<String>,
) -> Vec<Chart> {
    let drawing_part = resolve(sheet_part, drawing_target);
    let Some(xml) = text(&drawing_part) else { return Vec::new() };
    let Ok(doc) = Document::parse(&xml) else { return Vec::new() };
    let rels = parse_rels(&text(&rels_path(&drawing_part)).unwrap_or_default());
    let mut charts = Vec::new();
    for anchor in doc.root_element().children().filter(|n| is_anchor(*n)) {
        let Some((reference, id)) = chart_reference(anchor) else { continue };
        let Some(rel) = rels.iter().find(|r| r.id == id) else { continue };
        let part = resolve(&drawing_part, &rel.target);
        let Some(chart_xml) = text(&part) else { continue };
        let extended = reference.tag_name().namespace() == Some(chartex::NS);
        let resolver = |f: &str| names(f).unwrap_or_else(|| f.to_owned());
        let parsed = if extended {
            chartex::parse(&chart_xml, &resolver)
        } else {
            chart_read::parse(&chart_xml)
        };
        let Some(parsed) = parsed else { continue };
        let from = first(anchor, "from").map_or((CellRef::new(0, 0), 0, 0), corner);
        let to = first(anchor, "to").map_or_else(|| guess_end(anchor, from), corner);
        let mut chart = Chart {
            from,
            to,
            kind: ChartKind::Column,
            title: parsed.title,
            series: parsed.series,
            part,
            new: false,
            spec: parsed.spec,
        };
        read_object_properties(anchor, &mut chart);
        chart.spec.anchor_edit_as = anchor.attribute("editAs").map(str::to_owned);
        chart.sync_kind();
        charts.push(chart);
    }
    charts
}

/// The name and description a drawing gives its object.
fn read_object_properties(anchor: Node, chart: &mut Chart) {
    if let Some(props) = first(anchor, "cNvPr") {
        chart.spec.name = props.attribute("name").map(str::to_owned).filter(|n| !n.is_empty());
        chart.spec.alt_text = props.attribute("descr").map(str::to_owned).filter(|n| !n.is_empty());
    }
}

/// The part of a chart a drawing anchor refers to, if it refers to one.
fn anchor_part(anchor: Node, drawing_part: &str, rels: &[Relationship]) -> Option<String> {
    let (_, id) = chart_reference(anchor)?;
    let rel = rels.iter().find(|r| r.id == id)?;
    Some(resolve(drawing_part, &rel.target))
}

fn corner_xml(prefix: &str, tag: &str, (at, dx, dy): (CellRef, i64, i64)) -> String {
    format!(
        "<{prefix}:{tag}><{prefix}:col>{}</{prefix}:col><{prefix}:colOff>{dx}</{prefix}:colOff><{prefix}:row>{}</{prefix}:row><{prefix}:rowOff>{dy}</{prefix}:rowOff></{prefix}:{tag}>",
        at.col, at.row
    )
}

/// An anchor moved or resized: its `from` and `to` written again, and a one cell or absolute anchor
/// turned into a two cell anchor holding the same object.
fn patch_anchor(xml: &str, anchor: Node, chart: &Chart) -> String {
    let prefix = anchor.lookup_prefix(XDR_NS).filter(|p| !p.is_empty()).unwrap_or("xdr").to_owned();
    let mut inner = String::new();
    for c in anchor.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "from" | "to" | "ext" | "pos" => {}
            _ => inner.push_str(&xml[c.range()]),
        }
    }
    let edit_as = chart.spec.anchor_edit_as.as_deref().unwrap_or("oneCell");
    format!(
        "<{prefix}:twoCellAnchor editAs=\"{edit_as}\">{}{}{inner}</{prefix}:twoCellAnchor>",
        corner_xml(&prefix, "from", chart.from),
        corner_xml(&prefix, "to", chart.to),
    )
}

/// The drawing after the changes to its charts.
pub struct Rebuilt {
    pub xml: String,
    pub rels: Vec<Relationship>,
    /// Chart parts the drawing no longer refers to, which are not written.
    pub dropped: Vec<String>,
}

/// Rewrite a drawing: charts in `removed` leave it, charts that moved get their new anchors, and the
/// anchors in `added` (each with its relationship id already in `rels`) are appended.
pub fn rebuild(
    existing: Option<&str>,
    existing_rels: &[Relationship],
    drawing_part: &str,
    charts: &[Chart],
    removed: &[String],
    added: &str,
) -> Rebuilt {
    let mut rels = existing_rels.to_vec();
    let mut dropped = Vec::new();
    let Some(xml) = existing else {
        return Rebuilt { xml: drawing_xml(added), rels, dropped };
    };
    let Ok(doc) = Document::parse(xml) else {
        return Rebuilt { xml: xml.to_owned(), rels, dropped };
    };
    let root = doc.root_element();
    let mut anchors = String::new();
    for node in root.children().filter(Node::is_element) {
        if !is_anchor(node) {
            anchors.push_str(&xml[node.range()]);
            continue;
        }
        let part = anchor_part(node, drawing_part, existing_rels);
        if let Some(part) = part.as_ref().filter(|p| removed.contains(p)) {
            dropped.push(part.clone());
            if let Some((_, id)) = chart_reference(node) {
                rels.retain(|r| r.id != id);
            }
            continue;
        }
        let moved = part
            .as_ref()
            .and_then(|p| charts.iter().find(|c| &c.part == p && !c.new && c.spec.moved));
        match moved {
            Some(chart) => anchors.push_str(&patch_anchor(xml, node, chart)),
            None => anchors.push_str(&xml[node.range()]),
        }
    }
    anchors.push_str(added);
    let open_end = root.first_child().map_or(root.range().end, |c| c.range().start);
    let open = &xml[root.range().start..open_end];
    let close = format!("</{}>", qualified_name(&root));
    let head = &xml[..root.range().start];
    Rebuilt { xml: format!("{head}{open}{anchors}{close}"), rels, dropped }
}

fn qualified_name(root: &Node) -> String {
    match root.tag_name().namespace().and_then(|ns| root.lookup_prefix(ns)) {
        Some(p) if !p.is_empty() => format!("{p}:{}", root.tag_name().name()),
        _ => root.tag_name().name().to_owned(),
    }
}

/// One anchor holding a chart by its relationship id.
pub fn anchor_xml(chart: &Chart, rel_id: &str, shape_id: u32) -> String {
    let name = chart.spec.name.clone().unwrap_or_else(|| format!("Chart {}", shape_id.saturating_sub(1)));
    let descr = chart.spec.alt_text.as_deref().map(|d| format!(" descr=\"{}\"", escape(d))).unwrap_or_default();
    let edit_as = chart.spec.anchor_edit_as.as_deref().unwrap_or("oneCell");
    let frame = if chart.spec.ex.is_some() {
        format!(
            "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:cx1=\"http://schemas.microsoft.com/office/drawing/2015/9/8/chartex\" Requires=\"cx1\"><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{shape_id}\" name=\"{}\"{descr}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2014/chartex\"><cx:chart xmlns:cx=\"{}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rel_id}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"0\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"4064000\" cy=\"2921000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"green\"/></a:solidFill></a:ln></xdr:spPr><xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>This chart isn't available in your version of Excel.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent>",
            escape(&name),
            chartex::NS,
        )
    } else {
        format!(
            "<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{shape_id}\" name=\"{}\"{descr}/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rel_id}\"/></a:graphicData></a:graphic></xdr:graphicFrame>",
            escape(&name)
        )
    };
    format!(
        "<xdr:twoCellAnchor editAs=\"{edit_as}\">{}{}{frame}<xdr:clientData/></xdr:twoCellAnchor>",
        corner_xml("xdr", "from", chart.from),
        corner_xml("xdr", "to", chart.to),
    )
}

/// A drawing part holding these anchors.
pub fn drawing_xml(anchors: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"{XDR_NS}\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">{anchors}</xdr:wsDr>"
    )
}

/// The chart sheet part holding one drawing.
pub fn chart_sheet_xml(rel_id: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<chartsheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheetViews><sheetView workbookViewId=\"0\" zoomToFit=\"1\"/></sheetViews><pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/><drawing r:id=\"{rel_id}\"/></chartsheet>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWING: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="100" cy="100"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="2" name="Chart 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><a:graphic><a:graphicData uri="x"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>"#;

    fn rel() -> Vec<Relationship> {
        vec![Relationship {
            id: "rId1".into(),
            kind: super::super::REL_CHART.into(),
            target: "../charts/chart1.xml".into(),
            external: false,
        }]
    }

    #[test]
    fn a_moved_chart_gets_a_two_cell_anchor_and_a_removed_one_leaves() {
        let mut chart = Chart { part: "xl/charts/chart1.xml".into(), ..Chart::default() };
        chart.from = (CellRef::new(5, 6), 0, 0);
        chart.to = (CellRef::new(20, 12), 0, 0);
        chart.spec.moved = true;
        let moved = rebuild(Some(DRAWING), &rel(), "xl/drawings/drawing1.xml", &[chart], &[], "");
        assert!(moved.xml.contains("<xdr:twoCellAnchor editAs=\"oneCell\"><xdr:from><xdr:col>6</xdr:col>"));
        assert!(moved.xml.contains("r:id=\"rId1\""));
        assert!(Document::parse(&moved.xml).is_ok());
        let removed = rebuild(
            Some(DRAWING),
            &rel(),
            "xl/drawings/drawing1.xml",
            &[],
            &["xl/charts/chart1.xml".to_owned()],
            "",
        );
        assert!(!removed.xml.contains("graphicFrame"));
        assert!(removed.rels.is_empty());
        assert_eq!(removed.dropped, vec!["xl/charts/chart1.xml".to_owned()]);
    }
}
