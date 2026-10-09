//! Writing a notebook as something else, and reading a Python file as a notebook.
//!
//! - [`to_python`] and [`from_python`] use the percent format (`# %%` lines between cells) that
//!   jupytext, VS Code and Spyder also read and write. It holds the cells and their kinds and
//!   nothing else: no outputs, no ids, no metadata.
//! - [`to_markdown`] writes the notebook as one Markdown document, with a picture output saved
//!   as a separate file that the document refers to.
//! - [`to_html`] writes one web page that needs no other file, with the stylesheet in the page and
//!   every picture inside it as a `data:` address.
//!
//! None of these touch the disk. The caller writes the text and the picture files wherever the
//! person chose.

use crate::nbformat::{self, Cell, CellKind, Notebook, Output};
use crate::outputs::{base64_encode, shown, Ansi, Shown, Span, Table};
use crate::text::{percent_head, read_percent_marker};
use serde_json::Value;

// ---------------------------------------------------------------------------------------------
// Python percent format
// ---------------------------------------------------------------------------------------------

/// Writes a notebook as a Python file in the percent format.
///
/// A code cell is `# %%` and then its source. A Markdown cell is `# %% [markdown]` and then its
/// source with `# ` in front of every line (`#` alone for an empty line, so no line ends in a
/// space). A raw cell is the same with `# %% [raw]`. One blank line separates the cells. Trailing
/// newlines of a source are not written, because [`from_python`] trims them again. Outputs and cell
/// ids are not written.
///
/// The percent format has no way to write a `# %%` line that does not start a cell, so a cell with
/// such a line in its source comes back from [`from_python`] as two cells. The notebook itself keeps
/// it, because the notebook tab only treats its own marker lines as cell boundaries.
pub fn to_python(nb: &Notebook) -> String {
    nb.cells.iter().map(|cell| script_cell(cell, "#")).collect::<Vec<String>>().join("\n")
}

/// Writes a notebook as a Rust file in the percent format, which is [`to_python`] with `//` where
/// Python has `#`: `// %%` before a code cell, `// %% [markdown]` before a Markdown cell, and `// `
/// in front of each line of a Markdown or raw cell. `task-2229`.
pub fn to_rust(nb: &Notebook) -> String {
    nb.cells.iter().map(|cell| script_cell(cell, "//")).collect::<Vec<String>>().join("\n")
}

/// One cell of a script: its marker line and its source, each ending in a newline. `comment` is the
/// language's line comment, which starts the marker and every line of a Markdown or raw cell.
fn script_cell(cell: &Cell, comment: &str) -> String {
    let source = cell.source.trim_end_matches(['\n', '\r']);
    let head = percent_head(cell.kind);
    let marker = format!("{comment}{}", head.strip_prefix('#').unwrap_or(head));
    let body = match cell.kind {
        CellKind::Code => source.to_string(),
        CellKind::Markdown | CellKind::Raw => comment_lines(source, comment),
    };
    if body.is_empty() {
        format!("{marker}\n")
    } else {
        format!("{marker}\n{body}\n")
    }
}

/// Puts `comment` and a space in front of every line, and `comment` alone in place of an empty line.
fn comment_lines(text: &str, comment: &str) -> String {
    text.lines()
        .map(|line| match line.is_empty() {
            true => comment.to_string(),
            false => format!("{comment} {line}"),
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Reads a Python file as a notebook.
///
/// The file is split at the lines that start with `# %%` or `#%%`, with the same rules for
/// `[markdown]`, `[raw]` and `[code]` as the notebook tab uses. The text before the first marker is
/// a code cell unless it is blank, and a file with no marker is one code cell. In a Markdown or raw
/// cell one leading `# ` (or just `#`) is removed from each line, which undoes [`to_python`].
/// Blank lines at the end of each cell are dropped. The notebook has the metadata that
/// [`nbformat::empty`] gives a new notebook, and every cell has a new id.
pub fn from_python(source: &str) -> Notebook {
    let mut cells: Vec<Cell> = Vec::new();
    let mut kind = CellKind::Code;
    let mut lines: Vec<&str> = Vec::new();
    let mut seen_marker = false;
    for line in source.lines() {
        match read_percent_marker(line) {
            Some(next_kind) => {
                finish_python_cell(&mut cells, kind, &lines, seen_marker);
                (kind, lines, seen_marker) = (next_kind, Vec::new(), true);
            }
            None => lines.push(line),
        }
    }
    finish_python_cell(&mut cells, kind, &lines, seen_marker);
    let mut notebook = nbformat::empty();
    if !cells.is_empty() {
        notebook.cells = cells;
    }
    notebook
}

/// Adds the cell that the collected lines make. The text before the first marker only counts when
/// it is not blank, and a cell that follows a marker always counts.
fn finish_python_cell(cells: &mut Vec<Cell>, kind: CellKind, lines: &[&str], after_marker: bool) {
    if !after_marker && lines.iter().all(|line| line.trim().is_empty()) {
        return;
    }
    let mut kept: Vec<&str> = lines
        .iter()
        .map(|line| if kind == CellKind::Code { *line } else { uncomment(line) })
        .collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    cells.push(Cell::new(kind, &nbformat::new_id(), &kept.join("\n")));
}

/// Removes one `# ` or `#` from the start of a line.
fn uncomment(line: &str) -> &str {
    line.strip_prefix("# ").or_else(|| line.strip_prefix('#')).unwrap_or(line)
}

// ---------------------------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------------------------

/// The programming language of the notebook, from `language_info`, then the kernel spec, then
/// `python`, in lower case, because evcxr names its language `Rust` and a fence is read by its lower
/// case name.
pub fn language_of(nb: &Notebook) -> String {
    let name = |path: [&str; 2]| {
        nb.metadata
            .get(path[0])
            .and_then(|value| value.get(path[1]))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    name(["language_info", "name"])
        .or_else(|| name(["kernelspec", "language"]))
        .map(|language| language.to_lowercase())
        .unwrap_or_else(|| "python".to_string())
}

/// A fenced code block. The fence is made longer than any run of backticks in the text, so the text
/// cannot end the block early.
fn fenced(text: &str, language: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}{language}\n{text}\n{fence}")
}

/// Writes a notebook as a Markdown document.
///
/// A Markdown cell is written as it is. A code cell is a fenced block in the notebook's language
/// (`metadata.language_info.name`, or `python`). Raw cells are left out, because they hold text for
/// some other converter. Each output follows its cell: text, streams and JSON as fenced blocks,
/// errors as a fenced block with the colour codes removed, tables as pipe tables, Markdown as it is,
/// HTML that is not a table as its plain text, and LaTeX between `$$` lines.
///
/// A picture is not put in the text. Its bytes are added to `pictures` under the name
/// `<stem>_<n>.png` (`.jpg` or `.svg` for the other kinds, with `n` counting from the number of
/// pictures already in the list plus one), and the text refers to it as `![output](<name>)`. The
/// caller saves those files next to the document.
pub fn to_markdown(nb: &Notebook, pictures: &mut Vec<(String, Vec<u8>)>, stem: &str) -> String {
    let language = language_of(nb);
    let mut blocks: Vec<String> = Vec::new();
    for cell in &nb.cells {
        let source = cell.source.trim_end();
        match cell.kind {
            CellKind::Markdown if !source.trim().is_empty() => blocks.push(source.to_string()),
            CellKind::Code => {
                if !source.trim().is_empty() {
                    blocks.push(fenced(source, &language));
                }
                blocks.extend(
                    cell.outputs
                        .iter()
                        .filter_map(|output| output_markdown(output, pictures, stem)),
                );
            }
            _ => {}
        }
    }
    format!("{}\n", blocks.join("\n\n"))
}

/// The Markdown for one output, or `None` when it has nothing to show.
fn output_markdown(
    output: &Output,
    pictures: &mut Vec<(String, Vec<u8>)>,
    stem: &str,
) -> Option<String> {
    let text = match shown(output) {
        Shown::Markdown(text) | Shown::Html(text) => text.trim_end().to_string(),
        Shown::Table(table) => table_markdown(&table),
        Shown::Png(bytes) => picture_markdown(pictures, stem, "png", bytes),
        Shown::Jpeg(bytes) => picture_markdown(pictures, stem, "jpg", bytes),
        Shown::Svg(source) => picture_markdown(pictures, stem, "svg", source.into_bytes()),
        Shown::Latex(source) if source.trim_start().starts_with('$') => source.trim().to_string(),
        Shown::Latex(source) => format!("$$\n{}\n$$", source.trim()),
        Shown::Json(text) => fenced(&text, "json"),
        Shown::Text(text) | Shown::Stream { text, .. } if !text.trim().is_empty() => {
            fenced(text.trim_end_matches('\n'), "")
        }
        Shown::Error { ename, evalue, traceback } => {
            fenced(&error_text(&ename, &evalue, &traceback), "")
        }
        _ => String::new(),
    };
    (!text.trim().is_empty()).then_some(text)
}

/// The traceback as plain text, or the exception name and message when there is no traceback.
fn error_text(ename: &str, evalue: &str, traceback: &[Vec<Span>]) -> String {
    let lines: Vec<String> = traceback
        .iter()
        .map(|line| line.iter().map(|span| span.text.as_str()).collect::<String>())
        .collect();
    if lines.is_empty() {
        format!("{ename}: {evalue}")
    } else {
        lines.join("\n")
    }
}

/// Adds a picture to the list and returns the Markdown that refers to it.
fn picture_markdown(
    pictures: &mut Vec<(String, Vec<u8>)>,
    stem: &str,
    extension: &str,
    bytes: Vec<u8>,
) -> String {
    let name = format!("{stem}_{}.{extension}", pictures.len() + 1);
    pictures.push((name.clone(), bytes));
    format!("![output]({name})")
}

/// A table as a Markdown pipe table. Header rows are combined into one: for each column the
/// different non empty parts are joined with a space. A `|` inside a cell is escaped.
fn table_markdown(table: &Table) -> String {
    let width = table.header.iter().chain(&table.rows).map(Vec::len).max().unwrap_or(0);
    let heading: Vec<String> = (0..width).map(|column| combined_heading(table, column)).collect();
    let line = |cells: &[String]| {
        format!(
            "| {} |",
            cells.iter().map(|cell| cell.replace('|', "\\|")).collect::<Vec<String>>().join(" | ")
        )
    };
    let mut lines = vec![line(&heading), format!("|{}", " --- |".repeat(width))];
    lines.extend(table.rows.iter().map(|row| line(row)));
    let mut text = lines.join("\n");
    if let Some(caption) = &table.caption {
        text = format!("{caption}\n\n{text}");
    }
    if let Some(footer) = &table.footer {
        text = format!("{text}\n\n{footer}");
    }
    text
}

/// The heading of one column: the parts from each header row that differ from the part before.
fn combined_heading(table: &Table, column: usize) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for row in &table.header {
        let part = row.get(column).map_or("", String::as_str);
        if !part.is_empty() && parts.last() != Some(&part) {
            parts.push(part);
        }
    }
    parts.join(" ")
}

// ---------------------------------------------------------------------------------------------
// HTML page
// ---------------------------------------------------------------------------------------------

/// The stylesheet of the page: a light background, a sans serif face for text, a monospace face for
/// code, a grey box for code cells and a red box for text on the error stream.
const STYLE: &str = "\
body { margin: 0; background: #ffffff; color: #1f2328; font: 16px/1.55 system-ui, -apple-system, \"Segoe UI\", Helvetica, Arial, sans-serif; }
main { max-width: 62rem; margin: 0 auto; padding: 1.5rem 1rem 3rem; }
.cell { margin: 0 0 1rem; }
.md h1, .md h2, .md h3, .md h4, .md h5, .md h6 { line-height: 1.25; margin: 1.2em 0 0.5em; }
.md p, .md ul, .md ol, .md blockquote, .md table, .md pre { margin: 0 0 0.9em; }
.md blockquote { border-left: 4px solid #d0d7de; margin-left: 0; padding-left: 1em; color: #57606a; }
.md table, .output table { border-collapse: collapse; }
.md th, .md td { border: 1px solid #d0d7de; padding: 0.25em 0.7em; }
.md img { max-width: 100%; }
.md hr { border: 0; border-top: 1px solid #d0d7de; }
.md code { background: #eff1f3; padding: 0.1em 0.3em; border-radius: 3px; }
.md pre code { background: none; padding: 0; }
pre, code { font-family: ui-monospace, SFMono-Regular, Consolas, \"Liberation Mono\", monospace; font-size: 0.875rem; }
pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.row { display: flex; gap: 0.6rem; margin-bottom: 0.4rem; }
.prompt { flex: none; width: 5.5rem; text-align: right; color: #6e7781; font: 0.8rem ui-monospace, Consolas, monospace; padding-top: 0.5rem; }
.input { flex: 1; min-width: 0; background: #f3f4f6; border: 1px solid #d8dee4; border-radius: 4px; padding: 0.5rem 0.7rem; }
.output { flex: 1; min-width: 0; overflow-x: auto; padding: 0.3rem 0.2rem; }
.output.stderr { background: #fdecea; }
.output.error pre { background: #fdf2f2; }
.output img { max-width: 100%; }
.raw pre { color: #57606a; }
";

/// Writes a notebook as one web page that needs no other file.
///
/// A code cell is a grey box with an `In [n]:` label, and its outputs follow it. Markdown cells go
/// through a small converter in this module. It supports headings, paragraphs, bold, italic,
/// inline code, fenced code, lists, links, images, quotes, rules and pipe tables, escapes all
/// text, and never lets HTML or a `javascript:` address from a Markdown cell into the page.
///
/// Outputs are different. A `text/html` output is copied into the page as the kernel wrote it,
/// which is what Jupyter's own export does, so a script in such an output runs when the page is
/// opened. Pictures are `data:` addresses, SVG is copied in as it is, text is escaped, and error
/// tracebacks keep their colours as `<span style="color:...">`.
pub fn to_html(nb: &Notebook, title: &str) -> String {
    let body: String = nb.cells.iter().map(html_cell).collect();
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n<main>\n{body}</main>\n</body>\n</html>\n",
        escape_html(title)
    )
}

/// The page content of one cell.
fn html_cell(cell: &Cell) -> String {
    match cell.kind {
        CellKind::Markdown => format!(
            "<div class=\"cell markdown\"><div class=\"md\">\n{}</div></div>\n",
            markdown_to_html(&cell.source, cell.attachments.as_ref())
        ),
        CellKind::Raw => {
            format!("<div class=\"cell raw\"><pre>{}</pre></div>\n", escape_html(&cell.source))
        }
        CellKind::Code => {
            let label = cell.execution_count.map_or(" ".to_string(), |count| count.to_string());
            let mut html = format!("<div class=\"cell code\">\n<div class=\"row\"><div class=\"prompt\">In [{label}]:</div><pre class=\"input\">{}</pre></div>\n", escape_html(&cell.source));
            html.extend(cell.outputs.iter().map(|output| html_output(cell, output)));
            html.push_str("</div>\n");
            html
        }
    }
}

/// The page content of one output, as a row with a label on the left. Nothing is written for an
/// output with no content.
fn html_output(cell: &Cell, output: &Output) -> String {
    let shown = shown(output);
    let (class, content) = match &shown {
        Shown::Stream { stderr, text } if !text.is_empty() => (
            if *stderr { "stream stderr" } else { "stream" },
            format!("<pre>{}</pre>", escape_html(text)),
        ),
        Shown::Error { traceback, ename, evalue } => {
            ("error", format!("<pre>{}</pre>", error_html(ename, evalue, traceback)))
        }
        Shown::Stream { .. } => return String::new(),
        other => ("result", result_html(output, other)),
    };
    if content.trim().is_empty() {
        return String::new();
    }
    let label = match (output.output_type(), cell.execution_count) {
        ("execute_result", Some(count)) => format!("Out [{count}]:"),
        _ => String::new(),
    };
    format!("<div class=\"row\"><div class=\"prompt\">{label}</div><div class=\"output {class}\">{content}</div></div>\n")
}

/// The page content of a result or display output.
fn result_html(output: &Output, shown: &Shown) -> String {
    match shown {
        Shown::Markdown(source) => {
            format!("<div class=\"md\">\n{}</div>", markdown_to_html(source, None))
        }
        Shown::Table(_) | Shown::Html(_) => output.mime_text("text/html").unwrap_or_default(),
        Shown::Png(bytes) => {
            format!("<img alt=\"output\" src=\"data:image/png;base64,{}\">", base64_encode(bytes))
        }
        Shown::Jpeg(bytes) => {
            format!("<img alt=\"output\" src=\"data:image/jpeg;base64,{}\">", base64_encode(bytes))
        }
        Shown::Svg(source) => source.clone(),
        Shown::Latex(text) | Shown::Json(text) | Shown::Text(text) if !text.trim().is_empty() => {
            format!("<pre>{}</pre>", escape_html(text))
        }
        _ => String::new(),
    }
}

/// A traceback with its colours as `<span>` elements. A missing traceback gives the exception name
/// and message.
fn error_html(ename: &str, evalue: &str, traceback: &[Vec<Span>]) -> String {
    if traceback.is_empty() {
        return escape_html(&format!("{ename}: {evalue}"));
    }
    traceback.iter().map(|line| spans_html(line)).collect::<Vec<String>>().join("\n")
}

/// Spans as HTML. Text with no colour, background or bold is written without a `<span>`.
fn spans_html(spans: &[Span]) -> String {
    let mut html = String::new();
    for span in spans {
        let mut style = String::new();
        if let Some(colour) = span.colour {
            style.push_str(&format!("color:{};", css_colour(colour)));
        }
        if let Some(background) = span.background {
            style.push_str(&format!("background-color:{};", css_colour(background)));
        }
        if span.bold {
            style.push_str("font-weight:bold;");
        }
        if style.is_empty() {
            html.push_str(&escape_html(&span.text));
        } else {
            html.push_str(&format!("<span style=\"{style}\">{}</span>", escape_html(&span.text)));
        }
    }
    html
}

/// The colours of the 16 terminal colours, chosen to be readable on a white page.
const PALETTE: [(u8, u8, u8); 16] = [
    (46, 52, 54),
    (192, 57, 43),
    (30, 132, 73),
    (154, 125, 10),
    (46, 95, 181),
    (142, 68, 173),
    (20, 143, 143),
    (127, 140, 141),
    (85, 87, 83),
    (231, 76, 60),
    (39, 174, 96),
    (183, 149, 11),
    (52, 152, 219),
    (175, 96, 203),
    (22, 160, 133),
    (110, 118, 125),
];

/// A terminal colour as `#rrggbb`.
fn css_colour(colour: Ansi) -> String {
    let (r, g, b) = match colour {
        Ansi::Rgb(r, g, b) => (r, g, b),
        Ansi::Indexed(number) => indexed_rgb(number),
        named => PALETTE[named_index(named)],
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The place of a named colour in the 16 colour table.
fn named_index(colour: Ansi) -> usize {
    match colour {
        Ansi::Black => 0,
        Ansi::Red => 1,
        Ansi::Green => 2,
        Ansi::Yellow => 3,
        Ansi::Blue => 4,
        Ansi::Magenta => 5,
        Ansi::Cyan => 6,
        Ansi::White => 7,
        Ansi::BrightBlack => 8,
        Ansi::BrightRed => 9,
        Ansi::BrightGreen => 10,
        Ansi::BrightYellow => 11,
        Ansi::BrightBlue => 12,
        Ansi::BrightMagenta => 13,
        Ansi::BrightCyan => 14,
        Ansi::BrightWhite => 15,
        Ansi::Rgb(..) | Ansi::Indexed(_) => 0,
    }
}

/// A colour of the 256 colour table: the 16 terminal colours, a 6 by 6 by 6 cube, and 24 greys.
fn indexed_rgb(number: u8) -> (u8, u8, u8) {
    match number {
        0..=15 => PALETTE[number as usize],
        16..=231 => {
            let cube = number - 16;
            let level = |step: u8| if step == 0 { 0 } else { 55 + 40 * step };
            (level(cube / 36), level(cube / 6 % 6), level(cube % 6))
        }
        _ => {
            let grey = 8 + 10 * (number - 232);
            (grey, grey, grey)
        }
    }
}

/// Escapes the five characters that matter in HTML text and in a quoted attribute.
fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        push_escaped(&mut escaped, c);
    }
    escaped
}

/// Adds one character to a string, escaped for HTML.
fn push_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&#39;"),
        other => out.push(other),
    }
}

// ---------------------------------------------------------------------------------------------
// A small Markdown converter
// ---------------------------------------------------------------------------------------------

/// Converts Markdown to HTML. All text is escaped and HTML in the source is shown as text. The
/// attachments of the cell, when it has any, are what `attachment:name` image addresses refer to.
fn markdown_to_html(source: &str, attachments: Option<&Value>) -> String {
    let normalised = source.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ");
    let lines: Vec<&str> = normalised.lines().collect();
    blocks_html(&lines, attachments)
}

/// Converts lines to a run of block elements.
fn blocks_html(lines: &[&str], attachments: Option<&Value>) -> String {
    let mut html = String::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim().is_empty() {
            index += 1;
            continue;
        }
        let (block, used) = block_html(lines, index, attachments);
        html.push_str(&block);
        index += used.max(1);
    }
    html
}

/// Converts the block that starts at a line and says how many lines it used.
fn block_html(lines: &[&str], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let line = lines[index];
    if let Some(fence) = fence_start(line) {
        return code_block_html(lines, index, fence);
    }
    if let Some((level, text)) = heading_of(line) {
        return (format!("<h{level}>{}</h{level}>\n", inline_html(text, attachments)), 1);
    }
    if is_rule(line) {
        return ("<hr>\n".to_string(), 1);
    }
    if line.trim_start().starts_with('>') {
        return quote_html(lines, index, attachments);
    }
    if list_marker(line).is_some() {
        return list_html(lines, index, attachments);
    }
    if starts_table(lines, index) {
        return table_html(lines, index, attachments);
    }
    paragraph_html(lines, index, attachments)
}

/// The number of leading spaces of a line.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// The character and length of a code fence at the start of a line (up to three spaces of indent
/// allowed), such as `` ``` `` or `~~~~`.
fn fence_start(line: &str) -> Option<(char, usize)> {
    if indent_of(line) > 3 {
        return None;
    }
    let trimmed = line.trim_start();
    let c = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = trimmed.chars().take_while(|x| *x == c).count();
    (length >= 3 && !(c == '`' && trimmed[length..].contains('`'))).then_some((c, length))
}

/// A fenced code block, from its opening fence to its closing fence or the end of the text.
fn code_block_html(
    lines: &[&str],
    index: usize,
    (fence_char, fence_length): (char, usize),
) -> (String, usize) {
    let info = lines[index].trim_start()[fence_length..].trim();
    let language: String = info
        .split_whitespace()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_+#.".contains(*c))
        .collect();
    let mut code = String::new();
    let mut used = 1;
    for line in &lines[index + 1..] {
        used += 1;
        let trimmed = line.trim();
        if indent_of(line) <= 3
            && trimmed.len() >= fence_length
            && trimmed.chars().all(|c| c == fence_char)
        {
            break;
        }
        code.push_str(&escape_html(line));
        code.push('\n');
    }
    let class = if language.is_empty() {
        String::new()
    } else {
        format!(" class=\"language-{}\"", escape_html(&language))
    };
    (format!("<pre><code{class}>{code}</code></pre>\n"), used)
}

/// The level and text of a `#` heading line.
fn heading_of(line: &str) -> Option<(usize, &str)> {
    if indent_of(line) > 3 {
        return None;
    }
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    let rest = &trimmed[level..];
    if !(1..=6).contains(&level) || !(rest.is_empty() || rest.starts_with(' ')) {
        return None;
    }
    let text = rest.trim();
    let without_closing = text.trim_end_matches('#');
    let text = if without_closing.is_empty() || without_closing.ends_with(' ') {
        without_closing.trim_end()
    } else {
        text
    };
    Some((level, text))
}

/// Whether a line is a horizontal rule: three or more of `-`, `*` or `_` and nothing but spaces.
fn is_rule(line: &str) -> bool {
    let marks: Vec<char> = line.chars().filter(|c| *c != ' ').collect();
    indent_of(line) <= 3
        && marks.len() >= 3
        && ['-', '*', '_'].iter().any(|m| marks.iter().all(|c| c == m))
}

/// A block quote: the lines that start with `>`, converted as Markdown of their own.
fn quote_html(lines: &[&str], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let inner: Vec<&str> = lines[index..]
        .iter()
        .take_while(|line| line.trim_start().starts_with('>'))
        .map(|line| {
            let rest = &line.trim_start()[1..];
            rest.strip_prefix(' ').unwrap_or(rest)
        })
        .collect();
    (format!("<blockquote>\n{}</blockquote>\n", blocks_html(&inner, attachments)), inner.len())
}

/// What a list item marker line says: the indent, whether the list is numbered, the number, and
/// the offset where the item text starts.
struct ListMarker {
    indent: usize,
    ordered: bool,
    number: usize,
    content: usize,
}

/// Reads a list marker (`- `, `* `, `+ `, `1. ` or `1) `) at the start of a line.
fn list_marker(line: &str) -> Option<ListMarker> {
    let indent = indent_of(line);
    let rest = &line[indent..];
    let first = rest.chars().next()?;
    if matches!(first, '-' | '*' | '+') && (rest.len() == 1 || rest[1..].starts_with(' ')) {
        return Some(ListMarker { indent, ordered: false, number: 0, content: indent + 2 });
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let after = &rest[digits..];
    if (1..=9).contains(&digits)
        && (after.starts_with(". ") || after.starts_with(") ") || after == "." || after == ")")
    {
        return Some(ListMarker {
            indent,
            ordered: true,
            number: rest[..digits].parse().ok()?,
            content: indent + digits + 2,
        });
    }
    None
}

/// A list with its items. A line indented more than the markers belongs to the item above it, and
/// is converted as Markdown, so a list inside an item is a nested list.
fn list_html(lines: &[&str], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let first = list_marker(lines[index]).expect("a list starts at a marker");
    let mut items: Vec<Vec<String>> = Vec::new();
    let mut used = 0;
    while index + used < lines.len() {
        let line = lines[index + used];
        if line.trim().is_empty() {
            if !list_goes_on(lines, index + used + 1, &first) {
                break;
            }
            if let Some(item) = items.last_mut() {
                item.push(String::new());
            }
        } else if let Some(marker) =
            list_marker(line).filter(|marker| marker.indent <= first.indent + 1)
        {
            if marker.ordered != first.ordered {
                break;
            }
            let text = line.get(marker.content.min(line.len())..).unwrap_or("");
            items.push(vec![text.to_string()]);
        } else if indent_of(line) > first.indent
            || (!starts_other_block(line) && used > 0 && !lines[index + used - 1].trim().is_empty())
        {
            let strip = indent_of(line).min(first.content);
            items.last_mut().expect("an item has started").push(line[strip..].to_string());
        } else {
            break;
        }
        used += 1;
    }
    let tag = if first.ordered { "ol" } else { "ul" };
    let start = if first.ordered && first.number != 1 {
        format!(" start=\"{}\"", first.number)
    } else {
        String::new()
    };
    let body: String =
        items.iter().map(|item| format!("<li>{}</li>\n", item_html(item, attachments))).collect();
    (format!("<{tag}{start}>\n{body}</{tag}>\n"), used)
}

/// Whether the list continues after a blank line: the next line with text is a marker of this list,
/// or is indented under it.
fn list_goes_on(lines: &[&str], from: usize, first: &ListMarker) -> bool {
    match lines[from.min(lines.len())..].iter().find(|line| !line.trim().is_empty()) {
        Some(line) => match list_marker(line) {
            Some(marker) => {
                marker.indent > first.indent
                    || (marker.indent <= first.indent + 1 && marker.ordered == first.ordered)
            }
            None => indent_of(line) > first.indent,
        },
        None => false,
    }
}

/// The content of a list item. When it starts with a paragraph, the paragraph is written without
/// its `<p>` element.
fn item_html(item: &[String], attachments: Option<&Value>) -> String {
    let lines: Vec<&str> = item.iter().map(String::as_str).collect();
    let html = blocks_html(&lines, attachments);
    match html.strip_prefix("<p>") {
        Some(rest) => rest.replacen("</p>\n", "", 1),
        None => html,
    }
}

/// Whether a line starts a block that ends a paragraph or a list item's lazy continuation.
fn starts_other_block(line: &str) -> bool {
    let bullet_or_first =
        list_marker(line).is_some_and(|marker| !marker.ordered || marker.number == 1);
    fence_start(line).is_some()
        || heading_of(line).is_some()
        || is_rule(line)
        || line.trim_start().starts_with('>')
        || bullet_or_first
}

/// Splits a table row at the `|` characters that are not escaped, and trims each cell.
fn split_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let mut cells = vec![String::new()];
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                chars.next();
                cells.last_mut().expect("there is a cell").push('|');
            }
            '|' => cells.push(String::new()),
            other => cells.last_mut().expect("there is a cell").push(other),
        }
    }
    if cells.len() > 1 && cells.last().is_some_and(|cell| cell.trim().is_empty()) {
        cells.pop();
    }
    cells.into_iter().map(|cell| cell.trim().to_string()).collect()
}

/// The alignment of each column from a table's separator row (`---`, `:--`, `:-:`, `--:`), or
/// `None` when the line is not a separator row.
fn table_alignments(line: &str) -> Option<Vec<&'static str>> {
    if !line.contains('-') {
        return None;
    }
    split_row(line)
        .iter()
        .map(|cell| {
            let core = cell.trim_matches(':');
            if core.is_empty() || !core.chars().all(|c| c == '-') {
                return None;
            }
            Some(match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => "center",
                (true, false) => "left",
                (false, true) => "right",
                (false, false) => "",
            })
        })
        .collect()
}

/// Whether a table starts at this line: a row with a `|` followed by a separator row of the same
/// number of columns.
fn starts_table(lines: &[&str], index: usize) -> bool {
    let Some(separator) = lines.get(index + 1) else { return false };
    lines[index].contains('|')
        && table_alignments(separator)
            .is_some_and(|aligns| aligns.len() == split_row(lines[index]).len())
}

/// A pipe table, up to the first blank line or line without a `|`.
fn table_html(lines: &[&str], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let aligns = table_alignments(lines[index + 1]).unwrap_or_default();
    let cell = |tag: &str, text: &str, column: usize| {
        let style = aligns
            .get(column)
            .filter(|align| !align.is_empty())
            .map_or(String::new(), |align| format!(" style=\"text-align:{align}\""));
        format!("<{tag}{style}>{}</{tag}>", inline_html(text, attachments))
    };
    let row = |tag: &str, line: &str| {
        let cells: String = split_row(line)
            .iter()
            .enumerate()
            .map(|(column, text)| cell(tag, text, column))
            .collect();
        format!("<tr>{cells}</tr>\n")
    };
    let body_lines: Vec<&str> = lines[index + 2..]
        .iter()
        .take_while(|line| !line.trim().is_empty() && line.contains('|'))
        .copied()
        .collect();
    let body: String = body_lines.iter().map(|line| row("td", line)).collect();
    (
        format!(
            "<table>\n<thead>\n{}</thead>\n<tbody>\n{body}</tbody>\n</table>\n",
            row("th", lines[index])
        ),
        2 + body_lines.len(),
    )
}

/// A paragraph: lines up to a blank line or the start of another block.
fn paragraph_html(lines: &[&str], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let mut used = 1;
    while let Some(line) = lines.get(index + used) {
        if line.trim().is_empty() || starts_other_block(line) || starts_table(lines, index + used) {
            break;
        }
        used += 1;
    }
    let text =
        lines[index..index + used].iter().map(|line| line.trim()).collect::<Vec<&str>>().join("\n");
    (format!("<p>{}</p>\n", inline_html(&text, attachments)), used)
}

/// Converts the inline parts of a text: code, links, images, bold and italic.
fn inline_html(text: &str, attachments: Option<&Value>) -> String {
    let chars: Vec<char> = text.chars().collect();
    inline_chars(&chars, attachments)
}

/// Converts characters to inline HTML, one construct at a time.
fn inline_chars(chars: &[char], attachments: Option<&Value>) -> String {
    let mut html = String::new();
    let mut index = 0;
    while index < chars.len() {
        let (piece, used) = inline_at(chars, index, attachments);
        html.push_str(&piece);
        index += used.max(1);
    }
    html
}

/// Converts the inline construct that starts at a character, or the character itself when none
/// does, and says how many characters it used.
fn inline_at(chars: &[char], index: usize, attachments: Option<&Value>) -> (String, usize) {
    let literal = |c: char| {
        let mut text = String::new();
        push_escaped(&mut text, c);
        (text, 1)
    };
    match chars[index] {
        '\\' if chars.get(index + 1).is_some_and(char::is_ascii_punctuation) => {
            (literal(chars[index + 1]).0, 2)
        }
        '`' => code_span(chars, index).unwrap_or_else(|| {
            let run = chars[index..].iter().take_while(|c| **c == '`').count();
            ("`".repeat(run), run)
        }),
        '!' if chars.get(index + 1) == Some(&'[') => {
            match link_at(chars, index + 1, true, attachments) {
                Some((html, used)) => (html, used + 1),
                None => literal('!'),
            }
        }
        '[' => link_at(chars, index, false, attachments).unwrap_or_else(|| literal('[')),
        c @ ('*' | '_') => emphasis_at(chars, index, c, attachments).unwrap_or_else(|| literal(c)),
        other => literal(other),
    }
}

/// A code span: text between two runs of the same number of backticks.
fn code_span(chars: &[char], index: usize) -> Option<(String, usize)> {
    let run = chars[index..].iter().take_while(|c| **c == '`').count();
    let mut position = index + run;
    while position < chars.len() {
        if chars[position] != '`' {
            position += 1;
            continue;
        }
        let length = chars[position..].iter().take_while(|c| **c == '`').count();
        if length == run {
            let inner: String = chars[index + run..position]
                .iter()
                .map(|c| if *c == '\n' { ' ' } else { *c })
                .collect();
            let inner = if inner.len() > 2
                && inner.starts_with(' ')
                && inner.ends_with(' ')
                && inner.trim() != ""
            {
                inner[1..inner.len() - 1].to_string()
            } else {
                inner
            };
            return Some((format!("<code>{}</code>", escape_html(&inner)), position + run - index));
        }
        position += length;
    }
    None
}

/// The position of the bracket that closes the one at `open`, counting nested pairs.
fn matching_close(chars: &[char], open: usize, opener: char, closer: char) -> Option<usize> {
    let mut depth = 0;
    let mut index = open;
    while index < chars.len() {
        match chars[index] {
            '\\' => index += 1,
            c if c == opener => depth += 1,
            c if c == closer => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// A link `[text](address)` or an image `![alt](address)`, starting at the `[`. An address that is
/// not allowed leaves just the text, or the alt text of an image.
fn link_at(
    chars: &[char],
    open: usize,
    image: bool,
    attachments: Option<&Value>,
) -> Option<(String, usize)> {
    let close = matching_close(chars, open, '[', ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let paren = matching_close(chars, close + 1, '(', ')')?;
    let destination: String = chars[close + 2..paren].iter().collect();
    let address = destination
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string();
    let label = &chars[open + 1..close];
    let used = paren + 1 - open;
    let safe = safe_address(&address, image, attachments);
    let html = match (image, safe) {
        (true, Some(src)) => format!(
            "<img src=\"{}\" alt=\"{}\">",
            escape_html(&src),
            escape_html(&label.iter().collect::<String>())
        ),
        (true, None) => escape_html(&label.iter().collect::<String>()),
        (false, Some(href)) => format!(
            "<a href=\"{}\" rel=\"noopener\">{}</a>",
            escape_html(&href),
            inline_chars(label, attachments)
        ),
        (false, None) => inline_chars(label, attachments),
    };
    Some((html, used))
}

/// The address to write into a page, or `None` when it must not be. A link may go to `http`,
/// `https` or `mailto`, or be relative. An image may also be a `data:image/` address, or an
/// `attachment:` name that the cell holds a picture for, which becomes a `data:` address. Every
/// other scheme, such as `javascript:`, is refused.
fn safe_address(address: &str, image: bool, attachments: Option<&Value>) -> Option<String> {
    if address.is_empty() || address.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let lower = address.to_ascii_lowercase();
    if image && lower.starts_with("attachment:") {
        return attachment_address(&address["attachment:".len()..], attachments?);
    }
    let scheme_end =
        lower.find([':', '/', '?', '#']).filter(|position| lower.as_bytes()[*position] == b':');
    let Some(end) = scheme_end else { return Some(address.to_string()) };
    let allowed = match &lower[..end] {
        "http" | "https" => true,
        "mailto" => !image,
        "data" => image && lower.starts_with("data:image/"),
        _ => false,
    };
    allowed.then(|| address.to_string())
}

/// The `data:` address of a picture attached to a Markdown cell, found by file name.
fn attachment_address(name: &str, attachments: &Value) -> Option<String> {
    let bundle = attachments.get(name)?.as_object()?;
    let (mime, data) = bundle.iter().find(|(mime, _)| mime.starts_with("image/"))?;
    let text = match data {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let base64: String = text.split_whitespace().collect();
    base64
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        .then(|| format!("data:{mime};base64,{base64}"))
}

/// Bold (`**x**`, `__x__`) or italic (`*x*`, `_x_`) starting at a delimiter. An underscore inside a
/// word, as in `snake_case`, is not a delimiter.
fn emphasis_at(
    chars: &[char],
    index: usize,
    delimiter: char,
    attachments: Option<&Value>,
) -> Option<(String, usize)> {
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric());
    if delimiter == '_' && index > 0 && word(chars.get(index - 1)) {
        return None;
    }
    for length in [2, 1] {
        let inner_start = index + length;
        let opens = (0..length).all(|k| chars.get(index + k) == Some(&delimiter))
            && chars.get(inner_start).is_some_and(|c| !c.is_whitespace());
        if !opens {
            continue;
        }
        if let Some(end) = emphasis_close(chars, inner_start, delimiter, length) {
            let inner = inline_chars(&chars[inner_start..end], attachments);
            let tag = if length == 2 { "strong" } else { "em" };
            return Some((format!("<{tag}>{inner}</{tag}>"), end + length - index));
        }
    }
    None
}

/// The position where emphasis that started before `from` ends: a run of `length` delimiters that
/// follows a character that is not a space. A single delimiter inside a double one is skipped, and
/// so is a backslash escaped one.
fn emphasis_close(chars: &[char], from: usize, delimiter: char, length: usize) -> Option<usize> {
    let mut index = from;
    while index < chars.len() {
        if chars[index] == '\\' {
            index += 2;
            continue;
        }
        let run = chars[index..].iter().take_while(|c| **c == delimiter).count();
        if run == 0 {
            index += 1;
            continue;
        }
        let closes = run >= length
            && (length == 2 || run == 1)
            && index > from
            && !chars[index - 1].is_whitespace();
        let ends_word =
            delimiter != '_' || !chars.get(index + length).is_some_and(|c| c.is_alphanumeric());
        if closes && ends_word {
            return Some(index);
        }
        index += run;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbformat::parse;
    use crate::outputs::strip_ansi;
    use serde_json::json;

    const ALL_OUTPUTS: &str = include_str!("../tests/fixtures/all_outputs.ipynb");
    const PNG_BASE64: &str = "iVBORw0KGgo=";

    fn notebook_of(cells: &[(CellKind, &str)]) -> Notebook {
        let mut notebook = nbformat::empty();
        notebook.cells = cells
            .iter()
            .map(|(kind, source)| Cell::new(*kind, &nbformat::new_id(), source))
            .collect();
        notebook
    }

    fn code_with_output(source: &str, output: Output) -> Notebook {
        let mut notebook = notebook_of(&[(CellKind::Code, source)]);
        notebook.cells[0].outputs.push(output);
        notebook.cells[0].execution_count = Some(7);
        notebook
    }

    fn png_output() -> Output {
        Output::display_data(json!({"image/png": PNG_BASE64, "text/plain": "<Figure>"}), json!({}))
    }

    #[test]
    fn the_python_file_has_percent_markers_commented_markdown_and_one_blank_line_between_cells() {
        let notebook = notebook_of(&[
            (CellKind::Markdown, "# Title\n\ntext"),
            (CellKind::Code, "x = 1\n\ny = 2"),
            (CellKind::Raw, "raw"),
        ]);
        assert_eq!(
            to_python(&notebook),
            "# %% [markdown]\n# # Title\n#\n# text\n\n# %%\nx = 1\n\ny = 2\n\n# %% [raw]\n# raw\n"
        );
    }

    #[test]
    fn the_rust_file_has_slash_markers_and_slash_commented_markdown() {
        let notebook = notebook_of(&[
            (CellKind::Markdown, "# Title\n\ntext"),
            (CellKind::Code, "let x = 1;\nx + 1"),
        ]);
        assert_eq!(
            to_rust(&notebook),
            "// %% [markdown]\n// # Title\n//\n// text\n\n// %%\nlet x = 1;\nx + 1\n"
        );
    }

    #[test]
    fn a_rust_notebook_fences_its_code_as_rust_in_lower_case() {
        let mut notebook = notebook_of(&[(CellKind::Code, "let x = 1;")]);
        notebook.metadata = nbformat::empty_for(nbformat::Language::Rust).metadata;
        assert_eq!(language_of(&notebook), "rust");
        assert!(
            to_markdown(&notebook, &mut Vec::new(), "n").starts_with("```rust\nlet x = 1;\n```")
        );
    }

    #[test]
    fn a_python_file_read_back_keeps_every_cells_kind_and_source() {
        let notebook = notebook_of(&[
            (CellKind::Markdown, "# Title\n\n  indented\ntext"),
            (CellKind::Code, "import os\n\n\nprint(os.name)"),
            (CellKind::Code, ""),
            (CellKind::Raw, "a\nb"),
            (CellKind::Code, "# a comment\nx = 1"),
        ]);
        let again = from_python(&to_python(&notebook));
        let found: Vec<(CellKind, &str)> =
            again.cells.iter().map(|cell| (cell.kind, cell.source.as_str())).collect();
        let wanted: Vec<(CellKind, &str)> =
            notebook.cells.iter().map(|cell| (cell.kind, cell.source.as_str())).collect();
        assert_eq!(found, wanted);
    }

    #[test]
    fn from_python_accepts_other_markers_and_trims_trailing_blank_lines() {
        let notebook =
            from_python("import os\n\n#%%\nx = 1\n\n\n# %% [markdown]\n#just text\n# more\n\n");
        let found: Vec<(CellKind, &str)> =
            notebook.cells.iter().map(|cell| (cell.kind, cell.source.as_str())).collect();
        assert_eq!(
            found,
            vec![
                (CellKind::Code, "import os"),
                (CellKind::Code, "x = 1"),
                (CellKind::Markdown, "just text\nmore")
            ]
        );
    }

    #[test]
    fn a_python_file_with_no_markers_is_one_code_cell_and_has_distinct_ids() {
        let notebook = from_python("a = 1\nb = 2\n");
        assert_eq!(notebook.cells.len(), 1);
        assert_eq!(
            (notebook.cells[0].kind, notebook.cells[0].source.as_str()),
            (CellKind::Code, "a = 1\nb = 2")
        );
        assert_eq!(from_python("").cells.len(), 1);
        let two = from_python("# %%\na\n# %%\nb");
        assert_ne!(two.cells[0].id, two.cells[1].id);
        assert_eq!(two.nbformat_minor, 5);
    }

    #[test]
    fn markdown_has_fenced_code_in_the_kernel_language_and_the_markdown_cells_as_they_are() {
        let mut notebook = notebook_of(&[
            (CellKind::Markdown, "# Title\n\nSome *text*.\n"),
            (CellKind::Code, "print('hi')"),
            (CellKind::Raw, "ignored"),
        ]);
        notebook.metadata = json!({"language_info": {"name": "julia"}});
        notebook.cells[1].outputs.push(Output::stream("stdout", "hi\n"));
        let mut pictures = Vec::new();
        assert_eq!(
            to_markdown(&notebook, &mut pictures, "nb"),
            "# Title\n\nSome *text*.\n\n```julia\nprint('hi')\n```\n\n```\nhi\n```\n"
        );
        assert!(pictures.is_empty());
    }

    #[test]
    fn markdown_writes_a_picture_as_a_file_reference_and_hands_back_the_bytes() {
        let notebook = code_with_output("plot()", png_output());
        let mut pictures = Vec::new();
        let text = to_markdown(&notebook, &mut pictures, "report");
        assert!(text.contains("![output](report_1.png)"), "{text}");
        assert!(!text.contains("<Figure>"));
        assert_eq!(
            pictures,
            vec![(
                "report_1.png".to_string(),
                vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
            )]
        );
    }

    #[test]
    fn markdown_writes_errors_without_colour_codes_and_tables_as_pipe_tables() {
        let notebook = parse(ALL_OUTPUTS).unwrap();
        let mut pictures = Vec::new();
        let text = to_markdown(&notebook, &mut pictures, "nb");
        assert!(text.contains("ZeroDivisionError: division by zero"));
        assert!(!text.contains('\u{1b}'));
        let html = Output::execute_result(
            Some(1),
            json!({"text/html": include_str!("../tests/fixtures/html/named_index.html"), "text/plain": "x"}),
            json!({}),
        );
        let table = to_markdown(&code_with_output("df", html), &mut Vec::new(), "nb");
        assert!(
            table.contains("|  | price | name |") || table.contains("| id | price | name |"),
            "{table}"
        );
        assert!(table.contains("| r1 | 1200 | Apple & Co |"), "{table}");
        assert!(table.contains("| --- | --- | --- |"));
    }

    #[test]
    fn a_code_cell_that_contains_a_fence_gets_a_longer_fence() {
        let notebook = notebook_of(&[(CellKind::Code, "s = '''\n```\n'''")]);
        let text = to_markdown(&notebook, &mut Vec::new(), "nb");
        assert!(text.starts_with("````python\n"), "{text}");
        assert!(text.trim_end().ends_with("\n````"));
    }

    #[test]
    fn html_escapes_a_script_in_a_markdown_cell() {
        let notebook = notebook_of(&[(
            CellKind::Markdown,
            "hello <script>alert(1)</script> [x](javascript:alert(1)) <b>b</b>",
        )]);
        let page = to_html(&notebook, "T");
        assert!(!page.contains("<script"), "{page}");
        assert!(!page.contains("javascript:"));
        assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(page.contains("&lt;b&gt;b&lt;/b&gt;"));
    }

    #[test]
    fn html_has_the_png_as_a_data_address_and_an_in_prompt() {
        let page = to_html(&code_with_output("plot()", png_output()), "Plots & more");
        assert!(
            page.contains("<img alt=\"output\" src=\"data:image/png;base64,iVBORw0KGgo=\">"),
            "{page}"
        );
        assert!(page.contains("<title>Plots &amp; more</title>"));
        assert!(page.contains("In [7]:"));
        assert!(page.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn html_keeps_kernel_html_as_it_is_and_colours_an_error() {
        let html = Output::display_data(
            json!({"text/html": "<table><tr><td>1</td></tr></table>", "text/plain": "x"}),
            json!({}),
        );
        assert!(to_html(&code_with_output("x", html), "T")
            .contains("<table><tr><td>1</td></tr></table>"));
        let page = to_html(&parse(ALL_OUTPUTS).unwrap(), "T");
        assert!(
            page.contains(
                "<span style=\"color:#c0392b;font-weight:bold;\">ZeroDivisionError</span>"
            ),
            "{page}"
        );
        assert!(!page.contains('\u{1b}'));
    }

    #[test]
    fn html_shows_standard_error_in_its_own_box_and_escapes_text() {
        let notebook = code_with_output("x", Output::stream("stderr", "<warn> & more"));
        let page = to_html(&notebook, "T");
        assert!(
            page.contains(
                "<div class=\"output stream stderr\"><pre>&lt;warn&gt; &amp; more</pre></div>"
            ),
            "{page}"
        );
    }

    #[test]
    fn every_div_in_the_page_is_closed() {
        let mut notebook = parse(ALL_OUTPUTS).unwrap();
        notebook.cells.push(Cell::new(
            CellKind::Markdown,
            "m",
            "> quote\n\n- a\n- b\n  - c\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n```py\nx<1\n```",
        ));
        let page = to_html(&notebook, "T");
        assert!(page.matches("<div").count() > 10);
        assert_eq!(page.matches("<div").count(), page.matches("</div>").count());
    }

    #[test]
    fn the_markdown_converter_handles_the_common_constructs() {
        let html = markdown_to_html("## Head\n\nA **bold**, *it*, `co<de>` and [link](https://x.org/a?b=1&c=2).\n\n1. one\n2. two\n\n---\n", None);
        assert!(html.contains("<h2>Head</h2>"), "{html}");
        assert!(html.contains("<strong>bold</strong>") && html.contains("<em>it</em>"));
        assert!(html.contains("<code>co&lt;de&gt;</code>"));
        assert!(html.contains("<a href=\"https://x.org/a?b=1&amp;c=2\" rel=\"noopener\">link</a>"));
        assert!(html.contains("<ol>\n<li>one</li>\n<li>two</li>\n</ol>"), "{html}");
        assert!(html.contains("<hr>"));
    }

    #[test]
    fn the_markdown_converter_nests_lists_and_reads_tables_and_quotes() {
        let html = markdown_to_html(
            "- a\n  - b\n- c\n\n> quoted *text*\n\n| h1 | h2 |\n|:--|--:|\n| x \\| y | 2 |\n",
            None,
        );
        assert!(
            html.contains("<ul>\n<li>a<ul>\n<li>b</li>\n</ul>\n</li>\n<li>c</li>\n</ul>"),
            "{html}"
        );
        assert!(
            html.contains("<blockquote>\n<p>quoted <em>text</em></p>\n</blockquote>"),
            "{html}"
        );
        assert!(
            html.contains(
                "<th style=\"text-align:left\">h1</th><th style=\"text-align:right\">h2</th>"
            ),
            "{html}"
        );
        assert!(html.contains("<td style=\"text-align:left\">x | y</td>"), "{html}");
    }

    #[test]
    fn markdown_images_allow_pictures_and_attachments_and_refuse_script_addresses() {
        let attachments = json!({"pic.png": {"image/png": "iVBORw0KGgo="}});
        let html = markdown_to_html("![a](attachment:pic.png) ![b](https://x.org/i.png) ![c](javascript:alert(1)) ![d](data:text/html;base64,AAAA)", Some(&attachments));
        assert!(
            html.contains("<img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"a\">"),
            "{html}"
        );
        assert!(html.contains("<img src=\"https://x.org/i.png\" alt=\"b\">"));
        assert!(!html.contains("javascript:") && !html.contains("text/html"));
    }

    #[test]
    fn underscores_inside_words_and_fenced_code_are_left_alone() {
        let html = markdown_to_html("use snake_case_name and _it_\n\n~~~\n**no** <b>\n~~~", None);
        assert!(html.contains("snake_case_name") && html.contains("<em>it</em>"), "{html}");
        assert!(html.contains("<pre><code>**no** &lt;b&gt;\n</code></pre>"), "{html}");
    }

    #[test]
    fn the_colour_table_maps_every_kind_of_terminal_colour() {
        assert_eq!(css_colour(Ansi::Red), "#c0392b");
        assert_eq!(css_colour(Ansi::Rgb(1, 2, 255)), "#0102ff");
        assert_eq!(css_colour(Ansi::Indexed(1)), "#c0392b");
        assert_eq!(css_colour(Ansi::Indexed(16)), "#000000");
        assert_eq!(css_colour(Ansi::Indexed(231)), "#ffffff");
        assert_eq!(css_colour(Ansi::Indexed(232)), "#080808");
        assert_eq!(strip_ansi("\u{1b}[31mx"), "x");
    }
}
