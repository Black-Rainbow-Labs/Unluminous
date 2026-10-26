//! The verbs for the clipboard and fill, page setup and print, pivot tables, slicers and file formats
//! (`task-2360`, the `wiringb` lane). [`super::verbs::run`] hands any verb it does not know to [`run`] here
//! after the `wiringa` verbs, and [`first`] is asked before [`super::commands::run`] so a verb that already
//! existed (`paste`, `fill`, `fill-handle`, `insert`, `save`, `save-as`) can take the words round 2 added.

use unluminous_sheet::coord::Range;

use super::verbs_a::Out;
use super::verbs_b_clip as clip;
use super::{verbs_b_files, verbs_b_page, verbs_b_pivot, Spreadsheet};

/// Run a verb this lane owns, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    let own = match verb {
        "paste-special" => Some(clip::paste_special(sheet, args)),
        "paste-link" => Some(clip::paste_link(sheet, args)),
        "insert-copied" | "insert-cut" => Some(clip::insert_copied(sheet, args)),
        "paste-options" => Some(clip::paste_options(sheet, args)),
        "fill-options" => Some(clip::fill_options(sheet, args)),
        "paste-text" => Some(clip::paste_external(sheet, args, false)),
        "paste-html" => Some(clip::paste_external(sheet, args, true)),
        "series" => Some(clip::series(sheet, args)),
        "justify" => Some(clip::justify(sheet, args)),
        "fill-across" => Some(clip::fill_across(sheet, args)),
        "flash-fill" => Some(clip::flash_fill(sheet, args)),
        _ => None,
    };
    own.or_else(|| verbs_b_page::run(sheet, verb, args))
        .or_else(|| verbs_b_pivot::run(sheet, verb, args))
        .or_else(|| verbs_b_files::run(sheet, verb, args))
}

/// Whether a word looks like a range or a cell rather than a keyword.
fn looks_like_range(word: &str) -> bool {
    word.contains(':') || word.chars().any(|c| c.is_ascii_digit()) && Range::parse(word).is_some()
}

/// Verbs that existed before round 2 and now take more words. Answers `None` when the words are the old ones.
pub fn first(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "paste" if clip::is_new_paste(args) => Some(clip::paste_special(sheet, args)),
        "insert" if matches!(args.first().map(String::as_str), Some("copied") | Some("cut")) => {
            Some(clip::insert_copied(sheet, &args[1..]))
        }
        "fill" => fill_more(sheet, args),
        "fill-handle" => Some(clip::fill_handle(sheet, args)),
        "save" | "save-as" => verbs_b_files::first(sheet, verb, args),
        _ => None,
    }
}

/// `fill series linear|growth|date ...`, `fill justify` and `fill across`.
fn fill_more(sheet: &mut Spreadsheet, args: &[String]) -> Option<Out> {
    match args.first().map(String::as_str) {
        Some("series") if !args.get(1).is_some_and(|w| looks_like_range(w)) => {
            Some(clip::series(sheet, &args[1..]))
        }
        Some("justify") => Some(clip::justify(sheet, &args[1..])),
        Some("across") => Some(clip::fill_across(sheet, &args[1..])),
        _ => None,
    }
}
