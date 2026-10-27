//! The caret in a list paragraph (`task-2383`): the label and the tab after it hold no character, so the
//! caret before the first letter stands at that letter, not at the bullet.

use super::{document, laid, lines_of, para, para_with, small_page};
use crate::builtin::{new_list, ListKind};
use crate::layout::Item;

/// A one paragraph document whose paragraph is in a new list of `kind`, reading `text`.
fn listed(kind: ListKind, text: &str) -> crate::model::Document {
    let mut out = document(&para("x"), &small_page());
    let num = new_list(&mut out, kind);
    let numbered = format!(
        "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{num}\"/></w:numPr><w:ind w:left=\"720\" w:hanging=\"360\"/>"
    );
    out.body = document(&para_with(&numbered, text), &small_page()).body;
    out
}

/// The x of the first letter of `text` drawn on the first page.
fn x_of_letter(pages: &crate::layout::Pages, text: &str) -> f32 {
    pages.pages[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Text(t) if t.text.starts_with(text) => t.xs.first().copied(),
            _ => None,
        })
        .expect("the paragraph's text is drawn")
}

#[test]
fn the_caret_before_the_first_letter_of_a_bullet_stands_at_the_letter() {
    let pages = laid(&listed(ListKind::Bullet, "Apple"));
    let line = lines_of(&pages, &[0])[0].1;
    let letter = x_of_letter(&pages, "Apple");
    assert_eq!(line.stops[0].0, 0, "the first stop is offset 0: {:?}", line.stops);
    assert!(
        (line.x_of(0) - letter).abs() < 0.01,
        "offset 0 is drawn at {} and the first letter at {letter}: {:?}",
        line.x_of(0),
        line.stops
    );
}

#[test]
fn a_press_on_the_left_half_of_the_first_letter_puts_the_caret_before_it() {
    let pages = laid(&listed(ListKind::Number, "Apple"));
    let line = lines_of(&pages, &[0])[0].1;
    let letter = x_of_letter(&pages, "Apple");
    assert_eq!(line.offset_at(letter + 1.0), 0);
    assert_eq!(line.offset_at(letter + 5.0), 1);
}

#[test]
fn an_empty_bullet_puts_its_caret_where_the_text_will_start() {
    let pages = laid(&listed(ListKind::Bullet, ""));
    let line = lines_of(&pages, &[0])[0].1;
    // The text starts at the left indent, 720 twips past the 20 point margin.
    assert!((line.x_of(0) - 56.0).abs() < 0.01, "{:?}", line.stops);
}
