//! Writing slicers and timelines into the workbook before it is saved: the parts, their relationships and
//! content types, the extension entries that point at them, and the defined names Excel gives each cache.
//!
//! [`synced`] returns a copy of the workbook in which all of that is made from the model, so the writer
//! writes it like any kept part. A workbook whose slicers did not change is not copied.

use std::collections::BTreeSet;

use super::xml::{cache_xml, slicers_xml, timeline_cache_xml, timelines_xml};
use super::*;
use crate::model::{DefinedName, Relationship, Workbook};
use crate::xlsx::{next_id, parse_rels, relative, resolve};

const X14_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const X15_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
const URI_SLICER_CACHES: &str = "{BBE1A952-AA13-448e-AADC-164F8A28A991}";
const URI_TIMELINE_CACHES: &str = "{D0CA8CA8-9F24-4464-BF8E-62219DCF47F9}";
const URI_SLICER_LIST: &str = "{A8765BA9-456A-4dab-B4F3-ACF838C121DE}";
const URI_TIMELINE_LIST: &str = "{7E03D99C-DC04-49d9-9315-930204A7B6E9}";

/// Whether the parts have to be made again.
pub fn needs_sync(book: &Workbook) -> bool {
    book.slicers.changed
}

/// The workbook with the slicer and timeline parts made from the model.
pub fn synced(book: &Workbook) -> Workbook {
    let mut copy = book.clone();
    forget_what_has_no_object(&mut copy);
    rewrite_pivot_parts(&mut copy);
    write_caches(&mut copy);
    write_lists(&mut copy);
    copy.slicers.changed = false;
    copy
}

/// The pivot parts a slicer or timeline is tied to are written again, because choosing items lists the
/// field's items in the table and shares them in the cache, which the parts written earlier do not have.
fn rewrite_pivot_parts(book: &mut Workbook) {
    let names: Vec<String> = book
        .slicers
        .caches
        .iter()
        .flat_map(|c| c.pivots.clone())
        .chain(book.slicers.timeline_caches.iter().flat_map(|c| c.pivots.clone()))
        .collect();
    let mut tables: Vec<usize> = names.iter().filter_map(|n| book.pivots.find(n)).collect();
    tables.sort_unstable();
    tables.dedup();
    let mut caches: Vec<usize> = tables.iter().map(|t| book.pivots.tables[*t].cache).collect();
    caches.sort_unstable();
    caches.dedup();
    for c in caches {
        crate::pivot::xml::sync_cache(book, c);
    }
    for t in tables {
        crate::pivot::xml::sync_table(book, t);
    }
}

// ------------------------------------------------------------------------------------------ the drawing

/// The sheet whose drawing holds the graphic frame of a slicer or a timeline: an element named `slicer` or
/// `timeslicer` that carries the name.
pub fn home_of(book: &Workbook, local: &str, name: &str) -> Option<usize> {
    book.sheets.iter().position(|s| {
        s.drawing
            .objects
            .iter()
            .any(|o| o.tree.find(local).is_some_and(|e| e.attr("name") == Some(name)))
    })
}

/// A slicer or a timeline lives while its drawing object does, and a cache while a slicer uses it.
fn forget_what_has_no_object(book: &mut Workbook) {
    let slicers = book.slicers.slicers.clone();
    let kept: Vec<Slicer> = slicers
        .into_iter()
        .filter_map(|mut s| {
            s.sheet = home_of(book, "slicer", &s.name)?;
            Some(s)
        })
        .collect();
    let timelines = book.slicers.timelines.clone();
    let kept_timelines: Vec<Timeline> = timelines
        .into_iter()
        .filter_map(|mut t| {
            t.sheet = home_of(book, "timeslicer", &t.name)?;
            Some(t)
        })
        .collect();
    let s = &mut book.slicers;
    s.slicers = kept;
    s.timelines = kept_timelines;
    let (slicers, timelines) = (s.slicers.clone(), s.timelines.clone());
    s.caches.retain(|c| slicers.iter().any(|x| x.cache.eq_ignore_ascii_case(&c.name)));
    s.timeline_caches.retain(|c| timelines.iter().any(|x| x.cache.eq_ignore_ascii_case(&c.name)));
}

// --------------------------------------------------------------------------------------------- caches

/// The pivot tables a cache filters, with the number of the sheet each is on, and the number the pivot
/// cache is known by.
fn pivots_of(book: &Workbook, names: &[String]) -> Option<(Vec<(u32, String)>, usize, u64)> {
    let tables: Vec<usize> = names.iter().filter_map(|n| book.pivots.find(n)).collect();
    let first = *tables.first()?;
    let cache = book.pivots.tables[first].cache;
    let id = book.pivots.caches[cache]
        .slicer_id
        .unwrap_or(1_000_000_000 + u64::from(book.pivots.caches[cache].id) * 7919);
    let list = tables
        .iter()
        .map(|t| (book.pivots.tables[*t].sheet as u32 + 1, book.pivots.tables[*t].name.clone()))
        .collect();
    Some((list, first, id))
}

/// The items of a slicer cache in the order the slicer lists them, with whether each is selected.
pub fn items_of(book: &Workbook, cache: &SlicerCache, table: usize) -> Vec<(usize, bool)> {
    let t = &book.pivots.tables[table];
    let pc = &book.pivots.caches[t.cache];
    let Some(f) = pc.fields.iter().position(|f| f.name.eq_ignore_ascii_case(&cache.source)) else {
        return Vec::new();
    };
    let source = pc.fields[f].items();
    let mut order: Vec<usize> = (0..source.len()).collect();
    order.sort_by(|a, b| source[*a].order(&source[*b]));
    if cache.tabular.iter().any(|(k, v)| k == "sortOrder" && v == "descending") {
        order.reverse();
    }
    order
        .into_iter()
        .map(|x| {
            let shown = t.fields[f].items.iter().find(|e| e.x == x).is_none_or(|e| !e.hidden);
            (x, shown)
        })
        .collect()
}

fn fresh_part(book: &Workbook, pattern: &str) -> String {
    let taken: BTreeSet<&String> = book.kept.parts.iter().map(|(p, _)| p).collect();
    (1..)
        .map(|n| pattern.replace("{}", &n.to_string()))
        .find(|p| !taken.contains(p))
        .unwrap_or_default()
}

fn set_override(book: &mut Workbook, part: &str, content_type: &str) {
    let key = format!("/{part}");
    if !book.kept.overrides.iter().any(|(p, _)| *p == key) {
        book.kept.overrides.push((key, content_type.to_owned()));
    }
}

fn workbook_part(book: &Workbook) -> String {
    if book.part.is_empty() { "xl/workbook.xml".to_owned() } else { book.part.clone() }
}

/// The relationship of a workbook part of this kind that points at `part`, made when there is none.
fn workbook_rel(book: &mut Workbook, kind: &str, part: &str) -> (String, bool) {
    let from = workbook_part(book);
    if let Some(r) = book.kept.workbook_rels.iter().find(|r| r.kind == kind && resolve(&from, &r.target) == part) {
        return (r.id.clone(), false);
    }
    let id = next_id(&book.kept.workbook_rels);
    book.kept.workbook_rels.push(Relationship {
        id: id.clone(),
        kind: kind.to_owned(),
        target: relative(&from, part),
        external: false,
    });
    (id, true)
}

fn ensure_name(book: &mut Workbook, name: &str) {
    if !book.names.iter().any(|n| n.scope.is_none() && n.name.eq_ignore_ascii_case(name)) {
        book.names.push(DefinedName {
            name: name.to_owned(),
            scope: None,
            formula: "#N/A".to_owned(),
            hidden: false,
            attrs: Vec::new(),
        });
    }
}

/// The workbook parts of every slicer cache and timeline cache, and the ones that are gone.
fn write_caches(book: &mut Workbook) {
    let from = workbook_part(book);
    let mut live: BTreeSet<String> = BTreeSet::new();
    for i in 0..book.slicers.caches.len() {
        let cache = book.slicers.caches[i].clone();
        let Some((pivots, table, id)) = pivots_of(book, &cache.pivots) else { continue };
        let items = items_of(book, &cache, table);
        let part = if cache.part.is_empty() { fresh_part(book, "xl/slicerCaches/slicerCache{}.xml") } else { cache.part.clone() };
        book.kept.set(&part, cache_xml(&cache, id, &pivots, &items).into_bytes());
        set_override(book, &part, CT_SLICER_CACHE);
        let (rid, added) = workbook_rel(book, REL_SLICER_CACHE, &part);
        if added {
            let entry = format!("<x14:slicerCache r:id=\"{rid}\"/>");
            add_to_ext(workbook_ext(book), URI_SLICER_CACHES, ("x14", X14_NS), "x14:slicerCaches", &entry);
        }
        ensure_name(book, &cache.name);
        book.slicers.caches[i].part = part.clone();
        live.insert(part);
    }
    for i in 0..book.slicers.timeline_caches.len() {
        let cache = book.slicers.timeline_caches[i].clone();
        let Some((pivots, _, id)) = pivots_of(book, &cache.pivots) else { continue };
        let part = if cache.part.is_empty() { fresh_part(book, "xl/timelineCaches/timelineCache{}.xml") } else { cache.part.clone() };
        book.kept.set(&part, timeline_cache_xml(&cache, id, &pivots).into_bytes());
        set_override(book, &part, CT_TIMELINE_CACHE);
        let (rid, added) = workbook_rel(book, REL_TIMELINE_CACHE, &part);
        if added {
            let entry = format!("<x15:timelineCacheRef r:id=\"{rid}\"/>");
            add_to_ext(workbook_ext(book), URI_TIMELINE_CACHES, ("x15", X15_NS), "x15:timelineCacheRefs", &entry);
        }
        ensure_name(book, &cache.name);
        book.slicers.timeline_caches[i].part = part.clone();
        live.insert(part);
    }
    drop_dead_workbook_parts(book, &from, &live);
}

/// Take away the cache parts the model no longer has, with their relationships and entries.
fn drop_dead_workbook_parts(book: &mut Workbook, from: &str, live: &BTreeSet<String>) {
    let dead: Vec<Relationship> = book
        .kept
        .workbook_rels
        .iter()
        .filter(|r| (r.kind == REL_SLICER_CACHE || r.kind == REL_TIMELINE_CACHE) && !live.contains(&resolve(from, &r.target)))
        .cloned()
        .collect();
    for r in dead {
        let part = resolve(from, &r.target);
        book.kept.remove(&part);
        book.kept.overrides.retain(|(p, _)| *p != format!("/{part}"));
        book.kept.workbook_rels.retain(|x| x.id != r.id);
        remove_from_ext(workbook_ext(book), &r.id);
    }
    let names: BTreeSet<String> = book
        .slicers
        .caches
        .iter()
        .map(|c| c.name.clone())
        .chain(book.slicers.timeline_caches.iter().map(|c| c.name.clone()))
        .collect();
    book.names.retain(|n| {
        let ours = n.formula == "#N/A" && (n.name.starts_with("Slicer_") || n.name.starts_with("NativeTimeline_"));
        !ours || names.contains(&n.name)
    });
}

fn workbook_ext(book: &mut Workbook) -> &mut String {
    ext_of(&mut book.raw)
}

fn ext_of(list: &mut Vec<(String, String)>) -> &mut String {
    if !list.iter().any(|(n, _)| n == "extLst") {
        list.push(("extLst".to_owned(), "<extLst></extLst>".to_owned()));
    }
    &mut list.iter_mut().find(|(n, _)| n == "extLst").expect("just made").1
}

// ----------------------------------------------------------------------------------------- the sheets

/// The `slicers` and `timelines` part of every sheet, made from the slicers whose objects are on it.
fn write_lists(book: &mut Workbook) {
    for sheet in 0..book.sheets.len() {
        let slicers: Vec<Slicer> = book.slicers.slicers.iter().filter(|s| s.sheet == sheet).cloned().collect();
        let timelines: Vec<Timeline> = book.slicers.timelines.iter().filter(|s| s.sheet == sheet).cloned().collect();
        write_list(book, sheet, REL_SLICER, CT_SLICER, "xl/slicers/slicer{}.xml", URI_SLICER_LIST, ("x14", X14_NS), "x14:slicerList", "x14:slicer",
            (!slicers.is_empty()).then(|| slicers_xml(&slicers.iter().collect::<Vec<_>>())));
        write_list(book, sheet, REL_TIMELINE, CT_TIMELINE, "xl/timelines/timeline{}.xml", URI_TIMELINE_LIST, ("x15", X15_NS), "x15:timelineRefs", "x15:timelineRef",
            (!timelines.is_empty()).then(|| timelines_xml(&timelines.iter().collect::<Vec<_>>())));
    }
}

/// One sheet's part of one kind: written when there is XML for it, taken away when there is none.
#[allow(clippy::too_many_arguments)]
fn write_list(
    book: &mut Workbook,
    sheet: usize,
    kind: &str,
    content_type: &str,
    pattern: &str,
    uri: &str,
    ns: (&str, &str),
    list_tag: &str,
    entry_tag: &str,
    xml: Option<String>,
) {
    let from = book.sheets[sheet].raw.part.clone();
    let existing = book.sheets[sheet].raw.rels.iter().find(|r| r.kind == kind).cloned();
    match (xml, existing) {
        (Some(xml), Some(rel)) => {
            let part = resolve(&from, &rel.target);
            book.kept.set(&part, xml.into_bytes());
            set_override(book, &part, content_type);
        }
        (Some(xml), None) => {
            let part = fresh_part(book, pattern);
            book.kept.set(&part, xml.into_bytes());
            set_override(book, &part, content_type);
            let sheet_part = if from.is_empty() { "xl/worksheets/sheet1.xml".to_owned() } else { from };
            let id = next_id(&book.sheets[sheet].raw.rels);
            book.sheets[sheet].raw.rels.push(Relationship {
                id: id.clone(),
                kind: kind.to_owned(),
                target: relative(&sheet_part, &part),
                external: false,
            });
            ensure_relationship_prefix(book, sheet);
            let entry = format!("<{entry_tag} r:id=\"{id}\"/>");
            add_to_ext(ext_of(&mut book.sheets[sheet].raw.elements), uri, ns, list_tag, &entry);
        }
        (None, Some(rel)) => {
            let part = resolve(&from, &rel.target);
            book.kept.remove(&part);
            book.kept.overrides.retain(|(p, _)| *p != format!("/{part}"));
            book.sheets[sheet].raw.rels.retain(|r| r.id != rel.id);
            remove_from_ext(ext_of(&mut book.sheets[sheet].raw.elements), &rel.id);
        }
        (None, None) => {}
    }
}

/// The sheet's root must declare the prefix `r:` that the extension entries use.
fn ensure_relationship_prefix(book: &mut Workbook, sheet: usize) {
    let ns = &mut book.sheets[sheet].raw.namespaces;
    if !ns.iter().any(|(_, uri)| uri == crate::xlsx::REL_NS) {
        ns.push(("r".to_owned(), crate::xlsx::REL_NS.to_owned()));
    }
}

// --------------------------------------------------------------------------------- extension surgery

/// Put an entry into the extension of an `extLst` that has this uri, making the extension when it is not there.
pub fn add_to_ext(list: &mut String, uri: &str, ns: (&str, &str), list_tag: &str, entry: &str) {
    let close = format!("</{list_tag}>");
    if let Some(at) = list.find(&format!("uri=\"{uri}\"")) {
        if let Some(end) = list[at..].find(&close) {
            list.insert_str(at + end, entry);
            return;
        }
    }
    let ext = format!(
        "<ext uri=\"{uri}\" xmlns:{}=\"{}\"><{list_tag}>{entry}</{list_tag}></ext>",
        ns.0, ns.1
    );
    match list.rfind("</extLst>") {
        Some(at) => list.insert_str(at, &ext),
        None => *list = format!("<extLst>{ext}</extLst>"),
    }
}

/// Take away the entry that names a relationship, and the extension when it holds no entry any more.
pub fn remove_from_ext(list: &mut String, rid: &str) {
    let key = format!("r:id=\"{rid}\"");
    let Some(at) = list.find(&key) else { return };
    let Some(start) = list[..at].rfind('<') else { return };
    let Some(end) = list[at..].find("/>") else { return };
    list.replace_range(start..at + end + 2, "");
    // An extension of ours with nothing left in it goes too.
    for uri in [URI_SLICER_CACHES, URI_TIMELINE_CACHES, URI_SLICER_LIST, URI_TIMELINE_LIST] {
        let Some(open) = list.find(&format!("<ext uri=\"{uri}\"")) else { continue };
        let Some(close) = list[open..].find("</ext>") else { continue };
        let body = &list[open..open + close];
        if !body.contains("r:id=") {
            list.replace_range(open..open + close + 6, "");
        }
    }
}

/// The parts the sheet's relationships name, for a check that none was left behind.
pub fn rel_parts(book: &Workbook, sheet: usize, kind: &str) -> Vec<String> {
    let from = &book.sheets[sheet].raw.part;
    book.sheets[sheet]
        .raw
        .rels
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| resolve(from, &r.target))
        .collect()
}

/// Parse relationships for a kept `.rels` part, for tests of the written file.
pub fn rels_of(book: &Workbook, rels_part: &str) -> Vec<Relationship> {
    book.kept.get(rels_part).map(|b| parse_rels(&String::from_utf8_lossy(b))).unwrap_or_default()
}
