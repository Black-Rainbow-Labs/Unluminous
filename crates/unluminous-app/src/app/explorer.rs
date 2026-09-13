//! The explorer's half of the window: the keyboard, the cursor, and following the tab that is showing.
//!
//! The file showing in the pane with the keyboard is selected in the explorer, the folders above it
//! are opened out, and the list is scrolled the **least** amount that brings the row into view. That
//! is derived from the state rather than fired from each of the places a tab can change, because the
//! next one added would be the one that forgot.
//!
//! The folders that are showing are asked whether they have changed, on a timer. Creating, deleting
//! or renaming an entry moves the modification time of the folder it is in, so the root plus every
//! folder that is opened out is the complete set of places a visible change can happen.

use std::path::{Path, PathBuf};

use egui::Rect;

use crate::components::explorer;

use crate::app::actions::Action;
use crate::app::dock;
use crate::app::{
    a_modal_has_the_keyboard, text_box_has_the_keyboard, Focus, UnluminousApp, REVEAL_FRAMES,
    WATCH_INTERVAL,
};

impl UnluminousApp {
    /// The keys the explorer takes, and only while it has the keyboard.
    ///
    /// `Up` and `Down` walk the rows that are showing, so a row inside a shut folder is never
    /// stepped onto; `Right` opens a folder and `Left` shuts it or steps to its parent; `Enter`
    /// opens the file permanently and hands the keyboard to the editor; `Escape` hands it back
    /// without opening anything; and `Delete` — or `Backspace`, which is the key a Mac keyboard has
    /// — asks the question.
    pub(crate) fn route_the_explorer_keys(&mut self, ui: &egui::Ui) -> Option<Action> {
        if self.focus != Focus::Explorer || !self.explorer_visible {
            return None;
        }
        // A modal is open: its keys are its own. Without this, `Delete` in the explorer opens the
        // confirmation and the `Enter` that answers it also opens the row the cursor is on.
        if a_modal_has_the_keyboard(ui.ctx()) {
            return None;
        }
        // A field with the keyboard is typed into, not navigated with. The filter box is the only
        // one in the panel, and while it has the focus its own arrow keys move the caret.
        //
        // The question is whether a **text box** has the keyboard, not whether anything at all has
        // the focus: `hold_the_keyboard` keeps the focus on a widget of Unluminous's own the rest of the
        // time, and the broader question would leave the tree unable to be walked at all.
        if text_box_has_the_keyboard(ui.ctx()) {
            return None;
        }
        // A letter typed while the tree has the keyboard belongs to the **editor**. The explorer has
        // no use for one, and "click a file in the tree and start typing" has to go on working
        // exactly as it did — the keyboard is handed over here, before any pane reads the frame's
        // input, so the letter that caused it lands in the document.
        if ui.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Text(_))))
        {
            self.focus = Focus::Editor;
            return None;
        }
        let key = ui
            .input(|input| {
                [
                    egui::Key::ArrowDown,
                    egui::Key::ArrowUp,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowLeft,
                    egui::Key::Enter,
                    egui::Key::Escape,
                    egui::Key::Delete,
                ]
                .into_iter()
                .find(|key| input.key_pressed(*key))
            })
            .or_else(|| {
                // The Mac keyboard has no `Delete`, and `Backspace` on its own is far too close to what
                // somebody who has just clicked a file is about to type. The reference editor's own answer on macOS
                // is the command key with it, and that is unambiguous on every platform.
                ui.input(|input| input.key_pressed(egui::Key::Backspace) && input.modifiers.command)
                    .then_some(egui::Key::Delete)
            })?;
        match key {
            egui::Key::ArrowDown => self.step_the_selection(1),
            egui::Key::ArrowUp => self.step_the_selection(-1),
            egui::Key::ArrowRight => self.open_the_selected_folder(true),
            egui::Key::ArrowLeft => self.open_the_selected_folder(false),
            egui::Key::Enter => {
                let path = self.selected.clone()?;
                if path.is_dir() {
                    self.tree.toggle(&path);
                } else if self.open_path_permanently(&path).is_ok() {
                    // The keyboard goes with the file, so it stays in the explorer when the file
                    // did not open and the reason is on the status bar.
                    self.focus = Focus::Editor;
                }
            }
            egui::Key::Escape => self.focus = Focus::Editor,
            egui::Key::Delete => {
                return self.selected.clone().map(Action::DeletePath);
            }
            _ => {}
        }
        None
    }

    /// Move the explorer's cursor by `step` rows, through the rows that are showing.
    fn step_the_selection(&mut self, step: isize) {
        let rows: Vec<PathBuf> = self.explorer_rows();
        if rows.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|path| rows.iter().position(|row| row == path))
            .map(|at| (at as isize + step).clamp(0, rows.len() as isize - 1) as usize)
            .unwrap_or(if step > 0 { 0 } else { rows.len() - 1 });
        self.selected = Some(rows[at].clone());
        self.reveal_selection = REVEAL_FRAMES;
    }

    /// `Right` opens the folder the cursor is on; `Left` shuts it, or steps to the folder above.
    fn open_the_selected_folder(&mut self, open: bool) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        let showing = self.tree.find(&path).map(|entry| entry.expanded).unwrap_or(false);
        if path.is_dir() && showing != open {
            self.tree.toggle(&path);
            return;
        }
        if !open {
            if let Some(folder) = path.parent() {
                if folder.starts_with(self.tree.root()) && folder != self.tree.root() {
                    self.selected = Some(folder.to_path_buf());
                    self.reveal_selection = REVEAL_FRAMES;
                }
            }
        }
    }

    /// The rows the explorer is showing, in order — the same list it draws.
    fn explorer_rows(&self) -> Vec<PathBuf> {
        if self.filter.trim().is_empty() {
            self.tree.rows().iter().map(|row| row.entry.path.clone()).collect()
        } else {
            self.tree.matching(&self.filter).iter().map(|path| path.to_path_buf()).collect()
        }
    }

    /// Where the explorer's filter box is, which is what `components::explorer` really draws it at.
    ///
    /// Read by `the_filter_box_puts_its_words_on_the_same_line_as_the_magnifier`, which used to spell the
    /// position out as a copy of the component's own arithmetic and therefore went stale when the explorer
    /// moved. Only the panel's rectangle and its zoom are needed, and the window has both.
    pub fn explorer_filter_field(&self) -> Rect {
        let area = self.panel_area(dock::Panel::Explorer);
        // The `View` is built with the fields the drawing reads for a position and nothing else: the
        // filter box's rectangle depends on the panel's own measurements and its zoom, so `Host::Panel`
        // is what makes this the panel's answer rather than a node's.
        let view = explorer::View {
            current: None,
            selected: None,
            keyboard: false,
            unsaved: false,
            reveal: false,
            reveal_selected: false,
            opacity: self.settings.opacity,
            zoom: self.panes.zoom_of(dock::Panel::Explorer),
            scroll_to: None,
            host: explorer::Host::Panel,
        };
        explorer::filter_field(area, &view, explorer::heading_height(&view))
    }

    /// Keep the explorer's selection on the file that is showing.
    ///
    /// Derived from the state rather than fired from each of the places a tab can change — there are
    /// eleven of those today and the twelfth, added next month, would be the one that forgot. It
    /// costs one comparison a frame.
    pub(crate) fn follow_the_open_file(&mut self) {
        let showing = self.files.active().path().map(Path::to_path_buf);
        if showing == self.revealed {
            return;
        }
        self.revealed = showing.clone();
        let Some(path) = showing else {
            return;
        };
        // Opening out the folders above it, so there is a row to select at all. `expand` walks the
        // components and opens each folder; the file itself is not a folder, so it is left alone.
        self.tree.expand(&path);
        self.reveal_in_explorer = REVEAL_FRAMES;
    }

    /// Show the explorer, scrolled to the file that is showing. `View -> Select Opened File`.
    pub(crate) fn select_the_open_file(&mut self) {
        self.explorer_visible = true;
        if let Some(path) = self.files.active().path().map(Path::to_path_buf) {
            self.tree.expand(&path);
        }
        // The filter box is what the explorer draws instead of the tree, and a file that does not
        // match it has no row to scroll to, so asking to be shown where a file is clears it.
        self.filter.clear();
        self.reveal_in_explorer = REVEAL_FRAMES;
    }

    /// Read the tree again when a folder that is showing has been written to since it was last read.
    ///
    /// `task-1693`: a file or a folder made by anything other than Unluminous — an agent with its own
    /// tools, a build, a command in the terminal tile — never appeared in the explorer, because the
    /// tree is only read when Unluminous is told to read it. The folders that are **showing** are asked,
    /// on a timer, which is a handful of `metadata` calls; `FileTree::changed_on_disk` records why
    /// that is the right shape rather than a watcher.
    ///
    /// **Not while a row is being carried.** Reloading rebuilds the entries under the drag, and a
    /// drop that landed on a folder which had just been replaced would be a move somebody did not
    /// ask for.
    pub(crate) fn notice_what_changed_on_disk(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_watched) < WATCH_INTERVAL {
            return;
        }
        self.last_watched = now;
        if self.dragging_a_row {
            return;
        }
        if self.tree.changed_on_disk() {
            self.tree.reload();
        }
        // And the project's own state, on the same timer and for the same reason. `task-1794`: a
        // `git checkout` under a running window put `.unluminous/breakpoints.conf` back and the window
        // went on holding what it had. One `metadata` call, beside the handful the tree already
        // makes — and a session that starts before the timer next fires re-reads it itself, because
        // `send_every_breakpoint` asks at the moment of use.
        if self.adopt_the_breakpoints_from_disk() {
            // A live session is holding what it was told; the file has changed, so it is told again.
            if self.debug.is_some() {
                self.send_every_breakpoint();
            }
        }
    }
}
