//! The two worksheet elements Formula Auditing keeps: `ignoredErrors`, the errors Excel is told not to flag,
//! and `cellWatches`, the cells in the Watch Window.

use roxmltree::Node;

use crate::coord::{CellRef, Range};
use crate::model::{CellWatch, IgnoredError};

/// The attributes of an `ignoredError` element that name a rule. Each one set to 1 switches that rule off
/// for the cells in `sqref`.
pub const IGNORED_RULES: &[&str] = &[
    "evalError",
    "twoDigitTextYear",
    "numberStoredAsText",
    "formula",
    "formulaRange",
    "unlockedFormula",
    "emptyCellReference",
    "listDataValidation",
    "calculatedColumn",
];

/// The `ignoredErrors` element's rows.
pub fn read_ignored_errors(section: Node) -> Vec<IgnoredError> {
    section
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "ignoredError")
        .map(|n| IgnoredError {
            ranges: n
                .attribute("sqref")
                .unwrap_or("")
                .split_whitespace()
                .filter_map(Range::parse)
                .collect(),
            rules: IGNORED_RULES
                .iter()
                .filter(|rule| matches!(n.attribute(**rule), Some("1") | Some("true")))
                .map(|rule| (*rule).to_owned())
                .collect(),
        })
        .collect()
}

/// The `ignoredErrors` element for some rows, or nothing when there are none.
pub fn ignored_errors_xml(rows: &[IgnoredError]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::from("<ignoredErrors>");
    for row in rows {
        let sqref: Vec<String> = row.ranges.iter().map(|r| r.a1()).collect();
        out.push_str(&format!("<ignoredError sqref=\"{}\"", sqref.join(" ")));
        for rule in IGNORED_RULES.iter().filter(|rule| row.rules.iter().any(|r| r == **rule)) {
            out.push_str(&format!(" {rule}=\"1\""));
        }
        out.push_str("/>");
    }
    out.push_str("</ignoredErrors>");
    out
}

/// The cells of the `cellWatches` element.
pub fn read_watches(section: Node) -> Vec<CellWatch> {
    section
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "cellWatch")
        .filter_map(|n| n.attribute("r").and_then(CellRef::parse))
        .map(|at| CellWatch { at })
        .collect()
}

/// The `cellWatches` element for some cells, or nothing when there are none.
pub fn watches_xml(watches: &[CellWatch]) -> String {
    if watches.is_empty() {
        return String::new();
    }
    let mut out = String::from("<cellWatches>");
    for watch in watches {
        out.push_str(&format!("<cellWatch r=\"{}\"/>", watch.at.a1()));
    }
    out.push_str("</cellWatches>");
    out
}
