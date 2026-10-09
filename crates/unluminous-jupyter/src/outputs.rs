//! What drawing a notebook output needs, as functions that know nothing about a window.
//!
//! A cell output is a JSON object that may carry the same result in several forms (a picture, an HTML
//! table, plain text). [`shown`] decides which form to draw. The other functions turn the awkward
//! forms into data a window can lay out: terminal colour codes become [`Span`]s, a pandas HTML table
//! becomes a [`Table`], a progress bar written with carriage returns becomes the line a terminal would
//! have shown, and HTML that is not a table becomes plain text.
//!
//! The HTML reader here is small and tolerant. It is not a browser. It reads what kernels write for
//! tables and ignores what it does not understand.

use crate::nbformat::Output;

// ---------------------------------------------------------------------------------------------
// Choosing what to draw
// ---------------------------------------------------------------------------------------------

/// What to draw for one output.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// Markdown source, to be drawn as formatted text.
    Markdown(String),
    /// An HTML table that [`parse_table`] could read.
    Table(Table),
    /// HTML that is not a table. The string is the plain text of that HTML.
    Html(String),
    /// The bytes of a PNG picture.
    Png(Vec<u8>),
    /// The bytes of a JPEG picture.
    Jpeg(Vec<u8>),
    /// The source of an SVG picture.
    Svg(String),
    /// LaTeX source.
    Latex(String),
    /// JSON, already pretty printed.
    Json(String),
    /// Plain text.
    Text(String),
    /// Text a cell printed. Lines drawn with carriage returns are already collapsed.
    Stream { stderr: bool, text: String },
    /// An exception. Each traceback line is split into coloured spans.
    Error { ename: String, evalue: String, traceback: Vec<Vec<Span>> },
}

/// Chooses the form of an output to draw.
///
/// A stream and an error have one form each. For a result or display output the first of these that
/// the output holds wins: `text/markdown`, `text/html`, `image/png`, `image/jpeg`, `image/svg+xml`,
/// `text/latex`, `application/json`, `text/plain`. HTML whose text is empty (a script that draws a
/// widget, for example) is passed over so that the plain text fallback is drawn, and so is a picture
/// whose base64 text cannot be decoded. An output with none of them is drawn as empty text.
pub fn shown(output: &Output) -> Shown {
    match output.output_type() {
        "stream" => Shown::Stream {
            stderr: output.stream_name() == Some("stderr"),
            text: collapse_carriage_returns(&output.text().unwrap_or_default()),
        },
        "error" => Shown::Error {
            ename: output.ename().unwrap_or_default().to_string(),
            evalue: output.evalue().unwrap_or_default().to_string(),
            traceback: output.traceback().iter().map(|line| ansi_spans(line)).collect(),
        },
        _ => shown_bundle(output),
    }
}

/// Picks from a mime bundle in the order of preference, ending with plain text.
fn shown_bundle(output: &Output) -> Shown {
    shown_markup(output)
        .or_else(|| shown_picture(output))
        .or_else(|| shown_source(output))
        .unwrap_or_else(|| Shown::Text(output.mime_text("text/plain").unwrap_or_default()))
}

/// Markdown, then HTML.
fn shown_markup(output: &Output) -> Option<Shown> {
    if let Some(markdown) = output.mime_text("text/markdown") {
        return Some(Shown::Markdown(markdown));
    }
    let html = output.mime_text("text/html")?;
    if let Some(table) = parse_table(&html) {
        return Some(Shown::Table(table));
    }
    let text = html_to_text(&html);
    (!text.trim().is_empty()).then_some(Shown::Html(text))
}

/// PNG, JPEG, then SVG.
fn shown_picture(output: &Output) -> Option<Shown> {
    let decode = |mime: &str| {
        output
            .mime_text(mime)
            .and_then(|text| base64_decode(&text))
            .filter(|bytes| !bytes.is_empty())
    };
    if let Some(bytes) = decode("image/png") {
        return Some(Shown::Png(bytes));
    }
    if let Some(bytes) = decode("image/jpeg") {
        return Some(Shown::Jpeg(bytes));
    }
    output.mime_text("image/svg+xml").map(Shown::Svg)
}

/// LaTeX, then JSON.
fn shown_source(output: &Output) -> Option<Shown> {
    if let Some(latex) = output.mime_text("text/latex") {
        return Some(Shown::Latex(latex));
    }
    let json = output.data()?.get("application/json")?;
    Some(Shown::Json(serde_json::to_string_pretty(json).unwrap_or_else(|_| json.to_string())))
}

// ---------------------------------------------------------------------------------------------
// Base64
// ---------------------------------------------------------------------------------------------

/// Decodes standard alphabet base64. Spaces and line breaks are skipped, because notebooks store
/// pictures in lines. Decoding stops at the first `=`. It returns `None` for any other character
/// outside the alphabet.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
    let (mut accumulated, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            other if other.is_ascii_whitespace() => continue,
            _ => return None,
        };
        accumulated = (accumulated << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((accumulated >> bits) as u8);
            accumulated &= (1 << bits) - 1;
        }
    }
    Some(bytes)
}

/// Encodes bytes as standard alphabet base64 with padding and no line breaks, which is what a
/// `data:` address needs.
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for position in 0..4 {
            if position <= chunk.len() {
                text.push(ALPHABET[((group >> (18 - 6 * position)) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

// ---------------------------------------------------------------------------------------------
// ANSI colour codes
// ---------------------------------------------------------------------------------------------

/// A terminal colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ansi {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// A colour given as red, green and blue by `38;2;r;g;b`.
    Rgb(u8, u8, u8),
    /// A colour from the 256 colour table, given by `38;5;n`.
    Indexed(u8),
}

/// A run of text that was drawn in one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub colour: Option<Ansi>,
    pub bold: bool,
    pub background: Option<Ansi>,
}

/// The style that is in force while text is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Style {
    colour: Option<Ansi>,
    bold: bool,
    background: Option<Ansi>,
}

const NORMAL_COLOURS: [Ansi; 8] = [
    Ansi::Black,
    Ansi::Red,
    Ansi::Green,
    Ansi::Yellow,
    Ansi::Blue,
    Ansi::Magenta,
    Ansi::Cyan,
    Ansi::White,
];
const BRIGHT_COLOURS: [Ansi; 8] = [
    Ansi::BrightBlack,
    Ansi::BrightRed,
    Ansi::BrightGreen,
    Ansi::BrightYellow,
    Ansi::BrightBlue,
    Ansi::BrightMagenta,
    Ansi::BrightCyan,
    Ansi::BrightWhite,
];

/// Splits text into spans of one style each. The select graphic rendition codes `ESC [ ... m` set
/// the style: 0 resets it, 1 and 22 turn bold on and off, 30 to 37 and 90 to 97 set the colour, 39
/// clears it, 40 to 47 and 100 to 107 set the background, 49 clears it, and 38 and 48 take either
/// `5;n` or `2;r;g;b`. Every other escape sequence (cursor movement, erasing, window titles ended by
/// the bell or by `ESC \`) is removed. No span is empty, and text with no escapes is one span.
pub fn ansi_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut style = Style::default();
    let mut pending = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            pending.push(c);
            continue;
        }
        if let Some(params) = read_escape(&mut chars) {
            let next = apply_sgr(style, &params);
            if next != style {
                flush_span(&mut spans, &mut pending, style);
                style = next;
            }
        }
    }
    flush_span(&mut spans, &mut pending, style);
    spans
}

/// The text with every escape sequence removed.
pub fn strip_ansi(text: &str) -> String {
    ansi_spans(text).into_iter().map(|span| span.text).collect()
}

/// Moves pending text into a new span, unless there is none.
fn flush_span(spans: &mut Vec<Span>, pending: &mut String, style: Style) {
    if !pending.is_empty() {
        spans.push(Span {
            text: std::mem::take(pending),
            colour: style.colour,
            bold: style.bold,
            background: style.background,
        });
    }
}

/// Reads the rest of an escape sequence after the `ESC` character. It returns the numbers of a
/// colour code (`m` sequence) and `None` for every other sequence, which is consumed and dropped.
fn read_escape(chars: &mut std::str::Chars) -> Option<Vec<u32>> {
    match chars.next()? {
        '[' => {
            let mut parameters = String::new();
            loop {
                match chars.next()? {
                    c if ('\u{20}'..='\u{3f}').contains(&c) => parameters.push(c),
                    'm' => return Some(parse_parameters(&parameters)),
                    c if ('@'..='~').contains(&c) => return None,
                    _ => return None,
                }
            }
        }
        ']' => loop {
            match chars.next()? {
                '\u{7}' => return None,
                '\u{1b}' => {
                    chars.next();
                    return None;
                }
                _ => {}
            }
        },
        c if ('\u{20}'..='\u{2f}').contains(&c) => {
            // A sequence such as `ESC ( B` has intermediate characters and then one final one.
            for next in chars.by_ref() {
                if !('\u{20}'..='\u{2f}').contains(&next) {
                    break;
                }
            }
            None
        }
        _ => None,
    }
}

/// The numbers of a colour code. An empty number counts as 0, so `ESC [ m` resets the style.
fn parse_parameters(parameters: &str) -> Vec<u32> {
    parameters.split([';', ':']).map(|part| part.parse().unwrap_or(0)).collect()
}

/// Applies the numbers of one colour code to a style.
fn apply_sgr(mut style: Style, parameters: &[u32]) -> Style {
    let mut index = 0;
    while index < parameters.len() {
        let code = parameters[index];
        match code {
            0 => style = Style::default(),
            1 => style.bold = true,
            22 => style.bold = false,
            30..=37 => style.colour = Some(NORMAL_COLOURS[(code - 30) as usize]),
            39 => style.colour = None,
            40..=47 => style.background = Some(NORMAL_COLOURS[(code - 40) as usize]),
            49 => style.background = None,
            90..=97 => style.colour = Some(BRIGHT_COLOURS[(code - 90) as usize]),
            100..=107 => style.background = Some(BRIGHT_COLOURS[(code - 100) as usize]),
            38 | 48 => {
                let (colour, used) = read_extended_colour(&parameters[index + 1..]);
                if code == 38 {
                    style.colour = colour;
                } else {
                    style.background = colour;
                }
                index += used;
            }
            _ => {}
        }
        index += 1;
    }
    style
}

/// Reads what follows 38 or 48: `5;n` or `2;r;g;b`. It returns the colour and how many numbers it
/// used. A sequence cut short uses what is left and gives no colour.
fn read_extended_colour(rest: &[u32]) -> (Option<Ansi>, usize) {
    match rest.first() {
        Some(5) => match rest.get(1) {
            Some(&number) => (Some(Ansi::Indexed(number.min(255) as u8)), 2),
            None => (None, rest.len()),
        },
        Some(2) if rest.len() >= 4 => (
            Some(Ansi::Rgb(rest[1].min(255) as u8, rest[2].min(255) as u8, rest[3].min(255) as u8)),
            4,
        ),
        _ => (None, rest.len()),
    }
}

// ---------------------------------------------------------------------------------------------
// Streams
// ---------------------------------------------------------------------------------------------

/// Draws text the way a terminal would when it contains carriage returns.
///
/// Within a line, a `\r` that is not part of `\r\n` moves back to the start of the line and the
/// text after it overwrites what was there. A progress bar that redraws itself is therefore left as
/// its last state. A shorter redraw leaves the end of the longer text in place, as a terminal does.
/// `\r\n` is an ordinary line break.
pub fn collapse_carriage_returns(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut line: Vec<char> = Vec::new();
    let mut cursor = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                result.extend(line.drain(..));
                result.push('\n');
                cursor = 0;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => cursor = 0,
            other => {
                if cursor < line.len() {
                    line[cursor] = other;
                } else {
                    line.push(other);
                }
                cursor += 1;
            }
        }
    }
    result.extend(line);
    result
}

// ---------------------------------------------------------------------------------------------
// HTML reading
// ---------------------------------------------------------------------------------------------

/// One piece of an HTML string.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// An opening tag, with the name in lower case and everything after the name.
    Open {
        name: String,
        attributes: String,
    },
    Close(String),
    /// Text between tags, with entities not yet decoded.
    Text(String),
}

/// Splits HTML into tags and text. Comments, the doctype, and everything inside `<style>` and
/// `<script>` are dropped. A `<` that does not start a tag is text.
fn tokenize(html: &str) -> Vec<Token> {
    let lower = html.to_ascii_lowercase();
    let mut tokens = Vec::new();
    let mut position = 0;
    while position < html.len() {
        let Some(offset) = html[position..].find('<') else {
            tokens.push(Token::Text(html[position..].to_string()));
            break;
        };
        if offset > 0 {
            tokens.push(Token::Text(html[position..position + offset].to_string()));
        }
        position += offset;
        position = read_markup(html, &lower, position, &mut tokens);
    }
    tokens
}

/// Reads the markup that starts at a `<` and returns where reading continues.
fn read_markup(html: &str, lower: &str, start: usize, tokens: &mut Vec<Token>) -> usize {
    let rest = &html[start..];
    if rest.starts_with("<!--") {
        return rest.find("-->").map_or(html.len(), |end| start + end + 3);
    }
    let second = rest[1..].chars().next();
    if second == Some('!') || second == Some('?') {
        return rest.find('>').map_or(html.len(), |end| start + end + 1);
    }
    if !second.is_some_and(|c| c.is_ascii_alphabetic() || c == '/') {
        tokens.push(Token::Text("<".to_string()));
        return start + 1;
    }
    let Some(close) = tag_end(rest) else {
        tokens.push(Token::Text(rest.to_string()));
        return html.len();
    };
    let end = start + close + 1;
    let inner = html[start + 1..end - 1].trim();
    if let Some(name) = inner.strip_prefix('/') {
        tokens.push(Token::Close(tag_name(name)));
        return end;
    }
    let name = tag_name(inner);
    if name == "style" || name == "script" {
        let closing = format!("</{name}");
        return lower[end..].find(&closing).map_or(html.len(), |found| {
            html[end + found..].find('>').map_or(html.len(), |close| end + found + close + 1)
        });
    }
    tokens.push(Token::Open { attributes: inner[name.len()..].to_string(), name });
    end
}

/// The offset of the `>` that ends a tag, skipping any `>` inside a quoted attribute value.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (index, byte) in tag.bytes().enumerate() {
        match (quote, byte) {
            (Some(open), b) if b == open => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}

/// The tag name at the start of the text, in lower case.
fn tag_name(text: &str) -> String {
    text.chars()
        .take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// The value of an attribute such as `colspan="2"`, or `None` when the tag does not have it.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find(name) {
        let start = from + found;
        let after = lower[start + name.len()..].trim_start();
        let separated = start == 0 || lower.as_bytes()[start - 1].is_ascii_whitespace();
        if separated && after.starts_with('=') {
            let value_start = attributes.len() - after.len() + 1;
            let value = attributes[value_start..].trim_start();
            return Some(match value.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    value[1..].split(quote).next().unwrap_or("").to_string()
                }
                _ => value
                    .split(|c: char| c.is_whitespace() || c == '>')
                    .next()
                    .unwrap_or("")
                    .to_string(),
            });
        }
        from = start + name.len();
    }
    None
}

/// Replaces `&amp; &lt; &gt; &quot; &#39; &apos; &nbsp; &#NNN; &#xHH;` and a few more named
/// entities by the character they stand for. Text that looks like an entity but is not one is left
/// as it is. A non breaking space becomes an ordinary space.
fn decode_entities(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        result.push_str(&rest[..position]);
        rest = &rest[position..];
        let decoded = rest
            .find(';')
            .filter(|end| *end <= 12)
            .and_then(|end| entity_char(&rest[1..end]).map(|c| (c, end)));
        match decoded {
            Some((c, end)) => {
                result.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                result.push('&');
                rest = &rest[1..];
            }
        }
    }
    result.push_str(rest);
    result
}

/// The character an entity name stands for, without the `&` and `;`.
fn entity_char(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code).filter(|c| *c != '\0');
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "times" => '\u{d7}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "middot" => '\u{b7}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        _ => return None,
    })
}

/// Replaces every run of whitespace by one space and trims both ends.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

// ---------------------------------------------------------------------------------------------
// HTML to text
// ---------------------------------------------------------------------------------------------

/// Turns HTML into the plain text a person would read.
///
/// Tags are removed. `<br>` and the end of a paragraph, `div`, list item, table row, heading, list
/// or quote start a new line, a list item starts with `• `, and table cells are separated by a
/// space. Entities are decoded, `<style>` and `<script>` are dropped, whitespace is collapsed
/// except inside `<pre>`, and several blank lines in a row become one.
pub fn html_to_text(html: &str) -> String {
    let mut text = String::new();
    let mut preformatted = 0usize;
    for token in tokenize(html) {
        match token {
            Token::Text(raw) => push_html_text(&mut text, &decode_entities(&raw), preformatted > 0),
            Token::Open { name, .. } => match name.as_str() {
                "br" => text.push('\n'),
                "li" => {
                    start_line(&mut text);
                    text.push_str("\u{2022} ");
                }
                "pre" => preformatted += 1,
                other if is_block(other) => start_line(&mut text),
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "td" | "th" => text.push(' '),
                "pre" => preformatted = preformatted.saturating_sub(1),
                other if is_block(other) => text.push('\n'),
                _ => {}
            },
        }
    }
    tidy_lines(&text)
}

/// Whether a tag puts its content on lines of its own.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "li"
            | "tr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "table"
            | "blockquote"
    )
}

/// Starts a new line unless the text is empty or already at the start of a line.
fn start_line(text: &mut String) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

/// Adds text to the output. Whitespace is collapsed to one space, and none is added at the start of
/// a line, unless the text is preformatted.
fn push_html_text(output: &mut String, text: &str, preformatted: bool) {
    if preformatted {
        output.push_str(text);
        return;
    }
    for c in text.chars() {
        if !c.is_whitespace() {
            output.push(c);
        } else if !output.is_empty() && !output.ends_with([' ', '\n']) {
            output.push(' ');
        }
    }
}

/// Removes spaces at the ends of lines, reduces blank lines to at most one in a row, and removes
/// blank lines at the start and the end.
fn tidy_lines(text: &str) -> String {
    let mut result = String::new();
    let mut blank_run = 0;
    for line in text.lines() {
        let line = line.trim_end();
        blank_run = if line.is_empty() { blank_run + 1 } else { 0 };
        if blank_run <= 1 {
            result.push_str(line);
            result.push('\n');
        }
    }
    result.trim_matches('\n').to_string()
}

// ---------------------------------------------------------------------------------------------
// HTML tables
// ---------------------------------------------------------------------------------------------

/// A table read from HTML.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    /// The header rows. There is usually one, and a pandas frame with a column MultiIndex or a named
    /// index has more.
    pub header: Vec<Vec<String>>,
    /// The body rows. Every row has as many cells as the widest row of the table.
    pub rows: Vec<Vec<String>>,
    /// How many cells at the start of the first body row are `<th>`. pandas writes the index of a
    /// frame that way.
    pub index_columns: usize,
    pub caption: Option<String>,
    /// The first paragraph after the table, such as the "5 rows × 3 columns" that pandas adds when
    /// it has left rows out.
    pub footer: Option<String>,
}

/// One cell as written in the HTML.
struct RawCell {
    text: String,
    header: bool,
    colspan: usize,
    rowspan: usize,
}

/// One row as written in the HTML, and whether it sits inside `<thead>`.
struct RawRow {
    cells: Vec<RawCell>,
    in_head: bool,
}

/// A cell placed in the grid: its text and whether it was a `<th>`.
type Placed = (String, bool);

/// What the reader has seen so far while it walks the tokens of a table.
#[derive(Default)]
struct TableReader {
    rows: Vec<RawRow>,
    row: Option<RawRow>,
    cell: Option<RawCell>,
    in_head: bool,
    caption: Option<String>,
    in_caption: bool,
}

impl TableReader {
    /// Ends the cell being read, if there is one, and adds it to the current row.
    fn finish_cell(&mut self) {
        if let Some(mut cell) = self.cell.take() {
            cell.text = collapse_whitespace(&cell.text);
            self.row
                .get_or_insert_with(|| RawRow { cells: Vec::new(), in_head: self.in_head })
                .cells
                .push(cell);
        }
    }

    /// Ends the row being read, if there is one.
    fn finish_row(&mut self) {
        self.finish_cell();
        if let Some(row) = self.row.take() {
            self.rows.push(row);
        }
    }

    /// Starts a cell. `colspan` and `rowspan` are read, limited to 1000 so that a hostile value
    /// cannot make a huge table.
    fn start_cell(&mut self, header: bool, attributes: &str) {
        self.finish_cell();
        let number = |name: &str| {
            attribute(attributes, name)
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 1000)
        };
        self.cell = Some(RawCell {
            text: String::new(),
            header,
            colspan: number("colspan"),
            rowspan: number("rowspan"),
        });
    }

    /// Adds text to the cell or the caption that is open.
    fn add_text(&mut self, text: &str) {
        if let Some(cell) = &mut self.cell {
            cell.text.push_str(text);
        } else if let (true, Some(caption)) = (self.in_caption, &mut self.caption) {
            caption.push_str(text);
        }
    }

    /// Handles one token of the table's own content.
    fn read(&mut self, token: &Token) {
        match token {
            Token::Open { name, attributes } => match name.as_str() {
                "thead" => self.in_head = true,
                "tr" => {
                    self.finish_row();
                    self.row = Some(RawRow { cells: Vec::new(), in_head: self.in_head });
                }
                "th" | "td" => self.start_cell(name == "th", attributes),
                "caption" => {
                    self.in_caption = true;
                    self.caption = Some(String::new());
                }
                "br" => self.add_text(" "),
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "thead" => {
                    self.finish_row();
                    self.in_head = false;
                }
                "tr" => self.finish_row(),
                "th" | "td" => self.finish_cell(),
                "caption" => self.in_caption = false,
                _ => {}
            },
            Token::Text(raw) => self.add_text(&decode_entities(raw)),
        }
    }
}

/// Reads the first `<table>` of an HTML string.
///
/// It reads `<thead>`, `<tbody>`, `<tr>`, `<th>` and `<td>`, and rows that sit directly under
/// `<table>`. A row that is never closed ends where the next row or cell starts. When there is no
/// `<thead>` but the first row is made only of `<th>` cells, that row is the header.
///
/// **A cell with `colspan` or `rowspan` is repeated** in every grid position it covers, so every row
/// has the same number of cells and a column can be sorted or exported without special cases. A
/// table inside a table cell is ignored. Entities are decoded, whitespace is collapsed, and the
/// contents of `<style>` and `<script>` are dropped.
///
/// It returns `None` when the HTML has no `<table>` or the table has no cells.
pub fn parse_table(html: &str) -> Option<Table> {
    let tokens = tokenize(html);
    let start = tokens
        .iter()
        .position(|token| matches!(token, Token::Open { name, .. } if name == "table"))?;
    let mut reader = TableReader::default();
    let mut depth = 1;
    let mut end = tokens.len();
    for (index, token) in tokens.iter().enumerate().skip(start + 1) {
        match token {
            Token::Open { name, .. } if name == "table" => depth += 1,
            Token::Close(name) if name == "table" => {
                depth -= 1;
                if depth == 0 {
                    end = index + 1;
                    break;
                }
            }
            _ if depth == 1 => reader.read(token),
            _ => {}
        }
    }
    reader.finish_row();
    let footer = footer_after(&tokens[end..]);
    build_table(reader, footer)
}

/// The text of the first paragraph in the tokens that follow a table.
fn footer_after(tokens: &[Token]) -> Option<String> {
    let start =
        tokens.iter().position(|token| matches!(token, Token::Open { name, .. } if name == "p"))?;
    let mut text = String::new();
    for token in &tokens[start + 1..] {
        match token {
            Token::Text(raw) => text.push_str(&decode_entities(raw)),
            Token::Close(name) if name == "p" => break,
            _ => {}
        }
    }
    Some(collapse_whitespace(&text)).filter(|footer| !footer.is_empty())
}

/// Lays the rows out in a grid and splits it into header and body.
fn build_table(reader: TableReader, footer: Option<String>) -> Option<Table> {
    let (mut head, mut body): (Vec<RawRow>, Vec<RawRow>) =
        reader.rows.into_iter().filter(|row| !row.cells.is_empty()).partition(|row| row.in_head);
    if head.is_empty() && body.len() > 1 && body[0].cells.iter().all(|cell| cell.header) {
        head.push(body.remove(0));
    }
    let header_grid = lay_out(&head);
    let body_grid = lay_out(&body);
    if header_grid.is_empty() && body_grid.is_empty() {
        return None;
    }
    let width = header_grid.iter().chain(&body_grid).map(Vec::len).max().unwrap_or(0);
    let index_columns =
        body_grid.first().map_or(0, |row| row.iter().take_while(|(_, header)| *header).count());
    let pad = |grid: Vec<Vec<Placed>>| -> Vec<Vec<String>> {
        grid.into_iter()
            .map(|row| {
                let mut cells: Vec<String> = row.into_iter().map(|(text, _)| text).collect();
                cells.resize(width, String::new());
                cells
            })
            .collect()
    };
    Some(Table {
        header: pad(header_grid),
        rows: pad(body_grid),
        index_columns,
        caption: reader.caption.map(|c| collapse_whitespace(&c)).filter(|c| !c.is_empty()),
        footer,
    })
}

/// Places the cells of some rows on a grid, carrying a cell with a `rowspan` down into the rows
/// below it and repeating a cell with a `colspan` along its row.
fn lay_out(rows: &[RawRow]) -> Vec<Vec<Placed>> {
    let mut carried: Vec<Option<(usize, Placed)>> = Vec::new();
    let mut grid = Vec::new();
    for row in rows {
        let mut line: Vec<Placed> = Vec::new();
        for cell in &row.cells {
            take_carried(&mut line, &mut carried);
            for _ in 0..cell.colspan {
                let column = line.len();
                line.push((cell.text.clone(), cell.header));
                if cell.rowspan > 1 {
                    if carried.len() <= column {
                        carried.resize(column + 1, None);
                    }
                    carried[column] = Some((cell.rowspan - 1, (cell.text.clone(), cell.header)));
                }
            }
        }
        take_carried(&mut line, &mut carried);
        grid.push(line);
    }
    grid
}

/// Adds the cells that earlier rows carry into the next positions of a row, while there are any.
fn take_carried(line: &mut Vec<Placed>, carried: &mut [Option<(usize, Placed)>]) {
    while let Some(slot) = carried.get_mut(line.len()) {
        let Some((remaining, placed)) = slot.take() else { break };
        line.push(placed.clone());
        if remaining > 1 {
            *slot = Some((remaining - 1, placed));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Sorting
// ---------------------------------------------------------------------------------------------

/// What a cell is worth when a column is sorted. Declaration order is the sort order: numbers come
/// before text, and empty cells are put last separately.
#[derive(Debug, Clone, PartialEq)]
enum SortKey {
    Number(f64),
    Text(String),
}

/// The sort key of a cell, or `None` for an empty cell or `NaN`, `nan`, `None` or `null`.
fn sort_key(cell: &str) -> Option<SortKey> {
    let trimmed = cell.trim();
    if matches!(trimmed.to_ascii_lowercase().as_str(), "" | "nan" | "none" | "null") {
        return None;
    }
    let digits: String = trimmed.trim_end_matches('%').chars().filter(|c| *c != ',').collect();
    match digits.trim().parse::<f64>() {
        Ok(number) if number.is_finite() => Some(SortKey::Number(number)),
        _ => Some(SortKey::Text(trimmed.to_lowercase())),
    }
}

/// Orders two sort keys. Numbers compare as numbers and text compares as lower case text. A number
/// sorts before text, so the order is the same whichever two cells are compared first.
fn compare_keys(a: &SortKey, b: &SortKey) -> std::cmp::Ordering {
    match (a, b) {
        (SortKey::Number(x), SortKey::Number(y)) => x.total_cmp(y),
        (SortKey::Text(x), SortKey::Text(y)) => x.cmp(y),
        (SortKey::Number(_), SortKey::Text(_)) => std::cmp::Ordering::Less,
        (SortKey::Text(_), SortKey::Number(_)) => std::cmp::Ordering::Greater,
    }
}

/// The order to show the rows of a table in when it is sorted by one column, as indexes into
/// `table.rows`. Cells that both read as numbers compare as numbers, and a number may have
/// thousands separators and a trailing `%`. Other cells compare as lower case text. Rows with an
/// empty cell or `NaN` in the column come last in both directions, and rows that compare equal keep
/// their order. A column past the end of a short row counts as empty.
pub fn sort_rows(table: &Table, column: usize, descending: bool) -> Vec<usize> {
    let keys: Vec<Option<SortKey>> =
        table.rows.iter().map(|row| row.get(column).and_then(|cell| sort_key(cell))).collect();
    let mut order: Vec<usize> = (0..table.rows.len()).collect();
    order.sort_by(|&a, &b| match (&keys[a], &keys[b]) {
        (Some(x), Some(y)) if descending => compare_keys(y, x),
        (Some(x), Some(y)) => compare_keys(x, y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbformat::parse;
    use serde_json::json;

    const SIMPLE: &str = include_str!("../tests/fixtures/html/simple.html");
    const NAMED_INDEX: &str = include_str!("../tests/fixtures/html/named_index.html");
    const MULTI_INDEX: &str = include_str!("../tests/fixtures/html/multiindex.html");
    const NAN_LONG: &str = include_str!("../tests/fixtures/html/nan_long.html");
    const TRUNCATED: &str = include_str!("../tests/fixtures/html/truncated.html");
    const ALL_OUTPUTS: &str = include_str!("../tests/fixtures/all_outputs.ipynb");

    fn strings(row: &[&str]) -> Vec<String> {
        row.iter().map(|cell| cell.to_string()).collect()
    }

    fn table_of(rows: &[&[&str]]) -> Table {
        Table { rows: rows.iter().map(|row| strings(row)).collect(), ..Table::default() }
    }

    #[test]
    fn a_simple_pandas_frame_is_read_with_its_index_column() {
        let table = parse_table(SIMPLE).unwrap();
        assert_eq!(table.header, vec![strings(&["", "a", "b", "c"])]);
        assert_eq!(table.rows[0], strings(&["0", "1", "x", "1.50"]));
        assert_eq!((table.rows.len(), table.index_columns), (3, 1));
        assert_eq!((table.caption, table.footer), (None, None));
    }

    #[test]
    fn a_named_index_gives_a_second_header_row_and_decodes_entities() {
        let table = parse_table(NAMED_INDEX).unwrap();
        assert_eq!(table.header, vec![strings(&["", "price", "name"]), strings(&["id", "", ""])]);
        assert_eq!(table.rows[0], strings(&["r1", "1200", "Apple & Co"]));
        assert_eq!(table.rows[1][2], "<b>");
        assert_eq!(table.rows[2][2], "caf\u{e9}");
    }

    #[test]
    fn a_multiindex_frame_repeats_spanned_cells_so_every_row_is_full() {
        let table = parse_table(MULTI_INDEX).unwrap();
        assert_eq!(table.header[0], strings(&["", "", "x", "x"]));
        assert_eq!(table.header[1], strings(&["", "", "p", "q"]));
        assert_eq!(table.header[2], strings(&["grp", "n", "", ""]));
        assert_eq!(table.rows[0], strings(&["A", "1", "0", "1"]));
        assert_eq!(table.rows[1], strings(&["A", "2", "2", "3"]));
        assert_eq!(table.rows[3], strings(&["B", "2", "6", "7"]));
        assert_eq!(table.index_columns, 2);
    }

    #[test]
    fn nan_and_long_strings_are_kept_as_text() {
        let table = parse_table(NAN_LONG).unwrap();
        assert_eq!(table.rows[1], strings(&["1", "NaN", "NaN"]));
        assert!(table.rows[0][2].starts_with("a very long string"));
    }

    #[test]
    fn a_truncated_frame_keeps_its_dots_and_reads_the_footer() {
        let table = parse_table(TRUNCATED).unwrap();
        assert_eq!(table.header[0], strings(&["", "0", "1", "2", "...", "7", "8", "9"]));
        assert_eq!(table.rows.len(), 11);
        assert_eq!(
            table.rows[5],
            strings(&["...", "...", "...", "...", "...", "...", "...", "..."])
        );
        assert_eq!(table.rows[10][0], "39");
        assert_eq!(table.footer.as_deref(), Some("40 rows \u{d7} 10 columns"));
    }

    #[test]
    fn style_and_script_text_never_reaches_a_cell_and_rows_may_sit_under_table() {
        let html = "<table><caption> Sales </caption><tr><th>a</th><th>b</th></tr><tr><td>1<style>x{}</style></td><td>2<script>y()</script></td></table>";
        let table = parse_table(html).unwrap();
        assert_eq!(table.header, vec![strings(&["a", "b"])]);
        assert_eq!(table.rows, vec![strings(&["1", "2"])]);
        assert_eq!(table.caption.as_deref(), Some("Sales"));
    }

    #[test]
    fn colspan_repeats_the_text_and_short_rows_are_padded() {
        let table = parse_table(
            "<table><tr><td colspan=2>wide</td><td>c</td></tr><tr><td>1</td></tr></table>",
        )
        .unwrap();
        assert_eq!(table.rows, vec![strings(&["wide", "wide", "c"]), strings(&["1", "", ""])]);
    }

    #[test]
    fn html_without_a_table_gives_none() {
        assert_eq!(parse_table("<p>hello</p>"), None);
        assert_eq!(parse_table("<table></table>"), None);
    }

    #[test]
    fn entities_of_every_kind_are_decoded() {
        assert_eq!(
            decode_entities(
                "a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#39; &#x41;&#66; x&nbsp;y &bogus; & &"
            ),
            "a & b <c> \"d\" 'e' AB x y &bogus; & &"
        );
    }

    #[test]
    fn sorting_compares_numbers_as_numbers_and_puts_empty_cells_last() {
        let table = table_of(&[&["10"], &["9"], &["1,200"], &[""], &["NaN"], &["5%"]]);
        assert_eq!(sort_rows(&table, 0, false), vec![5, 1, 0, 2, 3, 4]);
        assert_eq!(sort_rows(&table, 0, true), vec![2, 0, 1, 5, 3, 4]);
    }

    #[test]
    fn sorting_text_ignores_case_and_a_missing_column_counts_as_empty() {
        let table = table_of(&[&["banana"], &["Apple"], &["cherry"]]);
        assert_eq!(sort_rows(&table, 0, false), vec![1, 0, 2]);
        assert_eq!(sort_rows(&table, 3, false), vec![0, 1, 2]);
    }

    #[test]
    fn sorting_a_mixed_column_puts_numbers_before_text() {
        let table = table_of(&[&["b"], &["2"], &["a"], &["1"]]);
        assert_eq!(sort_rows(&table, 0, false), vec![3, 1, 2, 0]);
    }

    #[test]
    fn carriage_returns_overwrite_the_line_like_a_terminal() {
        assert_eq!(collapse_carriage_returns("10%\r50%\r100%\ndone\n"), "100%\ndone\n");
        assert_eq!(collapse_carriage_returns("abcdef\rxy"), "xycdef");
        assert_eq!(collapse_carriage_returns("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(collapse_carriage_returns("loading\r"), "loading");
    }

    #[test]
    fn html_becomes_text_with_lines_bullets_and_decoded_entities() {
        let html = "<style>p{}</style><h1>Title</h1><p>One &amp; two<br>three</p><ul><li>a</li><li>b</li></ul><script>x()</script>";
        assert_eq!(html_to_text(html), "Title\nOne & two\nthree\n\u{2022} a\n\u{2022} b");
    }

    #[test]
    fn html_to_text_collapses_runs_of_blank_lines_and_keeps_preformatted_text() {
        assert_eq!(html_to_text("<div><div>a</div></div><br><br><br><br><div>b</div>"), "a\n\nb");
        assert_eq!(html_to_text("<pre>x  y\n  z</pre>"), "x  y\n  z");
    }

    #[test]
    fn ansi_colours_bold_and_reset_split_the_text_into_spans() {
        let spans = ansi_spans("a\u{1b}[1;31mred\u{1b}[0m\u{1b}[42mon\u{1b}[49m b");
        assert_eq!(spans.len(), 4);
        assert_eq!((spans[0].text.as_str(), spans[0].colour, spans[0].bold), ("a", None, false));
        assert_eq!(
            (spans[1].text.as_str(), spans[1].colour, spans[1].bold),
            ("red", Some(Ansi::Red), true)
        );
        assert_eq!((spans[2].text.as_str(), spans[2].background), ("on", Some(Ansi::Green)));
        assert_eq!((spans[3].text.as_str(), spans[3].background), (" b", None));
    }

    #[test]
    fn ansi_extended_colours_are_read_and_other_sequences_are_dropped() {
        let spans = ansi_spans("\u{1b}[38;5;241mq\u{1b}[38;2;1;2;3;100mr\u{1b}[2K\u{1b}[?25h\u{1b}]0;title\u{7}\u{1b}]8;;x\u{1b}\\s\u{1b}(Bt");
        assert_eq!(spans[0].colour, Some(Ansi::Indexed(241)));
        assert_eq!(
            (spans[1].colour, spans[1].background),
            (Some(Ansi::Rgb(1, 2, 3)), Some(Ansi::BrightBlack))
        );
        assert_eq!(strip_ansi("\u{1b}[2K\u{1b}[?25hx\u{1b}]0;title\u{7}y\u{1b}[1;31mz"), "xyz");
        assert_eq!(spans.iter().map(|span| span.text.as_str()).collect::<String>(), "qrst");
        assert!(ansi_spans("").is_empty() && ansi_spans("\u{1b}[31m").is_empty());
    }

    #[test]
    fn the_ipython_traceback_in_the_fixture_is_split_into_coloured_spans() {
        let notebook = parse(ALL_OUTPUTS).unwrap();
        let Shown::Error { ename, evalue, traceback } = shown(&notebook.cells[8].outputs[0]) else {
            panic!("not an error")
        };
        assert_eq!((ename.as_str(), evalue.as_str()), ("ZeroDivisionError", "division by zero"));
        assert_eq!(traceback.len(), 4);
        assert_eq!(traceback[1][0].colour, Some(Ansi::Red));
        assert_eq!(traceback[1][0].text, "ZeroDivisionError");
        let plain: String = traceback[3].iter().map(|span| span.text.as_str()).collect();
        assert_eq!(plain, "ZeroDivisionError: division by zero");
        assert!(traceback.iter().flatten().all(|span| !span.text.contains('\u{1b}')));
    }

    #[test]
    fn base64_round_trips_and_ignores_line_breaks() {
        for length in 0..20u8 {
            let bytes: Vec<u8> =
                (0..length).map(|n| n.wrapping_mul(37).wrapping_add(200)).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_decode("TW\nFu\r\n ").unwrap(), b"Man");
        assert_eq!(base64_decode("T*Fu"), None);
    }

    #[test]
    fn a_bundle_is_drawn_in_the_order_of_preference() {
        let png = "iVBORw0KGgo=";
        let bundle = |value| Output::display_data(value, json!({}));
        assert!(
            matches!(shown(&bundle(json!({"text/plain": "p", "text/markdown": "# m"}))), Shown::Markdown(m) if m == "# m")
        );
        assert!(
            matches!(shown(&bundle(json!({"text/plain": "p", "image/png": png}))), Shown::Png(bytes) if bytes.starts_with(&[0x89, b'P', b'N', b'G']))
        );
        assert!(matches!(
            shown(&bundle(json!({"text/plain": "p", "image/svg+xml": "<svg/>"}))),
            Shown::Svg(_)
        ));
        assert!(matches!(
            shown(&bundle(json!({"text/plain": "p", "text/latex": "$x$"}))),
            Shown::Latex(_)
        ));
        assert!(
            matches!(shown(&bundle(json!({"text/plain": "p", "application/json": {"a": 1}}))), Shown::Json(text) if text.contains("\"a\": 1"))
        );
        assert!(matches!(shown(&bundle(json!({"text/plain": "p"}))), Shown::Text(t) if t == "p"));
        assert!(matches!(shown(&bundle(json!({}))), Shown::Text(t) if t.is_empty()));
    }

    #[test]
    fn html_is_a_table_when_it_has_one_and_text_otherwise_and_empty_html_falls_through() {
        let bundle = |value| Output::display_data(value, json!({}));
        assert!(matches!(
            shown(&bundle(json!({"text/html": SIMPLE, "text/plain": "p"}))),
            Shown::Table(_)
        ));
        assert!(
            matches!(shown(&bundle(json!({"text/html": "<b>hi</b>", "text/plain": "p"}))), Shown::Html(t) if t == "hi")
        );
        assert!(
            matches!(shown(&bundle(json!({"text/html": "<script>x()</script>", "text/plain": "fallback"}))), Shown::Text(t) if t == "fallback")
        );
    }

    #[test]
    fn streams_collapse_progress_lines_and_remember_which_stream_they_are() {
        let output = Output::stream("stderr", "10%\r100%\n");
        assert_eq!(shown(&output), Shown::Stream { stderr: true, text: "100%\n".to_string() });
        assert_eq!(
            shown(&Output::stream("stdout", "x")),
            Shown::Stream { stderr: false, text: "x".to_string() }
        );
    }
}
