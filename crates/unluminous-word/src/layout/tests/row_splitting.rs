//! A table row is cut between the lines of its cells unless it says `w:cantSplit` (long-table.docx).

use super::*;

/// A one column table, 160 points wide, whose single row has a cell of `lines` lines.
fn one_row(trpr: &str, lines: usize) -> String {
    table("", &[3200], &row(trpr, &cell(3200, "", &para(&words(lines)))))
}

#[test]
fn long_table_docx_a_row_is_cut_between_the_lines_of_its_cell_when_it_does_not_fit() {
    // Seven fillers leave 36 points. The five line cell (60 points) is cut after the three lines that
    // fit, and widow control leaves two on the second page.
    let pages = laid_small(&format!("{}{}", paras("F", 7), one_row("", 5)));
    let lines = cell_lines(&pages, 7, 0, 0);
    let on: Vec<usize> = lines.iter().map(|(page, _)| *page).collect();
    assert_eq!(on, vec![0, 0, 0, 1, 1]);
    assert_eq!(lines[3].1.top, 0.0);
    assert_eq!(lines[4].1.top, 12.0);
}

#[test]
fn long_table_docx_a_cant_split_row_moves_whole_to_the_next_page() {
    // The same table with `w:cantSplit`: the 60 point row fits on a fresh page, so it starts page 2.
    let pages = laid_small(&format!("{}{}", paras("F", 7), one_row("<w:cantSplit/>", 5)));
    let on: Vec<usize> = cell_lines(&pages, 7, 0, 0).iter().map(|(page, _)| *page).collect();
    assert_eq!(on, vec![1, 1, 1, 1, 1]);
    assert_eq!(cell_lines(&pages, 7, 0, 0)[0].1.top, 0.0);
}

#[test]
fn long_table_docx_a_cant_split_row_taller_than_a_page_is_cut_anyway() {
    // Twelve lines (144 points) cannot fit a 120 point page, so waiting for a new page would not help
    // and the row is cut after ten lines.
    let pages = laid_small(&one_row("<w:cantSplit/>", 12));
    let on: Vec<usize> = cell_lines(&pages, 0, 0, 0).iter().map(|(page, _)| *page).collect();
    assert_eq!(on.iter().filter(|p| **p == 0).count(), 10);
    assert_eq!(on.iter().filter(|p| **p == 1).count(), 2);
}

#[test]
fn long_table_docx_a_row_moves_whole_when_only_one_line_of_its_cell_would_fit() {
    // Nine fillers leave one line. A four line cell cannot leave a single line behind (each side of the
    // cut keeps two), so Word moves the whole row to the next page.
    let pages = laid_small(&format!("{}{}", paras("F", 9), one_row("", 4)));
    let on: Vec<usize> = cell_lines(&pages, 9, 0, 0).iter().map(|(page, _)| *page).collect();
    assert_eq!(on, vec![1, 1, 1, 1]);
}

#[test]
fn long_table_docx_a_header_row_is_repeated_at_the_top_of_the_next_page() {
    // A header row and a twelve line row: the header, then nine lines fill page 1, and page 2 begins
    // with a copy of the header above the last three lines.
    let header = row("<w:tblHeader/>", &cell(3200, "", &para("Head")));
    let body = row("", &cell(3200, "", &para(&words(12))));
    let pages = laid_small(&table("", &[3200], &format!("{header}{body}")));
    assert_eq!(pages.pages.len(), 2);
    assert_eq!(text_starting(&pages.pages[0], "Head").baseline, 9.0);
    assert_eq!(text_starting(&pages.pages[1], "Head").baseline, 9.0);
    let lines = cell_lines(&pages, 0, 1, 0);
    assert_eq!(lines.iter().filter(|(page, _)| *page == 0).count(), 9);
    assert_eq!(lines.iter().filter(|(page, _)| *page == 1).count(), 3);
    assert_eq!(lines[9].1.top, 12.0, "the continued row starts under the repeated header");
}
