//! Reading HTML from the clipboard into a grid of cells: the tables in it, with row and column spans, and
//! the bold, italic, colours, alignment and number formats the cells ask for.

use crate::styles::Color;

/// What the HTML asks of one cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HtmlCell {
    /// Lines of text; more than one when the cell holds line breaks or blocks.
    pub lines: Vec<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<Color>,
    pub fill: Option<Color>,
    /// `left`, `center` or `right`.
    pub align: Option<&'static str>,
    pub link: Option<String>,
    /// A number the page says the text stands for (`x:num`).
    pub number: Option<f64>,
    /// A number format the page asks for (`mso-number-format`).
    pub format: Option<String>,
    /// The text is text whatever it looks like (`x:str` or the format `@`).
    pub as_text: bool,
    pub row_span: u32,
    pub col_span: u32,
    /// Keep the lines in one cell (`mso-data-placement:same-cell`).
    pub same_cell: bool,
}

/// The cells of the tables in a piece of HTML, one after the other, rows of cells.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HtmlGrid {
    pub rows: Vec<Vec<HtmlCell>>,
}

/// A piece of the page: a tag with its attributes, the end of one, or text.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

/// Parse the HTML into the cells of its tables. Text outside any table becomes cells of one column.
pub fn parse(html: &str) -> HtmlGrid {
    let body = fragment_of(html);
    let tokens = tokenize(body);
    Builder::default().run(&tokens)
}

/// The part of a clipboard HTML document that was copied: between the fragment markers when it has them.
fn fragment_of(html: &str) -> &str {
    let start = html.find("<!--StartFragment-->").map(|i| i + "<!--StartFragment-->".len());
    let end = html.find("<!--EndFragment-->");
    match (start, end) {
        (Some(s), Some(e)) if s <= e => &html[s..e],
        _ => match html.find('<') {
            Some(first) if html.starts_with("Version:") => &html[first..],
            _ => html,
        },
    }
}

fn tokenize(html: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        match rest.find('<') {
            None => {
                out.push(Token::Text(decode(rest)));
                break;
            }
            Some(0) => {
                let (token, tail) = read_tag(rest);
                out.extend(token);
                rest = tail;
            }
            Some(at) => {
                out.push(Token::Text(decode(&rest[..at])));
                rest = &rest[at..];
            }
        }
    }
    out
}

/// One tag (or comment) at the start of the text, and what follows it.
fn read_tag(text: &str) -> (Option<Token>, &str) {
    if let Some(rest) = text.strip_prefix("<!--") {
        let end = rest.find("-->").map_or(rest.len(), |i| i + 3);
        return (None, &rest[end..]);
    }
    let Some(close) = text.find('>') else { return (Some(Token::Text(decode(text))), "") };
    let inside = &text[1..close];
    let tail = &text[close + 1..];
    if inside.starts_with('!') || inside.starts_with('?') {
        return (None, tail);
    }
    if let Some(name) = inside.strip_prefix('/') {
        return (Some(Token::Close(name.trim().to_ascii_lowercase())), tail);
    }
    let (name, attrs) = split_tag(inside.trim_end_matches('/'));
    // The text of a script or a style sheet is not for the cells.
    if name == "script" || name == "style" {
        let end = tail.to_ascii_lowercase().find(&format!("</{name}")).unwrap_or(tail.len());
        let after = tail[end..].find('>').map_or(tail.len(), |i| end + i + 1);
        return (None, &tail[after..]);
    }
    (Some(Token::Open(name, attrs)), tail)
}

/// A tag's name in lower case and its attributes, names in lower case and values with the quotes off.
fn split_tag(inside: &str) -> (String, Vec<(String, String)>) {
    let inside = inside.trim();
    let name_end = inside.find(char::is_whitespace).unwrap_or(inside.len());
    let name = inside[..name_end].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut rest = inside[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (v, tail) = read_value(after);
            value = decode(&v);
            rest = tail.trim_start();
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    (name, attrs)
}

fn read_value(text: &str) -> (String, &str) {
    match text.chars().next() {
        Some(q @ ('"' | '\'')) => {
            let body = &text[1..];
            match body.find(q) {
                Some(end) => (body[..end].to_owned(), &body[end + 1..]),
                None => (body.to_owned(), ""),
            }
        }
        _ => {
            let end = text.find(char::is_whitespace).unwrap_or(text.len());
            (text[..end].to_owned(), &text[end..])
        }
    }
}

/// Text with character references turned into the characters.
pub fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        match rest.find(';').filter(|e| *e <= 10) {
            Some(end) => match reference(&rest[1..end]) {
                Some(c) => {
                    out.push(c);
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            },
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn reference(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "trade" => '\u{2122}',
        "euro" => '\u{20ac}',
        "pound" => '\u{a3}',
        "yen" => '\u{a5}',
        "cent" => '\u{a2}',
        "deg" => '\u{b0}',
        "plusmn" => '\u{b1}',
        "times" => '\u{d7}',
        "divide" => '\u{f7}',
        "middot" => '\u{b7}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "bull" => '\u{2022}',
        "sect" => '\u{a7}',
        "para" => '\u{b6}',
        "frac12" => '\u{bd}',
        "frac14" => '\u{bc}',
        "frac34" => '\u{be}',
        _ => return None,
    })
}

/// The decorations in force while reading: set by tags round the text.
#[derive(Debug, Clone, Default)]
struct Look {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Color>,
    link: Option<String>,
}

/// Reading the tokens into a grid.
#[derive(Default)]
struct Builder {
    grid: HtmlGrid,
    row: Option<Vec<HtmlCell>>,
    cell: Option<HtmlCell>,
    /// Decorations to apply to the text read so far in the cell, and how much text there has been.
    looks: Vec<(String, Look)>,
    seen: Vec<Seen>,
    depth: usize,
    block_gap: bool,
}

impl Builder {
    fn run(mut self, tokens: &[Token]) -> HtmlGrid {
        for token in tokens {
            match token {
                Token::Open(name, attrs) => self.open(name, attrs),
                Token::Close(name) => self.close(name),
                Token::Text(text) => self.text(text),
            }
        }
        self.end_cell();
        self.end_row();
        while self.grid.rows.last().is_some_and(|r| r.len() == 1 && r[0].lines.iter().all(String::is_empty)) {
            self.grid.rows.pop();
        }
        self.grid
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)]) {
        match name {
            "table" => {
                self.depth += 1;
            }
            "tr" if self.depth == 1 => {
                self.end_cell();
                self.end_row();
                self.row = Some(Vec::new());
            }
            "td" | "th" if self.depth == 1 => {
                self.end_cell();
                if self.row.is_none() {
                    self.row = Some(Vec::new());
                }
                self.start_cell(name == "th", attrs);
            }
            "br" => self.line_break(),
            "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.block_start(name, attrs);
            }
            _ => self.inline_open(name, attrs),
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "table" => {
                self.end_cell();
                self.end_row();
                self.depth = self.depth.saturating_sub(1);
            }
            "tr" if self.depth == 1 => {
                self.end_cell();
                self.end_row();
            }
            "td" | "th" if self.depth == 1 => self.end_cell(),
            "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.block_end(name),
            _ => self.inline_close(name),
        }
    }

    fn start_cell(&mut self, header: bool, attrs: &[(String, String)]) {
        let mut cell = HtmlCell { row_span: 1, col_span: 1, lines: vec![String::new()], ..Default::default() };
        cell.row_span = span(attrs, "rowspan");
        cell.col_span = span(attrs, "colspan");
        if header {
            cell.bold = true;
            cell.align = Some("center");
        }
        apply_attributes(&mut cell, attrs);
        self.cell = Some(cell);
        self.looks.clear();
        self.seen.clear();
    }

    fn end_cell(&mut self) {
        if let (Some(mut cell), Some(row)) = (self.cell.take(), self.row.as_mut()) {
            apply_common(&mut cell, &self.seen);
            while cell.lines.len() > 1 && cell.lines.last().is_some_and(|l| l.trim().is_empty()) {
                cell.lines.pop();
            }
            for line in &mut cell.lines {
                *line = tidy(line);
            }
            row.push(cell);
        }
        self.looks.clear();
    }

    fn end_row(&mut self) {
        if let Some(row) = self.row.take() {
            self.grid.rows.push(row);
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(cell) = self.cell.as_mut() {
            let look = self.looks.iter().fold(Look::default(), |acc, (_, l)| merge(acc, l));
            note_text(cell, &mut self.seen, text, &look);
            cell.lines.last_mut().unwrap().push_str(text);
        } else if self.depth == 0 && !text.trim().is_empty() {
            if self.block_gap {
                self.grid.rows.push(vec![HtmlCell { row_span: 1, col_span: 1, lines: vec![String::new()], ..Default::default() }]);
                self.block_gap = false;
            }
            let mut cell = HtmlCell { row_span: 1, col_span: 1, lines: vec![tidy(text)], ..Default::default() };
            let look = self.looks.iter().fold(Look::default(), |acc, (_, l)| merge(acc, l));
            (cell.bold, cell.italic, cell.underline, cell.strike) = (look.bold, look.italic, look.underline, look.strike);
            cell.color = look.color;
            self.grid.rows.push(vec![cell]);
        }
    }

    fn line_break(&mut self) {
        if let Some(cell) = self.cell.as_mut() {
            cell.lines.push(String::new());
        }
    }

    fn block_start(&mut self, name: &str, attrs: &[(String, String)]) {
        if self.cell.is_some() {
            let last_empty = self.cell.as_ref().is_some_and(|c| c.lines.last().is_some_and(|l| l.trim().is_empty()));
            if !last_empty {
                self.line_break();
            }
        }
        self.inline_open(name, attrs);
    }

    fn block_end(&mut self, name: &str) {
        self.inline_close(name);
        if self.cell.is_some() {
            self.line_break();
        } else if name == "p" && self.depth == 0 {
            self.block_gap = true;
        }
    }

    fn inline_open(&mut self, name: &str, attrs: &[(String, String)]) {
        let mut look = Look::default();
        match name {
            "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => look.bold = true,
            "i" | "em" | "cite" | "var" => look.italic = true,
            "u" | "ins" => look.underline = true,
            "s" | "strike" | "del" => look.strike = true,
            "a" => {
                look.link = attr(attrs, "href").map(str::to_owned);
                look.underline = look.link.is_some();
            }
            "font" => look.color = attr(attrs, "color").and_then(parse_color),
            _ => {}
        }
        let css = css_of(attrs);
        for (k, v) in &css {
            match k.as_str() {
                "font-weight" => look.bold = matches!(v.as_str(), "bold" | "bolder" | "600" | "700" | "800" | "900"),
                "font-style" => look.italic = v == "italic" || v == "oblique",
                "text-decoration" | "text-decoration-line" => {
                    look.underline = look.underline || v.contains("underline");
                    look.strike = v.contains("line-through");
                }
                "color" => look.color = parse_color(v),
                _ => {}
            }
        }
        if !matches!(name, "br" | "img" | "hr" | "input" | "meta" | "link") {
            self.looks.push((name.to_owned(), look));
        }
    }

    fn inline_close(&mut self, name: &str) {
        if let Some(at) = self.looks.iter().rposition(|(n, _)| n == name) {
            self.looks.truncate(at);
        }
    }
}

/// A span attribute, at least 1.
fn span(attrs: &[(String, String)], name: &str) -> u32 {
    attr(attrs, name).and_then(|v| v.trim().parse::<u32>().ok()).map_or(1, |n| n.clamp(1, 1000))
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

/// A style attribute as property and value pairs.
fn css_of(attrs: &[(String, String)]) -> Vec<(String, String)> {
    let Some(style) = attr(attrs, "style") else { return Vec::new() };
    style
        .split(';')
        .filter_map(|decl| decl.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().trim_end_matches("!important").trim().to_ascii_lowercase()))
        .collect()
}

/// The decorations a cell's own attributes ask for.
fn apply_attributes(cell: &mut HtmlCell, attrs: &[(String, String)]) {
    cell.fill = attr(attrs, "bgcolor").and_then(parse_color);
    cell.align = attr(attrs, "align").and_then(align_of).or(cell.align);
    if let Some(n) = attr(attrs, "x:num") {
        cell.number = n.trim().parse().ok().or(Some(f64::NAN)).filter(|v| !v.is_nan());
    }
    if attr(attrs, "x:str").is_some() {
        cell.as_text = true;
    }
    for (k, v) in css_of(attrs) {
        match k.as_str() {
            "background" | "background-color" => cell.fill = parse_color(&v).or(cell.fill),
            "text-align" => cell.align = align_of(&v).or(cell.align),
            "color" => cell.color = parse_color(&v),
            "font-weight" => cell.bold = cell.bold || matches!(v.as_str(), "bold" | "600" | "700" | "800" | "900"),
            "font-style" => cell.italic = v == "italic",
            "mso-number-format" => {
                cell.as_text = cell.as_text || v.trim_matches(['\'', '"']).replace('\\', "") == "@";
                cell.format = number_format_of(&v);
            }
            "mso-data-placement" => cell.same_cell = v == "same-cell",
            _ => {}
        }
    }
}

fn align_of(value: &str) -> Option<&'static str> {
    match value.trim() {
        "left" => Some("left"),
        "center" | "middle" => Some("center"),
        "right" => Some("right"),
        _ => None,
    }
}

/// A number format code from `mso-number-format`: quotes and backslashes off, and Excel's named formats.
fn number_format_of(value: &str) -> Option<String> {
    let bare = value.trim().trim_matches(['\'', '"']).replace('\\', "");
    Some(match bare.to_ascii_lowercase().as_str() {
        "" | "general" | "@" => return None,
        "standard" => "#,##0.00".to_owned(),
        "fixed" => "0.00".to_owned(),
        "percent" => "0.00%".to_owned(),
        "scientific" => "0.00E+00".to_owned(),
        "short date" => "m/d/yyyy".to_owned(),
        "medium date" => "d-mmm-yy".to_owned(),
        "long date" => "dddd, mmmm d, yyyy".to_owned(),
        _ => bare,
    })
}

/// Combine decorations: the inner one adds to the outer.
fn merge(outer: Look, inner: &Look) -> Look {
    Look {
        bold: outer.bold || inner.bold,
        italic: outer.italic || inner.italic,
        underline: outer.underline || inner.underline,
        strike: outer.strike || inner.strike,
        color: inner.color.or(outer.color),
        link: inner.link.clone().or(outer.link),
    }
}

/// Keep track of which decorations each piece of text in a cell has. The cell is given those that all have,
/// once it is read, because the formats a cell holds are for the whole of its text.
fn note_text(cell: &mut HtmlCell, seen: &mut Vec<Seen>, text: &str, look: &Look) {
    if text.trim().is_empty() {
        return;
    }
    seen.push(Seen { look: look.clone() });
    if cell.link.is_none() {
        cell.link = look.link.clone();
    }
}

/// The decorations of one piece of text in a cell.
struct Seen {
    look: Look,
}

/// Give a cell the decorations that all its text has.
fn apply_common(cell: &mut HtmlCell, seen: &[Seen]) {
    if seen.is_empty() {
        return;
    }
    let all = |f: fn(&Look) -> bool| seen.iter().all(|s| f(&s.look));
    cell.bold = cell.bold || all(|l| l.bold);
    cell.italic = cell.italic || all(|l| l.italic);
    cell.underline = cell.underline || all(|l| l.underline);
    cell.strike = cell.strike || all(|l| l.strike);
    if all(|l| l.color.is_some()) {
        cell.color = seen[0].look.color.or(cell.color);
    }
}

/// Text with white space folded and trimmed, the way a browser shows it. A non breaking space stays.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        if c.is_ascii_whitespace() {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out.trim_matches('\u{a0}').to_owned()
}

/// A colour written as `#rgb`, `#rrggbb`, `rgb(r, g, b)` or one of the common names.
pub fn parse_color(text: &str) -> Option<Color> {
    let t = text.trim().to_ascii_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let v = u32::from_str_radix(hex, 16).ok()?;
                let (r, g, b) = ((v >> 8) & 15, (v >> 4) & 15, v & 15);
                Some(Color::rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
            }
            6 => Color::parse_hex(hex),
            _ => None,
        };
    }
    if let Some(args) = t.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<u8> = args.split(',').filter_map(|p| p.trim().parse::<f64>().ok()).map(|v| v.clamp(0.0, 255.0) as u8).collect();
        return (parts.len() == 3).then(|| Color::rgb(parts[0], parts[1], parts[2]));
    }
    let named = match t.as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "lime" => 0x00ff00,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "gray" | "grey" => 0x808080,
        "silver" => 0xc0c0c0,
        "maroon" => 0x800000,
        "navy" => 0x000080,
        "teal" => 0x008080,
        "aqua" | "cyan" => 0x00ffff,
        "fuchsia" | "magenta" => 0xff00ff,
        "olive" => 0x808000,
        _ => return None,
    };
    Some(Color::rgb((named >> 16) as u8, (named >> 8) as u8, named as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_becomes_rows_of_cells_with_their_spans() {
        let grid = parse("<table><tr><td colspan=\"2\">wide</td><td>x</td></tr><tr><td rowspan=2>tall</td><td>b</td></tr></table>");
        assert_eq!(grid.rows.len(), 2);
        assert_eq!(grid.rows[0][0].col_span, 2);
        assert_eq!(grid.rows[1][0].row_span, 2);
        assert_eq!(grid.rows[0][2].lines, ["x"]);
    }

    #[test]
    fn bold_italic_and_colour_come_from_tags_and_styles() {
        let grid = parse("<table><tr><td><b>a</b></td><td><i>b</i></td><td style=\"color:#ff0000\">c</td><td bgcolor=\"#00ff00\">d</td></tr></table>");
        let row = &grid.rows[0];
        assert!(row[0].bold && row[1].italic);
        assert_eq!(row[2].color, Some(Color::rgb(255, 0, 0)));
        assert_eq!(row[3].fill, Some(Color::rgb(0, 255, 0)));
    }

    #[test]
    fn entities_are_decoded_and_scripts_are_left_out() {
        let grid = parse("<table><tr><td>a &amp; b &lt;c&gt; &#65;</td><td><script>x()</script>y</td></tr></table>");
        assert_eq!(grid.rows[0][0].lines, ["a & b <c> A"]);
        assert_eq!(grid.rows[0][1].lines, ["y"]);
    }

    #[test]
    fn the_fragment_markers_pick_the_copied_part() {
        let doc = "Version:0.9\r\nStartHTML:0\r\n<html><body><!--StartFragment--><table><tr><td>k</td></tr></table><!--EndFragment--></body></html>";
        assert_eq!(parse(doc).rows[0][0].lines, ["k"]);
    }

    #[test]
    fn a_line_break_in_a_cell_makes_a_second_line() {
        let grid = parse("<table><tr><td>l1<br>l2</td></tr></table>");
        assert_eq!(grid.rows[0][0].lines, ["l1", "l2"]);
    }
}
