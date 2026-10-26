//! The space between paragraphs: the larger of the two, none between paragraphs of one style that ask
//! for contextual spacing, their sum when the document asks for it, and none at the top of a page
//! (spacing-sum and paragraphs fixtures).

use super::*;
use crate::xml::{El, Node};

/// A one line paragraph with the given space before and after (twips) and `extra` properties.
fn spaced(before: u32, after: u32, extra: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr>{extra}<w:spacing w:before=\"{before}\" w:after=\"{after}\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

#[test]
fn paragraphs_docx_the_space_between_two_paragraphs_is_the_larger_of_the_two() {
    // 8 points after the first and 12 before the second: 12 between them, so the second starts at
    // 12 + 12 = 24, not at 12 + 20.
    let pages = laid_small(&format!("{}{}", spaced(0, 160, "", "a"), spaced(240, 0, "", "b")));
    assert_eq!(top_of(&pages, 1), 24.0);
}

#[test]
fn paragraphs_docx_the_space_after_wins_when_it_is_larger() {
    let pages = laid_small(&format!("{}{}", spaced(0, 400, "", "a"), spaced(100, 0, "", "b")));
    assert_eq!(top_of(&pages, 1), 12.0 + 20.0);
}

#[test]
fn spacing_sum_docx_the_two_are_added_when_the_document_turns_html_spacing_off() {
    // `doNotUseHTMLParagraphAutoSpacing` adds 8 and 12: the second paragraph starts at 12 + 20 = 32.
    let body = format!("{}{}", spaced(0, 160, "", "a"), spaced(240, 0, "", "b"));
    let mut doc = document(&body, &small_page());
    let compat = doc.settings.as_mut().unwrap().root.child_mut("w:compat").expect("compat");
    compat.children.push(Node::El(El::new("w:doNotUseHTMLParagraphAutoSpacing")));
    let pages = laid(&doc);
    assert_eq!(top_of(&pages, 1), 32.0);
}

#[test]
fn paragraphs_docx_contextual_spacing_removes_the_space_between_paragraphs_of_one_style() {
    // Both paragraphs are Normal and both ask for contextual spacing.
    let context = "<w:contextualSpacing/>";
    let body = format!("{}{}", spaced(0, 400, context, "a"), spaced(400, 0, context, "b"));
    let pages = laid_small(&body);
    assert_eq!(top_of(&pages, 1), 12.0);
}

#[test]
fn paragraphs_docx_contextual_spacing_keeps_the_space_between_paragraphs_of_different_styles() {
    let context = "<w:contextualSpacing/>";
    let other = "<w:pStyle w:val=\"Heading2\"/><w:contextualSpacing/>";
    let body = format!("{}{}", spaced(0, 400, context, "a"), spaced(400, 0, other, "b"));
    let pages = laid_small(&body);
    assert_eq!(top_of(&pages, 1), 12.0 + 20.0);
}

#[test]
fn paragraphs_docx_space_before_is_dropped_at_the_top_of_a_page_after_the_first() {
    // The first paragraph of the document keeps its 20 points before it. Ten lines fill page 1, and the
    // eleventh paragraph, with 20 points before it, is the first thing on page 2 and starts at 0.
    let pages = laid_small(&format!("{}{}", spaced(400, 0, "", "first"), paras("P", 4)));
    assert_eq!(top_of(&pages, 0), 20.0);
    let pages = laid_small(&format!("{}{}", paras("P", 10), spaced(400, 0, "", "late")));
    assert_eq!(page_of(&pages, 10), 1);
    assert_eq!(top_of(&pages, 10), 0.0);
}
