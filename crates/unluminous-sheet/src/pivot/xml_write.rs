//! Writing the parts of a pivot table: the way Excel writes them, so that Excel opens them with no repair
//! and refreshes them to the same cells.

use std::collections::HashMap;

use super::cache::{CacheField, CacheSource, CacheValue, PivotCache};
use super::items::{Line, LineKind, Seg};
use super::table::{AxisEntry, Filter, FilterKind, PivotField, PivotTable};
use super::{Form, Func, ShowAs, SortType};
use crate::styles::Styles;
use crate::xlsx::write::number_text;
use crate::xlsx::{escape, MAIN_NS, REL_NS};

pub const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

fn attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape(value));
    out.push('"');
}

fn flag_attr(out: &mut String, name: &str, value: bool) {
    attr(out, name, if value { "1" } else { "0" });
}

/// A serial number as the ISO text a pivot cache writes.
pub fn iso(serial: f64, date1904: bool) -> String {
    let whole = serial.floor();
    let (y, m, d) = crate::dates::date(whole, date1904).unwrap_or((1900, 1, 1));
    let seconds = ((serial - whole) * 86400.0).round() as u32;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

fn value_xml(v: &CacheValue, date1904: bool) -> String {
    match v {
        CacheValue::Blank => "<m/>".to_owned(),
        CacheValue::Number(n) => format!("<n v=\"{}\"/>", number_text(*n)),
        CacheValue::Date(n) => format!("<d v=\"{}\"/>", iso(*n, date1904)),
        CacheValue::Text(t) => format!("<s v=\"{}\"/>", escape(t)),
        CacheValue::Bool(b) => format!("<b v=\"{}\"/>", u8::from(*b)),
        CacheValue::Error(e) => format!("<e v=\"{}\"/>", e.text()),
    }
}

// ----------------------------------------------------------------------------------------------------
// The cache.

/// What a field's values are, which the flags of `sharedItems` say.
#[derive(Default)]
struct Stats {
    string: bool,
    blank: bool,
    number: bool,
    date: bool,
    boolean: bool,
    error: bool,
    integer: bool,
    min: f64,
    max: f64,
    min_date: f64,
    max_date: f64,
}

fn stats_of(values: impl Iterator<Item = CacheValue>) -> Stats {
    let mut s = Stats {
        integer: true,
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
        min_date: f64::INFINITY,
        max_date: f64::NEG_INFINITY,
        ..Default::default()
    };
    for v in values {
        match v {
            CacheValue::Blank => s.blank = true,
            CacheValue::Text(_) => s.string = true,
            CacheValue::Bool(_) => s.boolean = true,
            CacheValue::Error(_) => s.error = true,
            CacheValue::Number(n) => {
                s.number = true;
                s.integer &= n.fract() == 0.0 && n.abs() < 1e15;
                s.min = s.min.min(n);
                s.max = s.max.max(n);
            }
            CacheValue::Date(n) => {
                s.date = true;
                s.min_date = s.min_date.min(n);
                s.max_date = s.max_date.max(n);
            }
        }
    }
    s
}

/// Whether the file lists the shared items of a field: text fields always, others when a table uses them.
fn lists_items(cache: &PivotCache, f: usize, used: &[bool]) -> bool {
    let field = &cache.fields[f];
    if !field.database {
        return false;
    }
    if !field.shared.is_empty() || used[f] || field.group.is_some() {
        return true;
    }
    cache.records.iter().any(|r| matches!(r[f], CacheValue::Text(_) | CacheValue::Bool(_)))
}

/// The items a listed field has: its shared items, or the distinct values in order of first appearance.
fn listed_items(cache: &PivotCache, f: usize) -> Vec<CacheValue> {
    if !cache.fields[f].shared.is_empty() {
        return cache.fields[f].shared.clone();
    }
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut out = Vec::new();
    for r in &cache.records {
        if seen.insert(r[f].key(), ()).is_none() {
            out.push(r[f].clone());
        }
    }
    out
}

/// Which fields any table puts on an axis or in the filter area.
pub fn used_fields(cache_index: usize, tables: &[PivotTable], count: usize) -> Vec<bool> {
    let mut used = vec![false; count];
    for t in tables.iter().filter(|t| t.cache == cache_index) {
        for f in t.used_fields() {
            if f < count {
                used[f] = true;
            }
        }
        for d in &t.data {
            if d.field < count {
                used[d.field] |= false;
            }
        }
        // A field in no area that lists its items, as a slicer or a timeline makes it, shares them too.
        for (f, field) in t.fields.iter().enumerate() {
            if f < count && !field.items.is_empty() {
                used[f] = true;
            }
        }
    }
    used
}

/// The definition part of a cache.
pub fn cache_definition(
    cache: &PivotCache,
    used: &[bool],
    date1904: bool,
    records_rid: &str,
) -> String {
    let mut out = String::from(XML_HEAD);
    out.push_str(&format!("<pivotCacheDefinition xmlns=\"{MAIN_NS}\" xmlns:r=\"{REL_NS}\""));
    attr(&mut out, "r:id", records_rid);
    for (k, v) in &cache.attrs {
        attr(&mut out, k, v);
    }
    if cache.refresh_on_load {
        attr(&mut out, "refreshOnLoad", "1");
    }
    attr(&mut out, "recordCount", &cache.records.len().to_string());
    out.push('>');
    out.push_str("<cacheSource type=\"worksheet\"><worksheetSource");
    match &cache.source {
        CacheSource::Range { sheet, range } => {
            attr(&mut out, "ref", &range.a1());
            attr(&mut out, "sheet", sheet);
        }
        CacheSource::Name(name) => attr(&mut out, "name", name),
    }
    out.push_str("/></cacheSource>");
    out.push_str(&format!("<cacheFields count=\"{}\">", cache.fields.len()));
    for (f, field) in cache.fields.iter().enumerate() {
        out.push_str(&cache_field(cache, f, field, used, date1904));
    }
    out.push_str("</cacheFields>");
    if !cache.calculated_items.is_empty() {
        out.push_str(&format!("<calculatedItems count=\"{}\">", cache.calculated_items.len()));
        for c in &cache.calculated_items {
            out.push_str(&format!(
                "<calculatedItem formula=\"{}\"><pivotArea cacheIndex=\"1\" outline=\"0\" fieldPosition=\"0\"><references count=\"1\"><reference field=\"{}\" count=\"1\"><x v=\"{}\"/></reference></references></pivotArea></calculatedItem>",
                escape(&c.formula),
                c.field,
                c.item
            ));
        }
        out.push_str("</calculatedItems>");
    }
    if let Some(id) = cache.slicer_id {
        out.push_str(&format!("<extLst><ext uri=\"{{725AE2AE-9491-48be-B2B4-4EB974FC3084}}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:pivotCacheDefinition pivotCacheId=\"{id}\"/></ext></extLst>"));
    }
    out.push_str("</pivotCacheDefinition>");
    out
}

fn cache_field(
    cache: &PivotCache,
    f: usize,
    field: &CacheField,
    used: &[bool],
    date1904: bool,
) -> String {
    let mut out = String::from("<cacheField");
    attr(&mut out, "name", &field.name);
    attr(&mut out, "numFmtId", &field.num_fmt_id.to_string());
    for (k, v) in &field.attrs {
        attr(&mut out, k, v);
    }
    if let Some(formula) = &field.formula {
        attr(&mut out, "formula", formula);
    }
    if !field.database {
        attr(&mut out, "databaseField", "0");
    }
    out.push('>');
    if field.database {
        out.push_str(&shared_items(cache, f, used, date1904));
    }
    if let Some(g) = &field.group {
        out.push_str(&field_group(g, date1904));
    }
    out.push_str("</cacheField>");
    out
}

fn shared_items(cache: &PivotCache, f: usize, used: &[bool], date1904: bool) -> String {
    let listed = lists_items(cache, f, used);
    let items = if listed { listed_items(cache, f) } else { Vec::new() };
    let s = stats_of(cache.records.iter().map(|r| r[f].clone()).chain(items.iter().cloned()));
    let mut out = String::from("<sharedItems");
    let semi = s.string || s.blank || s.error;
    if !semi {
        attr(&mut out, "containsSemiMixedTypes", "0");
    }
    if s.date && !(s.string || s.number || s.boolean || s.error) {
        attr(&mut out, "containsNonDate", "0");
    }
    if s.date {
        attr(&mut out, "containsDate", "1");
    }
    if !s.string {
        attr(&mut out, "containsString", "0");
    }
    if s.blank {
        attr(&mut out, "containsBlank", "1");
    }
    let kinds = [s.string, s.number, s.date, s.boolean, s.error].into_iter().filter(|b| *b).count();
    if kinds > 1 {
        attr(&mut out, "containsMixedTypes", "1");
    }
    if s.number {
        attr(&mut out, "containsNumber", "1");
        if s.integer {
            attr(&mut out, "containsInteger", "1");
        }
        attr(&mut out, "minValue", &number_text(s.min));
        attr(&mut out, "maxValue", &number_text(s.max));
    }
    if s.date {
        attr(&mut out, "minDate", &iso(s.min_date, date1904));
        attr(&mut out, "maxDate", &iso(s.max_date, date1904));
    }
    if listed {
        attr(&mut out, "count", &items.len().to_string());
        out.push('>');
        for (i, v) in items.iter().enumerate() {
            let calculated = cache.calculated_items.iter().any(|c| c.field == f && c.item == i);
            let xml = value_xml(v, date1904);
            if calculated {
                out.push_str(&xml.replace("/>", " f=\"1\"/>"));
            } else {
                out.push_str(&xml);
            }
        }
        out.push_str("</sharedItems>");
    } else {
        out.push_str("/>");
    }
    out
}

fn field_group(g: &super::cache::FieldGroup, date1904: bool) -> String {
    let mut out = String::from("<fieldGroup");
    if let Some(p) = g.par {
        attr(&mut out, "par", &p.to_string());
    }
    if let Some(b) = g.base {
        attr(&mut out, "base", &b.to_string());
    }
    if g.range.is_none() && g.discrete.is_none() && g.items.is_empty() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    if let Some(r) = &g.range {
        out.push_str("<rangePr");
        if !r.auto_start {
            attr(&mut out, "autoStart", "0");
        }
        if !r.auto_end {
            attr(&mut out, "autoEnd", "0");
        }
        if r.group_by != super::cache::GroupBy::Range {
            attr(&mut out, "groupBy", r.group_by.xml());
        }
        if let Some(n) = r.start_num {
            attr(&mut out, "startNum", &number_text(n));
        }
        if let Some(n) = r.end_num {
            attr(&mut out, "endNum", &number_text(n));
        }
        if let Some(n) = r.start_date {
            attr(&mut out, "startDate", &iso(n, date1904));
        }
        if let Some(n) = r.end_date {
            attr(&mut out, "endDate", &iso(n, date1904));
        }
        if r.interval != 1.0 {
            attr(&mut out, "groupInterval", &number_text(r.interval));
        }
        out.push_str("/>");
    }
    if let Some(map) = &g.discrete {
        out.push_str(&format!("<discretePr count=\"{}\">", map.len()));
        for x in map {
            out.push_str(&format!("<x v=\"{x}\"/>"));
        }
        out.push_str("</discretePr>");
    }
    if !g.items.is_empty() {
        out.push_str(&format!("<groupItems count=\"{}\">", g.items.len()));
        for v in &g.items {
            out.push_str(&value_xml(v, date1904));
        }
        out.push_str("</groupItems>");
    }
    out.push_str("</fieldGroup>");
    out
}

/// The records part of a cache.
pub fn cache_records(cache: &PivotCache, used: &[bool], date1904: bool) -> String {
    let mut out = String::from(XML_HEAD);
    out.push_str(&format!(
        "<pivotCacheRecords xmlns=\"{MAIN_NS}\" xmlns:r=\"{REL_NS}\" count=\"{}\">",
        cache.records.len()
    ));
    let mut maps: Vec<Option<HashMap<String, usize>>> = Vec::new();
    for f in 0..cache.fields.len() {
        maps.push(lists_items(cache, f, used).then(|| {
            listed_items(cache, f).iter().enumerate().map(|(i, v)| (v.key(), i)).collect()
        }));
    }
    for r in &cache.records {
        out.push_str("<r>");
        for (f, field) in cache.fields.iter().enumerate() {
            if !field.database {
                continue;
            }
            match maps[f].as_ref().and_then(|m| m.get(&r[f].key())) {
                Some(x) => out.push_str(&format!("<x v=\"{x}\"/>")),
                None => out.push_str(&value_xml(&r[f], date1904)),
            }
        }
        out.push_str("</r>");
    }
    out.push_str("</pivotCacheRecords>");
    out
}

// ----------------------------------------------------------------------------------------------------
// The table.

/// The definition part of a table.
pub fn table_definition(
    table: &PivotTable,
    cache: &PivotCache,
    styles: &mut Styles,
    cache_id: u32,
) -> String {
    let mut out = String::from(XML_HEAD);
    out.push_str(&format!("<pivotTableDefinition xmlns=\"{MAIN_NS}\""));
    attr(&mut out, "name", &table.name);
    attr(&mut out, "cacheId", &cache_id.to_string());
    if table.values_on_rows() {
        attr(&mut out, "dataOnRows", "1");
    }
    if let Some(p) = table.values_position() {
        attr(&mut out, "dataPosition", &p.to_string());
    }
    attr(&mut out, "dataCaption", &table.options.data_caption);
    let o = &table.options;
    if let Some(c) = &o.grand_total_caption {
        attr(&mut out, "grandTotalCaption", c);
    }
    if let Some(c) = &o.error_caption {
        attr(&mut out, "errorCaption", c);
        attr(&mut out, "showError", "1");
    }
    if let Some(c) = &o.missing_caption {
        attr(&mut out, "missingCaption", c);
    }
    for (k, v) in &o.attrs {
        attr(&mut out, k, v);
    }
    match o.form {
        Form::Compact => {
            attr(&mut out, "outline", "1");
            attr(&mut out, "outlineData", "1");
        }
        Form::Outline => {
            attr(&mut out, "compact", "0");
            attr(&mut out, "outline", "1");
            attr(&mut out, "outlineData", "1");
            attr(&mut out, "compactData", "0");
        }
        Form::Tabular => {
            attr(&mut out, "compact", "0");
            attr(&mut out, "compactData", "0");
        }
    }
    if !o.grand_total_row {
        attr(&mut out, "rowGrandTotals", "0");
    }
    if !o.grand_total_col {
        attr(&mut out, "colGrandTotals", "0");
    }
    if !o.show_headers {
        attr(&mut out, "showHeaders", "0");
    }
    if let Some(c) = &o.row_header_caption {
        attr(&mut out, "rowHeaderCaption", c);
    }
    if let Some(c) = &o.col_header_caption {
        attr(&mut out, "colHeaderCaption", c);
    }
    if o.page_over_then_down {
        attr(&mut out, "pageOverThenDown", "1");
    }
    if o.page_wrap > 0 {
        attr(&mut out, "pageWrap", &o.page_wrap.to_string());
    }
    out.push('>');
    out.push_str(&location(table));
    out.push_str(&pivot_fields(table, cache, styles));
    let rows = table.row_axis();
    let cols = table.col_axis();
    let axis_fields = |axis: &[AxisEntry]| -> String {
        let mut s = format!("count=\"{}\">", axis.len());
        for e in axis {
            match e {
                AxisEntry::Field(f) => s.push_str(&format!("<field x=\"{f}\"/>")),
                AxisEntry::Values => s.push_str("<field x=\"-2\"/>"),
            }
        }
        s
    };
    if !rows.is_empty() {
        out.push_str(&format!("<rowFields {}</rowFields>", axis_fields(&rows)));
        out.push_str(&items_xml("rowItems", &table.lines.rows, rows.contains(&AxisEntry::Values)));
    } else if !table.data.is_empty() || !cols.is_empty() {
        out.push_str("<rowItems count=\"1\"><i/></rowItems>");
    }
    if !cols.is_empty() {
        out.push_str(&format!("<colFields {}</colFields>", axis_fields(&cols)));
        out.push_str(&items_xml("colItems", &table.lines.cols, cols.contains(&AxisEntry::Values)));
    } else if !table.data.is_empty() || !rows.is_empty() {
        out.push_str("<colItems count=\"1\"><i/></colItems>");
    }
    if !table.pages.is_empty() {
        out.push_str(&format!("<pageFields count=\"{}\">", table.pages.len()));
        for p in &table.pages {
            out.push_str(&format!("<pageField fld=\"{}\"", p.field));
            if let Some(i) = p.item {
                out.push_str(&format!(" item=\"{i}\""));
            }
            out.push_str(" hier=\"-1\"/>");
        }
        out.push_str("</pageFields>");
    }
    out.push_str(&data_fields(table, styles));
    if let Some(x) = &table.kept.formats {
        out.push_str(x);
    }
    if let Some(x) = &table.kept.conditional_formats {
        out.push_str(x);
    }
    if let Some(x) = &table.kept.chart_formats {
        out.push_str(x);
    }
    out.push_str(&style_info(table));
    out.push_str(&filters(table));
    if let Some(x) = &table.kept.ext_lst {
        out.push_str(x);
    } else if table.data.len() >= 2
        && !table.values_on_rows()
        && !table.cols.iter().any(|e| matches!(e, AxisEntry::Field(_)))
    {
        // With only the value fields across the top Excel leaves out the row that says Values.
        out.push_str("<extLst><ext uri=\"{962EF5D1-5CA2-4c93-8EF4-DBF5C05439D2}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:pivotTableDefinition hideValuesRow=\"1\" xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\"/></ext></extLst>");
    }
    out.push_str("</pivotTableDefinition>");
    out
}

fn location(table: &PivotTable) -> String {
    let mut out = String::from("<location");
    attr(&mut out, "ref", &table.location.a1());
    attr(&mut out, "firstHeaderRow", &table.first_header_row.to_string());
    attr(&mut out, "firstDataRow", &table.first_data_row.to_string());
    attr(&mut out, "firstDataCol", &table.first_data_col.to_string());
    if table.row_page_count > 0 {
        attr(&mut out, "rowPageCount", &table.row_page_count.to_string());
        attr(&mut out, "colPageCount", &table.col_page_count.to_string());
    }
    out.push_str("/>");
    out
}

fn pivot_fields(table: &PivotTable, cache: &PivotCache, styles: &mut Styles) -> String {
    let mut out = format!("<pivotFields count=\"{}\">", table.fields.len());
    for (f, pf) in table.fields.iter().enumerate() {
        out.push_str(&pivot_field(table, cache, f, pf, styles));
    }
    out.push_str("</pivotFields>");
    out
}

fn pivot_field(
    table: &PivotTable,
    cache: &PivotCache,
    f: usize,
    pf: &PivotField,
    styles: &mut Styles,
) -> String {
    let mut out = String::from("<pivotField");
    if let Some(n) = &pf.name {
        attr(&mut out, "name", n);
    }
    let axis = if table.rows.contains(&AxisEntry::Field(f))
        || table.row_axis().contains(&AxisEntry::Field(f))
    {
        Some("axisRow")
    } else if table.col_axis().contains(&AxisEntry::Field(f)) {
        Some("axisCol")
    } else if table.pages.iter().any(|p| p.field == f) {
        Some("axisPage")
    } else {
        None
    };
    if let Some(a) = axis {
        attr(&mut out, "axis", a);
    }
    if table.is_data_field(f) {
        attr(&mut out, "dataField", "1");
    }
    if pf.form != Form::Compact {
        attr(&mut out, "compact", "0");
    }
    if let Some(code) = &pf.num_fmt {
        attr(&mut out, "numFmtId", &styles.format_id(code).to_string());
    } else if cache.fields[f].num_fmt_id != 0 {
        attr(&mut out, "numFmtId", &cache.fields[f].num_fmt_id.to_string());
    }
    if pf.form == Form::Tabular {
        attr(&mut out, "outline", "0");
    }
    if !pf.show_all {
        attr(&mut out, "showAll", "0");
    }
    if pf.sort != SortType::Manual {
        attr(
            &mut out,
            "sortType",
            if pf.sort == SortType::Ascending { "ascending" } else { "descending" },
        );
    }
    if pf.blank_row {
        attr(&mut out, "insertBlankRow", "1");
    }
    if !pf.subtotal_top {
        attr(&mut out, "subtotalTop", "0");
    }
    if pf.multi_page {
        attr(&mut out, "multipleItemSelectionAllowed", "1");
    }
    if pf.filters.iter().any(|x| x.kind.is_value()) {
        attr(&mut out, "measureFilter", "1");
    }
    if !pf.subtotals.automatic && pf.subtotals.funcs.is_empty() {
        attr(&mut out, "defaultSubtotal", "0");
    }
    for func in &pf.subtotals.funcs {
        attr(&mut out, func.subtotal_attr(), "1");
    }
    for (k, v) in &pf.attrs {
        attr(&mut out, k, v);
    }
    let has_items = !pf.items.is_empty();
    let repeat = pf.repeat_labels;
    if !has_items && pf.auto_sort.is_none() && !repeat {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    if has_items {
        out.push_str(&items_element(pf));
    }
    if let Some(a) = &pf.auto_sort {
        out.push_str(&format!(
            "<autoSortScope><pivotArea dataOnly=\"0\" outline=\"0\" fieldPosition=\"0\"><references count=\"1\"><reference field=\"4294967294\" count=\"1\" selected=\"0\"><x v=\"{}\"/></reference></references></pivotArea></autoSortScope>",
            a.data
        ));
    }
    if repeat {
        out.push_str("<extLst><ext uri=\"{2946ED86-A175-432a-8AC1-64E0C546D7DE}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:pivotField fillDownLabels=\"1\"/></ext></extLst>");
    }
    out.push_str("</pivotField>");
    out
}

fn items_element(pf: &PivotField) -> String {
    let mut body = String::new();
    let mut count = 0;
    for item in &pf.items {
        body.push_str("<item");
        if item.hidden {
            attr(&mut body, "h", "1");
        }
        if let Some(n) = &item.name {
            attr(&mut body, "n", n);
        }
        if item.calculated {
            attr(&mut body, "f", "1");
        }
        attr(&mut body, "x", &item.x.to_string());
        body.push_str("/>");
        count += 1;
    }
    if pf.subtotals.automatic {
        body.push_str("<item t=\"default\"/>");
        count += 1;
    }
    for func in Func::ALL {
        if pf.subtotals.funcs.contains(&func) {
            body.push_str(&format!("<item t=\"{}\"/>", func.item_type()));
            count += 1;
        }
    }
    format!("<items count=\"{count}\">{body}</items>")
}

/// `rowItems` or `colItems`: one `i` for each line.
fn items_xml(tag: &str, lines: &[Line], has_values: bool) -> String {
    let mut body = String::new();
    let mut count = 0;
    let mut previous: Vec<usize> = Vec::new();
    for l in lines {
        let mut xs: Vec<usize> = l
            .path
            .iter()
            .map(|s| match s {
                Seg::Item(p) => *p,
                Seg::Data(d) => *d,
            })
            .collect();
        let mut t: Option<&str> = None;
        let mut i: Option<usize> = None;
        match l.kind {
            LineKind::Leaf | LineKind::Header => {
                i = l.path.iter().find_map(|s| match s {
                    Seg::Data(d) => Some(*d),
                    Seg::Item(_) => None,
                });
            }
            LineKind::Subtotal => {
                t = Some(l.func.map_or("default", |f| f.item_type()));
                if matches!(l.path.last(), Some(Seg::Data(_))) {
                    xs.pop();
                }
                if has_values {
                    i = l.data;
                }
            }
            LineKind::Grand => {
                t = Some("grand");
                xs = vec![0];
                if has_values {
                    i = l.data;
                }
            }
            LineKind::Blank => t = Some("blank"),
        }
        let r = if l.kind == LineKind::Grand {
            0
        } else {
            xs.iter()
                .zip(previous.iter())
                .take_while(|(a, b)| a == b)
                .count()
                .min(xs.len().saturating_sub(1))
        };
        previous = xs.clone();
        body.push_str("<i");
        if let Some(t) = t {
            attr(&mut body, "t", t);
        }
        if r > 0 {
            attr(&mut body, "r", &r.to_string());
        }
        if let Some(i) = i.filter(|i| *i > 0) {
            attr(&mut body, "i", &i.to_string());
        }
        if xs.len() > r || t.is_some() {
            body.push('>');
            for x in &xs[r.min(xs.len())..] {
                if *x == 0 {
                    body.push_str("<x/>");
                } else {
                    body.push_str(&format!("<x v=\"{x}\"/>"));
                }
            }
            body.push_str("</i>");
        } else {
            body.push_str("/>");
        }
        count += 1;
    }
    if count == 0 {
        return format!("<{tag} count=\"1\"><i/></{tag}>");
    }
    format!("<{tag} count=\"{count}\">{body}</{tag}>")
}

fn data_fields(table: &PivotTable, styles: &mut Styles) -> String {
    if table.data.is_empty() {
        return String::new();
    }
    let mut out = format!("<dataFields count=\"{}\">", table.data.len());
    for d in &table.data {
        out.push_str("<dataField");
        attr(&mut out, "name", &d.name);
        attr(&mut out, "fld", &d.field.to_string());
        if d.func != Func::Sum {
            attr(&mut out, "subtotal", d.func.xml());
        }
        if d.show != ShowAs::Normal && !d.show.is_extension() {
            attr(&mut out, "showDataAs", d.show.xml());
        }
        attr(&mut out, "baseField", &d.base_field.to_string());
        attr(&mut out, "baseItem", &d.base_item.to_string());
        if let Some(code) = &d.num_fmt {
            attr(&mut out, "numFmtId", &styles.format_id(code).to_string());
        }
        if d.show.is_extension() {
            out.push_str(&format!(
                "><extLst><ext uri=\"{{E15A36E0-9728-4e99-A89B-3F7291B0FE68}}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:dataField pivotShowAs=\"{}\"/></ext></extLst></dataField>",
                d.show.xml()
            ));
        } else {
            out.push_str("/>");
        }
    }
    out.push_str("</dataFields>");
    out
}

fn style_info(table: &PivotTable) -> String {
    let s = &table.style;
    let mut out = String::from("<pivotTableStyleInfo");
    if let Some(n) = &s.name {
        attr(&mut out, "name", n);
    }
    flag_attr(&mut out, "showRowHeaders", s.show_row_headers);
    flag_attr(&mut out, "showColHeaders", s.show_col_headers);
    flag_attr(&mut out, "showRowStripes", s.show_row_stripes);
    flag_attr(&mut out, "showColStripes", s.show_col_stripes);
    flag_attr(&mut out, "showLastColumn", s.show_last_column);
    out.push_str("/>");
    out
}

fn filters(table: &PivotTable) -> String {
    let mut body = String::new();
    let mut count = 0;
    for (f, pf) in table.fields.iter().enumerate() {
        for filter in &pf.filters {
            body.push_str(&filter_xml(f, filter));
            count += 1;
        }
    }
    if count == 0 {
        return String::new();
    }
    format!("<filters count=\"{count}\">{body}</filters>")
}

fn filter_xml(field: usize, f: &Filter) -> String {
    let mut out = format!("<filter fld=\"{field}\"");
    attr(&mut out, "type", f.kind.xml());
    attr(&mut out, "evalOrder", "-1");
    attr(&mut out, "id", &f.id.to_string());
    if let Some(d) = f.data {
        attr(&mut out, "iMeasureFld", &d.to_string());
    }
    if !f.kind.is_top() && !f.kind.is_value() && f.kind != FilterKind::DateBetween {
        attr(&mut out, "stringValue1", &f.value1);
        if let Some(v) = &f.value2 {
            attr(&mut out, "stringValue2", v);
        }
    }
    out.push_str("><autoFilter ref=\"A1\"><filterColumn colId=\"0\">");
    out.push_str(&filter_column(f));
    out.push_str("</filterColumn></autoFilter>");
    if f.kind == FilterKind::DateBetween {
        out.push_str("<extLst><ext uri=\"{0605FD5F-26C8-4aeb-8148-2DB25E43C511}\" xmlns:x15=\"http://schemas.microsoft.com/office/spreadsheetml/2010/11/main\"><x15:pivotFilter useWholeDay=\"1\"/></ext></extLst>");
    }
    out.push_str("</filter>");
    out
}

/// The conditions Excel writes inside a filter, which a refresh reads back.
fn filter_column(f: &Filter) -> String {
    let custom = |op: Option<&str>, val: &str| -> String {
        let mut s = String::from("<customFilter");
        if let Some(op) = op {
            attr(&mut s, "operator", op);
        }
        attr(&mut s, "val", val);
        s.push_str("/>");
        s
    };
    let v1 = &f.value1;
    let v2 = f.value2.clone().unwrap_or_default();
    let (ops, and): (Vec<(Option<&str>, String)>, bool) = match f.kind {
        FilterKind::CaptionEqual | FilterKind::ValueEqual => (vec![(None, v1.clone())], false),
        FilterKind::CaptionNotEqual | FilterKind::ValueNotEqual => {
            (vec![(Some("notEqual"), v1.clone())], false)
        }
        FilterKind::CaptionBeginsWith => (vec![(None, format!("{v1}*"))], false),
        FilterKind::CaptionNotBeginsWith => (vec![(Some("notEqual"), format!("{v1}*"))], false),
        FilterKind::CaptionEndsWith => (vec![(None, format!("*{v1}"))], false),
        FilterKind::CaptionNotEndsWith => (vec![(Some("notEqual"), format!("*{v1}"))], false),
        FilterKind::CaptionContains => (vec![(None, format!("*{v1}*"))], false),
        FilterKind::CaptionNotContains => (vec![(Some("notEqual"), format!("*{v1}*"))], false),
        FilterKind::CaptionGreaterThan | FilterKind::ValueGreaterThan => {
            (vec![(Some("greaterThan"), v1.clone())], false)
        }
        FilterKind::CaptionGreaterThanOrEqual | FilterKind::ValueGreaterThanOrEqual => {
            (vec![(Some("greaterThanOrEqual"), v1.clone())], false)
        }
        FilterKind::CaptionLessThan | FilterKind::ValueLessThan => {
            (vec![(Some("lessThan"), v1.clone())], false)
        }
        FilterKind::CaptionLessThanOrEqual | FilterKind::ValueLessThanOrEqual => {
            (vec![(Some("lessThanOrEqual"), v1.clone())], false)
        }
        FilterKind::CaptionBetween | FilterKind::ValueBetween | FilterKind::DateBetween => {
            (vec![(Some("greaterThanOrEqual"), v1.clone()), (Some("lessThanOrEqual"), v2)], true)
        }
        FilterKind::CaptionNotBetween | FilterKind::ValueNotBetween => {
            (vec![(Some("lessThan"), v1.clone()), (Some("greaterThan"), v2)], false)
        }
        FilterKind::Count | FilterKind::Percent | FilterKind::Sum => {
            let mut s = String::from("<top10");
            if !f.top {
                attr(&mut s, "top", "0");
            }
            if f.kind == FilterKind::Percent {
                attr(&mut s, "percent", "1");
            }
            attr(&mut s, "val", v1);
            attr(&mut s, "filterVal", v1);
            s.push_str("/>");
            return s;
        }
    };
    let mut s = String::from("<customFilters");
    if and {
        s.push_str(" and=\"1\"");
    }
    s.push('>');
    for (op, val) in ops {
        s.push_str(&custom(op, &val));
    }
    s.push_str("</customFilters>");
    s
}

/// The relationships part that points a table at its cache.
pub fn table_rels(cache_target: &str) -> String {
    format!(
        "{XML_HEAD}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"{}\"/></Relationships>",
        super::xml_read::REL_CACHE_DEFINITION,
        escape(cache_target)
    )
}

/// The relationships part that points a cache definition at its records.
pub fn cache_rels(records_target: &str) -> String {
    format!(
        "{XML_HEAD}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"{}\"/></Relationships>",
        super::xml_read::REL_CACHE_RECORDS,
        escape(records_target)
    )
}
