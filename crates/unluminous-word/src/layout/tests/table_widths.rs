//! How a table's column widths are worked out (table-widths.docx): the grid Word saved, a percentage,
//! cell widths with no grid, a stated width, and alignment.

use super::*;

/// The right edges of the cells of the first table row on the first page, as the pointer sees them.
fn edges(pages: &Pages) -> Vec<f32> {
    let mut xs: Vec<f32> = pages.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            Item::TableEdge { x, top, .. } if *top < 1.0 => Some(*x),
            _ => None,
        })
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    xs
}

/// A one row table with the cell widths given (twips) and the grid given (empty for no grid).
fn two_cells(
    props: &str,
    grid: &[u32],
    first: u32,
    second: Option<u32>,
    second_text: &str,
) -> String {
    let cells = format!(
        "{}{}",
        cell(first, "", &para("a")),
        match second {
            Some(w) => cell(w, "", &para(second_text)),
            None => format!("<w:tc><w:tcPr/>{}</w:tc>", para(second_text)),
        }
    );
    table(props, grid, &row("", &cells))
}

#[test]
fn table_widths_docx_fixed_columns_take_the_widths_of_the_saved_grid() {
    // A grid of 100 and 60 points puts the cell edges at 20 + 100 and 20 + 160.
    let pages = laid_small(&two_cells("", &[2000, 1200], 2000, Some(1200), "b"));
    assert_eq!(edges(&pages), vec![120.0, 180.0]);
}

#[test]
fn table_widths_docx_the_grid_wins_over_the_widths_the_cells_state() {
    // The cells say 40 and 40 points, but the grid is what Word laid the table out with.
    let pages = laid_small(&two_cells("", &[2000, 1200], 800, Some(800), "b"));
    assert_eq!(edges(&pages), vec![120.0, 180.0]);
}

#[test]
fn table_widths_docx_a_percentage_width_scales_the_grid_to_that_share_of_the_column() {
    // `tblW` of 50% (2500 fiftieths of a percent) of the 160 point column is 80 points, so the 100 and 60
    // point grid becomes 50 and 30.
    let pages = laid_small(&two_cells(
        "<w:tblW w:w=\"2500\" w:type=\"pct\"/>",
        &[2000, 1200],
        2000,
        Some(1200),
        "b",
    ));
    assert_eq!(edges(&pages), vec![70.0, 100.0]);
}

#[test]
fn table_widths_docx_with_no_grid_a_cell_with_a_width_keeps_it_and_the_other_fits_its_text() {
    // The first column says 100 points. The second has no width and holds five letters (30 points) plus its
    // two 5.4 point margins: 40.8 points, since the table fits its contents (and 40.8 is less than the 60 left).
    let pages = laid_small(&two_cells("", &[], 2000, None, "abcde"));
    let edges = edges(&pages);
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0], 120.0);
    assert!((edges[1] - 160.8).abs() < 0.01, "got {}", edges[1]);
}

#[test]
fn table_widths_docx_with_no_grid_a_stated_width_gives_the_rest_to_the_open_column() {
    // `tblW` of 150 points with a 100 point first column: the second column takes the other 50.
    let pages =
        laid_small(&two_cells("<w:tblW w:w=\"3000\" w:type=\"dxa\"/>", &[], 2000, None, "b"));
    assert_eq!(edges(&pages), vec![120.0, 170.0]);
}

#[test]
fn table_widths_docx_a_centred_table_is_centred_in_the_column() {
    // A 100 point table in the 160 point column starts 30 points in: edges at 20 + 30 + 60 and + 100.
    let pages =
        laid_small(&two_cells("<w:jc w:val=\"center\"/>", &[1200, 800], 1200, Some(800), "b"));
    assert_eq!(edges(&pages), vec![110.0, 150.0]);
}

#[test]
fn table_widths_docx_an_indented_table_starts_at_its_indent() {
    // `tblInd` of 20 points moves both edges right by 20.
    let pages = laid_small(&two_cells(
        "<w:tblInd w:w=\"400\" w:type=\"dxa\"/>",
        &[2000, 1200],
        2000,
        Some(1200),
        "b",
    ));
    assert_eq!(edges(&pages), vec![140.0, 200.0]);
}

#[test]
fn table_widths_docx_with_no_grid_content_wider_than_the_room_left_is_squeezed_to_it() {
    // Ten letters and the margins want 70.8 points, but only 160 - 100 = 60 are left in the column.
    let pages = laid_small(&two_cells("", &[], 2000, None, "abcdefghij"));
    assert_eq!(edges(&pages), vec![120.0, 180.0]);
}
