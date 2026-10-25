//! A page field in the body shows the page it lands on, found by laying the body out twice
//! (page-fields.docx).

use super::*;

/// A paragraph holding `label` and then a simple field with the code `code` whose stored result is `stored`.
fn with_field(code: &str, stored: &str) -> String {
    format!(
        "<w:p><w:pPr>{PLAIN}</w:pPr><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r><w:fldSimple w:instr=\" {code} \"><w:r><w:t>{stored}</w:t></w:r></w:fldSimple></w:p>"
    )
}

/// All the text drawn on a page, joined.
fn page_text(pages: &Pages, page: usize) -> String {
    texts(&pages.pages[page]).iter().map(|t| t.text.as_str()).collect()
}

#[test]
fn page_fields_docx_a_page_field_in_the_body_shows_the_page_it_is_on_not_its_stored_result() {
    // Ten fillers fill page 1, so the field's paragraph is on page 2; the file stored "1".
    let pages = laid_small(&format!("{}{}", paras("F", 10), with_field("PAGE", "1")));
    assert!(page_text(&pages, 1).contains("Page 2"), "got {:?}", page_text(&pages, 1));
}

#[test]
fn page_fields_docx_a_page_field_on_the_first_page_shows_one() {
    let pages = laid_small(&with_field("PAGE", "9"));
    assert!(page_text(&pages, 0).contains("Page 1"), "got {:?}", page_text(&pages, 0));
}

#[test]
fn page_fields_docx_numpages_shows_the_number_of_pages() {
    // Eleven fillers and the field's paragraph make two pages whatever the field says.
    let pages = laid_small(&format!("{}{}", paras("F", 11), with_field("NUMPAGES", "1")));
    assert_eq!(pages.pages.len(), 2);
    assert!(page_text(&pages, 1).contains("Page 2"), "got {:?}", page_text(&pages, 1));
}

#[test]
fn page_fields_docx_a_roman_format_switch_is_followed() {
    let pages = laid_small(&format!("{}{}", paras("F", 10), with_field(r"PAGE \* roman", "1")));
    assert!(page_text(&pages, 1).contains("Page ii"), "got {:?}", page_text(&pages, 1));
}

#[test]
fn page_fields_docx_a_section_that_restarts_numbering_starts_its_pages_there() {
    // `pgNumType start=5` makes the first page page 5.
    let sect = page(200.0, 120.0, "<w:pgNumType w:start=\"5\"/>");
    let pages = laid(&document(&with_field("PAGE", "1"), &sect));
    assert_eq!(pages.pages[0].number, 5);
    assert!(page_text(&pages, 0).contains("Page 5"), "got {:?}", page_text(&pages, 0));
}
