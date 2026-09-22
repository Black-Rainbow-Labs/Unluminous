//! The modals, and the two questions they ask on the window's behalf.
//!
//! Each is drawn from the context rather than into a pane, because `components::modal` places it,
//! drags it and resizes it and it has to sit over everything. Only one is open at a time, so where
//! they are drawn among each other decides nothing.

use std::path::Path;

use crate::app::debug::DebugState;
use crate::components::debug_dialogs::{self, EvaluateDialog};
use crate::components::prompt_dialog::{self, Prompt, Purpose};
use crate::services::recycle;

use crate::app::{Answer, Confirmation, UnluminousApp, REVEAL_FRAMES};

impl UnluminousApp {
    /// Ask the releases page whether a newer Unluminous exists.
    ///
    /// **A person pressing the menu entry, or `update.check` set to `start`, or an agent running
    /// `update check`. Nothing else.** See `services::update` for why that list is the whole of it.
    ///
    /// A check already running is left to finish rather than started again, so holding the menu
    /// entry down does not open twenty sockets.
    pub(crate) fn check_for_updates(&mut self) {
        if self.update.as_ref().is_some_and(|check| check.is_asking()) {
            return;
        }
        self.message = Some("Asking whether there is a newer Unluminous...".to_owned());
        self.update_asked_by_a_person = true;
        self.update = Some(crate::services::update::Check::start(self.thread_waker()));
    }

    /// Take in the answer, if one has arrived.
    ///
    /// Called once a frame, beside the git replies and the search results, which is where every
    /// other thread's answer is taken. True when something changed and the window has to draw.
    pub(crate) fn take_the_update_answer(&mut self) -> bool {
        let Some(check) = self.update.as_mut() else {
            return false;
        };
        let Some(answer) = check.poll() else {
            return false;
        };
        self.message = Some(answer.sentence());
        // The notice with `Install & Restart` and `Don't Ask Again` on it. `task-2063`.
        self.offer_what_the_check_found(&answer);
        self.update_answer = Some(answer);
        true
    }

    /// What the About box says about updates: the answer, or that it is still asking, or nothing.
    pub fn update_line(&self) -> Option<String> {
        if let Some(install) = &self.install {
            return Some(install.progress().sentence(&install.version));
        }
        if self.update.as_ref().is_some_and(|check| check.is_asking()) {
            return Some("Checking...".to_owned());
        }
        self.update_answer.as_ref().map(|answer| match answer {
            crate::services::update::Answer::Newer(release) => {
                format!("{} is available", release.version)
            }
            crate::services::update::Answer::Current(_) => "This is the newest release".to_owned(),
            crate::services::update::Answer::Failed(problem) => {
                format!("Could not check: {problem}")
            }
        })
    }

    /// Open `Evaluate Expression`, seeded with the selection when there is one.
    pub(crate) fn open_the_expression_box(&mut self) {
        let seed = match self.document().selection().is_empty() {
            true => String::new(),
            false => {
                let range = self.document().selection().range();
                self.document().text().byte_slice(range)
            }
        };
        self.close_every_modal();
        self.evaluate = Some(EvaluateDialog {
            expression: seed.trim().to_owned(),
            result: None,
            asking: false,
        });
    }

    /// The two debug modals, drawn after the panes so they sit over everything.
    pub(crate) fn show_the_debug_modals(&mut self, ctx: &egui::Context) {
        if let Some(mut dialog) = self.breakpoint_dialog.take() {
            let outcome = debug_dialogs::breakpoint(ctx, &mut dialog);
            match (outcome.confirmed, outcome.removed, outcome.cancelled) {
                (true, _, _) => {
                    let condition = dialog.condition();
                    let log = dialog.log();
                    let enabled = dialog.enabled;
                    let path = dialog.path.clone();
                    self.change_breakpoints(&path, |breakpoints| {
                        if let Some(breakpoint) = breakpoints.at_mut(dialog.offset) {
                            breakpoint.enabled = enabled;
                            breakpoint.condition = condition;
                            breakpoint.log_message = log;
                        }
                    });
                    self.send_the_breakpoints_of(&path);
                    self.message = Some(format!("Breakpoint on line {} saved", dialog.line));
                }
                (_, true, _) => {
                    let path = dialog.path.clone();
                    self.change_breakpoints(&path, |breakpoints| {
                        breakpoints.remove_at(dialog.offset);
                    });
                    self.send_the_breakpoints_of(&path);
                    self.message = Some(format!("Breakpoint removed from line {}", dialog.line));
                }
                // Cancelling takes back only a breakpoint this modal put there: somebody who right
                // clicked an empty line, chose `Add Conditional Breakpoint...` and then thought
                // better of it has not asked for a plain one. One that was already there is left
                // exactly as it was, which is what Cancel means everywhere else.
                (_, _, true) if dialog.created => {
                    let path = dialog.path.clone();
                    self.change_breakpoints(&path, |breakpoints| {
                        breakpoints.remove_at(dialog.offset);
                    });
                    self.send_the_breakpoints_of(&path);
                }
                _ => {}
            }
            if !outcome.confirmed && !outcome.removed && !outcome.cancelled {
                self.breakpoint_dialog = Some(dialog);
            }
        }
        if let Some(mut dialog) = self.evaluate.take() {
            let paused = self.debug.as_ref().is_some_and(DebugState::is_paused);
            let outcome = debug_dialogs::evaluate(ctx, &mut dialog, paused);
            if outcome.confirmed {
                let expression = dialog.expression.clone();
                dialog.asking = true;
                dialog.result = None;
                if let Some(debug) = self.debug.as_mut() {
                    debug.evaluate(&expression);
                }
            }
            // The answer, once the adapter has sent one. Read here rather than pushed, because a
            // modal is drawn every frame anyway and one place reading is one place to get right.
            if let Some((_, _, Some(answer))) =
                self.debug.as_ref().and_then(|debug| debug.evaluated.clone())
            {
                dialog.asking = false;
                dialog.result = Some(match answer {
                    Ok(value) => Ok(value.value),
                    Err(problem) => Err(problem),
                });
            }
            if !outcome.cancelled {
                self.evaluate = Some(dialog);
            }
        }
    }

    /// Draw the one confirmation, and do what it asks when it is answered.
    ///
    /// Drawn before every other modal, because it is asked *over* whatever asked it, and drawn on
    /// its own rather than inside `show_git_windows`, because `task-1681` gave it a second kind of
    /// answer and a window with no repository behind it can now ask a question.
    pub(crate) fn show_the_confirmation(&mut self, ctx: &egui::Context) {
        let Some(question) = self.confirmation.clone() else {
            return;
        };
        let outcome = prompt_dialog::confirm(
            ctx,
            &prompt_dialog::Confirmation {
                title: question.title.clone(),
                note: question.note.clone(),
                confirm: question.button.clone(),
                purpose: String::new(),
            },
        );
        if outcome.confirmed {
            self.confirmation = None;
            self.answer_the_question(question.answer);
        } else if outcome.cancelled {
            self.confirmation = None;
        }
    }

    /// Do what confirming a question does, and say what was done.
    ///
    /// One place rather than two, because the dialog is not the only thing that answers one:
    /// `unluminous-cli modal accept` presses the same button, and two arms that agreed today would be
    /// two arms that did not agree the day a fourth question was added. The sentence it returns is
    /// what the command line reports; the dialog throws it away, having already put anything worth
    /// saying in the status bar.
    pub fn answer_the_question(&mut self, answer: Answer) -> String {
        match answer {
            Answer::Git(request) => {
                let label = request.label();
                self.send_git(request);
                label
            }
            Answer::Delete(path) => {
                let name = path.display().to_string();
                self.delete_path(&path);
                format!("delete {name}")
            }
            Answer::RemoveRun(name) => {
                if let Some(at) = self.run.index_of(&name) {
                    self.run.close(at);
                }
                self.run_configurations.remove(&name);
                if self.run_selected.as_deref() == Some(name.as_str()) {
                    self.run_selected = None;
                }
                self.unsaved_run_configurations = true;
                self.message = Some(format!("Removed {name}"));
                format!("remove {name}")
            }
        }
    }

    /// Ask before throwing a file away.
    ///
    /// The question names what is about to go and where it is going, and for a folder it counts
    /// what is inside — the count is the fact that changes the answer. Where a deleted file goes is
    /// `services::recycle`'s to say, so the sentence is derived from it rather than written twice.
    pub fn ask_before_deleting(&mut self, path: &Path) {
        if !path.exists() {
            self.message = Some(format!("{} is not there.", path.display()));
            return;
        }
        if path == self.tree.root() {
            self.message =
                Some("The project folder itself cannot be deleted from here.".to_owned());
            return;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let what = if path.is_dir() {
            let inside = recycle::count_inside(path, 10_000);
            match inside {
                0 => format!("Delete {name}, which is empty."),
                1 => format!("Delete {name} and the 1 file in it."),
                _ => format!("Delete {name} and the {inside} files in it."),
            }
        } else {
            format!("Delete {name}.")
        };
        self.close_every_modal();
        // Nothing has happened yet, so whatever the status bar was saying about the last thing that
        // did is now misleading — and `unluminous-cli action run delete-path` answers with it, which
        // made asking the question report a deletion that had not happened.
        self.message = None;
        self.confirmation = Some(Confirmation {
            title: "Delete".to_owned(),
            note: format!("{what} {}", recycle::destination().reassurance()),
            button: "DELETE".to_owned(),
            answer: Answer::Delete(path.to_path_buf()),
        });
    }

    /// Confirm a prompt, which is what pressing its button does.
    ///
    /// Public so a test can drive it: a screenshot test can put a name in the field but cannot press
    /// the button, and a prompt that can only be answered with the mouse cannot be tested.
    pub fn run_prompt_for_test(&mut self, prompt: Prompt) {
        self.run_prompt(prompt);
    }

    /// Do what the text prompt was asking about, now that it has been confirmed.
    ///
    /// The prompt itself knows nothing about files or git; this is where a typed name turns into a
    /// change, which is the same rule every menu entry follows.
    pub(crate) fn run_prompt(&mut self, prompt: Prompt) {
        let name = prompt.value.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match prompt.purpose {
            Purpose::OpenWebAddress => {
                if let Err(problem) = self.open_browser(&name) {
                    self.message = Some(problem);
                }
            }
            Purpose::NewFile(folder) => {
                let target = crate::services::file_clipboard::free_name(&folder, &name);
                match std::fs::write(&target, "") {
                    Ok(()) => {
                        self.tree.reload();
                        self.tree.expand(&folder);
                        let _ = self.open_path_permanently(&target);
                    }
                    Err(problem) => {
                        self.message = Some(format!(
                            "Unluminous could not make {}: {problem}",
                            target.display()
                        ))
                    }
                }
            }
            Purpose::NewFolder(folder) => {
                // The same shape `NewFile` has, with `create_dir_all` in place of writing an empty
                // file. There is nothing to open in a tab, so what it does instead is open the new
                // folder out in the tree and put the explorer's cursor on it — which is where
                // somebody who has just made a folder is about to make a file.
                let target = crate::services::file_clipboard::free_name(&folder, &name);
                match std::fs::create_dir_all(&target) {
                    Ok(()) => {
                        self.tree.reload();
                        self.tree.expand(&target);
                        self.selected = Some(target.clone());
                        self.reveal_selection = REVEAL_FRAMES;
                        self.message = Some(format!("Made {}", target.display()));
                    }
                    Err(problem) => {
                        self.message = Some(format!(
                            "Unluminous could not make {}: {problem}",
                            target.display()
                        ))
                    }
                }
            }
            Purpose::Rename(path) => {
                let Some(folder) = path.parent() else {
                    return;
                };
                let target = folder.join(&name);
                if target == path {
                    return;
                }
                // A rename **is** a move to a new name, so it goes through the same function a drag
                // does and the code that names the file follows it. A rename that updated no
                // references while a drag did would be two answers to one question.
                if self.move_path(&path, &target, true) {
                    let said = self.message.take().unwrap_or_default();
                    let rest = said.split_once('\u{00B7}').map(|(_, rest)| rest).unwrap_or("");
                    self.message = Some(match rest.trim().is_empty() {
                        true => format!("Renamed to {name}"),
                        false => format!("Renamed to {name} \u{00B7} {}", rest.trim()),
                    });
                }
            }
            Purpose::NewBranch => {
                self.send_git(unluminous_git::worker::Request::CreateBranch(name))
            }
            Purpose::NewTag => self.send_git(unluminous_git::worker::Request::Tag(name)),
            Purpose::Stash => self.send_git(unluminous_git::worker::Request::Stash {
                message: name,
                include_untracked: true,
            }),
            Purpose::Clone => {
                let parent = self.tree.root().to_path_buf();
                self.send_git(unluminous_git::worker::Request::Clone { parent, url: name });
            }
            Purpose::CompareWithRevision(path) => {
                self.send_git(unluminous_git::worker::Request::Diff {
                    path,
                    staged: false,
                    revision: Some(name),
                });
            }
            Purpose::ResetTo(mode) => {
                let _ = mode;
            }
            Purpose::RenameTerminalTab(index) => {
                if self.terminal.tabs.rename(index, &name) {
                    self.message = Some(format!("Terminal tab {index} is called {name}"));
                }
            }
            Purpose::RenameSpaceNode(node) => {
                if self.space.space.title_node(node, name.trim()) {
                    self.message = Some(format!("That node is called {name}"));
                }
            }
            Purpose::GoToLine => match self.go_to_line(&name) {
                Ok((line, column)) => self.message = Some(format!("Line {line}, column {column}")),
                Err(problem) => self.message = Some(problem),
            },
            Purpose::RenameSpaceView(view) => {
                if self.space.space.rename_view(view, name.trim()) {
                    self.message = Some(format!("That view is called {name}"));
                }
            }
        }
    }
}
