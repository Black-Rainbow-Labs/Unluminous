//! A picture in a cell: the rich value parts Excel writes for Picture > Place in Cell, and reading which
//! image a cell shows.
//!
//! The cell holds `#VALUE!` and a `vm` index. The index names a `valueMetadata` entry, which names a rich
//! value, which names a relationship to the image. The `IMAGE` function is not modelled.

use std::sync::Arc;

use crate::coord::CellRef;
use crate::model::{Relationship, Workbook};
use crate::value::{ErrorKind, Value};
use crate::xlsx::xmltree::{self, Elem};
use crate::xlsx::{next_id, parse_rels, relative, rels_path, resolve, write_rels};

const REL_TYPES: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes";
const REL_STRUCTURE: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure";
const REL_VALUE: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue";
const REL_VALUE_REL: &str = "http://schemas.microsoft.com/office/2022/10/relationships/richValueRel";
const REL_METADATA: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const CT_METADATA: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";
const CT_TYPES: &str = "application/vnd.ms-excel.rdrichvaluetypes+xml";
const CT_STRUCTURE: &str = "application/vnd.ms-excel.rdrichvaluestructure+xml";
const CT_VALUE: &str = "application/vnd.ms-excel.rdrichvalue+xml";
const CT_VALUE_REL: &str = "application/vnd.ms-excel.richvaluerel+xml";
const NS_RICH: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const NS_RICH_REL: &str = "http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const TYPES_XML: &str = include_str!("rdrichvaluetypes.xml");

/// The key of a picture's local image in the rich value structure.
const LOCAL_IMAGE: &str = "_localImage";

fn workbook_part(book: &Workbook) -> String {
    if book.part.is_empty() { "xl/workbook.xml".to_owned() } else { book.part.clone() }
}

/// The part a workbook relationship of this type names, if the workbook has one.
fn part_of(book: &Workbook, kind: &str) -> Option<String> {
    book.kept
        .workbook_rels
        .iter()
        .find(|r| r.kind == kind && !r.external)
        .map(|r| resolve(&workbook_part(book), &r.target))
}

/// The part of this relationship type, made at `default` when the workbook has none.
fn ensure_part(book: &mut Workbook, kind: &str, default: &str, content_type: &str) -> String {
    if let Some(p) = part_of(book, kind) {
        return p;
    }
    let id = next_id(&book.kept.workbook_rels);
    let target = relative(&workbook_part(book), default);
    book.kept.workbook_rels.push(Relationship { id, kind: kind.to_owned(), target, external: false });
    book.kept.overrides.push((format!("/{default}"), content_type.to_owned()));
    default.to_owned()
}

fn read_tree(book: &Workbook, part: &str) -> Option<Elem> {
    book.kept.get(part).and_then(|b| xmltree::parse(&String::from_utf8_lossy(b)))
}

fn write_tree(book: &mut Workbook, part: &str, tree: &Elem) {
    book.kept.set(part, format!("{HEAD}{}", tree.to_xml()).into_bytes());
}

fn root(name: &str, ns: &str, extra: &[(&str, &str)]) -> Elem {
    let mut e = Elem::new(name);
    e.set_attr("xmlns", ns);
    for (n, v) in extra {
        e.set_attr(n, v);
    }
    e.set_attr("count", "0");
    e
}

fn bump_count(e: &mut Elem) -> usize {
    let n = e.attr("count").and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
    e.set_attr("count", &(n + 1).to_string());
    n
}

/// Add `_localImage` to the rich value structures unless they have it. Returns its index.
fn local_image_structure(book: &mut Workbook) -> usize {
    let part = ensure_part(book, REL_STRUCTURE, "xl/richData/rdrichvaluestructure.xml", CT_STRUCTURE);
    let mut tree = read_tree(book, &part).unwrap_or_else(|| root("rvStructures", NS_RICH, &[]));
    if let Some(i) = tree.elems().position(|s| s.attr("t") == Some(LOCAL_IMAGE)) {
        return i;
    }
    let index = bump_count(&mut tree);
    let mut s = Elem::new("s");
    s.set_attr("t", LOCAL_IMAGE);
    for (n, t) in [("_rvRel:LocalImageIdentifier", "i"), ("CalcOrigin", "i"), ("Text", "s")] {
        let mut k = Elem::new("k");
        k.set_attr("n", n);
        k.set_attr("t", t);
        s.push(k);
    }
    tree.push(s);
    write_tree(book, &part, &tree);
    index
}

/// Add an image relationship to the rich value relationships. Returns the index of the entry.
fn add_image_relationship(book: &mut Workbook, image_part: &str) -> usize {
    let part = ensure_part(book, REL_VALUE_REL, "xl/richData/richValueRel.xml", CT_VALUE_REL);
    let mut tree = read_tree(book, &part).unwrap_or_else(|| {
        let mut r = root("richValueRels", NS_RICH_REL, &[("xmlns:r", NS_REL)]);
        r.remove_attr("count");
        r
    });
    let rels_part = rels_path(&part);
    let mut rels = book.kept.get(&rels_part).map(|b| parse_rels(&String::from_utf8_lossy(b))).unwrap_or_default();
    let id = next_id(&rels);
    rels.push(Relationship { id: id.clone(), kind: REL_IMAGE.to_owned(), target: relative(&part, image_part), external: false });
    let index = tree.elems().count();
    let mut rel = Elem::new("rel");
    rel.set_attr("r:id", &id);
    tree.push(rel);
    write_tree(book, &part, &tree);
    book.kept.set(&rels_part, write_rels(&rels).into_bytes());
    index
}

/// Add a rich value for a local image. Returns its index.
fn add_rich_value(book: &mut Workbook, structure: usize, relationship: usize, alt: &str) -> usize {
    let part = ensure_part(book, REL_VALUE, "xl/richData/rdrichvalue.xml", CT_VALUE);
    let mut tree = read_tree(book, &part).unwrap_or_else(|| root("rvData", NS_RICH, &[]));
    let index = bump_count(&mut tree);
    let mut rv = Elem::new("rv");
    rv.set_attr("s", &structure.to_string());
    for text in [relationship.to_string(), "5".to_owned(), alt.to_owned()] {
        let mut v = Elem::new("v");
        v.set_text(&text);
        rv.push(v);
    }
    tree.push(rv);
    write_tree(book, &part, &tree);
    index
}

fn metadata_type(name: &str, cell_meta: bool) -> Elem {
    let mut t = Elem::new("metadataType");
    t.set_attr("name", name);
    t.set_attr("minSupportedVersion", "120000");
    for a in ["copy", "pasteAll", "pasteValues", "merge", "splitFirst", "rowColShift", "clearFormats", "clearComments", "assign", "coerce"] {
        t.set_attr(a, "1");
    }
    if cell_meta {
        t.set_attr("cellMeta", "1");
    }
    t
}

fn with_child(name: &str, child: Elem) -> Elem {
    let mut e = Elem::new(name);
    e.set_attr("count", "1");
    e.push(child);
    e
}

fn rich_value_block(rich_value: usize) -> Elem {
    let mut rvb = Elem::new("xlrd:rvb");
    rvb.set_attr("i", &rich_value.to_string());
    let mut ext = Elem::new("ext");
    ext.set_attr("uri", "{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}");
    ext.push(rvb);
    let mut list = Elem::new("extLst");
    list.push(ext);
    let mut bk = Elem::new("bk");
    bk.push(list);
    bk
}

fn value_metadata_block(type_index: usize, rich_value: usize) -> Elem {
    let mut rc = Elem::new("rc");
    rc.set_attr("t", &type_index.to_string());
    rc.set_attr("v", &rich_value.to_string());
    let mut bk = Elem::new("bk");
    bk.push(rc);
    bk
}

/// Metadata for a workbook that has none: the dynamic array type Excel always writes beside the rich value one.
fn new_metadata(book: &mut Workbook, rich_value: usize) -> Elem {
    let mut m = Elem::new("metadata");
    m.set_attr("xmlns", NS_MAIN);
    m.set_attr("xmlns:xlrd", NS_RICH);
    let mut types = Elem::new("metadataTypes");
    types.set_attr("count", "2");
    types.push(metadata_type("XLDAPR", true));
    types.push(metadata_type("XLRICHVALUE", false));
    m.push(types);
    let mut dyn_ext = Elem::new("ext");
    dyn_ext.set_attr("uri", "{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}");
    let mut props = Elem::new("xda:dynamicArrayProperties");
    props.set_attr("xmlns:xda", "http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray");
    props.set_attr("fDynamic", "1");
    props.set_attr("fCollapsed", "0");
    dyn_ext.push(props);
    let mut dyn_list = Elem::new("extLst");
    dyn_list.push(dyn_ext);
    let mut dyn_bk = Elem::new("bk");
    dyn_bk.push(dyn_list);
    let mut f1 = with_child("futureMetadata", dyn_bk);
    f1.set_attr("name", "XLDAPR");
    let mut f2 = with_child("futureMetadata", rich_value_block(rich_value));
    f2.set_attr("name", "XLRICHVALUE");
    m.push(f1);
    m.push(f2);
    m.push(with_child("cellMetadata", value_metadata_block(1, 0)));
    m.push(with_child("valueMetadata", value_metadata_block(2, 0)));
    // The first cell metadata entry is the dynamic array one, so a formula's `cm="1"` means a spill.
    book.dynamic_cm = Some(1);
    m
}

/// Add a rich value to existing metadata. Returns the index of the `valueMetadata` entry.
fn merge_metadata(m: &mut Elem, rich_value: usize) -> usize {
    let found = m.child("metadataTypes").and_then(|t| t.elems().position(|e| e.attr("name") == Some("XLRICHVALUE")));
    let type_index = match found {
        Some(i) => i + 1,
        None => {
            let mut n = 0;
            if let Some(t) = m.child_mut("metadataTypes") {
                bump_count(t);
                t.push(metadata_type("XLRICHVALUE", false));
                n = t.elems().count();
            }
            n
        }
    };
    if m.attr("xmlns:xlrd").is_none() {
        m.set_attr("xmlns:xlrd", NS_RICH);
    }
    let is_future = |e: &Elem| e.local() == "futureMetadata" && e.attr("name") == Some("XLRICHVALUE");
    let exists = m.elems().any(is_future);
    let future_index = if exists {
        let f = m.elems_mut().find(|e| is_future(e)).expect("checked above");
        let n = bump_count(f);
        f.push(rich_value_block(rich_value));
        n
    } else {
        {
            let mut f = with_child("futureMetadata", rich_value_block(rich_value));
            f.set_attr("name", "XLRICHVALUE");
            let last = m.children.iter().rposition(|n| {
                matches!(n, xmltree::Node::Elem(e) if matches!(e.local(), "metadataTypes" | "futureMetadata"))
            });
            m.children.insert(last.map_or(0, |i| i + 1), xmltree::Node::Elem(f));
            0
        }
    };
    match m.child_mut("valueMetadata") {
        Some(v) => {
            let n = bump_count(v);
            v.push(value_metadata_block(type_index, future_index));
            n
        }
        None => {
            m.push(with_child("valueMetadata", value_metadata_block(type_index, future_index)));
            0
        }
    }
}

/// Place an image in a cell: the cell shows the picture and holds `#VALUE!`, as Excel's Place in Cell leaves it.
pub fn place(book: &mut Workbook, sheet: usize, at: CellRef, image_part: &str, alt: &str) -> Result<(), String> {
    if book.sheets[sheet].cells.get(&(at.row, at.col)).is_some_and(|c| !c.value.is_empty() || c.formula.is_some()) {
        return Err("Place in Cell needs an empty cell.".to_owned());
    }
    let structure = local_image_structure(book);
    let relationship = add_image_relationship(book, image_part);
    let rich_value = add_rich_value(book, structure, relationship, alt);
    let meta_part = ensure_part(book, REL_METADATA, "xl/metadata.xml", CT_METADATA);
    let vm = match read_tree(book, &meta_part) {
        Some(mut m) => {
            let v = merge_metadata(&mut m, rich_value);
            write_tree(book, &meta_part, &m);
            v
        }
        None => {
            let m = new_metadata(book, rich_value);
            write_tree(book, &meta_part, &m);
            0
        }
    };
    ensure_part(book, REL_TYPES, "xl/richData/rdRichValueTypes.xml", CT_TYPES);
    if book.kept.get("xl/richData/rdRichValueTypes.xml").is_none() && part_of(book, REL_TYPES).as_deref() == Some("xl/richData/rdRichValueTypes.xml") {
        book.kept.set("xl/richData/rdRichValueTypes.xml", TYPES_XML.as_bytes().to_vec());
    }
    let cell = book.sheets[sheet].cells.entry((at.row, at.col)).or_default();
    cell.value = Value::Error(ErrorKind::Value);
    cell.extra = Some(Arc::from(format!(" vm=\"{}\"", vm + 1).as_str()));
    Ok(())
}

/// The image part a cell shows, when the cell holds a picture placed in it.
pub fn cell_image(book: &Workbook, sheet: usize, at: CellRef) -> Option<String> {
    let extra = book.sheets.get(sheet)?.cells.get(&(at.row, at.col))?.extra.as_deref()?;
    let vm: usize = extra.split_whitespace().find_map(|a| a.strip_prefix("vm=\"")?.strip_suffix('"')?.parse().ok())?;
    let meta = read_tree(book, &part_of(book, REL_METADATA)?)?;
    let value_meta = meta.child("valueMetadata")?;
    let rc = value_meta.elems().nth(vm.checked_sub(1)?)?.child("rc")?;
    let (t, v): (usize, usize) = (rc.attr("t")?.parse().ok()?, rc.attr("v")?.parse().ok()?);
    let name = meta.child("metadataTypes")?.elems().nth(t.checked_sub(1)?)?.attr("name")?.to_owned();
    if name != "XLRICHVALUE" {
        return None;
    }
    let future = meta.elems().find(|e| e.local() == "futureMetadata" && e.attr("name") == Some("XLRICHVALUE"))?;
    let rich_index: usize = future.elems().nth(v)?.find("rvb")?.attr("i")?.parse().ok()?;
    let values = read_tree(book, &part_of(book, REL_VALUE)?)?;
    let rv = values.elems().nth(rich_index)?;
    let rel_index: usize = rv.elems().next()?.text().trim().parse().ok()?;
    let rel_part = part_of(book, REL_VALUE_REL)?;
    let rels_tree = read_tree(book, &rel_part)?;
    let id = rels_tree.elems().nth(rel_index)?.attr_local("id")?.to_owned();
    let rels = parse_rels(&String::from_utf8_lossy(book.kept.get(&rels_path(&rel_part))?));
    let target = rels.iter().find(|r| r.id == id)?;
    Some(resolve(&rel_part, &target.target))
}
