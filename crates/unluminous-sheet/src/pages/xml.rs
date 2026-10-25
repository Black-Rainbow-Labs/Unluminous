//! Reading and writing the worksheet elements that hold page setup: `printOptions`, `pageMargins`,
//! `pageSetup`, `headerFooter`, `rowBreaks` and `colBreaks`, and the `pageSetUpPr` inside `sheetPr`.
//!
//! An attribute the model does not hold is kept in an `extra` list and written back, so a file Excel
//! wrote reads and writes without losing anything.

use roxmltree::Node;

use super::*;
use crate::xlsx::{attrs_of, attrs_text, escape};

fn flag(node: Node, name: &str) -> Option<bool> {
    node.attribute(name).map(|v| matches!(v, "1" | "true"))
}

fn number<T: std::str::FromStr>(node: Node, name: &str) -> Option<T> {
    node.attribute(name).and_then(|v| v.trim().parse().ok())
}

/// Read one of the page setup elements into the sheet's [`PageSetup`]. Returns false for any other element.
pub fn read_element(page: &mut PageSetup, node: Node) -> bool {
    match node.tag_name().name() {
        "printOptions" => read_print_options(page, node),
        "pageMargins" => read_margins(page, node),
        "pageSetup" => read_page_setup(page, node),
        "headerFooter" => read_header_footer(page, node),
        "rowBreaks" => page.row_breaks = read_breaks(node),
        "colBreaks" => page.col_breaks = read_breaks(node),
        _ => return false,
    }
    true
}

/// Read `pageSetUpPr` out of `sheetPr`: whether to fit to pages, and whether Excel breaks pages itself.
pub fn read_sheet_pr(page: &mut PageSetup, sheet_pr: Node) {
    let Some(setup) =
        sheet_pr.children().find(|c| c.is_element() && c.tag_name().name() == "pageSetUpPr")
    else {
        return;
    };
    page.fit_to_page = flag(setup, "fitToPage").unwrap_or(false);
    page.auto_page_breaks = flag(setup, "autoPageBreaks").unwrap_or(true);
}

fn read_print_options(page: &mut PageSetup, node: Node) {
    let o = &mut page.options;
    o.horizontal_centered = flag(node, "horizontalCentered").unwrap_or(false);
    o.vertical_centered = flag(node, "verticalCentered").unwrap_or(false);
    o.headings = flag(node, "headings").unwrap_or(false);
    o.grid_lines = flag(node, "gridLines").unwrap_or(false);
    o.grid_lines_set = flag(node, "gridLinesSet").unwrap_or(true);
    o.extra = attrs_of(
        node,
        &["horizontalCentered", "verticalCentered", "headings", "gridLines", "gridLinesSet"],
    );
}

/// A margin in inches, without the noise a binary double carries: Excel writes `1.1000000000000001` for 1.1.
fn margin(node: Node, name: &str, default: f64) -> f64 {
    number::<f64>(node, name).map_or(default, tidy)
}

/// A number of inches rounded to six places.
fn tidy(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn read_margins(page: &mut PageSetup, node: Node) {
    let defaults = Margins::default();
    page.margins = Margins {
        left: margin(node, "left", defaults.left),
        right: margin(node, "right", defaults.right),
        top: margin(node, "top", defaults.top),
        bottom: margin(node, "bottom", defaults.bottom),
        header: margin(node, "header", defaults.header),
        footer: margin(node, "footer", defaults.footer),
    };
}

fn read_page_setup(page: &mut PageSetup, node: Node) {
    page.paper_size = number(node, "paperSize").unwrap_or(1);
    page.paper_width = node.attribute("paperWidth").map(str::to_owned);
    page.paper_height = node.attribute("paperHeight").map(str::to_owned);
    page.scale = number(node, "scale").unwrap_or(100);
    page.first_page_number = number(node, "firstPageNumber").unwrap_or(1);
    page.use_first_page_number = flag(node, "useFirstPageNumber").unwrap_or(false);
    page.fit_to_width = number(node, "fitToWidth").unwrap_or(1);
    page.fit_to_height = number(node, "fitToHeight").unwrap_or(1);
    page.page_order = match node.attribute("pageOrder") {
        Some("overThenDown") => PageOrder::OverThenDown,
        _ => PageOrder::DownThenOver,
    };
    page.orientation = match node.attribute("orientation") {
        Some("portrait") => Some(Orientation::Portrait),
        Some("landscape") => Some(Orientation::Landscape),
        _ => None,
    };
    page.use_printer_defaults = flag(node, "usePrinterDefaults").unwrap_or(true);
    page.black_and_white = flag(node, "blackAndWhite").unwrap_or(false);
    page.draft = flag(node, "draft").unwrap_or(false);
    page.comments = match node.attribute("cellComments") {
        Some("atEnd") => PrintComments::AtEnd,
        Some("asDisplayed") => PrintComments::AsDisplayed,
        _ => PrintComments::None,
    };
    page.errors = match node.attribute("errors") {
        Some("blank") => PrintErrors::Blank,
        Some("dash") => PrintErrors::Dash,
        Some("NA") => PrintErrors::NotAvailable,
        _ => PrintErrors::Displayed,
    };
    page.horizontal_dpi = number(node, "horizontalDpi").unwrap_or(600);
    page.vertical_dpi = number(node, "verticalDpi").unwrap_or(600);
    page.copies = number(node, "copies").unwrap_or(1);
    page.printer_rid = node
        .attributes()
        .find(|a| a.name() == "id" && a.namespace() == Some(crate::xlsx::REL_NS))
        .map(|a| a.value().to_owned());
    page.extra = attrs_of(node, &KNOWN_PAGE_SETUP_ATTRIBUTES)
        .into_iter()
        .filter(|(name, _)| !name.ends_with(":id"))
        .collect();
}

const KNOWN_PAGE_SETUP_ATTRIBUTES: [&str; 18] = [
    "paperSize",
    "paperHeight",
    "paperWidth",
    "scale",
    "firstPageNumber",
    "fitToWidth",
    "fitToHeight",
    "pageOrder",
    "orientation",
    "usePrinterDefaults",
    "blackAndWhite",
    "draft",
    "cellComments",
    "useFirstPageNumber",
    "errors",
    "horizontalDpi",
    "verticalDpi",
    "copies",
];

fn read_header_footer(page: &mut PageSetup, node: Node) {
    let h = &mut page.header_footer;
    h.different_odd_even = flag(node, "differentOddEven").unwrap_or(false);
    h.different_first = flag(node, "differentFirst").unwrap_or(false);
    h.scale_with_doc = flag(node, "scaleWithDoc").unwrap_or(true);
    h.align_with_margins = flag(node, "alignWithMargins").unwrap_or(true);
    h.extra =
        attrs_of(node, &["differentOddEven", "differentFirst", "scaleWithDoc", "alignWithMargins"]);
    for child in node.children().filter(Node::is_element) {
        let text = child.text().unwrap_or("").to_owned();
        match child.tag_name().name() {
            "oddHeader" => h.odd_header = text,
            "oddFooter" => h.odd_footer = text,
            "evenHeader" => h.even_header = text,
            "evenFooter" => h.even_footer = text,
            "firstHeader" => h.first_header = text,
            "firstFooter" => h.first_footer = text,
            _ => {}
        }
    }
}

/// The file's `id` counts the rows (or columns) above the break, so it is the zero based index of the first
/// row (or column) after it.
fn read_breaks(node: Node) -> Vec<PageBreak> {
    node.children()
        .filter(|c| c.is_element() && c.tag_name().name() == "brk")
        .map(|b| PageBreak {
            id: number(b, "id").unwrap_or(0),
            min: number(b, "min").unwrap_or(0),
            max: number(b, "max").unwrap_or(0),
            manual: flag(b, "man").unwrap_or(false),
            pt: flag(b, "pt").unwrap_or(false),
        })
        .collect()
}

// ----------------------------------------------------------------------------------------------------
// Writing.

/// The XML of one of the page setup elements, or an empty string when the sheet needs none. `rel_prefix` is
/// the prefix the worksheet root gives the relationships namespace.
pub fn write_element(page: &PageSetup, name: &str, rel_prefix: &str) -> String {
    match name {
        "printOptions" => write_print_options(&page.options),
        "pageMargins" => write_margins(&page.margins),
        "pageSetup" => write_page_setup(page, rel_prefix),
        "headerFooter" => write_header_footer(&page.header_footer),
        "rowBreaks" => write_breaks("rowBreaks", &page.row_breaks),
        "colBreaks" => write_breaks("colBreaks", &page.col_breaks),
        _ => String::new(),
    }
}

/// `sheetPr` with the `pageSetUpPr` the setup asks for: any present is taken out and the right one put last,
/// which is where the schema puts it.
pub fn with_page_set_up_pr(sheet_pr: &str, page: &PageSetup) -> String {
    let wanted = page.fit_to_page || !page.auto_page_breaks;
    let mut body = remove_element(sheet_pr, "pageSetUpPr");
    if !wanted {
        return body;
    }
    let mut attrs = Vec::new();
    if !page.auto_page_breaks {
        attrs.push(("autoPageBreaks".to_owned(), "0".to_owned()));
    }
    if page.fit_to_page {
        attrs.push(("fitToPage".to_owned(), "1".to_owned()));
    }
    let element = format!("<pageSetUpPr{}/>", attrs_text(&attrs));
    if body.is_empty() {
        return format!("<sheetPr>{element}</sheetPr>");
    }
    if let Some(stripped) = body.strip_suffix("/>") {
        if !stripped.contains('>') {
            return format!("{stripped}>{element}</sheetPr>");
        }
    }
    match body.rfind("</sheetPr>") {
        Some(end) => {
            body.insert_str(end, &element);
            body
        }
        None => body,
    }
}

/// The XML with every element of one name taken out, whether written with a close tag or self closed.
fn remove_element(xml: &str, name: &str) -> String {
    let open = format!("<{name}");
    let mut out = xml.to_owned();
    while let Some(start) = out.find(&open) {
        let after = &out[start + open.len()..];
        if !after.starts_with([' ', '/', '>']) {
            break;
        }
        let end = match out[start..].find('>') {
            Some(gt) if out[start..start + gt].ends_with('/') => start + gt + 1,
            Some(gt) => match out[start + gt..].find(&format!("</{name}>")) {
                Some(close) => start + gt + close + name.len() + 3,
                None => start + gt + 1,
            },
            None => out.len(),
        };
        out.replace_range(start..end, "");
    }
    out
}

fn write_print_options(o: &PrintOptions) -> String {
    let mut attrs = Vec::new();
    let mut put = |name: &str, on: bool| {
        if on {
            attrs.push((name.to_owned(), "1".to_owned()));
        }
    };
    put("horizontalCentered", o.horizontal_centered);
    put("verticalCentered", o.vertical_centered);
    put("headings", o.headings);
    put("gridLines", o.grid_lines);
    if !o.grid_lines_set {
        attrs.push(("gridLinesSet".to_owned(), "0".to_owned()));
    }
    attrs.extend(o.extra.clone());
    if attrs.is_empty() {
        return String::new();
    }
    format!("<printOptions{}/>", attrs_text(&attrs))
}

fn write_margins(m: &Margins) -> String {
    format!(
        "<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"{}\" footer=\"{}\"/>",
        inches(m.left),
        inches(m.right),
        inches(m.top),
        inches(m.bottom),
        inches(m.header),
        inches(m.footer)
    )
}

/// A margin as text: `1.1` rather than `1.1000000000000001`.
fn inches(value: f64) -> String {
    format!("{}", tidy(value))
}

fn write_page_setup(p: &PageSetup, rel_prefix: &str) -> String {
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut put = |name: &str, value: String| attrs.push((name.to_owned(), value));
    if p.paper_size != 1 {
        put("paperSize", p.paper_size.to_string());
    }
    if let Some(v) = &p.paper_height {
        put("paperHeight", v.clone());
    }
    if let Some(v) = &p.paper_width {
        put("paperWidth", v.clone());
    }
    if p.scale != 100 {
        put("scale", p.scale.to_string());
    }
    if p.first_page_number != 1 {
        put("firstPageNumber", p.first_page_number.to_string());
    }
    if p.fit_to_width != 1 {
        put("fitToWidth", p.fit_to_width.to_string());
    }
    if p.fit_to_height != 1 {
        put("fitToHeight", p.fit_to_height.to_string());
    }
    if p.page_order == PageOrder::OverThenDown {
        put("pageOrder", "overThenDown".to_owned());
    }
    match p.orientation {
        Some(Orientation::Portrait) => put("orientation", "portrait".to_owned()),
        Some(Orientation::Landscape) => put("orientation", "landscape".to_owned()),
        None => {}
    }
    if !p.use_printer_defaults {
        put("usePrinterDefaults", "0".to_owned());
    }
    if p.black_and_white {
        put("blackAndWhite", "1".to_owned());
    }
    if p.draft {
        put("draft", "1".to_owned());
    }
    match p.comments {
        PrintComments::None => {}
        PrintComments::AtEnd => put("cellComments", "atEnd".to_owned()),
        PrintComments::AsDisplayed => put("cellComments", "asDisplayed".to_owned()),
    }
    if p.use_first_page_number {
        put("useFirstPageNumber", "1".to_owned());
    }
    match p.errors {
        PrintErrors::Displayed => {}
        PrintErrors::Blank => put("errors", "blank".to_owned()),
        PrintErrors::Dash => put("errors", "dash".to_owned()),
        PrintErrors::NotAvailable => put("errors", "NA".to_owned()),
    }
    if p.horizontal_dpi != 600 {
        put("horizontalDpi", p.horizontal_dpi.to_string());
    }
    if p.vertical_dpi != 600 {
        put("verticalDpi", p.vertical_dpi.to_string());
    }
    if p.copies != 1 {
        put("copies", p.copies.to_string());
    }
    attrs.extend(p.extra.clone());
    if let Some(rid) = &p.printer_rid {
        attrs.push((format!("{rel_prefix}:id"), rid.clone()));
    }
    if attrs.is_empty() {
        return String::new();
    }
    format!("<pageSetup{}/>", attrs_text(&attrs))
}

fn write_header_footer(h: &HeaderFooter) -> String {
    if h.is_default() {
        return String::new();
    }
    let mut attrs = Vec::new();
    if h.different_odd_even {
        attrs.push(("differentOddEven".to_owned(), "1".to_owned()));
    }
    if h.different_first {
        attrs.push(("differentFirst".to_owned(), "1".to_owned()));
    }
    if !h.scale_with_doc {
        attrs.push(("scaleWithDoc".to_owned(), "0".to_owned()));
    }
    if !h.align_with_margins {
        attrs.push(("alignWithMargins".to_owned(), "0".to_owned()));
    }
    attrs.extend(h.extra.clone());
    let mut out = format!("<headerFooter{}>", attrs_text(&attrs));
    for (tag, text) in [
        ("oddHeader", &h.odd_header),
        ("oddFooter", &h.odd_footer),
        ("evenHeader", &h.even_header),
        ("evenFooter", &h.even_footer),
        ("firstHeader", &h.first_header),
        ("firstFooter", &h.first_footer),
    ] {
        if !text.is_empty() {
            out.push_str(&format!("<{tag}>{}</{tag}>", escape(text)));
        }
    }
    out.push_str("</headerFooter>");
    out
}

fn write_breaks(tag: &str, breaks: &[PageBreak]) -> String {
    if breaks.is_empty() {
        return String::new();
    }
    let manual = breaks.iter().filter(|b| b.manual).count();
    let mut out = format!("<{tag} count=\"{}\" manualBreakCount=\"{manual}\">", breaks.len());
    for b in breaks {
        out.push_str(&format!("<brk id=\"{}\"", b.id));
        if b.max != 0 {
            out.push_str(&format!(" max=\"{}\"", b.max));
        }
        if b.min != 0 {
            out.push_str(&format!(" min=\"{}\"", b.min));
        }
        if b.manual {
            out.push_str(" man=\"1\"");
        }
        if b.pt {
            out.push_str(" pt=\"1\"");
        }
        out.push_str("/>");
    }
    out.push_str(&format!("</{tag}>"));
    out
}
