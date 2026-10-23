//! Read a workbook, compute it, and write it again: `cargo run -p unluminous-sheet --example roundtrip --
//! <in.xlsx> <out.xlsx>`. Prints each sheet's cells as Unluminous computed them, so the output can be
//! compared with what `tools/excel-oracle/inspect.ps1` says Excel sees.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let bytes = std::fs::read(input).expect("read the workbook");
    let mut book = unluminous_sheet::xlsx::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
    unluminous_sheet::calc::recalculate_all(&mut book, 46318.5);
    for sheet in &book.sheets {
        println!("== {} ({} cells)", sheet.name, sheet.cells.len());
        for (&(row, col), cell) in sheet.cells.iter().take(60) {
            let at = unluminous_sheet::CellRef::new(row, col);
            let formula = cell.formula.as_ref().map(|f| f.display()).unwrap_or_default();
            println!("{at}: {:?} {formula}", cell.value);
        }
    }
    let written = unluminous_sheet::xlsx::write(&book).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(output, written).expect("write the workbook");
}
