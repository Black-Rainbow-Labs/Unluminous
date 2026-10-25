//! The clipboard in the formats other programs read and write (task-2359, TDD 5.4 C11 and M20).
//!
//! Copy puts three things on the clipboard: the plain text, HTML (the HTML export of the selection,
//! wrapped in the `HTML Format` header Windows programs read, with the `StartFragment` and `EndFragment`
//! markers Word pastes from) and RTF (paragraphs and runs with bold, italic, underline, strike, colour,
//! size and font). Paste reads HTML first, then RTF, then the text, into paragraphs with the same
//! formatting, and [`PasteAs`] says how much of it the destination keeps.
//!
//! HTML is parsed with `dom_query`, which is `html5ever`'s tree builder, so whatever a browser or Word
//! put on the clipboard is read the way a browser reads it.

use std::sync::Arc;

use crate::model::*;
use crate::props::Rgb;
use crate::xml::El;

/// How much of a paste's formatting the destination keeps, Word's three Paste Options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PasteAs {
    /// The paragraphs and runs as they came.
    #[default]
    Source,
    /// The destination paragraph's style, the source's run formatting.
    Merge,
    /// The text alone, in the destination's formatting.
    Text,
}

impl PasteAs {
    pub fn parse(name: &str) -> Option<PasteAs> {
        Some(match name {
            "source" | "keep" => PasteAs::Source,
            "merge" => PasteAs::Merge,
            "text" | "plain" => PasteAs::Text,
            _ => return None,
        })
    }
}

// ---- Writing ----

/// The `HTML Format` the Windows clipboard holds: a header giving the byte offsets of the document and
/// of the fragment, then the HTML with the two comments that mark the fragment.
pub fn cf_html(fragment: &str) -> String {
    const HEADER: &str = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    let before = "<html><head><meta charset=\"utf-8\"></head><body>\r\n<!--StartFragment-->";
    let after = "<!--EndFragment-->\r\n</body></html>";
    let start_html = HEADER.len();
    let start_fragment = start_html + before.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + after.len();
    format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n{before}{fragment}{after}"
    )
}

/// The HTML inside an `HTML Format` block: the fragment its offsets or its comments mark, or the whole
/// text when it has no header, which is what macOS and Linux hand over.
pub fn html_from_cf(text: &str) -> &str {
    let offset = |name: &str| {
        text.lines()
            .take(8)
            .find_map(|line| line.strip_prefix(name))
            .and_then(|n| n.trim().parse::<usize>().ok())
    };
    if let (Some(start), Some(end)) = (offset("StartFragment:"), offset("EndFragment:")) {
        if start <= end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end)
        {
            return &text[start..end];
        }
    }
    if let (Some(a), Some(b)) = (text.find("<!--StartFragment-->"), text.find("<!--EndFragment-->"))
    {
        if a < b {
            return &text[a + "<!--StartFragment-->".len()..b];
        }
    }
    text
}

/// The paragraphs as RTF: the font and colour tables, then each paragraph's alignment and its runs.
pub fn to_rtf(document: &Document, paragraphs: &[Paragraph]) -> String {
    let resolver = crate::styles::Resolver::new(document);
    let mut fonts: Vec<String> = Vec::new();
    let mut colours: Vec<Rgb> = Vec::new();
    let mut body = String::new();
    for paragraph in paragraphs {
        let look = resolver.paragraph(&paragraph.props, None);
        body.push_str("\\pard\\plain");
        body.push_str(match look.align {
            crate::props::Align::Center => "\\qc",
            crate::props::Align::Right => "\\qr",
            crate::props::Align::Justify => "\\qj",
            crate::props::Align::Left => "\\ql",
        });
        body.push_str(&format!(
            "\\sb{}\\sa{} ",
            (look.space_before * 20.0) as i64,
            (look.space_after * 20.0) as i64
        ));
        for run in &paragraph.runs {
            let text = match &run.content {
                Content::Text(text) => text.as_str(),
                Content::Tab => "\t",
                other if other.character() == Some('\n') => "\n",
                _ => continue,
            };
            if run.is_deleted() {
                continue;
            }
            let run_look = resolver.run(&paragraph.props, &run.props, None);
            let font = fonts.iter().position(|f| **f == *run_look.font).unwrap_or_else(|| {
                fonts.push(run_look.font.to_string());
                fonts.len() - 1
            });
            body.push_str(&format!("{{\\f{font}\\fs{}", (run_look.size * 2.0).round() as i64));
            if run_look.bold {
                body.push_str("\\b");
            }
            if run_look.italic {
                body.push_str("\\i");
            }
            if run_look.underline != crate::props::Underline::None {
                body.push_str("\\ul");
            }
            if run_look.strike {
                body.push_str("\\strike");
            }
            match run_look.vert {
                crate::props::VertAlign::Superscript => body.push_str("\\super"),
                crate::props::VertAlign::Subscript => body.push_str("\\sub"),
                _ => {}
            }
            if let Some(colour) = run_look.color {
                let index = colours.iter().position(|c| *c == colour).unwrap_or_else(|| {
                    colours.push(colour);
                    colours.len() - 1
                });
                body.push_str(&format!("\\cf{}", index + 1));
            }
            body.push(' ');
            body.push_str(&rtf_escape(text));
            body.push('}');
        }
        body.push_str("\\par\r\n");
    }
    let font_table: String = fonts
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{{\\f{i}\\fnil {};}}", rtf_escape(f)))
        .collect();
    let colour_table: String =
        colours.iter().map(|c| format!("\\red{}\\green{}\\blue{};", c.0, c.1, c.2)).collect();
    format!("{{\\rtf1\\ansi\\ansicpg1252\\deff0{{\\fonttbl{font_table}}}{{\\colortbl;{colour_table}}}\r\n{body}}}")
}

/// Text as RTF writes it: the three control characters escaped, a tab and a line break as control
/// words, and anything outside ASCII as `\uN?`.
fn rtf_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\t' => out.push_str("\\tab "),
            '\n' => out.push_str("\\line "),
            c if c.is_ascii() => out.push(c),
            c => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{}?", *unit as i16));
                }
            }
        }
    }
    out
}

// ---- Reading HTML ----

/// What the runs being read carry, from the elements around them.
#[derive(Debug, Clone, Default)]
struct Format {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    sup: bool,
    sub: bool,
    colour: Option<String>,
    size: Option<f32>,
    font: Option<String>,
    link: Option<String>,
    code: bool,
}

impl Format {
    fn props(&self) -> Arc<El> {
        let mut rpr = El::new("w:rPr");
        let font = self.font.clone().or_else(|| self.code.then(|| "Consolas".to_owned()));
        if let Some(font) = font {
            rpr.put(El::new("w:rFonts").with("w:ascii", font.clone()).with("w:hAnsi", font));
        }
        if self.bold {
            rpr.put(El::new("w:b"));
        }
        if self.italic {
            rpr.put(El::new("w:i"));
        }
        if self.strike {
            rpr.put(El::new("w:strike"));
        }
        if let Some(colour) = &self.colour {
            rpr.put(El::new("w:color").with("w:val", colour.clone()));
        }
        if let Some(size) = self.size {
            rpr.put(El::new("w:sz").with("w:val", ((size * 2.0).round() as i64).to_string()));
        }
        if self.underline || self.link.is_some() {
            rpr.put(El::new("w:u").with("w:val", "single"));
        }
        if self.link.is_some() && self.colour.is_none() {
            rpr.put(El::new("w:color").with("w:val", "0563C1"));
        }
        if self.sup || self.sub {
            rpr.put(
                El::new("w:vertAlign")
                    .with("w:val", if self.sup { "superscript" } else { "subscript" }),
            );
        }
        crate::order::sort(&mut rpr);
        Arc::new(rpr)
    }

    /// The format with an element's own meaning and its `style` attribute added.
    fn with(&self, name: &str, style: Option<&str>) -> Format {
        let mut f = self.clone();
        match name {
            "b" | "strong" => f.bold = true,
            "i" | "em" | "cite" | "var" => f.italic = true,
            "u" | "ins" => f.underline = true,
            "s" | "strike" | "del" => f.strike = true,
            "sup" => f.sup = true,
            "sub" => f.sub = true,
            "code" | "kbd" | "samp" | "tt" | "pre" => f.code = true,
            _ => {}
        }
        if let Some(style) = style {
            for (key, value) in css(style) {
                match key.as_str() {
                    "font-weight" => {
                        f.bold = value == "bold"
                            || value == "bolder"
                            || value.parse::<u32>().is_ok_and(|w| w >= 600)
                    }
                    "font-style" => f.italic = value == "italic" || value == "oblique",
                    "text-decoration" | "text-decoration-line" => {
                        f.underline |= value.contains("underline");
                        f.strike |= value.contains("line-through");
                    }
                    "color" => f.colour = css_colour(&value).or(f.colour),
                    "font-size" => f.size = css_points(&value).or(f.size),
                    "font-family" => {
                        f.font = value
                            .split(',')
                            .map(|s| s.trim().trim_matches(['\'', '"']).to_owned())
                            .find(|s| !s.is_empty())
                            .or(f.font)
                    }
                    "vertical-align" if value == "super" => f.sup = true,
                    "vertical-align" if value == "sub" => f.sub = true,
                    _ => {}
                }
            }
        }
        f
    }
}

/// `a: b; c: d` as lower case pairs.
fn css(style: &str) -> Vec<(String, String)> {
    style
        .split(';')
        .filter_map(|pair| {
            let (key, value) = pair.split_once(':')?;
            Some((
                key.trim().to_ascii_lowercase(),
                value.trim().to_ascii_lowercase().replace("!important", "").trim().to_owned(),
            ))
        })
        .collect()
}

/// `#rgb`, `#rrggbb`, `rgb(r, g, b)` and a few names, as `RRGGBB`.
fn css_colour(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return match hex.len() {
            6 => Some(hex.to_ascii_uppercase()),
            3 => Some(hex.chars().flat_map(|c| [c, c]).collect::<String>().to_ascii_uppercase()),
            _ => None,
        };
    }
    if let Some(inner) = value.strip_prefix("rgb(").or_else(|| value.strip_prefix("rgba(")) {
        let parts: Vec<u8> = inner
            .trim_end_matches(')')
            .split(',')
            .take(3)
            .filter_map(|p| p.trim().parse::<f32>().ok())
            .map(|v| v.clamp(0.0, 255.0) as u8)
            .collect();
        return (parts.len() == 3)
            .then(|| format!("{:02X}{:02X}{:02X}", parts[0], parts[1], parts[2]));
    }
    Some(
        match value {
            "black" => "000000",
            "white" => "FFFFFF",
            "red" => "FF0000",
            "green" => "008000",
            "blue" => "0000FF",
            "gray" | "grey" => "808080",
            "orange" => "FFA500",
            "purple" => "800080",
            _ => return None,
        }
        .to_owned(),
    )
}

/// A CSS length in points: `pt`, `px` (three quarters of a point), `em` against 11 points.
fn css_points(value: &str) -> Option<f32> {
    let value = value.trim();
    let number =
        |suffix: &str| value.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    number("pt")
        .or_else(|| number("px").map(|px| px * 0.75))
        .or_else(|| number("em").map(|em| em * 11.0))
}

/// Paragraphs read from HTML. A heading becomes a Heading style, a list item a List Bullet or List
/// Number paragraph, `pre` keeps its lines, a table row one paragraph with its cells between tabs, a link
/// a hyperlink (a relationship of `document`'s main part), and a picture in a data address a picture.
pub fn from_html(document: &mut Document, html: &str) -> Vec<Paragraph> {
    let parsed = dom_query::Document::from(html);
    let mut reader = HtmlReader { document, out: Vec::new(), current: None, lists: Vec::new() };
    let root = parsed.body().unwrap_or_else(|| parsed.root());
    reader.walk(&root, &Format::default(), None);
    reader.finish();
    reader.out
}

struct HtmlReader<'a> {
    document: &'a mut Document,
    out: Vec<Paragraph>,
    /// The paragraph being filled, and whether its text is still at the start (so leading white space
    /// is dropped).
    current: Option<Paragraph>,
    /// The lists open around the point, `true` for an ordered one.
    lists: Vec<bool>,
}

const BLOCKS: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "pre",
    "blockquote",
    "tr",
    "table",
    "ul",
    "ol",
    "section",
    "article",
    "header",
    "footer",
    "dd",
    "dt",
    "figure",
    "figcaption",
    "address",
];

impl HtmlReader<'_> {
    fn paragraph(&mut self) -> &mut Paragraph {
        self.current.get_or_insert_with(Paragraph::new)
    }

    /// End the paragraph being filled, if it has anything in it or `keep_empty`.
    fn end(&mut self, keep_empty: bool) {
        if let Some(mut p) = self.current.take() {
            // Trailing white space at the end of a paragraph is not shown by a browser either.
            if let Some(Run { content: Content::Text(t), .. }) = p.runs.last_mut() {
                let trimmed = t.trim_end().to_owned();
                *t = trimmed;
            }
            p.merge_runs();
            if keep_empty || !p.runs.is_empty() {
                self.out.push(p);
            }
        }
    }

    fn finish(&mut self) {
        self.end(false);
    }

    fn style_for(&self, name: &str) -> Option<String> {
        Some(match name {
            "h1" => "Heading1".into(),
            "h2" => "Heading2".into(),
            "h3" => "Heading3".into(),
            "h4" => "Heading4".into(),
            "h5" => "Heading5".into(),
            "h6" => "Heading6".into(),
            "blockquote" => "Quote".into(),
            "li" => match self.lists.last() {
                Some(true) => "ListNumber".into(),
                _ => "ListBullet".into(),
            },
            "figcaption" => "Caption".into(),
            _ => return None,
        })
    }

    fn walk(&mut self, node: &dom_query::NodeRef, format: &Format, pre: Option<()>) {
        for child in node.children() {
            if child.is_text() {
                let text = child.text().to_string();
                let text = if pre.is_some() {
                    text
                } else {
                    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let lead = text.starts_with(char::is_whitespace) && !collapsed.is_empty();
                    let trail = text.ends_with(char::is_whitespace) && !collapsed.is_empty();
                    let starting = self
                        .current
                        .as_ref()
                        .is_none_or(|p| p.runs.is_empty() || p.flat_text().ends_with(' '));
                    if collapsed.is_empty() {
                        if !starting && text.chars().any(char::is_whitespace) {
                            " ".to_owned()
                        } else {
                            String::new()
                        }
                    } else {
                        format!(
                            "{}{collapsed}{}",
                            if lead && !starting { " " } else { "" },
                            if trail { " " } else { "" }
                        )
                    }
                };
                if text.is_empty() {
                    continue;
                }
                let props = format.props();
                let link = format.link.clone();
                let main = self.document.main.clone();
                let wrap = link.map(|address| {
                    let mut el = El::new("w:hyperlink").with("w:history", "1");
                    match address.strip_prefix('#') {
                        Some(anchor) => el.set_attr("w:anchor", anchor),
                        None => {
                            let id = self.document.package.rels_mut(&main).add(
                                crate::package::kind::HYPERLINK,
                                &address,
                                true,
                            );
                            el.set_attr("r:id", id);
                        }
                    }
                    Wrap::new(el)
                });
                let paragraph = self.paragraph();
                for (index, line) in text.split('\n').enumerate() {
                    if index > 0 {
                        paragraph.runs.push(Run {
                            content: Content::Break(El::new("w:br")),
                            ..Run::text("", Arc::clone(&props))
                        });
                    }
                    for (k, piece) in line.split('\t').enumerate() {
                        if k > 0 {
                            paragraph.runs.push(Run {
                                content: Content::Tab,
                                ..Run::text("", Arc::clone(&props))
                            });
                        }
                        if !piece.is_empty() {
                            let mut run = Run::text(piece, Arc::clone(&props));
                            if let Some(wrap) = &wrap {
                                run.wraps.push(Arc::clone(wrap));
                            }
                            paragraph.runs.push(run);
                        }
                    }
                }
                continue;
            }
            if !child.is_element() {
                continue;
            }
            let name =
                child.node_name().map(|n| n.to_string().to_ascii_lowercase()).unwrap_or_default();
            if matches!(name.as_str(), "script" | "style" | "head" | "title" | "meta" | "template")
            {
                continue;
            }
            let style = child.attr("style").map(|s| s.to_string());
            let mut inner = format.with(&name, style.as_deref());
            match name.as_str() {
                "br" => {
                    self.paragraph().runs.push(Run {
                        content: Content::Break(El::new("w:br")),
                        ..Run::text("", format.props())
                    });
                    continue;
                }
                "a" => {
                    if let Some(href) = child.attr("href") {
                        inner.link = Some(href.to_string());
                    }
                }
                "img" => {
                    self.picture(&child);
                    continue;
                }
                "td" | "th" => {
                    // A cell after the first starts after a tab.
                    if child.prev_element_sibling().is_some() {
                        self.paragraph()
                            .runs
                            .push(Run { content: Content::Tab, ..Run::text("", no_props()) });
                    }
                    if name == "th" {
                        inner.bold = true;
                    }
                    self.walk(&child, &inner, pre);
                    continue;
                }
                _ => {}
            }
            let is_block = BLOCKS.contains(&name.as_str());
            if !is_block {
                self.walk(&child, &inner, pre);
                continue;
            }
            match name.as_str() {
                "ul" | "ol" => {
                    self.end(false);
                    self.lists.push(name == "ol");
                    self.walk(&child, &inner, pre);
                    self.end(false);
                    self.lists.pop();
                }
                "table" => {
                    self.end(false);
                    self.walk(&child, &inner, pre);
                    self.end(false);
                }
                _ => {
                    self.end(false);
                    let mut paragraph = Paragraph::new();
                    if let Some(id) = self.style_for(&name) {
                        paragraph.props.put(El::new("w:pStyle").with("w:val", id));
                    }
                    if let Some(style) = &style {
                        for (key, value) in css(style) {
                            if key == "text-align" {
                                let jc = match value.as_str() {
                                    "center" => "center",
                                    "right" => "right",
                                    "justify" => "both",
                                    _ => continue,
                                };
                                paragraph.props.put(El::new("w:jc").with("w:val", jc));
                            }
                        }
                    }
                    if name == "pre" {
                        paragraph.props.put(El::new("w:pStyle").with("w:val", "HTMLPreformatted"));
                    }
                    crate::order::sort(&mut paragraph.props);
                    self.current = Some(paragraph);
                    let pre = if name == "pre" { Some(()) } else { pre };
                    self.walk(&child, &inner, pre);
                    self.end(name == "p");
                }
            }
        }
    }

    /// A picture whose source is a `data:` address, put in the document as Word puts one.
    fn picture(&mut self, node: &dom_query::NodeRef) {
        let Some(src) = node.attr("src").map(|s| s.to_string()) else { return };
        let Some(rest) = src.strip_prefix("data:image/") else { return };
        let Some((kind, data)) = rest.split_once(";base64,") else { return };
        let Some(bytes) = base64_decode(data) else { return };
        let extension = match kind {
            "jpeg" | "jpg" => "jpg",
            "gif" => "gif",
            "bmp" => "bmp",
            _ => "png",
        };
        let (w, h) = crate::media::picture_size(&bytes).unwrap_or((96, 96));
        let width =
            node.attr("width").and_then(|v| v.parse::<f32>().ok()).unwrap_or(w as f32) * 0.75;
        let height = node
            .attr("height")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(width / 0.75 * h as f32 / w.max(1) as f32)
            * 0.75;
        let drawing =
            crate::media::add_picture(self.document, &bytes, extension, width, height, "Picture");
        self.paragraph()
            .runs
            .push(Run { content: Content::Object(drawing), ..Run::text("", no_props()) });
    }
}

/// Standard base64, ignoring white space.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let clean: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

// ---- Reading RTF ----

/// Paragraphs read from RTF: the character formatting a run carries (bold, italic, underline, strike,
/// super and subscript, size, colour, font), paragraph alignment, tabs and line breaks, and Unicode.
/// Destinations a reader is told to skip (`\*`, the font and colour tables, the style sheet, the
/// document information, pictures) are skipped.
pub fn from_rtf(rtf: &str) -> Vec<Paragraph> {
    #[derive(Clone, Default)]
    struct State {
        format: Format,
        skip: bool,
        unicode_skip: usize,
    }
    let mut out = Vec::new();
    let mut paragraph = Paragraph::new();
    let mut stack: Vec<State> = Vec::new();
    let mut state = State { unicode_skip: 1, ..State::default() };
    let mut fonts: Vec<(i64, String)> = Vec::new();
    let mut colours: Vec<String> = Vec::new();
    let mut colour = (0u8, 0u8, 0u8);
    let mut table: Option<&'static str> = None;
    let mut pending_skip = 0usize;
    let mut font_name = String::new();
    let mut font_number = 0i64;
    let mut align: Option<&'static str> = None;
    let bytes = rtf.as_bytes();
    let mut i = 0;
    let push_text = |paragraph: &mut Paragraph, state: &State, text: &str| {
        if state.skip || text.is_empty() {
            return;
        }
        let props = state.format.props();
        if let Some(Run { content: Content::Text(t), props: p, wraps, .. }) =
            paragraph.runs.last_mut()
        {
            if wraps.is_empty() && **p == *props {
                t.push_str(text);
                return;
            }
        }
        paragraph.runs.push(Run::text(text, props));
    };
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'{' => {
                stack.push(state.clone());
                i += 1;
            }
            b'}' => {
                if let Some(t) = table {
                    if t == "font" && !font_name.is_empty() {
                        fonts
                            .push((font_number, font_name.trim_end_matches(';').trim().to_owned()));
                        font_name.clear();
                    }
                }
                let was_skip = state.skip;
                state = stack.pop().unwrap_or_default();
                if was_skip && !state.skip {
                    table = None;
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&next) = bytes.get(i) else { break };
                if !next.is_ascii_alphabetic() {
                    i += 1;
                    match next {
                        b'\\' | b'{' | b'}' => {
                            if pending_skip > 0 {
                                pending_skip -= 1;
                            } else if table == Some("font") {
                                font_name.push(next as char);
                            } else {
                                push_text(&mut paragraph, &state, &(next as char).to_string());
                            }
                        }
                        b'*' => state.skip = true,
                        b'~' => push_text(&mut paragraph, &state, "\u{a0}"),
                        b'-' | b'_' => {}
                        b'\'' => {
                            let hex = rtf.get(i..i + 2).unwrap_or("00");
                            i += 2;
                            if pending_skip > 0 {
                                pending_skip -= 1;
                            } else if let Ok(byte) = u8::from_str_radix(hex, 16) {
                                let ch = windows_1252(byte);
                                if table == Some("font") {
                                    font_name.push(ch);
                                } else {
                                    push_text(&mut paragraph, &state, &ch.to_string());
                                }
                            }
                        }
                        b'\r' | b'\n' => {
                            end_paragraph(&mut out, &mut paragraph, align);
                        }
                        _ => {}
                    }
                    continue;
                }
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &rtf[start..i];
                let number_start = i;
                if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let number: Option<i64> = rtf[number_start..i].parse().ok();
                if i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                let on = number != Some(0);
                match word {
                    "fonttbl" => {
                        table = Some("font");
                        state.skip = true;
                    }
                    "colortbl" => {
                        table = Some("colour");
                        state.skip = true;
                    }
                    "stylesheet" | "info" | "pict" | "header" | "footer" | "footnote"
                    | "object" | "listtable" | "listoverridetable" | "rsidtbl" | "generator"
                    | "themedata" | "colorschememapping" | "latentstyles" | "datastore"
                    | "xmlnstbl" => state.skip = true,
                    "f" if table == Some("font") => font_number = number.unwrap_or(0),
                    "f" => {
                        state.format.font = fonts
                            .iter()
                            .find(|(n, _)| Some(*n) == number)
                            .map(|(_, name)| name.clone())
                    }
                    "red" => colour.0 = number.unwrap_or(0) as u8,
                    "green" => colour.1 = number.unwrap_or(0) as u8,
                    "blue" => colour.2 = number.unwrap_or(0) as u8,
                    "b" => state.format.bold = on,
                    "i" => state.format.italic = on,
                    "ul" => state.format.underline = on,
                    "ulnone" => state.format.underline = false,
                    "strike" => state.format.strike = on,
                    "super" => state.format.sup = true,
                    "sub" => state.format.sub = true,
                    "nosupersub" => {
                        state.format.sup = false;
                        state.format.sub = false;
                    }
                    "fs" => state.format.size = number.map(|n| n as f32 / 2.0),
                    "cf" => {
                        state.format.colour =
                            number.filter(|n| *n > 0).and_then(|n| colours.get(n as usize).cloned())
                    }
                    "plain" => {
                        let font = state.format.font.take();
                        state.format = Format { font, ..Format::default() };
                    }
                    "pard" => align = None,
                    "qc" => align = Some("center"),
                    "qr" => align = Some("right"),
                    "qj" => align = Some("both"),
                    "ql" => align = None,
                    "par" | "sect" | "page" => end_paragraph(&mut out, &mut paragraph, align),
                    "line" => {
                        if !state.skip {
                            paragraph.runs.push(Run {
                                content: Content::Break(El::new("w:br")),
                                ..Run::text("", state.format.props())
                            });
                        }
                    }
                    "tab" => {
                        if !state.skip {
                            paragraph.runs.push(Run {
                                content: Content::Tab,
                                ..Run::text("", state.format.props())
                            });
                        }
                    }
                    "cell" => push_text(&mut paragraph, &state, "\t"),
                    "row" => end_paragraph(&mut out, &mut paragraph, align),
                    "uc" => state.unicode_skip = number.unwrap_or(1).max(0) as usize,
                    "u" => {
                        let unit = number.unwrap_or(0);
                        let unit = if unit < 0 { unit + 65536 } else { unit } as u32;
                        if let Some(ch) = char::from_u32(unit) {
                            push_text(&mut paragraph, &state, &ch.to_string());
                        }
                        pending_skip = state.unicode_skip;
                    }
                    "emdash" => push_text(&mut paragraph, &state, "—"),
                    "endash" => push_text(&mut paragraph, &state, "–"),
                    "lquote" => push_text(&mut paragraph, &state, "‘"),
                    "rquote" => push_text(&mut paragraph, &state, "’"),
                    "ldblquote" => push_text(&mut paragraph, &state, "“"),
                    "rdblquote" => push_text(&mut paragraph, &state, "”"),
                    "bullet" => push_text(&mut paragraph, &state, "•"),
                    _ => {}
                }
            }
            b'\r' | b'\n' => i += 1,
            b';' if table == Some("colour") => {
                colours.push(format!("{:02X}{:02X}{:02X}", colour.0, colour.1, colour.2));
                colour = (0, 0, 0);
                i += 1;
            }
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'{' | b'}' | b'\\' | b'\r' | b'\n') {
                    if table == Some("colour") && bytes[i] == b';' {
                        break;
                    }
                    i += 1;
                }
                let mut text = &rtf[start..i];
                while pending_skip > 0 && !text.is_empty() {
                    let skip = text.chars().next().map_or(0, char::len_utf8);
                    text = &text[skip..];
                    pending_skip -= 1;
                }
                if table == Some("font") {
                    font_name.push_str(text);
                } else {
                    push_text(&mut paragraph, &state, text);
                }
            }
        }
    }
    if !paragraph.runs.is_empty() {
        end_paragraph(&mut out, &mut paragraph, align);
    }
    // The colour table's first entry, the empty one before the first `;`, is the automatic colour, which
    // `\cf0` names, so `\cfN` is entry N.
    out
}

fn end_paragraph(out: &mut Vec<Paragraph>, paragraph: &mut Paragraph, align: Option<&str>) {
    let mut done = std::mem::replace(paragraph, Paragraph::new());
    if let Some(jc) = align {
        done.props.put(El::new("w:jc").with("w:val", jc));
    }
    out.push(done);
}

/// A byte of Windows code page 1252 as the character it stands for.
fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => HIGH[(byte - 0x80) as usize],
        other => other as char,
    }
}

// ---- Paste options ----

/// The paragraphs a paste puts in, as `how` asks: `Merge` takes away each paragraph's own properties so
/// it takes the destination's, `Text` also takes away each run's.
pub fn apply_paste_as(mut paragraphs: Vec<Paragraph>, how: PasteAs) -> Vec<Paragraph> {
    match how {
        PasteAs::Source => {}
        PasteAs::Merge => {
            for p in &mut paragraphs {
                p.props = El::new("w:pPr");
            }
        }
        PasteAs::Text => {
            for p in &mut paragraphs {
                p.props = El::new("w:pPr");
                for run in &mut p.runs {
                    run.props = no_props();
                    run.wraps.clear();
                }
                p.runs.retain(|r| !matches!(r.content, Content::Object(_)));
                p.merge_runs();
            }
        }
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_html_format_offsets_point_at_the_fragment() {
        let wrapped = cf_html("<p>Hé</p>");
        assert_eq!(html_from_cf(&wrapped), "<p>Hé</p>");
        assert_eq!(html_from_cf("<b>plain</b>"), "<b>plain</b>");
    }

    #[test]
    fn html_becomes_styled_paragraphs() {
        let mut document = crate::new::blank();
        let html = "<h2>Title</h2><p>Some <b>bold</b> and <span style=\"color:#ff0000;font-size:14pt\">red</span>.</p><ul><li>one</li><li>two</li></ul><p><a href=\"https://example.com\">link</a></p><table><tr><td>a</td><td>b</td></tr></table><style>p{}</style>";
        let out = from_html(&mut document, html);
        let texts: Vec<String> = out.iter().map(Paragraph::flat_text).collect();
        assert_eq!(texts, ["Title", "Some bold and red.", "one", "two", "link", "a\tb"]);
        assert_eq!(out[0].props.val_of("w:pStyle"), Some("Heading2"));
        assert_eq!(out[2].props.val_of("w:pStyle"), Some("ListBullet"));
        let bold = out[1]
            .runs
            .iter()
            .find(|r| matches!(&r.content, Content::Text(t) if t == "bold"))
            .unwrap();
        assert!(bold.props.child("w:b").is_some());
        let red = out[1]
            .runs
            .iter()
            .find(|r| matches!(&r.content, Content::Text(t) if t == "red"))
            .unwrap();
        assert_eq!(red.props.val_of("w:color"), Some("FF0000"));
        assert_eq!(red.props.val_of("w:sz"), Some("28"));
        assert!(out[4].runs[0].link().is_some());
    }

    #[test]
    fn rtf_written_reads_back_with_its_formatting() {
        let mut document = crate::new::blank();
        let html =
            "<p>Plain <b>bold</b> <i>it</i> café “q”</p><p style=\"text-align:center\">mid</p>";
        let paragraphs = from_html(&mut document, html);
        let rtf = to_rtf(&document, &paragraphs);
        let back = from_rtf(&rtf);
        let texts: Vec<String> = back.iter().map(Paragraph::flat_text).collect();
        assert_eq!(texts, ["Plain bold it café “q”", "mid"]);
        assert!(back[0].runs.iter().any(|r| r.props.child("w:b").is_some()
            && matches!(&r.content, Content::Text(t) if t == "bold")));
        assert_eq!(back[1].props.val_of("w:jc"), Some("center"));
    }

    #[test]
    fn word_rtf_with_tables_and_escapes_reads() {
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0\fswiss Arial;}}{\colortbl;\red255\green0\blue0;}\pard\plain\f0\fs24 Hello {\b world}\'e9 \cf1 red\cf0\par Next\u8212? done\par}";
        let out = from_rtf(rtf);
        let texts: Vec<String> = out.iter().map(Paragraph::flat_text).collect();
        assert_eq!(texts, ["Hello worldé red", "Next— done"]);
        let red = out[0]
            .runs
            .iter()
            .find(|r| matches!(&r.content, Content::Text(t) if t.contains("red")))
            .unwrap();
        assert_eq!(red.props.val_of("w:color"), Some("FF0000"));
        assert_eq!(red.props.val_of("w:rFonts").is_none(), true);
        assert_eq!(red.props.child("w:rFonts").and_then(|f| f.attr("w:ascii")), Some("Arial"));
    }

    #[test]
    fn paste_options_strip_what_they_say() {
        let mut document = crate::new::blank();
        let p = from_html(&mut document, "<h1>A <b>b</b></h1>");
        let merged = apply_paste_as(p.clone(), PasteAs::Merge);
        assert!(merged[0].props.val_of("w:pStyle").is_none());
        assert!(merged[0].runs.iter().any(|r| r.props.child("w:b").is_some()));
        let text = apply_paste_as(p, PasteAs::Text);
        assert!(text[0].runs.iter().all(|r| r.props.child("w:b").is_none()));
        assert_eq!(text[0].flat_text(), "A b");
    }
}
