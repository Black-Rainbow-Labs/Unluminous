//! Automatic hyphenation and its zone (hyphenation.docx).
//!
//! The text is a 16 letter word and a space (17 clusters, 102 points) and then "international". With six
//! points a letter, the word needs 78 points and only 58 are left on the 160 point line.

use super::*;
use crate::xml::{El, Node};

const TEXT: &str = "abcdefghijklmnop international";

/// A document with automatic hyphenation on. `mode` and `zone` (twips) set the compatibility mode and the
/// hyphenation zone when given; `extra` adds more settings.
fn hyphenating(text: &str, mode: Option<u32>, zone: Option<u32>, extra: &[El]) -> Pages {
    let mut doc = document(&para(text), &small_page());
    let root = &mut doc.settings.as_mut().expect("settings").root;
    root.children.push(Node::El(El::new("w:autoHyphenation")));
    if let Some(zone) = zone {
        root.children.push(Node::El(El::new("w:hyphenationZone").with("w:val", zone.to_string())));
    }
    for setting in extra {
        root.children.push(Node::El(setting.clone()));
    }
    if let Some(mode) = mode {
        let compat = root.child_mut("w:compat").expect("compat settings");
        for setting in compat.elements_mut() {
            if setting.attr("w:name") == Some("compatibilityMode") {
                setting.set_attr("w:val", mode.to_string());
            }
        }
    }
    laid(&doc)
}

/// Where each line of paragraph 0 ends, in characters.
fn ends(pages: &Pages) -> Vec<usize> {
    lines_of(pages, &[0]).into_iter().map(|(_, l)| l.end).collect()
}

#[test]
fn hyphenation_docx_without_automatic_hyphenation_the_long_word_moves_to_the_next_line() {
    let pages = laid_small(&para(TEXT));
    assert_eq!(ends(&pages)[0], 17, "the first line holds the 16 letters and the space");
    assert_eq!(lines_of(&pages, &[0]).len(), 2);
}

#[test]
fn hyphenation_docx_a_word_is_broken_where_a_part_of_it_and_a_hyphen_fit() {
    // Mode 15 has no zone, so any gap is used: 58 points hold nine letters, one of which is the
    // hyphen's room, so the line takes the longest break of "international" that leaves room for it.
    let pages = hyphenating(TEXT, None, None, &[]);
    let first = ends(&pages)[0];
    assert!(first > 17 && first <= 17 + 8, "the line took part of the word: ends at {first}");
    assert_eq!(lines_of(&pages, &[0]).len(), 2);
    let drawn: String = texts(&pages.pages[0]).iter().map(|t| t.text.as_str()).collect();
    assert!(drawn.contains('-'), "a hyphen is drawn after the first part: {drawn:?}");
}

#[test]
fn hyphenation_docx_an_older_document_does_not_hyphenate_inside_its_zone() {
    // Compatibility mode 14 keeps the zone. The gap left on the first line is 58 points; a zone of 60
    // points (1200 twips) is wider than that gap, so the word moves down whole.
    let pages = hyphenating(TEXT, Some(14), Some(1200), &[]);
    assert_eq!(ends(&pages)[0], 17);
}

#[test]
fn hyphenation_docx_an_older_document_hyphenates_when_the_gap_is_wider_than_its_zone() {
    // The same text with a zone of 18 points (360 twips): the 58 point gap is wider, so the word is broken.
    let pages = hyphenating(TEXT, Some(14), Some(360), &[]);
    assert!(ends(&pages)[0] > 17);
}

#[test]
fn hyphenation_docx_words_in_capitals_are_left_whole_when_the_document_says_so() {
    let caps = "ABCDEFGHIJKLMNOP INTERNATIONAL";
    let left_whole = hyphenating(caps, None, None, &[El::new("w:doNotHyphenateCaps")]);
    assert_eq!(ends(&left_whole)[0], 17);
    let hyphenated = hyphenating(caps, None, None, &[]);
    assert!(ends(&hyphenated)[0] > 17);
}

#[test]
fn hyphenation_docx_a_paragraph_that_suppresses_hyphens_is_not_hyphenated() {
    let mut doc = document(&para_with("<w:suppressAutoHyphens/>", TEXT), &small_page());
    doc.settings.as_mut().unwrap().root.children.push(Node::El(El::new("w:autoHyphenation")));
    let pages = laid(&doc);
    assert_eq!(ends(&pages)[0], 17);
}
