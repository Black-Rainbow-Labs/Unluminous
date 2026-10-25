//! The Evaluate Formula stepper and the traces. Excel gives no way to read its dialog, so these check the
//! steps against what the dialog is known to do: operands left to right, literals are not steps, a range
//! becomes an array, `IF` works out only the branch it takes, and a reference to a formula can be stepped into.

mod review_support;

use review_support::*;
use unluminous_sheet::edit::Session;
use unluminous_sheet::{Value, Workbook};

fn session(cells: &[(&str, &str)]) -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    for (a, text) in cells {
        s.set_input(0, at(a), text).unwrap();
    }
    s
}

/// Every text the stepper shows, the underlined part marked with brackets, until it has a value.
fn steps(s: &Session, cell: &str) -> (Vec<String>, Value) {
    let mut e = s.evaluator(0, at(cell)).unwrap();
    let mut shown = Vec::new();
    loop {
        let v = e.view(&s.book);
        let text = match v.underlined {
            Some((a, b)) => {
                let chars: Vec<char> = v.text.chars().collect();
                format!(
                    "{}[{}]{}",
                    chars[..a].iter().collect::<String>(),
                    chars[a..b].iter().collect::<String>(),
                    chars[b..].iter().collect::<String>()
                )
            }
            None => v.text.clone(),
        };
        shown.push(text);
        if let Some(result) = v.result {
            return (shown, result);
        }
        e.evaluate(&s.book).unwrap();
    }
}

#[test]
fn operands_are_worked_out_left_to_right_and_literals_are_not_steps() {
    let s = session(&[("A1", "5"), ("B1", "2"), ("C1", "=A1+B1*3")]);
    let (shown, result) = steps(&s, "C1");
    assert_eq!(shown, ["=[A1]+B1*3", "=5+[B1]*3", "=5+[2*3]", "=[5+6]", "=11"]);
    assert_eq!(result, Value::Number(11.0));
}

#[test]
fn a_range_becomes_an_array_and_if_takes_only_its_branch() {
    let s = session(&[
        ("A1", "5"),
        ("B1", "1"),
        ("B2", "2"),
        ("B3", "3"),
        ("C1", "=IF(A1>0,SUM(B1:B3),B1/0)*2"),
    ]);
    let (shown, result) = steps(&s, "C1");
    assert_eq!(
        shown,
        [
            "=IF([A1]>0,SUM(B1:B3),B1/0)*2",
            "=IF([5>0],SUM(B1:B3),B1/0)*2",
            "=IF(TRUE,SUM([B1:B3]),B1/0)*2",
            "=IF(TRUE,[SUM({1;2;3})],B1/0)*2",
            "=[IF(TRUE,6,B1/0)]*2",
            "=[6*2]",
            "=12",
        ]
    );
    assert_eq!(result, Value::Number(12.0));
}

#[test]
fn text_and_errors_are_shown_as_excel_shows_them() {
    let s = session(&[("A1", "abc"), ("B1", "=UPPER(A1)&\"!\""), ("C1", "=1/0+1")]);
    let (shown, result) = steps(&s, "B1");
    assert_eq!(
        shown,
        ["=UPPER([A1])&\"!\"", "=[UPPER(\"abc\")]&\"!\"", "=[\"ABC\"&\"!\"]", "=\"ABC!\""]
    );
    assert_eq!(result, Value::text("ABC!"));
    let (shown, result) = steps(&s, "C1");
    assert_eq!(shown, ["=[1/0]+1", "=[#DIV/0!+1]", "=#DIV/0!"]);
    assert!(matches!(result, Value::Error(_)));
}

#[test]
fn step_in_shows_the_formula_of_a_referenced_cell_and_step_out_finishes_it() {
    let s = session(&[("A1", "2"), ("B1", "=A1*10"), ("C1", "=B1+1")]);
    let mut e = s.evaluator(0, at("C1")).unwrap();
    let v = e.view(&s.book);
    assert!(v.can_step_in && !v.can_step_out);
    assert_eq!(v.reference, "Sheet1!$C$1");
    e.step_in(&s.book).unwrap();
    let inner = e.view(&s.book);
    assert_eq!(inner.reference, "Sheet1!$B$1");
    assert_eq!(inner.text, "=A1*10");
    assert!(inner.can_step_out);
    e.step_out(&s.book).unwrap();
    let back = e.view(&s.book);
    assert_eq!(back.text, "=20+1");
    assert_eq!(e.finish(&s.book), Value::Number(21.0));
    e.restart();
    assert_eq!(e.view(&s.book).text, "=B1+1");
    assert!(s.evaluator(0, at("A1")).is_err(), "a constant has no formula");
}

#[test]
fn trace_precedents_dependents_and_errors() {
    let s = session(&[
        ("A1", "1"),
        ("A2", "2"),
        ("B1", "=SUM(A1:A2)"),
        ("C1", "=B1*2"),
        ("D1", "=C1+A1"),
        ("E1", "=1/0"),
        ("F1", "=E1+1"),
        ("G1", "=F1*2+B1"),
    ]);
    let cells = |reaches: Vec<unluminous_sheet::edit::review::trace::Reach>| -> Vec<String> {
        reaches.iter().map(|r| r.range.a1()).collect()
    };
    assert_eq!(cells(s.direct_precedents(0, at("D1"))), ["C1", "A1"]);
    assert_eq!(
        cells(s.precedents(0, unluminous_sheet::Range::cell(at("D1")), true)),
        ["C1", "A1", "B1", "A1:A2"]
    );
    assert_eq!(
        cells(s.dependents(0, unluminous_sheet::Range::cell(at("B1")), false)),
        ["C1", "G1"]
    );
    assert_eq!(
        cells(s.dependents(0, unluminous_sheet::Range::cell(at("A1")), true)),
        ["B1", "D1", "C1", "G1"]
    );
    let trace = s.trace_error(0, at("G1")).unwrap();
    assert_eq!(trace.sources, vec![(0, at("E1"))]);
    assert_eq!(trace.arrows, vec![((0, at("E1")), (0, at("F1"))), ((0, at("F1")), (0, at("G1")))]);
    assert!(s.trace_error(0, at("B1")).is_err());
}
