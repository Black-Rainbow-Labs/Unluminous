//! A picture placed in a cell (Picture > Place in Cell): the rich value parts Excel writes, read and kept,
//! and written for a picture placed here, with Excel reading both the same way.
//!
//! `fixtures/objects/place-in-cell.xlsx` was made by Excel through `tools/excel-oracle/place-in-cell.ps1`.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects").join(name))
        .unwrap()
}

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn reread(book: &Workbook) -> Workbook {
    unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(book).unwrap()).unwrap()
}

#[test]
fn the_image_in_a_cell_excel_made_is_found_and_kept() {
    let s = Session::new(
        unluminous_sheet::xlsx::read(&fixture("place-in-cell.xlsx")).unwrap(),
        46318.5,
    );
    let part = s.cell_image(0, at("A1")).expect("the cell shows an image");
    assert!(s.book.kept.get(&part).unwrap().starts_with(&[0x89, b'P', b'N', b'G']));
    let back = reread(&s.book);
    for p in [
        "xl/metadata.xml",
        "xl/richData/rdrichvalue.xml",
        "xl/richData/richValueRel.xml",
        "xl/richData/rdrichvaluestructure.xml",
    ] {
        assert_eq!(s.book.kept.get(p), back.kept.get(p), "{p}");
    }
    assert_eq!(Session::new(back, 46318.5).cell_image(0, at("A1")), Some(part));
}

#[test]
fn a_picture_is_placed_in_a_cell_and_read_back() {
    let mut s = Session::new(Workbook::new(), 46318.5);
    let png = fixture("probe-picture.png");
    let id = s.insert_picture(0, &png, at("D4")).unwrap();
    s.set_alt_text(0, id, "A probe").unwrap();
    s.set_value(0, at("B2"), unluminous_sheet::Value::Number(1.0)).unwrap();
    assert!(s.place_in_cell(0, id, at("B2")).is_err(), "the cell is not empty");
    s.place_in_cell(0, id, at("C3")).unwrap();
    assert!(s.book.sheets[0].drawing.objects.is_empty());
    let two = s.insert_picture(0, &png, at("D4")).unwrap();
    s.place_in_cell(0, two, at("C4")).unwrap();
    let back = Session::new(reread(&s.book), 46318.5);
    for cell in ["C3", "C4"] {
        let part = back.cell_image(0, at(cell)).unwrap_or_else(|| panic!("{cell} shows no image"));
        assert_eq!(back.book.kept.get(&part).unwrap().as_slice(), png.as_slice());
    }
    assert!(back.cell_image(0, at("B2")).is_none());
    s.undo();
    s.undo();
    assert!(s.cell_image(0, at("C3")).is_some(), "undo went back two steps, not three");
}

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn inspect(paths: &[&Path]) -> Vec<serde_json::Value> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("tools/excel-oracle/objects-inspect.ps1");
    let list = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(",");
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(script)
        .arg("-Paths")
        .arg(list)
        .output()
        .expect("pwsh runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().trim_start_matches('\u{FEFF}'))
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn excel_reads_a_picture_placed_here_as_it_reads_its_own() {
    if !excel_is_installed() {
        assert!(
            std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
            "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
        );
        println!("Excel is not installed here, so a picture in a cell was not checked");
        return;
    }
    let mut s = Session::new(Workbook::new(), 46318.5);
    let id = s.insert_picture(0, &fixture("probe-picture.png"), at("D4")).unwrap();
    s.set_alt_text(0, id, "A probe").unwrap();
    s.place_in_cell(0, id, at("A1")).unwrap();
    let folder: PathBuf =
        std::env::temp_dir().join(format!("unluminous-incell-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let mine = folder.join("placed-here.xlsx");
    std::fs::write(&mine, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
    let theirs =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects/place-in-cell.xlsx");
    let results = inspect(&[&mine, &theirs]);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["repaired"], false, "{}", results[0]["problem"]);
    assert_eq!(results[0]["sheets"][0]["shapes"], results[1]["sheets"][0]["shapes"]);
    assert_eq!(
        results[0]["sheets"][0]["cells"], results[1]["sheets"][0]["cells"],
        "Excel reads the cell as it reads its own"
    );
    let _ = std::fs::remove_file(&mine);
    let _ = std::fs::remove_dir(&folder);
}
