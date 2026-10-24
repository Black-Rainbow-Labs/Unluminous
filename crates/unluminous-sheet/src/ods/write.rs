//! Writing an `.ods` file: `content.xml` with the sheets and the styles their cells name, `styles.xml`,
//! `settings.xml`, `meta.xml` and the manifest, with the `mimetype` entry stored first.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::cellstyle::{font_face, number, write_cell_style};
use super::cm_from_characters as cm_per_width;
use super::numstyle::write_styles;
use super::write_features::{
    calcext_xml, dxf_style_xml, formula_body, map_condition, names_xml, print_ranges, range_address,
    rich_runs, settings_xml, validation_xml,
};
use super::MIME;
use crate::coord::{CellRef, Range};
use crate::model::{Cell, ColumnInfo, Hyperlink, LinkTarget, Sheet, Workbook};
use crate::numfmt::Format;
use crate::value::Value;
use crate::xlsx::escape;
use crate::zip::Entry;

/// The most cells a validation or conditional format is expanded over when its range is very large.
const MOST_EXTRA_CELLS: u64 = 200_000;

const NAMESPACES: &str = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\" xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\"";

/// Write a workbook as an `.ods` file.
pub fn write(book: &Workbook) -> Result<Vec<u8>, String> {
    let mut w = Writer::new(book);
    w.register_dxf_styles();
    let mut tables = String::new();
    for index in 0..book.sheets.len() {
        let table = w.sheet(index);
        tables.push_str(&table);
    }
    let content = w.content_xml(&tables);
    let styles = w.styles_xml();
    let entries = vec![
        Entry { name: "mimetype".to_owned(), data: MIME.as_bytes().to_vec() },
        Entry { name: "META-INF/manifest.xml".to_owned(), data: manifest().into_bytes() },
        Entry { name: "content.xml".to_owned(), data: content.into_bytes() },
        Entry { name: "styles.xml".to_owned(), data: styles.into_bytes() },
        Entry { name: "settings.xml".to_owned(), data: settings_xml(book).into_bytes() },
        Entry { name: "meta.xml".to_owned(), data: meta().into_bytes() },
    ];
    crate::zip::write_with_stored(&entries, &["mimetype"])
}

fn manifest() -> String {
    let entry = |path: &str, kind: &str| {
        format!("<manifest:file-entry manifest:full-path=\"{path}\" manifest:media-type=\"{kind}\"/>")
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">{}{}{}{}{}</manifest:manifest>",
        entry("/", MIME),
        entry("content.xml", "text/xml"),
        entry("styles.xml", "text/xml"),
        entry("settings.xml", "text/xml"),
        entry("meta.xml", "text/xml"),
    )
}

fn meta() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" office:version=\"1.3\"><office:meta><meta:generator>Unluminous {}</meta:generator></office:meta></office:document-meta>",
        env!("CARGO_PKG_VERSION")
    )
}

/// What a cell position carries besides its own content.
#[derive(Default, Clone)]
struct Extra {
    formats: Vec<usize>,
    validation: Option<String>,
}

/// The writer's accumulated styles and definitions.
struct Writer<'b> {
    book: &'b Workbook,
    data_names: Vec<(String, String)>,
    automatic: String,
    cell_names: HashMap<(u32, Vec<usize>, usize), String>,
    column_names: HashMap<String, String>,
    row_names: HashMap<String, String>,
    text_names: HashMap<String, String>,
    dxf_names: HashMap<u32, String>,
    faces: BTreeSet<String>,
    validation_defs: Vec<(String, String)>,
    counter: usize,
}

impl<'b> Writer<'b> {
    fn new(book: &'b Workbook) -> Self {
        let mut w = Self {
            book,
            data_names: Vec::new(),
            automatic: String::new(),
            cell_names: HashMap::new(),
            column_names: HashMap::new(),
            row_names: HashMap::new(),
            text_names: HashMap::new(),
            dxf_names: HashMap::new(),
            faces: BTreeSet::new(),
            validation_defs: Vec::new(),
            counter: 0,
        };
        w.faces.insert(w.book.styles.fonts.first().map(|f| f.name.clone()).unwrap_or_default());
        w
    }

    /// A fresh number for naming a style.
    fn next(&mut self) -> usize {
        self.counter += 1;
        self.counter
    }

    /// The name of the data style for a format code, written the first time it is needed.
    fn data_name(&mut self, code: &str) -> String {
        if code == "General" {
            return "N0".to_owned();
        }
        if let Some((_, name)) = self.data_names.iter().find(|(c, _)| c == code) {
            return name.clone();
        }
        let name = format!("N{}", self.data_names.len() + 1);
        self.automatic.push_str(&write_styles(code, &name));
        self.data_names.push((code.to_owned(), name.clone()));
        name
    }

    /// The cell styles conditional formats apply, named `cf` and the differential format's index.
    fn register_dxf_styles(&mut self) {
        let mut used: BTreeSet<u32> = BTreeSet::new();
        for sheet in &self.book.sheets {
            for format in &sheet.conditional {
                used.extend(format.rules.iter().filter_map(|r| r.dxf));
            }
        }
        for index in used {
            let Some(dxf) = self.book.styles.dxfs.get(index as usize) else { continue };
            let name = format!("cf{}", index + 1);
            let data = dxf.num_fmt.clone().map(|c| self.data_name(&c));
            self.automatic.push_str(&dxf_style_xml(dxf, &name, data.as_deref()));
            self.dxf_names.insert(index, name);
        }
    }

    /// The name of the automatic cell style for a cell format, with the conditional formats over it.
    fn cell_style(&mut self, xf: u32, formats: &[usize], sheet: usize) -> String {
        let key = (xf, formats.to_vec(), sheet);
        if let Some(name) = self.cell_names.get(&key) {
            return name.clone();
        }
        let styles = &self.book.styles;
        let format = styles.xfs.get(xf as usize).cloned().unwrap_or_default();
        let code = styles.format_code(xf);
        let font = styles.fonts.get(format.font as usize).map(|f| f.name.clone());
        self.faces.extend(font);
        let name = format!("ce{}", self.next());
        let data = self.data_name(&code);
        let mut xml = write_cell_style(&format, &self.book.styles, &name, &data);
        let maps = self.maps(formats, sheet);
        if !maps.is_empty() {
            xml.truncate(xml.len() - "</style:style>".len());
            xml.push_str(&maps);
            xml.push_str("</style:style>");
        }
        self.automatic.push_str(&xml);
        self.cell_names.insert(key, name.clone());
        name
    }

    /// The `style:map` elements for the conditional formats over a cell.
    fn maps(&self, formats: &[usize], sheet: usize) -> String {
        let s = &self.book.sheets[sheet];
        let mut xml = String::new();
        for &index in formats {
            let format = &s.conditional[index];
            let Some(range) = format.ranges.first() else { continue };
            let mut rules: Vec<&crate::model::CfRule> = format.rules.iter().collect();
            rules.sort_by_key(|r| r.priority);
            for rule in rules {
                let (Some(dxf), Some(condition)) = (rule.dxf, map_condition(rule, *range, &s.name)) else { continue };
                let Some(style) = self.dxf_names.get(&dxf) else { continue };
                xml.push_str(&format!(
                    "<style:map style:condition=\"{}\" style:apply-style-name=\"{style}\" style:base-cell-address=\"{}\"/>",
                    escape(&condition),
                    range_address(&s.name, Range::cell(range.first)),
                ));
            }
        }
        xml
    }

    /// The name of a column style for a width in centimetres.
    fn column_style(&mut self, width: f64, optimal: bool) -> String {
        let key = format!("{}|{optimal}", number(width));
        if let Some(name) = self.column_names.get(&key) {
            return name.clone();
        }
        let name = format!("co{}", self.next());
        let flag = if optimal { " style:use-optimal-column-width=\"true\"" } else { "" };
        self.automatic.push_str(&format!(
            "<style:style style:name=\"{name}\" style:family=\"table-column\"><style:table-column-properties fo:break-before=\"auto\" style:column-width=\"{}cm\"{flag}/></style:style>",
            number(width)
        ));
        self.column_names.insert(key, name.clone());
        name
    }

    /// The name of a row style for a height in points.
    fn row_style(&mut self, height: f64, optimal: bool) -> String {
        let key = format!("{}|{optimal}", number(height));
        if let Some(name) = self.row_names.get(&key) {
            return name.clone();
        }
        let name = format!("ro{}", self.next());
        self.automatic.push_str(&format!(
            "<style:style style:name=\"{name}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}pt\" style:use-optimal-row-height=\"{optimal}\" fo:break-before=\"auto\"/></style:style>",
            number(height)
        ));
        self.row_names.insert(key, name.clone());
        name
    }

    /// The name of a text style for a run's properties.
    fn text_style(&mut self, props: &str) -> String {
        if let Some(name) = self.text_names.get(props) {
            return name.clone();
        }
        let name = format!("T{}", self.next());
        self.automatic.push_str(&format!(
            "<style:style style:name=\"{name}\" style:family=\"text\"><style:text-properties{props}/></style:style>"
        ));
        self.text_names.insert(props.to_owned(), name.clone());
        name
    }

    /// The name of a validation definition, added the first time its text is met.
    fn validation(&mut self, v: &crate::model::Validation, sheet: &str) -> String {
        let probe = validation_xml(v, "", sheet);
        if let Some((name, _)) = self.validation_defs.iter().find(|(_, x)| *x == probe) {
            return name.clone();
        }
        let name = format!("val{}", self.validation_defs.len() + 1);
        self.validation_defs.push((name.clone(), probe));
        name
    }

    // -----------------------------------------------------------------------------------------------
    // Sheets.

    /// One sheet as a `table:table` element.
    fn sheet(&mut self, index: usize) -> String {
        let book = self.book;
        let sheet = &book.sheets[index];
        let table_style = self.table_style(sheet);
        let mut xml = format!(
            "<table:table table:name=\"{}\" table:style-name=\"{table_style}\"",
            escape(&sheet.name)
        );
        if let Some(areas) = book
            .names
            .iter()
            .find(|n| n.scope == Some(index) && n.name == "_xlnm.Print_Area")
            .and_then(|n| print_ranges(&n.formula, book))
        {
            xml.push_str(&format!(" table:print-ranges=\"{}\"", escape(&areas)));
        }
        xml.push('>');
        let extras = self.extras(index);
        let (rows, cols) = extent(sheet, &extras);
        if let Some(block) = self.conditional_block(index) {
            xml.push_str(&block);
        }
        xml.push_str(&self.columns(sheet, cols));
        xml.push_str(&self.rows(index, &extras, rows, cols));
        let scoped: Vec<&crate::model::DefinedName> = book.names.iter().filter(|n| n.scope == Some(index)).collect();
        xml.push_str(&names_xml(&scoped, book, &sheet.name));
        xml.push_str("</table:table>");
        xml
    }

    fn table_style(&mut self, sheet: &Sheet) -> String {
        let display = sheet.state == crate::model::Visibility::Visible;
        let name = format!("ta{}", self.next());
        let tab = match sheet.tab_color {
            Some(crate::styles::Color::Rgb(argb)) => format!(" tableooo:tab-color=\"#{:06X}\"", argb & 0x00FF_FFFF),
            _ => String::new(),
        };
        let namespace = if tab.is_empty() { "" } else { " xmlns:tableooo=\"http://openoffice.org/2009/table\"" };
        self.automatic.push_str(&format!(
            "<style:style style:name=\"{name}\" style:family=\"table\"><style:table-properties table:display=\"{display}\" style:writing-mode=\"lr-tb\"{tab}{namespace}/></style:style>"
        ));
        name
    }

    /// The conditional formats and validations over each position of a sheet.
    fn extras(&mut self, index: usize) -> BTreeMap<(u32, u32), Extra> {
        let book = self.book;
        let sheet = &book.sheets[index];
        let used = sheet.used_range();
        let mut extras: BTreeMap<(u32, u32), Extra> = BTreeMap::new();
        for (i, format) in sheet.conditional.iter().enumerate() {
            if format.rules.iter().all(|r| r.dxf.is_none()) {
                continue;
            }
            for range in &format.ranges {
                for at in clamped(*range, used).cells() {
                    extras.entry((at.row, at.col)).or_default().formats.push(i);
                }
            }
        }
        for validation in &sheet.validations {
            let name = self.validation(validation, &sheet.name);
            for range in &validation.ranges {
                for at in clamped(*range, used).cells() {
                    extras.entry((at.row, at.col)).or_default().validation = Some(name.clone());
                }
            }
        }
        extras
    }

    fn conditional_block(&self, index: usize) -> Option<String> {
        let sheet = &self.book.sheets[index];
        let xml = calcext_xml(&sheet.conditional, &sheet.name, &self.dxf_names);
        (!xml.is_empty()).then_some(xml)
    }

    /// The `table:table-column` elements.
    fn columns(&mut self, sheet: &Sheet, cols: u32) -> String {
        let pixels = super::pixels_per_character(self.book);
        let default_width = sheet.default_col_width.unwrap_or(super::default_column_pixels(self.book) / pixels);
        let mut specs: Vec<(String, bool, u32)> = Vec::new();
        for c in 0..cols {
            let info: Option<&ColumnInfo> = sheet.column(c);
            let (width, hidden, style) = match info {
                Some(i) => (i.width.unwrap_or(default_width), i.hidden, i.style),
                None => (default_width, false, 0),
            };
            let optimal = info.is_none_or(|i| !i.custom_width);
            let name = self.column_style(cm_per_width(width, pixels), optimal);
            specs.push((name, hidden, style));
        }
        let mut xml = String::new();
        let mut i = 0;
        while i < specs.len() {
            let mut j = i;
            while j + 1 < specs.len() && specs[j + 1] == specs[i] {
                j += 1;
            }
            let (name, hidden, style) = specs[i].clone();
            let default = self.cell_style(style, &[], 0);
            xml.push_str(&format!("<table:table-column table:style-name=\"{name}\""));
            if hidden {
                xml.push_str(" table:visibility=\"collapse\"");
            }
            if j > i {
                xml.push_str(&format!(" table:number-columns-repeated=\"{}\"", j - i + 1));
            }
            xml.push_str(&format!(" table:default-cell-style-name=\"{default}\"/>"));
            i = j + 1;
        }
        // The rest of the sheet's columns have the default width.
        let name = self.column_style(cm_per_width(default_width, pixels), true);
        let default = self.cell_style(0, &[], 0);
        let rest = crate::coord::MAX_COLS.saturating_sub(cols);
        if rest > 0 {
            xml.push_str(&format!(
                "<table:table-column table:style-name=\"{name}\" table:number-columns-repeated=\"{rest}\" table:default-cell-style-name=\"{default}\"/>"
            ));
        }
        xml
    }

    /// The `table:table-row` elements with their cells.
    fn rows(&mut self, index: usize, extras: &BTreeMap<(u32, u32), Extra>, rows: u32, cols: u32) -> String {
        let book = self.book;
        let sheet = &book.sheets[index];
        let covered = covered_cells(sheet);
        let links = link_map(sheet);
        let mut xml = String::new();
        let mut gap = 0u32;
        let default_style = self.row_style(sheet.default_row_height, true);
        for r in 0..rows {
            let info = sheet.rows.get(&r);
            let has_cells = sheet.cells.range((r, 0)..=(r, u32::MAX)).next().is_some()
                || sheet.spills.values.range((r, 0)..=(r, u32::MAX)).next().is_some()
                || extras.range((r, 0)..=(r, u32::MAX)).next().is_some()
                || covered.range((r, 0)..=(r, u32::MAX)).next().is_some();
            if !has_cells && info.is_none() {
                gap += 1;
                continue;
            }
            if gap > 0 {
                xml.push_str(&empty_rows(&default_style, gap, cols));
                gap = 0;
            }
            let (height, hidden) = info.map_or((None, false), |i| (i.height, i.hidden));
            let style = self.row_style(height.unwrap_or(sheet.default_row_height), height.is_none());
            xml.push_str(&format!("<table:table-row table:style-name=\"{style}\""));
            if hidden {
                xml.push_str(" table:visibility=\"collapse\"");
            }
            xml.push('>');
            xml.push_str(&self.row_cells(index, r, extras, &covered, &links));
            xml.push_str("</table:table-row>");
        }
        // The rest of the sheet's rows are empty, as Excel writes them.
        let rest = crate::coord::MAX_ROWS.saturating_sub(rows);
        if rest > 0 {
            xml.push_str(&empty_rows(&default_style, rest, crate::coord::MAX_COLS));
        }
        xml
    }

    fn row_cells(
        &mut self,
        index: usize,
        r: u32,
        extras: &BTreeMap<(u32, u32), Extra>,
        covered: &BTreeMap<(u32, u32), ()>,
        links: &HashMap<(u32, u32), &Hyperlink>,
    ) -> String {
        let book = self.book;
        let sheet = &book.sheets[index];
        let mut at: BTreeSet<u32> = BTreeSet::new();
        at.extend(sheet.cells.range((r, 0)..=(r, u32::MAX)).map(|(&(_, c), _)| c));
        at.extend(sheet.spills.values.range((r, 0)..=(r, u32::MAX)).map(|(&(_, c), _)| c));
        at.extend(extras.range((r, 0)..=(r, u32::MAX)).map(|(&(_, c), _)| c));
        at.extend(covered.range((r, 0)..=(r, u32::MAX)).map(|(&(_, c), _)| c));
        let mut xml = String::new();
        let mut next = 0u32;
        for c in at {
            if c > next {
                xml.push_str(&repeated_empty(c - next));
            }
            if covered.contains_key(&(r, c)) {
                xml.push_str("<table:covered-table-cell/>");
            } else {
                let extra = extras.get(&(r, c)).cloned().unwrap_or_default();
                xml.push_str(&self.cell(index, CellRef::new(r, c), &extra, links.get(&(r, c)).copied()));
            }
            next = c + 1;
        }
        xml
    }

    /// One cell element.
    fn cell(&mut self, index: usize, at: CellRef, extra: &Extra, link: Option<&Hyperlink>) -> String {
        let book = self.book;
        let sheet = &book.sheets[index];
        let spilled;
        let cell: &Cell = match sheet.cells.get(&(at.row, at.col)) {
            Some(cell) => cell,
            None => {
                spilled = Cell::value(sheet.spills.values.get(&(at.row, at.col)).cloned().unwrap_or_default());
                &spilled
            }
        };
        let style = self.cell_style(cell.style, &extra.formats, index);
        let code = book.styles.format_code(cell.style);
        let mut xml = format!("<table:table-cell table:style-name=\"{style}\"");
        if let Some(v) = &extra.validation {
            xml.push_str(&format!(" table:content-validation-name=\"{v}\""));
        }
        if let Some(f) = &cell.formula {
            let body = formula_body(&f.expr, &f.text, f.frozen, book, index, at);
            xml.push_str(&format!(" table:formula=\"of:={}\"", escape(&body)));
            if let Some(range) = f.array {
                xml.push_str(&format!(
                    " table:number-matrix-columns-spanned=\"{}\" table:number-matrix-rows-spanned=\"{}\"",
                    range.cols(),
                    range.rows()
                ));
            }
        }
        if let Some(range) = sheet.spills.range_of((at.row, at.col)) {
            if cell.formula.is_some() && range.len() > 1 && !xml.contains("matrix") {
                xml.push_str(&format!(
                    " table:number-matrix-columns-spanned=\"{}\" table:number-matrix-rows-spanned=\"{}\"",
                    range.cols(),
                    range.rows()
                ));
            }
        }
        if let Some(merge) = sheet.merges.iter().find(|m| m.first == at) {
            if merge.cols() > 1 {
                xml.push_str(&format!(" table:number-columns-spanned=\"{}\"", merge.cols()));
            }
            if merge.rows() > 1 {
                xml.push_str(&format!(" table:number-rows-spanned=\"{}\"", merge.rows()));
            }
        }
        let shown = Format::parse(&code).render(&cell.value, book.date1904, 255).text;
        xml.push_str(&value_attributes(&cell.value, &code, book.date1904, cell.formula.is_some()));
        let paragraphs = self.paragraphs(cell, &shown, link);
        let note = sheet.notes.get(&(at.row, at.col)).map(note_xml).unwrap_or_default();
        if paragraphs.is_empty() && note.is_empty() {
            xml.push_str("/>");
        } else {
            xml.push_str(&format!(">{note}{paragraphs}</table:table-cell>"));
        }
        xml
    }

    /// The text paragraphs of a cell: its shown text, rich runs and link.
    fn paragraphs(&mut self, cell: &Cell, shown: &str, link: Option<&Hyperlink>) -> String {
        if cell.value.is_empty() && cell.formula.is_none() {
            return String::new();
        }
        // A reader takes a plain text cell's value from its paragraphs, so they hold the text itself and
        // not what a format with text in it would show.
        let shown = match (&cell.value, cell.formula.is_some()) {
            (Value::Text(text), false) => &**text,
            _ => shown,
        };
        if let Some(link) = link {
            return format!("<text:p>{}</text:p>", self.link_xml(link, shown));
        }
        if let (Some(raw), Value::Text(text)) = (&cell.rich, &cell.value) {
            let runs = rich_runs(raw);
            let joined: String = runs.iter().map(|(t, _)| t.as_str()).collect();
            if joined == **text {
                let mut body = String::new();
                for (run, props) in runs {
                    if props.is_empty() {
                        body.push_str(&inline(&run));
                    } else {
                        let style = self.text_style(&props);
                        body.push_str(&format!("<text:span text:style-name=\"{style}\">{}</text:span>", inline(&run)));
                    }
                }
                return format!("<text:p>{body}</text:p>");
            }
        }
        shown.split('\n').map(|line| format!("<text:p>{}</text:p>", inline(line))).collect()
    }

    fn link_xml(&self, link: &Hyperlink, shown: &str) -> String {
        let href = match &link.target {
            LinkTarget::External(url) => url.clone(),
            LinkTarget::Location(place) => format!("#{}", location_address(place)),
        };
        format!("<text:a xlink:href=\"{}\" xlink:type=\"simple\">{}</text:a>", escape(&href), inline(shown))
    }

    // -----------------------------------------------------------------------------------------------
    // Documents.

    /// `content.xml`.
    fn content_xml(&mut self, tables: &str) -> String {
        let book = self.book;
        let mut faces: BTreeSet<String> = self.faces.clone();
        faces.extend(book.styles.fonts.iter().map(|f| f.name.clone()));
        let faces: String = faces.iter().filter(|f| !f.is_empty()).map(|f| font_face(f)).collect();
        let validations = if self.validation_defs.is_empty() {
            String::new()
        } else {
            let items: String = self.validation_defs.iter().map(|(name, xml)| xml.replacen("table:name=\"\"", &format!("table:name=\"{name}\""), 1)).collect();
            format!("<table:content-validations>{items}</table:content-validations>")
        };
        let null_date = if book.date1904 { "<table:null-date table:date-value=\"1904-01-01\"/>" } else { "" };
        let global: Vec<&crate::model::DefinedName> = book.names.iter().filter(|n| n.scope.is_none()).collect();
        let names = names_xml(&global, book, book.sheets.first().map_or("Sheet1", |s| s.name.as_str()));
        let filters = database_ranges(book);
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NAMESPACES} office:version=\"1.3\"><office:font-face-decls>{faces}</office:font-face-decls><office:automatic-styles>{}</office:automatic-styles><office:body><office:spreadsheet><table:calculation-settings table:case-sensitive=\"false\" table:search-criteria-must-apply-to-whole-cell=\"true\" table:use-wildcards=\"true\" table:use-regular-expressions=\"false\" table:automatic-find-labels=\"false\">{null_date}</table:calculation-settings>{validations}{tables}{names}{filters}</office:spreadsheet></office:body></office:document-content>",
            self.automatic
        )
    }

    /// `styles.xml`: the default cell style and the General number format.
    fn styles_xml(&self) -> String {
        let font = self.book.styles.fonts.first().cloned().unwrap_or_default();
        let theme = &self.book.styles.theme;
        let color = font
            .color
            .resolve(theme)
            .map_or("#000000".to_owned(), |(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NAMESPACES} office:version=\"1.3\"><office:font-face-decls>{}</office:font-face-decls><office:styles>{}<style:style style:name=\"Default\" style:family=\"table-cell\" style:data-style-name=\"N0\"><style:table-cell-properties style:vertical-align=\"automatic\" fo:background-color=\"transparent\"/><style:text-properties fo:color=\"{color}\" style:font-name=\"{}\" fo:font-size=\"{}pt\"/></style:style></office:styles><office:automatic-styles/><office:master-styles/></office:document-styles>",
            font_face(&font.name),
            write_styles("General", "N0"),
            escape(&font.name),
            number(font.size)
        )
    }
}

/// The rows and columns a sheet's content reaches.
fn extent(sheet: &Sheet, extras: &BTreeMap<(u32, u32), Extra>) -> (u32, u32) {
    let (mut rows, mut cols) = (0u32, 0u32);
    let mut bump = |r: u32, c: u32| {
        rows = rows.max(r + 1);
        cols = cols.max(c + 1);
    };
    for &(r, c) in sheet.cells.keys().chain(sheet.spills.values.keys()).chain(extras.keys()) {
        bump(r, c);
    }
    for m in &sheet.merges {
        bump(m.last.row, m.last.col);
    }
    for &(r, c) in sheet.notes.keys() {
        bump(r, c);
    }
    if let Some(&last) = sheet.rows.keys().next_back() {
        rows = rows.max(last + 1);
    }
    for info in &sheet.columns {
        cols = cols.max((info.last + 1).min(crate::coord::MAX_COLS));
    }
    (rows, cols)
}

/// A range cut down to the cells worth expanding: large ranges stop at the used range plus a margin.
fn clamped(range: Range, used: Option<Range>) -> Range {
    if range.len() <= MOST_EXTRA_CELLS {
        return range;
    }
    let (last_row, last_col) = used.map_or((1000, 64), |u| (u.last.row + 1000, u.last.col + 64));
    Range::new(range.first, CellRef::new(range.last.row.min(last_row), range.last.col.min(last_col)))
}

/// The cells under the top left cell of each merge.
fn covered_cells(sheet: &Sheet) -> BTreeMap<(u32, u32), ()> {
    let mut covered = BTreeMap::new();
    for merge in &sheet.merges {
        for at in merge.cells().filter(|at| *at != merge.first).take(100_000) {
            covered.insert((at.row, at.col), ());
        }
    }
    covered
}

/// Each linked cell's link.
fn link_map(sheet: &Sheet) -> HashMap<(u32, u32), &Hyperlink> {
    let mut map = HashMap::new();
    for link in &sheet.links {
        for at in link.range.cells().take(10_000) {
            map.insert((at.row, at.col), link);
        }
    }
    map
}

/// Rows with nothing in them, repeated.
fn empty_rows(style: &str, count: u32, cols: u32) -> String {
    let repeat = if count > 1 { format!(" table:number-rows-repeated=\"{count}\"") } else { String::new() };
    format!("<table:table-row table:style-name=\"{style}\"{repeat}>{}</table:table-row>", repeated_empty(cols.max(1)))
}

/// Cells with nothing in them, repeated.
fn repeated_empty(count: u32) -> String {
    if count == 1 {
        "<table:table-cell/>".to_owned()
    } else {
        format!("<table:table-cell table:number-columns-repeated=\"{count}\"/>")
    }
}

/// A place in the workbook as an `.ods` link writes it: `Sheet2!A1` as `'Sheet2'.A1`.
fn location_address(place: &str) -> String {
    match place.rsplit_once('!') {
        Some((sheet, cell)) => {
            let sheet = sheet.trim_matches('\'').replace("''", "'");
            format!("'{}'.{}", sheet.replace('\'', "''"), cell.replace('$', ""))
        }
        None => place.to_owned(),
    }
}

/// A note as an annotation.
fn note_xml(note: &crate::model::Note) -> String {
    let paragraphs: String = note.text.split('\n').map(|p| format!("<text:p>{}</text:p>", inline(p))).collect();
    format!("<office:annotation><dc:creator>{}</dc:creator>{paragraphs}</office:annotation>", escape(&note.author))
}

/// The filter buttons of a workbook as database ranges.
fn database_ranges(book: &Workbook) -> String {
    let items: String = book
        .sheets
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.filter.as_ref().map(|f| (i, s, f)))
        .map(|(i, s, f)| {
            format!(
                "<table:database-range table:name=\"__Anonymous_Sheet_DB__{i}\" table:target-range-address=\"{}\" table:display-filter-buttons=\"true\"/>",
                range_address(&s.name, f.range)
            )
        })
        .collect();
    if items.is_empty() {
        items
    } else {
        format!("<table:database-ranges>{items}</table:database-ranges>")
    }
}

/// Text inside a paragraph: spaces, tabs and the escapes XML needs.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' => {
                let run = chars[i..].iter().take_while(|c| **c == ' ').count();
                if i == 0 || run > 1 {
                    let keep = usize::from(i != 0);
                    if keep == 1 {
                        out.push(' ');
                    }
                    out.push_str(&format!("<text:s text:c=\"{}\"/>", run - keep));
                } else {
                    out.push(' ');
                }
                i += run;
            }
            '\t' => {
                out.push_str("<text:tab/>");
                i += 1;
            }
            c => {
                out.push_str(&escape(&c.to_string()));
                i += 1;
            }
        }
    }
    out
}

/// The type and value attributes of a cell.
fn value_attributes(value: &Value, code: &str, date1904: bool, formula: bool) -> String {
    match value {
        Value::Empty => String::new(),
        Value::Number(n) => number_attributes(*n, code, date1904),
        Value::Bool(b) => format!(" office:value-type=\"boolean\" office:boolean-value=\"{b}\""),
        Value::Error(e) => format!(" office:value-type=\"error\" office:string-value=\"{}\"", e.text()),
        Value::Text(t) => {
            if formula {
                format!(" office:value-type=\"string\" office:string-value=\"{}\"", escape(t))
            } else {
                " office:value-type=\"string\"".to_owned()
            }
        }
    }
}

/// A number as a float, a percentage, a date or a time, by its format.
fn number_attributes(n: f64, code: &str, date1904: bool) -> String {
    let plain = outside_quotes(code);
    let lower = plain.to_ascii_lowercase();
    let has_date = lower.contains('y') || lower.contains('d') || lower.contains("mmm");
    let has_time = lower.contains('h') || lower.contains('s');
    if (has_date || has_time) && n >= 0.0 && n.is_finite() && Format::parse(code).is_date() {
        if has_date {
            if let Some((y, m, d)) = crate::dates::date(n.floor(), date1904) {
                let (h, mi, s, ms) = crate::dates::time(n);
                let fraction = if ms > 0 { format!(".{ms:03}") } else { String::new() };
                return format!(
                    " office:value-type=\"date\" office:date-value=\"{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}{fraction}\""
                );
            }
        } else {
            let total = (n * 86400.0 * 1000.0).round() / 1000.0;
            let (h, rest) = ((total / 3600.0).floor(), total % 3600.0);
            let (m, s) = ((rest / 60.0).floor(), rest % 60.0);
            return format!(" office:value-type=\"time\" office:time-value=\"PT{h:02}H{m:02}M{}S\"", seconds_text(s));
        }
    }
    let kind = if lower.contains('%') { "percentage" } else { "float" };
    format!(" office:value-type=\"{kind}\" office:value=\"{}\"", n)
}

/// Seconds as two digits and any fraction.
fn seconds_text(s: f64) -> String {
    if s.fract() == 0.0 {
        format!("{:02}", s as u32)
    } else {
        let text = format!("{s:06.3}");
        text.trim_end_matches('0').to_owned()
    }
}

/// A format code without the text in quotes, brackets and escapes.
fn outside_quotes(code: &str) -> String {
    let mut out = String::new();
    let (mut quoted, mut bracketed, mut escaped) = (false, false, false);
    for c in code.chars() {
        if escaped {
            escaped = false;
        } else if c == '\\' || c == '_' || c == '*' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == '[' {
            bracketed = true;
        } else if c == ']' {
            bracketed = false;
        } else if !quoted && !bracketed {
            out.push(c);
        }
    }
    out
}
