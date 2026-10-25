//! Looking inside a binary workbook, for working out a record's layout. Nothing here runs by default.
//!
//!     FIXTURE=05-condfmt.xlsb PART=xl/worksheets/sheet1.bin RECORDS=463,471 cargo test -p unluminous-sheet --test legacy_dump -- --ignored --nocapture
//!
//! `PART=list` lists the parts of the file. `FULL=1` prints every byte of a record.

use std::path::Path;

/// The records of one part of an `.xlsb` fixture, optionally only some ids, with their bytes.
#[test]
#[ignore]
fn dump_records_of_an_xlsb_part() {
    let name = std::env::var("FIXTURE").unwrap_or_else(|_| "features.xlsb".to_owned());
    let part = std::env::var("PART").unwrap_or_else(|_| "xl/workbook.bin".to_owned());
    let only: Vec<u16> = std::env::var("RECORDS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy").join(&name);
    let bytes = std::fs::read(path).unwrap();
    let entries = unluminous_sheet::zip::read(&bytes).unwrap();
    if part == "list" {
        for e in &entries {
            println!("{} {}", e.name, e.data.len());
        }
        return;
    }
    let entry = entries.iter().find(|e| e.name == part).expect("part");
    for record in unluminous_sheet::xlsb::records::split(&entry.data) {
        if !only.is_empty() && !only.contains(&record.id) {
            continue;
        }
        let limit = if std::env::var("FULL").is_ok() { usize::MAX } else { 120 };
        let hex: Vec<String> = record.data.iter().take(limit).map(|b| format!("{b:02x}")).collect();
        println!("{} len {}: {}", record.id, record.data.len(), hex.join(" "));
    }
}
