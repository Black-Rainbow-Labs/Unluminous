//! Absolute position tabs (ptab-footer.docx): they stop at the margins or the indents however the text
//! before them is laid out, with the stop's alignment.

use super::*;

/// A paragraph with `L`, a position tab, `mid`, a second position tab and `R`, with `ind` as the
/// paragraph's indents, the tabs' reference (`margin` or `indent`) and their alignments.
fn tabbed(ind: &str, relative: &str, first: &str, second: &str) -> String {
    format!(
        "<w:p><w:pPr>{ind}{PLAIN}</w:pPr><w:r><w:t>L</w:t></w:r><w:r><w:ptab w:relativeTo=\"{relative}\" w:alignment=\"{first}\" w:leader=\"none\"/></w:r><w:r><w:t>mid</w:t></w:r><w:r><w:ptab w:relativeTo=\"{relative}\" w:alignment=\"{second}\" w:leader=\"none\"/></w:r><w:r><w:t>R</w:t></w:r></w:p>"
    )
}

/// The x of the first letter of the text item starting with `prefix`.
fn x_at(pages: &Pages, prefix: &str) -> f32 {
    text_starting(&pages.pages[0], prefix).xs[0]
}

#[test]
fn ptab_footer_docx_a_centre_tab_centres_the_text_after_it_on_the_column() {
    // The column is 160 points from x 20, so its middle is 100 and "mid" (18 points) starts at 91. The
    // right tab ends "R" (6 points) at the right margin, 180, so it starts at 174.
    let pages = laid_small(&tabbed("", "margin", "center", "right"));
    assert_eq!(x_at(&pages, "L"), 20.0);
    assert_eq!(x_at(&pages, "mid"), 91.0);
    assert_eq!(x_at(&pages, "R"), 174.0);
}

#[test]
fn ptab_footer_docx_a_tab_relative_to_the_indents_uses_the_paragraph_indents() {
    // Indents of 20 points each side: the right stop is at 140 from the column (absolute 160), so "R"
    // starts at 154. The centre of 20 to 140 is 80 from the column (absolute 100), the same middle as
    // the margins give.
    let indents = "<w:ind w:left=\"400\" w:right=\"400\"/>";
    let pages = laid_small(&tabbed(indents, "indent", "center", "right"));
    assert_eq!(x_at(&pages, "L"), 40.0);
    assert_eq!(x_at(&pages, "mid"), 91.0);
    assert_eq!(x_at(&pages, "R"), 154.0);
}

#[test]
fn ptab_footer_docx_a_tab_relative_to_the_margin_ignores_the_indents() {
    let indents = "<w:ind w:left=\"400\" w:right=\"400\"/>";
    let pages = laid_small(&tabbed(indents, "margin", "center", "right"));
    assert_eq!(x_at(&pages, "R"), 174.0);
}

#[test]
fn ptab_footer_docx_a_left_tab_goes_to_the_left_edge_of_what_it_is_relative_to() {
    // The paragraph is indented 20 points. After "L" (40 to 46) a left tab relative to the indent
    // stops at the indent, behind the pen, so "mid" follows "L" directly.
    let indents = "<w:ind w:left=\"400\"/>";
    let pages = laid_small(&tabbed(indents, "indent", "left", "left"));
    assert_eq!(x_at(&pages, "mid"), 46.0);
}
