//! The rest of the PivotTable verbs (`task-2360`, the `wiringb` lane): sorting, hiding items, the report
//! filter, label, value and top filters, grouping, calculated fields and items, slicers, timelines, and the
//! `GETPIVOTDATA` text that clicking a pivot cell while typing a formula puts in.

use serde_json::json;
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::pivot::cache::{CacheValue, GroupBy};
use unluminous_sheet::pivot::items::{Line, Seg};
use unluminous_sheet::pivot::session::label_filter;
use unluminous_sheet::pivot::table::{AxisEntry, Filter, FilterKind};
use unluminous_sheet::pivot::SortType;

use super::verbs_a::{after, flag, Out};
use super::verbs_b_pivot::{field_in, func_of, table_of};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

fn said(text: impl Into<String>) -> Answer {
    Answer::said(text)
}

/// Run `slicer`, `timeline` and `getpivotdata`.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "slicer" | "timeline" => super::verbs_b_slicers::run(sheet, verb, args),
        _ => getpivotdata(sheet, args),
    })
}

/// The verbs on one table that this file owns, or `None` for another file's.
pub fn on_table(sheet: &mut Spreadsheet, what: &str, args: &[String]) -> Option<Out> {
    let known = matches!(
        what,
        "sort"
            | "items"
            | "page"
            | "filter"
            | "group"
            | "ungroup"
            | "calculated-field"
            | "calculated-item"
    );
    if !known {
        return None;
    }
    Some(sheet.current_book().and_then(|book| {
        let (t, rest) = table_of(book, args)?;
        let message = match what {
            "sort" => sort(book, t, &rest)?,
            "items" => return items(book, t, &rest),
            "page" => page(book, t, &rest)?,
            "filter" => filter(book, t, &rest)?,
            "group" => group(book, t, &rest)?,
            "ungroup" => {
                let f = field_in(book, t, rest.first().ok_or("ungroup takes a field")?)?;
                book.session.pivot_ungroup(t, f)?;
                "Ungrouped".to_owned()
            }
            "calculated-field" => calculated_field(book, t, &rest)?,
            _ => calculated_item(book, t, &rest)?,
        };
        book.changed();
        book.view.axes = None;
        Ok(said(message).with(super::verbs_b_pivot::table_json(book, t)))
    }))
}

// ------------------------------------------------------------------------------------------------ items

/// The text of every item of a field as the table shows them, in the table's order.
pub fn item_texts(book: &Book, t: usize, f: usize) -> Vec<String> {
    let pivots = &book.session.book.pivots;
    let table = &pivots.tables[t];
    let cache = &pivots.caches[table.cache];
    let source = cache.fields[f].items();
    table.fields[f]
        .items
        .iter()
        .map(|entry| {
            entry.name.clone().unwrap_or_else(|| match source.get(entry.x) {
                Some(CacheValue::Blank) | None => "(blank)".to_owned(),
                Some(CacheValue::Text(t)) => t.to_string(),
                Some(v) => v.to_value().to_text(),
            })
        })
        .collect()
}

/// The place among a field's items of the item a word names.
pub fn item_position(book: &Book, t: usize, f: usize, word: &str) -> Result<usize, String> {
    item_texts(book, t, f)
        .iter()
        .position(|name| name.eq_ignore_ascii_case(word))
        .ok_or_else(|| format!("there is no item `{word}` in that field"))
}

/// `pivot sort <field> ascending|descending|manual [by <value field>]`.
fn sort(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let f = field_in(book, t, args.first().ok_or("sort takes a field")?)?;
    let order = match args.get(1).map(String::as_str) {
        Some("ascending") | Some("asc") => SortType::Ascending,
        Some("descending") | Some("desc") => SortType::Descending,
        Some("manual") => SortType::Manual,
        _ => return Err("sort takes ascending, descending or manual".to_owned()),
    };
    let by = match after(args, "by") {
        Some(name) => Some(
            book.session.book.pivots.tables[t]
                .data
                .iter()
                .position(|d| d.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("there is no value field `{name}`"))?,
        ),
        None => None,
    };
    book.session.pivot_sort(t, f, order, by)?;
    Ok("Sorted".to_owned())
}

/// `pivot items <field> [list|show|hide|only|all] [<item>,<item>]`: which items of a field are shown.
fn items(book: &mut Book, t: usize, args: &[String]) -> Out {
    let f = field_in(book, t, args.first().ok_or("items takes a field")?)?;
    let how = args.get(1).map(String::as_str).unwrap_or("list");
    let names = item_texts(book, t, f);
    let wanted: Vec<usize> = match args.get(2) {
        Some(list) => list
            .split(',')
            .map(|n| item_position(book, t, f, n.trim()))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    for (position, _) in names.iter().enumerate() {
        let visible = match how {
            "show" if wanted.contains(&position) => Some(true),
            "hide" if wanted.contains(&position) => Some(false),
            "only" => Some(wanted.contains(&position)),
            "all" => Some(true),
            _ => None,
        };
        if let Some(visible) = visible {
            book.session.pivot_set_item_visible(t, f, position, visible)?;
        }
    }
    book.changed();
    book.view.axes = None;
    let table = &book.session.book.pivots.tables[t];
    let list: Vec<_> = names
        .iter()
        .zip(&table.fields[f].items)
        .map(|(n, e)| json!({ "name": n, "visible": !e.hidden }))
        .collect();
    Ok(said(format!("{} items", list.len())).with(json!(list)))
}

/// `pivot page <field> <item>|all`: the item a report filter shows.
fn page(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let f = field_in(book, t, args.first().ok_or("page takes a field and an item")?)?;
    let item = match args.get(1).map(String::as_str) {
        Some("all") | None => None,
        Some(name) => Some(item_position(book, t, f, name)?),
    };
    book.session.pivot_set_page_item(t, f, item)?;
    Ok("Filtered".to_owned())
}

// ------------------------------------------------------------------------------------------------ filters

/// The label filter a word names.
fn label_kind(word: &str) -> Option<FilterKind> {
    Some(match word {
        "equals" => FilterKind::CaptionEqual,
        "not-equals" => FilterKind::CaptionNotEqual,
        "begins-with" => FilterKind::CaptionBeginsWith,
        "not-begins-with" => FilterKind::CaptionNotBeginsWith,
        "ends-with" => FilterKind::CaptionEndsWith,
        "not-ends-with" => FilterKind::CaptionNotEndsWith,
        "contains" => FilterKind::CaptionContains,
        "not-contains" => FilterKind::CaptionNotContains,
        "greater-than" => FilterKind::CaptionGreaterThan,
        "greater-than-or-equal" => FilterKind::CaptionGreaterThanOrEqual,
        "less-than" => FilterKind::CaptionLessThan,
        "less-than-or-equal" => FilterKind::CaptionLessThanOrEqual,
        "between" => FilterKind::CaptionBetween,
        "not-between" => FilterKind::CaptionNotBetween,
        _ => return None,
    })
}

/// The value filter that goes with a label filter's word.
fn value_kind(word: &str) -> Option<FilterKind> {
    Some(match word {
        "equals" => FilterKind::ValueEqual,
        "not-equals" => FilterKind::ValueNotEqual,
        "greater-than" => FilterKind::ValueGreaterThan,
        "greater-than-or-equal" => FilterKind::ValueGreaterThanOrEqual,
        "less-than" => FilterKind::ValueLessThan,
        "less-than-or-equal" => FilterKind::ValueLessThanOrEqual,
        "between" => FilterKind::ValueBetween,
        "not-between" => FilterKind::ValueNotBetween,
        _ => return None,
    })
}

/// `pivot filter <field> clear`, `... <kind> <value> [<value2>] [by <value field>]`, or
/// `... top|bottom <n> items|percent|sum [by <value field>]`.
fn filter(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let f = field_in(book, t, args.first().ok_or("filter takes a field")?)?;
    let kind_word = args.get(1).map(String::as_str).unwrap_or("clear");
    let data = match after(args, "by") {
        Some(name) => Some(
            book.session.book.pivots.tables[t]
                .data
                .iter()
                .position(|d| d.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("there is no value field `{name}`"))?,
        ),
        None => None,
    };
    let made = match kind_word {
        "clear" => None,
        "top" | "bottom" => Some(top_filter(args, data)?),
        word => {
            let value = args.get(2).ok_or("a filter needs a value")?;
            let kind = if data.is_some() { value_kind(word) } else { label_kind(word) }
                .ok_or_else(|| format!("a filter is equals, not-equals, begins-with, ends-with, contains, greater-than, less-than, between, top or bottom; got `{word}`"))?;
            let mut filter = label_filter(kind, value);
            filter.data = data;
            filter.value2 = args.get(3).filter(|w| w.as_str() != "by").cloned();
            Some(filter)
        }
    };
    book.session.pivot_set_filter(t, f, made)?;
    Ok(format!("Filter {kind_word}"))
}

fn top_filter(args: &[String], data: Option<usize>) -> Result<Filter, String> {
    let n = args.get(2).ok_or("top and bottom take a number")?;
    let kind = match args.get(3).map(String::as_str).unwrap_or("items") {
        "items" => FilterKind::Count,
        "percent" => FilterKind::Percent,
        "sum" => FilterKind::Sum,
        other => return Err(format!("top and bottom count items, percent or sum; got `{other}`")),
    };
    let mut filter = label_filter(kind, n);
    filter.top = args[1] == "top";
    filter.data = data.or(Some(0));
    Ok(filter)
}

// -------------------------------------------------------------------------------------------------- group

/// `pivot group <field> dates years,quarters,months,days [start N] [end N] [step N]`, `... numbers step N
/// [start N] [end N]`, or `... items <name>=<a>,<b> ...`.
fn group(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let f = field_in(book, t, args.first().ok_or("group takes a field")?)?;
    let number = |key: &str| -> Result<Option<f64>, String> {
        after(args, key)
            .map(|w| w.parse::<f64>().map_err(|_| format!("`{w}` is not a number")))
            .transpose()
    };
    match args.get(1).map(String::as_str) {
        Some("dates") => {
            let units: Vec<GroupBy> = args
                .get(2)
                .ok_or("dates take years, quarters, months, days, hours, minutes or seconds")?
                .split(',')
                .map(group_unit)
                .collect::<Result<_, _>>()?;
            book.session.pivot_group_dates(
                t,
                f,
                &units,
                number("start")?,
                number("end")?,
                number("step")?,
            )?;
        }
        Some("numbers") => {
            let step = number("step")?.ok_or("numbers need a step")?;
            book.session.pivot_group_numbers(t, f, number("start")?, number("end")?, step)?;
        }
        Some("items") => {
            let mut chosen = Vec::new();
            for word in &args[2..] {
                let (name, list) =
                    word.split_once('=').ok_or("an item group is written name=item,item")?;
                let positions: Vec<usize> = list
                    .split(',')
                    .map(|n| item_position(book, t, f, n.trim()))
                    .collect::<Result<_, _>>()?;
                chosen.push((Some(name.to_owned()), positions));
            }
            book.session.pivot_group_items(t, f, &chosen)?;
        }
        _ => return Err("group takes dates, numbers or items".to_owned()),
    }
    Ok("Grouped".to_owned())
}

fn group_unit(word: &str) -> Result<GroupBy, String> {
    Ok(match word {
        "years" | "year" => GroupBy::Years,
        "quarters" | "quarter" => GroupBy::Quarters,
        "months" | "month" => GroupBy::Months,
        "days" | "day" => GroupBy::Days,
        "hours" => GroupBy::Hours,
        "minutes" => GroupBy::Minutes,
        "seconds" => GroupBy::Seconds,
        other => return Err(format!("a date unit is years, quarters, months, days, hours, minutes or seconds; got `{other}`")),
    })
}

/// `pivot calculated-field <name> <formula>`.
fn calculated_field(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let name = args.first().ok_or("calculated-field takes a name and a formula")?;
    let formula = args[1..].join(" ");
    book.session.pivot_add_calculated_field(t, name, &formula)?;
    Ok(format!("Added the calculated field {name}"))
}

/// `pivot calculated-item <field> <name> <formula>`.
fn calculated_item(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let f = field_in(
        book,
        t,
        args.first().ok_or("calculated-item takes a field, a name and a formula")?,
    )?;
    let name = args.get(1).ok_or("calculated-item takes a name and a formula")?;
    book.session.pivot_add_calculated_item(t, f, name, &args[2..].join(" "))?;
    Ok(format!("Added the calculated item {name}"))
}

// -------------------------------------------------------------------------------------------- GETPIVOTDATA

/// `getpivotdata [cell]`: the `GETPIVOTDATA` formula for a cell of a pivot table.
fn getpivotdata(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (at, _) = super::verbs_a::take_cell(book, args);
    let text = getpivotdata_text(book, book.view.sheet, at)
        .ok_or("That cell is not a figure of a PivotTable.")?;
    let value = book.session.book.sheets[book.view.sheet].value_at(at).to_text();
    let _ = flag(args, "formula");
    Ok(said(text.clone()).with(json!({ "formula": text, "value": value })))
}

/// The `GETPIVOTDATA(...)` text that reads the figure at a cell of a pivot table, as Excel writes it when a
/// formula points at the cell: the value field, the table's first cell, then each field and item.
pub fn getpivotdata_text(book: &Book, sheet: usize, at: CellRef) -> Option<String> {
    let pivots = &book.session.book.pivots;
    let t = pivots.at(sheet, at)?;
    let table = &pivots.tables[t];
    let cache = &pivots.caches[table.cache];
    let row = at.row.checked_sub(table.location.first.row + table.first_data_row)? as usize;
    let col = at.col.checked_sub(table.location.first.col + table.first_data_col)? as usize;
    let (r, c) = (table.lines.rows.get(row)?, table.lines.cols.get(col)?);
    if !(r.values && c.values) {
        return None;
    }
    let d = data_field_of(r, c, table.data.len())?;
    let mut parts = vec![
        format!("\"{}\"", table.data.get(d)?.name.replace('"', "\"\"")),
        format!(
            "${}${}",
            unluminous_sheet::coord::column_name(table.location.first.col),
            table.location.first.row + 1
        ),
    ];
    pairs(book, t, &table.row_axis(), r, &mut parts);
    pairs(book, t, &table.col_axis(), c, &mut parts);
    for page in &table.pages {
        if let Some(item) = page.item {
            parts.push(format!("\"{}\"", table.caption(cache, page.field)));
            parts.push(literal(&item_texts(book, t, page.field)[item]));
        }
    }
    Some(format!("GETPIVOTDATA({})", parts.join(",")))
}

/// Which value field a crossing of a row line and a column line shows.
fn data_field_of(r: &Line, c: &Line, count: usize) -> Option<usize> {
    let from = |l: &Line| {
        l.data.or_else(|| {
            l.path.iter().find_map(|s| match s {
                Seg::Data(d) => Some(*d),
                Seg::Item(_) => None,
            })
        })
    };
    from(r).or_else(|| from(c)).or((count > 0).then_some(0))
}

/// The field and item pairs of one line, outermost first.
fn pairs(book: &Book, t: usize, axis: &[AxisEntry], line: &Line, parts: &mut Vec<String>) {
    let table = &book.session.book.pivots.tables[t];
    let cache = &book.session.book.pivots.caches[table.cache];
    let fields: Vec<usize> = axis
        .iter()
        .filter_map(|e| match e {
            AxisEntry::Field(f) => Some(*f),
            AxisEntry::Values => None,
        })
        .collect();
    let items = line.path.iter().filter_map(|s| match s {
        Seg::Item(p) => Some(*p),
        Seg::Data(_) => None,
    });
    for (f, p) in fields.into_iter().zip(items) {
        if let Some(text) = item_texts(book, t, f).get(p) {
            parts.push(format!("\"{}\"", table.caption(cache, f).replace('"', "\"\"")));
            parts.push(literal(text));
        }
    }
}

/// An item as a formula argument: a number as it is, anything else in quotes.
fn literal(text: &str) -> String {
    if text.parse::<f64>().is_ok() {
        text.to_owned()
    } else {
        format!("\"{}\"", text.replace('"', "\"\""))
    }
}

/// The name of the function a word names, kept for the verbs that take a summary.
pub fn summary_of(word: &str) -> Result<unluminous_sheet::pivot::Func, String> {
    func_of(word)
}
