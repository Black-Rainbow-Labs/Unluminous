//! Breakpoints: where they live, what moves them, and what the adapter said about each one.
//!
//! They live in the `Document`, as the byte offset of each line's start, so the two functions that
//! know a range of bytes moved shift them in the same lines that already shift the highlights and
//! the folds. One rule decides every awkward case: **a file that is open is owned by its `Document`,
//! and every other file is owned by the store.**
//!
//! Unluminous draws the adapter's answer rather than its own hope: one it moved is drawn where it put
//! it, and one it could not bind stays hollow for the life of the session.

use std::path::{Path, PathBuf};

use crate::components::debug_dialogs::BreakpointDialog;

use crate::app::{line_number_in_file, UnluminousApp};

impl UnluminousApp {
    /// Put a breakpoint on the caret's line, or take away the one that is there.
    pub(crate) fn toggle_breakpoint_here(&mut self) {
        let caret = self.document().selection().head;
        let line = self.document_mut().line_start_of(caret);
        self.toggle_breakpoint_at_offset(line);
    }

    /// The same, from a click in the gutter, which names a paragraph rather than an offset.
    pub(crate) fn toggle_breakpoint_at_line(&mut self, paragraph: usize) {
        let offset = self.document().text().line_to_byte(paragraph);
        self.toggle_breakpoint_at_offset(offset);
    }

    /// The one place a breakpoint is put on or taken off the file that is showing.
    ///
    /// **Absent rather than refused** for a file whose language names no debugger: the gutter takes
    /// no click there at all, and this says so for the keyboard and the menu, which can still ask.
    fn toggle_breakpoint_at_offset(&mut self, offset: usize) {
        if !self.debug_applies_here() {
            self.message =
                Some("This file's language has not said which debugger to use.".to_owned());
            return;
        }
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            self.message =
                Some("Save the file first, so a breakpoint has somewhere to live.".to_owned());
            return;
        };
        let now = self.document_mut().toggle_breakpoint(offset);
        let line = self.document().line_number_of(offset);
        self.message = Some(match now {
            true => format!("Breakpoint on line {line}"),
            false => format!("Breakpoint removed from line {line}"),
        });
        self.send_the_breakpoints_of(&path);
    }

    /// Which line the gutter's menu and the breakpoint entries are about: the row it was opened
    /// over, or the caret's line when the question came from the keyboard or the command line.
    fn breakpoint_line_in_question(&self) -> usize {
        match self.gutter_menu_line {
            Some(paragraph) => self.document().text().line_to_byte(paragraph),
            None => {
                let caret = self.document().selection().head;
                self.document().line_start_of(caret)
            }
        }
    }

    /// The breakpoint on that line, if there is one. What the gutter's menu asks to decide whether
    /// it offers to set one or to remove one.
    pub(crate) fn breakpoint_in_question(&self) -> Option<&unluminous_core::Breakpoint> {
        self.document().breakpoints().at(self.breakpoint_line_in_question())
    }

    /// Switch the breakpoint in question off without taking it away, or back on again.
    ///
    /// A disabled breakpoint keeps its condition and its log message and is drawn hollow; it is
    /// simply not sent to the adapter, because `enabled` is Unluminous's own idea and the protocol has no
    /// field for it.
    pub(crate) fn toggle_the_breakpoint_enabled(&mut self) {
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            return;
        };
        let offset = self.breakpoint_line_in_question();
        let Some(was) = self.document().breakpoints().at(offset).map(|one| one.enabled) else {
            self.message = Some("There is no breakpoint on that line.".to_owned());
            return;
        };
        self.document_mut().change_breakpoint(offset, |breakpoint| breakpoint.enabled = !was);
        let line = self.document().line_number_of(offset);
        self.message = Some(match was {
            true => format!("Breakpoint on line {line} disabled"),
            false => format!("Breakpoint on line {line} enabled"),
        });
        self.send_the_breakpoints_of(&path);
    }

    /// Open `Edit Breakpoint...` on the line in question, putting one there if there is none.
    pub(crate) fn open_the_breakpoint_dialog(&mut self) {
        if !self.debug_applies_here() {
            self.message =
                Some("This file's language has not said which debugger to use.".to_owned());
            return;
        }
        let Some(path) = self.document().path().map(Path::to_path_buf) else {
            self.message =
                Some("Save the file first, so a breakpoint has somewhere to live.".to_owned());
            return;
        };
        let offset = self.breakpoint_line_in_question();
        let created = self.document().breakpoints().at(offset).is_none();
        if created {
            self.document_mut().toggle_breakpoint(offset);
        }
        let breakpoint = self.document().breakpoints().at(offset).cloned().unwrap_or_default();
        // A field whose capability is absent is absent. With **no session running** both are offered,
        // because a breakpoint edited now is one a debugger will be asked about later and refusing to
        // let somebody type a condition before they have pressed Debug would be absurd.
        let (conditions, log_points) = match self.debug.as_ref() {
            Some(debug) => {
                (debug.capabilities().conditional_breakpoints, debug.capabilities().log_points)
            }
            None => (true, true),
        };
        self.close_every_modal();
        self.breakpoint_dialog = Some(BreakpointDialog {
            path,
            offset,
            line: self.document().line_number_of(offset),
            enabled: breakpoint.enabled,
            condition: breakpoint.condition.clone().unwrap_or_default(),
            log_message: breakpoint.log_message.clone().unwrap_or_default(),
            conditions,
            log_points,
            created,
        });
    }

    /// Change what is set in any file of this project, whether it is open or not.
    ///
    /// **The one place that choice is made**, so no caller has to think about it: a file that is
    /// open is owned by its `Document`, and every other file is owned by `services::breakpoint_store`.
    /// It is `change_highlights` with one word changed, deliberately — the rule is the same rule and
    /// a second answer to it would be a second thing to keep in step.
    pub fn change_breakpoints(
        &mut self,
        path: &Path,
        change: impl FnOnce(&mut unluminous_core::Breakpoints),
    ) -> bool {
        if let Some(index) = self.files.index_of(path) {
            let mut breakpoints = self.files.at(index).document.breakpoints().clone();
            let before = breakpoints.clone();
            change(&mut breakpoints);
            if before == breakpoints {
                return false;
            }
            self.files.at_mut(index).document.set_breakpoints(breakpoints);
            return true;
        }
        self.breakpoints.change(path, change)
    }

    /// What is set in one file, whether it is open or not.
    pub fn breakpoints_of(&self, path: &Path) -> unluminous_core::Breakpoints {
        if let Some(index) = self.files.index_of(path) {
            return self.files.at(index).document.breakpoints().clone();
        }
        self.breakpoints.breakpoints(path).cloned().unwrap_or_default()
    }

    /// Every file in this project that has a breakpoint in it, open or not, in path order.
    pub fn every_breakpoint(&self) -> Vec<(PathBuf, unluminous_core::Breakpoints)> {
        let mut files: Vec<(PathBuf, unluminous_core::Breakpoints)> = Vec::new();
        for (path, breakpoints) in self.breakpoints.files() {
            files.push((path.clone(), breakpoints.clone()));
        }
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
                continue;
            };
            let breakpoints = self.files.at(index).document.breakpoints().clone();
            // The document owns an open file, so its set replaces whatever the store had.
            files.retain(|(known, _)| *known != path);
            if !breakpoints.is_empty() {
                files.push((path, breakpoints));
            }
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));
        files
    }

    /// Push what every open document holds into the store, and write the store if it changed.
    ///
    /// Called every frame and does almost nothing: an integer comparison for each open tab, because
    /// a document that has not changed since it was last pushed cannot have new breakpoints in it.
    /// `remember_the_marks`'s arrangement exactly, keyed on the same revision.
    pub(crate) fn remember_the_breakpoints(&mut self, settled: bool) {
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
                continue;
            };
            let revision = self.files.at(index).document.revision();
            if self.files.at(index).breakpoints_at == Some(revision) {
                continue;
            }
            let breakpoints = self.files.at(index).document.breakpoints().clone();
            self.breakpoints.set(&path, breakpoints);
            self.files.at_mut(index).breakpoints_at = Some(revision);
        }
        if self.remembers_this_project() && settled {
            let root = self.tree.root().to_path_buf();
            self.breakpoints.save(&root);
        }
    }

    /// Tell the adapter what one file's breakpoints are now.
    ///
    /// A disabled breakpoint is **not sent**: `enabled` is Unluminous's own idea and the protocol has no
    /// field for it, so switching one off means taking it out of the set the adapter holds. The dot
    /// stays, drawn hollow.
    pub(crate) fn send_the_breakpoints_of(&mut self, path: &Path) {
        let breakpoints = self.breakpoints_of(path);
        let (conditions, logs) = match self.debug.as_ref() {
            Some(debug) => {
                (debug.capabilities().conditional_breakpoints, debug.capabilities().log_points)
            }
            None => return,
        };
        let document_index = self.files.index_of(path);
        let lines: Vec<(usize, unluminous_dap::SourceBreakpoint)> = breakpoints
            .iter()
            .filter(|breakpoint| breakpoint.enabled)
            .map(|breakpoint| {
                let line = match document_index {
                    Some(index) => self.files.at(index).document.line_number_of(breakpoint.offset),
                    // A file that is not open has no laid-out text to count lines in, so its own
                    // bytes are read at the moment of use — the ownership rule's disk half, and the
                    // same "re-read rather than watch" `open_the_match` already does.
                    None => line_number_in_file(path, breakpoint.offset),
                };
                let carried = unluminous_dap::SourceBreakpoint {
                    line,
                    // Never sent to an adapter that did not offer it, which is the rule every
                    // optional feature here follows.
                    condition: conditions.then(|| breakpoint.condition.clone()).flatten(),
                    log_message: logs.then(|| breakpoint.log_message.clone()).flatten(),
                };
                (breakpoint.offset, carried)
            })
            .collect();
        if let Some(debug) = self.debug.as_mut() {
            debug.set_breakpoints(path, lines);
        }
    }

    /// Tell the adapter about every file that has one, which is what a session start does.
    ///
    /// The store is re-read first when something else has written it. A session start is a **moment
    /// of use**, which is where the disk-owned side is re-checked — the rule the `editor` commands
    /// and the symbol index already keep — and it is the moment that matters here, because a
    /// `git checkout` a moment before `debug start` would otherwise have the window send the offsets
    /// it was holding against the bytes the checkout brought back.
    pub(crate) fn send_every_breakpoint(&mut self) {
        self.adopt_the_breakpoints_from_disk();
        for (path, _) in self.every_breakpoint() {
            self.send_the_breakpoints_of(&path);
        }
    }

    /// Read `.unluminous/breakpoints.conf` again when something else has written it, and take what it says.
    ///
    /// `task-1794`: a breakpoint is a byte offset into a file, and putting the project back — a
    /// `git checkout`, a branch switch, a revert — puts the file **and** this file back together. A
    /// window that goes on holding the offsets it had is then holding offsets against bytes that have
    /// gone, and the adapter declines to bind them: the same silent "the program just does not stop"
    /// as the mixed separator above, from the other direction.
    ///
    /// The ownership rule decides what happens next, unchanged: **a file that is open is owned by its
    /// `Document`**, so a document that has been re-read has to be told, and the store's copy is
    /// pushed into every open tab. A tab with **unsaved changes** is left exactly alone — its offsets
    /// belong to the text the person is editing rather than to the text on the disk, which is the
    /// same answer `the_file_changed_underneath` gives about the words themselves.
    ///
    /// True when anything was adopted, so a caller can say so.
    pub(crate) fn adopt_the_breakpoints_from_disk(&mut self) -> bool {
        if !self.remembers_this_project() {
            return false;
        }
        let root = self.tree.root().to_path_buf();
        if !self.breakpoints.changed_on_disk(&root) {
            return false;
        }
        self.breakpoints.reload(&root);
        // The file and the offsets into it were put back **together**, so a tab still holding the
        // text from before is a tab the restored offsets do not describe — and pushing them into it
        // would not merely be wrong, it would *destroy* them, because `Document::set_breakpoints`
        // clamps to the text it has. Measured: an offset of 56 restored into a tab holding the 48
        // bytes from before came out as 48, which the window then wrote back over the checkout.
        //
        // So a tab whose own file changed underneath is read again first, through the one way in
        // that already exists. This is not Unluminous starting to watch files: it is reached only when
        // `.unluminous/breakpoints.conf` itself has been written by something else, which does not happen
        // while somebody is editing. A tab with unsaved changes is still never touched —
        // `reload_from_disk` refuses one, and those belong to the person.
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
                continue;
            };
            if self.files.at(index).the_file_changed_underneath() {
                self.reload_from_disk(&path, false);
            }
        }
        for index in 0..self.files.len() {
            let Some(path) = self.files.at(index).path().map(Path::to_path_buf) else {
                continue;
            };
            if self.files.at(index).document.is_modified() {
                continue;
            }
            let wanted = self.breakpoints.breakpoints(&path).cloned().unwrap_or_default();
            self.files.at_mut(index).document.set_breakpoints(wanted);
            // Stamped as pushed, or `remember_the_breakpoints` would put what the document held a
            // moment ago straight back over the file that has just been read.
            let revision = self.files.at(index).document.revision();
            self.files.at_mut(index).breakpoints_at = Some(revision);
        }
        true
    }
}
