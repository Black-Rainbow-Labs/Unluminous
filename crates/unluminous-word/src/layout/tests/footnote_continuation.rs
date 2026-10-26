//! A footnote that does not fit at the foot of its page continues on the next (long-footnote.docx).
//!
//! On the small page a footnote's first line is 15.3 points (the reference mark is raised), its other
//! lines 12, the separator above the notes 12, and the paragraph holding the reference is 15.96 points tall.

use super::*;
use crate::layout::NotePiece;
use crate::position::Pos;
use crate::session::Session;

/// `count` plain paragraphs on the small page with a footnote of `lines` lines referenced at the end of
/// paragraph `at`.
fn with_note(count: usize, at: usize, lines: usize) -> Pages {
    let mut session = Session::new(document(&paras("P", count), &small_page()));
    let offset = format!("P{at}").len();
    session.select(Pos::new(vec![at as u32], offset), Pos::new(vec![at as u32], offset));
    session.insert_note(false, &words(lines));
    laid(&session.document)
}

fn piece(id: i64, from: usize, to: usize, continued: bool) -> NotePiece {
    NotePiece { id, from, to, continued }
}

/// The length of the first rule drawn above the notes on a page, which is the separator.
fn separator_length(pages: &Pages, page: usize) -> f32 {
    pages.pages[page]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Rule { x0, x1, .. } => Some(*x1 - *x0),
            _ => None,
        })
        .expect("a separator")
}

#[test]
fn long_footnote_docx_a_note_that_fits_stays_whole_on_the_page_of_its_reference() {
    // Paragraph 0 and a two line note: 16 + 12 + 27 points, well inside 120.
    let pages = with_note(3, 0, 2);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(pages.pages[0].notes, vec![piece(1, 0, 2, false)]);
}

#[test]
fn long_footnote_docx_a_note_continues_on_the_next_page_when_only_its_first_line_fits() {
    // Paragraph 6 is at 72 to 87.96. The whole six line note needs 12 + 15.3 + 60, but its first line
    // alone fits (87.96 + 12 + 15.3 = 115.26), so the paragraph stays and the note is cut after one line.
    let pages = with_note(12, 6, 6);
    assert_eq!(page_of(&pages, 6), 0);
    assert_eq!(pages.pages[0].notes, vec![piece(1, 0, 1, false)]);
    assert_eq!(
        pages.pages[1].notes,
        vec![piece(1, 1, 6, true)],
        "the other five lines come first on page 2"
    );
}

#[test]
fn long_footnote_docx_the_rest_of_a_note_takes_room_from_the_body_of_the_next_page() {
    // Page 2 holds the five carried lines (60) and the 12 point separator at its foot, leaving 48 points
    // for the body: paragraphs 7 to 10, and paragraph 11 goes to page 3.
    let pages = with_note(12, 6, 6);
    assert_eq!(page_of(&pages, 7), 1);
    assert_eq!(page_of(&pages, 10), 1);
    assert_eq!(page_of(&pages, 11), 2);
}

#[test]
fn long_footnote_docx_the_continuation_separator_runs_the_width_of_the_text() {
    // The ordinary separator is a 144 point rule; the one over the rest of a note that began on an
    // earlier page is as wide as the 160 point column.
    let pages = with_note(12, 6, 6);
    assert_eq!(separator_length(&pages, 0), 144.0);
    assert_eq!(separator_length(&pages, 1), 160.0);
}

#[test]
fn long_footnote_docx_a_reference_whose_note_cannot_start_on_the_page_moves_with_its_note() {
    // Paragraph 7 is at 84, and 84 + 15.96 + 12 + 15.3 is past 120: not even the first line of the note
    // fits, so the paragraph and its whole note go to page 2 and page 1 has no note.
    let pages = with_note(12, 7, 6);
    assert!(pages.pages[0].notes.is_empty());
    assert_eq!(page_of(&pages, 7), 1);
    assert_eq!(pages.pages[1].notes, vec![piece(1, 0, 6, false)]);
}
