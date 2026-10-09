//! The text a notebook tab edits.
//!
//! A notebook tab holds one piece of plain text that contains every cell. Each cell starts with a
//! marker line, and the cell's source is every line after it up to the next marker. That makes every
//! editor command, search and agent tool work on a notebook without knowing what a notebook is.
//!
//! The markers are
//!
//! ```text
//! # %% id=4f2a9c01
//! # %% [markdown] id=77ab0c12
//! # %% [raw] id=0c3d99ab
//! ```
//!
//! The parser also accepts `#%%` and `[code]`, and ignores anything else on the marker line so that a
//! person can write `# %% Load the data`. Cells are separated by one newline, and no newline follows
//! the last cell. A source that ends with a newline therefore shows as an empty line before the next
//! marker, and the conversion back gives the same source.
//!
//! The editor normalises line endings to `\n` before this module sees the text.

use crate::nbformat::{new_id, Cell, CellKind, Notebook};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// The marker line for a cell, without a line ending.
pub fn marker(kind: CellKind, id: &str) -> String {
    match kind {
        CellKind::Code => format!("# %% id={id}"),
        CellKind::Markdown => format!("# %% [markdown] id={id}"),
        CellKind::Raw => format!("# %% [raw] id={id}"),
    }
}

/// What the parser finds on a marker line, with byte offsets inside the line so that a repair can
/// change the part that needs it and leave the rest of the line alone.
struct ParsedMarker {
    kind: CellKind,
    id: Option<String>,
    /// The end of `# %%` and of the kind tag if there is one.
    head_end: usize,
    /// Where the id value sits, when the line has one.
    id_range: Option<Range<usize>>,
}

/// Parses a marker line, or returns `None` when the line is not one.
fn parse_marker(line: &str) -> Option<ParsedMarker> {
    let prefix = if line.starts_with("# %%") {
        4
    } else if line.starts_with("#%%") {
        3
    } else {
        return None;
    };
    let (kind, head_end) = read_kind_tag(line, prefix);
    let (id, id_range) = read_id(line, head_end);
    Some(ParsedMarker { kind, id, head_end, id_range })
}

/// Reads the optional `[markdown]`, `[raw]` or `[code]` tag that follows the marker prefix. It
/// returns the kind, which is code when there is no tag, and the offset where the tag ends.
fn read_kind_tag(line: &str, prefix: usize) -> (CellKind, usize) {
    let rest = &line[prefix..];
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let after = &rest[spaces..];
    for (tag, kind) in
        [("[markdown]", CellKind::Markdown), ("[raw]", CellKind::Raw), ("[code]", CellKind::Code)]
    {
        if after.starts_with(tag) {
            return (kind, prefix + spaces + tag.len());
        }
    }
    (CellKind::Code, prefix)
}

/// Reads the optional `id=<id>` that follows the kind tag. An id is one to 64 characters from
/// letters, digits, `_` and `-`. Anything longer or empty counts as no id.
fn read_id(line: &str, from: usize) -> (Option<String>, Option<Range<usize>>) {
    let rest = &line[from..];
    let spaces = rest.len() - rest.trim_start_matches(' ').len();
    let Some(after) = rest[spaces..].strip_prefix("id=") else { return (None, None) };
    let length =
        after.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-').count();
    if length == 0 || length > 64 {
        return (None, None);
    }
    let start = from + spaces + 3;
    (Some(after[..length].to_string()), Some(start..start + length))
}

/// Whether a line is a cell marker.
pub fn is_marker(line: &str) -> bool {
    parse_marker(line).is_some()
}

/// The kind and the id (when it has one) written on a marker line, or `None` for any other line.
pub fn read_marker(line: &str) -> Option<(CellKind, Option<String>)> {
    parse_marker(line).map(|parsed| (parsed.kind, parsed.id))
}

/// The text of a whole notebook: for each cell its marker line and its source, with one newline
/// between cells and none after the last.
pub fn to_text(nb: &Notebook) -> String {
    let parts: Vec<String> = nb
        .cells
        .iter()
        .map(|cell| format!("{}\n{}", marker(cell.kind, &cell.id), cell.source))
        .collect();
    parts.join("\n")
}

/// Where one cell is in the text.
#[derive(Debug, Clone, PartialEq)]
pub struct CellSpan {
    /// The id on the marker line, or `None` when the marker has none or the cell has no marker.
    pub id: Option<String>,
    pub kind: CellKind,
    /// The zero based number of the marker line. It is `None` for text before the first marker, which
    /// counts as a code cell with no marker.
    pub marker: Option<usize>,
    /// The zero based numbers of the lines of the cell's source. It is empty when a marker is the last
    /// line of the text or is followed at once by another marker.
    pub body: Range<usize>,
    /// The bytes of the marker line, without its line ending. Empty for text before the first marker.
    pub marker_bytes: Range<usize>,
    /// The bytes of the cell's source. It leaves out the newline that separates the source from the
    /// next marker.
    pub body_bytes: Range<usize>,
}

/// The start and end of each line of the text, not counting the `\n`. Text with a final newline has an
/// empty last line, and empty text has one empty line.
fn line_bounds(text: &str) -> Vec<Range<usize>> {
    let mut bounds = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            bounds.push(start..index);
            start = index + 1;
        }
    }
    bounds.push(start..text.len());
    bounds
}

/// Finds every cell in the text in one pass. Text with no marker is one code cell, and empty text has
/// no cells. Text before the first marker is reported as a cell with no marker when there is any.
pub fn spans(text: &str) -> Vec<CellSpan> {
    if text.is_empty() {
        return Vec::new();
    }
    let lines = line_bounds(text);
    let mut found: Vec<(usize, ParsedMarker)> = Vec::new();
    for (number, bounds) in lines.iter().enumerate() {
        if let Some(parsed) = parse_marker(&text[bounds.clone()]) {
            found.push((number, parsed));
        }
    }
    let mut result = Vec::new();
    let first_marker = found.first().map_or(lines.len(), |(number, _)| *number);
    if first_marker > 0 {
        result.push(span_for(&lines, None, CellKind::Code, None, 0, first_marker));
    }
    for (index, (number, parsed)) in found.iter().enumerate() {
        let end = found.get(index + 1).map_or(lines.len(), |(next, _)| *next);
        result.push(span_for(
            &lines,
            Some(*number),
            parsed.kind,
            parsed.id.clone(),
            number + 1,
            end,
        ));
    }
    result
}

/// Builds the span of one cell from its first and one past its last body line (numbers of lines).
fn span_for(
    lines: &[Range<usize>],
    marker: Option<usize>,
    kind: CellKind,
    id: Option<String>,
    body_start: usize,
    body_end: usize,
) -> CellSpan {
    let marker_bytes = marker.map_or(0..0, |number| lines[number].clone());
    let start =
        if body_start < lines.len() { lines[body_start].start } else { lines[lines.len() - 1].end };
    let end = if body_end > body_start { lines[body_end - 1].end } else { start };
    CellSpan {
        id,
        kind,
        marker,
        body: body_start..body_end.max(body_start),
        marker_bytes,
        body_bytes: start..end,
    }
}

/// The source of a cell, as the text between its marker and the next.
pub fn source_of<'a>(text: &'a str, span: &CellSpan) -> &'a str {
    &text[span.body_bytes.clone()]
}

/// Builds the notebook the text now describes, using `previous` for everything the text does not hold.
///
/// Cells come in the order of the text. A cell whose id belongs to a cell of `previous` keeps that
/// cell's metadata, outputs, execution count, attachments and original JSON, and takes its source and
/// kind from the text. A code cell that became markdown or raw loses its outputs and execution count,
/// since only code cells have them. A cell with no id, or with an id an earlier cell already used,
/// gets a fresh one and starts with nothing but its source. The notebook level fields are those of
/// `previous`.
pub fn merge(text: &str, previous: &Notebook) -> Notebook {
    let known: HashMap<&str, &Cell> =
        previous.cells.iter().rev().map(|cell| (cell.id.as_str(), cell)).collect();
    let mut used: HashSet<String> = HashSet::new();
    let mut cells = Vec::new();
    for span in spans(text) {
        let wanted = span.id.clone().filter(|id| !used.contains(id));
        let id = wanted.unwrap_or_else(|| fresh_id(&used));
        used.insert(id.clone());
        let source = source_of(text, &span);
        cells.push(match known.get(id.as_str()) {
            Some(old) => revise_cell(old, span.kind, source),
            None => Cell::new(span.kind, &id, source),
        });
    }
    let mut notebook = previous.clone();
    notebook.cells = cells;
    notebook
}

/// The same as [`merge`], taking `previous` by value, and answering the cells of `previous` the text
/// no longer holds as well.
///
/// **What a notebook tab calls on every edit.** [`merge`] copies every cell it keeps, outputs and all,
/// and a notebook holding a few plots carries megabytes of base64 in its outputs — so a keystroke that
/// copied all of it was a keystroke that cost what the pictures weigh. This moves each cell instead.
/// The cells it answers are the ones an edit took out of the text, which the tab keeps so that an
/// undo, or `Z` in command mode, brings a deleted cell back with its outputs. `task-2220`.
pub fn merge_owned(text: &str, mut previous: Notebook) -> (Notebook, Vec<Cell>) {
    let mut known: HashMap<String, Cell> = HashMap::new();
    for cell in std::mem::take(&mut previous.cells) {
        // The first cell with an id wins, which is the rule `merge` keeps by reading backwards.
        known.entry(cell.id.clone()).or_insert(cell);
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut cells = Vec::new();
    for span in spans(text) {
        let wanted = span.id.clone().filter(|id| !used.contains(id));
        let id = wanted.unwrap_or_else(|| fresh_id(&used));
        used.insert(id.clone());
        let source = source_of(text, &span);
        cells.push(match known.remove(id.as_str()) {
            Some(old) => revise_owned(old, span.kind, source),
            None => Cell::new(span.kind, &id, source),
        });
    }
    previous.cells = cells;
    (previous, known.into_values().collect())
}

/// [`revise_cell`] on a cell that is being moved rather than copied.
fn revise_owned(mut cell: Cell, kind: CellKind, source: &str) -> Cell {
    if cell.source != source {
        cell.source = source.to_string();
    }
    if kind != cell.kind {
        let was_code = cell.kind == CellKind::Code;
        cell.kind = kind;
        if kind != CellKind::Code || !was_code {
            cell.outputs.clear();
            cell.execution_count = None;
        }
    }
    cell
}

/// A cell id that is not in the set. `new_id` is already unique within the process, so the loop only
/// guards against an id the text itself happens to contain.
fn fresh_id(used: &HashSet<String>) -> String {
    loop {
        let id = new_id();
        if !used.contains(&id) {
            return id;
        }
    }
}

/// A copy of an existing cell with the source and kind the text now gives it.
fn revise_cell(old: &Cell, kind: CellKind, source: &str) -> Cell {
    let mut cell = old.clone();
    cell.source = source.to_string();
    if kind != old.kind {
        cell.kind = kind;
        if kind != CellKind::Code || old.kind != CellKind::Code {
            cell.outputs.clear();
            cell.execution_count = None;
        }
    }
    cell
}

/// The edits that make every marker a proper one: each as a byte range of the text and the text that
/// replaces it, in order of position and not overlapping. They
///
/// - give a marker with no id a fresh one, rewriting the `# %%` and kind tag in the canonical form
///   and leaving the rest of the line;
/// - give the second and later markers that repeat an id a fresh id;
/// - put a code marker at the start when there is text before the first marker.
///
/// A marker that has an id and is not a duplicate is left as the person wrote it. The editor applies
/// the edits together, so a cell that an agent pastes or a bare `# %%` that a person types becomes a
/// real cell. The result is empty when nothing needs repairing.
pub fn repairs(text: &str) -> Vec<(Range<usize>, String)> {
    let mut edits = Vec::new();
    let mut used: HashSet<String> = spans(text).iter().filter_map(|span| span.id.clone()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    for span in spans(text) {
        let Some(line_number) = span.marker else {
            edits.push((0..0, format!("{}\n", marker(CellKind::Code, &fresh_id(&used)))));
            continue;
        };
        let _ = line_number;
        let line_start = span.marker_bytes.start;
        let parsed = parse_marker(&text[span.marker_bytes.clone()])
            .expect("a span with a marker line has a marker");
        match (&parsed.id, &parsed.id_range) {
            (Some(id), Some(range)) if !seen.insert(id.clone()) => {
                let id = fresh_id(&used);
                used.insert(id.clone());
                seen.insert(id.clone());
                edits.push((line_start + range.start..line_start + range.end, id));
            }
            (Some(_), _) => {}
            _ => {
                let id = fresh_id(&used);
                used.insert(id.clone());
                seen.insert(id.clone());
                edits.push((line_start..line_start + parsed.head_end, marker(parsed.kind, &id)));
            }
        }
    }
    edits
}

/// The index in `spans` of the cell a line belongs to. A marker line belongs to its own cell. It
/// returns `None` for a line past the end of the text.
pub fn cell_at(spans: &[CellSpan], line: usize) -> Option<usize> {
    let first_line = |span: &CellSpan| span.marker.unwrap_or(span.body.start);
    let index = spans.partition_point(|span| first_line(span) <= line).checked_sub(1)?;
    let span = &spans[index];
    let last_line = span.body.end.max(span.marker.map_or(0, |number| number + 1));
    (line < last_line).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbformat::{parse, serialize, Output};

    fn notebook_of(cells: &[(CellKind, &str, &str)]) -> Notebook {
        let mut notebook = crate::nbformat::empty();
        notebook.cells =
            cells.iter().map(|(kind, id, source)| Cell::new(*kind, id, source)).collect();
        notebook
    }

    #[test]
    fn merging_by_value_gives_what_merging_by_reference_does_and_hands_back_what_left() {
        let mut notebook = notebook_of(&[
            (CellKind::Code, "a", "1"),
            (CellKind::Code, "b", "2"),
            (CellKind::Markdown, "c", "# c"),
        ]);
        notebook.cells[1].outputs.push(Output::stream("stdout", "two\n"));
        notebook.cells[1].execution_count = Some(4);
        let text = "# %% id=c\n# c changed\n# %% id=a\n1";
        let by_reference = merge(text, &notebook);
        let (by_value, left) = merge_owned(text, notebook.clone());
        assert_eq!(serialize(&by_value), serialize(&by_reference));
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "b");
        assert_eq!(left[0].execution_count, Some(4), "what left keeps its outputs and count");
        // Putting the cell back brings its outputs back with it.
        let mut again = by_value.clone();
        again.cells.extend(left);
        let (restored, _) =
            merge_owned("# %% id=c\n# c changed\n# %% id=b\n2\n# %% id=a\n1", again);
        assert_eq!(restored.cells[1].outputs.len(), 1);
    }

    fn assert_round_trip(notebook: &Notebook) {
        let text = to_text(notebook);
        let merged = merge(&text, notebook);
        assert_eq!(serialize(&merged), serialize(notebook), "text was {text:?}");
        let found = spans(&text);
        assert_eq!(found.len(), notebook.cells.len(), "text was {text:?}");
        for (span, cell) in found.iter().zip(&notebook.cells) {
            assert_eq!(
                (span.id.as_deref(), span.kind, source_of(&text, span)),
                (Some(cell.id.as_str()), cell.kind, cell.source.as_str())
            );
        }
    }

    #[test]
    fn markers_are_written_in_the_canonical_form() {
        assert_eq!(marker(CellKind::Code, "4f2a9c01"), "# %% id=4f2a9c01");
        assert_eq!(marker(CellKind::Markdown, "77ab0c12"), "# %% [markdown] id=77ab0c12");
        assert_eq!(marker(CellKind::Raw, "x_y-1"), "# %% [raw] id=x_y-1");
    }

    #[test]
    fn marker_lines_are_read_in_every_accepted_form() {
        assert_eq!(read_marker("# %% id=ab"), Some((CellKind::Code, Some("ab".into()))));
        assert_eq!(read_marker("#%% id=ab"), Some((CellKind::Code, Some("ab".into()))));
        assert_eq!(
            read_marker("# %% [markdown] id=ab"),
            Some((CellKind::Markdown, Some("ab".into())))
        );
        assert_eq!(read_marker("# %% [raw]"), Some((CellKind::Raw, None)));
        assert_eq!(read_marker("# %% [code] id=ab"), Some((CellKind::Code, Some("ab".into()))));
        assert_eq!(read_marker("# %% Load the data"), Some((CellKind::Code, None)));
        assert_eq!(read_marker("#%%"), Some((CellKind::Code, None)));
        assert_eq!(read_marker("# % %"), None);
        assert_eq!(read_marker(" # %%"), None);
        assert!(is_marker("# %%") && !is_marker("x = 1"));
        let too_long = format!("# %% id={}", "a".repeat(65));
        assert_eq!(read_marker(&too_long), Some((CellKind::Code, None)));
    }

    #[test]
    fn a_source_with_two_lines_followed_by_another_cell_has_the_documented_text() {
        let notebook = notebook_of(&[(CellKind::Code, "x", "a\nb"), (CellKind::Code, "y", "c")]);
        assert_eq!(to_text(&notebook), "# %% id=x\na\nb\n# %% id=y\nc");
    }

    #[test]
    fn spans_report_lines_and_bytes() {
        let text = "# %% id=x\na\nb\n# %% [markdown] id=y\nc";
        let found = spans(text);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].marker, found[0].body.clone()), (Some(0), 1..3));
        assert_eq!(&text[found[0].marker_bytes.clone()], "# %% id=x");
        assert_eq!(source_of(text, &found[0]), "a\nb");
        assert_eq!(
            (found[1].marker, found[1].body.clone(), found[1].kind),
            (Some(3), 4..5, CellKind::Markdown)
        );
        assert_eq!(source_of(text, &found[1]), "c");
    }

    #[test]
    fn empty_cells_and_a_final_empty_cell_round_trip() {
        assert_round_trip(&notebook_of(&[
            (CellKind::Code, "a", ""),
            (CellKind::Code, "b", ""),
            (CellKind::Markdown, "c", ""),
        ]));
        assert_round_trip(&notebook_of(&[(CellKind::Code, "a", "x"), (CellKind::Code, "b", "")]));
        assert_round_trip(&notebook_of(&[(CellKind::Code, "a", "")]));
    }

    #[test]
    fn a_marker_that_is_the_last_line_has_an_empty_body() {
        let found = spans("# %% id=a\nx\n# %% id=b");
        assert_eq!(found[1].body, 3..3);
        assert_eq!(found[1].body_bytes, 21..21);
    }

    #[test]
    fn sources_ending_in_newlines_round_trip() {
        assert_round_trip(&notebook_of(&[
            (CellKind::Code, "a", "x\n"),
            (CellKind::Code, "b", "y\n\n"),
            (CellKind::Raw, "c", "\n"),
        ]));
        assert_eq!(
            to_text(&notebook_of(&[(CellKind::Code, "a", "x\n"), (CellKind::Code, "b", "")])),
            "# %% id=a\nx\n\n# %% id=b\n"
        );
    }

    #[test]
    fn unicode_round_trips_and_byte_ranges_are_in_bytes() {
        let notebook = notebook_of(&[
            (CellKind::Markdown, "a", "caf\u{e9} \u{4e2d}\u{6587} \u{1F600}"),
            (CellKind::Code, "b", "print('\u{fc}')"),
        ]);
        assert_round_trip(&notebook);
        let text = to_text(&notebook);
        let found = spans(&text);
        assert_eq!(&text[found[1].body_bytes.clone()], "print('\u{fc}')");
    }

    #[test]
    fn text_before_the_first_marker_is_an_implicit_code_cell() {
        let text = "import os\n\n# %% id=a\nx";
        let found = spans(text);
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].marker, found[0].id.clone(), found[0].kind),
            (None, None, CellKind::Code)
        );
        assert_eq!(source_of(text, &found[0]), "import os\n");
        assert_eq!(found[0].body, 0..2);
        let merged = merge(text, &notebook_of(&[(CellKind::Code, "a", "x")]));
        assert_eq!(merged.cells.len(), 2);
        assert_eq!(merged.cells[0].source, "import os\n");
        assert_eq!(merged.cells[1].id, "a");
    }

    #[test]
    fn text_with_no_marker_is_one_code_cell_and_empty_text_is_none() {
        let found = spans("x = 1\ny = 2");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].marker, None);
        assert!(spans("").is_empty());
        assert!(merge("", &crate::nbformat::empty()).cells.is_empty());
    }

    #[test]
    fn a_hash_percent_marker_without_a_space_is_a_marker() {
        let found = spans("#%% id=a\nx\n#%%[markdown] id=b\ny");
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].kind, CellKind::Markdown);
        assert_eq!(found[1].id.as_deref(), Some("b"));
    }

    #[test]
    fn duplicate_ids_give_the_second_cell_a_fresh_id_and_keep_the_first_cells_outputs() {
        let mut notebook = notebook_of(&[(CellKind::Code, "a", "1")]);
        notebook.cells[0].outputs.push(Output::stream("stdout", "out"));
        notebook.cells[0].execution_count = Some(3);
        let merged = merge("# %% id=a\none\n# %% id=a\ntwo", &notebook);
        assert_eq!(merged.cells.len(), 2);
        assert_eq!(
            (
                merged.cells[0].id.as_str(),
                merged.cells[0].outputs.len(),
                merged.cells[0].execution_count
            ),
            ("a", 1, Some(3))
        );
        assert_ne!(merged.cells[1].id, "a");
        assert!(merged.cells[1].outputs.is_empty());
        assert_eq!(merged.cells[1].source, "two");
    }

    #[test]
    fn a_marker_with_no_id_gets_a_fresh_id_in_merge() {
        let merged = merge("# %%\nx\n# %% [markdown]\ny", &crate::nbformat::empty());
        assert_eq!(merged.cells.len(), 2);
        assert_eq!(merged.cells[1].kind, CellKind::Markdown);
        assert_ne!(merged.cells[0].id, merged.cells[1].id);
    }

    #[test]
    fn changing_a_code_cell_to_markdown_drops_its_outputs_and_count() {
        let mut notebook = notebook_of(&[(CellKind::Code, "a", "1")]);
        notebook.cells[0].outputs.push(Output::stream("stdout", "out"));
        notebook.cells[0].execution_count = Some(3);
        let merged = merge("# %% [markdown] id=a\n1", &notebook);
        assert_eq!(merged.cells[0].kind, CellKind::Markdown);
        assert!(merged.cells[0].outputs.is_empty() && merged.cells[0].execution_count.is_none());
        let written = serialize(&merged);
        assert!(!written.contains("outputs") && !written.contains("execution_count"));
    }

    #[test]
    fn editing_one_cell_keeps_the_others_byte_identical() {
        let fixture = include_str!("../tests/fixtures/all_outputs.ipynb");
        let notebook = parse(fixture).unwrap();
        let text = to_text(&notebook);
        assert_eq!(serialize(&merge(&text, &notebook)), fixture);
        let edited = text.replacen("x = 1", "x = 2", 1);
        let written = serialize(&merge(&edited, &notebook));
        assert_eq!(written, fixture.replacen("\"x = 1\\n\"", "\"x = 2\\n\"", 1));
    }

    #[test]
    fn real_notebooks_survive_the_text_round_trip_byte_for_byte() {
        for fixture in [
            include_str!("../tests/fixtures/all_outputs.ipynb"),
            include_str!("../tests/fixtures/attachments.ipynb"),
            include_str!("../tests/fixtures/minor4_no_ids.ipynb"),
            include_str!("../tests/fixtures/executed.ipynb"),
            include_str!("../tests/fixtures/empty.ipynb"),
        ] {
            let notebook = parse(fixture).unwrap();
            assert_eq!(serialize(&merge(&to_text(&notebook), &notebook)), fixture);
        }
    }

    #[test]
    fn repairs_do_nothing_for_a_clean_text() {
        assert!(repairs("# %% id=a\nx\n# %% [markdown] id=b\ny").is_empty());
        assert!(repairs("").is_empty());
    }

    /// Applies edits from the last to the first so that earlier offsets stay valid.
    fn apply(text: &str, edits: &[(Range<usize>, String)]) -> String {
        let mut result = text.to_string();
        for (range, replacement) in edits.iter().rev() {
            result.replace_range(range.clone(), replacement);
        }
        result
    }

    #[test]
    fn repairs_give_a_bare_marker_an_id_and_keep_the_rest_of_the_line() {
        let text = "# %% Load the data\nx\n#%% [markdown]\ny";
        let repaired = apply(text, &repairs(text));
        let found = spans(&repaired);
        assert!(found.iter().all(|span| span.id.is_some()));
        assert!(repaired.contains(" Load the data\nx\n"));
        assert!(repaired.starts_with("# %% id="));
        assert_eq!(found[1].kind, CellKind::Markdown);
        assert!(repairs(&repaired).is_empty());
    }

    #[test]
    fn repairs_replace_the_second_of_a_duplicate_id() {
        let text = "# %% id=a\none\n# %% id=a\ntwo";
        let repaired = apply(text, &repairs(text));
        let found = spans(&repaired);
        assert_eq!(found[0].id.as_deref(), Some("a"));
        assert_ne!(found[1].id.as_deref(), Some("a"));
        assert_eq!(source_of(&repaired, &found[1]), "two");
    }

    #[test]
    fn repairs_put_a_code_marker_before_leading_text() {
        let text = "import os\n# %% id=a\nx";
        let repaired = apply(text, &repairs(text));
        let found = spans(&repaired);
        assert_eq!(found.len(), 2);
        assert!(found[0].marker == Some(0) && found[0].id.is_some());
        assert_eq!(source_of(&repaired, &found[0]), "import os");
        assert_eq!(repaired.matches("# %% id=").count(), 2);
    }

    #[test]
    fn cell_at_finds_the_cell_of_every_line() {
        let text = "pre\n# %% id=a\nx\ny\n# %% id=b\n# %% id=c\nz";
        let found = spans(text);
        let cells: Vec<Option<usize>> = (0..8).map(|line| cell_at(&found, line)).collect();
        assert_eq!(
            cells,
            vec![Some(0), Some(1), Some(1), Some(1), Some(2), Some(3), Some(3), None]
        );
    }

    /// A small deterministic generator, so the property test is the same on every run.
    struct Prng(u64);
    impl Prng {
        fn next(&mut self, limit: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % limit
        }
    }

    fn random_source(prng: &mut Prng) -> String {
        let pieces = [
            "",
            "x = 1",
            "\n",
            "\u{e9}\u{4e2d}",
            "# comment",
            "    indented",
            "\n\n",
            "#",
            "%",
            "print('a')\n",
        ];
        (0..prng.next(6))
            .map(|_| pieces[prng.next(pieces.len())])
            .collect::<Vec<&str>>()
            .join(if prng.next(2) == 0 { "" } else { "\n" })
    }

    fn random_notebook(prng: &mut Prng) -> Notebook {
        let mut notebook = crate::nbformat::empty();
        notebook.cells = (0..prng.next(7))
            .map(|_| {
                let kind = [CellKind::Code, CellKind::Markdown, CellKind::Raw][prng.next(3)];
                let mut cell = Cell::new(kind, &new_id(), &random_source(prng));
                if kind == CellKind::Code && prng.next(2) == 0 {
                    cell.execution_count = Some(prng.next(50) as u64);
                    cell.outputs.push(Output::stream("stdout", &random_source(prng)));
                }
                cell
            })
            .collect();
        notebook
    }

    #[test]
    fn random_notebooks_survive_the_text_round_trip() {
        let mut prng = Prng(42);
        for _ in 0..500 {
            let notebook = random_notebook(&mut prng);
            let text = to_text(&notebook);
            let merged = merge(&text, &notebook);
            assert_eq!(serialize(&merged), serialize(&notebook), "text was {text:?}");
            assert_eq!(to_text(&merged), text);
            assert!(repairs(&text).is_empty(), "text was {text:?}");
        }
    }
}
