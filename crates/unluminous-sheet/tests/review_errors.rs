//! Error Checking against `Range.Errors` in Excel, on the workbook `make-review-errors.ps1` built.

mod review_support;

use std::collections::BTreeMap;

use review_support::*;
use unluminous_sheet::edit::review::errors::{ErrorOptions, ErrorRule};

fn flags_here(session: &unluminous_sheet::edit::Session) -> BTreeMap<String, Vec<u64>> {
    let mut map: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for f in session.check_errors(None, None, ErrorOptions::all()) {
        map.entry(f.at.a1()).or_default().push(u64::from(f.rule.number()));
    }
    for rules in map.values_mut() {
        rules.sort_unstable();
    }
    map
}

#[test]
fn every_rule_flags_the_cells_excel_flags() {
    // Excel's `Errors.Item(i).Value` is true for an error that was ignored too; ignoring is its own flag.
    let mut session = fixture_session("errors.xlsx");
    session.book.sheets[0].ignored_errors.clear();
    let expected = fixture_json("errors-expected.json");
    let theirs: BTreeMap<String, Vec<u64>> = expected["flags"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            let mut rules: Vec<u64> = match v {
                serde_json::Value::Array(a) => a.iter().map(|n| n.as_u64().unwrap()).collect(),
                other => vec![other.as_u64().unwrap()],
            };
            rules.sort_unstable();
            (k.clone(), rules)
        })
        .collect();
    let ours = flags_here(&session);
    let mut wrong = Vec::new();
    for key in theirs.keys().chain(ours.keys()).collect::<std::collections::BTreeSet<_>>() {
        if theirs.get(key) != ours.get(key) {
            wrong.push(format!("{key}: Excel {:?}, here {:?}", theirs.get(key), ours.get(key)));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_ignored_error_is_read_written_and_left_out() {
    let session = fixture_session("errors.xlsx");
    let sheet = &session.book.sheets[0];
    assert_eq!(sheet.ignored_errors.len(), 2, "{:?}", sheet.ignored_errors);
    let ignored = fixture_json("errors-expected.json")["ignored"].clone();
    for (cell, number) in ignored.as_object().unwrap() {
        let rule = ErrorRule::ALL
            .iter()
            .find(|r| u64::from(r.number()) == number.as_u64().unwrap())
            .unwrap();
        assert!(
            sheet.ignored_errors.iter().any(|row| {
                row.ranges.iter().any(|r| r.contains(at(cell)))
                    && row.rules.iter().any(|a| a == rule.attribute())
            }),
            "{cell}"
        );
    }
    let at_b3 = session.check_errors(
        None,
        Some(unluminous_sheet::Range::parse("B3").unwrap()),
        ErrorOptions::all(),
    );
    assert!(at_b3.iter().all(|f| f.rule != ErrorRule::InconsistentFormula));
    let bytes = unluminous_sheet::xlsx::write(&session.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    assert_eq!(back.sheets[0].ignored_errors, sheet.ignored_errors);
}

#[test]
fn ignoring_an_error_is_one_undo_step_and_survives_a_row_insert() {
    use unluminous_sheet::edit::Shift;
    let mut session = fixture_session("errors.xlsx");
    let before = session.check_errors(None, None, ErrorOptions::all()).len();
    let range = unluminous_sheet::Range::parse("E5").unwrap();
    assert!(session
        .check_errors(None, Some(range), ErrorOptions::all())
        .iter()
        .any(|f| f.rule == ErrorRule::InconsistentFormula));
    session.ignore_error(0, range, ErrorRule::InconsistentFormula).unwrap();
    assert_eq!(session.check_errors(None, None, ErrorOptions::all()).len(), before - 1);
    session.insert(0, unluminous_sheet::Range::parse("A1").unwrap(), Shift::WholeRows).unwrap();
    assert!(session.book.sheets[0]
        .ignored_errors
        .iter()
        .any(|r| r.ranges == [unluminous_sheet::Range::parse("E6").unwrap()]));
    session.undo();
    session.undo();
    assert_eq!(session.check_errors(None, None, ErrorOptions::all()).len(), before);
    session.ignore_error(0, range, ErrorRule::InconsistentFormula).unwrap();
    session.reset_ignored_errors(0).unwrap();
    assert!(session.book.sheets[0].ignored_errors.is_empty());
}

#[test]
fn excel_opens_the_ignored_errors_the_engine_writes() {
    if !excel_available("what Excel makes of ignored errors written here") {
        return;
    }
    let mut session = fixture_session("errors.xlsx");
    session
        .ignore_error(
            0,
            unluminous_sheet::Range::parse("E5").unwrap(),
            ErrorRule::InconsistentFormula,
        )
        .unwrap();
    let scratch = Scratch::new("errors");
    let path = scratch.file("ignored.xlsx");
    std::fs::write(&path, unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap();
    let theirs = excel_script("review-read.ps1", &["-Path", path.to_str().unwrap()]);
    assert_eq!(theirs["repaired"], false);
}

#[test]
fn only_a_typed_number_beside_the_range_is_an_omitted_cell() {
    // What Excel answered through `Range.Errors.Item(5)` for `=AVERAGE(C5:C9)` in C11 (task-2378): a
    // total worked out by a formula in C10 is not flagged, and the same number typed in is.
    let omits = |below: &str| {
        let mut s =
            unluminous_sheet::edit::Session::new(unluminous_sheet::model::Workbook::new(), 46318.5);
        for row in 5..=9u32 {
            s.set_input(0, at(&format!("C{row}")), &format!("{}", row * 2)).unwrap();
        }
        s.set_input(0, at("C10"), below).unwrap();
        s.set_input(0, at("C11"), "=AVERAGE(C5:C9)").unwrap();
        s.set_input(0, at("E5"), "=INDEX(C5:C9,1)").unwrap();
        let flagged = |cell: &str| {
            s.check_errors(
                None,
                Some(unluminous_sheet::Range::parse(cell).unwrap()),
                ErrorOptions::all(),
            )
            .iter()
            .any(|f| f.rule == ErrorRule::OmittedCells)
        };
        (flagged("C11"), flagged("E5"))
    };
    assert_eq!(omits("=SUM(C5:C9)"), (false, false));
    assert_eq!(omits("=1+1"), (false, false));
    assert_eq!(omits("70"), (true, true));
}
