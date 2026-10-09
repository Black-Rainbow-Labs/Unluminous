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

use crate::components::notebook_view::{Drawn, StatusMark};

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
    /// Finished: whether it raised, how long it took, and when it ended, also as the time of day on
    /// this machine's clock, which is what the status line shows.
    Done { ok: bool, took: Duration, at: SystemTime, clock: crate::services::clock::TimeOfDay },
    /// Not run, because a cell before it in the same run failed or the run was interrupted.
    Skipped,
}

/// The words on the line under a code cell: its count and how long it took, or what it is waiting
/// for. The reference editor's own form: `[5] 222ms`.
pub fn status_words(run: Option<&Run>, count: Option<u64>) -> String {
    let count = count.map(|count| format!("[{count}]")).unwrap_or_else(|| "[ ]".to_owned());
    match run {
        Some(Run::Queued) => "Queued".to_owned(),
        Some(Run::Running { since }) => format!("[*] {}", duration(since.elapsed())),
        Some(Run::Done { took, clock, .. }) => format!("{count} {} at {clock}", duration(*took)),
        Some(Run::Skipped) => {
            "Not run, because a cell before it failed or the run was stopped".to_owned()
        }
        None => count,
    }
}

/// The mark at the left of a code cell's status line for how its last run went.
pub fn status_mark(run: Option<&Run>) -> StatusMark {
    match run {
        Some(Run::Done { ok: true, .. }) => StatusMark::Succeeded,
        Some(Run::Done { ok: false, .. }) => StatusMark::Failed,
        Some(Run::Queued) => StatusMark::Queued,
        Some(Run::Running { .. }) => StatusMark::Running,
        Some(Run::Skipped) => StatusMark::Skipped,
        None => StatusMark::Nothing,
    }
}

/// A duration the way the reference editor writes one: `< 10 ms`, `222ms`, `5s 3ms`, `2m 5s`.
pub fn duration(took: Duration) -> String {
    let millis = took.as_millis();
    match millis {
        0..10 => "< 10 ms".to_owned(),
        10..1000 => format!("{millis}ms"),
        1000..60_000 => format!("{}s {}ms", millis / 1000, millis % 1000),
        _ => format!("{}m {}s", millis / 60_000, (millis / 1000) % 60),
    }
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
    /// The byte the cell's source starts at in the document, and the source as it was sent, which is
    /// what the kernel's range is counted in. `task-2229`.
    pub body_start: usize,
    pub source: String,
    /// The byte in the document the kernel's matches replace from, once it has answered. It is often
    /// not `word_start`: ipykernel answers `%ti` from the `%`. See
    /// `unluminous_jupyter::completion::fit`.
    pub kernel_start: usize,
    /// What had been typed of the word when the kernel was asked. Its answer is already narrowed to
    /// that, so it only stands while the word still starts with it.
    pub typed: String,
    pub matches: Vec<unluminous_jupyter::completion::Match>,
    pub answered: bool,
}

impl Asked {
    /// Whether this question still answers the word `stem` starting at `word_start`, given `before`,
    /// the cell's text from its start to that word as it is now. The same place is not enough: the
    /// text in front may have changed, and the kernel narrowed its answer to what was typed when it
    /// was asked, so `v.it` changed to `v.` would be offered only the two methods that match `it`.
    /// `task-2229`.
    pub fn is_about(&self, word_start: usize, before: &str, stem: &str) -> bool {
        self.word_start == word_start
            && word_start >= self.body_start
            && self.source.get(..word_start - self.body_start) == Some(before)
            && stem.starts_with(&self.typed)
    }
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
    /// The Markdown headings whose sections are collapsed, by cell id. Every cell after one of them, up
    /// to the next heading of the same level or above, is hidden.
    pub sections_collapsed: HashSet<String>,
    /// The cell being dragged by its handle, by id.
    pub dragging: Option<String>,
    /// The cell whose tags are being edited, by id, and the words typed so far.
    pub editing_tags: Option<(String, String)>,
    /// Set by Notebook Outline: the toolbar opens its outline on the next frame.
    pub outline_wanted: bool,
    /// Set by Restart and Run All: the cells are queued when the new kernel says it is ready.
    pub run_all_after_restart: bool,
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
            sections_collapsed: HashSet::new(),
            dragging: None,
            editing_tags: None,
            outline_wanted: false,
            run_all_after_restart: false,
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

    /// The level of the heading cell `cell` starts with: 1 for `#`, up to 6. `None` for a cell that is
    /// not a Markdown cell whose first line is a heading.
    pub fn heading_level(&self, cell: usize) -> Option<usize> {
        let found = self.model.cells.get(cell)?;
        if found.kind != CellKind::Markdown {
            return None;
        }
        let line = found.source.lines().find(|line| !line.trim().is_empty())?.trim_start();
        let level = line.chars().take_while(|letter| *letter == '#').count();
        ((1..=6).contains(&level) && line[level..].starts_with(' ')).then_some(level)
    }

    /// The cells of the section the heading cell `heading` starts: the heading and every cell after
    /// it up to the next heading of the same level or above. Just the heading for a cell that is not
    /// one.
    pub fn section_of(&self, heading: usize) -> std::ops::Range<usize> {
        let Some(level) = self.heading_level(heading) else { return heading..heading + 1 };
        let end = (heading + 1..self.len())
            .find(|cell| self.heading_level(*cell).is_some_and(|other| other <= level))
            .unwrap_or(self.len());
        heading..end
    }

    /// The heading whose section holds `cell`: the cell itself when it is a heading, or the nearest
    /// heading above it. `None` when no heading comes before it.
    pub fn heading_above(&self, cell: usize) -> Option<usize> {
        (0..=cell.min(self.len().saturating_sub(1)))
            .rev()
            .find(|above| self.heading_level(*above).is_some())
    }

    /// Whether `cell` is inside a collapsed section, and so is not drawn.
    pub fn in_a_collapsed_section(&self, cell: usize) -> bool {
        // Only a heading of a higher level than every heading met on the way up can hold the cell.
        let mut enclosing = self.heading_level(cell).unwrap_or(usize::MAX);
        for above in (0..cell).rev() {
            let Some(level) = self.heading_level(above) else { continue };
            if level >= enclosing {
                continue;
            }
            if self.model.cells.get(above).is_some_and(|c| self.sections_collapsed.contains(&c.id))
            {
                return true;
            }
            enclosing = level;
        }
        false
    }

    /// How many cells a collapsed heading at `cell` is hiding.
    pub fn hidden_by(&self, cell: usize) -> usize {
        self.section_of(cell).len().saturating_sub(1)
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

    /// Which cell a byte of the text belongs to, by the same rule as [`text::cell_at`] uses for a line.
    /// A marker belongs to the cell it starts, and a byte past the last cell belongs to none.
    pub fn cell_at_offset(&self, offset: usize) -> Option<usize> {
        text::cell_at_byte(&self.spans, offset)
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
/// end of the new cell's source, in the text after it. `source` is the cell's own source, which is
/// escaped on the way in as [`text::escape_source`] says.
pub fn insert_cell(
    spans: &[CellSpan],
    text_len: usize,
    at: usize,
    kind: CellKind,
    id: &str,
    source: &str,
) -> (Vec<(Range<usize>, String)>, usize) {
    let marker = text::marker(kind, id);
    let source = text::escape_source(source);
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

/// Tags as a person types them: separated by commas or spaces, with blanks and repeats left out.
pub fn tags_typed(typed: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in typed.split([',', ' ']).map(str::trim).filter(|tag| !tag.is_empty()) {
        if !tags.iter().any(|kept| kept == tag) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

/// One cell's marker and source as they are in the text.
pub fn cell_text(text: &str, span: &CellSpan) -> String {
    text[span_start(span)..span_end(span)].to_owned()
}

/// The text of cells written again with fresh ids, so pasting a copy never gives two cells one id.
pub fn with_fresh_ids(cells: &[(CellKind, String)]) -> String {
    cells
        .iter()
        .map(|(kind, source)| text::cell_text(*kind, &nbformat::new_id(), source))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The kind and source of cells `range`, which is what a copy takes.
pub fn cells_copied(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
) -> Vec<(CellKind, String)> {
    range
        .map(|cell| (spans[cell].kind, text::cell_source(text, &spans[cell]).into_owned()))
        .collect()
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
            Some((span.marker_bytes.clone(), text::marker(kind, &id)))
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
    match by {
        -1 if range.start > 0 => move_cells_to(text, spans, range.clone(), range.start - 1),
        1 => move_cells_to(text, spans, range.clone(), range.end + 1),
        _ => None,
    }
}

/// The edit that moves cells `range` into the gap before cell `to`, counted in the cells as they are
/// now, from 0 for the top to the number of cells for the bottom. `None` when that gap is inside or
/// next to the range, which would leave the cells where they are.
pub fn move_cells_to(
    text: &str,
    spans: &[CellSpan],
    range: Range<usize>,
    to: usize,
) -> Option<(Range<usize>, String)> {
    if range.is_empty() || to > spans.len() || (range.start..=range.end).contains(&to) {
        return None;
    }
    let (outer, order) = match to < range.start {
        true => (to..range.end, [range.clone(), to..range.start]),
        false => (range.start..to, [range.end..to, range.clone()]),
    };
    let replaced = span_start(&spans[outer.start])..span_end(&spans[outer.end - 1]);
    let rebuilt: Vec<String> =
        order.into_iter().map(|part| cells_text(text, spans, part)).collect();
    Some((replaced, rebuilt.join("\n")))
}

/// Where the first of `moved` cells ends up when they are moved into the gap before cell `to`.
pub fn moved_to(moved: Range<usize>, to: usize) -> usize {
    match to < moved.start {
        true => to,
        false => to - moved.len(),
    }
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
mod asked_tests {
    use super::*;

    fn asked(source: &str, word_start: usize, typed: &str) -> Asked {
        Asked {
            request: "k1".to_owned(),
            word_start,
            body_start: 0,
            source: source.to_owned(),
            kernel_start: word_start,
            typed: typed.to_owned(),
            matches: Vec::new(),
            answered: true,
        }
    }

    #[test]
    fn an_answer_stands_while_the_word_grows_and_not_once_it_shrinks_or_moves() {
        let question = asked("v.it", 2, "it");
        assert!(question.is_about(2, "v.", "it"));
        assert!(question.is_about(2, "v.", "ite"), "typing on narrows the same answer");
        assert!(
            !question.is_about(2, "v.", ""),
            "`v.` wants every method, not those matching `it`"
        );
        assert!(!question.is_about(2, "s.", "it"), "the value in front changed");
        assert!(!question.is_about(3, "v.i", "t"), "another word");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duration_is_written_the_way_the_reference_editor_writes_one() {
        assert_eq!(duration(Duration::from_millis(3)), "< 10 ms");
        assert_eq!(duration(Duration::from_millis(222)), "222ms");
        assert_eq!(duration(Duration::from_millis(5003)), "5s 3ms");
        assert_eq!(duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn the_status_line_names_the_count_and_what_the_cell_is_waiting_for() {
        let done = Run::Done {
            ok: true,
            took: Duration::from_millis(69),
            at: SystemTime::now(),
            clock: crate::services::clock::TimeOfDay { hour: 14, minute: 3, second: 22 },
        };
        assert_eq!(status_words(Some(&done), Some(3)), "[3] 69ms at 14:03:22");
        assert_eq!(status_words(Some(&Run::Queued), None), "Queued");
        assert_eq!(status_words(None, Some(7)), "[7]");
        assert_eq!(status_words(None, None), "[ ]");
    }

    #[test]
    fn the_status_mark_follows_how_the_last_run_went() {
        assert_eq!(status_mark(None), StatusMark::Nothing);
        assert_eq!(status_mark(Some(&Run::Skipped)), StatusMark::Skipped);
        assert_eq!(status_mark(Some(&Run::Queued)), StatusMark::Queued);
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level_or_above_and_collapsing_hides_it() {
        let mut model = nbformat::empty();
        model.cells = vec![
            nbformat::Cell::new(CellKind::Markdown, "a", "# A"),
            nbformat::Cell::new(CellKind::Code, "a1", "1"),
            nbformat::Cell::new(CellKind::Markdown, "b", "## B"),
            nbformat::Cell::new(CellKind::Code, "b1", "2"),
            nbformat::Cell::new(CellKind::Markdown, "c", "# C"),
            nbformat::Cell::new(CellKind::Code, "c1", "3"),
            nbformat::Cell::new(CellKind::Markdown, "note", "not a #heading"),
        ];
        let (mut tab, text) = NotebookTab::new(model);
        tab.spans = text::spans(&text);
        assert_eq!((tab.section_of(0), tab.section_of(2), tab.section_of(4)), (0..4, 2..4, 4..7));
        assert_eq!(
            (tab.heading_above(3), tab.heading_above(1), tab.heading_level(6)),
            (Some(2), Some(0), None)
        );
        tab.sections_collapsed.insert("b".to_owned());
        let hidden: Vec<usize> = (0..7).filter(|cell| tab.in_a_collapsed_section(*cell)).collect();
        assert_eq!(hidden, vec![3]);
        tab.sections_collapsed.insert("a".to_owned());
        let hidden: Vec<usize> = (0..7).filter(|cell| tab.in_a_collapsed_section(*cell)).collect();
        assert_eq!(hidden, vec![1, 2, 3], "a heading inside a collapsed section is hidden with it");
        assert_eq!(tab.hidden_by(0), 3);
    }

    fn three() -> (String, Vec<CellSpan>) {
        let text = "# %% id=a\none\n# %% [markdown] id=b\n# Two\n# %% id=c\nthree".to_owned();
        let spans = text::spans(&text);
        (text, spans)
    }

    #[test]
    fn a_cell_moves_into_any_gap_and_lands_where_moved_to_says() {
        let (text, spans) = three();
        let last = applied(&text, vec![move_cells_to(&text, &spans, 0..1, 3).unwrap()]);
        assert_eq!(last, "# %% [markdown] id=b\n# Two\n# %% id=c\nthree\n# %% id=a\none");
        assert_eq!(moved_to(0..1, 3), 2);
        let first = applied(&text, vec![move_cells_to(&text, &spans, 2..3, 0).unwrap()]);
        assert_eq!(first, "# %% id=c\nthree\n# %% id=a\none\n# %% [markdown] id=b\n# Two");
        assert_eq!(moved_to(2..3, 0), 0);
        assert!(move_cells_to(&text, &spans, 1..2, 1).is_none(), "the gap before itself");
        assert!(move_cells_to(&text, &spans, 1..2, 2).is_none(), "the gap after itself");
        assert!(move_cells_to(&text, &spans, 1..2, 4).is_none(), "past the end");
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
        assert_eq!(tab.cell_at_offset(text.len() + 1), None);
    }
}
