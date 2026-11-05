//! How a table's style and a PivotTable's style dress their cells, and how a slicer style draws a slicer
//! (task-2513).
//!
//! A table made with Format as Table, or a PivotTable, names one of Excel's built-in styles
//! (`TableStyleMedium2`, `PivotStyleLight16`). The style is a list of parts: the whole table, the header
//! row, the first row stripe, the grand total row and so on, each with a fill, a font color, bold, italic and
//! borders. Excel keeps those definitions inside the program, so `table_styles.json` is what Excel itself
//! answered for every built-in style, written by `tools/excel-oracle/table-styles.ps1`, with each color kept
//! as a theme color and tint so a style follows the workbook's theme.
//!
//! [`dress_sheet`] lays the parts over the cells in Excel's order, least specific first, and answers what
//! each cell gets. The grid draws a cell's own formatting over that: a fill or a font color the cell sets
//! itself wins, as it does in Excel, and conditional formatting wins over both.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::coord::{CellRef, Range};
use crate::json::{self, Json};
use crate::model::{Table, Workbook};
use crate::pivot::compute::Calc;
use crate::pivot::layout::{lay_out, Role};
use crate::pivot::PivotTable;
use crate::styles::Color;

/// One side of a cell's border that a style draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// `thin`, `medium`, `thick`, `double` or `hair`.
    pub style: String,
    pub color: Color,
}

/// What a style gives one cell. A field left `None` is not set by any part that covers the cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dress {
    pub fill: Option<Color>,
    pub color: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub left: Option<Line>,
    pub top: Option<Line>,
    pub right: Option<Line>,
    pub bottom: Option<Line>,
    /// The header cell of a table that shows its filter buttons.
    pub filter_button: bool,
}

/// One part of a style, as Excel defines it.
#[derive(Debug, Clone, Default)]
struct Part {
    fill: Option<Color>,
    color: Option<Color>,
    bold: bool,
    italic: bool,
    /// left, top, bottom, right, vertical (inside), horizontal (inside).
    borders: [Option<Line>; 6],
    /// How many rows or columns one stripe covers.
    size: u32,
}

const LEFT: usize = 0;
const TOP: usize = 1;
const BOTTOM: usize = 2;
const RIGHT: usize = 3;
const VERTICAL: usize = 4;
const HORIZONTAL: usize = 5;

type Style = HashMap<String, Part>;

/// Every built-in style, read once from the file Excel's answers were written to.
fn styles() -> &'static HashMap<String, Style> {
    static STYLES: OnceLock<HashMap<String, Style>> = OnceLock::new();
    STYLES.get_or_init(|| {
        let Ok(Json::Object(entries)) = json::parse(include_str!("table_styles.json")) else {
            return HashMap::new();
        };
        entries
            .into_iter()
            .filter_map(|(name, parts)| match parts {
                Json::Object(parts) => {
                    Some((name, parts.into_iter().map(|(p, v)| (p, part_of(&v))).collect()))
                }
                _ => None,
            })
            .collect()
    })
}

fn field<'a>(json: &'a Json, key: &str) -> Option<&'a Json> {
    match json {
        Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

/// A color as the file writes it: `{theme, tint}` or `{rgb}`.
fn color_of(json: &Json) -> Option<Color> {
    if let Some(Json::Number(theme)) = field(json, "theme") {
        let tint = match field(json, "tint") {
            Some(Json::Number(t)) => *t,
            _ => 0.0,
        };
        return Some(Color::Theme(*theme as u32, tint));
    }
    match field(json, "rgb") {
        Some(Json::Text(hex)) => {
            u32::from_str_radix(hex, 16).ok().map(|v| Color::Rgb(0xFF00_0000 | v))
        }
        _ => None,
    }
}

fn part_of(json: &Json) -> Part {
    let mut part = Part {
        fill: field(json, "fill").and_then(color_of),
        color: field(json, "color").and_then(color_of),
        bold: matches!(field(json, "bold"), Some(Json::Bool(true))),
        italic: matches!(field(json, "italic"), Some(Json::Bool(true))),
        size: match field(json, "size") {
            Some(Json::Number(n)) => (*n as u32).max(1),
            _ => 1,
        },
        ..Part::default()
    };
    if let Some(borders) = field(json, "borders") {
        for (i, side) in
            ["left", "top", "bottom", "right", "vertical", "horizontal"].iter().enumerate()
        {
            if let Some(line) = field(borders, side) {
                let style = match field(line, "style") {
                    Some(Json::Text(s)) => s.clone(),
                    _ => "thin".to_owned(),
                };
                part.borders[i] =
                    Some(Line { style, color: color_of(line).unwrap_or(Color::Auto) });
            }
        }
    }
    part
}

/// Whether a built-in style of this name is known.
pub fn is_known(name: &str) -> bool {
    styles().contains_key(name)
}

/// Lay one part over a rectangle of cells: its fill, font and bold everywhere, its outer borders on the
/// rectangle's edges, and its inside borders between the cells.
fn lay(out: &mut HashMap<(u32, u32), Dress>, part: &Part, area: Range) {
    for row in area.first.row..=area.last.row {
        for col in area.first.col..=area.last.col {
            let dress = out.entry((row, col)).or_default();
            if part.fill.is_some() {
                dress.fill = part.fill;
            }
            if part.color.is_some() {
                dress.color = part.color;
            }
            dress.bold |= part.bold;
            dress.italic |= part.italic;
            let side = |outer: usize, inner: usize, on_edge: bool| {
                if on_edge {
                    part.borders[outer].clone()
                } else {
                    part.borders[inner].clone()
                }
            };
            if let Some(l) = side(LEFT, VERTICAL, col == area.first.col) {
                dress.left = Some(l);
            }
            if let Some(l) = side(RIGHT, VERTICAL, col == area.last.col) {
                dress.right = Some(l);
            }
            if let Some(l) = side(TOP, HORIZONTAL, row == area.first.row) {
                dress.top = Some(l);
            }
            if let Some(l) = side(BOTTOM, HORIZONTAL, row == area.last.row) {
                dress.bottom = Some(l);
            }
        }
    }
}

/// Lay a part named in the style, if the style defines it.
fn lay_named(out: &mut HashMap<(u32, u32), Dress>, style: &Style, name: &str, area: Range) {
    if let Some(part) = style.get(name) {
        lay(out, part, area);
    }
}

/// Lay the two stripes of a direction over an area, `size` rows or columns at a time.
fn stripes(out: &mut HashMap<(u32, u32), Dress>, style: &Style, rows: bool, area: Range) {
    let (first, second) = if rows {
        ("firstRowStripe", "secondRowStripe")
    } else {
        ("firstColumnStripe", "secondColumnStripe")
    };
    let size1 = style.get(first).map_or(1, |p| p.size);
    let size2 = style.get(second).map_or(1, |p| p.size);
    let (start, end) =
        if rows { (area.first.row, area.last.row) } else { (area.first.col, area.last.col) };
    let mut at = start;
    let mut odd = true;
    while at <= end {
        let size = if odd { size1 } else { size2 };
        let stop = (at + size - 1).min(end);
        let band = if rows {
            Range::new(CellRef::new(at, area.first.col), CellRef::new(stop, area.last.col))
        } else {
            Range::new(CellRef::new(area.first.row, at), CellRef::new(area.last.row, stop))
        };
        lay_named(out, style, if odd { first } else { second }, band);
        at = stop + 1;
        odd = !odd;
    }
}

/// The options a table's `tableStyleInfo` sets, and whether its header shows filter buttons.
struct TableOptions {
    name: Option<String>,
    row_stripes: bool,
    column_stripes: bool,
    first_column: bool,
    last_column: bool,
    filter_buttons: bool,
}

fn table_options(table: &Table) -> TableOptions {
    let doc = roxmltree::Document::parse(&table.xml).ok();
    let root = doc.as_ref().map(|d| d.root_element());
    let child = |name: &str| root.and_then(|r| r.children().find(|c| c.tag_name().name() == name));
    let info = child("tableStyleInfo");
    let flag = |n: &str| info.and_then(|i| i.attribute(n)).is_some_and(|v| v == "1" || v == "true");
    let filter_buttons = child("autoFilter").is_some_and(|a| {
        !a.children().any(|c| {
            c.tag_name().name() == "filterColumn" && c.attribute("hiddenButton") == Some("1")
        })
    });
    TableOptions {
        name: info.and_then(|i| i.attribute("name")).map(str::to_owned),
        row_stripes: flag("showRowStripes"),
        column_stripes: flag("showColumnStripes"),
        first_column: flag("showFirstColumn"),
        last_column: flag("showLastColumn"),
        filter_buttons,
    }
}

/// Dress the cells of a table made with Format as Table, in the order Excel lays a table style's parts.
fn dress_table(out: &mut HashMap<(u32, u32), Dress>, table: &Table) {
    let options = table_options(table);
    let r = table.range;
    let row_of =
        |row: u32| Range::new(CellRef::new(row, r.first.col), CellRef::new(row, r.last.col));
    if let Some(style) = options.name.as_deref().and_then(|n| styles().get(n)) {
        lay_named(out, style, "wholeTable", r);
        if let Some(data) = table.data_range() {
            if options.column_stripes {
                stripes(out, style, false, data);
            }
            if options.row_stripes {
                stripes(out, style, true, data);
            }
        }
        let column =
            |col: u32| Range::new(CellRef::new(r.first.row, col), CellRef::new(r.last.row, col));
        if options.last_column {
            lay_named(out, style, "lastColumn", column(r.last.col));
        }
        if options.first_column {
            lay_named(out, style, "firstColumn", column(r.first.col));
        }
        if table.header_row {
            lay_named(out, style, "headerRow", row_of(r.first.row));
        }
        if table.totals_row {
            lay_named(out, style, "totalRow", row_of(r.last.row));
        }
        if table.header_row {
            lay_named(out, style, "firstHeaderCell", Range::new(r.first, r.first));
            let last = CellRef::new(r.first.row, r.last.col);
            lay_named(out, style, "lastHeaderCell", Range::new(last, last));
        }
        if table.totals_row {
            let first = CellRef::new(r.last.row, r.first.col);
            lay_named(out, style, "firstTotalCell", Range::new(first, first));
            lay_named(out, style, "lastTotalCell", Range::new(r.last, r.last));
        }
    }
    if table.header_row && options.filter_buttons {
        for col in r.first.col..=r.last.col {
            out.entry((r.first.row, col)).or_default().filter_button = true;
        }
    }
}

/// Dress the cells of a PivotTable, from the role each of its cells was laid out with.
fn dress_pivot(out: &mut HashMap<(u32, u32), Dress>, book: &Workbook, table: &PivotTable) {
    let Some(style) = table.style.name.as_deref().and_then(|n| styles().get(n)) else { return };
    if !table.has_cells {
        return;
    }
    let Some(cache) = book.pivots.caches.get(table.cache) else { return };
    let laid = lay_out(&Calc::build(table, cache, book.date1904));
    let loc = table.location;
    let origin = CellRef::new(loc.first.row.saturating_sub(laid.page_rows), loc.first.col);
    let at = |row: u32, col: u32| CellRef::new(origin.row + row, origin.col + col);
    let row = |r: u32| Range::new(CellRef::new(r, loc.first.col), CellRef::new(r, loc.last.col));
    let col = |c: u32| Range::new(CellRef::new(loc.first.row, c), CellRef::new(loc.last.row, c));

    lay_named(out, style, "wholeTable", loc);
    for cell in laid.cells.iter().filter(|c| c.role == Role::PageField) {
        let a = at(cell.row, cell.col);
        lay_named(out, style, "pageFieldLabels", Range::new(a, a));
    }
    for cell in laid.cells.iter().filter(|c| c.role == Role::PageItem) {
        let a = at(cell.row, cell.col);
        lay_named(out, style, "pageFieldValues", Range::new(a, a));
    }
    // The figures: from the first data row down and from the first data column across.
    let header_rows = loc.first.row..loc.first.row + table.first_data_row;
    let body =
        Range::new(CellRef::new(loc.first.row + table.first_data_row, loc.first.col), loc.last);
    if table.style.show_col_stripes && body.first.row <= body.last.row {
        stripes(out, style, false, body);
    }
    if table.style.show_row_stripes && body.first.row <= body.last.row {
        stripes(out, style, true, body);
    }
    if table.style.show_row_headers && body.first.row <= body.last.row {
        let labels = Range::new(
            body.first,
            CellRef::new(body.last.row, loc.first.col + table.first_data_col.max(1) - 1),
        );
        lay_named(out, style, "firstColumn", labels);
    }
    if table.style.show_col_headers && !header_rows.is_empty() {
        // One area for all the header rows, so the part's outline goes round them together.
        let area = Range::new(row(header_rows.start).first, row(header_rows.end - 1).last);
        lay_named(out, style, "headerRow", area);
    }
    let outer_row_fields =
        table.rows.iter().filter(|e| matches!(e, crate::pivot::table::AxisEntry::Field(_))).count();
    for cell in &laid.cells {
        let a = at(cell.row, cell.col);
        let whole_row = row(a.row);
        match cell.role {
            Role::RowLabel if outer_row_fields > 1 && cell.indent == 0 && cell.col == 0 => {
                lay_named(out, style, "firstRowSubheading", whole_row)
            }
            Role::SubtotalLabel if cell.col == 0 => {
                lay_named(out, style, "firstSubtotalRow", whole_row)
            }
            _ => {}
        }
    }
    // The grand total column, from its caption in the header rows, and the grand total row.
    for cell in laid.cells.iter().filter(|c| c.role == Role::GrandLabel) {
        let a = at(cell.row, cell.col);
        if header_rows.contains(&a.row) {
            if table.style.show_last_column {
                lay_named(out, style, "lastColumn", col(a.col));
            }
        } else {
            lay_named(out, style, "totalRow", row(a.row));
        }
    }
    lay_named(out, style, "firstHeaderCell", Range::new(loc.first, loc.first));
}

/// How one part of a slicer is drawn: a fill, the text's color and weight, and a line round it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlicerPart {
    pub fill: Option<Color>,
    pub color: Option<Color>,
    pub bold: bool,
    /// The line round the part; for the header, the line under it.
    pub line: Option<Line>,
}

/// How a slicer style draws a slicer, from Excel's definition of the style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlicerLook {
    /// The line round the whole slicer.
    pub frame: Option<Line>,
    pub header: SlicerPart,
    pub selected: SlicerPart,
    pub unselected: SlicerPart,
    pub selected_no_data: SlicerPart,
    pub unselected_no_data: SlicerPart,
}

/// The look of a built-in slicer style, `SlicerStyleLight1` and the rest. Excel's hovered parts are gradients,
/// which come back through COM as black, so they are not read.
pub fn slicer_look(name: &str) -> Option<SlicerLook> {
    let style = styles().get(name)?;
    let part = |key: &str, line: usize| {
        style.get(key).map_or_else(SlicerPart::default, |p| SlicerPart {
            fill: p.fill,
            color: p.color,
            bold: p.bold,
            line: p.borders[line].clone(),
        })
    };
    Some(SlicerLook {
        frame: style.get("wholeTable").and_then(|p| p.borders[LEFT].clone()),
        header: part("headerRow", BOTTOM),
        selected: part("selectedItemWithData", LEFT),
        unselected: part("unselectedItemWithData", LEFT),
        selected_no_data: part("selectedItemWithNoData", LEFT),
        unselected_no_data: part("unselectedItemWithNoData", LEFT),
    })
}

/// The header cells of the tables on a sheet that show a filter button.
pub fn filter_button_cells(book: &Workbook, sheet: usize) -> Vec<CellRef> {
    book.tables
        .iter()
        .filter(|t| t.sheet == sheet && t.header_row && table_options(t).filter_buttons)
        .flat_map(|t| {
            (t.range.first.col..=t.range.last.col).map(move |c| CellRef::new(t.range.first.row, c))
        })
        .collect()
}

/// The table on a sheet whose header shows a filter button over a column: the one that holds the row
/// when the sheet has several, otherwise the first. Answers its index in `Workbook::tables`.
pub fn filter_table_at(book: &Workbook, sheet: usize, col: u32, row: u32) -> Option<usize> {
    let candidates: Vec<usize> = book
        .tables
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.sheet == sheet
                && t.header_row
                && col >= t.range.first.col
                && col <= t.range.last.col
                && table_options(t).filter_buttons
        })
        .map(|(i, _)| i)
        .collect();
    candidates
        .iter()
        .copied()
        .find(|i| {
            let r = book.tables[*i].range;
            row >= r.first.row && row <= r.last.row
        })
        .or_else(|| candidates.first().copied())
}

/// What the table styles and PivotTable styles on a sheet give each of its cells, by `(row, col)`.
pub fn dress_sheet(book: &Workbook, sheet: usize) -> HashMap<(u32, u32), Dress> {
    let mut out = HashMap::new();
    for table in book.tables.iter().filter(|t| t.sheet == sheet) {
        dress_table(&mut out, table);
    }
    for table in book.pivots.tables.iter().filter(|t| t.sheet == sheet) {
        dress_pivot(&mut out, book, table);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_excel_offers_is_read() {
        assert_eq!(styles().len(), 158);
        assert!(is_known("TableStyleMedium2"));
        assert!(is_known("PivotStyleLight16"));
        let slicer = slicer_look("SlicerStyleLight1").unwrap();
        assert_eq!(slicer.frame.map(|l| l.color), Some(Color::Theme(4, 0.0)));
        assert!(matches!(slicer.selected.fill, Some(Color::Theme(4, t)) if (t - 0.6).abs() < 0.01));
        assert_eq!(slicer.unselected.fill, Some(Color::Rgb(0xFFFF_FFFF)));
    }

    #[test]
    fn medium_2_is_a_header_in_accent_1_with_white_bold_text_and_light_stripes() {
        let style = &styles()["TableStyleMedium2"];
        let header = &style["headerRow"];
        assert_eq!(header.fill, Some(Color::Theme(4, 0.0)));
        assert_eq!(header.color, Some(Color::Theme(0, 0.0)));
        assert!(header.bold);
        let stripe = &style["firstRowStripe"];
        // Excel draws it C0E6F5; the tint math lands one unit off on two channels.
        assert_eq!(stripe.fill.and_then(|c| c.resolve(&[])), Some((0xC1, 0xE5, 0xF5)));
    }

    /// Every theme color in the file comes out within three units of each channel of the color Excel
    /// resolved it to in the Office theme, so the theme index and the tint are read the way Excel reads them.
    /// Exact is not reachable: Excel's tint math is not the floating point HLS `styles::tinted` uses, nor
    /// Windows' integer HLS at 240 or 255 steps, all of which were tried against these 2,884 colors. The
    /// largest difference is two units, which nobody can see.
    #[test]
    fn every_theme_color_resolves_to_what_excel_drew() {
        fn walk(json: &Json, path: &str, wrong: &mut Vec<String>) {
            match json {
                Json::Object(entries) => {
                    if let (Some(c), Some(Json::Text(hex))) = (color_of(json), field(json, "rgb")) {
                        let want = u32::from_str_radix(hex, 16).unwrap();
                        let (r, g, b) = c.resolve(&[]).unwrap();
                        let close = |a: u8, b: u32| (a as i32 - b as i32).abs() <= 3;
                        if !(close(r, want >> 16 & 255)
                            && close(g, want >> 8 & 255)
                            && close(b, want & 255))
                        {
                            wrong.push(format!("{path}: {r:02X}{g:02X}{b:02X} against {hex}"));
                        }
                    }
                    for (k, v) in entries {
                        walk(v, &format!("{path}/{k}"), wrong);
                    }
                }
                Json::Array(items) => items.iter().for_each(|v| walk(v, path, wrong)),
                _ => {}
            }
        }
        let mut wrong = Vec::new();
        walk(&json::parse(include_str!("table_styles.json")).unwrap(), "", &mut wrong);
        assert!(
            wrong.is_empty(),
            "{} colors differ from Excel's:
{}",
            wrong.len(),
            wrong.join(
                "
"
            )
        );
    }

    /// A small sheet of orders: Region, Product, Revenue over four rows, in A1:C5.
    fn orders() -> crate::edit::Session {
        let mut session = crate::edit::Session::new(Workbook::new(), 46318.5);
        let rows = [
            ["Region", "Product", "Revenue"],
            ["North", "Espresso", "10"],
            ["South", "House", "20"],
            ["North", "House", "30"],
            ["West", "Decaf", "40"],
        ];
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                session.set_input(0, CellRef::new(r as u32, c as u32), text).unwrap();
            }
        }
        session
    }

    #[test]
    fn a_table_gets_its_header_its_stripes_and_its_filter_buttons() {
        let mut session = orders();
        let range = Range::new(CellRef::new(0, 0), CellRef::new(4, 2));
        session.add_table(0, range, None).unwrap();
        let dress = dress_sheet(&session.book, 0);
        let header = &dress[&(0, 0)];
        assert_eq!(header.fill, Some(Color::Theme(4, 0.0)));
        assert_eq!(header.color, Some(Color::Theme(0, 0.0)));
        assert!(header.bold && header.filter_button);
        // The first data row is the first stripe and the second is not filled.
        assert!(matches!(dress[&(1, 1)].fill, Some(Color::Theme(4, t)) if (t - 0.8).abs() < 0.01));
        assert_eq!(dress[&(2, 1)].fill, None);
        assert_eq!(filter_button_cells(&session.book, 0).len(), 3);
        assert_eq!(filter_table_at(&session.book, 0, 1, 3), Some(0));
        assert_eq!(filter_table_at(&session.book, 0, 5, 3), None);
    }

    #[test]
    fn a_pivot_table_gets_light_16_header_and_grand_total_rows() {
        use crate::pivot::session::{Area, PivotSource};
        let mut session = orders();
        let range = Range::new(CellRef::new(0, 0), CellRef::new(4, 2));
        let t = session
            .pivot_create(PivotSource::Range { sheet: 0, range }, 0, CellRef::new(2, 5), None)
            .unwrap();
        session.pivot_move_field(t, 1, Area::Rows, None).unwrap();
        session.pivot_add_value(t, 2, crate::pivot::Func::Sum, None).unwrap();
        let table = &session.book.pivots.tables[t];
        let loc = table.location;
        let dress = dress_sheet(&session.book, 0);
        let light = |at: (u32, u32)| matches!(dress.get(&at).and_then(|d| d.fill), Some(Color::Theme(4, t)) if (t - 0.8).abs() < 0.01);
        // Row Labels and Sum of Revenue, then Decaf, Espresso and House, then Grand Total.
        assert!(light((loc.first.row, loc.first.col)) && light((loc.first.row, loc.last.col)));
        assert!(dress[&(loc.first.row, loc.first.col)].bold);
        assert!(!light((loc.first.row + 1, loc.first.col)));
        assert!(light((loc.last.row, loc.first.col)) && light((loc.last.row, loc.last.col)));
        assert!(dress[&(loc.last.row, loc.last.col)].top.is_some());
    }
}
