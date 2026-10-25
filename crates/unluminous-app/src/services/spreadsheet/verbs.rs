//! The verbs round 2 added (`task-2360`): the view of a sheet, calculation, Go To, and the boxes that ask
//! which way cells move. [`super::commands::run`] hands any verb it does not know to [`run`] here, so the
//! one dispatcher an agent's words and a menu entry's reach stays one dispatcher.

use serde_json::json;
use unluminous_sheet::coord::{CellRef, Range};
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
        "insert-function" => insert_function(sheet, args),
        "arguments" => arguments(sheet, args),
        "names" => names(sheet, args),
        "move-cells" => move_cells(sheet, args),
        "autofit" => match args.first().map(String::as_str) {
            Some("rows") | Some("row") => {
                let book = match sheet.current_book() {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                super::commands::take_range(book, &args[1..]);
                act(sheet, Act::AutoFitRows)
            }
            Some("columns") | Some("column") => {
                let book = match sheet.current_book() {
                    Ok(b) => b,
                    Err(e) => return Some(Err(e)),
                };
                super::commands::take_range(book, &args[1..]);
                act(sheet, Act::AutoFitColumns)
            }
            _ => Err("autofit takes rows or columns".to_owned()),
        },
        "fill-handle" => fill_handle(sheet, args),
        _ => {
            return super::verbs_a::run(sheet, verb, args)
                .or_else(|| super::verbs_c::run(sheet, verb, args))
                .or_else(|| super::verbs_b::run(sheet, verb, args))
        }
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

/// `insert-function` opens Insert Function in the panel; `insert-function <NAME>` opens that function's
/// Function Arguments box, as choosing it in Excel's dialog does.
fn insert_function(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let Some(name) = args.first() else {
        book.view.panel =
            Some(super::view::PanelPath(vec!["Insert".to_owned(), "Function".to_owned()]));
        return Ok(Answer::said(String::new()));
    };
    let function = unluminous_sheet::functions::find(name)
        .ok_or_else(|| format!("there is no function `{name}`"))?;
    let fields: Vec<Field> =
        argument_names(function.args).iter().map(|a| Field::argument(a, "")).collect();
    let mut verb = vec!["arguments".to_owned(), function.name.to_owned()];
    if fields.is_empty() {
        verb.push(String::new());
    }
    let title = format!("Function Arguments: {}", function.name);
    book.view.prompt = Some(Prompt::form(verb, title, fields));
    Ok(Answer::said(function.about))
}

/// A function's argument names as its hint spells them, without the brackets of an optional one.
pub fn argument_names(args: &str) -> Vec<String> {
    args.split(", ")
        .map(|a| a.trim().trim_start_matches('[').trim_end_matches(']').to_owned())
        .filter(|a| !a.is_empty() && a != "...")
        .collect()
}

/// `arguments <NAME> <a> <b> ...`: what OK in Function Arguments does, a formula with those arguments in
/// the active cell. Empty arguments at the end are left out; one in the middle keeps its comma.
fn arguments(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let name = args.first().ok_or("arguments needs a function")?;
    let mut values: Vec<&str> = args[1..].iter().map(|a| a.trim()).collect();
    while values.last().is_some_and(|v| v.is_empty()) {
        values.pop();
    }
    let formula = format!("={}({})", name.to_ascii_uppercase(), values.join(","));
    let book = sheet.current_book()?;
    let at = book.view.cursor;
    act(sheet, Act::Type { at, text: formula.clone() })?;
    Ok(Answer::said(formula))
}

/// `names` lists the defined names; `names new <name> <refers-to> [scope]`, `names edit <name> [scope]
/// <refers-to>`, `names delete <name> [scope]` change them; `names paste` types one into the active cell,
/// as F3 does; `names from-selection [top] [left] [bottom] [right]` names cells from their labels.
fn names(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    match args.first().map(String::as_str) {
        None => Ok(names_list(book)),
        Some("new") => {
            let name = args.get(1).ok_or("names new needs a name")?;
            let to = args.get(2).ok_or("names new needs what the name refers to")?;
            let scope = scope_of(book, args.get(3).map(String::as_str))?;
            book.session.set_name(name, to.trim_start_matches('='), scope)?;
            book.changed();
            Ok(Answer::said(format!("{name} is {to}")))
        }
        Some("edit") => {
            let name = args.get(1).ok_or("names edit needs a name")?;
            let (scope, to) = match args.len() {
                3 => (None, &args[2]),
                n if n >= 4 => (scope_of(book, Some(args[2].as_str()))?, &args[3]),
                _ => return Err("names edit needs a name and what it refers to".to_owned()),
            };
            book.session.set_name(name, to.trim_start_matches('='), scope)?;
            book.changed();
            Ok(Answer::said(format!("{name} is {to}")))
        }
        Some("delete") => {
            let name = args.get(1).ok_or("names delete needs a name")?;
            let scope = scope_of(book, args.get(2).map(String::as_str))?;
            book.session.delete_name(name, scope)?;
            book.changed();
            Ok(Answer::said(format!("{name} deleted")))
        }
        Some("paste") => paste_name(book, args.get(1)),
        Some("from-selection") => {
            let words: Vec<&str> = args[1..].iter().map(String::as_str).collect();
            let range = book.view.range();
            let index = book.view.sheet;
            let sides = [
                words.contains(&"top"),
                words.contains(&"left"),
                words.contains(&"bottom"),
                words.contains(&"right"),
            ];
            let made = super::names::from_selection(&mut book.session, index, range, sides)?;
            book.changed();
            Ok(Answer::said(format!("Made {} names", made.len())).with(json!(made)))
        }
        Some(other) => {
            Err(format!("names takes new, edit, delete, paste or from-selection, not `{other}`"))
        }
    }
}

/// Every name a person sees in Name Manager, with what it refers to, its value and its scope.
fn names_list(book: &Book) -> Answer {
    let wb = &book.session.book;
    let list: Vec<serde_json::Value> = wb
        .names
        .iter()
        .filter(|n| !n.name.starts_with("_xl"))
        .map(|n| {
            let value = book
                .session
                .evaluate_at(
                    n.scope.unwrap_or(book.view.sheet),
                    book.view.cursor,
                    &format!("={}", n.formula),
                )
                .map(|v| v.to_text())
                .unwrap_or_default();
            json!({
                "name": n.name,
                "refers_to": n.formula,
                "value": value,
                "scope": n.scope.map_or("workbook".to_owned(), |i| wb.sheets[i].name.clone()),
                "hidden": n.hidden,
            })
        })
        .collect();
    Answer::said(format!("{} names", list.len())).with(json!(list))
}

/// A scope word: `workbook`, a sheet's number from 1, or a sheet's name.
fn scope_of(book: &Book, word: Option<&str>) -> Result<Option<usize>, String> {
    match word {
        None | Some("workbook") => Ok(None),
        Some(w) => match w.parse::<usize>() {
            Ok(n) if n >= 1 && n <= book.session.book.sheets.len() => Ok(Some(n - 1)),
            _ => book
                .session
                .book
                .sheet_index(w)
                .map(Some)
                .ok_or_else(|| format!("there is no sheet `{w}`")),
        },
    }
}

/// F3: a name chosen from the list goes into the formula being typed, or the active cell gets `=name`.
fn paste_name(book: &mut Book, name: Option<&String>) -> Result<Answer, String> {
    let Some(name) = name else {
        let options: Vec<(String, String)> = book
            .session
            .book
            .names
            .iter()
            .filter(|n| !n.hidden && !n.name.starts_with("_xl"))
            .map(|n| (n.name.clone(), n.name.clone()))
            .collect();
        if options.is_empty() {
            return Err("There are no names to paste.".to_owned());
        }
        let first = options[0].1.clone();
        let fields = vec![Field::choices("Paste name", "name", options, &first)];
        book.view.prompt =
            Some(Prompt::form(vec!["names".into(), "paste".into()], "Paste Name", fields));
        return Ok(Answer::said(String::new()));
    };
    match book.view.edit.as_mut() {
        Some(edit) => {
            edit.text.push_str(name);
            edit.focus = true;
        }
        None => {
            let at = book.view.cursor;
            book.view.begin_edit(at, format!("={name}"));
        }
    }
    Ok(Answer::said(format!("Pasted {name}")))
}

/// `move-cells <to> [copy] [values|formats|link|hyperlink] [shift-down|shift-right]`: what dragging the
/// chosen cells by their border does, with Ctrl to copy and Shift to insert, and what each entry of the
/// right drag menu does.
fn move_cells(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let to = args
        .first()
        .and_then(|w| CellRef::parse(w))
        .ok_or("move-cells needs where to, such as D4")?;
    let words: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let copy = words.contains(&"copy");
    let mode = if words.contains(&"values") {
        unluminous_sheet::edit::PasteMode::Values
    } else if words.contains(&"formats") {
        unluminous_sheet::edit::PasteMode::Formats
    } else {
        unluminous_sheet::edit::PasteMode::All
    };
    let shift = if words.contains(&"shift-down") {
        Some(Shift::Down)
    } else if words.contains(&"shift-right") {
        Some(Shift::Right)
    } else {
        None
    };
    let book = sheet.current_book()?;
    let from = book.view.range();
    let index = book.view.sheet;
    let size = (from.rows() - 1, from.cols() - 1);
    let dest = Range::new(to, CellRef::new(to.row + size.0, to.col + size.1));
    if words.contains(&"link") || words.contains(&"hyperlink") {
        return link_cells(sheet, from, dest, words.contains(&"hyperlink"));
    }
    let mut source = from;
    if let Some(shift) = shift {
        book.session.insert(index, dest, shift)?;
        source = after_insert(from, dest, shift);
    }
    let clip =
        book.session.copy(index, source, !copy && mode == unluminous_sheet::edit::PasteMode::All);
    let pasted = book.session.paste(&clip, index, Range::cell(to), mode)?;
    if !copy && shift.is_some() {
        // A move that inserted closes the gap the cells left, as Excel's Insert Cut Cells does.
        let back = if shift == Some(Shift::Down) { Shift::Up } else { Shift::Left };
        book.session.delete(index, source, back)?;
    }
    book.view.select_range(pasted);
    book.changed();
    Ok(Answer::said(format!(
        "{} {} to {}",
        if copy { "Copied" } else { "Moved" },
        from.a1(),
        pasted.a1()
    )))
}

/// Where the source of a move is after cells were inserted at the destination.
fn after_insert(from: Range, dest: Range, shift: Shift) -> Range {
    let moves = match shift {
        Shift::Down => {
            from.first.row >= dest.first.row
                && from.last.col >= dest.first.col
                && from.first.col <= dest.last.col
        }
        _ => {
            from.first.col >= dest.first.col
                && from.last.row >= dest.first.row
                && from.first.row <= dest.last.row
        }
    };
    if !moves {
        return from;
    }
    let (rows, cols) = match shift {
        Shift::Down => (dest.rows() as i64, 0),
        _ => (0, dest.cols() as i64),
    };
    Range::new(
        from.first.offset(rows, cols).unwrap_or(from.first),
        from.last.offset(rows, cols).unwrap_or(from.last),
    )
}

/// Link Here and Create Hyperlink Here: formulas naming the source cells, or links to them.
fn link_cells(
    sheet: &mut Spreadsheet,
    from: Range,
    dest: Range,
    hyperlink: bool,
) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let name = unluminous_sheet::formula::print::sheet_name(
        &book.session.book.sheets[book.view.sheet].name,
    );
    let mut inputs = Vec::new();
    for r in 0..from.rows() {
        for c in 0..from.cols() {
            let source = CellRef::new(from.first.row + r, from.first.col + c);
            let target = CellRef::new(dest.first.row + r, dest.first.col + c);
            inputs.push((target, source));
        }
    }
    let index = book.view.sheet;
    if hyperlink {
        for (target, source) in &inputs {
            let shown = super::display::shown(
                &book.session.book,
                &book.session.book.sheets[index],
                *source,
                255,
            )
            .text;
            book.session.set_input(index, *target, &shown)?;
            let place =
                unluminous_sheet::model::LinkTarget::Location(format!("{name}!{}", source.a1()));
            book.session.set_link(index, Range::cell(*target), Some(place), None)?;
        }
    } else {
        let typed: Vec<(CellRef, String)> =
            inputs.iter().map(|(t, s)| (*t, format!("={}", s.a1()))).collect();
        book.session.set_inputs(index, &typed)?;
    }
    book.view.select_range(dest);
    book.changed();
    Ok(Answer::said(format!("Linked {} to {}", dest.a1(), from.a1())))
}

/// `fill-handle <target> copy|series|formats|values|days|weekdays|months|years|linear|growth|flash`: what
/// the menu a right drag of the fill handle opens does.
fn fill_handle(sheet: &mut Spreadsheet, args: &[String]) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let target = args
        .first()
        .and_then(|w| Range::parse(w))
        .ok_or("fill-handle needs the range to fill, such as A1:A10")?;
    let how = args.get(1).map(String::as_str).unwrap_or("series");
    match how {
        "series" => act(sheet, Act::FillTo(target)),
        "copy" => {
            let source = book.view.range();
            let index = book.view.sheet;
            let clip = book.session.copy(index, source, false);
            book.session.paste(&clip, index, target, unluminous_sheet::edit::PasteMode::All)?;
            book.view.select_range(target);
            book.changed();
            Ok(Answer::said(format!("Copied into {}", target.a1())))
        }
        other => Err(format!("fill-handle takes copy or series here, not `{other}`")),
    }
}

/// `autocomplete list`: the entries of the column under the active cell, which Alt+Down opens as a list.
fn pick_from_list(sheet: &mut Spreadsheet) -> Result<Answer, String> {
    let book = sheet.current_book()?;
    let at = book.view.cursor;
    if let Some(filter) = &book.session.book.sheets[book.view.sheet].filter {
        if at.row == filter.range.first.row && (filter.range.first.col..=filter.range.last.col).contains(&at.col) {
            book.view.audit.open_filter = Some(at.col);
            return Ok(Answer::said("Filter menu"));
        }
    }
    let entries = super::entry::column_entries(&book.session.book.sheets[book.view.sheet], at);
    book.view.choices = true;
    Ok(Answer::said(format!("{} entries", entries.len())).with(json!(entries)))
}
