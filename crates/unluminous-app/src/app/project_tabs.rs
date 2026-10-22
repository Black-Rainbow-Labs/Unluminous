//! Several projects in one window, a project tab each (`task-2322`).
//!
//! *"We want the ability to open N projects in a window, where each project gets its own custom layout (same
//! behavior as if they were open in different windows, but now I want to switch via tabs at the top)."*
//!
//! **A project tab is a whole [`UnluminousApp`].** That type is one project in one window, and almost every one of
//! its fields belongs to the project: the open files, the explorer, the terminals, the realm, the run
//! configurations, the debugger, git, the code index, the command channel, and the plugin providers that draw
//! Agent Chat, Agent Tasks and the rest. So [`ProjectWindow`] holds one `UnluminousApp` a tab and draws the one
//! showing, and everything about a tab is exactly what it would be in a window of its own, with nothing in any of
//! those features changed. `tasks/task-2322-multi-project-tabs-tdd.md` weighs this against splitting the type in
//! two.
//!
//! The window decides five things and only five:
//!
//! - **Which tab is drawn.** The others are given [`UnluminousApp::keep_working_while_hidden`] every frame, which
//!   is what eframe gives a minimised window: the command channel is answered, the debugger and the kernels are
//!   heard, and every plugin has its turn on the clock. Agent Chat in a hidden tab goes on streaming.
//! - **The settings**, which are one file a person: a tab that is shown is given the settings the tab that was
//!   showing had. The pane sizes and where each panel is docked are not copied, because those are each
//!   project's own layout (`project_state::save_layout`).
//! - **The native things there is one of**: the macOS menu bar moves to the tab that is showing, the pages of a
//!   tab that stops showing are hidden, and it lets go of the microphone.
//! - **What a tab's children are told.** Every call into a tab runs under [`control::running_for`], so an agent
//!   a tab starts is told that tab's port and drives that tab.
//! - **Closing**, of a tab and of the window.
//!
//! A tab asks the window for anything about the tabs through [`UnluminousApp::project_tab_asked`], because the
//! tabs are the window's: the title bar's strip, the menus and `unluminous-cli project tab-*` all go that way,
//! and the window acts once the frame is drawn.

use std::path::{Path, PathBuf};

use egui::{Pos2, Rect};

use crate::app::actions::Action;
use crate::app::UnluminousApp;
use crate::components::project_tabs as strip;
use crate::services::control;

/// One project tab, as every tab is told about it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectTab {
    /// The tab's number in its window, which never changes and is never reused while the window is open.
    pub key: u32,
    /// The project folder's own name, which is what its pill says.
    pub name: String,
    pub folder: PathBuf,
    /// The port its command channel listens on, when it has one.
    pub port: Option<u16>,
}

/// What a window says about its project tabs, given to every tab before it runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectTabs {
    /// False for an `UnluminousApp` that no `ProjectWindow` holds, which is what a screenshot test builds.
    pub in_a_window: bool,
    pub tabs: Vec<ProjectTab>,
    /// Which of them the window is drawing.
    pub showing: usize,
}

/// What a tab asks its window to do about the tabs.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Open this folder in a new tab and show it, or show the tab already on it.
    Open(PathBuf),
    /// Show this tab.
    Show(usize),
    /// Close this tab.
    Close(usize),
    /// Close every tab but this one.
    CloseOthers(usize),
    /// Close this tab and open its project in a window of its own.
    ToNewWindow(usize),
    /// Show the tab this many places after the one showing, going round.
    Step(isize),
    /// Close the window, every tab in it.
    CloseWindow,
}

/// The name a tab's pill gives a folder: its last part, or the whole path when it has none.
pub fn name_of(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.display().to_string())
}

impl UnluminousApp {
    /// What this window's tabs are, from the window when one holds this tab and as one tab of its own when none
    /// does.
    pub fn project_tabs(&self) -> ProjectTabs {
        if self.project_tabs.in_a_window {
            return self.project_tabs.clone();
        }
        ProjectTabs {
            in_a_window: false,
            tabs: vec![ProjectTab {
                key: 0,
                name: self.folder_name().unwrap_or_else(|| name_of(self.tree.root())),
                folder: self.tree.root().to_path_buf(),
                port: self.control.as_ref().map(|server| server.port()),
            }],
            showing: 0,
        }
    }

    /// Ask the window to do something about the tabs. A tab no window holds has nobody to ask, and says so.
    pub(crate) fn ask_the_window(&mut self, ask: Ask) {
        if !self.project_tabs.in_a_window {
            self.message = Some(
                "This window holds one project, so it has no project tabs to change.".to_owned(),
            );
            return;
        }
        self.project_tab_asked.push(ask);
    }

    /// The tab a project tab's menu was opened on, taken, or the one showing.
    fn the_tab_the_menu_is_about(&mut self) -> usize {
        self.project_tab_menu.take().map(|(_, index)| index).unwrap_or(self.project_tabs.showing)
    }

    /// The project tab entries of the menus. Reached only from `run_action`, which decides that an action is one
    /// of these, so the last arm cannot happen.
    pub(crate) fn a_project_tab_entry(&mut self, action: Action) {
        match action {
            Action::NewProjectTab => self.open_the_project_dropdown(),
            Action::OpenInProjectTab(folder) => self.ask_the_window(Ask::Open(folder)),
            Action::CloseProjectTab => {
                let index = self.the_tab_the_menu_is_about();
                self.ask_the_window(Ask::Close(index));
            }
            Action::CloseOtherProjectTabs => {
                let index = self.the_tab_the_menu_is_about();
                self.ask_the_window(Ask::CloseOthers(index));
            }
            Action::OpenProjectTabInNewWindow => {
                let index = self.the_tab_the_menu_is_about();
                self.ask_the_window(Ask::ToNewWindow(index));
            }
            Action::NextProjectTab => self.ask_the_window(Ask::Step(1)),
            Action::PreviousProjectTab => self.ask_the_window(Ask::Step(-1)),
            other => unreachable!("{other:?} is not handled by a_project_tab_entry"),
        }
    }

    /// Open the dropdown under the plus, or put it away when it is open.
    pub(crate) fn open_the_project_dropdown(&mut self) {
        self.project_dropdown = match self.project_dropdown {
            Some(_) => None,
            None => Some(strip::DropdownState::default()),
        };
    }

    /// The recent projects the dropdown offers, newest first, each saying which tab it is open in here.
    pub(crate) fn rows_for_the_project_dropdown(&self) -> Vec<strip::Row> {
        let tabs = self.project_tabs();
        let mut folders: Vec<PathBuf> = self.recent.clone();
        // A tab whose project is not in the recent list is still offered, so the list says what is open.
        for tab in &tabs.tabs {
            if !folders.contains(&tab.folder) {
                folders.push(tab.folder.clone());
            }
        }
        folders
            .into_iter()
            .map(|folder| {
                let tab = tabs.tabs.iter().position(|tab| tab.folder == folder);
                strip::Row {
                    name: name_of(&folder),
                    showing: tab == Some(tabs.showing),
                    tab,
                    folder,
                }
            })
            .collect()
    }

    /// Ask the platform's folder picker for a folder, and open it in a new tab.
    pub(crate) fn choose_a_folder_for_a_project_tab(&mut self) {
        let start = self.tree.root().parent().unwrap_or(self.tree.root()).to_path_buf();
        if let Some(folder) = (self.folder_picker)("Open a Folder in a New Tab", &start) {
            self.ask_the_window(Ask::Open(folder));
        }
    }

    /// The project tab's own menu and the plus's dropdown, drawn over the panes.
    pub(crate) fn show_the_project_tab_menus(
        &mut self,
        ui: &mut egui::Ui,
        action: &mut Option<Action>,
    ) {
        if let Some((at, _)) = self.project_tab_menu {
            let mut entries = crate::app::actions::project_tab_menu(&self.menu_state());
            crate::services::keymap::apply_entries(&mut entries, &self.keymap);
            let outcome = crate::components::context_menu::show(ui, "project-tab", at, &entries);
            if let Some(chosen) = outcome.chosen {
                *action = Some(chosen);
            } else if outcome.close {
                self.project_tab_menu = None;
            }
        }
        let Some(mut state) = self.project_dropdown.take() else { return };
        let anchor = match self.project_plus_rect {
            Some(rect) => rect,
            None => Rect::from_min_size(Pos2::new(120.0, 8.0), egui::vec2(30.0, 30.0)),
        };
        let rows = self.rows_for_the_project_dropdown();
        let outcome = strip::show_the_dropdown(ui.ctx(), anchor, &mut state, &rows);
        if let Some(folder) = outcome.open {
            self.ask_the_window(Ask::Open(folder));
        }
        if outcome.choose_a_folder {
            self.choose_a_folder_for_a_project_tab();
        }
        // A press on the plus that opened it is the plus's own, which toggles it on the frame after.
        if !outcome.closed {
            self.project_dropdown = Some(state);
        }
    }

    /// A new `UnluminousApp` on `folder`, set up the way this one was: given a settings folder when this one was,
    /// remembering its project when this one does, with a command channel when this one has one, and allowed
    /// the machine's clipboard, its language servers, its microphone and its code index on disk when this one
    /// was. So a tab opened in the released binary is a released window's project, and one opened in a test is a
    /// test's.
    ///
    /// `tab` is the new tab's number in the window, which names its instance file.
    pub fn sibling_on(&self, folder: &Path, ctx: &egui::Context, tab: u32) -> UnluminousApp {
        let mut app = UnluminousApp::new(folder);
        app.project_tab_number = tab;
        app.prepare(ctx);
        if self.drawn_deterministically {
            app.draw_deterministically();
        }
        app.menu_placement = self.menu_placement;
        app.folder_picker = self.folder_picker;
        if let Some(store) = &self.store {
            app.realm.live.real_players = self.realm.live.real_players;
            app.use_store(crate::services::store::Store::at(store.folder()));
        }
        if self.system_clipboard {
            app.use_the_system_clipboard();
        }
        if self.index_on_disk {
            app.index_on_disk();
        }
        if self.servers.allowed {
            app.allow_language_servers();
        }
        if self.voice.microphone_allowed {
            app.allow_the_microphone();
        }
        if self.endpoints.acp.is_read_when_shown() {
            app.endpoints.acp.read_when_shown();
        }
        // The same setup steps, and never leave to open the dialog by itself: the window's first tab asked.
        app.setup.planner = self.setup.planner.clone();
        app.setup.taker = self.setup.taker.clone();
        app.setup.opener = self.setup.opener.clone();
        if self.written_project.is_some() {
            app.restore_project();
        }
        if self.control.is_some() {
            app.open_control_channel_for_tab(ctx, tab);
        }
        app
    }

    /// This tab stops being drawn: its pages leave the screen, it stops listening, and its menus close.
    pub(crate) fn put_the_project_tab_away(&mut self) {
        self.browser.hide_every_view();
        self.browser_placements.clear();
        self.let_go_of_the_microphone();
        self.project_tab_menu = None;
        self.project_dropdown = None;
        if let Some(server) = self.control.as_mut() {
            server.set_showing(false);
        }
    }

    /// This tab is drawn from now on, taking over from `outgoing`: the settings it had, the macOS menu bar, and
    /// the theme, which `prepare` on a tab opened since may have put back to Unluminous's own.
    pub(crate) fn take_the_screen_from(&mut self, outgoing: &mut UnluminousApp) {
        if self.settings != outgoing.settings {
            self.set_settings(outgoing.settings.clone());
        }
        self.apply_the_theme();
        if self.native_menu.is_none() {
            self.native_menu = outgoing.native_menu.take();
        }
        self.native_resize |= outgoing.native_resize;
        if let Some(server) = self.control.as_mut() {
            server.set_showing(true);
        }
        self.mark_the_showing_tab_in_the_session();
    }

    /// Mark this tab as the one its window shows, in the session file, so the window comes back showing it.
    pub(crate) fn mark_the_showing_tab_in_the_session(&self) {
        if !crate::services::store::this_window_joins_the_session() || self.waiting_for_a_project {
            return;
        }
        if let Some(store) = &self.store {
            store.mark_the_showing_tab(self.tree.root(), std::process::id());
        }
    }

    /// Close the window, or ask the window holding this tab to close every tab in it.
    ///
    /// The close button, `Close Window` and `Quit` all come here. A tab in a window of several cannot send the
    /// window away by itself, because the other tabs have files to save and a session to write.
    pub(crate) fn close_the_window(&mut self, ctx: &egui::Context) {
        if self.project_tabs.in_a_window && self.project_tabs.tabs.len() > 1 {
            self.project_tab_asked.push(Ask::CloseWindow);
            return;
        }
        if self.may_the_window_close() {
            self.closing = true;
            self.closed_by_hand = true;
            self.write_settings();
            self.remember_the_project(None);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Answer every command this tab is holding, because the tab is closing and its channel goes with it.
    ///
    /// A click that closed the tab is the usual one: `input click` is answered once its steps have been fed,
    /// which is after the frame the click closed the tab on, and without this the caller was told the request
    /// was dropped. Input that was fed is answered as done; anything else as not finished.
    pub(crate) fn answer_what_is_held_before_closing(&mut self) {
        for (pending, _) in std::mem::take(&mut self.cli_waiting) {
            let command = pending.command().to_owned();
            let reply = match command.starts_with("input.") {
                true => unluminous_cli::protocol::Reply::done(
                    &command,
                    "Done. The project tab it was sent to has closed.",
                    serde_json::Value::Null,
                ),
                false => unluminous_cli::protocol::Reply::failed(
                    &command,
                    unluminous_cli::protocol::code::FAILED,
                    "The project tab this was sent to closed before it finished.",
                ),
            };
            pending.answer(reply);
        }
    }

    /// The address this tab's children are told: its port when it has a command channel.
    fn address_for_children(&self) -> Option<String> {
        self.control.as_ref().map(|server| server.port().to_string())
    }
}

/// One tab of a window.
struct Tab {
    key: u32,
    app: UnluminousApp,
}

/// A window holding a project a tab. See the module comment.
pub struct ProjectWindow {
    tabs: Vec<Tab>,
    showing: usize,
    next_key: u32,
    /// The context, once the window has one, which opening a tab needs to set its fonts up.
    context: Option<egui::Context>,
    /// Set when the window was asked to close: every tab has been asked, and the window is going.
    closing: bool,
}

impl ProjectWindow {
    /// A window whose first tab is `first`.
    pub fn new(mut first: UnluminousApp) -> Self {
        first.project_tab_number = 0;
        let mut window = Self {
            tabs: vec![Tab { key: 0, app: first }],
            showing: 0,
            next_key: 1,
            context: None,
            closing: false,
        };
        window.tell_every_tab();
        window
    }

    /// How many tabs the window holds.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Which tab is showing.
    pub fn showing(&self) -> usize {
        self.showing
    }

    /// The tab that is showing.
    pub fn app(&self) -> &UnluminousApp {
        &self.tabs[self.showing].app
    }

    pub fn app_mut(&mut self) -> &mut UnluminousApp {
        &mut self.tabs[self.showing].app
    }

    /// Tab `index`.
    pub fn tab(&self, index: usize) -> &UnluminousApp {
        &self.tabs[index].app
    }

    pub fn tab_mut(&mut self, index: usize) -> &mut UnluminousApp {
        &mut self.tabs[index].app
    }

    /// Every tab's project folder, in order.
    pub fn folders(&self) -> Vec<PathBuf> {
        self.tabs.iter().map(|tab| tab.app.tree.root().to_path_buf()).collect()
    }

    /// Whether the window has been asked to close.
    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Run `work` on tab `index`, with the tab's address named for any child it starts.
    fn run_on<R>(&mut self, index: usize, work: impl FnOnce(&mut UnluminousApp) -> R) -> R {
        let tab = &mut self.tabs[index];
        let _named = control::running_for(tab.app.address_for_children(), tab.key);
        work(&mut tab.app)
    }

    /// Tell every tab what the tabs are, before any of them runs.
    fn tell_every_tab(&mut self) {
        let view = ProjectTabs {
            in_a_window: true,
            tabs: self
                .tabs
                .iter()
                .map(|tab| ProjectTab {
                    key: tab.key,
                    name: name_of(tab.app.tree.root()),
                    folder: tab.app.tree.root().to_path_buf(),
                    port: tab.app.control.as_ref().map(|server| server.port()),
                })
                .collect(),
            showing: self.showing,
        };
        for tab in &mut self.tabs {
            tab.app.project_tabs = view.clone();
        }
        let open: Vec<u32> = self.tabs.iter().map(|tab| tab.key).collect();
        control::set_open_tabs(&open);
    }

    /// Open `folder` in a new tab and show it, or show the tab already on it. Answers the tab's index.
    ///
    /// A folder that is already open in a tab is shown rather than opened again, for the reason a file already
    /// open is shown rather than opened twice: two tabs on one folder would be two writers of one `.unluminous`.
    pub fn open(&mut self, folder: &Path, ctx: &egui::Context) -> usize {
        let folder = unluminous_terminal::paths::plain(folder);
        if let Some(index) = self.tabs.iter().position(|tab| tab.app.tree.root() == folder) {
            self.show(index);
            return index;
        }
        let key = self.next_key;
        self.next_key += 1;
        // Written first, so the new tab reads the settings the showing tab has rather than older ones.
        let outgoing = self.showing;
        if self.tabs[outgoing].app.store.is_some() {
            self.tabs[outgoing].app.write_settings();
        }
        let app = {
            let _named = control::running_for(None, key);
            self.tabs[outgoing].app.sibling_on(&folder, ctx, key)
        };
        self.tabs.push(Tab { key, app });
        let index = self.tabs.len() - 1;
        // A placeholder tab waiting for a project goes once a real one is open beside it. `task-2305`.
        self.tell_every_tab();
        self.show(index);
        let waiting: Vec<usize> = (0..self.tabs.len())
            .filter(|at| *at != self.showing && self.tabs[*at].app.waiting_for_a_project)
            .collect();
        for at in waiting.into_iter().rev() {
            self.close(at);
        }
        self.showing
    }

    /// Show tab `index`.
    pub fn show(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if index != self.showing {
            let outgoing = self.showing;
            self.tabs[outgoing].app.put_the_project_tab_away();
            let (first, second) = match outgoing < index {
                true => {
                    let (low, high) = self.tabs.split_at_mut(index);
                    (&mut high[0].app, &mut low[outgoing].app)
                }
                false => {
                    let (low, high) = self.tabs.split_at_mut(outgoing);
                    (&mut low[index].app, &mut high[0].app)
                }
            };
            first.take_the_screen_from(second);
            self.showing = index;
        } else {
            self.tabs[index].app.mark_the_showing_tab_in_the_session();
        }
        self.tell_every_tab();
        if let Some(ctx) = &self.context {
            ctx.request_repaint();
        }
    }

    /// Close tab `index`, writing down what was open in it and stopping what it started. The only tab is never
    /// closed, and a tab with a file that could not be saved stays open and says why.
    pub fn close(&mut self, index: usize) -> bool {
        if self.tabs.len() < 2 || index >= self.tabs.len() {
            return false;
        }
        if !self.run_on(index, |app| app.may_the_window_close()) {
            return false;
        }
        if index == self.showing {
            let next = if index + 1 < self.tabs.len() { index + 1 } else { index - 1 };
            self.show(next);
        }
        let showing_folder = self.tabs[self.showing].app.tree.root().to_path_buf();
        let mut tab = self.tabs.remove(index);
        {
            let _named = control::running_for(tab.app.address_for_children(), tab.key);
            tab.app.put_the_project_tab_away();
            tab.app.answer_what_is_held_before_closing();
            tab.app.closing = true;
            // The session line is this tab's alone, and the window keeps its own.
            tab.app.closed_by_hand = false;
            eframe::App::on_exit(&mut tab.app);
            if crate::services::store::this_window_joins_the_session() {
                if let Some(store) = &tab.app.store {
                    store.forget_open_tab(tab.app.tree.root(), std::process::id());
                }
            }
        }
        drop(tab);
        self.showing =
            self.tabs.iter().position(|tab| tab.app.tree.root() == showing_folder).unwrap_or(0);
        self.tell_every_tab();
        if let Some(ctx) = &self.context {
            ctx.request_repaint();
        }
        true
    }

    /// Act on what the tabs asked for this frame.
    fn act_on_what_the_tabs_asked(&mut self, ctx: &egui::Context) {
        let mut asked: Vec<Ask> = Vec::new();
        for tab in &mut self.tabs {
            asked.append(&mut tab.app.project_tab_asked);
        }
        for ask in asked {
            match ask {
                Ask::Open(folder) => {
                    if !folder.is_dir() {
                        self.app_mut().message = Some(format!(
                            "{} is not a folder, so it cannot be a project tab.",
                            folder.display()
                        ));
                        continue;
                    }
                    self.open(&folder, ctx);
                }
                Ask::Show(index) => self.show(index),
                Ask::Close(index) => {
                    self.close(index);
                }
                Ask::CloseOthers(index) => {
                    let keep = self.tabs.get(index).map(|tab| tab.key);
                    let others: Vec<u32> = self
                        .tabs
                        .iter()
                        .map(|tab| tab.key)
                        .filter(|key| Some(*key) != keep)
                        .collect();
                    for key in others {
                        if let Some(at) = self.tabs.iter().position(|tab| tab.key == key) {
                            self.close(at);
                        }
                    }
                }
                Ask::ToNewWindow(index) => {
                    let Some(folder) =
                        self.tabs.get(index).map(|tab| tab.app.tree.root().to_path_buf())
                    else {
                        continue;
                    };
                    if self.close(index)
                        && crate::services::launcher::open_window(&folder).is_none()
                    {
                        self.open(&folder, ctx);
                    }
                }
                Ask::Step(step) => {
                    let count = self.tabs.len() as isize;
                    let next = (self.showing as isize + step).rem_euclid(count);
                    self.show(next as usize);
                }
                Ask::CloseWindow => self.close_every_tab(ctx),
            }
        }
    }

    /// Close the window and every tab in it, the way the close button does: every modified file is written first,
    /// and the window stays when one of them could not be.
    fn close_every_tab(&mut self, ctx: &egui::Context) {
        for index in 0..self.tabs.len() {
            if !self.run_on(index, |app| app.may_the_window_close()) {
                self.show(index);
                return;
            }
        }
        for index in 0..self.tabs.len() {
            self.run_on(index, |app| {
                app.closing = true;
                app.closed_by_hand = true;
                app.write_settings();
                app.remember_the_project(None);
            });
        }
        self.closing = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Draw the window: every tab that is not showing keeps working, and the one showing is drawn. Split out of
    /// the `eframe::App` implementation so a test can drive it with no real window, as `UnluminousApp::ui` is.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.context = Some(ctx.clone());
        self.tell_every_tab();
        for index in 0..self.tabs.len() {
            if index != self.showing {
                self.run_on(index, |app| app.keep_working_while_hidden(&ctx));
            }
        }
        let showing = self.showing;
        let key = self.tabs[showing].key;
        self.run_on(showing, |app| match key {
            // The first tab is drawn with the ids it was always drawn with, so a window of one project is
            // the window it was. Every other tab's widgets keep their state under ids of their own.
            0 => app.ui(ui),
            _ => {
                ui.push_id(("project-tab", key), |ui| app.ui(ui));
            }
        });
        self.act_on_what_the_tabs_asked(&ctx);
        // A tab asked to close the window, by the cross or by `Quit`, with nobody else to ask.
        if self.tabs.iter().any(|tab| tab.app.closing) {
            self.closing = true;
        }
    }

    /// Run one command line against tab `index`, the way `unluminous-cli` would, for a test.
    pub fn run_command_line_on(
        &mut self,
        index: usize,
        line: &str,
        ctx: &egui::Context,
    ) -> Option<unluminous_cli::protocol::Reply> {
        self.tell_every_tab();
        let reply = self.run_on(index, |app| app.run_command_line(line, ctx));
        self.act_on_what_the_tabs_asked(ctx);
        reply
    }

    /// Run one command line against the tab that is showing.
    pub fn run_command_line(
        &mut self,
        line: &str,
        ctx: &egui::Context,
    ) -> Option<unluminous_cli::protocol::Reply> {
        let showing = self.showing;
        self.run_command_line_on(showing, line, ctx)
    }

    /// Open the tabs a window was asked to start with, after its first, and show tab `showing`. `main` calls it
    /// with the tabs a restored window had and the ones named with `--tab`.
    pub fn open_the_other_tabs(
        &mut self,
        folders: &[PathBuf],
        showing: usize,
        ctx: &egui::Context,
    ) {
        let first = self.showing;
        for folder in folders {
            if folder.is_dir() {
                self.open(folder, ctx);
            }
        }
        let wanted = showing.min(self.tabs.len().saturating_sub(1));
        self.show(if folders.is_empty() { first } else { wanted });
    }
}

impl eframe::App for ProjectWindow {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let showing = self.showing;
        self.run_on(showing, |app| eframe::App::raw_input_hook(app, ctx, raw_input));
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A window the operating system is not drawing gets no UI pass at all, so every tab keeps working here.
        if ctx.input(|input| input.viewport().visible()) == Some(false) {
            for index in 0..self.tabs.len() {
                self.run_on(index, |app| app.keep_working_while_hidden(ctx));
            }
            self.act_on_what_the_tabs_asked(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let showing = self.showing;
        self.run_on(showing, |app| app.before_the_frame(ui, frame));
        ProjectWindow::ui(self, ui);
        let showing = self.showing;
        self.run_on(showing, |app| app.after_the_frame(frame));
    }

    fn on_exit(&mut self) {
        let several = self.tabs.len() > 1;
        let pid = std::process::id();
        let by_hand = self.tabs.iter().any(|tab| tab.app.closed_by_hand);
        let folders = self.folders();
        let showing_folder = folders.get(self.showing).cloned();
        let store = self.tabs.iter().find_map(|tab| tab.app.store.clone());
        for index in 0..self.tabs.len() {
            if several {
                // The session is the window's to write, below, rather than each tab's.
                self.tabs[index].app.closed_by_hand = false;
            }
            self.run_on(index, eframe::App::on_exit);
        }
        if !several || !crate::services::store::this_window_joins_the_session() {
            return;
        }
        let Some(store) = store else { return };
        if by_hand {
            // Every line of this window goes when another window is running, and every tab is kept when none is.
            if let Some(first) = folders.first() {
                store.forget_open_window(first, pid, &crate::app::window_is_running);
            }
        }
        if let Some(folder) = showing_folder {
            store.mark_the_showing_tab(&folder, pid);
        }
    }
}
