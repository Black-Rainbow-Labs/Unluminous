//! Tests for the page setup model: the print area and print titles names, and the defaults.

use super::*;
use crate::coord::Range;
use crate::model::{Sheet, Workbook};

fn book_with(names: &[&str]) -> Workbook {
    let mut book = Workbook::new();
    book.sheets = names.iter().map(|n| Sheet::new(*n)).collect();
    book
}

#[test]
fn a_print_area_is_read_back_from_its_name() {
    let mut book = book_with(&["Print"]);
    set_print_area(&mut book, 0, &[Range::parse("A1:D50").unwrap(), Range::parse("F2").unwrap()]);
    assert_eq!(book.names[0].formula, "Print!$A$1:$D$50,Print!$F$2");
    assert_eq!(
        print_area(&book, 0),
        vec![Range::parse("A1:D50").unwrap(), Range::parse("F2").unwrap()]
    );
}

#[test]
fn a_sheet_name_that_needs_quotes_is_quoted_and_a_comma_inside_it_is_not_a_separator() {
    let mut book = book_with(&["Q3, East"]);
    set_print_area(&mut book, 0, &[Range::parse("B2:C3").unwrap(), Range::parse("E5:F6").unwrap()]);
    assert_eq!(book.names[0].formula, "'Q3, East'!$B$2:$C$3,'Q3, East'!$E$5:$F$6");
    assert_eq!(print_area(&book, 0).len(), 2);
}

#[test]
fn print_titles_name_columns_then_rows_the_way_excel_writes_them() {
    let mut book = book_with(&["Print"]);
    set_print_titles(&mut book, 0, PrintTitles { rows: Some((0, 1)), cols: Some((0, 0)) });
    assert_eq!(book.names[0].formula, "Print!$A:$A,Print!$1:$2");
    assert_eq!(print_titles(&book, 0), PrintTitles { rows: Some((0, 1)), cols: Some((0, 0)) });
}

#[test]
fn clearing_the_print_area_takes_its_name_away() {
    let mut book = book_with(&["Print"]);
    set_print_area(&mut book, 0, &[Range::parse("A1:B2").unwrap()]);
    set_print_area(&mut book, 0, &[]);
    assert!(book.names.is_empty());
    assert!(print_area(&book, 0).is_empty());
}

#[test]
fn a_name_belongs_to_its_own_sheet() {
    let mut book = book_with(&["One", "Two"]);
    set_print_area(&mut book, 1, &[Range::parse("A1:A9").unwrap()]);
    assert!(print_area(&book, 0).is_empty());
    assert_eq!(print_area(&book, 1).len(), 1);
}

#[test]
fn defaults_are_excels_defaults() {
    let page = PageSetup::default();
    assert_eq!(
        page.margins,
        Margins { left: 0.7, right: 0.7, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 }
    );
    assert_eq!(
        (page.scale, page.paper_size, page.fit_to_width, page.fit_to_height),
        (100, 1, 1, 1)
    );
    assert!(!page.has_manual_breaks());
}

#[test]
fn break_ids_are_the_first_row_of_the_next_page() {
    let page = PageSetup {
        row_breaks: vec![
            PageBreak { id: 34, min: 0, max: 3, manual: true, pt: false },
            PageBreak { id: 19, min: 0, max: 3, manual: true, pt: false },
        ],
        ..PageSetup::default()
    };
    assert_eq!(page.row_break_rows(), vec![19, 34]);
}
