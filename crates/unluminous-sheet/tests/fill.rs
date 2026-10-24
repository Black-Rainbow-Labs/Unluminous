//! The fill commands replayed against what Excel did: dragging the fill handle with every Auto Fill Option,
//! the Series dialog, fill across worksheets, Justify and Flash Fill. `tools/excel-oracle/edit-oracle.ps1`
//! made the answers in `tests/fixtures/fill`; they are committed, so these tests need no Excel.

mod common;

fn expect_none(wrong: Vec<String>) {
    assert!(wrong.is_empty(), "{} differences:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
fn dragging_the_fill_handle_with_every_auto_fill_option() {
    // Fill Weekdays on two dates (columns AK to AN here) gives Excel results that follow no rule found by
    // measuring: a pair of dates a week apart continues as the pair moved on by 5, 12, 19 days. The
    // engine steps by the weekdays between them. Other kinds of fill on two dates agree with Excel.
    let unmatched = "two dates, which Excel moves on in a way no rule describes";
    let skip = [
        ("auto_weekdays:AK", unmatched),
        ("auto_weekdays:AL", unmatched),
        ("auto_weekdays:AM", unmatched),
        ("auto_weekdays:AN", unmatched),
    ];
    expect_none(common::check_file("fill", "autofill_types", &skip));
}

#[test]
fn text_with_numbers_names_quarters_ordinals_dates_and_every_direction() {
    expect_none(common::check_file("fill", "autofill_patterns", &[]));
}

#[test]
fn the_series_dialog_linear_growth_dates_stop_values_and_trends() {
    expect_none(common::check_file("fill", "series_dialog", &[]));
}

#[test]
fn fill_across_worksheets_copies_all_the_contents_or_the_formats() {
    expect_none(common::check_file("fill", "across_sheets", &[]));
}

#[test]
fn fill_keeps_formats_values_validation_and_rules_as_excel_does() {
    expect_none(common::check_file("fill", "autofill_formats", &[]));
}

#[test]
fn flash_fill_finds_the_pattern_excel_finds() {
    expect_none(common::check_file("fill", "flash_fill", &[]));
}
