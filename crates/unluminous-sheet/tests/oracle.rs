//! The Excel oracle: every formula in the corpus, computed by Unluminous, against the answer Excel
//! computed for it on the same setup sheet.
//!
//! The corpus is every pair `oracle/<name>.txt` and `oracle/<name>.jsonl`, except `setup.txt` and the typed
//! input files (`inputs*`). A `.jsonl` file was written by `tools/excel-oracle/evaluate.ps1`, which enters
//! each formula into Excel 16 through COM and records Excel's value at full precision. They are committed,
//! so this test runs with no Excel. When a corpus file changes, run that script again for it:
//!
//! ```text
//! pwsh tools/excel-oracle/evaluate.ps1 -Formulas crates/unluminous-sheet/tests/oracle/<name>.txt \
//!     -Setup crates/unluminous-sheet/tests/oracle/setup.txt > crates/unluminous-sheet/tests/oracle/<name>.jsonl
//! ```
//!
//! `ORACLE=<name>` limits a run to one file.

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::eval::evaluate;
use unluminous_sheet::model::{Cell, Workbook};
use unluminous_sheet::styles::StyleChange;
use unluminous_sheet::value::Value;

/// The sheet every formula is evaluated on.
fn setup() -> Workbook {
    let mut book = Workbook::new();
    for line in include_str!("oracle/setup.txt").lines() {
        let Some((at, text)) = line.split_once('=') else { continue };
        let typed = unluminous_sheet::input::read(text, false, 2026);
        let mut cell = Cell::value(typed.value);
        if let Some(code) = typed.format {
            cell.style = book.styles.derive(0, &StyleChange::NumberFormat(code.to_owned()));
        }
        book.sheets[0].set(CellRef::parse(at).unwrap(), cell);
    }
    book
}

/// Formulas whose value changes on every calculation, which are compared for kind only.
const VOLATILE: &[&str] = &["RAND", "NOW", "TODAY", "RANDBETWEEN", "RANDARRAY"];

/// Formulas left out of the comparison, each with the reason. Nothing else is excluded.
const EXCLUDED: &[(&str, &str)] = &[(
    "TAN(PI()/2)",
    "Excel reduces the argument differently from the platform libm, so the fourth significant digit differs.",
)];

/// The rows and columns of a spill as Excel records them (`RxC`), or nothing for a single cell.
fn spill_size(array: &Option<unluminous_sheet::value::Array>) -> String {
    match array {
        Some(a) if a.rows * a.cols > 1 => format!("{}x{}", a.rows, a.cols),
        _ => String::new(),
    }
}

#[derive(Debug)]
struct Wrong {
    formula: String,
    excel: String,
    ours: String,
}

/// Functions Excel answers by iterating to a tolerance of its own, so its last digits are where its
/// iteration stopped rather than the root. Measured: XIRR of -100 and 110 a year apart is
/// 0.09999999403953552 in Excel, where the root is exactly 0.1, and RATE(10,-100,1000) has the root 0 and
/// is -1.96E-10. They are compared to the tolerance of 1e-7 of Excel's own stopping rule, of the answer or
/// of 1 where the answer is smaller, so a root of zero may come back as any number that small.
const ITERATIVE: &[&str] = &["IRR", "XIRR", "RATE"];

fn agrees(kind: &str, excel: &str, ours: &Value, tolerance: f64) -> bool {
    match (kind, ours) {
        ("number", Value::Number(n)) => {
            let e: f64 = excel.parse().unwrap();
            let floor = if tolerance >= 1e-7 { 1.0 } else { 1e-300 };
            let scale = e.abs().max(n.abs()).max(floor);
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

/// Every answer line of every corpus file, in file name order.
fn corpus_lines() -> Vec<String> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let only = std::env::var("ORACLE").ok();
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with("input"))
        .filter(|p| only.as_deref().is_none_or(|o| p.file_stem().unwrap() == o))
        .collect();
    files.sort();
    files
        .iter()
        .flat_map(|p| {
            std::fs::read_to_string(p).unwrap().lines().map(str::to_owned).collect::<Vec<_>>()
        })
        .filter(|l| !l.trim().is_empty())
        .collect()
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
    for line in corpus_lines() {
        let answer: serde_json::Value = serde_json::from_str(&line).unwrap();
        let formula = answer["formula"].as_str().unwrap();
        let kind = answer["kind"].as_str().unwrap();
        let value = answer["value"].as_str().unwrap_or("");
        if kind == "refused" || EXCLUDED.iter().any(|(f, _)| *f == formula) {
            continue;
        }
        let (ours, array) = match evaluate(&book, 0, at, formula, 46318.5) {
            Ok(found) => found,
            Err(problem) => {
                wrong.push(Wrong {
                    formula: formula.to_owned(),
                    excel: format!("{kind} {value}"),
                    ours: format!("refused: {problem}"),
                });
                continue;
            }
        };
        checked += 1;
        let volatile = VOLATILE.iter().any(|f| formula.contains(f));
        let tolerance = if ITERATIVE.iter().any(|f| formula.starts_with(f)) { 1e-7 } else { 1e-12 };
        let ok = if volatile {
            matches!((kind, &ours), ("number", Value::Number(_)))
        } else {
            agrees(kind, value, &ours, tolerance)
        };
        let excel_spill = answer["spill"].as_str().unwrap_or("");
        let ours_spill = spill_size(&array);
        let ok = ok && excel_spill == ours_spill;
        if !ok {
            wrong.push(Wrong {
                formula: formula.to_owned(),
                excel: format!("{kind} {value} spill {excel_spill:?}"),
                ours: format!("{} spill {ours_spill:?}", shown(&ours)),
            });
        }
    }
    let report: Vec<String> = wrong
        .iter()
        .map(|w| format!("{}\n    excel: {}\n    ours:  {}", w.formula, w.excel, w.ours))
        .collect();
    assert!(
        wrong.is_empty(),
        "{} of {checked} formulas differ from Excel:\n{}",
        wrong.len(),
        report.join("\n")
    );
}
