//! Write one small workbook for each feature the writer produces, so Excel can judge each on its own:
//! `cargo run -p unluminous-sheet --example feature_files -- <folder>`, then
//! `tools/excel-oracle/check-folder.ps1 -Folder <folder>`.

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::{LinkTarget, Workbook};
use unluminous_sheet::styles::{Color, FontEdit, StyleChange};

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn base() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    s.set_input(0, at("A1"), "Item").unwrap();
    s.set_input(0, at("B1"), "Qty").unwrap();
    s.set_input(0, at("A2"), "Tea").unwrap();
    s.set_input(0, at("B2"), "3").unwrap();
    s.set_input(0, at("A3"), "Cake").unwrap();
    s.set_input(0, at("B3"), "5").unwrap();
    s
}

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder"));
    std::fs::create_dir_all(&folder).unwrap();
    let mut cases: Vec<(&str, Session)> = Vec::new();
    cases.push(("values", base()));
    let mut s = base();
    s.set_input(0, at("C2"), "=B2*2").unwrap();
    s.set_input(0, at("C4"), "=SUM(B2:B3)").unwrap();
    cases.push(("formulas", s));
    let mut s = base();
    s.set_input(0, at("D1"), "=SORT(A2:A3)").unwrap();
    cases.push(("spill-text", s));
    let mut s = base();
    s.set_input(0, at("D1"), "=SEQUENCE(2,2)").unwrap();
    cases.push(("spill-numbers", s));
    let mut s = base();
    s.set_input(0, at("D1"), "=SEQUENCE(2,2)").unwrap();
    s.set_input(0, at("E2"), "x").unwrap();
    cases.push(("spill-blocked", s));
    let mut s = base();
    s.set_input(0, at("D1"), "=XLOOKUP(\"Cake\",A2:A3,B2:B3)").unwrap();
    s.set_input(0, at("D2"), "=LET(x,B2:B3,SUM(x*2))").unwrap();
    cases.push(("new-functions", s));
    let mut s = base();
    s.set_input(0, at("D1"), "10/23/2026").unwrap();
    s.set_input(0, at("D2"), "=TEXT(D1,\"dddd\")").unwrap();
    s.set_input(0, at("D3"), "12.5%").unwrap();
    cases.push(("formats", s));
    let mut s = base();
    s.set_style(0, &[Range::parse("A1:B1").unwrap()], &StyleChange::Font(FontEdit { bold: Some(true), color: Some(Color::rgb(0x1F, 0x4E, 0x79)), ..Default::default() })).unwrap();
    s.set_style(0, &[Range::parse("A1:B1").unwrap()], &StyleChange::Fill(Some(Color::rgb(0xDD, 0xEB, 0xF7)))).unwrap();
    cases.push(("styles", s));
    let mut s = base();
    s.set_input(0, at("A5"), "Merged").unwrap();
    s.merge(0, Range::parse("A5:C5").unwrap(), true).unwrap();
    cases.push(("merge", s));
    let mut s = base();
    s.freeze(0, 1, 0).unwrap();
    s.set_column_width(0, 0, 0, Some(16.0)).unwrap();
    s.set_row_height(0, 2, 2, Some(24.0)).unwrap();
    cases.push(("layout", s));
    let mut s = base();
    s.set_note(0, at("A2"), Some("Green tea"), "Jason").unwrap();
    cases.push(("note", s));
    let mut s = base();
    s.set_link(0, Range::parse("A5").unwrap(), Some(LinkTarget::External("https://example.com/".to_owned())), Some("Example".to_owned())).unwrap();
    s.set_link(0, Range::parse("A6").unwrap(), Some(LinkTarget::Location("Sheet1!A1".to_owned())), Some("Top".to_owned())).unwrap();
    cases.push(("links", s));
    let mut s = base();
    s.add_table(0, Range::parse("A1:B3").unwrap(), Some("Orders")).unwrap();
    s.set_input(0, at("D1"), "=SUM(Orders[Qty])").unwrap();
    cases.push(("table", s));
    let mut s = base();
    s.add_sheet(Some("Notes"), None).unwrap();
    s.set_input(1, at("A1"), "=Sheet1!B2").unwrap();
    s.set_input(1, at("A2"), "Line one\nline two").unwrap();
    s.set_tab_color(1, Some(Color::rgb(0xFF, 0xC0, 0))).unwrap();
    cases.push(("sheets", s));
    for (name, session) in cases {
        let path = folder.join(format!("{name}.xlsx"));
        std::fs::write(&path, unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap();
        println!("{}", path.display());
    }
}
