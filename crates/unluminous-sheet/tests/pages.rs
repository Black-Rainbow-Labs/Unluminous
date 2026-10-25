//! The paginator against Excel: every sheet of the page setup workbooks is paginated and its page breaks and page
//! count compared with the ones Excel computed for the same sheet.
//!
//! The workbooks `fixtures/pages/pages-<group>.xlsx` were built and saved by Excel 16 with
//! `tools/excel-oracle/page-breaks.ps1`, one sheet for each variant of paper size, orientation, margins, scale,
//! fit to pages, print titles, manual breaks, hidden rows and columns, print areas and data shape. Next to each is
//! `pages-<group>.json`, Excel's answers for it: `HPageBreaks`, `VPageBreaks` and `PageSetup.Pages.Count`. The
//! JSON names the printer Excel computed with, because Excel breaks pages from the printer driver's page
//! area, and these answers are for Microsoft Print to PDF at 600 dpi, which is what the paginator models.
//!
//! The tests run with no Excel. When a variant is added, run the script again for it.

use std::path::{Path, PathBuf};

use serde_json::Value;
use unluminous_sheet::pages::paginate::{paginate, Options};

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pages")
}

/// What is known to differ from Excel, with the reason. Each is a sheet whose answer Excel computes in a way the
/// paginator does not model. A sheet is listed by `group/sheet`.
const KNOWN_DIFFERENCES: &[(&str, &str)] = &[
    ("pages-calibri.json/fit-2x3", FIT_MORE_THAN_ONE_WIDE),
    ("pages-calibri.json/fit-2x0", FIT_MORE_THAN_ONE_WIDE),
    ("pages-calibri.json/titles-fit-2x3", FIT_MORE_THAN_ONE_WIDE),
    ("pages-calibri.json/fit-4x4", FIT_MORE_THAN_ONE_WIDE),
    ("pages-aptos-narrow.json/fit-1x1", FIT_MORE_THAN_ONE_WIDE),
    ("pages-times.json/fit-1x1", FIT_MORE_THAN_ONE_WIDE),
    (
        "pages-calibri.json/break-fit",
        "Excel lists only the manual break for a sheet fitted to pages, and lays the pages out as if there were none;          the paginator lists the automatic breaks of that layout",
    ),
];

/// Why a sheet fitted to pages differs. Measured (see the notes on fitting in `pages::paginate`): fitting to one
/// page wide gives Excel's scale every time, but when the columns fill the last of several pages exactly, Excel
/// takes a scale 1% to 8% below the largest at which the pages fit. Its scale depends on the number of columns and
/// pages and not on the rows, and its page breaks at the scale it chose are the ones the paginator makes at the
/// same scale.
const FIT_MORE_THAN_ONE_WIDE: &str =
    "Excel's scale for a fit to several pages wide is below the largest that fits";

/// The one based rows or columns of a list of breaks in Excel's answer.
fn locations(answer: &Value, key: &str, axis: &str) -> Vec<u32> {
    answer[key].as_array().map_or_else(Vec::new, |list| {
        list.iter().map(|b| b[axis].as_u64().unwrap() as u32).collect()
    })
}

/// Every group's answers, with the group's name.
fn groups() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(folder()).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("pages-") && name.ends_with(".json") && !name.contains("fonts") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((name, serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn every_sheet_breaks_where_excel_breaks_it() {
    let mut wrong = Vec::new();
    let mut compared = 0;
    for (name, group) in groups() {
        let bytes = std::fs::read(folder().join(group["workbook"].as_str().unwrap())).unwrap();
        let book = unluminous_sheet::xlsx::read(&bytes).unwrap();
        for answer in group["sheets"].as_array().unwrap() {
            let sheet_name = answer["sheet"].as_str().unwrap();
            if answer["pages"].is_null() || !answer["failure"].as_str().unwrap_or("").is_empty() {
                continue;
            }
            // Excel could not hand out the break locations of a few sheets ("Invalid index" from `Item`); for those
            // only the page count is compared.
            let unreadable = ["hbreaks", "vbreaks"]
                .iter()
                .any(|key| answer[*key].as_array().is_none_or(|l| l.iter().any(Value::is_null)));
            let key = format!("{name}/{sheet_name}");
            if KNOWN_DIFFERENCES.iter().any(|(k, _)| key.ends_with(k)) {
                continue;
            }
            let index = book.sheet_index(sheet_name).unwrap();
            let layout = paginate(&book, index, &Options::default());
            compared += 1;
            let ours = (layout.row_break_rows(), layout.col_break_cols(), layout.page_count());
            let excel = (
                if unreadable { Vec::new() } else { locations(answer, "hbreaks", "row") },
                if unreadable { Vec::new() } else { locations(answer, "vbreaks", "col") },
                answer["pages"].as_u64().unwrap() as usize,
            );
            // Several print areas print on pages of their own and Excel lists no breaks for them.
            let several_areas = unluminous_sheet::pages::print_area(&book, index).len() > 1;
            let breaks_agree =
                several_areas || unreadable || (ours.0 == excel.0 && ours.1 == excel.1);
            if !breaks_agree || ours.2 != excel.2 {
                wrong.push(format!("{key}: ours rows {:?} cols {:?} pages {} (scale {}), Excel rows {:?} cols {:?} pages {}", ours.0, ours.1, ours.2, layout.scale, excel.0, excel.1, excel.2));
            }
        }
    }
    assert!(compared > 100, "only {compared} sheets were compared");
    assert!(
        wrong.is_empty(),
        "{} of {compared} sheets differ from Excel:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

// ----------------------------------------------------------------------------------------------------
// A file Excel wrote with page setup in every element.

use unluminous_sheet::pages::{
    print_area, print_titles, HeaderFooterPart, Orientation, PageOrder, PageSetup, PrintComments,
    PrintErrors,
};

fn page_setup_book() -> unluminous_sheet::Workbook {
    unluminous_sheet::xlsx::read(&std::fs::read(folder().join("pagesetup.xlsx")).unwrap()).unwrap()
}

/// The text of a worksheet part in a written workbook.
fn sheet_xml(bytes: &[u8], part: &str) -> String {
    let files = unluminous_sheet::zip::read(bytes).unwrap();
    String::from_utf8(files.iter().find(|e| e.name == part).unwrap().data.clone()).unwrap()
}

#[test]
fn a_page_setup_file_excel_wrote_reads_into_the_model() {
    let book = page_setup_book();
    let a = &book.sheets[0].page;
    assert_eq!(a.orientation, Some(Orientation::Landscape));
    assert_eq!((a.paper_size, a.fit_to_height, a.fit_to_page), (5, 0, true));
    assert_eq!(
        (a.margins.left, a.margins.right, a.margins.top, a.margins.bottom),
        (0.5, 0.6, 1.1, 1.2)
    );
    assert_eq!((a.margins.header, a.margins.footer), (0.4, 0.45));
    assert!(a.options.horizontal_centered && a.options.vertical_centered && a.options.grid_lines);
    assert!(!a.options.headings);
    assert_eq!(a.header_footer.text(HeaderFooterPart::OddHeader), "&LPage &P of &N&C&A&R&D");
    assert_eq!(a.header_footer.text(HeaderFooterPart::OddFooter), "&L&F&CFooter &P/&N&R&D");
    assert_eq!(a.row_break_rows(), vec![19, 34]);
    assert_eq!(a.col_break_cols(), vec![2]);
    assert!(a.row_breaks.iter().all(|b| b.manual && b.max == 3));
    assert!(a.printer_rid.is_some());
    assert_eq!(print_area(&book, 0).len(), 1);
    assert_eq!(print_titles(&book, 0).rows, Some((0, 1)));

    let b = &book.sheets[1].page;
    assert_eq!(
        (b.paper_size, b.scale, b.first_page_number, b.use_first_page_number),
        (9, 85, 3, true)
    );
    assert_eq!(
        (b.page_order, b.black_and_white, b.options.headings),
        (PageOrder::OverThenDown, true, true)
    );
    assert_eq!(b.orientation, Some(Orientation::Portrait));
    assert_eq!(print_titles(&book, 1).cols, Some((0, 0)));
}

#[test]
fn the_page_setup_survives_a_write_and_read() {
    let book = page_setup_book();
    let again =
        unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&book).unwrap()).unwrap();
    for (before, after) in book.sheets.iter().zip(&again.sheets) {
        assert_eq!(before.page, after.page, "{}", before.name);
    }
    assert_eq!(print_area(&book, 0), print_area(&again, 0));
    assert_eq!(print_titles(&book, 1), print_titles(&again, 1));
}

#[test]
fn the_page_setup_elements_are_written_in_the_schemas_order_with_the_printer_part_attached() {
    let bytes = unluminous_sheet::xlsx::write(&page_setup_book()).unwrap();
    let sheet = sheet_xml(&bytes, "xl/worksheets/sheet1.xml");
    let at =
        |tag: &str| sheet.find(&format!("<{tag}")).unwrap_or_else(|| panic!("no {tag} in {sheet}"));
    let order =
        ["printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks"];
    assert!(order.windows(2).all(|w| at(w[0]) < at(w[1])), "{sheet}");
    assert!(sheet.contains("<pageSetUpPr fitToPage=\"1\"/>"), "{sheet}");
    assert!(sheet.contains("<pageSetup paperSize=\"5\" fitToHeight=\"0\" orientation=\"landscape\" r:id=\"rId1\"/>"), "{sheet}");
    assert!(sheet.contains("<brk id=\"19\" max=\"3\" man=\"1\"/>"), "{sheet}");
    let files = unluminous_sheet::zip::read(&bytes).unwrap();
    assert!(
        files.iter().any(|e| e.name.starts_with("xl/printerSettings/")),
        "the printer settings part is kept"
    );
}

#[test]
fn the_defaults_write_only_margins() {
    let mut book = unluminous_sheet::Workbook::new();
    book.sheets[0].page = PageSetup::default();
    let sheet =
        sheet_xml(&unluminous_sheet::xlsx::write(&book).unwrap(), "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>"));
    for absent in
        ["<pageSetup", "<printOptions", "<headerFooter", "<rowBreaks", "<colBreaks", "<pageSetUpPr"]
    {
        assert!(!sheet.contains(absent), "{absent} in {sheet}");
    }
}

#[test]
fn comments_and_errors_are_written_by_their_names() {
    let page = PageSetup {
        comments: PrintComments::AsDisplayed,
        errors: PrintErrors::NotAvailable,
        ..PageSetup::default()
    };
    let mut book = unluminous_sheet::Workbook::new();
    book.sheets[0].page = page.clone();
    let bytes = unluminous_sheet::xlsx::write(&book).unwrap();
    let sheet = sheet_xml(&bytes, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains("cellComments=\"asDisplayed\"") && sheet.contains("errors=\"NA\""),
        "{sheet}"
    );
    assert_eq!(unluminous_sheet::xlsx::read(&bytes).unwrap().sheets[0].page, page);
}

/// Write the page setup file back out for Excel to inspect, when `PAGES_ROUNDTRIP` names where to put it.
#[test]
#[ignore = "writes a file for tools/excel-oracle to compare"]
fn write_the_round_trip_for_excel() {
    let target = std::env::var("PAGES_ROUNDTRIP").expect("PAGES_ROUNDTRIP names the output file");
    let bytes = unluminous_sheet::xlsx::write(&page_setup_book()).unwrap();
    std::fs::write(target, bytes).unwrap();
}
