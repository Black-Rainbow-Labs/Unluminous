//! What a menu entry, a key chord, a button or a right click turns into.
//!
//! `UnluminousApp::run_action` is **the one place an action turns into a change**, which is what makes
//! a menu entry and `unluminous-cli action run` the same thing. It is a helper a menu, and its own
//! match is exhaustive so that an action added tomorrow fails to compile here rather than quietly
//! doing nothing.
//!
//! `menu_state` is the other half: what the menus need to know to draw themselves.

use unluminous_core::Command;

use crate::app::debug::DebugState;
use crate::components::about_dialog::About;
use crate::components::find_in_files::FindInFiles;
use crate::components::go_to_file::GoToFile;
use crate::components::prompt_dialog::{Prompt, Purpose};
use crate::components::run_panel::{self};
use crate::services::file_kind;
use crate::services::launcher;
use crate::settings::{self};

use crate::app::actions::{Action, MenuState};
use crate::app::{actions, dock};
use crate::app::{Focus, Maximise, UnluminousApp};

impl UnluminousApp {
    /// What the menus need to know about the window.
    pub fn menu_state(&self) -> MenuState {
        MenuState {
            space_visible: self.was_showing(dock::Panel::Space, self.space.visible),
            space_node: self.space.chosen().and_then(|node| {
                self.space.space.current().node(node).map(|found| {
                    let session = match &found.state {
                        crate::services::space::State::Terminal(terminal) => {
                            !terminal.session.is_empty()
                        }
                        _ => false,
                    };
                    (found.kind(), session)
                })
            }),
            // **What the chosen node was left running**, which is a different question from whether it has a
            // conversation to resume — see `MenuState::space_node_running`. `task-1907`.
            space_node_running: self
                .space
                .chosen()
                .and_then(|node| self.space.space.current().node(node))
                .and_then(|found| match &found.state {
                    crate::services::space::State::Terminal(terminal) => {
                        Some(terminal.running.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            space_pipe: self.space.in_hand.wire.is_some_and(|edge| {
                self.space.space.current().edges.iter().any(|other| {
                    other.id == edge && other.pipe == crate::services::space::Pipe::Lines
                })
            }),
            space_views: self.space.space.views().len(),

            plugin_menus: self
                .plugin_ui
                .surfaces()
                .menus
                .iter()
                .map(|surface| actions::PluginMenu {
                    plugin: surface.plugin.clone(),
                    name: surface.what.name.clone(),
                    items: surface.what.items.clone(),
                })
                .collect(),
            can_undo: self.document().can_undo(),
            can_redo: self.document().can_redo(),
            has_selection: !self.document().selection().is_empty(),
            // The same three questions the key chords and the command line ask, of the same
            // functions, so the menu cannot come to a different answer about the tab that is
            // showing — which is what `file_kind`'s family exists to prevent. `task-1922` WP4.
            line_comment_applies: self.line_comment_marker().is_some(),
            block_comment_applies: self.block_comment_markers().is_some(),
            line_edits_apply: self.line_edits_apply_here(),
            trimming_applies: self.line_edits_apply_here()
                && file_kind::trimming_applies(self.document().path()),
            can_reopen_tab: !self.closed_tabs.is_empty(),
            finding: self.find.is_some(),
            recent: self.recent.clone(),
            view_mode: self.view_mode(),
            // The same question the text tools ask, so the `View` menu and the title bar cannot come to
            // different answers about the tab that is showing — which is what `file_kind`'s two
            // functions exist to prevent.
            can_preview: self.preview_applies_here(),
            preview_kind: file_kind::preview_kind(self.document().path()),
            explorer_visible: self.explorer_visible,
            editor_visible: self.editor_visible,
            maximised: self.maximised != Maximise::No,
            dock: self.panes.dock,
            line_numbers: self.settings.line_numbers,
            terminal_visible: self.terminal.visible,
            terminal_tabs: self.terminal.tabs.count(),
            open_files: self.files.len(),
            panes: self.files.pane_count(),
            pane: self.files.focused_pane(),
            tab_is_on_a_node: self.files.home_of(self.files.active_index()).node().is_some(),
            tabs_in_pane: self.files.tabs_in(self.files.focused_pane()).len(),
            in_repository: self.repository_controls_apply(),
            has_file: self.document().path().is_some(),
            annotated: self.files.active().blame.is_some(),
            unfinished: self.git.as_ref().and_then(|git| git.snapshot.in_progress),
            highlights: self.document().highlights().len(),
            on_a_highlight: self.marks_under_the_caret(),
            folding_applies: file_kind::folding_applies(self.document().path()),
            foldable: self.fold_counts().0,
            folded: self.fold_counts().1,
            definitions_apply: self.definitions_apply_here(),
            symbols_apply: self.symbols_apply_here(),
            completion_applies: self.completion_applies_here(),
            can_go_back: !self.back.is_empty(),
            can_go_forward: !self.forward.is_empty(),
            run_selected: self.run_selected.clone(),
            run_names: self.run_rows().into_iter().map(|row| row.name).collect(),
            run_active: self
                .run_selected
                .as_deref()
                .and_then(|name| self.run.index_of(name))
                .and_then(|at| self.run.at(at))
                .is_some_and(run_panel::Run::is_running),
            run_file_applies: self.run_file_template().is_some(),
            run_tile_visible: self.run.visible,
            debug_applies: self.debug_applies_here(),
            debug_active: self.debug.is_some(),
            debug_paused: self.debug.as_ref().is_some_and(DebugState::is_paused),
            debug_tile_visible: self.debug_panel.visible,
            on_a_breakpoint: self.breakpoint_in_question().is_some(),
            breakpoint_enabled: self
                .breakpoint_in_question()
                .is_none_or(|breakpoint| breakpoint.enabled),
        }
    }

    /// Do what a menu, a keyboard shortcut or a test asked for.
    ///
    /// This is the only place an action turns into a change, so the two menu bars and the keyboard cannot
    /// disagree about what `Save` means.
    /// Carry out one action. **The one place an action turns into a change**, which is what
    /// makes a menu entry, a key chord and `unluminous-cli action run` the same thing.
    ///
    /// A helper a menu, because five hundred lines of one match is not a thing anybody reads.
    /// The match here is **exhaustive on purpose**: a variant added tomorrow fails to compile in
    /// this list rather than quietly doing nothing, which is what a chain of helpers each
    /// answering "not mine" would have cost. Each helper is reached only from here, so the last
    /// arm of its own match cannot happen and says so.
    pub fn run_action(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::PluginPane { .. } | Action::PluginTab { .. } | Action::PluginCommand { .. } => {
                self.a_plugin_entry(action)
            }
            Action::NewWindow
            | Action::OpenFolder
            | Action::CreateProject
            | Action::OpenFile
            | Action::OpenWebAddress
            | Action::OpenInBrowser(_)
            | Action::GoToFile
            | Action::OpenRecent(_)
            | Action::ForgetRecent
            | Action::Save
            | Action::SaveAs
            | Action::CloseWindow
            | Action::ReopenClosedTab
            | Action::Quit => self.a_file_entry(action, ctx),
            Action::ToggleLineComment
            | Action::ToggleBlockComment
            | Action::DuplicateLines
            | Action::MoveLines { .. }
            | Action::JoinLines
            | Action::SortLines
            | Action::TrimTrailingWhitespace
            | Action::GoToLine
            | Action::GoToMatchingBracket => self.a_line_entry(action),
            Action::Find
            | Action::Replace
            | Action::FindNext
            | Action::FindPrevious
            | Action::CommandPalette
            | Action::FindInFiles => self.a_find_entry(action),
            Action::Settings
            | Action::Undo
            | Action::Redo
            | Action::Cut
            | Action::Copy
            | Action::Paste
            | Action::SelectAll
            | Action::GoToDefinition
            | Action::FindReferences
            | Action::RenameSymbol
            | Action::CompleteWord
            | Action::NavigateBack
            | Action::NavigateForward
            | Action::Highlight(_)
            | Action::ClearHighlight
            | Action::ClearHighlights
            | Action::Fold(_) => self.an_edit_entry(action, ctx),
            Action::SetViewMode(_)
            | Action::ToggleExplorer
            | Action::ToggleMaximisedPane
            | Action::ToggleEditor
            | Action::ToggleLineNumbers
            | Action::ChangeFontSize { .. }
            | Action::ResetFontSize
            | Action::ToggleTerminal
            | Action::CloseTab
            | Action::NextTab
            | Action::PreviousTab
            | Action::SplitRight
            | Action::MoveTabRight
            | Action::MoveTabLeft
            | Action::Unsplit
            | Action::UnsplitAll
            | Action::NextPane
            | Action::PreviousPane
            | Action::SelectOpenFile
            | Action::NewTerminalTab
            | Action::RenameTerminalTab
            | Action::CloseTerminalTab
            | Action::ToggleRunTile
            | Action::ToggleDebugTile
            | Action::Dock { .. }
            | Action::ResetPanelLayout
            | Action::Space(_) => self.a_view_entry(action),
            Action::Run(_) | Action::Debug(_) => self.a_run_entry(action),
            Action::NewFile(_)
            | Action::NewFolder(_)
            | Action::CutPath(_)
            | Action::CopyPath(_)
            | Action::CopyPathReference(_)
            | Action::PasteInto(_)
            | Action::RenamePath(_)
            | Action::DeletePath(_)
            | Action::RevealPath(_)
            | Action::ReloadPath(_) => self.an_explorer_entry(action, ctx),
            Action::About | Action::CheckForUpdates => self.an_unluminous_entry(action),
            // The Git menu is `run_git`, which was already the one place a git action turns into
            // a command.
            Action::Git(what) => self.run_git(what),
        }
    }

    /// The three entries a plugin contributes: its pane, its tab and its own menu commands.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn a_plugin_entry(&mut self, action: Action) {
        match action {
            // A plugin's three actions, all of which go down `PluginUi::run` or its two siblings, which
            // is the same path `unluminous-cli plugin` takes. A button in the rail, an entry in the plugin's
            // menu and an agent asking therefore reach one function rather than three that agree today.
            Action::PluginPane { ref pane } => {
                let showing = match self.plugin_ui.slot_of(pane) {
                    Some(slot) => !self.plugin_ui.is_visible(slot),
                    None => true,
                };
                self.show_the_plugin_pane(pane, showing);
            }
            // Toggles, because this is what the rail button and the menu entry both run. The command line's
            // `--open` and `--close` reach `open_the_plugin_tab` and `close_tab` directly, so a named
            // switch still does exactly what it says.
            Action::PluginTab { ref tab } => self.toggle_the_plugin_tab(&tab.clone()),
            Action::PluginCommand { ref plugin, ref command } => {
                let (plugin, command) = (plugin.clone(), command.clone());
                // The answer is already in the status bar; the menu has nothing else to do with it.
                let _ = self.run_plugin_command(&plugin, &command, &[]);
            }
            other => unreachable!("{other:?} is not handled by a_plugin_entry"),
        }
    }

    /// The File menu.
    ///
    /// Make a project folder, start a git repository in it when asked, and open it.
    ///
    /// **The one place a project is made**, so `File -> Create Project...` and
    /// `unluminous-cli project new` are the same thing — which is `run_cli`'s own rule said about a
    /// dialog, and is what keeps an agent's project and a person's project from being two different
    /// things.
    ///
    /// **A window of its own**, which is what `Open Folder` and `Recent Projects` already do: a project
    /// is a window, so making a second one keeps the first. Only if a second process cannot be started
    /// does the folder take this window, which is `Open Folder`'s own fallback and is better than the
    /// entry appearing to do nothing.
    ///
    /// `git init` is the machine's own git through `unluminous_git`, so a machine without one says what
    /// git said rather than what Unluminous guessed — and it is **not** fatal: the folder is made either
    /// way, and a project that exists without a repository is better than a refusal that leaves a folder
    /// half made.
    pub(crate) fn make_the_project(
        &mut self,
        folder: &std::path::Path,
        git: bool,
    ) -> Result<(), String> {
        std::fs::create_dir_all(folder)
            .map_err(|problem| format!("{} could not be made: {problem}", folder.display()))?;
        if git {
            let done = unluminous_git::command::run(folder, &["init", "--initial-branch=main"]);
            if !done.ok {
                // git's own words, which is `unluminous-git`'s rule: nothing here invents a message.
                let said = match done.stderr.trim().is_empty() {
                    true => done.stdout.trim().to_owned(),
                    false => done.stderr.trim().to_owned(),
                };
                self.message = Some(format!("The project was made. git init: {said}"));
            }
        }
        let folder = unluminous_terminal::paths::plain(folder);
        if launcher::open_window(&folder).is_none() {
            self.open_folder(&folder);
        }
        self.new_project = None;
        Ok(())
    }

    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn a_file_entry(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::NewWindow => {
                launcher::open_window(self.tree.root());
            }
            Action::CreateProject => {
                self.new_project = Some(crate::components::new_project_dialog::NewProject::beside(
                    self.tree.root(),
                ));
            }
            Action::OpenFolder => {
                let start = self.tree.root().to_path_buf();
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("Open Folder")
                    .set_directory(&start)
                    .pick_folder()
                {
                    // A window of its own, which is what `Recent Projects` already did and what
                    // `task-1658` asks for: a project is a window, so opening a second one keeps the
                    // first. Only if a second process cannot be started does the folder take this
                    // window, which is better than the entry doing nothing at all.
                    if launcher::open_window(&folder).is_none() {
                        self.open_folder(&folder);
                    }
                }
            }
            Action::OpenFile => {
                let start = self.tree.root().to_path_buf();
                // Every file is offered, not only Markdown and plain text, because Unluminous opens any file
                // holding text. One that turns out not to be text says so rather than opening as nonsense.
                if let Some(file) =
                    rfd::FileDialog::new().set_title("Open File").set_directory(&start).pick_file()
                {
                    if let Some(parent) = file.parent() {
                        if !file.starts_with(self.tree.root()) {
                            self.open_folder(parent);
                        }
                    }
                    let _ = self.open_path(&file);
                }
            }
            Action::OpenWebAddress => {
                self.prompt = Some(Prompt::new(
                    "Open Web Address",
                    "An HTTP address or an HTML file in this project.",
                    "https://",
                    "Open",
                    Purpose::OpenWebAddress,
                ));
            }
            Action::OpenInBrowser(path) => {
                if let Err(problem) = self.open_browser(&path.to_string_lossy()) {
                    self.message = Some(problem);
                }
            }
            Action::GoToFile => {
                // The folder is read again first, so a file made since the window opened is in the
                // list. It is one walk of the project, on a key press rather than on every frame,
                // and a finder that cannot find a file you made a minute ago is not a finder.
                self.tree.reload();
                self.go_to_file = Some(GoToFile::default());
            }
            Action::OpenRecent(folder) => {
                // A window of its own, as the reference editor does it, so the project that is open stays open.
                if launcher::open_window(&folder).is_none() {
                    self.open_folder(&folder);
                }
            }
            Action::ForgetRecent => {
                self.recent.clear();
                if let Some(store) = &self.store {
                    let _ = std::fs::remove_file(store.recent_path());
                }
            }
            Action::Save | Action::SaveAs if self.files.active().is_browser() => {
                self.message = Some("A rendered page has no editable source to save.".to_owned());
            }
            Action::Save => self.save(),
            Action::SaveAs if self.files.active().is_picture() => {
                self.message =
                    Some("A picture cannot be edited, so there is nothing to save.".to_owned());
            }
            Action::SaveAs => {
                let start = self.tree.root().to_path_buf();
                if let Some(target) =
                    rfd::FileDialog::new().set_title("Save As").set_directory(&start).save_file()
                {
                    if self.document_mut().save_as(&target).is_ok() {
                        self.tree.reload();
                    }
                }
            }
            Action::ReopenClosedTab => match self.reopen_the_last_closed_tab() {
                Ok(path) => {
                    self.message = Some(format!("Reopened {}", path.display()));
                    self.focus = Focus::Editor;
                }
                Err(problem) => self.message = Some(problem),
            },
            Action::CloseWindow | Action::Quit => {
                // As the cross in the title bar does: every modified tab is written first and the
                // window stays when one of them could not be (`task-1984` A2).
                if self.may_the_window_close() {
                    self.closing = true;
                    self.write_settings();
                    self.remember_the_project(None);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            other => unreachable!("{other:?} is not handled by a_file_entry"),
        }
    }

    /// The Find menu.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn a_find_entry(&mut self, action: Action) {
        match action {
            Action::Find | Action::Replace => {
                let replacing = action == Action::Replace;
                // A bar already open is **reused** rather than replaced, so pressing the key twice
                // does not throw away what is typed in it; what it does do is put the keyboard back
                // in the box, which is what a person pressing it again is asking for.
                match self.find.as_mut() {
                    Some(find) => {
                        find.replacing |= replacing;
                        find.field = crate::services::find::Field::Find;
                    }
                    None => {
                        let selected = self.document().selected_text();
                        let mut find = crate::services::find::Find::opened_with(
                            Some(selected.as_str()),
                            replacing,
                        );
                        // The matches are worked out now rather than next frame, so that the bar
                        // opens with its tally already on it when it was seeded from a selection.
                        let text = self.document().text().to_string();
                        find.refresh(&text, self.document().text_revision());
                        find.start_from(self.document().selection().start());
                        let current = find.current();
                        self.find = Some(find);
                        // The match the bar opens on is **selected**, so the picture says which of
                        // them is current from the first frame rather than after the first Enter.
                        if let Some(range) = current {
                            self.select_the_match(range);
                        }
                    }
                }
                self.focus = Focus::Editor;
            }
            Action::FindNext => self.step_the_find(true),
            Action::FindPrevious => self.step_the_find(false),
            Action::FindInFiles => {
                // The folder is read again first, for the reason `Go to File` reads it: a file made
                // since the window opened is part of this project and has to be searched.
                self.tree.reload();
                self.find_in_files = Some(FindInFiles::open(self.thread_waker()));
            }
            Action::CommandPalette => self.open_the_command_palette(),
            other => unreachable!("{other:?} is not handled by a_find_entry"),
        }
    }

    /// The line editing entries, the two comment toggles, `Go to Line` and the bracket.
    ///
    /// `task-1922` WP4. Reached only from [`Self::run_action`], which is what decides that an action
    /// is one of these, so the last arm cannot happen. Every one of them is a `Command` in
    /// `unluminous-core` applied through `Document::apply`, so each is one undo step; what is here is
    /// the marker the language names and what the status bar says when there is nothing to do.
    fn a_line_entry(&mut self, action: Action) {
        match action {
            Action::ToggleLineComment => {
                if let Err(problem) = self.toggle_line_comment() {
                    self.message = Some(problem);
                }
            }
            Action::ToggleBlockComment => {
                if let Err(problem) = self.toggle_block_comment() {
                    self.message = Some(problem);
                }
            }
            Action::DuplicateLines => {
                self.duplicate_lines();
            }
            Action::MoveLines { down } => {
                if !self.move_lines(if down { 1 } else { -1 }) {
                    self.message = Some(
                        "Those lines are already at the end of the file they were moving towards."
                            .to_owned(),
                    );
                }
            }
            Action::JoinLines => {
                if !self.join_lines() {
                    self.message =
                        Some("There is no line below this one to join it to.".to_owned());
                }
            }
            Action::SortLines => {
                if let Err(problem) = self.sort_lines() {
                    self.message = Some(problem);
                }
            }
            Action::TrimTrailingWhitespace => match self.trim_trailing_whitespace() {
                Ok(true) => self.message = Some("Trimmed the trailing whitespace".to_owned()),
                Ok(false) => {
                    self.message = Some("No line ends in whitespace.".to_owned());
                }
                Err(problem) => self.message = Some(problem),
            },
            Action::GoToLine => self.ask_which_line(),
            Action::GoToMatchingBracket => {
                if self.go_to_matching_bracket().is_none() {
                    self.message = Some(
                        "The caret is not beside a bracket that has a partner in this file."
                            .to_owned(),
                    );
                }
            }
            other => unreachable!("{other:?} is not handled by a_line_entry"),
        }
    }

    /// The Edit menu, which holds the symbol, completion, navigation, highlight and folding
    /// entries as well as the clipboard.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn an_edit_entry(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::GoToDefinition => {
                let offset = self.caret_offset();
                self.go_to_definition(offset);
            }
            Action::FindReferences => {
                let offset = self.caret_offset();
                self.find_references(offset);
            }
            Action::RenameSymbol => {
                let offset = self.caret_offset();
                self.rename_symbol(offset);
            }
            // Only while the editing area has the keyboard. The menu's keyboard watcher does not
            // care what has the focus and does not consume the press, and `Ctrl+Space` is a key a
            // terminal sends — as a NUL byte — so without this guard one press would both open a
            // list over a file nobody was typing into and reach the program in the terminal.
            Action::CompleteWord if self.focus == Focus::Editor => self.complete_word(),
            Action::CompleteWord => {}
            Action::NavigateBack => self.navigate(true),
            Action::NavigateForward => self.navigate(false),
            Action::Settings => self.settings_window.open(),
            Action::Undo => {
                self.document_mut().apply(Command::Undo);
            }
            Action::Redo => {
                self.document_mut().apply(Command::Redo);
            }
            Action::Cut => {
                if self.focus == Focus::Terminal {
                    if let Some(text) = self.terminal.tabs.active().and_then(|s| s.selected_text())
                    {
                        ctx.copy_text(text);
                    }
                } else if !self.document().selection().is_empty() {
                    ctx.copy_text(self.document().selected_text());
                    self.document_mut().apply(Command::DeleteBackward);
                }
            }
            Action::Copy => {
                if self.focus == Focus::Terminal {
                    if let Some(text) = self.terminal.tabs.active().and_then(|s| s.selected_text())
                    {
                        ctx.copy_text(text);
                    }
                } else if self.preview_holds_the_selection() {
                    if let Some(text) = self.preview_selected_text() {
                        ctx.copy_text(text);
                    }
                } else if !self.document().selection().is_empty() {
                    ctx.copy_text(self.document().selected_text());
                }
            }
            Action::Paste => {
                // Reading the clipboard needs the operating system's own clipboard, which egui only hands
                // over as a paste event. A menu entry has no event behind it, so the clipboard is read
                // here. Typing the shortcut still goes through the event, which is why the clipboard
                // entries are marked as not coming from the keyboard.
                match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                    Ok(text) if !text.is_empty() => {
                        if self.focus == Focus::Terminal {
                            let mode =
                                self.terminal.tabs.active().map(|s| s.mode()).unwrap_or_default();
                            let bytes = unluminous_terminal::keys::paste(&text, mode);
                            if let Some(session) = self.terminal.tabs.active() {
                                session.send(bytes);
                            }
                        } else {
                            let text = text.replace("\r\n", "\n").replace('\r', "\n");
                            self.document_mut().apply(Command::Insert(text));
                        }
                    }
                    Ok(_) => {}
                    Err(problem) => eprintln!("Unluminous could not read the clipboard: {problem}"),
                }
            }
            Action::SelectAll => {
                if self.reading_preview && self.view_mode().shows_preview() {
                    self.select_the_whole_preview();
                } else {
                    self.document_mut().apply(Command::SelectAll);
                }
            }
            Action::Highlight(colour) => {
                self.highlight_selection(colour.rgba());
            }
            Action::ClearHighlight => {
                if !self.clear_highlight_here() {
                    self.message = Some("There is no highlight at the caret.".to_owned());
                }
            }
            Action::Fold(what) => {
                use crate::app::actions::FoldAction;
                match what {
                    FoldAction::Toggle => self.toggle_fold_at_caret(),
                    FoldAction::All => self.collapse_all_folds(),
                    FoldAction::None_ => self.expand_all_folds(),
                    FoldAction::Others => self.collapse_all_but_marked(),
                    FoldAction::CollapseRecursively => self.collapse_recursively_at_caret(),
                    FoldAction::ExpandRecursively => self.expand_recursively_at_caret(),
                };
            }
            Action::ClearHighlights => {
                let cleared = self.document().highlights().len();
                if self.clear_highlights_here() {
                    self.message = Some(format!(
                        "Cleared {cleared} highlight{}",
                        if cleared == 1 { "" } else { "s" }
                    ));
                }
            }
            other => unreachable!("{other:?} is not handled by an_edit_entry"),
        }
    }

    /// The View menu: what is showing, how big it is, and which tab or pane is in front.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn a_view_entry(&mut self, action: Action) {
        match action {
            Action::SetViewMode(mode) => self.set_view_mode(mode),
            // Both of the rules this used to state -- leave a maximise, and never leave the window
            // with nothing in it -- are in `show_a_panel` now, where `unluminous-cli explorer show`
            // and the two drop targets reach them as well (`task-1984` A9).
            Action::ToggleExplorer => {
                self.show_a_panel(dock::Panel::Explorer, !self.explorer_visible)
            }
            // The pane holding the keyboard, or the editing area when it is the one holding it. Which is
            // the same question the zoom keys ask, and it is asked in one place. `task-1771`.
            Action::ToggleMaximisedPane => {
                let pane = self.the_pane_the_keys_hold();
                self.toggle_maximised(pane);
            }
            Action::ToggleEditor => {
                self.leave_the_maximised_pane();
                let hiding = self.editor_visible;
                self.editor_visible = !self.editor_visible;
                // Hiding it with nothing else showing shows the explorer, rather than the button doing nothing.
                // One rule stated in both directions: there is always something to look at.
                if hiding && !self.anything_is_showing_in_the_panes() {
                    self.explorer_visible = true;
                }
            }
            Action::ToggleLineNumbers => {
                self.settings.line_numbers = !self.settings.line_numbers;
                self.unsaved_settings = true;
            }
            // **The keys zoom whatever has them.** `task-1771` asks for every pane to be zoomable, and a
            // shortcut that meant "make the editor's font bigger" while somebody was working in the file
            // tree would be a shortcut that does nothing where they are looking. Which pane that is comes
            // from `Focus`, which is the one value in the window that says who holds the keyboard.
            Action::ChangeFontSize { larger } => match self.the_pane_the_keys_zoom() {
                Some(panel) => self.step_the_zoom_of(panel, if larger { 1 } else { -1 }, None),
                // On a tab showing a picture the same keys zoom the picture. `task-1658` asks for
                // control and plus to zoom an image, and one shortcut meaning "make what I am looking
                // at bigger" is what a person expects of it.
                None => {
                    let area = self.editor_area.size();
                    match self.files.active_mut().picture.as_mut() {
                        Some(picture) => picture.step_zoom(larger, area),
                        None => {
                            // About the caret, which is what a person zooming with the keyboard is
                            // looking at. `task-1672`.
                            self.anchor_the_view_at_the_caret();
                            self.set_font_size(settings::step_font_size(
                                self.settings.font_size,
                                larger,
                            ))
                        }
                    }
                }
            },
            Action::ResetFontSize => match self.the_pane_the_keys_zoom() {
                Some(panel) => self.reset_the_zoom_of(panel),
                None => match self.files.active_mut().picture.as_mut() {
                    Some(picture) => picture.fit(),
                    None => {
                        self.anchor_the_view_at_the_caret();
                        self.set_font_size(settings::DEFAULT_FONT_SIZE)
                    }
                },
            },
            Action::ToggleTerminal => {
                let showing = !self.terminal.visible;
                self.show_the_terminal_tile(showing);
            }
            Action::CloseTab => {
                let index = self.files.active_index();
                self.close_tab(index);
            }
            Action::NextTab => {
                self.files.next();
                self.forget_layout();
            }
            Action::PreviousTab => {
                self.files.previous();
                self.forget_layout();
            }
            // The panes. Each is one call on `OpenFiles`, which is where the rules about panes live,
            // so a split made from a menu and a split made from the command line are the same split.
            Action::SplitRight => {
                self.files.split_right();
                self.focus = Focus::Editor;
            }
            Action::MoveTabRight => {
                if !self.files.move_tab(true) {
                    self.message = Some("There is no pane to the right of this one.".to_owned());
                }
            }
            Action::MoveTabLeft => {
                if !self.files.move_tab(false) {
                    self.message = Some("There is no pane to the left of this one.".to_owned());
                }
            }
            Action::Unsplit => {
                if !self.files.unsplit() {
                    self.message = Some("The editing area is not split.".to_owned());
                }
            }
            Action::UnsplitAll => {
                if !self.files.unsplit_all() {
                    self.message = Some("The editing area is not split.".to_owned());
                }
            }
            Action::NextPane => {
                self.files.next_pane();
                self.focus = Focus::Editor;
            }
            Action::PreviousPane => {
                self.files.previous_pane();
                self.focus = Focus::Editor;
            }
            Action::SelectOpenFile => self.select_the_open_file(),
            Action::NewTerminalTab => {
                self.show_the_terminal_tile(true);
                self.new_terminal_tab();
            }
            Action::RenameTerminalTab => {
                let index = self.terminal.tabs.active_index();
                match self.terminal.tabs.names().get(index) {
                    Some(name) => {
                        self.prompt = Some(Prompt::new(
                            "Rename Terminal Tab",
                            "What this tab is called in the strip. The name stays put when the program in it sets a title of its own.",
                            name,
                            "Rename",
                            Purpose::RenameTerminalTab(index),
                        ));
                    }
                    None => self.message = Some("There is no terminal tab to rename.".to_owned()),
                }
            }
            Action::ToggleDebugTile => {
                let showing = self.debug_panel.visible;
                self.show_the_debug_tile(!showing);
            }
            // `None` for the position: a menu row can only say "the left", and the end of that side
            // is where a person who did not aim means. The drag is what says before or after, and
            // `unluminous-cli panel dock --position` is what says it in a script.
            Action::Dock { panel, side } => self.dock_the_panel(panel, side, None),
            Action::ResetPanelLayout => self.reset_the_panel_layout(),
            Action::Space(what) => self.run_a_space_action(what),
            Action::ToggleRunTile => {
                let showing = !self.run.visible;
                self.show_the_run_tile(showing);
            }
            Action::CloseTerminalTab => {
                let index = self.terminal.tabs.active_index();
                self.terminal.tabs.close(index);
                if self.terminal.tabs.is_empty() {
                    self.terminal.visible = false;
                    self.focus = Focus::Editor;
                }
            }
            other => unreachable!("{other:?} is not handled by a_view_entry"),
        }
    }

    /// The Run menu, and the Debug entries drawn inside it.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn a_run_entry(&mut self, action: Action) {
        match action {
            Action::Debug(what) => self.debug_a_configuration(what),
            // The reason is already in the status bar, which is the whole of what a menu can say
            // about a run that would not start. It is the command line that needed it as a value,
            // and `cli_run_do` is what takes it.
            Action::Run(what) => {
                let _ = self.run_a_configuration(what);
            }
            other => unreachable!("{other:?} is not handled by a_run_entry"),
        }
    }

    /// The explorer's own right click menu: making, copying, moving and throwing away a file.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn an_explorer_entry(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::NewFile(folder) => {
                self.prompt = Some(Prompt::new(
                    "New File",
                    &format!(
                        "A new, empty file in {}. Any extension: example.txt, test.json, main.rs.",
                        crate::services::paths::the_useful_end_of(&folder.display().to_string())
                    ),
                    "example.txt",
                    "Create",
                    Purpose::NewFile(folder),
                ));
            }
            Action::NewFolder(folder) => {
                self.prompt = Some(Prompt::new(
                    "New Folder",
                    &format!("A new folder inside {}.", folder.display()),
                    "folder",
                    "Create",
                    Purpose::NewFolder(folder),
                ));
            }
            Action::CutPath(path) => self.clipboard.cut(path),
            Action::CopyPath(path) => self.clipboard.copy(path),
            Action::CopyPathReference(path) => ctx.copy_text(path.display().to_string()),
            Action::PasteInto(folder) => match self.clipboard.paste_into(&folder) {
                Ok(target) => {
                    self.tree.reload();
                    self.message = Some(format!("Pasted {}", target.display()));
                }
                Err(problem) => {
                    self.message = Some(format!("Unluminous could not paste: {problem}"))
                }
            },
            Action::RenamePath(path) => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.prompt = Some(Prompt::new(
                    "Rename",
                    &format!("Rename {}.", path.display()),
                    &name,
                    "Rename",
                    Purpose::Rename(path),
                ));
            }
            Action::DeletePath(path) => self.ask_before_deleting(&path),
            Action::RevealPath(path) => {
                launcher::reveal(&path);
            }
            Action::ReloadPath(path) => {
                self.reload_from_disk(&path, false);
            }
            other => unreachable!("{other:?} is not handled by an_explorer_entry"),
        }
    }

    /// The Unluminous menu, less the two entries that belong to another menu as well.
    ///
    /// Reached only from [`Self::run_action`], which is what decides that an action is one of
    /// these, so the last arm cannot happen.
    fn an_unluminous_entry(&mut self, action: Action) {
        match action {
            Action::About => {
                // One modal at a time, which is what opening any of the others does.
                self.close_every_modal();
                self.about = Some(About::current());
            }
            Action::CheckForUpdates => self.check_for_updates(),
            other => unreachable!("{other:?} is not handled by an_unluminous_entry"),
        }
    }
}
