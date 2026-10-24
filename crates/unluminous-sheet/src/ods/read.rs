//! Reading an `.ods` file: the zip, the sheets with their rows, columns and cells.

use std::collections::HashMap;
use std::sync::Arc;

use roxmltree::{Document, Node};

use super::cellstyle::{points, Bag, CellStyles};
use super::read_features::Features;
use super::{attr, characters_from_cm, NS_CALCEXT, NS_DC, NS_OFFICE, NS_TABLE, NS_TEXT, NS_XLINK};
use crate::coord::{CellRef, Range};
use crate::formula::{Expr, Formula};
use crate::model::{
    Cell, ColumnInfo, Hyperlink, LinkTarget, Note, RowInfo, Sheet, Visibility, Workbook,
};
use crate::styles::Color;
use crate::value::{ErrorKind, Value};

/// The most cells a repeated empty-looking cell with a format is expanded into, so a row formatted to the
/// end of the sheet does not become sixteen thousand cells.
const MOST_STYLED_COLUMNS: u32 = 1024;
/// The most rows a repeated row is expanded into.
const MOST_REPEATED_ROWS: u32 = 10_000;

/// Read an `.ods` file's bytes into a workbook.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    let entries = crate::zip::read(bytes)
        .map_err(|e| e.replace("which an .xlsx file is", "which an .ods file is"))?;
    let text = |name: &str| {
        entries.iter().find(|e| e.name == name).map(|e| String::from_utf8_lossy(&e.data).into_owned())
    };
    if let Some(mime) = text("mimetype") {
        if !mime.trim().starts_with("application/vnd.oasis.opendocument.spreadsheet") {
            return Err(format!("this OpenDocument file is not a spreadsheet ({})", mime.trim()));
        }
    }
    let content = text("content.xml")
        .ok_or_else(|| "the file has no content.xml, so it is not an OpenDocument spreadsheet".to_owned())?;
    let styles = text("styles.xml");
    let settings = text("settings.xml");
    let content_doc = parse_xml(&content, "content.xml")?;
    let styles_doc = styles.as_deref().map(|s| parse_xml(s, "styles.xml")).transpose()?;
    let settings_doc = settings.as_deref().map(|s| parse_xml(s, "settings.xml")).transpose()?;
    let mut docs: Vec<&Document> = Vec::new();
    if let Some(d) = &styles_doc {
        docs.push(d);
    }
    docs.push(&content_doc);
    let mut book = read_documents(&content_doc, &docs, settings_doc.as_ref())?;
    for (path, data) in entries.iter().map(|e| (e.name.as_str(), &e.data)) {
        if path == "meta.xml" {
            book.kept.set("meta.xml", data.clone());
        }
    }
    Ok(book)
}

/// Parse one XML part, naming it in the problem.
fn parse_xml<'a>(xml: &'a str, name: &str) -> Result<Document<'a>, String> {
    Document::parse(xml).map_err(|e| format!("{name} is not valid XML: {e}"))
}

/// Everything read from the documents, once they are parsed.
fn read_documents(
    content: &Document,
    docs: &[&Document],
    settings: Option<&Document>,
) -> Result<Workbook, String> {
    let codes = super::numstyle::read_codes(docs);
    let mut cells = CellStyles::new(docs, codes);
    let mut book = Workbook::new();
    book.sheets.clear();
    cells.adopt_default_font(&mut book.styles);
    let body = content
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "spreadsheet")
        .ok_or_else(|| "the file has no spreadsheet in it".to_owned())?;
    let features = Features::new(body);
    book.date1904 = features.date1904;
    let tables: Vec<Node> = body.children().filter(|n| is_table(*n)).collect();
    let sheet_names: Vec<String> =
        tables.iter().map(|t| attr(*t, NS_TABLE, "name").unwrap_or("Sheet").to_owned()).collect();
    for table in &tables {
        let mut reader = SheetReader { book: &mut book, cells: &mut cells, features: &features, dynamic: Vec::new() };
        let sheet = reader.sheet(*table);
        book.sheets.push(sheet);
    }
    if book.sheets.is_empty() {
        book.sheets.push(Sheet::new("Sheet1"));
    }
    features.read_names(body, &sheet_names, &mut book);
    features.read_database_ranges(body, &mut book);
    if let Some(doc) = settings {
        super::read_features::read_settings(doc, &mut book);
    }
    Ok(book)
}

/// Whether a node is a sheet. A table named for a file is the cached copy of another workbook that an
/// external reference reads, and is not a sheet of this one.
fn is_table(node: Node) -> bool {
    node.is_element()
        && node.tag_name().namespace() == Some(NS_TABLE)
        && node.tag_name().name() == "table"
        && !attr(node, NS_TABLE, "name").is_some_and(|n| n.starts_with("'file:") || n.starts_with("file:"))
}

/// One column run of a table.
struct ColumnRun {
    repeat: u32,
    width: Option<f64>,
    hidden: bool,
    outline: u8,
    collapsed: bool,
    style: Option<String>,
}

/// Reads one sheet.
struct SheetReader<'r, 'd> {
    book: &'r mut Workbook,
    cells: &'r mut CellStyles<'d>,
    features: &'r Features<'d>,
    dynamic: Vec<Range>,
}

impl<'d> SheetReader<'_, 'd> {
    fn sheet(&mut self, table: Node<'d, 'd>) -> Sheet {
        let name = attr(table, NS_TABLE, "name").unwrap_or("Sheet");
        let mut sheet = Sheet::new(name);
        self.table_properties(table, &mut sheet);
        let runs = self.columns(table);
        self.apply_columns(&runs, &mut sheet);
        let defaults: Vec<(u32, u32, u32)> = self.column_defaults(&runs);
        let mut collector = Collected::default();
        self.rows(table, &mut sheet, &defaults, &mut collector);
        let dynamic = std::mem::take(&mut self.dynamic);
        apply_spills(&mut sheet, &dynamic);
        self.features.finish_sheet(table, &mut sheet, collector, self.cells, &mut self.book.styles);
        sheet
    }

    /// Visibility, tab colour and print ranges from the table's style.
    fn table_properties(&mut self, table: Node<'d, 'd>, sheet: &mut Sheet) {
        if let Some(style) = attr(table, NS_TABLE, "style-name") {
            let bag = self.cells.bag_of("table", style);
            if bag.get("tab.display").is_some_and(|d| d == "false") {
                sheet.state = Visibility::Hidden;
            }
            if let Some(color) = bag.get("tab.tab-color").and_then(|c| Color::parse_hex(c)) {
                sheet.tab_color = Some(color);
            }
        }
    }

    /// The column runs of a table, in order, with groups flattened.
    fn columns(&self, table: Node<'d, 'd>) -> Vec<ColumnRun> {
        let mut runs = Vec::new();
        self.walk_columns(table, 0, false, &mut runs);
        runs
    }

    fn walk_columns(&self, node: Node<'d, 'd>, depth: u8, collapsed: bool, runs: &mut Vec<ColumnRun>) {
        for c in node.children().filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS_TABLE)) {
            match c.tag_name().name() {
                "table-column" => runs.push(self.column_run(c, depth, collapsed)),
                "table-column-group" => {
                    let shut = attr(c, NS_TABLE, "display") == Some("false");
                    self.walk_columns(c, depth.saturating_add(1), shut, runs);
                }
                "table-columns" | "table-header-columns" => self.walk_columns(c, depth, collapsed, runs),
                _ => {}
            }
        }
    }

    fn column_run(&self, c: Node<'d, 'd>, outline: u8, collapsed: bool) -> ColumnRun {
        let bag = attr(c, NS_TABLE, "style-name")
            .map(|s| self.cells.bag_of("table-column", s))
            .unwrap_or_default();
        let visibility = attr(c, NS_TABLE, "visibility").unwrap_or("visible");
        let pixels = super::pixels_per_character(self.book);
        ColumnRun {
            repeat: repeated(c, "number-columns-repeated"),
            width: bag.get("col.column-width").and_then(|w| points(w)).map(|pt| characters_from_cm(pt / 72.0 * 2.54, pixels)),
            hidden: visibility != "visible",
            outline,
            collapsed: collapsed && outline > 0,
            style: attr(c, NS_TABLE, "default-cell-style-name").map(str::to_owned),
        }
    }

    /// Set the sheet's default width to the most common, and record the columns that differ.
    fn apply_columns(&mut self, runs: &[ColumnRun], sheet: &mut Sheet) {
        let mut counts: Vec<(i64, u64)> = Vec::new();
        for run in runs.iter().filter(|r| r.width.is_some()) {
            let key = (run.width.unwrap_or(0.0) * 1000.0).round() as i64;
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += run.repeat as u64,
                None => counts.push((key, run.repeat as u64)),
            }
        }
        let default = counts.iter().max_by_key(|(_, n)| *n).map(|(k, _)| *k as f64 / 1000.0);
        sheet.default_col_width = default;
        let mut col = 0u32;
        for run in runs {
            let first = col;
            col = col.saturating_add(run.repeat);
            let width = run.width.filter(|w| default.is_none_or(|d| (w - d).abs() > 0.005));
            let style = run.style.as_deref().map_or(0, |s| self.cells.xf(s, &mut self.book.styles));
            if width.is_none() && !run.hidden && style == 0 && run.outline == 0 {
                continue;
            }
            if first >= crate::coord::MAX_COLS {
                break;
            }
            sheet.columns.push(ColumnInfo {
                first,
                last: (col - 1).min(crate::coord::MAX_COLS - 1),
                width: width.or(run.hidden.then_some(default).flatten()),
                hidden: run.hidden,
                style,
                custom_width: width.is_some(),
                outline: run.outline,
                collapsed: run.collapsed,
                best_fit: false,
            });
        }
    }

    /// For each column run, the cell format its default style names.
    fn column_defaults(&mut self, runs: &[ColumnRun]) -> Vec<(u32, u32, u32)> {
        let mut out = Vec::new();
        let mut col = 0u32;
        for run in runs {
            let first = col;
            col = col.saturating_add(run.repeat);
            if let Some(style) = &run.style {
                let xf = self.cells.xf(style, &mut self.book.styles);
                out.push((first, col - 1, xf));
            }
        }
        out
    }

    fn rows(&mut self, table: Node<'d, 'd>, sheet: &mut Sheet, defaults: &[(u32, u32, u32)], out: &mut Collected) {
        let mut list: Vec<(Node<'d, 'd>, u8, bool)> = Vec::new();
        collect_rows(table, 0, false, &mut list);
        let default_height = self.default_row_height(&list);
        sheet.default_row_height = default_height;
        let mut r = 0u32;
        for (row, outline, collapsed) in list {
            let repeat = repeated(row, "number-rows-repeated");
            let height = self.row_height(row);
            let hidden = attr(row, NS_TABLE, "visibility").is_some_and(|v| v != "visible");
            let info = RowInfo {
                height: height.filter(|h| (h - default_height).abs() > 0.005),
                hidden,
                style: None,
                outline,
                collapsed: collapsed && outline > 0,
                attrs: Vec::new(),
            };
            let copies = repeat.min(MOST_REPEATED_ROWS);
            for k in 0..copies {
                let at = r + k;
                if info != RowInfo::default() {
                    sheet.rows.insert(at, info.clone());
                }
                self.cells_of_row(row, at, sheet, defaults, out, k == 0);
            }
            r = r.saturating_add(repeat);
            if r >= crate::coord::MAX_ROWS {
                break;
            }
        }
    }

    /// The most common row height, weighted by how many rows have it.
    fn default_row_height(&self, list: &[(Node<'d, 'd>, u8, bool)]) -> f64 {
        let mut counts: Vec<(i64, u64)> = Vec::new();
        for (row, _, _) in list {
            let Some(height) = self.row_height(*row) else { continue };
            let key = (height * 1000.0).round() as i64;
            let n = repeated(*row, "number-rows-repeated") as u64;
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, c)) => *c += n,
                None => counts.push((key, n)),
            }
        }
        counts.iter().max_by_key(|(_, n)| *n).map_or(14.25, |(k, _)| *k as f64 / 1000.0)
    }

    fn row_height(&self, row: Node<'d, 'd>) -> Option<f64> {
        let style = attr(row, NS_TABLE, "style-name")?;
        self.cells.bag_of("table-row", style).get("row.row-height").and_then(|h| points(h))
    }

    /// Read the cells of one row (the `first` time a repeated row is met, merges and notes are kept).
    fn cells_of_row(
        &mut self,
        row: Node<'d, 'd>,
        r: u32,
        sheet: &mut Sheet,
        defaults: &[(u32, u32, u32)],
        out: &mut Collected,
        first: bool,
    ) {
        let mut col = 0u32;
        let elements: Vec<Node<'d, 'd>> = row
            .children()
            .filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS_TABLE))
            .collect();
        for cell in elements {
            let covered = match cell.tag_name().name() {
                "table-cell" => false,
                "covered-table-cell" => true,
                _ => continue,
            };
            let times = repeated(cell, "number-columns-repeated");
            if !covered {
                self.one_cell(cell, r, col, times, sheet, defaults, out, first);
            }
            col = col.saturating_add(times);
            if col >= crate::coord::MAX_COLS {
                break;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn one_cell(
        &mut self,
        cell: Node<'d, 'd>,
        r: u32,
        col: u32,
        times: u32,
        sheet: &mut Sheet,
        defaults: &[(u32, u32, u32)],
        out: &mut Collected,
        first: bool,
    ) {
        let style_name = attr(cell, NS_TABLE, "style-name");
        let xf = style_name.map_or_else(
            || defaults.iter().find(|(a, b, _)| *a <= col && col <= *b).map_or(0, |d| d.2),
            |s| self.cells.xf(s, &mut self.book.styles),
        );
        let column_default = defaults.iter().find(|(a, b, _)| *a <= col && col <= *b).map_or(0, |d| d.2);
        let content = self.content(cell, xf, r, col, sheet, first);
        let validation = attr(cell, NS_TABLE, "content-validation-name");
        let has_maps = style_name.is_some_and(|s| !self.cells.maps(s).is_empty());
        let reach = if content.is_some() { times.min(MOST_STYLED_COLUMNS) } else {
            if xf == column_default && validation.is_none() && !has_maps {
                return;
            }
            times.min(MOST_STYLED_COLUMNS.saturating_sub(col))
        };
        for k in 0..reach {
            let at = col + k;
            match &content {
                Some(c) => {
                    sheet.cells.insert((r, at), c.clone());
                }
                None => {
                    if xf != column_default {
                        sheet.cells.insert((r, at), Cell { style: xf, ..Default::default() });
                    }
                }
            }
            if let Some(v) = validation {
                out.validated.entry(v.to_owned()).or_default().push((r, at));
            }
            if let (Some(s), true) = (style_name, has_maps) {
                out.mapped.entry(s.to_owned()).or_default().push((r, at));
            }
        }
    }

    /// The cell a table cell element holds, or `None` when it holds nothing.
    fn content(
        &mut self,
        cell: Node<'d, 'd>,
        xf: u32,
        r: u32,
        col: u32,
        sheet: &mut Sheet,
        first: bool,
    ) -> Option<Cell> {
        let at = CellRef::new(r, col);
        let cols = attr(cell, NS_TABLE, "number-columns-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        let rows = attr(cell, NS_TABLE, "number-rows-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        if first && (cols > 1 || rows > 1) {
            sheet.merges.push(Range::new(at, CellRef::new(r + rows - 1, col + cols - 1)));
        }
        if first {
            self.note_and_link(cell, at, sheet);
        }
        let mut value = self.value(cell);
        let mut formula = attr(cell, NS_TABLE, "formula").and_then(|f| self.formula(f, sheet));
        let matrix_cols = attr(cell, NS_TABLE, "number-matrix-columns-spanned").and_then(|v| v.parse::<u32>().ok());
        let matrix_rows = attr(cell, NS_TABLE, "number-matrix-rows-spanned").and_then(|v| v.parse::<u32>().ok());
        if let Some(f) = formula.as_mut() {
            f.dynamic = spills(&f.expr);
        }
        if let (Some(f), Some(c), Some(rw)) = (formula.as_mut(), matrix_cols, matrix_rows) {
            let range = Range::new(at, CellRef::new(r + rw - 1, col + c - 1));
            if f.dynamic {
                self.dynamic.push(range);
            } else if c * rw > 1 {
                f.array = Some(range);
                sheet.array_formulas.push(range);
            }
        }
        let (rich, text) = self.rich_text(cell);
        if matches!(value, Value::Text(_)) {
            if let Some(t) = text {
                value = Value::Text(Arc::from(t.as_str()));
            }
        }
        if value.is_empty() && formula.is_none() {
            return None;
        }
        Some(Cell { value, formula, style: xf, rich: rich.map(|r| Arc::from(r.as_str())), extra: None })
    }

    /// A note and links in a cell.
    fn note_and_link(&mut self, cell: Node<'d, 'd>, at: CellRef, sheet: &mut Sheet) {
        for annotation in cell.children().filter(|n| n.is_element() && n.tag_name().name() == "annotation") {
            let author = annotation
                .children()
                .find(|n| n.is_element() && n.tag_name().namespace() == Some(NS_DC) && n.tag_name().name() == "creator")
                .and_then(|n| n.text())
                .unwrap_or("")
                .to_owned();
            let text = annotation
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "p")
                .map(paragraph_text)
                .collect::<Vec<_>>()
                .join("\n");
            sheet.notes.insert((at.row, at.col), Note { author, text });
        }
        for link in cell.descendants().filter(|n| n.is_element() && n.tag_name().name() == "a") {
            let Some(href) = attr(link, NS_XLINK, "href") else { continue };
            let target = match href.strip_prefix('#') {
                Some(place) => LinkTarget::Location(location_text(place)),
                None => LinkTarget::External(href.to_owned()),
            };
            let display: String = link.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
            sheet.links.push(Hyperlink {
                range: Range::cell(at),
                target,
                display: (!display.is_empty()).then_some(display),
                tooltip: None,
            });
        }
    }

    /// A cell's value from its type attributes.
    fn value(&self, cell: Node<'d, 'd>) -> Value {
        let kind = attr(cell, NS_OFFICE, "value-type");
        let text = || {
            cell.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "p" && n.tag_name().namespace() == Some(NS_TEXT))
                .map(paragraph_text)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let number = |name: &str| attr(cell, NS_OFFICE, name).and_then(|v| v.trim().parse::<f64>().ok());
        let calc_error = attr(cell, NS_CALCEXT, "value-type") == Some("error");
        match kind {
            Some("float") | Some("percentage") | Some("currency") => {
                number("value").map_or(Value::Empty, Value::Number)
            }
            Some("date") => attr(cell, NS_OFFICE, "date-value")
                .and_then(|v| date_serial(v, self.book.date1904))
                .map_or(Value::Empty, Value::Number),
            Some("time") => attr(cell, NS_OFFICE, "time-value")
                .and_then(duration_fraction)
                .map_or(Value::Empty, Value::Number),
            Some("boolean") => Value::Bool(attr(cell, NS_OFFICE, "boolean-value") == Some("true")),
            Some("error") => {
                let shown = attr(cell, NS_OFFICE, "string-value").map(str::to_owned).unwrap_or_else(text);
                Value::Error(ErrorKind::parse(&shown).unwrap_or(ErrorKind::Value))
            }
            Some("string") => {
                let shown = attr(cell, NS_OFFICE, "string-value").map(str::to_owned).unwrap_or_else(text);
                match (calc_error, ErrorKind::parse(&shown)) {
                    (true, Some(kind)) => Value::Error(kind),
                    _ => Value::text(shown),
                }
            }
            _ => {
                let shown = text();
                if shown.is_empty() { Value::Empty } else { Value::text(shown) }
            }
        }
    }

    /// The runs of a cell's text when it has formatted spans, and its plain text.
    fn rich_text(&self, cell: Node<'d, 'd>) -> (Option<String>, Option<String>) {
        let paragraphs: Vec<Node> = cell
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "p" && n.tag_name().namespace() == Some(NS_TEXT))
            .collect();
        if paragraphs.is_empty() {
            return (None, None);
        }
        let mut runs: Vec<(String, Option<Bag>)> = Vec::new();
        let mut any_style = false;
        for (i, p) in paragraphs.iter().enumerate() {
            if i > 0 {
                runs.push(("\n".to_owned(), None));
            }
            self.paragraph_runs(*p, None, &mut runs, &mut any_style);
        }
        let plain: String = runs.iter().map(|(t, _)| t.as_str()).collect();
        if !any_style {
            return (None, Some(plain));
        }
        let mut xml = String::from("<si>");
        for (text, bag) in &runs {
            if text.is_empty() {
                continue;
            }
            xml.push_str("<r>");
            if let Some(bag) = bag {
                xml.push_str(&run_properties(bag));
            }
            xml.push_str(&format!("<t xml:space=\"preserve\">{}</t></r>", crate::xlsx::escape(text)));
        }
        xml.push_str("</si>");
        (Some(xml), Some(plain))
    }

    fn paragraph_runs(&self, node: Node<'d, 'd>, bag: Option<&Bag>, runs: &mut Vec<(String, Option<Bag>)>, any: &mut bool) {
        for c in node.children() {
            if c.is_text() {
                runs.push((c.text().unwrap_or("").to_owned(), bag.cloned()));
            } else if c.is_element() {
                match c.tag_name().name() {
                    "span" => {
                        let own = attr(c, NS_TEXT, "style-name").map(|s| self.cells.bag_of("text", s));
                        let own = own.filter(|b| !b.is_empty());
                        *any |= own.is_some();
                        self.paragraph_runs(c, own.as_ref().or(bag), runs, any);
                    }
                    "a" => self.paragraph_runs(c, bag, runs, any),
                    "s" => {
                        let n = attr(c, NS_TEXT, "c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                        runs.push((" ".repeat(n), bag.cloned()));
                    }
                    "tab" => runs.push(("\t".to_owned(), bag.cloned())),
                    "line-break" => runs.push(("\n".to_owned(), bag.cloned())),
                    _ => {}
                }
            }
        }
    }

    /// A formula attribute as a formula, `None` when it is not one this reader can use.
    fn formula(&self, text: &str, sheet: &Sheet) -> Option<Formula> {
        let _ = sheet;
        let (excel, body) = match text.split_once(":=") {
            Some(("msoxl", body)) => (true, body),
            Some((_, body)) => (false, body),
            None => (false, text.strip_prefix('=').unwrap_or(text)),
        };
        let a1 = if excel { body.to_owned() } else { super::formula::to_a1(body) };
        Some(kept_formula(&a1))
    }
}

/// The `RowInfo`-free walk of the rows, flattening groups and header rows.
fn collect_rows<'d>(node: Node<'d, 'd>, depth: u8, collapsed: bool, out: &mut Vec<(Node<'d, 'd>, u8, bool)>) {
    for c in node.children().filter(|n| n.is_element() && n.tag_name().namespace() == Some(NS_TABLE)) {
        match c.tag_name().name() {
            "table-row" => out.push((c, depth, collapsed)),
            "table-row-group" => {
                let shut = attr(c, NS_TABLE, "display") == Some("false");
                collect_rows(c, depth.saturating_add(1), shut, out);
            }
            "table-rows" | "table-header-rows" => collect_rows(c, depth, collapsed, out),
            _ => {}
        }
    }
}

/// What the cells of a sheet need after all its cells are read: which cells carry a validation or a
/// conditional style.
#[derive(Default)]
pub struct Collected {
    pub validated: HashMap<String, Vec<(u32, u32)>>,
    pub mapped: HashMap<String, Vec<(u32, u32)>>,
}

/// A repeat count attribute, one when it is absent.
fn repeated(node: Node, name: &str) -> u32 {
    attr(node, NS_TABLE, name).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).max(1)
}

/// The text of a paragraph, with spaces, tabs and line breaks as the characters they stand for.
pub fn paragraph_text(p: Node) -> String {
    let mut out = String::new();
    for c in p.children() {
        if c.is_text() {
            out.push_str(c.text().unwrap_or(""));
        } else if c.is_element() {
            match c.tag_name().name() {
                "s" => out.push_str(&" ".repeat(attr(c, NS_TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1))),
                "tab" => out.push('\t'),
                "line-break" => out.push('\n'),
                "span" | "a" => out.push_str(&paragraph_text(c)),
                _ => {}
            }
        }
    }
    out
}

/// A place a link names inside the workbook: `'Other'.B2` as `Other!B2`.
fn location_text(place: &str) -> String {
    let (sheet, cells) = super::split_address(place);
    if cells.is_empty() {
        return sheet;
    }
    format!("{}!{cells}", crate::formula::print::sheet_name(&sheet))
}

/// The rich text run properties for a text style's properties.
fn run_properties(bag: &Bag) -> String {
    let mut out = String::from("<rPr>");
    if bag.get("t.font-weight").is_some_and(|w| w == "bold") {
        out.push_str("<b/>");
    }
    if bag.get("t.font-style").is_some_and(|w| w == "italic") {
        out.push_str("<i/>");
    }
    if bag.get("t.text-line-through-style").is_some_and(|w| w != "none") {
        out.push_str("<strike/>");
    }
    if bag.get("t.text-underline-style").is_some_and(|w| w != "none") {
        out.push_str("<u/>");
    }
    if let Some(size) = bag.get("t.font-size").and_then(|s| points(s)) {
        out.push_str(&format!("<sz val=\"{size}\"/>"));
    }
    if let Some(Color::Rgb(argb)) = bag.get("t.color").and_then(|c| Color::parse_hex(c)) {
        out.push_str(&format!("<color rgb=\"{argb:08X}\"/>"));
    }
    if let Some(name) = bag.get("t.font-name").or_else(|| bag.get("t.font-family")) {
        out.push_str(&format!("<rFont val=\"{}\"/>", crate::xlsx::escape(name.trim_matches('\''))));
    }
    out.push_str("</rPr>");
    out
}

/// The functions that return a whole array, which is how a matrix formula Excel wrote is told from a
/// legacy array formula: the file marks both the same way.
const SPILLING: [&str; 24] = [
    "SEQUENCE", "SORT", "SORTBY", "FILTER", "UNIQUE", "RANDARRAY", "TEXTSPLIT", "VSTACK", "HSTACK", "TOCOL",
    "TOROW", "WRAPCOLS", "WRAPROWS", "TAKE", "DROP", "CHOOSECOLS", "CHOOSEROWS", "EXPAND", "MAKEARRAY", "BYROW",
    "BYCOL", "SCAN", "REDUCE", "MAP",
];

/// Whether a formula calls a function that returns an array to spill.
fn spills(expr: &Expr) -> bool {
    let mut found = false;
    expr.walk(&mut |e| {
        if let Expr::Call(name, _) = e {
            found |= SPILLING.contains(&&**name);
        }
    });
    found
}

/// Move the values a dynamic array formula's range holds into the spill, as the `.xlsx` reader does.
fn apply_spills(sheet: &mut Sheet, ranges: &[Range]) {
    for range in ranges {
        let anchor = (range.first.row, range.first.col);
        if range.len() > 1 {
            sheet.spills.anchors.insert(anchor, (range.rows(), range.cols()));
        }
        for at in range.cells() {
            let key = (at.row, at.col);
            if key == anchor {
                continue;
            }
            let held = sheet.cells.get(&key).filter(|c| c.formula.is_none()).cloned();
            let value = held.as_ref().map_or(Value::Empty, |c| c.value.clone());
            sheet.spills.owner.insert(key, anchor);
            sheet.spills.values.insert(key, value);
            if let Some(cell) = held {
                sheet.cells.remove(&key);
                if cell.style != 0 {
                    sheet.cells.insert(key, Cell { style: cell.style, ..Default::default() });
                }
            }
        }
    }
}

/// A formula from text in Excel's syntax, frozen when it names a function this version does not have.
pub fn kept_formula(a1: &str) -> Formula {
    match Formula::parse_kept(a1) {
        Ok(mut f) => {
            f.dynamic = false;
            let mut unknown = false;
            f.expr.walk(&mut |e| match e {
                Expr::External(_) => unknown = true,
                Expr::Call(name, _) if crate::functions::find(name).is_none() && (name.contains('.') || name.starts_with('_')) => {
                    unknown = true;
                }
                _ => {}
            });
            f.frozen = unknown;
            f
        }
        Err(_) => Formula {
            text: Arc::from(a1),
            expr: Arc::new(Expr::Error(ErrorKind::Name)),
            dynamic: false,
            array: None,
            array_member: None,
            frozen: true,
        },
    }
}

/// The serial number of a date or date and time such as `2026-10-08T12:30:00`.
pub(crate) fn date_serial(text: &str, date1904: bool) -> Option<f64> {
    let (date, time) = match text.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (text, None),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    let days = crate::dates::serial(year, month, day, date1904)?;
    let fraction = time.map_or(0.0, |t| {
        let mut p = t.trim_end_matches('Z').split(':');
        let h: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let m: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let s: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        (h * 3600.0 + m * 60.0 + s) / 86400.0
    });
    Some(days + fraction)
}

/// A duration such as `PT12H30M00S` as a fraction of a day.
fn duration_fraction(text: &str) -> Option<f64> {
    let body = text.strip_prefix('P')?;
    let (days_part, time_part) = match body.split_once('T') {
        Some((d, t)) => (d, t),
        None => (body, ""),
    };
    let mut seconds = 0.0;
    if let Some(d) = days_part.strip_suffix('D') {
        seconds += d.parse::<f64>().ok()? * 86400.0;
    }
    let mut number = String::new();
    for c in time_part.chars() {
        match c {
            'H' => seconds += number.parse::<f64>().ok()? * 3600.0,
            'M' => seconds += number.parse::<f64>().ok()? * 60.0,
            'S' => seconds += number.parse::<f64>().ok()?,
            c => {
                number.push(c);
                continue;
            }
        }
        number.clear();
    }
    Some(seconds / 86400.0)
}
