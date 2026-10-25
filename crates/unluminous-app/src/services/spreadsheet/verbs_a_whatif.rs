//! What if analysis, protection, named cell styles and themes as verbs.

use serde_json::json;
use unluminous_sheet::cellstyles::Includes;
use unluminous_sheet::protect::{Allow, SheetProtection};
use unluminous_sheet::styles::{Color, FontEdit, StyleChange};
use unluminous_sheet::theme;
use unluminous_sheet::Value;

use super::verbs_a::{after, author, cell_or_active, edit, flag, keyword, ranges_on, Out};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a what if, protection, style or theme verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "goal-seek" => goal_seek(sheet, args),
        "data-table" => data_table(sheet, args),
        "scenario" => scenario(sheet, args),
        "protect" => protect(sheet, args),
        "unprotect" => unprotect(sheet, args),
        "edit-range" => edit_range(sheet, args),
        "style" => style(sheet, args),
        "theme" => theme_verb(sheet, args),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------- what if

/// `goal-seek <set cell> <to value> <by changing cell>`
fn goal_seek(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let set = cell_or_active(book, args.first().map(String::as_str))?;
    let to: f64 =
        args.get(1).and_then(|w| w.parse().ok()).ok_or("goal-seek needs the value to reach")?;
    let by = cell_or_active(book, Some(args.get(2).ok_or("goal-seek needs the cell to change")?))?;
    let index = book.view.sheet;
    let found = book.session.goal_seek(index, set, to, by)?;
    book.changed();
    let message = if found.found {
        format!("Goal Seeking with Cell {} found a solution.", set.a1())
    } else {
        format!("Goal Seeking with Cell {} may not have found a solution.", set.a1())
    };
    Ok(Answer::said(message).with(json!({
        "found": found.found, "value": found.value, "target": found.target, "iterations": found.iterations,
    })))
}

/// `data-table <range> [row <input cell>] [column <input cell>]`, or `data-table remove <cell>`.
fn data_table(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let index = book.view.sheet;
        if let Some(("remove", rest)) = keyword(args, &["remove"]) {
            let at = cell_or_active(book, rest.first().map(String::as_str))?;
            book.session.remove_data_table(index, at)?;
            return Ok("Data table turned into values".to_owned());
        }
        let (range, rest) = super::verbs_a::chosen(book, args);
        let input =
            |name: &str| after(rest, name).map(|w| cell_or_active(book, Some(w))).transpose();
        let (row, column) = (input("row")?, input("column")?);
        book.session.create_data_table(index, range, row, column)?;
        Ok(format!("Data table made over {}", range.a1()))
    })
}

/// The text of a scenario value as the cell would take it.
fn scenario_value(text: &str) -> Value {
    text.parse::<f64>().map(Value::Number).unwrap_or_else(|_| Value::Text(text.into()))
}

/// `scenario list|show <name>|delete <name>|add <name> <cells> [values a,b] [comment ...]|edit <name> ...|
/// summary <result cells>`
fn scenario(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    let word = args.first().map(String::as_str).unwrap_or("list");
    if word == "list" {
        let all = book.session.scenarios(index);
        let list: Vec<_> = all.list.iter().map(|s| json!({
            "name": s.name, "comment": s.comment, "user": s.user,
            "cells": s.cells.iter().map(|c| json!({ "cell": c.at.a1(), "value": c.value })).collect::<Vec<_>>(),
        })).collect();
        return Ok(Answer::said(format!("{} scenarios", list.len())).with(json!(list)));
    }
    if word == "pane" {
        book.view.audit.scenarios_open = super::verbs_a::on_off(args.get(1).map(String::as_str))?;
        return Ok(Answer::said("Scenario Manager"));
    }
    let name = args.get(1).cloned().unwrap_or_default();
    let rest = args.to_vec();
    edit(sheet, move |book| scenario_change(book, word_of(&rest), &name, &rest))
}

fn word_of(args: &[String]) -> &str {
    args.first().map(String::as_str).unwrap_or("list")
}

fn scenario_change(
    book: &mut Book,
    word: &str,
    name: &str,
    args: &[String],
) -> Result<String, String> {
    let index = book.view.sheet;
    let user = author();
    let values = |tail: &[String]| {
        after(tail, "values")
            .map(|v| v.split(',').map(|x| scenario_value(x.trim())).collect::<Vec<_>>())
    };
    let comment =
        |tail: &[String]| tail.iter().position(|w| w == "comment").map(|i| tail[i + 1..].join(" "));
    let cells = |book: &Book, tail: &[String]| {
        tail.get(2)
            .filter(|w| !matches!(w.as_str(), "values" | "comment" | "name"))
            .map(|w| ranges_on(book, w))
            .transpose()
    };
    match word {
        "add" => {
            let changing = cells(book, args)?.ok_or("scenario add needs the changing cells")?;
            let v = values(args);
            book.session.add_scenario(
                index,
                name,
                &changing,
                v.as_deref(),
                comment(args).as_deref(),
                &user,
                flag(args, "locked"),
                flag(args, "hidden"),
            )?;
            Ok(format!("Scenario {name} added"))
        }
        "edit" => {
            let changing = cells(book, args)?;
            let v = values(args);
            book.session.edit_scenario(
                index,
                name,
                after(args, "name"),
                changing.as_deref(),
                v.as_deref(),
                comment(args).as_deref(),
                &user,
            )?;
            Ok(format!("Scenario {name} changed"))
        }
        "delete" => {
            book.session.delete_scenario(index, name)?;
            Ok(format!("Scenario {name} deleted"))
        }
        "show" => {
            book.session.show_scenario(index, name)?;
            Ok(format!("Scenario {name} shown"))
        }
        "summary" => {
            let results = ranges_on(book, args.get(1).ok_or("summary needs the result cells")?)?;
            let made = book.session.scenario_summary(index, &results)?;
            book.view.sheet = made.min(book.session.book.sheets.len() - 1);
            Ok("Scenario summary made".to_owned())
        }
        other => {
            Err(format!("scenario takes list, add, edit, delete, show or summary, not `{other}`"))
        }
    }
}

// ---------------------------------------------------------------------------------------------- protection

/// Set the allow flags the words name: `format-cells`, `sort`, and the others; `no-select-locked` clears one.
fn allow_from(words: &[String]) -> Result<Allow, String> {
    let mut a = Allow::default();
    for w in words {
        let (on, name) = match w.strip_prefix("no-") {
            Some(n) => (false, n),
            None => (true, w.as_str()),
        };
        match name {
            "format-cells" => a.format_cells = on,
            "format-columns" => a.format_columns = on,
            "format-rows" => a.format_rows = on,
            "insert-columns" => a.insert_columns = on,
            "insert-rows" => a.insert_rows = on,
            "insert-hyperlinks" => a.insert_hyperlinks = on,
            "delete-columns" => a.delete_columns = on,
            "delete-rows" => a.delete_rows = on,
            "select-locked" => a.select_locked_cells = on,
            "select-unlocked" => a.select_unlocked_cells = on,
            "sort" => a.sort = on,
            "auto-filter" => a.auto_filter = on,
            "pivot-tables" => a.pivot_tables = on,
            other => return Err(format!("`{other}` is not something protection can allow")),
        }
    }
    Ok(a)
}

/// `protect sheet [password <p>] [allow <a,b,...>] [no-contents] [no-objects] [no-scenarios]`, or
/// `protect book [password <p>] [structure] [windows]`.
fn protect(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let (what, rest) =
            keyword(args, &["sheet", "book"]).ok_or("protect takes sheet or book")?;
        let password = after(rest, "password");
        let index = book.view.sheet;
        if what == "book" {
            let windows = flag(rest, "windows");
            let structure = flag(rest, "structure") || !windows;
            book.session.protect_workbook(password, structure, windows)?;
            return Ok("Workbook protected".to_owned());
        }
        let allow = match after(rest, "allow") {
            Some(list) => allow_from(&list.split(',').map(str::to_owned).collect::<Vec<_>>())?,
            None => Allow::default(),
        };
        let options = SheetProtection {
            sheet: !flag(rest, "no-contents"),
            objects: !flag(rest, "no-objects"),
            scenarios: !flag(rest, "no-scenarios"),
            allow,
            ..SheetProtection::default()
        };
        book.session.protect_sheet(index, password, &options)?;
        Ok("Sheet protected".to_owned())
    })
}

/// `unprotect [sheet|book] [password <p>]`
fn unprotect(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    edit(sheet, |book| {
        let password = after(args, "password");
        if flag(args, "book") {
            book.session.unprotect_workbook(password)?;
            return Ok("Workbook unprotected".to_owned());
        }
        let index = book.view.sheet;
        book.session.unprotect_sheet(index, password)?;
        Ok("Sheet unprotected".to_owned())
    })
}

/// `edit-range add <title> <ranges> [password <p>]`, `remove <title>`, `unlock <title> [password <p>]`, `list`.
fn edit_range(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let index = book.view.sheet;
    if matches!(args.first().map(String::as_str), None | Some("list")) {
        let list: Vec<_> = book.session.edit_ranges(index).iter().map(|r| json!({
            "title": r.name, "ranges": r.ranges.iter().map(|x| x.a1()).collect::<Vec<_>>(), "password": r.secret.is_some(),
        })).collect();
        return Ok(Answer::said(format!("{} ranges", list.len())).with(json!(list)));
    }
    edit(sheet, |book| {
        let index = book.view.sheet;
        let title = args.get(1).ok_or("name the range by its title")?;
        let password = after(args, "password");
        match args[0].as_str() {
            "add" => {
                let ranges = ranges_on(book, args.get(2).ok_or("add needs the ranges")?)?;
                book.session.add_edit_range(index, title, &ranges, password)?;
            }
            "remove" => book.session.remove_edit_range(index, title, password)?,
            "unlock" => book.session.unlock_edit_range(index, title, password.unwrap_or(""))?,
            other => {
                return Err(format!("edit-range takes add, remove, unlock or list, not `{other}`"))
            }
        }
        Ok(format!("Range {title} changed"))
    })
}

// --------------------------------------------------------------------------------------------------- styles

const STYLE_WORDS: &[&str] = &[
    "from",
    "include",
    "fill",
    "font-color",
    "bold",
    "italic",
    "size",
    "font",
    "number",
    "align",
    "name",
];

/// The style name: the words up to the first keyword, so `Heading 1` needs no quotes.
fn style_name(words: &[String]) -> String {
    let end = words.iter().position(|w| STYLE_WORDS.contains(&w.as_str())).unwrap_or(words.len());
    words[..end].join(" ")
}

/// Which groups a style includes, from `include number,font,...`; every group by default.
fn includes_from(words: &[String]) -> Option<Includes> {
    let list = after(words, "include")?;
    let has = |n: &str| list.split(',').any(|x| x == n || x == "all");
    Some(Includes {
        number: has("number"),
        font: has("font"),
        alignment: has("alignment"),
        border: has("border"),
        fill: has("fill"),
        protection: has("protection"),
    })
}

/// The changes `style modify` words describe.
fn style_changes(words: &[String]) -> Result<Vec<StyleChange>, String> {
    let mut out = Vec::new();
    let mut font = FontEdit::default();
    if let Some(c) = after(words, "fill") {
        out.push(StyleChange::Fill(Color::parse_hex(c)));
    }
    font.color = after(words, "font-color").and_then(Color::parse_hex);
    font.bold = flag(words, "bold").then_some(true);
    font.italic = flag(words, "italic").then_some(true);
    font.size = after(words, "size").and_then(|s| s.parse().ok());
    font.name = after(words, "font").map(str::to_owned);
    if font != FontEdit::default() {
        out.push(StyleChange::Font(font));
    }
    if let Some(code) = after(words, "number") {
        out.push(StyleChange::NumberFormat(code.to_owned()));
    }
    Ok(out)
}

/// `style apply|new|modify|delete|list|merge ...`
fn style(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    if matches!(args.first().map(String::as_str), None | Some("list")) {
        return Ok(styles_answer(book));
    }
    let word = args[0].clone();
    let rest = args[1..].to_vec();
    edit(sheet, move |book| style_change(book, &word, &rest))
}

fn style_change(book: &mut Book, word: &str, rest: &[String]) -> Result<String, String> {
    let index = book.view.sheet;
    let name = style_name(rest);
    match word {
        "apply" => {
            let ranges = std::iter::once(book.view.range())
                .chain(book.view.extra.iter().copied())
                .collect::<Vec<_>>();
            book.session.apply_style(index, &ranges, &name)?;
        }
        "new" => {
            let from = match after(rest, "from") {
                Some(w) => Some((index, cell_or_active(book, Some(w))?)),
                None => Some((index, book.view.cursor)),
            };
            book.session.new_style(&name, from, includes_from(rest).unwrap_or(Includes::ALL))?;
        }
        "modify" => {
            let changes = style_changes(rest)?;
            book.session.modify_style(&name, &changes, includes_from(rest))?;
        }
        "delete" => book.session.delete_style(&name)?,
        "merge" => {
            let path = std::path::Path::new(rest.first().ok_or("merge needs a workbook file")?);
            let full = if path.is_absolute() {
                path.to_path_buf()
            } else {
                book.path.parent().map(|p| p.join(path)).unwrap_or_else(|| path.to_path_buf())
            };
            let kind = unluminous_sheet::FileKind::of(&full)?;
            let bytes = std::fs::read(&full)
                .map_err(|e| format!("{} could not be read: {e}", full.display()))?;
            let other = unluminous_sheet::open(&bytes, kind, "merge", 2026)?;
            let n = book.session.merge_styles(&other)?;
            return Ok(format!("{n} styles merged"));
        }
        other => {
            return Err(format!(
                "style takes apply, new, modify, delete, merge or list, not `{other}`"
            ))
        }
    }
    Ok(format!("Style {name}"))
}

/// The cell styles the workbook offers and the style of the active cell.
pub fn styles_answer(book: &Book) -> Answer {
    let session = &book.session;
    let list: Vec<_> = session.style_gallery().iter().map(|s| json!({
        "name": s.name, "builtIn": s.builtin_id.is_some(), "defined": s.defined, "modified": s.modified,
    })).collect();
    let here = session.style_of(book.view.sheet, book.view.cursor);
    Answer::said(format!("{} styles", list.len())).with(json!({ "cell": here, "styles": list }))
}

// ---------------------------------------------------------------------------------------------------- theme

/// `theme` reads the theme back; `theme list`, `theme <name>`, `theme colors <scheme>|set <slot>=<hex>...`,
/// `theme fonts <scheme>|<major> <minor>` change it.
fn theme_verb(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    if args.is_empty() {
        let colors = book.session.theme_colors().map(|c| json!({
            "name": c.name, "slots": c.slots.iter().enumerate().map(|(i, s)| json!({ "slot": theme::SLOT_NAMES[i.min(11)], "rgb": format!("{:06X}", s.rgb()) })).collect::<Vec<_>>(),
        }));
        let fonts = book
            .session
            .theme_fonts()
            .map(|f| json!({ "name": f.name, "major": f.major, "minor": f.minor }));
        return Ok(Answer::said("Theme").with(json!({ "colors": colors, "fonts": fonts })));
    }
    if args[0] == "list" {
        let names: Vec<_> = theme::BUILT_IN.iter().map(|t| t.name).collect();
        return Ok(Answer::said(format!("{} themes", names.len())).with(json!({
            "themes": names, "colors": theme::color_scheme_names(), "fonts": theme::font_scheme_names(),
        })));
    }
    let args = args.to_vec();
    edit(sheet, move |book| theme_change(book, &args))
}

fn theme_change(book: &mut Book, args: &[String]) -> Result<String, String> {
    match args[0].as_str() {
        "colors" | "colours" => match args.get(1).map(String::as_str) {
            Some("set") => {
                let changes =
                    args[2..].iter().map(|w| slot_change(w)).collect::<Result<Vec<_>, _>>()?;
                book.session.set_theme_colors(&changes, after(args, "name"))?;
            }
            _ => book.session.apply_theme_colors(&args[1..].join(" "))?,
        },
        "fonts" => {
            if args.len() == 3
                && !theme::font_scheme_names().iter().any(|n| *n == args[1..].join(" "))
            {
                book.session.set_theme_fonts(Some(&args[1]), Some(&args[2]))?;
            } else {
                book.session.apply_theme_fonts(&args[1..].join(" "))?;
            }
        }
        _ => book.session.apply_theme(&args.join(" "))?,
    }
    Ok("Theme changed".to_owned())
}

/// `accent1=4472C4` or `4=4472C4`: a slot by name or number, and its colour.
fn slot_change(word: &str) -> Result<(usize, u32), String> {
    let (slot, hex) = word.split_once('=').ok_or("a colour is written slot=RRGGBB")?;
    let index = slot
        .parse::<usize>()
        .ok()
        .or_else(|| theme::SLOTS.iter().position(|s| s.eq_ignore_ascii_case(slot)))
        .ok_or_else(|| format!("`{slot}` is not a theme colour slot"))?;
    let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16)
        .map_err(|_| format!("`{hex}` is not a colour"))?;
    Ok((index, rgb))
}

/// The parts of the protection the sheet and the workbook have, for the read back verb.
pub fn protection_json(book: &Book) -> serde_json::Value {
    let index = book.view.sheet;
    let session = &book.session;
    let sheet = session.sheet_protection(index).map(|p| json!({
        "protected": p.sheet, "password": p.secret.is_some(), "objects": p.objects, "scenarios": p.scenarios,
        "allow": {
            "formatCells": p.allow.format_cells, "formatColumns": p.allow.format_columns, "formatRows": p.allow.format_rows,
            "insertColumns": p.allow.insert_columns, "insertRows": p.allow.insert_rows, "insertHyperlinks": p.allow.insert_hyperlinks,
            "deleteColumns": p.allow.delete_columns, "deleteRows": p.allow.delete_rows, "sort": p.allow.sort,
            "autoFilter": p.allow.auto_filter, "pivotTables": p.allow.pivot_tables,
            "selectLockedCells": p.allow.select_locked_cells, "selectUnlockedCells": p.allow.select_unlocked_cells,
        },
    }));
    let workbook = session.workbook_protection().map(|p| json!({
        "structure": p.lock_structure, "windows": p.lock_windows, "password": p.secret.is_some(),
    }));
    let ranges: Vec<_> = session.edit_ranges(index).iter().map(|r| json!({
        "title": r.name, "ranges": r.ranges.iter().map(|x| x.a1()).collect::<Vec<_>>(), "password": r.secret.is_some(),
    })).collect();
    json!({ "sheet": sheet, "workbook": workbook, "editRanges": ranges })
}
