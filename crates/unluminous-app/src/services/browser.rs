//! Rendered web pages, their constrained local origin, and the native views that show them.
//!
//! The tab is ordinary application state and the native browser is not. `BrowserTab` can therefore
//! be tested with no window, while `BrowserHost` owns WebView2 or WKWebView and creates either only
//! when a rendered tab is visible. Local pages are served through a custom origin rooted at the
//! project, so linked assets work without giving page JavaScript a filesystem handle.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::time::{Duration, Instant};

use egui::Rect;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use url::Url;

use crate::services::file_kind;

/// The browser engines Unluminous embeds on the platforms it ships.
pub const SUPPORTED: bool = cfg!(any(windows, target_os = "macos"));

/// Where a browser tab was asked to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserLocation {
    Local { path: PathBuf, root: PathBuf },
    Remote { url: String },
}

impl BrowserLocation {
    /// Resolve a web address or HTML path against the project and validate it before a tab exists.
    pub fn parse(value: &str, project: &Path) -> Result<Self, String> {
        let value = value.trim();
        if value.is_empty() {
            return Err("Say which HTML file or web address to open.".to_owned());
        }
        let path = Path::new(value);
        if path.is_absolute() || project.join(path).is_file() {
            return Self::local(value, project);
        }
        if let Ok(url) = Url::parse(value) {
            return match url.scheme() {
                "http" | "https" => Ok(Self::Remote { url: url.to_string() }),
                scheme => Err(format!(
                    "Unluminous opens HTTP and HTTPS addresses, not {scheme} addresses."
                )),
            };
        }
        // `example.com` and `example.com/page` are addresses a person and a model both write with no
        // scheme, and neither is a file in the project. Reading them as a missing file would answer
        // the wrong question; an HTML name that happens to have a dot in it is still read as a file.
        if let Some(url) = implied_address(value) {
            return Ok(Self::Remote { url });
        }
        Self::local(value, project)
    }

    /// Resolve one local HTML file and choose the folder its root-relative resources belong to.
    fn local(value: &str, project: &Path) -> Result<Self, String> {
        let given = PathBuf::from(value);
        let candidate = if given.is_absolute() { given } else { project.join(given) };
        // **Plain, because `canonicalize` on Windows answers with a verbatim path.** That form is the
        // one `unluminous_terminal::paths` exists to stop travelling: nothing inside Unluminous
        // notices one, so it reaches whatever is handed a path next. `task-2009` measured that at the
        // explorer — the tree's own rows are plain, so a rendered tab's file never matched a row and
        // opening a page selected nothing.
        let path =
            candidate.canonicalize().map(|path| unluminous_terminal::paths::plain(&path)).map_err(
                |problem| format!("Unluminous could not open {}: {problem}", candidate.display()),
            )?;
        if !path.is_file() || !file_kind::is_html(&path) {
            return Err(format!("{} is not an HTML file.", path.display()));
        }
        let project = project
            .canonicalize()
            .map(|path| unluminous_terminal::paths::plain(&path))
            .unwrap_or_else(|_| project.to_path_buf());
        let root = if path.starts_with(&project) {
            project
        } else {
            path.parent().map(Path::to_path_buf).unwrap_or_else(|| path.clone())
        };
        Ok(Self::Local { path, root })
    }

    /// The URL handed to the native browser for this tab.
    pub fn initial_url(&self, id: u64) -> String {
        match self {
            Self::Remote { url } => url.clone(),
            Self::Local { path, root } => local_url(id, path.strip_prefix(root).unwrap_or(path)),
        }
    }

    /// The local file represented by this tab, when it has one.
    pub fn source_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path, .. } => Some(path),
            Self::Remote { .. } => None,
        }
    }
}

/// State that belongs to a rendered tab rather than to its native child view.
///
/// Every rendered tab in a window shares one native view (see [`BrowserHost`]), so where a tab has
/// been is remembered here rather than read back out of the engine: two tabs sharing one view would
/// otherwise share one history, and `Back` on the second would land on the first one's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserTab {
    pub id: u64,
    pub location: BrowserLocation,
    pub title: String,
    /// Every address this tab has been at, oldest first, with `position` saying which one it is on.
    history: Vec<String>,
    position: usize,
    /// The address this tab has asked the shared view for, until that page arrives.
    awaiting: Option<Awaited>,
    pub loading: bool,
    pub problem: Option<String>,
}

/// An address a tab has asked the shared view for, and where arriving there leaves it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Awaited {
    url: String,
    /// Where in this tab's history the page belongs, when a step asked for it.
    position: Option<usize>,
    /// Whether this is a **new** address this tab was sent to, rather than a step along its own history.
    ///
    /// **What it decides is whether a redirect is followed.** A page that arrives at a different address from
    /// the one asked for is a redirect when the tab typed an address, and is the view reporting the page it is
    /// *leaving* when the tab took a step or was switched to — those two cases look identical from here and
    /// must not be treated the same. See the redirect branch in [`BrowserTab::arrived_at`]. `task-1907`.
    typed: bool,
}

impl BrowserTab {
    /// Create the testable state for a browser tab before a native view is needed.
    pub fn new(id: u64, location: BrowserLocation) -> Self {
        let first = location.initial_url(id);
        Self {
            id,
            title: String::new(),
            location,
            history: vec![first],
            position: 0,
            awaiting: None,
            loading: true,
            problem: None,
        }
    }

    /// The address this tab is showing.
    pub fn current_url(&self) -> &str {
        self.history.get(self.position).map(String::as_str).unwrap_or_default()
    }

    /// Whether this tab has somewhere of its own to go back to.
    pub fn can_go_back(&self) -> bool {
        self.position > 0
    }

    /// Whether this tab has been forward of here.
    pub fn can_go_forward(&self) -> bool {
        self.position + 1 < self.history.len()
    }

    /// The address one step in the given direction, and the position it would leave the tab at.
    ///
    /// `Back` and `Forward` are answered from this tab's own list rather than from the shared view's,
    /// which is what keeps one tab's history out of another's.
    pub fn step(&self, back: bool) -> Option<(usize, String)> {
        let to = match back {
            true => self.position.checked_sub(1)?,
            false => (self.position + 1 < self.history.len()).then_some(self.position + 1)?,
        };
        Some((to, self.history[to].clone()))
    }

    /// Take a step this tab asked for, so the page that arrives is not read as a new address.
    pub fn heading_for(&mut self, position: usize) {
        let url = self.history[position].clone();
        self.awaiting = Some(Awaited { url, position: Some(position), typed: false });
        self.loading = true;
    }

    /// Record an address this tab has been sent to, whether or not the shared view can go there now.
    ///
    /// **A window has one native view**, so `BrowserHost::navigate` refuses a tab that is not the one
    /// showing. Before `task-1907` that refusal threw the address away while the *node* recorded it, so a
    /// canvas with two browser nodes held an address its page had never reached — and `space.conf` was
    /// written from that. What a tab knows is where it *should* be; the view is sent there when this tab is
    /// the one rendering, which is what [`Self::pointed_at`] already does for a tab being switched to.
    ///
    /// This is [`Self::arrived_at`]'s push half without the arrival: whatever was ahead of the current
    /// position is dropped, because typing an address is a new branch of history rather than a step along
    /// the one that is there.
    pub fn heading_for_a_new_page(&mut self, url: &str) {
        let url = canonical(url);
        if self.current_url() != url {
            self.history.truncate(self.position + 1);
            self.history.push(url.clone());
            self.position = self.history.len() - 1;
        }
        self.awaiting = Some(Awaited { url, position: Some(self.position), typed: true });
        self.loading = true;
    }

    /// Note that the shared view has just been pointed back at this tab's own address.
    ///
    /// Switching tabs sends one view to another page, and the engine reports the page it is leaving
    /// on the way. Without this, the tab being switched *to* would record the tab being switched
    /// *from* as somewhere it had been, and offer a `Back` to a page it had never shown.
    pub fn pointed_at(&mut self) {
        let url = self.current_url().to_owned();
        self.awaiting = Some(Awaited { url, position: None, typed: false });
        self.loading = true;
    }

    /// Record where a finished page load left this tab.
    ///
    /// While the tab is waiting for an address it asked for, every other page the view reports is the
    /// one it is leaving, and is ignored. Otherwise the address is somewhere new — unless it is
    /// exactly the entry behind or ahead of this one, which is what the engine's own back gesture
    /// inside the page looks like from here.
    pub fn arrived_at(&mut self, url: String) {
        let url = canonical(&url);
        if let Some(awaited) = &self.awaiting {
            if awaited.url != url {
                // **A page this tab asked for that arrives somewhere else is a redirect, and it is taken.**
                // Ignoring it left `loading` true for ever and the history naming an address the tab never
                // reached: measured on a real window, `http://github.com/` was still reported as the tab's
                // address, still loading, minutes after the page had settled on `https://github.com/`. The
                // Codex Sol review of `task-1907` found it, and `task-1907`'s own §2 is what made it reachable
                // from a browser node — a redirect was previously only met on a tab in the editing area.
                //
                // **Only for an address this tab typed**, which is what `position: None` means. A *step* names
                // an entry that is already in the history, so a page arriving at a different address there is
                // the view reporting the page it is leaving — which is the case this whole branch exists to
                // ignore, and is why `pointed_at` and `heading_for` are unaffected.
                if !awaited.typed {
                    return;
                }
                self.awaiting = None;
                self.loading = false;
                // The destination replaces the address that redirected, rather than being pushed after it: a
                // `Back` to the address that only ever answered with a redirect would redirect again, which is
                // what every browser collapses and why `history` names where the tab really is.
                self.history[self.position] = url;
                return;
            }
            let position = awaited.position;
            self.awaiting = None;
            self.loading = false;
            if let Some(position) = position {
                self.position = position;
            }
            return;
        }
        self.loading = false;
        if self.current_url() == url {
            return;
        }
        if self.position > 0 && self.history[self.position - 1] == url {
            self.position -= 1;
            return;
        }
        if self.history.get(self.position + 1).is_some_and(|next| next == &url) {
            self.position += 1;
            return;
        }
        self.history.truncate(self.position + 1);
        self.history.push(url);
        self.position = self.history.len() - 1;
    }

    /// The concise label shown in the file tab strip.
    pub fn name(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        if let Some(path) = self.location.source_path() {
            return path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Web page".to_owned());
        }
        Url::parse(self.current_url())
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "Web page".to_owned())
    }
}

/// A browser view that should be visible in this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrowserPlacement {
    pub id: u64,
    /// Where the page lays itself out, in the window's own points.
    ///
    /// **The whole of it, even the part nobody can see.** A page is laid out against the viewport it is
    /// given, so narrowing this is what makes a responsive page reflow — which is `task-1914`'s report
    /// about a node half off the canvas: *"the page content width is 50%, rather than just have half the
    /// page not shown"*. What is cut is [`BrowserPlacement::visible`] instead.
    pub area: Rect,
    /// The part of [`BrowserPlacement::area`] that may be painted, in the same points.
    ///
    /// Equal to `area` for a page in a pane, which fills its pane. Smaller for a node hanging off the edge
    /// of the canvas, and a platform that can crop a native child crops it here. See
    /// `native::clip_to_the_visible_part` for what that costs on each platform.
    pub visible: Rect,
    pub focused: bool,
    /// The zoom the page itself is to be set to, or nothing to leave whatever it is on.
    ///
    /// **A native child cannot be transformed**, so the canvas spends its camera on the page's own zoom:
    /// a node at half the camera's scale draws a page half the size rather than reflowing it at half the
    /// width. `app::space::show_a_browser_node` works out the product of the node's own zoom and the
    /// camera's, and it used to send it straight to the engine from inside the egui pass — while the
    /// bounds went to the engine from `raw_input_hook`, **before** the pass, off the placement the
    /// *previous* frame recorded.
    ///
    /// So on every frame of a zoom the page was being scaled one step ahead of the rectangle it was being
    /// drawn into, and a page whose layout viewport is its bounds divided by its zoom therefore had a
    /// viewport that was wrong by one step, every step, for the whole glide. That is `task-2004`'s
    /// *"it jitters as it zooms in/out the content to match"*. Carrying it on the placement is what makes
    /// the two one answer applied at one moment.
    ///
    /// **`None` for a page in a pane**, whose zoom is the person's own through `browser zoom` and is not
    /// Unluminous's to write back on every frame.
    pub zoom: Option<f64>,
}

impl BrowserPlacement {
    /// A placement that is wholly visible, which is every page drawn in a pane.
    pub fn whole(id: u64, area: Rect, focused: bool) -> Self {
        Self { id, area, visible: area, focused, zoom: None }
    }
}

/// A command shared by the browser toolbar and `unluminous-cli browser`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserCommand {
    Back,
    Forward,
    Reload,
    /// An address was typed into the toolbar's own field and entered.
    ///
    /// `task-1905`: *"I need a url address bar and back/forward and reload buttons at the top."* A
    /// browser node had no way to be given an address at all except from the command line, and this is
    /// what the field reports. The window resolves it the way it resolves every other address, through
    /// `BrowserLocation::parse`, because handing typed text straight to the view is what a live window
    /// refused with "Class not registered".
    Go(String),
}

/// Something the embedded engine reported back to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserEvent {
    LoadStarted { id: u64, url: String },
    LoadFinished { id: u64, url: String },
    Title { id: u64, title: String },
    OpenRequested { source: u64, url: String },
}

/// The one native browser view a window owns, and the local roots its tabs read under.
///
/// **A window has at most one native view, however many rendered tabs are open**, and the view is
/// pointed at whichever tab is showing. That is a platform limit found by measurement on
/// `task-1756`, not a simplification: creating a second WebView2 controller while another view lives
/// on the same thread blocks inside a nested Windows message pump on a completion that never
/// arrives, and the window never draws again — no crash, no error, no way back. It is also the
/// cheaper answer. A tab costs its remembered state and nothing else, and the whole feature costs
/// one browser: measured at 197 MB with no rendered tab open, 521 MB with one, and **521 MB with
/// three**.
pub struct BrowserHost {
    next_id: u64,
    profile: Option<PathBuf>,
    resources: LocalResourceStore,
    /// The tab the one native view is pointed at, shared with the engine's own callbacks.
    showing: Arc<AtomicU64>,
    sender: std::sync::mpsc::Sender<BrowserEvent>,
    receiver: std::sync::mpsc::Receiver<BrowserEvent>,
    native: native::NativeHost,
    last_resource_check: Instant,
}

impl BrowserHost {
    /// A lazy host. Constructing an ordinary Unluminous window starts no browser process.
    pub fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            next_id: 1,
            profile: None,
            resources: LocalResourceStore::new(),
            showing: Arc::new(AtomicU64::new(0)),
            sender,
            receiver,
            native: native::NativeHost::new(),
            last_resource_check: Instant::now(),
        }
    }

    /// Keep browser cookies and caches in Unluminous's per-user settings folder.
    pub fn set_profile(&mut self, folder: PathBuf) {
        if !self.has_views() {
            self.profile = Some(folder);
        }
    }

    /// Whether a page holds the **operating system's** keyboard focus right now.
    ///
    /// The one question `app::frame::show_the_resize_grips` has to ask before it sends
    /// `ViewportCommand::BeginResize`, and it is narrower than the one it used to ask. See [`TheFocus`]
    /// for what one refused resize costs, and `task-2004` for what asking `winit` instead cost: a
    /// window that is merely in the background has no focus either, and there the eight grips were dead
    /// for no reason at all.
    pub fn page_holds_the_keyboard(&self) -> bool {
        self.native.page_holds_the_keyboard()
    }

    /// Whether the native view exists, which is what decides if there is anything to settle.
    pub fn has_views(&self) -> bool {
        self.native.has_view()
    }

    /// Remember the window a child view is created inside, which is the one thing reconciling needs
    /// from the frame and the one thing it cannot be handed outside the egui pass.
    pub fn remember_window(&mut self, frame: &eframe::Frame) {
        self.native.remember_window(frame);
    }

    /// The tab the native view is pointed at, which is the only one that can be driven.
    pub fn showing(&self) -> Option<u64> {
        match self.showing.load(Ordering::Relaxed) {
            0 => None,
            id => Some(id),
        }
    }

    /// Allocate a stable id and register any local root before the page can request an asset.
    pub fn open_tab(&mut self, location: BrowserLocation) -> BrowserTab {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if let BrowserLocation::Local { root, .. } = &location {
            self.resources.register(id, root.clone());
        }
        BrowserTab::new(id, location)
    }

    /// Release everything local that belonged to a closed tab, and the view once none are left.
    pub fn close_tab(&mut self, id: u64) {
        self.resources.unregister(id);
        if self.showing() == Some(id) {
            self.showing.store(0, Ordering::Relaxed);
        }
    }

    /// Create, point, place and hide the native view. Called before the egui pass, never inside it.
    pub fn reconcile(
        &mut self,
        tabs: &[BrowserTab],
        placements: &[BrowserPlacement],
        occluders: &[egui::Rect],
        repaint: egui::Context,
    ) -> Settled {
        let live: HashSet<u64> = tabs.iter().map(|tab| tab.id).collect();
        self.resources.retain(&live);
        if tabs.is_empty() {
            self.native.forget();
            self.showing.store(0, Ordering::Relaxed);
            return Settled::default();
        }
        let chosen = choose(placements, occluders, CAN_CUT_A_PAGE);
        let Some(placement) = chosen else {
            self.native.hide();
            // **A page that is there and is not being drawn**, which is what `browser status` reports
            // and the only way anything can be asked whether a menu is covering one. `task-2009`.
            return Settled { covered: !placements.is_empty(), ..Settled::default() };
        };
        let covered = covers_any_of(occluders, placement.visible);
        let Some(tab) = tabs.iter().find(|tab| tab.id == placement.id) else {
            return Settled::default();
        };
        let was = self.showing();
        let problems = self.native.settle(native::Settle {
            tab,
            placement,
            showing: &self.showing,
            profile: self.profile.clone(),
            resources: self.resources.clone(),
            sender: self.sender.clone(),
            occluders,
            repaint,
        });
        let pointed_at = (was != self.showing()).then(|| self.showing()).flatten();
        Settled { problems, pointed_at, covered }
    }

    /// Send the view to an address on behalf of the tab that is showing.
    pub fn navigate(&self, id: u64, url: &str) -> Result<(), String> {
        self.for_the_showing_tab(id)?;
        self.native.navigate(url)
    }

    /// Reload the page the showing tab is on.
    pub fn reload(&self, id: u64) -> Result<(), String> {
        self.for_the_showing_tab(id)?;
        self.native.reload()
    }

    /// How big the tab that is showing draws its page.
    ///
    /// A window has one native view, so only the tab it is pointed at can be zoomed — which is why a
    /// node's zoom is remembered in `space::live::Live` and applied again when the view moves to it.
    pub fn zoom(&self, id: u64, factor: f64) -> Result<(), String> {
        self.for_the_showing_tab(id)?;
        self.native.zoom(factor)
    }

    /// Refuse to drive a tab that the one native view is not pointed at.
    fn for_the_showing_tab(&self, id: u64) -> Result<(), String> {
        match self.showing() == Some(id) {
            true => Ok(()),
            false => Err("That rendered tab is not the one showing.".to_owned()),
        }
    }

    /// Drain browser callbacks at the top of a frame.
    pub fn take_events(&self) -> Vec<BrowserEvent> {
        self.receiver.try_iter().collect()
    }

    /// Reload the local tabs whose previously requested resources changed on disk.
    pub fn reload_changed_local_tabs(&mut self) -> Vec<u64> {
        if self.last_resource_check.elapsed() < Duration::from_millis(500) {
            return Vec::new();
        }
        self.last_resource_check = Instant::now();
        self.resources.changed_tabs()
    }
}

/// What settling the one native view did this frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settled {
    /// Anything the platform could not do, against the tab that asked for it.
    pub problems: Vec<(u64, String)>,
    /// The tab the view was pointed at, when that changed this frame.
    pub pointed_at: Option<u64>,
    /// Whether there is a page to draw and an egui surface is over some of it.
    ///
    /// A page is a native child view, so nothing Unluminous photographs holds one and no state in the
    /// window used to say whether it was on the screen — which is why `task-2009`'s blanked page had
    /// to be found by looking at it. `browser status` reports this.
    pub covered: bool,
}

/// What the one native view is to do about the **operating system's** keyboard focus this frame.
///
/// A `WebView2` or a `WKWebView` is a real child window, so the focus a person types into is the
/// platform's rather than Unluminous's own `Focus`. Until `task-1945` only half of that was ever said:
/// a chosen browser node called `WebView::focus`, and nothing ever called anything when it stopped
/// being chosen — so the page kept the focus for the life of the window.
///
/// **Three things followed from that, and they were three of the four reports on `task-1945`.** Every
/// key press went to the page, so a terminal node clicked afterwards could not be typed into.
/// `egui-winit` refuses to forward `ViewportCommand::StartDrag` at all while `Window::has_focus()` is
/// false — and `winit` sets that false on the `WM_KILLFOCUS` the window gets the moment `SetFocus`
/// moves to the engine's child — so the title bar's drag was dropped in silence. And
/// `ViewportCommand::BeginResize`, which is not behind that check, reached `winit`'s
/// `handle_os_dragging`, which latches a flag that only `WM_EXITSIZEMOVE` clears; see
/// `components::resize_edges` for what one such refusal costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TheFocus {
    /// Nothing to do: the view holds the focus and should, or does not and should not.
    LeaveIt,
    /// The page is the surface being typed into, so hand the focus to it.
    GiveItToThePage,
    /// The page is not that surface any more, so hand the focus back to Unluminous's own window.
    GiveItBackToTheWindow,
}

/// Whether the last press anywhere in the window landed on one of the pages.
///
/// **A press outside a page hands the operating system's keyboard back to the window**, which is the
/// half of [`TheFocus`] `task-1945` did not say. `placement.focused` means *this node is the chosen
/// one*, and pressing the title bar, a resize grip, the menu bar, the rail or the status bar changes
/// which node is chosen not at all — so a page kept the operating system's keyboard through every one
/// of them, and with it `winit` went on answering `Window::has_focus()` with false.
///
/// `task-2004` reports what that costs: *"when the base of infinite space is open on windows, i can't
/// resize the main window … it's intermittent"*. The intermittence is whether a browser node has been
/// pressed since the window opened.
///
/// Read from the events of the frame that is about to be drawn, against the placements the last frame
/// drew, which is the pair `UnluminousApp::settle_the_native_views_before_the_pass` already has in
/// hand. **`visible` rather than `area`**, because the part of a node hanging off the canvas is not on
/// the screen and a press there is a press on whatever is drawn over it.
/// **It is a state rather than an event**, which is what makes it hold. `placement.focused` is worked
/// out afresh on every frame from which node is chosen, so clearing it on the frame the press arrives
/// gave the keyboard back for exactly one frame and the next frame handed it straight to the page
/// again. Measured on the installed build: pressing the window's own chrome moved
/// `pageHasTheKeyboard` to false and it was true again by the time the next command read it. What the
/// title bar's drag needs is for `winit` to have had its `WM_SETFOCUS`, which is a frame or two later.
pub fn the_page_was_the_last_thing_pressed(
    events: &[egui::Event],
    placements: &[BrowserPlacement],
    before: bool,
) -> bool {
    events
        .iter()
        .filter_map(|event| match event {
            egui::Event::PointerButton { pos, pressed: true, .. } => Some(*pos),
            _ => None,
        })
        .fold(before, |_, at| placements.iter().any(|placement| placement.visible.contains(at)))
}

/// Which of [`TheFocus`]'s three, from what the view holds now and what this frame asked for.
///
/// **Asked from a remembered flag rather than of the platform**, so the call into the window manager
/// happens on the frame the answer changes and on no other — the same bargain `NativeView::clip` and
/// `NativeView::visible` already make.
pub fn the_focus(view_holds_it: bool, wanted: bool) -> TheFocus {
    match (view_holds_it, wanted) {
        (false, true) => TheFocus::GiveItToThePage,
        (true, false) => TheFocus::GiveItBackToTheWindow,
        _ => TheFocus::LeaveIt,
    }
}

/// Whether this platform can cut the native child around what is drawn over it.
///
/// Windows can: `wry` builds the engine inside a container window of its own, and a region set on that
/// container clips the engine — which is how a browser node hanging off the canvas has shown part of a
/// page since `task-1914`. macOS has no such container, so there a covered page is hidden instead.
const CAN_CUT_A_PAGE: bool = cfg!(windows);

/// The one placement the native view goes to: the pane with the keyboard, else the first drawn.
///
/// A second rendered tab beside the first in a split pane cannot have a view of its own — see
/// [`BrowserHost`] — so it is drawn as a pane that says where its page is.
fn choose<'a>(
    placements: &'a [BrowserPlacement],
    occluders: &[egui::Rect],
    can_cut_the_page_around_them: bool,
) -> Option<&'a BrowserPlacement> {
    // **The last rather than the first when nothing is focused.** A placement is pushed as its owner is
    // drawn, and both the pane loop and the canvas's node loop draw **back to front** — so the first is the
    // one furthest behind. With two browser nodes overlapping and the keyboard somewhere else entirely, the
    // one underneath took the native view and painted above the one on top of it, because a native child
    // composites over everything egui draws. The Codex Sol review of `task-1905` found it.
    let chosen =
        placements.iter().find(|placement| placement.focused).or_else(|| placements.last())?;
    // **A page is hidden only where there is no way to cut it around what is over it.** It used to be
    // hidden whenever any popup was open anywhere: `egui::Popup::is_any_open` is one answer for the
    // menu bar, every dropdown and every flyout, and it was read as "take the view off the screen".
    // So opening the branch picker in the title bar blanked a page at the other end of the window —
    // `task-2009`: *"When I select a branch, a dropdown comes down, and all of a sudden the url tab
    // just shows the background image rather than the page i was viewing."*
    //
    // On Windows the answer is better than hiding less often: the child is **cut around** the menu,
    // so the page keeps showing everywhere the menu is not. That is `clip_to_the_visible_part`, which
    // has cut the child to its pane since `task-1914` and now subtracts what is drawn over it too.
    // Where the whole page is covered — a modal, which dims the window — the region comes out empty
    // and the child shows nothing, which is the same answer by the same route.
    //
    // macOS has no container of `wry`'s to mask, which `clip_to_the_visible_part` already records, so
    // there a page really does have to be hidden. `visible` rather than `area`, because the part of a
    // node hanging off the canvas is not on the screen; and an overlap has to have some area in it,
    // since `Rect::intersects` is true of two rectangles that merely touch along an edge.
    if !can_cut_the_page_around_them && covers_any_of(occluders, chosen.visible) {
        return None;
    }
    Some(chosen)
}

/// Whether any of `occluders` is really over `page`, shadow and all.
///
/// **Grown by [`OCCLUDER_SHADOW`] first**, because a popup's shadow is painted outside the rectangle
/// egui records for it, and a strip of page showing through the gap between the menu and its shadow
/// reads as a fault rather than as a page.
pub fn covers_any_of(occluders: &[egui::Rect], page: egui::Rect) -> bool {
    occluders.iter().any(|over| over.expand(OCCLUDER_SHADOW).intersect(page).is_positive())
}

/// How far outside its own rectangle a popup is drawn.
///
/// egui's default popup shadow is offset `[6, 10]` with a blur of `8`, so it reaches eighteen points
/// below the rectangle and fourteen to the right of it. Eighteen covers every side of it.
pub const OCCLUDER_SHADOW: f32 = 18.0;

impl Default for BrowserHost {
    /// Construct the same lazy host as [`BrowserHost::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// Enough file metadata to notice a changed resource without reading it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResourceStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl ResourceStamp {
    /// Measure one existing file.
    fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self { modified: metadata.modified().ok(), len: metadata.len() })
    }
}

/// **Unreachable rather than dead where there is no web view.** `task-1922`: `cargo clippy
/// --workspace --all-targets` on Linux, which is where the `checks` job runs, reports this and the
/// four items below as never used -- because the one thing that reaches them, the protocol callback
/// at `resources.resolve(...)`, is inside `#[cfg(any(windows, target_os = "macos"))]`.
///
/// They stay compiled everywhere rather than being put behind the same `cfg`, because what they hold
/// is the rule that a local page may only read under its own registered root: the tests that drive
/// the traversal refusals need no browser runtime, and losing them on a platform would be losing the
/// checks on the one part of this file that is about what a page may reach.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Debug, Default)]
struct LocalRoot {
    root: PathBuf,
    resources: HashMap<PathBuf, ResourceStamp>,
}

/// Static resources available to local browser tabs, shared with Wry's protocol callbacks.
#[derive(Debug, Clone)]
struct LocalResourceStore(Arc<Mutex<HashMap<u64, LocalRoot>>>);

impl LocalResourceStore {
    /// An empty registry that opens no file and starts no thread.
    fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())))
    }

    /// Register the one canonical root a local tab may read under.
    ///
    /// **A poisoned lock is taken rather than panicked on**, which is `task-1922` B15 and is what
    /// `resolve` below and `services::control` already did. A `Mutex` is poisoned for the life of the
    /// process once any thread panics while holding it, so one unrelated panic anywhere made every
    /// later tab opening and closing panic too -- a fault that spreads out of the thing that caused
    /// it. What is behind this lock is a map of tab ids to folders, and a panic cannot leave that
    /// half updated in a way that matters: an entry is inserted or removed whole.
    fn register(&self, id: u64, root: PathBuf) {
        let root = root.canonicalize().unwrap_or(root);
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, LocalRoot { root, resources: HashMap::new() });
    }

    /// Forget a tab's root and the bounded list of resources it loaded.
    fn unregister(&self, id: u64) {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&id);
    }

    /// Forget roots whose tabs disappeared through a whole-window state change.
    fn retain(&self, live: &HashSet<u64>) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|id, _| live.contains(id));
    }

    /// Resolve one custom-origin request without exposing paths outside the registered root.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn resolve(&self, id: u64, method: &str, uri: &str) -> ResourceReply {
        if method != "GET" && method != "HEAD" {
            return ResourceReply::empty(405);
        }
        let Some(path) = self.safe_path(id, uri) else {
            return ResourceReply::empty(404);
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return ResourceReply::empty(404);
        };
        self.record(id, &path);
        let mime = mime_guess::from_path(&path).first_or_octet_stream().essence_str().to_owned();
        ResourceReply {
            status: 200,
            mime,
            bytes: if method == "HEAD" { Vec::new() } else { bytes },
        }
    }

    /// Resolve and canonicalize the URL path, returning nothing for every escape and miss.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn safe_path(&self, id: u64, uri: &str) -> Option<PathBuf> {
        // Recovered rather than given up on, which is `register`'s own note three screens up and is
        // the half of `task-1922` B15 this file was missing (`task-1984` S12). A poisoned lock here
        // meant every later request for a local page answered nothing, so a panic anywhere in the
        // process silently stopped the browser serving -- and what is behind the lock is a map of
        // tab ids to folders, which a panic cannot leave half updated in a way that matters.
        let roots = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = roots.get(&id)?.root.clone();
        drop(roots);
        let url = Url::parse(uri).ok()?;
        let mut relative = PathBuf::new();
        for encoded in url.path().split('/').filter(|part| !part.is_empty()) {
            let part = percent_decode_str(encoded).decode_utf8().ok()?;
            let mut components = Path::new(part.as_ref()).components();
            match (components.next(), components.next()) {
                (Some(Component::Normal(name)), None) => relative.push(name),
                _ => return None,
            }
        }
        let candidate = root.join(relative);
        let candidate = if candidate.is_dir() { candidate.join("index.html") } else { candidate };
        let canonical = candidate.canonicalize().ok()?;
        canonical.starts_with(&root).then_some(canonical)
    }

    /// Remember a served resource so change detection polls only what the page used.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn record(&self, id: u64, path: &Path) {
        let Some(stamp) = ResourceStamp::of(path) else { return };
        // Recovered rather than given up on -- `safe_path`'s own note.
        let mut roots = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(root) = roots.get_mut(&id) {
            root.resources.insert(path.to_path_buf(), stamp);
        }
    }

    /// Return each tab whose loaded resource set changed, updating its stamps once.
    fn changed_tabs(&self) -> Vec<u64> {
        // Recovered rather than given up on -- `safe_path`'s own note.
        let mut roots = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut changed = Vec::new();
        for (id, root) in roots.iter_mut() {
            let moved = root.resources.iter_mut().any(|(path, before)| {
                let now = ResourceStamp::of(path);
                let differs = now != Some(*before);
                if let Some(now) = now {
                    *before = now;
                }
                differs
            });
            if moved {
                changed.push(*id);
            }
        }
        changed
    }
}

/// A protocol response independent of Wry, so its security can be tested with no browser runtime.
///
/// Unreachable where there is no web view. See [`LocalRoot`].
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceReply {
    status: u16,
    mime: String,
    bytes: Vec<u8>,
}

impl ResourceReply {
    /// A response with no body, used for misses and refused methods.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn empty(status: u16) -> Self {
        Self { status, mime: "text/plain; charset=utf-8".to_owned(), bytes: Vec::new() }
    }
}

/// The address a scheme-less value stands for, when it names a host rather than a file.
fn implied_address(value: &str) -> Option<String> {
    let host = value.split(['/', '?', '#']).next().unwrap_or_default();
    let looks_like_a_host = host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains(' ')
        && !host.contains('\\')
        && !file_kind::is_html(Path::new(host));
    let url = looks_like_a_host.then(|| format!("https://{value}"))?;
    Url::parse(&url)
        .ok()
        .filter(|url| url.host_str().is_some_and(|host| host.contains('.')))
        .map(|url| url.to_string())
}

/// The bytes a path segment cannot carry literally. A dot, a dash and a space-free name are left
/// alone, because the address bar shows this URL and `index%2Ehtml` is a worse answer than
/// `index.html` for the same file.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// The one form of an address, whatever the engine calls it.
///
/// WebView2 has no way to serve a scheme of its own, so wry maps `unluminous://tab-1/page.html` to
/// `http://unluminous.tab-1/page.html` and reports that back. WKWebView reports the `unluminous://` form. A tab
/// comparing the two as different addresses would count arriving at its own first page as having
/// gone somewhere new, and would then offer a `Back` to the page it is already on.
fn canonical(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else { return url.to_owned() };
    let Some(host) = parsed.host_str() else { return url.to_owned() };
    match (parsed.scheme(), host.strip_prefix("unluminous.")) {
        ("http" | "https", Some(origin)) => format!("unluminous://{origin}{}", parsed.path()),
        _ => url.to_owned(),
    }
}

/// The name for an address that the engine will actually navigate to.
///
/// The inverse of [`canonical`], and needed for the same reason: WebView2 cannot navigate a scheme of
/// its own, so wry serves `unluminous://` under `http://unluminous.<origin>/`. It rewrites the address a view
/// is *built* with, but `WebView::load_url` hands its string straight to `Navigate`, where an unknown
/// scheme is refused in silence — the pane keeps showing the page it was already on while the toolbar
/// says it is loading the new one, which is what pointing the shared view at another tab looked like
/// until this existed.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn engine_url(url: &str) -> String {
    if !cfg!(windows) {
        return url.to_owned();
    }
    match url.strip_prefix("unluminous://") {
        Some(rest) => format!("http://unluminous.{rest}"),
        None => url.to_owned(),
    }
}

/// Build a custom-origin URL whose path retains normal browser-relative semantics.
fn local_url(id: u64, relative: &Path) -> String {
    let path = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .map(|name| utf8_percent_encode(&name.to_string_lossy(), SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/");
    format!("unluminous://tab-{id}/{path}")
}

#[cfg(any(windows, target_os = "macos"))]
mod native {
    use std::borrow::Cow;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
    use wry::{
        NewWindowResponse, PageLoadEvent, PermissionResponse, WebContext, WebView, WebViewBuilder,
    };

    use super::{BrowserEvent, BrowserPlacement, BrowserTab, LocalResourceStore};

    /// Everything needed to settle the native view on one tab for one frame.
    pub struct Settle<'a> {
        pub tab: &'a BrowserTab,
        pub placement: &'a BrowserPlacement,
        /// What egui is drawing over the page this frame, in the window's own points.
        pub occluders: &'a [egui::Rect],
        pub showing: &'a Arc<AtomicU64>,
        pub profile: Option<PathBuf>,
        pub resources: LocalResourceStore,
        pub sender: std::sync::mpsc::Sender<BrowserEvent>,
        pub repaint: egui::Context,
    }

    /// The window a child view is created inside, remembered by its handle.
    ///
    /// Held rather than borrowed because the view is created before the egui pass, where there is no
    /// `eframe::Frame` to ask. It is the handle eframe itself hands out, and it is used only while
    /// the window that owns this host is alive.
    struct Parent(RawWindowHandle);

    impl HasWindowHandle for Parent {
        /// Borrow the remembered handle for as long as this parent is.
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            // Safety: the handle belongs to the window this host lives in, which outlives the borrow.
            unsafe { Ok(WindowHandle::borrow_raw(self.0)) }
        }
    }

    /// What the native child is cut to: its pane, less whatever egui is drawing over it.
    ///
    /// Kept whole rather than as a yes or no, because it is what `place` compares a frame against —
    /// and during a canvas zoom every one of these numbers moves, which is what `task-2004` measured
    /// `SetWindowRgn` being called sixty times a second for.
    #[derive(Debug, Clone, PartialEq)]
    struct Clip {
        area: egui::Rect,
        visible: egui::Rect,
        over: Vec<egui::Rect>,
    }

    struct NativeView {
        webview: WebView,
        bounds: Option<wry::Rect>,
        /// The last answer [`clip_to_the_visible_part`] was given, so a frame that changed none of it
        /// costs no call into the window manager. `wry::Rect` is not comparable, so this keeps egui's
        /// own. `None` is no region at all, which is a page with nothing over it inside its own pane.
        clip: Option<Clip>,
        /// The zoom [`place`] last sent, so a frame that did not move it costs no call into the engine.
        zoom: Option<f64>,
        visible: bool,
        /// Whether the page holds the **operating system's** keyboard focus. See [`super::TheFocus`].
        has_the_focus: bool,
    }

    /// The platform objects, kept behind this module so the rest of Unluminous stays portable and testable.
    pub struct NativeHost {
        context: Option<WebContext>,
        view: Option<NativeView>,
        parent: Option<Parent>,
    }

    impl NativeHost {
        /// A host with no browser environment until a tab is first shown.
        pub fn new() -> Self {
            Self { context: None, view: None, parent: None }
        }

        /// Whether the native view exists.
        pub fn has_view(&self) -> bool {
            self.view.is_some()
        }

        /// Whether the page holds the operating system's keyboard focus.
        ///
        /// **Asked of the platform, not of the flag.** `view.has_the_focus` records what Unluminous
        /// last *did* about the focus, and a `WebView2` takes it on its own the moment it is created
        /// — see [`the_page_really_has_the_keyboard`]. `task-2009`.
        pub fn page_holds_the_keyboard(&self) -> bool {
            self.view.as_ref().is_some_and(page_has_it)
        }

        /// Take the window's handle while the frame that has one is in scope.
        pub fn remember_window(&mut self, frame: &eframe::Frame) {
            if self.parent.is_none() {
                self.parent = frame.window_handle().ok().map(|handle| Parent(handle.as_raw()));
            }
        }

        /// Drop the view and everything it holds, which is what closing the last rendered tab means.
        ///
        /// **The focus is handed back before the view goes.** Windows moves a destroyed window's focus
        /// to its parent by itself, but it does so through `WM_KILLFOCUS`/`WM_SETFOCUS` ordering that
        /// nothing here can assert; asking `wry` is one call and it is the same call the other two
        /// routes make. See [`super::TheFocus`].
        pub fn forget(&mut self) {
            if let Some(view) = &mut self.view {
                give_the_focus_back(view);
            }
            self.view = None;
        }

        /// Take the view off the screen and lower its memory target, keeping it ready.
        pub fn hide(&mut self) {
            if let Some(view) = &mut self.view {
                give_the_focus_back(view);
                set_visible(view, false);
            }
        }

        /// Create the view if there is none, point it at this tab, and place it in the pane.
        pub fn settle(&mut self, request: Settle<'_>) -> Vec<(u64, String)> {
            let id = request.tab.id;
            if self.view.is_none() {
                if let Err(problem) = self.create(&request) {
                    return vec![(id, problem)];
                }
                request.showing.store(id, Ordering::Relaxed);
            } else if request.showing.load(Ordering::Relaxed) != id {
                // The tab that is showing changed, so the one view follows it to that tab's address.
                request.showing.store(id, Ordering::Relaxed);
                if let Some(view) = &mut self.view {
                    let _ = view.webview.load_url(&super::engine_url(request.tab.current_url()));
                    // The zoom belongs to the tab, so what was sent for the last one says nothing about
                    // this one and the next placement has to be believed.
                    view.zoom = None;
                }
            }
            if let Some(view) = &mut self.view {
                place(view, request.placement, request.occluders);
            }
            Vec::new()
        }

        /// Build the one native view, with no host bridge and callbacks that report browser state.
        ///
        /// The callbacks read which tab the view is pointed at rather than closing over one, because
        /// the view outlives any single tab.
        fn create(&mut self, request: &Settle<'_>) -> Result<(), String> {
            let Some(parent) = &self.parent else {
                return Err("Unluminous has not finished opening its window yet.".to_owned());
            };
            let context =
                self.context.get_or_insert_with(|| WebContext::new(request.profile.clone()));
            let repaint = request.repaint.clone();
            let title_sender = request.sender.clone();
            let title_showing = request.showing.clone();
            let load_sender = request.sender.clone();
            let load_showing = request.showing.clone();
            let load_repaint = request.repaint.clone();
            let popup_sender = request.sender.clone();
            let popup_showing = request.showing.clone();
            let resources = request.resources.clone();
            let protocol_showing = request.showing.clone();
            let webview = WebViewBuilder::new_with_web_context(context)
                .with_id("unluminous-browser")
                .with_url(request.tab.current_url())
                .with_visible(false)
                .with_clipboard(true)
                .with_background_throttling(wry::BackgroundThrottlingPolicy::Throttle)
                .with_permission_handler(|_| PermissionResponse::Deny)
                .with_download_started_handler(|_, _| false)
                .with_navigation_handler(allowed_navigation)
                .with_new_window_req_handler(move |url, _| {
                    let source = popup_showing.load(Ordering::Relaxed);
                    let _ = popup_sender.send(BrowserEvent::OpenRequested { source, url });
                    NewWindowResponse::Deny
                })
                .with_document_title_changed_handler(move |title| {
                    let id = title_showing.load(Ordering::Relaxed);
                    let _ = title_sender.send(BrowserEvent::Title { id, title });
                    repaint.request_repaint();
                })
                .with_on_page_load_handler(move |event, url| {
                    let id = load_showing.load(Ordering::Relaxed);
                    let event = match event {
                        PageLoadEvent::Started => BrowserEvent::LoadStarted { id, url },
                        PageLoadEvent::Finished => BrowserEvent::LoadFinished { id, url },
                    };
                    let _ = load_sender.send(event);
                    load_repaint.request_repaint();
                })
                .with_custom_protocol("unluminous".to_owned(), move |_, request| {
                    protocol_response(&resources, protocol_showing.load(Ordering::Relaxed), request)
                })
                .build_as_child(parent)
                .map_err(|problem| format!("Unluminous could not start the browser: {problem}"))?;
            self.view = Some(NativeView {
                webview,
                bounds: None,
                clip: None,
                zoom: None,
                visible: false,
                has_the_focus: false,
            });
            Ok(())
        }

        /// Send the view to an address, which is what this tab's own history asked for.
        pub fn navigate(&self, url: &str) -> Result<(), String> {
            let view =
                self.view.as_ref().ok_or_else(|| "The browser tab is not ready yet.".to_owned())?;
            view.webview
                .load_url(&super::engine_url(url))
                .map_err(|problem| format!("The browser could not navigate: {problem}"))
        }

        /// Reload the page the view is on.
        pub fn reload(&self) -> Result<(), String> {
            let view =
                self.view.as_ref().ok_or_else(|| "The browser tab is not ready yet.".to_owned())?;
            view.webview
                .reload()
                .map_err(|problem| format!("The browser could not reload: {problem}"))
        }

        /// How big the page is drawn.
        ///
        /// **The engine's number rather than Unluminous's drawing.** `task-1905` asks that the modifier
        /// wheel over a node zoom that node, and for a browser node the thing that decides how big a page
        /// is is the page's own zoom — nothing Unluminous paints is involved.
        pub fn zoom(&self, factor: f64) -> Result<(), String> {
            let view =
                self.view.as_ref().ok_or_else(|| "The browser tab is not ready yet.".to_owned())?;
            view.webview
                .zoom(factor)
                .map_err(|problem| format!("The browser could not be zoomed: {problem}"))
        }
    }

    /// Allow ordinary web navigation and Unluminous's one local resource origin.
    fn allowed_navigation(url: String) -> bool {
        Url::parse(&url)
            .ok()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https" | "unluminous"))
    }

    /// Convert a constrained resource reply into the response Wry expects.
    fn protocol_response(
        resources: &LocalResourceStore,
        showing: u64,
        request: wry::http::Request<Vec<u8>>,
    ) -> wry::http::Response<Cow<'static, [u8]>> {
        let reply =
            resources.resolve(showing, request.method().as_str(), &request.uri().to_string());
        wry::http::Response::builder()
            .status(reply.status)
            .header("Content-Type", reply.mime)
            .header("Cache-Control", "no-store")
            .body(Cow::Owned(reply.bytes))
            .expect("resource response")
    }

    /// Keep the view inside its pane and change native state only when the answer moved.
    ///
    /// **The bounds are the whole page and the crop is a separate question.** `set_bounds` is the page's
    /// viewport as well as its position, so cutting it is what reflows a responsive page — see
    /// [`clip_to_the_visible_part`], which takes the part that may be painted off the same placement.
    fn place(view: &mut NativeView, placement: &BrowserPlacement, occluders: &[egui::Rect]) {
        let bounds = browser_rect(placement.area);
        if view.bounds != Some(bounds) && view.webview.set_bounds(bounds).is_ok() {
            view.bounds = Some(bounds);
        }
        // **And the zoom in the same breath as the bounds**, because a page's layout viewport is the one
        // divided by the other: sending them from two places on two different sides of the egui pass left
        // the viewport wrong by one step of the glide on every frame of it. See
        // [`BrowserPlacement::zoom`], and `task-2004`.
        if let Some(zoom) = placement.zoom {
            // Compared before it is sent, which the bounds beside it have always been: `put_ZoomFactor`
            // raises a zoom-changed event and re-lays the page out, and this used to be called on every
            // frame whether or not the number had moved.
            if view.zoom != Some(zoom) && view.webview.zoom(zoom).is_ok() {
                view.zoom = Some(zoom);
            }
        }
        // **Whether there is a region, not which numbers produced one.** During a canvas zoom every
        // rectangle here moves on every frame, so this was calling `SetWindowRgn(hwnd, …, TRUE)` sixty
        // times a second — redrawing the child from scratch each time — for a node wholly inside the pane
        // with nothing over it, where the answer is no region at all. `task-2004`.
        //
        // **And what is drawn over the page is subtracted from it**, rather than the whole page being
        // taken off the screen while a menu is open. `task-2009`: the branch picker hangs over part of a
        // pane, and a native child paints above everything egui draws, so the page has to come out from
        // under the menu — but only from under the menu. See `services::browser::choose`.
        let over: Vec<egui::Rect> = occluders
            .iter()
            .map(|rect| rect.expand(super::OCCLUDER_SHADOW))
            .filter(|rect| rect.intersect(placement.visible).is_positive())
            .collect();
        let wanted = match placement.visible.contains_rect(placement.area) && over.is_empty() {
            true => None,
            false => {
                Some(Clip { area: placement.area, visible: placement.visible, over: over.clone() })
            }
        };
        if view.clip != wanted {
            clip_to_the_visible_part(view, placement.area, placement.visible, &over);
            view.clip = wanted;
        }
        set_visible(view, true);
        // **What the platform says, not what this last did.** See [`the_page_really_has_the_keyboard`].
        match super::the_focus(page_has_it(view), placement.focused) {
            super::TheFocus::LeaveIt => {}
            super::TheFocus::GiveItToThePage => {
                let _ = view.webview.focus();
                view.has_the_focus = true;
            }
            super::TheFocus::GiveItBackToTheWindow => give_the_focus_back(view),
        }
    }

    /// Whether the engine's own window holds the operating system's keyboard focus **right now**.
    ///
    /// `NativeView::has_the_focus` is what Unluminous last *did* about the focus, and that is not the
    /// same thing: a `WebView2` calls `SetFocus` on itself when it is created, so a window that opens
    /// with a rendered tab restored has handed the keyboard to a page nobody asked for it — with
    /// `has_the_focus` still false, so nothing ever handed it back.
    ///
    /// `task-2009`: *"On windows, after launch, I can't move the window around by clicking the top bar
    /// and dragging, unless I first focus another window like Firefox, then focus unluminous."*
    /// `egui-winit` drops `ViewportCommand::StartDrag` while `Window::has_focus()` is false, and `winit`
    /// sets that false on the `WM_KILLFOCUS` the window gets when the child takes `SetFocus`. Clicking
    /// the title bar does not move the focus back — nothing in `winit` or `egui` calls `SetFocus` on a
    /// click — so it stays with the page; clicking another application and coming back is a real
    /// deactivate and activate, and Windows gives the focus to the top-level window that was clicked.
    /// Measured on the installed build, with a page open and nothing pressed:
    ///
    /// ```text
    /// status --section window  ->  focused: false, osForeground: true, osKeyboard: false
    /// GetGUIThreadInfo          ->  hwndFocus class = Chrome_WidgetWin_1
    /// ```
    ///
    /// Asked of `GetFocus`, which answers about the calling thread's own queue and is called on the
    /// window's thread, against the container `wry` builds the engine inside.
    #[cfg(windows)]
    fn the_page_really_has_the_keyboard(view: &NativeView) -> bool {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::IsChild;
        use wry::WebViewExtWindows as _;

        let hwnd = view.webview.hwnd().0 as HWND;
        if hwnd.is_null() {
            return false;
        }
        // SAFETY: `GetFocus` takes nothing, and both handles are windows in this process.
        unsafe {
            let focus = GetFocus();
            !focus.is_null() && (focus == hwnd || IsChild(hwnd, focus) != 0)
        }
    }

    /// The same question where there is nothing to ask it of, which leaves the flag as the answer.
    #[cfg(not(windows))]
    fn the_page_really_has_the_keyboard(_view: &NativeView) -> bool {
        false
    }

    /// Whether the page has the keyboard, by either of the two ways it can come to have it.
    fn page_has_it(view: &NativeView) -> bool {
        view.has_the_focus || the_page_really_has_the_keyboard(view)
    }

    /// Hand the operating system's keyboard focus back to the window `wry` built this view inside.
    ///
    /// `WebView::focus_parent` is `SetFocus` on the parent on Windows and `makeFirstResponder` on
    /// macOS, and the parent is the window eframe handed to `build_as_child` — so this is Unluminous's
    /// own window in both cases. It is asked of `wry` rather than done here because `wry` owns the
    /// handles, and because a host that calls `SetFocus` on itself without going back through the
    /// controller is the shape `WebView2` is documented not to restore reliably from.
    fn give_the_focus_back(view: &mut NativeView) {
        if !page_has_it(view) {
            return;
        }
        let _ = view.webview.focus_parent();
        view.has_the_focus = false;
    }

    /// Crop the native child to the part of it that may be painted, without touching its viewport.
    ///
    /// **On Windows this is a window region on the container `wry` already makes.** `WebViewExtWindows::hwnd`
    /// is that container: a real `WS_CHILD` window whose only child is the engine's own, so a region set on
    /// it clips the engine and leaves `ICoreWebView2Controller::SetBounds` — the page's viewport — at the
    /// whole node. That is what makes a node hanging off the canvas show *part of a page* rather than a page
    /// laid out into a narrower box, which is `task-1914`'s report.
    ///
    /// The scale is asked of the same window `wry` asks, `GetDpiForWindow`, so the two cannot disagree about
    /// where a logical point is. `SetWindowRgn` takes ownership of the region and the system deletes it, so
    /// nothing is deleted here; a placement that is wholly visible passes `None`, which is how a region is
    /// taken off again.
    ///
    /// **On macOS there is no crop and the placement is honest about it.** A `WKWebView` is an `NSView` and
    /// its superview is the window's content view, which does not clip its subviews — there is no container
    /// of `wry`'s to put a mask on, and adding one would mean reaching into the view hierarchy `wry` owns.
    /// So the bounds are the whole page there and the part outside the pane is drawn over Unluminous's own
    /// furniture, which is the trade the caller states in `show_a_browser_node`.
    #[cfg(windows)]
    fn clip_to_the_visible_part(
        view: &NativeView,
        area: egui::Rect,
        visible: egui::Rect,
        over: &[egui::Rect],
    ) {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_DIFF,
        };
        use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
        use wry::WebViewExtWindows as _;

        let hwnd = view.webview.hwnd().0 as HWND;
        if hwnd.is_null() {
            return;
        }
        // Whole, so the region comes off entirely rather than being set to the window's own size — a
        // region that happens to match is still a region, and one rounding point of difference would
        // shave a column of pixels off a page nothing is covering.
        if visible.contains_rect(area) && over.is_empty() {
            unsafe { SetWindowRgn(hwnd, std::ptr::null_mut(), 1) };
            return;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        // `0` is what `GetDpiForWindow` answers for a handle it does not like, and 96 is one physical
        // pixel to one point — which is `wry`'s own fallback in `webview2::util::hwnd_dpi`.
        let scale = f64::from(if dpi == 0 { 96 } else { dpi }) / 96.0;
        // The child's own top left is the node's, so every rectangle is measured from there.
        let physical = |value: f32| (f64::from(value) * scale).round() as i32;
        let inside = |rect: egui::Rect| {
            let left = physical(rect.left() - area.left());
            let top = physical(rect.top() - area.top());
            let right = physical(rect.right() - area.left()).max(left);
            let bottom = physical(rect.bottom() - area.top()).max(top);
            unsafe { CreateRectRgn(left, top, right, bottom) }
        };
        let region = inside(visible);
        if region.is_null() {
            return;
        }
        // Each thing drawn over the page is cut out of the region, which is what leaves the rest of the
        // page showing while a menu is open. `RGN_DIFF` writes into the first handle, so the region
        // being built is both a source and the destination.
        for rect in over {
            let hole = inside(*rect);
            if hole.is_null() {
                continue;
            }
            unsafe {
                CombineRgn(region, region, hole, RGN_DIFF);
                DeleteObject(hole as _);
            }
        }
        // The system owns the region from here and deletes it when the window is destroyed or the next
        // one replaces it, so it is not deleted here even when the call fails — `SetWindowRgn` documents
        // that it takes ownership.
        unsafe { SetWindowRgn(hwnd, region, 1) };
    }

    /// The same question on a platform whose native child cannot be cropped. See the Windows half.
    #[cfg(not(windows))]
    fn clip_to_the_visible_part(
        _view: &NativeView,
        _area: egui::Rect,
        _visible: egui::Rect,
        _over: &[egui::Rect],
    ) {
    }

    /// Show or hide the native child and lower an inactive Windows renderer's memory target.
    fn set_visible(view: &mut NativeView, visible: bool) {
        if view.visible == visible {
            return;
        }
        let _ = view.webview.set_visible(visible);
        #[cfg(windows)]
        {
            use wry::{MemoryUsageLevel, WebViewExtWindows as _};
            let level = if visible { MemoryUsageLevel::Normal } else { MemoryUsageLevel::Low };
            let _ = view.webview.set_memory_usage_level(level);
        }
        view.visible = visible;
    }

    /// Convert egui's logical points to Wry's logical child-window rectangle.
    fn browser_rect(area: egui::Rect) -> wry::Rect {
        wry::Rect {
            position: wry::dpi::LogicalPosition::new(area.left() as f64, area.top() as f64).into(),
            size: wry::dpi::LogicalSize::new(
                area.width().max(1.0) as f64,
                area.height().max(1.0) as f64,
            )
            .into(),
        }
    }

    use url::Url;
}

#[cfg(not(any(windows, target_os = "macos")))]
mod native {
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    use super::{BrowserEvent, BrowserPlacement, BrowserTab, LocalResourceStore};

    /// The same input on a platform without an embedded engine.
    pub struct Settle<'a> {
        pub tab: &'a BrowserTab,
        pub placement: &'a BrowserPlacement,
        /// What egui is drawing over the page this frame, in the window's own points.
        pub occluders: &'a [egui::Rect],
        pub showing: &'a Arc<AtomicU64>,
        pub profile: Option<std::path::PathBuf>,
        pub resources: LocalResourceStore,
        pub sender: std::sync::mpsc::Sender<BrowserEvent>,
        pub repaint: egui::Context,
    }

    /// A portable stub that keeps the workspace compiling and returns one clear refusal.
    pub struct NativeHost;

    const UNSUPPORTED: &str = "Browser tabs are available on Windows and macOS.";

    impl NativeHost {
        /// Construct the no-engine placeholder used on unsupported platforms.
        pub fn new() -> Self {
            Self
        }
        /// A placeholder never owns a native view.
        pub fn has_view(&self) -> bool {
            false
        }
        /// And so never holds the keyboard.
        pub fn page_holds_the_keyboard(&self) -> bool {
            false
        }
        /// There is no child window to create, so the window's handle is not wanted.
        pub fn remember_window(&mut self, _frame: &eframe::Frame) {}
        /// There is nothing to drop.
        pub fn forget(&mut self) {}
        /// There is nothing to hide.
        pub fn hide(&mut self) {}
        /// Report one clear platform refusal for the tab that would have been shown.
        pub fn settle(&mut self, request: Settle<'_>) -> Vec<(u64, String)> {
            let _ = (
                request.placement,
                request.showing,
                request.profile,
                request.resources,
                request.sender,
                request.repaint,
            );
            vec![(request.tab.id, UNSUPPORTED.to_owned())]
        }
        /// Refuse navigation where there is no embedded engine.
        pub fn navigate(&self, _url: &str) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// Refuse reloading for the same reason.
        pub fn reload(&self) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// And zooming, which is the engine's own number.
        pub fn zoom(&self, _factor: f64) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-1945`: a page that stops being the surface somebody types into hands the keyboard back.
    ///
    /// The platform half cannot be tested without a real engine, so what is asserted is the decision:
    /// the frame a placement stops being focused is the frame the focus goes back, and no frame after
    /// it asks again. Before this ticket there was no such frame at all — `place` had a branch for
    /// `focused` and none for anything else — and three of `task-1945`'s four reports were that.
    #[test]
    fn the_last_press_decides_whether_the_page_holds_the_keyboard() {
        let page = BrowserPlacement::whole(
            1,
            egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(400.0, 300.0)),
            true,
        );
        let placements = [page];
        let press = |x: f32, y: f32| egui::Event::PointerButton {
            pos: egui::pos2(x, y),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let release = egui::Event::PointerButton {
            pos: egui::pos2(10.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        };

        // **A frame with no press at all leaves the answer where it was**, which is what makes it hold:
        // `placement.focused` is worked out afresh every frame, so an event would give the keyboard back
        // for exactly one frame and the next would hand it straight to the page again.
        assert!(the_page_was_the_last_thing_pressed(&[], &placements, true));
        assert!(!the_page_was_the_last_thing_pressed(&[], &placements, false));
        assert!(
            the_page_was_the_last_thing_pressed(std::slice::from_ref(&release), &placements, true),
            "and a release is not a press"
        );

        assert!(the_page_was_the_last_thing_pressed(&[press(200.0, 200.0)], &placements, false));
        assert!(!the_page_was_the_last_thing_pressed(&[press(20.0, 20.0)], &placements, true));
        // The last press in the frame is the one that decides, which is the same rule said about a
        // frame that carried two.
        assert!(the_page_was_the_last_thing_pressed(
            &[press(20.0, 20.0), press(200.0, 200.0)],
            &placements,
            false
        ));
    }

    #[test]
    fn a_page_that_loses_the_keyboard_hands_it_back_once() {
        assert_eq!(the_focus(false, true), TheFocus::GiveItToThePage, "the page was clicked");
        assert_eq!(the_focus(true, true), TheFocus::LeaveIt, "and it still has it the next frame");
        assert_eq!(
            the_focus(true, false),
            TheFocus::GiveItBackToTheWindow,
            "something else was clicked, so the window takes the keyboard back"
        );
        assert_eq!(
            the_focus(false, false),
            TheFocus::LeaveIt,
            "and it is not asked for again on every frame afterwards"
        );
    }

    /// The one native view goes to the topmost placement, not the backmost.
    ///
    /// A placement is pushed as its owner is drawn, and both the pane loop and the canvas's node loop draw
    /// back to front — so `placements.first()` is the thing furthest *behind*. With two browser nodes
    /// overlapping and the keyboard elsewhere, the one underneath took the view and painted above the one on
    /// top of it, because a native child composites over everything egui draws. Found by the Codex Sol review
    /// of `task-1905`.
    #[test]
    fn the_native_view_goes_to_the_topmost_placement() {
        let placed = |id: u64, focused: bool| {
            BrowserPlacement::whole(
                id,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
                focused,
            )
        };
        // Back to front: 1 was drawn first and 2 is on top.
        let both = [placed(1, false), placed(2, false)];
        assert_eq!(choose(&both, &[], true).map(|one| one.id), Some(2), "the one on top");
        // A focused placement still wins, wherever it is in the order: that is somebody's own choice.
        let focused_behind = [placed(1, true), placed(2, false)];
        assert_eq!(choose(&focused_behind, &[], true).map(|one| one.id), Some(1));
        // A menu over it takes it off the screen **only** where there is no way to cut it around one,
        // which is macOS: `wry` builds no container there for a region to be set on.
        let over = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
        assert!(choose(&both, &[over], false).is_none(), "nothing to cut it with");
        assert_eq!(
            choose(&both, &[over], true).map(|one| one.id),
            Some(2),
            "and where there is, the page stays and the child is cut around the menu"
        );
        assert!(choose(&[], &[], true).is_none());
    }

    /// `task-2009`: what is really over a page, rather than anything being open anywhere.
    ///
    /// *"When I select a branch, a dropdown comes down, and all of a sudden the url tab just shows
    /// the background image rather than the page i was viewing."* This is the question the page is cut
    /// by on Windows and hidden by on macOS, so it is asked once and tested once.
    #[test]
    fn only_what_is_drawn_over_a_page_counts_as_covering_it() {
        let page = egui::Rect::from_min_max(egui::pos2(300.0, 100.0), egui::pos2(900.0, 700.0));
        // A dropdown that hangs from the title bar and stops well above the pane.
        let above = egui::Rect::from_min_max(egui::pos2(120.0, 10.0), egui::pos2(420.0, 60.0));
        assert!(!covers_any_of(&[above], page), "a dropdown above the pane is not over the page");
        // One long enough to reach into it.
        let reaches = egui::Rect::from_min_max(egui::pos2(120.0, 36.0), egui::pos2(420.0, 400.0));
        assert!(covers_any_of(&[reaches], page));
        // A menu whose own rectangle stops just above the pane still counts, because its shadow is
        // painted below it. See `OCCLUDER_SHADOW`.
        let just_above = egui::Rect::from_min_max(egui::pos2(120.0, 36.0), egui::pos2(420.0, 96.0));
        assert!(covers_any_of(&[just_above], page), "a popup's shadow is over the page too");
        assert!(!covers_any_of(&[], page));
    }

    /// A unique folder under the process temp directory for one resource test.
    fn fixture(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("unluminous-browser-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("make browser fixture");
        folder
    }

    #[test]
    fn local_pages_load_linked_resource_types_and_head() {
        let root = fixture("assets");
        std::fs::write(
            root.join("index.html"),
            "<link href='site.css'><script src='app.js'></script>",
        )
        .unwrap();
        std::fs::write(root.join("site.css"), "body { color: red; }").unwrap();
        std::fs::write(root.join("app.js"), "document.title = 'ready';").unwrap();
        let store = LocalResourceStore::new();
        store.register(7, root);
        let html = store.resolve(7, "GET", "unluminous://tab-7/index.html");
        let css = store.resolve(7, "GET", "unluminous://tab-7/site.css");
        let script = store.resolve(7, "HEAD", "unluminous://tab-7/app.js");
        assert_eq!((html.status, html.mime.as_str()), (200, "text/html"));
        assert_eq!((css.status, css.mime.as_str()), (200, "text/css"));
        assert_eq!(
            (script.status, script.mime.as_str(), script.bytes.len()),
            (200, "text/javascript", 0)
        );
    }

    #[test]
    fn local_origin_refuses_traversal_missing_files_and_writes() {
        let root = fixture("security");
        std::fs::write(root.join("index.html"), "ok").unwrap();
        let store = LocalResourceStore::new();
        store.register(3, root);
        assert_eq!(store.resolve(3, "GET", "unluminous://tab-3/../secret.txt").status, 404);
        assert_eq!(store.resolve(3, "GET", "unluminous://tab-3/%2E%2E/secret.txt").status, 404);
        assert_eq!(store.resolve(3, "GET", "unluminous://tab-3/missing.css").status, 404);
        assert_eq!(store.resolve(3, "POST", "unluminous://tab-3/index.html").status, 405);
    }

    #[test]
    fn local_resource_changes_are_reported_once() {
        let root = fixture("changes");
        let css = root.join("site.css");
        std::fs::write(&css, "a").unwrap();
        let store = LocalResourceStore::new();
        store.register(9, root);
        assert_eq!(store.resolve(9, "GET", "unluminous://tab-9/site.css").status, 200);
        assert!(store.changed_tabs().is_empty());
        std::fs::write(css, "longer").unwrap();
        assert_eq!(store.changed_tabs(), vec![9]);
        assert!(store.changed_tabs().is_empty());
    }

    /// `task-1756`: an address with no scheme is a host, and every other refusal names its reason.
    #[test]
    fn addresses_and_files_are_told_apart_and_bad_ones_are_refused() {
        let project = fixture("addresses");
        std::fs::write(project.join("index.html"), "<p>ok</p>").unwrap();
        std::fs::write(project.join("notes.md"), "not a page").unwrap();
        let parse = |value: &str| BrowserLocation::parse(value, &project);
        assert_eq!(
            parse("https://example.com/a").unwrap(),
            BrowserLocation::Remote { url: "https://example.com/a".to_owned() }
        );
        assert_eq!(
            parse("example.com/a").unwrap(),
            BrowserLocation::Remote { url: "https://example.com/a".to_owned() }
        );
        assert!(matches!(parse("index.html").unwrap(), BrowserLocation::Local { .. }));
        assert!(parse("ftp://example.com").unwrap_err().contains("not ftp addresses"));
        assert!(parse("notes.md").unwrap_err().contains("is not an HTML file"));
        assert!(parse("missing.html").unwrap_err().contains("could not open"));
        assert!(parse("   ").unwrap_err().contains("Say which"));
    }

    /// `task-1756`: the address bar shows a readable path, and a name with a space still resolves.
    #[test]
    fn local_addresses_stay_readable_and_still_resolve() {
        let root = fixture("readable");
        let folder = root.join("my site");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("index.html"), "<p>ok</p>").unwrap();
        let location =
            BrowserLocation::parse(&folder.join("index.html").to_string_lossy(), &root).unwrap();
        let url = location.initial_url(4);
        assert_eq!(url, "unluminous://tab-4/my%20site/index.html");
        let store = LocalResourceStore::new();
        store.register(4, root);
        assert_eq!(store.resolve(4, "GET", &url).status, 200);
    }

    /// `task-1756`: a closed tab keeps no root, so its origin answers nothing afterwards.
    #[test]
    fn a_closed_tab_releases_its_root_and_its_remembered_resources() {
        let root = fixture("lifecycle");
        std::fs::write(root.join("index.html"), "<p>ok</p>").unwrap();
        let mut host = BrowserHost::new();
        let tab = host.open_tab(BrowserLocation::parse("index.html", &root).unwrap());
        let resources = host.resources.clone();
        assert_eq!(
            resources
                .resolve(tab.id, "GET", &format!("unluminous://tab-{}/index.html", tab.id))
                .status,
            200
        );
        host.close_tab(tab.id);
        assert_eq!(
            resources
                .resolve(tab.id, "GET", &format!("unluminous://tab-{}/index.html", tab.id))
                .status,
            404
        );
        assert!(resources.changed_tabs().is_empty());
    }

    /// `task-1756`: an id is never reused, and a tab dropped by a whole-window change is forgotten.
    #[test]
    fn tab_ids_are_unique_and_dropped_tabs_are_retained_away() {
        let root = fixture("retain");
        std::fs::write(root.join("index.html"), "<p>ok</p>").unwrap();
        let mut host = BrowserHost::new();
        let first = host.open_tab(BrowserLocation::parse("index.html", &root).unwrap());
        let second =
            host.open_tab(BrowserLocation::Remote { url: "https://example.com/".to_owned() });
        assert_ne!(first.id, second.id);
        host.resources.retain(&HashSet::from([second.id]));
        assert_eq!(
            host.resources
                .resolve(first.id, "GET", &format!("unluminous://tab-{}/index.html", first.id))
                .status,
            404
        );
    }

    /// `task-1756`: the one view goes to the pane with the keyboard, and nowhere while occluded.
    #[test]
    fn one_placement_is_chosen_and_an_occluded_frame_chooses_none() {
        let area = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(100.0));
        let panes =
            [BrowserPlacement::whole(1, area, false), BrowserPlacement::whole(2, area, true)];
        assert_eq!(
            choose(&panes, &[], true).map(|placement| placement.id),
            Some(2),
            "the focused pane holds the view"
        );
        assert_eq!(
            choose(&panes[..1], &[], true).map(|placement| placement.id),
            Some(1),
            "with no focus, the first drawn"
        );
        assert_eq!(
            choose(&panes, &[area], false),
            None,
            "an egui surface over the pane takes the view off the screen where it cannot be cut"
        );
        assert_eq!(choose(&[], &[], true), None);
    }

    /// `task-1756`: a tab switched to ignores the page the shared view is leaving.
    #[test]
    fn a_tab_the_view_is_pointed_back_at_ignores_the_page_it_is_leaving() {
        let mut tab = BrowserTab::new(
            1,
            BrowserLocation::Remote { url: "https://example.com/one".to_owned() },
        );
        tab.arrived_at("https://example.com/one".to_owned());
        // The view is sent back to this tab; the engine reports the other tab's page on the way.
        tab.pointed_at();
        tab.arrived_at("https://example.org/somewhere-else".to_owned());
        assert_eq!(
            tab.current_url(),
            "https://example.com/one",
            "the page it is leaving is not this tab's"
        );
        assert!(!tab.can_go_back(), "so it is offered no way back to it");
        assert!(tab.loading, "and the tab is still waiting for its own page");
        tab.arrived_at("https://example.com/one".to_owned());
        assert!(!tab.loading);
        assert!(!tab.can_go_back() && !tab.can_go_forward());
    }

    /// `task-1756`: a tab's history is its own, so `Back` never lands on another tab's page.
    #[test]
    fn each_tab_remembers_where_it_has_been_by_itself() {
        let mut tab = BrowserTab::new(
            1,
            BrowserLocation::Remote { url: "https://example.com/one".to_owned() },
        );
        assert!(!tab.can_go_back() && !tab.can_go_forward());
        tab.arrived_at("https://example.com/one".to_owned());
        tab.arrived_at("https://example.com/two".to_owned());
        assert_eq!(tab.current_url(), "https://example.com/two");
        assert!(tab.can_go_back() && !tab.can_go_forward());

        let (position, url) = tab.step(true).expect("somewhere to go back to");
        assert_eq!(url, "https://example.com/one");
        tab.heading_for(position);
        assert!(tab.loading);
        tab.arrived_at(url);
        assert_eq!(tab.current_url(), "https://example.com/one");
        assert!(!tab.loading && !tab.can_go_back() && tab.can_go_forward());

        // Somewhere new from here forgets what was ahead, as every browser's history does.
        tab.arrived_at("https://example.com/three".to_owned());
        assert!(!tab.can_go_forward());
        assert_eq!(tab.step(true).map(|(_, url)| url), Some("https://example.com/one".to_owned()));
    }

    /// `task-1756`: the two names WebView2 and WKWebView give one local address are the same address.
    #[test]
    fn a_local_page_is_one_address_under_either_engines_name() {
        let root = fixture("canonical");
        let tab =
            BrowserTab::new(1, BrowserLocation::Local { path: root.join("index.html"), root });
        let asked_for = tab.current_url().to_owned();
        assert_eq!(asked_for, "unluminous://tab-1/index.html");
        let mut tab = tab;
        tab.arrived_at("http://unluminous.tab-1/index.html".to_owned());
        assert_eq!(tab.current_url(), asked_for, "the engine's own name for it is the same page");
        assert!(!tab.can_go_back(), "so the tab has nowhere behind it to go");
        assert_eq!(canonical("https://example.com/a"), "https://example.com/a");
        // And back again, which is the name the engine is given when a tab is sent to its page.
        if cfg!(windows) {
            assert_eq!(engine_url(&asked_for), "http://unluminous.tab-1/index.html");
            assert_eq!(canonical(&engine_url(&asked_for)), asked_for);
        }
        assert_eq!(engine_url("https://example.com/a"), "https://example.com/a");
    }

    /// `task-1756`: the engine's own back gesture inside the page is read as a step, not a new page.
    #[test]
    fn a_back_taken_inside_the_page_moves_rather_than_appends() {
        let mut tab = BrowserTab::new(
            1,
            BrowserLocation::Remote { url: "https://example.com/one".to_owned() },
        );
        tab.arrived_at("https://example.com/one".to_owned());
        tab.arrived_at("https://example.com/two".to_owned());
        tab.arrived_at("https://example.com/one".to_owned());
        assert_eq!(tab.current_url(), "https://example.com/one");
        assert!(tab.can_go_forward(), "the page it came from is still ahead of it");
        assert!(!tab.can_go_back());
    }

    /// `task-1756`: change detection polls the resources a page asked for and nothing else.
    #[test]
    fn only_the_resources_a_page_requested_are_watched() {
        let root = fixture("watched");
        std::fs::write(root.join("index.html"), "<p>ok</p>").unwrap();
        std::fs::write(root.join("unused.css"), "a").unwrap();
        let store = LocalResourceStore::new();
        store.register(5, root.clone());
        store.resolve(5, "GET", "unluminous://tab-5/index.html");
        std::fs::write(root.join("unused.css"), "changed").unwrap();
        assert!(store.changed_tabs().is_empty(), "a file the page never asked for is not watched");
        assert_eq!(store.0.lock().unwrap().get(&5).map(|root| root.resources.len()), Some(1));
    }

    #[test]
    fn browser_tab_names_use_title_file_then_host() {
        let root = fixture("names");
        let local =
            BrowserTab::new(1, BrowserLocation::Local { path: root.join("index.html"), root });
        let remote = BrowserTab::new(
            2,
            BrowserLocation::Remote { url: "https://example.com/page".to_owned() },
        );
        assert_eq!(local.name(), "index.html");
        assert_eq!(remote.name(), "example.com");
    }
}
