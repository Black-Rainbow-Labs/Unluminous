//! Writing an `.xlsx` file.
//!
//! The workbook part, every worksheet, the styles, the shared strings, the dynamic array metadata, the
//! tables and changed notes are generated; every other part is copied as it was read. Worksheet and
//! workbook elements are written in schema order, from the model or from the XML kept for them, which is
//! what stops Excel calling the file damaged.

use std::collections::{BTreeSet, HashMap};

use super::*;
use crate::coord::{column_name, CellRef, Range};
use crate::formula::print::print;
use crate::model::*;
use crate::styles::*;
use crate::value::Value;
use crate::zip::Entry;

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// The workbook as the bytes of an `.xlsx` file. Its formulas' values should already be computed.
pub fn write(book: &Workbook) -> Result<Vec<u8>, String> {
    let mut w = Writer::new(book);
    w.run()?;
    crate::zip::write(&w.entries)
}

struct Writer<'a> {
    book: &'a Workbook,
    entries: Vec<Entry>,
    overrides: Vec<(String, String)>,
    strings: Vec<String>,
    string_index: HashMap<String, usize>,
    rich_strings: HashMap<String, usize>,
    taken: BTreeSet<String>,
    workbook_part: String,
    needs_metadata: bool,
}

impl<'a> Writer<'a> {
    fn new(book: &'a Workbook) -> Self {
        let mut taken: BTreeSet<String> = book.kept.parts.iter().map(|(p, _)| p.clone()).collect();
        let workbook_part =
            if book.part.is_empty() { "xl/workbook.xml".to_owned() } else { book.part.clone() };
        taken.insert(workbook_part.clone());
        Self {
            book,
            entries: Vec::new(),
            overrides: Vec::new(),
            strings: Vec::new(),
            string_index: HashMap::new(),
            rich_strings: HashMap::new(),
            taken,
            workbook_part,
            needs_metadata: false,
        }
    }

    fn add(&mut self, path: &str, content_type: Option<&str>, data: Vec<u8>) {
        if let Some(ct) = content_type {
            self.overrides.push((format!("/{path}"), ct.to_owned()));
        }
        self.taken.insert(path.to_owned());
        self.entries.push(Entry { name: path.to_owned(), data });
    }

    /// A part path not in use, numbered from 1: `xl/worksheets/sheet{}.xml`.
    fn fresh(&self, pattern: &str) -> String {
        (1..)
            .map(|n| pattern.replace("{}", &n.to_string()))
            .find(|p| !self.taken.contains(p))
            .unwrap_or_default()
    }

    fn string(&mut self, text: &str, rich: Option<&str>) -> usize {
        if let Some(rich) = rich {
            if let Some(i) = self.rich_strings.get(rich) {
                return *i;
            }
            let si = rich_si(rich);
            self.strings.push(si.clone());
            let i = self.strings.len() - 1;
            self.rich_strings.insert(rich.to_owned(), i);
            return i;
        }
        if let Some(i) = self.string_index.get(text) {
            return *i;
        }
        let preserve = text.starts_with(char::is_whitespace)
            || text.ends_with(char::is_whitespace)
            || text.contains('\n');
        self.strings.push(format!(
            "<si><t{}>{}</t></si>",
            if preserve { " xml:space=\"preserve\"" } else { "" },
            escape(text)
        ));
        let i = self.strings.len() - 1;
        self.string_index.insert(text.to_owned(), i);
        i
    }

    fn run(&mut self) -> Result<(), String> {
        let book = self.book;
        // Sheet parts first, so the workbook knows their relationship ids and paths.
        let mut sheet_parts: Vec<(String, String, String)> = Vec::new(); // (rel id, part, rel type)
        let mut workbook_rels: Vec<Relationship> = book.kept.workbook_rels.clone();
        if book.persons_changed {
            workbook_rels.retain(|r| r.kind != super::threads::REL_PERSON);
        }
        for (index, sheet) in book.sheets.iter().enumerate() {
            if let Some(kept) = &sheet.kept_part {
                let id = next_id(&workbook_rels);
                workbook_rels.push(Relationship {
                    id: id.clone(),
                    kind: kept.kind.clone(),
                    target: relative(&self.workbook_part, &kept.part),
                    external: false,
                });
                if !kept.content_type.is_empty() {
                    self.overrides.push((format!("/{}", kept.part), kept.content_type.clone()));
                }
                sheet_parts.push((id, kept.part.clone(), kept.kind.clone()));
                continue;
            }
            let part = if sheet.raw.part.is_empty() {
                self.fresh("xl/worksheets/sheet{}.xml")
            } else {
                sheet.raw.part.clone()
            };
            self.taken.insert(part.clone());
            let id = next_id(&workbook_rels);
            workbook_rels.push(Relationship {
                id: id.clone(),
                kind: REL_WORKSHEET.to_owned(),
                target: relative(&self.workbook_part, &part),
                external: false,
            });
            sheet_parts.push((id, part.clone(), REL_WORKSHEET.to_owned()));
            let (xml, rels) = self.sheet_xml(index, sheet, &part)?;
            self.add(&part, Some(CT_WORKSHEET), xml.into_bytes());
            if !rels.is_empty() {
                self.add(&rels_path(&part), None, write_rels(&rels).into_bytes());
            }
        }
        if book.persons_changed && !book.persons.is_empty() {
            let part = self.fresh("xl/persons/person{}.xml");
            let id = next_id(&workbook_rels);
            workbook_rels.push(Relationship {
                id,
                kind: super::threads::REL_PERSON.to_owned(),
                target: relative(&self.workbook_part, &part),
                external: false,
            });
            let xml = super::threads::persons_xml(&book.persons);
            self.add(&part, Some(super::threads::CT_PERSON), xml.into_bytes());
        }
        // Styles, strings, theme, metadata.
        let styles_part = "xl/styles.xml".to_owned();
        let id = next_id(&workbook_rels);
        workbook_rels.push(Relationship {
            id,
            kind: REL_STYLES.to_owned(),
            target: relative(&self.workbook_part, &styles_part),
            external: false,
        });
        let styles = styles_xml(&book.styles);
        self.add(&styles_part, Some(CT_STYLES), styles.into_bytes());
        if !self.strings.is_empty() {
            let part = "xl/sharedStrings.xml".to_owned();
            let id = next_id(&workbook_rels);
            workbook_rels.push(Relationship {
                id,
                kind: REL_SHARED_STRINGS.to_owned(),
                target: relative(&self.workbook_part, &part),
                external: false,
            });
            let mut xml = format!(
                "{XML_HEAD}<sst xmlns=\"{MAIN_NS}\" count=\"{0}\" uniqueCount=\"{0}\">",
                self.strings.len()
            );
            for s in &self.strings {
                xml.push_str(s);
            }
            xml.push_str("</sst>");
            self.add(&part, Some(CT_SHARED_STRINGS), xml.into_bytes());
        }
        if !workbook_rels.iter().any(|r| r.kind == REL_THEME) {
            let part = self.fresh("xl/theme/theme{}.xml");
            let id = next_id(&workbook_rels);
            workbook_rels.push(Relationship {
                id,
                kind: REL_THEME.to_owned(),
                target: relative(&self.workbook_part, &part),
                external: false,
            });
            self.add(&part, Some(CT_THEME), DEFAULT_THEME_XML.as_bytes().to_vec());
        }
        if self.needs_metadata && book.dynamic_cm.is_none() {
            let part = self.fresh("xl/metadata.xml");
            let id = next_id(&workbook_rels);
            workbook_rels.push(Relationship {
                id,
                kind: REL_METADATA.to_owned(),
                target: relative(&self.workbook_part, &part),
                external: false,
            });
            self.add(&part, Some(CT_METADATA), METADATA_XML.as_bytes().to_vec());
        }
        // The workbook part.
        let workbook = self.workbook_xml(&sheet_parts);
        let ct = if book.has_macros { CT_WORKBOOK_MACRO } else { CT_WORKBOOK };
        let workbook_part = self.workbook_part.clone();
        self.add(&workbook_part, Some(ct), workbook.into_bytes());
        self.add(&rels_path(&workbook_part), None, write_rels(&workbook_rels).into_bytes());
        // Package relationships and document properties.
        let mut package = vec![Relationship {
            id: "rId1".to_owned(),
            kind: REL_OFFICE_DOCUMENT.to_owned(),
            target: workbook_part.clone(),
            external: false,
        }];
        for r in &book.kept.package_rels {
            let mut r = r.clone();
            if package.iter().any(|p| p.id == r.id) {
                r.id = next_id(&package);
            }
            package.push(r);
        }
        if !package.iter().any(|r| r.kind == REL_CORE) {
            let id = next_id(&package);
            package.push(Relationship {
                id,
                kind: REL_CORE.to_owned(),
                target: "docProps/core.xml".to_owned(),
                external: false,
            });
            self.add("docProps/core.xml", Some(CT_CORE), CORE_XML.as_bytes().to_vec());
        }
        if !package.iter().any(|r| r.kind == REL_APP) {
            let id = next_id(&package);
            package.push(Relationship {
                id,
                kind: REL_APP.to_owned(),
                target: "docProps/app.xml".to_owned(),
                external: false,
            });
            self.add("docProps/app.xml", Some(CT_APP), APP_XML.as_bytes().to_vec());
        }
        self.add("_rels/.rels", None, write_rels(&package).into_bytes());
        // Kept parts.
        for (path, bytes) in &book.kept.parts {
            if self.entries.iter().any(|e| &e.name == path) || self.dropped(path) {
                continue;
            }
            self.entries.push(Entry { name: path.clone(), data: bytes.to_vec() });
        }
        // Content types, first in the file.
        let types = self.content_types();
        self.entries
            .insert(0, Entry { name: "[Content_Types].xml".to_owned(), data: types.into_bytes() });
        // `_rels/.rels` second, which is where Excel puts it.
        if let Some(i) = self.entries.iter().position(|e| e.name == "_rels/.rels") {
            let e = self.entries.remove(i);
            self.entries.insert(1, e);
        }
        Ok(())
    }

    /// Kept parts the writer has replaced: notes that changed take their comments and VML parts with them.
    fn dropped(&self, path: &str) -> bool {
        for sheet in &self.book.sheets {
            if !sheet.notes_changed {
                continue;
            }
            for r in &sheet.raw.rels {
                let replaced = matches!(r.kind.as_str(), REL_COMMENTS | REL_VML)
                    || r.kind == super::threads::REL_THREADED;
                if replaced && resolve(&sheet.raw.part, &r.target) == path {
                    return true;
                }
            }
        }
        self.book.persons_changed
            && self.book.kept.workbook_rels.iter().any(|r| {
                r.kind == super::threads::REL_PERSON
                    && resolve(&self.workbook_part, &r.target) == path
            })
    }

    fn content_types(&self) -> String {
        let mut defaults: Vec<(String, String)> = self.book.kept.defaults.clone();
        for (ext, ct) in [("rels", CT_RELS), ("xml", "application/xml"), ("vml", CT_VML)] {
            if !defaults.iter().any(|(e, _)| e.eq_ignore_ascii_case(ext)) {
                defaults.push((ext.to_owned(), ct.to_owned()));
            }
        }
        let mut overrides: Vec<(String, String)> = self.overrides.clone();
        for (path, ct) in &self.book.kept.overrides {
            let present = self.entries.iter().any(|e| format!("/{}", e.name) == *path);
            if present && !overrides.iter().any(|(p, _)| p == path) {
                overrides.push((path.clone(), ct.clone()));
            }
        }
        let mut out = format!("{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
        for (ext, ct) in defaults {
            out.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                escape(&ext),
                escape(&ct)
            ));
        }
        for (path, ct) in overrides {
            out.push_str(&format!(
                "<Override PartName=\"{}\" ContentType=\"{}\"/>",
                escape(&path),
                escape(&ct)
            ));
        }
        out.push_str("</Types>");
        out
    }

    fn workbook_xml(&self, sheets: &[(String, String, String)]) -> String {
        let book = self.book;
        let raw = |name: &str| {
            book.raw.iter().filter(|(n, _)| n == name).map(|(_, x)| x.as_str()).collect::<String>()
        };
        let mut namespaces = book.namespaces.clone();
        if !namespaces.iter().any(|(_, uri)| uri == REL_NS) {
            namespaces.push(("r".to_owned(), REL_NS.to_owned()));
        }
        let r = namespaces
            .iter()
            .find(|(_, uri)| uri == REL_NS)
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| "r".to_owned());
        let mut out = format!("{XML_HEAD}<workbook{}>", namespaces_text(&namespaces, MAIN_NS));
        out.push_str(&raw("fileVersion"));
        out.push_str(&raw("fileSharing"));
        let mut pr = book.workbook_pr.clone();
        if book.date1904 {
            pr.push(("date1904".to_owned(), "1".to_owned()));
        }
        out.push_str(&format!("<workbookPr{}/>", attrs_text(&pr)));
        // Elements the schema does not name, such as `mc:AlternateContent` and `xr:revisionPtr`, go where
        // Excel writes them: after the workbook properties and before `workbookProtection`. Excel refuses a
        // file with the protection in front of them, and its repair refuses it too.
        const KNOWN: &[&str] = &[
            "fileVersion",
            "fileSharing",
            "workbookProtection",
            "functionGroups",
            "externalReferences",
            "oleSize",
            "customWorkbookViews",
            "pivotCaches",
            "smartTagPr",
            "smartTagTypes",
            "webPublishing",
            "fileRecoveryPr",
            "webPublishObjects",
            "extLst",
        ];
        for (name, xml) in &book.raw {
            if !KNOWN.contains(&name.as_str()) {
                out.push_str(xml);
            }
        }
        out.push_str(&raw("workbookProtection"));
        let mut view = book.view_attrs.clone();
        if book.active_sheet > 0 {
            view.push(("activeTab".to_owned(), book.active_sheet.to_string()));
        }
        out.push_str(&format!("<bookViews><workbookView{}/></bookViews>", attrs_text(&view)));
        out.push_str("<sheets>");
        for (i, (sheet, (id, _, _))) in book.sheets.iter().zip(sheets).enumerate() {
            let state = match sheet.state {
                Visibility::Visible => "",
                Visibility::Hidden => " state=\"hidden\"",
                Visibility::VeryHidden => " state=\"veryHidden\"",
            };
            out.push_str(&format!(
                "<sheet name=\"{}\" sheetId=\"{}\"{state} {r}:id=\"{}\"/>",
                escape(&sheet.name),
                i + 1,
                escape(id)
            ));
        }
        out.push_str("</sheets>");
        out.push_str(&raw("functionGroups"));
        out.push_str(&raw("externalReferences"));
        let names = self.defined_names();
        if !names.is_empty() {
            out.push_str("<definedNames>");
            for n in names {
                let mut attrs = vec![("name".to_owned(), n.name.clone())];
                if let Some(s) = n.scope {
                    attrs.push(("localSheetId".to_owned(), s.to_string()));
                }
                if n.hidden {
                    attrs.push(("hidden".to_owned(), "1".to_owned()));
                }
                attrs.extend(n.attrs.clone());
                out.push_str(&format!(
                    "<definedName{}>{}</definedName>",
                    attrs_text(&attrs),
                    escape(&n.formula)
                ));
            }
            out.push_str("</definedNames>");
        }
        let mut calc = book.calc.attrs.clone();
        if !calc.iter().any(|(k, _)| k == "calcId") {
            calc.insert(0, ("calcId".to_owned(), "191029".to_owned()));
        }
        if !book.calc.automatic {
            calc.push(("calcMode".to_owned(), "manual".to_owned()));
        }
        out.push_str(&format!("<calcPr{}/>", attrs_text(&calc)));
        for name in [
            "oleSize",
            "customWorkbookViews",
            "pivotCaches",
            "smartTagPr",
            "smartTagTypes",
            "webPublishing",
            "fileRecoveryPr",
            "webPublishObjects",
            "extLst",
        ] {
            out.push_str(&raw(name));
        }
        out.push_str("</workbook>");
        out
    }

    /// The names, with a hidden `_xlnm._FilterDatabase` for each sheet that has a filter.
    fn defined_names(&self) -> Vec<DefinedName> {
        let mut names: Vec<DefinedName> =
            self.book.names.iter().filter(|n| n.name != "_xlnm._FilterDatabase").cloned().collect();
        for (i, sheet) in self.book.sheets.iter().enumerate() {
            if let Some(filter) = &sheet.filter {
                let range = filter.range;
                names.push(DefinedName {
                    name: "_xlnm._FilterDatabase".to_owned(),
                    scope: Some(i),
                    formula: format!(
                        "{}!{}",
                        crate::formula::print::sheet_name(&sheet.name),
                        absolute(range)
                    ),
                    hidden: true,
                    attrs: Vec::new(),
                });
            }
        }
        names.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then(a.scope.cmp(&b.scope))
        });
        names
    }

    fn sheet_xml(
        &mut self,
        index: usize,
        sheet: &Sheet,
        part: &str,
    ) -> Result<(String, Vec<Relationship>), String> {
        let book = self.book;
        let raw = |name: &str| {
            sheet
                .raw
                .elements
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, x)| x.as_str())
                .collect::<String>()
        };
        let mut rels: Vec<Relationship> = sheet.raw.rels.clone();
        if sheet.notes_changed {
            rels.retain(|r| {
                !matches!(r.kind.as_str(), REL_COMMENTS | REL_VML)
                    && r.kind != super::threads::REL_THREADED
            });
        }
        let mut namespaces = sheet.raw.namespaces.clone();
        if !namespaces.iter().any(|(_, uri)| uri == REL_NS) {
            namespaces.push(("r".to_owned(), REL_NS.to_owned()));
        }
        let r = namespaces
            .iter()
            .find(|(_, uri)| uri == REL_NS)
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| "r".to_owned());
        let mut out = String::with_capacity(4096 + sheet.cells.len() * 40);
        out.push_str(XML_HEAD);
        out.push_str(&format!("<worksheet{}>", namespaces_text(&namespaces, MAIN_NS)));
        // sheetPr, with the tab colour written into it.
        out.push_str(&sheet_pr(&raw("sheetPr"), sheet.tab_color, &book.styles.theme));
        let dimension = sheet.used_range().map_or("A1".to_owned(), |r| r.a1());
        out.push_str(&format!("<dimension ref=\"{dimension}\"/>"));
        out.push_str(&sheet_views(sheet, index == book.active_sheet));
        let mut format_attrs =
            vec![("defaultRowHeight".to_owned(), number_text(sheet.default_row_height))];
        if let Some(w) = sheet.default_col_width {
            format_attrs.push(("defaultColWidth".to_owned(), number_text(w)));
        }
        format_attrs.extend(sheet.raw.format_attrs.iter().cloned());
        out.push_str(&format!("<sheetFormatPr{}/>", attrs_text(&format_attrs)));
        if !sheet.columns.is_empty() {
            out.push_str("<cols>");
            let mut cols = sheet.columns.clone();
            cols.sort_by_key(|c| c.first);
            for c in cols {
                let mut a = vec![
                    ("min".to_owned(), (c.first + 1).to_string()),
                    ("max".to_owned(), (c.last + 1).to_string()),
                ];
                a.push((
                    "width".to_owned(),
                    number_text(c.width.unwrap_or(sheet.default_col_width.unwrap_or(8.88671875))),
                ));
                if c.style != 0 {
                    a.push(("style".to_owned(), c.style.to_string()));
                }
                if c.hidden {
                    a.push(("hidden".to_owned(), "1".to_owned()));
                }
                if c.best_fit {
                    a.push(("bestFit".to_owned(), "1".to_owned()));
                }
                if c.custom_width || c.width.is_some() {
                    a.push(("customWidth".to_owned(), "1".to_owned()));
                }
                if c.outline > 0 {
                    a.push(("outlineLevel".to_owned(), c.outline.to_string()));
                }
                if c.collapsed {
                    a.push(("collapsed".to_owned(), "1".to_owned()));
                }
                out.push_str(&format!("<col{}/>", attrs_text(&a)));
            }
            out.push_str("</cols>");
        }
        self.sheet_data(sheet, &mut out);
        out.push_str(&raw("sheetCalcPr"));
        out.push_str(&raw("sheetProtection"));
        out.push_str(&raw("protectedRanges"));
        out.push_str(&raw("scenarios"));
        if let Some(filter) = &sheet.filter {
            out.push_str(&auto_filter(filter));
        }
        out.push_str(&raw("sortState"));
        out.push_str(&raw("dataConsolidate"));
        out.push_str(&raw("customSheetViews"));
        if !sheet.merges.is_empty() {
            out.push_str(&format!("<mergeCells count=\"{}\">", sheet.merges.len()));
            for m in &sheet.merges {
                out.push_str(&format!("<mergeCell ref=\"{}\"/>", m.a1()));
            }
            out.push_str("</mergeCells>");
        }
        out.push_str(&raw("phoneticPr"));
        for cf in &sheet.conditional {
            out.push_str(&conditional(cf, &book.styles.theme));
        }
        if !sheet.validations.is_empty() {
            out.push_str(&format!("<dataValidations count=\"{}\">", sheet.validations.len()));
            for v in &sheet.validations {
                out.push_str(&validation(v));
            }
            out.push_str("</dataValidations>");
        }
        if !sheet.links.is_empty() {
            out.push_str("<hyperlinks>");
            for link in &sheet.links {
                let mut a = vec![("ref".to_owned(), link.range.a1())];
                match &link.target {
                    LinkTarget::External(target) => {
                        let (address, location) = match target.split_once('#') {
                            Some((a, l)) => (a.to_owned(), Some(l.to_owned())),
                            None => (target.clone(), None),
                        };
                        let id = next_id(&rels);
                        rels.push(Relationship {
                            id: id.clone(),
                            kind: REL_HYPERLINK.to_owned(),
                            target: address,
                            external: true,
                        });
                        a.push((format!("{r}:id"), id));
                        if let Some(l) = location {
                            a.push(("location".to_owned(), l));
                        }
                    }
                    LinkTarget::Location(l) => a.push(("location".to_owned(), l.clone())),
                }
                if let Some(d) = &link.display {
                    a.push(("display".to_owned(), d.clone()));
                }
                if let Some(t) = &link.tooltip {
                    a.push(("tooltip".to_owned(), t.clone()));
                }
                out.push_str(&format!("<hyperlink{}/>", attrs_text(&a)));
            }
            out.push_str("</hyperlinks>");
        }
        for name in [
            "printOptions",
            "pageMargins",
            "pageSetup",
            "headerFooter",
            "rowBreaks",
            "colBreaks",
            "customProperties",
            "cellWatches",
            "ignoredErrors",
            "smartTags",
        ] {
            match name {
                "cellWatches" => out.push_str(&super::audit::watches_xml(&sheet.watches)),
                "ignoredErrors" => {
                    out.push_str(&super::audit::ignored_errors_xml(&sheet.ignored_errors))
                }
                _ => out.push_str(&raw(name)),
            }
            // A sheet with no margins gets Excel's default ones, in their place in the schema's order: before
            // `pageSetup` and before the drawing, or Excel refuses the file.
            if name == "pageMargins" && sheet.page_margins_missing() {
                out.push_str("<pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>");
            }
        }
        // The drawing: kept as it was, with the charts made here added to it, or a new one holding them.
        let new_charts: Vec<&crate::model::Chart> = sheet.charts.iter().filter(|c| c.new).collect();
        if new_charts.is_empty() {
            out.push_str(&raw("drawing"));
        } else {
            let existing = sheet.raw.rels.iter().find(|r| r.kind == REL_DRAWING).cloned();
            let (drawing_part, mut drawing_rels, mut anchors, kept_xml) = match &existing {
                Some(rel) => {
                    let path = resolve(part, &rel.target);
                    let xml = book
                        .kept
                        .get(&path)
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .unwrap_or_default();
                    let rels_bytes = book
                        .kept
                        .get(&rels_path(&path))
                        .map(|b| String::from_utf8_lossy(b).into_owned())
                        .unwrap_or_default();
                    (path, parse_rels(&rels_bytes), String::new(), Some(xml))
                }
                None => (self.fresh("xl/drawings/drawing{}.xml"), Vec::new(), String::new(), None),
            };
            self.taken.insert(drawing_part.clone());
            for (i, chart) in new_charts.iter().enumerate() {
                let chart_part = self.fresh("xl/charts/chart{}.xml");
                let text_categories = chart
                    .series
                    .first()
                    .and_then(|s| s.categories.as_deref())
                    .is_some_and(|c| categories_are_text(book, c));
                self.add(
                    &chart_part,
                    Some(CT_CHART),
                    super::chart::chart_xml(chart, text_categories).into_bytes(),
                );
                let id = next_id(&drawing_rels);
                drawing_rels.push(Relationship {
                    id: id.clone(),
                    kind: REL_CHART.to_owned(),
                    target: relative(&drawing_part, &chart_part),
                    external: false,
                });
                anchors.push_str(&super::chart::anchor_xml(chart, &id, 1000 + i as u32));
            }
            let drawing = match kept_xml {
                Some(xml) if xml.contains("</xdr:wsDr>") => {
                    xml.replacen("</xdr:wsDr>", &format!("{anchors}</xdr:wsDr>"), 1)
                }
                _ => super::chart::drawing_xml(&anchors),
            };
            let content_type = if existing.is_some() { None } else { Some(CT_DRAWING) };
            self.add(&drawing_part, content_type, drawing.into_bytes());
            self.add(&rels_path(&drawing_part), None, write_rels(&drawing_rels).into_bytes());
            match existing {
                Some(_) => out.push_str(&raw("drawing")),
                None => {
                    let id = next_id(&rels);
                    rels.push(Relationship {
                        id: id.clone(),
                        kind: REL_DRAWING.to_owned(),
                        target: relative(part, &drawing_part),
                        external: false,
                    });
                    out.push_str(&format!("<drawing {r}:id=\"{id}\"/>"));
                }
            }
        }
        // Notes: the kept legacy drawing, or new comments and VML parts when they changed.
        if sheet.notes_changed {
            if !sheet.notes.is_empty() || !sheet.threads.is_empty() {
                if !sheet.threads.is_empty() {
                    let threaded_part = self.fresh("xl/threadedComments/threadedComment{}.xml");
                    let id = next_id(&rels);
                    rels.push(Relationship {
                        id,
                        kind: super::threads::REL_THREADED.to_owned(),
                        target: relative(part, &threaded_part),
                        external: false,
                    });
                    let xml = super::threads::threads_xml(&sheet.threads);
                    self.add(&threaded_part, Some(super::threads::CT_THREADED), xml.into_bytes());
                }
                let comments_part = self.fresh("xl/comments{}.xml");
                let vml_part = self.fresh("xl/drawings/vmlDrawing{}.vml");
                let comments_id = next_id(&rels);
                rels.push(Relationship {
                    id: comments_id,
                    kind: REL_COMMENTS.to_owned(),
                    target: relative(part, &comments_part),
                    external: false,
                });
                let vml_id = next_id(&rels);
                rels.push(Relationship {
                    id: vml_id.clone(),
                    kind: REL_VML.to_owned(),
                    target: relative(part, &vml_part),
                    external: false,
                });
                self.add(&comments_part, Some(CT_COMMENTS), comments_xml(sheet).into_bytes());
                self.add(&vml_part, None, vml_xml(sheet).into_bytes());
                out.push_str(&format!("<legacyDrawing {r}:id=\"{vml_id}\"/>"));
            }
        } else {
            out.push_str(&raw("legacyDrawing"));
        }
        for name in
            ["legacyDrawingHF", "drawingHF", "picture", "oleObjects", "controls", "webPublishItems"]
        {
            out.push_str(&raw(name));
        }
        // Elements the schema does not name, such as `mc:AlternateContent` round form controls.
        const KNOWN: &[&str] = &[
            "sheetPr",
            "sheetCalcPr",
            "sheetProtection",
            "protectedRanges",
            "scenarios",
            "sortState",
            "dataConsolidate",
            "customSheetViews",
            "phoneticPr",
            "printOptions",
            "pageMargins",
            "pageSetup",
            "headerFooter",
            "rowBreaks",
            "colBreaks",
            "customProperties",
            "cellWatches",
            "ignoredErrors",
            "smartTags",
            "drawing",
            "legacyDrawing",
            "legacyDrawingHF",
            "drawingHF",
            "picture",
            "oleObjects",
            "controls",
            "webPublishItems",
            "extLst",
        ];
        for (name, xml) in &sheet.raw.elements {
            if !KNOWN.contains(&name.as_str()) {
                out.push_str(xml);
            }
        }
        // Tables.
        let tables: Vec<&Table> = book.tables.iter().filter(|t| t.sheet == index).collect();
        if !tables.is_empty() {
            out.push_str(&format!("<tableParts count=\"{}\">", tables.len()));
            for t in tables {
                let table_part = if t.part.is_empty() {
                    self.fresh("xl/tables/table{}.xml")
                } else {
                    t.part.clone()
                };
                let id = next_id(&rels);
                rels.push(Relationship {
                    id: id.clone(),
                    kind: REL_TABLE.to_owned(),
                    target: relative(part, &table_part),
                    external: false,
                });
                out.push_str(&format!("<tablePart {r}:id=\"{id}\"/>"));
                self.add(&table_part, Some(CT_TABLE), table_xml(t).into_bytes());
            }
            out.push_str("</tableParts>");
        }
        out.push_str(&raw("extLst"));
        out.push_str("</worksheet>");
        Ok((out, rels))
    }

    fn sheet_data(&mut self, sheet: &Sheet, out: &mut String) {
        out.push_str("<sheetData>");
        // Rows with cells, spilled values, or row information.
        let mut rows: BTreeSet<u32> = sheet.rows.keys().copied().collect();
        rows.extend(sheet.cells.keys().map(|(r, _)| *r));
        rows.extend(sheet.spills.values.keys().map(|(r, _)| *r));
        for row in rows {
            let info = sheet.rows.get(&row);
            let mut attrs = vec![("r".to_owned(), (row + 1).to_string())];
            if let Some(info) = info {
                if let Some(s) = info.style {
                    attrs.push(("s".to_owned(), s.to_string()));
                    attrs.push(("customFormat".to_owned(), "1".to_owned()));
                }
                if let Some(h) = info.height {
                    attrs.push(("ht".to_owned(), number_text(h)));
                    attrs.push(("customHeight".to_owned(), "1".to_owned()));
                }
                if info.hidden {
                    attrs.push(("hidden".to_owned(), "1".to_owned()));
                }
                if info.outline > 0 {
                    attrs.push(("outlineLevel".to_owned(), info.outline.to_string()));
                }
                if info.collapsed {
                    attrs.push(("collapsed".to_owned(), "1".to_owned()));
                }
                attrs.extend(info.attrs.iter().cloned());
            }
            out.push_str(&format!("<row{}>", attrs_text(&attrs)));
            // Cells and spilled values in column order.
            let mut cols: BTreeSet<u32> =
                sheet.cells.range((row, 0)..=(row, u32::MAX)).map(|((_, c), _)| *c).collect();
            cols.extend(
                sheet.spills.values.range((row, 0)..=(row, u32::MAX)).map(|((_, c), _)| *c),
            );
            for col in cols {
                let at = CellRef::new(row, col);
                match sheet.cells.get(&(row, col)) {
                    Some(cell) => {
                        // A styled empty cell inside a spill holds the spilled value.
                        let spilled = cell.formula.is_none()
                            && cell.value.is_empty()
                            && sheet.spills.values.contains_key(&(row, col));
                        if spilled {
                            let v = sheet.spills.values[&(row, col)].clone();
                            self.cell(
                                out,
                                at,
                                &Cell { value: v, style: cell.style, ..Default::default() },
                                sheet,
                                true,
                            );
                        } else {
                            self.cell(out, at, cell, sheet, false);
                        }
                    }
                    None => {
                        let v = sheet.spills.values[&(row, col)].clone();
                        self.cell(out, at, &Cell::value(v), sheet, true);
                    }
                }
            }
            out.push_str("</row>");
        }
        out.push_str("</sheetData>");
    }

    /// One cell. `spilled` is a value a dynamic array put there, which Excel writes as a formula's result
    /// would be written, text as `t="str"` rather than a shared string.
    fn cell(&mut self, out: &mut String, at: CellRef, cell: &Cell, sheet: &Sheet, spilled: bool) {
        out.push_str(&format!("<c r=\"{}\"", at.a1()));
        if cell.style != 0 {
            out.push_str(&format!(" s=\"{}\"", cell.style));
        }
        let (kind, value_text) = match &cell.value {
            Value::Empty => (None, None),
            Value::Number(n) => (None, Some(number_text(*n))),
            Value::Bool(b) => (Some("b"), Some(if *b { "1" } else { "0" }.to_owned())),
            // The newer errors are written as #VALUE!, which is what Excel writes beside the rich value that
            // carries the real one; Excel works the real one out again when it calculates.
            Value::Error(e) => (
                Some("e"),
                Some(match e {
                    crate::value::ErrorKind::Spill
                    | crate::value::ErrorKind::Calc
                    | crate::value::ErrorKind::GettingData => "#VALUE!".to_owned(),
                    other => other.text().to_owned(),
                }),
            ),
            Value::Text(t) => {
                if cell.formula.is_some() || spilled {
                    (Some("str"), Some(escape(t)))
                } else {
                    let rich = cell.rich.as_deref().filter(|r| rich_text_of(r) == **t);
                    let index = self.string(t, rich);
                    (Some("s"), Some(index.to_string()))
                }
            }
        };
        if let Some(k) = kind {
            out.push_str(&format!(" t=\"{k}\""));
        }
        let anchor = (at.row, at.col);
        let dynamic = cell.formula.as_ref().is_some_and(|f| {
            f.dynamic
                && f.array.is_none()
                && !f.frozen
                && (sheet.spills.anchors.contains_key(&anchor) || is_dynamic_shape(f))
        });
        if dynamic {
            self.needs_metadata = true;
            let cm = self.book.dynamic_cm.unwrap_or(1);
            out.push_str(&format!(" cm=\"{cm}\""));
        }
        if let Some(extra) = &cell.extra {
            out.push_str(extra);
        }
        out.push('>');
        if let Some(f) = &cell.formula {
            let text = if f.frozen { f.text.to_string() } else { print(&f.expr, true) };
            let kept = f.attrs.as_deref().unwrap_or("");
            if let Some(range) = f.array {
                out.push_str(&format!(
                    "<f t=\"array\" ref=\"{}\"{kept}>{}</f>",
                    range.a1(),
                    escape(&text)
                ));
            } else if dynamic {
                let range = sheet.spills.range_of(anchor).unwrap_or(Range::cell(at));
                out.push_str(&format!(
                    "<f t=\"array\" ref=\"{}\"{kept}>{}</f>",
                    range.a1(),
                    escape(&text)
                ));
            } else {
                out.push_str(&format!("<f{kept}>{}</f>", escape(&text)));
            }
        }
        if cell.formula.is_none() {
            if let Some(t) = sheet.data_tables.iter().find(|t| t.range.first == at) {
                out.push_str(&data_table_xml(t));
            }
        }
        if let Some(v) = value_text {
            out.push_str(&format!("<v>{v}</v>"));
        }
        out.push_str("</c>");
    }
}

/// The `f` element of a What If data table, written on its first results cell.
fn data_table_xml(t: &DataTable) -> String {
    let mut a = vec![("t".to_owned(), "dataTable".to_owned()), ("ref".to_owned(), t.range.a1())];
    if t.two_d {
        a.push(("dt2D".to_owned(), "1".to_owned()));
    }
    if t.row_input {
        a.push(("dtr".to_owned(), "1".to_owned()));
    }
    for (k, c) in [("r1", t.r1), ("r2", t.r2)] {
        if let Some(c) = c {
            a.push((k.to_owned(), c.a1()));
        }
    }
    a.extend(t.attrs.iter().cloned());
    format!("<f{}/>", attrs_text(&a))
}

/// Whether a dynamic formula is written as a dynamic array: when it spilled or could return an array.
fn is_dynamic_shape(f: &crate::formula::Formula) -> bool {
    let mut arrayish = false;
    f.expr.walk(&mut |e| match e {
        crate::formula::Expr::Ref(r) if r.shape != crate::formula::Shape::Cell => arrayish = true,
        crate::formula::Expr::Array(_)
        | crate::formula::Expr::Spill(_)
        | crate::formula::Expr::Name(_)
        | crate::formula::Expr::Table(_) => arrayish = true,
        crate::formula::Expr::Call(name, _) if ARRAY_FUNCTIONS.contains(&&**name) => {
            arrayish = true
        }
        _ => {}
    });
    arrayish
}

/// Functions that return an array with no range among their arguments.
const ARRAY_FUNCTIONS: &[&str] = &[
    "SEQUENCE",
    "RANDARRAY",
    "MUNIT",
    "TEXTSPLIT",
    "MAKEARRAY",
    "FREQUENCY",
    "MODE.MULT",
    "TREND",
    "MINVERSE",
    "MMULT",
    "TRANSPOSE",
];

trait PageMargins {
    fn page_margins_missing(&self) -> bool;
}

impl PageMargins for Sheet {
    /// Excel writes page margins on every sheet; a sheet made here gets the defaults.
    fn page_margins_missing(&self) -> bool {
        !self.raw.elements.iter().any(|(n, _)| n == "pageMargins")
    }
}

/// Whether a chart's categories are words rather than numbers, which decides how the file names them.
fn categories_are_text(book: &Workbook, formula: &str) -> bool {
    let Some((sheet, area)) = formula.rsplit_once('!') else { return false };
    let name = sheet.trim_matches('\'').replace("''", "'");
    let Some(index) = book.sheet_index(&name) else { return false };
    let Some(range) = Range::parse(&area.replace('$', "")) else { return false };
    matches!(book.sheets[index].value_at(range.first), crate::value::Value::Text(_))
}

fn absolute(range: Range) -> String {
    let p = |c: CellRef| format!("${}${}", column_name(c.col), c.row + 1);
    if range.first == range.last {
        p(range.first)
    } else {
        format!("{}:{}", p(range.first), p(range.last))
    }
}

/// A number as a file writes it: as short as reads back exactly.
pub fn number_text(n: f64) -> String {
    if n == 0.0 {
        return "0".to_owned();
    }
    let a = n.abs();
    if (1e-5..1e15).contains(&a) {
        format!("{n}")
    } else {
        format!("{n:e}").replace('e', "E")
    }
}

/// A shared string item from the XML a rich string was read with, which was an `si` or an `is`.
fn rich_si(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("<is") {
        let body = rest.strip_suffix("</is>").unwrap_or(rest);
        return format!("<si{body}</si>");
    }
    trimmed.to_owned()
}

/// The plain text of rich string XML, to check it still matches the cell.
fn rich_text_of(raw: &str) -> String {
    let wrapped = raw.trim();
    let Ok(doc) = roxmltree::Document::parse(wrapped) else { return String::new() };
    let mut text = String::new();
    for n in doc.root_element().children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "t" => text.push_str(n.text().unwrap_or("")),
            "r" => {
                if let Some(t) = n.children().find(|c| c.tag_name().name() == "t") {
                    text.push_str(t.text().unwrap_or(""));
                }
            }
            _ => {}
        }
    }
    unescape_office(&text)
}

fn color_attrs(c: &Color) -> String {
    match c {
        Color::Auto => " auto=\"1\"".to_owned(),
        Color::Rgb(argb) => format!(" rgb=\"{argb:08X}\""),
        Color::Theme(t, tint) => {
            if *tint == 0.0 {
                format!(" theme=\"{t}\"")
            } else {
                format!(" theme=\"{t}\" tint=\"{tint}\"")
            }
        }
        Color::Indexed(i) => format!(" indexed=\"{i}\""),
    }
}

fn sheet_pr(raw: &str, tab: Option<Color>, _theme: &[u32]) -> String {
    let tab_xml = tab.map(|c| format!("<tabColor{}/>", color_attrs(&c))).unwrap_or_default();
    if raw.is_empty() {
        return if tab_xml.is_empty() {
            String::new()
        } else {
            format!("<sheetPr>{tab_xml}</sheetPr>")
        };
    }
    // Take out any tab colour, then put the current one first.
    let mut body = raw.to_owned();
    while let Some(start) = body.find("<tabColor") {
        let end = body[start..].find("/>").map(|e| start + e + 2).unwrap_or(body.len());
        body.replace_range(start..end, "");
    }
    if tab_xml.is_empty() {
        return body;
    }
    if let Some(stripped) = body.strip_suffix("/>") {
        if !stripped.contains('>') {
            return format!("{stripped}>{tab_xml}</sheetPr>");
        }
    }
    match body.find('>') {
        Some(gt) => format!("{}{}{}", &body[..=gt], tab_xml, &body[gt + 1..]),
        None => body,
    }
}

fn sheet_views(sheet: &Sheet, selected: bool) -> String {
    let v = &sheet.view;
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut push = |k: &str, val: String| attrs.push((k.to_owned(), val));
    if selected {
        push("tabSelected", "1".to_owned());
    }
    if !sheet.show_grid {
        push("showGridLines", "0".to_owned());
    }
    if v.show_formulas {
        push("showFormulas", "1".to_owned());
    }
    if !v.show_headings {
        push("showRowColHeaders", "0".to_owned());
    }
    if !v.show_zeros {
        push("showZeros", "0".to_owned());
    }
    if v.right_to_left {
        push("rightToLeft", "1".to_owned());
    }
    if v.kind != ViewKind::Normal {
        push("view", v.kind.name().to_owned());
    }
    if let Some(at) = v.top_left {
        push("topLeftCell", at.a1());
    }
    if sheet.zoom != 100 && sheet.zoom != 0 {
        push("zoomScale", sheet.zoom.to_string());
    }
    let normal = v.zoom_normal.or((sheet.zoom != 100 && sheet.zoom != 0).then_some(sheet.zoom));
    for (k, z) in [
        ("zoomScaleNormal", normal),
        ("zoomScalePageLayoutView", v.zoom_layout),
        ("zoomScaleSheetLayoutView", v.zoom_page_break),
    ] {
        if let Some(z) = z {
            push(k, z.to_string());
        }
    }
    attrs.extend(v.extra.iter().cloned());
    attrs.push(("workbookViewId".to_owned(), "0".to_owned()));
    let mut inner = pane_xml(sheet);
    for s in &v.selections {
        let mut a: Vec<(String, String)> = Vec::new();
        if let Some(p) = &s.pane {
            a.push(("pane".to_owned(), p.clone()));
        }
        a.push(("activeCell".to_owned(), s.active.a1()));
        let sqref: Vec<String> = s.ranges.iter().map(Range::a1).collect();
        a.push(("sqref".to_owned(), sqref.join(" ")));
        inner.push_str(&format!("<selection{}/>", attrs_text(&a)));
    }
    if v.selections.is_empty() {
        if let Some((pane, at)) = frozen_pane(sheet) {
            let at = at.a1();
            inner.push_str(&format!(
                "<selection pane=\"{pane}\" activeCell=\"{at}\" sqref=\"{at}\"/>"
            ));
        }
    }
    let others: String = v.others.concat();
    format!("<sheetViews><sheetView{}>{inner}</sheetView>{others}</sheetViews>", attrs_text(&attrs))
}

/// The pane a frozen sheet's active pane is, with the first cell below and right of the frozen lines.
fn frozen_pane(sheet: &Sheet) -> Option<(&'static str, CellRef)> {
    let (rows, cols) = sheet.frozen.filter(|(r, c)| *r > 0 || *c > 0)?;
    let pane = match (rows > 0, cols > 0) {
        (true, true) => "bottomRight",
        (true, false) => "bottomLeft",
        _ => "topRight",
    };
    Some((pane, CellRef::new(rows, cols)))
}

/// The `pane` element: frozen panes from the model, or split panes as the split bars left them.
fn pane_xml(sheet: &Sheet) -> String {
    let v = &sheet.view;
    if let Some((pane, at)) = frozen_pane(sheet) {
        let (rows, cols) = sheet.frozen.unwrap_or_default();
        let top_left = v.pane_top_left.unwrap_or(at).a1();
        let active = v.active_pane.clone().unwrap_or_else(|| pane.to_owned());
        let state = if v.frozen_split { "frozenSplit" } else { "frozen" };
        return format!(
            "<pane{}{} topLeftCell=\"{top_left}\" activePane=\"{active}\" state=\"{state}\"/>",
            if cols > 0 { format!(" xSplit=\"{cols}\"") } else { String::new() },
            if rows > 0 { format!(" ySplit=\"{rows}\"") } else { String::new() }
        );
    }
    let Some(split) = &v.split else { return String::new() };
    let mut a: Vec<(String, String)> = Vec::new();
    if split.x > 0.0 {
        a.push(("xSplit".to_owned(), number_text(split.x)));
    }
    if split.y > 0.0 {
        a.push(("ySplit".to_owned(), number_text(split.y)));
    }
    if let Some(t) = split.top_left {
        a.push(("topLeftCell".to_owned(), t.a1()));
    }
    if let Some(p) = &v.active_pane {
        a.push(("activePane".to_owned(), p.clone()));
    }
    format!("<pane{}/>", attrs_text(&a))
}

/// A `sortState` element from the model.
pub(crate) fn sort_state_xml(sort: &SortState) -> String {
    let mut a = vec![("ref".to_owned(), sort.range.a1())];
    if sort.columns {
        a.push(("columnSort".to_owned(), "1".to_owned()));
    }
    if sort.case_sensitive {
        a.push(("caseSensitive".to_owned(), "1".to_owned()));
    }
    a.extend(sort.attrs.iter().cloned());
    let mut out = format!("<sortState{}>", attrs_text(&a));
    for c in &sort.conditions {
        let mut a: Vec<(String, String)> = Vec::new();
        if c.descending {
            a.push(("descending".to_owned(), "1".to_owned()));
        }
        if c.by != SortBy::Value {
            a.push(("sortBy".to_owned(), c.by.name().to_owned()));
        }
        a.push(("ref".to_owned(), c.range.a1()));
        if let Some(l) = &c.custom_list {
            a.push(("customList".to_owned(), l.clone()));
        }
        if let Some(d) = c.dxf {
            a.push(("dxfId".to_owned(), d.to_string()));
        }
        if let Some(i) = &c.icon_set {
            a.push(("iconSet".to_owned(), i.clone()));
        }
        if let Some(i) = c.icon_id {
            a.push(("iconId".to_owned(), i.to_string()));
        }
        out.push_str(&format!("<sortCondition{}/>", attrs_text(&a)));
    }
    out.push_str("</sortState>");
    out
}

fn auto_filter(filter: &AutoFilter) -> String {
    let mut out = format!("<autoFilter ref=\"{}\"", filter.range.a1());
    if filter.columns.is_empty() && filter.sort.is_none() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    for (col, criteria) in &filter.columns {
        match criteria {
            FilterCriteria::Raw(xml) => out.push_str(xml),
            FilterCriteria::Values { values, blank } => {
                out.push_str(&format!(
                    "<filterColumn colId=\"{col}\"><filters{}>",
                    if *blank { " blank=\"1\"" } else { "" }
                ));
                for v in values {
                    out.push_str(&format!("<filter val=\"{}\"/>", escape(v)));
                }
                out.push_str("</filters></filterColumn>");
            }
            FilterCriteria::Custom { conditions, and } => {
                out.push_str(&format!(
                    "<filterColumn colId=\"{col}\"><customFilters{}>",
                    if *and { " and=\"1\"" } else { "" }
                ));
                for (op, v) in conditions {
                    out.push_str(&format!(
                        "<customFilter operator=\"{}\" val=\"{}\"/>",
                        escape(op),
                        escape(v)
                    ));
                }
                out.push_str("</customFilters></filterColumn>");
            }
        }
    }
    if let Some(sort) = &filter.sort {
        out.push_str(&sort_state_xml(sort));
    }
    out.push_str("</autoFilter>");
    out
}

fn conditional(cf: &ConditionalFormat, _theme: &[u32]) -> String {
    let sqref: Vec<String> = cf.ranges.iter().map(Range::a1).collect();
    let mut out = format!("<conditionalFormatting sqref=\"{}\">", sqref.join(" "));
    for rule in &cf.rules {
        let mut a = vec![("type".to_owned(), rule.kind.clone())];
        if let Some(d) = rule.dxf {
            a.push(("dxfId".to_owned(), d.to_string()));
        }
        a.push(("priority".to_owned(), rule.priority.to_string()));
        if rule.stop {
            a.push(("stopIfTrue".to_owned(), "1".to_owned()));
        }
        if let Some(above) = rule.above_average {
            if !above {
                a.push(("aboveAverage".to_owned(), "0".to_owned()));
            }
        }
        if rule.percent {
            a.push(("percent".to_owned(), "1".to_owned()));
        }
        if rule.bottom {
            a.push(("bottom".to_owned(), "1".to_owned()));
        }
        if let Some(op) = &rule.operator {
            a.push(("operator".to_owned(), op.clone()));
        }
        if let Some(t) = &rule.text {
            a.push(("text".to_owned(), t.clone()));
        }
        if let Some(r) = rule.rank {
            a.push(("rank".to_owned(), r.to_string()));
        }
        a.extend(rule.attrs.iter().cloned());
        out.push_str(&format!("<cfRule{}>", attrs_text(&a)));
        for f in &rule.formulas {
            out.push_str(&format!("<formula>{}</formula>", escape(f)));
        }
        let cfvo = |c: &Cfvo| {
            format!(
                "<cfvo type=\"{}\"{}/>",
                escape(&c.kind),
                c.value.as_ref().map(|v| format!(" val=\"{}\"", escape(v))).unwrap_or_default()
            )
        };
        if let Some(scale) = &rule.color_scale {
            out.push_str("<colorScale>");
            for (c, _) in scale {
                out.push_str(&cfvo(c));
            }
            for (_, color) in scale {
                out.push_str(&format!("<color{}/>", color_attrs(color)));
            }
            out.push_str("</colorScale>");
        }
        if let Some((lo, hi, color)) = &rule.data_bar {
            out.push_str(&format!(
                "<dataBar>{}{}<color{}/></dataBar>",
                cfvo(lo),
                cfvo(hi),
                color_attrs(color)
            ));
        }
        out.push_str(&rule.raw_children);
        out.push_str("</cfRule>");
    }
    out.push_str("</conditionalFormatting>");
    out
}

fn validation(v: &Validation) -> String {
    let mut a = Vec::new();
    if v.kind != "none" {
        a.push(("type".to_owned(), v.kind.clone()));
    }
    if let Some(s) = &v.error_style {
        a.push(("errorStyle".to_owned(), s.clone()));
    }
    if let Some(op) = &v.operator {
        a.push(("operator".to_owned(), op.clone()));
    }
    if v.allow_blank {
        a.push(("allowBlank".to_owned(), "1".to_owned()));
    }
    if !v.show_dropdown {
        a.push(("showDropDown".to_owned(), "1".to_owned()));
    }
    let mut attrs = v.attrs.clone();
    if !attrs.iter().any(|(k, _)| k == "showInputMessage") {
        attrs.push(("showInputMessage".to_owned(), "1".to_owned()));
    }
    if !attrs.iter().any(|(k, _)| k == "showErrorMessage") {
        attrs.push(("showErrorMessage".to_owned(), "1".to_owned()));
    }
    a.extend(attrs);
    for (k, val) in [
        ("errorTitle", &v.error_title),
        ("error", &v.error),
        ("promptTitle", &v.prompt_title),
        ("prompt", &v.prompt),
    ] {
        if let Some(t) = val {
            a.push((k.to_owned(), t.clone()));
        }
    }
    let sqref: Vec<String> = v.ranges.iter().map(Range::a1).collect();
    a.push(("sqref".to_owned(), sqref.join(" ")));
    let mut out = format!("<dataValidation{}>", attrs_text(&a));
    if let Some(f) = &v.formula1 {
        out.push_str(&format!("<formula1>{}</formula1>", escape(f)));
    }
    if let Some(f) = &v.formula2 {
        out.push_str(&format!("<formula2>{}</formula2>", escape(f)));
    }
    out.push_str("</dataValidation>");
    out
}

fn table_xml(t: &Table) -> String {
    // The table's own XML with its range brought up to date.
    let Ok(doc) = roxmltree::Document::parse(&t.xml) else { return t.xml.clone() };
    let root = doc.root_element();
    let old = root.attribute("ref").unwrap_or("");
    let new = t.range.a1();
    if old == new {
        return t.xml.clone();
    }
    let filter_old = root
        .children()
        .find(|n| n.tag_name().name() == "autoFilter")
        .and_then(|n| n.attribute("ref"))
        .map(str::to_owned);
    let mut xml = t.xml.replacen(&format!("ref=\"{old}\""), &format!("ref=\"{new}\""), 1);
    if let Some(f) = filter_old {
        let data_last =
            if t.totals_row { t.range.last.row.saturating_sub(1) } else { t.range.last.row };
        let filter_new = Range::new(t.range.first, CellRef::new(data_last, t.range.last.col)).a1();
        xml = xml.replacen(&format!("ref=\"{f}\""), &format!("ref=\"{filter_new}\""), 1);
    }
    xml
}

fn comments_xml(sheet: &Sheet) -> String {
    let notes = super::threads::combined_notes(sheet);
    let mut authors: Vec<String> = Vec::new();
    for (_, note) in &notes {
        if !authors.contains(&note.author) {
            authors.push(note.author.clone());
        }
    }
    // A note that carries a threaded comment keeps the `xr:uid` the thread is matched by.
    let revision = notes.iter().any(|(_, n)| n.attrs.iter().any(|(k, _)| k.starts_with("xr:")));
    let declarations = if revision {
        " xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"xr\" xmlns:xr=\"http://schemas.microsoft.com/office/spreadsheetml/2014/revision\""
    } else {
        ""
    };
    let mut out = format!("{XML_HEAD}<comments xmlns=\"{MAIN_NS}\"{declarations}><authors>");
    for a in &authors {
        out.push_str(&format!("<author>{}</author>", escape(a)));
    }
    out.push_str("</authors><commentList>");
    for ((row, col), note) in &notes {
        let author = authors.iter().position(|a| *a == note.author).unwrap_or(0);
        let text = if super::threads::is_legacy_note(note) {
            format!("<t>{}</t>", escape(&note.text))
        } else {
            format!(
                "<r><rPr><sz val=\"9\"/><color indexed=\"81\"/><rFont val=\"Tahoma\"/><family val=\"2\"/></rPr><t xml:space=\"preserve\">{}</t></r>",
                escape(&note.text)
            )
        };
        out.push_str(&format!(
            "<comment ref=\"{}\" authorId=\"{author}\"{}><text>{text}</text></comment>",
            CellRef::new(*row, *col).a1(),
            attrs_text(&note.attrs),
        ));
    }
    out.push_str("</commentList></comments>");
    out
}

/// The legacy VML shapes Excel draws notes with. Without them Excel shows no red corner and no note.
fn vml_xml(sheet: &Sheet) -> String {
    let mut out = String::from(
        "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\
<o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"1\"/></o:shapelayout>\
<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>",
    );
    for (i, ((row, col), note)) in super::threads::combined_notes(sheet).iter().enumerate() {
        let (visibility, visible) =
            if note.visible { ("visible", "<x:Visible/>") } else { ("hidden", "") };
        let bounds = note.bounds.clone().unwrap_or_else(|| {
            crate::edit::review::notebox::default_box(sheet, CellRef::new(*row, *col))
        });
        let a = bounds.anchor;
        out.push_str(&format!(
            "<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:{}pt;margin-top:{}pt;width:{}pt;height:{}pt;z-index:{};visibility:{visibility}\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\">\
<v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/>\
<v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox>\
<x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, {}, {}, {}, {}, {}, {}, {}</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column>{visible}</x:ClientData></v:shape>",
            1025 + i,
            number_text(bounds.left),
            number_text(bounds.top),
            number_text(bounds.width),
            number_text(bounds.height),
            i + 1,
            a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7],
        ));
    }
    out.push_str("</xml>");
    out
}

fn styles_xml(styles: &Styles) -> String {
    let mut ns = styles.namespaces.clone();
    ns.retain(|(p, _)| !p.is_empty());
    let mut out = format!("{XML_HEAD}<styleSheet{}>", namespaces_text(&ns, MAIN_NS));
    let custom: Vec<(&u32, &String)> = styles
        .num_fmts
        .iter()
        .filter(|(id, _)| **id >= 164 || builtin_format(**id).is_none())
        .collect();
    if !custom.is_empty() {
        out.push_str(&format!("<numFmts count=\"{}\">", custom.len()));
        for (id, code) in custom {
            out.push_str(&format!("<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>", escape(code)));
        }
        out.push_str("</numFmts>");
    }
    out.push_str(&format!("<fonts count=\"{}\">", styles.fonts.len()));
    for f in &styles.fonts {
        out.push_str(&font_xml(f, "font"));
    }
    out.push_str("</fonts>");
    out.push_str(&format!("<fills count=\"{}\">", styles.fills.len()));
    for f in &styles.fills {
        match &f.raw {
            Some(raw) => out.push_str(raw),
            None => {
                out.push_str(&format!("<fill><patternFill patternType=\"{}\"", escape(&f.pattern)));
                let fg = !matches!(f.fg, Color::Auto);
                let bg = !matches!(f.bg, Color::Auto);
                if fg || bg {
                    out.push('>');
                    if fg {
                        out.push_str(&format!("<fgColor{}/>", color_attrs(&f.fg)));
                    }
                    if bg {
                        out.push_str(&format!("<bgColor{}/>", color_attrs(&f.bg)));
                    }
                    out.push_str("</patternFill></fill>");
                } else {
                    out.push_str("/></fill>");
                }
            }
        }
    }
    out.push_str("</fills>");
    out.push_str(&format!("<borders count=\"{}\">", styles.borders.len()));
    for b in &styles.borders {
        let mut attrs = String::new();
        if b.diagonal_up {
            attrs.push_str(" diagonalUp=\"1\"");
        }
        if b.diagonal_down {
            attrs.push_str(" diagonalDown=\"1\"");
        }
        out.push_str(&format!("<border{attrs}>"));
        for (name, e) in [
            ("left", &b.left),
            ("right", &b.right),
            ("top", &b.top),
            ("bottom", &b.bottom),
            ("diagonal", &b.diagonal),
        ] {
            if e.is_none() {
                out.push_str(&format!("<{name}/>"));
            } else if matches!(e.color, Color::Auto) {
                out.push_str(&format!(
                    "<{name} style=\"{}\"><color auto=\"1\"/></{name}>",
                    escape(&e.style)
                ));
            } else {
                out.push_str(&format!(
                    "<{name} style=\"{}\"><color{}/></{name}>",
                    escape(&e.style),
                    color_attrs(&e.color)
                ));
            }
        }
        out.push_str("</border>");
    }
    out.push_str("</borders>");
    out.push_str(styles.raw_cell_style_xfs.as_deref().unwrap_or("<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>"));
    out.push_str(&format!("<cellXfs count=\"{}\">", styles.xfs.len()));
    for x in &styles.xfs {
        let mut a = format!(
            "<xf numFmtId=\"{}\" fontId=\"{}\" fillId=\"{}\" borderId=\"{}\" xfId=\"{}\"",
            x.num_fmt, x.font, x.fill, x.border, x.xf_id
        );
        if x.applies.is_empty() {
            for (flag, on) in [
                ("applyNumberFormat", x.num_fmt != 0),
                ("applyFont", x.font != 0),
                ("applyFill", x.fill != 0),
                ("applyBorder", x.border != 0),
                ("applyAlignment", !x.align.is_default()),
                ("applyProtection", x.locked.is_some() || x.hidden.is_some()),
            ] {
                if on {
                    a.push_str(&format!(" {flag}=\"1\""));
                }
            }
        } else {
            a.push_str(&attrs_text(&x.applies));
        }
        if x.quote_prefix {
            a.push_str(" quotePrefix=\"1\"");
        }
        if x.pivot_button {
            a.push_str(" pivotButton=\"1\"");
        }
        let mut inner = String::new();
        if !x.align.is_default() {
            let al = &x.align;
            let mut s = String::from("<alignment");
            if let Some(h) = &al.horizontal {
                s.push_str(&format!(" horizontal=\"{}\"", escape(h)));
            }
            if let Some(v) = &al.vertical {
                s.push_str(&format!(" vertical=\"{}\"", escape(v)));
            }
            if al.rotation != 0 {
                s.push_str(&format!(" textRotation=\"{}\"", al.rotation));
            }
            if al.wrap {
                s.push_str(" wrapText=\"1\"");
            }
            if al.indent > 0 {
                s.push_str(&format!(" indent=\"{}\"", al.indent));
            }
            if al.shrink {
                s.push_str(" shrinkToFit=\"1\"");
            }
            if al.reading_order != 0 {
                s.push_str(&format!(" readingOrder=\"{}\"", al.reading_order));
            }
            s.push_str(&attrs_text(&al.extra));
            s.push_str("/>");
            inner.push_str(&s);
        }
        if x.locked.is_some() || x.hidden.is_some() {
            inner.push_str("<protection");
            if let Some(l) = x.locked {
                inner.push_str(&format!(" locked=\"{}\"", u8::from(l)));
            }
            if let Some(h) = x.hidden {
                inner.push_str(&format!(" hidden=\"{}\"", u8::from(h)));
            }
            inner.push_str("/>");
        }
        if inner.is_empty() {
            out.push_str(&a);
            out.push_str("/>");
        } else {
            out.push_str(&a);
            out.push('>');
            out.push_str(&inner);
            out.push_str("</xf>");
        }
    }
    out.push_str("</cellXfs>");
    out.push_str(styles.raw_cell_styles.as_deref().unwrap_or("<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>"));
    out.push_str(&format!("<dxfs count=\"{}\">", styles.dxfs.len()));
    for d in &styles.dxfs {
        match &d.raw {
            Some(raw) => out.push_str(raw),
            None => out.push_str(&dxf_xml(d)),
        }
    }
    out.push_str("</dxfs>");
    out.push_str(styles.raw_table_styles.as_deref().unwrap_or("<tableStyles count=\"0\" defaultTableStyle=\"TableStyleMedium2\" defaultPivotStyle=\"PivotStyleLight16\"/>"));
    if let Some(c) = &styles.raw_colors {
        out.push_str(c);
    }
    if let Some(e) = &styles.raw_ext {
        out.push_str(e);
    }
    out.push_str("</styleSheet>");
    out
}

fn font_xml(f: &Font, tag: &str) -> String {
    let mut out = format!("<{tag}>");
    if f.bold {
        out.push_str("<b/>");
    }
    if f.italic {
        out.push_str("<i/>");
    }
    if f.strike {
        out.push_str("<strike/>");
    }
    if let Some(u) = f.underline.name() {
        out.push_str(
            if u == "single" { "<u/>".to_owned() } else { format!("<u val=\"{u}\"/>") }.as_str(),
        );
    }
    if let Some(v) = &f.vert_align {
        out.push_str(&format!("<vertAlign val=\"{}\"/>", escape(v)));
    }
    out.push_str(&format!("<sz val=\"{}\"/>", number_text(f.size)));
    if !matches!(f.color, Color::Auto) {
        out.push_str(&format!("<color{}/>", color_attrs(&f.color)));
    }
    out.push_str(&format!("<name val=\"{}\"/>", escape(&f.name)));
    if let Some(fam) = f.family {
        out.push_str(&format!("<family val=\"{fam}\"/>"));
    }
    if let Some(cs) = f.charset {
        out.push_str(&format!("<charset val=\"{cs}\"/>"));
    }
    if let Some(s) = &f.scheme {
        out.push_str(&format!("<scheme val=\"{}\"/>", escape(s)));
    }
    out.push_str(&format!("</{tag}>"));
    out
}

pub fn dxf_xml(d: &Dxf) -> String {
    let mut out = String::from("<dxf>");
    let has_font = d.font_color.is_some()
        || d.bold.is_some()
        || d.italic.is_some()
        || d.strike.is_some()
        || d.underline.is_some();
    if has_font {
        out.push_str("<font>");
        if let Some(b) = d.bold {
            out.push_str(if b { "<b/>" } else { "<b val=\"0\"/>" });
        }
        if let Some(i) = d.italic {
            out.push_str(if i { "<i/>" } else { "<i val=\"0\"/>" });
        }
        if let Some(s) = d.strike {
            out.push_str(if s { "<strike/>" } else { "<strike val=\"0\"/>" });
        }
        if let Some(u) = d.underline {
            match u.name() {
                Some(name) => out.push_str(&format!("<u val=\"{name}\"/>")),
                None => out.push_str("<u val=\"none\"/>"),
            }
        }
        if let Some(c) = &d.font_color {
            out.push_str(&format!("<color{}/>", color_attrs(c)));
        }
        out.push_str("</font>");
    }
    if let Some(code) = &d.num_fmt {
        out.push_str(&format!("<numFmt numFmtId=\"200\" formatCode=\"{}\"/>", escape(code)));
    }
    if let Some(c) = &d.fill {
        out.push_str(&format!(
            "<fill><patternFill><bgColor{}/></patternFill></fill>",
            color_attrs(c)
        ));
    }
    if let Some(c) = &d.border {
        let side = |n: &str| format!("<{n} style=\"thin\"><color{}/></{n}>", color_attrs(c));
        out.push_str(&format!(
            "<border>{}{}{}{}</border>",
            side("left"),
            side("right"),
            side("top"),
            side("bottom")
        ));
    }
    out.push_str("</dxf>");
    out
}

const METADATA_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<metadata xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"1\"><metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/></metadataTypes><futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata><cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata></metadata>";

const CORE_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:creator></dc:creator></cp:coreProperties>";

const APP_XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><Application>Unluminous</Application></Properties>";

/// The Office theme a new workbook carries: its twelve colours, its two fonts and the three format lists
/// the schema requires.
pub const DEFAULT_THEME_XML: &str = include_str!("theme.xml");
