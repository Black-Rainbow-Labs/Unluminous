//! Changes to pivot tables from the sheet: each is one undo step, the table follows the sheet around it,
//! and what is saved is a file Excel's parts are right in.

use std::collections::BTreeSet;

use unluminous_sheet::edit::{Session, Shift};
use unluminous_sheet::pivot::session::{Area, PivotSource, ValueSettings};
use unluminous_sheet::pivot::{Func, ShowAs};
use unluminous_sheet::{CellRef, Range, Value, Workbook};

fn at(text: &str) -> CellRef {
    CellRef::parse(text).unwrap()
}

/// A workbook with sales in A1:C7 of `Data` and an empty `Report`.
fn session() -> Session {
    let mut book = Workbook::new();
    book.sheets[0].name = "Data".to_owned();
    book.sheets.push(unluminous_sheet::Sheet::new("Report"));
    let mut s = Session::new(book, 46319.5);
    let rows = [
        ("Region", "Product", "Units"),
        ("North", "Apples", "10"),
        ("South", "Pears", "20"),
        ("North", "Pears", "30"),
        ("East", "Apples", "40"),
        ("South", "Apples", "50"),
        ("East", "Pears", "60"),
    ];
    for (i, (a, b, c)) in rows.iter().enumerate() {
        s.set_input(0, CellRef::new(i as u32, 0), a).unwrap();
        s.set_input(0, CellRef::new(i as u32, 1), b).unwrap();
        s.set_input(0, CellRef::new(i as u32, 2), c).unwrap();
    }
    s
}

fn source() -> PivotSource {
    PivotSource::Range { sheet: 0, range: Range::parse("A1:C7").unwrap() }
}

fn number(s: &Session, sheet: usize, cell: &str) -> f64 {
    match s.book.sheets[sheet].value_at(at(cell)) {
        Value::Number(n) => n,
        other => panic!("{cell} holds {other:?}"),
    }
}

fn text(s: &Session, sheet: usize, cell: &str) -> String {
    match s.book.sheets[sheet].value_at(at(cell)) {
        Value::Text(t) => t.to_string(),
        other => format!("{other:?}"),
    }
}

/// Regions down the rows, the sum of units across.
fn regions(s: &mut Session) -> usize {
    let t = s.pivot_create(source(), 1, at("A3"), None).unwrap();
    s.pivot_move_field(t, 0, Area::Rows, None).unwrap();
    s.pivot_add_value(t, 2, Func::Sum, None).unwrap();
    t
}

fn part_names(book: &Workbook) -> BTreeSet<String> {
    let bytes = unluminous_sheet::xlsx::write(book).unwrap();
    unluminous_sheet::zip::read(&bytes).unwrap().into_iter().map(|e| e.name).collect()
}

#[test]
fn a_new_pivot_table_fills_the_sheet_and_undo_empties_it() {
    let mut s = session();
    let t = regions(&mut s);
    assert_eq!(s.book.pivots.tables[t].location.a1(), "A3:B7");
    assert_eq!(text(&s, 1, "A3"), "Row Labels");
    assert_eq!(text(&s, 1, "B3"), "Sum of Units");
    assert_eq!(text(&s, 1, "A4"), "East");
    assert_eq!(number(&s, 1, "B4"), 100.0);
    assert_eq!(number(&s, 1, "B7"), 210.0);
    s.undo().unwrap();
    assert!(s.book.sheets[1].value_at(at("B4")).is_empty(), "undo takes the value field out");
    assert_eq!(text(&s, 1, "A4"), "East");
    s.undo().unwrap();
    assert!(s.book.sheets[1].value_at(at("A4")).is_empty(), "undo takes the row field out");
    s.redo().unwrap();
    s.redo().unwrap();
    assert_eq!(number(&s, 1, "B7"), 210.0);
    s.undo().unwrap();
    s.undo().unwrap();
    s.undo().unwrap();
    assert!(s.book.pivots.tables.is_empty(), "undo takes the table away");
}

#[test]
fn a_change_to_the_source_shows_after_a_refresh_and_not_before() {
    let mut s = session();
    let t = regions(&mut s);
    s.set_input(0, at("C2"), "1000").unwrap();
    assert_eq!(number(&s, 1, "B5"), 40.0, "North is still 40 until the refresh");
    s.pivot_refresh(t).unwrap();
    assert_eq!(number(&s, 1, "B5"), 1030.0);
    s.undo().unwrap();
    assert_eq!(number(&s, 1, "B5"), 40.0);
}

#[test]
fn value_field_settings_change_what_is_shown_and_how() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_value_settings(t, 0, ValueSettings { show: Some(ShowAs::PercentOfTotal), ..Default::default() }).unwrap();
    assert!((number(&s, 1, "B4") - 100.0 / 210.0).abs() < 1e-12);
    let cell = s.book.sheets[1].cell(at("B4")).unwrap();
    assert_eq!(s.book.styles.format_code(cell.style), "0.00%");
    s.pivot_value_settings(t, 0, ValueSettings { func: Some(Func::Count), show: Some(ShowAs::Normal), ..Default::default() }).unwrap();
    assert_eq!(text(&s, 1, "B3"), "Count of Units");
    assert_eq!(number(&s, 1, "B4"), 2.0);
}

#[test]
fn a_pivot_table_follows_rows_inserted_above_it_and_above_its_source() {
    let mut s = session();
    let t = regions(&mut s);
    s.insert(1, Range::parse("A1:A2").unwrap(), Shift::WholeRows).unwrap();
    assert_eq!(s.book.pivots.tables[t].location.a1(), "A5:B9");
    assert_eq!(text(&s, 1, "A5"), "Row Labels");
    s.insert(0, Range::parse("A1:A3").unwrap(), Shift::WholeRows).unwrap();
    s.set_input(0, at("C5"), "7").unwrap();
    s.pivot_refresh(t).unwrap();
    assert_eq!(s.book.pivots.caches[0].source, unluminous_sheet::pivot::CacheSource::Range { sheet: "Data".into(), range: Range::parse("A4:C10").unwrap() });
    assert_eq!(number(&s, 1, "B9"), 210.0 - 10.0 + 7.0);
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.pivots.tables[0].location.a1(), "A5:B9");
    assert_eq!(
        back.pivots.caches[0].source,
        unluminous_sheet::pivot::CacheSource::Range { sheet: "Data".into(), range: Range::parse("A4:C10").unwrap() }
    );
}

#[test]
fn rows_cannot_be_inserted_or_deleted_through_a_pivot_table() {
    let mut s = session();
    regions(&mut s);
    let err = s.insert(1, Range::parse("A5:A5").unwrap(), Shift::WholeRows).unwrap_err();
    assert!(err.contains("PivotTable"), "{err}");
    assert!(s.delete(1, Range::parse("A5:A5").unwrap(), Shift::WholeRows).is_err());
    assert!(s.insert(1, Range::parse("A3:A3").unwrap(), Shift::WholeRows).is_ok(), "above the table is fine");
}

#[test]
fn renaming_the_source_sheet_keeps_the_pivot_table_readable() {
    let mut s = session();
    let t = regions(&mut s);
    s.rename_sheet(0, "Sales").unwrap();
    s.pivot_refresh(t).unwrap();
    assert_eq!(number(&s, 1, "B7"), 210.0);
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let definition = unluminous_sheet::zip::read(&bytes)
        .unwrap()
        .into_iter()
        .find(|e| e.name.contains("pivotCacheDefinition"))
        .map(|e| String::from_utf8(e.data).unwrap())
        .unwrap();
    assert!(definition.contains("sheet=\"Sales\""), "{definition}");
}

#[test]
fn deleting_the_sheet_a_pivot_is_on_takes_its_parts_out_of_the_file() {
    let mut s = session();
    regions(&mut s);
    assert!(part_names(&s.book).iter().any(|n| n.contains("pivotTable")));
    s.delete_sheet(1).unwrap();
    assert!(s.book.pivots.tables.is_empty());
    let names = part_names(&s.book);
    assert!(!names.iter().any(|n| n.contains("pivot")), "{names:?}");
    s.undo().unwrap();
    assert!(part_names(&s.book).iter().any(|n| n.contains("pivotTable")));
}

#[test]
fn deleting_a_pivot_table_clears_its_cells_and_its_parts() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_delete(t).unwrap();
    assert!(s.book.sheets[1].value_at(at("A4")).is_empty());
    assert!(!part_names(&s.book).iter().any(|n| n.contains("pivot")));
    let book = unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    assert!(book.pivots.tables.is_empty());
    assert!(!book.raw.iter().any(|(n, _)| n == "pivotCaches"));
}

#[test]
fn two_pivot_tables_cannot_overlap() {
    let mut s = session();
    regions(&mut s);
    let err = s.pivot_create(source(), 1, at("B5"), None).unwrap_err();
    assert!(err.contains("overlap"), "{err}");
    let t2 = s.pivot_create(PivotSource::Cache(0), 1, at("E3"), None).unwrap();
    s.pivot_move_field(t2, 1, Area::Rows, None).unwrap();
    let err = s.pivot_move(t2, 1, at("A4")).unwrap_err();
    assert!(err.contains("overlap"), "{err}");
}

#[test]
fn a_pivot_table_moves_and_leaves_nothing_behind() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_move(t, 1, at("D10")).unwrap();
    assert!(s.book.sheets[1].value_at(at("A3")).is_empty());
    assert_eq!(text(&s, 1, "D10"), "Row Labels");
    assert_eq!(number(&s, 1, "E14"), 210.0);
}

#[test]
fn getpivotdata_reads_the_figure_and_follows_a_refresh() {
    let mut s = session();
    let t = regions(&mut s);
    s.set_input(1, at("E1"), "=GETPIVOTDATA(\"Units\",$A$3,\"Region\",\"North\")").unwrap();
    s.set_input(1, at("E2"), "=GETPIVOTDATA(\"Units\",$A$3)").unwrap();
    s.set_input(1, at("E3"), "=GETPIVOTDATA(\"Units\",$A$3,\"Region\",\"Nowhere\")").unwrap();
    assert_eq!(number(&s, 1, "E1"), 40.0);
    assert_eq!(number(&s, 1, "E2"), 210.0);
    assert_eq!(s.book.sheets[1].value_at(at("E3")), Value::Error(unluminous_sheet::ErrorKind::Ref));
    s.set_input(0, at("C2"), "1000").unwrap();
    s.pivot_refresh(t).unwrap();
    assert_eq!(number(&s, 1, "E1"), 1030.0);
    assert_eq!(number(&s, 1, "E2"), 1200.0);
}

#[test]
fn sorting_and_filtering_are_one_undo_step_each() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_sort(t, 0, unluminous_sheet::pivot::SortType::Descending, Some(0)).unwrap();
    assert_eq!((text(&s, 1, "A4"), text(&s, 1, "A5")), ("East".into(), "South".into()));
    s.pivot_set_item_visible(t, 0, 0, false).unwrap();
    assert!(!(4..8).any(|r| text(&s, 1, &format!("A{r}")) == "East"), "East is hidden");
    assert_eq!(number(&s, 1, "B6"), 110.0, "the grand total leaves the hidden item out");
    s.undo().unwrap();
    assert_eq!(text(&s, 1, "A4"), "East");
    s.undo().unwrap();
    assert_eq!(text(&s, 1, "A5"), "North");
}

#[test]
fn changing_the_source_reads_the_new_range() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_change_source(t, PivotSource::Range { sheet: 0, range: Range::parse("A1:C4").unwrap() }).unwrap();
    let t = 0;
    s.pivot_move_field(t, 0, Area::Rows, None).unwrap();
    s.pivot_add_value(t, 2, Func::Sum, None).unwrap();
    assert_eq!(number(&s, 1, "B6"), 60.0, "only the first three records are read");
}

#[test]
fn the_written_file_names_its_pivot_parts_where_excel_looks() {
    let mut s = session();
    regions(&mut s);
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let entries = unluminous_sheet::zip::read(&bytes).unwrap();
    let get = |name: &str| -> String {
        String::from_utf8(entries.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name}")).data.clone()).unwrap()
    };
    let workbook = get("xl/workbook.xml");
    assert!(workbook.contains("<pivotCaches><pivotCache cacheId=\"1\" r:id=\""), "{workbook}");
    let rels = get("xl/_rels/workbook.xml.rels");
    assert!(rels.contains("pivotCacheDefinition1.xml"), "{rels}");
    let sheet_rels = entries.iter().filter(|e| e.name.starts_with("xl/worksheets/_rels/")).map(|e| String::from_utf8_lossy(&e.data).into_owned()).collect::<String>();
    assert!(sheet_rels.contains("../pivotTables/pivotTable1.xml"), "{sheet_rels}");
    let types = get("[Content_Types].xml");
    for part in ["pivotTable1", "pivotCacheDefinition1", "pivotCacheRecords1"] {
        assert!(types.contains(part), "{part} has a content type");
    }
    assert!(get("xl/pivotTables/_rels/pivotTable1.xml.rels").contains("../pivotCache/pivotCacheDefinition1.xml"));
    assert!(get("xl/pivotCache/_rels/pivotCacheDefinition1.xml.rels").contains("pivotCacheRecords1.xml"));
    let table = get("xl/pivotTables/pivotTable1.xml");
    assert!(table.contains("<location ref=\"A3:B7\""), "{table}");
}

#[test]
fn grouping_and_ungrouping_are_undoable() {
    let mut s = session();
    let t = regions(&mut s);
    s.pivot_group_items(t, 0, &[(None, vec![0, 1])]).unwrap();
    assert!(text(&s, 1, "A4").starts_with("Group") || (4..12).any(|r| text(&s, 1, &format!("A{r}")).starts_with("Group")));
    s.undo().unwrap();
    assert!(!(4..12).any(|r| text(&s, 1, &format!("A{r}")).starts_with("Group")));
}
