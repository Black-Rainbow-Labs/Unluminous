//! The browser tabs' half of the window.
//!
//! A window has **one** native child view whatever the tab count, and it is pointed at the tab that
//! is showing — `services::browser` records what creating a second one costs. Everything here either
//! decides which tab that is or carries a command to it.

use egui::Rect;

use crate::components::browser_view;
use crate::services::browser::{BrowserCommand, BrowserEvent, BrowserLocation, BrowserTab};

use crate::app::files;
use crate::app::{a_modal_has_the_keyboard, Focus, UnluminousApp};

impl UnluminousApp {
    /// Where each native browser view was asked to go last frame, in the window's own points.
    ///
    /// See the note on [`Self::browser_placements`]: a native child is not drawn by Unluminous, so this is
    /// the only way a test can check a page is inside the node it belongs to.
    pub fn browser_placements(&self) -> Vec<crate::services::browser::BrowserPlacement> {
        self.browser_placements.clone()
    }

    /// Draw Unluminous's browser toolbar and reserve the rest of the pane for its native child view.
    pub(crate) fn show_browser(&mut self, ui: &mut egui::Ui, area: Rect, focused: bool) -> bool {
        let Some(tab) = self.files.active().browser.clone() else { return false };
        let showing = self.browser.showing().is_none_or(|id| id == tab.id);
        // Taken off the tab, handed over, and put back — the same borrow a browser node does, and for the
        // same reason: a tab that is not showing is not drawn.
        let mut typed = std::mem::take(&mut self.files.active_mut().typed_address);
        let mut editing = self.files.active().editing_address;
        let suggestions = self.suggestions_for(&typed, editing);
        let (outcome, placement) = browser_view::show(
            ui,
            area,
            browser_view::Toolbar {
                tab: Some(&tab),
                typed: &mut typed,
                editing: &mut editing,
                id: egui::Id::new(("browser-address", tab.id)),
                // Pinning one element is a browser node's feature, so a tab in the editing area offers none
                // of it. `task-2203`.
                pickable: false,
                pinned: false,
                picking: false,
                suggestions,
            },
            focused,
            showing,
        );
        self.files.active_mut().typed_address = typed;
        self.files.active_mut().editing_address = editing;
        if let Some(placement) = placement {
            self.browser_placements.push(placement);
        }
        if let Some(command) = outcome.command {
            self.run_browser_command(tab.id, command);
        }
        if outcome.took_focus {
            self.focus = Focus::Editor;
        }
        outcome.took_focus
    }

    /// The visited addresses to offer under an address field holding `typed`. Nothing while the field is the
    /// page's rather than the person's, because a list that opens over a page nobody is typing at is in the way.
    pub(crate) fn suggestions_for(
        &self,
        typed: &str,
        editing: bool,
    ) -> Vec<browser_view::Suggestion> {
        if !editing {
            return Vec::new();
        }
        self.visits
            .matching(typed, crate::services::browser_session::MOST_OFFERED)
            .into_iter()
            .map(|visit| browser_view::Suggestion {
                url: visit.url.clone(),
                title: visit.title.clone(),
            })
            .collect()
    }

    /// Open a validated address or local HTML file through the same path used by menus and CLI.
    pub fn open_browser(&mut self, value: &str) -> Result<u64, String> {
        if !crate::services::browser::SUPPORTED {
            return Err("Rendered web tabs are available on Windows and macOS.".to_owned());
        }
        let location = BrowserLocation::parse(value, self.tree.root())?;
        let tab = self.browser.open_tab(location);
        let id = tab.id;
        self.files.open_file(files::OpenFile::browser(tab), true);
        self.focus = Focus::Editor;
        self.message = Some("Opened in a browser tab".to_owned());
        Ok(id)
    }

    /// Open the browser tabs a project left open, each in its pane and with the addresses it had been at.
    ///
    /// `task-2203`: *"we don't seem to be retaining session history."* A browser tab in the editing area was
    /// not written down at all, so a project reopened without it. A tab whose address no longer opens is
    /// skipped, which is the rule a file that has gone keeps.
    pub(crate) fn restore_browser_tabs(
        &mut self,
        tabs: &[crate::services::project_state::RememberedBrowser],
    ) {
        if !crate::services::browser::SUPPORTED {
            return;
        }
        for remembered in tabs {
            let Some(current) = remembered.current() else { continue };
            let Ok(location) = BrowserLocation::parse(current, self.tree.root()) else { continue };
            let mut tab = self.browser.open_tab(location);
            tab.restore_history(&remembered.history, remembered.position);
            let index = self.files.open_file(files::OpenFile::browser(tab), true);
            let pane = remembered.pane.min(self.files.pane_count().saturating_sub(1));
            if self.files.pane_of(index) != pane {
                let end = self.files.tabs_in(pane).len();
                self.files.drag_tab(index, pane, end);
            }
        }
    }

    /// Run one browser command against the tab that asked for it.
    ///
    /// `Back` and `Forward` are answered from the tab's own history rather than from the shared
    /// view's, so a tab never steps into a page another tab visited. See [`BrowserTab::step`].
    pub fn run_browser_command(&mut self, id: u64, command: BrowserCommand) {
        let step = match command {
            BrowserCommand::Reload => {
                if let Err(problem) = self.browser.reload(id) {
                    self.message = Some(problem);
                }
                return;
            }
            // **An address typed into the toolbar's own field.** It goes wherever the tab is: a node's
            // tab through `send_a_realm_browser_to`, which keeps the node's history for a remote address
            // and opens a fresh tab for a local one, and the editing area's through `open_browser`.
            // Both resolve through `BrowserLocation::parse` rather than handing the text to the view.
            BrowserCommand::Go(address) => {
                let answer = match self.realm.live.node_of_browser(id) {
                    Some(node) => self.send_a_realm_browser_to(node, address.trim()),
                    None => self.open_browser(address.trim()).map(|_| ()),
                };
                if let Err(problem) = answer {
                    self.message = Some(problem);
                }
                return;
            }
            // **The element picker and the pin, which only a browser node has.** `task-2203`.
            BrowserCommand::Pick | BrowserCommand::CancelPick | BrowserCommand::Unpin => {
                let Some(node) = self.realm.live.node_of_browser(id) else { return };
                let answer = match command {
                    BrowserCommand::Pick => self.pick_in_a_realm_browser(node, None),
                    BrowserCommand::CancelPick => {
                        self.browser.cancel_picking();
                        Ok(())
                    }
                    _ => self.unpin_a_realm_browser(node),
                };
                if let Err(problem) = answer {
                    self.message = Some(problem);
                }
                return;
            }
            BrowserCommand::Back => self.browser_tab(id).and_then(|tab| tab.step(true)),
            BrowserCommand::Forward => self.browser_tab(id).and_then(|tab| tab.step(false)),
        };
        let Some((position, url)) = step else {
            self.message = Some("There is nowhere for this tab to go that way.".to_owned());
            return;
        };
        match self.browser.navigate(id, &url) {
            Ok(()) => self.change_browser_tab(id, |tab| tab.heading_for(position)),
            Err(problem) => self.message = Some(problem),
        }
    }

    /// The rendered tab with this id, wherever it is open.
    ///
    /// **Two places, asked in one function**, which is `follow_the_open_file`'s rule: a tab is either in
    /// the editing area or on a browser **node** on the canvas, and a list of the places that have to
    /// remember to look on the canvas as well is a list whose next entry is the one that forgets. It is
    /// the reading half of what `raw_input_hook` already does for the placements, where the canvas's tabs
    /// join the list the editing area's are reconciled in.
    ///
    /// It forgot, and `task-1905` is the report. A node's tab was never found, so its title never
    /// arrived, `loading` was set once and never cleared, and `Back` answered *"There is nowhere for this
    /// tab to go that way"* however many pages had been visited — its history had one entry in it because
    /// nothing had ever added a second.
    ///
    /// An id is unique across both, because `BrowserHost` hands them out from one counter, so "the
    /// editing area first, then the canvas" cannot answer with the wrong tab.
    fn browser_tab(&self, id: u64) -> Option<&BrowserTab> {
        self.files
            .iter()
            .filter_map(|file| file.browser.as_ref())
            .find(|tab| tab.id == id)
            .or_else(|| self.realm.live.browsers().find(|tab| tab.id == id))
    }

    /// Apply browser callbacks to ordinary tab state and open requested popup URLs as Unluminous tabs.
    pub(crate) fn receive_browser_events(&mut self) {
        for id in self.browser.reload_changed_local_tabs() {
            let _ = self.browser.reload(id);
        }
        let arrived = self.browser.take_events();
        self.act_on_browser_events(arrived);
        // What the page's own right click menu offers, for the page showing now. `task-2203`.
        let node = self.browser.showing().and_then(|id| self.realm.live.node_of_browser(id));
        let pinned = node.is_some_and(|node| self.a_nodes_pin(node).is_some());
        self.browser
            .set_page_menu(crate::services::browser::PageMenu { pickable: node.is_some(), pinned });
        // A pick asked for while another page held the one view, opened once the view is on this node's page.
        if let Some((node, at)) = self.pick_when_showing {
            match self.realm.live.browser(node) {
                Some(tab) if self.browser.showing() == Some(tab.id) && !tab.loading => {
                    self.pick_when_showing = None;
                    if let Err(problem) = self.browser.start_picking(tab.id, at) {
                        self.message = Some(problem);
                    }
                }
                Some(_) => {}
                None => self.pick_when_showing = None,
            }
        }
    }

    /// What each event does, split out so a test can feed the four shapes with no engine behind them.
    ///
    /// `task-1905`: every one of these reached `change_browser_tab`, which walked `self.files` alone, so
    /// none of them reached a browser **node** on the canvas. A test could not have caught it, because
    /// there was no way to hand the window an event without a real WebView2 or WKWebView answering.
    pub fn act_on_browser_events(&mut self, events: Vec<BrowserEvent>) {
        for event in events {
            match event {
                BrowserEvent::OpenRequested { url, .. } => {
                    let _ = self.open_browser(&url);
                }
                BrowserEvent::PickAsked { id, at } => {
                    if let Some(node) = self.realm.live.node_of_browser(id) {
                        if let Err(problem) = self.pick_in_a_realm_browser(node, at) {
                            self.message = Some(problem);
                        }
                    }
                }
                BrowserEvent::UnpinAsked { id } => {
                    if let Some(node) = self.realm.live.node_of_browser(id) {
                        if let Err(problem) = self.unpin_a_realm_browser(node) {
                            self.message = Some(problem);
                        }
                    }
                }
                BrowserEvent::Picked { id, selector } => {
                    if let Some(node) = self.realm.live.node_of_browser(id) {
                        match self.pin_a_realm_browser(node, &selector) {
                            Ok(()) => {
                                self.message = Some(format!("Node {node} shows only {selector}."))
                            }
                            Err(problem) => self.message = Some(problem),
                        }
                    }
                }
                BrowserEvent::PickCancelled { .. } => {}
                BrowserEvent::Title { id, title } => {
                    // A video node's page says where it is in its title, which is how the position reaches
                    // the realm's sidecar with no script callback. `task-2202`.
                    if let Some(node) = self.realm.live.node_of_browser(id) {
                        if let Some(report) = crate::services::browser::read_a_video_title(&title) {
                            self.realm.live.report_a_video(node, report);
                        }
                    }
                    // **A title that arrives while a page is loading is the next page's**, and the tab's
                    // address still names the last one until the load finishes, so it is held until then.
                    // `task-2203`: filed at once, the second page of a visit was written down with the first
                    // page's title.
                    match self
                        .browser_tab(id)
                        .map(|tab| (tab.loading, tab.current_url().to_owned()))
                    {
                        Some((true, _)) => {
                            self.titles_arriving.insert(id, title.clone());
                        }
                        Some((false, url)) => self.visits.titled(&url, &title),
                        None => {}
                    }
                    self.change_browser_tab(id, |tab| tab.title = title)
                }
                BrowserEvent::LoadStarted { id, .. } => {
                    self.titles_arriving.remove(&id);
                    self.change_browser_tab(id, |tab| tab.loading = true)
                }
                BrowserEvent::LoadFinished { id, url } => {
                    self.change_browser_tab(id, |tab| tab.arrived_at(url));
                    // Where a person has been, kept for every project. `task-2203`.
                    if let Some(tab) = self.browser_tab(id).filter(|tab| !tab.loading) {
                        let now = tab.current_url().to_owned();
                        self.visits.visited(&now, crate::services::browser_session::now());
                        if let Some(title) = self.titles_arriving.remove(&id) {
                            self.visits.titled(&now, &title);
                        }
                    }
                    // And a node showing one element of this page shows it again.
                    if let Some(node) = self.realm.live.node_of_browser(id) {
                        self.show_a_nodes_pin(node);
                    }
                }
            }
        }
    }

    /// Change one browser tab without exposing native state to the file collection.
    ///
    /// The writing half of [`Self::browser_tab`], and it looks in the same two places for the same
    /// reason. `task-1905`.
    pub(crate) fn change_browser_tab(&mut self, id: u64, change: impl FnOnce(&mut BrowserTab)) {
        if let Some(tab) =
            self.files.iter_mut().filter_map(|file| file.browser.as_mut()).find(|tab| tab.id == id)
        {
            change(tab);
            return;
        }
        if let Some(node) = self.realm.live.node_of_browser(id) {
            if let Some(tab) = self.realm.live.browser_mut(node) {
                change(tab);
            }
        }
    }

    /// Where an egui surface will sit above every native child view in this frame.
    ///
    /// A native child view is a window rather than a painted rectangle, so nothing egui draws can be
    /// on top of one: a page under a menu has to be **hidden** while the menu is open. What changed in
    /// `task-2009` is *which* page. This used to answer yes or no for the whole window — any popup
    /// anywhere hid every page — so opening the branch picker in the title bar blanked a page in a pane
    /// at the other end of the window, which is the report: *"When I select a branch, a dropdown comes
    /// down, and all of a sudden the url tab just shows the background image rather than the page i was
    /// viewing."* It answers with the rectangles now, and `services::browser::choose` hides a page only
    /// when one of them is really over it.
    ///
    /// **It is read off egui's own layers rather than from a list of Unluminous's menus.** Every popup,
    /// every dropdown, every flyout, every context menu, the completion list and the value tooltip is
    /// an `egui::Area` above the background layer, and an area records where it was drawn — so one walk
    /// covers all of them and covers the next one added with no change here. The background layer is
    /// left out because that is where the window itself is drawn, and a canvas node's layer is a
    /// background layer too, which is what makes a page inside a node not occlude itself.
    ///
    /// A modal is the one whole-window answer that stays: `egui::Modal` dims everything behind it, so
    /// there is nowhere on the screen a page could honestly be drawn.
    pub(crate) fn occluding_rects(&self, ctx: &egui::Context) -> Vec<Rect> {
        if a_modal_has_the_keyboard(ctx) {
            return vec![ctx.content_rect()];
        }
        ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|layer| layer.order != egui::Order::Background)
                .filter_map(|layer| memory.area_rect(layer.id))
                .filter(|rect| rect.is_positive())
                .collect()
        })
    }
}
