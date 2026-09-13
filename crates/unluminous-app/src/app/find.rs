//! Find and Replace in the file that is showing.
//!
//! A bar rather than a modal, and that is the one design decision worth writing down: `Go to File`
//! and `Find in Files` are modals because they are about the *project* and their answer is a list you
//! read; find in the current file is about the text you are looking at, and a modal over it would
//! cover the thing being searched.
//!
//! Nothing about the search is decided here — `services::find` holds the needle, the matches and
//! which one is current, and is a unit test with no window.

use egui::Rect;
use unluminous_core::Command;

use crate::components::find_in_files::FindInFiles;

use crate::app::{Focus, UnluminousApp};

impl UnluminousApp {
    /// Draw the Find bar and act on what was pressed.
    ///
    /// `task-1804` §3.1. The keys are taken out of the frame's events **before** the bar is
    /// drawn, which is `go_to_file`'s rule: egui leaves the events a text box consumed in the list
    /// for everyone else to read, so Enter typed into the Find box would otherwise also reach the
    /// document and put a line break in the file being searched.
    pub(crate) fn show_the_find_bar(&mut self, ui: &mut egui::Ui, area: Rect) {
        let (enter, shift_enter, escape) = ui.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let mut outcome = crate::components::find_bar::show(ui, area, find);
        outcome.next |= enter;
        outcome.previous |= shift_enter;
        outcome.close |= escape;

        if outcome.close {
            self.close_the_find();
            return;
        }
        // Replace before the step, because replacing changes the text and `refresh` then works the
        // matches out again on the next frame -- stepping first would move off the match that is
        // about to be replaced.
        if outcome.replace_all {
            self.replace_every_match();
        } else if outcome.replace {
            self.replace_the_current_match();
        } else if outcome.next {
            self.step_the_find(true);
        } else if outcome.previous {
            self.step_the_find(false);
        }
        if outcome.in_files {
            self.take_the_search_to_the_project();
        }
    }

    /// Move to the next or the previous match and select it.
    ///
    /// **Selecting it rather than only putting the caret there** is `open_the_match`'s rule: a
    /// selection is how a document shows a piece of itself, and it is what makes Escape leave the
    /// caret on the match you stopped at and `Ctrl+C` copy it.
    pub(crate) fn step_the_find(&mut self, forward: bool) {
        let Some(find) = self.find.as_mut() else {
            return;
        };
        let range = if forward { find.next() } else { find.previous() };
        let Some(range) = range else {
            return;
        };
        self.select_the_match(range);
    }

    /// Put the selection over `range` and scroll it into view.
    pub(crate) fn select_the_match(&mut self, range: std::ops::Range<usize>) {
        let length = self.document().text().len_bytes();
        if range.end > length {
            return;
        }
        self.document_mut().apply(Command::PlaceCaret { offset: range.start, extend: false });
        self.document_mut().apply(Command::PlaceCaret { offset: range.end, extend: true });
        self.reveal_caret = true;
    }

    /// Replace the match the bar is on, as one undo step.
    pub(crate) fn replace_the_current_match(&mut self) {
        let Some((range, with)) =
            self.find.as_ref().and_then(|find| find.replacement_for_current())
        else {
            return;
        };
        if range.end > self.document().text().len_bytes() {
            return;
        }
        self.document_mut().apply(Command::ReplaceMany(vec![(range.clone(), with.clone())]));
        // The caret goes after what was written, so pressing Replace again lands on the next match
        // rather than on this one over again.
        let after = range.start + with.len();
        self.document_mut().apply(Command::PlaceCaret { offset: after, extend: false });
        self.reveal_caret = true;
        self.message = None;
    }

    /// Replace every match, as **one** undo step.
    ///
    /// Through `Command::ReplaceMany`, which is what `editor rename` is applied by and which exists
    /// for exactly this: undo restores a snapshot, so one snapshot and then every edit is one step.
    /// Forty occurrences replaced one at a time would be forty presses of `Ctrl+Z` to get back.
    pub(crate) fn replace_every_match(&mut self) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let edits = find.replacements_for_all();
        if edits.is_empty() {
            self.message = Some("Nothing matches, so there is nothing to replace.".to_owned());
            return;
        }
        let count = edits.len();
        let length = self.document().text().len_bytes();
        if edits.iter().any(|(range, _)| range.end > length) {
            return;
        }
        self.document_mut().apply(Command::ReplaceMany(edits));
        self.message = Some(format!(
            "Replaced {count} {} in this file. One undo puts them all back.",
            if count == 1 { "match" } else { "matches" }
        ));
    }

    /// Hand the search to `Find in Files`, so the same words can be replaced across the project.
    ///
    /// The bar's own Replace is about the file that is showing. Replacing across a project is a
    /// different and more serious thing -- it writes files nobody has open -- so it happens in the
    /// modal that already lists what it would touch, rather than behind a button on a bar.
    fn take_the_search_to_the_project(&mut self) {
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let (needle, replacement) = (find.needle.clone(), find.replacement.clone());
        let (match_case, whole_word) = (find.match_case, find.whole_word);
        self.close_the_find();
        // The folder is read again first, for the reason `Find in Files` reads it when it is opened
        // from the menu: a file made since the window opened is part of this project.
        self.tree.reload();
        let mut modal = FindInFiles::open(self.thread_waker());
        modal.query = needle;
        modal.match_case = match_case;
        // Whole words are not a mode Find in Files has: it searches text rather than asking a
        // grammar what a hit was found inside, which is Find References' question. So the toggle is
        // not carried over and the modal says what it is really doing.
        let _ = whole_word;
        modal.replacement = replacement;
        modal.replacing = true;
        self.find_in_files = Some(modal);
    }

    /// Put the bar away, leaving the caret on the match it was on.
    pub(crate) fn close_the_find(&mut self) {
        self.find = None;
        self.focus = Focus::Editor;
        self.reveal_caret = true;
    }
}
