//! The Watch Window and Workbook Statistics.

mod review_support;

use review_support::*;
use unluminous_sheet::Value;

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn watched(session: &unluminous_sheet::edit::Session) -> Vec<String> {
    session.watch_rows().iter().map(|w| format!("{}!{}", w.sheet_name, w.at.a1())).collect()
}

#[test]
fn the_watch_window_reads_what_excel_wrote() {
    let session = fixture_session("watch.xlsx");
    let expected: Vec<String> =
        serde_json::from_value(fixture_json("watch-expected.json")).unwrap();
    assert_eq!(sorted(watched(&session)), sorted(expected));
    let rows = session.watch_rows();
    let doubled = rows.iter().find(|w| w.sheet_name == "Sheet1" && w.at == at("B2")).unwrap();
    assert_eq!(doubled.name, "Doubled");
    assert_eq!(doubled.value, Value::Number(6.0));
    assert_eq!(doubled.formula, "=A1*2");
    let plain = rows.iter().find(|w| w.at == at("A1") && w.sheet_name == "Sheet1").unwrap();
    assert_eq!((plain.formula.as_str(), plain.name.as_str()), ("", ""));
}

#[test]
fn watches_are_added_deleted_undone_shifted_and_written() {
    use unluminous_sheet::edit::Shift;
    let mut session = fixture_session("watch.xlsx");
    let before = watched(&session);
    session.add_watch(0, at("C5")).unwrap();
    session.add_watch(0, at("C5")).unwrap();
    assert_eq!(watched(&session).len(), before.len() + 1, "a cell is watched once");
    session.insert(0, unluminous_sheet::Range::parse("A1").unwrap(), Shift::WholeRows).unwrap();
    assert!(
        watched(&session).contains(&"Sheet1!C6".to_owned())
            && watched(&session).contains(&"Sheet1!B3".to_owned())
    );
    let bytes = unluminous_sheet::xlsx::write(&session.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.sheets[0].watches, session.book.sheets[0].watches);
    session.delete_watch(0, at("C6")).unwrap();
    assert!(session.delete_watch(0, at("C6")).is_err());
    session.undo();
    assert!(watched(&session).contains(&"Sheet1!C6".to_owned()));
}

#[test]
fn excel_lists_the_watches_the_engine_writes() {
    if !excel_available("what Excel makes of watches written here") {
        return;
    }
    let mut session = fixture_session("watch.xlsx");
    session.add_watch(1, at("B1")).unwrap();
    let scratch = Scratch::new("watch");
    let path = scratch.file("watched.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap();
    let theirs: Vec<String> = serde_json::from_value(excel_script(
        "make-review-watch.ps1",
        &["-Read", "-Path", path.to_str().unwrap()],
    ))
    .unwrap();
    assert_eq!(sorted(theirs), sorted(watched(&session)));
}

#[test]
fn workbook_statistics_count_what_the_workbook_holds() {
    let session = fixture_session("special.xlsx");
    let stats = session.workbook_statistics();
    assert_eq!(stats.sheets, 2);
    assert_eq!(stats.notes, 1);
    assert_eq!(stats.comments, 1);
    assert_eq!(stats.charts, 1);
    assert_eq!(stats.tables, 0);
    let formulas: usize = session
        .book
        .sheets
        .iter()
        .map(|s| s.cells.values().filter(|c| c.formula.is_some()).count())
        .sum();
    assert_eq!(stats.formulas, formulas);
    assert!(stats.cells_with_data > stats.formulas);
    let features = unluminous_sheet::edit::Session::new(
        unluminous_sheet::xlsx::read(include_bytes!("fixtures/features.xlsx")).unwrap(),
        46318.5,
    );
    let stats = features.workbook_statistics();
    assert_eq!((stats.sheets, stats.tables, stats.charts, stats.notes), (3, 1, 1, 1));
}
