//! The data tools against Excel: sort, filter, Advanced Filter, Remove Duplicates, Text to Columns,
//! Consolidate and the table operations.
//!
//! `tests/fixtures/datatools/<family>.json` is a list of scenarios: the cells to fill, the formats to apply,
//! steps to set up and the operation. `<family>.excel.json` is what Excel 16 did with each, written by
//! `tools/excel-oracle/data-tools.ps1`: every cell (value, formula, number format), the hidden rows, the
//! tables, the notes, and the XML Excel saved for the filters and the tables. This test performs the same
//! steps with the engine and compares all of it. Both files are committed, so the test runs with no Excel.
//!
//! ```text
//! pwsh tools/excel-oracle/data-tools.ps1 -Scenarios tests/fixtures/datatools/sort.json -Out tests/fixtures/datatools/sort.excel.json
//! ```
//!
//! `DATATOOLS=<family>` limits a run to one family and `DATATOOLS_CASE=<name>` to one scenario.

mod datatools_support;

use datatools_support::*;

#[test]
fn every_scenario_gives_what_excel_gave() {
    let mut failures = Vec::new();
    let mut total = 0;
    for family in families() {
        for (scenario, expected) in cases(&family) {
            if let Ok(only) = std::env::var("DATATOOLS_CASE") {
                if scenario["name"] != only.as_str() {
                    continue;
                }
            }
            total += 1;
            let problems = check(&scenario, &expected);
            for p in problems {
                failures
                    .push(format!("{family}/{}: {p}", scenario["name"].as_str().unwrap_or("?")));
            }
        }
    }
    assert!(total > 0, "no scenarios found");
    if !failures.is_empty() {
        panic!(
            "{} of {} scenarios differ from Excel:\n{}",
            failures.len(),
            total,
            failures.join("\n")
        );
    }
}
