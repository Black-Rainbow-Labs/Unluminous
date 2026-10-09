//! The `.ipynb` file: reading it, changing it, and writing it back.
//!
//! **A notebook that is opened and saved without a change is written back byte for byte.** People keep
//! notebooks in version control, and a program that rewrites every line of a file it was only asked to
//! open fills the history with noise. The `nbformat` package that Jupyter uses writes JSON with the keys
//! sorted, one space of indent, non ASCII text left as it is, a newline at the end, and every multi line
//! string split into a list of lines. This module writes exactly that.
//!
//! Three things make the round trip exact:
//!
//! - Every cell and the notebook itself keep the JSON object they were read from. A key this module does
//!   not know about is therefore written back, and a field that has not changed is written back as it
//!   was found (a `source` that was one string stays one string).
//! - The indent width and the final newline are remembered from the input.
//! - `serde_json` keeps object keys sorted here, which is the order `nbformat` writes them in.
//!
//! Only notebook format 4 is read. Versions 3 and older are a different layout that nothing writes any
//! more.

use serde_json::{json, Map, Value};
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// The three kinds of cell a notebook holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Code,
    Markdown,
    Raw,
}

impl CellKind {
    /// The word the `.ipynb` file uses for this kind in its `cell_type` field.
    pub fn name(&self) -> &'static str {
        match self {
            CellKind::Code => "code",
            CellKind::Markdown => "markdown",
            CellKind::Raw => "raw",
        }
    }

    /// The kind a `cell_type` word names, or `None` when the word is not one of the three.
    pub fn from_name(name: &str) -> Option<CellKind> {
        match name {
            "code" => Some(CellKind::Code),
            "markdown" => Some(CellKind::Markdown),
            "raw" => Some(CellKind::Raw),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Splitting and joining text the way nbformat does
// ---------------------------------------------------------------------------------------------

/// Whether Python's `str.splitlines` treats this character as the end of a line.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r'
            | '\x0b'
            | '\x0c'
            | '\x1c'
            | '\x1d'
            | '\x1e'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// Splits text into lines that keep their line endings, exactly as Python's `str.splitlines(True)`
/// does. `nbformat` writes multi line strings this way, so matching it, including the unusual line
/// separators, is what keeps a changed cell identical to what Jupyter would have written.
pub fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if !is_line_break(c) {
            continue;
        }
        let mut end = index + c.len_utf8();
        if c == '\r' {
            if let Some(&(next, '\n')) = chars.peek() {
                chars.next();
                end = next + 1;
            }
        }
        lines.push(text[start..end].to_string());
        start = end;
    }
    if start < text.len() {
        lines.push(text[start..].to_string());
    }
    lines
}

/// The JSON a multi line string is written as: a list of lines, which is `[]` for an empty string.
fn lines_value(text: &str) -> Value {
    Value::Array(split_lines(text).into_iter().map(Value::String).collect())
}

/// Reads a field that may be one string or a list of strings, and returns the text it holds.
fn join_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items
            .iter()
            .map(|item| item.as_str())
            .collect::<Option<Vec<&str>>>()
            .map(|parts| parts.concat()),
        _ => None,
    }
}

/// Whether a mime type holds JSON, which `nbformat` leaves as a JSON value instead of splitting it.
fn is_json_mime(mime: &str) -> bool {
    mime == "application/json" || (mime.starts_with("application/") && mime.ends_with("+json"))
}

/// Whether `nbformat` splits the string stored under this mime type into lines when it writes. Text
/// types, JavaScript and SVG are split. Pictures such as `image/png` are one long base64 string and
/// stay as one string.
fn is_split_mime(mime: &str) -> bool {
    mime.starts_with("text/") || mime == "application/javascript" || mime == "image/svg+xml"
}

/// Returns a mime bundle with the values `nbformat` splits turned into lists of lines.
fn split_bundle(data: Value) -> Value {
    let Value::Object(mut map) = data else { return data };
    for (mime, value) in map.iter_mut() {
        if let (true, Value::String(text)) = (is_split_mime(mime), &*value) {
            *value = lines_value(text);
        }
    }
    Value::Object(map)
}

// ---------------------------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------------------------

/// One output of a code cell.
///
/// This wraps the output's JSON object instead of naming its fields, so a key this module has never
/// heard of is kept and written back. An output always holds the form it is written in: text and
/// mime bundle strings are already split into lines as `nbformat` splits them, and the accessors join
/// them again.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    map: Map<String, Value>,
}

impl Output {
    /// A `stream` output, which is text a cell printed on `stdout` or `stderr`.
    pub fn stream(name: &str, text: &str) -> Output {
        let mut map = Map::new();
        map.insert("output_type".into(), json!("stream"));
        map.insert("name".into(), json!(name));
        map.insert("text".into(), lines_value(text));
        Output { map }
    }

    /// A `display_data` output, which is a rich result such as a picture shown while the cell ran.
    pub fn display_data(data: Value, metadata: Value) -> Output {
        let mut map = Map::new();
        map.insert("output_type".into(), json!("display_data"));
        map.insert("data".into(), split_bundle(data));
        map.insert("metadata".into(), metadata);
        Output { map }
    }

    /// An `execute_result` output, which is the value of the last expression in the cell.
    pub fn execute_result(execution_count: Option<u64>, data: Value, metadata: Value) -> Output {
        let mut map = Map::new();
        map.insert("output_type".into(), json!("execute_result"));
        map.insert(
            "execution_count".into(),
            execution_count.map_or(Value::Null, |count| json!(count)),
        );
        map.insert("data".into(), split_bundle(data));
        map.insert("metadata".into(), metadata);
        Output { map }
    }

    /// An `error` output, which is the exception that stopped the cell. The traceback lines usually
    /// contain terminal colour codes.
    pub fn error(ename: &str, evalue: &str, traceback: Vec<String>) -> Output {
        let mut map = Map::new();
        map.insert("output_type".into(), json!("error"));
        map.insert("ename".into(), json!(ename));
        map.insert("evalue".into(), json!(evalue));
        map.insert(
            "traceback".into(),
            Value::Array(traceback.into_iter().map(Value::String).collect()),
        );
        Output { map }
    }

    /// Wraps an output read from a file or a kernel message. It returns `None` when the value is not an
    /// object with an `output_type` string.
    pub fn from_value(value: Value) -> Option<Output> {
        match value {
            Value::Object(map) if map.get("output_type").is_some_and(Value::is_string) => {
                Some(Output { map })
            }
            _ => None,
        }
    }

    /// The output as a JSON value, including anything that is not written to a file such as `transient`.
    pub fn to_value(&self) -> Value {
        Value::Object(self.map.clone())
    }

    /// The output as it is written to a file. The `transient` key is left out because it belongs to
    /// the running kernel session and the notebook format has no place for it.
    fn to_file_value(&self) -> Value {
        let mut map = self.map.clone();
        map.remove("transient");
        Value::Object(map)
    }

    /// The `output_type`: `stream`, `display_data`, `execute_result` or `error`.
    pub fn output_type(&self) -> &str {
        self.map.get("output_type").and_then(Value::as_str).unwrap_or("")
    }

    /// The stream a `stream` output was printed on, `stdout` or `stderr`.
    pub fn stream_name(&self) -> Option<&str> {
        self.map.get("name").and_then(Value::as_str)
    }

    /// The text of a `stream` output, with a list of lines joined back into one string.
    pub fn text(&self) -> Option<String> {
        self.map.get("text").and_then(join_value)
    }

    /// The mime bundle of a `display_data` or `execute_result` output.
    pub fn data(&self) -> Option<&Map<String, Value>> {
        self.map.get("data").and_then(Value::as_object)
    }

    /// The text stored under one mime type, with a list of lines joined into one string. It returns
    /// `None` when the type is absent or holds JSON, which is not text.
    pub fn mime_text(&self, mime: &str) -> Option<String> {
        if is_json_mime(mime) {
            return None;
        }
        self.data()?.get(mime).and_then(join_value)
    }

    /// The exception name of an `error` output, such as `ZeroDivisionError`.
    pub fn ename(&self) -> Option<&str> {
        self.map.get("ename").and_then(Value::as_str)
    }

    /// The exception message of an `error` output.
    pub fn evalue(&self) -> Option<&str> {
        self.map.get("evalue").and_then(Value::as_str)
    }

    /// The traceback lines of an `error` output, or an empty list for any other output.
    pub fn traceback(&self) -> Vec<String> {
        let Some(Value::Array(lines)) = self.map.get("traceback") else { return Vec::new() };
        lines.iter().filter_map(|line| line.as_str().map(str::to_string)).collect()
    }

    /// Adds text to a `stream` output. A kernel sends one message for each burst of printing, and a
    /// notebook shows consecutive messages on the same stream as one block, so the new text is joined
    /// to the old and the whole is split into lines again.
    pub fn append_stream_text(&mut self, text: &str) {
        let joined = format!("{}{}", self.text().unwrap_or_default(), text);
        self.map.insert("text".into(), lines_value(&joined));
    }

    /// The `display_id` the kernel gave this output, which a later `update_display_data` message uses
    /// to find it. It is kept under `transient`, which is never written to the file.
    pub fn display_id(&self) -> Option<&str> {
        self.map.get("transient")?.get("display_id")?.as_str()
    }

    /// Remembers the `display_id` of this output so that a later update can find it.
    pub fn set_display_id(&mut self, display_id: &str) {
        self.map.insert("transient".into(), json!({ "display_id": display_id }));
    }

    /// Replaces the data and metadata of a `display_data` or `execute_result` output, which is what
    /// an `update_display_data` message asks for.
    pub fn set_data(&mut self, data: Value, metadata: Value) {
        self.map.insert("data".into(), split_bundle(data));
        self.map.insert("metadata".into(), metadata);
    }
}

// ---------------------------------------------------------------------------------------------
// Cell
// ---------------------------------------------------------------------------------------------

/// One cell of a notebook.
///
/// The public fields are what a program changes. The private fields hold the JSON object the cell was
/// read from, so that the keys this module does not know about, and the fields that were not changed,
/// are written back as they were.
#[derive(Debug, Clone)]
pub struct Cell {
    pub id: String,
    pub kind: CellKind,
    pub source: String,
    pub metadata: Value,
    pub execution_count: Option<u64>,
    pub outputs: Vec<Output>,
    pub attachments: Option<Value>,
    original: Map<String, Value>,
    had_id: bool,
}

impl Cell {
    /// The cell's tags, from `metadata.tags`, in the order the file has them.
    pub fn tags(&self) -> Vec<String> {
        self.metadata["tags"]
            .as_array()
            .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// Set the cell's tags. An empty list takes `metadata.tags` away, as Jupyter does.
    pub fn set_tags(&mut self, tags: &[String]) {
        if !self.metadata.is_object() {
            self.metadata = json!({});
        }
        let Some(map) = self.metadata.as_object_mut() else { return };
        match tags.is_empty() {
            true => map.remove("tags"),
            false => map.insert("tags".to_owned(), json!(tags)),
        };
    }

    /// A new cell with the given kind, id and source, and nothing else. It is written with an `id`
    /// only into a notebook whose minor version is 5 or more, because older versions do not allow one.
    pub fn new(kind: CellKind, id: &str, source: &str) -> Cell {
        Cell {
            id: id.to_string(),
            kind,
            source: source.to_string(),
            metadata: json!({}),
            execution_count: None,
            outputs: Vec::new(),
            attachments: None,
            original: Map::new(),
            had_id: false,
        }
    }

    /// Reads one cell object. A cell with no `id` is given a fresh one, and remembers that it had none.
    fn from_value(value: Value) -> Result<Cell, String> {
        let Value::Object(original) = value else {
            return Err("A cell in this notebook is not a JSON object.".to_string());
        };
        let kind_name = original.get("cell_type").and_then(Value::as_str).unwrap_or("");
        let kind = CellKind::from_name(kind_name)
            .ok_or_else(|| format!("A cell has the unknown type \"{kind_name}\"."))?;
        let stored_id = original.get("id").and_then(Value::as_str).map(str::to_string);
        let mut outputs = Vec::new();
        if let Some(Value::Array(items)) = original.get("outputs") {
            for item in items {
                outputs.push(
                    Output::from_value(item.clone())
                        .ok_or("A cell has an output with no output_type.")?,
                );
            }
        }
        Ok(Cell {
            had_id: stored_id.is_some(),
            id: stored_id.unwrap_or_else(new_id),
            kind,
            source: original.get("source").and_then(join_value).unwrap_or_default(),
            metadata: original.get("metadata").cloned().unwrap_or_else(|| json!({})),
            execution_count: original.get("execution_count").and_then(Value::as_u64),
            outputs,
            attachments: original.get("attachments").cloned(),
            original,
        })
    }

    /// The cell as the JSON object to write. It starts from the object the cell was read from and
    /// replaces each field with the current value. `source` is only re-encoded when its text differs
    /// from the original, so a source that was stored as a single string stays that way.
    fn to_value(&self, nbformat_minor: u64) -> Value {
        let mut map = self.original.clone();
        map.insert("cell_type".into(), json!(self.kind.name()));
        if self.had_id || nbformat_minor >= 5 {
            map.insert("id".into(), json!(self.id));
        }
        let unchanged =
            map.get("source").and_then(join_value).is_some_and(|text| text == self.source);
        if !unchanged {
            map.insert("source".into(), lines_value(&self.source));
        }
        map.insert("metadata".into(), self.metadata.clone());
        self.put_kind_fields(&mut map);
        Value::Object(map)
    }

    /// Sets the fields that depend on the kind: a code cell always has `execution_count` and
    /// `outputs`, a markdown or raw cell has neither, and `attachments` is written when there are any.
    fn put_kind_fields(&self, map: &mut Map<String, Value>) {
        if self.kind == CellKind::Code {
            map.insert(
                "execution_count".into(),
                self.execution_count.map_or(Value::Null, |count| json!(count)),
            );
            map.insert(
                "outputs".into(),
                Value::Array(self.outputs.iter().map(Output::to_file_value).collect()),
            );
        } else {
            map.remove("execution_count");
            map.remove("outputs");
        }
        match &self.attachments {
            Some(attachments) => map.insert("attachments".into(), attachments.clone()),
            None => map.remove("attachments"),
        };
    }
}

// ---------------------------------------------------------------------------------------------
// Notebook
// ---------------------------------------------------------------------------------------------

/// A notebook: its cells and its format version, with whatever else the file held kept for writing.
#[derive(Debug, Clone)]
pub struct Notebook {
    pub nbformat: u64,
    pub nbformat_minor: u64,
    pub metadata: Value,
    pub cells: Vec<Cell>,
    original: Map<String, Value>,
    indent: usize,
    trailing_newline: bool,
}

/// Reads the text of an `.ipynb` file. Only notebook format 4 is read, which covers 4.0 to 4.5, and
/// the error says so for any other version.
pub fn parse(json: &str) -> Result<Notebook, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("This file is not valid JSON: {error}."))?;
    let Value::Object(original) = value else {
        return Err("This file is JSON but it is not a notebook.".to_string());
    };
    let nbformat = original
        .get("nbformat")
        .and_then(Value::as_u64)
        .ok_or("This file has no notebook format version, so it is not a notebook.")?;
    if nbformat != 4 {
        return Err(format!(
            "Only notebook format version 4 is read, and this file is version {nbformat}."
        ));
    }
    let mut cells = Vec::new();
    if let Some(Value::Array(items)) = original.get("cells") {
        for item in items {
            cells.push(Cell::from_value(item.clone())?);
        }
    }
    Ok(Notebook {
        nbformat,
        nbformat_minor: original.get("nbformat_minor").and_then(Value::as_u64).unwrap_or(0),
        metadata: original.get("metadata").cloned().unwrap_or_else(|| json!({})),
        cells,
        original,
        indent: detect_indent(json),
        trailing_newline: json.ends_with('\n'),
    })
}

/// The number of spaces the file indents its first level with. A file that is not indented at all, or
/// has an empty top level object, gets the one space that Jupyter writes.
fn detect_indent(json: &str) -> usize {
    let Some(rest) = json.trim_start().strip_prefix('{') else { return 1 };
    let Some(rest) = rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')) else {
        return 1;
    };
    let spaces = rest.chars().take_while(|c| *c == ' ').count();
    if rest[spaces..].starts_with('"') {
        spaces
    } else {
        1
    }
}

/// A new notebook as Jupyter creates one: format 4.5, a Python 3 kernel, and one empty code cell.
pub fn empty() -> Notebook {
    empty_for(Language::Python)
}

/// A language a new notebook can be made for, which decides the kernel it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Python, run by ipykernel's `python3` kernel.
    Python,
    /// Rust, run by evcxr, whose kernelspec `evcxr_jupyter --install` registers as `rust`. `task-2229`.
    Rust,
}

impl Language {
    /// The language a word names, such as `rust` or `python`, in any case. `None` for one Unluminous
    /// cannot make a notebook for.
    pub fn parse(word: &str) -> Option<Language> {
        match word.trim().to_ascii_lowercase().as_str() {
            "python" | "py" | "python3" => Some(Language::Python),
            "rust" | "rs" => Some(Language::Rust),
            _ => None,
        }
    }
}

/// A new notebook for `language`: format 4.5, the kernel that runs the language, and one empty code
/// cell. The metadata is what Jupyter itself writes for that kernel, so the notebook opens with the
/// right kernel in Jupyter, VS Code and PyCharm as well.
pub fn empty_for(language: Language) -> Notebook {
    let metadata = match language {
        Language::Python => json!({
            "kernelspec": { "display_name": "Python 3 (ipykernel)", "language": "python", "name": "python3" },
            "language_info": { "name": "python" }
        }),
        Language::Rust => json!({
            "kernelspec": { "display_name": "Rust", "language": "rust", "name": "rust" },
            "language_info": {
                "codemirror_mode": "rust",
                "file_extension": ".rs",
                "mimetype": "text/rust",
                "name": "Rust",
                "pygment_lexer": "rust",
                "version": ""
            }
        }),
    };
    Notebook {
        nbformat: 4,
        nbformat_minor: 5,
        metadata,
        cells: vec![Cell::new(CellKind::Code, &new_id(), "")],
        original: Map::new(),
        indent: 1,
        trailing_newline: true,
    }
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

/// Writes a notebook as the text of an `.ipynb` file. A notebook read from a file written by
/// `nbformat` and not changed comes back identical to that file.
pub fn serialize(nb: &Notebook) -> String {
    let mut map = nb.original.clone();
    map.insert("nbformat".into(), json!(nb.nbformat));
    map.insert("nbformat_minor".into(), json!(nb.nbformat_minor));
    map.insert("metadata".into(), nb.metadata.clone());
    map.insert(
        "cells".into(),
        Value::Array(nb.cells.iter().map(|cell| cell.to_value(nb.nbformat_minor)).collect()),
    );
    let mut text = String::new();
    write_pretty(&Value::Object(map), nb.indent, 0, &mut text);
    if nb.trailing_newline {
        text.push('\n');
    }
    text
}

/// Appends a line break followed by the indent for the given depth.
fn push_line_start(out: &mut String, indent: usize, depth: usize) {
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', indent * depth));
}

/// Appends a value the way Python's `json.dumps(indent=n, sort_keys=True, ensure_ascii=False)` writes
/// it: a comma at the end of each line, `": "` after a key, and `[]` or `{}` for an empty list or
/// object. It is written here rather than by `serde_json`'s pretty printer because that printer
/// can only be reached through the `serde` crate, which this crate does not otherwise need. Objects
/// iterate in sorted key order.
fn write_pretty(value: &Value, indent: usize, depth: usize, out: &mut String) {
    match value {
        Value::Array(items) if !items.is_empty() => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                push_line_start(out, indent, depth + 1);
                write_pretty(item, indent, depth + 1, out);
            }
            push_line_start(out, indent, depth);
            out.push(']');
        }
        Value::Object(map) if !map.is_empty() => {
            out.push('{');
            for (position, (key, item)) in map.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                push_line_start(out, indent, depth + 1);
                out.push_str(&Value::String(key.clone()).to_string());
                out.push_str(": ");
                write_pretty(item, indent, depth + 1, out);
            }
            push_line_start(out, indent, depth);
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------------------------

/// A new cell id: eight lowercase hexadecimal characters. It is built from the clock, the process id
/// and a counter, mixed through the standard library's randomly keyed hasher, and it is checked
/// against every id this process has already handed out, so two calls never return the same id.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    static ISSUED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let issued = ISSUED.get_or_init(|| Mutex::new(HashSet::new()));
    let state = RandomState::new();
    loop {
        let mut hasher = state.build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        hasher.write_u32(std::process::id());
        if let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(elapsed.as_nanos());
        }
        let id = format!("{:08x}", hasher.finish() as u32);
        if issued.lock().map_or(true, |mut set| set.insert(id.clone())) {
            return id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_read_from_and_written_to_the_cells_metadata() {
        let mut cell = Cell::new(CellKind::Code, "a", "1");
        assert!(cell.tags().is_empty());
        cell.set_tags(&["parameters".to_owned(), "slow".to_owned()]);
        assert_eq!(cell.tags(), vec!["parameters", "slow"]);
        assert_eq!(cell.metadata, json!({ "tags": ["parameters", "slow"] }));
        cell.set_tags(&[]);
        assert_eq!(cell.metadata, json!({}));
    }

    const ALL_OUTPUTS: &str = include_str!("../tests/fixtures/all_outputs.ipynb");
    const ALL_OUTPUTS_CHANGED: &str = include_str!("../tests/fixtures/all_outputs_changed.ipynb");
    const ATTACHMENTS: &str = include_str!("../tests/fixtures/attachments.ipynb");
    const EMPTY: &str = include_str!("../tests/fixtures/empty.ipynb");
    const EXECUTED: &str = include_str!("../tests/fixtures/executed.ipynb");
    const MINOR_4: &str = include_str!("../tests/fixtures/minor4_no_ids.ipynb");

    #[test]
    fn a_notebook_with_every_output_type_is_written_back_byte_for_byte() {
        assert_eq!(serialize(&parse(ALL_OUTPUTS).unwrap()), ALL_OUTPUTS);
    }

    #[test]
    fn a_notebook_with_attachments_is_written_back_byte_for_byte() {
        assert_eq!(serialize(&parse(ATTACHMENTS).unwrap()), ATTACHMENTS);
    }

    #[test]
    fn the_notebook_jupyter_writes_for_a_new_file_is_written_back_byte_for_byte() {
        assert_eq!(serialize(&parse(EMPTY).unwrap()), EMPTY);
    }

    #[test]
    fn a_notebook_that_was_really_executed_is_written_back_byte_for_byte() {
        assert_eq!(serialize(&parse(EXECUTED).unwrap()), EXECUTED);
    }

    #[test]
    fn an_older_notebook_with_no_cell_ids_is_written_back_without_adding_any() {
        let notebook = parse(MINOR_4).unwrap();
        assert!(notebook.cells.iter().all(|cell| cell.id.len() == 8));
        assert_eq!(serialize(&notebook), MINOR_4);
    }

    #[test]
    fn a_changed_source_is_written_as_nbformat_writes_it() {
        let mut notebook = parse(ALL_OUTPUTS).unwrap();
        notebook.cells[1].source = "print('changed \u{fc}')\n\nz = 2".to_string();
        notebook.cells[0].source = "new markdown".to_string();
        notebook.cells[2].source = "# was empty".to_string();
        assert_eq!(serialize(&notebook), ALL_OUTPUTS_CHANGED);
    }

    #[test]
    fn a_source_changed_and_changed_back_is_still_written_unchanged() {
        let mut notebook = parse(ALL_OUTPUTS).unwrap();
        let original = notebook.cells[1].source.clone();
        notebook.cells[1].source.push('x');
        notebook.cells[1].source = original;
        assert_eq!(serialize(&notebook), ALL_OUTPUTS);
    }

    #[test]
    fn the_four_output_types_are_read_with_their_fields() {
        let notebook = parse(ALL_OUTPUTS).unwrap();
        let stream = &notebook.cells[1].outputs[0];
        assert_eq!((stream.output_type(), stream.stream_name()), ("stream", Some("stdout")));
        assert_eq!(stream.text().unwrap(), "h\u{e9}llo\nsecond\n");
        let result = &notebook.cells[5].outputs[0];
        assert_eq!(result.output_type(), "execute_result");
        assert_eq!(result.mime_text("text/plain").unwrap(), "2");
        let display = &notebook.cells[6].outputs[0];
        assert!(display.mime_text("image/png").unwrap().starts_with("iVBOR"));
        assert_eq!(display.mime_text("text/plain").unwrap(), "<Figure size 4x4>\nmore");
        let error = &notebook.cells[8].outputs[0];
        assert_eq!(
            (error.ename(), error.evalue()),
            (Some("ZeroDivisionError"), Some("division by zero"))
        );
        assert_eq!(error.traceback().len(), 4);
        assert!(error.traceback()[0].contains('\u{1b}'));
    }

    #[test]
    fn application_json_stays_a_json_value_and_has_no_text() {
        let notebook = parse(ALL_OUTPUTS).unwrap();
        let output = &notebook.cells[9].outputs[0];
        assert!(output.data().unwrap()["application/json"].is_object());
        assert_eq!(output.mime_text("application/json"), None);
    }

    #[test]
    fn constructed_outputs_are_split_into_lines_like_nbformat_splits_them() {
        let output = Output::display_data(
            json!({"text/html": "<p>\na</p>", "image/png": "AAAA", "application/json": {"a": 1}}),
            json!({}),
        );
        let data = output.data().unwrap();
        assert_eq!(data["text/html"], json!(["<p>\n", "a</p>"]));
        assert_eq!(data["image/png"], json!("AAAA"));
        assert_eq!(data["application/json"], json!({"a": 1}));
        assert_eq!(output.mime_text("text/html").unwrap(), "<p>\na</p>");
    }

    #[test]
    fn consecutive_stream_text_is_joined_into_one_output() {
        let mut output = Output::stream("stdout", "one\ntw");
        output.append_stream_text("o\nthree");
        assert_eq!(output.text().unwrap(), "one\ntwo\nthree");
        assert_eq!(output.to_value()["text"], json!(["one\n", "two\n", "three"]));
    }

    #[test]
    fn a_display_id_is_found_again_and_is_not_written_to_the_file() {
        let mut notebook = empty();
        let mut output = Output::display_data(json!({"text/plain": "a"}), json!({}));
        output.set_display_id("abc");
        assert_eq!(output.display_id(), Some("abc"));
        output.set_data(json!({"text/plain": "b"}), json!({}));
        assert_eq!(output.mime_text("text/plain").unwrap(), "b");
        notebook.cells[0].outputs.push(output);
        let written = serialize(&notebook);
        assert!(!written.contains("transient") && !written.contains("abc"));
    }

    #[test]
    fn outputs_with_unknown_keys_keep_them() {
        let value =
            json!({"output_type": "stream", "name": "stdout", "text": ["a"], "extra": {"k": 1}});
        let output = Output::from_value(value.clone()).unwrap();
        assert_eq!(output.to_value(), value);
        assert!(Output::from_value(json!({"name": "stdout"})).is_none());
    }

    #[test]
    fn notebook_format_three_is_refused_with_a_sentence() {
        let error =
            parse(r#"{"nbformat": 3, "nbformat_minor": 0, "worksheets": []}"#).err().unwrap();
        assert!(error.starts_with("Only notebook format version 4 is read"), "{error}");
    }

    #[test]
    fn text_that_is_not_a_notebook_is_refused() {
        assert!(parse("not json").is_err());
        assert!(parse("[1]").is_err());
        assert!(parse("{}").is_err());
    }

    #[test]
    fn a_new_rust_notebook_names_the_kernel_evcxr_registers() {
        let written = serialize(&empty_for(Language::Rust));
        let reread = parse(&written).unwrap();
        assert_eq!(reread.metadata["kernelspec"]["name"], "rust");
        assert_eq!(reread.metadata["kernelspec"]["language"], "rust");
        assert_eq!(reread.metadata["language_info"]["file_extension"], ".rs");
        assert_eq!(Language::parse(" Rust "), Some(Language::Rust));
        assert_eq!(Language::parse("python3"), Some(Language::Python));
        assert_eq!(Language::parse("julia"), None);
    }

    #[test]
    fn the_empty_notebook_is_what_jupyter_would_write() {
        let written = serialize(&empty());
        let reread = parse(&written).unwrap();
        assert_eq!((reread.nbformat, reread.nbformat_minor, reread.cells.len()), (4, 5, 1));
        assert_eq!(reread.cells[0].kind, CellKind::Code);
        assert_eq!(reread.metadata["kernelspec"]["name"], "python3");
        assert!(
            written.ends_with("}\n")
                && written.contains("\n \"cells\": [\n  {\n   \"cell_type\": \"code\",")
        );
        assert!(written.contains("\"outputs\": [],") && written.contains("\"source\": []"));
        assert_eq!(serialize(&reread), written);
    }

    #[test]
    fn markdown_and_raw_cells_get_no_execution_count_or_outputs() {
        let mut notebook = parse(EMPTY).unwrap();
        notebook.cells[0].kind = CellKind::Markdown;
        notebook.cells[0].source = "text".to_string();
        let written = serialize(&notebook);
        assert!(!written.contains("execution_count") && !written.contains("outputs"));
        notebook.cells[0].kind = CellKind::Code;
        assert!(serialize(&notebook).contains("\"execution_count\": null"));
    }

    #[test]
    fn indent_width_and_a_missing_final_newline_are_kept() {
        let text = "{\n    \"cells\": [],\n    \"metadata\": {},\n    \"nbformat\": 4,\n    \"nbformat_minor\": 5\n}";
        assert_eq!(serialize(&parse(text).unwrap()), text);
    }

    #[test]
    fn splitting_lines_matches_python_splitlines() {
        assert_eq!(split_lines(""), Vec::<String>::new());
        assert_eq!(split_lines("a\n"), vec!["a\n"]);
        assert_eq!(split_lines("a\n\nb"), vec!["a\n", "\n", "b"]);
        assert_eq!(split_lines("a\r\nb\rc\u{2028}d"), vec!["a\r\n", "b\r", "c\u{2028}", "d"]);
    }

    #[test]
    fn new_ids_are_eight_lowercase_hex_characters_and_never_repeat() {
        let ids: Vec<String> = (0..2000).map(|_| new_id()).collect();
        let unique: HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ids.iter().all(|id| id.len() == 8
            && id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))));
    }

    #[test]
    fn cell_kind_names_round_trip() {
        for kind in [CellKind::Code, CellKind::Markdown, CellKind::Raw] {
            assert_eq!(CellKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(CellKind::from_name("other"), None);
    }
}
