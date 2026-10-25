//! Review and auditing as verbs: comments and notes, Find and Replace, Go To Special, the trace arrows, error
//! checking, Evaluate Formula, the Watch Window and workbook statistics.

use serde_json::{json, Value as Json};
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::review::errors::{ErrorOptions, ErrorRule};
use unluminous_sheet::edit::review::evaluate::Evaluator;
use unluminous_sheet::edit::review::search::{FindOptions, Hit, LookIn, Scope};
use unluminous_sheet::edit::review::special::{areas_of, Kinds, Special};
use unluminous_sheet::edit::review::trace::Reach;

use super::verbs_a::{after, author, edit, flag, keyword, take_cell, Out};
use super::view::{Field, Prompt};
use super::view_a::Arrow;
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Run a review or auditing verb.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    Some(match verb {
        "comment" => comment(sheet, args),
        "goto-special" => goto_special(sheet, args),
        "trace" => trace(sheet, args),
        "error-check" => error_check(sheet, args),
        "evaluate" => evaluate(sheet, args),
        "watch" => watch(sheet, args),
        "statistics" => statistics(sheet),
        "error-menu" => error_menu(sheet, args),
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------ comments

/// `comment new|reply|edit|resolve|reopen|delete|list|pane ...`; the cell is the first word when it is one.
fn comment(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let word = args.first().map(String::as_str).unwrap_or("list");
    if word == "list" {
        let book = sheet.current_book()?;
        return Ok(comments_answer(book, args.get(1).map(String::as_str)));
    }
    if word == "pane" {
        let book = sheet.current_book()?;
        book.view.audit.comments_open = super::verbs_a::on_off(args.get(1).map(String::as_str))?;
        return Ok(Answer::said("Comments pane"));
    }
    if matches!(word, "new" | "reply") {
        let book = sheet.current_book()?;
        let (at, tail) = take_cell(book, &args[1..]);
        if tail.is_empty() {
            let verb = vec!["comment".to_owned(), word.to_owned(), at.a1()];
            let fields = vec![Field::bare("Comment", "")];
            book.view.prompt = Some(Prompt::form(verb, "Comment", fields));
            return Ok(Answer::said(String::new()));
        }
    }
    let word = word.to_owned();
    let rest = args[1..].to_vec();
    edit(sheet, move |book| comment_change(book, &word, &rest))
}

fn comment_change(book: &mut Book, word: &str, rest: &[String]) -> Result<String, String> {
    let (at, tail) = take_cell(book, rest);
    let index = book.view.sheet;
    let user = author();
    let text = tail.join(" ");
    let session = &mut book.session;
    match word {
        "new" => session.add_thread(index, at, &text, &user)?,
        "reply" => session.reply_to_thread(index, at, &text, &user)?,
        "edit" => {
            let n: usize = tail
                .first()
                .and_then(|w| w.parse().ok())
                .ok_or("edit needs the message number, 0 for the first")?;
            session.edit_thread_message(index, at, n, &tail[1..].join(" "))?;
        }
        "resolve" => session.resolve_thread(index, at, true)?,
        "reopen" => session.resolve_thread(index, at, false)?,
        "delete" => match tail.first().and_then(|w| w.parse::<usize>().ok()) {
            Some(n) => session.delete_thread_message(index, at, n)?,
            None => session.delete_thread(index, at)?,
        },
        other => {
            return Err(format!(
            "comment takes new, reply, edit, resolve, reopen, delete, list or pane, not `{other}`"
        ))
        }
    }
    Ok(format!("Comment on {} changed", at.a1()))
}

/// The threads and notes of the sheet, or of one cell.
pub fn comments_answer(book: &Book, cell: Option<&str>) -> Answer {
    let index = book.view.sheet;
    let s = &book.session.book.sheets[index];
    let only = cell.and_then(|w| CellRef::parse(w));
    let threads: Vec<Json> = book
        .session
        .thread_rows(Some(index))
        .iter()
        .filter(|r| only.is_none_or(|c| c == r.at))
        .map(|r| {
            let thread = book.session.thread_at(index, r.at);
            let messages: Vec<Json> = thread.map(|t| t.messages().map(|m| json!({ "author": book.session.person_name(&m.person), "text": m.text })).collect()).unwrap_or_default();
            json!({ "cell": r.at.a1(), "author": r.author, "resolved": r.resolved, "replies": r.replies, "messages": messages })
        })
        .collect();
    let notes: Vec<Json> = s
        .notes
        .iter()
        .filter(|((row, col), _)| only.is_none_or(|c| (c.row, c.col) == (*row, *col)))
        .map(|((row, col), n)| json!({ "cell": CellRef::new(*row, *col).a1(), "text": n.text, "author": n.author, "shown": n.visible }))
        .collect();
    Answer::said(format!("{} comments, {} notes", threads.len(), notes.len()))
        .with(json!({ "threads": threads, "notes": notes }))
}

/// `note show|hide [cell|all]`, `note next|previous`, `note size <w> <h>`, `note box <l> <t> <w> <h>` and
/// `note convert [range]`; other words are left to the old `note`.
pub fn note_more(sheet: &mut Spreadsheet, args: &[String]) -> Option<Out> {
    let probe = {
        let book = sheet.current_book().ok()?;
        let (_, after_cell) = take_cell(book, args);
        after_cell.to_vec()
    };
    let has_cell = probe.len() != args.len();
    if probe.first().map(String::as_str) == Some("edit") && probe.len() == 1 {
        return Some(note_prompt(sheet));
    }
    let words: Vec<String> = if has_cell { args[1..].to_vec() } else { args.to_vec() };
    let first = words.first()?.clone();
    if !matches!(first.as_str(), "show" | "hide" | "next" | "previous" | "size" | "box" | "convert")
    {
        return None;
    }
    let target = if has_cell { Some(args[0].clone()) } else { None };
    Some(edit(sheet, move |book| note_change(book, &first, target.as_deref(), &words[1..])))
}

/// Shift+F2: a box holding the active cell's note, which OK writes back.
fn note_prompt(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let at = book.view.cursor;
    let text = book.session.book.sheets[book.view.sheet]
        .notes
        .get(&(at.row, at.col))
        .map(|n| n.text.clone())
        .unwrap_or_default();
    let verb = vec!["note".to_owned(), at.a1()];
    book.view.prompt = Some(Prompt::form(verb, "Note", vec![Field::bare("Note", &text)]));
    Ok(Answer::said(String::new()))
}

/// `error-menu [cell]` opens the menu of an error indicator, as Alt+Shift+F10 does.
fn error_menu(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let at = super::verbs_a::cell_or_active(book, args.first().map(String::as_str))?;
    let found = book.session.check_errors(
        Some(book.view.sheet),
        Some(Range::cell(at)),
        ErrorOptions::default(),
    );
    let f = found.first().ok_or("There is no error indicator on this cell.")?;
    let items = crate::components::spreadsheet::overlay_a::error_menu(at, f.rule, f.message());
    let origin = book
        .view
        .drawn
        .map_or(egui::Pos2::new(120.0, 120.0), |r| r.min + egui::Vec2::new(80.0, 60.0));
    book.view.drop_menu = Some((origin, items));
    Ok(Answer::said(f.message()))
}

fn note_change(
    book: &mut Book,
    word: &str,
    cell: Option<&str>,
    rest: &[String],
) -> Result<String, String> {
    let index = book.view.sheet;
    let at = super::verbs_a::cell_or_active(book, cell)?;
    let number = |i: usize| {
        rest.get(i)
            .and_then(|w| w.parse::<f64>().ok())
            .ok_or_else(|| "a size is a number of points".to_owned())
    };
    match word {
        "show" | "hide" => {
            let shown = word == "show";
            if rest.first().map(String::as_str) == Some("all") {
                let n = book.session.show_all_notes(Some(index), shown)?;
                return Ok(format!("{n} notes {}", if shown { "shown" } else { "hidden" }));
            }
            book.session.show_note(index, at, shown)?;
        }
        "next" | "previous" => {
            let found = book
                .session
                .neighbouring_note(index, at, word == "next")
                .ok_or("There are no notes.")?;
            book.view.select_cell(found);
            return Ok(format!("Note on {}", found.a1()));
        }
        "size" => book.session.set_note_size(index, at, number(0)?, number(1)?)?,
        "box" => {
            book.session.set_note_box(index, at, number(0)?, number(1)?, number(2)?, number(3)?)?
        }
        _ => {
            let range = rest.first().and_then(|w| Range::parse(&w.replace('$', "")));
            let n = book.session.convert_notes_to_threads(index, range)?;
            return Ok(format!("{n} notes converted to comments"));
        }
    }
    Ok(format!("Note on {} changed", at.a1()))
}

// ---------------------------------------------------------------------------------------------------- find

/// The options Find and Replace words describe.
fn find_options(book: &Book, text: &str, flags: &[String]) -> Result<FindOptions, String> {
    let scope = if flag(flags, "book") { Scope::Workbook } else { Scope::Sheet(book.view.sheet) };
    let mut options = FindOptions::new(text, scope);
    options.look_in = if flag(flags, "formulas") {
        LookIn::Formulas
    } else if flag(flags, "notes") {
        LookIn::Notes
    } else if flag(flags, "comments") {
        LookIn::Comments
    } else {
        LookIn::Values
    };
    options.by_columns = flag(flags, "by-columns");
    options.match_case = flag(flags, "case");
    options.entire_cell = flag(flags, "whole");
    if let Some(w) = after(flags, "format") {
        let at = super::verbs_a::cell_or_active(book, Some(w))?;
        options.format =
            Some(super::display::style_at(&book.session.book.sheets[book.view.sheet], at));
    }
    if let Some(w) = after(flags, "within") {
        options.within = Some(super::verbs_a::ranges_on(book, w)?);
    }
    Ok(options)
}

/// `find <text> [formulas|notes|comments] [whole] [case] [book] [by-columns] [format <cell>] [within <range>]`
/// and `replace <find> <with> ...`: wildcards `?` `*` `~` are always read.
pub fn find_or_replace(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Out {
    let text = args.first().ok_or("find needs text")?.clone();
    if verb == "replace" {
        let with = args.get(1).cloned().unwrap_or_default();
        let flags = args[2.min(args.len())..].to_vec();
        return edit(sheet, move |book| {
            let options = find_options(book, &text, &flags)?;
            let n = book.session.replace_cells(&options, &with)?;
            Ok(format!("{n} replaced"))
        });
    }
    let book = sheet.current_book()?;
    let options = find_options(book, &text, &args[1..])?;
    let found = book.session.find_cells(&options);
    if let Some(Hit { sheet: s, at }) = found.first() {
        book.view.sheet = *s;
        book.view.select_cell(*at);
    }
    let list: Vec<String> = found
        .iter()
        .map(|h| format!("{}!{}", book.session.book.sheets[h.sheet].name, h.at.a1()))
        .collect();
    Ok(Answer::said(format!("{} found", list.len())).with(json!(list)))
}

// ---------------------------------------------------------------------------------------------- go to special

fn kinds_from(words: &[String]) -> Kinds {
    let any = ["numbers", "text", "logical", "errors"].iter().any(|k| flag(words, k));
    if !any {
        return Kinds::ALL;
    }
    Kinds {
        numbers: flag(words, "numbers"),
        text: flag(words, "text"),
        logicals: flag(words, "logical"),
        errors: flag(words, "errors"),
    }
}

/// `goto-special <kind> [numbers|text|logical|errors] [all] [same]`: notes, comments, constants, formulas,
/// blanks, region, array, row-differences, column-differences, precedents, dependents, last, visible,
/// conditional, validation, objects.
fn goto_special(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let kind = args.first().map(String::as_str).ok_or("goto-special needs a kind")?;
    let rest = &args[1..];
    let special = match kind {
        "notes" => Special::Notes,
        "comments" => Special::Comments,
        "constants" => Special::Constants(kinds_from(rest)),
        "formulas" => Special::Formulas(kinds_from(rest)),
        "blanks" => Special::Blanks,
        "region" => Special::CurrentRegion,
        "array" => Special::CurrentArray,
        "row-differences" => Special::RowDifferences,
        "column-differences" => Special::ColumnDifferences,
        "precedents" => Special::Precedents { all_levels: flag(rest, "all") },
        "dependents" => Special::Dependents { all_levels: flag(rest, "all") },
        "last" => Special::LastCell,
        "visible" => Special::VisibleCells,
        "conditional" => Special::ConditionalFormats { same: flag(rest, "same") },
        "validation" => Special::Validation { same: flag(rest, "same") },
        "objects" => return objects_answer(book),
        other => return Err(format!("`{other}` is not a Go To Special kind")),
    };
    let index = book.view.sheet;
    let selection: Vec<Range> =
        std::iter::once(book.view.range()).chain(book.view.extra.iter().copied()).collect();
    let cells = book.session.special_cells(index, &selection, book.view.cursor, special)?;
    let areas = areas_of(&cells);
    choose_areas(book, &areas);
    Ok(Answer::said(format!("{} cells", cells.len())).with(json!({
        "cells": cells.len(), "areas": areas.iter().map(Range::a1).collect::<Vec<_>>(),
    })))
}

fn objects_answer(book: &Book) -> Out {
    let objects = book.session.drawing_objects(book.view.sheet);
    let list: Vec<Json> = objects
        .iter()
        .map(|o| json!({ "kind": o.kind, "from": o.from.a1(), "to": o.to.a1() }))
        .collect();
    Ok(Answer::said(format!("{} objects", list.len())).with(json!(list)))
}

/// Choose several areas at once, the last as the active one.
fn choose_areas(book: &mut Book, areas: &[Range]) {
    let Some((last, others)) = areas.split_last() else { return };
    book.view.held = None;
    book.view.extra = others.to_vec();
    book.view.cursor = last.first;
    book.view.anchor = last.last;
    book.view.reveal = true;
}

// ------------------------------------------------------------------------------------------------- tracing

/// `trace precedents|dependents [all]`, `trace error`, `trace clear`.
fn trace(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (word, rest) = keyword(args, &["precedents", "dependents", "error", "clear"])
        .ok_or("trace takes precedents, dependents, error or clear")?;
    let index = book.view.sheet;
    let at = book.view.cursor;
    let all = flag(rest, "all");
    let arrows = match word {
        "clear" => {
            book.view.audit.arrows.clear();
            return Ok(Answer::said("Arrows removed").with(json!({ "arrows": 0 })));
        }
        "precedents" => walk(book, index, at, true, all),
        "dependents" => walk(book, index, at, false, all),
        _ => {
            let cell = super::verbs_a::cell_or_active(book, rest.first().map(String::as_str))?;
            error_arrows(book, index, cell)?
        }
    };
    let count = arrows.len();
    if count == 0 && word != "error" {
        return Err(format!("The cell has no {word} to trace."));
    }
    book.view.audit.arrows.extend(arrows);
    Ok(Answer::said(format!("{count} arrows"))
        .with(json!({ "arrows": book.view.audit.arrows.len(), "added": count })))
}

fn error_arrows(book: &Book, index: usize, at: CellRef) -> Result<Vec<Arrow>, String> {
    let trace = book.session.trace_error(index, at)?;
    Ok(trace
        .arrows
        .iter()
        .map(|((fs, f), (ts, t))| Arrow {
            from_sheet: *fs,
            from: Range::cell(*f),
            to_sheet: *ts,
            to: *t,
            other_sheet: fs != ts,
            error: true,
        })
        .collect())
}

/// The arrows from or to a cell: one level, or every level (capped so a huge chain cannot stall the window).
fn walk(book: &Book, index: usize, start: CellRef, precedents: bool, all: bool) -> Vec<Arrow> {
    const MOST_CELLS: usize = 2000;
    let mut arrows = Vec::new();
    let mut seen = vec![(index, start)];
    let mut frontier = vec![(index, start)];
    while let Some((s, at)) = frontier.pop() {
        let reaches: Vec<Reach> = if precedents {
            book.session.direct_precedents(s, at)
        } else {
            book.session.dependents(s, Range::cell(at), false)
        };
        for reach in reaches {
            arrows.push(if precedents {
                Arrow {
                    from_sheet: reach.sheet,
                    from: reach.range,
                    to_sheet: s,
                    to: at,
                    other_sheet: reach.other_sheet,
                    error: false,
                }
            } else {
                Arrow {
                    from_sheet: s,
                    from: Range::cell(at),
                    to_sheet: reach.sheet,
                    to: reach.range.first,
                    other_sheet: reach.other_sheet,
                    error: false,
                }
            });
            if all && !reach.other_sheet {
                for row in reach.range.first.row..=reach.range.last.row {
                    for col in reach.range.first.col..=reach.range.last.col {
                        let next = (reach.sheet, CellRef::new(row, col));
                        if seen.len() < MOST_CELLS && !seen.contains(&next) {
                            seen.push(next);
                            frontier.push(next);
                        }
                    }
                }
            }
        }
    }
    arrows
}

// ----------------------------------------------------------------------------------------- error checking

const RULES: [(&str, ErrorRule); 8] = [
    ("eval-error", ErrorRule::EvalError),
    ("two-digit-year", ErrorRule::TwoDigitTextYear),
    ("number-as-text", ErrorRule::NumberAsText),
    ("inconsistent-formula", ErrorRule::InconsistentFormula),
    ("omitted-cells", ErrorRule::OmittedCells),
    ("unlocked-formula", ErrorRule::UnlockedFormula),
    ("empty-reference", ErrorRule::EmptyCellReference),
    ("inconsistent-column", ErrorRule::InconsistentCalculatedColumn),
];

/// The name of a rule as the verb writes it.
pub fn rule_name(rule: ErrorRule) -> &'static str {
    RULES.iter().find(|(_, r)| *r == rule).map_or("", |(n, _)| n)
}

/// The findings on a sheet, as data: the cell, the rule's name and Excel's message.
pub fn findings_json(book: &Book, within: Option<Range>, all: bool) -> Vec<Json> {
    let options = if all { ErrorOptions::all() } else { ErrorOptions::default() };
    book.session
        .check_errors(Some(book.view.sheet), within, options)
        .iter()
        .map(|f| json!({ "cell": f.at.a1(), "rule": rule_name(f.rule), "message": f.message() }))
        .collect()
}

/// `error-check [range] [all]`, `error-check ignore <cell> <rule>`, `error-check reset`.
fn error_check(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    match args.first().map(String::as_str) {
        Some("ignore") => {
            let rule = args
                .get(2)
                .and_then(|w| RULES.iter().find(|(n, _)| n == w))
                .map(|(_, r)| *r)
                .ok_or("name the rule: eval-error, number-as-text, inconsistent-formula, ...")?;
            let word = args[1].clone();
            edit(sheet, move |book| {
                let at = super::verbs_a::cell_or_active(book, Some(&word))?;
                let index = book.view.sheet;
                book.session.ignore_error(index, Range::cell(at), rule)?;
                Ok(format!("Error ignored on {}", at.a1()))
            })
        }
        Some("reset") => edit(sheet, |book| {
            let index = book.view.sheet;
            book.session.reset_ignored_errors(index)?;
            Ok("Ignored errors reset".to_owned())
        }),
        _ => {
            let book = sheet.current_book()?;
            let within = args.first().and_then(|w| Range::parse(&w.replace('$', "")));
            let list = findings_json(book, within, flag(args, "all"));
            Ok(Answer::said(format!("{} findings", list.len())).with(json!(list)))
        }
    }
}

// ---------------------------------------------------------------------------------------------- evaluate

/// What the stepper shows now.
pub fn evaluator_json(book: &Book) -> Json {
    match &book.view.audit.evaluator {
        None => Json::Null,
        Some(ev) => {
            let v = ev.view(&book.session.book);
            json!({
                "reference": v.reference, "text": v.text,
                "underlined": v.underlined.map(|(a, b)| json!([a, b])),
                "canEvaluate": v.can_evaluate, "canStepIn": v.can_step_in, "canStepOut": v.can_step_out,
                "result": v.result.map(|r| r.to_text()),
            })
        }
    }
}

/// `evaluate <cell>` starts the stepper; `evaluate evaluate|in|out|restart|finish|close` drives it.
fn evaluate(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let word = args.first().map(String::as_str);
    if !matches!(
        word,
        Some("evaluate")
            | Some("in")
            | Some("out")
            | Some("restart")
            | Some("finish")
            | Some("close")
    ) {
        let at = super::verbs_a::cell_or_active(book, word)?;
        let ev: Evaluator = book.session.evaluator(book.view.sheet, at)?;
        book.view.audit.evaluator = Some(ev);
        return Ok(Answer::said("Evaluate Formula").with(evaluator_json(book)));
    }
    if word == Some("close") {
        book.view.audit.evaluator = None;
        return Ok(Answer::said("Evaluate Formula closed"));
    }
    let mut ev = book.view.audit.evaluator.take().ok_or("Start with `evaluate <cell>`.")?;
    let result = match word {
        Some("evaluate") => ev.evaluate(&book.session.book),
        Some("in") => ev.step_in(&book.session.book),
        Some("out") => ev.step_out(&book.session.book),
        Some("restart") => {
            ev.restart();
            Ok(())
        }
        _ => {
            ev.finish(&book.session.book);
            Ok(())
        }
    };
    book.view.audit.evaluator = Some(ev);
    result?;
    Ok(Answer::said("Evaluate Formula").with(evaluator_json(book)))
}

// ------------------------------------------------------------------------------------------------- watch

/// `watch add [cell]`, `watch remove [cell]`, `watch list`, `watch show|hide`.
fn watch(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let word = args.first().map(String::as_str).unwrap_or("list");
    match word {
        "list" => {
            let book = sheet.current_book()?;
            Ok(watch_answer(book))
        }
        "show" | "hide" => {
            let book = sheet.current_book()?;
            book.view.audit.watch_open = word == "show";
            Ok(Answer::said("Watch Window"))
        }
        "add" | "remove" => {
            let word = word.to_owned();
            let cell = args.get(1).cloned();
            edit(sheet, move |book| {
                let at = super::verbs_a::cell_or_active(book, cell.as_deref())?;
                let index = book.view.sheet;
                if word == "add" {
                    book.session.add_watch(index, at)?;
                } else {
                    book.session.delete_watch(index, at)?;
                }
                Ok(format!("Watch {} {}", at.a1(), word))
            })
        }
        other => Err(format!("watch takes add, remove, list, show or hide, not `{other}`")),
    }
}

/// The watched cells with their values.
pub fn watch_answer(book: &Book) -> Answer {
    let rows: Vec<Json> = book
        .session
        .watch_rows()
        .iter()
        .map(|w| json!({ "sheet": w.sheet_name, "name": w.name, "cell": w.at.a1(), "value": w.value.to_text(), "formula": w.formula }))
        .collect();
    Answer::said(format!("{} watched", rows.len())).with(json!(rows))
}

/// Workbook statistics.
fn statistics(sheet: &mut Spreadsheet) -> Out {
    let book = sheet.current_book()?;
    let s = book.session.workbook_statistics();
    Ok(Answer::said("Workbook Statistics").with(json!({
        "sheets": s.sheets, "cellsWithData": s.cells_with_data, "formulas": s.formulas, "tables": s.tables,
        "charts": s.charts, "pivotTables": s.pivot_tables, "externalLinks": s.external_links, "names": s.names,
        "notes": s.notes, "comments": s.comments,
    })))
}
