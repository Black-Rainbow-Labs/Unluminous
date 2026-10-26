//! Write a workbook with one chart for each chart type that is not a `chartEx` type, and one workbook
//! for each type, so a type Excel does not accept can be found.
//!
//!   cargo run -p unluminous-sheet --example chart_files -- <folder>

use unluminous_sheet::chartspec::ChartType;
use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;

fn data() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    for (c, t) in ["Cat", "One", "Two"].iter().enumerate() {
        s.set_input(0, CellRef::new(0, c as u32), t).unwrap();
    }
    for r in 1..=5u32 {
        s.set_input(0, CellRef::new(r, 0), &format!("c{r}")).unwrap();
        s.set_input(0, CellRef::new(r, 1), &format!("{}", r * 3)).unwrap();
        s.set_input(0, CellRef::new(r, 2), &format!("{}", r * 2 + 1)).unwrap();
    }
    s
}

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).expect("a folder"));
    std::fs::create_dir_all(&folder).unwrap();
    let range = Range::new(CellRef::new(0, 0), CellRef::new(5, 2));
    let mut all = data();
    for ty in ChartType::ALL.iter().filter(|t| !t.is_extended()) {
        let mut one = data();
        one.insert_chart(0, range, *ty, None).unwrap();
        all.insert_chart(0, range, *ty, None).unwrap();
        let path = folder.join(format!("type-{}.xlsx", ty.name()));
        std::fs::write(&path, unluminous_sheet::xlsx::write(&one.book).unwrap()).unwrap();
    }
    std::fs::write(
        folder.join("all-types.xlsx"),
        unluminous_sheet::xlsx::write(&all.book).unwrap(),
    )
    .unwrap();
}
