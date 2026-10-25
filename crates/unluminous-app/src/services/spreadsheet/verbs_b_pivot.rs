//! The PivotTable verbs (`task-2360`, the `wiringb` lane): Insert PivotTable from the selection or a table, the
//! field list with its four areas, value field settings with summarise by and Show Values As, field
//! settings, layout, totals, refresh and change source, and the read back of every pivot table. The filters,
//! groups, calculated fields, slicers and `GETPIVOTDATA` text are in [`super::verbs_b_pivot_more`]. Every
//! verb calls the engine's `pivot_*` method of the same name.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::pivot::session::{Area, FieldSettings, PivotSource, ValueSettings};
use unluminous_sheet::pivot::table::{AxisEntry, Subtotals};
use unluminous_sheet::pivot::{Form, Func, PivotTable, ShowAs};

use super::actions::current_region;
use super::commands::parse_range;
use super::verbs_a::{after, on_off, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

fn said(text: impl Into<String>) -> Answer {
    Answer::said(text)
}

/// Run a pivot verb, or answer `None` for a verb this file does not own.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "pivots" => pivots(sheet),
        "pivot" => pivot(sheet, args),
        "slicer" | "timeline" | "getpivotdata" => {
            return super::verbs_b_pivot_more::run(sheet, verb, args)
        }
        _ => return None,
    })
}

/// `pivot <what> ...`.
fn pivot(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let Some(what) = args.first().map(|w| w.to_ascii_lowercase()) else {
        return pivots(sheet);
    };
    let rest = &args[1..];
    match what.as_str() {
        "create" | "new" | "insert" => create(sheet, rest),
        "list" => pivots(sheet),
        "refresh-all" => refresh_all(sheet),
        _ => {
            if let Some(answer) = super::verbs_b_pivot_more::on_table(sheet, &what, rest) {
                return answer;
            }
            on_table(sheet, &what, rest)
        }
    }
}

/// The pivot table a verb acts on: the one named after `in`, or the one the active cell is in, or the only one.
pub fn table_of(book: &Book, args: &[String]) -> Result<(usize, Vec<String>), String> {
    let mut words: Vec<String> = args.to_vec();
    let pivots = &book.session.book.pivots;
    if let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("in")) {
        let name = words.get(i + 1).ok_or("`in` takes the name of a PivotTable")?.clone();
        let t = pivots.find(&name).ok_or_else(|| format!("there is no PivotTable named `{name}`"))?;
        words.drain(i..=i + 1);
        return Ok((t, words));
    }
    if let Some(t) = pivots.at(book.view.sheet, book.view.cursor) {
        return Ok((t, words));
    }
    match pivots.tables.len() {
        0 => Err("There is no PivotTable in this workbook. `pivot create` makes one.".to_owned()),
        1 => Ok((0, words)),
        _ => Err("Name the PivotTable with `in <name>`; `pivots` lists them.".to_owned()),
    }
}

/// The field a word names: its caption, its name in the source, or `#3` for the third.
pub fn field_of(table: &PivotTable, cache: &unluminous_sheet::pivot::PivotCache, word: &str) -> Result<usize, String> {
    if let Some(n) = word.strip_prefix('#').and_then(|n| n.parse::<usize>().ok()) {
        if n >= 1 && n <= cache.fields.len() {
            return Ok(n - 1);
        }
    }
    let same = |a: &str| a.eq_ignore_ascii_case(word.trim());
    (0..cache.fields.len())
        .find(|f| same(&table.caption(cache, *f)))
        .or_else(|| (0..cache.fields.len()).find(|f| same(&cache.fields[*f].name)))
        .ok_or_else(|| format!("there is no field `{word}`; `pivot fields` lists them"))
}

/// The field a word names, in the table `t` of the workbook.
pub fn field_in(book: &Book, t: usize, word: &str) -> Result<usize, String> {
    let table = &book.session.book.pivots.tables[t];
    field_of(table, &book.session.book.pivots.caches[table.cache], word)
}

/// The area words an agent writes.
fn area_of(word: &str) -> Result<Area, String> {
    Ok(match word {
        "rows" | "row" => Area::Rows,
        "columns" | "column" | "cols" => Area::Cols,
        "filters" | "filter" | "pages" => Area::Pages,
        "remove" | "hidden" | "none" => Area::Hidden,
        other => return Err(format!("an area is rows, columns, filters or remove; got `{other}`")),
    })
}

/// The summary function a word names.
pub fn func_of(word: &str) -> Result<Func, String> {
    Ok(match word.to_ascii_lowercase().replace('_', "-").as_str() {
        "sum" => Func::Sum,
        "count" => Func::Count,
        "average" | "avg" => Func::Average,
        "max" => Func::Max,
        "min" => Func::Min,
        "product" => Func::Product,
        "count-numbers" | "countnums" | "count-nums" => Func::CountNums,
        "stddev" => Func::StdDev,
        "stddevp" => Func::StdDevP,
        "var" => Func::Var,
        "varp" => Func::VarP,
        other => return Err(format!("a summary is sum, count, average, max, min, product, count-numbers, stddev, stddevp, var or varp; got `{other}`")),
    })
}

/// The words of every Show Values As choice, with the setting each names.
pub const SHOW_AS: [(&str, ShowAs); 15] = [
    ("normal", ShowAs::Normal),
    ("difference", ShowAs::Difference),
    ("percent", ShowAs::Percent),
    ("percent-difference", ShowAs::PercentDiff),
    ("running-total", ShowAs::RunTotal),
    ("percent-of-row", ShowAs::PercentOfRow),
    ("percent-of-column", ShowAs::PercentOfCol),
    ("percent-of-total", ShowAs::PercentOfTotal),
    ("index", ShowAs::Index),
    ("percent-of-parent-row", ShowAs::PercentOfParentRow),
    ("percent-of-parent-column", ShowAs::PercentOfParentCol),
    ("percent-of-parent", ShowAs::PercentOfParent),
    ("percent-running-total", ShowAs::PercentRunTotal),
    ("rank-ascending", ShowAs::RankAscending),
    ("rank-descending", ShowAs::RankDescending),
];

fn show_as_of(word: &str) -> Result<ShowAs, String> {
    SHOW_AS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, s)| *s)
        .ok_or_else(|| format!("Show Values As takes {}; got `{word}`", SHOW_AS.map(|(n, _)| n).join(", ")))
}

fn show_as_name(show: ShowAs) -> &'static str {
    SHOW_AS.iter().find(|(_, s)| *s == show).map_or("normal", |(n, _)| n)
}

// ------------------------------------------------------------------------------------------------ create

/// `pivot create [<range or table>] [at <cell>] [name <name>]`: Insert PivotTable. The source is the
/// selection, or the block of data round the active cell; the table goes on a new sheet unless `at` says.
fn create(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let source = source_of(book, args)?;
    let name = after(args, "name").map(str::to_owned);
    let (dest_sheet, dest) = match after(args, "at") {
        Some(word) => parse_range(book, word)
            .map(|(s, r)| (s, r.first))
            .ok_or_else(|| format!("`{word}` is not a cell"))?,
        None => (book.session.add_sheet(None, None)?, CellRef::new(2, 0)),
    };
    let t = book.session.pivot_create(source, dest_sheet, dest, name.as_deref())?;
    book.view.sheet = dest_sheet;
    book.view.select_cell(dest);
    book.view.lane_b.pivot_pane = true;
    book.changed();
    book.view.axes = None;
    let table = &book.session.book.pivots.tables[t];
    Ok(said(format!("Made {} on {}", table.name, book.session.book.sheets[dest_sheet].name))
        .with(table_json(book, t)))
}

/// The source of a new pivot table: a table or defined name, a range, or the data round the active cell.
fn source_of(book: &Book, args: &[String]) -> Result<PivotSource, String> {
    let first = args.first().filter(|w| !matches!(w.as_str(), "at" | "name"));
    if let Some(word) = first {
        if book.session.book.table(word).is_some() {
            return Ok(PivotSource::Name(word.clone()));
        }
        let (sheet, range) = parse_range(book, word)
            .ok_or_else(|| format!("`{word}` is not a range or a table name"))?;
        return Ok(PivotSource::Range { sheet, range });
    }
    let index = book.view.sheet;
    let chosen = book.view.range();
    let range = if chosen.len() == 1 {
        current_region(&book.session.book.sheets[index], chosen.first)
    } else {
        chosen
    };
    if book.session.book.sheets[index].value_at(range.first).is_empty() && range.len() == 1 {
        return Err("Choose the cells with the data, or a cell in it, before making a PivotTable.".to_owned());
    }
    Ok(PivotSource::Range { sheet: index, range })
}

// ------------------------------------------------------------------------------------- on one table

/// The verbs that act on one pivot table.
fn on_table(sheet: &mut Spreadsheet, what: &str, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (t, rest) = table_of(book, args)?;
    let answer = match what {
        "fields" | "read" | "show" => return Ok(said("PivotTable").with(table_json(book, t))),
        "add" => add_field(book, t, &rest)?,
        "move" => move_field(book, t, &rest)?,
        "remove" => remove_field(book, t, &rest)?,
        "value" => value_settings(book, t, &rest)?,
        "field" => field_settings(book, t, &rest)?,
        "move-values" => move_values(book, t, &rest)?,
        "layout" => layout(book, t, &rest)?,
        "totals" => totals(book, t, &rest)?,
        "option" => option(book, t, &rest)?,
        "style" => style(book, t, &rest)?,
        "refresh" => {
            book.session.pivot_refresh(t)?;
            "Refreshed".to_owned()
        }
        "source" => change_source(book, t, &rest)?,
        "move-table" | "relocate" => move_table(book, t, &rest)?,
        "delete" => {
            book.session.pivot_delete(t)?;
            "Deleted the PivotTable".to_owned()
        }
        other => {
            return Err(format!(
                "pivot takes create, fields, add, move, remove, value, field, layout, totals, option, style, refresh, source, move-table, delete, sort, items, page, filter, group, ungroup, calculated-field or calculated-item; got `{other}`"
            ))
        }
    };
    book.changed();
    book.view.axes = None;
    let live = t.min(book.session.book.pivots.tables.len().saturating_sub(1));
    let value = if book.session.book.pivots.tables.is_empty() { Json::Null } else { table_json(book, live) };
    Ok(said(answer).with(value))
}

/// `pivot add <field> rows|columns|filters|values [at <n>] [<summary>] [name <text>]`.
fn add_field(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("add takes a field and an area")?;
    let f = field_in(book, t, word)?;
    let area_word = args.get(1).map(String::as_str).unwrap_or("rows");
    let position = after(args, "at").and_then(|n| n.parse::<usize>().ok()).map(|n| n.saturating_sub(1));
    if matches!(area_word, "values" | "value" | "data") {
        let func = args.iter().skip(2).find_map(|w| func_of(w).ok()).unwrap_or(Func::Sum);
        let name = after(args, "name");
        book.session.pivot_add_value(t, f, func, name)?;
        return Ok(format!("Added {word} to the values"));
    }
    book.session.pivot_move_field(t, f, area_of(area_word)?, position)?;
    Ok(format!("Moved {word} to {area_word}"))
}

/// `pivot move <field> rows|columns|filters|remove [at <n>]`.
fn move_field(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("move takes a field and an area")?;
    let f = field_in(book, t, word)?;
    let area_word = args.get(1).ok_or("move takes an area: rows, columns, filters or remove")?;
    let position = after(args, "at").and_then(|n| n.parse::<usize>().ok()).map(|n| n.saturating_sub(1));
    book.session.pivot_move_field(t, f, area_of(area_word)?, position)?;
    Ok(format!("Moved {word} to {area_word}"))
}

/// `pivot remove <field>`: out of the rows, columns and filters, and out of the values.
fn remove_field(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("remove takes a field")?;
    let f = field_in(book, t, word)?;
    let data: Vec<usize> = book.session.book.pivots.tables[t]
        .data
        .iter()
        .enumerate()
        .filter(|(_, d)| d.field == f)
        .map(|(i, _)| i)
        .collect();
    for d in data.into_iter().rev() {
        book.session.pivot_remove_value(t, d)?;
    }
    book.session.pivot_move_field(t, f, Area::Hidden, None)?;
    Ok(format!("Removed {word}"))
}

/// The value field a word names: its name, or its place from 1.
fn data_of(table: &PivotTable, word: &str) -> Result<usize, String> {
    if let Ok(n) = word.parse::<usize>() {
        if n >= 1 && n <= table.data.len() {
            return Ok(n - 1);
        }
    }
    table
        .data
        .iter()
        .position(|d| d.name.eq_ignore_ascii_case(word))
        .ok_or_else(|| format!("there is no value field `{word}`"))
}

/// `pivot value <name or n> [name X] [summarize F] [show S] [base-field F] [base-item I] [numfmt X|none]`:
/// Value Field Settings.
fn value_settings(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("value takes the value field to change")?;
    let d = data_of(&book.session.book.pivots.tables[t], word)?;
    let mut s = ValueSettings::default();
    s.name = after(args, "name").map(str::to_owned);
    if let Some(f) = after(args, "summarize").or_else(|| after(args, "summary")) {
        s.func = Some(func_of(f)?);
    }
    if let Some(w) = after(args, "show") {
        s.show = Some(show_as_of(w)?);
    }
    if let Some(w) = after(args, "base-field") {
        s.base_field = Some(field_in(book, t, w)?);
    }
    if let Some(w) = after(args, "base-item") {
        s.base_item = Some(base_item(book, t, s.base_field, d, w)?);
    }
    if let Some(w) = after(args, "numfmt") {
        s.num_fmt = Some((w != "none").then(|| w.to_owned()));
    }
    book.session.pivot_value_settings(t, d, s)?;
    Ok(format!("Changed value field {word}"))
}

/// The base item of Show Values As: `previous`, `next`, or an item of the base field by name.
fn base_item(book: &Book, t: usize, base_field: Option<usize>, d: usize, word: &str) -> Result<u32, String> {
    match word {
        "previous" => return Ok(unluminous_sheet::pivot::BASE_PREVIOUS),
        "next" => return Ok(unluminous_sheet::pivot::BASE_NEXT),
        _ => {}
    }
    let table = &book.session.book.pivots.tables[t];
    let field = base_field.unwrap_or(table.data[d].base_field);
    super::verbs_b_pivot_more::item_position(book, t, field, word).map(|p| p as u32)
}

/// `pivot field <field> [name X|none] [form F] [subtotals none|automatic|sum,count] [subtotal-top on|off]
/// [repeat-labels on|off] [blank-row on|off] [show-all on|off] [numfmt X|none] [multi-select on|off]`.
fn field_settings(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("field takes the field to change")?;
    let f = field_in(book, t, word)?;
    let mut s = FieldSettings::default();
    if let Some(n) = after(args, "name") {
        s.name = Some((n != "none").then(|| n.to_owned()));
    }
    if let Some(w) = after(args, "form") {
        s.form = Some(form_of(w)?);
    }
    if let Some(w) = after(args, "subtotals") {
        s.subtotals = Some(subtotals_of(w)?);
    }
    s.subtotal_top = switch_after(args, "subtotal-top")?;
    s.repeat_labels = switch_after(args, "repeat-labels")?;
    s.blank_row = switch_after(args, "blank-row")?;
    s.show_all = switch_after(args, "show-all")?;
    s.multi_page = switch_after(args, "multi-select")?;
    if let Some(w) = after(args, "numfmt") {
        s.num_fmt = Some((w != "none").then(|| w.to_owned()));
    }
    book.session.pivot_field_settings(t, f, s)?;
    Ok(format!("Changed field {word}"))
}

fn switch_after(args: &[String], key: &str) -> Result<Option<bool>, String> {
    match after(args, key) {
        Some(w) => on_off(Some(w)).map(Some),
        None => Ok(None),
    }
}

fn form_of(word: &str) -> Result<Form, String> {
    Ok(match word {
        "compact" => Form::Compact,
        "outline" => Form::Outline,
        "tabular" => Form::Tabular,
        other => return Err(format!("a layout is compact, outline or tabular; got `{other}`")),
    })
}

/// `none`, `automatic`, or a comma list of functions.
fn subtotals_of(word: &str) -> Result<Subtotals, String> {
    match word {
        "none" => Ok(Subtotals::none()),
        "automatic" | "auto" => Ok(Subtotals::default()),
        list => {
            let funcs: Result<Vec<Func>, String> = list.split(',').map(func_of).collect();
            Ok(Subtotals { automatic: false, funcs: funcs? })
        }
    }
}

/// `pivot move-values rows|columns [at <n>]`: where the Values entry sits.
fn move_values(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let area = area_of(args.first().map(String::as_str).unwrap_or("columns"))?;
    let position = after(args, "at").and_then(|n| n.parse::<usize>().ok()).map(|n| n.saturating_sub(1));
    book.session.pivot_move_values(t, area, position)?;
    Ok("Moved the values".to_owned())
}

/// `pivot layout compact|outline|tabular`: Report Layout.
fn layout(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let form = form_of(args.first().map(String::as_str).unwrap_or("compact"))?;
    book.session.pivot_set_form(t, form)?;
    Ok(format!("Layout {}", args[0]))
}

/// `pivot totals rows|columns|both|none`: `rows` keeps the grand total row at the bottom, `columns` the
/// grand total column at the right.
fn totals(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let (row, col) = match args.first().map(String::as_str) {
        Some("both") | Some("on") => (true, true),
        Some("none") | Some("off") => (false, false),
        Some("rows") | Some("row") => (true, false),
        Some("columns") | Some("column") => (false, true),
        _ => return Err("totals take both, rows, columns or none".to_owned()),
    };
    book.session.pivot_set_grand_totals(t, row, col)?;
    Ok("Grand totals set".to_owned())
}

/// `pivot option <name> <value>`: captions and the field headers.
fn option(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let key = args.first().ok_or("option takes a name and a value")?.to_ascii_lowercase();
    let value = args.get(1..).unwrap_or(&[]).join(" ");
    let text = (!value.is_empty() && value != "none").then(|| value.clone());
    let headers = if key == "headers" { Some(on_off(args.get(1).map(String::as_str))?) } else { None };
    let known = matches!(
        key.as_str(),
        "headers" | "grand-total-caption" | "error-caption" | "empty-caption" | "row-header-caption" | "column-header-caption" | "data-caption"
    );
    if !known {
        return Err("option takes headers, grand-total-caption, error-caption, empty-caption, row-header-caption, column-header-caption or data-caption".to_owned());
    }
    book.session.pivot_set_options(t, |o| match key.as_str() {
        "headers" => o.show_headers = headers.unwrap_or(true),
        "grand-total-caption" => o.grand_total_caption = text,
        "error-caption" => o.error_caption = text,
        "empty-caption" => o.missing_caption = text,
        "row-header-caption" => o.row_header_caption = text,
        "column-header-caption" => o.col_header_caption = text,
        _ => o.data_caption = text.unwrap_or_else(|| "Values".to_owned()),
    })?;
    Ok(format!("Option {key} set"))
}

/// `pivot style <name> [banded-rows on|off] [banded-columns on|off] [row-headers on|off] [column-headers on|off]`.
fn style(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let mut style = book.session.book.pivots.tables[t].style.clone();
    if let Some(name) = args.first().filter(|w| !w.contains('-') || w.starts_with("Pivot")) {
        style.name = (name != "none").then(|| name.clone());
    }
    if let Some(w) = after(args, "banded-rows") {
        style.show_row_stripes = on_off(Some(w))?;
    }
    if let Some(w) = after(args, "banded-columns") {
        style.show_col_stripes = on_off(Some(w))?;
    }
    if let Some(w) = after(args, "row-headers") {
        style.show_row_headers = on_off(Some(w))?;
    }
    if let Some(w) = after(args, "column-headers") {
        style.show_col_headers = on_off(Some(w))?;
    }
    book.session.pivot_set_style(t, style)?;
    Ok("PivotTable style set".to_owned())
}

/// `pivot source <range or table name>`: Change Data Source.
fn change_source(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let source = source_of(book, args)?;
    book.session.pivot_change_source(t, source)?;
    Ok("Changed the data source".to_owned())
}

/// `pivot move-table <cell>`: Move PivotTable.
fn move_table(book: &mut Book, t: usize, args: &[String]) -> Result<String, String> {
    let word = args.first().ok_or("move-table takes the cell to move to")?;
    let (sheet, range) = parse_range(book, word).ok_or_else(|| format!("`{word}` is not a cell"))?;
    book.session.pivot_move(t, sheet, range.first)?;
    Ok(format!("Moved the PivotTable to {}", range.first.a1()))
}

/// `pivot refresh-all`.
fn refresh_all(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    book.session.pivot_refresh_all()?;
    book.changed();
    book.view.axes = None;
    Ok(said("Refreshed every PivotTable"))
}

// ------------------------------------------------------------------------------------------------ read

/// `pivots`: every pivot table of the workbook.
fn pivots(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let all: Vec<Json> = (0..book.session.book.pivots.tables.len()).map(|t| table_json(book, t)).collect();
    Ok(said(format!("{} PivotTables", all.len())).with(json!(all)))
}

/// The area a field is in, and its place there.
fn area_of_field(table: &PivotTable, f: usize) -> (&'static str, Option<usize>) {
    let place = |axis: &[AxisEntry]| axis.iter().position(|e| *e == AxisEntry::Field(f));
    if let Some(p) = place(&table.rows) {
        return ("rows", Some(p + 1));
    }
    if let Some(p) = place(&table.cols) {
        return ("columns", Some(p + 1));
    }
    if let Some(p) = table.pages.iter().position(|pg| pg.field == f) {
        return ("filters", Some(p + 1));
    }
    if table.data.iter().any(|d| d.field == f) {
        return ("values", None);
    }
    ("none", None)
}

/// One pivot table as data: its place, source, fields with their areas, value fields and layout.
pub fn table_json(book: &Book, t: usize) -> Json {
    let pivots = &book.session.book.pivots;
    let table = &pivots.tables[t];
    let cache = &pivots.caches[table.cache];
    let fields: Vec<Json> = (0..cache.fields.len())
        .map(|f| {
            let (area, place) = area_of_field(table, f);
            json!({ "name": table.caption(cache, f), "area": area, "position": place })
        })
        .collect();
    let named = |axis: &[AxisEntry]| -> Vec<String> {
        axis.iter()
            .map(|e| match e {
                AxisEntry::Field(f) => table.caption(cache, *f),
                AxisEntry::Values => "Values".to_owned(),
            })
            .collect()
    };
    let values: Vec<Json> = table
        .data
        .iter()
        .map(|d| json!({
            "name": d.name,
            "field": table.caption(cache, d.field),
            "summarize": d.func.xml(),
            "showAs": show_as_name(d.show),
            "numFmt": d.num_fmt,
        }))
        .collect();
    json!({
        "name": table.name,
        "sheet": book.session.book.sheets[table.sheet].name,
        "location": table.location.a1(),
        "source": match &cache.source {
            unluminous_sheet::pivot::CacheSource::Range { sheet, range } => format!("{sheet}!{}", range.a1()),
            unluminous_sheet::pivot::CacheSource::Name(n) => n.clone(),
        },
        "records": cache.records.len(),
        "fields": fields,
        "rows": named(&table.rows),
        "columns": named(&table.cols),
        "filters": table.pages.iter().map(|p| table.caption(cache, p.field)).collect::<Vec<_>>(),
        "values": values,
        "layout": match table.options.form { Form::Compact => "compact", Form::Outline => "outline", Form::Tabular => "tabular" },
        "grandTotals": { "rows": table.options.grand_total_row, "columns": table.options.grand_total_col },
        "style": table.style.name,
    })
}

/// The pivot table whose range holds a cell, for the grid.
pub fn table_at(book: &Book, at: CellRef) -> Option<usize> {
    book.session.book.pivots.at(book.view.sheet, at)
}

/// The range of a pivot table, for drawing.
pub fn range_of(book: &Book, t: usize) -> Range {
    book.session.book.pivots.tables[t].full_range()
}
