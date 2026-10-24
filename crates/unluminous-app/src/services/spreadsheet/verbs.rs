//! The verbs round 2 added (`task-2360`): the view of a sheet, calculation, Go To, and the boxes that ask
//! which way cells move. [`super::commands::run`] hands any verb it does not know to [`run`] here, so the
//! one dispatcher an agent's words and a menu entry's reach stays one dispatcher.

use serde_json::json;
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::edit::Shift;
use unluminous_sheet::model::{Split, ViewKind};

use super::actions::Act;
use super::commands::{act, parse_range};
use super::view::{Field, Prompt};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a verb this module knows, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Result<Answer, String>> {
    Some(match verb {
        "show-formulas" => view_switch(sheet, args, "Show Formulas", |v| &mut v.show_formulas),
        "headings" => view_switch(sheet, args, "Headings", |v| &mut v.show_headings),
        "zeros" => view_switch(sheet, args, "Zero values", |v| &mut v.show_zeros),
        "right-to-left" => view_switch(sheet, args, "Right to left", |v| &mut v.right_to_left),
        "view" => view_kind(sheet, args),
        "split" if is_window_split(args) => split(sheet, args),
        "goto" => goto(sheet, args),
        "calculate" => calculate(sheet, args),
        "calc-mode" => calc_mode(sheet, args),
        "status-bar" => status_bar(sheet, args),
        "enter-direction" => setting(sheet, "enter", args),
        "autocomplete" if args.first().map(String::as_str) == Some("list") => pick_from_list(sheet),
        "autocomplete" => setting(sheet, "autocomplete", args),
        "autocorrect" => setting(sheet, "autocorrect", args),
        "custom-list" => custom_list(sheet, args),
        _ => return None,
    })
}

/// The `split` verb already splits text into columns; `split <cell>` and `split off` with no delimiter
/// meaning split the window. A cell reference or `off` or `window` names the window.
fn is_window_split(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("off") | Some("window") | Some("none") => true,
        Some(word) => CellRef::parse(word).is_some() && args.len() == 1,
        None => false,
    }
}

/// A switch in the sheet's view: `on`, `off`, or `toggle` (the default).
fn view_switch(
    sheet: &mut Spreadsheet,
    args: &[String],
    name: &str,
    field: fn(&mut unluminous_sheet::model::SheetView) -> &mut bool,
) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    let now = *field(&mut book.session.book.sheets[index].view);
    let on = switch_word(args.first().map(String::as_str), now)?;
    book.session.set_view(index, |v| *field(v) = on)?;
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("{name} {}", if on { "on" } else { "off" })))
}

/// What `on`, `off` and `toggle` turn a switch to.
pub fn switch_word(word: Option<&str>, now: bool) -> Result<bool, String> {
    match word {
        Some("on") | Some("show") | Some("yes") => Ok(true),
        Some("off") | Some("hide") | Some("no") => Ok(false),
        Some("toggle") | None => Ok(!now),
        Some(other) => Err(format!("that takes on, off or toggle, not `{other}`")),
    }
}

/// `view normal|layout|page-break`.
fn view_kind(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let word = args.first().map(String::as_str).unwrap_or("normal");
    let kind = ViewKind::parse(word).ok_or("view takes normal, layout or page-break")?;
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    book.session.set_view(index, |v| v.kind = kind)?;
    book.changed();
    Ok(Answer::said(format!("{} view", kind.name())))
}

/// `split <cell>` splits the window above and left of a cell, as View > Split does; `split off` removes it.
fn split(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    if matches!(args.first().map(String::as_str), Some("off") | Some("none")) {
        book.session.split(index, None)?;
        book.changed();
        return Ok(Answer::said("Split removed"));
    }
    let at = args.first().and_then(|w| CellRef::parse(w)).unwrap_or(book.view.cursor);
    let split = split_at(book, at);
    book.session.split(index, Some(split))?;
    book.changed();
    Ok(Answer::said(format!("Split at {}", at.a1())))
}

/// A split above and left of a cell, in twentieths of a point from the window's top left with the headers.
fn split_at(book: &Book, at: CellRef) -> Split {
    let sheet = &book.session.book.sheets[book.view.sheet];
    let columns = super::axis::Axis::columns(sheet);
    let rows = super::axis::Axis::rows(sheet);
    let header = if sheet.view.show_headings { (46.0, 22.0) } else { (0.0, 0.0) };
    let x = if at.col > 0 {
        (columns.start(at.col) - book.view.scroll.x + header.0) * 20.0
    } else {
        0.0
    };
    let y =
        if at.row > 0 { (rows.start(at.row) - book.view.scroll.y + header.1) * 20.0 } else { 0.0 };
    Split { x: x.max(0.0) as f64, y: y.max(0.0) as f64, top_left: Some(at) }
}

/// `goto` opens the Go To box; `goto <reference or name>` goes there, as F5 does.
fn goto(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    if args.is_empty() {
        let fields = vec![Field::bare("Reference", "")];
        book.view.prompt = Some(Prompt::form(vec!["goto".to_owned()], "Go To", fields));
        return Ok(Answer::said(String::new()));
    }
    let target = args.join(" ");
    let (index, range) = parse_range(book, &target)
        .ok_or_else(|| format!("`{target}` is not a reference or a name"))?;
    if index != book.view.sheet {
        act(sheet, Act::ShowSheet(index))?;
    }
    let book = sheet.current_book()?;
    book.view.select_range(range);
    Ok(Answer::said(format!("Went to {}", range.a1())))
}

/// `calculate sheet|all|full|rebuild`: Shift+F9, F9, Ctrl+Alt+F9 and Ctrl+Alt+Shift+F9.
fn calculate(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    match args.first().map(String::as_str).unwrap_or("all") {
        "sheet" | "all" | "full" | "rebuild" => act(sheet, Act::Recalculate),
        other => Err(format!("calculate takes sheet, all, full or rebuild, not `{other}`")),
    }
}

/// `calc-mode automatic|manual`, the calculation setting saved with the workbook.
fn calc_mode(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let calc = &mut book.session.book.calc;
    match args.first().map(String::as_str) {
        Some("automatic") | Some("auto") => calc.automatic = true,
        Some("manual") => calc.automatic = false,
        None => {
            let mode = if calc.automatic { "automatic" } else { "manual" };
            return Ok(Answer::said(mode).with(json!({ "mode": mode })));
        }
        Some(other) => return Err(format!("calc-mode takes automatic or manual, not `{other}`")),
    }
    book.session.modified = true;
    book.changed();
    Ok(Answer::said("Calculation set"))
}

/// The box Ctrl+Plus and Ctrl+Minus open on a range that is not whole rows or columns, asking which way the
/// cells beside move, as Excel's Insert and Delete dialogs ask.
pub fn ask_which_way(book: &mut Book, insert: bool) -> Result<Answer, String> {
    let (verb, title, options): (&str, &str, &[(&str, &str)]) = if insert {
        (
            "insert",
            "Insert",
            &[
                ("Shift cells right", "right"),
                ("Shift cells down", "down"),
                ("Entire row", "rows"),
                ("Entire column", "columns"),
            ],
        )
    } else {
        (
            "delete",
            "Delete",
            &[
                ("Shift cells left", "left"),
                ("Shift cells up", "up"),
                ("Entire row", "rows"),
                ("Entire column", "columns"),
            ],
        )
    };
    let default = if insert { "down" } else { "up" };
    let fields = vec![Field::choice(title, "shift", options, default)];
    book.view.prompt = Some(Prompt::form(vec![verb.to_owned()], title, fields));
    Ok(Answer::said(String::new()))
}

/// Which way an insert or a delete moves the cells beside, from a word.
pub fn shift_word(verb: &str, word: Option<&str>) -> Option<Shift> {
    Some(match (verb, word) {
        (_, Some("rows")) | (_, Some("row")) => Shift::WholeRows,
        (_, Some("columns")) | (_, Some("column")) => Shift::WholeColumns,
        ("insert", Some("down")) => Shift::Down,
        ("insert", Some("right")) => Shift::Right,
        ("delete", Some("up")) => Shift::Up,
        ("delete", Some("left")) => Shift::Left,
        _ => return None,
    })
}

/// `status-bar average,count,sum` sets what the status area adds up; `status-bar toggle min` turns one on
/// or off; no words says what it shows.
fn status_bar(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let mut chosen = sheet.config.aggregates.clone();
    match args.first().map(String::as_str) {
        None => return Ok(Answer::said(chosen.join(", ")).with(json!(chosen))),
        Some("toggle") => {
            let name = args.get(1).ok_or("status-bar toggle needs a figure")?.to_lowercase();
            if !super::config::AGGREGATES.contains(&name.as_str()) {
                return Err(format!(
                    "`{name}` is not one of {}",
                    super::config::AGGREGATES.join(", ")
                ));
            }
            match chosen.iter().position(|a| *a == name) {
                Some(i) => {
                    chosen.remove(i);
                }
                None => chosen.push(name),
            }
            chosen.sort_by_key(|a| super::config::AGGREGATES.iter().position(|x| x == a));
        }
        Some(_) => {
            let mut config = sheet.config.clone();
            config.set("aggregates", &args.join(","))?;
            chosen = config.aggregates;
        }
    }
    sheet.config.aggregates = chosen.clone();
    sheet.write_the_settings()?;
    Ok(Answer::said(format!("The status bar shows {}", chosen.join(", "))))
}

/// A setting of the plugin's own, `enter-direction right` or `autocomplete off`, written to its file.
fn setting(sheet: &mut Spreadsheet, name: &str, args: &[String]) -> Result<Answer, String> {
    let value = args.first().ok_or_else(|| format!("{name} needs a value"))?;
    let mut config = sheet.config.clone();
    config.set(name, value)?;
    sheet.config = config;
    sheet.write_the_settings()?;
    Ok(Answer::said(format!("{name} is {value}")))
}

/// `custom-list add <a,b,c>`, `custom-list delete <n>`, or the lists with no words: the fill lists a
/// person adds to Excel's own.
fn custom_list(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let list = args[1..].join(" ");
            if list.split(',').filter(|w| !w.trim().is_empty()).count() < 2 {
                return Err(
                    "a custom list needs at least two entries, separated by commas".to_owned()
                );
            }
            sheet.config.custom_lists.push(list);
        }
        Some("delete") => {
            let n: usize = args
                .get(1)
                .and_then(|w| w.parse().ok())
                .ok_or("custom-list delete needs a number from 1")?;
            if n == 0 || n > sheet.config.custom_lists.len() {
                return Err(format!("there is no custom list {n}"));
            }
            sheet.config.custom_lists.remove(n - 1);
        }
        None => {
            let lists = sheet.config.custom_lists.clone();
            return Ok(Answer::said(format!("{} custom lists", lists.len())).with(json!(lists)));
        }
        Some(other) => return Err(format!("custom-list takes add or delete, not `{other}`")),
    }
    sheet.write_the_settings()?;
    Ok(Answer::said("Custom lists saved"))
}

/// `autocomplete list`: the entries of the column under the active cell, which Alt+Down opens as a list.
fn pick_from_list(sheet: &mut Spreadsheet) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let at = book.view.cursor;
    let entries = super::entry::column_entries(&book.session.book.sheets[book.view.sheet], at);
    book.view.choices = true;
    Ok(Answer::said(format!("{} entries", entries.len())).with(json!(entries)))
}
