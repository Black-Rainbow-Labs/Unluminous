//! Find and Replace against Excel's `Range.Find`, `FindNext` and `Replace`.
//!
//! `fixtures/review/search.xlsx` and `search-expected.json` were made by `tools/excel-oracle/make-review-search.ps1`:
//! every combination of search text, Look in, Look at, match case and order, with the cells Excel visits.

mod review_support;

use review_support::*;
use unluminous_sheet::coord::CellRef;
use unluminous_sheet::edit::review::search::{formula_text, FindOptions, LookIn, Scope};
use unluminous_sheet::edit::Session;

fn address_of(session: &Session, text: &str) -> (usize, CellRef) {
    let (sheet, cell) = text.split_once('!').unwrap();
    let first = cell.split(':').next().unwrap();
    (session.book.sheet_index(sheet).unwrap(), at(first))
}

/// Excel's Find starts after the active cell, A1, so a match in A1 is the last of its sheet.
fn excel_order(hits: Vec<(usize, CellRef)>) -> Vec<(usize, CellRef)> {
    let mut out: Vec<(usize, CellRef)> = Vec::new();
    for sheet in 0..2 {
        let group: Vec<_> = hits.iter().filter(|(s, _)| *s == sheet).copied().collect();
        out.extend(group.iter().filter(|(_, a)| *a != at("A1")));
        out.extend(group.iter().filter(|(_, a)| *a == at("A1")));
    }
    out
}

fn options_of(q: &serde_json::Value) -> FindOptions {
    let mut o = FindOptions::new(q["what"].as_str().unwrap(), Scope::Workbook);
    o.look_in = match q["lookIn"].as_str().unwrap() {
        "formulas" => LookIn::Formulas,
        "values" => LookIn::Values,
        "notes" => LookIn::Notes,
        _ => LookIn::Comments,
    };
    o.entire_cell = q["lookAt"] == "whole";
    o.match_case = q["matchCase"] == true;
    o.by_columns = q["order"] == "columns";
    o
}

#[test]
fn every_search_finds_the_cells_excel_finds_in_the_order_excel_visits_them() {
    let session = fixture_session("search.xlsx");
    let expected = fixture_json("search-expected.json");
    let mut wrong = Vec::new();
    let queries = expected["queries"].as_array().unwrap();
    for q in queries {
        let theirs: Vec<(usize, CellRef)> = q["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| address_of(&session, h.as_str().unwrap()))
            .collect();
        let mut ours: Vec<(usize, CellRef)> =
            session.find_cells(&options_of(q)).iter().map(|h| (h.sheet, h.at)).collect();
        // A merged area is found once, as its top left cell.
        ours.dedup();
        ours = excel_order(ours);
        if ours != theirs {
            wrong.push(format!(
                "{} / {} / {} / case {} / {}: Excel {}, here {}",
                q["what"],
                q["lookIn"],
                q["lookAt"],
                q["matchCase"],
                q["order"],
                theirs.iter().map(|(s, a)| format!("{s}!{a}")).collect::<Vec<_>>().join(" "),
                ours.iter().map(|(s, a)| format!("{s}!{a}")).collect::<Vec<_>>().join(" ")
            ));
        }
    }
    println!("{} of {} searches differ", wrong.len(), queries.len());
    assert!(
        wrong.is_empty(),
        "{}\n...",
        wrong.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn searching_by_format_keeps_the_cells_with_the_format_of_a_cell() {
    // Excel's FindFormat could not be set reliably from a script, so this is checked against the
    // workbook's own formatting: B7 is the one bold cell with General, E7 is bold with 0.00.
    let session = fixture_session("search.xlsx");
    let style = |a: &str| session.book.sheets[0].cell(at(a)).map_or(0, |c| c.style);
    let found = |o: &FindOptions| session.find_cells(o).iter().map(|h| h.at).collect::<Vec<_>>();
    let mut o = FindOptions::new("", Scope::Sheet(0));
    o.format = Some(style("B7"));
    assert_eq!(found(&o), vec![at("B7")]);
    o.format = Some(style("E7"));
    assert_eq!(found(&o), vec![at("E7")]);
    o.format = Some(style("A2"));
    o.what = "1234".to_owned();
    assert_eq!(found(&o), vec![at("A2")]);
    o.what = "apple".to_owned();
    assert!(found(&o).is_empty());
}

#[test]
fn replace_changes_the_cells_excel_changes_to_what_excel_makes_of_them() {
    let expected = fixture_json("search-expected.json");
    let mut wrong = Vec::new();
    for r in expected["replacements"].as_array().unwrap() {
        let mut session = fixture_session("search.xlsx");
        let before: Vec<(usize, CellRef, String)> = session
            .book
            .sheets
            .iter()
            .enumerate()
            .flat_map(|(si, s)| {
                s.cells.iter().map(move |(&(row, col), c)| (si, CellRef::new(row, col), c.clone()))
            })
            .map(|(si, a, c)| (si, a, formula_text(&session.book, &c)))
            .collect();
        let mut o = FindOptions::new(r["what"].as_str().unwrap(), Scope::Workbook);
        o.entire_cell = r["lookAt"] == "whole";
        o.match_case = r["matchCase"] == true;
        session.replace_cells(&o, r["with"].as_str().unwrap()).unwrap();
        let mut ours = std::collections::BTreeMap::new();
        for (si, a, was) in before {
            let now = session.book.sheets[si]
                .cell(a)
                .map(|c| formula_text(&session.book, c))
                .unwrap_or_default();
            if now != was {
                ours.insert(format!("{}!{a}", session.book.sheets[si].name), now);
            }
        }
        let theirs: std::collections::BTreeMap<String, String> = r["changes"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.split(':').next().unwrap().to_owned(), v.as_str().unwrap().to_owned()))
            .collect();
        if ours != theirs {
            wrong.push(format!(
                "{:?} -> {:?} ({}, case {}): Excel {:?}, here {:?}",
                r["what"], r["with"], r["lookAt"], r["matchCase"], theirs, ours
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn find_next_wraps_round_and_goes_both_ways() {
    let session = fixture_session("search.xlsx");
    let o = FindOptions::new("apple", Scope::Sheet(0));
    let all = session.find_cells(&o);
    assert!(all.len() > 3);
    let first = session.find_next(&o, None, false).unwrap();
    assert_eq!(first, all[0]);
    assert_eq!(session.find_next(&o, Some(all[all.len() - 1]), false), Some(all[0]));
    assert_eq!(session.find_next(&o, Some(all[0]), true), Some(all[all.len() - 1]));
    assert_eq!(session.find_next(&o, Some(all[1]), true), Some(all[0]));
}
