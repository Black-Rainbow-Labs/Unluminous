//! Grouping, folding, Auto Outline and Subtotal, compared with what Excel did.
//!
//! `tools/excel-oracle/outline.ps1` builds each scenario's workbook in Excel, saves it as `<name>-in.xlsx`,
//! performs the operation through Excel's object model and saves `<name>.xlsx`. Each test here opens the
//! first file, performs the same operation with the engine, and compares the outline of every row and
//! column, the outline settings and every cell with the second.

use std::collections::BTreeMap;
use std::path::PathBuf;

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{Session, SubtotalFunction, SubtotalSpec};
use unluminous_sheet::outline::{self, Dim};
use unluminous_sheet::{FileKind, Value, Workbook};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/outline").join(name)
}

fn open(file: &str) -> Workbook {
    let bytes = std::fs::read(fixture(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    unluminous_sheet::open(&bytes, FileKind::Xlsx, file, 2026).unwrap()
}

/// What the outline says about every line that says anything: `(level, hidden, collapsed)`.
fn lines(book: &Workbook, dim: Dim) -> BTreeMap<u32, (u8, bool, bool)> {
    let s = &book.sheets[0];
    let indexes: Vec<u32> = match dim {
        Dim::Rows => s.rows.keys().copied().collect(),
        Dim::Columns => s.columns.iter().flat_map(|c| c.first..=c.last).collect(),
    };
    indexes
        .into_iter()
        .map(|i| {
            let l = outline::line(s, dim, i);
            (i, (l.level, l.hidden, l.collapsed))
        })
        .filter(|(_, v)| *v != (0, false, false))
        .collect()
}

/// Every cell that holds a value or a formula: its formula text and its value.
fn cells(book: &Workbook) -> BTreeMap<(u32, u32), (String, String)> {
    book.sheets[0]
        .cells
        .iter()
        .filter(|(_, c)| !c.value.is_empty() || c.formula.is_some())
        .map(|(k, c)| {
            let formula = c.formula.as_ref().map(|f| f.display()).unwrap_or_default();
            let value = match &c.value {
                Value::Number(n) => format!("{:.9}", n),
                other => other.to_text(),
            };
            (*k, (formula, value))
        })
        .collect()
}

/// Compare the engine's workbook with Excel's, and say where they differ.
fn assert_same(name: &str, mine: &Workbook, excel: &Workbook) {
    for dim in [Dim::Rows, Dim::Columns] {
        assert_eq!(lines(mine, dim), lines(excel, dim), "{name}: outline of the {dim:?}");
    }
    assert_eq!(mine.sheets[0].outline, excel.sheets[0].outline, "{name}: outline settings");
    assert_eq!(cells(mine), cells(excel), "{name}: cells");
}

/// Run a scenario: open Excel's input, perform the operation, and compare with Excel's output.
fn check(name: &str, operation: impl FnOnce(&mut Session) -> Result<(), String>) {
    let mut session = Session::new(open(&format!("{name}-in.xlsx")), 46318.5);
    operation(&mut session).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_same(name, &session.book, &open(&format!("{name}.xlsx")));
}

fn group_rows(s: &mut Session, first: u32, last: u32) -> Result<(), String> {
    s.group_rows(0, first, last)
}

fn group_columns(s: &mut Session, first: u32, last: u32) -> Result<(), String> {
    s.group_columns(0, first, last)
}

#[test]
fn grouping_rows_matches_excel() {
    check("group-rows", |s| group_rows(s, 2, 4));
    check("group-rows-nested", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        group_rows(s, 3, 3)
    });
    check("group-rows-adjacent", |s| {
        group_rows(s, 1, 3)?;
        group_rows(s, 4, 6)
    });
    check("group-with-hidden-rows", |s| {
        s.hide_rows(0, 3, 3, true)?;
        group_rows(s, 2, 4)
    });
    check("group-last-row", |s| group_rows(s, 1_048_574, 1_048_575));
    check("group-first-row-summary-above", |s| {
        s.set_outline_summary(0, Some(false), None)?;
        group_rows(s, 0, 2)
    });
}

#[test]
fn grouping_columns_matches_excel() {
    check("group-columns", |s| group_columns(s, 1, 2));
    check("group-columns-nested", |s| {
        group_columns(s, 1, 4)?;
        group_columns(s, 2, 3)
    });
    check("group-single-cells-range", |s| group_columns(s, 1, 2));
    check("group-columns-seven", |s| {
        group_columns(s, 1, 5)?;
        group_columns(s, 2, 5)?;
        group_columns(s, 3, 5)
    });
}

#[test]
fn seven_levels_are_the_most_and_the_eighth_is_refused() {
    check("group-seven-levels", |s| {
        for i in 1..=7 {
            group_rows(s, i, 11)?;
        }
        Ok(())
    });
    let mut session = Session::new(open("group-eighth-level-refused-in.xlsx"), 46318.5);
    for _ in 0..7 {
        group_rows(&mut session, 1, 11).unwrap();
    }
    let before = lines(&session.book, Dim::Rows);
    assert!(group_rows(&mut session, 1, 11).is_err());
    assert_eq!(lines(&session.book, Dim::Rows), before, "a refused group changes nothing");
    assert_same(
        "group-eighth-level-refused",
        &session.book,
        &open("group-eighth-level-refused.xlsx"),
    );
}

#[test]
fn ungrouping_matches_excel() {
    check("ungroup-rows", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.ungroup_rows(0, 2, 3)
    });
    check("ungroup-all-rows", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.ungroup_rows(0, 1, 7)?;
        s.ungroup_rows(0, 1, 7)
    });
    check("ungroup-columns", |s| {
        group_columns(s, 1, 4)?;
        group_columns(s, 2, 3)?;
        s.ungroup_columns(0, 1, 2)
    });
    check("ungroup-partial-level0", |s| {
        group_rows(s, 2, 4)?;
        s.ungroup_rows(0, 0, 5)
    });
    // Excel leaves rows hidden and the summary row marked when the group is taken apart.
    check("ungroup-collapsed", |s| {
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)?;
        s.ungroup_rows(0, 2, 4)
    });
}

#[test]
fn ungrouping_what_is_not_grouped_is_refused() {
    let mut session = Session::new(open("ungroup-nothing-in.xlsx"), 46318.5);
    assert!(session.ungroup_rows(0, 1, 2).is_err());
    assert!(!session.can_undo(), "a refusal is not an undo step");
    assert_same("ungroup-nothing", &session.book, &open("ungroup-nothing.xlsx"));
}

#[test]
fn collapsing_and_expanding_match_excel() {
    check("collapse-rows", |s| {
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)
    });
    check("collapse-then-expand-rows", |s| {
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)?;
        s.show_detail(0, Dim::Rows, 5, true)
    });
    check("collapse-nested-rows", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)
    });
    check("collapse-outer-nested-rows", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)?;
        s.show_detail(0, Dim::Rows, 8, false)
    });
    check("expand-outer-keeps-inner-collapsed", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)?;
        s.show_detail(0, Dim::Rows, 8, false)?;
        s.show_detail(0, Dim::Rows, 8, true)
    });
    check("collapse-columns", |s| {
        group_columns(s, 1, 2)?;
        s.show_detail(0, Dim::Columns, 3, false)
    });
    check("collapse-columns-then-expand", |s| {
        group_columns(s, 1, 2)?;
        s.show_detail(0, Dim::Columns, 3, false)?;
        s.show_detail(0, Dim::Columns, 3, true)
    });
    check("collapse-group-at-edge-columns", |s| {
        group_columns(s, 0, 1)?;
        s.show_detail(0, Dim::Columns, 2, false)
    });
    // Two groups of one level next to each other are one group, so the summary row of the second folds both.
    check("collapse-adjacent-groups", |s| {
        group_rows(s, 1, 3)?;
        group_rows(s, 4, 6)?;
        s.show_detail(0, Dim::Rows, 7, false)
    });
}

#[test]
fn collapsing_a_line_that_is_not_a_summary_is_refused() {
    let mut session = Session::new(open("collapse-non-summary-in.xlsx"), 46318.5);
    group_rows(&mut session, 2, 4).unwrap();
    assert!(session.show_detail(0, Dim::Rows, 7, false).is_err());
    assert_same("collapse-non-summary", &session.book, &open("collapse-non-summary.xlsx"));
}

#[test]
fn the_level_buttons_match_excel() {
    let grouped = |s: &mut Session| -> Result<(), String> {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        group_columns(s, 1, 4)
    };
    check("show-levels-1", |s| {
        grouped(s)?;
        s.show_levels(0, 1, 1)
    });
    check("show-levels-2", |s| {
        grouped(s)?;
        group_columns(s, 2, 3)?;
        s.show_levels(0, 2, 2)
    });
    check("show-levels-rows-only", |s| {
        grouped(s)?;
        s.show_levels(0, 1, 0)
    });
    check("show-levels-all", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.show_levels(0, 1, 0)?;
        s.show_levels(0, 3, 0)
    });
    check("show-levels-beyond", |s| {
        group_rows(s, 1, 7)?;
        group_rows(s, 2, 4)?;
        s.show_levels(0, 6, 0)
    });
    check("show-levels-manual-hidden", |s| {
        s.hide_rows(0, 10, 10, true)?;
        group_rows(s, 1, 3)?;
        s.show_levels(0, 1, 0)?;
        s.show_levels(0, 2, 0)
    });
    check("show-levels-manual-hidden-inside", |s| {
        group_rows(s, 1, 3)?;
        s.hide_rows(0, 2, 2, true)?;
        s.show_levels(0, 2, 0)
    });
}

#[test]
fn summary_settings_match_excel() {
    check("summary-above", |s| {
        s.set_outline_summary(0, Some(false), None)?;
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 1, false)
    });
    check("summary-above-collapse-nested", |s| {
        s.set_outline_summary(0, Some(false), None)?;
        group_rows(s, 2, 8)?;
        group_rows(s, 3, 5)?;
        s.show_detail(0, Dim::Rows, 2, false)
    });
    check("summary-left", |s| {
        s.set_outline_summary(0, None, Some(false))?;
        group_columns(s, 2, 3)?;
        s.show_detail(0, Dim::Columns, 1, false)
    });
    check("summary-both-default", |s| {
        s.set_outline_summary(0, Some(true), Some(true))?;
        group_rows(s, 2, 4)
    });
    // Excel marks the new summary row of a folded group and leaves the old mark.
    check("summary-flip-after-collapse", |s| {
        group_rows(s, 2, 4)?;
        s.show_detail(0, Dim::Rows, 5, false)?;
        s.set_outline_summary(0, Some(false), None)
    });
}

#[test]
fn clearing_the_outline_matches_excel() {
    check("clear-outline", |s| {
        group_rows(s, 1, 7)?;
        group_columns(s, 1, 2)?;
        s.show_detail(0, Dim::Rows, 8, false)?;
        s.clear_outline(0)
    });
    check("clear-outline-range", |s| {
        group_rows(s, 1, 3)?;
        group_rows(s, 5, 7)?;
        s.clear_outline_lines(0, Dim::Rows, 1, 3)
    });
    check("clear-outline-keeps-manual-hidden", |s| {
        s.hide_rows(0, 10, 10, true)?;
        group_rows(s, 1, 3)?;
        s.show_detail(0, Dim::Rows, 4, false)?;
        s.clear_outline(0)
    });
}

#[test]
fn auto_outline_matches_excel() {
    for name in [
        "auto-outline",
        "auto-outline-rows-only-range",
        "auto-above",
        "auto-single-ref",
        "auto-gap",
        "auto-partial-columns",
        "auto-subtotal-function",
        "auto-two-summary-rows",
        "auto-cell-references",
        "auto-expression-around-sum",
        "auto-columns-left",
        "auto-columns-gap",
        "auto-both-directions",
        "auto-over-existing-outline",
        "auto-three-levels",
        "auto-summary-above-setting",
        "auto-below-with-above-setting",
        "auto-no-formulas",
    ] {
        // Excel's formulas, and a manual group or the summary setting in some scenarios, are in the input.
        let range =
            (name == "auto-outline-rows-only-range").then(|| Range::parse("A1:D10").unwrap());
        check(name, |s| s.auto_outline(0, range));
    }
    check("auto-outline-then-clear", |s| {
        s.auto_outline(0, None)?;
        s.clear_outline(0)
    });
}

/// A subtotal request over the list in A1:D13 the scenarios share.
fn spec(function: SubtotalFunction, group_by: u32, totals: &[u32]) -> SubtotalSpec {
    SubtotalSpec {
        range: Range::new(CellRef::new(0, 0), CellRef::new(12, 3)),
        group_by,
        function,
        total_columns: totals.to_vec(),
        replace: true,
        page_breaks: false,
        summary_below: true,
    }
}

#[test]
fn subtotals_match_excel() {
    use SubtotalFunction::*;
    let cases: Vec<(&str, SubtotalSpec)> = vec![
        ("subtotal-sum", spec(Sum, 0, &[2, 3])),
        ("subtotal-count", spec(Count, 0, &[3])),
        ("subtotal-average", spec(Average, 0, &[2, 3])),
        ("subtotal-max", spec(Max, 0, &[3])),
        ("subtotal-min", spec(Min, 0, &[3])),
        ("subtotal-product", spec(Product, 0, &[2])),
        ("subtotal-count-numbers", spec(CountNumbers, 0, &[3])),
        ("subtotal-stdev", spec(StdDev, 0, &[3])),
        ("subtotal-stdevp", spec(StdDevP, 0, &[3])),
        ("subtotal-var", spec(Var, 0, &[3])),
        ("subtotal-varp", spec(VarP, 0, &[3])),
        ("subtotal-by-product", spec(Sum, 1, &[3])),
        ("subtotal-text-column", spec(Count, 0, &[1])),
        ("subtotal-middle-column-key", spec(Sum, 1, &[2])),
        ("subtotal-key-column-total", spec(Sum, 0, &[0, 3])),
        ("subtotal-after-manual-group", spec(Sum, 0, &[3])),
        ("subtotal-hidden-rows-preexisting", spec(Sum, 0, &[3])),
        ("subtotal-numeric-key", spec(Sum, 0, &[3])),
        ("subtotal-case-key", spec(Sum, 0, &[3])),
        ("subtotal-blank-key", spec(Sum, 0, &[3])),
    ];
    // The input of some of these holds a manual group, a hidden row, numbers, mixed case or blanks in the
    // group column, as the scenario in `outline.ps1` made them.
    for (name, request) in cases {
        check(name, |s| s.subtotal(0, &request));
    }
}

#[test]
fn subtotals_with_the_summary_above_match_excel() {
    check("subtotal-summary-above", |s| {
        let mut request = spec(SubtotalFunction::Sum, 0, &[3]);
        request.summary_below = false;
        s.subtotal(0, &request)
    });
}

#[test]
fn a_second_subtotal_replaces_or_nests_as_excel_does() {
    use SubtotalFunction::*;
    let whole = |function, group_by, totals: &[u32], replace| {
        let mut request = spec(function, group_by, totals);
        request.range = Range::new(CellRef::new(0, 0), CellRef::new(19, 3));
        request.replace = replace;
        request
    };
    check("subtotal-replace", |s| {
        s.subtotal(0, &spec(Sum, 0, &[3]))?;
        s.subtotal(0, &whole(Count, 0, &[2], true))
    });
    check("subtotal-no-replace", |s| {
        s.subtotal(0, &spec(Sum, 0, &[3]))?;
        s.subtotal(0, &whole(Count, 0, &[2], false))
    });
    check("subtotal-nested", |s| {
        s.subtotal(0, &spec(Sum, 0, &[3]))?;
        s.subtotal(0, &whole(Count, 1, &[2], false))
    });
    check("subtotal-remove", |s| {
        s.subtotal(0, &spec(Sum, 0, &[3]))?;
        s.remove_subtotal(0, Range::new(CellRef::new(0, 0), CellRef::new(19, 3)))
    });
    check("subtotal-collapsed-then-subtotal", |s| {
        s.subtotal(0, &spec(Sum, 0, &[3]))?;
        s.show_levels(0, 2, 0)
    });
}

#[test]
fn a_list_of_one_row_is_refused() {
    let mut session = Session::new(open("subtotal-one-row-range-in.xlsx"), 46318.5);
    let mut request = spec(SubtotalFunction::Sum, 0, &[3]);
    request.range = Range::new(CellRef::new(0, 0), CellRef::new(0, 3));
    assert!(session.subtotal(0, &request).is_err());
    assert!(!session.can_undo());
}

#[test]
fn subtotal_is_one_undo_step() {
    let mut session = Session::new(open("subtotal-sum-in.xlsx"), 46318.5);
    let before = cells(&session.book);
    session.subtotal(0, &spec(SubtotalFunction::Sum, 0, &[3])).unwrap();
    assert_ne!(cells(&session.book), before);
    session.undo().unwrap();
    assert_eq!(cells(&session.book), before);
    assert!(lines(&session.book, Dim::Rows).is_empty());
}

#[test]
fn page_breaks_go_after_each_group() {
    let mut session = Session::new(open("subtotal-sum-in.xlsx"), 46318.5);
    let mut request = spec(SubtotalFunction::Sum, 0, &[3]);
    request.page_breaks = true;
    session.subtotal(0, &request).unwrap();
    let breaks = session.book.sheets[0]
        .raw
        .elements
        .iter()
        .find(|(n, _)| n == "rowBreaks")
        .map(|(_, x)| x.clone())
        .unwrap();
    // The subtotal rows are rows 5, 8, 13 and 17, and a break comes after each.
    for id in [5, 8, 13, 17] {
        assert!(breaks.contains(&format!("<brk id=\"{id}\"")), "{breaks}");
    }
    assert!(breaks.contains("manualBreakCount=\"4\""));
}
