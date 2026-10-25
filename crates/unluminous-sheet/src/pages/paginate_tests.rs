//! Tests for the paginator on sheets small enough to count by hand. The workbooks use Calibri 11, so a default row is 121 printer pixels and a Letter page less normal margins holds 47 of them.
//! The comparison with Excel itself, over a hundred sheets, is `tests/pages.rs`.

use super::headerfooter::Sections;
use super::paginate::{paginate, printed_error, Options, PageLayout};
use super::*;
use crate::coord::{CellRef, Range};
use crate::model::{Cell, Note, Sheet, Workbook};
use crate::value::Value;

fn filled(rows: u32, cols: u32) -> Workbook {
    let mut book = Workbook::new();
    book.styles.fonts[0].name = "Calibri".to_owned();
    book.sheets = vec![Sheet::new("Data")];
    for r in 0..rows {
        for c in 0..cols {
            book.sheets[0]
                .set(CellRef::new(r, c), Cell::value(Value::Number(f64::from(r * 100 + c))));
        }
    }
    book
}

fn layout(book: &Workbook) -> PageLayout {
    paginate(book, 0, &Options::default())
}

#[test]
fn a_small_sheet_is_one_page_and_a_blank_sheet_is_none() {
    assert_eq!(layout(&filled(5, 3)).page_count(), 1);
    let mut empty = Workbook::new();
    empty.sheets = vec![Sheet::new("Empty")];
    assert_eq!(layout(&empty).page_count(), 0);
}

#[test]
fn rows_fill_a_page_and_the_next_starts_after_it() {
    let l = layout(&filled(100, 1));
    assert_eq!(l.row_break_rows(), vec![48, 95]);
    assert_eq!(l.pages.len(), 3);
    assert_eq!(l.pages[1].body, Range::parse("A48:A94").unwrap());
}

#[test]
fn landscape_swaps_the_paper_and_takes_fewer_rows() {
    let mut book = filled(100, 1);
    book.sheets[0].page.orientation = Some(Orientation::Landscape);
    let l = layout(&book);
    assert_eq!(l.paper.width, 792.0);
    assert_eq!(l.row_break_rows(), vec![35, 69]);
}

#[test]
fn a_manual_break_starts_a_page_and_counting_goes_on_from_it() {
    let mut book = filled(100, 1);
    book.sheets[0].page.row_breaks.push(PageBreak {
        id: 10,
        min: 0,
        max: 16383,
        manual: true,
        pt: false,
    });
    let l = layout(&book);
    let breaks = &l.areas[0].row_breaks;
    assert_eq!((breaks[0].at, breaks[0].manual), (10, true));
    assert_eq!((breaks[1].at, breaks[1].manual), (57, false));
}

#[test]
fn print_titles_repeat_on_the_pages_after_the_first() {
    let mut book = filled(100, 1);
    set_print_titles(&mut book, 0, PrintTitles { rows: Some((0, 1)), cols: None });
    let l = layout(&book);
    assert_eq!(l.pages[0].title_rows, None);
    assert_eq!(l.pages[1].title_rows, Some((0, 1)));
    // Two rows of every later page are the titles, so a page holds two fewer.
    assert_eq!(l.row_break_rows(), vec![48, 93]);
}

#[test]
fn a_hidden_row_takes_no_room() {
    let mut book = filled(100, 1);
    for row in 9..49u32 {
        book.sheets[0].rows.entry(row).or_default().hidden = true;
    }
    let l = layout(&book);
    assert_eq!(l.row_break_rows(), vec![88]);
}

#[test]
fn the_print_area_limits_what_prints_and_several_areas_print_apart() {
    let mut book = filled(100, 10);
    set_print_area(&mut book, 0, &[Range::parse("A1:B3").unwrap(), Range::parse("D5:E7").unwrap()]);
    let l = layout(&book);
    let bodies: Vec<Range> = l.pages.iter().map(|p| p.body).collect();
    assert_eq!(bodies, vec![Range::parse("A1:B3").unwrap(), Range::parse("D5:E7").unwrap()]);
    assert_eq!((l.pages[0].area, l.pages[1].area), (0, 1));
}

#[test]
fn the_order_of_pages_follows_the_setup() {
    let mut book = filled(100, 30);
    let down = layout(&book);
    book.sheets[0].page.page_order = PageOrder::OverThenDown;
    let over = layout(&book);
    assert_eq!(down.pages.len(), over.pages.len());
    assert_eq!(down.pages[1].body.first.row, 47, "down first");
    assert_eq!(over.pages[1].body.first.row, 0, "over first");
    assert_eq!(over.pages[1].body.first.col, down.pages[3].body.first.col);
}

#[test]
fn a_page_with_nothing_on_it_is_not_printed() {
    let mut book = filled(3, 3);
    set_print_area(&mut book, 0, &[Range::parse("A1:C300").unwrap()]);
    let l = layout(&book);
    assert!(l.areas[0].row_breaks.len() >= 2, "the breaks are still there");
    assert_eq!(l.pages.len(), 1);
}

#[test]
fn fitting_to_one_page_wide_shrinks_the_scale_and_never_enlarges() {
    let mut book = filled(10, 40);
    book.sheets[0].page.fit_to_page = true;
    book.sheets[0].page.fit_to_height = 0;
    let l = layout(&book);
    assert!(l.scale < 100 && l.scale >= 10, "{}", l.scale);
    assert!(l.col_break_cols().iter().all(|&c| c > 40), "one page wide: {:?}", l.col_break_cols());
    let mut small = filled(3, 3);
    small.sheets[0].page.fit_to_page = true;
    assert_eq!(layout(&small).scale, 100);
}

#[test]
fn page_numbers_start_at_the_first_page_number_and_the_total_is_the_count() {
    let mut book = filled(100, 1);
    book.sheets[0].page.use_first_page_number = true;
    book.sheets[0].page.first_page_number = 5;
    book.sheets[0].page.header_footer.odd_footer = "&LPage &P of &N&R&A".to_owned();
    let l = layout(&book);
    assert_eq!(l.pages.iter().map(|p| p.number).collect::<Vec<_>>(), vec![5, 6, 7]);
    assert_eq!(Sections::plain(&l.pages[2].footer.left), "Page 7 of 3");
    assert_eq!(Sections::plain(&l.pages[2].footer.right), "Data");
}

#[test]
fn the_first_page_and_the_even_pages_can_have_headers_of_their_own() {
    let mut book = filled(200, 1);
    let hf = &mut book.sheets[0].page.header_footer;
    hf.odd_header = "odd".to_owned();
    hf.even_header = "even".to_owned();
    hf.first_header = "first".to_owned();
    hf.different_first = true;
    hf.different_odd_even = true;
    let l = layout(&book);
    let texts: Vec<String> = l.pages.iter().map(|p| Sections::plain(&p.header.center)).collect();
    assert_eq!(texts[..4], ["first", "even", "odd", "even"]);
}

#[test]
fn centring_puts_the_content_in_the_middle_of_the_room_between_the_margins() {
    let mut book = filled(2, 2);
    book.sheets[0].page.options.horizontal_centered = true;
    let l = layout(&book);
    let page = &l.pages[0];
    let room = l.paper.width - l.margins.left - l.margins.right;
    let expected = l.margins.left + (room - page.content.width) / 2.0;
    assert!((page.origin.0 - expected).abs() < 1e-9);
    assert_eq!(page.origin.1, l.margins.top);
}

#[test]
fn notes_print_at_the_end_when_asked_and_errors_print_as_asked() {
    let mut book = filled(3, 3);
    book.sheets[0].notes.insert((1, 1), Note::new("Ann", "Check"));
    assert!(layout(&book).note_pages.is_empty());
    book.sheets[0].page.comments = PrintComments::AtEnd;
    let l = layout(&book);
    assert_eq!(l.note_pages.len(), 1);
    assert_eq!(l.note_pages[0][0].cell, CellRef::new(1, 1));
    assert_eq!(l.page_count(), 2);
    assert_eq!(printed_error(PrintErrors::Displayed, "#DIV/0!"), "#DIV/0!");
    assert_eq!(printed_error(PrintErrors::Blank, "#DIV/0!"), "");
    assert_eq!(printed_error(PrintErrors::Dash, "#DIV/0!"), "--");
    assert_eq!(printed_error(PrintErrors::NotAvailable, "#DIV/0!"), "#N/A");
}

#[test]
fn a_custom_paper_size_is_read_from_its_dimensions() {
    let mut book = filled(2, 2);
    book.sheets[0].page.paper_width = Some("100mm".into());
    book.sheets[0].page.paper_height = Some("5in".into());
    let l = layout(&book);
    assert!((l.paper.width - 100.0 * 72.0 / 25.4).abs() < 1e-9);
    assert_eq!(l.paper.height, 360.0);
}
