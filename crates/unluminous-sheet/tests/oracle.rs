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

/// Whether a value is of the kind Excel answered, which is all that is compared for a volatile formula.
fn same_kind(kind: &str, ours: &Value) -> bool {
    matches!(
        (kind, ours),
        ("number", Value::Number(_))
            | ("text", Value::Text(_))
            | ("bool", Value::Bool(_))
            | ("error", Value::Error(_))
    )
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
        if kind == "refused" {
            continue;
        }
        let ours = match evaluate(&book, 0, at, formula, 46318.5) {
            Ok((v, _)) => v,
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
        let ok =
            if volatile { same_kind(kind, &ours) } else { agrees(kind, value, &ours, tolerance) };
        if !ok {
            wrong.push(Wrong {
                formula: formula.to_owned(),
                excel: format!("{kind} {value}"),
                ours: shown(&ours),
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

/// The function names called in a formula: every identifier directly followed by an opening bracket and
/// not part of a longer name, so `T(` is not found inside `TEXT(` and `ERF(` is not found inside `ERF.PRECISE(`.
fn called_names(formula: &str) -> std::collections::BTreeSet<String> {
    let chars: Vec<char> = formula.chars().collect();
    let part_of_name = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';
    let mut names = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < chars.len() {
        if !part_of_name(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && part_of_name(chars[i]) {
            i += 1;
        }
        if i < chars.len() && chars[i] == '(' && chars[start].is_ascii_alphabetic() {
            names.insert(chars[start..i].iter().collect::<String>().to_ascii_uppercase());
        }
    }
    names
}

/// Every function must be called in at least five corpus formulas, so each has an error case and the
/// argument kinds it takes covered by what Excel answered.
// Switched on once every lane has merged, because the lanes that add functions bring their own cases and the
// audit's probe corpus is imported by another lane. The coordinator removes the ignore.
#[test]
#[ignore]
fn every_function_has_five_oracle_cases() {
    let mut cases: std::collections::BTreeMap<&str, usize> =
        unluminous_sheet::functions::all().iter().map(|f| (f.name, 0)).collect();
    for line in corpus_lines() {
        let answer: serde_json::Value = serde_json::from_str(&line).unwrap();
        for name in called_names(answer["formula"].as_str().unwrap()) {
            if let Some(count) = cases.get_mut(name.as_str()) {
                *count += 1;
            }
        }
    }
    let short: Vec<String> =
        cases.iter().filter(|(_, n)| **n < 5).map(|(name, n)| format!("{name} ({n})")).collect();
    assert!(
        short.is_empty(),
        "{} functions have fewer than five oracle cases: {}",
        short.len(),
        short.join(", ")
    );
}
