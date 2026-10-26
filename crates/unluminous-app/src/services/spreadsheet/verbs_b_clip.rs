//! The clipboard and fill verbs (`task-2360`, the `wiringb` lane): Paste Special with every option, Paste
//! Link, Insert Copied and Insert Cut Cells, the Paste Options and Auto Fill Options buttons, the fill
//! handle's menu, the Series form, Fill Justify, Fill Across Worksheets, Flash Fill and paste from text.
//! Each verb calls the engine method of the same name, so a press and an agent's call are one call.

use serde_json::json;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::clipboard::{
    AcrossSheets, CustomLists, ExternalPaste, FillKind, SeriesDate, SeriesKind, SeriesSpec,
};
use unluminous_sheet::edit::{PasteKind, PasteOperation, PasteSpecial, Shift};

use super::commands::{parse_range, take_range};
use super::verbs_a::{after, flag, Out};
use super::view_b::{After, AfterKind, PasteDraft};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// Words that only the round 2 `paste` takes; the old words of `paste` are left to the old code.
const NEW_PASTE_WORDS: [&str; 22] = [
    "special",
    "add",
    "subtract",
    "multiply",
    "divide",
    "skip-blanks",
    "comments",
    "notes",
    "validation",
    "widths",
    "column-widths",
    "number-formats",
    "link",
    "all-except-borders",
    "source-theme",
    "merge-conditional",
    "formulas-and-number-formats",
    "values-and-number-formats",
    "no-borders",
    "theme",
    "none",
    "insert",
];

/// Whether the words of a `paste` ask for something the old verb did not do.
pub fn is_new_paste(args: &[String]) -> bool {
    args.iter().any(|a| NEW_PASTE_WORDS.contains(&a.to_ascii_lowercase().as_str()))
}

/// What the words after a paste's range ask for.
#[derive(Debug, Default)]
pub struct PasteAsk {
    pub special: PasteSpecial,
    pub link: bool,
    pub asked_nothing: bool,
}

/// Read the words of `paste-special`: a kind, an operation, `skip-blanks`, `transpose` and `link`.
pub fn read_paste_words(words: &[String]) -> Result<PasteAsk, String> {
    let mut ask = PasteAsk { asked_nothing: true, ..PasteAsk::default() };
    let mut number_formats = false;
    for word in words {
        let w = word.to_ascii_lowercase();
        ask.asked_nothing = ask.asked_nothing && matches!(w.as_str(), "special" | "insert");
        match w.as_str() {
            "special" | "insert" => {}
            "link" => ask.link = true,
            "skip-blanks" | "skip" => ask.special.skip_blanks = true,
            "transpose" => ask.special.transpose = true,
            "number-formats" => number_formats = true,
            other => {
                if let Some(operation) = PasteOperation::parse(other) {
                    ask.special.operation = operation;
                } else if let Some(kind) = PasteKind::parse(other) {
                    ask.special.kind = kind;
                } else {
                    return Err(format!(
                        "paste-special takes a kind (all, values, formulas, formats, comments, validation, widths, all-except-borders, source-theme, merge-conditional), an operation (add, subtract, multiply, divide), skip-blanks, transpose or link; `{other}` is none of those"
                    ));
                }
            }
        }
    }
    if number_formats {
        ask.special.kind = if ask.special.kind == PasteKind::Values {
            PasteKind::ValuesAndNumberFormats
        } else {
            PasteKind::FormulasAndNumberFormats
        };
    }
    Ok(ask)
}

/// The words of a paste after the range it was aimed at, which is chosen first.
fn paste_args<'a>(sheet: &mut Spreadsheet, args: &'a [String]) -> Result<&'a [String], String> {
    let book = sheet.current_book()?;
    Ok(take_range(book, args))
}

/// `paste-special [range] [kind] [operation] [skip-blanks] [transpose] [link]`, or the popup with no words.
pub fn paste_special(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let rest = paste_args(sheet, args)?;
    let ask = read_paste_words(rest)?;
    if ask.asked_nothing {
        let book = sheet.current_book()?;
        book.view.lane_b.paste_draft = Some(PasteDraft::default());
        return Ok(Answer::said("Paste Special"));
    }
    sheet.current_book()?.view.lane_b.paste_draft = None;
    if ask.link {
        return paste_link(sheet, &[]);
    }
    paste_with(sheet, ask.special)
}

/// Paste what was copied, the way the options say, into the cells chosen.
pub fn paste_with(sheet: &mut Spreadsheet, options: PasteSpecial) -> Out {
    let from = sheet.clip.clone().ok_or("Nothing has been copied.")?;
    let book = sheet.current_book()?;
    if from.key != book.key && from.clip.cut {
        return Err("Cells cut in one workbook are pasted by copying them instead.".to_owned());
    }
    let (index, dest) = (book.view.sheet, book.view.range());
    let pasted = book.session.paste_special(&from.clip, index, dest, &options)?;
    book.view.select_range(pasted);
    book.view.marquee = None;
    book.changed();
    book.view.axes = None;
    let revision = book.revision;
    book.view.lane_b.after = Some(After {
        kind: AfterKind::Paste,
        sheet: index,
        target: pasted,
        dest,
        source: from.clip.range,
        clip: Some(from.clip.clone()),
        revision,
        open: false,
    });
    if from.clip.cut {
        sheet.clip = None;
    }
    Ok(Answer::said(format!("Pasted into {}", pasted.a1())))
}

/// `paste-link`: formulas that name the copied cells.
pub fn paste_link(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let _ = paste_args(sheet, args)?;
    let from = sheet.clip.clone().ok_or("Nothing has been copied.")?;
    let book = sheet.current_book()?;
    let (index, at) = (book.view.sheet, book.view.range().first);
    let made = book.session.paste_link(&from.clip, index, at)?;
    book.view.select_range(made);
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Linked {}", made.a1())))
}

/// `insert-copied [cell] down|right`, and `insert-cut`: the copied or cut cells go in and the cells there
/// move over to make room.
pub fn insert_copied(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let rest = paste_args(sheet, args)?;
    let shift = match rest.first().map(String::as_str) {
        Some("right") | Some("shift-right") => Shift::Right,
        _ => Shift::Down,
    };
    let from = sheet.clip.clone().ok_or("Nothing has been copied.")?;
    let book = sheet.current_book()?;
    let (index, at) = (book.view.sheet, book.view.range().first);
    let made = book.session.insert_copied(&from.clip, index, at, shift)?;
    book.view.select_range(made);
    book.view.marquee = None;
    book.changed();
    book.view.axes = None;
    if from.clip.cut {
        sheet.clip = None;
    }
    Ok(Answer::said(format!("Inserted {}", made.a1())))
}

/// `paste-options <how>`: change how the last paste was done, as the Paste Options button does.
pub fn paste_options(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let after = book
        .view
        .lane_b
        .after
        .clone()
        .filter(|a| a.kind == AfterKind::Paste && a.revision == book.revision)
        .ok_or("The Paste Options button is gone: the workbook changed after the paste.")?;
    let ask = read_paste_words(args)?;
    book.session.undo();
    let clip = after.clip.clone().ok_or("The paste has nothing to paste again.")?;
    let made = if ask.link {
        book.session.paste_link(&clip, after.sheet, after.dest.first)
    } else {
        book.session.paste_special(&clip, after.sheet, after.dest, &ask.special)
    }?;
    book.view.select_range(made);
    book.changed();
    book.view.axes = None;
    let revision = book.revision;
    book.view.lane_b.after = Some(After { target: made, revision, open: false, ..after });
    Ok(Answer::said(format!("Pasted into {}", made.a1())))
}

/// `paste-text [cell] [formulas] <text>`, and `paste-html`: what another program put on the clipboard.
pub fn paste_external(sheet: &mut Spreadsheet, args: &[String], html: bool) -> Out {
    let book = sheet.current_book()?;
    let (at, rest) = super::verbs_a::take_cell(book, args);
    let formulas = flag(rest, "formulas");
    let text: Vec<&str> =
        rest.iter().filter(|w| !w.eq_ignore_ascii_case("formulas")).map(|s| s.as_str()).collect();
    let text = text.join(" ").replace("\\t", "\t").replace("\\n", "\n");
    let index = book.view.sheet;
    let options = ExternalPaste { formulas };
    let made = if html {
        book.session.paste_html(index, at, &text, options)?
    } else {
        book.session.paste_text(index, at, &text, options)?
    };
    book.view.select_range(made);
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Pasted into {}", made.a1())))
}

// ------------------------------------------------------------------------------------------------ fill

/// The fill lists: Excel's own and the ones added with `custom-list`.
fn lists(sheet: &Spreadsheet) -> CustomLists {
    let mut lists = CustomLists::excel_defaults();
    for list in &sheet.config.custom_lists {
        let items: Vec<String> = list.split(',').map(|s| s.trim().to_owned()).collect();
        let _ = lists.add(items);
    }
    lists
}

/// The kind of fill a word of the fill handle's menu names.
fn fill_kind_of(word: &str) -> Option<FillKind> {
    Some(match word {
        "default" => FillKind::Default,
        "copy" => FillKind::Copy,
        "series" => FillKind::Series,
        "formats" => FillKind::Formats,
        "values" => FillKind::Values,
        "days" => FillKind::Days,
        "weekdays" => FillKind::Weekdays,
        "months" => FillKind::Months,
        "years" => FillKind::Years,
        "linear" => FillKind::LinearTrend,
        "growth" => FillKind::GrowthTrend,
        _ => return None,
    })
}

/// `fill-handle <range> copy|series|formats|values|days|weekdays|months|years|linear|growth|flash [control]`.
pub fn fill_handle(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let target = args
        .first()
        .and_then(|w| Range::parse(w))
        .ok_or("fill-handle needs the range to fill, such as A1:A10")?;
    let how = args.get(1).map(String::as_str).unwrap_or("series");
    let control = flag(args, "control");
    let lists = lists(sheet);
    let book = sheet.current_book()?;
    let (index, source) = (book.view.sheet, book.view.range());
    if how == "flash" {
        let made = book.session.flash_fill(index, target)?;
        book.view.select_range(target);
        book.changed();
        book.view.axes = None;
        return Ok(Answer::said(format!("Flash Fill wrote {}", made.a1())));
    }
    let kind = fill_kind_of(how)
        .ok_or_else(|| format!("fill-handle takes copy, series, formats, values, days, weekdays, months, years, linear, growth or flash; got `{how}`"))?;
    if control {
        book.session.fill_series_with_control(index, source, target, &lists)?;
    } else {
        book.session.fill_series_with(index, source, target, kind, &lists)?;
    }
    finish_fill(book, index, source, target);
    Ok(Answer::said(format!("Filled {}", target.a1())))
}

/// Note the fill, so the Auto Fill Options button is drawn beside it.
fn finish_fill(book: &mut Book, index: usize, source: Range, target: Range) {
    book.view.select_range(target);
    book.changed();
    book.view.axes = None;
    let revision = book.revision;
    book.view.lane_b.after = Some(After {
        kind: AfterKind::Fill,
        sheet: index,
        target,
        dest: target,
        source,
        clip: None,
        revision,
        open: false,
    });
}

/// `fill-options <kind>`: fill the same cells again another way, as the Auto Fill Options button does.
pub fn fill_options(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let lists = lists(sheet);
    let book = sheet.current_book()?;
    let state = book
        .view
        .lane_b
        .after
        .clone()
        .filter(|a| a.kind == AfterKind::Fill && a.revision == book.revision)
        .ok_or("The Auto Fill Options button is gone: the workbook changed after the fill.")?;
    let word = args.first().map(String::as_str).unwrap_or("series");
    let kind = fill_kind_of(word)
        .ok_or_else(|| format!("fill-options takes a kind of fill; got `{word}`"))?;
    book.session.undo();
    book.session.fill_series_with(state.sheet, state.source, state.target, kind, &lists)?;
    finish_fill(book, state.sheet, state.source, state.target);
    Ok(Answer::said(format!("Filled {}", state.target.a1())))
}

/// The date unit a word names.
fn date_unit(word: &str) -> Option<SeriesDate> {
    Some(match word {
        "day" | "days" => SeriesDate::Day,
        "weekday" | "weekdays" => SeriesDate::Weekday,
        "month" | "months" => SeriesDate::Month,
        "year" | "years" => SeriesDate::Year,
        _ => return None,
    })
}

/// Read the Series form's words: `rows|columns linear|growth|date|autofill [unit] [step n] [stop n] [trend]`.
fn read_series(args: &[String]) -> Result<SeriesSpec, String> {
    let mut spec = SeriesSpec::linear(1.0);
    for word in args {
        let w = word.to_ascii_lowercase();
        match w.as_str() {
            "rows" => spec.by_rows = true,
            "columns" => spec.by_rows = false,
            "linear" => spec.kind = SeriesKind::Linear,
            "growth" => spec.kind = SeriesKind::Growth,
            "autofill" => spec.kind = SeriesKind::AutoFill,
            "date" => {
                if !matches!(spec.kind, SeriesKind::Date(_)) {
                    spec.kind = SeriesKind::Date(SeriesDate::Day);
                }
            }
            "trend" => spec.trend = true,
            other => {
                if let Some(unit) = date_unit(other) {
                    spec.kind = SeriesKind::Date(unit);
                }
            }
        }
    }
    if let Some(step) = after(args, "step") {
        spec.step = step.parse().map_err(|_| format!("`{step}` is not a number for the step"))?;
    }
    if let Some(stop) = after(args, "stop") {
        spec.stop = Some(stop.parse().map_err(|_| format!("`{stop}` is not a number to stop at"))?);
    }
    Ok(spec)
}

/// `series [range] ...`: the Series form.
pub fn series(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let rest = take_range(book, args);
    let spec = read_series(rest)?;
    let (index, range) = (book.view.sheet, book.view.range());
    book.session.data_series(index, range, &spec)?;
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Series in {}", range.a1())))
}

/// `justify [range]`: Fill Justify.
pub fn justify(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    take_range(book, args);
    let (index, range) = (book.view.sheet, book.view.range());
    let made = book.session.justify(index, range)?;
    book.view.select_range(made);
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Justified into {}", made.a1())))
}

/// `fill-across <sheets> [all|contents|formats]`: Fill Across Worksheets.
pub fn fill_across(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    let (range_words, rest) = match args.first() {
        Some(first) if parse_range(book, first).is_some() => (&args[..1], &args[1..]),
        _ => (&args[..0], args),
    };
    take_range(book, range_words);
    let names =
        rest.first().ok_or("fill-across needs the sheets to fill, such as Sheet2,Sheet3")?;
    let mut others = Vec::new();
    for name in names.split(',') {
        let index = book
            .session
            .book
            .sheet_index(name.trim())
            .ok_or_else(|| format!("there is no sheet named `{name}`"))?;
        others.push(index);
    }
    let what = if flag(rest, "contents") {
        AcrossSheets::Contents
    } else if flag(rest, "formats") {
        AcrossSheets::Formats
    } else {
        AcrossSheets::All
    };
    let (index, range) = (book.view.sheet, book.view.range());
    book.session.fill_across_sheets(index, range, &others, what)?;
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Filled {} on {} sheets", range.a1(), others.len())))
}

/// `flash-fill [range]`: fill the column from the pattern of the examples typed beside it.
pub fn flash_fill(sheet: &mut Spreadsheet, args: &[String]) -> Out {
    let book = sheet.current_book()?;
    take_range(book, args);
    let (index, range) = (book.view.sheet, book.view.range());
    let made = if range.cols() == 1 && range.rows() >= 2 {
        book.session.flash_fill(index, range)?
    } else {
        book.session.flash_fill_down(index, book.view.cursor)?
    };
    book.view.select_range(made);
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(format!("Flash Fill wrote {}", made.a1())).with(json!({ "range": made.a1() })))
}

/// The cell a word names, for a verb that takes one.
pub fn cell_in(book: &Book, word: &str) -> Option<CellRef> {
    parse_range(book, word).map(|(_, r)| r.first)
}
