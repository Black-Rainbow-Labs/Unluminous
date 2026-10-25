//! The verbs for data tools, outlines, rules, what if, protection, styles, themes and review (`task-2360`,
//! the `wiringa` lane). [`super::verbs::run`] hands any verb it does not know to [`run`] here, and
//! [`first`] is asked before [`super::commands::run`] so a verb that already existed (`find`, `note`,
//! `table`, `remove-duplicates`) can take the words round 2 added without changing its old words.

use unluminous_sheet::coord::{column_number, CellRef, Range};

use super::commands::{parse_range, take_range};
use super::{Book, Spreadsheet};
use crate::services::plugin_ui::Answer;

/// What a verb answers.
pub type Out = Result<Answer, String>;

/// Run a verb this lane owns, or answer `None` for one it does not.
pub fn run(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    super::verbs_a_data::run(sheet, verb, args)
        .or_else(|| super::verbs_a_outline::run(sheet, verb, args))
        .or_else(|| super::verbs_a_whatif::run(sheet, verb, args))
        .or_else(|| super::verbs_a_review::run(sheet, verb, args))
        .or_else(|| super::verbs_a_read::run(sheet, verb, args))
}

/// Verbs that existed before round 2 and now take more words. Answers `None` when the words are the old ones.
pub fn first(sheet: &mut Spreadsheet, verb: &str, args: &[String]) -> Option<Out> {
    match verb {
        "find" | "replace" => Some(super::verbs_a_review::find_or_replace(sheet, verb, args)),
        "note" => super::verbs_a_review::note_more(sheet, args),
        "table" => super::verbs_a_data::table_more(sheet, args),
        "remove-duplicates" => super::verbs_a_data::remove_duplicates_more(sheet, args),
        "conditional" => super::verbs_a_outline::conditional_more(sheet, args),
        _ => None,
    }
}

/// Run a change on the current workbook and note that it happened, so the grid draws it again.
pub fn edit(
    sheet: &mut Spreadsheet,
    change: impl FnOnce(&mut Book) -> Result<String, String>,
) -> Out {
    let book = sheet.current_book()?;
    let message = change(book)?;
    book.changed();
    book.view.axes = None;
    Ok(Answer::said(message))
}

/// Whether a bare word is among the words.
pub fn flag(args: &[String], word: &str) -> bool {
    args.iter().any(|a| a.eq_ignore_ascii_case(word))
}

/// The word after a keyword, as in `to A9` or `password secret`.
pub fn after<'a>(args: &'a [String], word: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a.eq_ignore_ascii_case(word))?;
    args.get(at + 1).map(String::as_str)
}

/// A switch word: `on`, `off`, or nothing for on.
pub fn on_off(word: Option<&str>) -> Result<bool, String> {
    super::verbs::switch_word(word.or(Some("on")), true)
}

/// A cell named by a word on the sheet showing, or the active cell for no word.
pub fn cell_or_active(book: &Book, word: Option<&str>) -> Result<CellRef, String> {
    match word {
        Some(w) => {
            parse_range(book, w).map(|(_, r)| r.first).ok_or_else(|| format!("`{w}` is not a cell"))
        }
        None => Ok(book.view.cursor),
    }
}

/// A cell word at the front of the words, taken off them; the active cell when the first word is not a cell.
pub fn take_cell<'a>(book: &Book, args: &'a [String]) -> (CellRef, &'a [String]) {
    if let Some(first) = args.first() {
        if let Some((_, r)) = parse_range(book, first) {
            if r.len() == 1 {
                return (r.first, &args[1..]);
            }
        }
    }
    (book.view.cursor, args)
}

/// A list of ranges on the sheet showing: `A1:B2,D4`.
pub fn ranges_on(book: &Book, word: &str) -> Result<Vec<Range>, String> {
    word.split(',')
        .map(|part| {
            parse_range(book, part)
                .map(|(_, r)| r)
                .ok_or_else(|| format!("`{part}` is not a range"))
        })
        .collect()
}

/// A column named by letters, as an index.
pub fn column_of(word: &str) -> Result<u32, String> {
    column_number(word).ok_or_else(|| format!("`{word}` is not a column"))
}

/// Choose the range a first word names and answer the range now chosen and the words after it.
pub fn chosen<'a>(book: &mut Book, args: &'a [String]) -> (Range, &'a [String]) {
    let rest = take_range(book, args);
    (book.view.range(), rest)
}

/// The words that follow `args[0]` when it is one of `keywords`.
pub fn keyword<'a>(args: &'a [String], keywords: &[&str]) -> Option<(&'a str, &'a [String])> {
    let first = args.first()?;
    keywords.iter().find(|k| first.eq_ignore_ascii_case(k)).map(|_| (first.as_str(), &args[1..]))
}

/// The author name comments and scenarios are made under.
pub fn author() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "User".to_owned())
}
