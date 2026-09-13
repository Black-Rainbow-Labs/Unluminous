//! Starting a terminal, a run or a debug session, and the thread that wakes the window for them.
//!
//! A pseudoconsole must be opened at the size it will be drawn at and never resized while its
//! program is starting or after it has ended — both lose the program's output. That is why the grid
//! size is worked out from the rectangle the tile really has, showing or not.

use std::sync::Arc;

use egui::Vec2;

use crate::app::debug::DebugState;
use crate::components::terminal_panel::{self};
use crate::services::run_configurations::Configuration;

use crate::app::dock;
use crate::app::UnluminousApp;

impl UnluminousApp {
    /// Open a terminal tab if there is not one already, which is what showing the tile does.
    pub(crate) fn open_terminal_tab(&mut self) {
        if self.terminal.tabs.is_empty() {
            self.new_terminal_tab();
        }
    }

    /// Start another terminal, in the folder the explorer is showing, running the shell the settings
    /// name — or this machine's own when they name none.
    /// Start the shells this project was left with, once there is a window to show them in.
    ///
    /// **Nothing that the first frame does not need should happen before the first frame.** eframe
    /// keeps the window hidden until it has painted once, so every millisecond spent in
    /// `restore_project` is a millisecond of blank desktop — and starting a shell is a pseudoconsole
    /// and a process, which is by far the most expensive thing that function does. Measured for
    /// `task-1805` on a project left with two terminals: `restore_project` was **179 ms of a 724 ms
    /// startup**, and 179 of those 179 were these.
    ///
    /// Called from the end of the **second** frame, so the ordinary path is a window that appears and
    /// then fills its terminal tile a frame later. It is called from `pump_control` as well, because a
    /// command that arrives before then must not be answered with a window that is still half
    /// restored — `terminal list` would say there were none. Taking the list is what makes two call
    /// sites safe: whichever asks first does the work and the other finds nothing to do.
    ///
    /// The size is the same one the tile would have used before: `terminal_grid_size` falls back to a
    /// guess until the window has drawn, and after one frame it has the real rectangle — so this is,
    /// if anything, a better answer than the one it replaced.
    pub fn start_the_restored_terminals(&mut self) {
        if self.terminals_to_restore.is_empty() {
            return;
        }
        let names = std::mem::take(&mut self.terminals_to_restore);
        for _ in 0..names.len() {
            self.new_terminal_tab();
        }
        // A name somebody typed is the one thing about a terminal that survives its shell, so it is
        // put back. A blank leaves the tab named after whatever program it is running, which is what
        // `Session::rename` already means by an empty name.
        for (index, name) in names.iter().enumerate() {
            if !name.is_empty() {
                self.terminal.tabs.rename(index, name);
            }
        }
    }

    pub fn new_terminal_tab(&mut self) {
        // Both measurements from the rectangle the tile really has, since `task-1697`: a terminal
        // docked to the right is as tall as the body and as narrow as its column, and eighty columns
        // is not a guess that survives being moved.
        let size = self.terminal_grid_size();
        self.terminal.tabs.settings.shell = self.settings.shell();
        self.terminal.tabs.settings.working_directory = Some(self.tree.root().to_path_buf());
        let waker = self.waker();
        self.terminal.tabs.open(size, waker);
    }

    /// A terminal with no shell behind it, which is what the tests and the screenshot tests use so that what
    /// is drawn is the same on every run.
    pub fn new_detached_terminal_tab(&mut self, rows: usize, columns: usize) {
        let cell = self.renderer.cell_metrics(self.settings.terminal_font_size);
        let size = unluminous_terminal::session::Size::new(rows, columns)
            .with_cell(cell.width, cell.height);
        self.terminal.visible = true;
        self.terminal.tabs.open_detached(size);
    }

    /// A run with no program behind it, fed bytes directly.
    ///
    /// What the screenshot tests use, exactly as [`Self::new_detached_terminal_tab`] is what the
    /// terminal's use and for the same reason: when a real program answers is not something a test
    /// can know, so a picture of a run is taken of an emulator that was handed fixed bytes.
    ///
    /// The configuration is kept as a temporary as well, so the widget and the menu list it — which
    /// is what a real run would have done.
    pub fn new_detached_run(
        &mut self,
        configuration: Configuration,
        rows: usize,
        columns: usize,
    ) -> usize {
        let cell = self.renderer.cell_metrics(self.settings.terminal_font_size);
        let size = unluminous_terminal::session::Size::new(rows, columns)
            .with_cell(cell.width, cell.height);
        if self.run_configurations.find(&configuration.name).is_none() {
            self.run_configurations.add_temporary(configuration.clone());
        }
        self.run_selected = Some(configuration.name.clone());
        self.terminal.visible = false;
        self.run.visible = true;
        self.run.start_detached(configuration, size)
    }

    /// A debug session with no adapter behind it, fed messages directly.
    ///
    /// The third of the family, and it exists for the same reason [`Self::new_detached_run`] and
    /// [`Self::new_detached_terminal_tab`] do: when a real debugger answers is not something a test
    /// can know, so a picture of a paused program is taken of a session that was handed fixed
    /// messages. It goes through the same path a real session does — `begin`, and then every
    /// breakpoint in the project — so what a test drives is what the window really does.
    pub fn new_detached_debug_session(&mut self, adapter: &str, configuration: Configuration) {
        self.stop_debugging();
        if self.run_configurations.find(&configuration.name).is_none() {
            self.run_configurations.add_temporary(configuration.clone());
        }
        self.run_selected = Some(configuration.name.clone());
        let mut state = DebugState::detached(adapter, configuration);
        state.begin();
        self.debug = Some(state);
        self.send_every_breakpoint();
    }

    /// How many rows the terminal tile holds at its current height.
    fn terminal_grid_size(&self) -> unluminous_terminal::session::Size {
        let cell = self.renderer.cell_metrics(self.settings.terminal_font_size);
        let tile = self.panel_area(dock::Panel::Terminal);
        // The guess underneath is only ever reached before the window has drawn a frame at all,
        // which is the same fallback `run_grid_size` keeps and for the same reason.
        let size = match tile.width() > 1.0 && tile.height() > 1.0 {
            true => tile.size(),
            false => Vec2::new(
                self.editor_area.width().max(600.0),
                self.panes.height_of(dock::Panel::Terminal),
            ),
        };
        terminal_panel::grid_size(size, cell)
    }

    /// What the terminal calls to have the window drawn again when new output arrives.
    ///
    /// The terminal knows nothing about egui: it is given a function, and this is the function.
    pub(crate) fn waker(&self) -> unluminous_terminal::session::Waker {
        match &self.context {
            Some(context) => {
                let context = context.clone();
                Arc::new(move || context.request_repaint())
            }
            None => Arc::new(|| {}),
        }
    }

    /// What a thread calls to have the window drawn again when it has an answer.
    ///
    /// Two threads use it: the one that runs git, and the one `Find in Files` searches on. A reply
    /// arriving while the window is idle has to ask for the frame itself, or it sits unseen until
    /// the pointer next moves.
    pub(crate) fn thread_waker(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        match &self.context {
            Some(context) => {
                let context = context.clone();
                // Through `services::wake`, so a wake that arrives while the run loop has stopped
                // reading repaint requests goes by the route that gets through instead. `task-28`
                // measured what the bare `request_repaint` cost: a ticket's agent printed its banner,
                // this waker fired, the request was dropped, no frame was drawn for forty-one seconds,
                // and the handoff line — which is typed from `AgentTasks::pump`, on a frame — was never
                // typed at all. That module says what the two routes are and when each is used.
                Arc::new(move || {
                    let context = context.clone();
                    crate::services::wake::from_a_worker_thread(move || context.request_repaint());
                })
            }
            None => Arc::new(|| {}),
        }
    }
}
