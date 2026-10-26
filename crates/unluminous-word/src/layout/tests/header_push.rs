//! A header or footer taller than the room the margin leaves pushes the body away (headers.docx).

use super::*;
use crate::session::Session;

/// A document of `count` plain paragraphs on a 192 point page with 36 point margins and bands 36 points
/// from the edges, with the header and footer text given (empty for none).
fn with_bands(count: usize, header: &str, footer: &str, extra: &str) -> Pages {
    let mut session =
        Session::new(document(&paras("P", count), &banded_page(36.0, 36.0, 36.0, 36.0, extra)));
    if !header.is_empty() {
        session.set_header_text(false, "default", header);
    }
    if !footer.is_empty() {
        session.set_header_text(true, "default", footer);
    }
    laid(&session.document)
}

#[test]
fn headers_docx_without_a_header_the_body_starts_at_the_top_margin() {
    // The body runs from 36 to 156: ten lines of 12 points.
    let pages = with_bands(30, "", "", "");
    assert_eq!(pages.pages[0].body.y, 36.0);
    assert_eq!(top_of(&pages, 0), 36.0);
    assert_eq!(paragraphs_on(&pages, 30, 0), 10);
}

#[test]
fn headers_docx_a_header_that_reaches_past_the_margin_pushes_the_body_down() {
    // The header starts 36 points down and is one 12 point line, so it ends at 48, below the 36 point
    // margin: the body starts at 48 and has nine lines.
    let pages = with_bands(30, "Header", "", "");
    assert_eq!(pages.pages[0].body.y, 48.0);
    assert_eq!(top_of(&pages, 0), 48.0);
    assert_eq!(paragraphs_on(&pages, 30, 0), 9);
    assert_eq!(text_starting(&pages.pages[0], "Header").baseline, 36.0 + 9.0);
}

#[test]
fn headers_docx_a_footer_that_reaches_past_the_margin_pulls_the_foot_of_the_body_up() {
    // The footer is 12 points tall and 36 from the bottom edge, so the body ends at 192 - 48 = 144: nine
    // lines from 36 (108 points) where the header is absent.
    let pages = with_bands(30, "", "Footer", "");
    assert_eq!(pages.pages[0].body.h, 144.0 - 36.0);
    assert_eq!(paragraphs_on(&pages, 30, 0), 9);
}

#[test]
fn headers_docx_a_header_and_a_footer_each_take_their_line() {
    // Body from 48 to 144: eight lines.
    let pages = with_bands(30, "Header", "Footer", "");
    assert_eq!(pages.pages[0].body.y, 48.0);
    assert_eq!(paragraphs_on(&pages, 30, 0), 8);
    assert_eq!(paragraphs_on(&pages, 30, 1), 8);
}

#[test]
fn headers_docx_a_first_page_header_only_pushes_the_first_page() {
    // `titlePg` with a header for the first page alone: page 1 starts at 48, page 2 at the margin, 36.
    let mut session =
        Session::new(document(&paras("P", 30), &banded_page(36.0, 36.0, 36.0, 36.0, "")));
    session.set_header_text(false, "first", "Cover");
    let pages = laid(&session.document);
    assert_eq!(pages.pages[0].body.y, 48.0);
    assert_eq!(pages.pages[1].body.y, 36.0);
}
