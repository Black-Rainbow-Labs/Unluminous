//! The numbers a sparkline is drawn from, against Excel's own drawing of the same data.
//!
//! `fixtures/objects/column-sparkline-bars.txt` is Excel's drawing of five column sparklines in 60 point
//! rows, read off its pixels by `tools/excel-oracle/sparkline-pixels.ps1`: for each row, the top and bottom
//! pixel row of the bar at each of four values. The plot is pixel rows 5 to 76.

use std::path::Path;

use unluminous_sheet::coord::{CellRef, Range};
use unluminous_sheet::edit::Session;
use unluminous_sheet::model::Workbook;
use unluminous_sheet::objects::sparkline::{sparkline_data, SparklineKind};

#[test]
fn column_sparkline_bars_are_where_excel_drew_them() {
    let sets: [[f64; 4]; 5] =
        [[5.0, 7.0, 9.0, 6.0], [1.0, -2.0, 3.0, -4.0], [2.0, 2.0, 2.0, 2.0], [-3.0, -5.0, -4.0, -9.0], [0.0, 0.0, 4.0, 0.0]];
    let mut s = Session::new(Workbook::new(), 46318.5);
    for (r, set) in sets.iter().enumerate() {
        for (c, v) in set.iter().enumerate() {
            s.set_value(0, CellRef::new(r as u32, c as u32), unluminous_sheet::Value::Number(*v)).unwrap();
        }
    }
    let g = s
        .insert_sparklines(0, SparklineKind::Column, 0, Range::parse("A1:D5").unwrap(), Range::parse("F1:F5").unwrap())
        .unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/objects/column-sparkline-bars.txt");
    let expected = std::fs::read_to_string(path).unwrap();
    for (row, line) in expected.lines().enumerate() {
        let data = sparkline_data(&s.book, 0, g, row).unwrap();
        for (i, seen) in line.split(' ').skip(1).enumerate() {
            let ours = data.bar_span(i);
            if seen == "none" {
                assert!(ours.is_none(), "row {row} bar {i}");
                continue;
            }
            let (top, bottom) = seen.split_once('-').unwrap();
            let (top, bottom): (f64, f64) = (top.parse().unwrap(), bottom.parse().unwrap());
            let (a, b) = ours.unwrap_or_else(|| panic!("row {row} bar {i} has no bar"));
            let our_bottom = 5.0 + b * 72.0 - 1.0;
            let our_top = (5.0 + a * 72.0).min(our_bottom);
            assert!(
                (our_top - top).abs() <= 1.5 && (our_bottom - bottom).abs() <= 1.5,
                "row {row} bar {i}: Excel {top}-{bottom}, here {our_top:.1}-{our_bottom:.1}"
            );
        }
    }
}
