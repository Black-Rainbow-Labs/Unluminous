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
        let remembered = std::mem::take(&mut self.terminals_to_restore);
        for (index, terminal) in remembered.iter().enumerate() {
            // **In the folder it was in, showing what was on it** - `task-1945`. A tab used to come back
            // as a fresh shell in the project's root with an empty screen, because a count and a name were
            // the only things this window had ever written down about one. Both are the canvas's own
            // mechanisms, said about a tab: `RememberedTerminal::folder` is where the shell had got to, and
            // `Screen::Tab` is the same file a node's screen lives in.
            let folder = match terminal.folder.trim().is_empty() {
                true => None,
                false => Some(std::path::PathBuf::from(&terminal.folder)),
            };
            self.open_a_terminal_tab_in(folder, Some(index));
        }
        // A name somebody typed is the one thing about a terminal that survives its shell, so it is
        // put back. A blank leaves the tab named after whatever program it is running, which is what
        // `Session::rename` already means by an empty name.
        for (index, terminal) in remembered.iter().enumerate() {
            if !terminal.name.is_empty() {
                self.terminal.tabs.rename(index, &terminal.name);
            }
        }
    }

    pub fn new_terminal_tab(&mut self) {
        self.open_a_terminal_tab_in(None, None);
    }

    /// Start this shell under the script that makes PowerShell report the folder it is in, when the setting
    /// says so.
    ///
    /// **One function rather than the test at each place a shell is started**, which is
    /// `follow_the_open_file`'s rule: the next place added would be the one that forgot. Both callers ask it
    /// *before* `print_a_remembered_screen_first`, because that rewrites the program into the shim that
    /// prints the screen and passes the shell and its arguments along behind a separator — so a wrapper
    /// applied after it would be wrapping `unluminous-cli`.
    ///
    /// Off, this writes nothing and starts nothing differently. See `services::shell_integration` for what is
    /// written and why it is a setting at all.
    pub(crate) fn ask_the_shell_to_report_its_folder(
        &self,
        settings: &mut unluminous_terminal::session::SessionSettings,
    ) {
        if !self.settings.shell_integration {
            return;
        }
        let Some(store) = self.store.as_ref() else {
            return;
        };
        // A window with no store is a test's window, which starts no shell anybody types into.
        let Some(script) =
            crate::services::shell_integration::write_the_script(store.folder())
        else {
            return;
        };
        crate::services::shell_integration::apply(settings, &script);
    }

    /// Open one terminal tab, in `folder` when there is one, printing the screen tab `restoring` was left
    /// showing when there is one of those.
    ///
    /// **One function rather than two**, because a restored tab and a fresh one differ in exactly those two
    /// things and everything else about opening a terminal - the grid size, the shell, the waker - is the
    /// same. `task-1945`.
    fn open_a_terminal_tab_in(
        &mut self,
        folder: Option<std::path::PathBuf>,
        restoring: Option<usize>,
    ) {
        // Both measurements from the rectangle the tile really has, since `task-1697`: a terminal
        // docked to the right is as tall as the body and as narrow as its column, and eighty columns
        // is not a guess that survives being moved.
        let size = self.terminal_grid_size();
        let folder =
            folder.filter(|path| path.is_dir()).unwrap_or_else(|| self.tree.root().to_path_buf());
        self.terminal.tabs.settings.shell = self.settings.shell();
        self.terminal.tabs.settings.working_directory = Some(folder);
        // **The shell is started underneath the program that prints the screen**, which is the only thing
        // that works: a screen written into the emulator from outside is erased by the console host on the
        // first write and comes back corrupt if it is put back later. `task-1912` measured it, and this is
        // the same call a terminal node makes.
        let shell = self.terminal.tabs.settings.shell.clone();
        let args = self.terminal.tabs.settings.args.clone();
        let mut settings = self.terminal.tabs.settings.clone();
        // **Before the screen is put in front of it**, because that rewrites the program into the shim. The
        // shell and its arguments travel behind the shim's own separator, so this has to have said what they
        // are first. `task-1950`.
        self.ask_the_shell_to_report_its_folder(&mut settings);
        if let Some(index) = restoring {
            self.print_a_remembered_screen_first(
                crate::services::space::store::Screen::Tab(index),
                &mut settings,
            );
        }
        self.terminal.tabs.settings = settings;
        let waker = self.waker();
        self.terminal.tabs.open(size, waker);
        // Put back, so the **next** tab is an ordinary one rather than one replaying somebody else's
        // screen: the settings are one value shared by every tab in the strip.
        self.terminal.tabs.settings.shell = shell;
        self.terminal.tabs.settings.args = args;
        self.terminal.tabs.settings.name = None;
    }

    /// Write down what every terminal tab is showing, so each can come back showing it.
    ///
    /// **Called when the window closes and at no other time**, which is `write_the_screens_down`'s own rule
    /// about the canvas's terminals and is the same rule for the same reason: a screen changes on every
    /// keystroke and what somebody wants back is the last one. `task-1945`.
    pub fn write_the_tab_screens_down(&mut self) {
        if !self.remembers_this_project() {
            return;
        }
        let root = self.tree.root().to_path_buf();
        let screens: Vec<Option<Vec<u8>>> = self
            .terminal
            .tabs
            .sessions()
            .iter()
            .map(unluminous_terminal::Session::screen_to_replay)
            .collect();
        for (index, bytes) in screens.iter().enumerate() {
            // The window is closing, so there is nowhere to report a failure that anybody would read. What
            // is lost is a screen coming back, which is not worth failing an exit over.
            let _ = crate::services::space::store::save_a_screen(
                &root,
                crate::services::space::store::Screen::Tab(index),
                bytes.as_deref(),
            );
        }
        // A strip that is shorter than it was leaves the screens of the tabs that have gone behind it, and
        // a tab opened into that slot tomorrow would replay a conversation that was never its own.
        for index in screens.len()..screens.len() + 16 {
            crate::services::space::store::forget_a_screen(
                &root,
                crate::services::space::store::Screen::Tab(index),
            );
        }
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
