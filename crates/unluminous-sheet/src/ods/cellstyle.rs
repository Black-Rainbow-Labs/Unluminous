//! Cell formatting in an OpenDocument file: the `style:style` elements of the table cell family, read into
//! the workbook's cell formats and written from them.
//!
//! A cell style names a parent, a number format, and three groups of properties: the cell's (fill,
//! borders, wrapping), the text's (font, size, colour) and the paragraph's (alignment). A property a style
//! does not give comes from its parent, so reading merges the chain before building a format.

use std::collections::{BTreeMap, HashMap};

use roxmltree::Node;

use super::numstyle::NS_STYLE;
use crate::styles::{Alignment, Border, Color, Dxf, Edge, Fill, Font, Styles, Underline, Xf};
use crate::xlsx::escape;

/// A style's properties, keyed `c.`, `t.` or `p.` and then the attribute's local name.
pub type Bag = BTreeMap<String, String>;

/// The cell styles of the documents, by name.
pub struct CellStyles<'a> {
    nodes: HashMap<(String, String), Node<'a, 'a>>,
    faces: HashMap<String, String>,
    pub codes: HashMap<String, String>,
    cache: HashMap<String, u32>,
}

impl<'a> CellStyles<'a> {
    /// Gather the cell styles and font faces of the documents.
    pub fn new(docs: &[&'a roxmltree::Document<'a>], codes: HashMap<String, String>) -> Self {
        let mut nodes = HashMap::new();
        let mut faces = HashMap::new();
        for doc in docs {
            for node in doc.descendants().filter(|n| n.is_element()) {
                match node.tag_name().name() {
                    "style" => {
                        let family = node.attribute((NS_STYLE, "family")).unwrap_or("");
                        if let Some(name) = node.attribute((NS_STYLE, "name")) {
                            nodes.insert((family.to_owned(), name.to_owned()), node);
                        }
                    }
                    "font-face" => {
                        let name = node.attribute((NS_STYLE, "name"));
                        let family = node
                            .attributes()
                            .find(|a| a.name() == "font-family")
                            .map(|a| a.value().trim_matches('"').trim_matches('\'').to_owned());
                        if let (Some(name), Some(family)) = (name, family) {
                            faces.insert(name.to_owned(), family);
                        }
                    }
                    _ => {}
                }
            }
        }
        Self { nodes, faces, codes, cache: HashMap::new() }
    }

    /// The properties of a style with its parents' underneath, and its data style's name.
    pub fn bag(&self, name: &str) -> Bag {
        self.bag_of("table-cell", name)
    }

    /// The properties of a style of any family: `table-cell`, `table-column`, `table-row`, `table` or `text`.
    pub fn bag_of(&self, family: &str, name: &str) -> Bag {
        self.bag_at(family, name, 0)
    }

    /// The maps a cell style carries, as the condition, the style it applies and the base cell.
    pub fn maps(&self, name: &str) -> Vec<(String, String, String)> {
        let key = ("table-cell".to_owned(), name.to_owned());
        let Some(node) = self.nodes.get(&key) else { return Vec::new() };
        node.children()
            .filter(|n| n.is_element() && n.tag_name().name() == "map")
            .filter_map(|m| {
                Some((
                    m.attribute((NS_STYLE, "condition"))?.to_owned(),
                    m.attribute((NS_STYLE, "apply-style-name"))?.to_owned(),
                    m.attribute((NS_STYLE, "base-cell-address")).unwrap_or("").to_owned(),
                ))
            })
            .collect()
    }

    fn bag_at(&self, family: &str, name: &str, depth: usize) -> Bag {
        let key = (family.to_owned(), name.to_owned());
        let Some(node) = self.nodes.get(&key) else { return Bag::new() };
        let mut bag = match node.attribute((NS_STYLE, "parent-style-name")) {
            Some(parent) if depth < 8 && parent != name => self.bag_at(family, parent, depth + 1),
            _ => Bag::new(),
        };
        if let Some(data) = node.attribute((NS_STYLE, "data-style-name")) {
            bag.insert("data".to_owned(), data.to_owned());
        }
        for child in node.children().filter(|n| n.is_element()) {
            let prefix = match child.tag_name().name() {
                "table-cell-properties" => "c.",
                "text-properties" => "t.",
                "paragraph-properties" => "p.",
                "table-column-properties" => "col.",
                "table-row-properties" => "row.",
                "table-properties" => "tab.",
                _ => continue,
            };
            for attr in child.attributes() {
                bag.insert(format!("{prefix}{}", attr.name()), attr.value().to_owned());
            }
        }
        bag
    }

    /// The cell format index for a named style, made when it is new.
    pub fn xf(&mut self, name: &str, styles: &mut Styles) -> u32 {
        if let Some(index) = self.cache.get(name) {
            return *index;
        }
        let bag = self.bag(name);
        let xf = self.build_xf(&bag, styles);
        let index = intern(&mut styles.xfs, xf);
        self.cache.insert(name.to_owned(), index);
        index
    }

    /// Set the workbook's default font from the style called `Default`, which every other style inherits.
    pub fn adopt_default_font(&self, styles: &mut Styles) {
        let bag = self.bag("Default");
        if bag.is_empty() {
            return;
        }
        let base = styles.fonts.first().cloned().unwrap_or_default();
        let font = self.font_from(&bag, &base, &styles.theme);
        if let Some(first) = styles.fonts.first_mut() {
            *first = font;
        } else {
            styles.fonts.push(font);
        }
    }

    fn build_xf(&self, bag: &Bag, styles: &mut Styles) -> Xf {
        let default_font = styles.fonts.first().cloned().unwrap_or_default();
        let font = self.font_from(bag, &default_font, &styles.theme);
        let mut xf = Xf {
            font: intern(&mut styles.fonts, font),
            fill: intern(&mut styles.fills, fill_from(bag)),
            border: intern(&mut styles.borders, border_from(bag)),
            align: alignment_from(bag),
            ..Default::default()
        };
        if let Some(code) = bag.get("data").and_then(|d| self.codes.get(d)) {
            xf.num_fmt = styles.format_id(code);
        }
        protection_from(bag, &mut xf);
        xf
    }

    /// A font from a style's text properties, starting from `base`.
    fn font_from(&self, bag: &Bag, base: &Font, theme: &[u32]) -> Font {
        let mut font = base.clone();
        let name = bag
            .get("t.font-name")
            .map(|n| self.faces.get(n).cloned().unwrap_or_else(|| n.clone()))
            .or_else(|| {
                bag.get("t.font-family").map(|f| f.trim_matches('\'').trim_matches('"').to_owned())
            });
        if let Some(name) = name {
            if name != font.name {
                font.name = name;
                font.scheme = None;
            }
        }
        if let Some(size) = bag.get("t.font-size").and_then(|s| points(s)) {
            font.size = size;
        }
        if let Some(weight) = bag.get("t.font-weight") {
            font.bold = weight == "bold" || weight.parse::<u32>().is_ok_and(|w| w >= 600);
        }
        if let Some(style) = bag.get("t.font-style") {
            font.italic = style == "italic" || style == "oblique";
        }
        if let Some(style) = bag.get("t.text-underline-style") {
            font.underline =
                match (style.as_str(), bag.get("t.text-underline-type").map(String::as_str)) {
                    ("none", _) => Underline::None,
                    (_, Some("double")) => Underline::Double,
                    _ => Underline::Single,
                };
        }
        if let Some(style) = bag.get("t.text-line-through-style") {
            font.strike = style != "none";
        }
        if let Some(position) = bag.get("t.text-position") {
            font.vert_align = if position.starts_with("super") {
                Some("superscript".to_owned())
            } else if position.starts_with("sub") {
                Some("subscript".to_owned())
            } else {
                None
            };
        }
        if let Some(color) = bag.get("t.color").and_then(|c| Color::parse_hex(c)) {
            if font.color.resolve(theme) != color.resolve(theme) {
                font.color = color;
            }
        }
        font
    }

    /// A conditional format's style as a differential format.
    pub fn dxf(&self, name: &str) -> Dxf {
        let bag = self.bag(name);
        Dxf {
            font_color: bag.get("t.color").and_then(|c| Color::parse_hex(c)),
            bold: bag.get("t.font-weight").map(|w| w == "bold"),
            italic: bag.get("t.font-style").map(|s| s == "italic"),
            strike: bag.get("t.text-line-through-style").map(|s| s != "none"),
            underline: bag.get("t.text-underline-style").map(|s| {
                if s == "none" {
                    Underline::None
                } else {
                    Underline::Single
                }
            }),
            fill: bag.get("c.background-color").and_then(|c| Color::parse_hex(c)),
            num_fmt: bag
                .get("data")
                .and_then(|d| self.codes.get(d))
                .filter(|c| *c != "General")
                .cloned(),
            ..Default::default()
        }
    }
}

/// The index of an item in a list, added when it is not there.
fn intern<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    if let Some(index) = list.iter().position(|existing| *existing == item) {
        return index as u32;
    }
    list.push(item);
    (list.len() - 1) as u32
}

/// A length in points: `11pt`, `0.5cm`, `0.2in`, `4mm`.
pub fn points(text: &str) -> Option<f64> {
    let text = text.trim();
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
    let value: f64 = text[..split].parse().ok()?;
    Some(match &text[split..] {
        "pt" => value,
        "cm" => value / 2.54 * 72.0,
        "mm" => value / 25.4 * 72.0,
        "in" => value * 72.0,
        "pc" => value * 12.0,
        "px" => value * 0.75,
        _ => return None,
    })
}

/// A fill from the cell properties.
fn fill_from(bag: &Bag) -> Fill {
    match bag.get("c.background-color").map(String::as_str) {
        None | Some("transparent") => Fill::none(),
        Some(hex) => Color::parse_hex(hex).map_or_else(Fill::none, Fill::solid),
    }
}

/// A border from the cell properties.
fn border_from(bag: &Bag) -> Border {
    let side = |name: &str| {
        bag.get(&format!("c.border-{name}"))
            .or_else(|| bag.get("c.border"))
            .map(|v| edge_from(v))
            .unwrap_or_default()
    };
    let down = bag.get("c.diagonal-tl-br").map(|v| edge_from(v));
    let up = bag.get("c.diagonal-bl-tr").map(|v| edge_from(v));
    let diagonal = down
        .clone()
        .filter(|e| !e.is_none())
        .or(up.clone().filter(|e| !e.is_none()))
        .unwrap_or_default();
    Border {
        left: side("left"),
        right: side("right"),
        top: side("top"),
        bottom: side("bottom"),
        diagonal,
        diagonal_down: down.is_some_and(|e| !e.is_none()),
        diagonal_up: up.is_some_and(|e| !e.is_none()),
    }
}

/// An edge from `thin solid #000000`.
fn edge_from(text: &str) -> Edge {
    let (mut width, mut line, mut color) = (None, "solid", Color::Auto);
    for token in text.split_whitespace() {
        if let Some(c) = token.strip_prefix('#').and_then(|_| Color::parse_hex(token)) {
            color = c;
        } else if matches!(token, "none" | "hidden") {
            return Edge::default();
        } else if matches!(
            token,
            "solid"
                | "dashed"
                | "dotted"
                | "double"
                | "dot-dash"
                | "dot-dot-dash"
                | "groove"
                | "ridge"
                | "inset"
                | "outset"
        ) {
            line = match token {
                "groove" | "ridge" | "inset" | "outset" => "solid",
                other => other,
            };
        } else {
            width = Some(match token {
                "thin" => 0.75,
                "medium" => 2.0,
                "thick" => 3.0,
                other => points(other).unwrap_or(0.75),
            });
        }
    }
    let heavy = width.unwrap_or(0.75) > 1.5;
    let thick = width.unwrap_or(0.75) > 2.5;
    let style = match (line, heavy, thick) {
        ("double", _, _) => "double",
        ("dotted", _, _) => "dotted",
        ("dashed", true, _) => "mediumDashed",
        ("dashed", _, _) => "dashed",
        ("dot-dash", true, _) => "mediumDashDot",
        ("dot-dash", _, _) => "dashDot",
        ("dot-dot-dash", true, _) => "mediumDashDotDot",
        ("dot-dot-dash", _, _) => "dashDotDot",
        (_, _, true) => "thick",
        (_, true, _) => "medium",
        _ if width.is_some_and(|w| w < 0.5) => "hair",
        _ => "thin",
    };
    let color = if matches!(color, Color::Rgb(0xFF00_0000)) { Color::Indexed(64) } else { color };
    Edge::line(style, color)
}

/// Alignment from the paragraph and cell properties.
fn alignment_from(bag: &Bag) -> Alignment {
    let horizontal = match bag.get("p.text-align").map(String::as_str) {
        Some("left") | Some("start") => Some("left"),
        Some("right") | Some("end") => Some("right"),
        Some("center") => Some("center"),
        Some("justify") => Some("justify"),
        _ => None,
    }
    .filter(|_| bag.get("c.text-align-source").is_none_or(|s| s != "value-type"));
    let vertical = match bag.get("c.vertical-align").map(String::as_str) {
        Some("top") => Some("top"),
        Some("middle") => Some("center"),
        _ => None,
    };
    let rotation = match bag.get("c.rotation-angle").and_then(|a| a.parse::<f64>().ok()) {
        Some(angle) if bag.get("c.direction").is_some_and(|d| d == "ttb") => {
            let _ = angle;
            255
        }
        Some(angle) => {
            let degrees = angle.rem_euclid(360.0).round() as i32;
            if degrees <= 90 {
                degrees
            } else {
                90 + (360 - degrees)
            }
        }
        None if bag.get("c.direction").is_some_and(|d| d == "ttb") => 255,
        None => 0,
    };
    let indent = bag
        .get("p.margin-left")
        .and_then(|m| points(m))
        .map_or(0, |pt| (pt / INDENT_POINTS).round().max(0.0) as u32);
    Alignment {
        horizontal: horizontal.map(str::to_owned),
        vertical: vertical.map(str::to_owned),
        wrap: bag.get("c.wrap-option").is_some_and(|w| w == "wrap"),
        indent,
        rotation,
        shrink: bag.get("c.shrink-to-fit").is_some_and(|s| s == "true"),
    }
}

/// How many points one level of indent is, which is how Excel converts it for a file.
const INDENT_POINTS: f64 = 9.0;

/// Locked and hidden from `style:cell-protect`.
fn protection_from(bag: &Bag, xf: &mut Xf) {
    let Some(protect) = bag.get("c.cell-protect") else { return };
    if protect == "none" {
        xf.locked = Some(false);
    }
    if protect.contains("formula-hidden") || protect.contains("hidden-and-protected") {
        xf.hidden = Some(true);
    }
}

// ---------------------------------------------------------------------------------------------------
// Writing.

/// The properties of an automatic cell style for a cell format.
pub fn write_cell_style(xf: &Xf, styles: &Styles, name: &str, data_style: &str) -> String {
    let font = font_at(styles, xf.font);
    let fill = styles.fills.get(xf.fill as usize).cloned().unwrap_or_else(Fill::none);
    let border = styles.borders.get(xf.border as usize).cloned().unwrap_or_default();
    let mut xml = format!(
        "<style:style style:name=\"{name}\" style:family=\"table-cell\" style:parent-style-name=\"Default\" style:data-style-name=\"{data_style}\">"
    );
    xml.push_str(&format!(
        "<style:table-cell-properties{}/>",
        cell_attrs(xf, &fill, &border, &styles.theme)
    ));
    xml.push_str(&format!("<style:text-properties{}/>", text_attrs(&font, &styles.theme)));
    let paragraph = paragraph_attrs(&xf.align);
    if !paragraph.is_empty() {
        xml.push_str(&format!("<style:paragraph-properties{paragraph}/>"));
    }
    xml.push_str("</style:style>");
    xml
}

/// The cell properties: fill, borders, alignment, wrapping and protection.
fn cell_attrs(xf: &Xf, fill: &Fill, border: &Border, theme: &[u32]) -> String {
    let mut out = String::new();
    match fill.paint().and_then(|c| c.resolve(theme)) {
        Some((r, g, b)) => {
            out.push_str(&format!(" fo:background-color=\"#{r:02X}{g:02X}{b:02X}\""))
        }
        None => out.push_str(" fo:background-color=\"transparent\""),
    }
    for (name, edge) in [
        ("top", &border.top),
        ("bottom", &border.bottom),
        ("left", &border.left),
        ("right", &border.right),
    ] {
        out.push_str(&format!(" fo:border-{name}=\"{}\"", edge_text(edge, theme)));
    }
    if !border.diagonal.is_none() {
        let text = edge_text(&border.diagonal, theme);
        if border.diagonal_down {
            out.push_str(&format!(" style:diagonal-tl-br=\"{text}\""));
        }
        if border.diagonal_up {
            out.push_str(&format!(" style:diagonal-bl-tr=\"{text}\""));
        }
    }
    let align = &xf.align;
    match align.vertical.as_deref() {
        Some("top") => out.push_str(" style:vertical-align=\"top\""),
        Some("center") => out.push_str(" style:vertical-align=\"middle\""),
        _ => out.push_str(" style:vertical-align=\"bottom\""),
    }
    if align.horizontal.is_some() {
        out.push_str(" style:text-align-source=\"fix\"");
    }
    if align.wrap {
        out.push_str(" fo:wrap-option=\"wrap\"");
    }
    if align.shrink {
        out.push_str(" style:shrink-to-fit=\"true\"");
    }
    if align.rotation == 255 {
        out.push_str(" style:direction=\"ttb\"");
    } else if align.rotation != 0 {
        let angle = if align.rotation <= 90 { align.rotation } else { 360 - (align.rotation - 90) };
        out.push_str(&format!(" style:rotation-angle=\"{angle}\""));
    }
    let locked = xf.locked.unwrap_or(true);
    let hidden = xf.hidden.unwrap_or(false);
    let protect = match (locked, hidden) {
        (true, false) => "protected",
        (true, true) => "protected formula-hidden",
        (false, true) => "formula-hidden",
        (false, false) => "none",
    };
    out.push_str(&format!(" style:cell-protect=\"{protect}\""));
    out
}

/// An edge as `fo:border-*` writes it, the way Excel writes them.
fn edge_text(edge: &Edge, theme: &[u32]) -> String {
    if edge.is_none() {
        return "none".to_owned();
    }
    let (width, line) = match edge.style.as_str() {
        "medium" => ("2pt", "solid"),
        "thick" => ("thick", "solid"),
        "double" => ("thin", "double"),
        "dotted" => ("thin", "dotted"),
        "dashed" => ("thin", "dashed"),
        "mediumDashed" => ("2pt", "dashed"),
        "dashDot" => ("thin", "dot-dash"),
        "mediumDashDot" => ("2pt", "dot-dash"),
        "dashDotDot" | "slantDashDot" => ("thin", "dot-dot-dash"),
        "mediumDashDotDot" => ("2pt", "dot-dot-dash"),
        "hair" => ("0.5pt", "solid"),
        _ => ("thin", "solid"),
    };
    let color = edge
        .color
        .resolve(theme)
        .map_or("#000000".to_owned(), |(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
    format!("{width} {line} {color}")
}

/// The text properties of a font.
fn text_attrs(font: &Font, theme: &[u32]) -> String {
    let mut out = String::new();
    match font.color.resolve(theme) {
        Some((r, g, b)) => out.push_str(&format!(" fo:color=\"#{r:02X}{g:02X}{b:02X}\"")),
        None => out.push_str(" style:use-window-font-color=\"true\""),
    }
    let name = escape(&font.name);
    out.push_str(&format!(" style:font-name=\"{name}\" fo:font-size=\"{}pt\"", number(font.size)));
    if font.bold {
        out.push_str(" fo:font-weight=\"bold\"");
    }
    if font.italic {
        out.push_str(" fo:font-style=\"italic\"");
    }
    match font.underline {
        Underline::None => {}
        Underline::Double | Underline::DoubleAccounting => {
            out.push_str(
                " style:text-underline-style=\"solid\" style:text-underline-type=\"double\"",
            );
        }
        _ => out
            .push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"single\""),
    }
    if font.strike {
        out.push_str(" style:text-line-through-style=\"solid\"");
    }
    match font.vert_align.as_deref() {
        Some("superscript") => out.push_str(" style:text-position=\"super 58%\""),
        Some("subscript") => out.push_str(" style:text-position=\"sub 58%\""),
        _ => {}
    }
    out
}

/// The paragraph properties: horizontal alignment and indent.
fn paragraph_attrs(align: &Alignment) -> String {
    let mut out = String::new();
    match align.horizontal.as_deref() {
        Some("left") => out.push_str(" fo:text-align=\"start\""),
        Some("right") => out.push_str(" fo:text-align=\"end\""),
        Some("center") | Some("centerContinuous") => out.push_str(" fo:text-align=\"center\""),
        Some("justify") | Some("distributed") => out.push_str(" fo:text-align=\"justify\""),
        _ => {}
    }
    if align.indent > 0 {
        out.push_str(&format!(
            " fo:margin-left=\"{}pt\"",
            number(align.indent as f64 * INDENT_POINTS)
        ));
    }
    out
}

/// A number as short as it reads back.
pub fn number(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A font face declaration.
pub fn font_face(name: &str) -> String {
    let name = escape(name);
    format!("<style:font-face style:name=\"{name}\" svg:font-family=\"&quot;{name}&quot;\"/>")
}

/// The font at an index, or the first font.
fn font_at(styles: &Styles, index: u32) -> Font {
    styles.fonts.get(index as usize).or_else(|| styles.fonts.first()).cloned().unwrap_or_default()
}
