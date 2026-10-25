//! What the binary readers do with files that are not what they expect: a password, damage, and a request
//! to save in a format only Excel writes.

use std::path::{Path, PathBuf};

use unluminous_sheet::{FileKind, Workbook};

fn legacy_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

fn open_bytes(name: &str, bytes: &[u8]) -> Result<Workbook, String> {
    let kind = FileKind::of(Path::new(name)).unwrap();
    unluminous_sheet::open(bytes, kind, name, 2026)
}

#[test]
fn a_workbook_with_a_password_to_open_is_refused_in_every_format() {
    for name in ["password.xls", "password.xlsx", "password.xlsb"] {
        let bytes = std::fs::read(legacy_dir().join(name)).unwrap();
        let refusal = open_bytes(name, &bytes).expect_err(name);
        assert!(refusal.contains("protected with a password"), "{name}: {refusal}");
    }
}

#[test]
fn a_workbook_protected_without_a_password_to_open_is_read() {
    // Excel encrypts a workbook with its structure protected under a password everyone knows.
    let bytes = std::fs::read(legacy_dir().join("02-protection.xls")).unwrap();
    let book = open_bytes("02-protection.xls", &bytes).unwrap();
    assert_eq!(book.sheets.len(), 2);
    assert!(book
        .raw
        .iter()
        .any(|(name, xml)| name == "workbookProtection" && xml.contains("lockStructure")));
    assert!(book.sheets[0]
        .raw
        .elements
        .iter()
        .any(|(name, xml)| name == "sheetProtection" && xml.contains("sheet=\"1\"")));
}

#[test]
fn the_older_formats_are_opened_and_never_saved() {
    assert_eq!(FileKind::of(Path::new("old.xls")), Ok(FileKind::Xls));
    assert_eq!(FileKind::of(Path::new("binary.XLSB")), Ok(FileKind::Xlsb));
    let book = Workbook::new();
    for kind in [FileKind::Xls, FileKind::Xlsb] {
        let refusal = unluminous_sheet::save(&book, kind).unwrap_err();
        assert!(refusal.contains(".xlsx"), "{refusal}");
    }
}

#[test]
fn a_binary_file_named_for_another_format_is_read_as_what_it_is() {
    let bytes = std::fs::read(legacy_dir().join("features.xls")).unwrap();
    let book = unluminous_sheet::open(&bytes, FileKind::Xlsx, "renamed", 2026).unwrap();
    assert_eq!(book.sheets.len(), 3);
}

/// A damaged file is an error or a workbook with less in it, and never a panic or a long wait. Set
/// `DAMAGE_STEPS` to try more places in each file.
#[test]
fn damaged_files_do_not_stop_the_reader() {
    let places: usize =
        std::env::var("DAMAGE_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(13);
    for name in [
        "features.xls",
        "features.xlsb",
        "05-condfmt.xls",
        "05-condfmt.xlsb",
        "18-formulas.xls",
        "07-table.xlsb",
    ] {
        let bytes = std::fs::read(legacy_dir().join(name)).unwrap();
        let step = (bytes.len() / places).max(1);
        let try_open = |what: &str, bytes: &[u8]| {
            let started = std::time::Instant::now();
            let _ = open_bytes(name, bytes);
            assert!(started.elapsed().as_secs() < 5, "{name} {what} took {:?}", started.elapsed());
        };
        for cut in (0..bytes.len()).step_by(step) {
            try_open(&format!("cut at {cut}"), &bytes[..cut]);
        }
        for at in (0..bytes.len()).step_by(step) {
            let mut damaged = bytes.clone();
            damaged[at] ^= 0xFF;
            try_open(&format!("flipped at {at}"), &damaged);
            damaged[at] = 0;
            try_open(&format!("zeroed at {at}"), &damaged);
        }
    }
}
