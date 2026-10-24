//! Write the sample workbook the file tests build, for looking at by hand: `cargo run -p unluminous-sheet
//! --example build_sample -- <out.xlsx>`.

#[path = "../tests/files.rs"]
#[allow(dead_code)]
mod files;

fn main() {
    let out = std::env::args().nth(1).expect("an output path");
    let session = files::built_for_example();
    std::fs::write(out, unluminous_sheet::xlsx::write(&session.book).unwrap()).unwrap();
}
