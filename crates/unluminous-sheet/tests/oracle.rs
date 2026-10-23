//! The Excel oracle: every formula in `oracle/formulas.txt`, computed by Unluminous, against the answer
//! Excel computed for it on the same setup sheet.
//!
//! `excel.jsonl` was written by `tools/excel-oracle/evaluate.ps1`, which enters each formula into Excel 16
//! through COM and records Excel's value at full precision. It is committed, so this test runs with no
//! Excel. When the corpus changes, run that script again:
//!
//! ```text
//! pwsh tools/excel-oracle/evaluate.ps1 -Formulas crates/unluminous-sheet/tests/oracle/formulas.txt \
//!     -Setup crates/unluminous-sheet/tests/oracle/setup.txt > crates/unluminous-sheet/tests/oracle/excel.jsonl
//! ```

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::eval::evaluate;
use unluminous_sheet::model::{Cell, Workbook};
use unluminous_sheet::value::Value;

/// The sheet every formula is evaluated on.
fn setup() -> Workbook {
    let mut book = Workbook::new();
    for line in include_str!("oracle/setup.txt").lines() {
        let Some((at, text)) = line.split_once('=') else { continue };
        let typed = unluminous_sheet::input::read(text, false, 2026);
        book.sheets[0].set(CellRef::parse(at).unwrap(), Cell::value(typed.value));
    }
    book
}

/// Formulas whose value changes on every calculation, which are compared for kind only.
const VOLATILE: &[&str] = &["RAND", "NOW", "TODAY", "RANDBETWEEN", "RANDARRAY"];

#[derive(Debug)]
struct Wrong {
    formula: String,
    excel: String,
    ours: String,
}

/// Functions Excel answers by iterating to a tolerance of its own, so its last digits are where its
/// iteration stopped rather than the root. Measured: XIRR of -100 and 110 a year apart is
/// 0.09999999403953552 in Excel, where the root is exactly 0.1.
const ITERATIVE: &[&str] = &["IRR", "XIRR", "RATE"];

fn agrees(kind: &str, excel: &str, ours: &Value, tolerance: f64) -> bool {
    match (kind, ours) {
        ("number", Value::Number(n)) => {
            let e: f64 = excel.parse().unwrap();
            let scale = e.abs().max(n.abs()).max(1e-300);
            (e - n).abs() <= scale * tolerance || (e - n).abs() < 1e-14
        }
        ("text", Value::Text(t)) => &**t == excel,
        ("bool", Value::Bool(b)) => excel.eq_ignore_ascii_case(if *b { "true" } else { "false" }),
        ("error", Value::Error(e)) => e.text() == excel,
        ("empty", Value::Empty) => true,
        // An empty result in a cell reads as 0.
        ("number", Value::Empty) => excel == "0",
        _ => false,
    }
}

fn shown(v: &Value) -> String {
    match v {
        Value::Number(n) => format!("{n:?}"),
        Value::Text(t) => format!("text {t:?}"),
        other => other.to_text(),
    }
}

#[test]
fn every_formula_computes_to_what_excel_computed() {
    let book = setup();
    let at = CellRef::parse("Z100").unwrap();
    let mut wrong = Vec::new();
    let mut checked = 0;
    for line in include_str!("oracle/excel.jsonl").lines() {
        let answer: serde_json::Value = serde_json::from_str(line).unwrap();
        let formula = answer["formula"].as_str().unwrap();
        let kind = answer["kind"].as_str().unwrap();
        let value = answer["value"].as_str().unwrap_or("");
        if kind == "refused" {
            continue;
        }
        let ours = match evaluate(&book, 0, at, formula, 46318.5) {
            Ok((v, _)) => v,
            Err(problem) => {
                wrong.push(Wrong { formula: formula.to_owned(), excel: format!("{kind} {value}"), ours: format!("refused: {problem}") });
                continue;
            }
        };
        checked += 1;
        let volatile = VOLATILE.iter().any(|f| formula.contains(f));
        let tolerance = if ITERATIVE.iter().any(|f| formula.starts_with(f)) { 1e-7 } else { 1e-12 };
        let ok = if volatile { matches!((kind, &ours), ("number", Value::Number(_))) } else { agrees(kind, value, &ours, tolerance) };
        if !ok {
            wrong.push(Wrong { formula: formula.to_owned(), excel: format!("{kind} {value}"), ours: shown(&ours) });
        }
    }
    let report: Vec<String> = wrong.iter().map(|w| format!("{}\n    excel: {}\n    ours:  {}", w.formula, w.excel, w.ours)).collect();
    assert!(wrong.is_empty(), "{} of {checked} formulas differ from Excel:\n{}", wrong.len(), report.join("\n"));
}
