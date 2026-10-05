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
        // **Cut, copy and paste are the explorer's while it has the keyboard** (`task-2194`). egui
        // delivers the first two as events of their own rather than as key presses, and a paste as a
        // `Paste` event only when the clipboard holds text, so the `V` press is watched for as well:
        // files on the clipboard are exactly the case with no text. Taken out of the frame's input,
        // so the editor beside the tree does not paste the same thing into the document.
        if let Some(key) = self.take_a_clipboard_key(ui) {
            return Some(key);
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

    /// The clipboard key pressed this frame, turned into the action the explorer's menu would give, and
    /// taken out of the input so nothing else acts on it.
    fn take_a_clipboard_key(&mut self, ui: &egui::Ui) -> Option<Action> {
        let pressed = ui.ctx().input_mut(|input| {
            let found = input.events.iter().find_map(|event| match event {
                egui::Event::Copy => Some(egui::Key::C),
                egui::Event::Cut => Some(egui::Key::X),
                egui::Event::Paste(_) => Some(egui::Key::V),
                egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. }
                    if modifiers.command =>
                {
                    Some(egui::Key::V)
                }
                _ => None,
            })?;
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                        | egui::Event::Key { key: egui::Key::V, .. }
                )
            });
            Some(found)
        })?;
        let cursor = self.selected.clone();
        match pressed {
            egui::Key::C => cursor.map(Action::CopyPath),
            egui::Key::X => cursor.map(Action::CutPath),
            _ => Some(Action::PasteInto(self.folder_to_paste_into())),
        }
    }

    /// The folder a paste with the keyboard goes into: the cursor's row when it is a folder, the folder
    /// the cursor's file is in when it is a file, and the project when the cursor is on nothing.
    pub fn folder_to_paste_into(&self) -> PathBuf {
        match &self.selected {
            Some(path) if path.is_dir() => path.clone(),
            Some(path) => {
                path.parent().map_or_else(|| self.tree.root().to_path_buf(), Path::to_path_buf)
            }
            None => self.tree.root().to_path_buf(),
        }
    }

    /// Every row chosen in the explorer: the rows picked with the modifier or `Shift` when there are
    /// any, otherwise the cursor's row on its own.
    pub fn explorer_choice(&self) -> Vec<PathBuf> {
        match self.chosen.is_empty() {
            true => self.selected.iter().cloned().collect(),
            false => self.chosen.clone(),
        }
    }

    /// The rows an action about `path` is about: every chosen row when `path` is one of them, which is
    /// a right click on one of several chosen rows, and `path` alone otherwise.
    pub fn choice_including(&self, path: &Path) -> Vec<PathBuf> {
        match self.chosen.iter().any(|row| row == path) {
            true => self.chosen.clone(),
            false => vec![path.to_path_buf()],
        }
    }

    /// Pick a row the way the click said: on its own, added to or taken from what is chosen, or every
    /// row from the anchor to it.
    pub fn pick_in_the_explorer(&mut self, path: PathBuf, pick: explorer::Pick) {
        match pick {
            explorer::Pick::Only => {
                self.chosen.clear();
                self.explorer_anchor = Some(path.clone());
            }
            explorer::Pick::Toggle => {
                if self.chosen.is_empty() {
                    self.chosen.extend(self.selected.iter().cloned());
                }
                match self.chosen.iter().position(|row| *row == path) {
                    Some(at) => {
                        self.chosen.remove(at);
                    }
                    None => self.chosen.push(path.clone()),
                }
                self.explorer_anchor = Some(path.clone());
            }
            explorer::Pick::Range => {
                let rows = self.explorer_rows();
                let anchor = self.explorer_anchor.clone().or_else(|| self.selected.clone());
                let from = anchor.and_then(|anchor| rows.iter().position(|row| *row == anchor));
                let to = rows.iter().position(|row| *row == path);
                if let (Some(from), Some(to)) = (from, to) {
                    let (first, last) = (from.min(to), from.max(to));
                    self.chosen = rows[first..=last].to_vec();
                } else {
                    self.chosen.clear();
                }
            }
        }
        self.selected = Some(path);
    }

    /// Where files another program is carrying over the window are, and whether this is the frame
    /// they were let go, or nothing when no such drag is happening.
    ///
    /// The pointer is asked of the system, because the platform's drag and drop sends the window no
    /// pointer movement of its own; see `services::system_files`. While the drag is over the window
    /// the frames are asked for here, because nothing else would draw the folder being aimed at.
    pub(crate) fn files_from_another_program(
        &self,
        context: &egui::Context,
    ) -> Option<(egui::Pos2, bool)> {
        let (hovering, dropped) = context.input(|input| {
            (!input.raw.hovered_files.is_empty(), !input.raw.dropped_files.is_empty())
        });
        if !hovering && !dropped {
            return None;
        }
        if hovering {
            context.request_repaint_after(std::time::Duration::from_millis(30));
        }
        let at = crate::services::system_files::pointer(context)
            .or_else(|| context.input(|input| input.pointer.hover_pos()))?;
        Some((at, dropped))
    }

    /// Copy the files another program dropped on the pane into `folder`, and show them.
    pub(crate) fn take_the_dropped_files(&mut self, context: &egui::Context, folder: &Path) {
        let paths: Vec<PathBuf> = context.input(|input| {
            input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect()
        });
        if paths.is_empty() {
            return;
        }
        self.copy_in(&paths, folder);
    }

    /// Copy `paths` from anywhere into `folder`, and show them, which is what a drop from another
    /// program and `explorer copy-in` both are.
    pub(crate) fn copy_in(&mut self, paths: &[PathBuf], folder: &Path) {
        let copied = crate::services::file_clipboard::transfer_into(
            paths,
            folder,
            crate::services::file_clipboard::Transfer::Copy,
        );
        self.show_what_arrived(folder, copied, "Copied");
    }

    /// Let a cut, a copy and a paste in the explorer reach the operating system's clipboard. The
    /// released binary calls this and a test does not.
    pub fn use_the_system_clipboard(&mut self) {
        self.system_clipboard = true;
    }

    /// Whether a paste has anything to paste, here or on the system clipboard.
    pub(crate) fn something_to_paste(&self) -> bool {
        !self.clipboard.is_empty()
            || (self.system_clipboard && crate::services::system_files::clipboard_has_files())
    }

    /// Paste into `folder`: files another program put on the system clipboard when there are some it
    /// did not get from here, and otherwise what was cut or copied in the explorer.
    pub(crate) fn paste_into_folder(&mut self, folder: &Path) {
        let outside = match self.system_clipboard {
            true => crate::services::system_files::clipboard_files(),
            false => Vec::new(),
        };
        let pasted = if !outside.is_empty() && !self.clipboard.holds(&outside) {
            crate::services::file_clipboard::transfer_into(
                &outside,
                folder,
                crate::services::file_clipboard::Transfer::Copy,
            )
        } else {
            self.clipboard.paste_into(folder)
        };
        self.show_what_arrived(folder, pasted, "Pasted");
    }

    /// Hold `paths` to be copied or moved by the next paste, here and in other programs.
    pub(crate) fn hold_for_pasting(&mut self, paths: Vec<PathBuf>, cut: bool) {
        if self.system_clipboard {
            crate::services::system_files::put_files_on_the_clipboard(&paths);
        }
        let count = paths.len();
        match cut {
            true => self.clipboard.cut_all(paths),
            false => self.clipboard.copy_all(paths),
        }
        let what = if count == 1 { "1 item".to_owned() } else { format!("{count} items") };
        let verb = if cut { "Cut" } else { "Copied" };
        self.message = Some(format!("{verb} {what}"));
    }

    /// Read the tree again, open the folder something arrived in, choose what arrived, and say so.
    fn show_what_arrived(
        &mut self,
        folder: &Path,
        arrived: std::io::Result<Vec<PathBuf>>,
        verb: &str,
    ) {
        match arrived {
            Ok(paths) => {
                self.tree.reload();
                self.tree.expand(folder);
                let name = folder.file_name().map_or_else(
                    || folder.display().to_string(),
                    |name| name.to_string_lossy().to_string(),
                );
                self.message = Some(match paths.len() {
                    1 => format!("{verb} {} into {name}", paths[0].display()),
                    count => format!("{verb} {count} items into {name}"),
                });
                self.chosen = if paths.len() > 1 { paths.clone() } else { Vec::new() };
                self.selected = paths.last().cloned();
                self.reveal_selection = REVEAL_FRAMES;
            }
            Err(problem) => self.message = Some(format!("Unluminous could not paste: {problem}")),
        }
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
            chosen: &[],
            outside_drag: None,
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
        // **The file the tab is about, not the document's own path.** A rendered tab holding a local
        // HTML file has a page rather than a document, so its `path()` is `None` and the explorer
        // followed nothing at all when one was shown — `task-2009`: *"When I have an html file open in
        // browser tab, that file should be selected, and should change if I select other tabs."*
        let showing = self.files.active().file_on_disk().map(Path::to_path_buf);
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
        if let Some(path) = self.files.active().file_on_disk().map(Path::to_path_buf) {
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
        // And the tabs that are showing, on the same timer and for the same reason. `task-2062`: an agent
        // changed a Markdown file and the window went on drawing what it had — the tab is owned by its
        // `Document` and Unluminous watches nothing, which is right while Unluminous is the only writer.
        // `reread_if_the_file_changed` existed for exactly this and was called from the two command line
        // paths alone, so a caller that asked `editor text` was handed the new words and the person
        // looking at the window was not. One `metadata` call per pane, beside the handful the tree above
        // already makes.
        self.reread_the_showing_tabs_that_changed();
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

    /// Read the tabs a person can see again when their files have changed underneath them.
    ///
    /// `task-2062` reported that an agent's change to a Markdown file was not shown. A tab is owned by
    /// its `Document` and Unluminous watches nothing, which is the right rule while Unluminous is the
    /// only writer and the wrong answer the moment anything else writes — and
    /// `UnluminousApp::reread_if_the_file_changed` had existed for that since `task-1661` with its two
    /// callers both on the command line. So `editor text` was fresh and the window was stale, which is
    /// the one thing this repository's own rule says must never differ: a person and an agent looking
    /// at one file were shown different things.
    ///
    /// **Only the tabs that are showing**, which is one per editing pane and one per File Editor node on
    /// the canvas. A hidden tab is read again when it is next shown, because `reread_if_the_file_changed`
    /// asks at the moment of use and showing a tab is a use; walking every open tab here would be one
    /// `metadata` call per tab twice a second for files nobody is looking at, which is the shape of the
    /// fault `task-1805` found in the explorer's own footer.
    ///
    /// **A tab with unsaved changes is never touched**, which is `the_file_changed_underneath`'s own
    /// answer rather than a second rule here: those belong to the person and losing them has no undo.
    pub(crate) fn reread_the_showing_tabs_that_changed(&mut self) {
        // The homes there are, asked of the files rather than of the panes, so a File Editor node on the
        // canvas is covered by the same walk. Collected first because reading a file changes the list.
        let mut homes: Vec<crate::app::files::Home> = Vec::new();
        for home in self.files.iter().map(|file| file.home) {
            if !homes.contains(&home) {
                homes.push(home);
            }
        }

        // The path and where it was being read, because the second is given back afterwards. See below.
        let showing: Vec<(PathBuf, f32)> = homes
            .into_iter()
            .filter_map(|home| self.files.showing_at(home))
            .filter_map(|index| self.files.get(index))
            .filter(|file| file.the_file_changed_underneath())
            .filter_map(|file| {
                file.document.path().map(Path::to_path_buf).map(|at| (at, file.scroll))
            })
            .collect();
        for (path, scroll) in showing {
            // Through the one function that reads a file into a tab, so a reload from the timer, from
            // the explorer's own menu and from `tab reload` are the same thing. It never discards.
            //
            // **`reload_from_disk` walks the project as well**, and on this timer that walk has either
            // just happened above or was not needed. Measured on the installed build with an agent
            // appending to the open file once a second, this frame cost **88 ms** — the watch phase's
            // median is 0.59 — because a folder's modification time does not move when a file in it is
            // *written*, so the tree above was correctly left alone and this walked all 564 folders
            // anyway. `reread_the_tab_only` is that function without the walk.
            if !self.reread_the_tab_only(&path) {
                continue;
            }
            // **But the reading position is given back, which an explicit reload does not do.** The two
            // are different gestures: somebody who chose `Reload from Disk` asked to start again at the
            // top, and nobody asked for this one at all — so a person reading the end of a document an
            // agent appends to would be thrown to the top of it, twice a second, for as long as the
            // agent kept writing. The scroll is clamped by the layout on the next frame, so a file that
            // has become shorter than where they were is handled where that is already handled.
            if let Some(index) = self.files.index_of(&path) {
                if let Some(file) = self.files.get_mut(index) {
                    file.scroll = scroll;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests_task_2062 {
    use crate::app::UnluminousApp;

    /// A project with one file in it, and a window with that file open.
    fn a_window(name: &str) -> (std::path::PathBuf, UnluminousApp) {
        let folder = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("make the folder");
        std::fs::write(folder.join("notes.md"), "# before\n").expect("write notes.md");
        let mut app = UnluminousApp::new(&folder);
        app.open_path_permanently(&folder.join("notes.md")).expect("the file opens");
        (folder, app)
    }

    /// Let the timer come round, which is what a frame does every `crate::app::WATCH_INTERVAL`.
    ///
    /// Waiting the interval out rather than reaching in and moving the clock, so what is exercised is
    /// the real gate a frame goes through.
    fn let_the_watch_run(app: &mut UnluminousApp) {
        std::thread::sleep(crate::app::WATCH_INTERVAL + std::time::Duration::from_millis(50));
        app.notice_what_changed_on_disk();
    }

    /// **The window draws what is in the file, not what it read when the tab was opened.**
    ///
    /// `task-2062`: an agent changed a Markdown file and the editor went on showing the old words.
    /// `reread_if_the_file_changed` had existed since `task-1661` with both its callers on the command
    /// line, so `editor text` was fresh while the window was stale — which is the one thing this
    /// repository's rule says must never differ.
    #[test]
    fn a_file_another_program_changed_is_read_again_without_anybody_asking() {
        let (folder, mut app) = a_window("unluminous-agent-edit-shown");
        assert_eq!(app.document().text().to_string(), "# before\n");

        // A folder's modification time has whole-second resolution on some file systems, and
        // `DiskStamp` carries the length as well — so a change of length is seen whatever the clock
        // says. This one changes both.
        std::fs::write(folder.join("notes.md"), "# after, and longer\n").expect("the agent writes");
        let_the_watch_run(&mut app);
        assert_eq!(
            app.document().text().to_string(),
            "# after, and longer\n",
            "the window is still showing what it read when the tab was opened"
        );

        std::fs::remove_dir_all(&folder).ok();
    }

    /// **Unsaved changes are never thrown away by the watch.** They belong to the person, losing them
    /// has no undo, and `tab reload --discard` is how somebody says they mean it. This is
    /// `the_file_changed_underneath`'s own answer rather than a second rule, and it is asserted here
    /// because the watch is the one caller nobody asked for.
    #[test]
    fn a_tab_with_unsaved_changes_is_left_exactly_as_it_is() {
        let (folder, mut app) = a_window("unluminous-agent-edit-unsaved");
        app.document_mut().apply(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
        app.document_mut().apply(unluminous_core::Command::Insert("mine".to_owned()));
        assert!(
            app.document().is_modified(),
            "the tab is not modified, so this test is about nothing"
        );

        std::fs::write(folder.join("notes.md"), "# theirs\n").expect("the agent writes");
        let_the_watch_run(&mut app);
        assert!(
            app.document().text().to_string().starts_with("mine"),
            "the watch threw away what somebody had typed: {:?}",
            app.document().text().to_string()
        );
        assert!(app.document().is_modified(), "and it cleared the unsaved marker");

        std::fs::remove_dir_all(&folder).ok();
    }

    /// The reading position survives a re-read nobody asked for, so a person reading the end of a
    /// document an agent is appending to is not thrown to the top of it twice a second.
    #[test]
    fn a_re_read_nobody_asked_for_keeps_the_place_it_was_being_read_at() {
        let (folder, mut app) = a_window("unluminous-agent-edit-scroll");
        let index = app.files.active_index();
        app.files.at_mut(index).scroll = 420.0;

        std::fs::write(folder.join("notes.md"), "# a longer document than before\n")
            .expect("the agent writes");
        let_the_watch_run(&mut app);
        assert_eq!(app.document().text().to_string(), "# a longer document than before\n");
        assert_eq!(
            app.files.at(app.files.active_index()).scroll,
            420.0,
            "the reader was thrown back to the top of the file"
        );

        std::fs::remove_dir_all(&folder).ok();
    }
}
