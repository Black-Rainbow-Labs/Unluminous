//! Paste Special, Paste Link, Insert Copied Cells and the rest of the clipboard, each replayed against what
//! Excel did. `tools/excel-oracle/edit-oracle.ps1` made the answers in `tests/fixtures/clipboard`; they are
//! committed, so these tests need no Excel.

mod common;

fn expect_none(wrong: Vec<String>) {
    assert!(wrong.is_empty(), "{} differences:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
fn every_kind_of_paste_special_does_what_excel_does() {
    expect_none(common::check_file("clipboard", "paste_kinds", &[]));
}

#[test]
fn every_operation_on_every_kind_does_what_excel_does() {
    expect_none(common::check_file("clipboard", "paste_operations", &[]));
}

#[test]
fn operations_on_formulas_text_blanks_and_skipping_blanks() {
    expect_none(common::check_file("clipboard", "paste_operations_more", &[]));
}

#[test]
fn transposed_references_blank_destinations_and_inserting_copied_cells() {
    expect_none(common::check_file("clipboard", "paste_misc", &[]));
}

#[test]
fn paste_link_makes_references_to_the_copied_cells() {
    expect_none(common::check_file("clipboard", "paste_link", &[]));
}

#[test]
fn a_filtered_range_copies_its_visible_rows_and_pastes_into_hidden_rows_too() {
    expect_none(common::check_file("clipboard", "paste_filtered", &[]));
    expect_none(common::check_file("clipboard", "paste_filtered_from_other_sheet", &[]));
    expect_none(common::check_file("clipboard", "paste_filtered_to_filtered", &[]));
}

#[test]
fn the_text_a_copy_gives_other_programs_is_what_excel_gives() {
    expect_none(common::check_file("clipboard", "copy_text", &[]));
}

#[test]
fn text_from_other_programs_is_typed_into_the_cells() {
    // Typing "$3.50" gives the Currency format. Excel gives it with red negatives, which `input.rs`, held
    // to its own format by other tests, does not.
    let currency = "typed currency takes Excel's format with red negatives";
    expect_none(common::check_file("clipboard", "paste_text", &[("text_typed:E2", currency)]));
}

#[test]
fn html_tables_from_other_programs_become_cells() {
    let currency = "typed currency takes Excel's format with red negatives";
    expect_none(common::check_file("clipboard", "paste_html", &[("html_typed:F2", currency)]));
}

#[test]
fn more_operations_skipping_blanks_and_logical_values() {
    expect_none(common::check_file("clipboard", "paste_more", &[]));
}
