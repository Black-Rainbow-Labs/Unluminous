//! Charts and pictures in a drawing.
//!
//! **A chart read from a file and left alone is kept exactly as it was**, so everything Excel put in it
//! survives. A chart that was changed, or made here, is written from the model: see [`super::chart_read`],
//! [`super::chart_write`], [`super::chartex`] and [`super::chart_drawing`].

use roxmltree::{Document, Node};

use super::{parse_rels, rels_path, resolve};
use crate::coord::CellRef;
use crate::model::Picture;

pub use super::chart_drawing::{
    anchor_xml, chart_sheet_xml, drawing_xml, read_charts, rebuild, EMU_PER_PIXEL,
};

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
