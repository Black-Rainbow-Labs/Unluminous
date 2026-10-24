//! Tests for the edits to a sheet's page setup: each is one undo step, and what Excel refuses is refused here.

use super::*;
use crate::coord::{CellRef, Range};
use crate::edit::Session;
use crate::model::{Sheet, Workbook};

fn session(sheets: &[&str]) -> Session {
    let mut book = Workbook::new();
    book.sheets = sheets.iter().map(|n| Sheet::new(*n)).collect();
    Session::new(book, 0.0)
}

fn page(session: &Session, sheet: usize) -> PageSetup {
    session.book.sheets[sheet].page.clone()
}

#[test]
fn every_page_setup_change_is_one_undo_step() {
    let mut s = session(&["Sheet1"]);
    s.set_orientation(0, Orientation::Landscape).unwrap();
    s.set_paper_size(0, 9).unwrap();
    s.set_margins(0, Margins::narrow()).unwrap();
    s.set_scale(0, 80).unwrap();
    s.set_print_gridlines(0, true).unwrap();
    s.set_print_headings(0, true).unwrap();
    s.set_centered(0, true, true).unwrap();
    s.set_page_order(0, PageOrder::OverThenDown).unwrap();
    s.set_first_page_number(0, Some(7)).unwrap();
    s.set_print_comments(0, PrintComments::AtEnd).unwrap();
    s.set_print_errors(0, PrintErrors::Dash).unwrap();
    s.set_header_footer(0, HeaderFooterPart::OddFooter, "&P").unwrap();
    let done = page(&s, 0);
    assert_eq!((done.paper_size, done.scale, done.first_page_number), (9, 80, 7));
    for _ in 0..12 {
        assert!(s.undo().is_some());
    }
    assert_eq!(page(&s, 0), PageSetup::default());
    for _ in 0..12 {
        assert!(s.redo().is_some());
    }
    assert_eq!(page(&s, 0), done);
}

#[test]
fn a_scale_turns_fitting_off_and_a_fit_turns_it_on() {
    let mut s = session(&["Sheet1"]);
    s.set_fit_to_pages(0, 2, 0).unwrap();
    let fitted = page(&s, 0);
    assert!(fitted.fit_to_page && fitted.fit_to_width == 2 && fitted.fit_to_height == 0);
    s.set_scale(0, 60).unwrap();
    let scaled = page(&s, 0);
    assert!(!scaled.fit_to_page && scaled.scale == 60);
}

#[test]
fn values_outside_excels_limits_are_refused_and_change_nothing() {
    let mut s = session(&["Sheet1"]);
    assert!(s.set_scale(0, 9).is_err() && s.set_scale(0, 401).is_err());
    assert!(s.set_paper_size(0, 9999).is_err());
    assert!(s.set_margins(0, Margins { left: -1.0, ..Margins::default() }).is_err());
    assert!(s.set_margins(0, Margins { top: f64::NAN, ..Margins::default() }).is_err());
    assert!(s.set_first_page_number(0, Some(40_000)).is_err());
    assert!(s.set_header_footer(0, HeaderFooterPart::OddHeader, &"x".repeat(256)).is_err());
    assert!(s.set_print_titles(0, Some((5, 2)), None).is_err());
    assert!(s.set_orientation(3, Orientation::Landscape).is_err());
    assert!(!s.can_undo(), "a refused change records nothing");
}

#[test]
fn the_print_area_is_set_added_to_cleared_and_undone() {
    let mut s = session(&["Sheet1"]);
    s.set_print_area(0, &[Range::parse("A1:C10").unwrap()]).unwrap();
    s.add_to_print_area(0, Range::parse("E1:F5").unwrap()).unwrap();
    assert_eq!(print_area(&s.book, 0).len(), 2);
    s.clear_print_area(0).unwrap();
    assert!(print_area(&s.book, 0).is_empty());
    s.undo();
    assert_eq!(print_area(&s.book, 0).len(), 2);
    s.undo();
    assert_eq!(print_area(&s.book, 0), vec![Range::parse("A1:C10").unwrap()]);
    s.undo();
    assert!(print_area(&s.book, 0).is_empty());
}

#[test]
fn print_titles_are_set_and_cleared() {
    let mut s = session(&["Sheet1"]);
    s.set_print_titles(0, Some((0, 1)), Some((0, 0))).unwrap();
    assert_eq!(print_titles(&s.book, 0), PrintTitles { rows: Some((0, 1)), cols: Some((0, 0)) });
    s.set_print_titles(0, None, None).unwrap();
    assert_eq!(print_titles(&s.book, 0), PrintTitles::default());
    assert!(s.book.names.is_empty());
}

#[test]
fn page_breaks_are_inserted_at_a_cell_removed_and_reset() {
    let mut s = session(&["Sheet1"]);
    s.insert_page_break(0, CellRef::parse("C5").unwrap()).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![4]);
    assert_eq!(page(&s, 0).col_break_cols(), vec![2]);
    assert!(page(&s, 0).row_breaks[0].manual);
    s.insert_page_break(0, CellRef::parse("A9").unwrap()).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![4, 8]);
    assert_eq!(page(&s, 0).col_break_cols(), vec![2], "column A has nothing to break");
    s.insert_page_break(0, CellRef::parse("C5").unwrap()).unwrap();
    assert_eq!(page(&s, 0).row_breaks.len(), 2, "a break already there is not made twice");
    s.remove_page_break(0, CellRef::parse("C5").unwrap()).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![8]);
    assert!(page(&s, 0).col_breaks.is_empty());
    assert!(s.remove_page_break(0, CellRef::parse("C5").unwrap()).is_err());
    s.reset_page_breaks(0).unwrap();
    assert!(!page(&s, 0).has_manual_breaks());
    s.undo();
    assert!(page(&s, 0).has_manual_breaks());
    assert!(s.insert_page_break(0, CellRef::new(0, 0)).is_err());
}

#[test]
fn manual_breaks_move_with_the_rows_and_columns_around_them() {
    use crate::edit::Shift;
    let mut s = session(&["Sheet1"]);
    s.insert_page_break(0, CellRef::parse("D20").unwrap()).unwrap();
    s.insert_page_break(0, CellRef::parse("B40").unwrap()).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![19, 39]);
    // Two rows inserted above the first break push both breaks down two rows.
    s.insert(0, Range::parse("A5:A6").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![21, 41]);
    // A row inserted below a break leaves it where it was.
    s.insert(0, Range::parse("A100").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![21, 41]);
    // Deleting the row that starts a page takes the break away, and deleting above moves the next one up.
    s.delete(0, Range::parse("A22").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![40]);
    s.delete(0, Range::parse("A3:A4").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(page(&s, 0).row_break_rows(), vec![38]);
    // Columns the same way; cells shifted within a band do not move a break.
    assert_eq!(page(&s, 0).col_break_cols(), vec![1, 3]);
    s.insert(0, Range::parse("A1").unwrap(), Shift::WholeColumns).unwrap();
    assert_eq!(page(&s, 0).col_break_cols(), vec![2, 4]);
    s.insert(0, Range::parse("A1:A2").unwrap(), Shift::Down).unwrap();
    assert_eq!(page(&s, 0).col_break_cols(), vec![2, 4]);
    s.undo();
    s.undo();
    assert_eq!(page(&s, 0).col_break_cols(), vec![1, 3]);
}

#[test]
fn the_print_area_and_titles_follow_rows_inserted_above_them() {
    use crate::edit::Shift;
    let mut s = session(&["Sheet1"]);
    s.set_print_area(0, &[Range::parse("A10:D20").unwrap()]).unwrap();
    s.set_print_titles(0, Some((4, 5)), None).unwrap();
    s.insert(0, Range::parse("A1:A3").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(print_area(&s.book, 0), vec![Range::parse("A13:D23").unwrap()]);
    assert_eq!(print_titles(&s.book, 0).rows, Some((7, 8)));
}

#[test]
fn a_sheet_gets_the_changes_made_to_it_and_no_other() {
    let mut s = session(&["One", "Two"]);
    s.set_scale(1, 50).unwrap();
    assert_eq!(page(&s, 0).scale, 100);
    assert_eq!(page(&s, 1).scale, 50);
}

#[test]
fn the_whole_setup_can_be_replaced_at_once() {
    let mut s = session(&["Sheet1"]);
    let setup = PageSetup {
        paper_size: 8,
        options: PrintOptions { headings: true, ..PrintOptions::default() },
        ..PageSetup::default()
    };
    s.set_page_setup(0, setup.clone()).unwrap();
    assert_eq!(page(&s, 0), setup);
    assert_eq!(s.undo_label(), Some("Page setup"));
}
