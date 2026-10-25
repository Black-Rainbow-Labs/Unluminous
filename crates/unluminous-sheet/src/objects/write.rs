//! Writing the objects of a drawing back into its part.
//!
//! [`synced`] returns the workbook with each changed drawing part written again from the sheet's objects:
//! charts keep their anchors exactly as the part has them, and the other anchors of the part are replaced,
//! in order, by the objects as they stand now. A workbook whose drawings did not change is not copied.

use std::collections::BTreeSet;

use super::props::{self, Xfrm};
use super::read::{anchor_of, anchor_of_mut, is_chart_anchor, write_anchor};
use super::*;
use crate::model::{Relationship, Workbook};
use crate::xlsx::xmltree::{self, Elem, Node};
use crate::xlsx::{next_id, parse_rels, relative, rels_path, resolve, write_rels};
use crate::xlsx::{CT_DRAWING, REL_DRAWING, REL_NS};

const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const SPREADSHEET_DRAWING_NS: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// Whether any sheet's drawing has to be written again.
pub fn needs_sync(book: &Workbook) -> bool {
    book.sheets.iter().any(|s| s.drawing.changed)
}

/// The workbook with the drawing parts of its changed sheets written from their objects.
pub fn synced(book: &Workbook) -> Workbook {
    let mut copy = book.clone();
    for index in 0..copy.sheets.len() {
        if copy.sheets[index].drawing.changed {
            sync_sheet(&mut copy, index);
        }
    }
    copy
}

/// The part a sheet's drawing is in, made when the sheet has none.
fn drawing_part(book: &mut Workbook, index: usize) -> (String, bool) {
    let sheet = &book.sheets[index];
    if let Some(rel) = sheet.raw.rels.iter().find(|r| r.kind == REL_DRAWING) {
        return (resolve(&sheet.raw.part, &rel.target), false);
    }
    let taken: BTreeSet<&String> = book.kept.parts.iter().map(|(p, _)| p).collect();
    let part = (1..)
        .map(|n| format!("xl/drawings/drawing{n}.xml"))
        .find(|p| !taken.contains(p))
        .unwrap_or_default();
    let sheet_part =
        if sheet.raw.part.is_empty() { "xl/worksheets/sheet1.xml" } else { &sheet.raw.part };
    let id = next_id(&sheet.raw.rels);
    let target = relative(sheet_part, &part);
    let prefix = sheet
        .raw
        .namespaces
        .iter()
        .find(|(_, uri)| uri == REL_NS)
        .map(|(p, _)| p.clone())
        .unwrap_or_else(|| "r".to_owned());
    let sheet = &mut book.sheets[index];
    if !sheet.raw.namespaces.iter().any(|(_, uri)| uri == REL_NS) {
        sheet.raw.namespaces.push((prefix.clone(), REL_NS.to_owned()));
    }
    sheet.raw.rels.push(Relationship {
        id: id.clone(),
        kind: REL_DRAWING.to_owned(),
        target,
        external: false,
    });
    sheet.raw.elements.retain(|(n, _)| n != "drawing");
    sheet.raw.elements.push(("drawing".to_owned(), format!("<drawing {prefix}:id=\"{id}\"/>")));
    book.kept.overrides.push((format!("/{part}"), CT_DRAWING.to_owned()));
    (part, true)
}

fn empty_drawing() -> Elem {
    let mut root = Elem::new("xdr:wsDr");
    root.set_attr("xmlns:xdr", SPREADSHEET_DRAWING_NS);
    root.set_attr("xmlns:a", DRAWING_NS);
    root
}

/// The anchor element for an object whose tree is its content.
fn wrap_in_anchor(obj: &DrawingObject) -> Elem {
    let prefix = obj.tree.prefix().to_owned();
    let p = if prefix.is_empty() { String::new() } else { format!("{prefix}:") };
    let local = match obj.anchor.kind {
        AnchorKind::TwoCell => "twoCellAnchor",
        AnchorKind::OneCell => "oneCellAnchor",
        AnchorKind::Absolute => "absoluteAnchor",
    };
    let mut anchor = Elem::new(&format!("{p}{local}"));
    write_anchor(&mut anchor, &obj.anchor);
    let mut tree = obj.tree.clone();
    tree.attrs.retain(|(n, v)| {
        !((n == "xmlns:xdr" && v == SPREADSHEET_DRAWING_NS) || (n == "xmlns:a" && v == DRAWING_NS))
    });
    anchor.push(tree);
    let mut client = Elem::new(&format!("{p}clientData"));
    if !obj.locked {
        client.set_attr("fLocksWithSheet", "0");
    }
    if !obj.prints {
        client.set_attr("fPrintsWithSheet", "0");
    }
    anchor.push(client);
    anchor
}

/// The relationship id of an image part in the drawing's relationships, made when the drawing has none.
fn image_rel(rels: &mut Vec<Relationship>, drawing_part: &str, part: &str) -> String {
    if let Some(r) = rels.iter().find(|r| !r.external && resolve(drawing_part, &r.target) == part) {
        return r.id.clone();
    }
    let id = next_id(rels);
    rels.push(Relationship {
        id: id.clone(),
        kind: REL_IMAGE.to_owned(),
        target: relative(drawing_part, part),
        external: false,
    });
    id
}

/// The element a top level object is written as.
fn object_element(
    obj: &DrawingObject,
    sheet: &Sheet,
    rels: &mut Vec<Relationship>,
    drawing_part: &str,
) -> Elem {
    let mut obj = obj.clone();
    if obj.wrapped {
        let anchor = obj.anchor.clone();
        let mut tree = obj.tree;
        if let Some(a) = anchor_of_mut(&mut tree) {
            write_anchor(a, &anchor);
        }
        return tree;
    }
    let frame = obj.frame_rect(sheet);
    let t = Xfrm {
        off: (frame.x, frame.y),
        ext: (frame.w, frame.h),
        rot: (obj.rotation.rem_euclid(360.0) * 60_000.0).round() as i64,
        flip_h: obj.flip_h,
        flip_v: obj.flip_v,
        ..Xfrm::default()
    };
    let child_space = (obj.kind == ObjectKind::Group).then_some((
        (obj.child_space.x, obj.child_space.y),
        (obj.child_space.w, obj.child_space.h),
    ));
    props::write_xfrm(&mut obj.tree, &t, child_space);
    props::patch(&mut obj, &mut |part| image_rel(rels, drawing_part, part));
    wrap_in_anchor(&obj)
}

/// Write one sheet's drawing part from its objects.
fn sync_sheet(book: &mut Workbook, index: usize) {
    let has_drawing = book.sheets[index].raw.rels.iter().any(|r| r.kind == REL_DRAWING);
    if !has_drawing && book.sheets[index].drawing.objects.is_empty() {
        return;
    }
    let (part, _) = drawing_part(book, index);
    let rels_part = rels_path(&part);
    let mut rels = book
        .kept
        .get(&rels_part)
        .map(|b| parse_rels(&String::from_utf8_lossy(b)))
        .unwrap_or_default();
    let mut root = book
        .kept
        .get(&part)
        .and_then(|b| xmltree::parse(&String::from_utf8_lossy(b)))
        .unwrap_or_else(empty_drawing);
    for (n, uri) in [("xmlns:xdr", SPREADSHEET_DRAWING_NS), ("xmlns:a", DRAWING_NS)] {
        if root.attr(n).is_none() {
            root.set_attr(n, uri);
        }
    }
    let sheet = &book.sheets[index];
    let mut made: Vec<Elem> = sheet
        .drawing
        .objects
        .iter()
        .map(|o| object_element(o, sheet, &mut rels, &part))
        .collect::<Vec<_>>();
    made.reverse();
    let mut children = Vec::new();
    for node in std::mem::take(&mut root.children) {
        match &node {
            Node::Elem(e) if anchor_of(e).is_some() && !is_chart_anchor(e) => {
                if let Some(next) = made.pop() {
                    children.push(Node::Elem(next));
                }
            }
            _ => children.push(node),
        }
    }
    while let Some(next) = made.pop() {
        children.push(Node::Elem(next));
    }
    root.children = children;
    let xml = format!("{XML_HEAD}{}", root.to_xml());
    prune_image_rels(&mut rels, &xml);
    book.kept.set(&part, xml.into_bytes());
    book.kept.set(&rels_part, write_rels(&rels).into_bytes());
    drop_unused_media(book, index);
}

/// Take out the image relationships nothing in the drawing names any more.
fn prune_image_rels(rels: &mut Vec<Relationship>, xml: &str) {
    rels.retain(|r| r.kind != REL_IMAGE || xml.contains(&format!("\"{}\"", r.id)));
}

/// Remove the media parts that no relationship of the package reaches.
fn drop_unused_media(book: &mut Workbook, _index: usize) {
    let media: Vec<String> = book
        .kept
        .parts
        .iter()
        .map(|(p, _)| p.clone())
        .filter(|p| p.starts_with("xl/media/"))
        .collect();
    for part in media {
        let used =
            book.kept.parts.iter().filter(|(p, _)| p.ends_with(".rels")).any(|(rp, bytes)| {
                let owner = owner_of_rels(rp);
                parse_rels(&String::from_utf8_lossy(bytes))
                    .iter()
                    .any(|r| !r.external && resolve(&owner, &r.target) == part)
            });
        let in_sheet_rels = book.sheets.iter().any(|s| {
            s.raw.rels.iter().any(|r| !r.external && resolve(&s.raw.part, &r.target) == part)
        });
        if !used && !in_sheet_rels {
            book.kept.remove(&part);
        }
    }
}

/// The part a `.rels` path belongs to: `xl/drawings/_rels/drawing1.xml.rels` is `xl/drawings/drawing1.xml`.
fn owner_of_rels(rels: &str) -> String {
    let trimmed = rels.strip_suffix(".rels").unwrap_or(rels);
    match trimmed.rsplit_once("/_rels/") {
        Some((folder, name)) => format!("{folder}/{name}"),
        None => trimmed.trim_start_matches("_rels/").to_owned(),
    }
}
