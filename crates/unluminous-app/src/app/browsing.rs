//! The browser tabs' half of the window.
//!
//! Since `task-2270` every rendered tab has a native child view of its own, made the first time its page is
//! placed; `services::browser` records what was measured. Everything here either places a page or carries a
//! command to one.

use egui::Rect;

use crate::components::browser_view;
use crate::services::browser::{BrowserCommand, BrowserEvent, BrowserLocation, BrowserTab};

use crate::app::files;
use crate::app::{a_modal_has_the_keyboard, Focus, UnluminousApp};

/// How long a pane's size has to stand still before the live page is given it.
const RESIZE_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// How long the live page is left cut away at its new size before it is shown, so it is drawn at that size
/// first. The canvas's own `PAGE_SETTLE`, for the same reason.
const SHOW_SETTLE: std::time::Duration = std::time::Duration::from_millis(160);

/// The longest a switched-to tab's picture stands in for its page while the page reloads.
const SWITCH_LONGEST: std::time::Duration = std::time::Duration::from_secs(3);

/// What a browser tab in a pane shows while its live page is not worth showing (`task-2255`).
///
/// *"the resize is not smooth at all. it's very chunky"* and *"switching between tabs ... seems a bit slow and
/// clunky"*. A window has one native view, so showing another browser tab reloads that tab's page into it, and
/// for the length of the load the pane showed a blank page. And a page is laid out again by the engine on its own
/// process after every change of size, so on every frame of a resize it was drawn at a size a few frames old.
/// Both are answered the way the canvas answers a zoom (`task-2253`): a picture of the page, taken with
/// `BrowserHost::photograph`, is drawn in the pane, the native view is cut away, and the live page comes back
/// once the load has finished or the size has stood still.
#[derive(Default)]
pub struct PageStills {
    /// The last picture taken of each tab's page.
    pictures: std::collections::HashMap<u64, egui::TextureHandle>,
    /// The page area each tab was last drawn in, so a frame can tell its pane is being resized.
    areas: std::collections::HashMap<u64, Rect>,
    /// The tab being held, and why.
    held: Option<Held>,
    /// The pages a popup is over, each with the picture taken of it just before the popup was cut out of it.
    /// See [`UnluminousApp::stand_a_picture_under_what_covers_the_page`].
    covered: std::collections::HashMap<u64, egui::TextureHandle>,
}

/// One tab whose picture is standing in for its page.
struct Held {
    tab: u64,
    /// When the pane last changed size, or when the tab was switched to.
    since: std::time::Instant,
    /// True while a reload after a switch is what is being waited for, rather than a resize.
    switching: bool,
    /// The area the native view is kept at while the size moves, so the engine lays nothing out.
    area: Rect,
    /// When the live page was given its new size, still cut away.
    released: Option<std::time::Instant>,
}

impl PageStills {
    /// Remember a picture of a tab's page.
    pub fn keep(&mut self, tab: u64, picture: egui::TextureHandle) {
        self.pictures.insert(tab, picture);
    }

    /// Whether a picture is standing under a popup over `tab`'s page, which a test can ask.
    pub fn under_a_popup(&self, tab: u64) -> bool {
        self.covered.contains_key(&tab)
    }

    /// Forget a closed tab.
    pub fn forget(&mut self, tab: u64) {
        self.covered.remove(&tab);
        self.pictures.remove(&tab);
        self.areas.remove(&tab);
        if self.held.as_ref().is_some_and(|held| held.tab == tab) {
            self.held = None;
        }
    }

    /// Hold `tab`'s picture over it until its page has reloaded, because the view has just been sent to it.
    pub fn switched_to(&mut self, tab: u64) {
        if self.pictures.contains_key(&tab) {
            self.held = Some(Held {
                tab,
                since: std::time::Instant::now(),
                switching: true,
                area: Rect::NOTHING,
                released: None,
            });
        }
    }

    /// Whether a picture of `tab` is being drawn now, which a test and `browser status` can ask.
    pub fn holding(&self, tab: u64) -> bool {
        self.held.as_ref().is_some_and(|held| held.tab == tab)
    }
}

/// Where to draw a picture of a page in `area`, and which part of the picture, keeping the picture's own shape.
///
/// As wide as the area, cut off at the bottom when it is taller than the area and ending above the bottom when it
/// is shorter. A page is read from the top, and a picture stretched to a pane whose height changed after it was
/// taken squashed every line of text on it (`task-2263`).
pub fn a_picture_at_its_own_shape(picture: egui::Vec2, area: Rect) -> (Rect, Rect) {
    let whole = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    if picture.x <= 0.0 || picture.y <= 0.0 || area.width() <= 0.0 {
        return (area, whole);
    }
    let tall = area.width() * picture.y / picture.x;
    match tall > area.height() {
        true => {
            (area, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, area.height() / tall)))
        }
        false => (Rect::from_min_size(area.min, egui::vec2(area.width(), tall)), whole),
    }
}

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
        // **The pane with the keyboard, and the keyboard in the editing area** (`task-2263`). `focused` says only
        // that this is the focused one of the editing panes, which stays true while the Designer holds the
        // keyboard, so the tab and the Designer's page were both focused placements and the tab, drawn first,
        // took the one native view back on the frame after a click on the Designer.
        let focused = focused && matches!(self.focus, Focus::Editor);
        let showing = self.browser.renders(tab.id);
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
        self.zoom_the_page_in_the_pane(ui, &tab);
        if let Some(mut placement) = placement {
            // The tab's own zoom, sent with the bounds as a node's is, so the view takes it when it moves to
            // this tab. `task-2255`.
            placement.zoom = Some(tab.zoom);
            let placement = match showing {
                true => self.hold_a_still_over_the_page(ui, &tab, placement),
                false => {
                    self.paint_a_page_not_showing(ui, tab.id, placement.visible);
                    placement
                }
            };
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

    /// Draw the last picture of a page the one native view is not showing, and answer whether there was one.
    ///
    /// `task-2263`: *"I should be able to have the design html showing as well as a browser tab in file pane."* A
    /// window has one native view (see `services::browser`), so two pages side by side cannot both be live. The
    /// page that is not live shows the picture taken of it as the view left it, and pressing it moves the live
    /// view there, so both panes show their page and the one being used is the live one.
    pub(crate) fn paint_a_page_not_showing(&self, ui: &egui::Ui, tab: u64, area: Rect) -> bool {
        if self.browser.renders(tab) {
            return false;
        }
        let Some(picture) = self.page_stills.pictures.get(&tab) else { return false };
        let (place, part) = a_picture_at_its_own_shape(picture.size_vec2(), area);
        ui.painter().image(picture.id(), place, part, egui::Color32::WHITE);
        true
    }

    /// `Ctrl`/`Cmd` with the wheel over a browser tab's toolbar zooms its page (`task-2255`). Over the page
    /// itself the engine does it, because the page's window takes the wheel; the keys are the `View` menu's
    /// `Increase Font Size` and `Decrease Font Size`, which reach [`Self::step_the_page_zoom`].
    ///
    /// @param ui - the pane's `Ui`
    /// @param tab - the browser tab showing
    fn zoom_the_page_in_the_pane(&mut self, ui: &egui::Ui, tab: &BrowserTab) {
        let over = ui.ctx().pointer_hover_pos().is_some_and(|at| ui.max_rect().contains(at));
        // Claimed the way a pane claims the gesture, so the editor's font does not move with it.
        if !over || self.zoom == crate::app::ZoomClaim::Taken {
            return;
        }
        let steps = ui.ctx().input(|input| {
            let delta = input.zoom_delta();
            match () {
                _ if delta > 1.001 => 1,
                _ if delta < 0.999 => -1,
                _ => 0,
            }
        });
        if steps != 0 {
            self.zoom = crate::app::ZoomClaim::Taken;
            self.step_the_page_zoom(tab.id, steps > 0);
        }
    }

    /// Zoom a browser tab's page one step in or out.
    ///
    /// @param id - the browser tab
    /// @param larger - which way
    pub(crate) fn step_the_page_zoom(&mut self, id: u64, larger: bool) {
        self.change_browser_tab(id, |tab| {
            tab.zoom = crate::services::browser::step_page_zoom(tab.zoom, larger)
        });
    }

    /// Put a browser tab's page back at its own size.
    pub(crate) fn reset_the_page_zoom(&mut self, id: u64) {
        self.change_browser_tab(id, |tab| tab.zoom = 1.0);
    }

    /// Zoom the page that posted its modifier wheel or zoom keys (`task-2336`): the node it is on, through
    /// [`Self::zoom_a_node`] as the modifier wheel over the node's toolbar does, or the tab in a pane.
    ///
    /// @param id - the browser tab whose page asked
    /// @param steps - steps in when positive, out when negative, and back to its own size at `0`
    pub(crate) fn zoom_the_page_that_asked(&mut self, id: u64, steps: i32) {
        match (self.realm.live.node_of_browser(id), steps) {
            // The placement sends the node's zoom times the camera's on the next frame, so only the node's
            // own number is set here.
            (Some(node), 0) => self.realm.live.set_page_zoom(node, 1.0),
            (Some(node), steps) => self.zoom_a_node(node, steps),
            (None, 0) => self.reset_the_page_zoom(id),
            (None, steps) => {
                for _ in 0..steps.unsigned_abs() {
                    self.step_the_page_zoom(id, steps > 0);
                }
            }
        }
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }

    /// How large a browser tab draws its page.
    pub fn browser_zoom_of(&self, id: u64) -> f64 {
        self.browser_tab(id).map(|tab| tab.zoom).unwrap_or(1.0)
    }

    /// Draw a tab's picture in place of its page while its pane is resized or its page reloads after a switch,
    /// and answer the placement the native view is to have. See [`PageStills`].
    ///
    /// @param ui - the pane's `Ui`
    /// @param tab - the browser tab
    /// @param placement - where the page would be placed with nothing held
    fn hold_a_still_over_the_page(
        &mut self,
        ui: &egui::Ui,
        tab: &BrowserTab,
        placement: crate::services::browser::BrowserPlacement,
    ) -> crate::services::browser::BrowserPlacement {
        let now = std::time::Instant::now();
        let area = placement.area;
        let before = self.page_stills.areas.insert(tab.id, area);
        let resized = before.is_some_and(|was| (was.size() - area.size()).length() > 0.5);
        if resized {
            match self.page_stills.held.as_mut().filter(|held| held.tab == tab.id) {
                Some(held) => {
                    held.since = now;
                    held.switching = false;
                    held.released = None;
                }
                None => {
                    // The picture is taken from the page as it is, on the first frame of the resize.
                    if let Some(picture) = self.browser.photograph(tab.id) {
                        let texture = crate::services::picture::upload(
                            ui.ctx(),
                            "pane-page-still".to_owned(),
                            picture,
                            egui::TextureOptions::LINEAR,
                        );
                        self.page_stills.keep(tab.id, texture);
                    }
                    if self.page_stills.pictures.contains_key(&tab.id) {
                        self.page_stills.held = Some(Held {
                            tab: tab.id,
                            since: now,
                            switching: false,
                            area: before.unwrap_or(area),
                            released: None,
                        });
                    }
                }
            }
        }
        let Some(held) = self.page_stills.held.as_mut().filter(|held| held.tab == tab.id) else {
            return placement;
        };
        let Some(picture) = self.page_stills.pictures.get(&tab.id).cloned() else {
            self.page_stills.held = None;
            return placement;
        };
        // Whether whatever the picture is standing in for is over.
        let done = match held.switching {
            true => !tab.loading || now.duration_since(held.since) > SWITCH_LONGEST,
            false => now.duration_since(held.since) > RESIZE_SETTLE,
        };
        let mut placed = placement.clone();
        if !done {
            if !held.switching && held.area.is_positive() {
                placed.area = held.area;
                placed.visible = held.area;
            }
        } else if held.released.is_none() {
            held.released = Some(now);
        } else if held.released.is_some_and(|at| now.duration_since(at) > SHOW_SETTLE) {
            self.page_stills.held = None;
            ui.ctx().request_repaint();
            return placement;
        }
        let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
        ui.painter().image(picture.id(), placement.visible, uv, egui::Color32::WHITE);
        placed.over.push(placed.visible);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        placed
    }

    /// Paint a picture of the page under a popup that is over it, so the popup's shadow lands on the page.
    ///
    /// `task-2277`: *"When I right click the tab over a opened web page, theres an odd underlay for the popup
    /// that shows. It should not have that ugly black box underlay."* A page is a native child window, so a popup
    /// over it is cut out of it, grown by `OCCLUDER_SHADOW` so the popup's shadow is not hidden under the page.
    /// Nothing egui drew was under that hole, so the menu sat in a square of the window's own dark ground,
    /// eighteen points larger than the menu on every side.
    ///
    /// So on the frame a popup first lands on the page, the page is photographed while it is still whole, which
    /// is before `BrowserHost::follow` cuts the popup out of it, and the picture is painted where the page is for
    /// as long as anything is over it. The hole then shows the page as it was, with the popup's shadow on it. The
    /// picture is painted on the background layer at the end of the frame, over the pane and under every popup.
    /// It is taken again the next time a popup arrives, so it is never older than the popup over it.
    ///
    /// A page that is one colour all over, such as a new tab's empty page, is kept as its picture too
    /// (`task-2320`). Where the platform cannot photograph a page (macOS), the editing area's ground is
    /// painted there instead, so the hole round the popup never shows the background picture.
    ///
    /// @param ctx - the frame's context
    /// @param occluders - what egui is drawing over the native views this frame
    pub(crate) fn stand_a_picture_under_what_covers_the_page(
        &mut self,
        ctx: &egui::Context,
        occluders: &[Rect],
    ) {
        // **Every page a popup is over**, since every placed page is live (`task-2270`).
        let covered: Vec<(u64, Rect)> = self
            .browser_placements
            .iter()
            .filter(|page| {
                self.browser.renders(page.id)
                    && crate::services::browser::covers_any_of(occluders, page.visible)
                    && !self.page_stills.holding(page.id)
            })
            .map(|page| (page.id, page.visible))
            .collect();
        self.page_stills.covered.retain(|id, _| covered.iter().any(|(page, _)| page == id));
        for (id, visible) in covered {
            if !self.page_stills.under_a_popup(id) {
                // A page that cannot be photographed (macOS) gets the editing area's own ground, opaque,
                // so the hole round the popup never shows the background picture through it, and the
                // page is not asked again on every frame the popup is open. `task-2320`.
                let picture = self.browser.photograph_even_if_plain(id).unwrap_or_else(|| {
                    egui::ColorImage::new([1, 1], vec![crate::theme::color::editor().to_opaque()])
                });
                let texture = crate::services::picture::upload(
                    ctx,
                    "covered-page-still".to_owned(),
                    picture,
                    egui::TextureOptions::LINEAR,
                );
                self.page_stills.covered.insert(id, texture);
            }
            let Some(picture) = self.page_stills.covered.get(&id) else { continue };
            let (place, part) = a_picture_at_its_own_shape(picture.size_vec2(), visible);
            ctx.layer_painter(egui::LayerId::background()).with_clip_rect(visible).image(
                picture.id(),
                place,
                part,
                egui::Color32::WHITE,
            );
        }
    }

    /// Make the tab or node holding page `id` the one with the keyboard, because somebody clicked into the page.
    ///
    /// A click on a page goes to the page's own window, so nothing in egui saw it and nothing chose the tab or
    /// the node it is in. Without this the next frame's placement said the page was not the chosen one and gave
    /// the operating system's keyboard back to the window. `task-2255`.
    ///
    /// @param id - the browser tab whose page was clicked
    pub(crate) fn give_the_keyboard_to_the_page(&mut self, id: u64) {
        let index = self
            .files
            .iter()
            .position(|file| file.browser.as_ref().is_some_and(|tab| tab.id == id));
        if let Some(index) = index {
            let pane = self.files.pane_of(index);
            self.files.focus_pane(pane);
            self.focus = Focus::Editor;
            return;
        }
        if let Some(node) = self.realm.live.node_of_browser(id) {
            self.realm.realm.choose(Some(node));
            self.focus = Focus::Realm;
            return;
        }
        // A design's page is in the Designer pane, so the pane holds the keyboard. `task-2260`.
        if self.design_tabs.values().any(|tab| tab.id == id) {
            self.focus = Focus::Plugin;
            self.plugin_with_the_keyboard = Some("designer".to_owned());
        }
    }

    /// Carry one of the `Edit` menu's six entries to the page the active browser tab shows.
    ///
    /// The page's own `document.execCommand` does what the browser's own menu would: it acts on the field or
    /// the selection that has the page's focus. A paste reads Unluminous's clipboard and inserts it as text,
    /// because a script cannot read the clipboard without asking the person first. `task-2255`.
    ///
    /// @param action - which entry was chosen
    pub(crate) fn edit_in_the_page(&mut self, action: crate::app::actions::Action) {
        use crate::app::actions::Action;
        let Some(id) = self.files.active().browser.as_ref().map(|tab| tab.id) else { return };
        let script = match action {
            Action::Undo => "document.execCommand('undo')".to_owned(),
            Action::Redo => "document.execCommand('redo')".to_owned(),
            Action::Cut => "document.execCommand('cut')".to_owned(),
            Action::Copy => "document.execCommand('copy')".to_owned(),
            Action::SelectAll => "document.execCommand('selectAll')".to_owned(),
            Action::Paste => {
                let Ok(text) = arboard::Clipboard::new().and_then(|mut board| board.get_text())
                else {
                    return;
                };
                let text = serde_json::to_string(&text).unwrap_or_default();
                format!("document.execCommand('insertText', false, {text})")
            }
            _ => return,
        };
        if let Err(problem) = self.browser.evaluate(id, &script) {
            self.message = Some(problem);
        }
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
        self.show_the_editing_area_for_a_tab();
        self.focus = Focus::Editor;
        self.message = Some("Opened in a browser tab".to_owned());
        Ok(id)
    }

    /// Open an empty rendered tab in the pane with the keyboard, with its address field holding the keyboard.
    ///
    /// `task-2270`: *"Right click in top space or on existing tab -> open new browser tab."* The tab is on
    /// [`crate::services::browser::BLANK`] and its field is the person's from the start, so what is typed is
    /// never overwritten by the page's own address.
    pub fn open_a_new_browser_tab(&mut self) -> Result<u64, String> {
        if !crate::services::browser::SUPPORTED {
            return Err("Rendered web tabs are available on Windows and macOS.".to_owned());
        }
        let tab = self
            .browser
            .open_tab(BrowserLocation::Remote { url: crate::services::browser::BLANK.to_owned() });
        let id = tab.id;
        self.files.open_file(files::OpenFile::browser(tab), true);
        self.files.active_mut().typed_address.clear();
        self.files.active_mut().editing_address = true;
        self.show_the_editing_area_for_a_tab();
        self.focus = Focus::Editor;
        if let Some(ctx) = &self.context {
            let field = egui::Id::new(("browser-address", id));
            ctx.data_mut(|data| {
                data.insert_temp(crate::components::controls::wants_the_keyboard(), field)
            });
        }
        self.message = Some("Opened a new browser tab".to_owned());
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

    /// Open the page a link in page `source` asked for: in a new tab beside that tab in its own pane, or, for a
    /// page on a browser node, in that node itself. `task-2270`: *"Click on page links should open new tabs by
    /// default."* A link with `target="_blank"` and `window.open` arrive the same way.
    ///
    /// **A browser node never opens another node for a link** (`task-2325`: *"in Realm, a browser link press should
    /// not open a new node, instead, just navigate like normal. new tab opening should only be for file editor view
    /// browser tabs."*). The node goes to the page the way its own address bar sends it, so Back returns.
    pub fn open_a_page_a_link_asked_for(&mut self, source: u64, url: &str) -> Result<(), String> {
        if let Some(node) = self.realm.live.node_of_browser(source) {
            return self.send_a_realm_browser_to(node, url);
        }
        // The pane the link was clicked in is the pane the new tab opens in.
        let index = self
            .files
            .iter()
            .position(|file| file.browser.as_ref().is_some_and(|tab| tab.id == source));
        if let Some(index) = index {
            self.show_tab(index);
        }
        self.open_browser(url).map(|_| ())
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
            .or_else(|| self.design_tabs.values().find(|tab| tab.id == id))
    }

    /// Apply browser callbacks to ordinary tab state and open requested popup URLs as Unluminous tabs.
    pub(crate) fn receive_browser_events(&mut self) {
        for id in self.browser.reload_changed_local_tabs() {
            let _ = self.browser.reload(id);
        }
        let arrived = self.browser.take_events();
        self.act_on_browser_events(arrived);
        // What each browser node's page offers on its own right click menu. `task-2203`. A tab in the editing area
        // offers nothing, which is the menu's default.
        let nodes: Vec<u64> = self.realm.live.browsers().map(|tab| tab.id).collect();
        for id in nodes {
            let Some(node) = self.realm.live.node_of_browser(id) else { continue };
            let pinned = self.a_nodes_pin(node).is_some();
            self.browser
                .set_page_menu(id, crate::services::browser::PageMenu { pickable: true, pinned });
        }
        // A pick asked for before the node's page had a view, opened once it has one and has loaded.
        if let Some((node, at)) = self.pick_when_showing {
            match self.realm.live.browser(node) {
                Some(tab) if self.browser.is_live(tab.id) && !tab.loading => {
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
                BrowserEvent::OpenRequested { source, url } => {
                    if let Err(problem) = self.open_a_page_a_link_asked_for(source, &url) {
                        self.message = Some(problem);
                    }
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
                // The page's own modifier wheel or zoom keys. `task-2336`.
                BrowserEvent::ZoomAsked { id, steps } => self.zoom_the_page_that_asked(id, steps),
                // What a design's overlay script said, which the Designer reads. `task-2254`.
                BrowserEvent::Reported { id, json } => {
                    self.the_page_of_a_design_reported(id, &json)
                }
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
                    //
                    // **And the tab keeps the name it had until the load finishes** (`task-2255`: *"The browser
                    // tabs get renamed when I click on/off of them"*). A window has one native view, so showing a
                    // tab sends the view to that tab's page, and on the way the engine names the page it is
                    // leaving and then the bare address of the one it is loading. Taken at once, each of those
                    // renamed the tab for a moment.
                    match self
                        .browser_tab(id)
                        .map(|tab| (tab.loading, tab.current_url().to_owned()))
                    {
                        Some((true, _)) => {
                            self.titles_arriving.insert(id, title.clone());
                        }
                        Some((false, url)) => {
                            self.visits.titled(&url, &title);
                            self.change_browser_tab(id, |tab| tab.title = title);
                        }
                        None => {}
                    }
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
                            self.change_browser_tab(id, |tab| tab.title = title);
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
            return;
        }
        if let Some(tab) = self.design_tabs.values_mut().find(|tab| tab.id == id) {
            change(tab);
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

#[cfg(test)]
mod pictures {
    use super::*;

    /// `task-2263`: a picture keeps its shape in a pane whose height changed after it was taken.
    #[test]
    fn a_picture_keeps_its_shape_in_a_shorter_or_taller_area() {
        let area = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(300.0, 200.0));
        let (place, part) = a_picture_at_its_own_shape(egui::vec2(300.0, 400.0), area);
        assert_eq!(place, area, "a taller picture fills the area");
        assert!((part.max.y - 0.5).abs() < 1e-4, "and is cut at the bottom: {part:?}");
        let (place, part) = a_picture_at_its_own_shape(egui::vec2(600.0, 200.0), area);
        assert!(
            (place.height() - 100.0).abs() < 1e-4,
            "a wider picture ends above the bottom: {place:?}"
        );
        assert_eq!(part.max, egui::pos2(1.0, 1.0));
    }
}
