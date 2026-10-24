//! Named cell styles against Excel: each of the 47 built in styles gives a cell what Excel gives it, a style
//! made from a cell, changed or deleted changes the cells that use it as Excel changes them, and
//! the styles of another workbook merge in as Excel merges them.
//!
//! The fixtures were made by `tools/excel-oracle/style-fixtures.ps1`.

mod describe;

use describe::*;
use unluminous_sheet::cellstyles::Includes;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::styles::{Color, FontEdit, StyleChange};

const LANE: &str = "styles";
const NOW: f64 = 46319.5;

fn rg(a: &str) -> Range {
    Range::parse(a).unwrap()
}

fn session(name: &str) -> Session {
    Session::new(read_fixture(LANE, name), NOW)
}

/// Compare a workbook with the one Excel saved, cell by cell and format by format.
fn assert_same_as_excel(book: &Workbook, fixture: &str) {
    let excel = read_fixture(LANE, fixture);
    let ours = describe(book);
    let theirs = describe(&excel);
    assert_eq!(ours.len(), theirs.len(), "{fixture}: cells\n{}", differences(&ours, &theirs));
    for line in &ours {
        assert!(theirs.contains(line), "{fixture}\n{}", differences(&ours, &theirs));
    }
}

/// The names of the styles a workbook defines, with whether Excel marks each as changed.
fn defined_styles(book: &Workbook) -> Vec<(String, Option<u32>, bool, bool)> {
    let mut v: Vec<_> = book
        .styles
        .cell_styles
        .iter()
        .map(|s| (s.name.clone(), s.builtin_id, s.hidden, s.custom_builtin))
        .collect();
    v.sort();
    v
}

#[test]
fn every_built_in_style_gives_a_cell_what_excel_gives_it() {
    let mut s = session("styles-all-input.xlsx");
    let names: Vec<String> =
        s.style_gallery().into_iter().filter(|i| i.builtin_id.is_some()).map(|i| i.name).collect();
    assert_eq!(names.len(), 47);
    for (i, name) in names.iter().enumerate() {
        // Excel's workbook has the names in the first column in the order the script lists them, which
        // is the gallery's order but for Normal being first; find the row by the text.
        let row = s.book.sheets[0]
            .cells
            .iter()
            .find(|(_, c)| c.value.to_text() == *name)
            .map(|(k, _)| k.0)
            .unwrap_or_else(|| panic!("row for {name} ({i})"));
        let range = Range::new(CellRef::new(row, 0), CellRef::new(row, 1));
        s.apply_style(0, &[range], name).unwrap();
    }
    assert_same_as_excel(&s.book, "styles-all.xlsx");
    let excel = read_fixture(LANE, "styles-all.xlsx");
    assert_eq!(defined_styles(&s.book), defined_styles(&excel));
    // Each style's own definition is the one Excel wrote.
    for style in &excel.styles.cell_styles {
        let ours = s.book.styles.style_index(&style.name).unwrap();
        let (ox, ex) = (
            &s.book.styles.style_xfs[s.book.styles.cell_styles[ours].xf_id as usize],
            &excel.styles.style_xfs[style.xf_id as usize],
        );
        assert_eq!(Includes::of(ox), Includes::of(ex), "{} includes", style.name);
        assert_eq!(
            s.book.styles.code_for_id(ox.num_fmt),
            excel.styles.code_for_id(ex.num_fmt),
            "{} number format",
            style.name
        );
        assert_eq!(ox.locked, ex.locked, "{} protection", style.name);
        assert_eq!(ox.align, ex.align, "{} alignment", style.name);
    }
}

#[test]
fn a_style_replaces_direct_formatting_in_the_groups_it_includes() {
    let mut s = session("styles-direct-input.xlsx");
    s.apply_style(0, &[rg("A1:A3")], "Good").unwrap();
    assert_same_as_excel(&s.book, "styles-direct-good.xlsx");
    s.apply_style(0, &[rg("A1:A3")], "Normal").unwrap();
    assert_same_as_excel(&s.book, "styles-direct-normal.xlsx");
}

#[test]
fn a_style_made_applied_changed_and_deleted_does_what_excel_does() {
    let mut s = session("styles-custom-input.xlsx");
    s.new_style("My Style", Some((0, CellRef::parse("A1").unwrap())), Includes::ALL).unwrap();
    s.apply_style(0, &[rg("A2:A3")], "My Style").unwrap();
    s.set_style(
        0,
        &[rg("A3")],
        &StyleChange::Font(FontEdit { italic: Some(true), ..Default::default() }),
    )
    .unwrap();
    s.apply_style(0, &[rg("A4")], "Good").unwrap();
    assert_same_as_excel(&s.book, "styles-custom-applied.xlsx");
    s.modify_style(
        "My Style",
        &[
            StyleChange::Font(FontEdit {
                color: Some(Color::rgb(0, 0, 255)),
                bold: Some(false),
                ..Default::default()
            }),
            StyleChange::NumberFormat("0.0".to_owned()),
            StyleChange::Fill(Some(Color::rgb(0, 255, 0))),
        ],
        None,
    )
    .unwrap();
    assert_same_as_excel(&s.book, "styles-custom-modified.xlsx");
    s.delete_style("My Style").unwrap();
    assert_same_as_excel(&s.book, "styles-custom-deleted.xlsx");
    assert!(s.book.styles.style_index("My Style").is_none());
    s.delete_style("Good").unwrap();
    assert_same_as_excel(&s.book, "styles-builtin-deleted.xlsx");
    let good = &s.book.styles.cell_styles[s.book.styles.style_index("Good").unwrap()];
    assert!(good.hidden, "a deleted built in style is hidden, as Excel writes it");
    assert_eq!(s.delete_style("Normal").unwrap_err(), "You can't delete the Normal style.");
    assert!(s.delete_style("Nope").is_err());
}

#[test]
fn undo_puts_a_deleted_style_and_its_cells_back() {
    let mut s = session("styles-custom-input.xlsx");
    s.new_style("My Style", Some((0, CellRef::parse("A1").unwrap())), Includes::ALL).unwrap();
    s.apply_style(0, &[rg("A2")], "My Style").unwrap();
    let before = describe(&s.book);
    s.delete_style("My Style").unwrap();
    assert_ne!(describe(&s.book), before);
    s.undo();
    assert_eq!(describe(&s.book), before);
    s.undo();
    s.undo();
    assert!(s.book.styles.style_index("My Style").is_none());
}

#[test]
fn changing_a_built_in_style_changes_its_cells_and_marks_it_as_excel_does() {
    let mut s = session("styles-modify-input.xlsx");
    s.modify_style(
        "Good",
        &[StyleChange::Font(FontEdit {
            color: Some(Color::rgb(255, 0, 0)),
            bold: Some(true),
            ..Default::default()
        })],
        None,
    )
    .unwrap();
    assert_same_as_excel(&s.book, "styles-modify-builtin.xlsx");
    let excel = read_fixture(LANE, "styles-modify-builtin.xlsx");
    assert_eq!(defined_styles(&s.book), defined_styles(&excel));
}

#[test]
fn merging_styles_replaces_the_ones_with_the_same_name_as_excel_does() {
    let mut s = session("styles-merge-dst-input.xlsx");
    let src = read_fixture(LANE, "styles-merge-src.xlsx");
    let brought = s.merge_styles(&src).unwrap();
    assert!(brought >= 3, "{brought}");
    assert_same_as_excel(&s.book, "styles-merge-dst.xlsx");
    let excel = read_fixture(LANE, "styles-merge-dst.xlsx");
    assert_eq!(defined_styles(&s.book), defined_styles(&excel));
}

#[test]
fn styles_survive_a_file_and_the_flags_are_worked_out_from_the_style() {
    let mut s = session("styles-direct-input.xlsx");
    s.apply_style(0, &[rg("A1:A3")], "Good").unwrap();
    s.apply_style(0, &[rg("A2")], "Comma").unwrap();
    s.set_style(
        0,
        &[rg("A1")],
        &StyleChange::Font(FontEdit { italic: Some(true), ..Default::default() }),
    )
    .unwrap();
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(describe(&back), describe(&s.book));
    // Excel writes no apply flag for a group that has its style's value, and a flag for one that has not.
    let styles_xml = String::from_utf8(zip_part(&bytes, "xl/styles.xml")).unwrap();
    assert!(styles_xml.contains("<cellStyle name=\"Good\" xfId="));
    assert!(styles_xml.contains("applyFont=\"1\""), "{styles_xml}");
}

/// The bytes of one part of a zip.
fn zip_part(bytes: &[u8], name: &str) -> Vec<u8> {
    unluminous_sheet::zip::read(bytes)
        .unwrap()
        .into_iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} is not in the file"))
        .data
}

#[test]
fn a_protected_sheet_refuses_a_style_unless_formatting_is_allowed() {
    use unluminous_sheet::protect::SheetProtection;
    let mut s = session("styles-direct-input.xlsx");
    s.protect_sheet(0, None, &SheetProtection::default()).unwrap();
    assert!(s.apply_style(0, &[rg("A1")], "Good").is_err());
    s.unprotect_sheet(0, None).unwrap();
    let mut allow = SheetProtection::default();
    allow.allow.format_cells = true;
    s.protect_sheet(0, None, &allow).unwrap();
    s.apply_style(0, &[rg("A1")], "Good").unwrap();
}

// ---------------------------------------------------------------------------------------------------
// Excel opens what the engine writes.

fn inspect_styles(path: &std::path::Path) -> serde_json::Value {
    let text = run_script("inspect-styles.ps1", &["-Path", path.to_str().unwrap()]);
    serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}'))
        .unwrap_or_else(|e| panic!("Excel's answer did not read: {e}\n{text}"))
}

#[test]
fn excel_reads_the_styles_unluminous_writes_as_the_same_styles() {
    if !have_excel("what Excel makes of the styles written here") {
        return;
    }
    let mut s = session("styles-all-input.xlsx");
    for name in [
        "Good",
        "Bad",
        "Heading 1",
        "Title",
        "Accent2",
        "20% - Accent3",
        "Comma",
        "Percent",
        "Note",
    ] {
        let row = s.book.sheets[0]
            .cells
            .iter()
            .find(|(_, c)| c.value.to_text() == name)
            .map(|(k, _)| k.0)
            .unwrap();
        s.apply_style(0, &[Range::new(CellRef::new(row, 0), CellRef::new(row, 1))], name).unwrap();
    }
    s.new_style("Plain Number", None, Includes { number: false, font: false, ..Includes::ALL })
        .unwrap();
    let row = s.book.sheets[0]
        .cells
        .iter()
        .find(|(_, c)| c.value.to_text() == "Normal")
        .map(|(k, _)| k.0)
        .unwrap();
    s.apply_style(0, &[Range::new(CellRef::new(row, 0), CellRef::new(row, 1))], "Plain Number")
        .unwrap();
    let dir = scratch("styles");
    let path = dir.join("built.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let excel = inspect_styles(&path);
    for cell in excel["cells"].as_array().unwrap() {
        let at = CellRef::parse(cell["cell"].as_str().unwrap()).unwrap();
        let ours = s.style_of(0, at);
        assert_eq!(cell["style"].as_str().unwrap(), ours, "{at}");
    }
    let plain = excel["styles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|st| st["name"] == "Plain Number")
        .expect("Excel lists the style made here");
    assert_eq!(plain["number"], false);
    assert_eq!(plain["font"], false);
    assert_eq!(plain["border"], true);
    // And the built in ones carry the groups Excel's own definition includes.
    let note = excel["styles"].as_array().unwrap().iter().find(|st| st["name"] == "Note").unwrap();
    assert_eq!(note["font"], false);
    assert_eq!(note["border"], true);
}
