//! What the window's status bar says while a Word document is showing (`task-2366`).
//!
//! The Word tab used to draw a strip of its own under the pages with the page, the words, the language,
//! Track Changes, the three views and a zoom slider, which repeated the window's status bar right above it.
//! The facts moved here and the strip went: the views are the view chip in the format bar, and the zoom is
//! `Ctrl` with the wheel or with plus and minus, and `Document -> Zoom`.
//!
//! Each fact can be pressed, and a press runs a Word command by its words, which is the path
//! `plugins run word` takes.

use crate::services::plugin_ui::{StatusItem, TabStatus};

use super::Doc;

/// What the status bar says about `doc`.
pub fn of(doc: &Doc) -> TabStatus {
    let page = doc.pages.find(&doc.session.selection.head).map_or(1, |(index, _)| index + 1);
    let pages = doc.pages.pages.len().max(1);
    let stats = doc.session.stats(Some(&doc.pages));
    let words = match doc.session.selection.is_empty() {
        true => format!("{} {}", grouped(stats.words), plural(stats.words, "word")),
        false => {
            let selected = doc.session.selected_text().split_whitespace().count();
            format!("{} of {} words", grouped(selected), grouped(stats.words))
        }
    };
    let action = |id: &str| Some(vec!["action".to_owned(), id.to_owned()]);
    let mut items = vec![
        StatusItem {
            text: format!("Page {page} of {pages}"),
            name: "Status: page".to_owned(),
            press: action("go-to"),
        },
        StatusItem { text: words, name: "Status: words".to_owned(), press: action("word-count") },
    ];
    if doc.session.tracking {
        items.push(StatusItem {
            text: "Tracking changes".to_owned(),
            name: "Status: track changes".to_owned(),
            press: action("track-changes"),
        });
    }
    let errors = match doc.view.spelling {
        true => super::spelling::misspellings(&doc.session.document).len(),
        false => 0,
    };
    if errors > 0 {
        items.push(StatusItem {
            text: format!("{} {}", grouped(errors), plural(errors, "spelling error")),
            name: "Next spelling error (F7)".to_owned(),
            press: action("spelling"),
        });
    }
    TabStatus { kind: "Page document".to_owned(), items }
}

/// `noun`, or its plural when `count` is not one.
fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => noun.to_owned(),
        _ => format!("{noun}s"),
    }
}

/// A count with its thousands separated, `1,036`.
pub fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_grouped_in_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1036), "1,036");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
