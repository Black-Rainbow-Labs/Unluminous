//! The slicer and timeline verbs (`task-2360`, the `wiringb` lane). A slicer and a timeline are the engine's
//! (`unluminous_sheet::slicers`): they are saved in the workbook and filter the pivot table, so these verbs
//! only name what the engine's `slicer_*` and `timeline_*` methods take.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::slicers::sync::home_of;

use super::verbs_a::{after, Out};
use super::verbs_b_pivot::{field_in, table_of};
use super::Book;
use super::Spreadsheet;
use crate::services::plugin_ui::Answer;

fn said(text: impl Into<String>) -> Answer {
    Answer::said(text)
}

/// Run `slicer` or `timeline`.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Out {
    if verb == "slicer" {
        slicer(sheet, args)
    } else {
        timeline(sheet, args)
    }
}

/// Whether a name typed matches a stored one; a menu writes spaces as underscores.
pub fn same_name(stored: &str, typed: &str) -> bool {
    stored.eq_ignore_ascii_case(typed) || stored.replace(' ', "_").eq_ignore_ascii_case(typed)
}

/// The name a slicer or timeline is stored under, from what was typed.
fn stored_name(names: impl Iterator<Item = String>, typed: &str, what: &str) -> Result<String, String> {
    names
        .into_iter()
        .find(|n| same_name(n, typed))
        .ok_or_else(|| format!("There is no {what} named {typed}."))
}

/// The sheet and the cell a slicer's or timeline's frame starts at.
pub fn frame_at(book: &Book, local: &str, name: &str) -> Option<(usize, CellRef)> {
    let sheet = home_of(&book.session.book, local, name)?;
    let object = book.session.book.sheets[sheet]
        .drawing
        .objects
        .iter()
        .find(|o| o.tree.find(local).is_some_and(|e| e.attr("name") == Some(name)))?;
    Some((sheet, object.anchor.from.0))
}

fn cell_word(book: &Book, words: &[String], default: CellRef) -> Result<CellRef, String> {
    match after(words, "at") {
        Some(w) => super::commands::parse_range(book, w)
            .map(|(_, r)| r.first)
            .ok_or_else(|| format!("`{w}` is not a cell")),
        None => Ok(default),
    }
}

/// The cell right of a pivot table, two columns over, where a new slicer or timeline goes.
fn beside(book: &Book, t: usize) -> CellRef {
    let table = &book.session.book.pivots.tables[t];
    CellRef::new(table.location.first.row, table.location.last.col + 2)
}

/// The name of the source field a word names in a pivot table.
fn source_field(book: &Book, t: usize, word: &str) -> Result<String, String> {
    let f = field_in(book, t, word)?;
    let table = &book.session.book.pivots.tables[t];
    Ok(book.session.book.pivots.caches[table.cache].fields[f].name.clone())
}

// ------------------------------------------------------------------------------------------------ slicers

/// `slicer add <field> [in <pivot>] [at <cell>] [name <n>]`, `slicer list`, `slicer select <name> <item>,<item>|all`,
/// `slicer delete <name>`.
fn slicer(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let how = args.first().map(String::as_str).unwrap_or("list");
    let rest = args.get(1..).unwrap_or(&[]);
    let message = match how {
        "add" => {
            let (t, words) = table_of(book, rest)?;
            let field = source_field(book, t, words.first().ok_or("add takes the field to slice by")?)?;
            let at = cell_word(book, &words, beside(book, t))?;
            let pivot = book.session.book.pivots.tables[t].name.clone();
            let index = book.view.sheet;
            let made = book.session.slicer_add(index, &pivot, &field, after(&words, "name"), at)?;
            format!("Added the slicer {made}")
        }
        "list" => {
            let list = slicers_json(book);
            return Ok(said(format!("{} slicers", list.len())).with(json!(list)));
        }
        "select" => {
            let typed = rest.first().ok_or("select takes the slicer's name")?;
            let name = stored_name(book.session.book.slicers.slicers.iter().map(|s| s.name.clone()), typed, "slicer")?;
            let chosen = rest.get(1).map(String::as_str).unwrap_or("all");
            let items: Option<Vec<String>> = (chosen != "all" && chosen != "clear")
                .then(|| chosen.split(',').map(|s| s.trim().to_owned()).collect());
            book.session.slicer_select(&name, items.as_deref())?;
            format!("{name}: {chosen}")
        }
        "delete" => {
            let typed = rest.first().ok_or("delete takes the slicer's name")?;
            let name = stored_name(book.session.book.slicers.slicers.iter().map(|s| s.name.clone()), typed, "slicer")?;
            book.session.slicer_delete(&name)?;
            format!("Deleted {name}")
        }
        other => return Err(format!("slicer takes add, list, select or delete; got `{other}`")),
    };
    book.changed();
    book.view.axes = None;
    let list = slicers_json(book);
    Ok(said(message).with(json!(list)))
}

/// Every slicer as data: its caption, the table and field it filters, where it sits and its items.
pub fn slicers_json(book: &Book) -> Vec<Json> {
    book.session
        .book
        .slicers
        .slicers
        .iter()
        .map(|s| {
            let cache = book.session.book.slicers.cache(&s.cache);
            let items: Vec<Json> = book
                .session
                .slicer_items(&s.name)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, selected)| json!({ "name": name, "selected": selected }))
                .collect();
            let place = frame_at(book, "slicer", &s.name);
            json!({
                "name": s.name,
                "caption": s.caption,
                "cache": s.cache,
                "pivot": cache.map(|c| c.pivots.clone()),
                "field": cache.map(|c| c.source.clone()),
                "sheet": place.map(|(i, _)| book.session.book.sheets[i].name.clone()),
                "at": place.map(|(_, at)| at.a1()),
                "items": items,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------- timelines

/// A date given as `2026-01-31`, `1/31/2026` or a serial number.
fn date_of(word: &str) -> Result<f64, String> {
    if let Ok(n) = word.parse::<f64>() {
        return Ok(n);
    }
    unluminous_sheet::input::date_time_from_text(word, false)
        .ok_or_else(|| format!("`{word}` is not a date"))
}

/// `timeline add <date field> [in <pivot>] [at <cell>] [name <n>]`, `timeline list`,
/// `timeline select <name> <from> <to>|all`, `timeline delete <name>`.
fn timeline(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let how = args.first().map(String::as_str).unwrap_or("list");
    let rest = args.get(1..).unwrap_or(&[]);
    let names = |book: &Book| -> Vec<String> {
        book.session.book.slicers.timelines.iter().map(|t| t.name.clone()).collect()
    };
    let message = match how {
        "add" => {
            let (t, words) = table_of(book, rest)?;
            let field = source_field(book, t, words.first().ok_or("add takes the date field")?)?;
            let at = cell_word(book, &words, beside(book, t))?;
            let pivot = book.session.book.pivots.tables[t].name.clone();
            let index = book.view.sheet;
            let made = book.session.timeline_add(index, &pivot, &field, after(&words, "name"), at)?;
            format!("Added the timeline {made}")
        }
        "list" => {
            let list = timelines_json(book);
            return Ok(said(format!("{} timelines", list.len())).with(json!(list)));
        }
        "select" => {
            let name = stored_name(names(book).into_iter(), rest.first().ok_or("select takes the timeline's name")?, "timeline")?;
            let span = match rest.get(1).map(String::as_str) {
                None | Some("all") => None,
                Some(from) => Some((date_of(from)?, date_of(rest.get(2).ok_or("select takes a first and a last date")?)?)),
            };
            book.session.timeline_select(&name, span)?;
            format!("{name} set")
        }
        "delete" => {
            let name = stored_name(names(book).into_iter(), rest.first().ok_or("delete takes the timeline's name")?, "timeline")?;
            book.session.timeline_delete(&name)?;
            format!("Deleted {name}")
        }
        other => return Err(format!("timeline takes add, list, select or delete; got `{other}`")),
    };
    book.changed();
    book.view.axes = None;
    let list = timelines_json(book);
    Ok(said(message).with(json!(list)))
}

/// Every timeline as data: its field, the dates it keeps and where it sits.
pub fn timelines_json(book: &Book) -> Vec<Json> {
    book.session
        .book
        .slicers
        .timelines
        .iter()
        .map(|t| {
            let cache = book.session.book.slicers.timeline_caches.iter().find(|c| c.name == t.cache);
            let place = frame_at(book, "timeslicer", &t.name);
            json!({
                "name": t.name,
                "caption": t.caption,
                "pivot": cache.map(|c| c.pivots.clone()),
                "field": cache.map(|c| c.source.clone()),
                "from": cache.and_then(|c| c.selection.as_ref().map(|s| s.0.clone())),
                "to": cache.and_then(|c| c.selection.as_ref().map(|s| s.1.clone())),
                "bounds": cache.and_then(|c| c.bounds.clone()),
                "sheet": place.map(|(i, _)| book.session.book.sheets[i].name.clone()),
                "at": place.map(|(_, at)| at.a1()),
            })
        })
        .collect()
}
