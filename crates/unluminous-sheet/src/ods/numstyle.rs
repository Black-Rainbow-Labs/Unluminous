//! Number formats as OpenDocument data styles, in both directions.
//!
//! An Excel format code (`#,##0.00;[Red]-#,##0.00`) is a list of sections. An OpenDocument number format
//! is a style element (`number:number-style`, `number:date-style`, `number:percentage-style` and the
//! rest) holding the pieces of one section in order, and a style with `style:map` children for the
//! other sections, each of which names a style of its own and the condition it applies under. Reading
//! joins the sections back into one code and writing splits a code into styles.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::xlsx::escape;

pub const NS_NUMBER: &str = "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0";
pub const NS_STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
pub const NS_FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";

/// The colours a format code can name, with the red, green and blue Excel gives them.
const NAMED_COLORS: [(&str, u32); 8] = [
    ("Black", 0x000000),
    ("Blue", 0x0000FF),
    ("Cyan", 0x00FFFF),
    ("Green", 0x00FF00),
    ("Magenta", 0xFF00FF),
    ("Red", 0xFF0000),
    ("White", 0xFFFFFF),
    ("Yellow", 0xFFFF00),
];

/// Locales a currency symbol is written with in a format code, by language and country.
const LOCALES: [(&str, &str, &str); 8] = [
    ("en", "US", "409"),
    ("en", "GB", "809"),
    ("de", "DE", "407"),
    ("fr", "FR", "40C"),
    ("ja", "JP", "411"),
    ("es", "ES", "C0A"),
    ("it", "IT", "410"),
    ("zh", "CN", "804"),
];

// ---------------------------------------------------------------------------------------------------
// Reading.

/// Every data style in the documents, by name, as an Excel format code.
pub fn read_codes(docs: &[&Document]) -> HashMap<String, String> {
    let mut nodes: HashMap<String, Node> = HashMap::new();
    for doc in docs {
        for node in doc.descendants().filter(|n| is_data_style(*n)) {
            if let Some(name) = node.attribute((NS_STYLE, "name")) {
                nodes.insert(name.to_owned(), node);
            }
        }
    }
    nodes.keys().map(|name| (name.clone(), style_code(name, &nodes, 0))).collect()
}

/// Whether a node is one of the data style elements.
fn is_data_style(node: Node) -> bool {
    node.is_element()
        && node.tag_name().namespace() == Some(NS_NUMBER)
        && node.tag_name().name().ends_with("-style")
}

/// The code of one named style, joining the sections its `style:map` children name.
fn style_code(name: &str, nodes: &HashMap<String, Node>, depth: usize) -> String {
    let Some(node) = nodes.get(name) else { return "General".to_owned() };
    let own = section_code(*node);
    let maps: Vec<(String, String)> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "map")
        .filter_map(|m| {
            let condition = m.attribute((NS_STYLE, "condition"))?.to_owned();
            let target = m.attribute((NS_STYLE, "apply-style-name"))?.to_owned();
            Some((condition, target))
        })
        .collect();
    if maps.is_empty() || depth > 2 {
        return own;
    }
    let sections: Vec<(Option<(String, String)>, String)> = maps
        .iter()
        .map(|(condition, target)| {
            (parse_condition(condition), style_code(target, nodes, depth + 1))
        })
        .collect();
    join_sections(&sections, &own, node.tag_name().name() == "text-style")
}

/// A map's condition, `value()>=5`, as an operator and a number.
fn parse_condition(text: &str) -> Option<(String, String)> {
    let rest = text.trim().strip_prefix("value()")?;
    let operator: String = rest.chars().take_while(|c| "<>=".contains(*c)).collect();
    let number = rest[operator.len()..].trim().to_owned();
    (!operator.is_empty() && !number.is_empty()).then_some((operator, number))
}

/// Join the mapped sections and the style's own content into one code.
fn join_sections(
    maps: &[(Option<(String, String)>, String)],
    base: &str,
    base_is_text: bool,
) -> String {
    let ops: Vec<&str> = maps
        .iter()
        .map(|(c, _)| c.as_ref().map_or("", |(op, num)| if num == "0" { op.as_str() } else { "" }))
        .collect();
    let codes: Vec<&str> = maps.iter().map(|(_, code)| code.as_str()).collect();
    match (ops.as_slice(), base_is_text) {
        ([">", "<", "="], _) => format!("{};{};{};{base}", codes[0], codes[1], codes[2]),
        ([">", "<"], false) => format!("{};{};{base}", codes[0], codes[1]),
        ([">", "<"], true) => format!("{};{};{};{base}", codes[0], codes[1], codes[0]),
        ([">="], _) => format!("{};{base}", codes[0]),
        _ => {
            let mut parts: Vec<String> = Vec::new();
            for (condition, code) in maps.iter().take(2) {
                let prefix = condition
                    .as_ref()
                    .map(|(op, num)| format!("[{op}{num}]"))
                    .unwrap_or_default();
                parts.push(format!("{prefix}{code}"));
            }
            parts.push(base.to_owned());
            parts.join(";")
        }
    }
}

/// The code of one style's own content: its colour, then its pieces in order.
fn section_code(node: Node) -> String {
    let percent = node.tag_name().name() == "percentage-style";
    let elapsed = node.attribute((NS_NUMBER, "truncate-on-overflow")) == Some("false");
    let mut out = String::new();
    if let Some(color) = style_color(node) {
        out.push_str(&format!("[{color}]"));
    }
    let mut text = String::new();
    let mut first_time = true;
    for child in node.children().filter(|n| n.is_element()) {
        if child.tag_name().namespace() != Some(NS_NUMBER) {
            continue;
        }
        if child.tag_name().name() == "text" {
            text.push_str(child.text().unwrap_or(""));
            continue;
        }
        flush_text(&mut text, &mut out, percent);
        match child.tag_name().name() {
            "number" => number_code(child, &mut out),
            "scientific-number" => scientific_code(child, &mut out),
            "fraction" => fraction_code(child, &mut out),
            "currency-symbol" => out.push_str(&currency_code(child)),
            "fill-character" => {
                out.push('*');
                out.push_str(child.text().unwrap_or(" "));
            }
            "text-content" => out.push('@'),
            "boolean" => out.push_str("General"),
            name => {
                let bracket = elapsed && first_time && matches!(name, "hours" | "minutes" | "seconds");
                if bracket {
                    first_time = false;
                }
                out.push_str(&date_code(child, bracket));
            }
        }
    }
    flush_text(&mut text, &mut out, percent);
    out
}

/// Write the literal text gathered so far.
fn flush_text(text: &mut String, out: &mut String, percent: bool) {
    if text.is_empty() {
        return;
    }
    if percent && text == "%" {
        out.push('%');
    } else {
        out.push_str(&literal(text));
    }
    text.clear();
}

/// Literal text as a format code writes it: bare when every character is one that needs no quotes.
fn literal(text: &str) -> String {
    let mut out = String::new();
    let mut quoted = String::new();
    for c in text.chars() {
        if c == ',' || c == '.' {
            if !quoted.is_empty() {
                out.push_str(&quote(&std::mem::take(&mut quoted)));
            }
            out.push('\\');
            out.push(c);
        } else if " $-+/():!^&'~{}<>=".contains(c) {
            if !quoted.is_empty() {
                out.push_str(&quote(&std::mem::take(&mut quoted)));
            }
            if c == ' ' || c == '-' {
                out.push('\\');
                out.push(c);
            } else {
                out.push(c);
            }
        } else {
            quoted.push(c);
        }
    }
    if !quoted.is_empty() {
        out.push_str(&quote(&quoted));
    }
    out
}

/// Text in quotes, with a quote inside it written as an escaped character.
fn quote(text: &str) -> String {
    if text.contains('"') {
        text.chars().map(|c| format!("\\{c}")).collect()
    } else {
        format!("\"{text}\"")
    }
}

/// The colour a style's text properties name, as a format code colour.
fn style_color(node: Node) -> Option<String> {
    let props = node.children().find(|n| n.is_element() && n.tag_name().name() == "text-properties")?;
    let hex = props.attribute((NS_FO, "color"))?;
    let rgb = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    if let Some((name, _)) = NAMED_COLORS.iter().find(|(_, c)| *c == rgb) {
        return Some((*name).to_owned());
    }
    let argb = 0xFF00_0000 | rgb;
    let index = crate::styles::INDEXED[8..].iter().position(|c| *c == argb)?;
    Some(format!("Color{}", index + 1))
}

/// A number element as digit placeholders.
fn number_code(node: Node, out: &mut String) {
    let attr = |name: &str| node.attribute((NS_NUMBER, name));
    let decimals = attr("decimal-places").and_then(|v| v.parse::<usize>().ok());
    let grouping = attr("grouping") == Some("true");
    let min_integer = attr("min-integer-digits").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
    let embedded: Vec<(usize, String)> = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "embedded-text")
        .filter_map(|n| {
            let at = n.attribute((NS_NUMBER, "position"))?.parse().ok()?;
            Some((at, n.text().unwrap_or("").to_owned()))
        })
        .collect();
    if decimals.is_none() && !grouping && embedded.is_empty() {
        out.push_str("General");
        return;
    }
    let mut width = min_integer.max(embedded.iter().map(|(p, _)| *p).max().unwrap_or(0)).max(1);
    if grouping {
        width = width.max(4);
    }
    let mut digits: Vec<String> =
        (0..width).map(|i| if i >= width - min_integer { "0" } else { "#" }.to_owned()).collect();
    if min_integer == 0 && embedded.is_empty() && !grouping {
        digits = vec!["#".to_owned()];
    }
    for (position, text) in &embedded {
        let last = digits.len().saturating_sub(1);
        let at = digits.len().saturating_sub(*position).min(last);
        digits[at].insert_str(0, &literal(text));
    }
    if grouping {
        let at = digits.len().saturating_sub(3);
        digits[at].insert(0, ',');
    }
    out.push_str(&digits.concat());
    let decimals = decimals.unwrap_or(0);
    if decimals > 0 {
        let minimum = attr("min-decimal-places").and_then(|v| v.parse::<usize>().ok()).unwrap_or(decimals);
        out.push('.');
        out.push_str(&"0".repeat(minimum.min(decimals)));
        out.push_str(&"#".repeat(decimals - minimum.min(decimals)));
    }
    let factor: f64 = attr("display-factor").and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let commas = if factor > 1.0 { (factor.ln() / 1000f64.ln()).round() as usize } else { 0 };
    out.push_str(&",".repeat(commas));
}

/// A scientific number: `0.00E+00`.
fn scientific_code(node: Node, out: &mut String) {
    let attr = |name: &str| node.attribute((NS_NUMBER, name)).and_then(|v| v.parse::<usize>().ok());
    let integer = attr("min-integer-digits").unwrap_or(1).max(1);
    let decimals = attr("decimal-places").unwrap_or(0);
    let exponent = attr("min-exponent-digits").unwrap_or(2).max(1);
    out.push_str(&"#".repeat(integer - 1));
    out.push('0');
    if decimals > 0 {
        out.push('.');
        out.push_str(&"0".repeat(decimals));
    }
    out.push_str("E+");
    out.push_str(&"0".repeat(exponent));
}

/// A fraction: `# ??/??`.
fn fraction_code(node: Node, out: &mut String) {
    let attr = |name: &str| node.attribute((NS_NUMBER, name));
    if attr("min-integer-digits").is_some() {
        out.push_str("# ");
    }
    let numerator = attr("min-numerator-digits").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
    out.push_str(&"?".repeat(numerator.max(1)));
    out.push('/');
    match attr("denominator-value") {
        Some(fixed) => out.push_str(fixed),
        None => {
            let denominator =
                attr("min-denominator-digits").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            out.push_str(&"?".repeat(denominator.max(1)));
        }
    }
}

/// A currency symbol, with its locale when the element names one.
fn currency_code(node: Node) -> String {
    let symbol = node.text().unwrap_or("$");
    let language = node.attribute((NS_NUMBER, "language"));
    let country = node.attribute((NS_NUMBER, "country"));
    if let (Some(language), Some(country)) = (language, country) {
        if let Some((_, _, lcid)) = LOCALES.iter().find(|(l, c, _)| *l == language && *c == country) {
            return format!("[${symbol}-{lcid}]");
        }
    }
    if symbol == "$" {
        "$".to_owned()
    } else {
        quote(symbol)
    }
}

/// A date or time element as its letters.
fn date_code(node: Node, bracket: bool) -> String {
    let attr = |name: &str| node.attribute((NS_NUMBER, name));
    let long = attr("style") == Some("long");
    let textual = attr("textual") == Some("true");
    let pick = |short: &str, long_form: &str| if long { long_form } else { short }.to_owned();
    match node.tag_name().name() {
        "year" => pick("yy", "yyyy"),
        "month" if textual => pick("mmm", "mmmm"),
        "month" => pick("m", "mm"),
        "day" => pick("d", "dd"),
        "day-of-week" => pick("ddd", "dddd"),
        "hours" if bracket => "[h]".to_owned(),
        "hours" => pick("h", "hh"),
        "minutes" if bracket => "[mm]".to_owned(),
        "minutes" => pick("m", "mm"),
        "seconds" => {
            let mut text = if bracket { "[ss]".to_owned() } else { pick("s", "ss") };
            if let Some(places) = attr("decimal-places").and_then(|v| v.parse::<usize>().ok()) {
                if places > 0 {
                    text.push('.');
                    text.push_str(&"0".repeat(places));
                }
            }
            text
        }
        "am-pm" => "AM/PM".to_owned(),
        "era" => "gg".to_owned(),
        "week-of-year" | "quarter" => String::new(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------------------------------
// Writing.

/// One section of a format code.
#[derive(Debug, Clone, Default, PartialEq)]
struct Section {
    color: Option<u32>,
    condition: Option<(String, String)>,
    body: String,
}

/// The XML of the data styles for a code, the first of them named `name`, followed by the styles for
/// the sections it maps to.
pub fn write_styles(code: &str, name: &str) -> String {
    let sections = split_sections(code);
    if sections.len() <= 1 {
        let section = sections.into_iter().next().unwrap_or_default();
        return style_xml(&section, name, &[]);
    }
    let (mapped, base, base_is_text) = plan(&sections);
    let mut xml = String::new();
    let mut maps: Vec<(String, String)> = Vec::new();
    for (i, (condition, section)) in mapped.iter().enumerate() {
        let sub = format!("{name}P{i}");
        xml.push_str(&style_xml(section, &sub, &[]));
        maps.push((condition.clone(), sub));
    }
    if base_is_text {
        xml.push_str(&text_style_xml(&base, name, &maps));
    } else {
        xml.push_str(&style_xml(&base, name, &maps));
    }
    xml
}

/// Decide which sections become mapped styles with which conditions, and which one is the style's own
/// content, which also shows what no condition matched.
fn plan(sections: &[Section]) -> (Vec<(String, Section)>, Section, bool) {
    let has_text = sections.last().is_some_and(|s| is_text_section(&s.body));
    let numbers = if has_text { &sections[..sections.len() - 1] } else { sections };
    let general = Section { body: "General".to_owned(), ..Default::default() };
    if numbers.iter().any(|s| s.condition.is_some()) {
        let mapped = numbers
            .iter()
            .filter_map(|s| s.condition.as_ref().map(|(op, num)| (format!("value(){op}{num}"), s.clone())))
            .take(2)
            .collect();
        let rest = numbers.iter().find(|s| s.condition.is_none());
        let base = if has_text { sections.last() } else { rest }.cloned().unwrap_or(general);
        return (mapped, base, has_text);
    }
    let conditions: Vec<&str> = match (numbers.len(), has_text) {
        (1, true) => vec!["value()>0", "value()<0", "value()=0"],
        (2, true) => vec!["value()>0", "value()<0", "value()=0"],
        (2, false) => vec!["value()>=0"],
        (3, true) => vec!["value()>0", "value()<0", "value()=0"],
        _ => vec!["value()>0", "value()<0"],
    };
    let pick = |i: usize| numbers[i.min(numbers.len() - 1)].clone();
    let mapped: Vec<(String, Section)> = conditions
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let source = match (numbers.len(), i) {
                (1, _) | (2, 2) => 0,
                (_, i) => i,
            };
            ((*c).to_owned(), pick(source))
        })
        .collect();
    if has_text {
        return (mapped, sections[sections.len() - 1].clone(), true);
    }
    (mapped, numbers[numbers.len() - 1].clone(), false)
}


/// Whether a section is the text section: it holds `@` and no digit placeholders.
fn is_text_section(body: &str) -> bool {
    body.contains('@') && !body.chars().any(|c| "0#?".contains(c))
}

/// Split a format code into sections at the semicolons outside quotes and brackets.
fn split_sections(code: &str) -> Vec<Section> {
    let mut parts: Vec<String> = vec![String::new()];
    let (mut quoted, mut bracketed, mut escaped) = (false, false, false);
    for c in code.chars() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if !quoted && c == '[' {
            bracketed = true;
        } else if !quoted && c == ']' {
            bracketed = false;
        } else if !quoted && !bracketed && c == ';' {
            parts.push(String::new());
            continue;
        }
        if let Some(last) = parts.last_mut() {
            last.push(c);
        }
    }
    parts.into_iter().map(|p| read_brackets(&p)).collect()
}

/// Take the colour and the condition off the front of a section.
fn read_brackets(part: &str) -> Section {
    let mut section = Section::default();
    let mut rest = part;
    while let Some(after) = rest.strip_prefix('[') {
        let Some(end) = after.find(']') else { break };
        let inner = &after[..end];
        if let Some(color) = color_of(inner) {
            section.color = Some(color);
        } else if let Some(condition) = condition_of(inner) {
            section.condition = Some(condition);
        } else {
            break;
        }
        rest = &after[end + 1..];
    }
    section.body = rest.to_owned();
    section
}

/// The red, green and blue a colour name stands for.
fn color_of(name: &str) -> Option<u32> {
    if let Some((_, rgb)) = NAMED_COLORS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        return Some(*rgb);
    }
    let index: usize = name.strip_prefix("Color").or_else(|| name.strip_prefix("color"))?.parse().ok()?;
    crate::styles::INDEXED.get(index + 7).map(|c| c & 0x00FF_FFFF)
}

/// A condition such as `>=5`, as an operator and a number.
fn condition_of(text: &str) -> Option<(String, String)> {
    let operator: String = text.chars().take_while(|c| "<>=".contains(*c)).collect();
    let number = text[operator.len()..].trim();
    (!operator.is_empty() && number.parse::<f64>().is_ok()).then(|| (operator, number.to_owned()))
}

/// A data style element for a section, with maps to the styles it chooses between.
fn style_xml(section: &Section, name: &str, maps: &[(String, String)]) -> String {
    let pieces = Pieces::read(&section.body);
    let tag = pieces.kind();
    let mut attrs = format!(" style:name=\"{name}\"");
    if pieces.elapsed {
        attrs.push_str(" number:truncate-on-overflow=\"false\"");
    }
    let mut inner = color_xml(section.color);
    inner.push_str(&pieces.xml());
    inner.push_str(&maps_xml(maps));
    format!("<number:{tag}{attrs}>{inner}</number:{tag}>")
}

/// A text style element, for the text section of a code with several sections.
fn text_style_xml(section: &Section, name: &str, maps: &[(String, String)]) -> String {
    let pieces = Pieces::read(&section.body);
    let mut inner = color_xml(section.color);
    inner.push_str(&pieces.xml());
    inner.push_str(&maps_xml(maps));
    format!("<number:text-style style:name=\"{name}\">{inner}</number:text-style>")
}

/// The `style:map` children.
fn maps_xml(maps: &[(String, String)]) -> String {
    maps.iter()
        .map(|(condition, target)| {
            format!(
                "<style:map style:condition=\"{}\" style:apply-style-name=\"{target}\"/>",
                escape(condition)
            )
        })
        .collect()
}

/// A section's colour as the style's text properties.
fn color_xml(color: Option<u32>) -> String {
    color.map(|c| format!("<style:text-properties fo:color=\"#{c:06X}\"/>")).unwrap_or_default()
}

/// One piece of a section.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Fill(char),
    TextContent,
    General,
    Number { integer: Vec<char>, embedded: Vec<(usize, String)>, decimals: Vec<char>, commas: usize },
    Scientific { integer: usize, decimals: usize, exponent: usize },
    Fraction { integer: bool, numerator: usize, denominator: Result<usize, String> },
    Percent,
    Year(bool),
    Month { long: bool, textual: bool },
    Day(bool),
    DayOfWeek(bool),
    Hour(bool, bool),
    Minute(bool, bool),
    Second { long: bool, elapsed: bool, places: usize },
    AmPm,
}

/// A section's pieces.
struct Pieces {
    list: Vec<Piece>,
    elapsed: bool,
}

impl Pieces {
    /// Scan a section's body.
    fn read(body: &str) -> Pieces {
        let chars: Vec<char> = body.chars().collect();
        let mut list: Vec<Piece> = Vec::new();
        let mut elapsed = false;
        let mut i = 0;
        while i < chars.len() {
            i = scan_piece(&chars, i, &mut list, &mut elapsed);
        }
        resolve_minutes(&mut list);
        Pieces { list, elapsed }
    }

    /// The element name that suits the pieces.
    fn kind(&self) -> &'static str {
        let has = |f: fn(&Piece) -> bool| self.list.iter().any(f);
        if has(|p| matches!(p, Piece::Year(_) | Piece::Month { .. } | Piece::Day(_) | Piece::DayOfWeek(_))) {
            "date-style"
        } else if has(|p| matches!(p, Piece::Hour(..) | Piece::Minute(..) | Piece::Second { .. } | Piece::AmPm)) {
            "time-style"
        } else if has(|p| matches!(p, Piece::Percent)) {
            "percentage-style"
        } else if has(|p| matches!(p, Piece::TextContent)) && !has(|p| matches!(p, Piece::Number { .. })) {
            "text-style"
        } else {
            "number-style"
        }
    }

    /// The pieces as elements.
    fn xml(&self) -> String {
        self.list.iter().map(piece_xml).collect()
    }
}

/// Scan one piece at `i`, push it, and answer the index after it.
fn scan_piece(chars: &[char], i: usize, list: &mut Vec<Piece>, elapsed: &mut bool) -> usize {
    let c = chars[i];
    match c {
        '"' => {
            let end = chars[i + 1..].iter().position(|x| *x == '"').map_or(chars.len(), |p| i + 1 + p);
            push_text(list, &chars[i + 1..end].iter().collect::<String>());
            end + 1
        }
        '\\' => {
            if let Some(next) = chars.get(i + 1) {
                push_text(list, &next.to_string());
            }
            i + 2
        }
        '_' => {
            push_text(list, " ");
            i + 2
        }
        '*' => {
            if let Some(next) = chars.get(i + 1) {
                list.push(Piece::Fill(*next));
            }
            i + 2
        }
        '@' => {
            list.push(Piece::TextContent);
            i + 1
        }
        '%' => {
            list.push(Piece::Percent);
            i + 1
        }
        '[' => scan_bracket(chars, i, list, elapsed),
        '0' | '#' | '?' => scan_number(chars, i, list),
        '.' if chars.get(i + 1).is_some_and(|n| "0#?".contains(*n)) => scan_number(chars, i, list),
        c if c.is_ascii_alphabetic() => scan_letters(chars, i, list),
        c => {
            push_text(list, &c.to_string());
            i + 1
        }
    }
}

/// Append literal text to the pieces, joining it to text just before.
fn push_text(list: &mut Vec<Piece>, text: &str) {
    if let Some(Piece::Text(last)) = list.last_mut() {
        last.push_str(text);
    } else {
        list.push(Piece::Text(text.to_owned()));
    }
}

/// A bracket in the body: elapsed time, or a currency symbol with its locale.
fn scan_bracket(chars: &[char], i: usize, list: &mut Vec<Piece>, elapsed: &mut bool) -> usize {
    let end = chars[i..].iter().position(|c| *c == ']').map_or(chars.len(), |p| i + p);
    let inner: String = chars[i + 1..end.min(chars.len())].iter().collect();
    let lower = inner.to_ascii_lowercase();
    if let Some(symbol) = inner.strip_prefix('$') {
        let symbol = symbol.split('-').next().unwrap_or("");
        if !symbol.is_empty() {
            push_text(list, symbol);
        }
    } else if lower.starts_with('h') && lower.chars().all(|c| c == 'h') {
        *elapsed = true;
        list.push(Piece::Hour(lower.len() > 1, true));
    } else if lower.starts_with('m') && lower.chars().all(|c| c == 'm') {
        *elapsed = true;
        list.push(Piece::Minute(lower.len() > 1, true));
    } else if lower.starts_with('s') && lower.chars().all(|c| c == 's') {
        *elapsed = true;
        list.push(Piece::Second { long: lower.len() > 1, elapsed: true, places: 0 });
    }
    end + 1
}

/// Letters that mean a date or time part, or are plain text.
fn scan_letters(chars: &[char], i: usize, list: &mut Vec<Piece>) -> usize {
    let c = chars[i].to_ascii_lowercase();
    let run = chars[i..].iter().take_while(|x| x.to_ascii_lowercase() == c).count();
    let rest: String = chars[i..].iter().collect::<String>().to_ascii_uppercase();
    if rest.starts_with("GENERAL") {
        list.push(Piece::General);
        return i + 7;
    }
    if rest.starts_with("AM/PM") {
        list.push(Piece::AmPm);
        return i + 5;
    }
    if rest.starts_with("A/P") {
        list.push(Piece::AmPm);
        return i + 3;
    }
    let piece = match c {
        'y' => Piece::Year(run >= 3),
        'm' => Piece::Month { long: run == 2 || run >= 4, textual: run >= 3 },
        'd' if run >= 3 => Piece::DayOfWeek(run >= 4),
        'd' => Piece::Day(run >= 2),
        'h' => Piece::Hour(run >= 2, false),
        's' => {
            let after = i + run;
            let places = if chars.get(after) == Some(&'.') {
                chars[after + 1..].iter().take_while(|x| **x == '0').count()
            } else {
                0
            };
            list.push(Piece::Second { long: run >= 2, elapsed: false, places });
            return after + if places > 0 { places + 1 } else { 0 };
        }
        _ => {
            push_text(list, &chars[i].to_string());
            return i + 1;
        }
    };
    list.push(piece);
    i + run
}

/// A run of digit placeholders, a fraction or a scientific number.
fn scan_number(chars: &[char], i: usize, list: &mut Vec<Piece>) -> usize {
    if let Some((piece, next)) = scan_fraction(chars, i) {
        list.push(piece);
        return next;
    }
    let mut integer: Vec<char> = Vec::new();
    let mut embedded: Vec<(usize, String)> = Vec::new();
    let mut at = i;
    while at < chars.len() {
        let c = chars[at];
        if "0#?".contains(c) {
            integer.push(c);
            at += 1;
        } else if c == ',' && chars.get(at + 1).is_some_and(|n| "0#?".contains(*n)) {
            at += 1;
        } else if "-() ".contains(c) && !integer.is_empty() {
            let literal_end = (at..chars.len()).take_while(|k| "-() ".contains(chars[*k])).last().map_or(at, |k| k + 1);
            if chars.get(literal_end).is_some_and(|n| "0#?".contains(*n)) {
                embedded.push((integer.len(), chars[at..literal_end].iter().collect()));
                at = literal_end;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let mut decimals: Vec<char> = Vec::new();
    if chars.get(at) == Some(&'.') && chars.get(at + 1).is_some_and(|n| "0#?".contains(*n)) {
        at += 1;
        while at < chars.len() && "0#?".contains(chars[at]) {
            decimals.push(chars[at]);
            at += 1;
        }
    } else if chars.get(at) == Some(&'.') && integer.is_empty() {
        return at + 1;
    }
    if matches!(chars.get(at), Some('E') | Some('e')) && matches!(chars.get(at + 1), Some('+') | Some('-')) {
        let exponent = chars[at + 2..].iter().take_while(|c| **c == '0').count();
        if exponent > 0 {
            list.push(Piece::Scientific { integer: integer.len(), decimals: decimals.len(), exponent });
            return at + 2 + exponent;
        }
    }
    let mut commas = 0;
    while chars.get(at) == Some(&',') {
        commas += 1;
        at += 1;
    }
    let grouped = chars[i..at.min(chars.len())].windows(2).any(|w| w[0] == ',' && "0#?".contains(w[1]));
    if integer.is_empty() && decimals.is_empty() {
        return at.max(i + 1);
    }
    let positions = embedded.iter().map(|(p, t)| (integer.len() - p, t.clone())).collect();
    let mut digits = integer;
    if grouped {
        digits.insert(0, ',');
    }
    list.push(Piece::Number { integer: digits, embedded: positions, decimals, commas });
    at
}

/// A fraction such as `# ??/??` or `?/16`, answering the piece and the index after it.
fn scan_fraction(chars: &[char], i: usize) -> Option<(Piece, usize)> {
    let digits = |from: usize| chars[from..].iter().take_while(|c| "0#?".contains(**c)).count();
    let first = digits(i);
    if first == 0 {
        return None;
    }
    let mut at = i + first;
    let mut integer = false;
    let mut numerator = first;
    if chars.get(at) == Some(&' ') && digits(at + 1) > 0 {
        integer = true;
        at += 1;
        numerator = digits(at);
        at += numerator;
    }
    if chars.get(at) != Some(&'/') {
        return None;
    }
    at += 1;
    let placeholders = digits(at);
    if placeholders > 0 {
        return Some((Piece::Fraction { integer, numerator, denominator: Ok(placeholders) }, at + placeholders));
    }
    let fixed: String = chars[at..].iter().take_while(|c| c.is_ascii_digit()).collect();
    if fixed.is_empty() {
        return None;
    }
    let next = at + fixed.len();
    Some((Piece::Fraction { integer, numerator, denominator: Err(fixed) }, next))
}

/// A month that comes after an hour or before a second is a minute.
fn resolve_minutes(list: &mut [Piece]) {
    let parts: Vec<usize> = (0..list.len())
        .filter(|k| {
            matches!(
                list[*k],
                Piece::Year(_)
                    | Piece::Month { .. }
                    | Piece::Day(_)
                    | Piece::DayOfWeek(_)
                    | Piece::Hour(..)
                    | Piece::Minute(..)
                    | Piece::Second { .. }
            )
        })
        .collect();
    for (n, k) in parts.iter().enumerate() {
        let Piece::Month { long, textual: false } = list[*k] else { continue };
        let after_hour = n > 0 && matches!(list[parts[n - 1]], Piece::Hour(..));
        let before_second = parts.get(n + 1).is_some_and(|next| matches!(list[*next], Piece::Second { .. }));
        if after_hour || before_second {
            list[*k] = Piece::Minute(long, false);
        }
    }
}

/// One piece as an element.
fn piece_xml(piece: &Piece) -> String {
    let long = |on: bool| if on { " number:style=\"long\"" } else { "" };
    match piece {
        Piece::Text(text) => format!("<number:text>{}</number:text>", escape(text)),
        Piece::Fill(c) => format!("<number:fill-character>{}</number:fill-character>", escape(&c.to_string())),
        Piece::TextContent => "<number:text-content/>".to_owned(),
        Piece::General => "<number:number number:min-integer-digits=\"1\"/>".to_owned(),
        Piece::Percent => "<number:text>%</number:text>".to_owned(),
        Piece::Number { integer, embedded, decimals, commas } => {
            number_xml(integer, embedded, decimals, *commas)
        }
        Piece::Scientific { integer, decimals, exponent } => format!(
            "<number:scientific-number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"{}\" number:min-exponent-digits=\"{exponent}\"/>",
            (*integer).max(1)
        ),
        Piece::Fraction { integer, numerator, denominator } => {
            let integer_attr = if *integer { " number:min-integer-digits=\"0\"" } else { "" };
            let denominator_attr = match denominator {
                Ok(count) => format!(" number:min-denominator-digits=\"{count}\""),
                Err(fixed) => format!(" number:denominator-value=\"{fixed}\""),
            };
            format!("<number:fraction{integer_attr} number:min-numerator-digits=\"{numerator}\"{denominator_attr}/>")
        }
        Piece::Year(l) => format!("<number:year{}/>", long(*l)),
        Piece::Month { long: l, textual } => {
            format!("<number:month{}{}/>", long(*l), if *textual { " number:textual=\"true\"" } else { "" })
        }
        Piece::Day(l) => format!("<number:day{}/>", long(*l)),
        Piece::DayOfWeek(l) => format!("<number:day-of-week{}/>", long(*l)),
        Piece::Hour(l, _) => format!("<number:hours{}/>", long(*l)),
        Piece::Minute(l, _) => format!("<number:minutes{}/>", long(*l)),
        Piece::Second { long: l, places, .. } => {
            let places = if *places > 0 { format!(" number:decimal-places=\"{places}\"") } else { String::new() };
            format!("<number:seconds{}{places}/>", long(*l))
        }
        Piece::AmPm => "<number:am-pm/>".to_owned(),
    }
}

/// A number element from its placeholders.
fn number_xml(integer: &[char], embedded: &[(usize, String)], decimals: &[char], commas: usize) -> String {
    let grouping = integer.first() == Some(&',');
    let digits: Vec<&char> = integer.iter().filter(|c| **c != ',').collect();
    let min_integer = digits.iter().filter(|c| ***c == '0').count();
    let min_decimals = decimals.iter().filter(|c| **c == '0').count();
    let mut attrs = format!(
        " number:decimal-places=\"{}\" number:min-decimal-places=\"{min_decimals}\" number:min-integer-digits=\"{min_integer}\"",
        decimals.len()
    );
    if grouping {
        attrs.push_str(" number:grouping=\"true\"");
    }
    if commas > 0 {
        attrs.push_str(&format!(" number:display-factor=\"{}\"", 1000u64.pow(commas as u32)));
    }
    let inner: String = embedded
        .iter()
        .map(|(position, text)| {
            format!("<number:embedded-text number:position=\"{position}\">{}</number:embedded-text>", escape(text))
        })
        .collect();
    if inner.is_empty() {
        format!("<number:number{attrs}/>")
    } else {
        format!("<number:number{attrs}>{inner}</number:number>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A styles document holding the XML written for a code, read back as one code.
    fn round_trip(code: &str) -> String {
        let xml = write_styles(code, "N1");
        let doc = format!(
            "<o:s xmlns:o=\"x\" xmlns:style=\"{NS_STYLE}\" xmlns:number=\"{NS_NUMBER}\" xmlns:fo=\"{NS_FO}\">{xml}</o:s>"
        );
        let parsed = Document::parse(&doc).unwrap();
        read_codes(&[&parsed]).remove("N1").unwrap()
    }

    /// What a value shows as with a code, which is what must survive.
    fn shown(code: &str, value: f64) -> String {
        crate::numfmt::Format::parse(code)
            .render(&crate::value::Value::Number(value), false, 255)
            .text
    }

    #[test]
    fn codes_survive_a_write_and_a_read_by_showing_the_same_text() {
        for (code, values) in [
            ("General", vec![1.5, -2.0]),
            ("0", vec![3.7]),
            ("0.00", vec![3.14159]),
            ("#,##0.00", vec![1234567.891]),
            ("0%", vec![0.256]),
            ("0.0%", vec![0.256]),
            ("0.00E+00", vec![12345.678]),
            ("# ?/?", vec![0.75, 2.5]),
            ("# ??/??", vec![0.3333]),
            ("0.0,,\"M\"", vec![12345678.0]),
            ("[Green]#,##0.00;[Red]-#,##0.00;[Blue]0;\"text:\"@", vec![1.5, -2.5, 0.0]),
            ("#,##0_);(#,##0)", vec![1500.0, -1500.0]),
            ("\"$\"#,##0.00", vec![45.5]),
            ("m/d/yyyy", vec![46318.0]),
            ("d-mmm-yy", vec![46318.0]),
            ("yyyy-mm-dd", vec![46318.0]),
            ("h:mm:ss", vec![0.5208]),
            ("h:mm AM/PM", vec![0.7]),
            ("[h]:mm:ss", vec![1.75]),
            ("[mm]:ss", vec![0.0625]),
            ("mm:ss", vec![0.0006]),
            ("dddd, mmmm d, yyyy", vec![46318.0]),
            ("m/d/yyyy h:mm", vec![46318.5]),
            ("[Red][<=3]0.0;[Blue][>3]0.00", vec![2.0, 5.0]),
            ("000-0000", vec![1234567.0]),
            ("0.000", vec![2.0]),
            ("@", vec![]),
        ] {
            let again = round_trip(code);
            for v in values {
                assert_eq!(shown(&again, v), shown(code, v), "{code} became {again}");
            }
        }
    }

    #[test]
    fn a_style_reads_as_the_code_it_stands_for() {
        assert_eq!(round_trip("0.00"), "0.00");
        assert_eq!(round_trip("#,##0"), "#,##0");
        assert_eq!(round_trip("0%"), "0%");
        assert_eq!(round_trip("yyyy-mm-dd"), "yyyy\\-mm\\-dd");
        assert_eq!(round_trip("[h]:mm:ss"), "[h]:mm:ss");
    }

    #[test]
    fn sections_become_maps_and_join_again() {
        let xml = write_styles("0.0;-0.0;\"zero\"", "N7");
        assert!(xml.contains("value()&gt;0") && xml.contains("value()&lt;0"));
        assert!(xml.contains("N7P0"));
        assert_eq!(round_trip("0.0;-0.0;0"), "0.0;\\-0.0;0");
    }
}
