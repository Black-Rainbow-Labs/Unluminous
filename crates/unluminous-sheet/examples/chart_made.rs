//! Write one workbook for each extended chart type, each made with a session, so the one Excel refuses can
//! be found.
//!
//!   cargo run -p unluminous-sheet --example chart_made -- <folder>

use unluminous_sheet::chartspec::ChartType;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;

/// A session whose first sheet holds eight rows of a name and a number under a header.
fn data() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    let rows = [
        ("a", 5.0),
        ("b", 9.0),
        ("c", 3.0),
        ("a", 7.0),
        ("b", 4.0),
        ("c", 8.0),
        ("a", 6.0),
        ("b", 2.0),
    ];
    for (r, (name, v)) in rows.iter().enumerate() {
        s.set_input(0, CellRef::new(r as u32 + 1, 0), name).unwrap();
        s.set_input(0, CellRef::new(r as u32 + 1, 1), &format!("{v}")).unwrap();
    }
    s.set_input(0, CellRef::new(0, 0), "Group").unwrap();
    s.set_input(0, CellRef::new(0, 1), "Value").unwrap();
    s
}

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder"));
    std::fs::create_dir_all(&folder).unwrap();
    let range = Range::new(CellRef::new(0, 0), CellRef::new(8, 1));
    for ty in ChartType::ALL.iter().filter(|t| t.is_extended() && **t != ChartType::Map) {
        let mut s = data();
        s.insert_chart(0, range, *ty, None).unwrap();
        let name = format!("made-{}.xlsx", ty.name());
        std::fs::write(folder.join(name), unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    }
}
