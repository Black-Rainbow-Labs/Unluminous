//! How far text keeps from a floating picture, by each kind of wrapping (the wrapping fixture).

use super::*;

#[test]
fn wrapping_docx_square_wrapping_puts_the_text_beside_the_picture_and_then_under_it() {
    // A 60 by 30 point picture at the column's left edge (x 20 to 80), nothing kept clear. Beside it the
    // column is 100 points wide and holds one 60 point word (two need 126), so the first three lines
    // (0, 12 and 24, the last still above the picture's bottom at 30) start at x 80. From 36 down the
    // whole 160 points are free and two words fit a line, which is why the fourth line has two.
    let body = para_with_drawing(
        &floating("<wp:wrapSquare wrapText=\"bothSides\"/>", 0.0, 0.0, 60.0, 30.0, 0.0),
        &short_words(8),
    );
    let pages = laid_small(&body);
    let starts = line_starts(&pages, 0);
    assert_eq!(&starts[..3], &[(0.0, 80.0), (12.0, 80.0), (24.0, 80.0)]);
    assert_eq!(starts[3], (36.0, 20.0));
}

#[test]
fn wrapping_docx_square_wrapping_keeps_the_distance_from_text() {
    // `distR` of 6 points moves the text from x 80 to 86.
    let body = para_with_drawing(
        &floating("<wp:wrapSquare wrapText=\"bothSides\"/>", 0.0, 0.0, 60.0, 30.0, 6.0),
        &short_words(3),
    );
    let starts = line_starts(&laid_small(&body), 0);
    assert_eq!(starts[0], (0.0, 86.0));
}

#[test]
fn wrapping_docx_wrap_text_left_keeps_the_text_on_the_left_of_a_picture_on_the_right() {
    // A 60 point picture at x 100 to 160 of the column (absolute 120 to 180). With `wrapText="left"` the
    // text stays left of it, in the 100 points from 20 to 120, and starts at the column's left.
    let body = para_with_drawing(
        &floating("<wp:wrapSquare wrapText=\"left\"/>", 100.0, 0.0, 60.0, 30.0, 0.0),
        &short_words(3),
    );
    let pages = laid_small(&body);
    let lines = lines_of(&pages, &[0]);
    assert_eq!(lines[0].1.top, 0.0);
    assert_eq!(lines[0].1.stops.first().unwrap().1, 20.0);
    let text = text_starting(&pages.pages[0], "abc");
    assert!(text.xs.iter().all(|x| *x < 120.0), "the letters stay left of the picture");
}

#[test]
fn wrapping_docx_top_and_bottom_wrapping_starts_the_text_under_the_picture() {
    // Nothing goes beside the picture, so the first line is at the picture's bottom, 30 points down.
    let body =
        para_with_drawing(&floating("<wp:wrapTopAndBottom/>", 0.0, 0.0, 60.0, 30.0, 0.0), "Under");
    let starts = line_starts(&laid_small(&body), 0);
    assert_eq!(starts[0], (30.0, 20.0));
}

#[test]
fn wrapping_docx_a_picture_wrapped_none_does_not_move_the_text() {
    let body = para_with_drawing(&floating("<wp:wrapNone/>", 0.0, 0.0, 60.0, 30.0, 0.0), "Over");
    let starts = line_starts(&laid_small(&body), 0);
    assert_eq!(starts[0], (0.0, 20.0));
}

#[test]
fn wrapping_docx_a_gap_narrower_than_eighteen_points_puts_no_text_beside_the_picture() {
    // A 150 point picture leaves 10 points on its right, less than the 18 point least width, so the
    // text starts under it, at 30.
    let body = para_with_drawing(
        &floating("<wp:wrapSquare wrapText=\"bothSides\"/>", 0.0, 0.0, 150.0, 30.0, 0.0),
        "Under",
    );
    let starts = line_starts(&laid_small(&body), 0);
    assert_eq!(starts[0], (30.0, 20.0));
}

#[test]
fn wrapping_docx_tight_wrapping_follows_the_wrap_polygon_bounds() {
    // A 60 point picture whose wrap polygon covers only its left half (x 0 to 10800 of 21600): text
    // keeps from x 50, 30 points in, instead of from the picture's full width at 80.
    let polygon = "<wp:wrapTight wrapText=\"bothSides\"><wp:wrapPolygon edited=\"0\"><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"10800\" y=\"0\"/><wp:lineTo x=\"10800\" y=\"21600\"/><wp:lineTo x=\"0\" y=\"21600\"/></wp:wrapPolygon></wp:wrapTight>";
    let body = para_with_drawing(&floating(polygon, 0.0, 0.0, 60.0, 30.0, 0.0), "Near");
    let starts = line_starts(&laid_small(&body), 0);
    assert_eq!(starts[0], (0.0, 50.0));
}
