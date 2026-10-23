//! Evaluate formulas against a workbook: `cargo run -p unluminous-sheet --example eval -- <book.xlsx>
//! <sheet> <cell> <formula>...`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("read");
    let mut book = unluminous_sheet::xlsx::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
    unluminous_sheet::calc::recalculate_all(&mut book, 46318.5);
    println!("circular: {:?}", book.circular);
    for s in &book.sheets { for (k, c) in &s.cells { if let Some(f) = &c.formula { println!("{} {:?} {} dyn={} frozen={} -> {:?}", s.name, k, f.text, f.dynamic, f.frozen, c.value); } } }
    let sheet = book.sheet_index(&args[2]).expect("sheet");
    let at = unluminous_sheet::CellRef::parse(&args[3]).expect("cell");
    for formula in &args[4..] {
        println!("{formula} => {:?}", unluminous_sheet::eval::evaluate(&book, sheet, at, formula, 46318.5));
    }
}
