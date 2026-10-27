//! What the window's status bar says while a workbook's tab is showing (`task-2382`): the kind of file, the
//! cell mode, the figures for the selection that the settings choose, and the zoom, as Excel's own status bar
//! shows them. Pressing a figure copies its number and pressing the zoom sets it back to 100%, through the
//! `plugins run spreadsheet` words, so a press and an agent's command are the same call.

use crate::services::plugin_ui::{StatusItem, TabStatus};

use super::view::SelectMode;
use super::Book;

/// The status of one workbook.
pub fn of(book: &Book) -> TabStatus {
    let ext = book.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let kind = match ext.as_str() {
        "csv" => "CSV file",
        "tsv" | "txt" | "prn" => "Text file",
        _ => "Workbook",
    };
    let mut items = vec![StatusItem {
        text: book.view.mode().to_owned(),
        name: "Status: mode".to_owned(),
        press: None,
    }];
    let selecting = match book.view.selecting {
        SelectMode::Extend => Some("Extend Selection"),
        SelectMode::Add => Some("Add to Selection"),
        SelectMode::Normal if book.view.end_mode => Some("End Mode"),
        SelectMode::Normal => None,
    };
    if let Some(selecting) = selecting {
        items.push(StatusItem {
            text: selecting.to_owned(),
            name: "Status: selection mode".to_owned(),
            press: None,
        });
    }
    for (name, text) in figures(book) {
        items.push(StatusItem {
            text,
            name: format!("Status: {name}"),
            press: Some(vec!["copy-figure".to_owned(), name.to_owned()]),
        });
    }
    items.push(StatusItem {
        text: format!("{}%", (book.view.zoom * 100.0).round()),
        name: "Status: zoom".to_owned(),
        press: Some(vec!["zoom".to_owned(), "100".to_owned()]),
    });
    TabStatus { kind: kind.to_owned(), items }
}

/// For a selection of more than one cell, the figures the settings choose, by their setting's name, and as
/// they are shown: `("sum", "Sum 9.75")`.
pub fn figures(book: &Book) -> Vec<(&'static str, String)> {
    let ranges = book.view.ranges();
    if ranges.iter().map(|r| r.len()).sum::<u64>() <= 1 {
        return Vec::new();
    }
    let summary = book.session.summary(book.view.sheet, &ranges);
    let n = |v: f64| unluminous_sheet::numfmt::general(v, 11);
    let mut out = Vec::new();
    for name in &book.view.aggregates {
        let figure = match name.as_str() {
            "average" => summary.average().map(|a| ("average", format!("Average {}", n(a)))),
            "count" => (summary.count > 0).then(|| ("count", format!("Count {}", summary.count))),
            "numbers" => (summary.numbers > 0)
                .then(|| ("numbers", format!("Numerical Count {}", summary.numbers))),
            "min" => summary.min.map(|m| ("min", format!("Min {}", n(m)))),
            "max" => summary.max.map(|m| ("max", format!("Max {}", n(m)))),
            "sum" => (summary.numbers > 0).then(|| ("sum", format!("Sum {}", n(summary.sum)))),
            _ => None,
        };
        out.extend(figure);
    }
    out
}

/// The number one figure comes to, as copying it puts it on the clipboard.
pub fn figure_value(book: &Book, name: &str) -> Option<f64> {
    let ranges = book.view.ranges();
    let summary = book.session.summary(book.view.sheet, &ranges);
    match name {
        "average" => summary.average(),
        "count" => Some(summary.count as f64),
        "numbers" => Some(summary.numbers as f64),
        "min" => summary.min,
        "max" => summary.max,
        "sum" => (summary.numbers > 0).then_some(summary.sum),
        _ => None,
    }
}
