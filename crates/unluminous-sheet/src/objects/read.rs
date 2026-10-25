//! Reading a worksheet's drawing part into [`DrawingObject`]s.

use super::props::{self, read_xfrm};
use super::*;
use crate::model::Relationship;
use crate::xlsx::xmltree::{self, Elem};
use crate::xlsx::{resolve, CHART_NS};

const ANCHORS: &[&str] = &["twoCellAnchor", "oneCellAnchor", "absoluteAnchor"];
/// Children of an anchor that are not the object it holds.
const ANCHOR_PARTS: &[&str] = &["from", "to", "pos", "ext", "clientData"];

/// Whether an anchor of the drawing holds a chart, which the chart code owns.
pub fn is_chart_anchor(anchor: &Elem) -> bool {
    let mut frames = Vec::new();
    anchor.find_all("graphicData", &mut frames);
    frames
        .iter()
        .any(|g| g.attr("uri").is_some_and(|u| u == CHART_NS || u.contains("chartex")))
}

/// The anchor element of a top level child of the drawing: the child itself, or the anchor inside an
/// `mc:AlternateContent`.
pub fn anchor_of(child: &Elem) -> Option<&Elem> {
    if ANCHORS.contains(&child.local()) {
        return Some(child);
    }
    if child.local() == "AlternateContent" {
        return child.child("Choice").and_then(|c| c.elems().find(|e| ANCHORS.contains(&e.local())));
    }
    None
}

pub fn anchor_of_mut(child: &mut Elem) -> Option<&mut Elem> {
    if ANCHORS.contains(&child.local()) {
        return Some(child);
    }
    if child.local() == "AlternateContent" {
        return child
            .child_mut("Choice")
            .and_then(|c| c.elems_mut().find(|e| ANCHORS.contains(&e.local())));
    }
    None
}

/// The element an anchor holds.
pub fn content_of(anchor: &Elem) -> Option<&Elem> {
    anchor.elems().find(|e| !ANCHOR_PARTS.contains(&e.local()))
}

fn number(e: Option<&Elem>, child: &str) -> i64 {
    e.and_then(|e| e.child(child)).map(Elem::text).and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

fn corner(e: Option<&Elem>) -> (CellRef, i64, i64) {
    (
        CellRef::new(number(e, "row").max(0) as u32, number(e, "col").max(0) as u32),
        number(e, "colOff"),
        number(e, "rowOff"),
    )
}

/// The anchor an element writes.
pub fn read_anchor(e: &Elem) -> Anchor {
    let kind = match e.local() {
        "oneCellAnchor" => AnchorKind::OneCell,
        "absoluteAnchor" => AnchorKind::Absolute,
        _ => AnchorKind::TwoCell,
    };
    let ext = e.child("ext");
    let size = |n: &str| ext.and_then(|x| x.attr(n)).and_then(|v| v.parse().ok()).unwrap_or(0);
    let pos = e.child("pos");
    let at = |n: &str| pos.and_then(|x| x.attr(n)).and_then(|v| v.parse().ok()).unwrap_or(0);
    Anchor {
        kind,
        placement: Placement::parse(e.attr("editAs").unwrap_or("twoCell")),
        from: corner(e.child("from")),
        to: if kind == AnchorKind::TwoCell { corner(e.child("to")) } else { Default::default() },
        pos: (at("x"), at("y")),
        ext: (size("cx"), size("cy")),
    }
}

fn corner_elem(prefix: &str, local: &str, c: (CellRef, i64, i64)) -> Elem {
    let mut e = Elem::new(&format!("{prefix}{local}"));
    for (n, v) in [("col", c.0.col as i64), ("colOff", c.1), ("row", c.0.row as i64), ("rowOff", c.2)] {
        let mut child = Elem::new(&format!("{prefix}{n}"));
        child.set_text(&v.to_string());
        e.push(child);
    }
    e
}

/// Write an anchor's numbers into its element, leaving the object inside it alone.
pub fn write_anchor(e: &mut Elem, a: &Anchor) {
    let prefix = e.name.split_once(':').map_or(String::new(), |(p, _)| format!("{p}:"));
    if a.kind == AnchorKind::TwoCell {
        e.set_attr("editAs", a.placement.attr());
        if a.placement == Placement::MoveAndSize {
            e.remove_attr("editAs");
        }
    }
    let replace = |e: &mut Elem, local: &str, new: Elem, at: usize| {
        e.remove_children(local);
        let at = at.min(e.children.len());
        e.children.insert(at, xmltree::Node::Elem(new));
    };
    match a.kind {
        AnchorKind::TwoCell => {
            replace(e, "from", corner_elem(&prefix, "from", a.from), 0);
            replace(e, "to", corner_elem(&prefix, "to", a.to), 1);
        }
        AnchorKind::OneCell => {
            replace(e, "from", corner_elem(&prefix, "from", a.from), 0);
            let mut ext = Elem::new(&format!("{prefix}ext"));
            ext.set_attr("cx", &a.ext.0.to_string());
            ext.set_attr("cy", &a.ext.1.to_string());
            replace(e, "ext", ext, 1);
        }
        AnchorKind::Absolute => {
            let mut pos = Elem::new(&format!("{prefix}pos"));
            pos.set_attr("x", &a.pos.0.to_string());
            pos.set_attr("y", &a.pos.1.to_string());
            replace(e, "pos", pos, 0);
            let mut ext = Elem::new(&format!("{prefix}ext"));
            ext.set_attr("cx", &a.ext.0.to_string());
            ext.set_attr("cy", &a.ext.1.to_string());
            replace(e, "ext", ext, 1);
        }
    }
}

/// What kind of object an element is.
fn classify(content: &Elem, wrapped: bool) -> ObjectKind {
    if wrapped {
        let text = content.to_xml();
        let has = |w: &str| text.contains(w);
        return if has("compatExt") || has("ctrlProp") || has("controlPr") {
            ObjectKind::FormControl
        } else if has("oMath") || has("a14:m") {
            ObjectKind::Equation
        } else if has("am3d") || has("model3d") {
            ObjectKind::Model3D
        } else if has("svgBlip") {
            ObjectKind::Icon
        } else if has("diagram") {
            ObjectKind::SmartArt
        } else {
            ObjectKind::Other
        };
    }
    match content.local() {
        "pic" => {
            if content.find("svgBlip").is_some() {
                ObjectKind::Icon
            } else {
                ObjectKind::Picture
            }
        }
        "sp" => {
            let tx_box = content.find("cNvSpPr").and_then(|c| c.attr("txBox")) == Some("1");
            let warped = content
                .find("prstTxWarp")
                .and_then(|w| w.attr("prst"))
                .is_some_and(|p| p != "textNoShape");
            if tx_box {
                ObjectKind::TextBox
            } else if warped {
                ObjectKind::WordArt
            } else {
                ObjectKind::Shape
            }
        }
        "cxnSp" => ObjectKind::Connector,
        "grpSp" => ObjectKind::Group,
        "graphicFrame" => {
            let uri = content.find("graphicData").and_then(|g| g.attr("uri")).unwrap_or("");
            if uri.contains("diagram") {
                ObjectKind::SmartArt
            } else if uri.contains("ole") {
                ObjectKind::OleObject
            } else {
                ObjectKind::Other
            }
        }
        _ => ObjectKind::Other,
    }
}

/// The part a relationship id names, from the drawing.
fn part_of(rels: &[Relationship], drawing_part: &str, id: &str) -> Option<String> {
    rels.iter().find(|r| r.id == id && !r.external).map(|r| resolve(drawing_part, &r.target))
}

/// The stored picture of an object kept as written: the one in its fallback, else any it has.
fn fallback_picture(tree: &Elem, rels: &[Relationship], drawing_part: &str) -> Option<String> {
    let search = tree.child("Fallback").unwrap_or(tree);
    let mut blips = Vec::new();
    search.find_all("blip", &mut blips);
    if blips.is_empty() {
        tree.find_all("blip", &mut blips);
    }
    blips
        .iter()
        .find_map(|b| b.attr_local("embed"))
        .and_then(|id| part_of(rels, drawing_part, id))
}

/// Everything needed to turn elements into objects.
pub struct Context<'a> {
    pub rels: &'a [Relationship],
    pub drawing_part: &'a str,
}

/// One object from the element it is in. `anchor` is `None` for an object inside a group.
fn build(cx: &Context, tree: Elem, anchor: Option<(Anchor, &Elem)>, wrapped: bool) -> DrawingObject {
    let probe = if wrapped { inner_content(&tree).unwrap_or(&tree) } else { &tree };
    let kind = classify(&tree, wrapped);
    let nv = props::non_visual(probe).or_else(|| tree.find("cNvPr"));
    let attr = |n: &str| nv.and_then(|c| c.attr(n)).unwrap_or("").to_owned();
    let (anchor_value, client) = match anchor {
        Some((a, client)) => (a, Some(client)),
        None => (Anchor::default(), None),
    };
    let flag = |n: &str| client.and_then(|c| c.child("clientData")).and_then(|d| d.attr(n)) != Some("0");
    let t = if wrapped { props::Xfrm::default() } else { read_xfrm(&tree) };
    let mut obj = DrawingObject {
        id: attr("id").parse().unwrap_or(0),
        name: attr("name"),
        descr: attr("descr"),
        hidden: attr("hidden") == "1",
        kind,
        anchor: anchor_value,
        frame: Rect::new(t.off.0, t.off.1, t.ext.0, t.ext.1),
        rotation: t.rot as f64 / 60_000.0,
        frame_size: t.ext,
        flip_h: t.flip_h,
        flip_v: t.flip_v,
        lock_aspect: !wrapped && props::read_lock(&tree),
        locked: flag("fLocksWithSheet"),
        prints: flag("fPrintsWithSheet"),
        picture: None,
        shape: None,
        children: Vec::new(),
        fallback: None,
        child_space: Rect::new(t.ch_off.0, t.ch_off.1, t.ch_ext.0, t.ch_ext.1),
        tree,
        wrapped,
    };
    fill_body(cx, &mut obj);
    obj
}

/// The content element inside an `mc:AlternateContent`.
fn inner_content(tree: &Elem) -> Option<&Elem> {
    anchor_of(tree).and_then(content_of)
}

fn fill_body(cx: &Context, obj: &mut DrawingObject) {
    match obj.kind {
        ObjectKind::Picture | ObjectKind::Icon if !obj.wrapped => {
            if let Some((embed, crop, svg)) = props::read_picture(&obj.tree) {
                if let Some(part) = part_of(cx.rels, cx.drawing_part, &embed) {
                    let svg = svg.and_then(|id| part_of(cx.rels, cx.drawing_part, &id));
                    obj.picture = Some(PictureData { part, crop, svg });
                }
            }
        }
        ObjectKind::Shape | ObjectKind::TextBox | ObjectKind::Connector => {
            obj.shape = props::read_shape(&obj.tree);
        }
        ObjectKind::Group => {
            let space = obj.child_space;
            obj.children = obj
                .tree
                .elems()
                .filter(|e| matches!(e.local(), "sp" | "pic" | "cxnSp" | "grpSp" | "graphicFrame"))
                .map(|e| build(cx, e.clone(), None, false))
                .collect();
            obj.child_space = space;
        }
        _ => {}
    }
    if obj.picture.is_none() && obj.shape.is_none() && obj.children.is_empty() {
        obj.fallback = fallback_picture(&obj.tree, cx.rels, cx.drawing_part);
    }
}

/// Every object in a drawing part except charts, bottom first.
pub fn read_drawing(xml: &str, rels: &[Relationship], drawing_part: &str) -> Vec<DrawingObject> {
    let Some(root) = xmltree::parse(xml) else { return Vec::new() };
    let cx = Context { rels, drawing_part };
    let mut objects = Vec::new();
    for child in root.elems() {
        let Some(anchor_el) = anchor_of(child) else { continue };
        if is_chart_anchor(child) {
            continue;
        }
        let anchor = read_anchor(anchor_el);
        if child.local() == "AlternateContent" {
            objects.push(build(&cx, child.clone(), Some((anchor, anchor_el)), true));
        } else if let Some(content) = content_of(anchor_el) {
            objects.push(build(&cx, content.clone(), Some((anchor, anchor_el)), false));
        }
    }
    objects
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWING: &str = "<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><xdr:twoCellAnchor editAs=\"oneCell\"><xdr:from><xdr:col>2</xdr:col><xdr:colOff>10</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>20</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>6</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"4\" name=\"Rect1\" descr=\"A box\"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x=\"1\" y=\"2\"/><a:ext cx=\"3\" cy=\"4\"/></a:xfrm><a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val 30000\"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val=\"FF0000\"><a:alpha val=\"50000\"/></a:srgbClr></a:solidFill><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"accent2\"/></a:solidFill><a:prstDash val=\"dash\"/></a:ln></xdr:spPr><xdr:txBody><a:bodyPr anchor=\"ctr\"/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"en-US\" sz=\"1400\" b=\"1\"/><a:t>Hello</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>";

    #[test]
    fn a_shape_is_read_with_its_anchor_looks_and_text() {
        let sheet = Sheet::new("S");
        let objects = read_drawing(DRAWING, &[], "xl/drawings/drawing1.xml");
        assert_eq!(objects.len(), 1);
        let o = &objects[0];
        assert_eq!((o.id, o.name.as_str(), o.descr.as_str()), (4, "Rect1", "A box"));
        assert_eq!(o.kind, ObjectKind::Shape);
        assert_eq!(o.anchor.placement, Placement::Move);
        assert_eq!(o.anchor.from, (CellRef::new(3, 2), 10, 20));
        let shape = o.shape.as_ref().unwrap();
        assert_eq!((shape.preset.as_str(), shape.adjust), ("roundRect", Some(30000)));
        assert_eq!(shape.fill, Fill::Solid(ShapeColor { alpha: 50_000, ..ShapeColor::rgb(255, 0, 0) }));
        assert_eq!(shape.line.width, Some(19050));
        assert_eq!(shape.line.dash, Dash::Dash);
        let text = shape.text.as_ref().unwrap();
        assert_eq!(text.plain_text(), "Hello");
        assert_eq!(text.valign, VAlign::Middle);
        assert_eq!(text.paragraphs[0].runs[0].size, Some(14.0));
    }

    #[test]
    fn a_chart_anchor_is_not_an_object() {
        let xml = "<xdr:wsDr xmlns:xdr=\"x\"><xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id=\"2\" name=\"Chart 1\"/></xdr:nvGraphicFramePr><a:graphic xmlns:a=\"a\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/></a:graphic></xdr:graphicFrame></xdr:twoCellAnchor></xdr:wsDr>";
        assert!(read_drawing(xml, &[], "p").is_empty());
    }
}
