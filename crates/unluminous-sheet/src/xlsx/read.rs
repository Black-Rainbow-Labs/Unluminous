//! Reading an `.xlsx` file.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use roxmltree::{Document, Node};

use super::*;
use crate::coord::{CellRef, Range};
use crate::formula::{Expr, Formula};
use crate::model::*;
use crate::styles::*;
use crate::value::{ErrorKind, Value};

/// Parts the writer regenerates, by relationship type, so they are not kept as bytes.
const REGENERATED: &[&str] = &[REL_WORKSHEET, REL_STYLES, REL_SHARED_STRINGS, REL_CALC_CHAIN];

/// Read a workbook from the bytes of an `.xlsx` or `.xlsm` file.
pub fn read(bytes: &[u8]) -> Result<Workbook, String> {
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err("this is a binary Excel file (.xls) or an encrypted workbook, which this version does not read. Excel can save it as .xlsx".to_owned());
    }
    let entries = crate::zip::read(bytes)?;
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for e in entries {
        order.push(e.name.clone());
        files.insert(e.name, e.data);
    }
    let text = |path: &str| -> Option<String> { files.get(path).map(|b| decode(b)) };

    let mut book = Workbook { styles: Styles::default_workbook(), ..Default::default() };
    // Content types.
    let types = text("[Content_Types].xml")
        .ok_or("the file has no [Content_Types].xml, so it is not an Office file")?;
    let types_doc =
        Document::parse(&types).map_err(|e| format!("[Content_Types].xml is damaged: {e}"))?;
    let mut overrides: HashMap<String, String> = HashMap::new();
    for n in types_doc.root_element().children().filter(Node::is_element) {
        match n.tag_name().name() {
            "Default" => book.kept.defaults.push((
                n.attribute("Extension").unwrap_or("").to_owned(),
                n.attribute("ContentType").unwrap_or("").to_owned(),
            )),
            "Override" => {
                overrides.insert(
                    n.attribute("PartName").unwrap_or("").to_owned(),
                    n.attribute("ContentType").unwrap_or("").to_owned(),
                );
            }
            _ => {}
        }
    }
    // The package relationships lead to the workbook.
    let root_rels = parse_rels(&text("_rels/.rels").unwrap_or_default());
    let workbook_rel = root_rels
        .iter()
        .find(|r| r.kind == REL_OFFICE_DOCUMENT)
        .ok_or("the file names no workbook")?;
    let workbook_part = resolve("", &workbook_rel.target);
    book.kept.package_rels =
        root_rels.iter().filter(|r| r.kind != REL_OFFICE_DOCUMENT).cloned().collect();
    book.part = workbook_part.clone();
    if overrides.get(&format!("/{workbook_part}")).is_some_and(|t| t == CT_WORKBOOK_MACRO) {
        book.has_macros = true;
    }
    let workbook_xml = text(&workbook_part).ok_or("the workbook part is missing")?;
    let workbook_rels = parse_rels(&text(&rels_path(&workbook_part)).unwrap_or_default());
    let by_id: HashMap<&str, &Relationship> =
        workbook_rels.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut regenerated_parts: HashSet<String> = HashSet::new();
    regenerated_parts.insert(workbook_part.clone());
    regenerated_parts.insert(rels_path(&workbook_part));
    regenerated_parts.insert("[Content_Types].xml".to_owned());
    regenerated_parts.insert("_rels/.rels".to_owned());

    // Theme, styles and shared strings.
    let target_of = |kind: &str| {
        workbook_rels.iter().find(|r| r.kind == kind).map(|r| resolve(&workbook_part, &r.target))
    };
    if let Some(theme) = target_of(REL_THEME).and_then(|p| text(&p)) {
        read_theme(&theme, &mut book.styles);
    }
    if let Some(path) = target_of(REL_STYLES) {
        if let Some(xml) = text(&path) {
            read_styles(&xml, &mut book.styles)?;
        }
        regenerated_parts.insert(path);
    }
    let mut strings: Vec<(Arc<str>, Option<Arc<str>>)> = Vec::new();
    if let Some(path) = target_of(REL_SHARED_STRINGS) {
        if let Some(xml) = text(&path) {
            strings = read_strings(&xml)?;
        }
        regenerated_parts.insert(path);
    }
    if let Some(path) = target_of(REL_CALC_CHAIN) {
        regenerated_parts.insert(path);
    }
    // Dynamic array metadata: which cm index means a dynamic array.
    let mut dynamic_cm: Option<u32> = None;
    let mut rich_metadata = false;
    if let Some(path) = target_of(REL_METADATA) {
        if let Some(xml) = text(&path) {
            let (cm, only_dynamic) = read_metadata(&xml);
            rich_metadata = xml.contains("XLRICHVALUE");
            dynamic_cm = cm;
            if only_dynamic {
                regenerated_parts.insert(path);
            } else {
                book.dynamic_cm = cm;
            }
        }
    }

    // The workbook part.
    let doc =
        Document::parse(&workbook_xml).map_err(|e| format!("the workbook part is damaged: {e}"))?;
    let root = doc.root_element();
    book.namespaces = namespaces_of(root);
    let mut sheet_entries: Vec<(String, String, Visibility)> = Vec::new();
    let mut names: Vec<(Node, String)> = Vec::new();
    for child in root.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "workbookPr" => {
                book.date1904 = matches!(child.attribute("date1904"), Some("1") | Some("true"));
                book.workbook_pr = attrs_of(child, &["date1904"]);
            }
            "bookViews" => {
                if let Some(view) =
                    child.children().find(|n| n.has_tag_name((MAIN_NS, "workbookView")))
                {
                    book.active_sheet =
                        view.attribute("activeTab").and_then(|v| v.parse().ok()).unwrap_or(0);
                    book.view_attrs = attrs_of(view, &["activeTab", "firstSheet"]);
                }
            }
            "sheets" => {
                for s in child.children().filter(|n| n.has_tag_name((MAIN_NS, "sheet"))) {
                    let id = s.attribute((REL_NS, "id")).unwrap_or("").to_owned();
                    let state = match s.attribute("state") {
                        Some("hidden") => Visibility::Hidden,
                        Some("veryHidden") => Visibility::VeryHidden,
                        _ => Visibility::Visible,
                    };
                    sheet_entries.push((
                        s.attribute("name").unwrap_or("Sheet").to_owned(),
                        id,
                        state,
                    ));
                }
            }
            "definedNames" => {
                for n in child.children().filter(|n| n.has_tag_name((MAIN_NS, "definedName"))) {
                    names.push((n, n.text().unwrap_or("").to_owned()));
                }
            }
            "calcPr" => {
                book.calc = calc_settings(child);
            }
            name => book.raw.push((name.to_owned(), raw(&workbook_xml, child).to_owned())),
        }
    }
    for (n, formula) in names {
        book.names.push(DefinedName {
            name: n.attribute("name").unwrap_or("").to_owned(),
            scope: n.attribute("localSheetId").and_then(|v| v.parse().ok()),
            formula,
            hidden: matches!(n.attribute("hidden"), Some("1") | Some("true")),
            attrs: attrs_of(n, &["name", "localSheetId", "hidden"]),
        });
    }

    // Worksheets.
    for (index, (name, id, state)) in sheet_entries.into_iter().enumerate() {
        let Some(rel) = by_id.get(id.as_str()) else { continue };
        let part = resolve(&workbook_part, &rel.target);
        let mut sheet = Sheet::new(name);
        sheet.state = state;
        if rel.kind != REL_WORKSHEET {
            // A chart sheet, dialog sheet or macro sheet: kept whole.
            sheet.kept_part = Some(KeptSheet {
                part: part.clone(),
                kind: rel.kind.clone(),
                content_type: overrides.get(&format!("/{part}")).cloned().unwrap_or_default(),
            });
            book.sheets.push(sheet);
            continue;
        }
        regenerated_parts.insert(part.clone());
        let xml = text(&part).ok_or_else(|| format!("{part} is missing from the file"))?;
        let sheet_rels = parse_rels(&text(&rels_path(&part)).unwrap_or_default());
        regenerated_parts.insert(rels_path(&part));
        sheet.raw.part = part.clone();
        read_sheet(
            &xml,
            &part,
            &sheet_rels,
            &strings,
            dynamic_cm,
            &mut sheet,
            &mut book,
            index,
            &text,
            &mut regenerated_parts,
        )?;
        book.sheets.push(sheet);
    }
    if book.sheets.is_empty() {
        return Err("the workbook has no sheets".to_owned());
    }
    book.active_sheet = book.active_sheet.min(book.sheets.len() - 1);

    // Relationships of the workbook that are kept: themes, external links, pivot caches, VBA, connections.
    book.kept.workbook_rels = workbook_rels
        .iter()
        .filter(|r| {
            let regenerated = REGENERATED.contains(&r.kind.as_str());
            let unused_metadata = r.kind == REL_METADATA && book.dynamic_cm.is_none() && !rich_metadata;
            !(regenerated || unused_metadata)
        })
        .filter(|r| {
            r.kind != REL_WORKSHEET
                && !book.sheets.iter().any(|s| {
                    s.kept_part
                        .as_ref()
                        .is_some_and(|k| resolve(&workbook_part, &r.target) == k.part)
                })
        })
        .cloned()
        .collect();
    // Every other part is kept as bytes, with its content type.
    for path in &order {
        if regenerated_parts.contains(path) {
            continue;
        }
        if let Some(bytes) = files.get(path) {
            book.kept.parts.push((path.clone(), Arc::new(bytes.clone())));
            if let Some(ct) = overrides.get(&format!("/{path}")) {
                book.kept.overrides.push((format!("/{path}"), ct.clone()));
            }
        }
    }
    crate::pivot::xml::attach(&mut book);
    // Formulas read from a file are computed once now, except the ones that cannot be.
    crate::objects::geometry::set_metrics(&mut book);
    Ok(book)
}

/// Text from bytes, with a byte order mark removed.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub(crate) fn read_theme(xml: &str, styles: &mut Styles) {
    let Ok(doc) = Document::parse(xml) else { return };
    let order = [
        "dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ];
    let Some(scheme) = doc.descendants().find(|n| n.has_tag_name((DRAWING_NS, "clrScheme"))) else {
        return;
    };
    let mut colors = Vec::new();
    for name in order {
        let value = scheme
            .children()
            .find(|n| n.has_tag_name((DRAWING_NS, name)))
            .and_then(|n| n.children().find(Node::is_element))
            .and_then(|c| {
                c.attribute("val")
                    .filter(|_| c.tag_name().name() == "srgbClr")
                    .or_else(|| c.attribute("lastClr"))
            })
            .and_then(|v| u32::from_str_radix(v, 16).ok())
            .unwrap_or(0);
        colors.push(value);
    }
    styles.theme = colors;
    styles.theme_font = doc
        .descendants()
        .find(|n| n.has_tag_name((DRAWING_NS, "minorFont")))
        .and_then(|f| f.children().find(|n| n.has_tag_name((DRAWING_NS, "latin"))))
        .and_then(|l| l.attribute("typeface"))
        .map(str::to_owned);
    styles.theme_major_font = doc
        .descendants()
        .find(|n| n.has_tag_name((DRAWING_NS, "majorFont")))
        .and_then(|f| f.children().find(|n| n.has_tag_name((DRAWING_NS, "latin"))))
        .and_then(|l| l.attribute("typeface"))
        .map(str::to_owned);
}

/// The calculation settings of a `calcPr` element: the mode, whether circular references are iterated and
/// how far, and whether numbers are kept as shown. The other attributes are kept as they were.
fn calc_settings(n: Node) -> CalcSettings {
    let flag = |name: &str, default: bool| match n.attribute(name) {
        Some("1") | Some("true") => true,
        Some("0") | Some("false") => false,
        _ => default,
    };
    CalcSettings {
        automatic: n.attribute("calcMode") != Some("manual"),
        iterate: flag("iterate", false),
        iterate_count: n
            .attribute("iterateCount")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ITERATE_COUNT),
        iterate_delta: n
            .attribute("iterateDelta")
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ITERATE_DELTA),
        full_precision: flag("fullPrecision", true),
        attrs: attrs_of(
            n,
            &[
                "calcMode",
                "fullCalcOnLoad",
                "iterate",
                "iterateCount",
                "iterateDelta",
                "fullPrecision",
            ],
        ),
    }
}

pub(super) fn bool_attr(n: Node, name: &str) -> bool {
    matches!(n.attribute(name), Some("1") | Some("true"))
}

/// The `outlinePr` element of `sheetPr`: where summary rows and columns sit.
fn read_outline_settings(n: Node) -> OutlineSettings {
    let flag = |name: &str, default: bool| match n.attribute(name) {
        Some(v) => v == "1" || v == "true",
        None => default,
    };
    OutlineSettings {
        summary_below: flag("summaryBelow", true),
        summary_right: flag("summaryRight", true),
        apply_styles: flag("applyStyles", false),
        show_symbols: flag("showOutlineSymbols", true),
    }
}

/// A differential format element (`dxf`, or the `x14:dxf` of an extension rule), with its XML kept.
pub(super) fn read_dxf(xml: &str, d: Node) -> Dxf {
    let font = child(d, "font");
    let flag =
        |n: Option<Node>| n.map(|b| !matches!(b.attribute("val"), Some("0") | Some("false")));
    Dxf {
        font_color: font.and_then(|f| child(f, "color")).map(color),
        bold: flag(font.and_then(|f| child(f, "b"))),
        italic: flag(font.and_then(|f| child(f, "i"))),
        strike: flag(font.and_then(|f| child(f, "strike"))),
        underline: font
            .and_then(|f| child(f, "u"))
            .map(|u| Underline::parse(u.attribute("val").unwrap_or("single"))),
        fill: child(d, "fill")
            .and_then(|f| child(f, "patternFill"))
            .and_then(|p| child(p, "bgColor").or_else(|| child(p, "fgColor")))
            .map(color),
        num_fmt: child(d, "numFmt").and_then(|n| n.attribute("formatCode")).map(str::to_owned),
        border: child(d, "border")
            .and_then(|b| b.descendants().find(|n| n.tag_name().name() == "color"))
            .map(color),
        raw: Some(raw(xml, d).to_owned()),
    }
}

pub(super) fn color(n: Node) -> Color {
    if bool_attr(n, "auto") {
        return Color::Auto;
    }
    if let Some(rgb) = n.attribute("rgb") {
        return Color::parse_hex(rgb).unwrap_or(Color::Auto);
    }
    let tint = n.attribute("tint").and_then(|t| t.parse().ok()).unwrap_or(0.0);
    if let Some(theme) = n.attribute("theme").and_then(|t| t.parse().ok()) {
        return Color::Theme(theme, tint);
    }
    if let Some(i) = n.attribute("indexed").and_then(|t| t.parse().ok()) {
        return Color::Indexed(i);
    }
    Color::Auto
}

pub(super) fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn read_font(n: Node) -> Font {
    let val = |name: &str| child(n, name).and_then(|c| c.attribute("val"));
    let flag = |name: &str| {
        child(n, name).is_some_and(|c| !matches!(c.attribute("val"), Some("0") | Some("false")))
    };
    Font {
        name: val("name").unwrap_or("Calibri").to_owned(),
        size: val("sz").and_then(|v| v.parse().ok()).unwrap_or(11.0),
        bold: flag("b"),
        italic: flag("i"),
        underline: child(n, "u")
            .map_or(Underline::None, |u| Underline::parse(u.attribute("val").unwrap_or("single"))),
        strike: flag("strike"),
        color: child(n, "color").map_or(Color::Auto, color),
        vert_align: val("vertAlign").map(str::to_owned),
        family: val("family").and_then(|v| v.parse().ok()),
        scheme: val("scheme").map(str::to_owned),
        charset: val("charset").and_then(|v| v.parse().ok()),
    }
}

fn read_edge(n: Option<Node>) -> Edge {
    match n {
        Some(n) => Edge {
            style: n.attribute("style").unwrap_or("").to_owned(),
            color: child(n, "color").map_or(Color::Auto, color),
        },
        None => Edge::default(),
    }
}

pub(crate) fn read_styles(xml: &str, styles: &mut Styles) -> Result<(), String> {
    let doc = Document::parse(xml).map_err(|e| format!("the styles part is damaged: {e}"))?;
    let root = doc.root_element();
    styles.namespaces = namespaces_of(root);
    styles.fonts.clear();
    styles.fills.clear();
    styles.borders.clear();
    styles.xfs.clear();
    styles.style_xfs.clear();
    styles.cell_styles.clear();
    for section in root.children().filter(Node::is_element) {
        match section.tag_name().name() {
            "numFmts" => {
                for f in section.children().filter(Node::is_element) {
                    if let (Some(id), Some(code)) = (
                        f.attribute("numFmtId").and_then(|v| v.parse().ok()),
                        f.attribute("formatCode"),
                    ) {
                        styles.num_fmts.insert(id, code.to_owned());
                    }
                }
            }
            "fonts" => {
                styles.fonts = section.children().filter(Node::is_element).map(read_font).collect()
            }
            "fills" => {
                for f in section.children().filter(Node::is_element) {
                    let fill = match child(f, "patternFill") {
                        Some(p) => Fill {
                            pattern: p.attribute("patternType").unwrap_or("none").to_owned(),
                            fg: child(p, "fgColor").map_or(Color::Auto, color),
                            bg: child(p, "bgColor").map_or(Color::Auto, color),
                            raw: None,
                        },
                        None => {
                            // A gradient: kept, and drawn as its first stop.
                            let first = f
                                .descendants()
                                .find(|n| n.tag_name().name() == "color")
                                .map_or(Color::Auto, color);
                            Fill {
                                pattern: "gradient".to_owned(),
                                fg: first,
                                bg: Color::Auto,
                                raw: Some(raw(xml, f).to_owned()),
                            }
                        }
                    };
                    styles.fills.push(fill);
                }
            }
            "borders" => {
                for b in section.children().filter(Node::is_element) {
                    styles.borders.push(Border {
                        left: read_edge(child(b, "left").or_else(|| child(b, "start"))),
                        right: read_edge(child(b, "right").or_else(|| child(b, "end"))),
                        top: read_edge(child(b, "top")),
                        bottom: read_edge(child(b, "bottom")),
                        diagonal: read_edge(child(b, "diagonal")),
                        diagonal_up: bool_attr(b, "diagonalUp"),
                        diagonal_down: bool_attr(b, "diagonalDown"),
                    });
                }
            }
            "cellXfs" => {
                styles.xfs = section.children().filter(Node::is_element).map(read_xf).collect();
            }
            "cellStyleXfs" => {
                styles.style_xfs =
                    section.children().filter(Node::is_element).map(read_xf).collect();
            }
            "cellStyles" => {
                styles.cell_styles = section
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "cellStyle")
                    .map(|n| read_cell_style(xml, n))
                    .collect();
            }
            "dxfs" => {
                for d in section.children().filter(Node::is_element) {
                    styles.dxfs.push(read_dxf(xml, d));
                }
            }
            "tableStyles" => styles.raw_table_styles = Some(raw(xml, section).to_owned()),
            "colors" => styles.raw_colors = Some(raw(xml, section).to_owned()),
            "extLst" => styles.raw_ext = Some(raw(xml, section).to_owned()),
            _ => {}
        }
    }
    if styles.fonts.is_empty() {
        styles.fonts.push(Font::default());
    }
    if styles.fills.len() < 2 {
        styles.fills =
            vec![Fill::none(), Fill { pattern: "gray125".to_owned(), ..Default::default() }];
    }
    if styles.borders.is_empty() {
        styles.borders.push(Border::default());
    }
    if styles.xfs.is_empty() {
        styles.xfs.push(Xf::default());
    }
    if styles.style_xfs.is_empty() {
        styles.style_xfs.push(Xf::default());
    }
    if !styles.cell_styles.iter().any(|s| s.builtin_id == Some(0)) {
        styles.cell_styles.push(CellStyle::normal());
    }
    Ok(())
}

/// One `xf` of `cellXfs` or `cellStyleXfs`.
fn read_xf(x: Node) -> Xf {
    let num = |name: &str| x.attribute(name).and_then(|v| v.parse().ok()).unwrap_or(0);
    let align = child(x, "alignment").map_or(Alignment::default(), |a| Alignment {
        horizontal: a.attribute("horizontal").filter(|h| *h != "general").map(str::to_owned),
        vertical: a.attribute("vertical").filter(|v| *v != "bottom").map(str::to_owned),
        wrap: bool_attr(a, "wrapText"),
        indent: a.attribute("indent").and_then(|v| v.parse().ok()).unwrap_or(0),
        rotation: a.attribute("textRotation").and_then(|v| v.parse().ok()).unwrap_or(0),
        shrink: bool_attr(a, "shrinkToFit"),
        reading_order: a.attribute("readingOrder").and_then(|v| v.parse().ok()).unwrap_or(0),
        extra: attrs_of(
            a,
            &[
                "horizontal",
                "vertical",
                "wrapText",
                "indent",
                "textRotation",
                "shrinkToFit",
                "readingOrder",
            ],
        ),
    });
    let protection = child(x, "protection");
    Xf {
        num_fmt: num("numFmtId"),
        font: num("fontId"),
        fill: num("fillId"),
        border: num("borderId"),
        xf_id: num("xfId"),
        align,
        locked: protection.and_then(|p| p.attribute("locked")).map(|v| v == "1" || v == "true"),
        hidden: protection.and_then(|p| p.attribute("hidden")).map(|v| v == "1" || v == "true"),
        quote_prefix: bool_attr(x, "quotePrefix"),
        pivot_button: bool_attr(x, "pivotButton"),
        applies: x
            .attributes()
            .filter(|a| a.name().starts_with("apply"))
            .map(|a| (a.name().to_owned(), a.value().to_owned()))
            .collect(),
    }
}

/// One `cellStyle`: its name, the entry of `cellStyleXfs` it is, and the attributes and children kept.
fn read_cell_style(xml: &str, n: Node) -> CellStyle {
    let known = ["name", "xfId", "builtinId", "iLevel", "hidden", "customBuiltin"];
    CellStyle {
        name: n.attribute("name").unwrap_or("").to_owned(),
        xf_id: n.attribute("xfId").and_then(|v| v.parse().ok()).unwrap_or(0),
        builtin_id: n.attribute("builtinId").and_then(|v| v.parse().ok()),
        i_level: n.attribute("iLevel").and_then(|v| v.parse().ok()),
        hidden: bool_attr(n, "hidden"),
        custom_builtin: bool_attr(n, "customBuiltin"),
        other: attrs_of(n, &known),
        children: n.children().filter(Node::is_element).map(|c| raw(xml, c)).collect(),
    }
}

/// The text of a shared string or inline string: its runs joined, phonetic guides left out. The XML is
/// kept when it has formatting runs or phonetic guides, which PHONETIC reads and the writer gives back.
fn string_item(xml: &str, si: Node) -> (Arc<str>, Option<Arc<str>>) {
    let mut text = String::new();
    let mut rich = false;
    for c in si.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "t" => text.push_str(c.text().unwrap_or("")),
            "r" => {
                rich = true;
                if let Some(t) = child(c, "t") {
                    text.push_str(t.text().unwrap_or(""));
                }
            }
            "rPh" | "phoneticPr" => rich = true,
            _ => {}
        }
    }
    let text = unescape_office(&text);
    (Arc::from(text.as_str()), rich.then(|| Arc::from(raw(xml, si))))
}

/// The shared strings: each one's text, and its rich text runs as XML when it has them.
type Strings = Vec<(Arc<str>, Option<Arc<str>>)>;

fn read_strings(xml: &str) -> Result<Strings, String> {
    let doc =
        Document::parse(xml).map_err(|e| format!("the shared strings part is damaged: {e}"))?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name((MAIN_NS, "si")))
        .map(|si| string_item(xml, si))
        .collect())
}

/// Which `cm` index is a dynamic array, and whether that is all the metadata part holds.
fn read_metadata(xml: &str) -> (Option<u32>, bool) {
    let Ok(doc) = Document::parse(xml) else { return (None, false) };
    let root = doc.root_element();
    let types: Vec<String> = root
        .descendants()
        .filter(|n| n.tag_name().name() == "metadataType")
        .map(|n| n.attribute("name").unwrap_or("").to_owned())
        .collect();
    let Some(dynamic_type) = types.iter().position(|t| t == "XLDAPR") else { return (None, false) };
    let cell_metadata = root.children().find(|n| n.tag_name().name() == "cellMetadata");
    let mut cm = None;
    if let Some(cells) = cell_metadata {
        for (i, bk) in cells.children().filter(|n| n.tag_name().name() == "bk").enumerate() {
            if bk.children().filter(|n| n.tag_name().name() == "rc").any(|rc| {
                rc.attribute("t").and_then(|t| t.parse::<usize>().ok()) == Some(dynamic_type + 1)
            }) {
                cm = Some(i as u32 + 1);
                break;
            }
        }
    }
    let only = types.len() == 1 && !root.children().any(|n| n.tag_name().name() == "valueMetadata");
    (cm, only)
}

/// Read a formula from a file, keeping it frozen when it cannot be computed here.
pub(crate) fn file_formula(text: &str, dynamic: bool) -> Formula {
    match Formula::parse_kept(text) {
        Ok(mut f) => {
            f.dynamic = dynamic;
            let mut unknown = false;
            f.expr.walk(&mut |e| match e {
                Expr::External(_) => unknown = true,
                Expr::Call(name, _)
                    if crate::functions::find(name).is_none() && !is_name_call(name) =>
                {
                    unknown = true;
                }
                // A function that needs a service of Excel's keeps the value the file saved.
                Expr::Call(name, _) if crate::functions::find(name).is_some_and(|f| f.offline) => {
                    unknown = true;
                }
                _ => {}
            });
            f.frozen = unknown;
            f
        }
        Err(_) => Formula {
            text: Arc::from(text),
            expr: Arc::new(Expr::Error(ErrorKind::Name)),
            dynamic,
            array: None,
            array_member: None,
            frozen: true,
            attrs: None,
        },
    }
}

/// An `autoFilter` element: its range, what each column keeps and how it was last sorted.
pub(crate) fn read_auto_filter(xml: &str, node: Node) -> Option<AutoFilter> {
    let range = node.attribute("ref").and_then(Range::parse)?;
    let mut columns = BTreeMap::new();
    for fc in node.children().filter(|n| n.has_tag_name((MAIN_NS, "filterColumn"))) {
        let id: u32 = fc.attribute("colId").and_then(|v| v.parse().ok()).unwrap_or(0);
        columns.insert(id, read_filter_column(xml, fc));
    }
    let sort = child(node, "sortState").and_then(read_sort_state);
    Some(AutoFilter { range, columns, sort })
}

/// What one `filterColumn` keeps. A filter this version has no variant for is kept as its XML.
pub(crate) fn read_filter_column(xml: &str, fc: Node) -> FilterCriteria {
    let number = |n: Node, name: &str| n.attribute(name).and_then(|v| v.parse::<f64>().ok());
    // Excel keeps a filter that names parts of the date tree in an extension of the column.
    let in_extension =
        || fc.descendants().find(|n| n.has_tag_name((crate::xlsx::write::RICHDATA2_NS, "filters")));
    if let Some(filters) = child(fc, "filters").or_else(in_extension) {
        let values: Vec<String> = filters
            .children()
            .filter(|n| n.tag_name().name() == "filter")
            .filter_map(|f| f.attribute("val").map(str::to_owned))
            .collect();
        let blank = bool_attr(filters, "blank");
        let groups: Vec<DateGroup> = filters
            .children()
            .filter(|n| n.tag_name().name() == "dateGroupItem")
            .filter_map(|g| {
                let part = |name: &str| g.attribute(name).and_then(|v| v.parse::<u32>().ok());
                Some(DateGroup {
                    year: g.attribute("year")?.parse().ok()?,
                    month: part("month"),
                    day: part("day"),
                    hour: part("hour"),
                    minute: part("minute"),
                    second: part("second"),
                    grouping: DateGrouping::parse(g.attribute("dateTimeGrouping")?)?,
                })
            })
            .collect();
        return if groups.is_empty() {
            FilterCriteria::Values { values, blank }
        } else {
            FilterCriteria::Dates { values, groups, blank }
        };
    }
    if let Some(custom) = child(fc, "customFilters") {
        return FilterCriteria::Custom {
            conditions: custom
                .children()
                .filter(|n| n.tag_name().name() == "customFilter")
                .map(|f| {
                    (
                        f.attribute("operator").unwrap_or("equal").to_owned(),
                        f.attribute("val").unwrap_or("").to_owned(),
                    )
                })
                .collect(),
            and: bool_attr(custom, "and"),
        };
    }
    if let Some(top) = child(fc, "top10") {
        if let Some(value) = number(top, "val") {
            return FilterCriteria::Top10 {
                top: top.attribute("top") != Some("0"),
                percent: bool_attr(top, "percent"),
                value,
                threshold: number(top, "filterVal"),
            };
        }
    }
    if let Some(dynamic) = child(fc, "dynamicFilter") {
        if let Some(kind) = dynamic.attribute("type") {
            return FilterCriteria::Dynamic {
                kind: kind.to_owned(),
                value: number(dynamic, "val"),
                max: number(dynamic, "maxVal"),
            };
        }
    }
    if let Some(color) = child(fc, "colorFilter") {
        if let Some(dxf) = color.attribute("dxfId").and_then(|v| v.parse().ok()) {
            return FilterCriteria::Color { cell: color.attribute("cellColor") != Some("0"), dxf };
        }
    }
    if let Some(icon) = child(fc, "iconFilter") {
        if let Some(set) = icon.attribute("iconSet") {
            let id = icon.attribute("iconId").and_then(|v| v.parse().ok()).unwrap_or(0);
            return FilterCriteria::Icon { set: set.to_owned(), id };
        }
    }
    FilterCriteria::Raw(raw(xml, fc).to_owned())
}

/// A `sortState` element: the sorted range and its levels.
pub(crate) fn read_sort_state(node: Node) -> Option<SortState> {
    let range = node.attribute("ref").and_then(Range::parse)?;
    let conditions = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "sortCondition")
        .filter_map(|c| {
            Some(SortCondition {
                range: c.attribute("ref").and_then(Range::parse)?,
                descending: bool_attr(c, "descending"),
                by: c.attribute("sortBy").and_then(SortBy::parse).unwrap_or_default(),
                custom_list: c.attribute("customList").map(str::to_owned),
                dxf: c.attribute("dxfId").and_then(|v| v.parse().ok()),
                icon_set: c.attribute("iconSet").map(str::to_owned),
                icon_id: c.attribute("iconId").and_then(|v| v.parse().ok()),
            })
        })
        .collect();
    Some(SortState {
        range,
        conditions,
        columns: bool_attr(node, "columnSort"),
        case_sensitive: bool_attr(node, "caseSensitive"),
        attrs: attrs_of(node, &["ref", "columnSort", "caseSensitive"]),
    })
}

/// The first `sheetView` into the model, and any other as XML.
fn read_sheet_views(xml: &str, section: Node, sheet: &mut Sheet) {
    let mut views =
        section.children().filter(|n| n.is_element() && n.tag_name().name() == "sheetView");
    let Some(view) = views.next() else { return };
    sheet.view.others = views.map(|v| raw(xml, v).to_owned()).collect();
    let flag = |name: &str, default: bool| match view.attribute(name) {
        Some(v) => v == "1" || v == "true",
        None => default,
    };
    let number = |name: &str| view.attribute(name).and_then(|z| z.parse::<u32>().ok());
    sheet.show_grid = flag("showGridLines", true);
    sheet.zoom = number("zoomScale").unwrap_or(100);
    let v = &mut sheet.view;
    v.kind = view.attribute("view").and_then(ViewKind::parse).unwrap_or_default();
    v.zoom_normal = number("zoomScaleNormal");
    v.zoom_layout = number("zoomScalePageLayoutView");
    v.zoom_page_break = number("zoomScaleSheetLayoutView");
    v.show_headings = flag("showRowColHeaders", true);
    v.show_zeros = flag("showZeros", true);
    v.show_formulas = flag("showFormulas", false);
    v.right_to_left = flag("rightToLeft", false);
    v.top_left = view.attribute("topLeftCell").and_then(CellRef::parse);
    v.extra = attrs_of(view, VIEW_MODELLED);
    if let Some(pane) = child(view, "pane") {
        let split =
            |name: &str| pane.attribute(name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let top_left = pane.attribute("topLeftCell").and_then(CellRef::parse);
        v.active_pane = pane.attribute("activePane").map(str::to_owned);
        match pane.attribute("state") {
            Some("frozen") | Some("frozenSplit") => {
                v.frozen_split = pane.attribute("state") == Some("frozenSplit");
                v.pane_top_left = top_left;
                sheet.frozen = Some((split("ySplit") as u32, split("xSplit") as u32));
            }
            _ => v.split = Some(Split { x: split("xSplit"), y: split("ySplit"), top_left }),
        }
    }
    for s in view.children().filter(|n| n.is_element() && n.tag_name().name() == "selection") {
        let ranges: Vec<Range> = s
            .attribute("sqref")
            .unwrap_or("A1")
            .split_whitespace()
            .filter_map(Range::parse)
            .collect();
        let active = s
            .attribute("activeCell")
            .and_then(CellRef::parse)
            .or_else(|| ranges.first().map(|r| r.first))
            .unwrap_or(CellRef::new(0, 0));
        sheet.view.selections.push(Selection {
            pane: s.attribute("pane").map(str::to_owned),
            active,
            ranges,
        });
    }
}

/// The `sheetView` attributes the model reads; every other one is kept.
const VIEW_MODELLED: &[&str] = &[
    "tabSelected",
    "showGridLines",
    "zoomScale",
    "zoomScaleNormal",
    "zoomScalePageLayoutView",
    "zoomScaleSheetLayoutView",
    "view",
    "showRowColHeaders",
    "showZeros",
    "showFormulas",
    "rightToLeft",
    "topLeftCell",
    "workbookViewId",
];

/// A call that may be a LAMBDA in Name Manager rather than an unknown function: a name with no dot.
fn is_name_call(name: &str) -> bool {
    !name.contains('.') && !name.starts_with('_')
}

#[allow(clippy::too_many_arguments)]
fn read_sheet(
    xml: &str,
    part: &str,
    rels: &[Relationship],
    strings: &[(Arc<str>, Option<Arc<str>>)],
    dynamic_cm: Option<u32>,
    sheet: &mut Sheet,
    book: &mut Workbook,
    index: usize,
    text: &dyn Fn(&str) -> Option<String>,
    regenerated: &mut HashSet<String>,
) -> Result<(), String> {
    let doc = Document::parse(xml).map_err(|e| format!("{part} is damaged: {e}"))?;
    let root = doc.root_element();
    sheet.raw.namespaces = namespaces_of(root);
    let rel_by_id: HashMap<&str, &Relationship> = rels.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut kept_rels: Vec<Relationship> = Vec::new();
    let mut shared: HashMap<u32, (CellRef, Arc<Expr>)> = HashMap::new();
    let mut dynamic_ranges: Vec<Range> = Vec::new();
    for section in root.children().filter(Node::is_element) {
        let name = section.tag_name().name();
        match name {
            "sheetPr" => {
                if let Some(tab) = child(section, "tabColor") {
                    sheet.tab_color = Some(color(tab));
                }
                if let Some(outline) = child(section, "outlinePr") {
                    sheet.outline = read_outline_settings(outline);
                }
                crate::pages::xml::read_sheet_pr(&mut sheet.page, section);
                sheet.raw.elements.push((name.to_owned(), raw(xml, section).to_owned()));
            }
            "printOptions" | "pageMargins" | "pageSetup" | "headerFooter" | "rowBreaks"
            | "colBreaks" => {
                crate::pages::xml::read_element(&mut sheet.page, section);
            }
            "dimension" => {}
            "sheetViews" => read_sheet_views(xml, section, sheet),
            "sheetFormatPr" => {
                sheet.default_row_height = section
                    .attribute("defaultRowHeight")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(15.0);
                sheet.default_col_width =
                    section.attribute("defaultColWidth").and_then(|v| v.parse().ok());
                // The deepest outline levels are worked out again when the sheet is written.
                sheet.raw.format_attrs = attrs_of(
                    section,
                    &["defaultRowHeight", "defaultColWidth", "outlineLevelRow", "outlineLevelCol"],
                );
            }
            "cols" => {
                for c in section.children().filter(Node::is_element) {
                    let first: u32 = c.attribute("min").and_then(|v| v.parse().ok()).unwrap_or(1);
                    let last: u32 =
                        c.attribute("max").and_then(|v| v.parse().ok()).unwrap_or(first);
                    sheet.columns.push(ColumnInfo {
                        first: first.saturating_sub(1),
                        last: last.saturating_sub(1).min(crate::coord::MAX_COLS - 1),
                        width: c.attribute("width").and_then(|v| v.parse().ok()),
                        hidden: bool_attr(c, "hidden"),
                        style: c.attribute("style").and_then(|v| v.parse().ok()).unwrap_or(0),
                        custom_width: bool_attr(c, "customWidth"),
                        outline: c
                            .attribute("outlineLevel")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0),
                        collapsed: bool_attr(c, "collapsed"),
                        best_fit: bool_attr(c, "bestFit"),
                    });
                }
            }
            "sheetData" => {
                let mut next_row = 0u32;
                for row in section.children().filter(|n| n.has_tag_name((MAIN_NS, "row"))) {
                    let r: u32 = row
                        .attribute("r")
                        .and_then(|v| v.parse::<u32>().ok())
                        .map_or(next_row, |v| v - 1);
                    next_row = r + 1;
                    let info = RowInfo {
                        height: if bool_attr(row, "customHeight") || row.attribute("ht").is_some() {
                            row.attribute("ht").and_then(|v| v.parse().ok())
                        } else {
                            None
                        },
                        hidden: bool_attr(row, "hidden"),
                        style: if bool_attr(row, "customFormat") {
                            row.attribute("s").and_then(|v| v.parse().ok())
                        } else {
                            None
                        },
                        outline: row
                            .attribute("outlineLevel")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0),
                        collapsed: bool_attr(row, "collapsed"),
                        attrs: attrs_of(
                            row,
                            &[
                                "r",
                                "spans",
                                "ht",
                                "customHeight",
                                "hidden",
                                "s",
                                "customFormat",
                                "outlineLevel",
                                "collapsed",
                            ],
                        ),
                    };
                    if info != RowInfo::default() {
                        sheet.rows.insert(r, info);
                    }
                    let mut next_col = 0u32;
                    for c in row.children().filter(|n| n.has_tag_name((MAIN_NS, "c"))) {
                        let at = c
                            .attribute("r")
                            .and_then(CellRef::parse)
                            .unwrap_or(CellRef::new(r, next_col));
                        next_col = at.col + 1;
                        let style = c.attribute("s").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let kind = c.attribute("t").unwrap_or("n");
                        let v_text = child(c, "v").and_then(|v| v.text()).unwrap_or("");
                        let mut rich = None;
                        let value = match kind {
                            "s" => match v_text
                                .trim()
                                .parse::<usize>()
                                .ok()
                                .and_then(|i| strings.get(i))
                            {
                                Some((t, r)) => {
                                    rich = r.clone();
                                    Value::Text(t.clone())
                                }
                                None => Value::Empty,
                            },
                            "str" => Value::text(unescape_office(v_text)),
                            "inlineStr" => match child(c, "is") {
                                Some(is) => {
                                    let (t, r) = string_item(xml, is);
                                    rich = r;
                                    Value::Text(t)
                                }
                                None => Value::Empty,
                            },
                            "b" => Value::Bool(v_text.trim() == "1"),
                            "e" => {
                                Value::Error(ErrorKind::parse(v_text).unwrap_or(ErrorKind::Value))
                            }
                            _ => match v_text.trim() {
                                "" => Value::Empty,
                                t => t.parse::<f64>().map(Value::Number).unwrap_or(Value::Empty),
                            },
                        };
                        let cm = c.attribute("cm").and_then(|v| v.parse::<u32>().ok());
                        let is_dynamic = cm.is_some() && cm == dynamic_cm;
                        let mut formula = None;
                        if let Some(f) = child(c, "f") {
                            let f_text = f.text().unwrap_or("");
                            match f.attribute("t") {
                                Some("shared") => {
                                    let si: u32 =
                                        f.attribute("si").and_then(|v| v.parse().ok()).unwrap_or(0);
                                    if !f_text.is_empty() {
                                        let made = file_formula(f_text, false);
                                        shared.insert(si, (at, made.expr.clone()));
                                        formula = Some(made);
                                    } else if let Some((origin, expr)) = shared.get(&si) {
                                        let moved = crate::formula::shifted(
                                            expr,
                                            at.row as i64 - origin.row as i64,
                                            at.col as i64 - origin.col as i64,
                                        );
                                        let mut made = Formula::from_expr(moved, false);
                                        made.text = Arc::from(
                                            crate::formula::print::print(&made.expr, true).as_str(),
                                        );
                                        formula = Some(made);
                                    }
                                }
                                Some("array") => {
                                    let range = f
                                        .attribute("ref")
                                        .and_then(Range::parse)
                                        .unwrap_or(Range::cell(at));
                                    let mut made = file_formula(f_text, is_dynamic);
                                    if is_dynamic {
                                        dynamic_ranges.push(range);
                                    } else {
                                        made.array = Some(range);
                                        sheet.array_formulas.push(range);
                                    }
                                    formula = Some(made);
                                }
                                Some("dataTable") => {
                                    let cell =
                                        |name: &str| f.attribute(name).and_then(CellRef::parse);
                                    sheet.data_tables.push(DataTable {
                                        range: f
                                            .attribute("ref")
                                            .and_then(Range::parse)
                                            .unwrap_or(Range::cell(at)),
                                        two_d: bool_attr(f, "dt2D"),
                                        row_input: bool_attr(f, "dtr"),
                                        r1: cell("r1"),
                                        r2: cell("r2"),
                                        attrs: attrs_of(
                                            f,
                                            &["t", "ref", "dt2D", "dtr", "r1", "r2"],
                                        ),
                                    });
                                }
                                _ => {
                                    if !f_text.is_empty() {
                                        formula = Some(file_formula(f_text, is_dynamic));
                                    }
                                }
                            }
                        }
                        if let (Some(made), Some(f)) = (&mut formula, child(c, "f")) {
                            let kept = attrs_of(f, &["t", "ref", "si"]);
                            if !kept.is_empty() {
                                made.attrs = Some(Arc::from(attrs_text(&kept).as_str()));
                            }
                        }
                        let extra_attrs = attrs_of(c, &["r", "s", "t", "cm"]);
                        let extra = (!extra_attrs.is_empty())
                            .then(|| Arc::from(attrs_text(&extra_attrs).as_str()));
                        sheet
                            .cells
                            .insert((at.row, at.col), Cell { value, formula, style, rich, extra });
                    }
                }
            }
            "mergeCells" => {
                for m in section.children().filter(Node::is_element) {
                    if let Some(range) = m.attribute("ref").and_then(Range::parse) {
                        sheet.merges.push(range);
                    }
                }
            }
            "conditionalFormatting" => {
                sheet.conditional.push(super::cf::read_conditional_formatting(xml, section))
            }
            "extLst" => {
                // The conditional formats of the extension join the sheet's; the other extensions are kept.
                if let Some(rest) =
                    super::cf::read_extension_list(xml, section, sheet, &mut book.styles)
                {
                    sheet.raw.elements.push((name.to_owned(), rest));
                }
            }
            "dataValidations" => {
                for dv in section.children().filter(|n| n.has_tag_name((MAIN_NS, "dataValidation")))
                {
                    sheet.validations.push(read_validation(dv));
                }
            }
            "hyperlinks" => {
                for h in section.children().filter(Node::is_element) {
                    let Some(range) = h.attribute("ref").and_then(Range::parse) else { continue };
                    let target = match h.attribute((REL_NS, "id")).and_then(|id| rel_by_id.get(id))
                    {
                        Some(rel) => {
                            let mut target = rel.target.clone();
                            if let Some(loc) = h.attribute("location") {
                                target = format!("{target}#{loc}");
                            }
                            LinkTarget::External(target)
                        }
                        None => {
                            LinkTarget::Location(h.attribute("location").unwrap_or("").to_owned())
                        }
                    };
                    sheet.links.push(Hyperlink {
                        range,
                        target,
                        display: h.attribute("display").map(str::to_owned),
                        tooltip: h.attribute("tooltip").map(str::to_owned),
                    });
                }
            }
            "autoFilter" => {
                if let Some(filter) = read_auto_filter(xml, section) {
                    sheet.filter = Some(filter);
                }
            }
            "legacyDrawing" => {
                // The notes' shapes: kept unless the notes change.
                sheet.raw.elements.push((name.to_owned(), raw(xml, section).to_owned()));
            }
            "tableParts" => {
                for tp in section.children().filter(Node::is_element) {
                    let Some(rel) = tp.attribute((REL_NS, "id")).and_then(|id| rel_by_id.get(id))
                    else {
                        continue;
                    };
                    let table_part = resolve(part, &rel.target);
                    if let Some(table_xml) = text(&table_part) {
                        if let Some(table) = read_table(&table_xml, &table_part, index) {
                            book.tables.push(table);
                            regenerated.insert(table_part);
                        }
                    }
                }
            }
            _ => sheet.raw.elements.push((name.to_owned(), raw(xml, section).to_owned())),
        }
    }
    // Notes from the comments part.
    for rel in rels {
        if rel.kind == REL_COMMENTS {
            let comments_part = resolve(part, &rel.target);
            if let Some(cx) = text(&comments_part) {
                read_notes(&cx, sheet);
            }
            for vml in rels.iter().filter(|r| r.kind == REL_VML) {
                if let Some(v) = text(&resolve(part, &vml.target)) {
                    read_note_visibility(&v, sheet);
                }
            }
            // Kept as bytes; regenerated only if a note changes.
        }
        // Charts, read so they can be drawn; their parts are kept as they were.
        if rel.kind == REL_DRAWING {
            sheet.charts.extend(super::chart::read_charts(part, &rel.target, text));
            read_drawing_objects(sheet, part, &rel.target, text);
        }
        let kept = !matches!(rel.kind.as_str(), REL_HYPERLINK | REL_TABLE);
        if kept {
            kept_rels.push(rel.clone());
        }
    }
    sheet.raw.rels = kept_rels;
    crate::objects::sparkline::extract(sheet);
    // The cells a dynamic array spilled into were saved as values; they are the spill now.
    for range in dynamic_ranges {
        let anchor = (range.first.row, range.first.col);
        if range.len() > 1 {
            sheet.spills.anchors.insert(anchor, (range.rows(), range.cols()));
        }
        for at in range.cells() {
            let key = (at.row, at.col);
            if key == anchor {
                continue;
            }
            if let Some(cell) = sheet.cells.get(&key) {
                if cell.formula.is_none() {
                    let cell = sheet.cells.remove(&key).unwrap_or_default();
                    sheet.spills.owner.insert(key, anchor);
                    sheet.spills.values.insert(key, cell.value);
                    // A style on a spilled cell stays on an empty cell.
                    if cell.style != 0 {
                        sheet.cells.insert(key, Cell { style: cell.style, ..Default::default() });
                    }
                }
            } else {
                sheet.spills.owner.insert(key, anchor);
                sheet.spills.values.insert(key, Value::Empty);
            }
        }
    }
    Ok(())
}

fn read_validation(dv: Node) -> Validation {
    let text = |name: &str| child(dv, name).and_then(|n| n.text()).map(str::to_owned);
    Validation {
        ranges: dv
            .attribute("sqref")
            .unwrap_or("")
            .split_whitespace()
            .filter_map(Range::parse)
            .collect(),
        kind: dv.attribute("type").unwrap_or("none").to_owned(),
        operator: dv.attribute("operator").map(str::to_owned),
        formula1: text("formula1"),
        formula2: text("formula2"),
        allow_blank: bool_attr(dv, "allowBlank"),
        show_dropdown: !bool_attr(dv, "showDropDown"),
        prompt_title: dv.attribute("promptTitle").map(str::to_owned),
        prompt: dv.attribute("prompt").map(str::to_owned),
        error_title: dv.attribute("errorTitle").map(str::to_owned),
        error: dv.attribute("error").map(str::to_owned),
        error_style: dv.attribute("errorStyle").map(str::to_owned),
        attrs: attrs_of(
            dv,
            &[
                "sqref",
                "type",
                "operator",
                "allowBlank",
                "showDropDown",
                "promptTitle",
                "prompt",
                "errorTitle",
                "error",
                "errorStyle",
            ],
        ),
    }
}

fn read_table(xml: &str, part: &str, sheet: usize) -> Option<Table> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    let range = root.attribute("ref").and_then(Range::parse)?;
    let columns = child(root, "tableColumns")
        .map(|tc| {
            tc.children()
                .filter(Node::is_element)
                .map(|c| c.attribute("name").unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default();
    Some(Table {
        sheet,
        name: root
            .attribute("displayName")
            .or_else(|| root.attribute("name"))
            .unwrap_or("Table")
            .to_owned(),
        range,
        columns,
        header_row: root.attribute("headerRowCount") != Some("0"),
        totals_row: root.attribute("totalsRowCount").is_some_and(|v| v != "0")
            || bool_attr(root, "totalsRowShown") && root.attribute("totalsRowCount").is_some(),
        part: part.to_owned(),
        xml: xml.to_owned(),
    })
}

/// The pictures, shapes and other objects a sheet's drawing holds, read so they can be drawn and edited.
pub(crate) fn read_drawing_objects(
    sheet: &mut Sheet,
    sheet_part: &str,
    target: &str,
    text: &dyn Fn(&str) -> Option<String>,
) {
    let drawing_part = resolve(sheet_part, target);
    let Some(xml) = text(&drawing_part) else { return };
    let rels = parse_rels(&text(&rels_path(&drawing_part)).unwrap_or_default());
    let objects = crate::objects::read::read_drawing(&xml, &rels, &drawing_part);
    sheet.drawing.objects.extend(objects);
}

fn read_notes(xml: &str, sheet: &mut Sheet) {
    let Ok(doc) = Document::parse(xml) else { return };
    let root = doc.root_element();
    let authors: Vec<String> = child(root, "authors")
        .map(|a| {
            a.children()
                .filter(Node::is_element)
                .map(|n| n.text().unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default();
    let Some(list) = child(root, "commentList") else { return };
    for c in list.children().filter(Node::is_element) {
        let Some(at) = c.attribute("ref").and_then(CellRef::parse) else { continue };
        let author = c
            .attribute("authorId")
            .and_then(|v| v.parse::<usize>().ok())
            .and_then(|i| authors.get(i))
            .cloned()
            .unwrap_or_default();
        let text = child(c, "text").map(|t| string_item(xml, t).0.to_string()).unwrap_or_default();
        let attrs = attrs_of(c, &["ref", "authorId"]);
        sheet.notes.insert((at.row, at.col), Note { author, text, visible: false, attrs });
    }
}

/// Which notes the VML drawing shows all the time: a shape whose client data says `Visible`. VML is not
/// always well formed XML, so it is read as text.
fn read_note_visibility(vml: &str, sheet: &mut Sheet) {
    for shape in vml.split("<v:shape ").skip(1) {
        let shape = shape.split("</v:shape>").next().unwrap_or("");
        let number = |tag: &str| -> Option<u32> {
            let start = shape.find(&format!("<x:{tag}>"))? + tag.len() + 4;
            let end = shape[start..].find('<')? + start;
            shape[start..end].trim().parse().ok()
        };
        let (Some(row), Some(col)) = (number("Row"), number("Column")) else { continue };
        if shape.contains("<x:Visible") {
            if let Some(note) = sheet.notes.get_mut(&(row, col)) {
                note.visible = true;
            }
        }
    }
}
