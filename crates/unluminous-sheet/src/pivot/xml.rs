//! Reading and writing the parts of a pivot table: `pivotCacheDefinition`, `pivotCacheRecords` and
//! `pivotTableDefinition`, and keeping the workbook's own pointers to them right.
//!
//! The workbook keeps every part it does not regenerate as bytes, with its relationships and content type.
//! A pivot the model understands is read from those bytes when the file is opened and left as they are until
//! a change makes the model differ; a change writes the new bytes into the same place. The workbook's
//! `pivotCaches` element, the relationships and the content types are kept in step here, so the file writer
//! needs no knowledge of pivots at all.

use super::compute::Calc;
use super::layout::lay_out;
use super::xml_read::{
    join_path, listed_caches, read_cache, read_table, rels_path, REL_CACHE_DEFINITION,
    REL_CACHE_RECORDS, REL_PIVOT_TABLE,
};
use super::xml_write::{
    cache_definition, cache_records, cache_rels, table_definition, table_rels, used_fields, XML_HEAD,
};
use crate::model::{Relationship, Workbook};
use crate::xlsx::{escape, REL_NS};

const CT_CACHE_DEFINITION: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const CT_CACHE_RECORDS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";
const CT_PIVOT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";

/// Read the pivot tables a file has into the model. A cache or a table this version does not understand
/// stays as the bytes it was.
pub fn attach(book: &mut Workbook) {
    let workbook_part = if book.part.is_empty() { "xl/workbook.xml".to_owned() } else { book.part.clone() };
    for (id, rid) in listed_caches(book) {
        let Some(rel) = book.kept.workbook_rels.iter().find(|r| r.id == rid && r.kind == REL_CACHE_DEFINITION)
        else {
            continue;
        };
        let definition = join_path(&workbook_part, &rel.target);
        if let Some(cache) = read_cache(book, id, &definition) {
            book.pivots.caches.push(cache);
        }
    }
    if book.pivots.caches.is_empty() {
        return;
    }
    for sheet in 0..book.sheets.len() {
        let part = book.sheets[sheet].raw.part.clone();
        let rels: Vec<Relationship> =
            book.sheets[sheet].raw.rels.iter().filter(|r| r.kind == REL_PIVOT_TABLE).cloned().collect();
        for rel in rels {
            let table_part = join_path(&part, &rel.target);
            if let Some(table) = read_table(book, sheet, &table_part) {
                book.pivots.tables.push(table);
            }
        }
    }
}

/// A part path that is not in use, numbered from 1.
fn fresh_part(book: &Workbook, pattern: &str) -> String {
    (1..)
        .map(|n| pattern.replace("{}", &n.to_string()))
        .find(|p| {
            book.kept.get(p).is_none()
                && !book.pivots.caches.iter().any(|c| c.definition_part == *p || c.records_part == *p)
                && !book.pivots.tables.iter().any(|t| t.part == *p)
        })
        .unwrap_or_default()
}

/// The target of a relationship from one part to another.
pub fn relative(from: &str, to: &str) -> String {
    let from_dir: Vec<&str> = from.split('/').collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let dir = &from_dir[..from_dir.len().saturating_sub(1)];
    let mut common = 0;
    while common < dir.len() && common < to_parts.len() - 1 && dir[common] == to_parts[common] {
        common += 1;
    }
    let mut out: Vec<String> = Vec::new();
    for _ in common..dir.len() {
        out.push("..".to_owned());
    }
    out.extend(to_parts[common..].iter().map(|s| (*s).to_owned()));
    out.join("/")
}

fn set_override(book: &mut Workbook, part: &str, content_type: &str) {
    let key = format!("/{part}");
    match book.kept.overrides.iter_mut().find(|(p, _)| *p == key) {
        Some((_, ct)) => *ct = content_type.to_owned(),
        None => book.kept.overrides.push((key, content_type.to_owned())),
    }
}

fn next_rel_id(rels: &[Relationship]) -> String {
    let max = rels
        .iter()
        .filter_map(|r| r.id.strip_prefix("rId").and_then(|n| n.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    format!("rId{}", max + 1)
}

/// Bring the parts of a cache up to date with the model.
pub fn sync_cache(book: &mut Workbook, c: usize) {
    if book.pivots.caches[c].definition_part.is_empty() {
        let part = fresh_part(book, "xl/pivotCache/pivotCacheDefinition{}.xml");
        book.pivots.caches[c].definition_part = part;
    }
    if book.pivots.caches[c].records_part.is_empty() {
        let n = book.pivots.caches[c]
            .definition_part
            .trim_start_matches("xl/pivotCache/pivotCacheDefinition")
            .trim_end_matches(".xml")
            .to_owned();
        let wanted = format!("xl/pivotCache/pivotCacheRecords{n}.xml");
        let part = if book.kept.get(&wanted).is_none() {
            wanted
        } else {
            fresh_part(book, "xl/pivotCache/pivotCacheRecords{}.xml")
        };
        book.pivots.caches[c].records_part = part;
    }
    let cache = book.pivots.caches[c].clone();
    let used = used_fields(c, &book.pivots.tables, cache.fields.len());
    let definition = cache_definition(&cache, &used, book.date1904, "rId1");
    let records = cache_records(&cache, &used, book.date1904);
    book.kept.set(&cache.definition_part, definition.into_bytes());
    book.kept.set(&cache.records_part, records.into_bytes());
    let rels = cache_rels(&relative(&cache.definition_part, &cache.records_part));
    book.kept.set(&rels_path(&cache.definition_part), rels.into_bytes());
    set_override(book, &cache.definition_part, CT_CACHE_DEFINITION);
    set_override(book, &cache.records_part, CT_CACHE_RECORDS);
    ensure_workbook_rel(book, &cache.definition_part);
    rewrite_cache_list(book);
}

fn workbook_part(book: &Workbook) -> String {
    if book.part.is_empty() {
        "xl/workbook.xml".to_owned()
    } else {
        book.part.clone()
    }
}

fn ensure_workbook_rel(book: &mut Workbook, definition: &str) {
    let wb = workbook_part(book);
    let present = book
        .kept
        .workbook_rels
        .iter()
        .any(|r| r.kind == REL_CACHE_DEFINITION && join_path(&wb, &r.target) == definition);
    if !present {
        let id = next_rel_id(&book.kept.workbook_rels);
        book.kept.workbook_rels.push(Relationship {
            id,
            kind: REL_CACHE_DEFINITION.to_owned(),
            target: relative(&wb, definition),
            external: false,
        });
    }
}

/// Write the workbook's `pivotCaches` element: the caches the model holds, then the ones it does not.
fn rewrite_cache_list(book: &mut Workbook) {
    let wb = workbook_part(book);
    let modelled: Vec<u32> = book.pivots.caches.iter().map(|c| c.id).collect();
    let mut entries: Vec<(u32, String)> = listed_caches(book)
        .into_iter()
        .filter(|(id, _)| !modelled.contains(id))
        .filter(|(_, rid)| book.kept.workbook_rels.iter().any(|r| r.id == *rid))
        .collect();
    for cache in &book.pivots.caches {
        let rid = book
            .kept
            .workbook_rels
            .iter()
            .find(|r| r.kind == REL_CACHE_DEFINITION && join_path(&wb, &r.target) == cache.definition_part)
            .map(|r| r.id.clone());
        if let Some(rid) = rid {
            entries.push((cache.id, rid));
        }
    }
    entries.sort_by_key(|(id, _)| *id);
    let mut xml = String::from("<pivotCaches>");
    for (id, rid) in &entries {
        xml.push_str(&format!("<pivotCache cacheId=\"{id}\" r:id=\"{}\"/>", escape(rid)));
    }
    xml.push_str("</pivotCaches>");
    let _ = REL_NS;
    match book.raw.iter_mut().find(|(n, _)| n == "pivotCaches") {
        Some((_, existing)) => {
            if entries.is_empty() {
                *existing = String::new();
            } else {
                *existing = xml;
            }
        }
        None => {
            if !entries.is_empty() {
                book.raw.push(("pivotCaches".to_owned(), xml));
            }
        }
    }
    book.raw.retain(|(n, x)| !(n == "pivotCaches" && x.is_empty()));
    // The root element must declare the prefix the entries use.
    if !book.namespaces.iter().any(|(p, _)| p == "r") {
        book.namespaces.push(("r".to_owned(), REL_NS.to_owned()));
    }
}

/// Bring the part of a table up to date with the model.
pub fn sync_table(book: &mut Workbook, t: usize) {
    if book.pivots.tables[t].lines.rows.is_empty()
        && book.pivots.tables[t].lines.cols.is_empty()
        && (!book.pivots.tables[t].data.is_empty() || !book.pivots.tables[t].rows.is_empty())
    {
        let laid = {
            let table = &book.pivots.tables[t];
            let cache = &book.pivots.caches[table.cache];
            lay_out(&Calc::build(table, cache, book.date1904)).lines
        };
        book.pivots.tables[t].lines = laid;
    }
    let part_is_new = book.pivots.tables[t].part.is_empty();
    if part_is_new {
        let part = fresh_part(book, "xl/pivotTables/pivotTable{}.xml");
        book.pivots.tables[t].part = part;
    }
    let table = book.pivots.tables[t].clone();
    let cache = book.pivots.caches[table.cache].clone();
    if cache.definition_part.is_empty() {
        sync_cache(book, table.cache);
    }
    let cache = book.pivots.caches[table.cache].clone();
    let xml = table_definition(&table, &cache, &mut book.styles, cache.id);
    book.kept.set(&table.part, xml.into_bytes());
    let rels = table_rels(&relative(&table.part, &cache.definition_part));
    book.kept.set(&rels_path(&table.part), rels.into_bytes());
    set_override(book, &table.part, CT_PIVOT_TABLE);
    ensure_sheet_rel(book, table.sheet, &table.part);
    book.pivots.tables[t].dirty = false;
}

fn ensure_sheet_rel(book: &mut Workbook, sheet: usize, part: &str) {
    let base = if book.sheets[sheet].raw.part.is_empty() {
        "xl/worksheets/sheet.xml".to_owned()
    } else {
        book.sheets[sheet].raw.part.clone()
    };
    let rels = &mut book.sheets[sheet].raw.rels;
    let present = rels.iter().any(|r| r.kind == REL_PIVOT_TABLE && join_path(&base, &r.target) == part);
    if !present {
        let id = next_rel_id(rels);
        rels.push(Relationship {
            id,
            kind: REL_PIVOT_TABLE.to_owned(),
            target: relative(&base, part),
            external: false,
        });
    }
}

/// After tables or caches were removed: take their parts and pointers out, and write the cache list.
pub fn sync_all(book: &mut Workbook) {
    let removed = std::mem::take(&mut book.pivots.removed_parts);
    let wb = workbook_part(book);
    for part in &removed {
        book.kept.remove(part);
        book.kept.remove(&rels_path(part));
        for sheet in &mut book.sheets {
            let base = if sheet.raw.part.is_empty() { "xl/worksheets/sheet.xml".to_owned() } else { sheet.raw.part.clone() };
            sheet.raw.rels.retain(|r| !(r.kind == REL_PIVOT_TABLE && join_path(&base, &r.target) == *part));
        }
        book.kept.workbook_rels.retain(|r| !(r.kind == REL_CACHE_DEFINITION && join_path(&wb, &r.target) == *part));
    }
    // Cache ids that are no longer in the model and have no definition left are dropped from the list.
    rewrite_cache_list(book);
    let valid: Vec<String> = book
        .kept
        .workbook_rels
        .iter()
        .filter(|r| r.kind == REL_CACHE_DEFINITION)
        .map(|r| r.id.clone())
        .collect();
    if let Some((_, xml)) = book.raw.iter_mut().find(|(n, _)| n == "pivotCaches") {
        let _ = (&valid, &xml);
    }
    let _ = (XML_HEAD, REL_CACHE_RECORDS);
}

/// Bring the parts of every cache and table that changed up to date, after a change around them.
pub fn sync_dirty(book: &mut Workbook) {
    if !book.pivots.removed_parts.is_empty() {
        sync_all(book);
    }
    for c in 0..book.pivots.caches.len() {
        if book.pivots.caches[c].dirty {
            sync_cache(book, c);
            book.pivots.caches[c].dirty = false;
        }
    }
    for t in 0..book.pivots.tables.len() {
        if book.pivots.tables[t].dirty {
            sync_table(book, t);
        }
    }
}
