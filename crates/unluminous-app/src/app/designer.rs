//! The window's side of the Designer: drawing the Designer pane and its tabs, putting the page where it goes,
//! and acting on what the page reports.
//!
//! `services::designer` holds the notes, the pane's tabs and every change to them, and `components::designer`
//! draws the tab strip, the toolbar, the notes and the prompt box. What neither can do is the page, because a
//! window has one native web view and only the window owns it (`services::browser` says why). So the window
//! draws the Designer pane's body itself, rather than asking the provider for it the way it asks every other
//! plugin pane, and keeps one browser tab for each design open in it in [`UnluminousApp::design_tabs`], keyed
//! `designer:<path of the design relative to the project>`. They are listed with every other browser tab, so the
//! one native view is pointed at the showing design by the same code that points it at the rest.
//!
//! ## The pane has its own tabs (`task-2260`)
//!
//! *"The designer pane should include it's own set of tabs for the rendered html, not require the file panel
//! that's mixed with all the other files that are open. It should have a blank tab showing by default, and
//! remember which tabs/files are open between restarts."* A design used to open as a plugin tab in the editing
//! area. It opens in a tab of the Designer pane now, and the blank tab shows the list of designs. A project
//! that remembered a `designer:` tab in the editing area brings it back in the pane instead.
//!
//! ## The page talks by being asked
//!
//! The script in the page (`designer.js`) is kept there by `BrowserHost::watch` and asked what happened every
//! 150 milliseconds. What it reports arrives as `BrowserEvent::Reported` and goes to the provider, and what the
//! provider wants the page to do is sent back as a script through `BrowserHost::evaluate`. Nothing the page
//! can call exists.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, Rect};

use crate::app::{Focus, UnluminousApp};
use crate::services::browser::{BrowserLocation, BrowserPlacement, BrowserTab};
use crate::services::designer::config::{slashed, Configuration};
use crate::services::designer::page::{self, Mode};
use crate::services::designer::{Designer, PageCall, Want};
use crate::services::plugin_ui::Look;

/// What a design tab's key starts with. The rest is the design's path, relative to the project and written
/// with `/`, so `plugin-tabs.txt` brings the tab back on the design it showed.
pub(crate) const DESIGN_TAB: &str = "designer:";

/// The key of the tab that shows the design at `design`.
///
/// @param design - the design, relative to the project
pub(crate) fn design_tab_key(design: &Path) -> String {
    format!("{DESIGN_TAB}{}", slashed(design))
}

/// The design a tab's key names, when it is a design tab's key.
///
/// @param key - a plugin tab's key
pub(crate) fn design_of_a_tab_key(key: &str) -> Option<PathBuf> {
    key.strip_prefix(DESIGN_TAB).map(PathBuf::from)
}

impl UnluminousApp {
    /// Whether the Designer plugin is switched on.
    pub(crate) fn designer_is_on(&self) -> bool {
        self.plugins.is_on("designer")
    }

    /// The Designer provider, opened if it has not been, as its own type.
    ///
    /// The one way the window reaches it, so the three places that need it (opening a file, drawing a tab and
    /// acting on a page's report) cannot come to different answers about whether it is there.
    pub fn designer(&mut self) -> Result<&mut Designer, String> {
        if !self.designer_is_on() {
            return Err("The Designer plugin is switched off.".to_owned());
        }
        let provider = self.plugin_ui.opened("designer", "designer")?;
        provider
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Designer>())
            .ok_or_else(|| "The Designer is not available.".to_owned())
    }

    /// The design a file is, relative to the project, when it is one.
    ///
    /// Asked before a file is opened, so an HTML file anywhere else opens as it always did. The provider is
    /// only opened for an HTML file, which is what keeps a window that never touches a design from building it.
    ///
    /// @param path - the file about to be opened
    pub(crate) fn the_design_a_file_is(&mut self, path: &Path) -> Option<PathBuf> {
        if !crate::services::designer::library::is_html(path)
            || !crate::services::browser::SUPPORTED
        {
            return None;
        }
        let designer = self.designer().ok()?;
        designer.is_a_design(path).then(|| designer.key_of(path)).flatten()
    }

    /// Open a design in a tab of the Designer pane, and put the pane on the screen.
    ///
    /// What opening an HTML file in the designs folder does, from the explorer, `Go to File` or `tab open`, and
    /// what a `designer:` tab a project remembered in the editing area becomes (`task-2260`).
    ///
    /// @param design - the design, relative to the project
    pub(crate) fn open_a_design_in_the_pane(&mut self, design: &Path) -> Result<(), String> {
        let designer = self.designer()?;
        designer.loaded(design)?;
        designer.open_in_a_tab(design);
        self.show_the_designer_pane();
        Ok(())
    }

    /// Put the Designer pane on the screen.
    fn show_the_designer_pane(&mut self) {
        let key = self
            .plugin_ui
            .surfaces()
            .panes
            .iter()
            .find(|surface| surface.plugin == "designer")
            .map(|surface| surface.key(&surface.what.id));
        if let Some(key) = key {
            self.show_the_plugin_pane(&key, true);
        }
    }

    /// The browser tab that loads a design's file, with the project as the root its links resolve under.
    fn make_the_page_of(&mut self, _key: &str, design: &Path) -> Result<BrowserTab, String> {
        let file = self.tree.root().join(design);
        let location = BrowserLocation::parse(&file.to_string_lossy(), self.tree.root())?;
        Ok(self.browser.open_tab(location))
    }

    /// How many design pages the window holds, one for each design open in a tab of the Designer pane that has been
    /// drawn. A test reads it to see that a closed tab let go of its page.
    pub fn design_pages(&self) -> usize {
        self.design_tabs.len()
    }

    /// The design the Designer pane is showing, as a path relative to the project, while the pane is drawn.
    pub(crate) fn showing_design_path(&self) -> Option<PathBuf> {
        self.design_drawn.clone()
    }

    /// Let go of the page of every design that is no longer open in a tab of the Designer pane, and of every page
    /// when the plugin is switched off. Asked once a frame, so a tab closed by its cross, by `close-tab` or by
    /// switching the plugin off all let go of their page the same way.
    pub(crate) fn let_go_of_closed_designs(&mut self) {
        if self.design_tabs.is_empty() {
            return;
        }
        let open: Vec<String> = match self.designer_is_on() && self.plugin_ui.is_open("designer") {
            true => match self.designer() {
                Ok(designer) => {
                    designer.tabs().iter().flatten().map(|design| design_tab_key(design)).collect()
                }
                Err(_) => Vec::new(),
            },
            false => Vec::new(),
        };
        let closed: Vec<String> =
            self.design_tabs.keys().filter(|key| !open.contains(key)).cloned().collect();
        for key in closed {
            if let Some(page) = self.design_tabs.remove(&key) {
                self.design_zooms.remove(&page.id);
                self.browser.close_tab(page.id);
            }
        }
    }

    /// Draw the Designer pane's body: its tab strip, and under it the list of designs for a blank tab or the
    /// design for a design's tab, with the page put in its well.
    ///
    /// The window draws it rather than the provider, because the page is the window's one native view.
    ///
    /// @param ui - where to draw
    /// @param body - the pane under its header
    /// @param panel - the pane's panel, whose zoom it is drawn at
    pub(crate) fn show_the_designer_pane_body(
        &mut self,
        ui: &mut egui::Ui,
        body: Rect,
        panel: crate::app::dock::Panel,
    ) {
        self.design_drawn = None;
        let design = match self.designer() {
            Ok(designer) => designer.active_design().map(Path::to_path_buf),
            Err(problem) => {
                self.say_in_a_design_tab(ui, body, &problem);
                return;
            }
        };
        let page = match &design {
            Some(design) => match self.the_page_of(design) {
                Ok(id) => Some(id),
                Err(problem) => {
                    self.say_in_a_design_tab(ui, body, &problem);
                    return;
                }
            },
            None => None,
        };
        if let Ok(designer) = self.designer() {
            match &design {
                Some(design) => designer.show(design),
                None => designer.show_nothing(),
            }
        }
        self.design_drawn = design.clone();
        self.take_the_keyboard_on_a_press(ui, body);
        let focused = self.focus == Focus::Plugin
            && self.plugin_with_the_keyboard.as_deref() == Some("designer");
        let Some(drawn) = self.draw_the_pane(ui, body, panel, design.as_deref()) else { return };
        if let (Some(id), Some(design), Some(outcome)) = (page, &design, &drawn.design) {
            self.put_the_page_in_its_well(id, outcome, drawn.content, ui.clip_rect(), focused);
            // The design's page while the view shows a browser tab beside it: its last picture. `task-2263`.
            let well = outcome.page.intersect(drawn.content.intersect(ui.clip_rect()));
            self.paint_a_page_not_showing(ui, id, well);
            self.talk_to_the_page(id, design, outcome.zoom);
        }
        for request in drawn.requests {
            self.act_on_a_plugin_request("designer", request, ui.ctx());
        }
    }

    /// The browser tab a design's page is in, made the first time the design is drawn.
    fn the_page_of(&mut self, design: &Path) -> Result<u64, String> {
        let key = design_tab_key(design);
        if !self.design_tabs.contains_key(&key) {
            let page = self.make_the_page_of(&key, design)?;
            self.design_tabs.insert(key.clone(), page);
        }
        self.design_tabs.get(&key).map(|page| page.id).ok_or_else(|| "The page went.".to_owned())
    }

    /// A press anywhere in the pane gives it the keyboard, so `C`, `V`, `B`, `[`, `]` and `Escape` reach the
    /// design, which is what a press in the design tab did when it was in the editing area.
    fn take_the_keyboard_on_a_press(&mut self, ui: &egui::Ui, body: Rect) {
        let pressed = ui.input(|input| {
            input.pointer.any_pressed()
                && input.pointer.interact_pos().is_some_and(|at| body.contains(at))
        });
        let mine = self.focus == Focus::Plugin
            && self.plugin_with_the_keyboard.as_deref() == Some("designer");
        if pressed && !mine {
            self.act_on_a_plugin_request(
                "designer",
                crate::services::plugin_ui::Request::TakeTheKeyboard(true),
                ui.ctx(),
            );
        }
    }

    /// One sentence in the middle of a design tab that cannot be drawn.
    fn say_in_a_design_tab(&self, ui: &egui::Ui, area: Rect, said: &str) -> bool {
        ui.painter().rect_filled(area, 0, crate::theme::color::editor());
        crate::app::realm_nodes::say_in_a_node(ui, area, u64::MAX - 2, said);
        false
    }

    /// Draw the ground, the decoration, the tab strip and what the showing tab holds, and rasterise what was
    /// recorded.
    fn draw_the_pane(
        &mut self,
        ui: &mut egui::Ui,
        body: Rect,
        panel: crate::app::dock::Panel,
        design: Option<&Path>,
    ) -> Option<Drawn> {
        let ground = Look::of(&self.settings, &self.renderer);
        ui.painter().rect_filled(body, 0, ground.ground(ground.palette.editor));
        drop(ground);
        let slot = ui.painter().add(egui::Shape::Noop);
        let chrome = self.chrome_for("designer");
        let zoom = self.panes.zoom_of(panel);
        let drawn = {
            let highlighter = crate::app::PluginHighlighter { plugins: &self.plugins };
            let look = Look::of(&self.settings, &self.renderer)
                .following_the_editor_font(false)
                .zoomed_by(zoom)
                .holding_the_keyboard(matches!(self.focus, Focus::Plugin))
                .colouring_with(&highlighter)
                .drawing_into(&chrome);
            let mut pane_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            pane_ui.set_clip_rect(body.intersect(ui.clip_rect()));
            let designer = self.plugin_ui.opened("designer", "designer").ok()?;
            let designer = designer.as_any_mut()?.downcast_mut::<Designer>()?;
            let (content, mut requests) =
                crate::components::designer::tabs::show(designer, &mut pane_ui, &look, body);
            let mut inside = pane_ui.new_child(egui::UiBuilder::new().max_rect(content));
            inside.set_clip_rect(content.intersect(pane_ui.clip_rect()));
            let outcome = match design {
                Some(design) => {
                    let outcome = crate::components::designer::tab::show(
                        designer,
                        &mut inside,
                        content,
                        &look,
                        design,
                    );
                    requests.extend(outcome.requests.clone());
                    Some(outcome)
                }
                None => {
                    requests.extend(crate::components::designer::pane::show(
                        designer,
                        &mut inside,
                        &look,
                    ));
                    None
                }
            };
            Drawn { content, design: outcome, requests }
        };
        self.paint_the_chrome(ui, slot, egui::Id::new("designer-pane"), body, &chrome);
        Some(drawn)
    }

    /// Say where this frame drew the page, so the one native view is put there.
    fn put_the_page_in_its_well(
        &mut self,
        id: u64,
        outcome: &crate::components::designer::tab::Outcome,
        area: Rect,
        clip: Rect,
        focused: bool,
    ) {
        let visible = outcome.page.intersect(area.intersect(clip));
        if visible.width() < 1.0 || visible.height() < 1.0 {
            return;
        }
        self.browser_placements.push(BrowserPlacement {
            id,
            area: outcome.page,
            visible,
            focused,
            zoom: Some(outcome.zoom),
            over: Vec::new(),
            playing: false,
        });
    }

    /// Keep the overlay script in the page, and tell it what the provider wants it told.
    ///
    /// Only to the page the view is showing: a script meant for one page must not run in another, and what a
    /// page that is not showing missed is sent again when it loads, because loading reports `loaded` and the
    /// provider answers that by sending the mode, the pins and the choice.
    fn talk_to_the_page(&mut self, id: u64, design: &Path, zoom: f64) {
        if self.browser.watching() != Some(id) {
            self.browser.watch(id, Some(page::watch()));
        }
        if self.browser.showing() != Some(id) {
            return;
        }
        let calls = match self.designer() {
            Ok(designer) => designer.take_calls(design),
            Err(_) => return,
        };
        // A reload throws the page's scale away, and the pins are what a reload is answered with.
        if calls.iter().any(|call| matches!(call, PageCall::SetPins(_))) {
            self.design_zooms.remove(&id);
        }
        for call in calls {
            if let Err(problem) = self.browser.evaluate(id, &script_for(&call)) {
                self.message = Some(problem);
            }
        }
        self.keep_the_overlay_its_size(id, zoom);
    }

    /// Tell the page's overlay to undo the zoom the page was given to fit its device, so a pin and an outline's
    /// label are the same size on the screen whatever device is chosen. Sent only when the zoom moved.
    ///
    /// @param id - the design's browser tab
    /// @param zoom - the zoom the page is drawn at
    fn keep_the_overlay_its_size(&mut self, id: u64, zoom: f64) {
        if self.design_zooms.get(&id).is_some_and(|sent| (sent - zoom).abs() < 0.001) {
            return;
        }
        if self.browser.evaluate(id, &page::set_scale(1.0 / zoom.max(0.05))).is_ok() {
            self.design_zooms.insert(id, zoom);
        }
    }

    /// Hand what the page reported to the provider.
    ///
    /// @param id - the browser tab that reported
    /// @param json - the answer `take` gave
    pub(crate) fn the_page_of_a_design_reported(&mut self, id: u64, json: &str) {
        let Some(key) =
            self.design_tabs.iter().find(|(_, page)| page.id == id).map(|(key, _)| key.clone())
        else {
            return;
        };
        let Some(design) = design_of_a_tab_key(&key) else { return };
        let reports = page::read_reports(json);
        let Ok(designer) = self.designer() else { return };
        for report in reports {
            designer.page_reported(&design, report);
        }
    }

    /// Do what the provider asked of the window outside a draw: open a design as text, load a page again, and
    /// tell the explorer where the designs are when the folder changed.
    pub(crate) fn act_on_the_designer_wants(&mut self) {
        self.let_go_of_closed_designs();
        if !self.designer_is_on() || !self.plugin_ui.is_open("designer") {
            return;
        }
        let Ok(designer) = self.designer() else { return };
        let wants = designer.take_wants();
        let changed = designer.take_config_changed();
        for want in wants {
            match want {
                Want::OpenSource(design) => {
                    let file = self.tree.root().join(design);
                    if let Err(problem) = self.open_a_file_as_text(&file) {
                        self.message = Some(problem);
                    }
                }
                Want::Reload(design) => self.reload_a_design(&design),
            }
        }
        if changed {
            let shows = self.dot_folders_the_explorer_shows();
            self.tree.set_shows(shows);
        }
    }

    /// Load a design's page again, which is what an agent's edit makes the file watcher do by itself.
    fn reload_a_design(&mut self, design: &Path) {
        let Some(id) = self.design_tabs.get(&design_tab_key(design)).map(|page| page.id) else {
            return;
        };
        if let Err(problem) = self.browser.reload(id) {
            self.message = Some(problem);
        }
    }

    /// The dot folders the explorer lists: every switched on plugin's `explorer.shows`, and the Designer's
    /// folder when Settings put it somewhere else and its first part starts with a dot.
    pub(crate) fn dot_folders_the_explorer_shows(&mut self) -> Vec<String> {
        let mut shown = self.plugins.explorer_shows();
        if !self.designer_is_on() {
            return shown;
        }
        // What the provider holds when it is open, because that is what the person has set; and what is in the
        // plugin's own file when it is not, which is what it will hold when it is.
        let configured = match self.plugin_ui.is_open("designer") {
            true => self.designer().ok().map(|designer| designer.config.folder.clone()),
            false => self
                .plugin_ui
                .context_for("designer")
                .folder
                .map(|folder| Configuration::read(&folder).0.folder),
        };
        let first = configured.and_then(|folder| {
            folder.components().next().map(|part| part.as_os_str().to_string_lossy().into_owned())
        });
        if let Some(first) = first.filter(|first| first.starts_with('.')) {
            if !shown.contains(&first) {
                shown.push(first);
                shown.sort();
            }
        }
        shown
    }

    /// The keys the Designer pane has while it holds the keyboard on a design and no text box does.
    ///
    /// `Escape` puts the choice away, `C` and `B` switch to comment and browse, and `[` and `]`
    /// move the choice to the parent and the first child. Read before the panes are drawn, which is the one
    /// frame ordering every other reader of keys in `route_the_keys_before_the_panes` relies on, and taken
    /// out of the frame so one press is not also read as typing.
    pub(crate) fn route_the_designers_keys(&mut self, ui: &mut egui::Ui) {
        let Some(design) = self.showing_design_path() else { return };
        let ctx = ui.ctx().clone();
        if self.focus != Focus::Plugin
            || self.plugin_with_the_keyboard.as_deref() != Some("designer")
            || crate::app::text_box_has_the_keyboard(&ctx)
            || crate::app::a_modal_has_the_keyboard(&ctx)
            || !ui.input(|input| input.modifiers.is_none())
        {
            return;
        }
        let pressed = |ui: &mut egui::Ui, key: Key| {
            ui.input_mut(|input| input.consume_key(Modifiers::NONE, key))
        };
        let Ok(designer) = self.designer() else { return };
        let result = if pressed(ui, Key::C) {
            designer.set_mode(&design, Mode::Comment)
        } else if pressed(ui, Key::B) {
            designer.set_mode(&design, Mode::Browse)
        } else if pressed(ui, Key::OpenBracket) {
            designer.walk_the_selection(&design, true)
        } else if pressed(ui, Key::CloseBracket) {
            designer.walk_the_selection(&design, false)
        } else if designer
            .design_view(&design)
            .is_some_and(|view| view.selection.is_some() || view.composing.is_some())
            && pressed(ui, Key::Escape)
        {
            designer.clear_the_selection(&design)
        } else {
            Ok(())
        };
        if let Err(problem) = result {
            self.message = Some(problem);
        }
    }
}

/// What drawing the Designer pane came to.
struct Drawn {
    /// The room under the tab strip.
    content: Rect,
    /// What the design's tab came to, when a design's tab is showing.
    design: Option<crate::components::designer::tab::Outcome>,
    requests: Vec<crate::services::plugin_ui::Request>,
}

/// The script that tells the page what a call from the provider means.
///
/// @param call - what the provider wants the page to do
pub(crate) fn script_for(call: &PageCall) -> String {
    match call {
        PageCall::SetMode(mode) => page::set_mode(*mode),
        PageCall::SetPins(pins) => page::set_pins(pins),
        PageCall::Select(selector) => page::select(selector),
        PageCall::Focus(id) => page::focus(*id),
        PageCall::Clear => page::clear(),
        PageCall::Parent => page::parent(),
        PageCall::Child => page::child(),
    }
}
