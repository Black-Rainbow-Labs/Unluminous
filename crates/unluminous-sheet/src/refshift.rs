//! Moving the cell references inside everything the model keeps as XML rather than understands.
//!
//! A structural edit (rows, columns or cells inserted or deleted, a sheet renamed or deleted) moves every
//! cell and rewrites every formula the model holds. The parts and elements the model keeps as text hold
//! references too: a sparkline's source range, a chart's series formula, the anchor that places a picture,
//! a pivot table's location, an x14 data bar's `sqref`, a threaded comment's cell, a manual page break.
//! [`REGISTRY`] names each of them: the element, the attribute or text that holds the reference, and how
//! the reference is written. [`shift_kept`] runs every row over every kept element and part, so a kept item
//! moves with the cells it belongs to, which is what Excel does.
//!
//! The registry is data, so a newly kept item is one row. `tests::the_registry_covers_every_reference_in_the_schema`
//! fails on an element of the worksheet, drawing, chart or pivot schemas that holds a reference and is
//! neither modelled nor in the registry.

use crate::coord::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::formula::{self, Axis, Expr};
use crate::model::Workbook;

/// One change that moves cells or sheets.
#[derive(Debug, Clone)]
pub enum StructuralEdit {
    /// Rows or columns inserted (`count` positive) or deleted (negative) at `at` on the sheet named
    /// `sheet`. `band` limits the move to some columns (rows) when cells rather than lines move.
    Lines { sheet: String, axis: Axis, at: u32, count: i64, band: Option<(u32, u32)> },
    /// A sheet renamed.
    Rename { from: String, to: String },
    /// A sheet deleted: references to it become `#REF!`.
    DeleteSheet { name: String },
}

/// How a reference is written where the registry finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    /// A list of ranges on the owning sheet, separated by spaces: `A1:B2 D4`.
    Sqref,
    /// One cell or range on the owning sheet.
    Ref,
    /// Formula text in A1 style; a reference without a sheet is on the owning sheet.
    Formula,
    /// A sheet's name.
    SheetName,
    /// A row index counting from 0, as a drawing anchor writes it.
    Row0,
    /// A column index counting from 0.
    Col0,
    /// A manual page break: the first row (or column) after the break, counting from 0.
    Break,
    /// The pivot table a chart shows, written `[book.xlsx]Sheet!PivotTable`; only the sheet follows a rename.
    PivotSource,
}

/// Where the reference is in an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Attr(&'static str),
    Text,
}

/// One registry row: the local name of the element, where in it the reference is, and its syntax.
/// `parent` narrows a name that means different things in different places (a drawing anchor's `row`).
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub element: &'static str,
    pub parent: Option<&'static str>,
    pub target: Target,
    pub syntax: Syntax,
}

const fn row(element: &'static str, target: Target, syntax: Syntax) -> Row {
    Row { element, parent: None, target, syntax }
}

const fn under(parent: &'static str, element: &'static str, target: Target, syntax: Syntax) -> Row {
    Row { element, parent: Some(parent), target, syntax }
}

/// Every reference in kept XML that moves with the cells.
pub const REGISTRY: &[Row] = &[
    // Worksheet elements kept as XML.
    row("sortState", Target::Attr("ref"), Syntax::Ref),
    row("sortCondition", Target::Attr("ref"), Syntax::Ref),
    row("protectedRange", Target::Attr("sqref"), Syntax::Sqref),
    row("ignoredError", Target::Attr("sqref"), Syntax::Sqref),
    row("cellWatch", Target::Attr("r"), Syntax::Ref),
    row("dataRef", Target::Attr("ref"), Syntax::Ref),
    row("dataRef", Target::Attr("sheet"), Syntax::SheetName),
    row("scenario", Target::Attr("sqref"), Syntax::Sqref),
    row("inputCells", Target::Attr("r"), Syntax::Ref),
    row("scenarios", Target::Attr("sqref"), Syntax::Sqref),
    row("selection", Target::Attr("activeCell"), Syntax::Ref),
    row("selection", Target::Attr("sqref"), Syntax::Sqref),
    row("pane", Target::Attr("topLeftCell"), Syntax::Ref),
    row("customSheetView", Target::Attr("topLeftCell"), Syntax::Ref),
    row("autoFilter", Target::Attr("ref"), Syntax::Ref),
    under("rowBreaks", "brk", Target::Attr("id"), Syntax::Break),
    under("colBreaks", "brk", Target::Attr("id"), Syntax::Break),
    row("singleXmlCell", Target::Attr("r"), Syntax::Ref),
    row("oleSize", Target::Attr("ref"), Syntax::Ref),
    // The x14 and later extensions: data bars, icon sets, rules with formulas, validations, sparklines.
    row("sqref", Target::Text, Syntax::Sqref),
    row("f", Target::Text, Syntax::Formula),
    row("dataValidation", Target::Attr("sqref"), Syntax::Sqref),
    row("conditionalFormatting", Target::Attr("sqref"), Syntax::Sqref),
    // Drawing anchors of charts, pictures, shapes, form controls and slicers.
    under("from", "row", Target::Text, Syntax::Row0),
    under("from", "col", Target::Text, Syntax::Col0),
    under("to", "row", Target::Text, Syntax::Row0),
    under("to", "col", Target::Text, Syntax::Col0),
    under("pivotSource", "name", Target::Text, Syntax::PivotSource),
    // Threaded comments, kept comments and pivot tables.
    row("threadedComment", Target::Attr("ref"), Syntax::Ref),
    row("comment", Target::Attr("ref"), Syntax::Ref),
    row("location", Target::Attr("ref"), Syntax::Ref),
    row("worksheetSource", Target::Attr("ref"), Syntax::Ref),
    row("worksheetSource", Target::Attr("sheet"), Syntax::SheetName),
    row("rangeSet", Target::Attr("ref"), Syntax::Ref),
    row("rangeSet", Target::Attr("sheet"), Syntax::SheetName),
    row("queryTableRefresh", Target::Attr("ref"), Syntax::Ref),
    // Charts and charts of the newer kinds: series, category and title formulas.
    row("datalabelsRange", Target::Text, Syntax::Formula),
];

/// The names of elements whose reference the model holds itself rather than the registry, and the
/// elements the schema has that carry a reference but are written only by Excel features with no
/// position (named sets, cube formulas). The schema completeness test reads both lists.
pub const MODELLED: &[&str] = &[
    "mergeCell",
    "hyperlink",
    "c",
    "row",
    "col",
    "definedName",
    "dimension",
    "cfRule",
    "formula",
    "formula1",
    "formula2",
    "tablePart",
    "table",
    "tableColumn",
    "calculatedColumnFormula",
    "totalsRowFormula",
    "brk",
];

/// Apply a structural edit to every kept element and part of the workbook.
pub fn shift_kept(book: &mut Workbook, edit: &StructuralEdit) {
    let owners = part_owners(book);
    for si in 0..book.sheets.len() {
        let own = book.sheets[si].name.clone();
        let namespaces = book.sheets[si].raw.namespaces.clone();
        let elements = std::mem::take(&mut book.sheets[si].raw.elements);
        book.sheets[si].raw.elements = elements
            .into_iter()
            .map(|(name, xml)| {
                let shifted = shift_fragment(&xml, &namespaces, &own, edit);
                (name, shifted)
            })
            .collect();
        shift_modelled_text(book, si, edit);
    }
    for (path, owner) in owners {
        let Some(bytes) = book.kept.get(&path) else { continue };
        let Ok(text) = std::str::from_utf8(bytes) else { continue };
        let shifted = shift_document(text, &owner, edit);
        if shifted != text {
            book.kept.set(&path, shifted.into_bytes());
        }
    }
    for table in &mut book.tables {
        let own = book.sheets.get(table.sheet).map(|s| s.name.clone()).unwrap_or_default();
        table.xml = shift_document(&table.xml, &own, edit);
    }
}

/// The text formulas of conditional formats and validations, which Excel stores as if written in the first
/// cell of their range and which move as cell formulas do.
fn shift_modelled_text(book: &mut Workbook, sheet: usize, edit: &StructuralEdit) {
    let own = book.sheets[sheet].name.clone();
    let s = &mut book.sheets[sheet];
    for cf in &mut s.conditional {
        for rule in &mut cf.rules {
            for f in &mut rule.formulas {
                *f = shift_formula_text(f, &own, edit);
            }
        }
    }
    for v in &mut s.validations {
        for f in [&mut v.formula1, &mut v.formula2].into_iter().flatten() {
            *f = shift_formula_text(f, &own, edit);
        }
    }
    for chart in &mut s.charts {
        for series in &mut chart.series {
            for f in [&mut series.name, &mut series.categories].into_iter().flatten() {
                *f = shift_formula_text(f, &own, edit);
            }
            series.values = shift_formula_text(&series.values, &own, edit);
        }
        chart.from.0 = shift_anchor(chart.from.0, &own, edit);
        chart.to.0 = shift_anchor(chart.to.0, &own, edit);
    }
    // A data table whose cells were all deleted goes with them.
    s.data_tables.retain_mut(|t| {
        let Some(range) = shift_own_range(t.range, &own, edit) else { return false };
        t.range = range;
        for c in [&mut t.r1, &mut t.r2].into_iter().flatten() {
            *c = shift_cell_range(Range::cell(*c), &own, edit).first;
        }
        true
    });
    let view = &mut s.view;
    for sel in &mut view.selections {
        sel.ranges = sel.ranges.iter().map(|r| shift_cell_range(*r, &own, edit)).collect();
        sel.active = shift_cell_range(Range::cell(sel.active), &own, edit).first;
    }
    for at in [&mut view.top_left, &mut view.pane_top_left].into_iter().flatten() {
        *at = shift_cell_range(Range::cell(*at), &own, edit).first;
    }
    if let Some(split) = &mut view.split {
        if let Some(at) = &mut split.top_left {
            *at = shift_cell_range(Range::cell(*at), &own, edit).first;
        }
    }
    for picture in &mut s.pictures {
        picture.from.0 = shift_anchor(picture.from.0, &own, edit);
        picture.to.0 = shift_anchor(picture.to.0, &own, edit);
    }
}

/// A range on `own` after the edit, or `None` when it was wholly deleted.
fn shift_own_range(range: Range, own: &str, edit: &StructuralEdit) -> Option<Range> {
    match edit {
        StructuralEdit::Lines { sheet, axis, at, count, band }
            if own.eq_ignore_ascii_case(sheet) =>
        {
            shift_range(range, *axis, *at, *count, *band)
        }
        _ => Some(range),
    }
}

/// A range the view names after the edit; one wholly deleted becomes the first cell after the deletion.
fn shift_cell_range(range: Range, own: &str, edit: &StructuralEdit) -> Range {
    let text = shift_sqref(&range.a1(), own, edit);
    Range::parse(&text).unwrap_or(range)
}

/// The cell a drawing anchor names after the edit, moved as [`shift_index`] moves the row and column the
/// kept drawing part writes.
fn shift_anchor(at: CellRef, own: &str, edit: &StructuralEdit) -> CellRef {
    let row = shift_index(&at.row.to_string(), Syntax::Row0, own, edit);
    let col = shift_index(&at.col.to_string(), Syntax::Col0, own, edit);
    CellRef::new(row.parse().unwrap_or(at.row), col.parse().unwrap_or(at.col))
}

/// Every kept part with the name of the sheet it belongs to: the parts a sheet's relationships reach, and
/// the parts those reach in turn (a drawing's charts). Parts no sheet reaches belong to no sheet.
fn part_owners(book: &Workbook) -> Vec<(String, String)> {
    let mut owners: Vec<(String, String)> = Vec::new();
    for sheet in &book.sheets {
        let mut queue: Vec<String> = sheet
            .raw
            .rels
            .iter()
            .filter(|r| !r.external)
            .map(|r| crate::xlsx::resolve(&sheet.raw.part, &r.target))
            .collect();
        while let Some(part) = queue.pop() {
            if owners.iter().any(|(p, _)| *p == part) {
                continue;
            }
            if let Some(rels) = book.kept.get(&crate::xlsx::rels_path(&part)) {
                let rels = String::from_utf8_lossy(rels);
                for r in crate::xlsx::parse_rels(&rels).iter().filter(|r| !r.external) {
                    queue.push(crate::xlsx::resolve(&part, &r.target));
                }
            }
            owners.push((part, sheet.name.clone()));
        }
    }
    for (path, _) in &book.kept.parts {
        let xml = path.ends_with(".xml") && !path.ends_with(".rels");
        if xml && !owners.iter().any(|(p, _)| p == path) {
            owners.push((path.clone(), String::new()));
        }
    }
    owners
}

/// A fragment of a worksheet (one kept element) after the edit. The fragment is parsed inside a root that
/// declares the worksheet's namespaces, because a kept element uses prefixes declared on the worksheet.
pub fn shift_fragment(
    xml: &str,
    namespaces: &[(String, String)],
    own: &str,
    edit: &StructuralEdit,
) -> String {
    let mut root = String::from("<r");
    for (prefix, uri) in namespaces {
        if prefix.is_empty() {
            root.push_str(&format!(" xmlns=\"{}\"", crate::xlsx::escape(uri)));
        } else {
            root.push_str(&format!(" xmlns:{prefix}=\"{}\"", crate::xlsx::escape(uri)));
        }
    }
    for (prefix, uri) in COMMON_PREFIXES {
        if !namespaces.iter().any(|(p, _)| p == prefix) {
            root.push_str(&format!(" xmlns:{prefix}=\"{uri}\""));
        }
    }
    root.push('>');
    let wrapped = format!("{root}{xml}</r>");
    let shifted = shift_document(&wrapped, own, edit);
    shifted[root.len()..shifted.len() - 4].to_owned()
}

/// Prefixes Excel uses in worksheet extensions, declared for a fragment whose worksheet did not declare them.
const COMMON_PREFIXES: &[(&str, &str)] = &[
    ("x14", "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"),
    ("xm", "http://schemas.microsoft.com/office/excel/2006/main"),
    ("x14ac", "http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac"),
    ("mc", "http://schemas.openxmlformats.org/markup-compatibility/2006"),
    ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships"),
    ("xr", "http://schemas.microsoft.com/office/spreadsheetml/2014/revision"),
    ("x15", "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main"),
    ("xdr", "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"),
];

/// A whole XML document after the edit, every registry row applied. A document that does not parse is
/// returned as it was.
pub fn shift_document(xml: &str, own: &str, edit: &StructuralEdit) -> String {
    let without = remove_deleted_sparklines(xml, own, edit);
    let xml = without.as_str();
    let Ok(doc) = roxmltree::Document::parse(xml) else { return xml.to_owned() };
    let mut changes: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let name = node.tag_name().name();
        for r in REGISTRY.iter().filter(|r| r.element == name) {
            if let Some(parent) = r.parent {
                let p = node.parent_element().map(|p| p.tag_name().name());
                if p != Some(parent) {
                    continue;
                }
            }
            match r.target {
                Target::Attr(attr) => {
                    let Some(a) = node.attributes().find(|a| a.name() == attr) else { continue };
                    let new = shift_value(a.value(), node, r.syntax, own, edit);
                    if new != a.value() {
                        changes.push((a.range_value(), crate::xlsx::escape(&new)));
                    }
                }
                Target::Text => {
                    let Some(text) = node.first_child().filter(|t| t.is_text()) else { continue };
                    let value = text.text().unwrap_or("");
                    let new = anchored_value(value, node, r.syntax, own, edit)
                        .unwrap_or_else(|| shift_value(value, node, r.syntax, own, edit));
                    if new != value {
                        changes.push((text.range(), crate::xlsx::escape(&new)));
                    }
                }
            }
        }
    }
    if changes.is_empty() {
        return xml.to_owned();
    }
    changes.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = xml.to_owned();
    for (range, text) in changes {
        out.replace_range(range, &text);
    }
    out
}

/// Whether the first range in a reference text (`Sheet1!B3:E3`, `H3`) is wholly inside the rows or columns an
/// edit deletes. `own` is the sheet a reference with no sheet name is on.
fn wholly_deleted(text: &str, own: &str, edit: &StructuralEdit) -> bool {
    let StructuralEdit::Lines { sheet, axis, at, count, band } = edit else { return false };
    if *count >= 0 {
        return false;
    }
    let (on, cells) = match text.trim().rsplit_once('!') {
        Some((name, cells)) => (name.trim_matches('\''), cells),
        None => (own, text.trim()),
    };
    if !on.eq_ignore_ascii_case(sheet) {
        return false;
    }
    let Some(range) = Range::parse(&cells.replace('$', "")) else { return false };
    shift_range(range, *axis, *at, *count, *band).is_none()
}

/// A document without the sparklines whose source or cell is wholly deleted by the edit, as Excel drops them.
fn remove_deleted_sparklines(xml: &str, own: &str, edit: &StructuralEdit) -> String {
    if !xml.contains("sparkline>") {
        return xml.to_owned();
    }
    let Ok(doc) = roxmltree::Document::parse(xml) else { return xml.to_owned() };
    let mut cuts: Vec<std::ops::Range<usize>> = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "sparkline") {
        let gone = node.children().filter(|c| c.is_element()).any(|c| {
            matches!(c.tag_name().name(), "f" | "sqref")
                && wholly_deleted(c.text().unwrap_or(""), own, edit)
        });
        if gone {
            cuts.push(node.range());
        }
    }
    let mut out = xml.to_owned();
    for range in cuts.into_iter().rev() {
        out.replace_range(range, "");
    }
    out
}

/// A row or column inside a drawing anchor's `from` or `to`, when the anchor says how it follows its cells.
/// `editAs="absolute"` does not move or resize with the cells; `oneCell` moves with them and keeps its size,
/// so its `to` moves by what its `from` moved. Any other anchor, and any other element, answers `None`.
fn anchored_value(
    value: &str,
    node: roxmltree::Node,
    syntax: Syntax,
    own: &str,
    edit: &StructuralEdit,
) -> Option<String> {
    let corner = node.parent_element()?;
    let corner_name = corner.tag_name().name();
    if !matches!(syntax, Syntax::Row0 | Syntax::Col0) || !matches!(corner_name, "from" | "to") {
        return None;
    }
    let anchor = corner.parent_element()?;
    match anchor.attribute("editAs")? {
        "absolute" => Some(value.to_owned()),
        "oneCell" if corner_name == "to" => {
            let from = anchor
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == "from")?
                .children()
                .find(|n| n.is_element() && n.tag_name().name() == node.tag_name().name())?;
            let old: i64 = from.text()?.trim().parse().ok()?;
            let new: i64 = shift_index(from.text()?.trim(), syntax, own, edit).parse().ok()?;
            let at: i64 = value.trim().parse().ok()?;
            Some((at + new - old).max(0).to_string())
        }
        _ => None,
    }
}

/// One value after the edit, read in its syntax. `node` is the element, for values whose sheet is named by
/// a sibling attribute (a pivot cache's `ref` and `sheet`).
fn shift_value(
    value: &str,
    node: roxmltree::Node,
    syntax: Syntax,
    own: &str,
    edit: &StructuralEdit,
) -> String {
    let own = node.attribute("sheet").unwrap_or(own);
    match syntax {
        Syntax::Sqref => shift_sqref(value, own, edit),
        Syntax::Ref => shift_sqref(value, own, edit),
        Syntax::Formula => shift_formula_text(value, own, edit),
        Syntax::SheetName => match edit {
            StructuralEdit::Rename { from, to } if value.eq_ignore_ascii_case(from) => to.clone(),
            _ => value.to_owned(),
        },
        Syntax::PivotSource => match (edit, value.split_once(']')) {
            (StructuralEdit::Rename { from, to }, Some((book, rest))) => match rest.split_once('!')
            {
                Some((sheet, pivot)) if sheet.eq_ignore_ascii_case(from) => {
                    format!("{book}]{to}!{pivot}")
                }
                _ => value.to_owned(),
            },
            _ => value.to_owned(),
        },
        Syntax::Row0 | Syntax::Col0 | Syntax::Break => shift_index(value, syntax, own, edit),
    }
}

/// A space separated list of ranges on `own` after the edit. A range wholly deleted is dropped; when every
/// range is gone the list is the first cell of the deleted band, as Excel leaves a kept item's place.
fn shift_sqref(value: &str, own: &str, edit: &StructuralEdit) -> String {
    let StructuralEdit::Lines { sheet, axis, at, count, band } = edit else {
        return value.to_owned();
    };
    if !own.eq_ignore_ascii_case(sheet) {
        return value.to_owned();
    }
    let mut out: Vec<String> = Vec::new();
    for part in value.split_whitespace() {
        let absolute = part.contains('$');
        let Some(range) = Range::parse(&part.replace('$', "")) else {
            out.push(part.to_owned());
            continue;
        };
        if let Some(moved) = shift_range(range, *axis, *at, *count, *band) {
            out.push(if absolute { dollar(moved) } else { moved.a1() });
        }
    }
    if out.is_empty() {
        let first = Range::parse(value.split_whitespace().next().unwrap_or("A1").trim_matches('$'))
            .map(|r| r.first)
            .unwrap_or(CellRef::new(0, 0));
        return CellRef::new(first.row.min(*at), first.col).a1();
    }
    out.join(" ")
}

fn dollar(range: Range) -> String {
    let p = |c: CellRef| format!("${}${}", crate::coord::column_name(c.col), c.row + 1);
    if range.first == range.last {
        p(range.first)
    } else {
        format!("{}:{}", p(range.first), p(range.last))
    }
}

/// One range after rows or columns moved, or `None` when it was wholly deleted.
pub fn shift_range(
    range: Range,
    axis: Axis,
    at: u32,
    count: i64,
    band: Option<(u32, u32)>,
) -> Option<Range> {
    if let Some((lo, hi)) = band {
        let (other_lo, other_hi) = match axis {
            Axis::Rows => (range.first.col, range.last.col),
            Axis::Columns => (range.first.row, range.last.row),
        };
        if other_lo < lo || other_hi > hi {
            return Some(range);
        }
    }
    let reference = formula::Reference::from_range(range, None);
    formula::adjust_reference(&reference, axis, at, count).map(|r| r.range())
}

/// An index (a drawing anchor's row or column, or a page break) after the edit. An index inside deleted
/// lines moves to the first line after them, which is where Excel puts an object whose top row is deleted.
fn shift_index(value: &str, syntax: Syntax, own: &str, edit: &StructuralEdit) -> String {
    let StructuralEdit::Lines { sheet, axis, at, count, band: None } = edit else {
        return value.to_owned();
    };
    let along = matches!(
        (syntax, axis),
        (Syntax::Row0, Axis::Rows) | (Syntax::Col0, Axis::Columns) | (Syntax::Break, _)
    );
    if !own.eq_ignore_ascii_case(sheet) || !along {
        return value.to_owned();
    }
    let Ok(index) = value.trim().parse::<i64>() else { return value.to_owned() };
    let at = *at as i64;
    let next = if *count > 0 {
        if index >= at {
            index + count
        } else {
            index
        }
    } else {
        let end = at - count;
        if index >= end {
            index + count
        } else if index >= at {
            at
        } else {
            index
        }
    };
    let limit = match axis {
        Axis::Rows => MAX_ROWS,
        Axis::Columns => MAX_COLS,
    } as i64;
    next.clamp(0, limit - 1).to_string()
}

/// Formula text after the edit, printed for a file. Text that does not parse is left as it was.
pub fn shift_formula_text(text: &str, own: &str, edit: &StructuralEdit) -> String {
    let body = text.strip_prefix('=').unwrap_or(text);
    let Ok(expr) = formula::parser::parse(body) else { return text.to_owned() };
    let shifted = shift_expr(&expr, own, edit);
    if shifted == expr {
        return text.to_owned();
    }
    let printed = formula::print::print(&shifted, true);
    if text.starts_with('=') {
        format!("={printed}")
    } else {
        printed
    }
}

/// A formula tree after the edit.
pub fn shift_expr(expr: &Expr, own: &str, edit: &StructuralEdit) -> Expr {
    match edit {
        StructuralEdit::Lines { sheet, axis, at, count, band: None } => {
            formula::adjusted(expr, sheet, own, *axis, *at, *count)
        }
        StructuralEdit::Lines { sheet, axis, at, count, band: Some(band) } => {
            crate::edit::adjust_band(expr, sheet, own, *axis, *at, *count, *band)
        }
        StructuralEdit::Rename { from, to } => expr.rename_sheet(from, to),
        StructuralEdit::DeleteSheet { name } => expr.map_refs(&mut |r| match &r.sheet {
            Some(n) if n.eq_ignore_ascii_case(name) => None,
            _ => Some(r.clone()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(sheet: &str, at: u32, count: i64) -> StructuralEdit {
        StructuralEdit::Lines { sheet: sheet.to_owned(), axis: Axis::Rows, at, count, band: None }
    }

    #[test]
    fn an_x14_data_bar_moves_with_its_cells() {
        let xml = "<extLst><ext uri=\"{78C0D931-6437-407d-A8EE-F0AAD7539E65}\" xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"><x14:conditionalFormattings><x14:conditionalFormatting xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\"><x14:cfRule type=\"dataBar\" id=\"{1}\"><x14:dataBar><x14:cfvo type=\"num\"><xm:f>I1</xm:f></x14:cfvo></x14:dataBar></x14:cfRule><xm:sqref>F1:F10</xm:sqref></x14:conditionalFormatting></x14:conditionalFormattings></ext></extLst>";
        let out = shift_fragment(xml, &[], "Sheet1", &rows("Sheet1", 0, 2));
        assert!(out.contains("<xm:sqref>F3:F12</xm:sqref>"), "{out}");
        assert!(out.contains("<xm:f>I3</xm:f>"), "{out}");
        let other = shift_fragment(xml, &[], "Sheet2", &rows("Sheet1", 0, 2));
        assert_eq!(other, xml);
    }

    #[test]
    fn a_sparkline_follows_its_source_and_its_sheet_name() {
        let xml = "<x14:sparkline xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\" xmlns:xm=\"http://schemas.microsoft.com/office/excel/2006/main\"><xm:f>Spark!B2:E2</xm:f><xm:sqref>F2</xm:sqref></x14:sparkline>";
        let moved = shift_document(xml, "Spark", &rows("Spark", 0, 2));
        assert!(moved.contains("<xm:f>Spark!B4:E4</xm:f><xm:sqref>F4</xm:sqref>"), "{moved}");
        let renamed = shift_document(
            xml,
            "Spark",
            &StructuralEdit::Rename { from: "Spark".into(), to: "My sheet".into() },
        );
        assert!(renamed.contains("<xm:f>'My sheet'!B2:E2</xm:f>"), "{renamed}");
    }

    #[test]
    fn a_drawing_anchor_moves_down_and_survives_its_row_being_deleted() {
        let xml = "<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\"><xdr:twoCellAnchor><xdr:from><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row></xdr:from><xdr:to><xdr:col>13</xdr:col><xdr:row>16</xdr:row></xdr:to></xdr:twoCellAnchor></xdr:wsDr>";
        let out = shift_document(xml, "Charts", &rows("Charts", 0, 2));
        assert!(out.contains("<xdr:row>3</xdr:row>") && out.contains("<xdr:row>18</xdr:row>"));
        assert!(out.contains("<xdr:col>7</xdr:col>"));
        let deleted = shift_document(xml, "Charts", &rows("Charts", 1, -3));
        assert!(
            deleted.contains("<xdr:row>1</xdr:row>") && deleted.contains("<xdr:row>13</xdr:row>")
        );
    }

    #[test]
    fn a_pivot_cache_source_moves_and_is_renamed() {
        let xml = "<pivotCacheDefinition><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:D9\" sheet=\"Data\"/></cacheSource></pivotCacheDefinition>";
        let out = shift_document(xml, "", &rows("Data", 0, 2));
        assert!(out.contains("ref=\"A3:D11\""), "{out}");
        let renamed = shift_document(
            xml,
            "",
            &StructuralEdit::Rename { from: "Data".into(), to: "Renamed".into() },
        );
        assert!(renamed.contains("sheet=\"Renamed\""), "{renamed}");
    }

    #[test]
    fn a_page_break_moves_with_the_rows() {
        let xml = "<rowBreaks count=\"1\" manualBreakCount=\"1\"><brk id=\"19\" max=\"16383\" man=\"1\"/></rowBreaks>";
        let out = shift_fragment(xml, &[], "Print", &rows("Print", 0, 2));
        assert!(out.contains("id=\"21\""), "{out}");
    }

    #[test]
    fn a_chart_series_formula_moves_and_a_deleted_sheet_becomes_ref() {
        let xml = "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:val><c:numRef><c:f>Charts!$B$2:$B$6</c:f></c:numRef></c:val></c:chartSpace>";
        let out = shift_document(xml, "Charts", &rows("Charts", 0, 2));
        assert!(out.contains("<c:f>Charts!$B$4:$B$8</c:f>"), "{out}");
        let gone = shift_document(xml, "", &StructuralEdit::DeleteSheet { name: "Charts".into() });
        assert!(gone.contains("<c:f>#REF!</c:f>"), "{gone}");
    }

    /// Every element of the worksheet, drawing, chart, pivot and comment schemas that carries a cell
    /// reference, a sqref or a formula. Each must be modelled or in the registry.
    const SCHEMA_REFERENCES: &[&str] = &[
        "mergeCell",
        "hyperlink",
        "c",
        "row",
        "col",
        "dimension",
        "selection",
        "pane",
        "customSheetView",
        "autoFilter",
        "sortState",
        "sortCondition",
        "protectedRange",
        "ignoredError",
        "cellWatch",
        "dataRef",
        "scenario",
        "inputCells",
        "conditionalFormatting",
        "cfRule",
        "formula",
        "dataValidation",
        "formula1",
        "formula2",
        "brk",
        "singleXmlCell",
        "oleSize",
        "definedName",
        "sqref",
        "f",
        "from",
        "to",
        "threadedComment",
        "comment",
        "location",
        "worksheetSource",
        "rangeSet",
        "datalabelsRange",
        "tablePart",
        "table",
        "tableColumn",
        "calculatedColumnFormula",
        "totalsRowFormula",
    ];

    #[test]
    fn the_registry_covers_every_reference_in_the_schema() {
        let missing: Vec<&&str> = SCHEMA_REFERENCES
            .iter()
            .filter(|name| {
                let anchor = **name == "from" || **name == "to";
                let registered = REGISTRY
                    .iter()
                    .any(|r| r.element == **name || (anchor && r.parent == Some(**name)));
                !registered && !MODELLED.contains(name)
            })
            .collect();
        assert!(missing.is_empty(), "neither modelled nor in the registry: {missing:?}");
    }
}
