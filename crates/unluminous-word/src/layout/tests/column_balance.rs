//! Columns that end a section are balanced so they end level (columns-balance.docx).

use super::*;

/// The section of the small page in two columns of 70 points with 20 points between them.
fn two_columns(extra: &str) -> String {
    page(200.0, 120.0, &format!("<w:cols w:num=\"2\" w:space=\"400\"/>{extra}"))
}

/// The x of the first stop of body block `index`.
fn x_of(pages: &Pages, index: u32) -> f32 {
    lines_of(pages, &[index])[0].1.stops[0].1
}

#[test]
fn columns_balance_docx_the_last_section_is_split_into_two_level_columns() {
    // Seven lines in two columns: the left takes four and the right three, not seven and none. The
    // second column starts at 20 + 70 + 20.
    let pages = laid(&document(&paras("P", 7), &two_columns("")));
    assert_eq!(pages.pages.len(), 1);
    for i in 0..4 {
        assert_eq!(x_of(&pages, i), 20.0, "paragraph {i} is in the left column");
        assert_eq!(top_of(&pages, i), 12.0 * i as f32);
    }
    for i in 4..7 {
        assert_eq!(x_of(&pages, i), 110.0, "paragraph {i} is in the right column");
        assert_eq!(top_of(&pages, i), 12.0 * (i - 4) as f32);
    }
}

#[test]
fn columns_balance_docx_an_even_number_of_lines_is_split_in_half() {
    let pages = laid(&document(&paras("P", 6), &two_columns("")));
    assert_eq!(x_of(&pages, 2), 20.0);
    assert_eq!(x_of(&pages, 3), 110.0);
    assert_eq!(top_of(&pages, 3), 0.0);
}

#[test]
fn columns_balance_docx_a_continuous_break_balances_the_columns_above_it() {
    // The first section's two columns hold six lines. Its sectPr is in paragraph 5, and the document's own
    // section starts "continuous", so the one column text starts under the three level lines at 36.
    let first_section = format!(
        "<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"4000\" w:h=\"2400\"/><w:pgMar w:top=\"0\" w:right=\"400\" w:bottom=\"0\" w:left=\"400\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/><w:cols w:num=\"2\" w:space=\"400\"/></w:sectPr>{PLAIN}</w:pPr><w:r><w:t>P5</w:t></w:r></w:p>"
    );
    let body = format!("{}{}{}", paras("P", 5), first_section, para("After"));
    let pages = laid(&document(&body, &page(200.0, 120.0, "<w:type w:val=\"continuous\"/>")));
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(x_of(&pages, 2), 20.0);
    assert_eq!(x_of(&pages, 3), 110.0);
    assert_eq!(top_of(&pages, 3), 0.0);
    assert_eq!(top_of(&pages, 6), 36.0);
    assert_eq!(x_of(&pages, 6), 20.0);
}

#[test]
fn columns_balance_docx_a_full_page_of_columns_is_not_shortened() {
    // Twenty lines fill both 120 point columns exactly, so there is nothing to level.
    let pages = laid(&document(&paras("P", 20), &two_columns("")));
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(x_of(&pages, 9), 20.0);
    assert_eq!(x_of(&pages, 10), 110.0);
    assert_eq!(top_of(&pages, 19), 108.0);
}

#[test]
fn columns_balance_docx_a_column_break_starts_the_right_column() {
    let body = format!(
        "{}<w:p><w:pPr>{PLAIN}</w:pPr><w:r><w:br w:type=\"column\"/><w:t>Right</w:t></w:r></w:p>",
        para("Left")
    );
    let pages = laid(&document(&body, &two_columns("")));
    // The line holding the break ends the left column; the text after it is the first line of the right one.
    assert_eq!(x_of(&pages, 0), 20.0);
    assert_eq!(line_starts(&pages, 1), vec![(12.0, 20.0), (0.0, 110.0)]);
}
