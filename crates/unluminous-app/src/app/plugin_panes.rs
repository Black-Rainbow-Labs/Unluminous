//! The window's side of the UI plugins: which providers are open, and which of their panes are showing.
//!
//! `services::plugins` says what a manifest contributed and `services::plugin_ui` says what a provider
//! is. This is the third piece: the window has to hold one provider per plugin that draws, know which
//! slot each contributed pane is in, and build the provider the first time somebody presses its button
//! rather than at startup.
//!
//! ## Nothing is built until it is asked for
//!
//! [`PluginUi::opened`] is the only thing that builds a provider, and it is called from the rail button,
//! the tab and the Settings page. A plugin nobody opens costs one row in a list. That is the reference editor's own
//! arrangement, whose documented reason is the same: a tool window a person never clicks loads and runs
//! no plugin code.
//!
//! ## A pane's slot is the window's business
//!
//! `dock::Panel::Plugin(0)` is the first contributed pane. Which pane that is comes from
//! `plugins::Surfaces`, which is worked out from the manifests, so it changes when a plugin is switched
//! on or off. The settings file records a pane's side against its own `<plugin id>/<pane id>` rather
//! than against its slot, so installing a second plugin does not move the first one's pane.

use crate::app::dock::{Panel, PLUGIN_PANES};
use crate::services::plugin_ui::{self, Context, UiProvider};
use crate::services::plugins::{Plugins, Surfaces};

use std::path::{Path, PathBuf};

use egui::Rect;

use crate::app::{dock, files};
use crate::app::{Focus, PluginHighlighter, UnluminousApp, PLUGIN_TICK};

/// One provider, and whether it has been opened.
struct Loaded {
    /// The `plugin.id` of the plugin whose manifest named it.
    plugin: String,
    provider: Box<dyn UiProvider>,
    /// What went wrong when it was opened, if it did. Drawn in the pane rather than thrown away.
    problem: Option<String>,
}

/// Every plugin that draws, and what is showing.
#[derive(Default)]
pub struct PluginUi {
    loaded: Vec<Loaded>,
    /// The contributed panes, tabs, menus and pages, in the order the plugins are listed.
    surfaces: Surfaces,
    /// The panes that are showing, by their own `<plugin id>/<pane id>`.
    ///
    /// **By name rather than by slot**, for the reason their sides and sizes are recorded by name: which
    /// slot a pane is in comes from the manifests and moves when a plugin is switched on or off. Keyed by
    /// slot, switching off the first of two plugins would leave the second one's pane showing or hidden
    /// according to what the first one was doing.
    showing: Vec<String>,
    /// The folder each plugin keeps its own files in, worked out once when the settings folder is known.
    settings_folder: Option<PathBuf>,
    /// The project this window has open, which a provider is told about when it is opened.
    project: Option<PathBuf>,
    /// The file showing, told to a provider when it opens. See `plugin_ui::Context::showing`.
    showing_file: Option<PathBuf>,
    /// The folders this machine has had open, newest first, told to a provider the same way.
    recent_projects: Vec<PathBuf>,
    /// How a provider asks the window to draw again from another thread.
    wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for PluginUi {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PluginUi")
            .field(
                "loaded",
                &self.loaded.iter().map(|one| one.plugin.as_str()).collect::<Vec<&str>>(),
            )
            .field("panes", &self.surfaces.panes.len())
            .field("showing", &self.showing)
            .finish()
    }
}

impl PluginUi {
    /// Read what the enabled plugins contribute, and drop any provider whose plugin has gone.
    ///
    /// Called when the plugins are loaded and again whenever one is switched on or off, which is what
    /// makes a contribution appear and disappear in the same frame. A provider whose plugin was switched
    /// off is closed, so it drops what it held — an open database file, in Agent-Tasks' case.
    pub fn refresh(&mut self, plugins: &Plugins) {
        self.surfaces = plugins.surfaces();
        let still_contributing = self.surfaces.plugins();
        // A provider whose plugin has gone is closed, so it drops what it held — an open board file, in
        // Agent-Tasks' case. Closed before it is dropped rather than left to `Drop`, because closing is a
        // thing the provider does and dropping is a thing that happens to it.
        for one in &mut self.loaded {
            if !still_contributing.contains(&one.plugin) {
                one.provider.close();
            }
        }
        self.loaded.retain(|one| still_contributing.contains(&one.plugin));
        // A pane whose plugin has gone cannot still be showing, and neither can one whose condition has
        // stopped being met — a project closing is the case that matters. Held by name, so this is the one
        // place a pane stops showing and a slot moving underneath it changes nothing.
        let still_shown: Vec<String> = (0..self.pane_count())
            .filter(|slot| self.applies(*slot))
            .filter_map(|slot| self.pane_key(slot))
            .collect();
        self.showing.retain(|open| still_shown.contains(open));
    }

    pub fn surfaces(&self) -> &Surfaces {
        &self.surfaces
    }

    /// Where the plugins keep their own files, which is `<settings folder>/plugins`.
    pub fn set_settings_folder(&mut self, folder: PathBuf) {
        self.settings_folder = Some(folder);
    }

    pub fn set_project(&mut self, project: Option<PathBuf>) {
        self.project = project;
    }

    /// Which file is showing, so a provider opened later is told at once rather than at the next
    /// change. See `plugin_ui::Context::showing`.
    pub fn set_showing(&mut self, showing: Option<PathBuf>) {
        self.showing_file = showing;
    }

    /// The recent projects, for a provider that offers them as choices. See `plugin_ui::Context`.
    pub fn set_recent_projects(&mut self, projects: Vec<PathBuf>) {
        self.recent_projects = projects;
    }

    /// How a provider asks for a frame from another thread, handed on when one is opened.
    pub fn set_waker(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.wake = Some(wake);
    }

    /// How many panes are contributed, which is what `dock::Panel::all` is asked for.
    pub fn pane_count(&self) -> usize {
        self.surfaces.panes.len().min(PLUGIN_PANES)
    }

    /// Whether the pane in `slot` applies just now, which is `pane.applies` in its manifest.
    ///
    /// **A control that cannot apply is absent**, which is Unluminous's rule everywhere: the `F` button is not
    /// drawn for a `.rs` file and the three code navigation entries are not on the Edit menu for a
    /// stylesheet. So a pane whose condition is not met has no button in the rail and cannot be shown,
    /// rather than having a button that reports a refusal.
    ///
    /// Two conditions, checked against `plugins::PANE_CONDITIONS` when the manifest was read, so an
    /// unknown one was refused there and cannot reach here.
    pub fn applies(&self, slot: usize) -> bool {
        match self.pane(slot).map(|pane| pane.applies.as_str()) {
            Some("in_project") => self.project.is_some(),
            // `always`, and anything the reader let through, which is only `always`.
            Some(_) => true,
            None => false,
        }
    }

    /// The `<plugin id>/<pane id>` of the pane in `slot`.
    pub fn pane_key(&self, slot: usize) -> Option<String> {
        self.surfaces.panes.get(slot).map(|surface| surface.key(&surface.what.id))
    }

    /// Every contributed pane's name, in slot order, for the settings file.
    pub fn pane_keys(&self) -> Vec<String> {
        (0..self.pane_count()).filter_map(|slot| self.pane_key(slot)).collect()
    }

    /// The slot the pane named `key` is in.
    pub fn slot_of(&self, key: &str) -> Option<usize> {
        (0..self.pane_count()).find(|slot| self.pane_key(*slot).as_deref() == Some(key))
    }

    /// What the manifest said about the pane in `slot`.
    pub fn pane(&self, slot: usize) -> Option<&crate::services::plugins::PaneContribution> {
        self.surfaces.panes.get(slot).map(|surface| &surface.what)
    }

    pub fn is_visible(&self, slot: usize) -> bool {
        self.pane_key(slot).is_some_and(|key| self.showing.contains(&key))
    }

    /// Which contributed panes are showing, for `dock::regions`, in slot order.
    pub fn visible(&self) -> [bool; PLUGIN_PANES] {
        let mut visible = [false; PLUGIN_PANES];
        for (slot, showing) in visible.iter_mut().enumerate() {
            *showing = self.is_visible(slot);
        }
        visible
    }

    /// Show or hide the pane in `slot`, building its provider the first time it is shown.
    pub fn set_visible(&mut self, slot: usize, showing: bool) -> Option<String> {
        if slot >= self.pane_count() {
            return Some(format!("there is no plugin pane in slot {slot}"));
        }
        if showing {
            if !self.applies(slot) {
                return Some(format!(
                    "{} asks for a project to be open, and this window has none",
                    self.pane(slot).map(|pane| pane.label.clone()).unwrap_or_default()
                ));
            }
            let problem = self.open_slot(slot);
            if problem.is_some() {
                return problem;
            }
        }
        let Some(key) = self.pane_key(slot) else {
            return Some(format!("there is no plugin pane in slot {slot}"));
        };
        self.showing.retain(|open| *open != key);
        if showing {
            self.showing.push(key);
        }
        None
    }

    /// The provider behind the pane in `slot`, built if it has not been yet.
    fn open_slot(&mut self, slot: usize) -> Option<String> {
        let plugin = self.surfaces.panes.get(slot)?.plugin.clone();
        let provider = self.surfaces.panes.get(slot)?.provider.clone();
        self.opened(&plugin, &provider).err()
    }

    /// The provider named `provider` for the plugin `plugin`, opened.
    ///
    /// The one place a provider is built and the one place `open` is called, so "opened" cannot mean two
    /// different things in two places, and a provider that failed to open keeps its reason.
    pub fn opened(
        &mut self,
        plugin: &str,
        provider: &str,
    ) -> Result<&mut (dyn UiProvider + 'static), String> {
        if let Some(index) = self.loaded.iter().position(|one| one.plugin == plugin) {
            if let Some(problem) = self.loaded[index].problem.clone() {
                return Err(problem);
            }
            return Ok(self.loaded[index].provider.as_mut());
        }
        let mut built = plugin_ui::provider(provider)
            .ok_or_else(|| format!("this version of Unluminous has no `{provider}` provider"))?;
        let context = self.context_for(plugin);
        let problem = built.open(&context).err();
        self.loaded.push(Loaded {
            plugin: plugin.to_owned(),
            provider: built,
            problem: problem.clone(),
        });
        match problem {
            Some(problem) => Err(problem),
            None => Ok(self.loaded.last_mut().expect("just pushed").provider.as_mut()),
        }
    }

    /// What a provider for `plugin` is opened with: the project, the file showing, the recent folders, its
    /// own folder under the settings folder, and the waker.
    ///
    /// **A function rather than a literal inside `opened`**, because the canvas opens one too: a chat node
    /// holds an `AgentChat` of its own — see `services::space::node::Kind::Chat` — and it has to be opened
    /// with the same five things, out of the same folder, or the endpoints somebody configured in
    /// `Settings -> Agent-Chat` would not reach it.
    pub fn context_for(&self, plugin: &str) -> Context {
        Context {
            project: self.project.clone(),
            showing: self.showing_file.clone(),
            recent_projects: self.recent_projects.clone(),
            folder: self.settings_folder.as_ref().map(|folder| folder.join(plugin)),
            wake: self.wake.clone(),
        }
    }

    /// The provider for `plugin`, if it has been opened. Nothing is built here.
    pub fn provider(&mut self, plugin: &str) -> Option<&mut (dyn UiProvider + 'static)> {
        let one =
            self.loaded.iter_mut().find(|one| one.plugin == plugin && one.problem.is_none())?;
        Some(one.provider.as_mut())
    }

    /// The provider for `plugin` without changing it, for reading its view.
    pub fn view_of(&self, plugin: &str) -> Option<serde_json::Value> {
        self.loaded
            .iter()
            .find(|one| one.plugin == plugin && one.problem.is_none())
            .map(|one| one.provider.view())
    }

    /// Why this plugin's pane is empty, if it is.
    pub fn problem_with(&self, plugin: &str) -> Option<&str> {
        self.loaded.iter().find(|one| one.plugin == plugin).and_then(|one| one.problem.as_deref())
    }

    /// True when this plugin's provider has been built and opened.
    pub fn is_open(&self, plugin: &str) -> bool {
        self.loaded.iter().any(|one| one.plugin == plugin && one.provider.is_open())
    }

    /// The plugin a slot's pane belongs to.
    pub fn plugin_of(&self, slot: usize) -> Option<String> {
        self.surfaces.panes.get(slot).map(|surface| surface.plugin.clone())
    }

    /// Run one command against a plugin's provider, opening it first if it has not been opened.
    ///
    /// The one path a plugin command goes down, whichever of the three ways in asked for it: a menu
    /// entry, a button inside a pane, or `unluminous-cli plugin run`.
    pub fn run(
        &mut self,
        plugin: &str,
        command: &str,
        arguments: &[String],
    ) -> Result<plugin_ui::Answer, String> {
        let provider = self.surfaces.provider_of(plugin).ok_or_else(|| {
            format!("`{plugin}` is not a plugin that draws, or it is switched off")
        })?;
        let opened = self.opened(plugin, &provider)?;
        opened.command(command, arguments)
    }

    /// Close every provider, which is what happens when the window closes or the project changes.
    pub fn close(&mut self) {
        for one in &mut self.loaded {
            one.provider.close();
        }
        self.loaded.clear();
        self.showing.clear();
    }
}

impl Panel {
    /// The pane a contributed slot holds, as a `Panel`, when there is one.
    pub fn plugin_pane(slot: usize) -> Option<Panel> {
        (slot < PLUGIN_PANES).then_some(Panel::Plugin(slot as u8))
    }
}

impl UnluminousApp {
    /// Draw every contributed pane that is showing, and gather what they asked for.
    ///
    /// One function rather than a call per plugin, because the rectangles come from one place —
    /// `dock::regions`, which laid out every panel including these — and because a provider is borrowed
    /// while it draws, so what it asked for has to be acted on after the borrow has ended.
    pub(crate) fn show_the_plugin_panes(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Vec<(String, crate::services::plugin_ui::Request)> {
        let mut asked = Vec::new();
        let mut closing: Option<String> = None;
        // What each header reported, acted on after the loop: `note_a_panel_grab` changes the window and the
        // loop is holding a borrow of the renderer for the whole of it.
        let mut grabs: Vec<(dock::Panel, crate::components::dock::Grab)> = Vec::new();
        // What each pane recorded, rasterised after the loop for the same reason the grabs are acted on
        // after it: the canvases are `self`'s and a provider is drawing with a borrow of `self` until the
        // iteration ends.
        let mut chromes: Vec<(
            egui::Id,
            Rect,
            egui::Painter,
            egui::layers::ShapeIdx,
            crate::services::vello_canvas::Chrome,
        )> = Vec::new();
        for slot_number in 0..self.plugin_ui.pane_count() {
            let slot = slot_number;
            if !self.plugin_ui.is_visible(slot) {
                continue;
            }
            let rect = self.panel_rects.of(dock::Panel::Plugin(slot as u8));
            if rect.width() < 1.0 || rect.height() < 1.0 {
                continue;
            }
            let Some(plugin) = self.plugin_ui.plugin_of(slot) else {
                continue;
            };
            let label =
                self.plugin_ui.pane(slot).map(|pane| pane.label.clone()).unwrap_or_default();
            let problem = self.plugin_ui.problem_with(&plugin).map(str::to_owned);
            let panel = dock::Panel::Plugin(slot as u8);
            // The header, which is what every other panel in Unluminous has and what the board pane did not: it
            // names the panel, it closes it, and it is the handle the panel is dragged to another edge by.
            // `components::dock::handle` is the one function that makes a rectangle a drag handle, and the
            // four drop bands, the strong rectangle and the `Move to` menu are all `app::dock`'s already.
            let header = Rect::from_min_size(
                rect.min,
                egui::Vec2::new(rect.width(), crate::components::agent_tasks::PANE_HEADER),
            );
            let count = self
                .plugin_ui
                .view_of(&plugin)
                .and_then(|view| view["total"].as_u64())
                .map(|total| total.to_string());
            let outcome = {
                let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(header));
                crate::components::agent_tasks::pane_header(
                    &mut header_ui,
                    header,
                    &label,
                    count.as_deref(),
                    panel,
                    self.settings.opacity,
                )
            };
            grabs.push((panel, outcome.grab));
            if outcome.closed {
                closing = self.plugin_ui.pane_key(slot);
            }
            let body = Rect::from_min_max(egui::Pos2::new(rect.min.x, header.max.y), rect.max);
            let mut pane_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            pane_ui.set_clip_rect(body);
            // The pane's ground first, then the slot the decoration goes in, then the provider's widgets.
            // The ground is the window's rather than the provider's, and it has to be: a provider that
            // painted its own would add it to the painter *after* this slot and wash the decoration out —
            // which is what it did, and it was invisible only because the ground carries the window's own
            // opacity and let some of the decoration through.
            {
                let ground = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer);
                pane_ui.painter().rect_filled(body, 0, ground.ground(ground.palette.editor));
            }
            // Filled in by `paint_the_chrome` after the loop, which is the earliest moment `self` is not
            // borrowed by the `Look` the providers are drawing with.
            let shape = pane_ui.painter().add(egui::Shape::Noop);
            let chrome = self.chrome_for(&plugin);
            // The pane's own zoom, which is `task-1771`'s: every pane is zoomable, and a pane a plugin
            // contributed has no font size of its own to walk, so it has a multiplier instead.
            // **The colouring a plugin draws a fenced block or a SQL console with.** `unluminous-core` holds
            // no plugin registry, so it asks through `CodeHighlighter` and the window answers with the
            // same two calls `colour_the_file` makes for a source file — which is what makes a fence of
            // Rust in an answer look like a `.rs` file, and a statement in a query console look like a
            // `.sql` one. `Look::colouring_with` had no caller at all until now, so both were drawing
            // in one flat colour: the seam was built and never plugged in.
            let highlighter = PluginHighlighter { plugins: &self.plugins };
            let look = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer)
                .zoomed_by(self.panes.zoom_of(panel))
                .holding_the_keyboard(matches!(self.focus, Focus::Plugin))
                .colouring_with(&highlighter)
                .drawing_into(&chrome);
            let rect = body;
            match problem {
                Some(problem) => {
                    // The provider could not be opened. Its reason is drawn where its board would be,
                    // because a blank pane is a pane somebody reports as a bug in Unluminous.
                    pane_ui.painter().rect_filled(rect, 0, look.ground(look.palette.panel));
                    let galley = pane_ui.painter().layout(
                        format!("{header} could not be opened.\n\n{problem}"),
                        egui::FontId::proportional(look.font_size),
                        look.palette.text_dim,
                        rect.width() - 24.0,
                    );
                    pane_ui.painter().galley(
                        rect.min + egui::Vec2::splat(12.0),
                        galley,
                        look.palette.text_dim,
                    );
                }
                None => {
                    if let Some(provider) = self.plugin_ui.provider(&plugin) {
                        let wanted = provider.pane(&mut pane_ui, &look);
                        asked.extend(wanted.into_iter().map(|request| (plugin.clone(), request)));
                    }
                }
            }
            drop(look);
            chromes.push((
                egui::Id::new(("plugin-pane", slot_number)),
                body,
                pane_ui.painter().clone(),
                shape,
                chrome,
            ));
        }
        for (id, body, painter, shape, chrome) in &chromes {
            let items = chrome.take();
            if items.is_empty() {
                continue;
            }
            if let Some((texture, drawn)) = self.canvases.texture_for(ui.ctx(), *id, *body, &items)
            {
                let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
                painter.set(*shape, egui::Shape::image(texture, drawn, uv, egui::Color32::WHITE));
            }
        }
        // After the loop, because closing a pane changes what the loop is walking and a grab changes the dock.
        for (panel, grab) in grabs {
            self.note_a_panel_grab(panel, grab);
        }
        // And the zoom, for the same reason: it changes the settings, and the loop is holding a borrow of
        // the renderer and of every provider for the whole of itself. `task-1771`.
        for slot in 0..self.plugin_ui.pane_count() {
            if !self.plugin_ui.is_visible(slot) {
                continue;
            }
            let panel = dock::Panel::Plugin(slot as u8);
            let rect = self.panel_rects.of(panel);
            self.zoom_over_a_panel(ui, panel, rect);
        }
        if let Some(key) = closing {
            self.show_the_plugin_pane(&key, false);
        }
        asked
    }

    /// Draw whatever modal each open plugin has, and act on what it asked for.
    pub(crate) fn show_the_plugin_modals(&mut self, ui: &mut egui::Ui) {
        let mut asked: Vec<(String, crate::services::plugin_ui::Request)> = Vec::new();
        // What each modal recorded, rasterised after the loop for the reason a pane's is: the canvases are
        // `self`'s and a provider is drawing with a borrow of `self` until the iteration ends.
        let mut chromes: Vec<(
            crate::services::plugin_ui::ChromeSlot,
            crate::services::vello_canvas::Chrome,
        )> = Vec::new();
        for plugin in self.plugin_ui.surfaces().plugins() {
            if !self.plugin_ui.is_open(&plugin) {
                continue;
            }
            // **A modal gets the same depth its pane does.** `task-1771` reports the ticket as "plain, not
            // much effort", and the reason it looked plain beside the board it opens from is that the board
            // is drawn on a decoration canvas and the modal was not: every well, every field and every
            // button on it fell back to the flat form.
            let chrome = self.chrome_for(&plugin);
            // **The colouring a plugin draws a fenced block or a SQL console with.** `unluminous-core` holds
            // no plugin registry, so it asks through `CodeHighlighter` and the window answers with the
            // same two calls `colour_the_file` makes for a source file — which is what makes a fence of
            // Rust in an answer look like a `.rs` file, and a statement in a query console look like a
            // `.sql` one. `Look::colouring_with` had no caller at all until now, so both were drawing
            // in one flat colour: the seam was built and never plugged in.
            let highlighter = PluginHighlighter { plugins: &self.plugins };
            let look = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer)
                .holding_the_keyboard(matches!(self.focus, Focus::Plugin))
                .colouring_with(&highlighter)
                .drawing_into(&chrome);
            if let Some(provider) = self.plugin_ui.provider(&plugin) {
                let (wanted, _closed) = provider.modal(ui.ctx(), &look);
                asked.extend(wanted.into_iter().map(|request| (plugin.clone(), request)));
                if let Some(slot) = provider.take_the_modals_canvas() {
                    drop(look);
                    chromes.push((slot, chrome));
                    continue;
                }
            }
            drop(look);
        }
        for (slot, chrome) in &chromes {
            let items = chrome.take();
            if items.is_empty() {
                continue;
            }
            if let Some((texture, drawn)) =
                self.canvases.texture_for(ui.ctx(), slot.id, slot.area, &items)
            {
                let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
                slot.painter
                    .set(slot.shape, egui::Shape::image(texture, drawn, uv, egui::Color32::WHITE));
            }
        }
        for (plugin, request) in asked {
            self.act_on_a_plugin_request(&plugin, request, ui.ctx());
        }
    }

    /// Act on one thing a provider asked the window for.
    ///
    /// A provider does none of these itself, so there is one owner of the tab strip, one owner of the
    /// status bar and one place a file is opened.
    ///
    /// `plugin` is the plugin that asked. It has to be carried, because two plugins can both contribute a
    /// pane and a request that acted on the first one would show the wrong pane the day a second plugin is
    /// installed.
    pub(crate) fn act_on_a_plugin_request(
        &mut self,
        plugin: &str,
        request: crate::services::plugin_ui::Request,
        ctx: &egui::Context,
    ) {
        use crate::services::plugin_ui::Request;
        match request {
            // The reason is already in the status bar; a plugin that asked for a file has no
            // reply channel for one, so there is nothing further to do with it here.
            Request::OpenFile(path) => {
                let _ = self.open_path_in_tab(&path, true);
            }
            // **The one place a command becomes a change is `run_cli`**, and this is a plugin
            // reaching it. The answer goes back to the provider that asked, by the id it asked with,
            // because more than one may be outstanding and they do not finish in order.
            // The window reads the clipboard, because the window owns it. `arboard` is the same
            // handle `Request::Copy` writes through.
            Request::ClipboardPicture { id } => {
                let answer = crate::services::picture::from_the_clipboard();
                if let Some(provider) = self.plugin_ui.provider(plugin) {
                    provider.answered(&id, answer);
                }
            }
            Request::RunCommand { id, command, arguments } => {
                let request = unluminous_cli::protocol::Request::new("", &command, arguments);
                let answer = self.run_cli_for_a_plugin(&request, ctx);
                if let Some(provider) = self.plugin_ui.provider(plugin) {
                    provider.answered(&id, answer);
                }
            }
            Request::Message(said) => self.message = Some(said),
            // The status bar as well as the toast, so a notice is still in the one place a person looks
            // for the last thing that happened — and so a screenshot of the status bar keeps saying what
            // it said before this existed.
            Request::Notice { text, kind } => {
                self.message = Some(text.clone());
                self.toasts.say(text, kind);
            }
            Request::ShowTab => {
                let key = self
                    .plugin_ui
                    .surfaces()
                    .tabs
                    .iter()
                    .find(|surface| surface.plugin == plugin)
                    .map(|surface| surface.key(&surface.what.id));
                if let Some(key) = key {
                    self.open_the_plugin_tab(&key);
                }
            }
            Request::ShowPane(showing) => {
                let key = self
                    .plugin_ui
                    .surfaces()
                    .panes
                    .iter()
                    .find(|surface| surface.plugin == plugin)
                    .map(|surface| surface.key(&surface.what.id));
                if let Some(key) = key {
                    self.show_the_plugin_pane(&key, showing);
                }
            }
            // A provider with a terminal in it needs the window to keep drawing while that terminal
            // prints, which is what the terminal tile already asks for through its waker. It is a flag
            // rather than a call, because the context is not at hand here and the frame asks for it once
            // at the end whatever number of providers wanted it.
            Request::Repaint => self.plugin_wants_a_repaint = true,
            // The window owns the one handle to the platform's clipboard, which is why a provider asks.
            Request::Copy(text) => self.plugin_wants_copied = Some(text),
            // The keyboard, through the one value that owns it. `Focus::Plugin` is a fifth thing that can hold the
            // keys, beside the editing area, the explorer, the terminal tile and the run tile, and naming it is
            // what stops a key press reaching two of them.
            Request::TakeTheKeyboard(taking) => {
                self.focus = match taking {
                    true => Focus::Plugin,
                    false => Focus::Editor,
                };
                self.plugin_with_the_keyboard = taking.then(|| plugin.to_owned());
                if let Some(provider) = self.plugin_ui.provider(plugin) {
                    provider.keyboard(taking);
                }
            }
            // `services::launcher` is the one place Unluminous asks the operating system to open something.
            Request::Reveal(path) => {
                if !crate::services::launcher::reveal(&path) {
                    self.message = Some(format!("{} could not be shown", path.display()));
                }
            }
        }
    }

    /// Show or hide the pane a plugin contributed, building its provider the first time it is shown.
    ///
    /// A pane in a strip is a tile, so it puts the other tiles on that side away, which is the rule the
    /// terminal, the run tile and the debug tile already keep. What went wrong, if anything did, is said
    /// in the status bar rather than swallowed, which is what every honest miss in Unluminous does.
    pub fn show_the_plugin_pane(&mut self, pane: &str, showing: bool) {
        self.leave_the_maximised_pane();
        let Some(slot) = self.plugin_ui.slot_of(pane) else {
            self.message = Some(format!("there is no `{pane}` pane"));
            return;
        };
        if let Some(problem) = self.plugin_ui.set_visible(slot, showing) {
            self.message = Some(problem);
            return;
        }
        if showing {
            let panel = dock::Panel::Plugin(slot as u8);
            // Only a pane that is a tile competes for a strip. One in the top group is a list, like the
            // explorer, and the explorer has never put the terminal away.
            let tiles = self.plugin_panes_that_are_tiles();
            if panel.is_a_tile_given(&tiles) && self.panes.dock.side_of(panel).is_a_strip() {
                self.put_the_other_tiles_away(panel);
            }
        } else if self.focus == Focus::Plugin
            && !self.plugin_ui.visible().iter().any(|showing| *showing)
        {
            // **The keyboard comes back to the editing area when the last plugin pane goes.** Clicking the board
            // gives the keys to the plugin, and hiding the pane used to leave them there: somebody typed at the
            // caret they could see and nothing appeared until they clicked the editor. `show_the_terminal_tile`
            // already does this and for the same reason. Only when the last one goes, because two plugin panes
            // can be open and the keys still belong to the one that is left.
            self.focus = Focus::Editor;
            if let Some(provider) = self.plugin_ui.provider(pane.split('/').next().unwrap_or(pane))
            {
                provider.keyboard(false);
            }
        }
        self.unsaved_settings = true;
    }

    /// Show a plugin's tab, or close it when it is the one showing.
    ///
    /// **What a rail button does**, and `task-1848` is the report: "I can't untoggle it to hide it. It
    /// should always open/close." `open_the_plugin_tab` only ever opened — a tab already open was *shown*,
    /// and no path closed one — so the button could be pressed once and then did nothing anybody could
    /// see, which is the one button in that rail that did not behave like the rest of the row.
    ///
    /// Three states and three answers, which is what makes it read correctly rather than merely toggle:
    /// not open at all, so open it; open behind another tab, so **show** it, because pressing the button
    /// for something you cannot see means "bring it here"; and open and showing, so close it. A plugin tab
    /// holds no text, so closing one asks nothing and saves nothing.
    pub fn toggle_the_plugin_tab(&mut self, tab: &str) {
        if let Some(index) = self.files.index_of_plugin_tab(tab) {
            if self.files.active_index() == index {
                self.close_tab(index);
                return;
            }
        }
        self.open_the_plugin_tab(tab);
    }

    /// Whether the tab that is showing belongs to a plugin rather than holding a file.
    ///
    /// One question, asked by the title bar's text tools and by the `View` menu, so the two cannot
    /// disagree about it. A plugin tab is a `Document` with no path — the picture precedent, followed
    /// exactly — and `services::file_kind` reads a document with no path as unsaved prose, which is
    /// right for a new file and wrong for a board.
    pub fn showing_a_plugins_tab(&self) -> bool {
        self.files.active().plugin.is_some()
    }

    /// Open a plugin's own tab in the editing area, or show it if it is already open.
    ///
    /// **Opening, not toggling.** `unluminous-cli plugins tab <key> --open` reaches this, and a command
    /// called `--open` that closed a tab would be a command that did the opposite of its name; `--close`
    /// is the other switch. What toggles is the rail button, through
    /// [`Self::toggle_the_plugin_tab`] — see `task-1848`.
    ///
    /// A contributed tab is a `Document` with a `PluginTab` beside it, which is exactly what a picture
    /// tab is: the four questions the window asks a tab — is it modified, can it be saved, has it a
    /// preview, has it a gutter — all answer the same way for both.
    pub fn open_the_plugin_tab(&mut self, tab: &str) {
        let Some(surface) = self.plugin_ui.surfaces().tab(tab).cloned() else {
            self.message = Some(format!("there is no `{tab}` tab"));
            return;
        };
        if let Err(problem) = self.plugin_ui.opened(&surface.plugin, &surface.provider) {
            self.message = Some(problem);
            return;
        }
        if let Some(index) = self.files.index_of_plugin_tab(tab) {
            self.files.show(index);
            self.focus = Focus::Editor;
            return;
        }
        // An empty document, because the tab's contents are drawn by the plugin. It exists because a tab
        // in Unluminous is a `Document`, which is the picture precedent followed exactly.
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            files::PluginTab {
                key: tab.to_owned(),
                plugin: surface.plugin.clone(),
                label: surface.what.label.clone(),
            },
        );
        self.focus = Focus::Editor;
    }

    /// Run one command against a plugin, whichever of the three ways in asked for it.
    ///
    /// The one place a plugin command is run from the window, so the menu entry, the button in the pane
    /// and `unluminous-cli plugin run` are one path. What it said goes in the status bar.
    pub fn run_plugin_command(
        &mut self,
        plugin: &str,
        command: &str,
        arguments: &[String],
    ) -> Result<crate::services::plugin_ui::Answer, String> {
        let answer = self.plugin_ui.run(plugin, command, arguments);
        match &answer {
            Ok(said) if !said.message.is_empty() => self.message = Some(said.message.clone()),
            Ok(_) => {}
            Err(problem) => self.message = Some(problem.clone()),
        }
        // Two commands are about the window rather than about the plugin, and the plugin says so by
        // answering them: a plugin asks for its pane to be shown or its tab to be opened, and the window is
        // what can do either. They are named for what they do to the window, so that asking a board for its
        // data (`board`) and asking for it to be put on the screen (`open-tab`) are two different questions.
        //
        // Both are kept here even though the one plugin that draws today contributes only a tab. This is
        // Unluminous's side of the plugin contract rather than Agent-Tasks's own code, and a manifest with a pane
        // in it is still a manifest Unluminous supports — `tasks/ui-plugin-architecture.md`.
        if answer.is_ok() {
            match command {
                "open-pane" => {
                    if let Some(pane) = self
                        .plugin_ui
                        .surfaces()
                        .panes
                        .iter()
                        .find(|surface| surface.plugin == plugin)
                    {
                        let key = pane.key(&pane.what.id);
                        self.show_the_plugin_pane(&key, true);
                    }
                }
                "open-tab" => {
                    if let Some(tab) = self
                        .plugin_ui
                        .surfaces()
                        .tabs
                        .iter()
                        .find(|surface| surface.plugin == plugin)
                    {
                        let key = tab.key(&tab.what.id);
                        self.open_the_plugin_tab(&key);
                    }
                }
                _ => {}
            }
        }
        answer
    }

    /// Let every open plugin catch up with whatever it owns that is not drawing.
    ///
    /// Once a frame, and cheap by construction: for Agent-Tasks it reads its terminals and sends anything
    /// queued behind an agent's prompt. It cannot wait for the pane to be looked at, because a board started
    /// from the command line has no pane showing and its handoff would sit in the queue until the next two
    /// minute tick. A plugin with nothing running does nothing here.
    pub(crate) fn let_the_plugins_catch_up(&mut self, ctx: &egui::Context) {
        // What each provider decided while nobody was looking at it, acted on after the loop for the
        // reason every other plugin request is: acting on one changes the window, and the loop is
        // holding a borrow of the registry the whole time.
        let mut asked: Vec<(String, crate::services::plugin_ui::Request)> = Vec::new();
        for plugin in self.plugin_ui.surfaces().plugins() {
            if !self.plugin_ui.is_open(&plugin) {
                continue;
            }
            if let Some(provider) = self.plugin_ui.provider(&plugin) {
                if provider.catch_up() {
                    // Something is running, so the window keeps drawing. A terminal that is printing while
                    // nobody is pointing at the window is the case this is for.
                    ctx.request_repaint_after(std::time::Duration::from_millis(120));
                }
                asked
                    .extend(provider.asking().into_iter().map(|request| (plugin.clone(), request)));
            }
        }
        for (plugin, request) in asked {
            self.act_on_a_plugin_request(&plugin, request, ctx);
        }
        self.tell_the_plugins_what_is_showing();
        // The canvas, on exactly the same terms and for the same reason - `task-1904`. A terminal
        // node that printed while its node was scrolled off the canvas, or while the canvas was put
        // away, must not lose what it printed, and a pipe between two of them has to be read whether
        // anybody is looking or not.
        let now = ctx.input(|input| input.time);
        if self.catch_the_space_up(now, ctx) {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    /// Tell every open provider which project is open and which file is showing.
    ///
    /// **Only when it has changed**, which is two `Option<PathBuf>` comparisons a frame — `task-1666`'s
    /// rule, that nothing which runs once a frame may do work it does not need to. The window owns
    /// this the way it owns the keyboard, so it is the window that says so; a provider that read it
    /// for itself would be a second thing deciding what a tab is.
    fn tell_the_plugins_what_is_showing(&mut self) {
        // **Compared before anything is cloned**, which is the whole of the once-a-frame rule: the
        // comparison is two borrowed paths and a frame in which neither moved allocates nothing.
        let showing = self.files.active().path();
        let project = self.tree.root();
        let same = self.told_the_plugins.as_ref().is_some_and(|(before, was)| {
            before.as_deref() == Some(project) && was.as_deref() == showing
        });
        if same {
            return;
        }
        let project = Some(project.to_path_buf());
        let showing = showing.map(std::path::Path::to_path_buf);
        self.told_the_plugins = Some((project.clone(), showing.clone()));
        // Kept for a provider opened later, which is told at open rather than at the next change.
        self.plugin_ui.set_showing(showing.clone());
        for plugin in self.plugin_ui.surfaces().plugins() {
            if !self.plugin_ui.is_open(&plugin) {
                continue;
            }
            if let Some(provider) = self.plugin_ui.provider(&plugin) {
                provider.showing(project.as_deref(), showing.as_deref());
            }
        }
    }

    /// Let every open plugin do whatever it does on a clock.
    ///
    /// Once every [`PLUGIN_TICK`], and nothing at all in between: a frame in which the interval has not
    /// passed costs one comparison, which is `task-1666`'s rule. For Agent-Tasks this is the watchdog and
    /// the terminals: a ticket's agent that has stopped is nudged and a ticket whose worker is gone is
    /// struck, and every terminal is read even while the board is not showing, so an agent that printed
    /// while the pane was put away is not counted as silent.
    pub(crate) fn tick_the_plugins(&mut self) {
        let now = std::time::Instant::now();
        let due = self.plugins_ticked_at.is_none_or(|last| now.duration_since(last) >= PLUGIN_TICK);
        if !due {
            return;
        }
        self.plugins_ticked_at = Some(now);
        for plugin in self.plugin_ui.surfaces().plugins() {
            if !self.plugin_ui.is_open(&plugin) {
                continue;
            }
            // `tick` is a command like any other, so it goes down the one path a change goes down and an
            // agent can ask for it by hand with `plugins run agent-tasks watchdog`.
            let _ = self.plugin_ui.run(&plugin, "tick", &[]);
        }
    }

    /// Read every plugin manifest from disk again, and say what would not parse.
    ///
    /// What `Settings -> Plugins -> Reload` and `unluminous-cli plugins reload` both call. **No restart is
    /// needed for anything a manifest says**, because a provider is already in the binary and loading a
    /// plugin is reading a file. That is the property this design has that the reference editor's dynamic plugins
    /// buy with a page of restrictions.
    pub fn reload_the_plugins(&mut self) -> Vec<String> {
        let (plugins, problems) = crate::services::plugins::Plugins::load(self.store.as_ref());
        self.plugins = plugins;
        self.refresh_the_plugins();
        // A manifest read again can have different keywords, a different comment marker or a
        // different colour scheme, and none of that moves any document's text -- so nothing else
        // would ask for a file to be coloured again. `set_plugin_enabled` has done this since it was
        // written and a reload did not, which is why `plugins reload` after editing a manifest left
        // every open file in the colours of the manifest before it.
        for file in self.files.iter_mut() {
            file.coloured_revision = None;
            file.document.syntax_is_wholly_dirty();
            file.cached.preview = None;
            file.cached.preview_diagrams.clear();
            file.cached.stale = true;
        }
        // A manifest edited by hand can take a tab away, so a reload closes an open tab whose contribution
        // has gone for the same reason switching a plugin off does: a tab whose plugin no longer offers it
        // would draw nothing and could not be told what it was.
        self.close_any_plugin_tabs_that_have_gone();
        self.set_the_font_everywhere();
        if let Some(first) = problems.first() {
            self.message = Some(format!("A plugin could not be read \u{2014} {first}"));
        }
        problems
    }

    /// Read what the plugins contribute, and tell the dock how many panes there are and where they asked
    /// to go.
    ///
    /// Called when the plugins are loaded and again whenever one is switched on or off. That is what
    /// makes a contribution appear and disappear in the same frame rather than at the next restart, which
    /// is the property `Plugins::renders` already gives a Mermaid diagram.
    pub fn refresh_the_plugins(&mut self) {
        self.plugin_ui.refresh(&self.plugins);
        if let Some(folder) = self.store.as_ref().map(|store| store.folder().join("plugins")) {
            self.plugin_ui.set_settings_folder(folder);
        }
        self.plugin_ui.set_project(Some(self.tree.root().to_path_buf()));
        self.plugin_ui.set_recent_projects(
            self.store.as_ref().map(|store| store.recent_projects()).unwrap_or_default(),
        );
        // The same waker `unluminous_git::Worker` and the terminal tile use, so a plugin's terminal that prints
        // while nobody is pointing at the window is drawn rather than waiting for the pointer to move.
        self.plugin_ui.set_waker(self.thread_waker());
        self.place_the_plugin_panes(true);
    }

    /// Tell the dock how many panes the plugins contribute and where each one goes.
    ///
    /// **The one place the dock is told**, which is what `task-1794` is about. `Layout::reset` is
    /// `*self = Layout::new()` and a fresh `Layout` has *no* contributed panes, so `panel reset` —
    /// and `settings reset`, which builds a fresh `Panes` — left `plugin_panes` at zero. From there
    /// `panels_on` filters through `Panel::all(0)`, the pane is on no side, `regions` gives it
    /// `Rect::ZERO`, and `show_the_plugin_panes` skips it for being smaller than a point. Nothing
    /// reports any of that: `PluginUi::is_visible` is a flag of the provider's own and still says
    /// yes, so the pane is gone from the window while every command says it is showing.
    ///
    /// `keep_where_they_were_left` is what separates the two callers. Refreshing the plugins must not
    /// undo a drag, so a pane the settings file has spoken about stays where it was put; resetting
    /// the layout means exactly the opposite, because "back where they started" for a contributed
    /// pane is where its **manifest** asked it to go.
    pub(crate) fn place_the_plugin_panes(&mut self, keep_where_they_were_left: bool) {
        let count = self.plugin_ui.pane_count();
        let sides: Vec<dock::Side> = (0..count)
            .map(|slot| {
                self.plugin_ui.pane(slot).map(|pane| pane.side).unwrap_or(dock::Side::Right)
            })
            .collect();
        // A pane whose side the settings file already records keeps where it was left; one it has not
        // spoken about goes where its manifest asked. Where somebody dragged a pane wins over what its
        // manifest wanted, which is the rule every panel already follows.
        let keys = self.plugin_ui.pane_keys();
        let placed: Vec<bool> = keys
            .iter()
            .map(|key| {
                keep_where_they_were_left
                    && self
                        .store
                        .as_ref()
                        .map(|store| {
                            store.read_values().text(&format!("panes.{key}.side")).is_some()
                        })
                        .unwrap_or(false)
            })
            .collect();
        self.panes.dock.set_plugin_panes(&sides, &placed);
        for slot in 0..count {
            if let Some(pane) = self.plugin_ui.pane(slot) {
                let panel = dock::Panel::Plugin(slot as u8);
                if !placed.get(slot).copied().unwrap_or(false) {
                    self.panes.set_width_of(panel, pane.width);
                    self.panes.set_height_of(panel, pane.height);
                }
            }
        }
    }

    /// Whether a contributed pane would really be drawn if it were switched on.
    ///
    /// Two things have to be true and only one of them was ever asked: the provider's own flag, and
    /// the **dock knowing the pane exists**. They are separate pieces of state, which is why they
    /// came apart at all, so the question is asked in one place and every answer Unluminous gives about a
    /// pane comes through it.
    ///
    /// It reads the layout rather than the rectangle the last frame drew, because a command runs at
    /// the top of a frame before anything has been laid out — a pane switched on a moment ago has no
    /// rectangle yet and is not broken.
    pub fn plugin_pane_is_reachable(&self, slot: usize) -> bool {
        slot < self.panes.dock.plugin_panes()
    }

    /// Whether a contributed pane is on the screen: switched on, and known to the dock.
    pub fn plugin_pane_is_showing(&self, slot: usize) -> bool {
        self.plugin_ui.is_visible(slot) && self.plugin_pane_is_reachable(slot)
    }

    /// Write a bundled plugin out to the settings folder and load it back from there.
    pub(crate) fn install_plugin(&mut self, id: &str) {
        let Some(store) = self.store.clone() else {
            self.message = Some("There is nowhere to install a plugin to.".to_owned());
            return;
        };
        match self.plugins.install(&store, id) {
            Ok(()) => {
                self.message =
                    Some(format!("Installed {id} into {}", Plugins::folder(&store, id).display()));
                for file in self.files.iter_mut() {
                    file.coloured_revision = None;
                    file.document.syntax_is_wholly_dirty();
                }
            }
            Err(problem) => self.message = Some(format!("{id} could not be installed: {problem}")),
        }
    }

    /// Take an installed plugin’s folder away, and go back to the copy that shipped in the binary.
    pub(crate) fn uninstall_plugin(&mut self, id: &str) {
        let Some(store) = self.store.clone() else {
            self.message =
                Some("There is nowhere a plugin could have been installed to.".to_owned());
            return;
        };
        match self.plugins.uninstall(&store, id) {
            Ok(()) => {
                self.message = Some(format!("Uninstalled {id}"));
                for file in self.files.iter_mut() {
                    file.coloured_revision = None;
                    file.document.syntax_is_wholly_dirty();
                }
            }
            Err(problem) => {
                self.message = Some(format!("{id} could not be uninstalled: {problem}"))
            }
        }
    }

    /// The picture the plugin that claims `path` puts in front of it, decoded and ready to draw.
    pub(crate) fn plugin_icon(
        &mut self,
        ctx: &egui::Context,
        path: Option<&Path>,
    ) -> Option<egui::TextureHandle> {
        let path = path?;
        let (id, bytes) = {
            let plugin = self.plugins.for_path(path)?;
            (plugin.id.clone(), plugin.icon.clone()?)
        };
        self.icons.texture(ctx, &id, &bytes)
    }

    /// The rectangle a contributed pane has, by `<plugin id>/<pane id>`, or `None` when it is not showing.
    ///
    /// Read by the tests, which need to say something about how wide a plugin's pane really turned out to
    /// be rather than about a control that happens to be absent at that width.
    /// How wide a contributed pane is, for the tests: `plugins pane` has no `--width`, and `panel size`
    /// names Unluminous's own four rather than a plugin's.
    pub fn set_plugin_pane_width_for(&mut self, key: &str, width: f32) {
        if let Some(slot) = self.plugin_ui.slot_of(key) {
            self.panes.set_width_of(dock::Panel::Plugin(slot as u8), width);
        }
    }

    pub fn plugin_pane_area_for(&self, key: &str) -> Option<Rect> {
        let slot = self.plugin_ui.slot_of(key)?;
        self.plugin_ui.is_visible(slot).then(|| self.panel_area(dock::Panel::Plugin(slot as u8)))
    }

    /// Switch a plugin on or off, and undo whatever it was doing to the window.
    ///
    /// The one place it happens, so the two things that have to follow always do: the open file may
    /// have just gained or lost its colours, and it may have just gained or lost its diagram. Doing
    /// them here rather than in the settings dialog is what makes `unluminous-cli plugins disable` and
    /// the tick box in `Plugins` mean exactly the same thing.
    pub fn set_plugin_enabled(&mut self, id: &str, on: bool) {
        self.plugins.set_enabled(self.store.as_ref(), id, on);
        // Every open file, not only the one showing: a plugin is a setting for the window, and with
        // panes there is more than one file being drawn.
        for file in self.files.iter_mut() {
            file.coloured_revision = None;
            // **And the tokeniser reads the whole file again.** `coloured_revision` on its own only
            // says the colours are to be worked out again; the incremental reading in
            // `colour_the_file` then asks `syntax_dirt` which *part* changed, and after the last
            // colouring that is `Clean`. The text has not moved -- the **grammar** has -- so nothing
            // but this says the old tokens are no longer about anything. `task-1922` found
            // `syntax_is_wholly_dirty` with no caller at all, which is why.
            file.document.syntax_is_wholly_dirty();
            // The preview is thrown away rather than kept, because whether a mermaid fence is a
            // picture or a piece of code has just changed and the preview is built from that answer.
            file.cached.preview = None;
            file.cached.preview_diagrams.clear();
        }
        self.mermaid_scenes.forget();
        // And what the plugins contribute, because a plugin switched off contributes nothing: its rail
        // button, its pane, its tab, its menu and its Settings page all go in this frame, and its
        // provider is closed so it drops what it held. That is the rule `Plugins::renders` already keeps
        // for a Mermaid diagram, made once more for everything a plugin adds to the window.
        // The surfaces first and the tabs after: what a plugin contributes is what decides whether its tab
        // is still offered, so checking the tabs before the rebuild would check them against the old answer.
        self.refresh_the_plugins();
        self.close_any_plugin_tabs_that_have_gone();
        // And the theme, for the same reason: a themes plugin switched off takes its palettes with it, so
        // the window falls back to Unluminous's own in this frame rather than staying in colours nothing in the
        // Settings list can name.
        self.apply_the_theme();
    }

    /// Close a plugin's tab when its plugin has gone, which is what switching one off does.
    ///
    /// A tab whose plugin is not there would draw nothing and could not be told what it was, so it is
    /// closed rather than left as an empty tab with a name on it.
    fn close_any_plugin_tabs_that_have_gone(&mut self) {
        let contributed: Vec<String> = self
            .plugin_ui
            .surfaces()
            .tabs
            .iter()
            .map(|surface| surface.key(&surface.what.id))
            .collect();
        let going: Vec<usize> = (0..self.files.len())
            .filter(|index| {
                self.files
                    .at(*index)
                    .plugin
                    .as_ref()
                    .is_some_and(|tab| !contributed.contains(&tab.key))
            })
            .collect();
        // Backwards, because closing a tab renumbers the ones after it.
        for index in going.into_iter().rev() {
            self.close_tab(index);
        }
    }

    /// Draw the picture the open tab holds, and take the gestures that move and zoom it.
    /// Draw the tab a plugin contributed, and act on what it asked for.
    ///
    /// A whole editing area, which is why `UiProvider::tab` exists beside `pane`: Agent-Tasks draws the
    /// lanes and the open ticket side by side here, where a 420 point column can only show one of them.
    pub(crate) fn show_plugin_tab(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        tab: &files::PluginTab,
    ) -> bool {
        // The page's own ground first, then the slot the decoration goes in, then the plugin's widgets.
        // egui hands a layer's shapes to the tessellator in the order they arrive, so a ground drawn after
        // the slot would paint over the very thing the slot is reserved for — which is what it did, and the
        // board came out with no lanes and no cards at all.
        let ground = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer);
        // The **editor's** ground, not the board's: `board_page` in the generic host would be one plugin's
        // idea of a colour leaking into the seam every plugin uses. They are the same constant, and this is
        // the one that says why it is that colour.
        ui.painter().rect_filled(area, 0, ground.ground(ground.palette.editor));
        drop(ground);
        // `Painter::set` fills the slot in once the drawing is over — see `paint_the_chrome`.
        let slot = ui.painter().add(egui::Shape::Noop);
        let chrome = self.chrome_for(&tab.plugin);
        let asked = {
            // **The colouring a plugin draws a fenced block or a SQL console with.** `unluminous-core` holds
            // no plugin registry, so it asks through `CodeHighlighter` and the window answers with the
            // same two calls `colour_the_file` makes for a source file — which is what makes a fence of
            // Rust in an answer look like a `.rs` file, and a statement in a query console look like a
            // `.sql` one. `Look::colouring_with` had no caller at all until now, so both were drawing
            // in one flat colour: the seam was built and never plugged in.
            let highlighter = PluginHighlighter { plugins: &self.plugins };
            let look = crate::services::plugin_ui::Look::of(&self.settings, &self.renderer)
                .holding_the_keyboard(matches!(self.focus, Focus::Plugin))
                .colouring_with(&highlighter)
                .drawing_into(&chrome);
            match self.plugin_ui.problem_with(&tab.plugin) {
                Some(problem) => {
                    let galley = ui.painter().layout(
                        format!("{} could not be opened.\n\n{problem}", tab.label),
                        egui::FontId::proportional(look.font_size),
                        look.palette.text_dim,
                        area.width() - 48.0,
                    );
                    ui.painter().galley(
                        area.min + egui::Vec2::splat(24.0),
                        galley,
                        look.palette.text_dim,
                    );
                    Vec::new()
                }
                None => {
                    let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(area));
                    tab_ui.set_clip_rect(area);
                    match self.plugin_ui.provider(&tab.plugin) {
                        Some(provider) => provider.tab(&mut tab_ui, &look),
                        None => Vec::new(),
                    }
                }
            }
        };
        self.paint_the_chrome(ui, slot, egui::Id::new(("plugin-tab", &tab.plugin)), area, &chrome);
        for request in asked {
            self.act_on_a_plugin_request(&tab.plugin, request, ui.ctx());
        }
        false
    }

    /// The chrome this plugin's surface records into this frame.
    ///
    /// Three things have to agree before there is one, and each says no on its own: the manifest asked for
    /// a renderer with `ui.chrome`, the provider says it draws decoration, and the person has not switched
    /// `plugins.chrome` off. Otherwise it is `Chrome::off`, which records nothing and costs nothing, and
    /// the board draws its flat form.
    fn chrome_for(&mut self, plugin: &str) -> crate::services::vello_canvas::Chrome {
        // The renderer the manifest named, matched rather than merely counted: there is one today, and the
        // day there are two this is where the second one is chosen.
        let asked = matches!(self.plugin_ui.surfaces().chrome_for(plugin), Some("vello"));
        let draws = self
            .plugin_ui
            .provider(plugin)
            .map(|provider| provider.draws_chrome())
            .unwrap_or(false);
        match self.settings.plugin_chrome && asked && draws {
            true => crate::services::vello_canvas::Chrome::recording(),
            false => crate::services::vello_canvas::Chrome::off(),
        }
    }

    /// Rasterise what a plugin recorded and fill in the slot reserved for it.
    ///
    /// The one place a `Decor` list becomes pixels, so the pane, the tab and the settings page cannot
    /// disagree about when there is decoration. A list that is empty, or a surface too big to rasterise,
    /// leaves the slot as the `Noop` it was — which draws nothing rather than a blank rectangle.
    pub(crate) fn paint_the_chrome(
        &mut self,
        ui: &egui::Ui,
        slot: egui::layers::ShapeIdx,
        id: egui::Id,
        area: Rect,
        chrome: &crate::services::vello_canvas::Chrome,
    ) {
        let items = chrome.take();
        if items.is_empty() {
            return;
        }
        if let Some((texture, drawn)) = self.canvases.texture_for(ui.ctx(), id, area, &items) {
            let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
            ui.painter().set(slot, egui::Shape::image(texture, drawn, uv, egui::Color32::WHITE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::plugins::{PaneContribution, RailGroup, Surface};

    /// A pane a fictional plugin contributed, so the visibility slots can be exercised without
    /// loading a real manifest. `provider` deliberately names nothing in `UI_PROVIDERS`, because
    /// none of these tests ever asks one to be built.
    fn a_pane(id: &str, applies: &str) -> Surface<PaneContribution> {
        Surface {
            plugin: "demo".to_owned(),
            provider: "demo".to_owned(),
            what: PaneContribution {
                id: id.to_owned(),
                label: id.to_owned(),
                icon: "board".to_owned(),
                group: RailGroup::Bottom,
                side: dock::Side::Right,
                width: 300.0,
                height: 300.0,
                applies: applies.to_owned(),
            },
        }
    }

    #[test]
    fn a_pane_that_always_applies_does_so_with_or_without_a_project() {
        let mut ui = PluginUi::default();
        ui.surfaces.panes.push(a_pane("board", "always"));
        assert!(ui.applies(0));
        ui.set_project(Some(PathBuf::from("/a/project")));
        assert!(ui.applies(0));
    }

    #[test]
    fn a_pane_that_asks_for_a_project_is_absent_without_one() {
        let mut ui = PluginUi::default();
        ui.surfaces.panes.push(a_pane("board", "in_project"));
        assert!(!ui.applies(0), "no project is open");
        ui.set_project(Some(PathBuf::from("/a/project")));
        assert!(ui.applies(0), "one is open now");
    }

    #[test]
    fn the_pane_count_never_exceeds_the_number_of_slots_there_are() {
        let mut ui = PluginUi::default();
        for index in 0..PLUGIN_PANES + 3 {
            ui.surfaces.panes.push(a_pane(&format!("pane-{index}"), "always"));
        }
        assert_eq!(ui.pane_count(), PLUGIN_PANES);
    }

    #[test]
    fn a_pane_is_found_by_its_key_and_a_key_is_found_by_its_slot() {
        let mut ui = PluginUi::default();
        ui.surfaces.panes.push(a_pane("board", "always"));
        assert_eq!(ui.pane_key(0), Some("demo/board".to_owned()));
        assert_eq!(ui.slot_of("demo/board"), Some(0));
        assert_eq!(ui.slot_of("nothing/like-this"), None);
    }

    #[test]
    fn showing_or_hiding_an_out_of_range_slot_is_refused_rather_than_panicking() {
        let mut ui = PluginUi::default();
        assert!(ui.set_visible(0, true).is_some(), "there is no pane in slot 0 yet");
    }

    #[test]
    fn a_pane_that_needs_a_project_cannot_be_shown_without_one() {
        let mut ui = PluginUi::default();
        ui.surfaces.panes.push(a_pane("board", "in_project"));
        let refusal = ui.set_visible(0, true);
        assert!(refusal.is_some());
        assert!(!ui.is_visible(0), "refused, so it never opened");

        ui.set_project(Some(PathBuf::from("/a/project")));
        // Hiding never has to open anything, so it is never refused by the condition — only showing is.
        assert!(ui.set_visible(0, false).is_none());
    }

    #[test]
    fn a_panes_slot_maps_onto_a_plugin_panel_only_within_the_slots_there_are() {
        assert_eq!(Panel::plugin_pane(0), Some(Panel::Plugin(0)));
        assert_eq!(
            Panel::plugin_pane(PLUGIN_PANES - 1),
            Some(Panel::Plugin((PLUGIN_PANES - 1) as u8))
        );
        assert_eq!(
            Panel::plugin_pane(PLUGIN_PANES),
            None,
            "there is no fifth slot to move a pane into"
        );
    }

    /// `refresh` is what withdraws a contribution the moment its plugin is switched off — the rule
    /// `Plugins::renders` already keeps for a Mermaid diagram, applied to a pane instead. `showing` is
    /// set directly here rather than through `set_visible`, which would have to build a real provider;
    /// `refresh`'s pruning does not care how a pane came to be marked open.
    #[test]
    fn a_pane_stops_showing_the_moment_its_plugin_is_switched_off() {
        let (mut plugins, _) = Plugins::load(None);
        let mut ui = PluginUi::default();
        ui.refresh(&plugins);
        let slot = ui.slot_of("agent-tasks/board").expect("agent-tasks contributes a pane");
        ui.showing.push("agent-tasks/board".to_owned());
        assert!(ui.is_visible(slot));

        plugins.set_enabled(None, "agent-tasks", false);
        ui.refresh(&plugins);
        assert_eq!(ui.slot_of("agent-tasks/board"), None, "the plugin contributes nothing now");
        assert!(
            !ui.showing.contains(&"agent-tasks/board".to_owned()),
            "a pane whose plugin has gone cannot still be showing"
        );
    }
}
