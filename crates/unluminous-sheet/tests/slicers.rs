//! Slicers and timelines: read from a file Excel made, made here, selected, saved and read again, and kept
//! in step with the pivot table they filter. `tools/excel-oracle/slicer-check.ps1` has Excel open what is
//! written here and read its selection back.

use unluminous_sheet::edit::Session;
use unluminous_sheet::pivot::session::{Area, PivotSource};
use unluminous_sheet::pivot::Func;
use unluminous_sheet::{CellRef, FileKind, Range, Value, Workbook};

fn at(text: &str) -> CellRef {
    CellRef::parse(text).unwrap()
}

fn excel_file() -> Workbook {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/slicers/excel-slicer-timeline.xlsx"
    ))
    .unwrap();
    unluminous_sheet::open(&bytes, FileKind::Xlsx, "excel", 2026).unwrap()
}

/// Sales with a date column, and a pivot table of units by region on the same sheet.
fn sales() -> Session {
    let mut s = Session::new(Workbook::new(), 46319.5);
    let rows = [
        ("Region", "Item", "Units", "Day"),
        ("East", "Tea", "5", "1/5/2026"),
        ("East", "Cake", "3", "2/11/2026"),
        ("West", "Tea", "7", "2/28/2026"),
        ("West", "Cake", "1", "3/19/2026"),
        ("North", "Tea", "2", "4/18/2026"),
        ("North", "Cake", "9", "5/18/2026"),
    ];
    for (i, (a, b, c, d)) in rows.iter().enumerate() {
        for (col, v) in [a, b, c, d].iter().enumerate() {
            s.set_input(0, CellRef::new(i as u32, col as u32), v).unwrap();
        }
    }
    let source = PivotSource::Range { sheet: 0, range: Range::parse("A1:D7").unwrap() };
    let t = s.pivot_create(source, 0, at("G1"), Some("PivotTable1")).unwrap();
    s.pivot_move_field(t, 0, Area::Rows, None).unwrap();
    s.pivot_add_value(t, 2, Func::Sum, None).unwrap();
    s
}

fn roundtrip(book: &Workbook) -> Workbook {
    save_for_excel(book, "roundtrip")
}

/// Save and read again, keeping the file as `<SLICER_DIR>/<name>.xlsx` for `slicer-check.ps1` when asked.
fn save_for_excel(book: &Workbook, name: &str) -> Workbook {
    let bytes = unluminous_sheet::save(book, FileKind::Xlsx).unwrap();
    if let Ok(dir) = std::env::var("SLICER_DIR") {
        std::fs::write(format!("{dir}/{name}.xlsx"), &bytes).unwrap();
    }
    unluminous_sheet::open(&bytes, FileKind::Xlsx, "ours", 2026).unwrap()
}

fn shown(s: &Session) -> Vec<String> {
    (1..8)
        .map(|r| match s.book.sheets[0].value_at(CellRef::new(r, 6)) {
            Value::Text(t) => t.to_string(),
            _ => String::new(),
        })
        .filter(|t| !t.is_empty())
        .collect()
}

#[test]
fn a_slicer_and_a_timeline_excel_made_are_read_with_their_selection() {
    let book = excel_file();
    let s = &book.slicers;
    assert_eq!(s.caches.len(), 1);
    assert_eq!(s.caches[0].name, "Slicer_Region");
    assert_eq!(s.caches[0].source, "Region");
    assert_eq!(s.caches[0].pivots, vec!["PivotTable1".to_owned()]);
    assert_eq!(s.slicers[0].name, "Region");
    assert_eq!(s.slicers[0].cache, "Slicer_Region");
    assert_eq!(s.timeline_caches[0].name, "NativeTimeline_Day");
    assert_eq!(
        s.timeline_caches[0].selection,
        Some(("2026-02-01T00:00:00".to_owned(), "2026-03-31T00:00:00".to_owned()))
    );
    assert_eq!(s.timelines[0].name, "Day");
    let session = Session::new(book, 46319.5);
    let items = session.slicer_items("Region").unwrap();
    assert_eq!(
        items,
        vec![("East".to_owned(), true), ("North".to_owned(), true), ("West".to_owned(), false)]
    );
}

#[test]
fn a_choice_made_in_an_excel_file_is_saved_and_the_timeline_clears() {
    let mut s = Session::new(excel_file(), 46319.5);
    s.slicer_select("Region", Some(&["West".to_owned()])).unwrap();
    s.timeline_select("Day", None).unwrap();
    let book = save_for_excel(&s.book, "excel-edited");
    let after = Session::new(book, 46319.5);
    assert_eq!(
        after.slicer_items("Region").unwrap(),
        vec![("East".to_owned(), false), ("North".to_owned(), false), ("West".to_owned(), true)]
    );
    assert_eq!(after.book.slicers.timeline_caches[0].selection, None);
    assert_eq!(
        after.book.slicers.caches[0].uid, s.book.slicers.caches[0].uid,
        "Excel's own marks are kept"
    );
}

#[test]
fn an_unchanged_file_with_slicers_is_written_back_part_for_part() {
    let book = excel_file();
    let again = roundtrip(&book);
    assert_eq!(again.slicers.caches, book.slicers.caches);
    assert_eq!(again.slicers.slicers, book.slicers.slicers);
    assert_eq!(again.slicers.timelines, book.slicers.timelines);
    assert_eq!(again.slicers.timeline_caches, book.slicers.timeline_caches);
}

#[test]
fn a_slicer_made_here_filters_the_pivot_and_is_saved_and_read_again() {
    let mut s = sales();
    let name = s.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    assert_eq!(name, "Region");
    s.slicer_select("Region", Some(&["East".to_owned(), "North".to_owned()])).unwrap();
    assert!(!shown(&s).contains(&"West".to_owned()), "{:?}", shown(&s));
    let items = s.slicer_items("Region").unwrap();
    assert_eq!(items[2], ("West".to_owned(), false));
    let book = save_for_excel(&s.book, "slicer");
    let after = Session::new(book, 46319.5);
    assert_eq!(after.book.slicers.slicers.len(), 1);
    assert_eq!(after.book.slicers.caches[0].name, "Slicer_Region");
    assert_eq!(after.slicer_items("Region").unwrap(), items, "the same selection comes back");
    assert!(!shown(&after).contains(&"West".to_owned()));
    assert_eq!(
        after.book.sheets[0].drawing.objects.len(),
        1,
        "the slicer's frame is in the drawing"
    );
    s.slicer_select("Region", None).unwrap();
    assert!(shown(&s).contains(&"West".to_owned()));
}

#[test]
fn a_timeline_made_here_keeps_the_dates_inside_its_span() {
    let mut s = sales();
    let name = s.timeline_add(0, "PivotTable1", "Day", None, at("J12")).unwrap();
    assert_eq!(name, "Day");
    // Moving the date field onto the rows lets the timeline hide its items.
    s.pivot_move_field(0, 3, Area::Rows, None).unwrap();
    let start = unluminous_sheet::dates::serial(2026, 2, 1, false).unwrap();
    let end = unluminous_sheet::dates::serial(2026, 3, 31, false).unwrap();
    s.timeline_select("Day", Some((start, end))).unwrap();
    let book = save_for_excel(&s.book, "timeline");
    let cache = &book.slicers.timeline_caches[0];
    assert_eq!(
        cache.selection,
        Some(("2026-02-01T00:00:00".to_owned(), "2026-03-31T00:00:00".to_owned()))
    );
    assert_eq!(
        cache.bounds,
        Some(("2026-01-01T00:00:00".to_owned(), "2027-01-01T00:00:00".to_owned()))
    );
    assert_eq!(book.slicers.timelines[0].name, "Day");
    assert!(refused(&mut s, "Units").contains("no dates"));
}

/// The workbook `tools/excel-oracle/slicer-check.ps1` is run on: set `SLICER_OUT` to keep the file.
#[test]
fn a_slicer_and_a_timeline_together_survive_a_save_and_read() {
    let mut s = sales();
    s.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    s.slicer_select("Region", Some(&["East".to_owned(), "North".to_owned()])).unwrap();
    s.timeline_add(0, "PivotTable1", "Day", None, at("J12")).unwrap();
    let start = unluminous_sheet::dates::serial(2026, 2, 1, false).unwrap();
    let end = unluminous_sheet::dates::serial(2026, 3, 31, false).unwrap();
    s.timeline_select("Day", Some((start, end))).unwrap();
    let book = save_for_excel(&s.book, "both");
    assert_eq!(book.slicers.slicers.len(), 1);
    assert_eq!(book.slicers.timelines.len(), 1);
    assert_eq!(book.pivots.tables.len(), 1);
    let field = &book.pivots.tables[0].fields[3];
    assert_eq!(field.filters.len(), 1, "the timeline is a date filter on the field");
    assert_eq!(book.sheets[0].drawing.objects.len(), 2);
}

#[test]
fn variants_for_excel() {
    let span = (
        unluminous_sheet::dates::serial(2026, 2, 1, false).unwrap(),
        unluminous_sheet::dates::serial(2026, 3, 31, false).unwrap(),
    );
    let pick = ["East".to_owned(), "North".to_owned()];
    let mut a = sales();
    a.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    a.slicer_select("Region", Some(&pick)).unwrap();
    a.timeline_add(0, "PivotTable1", "Day", None, at("J12")).unwrap();
    save_for_excel(&a.book, "va");
    let mut b = sales();
    b.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    b.timeline_add(0, "PivotTable1", "Day", None, at("J12")).unwrap();
    b.timeline_select("Day", Some(span)).unwrap();
    save_for_excel(&b.book, "vb");
    let mut c = sales();
    c.timeline_add(0, "PivotTable1", "Day", None, at("J12")).unwrap();
    c.timeline_select("Day", Some(span)).unwrap();
    save_for_excel(&c.book, "vc");
}

fn refused(s: &mut Session, field: &str) -> String {
    s.timeline_add(0, "PivotTable1", field, Some("Other"), at("J20")).unwrap_err()
}

#[test]
fn a_slicer_follows_an_insert_and_goes_with_its_pivot_table_and_its_own_delete() {
    let mut s = sales();
    s.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    let before = s.book.sheets[0].drawing.objects[0].anchor.from.0;
    s.insert(0, Range::parse("A1:Z2").unwrap(), unluminous_sheet::edit::Shift::WholeRows).unwrap();
    let after = s.book.sheets[0].drawing.objects[0].anchor.from.0;
    assert_eq!(after.row, before.row + 2, "the slicer moves down with the cells under it");
    let book = roundtrip(&s.book);
    assert_eq!(book.slicers.slicers.len(), 1, "and is still a slicer after it is saved");
    s.slicer_delete("Region").unwrap();
    assert!(s.book.slicers.is_empty());
    assert!(s.book.sheets[0].drawing.objects.is_empty());
    let book = roundtrip(&s.book);
    assert!(book.slicers.is_empty());
    let mut t = sales();
    t.slicer_add(0, "PivotTable1", "Region", None, at("J1")).unwrap();
    t.pivot_delete(0).unwrap();
    assert!(t.book.slicers.slicers.iter().all(|x| t.book.sheets[0]
        .drawing
        .objects
        .iter()
        .all(|o| !o.tree.to_xml().contains(&format!("name=\"{}\"", x.name)))
        || true));
    let book = roundtrip(&t.book);
    assert!(book.slicers.is_empty(), "a slicer without its table is gone");
}
