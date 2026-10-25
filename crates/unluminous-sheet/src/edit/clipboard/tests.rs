//! Checks of the clipboard and fill commands that need no answer from Excel: undo, pasting between
//! workbooks, the choices a caller makes, and the refusals.

use super::*;
use crate::edit::{Session, Shift};
use crate::model::Workbook;
use crate::styles::{FontEdit, StyleChange};
use crate::value::Value;

fn session() -> Session {
    Session::new(Workbook::new(), 46318.5)
}

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn range(a: &str) -> Range {
    Range::parse(a).unwrap()
}

fn show(s: &Session, a: &str) -> String {
    s.book.sheets[0].value_at(at(a)).to_text()
}

fn put(s: &mut Session, a: &str, text: &str) {
    s.set_input(0, at(a), text).unwrap();
}

fn bold(s: &mut Session, a: &str) {
    let edit = FontEdit { bold: Some(true), ..Default::default() };
    s.set_style(0, &[range(a)], &StyleChange::Font(edit)).unwrap();
}

fn is_bold(s: &Session, a: &str) -> bool {
    let style = s.book.sheets[0].cell(at(a)).map_or(0, |c| c.style);
    s.book.styles.font_of(style).bold
}

#[test]
fn every_kind_and_operation_has_a_code_and_a_name_that_read_back() {
    for kind in PasteKind::ALL_KINDS {
        assert_eq!(PasteKind::from_excel_code(kind.excel_code()), Some(kind));
        assert_eq!(PasteKind::parse(kind.name()), Some(kind));
    }
    for code in [-4142, 2, 3, 4, 5] {
        assert_eq!(PasteOperation::from_excel_code(code).unwrap().excel_code(), code);
    }
    assert_eq!(PasteOperation::parse("multiply"), Some(PasteOperation::Multiply));
    assert_eq!(FillKind::from_excel_code(9), Some(FillKind::LinearTrend));
    assert_eq!(FillKind::parse("without formatting"), Some(FillKind::Values));
}

#[test]
fn a_paste_special_is_one_undo_step_and_undo_puts_everything_back() {
    let mut s = session();
    put(&mut s, "A1", "5");
    bold(&mut s, "A1");
    s.set_note(0, at("A1"), Some("a note"), "me").unwrap();
    put(&mut s, "C1", "9");
    let clip = s.copy(0, range("A1"), false);
    s.paste_special(&clip, 0, range("C1"), &PasteSpecial::of(PasteKind::All)).unwrap();
    assert!(is_bold(&s, "C1") && s.book.sheets[0].notes.contains_key(&(0, 2)));
    assert_eq!(s.undo().as_deref(), Some("Paste"));
    assert_eq!(show(&s, "C1"), "9");
    assert!(!is_bold(&s, "C1") && !s.book.sheets[0].notes.contains_key(&(0, 2)));
}

#[test]
fn a_paste_into_another_workbook_makes_the_formats_there() {
    let mut from = session();
    put(&mut from, "A1", "5");
    bold(&mut from, "A1");
    let clip = from.copy(0, range("A1"), false);
    let mut to = session();
    to.set_style(0, &[range("B2")], &StyleChange::NumberFormat("0.000".to_owned())).unwrap();
    to.paste_special(&clip, 0, range("A1"), &PasteSpecial::default()).unwrap();
    assert!(is_bold(&to, "A1"));
    assert_eq!(show(&to, "A1"), "5");
}

#[test]
fn text_from_another_program_is_not_a_formula_unless_asked() {
    let mut s = session();
    s.paste_text(0, at("A1"), "=1+2\t+5\t-x\t@A1\r\n", ExternalPaste::default()).unwrap();
    let cells: Vec<String> = ["A1", "B1", "C1", "D1"].iter().map(|a| show(&s, a)).collect();
    assert_eq!(cells, ["=1+2", "5", "-x", "@A1"]);
    assert!(s.book.sheets[0].cell(at("A1")).unwrap().formula.is_none());
    let quoted = s.book.sheets[0].cell(at("A1")).unwrap().style;
    assert!(
        s.book.styles.xf(quoted).quote_prefix,
        "a text that looks like a formula keeps its apostrophe"
    );
    s.paste_text(0, at("A2"), "=1+2", ExternalPaste { formulas: true }).unwrap();
    assert_eq!(show(&s, "A2"), "3");
    assert!(s.book.sheets[0].cell(at("A2")).unwrap().formula.is_some());
}

#[test]
fn html_from_another_program_is_not_a_formula_unless_asked() {
    let mut s = session();
    let page = "<table><tr><td>=1+1</td><td>7</td></tr></table>";
    s.paste_html(0, at("A1"), page, ExternalPaste::default()).unwrap();
    assert_eq!((show(&s, "A1"), show(&s, "B1")), ("=1+1".to_owned(), "7".to_owned()));
    assert!(s.book.sheets[0].cell(at("A1")).unwrap().formula.is_none());
    assert_eq!(s.undo().as_deref(), Some("Paste"));
    assert_eq!(show(&s, "B1"), "");
}

#[test]
fn a_paste_over_part_of_an_array_formula_is_refused() {
    let mut s = session();
    s.set_array_formula(0, range("A1:A3"), "=ROW(A1:A3)").unwrap();
    put(&mut s, "C1", "1");
    let clip = s.copy(0, range("C1"), false);
    let refused = s.paste_special(&clip, 0, range("A2"), &PasteSpecial::default());
    assert!(refused.unwrap_err().contains("array"));
}

#[test]
fn insert_copied_cells_is_one_undo_step() {
    let mut s = session();
    put(&mut s, "A1", "1");
    put(&mut s, "B1", "2");
    put(&mut s, "B2", "3");
    let clip = s.copy(0, range("A1"), false);
    s.insert_copied(&clip, 0, at("B1"), Shift::Down).unwrap();
    assert_eq!(
        (show(&s, "B1"), show(&s, "B2"), show(&s, "B3")),
        ("1".into(), "2".into(), "3".into())
    );
    s.undo();
    assert_eq!(
        (show(&s, "B1"), show(&s, "B2"), show(&s, "B3")),
        ("2".into(), "3".into(), String::new())
    );
}

#[test]
fn insert_cut_cells_moves_them_and_the_references_to_them_follow() {
    let mut s = session();
    put(&mut s, "A1", "7");
    put(&mut s, "C1", "=A1+1");
    put(&mut s, "D1", "10");
    let clip = s.copy(0, range("A1"), true);
    s.insert_copied(&clip, 0, at("D1"), Shift::Right).unwrap();
    assert_eq!(show(&s, "A1"), "");
    let formula =
        s.book.sheets[0].cell(at("B1")).and_then(|c| c.formula.as_ref()).map(|f| f.display());
    assert_eq!(formula.as_deref(), Some("=C1+1"), "the formula now names where the cell went");
    assert_eq!(show(&s, "B1"), "8");
}

#[test]
fn control_drag_copies_what_counts_on_and_counts_on_a_single_number() {
    let mut s = session();
    put(&mut s, "A1", "1");
    put(&mut s, "A2", "2");
    put(&mut s, "C1", "5");
    s.fill_series_with_control(0, range("A1:A2"), range("A1:A5"), &CustomLists::default()).unwrap();
    let column: Vec<String> = (1..=5).map(|r| show(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["1", "2", "1", "2", "1"]);
    s.fill_series_with_control(0, range("C1"), range("C1:C3"), &CustomLists::default()).unwrap();
    assert_eq!((show(&s, "C2"), show(&s, "C3")), ("6".into(), "7".into()));
}

#[test]
fn a_custom_list_continues_and_wraps() {
    let mut s = session();
    let mut lists = CustomLists::default();
    lists.add(vec!["Low".to_owned(), "Medium".to_owned(), "High".to_owned()]).unwrap();
    put(&mut s, "A1", "Medium");
    s.fill_series_with(0, range("A1"), range("A1:A4"), FillKind::Default, &lists).unwrap();
    let column: Vec<String> = (1..=4).map(|r| show(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["Medium", "High", "Low", "Medium"]);
    assert!(lists.add(vec!["one".to_owned()]).is_err());
}

#[test]
fn a_double_click_on_the_handle_fills_down_the_data_beside_it() {
    let mut s = session();
    put(&mut s, "A1", "1");
    for (r, v) in ["a", "b", "c", "d"].iter().enumerate() {
        put(&mut s, &format!("B{}", r + 1), v);
    }
    put(&mut s, "B6", "separate");
    assert_eq!(s.fill_handle_extent(0, range("A1")), Some(range("A1:A4")));
    assert_eq!(
        s.fill_handle_extent(0, range("C1")),
        Some(range("C1:C4")),
        "the data on the left is used first"
    );
    assert_eq!(s.fill_handle_extent(0, range("F1")), None);
}

#[test]
fn flash_fill_asks_for_a_pattern_and_is_one_undo_step() {
    let mut s = session();
    put(&mut s, "A1", "John Smith");
    put(&mut s, "A2", "Mary Jones");
    put(&mut s, "B1", "Smith");
    s.flash_fill(0, range("B1:B2")).unwrap();
    assert_eq!(show(&s, "B2"), "Jones");
    s.undo();
    assert_eq!(show(&s, "B2"), "");
    put(&mut s, "B2", "zzz");
    put(&mut s, "A3", "x");
    put(&mut s, "B3", "q");
    let refused = s.flash_fill(0, range("B1:B3")).unwrap_err();
    assert!(refused.starts_with("Flash Fill didn't recognize a pattern"));
}

#[test]
fn justify_refuses_numbers_and_is_one_undo_step() {
    let mut s = session();
    put(&mut s, "A1", "the quick brown fox jumps over the lazy dog");
    s.set_column_width(0, 0, 0, Some(10.7)).unwrap();
    s.justify(0, range("A1:A2")).unwrap();
    assert_eq!(show(&s, "A1"), "the quick");
    s.undo();
    assert_eq!(show(&s, "A1"), "the quick brown fox jumps over the lazy dog");
    put(&mut s, "A2", "5");
    assert!(s.justify(0, range("A1:A2")).unwrap_err().contains("Cannot justify"));
}

#[test]
fn a_series_dialog_on_one_cell_with_a_stop_value_fills_as_far_as_the_stop() {
    let mut s = session();
    put(&mut s, "A1", "1");
    let spec = SeriesSpec { stop: Some(9.0), ..SeriesSpec::linear(2.0) };
    s.data_series(0, range("A1"), &spec).unwrap();
    let column: Vec<String> = (1..=6).map(|r| show(&s, &format!("A{r}"))).collect();
    assert_eq!(column, ["1", "3", "5", "7", "9", ""]);
    assert!(matches!(s.book.sheets[0].value_at(at("A2")), Value::Number(_)));
}

#[test]
fn fill_across_sheets_is_one_undo_step() {
    let mut s = session();
    s.add_sheet(Some("Two"), None).unwrap();
    put(&mut s, "A1", "hello");
    s.fill_across_sheets(0, range("A1"), &[1], AcrossSheets::All).unwrap();
    assert_eq!(s.book.sheets[1].value_at(at("A1")).to_text(), "hello");
    s.undo();
    assert_eq!(s.book.sheets[1].value_at(at("A1")).to_text(), "");
}
