//! The window's side of the Designer: opening a design as a tab, drawing it, putting the page where it goes,
//! and acting on what the page reports.
//!
//! `services::designer` holds the notes and every change to them, and `components::designer` draws the
//! toolbar, the notes and the prompt box. What neither can do is the page, because a window has one native web
//! view and only the window owns it (`services::browser` says why). So a design opens as a **design tab**: a
//! plugin tab whose key is `designer:<path of the design relative to the project>`, drawn here the way a realm
//! tab is, with a browser tab kept beside it in [`UnluminousApp::design_tabs`] and listed with every other
//! browser tab, so the one native view is pointed at it by the same code that points it at the rest.
//!
//! ## The page talks by being asked
//!
//! The script in the page (`designer.js`) is kept there by `BrowserHost::watch` and asked what happened every
//! 150 milliseconds. What it reports arrives as `BrowserEvent::Reported` and goes to the provider, and what the
//! provider wants the page to do is sent back as a script through `BrowserHost::evaluate`. Nothing the page
//! can call exists.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, Rect};

use crate::app::files::PluginTab;
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

    /// Open a design in a tab of the editing area, or show the tab already open on it.
    ///
    /// @param design - the design, relative to the project
    pub(crate) fn open_a_design_in_a_tab(&mut self, design: &Path) -> Result<(), String> {
        if !self.designer_is_on() {
            return Err("The Designer plugin is switched off.".to_owned());
        }
        let key = design_tab_key(design);
        self.show_the_editing_area_for_a_tab();
        if let Some(index) = self.files.index_of_plugin_tab(&key) {
            self.show_tab(index);
            self.focus = Focus::Editor;
            return Ok(());
        }
        let page = self.make_the_page_of(&key, design)?;
        self.design_tabs.insert(key.clone(), page);
        let label =
            design.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        self.files.open_plugin_tab(
            unluminous_core::Document::new(),
            PluginTab { key, plugin: "designer".to_owned(), label },
        );
        self.focus = Focus::Editor;
        Ok(())
    }

    /// The browser tab that loads a design's file, with the project as the root its links resolve under.
    fn make_the_page_of(&mut self, _key: &str, design: &Path) -> Result<BrowserTab, String> {
        let file = self.tree.root().join(design);
        let location = BrowserLocation::parse(&file.to_string_lossy(), self.tree.root())?;
        Ok(self.browser.open_tab(location))
    }

    /// The design a showing design tab is on, as a path relative to the project.
    pub(crate) fn showing_design_path(&self) -> Option<PathBuf> {
        let tab = self.files.active().plugin.as_ref()?;
        match tab.plugin == "designer" {
            true => design_of_a_tab_key(&tab.key),
            false => None,
        }
    }

    /// Let go of the page behind a design tab that is closing.
    ///
    /// @param index - the tab about to close
    pub(crate) fn forget_a_design_tab(&mut self, index: usize) {
        let Some(key) =
            self.files.get(index).and_then(|file| file.plugin.as_ref()).map(|tab| tab.key.clone())
        else {
            return;
        };
        let Some(design) = design_of_a_tab_key(&key) else { return };
        if let Some(page) = self.design_tabs.remove(&key) {
            self.design_zooms.remove(&page.id);
            self.browser.close_tab(page.id);
        }
        if let Ok(designer) = self.designer() {
            designer.forget(&design);
        }
    }

    /// Draw a design tab into `area`: the toolbar, the well and the notes, and put the page in the well.
    ///
    /// @param ui - where to draw
    /// @param area - the editing area
    /// @param tab - the plugin tab
    /// @param focused - whether the pane this tab is in has the keyboard
    pub(crate) fn show_a_design_tab(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        tab: &PluginTab,
        focused: bool,
    ) -> bool {
        let Some(design) = design_of_a_tab_key(&tab.key) else { return false };
        if !self.design_tabs.contains_key(&tab.key) {
            match self.make_the_page_of(&tab.key, &design) {
                Ok(page) => {
                    self.design_tabs.insert(tab.key.clone(), page);
                }
                Err(problem) => return self.say_in_a_design_tab(ui, area, &problem),
            }
        }
        let Some(id) = self.design_tabs.get(&tab.key).map(|page| page.id) else { return false };
        match self.designer() {
            Ok(designer) => designer.show(&design),
            Err(problem) => return self.say_in_a_design_tab(ui, area, &problem),
        }
        let outcome = self.draw_the_design(ui, area, &design, tab);
        let Some(outcome) = outcome else { return false };
        self.put_the_page_in_its_well(id, &outcome, area, ui.clip_rect(), focused);
        self.talk_to_the_page(id, &design, outcome.zoom);
        for request in outcome.requests {
            self.act_on_a_plugin_request("designer", request, ui.ctx());
        }
        false
    }

    /// One sentence in the middle of a design tab that cannot be drawn.
    fn say_in_a_design_tab(&self, ui: &egui::Ui, area: Rect, said: &str) -> bool {
        ui.painter().rect_filled(area, 0, crate::theme::color::editor());
        crate::app::realm_nodes::say_in_a_node(ui, area, u64::MAX - 2, said);
        false
    }

    /// Draw the ground, the decoration and the provider's own surfaces, and rasterise what was recorded.
    fn draw_the_design(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        design: &Path,
        tab: &PluginTab,
    ) -> Option<crate::components::designer::tab::Outcome> {
        let ground = Look::of(&self.settings, &self.renderer);
        ui.painter().rect_filled(area, 0, ground.ground(ground.palette.editor));
        drop(ground);
        let slot = ui.painter().add(egui::Shape::Noop);
        let chrome = self.chrome_for("designer");
        let outcome = {
            let highlighter = crate::app::PluginHighlighter { plugins: &self.plugins };
            let look = Look::of(&self.settings, &self.renderer)
                .following_the_editor_font(false)
                .holding_the_keyboard(matches!(self.focus, Focus::Editor))
                .colouring_with(&highlighter)
                .drawing_into(&chrome);
            let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(area));
            tab_ui.set_clip_rect(area.intersect(ui.clip_rect()));
            let designer = self.plugin_ui.opened("designer", "designer").ok()?;
            let designer = designer.as_any_mut()?.downcast_mut::<Designer>()?;
            crate::components::designer::tab::show(designer, &mut tab_ui, area, &look, design)
        };
        self.paint_the_chrome(ui, slot, egui::Id::new(("designer-tab", &tab.key)), area, &chrome);
        Some(outcome)
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

    /// The keys a design tab has while it holds the keyboard and no text box does.
    ///
    /// `Escape` puts the choice away, `C`, `V` and `B` switch to comment, select and browse, and `[` and `]`
    /// move the choice to the parent and the first child. Read before the panes are drawn, which is the one
    /// frame ordering every other reader of keys in `route_the_keys_before_the_panes` relies on, and taken
    /// out of the frame so one press is not also read as typing.
    pub(crate) fn route_the_designers_keys(&mut self, ui: &mut egui::Ui) {
        let Some(design) = self.showing_design_path() else { return };
        let ctx = ui.ctx().clone();
        if self.focus != Focus::Editor
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
        } else if pressed(ui, Key::V) {
            designer.set_mode(&design, Mode::Select)
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
