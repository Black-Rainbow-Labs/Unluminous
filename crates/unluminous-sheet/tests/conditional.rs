//! Conditional formats: which cells each rule formats, against what Excel answered, and how the rules are
//! read, written back and managed.
//!
//! `tools/excel-oracle/cf-evaluate.ps1` makes a sheet for every case, with its rules created through Excel's
//! object model, and records for every cell the fill, font colour, bold and italic that
//! `Range.DisplayFormat` says it has. `fixtures/conditional/cf-evaluate.xlsx` is the workbook it saved and
//! `cf-evaluate.jsonl` the answers. Each test reads the rules back out of the file and asks the engine the
//! same question.

use std::path::{Path, PathBuf};

use unluminous_sheet::conditional::RuleSet;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{RuleMove, Session};
use unluminous_sheet::model::{CfRule, Cfvo, IconSetRule};
use unluminous_sheet::{FileKind, Workbook};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conditional").join(name)
}

fn open(name: &str) -> Workbook {
    let bytes = std::fs::read(fixture(name)).unwrap();
    unluminous_sheet::open(&bytes, FileKind::Xlsx, name, 2026).unwrap()
}

fn answers() -> Vec<serde_json::Value> {
    std::fs::read_to_string(fixture("cf-evaluate.jsonl"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l.trim_start_matches('\u{FEFF}')).unwrap())
        .collect()
}

/// Excel's BGR integer as red, green and blue.
fn rgb(bgr: i64) -> (u8, u8, u8) {
    ((bgr & 0xFF) as u8, ((bgr >> 8) & 0xFF) as u8, ((bgr >> 16) & 0xFF) as u8)
}

/// Whether two colours are the same to within a unit of rounding in each channel.
fn near(a: (u8, u8, u8), b: (u8, u8, u8)) -> bool {
    a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1 && a.2.abs_diff(b.2) <= 1
}

/// Compare the engine with Excel on every cell of every case. Answers the differences.
fn differences(only: Option<&str>) -> Vec<String> {
    let book = open("cf-evaluate.xlsx");
    let mut wrong = Vec::new();
    for case in answers() {
        let name = case["case"].as_str().unwrap();
        if only.is_some_and(|o| o != name) || case.get("error").is_some() {
            continue;
        }
        let sheet = book.sheet_index(name).unwrap_or_else(|| panic!("no sheet {name}"));
        let rules = RuleSet::prepare(&book, sheet);
        let now = case["today"].as_f64().unwrap() + 0.5;
        for cell in case["cells"].as_array().unwrap() {
            let row = cell["row"].as_u64().unwrap() as u32 - 1;
            let look = rules.apply(&book, now, CellRef::new(row, 0)).look(&book);
            let expected_fill = cell["fill"].as_i64().map(rgb);
            let expected_font = cell["font"].as_i64().filter(|f| *f != 0).map(rgb);
            let fill_ok = match (look.fill, expected_fill) {
                (Some(a), Some(b)) => near(a, b),
                (None, None) => true,
                _ => false,
            };
            let font_ok = look.font_color == expected_font;
            let bold_ok = look.bold.unwrap_or(false) == cell["bold"].as_bool().unwrap();
            let italic_ok = look.italic.unwrap_or(false) == cell["italic"].as_bool().unwrap();
            if !(fill_ok && font_ok && bold_ok && italic_ok) {
                wrong.push(format!(
                    "{name} row {}: Excel fill {:?} font {:?} bold {} italic {}; engine fill {:?} font {:?} bold {:?} italic {:?}",
                    row + 1,
                    expected_fill,
                    expected_font,
                    cell["bold"],
                    cell["italic"],
                    look.fill,
                    look.font_color,
                    look.bold,
                    look.italic
                ));
            }
        }
    }
    wrong
}

#[test]
fn the_engine_formats_every_cell_as_excel_does() {
    let wrong = differences(None);
    assert!(wrong.is_empty(), "{} cells differ:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
fn the_oracle_covers_every_kind_of_rule() {
    let book = open("cf-evaluate.xlsx");
    let mut kinds = std::collections::BTreeSet::new();
    for s in &book.sheets {
        for cf in &s.conditional {
            for rule in &cf.rules {
                kinds.insert(rule.kind.clone());
            }
        }
    }
    for kind in [
        "cellIs",
        "expression",
        "containsText",
        "notContainsText",
        "beginsWith",
        "endsWith",
        "containsBlanks",
        "notContainsBlanks",
        "containsErrors",
        "notContainsErrors",
        "top10",
        "aboveAverage",
        "duplicateValues",
        "uniqueValues",
        "colorScale",
        "timePeriod",
    ] {
        assert!(kinds.contains(kind), "no case with a {kind} rule: {kinds:?}");
    }
    let periods: std::collections::BTreeSet<String> = book
        .sheets
        .iter()
        .flat_map(|s| s.conditional.iter())
        .flat_map(|cf| cf.rules.iter())
        .filter_map(|r| r.time_period.clone())
        .collect();
    assert_eq!(periods.len(), 10, "{periods:?}");
}

fn session(book: &str) -> Session {
    Session::new(open(book), 46319.5)
}

fn rule_of(kind: &str, formula: &str, fill: u32) -> CfRule {
    CfRule {
        dxf: Some(fill),
        operator: Some("greaterThan".to_owned()),
        formulas: vec![formula.to_owned()],
        ..CfRule::new(kind)
    }
}

#[test]
fn rules_are_listed_in_priority_order_and_managed() {
    let mut s = Session::new(Workbook::new(), 46319.5);
    s.book.styles.dxfs.push(unluminous_sheet::styles::Dxf::default());
    for text in ["1", "2", "3"] {
        s.add_conditional(0, vec![Range::parse("A1:A5").unwrap()], rule_of("cellIs", text, 0))
            .unwrap();
    }
    let listed = s.conditional_rules(0, None);
    assert_eq!(
        listed.iter().map(|e| e.rule.formulas[0].as_str()).collect::<Vec<_>>(),
        ["1", "2", "3"]
    );
    assert_eq!(listed.iter().map(|e| e.priority).collect::<Vec<_>>(), [1, 2, 3]);
    // Move the last rule to the top, then one place down.
    s.move_conditional_rule(0, 3, RuleMove::First).unwrap();
    let order = |s: &Session| -> Vec<String> {
        s.conditional_rules(0, None).iter().map(|e| e.rule.formulas[0].clone()).collect()
    };
    assert_eq!(order(&s), ["3", "1", "2"]);
    s.move_conditional_rule(0, 1, RuleMove::Down).unwrap();
    assert_eq!(order(&s), ["1", "3", "2"]);
    s.move_conditional_rule(0, 3, RuleMove::Up).unwrap();
    assert_eq!(order(&s), ["1", "2", "3"]);
    s.move_conditional_rule(0, 1, RuleMove::Last).unwrap();
    assert_eq!(order(&s), ["2", "3", "1"]);
    // Stop if true, edit, applies to and delete.
    s.set_conditional_stop(0, 2, true).unwrap();
    assert!(s.conditional_rules(0, None)[1].rule.stop);
    let mut edited = rule_of("cellIs", "9", 0);
    edited.stop = false;
    s.edit_conditional_rule(0, 2, edited).unwrap();
    assert_eq!(order(&s), ["2", "9", "1"]);
    s.set_conditional_ranges(0, 2, vec![Range::parse("C1:C3").unwrap()]).unwrap();
    assert_eq!(s.conditional_rules(0, Some(Range::parse("C2").unwrap())).len(), 1);
    assert_eq!(s.conditional_rules(0, Some(Range::parse("A1").unwrap())).len(), 2);
    s.delete_conditional_rule(0, 1).unwrap();
    assert_eq!(s.conditional_rules(0, None).iter().map(|e| e.priority).collect::<Vec<_>>(), [1, 2]);
    assert!(s.delete_conditional_rule(0, 9).is_err());
    // Every change is one undo step.
    s.undo().unwrap();
    assert_eq!(s.conditional_rules(0, None).len(), 3);
}

#[test]
fn an_icon_set_rule_keeps_its_choices_through_a_file() {
    let mut s = Session::new(Workbook::new(), 46319.5);
    for (i, v) in ["1", "5", "9"].iter().enumerate() {
        s.set_input(0, CellRef::new(i as u32, 0), v).unwrap();
    }
    let mut set = IconSetRule::new("3Stars");
    set.reverse = true;
    set.show_value = false;
    set.thresholds[1] = (Cfvo { kind: "num".to_owned(), value: Some("4".to_owned()) }, false);
    set.custom = vec![None, Some(("3Arrows".to_owned(), 1)), Some(("3Flags".to_owned(), 2))];
    let rule = CfRule { icon_set: Some(set.clone()), ..CfRule::new("iconSet") };
    s.add_conditional(0, vec![Range::parse("A1:A3").unwrap()], rule).unwrap();
    let bytes = unluminous_sheet::xlsx::write(&s.book).unwrap();
    let back = unluminous_sheet::xlsx::read(&bytes).unwrap();
    let rules = &back.sheets[0].conditional[0].rules[0];
    assert_eq!(rules.icon_set.as_ref().unwrap(), &set);
    assert!(rules.x14.as_ref().is_some_and(|x| x.only), "custom icons live in the extension");
}
