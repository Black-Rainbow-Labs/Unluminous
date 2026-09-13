//! The line commands, the comment toggles, the bracket pair, `Go to Line`, the palette and the
//! closed tabs — `task-1922` WP4.
//!
//! Everything here is the window's half of something `unluminous-core` already decided. The seven
//! editing commands are `Command` variants applied through `Document::apply`, so each is one undo
//! step by construction and each is already walked by `every_command()`; the bracket pair is
//! `Document::bracket_pair`; the indentation a new line starts with is
//! `unluminous_core::indentation_for_a_new_line`. What is left for the window is the three things
//! the crate deliberately does not know: **which marker this language comments with**, which comes
//! from the plugin; **whether the setting says to do it**, which is `settings.rs`; and **what a
//! person sees**, which is the menu entry, the prompt and the palette.
//!
//! ## One thing does not fit, and it is said here rather than left to be found
//!
//! `editor.indent` can say `spaces:4`, and what that changes is what the `Tab` key **types** where
//! nothing is selected. Indenting a **selection** still moves each line by one character, because
//! `unluminous_core::IndentUnit` is a character and the crate says why: *Unluminous has no tab width
//! and no "insert spaces for tabs" preference, and the character the key says is the one answer that
//! needs no setting.* Applying `Command::Indent` four times would be four undo steps, and rebuilding
//! the edit here out of `Command::ReplaceMany` would be a second implementation of a command the
//! crate already has. The fix is one field — a width on `Command::Indent` — and it belongs in
//! `unluminous-core` beside the loop that would read it.

use std::path::PathBuf;

use unluminous_core::Command;

use crate::app::actions::Action;
use crate::app::files::Home;
use crate::app::UnluminousApp;
use crate::components::command_palette::{self, CommandPalette};
use crate::components::prompt_dialog::{Prompt, Purpose};
use crate::services::file_kind;

/// How many closed tabs are remembered, which is what `Reopen Closed Tab` walks.
///
/// Ten, which is `task-1922` §5.5's number and is what every editor keeps. It is travel history
/// rather than state — bounded, and not written to disk — which is the line `app::back` already
/// draws about the places a jump has been from.
pub const CLOSED_TABS_KEPT: usize = 10;

/// A tab that was closed, and where it was.
///
/// The **path**, not the document: a tab closed with unsaved changes has already been written by
/// `save_before_closing`, and one closed with `--discard` was discarded on purpose, so what reopening
/// it means in both cases is reading the file again. `task-1922` §5.5 says so — *a tab closed without
/// saving reopens from disk.*
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedTab {
    pub path: PathBuf,
    /// The pane or canvas node it was closed from, so it comes back where it was.
    pub home: Home,
}

impl UnluminousApp {
    // ------------------------------------------------------------------- the comment toggles

    /// The marker this file's language starts a line comment with, or nothing.
    ///
    /// One question asked in one place, which is what keeps the menu entry, the key chord and
    /// `editor comment --toggle` from disagreeing about a file.
    pub(crate) fn line_comment_marker(&self) -> Option<String> {
        file_kind::line_comment(self.document().path(), self.plugins.grammars())
    }

    /// The pair this file's language opens and closes a block comment with, or nothing.
    pub(crate) fn block_comment_markers(&self) -> Option<(String, String)> {
        file_kind::block_comment(self.document().path(), self.plugins.grammars())
    }

    /// True when the tab that is showing holds text that can be edited a line at a time.
    ///
    /// A picture holds an empty document over the picture's path and a browser tab holds an empty
    /// document behind a native view, so neither has lines to duplicate, move, join or sort.
    pub(crate) fn line_edits_apply_here(&self) -> bool {
        let file = self.files.active();
        !file.is_picture() && !file.is_browser() && !file.is_a_plugin()
    }

    /// Comment or uncomment the lines the selection touches, with the language's own marker.
    ///
    /// Refused in a sentence rather than silently when the language names none, because the menu
    /// entry is absent in that case and this is reachable from the command line whatever is open.
    pub(crate) fn toggle_line_comment(&mut self) -> Result<usize, String> {
        let Some(marker) = self.line_comment_marker() else {
            return Err(self.no_comment_marker("line"));
        };
        let lines = self.lines_the_selection_touches();
        self.document_mut().apply(Command::ToggleLineComment { marker });
        self.forget_layout();
        self.reveal_caret = true;
        Ok(lines)
    }

    /// Wrap the selection in the language's block comment markers, or take them off.
    pub(crate) fn toggle_block_comment(&mut self) -> Result<usize, String> {
        let Some((open, close)) = self.block_comment_markers() else {
            return Err(self.no_comment_marker("block"));
        };
        let lines = self.lines_the_selection_touches();
        self.document_mut().apply(Command::ToggleBlockComment { open, close });
        self.forget_layout();
        self.reveal_caret = true;
        Ok(lines)
    }

    /// Why a file cannot be commented, naming the language rather than the file.
    ///
    /// `unluminous-git`'s rule about never inventing an error message does not reach here — there is
    /// no other program to quote — so what this does instead is say which of the two things is
    /// missing: the language, or that language's marker.
    fn no_comment_marker(&self, which: &str) -> String {
        let path = self.document().path().map(|path| path.to_path_buf());
        match path.as_deref().and_then(|path| self.plugins.grammars().for_path(path)) {
            Some(_) => format!("This file's language has no {which} comment."),
            None => "No plugin claims this file, so Unluminous does not know how it is commented."
                .to_owned(),
        }
    }

    // --------------------------------------------------------------------- the line commands

    /// Copy the lines the selection touches in below themselves.
    pub(crate) fn duplicate_lines(&mut self) -> usize {
        let lines = self.lines_the_selection_touches();
        self.document_mut().apply(Command::DuplicateLines);
        self.forget_layout();
        self.reveal_caret = true;
        lines
    }

    /// Move the lines the selection touches up or down, carrying the selection with them.
    ///
    /// Answers whether anything moved, because a block already against the top or the bottom of the
    /// file does not — which is what the command line reports rather than claiming a change.
    pub(crate) fn move_lines(&mut self, by: i32) -> bool {
        let moved = self.document_mut().apply(Command::MoveLines { by });
        if moved {
            self.forget_layout();
            self.reveal_caret = true;
        }
        moved
    }

    /// Join the lines the selection touches into one.
    pub(crate) fn join_lines(&mut self) -> bool {
        let joined = self.document_mut().apply(Command::JoinLines);
        if joined {
            self.forget_layout();
            self.reveal_caret = true;
        }
        joined
    }

    /// Sort the lines the selection touches, stably, in byte order.
    ///
    /// Nothing selected, nothing sorted, which is the crate's own decision: one line is already in
    /// order and a command that silently sorted the whole file would be one nobody could undo in
    /// their head. The menu row is dimmed in that case, and this says so.
    pub(crate) fn sort_lines(&mut self) -> Result<usize, String> {
        if self.document().selection().is_empty() {
            return Err("Select the lines to sort. Sorting one line would change nothing, and \
                        sorting the whole file is not what a command with nothing chosen should do."
                .to_owned());
        }
        let lines = self.lines_the_selection_touches();
        self.document_mut().apply(Command::SortLines);
        self.forget_layout();
        self.reveal_caret = true;
        Ok(lines)
    }

    /// Take the trailing whitespace off every line of the tab that is showing.
    ///
    /// Answers whether anything changed. Refused for a file where trailing whitespace means
    /// something, which is Markdown.
    pub(crate) fn trim_trailing_whitespace(&mut self) -> Result<bool, String> {
        if !self.line_edits_apply_here() {
            return Err("This tab holds no text to trim.".to_owned());
        }
        if !file_kind::trimming_applies(self.document().path()) {
            return Err("Two spaces at the end of a line are a line break in Markdown, so \
                        trailing whitespace is left alone here."
                .to_owned());
        }
        let trimmed = self.document_mut().apply(Command::TrimTrailingWhitespace);
        if trimmed {
            self.forget_layout();
        }
        Ok(trimmed)
    }

    /// How many lines the selection touches, which is what a reply counts.
    fn lines_the_selection_touches(&self) -> usize {
        let document = self.document();
        let selection = document.selection();
        if selection.is_empty() {
            return 1;
        }
        let text = document.text();
        text.byte_to_line(selection.end().saturating_sub(1)) - text.byte_to_line(selection.start())
            + 1
    }

    // ------------------------------------------------------------------- the bracket and the line

    /// The two brackets that answer each other around the caret, when there are two.
    ///
    /// **The comments and strings come from the tab**, not from a second reading: `colour_the_file`
    /// has already run `syntax::scan` over this text at this revision and kept what it found, and a
    /// second pass was worth 2.5 ms a keystroke on the largest file in this repository. A tab whose
    /// colours are stale, or one no plugin claims, is asked with an empty reading and gets an answer
    /// that does not know about comments — which is `Document::bracket_pair`'s own bargain.
    pub(crate) fn bracket_pair_at_the_caret(&self) -> Option<(usize, usize)> {
        self.bracket_pair_at(self.document().selection().head)
    }

    /// The same about any offset, which is what the painter asks and what `editor bracket` takes.
    pub(crate) fn bracket_pair_at(&self, offset: usize) -> Option<(usize, usize)> {
        let file = self.files.active();
        let revision = file.document.text_revision();
        let nothing = unluminous_core::folding::Tokens::default();
        let read = match file.cached.fold_tokens.as_ref() {
            Some((at, tokens)) if *at == revision => tokens,
            _ => &nothing,
        };
        file.document.bracket_pair(offset, read)
    }

    /// Move the caret to the bracket answering the one beside it.
    ///
    /// Answers where it went, or nothing when the caret is not beside a bracket that has a partner.
    pub(crate) fn go_to_matching_bracket(&mut self) -> Option<usize> {
        let (_, other) = self.bracket_pair_at_the_caret()?;
        self.document_mut().apply(Command::PlaceCaret { offset: other, extend: false });
        self.reveal_caret = true;
        Some(other)
    }

    /// Open the prompt that asks which line to go to.
    ///
    /// Seeded with the line the caret is on, which is what the status bar is already showing, so
    /// `Cmd+L` then Enter is not a jump to somewhere else.
    pub(crate) fn ask_which_line(&mut self) {
        let document = self.document();
        let line = document.text().byte_to_line(document.selection().head) + 1;
        let lines = document.text().len_lines();
        self.prompt = Some(Prompt::new(
            "Go to Line",
            &format!("A line number, or line:column. This file has {lines} lines."),
            &line.to_string(),
            "Go",
            Purpose::GoToLine,
        ));
    }

    /// Read `12` or `12:5` and put the caret there.
    ///
    /// Both count from 1, which is what the status bar shows and what `editor caret` takes. A line
    /// past the end is the end rather than a refusal, which is what every `Go to Line` does and what
    /// somebody typing `9999` means.
    pub(crate) fn go_to_line(&mut self, typed: &str) -> Result<(usize, usize), String> {
        let typed = typed.trim();
        let (line, column) = match typed.split_once(':') {
            Some((line, column)) => (line.trim(), Some(column.trim())),
            None => (typed, None),
        };
        let line: usize = line
            .parse()
            .map_err(|_| format!("{typed} is not a line number. Write 42, or 42:5."))?;
        let column: usize = match column {
            Some(column) => column
                .parse()
                .map_err(|_| format!("{typed} is not a line and column. Write 42, or 42:5."))?,
            None => 1,
        };
        if line == 0 || column == 0 {
            return Err(
                "Lines and columns count from 1, which is what the status bar shows.".to_owned()
            );
        }
        // The same reckoning `editor caret --line --column` uses, from the same function, so the
        // prompt and the command line cannot land in two different places.
        let offset = crate::app::cli::offset_at(self.document().text(), line, column);
        self.document_mut().apply(Command::PlaceCaret { offset, extend: false });
        self.reveal_caret = true;
        let landed = self.document().text().byte_to_line(offset) + 1;
        Ok((landed, column))
    }

    // ------------------------------------------------------------------------- the closed tabs

    /// Write down where a tab was before it is closed, so it can be opened again.
    ///
    /// Called by both closing paths, because a tab closed with `--discard` is still a tab somebody
    /// may want back. A tab with no path has nothing to reopen from — there is no file behind an
    /// untitled buffer — and a plugin's own tab is the plugin's to open.
    pub(crate) fn remember_a_closed_tab(&mut self, index: usize) {
        let Some(file) = self.files.get(index) else {
            return;
        };
        if file.is_a_plugin() {
            return;
        }
        let Some(path) = file.path().map(|path| path.to_path_buf()) else {
            return;
        };
        let home = self.files.home_of(index);
        // The same file closed twice is one row rather than two, so pressing the chord twice reaches
        // two different files, which is what a person means by "the one before that".
        self.closed_tabs.retain(|closed| closed.path != path);
        self.closed_tabs.push(ClosedTab { path, home });
        if self.closed_tabs.len() > CLOSED_TABS_KEPT {
            self.closed_tabs.remove(0);
        }
    }

    /// Open the last tab that was closed, in the pane it was closed from.
    ///
    /// **The home is borrowed the way the pane loop borrows it**: opening a file lands it wherever
    /// the keyboard is, so the keyboard is put where the tab used to live and the file is opened
    /// there. A pane or a node that has since gone is not restored to — `OpenFiles::restore_focus`
    /// clamps a pane number into the panes there are — which is right: the file comes back in the
    /// editing area rather than not at all.
    pub(crate) fn reopen_the_last_closed_tab(&mut self) -> Result<PathBuf, String> {
        let Some(closed) = self.closed_tabs.pop() else {
            return Err("No tab has been closed in this window.".to_owned());
        };
        if !closed.path.exists() {
            return Err(format!(
                "{} is not there any more, so there is nothing to reopen.",
                closed.path.display()
            ));
        }
        let was = self.files.focus();
        let goes_to_a_node =
            closed.home.node().is_some_and(|node| self.space.space.current().node(node).is_some());
        if goes_to_a_node || closed.home.pane().is_some() {
            self.files.restore_focus(closed.home);
        }
        let opened = self.open_path_permanently(&closed.path);
        if opened.is_err() {
            self.files.restore_focus(was);
        }
        opened.map(|()| closed.path)
    }

    // ------------------------------------------------------------------------- the palette

    /// Every menu entry there is, as a row the palette can offer and `action find` can rank.
    ///
    /// **Built by walking the real menus**, which is `cli_action::every_menu_entry`'s rule and the
    /// reason it is one function rather than two: a menu entry added tomorrow is in the palette
    /// tomorrow, with no list anywhere to add it to.
    pub(crate) fn every_menu_command(&self) -> Vec<command_palette::Command> {
        use crate::app::actions::{self, Entry};
        fn walk(entries: &[Entry], menu: &str, out: &mut Vec<command_palette::Command>) {
            for entry in entries {
                match entry {
                    Entry::Item { name, action, shortcut, enabled, checked, .. } => {
                        out.push(command_palette::Command {
                            name: action.name(),
                            label: name.clone(),
                            menu: menu.to_owned(),
                            shortcut: shortcut.map(|keys| keys.label()).unwrap_or_default(),
                            enabled: *enabled,
                            checked: *checked,
                            action: action.clone(),
                        });
                    }
                    Entry::Submenu { name, entries } => walk(entries, name, out),
                    Entry::Separator => {}
                }
            }
        }
        let mut out = Vec::new();
        for menu in actions::menus(&self.menu_state()) {
            walk(&menu.entries, &menu.name, &mut out);
        }
        out
    }

    /// Open the palette over the entries the menus hold right now.
    pub(crate) fn open_the_command_palette(&mut self) {
        self.palette = Some(CommandPalette::open(self.every_menu_command()));
    }

    /// Run what the palette chose, or say why it cannot be run just now.
    ///
    /// A row that is dimmed is **refused with a reason** rather than being left out of the list:
    /// somebody looking for `Redo` wants to be told there is nothing to redo, not told there is no
    /// such command.
    ///
    /// Public so a test can drive it, which is `run_prompt_for_test`'s own reason: a test can put a
    /// query in the box and cannot press Enter in it, and a palette that could only be answered with
    /// the keyboard could not be tested at all.
    pub fn run_a_palette_row(&mut self, command: command_palette::Command, ctx: &egui::Context) {
        if !command.enabled {
            self.message = Some(format!(
                "{} cannot be used just now. It is on the {} menu.",
                command.label, command.menu
            ));
            return;
        }
        // Through `run_action`, which is the one place an action turns into a change, so a command
        // chosen in the palette and the same command chosen on the menu are the same thing.
        self.run_action(command.action, ctx);
    }

    // ------------------------------------------------------------------------ the two settings

    /// What the `Tab` key types where nothing is selected: a tab, or that many spaces.
    pub(crate) fn indent_text(&self) -> String {
        self.settings.indent.text()
    }

    /// Take the trailing whitespace off before a file is written, if the setting says to.
    ///
    /// **Called from the one place a tab is written**, so a save from the menu, from `Ctrl+S`, from
    /// `tab save` and from closing a modified tab all do the same thing. It is an ordinary
    /// `Command`, so it is one undo step and a person who did not mean it can put it back.
    pub(crate) fn trim_before_writing(&mut self, index: usize) {
        if !self.settings.trim_on_save {
            return;
        }
        let Some(file) = self.files.get(index) else {
            return;
        };
        if file.is_picture() || file.is_browser() || file.is_a_plugin() {
            return;
        }
        if !file_kind::trimming_applies(file.path()) {
            return;
        }
        self.files.at_mut(index).document.apply(Command::TrimTrailingWhitespace);
    }

    /// Take `Alt+Up` and `Alt+Down` out of the frame before any pane reads it.
    ///
    /// Only while the **editing area** has the keyboard, and never while a text box or a modal has
    /// it, which is `route_the_explorer_keys`' guard and is what stops the chord moving lines in a
    /// file nobody is typing into. The key is removed from the frame's events rather than merely
    /// read, because nothing in Unluminous consumes a key press by acting on it: left in, the same
    /// `ArrowUp` would reach `editor_view::handle_input` and move the caret off the line it had just
    /// moved.
    pub(crate) fn route_the_line_move_keys(&mut self, ui: &egui::Ui) -> Option<Action> {
        if self.focus != crate::app::Focus::Editor
            || crate::app::text_box_has_the_keyboard(ui.ctx())
            || crate::app::a_modal_has_the_keyboard(ui.ctx())
        {
            return None;
        }
        ui.ctx().input_mut(|input| {
            let mut found = None;
            input.events.retain(|event| {
                let egui::Event::Key { key, pressed, modifiers, .. } = event else {
                    return true;
                };
                match Self::a_line_move_chord(*key, modifiers) {
                    // The release is dropped with the press, or the pane would read a key up for a
                    // key it never saw go down.
                    Some(action) => {
                        if *pressed {
                            found = Some(action);
                        }
                        false
                    }
                    None => true,
                }
            });
            found
        })
    }

    /// The action `Alt+Up` and `Alt+Down` stand for, or nothing when that is not what was pressed.
    ///
    /// **The one chord pair in WP4 read from the keyboard**, because it is the one pair with no menu
    /// entry — `CLAUDE.md`'s rule is not to read the keyboard for something a menu already claims,
    /// and its converse is that something no menu claims has to be read somewhere. It is read in
    /// `app::frame::route_the_keys_before_the_panes`, with the completion popup's keys and the
    /// explorer's, for the reason written there: a key taken before the panes are drawn cannot also
    /// reach `editor_view::handle_input`, where a bare `ArrowUp` moves the caret.
    pub(crate) fn a_line_move_chord(key: egui::Key, modifiers: &egui::Modifiers) -> Option<Action> {
        if !modifiers.alt || modifiers.shift || modifiers.command || modifiers.ctrl {
            return None;
        }
        match key {
            egui::Key::ArrowUp => Some(Action::MoveLines { down: false }),
            egui::Key::ArrowDown => Some(Action::MoveLines { down: true }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_and_an_arrow_is_a_line_move_and_nothing_else_is() {
        let alt = egui::Modifiers { alt: true, ..egui::Modifiers::NONE };
        assert_eq!(
            UnluminousApp::a_line_move_chord(egui::Key::ArrowUp, &alt),
            Some(Action::MoveLines { down: false })
        );
        assert_eq!(
            UnluminousApp::a_line_move_chord(egui::Key::ArrowDown, &alt),
            Some(Action::MoveLines { down: true })
        );
        // A bare arrow moves the caret and must go on doing so.
        assert_eq!(
            UnluminousApp::a_line_move_chord(egui::Key::ArrowUp, &egui::Modifiers::NONE),
            None
        );
        // And `Ctrl+Alt+Left`/`Right` are Navigate Back and Forward, so a chord carrying a second
        // modifier is not this one — the guard `editor_view` already keeps about `navigating`.
        let both = egui::Modifiers { alt: true, command: true, ..egui::Modifiers::NONE };
        assert_eq!(UnluminousApp::a_line_move_chord(egui::Key::ArrowUp, &both), None);
    }
}
