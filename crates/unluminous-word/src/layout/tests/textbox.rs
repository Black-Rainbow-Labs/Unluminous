//! A text box lays its text out inside its insets and anchors it to the top, middle or bottom of the box
//! (linked-textboxes.docx).

use super::*;
use crate::model::TextBox;

/// A 100 by 60 point box anchored at the column's left and its paragraph's top, in front of the text, with
/// `body_pr` as the attributes of its `wps:bodyPr` and `inside` (paragraphs as XML) as its text. The box is
/// at (20, 0) on the page.
fn boxed(body_pr: &str, inside: &str) -> Pages {
    let emu = |points: f32| (points * 12700.0) as i64;
    let (w, h) = (emu(100.0), emu(60.0));
    let drawing = format!(
        "<w:r><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"1\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx=\"{w}\" cy=\"{h}\"/><wp:wrapNone/><wp:docPr id=\"7\" name=\"Text Box 7\"/><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"/></wps:spPr><wps:txbx><w:txbxContent>{inside}</w:txbxContent></wps:txbx><wps:bodyPr {body_pr}/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"
    );
    let body = format!(
        "<w:p xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><w:pPr>{PLAIN}</w:pPr>{drawing}</w:p>"
    );
    let mut doc = document(&body, &small_page());
    let content =
        crate::xml::parse(&format!("<w:txbxContent xmlns:w=\"{W_NS}\">{inside}</w:txbxContent>"))
            .expect("the box text is well formed");
    doc.textboxes.push(TextBox { id: 7, blocks: crate::read::blocks(&content), dirty: false });
    laid(&doc)
}

#[test]
fn linked_textboxes_docx_text_starts_inside_the_default_insets() {
    // The defaults are 0.1 inch (7.2 points) left and right and 0.05 inch (3.6 points) above and below,
    // so the first letter is at 20 + 7.2 and the first baseline at 3.6 + 9.
    let pages = boxed("", &para("Hi"));
    let hi = text_starting(&pages.pages[0], "Hi");
    assert!((hi.xs[0] - 27.2).abs() < 0.01, "x {}", hi.xs[0]);
    assert!((hi.baseline - 12.6).abs() < 0.01, "baseline {}", hi.baseline);
}

#[test]
fn linked_textboxes_docx_the_insets_the_box_names_replace_the_defaults() {
    // lIns of 10 points and tIns of 20 points, written in EMUs: the letter at 20 + 10, the baseline at 20 + 9.
    let pages = boxed("lIns=\"127000\" tIns=\"254000\" rIns=\"0\" bIns=\"0\"", &para("Hi"));
    let hi = text_starting(&pages.pages[0], "Hi");
    assert!((hi.xs[0] - 30.0).abs() < 0.01, "x {}", hi.xs[0]);
    assert!((hi.baseline - 29.0).abs() < 0.01, "baseline {}", hi.baseline);
}

#[test]
fn linked_textboxes_docx_text_wraps_at_the_width_less_the_insets() {
    // 100 - 14.4 = 85.6 points is 14 clusters. Ten letters, a space and ten more need 21, so the second
    // word is on a second line 12 points lower.
    let pages = boxed("", &para(&short_words(2)));
    let lines: Vec<&TextItem> =
        texts(&pages.pages[0]).into_iter().filter(|t| t.text.starts_with("abcdefghij")).collect();
    assert_eq!(lines.len(), 2, "two lines of text");
    assert!((lines[1].baseline - lines[0].baseline - 12.0).abs() < 0.01);
}

#[test]
fn linked_textboxes_docx_anchor_ctr_puts_the_text_in_the_middle_of_the_box() {
    // Inside the insets there are 60 - 7.2 = 52.8 points and the line takes 12, so 40.8 are free and half
    // of that, 20.4, comes before the line: baseline 3.6 + 20.4 + 9.
    let pages = boxed("anchor=\"ctr\"", &para("Hi"));
    assert!((text_starting(&pages.pages[0], "Hi").baseline - 33.0).abs() < 0.01);
}

#[test]
fn linked_textboxes_docx_anchor_b_puts_the_text_at_the_bottom_of_the_box() {
    // All 40.8 free points come before the line: baseline 3.6 + 40.8 + 9.
    let pages = boxed("anchor=\"b\"", &para("Hi"));
    assert!((text_starting(&pages.pages[0], "Hi").baseline - 53.4).abs() < 0.01);
}
