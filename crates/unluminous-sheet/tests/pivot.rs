//! Pivot tables against Excel.
//!
//! `tests/pivot/cases.json` holds datasets and cases (which fields go where, which settings are changed).
//! `tests/pivot/excel.jsonl` holds, for every case, the cells Excel laid out when it built the same pivot
//! (`tools/excel-oracle/pivot-check.ps1 -Cases tests/pivot/cases.json`). The test builds every case with the
//! engine and compares the cells one by one: the value at full precision, the number format, and the text
//! where the format is not General.
//!
//! `PIVOT_CASE=<id>` runs one case and prints what both sides have.


use std::collections::HashMap;

use serde_json::Value as Json;
use unluminous_sheet::edit::Session;
use unluminous_sheet::pivot::session::{Area, FieldSettings, PivotSource, ValueSettings};
use unluminous_sheet::pivot::table::{Filter, FilterKind, Subtotals};
use unluminous_sheet::pivot::{Form, Func, ShowAs, SortType, BASE_NEXT, BASE_PREVIOUS};
use unluminous_sheet::styles::StyleChange;
use unluminous_sheet::{Cell, CellRef, Range, Sheet, Value, Workbook};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("pivot")
}

fn read_json(name: &str) -> Json {
    let text = std::fs::read_to_string(root().join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn expected() -> HashMap<String, Json> {
    let text = std::fs::read_to_string(root().join("excel.jsonl")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let j: Json = serde_json::from_str(l).unwrap();
            (j["id"].as_str().unwrap().to_owned(), j)
        })
        .collect()
}

/// A workbook with a sheet for each dataset and an empty sheet for each case.
fn workbook(spec: &Json) -> Workbook {
    let mut book = Workbook::new();
    book.sheets.clear();
    for (name, set) in spec["datasets"].as_object().unwrap() {
        let mut sheet = Sheet::new(format!("Data_{name}"));
        let headers = set["headers"].as_array().unwrap();
        for (c, h) in headers.iter().enumerate() {
            sheet.cells.insert((0, c as u32), Cell::value(Value::text(h.as_str().unwrap())));
        }
        let formats = set["formats"].as_array().unwrap();
        for (r, row) in set["rows"].as_array().unwrap().iter().enumerate() {
            for (c, v) in row.as_array().unwrap().iter().enumerate() {
                let value = match v {
                    Json::Null => continue,
                    Json::String(s) => Value::text(s),
                    Json::Bool(b) => Value::Bool(*b),
                    other => Value::Number(other.as_f64().unwrap()),
                };
                let code = formats[c].as_str().unwrap_or("General");
                let style = if code == "General" {
                    0
                } else {
                    book.styles.derive(0, &StyleChange::NumberFormat(code.to_owned()))
                };
                sheet.cells.insert(((r + 1) as u32, c as u32), Cell { value, style, ..Default::default() });
            }
        }
        book.sheets.push(sheet);
    }
    for case in spec["cases"].as_array().unwrap() {
        book.sheets.push(Sheet::new(sheet_name(case["id"].as_str().unwrap())));
    }
    book
}

fn sheet_name(id: &str) -> String {
    format!("P_{}", &id[..id.len().min(25)])
}

fn func_of(name: &str) -> Func {
    match name {
        "sum" => Func::Sum,
        "count" => Func::Count,
        "average" => Func::Average,
        "max" => Func::Max,
        "min" => Func::Min,
        "product" => Func::Product,
        "countnums" => Func::CountNums,
        "stdev" => Func::StdDev,
        "stdevp" => Func::StdDevP,
        "var" => Func::Var,
        "varp" => Func::VarP,
        other => panic!("function {other}"),
    }
}

fn show_of(name: &str) -> ShowAs {
    match name {
        "none" => ShowAs::Normal,
        "difference" => ShowAs::Difference,
        "percentof" => ShowAs::Percent,
        "percentdifference" => ShowAs::PercentDiff,
        "running" => ShowAs::RunTotal,
        "percentrow" => ShowAs::PercentOfRow,
        "percentcolumn" => ShowAs::PercentOfCol,
        "percenttotal" => ShowAs::PercentOfTotal,
        "index" => ShowAs::Index,
        "percentparentrow" => ShowAs::PercentOfParentRow,
        "percentparentcolumn" => ShowAs::PercentOfParentCol,
        "percentparent" => ShowAs::PercentOfParent,
        "percentrunning" => ShowAs::PercentRunTotal,
        "rankasc" => ShowAs::RankAscending,
        "rankdesc" => ShowAs::RankDescending,
        other => panic!("show {other}"),
    }
}

fn field_index(session: &Session, t: usize, name: &str) -> usize {
    let table = &session.book.pivots.tables[t];
    let cache = &session.book.pivots.caches[table.cache];
    cache
        .fields
        .iter()
        .position(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn names(j: &Json) -> Vec<String> {
    match j {
        Json::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

/// Build one case in its sheet.
fn build(session: &mut Session, spec: &Json, case: &Json) -> Result<usize, String> {
    let id = case["id"].as_str().unwrap();
    let sheet = session.book.sheet_index(&sheet_name(id)).unwrap();
    let data_name = format!("Data_{}", case["data"].as_str().unwrap());
    let data = session.book.sheet_index(&data_name).unwrap();
    let used = session.book.sheets[data].used_range().unwrap();
    let at = CellRef::parse(case["at"].as_str().unwrap_or("A6")).unwrap();
    let t = session.pivot_create(PivotSource::Range { sheet: data, range: used }, sheet, at, Some("PT1"))?;
    if let Some(layout) = case["layout"].as_str() {
        session.pivot_set_form(t, form_of(layout))?;
    }
    for (i, n) in names(&case["rows"]).iter().enumerate() {
        let f = field_index(session, t, n);
        session.pivot_move_field(t, f, Area::Rows, Some(i))?;
    }
    for (i, n) in names(&case["cols"]).iter().enumerate() {
        let f = field_index(session, t, n);
        session.pivot_move_field(t, f, Area::Cols, Some(i))?;
    }
    for (i, n) in names(&case["pages"]).iter().enumerate() {
        let f = field_index(session, t, n);
        session.pivot_move_field(t, f, Area::Pages, Some(i))?;
    }
    for v in case["values"].as_array().into_iter().flatten() {
        let f = field_index(session, t, v["field"].as_str().unwrap());
        let d = session.pivot_add_value(t, f, func_of(v["func"].as_str().unwrap()), v["name"].as_str())?;
        let mut settings = ValueSettings::default();
        if let Some(show) = v["show"].as_str() {
            settings.show = Some(show_of(show));
        }
        if let Some(base) = v["base"].as_str() {
            settings.base_field = Some(field_index(session, t, base));
        }
        if let Some(item) = v["item"].as_str() {
            settings.base_item = Some(base_item(session, t, settings.base_field.unwrap_or(0), item));
        }
        if let Some(fmt) = v["fmt"].as_str() {
            settings.num_fmt = Some(Some(fmt.to_owned()));
        }
        session.pivot_value_settings(t, d, settings)?;
    }
    if case["values_on"].as_str() == Some("rows") {
        let pos = case["values_pos"].as_u64().map(|p| p as usize - 1);
        session.pivot_move_values(t, Area::Rows, pos)?;
    } else if let Some(pos) = case["values_pos"].as_u64() {
        session.pivot_move_values(t, Area::Cols, Some(pos as usize - 1))?;
    }
    if case["repeat"].as_bool() == Some(true) {
        for f in 0..session.book.pivots.tables[t].fields.len() {
            let s = FieldSettings { repeat_labels: Some(true), ..Default::default() };
            session.pivot_field_settings(t, f, s)?;
        }
    }
    if let Some(g) = case["grand"].as_object() {
        let row = g.get("row").and_then(Json::as_bool).unwrap_or(true);
        let col = g.get("col").and_then(Json::as_bool).unwrap_or(true);
        session.pivot_set_grand_totals(t, row, col)?;
    }
    field_options(session, t, case)?;
    if let Some(sel) = case["page_select"].as_object() {
        for (name, item) in sel {
            let f = field_index(session, t, name);
            let pos = item_position(session, t, f, item.as_str().unwrap());
            session.pivot_set_page_item(t, f, Some(pos))?;
        }
    }
    let _ = spec;
    Ok(t)
}

fn form_of(name: &str) -> Form {
    match name {
        "tabular" => Form::Tabular,
        "outline" => Form::Outline,
        _ => Form::Compact,
    }
}

fn item_position(session: &Session, t: usize, field: usize, label: &str) -> usize {
    let table = &session.book.pivots.tables[t];
    let cache = &session.book.pivots.caches[table.cache];
    let calc = unluminous_sheet::pivot::compute::Calc::build(table, cache, false);
    (0..table.fields[field].items.len())
        .find(|p| calc.label(field, *p).eq_ignore_ascii_case(label))
        .unwrap_or_else(|| panic!("no item {label}"))
}

fn base_item(session: &Session, t: usize, field: usize, label: &str) -> u32 {
    match label {
        "(previous)" => BASE_PREVIOUS,
        "(next)" => BASE_NEXT,
        other => item_position(session, t, field, other) as u32,
    }
}

fn field_options(session: &mut Session, t: usize, case: &Json) -> Result<(), String> {
    let Some(fields) = case["fields"].as_object() else { return Ok(()) };
    for (name, o) in fields {
        let f = field_index(session, t, name);
        let mut s = FieldSettings::default();
        match &o["subtotals"] {
            Json::String(x) if x == "off" => s.subtotals = Some(Subtotals::none()),
            Json::Array(list) => {
                let mut sub = Subtotals { automatic: false, funcs: Vec::new() };
                for x in list {
                    match x.as_str().unwrap() {
                        "automatic" => sub.automatic = true,
                        other => sub.funcs.push(func_of(other)),
                    }
                }
                s.subtotals = Some(sub);
            }
            _ => {}
        }
        if let Some(at) = o["subtotal_at"].as_str() {
            s.subtotal_top = Some(at != "bottom");
        }
        if let Some(b) = o["blank_row"].as_bool() {
            s.blank_row = Some(b);
        }
        if let Some(b) = o["repeat"].as_bool() {
            s.repeat_labels = Some(b);
        }
        if let Some(l) = o["layout"].as_str() {
            s.form = Some(form_of(l));
        }
        if let Some(b) = o["show_all_items"].as_bool() {
            s.show_all = Some(b);
        }
        if let Some(b) = o["multi_page"].as_bool() {
            s.multi_page = Some(b);
        }
        if let Some(c) = o["caption"].as_str() {
            s.name = Some(Some(c.to_owned()));
        }
        session.pivot_field_settings(t, f, s)?;
        if let Some(order) = o["sort"].as_str() {
            let by = o["sort_by"].as_str().map(|n| {
                session.book.pivots.tables[t].data.iter().position(|d| d.name == n).unwrap()
            });
            let order = if order == "desc" { SortType::Descending } else { SortType::Ascending };
            session.pivot_sort(t, f, order, by)?;
        }
        for item in names(&o["hide"]) {
            let p = item_position(session, t, f, &item);
            session.pivot_set_item_visible(t, f, p, false)?;
        }
        if let Some(order) = o["order"].as_array() {
            for (n, item) in order.iter().enumerate() {
                let p = item_position(session, t, f, item.as_str().unwrap());
                session.pivot_move_item(t, f, p, n)?;
            }
        }
        if let Some(filter) = o["filter"].as_object() {
            let kind = filter_kind(filter["type"].as_str().unwrap());
            let data = filter.get("data").and_then(Json::as_str).map(|n| {
                session.book.pivots.tables[t].data.iter().position(|d| d.name == n).unwrap()
            });
            let value1 = match &filter["value"] {
                Json::String(s) => s.clone(),
                other => other.to_string(),
            };
            let value2 = filter.get("value2").map(|v| match v {
                Json::String(s) => s.clone(),
                other => other.to_string(),
            });
            let top = !matches!(filter["type"].as_str(), Some("bottom" | "bottompercent" | "bottomsum"));
            session.pivot_set_filter(
                t,
                f,
                Some(Filter { kind, data, value1, value2, top, id: 1 }),
            )?;
        }
    }
    Ok(())
}

fn filter_kind(name: &str) -> FilterKind {
    match name {
        "top" | "bottom" => FilterKind::Count,
        "toppercent" | "bottompercent" => FilterKind::Percent,
        "topsum" | "bottomsum" => FilterKind::Sum,
        "veq" => FilterKind::ValueEqual,
        "vne" => FilterKind::ValueNotEqual,
        "vgt" => FilterKind::ValueGreaterThan,
        "vge" => FilterKind::ValueGreaterThanOrEqual,
        "vlt" => FilterKind::ValueLessThan,
        "vle" => FilterKind::ValueLessThanOrEqual,
        "vbetween" => FilterKind::ValueBetween,
        "vnotbetween" => FilterKind::ValueNotBetween,
        "eq" => FilterKind::CaptionEqual,
        "ne" => FilterKind::CaptionNotEqual,
        "begins" => FilterKind::CaptionBeginsWith,
        "notbegins" => FilterKind::CaptionNotBeginsWith,
        "ends" => FilterKind::CaptionEndsWith,
        "notends" => FilterKind::CaptionNotEndsWith,
        "contains" => FilterKind::CaptionContains,
        "notcontains" => FilterKind::CaptionNotContains,
        "gt" => FilterKind::CaptionGreaterThan,
        "ge" => FilterKind::CaptionGreaterThanOrEqual,
        "lt" => FilterKind::CaptionLessThan,
        "le" => FilterKind::CaptionLessThanOrEqual,
        "between" => FilterKind::CaptionBetween,
        "notbetween" => FilterKind::CaptionNotBetween,
        other => panic!("filter {other}"),
    }
}

/// How the two sides differ at one cell, if they do.
fn compare_cell(session: &Session, sheet: usize, at: CellRef, want: &Json) -> Option<String> {
    let book = &session.book;
    let have = book.sheets[sheet].value_at(at);
    let style = book.sheets[sheet].cell(at).map_or(0, |c| c.style);
    let code = book.styles.format_code(style);
    if want.is_null() {
        return match have {
            Value::Empty => None,
            other => Some(format!("{}: Excel has nothing, we have {other:?}", at.a1())),
        };
    }
    let kind = want["k"].as_str().unwrap();
    let text = want["v"].as_str().unwrap();
    let ok = match (&have, kind) {
        (Value::Number(n), "n") => {
            let w: f64 = text.parse().unwrap();
            (n - w).abs() <= 1e-9 * w.abs().max(1e-300) || (n - w).abs() < 1e-12
        }
        (Value::Text(t), "s") => &**t == text,
        (Value::Bool(b), "b") => (text == "True") == *b || (text == "TRUE") == *b,
        (Value::Error(e), "e") => e.text() == text,
        _ => false,
    };
    if !ok {
        return Some(format!("{}: Excel has {kind}:{text}, we have {have:?}", at.a1()));
    }
    let want_format = want["f"].as_str().unwrap_or("General");
    if !have.is_empty() && code != want_format {
        return Some(format!("{}: Excel format {want_format}, ours {code}", at.a1()));
    }
    None
}

fn run_case(session: &mut Session, spec: &Json, case: &Json, want: &Json) -> Vec<String> {
    let id = case["id"].as_str().unwrap();
    if let Some(error) = want.get("error") {
        return vec![format!("Excel could not build {id}: {error}")];
    }
    let t = match build(session, spec, case) {
        Ok(t) => t,
        Err(e) => return vec![format!("building failed: {e}")],
    };
    let sheet = session.book.pivots.tables[t].sheet;
    let table = &session.book.pivots.tables[t];
    let mut problems = Vec::new();
    let range = want["range"].as_str().unwrap();
    if table.full_range().a1() != range {
        problems.push(format!("range: Excel {range}, ours {}", table.full_range().a1()));
    }
    let data = want["data"].as_str().unwrap();
    if table.location.a1() != data {
        problems.push(format!("data range: Excel {data}, ours {}", table.location.a1()));
    }
    let first = Range::parse(range).unwrap().first;
    for (i, row) in want["cells"].as_array().unwrap().iter().enumerate() {
        for (j, cell) in row.as_array().unwrap().iter().enumerate() {
            let at = CellRef::new(first.row + i as u32, first.col + j as u32);
            if let Some(p) = compare_cell(session, sheet, at, cell) {
                problems.push(p);
            }
        }
    }
    // Nothing outside the range.
    let table_range = Range::parse(range).unwrap();
    for (&(r, c), cell) in &session.book.sheets[sheet].cells {
        if !table_range.contains(CellRef::new(r, c)) && !cell.value.is_empty() {
            problems.push(format!("{}: we have {:?} outside the table", CellRef::new(r, c).a1(), cell.value));
        }
    }
    problems
}

#[test]
fn pivots_match_excel() {
    let spec = read_json("cases.json");
    let expected = expected();
    let only = std::env::var("PIVOT_CASE").ok();
    let mut failed = Vec::new();
    let mut checked = 0;
    for case in spec["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        if only.as_deref().is_some_and(|o| o != id) {
            continue;
        }
        let Some(want) = expected.get(id) else { continue };
        let book = workbook(&spec);
        let mut session = Session::new(book, 46319.5);
        let problems = run_case(&mut session, &spec, case, want);
        checked += 1;
        if !problems.is_empty() {
            failed.push((id.to_owned(), problems));
        }
    }
    for (id, problems) in &failed {
        println!("--- {id}");
        for p in problems.iter().take(14) {
            println!("    {p}");
        }
        if problems.len() > 14 {
            println!("    ... and {} more", problems.len() - 14);
        }
    }
    println!("{} of {} cases match Excel", checked - failed.len(), checked);
    assert!(failed.is_empty(), "{} cases differ from Excel", failed.len());
}
