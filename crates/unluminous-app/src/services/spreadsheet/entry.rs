//! What typing into a cell does besides putting the text there, as Excel's options do: AutoComplete from
//! the entries already in the column, AutoCorrect of common slips, and a web or mail address becoming a
//! link. Pure functions over the sheet, so a test needs no window.

use unluminous_sheet::coord::{CellRef, MAX_ROWS};
use unluminous_sheet::model::LinkTarget;
use unluminous_sheet::{Sheet, Value};

/// The text entries of the block of the column `at` is in: the cells above and below it up to the first
/// empty one, which is the list AutoComplete and Alt+Down offer.
pub fn column_entries(sheet: &Sheet, at: CellRef) -> Vec<String> {
    let text = |row: u32| match sheet.value_at(CellRef::new(row, at.col)) {
        Value::Text(t) if !t.is_empty() => Some(t.to_string()),
        Value::Empty => None,
        _ => Some(String::new()),
    };
    let mut entries: Vec<String> = Vec::new();
    let mut row = at.row;
    while row > 0 {
        row -= 1;
        match text(row) {
            Some(t) => entries.push(t),
            None => break,
        }
    }
    let mut row = at.row + 1;
    while row < MAX_ROWS {
        match text(row) {
            Some(t) => entries.push(t),
            None => break,
        }
        row += 1;
    }
    entries.retain(|t| !t.is_empty());
    entries.sort_by_key(|t| t.to_lowercase());
    entries.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    entries
}

/// What AutoComplete makes of what was typed: the one entry of the column that starts with it, when
/// exactly one does and it is longer. Numbers and formulas are never completed.
pub fn complete(sheet: &Sheet, at: CellRef, typed: &str) -> Option<String> {
    if typed.is_empty() || typed.starts_with('=') || typed.trim().parse::<f64>().is_ok() {
        return None;
    }
    let lower = typed.to_lowercase();
    let mut found = column_entries(sheet, at)
        .into_iter()
        .filter(|entry| entry.to_lowercase().starts_with(&lower) && entry.len() > typed.len());
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// Excel's own AutoCorrect replacements that apply to a word typed in a cell.
const REPLACEMENTS: &[(&str, &str)] = &[
    ("(c)", "\u{a9}"),
    ("(r)", "\u{ae}"),
    ("(tm)", "\u{2122}"),
    ("abbout", "about"),
    ("abotu", "about"),
    ("accross", "across"),
    ("acheive", "achieve"),
    ("adn", "and"),
    ("alot", "a lot"),
    ("beleive", "believe"),
    ("becuase", "because"),
    ("comming", "coming"),
    ("definately", "definitely"),
    ("doesnt", "doesn't"),
    ("dont", "don't"),
    ("hte", "the"),
    ("i", "I"),
    ("occured", "occurred"),
    ("recieve", "receive"),
    ("seperate", "separate"),
    ("teh", "the"),
    ("thier", "their"),
    ("tommorow", "tomorrow"),
    ("untill", "until"),
    ("wich", "which"),
    ("wiht", "with"),
];

/// Text after AutoCorrect: two initial capitals become one (`THe` to `The`) and the words on Excel's list
/// are replaced. A formula is left as it is.
pub fn autocorrect(text: &str) -> String {
    if text.starts_with('=') || text.starts_with('\'') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(&correct_word(word));
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || matches!(c, '(' | ')' | '\'') {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn correct_word(word: &str) -> String {
    if let Some((_, with)) = REPLACEMENTS.iter().find(|(from, _)| *from == word) {
        return (*with).to_owned();
    }
    let chars: Vec<char> = word.chars().collect();
    let two_capitals = chars.len() > 2
        && chars[0].is_uppercase()
        && chars[1].is_uppercase()
        && chars[2..].iter().all(|c| c.is_lowercase());
    if two_capitals {
        let mut fixed = String::new();
        fixed.push(chars[0]);
        fixed.extend(chars[1].to_lowercase());
        fixed.extend(&chars[2..]);
        return fixed;
    }
    word.to_owned()
}

/// The link a typed web or mail address makes, as Excel's "Internet and network paths with hyperlinks".
pub fn typed_link(text: &str) -> Option<LinkTarget> {
    let t = text.trim();
    if t.contains(char::is_whitespace) || t.starts_with('=') {
        return None;
    }
    let lower = t.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("ftp://")
    {
        return Some(LinkTarget::External(t.to_owned()));
    }
    if lower.starts_with("www.") && t.len() > 4 {
        return Some(LinkTarget::External(format!("http://{t}")));
    }
    if lower.starts_with("mailto:") {
        return Some(LinkTarget::External(t.to_owned()));
    }
    let (user, domain) = t.split_once('@')?;
    let mail = !user.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.');
    mail.then(|| LinkTarget::External(format!("mailto:{t}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unluminous_sheet::edit::Session;
    use unluminous_sheet::Workbook;

    #[test]
    fn autocomplete_finishes_the_one_entry_that_starts_with_what_was_typed() {
        let mut s = Session::new(Workbook::new(), 0.0);
        for (row, text) in ["Apple", "Banana", "Apricot", "Cherry"].iter().enumerate() {
            s.set_input(0, CellRef::new(row as u32, 0), text).unwrap();
        }
        let sheet = &s.book.sheets[0];
        let at = CellRef::new(4, 0);
        assert_eq!(complete(sheet, at, "ba").as_deref(), Some("Banana"));
        assert_eq!(complete(sheet, at, "ap"), None, "two entries start with ap");
        assert_eq!(complete(sheet, at, "apr").as_deref(), Some("Apricot"));
        assert_eq!(complete(sheet, at, "12"), None);
        assert_eq!(column_entries(sheet, at), vec!["Apple", "Apricot", "Banana", "Cherry"]);
    }

    #[test]
    fn autocorrect_fixes_two_capitals_and_the_common_slips() {
        assert_eq!(autocorrect("THe cat adn teh dog (c)"), "The cat and the dog \u{a9}");
        assert_eq!(autocorrect("=TEH()"), "=TEH()");
        assert_eq!(autocorrect("USA"), "USA");
    }

    #[test]
    fn a_typed_address_becomes_a_link() {
        assert_eq!(
            typed_link("www.example.com"),
            Some(LinkTarget::External("http://www.example.com".into()))
        );
        assert_eq!(
            typed_link("sam@example.com"),
            Some(LinkTarget::External("mailto:sam@example.com".into()))
        );
        assert_eq!(typed_link("not a link"), None);
        assert_eq!(typed_link("3.5"), None);
    }
}
