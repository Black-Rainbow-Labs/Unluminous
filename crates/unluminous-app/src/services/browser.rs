//! Rendered web pages, their constrained local origin, and the native views that show them.
//!
//! The tab is ordinary application state and the native browser is not. `BrowserTab` can therefore
//! be tested with no window, while `BrowserHost` owns WebView2 or WKWebView and creates either only
//! when a rendered tab is visible. Local pages are served through a custom origin rooted at the
//! project, so linked assets work without giving page JavaScript a filesystem handle.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
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

/// The address a new, empty browser tab is on, before anything has been typed into its address field.
/// `task-2270`'s `New Browser Tab`.
pub const BLANK: &str = "about:blank";

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
        // **An address this tab's own origin wrote down in an earlier run.** A local page is served as
        // `unluminous://tab-<id>/<path>` and the id is handed out again every run, so the address a
        // realm or a project wrote down names a tab that no longer exists. It is read as the file it
        // was, which is what lets a node on a local page come back at all. `task-2203`.
        if let Some(relative) = a_local_tab_address(value) {
            return Self::local(&relative.to_string_lossy(), project);
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
#[derive(Debug, Clone, PartialEq)]
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
    /// How large a tab in the editing area draws its page, which `Ctrl`/`Cmd` with the wheel or with plus and
    /// minus changes (`task-2255`). Kept on the tab because one native view is shared by every tab, so the view
    /// is given each tab's own zoom when it moves to that tab. A browser node's zoom is the canvas's.
    pub zoom: f64,
}

/// The least and the most a page in a tab can be zoomed to, which are the engine's own limits.
pub const PAGE_ZOOM_LEAST: f64 = 0.25;
pub const PAGE_ZOOM_MOST: f64 = 5.0;

/// The zoom one step up or down from `zoom`: the steps a browser takes, ten percent at a time near one.
///
/// @param zoom - the zoom now
/// @param larger - which way
pub fn step_page_zoom(zoom: f64, larger: bool) -> f64 {
    const STEPS: [f64; 15] =
        [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 3.0, 5.0];
    let next = match larger {
        true => STEPS.iter().copied().find(|step| *step > zoom + 0.001),
        false => STEPS.iter().rev().copied().find(|step| *step < zoom - 0.001),
    };
    next.unwrap_or(zoom).clamp(PAGE_ZOOM_LEAST, PAGE_ZOOM_MOST)
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
            zoom: 1.0,
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

    /// Every address this tab has been at, oldest first, and which of them it is on.
    ///
    /// What a project and a realm write down, so a tab comes back with a `Back` that works. `task-2203`.
    pub fn history(&self) -> (&[String], usize) {
        (&self.history, self.position)
    }

    /// Put back a history written down in an earlier run, keeping the page this tab was opened on.
    ///
    /// **The tab keeps the address it was opened with, and the history is put around it.** A tab is opened
    /// at the address that was written down as its current one, so that address replaces the entry at the
    /// position: the two name the same page, and the one the tab was opened with is the spelling the engine
    /// is about to report. A local page's addresses carry the tab's id in their origin, and the id is new
    /// every run, so each one is moved under this tab's own.
    pub fn restore_history(&mut self, history: &[String], position: usize) {
        let mut history: Vec<String> = history
            .iter()
            .filter(|url| !url.trim().is_empty())
            .map(|url| under_tab(url, self.id))
            .collect();
        if history.is_empty() {
            return;
        }
        let position = position.min(history.len() - 1);
        history[position] = self.current_url().to_owned();
        self.history = history;
        self.position = position;
    }

    /// The concise label shown in the file tab strip.
    pub fn name(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        if self.current_url() == BLANK {
            return "New Tab".to_owned();
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
#[derive(Debug, Clone, PartialEq)]
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
    /// width. `app::realm::show_a_browser_node` works out the product of the node's own zoom and the
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
    /// What else on the canvas is drawn over this page, in the window's own points, cut to the pane.
    ///
    /// `task-2207`: *"The web browser node doesn't respect the bring to front/back order. the node seems to
    /// but the content is always on top of everything."* A native child is a window, so it paints above
    /// everything egui draws, whatever place its node has in the stacking order. The occluders the window
    /// reads off egui's layers cannot see this, because every canvas node is drawn into a background layer.
    /// So the canvas says which nodes are in front of this one, and they are cut out of the page exactly as
    /// a menu over it is.
    pub over: Vec<Rect>,
    /// Whether this is a video that is playing. A playing video keeps the one native view when nothing has
    /// the keyboard, so clicking somewhere else on the canvas does not stop it. `task-2207`.
    pub playing: bool,
}

impl BrowserPlacement {
    /// A placement that is wholly visible, which is every page drawn in a pane.
    pub fn whole(id: u64, area: Rect, focused: bool) -> Self {
        Self { id, area, visible: area, focused, zoom: None, over: Vec::new(), playing: false }
    }

    /// Whether the page is what is showing at this point: inside the part that may be painted, and not under
    /// a node drawn in front of it.
    ///
    /// `task-2317`: *"If two browser nodes are overlapping in realm, and I click the top of the one that is on
    /// top, and its bar is overlaying the one below, i'm unable to drag the node."* A press on the front node's
    /// header was read as a press on the page behind it, because only `visible` was asked. The page behind
    /// then took the keyboard and the operating system's focus in the middle of the press, and the header's
    /// drag never started.
    pub fn shows(&self, at: egui::Pos2) -> bool {
        self.visible.contains(at) && !self.over.iter().any(|over| over.contains(at))
    }
}

/// The interface zoom the window is drawn at, as bits of an `f32`, for the native views to read.
///
/// `task-2280`. A native child is placed in the window's logical points, which are egui's points times
/// egui's zoom factor, and the views are placed from code that has no `egui::Context` to ask.
static INTERFACE_ZOOM: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0x3F80_0000);

/// Tell the native views the interface zoom. Called once a frame by the window.
pub fn follow_the_interface(zoom: f32) {
    if zoom.is_finite() && zoom > 0.0 {
        INTERFACE_ZOOM.store(zoom.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

/// A rectangle in egui's points as the window's logical points, which is what a native view is placed in.
pub fn to_logical_points(rect: Rect) -> Rect {
    let zoom = f32::from_bits(INTERFACE_ZOOM.load(std::sync::atomic::Ordering::Relaxed));
    Rect::from_min_max((rect.min.to_vec2() * zoom).to_pos2(), (rect.max.to_vec2() * zoom).to_pos2())
}

/// The part of `visible` left once every rectangle in `over` is taken out of it, as rectangles that do
/// not overlap.
///
/// A platform that crops a native child by a list of rectangles rather than by a region, which is macOS
/// with a layer mask, needs the answer this way: two holes that overlap would otherwise be counted twice.
/// Each hole splits what is left of every piece it touches into at most four, above, below, left and right.
pub fn pieces_of(visible: Rect, over: &[Rect]) -> Vec<Rect> {
    let mut pieces = vec![visible];
    for hole in over {
        let mut next = Vec::with_capacity(pieces.len() + 4);
        for piece in pieces {
            let cut = piece.intersect(*hole);
            if !cut.is_positive() {
                next.push(piece);
                continue;
            }
            let above = Rect::from_min_max(piece.min, egui::pos2(piece.max.x, cut.min.y));
            let below = Rect::from_min_max(egui::pos2(piece.min.x, cut.max.y), piece.max);
            let left = Rect::from_min_max(
                egui::pos2(piece.min.x, cut.min.y),
                egui::pos2(cut.min.x, cut.max.y),
            );
            let right = Rect::from_min_max(
                egui::pos2(cut.max.x, cut.min.y),
                egui::pos2(piece.max.x, cut.max.y),
            );
            next.extend([above, below, left, right].into_iter().filter(|part| part.is_positive()));
        }
        pieces = next;
    }
    pieces
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
    /// Open the element picker in the page. A browser node's toolbar only. `task-2203`.
    Pick,
    /// Put the picker away with nothing chosen.
    CancelPick,
    /// Show the whole page again after one element of it was shown alone.
    Unpin,
}

/// Something the embedded engine reported back to the application.
#[derive(Debug, Clone, PartialEq)]
pub enum BrowserEvent {
    LoadStarted {
        id: u64,
        url: String,
    },
    LoadFinished {
        id: u64,
        url: String,
    },
    Title {
        id: u64,
        title: String,
    },
    OpenRequested {
        source: u64,
        url: String,
    },
    /// `Select Element` was chosen on the page's own right click menu, at a point in the page's own pixels.
    /// `task-2203`.
    PickAsked {
        id: u64,
        at: Option<(f64, f64)>,
    },
    /// `Show Whole Page` was chosen on the page's own right click menu.
    UnpinAsked {
        id: u64,
    },
    /// The picker was answered with the element to show.
    Picked {
        id: u64,
        selector: String,
    },
    /// The picker was put away with nothing chosen, or the page it was on went away.
    PickCancelled {
        id: u64,
    },
    /// A script the host is watching a page with had something to say, as the JSON it answered with.
    /// `task-2254`: the Designer's overlay reports a click, a choice and where its pins are this way.
    Reported {
        id: u64,
        json: String,
    },
    /// The page was asked to zoom with `Ctrl`/`Cmd` and the wheel or the keys, and posted that instead of
    /// zooming itself: one step in for a positive `steps`, out for a negative one, and back to its own size
    /// for `0`. `task-2336`. See [`ZOOM_GESTURES`].
    ZoomAsked {
        id: u64,
        steps: i32,
    },
}

/// A script the host keeps in a page and asks for its answers, the way it asks the picker.
///
/// `install` is evaluated in the page whenever it finishes loading, and `take` is evaluated every
/// [`PICK_POLL`] and answers the JSON of what happened since it was last asked. There is no bridge: the
/// page is given nothing to call, so a page that wrote `window.__unluminousDesigner` itself could only
/// change what the host reads, and the host reads only a shape. `task-2254` section 7.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub install: String,
    pub take: String,
}

/// What the page's own right click menu offers, for the page that is showing. `task-2203`.
///
/// Shared with the engine's menu callback, which runs when the menu opens, and set by the window each frame
/// from what it knows about the tab that is showing: whether it is a browser node, and whether that node is
/// showing one element.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageMenu {
    /// `Select Element` is offered. True for a browser node, never for a tab in the editing area.
    pub pickable: bool,
    /// `Show Whole Page` is offered as well.
    pub pinned: bool,
}

/// The script that holds the element picker and the pin, evaluated in the page when either is wanted.
///
/// It defines two objects on the page's `window` and gives the page nothing to call: the host asks for the
/// picker's answer itself. See `browser_pin.js` and §3.3 of the design.
pub const PIN_SCRIPT: &str = include_str!("browser_pin.js");

/// How often the host asks the picker for its answer while it is open.
const PICK_POLL: Duration = Duration::from_millis(150);

/// How long a question to a watched page is waited for before it is asked again.
const WATCH_PATIENCE: Duration = Duration::from_secs(2);

/// How often the session cookies are read while a page is open, and how soon after a page finishes loading.
const COOKIES_EVERY: Duration = Duration::from_secs(30);
const COOKIES_AFTER_A_LOAD: Duration = Duration::from_secs(2);

/// The script that asks the picker for its answer, which the engine hands back as JSON.
const TAKE_THE_PICK: &str =
    "window.__unluminousPicker ? window.__unluminousPicker.take() : { state: 'gone' }";

/// The script that starts the picker, with the point to start at in the page's own pixels.
fn start_the_picker(at: Option<(f64, f64)>) -> String {
    let (x, y) = match at {
        Some((x, y)) if x.is_finite() && y.is_finite() => (format!("{x:.1}"), format!("{y:.1}")),
        _ => ("undefined".to_owned(), "undefined".to_owned()),
    };
    format!("{PIN_SCRIPT}\nwindow.__unluminousPicker.start({x}, {y});")
}

/// The script that shows only the element `selector` finds.
pub fn pin_script(selector: &str) -> String {
    let quoted = serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".to_owned());
    format!("{PIN_SCRIPT}\nwindow.__unluminousPin.apply({quoted});")
}

/// The script that shows the whole page again.
const UNPIN_SCRIPT: &str = "window.__unluminousPin && window.__unluminousPin.clear();";

/// What the picker's answer, as the engine reported it, means.
fn read_the_pick(id: u64, json: &str) -> Option<BrowserEvent> {
    let answer: serde_json::Value = serde_json::from_str(json).ok()?;
    match answer["state"].as_str()? {
        "picked" => {
            let selector = answer["selector"].as_str()?.trim().to_owned();
            Some(match selector.is_empty() {
                true => BrowserEvent::PickCancelled { id },
                false => BrowserEvent::Picked { id, selector },
            })
        }
        "cancelled" | "gone" => Some(BrowserEvent::PickCancelled { id }),
        _ => None,
    }
}

/// How many native views a window keeps, the hidden ones included (`task-2270`).
///
/// A view that is not showing is kept so that switching back to its tab shows the page at once rather than
/// loading it again; past this many, the one hidden longest is dropped and made again when it is next shown.
/// Measured on `task-1756`, a view costs about 330 MB with its engine processes, and hidden views at
/// WebView2's low memory target cost a fraction of that.
pub const MOST_LIVE_VIEWS: usize = 6;

/// The script that makes a link a person clicks open in a new tab (`task-2270`).
///
/// *"Click on page links should open new tabs by default."* It listens for a plain left click on a link to
/// another page and asks the browser for a new window with `window.open`, which the view's new window handler
/// turns into [`BrowserEvent::OpenRequested`]. **It gives the page nothing to call**: it uses the browser's own
/// new window mechanism, so the rule that a page has no host bridge stands. A click with a modifier, a link to a
/// place on the same page, a `download` link and an address that is not a page are left to the page.
pub const LINKS_IN_NEW_TABS: &str = include_str!("browser_links.js");

/// The script that hands `Ctrl`/`Cmd` with the wheel, plus, minus and zero to Unluminous (`task-2336`).
///
/// *"When hovering over browser node, CMD + and - should zoom/enlarge the web browser content … Same with CMD +
/// scroll."* The engine zoomed the page itself, and nothing told Unluminous: a browser node's page is drawn at
/// its node's zoom times the canvas's, so the next move of the canvas sent that product again and threw the
/// person's zoom away. On macOS `wry` cannot turn the engine's zoom on at all, so the wheel did nothing.
///
/// **This is the one message a page can send the host**, through `window.ipc.postMessage`, and
/// [`zoom_message`] reads only the three words it posts. Anything else a page posts is ignored. It is put in
/// every page, which is the only way the gesture reaches Unluminous while the page holds the pointer.
pub const ZOOM_GESTURES: &str = include_str!("browser_zoom.js");

/// What a page's post asked for: `Some(1)` one step in, `Some(-1)` one out, `Some(0)` its own size again,
/// and `None` for anything that is not one of [`ZOOM_GESTURES`]'s three messages.
///
/// @param message - what the page posted
pub fn zoom_message(message: &str) -> Option<i32> {
    match message {
        "unluminous-zoom:in" => Some(1),
        "unluminous-zoom:out" => Some(-1),
        "unluminous-zoom:reset" => Some(0),
        _ => None,
    }
}

/// The native browser views a window owns, and the local roots its tabs read under.
///
/// **One native view a rendered tab, since `task-2270`.** *"we need to be able to show more than one window at a
/// time. right now only one web page renders at a time."* Until then a window had one view, pointed at whichever
/// tab was showing, because `task-1756` measured a second WebView2 controller hanging the window in a nested
/// message pump. That measurement was taken while views were still made inside the egui pass; `task-1756` then
/// moved the making to before the pass, and the two view case was not measured again. `task-2270` measured it with
/// views made before the pass, one a frame, and two editing panes and three browser nodes all rendered at once with
/// no hang: `tasks/task-2270-unluminous-issues-v20-tdd.md` §5 has the numbers.
///
/// `UNLUMINOUS_BROWSER_VIEWS=one` keeps a window to one view at a time, which is what it was before, so the two
/// can be compared on one build.
/// The id the next browser tab opened in this process is given.
///
/// **One counter for the process, not one a host** (`task-2322`). A window holds one `UnluminousApp` a project
/// tab and each has a host of its own, and a tab id is what a native view, a local resource root and every
/// browser event are keyed by. Two hosts counting from one each would give two projects the same ids.
static NEXT_TAB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct BrowserHost {
    profile: Option<PathBuf>,
    /// The session cookies kept beside the profile, read back when a view is made. `task-2203`.
    session: Option<crate::services::browser_session::SessionCookies>,
    /// What was last written to that file, so a read that found the same cookies writes nothing.
    cookies_written: Option<Vec<crate::services::browser_session::KeptCookie>>,
    /// When the cookies are read next.
    cookies_due: Option<Instant>,
    /// The tab the picker is open on, shared with the callback that reads its answer.
    picking: Arc<Mutex<Option<u64>>>,
    /// Whether an answer has been asked for and not yet given, so two are never in flight.
    asking: Arc<std::sync::atomic::AtomicBool>,
    last_ask: Instant,
    /// The tab a [`Watch`] is kept on, and the script.
    watching: Option<(u64, Watch)>,
    /// Whether the watch's script has to be put in the page again, because the page just loaded.
    install_due: bool,
    /// Whether the watch has asked a question and not had its answer, with when it asked.
    watch_asking: Arc<std::sync::atomic::AtomicBool>,
    watch_asked_at: Instant,
    /// What each page's own right click menu offers, by tab. See [`PageMenu`].
    menus: Arc<Mutex<HashMap<u64, PageMenu>>>,
    resources: LocalResourceStore,
    /// The page with the keyboard, or the one used last: what the picker, the page's menu and a click into a
    /// page are about. `0` is none.
    showing: u64,
    /// Whether a window keeps to one view at a time. See [`BrowserHost`].
    one_view: bool,
    /// Whether a link a person clicks opens a new tab. `browser.links`. `task-2270`.
    links_open_new_tabs: bool,
    sender: std::sync::mpsc::Sender<BrowserEvent>,
    receiver: std::sync::mpsc::Receiver<BrowserEvent>,
    native: native::NativeHost,
    last_resource_check: Instant,
    /// Which browser pages are told they are talking to. See `settings::BrowserAgent`. `task-2255`.
    agent: crate::settings::BrowserAgent,
}

impl BrowserHost {
    /// A lazy host. Constructing an ordinary Unluminous window starts no browser process.
    pub fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self {
            profile: None,
            session: None,
            cookies_written: None,
            cookies_due: None,
            picking: Arc::new(Mutex::new(None)),
            asking: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_ask: Instant::now(),
            watching: None,
            install_due: false,
            watch_asking: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            watch_asked_at: Instant::now(),
            menus: Arc::new(Mutex::new(HashMap::new())),
            resources: LocalResourceStore::new(),
            showing: 0,
            one_view: std::env::var("UNLUMINOUS_BROWSER_VIEWS").is_ok_and(|views| views == "one"),
            links_open_new_tabs: true,
            sender,
            receiver,
            native: native::NativeHost::new(),
            last_resource_check: Instant::now(),
            agent: crate::settings::BrowserAgent::Default,
        }
    }

    /// Say which browser pages are told they are talking to.
    ///
    /// The engine takes its `User-Agent` when a view is made, so a change drops the views, and the next frames
    /// make them again with the new one and put each page back in its own. Comparing the setting is all a frame
    /// that changed nothing costs.
    pub fn set_agent(&mut self, agent: crate::settings::BrowserAgent) {
        if agent == self.agent {
            return;
        }
        self.agent = agent;
        self.native.set_user_agent(agent.header());
        if self.native.has_view() {
            self.native.forget();
            self.showing = 0;
        }
    }

    /// Which browser pages are told they are talking to.
    pub fn agent(&self) -> crate::settings::BrowserAgent {
        self.agent
    }

    /// Say whether a link a person clicks opens in a new tab. A view takes it when it is made, so a change
    /// drops the views, the way a change of `User-Agent` does. `task-2270`.
    pub fn set_links_open_new_tabs(&mut self, new_tabs: bool) {
        if new_tabs == self.links_open_new_tabs {
            return;
        }
        self.links_open_new_tabs = new_tabs;
        if self.native.has_view() {
            self.native.forget();
            self.showing = 0;
        }
    }

    /// Whether a window keeps to one view at a time, which `UNLUMINOUS_BROWSER_VIEWS=one` asks for.
    pub fn one_view(&self) -> bool {
        self.one_view
    }

    /// Keep browser cookies and caches in Unluminous's per-user settings folder.
    pub fn set_profile(&mut self, folder: PathBuf) {
        if !self.has_views() {
            self.session =
                Some(crate::services::browser_session::SessionCookies::in_profile(&folder));
            self.profile = Some(folder);
        }
    }

    /// Say what tab `id`'s page offers on its own right click menu.
    pub fn set_page_menu(&self, id: u64, menu: PageMenu) {
        if let Ok(mut held) = self.menus.lock() {
            held.insert(id, menu);
        }
    }

    /// Open the element picker in tab `id`'s page, starting at a point in the page's own pixels.
    pub fn start_picking(&mut self, id: u64, at: Option<(f64, f64)>) -> Result<(), String> {
        self.native.evaluate(id, &start_the_picker(at))?;
        if let Ok(mut picking) = self.picking.lock() {
            *picking = Some(id);
        }
        self.asking.store(false, Ordering::Relaxed);
        self.last_ask = Instant::now();
        Ok(())
    }

    /// The tab the picker is open on, if it is open.
    pub fn picking(&self) -> Option<u64> {
        self.picking.lock().ok().and_then(|picking| *picking)
    }

    /// Put the picker away without choosing anything.
    pub fn cancel_picking(&mut self) {
        let Some(id) = self.picking() else { return };
        let _ = self
            .native
            .evaluate(id, "window.__unluminousPicker && window.__unluminousPicker.cancel();");
        if let Ok(mut picking) = self.picking.lock() {
            *picking = None;
        }
        let _ = self.sender.send(BrowserEvent::PickCancelled { id });
    }

    /// Keep a script in the page of tab `id` and ask it for its answers, or put the watch away with `None`.
    ///
    /// Setting the watch it already has changes nothing, so the window can say it on every frame it
    /// draws the tab. A different watch, or the first one, is put in the page at the next chance.
    ///
    /// @param id - the tab
    /// @param watch - the scripts, or nothing to stop watching `id`
    pub fn watch(&mut self, id: u64, watch: Option<Watch>) {
        match watch {
            Some(watch) => {
                if self.watching.as_ref() != Some(&(id, watch.clone())) {
                    self.watching = Some((id, watch));
                    self.install_due = true;
                }
            }
            None => {
                if self.watching.as_ref().is_some_and(|(held, _)| *held == id) {
                    self.watching = None;
                }
            }
        }
    }

    /// The tab a watch is kept on, if there is one.
    pub fn watching(&self) -> Option<u64> {
        self.watching.as_ref().map(|(id, _)| *id)
    }

    /// Put the watch's script in the page when it has just loaded, and ask it what happened, at most once
    /// every [`PICK_POLL`] and never twice at once.
    ///
    /// **Before the egui pass**, from [`Self::reconcile`], like the picker's question. An answer that never
    /// arrives, because the page went away between the question and the answer, is given up on after two
    /// seconds rather than silencing the watch for good.
    fn ask_the_watch(&mut self) {
        let Some(id) = self.watching() else { return };
        if !self.native.has_view_for(id) {
            return;
        }
        if self.install_due {
            let install = self.watching.as_ref().map(|(_, watch)| watch.install.clone());
            if let Some(install) = install {
                if self.native.evaluate(id, &install).is_ok() {
                    self.install_due = false;
                }
            }
        }
        let busy = self.watch_asking.load(Ordering::Relaxed)
            && self.watch_asked_at.elapsed() < WATCH_PATIENCE;
        if busy || self.watch_asked_at.elapsed() < PICK_POLL {
            return;
        }
        self.watch_asking.store(true, Ordering::Relaxed);
        self.watch_asked_at = Instant::now();
        let take = self.watching.as_ref().map(|(_, watch)| watch.take.clone()).unwrap_or_default();
        let (sender, asking) = (self.sender.clone(), self.watch_asking.clone());
        let asked = self.native.evaluate_with_callback(id, &take, move |json| {
            asking.store(false, Ordering::Relaxed);
            if json.contains("\"kind\"") {
                let _ = sender.send(BrowserEvent::Reported { id, json });
            }
        });
        if asked.is_err() {
            self.watch_asking.store(false, Ordering::Relaxed);
        }
    }

    /// Show only the element `selector` finds, in tab `id`'s page.
    pub fn pin(&self, id: u64, selector: &str) -> Result<(), String> {
        self.native.evaluate(id, &pin_script(selector))
    }

    /// Show the whole page again.
    pub fn unpin(&self, id: u64) -> Result<(), String> {
        self.native.evaluate(id, UNPIN_SCRIPT)
    }

    /// Ask the picker for its answer, at most once every [`PICK_POLL`] and never twice at once.
    fn ask_the_picker(&mut self) {
        let Some(id) = self.picking() else { return };
        if !self.native.has_view_for(id) {
            // The page the picker was on has no view any more, so it is not there to answer.
            if let Ok(mut picking) = self.picking.lock() {
                *picking = None;
            }
            let _ = self.sender.send(BrowserEvent::PickCancelled { id });
            return;
        }
        if self.last_ask.elapsed() < PICK_POLL || self.asking.swap(true, Ordering::Relaxed) {
            return;
        }
        self.last_ask = Instant::now();
        let (sender, picking, asking) =
            (self.sender.clone(), self.picking.clone(), self.asking.clone());
        let asked = self.native.evaluate_with_callback(id, TAKE_THE_PICK, move |json| {
            asking.store(false, Ordering::Relaxed);
            if let Some(event) = read_the_pick(id, &json) {
                if let Ok(mut open) = picking.lock() {
                    if *open == Some(id) {
                        *open = None;
                        let _ = sender.send(event);
                    }
                }
            }
        });
        if asked.is_err() {
            self.asking.store(false, Ordering::Relaxed);
        }
    }

    /// Read the session cookies and write them down when they changed. `task-2203`.
    ///
    /// **Asked on one call and taken on a later one.** On Windows the answer is there at once, because
    /// `WebView::cookies` waits on the engine with a nested message pump, and that is why this is called
    /// before the egui pass, from [`Self::reconcile`]: `task-1756` measured what a nested pump inside the
    /// pass does. On macOS WebKit answers on a later turn of the run loop and nothing waits for it, because
    /// `wry`'s wait runs the run loop by hand, and a redraw that arrives during it reaches `winit` while it is
    /// still handling this frame: `winit` panics and the process aborts. An answer is taken whether or not
    /// there is still a view, so the read asked for just before the last view goes is still written down.
    fn keep_the_session(&mut self, now_or_never: bool) {
        let Some(session) = &self.session else { return };
        if self.native.has_view() {
            let now = Instant::now();
            if now_or_never || self.cookies_due.is_none_or(|due| now >= due) {
                self.cookies_due = Some(now + COOKIES_EVERY);
                self.native.ask_for_session_cookies();
            }
        }
        let Some(cookies) = self.native.take_session_cookies() else { return };
        if self.cookies_written.as_ref() == Some(&cookies) {
            return;
        }
        if session.write(&cookies).is_ok() {
            self.cookies_written = Some(cookies);
        }
    }

    /// Note that a page finished loading, so the cookies it set are read soon rather than in half a minute.
    fn a_page_loaded(&mut self) {
        let soon = Instant::now() + COOKIES_AFTER_A_LOAD;
        if self.cookies_due.is_none_or(|due| due > soon) {
            self.cookies_due = Some(soon);
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

    /// The tab whose page somebody has clicked into: it holds the operating system's keyboard and the pointer
    /// is over it. See `UnluminousApp::settle_the_native_views_before_the_pass`. `task-2255`.
    pub fn the_page_clicked_into(&self) -> Option<u64> {
        self.native.the_page_clicked_into()
    }

    /// Whether any native view exists, which is what decides if there is anything to settle.
    pub fn has_views(&self) -> bool {
        self.native.has_view()
    }

    /// How many native views there are, the hidden ones included, which `browser status` reports.
    pub fn view_count(&self) -> usize {
        self.native.count()
    }

    /// Remember the window a child view is created inside, which is the one thing reconciling needs
    /// from the frame and the one thing it cannot be handed outside the egui pass.
    pub fn remember_window(&mut self, frame: &eframe::Frame) {
        self.native.remember_window(frame);
    }

    /// The page with the keyboard, or the one used last, which is what the picker and the page's own menu are
    /// about.
    pub fn showing(&self) -> Option<u64> {
        match self.showing {
            0 => None,
            id => Some(id),
        }
    }

    /// Whether tab `id`'s page has a native view of its own, which is what a command that drives the page needs.
    pub fn is_live(&self, id: u64) -> bool {
        self.native.has_view_for(id)
    }

    /// Whether tab `id`'s page is drawn live where it is placed, rather than as a picture or a sentence. Every
    /// placed page is, unless the window keeps to one view, and then only the one showing is.
    pub fn renders(&self, id: u64) -> bool {
        !self.one_view || self.showing().is_none_or(|showing| showing == id)
    }

    /// The videos the realm's video nodes play. `task-2202`.
    pub fn media(&self) -> &MediaStore {
        self.resources.media()
    }

    /// Run a script in tab `id`'s page. `task-2202`: how `realm play` reaches a video.
    pub fn evaluate(&self, id: u64, script: &str) -> Result<(), String> {
        self.native.evaluate(id, script)
    }

    /// Allocate a stable id and register any local root before the page can request an asset.
    pub fn open_tab(&mut self, location: BrowserLocation) -> BrowserTab {
        let id = NEXT_TAB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let BrowserLocation::Local { root, .. } = &location {
            self.resources.register(id, root.clone());
        }
        BrowserTab::new(id, location)
    }

    /// Take every page of this host off the screen, keeping each view ready to be shown again.
    ///
    /// What a project tab that stops showing asks for (`task-2322`): its pages are native child windows, so a
    /// page left visible would be drawn over the project tab that took its place.
    pub fn hide_every_view(&mut self) {
        self.native.hide();
    }

    /// Release everything that belonged to a closed tab: its local root, its watch, its menu and its view.
    pub fn close_tab(&mut self, id: u64) {
        self.resources.unregister(id);
        self.watch(id, None);
        self.native.forget_one(id);
        if let Ok(mut menus) = self.menus.lock() {
            menus.remove(&id);
        }
        if self.showing() == Some(id) {
            self.showing = 0;
        }
    }

    /// The placements that get a view this frame: every one, or only the one chosen when the window keeps to
    /// one view. A tab drawn twice in one frame is placed where it was drawn last.
    fn the_pages_to_show<'a>(
        &self,
        placements: &'a [BrowserPlacement],
        occluders: &[egui::Rect],
    ) -> Vec<&'a BrowserPlacement> {
        if self.one_view {
            return choose(placements, occluders, CAN_CUT_A_PAGE, self.showing())
                .into_iter()
                .collect();
        }
        let mut shown: Vec<&BrowserPlacement> = Vec::with_capacity(placements.len());
        for placement in placements {
            shown.retain(|earlier| earlier.id != placement.id);
            shown.push(placement);
        }
        shown
    }

    /// Move every view to where this frame drew its page, at the end of the frame.
    ///
    /// `task-2207`: *"Web browser node contents dont zoom at the same time as everything else. seems to be
    /// delayed in catching up."* [`Self::reconcile`] runs before the egui pass and so places a view where
    /// the **previous** frame drew its node: during a pan or a zoom of the canvas the page was always one
    /// frame behind the node around it, on top of whatever the engine itself takes to lay the page out at a
    /// new zoom. This moves it again once the frame that drew the node has finished, so the page and its
    /// node are presented together.
    ///
    /// **Only a move.** Making a view stays before the pass, because making one waits in a nested message
    /// pump that the pass must not be inside; moving one, zooming it and setting its crop return at once.
    pub fn follow(&mut self, placements: &[BrowserPlacement], occluders: &[egui::Rect]) {
        let shown = self.the_pages_to_show(placements, occluders);
        self.native.follow(&shown, occluders);
    }

    /// Make, place and hide the native views. Called before the egui pass, never inside it.
    ///
    /// `links_stay` are the tabs whose links stay where they are whatever `browser.links` says, which are the
    /// Designer's pages.
    pub fn reconcile(
        &mut self,
        tabs: &[BrowserTab],
        placements: &[BrowserPlacement],
        occluders: &[egui::Rect],
        repaint: egui::Context,
        links_stay: &[u64],
    ) -> Settled {
        let live: HashSet<u64> = tabs.iter().map(|tab| tab.id).collect();
        self.resources.retain(&live);
        if let Ok(mut menus) = self.menus.lock() {
            menus.retain(|id, _| live.contains(id));
        }
        self.ask_the_picker();
        self.ask_the_watch();
        // Read once more just before the last view goes, because nothing will read them afterwards.
        self.keep_the_session(tabs.is_empty());
        if tabs.is_empty() {
            self.native.forget();
            self.showing = 0;
            return Settled::default();
        }
        let chosen = going_to(placements, self.showing()).map(|placement| placement.id);
        let shown = self.the_pages_to_show(placements, occluders);
        if shown.is_empty() {
            self.native.hide();
            // **A page that is there and is not being drawn**, which is what `browser status` reports
            // and the only way anything can be asked whether a menu is covering one. `task-2009`.
            return Settled { covered: !placements.is_empty(), ..Settled::default() };
        }
        let covered = shown.iter().any(|placement| covers_any_of(occluders, placement.visible));
        // The session cookies are read from the file once, when there is about to be a first view to give them to.
        let cookies = match (self.native.has_view(), &self.session) {
            (false, Some(session)) => session.read(),
            _ => Vec::new(),
        };
        if !self.native.has_view() {
            self.cookies_written = Some(cookies.clone());
            self.cookies_due = Some(Instant::now() + COOKIES_EVERY);
        }
        let (problems, made) = self.native.settle(native::Settle {
            tabs,
            placements: &shown,
            cookies: &cookies,
            menus: self.menus.clone(),
            profile: self.profile.clone(),
            resources: self.resources.clone(),
            sender: self.sender.clone(),
            occluders,
            repaint,
            most: if self.one_view { 1 } else { MOST_LIVE_VIEWS },
            links_open_new_tabs: self.links_open_new_tabs,
            links_stay,
        });
        if let Some(chosen) = chosen {
            self.showing = chosen;
        }
        Settled { problems, pointed_at: made, covered }
    }

    /// Send tab `id`'s view to an address on behalf of that tab.
    pub fn navigate(&self, id: u64, url: &str) -> Result<(), String> {
        self.native.navigate(id, url)
    }

    /// Reload tab `id`'s page.
    pub fn reload(&self, id: u64) -> Result<(), String> {
        self.native.reload(id)
    }

    /// How big tab `id` draws its page. A tab whose view is still to be made takes the zoom from its placement
    /// when it is made, which is why a node's zoom is remembered in `realm::live::Live`.
    pub fn zoom(&self, id: u64, factor: f64) -> Result<(), String> {
        self.native.zoom(id, factor)
    }

    /// A picture of tab `id`'s page, as it is on the screen now, or `None` when it has no view or the platform
    /// cannot take one in the moment it is asked.
    ///
    /// `task-2253`: *"In realm view, the web browser content adjusts as I zoom in and out, however, it's very
    /// jagged/jittery and isn't smooth. Often the content will be a bit smaller than the node as I'm zooming
    /// and will bounce fill until I stop."* The engine lays a page out again on its own process after every
    /// change of zoom, so a page that is given a new zoom on every frame of a glide is always drawn at a zoom
    /// a few frames old inside bounds that are not. The canvas draws this picture in the page's place while
    /// its zoom moves, scaled by the same transform as everything else in the node, and gives the live page
    /// its new size once the camera has stopped. See `app::realm::show_a_browser_node`.
    ///
    /// @param id - the tab
    pub fn photograph(&self, id: u64) -> Option<egui::ColorImage> {
        self.native.photograph(id, false)
    }

    /// The same picture, kept even when the page is one colour all over.
    ///
    /// A new tab's empty page is one colour, which [`Self::photograph`] reads as a capture that did not
    /// work. Under a popup a picture of one colour is exactly what is wanted, and with none the hole the
    /// popup is cut out of showed the background picture (`task-2320`).
    ///
    /// @param id - the tab
    pub fn photograph_even_if_plain(&self, id: u64) -> Option<egui::ColorImage> {
        self.native.photograph(id, true)
    }

    /// Drain browser callbacks at the top of a frame.
    pub fn take_events(&mut self) -> Vec<BrowserEvent> {
        let events: Vec<BrowserEvent> = self.receiver.try_iter().collect();
        if events.iter().any(|event| matches!(event, BrowserEvent::LoadFinished { .. })) {
            self.a_page_loaded();
        }
        // A page that finished loading is a new document, so a watch's script has to go in it again.
        let watched = self.watching();
        if events.iter().any(
            |event| matches!(event, BrowserEvent::LoadFinished { id, .. } if Some(*id) == watched),
        ) {
            self.install_due = true;
        }
        events
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

/// What settling the native views did this frame.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settled {
    /// Anything the platform could not do, against the tab that asked for it.
    pub problems: Vec<(u64, String)>,
    /// The tabs whose view was made this frame, and is loading the tab's own address.
    pub pointed_at: Vec<u64>,
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
///
/// **A press on something egui draws over a page is not a press on the page** (`task-2317`), which is what
/// `occluders` are: a menu, a dropdown or a tooltip. A press on the `New Browser Tab` row of a browser tab's
/// own right click menu was read as a press on the page under it, the page took the operating system's
/// keyboard, and the next frame closed every menu because a page had been clicked into, so the row never
/// saw its release and nothing happened.
pub fn the_page_was_the_last_thing_pressed(
    events: &[egui::Event],
    placements: &[BrowserPlacement],
    occluders: &[Rect],
    before: bool,
) -> bool {
    events
        .iter()
        .filter_map(|event| match event {
            egui::Event::PointerButton { pos, pressed: true, .. } => Some(*pos),
            _ => None,
        })
        .fold(before, |_, at| {
            !occluders.iter().any(|over| over.contains(at))
                && placements.iter().any(|placement| placement.shows(at))
        })
}

/// The page a press this frame landed on, when egui saw the press: the last one drawn that holds the point, which is
/// the one on top.
///
/// `task-2263`. A press egui sees inside a page is a press on a page the one native view is not showing, because
/// the live page takes its own clicks. That page is the one the person wants, so it is the one the view goes to.
/// Choosing by which pane had the keyboard instead sent the view to the browser tab when the Designer's page was
/// clicked, because the tab's pane still held the keyboard from a click on its address field.
///
/// A press on something egui draws over the page, such as a menu, names no page, for the reason
/// [`the_page_was_the_last_thing_pressed`] gives.
pub fn the_page_pressed(
    events: &[egui::Event],
    placements: &[BrowserPlacement],
    occluders: &[Rect],
) -> Option<u64> {
    let at = events.iter().rev().find_map(|event| match event {
        egui::Event::PointerButton { pos, pressed: true, .. } => Some(*pos),
        _ => None,
    })?;
    if occluders.iter().any(|over| over.contains(at)) {
        return None;
    }
    placements.iter().rev().find(|placement| placement.shows(at)).map(|placement| placement.id)
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
/// page since `task-1914`. macOS can since `task-2207`, with a mask on the web view's own layer. The Linux
/// build has no web view at all, so the answer there is never asked.
const CAN_CUT_A_PAGE: bool = cfg!(any(windows, target_os = "macos"));

/// The one placement the native view goes to: the pane with the keyboard, else the first drawn.
///
/// A second rendered tab beside the first in a split pane cannot have a view of its own — see
/// [`BrowserHost`] — so it is drawn as a pane that says where its page is.
fn choose<'a>(
    placements: &'a [BrowserPlacement],
    occluders: &[egui::Rect],
    can_cut_the_page_around_them: bool,
    showing: Option<u64>,
) -> Option<&'a BrowserPlacement> {
    // **The last rather than the first when nothing is focused.** A placement is pushed as its owner is
    // drawn, and both the pane loop and the canvas's node loop draw **back to front** — so the first is the
    // one furthest behind. With two browser nodes overlapping and the keyboard somewhere else entirely, the
    // one underneath took the native view and painted above the one on top of it, because a native child
    // composites over everything egui draws. The Codex Sol review of `task-1905` found it.
    //
    // **A playing video comes before the last one drawn**, so clicking the canvas's empty ground, or a node
    // that is not a page, does not take the view away from a video somebody is watching. `task-2207`.
    let chosen = going_to(placements, showing)?;
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
    // A platform that cannot cut a page has to hide it instead. Both platforms with a web view can since
    // `task-2207`, when macOS gained a layer mask, so this is kept for a third. `visible` rather than `area`, because the part of a
    // node hanging off the canvas is not on the screen; and an overlap has to have some area in it,
    // since `Rect::intersects` is true of two rectangles that merely touch along an edge.
    if !can_cut_the_page_around_them && covers_any_of(occluders, chosen.visible) {
        return None;
    }
    Some(chosen)
}

/// The placement the one native view is pointed at this frame, before anything is asked about what covers it.
///
/// The focused one, then a playing video, then **the page the view already shows**, and only then the last one
/// drawn. The third rule is `task-2263`: *"when i have Designer and files open, and click the url of the browser
/// tab, suddenly the page content disappears and the content in Designer appears, and vice versa."* A press on a
/// browser tab's own toolbar, its address field included, lands on none of the pages, so the frame clears every
/// placement's `focused`, and the view went to whichever page was pushed last that frame: the Designer's well when
/// it is drawn after the editing pane, or the tab when it is drawn after the Designer. Nothing about the press
/// asked for the other page. So with nothing focused the view stays where it is, unless a page drawn after it
/// overlaps it, which is the overlapping canvas node case the last one drawn exists for.
pub fn going_to(
    placements: &[BrowserPlacement],
    showing: Option<u64>,
) -> Option<&BrowserPlacement> {
    if let Some(focused) = placements.iter().find(|placement| placement.focused) {
        return Some(focused);
    }
    if let Some(playing) = placements.iter().rev().find(|placement| placement.playing) {
        return Some(playing);
    }
    let staying = showing.and_then(|id| placements.iter().position(|placement| placement.id == id));
    if let Some(at) = staying {
        let page = placements[at].visible;
        let covered_by_a_later_one =
            placements[at + 1..].iter().any(|later| later.visible.intersect(page).area() > 0.0);
        if !covered_by_a_later_one {
            return Some(&placements[at]);
        }
    }
    placements.last()
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
struct LocalResourceStore(Arc<Mutex<HashMap<u64, LocalRoot>>>, MediaStore);

/// The videos a Realm's video nodes play, by node id. `task-2202`, §5.4 of `tasks/task-2199-realm-tdd.md`.
///
/// **A file is served by the node that names it, never by a path in the address.** The page a video node
/// shows is `unluminous://realm/video/<node>` and the file is `unluminous://realm/media/<node>`, and both are
/// answered from this map, which the window fills with the files the open realm's video nodes name after
/// `store::inside` has checked each one is in the project. A page asking for any other number gets a 404.
#[derive(Debug, Clone, Default)]
pub struct MediaStore(Arc<Mutex<HashMap<u64, MediaPage>>>);

/// What one video node's page plays and how.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPage {
    pub file: PathBuf,
    pub volume: f32,
    pub looping: bool,
    pub muted: bool,
    /// Where to start, in seconds.
    pub position: f32,
    /// Whether to start playing as soon as it has loaded, which is what `realm play` on a video nobody is
    /// looking at asks for.
    pub autoplay: bool,
}

impl MediaStore {
    /// Name the file a video node plays, or change how it plays it.
    pub fn register(&self, node: u64, page: MediaPage) {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(node, page);
    }

    /// Forget a video node, so its page and its file are no longer served.
    pub fn forget(&self, node: u64) {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&node);
    }

    fn page(&self, node: u64) -> Option<MediaPage> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&node).cloned()
    }

    /// Answer a request to the `realm` origin, or `None` when it is for another one.
    fn serve(&self, method: &str, uri: &str, range: Option<&str>) -> Option<ResourceReply> {
        let url = Url::parse(uri).ok()?;
        let host = url.host_str()?;
        if host != "realm" && host != "unluminous.realm" {
            return None;
        }
        if method != "GET" && method != "HEAD" {
            return Some(ResourceReply::empty(405));
        }
        let mut parts = url.path().trim_matches('/').split('/');
        let (which, node) = (parts.next()?, parts.next()?);
        let Some(node) = node.parse::<u64>().ok() else { return Some(ResourceReply::empty(404)) };
        let Some(page) = self.page(node) else { return Some(ResourceReply::empty(404)) };
        Some(match which {
            "video" => ResourceReply {
                status: 200,
                mime: "text/html; charset=utf-8".to_owned(),
                bytes: match method {
                    "HEAD" => Vec::new(),
                    _ => video_page(node, &page).into_bytes(),
                },
                headers: Vec::new(),
            },
            "media" => media_reply(&page.file, method, range),
            _ => ResourceReply::empty(404),
        })
    }
}

/// The page a video node shows: the video and the browser's own controls, filling the node, and a title that
/// says where it is, which is how the window learns the position without a script callback. `task-2202`.
pub fn video_page(node: u64, page: &MediaPage) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>realm 0 0 0</title><style>\
         html,body{{margin:0;height:100%;background:#000;overflow:hidden}}\
         video{{width:100%;height:100%;object-fit:contain;display:block}}</style></head><body>\
         <video id=\"v\" controls preload=\"metadata\" src=\"../media/{node}\"></video><script>\
         const v=document.getElementById('v');v.volume={volume};v.loop={looping};v.muted={muted};\
         v.addEventListener('loadedmetadata',()=>{{v.currentTime={position};if({autoplay}){{v.play();}}}});\
         const say=()=>{{document.title='realm '+v.currentTime.toFixed(1)+' '+(v.paused?0:1)+' '+(v.duration||0).toFixed(1);}};\
         ['timeupdate','play','pause','seeked','loadedmetadata'].forEach(e=>v.addEventListener(e,say));\
         </script></body></html>",
        volume = page.volume.clamp(0.0, 1.0),
        looping = page.looping,
        muted = page.muted,
        position = page.position.max(0.0),
        autoplay = page.autoplay,
    )
}

/// What a video page's title says about where it is: `realm <seconds> <1 when playing> <length>`.
pub fn read_a_video_title(title: &str) -> Option<(f32, bool, f32)> {
    let mut words = title.strip_prefix("realm ")?.split_whitespace();
    let position = words.next()?.parse().ok()?;
    let playing = words.next()? == "1";
    let length = words.next()?.parse().ok()?;
    Some((position, playing, length))
}

/// The largest piece of a file one answer carries when the page asked for "the rest of it".
///
/// A video is not read whole into memory: a player asks for `bytes=0-` and then for what it needs, and an
/// answer of a few megabytes at a time is what keeps a two gigabyte file from being two gigabytes of memory.
const MEDIA_CHUNK: u64 = 4 * 1024 * 1024;

/// A media file, or the part of it a `Range` header asks for.
///
/// **A `Range` is answered with 206**, which is not optional: without it WebView2 and WKWebView cannot seek,
/// and WKWebView will not play at all. §5.4.
pub fn media_reply(file: &Path, method: &str, range: Option<&str>) -> ResourceReply {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut opened) = std::fs::File::open(file) else { return ResourceReply::empty(404) };
    let total = opened.metadata().map(|about| about.len()).unwrap_or(0);
    let mime = mime_guess::from_path(file).first_or_octet_stream().essence_str().to_owned();
    let asked = range.and_then(|range| byte_range(range, total));
    let (start, end, status) = match (range, asked) {
        (Some(_), None) => {
            return ResourceReply {
                status: 416,
                mime,
                bytes: Vec::new(),
                headers: vec![("Content-Range", format!("bytes */{total}"))],
            };
        }
        (Some(_), Some((start, end))) => (start, end, 206),
        (None, _) => (0, total.saturating_sub(1), 200),
    };
    let length = if total == 0 { 0 } else { end - start + 1 };
    let mut bytes = Vec::new();
    if method != "HEAD" && length > 0 {
        bytes.resize(length as usize, 0);
        if opened.seek(SeekFrom::Start(start)).is_err() || opened.read_exact(&mut bytes).is_err() {
            return ResourceReply::empty(500);
        }
    }
    let mut headers = vec![("Accept-Ranges", "bytes".to_owned())];
    if status == 206 {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{total}")));
    }
    ResourceReply { status, mime, bytes, headers }
}

/// The first and last byte a `Range: bytes=...` header asks for, within a file `total` bytes long.
///
/// One range only, which is what a media element asks for. An open ended range is cut to [`MEDIA_CHUNK`].
pub fn byte_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (from, to) = spec.split_once('-')?;
    if total == 0 {
        return None;
    }
    let (start, end) = match (from.trim(), to.trim()) {
        ("", suffix) => {
            let suffix: u64 = suffix.parse().ok()?;
            (total.saturating_sub(suffix), total - 1)
        }
        (from, "") => {
            let start: u64 = from.parse().ok()?;
            (start, (start + MEDIA_CHUNK - 1).min(total - 1))
        }
        (from, to) => (from.parse().ok()?, to.parse::<u64>().ok()?.min(total - 1)),
    };
    (start <= end && start < total).then_some((start, end))
}

impl LocalResourceStore {
    /// An empty registry that opens no file and starts no thread.
    fn new() -> Self {
        Self(Arc::new(Mutex::new(HashMap::new())), MediaStore::default())
    }

    /// The videos this store serves for the realm's video nodes.
    fn media(&self) -> &MediaStore {
        &self.1
    }

    /// Answer one request to the custom origin: a video node's page or its file, or a local tab's resource.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn serve(&self, id: u64, method: &str, uri: &str, range: Option<&str>) -> ResourceReply {
        match self.1.serve(method, uri, range) {
            Some(reply) => reply,
            None => self.resolve(id, method, uri),
        }
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
            headers: Vec::new(),
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
pub struct ResourceReply {
    pub status: u16,
    pub mime: String,
    pub bytes: Vec<u8>,
    /// Anything else the answer needs, which is `Content-Range` and `Accept-Ranges` for a video.
    pub headers: Vec<(&'static str, String)>,
}

impl ResourceReply {
    /// A response with no body, used for misses and refused methods.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    fn empty(status: u16) -> Self {
        Self {
            status,
            mime: "text/plain; charset=utf-8".to_owned(),
            bytes: Vec::new(),
            headers: Vec::new(),
        }
    }
}

/// The project relative path an `unluminous://tab-<id>/<path>` address names, in either of the two forms
/// the engines report it in.
///
/// `None` for anything else, including the realm's own `unluminous://realm/...` pages, which are not files.
fn a_local_tab_address(value: &str) -> Option<PathBuf> {
    let url = Url::parse(&canonical(value)).ok()?;
    if url.scheme() != "unluminous" {
        return None;
    }
    url.host_str()?.strip_prefix("tab-")?.parse::<u64>().ok()?;
    let mut relative = PathBuf::new();
    for segment in url.path_segments()?.filter(|segment| !segment.is_empty()) {
        let name = percent_decode_str(segment).decode_utf8().ok()?;
        if name == ".." || name == "." {
            return None;
        }
        relative.push(name.as_ref());
    }
    (!relative.as_os_str().is_empty()).then_some(relative)
}

/// The same address under another tab's origin, so a history written in one run reads its files in the next.
///
/// Every address outside a tab's own origin is returned as it is.
fn under_tab(url: &str, id: u64) -> String {
    let url = canonical(url);
    let Some(rest) = url.strip_prefix("unluminous://tab-") else { return url };
    match rest.split_once('/') {
        Some((old, path)) if old.parse::<u64>().is_ok() => format!("unluminous://tab-{id}/{path}"),
        _ => url,
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
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
    use wry::{
        NewWindowResponse, PageLoadEvent, PermissionResponse, WebContext, WebView, WebViewBuilder,
    };

    use super::{BrowserEvent, BrowserPlacement, BrowserTab, LocalResourceStore, PageMenu};
    use crate::services::browser_session::KeptCookie;

    /// Everything needed to settle the native views for one frame.
    pub struct Settle<'a> {
        /// Every rendered tab there is. A view whose tab is not among them is dropped.
        pub tabs: &'a [BrowserTab],
        /// The pages to show this frame, in the order they were drawn. A view whose tab has no placement here
        /// is hidden, and kept.
        pub placements: &'a [&'a BrowserPlacement],
        /// The session cookies to give a view that is about to be made, before its first page. `task-2203`.
        pub cookies: &'a [KeptCookie],
        /// What each page's own right click menu offers, by tab. Read by the menu callback when a menu opens.
        pub menus: Arc<Mutex<HashMap<u64, PageMenu>>>,
        /// What egui is drawing over the pages this frame, in the window's own points.
        pub occluders: &'a [egui::Rect],
        pub profile: Option<PathBuf>,
        pub resources: LocalResourceStore,
        pub sender: std::sync::mpsc::Sender<BrowserEvent>,
        pub repaint: egui::Context,
        /// How many views may be kept, the hidden ones included. See [`super::MOST_LIVE_VIEWS`].
        pub most: usize,
        /// Whether a link a person clicks opens in a new tab, and the tabs whose links stay where they are
        /// whatever that says, which are the Designer's. `task-2270`.
        pub links_open_new_tabs: bool,
        pub links_stay: &'a [u64],
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

    /// One page's native child view, which belongs to one rendered tab for as long as it lives.
    struct NativeView {
        webview: WebView,
        bounds: Option<wry::Rect>,
        /// The last answer [`clip_to_the_visible_part`] was given, so a frame that changed none of it
        /// costs no call into the window manager. `wry::Rect` is not comparable, so this keeps egui's
        /// own. `None` is no region at all, which is a page with nothing over it inside its own pane.
        clip: Option<Clip>,
        /// The zoom [`place`] last sent, so a frame that did not move it costs no call into the engine.
        zoom: Option<f64>,
        /// The same zoom, shared with the page's right click menu so a point it reports in the view's own
        /// units can be turned into the page's pixels. Stored as the bits of an `f64`.
        zoom_bits: Arc<AtomicU64>,
        visible: bool,
        /// Whether the view was built on the editing area's ground rather than the engine's white,
        /// which a new tab's empty page is, so it does not flash white as it opens (`task-2320`). The
        /// white goes back the first time the view is sent to a page, because a page that names no
        /// background of its own is drawn on this colour.
        on_the_editor_ground: AtomicBool,
        /// Whether the page holds the **operating system's** keyboard focus. See [`super::TheFocus`].
        has_the_focus: bool,
        /// The frame this view was last placed on, so the one hidden longest goes first when there are too
        /// many.
        placed_at: u64,
        /// The address a new view goes to once WebKit has stored the session cookies it was given.
        #[cfg(target_os = "macos")]
        first_page: Mutex<Option<cookies::FirstPage>>,
    }

    /// The platform objects, kept behind this module so the rest of Unluminous stays portable and testable.
    ///
    /// **One native view a rendered tab** (`task-2270`), made the first time the tab's page is placed and kept,
    /// hidden at WebView2's low memory target, while the tab is not showing. See [`super::BrowserHost`] for
    /// what was measured.
    pub struct NativeHost {
        context: Option<WebContext>,
        views: HashMap<u64, NativeView>,
        parent: Option<Parent>,
        /// How many times the views have been settled, which is what [`NativeView::placed_at`] counts in.
        frame: u64,
        /// The `User-Agent` a view is made with, or `None` for the engine's own.
        user_agent: Option<String>,
        /// The session cookies read by [`Self::ask_for_session_cookies`], until they are taken.
        #[cfg(windows)]
        answered: Option<Vec<KeptCookie>>,
        /// The read of the session cookies WebKit answers on a later turn of the run loop.
        #[cfg(target_os = "macos")]
        reading: cookies::Reading,
    }

    impl NativeHost {
        /// A host with no browser environment until a tab is first shown.
        pub fn new() -> Self {
            Self {
                context: None,
                views: HashMap::new(),
                parent: None,
                frame: 0,
                user_agent: None,
                #[cfg(windows)]
                answered: None,
                #[cfg(target_os = "macos")]
                reading: cookies::Reading::default(),
            }
        }

        /// The `User-Agent` the next view is made with.
        pub fn set_user_agent(&mut self, agent: Option<String>) {
            self.user_agent = agent;
        }

        /// The view tab `id`'s page is in, or the sentence a command that needed one answers with.
        fn view_for(&self, id: u64) -> Result<&NativeView, String> {
            self.views.get(&id).ok_or_else(|| "The browser tab is not ready yet.".to_owned())
        }

        /// Run a script in tab `id`'s page and hand its answer, as JSON, to `answer` when the engine has it.
        pub fn evaluate_with_callback(
            &self,
            id: u64,
            script: &str,
            answer: impl Fn(String) + Send + 'static,
        ) -> Result<(), String> {
            self.view_for(id)?
                .webview
                .evaluate_script_with_callback(script, answer)
                .map_err(|problem| format!("The page could not be asked: {problem}"))
        }

        /// Ask the engine for the cookies with no expiry it holds now. The answer is
        /// [`Self::take_session_cookies`]. Every view shares one profile, so any of them answers for all.
        ///
        /// **On Windows this waits on the engine with a nested message pump**, so it is called from before
        /// the egui pass and nowhere else. See `BrowserHost::keep_the_session`.
        #[cfg(windows)]
        pub fn ask_for_session_cookies(&mut self) {
            let Some(view) = self.views.values().next() else { return };
            let Ok(cookies) = view.webview.cookies() else { return };
            self.answered = Some(session_only(
                cookies.iter().filter(|cookie| cookie.expires_datetime().is_none()).map(|cookie| {
                    KeptCookie {
                        name: cookie.name().to_owned(),
                        value: cookie.value().to_owned(),
                        domain: cookie.domain().unwrap_or_default().to_owned(),
                        path: cookie.path().unwrap_or("/").to_owned(),
                        secure: cookie.secure().unwrap_or(false),
                        http_only: cookie.http_only().unwrap_or(false),
                        same_site: cookie
                            .same_site()
                            .map(|same| same.to_string())
                            .unwrap_or_default(),
                    }
                }),
            ));
        }

        /// Ask WebKit for the cookies with no expiry it holds now. WebKit answers on a later turn of the
        /// run loop and nothing waits for it. See [`cookies`].
        #[cfg(target_os = "macos")]
        pub fn ask_for_session_cookies(&mut self) {
            if let Some(view) = self.views.values().next() {
                self.reading.ask(&view.webview);
            }
        }

        /// The cookies asked for by [`Self::ask_for_session_cookies`], once, when the engine has answered.
        pub fn take_session_cookies(&mut self) -> Option<Vec<KeptCookie>> {
            #[cfg(windows)]
            return self.answered.take();
            #[cfg(target_os = "macos")]
            return self.reading.take();
        }

        /// Whether any native view exists.
        pub fn has_view(&self) -> bool {
            !self.views.is_empty()
        }

        /// Whether tab `id` has a view of its own.
        pub fn has_view_for(&self, id: u64) -> bool {
            self.views.contains_key(&id)
        }

        /// How many views there are, the hidden ones included.
        pub fn count(&self) -> usize {
            self.views.len()
        }

        /// Whether a page holds the operating system's keyboard focus.
        ///
        /// **Asked of the platform, not of the flag.** `view.has_the_focus` records what Unluminous
        /// last *did* about the focus, and a `WebView2` takes it on its own the moment it is created
        /// — see [`the_page_really_has_the_keyboard`]. `task-2009`.
        pub fn page_holds_the_keyboard(&self) -> bool {
            self.views.values().any(page_has_it)
        }

        /// The tab whose page somebody has clicked into: it holds the operating system's keyboard and the
        /// pointer is over it, asked of the platform because a page's window takes the pointer's events and
        /// egui never hears of them.
        pub fn the_page_clicked_into(&self) -> Option<u64> {
            self.views
                .iter()
                .find(|(_, view)| view.visible && page_has_it(view) && pointer_over(view))
                .map(|(id, _)| *id)
        }

        /// Take the window's handle while the frame that has one is in scope.
        pub fn remember_window(&mut self, frame: &eframe::Frame) {
            if self.parent.is_none() {
                self.parent = frame.window_handle().ok().map(|handle| Parent(handle.as_raw()));
            }
        }

        /// Drop every view and everything it holds, which is what closing the last rendered tab means.
        ///
        /// **The focus is handed back before a view goes.** Windows moves a destroyed window's focus
        /// to its parent by itself, but it does so through `WM_KILLFOCUS`/`WM_SETFOCUS` ordering that
        /// nothing here can assert; asking `wry` is one call and it is the same call the other two
        /// routes make. See [`super::TheFocus`].
        pub fn forget(&mut self) {
            for view in self.views.values_mut() {
                give_the_focus_back(view);
            }
            self.views.clear();
        }

        /// Drop tab `id`'s view, which is what closing that tab means.
        pub fn forget_one(&mut self, id: u64) {
            if let Some(mut view) = self.views.remove(&id) {
                give_the_focus_back(&mut view);
            }
        }

        /// Take every view off the screen and lower its memory target, keeping it ready.
        pub fn hide(&mut self) {
            for view in self.views.values_mut() {
                give_the_focus_back(view);
                set_visible(view, false);
            }
        }

        /// Make a view for each placed page that has none, at most one a frame, place every placed page, and
        /// hide every view whose page is not placed. Answers what could not be done and the tabs whose view
        /// was made this frame.
        ///
        /// **One view is made a frame**, because making one waits in a nested message pump, and two waits in
        /// one frame are two chances for the window to stop drawing for the length of a page's start. A page
        /// whose view is still to be made draws its picture or its sentence for that frame.
        pub fn settle(&mut self, request: Settle<'_>) -> (Vec<(u64, String)>, Vec<u64>) {
            self.frame += 1;
            let mut problems = Vec::new();
            let mut made = Vec::new();
            self.views.retain(|id, _| request.tabs.iter().any(|tab| tab.id == *id));
            for placement in request.placements {
                if self.views.contains_key(&placement.id) || !made.is_empty() {
                    continue;
                }
                let Some(tab) = request.tabs.iter().find(|tab| tab.id == placement.id) else {
                    continue;
                };
                match self.create(tab, &request) {
                    Ok(()) => made.push(tab.id),
                    Err(problem) => problems.push((tab.id, problem)),
                }
            }
            let placed: Vec<u64> =
                request.placements.iter().map(|placement| placement.id).collect();
            for (id, view) in &mut self.views {
                if !placed.contains(id) {
                    give_the_focus_back(view);
                    set_visible(view, false);
                }
            }
            #[cfg(target_os = "macos")]
            for view in self.views.values() {
                cookies::load_the_first_page_when_due(&view.first_page, &view.webview);
            }
            // **The pages that are not focused first, and the focused one last**, because handing the keyboard
            // back from a page asks the window to take it: done after the focused page was given it, that
            // would take it straight off the page again.
            let frame = self.frame;
            for focused in [false, true] {
                for placement in request.placements.iter().filter(|one| one.focused == focused) {
                    if let Some(view) = self.views.get_mut(&placement.id) {
                        place(view, placement, request.occluders);
                        view.zoom_bits.store(view.zoom.unwrap_or(1.0).to_bits(), Ordering::Relaxed);
                        view.placed_at = frame;
                    }
                }
            }
            self.keep_the_number_of_views_down(request.most, &placed);
            (problems, made)
        }

        /// Drop the views hidden longest until there are no more than `most`. A view placed this frame is
        /// never dropped.
        fn keep_the_number_of_views_down(&mut self, most: usize, placed: &[u64]) {
            while self.views.len() > most.max(1) {
                let oldest = self
                    .views
                    .iter()
                    .filter(|(id, _)| !placed.contains(id))
                    .min_by_key(|(_, view)| view.placed_at)
                    .map(|(id, _)| *id);
                let Some(oldest) = oldest else { return };
                self.forget_one(oldest);
            }
        }

        /// Move every placed page that already has a view to where this frame drew it, at the end of the
        /// frame. See [`super::BrowserHost::follow`].
        pub fn follow(&mut self, placements: &[&BrowserPlacement], occluders: &[egui::Rect]) {
            for placement in placements {
                let Some(view) = self.views.get_mut(&placement.id) else { continue };
                if view.visible {
                    let showing = move_to(view, placement, occluders);
                    set_visible(view, showing);
                    view.zoom_bits.store(view.zoom.unwrap_or(1.0).to_bits(), Ordering::Relaxed);
                }
            }
        }

        /// Build tab `tab`'s native view, with no host bridge and callbacks that report its browser state.
        ///
        /// Every callback names the tab the view belongs to, because a view never moves to another tab.
        fn create(&mut self, tab: &BrowserTab, request: &Settle<'_>) -> Result<(), String> {
            let Some(parent) = &self.parent else {
                return Err("Unluminous has not finished opening its window yet.".to_owned());
            };
            let id = tab.id;
            let context =
                self.context.get_or_insert_with(|| WebContext::new(request.profile.clone()));
            let repaint = request.repaint.clone();
            let title_sender = request.sender.clone();
            let load_sender = request.sender.clone();
            let load_repaint = request.repaint.clone();
            let popup_sender = request.sender.clone();
            let popup_repaint = request.repaint.clone();
            let zoom_sender = request.sender.clone();
            let zoom_repaint = request.repaint.clone();
            let resources = request.resources.clone();
            let zoom_bits = Arc::new(AtomicU64::new(1f64.to_bits()));
            // **Built with no address**, so the session cookies can be given to it before its first request
            // goes. Built with the address, the request starts inside `build` and the page arrives signed
            // out. `task-2203`.
            let first = super::engine_url(tab.current_url());
            let mut builder = WebViewBuilder::new_with_web_context(context);
            let blank = tab.current_url() == super::BLANK;
            if blank {
                builder = builder.with_background_color(editor_ground());
            }
            if let Some(agent) = &self.user_agent {
                builder = builder.with_user_agent(agent);
            }
            // **A link a person clicks opens a new tab** (`task-2270`), unless the setting says otherwise or the
            // page is a design. See [`super::LINKS_IN_NEW_TABS`].
            if request.links_open_new_tabs && !request.links_stay.contains(&id) {
                builder = builder.with_initialization_script(super::LINKS_IN_NEW_TABS);
            }
            builder = builder.with_initialization_script(super::ZOOM_GESTURES);
            let webview = builder
                // **Each view its own id**, because wry's macOS half keeps a view's state in a map keyed by it,
                // and two views with one id overwrite each other's custom protocol. `task-2270`.
                .with_id(&format!("unluminous-browser-{id}"))
                .with_visible(false)
                .with_clipboard(true)
                // `Ctrl`/`Cmd` with the wheel or with plus and minus zooms the page while the page has the
                // keyboard, as in any browser. `wry` turns the engine's own zoom keys off unless asked. `task-2255`.
                .with_hotkeys_zoom(true)
                .with_background_throttling(wry::BackgroundThrottlingPolicy::Throttle)
                .with_permission_handler(|_| PermissionResponse::Deny)
                .with_download_started_handler(|_, _| false)
                .with_navigation_handler(allowed_navigation)
                // The page's modifier wheel and zoom keys, which [`super::ZOOM_GESTURES`] posts. `task-2336`.
                .with_ipc_handler(move |message| {
                    if let Some(steps) = super::zoom_message(message.body()) {
                        let _ = zoom_sender.send(BrowserEvent::ZoomAsked { id, steps });
                        zoom_repaint.request_repaint();
                    }
                })
                .with_new_window_req_handler(move |url, _| {
                    let _ = popup_sender.send(BrowserEvent::OpenRequested { source: id, url });
                    popup_repaint.request_repaint();
                    NewWindowResponse::Deny
                })
                .with_document_title_changed_handler(move |title| {
                    let _ = title_sender.send(BrowserEvent::Title { id, title });
                    repaint.request_repaint();
                })
                .with_on_page_load_handler(move |event, url| {
                    let event = match event {
                        PageLoadEvent::Started => BrowserEvent::LoadStarted { id, url },
                        PageLoadEvent::Finished => BrowserEvent::LoadFinished { id, url },
                    };
                    let _ = load_sender.send(event);
                    load_repaint.request_repaint();
                })
                .with_custom_protocol("unluminous".to_owned(), move |_, request| {
                    protocol_response(&resources, id, request)
                })
                .build_as_child(parent)
                .map_err(|problem| format!("Unluminous could not start the browser: {problem}"))?;
            #[cfg(windows)]
            for cookie in request.cookies {
                let _ = webview.set_cookie(&engine_cookie(cookie));
            }
            #[cfg(windows)]
            context_menu::offer(
                &webview,
                request.menus.clone(),
                id,
                zoom_bits.clone(),
                request.sender.clone(),
                request.repaint.clone(),
            );
            // macOS has no host hook into WKWebView's own menu that `wry` exposes; the toolbar's button is the
            // way in there. See the design, §3.2.
            #[cfg(not(windows))]
            let _ = &request.menus;
            #[cfg(windows)]
            let _ = webview.load_url(&first);
            // **On macOS the first page waits for WebKit to store the cookies**, which it says on a later turn
            // of the run loop. `settle` sends it when every cookie is stored, or after
            // [`cookies::FIRST_PAGE_WAITS`] if WebKit never says.
            #[cfg(target_os = "macos")]
            let first_page = {
                self.reading.repaint = Some(request.repaint.clone());
                let page = cookies::give(&webview, request.cookies, first, request.repaint.clone());
                Mutex::new(Some(page))
            };
            self.views.insert(
                id,
                NativeView {
                    webview,
                    bounds: None,
                    clip: None,
                    zoom: None,
                    zoom_bits,
                    visible: false,
                    on_the_editor_ground: AtomicBool::new(blank),
                    has_the_focus: false,
                    placed_at: self.frame,
                    #[cfg(target_os = "macos")]
                    first_page,
                },
            );
            Ok(())
        }

        /// Send tab `id`'s view to an address, which is what that tab's own history asked for.
        pub fn navigate(&self, id: u64, url: &str) -> Result<(), String> {
            let view = self.view_for(id)?;
            #[cfg(target_os = "macos")]
            cookies::replace(&view.first_page, None);
            if url != super::BLANK && view.on_the_editor_ground.swap(false, Ordering::Relaxed) {
                let _ = view.webview.set_background_color((255, 255, 255, 255));
            }
            view.webview
                .load_url(&super::engine_url(url))
                .map_err(|problem| format!("The browser could not navigate: {problem}"))
        }

        /// Run a script in tab `id`'s page.
        pub fn evaluate(&self, id: u64, script: &str) -> Result<(), String> {
            self.view_for(id)?
                .webview
                .evaluate_script(script)
                .map_err(|problem| format!("The page could not be asked: {problem}"))
        }

        /// Reload tab `id`'s page.
        pub fn reload(&self, id: u64) -> Result<(), String> {
            self.view_for(id)?
                .webview
                .reload()
                .map_err(|problem| format!("The browser could not reload: {problem}"))
        }

        /// How big tab `id`'s page is drawn.
        ///
        /// **The engine's number rather than Unluminous's drawing.** `task-1905` asks that the modifier
        /// wheel over a node zoom that node, and for a browser node the thing that decides how big a page
        /// is is the page's own zoom — nothing Unluminous paints is involved.
        pub fn zoom(&self, id: u64, factor: f64) -> Result<(), String> {
            self.view_for(id)?
                .webview
                .zoom(factor)
                .map_err(|problem| format!("The browser could not be zoomed: {problem}"))
        }

        /// A picture of tab `id`'s page as it is on the screen now. See [`super::BrowserHost::photograph`].
        pub fn photograph(&self, id: u64, plain: bool) -> Option<egui::ColorImage> {
            photograph(self.views.get(&id)?, plain)
        }
    }

    /// **On Windows it is `PrintWindow` on the container `wry` builds the engine inside**, with
    /// `PW_RENDERFULLCONTENT`, which is the flag for content a compositor draws. Measured on `task-2253`
    /// against a 640 by 352 point page: the picture held the page, and it took 11 to 18 ms. It is asked once,
    /// on the frame a zoom starts. A picture that came back all one colour is a capture that did not work,
    /// and is answered `None` so the live page is used instead, unless `plain` says one colour is a
    /// picture too.
    #[cfg(windows)]
    fn photograph(view: &NativeView, plain: bool) -> Option<egui::ColorImage> {
        use windows_sys::Win32::Foundation::{HWND, RECT};
        use windows_sys::Win32::Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
            ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use windows_sys::Win32::Storage::Xps::PrintWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, PW_RENDERFULLCONTENT};
        use wry::WebViewExtWindows as _;

        let hwnd = view.webview.hwnd().0 as HWND;
        if hwnd.is_null() {
            return None;
        }
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: the handle is the live container of the one view this host owns.
        if unsafe { GetClientRect(hwnd, &mut rect) } == 0 {
            return None;
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width < 2 || height < 2 || width > 16_384 || height > 16_384 {
            return None;
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: every handle made here is released before returning, and `GetDIBits` writes at most
        // `height` rows of `width` 32-bit pixels, which is the buffer's size.
        let read = unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let old = SelectObject(memory, bitmap as _);
            let printed = PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT) != 0;
            SelectObject(memory, old);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative, so the rows come top first, which is the order egui reads them in.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let rows = GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            DeleteObject(bitmap as _);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            printed && rows == height
        };
        if !read {
            return None;
        }
        // GDI writes blue, green, red and a byte that means nothing; egui wants red, green, blue, alpha.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        let first = [pixels[0], pixels[1], pixels[2]];
        if !plain && pixels.chunks_exact(4).all(|pixel| pixel[..3] == first) {
            return None;
        }
        Some(egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &pixels))
    }

    /// **On macOS there is no picture yet**, because `WKWebView` takes one only asynchronously, and the
    /// canvas then uses the live page during a zoom as it always did.
    #[cfg(target_os = "macos")]
    fn photograph(_view: &NativeView, _plain: bool) -> Option<egui::ColorImage> {
        None
    }

    /// A kept cookie in the form the engine takes, with no expiry, which is what makes it a session cookie.
    #[cfg(windows)]
    fn engine_cookie(kept: &KeptCookie) -> wry::cookie::Cookie<'static> {
        let mut cookie = wry::cookie::Cookie::new(kept.name.clone(), kept.value.clone());
        cookie.set_domain(kept.domain.clone());
        cookie.set_path(kept.path.clone());
        cookie.set_secure(kept.secure);
        cookie.set_http_only(kept.http_only);
        let same_site = match kept.same_site.to_ascii_lowercase().as_str() {
            "strict" => Some(wry::cookie::SameSite::Strict),
            "lax" => Some(wry::cookie::SameSite::Lax),
            "none" => Some(wry::cookie::SameSite::None),
            _ => None,
        };
        cookie.set_same_site(same_site);
        cookie
    }

    /// The session cookies worth keeping, in the order the file is written in, so two reads of the same
    /// cookies compare equal.
    fn session_only(cookies: impl Iterator<Item = KeptCookie>) -> Vec<KeptCookie> {
        let mut kept: Vec<KeptCookie> =
            cookies.filter(|cookie| !cookie.name.is_empty() && !cookie.domain.is_empty()).collect();
        kept.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        kept
    }

    /// The session cookies on macOS, given to WebKit's cookie store and read back from it without waiting.
    ///
    /// **`wry`'s `set_cookie` and `cookies` must not be called on macOS.** Each one waits up to a second for
    /// WebKit's completion block by running the main run loop by hand (`wait_for_blocking_operation`). They
    /// are called from `BrowserHost::reconcile`, which runs inside `winit`'s handling of a redraw, and a
    /// redraw that arrives while the run loop is being run by hand reaches `winit` a second time. `winit`
    /// answers that with "tried to handle event while another event is currently being handled", inside an
    /// Objective C block where a panic cannot unwind, so the process aborts. That is the crash 0.84.4 had on
    /// every start with fifteen cookies in `browser/session-cookies`, and 0.78.0 once before it.
    ///
    /// So the store is reached directly, the completion blocks only count and ask for a frame, and nothing
    /// here runs the run loop. A completion block must never panic, for the same reason.
    #[cfg(target_os = "macos")]
    mod cookies {
        use std::ptr::NonNull;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::{Duration, Instant};

        use block2::RcBlock;
        use objc2::rc::Retained;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{
            ns_string, NSArray, NSHTTPCookie, NSHTTPCookieDomain, NSHTTPCookieName,
            NSHTTPCookiePath, NSHTTPCookiePropertyKey, NSHTTPCookieSecure, NSHTTPCookieValue,
            NSMutableDictionary, NSString,
        };
        use objc2_web_kit::WKHTTPCookieStore;
        use wry::{WebView, WebViewExtMacOS as _};

        use crate::services::browser_session::KeptCookie;

        /// How long a new view's first page waits for WebKit to say the cookies are stored. Storing fifteen
        /// takes a few milliseconds; a WebKit that never answers costs a page that loads signed out.
        pub const FIRST_PAGE_WAITS: Duration = Duration::from_secs(2);

        /// The address a new view goes to, and how many of its cookies WebKit has not yet stored.
        pub struct FirstPage {
            url: String,
            left: Arc<AtomicUsize>,
            since: Instant,
        }

        /// Set or clear the address a new view is waiting to go to.
        pub fn replace(slot: &Mutex<Option<FirstPage>>, page: Option<FirstPage>) {
            if let Ok(mut slot) = slot.lock() {
                *slot = page;
            }
        }

        /// Send the view to its first page once every cookie is stored, or once it has waited long enough.
        pub fn load_the_first_page_when_due(slot: &Mutex<Option<FirstPage>>, webview: &WebView) {
            let Ok(mut slot) = slot.lock() else { return };
            let due = slot.as_ref().is_some_and(|page| {
                page.left.load(Ordering::Acquire) == 0 || page.since.elapsed() >= FIRST_PAGE_WAITS
            });
            if due {
                if let Some(page) = slot.take() {
                    let _ = webview.load_url(&page.url);
                }
            }
        }

        /// The cookie store of the data store the view was built with, which is the one `wry` would use.
        fn store(webview: &WebView) -> Retained<WKHTTPCookieStore> {
            let view = webview.webview();
            // SAFETY: called on the main thread, where the view was built and is used.
            unsafe { view.configuration().websiteDataStore().httpCookieStore() }
        }

        /// Give WebKit every kept cookie, and answer the address to go to once they are stored.
        pub fn give(
            webview: &WebView,
            cookies: &[KeptCookie],
            url: String,
            repaint: egui::Context,
        ) -> FirstPage {
            let left = Arc::new(AtomicUsize::new(cookies.len()));
            let store = store(webview);
            for kept in cookies {
                let Some(cookie) = engine_cookie(kept) else {
                    left.fetch_sub(1, Ordering::AcqRel);
                    continue;
                };
                let counted = left.clone();
                let repaint = repaint.clone();
                let stored = RcBlock::new(move || {
                    counted.fetch_sub(1, Ordering::AcqRel);
                    repaint.request_repaint();
                });
                // SAFETY: the cookie and the block are retained by the call for as long as WebKit needs them.
                unsafe { store.setCookie_completionHandler(&cookie, Some(&stored)) };
            }
            FirstPage { url, left, since: Instant::now() }
        }

        /// A read of the cookies WebKit holds, asked for on one frame and answered on a later one.
        #[derive(Default)]
        pub struct Reading {
            asking: Arc<AtomicBool>,
            answer: Arc<Mutex<Option<Vec<KeptCookie>>>>,
            /// Asked for a frame when the answer arrives, so it is written down without waiting for one.
            pub repaint: Option<egui::Context>,
        }

        impl Reading {
            /// Ask for the cookies, unless a read is still unanswered.
            pub fn ask(&self, webview: &WebView) {
                if self.asking.swap(true, Ordering::AcqRel) {
                    return;
                }
                let asking = self.asking.clone();
                let answer = self.answer.clone();
                let repaint = self.repaint.clone();
                let answered = RcBlock::new(move |cookies: NonNull<NSArray<NSHTTPCookie>>| {
                    // SAFETY: WebKit hands the block a valid array for the length of the call.
                    let cookies = unsafe { cookies.as_ref() };
                    let kept = super::session_only(
                        cookies.to_vec().iter().filter_map(|cookie| kept_cookie(cookie)),
                    );
                    if let Ok(mut slot) = answer.lock() {
                        *slot = Some(kept);
                    }
                    asking.store(false, Ordering::Release);
                    if let Some(repaint) = &repaint {
                        repaint.request_repaint();
                    }
                });
                // SAFETY: called on the main thread; the block is retained by the call.
                unsafe { store(webview).getAllCookies(&answered) };
            }

            /// The answer to the last read, once.
            pub fn take(&self) -> Option<Vec<KeptCookie>> {
                self.answer.lock().ok()?.take()
            }
        }

        /// A cookie WebKit holds, if it is a session cookie, which is one with no expiry.
        fn kept_cookie(cookie: &NSHTTPCookie) -> Option<KeptCookie> {
            if cookie.expiresDate().is_some() {
                return None;
            }
            // Compared as text, as `wry` does, because of https://github.com/tauri-apps/wry/issues/1616.
            let policy =
                cookie.sameSitePolicy().map(|policy| policy.to_string().to_ascii_lowercase());
            let same_site = match policy.as_deref() {
                Some("lax") => "Lax",
                Some("strict") => "Strict",
                _ => "None",
            };
            Some(KeptCookie {
                name: cookie.name().to_string(),
                value: cookie.value().to_string(),
                domain: cookie.domain().to_string(),
                path: cookie.path().to_string(),
                secure: cookie.isSecure(),
                http_only: cookie.isHTTPOnly(),
                same_site: same_site.to_owned(),
            })
        }

        /// A kept cookie as WebKit takes it, with no expiry, which is what makes it a session cookie. These
        /// are the properties `wry`'s own conversion sets.
        fn engine_cookie(kept: &KeptCookie) -> Option<Retained<NSHTTPCookie>> {
            let name = NSString::from_str(&kept.name);
            let value = NSString::from_str(&kept.value);
            let domain = NSString::from_str(&kept.domain);
            let path = NSString::from_str(&kept.path);
            // SAFETY: the keys are the constant strings Foundation defines for a cookie's properties, and
            // every value is a string, which is what `cookieWithProperties` reads each of them as.
            unsafe {
                let properties: Retained<NSMutableDictionary<NSHTTPCookiePropertyKey, AnyObject>> =
                    NSMutableDictionary::from_slices(
                        &[
                            NSHTTPCookieName,
                            NSHTTPCookieValue,
                            NSHTTPCookieDomain,
                            NSHTTPCookiePath,
                        ],
                        &[&*name, &*value, &*domain, &*path],
                    );
                if kept.secure {
                    properties.insert(NSHTTPCookieSecure, ns_string!("TRUE"));
                }
                if kept.http_only {
                    properties.insert(ns_string!("HttpOnly"), ns_string!("TRUE"));
                }
                match kept.same_site.to_ascii_lowercase().as_str() {
                    "lax" => properties.insert(ns_string!("SameSite"), ns_string!("lax")),
                    "strict" => properties.insert(ns_string!("SameSite"), ns_string!("strict")),
                    _ => {}
                }
                NSHTTPCookie::cookieWithProperties(&properties)
            }
        }
    }

    /// `Select Element` and `Show Whole Page` on WebView2's own right click menu. `task-2203`.
    ///
    /// `wry` does not wrap `ContextMenuRequested`, so the engine is reached through the `ICoreWebView2` `wry`
    /// hands out. The items are added to the **default** menu, at the top with a separator under them, and
    /// only while the page showing is a browser node: the event's `Handled` is left alone so the rest of the
    /// menu is the engine's own. A runtime older than `ICoreWebView2_11` has no such event, and the toolbar's
    /// button is then the way in.
    #[cfg(windows)]
    mod context_menu {
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::{Arc, Mutex};

        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2ContextMenuItem, ICoreWebView2Environment9, ICoreWebView2_11,
            ICoreWebView2_2, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
        };
        use webview2_com::{ContextMenuRequestedEventHandler, CustomItemSelectedEventHandler};
        use windows::Win32::Foundation::POINT;
        use windows::Win32::System::Com::IStream;
        use windows_core::{Interface, HSTRING};
        use wry::WebViewExtWindows;

        use super::super::{BrowserEvent, PageMenu};

        /// Register the menu callback on a view that has just been made.
        pub fn offer(
            webview: &wry::WebView,
            menus: Arc<Mutex<HashMap<u64, PageMenu>>>,
            id: u64,
            zoom: Arc<AtomicU64>,
            sender: std::sync::mpsc::Sender<BrowserEvent>,
            repaint: egui::Context,
        ) {
            let Ok(engine) = webview.webview().cast::<ICoreWebView2_11>() else { return };
            let handler =
                ContextMenuRequestedEventHandler::create(Box::new(move |sender_view, args| {
                    let (Some(view), Some(args)) = (sender_view, args) else { return Ok(()) };
                    let offered = menus
                        .lock()
                        .ok()
                        .and_then(|held| held.get(&id).copied())
                        .unwrap_or_default();
                    if !offered.pickable {
                        return Ok(());
                    }
                    let mut at = POINT::default();
                    // Safety: every call here is on the engine's own thread, inside its own callback.
                    unsafe {
                        let _ = args.Location(&mut at);
                        let environment = view
                            .cast::<ICoreWebView2_2>()?
                            .Environment()?
                            .cast::<ICoreWebView2Environment9>()?;
                        let items = args.MenuItems()?;
                        let scale = f64::from_bits(zoom.load(Ordering::Relaxed)).max(0.05);
                        let point = (f64::from(at.x) / scale, f64::from(at.y) / scale);
                        let separator =
                            item(&environment, "", COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR)?;
                        items.InsertValueAtIndex(0, &separator)?;
                        if offered.pinned {
                            let whole = item(
                                &environment,
                                "Show Whole Page",
                                COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                            )?;
                            let (events, again) = (sender.clone(), repaint.clone());
                            on_choice(&whole, move || {
                                let _ = events.send(BrowserEvent::UnpinAsked { id });
                                again.request_repaint();
                            })?;
                            items.InsertValueAtIndex(0, &whole)?;
                        }
                        let select = item(
                            &environment,
                            "Select Element",
                            COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
                        )?;
                        let (events, again) = (sender.clone(), repaint.clone());
                        on_choice(&select, move || {
                            let _ = events.send(BrowserEvent::PickAsked { id, at: Some(point) });
                            again.request_repaint();
                        })?;
                        items.InsertValueAtIndex(0, &select)?;
                    }
                    Ok(())
                }));
            let mut token = 0i64;
            // Safety: the handler is a COM object the engine holds a reference to for as long as it needs.
            let _ = unsafe { engine.add_ContextMenuRequested(&handler, &mut token) };
        }

        /// One item for the menu.
        unsafe fn item(
            environment: &ICoreWebView2Environment9,
            label: &str,
            kind: COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND,
        ) -> windows_core::Result<ICoreWebView2ContextMenuItem> {
            let label = HSTRING::from(label);
            // Safety: the label outlives the call, and no icon is given.
            unsafe { environment.CreateContextMenuItem(&label, None::<&IStream>, kind) }
        }

        /// What choosing an item does.
        unsafe fn on_choice(
            item: &ICoreWebView2ContextMenuItem,
            chosen: impl Fn() + 'static,
        ) -> windows_core::Result<()> {
            let handler = CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                chosen();
                Ok(())
            }));
            let mut token = 0i64;
            // Safety: as above.
            unsafe { item.add_CustomItemSelected(&handler, &mut token) }
        }
    }

    /// The editing area's ground, opaque, in the form the engine takes a background colour in.
    fn editor_ground() -> wry::RGBA {
        let ground = crate::theme::color::editor().to_opaque();
        (ground.r(), ground.g(), ground.b(), 255)
    }

    /// Allow ordinary web navigation and Unluminous's one local resource origin.
    fn allowed_navigation(url: String) -> bool {
        // The one page of the `about` scheme that is allowed is the empty one a new tab starts on (`task-2270`).
        url == super::BLANK
            || Url::parse(&url)
                .ok()
                .is_some_and(|url| matches!(url.scheme(), "http" | "https" | "unluminous"))
    }

    /// Convert a constrained resource reply into the response Wry expects.
    fn protocol_response(
        resources: &LocalResourceStore,
        tab: u64,
        request: wry::http::Request<Vec<u8>>,
    ) -> wry::http::Response<Cow<'static, [u8]>> {
        let range = request.headers().get("range").and_then(|value| value.to_str().ok());
        let reply =
            resources.serve(tab, request.method().as_str(), &request.uri().to_string(), range);
        let mut response = wry::http::Response::builder()
            .status(reply.status)
            .header("Content-Type", reply.mime)
            .header("Cache-Control", "no-store");
        for (name, value) in &reply.headers {
            response = response.header(*name, value);
        }
        response.body(Cow::Owned(reply.bytes)).expect("resource response")
    }

    /// Keep the view inside its pane and change native state only when the answer moved.
    ///
    /// **The bounds are the whole page and the crop is a separate question.** `set_bounds` is the page's
    /// viewport as well as its position, so cutting it is what reflows a responsive page — see
    /// [`clip_to_the_visible_part`], which takes the part that may be painted off the same placement.
    fn place(view: &mut NativeView, placement: &BrowserPlacement, occluders: &[egui::Rect]) {
        let showing = move_to(view, placement, occluders);
        set_visible(view, showing);
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

    /// Put the view where a placement says, at its zoom and cut to what may be painted. Nothing else: not
    /// whether it is showing and not where the keyboard is, so it is safe at the end of a frame as well as
    /// before one. See [`NativeHost::follow`].
    ///
    /// Answers whether the view should be on the screen. **On macOS a page with nothing left to show is hidden
    /// rather than masked to nothing** (`task-2308`). The mask is on the view's layer, and AppKit sends the
    /// pointer and the keyboard to a view by its frame, never by its layer's mask, so a page cut away under the
    /// Settings dialog still took the wheel over its rectangle and took the keyboard from a field it was
    /// under: *"some sections of the screen don't have wheel scroll detection working"* and *"I'm unable to
    /// type in the Find field"*. A window region on Windows cuts the hits with the picture, so there the view
    /// stays as it was.
    fn move_to(
        view: &mut NativeView,
        placement: &BrowserPlacement,
        occluders: &[egui::Rect],
    ) -> bool {
        // **In the window's own logical points, which egui's are not once the interface is zoomed**
        // (`task-2280`): `wry` and the window region both measure from the system's scale alone, so every
        // rectangle egui reports is multiplied by the zoom before it reaches them.
        let logical = super::to_logical_points;
        let (area, visible) = (logical(placement.area), logical(placement.visible));
        let bounds = browser_rect(area);
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
        //
        // **And so are the canvas nodes stacked in front of it**, which egui's layers cannot report because
        // every node is drawn into a background layer. They are not grown by the popup shadow: a node's own
        // edge is where the node in front of the page begins. `task-2207`.
        let over: Vec<egui::Rect> = occluders
            .iter()
            .map(|rect| rect.expand(super::OCCLUDER_SHADOW))
            .chain(placement.over.iter().copied())
            .map(logical)
            .filter(|rect| rect.intersect(visible).is_positive())
            .collect();
        let wanted = match visible.contains_rect(area) && over.is_empty() {
            true => None,
            false => Some(Clip { area, visible, over: over.clone() }),
        };
        if view.clip != wanted {
            clip_to_the_visible_part(view, area, visible, &over);
            view.clip = wanted;
        }
        !cfg!(target_os = "macos") || !super::pieces_of(visible.intersect(area), &over).is_empty()
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

    /// Whether the pointer is inside the container `wry` builds the engine in, in screen pixels.
    #[cfg(windows)]
    fn pointer_over(view: &NativeView) -> bool {
        use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
        use wry::WebViewExtWindows as _;

        let hwnd = view.webview.hwnd().0 as HWND;
        if hwnd.is_null() {
            return false;
        }
        let mut at = POINT { x: 0, y: 0 };
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: both take a pointer to a value on this stack, and the handle is a window in this process.
        unsafe {
            if GetCursorPos(&mut at) == 0 || GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
        }
        at.x >= rect.left && at.x < rect.right && at.y >= rect.top && at.y < rect.bottom
    }

    /// Nothing to ask on the other platforms, where the page's focus is not read back.
    #[cfg(not(windows))]
    fn pointer_over(_view: &NativeView) -> bool {
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
    /// macOS has no window region, and is cropped with a layer mask instead. See its half below.
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

    /// **On macOS this is a mask on the web view's own layer**, since `task-2207`. There is no window region
    /// on macOS and no container of `wry`'s to set one on, so until then a page was never cropped there at
    /// all: it drew over the realm's bar, over a panel beside the canvas and over every node in front of it,
    /// which is the report. A layer mask clips what the layer and its sublayers draw to the opaque part of
    /// the mask, and the bounds are left alone, so the page's viewport is still the whole node.
    ///
    /// The mask is one sublayer for each rectangle [`super::pieces_of`] answers, which do not overlap, so
    /// two things over the page that overlap each other cannot cancel out. Its rectangles are measured in
    /// the view's own coordinates, which go down the screen when the view is flipped and up it otherwise,
    /// and the view is asked which rather than assumed.
    #[cfg(target_os = "macos")]
    fn clip_to_the_visible_part(
        view: &NativeView,
        area: egui::Rect,
        visible: egui::Rect,
        over: &[egui::Rect],
    ) {
        use objc2_core_foundation::{CGPoint, CGRect, CGSize};
        use objc2_core_graphics::CGColor;
        use objc2_quartz_core::CALayer;
        use wry::WebViewExtMacOS as _;

        let webview = view.webview.webview();
        if visible.contains_rect(area) && over.is_empty() {
            if let Some(layer) = webview.layer() {
                // SAFETY: taking a layer's mask away is always allowed.
                unsafe { layer.setMask(None) };
            }
            return;
        }
        if !webview.wantsLayer() {
            webview.setWantsLayer(true);
        }
        let Some(layer) = webview.layer() else {
            return;
        };
        let flipped = webview.isFlipped();
        let mask = CALayer::new();
        mask.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(f64::from(area.width()), f64::from(area.height())),
        ));
        let opaque = CGColor::new_generic_gray(0.0, 1.0);
        for piece in super::pieces_of(visible.intersect(area), over) {
            let x = f64::from(piece.left() - area.left());
            let y = match flipped {
                true => f64::from(piece.top() - area.top()),
                false => f64::from(area.bottom() - piece.bottom()),
            };
            let part = CALayer::new();
            part.setFrame(CGRect::new(
                CGPoint::new(x, y),
                CGSize::new(f64::from(piece.width()), f64::from(piece.height())),
            ));
            part.setBackgroundColor(Some(&opaque));
            mask.addSublayer(&part);
        }
        // SAFETY: the mask is a fresh layer that belongs to nothing else, which is what `setMask` asks.
        unsafe { layer.setMask(Some(&mask)) };
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use super::{BrowserEvent, BrowserPlacement, BrowserTab, LocalResourceStore, PageMenu};
    use crate::services::browser_session::KeptCookie;

    /// The same input on a platform without an embedded engine.
    pub struct Settle<'a> {
        pub tabs: &'a [BrowserTab],
        pub placements: &'a [&'a BrowserPlacement],
        pub cookies: &'a [KeptCookie],
        pub menus: Arc<Mutex<HashMap<u64, PageMenu>>>,
        /// What egui is drawing over the pages this frame, in the window's own points.
        pub occluders: &'a [egui::Rect],
        pub profile: Option<std::path::PathBuf>,
        pub resources: LocalResourceStore,
        pub sender: std::sync::mpsc::Sender<BrowserEvent>,
        pub repaint: egui::Context,
        pub most: usize,
        pub links_open_new_tabs: bool,
        pub links_stay: &'a [u64],
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
        /// For any tab.
        pub fn has_view_for(&self, _id: u64) -> bool {
            false
        }
        /// So it has none to count.
        pub fn count(&self) -> usize {
            0
        }
        /// And so never holds the keyboard.
        pub fn page_holds_the_keyboard(&self) -> bool {
            false
        }
        /// And has no page for the pointer to be over.
        pub fn the_page_clicked_into(&self) -> Option<u64> {
            None
        }
        /// There is no view to give a `User-Agent` to.
        pub fn set_user_agent(&mut self, _agent: Option<String>) {}
        /// There is no child window to create, so the window's handle is not wanted.
        pub fn remember_window(&mut self, _frame: &eframe::Frame) {}
        /// There is nothing to drop.
        pub fn forget(&mut self) {}
        /// Nor one view of it.
        pub fn forget_one(&mut self, _id: u64) {}
        /// There is nothing to hide.
        pub fn hide(&mut self) {}
        /// Nothing to move where there is no view.
        pub fn follow(&mut self, _placements: &[&BrowserPlacement], _occluders: &[egui::Rect]) {}
        /// Report one clear platform refusal for each page that would have been shown.
        pub fn settle(&mut self, request: Settle<'_>) -> (Vec<(u64, String)>, Vec<u64>) {
            let _ = (
                request.tabs,
                request.cookies,
                request.menus,
                request.occluders,
                request.profile,
                request.resources,
                request.sender,
                request.repaint,
                request.most,
                request.links_open_new_tabs,
                request.links_stay,
            );
            let problems = request
                .placements
                .iter()
                .map(|placement| (placement.id, UNSUPPORTED.to_owned()))
                .collect();
            (problems, Vec::new())
        }
        /// Refuse navigation where there is no embedded engine.
        pub fn navigate(&self, _id: u64, _url: &str) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// Refuse reloading for the same reason.
        pub fn reload(&self, _id: u64) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// And a script.
        pub fn evaluate(&self, _id: u64, _script: &str) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// Refuse a script with an answer for the same reason.
        pub fn evaluate_with_callback(
            &self,
            _id: u64,
            _script: &str,
            _answer: impl Fn(String) + Send + 'static,
        ) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// There is no engine to hold any cookies.
        pub fn ask_for_session_cookies(&mut self) {}
        /// And so no answer to take.
        pub fn take_session_cookies(&mut self) -> Option<Vec<KeptCookie>> {
            None
        }
        /// And zooming, which is the engine's own number.
        pub fn zoom(&self, _id: u64, _factor: f64) -> Result<(), String> {
            Err(UNSUPPORTED.to_owned())
        }
        /// There is no page to take a picture of.
        pub fn photograph(&self, _id: u64, _plain: bool) -> Option<egui::ColorImage> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2336`: a page can post exactly three words to the host, and everything else it posts is ignored.
    #[test]
    fn a_page_can_ask_for_a_zoom_and_for_nothing_else() {
        assert_eq!(zoom_message("unluminous-zoom:in"), Some(1));
        assert_eq!(zoom_message("unluminous-zoom:out"), Some(-1));
        assert_eq!(zoom_message("unluminous-zoom:reset"), Some(0));
        for other in
            ["", "unluminous-zoom:", "unluminous-zoom:in ", "open file:///c:/", "{\"zoom\":1}"]
        {
            assert_eq!(zoom_message(other), None, "{other:?}");
        }
        // The script posts those words and nothing else, so the two halves cannot drift apart unnoticed.
        for word in ["in", "out", "reset"] {
            assert!(ZOOM_GESTURES.contains(&format!("'{word}'")), "the script posts {word}");
        }
        assert!(ZOOM_GESTURES.contains("'unluminous-zoom:' + which"));
    }

    /// `task-2203`: a node left on a local page wrote `unluminous://tab-3/page.html`, and the next run refused
    /// the scheme, so the node came back with no page. Both of the forms an engine reports are read as the
    /// file, and a path that climbs out of the origin stays inside it.
    /// `task-2254`: the Designer keeps a script in a page and asks it what happened. The host keeps one watch, for
    /// one tab; setting what it already has changes nothing, a new one is put in the page at the next chance, and
    /// a page that finishes loading is a new document the script has to go into again.
    #[test]
    fn a_watch_is_kept_for_one_tab_and_put_in_again_when_its_page_loads() {
        let mut host = BrowserHost::new();
        let watch = Watch { install: "install();".to_owned(), take: "take()".to_owned() };
        assert_eq!(host.watching(), None);
        host.watch(3, Some(watch.clone()));
        assert_eq!(host.watching(), Some(3));
        assert!(host.install_due, "a watch that was just set has not been put in the page");
        host.install_due = false;
        host.watch(3, Some(watch.clone()));
        assert!(!host.install_due, "the same watch said again changes nothing");
        host.watch(3, Some(Watch { install: "another();".to_owned(), take: "take()".to_owned() }));
        assert!(host.install_due, "a different script is put in");
        host.install_due = false;

        // Another page loading says nothing about this one, and this one loading starts it again.
        host.sender.send(BrowserEvent::LoadFinished { id: 9, url: "u".to_owned() }).unwrap();
        host.take_events();
        assert!(!host.install_due);
        host.sender.send(BrowserEvent::LoadFinished { id: 3, url: "u".to_owned() }).unwrap();
        host.take_events();
        assert!(host.install_due, "a new document needs the script again");

        // Taking it away for another tab does nothing; for its own tab, or by closing the tab, it goes.
        host.watch(4, None);
        assert_eq!(host.watching(), Some(3));
        host.close_tab(3);
        assert_eq!(host.watching(), None);
    }

    #[test]
    fn an_address_a_local_tab_wrote_down_is_read_as_its_file() {
        let project =
            std::env::temp_dir().join(format!("unluminous-old-tab-{}", std::process::id()));
        std::fs::create_dir_all(project.join("site")).expect("folder");
        std::fs::write(project.join("site").join("my page.html"), "<p>hi</p>").expect("page");
        for written in [
            "unluminous://tab-3/site/my%20page.html",
            "http://unluminous.tab-17/site/my%20page.html",
        ] {
            let location = BrowserLocation::parse(written, &project).expect("read as a file");
            let path = location.source_path().expect("a local page");
            assert!(path.ends_with(Path::new("site").join("my page.html")), "{written}: {path:?}");
        }
        // An address cannot climb out of its origin: the parser resolves `..` against the root first.
        assert_eq!(
            a_local_tab_address("unluminous://tab-3/../secret.html"),
            Some(PathBuf::from("secret.html"))
        );
        assert!(a_local_tab_address("unluminous://realm/video/12").is_none());
        let _ = std::fs::remove_dir_all(&project);
    }

    /// `task-2203`: the picker's answer, as the engine hands it back, is read into the event it means.
    #[test]
    fn the_pickers_answer_is_read_into_what_it_means() {
        assert_eq!(
            read_the_pick(4, r#"{"state":"picked","selector":"main > table","label":"table"}"#),
            Some(BrowserEvent::Picked { id: 4, selector: "main > table".to_owned() })
        );
        assert_eq!(
            read_the_pick(4, r#"{"state":"cancelled"}"#),
            Some(BrowserEvent::PickCancelled { id: 4 })
        );
        assert_eq!(
            read_the_pick(4, r#"{"state":"gone"}"#),
            Some(BrowserEvent::PickCancelled { id: 4 })
        );
        assert_eq!(read_the_pick(4, r#"{"state":"picking"}"#), None, "still open");
        assert_eq!(read_the_pick(4, "null"), None, "nothing to say yet");
        assert_eq!(
            read_the_pick(4, r#"{"state":"picked","selector":"  "}"#),
            Some(BrowserEvent::PickCancelled { id: 4 })
        );
        // The selector reaches the page as a JavaScript string, whatever is in it.
        assert!(pin_script("a[title=\"x\"]\n").ends_with(r#"apply("a[title=\"x\"]\n");"#));
    }

    /// `task-2203`: a tab's back and forward list comes back, under the tab's new origin.
    #[test]
    fn a_history_written_in_one_run_is_put_back_under_the_new_tab() {
        let mut tab = BrowserTab::new(
            42,
            BrowserLocation::Remote { url: "https://example.com/b".to_owned() },
        );
        tab.restore_history(
            &[
                "https://example.com/a".to_owned(),
                "https://example.com/b".to_owned(),
                "unluminous://tab-7/docs/index.html".to_owned(),
            ],
            1,
        );
        let (history, position) = tab.history();
        assert_eq!(position, 1);
        assert_eq!(history[2], "unluminous://tab-42/docs/index.html");
        assert!(tab.can_go_back() && tab.can_go_forward());
        assert_eq!(tab.step(true), Some((0, "https://example.com/a".to_owned())));
        // A position past the end is clamped, and the entry there is the address the tab was opened on.
        let mut tab =
            BrowserTab::new(1, BrowserLocation::Remote { url: "https://example.org/".to_owned() });
        tab.restore_history(&["https://example.com/a".to_owned(), "https://x.test/".to_owned()], 9);
        assert_eq!(tab.history().0, ["https://example.com/a", "https://example.org/"]);
        assert_eq!(tab.current_url(), "https://example.org/");
    }

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
        assert!(the_page_was_the_last_thing_pressed(&[], &placements, &[], true));
        assert!(!the_page_was_the_last_thing_pressed(&[], &placements, &[], false));
        assert!(
            the_page_was_the_last_thing_pressed(
                std::slice::from_ref(&release),
                &placements,
                &[],
                true
            ),
            "and a release is not a press"
        );

        assert!(the_page_was_the_last_thing_pressed(
            &[press(200.0, 200.0)],
            &placements,
            &[],
            false
        ));
        assert!(!the_page_was_the_last_thing_pressed(&[press(20.0, 20.0)], &placements, &[], true));
        // The last press in the frame is the one that decides, which is the same rule said about a
        // frame that carried two.
        assert!(the_page_was_the_last_thing_pressed(
            &[press(20.0, 20.0), press(200.0, 200.0)],
            &placements,
            &[],
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
        assert_eq!(choose(&both, &[], true, None).map(|one| one.id), Some(2), "the one on top");
        // A focused placement still wins, wherever it is in the order: that is somebody's own choice.
        let focused_behind = [placed(1, true), placed(2, false)];
        assert_eq!(choose(&focused_behind, &[], true, None).map(|one| one.id), Some(1));
        // A menu over it takes it off the screen **only** where there is no way to cut it around one,
        // which since `task-2207` is neither platform with a web view; the answer is kept for a third.
        let over = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
        assert!(choose(&both, &[over], false, None).is_none(), "nothing to cut it with");
        assert_eq!(
            choose(&both, &[over], true, None).map(|one| one.id),
            Some(2),
            "and where there is, the page stays and the child is cut around the menu"
        );
        assert!(choose(&[], &[], true, None).is_none());
    }

    /// `task-2207`: a video that is playing keeps the one view when the keyboard is elsewhere, so clicking
    /// another node or the empty canvas does not stop it. A page somebody chose still takes it.
    #[test]
    fn a_playing_video_keeps_the_view_until_another_page_is_chosen() {
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let mut video = BrowserPlacement::whole(1, area, false);
        video.playing = true;
        let page = BrowserPlacement::whole(2, area, false);
        let drawn = [video.clone(), page.clone()];
        assert_eq!(choose(&drawn, &[], true, None).map(|one| one.id), Some(1), "the playing video");
        let chosen_page = BrowserPlacement::whole(2, area, true);
        assert_eq!(choose(&[video, chosen_page], &[], true, None).map(|one| one.id), Some(2));
    }

    /// `task-2207`: what is left of a page once what is over it is taken out, as rectangles that do not
    /// overlap, which is what a macOS layer mask is made of.
    #[test]
    fn the_pieces_of_a_page_cover_what_is_not_covered_and_nothing_twice() {
        let page = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 80.0));
        assert_eq!(pieces_of(page, &[]), vec![page]);
        // Two holes that overlap each other and one wholly outside the page.
        let holes = [
            egui::Rect::from_min_max(egui::pos2(20.0, 10.0), egui::pos2(60.0, 50.0)),
            egui::Rect::from_min_max(egui::pos2(40.0, 30.0), egui::pos2(90.0, 70.0)),
            egui::Rect::from_min_max(egui::pos2(200.0, 0.0), egui::pos2(300.0, 50.0)),
        ];
        let pieces = pieces_of(page, &holes);
        let area: f32 = pieces.iter().map(|piece| piece.area()).sum();
        let covered = 40.0 * 40.0 + 50.0 * 40.0 - 20.0 * 20.0;
        assert!((area - (page.area() - covered)).abs() < 0.01, "{area} {pieces:?}");
        for (i, one) in pieces.iter().enumerate() {
            assert!(page.contains_rect(*one));
            for hole in &holes {
                assert!(!one.intersect(*hole).is_positive(), "{one:?} is under {hole:?}");
            }
            for other in &pieces[i + 1..] {
                assert!(!one.intersect(*other).is_positive(), "{one:?} overlaps {other:?}");
            }
        }
        // A page wholly covered has no pieces at all.
        assert!(pieces_of(page, &[page.expand(1.0)]).is_empty());
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
            choose(&panes, &[], true, None).map(|placement| placement.id),
            Some(2),
            "the focused pane holds the view"
        );
        assert_eq!(
            choose(&panes[..1], &[], true, None).map(|placement| placement.id),
            Some(1),
            "with no focus, the first drawn"
        );
        assert_eq!(
            choose(&panes, &[area], false, None),
            None,
            "an egui surface over the pane takes the view off the screen where it cannot be cut"
        );
        assert_eq!(choose(&[], &[], true, None), None);
    }

    /// `task-2263`: a press egui saw on a page names that page, the one on top where two overlap.
    #[test]
    fn the_page_pressed_is_the_one_under_the_press() {
        let left = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(100.0));
        let right = Rect::from_min_size(egui::pos2(120.0, 0.0), egui::Vec2::splat(100.0));
        let panes =
            [BrowserPlacement::whole(1, left, true), BrowserPlacement::whole(2, right, false)];
        let press = |at: egui::Pos2| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        assert_eq!(the_page_pressed(&[press(egui::pos2(150.0, 50.0))], &panes, &[]), Some(2));
        assert_eq!(the_page_pressed(&[press(egui::pos2(50.0, 50.0))], &panes, &[]), Some(1));
        assert_eq!(
            the_page_pressed(&[press(egui::pos2(110.0, 50.0))], &panes, &[]),
            None,
            "between them"
        );
        assert_eq!(the_page_pressed(&[], &panes, &[]), None);
    }

    /// `task-2317`: a press on a node drawn in front of a page is not a press on the page, so the page
    /// behind neither takes the keyboard nor counts as the last thing pressed, and the front node's header
    /// can be dragged.
    #[test]
    fn a_press_on_a_node_in_front_of_a_page_is_not_a_press_on_the_page() {
        let behind = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(200.0));
        let front = Rect::from_min_size(egui::pos2(100.0, 100.0), egui::Vec2::splat(200.0));
        let mut page = BrowserPlacement::whole(1, behind, false);
        page.over = vec![front];
        let press = |at: egui::Pos2| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let pages = [page];
        assert_eq!(the_page_pressed(&[press(egui::pos2(150.0, 110.0))], &pages, &[]), None);
        assert!(!the_page_was_the_last_thing_pressed(
            &[press(egui::pos2(150.0, 110.0))],
            &pages,
            &[],
            true
        ));
        assert_eq!(the_page_pressed(&[press(egui::pos2(50.0, 50.0))], &pages, &[]), Some(1));
    }

    /// `task-2317`: a press on a menu row drawn over a page is the menu's, so the page neither takes the
    /// keyboard nor makes the window close the menu before the row sees its release.
    #[test]
    fn a_press_on_a_menu_over_a_page_is_not_a_press_on_the_page() {
        let area = Rect::from_min_size(egui::pos2(0.0, 40.0), egui::Vec2::splat(400.0));
        let menu = Rect::from_min_size(egui::pos2(60.0, 20.0), egui::vec2(180.0, 140.0));
        let pages = [BrowserPlacement::whole(7, area, true)];
        let press = |at: egui::Pos2| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        };
        let on_the_row = press(egui::pos2(120.0, 130.0));
        assert_eq!(the_page_pressed(std::slice::from_ref(&on_the_row), &pages, &[menu]), None);
        assert!(!the_page_was_the_last_thing_pressed(&[on_the_row], &pages, &[menu], false));
        let beside = press(egui::pos2(300.0, 130.0));
        assert_eq!(the_page_pressed(std::slice::from_ref(&beside), &pages, &[menu]), Some(7));
    }

    /// `task-2263`: the Designer's page and a browser tab side by side, and a press on the tab's address field,
    /// which lands on neither page. The view stays on the page it shows rather than moving to the one drawn last.
    #[test]
    fn a_press_on_no_page_leaves_the_view_where_it_is() {
        let left = Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(100.0));
        let right = Rect::from_min_size(egui::pos2(120.0, 0.0), egui::Vec2::splat(100.0));
        let tab = BrowserPlacement::whole(1, left, false);
        let designer = BrowserPlacement::whole(2, right, false);
        let both = [tab.clone(), designer];
        assert_eq!(
            choose(&both, &[], true, Some(1)).map(|one| one.id),
            Some(1),
            "the tab keeps it"
        );
        assert_eq!(
            choose(&both, &[], true, Some(2)).map(|one| one.id),
            Some(2),
            "the Designer keeps it"
        );
        assert_eq!(
            choose(&both, &[], true, None).map(|one| one.id),
            Some(2),
            "nothing shown yet: the last"
        );
        // A focused page still takes the view, which is what clicking into the other page does.
        let focused = [tab, BrowserPlacement::whole(2, right, true)];
        assert_eq!(choose(&focused, &[], true, Some(1)).map(|one| one.id), Some(2));
        // Overlapping nodes on the canvas keep the rule they had: the one on top is drawn last and wins.
        let overlapping =
            [BrowserPlacement::whole(1, left, false), BrowserPlacement::whole(2, left, false)];
        assert_eq!(choose(&overlapping, &[], true, Some(1)).map(|one| one.id), Some(2));
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
