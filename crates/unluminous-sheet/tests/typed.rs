//! What typing into a cell does, against what Excel did with the same text.
//!
//! `oracle/inputs.json` is the list of texts and `oracle/input-excel.jsonl` is Excel's answer for each,
//! written by `tools/excel-oracle/typed.ps1`: it clears A1, sets `Range("A1").Formula` to the text, and
//! records the value, the kind of value and the number format. They are committed, so this test runs with
//! no Excel. When the list changes, run the script again:
//!
//! ```text
//! pwsh tools/excel-oracle/typed.ps1 -Inputs crates/unluminous-sheet/tests/oracle/inputs.json \
//!     > crates/unluminous-sheet/tests/oracle/input-excel.jsonl
//! ```
//!
//! A text that starts with `=` is a formula and goes through a session, because Excel refuses some of
//! them (`=1+`) and keeps the rest as formulas.

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::edit::Session;
use unluminous_sheet::input::read;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::value::Value;

/// One line of Excel's answers.
struct Answer {
    input: String,
    kind: String,
    value: String,
    format: String,
    has_formula: bool,
}

fn answers() -> Vec<Answer> {
    let inputs: Vec<String> = serde_json::from_str(include_str!("oracle/inputs.json")).unwrap();
    let lines: Vec<serde_json::Value> = include_str!("oracle/input-excel.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(inputs.len(), lines.len(), "every input has an answer from Excel");
    inputs
        .into_iter()
        .zip(lines)
        .map(|(input, line)| {
            assert_eq!(
                line["input"].as_str().unwrap(),
                input,
                "the answers are in the order of the inputs"
            );
            Answer {
                input,
                kind: line["kind"].as_str().unwrap().to_owned(),
                value: line["value"].as_str().unwrap_or("").to_owned(),
                format: line["numberformat"].as_str().unwrap_or("").to_owned(),
                has_formula: line["hasformula"].as_bool().unwrap_or(false),
            }
        })
        .collect()
}

/// Whether a value is what Excel recorded, and if not, how it differs.
fn value_difference(ours: &Value, answer: &Answer) -> Option<String> {
    let same = match (answer.kind.as_str(), ours) {
        ("number", Value::Number(n)) => {
            let e: f64 = answer.value.parse().unwrap();
            (e - n).abs() <= e.abs().max(n.abs()) * 1e-12 || (e - n).abs() < 1e-300
        }
        ("text", Value::Text(t)) => **t == *answer.value,
        ("bool", Value::Bool(b)) => answer.value.eq_ignore_ascii_case(&b.to_string()),
        ("error", Value::Error(e)) => e.text() == answer.value,
        ("empty", Value::Empty) => true,
        _ => false,
    };
    (!same).then(|| format!("excel {} {:?}, ours {:?}", answer.kind, answer.value, ours))
}

/// A formula typed into a session: refused, or the value it makes.
fn typed_formula(input: &str) -> Result<Value, String> {
    let mut session = Session::new(Workbook::new(), 46318.5);
    let at = CellRef::parse("A1").unwrap();
    session.set_input(0, at, input)?;
    Ok(session.book.sheets[0].cell(at).map(|c| c.value.clone()).unwrap_or(Value::Empty))
}

#[test]
fn every_typed_text_is_read_as_excel_read_it() {
    let mut wrong = Vec::new();
    let all = answers();
    for answer in &all {
        if answer.input.starts_with('=') && answer.input.len() > 1 {
            let found = typed_formula(&answer.input);
            let problem = match (answer.kind.as_str(), found) {
                ("refused", Err(_)) => None,
                ("refused", Ok(v)) => Some(format!("excel refused it, ours made {v:?}")),
                (_, Err(why)) => Some(format!("excel accepted it, ours refused: {why}")),
                (_, Ok(v)) => {
                    assert!(answer.has_formula);
                    value_difference(&v, answer)
                }
            };
            wrong.extend(problem.map(|p| format!("{:?}: {p}", answer.input)));
            continue;
        }
        let typed = read(&answer.input, false, 2026);
        if let Some(p) = value_difference(&typed.value, answer) {
            wrong.push(format!("{:?}: value {p}", answer.input));
        }
        let ours = typed.format.unwrap_or("General");
        if matches!(typed.value, Value::Number(_)) && ours != answer.format {
            wrong.push(format!(
                "{:?}: format excel {:?}, ours {:?}",
                answer.input, answer.format, ours
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} typed texts differ from Excel:\n{}",
        wrong.len(),
        all.len(),
        wrong.join("\n")
    );
}
