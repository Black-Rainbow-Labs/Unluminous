//! Excel opens the outlines and subtotals written here without repairing the file.

use std::path::{Path, PathBuf};

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::{Session, SubtotalFunction, SubtotalSpec};
use unluminous_sheet::FileKind;

fn open(file: &str) -> unluminous_sheet::Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/outline").join(file);
    unluminous_sheet::open(&std::fs::read(path).unwrap(), FileKind::Xlsx, file, 2026).unwrap()
}

fn request(summary_below: bool) -> SubtotalSpec {
    SubtotalSpec {
        range: Range::new(CellRef::new(0, 0), CellRef::new(12, 3)),
        group_by: 0,
        function: SubtotalFunction::Sum,
        total_columns: vec![2, 3],
        replace: true,
        page_breaks: false,
        summary_below,
    }
}

#[test]
fn excel_opens_outlines_and_subtotals_written_here_without_repairing_the_file() {
    let installed = cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success());
    if !installed {
        assert!(std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err());
        println!("Excel is not installed here, so the files were not opened");
        return;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/excel-oracle/inspect.ps1");
    let folder = std::env::temp_dir().join(format!("unluminous-outline-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let mut written: Vec<PathBuf> = Vec::new();
    for (name, below) in
        [("subtotal-sum-in", true), ("subtotal-sum-in", false), ("auto-three-levels-in", true)]
    {
        let mut s = Session::new(open(&format!("{name}.xlsx")), 46318.5);
        if name.starts_with("subtotal") {
            s.subtotal(0, &request(below)).unwrap();
            s.show_levels(0, 2, 0).unwrap();
        } else {
            s.auto_outline(0, None).unwrap();
        }
        let path = folder.join(format!("{name}-{below}.xlsx"));
        std::fs::write(&path, unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap();
        let out = std::process::Command::new("pwsh")
            .args(["-NoProfile", "-File"])
            .arg(&script)
            .arg("-Path")
            .arg(&path)
            .output()
            .unwrap();
        let text =
            String::from_utf8_lossy(&out.stdout).trim().trim_start_matches('\u{FEFF}').to_owned();
        let json: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        assert_eq!(json["repaired"], false, "{name}: {}", json["problem"]);
        written.push(path);
    }
    for p in written {
        let _ = std::fs::remove_file(p);
    }
    let _ = std::fs::remove_dir(&folder);
}
