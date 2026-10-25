//! Charts: reading Excel's own, writing them again from the model, making them here, and checking what
//! Excel makes of the files.
//!
//! The Excel checks start the installed Excel through `tools/excel-oracle/charts-inspect.ps1` and pass with
//! a printed line where Excel is not installed (`UNLUMINOUS_REQUIRE_EXCEL=1` makes that a failure).

use std::path::{Path, PathBuf};

use unluminous_sheet::chartspec::*;
use unluminous_sheet::coord::{CellRef, Range};

use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/charts").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn excel_is_installed() -> bool {
    cfg!(windows)
        && std::process::Command::new("reg")
            .args(["query", "HKCR\\Excel.Application"])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).unwrap().to_path_buf()
}

/// Whether Excel checks can run; a missing Excel is a printed line unless the environment requires it.
fn excel_available() -> bool {
    if excel_is_installed() {
        return true;
    }
    assert!(std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(), "Excel is required and is not installed");
    println!("Excel is not installed here, so what it makes of the charts was not checked");
    false
}

/// Only one Excel at a time: the tests run on threads, and two instances of Excel fight over the server.
static EXCEL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What Excel sees of every chart in some files, from one run of Excel (opening a workbook with many charts
/// takes Excel most of a minute).
fn inspect_many(paths: &[PathBuf]) -> Vec<serde_json::Value> {
    let _one_at_a_time = EXCEL.lock().unwrap_or_else(|e| e.into_inner());
    let script = repo_root().join("tools/excel-oracle/charts-inspect.ps1");
    // Other programs on a busy machine sometimes close the Excel this one started; a second try is made.
    let mut last = String::new();
    for _ in 0..3 {
        let mut command = std::process::Command::new("pwsh");
        command.args(["-NoProfile", "-File"]).arg(&script).arg("-Path");
        command.arg(paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(","));
        let output = command.output().expect("pwsh runs");
        let text = String::from_utf8_lossy(&output.stdout);
        let text = text.trim().trim_start_matches('\u{FEFF}');
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => return value.as_array().cloned().unwrap_or_else(|| vec![value]),
            Err(e) => last = format!("{e}\n{text}\n{}", String::from_utf8_lossy(&output.stderr)),
        }
    }
    panic!("Excel's answer did not read: {last}");
}
fn scratch(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("unluminous-sheet-charts-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    folder.join(name)
}

fn save(book: &Workbook, name: &str) -> PathBuf {
    let path = scratch(name);
    std::fs::write(&path, unluminous_sheet::xlsx::write(book).unwrap()).unwrap();
    path
}

#[test]
fn excels_chart_workbook_reads_with_every_type_and_option() {
    let book = unluminous_sheet::xlsx::read(&fixture("14-charts.xlsx")).unwrap();
    let sheet = book.sheet_index("Charts").unwrap();
    let charts = &book.sheets[sheet].charts;
    assert_eq!(charts.len(), 19, "13 chart parts and 6 chartEx parts, the 14th chart part is on a chart sheet");
    let types: Vec<ChartType> = charts.iter().map(|c| c.chart_type()).collect();
    for expected in [
        ChartType::ColumnClustered,
        ChartType::ColumnStacked,
        ChartType::LineMarkers,
        ChartType::Pie,
        ChartType::Doughnut,
        ChartType::Bubble,
        ChartType::StockHlc,
        ChartType::RadarMarkers,
        ChartType::Surface,
        ChartType::Combo,
        ChartType::Treemap,
        ChartType::Sunburst,
        ChartType::Histogram,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::BoxWhisker,
    ] {
        assert!(types.contains(&expected), "{expected:?} is missing from {types:?}");
    }
    let sheet_one = book.sheets.iter().find(|s| s.kept_part.is_some()).expect("the chart sheet");
    assert_eq!(sheet_one.charts.len(), 1, "a chart sheet's chart is read");
    // The chartEx data names resolve to ranges.
    let treemap = charts.iter().find(|c| c.chart_type() == ChartType::Treemap).unwrap();
    assert!(treemap.series[0].values.contains("Charts!"), "{:?}", treemap.series[0]);
}

#[test]
fn every_chart_written_from_the_model_reads_back_the_same() {
    let book = unluminous_sheet::xlsx::read(&fixture("14-charts.xlsx")).unwrap();
    let mut changed = book.clone();
    for sheet in &mut changed.sheets {
        for chart in &mut sheet.charts {
            chart.spec.dirty = true;
        }
    }
    let bytes = unluminous_sheet::xlsx::write(&changed).unwrap();
    let again = unluminous_sheet::xlsx::read(&bytes).unwrap();
    for (a, b) in book.sheets.iter().zip(&again.sheets) {
        assert_eq!(a.charts.len(), b.charts.len(), "sheet {}", a.name);
        for (x, y) in a.charts.iter().zip(&b.charts) {
            assert_eq!(x.chart_type(), y.chart_type());
            assert_eq!(x.title, y.title);
            assert_eq!(x.series.len(), y.series.len());
            for (s, t) in x.series.iter().zip(&y.series) {
                assert_eq!((&s.name, &s.categories, &s.values), (&t.name, &t.categories, &t.values));
                assert_eq!(s.fmt.trendlines, t.fmt.trendlines);
                assert_eq!(s.fmt.error_bars, t.fmt.error_bars);
                assert_eq!(s.fmt.labels, t.fmt.labels);
            }
            assert_eq!(x.spec.groups.len(), y.spec.groups.len());
            assert_eq!(x.spec.axes, y.spec.axes);
            assert_eq!(x.spec.legend, y.spec.legend);
        }
    }
}

fn data_session() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    s.set_input(0, CellRef::new(0, 0), "Cat").unwrap();
    s.set_input(0, CellRef::new(0, 1), "One").unwrap();
    s.set_input(0, CellRef::new(0, 2), "Two").unwrap();
    for r in 1..=5u32 {
        s.set_input(0, CellRef::new(r, 0), &format!("c{r}")).unwrap();
        s.set_input(0, CellRef::new(r, 1), &format!("{}", r * 3)).unwrap();
        s.set_input(0, CellRef::new(r, 2), &format!("{}", r * 2 + 1)).unwrap();
    }
    s
}

/// A workbook with one chart of every type that is not a `chartEx` type, made by `insert_chart`.
fn every_type_workbook() -> Session {
    let mut s = data_session();
    let range = Range::new(CellRef::new(0, 0), CellRef::new(5, 2));
    for ty in ChartType::ALL {
        if ty.is_extended() {
            continue;
        }
        s.insert_chart(0, range, *ty, None).unwrap_or_else(|e| panic!("{ty:?}: {e}"));
    }
    s
}

#[test]
fn every_type_can_be_inserted() {
    let s = every_type_workbook();
    assert!(s.book.sheets[0].charts.len() > 40);
}

/// The parts of a package under some folders, by name.
fn parts_under(bytes: &[u8], folders: &[&str]) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = unluminous_sheet::zip::read(bytes)
        .unwrap()
        .into_iter()
        .filter(|e| folders.iter().any(|f| e.name.starts_with(f)))
        .map(|e| (e.name, e.data))
        .collect();
    out.sort();
    out
}

#[test]
fn a_workbook_written_back_with_no_edit_keeps_every_chart_part_byte_for_byte() {
    for name in ["14-charts.xlsx", "20-calcprops.xlsx"] {
        let original = fixture(name);
        let book = unluminous_sheet::xlsx::read(&original).unwrap();
        let again = unluminous_sheet::xlsx::write(&book).unwrap();
        let folders = ["xl/charts/", "xl/chartsheets/", "xl/drawings/"];
        assert_eq!(parts_under(&original, &folders), parts_under(&again, &folders), "{name}");
    }
}

/// All the checks that need Excel, in one test so that Excel is started once for them all. Opening
/// `14-charts.xlsx` takes Excel nearly a minute, so it is opened once.
#[test]
fn excel_agrees_with_what_is_read_written_and_made() {
    if !excel_available() {
        return;
    }
    let original = scratch("14-original.xlsx");
    std::fs::write(&original, fixture("14-charts.xlsx")).unwrap();
    let book = unluminous_sheet::xlsx::read(&fixture("14-charts.xlsx")).unwrap();
    let mut dirty = book.clone();
    for sheet in &mut dirty.sheets {
        for chart in &mut sheet.charts {
            chart.spec.dirty = true;
        }
    }
    let regenerated = save(&dirty, "14-regenerated.xlsx");
    let made = every_type_workbook();
    let all_types = save(&made.book, "all-types.xlsx");
    let seen = inspect_many(&[original, regenerated, all_types]);
    let [a, c, d] = &seen[..] else { panic!("three answers expected") };
    assert_eq!(c["repaired"], false, "{}", c["problem"]);
    let (left, right) = (a["charts"].as_array().unwrap(), c["charts"].as_array().unwrap());
    assert_eq!(left.len(), right.len());
    for (x, y) in left.iter().zip(right) {
        for key in ["type", "title", "legend", "table"] {
            assert_eq!(x[key], y[key], "{} {key}", x["name"]);
        }
        assert_eq!(x["series"], y["series"], "{}", x["name"]);
        assert_eq!(x["axes"], y["axes"], "{}", x["name"]);
    }
    assert_eq!(d["repaired"], false, "{}", d["problem"]);
    let charts = d["charts"].as_array().unwrap();
    assert_eq!(charts.len(), made.book.sheets[0].charts.len());
    for (chart, made) in charts.iter().zip(&made.book.sheets[0].charts) {
        assert_eq!(chart["type"], made.chart_type().excel_number(), "{:?}", made.chart_type());
    }
}
