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
    assert!(
        std::env::var("UNLUMINOUS_REQUIRE_EXCEL").is_err(),
        "Excel is required and is not installed"
    );
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
    let folder =
        std::env::temp_dir().join(format!("unluminous-sheet-charts-{}", std::process::id()));
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
    assert_eq!(
        charts.len(),
        19,
        "13 chart parts and 6 chartEx parts, the 14th chart part is on a chart sheet"
    );
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
                assert_eq!(
                    (&s.name, &s.categories, &s.values),
                    (&t.name, &t.categories, &t.values)
                );
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

/// A workbook with one chart of each newer type Excel's Insert Chart offers, other than the map.
fn extended_workbook() -> Session {
    let mut s = Session::new(Workbook::new(), 46318.5);
    for (r, (name, v)) in [
        ("a", 5.0),
        ("b", 9.0),
        ("c", 3.0),
        ("a", 7.0),
        ("b", 4.0),
        ("c", 8.0),
        ("a", 6.0),
        ("b", 2.0),
    ]
    .iter()
    .enumerate()
    {
        s.set_input(0, CellRef::new(r as u32 + 1, 0), name).unwrap();
        s.set_input(0, CellRef::new(r as u32 + 1, 1), &format!("{v}")).unwrap();
    }
    s.set_input(0, CellRef::new(0, 0), "Group").unwrap();
    s.set_input(0, CellRef::new(0, 1), "Value").unwrap();
    let range = Range::new(CellRef::new(0, 0), CellRef::new(8, 1));
    for ty in ChartType::ALL.iter().filter(|t| t.is_extended() && **t != ChartType::Map) {
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
    let extended = extended_workbook();
    let extended_path = save(&extended.book, "extended-types.xlsx");
    let seen = inspect_many(&[original, regenerated, all_types, extended_path]);
    let [a, c, d, e] = &seen[..] else { panic!("four answers expected") };
    assert_eq!(e["repaired"], false, "{}", e["problem"]);
    let types: Vec<i64> =
        e["charts"].as_array().unwrap().iter().map(|c| c["type"].as_i64().unwrap()).collect();
    let wanted: Vec<i64> = extended.book.sheets[0]
        .charts
        .iter()
        .map(|c| i64::from(c.chart_type().excel_number()))
        .collect();
    assert_eq!(types, wanted, "Excel reads the newer chart types made here");
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
    let excel_types: Vec<i64> = charts.iter().map(|c| c["type"].as_i64().unwrap()).collect();
    let our_types: Vec<i64> = made.book.sheets[0]
        .charts
        .iter()
        .map(|c| i64::from(c.chart_type().excel_number()))
        .collect();
    let names: Vec<String> =
        made.book.sheets[0].charts.iter().map(|c| format!("{:?}", c.chart_type())).collect();
    let differing: Vec<String> = (0..our_types.len())
        .filter(|i| excel_types[*i] != our_types[*i])
        .map(|i| format!("{} ours {} Excel {}", names[i], our_types[i], excel_types[i]))
        .collect();
    assert!(
        differing.is_empty(),
        "types differ: {differing:?}\nexcel {excel_types:?}\nours  {our_types:?}\nnames {names:?}"
    );
}

// ---------------------------------------------------------------------------------------------------
// Making and changing charts, with no Excel.

use unluminous_sheet::edit::charts::SeriesSource;

fn first_chart(s: &Session) -> &unluminous_sheet::model::Chart {
    &s.book.sheets[0].charts[0]
}

fn all() -> Range {
    Range::new(CellRef::new(0, 0), CellRef::new(5, 2))
}

fn read_back(s: &Session) -> Workbook {
    unluminous_sheet::xlsx::read(&unluminous_sheet::xlsx::write(&s.book).unwrap()).unwrap()
}

#[test]
fn a_selection_is_laid_out_as_excel_lays_out_a_chart() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    let c = first_chart(&s);
    assert_eq!(c.series.len(), 2, "two columns of numbers under text headings");
    assert_eq!(c.series[0].name.as_deref(), Some("Sheet1!$B$1"));
    assert_eq!(c.series[0].categories.as_deref(), Some("Sheet1!$A$2:$A$6"));
    assert_eq!(c.series[1].values, "Sheet1!$C$2:$C$6");
    // More columns than rows lay the series out in rows.
    let mut wide = Session::new(Workbook::new(), 46318.5);
    for (c, t) in ["", "a", "b", "c", "d"].iter().enumerate() {
        wide.set_input(0, CellRef::new(0, c as u32), t).unwrap();
    }
    for (r, name) in ["x", "y"].iter().enumerate() {
        wide.set_input(0, CellRef::new(r as u32 + 1, 0), name).unwrap();
        for c in 1..=4u32 {
            wide.set_input(0, CellRef::new(r as u32 + 1, c), &format!("{}", c * 2)).unwrap();
        }
    }
    wide.insert_chart(0, Range::new(CellRef::new(0, 0), CellRef::new(2, 4)), ChartType::Line, None)
        .unwrap();
    let c = first_chart(&wide);
    assert_eq!(c.series[0].values, "Sheet1!$B$2:$E$2");
    assert_eq!(c.series[0].name.as_deref(), Some("Sheet1!$A$2"));
    assert_eq!(c.series[0].categories.as_deref(), Some("Sheet1!$B$1:$E$1"));
}

#[test]
fn every_chart_method_is_one_undo_step_and_undo_restores_the_chart() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    let before = first_chart(&s).clone();
    type Step = Box<dyn Fn(&mut Session) -> Result<(), String>>;
    let steps: Vec<Step> = vec![
        Box::new(|s| s.change_chart_type(0, 0, ChartType::LineMarkers)),
        Box::new(|s| s.change_series_type(0, 0, 1, ChartType::Line)),
        Box::new(|s| s.set_series_secondary(0, 0, 1, true)),
        Box::new(|s| s.set_chart_title(0, 0, Some("Sales".into()), None)),
        Box::new(|s| s.set_legend(0, 0, Some(LegendPosition::Top), false)),
        Box::new(|s| s.set_data_table(0, 0, Some(DataTable::default()))),
        Box::new(|s| {
            s.set_data_labels(
                0,
                0,
                None,
                true,
                LabelOptions { show_value: Some(true), ..Default::default() },
            )
        }),
        Box::new(|s| s.add_trendline(0, 0, 0, Trendline::default()).map(|_| ())),
        Box::new(|s| s.set_error_bars(0, 0, 0, ErrorBars::default())),
        Box::new(|s| s.apply_quick_layout(0, 0, 3)),
        Box::new(|s| s.set_chart_style(0, 0, 10)),
        Box::new(|s| s.filter_series(0, 0, 1, true)),
        Box::new(|s| s.switch_row_column(0, 0)),
        Box::new(|s| s.set_hidden_and_empty_cells(0, 0, true, Blanks::Zero)),
        Box::new(|s| {
            s.set_axis_options(
                0,
                0,
                AxisSelector::Value,
                AxisOptions { minimum: Some(Some(2.0)), ..Default::default() },
            )
        }),
    ];
    for (i, step) in steps.iter().enumerate() {
        let mut t = s.clone();
        step(&mut t).unwrap_or_else(|e| panic!("step {i}: {e}"));
        assert!(t.book.sheets[0].charts[0] != before, "step {i} changed the chart");
        t.undo();
        assert_eq!(t.book.sheets[0].charts[0], before, "step {i} is undone by one undo");
    }
}

#[test]
fn a_secondary_axis_series_makes_a_combination_chart_that_reads_back() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    s.change_series_type(0, 0, 1, ChartType::LineMarkers).unwrap();
    s.set_series_secondary(0, 0, 1, true).unwrap();
    let c = first_chart(&s);
    assert_eq!(c.chart_type(), ChartType::Combo);
    assert_eq!(c.spec.groups.len(), 2);
    assert!(c.spec.is_secondary(1) && !c.spec.is_secondary(0));
    assert_eq!(c.spec.axes.len(), 4);
    let back = read_back(&s);
    let again = &back.sheets[0].charts[0];
    assert_eq!(again.spec.groups.len(), 2);
    assert!(again.spec.is_secondary(1));
    assert_eq!(again.series[1].group, 1);
    // Taking it back to the primary axis removes the second pair of axes.
    s.set_series_secondary(0, 0, 1, false).unwrap();
    assert_eq!(first_chart(&s).spec.axes.len(), 2);
}

#[test]
fn select_data_adds_edits_removes_and_refuses() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    let n = s
        .add_series(
            0,
            0,
            SeriesSource {
                name: Some(Some("\"Extra\"".into())),
                values: Some("Sheet1!$B$2:$B$6".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(n, 2);
    s.edit_series(
        0,
        0,
        2,
        SeriesSource { values: Some("Sheet1!$C$2:$C$4".into()), ..Default::default() },
    )
    .unwrap();
    assert_eq!(first_chart(&s).series[2].values, "Sheet1!$C$2:$C$4");
    assert!(s
        .edit_series(
            0,
            0,
            2,
            SeriesSource { values: Some("not a range".into()), ..Default::default() }
        )
        .is_err());
    s.remove_series(0, 0, 2).unwrap();
    s.remove_series(0, 0, 1).unwrap();
    let error = s.remove_series(0, 0, 0).unwrap_err();
    assert!(error.contains("at least one series"), "{error}");
    assert_eq!(first_chart(&s).series.len(), 1);
}

#[test]
fn switching_rows_and_columns_swaps_the_series_and_the_categories() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    s.switch_row_column(0, 0).unwrap();
    let c = first_chart(&s);
    assert_eq!(c.series.len(), 5);
    assert_eq!(c.series[0].values, "Sheet1!$B$2:$C$2");
    assert_eq!(c.series[0].name.as_deref(), Some("Sheet1!$A$2"));
    assert_eq!(c.series[0].categories.as_deref(), Some("Sheet1!$B$1:$C$1"));
    s.switch_row_column(0, 0).unwrap();
    assert_eq!(first_chart(&s).series.len(), 2);
}

#[test]
fn a_chart_moves_and_is_written_with_its_new_anchor() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    s.move_chart(0, 0, (CellRef::new(10, 1), 1000, 2000), (CellRef::new(25, 9), 0, 0)).unwrap();
    let back = read_back(&s);
    let c = &back.sheets[0].charts[0];
    assert_eq!((c.from.0, c.from.1, c.from.2), (CellRef::new(10, 1), 1000, 2000));
    assert_eq!(c.to.0, CellRef::new(25, 9));
    // A chart read from a file that is moved leaves the others in its drawing where they were.
    let book = unluminous_sheet::xlsx::read(&fixture("14-charts.xlsx")).unwrap();
    let sheet = book.sheet_index("Charts").unwrap();
    let mut t = Session::new(book, 46318.5);
    t.move_chart(sheet, 0, (CellRef::new(80, 2), 0, 0), (CellRef::new(95, 8), 0, 0)).unwrap();
    let again = read_back(&t);
    assert_eq!(again.sheets[sheet].charts.len(), 19);
    assert_eq!(again.sheets[sheet].charts[0].from.0, CellRef::new(80, 2));
    assert_eq!(again.sheets[sheet].charts[1].from.0, t.book.sheets[sheet].charts[1].from.0);
}

#[test]
fn deleting_a_chart_from_a_file_takes_its_parts_with_it() {
    let book = unluminous_sheet::xlsx::read(&fixture("14-charts.xlsx")).unwrap();
    let sheet = book.sheet_index("Charts").unwrap();
    let mut t = Session::new(book, 46318.5);
    t.delete_chart(sheet, 0).unwrap();
    let bytes = unluminous_sheet::xlsx::write(&t.book).unwrap();
    let names: Vec<String> =
        unluminous_sheet::zip::read(&bytes).unwrap().into_iter().map(|e| e.name).collect();
    assert!(!names.contains(&"xl/charts/chart2.xml".to_owned()), "the chart part goes");
    let rels = unluminous_sheet::zip::read(&fixture("14-charts.xlsx")).unwrap();
    let rels = rels.iter().find(|e| e.name == "xl/charts/_rels/chart2.xml.rels").unwrap();
    let rels = String::from_utf8_lossy(&rels.data).into_owned();
    assert!(rels.contains("style1.xml") && rels.contains("colors1.xml"), "{rels}");
    assert!(!names.contains(&"xl/charts/style1.xml".to_owned()), "and its style part");
    assert!(!names.contains(&"xl/charts/colors1.xml".to_owned()), "and its colour part");
    assert!(!names.contains(&"xl/charts/_rels/chart2.xml.rels".to_owned()));
    assert!(names.contains(&"xl/charts/chart3.xml".to_owned()));
    assert_eq!(unluminous_sheet::xlsx::read(&bytes).unwrap().sheets[sheet].charts.len(), 18);
}

#[test]
fn a_chart_moves_to_a_chart_sheet_and_back_out_to_a_sheet() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::Pie, Some("Share".into())).unwrap();
    let at = s.move_chart_to_new_sheet(0, 0, Some("Pie sheet")).unwrap();
    assert!(s.book.sheets[0].charts.is_empty());
    let back = read_back(&s);
    let sheet = &back.sheets[at];
    assert!(sheet.kept_part.is_some(), "a chart sheet is not a worksheet");
    assert_eq!(sheet.charts.len(), 1);
    assert_eq!(sheet.charts[0].title.as_deref(), Some("Share"));
    assert!(
        sheet.charts[0].series[0].values.starts_with("Sheet1!"),
        "the data is still on the first sheet"
    );
    s.move_chart_to_sheet(at, 0, 0).unwrap();
    assert_eq!(s.book.sheets[0].charts.len(), 1);
}

#[test]
fn refused_changes_leave_the_chart_alone() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::Pie, None).unwrap();
    let before = first_chart(&s).clone();
    assert!(s.add_trendline(0, 0, 0, Trendline::default()).is_err(), "no trendlines on a pie");
    assert!(s.apply_quick_layout(0, 0, 99).is_err());
    assert!(s.set_axis_options(0, 0, AxisSelector::Value, AxisOptions::default()).is_err());
    assert!(s.change_chart_type(0, 0, ChartType::Combo).is_err());
    assert!(s.move_chart(0, 5, (CellRef::new(0, 0), 0, 0), (CellRef::new(1, 1), 0, 0)).is_err());
    assert_eq!(first_chart(&s), &before);
    let mut line = data_session();
    line.insert_chart(0, all(), ChartType::Line, None).unwrap();
    let bad =
        AxisOptions { minimum: Some(Some(10.0)), maximum: Some(Some(5.0)), ..Default::default() };
    assert!(line.set_axis_options(0, 0, AxisSelector::Value, bad).is_err());
    let bad = Trendline { kind: TrendKind::Polynomial(9), ..Default::default() };
    assert!(line.add_trendline(0, 0, 0, bad).is_err());
}

#[test]
fn recommended_charts_follow_the_shape_of_the_data() {
    let s = data_session();
    let one = Range::new(CellRef::new(0, 1), CellRef::new(5, 1));
    let rec = s.recommended_charts(0, one).unwrap();
    assert_eq!(rec.len(), 5);
    assert_eq!(rec[0], ChartType::ColumnClustered);
    assert!(rec.contains(&ChartType::Pie));
    let two = s.recommended_charts(0, all()).unwrap();
    assert_eq!(two.len(), 5);
    assert!(!two.contains(&ChartType::Pie));
}

// ---------------------------------------------------------------------------------------------------
// The same changes made by Excel and by Session methods, compared.

use unluminous_sheet::edit::charts::*;

/// The scenario names, which are the ones in `tools/excel-oracle/charts-scenarios.ps1`.
const SCENARIOS: &[&str] = &[
    "secondary",
    "titles",
    "labels",
    "trend",
    "errbars",
    "axis",
    "log",
    "filter",
    "layout",
    "style",
    "switch",
    "blanks",
    "bargroup",
    "pie",
];

fn scenario(name: &str) -> Session {
    let mut s = data_session();
    let a1c6 = Range::new(CellRef::new(0, 0), CellRef::new(5, 2));
    s.insert_chart(0, a1c6, ChartType::ColumnClustered, None).unwrap();
    let axis = |s: &mut Session, which, o: AxisOptions| s.set_axis_options(0, 0, which, o).unwrap();
    match name {
        "secondary" => {
            s.change_series_type(0, 0, 1, ChartType::LineMarkers).unwrap();
            s.set_series_secondary(0, 0, 1, true).unwrap();
        }
        "titles" => {
            s.set_chart_title(0, 0, Some("Sales".into()), None).unwrap();
            axis(
                &mut s,
                AxisSelector::Category,
                AxisOptions { title: Some(Some("Month".into())), ..Default::default() },
            );
            axis(
                &mut s,
                AxisSelector::Value,
                AxisOptions { title: Some(Some("Units".into())), ..Default::default() },
            );
            s.set_legend(0, 0, Some(LegendPosition::Top), false).unwrap();
            s.set_data_table(0, 0, Some(DataTable::default())).unwrap();
        }
        "labels" => s
            .set_data_labels(
                0,
                0,
                Some(0),
                true,
                LabelOptions {
                    show_value: Some(true),
                    show_category: Some(true),
                    position: Some(Some(LabelPosition::OutsideEnd)),
                    ..Default::default()
                },
            )
            .unwrap(),
        "trend" => {
            let linear = Trendline {
                show_equation: true,
                show_r_squared: true,
                forward: 1.0,
                backward: 0.5,
                ..Default::default()
            };
            s.add_trendline(0, 0, 0, linear).unwrap();
            s.add_trendline(
                0,
                0,
                1,
                Trendline { kind: TrendKind::Polynomial(3), ..Default::default() },
            )
            .unwrap();
            s.add_trendline(
                0,
                0,
                1,
                Trendline { kind: TrendKind::MovingAverage(3), ..Default::default() },
            )
            .unwrap();
        }
        "errbars" => {
            s.set_error_bars(0, 0, 0, ErrorBars { value: Some(2.0), ..Default::default() })
                .unwrap();
            let percent = ErrorBars {
                amount: ErrorAmount::Percentage,
                sides: ErrorSides::Minus,
                value: Some(10.0),
                ..Default::default()
            };
            s.set_error_bars(0, 0, 1, percent).unwrap();
        }
        "axis" => {
            let o = AxisOptions {
                minimum: Some(Some(2.0)),
                maximum: Some(Some(40.0)),
                major_unit: Some(Some(5.0)),
                minor_unit: Some(Some(1.0)),
                minor_gridlines: Some(true),
                number_format: Some(Some(("0.0".into(), false))),
                major_tick: Some(TickMark::Outside),
                tick_labels: Some(TickLabels::NextTo),
                ..Default::default()
            };
            axis(&mut s, AxisSelector::Value, o);
            axis(
                &mut s,
                AxisSelector::Category,
                AxisOptions { reverse: Some(true), ..Default::default() },
            );
        }
        "log" => axis(
            &mut s,
            AxisSelector::Value,
            AxisOptions { log_base: Some(Some(10.0)), ..Default::default() },
        ),
        "filter" => s.filter_series(0, 0, 1, true).unwrap(),
        "layout" => s.apply_quick_layout(0, 0, 3).unwrap(),
        "style" => s.set_chart_style(0, 0, 10).unwrap(),
        "switch" => s.switch_row_column(0, 0).unwrap(),
        "blanks" => s.set_hidden_and_empty_cells(0, 0, true, Blanks::Zero).unwrap(),
        "bargroup" => {
            s.set_group_options(
                0,
                0,
                0,
                GroupOptions { gap_width: Some(80), overlap: Some(20), ..Default::default() },
            )
            .unwrap();
            s.set_series_format(
                0,
                0,
                0,
                None,
                SeriesOptions { fill: Some(Some((255, 0, 0))), ..Default::default() },
            )
            .unwrap();
        }
        "pie" => {
            s.change_chart_type(0, 0, ChartType::Pie).unwrap();
            s.set_group_options(
                0,
                0,
                0,
                GroupOptions { first_slice_angle: Some(90), ..Default::default() },
            )
            .unwrap();
        }
        other => panic!("no scenario {other}"),
    }
    s
}

/// The parts of a chart both Excel's file and ours must agree on, as text.
fn summary(chart: &unluminous_sheet::model::Chart) -> String {
    let spec = &chart.spec;
    let mut out = vec![
        format!("type {:?}", chart.chart_type()),
        format!("title {:?} auto {} deleted {}", chart.title, spec.title_auto, spec.title_deleted),
    ];
    out.push(format!("legend {:?}", spec.legend.as_ref().map(|l| l.position)));
    out.push(format!("table {}", spec.data_table.is_some()));
    out.push(format!("visible {} blanks {:?}", spec.visible_only(), spec.blanks_as()));
    for (i, s) in chart.series.iter().enumerate() {
        let f = &s.fmt;
        out.push(format!(
            "series {i} {:?} {:?} {} group {} secondary {} filtered {}",
            s.name,
            s.categories,
            s.values,
            s.group,
            spec.is_secondary(s.group),
            f.filtered
        ));
        let l = f.labels.as_ref().map(|l| {
            let x = &l.fields;
            (x.show_value, x.show_category, x.show_series, x.show_percent, x.position)
        });
        out.push(format!("  labels {l:?}"));
        for t in &f.trendlines {
            out.push(format!(
                "  trend {:?} fwd {} back {} eq {} r2 {}",
                t.kind, t.forward, t.backward, t.show_equation, t.show_r_squared
            ));
        }
        for e in &f.error_bars {
            out.push(format!(
                "  errbars {:?} {:?} {:?} cap {}",
                e.sides, e.amount, e.value, !e.no_cap
            ));
        }
    }
    for g in &spec.groups {
        out.push(format!(
            "group {:?} {:?} gap {:?} overlap {:?} angle {:?}",
            g.kind, g.grouping, g.gap_width, g.overlap, g.first_slice_angle
        ));
    }
    for a in &spec.axes {
        out.push(format!(
            "axis {:?} {:?} deleted {} min {:?} max {:?} major {:?} minor {:?} log {:?} reverse {} title {:?} grid {} {} fmt {:?} ticks {:?} {:?} labels {:?}",
            a.kind,
            a.position,
            a.deleted,
            a.min,
            a.max,
            a.major_unit,
            a.minor_unit,
            a.log_base,
            a.reverse,
            a.title.as_ref().map(|t| t.text.clone()),
            a.major_gridlines.is_some(),
            a.minor_gridlines.is_some(),
            a.num_fmt.as_ref().filter(|f| !f.1).map(|f| f.0.clone()),
            a.major_tick,
            a.minor_tick,
            a.tick_labels
        ));
    }
    out.join("\n")
}

fn excel_fixture(name: &str) -> Option<Workbook> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/charts/excel/excel-{name}.xlsx"));
    std::fs::read(path).ok().map(|b| unluminous_sheet::xlsx::read(&b).unwrap())
}

/// The lines on which two summaries differ, as `excel line` and `ours line`.
fn differences(excel: &str, ours: &str) -> Vec<String> {
    let (a, b): (Vec<&str>, Vec<&str>) = (excel.lines().collect(), ours.lines().collect());
    let mut out = Vec::new();
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or("-"), b.get(i).copied().unwrap_or("-"));
        if x != y {
            out.push(format!("  excel: {x}\n  ours:  {y}"));
        }
    }
    out
}

#[test]
fn what_session_methods_make_reads_as_what_excel_makes_through_com() {
    let mut checked = 0;
    let mut problems = Vec::new();
    for name in SCENARIOS {
        let Some(excel) = excel_fixture(name) else { continue };
        let ours = read_back(&scenario(name));
        let (a, b) = (&excel.sheets[0].charts[0], &ours.sheets[0].charts[0]);
        for d in differences(&summary(a), &summary(b)) {
            problems.push(format!("scenario {name}\n{d}"));
        }
        checked += 1;
    }
    assert!(problems.is_empty(), "{checked} scenarios compared\n{}", problems.join("\n"));
    println!("{checked} scenarios compared with Excel's own files");
}

/// Excel opens what the scenarios make and sees what it sees in its own files.
#[test]
fn excel_sees_the_same_chart_in_a_session_made_file_as_in_its_own() {
    if !excel_available() {
        return;
    }
    let mut paths = Vec::new();
    let mut pairs = Vec::new();
    for name in SCENARIOS {
        let Some(_) = excel_fixture(name) else { continue };
        let theirs = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/charts/excel/excel-{name}.xlsx"));
        let ours = save(&scenario(name).book, &format!("unl-{name}.xlsx"));
        pairs.push((paths.len(), name));
        paths.push(theirs);
        paths.push(ours);
    }
    let seen = inspect_many(&paths);
    for (at, name) in pairs {
        let (theirs, ours) = (&seen[at], &seen[at + 1]);
        assert_eq!(ours["repaired"], false, "{name}: {}", ours["problem"]);
        let (a, b) = (&theirs["charts"][0], &ours["charts"][0]);
        for key in [
            "type",
            "title",
            "legend",
            "table",
            "plotVisibleOnly",
            "blanks",
            "gapWidth",
            "overlap",
            "firstSliceAngle",
            "groups",
        ] {
            assert_eq!(a[key], b[key], "{name} {key}");
        }
        assert_eq!(a["series"], b["series"], "{name} series");
        assert_eq!(
            without_automatic_scale(&a["axes"]),
            without_automatic_scale(&b["axes"]),
            "{name} axes"
        );
    }
}

/// Excel's automatic scale depends on the size of the plot, which differs between the two files, so
/// the numbers it chose are left out where the scale is automatic.
fn without_automatic_scale(axes: &serde_json::Value) -> serde_json::Value {
    let mut axes = axes.clone();
    for key in ["value", "secondaryValue"] {
        if let Some(axis) = axes.get_mut(key).and_then(|a| a.as_object_mut()) {
            for (auto, number) in [
                ("majorAuto", "majorUnit"),
                ("majorAuto", "minorUnit"),
                ("maxAuto", "max"),
                ("minAuto", "min"),
            ] {
                if axis.get(auto) == Some(&serde_json::Value::Bool(true)) {
                    axis.remove(number);
                }
            }
        }
    }
    axes
}

// ---------------------------------------------------------------------------------------------------
// What a drawing is given.

use unluminous_sheet::plot::{plot, ExtendedPlot, PlotSize};

#[test]
fn a_plot_reads_the_cells_now_and_follows_hidden_rows_and_blanks() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnClustered, None).unwrap();
    let p = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(p.series.len(), 2);
    assert_eq!(p.series[0].name, "One");
    assert_eq!(p.series[0].values, vec![Some(3.0), Some(6.0), Some(9.0), Some(12.0), Some(15.0)]);
    assert_eq!(p.series[0].labels, vec!["c1", "c2", "c3", "c4", "c5"]);
    s.hide_rows(0, 2, 3, true).unwrap();
    let p = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(
        p.series[0].values,
        vec![Some(3.0), Some(12.0), Some(15.0)],
        "hidden rows are not plotted"
    );
    s.set_hidden_and_empty_cells(0, 0, true, Blanks::Gap).unwrap();
    let p = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(p.series[0].values.len(), 5, "unless the chart shows hidden cells");
    s.set_input(0, CellRef::new(1, 1), "").unwrap();
    let gap = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(gap.series[0].values[0], None);
    s.set_hidden_and_empty_cells(0, 0, true, Blanks::Zero).unwrap();
    let zero = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(zero.series[0].values[0], Some(0.0));
}

#[test]
fn stacked_series_start_where_the_series_before_them_end() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::ColumnStacked, None).unwrap();
    let p = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    assert_eq!(p.series[1].base[0], 3.0);
    assert_eq!(p.series[1].top[0], Some(6.0));
    let mut percent = data_session();
    percent.insert_chart(0, all(), ChartType::ColumnStacked100, None).unwrap();
    let p = plot(&percent.book, 0, first_chart(&percent), PlotSize::default());
    assert!((p.series[1].top[0].unwrap() - 1.0).abs() < 1e-12);
    let axis = p.axes.iter().find(|a| a.kind == AxisKind::Value).unwrap();
    assert_eq!((axis.scale.unwrap().min, axis.scale.unwrap().max), (0.0, 1.0));
}

#[test]
fn trendlines_and_error_bars_are_worked_out_for_the_drawing() {
    let mut s = data_session();
    s.insert_chart(0, all(), ChartType::Line, None).unwrap();
    s.add_trendline(
        0,
        0,
        0,
        Trendline { show_equation: true, show_r_squared: true, forward: 2.0, ..Default::default() },
    )
    .unwrap();
    s.set_error_bars(0, 0, 0, ErrorBars { value: Some(2.0), ..Default::default() }).unwrap();
    let p = plot(&s.book, 0, first_chart(&s), PlotSize::default());
    let fit = &p.series[0].trendlines[0];
    assert!(
        (fit.coefficients[0] - 3.0).abs() < 1e-9 && fit.coefficients[1].abs() < 1e-9,
        "{:?}",
        fit.coefficients
    );
    assert!((fit.r_squared.unwrap() - 1.0).abs() < 1e-12);
    assert_eq!(fit.name, "Linear (One)");
    assert!(fit.samples.last().unwrap().0 > 5.9, "forecast reaches two periods past the data");
    let bars = &p.series[0].error_bars[0];
    assert_eq!(bars.amounts.plus, vec![2.0; 5]);
    // The error bars widen the axis: 15 + 2 is the top of the data now.
    let axis = p.axes.iter().find(|a| a.kind == AxisKind::Value).unwrap();
    assert!(axis.scale.unwrap().max >= 17.0);
}

#[test]
fn the_newer_charts_get_their_figures() {
    let mut book = Workbook::new();
    let mut s = Session::new(std::mem::take(&mut book), 46318.5);
    for (i, v) in [1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 6.0, 6.0, 9.0, 40.0].iter().enumerate() {
        s.set_input(0, CellRef::new(i as u32, 0), ["a", "b"][i % 2]).unwrap();
        s.set_input(0, CellRef::new(i as u32, 1), &format!("{v}")).unwrap();
    }
    let range = Range::new(CellRef::new(0, 0), CellRef::new(9, 1));
    let mut chart = |ty: ChartType| {
        let mut t = s.clone();
        t.insert_chart(0, range, ty, None).map_err(|e| format!("{ty:?}: {e}"))
    };
    let _ = &mut chart;
    for ty in [
        ChartType::Histogram,
        ChartType::Pareto,
        ChartType::BoxWhisker,
        ChartType::Waterfall,
        ChartType::Funnel,
        ChartType::Treemap,
        ChartType::Sunburst,
    ] {
        let mut t = s.clone();
        t.insert_chart(0, range, ty, None).unwrap_or_else(|e| panic!("{ty:?}: {e}"));
        let chart = &t.book.sheets[0].charts[0];
        assert_eq!(chart.chart_type(), ty);
        let p = plot(&t.book, 0, chart, PlotSize::default());
        match (ty, p.extended.expect("extended figures")) {
            (ChartType::Histogram, ExtendedPlot::Histogram { bins }) => {
                assert_eq!(bins.iter().map(|b| b.count).sum::<usize>(), 10);
            }
            (ChartType::Pareto, ExtendedPlot::Pareto { rows }) => {
                assert_eq!(rows.len(), 2);
                assert!((rows.last().unwrap().2 - 1.0).abs() < 1e-12);
            }
            (ChartType::BoxWhisker, ExtendedPlot::BoxWhisker { boxes }) => {
                assert!(!boxes.is_empty())
            }
            (ChartType::Waterfall, ExtendedPlot::Waterfall { bars }) => assert_eq!(bars.len(), 10),
            (ChartType::Funnel, ExtendedPlot::Funnel { items }) => assert_eq!(items.len(), 10),
            (ChartType::Treemap | ChartType::Sunburst, ExtendedPlot::Hierarchy { leaves }) => {
                assert_eq!(leaves.len(), 10)
            }
            other => panic!("unexpected {other:?}"),
        }
        // And they read back from the file they are written to.
        let back = read_back(&t);
        assert_eq!(back.sheets[0].charts[0].chart_type(), ty);
    }
    assert!(s.clone().insert_chart(0, range, ChartType::Map, None).is_err());
}

// ---------------------------------------------------------------------------------------------------
// A chart written from the model says what the chart Excel wrote said.

/// The tags and text of an XML part, with the caches Excel stores beside every reference left out.
fn tokens(xml: &str) -> Vec<String> {
    let mut stripped = String::new();
    let mut rest = xml;
    while let Some(at) =
        rest.find("<c:strCache>").into_iter().chain(rest.find("<c:numCache>")).min()
    {
        stripped.push_str(&rest[..at]);
        let close =
            if rest[at..].starts_with("<c:strCache>") { "</c:strCache>" } else { "</c:numCache>" };
        let end = rest[at..].find(close).map_or(rest.len(), |e| at + e + close.len());
        rest = &rest[end..];
    }
    stripped.push_str(rest);
    let mut out = Vec::new();
    let mut text = String::new();
    for c in stripped.chars() {
        match c {
            '<' => {
                if !text.trim().is_empty() {
                    out.push(text.trim().to_owned());
                }
                text.clear();
                text.push('<');
            }
            '>' => {
                text.push('>');
                out.push(std::mem::take(&mut text));
            }
            c => text.push(c),
        }
    }
    out.into_iter().filter(|t| !t.starts_with("<?xml")).collect()
}

#[test]
fn every_classic_chart_written_from_the_model_has_the_tags_of_the_one_excel_wrote() {
    let original = fixture("14-charts.xlsx");
    let mut book = unluminous_sheet::xlsx::read(&original).unwrap();
    for sheet in &mut book.sheets {
        for chart in &mut sheet.charts {
            chart.spec.dirty = true;
        }
    }
    let again = unluminous_sheet::xlsx::write(&book).unwrap();
    let folders = ["xl/charts/chart"];
    let before = parts_under(&original, &folders);
    let after = parts_under(&again, &folders);
    let mut compared = 0;
    for ((name, a), (_, b)) in before.iter().zip(&after) {
        if name.contains("chartEx") || !name.ends_with(".xml") || name.contains("_rels") {
            continue;
        }
        let (ta, mut tb) =
            (tokens(&String::from_utf8_lossy(a)), tokens(&String::from_utf8_lossy(b)));
        if ta.len() + 1 == tb.len() {
            // Excel leaves out `crosses` on an axis that crosses at the automatic place; it is written.
            let at = tb
                .iter()
                .rposition(|t| t == "<c:crosses val=\"autoZero\"/>")
                .expect("one tag more");
            tb.remove(at);
        }
        assert_eq!(ta, tb, "{name}");
        compared += 1;
    }
    assert_eq!(compared, 14);
}
