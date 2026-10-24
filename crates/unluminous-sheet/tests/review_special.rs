//! Go To Special and the traces against Excel's `SpecialCells`, `CurrentRegion`, `RowDifferences`,
//! `Precedents` and the rest, on the workbook `make-review-special.ps1` built.

mod review_support;

use review_support::*;
use unluminous_sheet::edit::review::special::{areas_of, Kinds, Special};
use unluminous_sheet::{CellRef, Range};

fn kinds(numbers: bool, text: bool, logicals: bool, errors: bool) -> Kinds {
    Kinds { numbers, text, logicals, errors }
}

fn special_of(kind: &str) -> Option<Special> {
    Some(match kind {
        "notes" => Special::Notes,
        "constants" => Special::Constants(Kinds::ALL),
        "constants-numbers" => Special::Constants(kinds(true, false, false, false)),
        "constants-text" => Special::Constants(kinds(false, true, false, false)),
        "constants-logical" => Special::Constants(kinds(false, false, true, false)),
        "constants-errors" => Special::Constants(kinds(false, false, false, true)),
        "formulas" => Special::Formulas(Kinds::ALL),
        "formulas-numbers" => Special::Formulas(kinds(true, false, false, false)),
        "formulas-text" => Special::Formulas(kinds(false, true, false, false)),
        "formulas-logical" => Special::Formulas(kinds(false, false, true, false)),
        "formulas-errors" => Special::Formulas(kinds(false, false, false, true)),
        "blanks" => Special::Blanks,
        "visible" => Special::VisibleCells,
        "last" => Special::LastCell,
        "cf-all" => Special::ConditionalFormats { same: false },
        "cf-same" => Special::ConditionalFormats { same: true },
        "validation-all" => Special::Validation { same: false },
        "validation-same" => Special::Validation { same: true },
        "current-region" => Special::CurrentRegion,
        "current-array" => Special::CurrentArray,
        "row-differences" => Special::RowDifferences,
        "column-differences" => Special::ColumnDifferences,
        "precedents-direct" => Special::Precedents { all_levels: false },
        "precedents-all" => Special::Precedents { all_levels: true },
        "dependents-direct" => Special::Dependents { all_levels: false },
        "dependents-all" => Special::Dependents { all_levels: true },
        _ => return None,
    })
}

#[test]
fn every_option_selects_the_cells_excel_selects() {
    let session = fixture_session("special.xlsx");
    let expected = fixture_json("special-expected.json");
    let rows = expected["rows"].as_array().unwrap();
    let mut wrong = Vec::new();
    for row in rows {
        let kind = row["kind"].as_str().unwrap();
        let selection = Range::parse(row["selection"].as_str().unwrap()).unwrap();
        let active = at(row["active"].as_str().unwrap());
        let ours = session.special_cells(0, &[selection], active, special_of(kind).unwrap());
        let theirs: Option<Vec<CellRef>> = row["cells"].as_array().map(|a| {
            let mut v: Vec<CellRef> = a.iter().map(|c| at(c.as_str().unwrap())).collect();
            v.sort();
            v.dedup();
            v
        });
        let same = match (&ours, &theirs) {
            (Ok(o), Some(t)) => o == t,
            (Err(_), None) => true,
            _ => false,
        };
        if !same {
            let show = |v: &[CellRef]| v.iter().map(CellRef::a1).collect::<Vec<_>>().join(",");
            wrong.push(format!(
                "{kind} on {} (active {}): Excel {}, here {}",
                row["selection"],
                row["active"],
                theirs.as_deref().map_or(format!("error {}", row["error"]), show),
                ours.as_ref().map_or_else(|e| format!("error {e}"), |c| show(c)),
            ));
        }
    }
    println!("{} of {} options differ", wrong.len(), rows.len());
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn drawing_objects_are_counted_as_excel_counts_shapes() {
    let session = fixture_session("special.xlsx");
    let expected = fixture_json("special-expected.json");
    let theirs: Vec<String> = expected["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_str().unwrap().to_owned())
        .collect();
    let ours: Vec<String> = session
        .drawing_objects(0)
        .iter()
        .map(|o| format!("{}|{}", o.from.a1(), o.to.a1()))
        .collect();
    assert_eq!(ours.len(), theirs.len(), "{ours:?} against {theirs:?}");
    for (o, t) in ours.iter().zip(&theirs) {
        // Excel names the cell a shape's corner is in, and the file names the cell the anchor starts the
        // corner at, which can be the cell beside it when the corner is on a boundary.
        assert_eq!(o.split('|').next(), t.split('|').next(), "{o} against {t}");
    }
}

#[test]
fn the_last_cell_and_the_used_range_agree() {
    let session = fixture_session("special.xlsx");
    let expected = fixture_json("special-expected.json");
    let used = Range::parse(expected["usedRange"].as_str().unwrap()).unwrap();
    let last = session.special_cells(0, &[], at("A1"), Special::LastCell).unwrap();
    assert_eq!(last, vec![used.last]);
}

#[test]
fn cells_group_into_the_rectangles_excel_calls_areas() {
    let cells: Vec<CellRef> =
        ["A1", "B1", "A2", "B2", "D1", "D2", "A4"].iter().map(|a| at(a)).collect();
    let areas: Vec<String> = areas_of(&cells).iter().map(Range::a1).collect();
    assert_eq!(areas, ["A1:B2", "D1:D2", "A4"]);
}

#[test]
fn nothing_found_says_what_excel_says() {
    let session = fixture_session("comments-start.xlsx");
    let err = session.special_cells(0, &[], at("A1"), Special::Notes).unwrap_err();
    assert_eq!(err, "No cells were found.");
}
