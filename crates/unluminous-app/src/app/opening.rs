//! Opening, saving, closing, moving and throwing away a file.
//!
//! A file that moves takes the code that names it with it, which is `services::file_move` and the one
//! rule it keeps: work out what the written text will mean **after** the move, and if that is not
//! what it means now, rewrite it. An open file is edited as a document and left unsaved; a closed one
//! is read, checked and written once.

use std::path::{Path, PathBuf};

use unluminous_core::{Command, Document};

use crate::services::file_kind;
use crate::services::file_move;
use crate::services::file_tree::FileTree;
use crate::services::imports;
use crate::services::recycle;

use crate::app::files;
use crate::app::{move_the_bytes, write_the_edits, Focus, UnluminousApp, ViewMode};

impl UnluminousApp {
    /// Show `folder` in the explorer, and remember it as a recent project.
    ///
    /// What was open in the project being left is written down first, because after this the window no
    /// longer knows which project its tabs belonged to. What the *new* project had open is deliberately
    /// not restored here: this is also the path `Open File` takes when the file chosen is outside the
    /// folder that is open, and quietly closing somebody's tabs and opening a different set because they
    /// opened one file elsewhere would be a surprise. A project's state is restored when a window opens
    /// on it, which is what `File -> Open Folder` now does.
    pub fn open_folder(&mut self, folder: &Path) {
        self.remember_the_project();
        self.tree = FileTree::new(folder);
        // A fresh tree knows nothing of the settings, and the folder it has just read may be a
        // different repository with a different `.gitignore`.
        self.tree.set_exclude(&self.settings.exclude);
        self.filter.clear();
        self.explorer_visible = true;
        self.terminal.tabs.settings.working_directory = Some(folder.to_path_buf());
        if let Some(store) = &self.store {
            store.remember_project(folder);
            self.recent = store.recent_projects();
        }
        // The second folder may be a different repository, or none at all.
        self.open_repository();
    }

    /// Open a file into the tab that a single click reuses.
    ///
    /// Any file holding text opens. A `.md` file is Markdown, which means the preview button does
    /// something with it; everything else opens as plain text, which is what
    /// `tasks/improvements.md` asks for.
    pub fn open_path(&mut self, path: &Path) -> Result<(), String> {
        self.open_path_in_tab(path, false)
    }

    /// Open a file in a tab of its own, which is what a double click in the explorer does.
    pub fn open_path_permanently(&mut self, path: &Path) -> Result<(), String> {
        self.open_path_in_tab(path, true)
    }

    /// The one place a file is loaded, whether it is text or a picture. `permanent` decides whether it
    /// takes a tab of its own or reuses the transient one; [`files::OpenFiles::open`] decides what that
    /// means.
    ///
    /// **It answers whether the file opened**, and `task-1804` §7.2 is why. It used to set
    /// `self.message` and return, which is a good answer for a person -- the reason is in the status
    /// bar in front of them -- and no answer at all for the command line, which built its reply
    /// without asking whether the tab was there: `tab open` on a file that could not be decoded
    /// replied `ok: true`, `tab: 0`, exit 0, and every step after it operated on whatever had been
    /// open before. For an agent that is the worst shape a fault can have, because the product's
    /// argument is that these answers can be trusted.
    ///
    /// The message is still set, because the person at the window still needs it. What is added is
    /// that the reason is *returned* as well, so a caller can tell.
    pub(crate) fn open_path_in_tab(&mut self, path: &Path, permanent: bool) -> Result<(), String> {
        if let Err(refusal) = file_kind::openable(path) {
            let reason = format!("{}: {}", path.display(), refusal.reason());
            self.message = Some(reason.clone());
            return Err(reason);
        }
        // A file that is already open is shown rather than read from disk again, so switching back
        // to a tab does not throw away what has been typed into it.
        if let Some(index) = self.files.index_of(path) {
            self.show_tab(index);
            if permanent {
                self.files.make_permanent(index);
            }
            return Ok(());
        }
        // A picture is a tab of its own kind. It is read here rather than in `files`, so that the one
        // place a file is opened stays the one place a file is opened.
        if file_kind::is_image(path) {
            self.files.open_file(files::OpenFile::picture(path), permanent);
            self.message = None;
            self.forget_layout();
            return Ok(());
        }
        match Document::open(path) {
            Ok(mut document) => {
                document.apply(Command::MoveDocumentStart { extend: false });
                self.files.open(document, permanent);
                let change = self.settings.as_style_change();
                self.document_mut().set_base_style(change);
                // Whatever was marked in this file last time. The document clamps the ranges to the
                // text it has just read, so a file that changed on the disk since is a mark in the
                // wrong place rather than a range past the end of the rope.
                if let Some(marks) = self.marks.highlights(path).cloned() {
                    self.document_mut().set_highlights(marks);
                }
                // And wherever it was to stop. The document clamps these too, so a file rewritten
                // outside Unluminous gives a dot on the wrong line — which the adapter's `verified`
                // answer then says so about — rather than a panic in the layout engine.
                if let Some(breakpoints) = self.breakpoints.breakpoints(path).cloned() {
                    self.document_mut().set_breakpoints(breakpoints);
                }
                let revision = self.document().revision();
                self.files.active_mut().marked_revision = Some(revision);
                self.files.active_mut().breakpoints_at = Some(revision);
                // What the file looked like at the moment it was read, so a later read can tell that
                // something else has changed it since.
                self.files.active_mut().note_what_is_on_disk();
                self.message = None;
                // A file that is not Markdown has nothing to preview, so the raw source is shown.
                if !file_kind::is_markdown(Some(path)) {
                    self.files.active_mut().view_mode = ViewMode::Raw;
                }
                // The new document counts its revisions from the beginning, so what was laid out for
                // the last one has to be thrown away rather than compared against.
                self.forget_layout();
                Ok(())
            }
            Err(error) => {
                // Nothing is thrown away: the document that was open stays open, and the reason is said in
                // the status bar rather than only on the error output.
                let reason = format!("Unluminous could not open {}: {error}", path.display());
                self.message = Some(reason.clone());
                eprintln!("{reason}");
                Err(reason)
            }
        }
    }

    /// Open a file at a match found by `Find in Files`, with the match itself selected.
    ///
    /// Selecting it rather than only putting the caret there is what the ticket asks for when it
    /// says the result should highlight the matching spot in the document: a selection is how a
    /// document shows a piece of itself, and it is the same highlight a search inside a file leaves.
    pub(crate) fn open_the_match(&mut self, path: &Path, range: std::ops::Range<usize>) {
        // A tab of its own, because choosing a line out of a list of matches is not glancing.
        if self.open_path_permanently(path).is_err() {
            return; // it would not open, and `open_path_permanently` has already said why
        }
        // The offsets came from the file on disk. A tab that was already open and has been edited
        // since is a different text, so a range that runs past its end is left alone rather than
        // selecting the wrong thing.
        let length = self.document().text().len_bytes();
        if range.end > length {
            self.message = Some(format!(
                "{} has changed since it was searched, so the match could not be shown.",
                path.display()
            ));
            return;
        }
        self.document_mut().apply(Command::PlaceCaret { offset: range.start, extend: false });
        self.document_mut().apply(Command::PlaceCaret { offset: range.end, extend: true });
        // The file is nearly always taller than the editing area, so the match has to be scrolled to
        // as well as selected.
        self.reveal_caret = true;
        self.focus = Focus::Editor;
    }

    /// Throw a file or a folder away, and tidy up after it.
    ///
    /// Every tab on the file — or on anything under the folder — is closed **without** the save
    /// `close_tab` does, because writing a file in order to throw it away is not a thing to do. The
    /// project's marks for those paths go with them, and the index is told the project changed.
    pub fn delete_path(&mut self, path: &Path) {
        match recycle::delete(path) {
            Ok(()) => {
                let gone: Vec<PathBuf> = self
                    .files
                    .paths()
                    .into_iter()
                    .filter(|open| open == path || open.starts_with(path))
                    .collect();
                for open in &gone {
                    if let Some(index) = self.files.index_of(open) {
                        self.close_tab_without_saving(index);
                    }
                    self.marks.forget(open);
                }
                if self.selected.as_deref() == Some(path) {
                    self.selected = path.parent().map(Path::to_path_buf);
                }
                self.tree.reload();
                self.the_project_changed_on_disk();
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                self.message = Some(format!("Deleted {name} to {}", recycle::destination().name()));
            }
            Err(problem) => {
                self.message =
                    Some(format!("Unluminous could not delete {}: {problem}", path.display()))
            }
        }
    }

    /// Move a file or a folder, and take the code that names it with it.
    ///
    /// `to` is where the thing itself lands, not the folder it was dropped into, because a name
    /// already taken in the destination has to be settled before anything is planned.
    ///
    /// The order matters. The plan is worked out **first**, against the project as it is, because
    /// every specifier in it is resolved against files that are still where they were. Then the
    /// bytes move. Then the edits are applied, following `task-1675`'s ownership rule: an open file
    /// is edited as a document and left modified, and a closed file is read, checked and written
    /// once.
    pub fn move_path(&mut self, from: &Path, to: &Path, refactor: bool) -> bool {
        if from == to {
            return false;
        }
        if to.exists() {
            self.message = Some(format!("{} is already there", to.display()));
            return false;
        }
        let plan = match refactor {
            true => self.plan_a_move(from, to),
            false => file_move::Plan::default(),
        };
        if let Some(folder) = to.parent() {
            if let Err(problem) = std::fs::create_dir_all(folder) {
                self.message =
                    Some(format!("Unluminous could not make {}: {problem}", folder.display()));
                return false;
            }
        }
        if let Err(problem) = move_the_bytes(from, to) {
            self.message = Some(format!("Unluminous could not move {}: {problem}", from.display()));
            return false;
        }
        // The tabs follow the file before anything is written, so a tab on a moved file is edited
        // at its new path rather than at one with nothing behind it.
        self.retarget_the_tabs(&plan.moved);
        self.marks.moved(&plan.moved);
        let report = self.apply_a_move(&plan);
        self.tree.reload();
        if let Some(folder) = to.parent() {
            self.tree.expand(folder);
        }
        self.the_project_changed_on_disk();
        self.selected = Some(to.to_path_buf());
        let name = from
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| from.display().to_string());
        let where_to = to
            .parent()
            .and_then(|folder| folder.strip_prefix(self.tree.root()).ok())
            .map(|folder| folder.display().to_string())
            .filter(|folder| !folder.is_empty())
            .unwrap_or_else(|| "the project".to_owned());
        let mut said = format!("Moved {name} to {where_to} \u{00B7} {}", plan.sentence());
        for note in plan.notes.iter().chain(report.iter()) {
            said.push_str(&format!(" \u{00B7} {note}"));
        }
        self.message = Some(said);
        true
    }

    /// Work out what a move would change, without changing anything.
    ///
    /// The reader it hands the planner is where the ownership rule enters: a file that is open
    /// answers with the text in its tab, and every other file answers with what is on the disk.
    pub fn plan_a_move(&self, from: &Path, to: &Path) -> file_move::Plan {
        let files = self.tree.all_files().to_vec();
        let project = imports::Project { root: self.tree.root(), files: &files };
        let open: Vec<(PathBuf, String)> = self
            .files
            .iter()
            .filter_map(|file| {
                file.path().map(|path| (path.to_path_buf(), file.document.text().to_string()))
            })
            .collect();
        let read = |path: &Path| -> Option<String> {
            if let Some((_, text)) = open.iter().find(|(known, _)| known == path) {
                return Some(text.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        file_move::plan(&project, self.plugins.grammars(), from, to, &read)
    }

    /// Point every tab that was on a moved file at where the file went.
    fn retarget_the_tabs(&mut self, moved: &[(PathBuf, PathBuf)]) {
        for (old, new) in moved {
            let Some(index) = self.files.index_of(old) else {
                continue;
            };
            self.files.at_mut(index).document.set_path(new.clone());
            self.files.at_mut(index).forget_what_was_worked_out();
        }
    }

    /// Apply a plan's edits, and say what could not be applied.
    ///
    /// An open file is one `Command::ReplaceMany`, which is one undo step, and is left **modified
    /// rather than written**: a refactor must never silently write a buffer somebody was editing.
    /// A closed file is read, every range is checked to still hold what the plan expected, and only
    /// then is it written once — and a file that changed underneath the plan is skipped whole and
    /// named rather than patched on faith.
    fn apply_a_move(&mut self, plan: &file_move::Plan) -> Vec<String> {
        let mut skipped = Vec::new();
        for file in &plan.files {
            if file.edits.is_empty() {
                continue;
            }
            match self.files.index_of(&file.path) {
                Some(index) => {
                    let edits = file.edits.clone();
                    self.files.at_mut(index).document.apply(Command::ReplaceMany(edits));
                }
                None => {
                    if let Err(reason) = write_the_edits(&file.path, &file.edits) {
                        let name = file
                            .path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default();
                        skipped.push(format!("{name} was left alone: {reason}"));
                    }
                }
            }
        }
        skipped
    }

    /// Read a path again from disk: the folder it is in, and the file itself if it is open.
    ///
    /// Unsaved changes are kept rather than thrown away. A person asking to reload has asked for
    /// what is on disk, but nothing in the entry says "and lose what I typed", and quietly losing an
    /// edit is not a thing an editor should do without asking. So a file with unsaved changes says
    /// so and is left alone.
    /// Read the tab that is showing again when its file has changed underneath it.
    ///
    /// Unluminous watches nothing and a tab is owned by its `Document`, which is the right rule while
    /// Unluminous is the only thing writing. It is the wrong answer the moment something else does: the tab
    /// went on showing text that was no longer in the file, and `editor text` answered with it — so a
    /// caller that wrote a file and read it back through the window was handed what it had replaced.
    ///
    /// Checked at the moment of use rather than watched, which is the rule
    /// `services::symbol_index` already follows for a closed file. The cost is one `metadata` call on
    /// the command that reads, and nothing at all on a frame.
    ///
    /// A tab with unsaved changes is never touched. Those belong to the person, and throwing them
    /// away has no undo — `tab reload --discard` is how somebody says they mean it.
    pub fn reread_if_the_file_changed(&mut self) {
        if !self.files.active().the_file_changed_underneath() {
            return;
        }
        let Some(path) = self.document().path().map(Path::to_path_buf) else { return };
        self.reload_from_disk(&path, false);
    }

    pub fn reload_from_disk(&mut self, path: &Path, discard: bool) -> bool {
        self.tree.reload();
        let Some(index) = self.files.index_of(path) else {
            self.message = Some(format!("Reloaded {}", path.display()));
            return true;
        };
        // A tab with unsaved changes is not reloaded, because reading the file again would throw
        // them away and there is no undo for that. The explorer's own `Reload from Disk` never
        // discards; the command line can ask to, because a script that means it has no way to say
        // so through a menu.
        if !discard && self.files.get(index).is_some_and(|file| file.document.is_modified()) {
            self.message =
                Some(format!("{} has unsaved changes, so it was not reloaded", path.display()));
            return false;
        }
        match Document::open(path) {
            Ok(mut document) => {
                document.apply(Command::MoveDocumentStart { extend: false });
                let change = self.settings.as_style_change();
                document.set_base_style(change);
                // The tab that holds the file, which is not necessarily the one showing: reloading
                // a file from the explorer must not drag a different tab into view.
                if let Some(file) = self.files.get_mut(index) {
                    file.document = document;
                    file.scroll = 0.0;
                    file.forget_git();
                    file.forget_where_it_was_being_read();
                    file.note_what_is_on_disk();
                }
                if index == self.files.active_index() {
                    self.forget_layout();
                }
                self.message = Some(format!("Reloaded {}", path.display()));
                true
            }
            Err(problem) => {
                self.message =
                    Some(format!("Unluminous could not reload {}: {problem}", path.display()));
                false
            }
        }
    }

    /// Show the tab at `index`.
    ///
    /// The laid out text is a cache of the tab that is showing, and each document counts its own
    /// revisions from one, so two tabs can be at the same revision. Comparing revisions alone would
    /// therefore keep the layout of the file that was showing before. That is the same fault
    /// [`Self::forget_layout`] exists for, wearing a different hat.
    pub fn show_tab(&mut self, index: usize) {
        if index == self.files.active_index() && index < self.files.len() {
            return;
        }
        self.files.show(index);
        self.forget_layout();
    }

    /// Close the tab at `index`, and show whatever is left.
    /// Close the tab at `index`, and show whatever is left.
    ///
    /// **A tab with unsaved changes is written first**, which is what `task-1681` asks for: *"If I
    /// close a tab that has been edited but not saved, it should save and close."* Every other
    /// editor puts a three-answer dialog here; Unluminous can give the simpler answer because it saves
    /// plain text and nothing else, so writing the buffer to the file it came from is exactly what
    /// was typed. There is no format conversion to get wrong and no decision for a dialog to ask
    /// about.
    ///
    /// This is the one place a tab is closed — the cross on the tab, `Ctrl+W`, the tab's own menu
    /// and `unluminous-cli tab close` all reach it — so it is one change in one function.
    pub fn close_tab(&mut self, index: usize) {
        if let Some(id) =
            self.files.get(index).and_then(|file| file.browser.as_ref()).map(|tab| tab.id)
        {
            self.browser.close_tab(id);
        }
        // Written down **before** it is closed, because afterwards there is no tab to ask where it
        // was. `task-1922` WP4.
        self.remember_a_closed_tab(index);
        self.save_before_closing(index);
        self.files.close(index);
        self.forget_layout();
    }

    /// Write a tab that is about to be closed, if it has unsaved changes and somewhere to put them.
    ///
    /// Two tabs are deliberately left alone. **A picture** holds an empty document over the
    /// picture's path, so writing it would put nothing over the file — `save` already refuses for
    /// this reason. **A tab with no path** has nowhere to be written, and choosing one is a dialog,
    /// which is the thing this is removing; it says so rather than writing `untitled.md` into
    /// somebody's project because they shut a scratch buffer.
    fn save_before_closing(&mut self, index: usize) {
        let Some(file) = self.files.get(index) else {
            return;
        };
        if file.is_picture() || file.is_browser() || !file.document.is_modified() {
            return;
        }
        let Some(path) = file.path().map(Path::to_path_buf) else {
            self.message = Some(
                "That tab has no file to save to, so it was closed without saving.".to_owned(),
            );
            return;
        };
        let name = file.name();
        // The same trim a save from the menu does, because closing a modified tab **is** a save.
        self.trim_before_writing(index);
        match self.files.at_mut(index).document.save() {
            Ok(()) => {
                self.files.at_mut(index).note_what_is_on_disk();
                self.message = Some(format!("Saved {name}"));
                // The disk is what the index holds for every file that is not open, and this one is
                // about to stop being open.
                self.the_project_changed_on_disk();
            }
            Err(problem) => {
                self.message =
                    Some(format!("Unluminous could not save {}: {problem}", path.display()))
            }
        }
    }

    /// Close a tab without writing it, which is what deleting its file means and what
    /// `unluminous-cli tab close --discard` asks for.
    pub fn close_tab_without_saving(&mut self, index: usize) {
        if let Some(id) =
            self.files.get(index).and_then(|file| file.browser.as_ref()).map(|tab| tab.id)
        {
            self.browser.close_tab(id);
        }
        // A tab closed with `--discard` is still a tab somebody may want back, and what reopening it
        // means is reading the file again — which is what it would have meant either way.
        self.remember_a_closed_tab(index);
        self.files.close(index);
        self.forget_layout();
    }

    /// The name shown in the title bar and the status bar.
    pub(crate) fn file_name(&self) -> String {
        self.files.active().name()
    }

    /// The folder shown after the file name in the title bar.
    pub(crate) fn folder_name(&self) -> Option<String> {
        self.tree.root().file_name().map(|name| name.to_string_lossy().to_string())
    }

    pub(crate) fn save(&mut self) {
        // A tab showing a picture holds an empty document over the picture's path, so saving it would
        // write nothing over the file. There is nothing in a picture Unluminous can change, so there is
        // nothing to save.
        if self.files.active().is_picture() {
            self.message =
                Some("A picture cannot be edited, so there is nothing to save.".to_owned());
            return;
        }
        // A tab a plugin draws holds an empty document with no path, so saving it would write an empty
        // `untitled.md` into the project. There is nothing in it to save: what a plugin holds, the plugin
        // keeps — the board is a database file of its own.
        if let Some(plugin) = self.files.active().plugin.clone() {
            self.message = Some(format!(
                "{} is not a file, so there is nothing to save. It keeps what it holds itself.",
                plugin.label
            ));
            return;
        }
        if self.document().path().is_none() {
            // With no file to save to, write into the folder the explorer is showing rather than silently
            // doing nothing.
            let target = self.tree.root().join("untitled.md");
            if self.document_mut().save_as(&target).is_ok() {
                self.files.active_mut().note_what_is_on_disk();
                self.tree.reload();
                self.the_project_changed_on_disk();
            }
            return;
        }
        // `editor.trim`, before the bytes are written and as an ordinary `Command`, so it is one undo
        // step somebody who did not mean it can put back. Off unless it has been asked for, and never
        // on a Markdown file. `task-1922` WP4.
        let index = self.files.active_index();
        self.trim_before_writing(index);
        // The setting is applied at the moment of writing rather than at the moment of opening, so
        // that changing it takes effect on the next save of a file that is already open, and so that
        // `keep` -- the default -- never touches what was read. `task-1804` §7.1.
        let chosen = self.settings.line_endings.applied_to(self.document().line_ending());
        self.document_mut().set_line_ending(chosen);
        // A file Unluminous reads and does not write refuses here rather than silently doing nothing,
        // which is the same rule `tab open` is held to in §7.2: a command that could not do what it
        // was asked says so.
        if let Err(refusal) = self.document_mut().save() {
            self.message = Some(refusal.to_string());
            return;
        }
        // Written by this tab, so this tab and the file agree again: without this the tab's own write
        // would look like somebody else's change and the next read would re-read what it just wrote.
        self.files.active_mut().note_what_is_on_disk();
        // The file on the disk is what the index holds for every file that is not open, and this
        // one is about to stop being open one day. Reading the project again is tens of
        // milliseconds on a thread, and saving is not something anybody does sixty times a second.
        self.the_project_changed_on_disk();
    }
}
