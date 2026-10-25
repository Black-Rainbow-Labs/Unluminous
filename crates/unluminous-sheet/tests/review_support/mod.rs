//! What the review tests share: the fixtures Excel made, Excel itself, and a workbook's cells as text.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::CellRef;
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;

/// The folder of fixtures Excel made for the review lane.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/review").join(name)
}

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// A fixture workbook read.
pub fn fixture_book(name: &str) -> Workbook {
    unluminous_sheet::xlsx::read(&fixture_bytes(name)).unwrap()
}

/// A fixture workbook as a session, computed at a fixed time.
pub fn fixture_session(name: &str) -> Session {
    Session::new(fixture_book(name), 46318.5)
}

/// What Excel said about a fixture, as JSON.
pub fn fixture_json(name: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(fixture_path(name)).unwrap();
    serde_json::from_str(text.trim().trim_start_matches('\u{FEFF}')).unwrap()
}

pub fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

pub fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Whether the tests that start Excel can run. Without Excel they print a line and pass, unless
/// `UNLUMINOUS_REQUIRE_EXCEL` is set.
pub fn excel_available(what: &str) -> bool {
    if excel_is_installed() {
        return true;
    }
    assert!(
        std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
        "UNLUMINOUS_REQUIRE_EXCEL is set and Excel is not installed"
    );
    println!("Excel is not installed here, so {what} was not checked");
    false
}

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// Run one of the scripts in `tools/excel-oracle` and read its output as JSON.
pub fn excel_script(script: &str, args: &[&str]) -> serde_json::Value {
    let output = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(repo_root().join("tools/excel-oracle").join(script))
        .args(args)
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim().trim_start_matches('\u{FEFF}');
    serde_json::from_str(text).unwrap_or_else(|e| {
        panic!(
            "{script}: Excel's answer did not read: {e}\n{text}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// A scratch file path in a folder of its own, which is removed when the guard drops.
pub struct Scratch {
    pub folder: PathBuf,
}

impl Scratch {
    pub fn new(tag: &str) -> Self {
        let folder =
            std::env::temp_dir().join(format!("unluminous-review-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        Self { folder }
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.folder.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}
