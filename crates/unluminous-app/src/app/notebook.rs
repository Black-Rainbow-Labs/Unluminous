//! A Jupyter notebook tab: the cells its text describes, which cell is chosen, what has run, and every
//! operation on whole cells.
//!
//! **A notebook tab edits one `Document`**, holding every cell behind a marker line, which is the
//! decision `tasks/task-2220-jupyter-notebooks-tdd.md` §1 records. So a cell operation here is never a
//! change to a list of cells: it is one `Command::ReplaceMany` on the text, which makes it one undo
//! step, and the cells are read back out of the text on the next frame by
//! `unluminous_jupyter::text::merge`. Outputs, execution counts and metadata are not text, and they
//! live in [`NotebookTab::model`], found again by each cell's id after any edit.
//!
//! The kernel half, running cells and hearing back from the kernel, is `app::notebook_kernel`. The
//! drawing is `components::notebook_view`, and the glue between the two and the editing area is
//! `app::notebook_frame`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use unluminous_core::{Command, Document};
use unluminous_jupyter::kernel::{Kernel, Variable};
use unluminous_jupyter::nbformat::{self, CellKind, Notebook};
use unluminous_jupyter::text::{self, CellSpan};

use crate::components::notebook_view::Drawn;

/// Whether the keys type into a cell or act on whole cells, which is Jupyter's own pair of modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The caret is in a cell and the keys type.
    #[default]
    Edit,
    /// Cells are chosen, from `anchor` to `head` either way round, and a letter is a command.
    Command { anchor: usize, head: usize },
}

/// What has happened to one cell's last run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Run {
    /// Waiting its turn behind another cell.
    Queued,
    /// Sent to the kernel at `since`, and not finished.
    Running { since: Instant },
    /// Finished: whether it raised, how long it took, and when it ended.
    Done { ok: bool, took: Duration, at: SystemTime },
    /// Not run, because a cell before it in the same run failed or the run was interrupted.
    Skipped,
}

/// The kernel behind a notebook, or why there is none.
pub enum KernelSlot {
    /// Nothing has been run yet, and no kernel is started until something is.
    NotStarted,
    /// A kernel has been started. It may still be starting, or have died: [`Kernel::state`] says.
    Live(Box<Kernel>),
    /// No kernel could be started. `missing` names the Python package that was not there, which is
    /// what the offer to install it is about.
    Failed { reason: String, missing: Option<String> },
}

/// An `input()` the kernel is waiting on, and what has been typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub cell: String,
    pub prompt: String,
    pub password: bool,
    pub typed: String,
}

/// The cell that is running now, and the kernel request it was sent as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub cell: String,
    pub request: String,
}

/// A completion the kernel was asked for: where the word being completed starts, and the names the
/// kernel answered with once it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub request: String,
    /// The byte the word starts at in the document, which is what one answer is good for: the
    /// answer is reused while the rest of the word is typed.
    pub word_start: usize,
    pub matches: Vec<String>,
    pub answered: bool,
}

/// A cell that was deleted, kept so `Z` in command mode can bring it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    /// Where it was, counting cells from the top.
    pub at: usize,
    /// Its marker and its source, as they were in the text.
    pub text: String,
}

/// One notebook tab's state beside its document. See the module comment.
pub struct NotebookTab {
    /// The notebook as of [`Self::merged_at`]: every cell's outputs, execution count and metadata.
    pub model: Notebook,
    /// The text revision `model` and `spans` were read from.
    pub merged_at: u64,
    /// Where each cell is in the text, by line and by byte, at the same revision.
    pub spans: Vec<CellSpan>,
    pub mode: Mode,
    /// What happened to each cell's last run, by cell id.
    pub runs: HashMap<String, Run>,
    /// The cells waiting to run, in order, by id.
    pub queue: VecDeque<String>,
    pub running: Option<Running>,
    /// Which cell each request a cell was run as belongs to, so output a kernel sends after the cell
    /// finished (a thread it started, a progress bar) still lands under the right cell.
    pub requests: HashMap<String, String>,
    pub kernel: KernelSlot,
    /// The Python the kernel is started with. `None` until one is chosen or found.
    pub python: Option<PathBuf>,
    /// The kernelspec to start, by name. `None` is the kernel the notebook's metadata names, or the
    /// Python's own.
    pub kernel_name: Option<String>,
    /// The Markdown cells open for editing. Every other Markdown cell is drawn rendered.
    pub editing: HashSet<String>,
    /// Cells whose source is collapsed to one line.
    pub collapsed: HashSet<String>,
    /// Cells whose outputs are collapsed to one line.
    pub outputs_collapsed: HashSet<String>,
    /// Cells whose traceback is shown in full rather than as its last line.
    pub tracebacks_open: HashSet<String>,
    /// How far each cell's own output is scrolled, when it is taller than the most it may be.
    pub output_scroll: HashMap<String, f32>,
    /// Bumped whenever anything that decides the room round a cell changes without the text
    /// changing: an output arriving, a Markdown cell rendered or opened, a cell collapsed. It is the
    /// layout's third key, beside the text and the folds. See `app::notebook_frame`.
    pub bands_revision: u64,
    /// Each cell's outputs, laid out for drawing at a width, by cell id.
    pub drawn: HashMap<String, Drawn>,
    /// Each rendered Markdown cell, laid out at a width, by cell id.
    pub rendered: HashMap<String, crate::app::notebook_frame::Rendered>,
    /// The room each cell was last given round its text, by cell id. See `app::notebook_frame`.
    pub rooms: HashMap<String, crate::app::notebook_frame::Room>,
    /// The column a cell's table is sorted by, and whether downwards, by cell id.
    pub sorts: HashMap<String, (usize, bool)>,
    /// Bumped per cell when its outputs change, which is what `drawn` is keyed on.
    pub output_revisions: HashMap<String, u64>,
    pub variables: Vec<Variable>,
    pub variables_showing: bool,
    /// The request the last variables question was sent as, so its answer is recognised.
    pub variables_request: Option<String>,
    pub waiting: Option<Waiting>,
    /// Cells told `clear_output(wait=True)`: their outputs are cleared when the next one arrives.
    pub clear_before_next: HashSet<String>,
    /// The cells deleted in command mode, newest last.
    pub deleted: Vec<Deleted>,
    /// Cells an edit took out of the text, outputs and all, offered back to the next merge so that an
    /// undo of the edit brings a cell back as it was.
    pub removed: Vec<unluminous_jupyter::nbformat::Cell>,
    /// The cells copied or cut in command mode, as marker and source text, which is what `V` pastes.
    pub clipboard: Vec<(CellKind, String)>,
    /// The first of a two key command (`D D`, `I I`, `0 0`) and when it was pressed, in egui seconds.
    pub first_key: Option<(egui::Key, f64)>,
    /// Line numbers drawn in the gutter, counted within each cell. `L` turns them off and on.
    pub line_numbers: bool,
    /// Set when a cell ran longer than the notice threshold and the notice has been given.
    pub long_run_noticed: bool,
    /// The cell the pointer is over, worked out while drawing and read by the next frame.
    pub hovered: Option<usize>,
    /// What the kernel was last asked to complete, and its answer once it has one. See
    /// `UnluminousApp::kernel_candidates`.
    pub asked: Option<Asked>,
    /// Debugging a cell, while it is being set up or once it has been. See `app::notebook_debug`.
    pub debugging: Option<crate::app::notebook_debug::CellDebug>,
    /// The number the gutter draws beside each paragraph: counted within its cell, and none beside a
    /// marker or a rendered cell. Worked out with the layout.
    pub numbers: Vec<Option<usize>>,
    /// The line breaks the file was written with, which it is written back with. Jupyter on Windows
    /// writes `\r\n`, and a notebook saved unchanged must come back byte for byte.
    pub line_ending: unluminous_core::LineEnding,
}

/// How long a cell may run before the notebook says so, which is the reference editor's default.
pub const LONG_RUN: Duration = Duration::from_secs(60);

/// How many removed cells a tab keeps for an undo to bring back.
pub const REMEMBERED_REMOVALS: usize = 64;

/// How long the second key of `D D`, `I I` and `0 0` may follow the first, in seconds.
pub const SECOND_KEY: f64 = 1.0;

impl NotebookTab {
    /// A tab over a notebook that has been read, and the text its document is to hold.
    pub fn new(model: Notebook) -> (NotebookTab, String) {
        let text = text::to_text(&model);
        let spans = text::spans(&text);
        let tab = NotebookTab {
            model,
            merged_at: 0,
            spans,
            mode: Mode::Edit,
            runs: HashMap::new(),
            queue: VecDeque::new(),
            running: None,
            requests: HashMap::new(),
            kernel: KernelSlot::NotStarted,
            python: None,
            kernel_name: None,
            editing: HashSet::new(),
            collapsed: HashSet::new(),
            outputs_collapsed: HashSet::new(),
            tracebacks_open: HashSet::new(),
            output_scroll: HashMap::new(),
            bands_revision: 1,
            drawn: HashMap::new(),
            rendered: HashMap::new(),
            rooms: HashMap::new(),
            sorts: HashMap::new(),
            output_revisions: HashMap::new(),
            variables: Vec::new(),
            variables_showing: false,
            variables_request: None,
            waiting: None,
            clear_before_next: HashSet::new(),
            deleted: Vec::new(),
            removed: Vec::new(),
            clipboard: Vec::new(),
            first_key: None,
            line_numbers: true,
            long_run_noticed: false,
            hovered: None,
            asked: None,
            debugging: None,
            numbers: Vec::new(),
            line_ending: unluminous_core::LineEnding::Lf,
        };
        (tab, text)
    }

    /// Read an `.ipynb` file's bytes into a tab, and the text its document is to hold.
    pub fn open(json: &str) -> Result<(NotebookTab, String), String> {
        nbformat::parse(json).map(NotebookTab::new)
    }

    /// Bring the cells up to date with the text, when the text has changed since they were read.
    ///
    /// Linear in the size of the text, and a notebook is small, so it is done on the frame the text
    /// changes rather than kept incremental.
    pub fn refresh(&mut self, document: &Document) {
        let revision = document.text_revision();
        if revision == self.merged_at {
            return;
        }
        let text = document.text().to_string();
        // The cells an earlier edit took out are offered back, so an undo brings one back with its
        // outputs; the model is moved rather than copied, because outputs can weigh megabytes.
        let mut previous = std::mem::replace(&mut self.model, nbformat::empty());
        previous.cells.append(&mut self.removed);
        let (model, removed) = text::merge_owned(&text, previous);
        self.model = model;
        self.removed = removed;
        // Bounded, so a session of edits does not keep every cell it ever deleted.
        if self.removed.len() > REMEMBERED_REMOVALS {
            let excess = self.removed.len() - REMEMBERED_REMOVALS;
            self.removed.drain(..excess);
        }
        self.spans = text::spans(&text);
        self.merged_at = revision;
        // A Markdown cell that has gone, or stopped being Markdown, is not being edited any more.
        let markdown: HashSet<&str> = self
            .model
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Markdown)
            .map(|cell| cell.id.as_str())
            .collect();
        self.editing.retain(|id| markdown.contains(id.as_str()));
    }

    /// How many cells there are.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// True when the notebook has no cells at all, which only an empty text gives.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The id of the cell at `index`, which the text always has once it has been repaired.
    pub fn id_of(&self, index: usize) -> Option<String> {
        self.model.cells.get(index).map(|cell| cell.id.clone())
    }

    /// Which cell has this id, counting from the top.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.model.cells.iter().position(|cell| cell.id == id)
    }

    /// Which cell a byte of the text belongs to. A marker belongs to the cell it starts.
    pub fn cell_at_offset(&self, offset: usize) -> Option<usize> {
        if self.spans.is_empty() {
            return None;
        }
        let found = self.spans.partition_point(|span| span_start(span) <= offset);
        Some(found.saturating_sub(1))
    }

    /// The cells the keys act on: the chosen range in command mode, or the caret's cell in edit mode.
    pub fn chosen(&self, caret: usize) -> Range<usize> {
        match self.mode {
            Mode::Command { anchor, head } => anchor.min(head)..anchor.max(head) + 1,
            Mode::Edit => match self.cell_at_offset(caret) {
                Some(cell) => cell..cell + 1,
                None => 0..0,
            },
        }
    }

    /// Note that a cell's outputs have changed, so it is drawn again and the room under it measured
    /// again.
    pub fn outputs_changed(&mut self, id: &str) {
        *self.output_revisions.entry(id.to_owned()).or_insert(0) += 1;
        self.bands_revision += 1;
    }

    /// The `.ipynb` file this tab writes, as its text stands now.
    pub fn serialize(&self, text: &str) -> String {
        let json = nbformat::serialize(&text::merge(text, &self.model));
        // Only the line breaks between the JSON's own lines are raw; a line break inside a cell is
        // written `\n` inside a string, so this cannot change a byte of anybody's source.
        self.line_ending.apply(&json).into_owned()
    }

    /// The kernel, when one has been started.
    pub fn kernel(&mut self) -> Option<&mut Kernel> {
        match &mut self.kernel {
            KernelSlot::Live(kernel) => Some(kernel),
            _ => None,
        }
    }
}

/// Where a cell starts in the text: its marker, or its body when it has none.
pub fn span_start(span: &CellSpan) -> usize {
    match span.marker {
        Some(_) => span.marker_bytes.start,
        None => span.body_bytes.start,
    }
}

/// Where a cell ends in the text: the end of its body, which is before the line break that
/// separates it from the next marker.
pub fn span_end(span: &CellSpan) -> usize {
    span.body_bytes.end
}

/// The edit that puts a new cell of `kind` holding `source` before cell `at`, or after the last cell
/// when `at` is the number of cells. Answers the edit and the offset the caret goes to, which is the
/// end of the new cell's source, in the text after it.
pub fn insert_cell(
    spans: &[CellSpan],
    text_len: usize,
    at: usize,
    kind: CellKind,
    id: &str,
    source: &str,
) -> (Vec<(Range<usize>, String)>, usize) {
    let marker = text::marker(kind, id);
    match spans.get(at) {
        // Before an existing cell: the marker, the new cell's source, and the line break the old
        // marker needs in front of it.
        Some(span) => {
            let start = span_start(span);
            let inserted = format!("{marker}\n{source}\n");
            (vec![(start..start, inserted)], start + marker.len() + 1 + source.len())
        }
        // After the last cell: a line break, then the marker, then the source that is its body.
        None if text_len > 0 => {
            let inserted = format!("\n{marker}\n{source}");
            (vec![(text_len..text_len, inserted.clone())], text_len + inserted.len())
        }
        None => {
            let inserted = format!("{marker}\n{source}");
            (vec![(0..0, inserted.clone())], inserted.len())
        }
    }
}

/// The bytes cells `range` take in the text, with the one line break that joins them to their
/// neighbours, so taking them out leaves the cells either side joined by one line break.
pub fn cells_bytes(spans: &[CellSpan], range: Range<usize>, text_len: usize) -> Range<usize> {
    let first = span_start(&spans[range.start]);
    match spans.get(range.end) {
        // A cell follows: up to its marker, so the separator before it goes with what is removed.
        Some(next) => first..span_start(next),
        // These are the last cells: from the separator before them to the end.
        None => first.saturating_sub(usize::from(first > 0))..text_len,
    }
}

/// The text of cells `range`, each as marker and source, joined the way the text joins cells.
pub fn cells_text(text: &str, spans: &[CellSpan], range: Range<usize>) -> String {
    range.map(|cell| cell_text(text, &spans[cell])).collect::<Vec<_>>().join("\n")
}

/// One cell's marker and source as they are in the text.
pub fn cell_text(text: &str, span: &CellSpan) -> String {
    text[span_start(span)..span_end(span)].to_owned()
}

/// The text of cells written again with fresh ids, so pasting a copy never gives two cells one id.
pub fn with_fresh_ids(cells: &[(CellKind, String)]) -> String {
    cells
        .iter()
        .map(|(kind, source)| format!("{}\n{}", text::marker(*kind, &nbformat::new_id()), source))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The kind and source of cells `range`, which is what a copy takes.
pub fn cells_copied(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
) -> Vec<(CellKind, String)> {
    range.map(|cell| (spans[cell].kind, text::source_of(text, &spans[cell]).to_owned())).collect()
}

/// The edit that turns cells `range` into one cell: the first cell's marker, then every source
/// joined by a line break. The ids of the others go, and their outputs with them.
pub fn merge_cells(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
) -> Option<(Range<usize>, String)> {
    if range.len() < 2 {
        return None;
    }
    let first = &spans[range.start];
    let marker = &text[first.marker_bytes.clone()];
    let sources: Vec<&str> =
        range.clone().map(|cell| text::source_of(text, &spans[cell])).collect();
    let replaced = span_start(first)..span_end(&spans[range.end - 1]);
    Some((replaced, format!("{marker}\n{}", sources.join("\n"))))
}

/// The edit that changes the kind of cells `range`, keeping each cell's id.
pub fn change_kind(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
    kind: CellKind,
) -> Vec<(Range<usize>, String)> {
    range
        .filter(|cell| spans[*cell].kind != kind)
        .filter_map(|cell| {
            let span = &spans[cell];
            span.marker?;
            let (_, id) = text::read_marker(&text[span.marker_bytes.clone()])?;
            Some((span.marker_bytes.clone(), text::marker(kind, &id?)))
        })
        .collect()
}

/// The edit that moves cells `range` one place up (`by` is -1) or down (`by` is 1). `None` when they
/// are already at that end.
pub fn move_cells(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
    by: i32,
) -> Option<(Range<usize>, String)> {
    let (outer, order): (Range<usize>, Vec<Range<usize>>) = match by {
        -1 if range.start > 0 => {
            let outer = range.start - 1..range.end;
            (outer, vec![range.clone(), range.start - 1..range.start])
        }
        1 if range.end < spans.len() => {
            let outer = range.start..range.end + 1;
            (outer, vec![range.end..range.end + 1, range.clone()])
        }
        _ => return None,
    };
    let replaced = span_start(&spans[outer.start])..span_end(&spans[outer.end - 1]);
    let rebuilt: Vec<String> =
        order.into_iter().map(|part| cells_text(text, spans, part)).collect();
    Some((replaced, rebuilt.join("\n")))
}

/// Apply a set of edits to a document as one undo step, then put the caret at `caret`.
pub fn apply_edits(
    document: &mut Document,
    edits: Vec<(Range<usize>, String)>,
    caret: Option<usize>,
) {
    if !edits.is_empty() {
        let mut ordered = edits;
        // `ReplaceMany` applies back to front and wants them in order; sorted here so a caller can
        // build them in whatever order reads best.
        ordered.sort_by_key(|(range, _)| range.start);
        document.apply(Command::ReplaceMany(ordered));
    }
    if let Some(offset) = caret {
        let offset = offset.min(document.text().len_bytes());
        document.apply(Command::PlaceCaret { offset, extend: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> (String, Vec<CellSpan>) {
        let text = "# %% id=a\none\n# %% [markdown] id=b\n# Two\n# %% id=c\nthree".to_owned();
        let spans = text::spans(&text);
        (text, spans)
    }

    fn applied(text: &str, edits: Vec<(Range<usize>, String)>) -> String {
        let mut document = Document::from_text(text);
        apply_edits(&mut document, edits, None);
        document.text().to_string()
    }

    #[test]
    fn a_cell_inserted_before_another_has_an_empty_line_of_its_own() {
        let (text, spans) = three();
        let (edits, caret) = insert_cell(&spans, text.len(), 1, CellKind::Code, "n", "");
        let after = applied(&text, edits);
        assert_eq!(
            after,
            "# %% id=a\none\n# %% id=n\n\n# %% [markdown] id=b\n# Two\n# %% id=c\nthree"
        );
        assert_eq!(&after[caret..caret + 1], "\n", "the caret is on the new cell's empty line");
    }

    #[test]
    fn a_cell_inserted_after_the_last_one_ends_the_text_with_its_empty_line() {
        let (text, spans) = three();
        let (edits, caret) = insert_cell(&spans, text.len(), 3, CellKind::Markdown, "n", "");
        let after = applied(&text, edits);
        assert!(after.ends_with("three\n# %% [markdown] id=n\n"));
        assert_eq!(caret, after.len());
        assert_eq!(text::spans(&after).len(), 4);
    }

    #[test]
    fn deleting_a_middle_cell_or_the_last_cell_leaves_the_others_joined_by_one_line_break() {
        let (text, spans) = three();
        let middle = applied(&text, vec![(cells_bytes(&spans, 1..2, text.len()), String::new())]);
        assert_eq!(middle, "# %% id=a\none\n# %% id=c\nthree");
        let last = applied(&text, vec![(cells_bytes(&spans, 2..3, text.len()), String::new())]);
        assert_eq!(last, "# %% id=a\none\n# %% [markdown] id=b\n# Two");
        let first = applied(&text, vec![(cells_bytes(&spans, 0..1, text.len()), String::new())]);
        assert_eq!(first, "# %% [markdown] id=b\n# Two\n# %% id=c\nthree");
    }

    #[test]
    fn moving_a_cell_up_and_down_swaps_it_with_its_neighbour() {
        let (text, spans) = three();
        let (range, moved) = move_cells(&text, &spans, 2..3, -1).expect("it can move up");
        let up = applied(&text, vec![(range, moved)]);
        assert_eq!(up, "# %% id=a\none\n# %% id=c\nthree\n# %% [markdown] id=b\n# Two");
        assert!(move_cells(&text, &spans, 0..1, -1).is_none(), "the first cell cannot go higher");
        let (range, moved) = move_cells(&text, &spans, 0..2, 1).expect("two cells move down");
        assert_eq!(
            applied(&text, vec![(range, moved)]),
            "# %% id=c\nthree\n# %% id=a\none\n# %% [markdown] id=b\n# Two"
        );
    }

    #[test]
    fn merging_keeps_the_first_marker_and_joins_the_sources() {
        let (text, spans) = three();
        let (range, merged) = merge_cells(&text, &spans, 0..2).expect("two cells merge");
        assert_eq!(
            applied(&text, vec![(range, merged)]),
            "# %% id=a\none\n# Two\n# %% id=c\nthree"
        );
    }

    #[test]
    fn changing_kind_rewrites_only_the_markers_and_keeps_the_ids() {
        let (text, spans) = three();
        let after = applied(&text, change_kind(&text, &spans, 0..3, CellKind::Markdown));
        assert_eq!(
            after,
            "# %% [markdown] id=a\none\n# %% [markdown] id=b\n# Two\n# %% [markdown] id=c\nthree"
        );
    }

    #[test]
    fn a_pasted_copy_never_shares_an_id_with_what_it_was_copied_from() {
        let (text, spans) = three();
        let copied = cells_copied(&text, &spans, 0..2);
        let pasted = with_fresh_ids(&copied);
        let ids: Vec<_> = text::spans(&pasted).into_iter().map(|span| span.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|id| id.as_deref() != Some("a") && id.as_deref() != Some("b")));
    }

    #[test]
    fn the_cell_at_an_offset_counts_a_marker_as_part_of_the_cell_it_starts() {
        let (text, _) = three();
        let (mut tab, _) = NotebookTab::new(nbformat::empty());
        tab.spans = text::spans(&text);
        assert_eq!(tab.cell_at_offset(0), Some(0));
        assert_eq!(tab.cell_at_offset(text.find("# %% [markdown]").unwrap()), Some(1));
        assert_eq!(tab.cell_at_offset(text.len()), Some(2));
    }
}
