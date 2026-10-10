//! The window's half of auto-complete: what is offered, when the popup is open, and what the five
//! keys mean while it is.
//!
//! `unluminous_core::completion` says what a stem is, what matches it and in what order;
//! `components::completion` draws the list; this is what sits between them. Nothing here draws and
//! nothing here decides what a match is.
//!
//! ## Everything it offers is already in memory
//!
//! There is no new index, no new thread and no watcher. The four sources are ones `task-1676`
//! already keeps fresh: this tab's definitions and its distinct words, cached on the tab and keyed
//! on `Document::text_revision()`; the other open tabs' definitions, the same; the project's
//! definitions from the code index (`services::project_symbols`, which Atrius keeps fresh on its own
//! threads); and the file's `Grammar`. Completion **reads** what is there.
//!
//! ## Three tiers, one order (`task-2231`)
//!
//! The lexical tier is the four sources above. The structural tier (`app::gather`) adds what the code
//! index knows about the shape of a definition: the locals and parameters of the enclosing function,
//! the members of the value before a `.` or `::`, and the project's names a file does not import yet,
//! which accepting imports. The semantic tier is a language server's answer (`app::servers`), merged in
//! when it arrives. All of it is ordered by `unluminous_core::completion::order`, the chain of weighers,
//! with the place the caret is in and what was chosen before, so the popup and `unluminous-cli editor
//! complete` cannot disagree.
//!
//! The ownership rule of `task-1675` §3.3 carries over unchanged: *a file that is open is owned by
//! its `Document`, and every other file is owned by the index*. So the open files' paths are
//! dropped from what the index offers, or a name being edited in a tab would be offered twice —
//! once as it is now and once as the disk last saw it.
//!
//! ## And nothing here runs once a frame
//!
//! [`CompletionState`] carries the `text_revision` and the caret its rows were worked out at, and
//! [`UnluminousApp::keep_the_completion_fresh`] compares two integers before it does anything at all. A
//! caret blink, a repaint, a frame of idling: two comparisons and no allocation, which is
//! `task-1666`'s rule kept the way `symbols::Hover` already keeps it.
//!
//! ## The five keys, and why they are consumed
//!
//! `Up`, `Down`, `Tab`, `Enter` and `Escape` are removed from the frame's input with
//! `consume_key` **before** the panes are drawn, so they never reach
//! `editor_view::handle_input`. That is the one-frame ordering `Go to File` and `Find in Files`
//! already rely on. Everything else flows through untouched: letters keep typing and refiltering the
//! list, and the popup takes exactly five keys and only while it is open.

use std::ops::Range;
use std::path::{Path, PathBuf};

use unluminous_core::completion::{self, Candidate, Kind, Question, Row, Source};
use unluminous_core::imports::{self as core_imports, Context as ImportContext};
use unluminous_core::{Command, Grammar, Role};

use crate::app::{Focus, UnluminousApp};
use crate::components::completion as view;
use crate::services::{file_kind, imports};

/// How many rows are drawn before the list scrolls.
pub const VISIBLE_ROWS: usize = 8;

/// How much of a word has to be typed before the popup arrives unasked.
///
/// One character opens on nearly every letter of a file and matches most of it, which is noise; the
/// same argument in Helix's own thread talked its users down from seven to two or three. Two is not
/// a setting: `Ctrl+Space` covers the rest, and it works from one character.
pub const AUTOMATIC_STEM: usize = 2;

/// What is being offered, where, and which row is chosen.
///
/// One of these on the window at most, not one per tab: only one popup can exist and it belongs to
/// the pane with the keyboard, which is the same reasoning as the one `hover` and the one
/// `references` modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionState {
    /// The stem's byte range in the document: what `Enter` replaces.
    pub stem: Range<usize>,
    /// What is offered, best first.
    pub rows: Vec<Row>,
    /// Which row is chosen. The first to begin with, so `Tab` alone takes the best match.
    pub chosen: usize,
    /// The first row drawn, which is what the list scrolls by.
    pub scroll: usize,
    /// The `text_revision` the rows were worked out at.
    pub revision: u64,
    /// Where the caret was then, so a caret that moved with no edit behind it closes the popup.
    pub caret: usize,
    /// True when it was asked for by hand, which is what lets it live in a comment or a string.
    pub manual: bool,
    /// The tab it belongs to. A popup can only exist on a file a plugin claims, so there is always
    /// a path, and comparing it against the tab that is showing is what closes the popup when the
    /// tab changes or the keyboard moves to another pane — derived from the state rather than fired
    /// from each of the places a tab can change.
    pub path: PathBuf,
    /// The kind of place the rows were offered at, which a choice is counted under. `task-2231` §6.3.
    pub place: unluminous_core::place::Place,
    /// The import the rows were worked out for, when they were worked out for one. `task-1680`.
    ///
    /// It is carried so that accepting knows what `Tab` means — a specifier's whole range comes out
    /// of the reading, because the grammar cannot say what the whole of `./lay/out` is — and so the
    /// popup never has to read the text again to find out. `None` for every ordinary completion,
    /// which is what makes the field invisible to the four sources.
    pub import: Option<ImportContext>,
}

impl CompletionState {
    /// The rows that are drawn: at most [`VISIBLE_ROWS`] of them, starting at the scroll.
    pub fn shown(&self) -> Range<usize> {
        let visible = VISIBLE_ROWS.min(self.rows.len());
        self.scroll..(self.scroll + visible).min(self.rows.len())
    }

    /// The row that would be accepted.
    pub fn chosen_row(&self) -> Option<&Row> {
        self.rows.get(self.chosen)
    }

    /// Bring the chosen row back inside the eight that are drawn, dragging the list with it.
    fn settle_the_scroll(&mut self) {
        let visible = VISIBLE_ROWS.min(self.rows.len());
        if visible == 0 {
            self.scroll = 0;
            return;
        }
        if self.chosen < self.scroll {
            self.scroll = self.chosen;
        }
        if self.chosen >= self.scroll + visible {
            self.scroll = self.chosen + 1 - visible;
        }
        self.scroll = self.scroll.min(self.rows.len() - visible);
    }
}

/// The five keys the popup takes, once they have been read out of a frame's input.
///
/// A structure rather than five arguments, so a caller cannot pass `Tab` where `Enter` goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionKeys {
    pub down: bool,
    pub up: bool,
    /// Accept, replacing the whole identifier the caret is inside.
    pub tab: bool,
    /// Accept, replacing the stem only.
    pub enter: bool,
    pub escape: bool,
}

impl CompletionKeys {
    /// One key on its own, which is what a test presses.
    pub fn down() -> Self {
        Self { down: true, ..Self::default() }
    }

    pub fn up() -> Self {
        Self { up: true, ..Self::default() }
    }

    pub fn tab() -> Self {
        Self { tab: true, ..Self::default() }
    }

    pub fn enter() -> Self {
        Self { enter: true, ..Self::default() }
    }

    pub fn escape() -> Self {
        Self { escape: true, ..Self::default() }
    }
}

/// What is on offer at one point in the file.
///
/// A structure rather than a tuple, because `task-1680` gave it a fourth part and a four-tuple is
/// where a caller starts getting the order wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// What a row replaces: the stem, or what has been typed of a module specifier.
    pub range: Range<usize>,
    /// The text of that range.
    pub typed: String,
    /// What is offered, best first.
    pub rows: Vec<Row>,
    /// The import these rows belong to, if they belong to one.
    pub import: Option<ImportContext>,
}

/// Whether a candidate is worth building at all: everything when nothing has been typed, and the
/// cheap subsequence reject otherwise.
///
/// `task-1678` added `could_match` because turning thousands of names into owned strings to throw
/// nearly all of them away is the difference between a keystroke that allocates and one that does
/// not. The empty case is `task-1680`'s: inside an import there is a real answer to a stem with
/// nothing in it.
fn offers(typed: &str, name: &str) -> bool {
    typed.is_empty() || completion::could_match(typed, name)
}

/// Where the popup hangs, worked out by the pane that has the keyboard while it draws itself.
///
/// Frame local. The pane loop borrows the focus, so the pane being drawn is the only thing that
/// knows where its caret ended up on the screen; the window draws the popup **after** the loop,
/// from what that pane recorded, so the list is never underneath a divider or a later pane and
/// never drawn twice in a split view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompletionAnchor {
    /// The caret's own box on the screen, which the list hangs under.
    pub caret: egui::Rect,
    /// The editing area it is in, which the list is flipped and clamped inside.
    pub pane: egui::Rect,
}

/// How many of the project index's names one stem may draw into the candidate pool.
///
/// **`task-1984` C12.** `task-1677` §7 set one budget for this feature -- under 5 ms for a whole
/// keystroke on the largest file in this repository -- and named the answer if a project ever broke
/// it: an honest limit on the pool. This is that limit.
///
/// Two thousand, because a stem of two letters is where the popup opens and where the cost lives:
/// on this repository a two letter stem matched about three thousand of the index's eleven thousand
/// names, and scoring a candidate is about a microsecond. It is deliberately far above what a stem
/// of three or more letters reaches, so the cut is invisible except in the one case where a person
/// has typed almost nothing and cannot be looking for a particular name yet.
///
/// `cargo run --release -p unluminous-app --example completion_cost` is how this is measured again.
pub const MOST_FROM_THE_INDEX: usize = 2_000;

/// How many of the project index's names a stem of one letter may draw. A tighter cap than
/// [`MOST_FROM_THE_INDEX`] because one letter reaches the most names of any stem and is only ever asked
/// by hand: with the richer rows of `task-2231` a thousand more of them cost a millisecond, and the
/// budget for a whole keystroke is five.
pub const MOST_FOR_ONE_LETTER: usize = 1_000;

/// How many rows have the import they would add worked out for their detail: the most the command line
/// prints without being asked for more, and more than the popup shows.
const COMPLETIONS_LABELLED: usize = 50;

impl UnluminousApp {
    /// Whether auto-complete applies to the file that is showing.
    pub fn completion_applies_here(&self) -> bool {
        file_kind::completion_applies(self.files.active().path(), self.plugins.grammars())
    }

    /// Whether a modal owns the keyboard, in which case there is no popup and no trigger.
    ///
    /// A modal already stands the editing area aside, so this is belt and braces — but the belt is
    /// what stops a popup that was open when `Find in Files` opened over it from staying there.
    fn a_modal_is_open(&self) -> bool {
        self.settings_window.open
            || self.prompt.is_some()
            || self.go_to_file.is_some()
            || self.find_in_files.is_some()
            || self.references.is_some()
            || self.about.is_some()
            || self.confirmation.is_some()
    }

    /// Everything that could be offered for a stem, gathered once and never per frame.
    ///
    /// The four sources of §4.1, in the order the tie-break reads them. Each name is put through
    /// `completion::could_match` before a candidate is built for it, because turning four thousand
    /// index names into owned strings to throw nearly all of them away again is the difference
    /// between a keystroke that allocates and one that does not.
    pub fn completion_candidates(&mut self, stem: &str, offset: usize) -> Vec<Candidate> {
        let mut pool: Vec<Candidate> = Vec::new();
        let word_start = offset.saturating_sub(stem.len());
        // With nothing typed straight after a `.` or `::`, the members of the value are the answer
        // (`task-2231` §6.1). Anywhere else, nothing typed is a list somebody asked for, with
        // Ctrl+Space or the command line, and it holds what fits here: the locals, this file's
        // definitions, the server's rows and the language's words, but not every name in the project
        // or every word of the file, which with nothing to filter them would bury the rest.
        let nothing_typed = stem.is_empty();
        if nothing_typed {
            if let Some(receiver) = self.asked_at(word_start).receiver {
                if !self.at_a_member_access(word_start) {
                    let mut pool = self.member_candidates(&receiver, stem, offset);
                    pool.extend(self.server_members(stem, offset));
                    return pool;
                }
                return pool;
            }
        }
        // After a dot in a notebook the kernel knows what the value has, and a keyword, a word of the
        // file or a definition elsewhere is not a member of it: `df.de` offering `def` and `del` is a
        // list nobody can use. Until the kernel answers, the ordinary sources stand in. `task-2229`.
        if self.at_a_member_access(word_start) {
            let from_the_kernel = self.kernel_candidates(stem, offset);
            if !from_the_kernel.is_empty() || self.kernel_is_being_asked(word_start, offset) {
                return from_the_kernel;
            }
        }
        // After a `.` or `::` the members of the value are the answer, and a keyword, a word of the
        // file or an unrelated project name is not one of them. `task-2231` §6.4.
        if let Some(receiver) = self.asked_at(word_start).receiver {
            let mut pool = self.member_candidates(&receiver, stem, offset);
            pool.extend(self.server_members(stem, offset));
            return pool;
        }
        let here = self.files.active_index();

        // The open tabs, read from their live text: this one's definitions and its words first,
        // then every other tab's definitions.
        for index in 0..self.files.len() {
            let path = self.files.at(index).path().map(Path::to_path_buf);
            let detail = path.as_deref().map(file_name).unwrap_or_default();
            let source = if index == here { Source::ThisFile } else { Source::OpenTab };
            let symbols = self.tab_symbols(index);
            for (name, definition) in &symbols.named {
                if completion::could_match(stem, name) {
                    pool.push(Candidate::described(
                        name.clone(),
                        source,
                        Some(Kind::from(definition.kind)),
                        detail.clone(),
                    ));
                }
            }
            // Only this file's words. Harvesting every open file's words is §12's rejection: the
            // index's definitions are the cross-file offer, and they carry a kind and a file where
            // a raw word carries nothing.
            if index == here && !nothing_typed {
                // The word being typed is in the file too, half typed; it is never offered from here.
                // A real name equal to it still comes from the definitions and the server.
                for word in &symbols.words {
                    if word != stem && completion::could_match(stem, word) {
                        pool.push(Candidate::new(word.clone(), Source::Word));
                    }
                }
            }
        }

        // The project's definitions, with the open files' paths dropped: the ownership rule.
        //
        // **Bounded at [`MOST_FROM_THE_INDEX`]** (`task-1984` C12). `task-1677` §7 set the budget at
        // under 5 ms a keystroke on the largest file in this repository and said what to do if a
        // future project broke it: *"the answer is capping the pool (an honest `LIMIT`, the
        // references modal's pattern), not a thread"*. A future project turned out to be this one --
        // the index held 4,445 names when that was written and holds 11,116 today, so a two letter
        // stem gathered and scored three thousand names and one whole keystroke came to 11.3 ms.
        //
        // **The index is the source that is cut and the only one**, because the pool is gathered in
        // order of how much a row is worth: this tab's definitions, this tab's words, the other tabs'
        // definitions, then the project, then the language's own keywords. The first three are what a
        // person is most likely to want and are small; the last is smaller still and must never be
        // lost, because a keyword the language defines is always a right answer. What is left is the
        // project, which is both the largest and the least certain.
        pool.extend(self.local_candidates(stem, offset));
        if !nothing_typed {
            pool.extend(self.project_candidates(stem));
        }
        pool.extend(self.server_candidates(stem, offset));

        // The language's own words. The manifest already holds them; completion is the second
        // reader of the same data. In a markup file the position decides which of the lists is
        // offered at all — `task-1694` §6.4: a tag name offers the element names, an attribute
        // offers the attribute names, and prose, a value and raw text offer none of them. Without
        // the filter, typing an ordinary sentence in an HTML file pops a list of element names. A
        // grammar that is not markup offers all three lists, as it always did, and the position is
        // never asked, so the document is never read for it — the question is what costs a read,
        // and it is asked only where it can change the answer.
        if let Some(grammar) = self.grammar_for(self.files.active().path()) {
            let lists: Vec<(&Vec<String>, &str)> = if grammar.markup {
                // `None` here is an offset the document cannot answer for; all three lists is the
                // safe offer when the position is unknown.
                let position = unluminous_core::syntax::markup_position(
                    &self.document().text().to_string(),
                    offset,
                    grammar,
                );
                match position {
                    Some(unluminous_core::syntax::MarkupPosition::TagName) => {
                        vec![(&grammar.keywords, "keyword")]
                    }
                    Some(unluminous_core::syntax::MarkupPosition::Attribute) => {
                        vec![(&grammar.builtins, "builtin")]
                    }
                    Some(_) => Vec::new(),
                    None => vec![
                        (&grammar.keywords, "keyword"),
                        (&grammar.builtins, "builtin"),
                        (&grammar.types, "type"),
                    ],
                }
            } else {
                vec![
                    (&grammar.keywords, "keyword"),
                    (&grammar.builtins, "builtin"),
                    (&grammar.types, "type"),
                ]
            };
            for (list, detail) in lists {
                for word in list {
                    if completion::could_match(stem, word) {
                        let kind = (detail == "keyword").then_some(Kind::Keyword);
                        pool.push(Candidate::described(
                            word.clone(),
                            Source::Language,
                            kind,
                            detail,
                        ));
                    }
                }
            }
        }
        // A notebook's live kernel, which knows what the cells that ran have made. `task-2220`.
        if self.files.active().notebook.is_some() {
            pool.extend(self.kernel_candidates(stem, offset));
        }
        pool
    }

    /// Everything that could be offered inside an import, which is one pool instead of four.
    ///
    /// The four ordinary sources are **not** added to it. A keyword, a local word and an unrelated
    /// name from the project are all wrong answers to `from '│'`, and a list holding them would be
    /// a list nobody could use.
    fn import_candidates(&mut self, context: &ImportContext, typed: &str) -> Vec<Candidate> {
        let Some(from) = self.files.active().path().map(Path::to_path_buf) else {
            return Vec::new();
        };
        let grammar = self.completion_grammar();
        match context {
            ImportContext::Specifier { .. } => self.specifier_candidates(&from, typed, &grammar),
            ImportContext::Named { module, .. } => {
                let found = {
                    let project = self.the_project();
                    imports::resolve_specifier(&project, &from, module, &grammar)
                };
                match found {
                    Some(module) => self.export_candidates(&module, typed),
                    None => Vec::new(),
                }
            }
            ImportContext::Segment { segments, .. } => {
                self.segment_candidates(&from, segments, typed, &grammar)
            }
        }
    }

    /// The project as `services::imports` reads it: where it is, and every file in it.
    fn the_project(&self) -> imports::Project<'_> {
        imports::Project { root: self.tree.root(), files: self.tree.all_files() }
    }

    /// Every specifier that would reach a file of this language from the file being edited.
    fn specifier_candidates(&self, from: &Path, typed: &str, grammar: &Grammar) -> Vec<Candidate> {
        let project = self.the_project();
        imports::specifiers(&project, from, grammar)
            .into_iter()
            .filter(|(written, _)| offers(typed, written))
            .map(|(written, path)| {
                Candidate::described(written, Source::Module, Some(Kind::Module), file_name(&path))
            })
            .collect()
    }

    /// What a module path could become: the child modules where it has reached, and the exported
    /// names of the file it has reached.
    fn segment_candidates(
        &mut self,
        from: &Path,
        segments: &[String],
        typed: &str,
        grammar: &Grammar,
    ) -> Vec<Candidate> {
        // The tree is borrowed for the whole of this block and given back before the exports are
        // asked for, because reading an open tab's symbols needs the window mutably.
        let (mut pool, reached) = {
            let project = self.the_project();
            if segments.is_empty() {
                let rows = imports::roots(&project, grammar)
                    .into_iter()
                    .filter(|(name, _)| offers(typed, name))
                    .map(|(name, folder)| {
                        let detail = match folder.is_some() {
                            true => "package",
                            false => "module",
                        };
                        Candidate::described(name, Source::Module, Some(Kind::Module), detail)
                    })
                    .collect();
                return rows;
            }
            let Some(reached) = imports::resolve_segments(&project, from, segments, grammar) else {
                return Vec::new();
            };
            let mut pool: Vec<Candidate> = Vec::new();
            if let Some(folder) = reached.folder.as_deref() {
                for (name, path) in imports::children(&project, folder, grammar) {
                    if !offers(typed, &name) {
                        continue;
                    }
                    let detail = match path.is_dir() || path.extension().is_none() {
                        true => "module".to_owned(),
                        false => file_name(&path),
                    };
                    pool.push(Candidate::described(
                        name,
                        Source::Module,
                        Some(Kind::Module),
                        detail,
                    ));
                }
            }
            (pool, reached.file)
        };
        if let Some(module) = reached {
            pool.extend(self.export_candidates(&module, typed));
        }
        pool
    }

    /// What one module exports, from wherever that module is owned.
    ///
    /// The ownership rule of `task-1675` §3.3, unchanged: a module that is **open** is owned by its
    /// `Document`, so a function added in the tab beside this one is offered before it is saved;
    /// every other module is owned by the index.
    fn export_candidates(&mut self, module: &Path, typed: &str) -> Vec<Candidate> {
        let detail = file_name(module);
        let open = self.files.iter().position(|file| file.path() == Some(module));
        if let Some(index) = open {
            let symbols = self.tab_symbols(index);
            return symbols
                .named
                .iter()
                .filter(|(name, definition)| definition.exported && offers(typed, name))
                .map(|(name, definition)| {
                    Candidate::described(
                        name.clone(),
                        Source::OpenTab,
                        Some(Kind::from(definition.kind)),
                        detail.clone(),
                    )
                })
                .collect();
        }
        let Some(symbols) = self.project_symbols.as_ref() else {
            return Vec::new();
        };
        let Some(rel) = symbols.relative(module) else { return Vec::new() };
        symbols
            .read(|table| {
                table
                    .files()
                    .find(|(path, _)| *path == rel)
                    .map(|(_, defs)| {
                        defs.iter()
                            .filter(|d| {
                                d.exported && d.container.is_none() && offers(typed, &d.name)
                            })
                            .map(|d| {
                                let kind = crate::app::gather::kind_of(d.symbol_kind);
                                Candidate::described(
                                    d.name.clone(),
                                    Source::Index,
                                    Some(kind),
                                    detail.clone(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// What is on offer at a point: what a row replaces, what has been typed of it, the rows
    /// themselves, and the import they belong to if they belong to one.
    ///
    /// One function, so `unluminous-cli editor complete` prints exactly the list the popup would show
    /// and the two can never come to disagree about what is on offer.
    pub fn completion_offer(&mut self, offset: usize) -> Offer {
        let text = self.document().text().to_string();
        let grammar = self.completion_grammar();
        // The import question is asked first, and when it answers the four sources are not gathered
        // at all.
        if let Some(context) = core_imports::context_at(&text, offset, &grammar) {
            let range = context.typed_range();
            let typed = text[range.clone()].to_owned();
            let mut pool = self.import_candidates(&context, &typed);
            // The project's own modules come from the structure; the standard library and the
            // project's dependencies are only known to a language server. Asked only where the caret
            // is the end of what is typed, which a server can answer about.
            if range.end == offset {
                pool.extend(self.server_candidates(&typed, offset));
            }
            let rows = completion::rank_all(&typed, pool);
            return Offer { range, typed, rows, import: Some(context) };
        }
        let range = completion::stem_at(&text, offset, &grammar);
        if range.is_empty() {
            // Straight after `.` or `::` in a notebook's code cell, the kernel says what the value in
            // front has, which is the answer a notebook is for. `task-2229`.
            let rows = match self.at_a_member_access(offset) {
                true => completion::rank_all("", self.kernel_candidates("", offset)),
                false => self.completion_rows("", offset),
            };
            return Offer { range, typed: String::new(), rows, import: None };
        }
        let typed = text[range.clone()].to_owned();
        let rows = self.completion_rows(&typed, offset);
        Offer { range, typed, rows, import: None }
    }

    /// Whether `offset` is straight after `.` or `::` in a notebook's code cell, with something in
    /// front of the dot to be a member of: `df.`, `v.`, `std::`, `f().`, `a[0].`. A dot after a
    /// space or at the start of a line is not one, and neither is a decimal point (`3.`). `task-2229`.
    pub(crate) fn at_a_member_access(&self, offset: usize) -> bool {
        let file = self.files.active();
        let Some(tab) = file.notebook.as_deref() else { return false };
        let Some(cell) = tab.cell_at_offset(offset) else { return false };
        let span = &tab.spans[cell];
        let in_code = span.kind == unluminous_jupyter::nbformat::CellKind::Code
            && offset > span.body_bytes.start
            && offset <= span.body_bytes.end;
        // The cell's own text up to the point, which is all the question needs to read.
        in_code
            && is_a_member_access(
                &file.document.text().byte_slice(span.body_bytes.start..offset).to_string(),
            )
    }

    /// What a hypothetical stem would offer at a point, without putting that stem in the document.
    ///
    /// The position still matters inside imports: it decides whether the candidates are project
    /// files or one module's exports. Outside an import the ordinary four candidate sources are
    /// ranked directly. The empty replacement range states that no real bytes are involved.
    pub fn hypothetical_completion_offer(&mut self, offset: usize, stem: &str) -> Offer {
        let text = self.document().text().to_string();
        let grammar = self.completion_grammar();
        if let Some(context) = core_imports::context_at(&text, offset, &grammar) {
            let pool = self.import_candidates(&context, stem);
            let rows = completion::rank_all(stem, pool);
            return Offer {
                range: offset..offset,
                typed: stem.to_owned(),
                rows,
                import: Some(context),
            };
        }
        // A hypothetical word is not in the document, so no language server is asked about it: the
        // server would answer about the text as it is, which is not the question.
        self.asking_hypothetically = true;
        let rows = self.completion_rows(stem, offset);
        self.asking_hypothetically = false;
        Offer { range: offset..offset, typed: stem.to_owned(), rows, import: None }
    }

    /// Opens the popup at a caret if the rows there offer anything, the way typing does. A language
    /// server's answer arriving after a member trigger found nothing structural calls it.
    ///
    /// @param offset - the caret
    pub(crate) fn open_the_completion_if_anything(&mut self, offset: usize) {
        self.open_the_completion(offset, false);
    }

    /// The rows a stem offers here, best first. What the popup shows and what the command line
    /// prints, so the two can never disagree.
    pub fn completion_rows(&mut self, stem: &str, offset: usize) -> Vec<Row> {
        let word_start = offset.saturating_sub(stem.len());
        let asked = self.asked_at(word_start);
        let mut pool = self.completion_candidates(stem, offset);
        self.mark_the_names_written_here(&mut pool);
        let language = self.completion_grammar().language;
        let question = Question { stem: stem.to_owned(), place: asked.place };
        let stats = &self.completion_stats;
        let mut rows = completion::order(&question, pool, &|name| {
            stats.chosen(&language, asked.place, stem, name)
        });
        // A row that needs its import says which, worked out for the rows anybody can see.
        for row in rows.iter_mut().take(COMPLETIONS_LABELLED) {
            if let Some(rel) = row.info.needs_import.clone() {
                if let Some(label) = self.import_label(&rel) {
                    row.detail = label;
                }
            }
        }
        rows
    }

    /// The server's rows at a member access, as near as the value's own members: the server knows the
    /// value's type, where the structure, when it could not work the type out, only guesses.
    ///
    /// @param stem - what has been typed
    /// @param offset - the caret
    fn server_members(&mut self, stem: &str, offset: usize) -> Vec<Candidate> {
        self.server_candidates(stem, offset).into_iter().map(|c| c.at(completion::Locality::Receiver)).collect()
    }

    /// Marks every candidate with how many times its name is already written in the file that is
    /// showing ([`completion::Info::uses_here`]), from the counts the tab keeps for its revision.
    ///
    /// @param pool - the candidates
    fn mark_the_names_written_here(&mut self, pool: &mut [Candidate]) {
        let here = self.files.active_index();
        let counts = &self.tab_symbols(here).counts;
        for candidate in pool.iter_mut() {
            candidate.info.uses_here = counts.get(candidate.name.as_str()).copied().unwrap_or(0);
        }
    }

    /// The grammar reading the file that is showing, or an empty one.
    fn completion_grammar(&self) -> Grammar {
        self.grammar_for(self.files.active().path()).cloned().unwrap_or_default()
    }

    /// Work the popup for a frame: close it when it has stopped being an answer, and refilter it
    /// when the word being typed has changed.
    ///
    /// Called from the pane with the keyboard, after its input has been handled, so the stem
    /// includes the letter that was just typed. `typed` is whether a character reached the document
    /// this frame — the automatic trigger fires on that and on nothing else, which is why a paste,
    /// an undo or a command line edit does not make a list appear over somebody's work.
    ///
    /// A refresh that closed the popup asks again in the same frame, which is `task-1680` §8: typing
    /// `::` ends one segment and starts another, and without this the new list would need one more
    /// keystroke before it came back. It changes nothing outside an import, because
    /// [`Self::offer_a_completion`] still refuses a stem shorter than [`AUTOMATIC_STEM`].
    pub fn keep_the_completion_fresh(&mut self, typed: bool) {
        if self.completion.is_some() {
            self.refresh_the_completion();
        }
        if self.completion.is_none() && typed {
            self.offer_a_completion();
        }
        self.keep_the_signature_fresh(typed);
    }

    /// Open the popup unasked, if every one of §5.1's conditions holds.
    ///
    /// An import changes two of them and nothing else. The two-character threshold does not apply,
    /// because `from '│'` and `use │` are positions at which the language itself says what comes
    /// next; and a module specifier **is** a string, so the refusal to open inside one is asked of
    /// the import reading instead of the tokeniser's.
    fn offer_a_completion(&mut self) {
        if !self.settings.suggestions.is_automatic()
            || !self.completion_applies_here()
            || self.a_modal_is_open()
            || self.focus != Focus::Editor
        {
            return;
        }
        let head = self.document().selection().head;
        let text = self.document().text().to_string();
        let grammar = self.completion_grammar();
        match core_imports::context_at(&text, head, &grammar) {
            Some(context) => {
                if !context.is_specifier() && !self.point_is_code(context.typed_range().start) {
                    return;
                }
            }
            None => {
                let stem = completion::stem_at(&text, head, &grammar);
                // In a notebook, `.` and `::` ask the kernel at once, with nothing typed after them,
                // and in any language a separator `language.members` names opens the members of the
                // value in front of it. `task-2231` §6.1.
                let member =
                    self.at_a_member_access(head) || self.asked_at(head).receiver.is_some();
                if stem.is_empty() && member {
                    // When the structure has nothing to offer for this value, the popup opens when the
                    // language server answers, which `server_completions_arrived` does.
                    if head > 0
                        && self.point_is_code(head - 1)
                        && !self.open_the_completion(head, false)
                    {
                        self.awaiting_a_server_popup = self.server_is_being_asked();
                    }
                    return;
                }
                if text[stem.clone()].chars().count() < AUTOMATIC_STEM {
                    return;
                }
                // A doc comment's prose does not want a list flickering over it. Asked at the
                // stem's own first byte rather than at the caret, because a caret sitting exactly
                // at the end of a comment is past the span and would read as code.
                if !self.point_is_code(stem.start) {
                    return;
                }
            }
        }
        self.open_the_completion(head, false);
    }

    /// Whether a point in the tab that is showing is code rather than a comment or a string.
    fn point_is_code(&mut self, at: usize) -> bool {
        let index = self.files.active_index();
        self.tab_symbols(index).read.role_at(at) == Role::Code
    }

    /// `Complete Word`, `Ctrl+Space`, and `unluminous-cli editor complete`.
    ///
    /// Works from one character and works inside a comment or a string, where the automatic popup
    /// never opens: somebody who asks in a doc comment deserves the file's words. With no
    /// identifier character to the left of the caret at all it does what every honest miss in Unluminous
    /// does and says so in the status bar.
    pub fn complete_word(&mut self) {
        if !self.completion_applies_here() {
            self.message =
                Some("No plugin claims this file, so Unluminous has no words to offer.".to_owned());
            return;
        }
        let head = self.document().selection().head;
        let text = self.document().text().to_string();
        let grammar = self.completion_grammar();
        let inside_an_import = core_imports::context_at(&text, head, &grammar).is_some();
        let stem = completion::stem_at(&text, head, &grammar);
        let word = text[stem.clone()].to_owned();
        if !self.open_the_completion(head, true) {
            // A notebook's kernel answers a round trip later, and the popup opens when it does.
            if self.kernel_is_being_asked(stem.start, head) {
                self.message = Some("Asking the kernel...".to_owned());
                return;
            }
            self.message = match (word.is_empty(), inside_an_import) {
                (true, true) => Some("There is nothing to import here.".to_owned()),
                (true, false) => Some("There is nothing to complete here.".to_owned()),
                (false, _) => Some(format!("Nothing completes '{word}'.")),
            };
        }
    }

    /// Work out the rows and open the popup on them. False when there was nothing to offer, in
    /// which case nothing opens: a list that lingers empty is a list that says nothing.
    /// The kernel has answered a completion for a notebook cell: work the popup out again with its
    /// names in it, or open it with them when nothing local matched and the caret is still on the word
    /// that was asked about. `task-2220`.
    pub(crate) fn kernel_completions_arrived(&mut self) {
        if self.focus != Focus::Editor {
            return;
        }
        if let Some(state) = self.completion.as_mut() {
            // A revision no edit produces, so the next refresh works the rows out rather than
            // deciding nothing moved.
            state.revision = u64::MAX;
            self.refresh_the_completion();
            return;
        }
        let head = self.document().selection().head;
        let asked_here = self
            .files
            .active()
            .notebook
            .as_deref()
            .and_then(|tab| tab.asked.as_ref())
            .is_some_and(|asked| asked.word_start <= head);
        if asked_here {
            self.open_the_completion(head, true);
        }
    }

    /// What the kernel offers for the word at `offset` in a notebook's code cell.
    ///
    /// **Asked once for each place a word starts.** The first call sends the question and offers
    /// nothing; the answer arrives a round trip later and calls [`Self::kernel_completions_arrived`];
    /// every call after that, while the same word is being typed, filters that one answer. So a word
    /// typed a letter at a time is one question to the kernel rather than one a keystroke.
    fn kernel_candidates(&mut self, stem: &str, offset: usize) -> Vec<Candidate> {
        let index = self.files.active_index();
        let file = self.files.at_mut(index);
        let Some(tab) = file.notebook.as_deref_mut() else { return Vec::new() };
        let Some(cell) = tab.cell_at_offset(offset) else { return Vec::new() };
        let span = tab.spans[cell].clone();
        if span.kind != unluminous_jupyter::nbformat::CellKind::Code
            || offset < span.body_bytes.start
            || offset > span.body_bytes.end
        {
            return Vec::new();
        }
        let word_start = offset.saturating_sub(stem.len());
        let about = tab.asked.as_ref().is_some_and(|asked| {
            let before = file.document.text().byte_slice(span.body_bytes.start..word_start);
            asked.is_about(word_start, &before.to_string(), stem)
        });
        match &tab.asked {
            Some(asked) if about && asked.answered => {
                // What lies between the kernel's start and the editor's, which fits each match to the
                // editor's stem. See `unluminous_jupyter::completion::fit`.
                let (low, high) = match asked.kernel_start <= word_start {
                    true => (asked.kernel_start, word_start),
                    false => (word_start, asked.kernel_start),
                };
                let between = file.document.text().byte_slice(low..high).to_string();
                let (kernel_start, stem_start) = match asked.kernel_start <= word_start {
                    true => (0, between.len()),
                    false => (between.len(), 0),
                };
                asked
                    .matches
                    .iter()
                    .filter_map(|found| {
                        let name = unluminous_jupyter::completion::fit(
                            &found.insert,
                            &between,
                            kernel_start,
                            stem_start,
                        )?;
                        let offered = stem.is_empty() || completion::could_match(stem, &name);
                        offered.then(|| {
                            Candidate::described(
                                name,
                                Source::Kernel,
                                kind_of(found.kind.as_deref()),
                                found.detail(),
                            )
                        })
                    })
                    .collect()
            }
            Some(_) if about => Vec::new(),
            _ => {
                let text = file.document.text().to_string();
                let source = text[span.body_bytes.clone()].to_owned();
                let cursor = text[span.body_bytes.start..offset].chars().count();
                if let Some(kernel) = tab.kernel() {
                    let request = kernel.complete(&source, cursor);
                    tab.asked = Some(crate::app::notebook::Asked {
                        request,
                        word_start,
                        body_start: span.body_bytes.start,
                        source,
                        kernel_start: word_start,
                        typed: stem.to_owned(),
                        matches: Vec::new(),
                        answered: false,
                    });
                }
                Vec::new()
            }
        }
    }

    /// Whether the notebook's kernel has been asked about the word from `word_start` to `offset` and
    /// has not answered yet.
    pub(crate) fn kernel_is_being_asked(&self, word_start: usize, offset: usize) -> bool {
        let file = self.files.active();
        let Some(asked) = file.notebook.as_deref().and_then(|tab| tab.asked.as_ref()) else {
            return false;
        };
        if asked.answered || asked.word_start != word_start || word_start < asked.body_start {
            return false;
        }
        let before = file.document.text().byte_slice(asked.body_start..word_start).to_string();
        let stem = file.document.text().byte_slice(word_start..offset.max(word_start));
        asked.is_about(word_start, &before, &stem.to_string())
    }

    fn open_the_completion(&mut self, offset: usize, manual: bool) -> bool {
        let Some(path) = self.files.active().path().map(Path::to_path_buf) else {
            return false;
        };
        let offer = self.completion_offer(offset);
        if offer.rows.is_empty() || (!manual && nothing_longer(&offer)) {
            self.completion = None;
            return false;
        }
        // Whatever the status bar was saying described the state before this list existed, and a
        // popup opening over a stale sentence reads as an answer to the wrong question.
        self.message = None;
        let place = self.asked_at(offer.range.start).place;
        self.completion = Some(CompletionState {
            place,
            stem: offer.range,
            rows: offer.rows,
            chosen: 0,
            scroll: 0,
            revision: self.document().text_revision(),
            caret: self.document().selection().head,
            manual,
            path,
            import: offer.import,
        });
        true
    }

    /// Close it. Nothing but dropping the state: no animation, no memory, nothing written anywhere.
    pub fn close_the_completion(&mut self) {
        self.completion = None;
    }

    /// True while the popup is open, which is what the key routing and the drawing both ask.
    pub fn completion_is_open(&self) -> bool {
        self.completion.is_some()
    }

    /// What is being offered, for a test and for the command line.
    pub fn completion(&self) -> Option<&CompletionState> {
        self.completion.as_ref()
    }

    /// Where the popup was drawn on the last frame, for a test that has to say it flipped.
    pub fn completion_anchor(&self) -> Option<CompletionAnchor> {
        self.completion_anchor
    }

    /// Recompute the rows, or close the popup, by §5.5's one sentence: *it is open only while it is
    /// an answer to the word being typed at the caret.*
    fn refresh_the_completion(&mut self) {
        let Some(state) = self.completion.as_ref() else {
            return;
        };
        let showing = self.files.active().path().map(Path::to_path_buf);
        if showing.as_deref() != Some(state.path.as_path())
            || !self.completion_applies_here()
            || self.a_modal_is_open()
            || self.focus != Focus::Editor
        {
            self.close_the_completion();
            return;
        }
        let revision = self.document().text_revision();
        let head = self.document().selection().head;
        // Nothing moved. Two integer comparisons and no work at all, which is what a caret blink,
        // a repaint and a frame of idling cost.
        if revision == state.revision && head == state.caret {
            return;
        }
        // The caret moved with no edit behind it: a click, `Left`, `Home`, a jump. The popup is an
        // answer to the word being typed, and that is no longer the word being typed.
        if revision == state.revision {
            self.close_the_completion();
            return;
        }
        // Worked out again at the caret as it is now. What decides whether the popup survives is
        // whether it still has anything to say: `task-1680` replaced the older rule — close when
        // the stem's first byte moved — with this one, which says the same thing everywhere the
        // older one did and also lets a specifier grow a `/` and a module path grow a `::`.
        let offer = self.completion_offer(head);
        let manual = self.completion.as_ref().is_some_and(|state| state.manual);
        if offer.rows.is_empty() || (!manual && nothing_longer(&offer)) {
            // Typing narrowed it to nothing, or to the word already typed. It does not linger; the
            // next character typed asks again.
            self.close_the_completion();
            return;
        }
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        state.stem = offer.range;
        state.chosen = state.chosen.min(offer.rows.len() - 1);
        state.rows = offer.rows;
        state.import = offer.import;
        state.revision = revision;
        state.caret = head;
        state.settle_the_scroll();
    }

    /// The popup's five keys, taken out of the frame's input before any pane reads it.
    ///
    /// Only a **bare** `Tab` is taken, so `Ctrl+Tab` is still `Next Tab` and the three meanings of
    /// the key — move tab, indent, complete — stay on their three distinct chords. The same is true
    /// of every other key here: `Modifiers::NONE` is compared, so nothing with the command key held
    /// is ever swallowed.
    ///
    /// Reading the keys and acting on them are two functions, because they fail in different ways:
    /// what a test of the *meanings* wants is [`Self::the_completion_keys`] with five booleans, and
    /// what a test of the **consumption** wants is a real context with a real key event in it — the
    /// property that a key the popup took never reaches `editor_view::handle_input`.
    pub(crate) fn route_the_completion_keys(&mut self, ui: &egui::Ui) {
        // `Escape` closes the signature line when no list is open to take it first. `task-2231` §6.8.
        if self.completion.is_none()
            && self.signature_open
            && self.focus == Focus::Editor
            && !crate::app::text_box_has_the_keyboard(ui.ctx())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_the_signature();
            return;
        }
        if self.completion.is_none()
            || self.focus != Focus::Editor
            || crate::app::text_box_has_the_keyboard(ui.ctx())
        {
            return;
        }
        // `Enter` on the row that is exactly the word already typed would change nothing, so it is left
        // for the editing area as the new line it means and the list goes. `task-2231` offers that
        // row, as the reference editor does, where `task-1678` used to drop it.
        let typed_already = self.completion.as_ref().is_some_and(|state| {
            let typed = self.document().text().byte_slice(state.stem.clone());
            state.chosen_row().is_some_and(|row| row.name == typed)
        });
        let keys = ui.input_mut(|input| take_the_five_keys(input, !typed_already));
        if typed_already && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            self.close_the_completion();
            return;
        }
        self.the_completion_keys(keys);
    }

    /// What the five keys mean, once they have been read.
    pub fn the_completion_keys(&mut self, keys: CompletionKeys) {
        if self.completion.is_none() {
            return;
        }
        if keys.escape {
            self.close_the_completion();
            return;
        }
        if keys.down {
            self.move_the_completion(1);
        }
        if keys.up {
            self.move_the_completion(-1);
        }
        // `Tab` replaces the whole identifier and `Enter` replaces the stem, which is the reference editor's own
        // distinction and is right in both directions: `Enter` when finishing a fresh word, `Tab`
        // when retyping the front of an existing one.
        if keys.tab {
            self.accept_the_completion(true);
        } else if keys.enter {
            self.accept_the_completion(false);
        }
    }

    /// Move the pill, clamped at the ends. No wrap: a list that jumps from the last row back to the
    /// first is a list you cannot hold `Down` on.
    pub fn move_the_completion(&mut self, delta: i32) {
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        if state.rows.is_empty() {
            return;
        }
        let last = state.rows.len() - 1;
        let wanted = state.chosen as i64 + delta as i64;
        state.chosen = wanted.clamp(0, last as i64) as usize;
        state.settle_the_scroll();
    }

    /// Choose a row by name, which is what a click and `editor complete --choose` both do.
    pub fn choose_the_completion(&mut self, name: &str) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };
        let Some(at) = state.rows.iter().position(|row| row.name == name) else {
            return false;
        };
        state.chosen = at;
        state.settle_the_scroll();
        true
    }

    /// Take the chosen row.
    ///
    /// One `Command::ReplaceMany`, which is one undo step by construction because undo restores a
    /// snapshot — so one press of undo puts back the stem as it was typed. The caret lands at the
    /// end of the inserted name, the marked passages and the selection shift exactly as they do for
    /// every other edit, and the file is marked as changed, because this **is** an edit.
    ///
    /// `whole_word` is `Tab`: the range is the identifier the caret is inside rather than the stem,
    /// so `dra│wing` completed to `draw_frame` does not leave `wing` dangling behind the caret.
    pub fn accept_the_completion(&mut self, whole_word: bool) -> bool {
        let Some(state) = self.completion.take() else {
            return false;
        };
        let Some(row) = state.chosen_row().cloned() else {
            return false;
        };
        let text = self.document().text().to_string();
        let head = self.document().selection().head;
        // `Tab` replaces the whole of what is being written, and inside a specifier the grammar
        // cannot say what the whole of `./lay/out` is — so the reading answers instead.
        let range =
            match whole_word {
                true => state.import.as_ref().and_then(ImportContext::whole_range).unwrap_or_else(
                    || completion::word_at(&text, head, &self.completion_grammar()),
                ),
                false => state.stem.clone(),
            };
        // A range that came out empty is a caret with nothing to its left, which cannot happen while
        // a popup is open; falling back to the stem rather than inserting at a guess keeps it true.
        let range = if range.is_empty() { state.stem.clone() } else { range };
        let (range, inserted, caret_in) = insertion(&row, range, whole_word, &text);
        // The import the name needs, in the same command so one undo takes both away. A server's
        // own edits when it gave them; the structural tier's when the row needs an import and no
        // server answered. `task-2231` §6.5.
        let mut extra: Vec<completion::Edit> = row.info.extra_edits.clone();
        if extra.is_empty() && row.source == Source::Import {
            if let Some(rel) = row.info.needs_import.clone() {
                extra.extend(self.import_edit(&row.name, &rel));
            }
        }
        extra.retain(|edit| edit.range.end <= range.start || edit.range.start >= range.end);
        let shift: isize = extra
            .iter()
            .filter(|edit| edit.range.end <= range.start)
            .map(|edit| edit.text.len() as isize - (edit.range.end - edit.range.start) as isize)
            .sum();
        let caret = (range.start as isize + shift) as usize + caret_in;
        let mut edits = vec![(range.clone(), inserted)];
        edits.extend(extra.into_iter().map(|edit| (edit.range, edit.text)));
        let typed = text.get(state.stem.clone()).unwrap_or_default().to_owned();
        let applied = self.document_mut().apply(Command::ReplaceMany(edits));
        if applied {
            self.document_mut().apply(Command::PlaceCaret { offset: caret, extend: false });
            // Completing into a file you were only glancing at plainly means you meant to open it,
            // which is what typing into one already does.
            let active = self.files.active_index();
            self.files.make_permanent(active);
            let language = self.completion_grammar().language;
            let now = self.completion_clock.elapsed().as_secs_f64();
            self.completion_stats.record(&language, state.place, &typed, &row.name, now);
            // A call with parameters to type, whether the structure inserted it or a server's snippet
            // did, opens the signature line. A server row says so by leaving the caret after `(`.
            let opens_a_call = match &row.info.insert {
                completion::Insert::Call { has_parameters } => *has_parameters,
                completion::Insert::Text { text, caret, .. } => {
                    caret.is_some_and(|at| text[..at.min(text.len())].ends_with('('))
                }
                completion::Insert::Name => false,
            };
            if opens_a_call {
                self.open_the_signature();
            }
        }
        applied
    }
}

/// What accepting a row puts where: the range it replaces, the text, and where the caret goes inside
/// that text. `task-2231` §6.7.
///
/// A call's brackets are added unless a bracket already follows the word, with the caret between them
/// when the function takes parameters and after them when it takes none. A server's own text replaces
/// its own range: `insert` on `Enter`, `replace` on `Tab`.
///
/// @param row - the row
/// @param range - the stem or the word, whichever the key replaces
/// @param whole_word - true for `Tab`
/// @param text - the document's text
fn insertion(
    row: &Row,
    range: Range<usize>,
    whole_word: bool,
    text: &str,
) -> (Range<usize>, String, usize) {
    match &row.info.insert {
        completion::Insert::Name => (range, row.name.clone(), row.name.len()),
        completion::Insert::Call { has_parameters } => {
            if text[range.end..].starts_with('(') {
                return (range, row.name.clone(), row.name.len());
            }
            let caret = row.name.len() + if *has_parameters { 1 } else { 2 };
            (range, format!("{}()", row.name), caret)
        }
        completion::Insert::Text { insert, replace, text: written, caret } => {
            let chosen = if whole_word { replace.clone() } else { insert.clone() };
            // A server's range is for the text it was asked about; a stem typed since only grows it.
            let chosen = chosen.start.min(range.start)..chosen.end.max(range.end);
            (chosen, written.clone(), caret.unwrap_or(written.len()))
        }
    }
}

impl UnluminousApp {
    /// Write down where the popup hangs, from the caret's own box in the pane that has the keyboard.
    ///
    /// The same arithmetic the caret itself is painted with, and taken at the position the frame
    /// settled on rather than the one it opened with — a list anchored to where the caret was before
    /// the wheel had its say would be a frame behind the writing.
    ///
    /// A wheel scroll that leaves the caret on the screen keeps the popup, and one that takes its
    /// line off the screen closes it: an offer hanging off a word nobody can see is not an offer.
    pub(crate) fn remember_where_the_completion_hangs(
        &mut self,
        origin: egui::Pos2,
        area: egui::Rect,
    ) {
        self.completion_anchor = None;
        self.caret_anchor = None;
        if self.completion.is_none() && !self.signature_open {
            return;
        }
        let caret = self.layout().caret_at(self.document().selection().head);
        let box_of_it = egui::Rect::from_min_size(
            egui::Pos2::new(origin.x + caret.x, origin.y + caret.y),
            egui::Vec2::new(2.0, caret.height),
        );
        if box_of_it.bottom() < area.top() || box_of_it.top() > area.bottom() {
            self.close_the_completion();
            return;
        }
        // The signature line hangs from the same caret, and is drawn whether or not a list is open.
        self.caret_anchor = Some(CompletionAnchor { caret: box_of_it, pane: area });
        if self.completion.is_some() {
            self.completion_anchor = self.caret_anchor;
        }
    }

    /// Draw the popup, and take the row a click landed on.
    ///
    /// A click accepts the same way `Enter` does — the stem only — and never reaches the editing
    /// area behind it, because the list's own `Area` is in front and takes the hit.
    pub(crate) fn show_the_completion(&mut self, ui: &mut egui::Ui) {
        let (Some(anchor), Some(state)) = (self.completion_anchor, self.completion.as_ref()) else {
            self.completion_rest = None;
            return;
        };
        // The documentation panel is drawn once the chosen row has rested for one heartbeat, which
        // the window already asks for on every frame, so no timer of its own. `task-2231` §6.6.
        let now = ui.input(|input| input.time);
        let chosen = state.chosen_row().map(|row| row.name.clone()).unwrap_or_default();
        let since = match &self.completion_rest {
            Some((name, since)) if *name == chosen => *since,
            _ => now,
        };
        self.completion_rest = Some((chosen, since));
        let documented = now - since >= crate::app::HEARTBEAT.as_secs_f64();
        if documented {
            self.resolve_the_chosen_completion();
        }
        let Some(state) = self.completion.as_ref() else { return };
        let outcome = view::show(ui, state, anchor.caret, anchor.pane, documented);
        if let Some(name) = outcome.accepted {
            if self.choose_the_completion(&name) {
                self.accept_the_completion(false);
            }
        }
    }
}

/// Take the popup's five keys out of a frame's input, leaving everything else in it.
///
/// Written out rather than five calls to `InputState::consume_key`, and the reason is worth keeping.
/// `consume_key` matches through `Modifiers::matches_logically`, which asks only that the modifiers
/// the *pattern* names are held — so a pattern of `NONE` matches a press with **shift** held as
/// well, and `Shift+Enter`, which is an ordinary new line in the editing area, was being swallowed
/// as an accept. What the popup wants is the bare key and nothing else, so the modifiers are
/// compared for real.
///
/// `Ctrl+Tab` was never at risk, because the command key is the one modifier `matches_logically`
/// does compare both ways — but a rule that holds for one of the four by accident is not a rule.
fn take_the_five_keys(input: &mut egui::InputState, take_enter: bool) -> CompletionKeys {
    let mut keys = CompletionKeys::default();
    input.events.retain(|event| {
        let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
            return true;
        };
        if !modifiers.is_none() {
            return true;
        }
        match key {
            egui::Key::ArrowDown => keys.down = true,
            egui::Key::ArrowUp => keys.up = true,
            egui::Key::Tab => keys.tab = true,
            egui::Key::Enter if take_enter => keys.enter = true,
            egui::Key::Enter => return true,
            egui::Key::Escape => keys.escape = true,
            _ => return true,
        }
        false
    });
    keys
}

/// Whether text ends in a member access: `.` or `::` with a name, a closing bracket or a closing
/// quote in front of it. `3.` is a number, and a dot after a space or at the start of a line has
/// nothing in front of it to be a member of. `task-2229`.
pub fn is_a_member_access(before: &str) -> bool {
    let rest = match before.strip_suffix("::") {
        Some(rest) => rest,
        None => match before.strip_suffix('.') {
            // `..` is a range, and `...` an ellipsis.
            Some(rest) if !rest.ends_with('.') => rest,
            _ => return false,
        },
    };
    let Some(last) = rest.chars().last() else { return false };
    if matches!(last, ')' | ']' | '"' | '\'' | '>') {
        return true;
    }
    if !(last.is_alphanumeric() || last == '_') {
        return false;
    }
    // A run of digits on its own is a number, and `3.` is the start of `3.5`.
    let word: String = rest
        .chars()
        .rev()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    !word.chars().all(|character| character.is_ascii_digit())
}

/// The kind of definition a kernel's type for a match names, for the row's icon and its order.
fn kind_of(kernel: Option<&str>) -> Option<Kind> {
    match kernel? {
        "function" | "magic" => Some(Kind::Function),
        "method" => Some(Kind::Method),
        "macro" => Some(Kind::Macro),
        "class" => Some(Kind::Class),
        "struct" | "union" => Some(Kind::Struct),
        "enum" => Some(Kind::Enum),
        "trait" => Some(Kind::Trait),
        "type" => Some(Kind::Type),
        "module" | "crate" | "namespace" => Some(Kind::Module),
        "field" | "property" => Some(Kind::Field),
        "param" => Some(Kind::Parameter),
        "instance" | "statement" | "variable" | "local" => Some(Kind::Variable),
        "const" | "constant" => Some(Kind::Constant),
        "keyword" => Some(Kind::Keyword),
        _ => None,
    }
}

/// A path's last part, which is what a row says about where a definition came from.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod member_tests {
    use super::*;

    #[test]
    fn a_dot_or_two_colons_after_a_name_or_a_bracket_is_a_member_access() {
        for before in ["df.", "v.", "std::", "f().", "a[0].", "self.items.", "'text'.", "x2."] {
            assert!(is_a_member_access(before), "{before}");
        }
    }

    #[test]
    fn a_number_a_range_and_a_lonely_dot_are_not() {
        for before in ["3.", "12.", "0..", "...", " .", ".", "", "x", "a :", "%"] {
            assert!(!is_a_member_access(before), "{before}");
        }
    }

    #[test]
    fn a_kernels_type_names_the_kind_of_row() {
        assert_eq!(kind_of(Some("function")), Some(Kind::Function));
        assert_eq!(kind_of(Some("module")), Some(Kind::Module));
        assert_eq!(kind_of(Some("instance")), Some(Kind::Variable));
        assert_eq!(kind_of(Some("class")), Some(Kind::Class));
        assert_eq!(kind_of(Some("keyword")), Some(Kind::Keyword));
        assert_eq!(kind_of(Some("snippet")), None);
        assert_eq!(kind_of(None), None);
    }
}


/// True when every row an offer holds is exactly the word already typed, so the list has nothing to
/// complete it to. The automatic list does not open for such an offer, which is what keeps `Enter`
/// meaning a new line once a word is fully typed.
///
/// @param offer - the offer
fn nothing_longer(offer: &Offer) -> bool {
    offer.rows.iter().all(|row| row.name == offer.typed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::actions::{Action, Entry};
    use crate::settings::Suggestions;
    use unluminous_core::Command;

    /// A little project to type into: two Rust files that are opened, one that is not, a stylesheet
    /// and a note.
    ///
    /// `layout.rs` defines five things starting `draw` or near enough to rank against each other,
    /// `distant.rs` is never opened so its `draw_everything` can only come from the index, and
    /// `notes.md` is what a file no plugin claims looks like.
    fn a_project(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(&folder).expect("make the folder");
        std::fs::write(
            folder.join("layout.rs"),
            "pub struct Layout;\n\nimpl Layout {\n    pub fn new() -> Self {\n        Layout\n    }\n\n    pub fn draw(&self) {}\n\n    pub fn draw_frame(&self) {}\n\n    pub fn redraw(&self) {}\n\n    // draw the whole page\n    pub fn paint_text(&self) {}\n}\n",
        )
        .expect("write layout.rs");
        std::fs::write(
            folder.join("caret.rs"),
            "pub struct Caret;\n\nimpl Caret {\n    pub fn new() -> Self {\n        Caret\n    }\n\n    pub fn paint(&self, layout: &Layout) {\n        layout.draw();\n    }\n}\n",
        )
        .expect("write caret.rs");
        std::fs::write(folder.join("distant.rs"), "pub fn draw_everything() {}\n")
            .expect("write distant.rs");
        std::fs::write(folder.join("site.css"), ".card {\n  --brand-hue: 280;\n}\n")
            .expect("write site.css");
        std::fs::write(folder.join("notes.md"), "# draw\nA note about drawing.\n")
            .expect("write notes.md");
        folder
    }

    /// A project with more names in it than one stem may draw from the index.
    ///
    /// `task-1984` C12. The names are all in a file that is never opened, so the only way any of
    /// them can reach the pool is the project index — which is the source the cap is on.
    fn a_project_with_many_names(name: &str, many: usize) -> PathBuf {
        let folder = a_project(name);
        let mut source = String::new();
        for index in 0..many {
            source.push_str(&format!("pub fn drawing_number_{index}() {{}}\n"));
        }
        std::fs::write(folder.join("many.rs"), source).expect("write many.rs");
        folder
    }

    /// One stem never draws more than [`MOST_FROM_THE_INDEX`] names out of the project index.
    ///
    /// **`task-1984` C12.** `task-1677` §7 set one budget — gathering, scoring and sorting a stem
    /// under 5 ms on the largest file in this repository — and named the answer if a project ever
    /// broke it: *"capping the pool (an honest `LIMIT`, the references modal's pattern), not a
    /// thread"*. A project did break it, so the pool is capped, and this is the cap holding.
    ///
    /// **The other half is what is *not* capped**, which matters more than the number: the pool is
    /// gathered in order of what a row is worth, and the cap is on the project index alone. This
    /// file's own definitions, its words, the other open tabs and the language's own keywords all
    /// come through whole — a keyword the language defines is always a right answer, and there are
    /// only ever a handful of them.
    #[test]
    fn one_stem_draws_no_more_than_the_cap_from_the_project_index() {
        let folder =
            a_project_with_many_names("unluminous-completion-cap", MOST_FROM_THE_INDEX * 2);
        let mut app = UnluminousApp::new(&folder);
        build_the_index(&mut app);
        app.open_path_permanently(&folder.join("layout.rs")).expect("the file opens");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });

        let pool = app.completion_candidates("dr", end);
        // A project name a file does not import comes as needing its import (`task-2231` §6.5), and
        // it is still the project index it came from.
        let from_the_index =
            pool.iter().filter(|one| matches!(one.source, Source::Index | Source::Import)).count();
        assert!(
            from_the_index > 0,
            "the fixture really does reach the index, or this test is about nothing"
        );
        assert!(
            from_the_index <= MOST_FROM_THE_INDEX,
            "{from_the_index} names came from the index, past the cap of {MOST_FROM_THE_INDEX}"
        );
        // And this file's own `draw`, `draw_frame` and `redraw` are still there, which is the half
        // of the rule the cap must not touch.
        let here: Vec<&str> = pool
            .iter()
            .filter(|one| one.source == Source::ThisFile)
            .map(|one| one.name.as_str())
            .collect();
        for named in ["draw", "draw_frame", "redraw"] {
            assert!(here.contains(&named), "{named} is missing from {here:?}");
        }
        std::fs::remove_dir_all(&folder).ok();
    }

    /// A window on that project, its index built, with `layout.rs` open and the caret at the end.
    fn a_window(name: &str) -> (PathBuf, UnluminousApp) {
        let folder = a_project(name);
        let mut app = UnluminousApp::new(&folder);
        build_the_index(&mut app);
        app.open_path_permanently(&folder.join("layout.rs")).expect("the file opens");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        (folder, app)
    }

    /// Read the project and wait for the thread, which is what a frame of the real window does over
    /// however many frames it takes.
    fn build_the_index(app: &mut UnluminousApp) {
        app.the_project_changed_on_disk();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            app.keep_the_symbol_index_fresh();
            if app.symbols_indexer().is_some_and(|indexer| !indexer.is_building()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the index should have been built");
    }

    /// Type, one character at a time, exactly as the window does: the letter lands in the document
    /// and then the popup is worked for that frame.
    fn typing(app: &mut UnluminousApp, text: &str) {
        for character in text.chars() {
            app.document_mut().apply(Command::Insert(character.to_string()));
            app.keep_the_completion_fresh(true);
        }
    }

    /// A frame in which nothing was typed, which is what everything that is not typing looks like.
    fn a_quiet_frame(app: &mut UnluminousApp) {
        app.keep_the_completion_fresh(false);
    }

    /// Put the file back as it was and type the stem again, so a test that presses several things in
    /// turn presses each of them against the same open popup rather than against whatever the last
    /// one left behind.
    fn an_open_popup(app: &mut UnluminousApp, original: &str) {
        app.close_the_completion();
        let whole = 0..app.document().text().len_bytes();
        app.document_mut().apply(Command::ReplaceMany(vec![(whole, original.to_owned())]));
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        typing(app, "dra");
    }

    /// The names on offer, in order.
    fn offered(app: &UnluminousApp) -> Vec<String> {
        app.completion()
            .map(|state| state.rows.iter().map(|row| row.name.clone()).collect())
            .unwrap_or_default()
    }

    fn text_of(app: &UnluminousApp) -> String {
        app.document().text().to_string()
    }

    #[test]
    fn typing_the_second_letter_of_a_word_opens_the_list_with_the_best_row_chosen() {
        // Scenario 12. One letter is not enough — that is scenario 11 — and the second one is.
        let (folder, mut app) = a_window("unluminous-completion-opens");
        typing(&mut app, "d");
        assert!(app.completion().is_none(), "one character is noise, not an offer");
        typing(&mut app, "r");
        let state = app.completion().expect("the popup opened on the second letter");
        assert_eq!(
            state.chosen, 0,
            "the first row is pre-chosen, so Tab alone takes the best match"
        );
        assert_eq!(state.rows[0].name, "draw", "which is the shortest thing starting with `dr`");
        assert!(offered(&app).contains(&"draw_frame".to_owned()), "{:?}", offered(&app));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_one_character_stem_asks_the_pool_for_nothing_at_all() {
        // Scenario 11 at the layer that decides it: the automatic path never even gathers.
        let (folder, mut app) = a_window("unluminous-completion-one-letter");
        typing(&mut app, "d");
        assert!(app.completion().is_none());
        // And an empty stem answers nothing however it is asked.
        assert!(app.completion_candidates("", 0).is_empty());
        assert!(app.completion_rows("", 0).is_empty());
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn nothing_ever_opens_in_a_file_no_plugin_claims() {
        // Scenario 13. Absent rather than dimmed, which is Unluminous's rule for a control that can never
        // apply: the menu entry is not there either.
        let (folder, mut app) = a_window("unluminous-completion-prose");
        app.open_path_permanently(&folder.join("notes.md")).expect("the file opens");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        typing(&mut app, "draw");
        assert!(app.completion().is_none(), "prose has no words worth offering");
        assert!(!app.completion_applies_here());
        app.complete_word();
        assert!(app.completion().is_none(), "and asking by hand says so rather than opening");
        assert!(app.message.is_some());
        let entries = crate::app::actions::completion_entries(&app.menu_state());
        assert!(
            !entries
                .iter()
                .any(|entry| matches!(entry, Entry::Item { action: Action::CompleteWord, .. })),
            "the menu entry is absent for a note"
        );
        // A stylesheet is the opposite: no definers, but its own words and keywords are real offers.
        app.open_path_permanently(&folder.join("site.css")).expect("the file opens");
        assert!(app.completion_applies_here(), "CSS completes");
        let entries = crate::app::actions::completion_entries(&app.menu_state());
        assert!(entries
            .iter()
            .any(|entry| matches!(entry, Entry::Item { action: Action::CompleteWord, .. })));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_css_file_completes_its_own_custom_properties() {
        // The other half of scenario 9, in the window: CSS names no definers, so source 1 — this
        // file's words — is the only thing it has, and it is enough.
        let (folder, mut app) = a_window("unluminous-completion-css");
        app.open_path_permanently(&folder.join("site.css")).expect("the file opens");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        typing(&mut app, "--br");
        let rows = offered(&app);
        assert!(rows.contains(&"--brand-hue".to_owned()), "{rows:?}");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_automatic_popup_stays_out_of_a_comment_and_asking_by_hand_opens_one_there() {
        // Scenario 14. A doc comment's prose does not want a list flickering over it; somebody who
        // asks in one deserves the file's words.
        let (folder, mut app) = a_window("unluminous-completion-comment");
        let inside = text_of(&app).find("the whole page").expect("the comment") + "the ".len();
        app.document_mut().apply(Command::PlaceCaret { offset: inside, extend: false });
        typing(&mut app, "dr");
        assert!(app.completion().is_none(), "nothing arrives unasked inside a comment");
        app.complete_word();
        let rows = offered(&app);
        assert!(rows.contains(&"draw".to_owned()), "but asking gets the file's words: {rows:?}");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn typing_on_until_nothing_matches_closes_it_and_the_next_letter_asks_again() {
        // Scenario 15. It does not linger empty, and it does not reopen on its own.
        let (folder, mut app) = a_window("unluminous-completion-narrows");
        typing(&mut app, "dra");
        assert!(app.completion().is_some());
        typing(&mut app, "z");
        assert!(app.completion().is_none(), "`draz` matches nothing here");
        a_quiet_frame(&mut app);
        assert!(app.completion().is_none(), "and an idle frame does not bring it back");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn backspace_inside_the_word_keeps_it_open_and_refilters_it() {
        // Scenario 16.
        let (folder, mut app) = a_window("unluminous-completion-backspace");
        typing(&mut app, "draw_");
        let narrow = offered(&app);
        assert!(narrow.contains(&"draw_frame".to_owned()), "{narrow:?}");
        assert!(!narrow.contains(&"redraw".to_owned()), "the underscore ruled it out: {narrow:?}");
        app.document_mut().apply(Command::DeleteBackward);
        app.keep_the_completion_fresh(false);
        let wider = offered(&app);
        assert!(app.completion().is_some(), "still open");
        assert!(wider.contains(&"redraw".to_owned()), "and refiltered: {wider:?}");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn typing_a_word_boundary_closes_it() {
        // Scenario 17. The stem is gone, so there is no word being typed to be an answer to.
        for boundary in ["(", " ", "."] {
            let (folder, mut app) = a_window("unluminous-completion-boundary");
            typing(&mut app, "dra");
            assert!(app.completion().is_some());
            typing(&mut app, boundary);
            assert!(app.completion().is_none(), "{boundary} ends the word");
            std::fs::remove_dir_all(&folder).ok();
        }
    }

    #[test]
    fn the_caret_moving_by_anything_but_typing_closes_it() {
        // Scenario 18: a click, an arrow, Home, a jump — every one of them is a caret that moved
        // with no edit behind it, which is one rule rather than six.
        let (folder, mut app) = a_window("unluminous-completion-caret-moved");
        let original = text_of(&app);
        for movement in [
            Command::MoveLeft { extend: false },
            Command::MoveLineStart { extend: false },
            Command::MoveDocumentStart { extend: false },
            Command::PlaceCaret { offset: 0, extend: false },
        ] {
            an_open_popup(&mut app, &original);
            assert!(app.completion().is_some(), "{movement:?} needs a popup to close");
            app.document_mut().apply(movement.clone());
            a_quiet_frame(&mut app);
            assert!(app.completion().is_none(), "{movement:?} closed it");
        }
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_modal_opening_and_the_tab_changing_both_close_it() {
        // The rest of scenario 18, and scenario 32's second half.
        let (folder, mut app) = a_window("unluminous-completion-modal");
        typing(&mut app, "dra");
        assert!(app.completion().is_some());
        app.settings_window.open = true;
        a_quiet_frame(&mut app);
        assert!(app.completion().is_none(), "a modal owns the keyboard");
        app.settings_window.open = false;

        typing(&mut app, "w");
        typing(&mut app, "_");
        assert!(app.completion().is_some(), "{:?}", offered(&app));
        app.open_path_permanently(&folder.join("caret.rs")).expect("the file opens");
        a_quiet_frame(&mut app);
        assert!(app.completion().is_none(), "the popup goes with the tab it belonged to");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn escape_closes_it_and_leaves_the_document_alone() {
        // Scenario 19. Consumed, so it cannot also clear a selection.
        let (folder, mut app) = a_window("unluminous-completion-escape");
        typing(&mut app, "dra");
        let before = text_of(&app);
        app.document_mut().apply(Command::MoveLeft { extend: true });
        app.the_completion_keys(CompletionKeys::escape());
        assert!(app.completion().is_none());
        assert_eq!(text_of(&app), before, "Escape is not an edit");
        assert!(!app.document().selection().is_empty(), "and the selection survives it");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn manual_suggestions_stop_the_unasked_popup_and_keep_the_asked_one() {
        // Scenario 20. `manual` is already the off switch, which is why there is no third value.
        let (folder, mut app) = a_window("unluminous-completion-manual");
        app.settings.suggestions = Suggestions::Manual;
        typing(&mut app, "draw");
        assert!(app.completion().is_none(), "nothing arrives unasked");
        app.complete_word();
        assert!(app.completion().is_some(), "and Ctrl+Space still works: {:?}", app.message);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn asking_with_no_word_to_the_left_of_the_caret_says_so_and_opens_nothing() {
        // Scenario 21, and it works from one character, which the automatic path does not.
        let (folder, mut app) = a_window("unluminous-completion-nothing-there");
        typing(&mut app, " ");
        app.complete_word();
        assert!(app.completion().is_none());
        assert_eq!(app.message.as_deref(), Some("There is nothing to complete here."));
        app.message = None;
        typing(&mut app, "d");
        assert!(app.completion().is_none(), "one letter is still not an unasked offer");
        app.complete_word();
        assert!(app.completion().is_some(), "but asking from one letter works: {:?}", app.message);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn only_the_pane_with_the_keyboard_has_a_popup() {
        // Scenario 23. One `Option` on the window is what makes "at most one" true by construction;
        // what has to be shown is that it belongs to the pane being typed into.
        let (folder, mut app) = a_window("unluminous-completion-split");
        app.open_path_permanently(&folder.join("caret.rs")).expect("the file opens");
        let context = egui::Context::default();
        app.run_action(Action::SplitRight, &context);
        assert_eq!(app.files.pane_count(), 2, "the editing area is split");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        typing(&mut app, "pai");
        assert!(app.completion().is_some(), "the pane with the keyboard has it");
        app.run_action(Action::PreviousPane, &context);
        a_quiet_frame(&mut app);
        assert!(app.completion().is_none(), "and the keyboard moving away closes it");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn down_twice_then_enter_replaces_the_stem_and_one_undo_puts_it_back() {
        // Scenario 24, and scenario 31 with it: the third row is the one from the file that is not
        // open, so accepting it proves nothing is opened and nothing is read from the disk.
        let (folder, mut app) = a_window("unluminous-completion-accept");
        let before = text_of(&app);
        typing(&mut app, "dra");
        assert_eq!(
            offered(&app),
            vec!["draw", "draw_frame", "draw_everything", "redraw"],
            "the order the rubric gives"
        );
        app.the_completion_keys(CompletionKeys::down());
        app.the_completion_keys(CompletionKeys::down());
        assert_eq!(app.completion().expect("open").chosen, 2);
        app.the_completion_keys(CompletionKeys::enter());
        assert!(app.completion().is_none(), "accepting closes it");
        // A function is inserted with its call brackets, the caret after them when it takes no
        // parameters (`task-2231` §6.7).
        assert!(text_of(&app).ends_with("draw_everything()"), "{:?}", text_of(&app));
        assert_eq!(
            app.document().selection().head,
            app.document().text().len_bytes(),
            "the caret lands after the inserted name"
        );
        assert_eq!(
            app.files.active().path(),
            Some(folder.join("layout.rs").as_path()),
            "nothing was opened to insert a name from a closed file"
        );
        app.document_mut().apply(Command::Undo);
        assert!(text_of(&app).ends_with("dra"), "one step puts the stem back: {:?}", text_of(&app));
        assert!(text_of(&app).starts_with(&before[..before.len() - 1]));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_pill_is_clamped_at_the_ends_and_the_list_scrolls_with_it() {
        // Scenario 25. No wrap: a list that jumps from the last row back to the first is one you
        // cannot hold `Down` on.
        let (folder, mut app) = a_window("unluminous-completion-steering");
        typing(&mut app, "dr");
        let rows = app.completion().expect("open").rows.len();
        app.the_completion_keys(CompletionKeys::up());
        assert_eq!(app.completion().expect("open").chosen, 0, "clamped at the top");
        for _ in 0..rows + 5 {
            app.the_completion_keys(CompletionKeys::down());
        }
        let state = app.completion().expect("open");
        assert_eq!(state.chosen, rows - 1, "clamped at the bottom");
        assert!(state.shown().contains(&state.chosen), "and the list scrolled to it");
        assert!(state.shown().len() <= VISIBLE_ROWS, "eight rows at most");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_list_scrolls_when_the_pill_walks_off_the_eight_that_are_drawn() {
        // The other half of scenario 25, which needs more rows than the list draws.
        let (folder, mut app) = a_window("unluminous-completion-scrolls");
        typing(&mut app, "ra");
        let rows = app.completion().expect("open").rows.len();
        assert!(rows > VISIBLE_ROWS, "the fixture has to offer more than eight: {rows}");
        assert_eq!(app.completion().expect("open").scroll, 0, "it starts at the top");
        for _ in 0..VISIBLE_ROWS {
            app.the_completion_keys(CompletionKeys::down());
        }
        let state = app.completion().expect("open");
        assert!(state.scroll > 0, "walking past the eighth row drags the list");
        assert!(state.shown().contains(&state.chosen));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn tab_replaces_the_whole_word_and_enter_replaces_only_the_stem() {
        // Scenarios 26 and 27, the pair the reference editor keeps apart and this design keeps apart with it.
        for (whole_word, expected) in [(true, "draw_frame;"), (false, "draw_framewing;")] {
            let (folder, mut app) = a_window("unluminous-completion-mid-word");
            typing(&mut app, "drawing;");
            let at = text_of(&app).find("drawing").expect("the word") + "dra".len();
            app.document_mut().apply(Command::PlaceCaret { offset: at, extend: false });
            app.complete_word();
            assert!(app.choose_the_completion("draw_frame"), "{:?}", offered(&app));
            app.the_completion_keys(if whole_word {
                CompletionKeys::tab()
            } else {
                CompletionKeys::enter()
            });
            assert!(text_of(&app).ends_with(expected), "{:?}", text_of(&app));
            std::fs::remove_dir_all(&folder).ok();
        }
    }

    #[test]
    fn typing_while_it_is_open_lands_in_the_document_and_refilters_the_list() {
        // Scenario 29, in that order: the letters are not consumed, and the list narrows to them.
        let (folder, mut app) = a_window("unluminous-completion-typing-through");
        typing(&mut app, "dr");
        let wide = offered(&app);
        typing(&mut app, "aw_f");
        assert!(text_of(&app).ends_with("draw_f"), "every letter reached the file");
        let narrow = offered(&app);
        assert!(narrow.len() < wide.len(), "{wide:?} narrowed to {narrow:?}");
        assert_eq!(narrow, vec!["draw_frame".to_owned()]);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_click_on_a_row_accepts_it_exactly_as_enter_does() {
        // Scenario 30 at the layer that decides what a click means; the click itself is the
        // screenshot test's.
        let (folder, mut app) = a_window("unluminous-completion-click");
        typing(&mut app, "dra");
        assert!(app.choose_the_completion("redraw"));
        assert!(app.accept_the_completion(false));
        assert!(text_of(&app).ends_with("redraw"), "{:?}", text_of(&app));
        assert!(app.completion().is_none());
        // And a name that is not on offer changes nothing at all.
        typing(&mut app, "_x");
        assert!(!app.choose_the_completion("nothing_offers_this"));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_popup_takes_exactly_five_keys_and_only_while_it_is_open() {
        // The **non-interference** invariant. With it shut, nothing is consumed and every key means
        // what it meant before this ticket; with it open, exactly the five of §5.3 are taken out of
        // the frame and everything else — the letters above all — flows through.
        let (folder, mut app) = a_window("unluminous-completion-non-interference");
        let original = text_of(&app);
        let five = [
            egui::Key::ArrowDown,
            egui::Key::ArrowUp,
            egui::Key::Tab,
            egui::Key::Enter,
            egui::Key::Escape,
        ];
        for key in five {
            assert!(app.completion().is_none());
            assert!(!pressing(&mut app, key, egui::Modifiers::NONE), "{key:?} with it shut");
        }
        for key in five {
            an_open_popup(&mut app, &original);
            assert!(pressing(&mut app, key, egui::Modifiers::NONE), "{key:?} with it open");
        }
        // Everything else is left alone, including a `Tab` with the control key held, which is
        // `Next Tab` and must stay `Next Tab`.
        for (key, modifiers) in [
            (egui::Key::Tab, egui::Modifiers::COMMAND),
            (egui::Key::ArrowLeft, egui::Modifiers::NONE),
            (egui::Key::ArrowRight, egui::Modifiers::NONE),
            (egui::Key::Home, egui::Modifiers::NONE),
            (egui::Key::Backspace, egui::Modifiers::NONE),
            (egui::Key::Enter, egui::Modifiers::SHIFT),
        ] {
            an_open_popup(&mut app, &original);
            assert!(
                !pressing(&mut app, key, modifiers),
                "{key:?} {modifiers:?} is not the popup's"
            );
        }
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn enter_on_the_word_already_typed_is_a_new_line_and_closes_the_list() {
        // `task-2231` offers the typed word as a row, first, and `Enter` on it must still be the new
        // line somebody who finished typing a word meant.
        let (folder, mut app) = a_window("unluminous-completion-enter-on-the-typed-word");
        typing(&mut app, "draw");
        let state = app.completion().expect("draw_frame is longer, so the list is open");
        assert_eq!(state.chosen_row().map(|row| row.name.as_str()), Some("draw"));
        assert!(!pressing(&mut app, egui::Key::Enter, egui::Modifiers::NONE), "Enter is left for the new line");
        assert!(app.completion().is_none(), "and the list goes");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_word_typed_in_full_with_nothing_longer_opens_no_list() {
        let (folder, mut app) = a_window("unluminous-completion-nothing-longer");
        typing(&mut app, "redraw");
        assert!(app.completion().is_none(), "{:?}", offered(&app));
        std::fs::remove_dir_all(&folder).ok();
    }

    /// Press one key at a real `egui::Context` and say whether the popup took it out of the frame.
    ///
    /// The consumption is the property, so it is measured the only way it can be: the event is put
    /// into a frame, the routing runs, and what is left in the frame afterwards is looked at.
    fn pressing(app: &mut UnluminousApp, key: egui::Key, modifiers: egui::Modifiers) -> bool {
        let context = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        let mut left = 0;
        let output = context.run_ui(input, |ui| {
            app.route_the_completion_keys(ui);
            left = ui.input(|input| {
                input
                    .events
                    .iter()
                    .filter(|event| matches!(event, egui::Event::Key { key: pressed, .. } if *pressed == key))
                    .count()
            });
        });
        output.drop_without_applying_deltas();
        left == 0
    }

    #[test]
    fn the_index_answers_for_closed_files_and_never_for_open_ones() {
        // The ownership rule of `task-1675` §3.3, at the completion end: a name being edited in a
        // tab must never be offered twice, once live and once as the disk last saw it.
        let (folder, mut app) = a_window("unluminous-completion-ownership");
        let rows = app.completion_rows("dra", 0);
        let everything: Vec<&Row> =
            rows.iter().filter(|row| row.name == "draw_everything").collect();
        assert_eq!(everything.len(), 1, "one row for it: {rows:?}");
        // From the index, and needing its import, since nothing in `layout.rs` imports it.
        assert_eq!(everything[0].source, Source::Import, "and it comes from the index");
        assert!(everything[0].info.needs_import.is_some());
        assert_eq!(everything[0].detail, "distant.rs");
        let here: Vec<&Row> = rows.iter().filter(|row| row.name == "draw_frame").collect();
        assert_eq!(here.len(), 1, "and the open file's own definition is not doubled: {here:?}");
        assert_eq!(here[0].source, Source::ThisFile);

        // Open the file the index knew about, and the index's copy of it stops being offered.
        app.open_path_permanently(&folder.join("distant.rs")).expect("the file opens");
        let index = app.files.active_index();
        let _ = index;
        app.open_path_permanently(&folder.join("layout.rs")).expect("the file opens");
        let rows = app.completion_rows("dra", 0);
        let everything: Vec<&Row> =
            rows.iter().filter(|row| row.name == "draw_everything").collect();
        assert_eq!(everything.len(), 1, "still one row: {rows:?}");
        assert_eq!(everything[0].source, Source::OpenTab, "now from the tab, read live");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_language_s_own_words_are_offered_and_labelled_as_such() {
        // Source 3: the manifest already holds them, and completion is the second reader of it.
        let (folder, mut app) = a_window("unluminous-completion-keywords");
        let rows = app.completion_rows("str", 0);
        let keyword = rows.iter().find(|row| row.name == "struct").expect("`struct`: {rows:?}");
        assert_eq!(keyword.source, Source::Language);
        assert_eq!(keyword.detail, "keyword");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn the_position_in_a_tag_decides_which_language_words_are_offered() {
        // `task-1694` §6.4: a tag name offers the element names, an attribute offers the attribute
        // names, and prose offers none of them. The same stem, asked at three positions of one file,
        // answers three different ways, which is what the filter is for. Each name is one the file
        // does not contain, so if it is offered it came from the language's list and not the file's
        // own words.
        let folder = std::env::temp_dir().join("unluminous-completion-markup");
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(&folder).expect("make the folder");
        std::fs::write(folder.join("page.html"), "<div class=\"card\">Hello world</div>\n")
            .expect("write page.html");
        let mut app = UnluminousApp::new(&folder);
        app.open_path_permanently(&folder.join("page.html")).expect("the file opens");

        // Inside the first word of a tag, so the element names are on offer.
        let tag_name = app.completion_rows("ta", 3);
        assert!(
            tag_name.iter().any(|row| row.name == "table" && row.source == Source::Language),
            "an element name at a tag name: {tag_name:?}"
        );

        // Inside an attribute name, so the attribute names are on offer.
        let attribute = app.completion_rows("pl", 7);
        assert!(
            attribute.iter().any(|row| row.name == "placeholder" && row.source == Source::Language),
            "an attribute name at an attribute: {attribute:?}"
        );

        // In the body, where no language word is on offer, so the element name the tag-name position
        // offered is not here.
        let prose = app.completion_rows("ta", 20);
        assert!(
            !prose.iter().any(|row| row.name == "table"),
            "no element name in prose: {prose:?}"
        );

        std::fs::remove_dir_all(&folder).ok();
    }

    // Import completion (`task-1680`). The two families, each against a project shaped like one a
    // person really writes in.

    /// A TypeScript project: a file to type in, two siblings, a folder with an index, and a note.
    fn a_web_project(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(folder.join("src/app/widgets")).expect("make the folders");
        std::fs::create_dir_all(folder.join("src/core")).expect("make the core folder");
        std::fs::write(folder.join("src/app/mod.ts"), "").expect("write mod.ts");
        std::fs::write(
            folder.join("src/app/layout.ts"),
            "export class Layout {}\n\nexport function drawFrame() {\n  const hidden = 1;\n  return hidden;\n}\n\nconst secret = 2;\n\nexport const LIMIT = 8;\n",
        )
        .expect("write layout.ts");
        std::fs::write(folder.join("src/app/widgets/index.ts"), "export class Button {}\n")
            .expect("write index.ts");
        std::fs::write(folder.join("src/app/widgets/button.tsx"), "export class Pressed {}\n")
            .expect("write button.tsx");
        std::fs::write(folder.join("src/core/completion.ts"), "export function rank() {}\n")
            .expect("write completion.ts");
        std::fs::write(folder.join("src/notes.md"), "# a note\n").expect("write notes.md");
        folder
    }

    /// A window on it, its index built, with `src/app/mod.ts` open and the caret at the end.
    fn a_web_window(name: &str) -> (PathBuf, UnluminousApp) {
        let folder = a_web_project(name);
        let mut app = UnluminousApp::new(&folder);
        build_the_index(&mut app);
        app.open_path_permanently(&folder.join("src/app/mod.ts")).expect("the file opens");
        (folder, app)
    }

    /// A Rust workspace shaped like Unluminous's own: two packages, each with a source root.
    fn a_workspace(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(name);
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(folder.join("crates/unluminous-core/src")).expect("make the core");
        std::fs::create_dir_all(folder.join("crates/unluminous-app/src/app"))
            .expect("make the app");
        std::fs::write(folder.join("crates/unluminous-core/src/lib.rs"), "pub mod completion;\n")
            .expect("write core lib.rs");
        std::fs::write(
            folder.join("crates/unluminous-core/src/completion.rs"),
            "pub struct Candidate;\n\npub fn rank() {}\n\nfn hidden() {}\n",
        )
        .expect("write completion.rs");
        std::fs::write(folder.join("crates/unluminous-app/src/lib.rs"), "pub mod app;\n")
            .expect("write app lib.rs");
        std::fs::write(folder.join("crates/unluminous-app/src/app/mod.rs"), "")
            .expect("write app/mod.rs");
        std::fs::write(
            folder.join("crates/unluminous-app/src/app/actions.rs"),
            "pub enum Action {}\n\npub fn menus() {}\n",
        )
        .expect("write actions.rs");
        folder
    }

    /// A window on it, with `crates/unluminous-app/src/app/mod.rs` open.
    fn a_rust_window(name: &str) -> (PathBuf, UnluminousApp) {
        let folder = a_workspace(name);
        let mut app = UnluminousApp::new(&folder);
        build_the_index(&mut app);
        app.open_path_permanently(&folder.join("crates/unluminous-app/src/app/mod.rs"))
            .expect("the file opens");
        (folder, app)
    }

    /// Put a line in the open file and ask for the list at the `|`, which is taken out first.
    fn ask_at(app: &mut UnluminousApp, line: &str) {
        let caret = line.find('|').expect("the sample marks the caret with |");
        let whole = 0..app.document().text().len_bytes();
        app.document_mut().apply(Command::ReplaceMany(vec![(whole, line.replace('|', ""))]));
        app.document_mut().apply(Command::PlaceCaret { offset: caret, extend: false });
        app.close_the_completion();
        app.complete_word();
    }

    #[test]
    fn a_specifier_offers_the_projects_own_files_and_nothing_else() {
        // Scenarios 41, 42, 43 and 51.
        let (folder, mut app) = a_web_window("unluminous-import-specifier");
        typing(&mut app, "import { Layout } from '");
        let rows = offered(&app);
        assert!(rows.contains(&"./layout".to_owned()), "{rows:?}");
        assert!(rows.contains(&"./widgets".to_owned()), "a folder's index is the folder: {rows:?}");
        assert!(rows.contains(&"./widgets/button".to_owned()), "{rows:?}");
        assert!(rows.contains(&"../core/completion".to_owned()), "{rows:?}");
        assert!(!rows.iter().any(|row| row.ends_with(".ts")), "the extension is dropped: {rows:?}");
        assert!(!rows.iter().any(|row| row.contains("notes")), "a note is not TypeScript");
        // Scenario 51: none of the four ordinary sources reaches an import.
        assert!(!rows.contains(&"import".to_owned()), "no keyword: {rows:?}");
        assert!(!rows.contains(&"Layout".to_owned()), "no name from the project: {rows:?}");
        for row in app.completion().expect("open").rows.iter() {
            assert_eq!(row.source, Source::Module, "{}", row.name);
        }
        // And typing narrows it, exactly as it narrows a word.
        typing(&mut app, "./wid");
        assert_eq!(offered(&app), vec!["./widgets".to_owned(), "./widgets/button".to_owned()]);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_named_import_offers_what_the_module_exports_and_not_what_it_hides() {
        // Scenarios 44 and 46: a `const` inside a function body is a definition and is not
        // something another file can name.
        let (folder, mut app) = a_web_window("unluminous-import-named");
        ask_at(&mut app, "import { | } from './layout'");
        let rows = offered(&app);
        assert!(rows.contains(&"Layout".to_owned()), "{rows:?}");
        assert!(rows.contains(&"drawFrame".to_owned()), "{rows:?}");
        assert!(rows.contains(&"LIMIT".to_owned()), "{rows:?}");
        assert!(!rows.contains(&"hidden".to_owned()), "a local is not an export: {rows:?}");
        assert!(!rows.contains(&"secret".to_owned()), "nor is an unexported const: {rows:?}");
        let row = &app.completion().expect("open").rows[0];
        assert_eq!(row.source, Source::Index, "a closed module is owned by the index");
        assert_eq!(row.detail, "layout.ts");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_module_that_is_open_answers_from_its_live_text() {
        // Scenario 45, which is `task-1675` §3.3's ownership rule reaching one more feature: a
        // function added in the tab beside this one is offered before it is saved.
        let (folder, mut app) = a_web_window("unluminous-import-live");
        app.open_path_permanently(&folder.join("src/app/layout.ts")).expect("the file opens");
        let end = app.document().text().len_bytes();
        app.document_mut().apply(Command::PlaceCaret { offset: end, extend: false });
        app.document_mut().apply(Command::Insert("\nexport function justAdded() {}\n".to_owned()));
        app.open_path_permanently(&folder.join("src/app/mod.ts")).expect("the file opens");
        ask_at(&mut app, "import { just| } from './layout'");
        assert_eq!(offered(&app), vec!["justAdded".to_owned()]);
        assert_eq!(app.completion().expect("open").rows[0].source, Source::OpenTab);
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_module_that_resolves_to_nothing_offers_nothing() {
        // Scenarios 15 and 40 at this layer: guessing would be worse than saying nothing.
        let (folder, mut app) = a_web_window("unluminous-import-unresolved");
        ask_at(&mut app, "import { | } from './nowhere'");
        assert!(app.completion().is_none(), "{:?}", offered(&app));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn tab_replaces_the_whole_specifier_and_enter_replaces_what_was_typed() {
        // Scenario 48. `completion::word_at` cannot answer this: a specifier is not made of word
        // characters, so the reading has to carry the range.
        let (folder, mut app) = a_web_window("unluminous-import-accept");
        ask_at(&mut app, "import { Layout } from './la|zy'");
        assert!(app.choose_the_completion("./layout"), "{:?}", offered(&app));
        app.accept_the_completion(false);
        assert_eq!(text_of(&app), "import { Layout } from './layoutzy'", "Enter takes the stem");

        ask_at(&mut app, "import { Layout } from './la|zy'");
        assert!(app.choose_the_completion("./layout"));
        app.accept_the_completion(true);
        assert_eq!(text_of(&app), "import { Layout } from './layout'", "Tab takes the whole of it");

        // And with nothing typed at all, which is the one place a completion replaces no bytes.
        ask_at(&mut app, "import { Layout } from '|'");
        assert!(app.choose_the_completion("./layout"), "{:?}", offered(&app));
        app.accept_the_completion(false);
        assert_eq!(text_of(&app), "import { Layout } from './layout'");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn accepting_an_import_row_is_one_undo_step() {
        // Scenario 49, which is `Command::ReplaceMany` doing what it already does.
        let (folder, mut app) = a_web_window("unluminous-import-undo");
        ask_at(&mut app, "import { Layout } from './la|zy'");
        assert!(app.choose_the_completion("./layout"));
        app.accept_the_completion(true);
        app.document_mut().apply(Command::Undo);
        assert_eq!(text_of(&app), "import { Layout } from './lazy'", "one press puts it back");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_module_path_offers_the_packages_then_walks_into_one() {
        // Scenarios 20, 36 and 47, in the order a person types them.
        let (folder, mut app) = a_rust_window("unluminous-import-path");
        ask_at(&mut app, "use |");
        let rows = offered(&app);
        assert!(rows.contains(&"crate".to_owned()), "{rows:?}");
        assert!(rows.contains(&"super".to_owned()), "{rows:?}");
        assert!(
            rows.contains(&"unluminous_core".to_owned()),
            "a folder named with a hyphen: {rows:?}"
        );
        assert!(!rows.contains(&"unluminous-core".to_owned()), "{rows:?}");

        ask_at(&mut app, "use unluminous_core::|");
        assert_eq!(
            offered(&app),
            vec!["completion".to_owned()],
            "lib.rs is the package itself rather than a module in it"
        );

        ask_at(&mut app, "use unluminous_core::completion::|");
        let rows = offered(&app);
        assert!(rows.contains(&"Candidate".to_owned()), "{rows:?}");
        assert!(rows.contains(&"rank".to_owned()), "{rows:?}");
        assert!(!rows.contains(&"hidden".to_owned()), "only what `pub` marks: {rows:?}");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn crate_and_super_are_read_from_where_the_file_is() {
        // Scenarios 35 and 37 through the window, which is what proves the grammar's own words
        // reached the resolver.
        let (folder, mut app) = a_rust_window("unluminous-import-roots");
        ask_at(&mut app, "use crate::|");
        assert_eq!(offered(&app), vec!["app".to_owned()]);
        ask_at(&mut app, "use crate::app::|");
        assert_eq!(offered(&app), vec!["actions".to_owned()]);
        ask_at(&mut app, "use super::|");
        assert_eq!(
            offered(&app),
            vec!["app".to_owned()],
            "mod.rs is its folder, so super is above"
        );
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn typing_the_separator_opens_the_next_segments_list_in_the_same_frame() {
        // Scenario 50: without the one line in `keep_the_completion_fresh` the new list would need
        // one more keystroke before it came back.
        let (folder, mut app) = a_rust_window("unluminous-import-separator");
        typing(&mut app, "use unluminous_core:");
        typing(&mut app, ":");
        assert_eq!(offered(&app), vec!["completion".to_owned()], "the list is already there");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn an_ordinary_path_in_code_is_offered_the_ordinary_sources() {
        // Scenario 25 through the window: the anchor is what makes the whole reading trustworthy.
        let (folder, mut app) = a_rust_window("unluminous-import-not-an-import");
        ask_at(&mut app, "let x = unluminous_core::comp|");
        let modules = app
            .completion()
            .map(|state| state.rows.iter().filter(|row| row.source == Source::Module).count())
            .unwrap_or(0);
        assert_eq!(modules, 0, "{:?}", offered(&app));
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_stylesheet_offers_the_projects_stylesheets_with_their_extensions() {
        // CSS asks for the quoted family with no export keyword and no definers, so what it offers
        // is files and only files.
        let folder = std::env::temp_dir().join("unluminous-import-css");
        std::fs::remove_dir_all(&folder).ok();
        std::fs::create_dir_all(&folder).expect("make the folder");
        std::fs::write(folder.join("site.css"), "").expect("write site.css");
        std::fs::write(folder.join("theme.css"), ".card { color: red; }\n").expect("write theme");
        let mut app = UnluminousApp::new(&folder);
        build_the_index(&mut app);
        app.open_path_permanently(&folder.join("site.css")).expect("the file opens");
        typing(&mut app, "@import '");
        assert_eq!(offered(&app), vec!["./theme.css".to_owned()], "written out, and never itself");
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn a_frame_in_which_nothing_moved_recomputes_nothing() {
        // `task-1666`'s rule: a caret blink, a repaint and a frame of idling are two integer
        // comparisons. What is checked is the answer that rule produces — the rows are the same
        // objects, untouched — because the comparisons themselves cannot be seen from outside.
        let (folder, mut app) = a_window("unluminous-completion-idle");
        typing(&mut app, "dra");
        let before = app.completion().cloned().expect("open");
        for _ in 0..10 {
            a_quiet_frame(&mut app);
        }
        assert_eq!(app.completion(), Some(&before), "ten idle frames changed nothing");
        std::fs::remove_dir_all(&folder).ok();
    }
}
