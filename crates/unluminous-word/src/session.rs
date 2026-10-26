//! One open document: the document, the selection, undo and redo, and the commands a person and an agent
//! both use.
//!
//! Every change goes through [`Session::change`], which records what the change touched before it runs
//! and moves the selection afterwards, so the format bar, the keys, the panel and `plugins run word` are
//! the same code. Typing into one paragraph is one undo step per word: a run of typed characters joins
//! the step before it until a space follows a word, the caret moves, or a second passes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::edit::{self, Tracking, Unit};
use crate::model::*;
use crate::position::{self, Pos, Selection, StoryId};
use crate::props;
use crate::xml::El;

/// What an undo step puts back.
#[derive(Debug, Clone)]
enum Snapshot {
    /// The blocks of one story.
    Story(StoryId, Vec<Block>),
    /// The whole document, for a change outside the stories: styles, lists, sections, notes.
    Whole(Box<Document>),
}

#[derive(Debug, Clone)]
struct Entry {
    before: Snapshot,
    selection: Selection,
    after: Selection,
    /// Typing that a following keystroke may join.
    typing: Option<(Vec<u32>, Instant)>,
}

/// What a change touches, which decides how much is saved for undo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Story,
    Whole,
}

/// The counts Word's Word Count dialog shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub pages: usize,
    pub words: usize,
    pub characters: usize,
    pub characters_with_spaces: usize,
    pub paragraphs: usize,
    pub lines: usize,
}

/// One open document.
#[derive(Debug, Clone)]
pub struct Session {
    pub document: Document,
    pub selection: Selection,
    /// Formatting chosen with nothing selected, which the next typed text takes.
    pub pending: Option<El>,
    /// Track Changes is on.
    pub tracking: bool,
    pub author: String,
    /// Moves on every change, so a layout knows when it is stale.
    pub revision: u64,
    /// Changed since it was opened or saved.
    pub modified: bool,
    /// The file the document was opened from or saved to, for the `FILENAME` and `FILESIZE` fields.
    pub file: Option<std::path::PathBuf>,
    /// Which corrections are made as words are typed.
    pub autocorrect_rules: crate::autocorrect::Rules,
    /// Opened without its password to modify: nothing can be changed (task-2359, TDD 5.6).
    pub read_only: bool,
    /// Restrict Editing, when the document enforces it: `readOnly`, `comments`, `trackedChanges` or
    /// `forms`.
    pub protection: Option<String>,
    /// Why the last edit was refused, for the window to say.
    pub refused: Option<String>,
    /// An operation Restrict Editing allows is running (a comment under `comments`, the protection
    /// itself), so its change is let through.
    pub(crate) permit: bool,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

/// How many steps undo keeps.
const UNDO_LIMIT: usize = 500;
/// Typing within this long of the last keystroke joins its undo step.
const TYPING_JOINS: Duration = Duration::from_millis(1000);

impl Session {
    pub fn new(document: Document) -> Session {
        let first = position::paragraphs(&document.body)
            .first()
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| vec![0]);
        let tracking = document
            .settings
            .as_ref()
            .and_then(|s| props::on_off(&s.root, "w:trackRevisions"))
            .unwrap_or(false);
        let settings = document.settings.as_ref().map(|s| &s.root);
        let protection = settings
            .and_then(|s| s.child("w:documentProtection"))
            .filter(|p| props::on_off_attr(p.attr("w:enforcement")))
            .and_then(|p| p.attr("w:edit"))
            .filter(|e| *e != "none")
            .map(str::to_owned);
        let read_only = settings
            .and_then(|s| s.child("w:writeProtection"))
            .is_some_and(|w| w.attr("w:hash").is_some() || w.attr("w:hashValue").is_some());
        Session {
            document,
            selection: Selection::caret(Pos::new(first, 0)),
            pending: None,
            tracking,
            author: String::new(),
            revision: 1,
            modified: false,
            file: None,
            autocorrect_rules: crate::autocorrect::Rules::default(),
            read_only,
            protection,
            refused: None,
            permit: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The formatting of the first selected character, which text typed over a selection takes, as in
    /// Word (`task-2341`). `None` with no selection, or when the selection starts at a paragraph's end.
    fn first_selected_props(&self) -> Option<Arc<El>> {
        if self.selection.is_empty() {
            return None;
        }
        let (from, _) = self.selection.ordered();
        let paragraph = position::paragraph(&self.document, &from.story, &from.path)?;
        (from.offset < paragraph.len()).then(|| edit::props_at(paragraph, from.offset + 1).0)
    }

    fn tracking_info(&self) -> Option<Tracking> {
        self.tracking.then(|| Tracking {
            author: if self.author.is_empty() {
                "Unluminous".to_owned()
            } else {
                self.author.clone()
            },
            date: now_iso(),
        })
    }

    /// Whether the document may be changed now, under Restrict Editing and Password to Modify. A refusal
    /// is kept in `refused` for the window to say.
    pub fn may_edit(&mut self) -> bool {
        if self.permit {
            return true;
        }
        let refusal = if self.read_only {
            Some("this document was opened read only: give its password to modify it")
        } else {
            match self.protection.as_deref() {
                Some("readOnly") => Some("Restrict Editing allows no changes to this document"),
                Some("comments")
                    if !matches!(
                        self.selection.head.story,
                        crate::position::StoryId::Comment(_)
                    ) =>
                {
                    Some("Restrict Editing allows only comments in this document")
                }
                Some("forms") if !self.in_a_form_field() => {
                    Some("Restrict Editing allows only filling in forms in this document")
                }
                _ => None,
            }
        };
        self.refused = refusal.map(str::to_owned);
        refusal.is_none()
    }

    /// Whether the caret is in a content control or a form field, which `forms` protection lets a person
    /// type in.
    fn in_a_form_field(&self) -> bool {
        let head = &self.selection.head;
        let Some(paragraph) = position::paragraph(&self.document, &head.story, &head.path) else {
            return false;
        };
        let (_, wraps) = edit::props_at(paragraph, head.offset);
        wraps.iter().any(|w| w.el.name == "w:sdt")
            || matches!(
                position::block(&self.document.body, &head.path[..1]),
                Some(crate::model::Block::Sdt(_))
            )
    }

    /// Run a change, recording what it needs for undo.
    pub fn change(
        &mut self,
        scope: Scope,
        change: impl FnOnce(&mut Document, &Selection, Option<&Tracking>) -> Selection,
    ) {
        if !self.may_edit() {
            return;
        }
        let before = match scope {
            Scope::Story => {
                let story = self.selection.head.story.clone();
                let blocks = position::story(&self.document, &story).cloned().unwrap_or_default();
                Snapshot::Story(story, blocks)
            }
            Scope::Whole => Snapshot::Whole(Box::new(self.document.clone())),
        };
        let tracking = self.tracking_info();
        let selection = self.selection.clone();
        let after = change(&mut self.document, &selection, tracking.as_ref());
        self.push(Entry { before, selection, after: after.clone(), typing: None });
        self.selection = after;
        self.touched();
    }

    fn push(&mut self, entry: Entry) {
        self.undo.push(entry);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn touched(&mut self) {
        self.revision += 1;
        self.modified = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Put the document back as it was before the last change.
    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo.pop() else { return false };
        let current = self.snapshot_like(&entry.before);
        self.restore(entry.before.clone());
        self.selection = entry.selection.clone();
        self.redo.push(Entry { before: current, ..entry });
        self.touched();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.redo.pop() else { return false };
        let current = self.snapshot_like(&entry.before);
        self.restore(entry.before.clone());
        self.selection = entry.after.clone();
        self.undo.push(Entry { before: current, ..entry });
        self.touched();
        true
    }

    fn snapshot_like(&self, like: &Snapshot) -> Snapshot {
        match like {
            Snapshot::Story(story, _) => Snapshot::Story(
                story.clone(),
                position::story(&self.document, story).cloned().unwrap_or_default(),
            ),
            Snapshot::Whole(_) => Snapshot::Whole(Box::new(self.document.clone())),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        match snapshot {
            Snapshot::Story(story, blocks) => {
                if let Some(target) = position::story_mut(&mut self.document, &story) {
                    *target = blocks;
                }
            }
            Snapshot::Whole(document) => self.document = *document,
        }
    }

    // ---- Typing ----

    /// Type text at the selection, replacing it, as a keyboard or a paste does.
    pub fn type_text(&mut self, text: &str) {
        if !self.may_edit() {
            return;
        }
        if text.is_empty() {
            return;
        }
        let joins =
            self.undo.last().and_then(|entry| entry.typing.clone()).is_some_and(|(path, at)| {
                self.selection.is_empty()
                    && path == self.selection.head.path
                    && at.elapsed() < TYPING_JOINS
                    && self.redo.is_empty()
                    && self.undo.last().is_some_and(|e| e.after == self.selection)
            }) && !text.contains('\n')
                && !text.starts_with(' ');
        let pending = self.pending.take().map(|rpr| {
            let (base, _) = position::paragraph(
                &self.document,
                &self.selection.head.story,
                &self.selection.head.path,
            )
            .map(|p| edit::props_at(p, self.selection.head.offset))
            .unwrap_or_else(|| (no_props(), Vec::new()));
            let mut merged = base.as_ref().clone();
            for child in rpr.elements() {
                merged.put(child.clone());
            }
            for child in rpr.elements() {
                if child.attr("w:val") == Some("__clear") {
                    merged.remove_children(&child.name);
                }
            }
            Arc::new(merged)
        });
        let pending = pending.or_else(|| self.first_selected_props());
        let tracking = self.tracking_info();
        if joins {
            let selection = self.selection.clone();
            let after = edit::insert_text(
                &mut self.document,
                &selection.head,
                text,
                pending,
                tracking.as_ref(),
            );
            self.selection = Selection::caret(after.clone());
            if let Some(last) = self.undo.last_mut() {
                last.after = self.selection.clone();
                last.typing = Some((after.path.clone(), Instant::now()));
            }
            self.touched();
            return;
        }
        self.change(Scope::Story, |document, selection, tracking| {
            let at = if selection.is_empty() {
                selection.head.clone()
            } else {
                edit::delete_range(document, &selection.anchor, &selection.head, tracking)
            };
            Selection::caret(edit::insert_text(document, &at, text, pending, tracking))
        });
        if let Some(last) = self.undo.last_mut() {
            last.typing =
                (!text.contains('\n')).then(|| (self.selection.head.path.clone(), Instant::now()));
        }
    }

    /// Enter: a new paragraph.
    pub fn enter(&mut self) {
        if !self.may_edit() {
            return;
        }
        self.change(Scope::Story, |document, selection, tracking| {
            let at = if selection.is_empty() {
                selection.head.clone()
            } else {
                edit::delete_range(document, &selection.anchor, &selection.head, tracking)
            };
            // Enter in an empty list item ends the list, as Word does.
            if let Some(paragraph) = position::paragraph_mut(document, &at.story, &at.path) {
                if paragraph.is_empty() && paragraph.props.child("w:numPr").is_some() {
                    paragraph.props.remove_children("w:numPr");
                    if paragraph.props.val_of("w:pStyle") == Some("ListParagraph") {
                        paragraph.props.remove_children("w:pStyle");
                    }
                    return Selection::caret(at);
                }
            }
            Selection::caret(edit::split_paragraph(document, &at, tracking))
        });
    }

    /// Backspace.
    pub fn backspace(&mut self, unit: Unit) {
        if !self.may_edit() {
            return;
        }
        self.change(Scope::Story, |document, selection, tracking| {
            if !selection.is_empty() {
                return Selection::caret(edit::delete_range(
                    document,
                    &selection.anchor,
                    &selection.head,
                    tracking,
                ));
            }
            Selection::caret(edit::delete_backward(document, &selection.head, unit, tracking))
        });
    }

    /// Delete.
    pub fn delete(&mut self, unit: Unit) {
        if !self.may_edit() {
            return;
        }
        self.change(Scope::Story, |document, selection, tracking| {
            if !selection.is_empty() {
                return Selection::caret(edit::delete_range(
                    document,
                    &selection.anchor,
                    &selection.head,
                    tracking,
                ));
            }
            Selection::caret(edit::delete_forward(document, &selection.head, unit, tracking))
        });
    }

    /// A line break (Shift+Enter), a page break (Ctrl+Enter) or a column break (Ctrl+Shift+Enter).
    pub fn insert_break(&mut self, character: char) {
        self.type_text(&character.to_string());
    }

    // ---- Formatting ----

    /// The run properties at the selection's start, resolved, which the format bar shows.
    pub fn look_at_caret(&self) -> props::RunLook {
        let resolver = crate::styles::Resolver::new(&self.document);
        let head = self.selection.ordered().0;
        let Some(paragraph) = position::paragraph(&self.document, &head.story, &head.path) else {
            return props::RunLook::default();
        };
        let offset = if self.selection.is_empty() { head.offset } else { head.offset + 1 };
        let (rpr, _) = edit::props_at(paragraph, offset.min(paragraph.len()));
        let mut rpr = rpr.as_ref().clone();
        if let Some(pending) = &self.pending {
            for child in pending.elements() {
                rpr.put(child.clone());
            }
        }
        resolver.run(&paragraph.props, &rpr, None)
    }

    /// The paragraph properties at the caret, resolved.
    pub fn paragraph_at_caret(&self) -> props::ParaLook {
        let resolver = crate::styles::Resolver::new(&self.document);
        let head = &self.selection.head;
        position::paragraph(&self.document, &head.story, &head.path)
            .map(|p| resolver.paragraph(&p.props, None))
            .unwrap_or_default()
    }

    /// Change the run properties of the selection, or of what is typed next when nothing is selected.
    pub fn format(&mut self, change: impl Fn(&mut El) + 'static) {
        if self.selection.is_empty() {
            let mut pending = self.pending.take().unwrap_or_else(|| El::new("w:rPr"));
            // What the caret would type with, changed, recorded as the difference.
            let base = {
                let head = &self.selection.head;
                position::paragraph(&self.document, &head.story, &head.path)
                    .map(|p| edit::props_at(p, head.offset).0.as_ref().clone())
                    .unwrap_or_else(|| El::new("w:rPr"))
            };
            let mut changed = base.clone();
            for child in pending.elements() {
                changed.put(child.clone());
            }
            change(&mut changed);
            pending = El::new("w:rPr");
            for child in changed.elements() {
                if base.child(&child.name) != Some(child) {
                    pending.put(child.clone());
                }
            }
            for child in base.elements() {
                if changed.child(&child.name).is_none() {
                    pending.put(El::new(child.name.clone()).with("w:val", "__clear"));
                }
            }
            self.pending = Some(pending);
            return;
        }
        let change = Arc::new(change);
        self.change(Scope::Story, move |document, selection, tracking| {
            // With Track Changes on, what the text looked like before goes into a w:rPrChange.
            if let Some(t) = tracking {
                let (from, to) = selection.ordered();
                Session::track_run_formatting(document, from, to, &t.author);
            }
            edit::format_range(document, &selection.anchor, &selection.head, &|el| change(el));
            selection.clone()
        });
    }

    /// Turn a toggle (`w:b`, `w:i`, `w:strike`...) on or off for the selection: off when the whole
    /// selection already has it, on otherwise, which is what Ctrl+B does.
    pub fn toggle(&mut self, name: &'static str) {
        let on = {
            let look = self.look_at_caret();
            let mut current = look.clone();
            !*props::toggle_slot(&mut current, name)
        };
        let on = if name == "w:u" {
            self.look_at_caret().underline == props::Underline::None
        } else {
            on
        };
        self.format(move |rpr| match name {
            "w:u" => props::set_val(rpr, "w:u", on.then_some("single")),
            _ => props::set_on_off(rpr, name, Some(on)),
        });
    }

    /// Change the paragraph properties of every paragraph the selection touches.
    pub fn format_paragraphs(&mut self, change: impl Fn(&mut El) + 'static) {
        self.change(Scope::Story, move |document, selection, tracking| {
            if let Some(t) = tracking {
                let (from, to) = selection.ordered();
                Session::track_paragraph_formatting(document, from, to, &t.author);
            }
            edit::format_paragraphs(document, &selection.anchor, &selection.head, &change);
            selection.clone()
        });
    }

    /// Apply a style by id: a paragraph style to the paragraphs, a character style to the selection.
    pub fn apply_style(&mut self, id: &str) -> Result<(), String> {
        crate::builtin::ensure_style(&mut self.document, id)?;
        let kind = crate::styles::Resolver::new(&self.document)
            .style(id)
            .and_then(|s| s.attr("w:type").map(str::to_owned));
        let id = id.to_owned();
        match kind.as_deref() {
            Some("character") => self.format(move |rpr| props::set_val(rpr, "w:rStyle", Some(&id))),
            Some("paragraph") | None => self.change(Scope::Whole, move |document, selection, _| {
                edit::format_paragraphs(document, &selection.anchor, &selection.head, &|ppr| {
                    if id == "Normal" {
                        ppr.remove_children("w:pStyle");
                    } else {
                        props::set_val(ppr, "w:pStyle", Some(&id));
                    }
                });
                selection.clone()
            }),
            Some(other) => {
                return Err(format!("{id} is a {other} style, which is not applied to text"))
            }
        }
        Ok(())
    }

    // ---- Selection ----

    pub fn select_all(&mut self) {
        let story = self.selection.head.story.clone();
        let Some(blocks) = position::story(&self.document, &story) else { return };
        let all = position::paragraphs(blocks);
        let (Some((first, _)), Some((last, paragraph))) = (all.first(), all.last()) else { return };
        self.selection = Selection {
            anchor: Pos { story: story.clone(), path: first.clone(), offset: 0 },
            head: Pos { story, path: last.clone(), offset: paragraph.len() },
        };
    }

    /// Put the caret or a selection somewhere, clamped to the text that is there.
    pub fn select(&mut self, anchor: Pos, head: Pos) {
        let clamp = |document: &Document, pos: Pos| {
            let len =
                position::paragraph(document, &pos.story, &pos.path).map_or(0, Paragraph::len);
            Pos { offset: pos.offset.min(len), ..pos }
        };
        self.selection =
            Selection { anchor: clamp(&self.document, anchor), head: clamp(&self.document, head) };
        self.pending = None;
    }

    /// The selected text, with paragraphs separated by line ends.
    pub fn selected_text(&self) -> String {
        let (from, to) = self.selection.ordered();
        let mut out = Vec::new();
        for (path, len, _) in edit::covered(&self.document, from, to) {
            let Some(paragraph) = position::paragraph(&self.document, &from.story, &path) else {
                continue;
            };
            let lo = if path == from.path { from.offset } else { 0 };
            let hi = if path == to.path { to.offset } else { len };
            let text = paragraph.flat_text();
            out.push(clean_text(&text[lo.min(text.len())..hi.min(text.len())]));
        }
        out.join("\n")
    }

    /// The selection as text and as paragraphs with their formatting, for the clipboard.
    pub fn copy(&self) -> (String, Vec<Paragraph>) {
        (
            self.selected_text(),
            edit::copy_fragment(&self.document, &self.selection.anchor, &self.selection.head),
        )
    }

    /// Paste paragraphs this or another document copied, keeping their formatting, over the selection.
    pub fn paste_fragment(&mut self, fragment: Vec<Paragraph>) {
        // A style a pasted paragraph names that this document lacks comes from Word's built in ones.
        let mut wanted: Vec<String> = Vec::new();
        for paragraph in &fragment {
            if let Some(id) = paragraph.props.val_of("w:pStyle") {
                wanted.push(id.to_owned());
            }
            for run in &paragraph.runs {
                if let Some(id) = run.props.val_of("w:rStyle") {
                    wanted.push(id.to_owned());
                }
            }
        }
        for id in wanted {
            let _ = crate::builtin::ensure_style(&mut self.document, &id);
        }
        self.change(Scope::Whole, move |document, selection, tracking| {
            let at = if selection.is_empty() {
                selection.head.clone()
            } else {
                edit::delete_range(document, &selection.anchor, &selection.head, tracking)
            };
            Selection::caret(edit::paste_fragment(document, &at, &fragment, tracking))
        });
    }

    // ---- Review ----

    /// Accept or reject every tracked change, or those the selection touches.
    pub fn resolve_revisions(&mut self, accept: bool, all: bool) -> usize {
        let mut count = 0;
        let story = self.selection.head.story.clone();
        let (from, to) = (self.selection.ordered().0.clone(), self.selection.ordered().1.clone());
        self.change(Scope::Whole, |document, selection, _| {
            if all {
                count = edit::resolve_revisions(&mut document.body, accept, None);
                for header in &mut document.headers {
                    let n = edit::resolve_revisions(&mut header.story.blocks, accept, None);
                    header.dirty |= n > 0;
                    count += n;
                }
            } else {
                for (path, _, _) in edit::covered(document, &from, &to) {
                    if let Some(Block::Paragraph(paragraph)) = position::story_mut(document, &story)
                        .and_then(|b| position::block_mut(b, &path))
                    {
                        let mut one = vec![Block::Paragraph(std::mem::take(paragraph))];
                        count += edit::resolve_revisions(&mut one, accept, None);
                        if let Some(Block::Paragraph(done)) = one.pop() {
                            *paragraph = done;
                        }
                    }
                }
            }
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        count
    }

    /// Turn Track Changes on or off, and say so in the file's settings as Word does.
    pub fn set_tracking(&mut self, on: bool) {
        if !on && self.protection.as_deref() == Some("trackedChanges") {
            self.refused = Some("Restrict Editing keeps Track Changes on in this document".into());
            return;
        }
        self.tracking = on;
        if let Some(settings) = self.document.settings.as_mut() {
            props::set_on_off(&mut settings.root, "w:trackRevisions", on.then_some(true));
            settings.dirty = true;
            self.touched();
        }
    }

    // ---- Find ----

    /// Every match of `needle` in the body's visible text, as selections.
    pub fn find(&self, needle: &str, match_case: bool, whole_word: bool) -> Vec<Selection> {
        let search = crate::search::Search {
            query: crate::find::Query {
                text: needle.to_owned(),
                match_case,
                whole_word,
                wildcards: false,
            },
            ..crate::search::Search::default()
        };
        self.search(&search).unwrap_or_default()
    }

    /// Replace every match, as one undo step. Answers how many were replaced.
    pub fn replace_all(
        &mut self,
        needle: &str,
        with: &str,
        match_case: bool,
        whole_word: bool,
    ) -> usize {
        let matches = self.find(needle, match_case, whole_word);
        if matches.is_empty() {
            return 0;
        }
        let count = matches.len();
        let with = with.to_owned();
        self.change(Scope::Story, move |document, selection, tracking| {
            // Backwards, so earlier offsets stay right.
            for found in matches.iter().rev() {
                let at = edit::delete_range(document, &found.anchor, &found.head, tracking);
                edit::insert_text(document, &at, &with, None, tracking);
            }
            let len = position::paragraph(document, &selection.head.story, &selection.head.path)
                .map_or(0, Paragraph::len);
            Selection::caret(selection.head.at(selection.head.offset.min(len)))
        });
        count
    }

    // ---- Counting ----

    /// Words, characters and paragraphs as Word counts them, and pages and lines from a layout.
    pub fn stats(&self, pages: Option<&crate::layout::Pages>) -> Stats {
        let mut stats = Stats::default();
        for (_, paragraph) in position::paragraphs(&self.document.body) {
            let text = paragraph.visible_text();
            let clean = clean_text(&text);
            if !clean.trim().is_empty() {
                stats.paragraphs += 1;
            }
            stats.words += clean
                .split(|c: char| c.is_whitespace())
                .filter(|w| w.chars().any(|c| c.is_alphanumeric() || !c.is_ascii_punctuation()))
                .count();
            stats.characters += clean.chars().filter(|c| !c.is_whitespace()).count();
            stats.characters_with_spaces += clean.chars().filter(|c| *c != '\u{b}').count();
        }
        if let Some(pages) = pages {
            stats.pages = pages.pages.len();
            stats.lines = pages
                .pages
                .iter()
                .flat_map(|p| p.carets.iter())
                .filter(|l| l.story == StoryId::Body && l.stops.len() > 1)
                .count();
        }
        stats
    }
}

/// Flat text with the characters that stand for objects and field marks taken out, and breaks as line ends.
pub fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c, OBJECT | FIELD_BEGIN | FIELD_SEPARATE | FIELD_END))
        .map(|c| if matches!(c, LINE_BREAK | PAGE_BREAK | COLUMN_BREAK) { '\n' } else { c })
        .collect()
}

/// The time now as Word writes a revision's date.
pub fn now_iso() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::new(crate::new::blank())
    }

    fn texts(session: &Session) -> Vec<String> {
        position::paragraphs(&session.document.body)
            .into_iter()
            .map(|(_, p)| p.flat_text())
            .collect()
    }

    #[test]
    fn typing_is_one_undo_step_per_word() {
        let mut s = session();
        for c in "hello world".chars() {
            s.type_text(&c.to_string());
        }
        assert_eq!(texts(&s), ["hello world"]);
        s.undo();
        assert_eq!(texts(&s), ["hello"]);
        s.undo();
        assert_eq!(texts(&s), [""]);
        s.redo();
        assert_eq!(texts(&s), ["hello"]);
    }

    #[test]
    fn bold_with_nothing_selected_applies_to_what_is_typed_next() {
        let mut s = session();
        s.type_text("plain ");
        s.toggle("w:b");
        s.type_text("bold");
        let Block::Paragraph(p) = &s.document.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 2);
        assert!(p.runs[1].props.child("w:b").is_some());
        assert!(s.look_at_caret().bold);
    }

    #[test]
    fn find_and_replace_all_is_one_step() {
        let mut s = session();
        s.type_text("the cat and the hat");
        assert_eq!(s.find("THE", false, true).len(), 2);
        assert_eq!(s.replace_all("the", "a", false, true), 2);
        assert_eq!(texts(&s), ["a cat and a hat"]);
        s.undo();
        assert_eq!(texts(&s), ["the cat and the hat"]);
    }

    #[test]
    fn counting_follows_words_rules() {
        let mut s = session();
        s.type_text("One two, three.\nFour");
        let stats = s.stats(None);
        assert_eq!((stats.words, stats.paragraphs), (4, 2));
        assert_eq!(stats.characters, "Onetwo,three.Four".len());
    }

    #[test]
    fn the_date_is_written_the_way_word_writes_it() {
        let date = now_iso();
        assert_eq!(date.len(), 20);
        assert!(date.ends_with('Z') && date.as_bytes()[10] == b'T');
    }
}
