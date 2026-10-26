//! Reading the parts of a pivot table into the model.
//!
//! A part this version does not understand makes the reader give up on that cache or table, which then
//! stays in the workbook as the bytes it was read as.

use std::sync::Arc;

use roxmltree::{Document, Node};

use super::cache::{
    CacheField, CacheSource, CacheValue, CalculatedItem, FieldGroup, GroupBy, PivotCache, RangePr,
};
use super::table::{
    AutoSort, AxisEntry, DataField, Filter, FilterKind, ItemEntry, Options, PageField, PivotField,
    PivotTable, StyleInfo, Subtotals,
};
use super::{Form, Func, ShowAs, SortType};
use crate::coord::Range;
use crate::model::{Relationship, Workbook};
use crate::value::ErrorKind;
use crate::xlsx::{unescape_office, MAIN_NS};

pub const REL_CACHE_DEFINITION: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
pub const REL_CACHE_RECORDS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords";
pub const REL_PIVOT_TABLE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";

/// The cache ids the workbook lists, with the relationship that names each one's definition.
pub fn listed_caches(book: &Workbook) -> Vec<(u32, String)> {
    let Some((_, xml)) = book.raw.iter().find(|(n, _)| n == "pivotCaches") else {
        return Vec::new();
    };
    let wrapped = format!("<w xmlns=\"{MAIN_NS}\" xmlns:r=\"{}\">{xml}</w>", crate::xlsx::REL_NS);
    let Ok(doc) = Document::parse(&wrapped) else { return Vec::new() };
    let mut out = Vec::new();
    for n in doc.descendants().filter(|n| n.has_tag_name((MAIN_NS, "pivotCache"))) {
        let id = n.attribute("cacheId").and_then(|v| v.parse().ok());
        let rid = n.attribute((crate::xlsx::REL_NS, "id"));
        if let (Some(id), Some(rid)) = (id, rid) {
            out.push((id, rid.to_owned()));
        }
    }
    out
}

/// A path in the zip made from the part that refers to it and a relationship target.
pub fn join_path(base_part: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_owned();
    }
    let mut parts: Vec<&str> = base_part.split('/').collect();
    parts.pop();
    for piece in target.split('/') {
        match piece {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The path of the relationships part of a part.
pub fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// The relationships in a `.rels` part kept in the workbook.
pub fn read_rels(book: &Workbook, part: &str) -> Vec<Relationship> {
    let Some(bytes) = book.kept.get(&rels_path(part)) else { return Vec::new() };
    let text = String::from_utf8_lossy(bytes).into_owned();
    let Ok(doc) = Document::parse(&text) else { return Vec::new() };
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Relationship")
        .map(|n| Relationship {
            id: n.attribute("Id").unwrap_or("").to_owned(),
            kind: n.attribute("Type").unwrap_or("").to_owned(),
            target: n.attribute("Target").unwrap_or("").to_owned(),
            external: n.attribute("TargetMode") == Some("External"),
        })
        .collect()
}

fn text_of(book: &Workbook, part: &str) -> Option<String> {
    let bytes = book.kept.get(part)?;
    let bytes: &[u8] = bytes;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'b>(n: Node<'a, 'b>, name: &'a str) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn flag(n: Node, name: &str, default: bool) -> bool {
    match n.attribute(name) {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => default,
    }
}

fn number<T: std::str::FromStr>(n: Node, name: &str) -> Option<T> {
    n.attribute(name).and_then(|v| v.parse().ok())
}

/// The raw text of an element, from the source it was parsed from.
fn raw<'a>(xml: &'a str, n: Node) -> &'a str {
    &xml[n.range()]
}

// ----------------------------------------------------------------------------------------------------
// The cache.

/// Read a cache from its definition part, or give up on it.
pub fn read_cache(book: &Workbook, id: u32, definition: &str) -> Option<PivotCache> {
    let xml = text_of(book, definition)?;
    let doc = Document::parse(&xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "pivotCacheDefinition" {
        return None;
    }
    let source = read_source(root)?;
    let rels = read_rels(book, definition);
    let records_rel = rels.iter().find(|r| r.kind == REL_CACHE_RECORDS);
    let records_part = records_rel.map(|r| join_path(definition, &r.target)).unwrap_or_default();
    let mut fields = Vec::new();
    let cache_fields = child(root, "cacheFields")?;
    for cf in children(cache_fields, "cacheField") {
        fields.push(read_cache_field(book, cf)?);
    }
    let records = if records_part.is_empty() {
        Vec::new()
    } else {
        read_records(book, &records_part, &fields)?
    };
    let calculated_items = read_calculated_items(root);
    // Anything this version does not know in the definition is a reason to keep it as it is.
    for c in root.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "cacheSource" | "cacheFields" | "calculatedItems" | "extLst" => {}
            _ => return None,
        }
    }
    let attrs = root
        .attributes()
        .filter(|a| {
            !matches!(a.name(), "id" | "recordCount")
                && a.namespace().is_none_or(|ns| ns != crate::xlsx::REL_NS)
        })
        .map(|a| (a.name().to_owned(), a.value().to_owned()))
        .collect();
    Some(PivotCache {
        id,
        source,
        fields,
        records,
        attrs,
        refresh_on_load: flag(root, "refreshOnLoad", false),
        calculated_items,
        definition_part: definition.to_owned(),
        records_part,
        dirty: false,
        slicer_id: read_slicer_id(root),
    })
}

/// The number slicers and timelines know a cache by, kept in an extension of its definition.
fn read_slicer_id(root: Node) -> Option<u64> {
    let list = child(root, "extLst")?;
    list.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "pivotCacheDefinition")
        .and_then(|n| n.attribute("pivotCacheId"))
        .and_then(|v| v.parse().ok())
}

fn read_source(root: Node) -> Option<CacheSource> {
    let src = child(root, "cacheSource")?;
    if src.attribute("type") != Some("worksheet") {
        return None;
    }
    let ws = child(src, "worksheetSource")?;
    if ws.attribute((crate::xlsx::REL_NS, "id")).is_some() {
        return None;
    }
    if let Some(name) = ws.attribute("name") {
        return Some(CacheSource::Name(name.to_owned()));
    }
    let range = Range::parse(ws.attribute("ref")?)?;
    Some(CacheSource::Range { sheet: ws.attribute("sheet")?.to_owned(), range })
}

fn read_cache_field(book: &Workbook, cf: Node) -> Option<CacheField> {
    let name = cf.attribute("name")?.to_owned();
    let num_fmt_id: u32 = number(cf, "numFmtId").unwrap_or(0);
    let mut field = CacheField::new(&name);
    field.num_fmt_id = num_fmt_id;
    field.num_fmt_code = (num_fmt_id >= 164).then(|| book.styles.code_for_id(num_fmt_id));
    field.database = flag(cf, "databaseField", true);
    field.formula = cf.attribute("formula").map(str::to_owned);
    for a in cf.attributes() {
        if !matches!(a.name(), "name" | "numFmtId" | "databaseField" | "formula") {
            field.attrs.push((a.name().to_owned(), a.value().to_owned()));
        }
    }
    for c in cf.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "sharedItems" => {
                for item in c.children().filter(Node::is_element) {
                    field.shared.push(read_value(book, item)?);
                }
            }
            "fieldGroup" => field.group = Some(read_group(book, c)?),
            "extLst" => {}
            _ => return None,
        }
    }
    Some(field)
}

fn read_group(book: &Workbook, g: Node) -> Option<FieldGroup> {
    let mut group =
        FieldGroup { par: number(g, "par"), base: number(g, "base"), ..Default::default() };
    for c in g.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "rangePr" => {
                let group_by = GroupBy::from_xml(c.attribute("groupBy").unwrap_or("range"))?;
                group.range = Some(RangePr {
                    group_by,
                    start_date: c.attribute("startDate").and_then(|v| iso_serial(v, book.date1904)),
                    end_date: c.attribute("endDate").and_then(|v| iso_serial(v, book.date1904)),
                    start_num: number(c, "startNum"),
                    end_num: number(c, "endNum"),
                    interval: number(c, "groupInterval").unwrap_or(1.0),
                    auto_start: flag(c, "autoStart", true),
                    auto_end: flag(c, "autoEnd", true),
                });
            }
            "discretePr" => {
                group.discrete =
                    Some(children(c, "x").map(|x| number::<usize>(x, "v").unwrap_or(0)).collect());
            }
            "groupItems" => {
                for item in c.children().filter(Node::is_element) {
                    group.items.push(read_value(book, item)?);
                }
            }
            _ => return None,
        }
    }
    Some(group)
}

/// A value element: `s`, `n`, `d`, `b`, `e` or `m`.
fn read_value(book: &Workbook, n: Node) -> Option<CacheValue> {
    let v = n.attribute("v");
    Some(match n.tag_name().name() {
        "m" => CacheValue::Blank,
        "s" => CacheValue::Text(Arc::from(unescape_office(v.unwrap_or("")).as_str())),
        "n" => CacheValue::Number(v?.parse().ok()?),
        "d" => CacheValue::Date(iso_serial(v?, book.date1904)?),
        "b" => CacheValue::Bool(matches!(v?, "1" | "true")),
        "e" => CacheValue::Error(ErrorKind::parse(v?).unwrap_or(ErrorKind::Value)),
        _ => return None,
    })
}

/// `2024-01-30T00:00:00` as a serial number.
pub fn iso_serial(text: &str, date1904: bool) -> Option<f64> {
    let (date, time) = text.split_once('T').unwrap_or((text, "00:00:00"));
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: i64 = d.next()?.parse().ok()?;
    let day: i64 = d.next()?.parse().ok()?;
    let mut t = time.trim_end_matches('Z').split(':');
    let hours: f64 = t.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let minutes: f64 = t.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let seconds: f64 = t.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    let serial = crate::dates::serial(year, month, day, date1904)?;
    Some(serial + (hours * 3600.0 + minutes * 60.0 + seconds) / 86400.0)
}

fn read_records(
    book: &Workbook,
    part: &str,
    fields: &[CacheField],
) -> Option<Vec<Vec<CacheValue>>> {
    let xml = text_of(book, part)?;
    let doc = Document::parse(&xml).ok()?;
    let mut records = Vec::new();
    let database: Vec<usize> = (0..fields.len()).filter(|i| fields[*i].database).collect();
    for r in children(doc.root_element(), "r") {
        let mut record = vec![CacheValue::Blank; fields.len()];
        for (at, item) in r.children().filter(Node::is_element).enumerate() {
            let f = *database.get(at)?;
            record[f] = if item.tag_name().name() == "x" {
                let index: usize = number(item, "v").unwrap_or(0);
                fields[f].shared.get(index).cloned().unwrap_or(CacheValue::Blank)
            } else {
                read_value(book, item)?
            };
        }
        records.push(record);
    }
    Some(records)
}

fn read_calculated_items(root: Node) -> Vec<CalculatedItem> {
    let Some(list) = child(root, "calculatedItems") else { return Vec::new() };
    let mut out = Vec::new();
    for c in children(list, "calculatedItem") {
        let formula = c.attribute("formula").unwrap_or("").to_owned();
        let reference =
            c.descendants().find(|n| n.is_element() && n.tag_name().name() == "reference");
        let (Some(reference), true) = (reference, true) else { continue };
        let field: usize = number(reference, "field").unwrap_or(0);
        let item: usize = child(reference, "x").and_then(|x| number(x, "v")).unwrap_or(0);
        out.push(CalculatedItem { field, formula, item });
    }
    out
}

// ----------------------------------------------------------------------------------------------------
// The table.

/// Read a table from its part, or give up on it.
pub fn read_table(book: &Workbook, sheet: usize, part: &str) -> Option<PivotTable> {
    let xml = text_of(book, part)?;
    let doc = Document::parse(&xml).ok()?;
    let root = doc.root_element();
    if root.tag_name().name() != "pivotTableDefinition" {
        return None;
    }
    let cache_id: u32 = number(root, "cacheId")?;
    let cache = book.pivots.caches.iter().position(|c| c.id == cache_id)?;
    let field_count = book.pivots.caches[cache].fields.len();
    let location = child(root, "location")?;
    let range = Range::parse(location.attribute("ref")?)?;
    let mut table = PivotTable::new(
        root.attribute("name")?,
        sheet,
        cache,
        range.first,
        &book.pivots.caches[cache],
    );
    table.part = part.to_owned();
    table.location = range;
    table.has_cells = true;
    table.dirty = false;
    table.first_header_row = number(location, "firstHeaderRow").unwrap_or(0);
    table.first_data_row = number(location, "firstDataRow").unwrap_or(0);
    table.first_data_col = number(location, "firstDataCol").unwrap_or(0);
    table.row_page_count = number(location, "rowPageCount").unwrap_or(0);
    table.col_page_count = number(location, "colPageCount").unwrap_or(0);
    table.options = read_options(root);
    for c in root.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "location" | "rowItems" | "colItems" => {}
            "pivotFields" => read_fields(book, &mut table, c, field_count)?,
            "rowFields" => table.rows = read_axis(c, field_count)?,
            "colFields" => table.cols = read_axis(c, field_count)?,
            "pageFields" => read_pages(&mut table, c)?,
            "dataFields" => read_data(book, &mut table, c)?,
            "formats" => table.kept.formats = Some(raw(&xml, c).to_owned()),
            "conditionalFormats" => table.kept.conditional_formats = Some(raw(&xml, c).to_owned()),
            "chartFormats" => table.kept.chart_formats = Some(raw(&xml, c).to_owned()),
            "pivotTableStyleInfo" => table.style = read_style(c),
            "filters" => read_filters(&mut table, c)?,
            "extLst" => table.kept.ext_lst = Some(raw(&xml, c).to_owned()),
            _ => return None,
        }
    }
    if table.fields.len() != field_count {
        return None;
    }
    let (r, c) = (table.location.first.row, table.location.first.col);
    let _ = (r, c);
    Some(table)
}

#[allow(clippy::field_reassign_with_default)]
fn read_options(root: Node) -> Options {
    let mut o = Options::default();
    o.form = if flag(root, "compact", true) {
        Form::Compact
    } else if flag(root, "outline", false) {
        Form::Outline
    } else {
        Form::Tabular
    };
    o.grand_total_row = flag(root, "rowGrandTotals", true);
    o.grand_total_col = flag(root, "colGrandTotals", true);
    o.data_caption = root.attribute("dataCaption").unwrap_or("Values").to_owned();
    o.grand_total_caption = root.attribute("grandTotalCaption").map(str::to_owned);
    o.row_header_caption = root.attribute("rowHeaderCaption").map(str::to_owned);
    o.col_header_caption = root.attribute("colHeaderCaption").map(str::to_owned);
    if flag(root, "showError", false) {
        o.error_caption = Some(root.attribute("errorCaption").unwrap_or("").to_owned());
    }
    if flag(root, "showMissing", true) {
        o.missing_caption = root.attribute("missingCaption").map(str::to_owned);
    }
    o.show_headers = flag(root, "showHeaders", true);
    o.page_over_then_down = flag(root, "pageOverThenDown", false);
    o.page_wrap = number(root, "pageWrap").unwrap_or(0);
    o.attrs = root
        .attributes()
        .filter(|a| {
            !matches!(
                a.name(),
                "name"
                    | "cacheId"
                    | "dataOnRows"
                    | "dataCaption"
                    | "grandTotalCaption"
                    | "rowHeaderCaption"
                    | "colHeaderCaption"
                    | "errorCaption"
                    | "showError"
                    | "missingCaption"
                    | "showMissing"
                    | "rowGrandTotals"
                    | "colGrandTotals"
                    | "compact"
                    | "outline"
                    | "compactData"
                    | "outlineData"
                    | "showHeaders"
                    | "pageOverThenDown"
                    | "pageWrap"
                    | "dataPosition"
                    | "uid"
            ) && a.namespace().is_none()
        })
        .map(|a| (a.name().to_owned(), a.value().to_owned()))
        .collect();
    o
}

fn read_fields(book: &Workbook, table: &mut PivotTable, list: Node, expected: usize) -> Option<()> {
    let mut fields = Vec::new();
    let cache = &book.pivots.caches[table.cache];
    for (i, pf) in children(list, "pivotField").enumerate() {
        let mut field = read_field(book, pf)?;
        // A format the field has because its source cells have it is not a format the person chose.
        if number::<u32>(pf, "numFmtId") == cache.fields.get(i).map(|f| f.num_fmt_id) {
            field.num_fmt = None;
        }
        fields.push(field);
    }
    if fields.len() != expected {
        return None;
    }
    table.fields = fields;
    Some(())
}

const FIELD_KNOWN: &[&str] = &[
    "name",
    "axis",
    "dataField",
    "compact",
    "outline",
    "numFmtId",
    "defaultSubtotal",
    "sortType",
    "subtotalTop",
    "insertBlankRow",
    "showAll",
    "multipleItemSelectionAllowed",
    "measureFilter",
    "sumSubtotal",
    "countASubtotal",
    "avgSubtotal",
    "maxSubtotal",
    "minSubtotal",
    "productSubtotal",
    "countSubtotal",
    "stdDevSubtotal",
    "stdDevPSubtotal",
    "varSubtotal",
    "varPSubtotal",
];

fn read_field(book: &Workbook, pf: Node) -> Option<PivotField> {
    let form = if flag(pf, "compact", true) {
        Form::Compact
    } else if flag(pf, "outline", true) {
        Form::Outline
    } else {
        Form::Tabular
    };
    let mut field = PivotField::new(form);
    field.name = pf.attribute("name").map(str::to_owned);
    field.num_fmt = number::<u32>(pf, "numFmtId").map(|id| book.styles.code_for_id(id));
    field.sort = match pf.attribute("sortType") {
        Some("ascending") => SortType::Ascending,
        Some("descending") => SortType::Descending,
        _ => SortType::Manual,
    };
    field.subtotal_top = flag(pf, "subtotalTop", true);
    field.blank_row = flag(pf, "insertBlankRow", false);
    field.show_all = flag(pf, "showAll", true);
    field.multi_page = flag(pf, "multipleItemSelectionAllowed", false);
    for func in Func::ALL {
        if flag(pf, func.subtotal_attr(), false) {
            field.subtotals.funcs.push(func);
        }
    }
    field.subtotals.automatic =
        flag(pf, "defaultSubtotal", true) && field.subtotals.funcs.is_empty();
    for a in pf.attributes() {
        if !FIELD_KNOWN.contains(&a.name()) {
            field.attrs.push((a.name().to_owned(), a.value().to_owned()));
        }
    }
    for c in pf.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "items" => read_items(&mut field, c)?,
            "autoSortScope" => {
                let x = c.descendants().find(|n| n.is_element() && n.tag_name().name() == "x");
                let data = x.and_then(|x| number(x, "v")).unwrap_or(0);
                field.auto_sort = Some(AutoSort { data });
            }
            "extLst" => {
                if raw_contains_repeat(c) {
                    field.repeat_labels = true;
                }
            }
            _ => return None,
        }
    }
    Some(field)
}

fn raw_contains_repeat(ext: Node) -> bool {
    ext.descendants().any(|n| {
        n.is_element() && n.tag_name().name() == "pivotField" && flag(n, "fillDownLabels", false)
    })
}

fn read_items(field: &mut PivotField, items: Node) -> Option<()> {
    let mut has_default = false;
    let mut funcs = Vec::new();
    for item in children(items, "item") {
        match item.attribute("t") {
            None | Some("data") => {
                let x: usize = number(item, "x")?;
                field.items.push(ItemEntry {
                    x,
                    hidden: flag(item, "h", false),
                    name: item.attribute("n").map(str::to_owned),
                    calculated: flag(item, "f", false),
                });
            }
            Some("default") => has_default = true,
            Some(t) => funcs.push(Func::from_item_type(t)?),
        }
    }
    if !has_default && funcs.is_empty() {
        field.subtotals = Subtotals::none();
    } else {
        field.subtotals = Subtotals { automatic: has_default, funcs };
    }
    Some(())
}

fn read_axis(list: Node, field_count: usize) -> Option<Vec<AxisEntry>> {
    let mut out = Vec::new();
    for f in children(list, "field") {
        let x: i64 = number(f, "x")?;
        if x == -2 {
            out.push(AxisEntry::Values);
        } else if x >= 0 && (x as usize) < field_count {
            out.push(AxisEntry::Field(x as usize));
        } else {
            return None;
        }
    }
    Some(out)
}

fn read_pages(table: &mut PivotTable, list: Node) -> Option<()> {
    for p in children(list, "pageField") {
        if number::<i64>(p, "hier").is_some_and(|h| h >= 0) {
            return None;
        }
        let field: usize = number(p, "fld")?;
        let item = number::<usize>(p, "item");
        table.pages.push(PageField { field, item });
    }
    Some(())
}

fn read_data(book: &Workbook, table: &mut PivotTable, list: Node) -> Option<()> {
    for d in children(list, "dataField") {
        let field: usize = number(d, "fld")?;
        let func = match d.attribute("subtotal") {
            Some(s) => Func::from_xml(s)?,
            None => Func::Sum,
        };
        let mut data = DataField::new(d.attribute("name").unwrap_or(""), field, func);
        data.show = match d.attribute("showDataAs") {
            None => ShowAs::Normal,
            Some(s) => ShowAs::from_xml(s)?,
        };
        for ext in d.descendants().filter(|n| n.is_element() && n.tag_name().name() == "dataField")
        {
            if let Some(s) = ext.attribute("pivotShowAs") {
                data.show = ShowAs::from_xml(s)?;
            }
        }
        data.base_field = number(d, "baseField").unwrap_or(0);
        data.base_item = number(d, "baseItem").unwrap_or(0);
        data.num_fmt = number::<u32>(d, "numFmtId").map(|id| book.styles.code_for_id(id));
        table.data.push(data);
    }
    Some(())
}

fn read_style(s: Node) -> StyleInfo {
    StyleInfo {
        name: s.attribute("name").map(str::to_owned),
        show_row_headers: flag(s, "showRowHeaders", false),
        show_col_headers: flag(s, "showColHeaders", false),
        show_row_stripes: flag(s, "showRowStripes", false),
        show_col_stripes: flag(s, "showColStripes", false),
        show_last_column: flag(s, "showLastColumn", false),
    }
}

fn read_filters(table: &mut PivotTable, list: Node) -> Option<()> {
    for f in children(list, "filter") {
        let field: usize = number(f, "fld")?;
        let kind = FilterKind::from_xml(f.attribute("type")?)?;
        let mut filter = Filter {
            kind,
            data: number(f, "iMeasureFld"),
            value1: f.attribute("stringValue1").unwrap_or("").to_owned(),
            value2: f.attribute("stringValue2").map(str::to_owned),
            top: true,
            id: number(f, "id").unwrap_or(1),
        };
        for n in f.descendants().filter(|n| n.is_element()) {
            match n.tag_name().name() {
                "top10" => {
                    filter.top = flag(n, "top", true);
                    filter.value1 = n.attribute("val").unwrap_or("0").to_owned();
                }
                "customFilter"
                    if (kind.is_value() || kind == FilterKind::DateBetween) && !kind.is_top() =>
                {
                    if filter.value1.is_empty() {
                        filter.value1 = n.attribute("val").unwrap_or("").to_owned();
                    } else if filter.value2.is_none() {
                        filter.value2 = n.attribute("val").map(str::to_owned);
                    }
                }
                _ => {}
            }
        }
        table.fields.get_mut(field)?.filters.push(filter);
    }
    Some(())
}
