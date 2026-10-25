//! Reading and writing the parts of slicers and timelines: `slicerCacheDefinition`, `slicers`,
//! `timelineCacheDefinition` and `timelines`.

use roxmltree::{Document, Node};

use super::{Slicer, SlicerCache, Slicers, Timeline, TimelineCache};
use super::{REL_SLICER, REL_SLICER_CACHE, REL_TIMELINE, REL_TIMELINE_CACHE};
use crate::model::Workbook;
use crate::xlsx::{escape, resolve};

const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
const XR10: &str = "http://schemas.microsoft.com/office/spreadsheetml/2016/revision10";
const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// The attributes of an element that have no namespace, except the ones named, in file order.
fn plain_attrs(n: Node, skip: &[&str]) -> Vec<(String, String)> {
    n.attributes()
        .filter(|a| a.namespace().is_none() && !skip.contains(&a.name()))
        .map(|a| (a.name().to_owned(), a.value().to_owned()))
        .collect()
}

fn uid_of(n: Node) -> Option<String> {
    n.attribute((XR10, "uid")).map(str::to_owned)
}

/// What is inside an element, as written, for the parts of it this version does not model.
fn inner(xml: &str, n: Node) -> String {
    n.children().filter(|c| c.is_element()).map(|c| xml[c.range()].to_owned()).collect()
}

fn pivot_names(root: Node) -> Vec<String> {
    child(root, "pivotTables")
        .map(|p| {
            p.children()
                .filter(|c| c.is_element() && c.tag_name().name() == "pivotTable")
                .filter_map(|c| c.attribute("name").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn text_of(book: &Workbook, part: &str) -> Option<String> {
    book.kept.get(part).map(|b| String::from_utf8_lossy(b).into_owned())
}

/// Read every slicer, timeline and cache the workbook's relationships name into the model. The parts stay
/// where they are in the kept parts; they are written again only when the model changes.
pub fn attach(book: &mut Workbook) {
    let workbook_part =
        if book.part.is_empty() { "xl/workbook.xml".to_owned() } else { book.part.clone() };
    let rels = book.kept.workbook_rels.clone();
    let mut found = Slicers::default();
    for r in &rels {
        let part = resolve(&workbook_part, &r.target);
        let Some(xml) = text_of(book, &part) else { continue };
        if r.kind == REL_SLICER_CACHE {
            found.caches.extend(read_cache(&xml, &part));
        } else if r.kind == REL_TIMELINE_CACHE {
            found.timeline_caches.extend(read_timeline_cache(&xml, &part));
        }
    }
    for sheet in 0..book.sheets.len() {
        let part = book.sheets[sheet].raw.part.clone();
        for r in book.sheets[sheet].raw.rels.clone() {
            let target = resolve(&part, &r.target);
            let Some(xml) = text_of(book, &target) else { continue };
            if r.kind == REL_SLICER {
                found.slicers.extend(read_slicers(&xml, sheet));
            } else if r.kind == REL_TIMELINE {
                found.timelines.extend(read_timelines(&xml, sheet));
            }
        }
    }
    book.slicers = found;
}

/// A slicer cache over a pivot field, or `None` for a kind this version does not model (an OLAP source).
pub fn read_cache(xml: &str, part: &str) -> Option<SlicerCache> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "slicerCacheDefinition" {
        return None;
    }
    let tabular = child(child(root, "data")?, "tabular")?;
    let no_data = child(tabular, "items")
        .map(|items| {
            items
                .children()
                .filter(|c| c.is_element() && c.attribute("nd") == Some("1"))
                .filter_map(|c| c.attribute("x").and_then(|x| x.parse().ok()))
                .collect()
        })
        .unwrap_or_default();
    Some(SlicerCache {
        name: root.attribute("name")?.to_owned(),
        source: root.attribute("sourceName")?.to_owned(),
        pivots: pivot_names(root),
        part: part.to_owned(),
        tabular: plain_attrs(tabular, &["pivotCacheId"]),
        uid: uid_of(root),
        no_data,
    })
}

/// The slicers of a `slicers` part.
pub fn read_slicers(xml: &str, sheet: usize) -> Vec<Slicer> {
    let Ok(doc) = Document::parse(xml) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "slicer")
        .filter_map(|n| {
            Some(Slicer {
                name: n.attribute("name")?.to_owned(),
                cache: n.attribute("cache")?.to_owned(),
                caption: n.attribute("caption").map(str::to_owned),
                attrs: plain_attrs(n, &["name", "cache", "caption"]),
                uid: uid_of(n),
                sheet,
                extra: inner(xml, n),
            })
        })
        .collect()
}

pub fn read_timeline_cache(xml: &str, part: &str) -> Option<TimelineCache> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "timelineCacheDefinition" {
        return None;
    }
    let state = child(root, "state");
    let pair = |name: &str| {
        state.and_then(|s| child(s, name)).map(|e| {
            (
                e.attribute("startDate").unwrap_or("").to_owned(),
                e.attribute("endDate").unwrap_or("").to_owned(),
            )
        })
    };
    Some(TimelineCache {
        name: root.attribute("name")?.to_owned(),
        source: root.attribute("sourceName")?.to_owned(),
        pivots: pivot_names(root),
        part: part.to_owned(),
        state: state.map(|s| plain_attrs(s, &["pivotCacheId", "filterType"])).unwrap_or_default(),
        selection: pair("selection"),
        bounds: pair("bounds"),
        uid: uid_of(root),
    })
}

pub fn read_timelines(xml: &str, sheet: usize) -> Vec<Timeline> {
    let Ok(doc) = Document::parse(xml) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "timeline")
        .filter_map(|n| {
            Some(Timeline {
                name: n.attribute("name")?.to_owned(),
                cache: n.attribute("cache")?.to_owned(),
                caption: n.attribute("caption").map(str::to_owned),
                attrs: plain_attrs(n, &["name", "cache", "caption"]),
                uid: uid_of(n),
                sheet,
                extra: inner(xml, n),
            })
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ writing

fn attr(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!(" {name}=\"{}\"", escape(value)));
}

fn pivot_tables(out: &mut String, pivots: &[(u32, String)]) {
    out.push_str("<pivotTables>");
    for (tab, name) in pivots {
        out.push_str(&format!("<pivotTable tabId=\"{tab}\" name=\"{}\"/>", escape(name)));
    }
    out.push_str("</pivotTables>");
}

/// The part of a slicer cache. `items` are the shared item numbers in the order the slicer lists them with
/// whether each is selected; `pivots` are the tables it filters with the number of the sheet each is on.
pub fn cache_xml(
    cache: &SlicerCache,
    pivot_cache_id: u64,
    pivots: &[(u32, String)],
    items: &[(usize, bool)],
) -> String {
    let mut out = String::from(HEAD);
    out.push_str(&format!(
        "<slicerCacheDefinition xmlns=\"{X14}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"x xr10\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xr10=\"{XR10}\""
    ));
    attr(&mut out, "name", &cache.name);
    if let Some(uid) = &cache.uid {
        attr(&mut out, "xr10:uid", uid);
    }
    attr(&mut out, "sourceName", &cache.source);
    out.push('>');
    pivot_tables(&mut out, pivots);
    out.push_str(&format!("<data><tabular pivotCacheId=\"{pivot_cache_id}\""));
    for (k, v) in &cache.tabular {
        attr(&mut out, k, v);
    }
    out.push_str(&format!("><items count=\"{}\">", items.len()));
    for (x, selected) in items {
        out.push_str(&format!("<i x=\"{x}\""));
        if *selected {
            out.push_str(" s=\"1\"");
        }
        if cache.no_data.contains(x) {
            out.push_str(" nd=\"1\"");
        }
        out.push_str("/>");
    }
    out.push_str("</items></tabular></data></slicerCacheDefinition>");
    out
}

fn list_header(root: &str, ns: &str) -> String {
    format!(
        "{HEAD}<{root} xmlns=\"{ns}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"x xr10\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xr10=\"{XR10}\">"
    )
}

fn entry(tag: &str, name: &str, uid: &Option<String>, cache: &str, caption: &Option<String>, attrs: &[(String, String)], extra: &str) -> String {
    let mut out = format!("<{tag}");
    attr(&mut out, "name", name);
    if let Some(uid) = uid {
        attr(&mut out, "xr10:uid", uid);
    }
    attr(&mut out, "cache", cache);
    if let Some(caption) = caption {
        attr(&mut out, "caption", caption);
    }
    for (k, v) in attrs {
        attr(&mut out, k, v);
    }
    if extra.is_empty() {
        out.push_str("/>");
    } else {
        out.push_str(&format!(">{extra}</{tag}>"));
    }
    out
}

/// A `slicers` part holding these slicers.
pub fn slicers_xml(slicers: &[&Slicer]) -> String {
    let mut out = list_header("slicers", X14);
    for s in slicers {
        out.push_str(&entry("slicer", &s.name, &s.uid, &s.cache, &s.caption, &s.attrs, &s.extra));
    }
    out.push_str("</slicers>");
    out
}

/// A `timelines` part holding these timelines.
pub fn timelines_xml(timelines: &[&Timeline]) -> String {
    let mut out = list_header("timelines", X15);
    for t in timelines {
        out.push_str(&entry("timeline", &t.name, &t.uid, &t.cache, &t.caption, &t.attrs, &t.extra));
    }
    out.push_str("</timelines>");
    out
}

/// The part of a timeline cache.
pub fn timeline_cache_xml(cache: &TimelineCache, pivot_cache_id: u64, pivots: &[(u32, String)]) -> String {
    let mut out = String::from(HEAD);
    out.push_str(&format!(
        "<timelineCacheDefinition xmlns=\"{X15}\" xmlns:x15=\"{X15}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"xr10\" xmlns:xr10=\"{XR10}\""
    ));
    attr(&mut out, "name", &cache.name);
    if let Some(uid) = &cache.uid {
        attr(&mut out, "xr10:uid", uid);
    }
    attr(&mut out, "sourceName", &cache.source);
    out.push('>');
    pivot_tables(&mut out, pivots);
    out.push_str("<state");
    let mut state = cache.state.clone();
    for (k, v) in [("minimalRefreshVersion", "6"), ("lastRefreshVersion", "6")] {
        if !state.iter().any(|(n, _)| n == k) {
            state.push((k.to_owned(), v.to_owned()));
        }
    }
    for (k, v) in &state {
        attr(&mut out, k, v);
    }
    attr(&mut out, "pivotCacheId", &pivot_cache_id.to_string());
    attr(&mut out, "filterType", if cache.selection.is_some() { "dateBetween" } else { "unknown" });
    out.push('>');
    if let Some((a, b)) = &cache.selection {
        out.push_str(&format!("<selection startDate=\"{}\" endDate=\"{}\"/>", escape(a), escape(b)));
    }
    if let Some((a, b)) = &cache.bounds {
        out.push_str(&format!("<bounds startDate=\"{}\" endDate=\"{}\"/>", escape(a), escape(b)));
    }
    out.push_str("</state></timelineCacheDefinition>");
    out
}
