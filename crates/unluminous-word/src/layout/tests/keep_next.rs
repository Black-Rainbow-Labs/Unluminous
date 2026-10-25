//! Keep with next, keep lines together and widow control (keeps.docx).

use super::*;

#[test]
fn keeps_docx_a_heading_without_keep_next_stays_at_the_foot_of_the_page() {
    // Nine fillers take 108 points, so the heading's line (108 to 120) is the last that fits. Nothing
    // asks for it to stay with the paragraph after it, so that paragraph is alone on page 2.
    let body = format!("{}{}{}", paras("F", 9), para("Heading"), para("Body"));
    let pages = laid_small(&body);
    assert_eq!(page_of(&pages, 9), 0);
    assert_eq!(page_of(&pages, 10), 1);
}

#[test]
fn keeps_docx_a_heading_with_keep_next_moves_to_the_next_page_with_its_paragraph() {
    // The same pages with `w:keepNext` on the heading: the heading and the first line of its paragraph
    // need 24 points and only 12 are left, so both go to page 2, where the heading is at the top.
    let body =
        format!("{}{}{}", paras("F", 9), para_with("<w:keepNext/>", "Heading"), para("Body"));
    let pages = laid_small(&body);
    assert_eq!(page_of(&pages, 8), 0);
    assert_eq!(page_of(&pages, 9), 1);
    assert_eq!(top_of(&pages, 9), 0.0);
    assert_eq!(page_of(&pages, 10), 1);
    assert_eq!(top_of(&pages, 10), 12.0);
}

#[test]
fn keeps_docx_a_chain_of_keep_next_paragraphs_moves_together_to_the_next_page() {
    // Eight fillers leave 24 points. A and B keep with next, C does not: the three lines need 36 points,
    // so the whole chain starts page 2 and page 1 ends after the fillers.
    let body = format!(
        "{}{}{}{}",
        paras("F", 8),
        para_with("<w:keepNext/>", "A"),
        para_with("<w:keepNext/>", "B"),
        para("C")
    );
    let pages = laid_small(&body);
    assert_eq!(page_of(&pages, 7), 0);
    assert_eq!(page_of(&pages, 8), 1);
    assert_eq!(page_of(&pages, 9), 1);
    assert_eq!(page_of(&pages, 10), 1);
    assert_eq!(top_of(&pages, 8), 0.0);
}

#[test]
fn keeps_docx_a_chain_that_fits_in_the_room_left_stays_on_the_page() {
    // Eight fillers, then A keeping with B: the two lines need 24 points and there are 24.
    let body = format!("{}{}{}", paras("F", 8), para_with("<w:keepNext/>", "A"), para("B"));
    let pages = laid_small(&body);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(top_of(&pages, 8), 96.0);
    assert_eq!(top_of(&pages, 9), 108.0);
}

#[test]
fn keeps_docx_a_chain_longer_than_a_page_gives_up_and_flows() {
    // Twelve paragraphs each keeping with the next cannot all be on one 120 point page, so Word stops
    // trying and the chain starts where it is, at the top of page 1, and runs on to page 2.
    let kept: String = (0..12).map(|i| para_with("<w:keepNext/>", &format!("K{i}"))).collect();
    let pages = laid_small(&format!("{kept}{}", para("End")));
    assert_eq!(page_of(&pages, 0), 0);
    assert_eq!(top_of(&pages, 0), 0.0);
    assert_eq!(page_of(&pages, 9), 0);
    assert_eq!(page_of(&pages, 10), 1);
    assert_eq!(pages.pages.len(), 2);
}

#[test]
fn keeps_docx_only_the_last_two_lines_of_a_keep_next_paragraph_go_with_what_follows() {
    // Seven fillers leave three lines. A five line paragraph with keep with next puts three lines on
    // page 1 and its last two, which widow control keeps together, go to page 2 with the next paragraph.
    let body =
        format!("{}{}{}", paras("F", 7), para_with("<w:keepNext/>", &words(5)), para("Next"));
    let pages = laid_small(&body);
    assert_eq!(pages_of_lines(&pages, 7), vec![0, 0, 0, 1, 1]);
    assert_eq!(page_of(&pages, 8), 1);
    assert_eq!(top_of(&pages, 8), 24.0);
}

#[test]
fn keeps_docx_keep_lines_moves_a_paragraph_that_would_split_whole_to_the_next_page() {
    // Seven fillers leave three lines. Five lines would split 3 and 2 (widow control allows that), but
    // with `w:keepLines` the paragraph does not split and starts page 2.
    let split = laid_small(&format!("{}{}", paras("F", 7), para(&words(5))));
    assert_eq!(pages_of_lines(&split, 7), vec![0, 0, 0, 1, 1]);
    let kept = laid_small(&format!("{}{}", paras("F", 7), para_with("<w:keepLines/>", &words(5))));
    assert_eq!(pages_of_lines(&kept, 7), vec![1, 1, 1, 1, 1]);
}

#[test]
fn keeps_docx_widow_control_does_not_leave_one_line_at_the_top_of_a_page() {
    // Eight fillers leave two lines. A three line paragraph cannot split into 2 and 1 under widow
    // control (and a paragraph under four lines is not split at all), so it starts page 2.
    let pages = laid_small(&format!("{}{}", paras("F", 8), para(&words(3))));
    assert_eq!(pages_of_lines(&pages, 8), vec![1, 1, 1]);
}
